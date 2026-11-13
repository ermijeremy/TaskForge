//! Schema demonstration only; no Python, sandbox execution, or verdict classifier.
use std::process::ExitCode;

use taskforge_execution::{
    BoundedDiagnostic, EvaluationReason, EvaluationResult, EvaluationResultFields,
    InfrastructureError, InfrastructureReason, ResourceUsage, Verdict,
};

fn examples() -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let accepted = EvaluationResult::try_from(EvaluationResultFields {
        verdict: Verdict::Accepted,
        reason_code: EvaluationReason::AllTestsPassed,
        passed_tests: 8,
        total_tests: 8,
        first_failing_test: None,
        resources: ResourceUsage {
            cpu_time_ms: Some(120),
            wall_time_ms: Some(150),
            peak_memory_bytes: Some(8_388_608),
        },
        diagnostic: BoundedDiagnostic::new("All tests passed."),
    })?;
    let wrong_answer = EvaluationResult::try_from(EvaluationResultFields {
        verdict: Verdict::WrongAnswer,
        reason_code: EvaluationReason::OutputMismatch,
        passed_tests: 1,
        first_failing_test: Some(1),
        resources: ResourceUsage {
            cpu_time_ms: Some(10),
            wall_time_ms: Some(20),
            peak_memory_bytes: Some(8_388_608),
        },
        diagnostic: BoundedDiagnostic::new("Output tokens differ."),
        ..accepted.fields().clone()
    })?;
    let unavailable_measurements = EvaluationResult::try_from(EvaluationResultFields {
        verdict: Verdict::RuntimeError,
        reason_code: EvaluationReason::ProgramFailed,
        passed_tests: 0,
        first_failing_test: Some(0),
        resources: ResourceUsage::default(),
        diagnostic: BoundedDiagnostic::new(
            "Program exited unsuccessfully; measurements unavailable.",
        ),
        ..accepted.fields().clone()
    })?;
    let infrastructure = InfrastructureError::new(
        InfrastructureReason::SandboxUnavailable,
        "Isolate is unavailable; no submission was launched.",
    );
    Ok(serde_json::json!({
        "accepted": accepted,
        "wrong_answer": wrong_answer,
        "unavailable_measurements": unavailable_measurements,
        "infrastructure_failure": infrastructure,
    }))
}

fn main() -> ExitCode {
    match examples().and_then(|value| Ok(serde_json::to_string_pretty(&value)?)) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Schema demo: {error}");
            ExitCode::FAILURE
        }
    }
}
