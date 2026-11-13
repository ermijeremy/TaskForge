use std::path::PathBuf;

use taskforge_execution::{
    EvaluationOutcome, ExecutionLimits, InfrastructureError, InfrastructureReason, Problem, Runner,
};

// Contract test double only. No source is executed and no CLI/process is used.
struct UnavailableRunner;

impl Runner for UnavailableRunner {
    fn evaluate(
        &self,
        _source: &str,
        _problem: &Problem,
        _limits: &ExecutionLimits,
    ) -> EvaluationOutcome {
        Err(InfrastructureError::new(
            InfrastructureReason::SandboxUnavailable,
            "Sandbox prerequisites are unavailable.",
        ))
    }
}

#[test]
fn a_worker_can_call_the_library_and_match_typed_infrastructure_failure() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/problems/sum-two-integers-v1.json");
    let problem = Problem::load(path).unwrap();
    let runner: &dyn Runner = &UnavailableRunner;
    match runner.evaluate("print(0)", &problem, problem.limits()) {
        Err(error) => {
            assert_eq!(
                error.reason_code(),
                InfrastructureReason::SandboxUnavailable
            );
            let serialized = serde_json::to_value(error).unwrap();
            assert_eq!(serialized["reason_code"], "sandbox_unavailable");
            assert!(serialized.get("verdict").is_none());
        }
        Ok(_) => panic!("unavailable sandbox must not become a user-code verdict"),
    }
}
