use thiserror::Error;

use crate::ExecutionLimits;

/// Isolate 2.7 stores time in signed integer milliseconds and accepts memory
/// as a positive signed integer number of KiB. Never clamp or wrap a limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolateLimits {
    pub(super) cpu_seconds: String,
    pub(super) wall_seconds: String,
    pub(super) memory_kib: u32,
}

impl IsolateLimits {
    pub fn cpu_seconds(&self) -> &str {
        &self.cpu_seconds
    }
    pub fn wall_seconds(&self) -> &str {
        &self.wall_seconds
    }
    pub fn memory_kib(&self) -> u32 {
        self.memory_kib
    }

    pub fn arguments(&self) -> Vec<String> {
        vec![
            format!("--time={}", self.cpu_seconds),
            format!("--wall-time={}", self.wall_seconds),
            format!("--cg-mem={}", self.memory_kib),
            "--processes=1".into(),
        ]
    }
}

impl TryFrom<&ExecutionLimits> for IsolateLimits {
    type Error = LimitConversionError;

    fn try_from(limits: &ExecutionLimits) -> Result<Self, Self::Error> {
        if limits.processes().get() != 1 {
            return Err(LimitConversionError::UnsupportedProcessCount);
        }
        if limits.cpu_time().as_millis() > i32::MAX as u128 {
            return Err(LimitConversionError::TimeOutOfRange {
                field: "cpu_time_ms",
            });
        }
        if limits.wall_time().as_millis() > i32::MAX as u128 {
            return Err(LimitConversionError::TimeOutOfRange {
                field: "wall_time_ms",
            });
        }
        if limits.memory_kib_ceil() > i32::MAX as u64 {
            return Err(LimitConversionError::MemoryOutOfRange);
        }
        let cpu_seconds = format!(
            "{}.{:03}",
            limits.cpu_time().as_millis() / 1000,
            limits.cpu_time().as_millis() % 1000
        );
        let wall_seconds = format!(
            "{}.{:03}",
            limits.wall_time().as_millis() / 1000,
            limits.wall_time().as_millis() % 1000
        );
        let memory_kib = limits.memory_kib_ceil() as u32;
        Ok(Self {
            cpu_seconds,
            wall_seconds,
            memory_kib,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LimitConversionError {
    #[error("{field} must fit Isolate's positive signed 32-bit millisecond range")]
    TimeOutOfRange { field: &'static str },
    #[error("memory rounded to KiB must fit Isolate's positive signed 32-bit range")]
    MemoryOutOfRange,
    #[error("Phase 1 permits exactly one submitted process")]
    UnsupportedProcessCount,
}
