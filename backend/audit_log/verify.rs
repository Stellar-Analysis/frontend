//! On-demand hash-chain and external-anchor verification for incident response.
//!
//! # Incident-Response Verification Guide
//!
//! During a security review, compliance audit, or suspected credential
//! compromise, responders can run [`verify_chain`] (or [`verify_with_anchors`]
//! when external checkpoints are available) against an ordered dump of
//! `admin_audit_log` rows.
//!
//! ## Step-by-step responder workflow
//!
//! 1. **Fetch rows using the read-only verifier role** (`stellar_audit_verifier`)
//!    ordered by `sequence ASC`.
//! 2. **Fetch immutable checkpoints** from the external write-once sink
//!    (S3 Object Lock / Soroban commitment storage).
//! 3. **Invoke [`verify_with_anchors`]** and inspect the result:
//!    - `Ok(VerificationReport)` confirms that every entry from sequence `1`
//!      through `verified_entries` is intact and matches all external anchors.
//!    - `Err(TamperViolation)` pinpoints the exact zero-based `index`, the
//!      affected `sequence`, and the [`TamperReason`] classification:
//!      - [`TamperReason::InvalidGenesisPrevHash`]: the first entry does not
//!        chain from [`GENESIS_HASH_HEX`] (e.g. prefix rows were deleted and
//!        sequence numbers renumbered, or the genesis row was forged).
//!      - [`TamperReason::SequenceGap`]: one or more rows were deleted or
//!        reordered (`expected_sequence` vs. `actual_sequence`).
//!      - [`TamperReason::EntryHashMismatch`]: a row's payload (`action`,
//!        `actor`, `timestamp_ms`, `sequence`, or `prev_hash`) was mutated in
//!        place without recomputing `entry_hash`.
//!      - [`TamperReason::PrevHashMismatch`]: a row's hash was recomputed after
//!        an edit, or a row was spliced out, breaking the link from entry
//!        `i - 1` to entry `i`.
//!      - [`TamperReason::AnchorMismatch`]: an attacker with raw DB access
//!        rewrote the tail of the log and recomputed downstream hashes,
//!        diverging from the external write-once checkpoint.
//!      - [`TamperReason::TruncatedBelowAnchor`]: trailing rows were deleted
//!        after an external checkpoint had already committed to a higher
//!        sequence number.

use std::fmt;

use super::anchor::AnchorCheckpoint;
use super::entry::{AuditEntry, GENESIS_HASH_HEX};

/// Summary returned when the entire audit log chain passes verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    /// Total number of entries verified from genesis.
    pub verified_entries: usize,
    /// The `entry_hash` of the latest verified entry, or [`GENESIS_HASH_HEX`]
    /// if the log is empty.
    pub tip_hash: String,
    /// Number of external anchor checkpoints verified against the chain.
    pub verified_anchors: usize,
}

/// Specific cause of the first detected integrity violation in the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TamperReason {
    /// The first entry does not reference [`GENESIS_HASH_HEX`] as its `prev_hash`.
    InvalidGenesisPrevHash { actual_prev_hash: String },
    /// Entries are not strictly contiguous 1-based sequence numbers.
    SequenceGap {
        expected_sequence: u64,
        actual_sequence: u64,
    },
    /// An entry's stored `prev_hash` does not match the preceding entry's `entry_hash`.
    PrevHashMismatch {
        expected_prev_hash: String,
        actual_prev_hash: String,
    },
    /// An entry's stored `entry_hash` does not match the recomputed SHA-256 of its fields.
    EntryHashMismatch {
        expected_hash: String,
        actual_hash: String,
    },
    /// An entry's `entry_hash` disagrees with an external write-once anchor commitment.
    AnchorMismatch {
        anchored_hash: String,
        actual_hash: String,
    },
    /// The audit log ends before a sequence number that was already anchored externally.
    TruncatedBelowAnchor {
        anchored_sequence: u64,
        latest_sequence: u64,
    },
}

/// Detailed diagnostic describing the first point of inconsistency in the audit log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TamperViolation {
    /// Zero-based index into the inspected slice where inconsistency was first observed.
    pub index: usize,
    /// Sequence number associated with the violation.
    pub sequence: u64,
    /// Classification of the integrity failure.
    pub reason: TamperReason,
}

impl fmt::Display for TamperViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "audit log tamper detected at index {} (sequence {}): {:?}",
            self.index, self.sequence, self.reason
        )
    }
}

impl std::error::Error for TamperViolation {}

/// Walk the hash chain from genesis and return the first point of inconsistency,
/// if any.
pub fn verify_chain(entries: &[AuditEntry]) -> Result<VerificationReport, TamperViolation> {
    let mut expected_prev_hash = GENESIS_HASH_HEX;

    for (index, entry) in entries.iter().enumerate() {
        let expected_sequence = (index as u64) + 1;
        if entry.sequence != expected_sequence {
            return Err(TamperViolation {
                index,
                sequence: entry.sequence,
                reason: TamperReason::SequenceGap {
                    expected_sequence,
                    actual_sequence: entry.sequence,
                },
            });
        }

        if index == 0 && entry.prev_hash != GENESIS_HASH_HEX {
            return Err(TamperViolation {
                index: 0,
                sequence: entry.sequence,
                reason: TamperReason::InvalidGenesisPrevHash {
                    actual_prev_hash: entry.prev_hash.clone(),
                },
            });
        }

        if entry.prev_hash != expected_prev_hash {
            return Err(TamperViolation {
                index,
                sequence: entry.sequence,
                reason: TamperReason::PrevHashMismatch {
                    expected_prev_hash: expected_prev_hash.to_string(),
                    actual_prev_hash: entry.prev_hash.clone(),
                },
            });
        }

        let recomputed = entry.recompute_hash();
        if entry.entry_hash != recomputed {
            return Err(TamperViolation {
                index,
                sequence: entry.sequence,
                reason: TamperReason::EntryHashMismatch {
                    expected_hash: recomputed,
                    actual_hash: entry.entry_hash.clone(),
                },
            });
        }

        expected_prev_hash = &entry.entry_hash;
    }

    Ok(VerificationReport {
        verified_entries: entries.len(),
        tip_hash: expected_prev_hash.to_string(),
        verified_anchors: 0,
    })
}

/// Verify both the internal hash chain and its consistency with external
/// write-once [`AnchorCheckpoint`]s.
///
/// This catches full-tail rewrites (where an attacker with raw DB access edits
/// an entry and recomputes all subsequent hashes) as well as tail truncations.
pub fn verify_with_anchors(
    entries: &[AuditEntry],
    anchors: &[AnchorCheckpoint],
) -> Result<VerificationReport, TamperViolation> {
    let mut report = verify_chain(entries)?;
    let latest_sequence = entries.len() as u64;

    for anchor in anchors {
        if anchor.sequence == 0 || anchor.sequence > latest_sequence {
            return Err(TamperViolation {
                index: entries.len(),
                sequence: anchor.sequence,
                reason: TamperReason::TruncatedBelowAnchor {
                    anchored_sequence: anchor.sequence,
                    latest_sequence,
                },
            });
        }

        let index = (anchor.sequence - 1) as usize;
        let entry = &entries[index];
        if entry.entry_hash != anchor.entry_hash {
            return Err(TamperViolation {
                index,
                sequence: entry.sequence,
                reason: TamperReason::AnchorMismatch {
                    anchored_hash: anchor.entry_hash.clone(),
                    actual_hash: entry.entry_hash.clone(),
                },
            });
        }
    }

    report.verified_anchors = anchors.len();
    Ok(report)
}
