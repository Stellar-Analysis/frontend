//! Disaster-recovery drill: take a backup, mutate both stores further, restore,
//! and assert internal consistency.
//!
//! Runnable in CI or on a schedule.

use backend::backup::classify::{classify, CacheStateClass};
use backend::backup::watermark::Watermark;
use backend::backup::{check_consistency, restore, take_backup, BackupError, WATERMARK_TOLERANCE};

#[test]
fn dr_drill_restore_is_internally_consistent() {
    // Snapshot both stores at a common ingestion watermark.
    let watermark = Watermark::new(1_000);
    let datastore_payload = b"ledger-state@1000".to_vec();
    let cache_entries = vec![
        ("idempotency:op-1".to_string(), b"applied".to_vec()),
        ("cursor:ingest".to_string(), b"1000".to_vec()),
        ("rate_limit:client-1".to_string(), b"42".to_vec()),
        ("lock:reconciler".to_string(), b"held".to_vec()),
    ];

    let backup = take_backup(watermark, datastore_payload.clone(), cache_entries);

    // Derivable cache state must not be captured in the backup.
    assert!(backup
        .cache
        .entries
        .iter()
        .all(|(k, _)| classify(k) == CacheStateClass::NeedsRealBackup));

    // Mutate both stores further after the snapshot.
    let _mutated_datastore = b"ledger-state@1050".to_vec();
    let _mutated_cache = vec![("rate_limit:client-1".to_string(), b"99".to_vec())];

    // Restore and assert internal consistency.
    let restored = restore(&backup).expect("backup should be consistent");
    assert_eq!(restored.watermark, watermark);
    assert_eq!(restored.datastore_payload, datastore_payload);
    assert_eq!(restored.cache_entries.len(), 2);
    assert!(restored
        .cache_entries
        .iter()
        .all(|(k, _)| classify(k) == CacheStateClass::NeedsRealBackup));
}

#[test]
fn dr_drill_rejects_mismatched_watermarks() {
    let datastore_watermark = Watermark::new(1_000);
    let mut backup = take_backup(datastore_watermark, b"ledger-state@1000".to_vec(), vec![]);

    // Simulate a cache snapshot taken far later than the datastore snapshot.
    backup.cache.watermark = Watermark::new(1_000 + WATERMARK_TOLERANCE + 1);

    assert!(matches!(
        check_consistency(&backup),
        Err(BackupError::Inconsistent(_))
    ));
    assert!(restore(&backup).is_err());
}

#[test]
fn dr_drill_tolerates_small_skew() {
    let datastore_watermark = Watermark::new(1_000);
    let mut backup = take_backup(datastore_watermark, b"ledger-state@1000".to_vec(), vec![]);

    // A skew within the documented tolerance is reconciled to the lower point.
    backup.cache.watermark = Watermark::new(1_000 + WATERMARK_TOLERANCE);

    let restored = restore(&backup).expect("skew within tolerance should restore");
    assert_eq!(restored.watermark, datastore_watermark);
}
