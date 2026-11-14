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
    if test -f slow-init; then exec sleep 60; fi
    echo "$PWD/boxes/$id";;
--cleanup)
    if test -f fail-cleanup; then exit 2; fi
    rm -rf "boxes/$id";;
--run)
    if test -f term-run; then
        trap 'echo stopped > term-seen; exit 0' TERM
        echo ready > run-ready
        while :; do :; done
    fi
    if test -f sleep-run; then exec sleep 60; fi
    if test -f fail-run; then exit 2; fi
    for arg in "$@"; do case "$arg" in --meta=*) meta=${arg#--meta=};; esac; done
    if test -f bad-meta; then
        printf 'exitcode:0\ntime:NaN\n' > "$meta"
    elif ! test -f missing-meta; then
        printf 'exitcode:0\ntime:0.001\ntime-wall:0.002\ncg-mem:8\n' > "$meta"
    fi
    cat >/dev/null
    if test -f mixed-output; then
        head -c 32768 /dev/zero
        head -c 32768 /dev/zero >&2
    elif test -f stderr-output; then
        head -c 65536 /dev/zero >&2
    elif test -f stdout-output; then
        head -c 65536 /dev/zero
    else
        echo 5
    fi;;
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
fn cancellation_allows_the_manager_to_terminate_its_sandbox_before_cleanup() {
    let f = Fixture::new();
    f.flag("term-run");
    let running = f.prepare().unwrap().spawn(b"").unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while !f.directory.path().join("run-ready").exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    running.cancel().unwrap();
    assert_eq!(
        fs::read_to_string(f.directory.path().join("term-seen")).unwrap(),
        "stopped\n"
    );
    assert!(!f.directory.path().join("boxes/3").exists());
    fs::remove_file(f.directory.path().join("term-run")).unwrap();
    f.prepare().unwrap().cleanup().unwrap();
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

#[test]
fn shared_deadline_interrupts_setup_and_cleanup_releases_the_slot() {
    let f = Fixture::new();
    f.flag("slow-init");
    let control = ExecutionControl::evaluation(Duration::from_millis(500)).unwrap();
    let deadline = control.final_deadline().unwrap();
    let error = f
        .backend
        .prepare_with_control("print(5)", &ExecutionLimits::default(), control)
        .unwrap_err();
    assert_eq!(
        error.reason_code(),
        InfrastructureReason::EvaluationDeadlineExceeded
    );
    assert!(Instant::now() < deadline);
    assert!(f.calls().contains("--cleanup"));
    assert!(!f.directory.path().join("boxes/3").exists());
    fs::remove_file(f.directory.path().join("slow-init")).unwrap();
    assert_eq!(f.prepare().unwrap().box_id(), 3);
}

#[test]
fn cancellation_during_setup_still_performs_whole_box_cleanup() {
    let f = Fixture::new();
    f.flag("slow-init");
    let control = ExecutionControl::evaluation(Duration::from_secs(3)).unwrap();
    let cancellation = control.cancellation();
    let calls = f.directory.path().join("calls");
    let canceller = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(1);
        while !fs::read_to_string(&calls)
            .unwrap_or_default()
            .contains("--init")
        {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        cancellation.cancel();
    });
    let error = f
        .backend
        .prepare_with_control("print(5)", &ExecutionLimits::default(), control)
        .unwrap_err();
    canceller.join().unwrap();
    assert_eq!(error.reason_code(), InfrastructureReason::Cancelled);
    assert!(f.calls().contains("--cleanup"));
    assert!(!f.directory.path().join("boxes/3").exists());
    fs::remove_file(f.directory.path().join("slow-init")).unwrap();
    f.prepare().unwrap().cleanup().unwrap();
}

#[test]
fn exercise_guard_blocks_launch_until_output_accounting_is_available() {
    use crate::supervision::{OutputBudget, OutputStream, Supervisor};
    let f = Fixture::new();
    let supervisor = Supervisor::from_backend(f.backend.clone());
    let session = supervisor.evaluation().unwrap();
    let mut budget = OutputBudget::new(ExecutionLimits::default().output_bytes());
    // This regression remains valid when the owner completes the exercise.
    if budget.record(OutputStream::Stdout, b"").is_err() {
        let error = session
            .start_test("print(5)", b"", &ExecutionLimits::default())
            .unwrap_err();
        assert_eq!(error.reason_code(), InfrastructureReason::InternalFailure);
        assert!(f.calls().is_empty());
        assert!(session.is_idle());
    } else {
        runtime().block_on(async {
            let result = session
                .run_test("print(5)", b"", &ExecutionLimits::default())
                .await
                .unwrap();
            assert_eq!(result.output.stdout(), b"5\n");
        });
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
}

#[test]
fn supervised_capture_and_errors_cleanup_before_slot_reuse() {
    use crate::supervision::Supervisor;
    let f = Fixture::new();
    runtime().block_on(async {
        let supervisor = Supervisor::from_backend(f.backend.clone());
        let session = supervisor.evaluation().unwrap();
        let deadline = session.deadline();
        for _ in 0..2 {
            let result = session
                .run_test("print(5)", b"", &ExecutionLimits::default())
                .await
                .unwrap();
            assert_eq!(result.output.stdout(), b"5\n");
            assert!(result.output.stderr().is_empty());
            assert!(!result.output_limit_exceeded);
            assert_eq!(result.metadata.unwrap().exit_code, Some(0));
            assert_eq!(session.deadline(), deadline);
            assert!(session.is_idle());
            assert!(!f.directory.path().join("boxes/3").exists());
        }
        for (flag, reason) in [
            ("fail-init", InfrastructureReason::SandboxSetupFailed),
            ("fail-run", InfrastructureReason::SandboxSetupFailed),
            ("missing-meta", InfrastructureReason::MetadataMissing),
            ("bad-meta", InfrastructureReason::MetadataInvalid),
        ] {
            f.flag(flag);
            let error = session
                .run_test("print(5)", b"", &ExecutionLimits::default())
                .await
                .unwrap_err();
            assert_eq!(error.reason_code(), reason);
            assert!(!f.directory.path().join("boxes/3").exists());
            fs::remove_file(f.directory.path().join(flag)).unwrap();
        }
        session
            .run_test("print(5)", b"", &ExecutionLimits::default())
            .await
            .unwrap();
    });
}

#[test]
fn supervised_stdout_stderr_and_mixed_excess_are_bounded() {
    use crate::supervision::Supervisor;
    let f = Fixture::new();
    let limits = ExecutionLimits::try_from(crate::LimitSettings {
        output_bytes: 40_000,
        ..crate::LimitSettings::default()
    })
    .unwrap();
    runtime().block_on(async {
        let supervisor = Supervisor::from_backend(f.backend.clone());
        for flag in ["stdout-output", "stderr-output", "mixed-output"] {
            f.flag(flag);
            let session = supervisor.evaluation().unwrap();
            let result = session
                .run_test("ignored by trusted double", b"", &limits)
                .await
                .unwrap();
            assert!(result.output_limit_exceeded);
            assert_eq!(result.output.retained_bytes(), 40_000);
            assert!(result.output.observed_bytes() > 40_000);
            assert!(result.metadata.is_none());
            if flag == "mixed-output" {
                assert!(!result.output.stdout().is_empty());
                assert!(!result.output.stderr().is_empty());
            }
            assert!(!f.directory.path().join("boxes/3").exists());
            fs::remove_file(f.directory.path().join(flag)).unwrap();
        }
        supervisor
            .evaluation()
            .unwrap()
            .run_test("print(5)", b"", &limits)
            .await
            .unwrap();
    });
}

#[test]
fn explicit_and_dropped_waiter_cancellation_allow_reuse_without_blocking_tokio() {
    use crate::supervision::Supervisor;
    let f = Fixture::new();
    f.flag("sleep-run");
    runtime().block_on(async {
        let supervisor = Supervisor::from_backend(f.backend.clone());
        let session = supervisor.evaluation().unwrap();
        let task = session
            .start_test("ignored", b"", &ExecutionLimits::default())
            .unwrap();
        // A current-thread runtime must continue ticking while the worker blocks.
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!session.is_idle());
        assert!(
            session
                .start_test("ignored", b"", &ExecutionLimits::default())
                .is_err()
        );
        task.cancel();
        assert_eq!(
            task.wait().await.unwrap_err().reason_code(),
            InfrastructureReason::Cancelled
        );
        assert!(session.is_idle());
        assert!(!f.directory.path().join("boxes/3").exists());

        let session = supervisor.evaluation().unwrap();
        let task = session
            .start_test("ignored", b"", &ExecutionLimits::default())
            .unwrap();
        let waiter = tokio::spawn(task.wait());
        tokio::time::sleep(Duration::from_millis(30)).await;
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        tokio::time::timeout(Duration::from_secs(2), async {
            while !session.is_idle() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        assert!(!f.directory.path().join("boxes/3").exists());
        fs::remove_file(f.directory.path().join("sleep-run")).unwrap();
        supervisor
            .evaluation()
            .unwrap()
            .run_test("print(5)", b"", &ExecutionLimits::default())
            .await
            .unwrap();
    });
}

#[test]
fn exhausted_cleanup_budget_attempts_recovery_and_quarantines_the_slot() {
    let f = Fixture::new();
    let mut prepared = f.prepare().unwrap();
    prepared.control = ExecutionControl::evaluation(Duration::from_nanos(1)).unwrap();
    thread::sleep(Duration::from_millis(1));
    assert_eq!(
        prepared.cleanup().unwrap_err().reason_code(),
        InfrastructureReason::CleanupFailed
    );
    assert!(f.calls().contains("--cleanup"));
    assert!(!f.directory.path().join("boxes/3").exists());
    assert!(f.directory.path().join("locks/box-3.quarantine").exists());
    assert_eq!(f.prepare().unwrap().box_id(), 4);
}

#[test]
fn supervisor_rejects_unsupported_output_capacity_before_allocating_a_box() {
    use crate::supervision::{MAX_CAPTURE_BYTES, Supervisor};
    let f = Fixture::new();
    let supervisor = Supervisor::from_backend(f.backend.clone());
    let session = supervisor.evaluation().unwrap();
    let limits = ExecutionLimits::try_from(crate::LimitSettings {
        output_bytes: MAX_CAPTURE_BYTES as u64 + 1,
        ..crate::LimitSettings::default()
    })
    .unwrap();
    assert_eq!(
        session
            .start_test("print(5)", b"", &limits)
            .unwrap_err()
            .reason_code(),
        InfrastructureReason::InvalidConfiguration
    );
    assert!(f.calls().is_empty());
}
