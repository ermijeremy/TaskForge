//! Ordered evaluation with deterministic classification and validated final results.
mod classification;
mod comparison;
mod resources;
pub use classification::{ExecutionDecision, classify_execution};
pub use comparison::stdout_tokens_equal;

use crate::sandbox::{IsolateLimits, MAX_SOURCE_BYTES};
use crate::supervision::{EvaluationSession, MAX_CAPTURE_BYTES, Supervisor};
use crate::{
    BoundedDiagnostic, EvaluationOutcome, EvaluationReason, EvaluationResult,
    EvaluationResultFields, ExecutionConfig, ExecutionLimits, InfrastructureError,
    InfrastructureReason, Problem, ProblemRuntime, Runner, Verdict,
};
use resources::Resources;
use std::future::Future;

#[derive(Debug, Clone)]
pub struct IsolateRunner {
    supervisor: Supervisor,
}
impl IsolateRunner {
    /// Synchronous startup for ordinary threads. Async callers use initialize.
    pub fn new(config: ExecutionConfig) -> Result<Self, InfrastructureError> {
        block_on(Self::initialize(config))
    }
    pub async fn initialize(config: ExecutionConfig) -> Result<Self, InfrastructureError> {
        classification::ensure_ready()?;
        Ok(Self::from_supervisor(Supervisor::initialize(config).await?))
    }
    /// Reuse an already initialized supervisor. Evaluation still uses the real
    /// classifier readiness check, supported runtime, and validated per-test limits.
    pub fn from_supervisor(supervisor: Supervisor) -> Self {
        Self { supervisor }
    }
    pub async fn evaluate_async(
        &self,
        source: &str,
        problem: &Problem,
        limits: &ExecutionLimits,
    ) -> EvaluationOutcome {
        let session = self.supervisor.evaluation()?;
        validate(source, problem, limits)?;
        classification::ensure_ready()?;
        let mut usage = Resources::default();
        let total = u32::try_from(problem.test_cases().len()).map_err(|_| internal())?;
        for (position, case) in problem.test_cases().iter().enumerate() {
            session.check_work()?;
            // Only the input enters the sandbox. Expected output stays in this process.
            let execution = session
                .run_test(source, case.input.as_bytes(), limits)
                .await?;
            session.check_work()?;
            usage.record(execution.metadata.as_ref())?;
            let decision =
                classify_execution(execution.output_limit_exceeded, execution.metadata.as_ref())?;
            let evidence = evidence_note(&execution);
            if let Some((verdict, reason)) = decision.failure() {
                return finish(
                    &session,
                    usage,
                    verdict,
                    reason,
                    position as u32,
                    total,
                    Some(position as u32),
                    &format!("Execution failed: {decision:?}; {evidence}"),
                );
            }
            // Bounded CPU work can still be substantial for a 1 MiB token.
            // Keep comparison off Tokio and include queue time in the deadline.
            let expected = case.expected_output.as_bytes().to_vec();
            let output = execution.output;
            let mut compare = tokio::task::spawn_blocking(move || {
                stdout_tokens_equal(output.stdout(), &expected)
            });
            let equal = match tokio::time::timeout_at(
                tokio::time::Instant::from_std(session.deadline()),
                &mut compare,
            )
            .await
            {
                Ok(result) => result.map_err(|_| internal())?,
                Err(_) => {
                    compare.abort(); // Pending work is discarded; started work owns bounded data only.
                    return Err(failure(
                        InfrastructureReason::EvaluationDeadlineExceeded,
                        "evaluation deadline expired during output comparison",
                    ));
                }
            };
            session.check_work()?;
            if !equal {
                return finish(
                    &session,
                    usage,
                    Verdict::WrongAnswer,
                    EvaluationReason::OutputMismatch,
                    position as u32,
                    total,
                    Some(position as u32),
                    "Output byte tokens differ.",
                );
            }
        }
        finish(
            &session,
            usage,
            Verdict::Accepted,
            EvaluationReason::AllTestsPassed,
            total,
            total,
            None,
            "All tests passed.",
        )
    }
}
impl Runner for IsolateRunner {
    fn evaluate(
        &self,
        source: &str,
        problem: &Problem,
        limits: &ExecutionLimits,
    ) -> EvaluationOutcome {
        block_on(self.evaluate_async(source, problem, limits))
    }
}
fn validate(
    source: &str,
    problem: &Problem,
    limits: &ExecutionLimits,
) -> Result<(), InfrastructureError> {
    if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
        return Err(failure(
            InfrastructureReason::InvalidConfiguration,
            "source must contain 1..=65536 UTF-8 bytes",
        ));
    }
    if problem.public().runtime != ProblemRuntime::Python31410 {
        return Err(failure(
            InfrastructureReason::InvalidConfiguration,
            "problem runtime does not match the supported Python pin; choose an active problem revision",
        ));
    }
    IsolateLimits::try_from(limits).map_err(|_| {
        failure(
            InfrastructureReason::InvalidConfiguration,
            "per-test limits cannot be represented by the sandbox",
        )
    })?;
    if limits.output_bytes().get() > MAX_CAPTURE_BYTES as u64 {
        return Err(failure(
            InfrastructureReason::InvalidConfiguration,
            "supervised output budget must be at most 1 MiB",
        ));
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn finish(
    session: &EvaluationSession,
    resources: Resources,
    verdict: Verdict,
    reason_code: EvaluationReason,
    passed_tests: u32,
    total_tests: u32,
    first_failing_test: Option<u32>,
    diagnostic: &str,
) -> EvaluationOutcome {
    let result = EvaluationResult::try_from(EvaluationResultFields {
        verdict,
        reason_code,
        passed_tests,
        total_tests,
        first_failing_test,
        resources: resources.finish()?,
        diagnostic: BoundedDiagnostic::new(diagnostic),
    })
    .map_err(|_| internal())?;
    session.check_result()?;
    Ok(result)
}
fn evidence_note(execution: &crate::supervision::SupervisedTest) -> String {
    // Only typed evidence and bounded numeric fields; never raw stderr, source,
    // input, expected output or Isolate's arbitrary message text.
    match execution.metadata.as_ref() {
        Some(meta) => format!(
            "output_limit={}; oom_killed={}; status={:?}; timeout={:?}; exit_code={:?}; signal={:?}; killed={}",
            execution.output_limit_exceeded,
            meta.oom_killed,
            meta.status,
            meta.timeout_kind,
            meta.exit_code,
            meta.exit_signal,
            meta.killed
        ),
        None => format!(
            "output_limit={}; execution metadata unavailable",
            execution.output_limit_exceeded
        ),
    }
}
fn block_on<T>(
    future: impl Future<Output = Result<T, InfrastructureError>>,
) -> Result<T, InfrastructureError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(failure(
            InfrastructureReason::InvalidConfiguration,
            "use the async runner methods from a Tokio context",
        ));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|_| internal())?;
    let result = runtime.block_on(future);
    // An expired evaluation may still own a cleanup/recovery worker. Do not
    // block this response on runtime drop; that worker retains its resources.
    runtime.shutdown_background();
    result
}
fn internal() -> InfrastructureError {
    failure(
        InfrastructureReason::InternalFailure,
        "trusted evaluation state or worker failed",
    )
}
fn failure(reason: InfrastructureReason, diagnostic: &str) -> InfrastructureError {
    InfrastructureError::new(reason, diagnostic)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn synchronous_boundary_rejects_tokio_context_instead_of_panicking_or_blocking() {
        assert_eq!(
            block_on(async { Ok(()) }).unwrap_err().reason_code(),
            InfrastructureReason::InvalidConfiguration
        );
    }
    #[test]
    fn synchronous_boundary_runs_on_an_ordinary_thread() {
        assert_eq!(block_on(async { Ok(7) }).unwrap(), 7);
    }
}
