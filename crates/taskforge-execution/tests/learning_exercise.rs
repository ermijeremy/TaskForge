//! These tests are deliberately opt-in until you finish the issue #2 exercise.

use taskforge_execution::{ExecutionLimits, LimitSettings};

fn limits_with_memory(bytes: u64) -> ExecutionLimits {
    ExecutionLimits::try_from(LimitSettings {
        memory_bytes: bytes,
        ..LimitSettings::default()
    })
    .unwrap()
}

#[test]
#[ignore = "Implement memory_kib_ceil; see docs/learning/issue-2.md"]
fn memory_conversion_rounds_up_partial_kibibytes() {
    for (bytes, expected_kib) in [(1, 1), (1023, 1), (1024, 1), (1025, 2), (2048, 2)] {
        assert_eq!(
            limits_with_memory(bytes).memory_kib_ceil(),
            expected_kib,
            "wrong conversion for {bytes} bytes"
        );
    }
    assert_eq!(ExecutionLimits::default().memory_kib_ceil(), 131_072);
}

#[test]
#[ignore = "Implement memory_kib_ceil; see docs/learning/issue-2.md"]
fn memory_conversion_handles_the_largest_u64_without_overflow() {
    assert_eq!(
        limits_with_memory(u64::MAX).memory_kib_ceil(),
        18_014_398_509_481_984
    );
}
