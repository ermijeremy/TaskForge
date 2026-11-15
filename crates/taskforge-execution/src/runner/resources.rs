use crate::sandbox::SandboxMetadata;
use crate::{InfrastructureError, InfrastructureReason, ResourceUsage};
use std::time::Duration;

pub(super) struct Resources {
    cpu: Option<Duration>,
    wall: Option<Duration>,
    memory: Option<u64>,
}
impl Default for Resources {
    fn default() -> Self {
        Self {
            cpu: Some(Duration::ZERO),
            wall: Some(Duration::ZERO),
            memory: Some(0),
        }
    }
}
impl Resources {
    pub fn record(
        &mut self,
        metadata: Option<&SandboxMetadata>,
    ) -> Result<(), InfrastructureError> {
        self.cpu = add(self.cpu, metadata.and_then(|m| m.cpu_time))?;
        self.wall = add(self.wall, metadata.and_then(|m| m.wall_time))?;
        self.memory = self
            .memory
            .zip(metadata.and_then(|m| m.peak_memory_bytes))
            .map(|(a, b)| a.max(b));
        Ok(())
    }
    pub fn finish(self) -> Result<ResourceUsage, InfrastructureError> {
        Ok(ResourceUsage {
            cpu_time_ms: milliseconds(self.cpu)?,
            wall_time_ms: milliseconds(self.wall)?,
            peak_memory_bytes: self.memory,
        })
    }
}
fn add(a: Option<Duration>, b: Option<Duration>) -> Result<Option<Duration>, InfrastructureError> {
    a.zip(b)
        .map(|(a, b)| a.checked_add(b).ok_or_else(overflow))
        .transpose()
}
fn milliseconds(value: Option<Duration>) -> Result<Option<u64>, InfrastructureError> {
    value
        .map(|value| u64::try_from(value.as_millis()).map_err(|_| overflow()))
        .transpose()
}
fn overflow() -> InfrastructureError {
    InfrastructureError::new(
        InfrastructureReason::MetadataInvalid,
        "execution measurements exceed the representable aggregate range",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durations_are_summed_before_rounding_and_memory_uses_peak() {
        let mut usage = Resources::default();
        for memory in [9, 12, 0] {
            let text = format!("exitcode:0\ntime:0.0007\ntime-wall:0.002\ncg-mem:{memory}\n");
            usage
                .record(Some(&SandboxMetadata::parse(text.as_bytes()).unwrap()))
                .unwrap();
        }
        assert_eq!(
            usage.finish().unwrap(),
            ResourceUsage {
                cpu_time_ms: Some(2),
                wall_time_ms: Some(6),
                peak_memory_bytes: Some(12 * 1024)
            }
        );
    }
    #[test]
    fn missing_fields_remain_unknown_and_zero_remains_measured() {
        let mut usage = Resources::default();
        usage
            .record(Some(
                &SandboxMetadata::parse(b"exitcode:0\ntime:0\ntime-wall:0\ncg-mem:0\n").unwrap(),
            ))
            .unwrap();
        usage
            .record(Some(
                &SandboxMetadata::parse(b"exitcode:0\ntime:0\n").unwrap(),
            ))
            .unwrap();
        assert_eq!(
            usage.finish().unwrap(),
            ResourceUsage {
                cpu_time_ms: Some(0),
                wall_time_ms: None,
                peak_memory_bytes: None
            }
        );
        let mut usage = Resources::default();
        usage.record(None).unwrap();
        usage
            .record(Some(
                &SandboxMetadata::parse(b"exitcode:0\ntime:1\n").unwrap(),
            ))
            .unwrap();
        assert_eq!(usage.finish().unwrap(), ResourceUsage::default());
    }
    #[test]
    fn oversized_measurements_are_infrastructure_errors_instead_of_wrapping() {
        let mut usage = Resources::default();
        let meta = SandboxMetadata::parse(b"exitcode:0\ntime:18446744073709551615\n").unwrap();
        usage.record(Some(&meta)).unwrap();
        assert_eq!(
            usage.finish().unwrap_err().reason_code(),
            InfrastructureReason::MetadataInvalid
        );
        assert!(add(Some(Duration::MAX), Some(Duration::from_secs(1))).is_err());
    }
}
