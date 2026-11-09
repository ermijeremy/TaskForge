#![cfg(target_os = "linux")]

// Protocol/failure-path tests with fake executables. These do not verify isolation.
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use serde_json::{Value, json};
use taskforge_execution::{ConfigError, ExecutionConfig, ReadinessError, check_readiness};
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        for dir in ["locks", "cgroup", "box"] {
            fs::create_dir(base.join(dir)).unwrap();
        }
        fs::set_permissions(base.join("locks"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(base.join("cgroup/cgroup.controllers"), "cpu memory pids").unwrap();
        executable(&base.join("python"), "#!/bin/sh\necho 'Python 3.14.8'\n");
        executable(
            &base.join("isolate"),
            r#"#!/bin/sh
cd -- "$(dirname -- "$0")"
printf '%s\n' "$*" >> calls
case "$1" in
  --version) printf 'The process isolator 2.7\n(c) 2012--2026 Martin Mares and Bernard Blackham\n'; exit;;
  --print-cg-root) echo "$PWD/cgroup"; exit;;
esac
case "$3" in
  --init)
    echo "$PWD/box"
    if test -f fail-init; then exit 2; fi;;
  --cleanup)
    if test -f fail-cleanup; then echo 'cleanup denied' >&2; exit 2; fi;;
  --run)
    if test -f timeout; then exec sleep 1; fi
    if test -f fail-run; then echo 'sandbox failed' >&2; exit 2; fi
    for arg in "$@"; do
      case "$arg" in --meta=*) meta=${arg#--meta=};; esac
    done
    if ! test -f missing-meta; then
      printf 'exitcode:0\ntime:0.001\ntime-wall:0.003\ncg-mem:8192\n' > "$meta"
    fi
    if test -f metadata-override; then cat metadata-override > "$meta"; fi
    echo 'TASKFORGE_SMOKE_OK 3.14.8';;
esac
"#,
        );
        Self { directory }
    }

    fn settings(&self) -> Value {
        json!({
            "runtime": {"python_path": "python", "isolate_path": "isolate"},
            "sandbox": {"box_id_start": 3, "box_id_end": 4, "lock_directory": "locks"}
        })
    }

    fn load(&self, settings: &Value) -> Result<ExecutionConfig, ConfigError> {
        let path = self.directory.path().join("judge.json");
        fs::write(&path, serde_json::to_vec(settings).unwrap()).unwrap();
        ExecutionConfig::load(path)
    }

    fn config(&self) -> ExecutionConfig {
        self.load(&self.settings()).unwrap()
    }

    fn flag(&self, flag: &str) {
        fs::write(self.directory.path().join(flag), "").unwrap();
    }

    fn calls(&self) -> String {
        fs::read_to_string(self.directory.path().join("calls")).unwrap()
    }
}

fn executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn range_checks_boundaries_and_lock_directory() {
    let f = Fixture::new();
    for (start, end, valid) in [
        (0, 0, true),
        (999, 999, true),
        (5, 4, false),
        (0, 1000, false),
    ] {
        let mut settings = f.settings();
        settings["sandbox"]["box_id_start"] = json!(start);
        settings["sandbox"]["box_id_end"] = json!(end);
        assert_eq!(f.load(&settings).is_ok(), valid);
    }
    let mut settings = f.settings();
    settings["sandbox"]["lock_directory"] = json!("");
    assert!(matches!(
        f.load(&settings),
        Err(ConfigError::EmptyPath { .. })
    ));
    settings["sandbox"] = json!({"box_id_strat": 1});
    assert!(matches!(f.load(&settings), Err(ConfigError::Parse(_))));
}

#[test]
fn protocol_checks_all_ids_and_releases_locks_for_reuse() {
    let f = Fixture::new();
    for _ in 0..2 {
        let report = check_readiness(&f.config()).unwrap();
        assert_eq!(report.checked_box_ids, [3, 4]);
        assert!(report.smoke_metadata.contains("exitcode:0"));
    }
    let calls = f.calls();
    assert_eq!(calls.matches("--init").count(), 4);
    assert_eq!(calls.matches("--cleanup").count(), 4);
    assert_eq!(calls.matches("--run").count(), 2);
    assert!(calls.contains("--no-default-dirs"));
    assert!(calls.contains("--processes=1"));
    assert!(!calls.contains("--share-net"));
}

#[test]
fn failed_initialization_and_probe_always_attempt_cleanup() {
    for flag in ["fail-init", "fail-run", "missing-meta"] {
        let f = Fixture::new();
        f.flag(flag);
        assert!(check_readiness(&f.config()).is_err(), "{flag}");
        assert_eq!(f.calls().matches("--cleanup").count(), 1, "{flag}");
        assert!(!f.calls().contains("--box-id=4"));
        fs::remove_file(f.directory.path().join(flag)).unwrap();
        check_readiness(&f.config()).unwrap();
    }
}

#[test]
fn cleanup_failure_prevents_a_success_report_and_preserves_original_error() {
    let f = Fixture::new();
    f.flag("fail-cleanup");
    f.flag("fail-run");
    let error = check_readiness(&f.config()).unwrap_err();
    assert!(matches!(error, ReadinessError::Cleanup { .. }));
    let error = error.to_string();
    assert!(error.contains("sandbox failed"));
    assert!(error.contains("cleanup denied"));
}

#[test]
fn busy_range_never_initializes_or_cleans_someone_elses_box() {
    let f = Fixture::new();
    let lock = File::create(f.directory.path().join("locks/box-4.lock")).unwrap();
    lock.lock().unwrap();
    assert!(matches!(
        check_readiness(&f.config()),
        Err(ReadinessError::Busy(4))
    ));
    let calls = f.calls();
    assert!(!calls.contains("--init"));
    assert!(!calls.contains("--cleanup"));
    drop(lock);
    check_readiness(&f.config()).unwrap();
}

#[test]
fn deadline_failure_still_runs_cleanup() {
    let f = Fixture::new();
    f.flag("timeout");
    let mut settings = f.settings();
    settings["evaluation_timeout_ms"] = json!(250);
    let error = check_readiness(&f.load(&settings).unwrap()).unwrap_err();
    assert!(matches!(error, ReadinessError::Timeout(_)));
    assert!(f.calls().contains("--cleanup"));
}

#[test]
fn wrong_version_missing_delegation_and_bad_permissions_fail_before_init() {
    for cause in ["version", "delegation", "permissions"] {
        let f = Fixture::new();
        match cause {
            "version" => executable(
                &f.directory.path().join("isolate"),
                "#!/bin/sh\necho 'The process isolator 1.0'\n",
            ),
            "delegation" => {
                fs::write(f.directory.path().join("cgroup/cgroup.controllers"), "cpu").unwrap()
            }
            _ => fs::set_permissions(
                f.directory.path().join("locks"),
                fs::Permissions::from_mode(0o777),
            )
            .unwrap(),
        }
        assert!(matches!(
            check_readiness(&f.config()),
            Err(ReadinessError::Prerequisite(_))
        ));
    }
}

#[test]
fn unavailable_isolate_never_launches_python() {
    let f = Fixture::new();
    executable(
        &f.directory.path().join("isolate"),
        "#!/bin/sh\necho 'sandbox unavailable' >&2\nexit 2\n",
    );
    executable(
        &f.directory.path().join("python"),
        "#!/bin/sh\ntouch \"$(dirname -- \"$0\")/UNSAFE_FALLBACK\"\nexit 99\n",
    );
    assert!(matches!(
        check_readiness(&f.config()),
        Err(ReadinessError::Command { .. })
    ));
    assert!(!f.directory.path().join("UNSAFE_FALLBACK").exists());
}

#[test]
#[ignore = "requires a provisioned native Linux host; never counts a skip as isolation evidence"]
fn real_isolate_readiness_smoke() {
    let path = std::env::var_os("TASKFORGE_REAL_CONFIG")
        .expect("set TASKFORGE_REAL_CONFIG to the provisioned host config");
    let config = ExecutionConfig::load(path).unwrap();
    let report = check_readiness(&config).unwrap();
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    assert!(!report.checked_box_ids.is_empty());
}

#[test]
fn malformed_or_ambiguous_measurements_are_rejected_and_cleaned() {
    for metadata in [
        "exitcode:0\ntime:nan\ntime-wall:1\ncg-mem:1\n",
        "exitcode:0\ntime:-1\ntime-wall:1\ncg-mem:1\n",
        "exitcode:0\ntime:1\ntime:2\ntime-wall:1\ncg-mem:1\n",
        "exitcode:0\ntime:1\ntime-wall:inf\ncg-mem:1\n",
        "exitcode:0\ntime:1\ntime-wall:1\ncg-mem:garbage\n",
        "exitcode:0\ntime:1\ntime-wall:1\ncg-mem:1\nstatus:XX\n",
    ] {
        let f = Fixture::new();
        fs::write(f.directory.path().join("metadata-override"), metadata).unwrap();
        assert!(matches!(
            check_readiness(&f.config()),
            Err(ReadinessError::Prerequisite(_))
        ));
        assert!(f.calls().contains("--cleanup"));
    }
}

#[test]
fn diagnostic_and_metadata_capture_reject_oversized_content() {
    let f = Fixture::new();
    executable(
        &f.directory.path().join("isolate"),
        "#!/bin/sh\nexec head -c 70000 /dev/zero\n",
    );
    assert!(matches!(
        check_readiness(&f.config()),
        Err(ReadinessError::ExcessOutput(_))
    ));
    let f = Fixture::new();
    fs::write(
        f.directory.path().join("metadata-override"),
        vec![b'x'; 70_000],
    )
    .unwrap();
    assert!(matches!(
        check_readiness(&f.config()),
        Err(ReadinessError::ExcessOutput(_))
    ));
    assert!(f.calls().contains("--cleanup"));
}

#[test]
fn multi_process_policy_and_symlinked_lock_fail_without_initialization() {
    let f = Fixture::new();
    let mut settings = f.settings();
    settings["limits"] = json!({"processes": 2});
    assert!(matches!(
        check_readiness(&f.load(&settings).unwrap()),
        Err(ReadinessError::Prerequisite(_))
    ));
    assert!(!f.directory.path().join("calls").exists());
    let target = f.directory.path().join("target");
    fs::write(&target, "do not overwrite").unwrap();
    std::os::unix::fs::symlink(&target, f.directory.path().join("locks/box-3.lock")).unwrap();
    assert!(matches!(
        check_readiness(&f.config()),
        Err(ReadinessError::Io(_))
    ));
    assert!(!f.calls().contains("--init"));
    assert_eq!(fs::read_to_string(target).unwrap(), "do not overwrite");
}
