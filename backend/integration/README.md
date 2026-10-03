# Stellar-Analysis backend webhook repair

## Target and application

This patch targets the root of **Stellar-Analysis/backend**, based on commit
`965e916227cf9173cf6ad254a6971429060c1d19`. The frontend issue and its existing
PR are coordination and transport locations; the production patch does not
belong in the frontend's standalone webhook mini-crate.

Apply the companion `stellar-analysis-backend.patch` to that backend revision:

```bash
git switch -c fix/durable-webhook-delivery 965e916227cf9173cf6ad254a6971429060c1d19
git apply --check /path/to/stellar-analysis-backend.patch
git apply /path/to/stellar-analysis-backend.patch
cargo test --locked --test webhook_delivery_regression_tests --test webhook_event_service_tests
```

The patch changes 13 existing files and adds two files. The accompanying
`stellar-analysis-backend-manifest.json` records each file's original Git blob
SHA, final Git blob SHA, SHA-256, and size. Every preserved original was checked
against the blob SHA obtained from the pinned GitHub tree. A source audit found
exactly the claimed changes. `git apply --check` succeeds against those original
files. Formatting checks pass for every changed Rust source and test file.

## Production behavior

The existing `WebhookDispatcher`, `WebhookService`, event producer service,
SQLite migrations, and authenticated API implement the repair. No additional
dispatcher, standalone runtime, or test-only implementation is introduced.

- Enqueue persists the event UUID and complete serialized envelope. Retries
  reuse the same ID, event timestamp, bytes, headers, and HMAC input after a
  dispatcher restart. Legacy rows derive their envelope from the stored event
  ID and creation timestamp, then persist it before the first new send.
- A due event is reserved atomically for 30 seconds before delivery. A random
  reservation token fences completion writes, preventing an expired worker
  from overwriting the result of a newer reservation. Expired reservations can
  be recovered by another worker.
- Each request has a 10-second timeout. Redirects are rejected without
  contacting another destination. Any non-2xx response or transport failure is
  recorded as a failure; receiver response bodies are not retained or returned
  through the status API.
- Failure count, last error, last attempt time, and the next due time are
  persisted. The first and second failures schedule retries after five and ten
  seconds. The exponential helper saturates at 300 seconds if the attempt limit
  is later increased. The third recorded failure becomes terminal `failed`.
  Inactive or missing subscriptions also terminate without an HTTP request.
- Successful delivery preserves previous failure count and last error and
  records the delivered timestamp. The event update and subscription's
  `last_fired_at` share one transaction. Database errors propagate instead of
  producing success logs. Event enqueue errors also propagate to the producer.
- Authenticated owners can query
  `GET /api/webhooks/{webhook_id}/events/{event_id}` or the corresponding
  `/api/v1/webhooks/...` resource. The result contains persisted delivery
  metadata, excludes payload and secrets, and uses the same 404 response for
  absent, mismatched, and non-owned events. Access-token JWT middleware protects
  the actual mounted routes. Existing malformed nested route aliases are
  preserved for compatibility.
- Successful authenticated registration returns the signing secret once with
  `Cache-Control: no-store`. List, detail, and delivery status responses omit
  it. The README specifies raw-body HMAC verification and event-ID deduplication.

Migration `035_webhook_delivery_retries.sql` adds the delivery envelope,
reservation token, and nullable scheduling/timestamp columns, plus a due-event
index. It repairs exhausted legacy `pending` rows to `failed` while preserving
existing errors. Migration 019 remains byte-identical to the pinned source.

The existing backend event types remain `corridor.health_degraded`,
`anchor.status_changed`, `payment.created`, and `corridor.liquidity_dropped`.
This patch repairs their delivery path. It does not add new anomaly or snapshot
event producers corresponding to other wording in frontend issue 336.

## Necessary build and authentication prerequisites

The focused native target exposed existing production prerequisites. These
small fixes are included in the same patch so the real package can compile and
its authenticated behavior can execute:

| Files | Change and reason |
| --- | --- |
| `src/observability/job_metrics.rs` | Four `with_label_values::<&str>` annotations resolve generic inference for mixed label references. |
| `src/observability/metrics.rs` | One identical explicit type annotation resolves the same inference failure. |
| `src/main.rs` | Converts the configured compression threshold from `u16` to `u64` for the existing tower-http API. Adds the existing validated `JWT_SECRET` as the router's `JwtSecret` extension. |
| `Cargo.toml`, `Cargo.lock` | Enables `jsonwebtoken`'s `aws_lc_rs` feature at its unchanged 10.4 version. The default dependency had no crypto provider and panicked when the real JWT tests signed or verified a token. AWS-LC was already present in the dependency graph; the lockfile adds its JWT edge and required `untrusted` 0.7.1 parser dependency without unrelated version updates. |

No Cargo target is disabled, no binary is excluded, and no stub authentication
provider replaces the production implementation. No credential values or
provider configuration are changed.

## Verification

**Final focused native result: 26 passed, zero failed; Cargo exited 0.**
`webhook_delivery_regression_tests` passed all 21 tests, and
`webhook_event_service_tests` passed all five tests. The production library and
binary compiled with the real JWT provider enabled. The final run retained the
same native targets and succeeded after relocating completed compiler archives
to reduce the shared machine's memory consumption. Evidence:
`stellar_backend_native_final.log`.

The native regression target imports the exact production webhook source files
by path to execute their private inline tests. It uses the compiled backend
library's real auth, middleware, crypto, validation, and broadcast modules. This
keeps the tests runnable without compiling unrelated broken library unit tests;
the normal production library and binary remain part of the Cargo build.

The regression target currently contains 21 tests covering:

- Actual localhost HTTP requests and SQLite persistence for failure followed by
  success, dispatcher recreation, stable headers and body, tamper rejection,
  wrong-secret rejection, and retained failure metadata.
- Three recorded failures becoming terminal, with no fourth request; no early
  retry before the persisted due time; inactive subscriptions sending nothing.
- Receiver redirects not followed and receiver error bodies not persisted.
- Injected SQLite failures propagating and rolling back both delivered state
  and `last_fired_at`, while keeping a recoverable reservation.
- Legacy timestamp/envelope handling, preservation through retries, migration
  repair, saturated backoff, concurrent selectors, and expired-worker fencing.
- An independent RFC 4231 HMAC vector and existing signature/type checks.
- Actual access-token JWT validation at the mounted versioned path, invalid,
  wrong-key, and refresh-token rejection, owner isolation, and creation-only
  usable signing-secret disclosure.

The existing `webhook_event_service_tests` target contains five tests, including
the new failure-injection assertion that a rejected enqueue is not reported as
a successful trigger.

### Captured intermediate results and limits

1. The first native regression execution completed with **19 passed and two
   failed**. Both failures were the real JWT tests panicking because no crypto
   provider was enabled. This exposed the production dependency defect above;
   it is not a successful final validation result. Evidence:
   `stellar_backend_jwt_provider_failure.log`.
2. Two subsequent attempts after enabling the provider were terminated by
   `SIGKILL` while compiling the backend library. They emitted only existing
   production warnings and no source error. The shared 8 GiB memory cgroup
   recorded OOM kills. Completed compiler archives were moved out of tmpfs to
   reduce memory consumption; source and target selection were unchanged.
   Evidence from the later failure: `stellar_backend_resource_failure.log`.
3. An earlier `cargo test --locked --lib webhook --no-run` found 41 compilation
   errors across unrelated unit-test code and the five production metric
   inference problems now fixed. Unrelated failures include missing `Arc` and
   `mock_stellar` references, `JobHealthStatus` comparisons without `PartialEq`,
   trace executor `Unpin` requirements, stale numeric assertions, and websocket
   methods called without their required IP argument. Evidence:
   `stellar_backend_build.log`. The full library unit-test suite and all-repo
   test suite are **not claimed to pass**.

Exact native command used in the shared environment, from the backend root:

```bash
RUSTUP_HOME=/workspace/scratch/1b6b1b08e6b1/toolchain/rustup \
CARGO_HOME=/workspace/scratch/1b6b1b08e6b1/toolchain/cargo \
CARGO_BUILD_JOBS=1 \
CARGO_TARGET_DIR=/workspace/scratch/1b6b1b08e6b1/backlog/stellar_backend/target \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
OPENSSL_LIB_DIR=/usr/lib/x86_64-linux-gnu OPENSSL_INCLUDE_DIR=/usr/include \
/workspace/scratch/1b6b1b08e6b1/toolchain/cargo/bin/rustup run 1.89.0 \
  cargo test --locked \
  --test webhook_delivery_regression_tests \
  --test webhook_event_service_tests
```

The OpenSSL paths compensate for the shared environment lacking `pkg-config`.
The reduced debug output and disabled incremental compilation are resource
settings; neither changes the repository manifest nor removes build targets.

## Operational boundaries

Delivery remains at least once. If an HTTP request succeeds but the database
write fails, a later worker may resend that event with identical ID and bytes.
Crashes or failed status writes may therefore produce more physical requests
than the three recorded failure limit. Consumers must verify the raw-body HMAC
and deduplicate by event ID. The status resource exposes the persisted current
state and retained failure metadata; it is not an append-only per-attempt audit
log. Attempts made before migration used old per-attempt IDs and cannot be
retroactively deduplicated against the new stable identity.

The patch must still be landed in the actual backend repository and its normal
migration process run before these changes affect a deployed service. A copy
carried in the frontend PR is a reviewable handoff, not a backend deployment.
