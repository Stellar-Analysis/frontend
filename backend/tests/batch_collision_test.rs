//! Insert a batch of rows sharing an identical timestamp, paginate with a
//! small page size, and assert every row appears exactly once.

use stellar_insights_backend::pagination::{
    decode_cursor, paginate, CompoundCursor, CursorKey, SortDirection,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    ts: i64,
    id: String,
    label: String,
}

impl CursorKey for Row {
    fn cursor_ts(&self) -> i64 {
        self.ts
    }
    fn cursor_id(&self) -> &str {
        &self.id
    }
}

#[test]
fn batch_with_identical_timestamps_paginates_without_skip_or_dup() {
    let batch_ts = 1_720_000_000_000_i64;
    let rows: Vec<Row> = (0..12)
        .map(|i| Row {
            ts: batch_ts,
            id: format!("{i:04}"),
            label: format!("row-{i}"),
        })
        .collect();

    let page_size = 3;
    let mut seen = Vec::new();
    let mut cursor: Option<CompoundCursor> = None;

    loop {
        let page = paginate(&rows, page_size, cursor.as_ref(), SortDirection::Desc);
        assert!(page.items.len() <= page_size);
        for item in &page.items {
            seen.push(item.id.clone());
        }
        match page.next_cursor {
            Some(raw) => cursor = Some(decode_cursor(&raw).expect("cursor must decode")),
            None => break,
        }
    }

    seen.sort();
    let mut expected: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    expected.sort();
    assert_eq!(seen, expected, "every batched row must appear exactly once");
    assert_eq!(seen.len(), rows.len());
}

#[test]
fn mixed_timestamps_still_stable_across_pages() {
    let mut rows = Vec::new();
    for ts in [100_i64, 100, 100, 99, 99, 98] {
        let id = format!("{}-{}", ts, rows.len());
        rows.push(Row {
            ts,
            id: id.clone(),
            label: id,
        });
    }

    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = paginate(&rows, 2, cursor.as_ref(), SortDirection::Desc);
        seen.extend(page.items.iter().map(|r| r.id.clone()));
        match page.next_cursor {
            Some(raw) => cursor = Some(decode_cursor(&raw).unwrap()),
            None => break,
        }
    }
    assert_eq!(seen.len(), rows.len());
    let unique: std::collections::BTreeSet<_> = seen.iter().cloned().collect();
    assert_eq!(unique.len(), rows.len());
}

#[test]
fn oversized_page_limits_return_every_remaining_collision_row() {
    let rows: Vec<Row> = (0..5)
        .map(|i| Row {
            ts: 100,
            id: format!("{i:04}"),
            label: format!("row-{i}"),
        })
        .collect();

    for direction in [SortDirection::Asc, SortDirection::Desc] {
        let first = paginate(&rows, 2, None, direction);
        let cursor = decode_cursor(first.next_cursor.as_deref().unwrap()).unwrap();
        let expected = match direction {
            SortDirection::Asc => vec!["0002", "0003", "0004"],
            SortDirection::Desc => vec!["0002", "0001", "0000"],
        };

        for limit in [usize::MAX, usize::MAX - 1] {
            let page = paginate(&rows, limit, Some(&cursor), direction);
            let ids: Vec<&str> = page.items.iter().map(|row| row.id.as_str()).collect();
            assert_eq!(ids, expected);
            assert!(page.next_cursor.is_none());
        }
    }
}

#[test]
fn oversized_page_limit_at_an_exhausted_cursor_returns_an_empty_page() {
    let rows: Vec<Row> = (0..5)
        .map(|i| Row {
            ts: 100,
            id: format!("{i:04}"),
            label: format!("row-{i}"),
        })
        .collect();

    for direction in [SortDirection::Asc, SortDirection::Desc] {
        let all = paginate(&rows, rows.len(), None, direction);
        let last = all.items.last().unwrap();
        let cursor = CompoundCursor::new(last.ts, last.id.clone());
        let page = paginate(&rows, usize::MAX, Some(&cursor), direction);
        assert!(page.items.is_empty());
        assert!(page.next_cursor.is_none());
    }
}
