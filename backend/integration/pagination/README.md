# Live backend cursor pagination for issue 330

This packet contains the complete backend implementation for
[Stellar-Analysis/frontend#330](https://github.com/Stellar-Analysis/frontend/issues/330).
It extends the existing contribution in
[frontend#404](https://github.com/Stellar-Analysis/frontend/pull/404)
without replacing its pagination library. The patch targets
[Stellar-Analysis/backend](https://github.com/Stellar-Analysis/backend) at
`965e916227cf9173cf6ad254a6971429060c1d19`.

The separate `backend/integration/pagination/` directory keeps this change
independent of other backend integration packets. Apply it in the backend
repository, not in the frontend checkout.

## Apply

In a clean backend checkout at the pinned commit, use the absolute path to this
packet's patch:

```sh
git apply --check /path/to/backend/integration/pagination/stellar-analysis-backend.patch
git apply /path/to/backend/integration/pagination/stellar-analysis-backend.patch
```

`manifest.json` records every changed path, its original Git blob where present,
and its resulting Git blob and SHA-256 hashes. The patch contains all 14 changed
or new files; it does not depend on uncommitted files from another checkout.

The complete patch passed `git apply --check` against the clean pinned backend
checkout. `git diff --check` also passed.

## Resulting behavior

- Live anchor, corridor and ingested ledger transaction lists use an opaque
  `(timestamp_ms, sequence)` cursor with a monotonic SQLite sequence.
- Migration 035 backfills immutable keys, creates matching seek indexes, and
  assigns new keys atomically on source inserts. Database traversal captures an
  insert ceiling, excluding both simultaneous and backdated later inserts.
- Corridor page one retains the full filtered RPC result. Continuation reads
  stored payloads without querying a moving RPC window or recomputing prices.
- `/api/v1/ledger/transactions` exposes existing ledger-ingestion records;
  pending signing requests retain their existing routes and behavior.
- Continuation uses a strict seek boundary from the last returned row and reads
  one lookahead row. Database continuation reuses the initial count.

The patch includes route wiring, API error conversion, OpenAPI registration,
and `docs/CURSOR_PAGINATION.md` with the request and encoding contract.

## Compatibility and limits

Clients must use returned cursors for continuation. Nonzero offsets return
`400 INVALID_PAGINATION`; limits are 1–100. Traversal uses ascending immutable
keys, so changing reliability or corridor metrics does not reorder results.
Corridor filters must remain the same throughout a traversal.

Corridor snapshots expire after 15 minutes; first-page caching is capped at
5 minutes. Missing or expired snapshots return `400 CURSOR_EXPIRED`. Database
payload updates and deletions can be visible during traversal, while insert
membership, ordering and the initial total remain fixed. Ledger records may
include placeholders created by existing ingestion; this change does not add
transaction decoding.

## Validation receipt

The two tests expressly requested by issue 330 passed against the exact
production pagination modules, real SQLx 0.8.6 queries, and actual SQLite
migrations:

| Maintained test target | Result | Test runtime |
| --- | --- | --- |
| `batch_collision_test` | 1 passed, 0 failed | 0.04 seconds |
| `concurrent_write_stability_test` | 1 passed, 0 failed | 0.03 seconds |

The first test traverses tied timestamps through the actual anchor, ledger and
snapshot selectors, checking boundaries, completeness and indexed seeks. The
second commits simultaneous and backdated inserts after issuing a cursor, then
checks replay, original membership and retained corridor payloads.

The run used a small manifest importing the unchanged production module files
with `#[path]` and matching direct dependency versions. The test files use the
actual migrations. This was necessary because shared storage exhausted during
dependency compilation of the full backend crate. Whole-server compilation,
HTTP composition and live-provider performance were not verified by that run.

The maintained command for a backend environment with sufficient storage is:

```sh
cargo test --locked --test batch_collision_test --test concurrent_write_stability_test
```

No workflows or broad test suite were added. The original Cargo.lock is unchanged.
