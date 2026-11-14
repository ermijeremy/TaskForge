//! Per-test Isolate lifecycle, shared deadline checkpoints and cgroup cleanup.
//! Public launch requires validated Isolate limit conversion.
mod limits;
mod metadata;
mod runtime;

pub use limits::{IsolateLimits, LimitConversionError};
pub use metadata::{MAX_METADATA_BYTES, SandboxMetadata, SandboxStatus};

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::execution_control::ExecutionControl;
use crate::{
    ExecutionConfig, ExecutionLimits, InfrastructureError, InfrastructureReason, check_readiness,
};
use runtime::RuntimeImage;

pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
const CONTROL_BYTES: u64 = 16 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);

/// Long-lived, immutable runtime image shared by fresh test sandboxes.
#[derive(Debug, Clone)]
pub struct IsolateBackend {
    config: ExecutionConfig,
    runtime: Arc<RuntimeImage>,
}

impl IsolateBackend {
    /// Native host readiness is checked once before staging the permitted runtime.
    /// Construct before beginning concurrent evaluations; readiness needs idle IDs.
    pub fn new(config: ExecutionConfig) -> Result<Self, InfrastructureError> {
        check_readiness(&config).map_err(|_| {
            error(
                InfrastructureReason::SandboxUnavailable,
                "native Isolate readiness failed; run judge-local check-ready",
            )
        })?;
        let runtime = Arc::new(RuntimeImage::stage(&config)?);
        Ok(Self { config, runtime })
    }

    pub fn prepare(
        &self,
        source: &str,
        limits: &ExecutionLimits,
    ) -> Result<PreparedSandbox, InfrastructureError> {
        self.prepare_with_control(source, limits, ExecutionControl::default())
    }

    pub(crate) fn evaluation_timeout(&self) -> Duration {
        self.config.evaluation_timeout()
    }

    pub(crate) fn prepare_with_control(
        &self,
        source: &str,
        limits: &ExecutionLimits,
        control: ExecutionControl,
    ) -> Result<PreparedSandbox, InfrastructureError> {
        control.check_work()?;
        // Reject unrepresentable limits before reserving/staging a submission.
        let mapped = IsolateLimits::try_from(limits)
            .map_err(|e| error(InfrastructureReason::InvalidConfiguration, &e.to_string()))?;
        self.prepare_mapped(source, *limits, &mapped, control)
    }

    fn prepare_mapped(
        &self,
        source: &str,
        limits: ExecutionLimits,
        mapped: &IsolateLimits,
        control: ExecutionControl,
    ) -> Result<PreparedSandbox, InfrastructureError> {
        if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
            return Err(error(
                InfrastructureReason::InvalidConfiguration,
                "source must contain 1..=65536 UTF-8 bytes",
            ));
        }
        control.check_work()?;
        let lease = SlotLease::reserve(&self.config)?;
        control.check_work()?;
        let host = tempfile::tempdir().map_err(|_| {
            error(
                InfrastructureReason::SandboxSetupFailed,
                "cannot allocate trusted test files",
            )
        })?;
        mount_path(host.path())?;
        let mut sandbox = PreparedSandbox {
            config: self.config.clone(),
            runtime: self.runtime.clone(),
            lease,
            host,
            box_root: PathBuf::new(),
            arguments: mapped.arguments(),
            limits,
            control,
            active: true,
        };
        let result = (|| {
            let mut init = sandbox.command();
            init.arg("--init");
            let directory = run_control(
                &mut init,
                InfrastructureReason::SandboxSetupFailed,
                &sandbox.control,
                false,
            )?;
            let box_root = PathBuf::from(directory.trim());
            if !box_root.is_absolute() || !box_root.is_dir() {
                return Err(error(
                    InfrastructureReason::SandboxSetupFailed,
                    "Isolate returned an invalid sandbox root",
                ));
            }
            let box_dir = box_root.join("box");
            let kind = fs::symlink_metadata(&box_dir).map_err(|_| {
                error(
                    InfrastructureReason::SandboxSetupFailed,
                    "missing fresh source directory",
                )
            })?;
            if !kind.is_dir() || kind.file_type().is_symlink() {
                return Err(error(
                    InfrastructureReason::SandboxSetupFailed,
                    "invalid fresh source directory",
                ));
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(box_dir.join("main.py"))
                .map_err(|_| {
                    error(
                        InfrastructureReason::SandboxSetupFailed,
                        "cannot stage submitted source",
                    )
                })?;
            file.write_all(source.as_bytes()).map_err(|_| {
                error(
                    InfrastructureReason::SandboxSetupFailed,
                    "cannot write submitted source",
                )
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o444))
                    .map_err(|_| {
                        error(
                            InfrastructureReason::SandboxSetupFailed,
                            "cannot protect staged source",
                        )
                    })?;
            }
            sandbox.box_root = box_root;
            sandbox.control.check_work()?;
            Ok(())
        })();
        if let Err(original) = result {
            return match sandbox.cleanup_inner() {
                Ok(()) => Err(original),
                Err(cleanup) => Err(cleanup_with_original(original, cleanup)),
            };
        }
        Ok(sandbox)
    }
}

#[derive(Debug)]
struct SlotLease {
    id: u16,
    _file: File,
    quarantine: PathBuf,
}

impl SlotLease {
    fn reserve(config: &ExecutionConfig) -> Result<Self, InfrastructureError> {
        let directory = &config.sandbox().lock_directory;
        let metadata = fs::metadata(directory).map_err(|_| {
            error(
                InfrastructureReason::SandboxUnavailable,
                "cannot access judge lock directory",
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if !metadata.is_dir() || metadata.permissions().mode() & 0o022 != 0 {
                return Err(error(
                    InfrastructureReason::SandboxUnavailable,
                    "judge lock directory must be private and not group/world writable",
                ));
            }
        }
        for id in config.sandbox().box_id_start..=config.sandbox().box_id_end {
            let quarantine = directory.join(format!("box-{id}.quarantine"));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true).truncate(false);
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600).custom_flags(0o400000); // O_NOFOLLOW
            }
            let file = options
                .open(directory.join(format!("box-{id}.lock")))
                .map_err(|_| {
                    error(
                        InfrastructureReason::SandboxUnavailable,
                        "cannot open sandbox slot lock",
                    )
                })?;
            match file.try_lock() {
                Ok(())
                    if !quarantine.try_exists().map_err(|_| {
                        error(
                            InfrastructureReason::SandboxUnavailable,
                            "cannot inspect slot quarantine",
                        )
                    })? =>
                {
                    return Ok(Self {
                        id,
                        _file: file,
                        quarantine,
                    });
                }
                Ok(()) | Err(std::fs::TryLockError::WouldBlock) => continue,
                Err(std::fs::TryLockError::Error(_)) => {
                    return Err(error(
                        InfrastructureReason::SandboxUnavailable,
                        "cannot lock sandbox slot",
                    ));
                }
            }
        }
        Err(error(
            InfrastructureReason::SandboxUnavailable,
            "all configured sandbox IDs are busy or quarantined",
        ))
    }

    fn quarantine(&self) {
        // Persistent marker keeps later allocations from reusing a failed cleanup.
        let _ = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.quarantine);
    }
}

/// Owns the slot lock and trusted files until successful explicit cleanup.
#[derive(Debug)]
pub struct PreparedSandbox {
    config: ExecutionConfig,
    runtime: Arc<RuntimeImage>,
    lease: SlotLease,
    host: TempDir,
    box_root: PathBuf,
    arguments: Vec<String>,
    limits: ExecutionLimits,
    control: ExecutionControl,
    active: bool,
}

impl PreparedSandbox {
    pub fn box_id(&self) -> u16 {
        self.lease.id
    }

    fn command(&self) -> Command {
        let mut command = Command::new(self.config.isolate_path());
        command.args(["--cg", &format!("--box-id={}", self.box_id())]);
        command
    }

    fn run_command(&self) -> Result<Command, InfrastructureError> {
        let mut command = self.command();
        command.args([
            "--run",
            "--silent",
            "--no-default-dirs",
            "--dir=box=./box:norec",
            "--env=LANG=C.UTF-8",
            "--open-files=32",
            "--core=0",
            "--fsize=0",
        ]);
        command.arg(format!(
            "--dir=usr={}:norec",
            mount_path(&self.runtime.usr())?
        ));
        command.arg(format!(
            "--dir=lib64={}:norec",
            mount_path(&self.runtime.lib64())?
        ));
        command.args(&self.arguments);
        command.arg(format!(
            "--meta={}",
            self.host.path().join("metadata").display()
        ));
        command.args([
            "--",
            "/usr/bin/python3.14",
            "-I",
            "-B",
            "-S",
            "/box/main.py",
        ]);
        Ok(command)
    }

    /// Input stays in trusted host storage and is inherited read-only on fd 0.
    /// The API accepts no expected output, problem definition or host mount list.
    pub fn spawn(mut self, input: &[u8]) -> Result<RunningSandbox, InfrastructureError> {
        let result = (|| {
            self.control.check_work()?;
            if input.len() > crate::problem::MAX_CASE_BYTES {
                return Err(error(
                    InfrastructureReason::InvalidConfiguration,
                    "test input exceeds the 8 KiB bound",
                ));
            }
            let input_path = self.host.path().join("input");
            fs::write(&input_path, input).map_err(|_| {
                error(
                    InfrastructureReason::SandboxSetupFailed,
                    "cannot prepare trusted stdin",
                )
            })?;
            let input = File::open(input_path).map_err(|_| {
                error(
                    InfrastructureReason::SandboxSetupFailed,
                    "cannot open trusted stdin",
                )
            })?;
            self.control.check_work()?;
            let mut command = self.run_command()?;
            command
                .stdin(input)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            command.spawn().map_err(|_| {
                error(
                    InfrastructureReason::SandboxSetupFailed,
                    "cannot launch Isolate; no host-process fallback exists",
                )
            })
        })();
        match result {
            Ok(mut child) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                Ok(RunningSandbox {
                    child: Some(child),
                    sandbox: Some(self),
                    stdout,
                    stderr,
                })
            }
            Err(original) => match self.cleanup_inner() {
                Ok(()) => Err(original),
                Err(cleanup) => Err(cleanup_with_original(original, cleanup)),
            },
        }
    }

    pub fn cleanup(mut self) -> Result<(), InfrastructureError> {
        self.cleanup_inner()
    }

    fn cleanup_inner(&mut self) -> Result<(), InfrastructureError> {
        if !self.active {
            return Ok(());
        }
        let mut command = self.command();
        command.arg("--cleanup");
        let result = run_control(
            &mut command,
            InfrastructureReason::CleanupFailed,
            &self.control,
            true,
        )
        .map(|_| ());
        if result.is_err() && self.control.check_cleanup().is_err() {
            // Exhausting the response deadline must never suppress an attempt
            // to kill the cgroup. Recovery stays on the blocking worker, with a
            // separate bounded control guard; the caller gets no successful result
            // and the slot is quarantined even if this late recovery succeeds.
            let mut recovery = self.command();
            recovery.arg("--cleanup");
            let _ = run_control(
                &mut recovery,
                InfrastructureReason::CleanupFailed,
                &ExecutionControl::default(),
                true,
            );
        }
        self.active = false;
        if result.is_err() {
            self.lease.quarantine();
        }
        result
    }
}

impl Drop for PreparedSandbox {
    fn drop(&mut self) {
        let _ = self.cleanup_inner();
    }
}

#[derive(Debug)]
pub struct SandboxExit {
    pub wrapper_status: ExitStatus,
    pub metadata: SandboxMetadata,
}

/// Low-level handles for the issue #7 supervisor. The caller must drain stdout
/// and stderr concurrently before finish; unbounded read_to_end is not a supervisor.
#[derive(Debug)]
pub struct RunningSandbox {
    child: Option<Child>,
    sandbox: Option<PreparedSandbox>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

impl RunningSandbox {
    pub fn box_id(&self) -> u16 {
        self.sandbox.as_ref().expect("active sandbox").box_id()
    }
    pub fn output_limit_bytes(&self) -> u64 {
        self.sandbox
            .as_ref()
            .expect("active sandbox")
            .limits
            .output_bytes()
            .get()
    }

    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, InfrastructureError> {
        self.child
            .as_mut()
            .expect("active child")
            .try_wait()
            .map_err(|_| {
                error(
                    InfrastructureReason::ProcessIoFailed,
                    "cannot poll Isolate process",
                )
            })
    }

    pub fn finish(mut self) -> Result<SandboxExit, InfrastructureError> {
        let mut child = self.child.take().expect("active child");
        let control = self
            .sandbox
            .as_ref()
            .expect("active sandbox")
            .control
            .clone();
        let result = (|| {
            let status = wait_child(&mut child, &control)?;
            if !matches!(status.code(), Some(0 | 1)) {
                return Err(error(
                    InfrastructureReason::SandboxSetupFailed,
                    "Isolate reported an infrastructure failure",
                ));
            }
            let sandbox = self.sandbox.as_ref().expect("active sandbox");
            let file = File::open(sandbox.host.path().join("metadata")).map_err(|_| {
                error(
                    InfrastructureReason::MetadataMissing,
                    "Isolate did not produce readable metadata",
                )
            })?;
            let mut bytes = Vec::new();
            file.take((MAX_METADATA_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| {
                    error(
                        InfrastructureReason::MetadataInvalid,
                        "cannot read sandbox metadata",
                    )
                })?;
            let metadata = SandboxMetadata::parse(&bytes)?;
            if metadata.status == Some(SandboxStatus::InternalError) {
                return Err(error(
                    InfrastructureReason::SandboxSetupFailed,
                    "Isolate metadata reports an internal failure",
                ));
            }
            Ok(SandboxExit {
                wrapper_status: status,
                metadata,
            })
        })();
        if result.is_err() {
            let _ = stop_child(&mut child, &control);
        }
        let cleanup = self.sandbox.take().expect("active sandbox").cleanup();
        combine(result, cleanup)
    }

    /// Stop the wrapper, then invoke Isolate cleanup to kill the whole cgroup.
    /// Killing only the wrapper never counts as successful cleanup.
    pub fn cancel(mut self) -> Result<(), InfrastructureError> {
        let mut child = self.child.take().expect("active child");
        let control = self
            .sandbox
            .as_ref()
            .expect("active sandbox")
            .control
            .clone();
        let result = stop_child(&mut child, &control);
        let cleanup = self.sandbox.take().expect("active sandbox").cleanup();
        combine(result, cleanup)
    }
}

impl Drop for RunningSandbox {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let control = self
                .sandbox
                .as_ref()
                .expect("active sandbox")
                .control
                .clone();
            let _ = stop_child(&mut child, &control);
        }
        // PreparedSandbox's Drop attempts whole-sandbox cleanup after the wrapper.
    }
}

fn combine<T>(
    result: Result<T, InfrastructureError>,
    cleanup: Result<(), InfrastructureError>,
) -> Result<T, InfrastructureError> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(original), Ok(())) => Err(original),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(original), Err(cleanup)) => Err(cleanup_with_original(original, cleanup)),
    }
}

fn cleanup_with_original(
    original: InfrastructureError,
    cleanup: InfrastructureError,
) -> InfrastructureError {
    error(
        InfrastructureReason::CleanupFailed,
        &format!(
            "cleanup failed after {:?}: {}",
            original.reason_code(),
            cleanup.diagnostic()
        ),
    )
}

fn mount_path(path: &Path) -> Result<String, InfrastructureError> {
    let text = path
        .to_str()
        .filter(|text| !text.contains([':', '\n', '\r']) && path.is_absolute())
        .ok_or_else(|| {
            error(
                InfrastructureReason::SandboxSetupFailed,
                "runtime/control path cannot be represented safely in Isolate arguments",
            )
        })?;
    Ok(text.to_owned())
}

fn error(code: InfrastructureReason, diagnostic: &str) -> InfrastructureError {
    InfrastructureError::new(code, diagnostic)
}

// Only trusted init/cleanup commands use temporary-file capture. Submitted
// stdout/stderr are pipes drained by the bounded stream supervisor.
fn run_control(
    command: &mut Command,
    code: InfrastructureReason,
    guard: &ExecutionControl,
    cleanup: bool,
) -> Result<String, InfrastructureError> {
    let checkpoint = || {
        if cleanup {
            guard.check_cleanup()
        } else {
            guard.check_work()
        }
    };
    checkpoint()?;
    let mut stdout =
        tempfile::tempfile().map_err(|_| error(code, "cannot capture control output"))?;
    let stderr =
        tempfile::tempfile().map_err(|_| error(code, "cannot capture control diagnostics"))?;
    command
        .stdin(Stdio::null())
        .stdout(
            stdout
                .try_clone()
                .map_err(|_| error(code, "cannot clone control capture"))?,
        )
        .stderr(
            stderr
                .try_clone()
                .map_err(|_| error(code, "cannot clone control capture"))?,
        );
    checkpoint()?;
    let mut child = command
        .spawn()
        .map_err(|_| error(code, "cannot start Isolate control command"))?;
    let operation_deadline = Instant::now() + CONTROL_TIMEOUT;
    let result = (|| {
        loop {
            checkpoint()?;
            let total = stdout
                .metadata()
                .and_then(|out| {
                    stderr
                        .metadata()
                        .map(|err| out.len().saturating_add(err.len()))
                })
                .map_err(|_| error(code, "cannot inspect control output"))?;
            if total > CONTROL_BYTES || Instant::now() >= operation_deadline {
                return Err(error(
                    code,
                    "Isolate control command exceeded its time/output guard",
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        return Err(error(
                            code,
                            "Isolate control command failed; check native host prerequisites",
                        ));
                    }
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(2)),
                Err(_) => return Err(error(code, "cannot monitor Isolate control command")),
            }
        }
        if stdout
            .metadata()
            .map_err(|_| error(code, "cannot inspect control output"))?
            .len()
            .saturating_add(
                stderr
                    .metadata()
                    .map_err(|_| error(code, "cannot inspect control output"))?
                    .len(),
            )
            > CONTROL_BYTES
        {
            return Err(error(code, "Isolate control output exceeded its bound"));
        }
        stdout
            .seek(SeekFrom::Start(0))
            .map_err(|_| error(code, "cannot rewind control output"))?;
        let mut bytes = Vec::new();
        stdout
            .take(CONTROL_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error(code, "cannot read control output"))?;
        if bytes.len() as u64 > CONTROL_BYTES {
            return Err(error(code, "Isolate control output exceeded its bound"));
        }
        checkpoint()?;
        String::from_utf8(bytes).map_err(|_| error(code, "invalid Isolate control output"))
    })();
    if result.is_err() {
        let _ = stop_child(&mut child, guard);
    }
    result
}

fn wait_child(
    child: &mut Child,
    control: &ExecutionControl,
) -> Result<ExitStatus, InfrastructureError> {
    loop {
        control.check_work()?;
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(_) => {
                return Err(error(
                    InfrastructureReason::ProcessIoFailed,
                    "cannot wait for Isolate",
                ));
            }
        }
    }
}

fn stop_child(child: &mut Child, control: &ExecutionControl) -> Result<(), InfrastructureError> {
    if child
        .try_wait()
        .map_err(|_| error(InfrastructureReason::ProcessIoFailed, "cannot poll Isolate"))?
        .is_some()
    {
        return Ok(());
    }
    child.kill().map_err(|_| {
        error(
            InfrastructureReason::ProcessIoFailed,
            "cannot stop Isolate wrapper",
        )
    })?;
    let deadline = control
        .final_deadline()
        .unwrap_or_else(|| Instant::now() + CONTROL_TIMEOUT);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            _ => {
                return Err(error(
                    InfrastructureReason::CleanupFailed,
                    "cannot reap Isolate before cleanup deadline",
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests;
