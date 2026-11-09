#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use serde_json::json;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_judge-local"))
}

#[test]
fn help_describes_the_configuration_scaffold() {
    let output = cli().arg("--help").output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("check-config"));
}

#[test]
fn check_config_succeeds_without_launching_the_configured_files() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("static-check-only");
    fs::write(
        &executable,
        "#!/bin/sh\necho UNEXPECTED_EXECUTION\nexit 99\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let config = directory.path().join("judge.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"runtime": {
            "python_path": "static-check-only", "isolate_path": "static-check-only"
        }}))
        .unwrap(),
    )
    .unwrap();

    let output = cli()
        .args(["check-config", "--config"])
        .arg(config)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("static checks only"));
    assert!(stdout.contains("CPU per test: 1000 ms"));
    assert!(stdout.contains("Whole evaluation deadline: 60000 ms"));
    assert!(!stdout.contains("UNEXPECTED_EXECUTION"));
}

#[test]
fn invalid_configuration_returns_a_nonzero_exit_status() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("invalid.json");
    fs::write(&config, "{not json}").unwrap();
    let output = cli()
        .args(["check-config", "--config"])
        .arg(config)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Configuration error:"));
    assert!(output.stdout.is_empty());
}

#[test]
fn judging_is_not_exposed_before_execution_is_implemented() {
    let output = cli().args(["judge", "solution.py"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
}
