//! Configuration, readiness, problems, evaluation contracts, and sandbox lifecycle.
//!
//! Readiness executes a trusted probe. Per-test execution uses validated Isolate
//! limits and restricted runtime staging; bounded stream supervision is later work.

// make the two module(files) available/expose
pub mod config; // -> config.rs
pub mod evaluation;
pub mod limits; // -> limits.rs
pub mod problem;
pub mod readiness;
pub mod sandbox;

// re-export commonly used types for easier access
pub use config::{
    ConfigError, ExecutionConfig, ExecutionSettings, RuntimeSettings, SandboxSettings,
};
pub use evaluation::{
    BoundedDiagnostic, EvaluationOutcome, EvaluationReason, EvaluationResult,
    EvaluationResultFields, InfrastructureError, InfrastructureReason, MAX_DIAGNOSTIC_BYTES,
    ResourceUsage, ResultValidationError, Runner, Verdict,
};
pub use limits::{ExecutionLimits, LimitSettings};
pub use problem::{
    Problem, ProblemError, ProblemLimits, ProblemRuntime, PublicExample, PublicProblem, TestCase,
    load_problems,
};
pub use readiness::{ReadinessError, ReadinessReport, check_readiness};
