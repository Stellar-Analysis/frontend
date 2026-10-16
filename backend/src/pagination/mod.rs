//! Collision-proof cursor pagination for batch-ingested ledger rows.
//!
//! # Why compound cursors
//!
//! Corridor / anchor / transaction tables are ledger-derived and often written
//! in batches. Many rows can share an identical ingestion timestamp after a
//! single ledger close. A cursor over timestamp alone will skip or duplicate
//! rows across page boundaries whenever two rows tie.
//!
//! # Cursor encoding
//!
//! The opaque cursor is URL-safe base64 (no padding) over a JSON object:
//!
//! ```json
//! { "ts": 1710000000123, "id": "42" }
//! ```
//!
//! - `ts` — ingestion timestamp in milliseconds since Unix epoch (i64).
//! - `id` — monotonic tiebreaker (autoincrement id, or `ledger_seq:row_idx`).
//!
//! Seek predicate for descending pages (`ORDER BY ts DESC, id DESC`):
//!
//! ```text
//! (ts, id) < (cursor.ts, cursor.id)
//! ```
//!
//! Ascending pages invert the comparison. Callers should maintain a compound
//! index on `(ts, id)` (or `(ingested_at, id)`) for efficient seeking.
//!
//! # Page limits
//!
//! A limit is an upper bound on the number of rows returned. After seeking past
//! a cursor, a limit larger than the remaining snapshot (including `usize::MAX`)
//! returns all remaining rows without arithmetic overflow. An exhausted cursor
//! returns an empty page. A zero limit retains its empty-page behavior.

mod cursor;

pub use cursor::{
    decode_cursor, encode_cursor, paginate, CompoundCursor, CursorKey, Page, PaginateError,
    SortDirection,
};
