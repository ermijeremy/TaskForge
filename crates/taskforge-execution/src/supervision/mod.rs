//! Bounded per-test supervision under one evaluation-wide absolute deadline.
mod budget;
pub use crate::execution_control::Cancellation;
pub use budget::{
    CapturedOutput, MAX_CAPTURE_BYTES, OutputAccountingError, OutputBudget, OutputStream,
};

use std::io::{self, Read};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use crate::execution_control::ExecutionControl;
use crate::sandbox::{IsolateBackend, SandboxMetadata};
use crate::{ExecutionConfig, ExecutionLimits, InfrastructureError, InfrastructureReason};

/// Startup/readiness/runtime-image work occurs on Tokio's blocking pool before
/// evaluations are admitted. Cached runtime initialization is not an evaluation.
#[derive(Debug, Clone)]
pub struct Supervisor {
    backend: Arc<BackendHolder>,
}

#[derive(Debug)]
struct BackendHolder(Option<IsolateBackend>);
impl Drop for BackendHolder {
    fn drop(&mut self) {
        if let Some(backend) = self.0.take() {
            // The last runtime-image clone may remove many files. Never do that
            // filesystem work on the async executor when a session is dropped.
            let _ = thread::Builder::new()
                .name("taskforge-runtime-drop".into())
                .spawn(move || drop(backend));
        }
    }
}
impl BackendHolder {
    fn get(&self) -> &IsolateBackend {
        self.0.as_ref().expect("live backend")
    }
}

impl Supervisor {
    pub async fn initialize(config: ExecutionConfig) -> Result<Self, InfrastructureError> {
        let backend = tokio::task::spawn_blocking(move || IsolateBackend::new(config))
            .await
            .map_err(|_| {
                failure(
                    InfrastructureReason::InternalFailure,
                    "sandbox startup worker failed",
                )
            })??;
        Ok(Self::from_backend(backend))
    }
    /// A preinitialized backend must have been created off the async executor.
    pub fn from_backend(backend: IsolateBackend) -> Self {
        Self {
            backend: Arc::new(BackendHolder(Some(backend))),
        }
    }
    pub fn evaluation(&self) -> Result<EvaluationSession, InfrastructureError> {
        let control = ExecutionControl::evaluation(self.backend.get().evaluation_timeout())?;
        Ok(EvaluationSession {
            backend: self.backend.clone(),
            control,
            active: Arc::new(AtomicBool::new(false)),
        })
    }
}

/// Reuse this session across ordered tests. Creating a new session per test would
/// incorrectly reset the evaluation-wide deadline. One test may run at a time.
#[derive(Debug)]
pub struct EvaluationSession {
    backend: Arc<BackendHolder>,
    control: ExecutionControl,
    active: Arc<AtomicBool>,
}
impl EvaluationSession {
    pub fn cancellation(&self) -> Cancellation {
        self.control.cancellation()
    }
    pub fn deadline(&self) -> Instant {
        self.control.final_deadline().expect("bounded evaluation")
    }
    pub fn is_idle(&self) -> bool {
        !self.active.load(Ordering::Acquire)
    }

    pub fn start_test(
        &self,
        source: &str,
        input: &[u8],
        limits: &ExecutionLimits,
    ) -> Result<EvaluationTask, InfrastructureError> {
        self.control.check_work()?;
        if source.is_empty()
            || source.len() > crate::sandbox::MAX_SOURCE_BYTES
            || input.len() > crate::problem::MAX_CASE_BYTES
        {
            return Err(failure(
                InfrastructureReason::InvalidConfiguration,
                "source or input exceeds the supported size bound",
            ));
        }
        if limits.output_bytes().get() > MAX_CAPTURE_BYTES as u64 {
            return Err(failure(
                InfrastructureReason::InvalidConfiguration,
                "supervised output budget must be at most 1 MiB",
            ));
        }
        let mut budget = OutputBudget::new(limits.output_bytes());
        // Validate accounting initialization before reserving a sandbox.
        budget
            .record(OutputStream::Stdout, &[])
            .map_err(accounting_error)?;
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            failure(
                InfrastructureReason::InvalidConfiguration,
                "supervision requires a Tokio runtime",
            )
        })?;
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(failure(
                InfrastructureReason::InternalFailure,
                "an evaluation session already has an active test",
            ));
        }
        let lease = ActiveJob(self.active.clone());
        let backend = self.backend.clone();
        let control = self.control.clone();
        let cancellation = control.cancellation();
        let source = source.to_owned();
        let input = input.to_vec();
        let limits = *limits;
        let join = runtime.spawn_blocking(move || {
            let _lease = lease;
            execute(backend.get(), &control, &source, &input, &limits, budget)
        });
        Ok(EvaluationTask {
            join,
            cancellation,
            deadline: self.deadline(),
            completed: false,
        })
    }

    pub async fn run_test(
        &self,
        source: &str,
        input: &[u8],
        limits: &ExecutionLimits,
    ) -> Result<SupervisedTest, InfrastructureError> {
        self.start_test(source, input, limits)?.wait().await
    }
}
struct ActiveJob(Arc<AtomicBool>);
impl Drop for ActiveJob {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Dropping/aborting the waiting future requests cancellation; the blocking
/// worker retains the process, lease and cleanup ownership until it finishes.
#[derive(Debug)]
pub struct EvaluationTask {
    join: tokio::task::JoinHandle<Result<SupervisedTest, InfrastructureError>>,
    cancellation: Cancellation,
    deadline: Instant,
    completed: bool,
}
impl EvaluationTask {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub async fn wait(mut self) -> Result<SupervisedTest, InfrastructureError> {
        let result = tokio::time::timeout_at(
            tokio::time::Instant::from_std(self.deadline),
            &mut self.join,
        )
        .await;
        self.completed = true;
        let result = result.map_err(|_| {
            self.cancellation.cancel();
            // The worker still owns the sandbox and lease. Cleanup/recovery may
            // continue, but no success or reusable slot is reported prematurely.
            failure(
                InfrastructureReason::EvaluationDeadlineExceeded,
                "evaluation deadline expired while awaiting worker cleanup",
            )
        })?;
        result.map_err(|_| {
            failure(
                InfrastructureReason::InternalFailure,
                "evaluation worker panicked or could not start",
            )
        })?
    }
}
impl Drop for EvaluationTask {
    fn drop(&mut self) {
        if !self.completed {
            self.cancellation.cancel();
        }
    }
}

#[derive(Debug)]
pub struct SupervisedTest {
    pub output: CapturedOutput,
    /// Missing after a supervisor kill is expected; measurements are not invented.
    pub metadata: Option<SandboxMetadata>,
    /// Explicit supervisor evidence for later Output Limit Exceeded classification.
    pub output_limit_exceeded: bool,
}

fn execute(
    backend: &IsolateBackend,
    control: &ExecutionControl,
    source: &str,
    input: &[u8],
    limits: &ExecutionLimits,
    mut budget: OutputBudget,
) -> Result<SupervisedTest, InfrastructureError> {
    control.check_work()?;
    let prepared = backend.prepare_with_control(source, limits, control.clone())?;
    let mut running = prepared.spawn(input)?;
    let outcome = (|| {
        let stdout = running.stdout.as_ref().ok_or_else(pipe_error)?;
        let stderr = running.stderr.as_ref().ok_or_else(pipe_error)?;
        make_nonblocking(stdout)?;
        make_nonblocking(stderr)?;
        let mut stdout_eof = false;
        let mut stderr_eof = false;
        loop {
            control.check_work()?;
            read_one(
                running.stdout.as_mut().expect("stdout"),
                OutputStream::Stdout,
                &mut stdout_eof,
                &mut budget,
            )?;
            if budget.exceeded() {
                return Ok(true);
            }
            read_one(
                running.stderr.as_mut().expect("stderr"),
                OutputStream::Stderr,
                &mut stderr_eof,
                &mut budget,
            )?;
            if budget.exceeded() {
                return Ok(true);
            }
            if running.try_wait()?.is_some() && stdout_eof && stderr_eof {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(2));
        }
    })();
    match outcome {
        Ok(false) => {
            let exit = running.finish()?;
            control.check_result()?;
            Ok(SupervisedTest {
                output: budget.into_capture(),
                metadata: Some(exit.metadata),
                output_limit_exceeded: false,
            })
        }
        Ok(true) => {
            // A known wrapper failure takes precedence over stream-limit evidence.
            if running
                .try_wait()?
                .is_some_and(|status| !matches!(status.code(), Some(0 | 1)))
            {
                running.cancel()?;
                return Err(failure(
                    InfrastructureReason::SandboxSetupFailed,
                    "Isolate reported an infrastructure failure",
                ));
            }
            running.cancel()?;
            control.check_result()?;
            Ok(SupervisedTest {
                output: budget.into_capture(),
                metadata: None,
                output_limit_exceeded: true,
            })
        }
        Err(original) => match running.cancel() {
            Ok(()) => Err(original),
            Err(cleanup) => Err(failure(
                InfrastructureReason::CleanupFailed,
                &format!(
                    "cleanup failed after {:?}: {}",
                    original.reason_code(),
                    cleanup.diagnostic()
                ),
            )),
        },
    }
}

fn read_one(
    reader: &mut impl Read,
    stream: OutputStream,
    eof: &mut bool,
    budget: &mut OutputBudget,
) -> Result<(), InfrastructureError> {
    if *eof {
        return Ok(());
    }
    let mut chunk = [0_u8; 8192];
    match reader.read(&mut chunk) {
        Ok(0) => {
            *eof = true;
            Ok(())
        }
        Ok(size) => budget
            .record(stream, &chunk[..size])
            .map_err(accounting_error),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) =>
        {
            Ok(())
        }
        Err(_) => Err(pipe_error()),
    }
}
#[cfg(unix)]
fn make_nonblocking(fd: &impl std::os::fd::AsFd) -> Result<(), InfrastructureError> {
    let flags = rustix::fs::fcntl_getfl(fd).map_err(|_| pipe_error())?;
    rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK).map_err(|_| pipe_error())
}
#[cfg(not(unix))]
fn make_nonblocking<T>(_fd: &T) -> Result<(), InfrastructureError> {
    Err(failure(
        InfrastructureReason::InvalidConfiguration,
        "sandbox supervision requires Unix file descriptors",
    ))
}
fn accounting_error(error: OutputAccountingError) -> InfrastructureError {
    failure(InfrastructureReason::InternalFailure, &error.to_string())
}
fn pipe_error() -> InfrastructureError {
    failure(
        InfrastructureReason::ProcessIoFailed,
        "cannot configure or read sandbox output pipes",
    )
}
fn failure(reason: InfrastructureReason, message: &str) -> InfrastructureError {
    InfrastructureError::new(reason, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn response_deadline_does_not_abort_the_worker_or_claim_cleanup_finished() {
        let cancellation = Cancellation::default();
        let done = Arc::new(AtomicBool::new(false));
        let worker_done = done.clone();
        let worker = tokio::task::spawn_blocking(move || {
            thread::sleep(Duration::from_millis(150));
            worker_done.store(true, Ordering::Release);
            Err(failure(
                InfrastructureReason::Cancelled,
                "test worker stopped",
            ))
        });
        let task = EvaluationTask {
            join: worker,
            cancellation: cancellation.clone(),
            deadline: Instant::now() + Duration::from_millis(20),
            completed: false,
        };
        let error = task.wait().await.unwrap_err();
        assert_eq!(
            error.reason_code(),
            InfrastructureReason::EvaluationDeadlineExceeded
        );
        assert!(cancellation.is_cancelled());
        assert!(!done.load(Ordering::Acquire));
        tokio::time::timeout(Duration::from_secs(1), async {
            while !done.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
    }
}
