//! Periodic external anchoring of the audit log chain tip.
//!
//! Even with a cryptographic hash chain and database-level privilege separation,
//! a sufficiently privileged adversary with raw database superuser access could
//! rewrite the tail of `admin_audit_log` and recompute every subsequent hash.
//!
//! To defeat tail-rewrite attacks, [`AnchorScheduler`] periodically ships the
//! current chain-tip digest (`sequence`, `entry_hash`, `anchored_at_ms`) to an
//! external write-once sink ([`ExternalAnchorSink`])—such as an object-locked
//! WORM bucket or an on-chain Soroban commitment contract—under separate
//! credentials. Once anchored, any rewrite or truncation at or before the
//! anchored sequence is caught by [`super::verify::verify_with_anchors`].

use std::fmt;
use std::sync::Mutex;

use super::entry::AuditEntry;

/// An immutable commitment to the audit log tip at a specific sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnchorCheckpoint {
    /// The `sequence` of the entry whose hash was anchored.
    pub sequence: u64,
    /// The 64-character hex SHA-256 `entry_hash` at `sequence`.
    pub entry_hash: String,
    /// UTC timestamp in milliseconds when the commitment was shipped externally.
    pub anchored_at_ms: u64,
}

/// Errors produced when shipping or validating an external anchor commitment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorError {
    /// Attempted to anchor an empty log with no entries.
    EmptyAuditLog,
    /// A conflicting commitment already exists for this sequence in the
    /// write-once external sink.
    ConflictingCommitment {
        sequence: u64,
        existing_hash: String,
        attempted_hash: String,
    },
    /// The external storage sink encountered an operational or lock error.
    SinkUnavailable(String),
}

impl fmt::Display for AnchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnchorError::EmptyAuditLog => {
                write!(f, "cannot anchor an empty audit log")
            }
            AnchorError::ConflictingCommitment {
                sequence,
                existing_hash,
                attempted_hash,
            } => write!(
                f,
                "write-once anchor conflict at sequence {sequence}: existing={existing_hash}, attempted={attempted_hash}"
            ),
            AnchorError::SinkUnavailable(reason) => {
                write!(f, "external anchor sink unavailable: {reason}")
            }
        }
    }
}

impl std::error::Error for AnchorError {}

/// Interface for an external, write-once commitment sink (e.g. S3 Object Lock
/// in compliance mode or a Soroban on-chain commitment contract).
///
/// Implementations must enforce write-once semantics: once a sequence number is
/// anchored with a given `entry_hash`, subsequent attempts to overwrite that
/// sequence with a different hash must fail with
/// [`AnchorError::ConflictingCommitment`].
pub trait ExternalAnchorSink: Send + Sync {
    /// Persist a chain-tip commitment. Idempotent if the exact same
    /// `(sequence, entry_hash)` is submitted again; fails if a different hash
    /// was already committed for `sequence`.
    fn publish_checkpoint(&self, checkpoint: AnchorCheckpoint) -> Result<(), AnchorError>;

    /// Retrieve all committed checkpoints in ascending sequence order.
    fn checkpoints(&self) -> Result<Vec<AnchorCheckpoint>, AnchorError>;
}

/// In-memory write-once sink modeling an external WORM bucket or on-chain
/// Soroban commitment store.
#[derive(Default)]
pub struct WriteOnceAnchorSink {
    checkpoints: Mutex<Vec<AnchorCheckpoint>>,
}

impl WriteOnceAnchorSink {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ExternalAnchorSink for WriteOnceAnchorSink {
    fn publish_checkpoint(&self, checkpoint: AnchorCheckpoint) -> Result<(), AnchorError> {
        let mut guard = self
            .checkpoints
            .lock()
            .map_err(|_| AnchorError::SinkUnavailable("anchor sink mutex poisoned".to_string()))?;

        if let Some(existing) = guard.iter().find(|c| c.sequence == checkpoint.sequence) {
            if existing.entry_hash == checkpoint.entry_hash {
                return Ok(());
            }
            return Err(AnchorError::ConflictingCommitment {
                sequence: checkpoint.sequence,
                existing_hash: existing.entry_hash.clone(),
                attempted_hash: checkpoint.entry_hash,
            });
        }

        guard.push(checkpoint);
        guard.sort_by_key(|c| c.sequence);
        Ok(())
    }

    fn checkpoints(&self) -> Result<Vec<AnchorCheckpoint>, AnchorError> {
        let guard = self
            .checkpoints
            .lock()
            .map_err(|_| AnchorError::SinkUnavailable("anchor sink mutex poisoned".to_string()))?;
        Ok(guard.clone())
    }
}

/// Policy and coordinator for periodically shipping the chain-tip hash to an
/// [`ExternalAnchorSink`].
pub struct AnchorScheduler<'a> {
    every_n_entries: u64,
    sink: &'a dyn ExternalAnchorSink,
}

impl<'a> AnchorScheduler<'a> {
    /// Create a scheduler that anchors the chain tip every `every_n_entries`
    /// (minimum 1) or on explicit demand.
    pub fn new(every_n_entries: u64, sink: &'a dyn ExternalAnchorSink) -> Self {
        Self {
            every_n_entries: every_n_entries.max(1),
            sink,
        }
    }

    /// Returns `true` when the given `sequence` crosses a periodic anchor boundary.
    pub const fn should_anchor(&self, sequence: u64) -> bool {
        sequence > 0 && sequence.is_multiple_of(self.every_n_entries)
    }

    /// Unconditionally ship the current tip of `entries` to the external sink.
    pub fn anchor_tip(
        &self,
        entries: &[AuditEntry],
        anchored_at_ms: u64,
    ) -> Result<AnchorCheckpoint, AnchorError> {
        let tip = entries.last().ok_or(AnchorError::EmptyAuditLog)?;
        let checkpoint = AnchorCheckpoint {
            sequence: tip.sequence,
            entry_hash: tip.entry_hash.clone(),
            anchored_at_ms,
        };
        self.sink.publish_checkpoint(checkpoint.clone())?;
        Ok(checkpoint)
    }

    /// Anchor the tip if `entries.last()` hits the configured periodic cadence.
    pub fn maybe_anchor_tip(
        &self,
        entries: &[AuditEntry],
        anchored_at_ms: u64,
    ) -> Result<Option<AnchorCheckpoint>, AnchorError> {
        let Some(tip) = entries.last() else {
            return Ok(None);
        };
        if !self.should_anchor(tip.sequence) {
            return Ok(None);
        }
        let checkpoint = self.anchor_tip(entries, anchored_at_ms)?;
        Ok(Some(checkpoint))
    }
}
