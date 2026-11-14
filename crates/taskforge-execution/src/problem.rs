//! Trusted, versioned problem fixtures. Loading never executes a submission.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ConfigError, ExecutionLimits, LimitSettings};

pub const PROBLEM_IDS: [&str; 3] = ["sum-two-integers-v2", "sum-list-v2", "balanced-brackets-v2"];
pub const MAX_DEFINITION_BYTES: usize = 256 * 1024;
pub const MAX_CASE_BYTES: usize = 8 * 1024;

/// A runtime identifier, separate from the executable path on a particular host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum ProblemRuntime {
    #[serde(rename = "python-3.14.8")]
    Python3148,
    #[serde(rename = "python-3.14.10")]
    Python31410,
}

/// Unlike host configuration, immutable problem fixtures require every limit.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProblemLimits {
    pub cpu_time_ms: u64,
    pub wall_time_ms: u64,
    pub memory_bytes: u64,
    pub output_bytes: u64,
    pub processes: u32,
}

impl From<ProblemLimits> for LimitSettings {
    fn from(value: ProblemLimits) -> Self {
        Self {
            cpu_time_ms: value.cpu_time_ms,
            wall_time_ms: value.wall_time_ms,
            memory_bytes: value.memory_bytes,
            output_bytes: value.output_bytes,
            processes: value.processes,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicExample {
    pub input: String,
    pub output: String,
}

/// Safe to serialize as public problem information; contains no private cases.
#[derive(Debug, Clone, Serialize)]
pub struct PublicProblem {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub description: String,
    pub input_format: String,
    pub output_format: String,
    pub runtime: ProblemRuntime,
    pub limits: ProblemLimits,
    pub examples: Vec<PublicExample>,
}

/// Trusted judge data. These values must never be staged with submitted source.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestCase {
    pub id: String,
    pub position: usize,
    pub input: String,
    pub expected_output: String,
}

/// Validated fixture. Intentionally not Serialize: choose public() explicitly
/// for external responses; test_cases() is for trusted evaluation/seeding only.
#[derive(Debug, Clone)]
pub struct Problem {
    public: PublicProblem,
    test_cases: Vec<TestCase>,
    execution_limits: ExecutionLimits,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProblemDocument {
    schema_version: u32,
    id: String,
    title: String,
    description: String,
    input_format: String,
    output_format: String,
    runtime: ProblemRuntime,
    limits: ProblemLimits,
    examples: Vec<PublicExample>,
    test_cases: Vec<TestCase>,
}

impl Problem {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProblemError> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|source| ProblemError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut bytes = Vec::new();
        file.take((MAX_DEFINITION_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| ProblemError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Self::from_json(&bytes)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ProblemError> {
        if bytes.len() > MAX_DEFINITION_BYTES {
            return Err(ProblemError::TooLarge);
        }
        let document: ProblemDocument = serde_json::from_slice(bytes)?;
        if document.schema_version != 1 {
            return Err(ProblemError::Invalid("schema_version must be 1".into()));
        }
        validate_id(&document.id)?;
        for (field, value) in [
            ("title", document.title.as_str()),
            ("description", document.description.as_str()),
            ("input_format", document.input_format.as_str()),
            ("output_format", document.output_format.as_str()),
        ] {
            if value.trim().is_empty() || value.len() > 16 * 1024 {
                return Err(ProblemError::Invalid(format!(
                    "{field} must contain 1..=16384 bytes of nonblank text"
                )));
            }
        }
        let execution_limits = ExecutionLimits::try_from(LimitSettings::from(document.limits))?;
        if execution_limits.processes().get() != 1 {
            return Err(ProblemError::Invalid(
                "Phase 1 fixtures require processes = 1".into(),
            ));
        }
        if !(1..=8).contains(&document.examples.len()) {
            return Err(ProblemError::Invalid(
                "examples must contain 1..=8 entries".into(),
            ));
        }
        for example in &document.examples {
            validate_case_size(&example.input, &example.output)?;
        }
        if !(5..=128).contains(&document.test_cases.len()) {
            return Err(ProblemError::Invalid(
                "test_cases must contain 5..=128 ordered entries".into(),
            ));
        }
        let mut ids = HashSet::new();
        for (position, case) in document.test_cases.iter().enumerate() {
            validate_id(&case.id)?;
            if !ids.insert(&case.id) {
                return Err(ProblemError::Invalid(format!(
                    "duplicate test ID '{}'",
                    case.id
                )));
            }
            if case.position != position {
                return Err(ProblemError::Invalid(format!(
                    "test '{}' must have position {position}; array order is authoritative",
                    case.id
                )));
            }
            validate_case_size(&case.input, &case.expected_output)?;
        }
        Ok(Self {
            public: PublicProblem {
                schema_version: document.schema_version,
                id: document.id,
                title: document.title,
                description: document.description,
                input_format: document.input_format,
                output_format: document.output_format,
                runtime: document.runtime,
                limits: document.limits,
                examples: document.examples,
            },
            test_cases: document.test_cases,
            execution_limits,
        })
    }

    pub fn public(&self) -> &PublicProblem {
        &self.public
    }

    /// Hidden inputs and expected outputs belong to trusted judge code only.
    pub fn test_cases(&self) -> &[TestCase] {
        &self.test_cases
    }

    pub fn limits(&self) -> &ExecutionLimits {
        &self.execution_limits
    }
}

/// Load the three fixed definitions in a stable order. No directory enumeration
/// or sorting of cases: positions and IDs remain suitable for immutable seeding.
pub fn load_problems(directory: impl AsRef<Path>) -> Result<Vec<Problem>, ProblemError> {
    PROBLEM_IDS
        .iter()
        .map(|id| {
            let problem = Problem::load(directory.as_ref().join(format!("{id}.json")))?;
            if problem.public().id != *id {
                return Err(ProblemError::Invalid(format!(
                    "file '{id}.json' must declare ID '{id}'"
                )));
            }
            Ok(problem)
        })
        .collect()
}

fn validate_id(id: &str) -> Result<(), ProblemError> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || id.starts_with('-')
        || id.ends_with('-')
    {
        return Err(ProblemError::Invalid("IDs must be 1..=64 lowercase ASCII letters/digits/hyphens with no leading/trailing hyphen".into()));
    }
    Ok(())
}

fn validate_case_size(input: &str, output: &str) -> Result<(), ProblemError> {
    if input.len() > MAX_CASE_BYTES || output.len() > MAX_CASE_BYTES {
        return Err(ProblemError::Invalid(
            "each case/example input and output must be at most 8192 bytes".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ProblemError {
    #[error("cannot read problem fixture '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("problem definition exceeds the 256 KiB limit")]
    TooLarge,
    #[error("invalid problem JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid problem definition: {0}")]
    Invalid(String),
    #[error("invalid problem limits: {0}")]
    Limits(#[from] ConfigError),
}
