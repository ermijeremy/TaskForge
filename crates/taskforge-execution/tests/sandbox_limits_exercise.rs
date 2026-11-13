//! Regressions for the completed issue #6 limit-conversion exercise.
use taskforge_execution::sandbox::{IsolateLimits, LimitConversionError};
use taskforge_execution::{ExecutionLimits, LimitSettings};

fn convert(settings: LimitSettings) -> Result<IsolateLimits, LimitConversionError> {
    IsolateLimits::try_from(&ExecutionLimits::try_from(settings).unwrap())
}

#[test]
fn defaults_map_to_explicit_isolate_units() {
    let result = convert(LimitSettings::default()).unwrap();
    assert_eq!(result.cpu_seconds(), "1.000");
    assert_eq!(result.wall_seconds(), "3.000");
    assert_eq!(result.memory_kib(), 131072);
    assert_eq!(
        result.arguments(),
        [
            "--time=1.000",
            "--wall-time=3.000",
            "--cg-mem=131072",
            "--processes=1"
        ]
    );
}

#[test]
fn fractional_seconds_preserve_milliseconds_exactly() {
    let result = convert(LimitSettings {
        cpu_time_ms: 1,
        wall_time_ms: 2501,
        ..LimitSettings::default()
    })
    .unwrap();
    assert_eq!(result.cpu_seconds(), "0.001");
    assert_eq!(result.wall_seconds(), "2.501");
}

#[test]
fn memory_rounds_up_without_overflow() {
    for (bytes, kib) in [(1, 1), (1024, 1), (1025, 2)] {
        assert_eq!(
            convert(LimitSettings {
                memory_bytes: bytes,
                ..LimitSettings::default()
            })
            .unwrap()
            .memory_kib(),
            kib
        );
    }
    assert_eq!(
        convert(LimitSettings {
            memory_bytes: u64::MAX,
            ..LimitSettings::default()
        })
        .unwrap_err(),
        LimitConversionError::MemoryOutOfRange
    );
}

#[test]
fn maximum_representable_time_and_memory_are_accepted() {
    let result = convert(LimitSettings {
        cpu_time_ms: i32::MAX as u64,
        wall_time_ms: i32::MAX as u64,
        memory_bytes: i32::MAX as u64 * 1024,
        ..LimitSettings::default()
    })
    .unwrap();
    assert_eq!(result.cpu_seconds(), "2147483.647");
    assert_eq!(result.wall_seconds(), "2147483.647");
    assert_eq!(result.memory_kib(), i32::MAX as u32);
}

#[test]
fn out_of_range_limits_are_rejected_instead_of_clamped_or_wrapped() {
    for value in [i32::MAX as u64 + 1, u64::MAX] {
        assert_eq!(
            convert(LimitSettings {
                cpu_time_ms: value,
                ..LimitSettings::default()
            })
            .unwrap_err(),
            LimitConversionError::TimeOutOfRange {
                field: "cpu_time_ms"
            }
        );
        assert_eq!(
            convert(LimitSettings {
                wall_time_ms: value,
                ..LimitSettings::default()
            })
            .unwrap_err(),
            LimitConversionError::TimeOutOfRange {
                field: "wall_time_ms"
            }
        );
    }
    assert_eq!(
        convert(LimitSettings {
            memory_bytes: i32::MAX as u64 * 1024 + 1,
            ..LimitSettings::default()
        })
        .unwrap_err(),
        LimitConversionError::MemoryOutOfRange
    );
}

#[test]
fn multiple_processes_are_rejected_and_stream_output_is_not_a_file_limit() {
    assert_eq!(
        convert(LimitSettings {
            processes: 2,
            ..LimitSettings::default()
        })
        .unwrap_err(),
        LimitConversionError::UnsupportedProcessCount
    );
    let result = convert(LimitSettings {
        output_bytes: 17,
        ..LimitSettings::default()
    })
    .unwrap();
    assert_eq!(result.arguments().len(), 4);
    assert!(
        !result
            .arguments()
            .iter()
            .any(|arg| arg.starts_with("--fsize"))
    );
}
