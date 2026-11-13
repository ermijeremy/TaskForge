#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
//! Explicit native-host tests. An ignored/default run is not isolation evidence.
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use taskforge_execution::sandbox::{IsolateBackend, IsolateLimits, SandboxExit};
use taskforge_execution::{ExecutionConfig, ExecutionSettings, load_problems};

fn config() -> ExecutionConfig {
    let path = fs::canonicalize(
        std::env::var_os("TASKFORGE_REAL_CONFIG")
            .expect("set TASKFORGE_REAL_CONFIG to the provisioned host configuration"),
    )
    .unwrap();
    let mut settings: ExecutionSettings =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    // Every call must reuse one dedicated slot, not accidentally another free ID.
    settings.sandbox.box_id_end = settings.sandbox.box_id_start;
    settings.validate(path.parent().unwrap()).unwrap()
}

fn run(
    backend: &IsolateBackend,
    config: &ExecutionConfig,
    source: &str,
    input: &[u8],
) -> (String, SandboxExit) {
    let prepared = backend.prepare(source, config.limits()).unwrap();
    let id = prepared.box_id();
    let mut running = prepared.spawn(input).unwrap();
    // Harness for these fixed small probes only, not the production supervisor.
    let out = running.stdout.take().unwrap();
    let err = running.stderr.take().unwrap();
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        out.take(4097).read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        err.take(4097).read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if running.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let cancelled = running.cancel();
            let _ = stdout.join();
            let _ = stderr.join();
            panic!("real probe exceeded the harness deadline; cleanup: {cancelled:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let result = running.finish();
    let stdout = stdout.join().unwrap();
    let stderr = stderr.join().unwrap();
    assert!(
        stdout.len() <= 4096 && stderr.len() <= 4096,
        "fixed probe output exceeded its harness bound"
    );
    let result = result.unwrap();
    assert!(
        result.wrapper_status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert_eq!(result.metadata.exit_code, Some(0));
    assert!(
        !Path::new("/var/local/lib/isolate")
            .join(id.to_string())
            .exists()
    );
    let cg_root = fs::read_to_string("/run/isolate/cgroup").unwrap();
    assert!(!Path::new(cg_root.trim()).join(format!("box-{id}")).exists());
    println!("box {id}: {:?}", result.metadata);
    (String::from_utf8(stdout).unwrap(), result)
}

#[test]
#[ignore = "requires provisioned native Linux/Isolate; run explicitly on the supported host"]
fn real_restricted_execution_and_access_probes() {
    let config = config();
    // Reject unsupported host limits before staging or executing any submission.
    IsolateLimits::try_from(config.limits()).expect("host limits must be representable by Isolate");
    let backend = IsolateBackend::new(config.clone()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let problems = load_problems(root.join("problems")).unwrap();
    for (problem, file, position) in [
        (&problems[0], "sum_two_accepted.py", 1),
        (&problems[1], "sum_list_accepted.py", 4),
        (&problems[2], "balanced_brackets_accepted.py", 4),
    ] {
        let source = fs::read_to_string(root.join("submissions").join(file)).unwrap();
        let case = &problem.test_cases()[position];
        let (stdout, metadata) = run(&backend, &config, &source, case.input.as_bytes());
        assert_eq!(stdout, case.expected_output);
        assert!(metadata.metadata.cpu_time.is_some());
        assert!(metadata.metadata.wall_time.is_some());
        assert!(metadata.metadata.peak_memory_bytes.is_some());
    }
    for (file, expected) in [
        ("subprocess_probe.py", "SUBPROCESS_DENIED\n"),
        ("network_probe.py", "NETWORK_DENIED\n"),
    ] {
        let source = fs::read_to_string(root.join("submissions").join(file)).unwrap();
        assert_eq!(run(&backend, &config, &source, b"").0, expected);
    }
    let trusted = tempfile::tempdir().unwrap();
    let marker = trusted.path().join("host-only-marker");
    let expected = trusted.path().join("expected-output");
    fs::write(&marker, "HOST_ONLY_MARKER").unwrap();
    fs::write(&expected, "PRIVATE_EXPECTED_OUTPUT").unwrap();
    assert_eq!(fs::read_to_string(&marker).unwrap(), "HOST_ONLY_MARKER");
    assert_eq!(
        fs::read_to_string(&expected).unwrap(),
        "PRIVATE_EXPECTED_OUTPUT"
    );
    let source = format!(
        r#"import os
assert os.listdir('/box') == ['main.py']
for path in ({marker}, {expected}, '/etc/passwd', '/proc/self/environ', '/run/docker.sock'):
    try:
        open(path, 'rb').close()
    except (FileNotFoundError, PermissionError):
        pass
    else:
        raise AssertionError('unexpected host access')
for path in ('/box/leak', '/usr/leak', '/lib64/leak'):
    try:
        open(path, 'wb').close()
    except OSError:
        pass
    else:
        raise AssertionError('unexpected write access')
print('FILESYSTEM_DENIED')
"#,
        marker = serde_json::to_string(marker.to_str().unwrap()).unwrap(),
        expected = serde_json::to_string(expected.to_str().unwrap()).unwrap()
    );
    assert_eq!(
        run(&backend, &config, &source, b"").0,
        "FILESYSTEM_DENIED\n"
    );
    assert_eq!(
        run(
            &backend,
            &config,
            "import os\nassert os.listdir('/box') == ['main.py']\nprint('FRESH')\n",
            b""
        )
        .0,
        "FRESH\n"
    );
}
