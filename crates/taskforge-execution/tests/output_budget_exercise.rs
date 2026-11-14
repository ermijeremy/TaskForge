//! Regression tests for the completed issue #7 output-accounting exercise.
use std::num::NonZeroU64;
use taskforge_execution::supervision::{MAX_CAPTURE_BYTES, OutputBudget, OutputStream::*};

fn budget(limit: u64) -> OutputBudget {
    OutputBudget::new(NonZeroU64::new(limit).unwrap())
}

#[test]
fn empty_chunks_do_not_change_accounting() {
    let mut b = budget(1);
    b.record(Stdout, b"").unwrap();
    b.record(Stderr, b"").unwrap();
    assert_eq!(b.capture().observed_bytes(), 0);
    assert_eq!(b.capture().retained_bytes(), 0);
    assert!(!b.exceeded());
}

#[test]
fn stdout_and_stderr_share_one_budget_with_equality_allowed() {
    let mut b = budget(5);
    b.record(Stdout, b"abc").unwrap();
    b.record(Stderr, b"de").unwrap();
    assert!(!b.exceeded());
    assert_eq!(b.capture().stdout(), b"abc");
    assert_eq!(b.capture().stderr(), b"de");
    b.record(Stderr, b"f").unwrap();
    assert!(b.exceeded());
    assert_eq!(b.capture().observed_bytes(), 6);
    assert_eq!(b.capture().retained_bytes(), 5);
}

#[test]
fn crossing_chunk_retains_only_the_remaining_prefix() {
    let mut b = budget(5);
    b.record(Stderr, b"12").unwrap();
    b.record(Stdout, b"abcdef").unwrap();
    assert_eq!(b.capture().stderr(), b"12");
    assert_eq!(b.capture().stdout(), b"abc");
    assert_eq!(b.capture().observed_bytes(), 8);
    assert!(b.exceeded());
}

#[test]
fn excess_is_sticky_and_later_chunks_are_counted_without_retention() {
    let mut b = budget(2);
    b.record(Stderr, b"abcd").unwrap();
    b.record(Stdout, b"123").unwrap();
    b.record(Stderr, b"").unwrap();
    assert!(b.exceeded());
    assert_eq!(b.capture().observed_bytes(), 7);
    assert_eq!(b.capture().stderr(), b"ab");
    assert!(b.capture().stdout().is_empty());
}

#[test]
fn arbitrary_bytes_are_preserved_without_utf8_conversion() {
    let mut b = budget(4);
    b.record(Stdout, &[0, 255]).unwrap();
    b.record(Stderr, &[254, 0]).unwrap();
    assert_eq!(b.capture().stdout(), &[0, 255]);
    assert_eq!(b.capture().stderr(), &[254, 0]);
    assert!(!b.exceeded());
}

#[test]
fn huge_configured_limits_never_allow_huge_capture_allocations() {
    let mut b = budget(u64::MAX);
    let bytes = vec![42; MAX_CAPTURE_BYTES + 100];
    b.record(Stdout, &bytes).unwrap();
    b.record(Stderr, b"more").unwrap();
    assert_eq!(b.capture().retained_bytes(), MAX_CAPTURE_BYTES);
    assert_eq!(b.capture().observed_bytes(), bytes.len() as u64 + 4);
    assert!(!b.exceeded());
    assert!(b.capture().stderr().is_empty());
}

#[test]
fn chunk_boundaries_do_not_change_accounting_or_per_stream_prefixes() {
    let mut a = budget(7);
    let mut b = budget(7);
    a.record(Stdout, b"abc").unwrap();
    a.record(Stderr, b"defghi").unwrap();
    for byte in b"abc" {
        b.record(Stdout, &[*byte]).unwrap();
    }
    for byte in b"defghi" {
        b.record(Stderr, &[*byte]).unwrap();
    }
    assert_eq!(a.into_capture(), b.into_capture());
}
