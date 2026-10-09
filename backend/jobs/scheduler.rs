//! Retry-with-backoff scheduler for retryable failures and immediate
//! quarantine for terminal failures.

use std::time::Duration;

use super::dead_letter::{record_failure, AlertSink, DeadLetter, DeadLetterQueue, FailureRecord};
use super::error::JobError;

/// A unit of background work. Handlers must be idempotent: their writes are
/// upserts, so re-running after a partial failure never double-applies.
pub trait Job {
    fn name(&self) -> &str;
    fn payload(&self) -> String;
    fn run(&self) -> Result<(), JobError>;
}

/// Retry policy: maximum attempts and base backoff.
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_backoff: Duration::from_millis(100),
        }
    }
}

impl RetryPolicy {
    /// Exponential backoff for the given (1-based) attempt.
    pub fn backoff(&self, attempt: u32) -> Duration {
        let factor = 1u32 << attempt.saturating_sub(1).min(16);
        self.base_backoff * factor
    }
}

/// Outcome of running a job through the scheduler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Quarantined,
}

/// Runs jobs, retrying retryable failures with backoff and quarantining
/// terminal failures (or retryable failures that exhaust the retry ceiling).
pub struct Scheduler<'a> {
    policy: RetryPolicy,
    dead_letters: &'a DeadLetterQueue,
    alerts: &'a dyn AlertSink,
}

impl<'a> Scheduler<'a> {
    pub fn new(
        policy: RetryPolicy,
        dead_letters: &'a DeadLetterQueue,
        alerts: &'a dyn AlertSink,
    ) -> Self {
        Self {
            policy,
            dead_letters,
            alerts,
        }
    }

    /// Run a job, applying the retry/quarantine policy.
    pub fn run(&self, job: &dyn Job) -> Outcome {
        let mut history: Vec<FailureRecord> = Vec::new();

        for attempt in 1..=self.policy.max_attempts {
            match job.run() {
                Ok(()) => return Outcome::Succeeded,
                Err(error) => {
                    history.push(record_failure(attempt, &error));

                    if error.is_terminal() {
                        self.quarantine(job, history, "terminal error");
                        return Outcome::Quarantined;
                    }

                    if attempt < self.policy.max_attempts {
                        std::thread::sleep(self.policy.backoff(attempt));
                    }
                }
            }
        }

        self.quarantine(job, history, "retry ceiling exceeded");
        Outcome::Quarantined
    }

    fn quarantine(&self, job: &dyn Job, history: Vec<FailureRecord>, reason: &str) -> DeadLetter {
        self.dead_letters.quarantine(
            job.name(),
            &job.payload(),
            history,
            reason,
            self.alerts,
        )
    }
}
