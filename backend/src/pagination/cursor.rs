use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Opaque compound cursor: `(timestamp_ms, id)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompoundCursor {
    /// Ingestion / event timestamp in milliseconds since Unix epoch.
    pub ts: i64,
    /// Monotonic tiebreaker unique within the result set.
    pub id: String,
}

impl CompoundCursor {
    #[must_use]
    pub fn new(ts: i64, id: impl Into<String>) -> Self {
        Self {
            ts,
            id: id.into(),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PaginateError {
    #[error("invalid cursor encoding")]
    InvalidEncoding,
    #[error("invalid cursor payload")]
    InvalidPayload,
}

/// Encode a compound cursor as an opaque URL-safe string.
#[must_use]
pub fn encode_cursor(cursor: &CompoundCursor) -> String {
    let json = serde_json::to_vec(cursor).unwrap_or_else(|_| b"{}".to_vec());
    URL_SAFE_NO_PAD.encode(json)
}

/// Decode an opaque cursor back into `(ts, id)`.
pub fn decode_cursor(raw: &str) -> Result<CompoundCursor, PaginateError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw.trim())
        .map_err(|_| PaginateError::InvalidEncoding)?;
    serde_json::from_slice(&bytes).map_err(|_| PaginateError::InvalidPayload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A page of items plus the next opaque cursor (when more rows remain).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

/// Row contract for pagination: expose the compound key components.
pub trait CursorKey {
    fn cursor_ts(&self) -> i64;
    fn cursor_id(&self) -> &str;
}

impl CursorKey for CompoundCursor {
    fn cursor_ts(&self) -> i64 {
        self.ts
    }
    fn cursor_id(&self) -> &str {
        &self.id
    }
}

fn comes_after(row: &impl CursorKey, cursor: &CompoundCursor, dir: SortDirection) -> bool {
    match dir {
        SortDirection::Desc => {
            row.cursor_ts() < cursor.ts
                || (row.cursor_ts() == cursor.ts && row.cursor_id() < cursor.id.as_str())
        }
        SortDirection::Asc => {
            row.cursor_ts() > cursor.ts
                || (row.cursor_ts() == cursor.ts && row.cursor_id() > cursor.id.as_str())
        }
    }
}

fn cmp_keys(a: &impl CursorKey, b: &impl CursorKey, dir: SortDirection) -> std::cmp::Ordering {
    let ord = a
        .cursor_ts()
        .cmp(&b.cursor_ts())
        .then_with(|| a.cursor_id().cmp(b.cursor_id()));
    match dir {
        SortDirection::Asc => ord,
        SortDirection::Desc => ord.reverse(),
    }
}

/// Paginate an in-memory snapshot with a compound cursor.
///
/// `rows` must already be filtered to the query; this helper sorts stably by
/// `(ts, id)` and seeks past `cursor` when present. Designed so unit tests can
/// prove collision behaviour without a live database. Sorting borrows the input
/// rows; only the returned page is cloned, leaving the input order untouched.
pub fn paginate<T: CursorKey + Clone>(
    rows: &[T],
    limit: usize,
    cursor: Option<&CompoundCursor>,
    direction: SortDirection,
) -> Page<T> {
    let mut ordered: Vec<&T> = rows.iter().collect();
    ordered.sort_by(|a, b| cmp_keys(*a, *b, direction));

    let start = match cursor {
        Some(c) => ordered
            .iter()
            .position(|row| comes_after(*row, c, direction))
            .unwrap_or(ordered.len()),
        None => 0,
    };

    let end = start.saturating_add(limit).min(ordered.len());
    let items: Vec<T> = ordered[start..end].iter().map(|&row| row.clone()).collect();
    let next_cursor = if end < ordered.len() {
        items.last().map(|row| {
            encode_cursor(&CompoundCursor::new(
                row.cursor_ts(),
                row.cursor_id().to_string(),
            ))
        })
    } else {
        None
    };

    Page { items, next_cursor }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let c = CompoundCursor::new(1_710_000_000_123, "42");
        let raw = encode_cursor(&c);
        assert_eq!(decode_cursor(&raw).unwrap(), c);
    }

    #[test]
    fn rejects_garbage_cursor() {
        assert_eq!(decode_cursor("%%%"), Err(PaginateError::InvalidEncoding));
        let junk = URL_SAFE_NO_PAD.encode(b"not-json");
        assert_eq!(decode_cursor(&junk), Err(PaginateError::InvalidPayload));
    }

    #[test]
    fn paginate_clones_only_returned_rows() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Row {
            key: CompoundCursor,
            payload: String,
            clones: Rc<Cell<usize>>,
        }

        impl Clone for Row {
            fn clone(&self) -> Self {
                self.clones.set(self.clones.get() + 1);
                Self {
                    key: self.key.clone(),
                    payload: self.payload.clone(),
                    clones: Rc::clone(&self.clones),
                }
            }
        }

        impl CursorKey for Row {
            fn cursor_ts(&self) -> i64 {
                self.key.ts
            }

            fn cursor_id(&self) -> &str {
                &self.key.id
            }
        }

        let clones = Rc::new(Cell::new(0));
        let rows: Vec<Row> = [(300, "c"), (100, "a"), (400, "e"), (300, "d"), (200, "b")]
            .into_iter()
            .map(|(ts, id)| Row {
                key: CompoundCursor::new(ts, id),
                payload: format!("payload-{id}"),
                clones: Rc::clone(&clones),
            })
            .collect();
        let cases = [
            (SortDirection::Asc, None, 2, vec!["a", "b"], true),
            (SortDirection::Desc, None, 2, vec!["e", "d"], true),
            (
                SortDirection::Asc,
                Some(CompoundCursor::new(300, "c")),
                1,
                vec!["d"],
                true,
            ),
            (
                SortDirection::Desc,
                Some(CompoundCursor::new(300, "d")),
                1,
                vec!["c"],
                true,
            ),
            (
                SortDirection::Asc,
                Some(CompoundCursor::new(400, "e")),
                10,
                vec![],
                false,
            ),
            (SortDirection::Asc, None, 0, vec![], false),
            (
                SortDirection::Asc,
                Some(CompoundCursor::new(200, "b")),
                usize::MAX,
                vec!["c", "d", "e"],
                false,
            ),
        ];
        for (direction, cursor, limit, expected_ids, has_next) in cases {
            clones.set(0);
            let page = paginate(&rows, limit, cursor.as_ref(), direction);
            let ids: Vec<&str> = page.items.iter().map(|row| row.cursor_id()).collect();
            assert_eq!(ids, expected_ids);
            assert_eq!(clones.get(), page.items.len());
            assert_eq!(page.next_cursor.is_some(), has_next);
            for row in &page.items {
                assert_eq!(row.payload, format!("payload-{}", row.key.id));
            }
            if let Some(next) = &page.next_cursor {
                assert_eq!(decode_cursor(next).unwrap(), page.items.last().unwrap().key);
            }
            let original_ids: Vec<&str> = rows.iter().map(|row| row.cursor_id()).collect();
            assert_eq!(original_ids, ["c", "a", "e", "d", "b"]);
        }
    }
}
