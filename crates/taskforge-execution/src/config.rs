use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::limits::{ExecutionLimits, LimitSettings, positive_u64};

/// File-boundary settings, before numeric and filesystem validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSettings {
    #[serde(default)]
    pub limits: LimitSettings,
    #[serde(default = "default_evaluation_timeout_ms")]
    pub evaluation_timeout_ms: u64,
    pub runtime: RuntimeSettings,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSettings {
    pub python_path: PathBuf,
    pub isolate_path: PathBuf,
}

/// Statically validated configuration. This is not sandbox readiness evidence.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    limits: ExecutionLimits,
    evaluation_timeout: Duration,
    python_path: PathBuf,
    isolate_path: PathBuf,
}

impl ExecutionConfig {
    /// Load JSON, resolving executable paths relative to the configuration file.
    /// No configured executable is launched by this method.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let absolute = fs::canonicalize(path).map_err(|source| ConfigError::ReadConfig {
            path: path.to_path_buf(),
            source,
        })?;
        let contents = fs::read_to_string(&absolute).map_err(|source| ConfigError::ReadConfig {
            path: absolute.clone(),
            source,
        })?;
        let settings: ExecutionSettings = serde_json::from_str(&contents)?;
        let base = absolute.parent().unwrap_or_else(|| Path::new("/"));
        settings.validate(base)
    }

    pub fn limits(&self) -> &ExecutionLimits {
        &self.limits
    }

    pub fn evaluation_timeout(&self) -> Duration {
        self.evaluation_timeout
    }

    pub fn python_path(&self) -> &Path {
        &self.python_path
    }

    pub fn isolate_path(&self) -> &Path {
        &self.isolate_path
    }
}

impl ExecutionSettings {
    /// Validate numeric settings and regular executable files on this host.
    /// Linux privileges, cgroups, binary identity, and sandbox restrictions are
    /// verified separately in issue #3 before any submitted code is executed.
    pub fn validate(self, base: &Path) -> Result<ExecutionConfig, ConfigError> {
        let limits = ExecutionLimits::try_from(self.limits)?;
        let timeout = positive_u64("evaluation_timeout_ms", self.evaluation_timeout_ms)?;
        Ok(ExecutionConfig {
            limits,
            evaluation_timeout: Duration::from_millis(timeout.get()),
            python_path: executable_path("runtime.python_path", base, &self.runtime.python_path)?,
            isolate_path: executable_path(
                "runtime.isolate_path",
                base,
                &self.runtime.isolate_path,
            )?,
        })
    }
}

fn executable_path(field: &'static str, base: &Path, path: &Path) -> Result<PathBuf, ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::EmptyPath { field });
    }
    let path = base.join(path);
    let absolute = fs::canonicalize(&path).map_err(|source| ConfigError::AccessExecutable {
        field,
        path: path.clone(),
        source,
    })?;
    let metadata = fs::metadata(&absolute).map_err(|source| ConfigError::AccessExecutable {
        field,
        path: absolute.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(ConfigError::NotFile {
            field,
            path: absolute,
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(ConfigError::NotExecutable {
                field,
                path: absolute,
            });
        }
    }
    Ok(absolute)
}

fn default_evaluation_timeout_ms() -> u64 {
    60_000
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read configuration '{path}': {source}")]
    ReadConfig {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid configuration JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("{field} must be greater than zero")]
    NonPositive { field: &'static str },
    #[error("{field} must not be an empty path")]
    EmptyPath { field: &'static str },
    #[error("cannot access {field} at '{path}': {source}")]
    AccessExecutable {
        field: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{field} at '{path}' must be a regular file")]
    NotFile { field: &'static str, path: PathBuf },
    #[error("{field} at '{path}' has no Unix executable permission bits")]
    NotExecutable { field: &'static str, path: PathBuf },
}
