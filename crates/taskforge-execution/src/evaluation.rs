//! Library contract for the future runner. No submission is executed here.

use std::fmt;

use serde::Serialize;
use thiserror::Error;

use crate::{ExecutionLimits, Problem};

/// Maximum retained diagnostic length in UTF-8 bytes, before JSON escaping.
pub const MAX_DIAGNOSTIC_BYTES: usize = 4096;

/// Trusted judge explanation only. Never put private tests, expected output,
/// host credentials, or unfiltered submission stdout/stderr in this value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedDiagnostic(String);

impl BoundedDiagnostic {
    pub fn new(message: &str) -> Self {
        let mut end = message.len().min(MAX_DIAGNOSTIC_BYTES);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        Self(message[..end].to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BoundedDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Final user-code outcomes. Infrastructure failure is a separate error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    TimeLimitExceeded,
    MemoryLimitExceeded,
    OutputLimitExceeded,
    RuntimeError,
}

/// Stable machine-readable explanation. Mapping/validation does not classify
/// sandbox metadata or decide precedence; those are issue #8 responsibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationReason {
    AllTestsPassed,
    OutputMismatch,
    CpuTimeLimitExceeded,
    WallTimeLimitExceeded,
    ConfirmedMemoryLimitKill,
    CombinedOutputLimitExceeded,
    PythonSyntaxError,
    ProgramFailed,
}

/// Measurements across attempted tests, including the first failing test.
/// CPU/wall are sums of test execution; memory is the maximum per-test peak.
/// Setup/cleanup are excluded. If any required measurement is unavailable, its
/// aggregate is None (JSON null). A measured zero remains Some(0).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ResourceUsage {
    pub cpu_time_ms: Option<u64>,
    pub wall_time_ms: Option<u64>,
    pub peak_memory_bytes: Option<u64>,
}

/// Candidate result assembled by trusted evaluation code. Constructing these
/// fields does not validate them. Return only EvaluationResult from a Runner.
#[derive(Debug, Clone, Serialize)]
pub struct EvaluationResultFields {
    pub verdict: Verdict,
    pub reason_code: EvaluationReason,
    pub passed_tests: u32,
    pub total_tests: u32,
    /// Zero-based position in the problem's stored case order.
    pub first_failing_test: Option<u32>,
    pub resources: ResourceUsage,
    pub diagnostic: BoundedDiagnostic,
}

/// Final validated result. No Deserialize or public unchecked constructor:
/// callers must use TryFrom<EvaluationResultFields> to establish invariants.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct EvaluationResult(EvaluationResultFields);

impl EvaluationResult {
    pub fn fields(&self) -> &EvaluationResultFields {
        &self.0
    }
}

impl TryFrom<EvaluationResultFields> for EvaluationResult {
    type Error = ResultValidationError;

    fn try_from(fields: EvaluationResultFields) -> Result<Self, Self::Error> {
        if fields.total_tests == 0 {
            return Err(ResultValidationError::EmptyTestSuite);
        }

        if fields.passed_tests > fields.total_tests {
            return Err(ResultValidationError::PassedExceedsTotal);
        }

        let compatible = match fields.reason_code {
            EvaluationReason::AllTestsPassed => fields.verdict == Verdict::Accepted,
            EvaluationReason::OutputMismatch => fields.verdict == Verdict::WrongAnswer,
            EvaluationReason::CpuTimeLimitExceeded => fields.verdict == Verdict::TimeLimitExceeded,
            EvaluationReason::WallTimeLimitExceeded => fields.verdict == Verdict::TimeLimitExceeded,
            EvaluationReason::ConfirmedMemoryLimitKill => {
                fields.verdict == Verdict::MemoryLimitExceeded
            }
            EvaluationReason::CombinedOutputLimitExceeded => {
                fields.verdict == Verdict::OutputLimitExceeded
            }
            EvaluationReason::ProgramFailed => fields.verdict == Verdict::RuntimeError,
            EvaluationReason::PythonSyntaxError => fields.verdict == Verdict::RuntimeError,
        };

        if !compatible {
            return Err(ResultValidationError::ReasonVerdictMismatch);
        }

        if fields.verdict == Verdict::Accepted {
            if fields.passed_tests != fields.total_tests || fields.first_failing_test.is_some() {
                return Err(ResultValidationError::InvalidAcceptedProgress);
            }
        } else {
            if fields.passed_tests >= fields.total_tests
                || fields.first_failing_test != Some(fields.passed_tests)
            {
                return Err(ResultValidationError::InvalidFailureProgress);
            }
        }

        Ok(Self(fields))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ResultValidationError {
    #[error("total_tests must be greater than zero")]
    EmptyTestSuite,
    #[error("passed_tests cannot exceed total_tests")]
    PassedExceedsTotal,
    #[error("reason_code is incompatible with the verdict")]
    ReasonVerdictMismatch,
    #[error("Accepted requires all tests passed and no failing index")]
    InvalidAcceptedProgress,
    #[error(
        "a failing verdict requires passed_tests < total_tests and first_failing_test = passed_tests"
    )]
    InvalidFailureProgress,
}

/// Infrastructure failures remain distinct from valid failing submissions.
/// A future worker decides whether/when to retry; the contract does not do so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureReason {
    InvalidConfiguration,
    SandboxUnavailable,
    SandboxSetupFailed,
    MetadataMissing,
    MetadataInvalid,
    ProcessIoFailed,
    EvaluationDeadlineExceeded,
    Cancelled,
    CleanupFailed,
    InternalFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
#[error("infrastructure failure ({reason_code:?}): {diagnostic}")]
pub struct InfrastructureError {
    reason_code: InfrastructureReason,
    diagnostic: BoundedDiagnostic,
}

impl InfrastructureError {
    pub fn new(reason_code: InfrastructureReason, diagnostic: &str) -> Self {
        Self {
            reason_code,
            diagnostic: BoundedDiagnostic::new(diagnostic),
        }
    }

    pub fn reason_code(&self) -> InfrastructureReason {
        self.reason_code
    }

    pub fn diagnostic(&self) -> &BoundedDiagnostic {
        &self.diagnostic
    }
}

pub type EvaluationOutcome = Result<EvaluationResult, InfrastructureError>;

/// Contract only: concrete Isolate execution and evaluation arrive in #6–#8.
/// Implementations own host configuration and the infrastructure deadline.
/// This synchronous boundary can run on a future worker's blocking thread.
/// No CLI invocation, HTTP, SQL, retry policy, or host-process fallback is needed.
pub trait Runner {
    fn evaluate(
        &self,
        source: &str,
        problem: &Problem,
        limits: &ExecutionLimits,
    ) -> EvaluationOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    // Schema fixtures use the same validated construction gate as callers.
    fn schema_fixture(
        verdict: Verdict,
        reason_code: EvaluationReason,
        passed_tests: u32,
        first_failing_test: Option<u32>,
        resources: ResourceUsage,
        diagnostic: &str,
    ) -> EvaluationResult {
        EvaluationResult::try_from(EvaluationResultFields {
            verdict,
            reason_code,
            passed_tests,
            total_tests: 8,
            first_failing_test,
            resources,
            diagnostic: BoundedDiagnostic::new(diagnostic),
        })
        .expect("schema fixture must satisfy the evaluation contract")
    }

    #[test]
    fn diagnostics_preserve_valid_utf8_and_bound_allocated_text() {
        assert_eq!(BoundedDiagnostic::new("").as_str(), "");
        let exact = "x".repeat(MAX_DIAGNOSTIC_BYTES);
        assert_eq!(BoundedDiagnostic::new(&exact).as_str(), exact);
        let long = "x".repeat(MAX_DIAGNOSTIC_BYTES + 1000);
        assert_eq!(
            BoundedDiagnostic::new(&long).as_str().len(),
            MAX_DIAGNOSTIC_BYTES
        );
        let crossing = format!("{}🦀tail", "a".repeat(MAX_DIAGNOSTIC_BYTES - 1));
        let bounded = BoundedDiagnostic::new(&crossing);
        assert_eq!(bounded.as_str().len(), MAX_DIAGNOSTIC_BYTES - 1);
        assert!(crossing.starts_with(bounded.as_str()));
        let all_multibyte = BoundedDiagnostic::new(&"界".repeat(MAX_DIAGNOSTIC_BYTES));
        assert!(all_multibyte.as_str().len() <= MAX_DIAGNOSTIC_BYTES);
        assert_eq!(format!("{all_multibyte}"), all_multibyte.as_str());
    }

    #[test]
    fn unavailable_measurements_are_null_while_measured_zero_is_zero() {
        assert_eq!(
            serde_json::to_value(ResourceUsage::default()).unwrap(),
            json!({
                "cpu_time_ms": null, "wall_time_ms": null, "peak_memory_bytes": null
            })
        );
        let usage = ResourceUsage {
            cpu_time_ms: Some(0),
            wall_time_ms: Some(1),
            peak_memory_bytes: None,
        };
        assert_eq!(
            serde_json::to_value(usage).unwrap(),
            json!({
                "cpu_time_ms": 0, "wall_time_ms": 1, "peak_memory_bytes": null
            })
        );
    }

    #[test]
    fn accepted_failure_unknown_and_infrastructure_examples_match_wire_types() {
        let accepted = schema_fixture(
            Verdict::Accepted,
            EvaluationReason::AllTestsPassed,
            8,
            None,
            ResourceUsage {
                cpu_time_ms: Some(120),
                wall_time_ms: Some(150),
                peak_memory_bytes: Some(8_388_608),
            },
            "All tests passed.",
        );
        let wrong = schema_fixture(
            Verdict::WrongAnswer,
            EvaluationReason::OutputMismatch,
            1,
            Some(1),
            ResourceUsage {
                cpu_time_ms: Some(10),
                wall_time_ms: Some(20),
                peak_memory_bytes: Some(8_388_608),
            },
            "Output tokens differ.",
        );
        let unknown = schema_fixture(
            Verdict::RuntimeError,
            EvaluationReason::ProgramFailed,
            0,
            Some(0),
            ResourceUsage::default(),
            "Program exited unsuccessfully; measurements unavailable.",
        );
        for (result, contents) in [
            (
                accepted,
                include_str!("../../../examples/results/accepted.json"),
            ),
            (
                wrong,
                include_str!("../../../examples/results/wrong-answer.json"),
            ),
            (
                unknown,
                include_str!("../../../examples/results/unavailable-measurements.json"),
            ),
        ] {
            let expected: Value = serde_json::from_str(contents).unwrap();
            assert_eq!(serde_json::to_value(result).unwrap(), expected);
        }
        let infrastructure = InfrastructureError::new(
            InfrastructureReason::SandboxUnavailable,
            "Isolate is unavailable; no submission was launched.",
        );
        let expected: Value = serde_json::from_str(include_str!(
            "../../../examples/results/infrastructure-failure.json"
        ))
        .unwrap();
        assert_eq!(serde_json::to_value(infrastructure).unwrap(), expected);
    }

    #[test]
    fn infrastructure_errors_have_bounded_diagnostics_without_a_user_verdict() {
        let error =
            InfrastructureError::new(InfrastructureReason::CleanupFailed, &"x".repeat(10_000));
        assert_eq!(error.reason_code(), InfrastructureReason::CleanupFailed);
        assert_eq!(error.diagnostic().as_str().len(), MAX_DIAGNOSTIC_BYTES);
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["reason_code"], "cleanup_failed");
        assert!(json.get("verdict").is_none());
        assert!(error.to_string().contains("CleanupFailed"));
    }

    #[test]
    fn all_verdicts_and_infrastructure_reasons_have_stable_serialized_codes() {
        for (verdict, code) in [
            (Verdict::Accepted, "accepted"),
            (Verdict::WrongAnswer, "wrong_answer"),
            (Verdict::TimeLimitExceeded, "time_limit_exceeded"),
            (Verdict::MemoryLimitExceeded, "memory_limit_exceeded"),
            (Verdict::OutputLimitExceeded, "output_limit_exceeded"),
            (Verdict::RuntimeError, "runtime_error"),
        ] {
            assert_eq!(serde_json::to_value(verdict).unwrap(), code);
        }
        for (reason, code) in [
            (
                InfrastructureReason::InvalidConfiguration,
                "invalid_configuration",
            ),
            (
                InfrastructureReason::SandboxUnavailable,
                "sandbox_unavailable",
            ),
            (
                InfrastructureReason::SandboxSetupFailed,
                "sandbox_setup_failed",
            ),
            (InfrastructureReason::MetadataMissing, "metadata_missing"),
            (InfrastructureReason::MetadataInvalid, "metadata_invalid"),
            (InfrastructureReason::ProcessIoFailed, "process_io_failed"),
            (
                InfrastructureReason::EvaluationDeadlineExceeded,
                "evaluation_deadline_exceeded",
            ),
            (InfrastructureReason::Cancelled, "cancelled"),
            (InfrastructureReason::CleanupFailed, "cleanup_failed"),
            (InfrastructureReason::InternalFailure, "internal_failure"),
        ] {
            assert_eq!(serde_json::to_value(reason).unwrap(), code);
        }
    }
}
