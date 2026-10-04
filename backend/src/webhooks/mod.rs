//! Outbound webhook delivery for subscriber integrations.
//!
//! Design follows the Stripe/GitHub convention subscribers already know:
//! every logical event gets a stable idempotency key that does not change
//! across retries, and every payload is HMAC-signed with a per-subscriber
//! secret so forged deliveries are detectable.

pub mod deliver;
pub mod event;
pub mod sign;
pub mod status;

pub use deliver::{DeliverError, DeliveryAttempt, DeliveryPolicy, WebhookDeliverer};
pub use event::{WebhookEvent, WebhookEventKind};
pub use sign::{sign_payload, verify_signature, SIGNATURE_HEADER};
pub use status::{DeliveryRecord, DeliveryStatus, DeliveryStore};
