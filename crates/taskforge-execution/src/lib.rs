//! Configuration, readiness, fixtures, sandbox lifecycle, and bounded supervision.
//!
//! Readiness executes a trusted probe. Per-test execution uses validated Isolate
//! limits, restricted runtime staging and bounded combined-output supervision.

// make the two module(files) available/expose
pub mod config; // -> config.rs
pub mod evaluation;
mod execution_control;
pub mod limits; // -> limits.rs
pub mod problem;
pub mod readiness;
pub mod sandbox;
pub mod supervision;

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
