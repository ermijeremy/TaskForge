use taskforge_execution::EvaluationOutcome;
use thiserror::Error;

pub const EVALUATED_EXIT_CODE: u8 = 0;
pub const INFRASTRUCTURE_EXIT_CODE: u8 = 3;
/// Covers worst-case JSON escaping of the 4 KiB diagnostic plus fixed fields.
pub const MAX_JSON_BYTES: usize = 32 * 1024;

#[derive(Debug)]
pub struct JudgingReport {
    json: Vec<u8>,
    exit_code: u8,
}
impl JudgingReport {
    pub fn json_bytes(&self) -> &[u8] {
        &self.json
    }
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("cannot serialize the judging report: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("judging JSON exceeded its 32 KiB report bound")]
    TooLarge,
}

/// Preserve the typed library outcome and distinguish it through the exit code.
/// Every completed user-code evaluation, including failing verdicts, exits zero.
pub fn report_outcome(outcome: EvaluationOutcome) -> Result<JudgingReport, ReportError> {
    let (json, exit_code) = match outcome {
        Ok(result) => (serde_json::to_vec(&result)?, EVALUATED_EXIT_CODE),
        Err(error) => (serde_json::to_vec(&error)?, INFRASTRUCTURE_EXIT_CODE),
    };
    if json.len() > MAX_JSON_BYTES {
        return Err(ReportError::TooLarge);
    }
    Ok(JudgingReport { json, exit_code })
}
