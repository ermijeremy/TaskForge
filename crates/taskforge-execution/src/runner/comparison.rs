/// Compare raw byte tokens separated by ASCII whitespace. No UTF-8 conversion,
/// numeric interpretation, tolerance, or allocation proportional to token count.
pub fn stdout_tokens_equal(actual: &[u8], expected: &[u8]) -> bool {
    let separator = |byte: &u8| matches!(*byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    actual
        .split(separator)
        .filter(|part| !part.is_empty())
        .eq(expected.split(separator).filter(|part| !part.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ascii_whitespace_and_empty_outputs_compare_by_tokens() {
        assert!(stdout_tokens_equal(b" \t5\r\n\x0b\x0c", b"5\n"));
        assert!(stdout_tokens_equal(b"", b" \t\r\n"));
        assert!(stdout_tokens_equal(b"a  b\n", b"a\tb"));
        assert!(!stdout_tokens_equal(b"", b"0"));
        assert!(!stdout_tokens_equal(b"1 2", b"12"));
        assert!(!stdout_tokens_equal(b"1 2", b"2 1"));
        assert!(!stdout_tokens_equal(b"1 2 3", b"1 2"));
    }
    #[test]
    fn comparison_is_exact_raw_bytes_without_numeric_or_unicode_normalization() {
        assert!(!stdout_tokens_equal(b"1.0", b"1"));
        assert!(!stdout_tokens_equal(b"05", b"5"));
        assert!(!stdout_tokens_equal(b"YES", b"yes"));
        assert!(!stdout_tokens_equal("a\u{a0}b".as_bytes(), b"a b"));
        assert!(stdout_tokens_equal(&[255, b' ', 0], &[255, b'\n', 0]));
        assert!(!stdout_tokens_equal(&[255], &[254]));
    }
}
