//! Regression tests for the completed core classification exercise.
use taskforge_execution::InfrastructureReason as I;
use taskforge_execution::runner::{ExecutionDecision as D, classify_execution};
use taskforge_execution::sandbox::{SandboxMetadata, TimeoutKind};

fn metadata(text: &str) -> SandboxMetadata {
    SandboxMetadata::parse(text.as_bytes()).unwrap()
}
fn decision(text: &str) -> D {
    classify_execution(false, Some(&metadata(text))).unwrap()
}
#[test]
fn clean_success_permits_output_comparison_even_with_unknown_measurements() {
    assert_eq!(decision("exitcode:0\n"), D::CompareOutput);
    assert_eq!(
        decision("exitcode:0\ntime:999\ncg-mem:999999999\n"),
        D::CompareOutput
    );
}
#[test]
fn supervisor_output_limit_precedes_memory_timeout_and_signal_and_needs_no_metadata() {
    assert_eq!(classify_execution(true, None).unwrap(), D::OutputLimit);
    let all = metadata(
        "status:TO\nexitsig:9\ncg-oom-killed:1\nkilled:1\nmessage:Time limit exceeded (wall clock)\n",
    );
    assert_eq!(
        classify_execution(true, Some(&all)).unwrap(),
        D::OutputLimit
    );
}
#[test]
fn confirmed_oom_precedes_timeout_and_other_program_failures() {
    assert_eq!(
        decision("status:TO\nexitsig:9\ncg-oom-killed:1\nmessage:Time limit exceeded\n"),
        D::MemoryLimit
    );
    assert_eq!(
        decision("status:SG\nexitsig:9\ncg-oom-killed:1\n"),
        D::MemoryLimit
    );
}
#[test]
fn timeout_uses_the_reported_clock_without_inventing_missing_measurements() {
    assert_eq!(
        decision("status:TO\nmessage:Time limit exceeded\n"),
        D::CpuTimeLimit
    );
    assert_eq!(
        decision("status:TO\nmessage:Time limit exceeded (wall clock)\n"),
        D::WallTimeLimit
    );
    assert_eq!(decision("status:TO\n"), D::UnspecifiedTimeLimit);
    assert_eq!(
        decision("status:TO\nmessage:unknown future message\n"),
        D::UnspecifiedTimeLimit
    );
}
#[test]
fn signal_or_memory_usage_alone_never_implies_a_memory_limit_kill() {
    assert_eq!(
        decision("status:SG\nexitsig:9\ncg-mem:999999999\nkilled:1\n"),
        D::RuntimeError
    );
    assert_eq!(decision("exitsig:11\n"), D::RuntimeError);
}
#[test]
fn reported_failures_and_unexplained_kills_never_compare_output() {
    for text in [
        "exitcode:1\n",
        "status:RE\nexitcode:1\n",
        "status:RE\nexitcode:0\n",
        "exitcode:0\nkilled:1\n",
    ] {
        assert_eq!(decision(text), D::RuntimeError);
    }
}
#[test]
fn internal_sandbox_errors_are_infrastructure_failures_even_with_output_evidence() {
    let meta = metadata("status:XX\n");
    for excess in [false, true] {
        assert_eq!(
            classify_execution(excess, Some(&meta))
                .unwrap_err()
                .reason_code(),
            I::SandboxSetupFailed
        );
    }
}
#[test]
fn missing_metadata_without_a_supervisor_output_kill_is_an_infrastructure_error() {
    assert_eq!(
        classify_execution(false, None).unwrap_err().reason_code(),
        I::MetadataMissing
    );
}
#[test]
fn inconsistent_trusted_evidence_is_rejected_instead_of_guessed() {
    let mut both_exits = metadata("exitcode:0\n");
    both_exits.exit_signal = Some(9);
    let mut no_outcome = metadata("exitcode:0\n");
    no_outcome.exit_code = None;
    let mut wrong_clock = metadata("exitcode:0\n");
    wrong_clock.timeout_kind = Some(TimeoutKind::Wall);
    for meta in [both_exits, no_outcome, wrong_clock] {
        for excess in [false, true] {
            assert_eq!(
                classify_execution(excess, Some(&meta))
                    .unwrap_err()
                    .reason_code(),
                I::MetadataInvalid
            );
        }
    }
}
