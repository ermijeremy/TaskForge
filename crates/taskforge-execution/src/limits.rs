use std::num::{NonZeroU32, NonZeroU64};
use std::time::Duration;

use serde::Deserialize;

use crate::ConfigError;

/// Unvalidated configuration values. Units are explicit at the file boundary.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LimitSettings {
    pub cpu_time_ms: u64,
    pub wall_time_ms: u64,
    pub memory_bytes: u64,
    pub output_bytes: u64,
    pub processes: u32,
}

impl Default for LimitSettings {
    fn default() -> Self {
        Self {
            cpu_time_ms: 1_000,
            wall_time_ms: 3_000,
            memory_bytes: 128 * 1024 * 1024,
            output_bytes: 1024 * 1024,
            processes: 1,
        }
    }
}

/// Validated per-test limits. Private fields preserve the nonzero invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionLimits {
    cpu_time: Duration,
    wall_time: Duration,
    memory_bytes: NonZeroU64,
    output_bytes: NonZeroU64,
    processes: NonZeroU32,
}

impl TryFrom<LimitSettings> for ExecutionLimits {
    type Error = ConfigError;

    fn try_from(settings: LimitSettings) -> Result<Self, Self::Error> {
        Ok(Self {
            cpu_time: Duration::from_millis(
                positive_u64("limits.cpu_time_ms", settings.cpu_time_ms)?.get(),
            ),
            wall_time: Duration::from_millis(
                positive_u64("limits.wall_time_ms", settings.wall_time_ms)?.get(),
            ),
            memory_bytes: positive_u64("limits.memory_bytes", settings.memory_bytes)?,
            output_bytes: positive_u64("limits.output_bytes", settings.output_bytes)?,
            processes: NonZeroU32::new(settings.processes).ok_or(ConfigError::NonPositive {
                field: "limits.processes",
            })?,
        })
    }
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self::try_from(LimitSettings::default()).expect("built-in limits are positive")
    }
}

impl ExecutionLimits {
    pub fn cpu_time(&self) -> Duration {
        self.cpu_time
    }

    pub fn wall_time(&self) -> Duration {
        self.wall_time
    }

    pub fn memory_bytes(&self) -> NonZeroU64 {
        self.memory_bytes
    }

    pub fn output_bytes(&self) -> NonZeroU64 {
        self.output_bytes
    }

    pub fn processes(&self) -> NonZeroU32 {
        self.processes
    }

    /// Convert the memory limit to whole KiB, rounding upward.
    ///
    /// Learning exercise for issue #2: implement this method yourself.
    /// One KiB is 1,024 bytes; the conversion must not overflow for `u64::MAX`.
    /// See `docs/learning/issue-2.md` for the contract and test command.
    ///
    /// # Panics
    ///
    /// Currently panics because the exercise is intentionally unfinished.
    /// The configuration command does not call this method.
    pub fn memory_kib_ceil(&self) -> u64 {
        todo!("Your Rust exercise: convert memory_bytes to KiB, rounding up without overflow")
    }
}

pub(crate) fn positive_u64(field: &'static str, value: u64) -> Result<NonZeroU64, ConfigError> {
    NonZeroU64::new(value).ok_or(ConfigError::NonPositive { field })
}
