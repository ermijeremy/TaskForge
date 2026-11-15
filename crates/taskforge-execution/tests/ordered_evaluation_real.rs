#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
//! Full runner acceptance probes, explicitly opt-in on the supported native host.
use std::fs;
use std::path::{Path, PathBuf};
use taskforge_execution::runner::{ExecutionDecision, classify_execution};
use taskforge_execution::sandbox::SandboxMetadata;
use taskforge_execution::{
    EvaluationReason, ExecutionConfig, ExecutionSettings, IsolateRunner, Runner, Verdict,
    load_problems,
};

fn config() -> ExecutionConfig {
    let path = fs::canonicalize(
        std::env::var_os("TASKFORGE_REAL_CONFIG")
            .expect("set TASKFORGE_REAL_CONFIG to the native host configuration"),
    )
    .unwrap();
    let mut settings: ExecutionSettings =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    settings.sandbox.box_id_end = settings.sandbox.box_id_start;
    settings.validate(path.parent().unwrap()).unwrap()
}
fn assert_clean(config: &ExecutionConfig) {
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
#[test]
#[ignore = "requires the supported native Linux/Isolate host; run explicitly"]
fn real_ordered_evaluation_accepts_fixtures_and_classifies_failures() {
    let success = SandboxMetadata::parse(b"exitcode:0\n").unwrap();
    assert_eq!(
        classify_execution(false, Some(&success)).expect("classifier readiness probe failed"),
        ExecutionDecision::CompareOutput
    );
    let config = config();
    let runner = IsolateRunner::new(config.clone()).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let problems = load_problems(root.join("problems")).unwrap();
    let source = |name: &str| fs::read_to_string(root.join("submissions").join(name)).unwrap();
    for (problem, name) in problems.iter().zip([
        "sum_two_accepted.py",
        "sum_list_accepted.py",
        "balanced_brackets_accepted.py",
    ]) {
        let result = runner
            .evaluate(&source(name), problem, problem.limits())
            .unwrap();
        assert_eq!(result.fields().verdict, Verdict::Accepted);
        assert_eq!(
            result.fields().passed_tests,
            problem.test_cases().len() as u32
        );
        assert_eq!(result.fields().first_failing_test, None);
        assert!(result.fields().resources.cpu_time_ms.is_some());
        assert!(result.fields().resources.wall_time_ms.is_some());
        assert!(result.fields().resources.peak_memory_bytes.is_some());
        assert_clean(&config);
    }
    let problem = &problems[0];
    let wrong = runner
        .evaluate(&source("sum_two_wrong.py"), problem, problem.limits())
        .unwrap();
    assert_eq!(wrong.fields().verdict, Verdict::WrongAnswer);
    assert_eq!(wrong.fields().passed_tests, 1);
    assert_eq!(wrong.fields().first_failing_test, Some(1));
    assert_clean(&config);
    assert_eq!(
        runner
            .evaluate(&source("sum_two_whitespace.py"), problem, problem.limits())
            .unwrap()
            .fields()
            .verdict,
        Verdict::Accepted
    );
    assert_clean(&config);
    for (name, verdict, reason) in [
        (
            "syntax_error.py",
            Verdict::RuntimeError,
            EvaluationReason::ProgramFailed,
        ),
        (
            "exception.py",
            Verdict::RuntimeError,
            EvaluationReason::ProgramFailed,
        ),
        (
            "infinite_loop.py",
            Verdict::TimeLimitExceeded,
            EvaluationReason::CpuTimeLimitExceeded,
        ),
        (
            "sleeping.py",
            Verdict::TimeLimitExceeded,
            EvaluationReason::WallTimeLimitExceeded,
        ),
        (
            "stdout_flood.py",
            Verdict::OutputLimitExceeded,
            EvaluationReason::CombinedOutputLimitExceeded,
        ),
        (
            "stderr_flood.py",
            Verdict::OutputLimitExceeded,
            EvaluationReason::CombinedOutputLimitExceeded,
        ),
        (
            "mixed_output_flood.py",
            Verdict::OutputLimitExceeded,
            EvaluationReason::CombinedOutputLimitExceeded,
        ),
    ] {
        let result = runner
            .evaluate(&source(name), problem, problem.limits())
            .unwrap();
        assert_eq!(result.fields().verdict, verdict, "fixture {name}");
        assert_eq!(result.fields().reason_code, reason);
        assert_eq!(result.fields().passed_tests, 0);
        assert_eq!(result.fields().first_failing_test, Some(0));
        if verdict == Verdict::OutputLimitExceeded {
            assert_eq!(
                result.fields().resources,
                taskforge_execution::ResourceUsage::default()
            );
        }
        assert!(!result.fields().diagnostic.as_str().contains("Traceback"));
        assert_clean(&config);
    }
    let memory = runner
        .evaluate(&source("memory_pressure.py"), problem, problem.limits())
        .unwrap();
    // Python may raise MemoryError instead of the cgroup recording an OOM kill.
    // The pure exercise tests verify that only explicit OOM evidence selects MLE.
    assert!(matches!(
        memory.fields().verdict,
        Verdict::MemoryLimitExceeded | Verdict::RuntimeError
    ));
    let memory_reason = memory.fields().reason_code;
    match memory.fields().verdict {
        Verdict::MemoryLimitExceeded => {
            assert_eq!(memory_reason, EvaluationReason::ConfirmedMemoryLimitKill)
        }
        Verdict::RuntimeError => assert_eq!(memory_reason, EvaluationReason::ProgramFailed),
        _ => unreachable!(),
    }
    println!(
        "memory probe: {:?}, reason {:?}",
        memory.fields().verdict,
        memory_reason
    );
    assert_clean(&config);
    let reused = runner
        .evaluate(&source("sum_two_accepted.py"), problem, problem.limits())
        .unwrap();
    assert_eq!(reused.fields().verdict, Verdict::Accepted);
    assert_clean(&config);
    println!("verified ordered fixture evaluation, first failure, verdicts and clean slot reuse");
}
