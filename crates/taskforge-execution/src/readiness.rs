//! Synchronous preflight for a dedicated native Linux host.
//!
//! Call from a blocking thread if integrated into an asynchronous worker later.
//! Only the compiled-in trusted probe is executed. Submission supervision is a
//! separate concern; this is not the runner API.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

use crate::ExecutionConfig;

pub const ISOLATE_VERSION: &str = "2.7";
pub const PYTHON_VERSION: &str = "3.14.8";
const CAPTURE_LIMIT: u64 = 65_536;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
const SMOKE: &str = include_str!("smoke.py");

#[derive(Debug, Serialize)]
pub struct ReadinessReport {
    pub isolate_version: String,
    pub python_version: String,
    pub cgroup_root: PathBuf,
    pub checked_box_ids: Vec<u16>,
    /// Trusted Isolate metadata, not a user-code verdict.
    pub smoke_metadata: String,
}

#[derive(Debug, Error)]
pub enum ReadinessError {
    #[error("readiness requires native Linux with cgroup v2")]
    UnsupportedHost,
    #[error("readiness I/O failed: {0}; see docs/setup/linux-isolate.md")]
    Io(#[from] io::Error),
    #[error("{0}; see docs/setup/linux-isolate.md")]
    Prerequisite(String),
    #[error("sandbox ID {0} is busy; retry after the other TaskForge operation finishes")]
    Busy(u16),
    #[error("readiness command '{0}' exceeded its deadline")]
    Timeout(String),
    #[error("readiness command '{0}' exceeded the 64 KiB diagnostic bound")]
    ExcessOutput(String),
    #[error(
        "readiness command '{operation}' failed: {diagnostic}; see docs/setup/linux-isolate.md"
    )]
    Command {
        operation: String,
        diagnostic: String,
    },
    #[error("sandbox cleanup failed: {cleanup}; original check: {original}")]
    Cleanup { original: String, cleanup: String },
}

/// Check every configured ID with init/cleanup, and run a trusted Python probe
/// in the first ID. A successful return requires successful cleanup too.
/// The configured range must be exclusive to TaskForge on this host.
pub fn check_readiness(config: &ExecutionConfig) -> Result<ReadinessReport, ReadinessError> {
    if !cfg!(target_os = "linux") || !Path::new("/sys/fs/cgroup/cgroup.controllers").is_file() {
        return Err(ReadinessError::UnsupportedHost);
    }
    if config.limits().processes().get() != 1 {
        return Err(ReadinessError::Prerequisite(
            "Phase 1 requires limits.processes = 1".into(),
        ));
    }
    let deadline = Instant::now() + config.evaluation_timeout().min(Duration::from_secs(60));
    let mut version = Command::new(config.isolate_path());
    version.arg("--version");
    let isolate_version = checked(&mut version, deadline)?.trim().to_owned();
    // The pinned release prints its version on the first line, followed by a
    // copyright line (and build details when compiled from a Git checkout).
    if isolate_version.lines().next()
        != Some(format!("The process isolator {ISOLATE_VERSION}").as_str())
    {
        return Err(ReadinessError::Prerequisite(format!(
            "expected Isolate {ISOLATE_VERSION}, found {isolate_version:?}"
        )));
    }
    let mut python = Command::new(config.python_path());
    python.args(["-I", "-B", "--version"]);
    let python_version = checked(&mut python, deadline)?.trim().to_owned();
    if python_version != format!("Python {PYTHON_VERSION}") {
        return Err(ReadinessError::Prerequisite(format!(
            "expected Python {PYTHON_VERSION}, found {python_version:?}"
        )));
    }
    let mut cg = Command::new(config.isolate_path());
    cg.arg("--print-cg-root");
    let cgroup_root = PathBuf::from(checked(&mut cg, deadline)?.trim());
    if !cgroup_root.is_absolute() || !cgroup_root.join("cgroup.controllers").is_file() {
        return Err(ReadinessError::Prerequisite(
            "Isolate's delegated cgroup v2 root is unavailable; start isolate.service".into(),
        ));
    }
    let controllers = fs::read_to_string(cgroup_root.join("cgroup.controllers"))?;
    if !["cpu", "memory", "pids"]
        .iter()
        .all(|required| controllers.split_whitespace().any(|c| c == *required))
    {
        return Err(ReadinessError::Prerequisite(
            "Isolate needs delegated cpu, memory, and pids controllers".into(),
        ));
    }

    // Lock all IDs before initializing any, so a partially occupied range causes
    // no sandbox changes. Never unlink lock files: doing so breaks file locks.
    let _locks = lock_range(config)?;
    let mut checked_box_ids = Vec::new();
    let mut smoke_metadata = String::new();
    for id in config.sandbox().box_id_start..=config.sandbox().box_id_end {
        let result = (|| {
            let mut init = isolate(config, id);
            init.arg("--init");
            let box_path = PathBuf::from(checked(&mut init, deadline)?.trim());
            if !box_path.is_absolute() || !box_path.is_dir() {
                return Err(ReadinessError::Prerequisite(
                    "Isolate --init returned an invalid sandbox directory".into(),
                ));
            }
            if id == config.sandbox().box_id_start {
                smoke_metadata = smoke(config, id, deadline)?;
            }
            Ok(())
        })();
        // Init may have allocated resources even when it returned an error.
        // Give cleanup its own bounded grace period after the main deadline.
        let cleanup = cleanup(config, id);
        if let Err(error) = cleanup {
            return Err(ReadinessError::Cleanup {
                original: result
                    .err()
                    .map_or_else(|| "passed".into(), |e: ReadinessError| e.to_string()),
                cleanup: error.to_string(),
            });
        }
        result?;
        if Instant::now() >= deadline {
            return Err(ReadinessError::Timeout("whole readiness check".into()));
        }
        checked_box_ids.push(id);
    }
    Ok(ReadinessReport {
        isolate_version,
        python_version,
        cgroup_root,
        checked_box_ids,
        smoke_metadata,
    })
}

fn lock_range(config: &ExecutionConfig) -> Result<Vec<File>, ReadinessError> {
    let directory = &config.sandbox().lock_directory;
    let metadata = fs::metadata(directory).map_err(|e| {
        ReadinessError::Prerequisite(format!(
            "cannot access lock directory '{}': {e}; provision it for the judge user",
            directory.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(ReadinessError::Prerequisite(
                "lock directory must not be group/world writable".into(),
            ));
        }
    }
    let mut locks = Vec::new();
    for id in config.sandbox().box_id_start..=config.sandbox().box_id_end {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(0o400000); // Linux O_NOFOLLOW
        }
        let file = options.open(directory.join(format!("box-{id}.lock")))?;
        match file.try_lock() {
            Ok(()) => locks.push(file),
            Err(std::fs::TryLockError::WouldBlock) => return Err(ReadinessError::Busy(id)),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
    Ok(locks)
}

fn isolate(config: &ExecutionConfig, id: u16) -> Command {
    let mut command = Command::new(config.isolate_path());
    command.args(["--cg", &format!("--box-id={id}")]);
    command
}

fn cleanup(config: &ExecutionConfig, id: u16) -> Result<(), ReadinessError> {
    let mut command = isolate(config, id);
    command.arg("--cleanup");
    checked(&mut command, Instant::now() + OPERATION_TIMEOUT).map(|_| ())
}

fn smoke(config: &ExecutionConfig, id: u16, deadline: Instant) -> Result<String, ReadinessError> {
    let metadata = tempfile::NamedTempFile::new()?;
    let mut command = isolate(config, id);
    command.args([
        "--run",
        "--no-default-dirs",
        "--dir=box=./box:norec",
        "--dir=usr=/usr:norec",
        "--dir=lib=/lib:norec",
        "--dir=lib64=/lib64:maybe:norec",
        "--processes=1",
        "--time=1",
        "--wall-time=3",
        "--cg-mem=131072",
        "--fsize=0",
        "--core=0",
        "--open-files=32",
        "--env=LANG=C.UTF-8",
    ]);
    command.arg(format!("--meta={}", metadata.path().display()));
    command
        .arg("--")
        .arg(config.python_path())
        .args(["-I", "-B", "-c", SMOKE]);
    let output = checked(&mut command, deadline)?;
    if output.trim() != format!("TASKFORGE_SMOKE_OK {PYTHON_VERSION}") {
        return Err(ReadinessError::Prerequisite(format!(
            "unexpected sandbox probe output: {output:?}"
        )));
    }
    let text = read_bounded(metadata.reopen()?)?;
    if metadata_value(&text, "exitcode:") != Some("0")
        || text.lines().any(|line| {
            ["status:", "exitsig:", "killed:", "cg-oom-killed:"]
                .iter()
                .any(|key| line.starts_with(key))
        })
        || !["time:", "time-wall:"].iter().all(|key| {
            metadata_value(&text, key)
                .and_then(|value| value.parse::<f64>().ok())
                .is_some_and(|value| value.is_finite() && value >= 0.0)
        })
        || !metadata_value(&text, "cg-mem:").is_some_and(|value| value.parse::<u64>().is_ok())
    {
        return Err(ReadinessError::Prerequisite(format!(
            "missing or unsuccessful smoke metadata: {text}"
        )));
    }
    Ok(text)
}

fn metadata_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut values = text.lines().filter_map(|line| line.strip_prefix(key));
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

// Redirect to trusted temporary files to avoid a pipe deadlock. Retained
// diagnostics are bounded, and size/deadline checks run while the child runs.
// These are trusted tools and a fixed probe, not arbitrary submitted programs.
fn checked(command: &mut Command, deadline: Instant) -> Result<String, ReadinessError> {
    let operation = format!(
        "{} {:?}",
        command.get_program().to_string_lossy(),
        command.get_args().next()
    );
    let deadline = deadline.min(Instant::now() + OPERATION_TIMEOUT);
    if Instant::now() >= deadline {
        return Err(ReadinessError::Timeout(operation));
    }
    let stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    let mut child = command.spawn()?;
    let status = loop {
        let excess = stdout
            .metadata()?
            .len()
            .saturating_add(stderr.metadata()?.len())
            > CAPTURE_LIMIT;
        if excess || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if excess {
                ReadinessError::ExcessOutput(operation)
            } else {
                ReadinessError::Timeout(operation)
            });
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(10));
    };
    // Cloned descriptors share offsets with the child; rewind before reading.
    use std::io::{Seek, SeekFrom};
    let mut stdout = stdout;
    let mut stderr = stderr;
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let output = read_bounded(stdout)?;
    let diagnostic = read_bounded(stderr)?;
    if output.len().saturating_add(diagnostic.len()) as u64 > CAPTURE_LIMIT {
        return Err(ReadinessError::ExcessOutput(operation));
    }
    if !status.success() {
        return Err(ReadinessError::Command {
            operation,
            diagnostic: format!("{status}: {diagnostic}"),
        });
    }
    Ok(output)
}

fn read_bounded(file: File) -> Result<String, ReadinessError> {
    let mut bytes = Vec::new();
    file.take(CAPTURE_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > CAPTURE_LIMIT {
        return Err(ReadinessError::ExcessOutput("capture/metadata".into()));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
