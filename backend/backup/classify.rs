//! Classification of cache-held state for backup/restore.
//!
//! Every piece of cache-held state is classified as either pure cache
//! (derivable, safe to cold-start empty after a restore) or needs-real-backup
//! (non-reconstructable, must be tagged with the ingestion watermark and
//! restored).
//!
//! ## Classification
//!
//! | Cache state            | Class             | Rationale |
//! |------------------------|-------------------|-----------|
//! | `rate_limit:*`         | PureCache         | Rate-limit counters are derived from request traffic and reset naturally; restoring a stale snapshot could wrongly throttle or unthrottle clients. |
//! | `lock:*`               | PureCache         | Distributed lock state is ephemeral and tied to live holders; a stale lock snapshot could deadlock the restored service. |
//! | `metrics:derived:*`    | PureCache         | Ledger-derived metrics are recomputed from the datastore, the source of truth. |
//! | `idempotency:*`        | NeedsRealBackup   | Idempotency keys record which operations were already applied and are not reconstructable from the datastore alone. |
//! | `cursor:ingest`        | NeedsRealBackup   | The ingestion cursor is non-reconstructable progress state that must resume exactly where it left off. |

/// How a piece of cache-held state should be treated across backup/restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStateClass {
    /// Derivable from the datastore or naturally ephemeral. Safe to cold-start
    /// empty after a restore; never restored from a stale backup.
    PureCache,
    /// Non-reconstructable. Must be tagged with the ingestion watermark and
    /// restored, with watermark consistency checked against the datastore.
    NeedsRealBackup,
}

/// Classify a cache key by its namespace prefix.
///
/// Unknown keys default to [`CacheStateClass::PureCache`] so that a restore
/// never reintroduces state we cannot reason about; anything that genuinely
/// needs a real backup must be listed explicitly here.
pub fn classify(key: &str) -> CacheStateClass {
    match key.split(':').next().unwrap_or("") {
        "idempotency" | "cursor" => CacheStateClass::NeedsRealBackup,
        "rate_limit" | "lock" | "metrics" => CacheStateClass::PureCache,
        _ => CacheStateClass::PureCache,
    }
}
