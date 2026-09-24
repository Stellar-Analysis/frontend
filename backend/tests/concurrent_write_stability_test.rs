//! Paginate through a list while a background task inserts new matching rows.
//! Already-issued cursors' results must remain stable (no reshuffle / dup of
//! rows that were already returned).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use stellar_insights_backend::pagination::{
    decode_cursor, paginate, CompoundCursor, CursorKey, SortDirection,
};
use tokio::sync::Barrier;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    ts: i64,
    id: String,
}

impl CursorKey for Row {
    fn cursor_ts(&self) -> i64 {
        self.ts
    }
    fn cursor_id(&self) -> &str {
        &self.id
    }
}

#[tokio::test]
async fn issued_cursors_remain_stable_under_concurrent_inserts() {
    let shared = Arc::new(Mutex::new(
        (0..8)
            .map(|i| Row {
                ts: 5_000,
                id: format!("base-{i:03}"),
            })
            .collect::<Vec<_>>(),
    ));

    let barrier = Arc::new(Barrier::new(2));

    let writer_rows = Arc::clone(&shared);
    let writer_barrier = Arc::clone(&barrier);
    let writer = tokio::spawn(async move {
        writer_barrier.wait().await;
        for i in 0..8 {
            {
                let mut guard = writer_rows.lock().expect("lock");
                guard.push(Row {
                    ts: 5_000,
                    id: format!("new-{i:03}"),
                });
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });

    let reader_rows = Arc::clone(&shared);
    let reader_barrier = Arc::clone(&barrier);
    let reader = tokio::spawn(async move {
        reader_barrier.wait().await;

        // Snapshot the first page and remember its next cursor.
        let first_snapshot = {
            let guard = reader_rows.lock().expect("lock");
            paginate(&guard, 3, None, SortDirection::Desc)
        };
        assert_eq!(first_snapshot.items.len(), 3);
        let first_ids: Vec<String> = first_snapshot
            .items
            .iter()
            .map(|r| r.id.clone())
            .collect();
        let cursor = first_snapshot
            .next_cursor
            .as_deref()
            .map(decode_cursor)
            .transpose()
            .expect("cursor")
            .expect("next cursor");

        // Allow the writer to land some inserts, then re-read from the same cursor.
        tokio::time::sleep(Duration::from_millis(20)).await;

        let second = {
            let guard = reader_rows.lock().expect("lock");
            paginate(&guard, 3, Some(&cursor), SortDirection::Desc)
        };

        // Stability: nothing from the first page may reappear after the cursor.
        for id in &first_ids {
            assert!(
                second.items.iter().all(|r| &r.id != id),
                "cursor must not resurface already-returned id {id}"
            );
        }

        // Continuing to exhaustion from the original cursor should never
        // duplicate any id within the continuation stream.
        let mut continuation = second.items.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        let mut walk: Option<CompoundCursor> = second
            .next_cursor
            .as_deref()
            .map(decode_cursor)
            .transpose()
            .unwrap();
        while let Some(c) = walk {
            let page = {
                let guard = reader_rows.lock().expect("lock");
                paginate(&guard, 3, Some(&c), SortDirection::Desc)
            };
            for item in &page.items {
                assert!(
                    !continuation.contains(&item.id),
                    "duplicate {} in continuation",
                    item.id
                );
                assert!(
                    !first_ids.contains(&item.id),
                    "first-page id {} leaked into continuation",
                    item.id
                );
                continuation.push(item.id.clone());
            }
            walk = page
                .next_cursor
                .as_deref()
                .map(decode_cursor)
                .transpose()
                .unwrap();
        }

        first_ids
    });

    let first_ids = reader.await.expect("reader");
    writer.await.expect("writer");
    assert_eq!(first_ids.len(), 3);
}
