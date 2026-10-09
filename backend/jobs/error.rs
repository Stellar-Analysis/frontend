//! Explicit retryable vs. terminal error taxonomy for background jobs.
//!
//! Generic retry-with-backoff cannot distinguish "failed because of a
//! transient issue" from "will fail every single time". Job handlers must
//! classify their failures explicitly so the scheduler can retry the former
//! and quarantine the latter immediately.

use std::fmt;

/// Classification of a job failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Transient failure: safe to retry with backoff.
    Retryable,
    /// Permanent failure: will fail on every retry, quarantine immediately.
    Terminal,
}

/// Error returned by a job handler.
#[derive(Debug, Clone)]
pub struct JobError {
    pub kind: ErrorKind,
    pub message: String,
}

impl JobError {
    /// A transient failure that should be retried with backoff.
    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Retryable,
            message: message.into(),
        }
    }

    /// A permanent failure that should be quarantined immediately.
    pub fn terminal(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Terminal,
            message: message.into(),
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.kind == ErrorKind::Retryable
    }

    pub fn is_terminal(&self) -> bool {
        self.kind == ErrorKind::Terminal
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.kind {
            ErrorKind::Retryable => "retryable",
            ErrorKind::Terminal => "terminal",
        };
        write!(f, "{} job error: {}", label, self.message)
    }
}

impl std::error::Error for JobError {}

/// Convenience alias for job handler results.
pub type JobResult<T> = Result<T, JobError>;
