//! Configuration and resource limits for TaskForge's future execution runner.
//!
//! This crate does not execute submitted code yet. Sandbox readiness and actual
//! execution belong to later Phase 1 issues.

// make the two module(files) available/expose
pub mod config; // -> config.rs
pub mod limits; // -> limits.rs

// re-export commonly used types for easier access
pub use config::{ConfigError, ExecutionConfig, ExecutionSettings, RuntimeSettings};
pub use limits::{ExecutionLimits, LimitSettings};
