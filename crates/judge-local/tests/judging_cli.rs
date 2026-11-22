use std::path::PathBuf;
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_judge-local"))
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn judging_help_lists_required_arguments_and_only_active_problem_ids() {
    let out = cli().args(["judge", "--help"]).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    for flag in [
        "--problem",
        "--source",
        "--config",
        "--problems-dir",
        "sum-two-integers-v2",
    ] {
        assert!(text.contains(flag));
    }
    assert!(!text.contains("sum-two-integers-v1"));
}
#[test]
fn unknown_problem_is_a_usage_error_before_loading_or_launching_anything() {
    let out = cli()
        .args([
            "judge",
            "--problem",
            "../../private/secret",
            "--source",
            "missing.py",
            "--config",
            "missing.json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid value"));
}
#[test]
fn judging_configuration_errors_emit_json_before_startup() {
    let out = cli()
        .args(["judge", "--problem", "sum-two-integers-v2", "--source"])
        .arg(root().join("fixtures/submissions/sum_two_accepted.py"))
        .args(["--config", "missing.json", "--problems-dir"])
        .arg(root().join("fixtures/problems"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let error: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["reason_code"], "invalid_configuration");
    assert!(out.stderr.is_empty());
}

#[test]
fn readiness_json_reports_invalid_configuration_without_judging_or_exercise_dependency() {
    let out = cli()
        .args(["check-ready", "--json", "--config", "does-not-exist.json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let error: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["reason_code"], "invalid_configuration");
    assert!(error.get("verdict").is_none());
    assert!(out.stderr.is_empty());
}
#[test]
fn judging_input_errors_emit_only_json_and_the_infrastructure_exit_code() {
    let out = cli()
        .args([
            "judge",
            "--problem",
            "sum-two-integers-v2",
            "--source",
            "missing.py",
            "--config",
            "missing.json",
            "--problems-dir",
        ])
        .arg(root().join("fixtures/problems"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stderr.is_empty());
    let error: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["reason_code"], "invalid_configuration");
    assert!(error.get("verdict").is_none());
}
