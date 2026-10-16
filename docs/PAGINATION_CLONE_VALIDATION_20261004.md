# Pagination clone validation — 2026-10-04

This is a focused continuation of [PR 404](https://github.com/Stellar-Analysis/frontend/pull/404), preserving the original compound-cursor implementation and its saturated page-end calculation.

## Implementation

Source commit: `31a9d399d4c775308b62b94f014e252c99e2febd`.

File: `backend/src/pagination/cursor.rs`.

Source blob: `7a36f46c742fa876c222fb103095f69b62f8e232`.

The helper previously cloned the entire input snapshot, sorted those owned rows, and cloned the selected page again. It now sorts borrowed row references and clones only the rows actually returned. For an input of N rows and a returned page of M rows, the source-level clone count changes from N + M to M. This is an operation-count statement, not a measured wall-clock, memory, or database-throughput benchmark.

Stable compound-key ordering, ascending/descending selection, cursor encoding, the saturated end calculation, returned payloads, and the caller's input order are preserved. No dependency, public API, storage policy, or endpoint behavior was added.

## Executed check

[Hosted run 37193324458](https://github.com/woahwhattheheck/frontend/actions/runs/37193324458), job `111409913489`, completed successfully. The job independently checked the exact source commit and Git blob before running:

```bash
cargo test --locked --manifest-path backend/Cargo.toml --lib paginate_clones_only_returned_rows
```

Runtime: Ubuntu 24.04.5; rustc 1.98.1 (`48a229cea`, 2026-09-01); cargo 1.98.1 (`797e8a9bc`, 2026-08-05). The existing lockfile was used without modification.

Result: **1 test passed, 0 failed, 0 ignored, 2 filtered out; exit 0**. This one test executes seven cases: ascending and descending first pages; same-timestamp cursor boundaries in each direction; an exhausted cursor; a zero limit; and an oversized limit after a nonzero starting offset. Each case checks the returned IDs and payloads, one row clone per returned item, next-cursor presence/value, and unchanged original input order. The retained post-run source diff is empty.

[Artifact 11299976692](https://github.com/woahwhattheheck/frontend/actions/runs/37193324458/artifacts/11299976692) contains the exact source identifiers, compiler versions, complete command output, exit status, and source diff. The downloaded 1,215-byte ZIP was inspected; SHA-256:

`d8d60ccb682a45b6fc25bfac37a05715e9b2f56e1ce6f6f0efed3cccc6c068b2`

## Scope

Only this one focused library test was run for the continuation. The two encoding tests were filtered out, and the full integration suite, frontend, database endpoints, and concurrent-write tests were not rerun. The baseline was not executed again. Earlier results remain evidence for their originally tested revisions.

The validation controller is on a separate branch and is not part of this submission. This documentation adds no new execution, dependency, deployment, or claim of sponsor acceptance or payment.
