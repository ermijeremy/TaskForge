use crate::sandbox::{SandboxMetadata, SandboxStatus, TimeoutKind};
use crate::{EvaluationReason, InfrastructureError, InfrastructureReason, Verdict};

/// Execution-only decision. Only CompareOutput permits stdout comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDecision {
    CompareOutput,
    OutputLimit,
    MemoryLimit,
    CpuTimeLimit,
    WallTimeLimit,
    UnspecifiedTimeLimit,
    RuntimeError,
}
impl ExecutionDecision {
    pub(super) fn failure(self) -> Option<(Verdict, EvaluationReason)> {
        Some(match self {
            Self::CompareOutput => return None,
            Self::OutputLimit => (
                Verdict::OutputLimitExceeded,
                EvaluationReason::CombinedOutputLimitExceeded,
            ),
            Self::MemoryLimit => (
                Verdict::MemoryLimitExceeded,
                EvaluationReason::ConfirmedMemoryLimitKill,
            ),
            Self::CpuTimeLimit => (
                Verdict::TimeLimitExceeded,
                EvaluationReason::CpuTimeLimitExceeded,
            ),
            Self::WallTimeLimit => (
                Verdict::TimeLimitExceeded,
                EvaluationReason::WallTimeLimitExceeded,
            ),
            Self::UnspecifiedTimeLimit => (
                Verdict::TimeLimitExceeded,
                EvaluationReason::ReportedTimeLimitExceeded,
            ),
            Self::RuntimeError => (Verdict::RuntimeError, EvaluationReason::ProgramFailed),
        })
    }
}

/// Classify trusted execution evidence before any answer comparison.
/// Infrastructure errors remain errors; user-code outcomes use the documented
/// output > confirmed OOM > reported timeout > runtime failure precedence.
pub fn classify_execution(
    output_limit_exceeded: bool,
    metadata: Option<&SandboxMetadata>,
) -> Result<ExecutionDecision, InfrastructureError> {
    if let Some(meta) = metadata {
        if meta.status == Some(SandboxStatus::InternalError) {
            return Err(InfrastructureError::new(
                InfrastructureReason::SandboxSetupFailed,
                "Sandbox reported an internal error",
            ));
        }
        if meta.exit_code.is_some() && meta.exit_signal.is_some() {
            return Err(InfrastructureError::new(
                InfrastructureReason::MetadataInvalid,
                "Metadata reports both an exit code and an exit signal",
            ));
        }
        if meta.status.is_none() && meta.exit_code.is_none() && meta.exit_signal.is_none() {
            return Err(InfrastructureError::new(
                InfrastructureReason::MetadataInvalid,
                "Metadata contains no execution outcome",
            ));
        }
        if meta.timeout_kind.is_some() && meta.status != Some(SandboxStatus::Timeout) {
            return Err(InfrastructureError::new(
                InfrastructureReason::MetadataInvalid,
                "Metadata reports a timeout clock without timeout status",
            ));
        }
    }

    // Supervisor kills can prevent metadata from being produced. This decision
    // must precede the missing-metadata error and other user-code outcomes.
    if output_limit_exceeded {
        return Ok(ExecutionDecision::OutputLimit);
    }
    let meta = metadata.ok_or_else(|| {
        InfrastructureError::new(
            InfrastructureReason::MetadataMissing,
            "Execution metadata is missing without a supervisor output-limit kill",
        )
    })?;
    if meta.oom_killed {
        return Ok(ExecutionDecision::MemoryLimit);
    }
    if meta.status == Some(SandboxStatus::Timeout) {
        return Ok(match meta.timeout_kind {
            Some(TimeoutKind::Cpu) => ExecutionDecision::CpuTimeLimit,
            Some(TimeoutKind::Wall) => ExecutionDecision::WallTimeLimit,
            None => ExecutionDecision::UnspecifiedTimeLimit,
        });
    }
    if matches!(
        meta.status,
        Some(SandboxStatus::RuntimeError | SandboxStatus::Signal)
    ) || meta.exit_signal.is_some()
        || meta.exit_code.is_some_and(|code| code != 0)
        || meta.killed
    {
        return Ok(ExecutionDecision::RuntimeError);
    }
    if meta.exit_code == Some(0) {
        return Ok(ExecutionDecision::CompareOutput);
    }
    Err(InfrastructureError::new(
        InfrastructureReason::MetadataInvalid,
        "Metadata does not identify a completed program outcome",
    ))
}

pub(super) fn ensure_ready() -> Result<(), InfrastructureError> {
    let success = SandboxMetadata::parse(b"exitcode:0\n")?;
    if classify_execution(false, Some(&success))? == ExecutionDecision::CompareOutput {
        Ok(())
    } else {
        Err(InfrastructureError::new(
            InfrastructureReason::InternalFailure,
            "Execution classifier rejected the clean-success readiness probe",
        ))
    }
}
