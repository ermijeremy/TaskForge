use std::num::NonZeroU64;
use thiserror::Error;

/// Absolute retained-data ceiling, shared by stdout and stderr.
pub const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedOutput {
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) observed_bytes: u64,
}
impl CapturedOutput {
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
    /// Saturating count of bytes actually read, not a claim about unread output.
    pub fn observed_bytes(&self) -> u64 {
        self.observed_bytes
    }
    pub fn retained_bytes(&self) -> usize {
        self.stdout.len() + self.stderr.len()
    }
}

#[derive(Debug)]
pub struct OutputBudget {
    limit: NonZeroU64,
    capture: CapturedOutput,
    exceeded: bool,
}
impl OutputBudget {
    pub fn new(limit: NonZeroU64) -> Self {
        Self {
            limit,
            capture: CapturedOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                observed_bytes: 0,
            },
            exceeded: false,
        }
    }
    pub fn limit(&self) -> u64 {
        self.limit.get()
    }
    pub fn capture(&self) -> &CapturedOutput {
        &self.capture
    }
    pub fn exceeded(&self) -> bool {
        self.exceeded
    }
    pub fn into_capture(self) -> CapturedOutput {
        self.capture
    }

    pub fn record(
        &mut self,
        stream: OutputStream,
        chunk: &[u8],
    ) -> Result<(), OutputAccountingError> {
        if chunk.is_empty() {
            return Ok(());
        }
        let already_exceeded = self.exceeded;
        let total = u64::try_from(chunk.len())
            .ok()
            .and_then(|size| self.capture.observed_bytes.checked_add(size));
        self.capture.observed_bytes = total.unwrap_or(u64::MAX);
        self.exceeded |= total.is_none_or(|count| count > self.limit());

        if already_exceeded {
            return Ok(());
        }
        // Bound in u64 before narrowing, even for a configured u64::MAX limit.
        let ceiling = self.limit().min(MAX_CAPTURE_BYTES as u64) as usize;
        let remaining = ceiling.saturating_sub(self.capture.retained_bytes());
        let keep = chunk.len().min(remaining);
        match stream {
            OutputStream::Stdout => self.capture.stdout.extend_from_slice(&chunk[..keep]),
            OutputStream::Stderr => self.capture.stderr.extend_from_slice(&chunk[..keep]),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OutputAccountingError {
    #[error(
        "complete the issue #7 combined-output accounting exercise before supervising a submission"
    )]
    ExercisePending,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn count_overflow_records_excess_instead_of_wrapping() {
        let mut budget = OutputBudget::new(NonZeroU64::new(u64::MAX).unwrap());
        budget.capture.stdout = vec![b'x'; MAX_CAPTURE_BYTES];
        budget.capture.observed_bytes = u64::MAX;
        budget.record(OutputStream::Stderr, b"x").unwrap();
        assert!(budget.exceeded());
        assert_eq!(budget.capture().observed_bytes(), u64::MAX);
        assert_eq!(budget.capture().retained_bytes(), MAX_CAPTURE_BYTES);
    }
}
