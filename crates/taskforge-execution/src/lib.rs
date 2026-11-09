//! Configuration, resource limits, and trusted problem fixtures for TaskForge.
//!
//! This crate does not execute submitted code yet. Sandbox readiness and actual
//! execution belong to later Phase 1 issues.

// make the two module(files) available/expose
pub mod config; // -> config.rs
pub mod limits; // -> limits.rs
pub mod problem;

// re-export commonly used types for easier access
pub use config::{ConfigError, ExecutionConfig, ExecutionSettings, RuntimeSettings};
pub use limits::{ExecutionLimits, LimitSettings};
pub use problem::{
    Problem, ProblemError, ProblemLimits, ProblemRuntime, PublicExample, PublicProblem, TestCase,
    load_problems,
};
