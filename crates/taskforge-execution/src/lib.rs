//! Configuration, resource limits, readiness, and trusted problem fixtures for TaskForge.
//!
//! Readiness executes only a built-in trusted smoke probe, never a submission.

// make the two module(files) available/expose
pub mod config; // -> config.rs
pub mod limits; // -> limits.rs
pub mod problem;
pub mod readiness;

// re-export commonly used types for easier access
pub use config::{
    ConfigError, ExecutionConfig, ExecutionSettings, RuntimeSettings, SandboxSettings,
};
pub use limits::{ExecutionLimits, LimitSettings};
pub use problem::{
    Problem, ProblemError, ProblemLimits, ProblemRuntime, PublicExample, PublicProblem, TestCase,
    load_problems,
};
pub use readiness::{ReadinessError, ReadinessReport, check_readiness};
