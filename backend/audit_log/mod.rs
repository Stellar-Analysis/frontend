//! Hash-chained, tamper-evident administrative audit log.
//!
//! Records privileged administrative actions (manual snapshot corrections,
//! secret rotations, feature-flag toggles, role changes) with three layered
//! tamper-evidence guarantees:
//!
//! 1. **Cryptographic hash chaining** ([`entry`]): every entry commits to the
//!    SHA-256 digest of the previous entry (`prev_hash`), so any retroactive
//!    `UPDATE` or `DELETE` breaks the chain visibly.
//! 2. **Database-level privilege separation** ([`DbRole`], [`ADMIN_AUDIT_LOG_DDL`]):
//!    ordinary application database credentials (`stellar_app_writer`) and the
//!    dedicated audit writer role (`stellar_audit_appender`) have `UPDATE`,
//!    `DELETE`, and `TRUNCATE` revoked at the SQL role level.
//! 3. **Periodic external anchoring** ([`anchor`]) and **on-demand incident
//!    verification** ([`verify`]): chain-tip hashes are periodically shipped to
//!    a write-once sink so even a raw database superuser cannot rewrite the tail
//!    of the log undetected.

pub mod anchor;
pub mod entry;
pub mod verify;

use std::fmt;

pub use anchor::{
    AnchorCheckpoint, AnchorError, AnchorScheduler, ExternalAnchorSink, WriteOnceAnchorSink,
};
pub use entry::{
    compute_entry_hash, sha256_bytes, sha256_hex, AdminAction, AuditEntry, GENESIS_HASH_HEX,
};
pub use verify::{
    verify_chain, verify_with_anchors, TamperReason, TamperViolation, VerificationReport,
};

/// Authoritative PostgreSQL DDL enforcing database-level role separation and
/// revoking `UPDATE`, `DELETE`, and `TRUNCATE` privileges on `admin_audit_log`.
pub const ADMIN_AUDIT_LOG_DDL: &str = include_str!("schema.sql");

/// Database roles with strictly separated privileges over `admin_audit_log`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbRole {
    /// General application write credentials (`stellar_app_writer`).
    /// Cannot mutate or delete audit log rows.
    AppWriter,
    /// Dedicated audit appender credentials (`stellar_audit_appender`).
    /// Permitted only to `INSERT` and `SELECT` on `admin_audit_log`.
    AuditAppender,
    /// Incident-response and compliance verifier (`stellar_audit_verifier`).
    /// Permitted only to `SELECT` on `admin_audit_log`.
    AuditVerifier,
}

/// Database operations checked by the privilege guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbOperation {
    Select,
    Insert,
    Update,
    Delete,
    Truncate,
}

impl DbRole {
    /// Returns whether this database role is permitted to perform `op` on
    /// `admin_audit_log`.
    ///
    /// No application or verifier role is ever granted `UPDATE`, `DELETE`, or
    /// `TRUNCATE`.
    pub const fn allows(self, op: DbOperation) -> bool {
        matches!(
            (self, op),
            (
                DbRole::AppWriter | DbRole::AuditVerifier,
                DbOperation::Select
            ) | (
                DbRole::AuditAppender,
                DbOperation::Select | DbOperation::Insert
            )
        )
    }
}

/// Errors returned when operating on [`AuditLog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditLogError {
    /// The caller's database role lacks privilege for the requested operation.
    PermissionDenied {
        role: DbRole,
        operation: DbOperation,
    },
    /// Propagated error from the external anchor sink.
    Anchor(AnchorError),
}

impl fmt::Display for AuditLogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditLogError::PermissionDenied { role, operation } => write!(
                f,
                "database role {role:?} is not permitted to perform {operation:?} on admin_audit_log"
            ),
            AuditLogError::Anchor(err) => write!(f, "audit log anchor error: {err}"),
        }
    }
}

impl std::error::Error for AuditLogError {}

impl From<AnchorError> for AuditLogError {
    fn from(err: AnchorError) -> Self {
        AuditLogError::Anchor(err)
    }
}

/// Append-only administrative audit log enforcing [`DbRole`] privileges and
/// cryptographic hash chaining.
#[derive(Debug, Default, Clone)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a new administrative action using `role`.
    ///
    /// Only [`DbRole::AuditAppender`] has `INSERT` privilege on `admin_audit_log`.
    pub fn append(
        &mut self,
        role: DbRole,
        action: AdminAction,
        actor: impl Into<String>,
        timestamp_ms: u64,
    ) -> Result<AuditEntry, AuditLogError> {
        if !role.allows(DbOperation::Insert) {
            return Err(AuditLogError::PermissionDenied {
                role,
                operation: DbOperation::Insert,
            });
        }

        let sequence = (self.entries.len() as u64) + 1;
        let prev_hash = self
            .entries
            .last()
            .map(|last| last.entry_hash.as_str())
            .unwrap_or(GENESIS_HASH_HEX)
            .to_string();

        let entry = AuditEntry::new(sequence, prev_hash, action, actor, timestamp_ms);
        self.entries.push(entry.clone());
        Ok(entry)
    }

    /// Attempt to update an existing entry via application database credentials.
    ///
    /// Always rejected at the privilege boundary (`REVOKE UPDATE ON admin_audit_log`).
    pub fn update_entry(
        &mut self,
        role: DbRole,
        _sequence: u64,
        _new_action: AdminAction,
    ) -> Result<(), AuditLogError> {
        Err(AuditLogError::PermissionDenied {
            role,
            operation: DbOperation::Update,
        })
    }

    /// Attempt to delete an existing entry via application database credentials.
    ///
    /// Always rejected at the privilege boundary (`REVOKE DELETE ON admin_audit_log`).
    pub fn delete_entry(&mut self, role: DbRole, _sequence: u64) -> Result<(), AuditLogError> {
        Err(AuditLogError::PermissionDenied {
            role,
            operation: DbOperation::Delete,
        })
    }

    /// Attempt to truncate the audit log via application database credentials.
    ///
    /// Always rejected at the privilege boundary (`REVOKE TRUNCATE ON admin_audit_log`).
    pub fn truncate(&mut self, role: DbRole) -> Result<(), AuditLogError> {
        Err(AuditLogError::PermissionDenied {
            role,
            operation: DbOperation::Truncate,
        })
    }

    /// Read entries under the given [`DbRole`], checking `SELECT` privilege.
    pub fn read_entries(&self, role: DbRole) -> Result<&[AuditEntry], AuditLogError> {
        if !role.allows(DbOperation::Select) {
            return Err(AuditLogError::PermissionDenied {
                role,
                operation: DbOperation::Select,
            });
        }
        Ok(&self.entries)
    }

    /// Read-only view of all recorded entries.
    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    /// Verify the internal hash chain from genesis.
    pub fn verify(&self) -> Result<VerificationReport, TamperViolation> {
        verify_chain(&self.entries)
    }

    /// Verify the internal hash chain and compare against external [`AnchorCheckpoint`]s.
    pub fn verify_against_anchors(
        &self,
        anchors: &[AnchorCheckpoint],
    ) -> Result<VerificationReport, TamperViolation> {
        verify_with_anchors(&self.entries, anchors)
    }

    /// Simulate raw database superuser tampering bypassing role privileges.
    ///
    /// Provided so integration tests (`tamper_detection_test.rs`) and security
    /// drills can mutate or delete underlying rows and verify that [`verify`]
    /// and [`verify_against_anchors`] detect the tampering.
    pub fn raw_entries_mut(&mut self) -> &mut Vec<AuditEntry> {
        &mut self.entries
    }
}
