//! JSON and exit-code regression tests for the completed reporting exercise.
use judge_local::reporting::{MAX_JSON_BYTES, report_outcome};
use taskforge_execution::{
    BoundedDiagnostic, EvaluationReason as R, EvaluationResult, EvaluationResultFields,
    InfrastructureError, InfrastructureReason as I, ResourceUsage, Verdict as V,
};

fn result(verdict: V, reason: R) -> EvaluationResult {
    let accepted = verdict == V::Accepted;
    EvaluationResult::try_from(EvaluationResultFields {
        verdict,
        reason_code: reason,
        passed_tests: if accepted { 3 } else { 1 },
        total_tests: 3,
        first_failing_test: if accepted { None } else { Some(1) },
        resources: ResourceUsage {
            cpu_time_ms: Some(12),
            wall_time_ms: Some(15),
            peak_memory_bytes: Some(8192),
        },
        diagnostic: BoundedDiagnostic::new("Trusted explanation"),
    })
    .unwrap()
}
#[test]
fn accepted_result_is_preserved_as_one_compact_json_document_with_exit_zero() {
    let result = result(V::Accepted, R::AllTestsPassed);
    let expected = serde_json::to_value(&result).unwrap();
    let report = report_outcome(Ok(result)).unwrap();
    assert_eq!(report.exit_code(), 0);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(report.json_bytes()).unwrap(),
        expected
    );
    assert!(!report.json_bytes().contains(&b'\n'));
}
#[test]
fn valid_failing_verdicts_are_completed_evaluations_and_also_exit_zero() {
    for (verdict, reason) in [
        (V::WrongAnswer, R::OutputMismatch),
        (V::TimeLimitExceeded, R::CpuTimeLimitExceeded),
        (V::TimeLimitExceeded, R::WallTimeLimitExceeded),
        (V::TimeLimitExceeded, R::ReportedTimeLimitExceeded),
        (V::MemoryLimitExceeded, R::ConfirmedMemoryLimitKill),
        (V::OutputLimitExceeded, R::CombinedOutputLimitExceeded),
        (V::RuntimeError, R::ProgramFailed),
        (V::RuntimeError, R::PythonSyntaxError),
    ] {
        let result = result(verdict, reason);
        let expected = serde_json::to_value(&result).unwrap();
        let report = report_outcome(Ok(result)).unwrap();
        assert_eq!(report.exit_code(), 0, "{verdict:?}");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(report.json_bytes()).unwrap(),
            expected
        );
    }
}
#[test]
fn infrastructure_errors_exit_three_and_do_not_invent_a_verdict_or_progress() {
    for reason in [
        I::InvalidConfiguration,
        I::SandboxUnavailable,
        I::MetadataInvalid,
        I::EvaluationDeadlineExceeded,
        I::Cancelled,
        I::CleanupFailed,
    ] {
        let error = InfrastructureError::new(reason, "Trusted failure explanation");
        let expected = serde_json::to_value(&error).unwrap();
        let report = report_outcome(Err(error)).unwrap();
        assert_eq!(report.exit_code(), 3);
        let actual: serde_json::Value = serde_json::from_slice(report.json_bytes()).unwrap();
        assert_eq!(actual, expected);
        assert!(actual.get("verdict").is_none());
        assert!(actual.get("passed_tests").is_none());
    }
}
#[test]
fn unknown_measurements_and_measured_zero_remain_distinct() {
    let mut fields = result(V::Accepted, R::AllTestsPassed).fields().clone();
    fields.resources = ResourceUsage {
        cpu_time_ms: Some(0),
        wall_time_ms: None,
        peak_memory_bytes: None,
    };
    let report = report_outcome(Ok(EvaluationResult::try_from(fields).unwrap())).unwrap();
    let value: serde_json::Value = serde_json::from_slice(report.json_bytes()).unwrap();
    assert_eq!(value["resources"]["cpu_time_ms"], 0);
    assert!(value["resources"]["wall_time_ms"].is_null());
    assert!(value["resources"]["peak_memory_bytes"].is_null());
}
#[test]
fn diagnostic_escaping_keeps_valid_json_within_the_report_bound() {
    let diagnostic = "\0".repeat(taskforge_execution::MAX_DIAGNOSTIC_BYTES);
    let error = InfrastructureError::new(I::InternalFailure, &diagnostic);
    let report = report_outcome(Err(error)).unwrap();
    assert!(report.json_bytes().len() > diagnostic.len());
    assert!(report.json_bytes().len() <= MAX_JSON_BYTES);
    assert!(!report.json_bytes().contains(&b'\n'));
    let actual: serde_json::Value = serde_json::from_slice(report.json_bytes()).unwrap();
    assert_eq!(actual["diagnostic"], diagnostic);
}
#[test]
fn maximum_integer_values_are_not_rounded_or_truncated() {
    let mut fields = result(V::Accepted, R::AllTestsPassed).fields().clone();
    fields.total_tests = u32::MAX;
    fields.passed_tests = u32::MAX;
    fields.resources.cpu_time_ms = Some(u64::MAX);
    let report = report_outcome(Ok(EvaluationResult::try_from(fields).unwrap())).unwrap();
    let value: serde_json::Value = serde_json::from_slice(report.json_bytes()).unwrap();
    assert_eq!(value["resources"]["cpu_time_ms"].as_u64(), Some(u64::MAX));
    assert_eq!(value["total_tests"].as_u64(), Some(u32::MAX as u64));
}
