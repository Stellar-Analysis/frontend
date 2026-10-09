//! Idempotent, dead-letter-aware background job runner.
//!
//! Jobs are built so their writes are upserts (safe to repeat), failures are
//! classified as retryable or terminal, and poison messages are quarantined
//! with full failure history and an alert instead of looping forever.

pub mod backfill;
pub mod dead_letter;
pub mod error;
pub mod scheduler;

pub use backfill::{BackfillJob, Row, UpsertStore};
pub use dead_letter::{AlertSink, DeadLetter, DeadLetterQueue, FailureRecord, RecordingAlertSink};
pub use error::{ErrorKind, JobError, JobResult};
pub use scheduler::{Job, Outcome, RetryPolicy, Scheduler};
