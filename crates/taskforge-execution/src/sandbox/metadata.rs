use std::collections::BTreeMap;
use std::time::Duration;

use crate::{InfrastructureError, InfrastructureReason};

pub const MAX_METADATA_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxStatus {
    RuntimeError,
    Signal,
    Timeout,
    InternalError,
}

/// Raw sandbox evidence. Verdict classification belongs to issue #8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxMetadata {
    pub status: Option<SandboxStatus>,
    pub exit_code: Option<u32>,
    pub exit_signal: Option<u32>,
    pub cpu_time: Option<Duration>,
    pub wall_time: Option<Duration>,
    pub peak_memory_bytes: Option<u64>,
    pub oom_killed: bool,
    pub killed: bool,
}

impl SandboxMetadata {
    pub fn parse(bytes: &[u8]) -> Result<Self, InfrastructureError> {
        if bytes.is_empty() || bytes.len() > MAX_METADATA_BYTES {
            return Err(invalid());
        }
        let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let (key, value) = line.split_once(':').ok_or_else(invalid)?;
            if key.is_empty() || values.insert(key, value).is_some() {
                return Err(invalid());
            }
        }
        let status = values
            .get("status")
            .map(|value| match *value {
                "RE" => Ok(SandboxStatus::RuntimeError),
                "SG" => Ok(SandboxStatus::Signal),
                "TO" => Ok(SandboxStatus::Timeout),
                "XX" => Ok(SandboxStatus::InternalError),
                _ => Err(invalid()),
            })
            .transpose()?;
        let exit_code = integer(&values, "exitcode")?;
        let exit_signal = integer(&values, "exitsig")?;
        if (status.is_none() && exit_code.is_none() && exit_signal.is_none())
            || (exit_code.is_some() && exit_signal.is_some())
        {
            return Err(invalid());
        }
        let memory = values
            .get("cg-mem")
            .or_else(|| values.get("max-rss"))
            .map(|value| {
                value
                    .parse::<u64>()
                    .ok()
                    .and_then(|v| v.checked_mul(1024))
                    .ok_or_else(invalid)
            })
            .transpose()?;
        Ok(Self {
            status,
            exit_code,
            exit_signal,
            cpu_time: values.get("time").map(|value| seconds(value)).transpose()?,
            wall_time: values
                .get("time-wall")
                .map(|value| seconds(value))
                .transpose()?,
            peak_memory_bytes: memory,
            oom_killed: flag(&values, "cg-oom-killed")?,
            killed: flag(&values, "killed")?,
        })
    }
}

fn integer(values: &BTreeMap<&str, &str>, key: &str) -> Result<Option<u32>, InfrastructureError> {
    values
        .get(key)
        .map(|value| value.parse().map_err(|_| invalid()))
        .transpose()
}

fn flag(values: &BTreeMap<&str, &str>, key: &str) -> Result<bool, InfrastructureError> {
    match values.get(key) {
        None | Some(&"0") => Ok(false),
        Some(&"1") => Ok(true),
        _ => Err(invalid()),
    }
}

fn seconds(value: &str) -> Result<Duration, InfrastructureError> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 9
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let seconds: u64 = whole.parse().map_err(|_| invalid())?;
    let nanos = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u32>().map_err(|_| invalid())? * 10_u32.pow(9 - fraction.len() as u32)
    };
    Ok(Duration::new(seconds, nanos))
}

fn invalid() -> InfrastructureError {
    super::error(
        InfrastructureReason::MetadataInvalid,
        "missing, oversized, ambiguous or corrupt sandbox metadata",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_preserves_optional_values_zero_and_confirmed_memory_evidence() {
        let meta = SandboxMetadata::parse(b"exitcode:0\ntime:0.000\ntime-wall:0.001\ncg-mem:12\n")
            .unwrap();
        assert_eq!(meta.cpu_time, Some(Duration::ZERO));
        assert_eq!(meta.wall_time, Some(Duration::from_millis(1)));
        assert_eq!(meta.peak_memory_bytes, Some(12 * 1024));
        let meta = SandboxMetadata::parse(b"status:SG\nexitsig:9\ncg-oom-killed:1\n").unwrap();
        assert!(meta.oom_killed);
        assert_eq!(meta.cpu_time, None);
        assert_eq!(meta.peak_memory_bytes, None);
    }
    #[test]
    fn metadata_rejects_duplicates_invalid_numbers_and_missing_outcomes() {
        for text in [
            "",
            "time:1\n",
            "exitcode:0\nexitcode:1\n",
            "exitcode:0\nexitsig:9\n",
            "exitcode:0\ntime:NaN\n",
            "exitcode:0\ntime:-1\n",
            "status:BAD\n",
            "status:TO\nkilled:2\n",
            "exitcode:0\ncg-mem:18446744073709551615\n",
        ] {
            assert_eq!(
                SandboxMetadata::parse(text.as_bytes())
                    .unwrap_err()
                    .reason_code(),
                InfrastructureReason::MetadataInvalid
            );
        }
        assert!(SandboxMetadata::parse(&vec![b'x'; MAX_METADATA_BYTES + 1]).is_err());
    }
}
