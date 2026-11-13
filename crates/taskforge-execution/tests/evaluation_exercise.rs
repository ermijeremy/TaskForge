//! Regression tests for the completed issue #5 result-validation exercise.

use taskforge_execution::{
    BoundedDiagnostic, EvaluationReason, EvaluationResult, EvaluationResultFields, ResourceUsage,
    ResultValidationError, Verdict,
};

fn accepted() -> EvaluationResultFields {
    EvaluationResultFields {
        verdict: Verdict::Accepted,
        reason_code: EvaluationReason::AllTestsPassed,
        passed_tests: 8,
        total_tests: 8,
        first_failing_test: None,
        resources: ResourceUsage::default(),
        diagnostic: BoundedDiagnostic::new("All tests passed."),
    }
}

fn failed() -> EvaluationResultFields {
    EvaluationResultFields {
        verdict: Verdict::WrongAnswer,
        reason_code: EvaluationReason::OutputMismatch,
        passed_tests: 1,
        first_failing_test: Some(1),
        diagnostic: BoundedDiagnostic::new("Output tokens differ."),
        ..accepted()
    }
}

#[test]
fn valid_accepted_result_preserves_fields_and_serializes_null_measurements() {
    let result = EvaluationResult::try_from(accepted()).unwrap();
    assert_eq!(result.fields().passed_tests, 8);
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["verdict"], "accepted");
    assert_eq!(value["reason_code"], "all_tests_passed");
    assert!(value["first_failing_test"].is_null());
    assert!(value["resources"]["cpu_time_ms"].is_null());
    assert!(value["resources"]["wall_time_ms"].is_null());
    assert!(value["resources"]["peak_memory_bytes"].is_null());
}

#[test]
fn all_allowed_verdict_reason_pairs_construct_valid_results() {
    for (verdict, reason) in [
        (Verdict::WrongAnswer, EvaluationReason::OutputMismatch),
        (
            Verdict::TimeLimitExceeded,
            EvaluationReason::CpuTimeLimitExceeded,
        ),
        (
            Verdict::TimeLimitExceeded,
            EvaluationReason::WallTimeLimitExceeded,
        ),
        (
            Verdict::MemoryLimitExceeded,
            EvaluationReason::ConfirmedMemoryLimitKill,
        ),
        (
            Verdict::OutputLimitExceeded,
            EvaluationReason::CombinedOutputLimitExceeded,
        ),
        (Verdict::RuntimeError, EvaluationReason::PythonSyntaxError),
        (Verdict::RuntimeError, EvaluationReason::ProgramFailed),
    ] {
        let mut fields = failed();
        fields.verdict = verdict;
        fields.reason_code = reason;
        let result = EvaluationResult::try_from(fields).unwrap();
        assert_eq!(result.fields().verdict, verdict);
        assert_eq!(result.fields().reason_code, reason);
        assert_eq!(result.fields().first_failing_test, Some(1));
    }
}

#[test]
fn empty_suites_and_impossible_pass_counts_have_distinct_errors() {
    let mut fields = accepted();
    fields.total_tests = 0;
    fields.passed_tests = 0;
    assert_eq!(
        EvaluationResult::try_from(fields).unwrap_err(),
        ResultValidationError::EmptyTestSuite
    );
    let mut fields = accepted();
    fields.total_tests = 0;
    assert_eq!(
        EvaluationResult::try_from(fields).unwrap_err(),
        ResultValidationError::EmptyTestSuite
    );
    let mut fields = accepted();
    fields.passed_tests = 9;
    assert_eq!(
        EvaluationResult::try_from(fields).unwrap_err(),
        ResultValidationError::PassedExceedsTotal
    );
}

#[test]
fn every_incompatible_verdict_reason_pair_is_rejected() {
    for verdict in [
        Verdict::Accepted,
        Verdict::WrongAnswer,
        Verdict::TimeLimitExceeded,
        Verdict::MemoryLimitExceeded,
        Verdict::OutputLimitExceeded,
        Verdict::RuntimeError,
    ] {
        for (reason, allowed_verdict) in [
            (EvaluationReason::AllTestsPassed, Verdict::Accepted),
            (EvaluationReason::OutputMismatch, Verdict::WrongAnswer),
            (
                EvaluationReason::CpuTimeLimitExceeded,
                Verdict::TimeLimitExceeded,
            ),
            (
                EvaluationReason::WallTimeLimitExceeded,
                Verdict::TimeLimitExceeded,
            ),
            (
                EvaluationReason::ConfirmedMemoryLimitKill,
                Verdict::MemoryLimitExceeded,
            ),
            (
                EvaluationReason::CombinedOutputLimitExceeded,
                Verdict::OutputLimitExceeded,
            ),
            (EvaluationReason::PythonSyntaxError, Verdict::RuntimeError),
            (EvaluationReason::ProgramFailed, Verdict::RuntimeError),
        ] {
            if verdict == allowed_verdict {
                continue;
            }
            let mut fields = if verdict == Verdict::Accepted {
                accepted()
            } else {
                failed()
            };
            fields.verdict = verdict;
            fields.reason_code = reason;
            assert_eq!(
                EvaluationResult::try_from(fields).unwrap_err(),
                ResultValidationError::ReasonVerdictMismatch,
                "{verdict:?} / {reason:?}"
            );
        }
    }
}

#[test]
fn accepted_requires_every_test_passed_and_no_failing_index() {
    for (passed, failing) in [(7, None), (8, Some(0)), (8, Some(8))] {
        let mut fields = accepted();
        fields.passed_tests = passed;
        fields.first_failing_test = failing;
        assert_eq!(
            EvaluationResult::try_from(fields).unwrap_err(),
            ResultValidationError::InvalidAcceptedProgress
        );
    }
}

#[test]
fn failing_index_must_be_the_next_stored_test_after_passed_tests() {
    for (passed, failing) in [
        (8, Some(8)),
        (0, None),
        (1, Some(2)),
        (1, Some(0)),
        (0, Some(8)),
    ] {
        let mut fields = failed();
        fields.passed_tests = passed;
        fields.first_failing_test = failing;
        assert_eq!(
            EvaluationResult::try_from(fields).unwrap_err(),
            ResultValidationError::InvalidFailureProgress
        );
    }
    let mut first_failure = failed();
    first_failure.passed_tests = 0;
    first_failure.first_failing_test = Some(0);
    assert!(EvaluationResult::try_from(first_failure).is_ok());
}

#[test]
fn maximum_counts_do_not_overflow_and_measured_zero_is_preserved() {
    let mut fields = accepted();
    fields.total_tests = u32::MAX;
    fields.passed_tests = u32::MAX;
    fields.resources = ResourceUsage {
        cpu_time_ms: Some(0),
        wall_time_ms: Some(0),
        peak_memory_bytes: Some(0),
    };
    let result = EvaluationResult::try_from(fields).unwrap();
    assert_eq!(result.fields().resources.cpu_time_ms, Some(0));
    let mut fields = failed();
    fields.total_tests = u32::MAX;
    fields.passed_tests = u32::MAX - 1;
    fields.first_failing_test = Some(u32::MAX - 1);
    assert!(EvaluationResult::try_from(fields).is_ok());
}

#[test]
fn constructor_keeps_diagnostics_and_measurements_without_repairing_fields() {
    let mut fields = failed();
    fields.diagnostic = BoundedDiagnostic::new("safe trusted reason");
    fields.resources = ResourceUsage {
        cpu_time_ms: Some(12),
        wall_time_ms: None,
        peak_memory_bytes: Some(8192),
    };
    let result = EvaluationResult::try_from(fields).unwrap();
    assert_eq!(result.fields().diagnostic.as_str(), "safe trusted reason");
    assert_eq!(result.fields().resources.cpu_time_ms, Some(12));
    assert_eq!(result.fields().resources.wall_time_ms, None);
    assert_eq!(result.fields().resources.peak_memory_bytes, Some(8192));
}

#[test]
fn no_failing_verdict_can_claim_all_tests_passed_without_a_failure() {
    for (verdict, reason_code) in [
        (Verdict::WrongAnswer, EvaluationReason::OutputMismatch),
        (
            Verdict::TimeLimitExceeded,
            EvaluationReason::CpuTimeLimitExceeded,
        ),
        (
            Verdict::TimeLimitExceeded,
            EvaluationReason::WallTimeLimitExceeded,
        ),
        (
            Verdict::MemoryLimitExceeded,
            EvaluationReason::ConfirmedMemoryLimitKill,
        ),
        (
            Verdict::OutputLimitExceeded,
            EvaluationReason::CombinedOutputLimitExceeded,
        ),
        (Verdict::RuntimeError, EvaluationReason::PythonSyntaxError),
        (Verdict::RuntimeError, EvaluationReason::ProgramFailed),
    ] {
        let fields = EvaluationResultFields {
            verdict,
            reason_code,
            ..accepted()
        };
        assert_eq!(
            EvaluationResult::try_from(fields).unwrap_err(),
            ResultValidationError::InvalidFailureProgress,
            "{verdict:?} / {reason_code:?}"
        );
    }
}
