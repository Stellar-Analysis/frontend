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
