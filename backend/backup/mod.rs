//! Point-in-time consistent backup/restore across the datastore and cache.
//!
//! The primary datastore is the source of truth for ledger-derived metrics.
//! The cache layer holds rate-limit counters and distributed lock state. These
//! two stores do not share a transaction boundary, so a naive restore could
//! combine two snapshots taken at slightly different moments into a state that
//! never actually existed.
//!
//! To avoid that, every backup is tagged with the ingestion watermark (the last
//! ingested ledger sequence) recorded at snapshot time. On restore we refuse to
//! bring the service up if the datastore and cache watermarks disagree beyond a
//! documented tolerance. Cache state that is fully derivable is never restored
//! from a stale backup; it is cold-started empty instead.

pub mod classify;
pub mod watermark;

use classify::CacheStateClass;
use watermark::{Watermark, WatermarkMismatch};

/// Documented tolerance for watermark disagreement between the datastore and
/// the cache backup. A small skew is expected because the two snapshots are not
/// taken under a shared transaction boundary; anything larger means the two
/// backups cannot be safely combined.
pub const WATERMARK_TOLERANCE: u64 = 1;

/// A backup of the primary datastore, tagged with the ingestion watermark at
/// snapshot time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatastoreBackup {
    pub watermark: Watermark,
    pub payload: Vec<u8>,
}

/// A backup of cache-held state that is *not* derivable and therefore needs a
/// real backup. Derivable cache state is intentionally absent here: it is
/// cold-started empty on restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheBackup {
    pub watermark: Watermark,
    pub entries: Vec<(String, Vec<u8>)>,
}

/// A coordinated backup pairing the datastore snapshot with the cache snapshot
/// for state that needs real backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub datastore: DatastoreBackup,
    pub cache: CacheBackup,
}

/// Errors that can occur while taking or restoring a backup.
#[derive(Debug, PartialEq, Eq)]
pub enum BackupError {
    /// The datastore and cache watermarks disagree beyond the documented
    /// tolerance, so the two snapshots must not be combined.
    Inconsistent(WatermarkMismatch),
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupError::Inconsistent(m) => write!(f, "inconsistent backup: {m}"),
        }
    }
}

impl std::error::Error for BackupError {}

/// Take a coordinated backup of the datastore and the cache state that needs a
/// real backup, tagging both with the same ingestion watermark.
///
/// `cache_entries` should only contain state classified as
/// [`CacheStateClass::NeedsRealBackup`]; derivable state is dropped here so it
/// can never be reintroduced from a stale snapshot.
pub fn take_backup(
    watermark: Watermark,
    datastore_payload: Vec<u8>,
    cache_entries: Vec<(String, Vec<u8>)>,
) -> Backup {
    let cache_entries = cache_entries
        .into_iter()
        .filter(|(key, _)| classify::classify(key) == CacheStateClass::NeedsRealBackup)
        .collect();

    Backup {
        datastore: DatastoreBackup {
            watermark,
            payload: datastore_payload,
        },
        cache: CacheBackup {
            watermark,
            entries: cache_entries,
        },
    }
}

/// Validate that a backup's datastore and cache watermarks agree within the
/// documented tolerance. Returns the reconciled watermark on success.
pub fn check_consistency(backup: &Backup) -> Result<Watermark, BackupError> {
    backup
        .datastore
        .watermark
        .reconcile(&backup.cache.watermark, WATERMARK_TOLERANCE)
        .map_err(BackupError::Inconsistent)
}

/// Restore a backup, refusing to bring the service up if the two watermarks
/// disagree beyond the documented tolerance.
///
/// Derivable cache state is intentionally not restored: the caller cold-starts
/// it empty. Only entries classified as [`CacheStateClass::NeedsRealBackup`]
/// are returned for restoration.
pub fn restore(backup: &Backup) -> Result<RestoredState, BackupError> {
    let watermark = check_consistency(backup)?;

    let cache_entries = backup
        .cache
        .entries
        .iter()
        .filter(|(key, _)| classify::classify(key) == CacheStateClass::NeedsRealBackup)
        .cloned()
        .collect();

    Ok(RestoredState {
        watermark,
        datastore_payload: backup.datastore.payload.clone(),
        cache_entries,
    })
}

/// The state produced by a successful restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoredState {
    /// The reconciled ingestion watermark the restored service resumes from.
    pub watermark: Watermark,
    pub datastore_payload: Vec<u8>,
    /// Only cache entries that needed a real backup. Derivable cache state is
    /// cold-started empty by the caller.
    pub cache_entries: Vec<(String, Vec<u8>)>,
}
