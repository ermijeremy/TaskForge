//! Configuration and resource limits for TaskForge's future execution runner.
//!
//! This crate does not execute submitted code yet. Sandbox readiness and actual
//! execution belong to later Phase 1 issues.

pub mod config;
pub mod limits;

pub use config::{ConfigError, ExecutionConfig, ExecutionSettings, RuntimeSettings};
pub use limits::{ExecutionLimits, LimitSettings};
