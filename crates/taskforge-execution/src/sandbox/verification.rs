#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
//! Real failure injection is confined to this test's private runtime/metadata.
use super::*;
use crate::supervision::Supervisor;

fn native_config() -> ExecutionConfig {
    let path = fs::canonicalize(
        std::env::var_os("TASKFORGE_REAL_CONFIG")
            .expect("set TASKFORGE_REAL_CONFIG to the supported host configuration"),
    )
    .unwrap();
    let mut settings: crate::ExecutionSettings =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    settings.sandbox.box_id_end = settings.sandbox.box_id_start;
    settings.validate(path.parent().unwrap()).unwrap()
}
fn clean(config: &ExecutionConfig) {
    let id = config.sandbox().box_id_start;
    assert!(
        !Path::new("/var/local/lib/isolate")
            .join(id.to_string())
            .exists()
    );
    let root = fs::read_to_string("/run/isolate/cgroup").unwrap();
    assert!(!Path::new(root.trim()).join(format!("box-{id}")).exists());
    assert!(
        !config
            .sandbox()
            .lock_directory
            .join(format!("box-{id}.quarantine"))
            .exists()
    );
}
#[derive(Clone, Copy)]
enum MetadataFault {
    None,
    Missing,
    Corrupt,
}

fn fixed_probe(
    backend: &IsolateBackend,
    fault: MetadataFault,
) -> Result<SandboxExit, InfrastructureError> {
    bounded_probe(backend, fault, "print(5)\n")
}

fn bounded_probe(
    backend: &IsolateBackend,
    fault: MetadataFault,
    source: &str,
) -> Result<SandboxExit, InfrastructureError> {
    let mut running = backend
        .prepare(source, &ExecutionLimits::default())?
        .spawn(b"")?;
    let metadata = running
        .sandbox
        .as_ref()
        .unwrap()
        .host
        .path()
        .join("metadata");
    let stdout = running.stdout.take().unwrap();
    let stderr = running.stderr.take().unwrap();
    // Fixed benign source, two bounded readers, and an independent harness guard.
    let drain = |reader: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut data = Vec::new();
            reader.take(4097).read_to_end(&mut data).unwrap();
            data
        })
    };
    let out = drain(Box::new(stdout));
    let err = drain(Box::new(stderr));
    let deadline = Instant::now() + Duration::from_secs(5);
    while running.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            running.cancel()?;
            let _ = out.join();
            let _ = err.join();
            return Err(error(
                InfrastructureReason::EvaluationDeadlineExceeded,
                "fixed native probe exceeded its harness guard",
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
    match fault {
        MetadataFault::None => {}
        MetadataFault::Missing => fs::remove_file(&metadata).unwrap(),
        MetadataFault::Corrupt => fs::write(&metadata, b"exitcode:0\ntime:NaN\n").unwrap(),
    }
    let result = running.finish();
    assert!(out.join().unwrap().len() <= 4096);
    assert!(err.join().unwrap().len() <= 4096);
    result
}

#[test]
#[ignore = "requires native Isolate; injects faults only in private staged files"]
fn real_setup_metadata_and_busy_slot_errors_cleanup_and_allow_reuse() {
    let config = native_config();
    let backend = IsolateBackend::new(config.clone()).unwrap();
    let image_root = backend.runtime.usr().parent().unwrap().to_owned();
    let interpreter = backend.runtime.usr().join("bin/python3.14");
    let saved = interpreter.with_extension("saved");
    // Remove only the copied interpreter, never the host's Python installation.
    fs::rename(&interpreter, &saved).unwrap();
    let setup = fixed_probe(&backend, MetadataFault::None);
    fs::rename(&saved, &interpreter).unwrap();
    assert_eq!(
        setup.unwrap_err().reason_code(),
        InfrastructureReason::SandboxSetupFailed
    );
    clean(&config);
    fixed_probe(&backend, MetadataFault::None).unwrap();
    clean(&config);
    for (fault, expected) in [
        (
            MetadataFault::Missing,
            InfrastructureReason::MetadataMissing,
        ),
        (
            MetadataFault::Corrupt,
            InfrastructureReason::MetadataInvalid,
        ),
    ] {
        assert_eq!(
            fixed_probe(&backend, fault).unwrap_err().reason_code(),
            expected
        );
        clean(&config);
        fixed_probe(&backend, MetadataFault::None).unwrap();
        clean(&config);
    }
    let lease = SlotLease::reserve(&config).unwrap();
    assert_eq!(
        backend
            .prepare("print(5)", config.limits())
            .unwrap_err()
            .reason_code(),
        InfrastructureReason::SandboxUnavailable
    );
    clean(&config);
    drop(lease);
    fixed_probe(&backend, MetadataFault::None).unwrap();
    clean(&config);
    let program_failure =
        bounded_probe(&backend, MetadataFault::None, "raise SystemExit(127)\n").unwrap();
    assert_eq!(program_failure.metadata.exit_code, Some(127));
    assert_eq!(
        crate::runner::classify_execution(false, Some(&program_failure.metadata)).unwrap(),
        crate::runner::ExecutionDecision::RuntimeError
    );
    clean(&config);
    backend.shutdown().unwrap();
    assert!(!image_root.exists());
}

#[test]
#[ignore = "requires native Isolate; runs the three named access-probe fixtures"]
fn real_catalog_access_probes_deny_process_network_and_host_file_access() {
    let config = native_config();
    let backend = IsolateBackend::new(config.clone()).unwrap();
    let image_root = backend.runtime.usr().parent().unwrap().to_owned();
    let supervisor = Supervisor::from_backend(backend);
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let host = tempfile::tempdir().unwrap();
    let marker = host.path().join("host-only-marker.txt");
    let marker_content = fs::read(fixtures.join("private/host-only-marker.txt")).unwrap();
    fs::write(&marker, &marker_content).unwrap();
    use std::os::unix::fs::PermissionsExt;
    // If accidentally mounted, ordinary permissions would permit a read.
    fs::set_permissions(host.path(), fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(fs::read(&marker).unwrap(), marker_content);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        for (file, expected) in [
            ("subprocess_probe.py", b"SUBPROCESS_DENIED\n".as_slice()),
            ("network_probe.py", b"NETWORK_DENIED\n".as_slice()),
            ("host_file_probe.py", b"HOST_FILE_DENIED\n".as_slice()),
        ] {
            let mut source = fs::read_to_string(fixtures.join("submissions").join(file)).unwrap();
            if file == "host_file_probe.py" {
                // Parameterize the fixture's one path constant with a verified,
                // world-readable host marker; do not mount its directory.
                let original = "\"/var/local/lib/taskforge/host-only-marker.txt\"";
                assert_eq!(source.matches(original).count(), 1);
                source = source.replace(
                    original,
                    &serde_json::to_string(marker.to_str().unwrap()).unwrap(),
                );
            }
            let outcome = supervisor
                .evaluation()
                .unwrap()
                .run_test(&source, b"", config.limits())
                .await
                .unwrap();
            assert_eq!(outcome.output.stdout(), expected);
            assert!(outcome.output.stderr().is_empty());
            assert!(!outcome.output_limit_exceeded);
            assert_eq!(outcome.metadata.unwrap().exit_code, Some(0));
            clean(&config);
        }
        supervisor.shutdown().await.unwrap();
    });
    assert!(!image_root.exists());
}
