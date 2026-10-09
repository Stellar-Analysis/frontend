//! Quarantine state, full failure history, and alerting for poison messages.
//!
//! After N retries or a terminal error a job is quarantined here rather than
//! silently dropped or retried forever. The full failure history is retained
//! so the input can be investigated.

use std::sync::Mutex;

use super::error::JobError;

/// A single recorded failure attempt.
#[derive(Debug, Clone)]
pub struct FailureRecord {
    pub attempt: u32,
    pub message: String,
    pub retryable: bool,
}

/// A quarantined job with its complete failure history.
#[derive(Debug, Clone)]
pub struct DeadLetter {
    pub job_name: String,
    pub payload: String,
    pub history: Vec<FailureRecord>,
    pub reason: String,
}

/// Sink notified when a job is quarantined.
pub trait AlertSink: Send + Sync {
    fn alert(&self, dead_letter: &DeadLetter);
}

/// Default alert sink that records alerts in memory (used by tests and as a
/// fallback when no external alerting channel is configured).
#[derive(Default)]
pub struct RecordingAlertSink {
    alerts: Mutex<Vec<DeadLetter>>,
}

impl RecordingAlertSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alerts(&self) -> Vec<DeadLetter> {
        self.alerts.lock().expect("alert sink poisoned").clone()
    }
}

impl AlertSink for RecordingAlertSink {
    fn alert(&self, dead_letter: &DeadLetter) {
        self.alerts
            .lock()
            .expect("alert sink poisoned")
            .push(dead_letter.clone());
    }
}

/// Quarantine store holding dead-lettered jobs.
#[derive(Default)]
pub struct DeadLetterQueue {
    entries: Mutex<Vec<DeadLetter>>,
}

impl DeadLetterQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Quarantine a job with its full failure history and raise an alert.
    pub fn quarantine(
        &self,
        job_name: &str,
        payload: &str,
        history: Vec<FailureRecord>,
        reason: &str,
        sink: &dyn AlertSink,
    ) -> DeadLetter {
        let entry = DeadLetter {
            job_name: job_name.to_string(),
            payload: payload.to_string(),
            history,
            reason: reason.to_string(),
        };
        sink.alert(&entry);
        self.entries
            .lock()
            .expect("dead letter queue poisoned")
            .push(entry.clone());
        entry
    }

    pub fn entries(&self) -> Vec<DeadLetter> {
        self.entries
            .lock()
            .expect("dead letter queue poisoned")
            .clone()
    }

    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .expect("dead letter queue poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Build a failure record from an attempt and its error.
pub fn record_failure(attempt: u32, error: &JobError) -> FailureRecord {
    FailureRecord {
        attempt,
        message: error.message.clone(),
        retryable: error.is_retryable(),
    }
}
