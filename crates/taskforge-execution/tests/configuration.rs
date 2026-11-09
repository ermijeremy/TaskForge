#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use taskforge_execution::{ConfigError, ExecutionConfig};
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        for name in ["python fixture", "isolate fixture"] {
            let path = directory.path().join(name);
            // Static validation must not execute this intentionally failing file.
            fs::write(&path, "#!/bin/sh\nexit 99\n").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self { directory }
    }

    fn settings(&self) -> Value {
        json!({"runtime": {
            "python_path": "python fixture",
            "isolate_path": "isolate fixture"
        }})
    }

    fn write(&self, settings: &Value) -> PathBuf {
        let path = self.directory.path().join("judge.json");
        fs::write(&path, serde_json::to_vec(settings).unwrap()).unwrap();
        path
    }

    fn load(&self, settings: &Value) -> Result<ExecutionConfig, ConfigError> {
        ExecutionConfig::load(self.write(settings))
    }
}

#[test]
fn defaults_have_explicit_units_and_paths_use_the_config_directory() {
    let fixture = Fixture::new();
    let config = fixture.load(&fixture.settings()).unwrap();
    let limits = config.limits();

    assert_eq!(limits.cpu_time(), Duration::from_secs(1));
    assert_eq!(limits.wall_time(), Duration::from_secs(3));
    assert_eq!(limits.memory_bytes().get(), 128 * 1024 * 1024);
    assert_eq!(limits.output_bytes().get(), 1024 * 1024);
    assert_eq!(limits.processes().get(), 1);
    assert_eq!(config.evaluation_timeout(), Duration::from_secs(60));
    assert_eq!(
        config.python_path(),
        fs::canonicalize(fixture.directory.path().join("python fixture")).unwrap()
    );
    assert!(config.isolate_path().is_absolute());
}

#[test]
fn custom_positive_limits_are_preserved() {
    let fixture = Fixture::new();
    let mut settings = fixture.settings();
    settings["limits"] = json!({
        "cpu_time_ms": 25, "wall_time_ms": 100,
        "memory_bytes": 4096, "output_bytes": 17, "processes": 2
    });
    settings["evaluation_timeout_ms"] = json!(150);
    let config = fixture.load(&settings).unwrap();

    assert_eq!(config.limits().cpu_time(), Duration::from_millis(25));
    assert_eq!(config.limits().wall_time(), Duration::from_millis(100));
    assert_eq!(config.limits().memory_bytes().get(), 4096);
    assert_eq!(config.limits().output_bytes().get(), 17);
    assert_eq!(config.limits().processes().get(), 2);
    assert_eq!(config.evaluation_timeout(), Duration::from_millis(150));
}

#[test]
fn every_zero_limit_is_rejected_with_its_field_name() {
    let fixture = Fixture::new();
    for field in [
        "cpu_time_ms",
        "wall_time_ms",
        "memory_bytes",
        "output_bytes",
        "processes",
    ] {
        let mut settings = fixture.settings();
        settings["limits"] = json!({field: 0});
        let error = fixture.load(&settings).unwrap_err();
        match error {
            ConfigError::NonPositive { field: actual } => {
                assert_eq!(actual, format!("limits.{field}"))
            }
            other => panic!("unexpected error for {field}: {other}"),
        }
    }
    let mut settings = fixture.settings();
    settings["evaluation_timeout_ms"] = json!(0);
    assert!(matches!(
        fixture.load(&settings),
        Err(ConfigError::NonPositive {
            field: "evaluation_timeout_ms"
        })
    ));
}

#[test]
fn negative_fractional_and_out_of_range_values_are_rejected() {
    let fixture = Fixture::new();
    for invalid in [
        json!({"cpu_time_ms": -1}),
        json!({"wall_time_ms": 0.5}),
        json!({"processes": u64::from(u32::MAX) + 1}),
    ] {
        let mut settings = fixture.settings();
        settings["limits"] = invalid;
        assert!(matches!(
            fixture.load(&settings),
            Err(ConfigError::Parse(_))
        ));
    }
}

#[test]
fn unknown_fields_are_rejected_instead_of_silently_using_defaults() {
    let fixture = Fixture::new();
    for section in ["root", "limits", "runtime"] {
        let mut settings = fixture.settings();
        match section {
            "root" => settings["cpu_time_mss"] = json!(5),
            "limits" => settings["limits"] = json!({"memory_byte": 1}),
            _ => settings["runtime"]["isolate_pat"] = json!("wrong"),
        }
        assert!(matches!(
            fixture.load(&settings),
            Err(ConfigError::Parse(_))
        ));
    }
}

#[test]
fn missing_empty_directory_and_nonexecutable_paths_are_rejected() {
    let fixture = Fixture::new();
    let nonexecutable = fixture.directory.path().join("not executable");
    fs::write(&nonexecutable, "not a runtime").unwrap();
    fs::set_permissions(&nonexecutable, fs::Permissions::from_mode(0o600)).unwrap();

    let mut settings = fixture.settings();
    settings["runtime"]["isolate_path"] = json!("missing");
    assert!(matches!(
        fixture.load(&settings),
        Err(ConfigError::AccessExecutable {
            field: "runtime.isolate_path",
            ..
        })
    ));
    settings["runtime"]["isolate_path"] = json!("");
    assert!(matches!(
        fixture.load(&settings),
        Err(ConfigError::EmptyPath { .. })
    ));
    settings["runtime"]["isolate_path"] = json!(".");
    assert!(matches!(
        fixture.load(&settings),
        Err(ConfigError::NotFile { .. })
    ));
    settings["runtime"]["isolate_path"] = json!(nonexecutable);
    assert!(matches!(
        fixture.load(&settings),
        Err(ConfigError::NotExecutable { .. })
    ));
}

#[test]
fn missing_and_malformed_configuration_files_return_errors() {
    let fixture = Fixture::new();
    let path = fixture.directory.path().join("missing.json");
    assert!(matches!(
        ExecutionConfig::load(path),
        Err(ConfigError::ReadConfig { .. })
    ));
    let path = fixture.directory.path().join("invalid.json");
    fs::write(&path, "{invalid json").unwrap();
    assert!(matches!(
        ExecutionConfig::load(path),
        Err(ConfigError::Parse(_))
    ));
}

#[test]
fn absolute_runtime_paths_are_not_prefixed_with_the_config_directory() {
    let fixture = Fixture::new();
    let mut settings = fixture.settings();
    let path = fs::canonicalize(fixture.directory.path().join("python fixture")).unwrap();
    settings["runtime"]["python_path"] = json!(path);
    let config = fixture.load(&settings).unwrap();
    assert_eq!(config.python_path(), Path::new(&path));
}
