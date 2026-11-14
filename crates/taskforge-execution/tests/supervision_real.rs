#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
//! Explicit native-host evidence; normal regression tests do not verify isolation.
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use taskforge_execution::sandbox::SandboxStatus;
use taskforge_execution::supervision::{OutputBudget, OutputStream, Supervisor};
use taskforge_execution::{
    ExecutionConfig, ExecutionLimits, ExecutionSettings, InfrastructureReason, LimitSettings,
};

fn config(timeout_ms: u64) -> ExecutionConfig {
    let path = fs::canonicalize(
        std::env::var_os("TASKFORGE_REAL_CONFIG")
            .expect("set TASKFORGE_REAL_CONFIG to the provisioned native-host configuration"),
    )
    .unwrap();
    let mut settings: ExecutionSettings =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    settings.sandbox.box_id_end = settings.sandbox.box_id_start;
    settings.evaluation_timeout_ms = timeout_ms;
    settings.validate(path.parent().unwrap()).unwrap()
}

fn assert_clean(config: &ExecutionConfig) {
    let id = config.sandbox().box_id_start;
    assert!(
        !Path::new("/var/local/lib/isolate")
            .join(id.to_string())
            .exists(),
        "sandbox directory survived cleanup"
    );
    let root = fs::read_to_string("/run/isolate/cgroup").unwrap();
    assert!(
        !Path::new(root.trim()).join(format!("box-{id}")).exists(),
        "sandbox cgroup survived cleanup"
    );
}

fn limits(cpu_ms: u64, wall_ms: u64) -> ExecutionLimits {
    ExecutionLimits::try_from(LimitSettings {
        cpu_time_ms: cpu_ms,
        wall_time_ms: wall_ms,
        output_bytes: 40_000,
        ..LimitSettings::default()
    })
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires provisioned native Linux/Isolate; run explicitly on the supported host"]
async fn real_supervision_limits_deadline_cancellation_and_slot_reuse() {
    // Validate accounting initialization before host setup.
    OutputBudget::new(ExecutionLimits::default().output_bytes())
        .record(OutputStream::Stdout, b"")
        .expect("output accounting initialization failed");
    let config = config(10_000);
    let supervisor = Supervisor::initialize(config.clone()).await.unwrap();
    let normal = limits(200, 1_500);
    let session = supervisor.evaluation().unwrap();
    let deadline = session.deadline();
    for _ in 0..2 {
        let result = session
            .run_test(
                "import sys\nprint(sys.stdin.read().strip())\n",
                b"OK\n",
                &normal,
            )
            .await
            .unwrap();
        assert_eq!(result.output.stdout(), b"OK\n");
        assert!(!result.output_limit_exceeded);
        assert_eq!(result.metadata.unwrap().exit_code, Some(0));
        assert_eq!(session.deadline(), deadline);
        assert_clean(&config);
    }
    for source in [
        "import os\nwhile True: os.write(1, b'x' * 8192)\n",
        "import os\nwhile True: os.write(2, b'x' * 8192)\n",
        "import os\nos.write(1, b'x' * 32768)\nos.write(2, b'y' * 32768)\n",
    ] {
        let result = supervisor
            .evaluation()
            .unwrap()
            .run_test(source, b"", &normal)
            .await
            .unwrap();
        assert!(result.output_limit_exceeded);
        assert_eq!(result.output.retained_bytes(), 40_000);
        assert!(result.output.observed_bytes() > 40_000);
        assert!(result.metadata.is_none());
        assert_clean(&config);
    }
    let cpu = supervisor
        .evaluation()
        .unwrap()
        .run_test("while True: pass\n", b"", &normal)
        .await
        .unwrap();
    let cpu = cpu.metadata.unwrap();
    assert_eq!(cpu.status, Some(SandboxStatus::Timeout));
    assert!(cpu.cpu_time.unwrap() >= Duration::from_millis(150));
    assert_clean(&config);
    let wall = supervisor
        .evaluation()
        .unwrap()
        .run_test("import time\ntime.sleep(60)\n", b"", &limits(1_000, 300))
        .await
        .unwrap();
    let wall = wall.metadata.unwrap();
    assert_eq!(wall.status, Some(SandboxStatus::Timeout));
    assert!(wall.wall_time.unwrap() >= Duration::from_millis(250));
    assert!(wall.cpu_time.unwrap() < Duration::from_millis(250));
    assert_clean(&config);

    for drop_waiter in [false, true] {
        let session = supervisor.evaluation().unwrap();
        let task = session
            .start_test("import time\ntime.sleep(60)\n", b"", &limits(5_000, 5_000))
            .unwrap();
        // Allow actual sandbox execution before requesting cancellation.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!session.is_idle());
        if drop_waiter {
            let waiter = tokio::spawn(task.wait());
            tokio::task::yield_now().await;
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
        } else {
            task.cancel();
            assert_eq!(
                task.wait().await.unwrap_err().reason_code(),
                InfrastructureReason::Cancelled
            );
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while !session.is_idle() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_clean(&config);
        supervisor
            .evaluation()
            .unwrap()
            .run_test("print('REUSED')\n", b"", &normal)
            .await
            .unwrap();
        assert_clean(&config);
    }
    // A separate overall budget, shorter than either sandbox time limit.
    let short_config = self::config(1_200);
    let short = Supervisor::initialize(short_config.clone()).await.unwrap();
    let session = short.evaluation().unwrap();
    let began = Instant::now();
    let error = session
        .run_test("while True: pass\n", b"", &limits(5_000, 5_000))
        .await
        .unwrap_err();
    assert_eq!(
        error.reason_code(),
        InfrastructureReason::EvaluationDeadlineExceeded
    );
    assert!(began.elapsed() < Duration::from_millis(1_450));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !session.is_idle() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_clean(&short_config);
    short
        .evaluation()
        .unwrap()
        .run_test("print('REUSED')\n", b"", &normal)
        .await
        .unwrap();
    assert_clean(&short_config);
    println!(
        "verified bounded streams, CPU/wall evidence, overall deadline, cancellation and slot reuse"
    );
}
