#![cfg(target_os = "linux")]
// Trusted protocol doubles only. No submitted Python executes in these tests.
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Mutex, MutexGuard};

// Serialize script creation/execution so another test cannot fork while a new
// executable script is still open for writing (a transient ETXTBSY race).
static PROTOCOL_LOCK: Mutex<()> = Mutex::new(());

struct Fixture {
    _guard: MutexGuard<'static, ()>,
    directory: TempDir,
    backend: IsolateBackend,
}

impl Fixture {
    fn new() -> Self {
        let guard = PROTOCOL_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("locks")).unwrap();
        fs::set_permissions(
            directory.path().join("locks"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let isolate = directory.path().join("isolate");
        fs::write(
            &isolate,
            r#"#!/bin/sh
cd -- "$(dirname -- "$0")"
id=${2#--box-id=}
printf '%s\n' "$*" >> calls
case "$3" in
--init)
    mkdir -p "boxes/$id/box"
    if test -f fail-init; then exit 2; fi
    echo "$PWD/boxes/$id";;
--cleanup)
    if test -f fail-cleanup; then exit 2; fi
    rm -rf "boxes/$id";;
--run)
    if test -f sleep-run; then exec sleep 60; fi
    if test -f fail-run; then exit 2; fi
    for arg in "$@"; do case "$arg" in --meta=*) meta=${arg#--meta=};; esac; done
    if test -f bad-meta; then
        printf 'exitcode:0\ntime:NaN\n' > "$meta"
    elif ! test -f missing-meta; then
        printf 'exitcode:0\ntime:0.001\ntime-wall:0.002\ncg-mem:8\n' > "$meta"
    fi
    cat >/dev/null
    echo 5;;
esac
"#,
        )
        .unwrap();
        fs::set_permissions(&isolate, fs::Permissions::from_mode(0o700)).unwrap();
        let settings: crate::ExecutionSettings = serde_json::from_value(serde_json::json!({
            "runtime": { "python_path": "isolate", "isolate_path": "isolate" },
            "sandbox": { "box_id_start": 3, "box_id_end": 4, "lock_directory": "locks" }
        }))
        .unwrap();
        let config = settings.validate(directory.path()).unwrap();
        let backend = IsolateBackend {
            config,
            runtime: Arc::new(RuntimeImage::empty_for_test()),
        };
        Self {
            _guard: guard,
            directory,
            backend,
        }
    }
    fn prepare(&self) -> Result<PreparedSandbox, InfrastructureError> {
        self.backend
            .prepare("print(5)\n", &ExecutionLimits::default())
    }

    fn flag(&self, flag: &str) {
        fs::write(self.directory.path().join(flag), "").unwrap();
    }
    fn calls(&self) -> String {
        fs::read_to_string(self.directory.path().join("calls")).unwrap_or_default()
    }
}

fn collect(mut running: RunningSandbox) -> Result<SandboxExit, InfrastructureError> {
    let mut stdout = String::new();
    running
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    running
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(stdout, "5\n");
    assert!(stderr.is_empty());
    running.finish()
}

#[test]
fn protocol_stages_only_read_only_source_and_reuses_a_clean_slot() {
    let f = Fixture::new();
    for _ in 0..2 {
        let sandbox = f.prepare().unwrap();
        assert_eq!(sandbox.box_id(), 3);
        let source = sandbox.box_root.join("box/main.py");
        assert_eq!(fs::read_to_string(&source).unwrap(), "print(5)\n");
        assert_eq!(
            fs::metadata(&source).unwrap().permissions().mode() & 0o777,
            0o444
        );
        assert_eq!(fs::read_dir(source.parent().unwrap()).unwrap().count(), 1);
        let command = sandbox.run_command().unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--silent".into()));
        assert!(args.contains(&"--no-default-dirs".into()));
        assert!(args.contains(&"--dir=box=./box:norec".into()));
        assert!(args.contains(&"--processes=1".into()));
        assert!(args.contains(&"--time=1.000".into()));
        assert!(args.contains(&"--cg-mem=131072".into()));
        assert!(
            !args
                .iter()
                .any(|arg| arg.contains(":rw") || arg == "--share-net" || arg == "--full-env")
        );
        assert_eq!(
            &args[args.len() - 5..],
            ["/usr/bin/python3.14", "-I", "-B", "-S", "/box/main.py"]
        );
        let running = sandbox.spawn(b"2 3\n").unwrap();
        assert_eq!(running.output_limit_bytes(), 1_048_576);
        let result = collect(running).unwrap();
        assert_eq!(result.metadata.exit_code, Some(0));
        assert!(!f.directory.path().join("boxes/3").exists());
    }
    assert_eq!(f.calls().matches("--cleanup").count(), 2);
}

#[test]
fn live_leases_skip_busy_ids_and_report_exhaustion_without_resetting_boxes() {
    let f = Fixture::new();
    let first = f.prepare().unwrap();
    let second = f.prepare().unwrap();
    assert_eq!((first.box_id(), second.box_id()), (3, 4));
    assert_eq!(
        f.prepare().unwrap_err().reason_code(),
        InfrastructureReason::SandboxUnavailable
    );
    assert_eq!(f.calls().matches("--init").count(), 2);
    first.cleanup().unwrap();
    let next = f.prepare().unwrap();
    assert_eq!(next.box_id(), 3);
    next.cleanup().unwrap();
    second.cleanup().unwrap();
}

#[test]
fn setup_failure_and_invalid_input_attempt_cleanup_and_allow_reuse() {
    let f = Fixture::new();
    f.flag("fail-init");
    assert_eq!(
        f.prepare().unwrap_err().reason_code(),
        InfrastructureReason::SandboxSetupFailed
    );
    assert_eq!(f.calls().matches("--cleanup").count(), 1);
    fs::remove_file(f.directory.path().join("fail-init")).unwrap();
    let sandbox = f.prepare().unwrap();
    assert!(
        sandbox
            .spawn(&vec![0; crate::problem::MAX_CASE_BYTES + 1])
            .is_err()
    );
    assert!(!f.directory.path().join("boxes/3").exists());
    f.prepare().unwrap().cleanup().unwrap();
}

#[test]
fn execution_and_metadata_errors_remain_infrastructure_failures_with_cleanup() {
    for (flag, code) in [
        ("fail-run", InfrastructureReason::SandboxSetupFailed),
        ("bad-meta", InfrastructureReason::MetadataInvalid),
        ("missing-meta", InfrastructureReason::MetadataMissing),
    ] {
        let f = Fixture::new();
        f.flag(flag);
        let mut running = f.prepare().unwrap().spawn(b"2 3\n").unwrap();
        let mut out = Vec::new();
        running
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        running
            .stderr
            .take()
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(running.finish().unwrap_err().reason_code(), code);
        assert!(!f.directory.path().join("boxes/3").exists());
    }
}

#[test]
fn cleanup_failure_quarantines_the_slot_instead_of_claiming_success() {
    let f = Fixture::new();
    let sandbox = f.prepare().unwrap();
    f.flag("fail-cleanup");
    assert_eq!(
        sandbox.cleanup().unwrap_err().reason_code(),
        InfrastructureReason::CleanupFailed
    );
    assert!(f.directory.path().join("locks/box-3.quarantine").is_file());
    fs::remove_file(f.directory.path().join("fail-cleanup")).unwrap();
    let next = f.prepare().unwrap();
    assert_eq!(next.box_id(), 4);
    next.cleanup().unwrap();
}

#[test]
fn explicit_cancel_and_drop_cleanup_the_protocol_box() {
    let f = Fixture::new();
    f.flag("sleep-run");
    let running = f.prepare().unwrap().spawn(b"").unwrap();
    running.cancel().unwrap();
    assert!(!f.directory.path().join("boxes/3").exists());
    let sandbox = f.prepare().unwrap();
    drop(sandbox);
    assert!(!f.directory.path().join("boxes/3").exists());
}

#[test]
fn slot_locks_reject_symlinks_and_mount_arguments_reject_option_injection() {
    let f = Fixture::new();
    let target = f.directory.path().join("target");
    fs::write(&target, "marker").unwrap();
    std::os::unix::fs::symlink(&target, f.directory.path().join("locks/box-3.lock")).unwrap();
    assert!(SlotLease::reserve(&f.backend.config).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "marker");
    assert!(mount_path(Path::new("/tmp/unsafe:rw")).is_err());
    assert!(mount_path(Path::new("relative")).is_err());
}

#[test]
fn public_preparation_honors_conversion_failure_before_touching_a_box() {
    let f = Fixture::new();
    let limits = ExecutionLimits::try_from(crate::LimitSettings {
        processes: 2,
        ..crate::LimitSettings::default()
    })
    .unwrap();
    assert_eq!(
        f.backend
            .prepare("print(5)\n", &limits)
            .unwrap_err()
            .reason_code(),
        InfrastructureReason::InvalidConfiguration
    );
    assert!(f.calls().is_empty());
    assert!(!f.directory.path().join("boxes").exists());
}
