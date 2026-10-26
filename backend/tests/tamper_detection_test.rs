//! Integration tests proving that retroactive edits, row deletions, privilege
//! violations, and tail rewrites on the admin audit log are detected.

#[path = "../audit_log/mod.rs"]
mod audit_log;

use audit_log::{
    compute_entry_hash, sha256_bytes, sha256_hex, verify_chain, verify_with_anchors, AdminAction,
    AnchorCheckpoint, AnchorError, AnchorScheduler, AuditEntry, AuditLog, AuditLogError,
    DbOperation, DbRole, ExternalAnchorSink, TamperReason, TamperViolation, VerificationReport,
    WriteOnceAnchorSink, ADMIN_AUDIT_LOG_DDL, GENESIS_HASH_HEX,
};

fn sample_log() -> AuditLog {
    let mut log = AuditLog::new();
    log.append(
        DbRole::AuditAppender,
        AdminAction::ManualSnapshotCorrection {
            epoch: 42,
            corridor_id: "USDC-XLM".to_string(),
            reason: "backfill missing horizon ledger 1042".to_string(),
        },
        "admin:alice",
        1_700_000_001_000,
    )
    .expect("audit appender can insert entry 1");

    log.append(
        DbRole::AuditAppender,
        AdminAction::SecretRotation {
            secret_name: "HORIZON_INGEST_HMAC".to_string(),
            new_key_version: 3,
        },
        "admin:bob",
        1_700_000_002_000,
    )
    .expect("audit appender can insert entry 2");

    log.append(
        DbRole::AuditAppender,
        AdminAction::FeatureFlagToggle {
            flag_key: "enable_live_corridor_heatmap".to_string(),
            enabled: true,
        },
        "admin:carol",
        1_700_000_003_000,
    )
    .expect("audit appender can insert entry 3");

    log
}

#[test]
fn intact_hash_chain_passes_verification() {
    // NIST / RFC 6234 SHA-256 vector check ("abc").
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(sha256_bytes(b"abc")[0], 0xba);

    let log = sample_log();
    let entries = log
        .read_entries(DbRole::AuditVerifier)
        .expect("verifier role can read entries");
    assert_eq!(entries[0].prev_hash, GENESIS_HASH_HEX);
    assert_eq!(entries[1].prev_hash, entries[0].entry_hash);
    assert_eq!(entries[2].prev_hash, entries[1].entry_hash);
    assert_eq!(
        entries[0].entry_hash,
        compute_entry_hash(
            entries[0].sequence,
            &entries[0].prev_hash,
            &entries[0].action,
            &entries[0].actor,
            entries[0].timestamp_ms,
        )
    );
    assert!(!entries[0].action.to_string().is_empty());

    let report: VerificationReport = log.verify().expect("intact chain must verify");
    assert_eq!(report.verified_entries, 3);
    assert_eq!(report.tip_hash, entries[2].entry_hash);
}

#[test]
fn retroactive_edit_to_entry_payload_is_detected_at_exact_index() {
    let mut log = sample_log();

    // Simulate a raw DB edit to row 2's actor field without recomputing hashes.
    log.raw_entries_mut()[1].actor = "admin:mallory".to_string();

    let err = log.verify().expect_err("tampered actor must be detected");
    assert_eq!(err.index, 1);
    assert_eq!(err.sequence, 2);
    assert!(matches!(err.reason, TamperReason::EntryHashMismatch { .. }));
}

#[test]
fn retroactive_edit_with_recomputed_single_row_hash_breaks_next_entry_prev_hash() {
    let mut log = sample_log();

    // Attacker modifies entry 2's action AND recomputes entry 2's own entry_hash,
    // but does not rewrite downstream entries.
    log.raw_entries_mut()[1].action = AdminAction::FeatureFlagToggle {
        flag_key: "disable_audit_alerts".to_string(),
        enabled: true,
    };
    let forged_hash = log.raw_entries_mut()[1].recompute_hash();
    log.raw_entries_mut()[1].entry_hash = forged_hash;

    let err = verify_chain(log.entries())
        .expect_err("downstream prev_hash link must break when entry 2 is rewritten");
    assert_eq!(err.index, 2);
    assert_eq!(err.sequence, 3);
    assert!(matches!(err.reason, TamperReason::PrevHashMismatch { .. }));
}

#[test]
fn deleting_middle_row_is_detected_as_sequence_gap() {
    let mut log = sample_log();
    log.raw_entries_mut().remove(1);

    let err = log
        .verify()
        .expect_err("deleted row must trigger sequence discontinuity");
    assert_eq!(err.index, 1);
    assert_eq!(err.sequence, 3);
    assert_eq!(
        err.reason,
        TamperReason::SequenceGap {
            expected_sequence: 2,
            actual_sequence: 3,
        }
    );
}

#[test]
fn external_anchor_detects_full_tail_rewrite_and_truncation() {
    let mut log = sample_log();
    let sink = WriteOnceAnchorSink::new();
    let scheduler = AnchorScheduler::new(3, &sink);

    // Periodically ship the chain-tip hash at sequence 3 to the external sink.
    let checkpoint = scheduler
        .maybe_anchor_tip(log.entries(), 1_700_000_004_000)
        .expect("anchoring must succeed")
        .expect("sequence 3 hits cadence of 3");
    assert_eq!(checkpoint.sequence, 3);

    // An attacker with raw DB superuser access rewrites entry 2 AND recomputes
    // both entry 2 and entry 3 so the internal chain appears self-consistent.
    log.raw_entries_mut()[1].actor = "admin:root-attacker".to_string();
    let new_hash_2 = log.raw_entries_mut()[1].recompute_hash();
    log.raw_entries_mut()[1].entry_hash = new_hash_2.clone();
    log.raw_entries_mut()[2].prev_hash = new_hash_2;
    let new_hash_3 = log.raw_entries_mut()[2].recompute_hash();
    log.raw_entries_mut()[2].entry_hash = new_hash_3;

    // Internal chain check alone passes because the attacker recomputed the tail...
    assert!(log.verify().is_ok());

    // ...but verification against the external write-once anchor catches the rewrite!
    let anchors = sink.checkpoints().expect("read checkpoints");
    let err = log
        .verify_against_anchors(&anchors)
        .expect_err("external anchor must detect tail rewrite");
    assert_eq!(err.index, 2);
    assert_eq!(err.sequence, 3);
    assert!(matches!(err.reason, TamperReason::AnchorMismatch { .. }));

    // Attempting to overwrite the existing checkpoint in the write-once sink also fails.
    assert!(scheduler
        .anchor_tip(log.entries(), 1_700_000_005_000)
        .is_err());

    // Deleting the anchored tail entry is detected as TruncatedBelowAnchor.
    log.raw_entries_mut().pop();
    let trunc_err = verify_with_anchors(log.entries(), &anchors)
        .expect_err("truncation below anchored sequence must fail");
    assert_eq!(
        trunc_err.reason,
        TamperReason::TruncatedBelowAnchor {
            anchored_sequence: 3,
            latest_sequence: 2,
        }
    );
}

#[test]
fn database_role_privileges_prevent_ordinary_credentials_from_mutating_rows() {
    let mut log = sample_log();

    // General application credentials cannot even INSERT directly into admin_audit_log,
    // nor can they UPDATE or DELETE.
    assert_eq!(
        log.append(
            DbRole::AppWriter,
            AdminAction::RoleChange {
                principal: "user:1".to_string(),
                role: "admin".to_string(),
                granted: true,
            },
            "app-service",
            1_700_000_010_000,
        ),
        Err(AuditLogError::PermissionDenied {
            role: DbRole::AppWriter,
            operation: DbOperation::Insert,
        })
    );

    for role in [
        DbRole::AppWriter,
        DbRole::AuditAppender,
        DbRole::AuditVerifier,
    ] {
        assert_eq!(
            log.update_entry(
                role,
                1,
                AdminAction::FeatureFlagToggle {
                    flag_key: "tampered".to_string(),
                    enabled: false,
                },
            ),
            Err(AuditLogError::PermissionDenied {
                role,
                operation: DbOperation::Update,
            })
        );
        assert_eq!(
            log.delete_entry(role, 1),
            Err(AuditLogError::PermissionDenied {
                role,
                operation: DbOperation::Delete,
            })
        );
        assert_eq!(
            log.truncate(role),
            Err(AuditLogError::PermissionDenied {
                role,
                operation: DbOperation::Truncate,
            })
        );
    }

    // Exercise Display / Error conversions for AnchorError, AuditLogError, and TamperViolation.
    let anchor_err = AuditLogError::from(AnchorError::EmptyAuditLog);
    assert!(anchor_err.to_string().contains("empty audit log"));
    let manual_entry = AuditEntry::new(
        1,
        "bad_genesis",
        AdminAction::SecretRotation {
            secret_name: "k".to_string(),
            new_key_version: 1,
        },
        "actor",
        100,
    );
    let genesis_violation: TamperViolation =
        verify_chain(&[manual_entry]).expect_err("non-genesis first entry must fail");
    assert!(matches!(
        genesis_violation.reason,
        TamperReason::InvalidGenesisPrevHash { .. }
    ));
    assert!(genesis_violation.to_string().contains("sequence 1"));
    let cp = AnchorCheckpoint {
        sequence: 1,
        entry_hash: GENESIS_HASH_HEX.to_string(),
        anchored_at_ms: 100,
    };
    assert_eq!(cp.sequence, 1);

    // Verify SQL DDL revokes UPDATE, DELETE, TRUNCATE from application roles.
    assert!(ADMIN_AUDIT_LOG_DDL.contains(
        "REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON TABLE admin_audit_log FROM stellar_app_writer"
    ));
    assert!(ADMIN_AUDIT_LOG_DDL.contains(
        "REVOKE UPDATE, DELETE, TRUNCATE ON TABLE admin_audit_log FROM stellar_audit_appender"
    ));
    assert!(ADMIN_AUDIT_LOG_DDL.contains("CREATE TRIGGER trg_admin_audit_log_immutable"));
}
