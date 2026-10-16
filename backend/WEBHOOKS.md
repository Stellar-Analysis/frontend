# Webhook library

This crate provides a signed delivery loop behind an `HttpTransport` interface.
It is not started by this repository's Next.js application. The deployed Rust
service lives in `Stellar-Analysis/backend`; its dispatcher and persistence must
be integrated there before this frontend PR can establish end-to-end delivery.

A complete patch for the existing backend is included in
[integration/README.md](integration/README.md), with its pinned source revision,
application instructions, file digests and native test evidence. It repairs the
real service's dispatcher, persistence and authenticated API. Keeping that patch
on this contribution branch does not apply it to the backend or deploy it.

## Delivery contract

Create a `WebhookEvent` once for a logical event. Retain its `idempotency_key`
when recovering the event. Every attempt for that event and subscriber sends
the same JSON bytes, signature and `X-Stellar-Insights-Idempotency-Key` header.
Treat a 2xx response as accepted; transport errors and other status codes retry
until `max_attempts` is exhausted.

`WebhookDeliverer::new` applies real blocking waits before retry attempts. With
the default policy, attempts begin after waits of 0, 100, 200, 400 and 800 ms,
plus the time spent in preceding transport calls. A successful attempt ends
the loop immediately. Each wait is capped by `max_backoff`.
The policy retains nanosecond precision: for attempt `n >= 2`, the requested
wait is `min(initial_backoff * 2^(n - 2), max_backoff)`. Large attempt numbers
saturate at the configured cap without wrapping; zero initial delay or zero
cap still requests no wait. This describes requested delays, not a guarantee
of the operating system's sleep precision.

An embedding runtime can use `WebhookDeliverer::with_sleeper` and implement
`RetrySleeper`. That implementation must actually wait for the requested delay;
the constructor exists to integrate the runtime's clock, not to defer a retry
after it has already been sent. The caller must bound each HTTP request's own
duration through its transport.

## Subscriber status

Status belongs to `(event_id, subscriber_id)`. One subscriber's failed request
does not replace another subscriber's delivered result.

- `store.by_subscriber(event_id, subscriber_id)` returns one target's status.
- `store.by_event_id(event_id)` returns all target statuses in subscriber-id order.
- `store.len()` counts target statuses, not distinct logical events.

The event lookup now returns a collection rather than the original single-row
`Option`. This is an intentional API correction for fan-out delivery. Records
are still kept in memory only; they do not survive process restart and are not
an HTTP status endpoint or a durable queue.

## Signature verification

Read the exact request body bytes and the `X-Stellar-Insights-Signature` header.
Use `verify_signature(subscriber_secret, body, signature)` before parsing or
acting on the payload. Do not reserialize JSON before verification. Retain the
event id in the receiver's own deduplication record before applying an effect.

## Validation

Run `cargo test` from this directory. The delivery regression exercises the
actual loop with an observed clock: HTTP 503, a transport failure, then 204.
It checks attempt timing, stable body and headers, and the stop on success.
A second case sends one event to two subscribers and checks that an exhausted
delivery cannot erase the other subscriber's successful status. The original
signature-tampering regression remains in place.

## Backoff boundary validation (2026-10-04)

The two issue-required targets passed **6/6 tests, zero failed or ignored**
with Rust/Cargo 1.98.1 and the unchanged lockfile:

```bash
cargo test --locked --manifest-path backend/Cargo.toml \
  --test idempotency_key_test --test tampered_payload_test -- --nocapture
```

[Successful run 37200239373, job 111430260057](https://github.com/woahwhattheheck/frontend/actions/runs/37200239373/job/111430260057)
compiled the actual library and executed all five delivery tests plus the
signature-tampering test. The delivery target took 0.30 seconds. Source was
product commit `499c7f6c37cb04a541e26d9ca87b503e3689077a`, with exact executed
blobs `80024ec002e6a5a07bffa451aac6ea486cecc367` (deliver.rs) and
`45ef8ef9cd4f61cb9d7d23c2f9dcf48bfd1c36e6` (idempotency_key_test.rs).
The isolated runner commit was `29fa5248754eed2e7989c52f0e1308380e65ff27`;
its temporary workflow is not part of the contribution branch.

The new observed-clock case previously recorded four requests at zero delay;
it now records cumulative times 0, 250, 750 and 1500 microseconds. The scaling
case previously returned 65.536 seconds at attempt 19 instead of 131.072;
it now doubles until the configured 300-second cap. The same case retains
controls for extreme attempt counts, Duration overflow, zero waits and the
unchanged default schedule. These are requested-delay and library correctness
results, not measured operating-system sleep precision or deployed throughput.

[Baseline run 37200127974, job 111429935872](https://github.com/woahwhattheheck/frontend/actions/runs/37200127974/job/111429935872)
ran those new cases against original production blob
`5718b74e2bfcb57a986e57c03dc23fb94bfde12b`: **one existing control passed and
both new cases failed**, Cargo exit 101. The substring filter also matched the
existing control, so an overly specific zero-pass log guard stopped that job
before candidate execution. The runner-only correction reused this recorded
baseline without repeating it; no source or test assertion was weakened.

Actions retained raw logs as artifact `11301904770` for the successful run
(uploaded ZIP SHA-256 `085c8414bff1ea80a7e278c4764c2f1e9d7bb4d87a21ca58e426bc7d9eecc679`)
and `11302477232` for the baseline
(`7f37aa5b1fdbe7d109dd78dfc0e59a1df18c1f67bb124b87ee84d02e5f3ac206`).
Both were configured for seven-day artifact retention; this source-bound
summary remains in Git. No full frontend suite, backend integration replay,
external HTTP delivery, deployment or sponsor acceptance is claimed here.
