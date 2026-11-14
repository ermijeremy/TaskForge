//! Shared absolute evaluation budget, including reserved cleanup time.
use crate::{InfrastructureError, InfrastructureReason};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExecutionControl {
    cancellation: Cancellation,
    work_deadline: Option<Instant>,
    final_deadline: Option<Instant>,
}

impl ExecutionControl {
    pub fn evaluation(timeout: Duration) -> Result<Self, InfrastructureError> {
        if timeout.is_zero() {
            return Err(invalid());
        }
        let final_deadline = Instant::now().checked_add(timeout).ok_or_else(invalid)?;
        let reserve = (timeout / 4).min(Duration::from_secs(2));
        let work_deadline = final_deadline.checked_sub(reserve).ok_or_else(invalid)?;
        Ok(Self {
            cancellation: Cancellation::default(),
            work_deadline: Some(work_deadline),
            final_deadline: Some(final_deadline),
        })
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
    pub fn final_deadline(&self) -> Option<Instant> {
        self.final_deadline
    }
    pub fn check_work(&self) -> Result<(), InfrastructureError> {
        if self.cancellation.is_cancelled() {
            return Err(failure(
                InfrastructureReason::Cancelled,
                "evaluation was cancelled",
            ));
        }
        if self
            .work_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(failure(
                InfrastructureReason::EvaluationDeadlineExceeded,
                "evaluation work budget expired; remaining time is reserved for cleanup",
            ));
        }
        Ok(())
    }
    pub fn check_cleanup(&self) -> Result<(), InfrastructureError> {
        if self
            .final_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(failure(
                InfrastructureReason::CleanupFailed,
                "evaluation deadline expired before cleanup completed",
            ));
        }
        Ok(())
    }
    pub fn check_result(&self) -> Result<(), InfrastructureError> {
        if self.cancellation.is_cancelled() {
            return Err(failure(
                InfrastructureReason::Cancelled,
                "evaluation was cancelled",
            ));
        }
        if self
            .final_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(failure(
                InfrastructureReason::EvaluationDeadlineExceeded,
                "evaluation deadline expired including cleanup",
            ));
        }
        Ok(())
    }
}
fn invalid() -> InfrastructureError {
    failure(
        InfrastructureReason::InvalidConfiguration,
        "evaluation timeout must be positive and representable by the host clock",
    )
}
fn failure(reason: InfrastructureReason, message: &str) -> InfrastructureError {
    InfrastructureError::new(reason, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn work_cancellation_does_not_disable_cleanup_and_deadlines_do_not_reset() {
        let control = ExecutionControl::evaluation(Duration::from_secs(1)).unwrap();
        let clone = control.clone();
        assert_eq!(control.final_deadline(), clone.final_deadline());
        control.cancellation().cancel();
        assert_eq!(
            clone.check_work().unwrap_err().reason_code(),
            InfrastructureReason::Cancelled
        );
        assert!(clone.check_cleanup().is_ok());
    }
    #[test]
    fn expired_work_budget_preserves_a_cleanup_window() {
        let control = ExecutionControl {
            work_deadline: Some(Instant::now()),
            final_deadline: Some(Instant::now() + Duration::from_secs(1)),
            ..ExecutionControl::default()
        };
        assert_eq!(
            control.check_work().unwrap_err().reason_code(),
            InfrastructureReason::EvaluationDeadlineExceeded
        );
        assert!(control.check_cleanup().is_ok());
        assert!(ExecutionControl::evaluation(Duration::ZERO).is_err());
    }
}
