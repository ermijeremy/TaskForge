#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
//! Explicit native-host checks of CLI JSON, exit codes and cleanup.
use judge_local::reporting::MAX_JSON_BYTES;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use taskforge_execution::ExecutionConfig;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_judge-local"))
}
fn native_config() -> PathBuf {
    fs::canonicalize(std::env::var_os("TASKFORGE_REAL_CONFIG").expect("set TASKFORGE_REAL_CONFIG"))
        .unwrap()
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn document(output: &Output) -> serde_json::Value {
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() <= MAX_JSON_BYTES + 1);
    assert_eq!(output.stdout.last(), Some(&b'\n'));
    assert_eq!(output.stdout.iter().filter(|&&b| b == b'\n').count(), 1);
    serde_json::from_slice(&output.stdout).unwrap()
}
fn assert_clean(config: &ExecutionConfig) {
    let cgroot = fs::read_to_string("/run/isolate/cgroup").unwrap();
    for id in config.sandbox().box_id_start..=config.sandbox().box_id_end {
        assert!(
            !Path::new("/var/local/lib/isolate")
                .join(id.to_string())
                .exists()
        );
        assert!(!Path::new(cgroot.trim()).join(format!("box-{id}")).exists());
        assert!(
            !config
                .sandbox()
                .lock_directory
                .join(format!("box-{id}.quarantine"))
                .exists()
        );
    }
}
#[test]
#[ignore = "requires the supported native Linux/Isolate host; run explicitly"]
fn real_readiness_json_is_machine_readable_and_cleans_its_boxes() {
    let path = native_config();
    let output = cli()
        .args(["check-ready", "--json", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value = document(&output);
    assert!(value["isolate_version"].is_string());
    assert!(value["python_version"].is_string());
    assert!(!value["checked_box_ids"].as_array().unwrap().is_empty());
    assert_clean(&ExecutionConfig::load(path).unwrap());
}
#[test]
#[ignore = "requires the supported native Linux/Isolate host; run explicitly"]
fn real_judging_cli_reports_fixture_verdicts_and_distinguishes_infrastructure_failure() {
    let path = native_config();
    let config = ExecutionConfig::load(&path).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let source_dir = root().join("fixtures/submissions");
    let command_tmp = directory.path().join("command-tmp");
    fs::create_dir(&command_tmp).unwrap();
    for (id, source, verdict) in [
        ("sum-two-integers-v2", "sum_two_accepted.py", "accepted"),
        ("sum-list-v2", "sum_list_accepted.py", "accepted"),
        (
            "balanced-brackets-v2",
            "balanced_brackets_accepted.py",
            "accepted",
        ),
        ("sum-two-integers-v2", "sum_two_wrong.py", "wrong_answer"),
        ("sum-two-integers-v2", "sum_two_whitespace.py", "accepted"),
        ("sum-two-integers-v2", "syntax_error.py", "runtime_error"),
        ("sum-two-integers-v2", "exception.py", "runtime_error"),
        (
            "sum-two-integers-v2",
            "infinite_loop.py",
            "time_limit_exceeded",
        ),
        ("sum-two-integers-v2", "sleeping.py", "time_limit_exceeded"),
        (
            "sum-two-integers-v2",
            "stdout_flood.py",
            "output_limit_exceeded",
        ),
        (
            "sum-two-integers-v2",
            "stderr_flood.py",
            "output_limit_exceeded",
        ),
        (
            "sum-two-integers-v2",
            "mixed_output_flood.py",
            "output_limit_exceeded",
        ),
        ("sum-two-integers-v2", "memory_pressure.py", "memory_probe"),
    ] {
        let output = cli()
            .env("TMPDIR", &command_tmp)
            .current_dir(directory.path())
            .args(["judge", "--problem", id, "--source"])
            .arg(source_dir.join(source))
            .arg("--config")
            .arg(&path)
            .arg("--problems-dir")
            .arg(root().join("fixtures/problems"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "fixture {source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value = document(&output);
        if verdict == "memory_probe" {
            assert!(matches!(
                value["verdict"].as_str(),
                Some("memory_limit_exceeded" | "runtime_error")
            ));
        } else {
            assert_eq!(value["verdict"], verdict, "fixture {source}");
        }
        assert!(value.get("test_cases").is_none());
        assert!(value.get("expected_output").is_none());
        if source == "sum_two_wrong.py" {
            assert_eq!(value["passed_tests"], 1);
            assert_eq!(value["first_failing_test"], 1);
        }
        assert_clean(&config);
        assert!(
            fs::read_dir(&command_tmp).unwrap().next().is_none(),
            "CLI left trusted temporary files or a runtime image"
        );
    }
    // Existing but incompatible executable: startup must fail explicitly with no host fallback.
    let mut invalid: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    invalid["runtime"]["python_path"] = serde_json::json!(config.python_path());
    invalid["runtime"]["isolate_path"] = serde_json::json!(config.python_path());
    invalid["sandbox"]["lock_directory"] = serde_json::json!(config.sandbox().lock_directory);
    let bad_config = directory.path().join("unavailable.json");
    fs::write(&bad_config, serde_json::to_vec(&invalid).unwrap()).unwrap();
    let output = cli()
        .env("TMPDIR", &command_tmp)
        .args(["judge", "--problem", "sum-two-integers-v2", "--source"])
        .arg(source_dir.join("sum_two_accepted.py"))
        .arg("--config")
        .arg(bad_config)
        .arg("--problems-dir")
        .arg(root().join("fixtures/problems"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    let error = document(&output);
    assert_eq!(error["reason_code"], "sandbox_unavailable");
    assert!(error.get("verdict").is_none());
    assert_clean(&config);
}
