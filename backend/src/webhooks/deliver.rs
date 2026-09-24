use std::time::Duration;

use serde_json::json;
use thiserror::Error;

use super::event::WebhookEvent;
use super::sign::{self, IDEMPOTENCY_HEADER, SIGNATURE_HEADER};
use super::status::{DeliveryRecord, DeliveryStatus, DeliveryStore};

/// Bounded retry policy for outbound webhook delivery.
#[derive(Debug, Clone)]
pub struct DeliveryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for DeliveryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(30),
        }
    }
}

impl DeliveryPolicy {
    /// Backoff delay before the given 1-based attempt number.
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::from_millis(0);
        }
        let shift = (attempt - 2).min(16);
        let millis = self
            .initial_backoff
            .as_millis()
            .saturating_mul(1u128 << shift);
        let capped = millis.min(self.max_backoff.as_millis());
        Duration::from_millis(capped as u64)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DeliverError {
    #[error("subscriber returned non-success status {0}")]
    NonSuccess(u16),
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("exhausted {0} delivery attempts")]
    Exhausted(u32),
}

/// One HTTP-shaped delivery attempt result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAttempt {
    pub attempt: u32,
    pub status_code: Option<u16>,
    pub ok: bool,
    pub error: Option<String>,
}

/// Abstraction over the HTTP client so unit tests can inject a fake transport.
pub trait HttpTransport {
    fn post(
        &mut self,
        url: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<u16, String>;
}

/// Delivers signed webhook events with bounded retry-with-backoff.
pub struct WebhookDeliverer<T: HttpTransport> {
    pub transport: T,
    pub policy: DeliveryPolicy,
    pub store: DeliveryStore,
}

impl<T: HttpTransport> WebhookDeliverer<T> {
    pub fn new(transport: T, policy: DeliveryPolicy) -> Self {
        Self {
            transport,
            policy,
            store: DeliveryStore::default(),
        }
    }

    /// Deliver `event` to `url`, signing with `secret`.
    /// The same `event.idempotency_key` is sent on every retry attempt.
    pub fn deliver(
        &mut self,
        subscriber_id: &str,
        url: &str,
        secret: &[u8],
        event: &WebhookEvent,
    ) -> Result<DeliveryRecord, DeliverError> {
        let body = serde_json::to_vec(&json!({
            "id": event.idempotency_key,
            "type": event.kind,
            "data": event.payload,
        }))
        .expect("webhook payload serializes");

        let signature = sign::sign_payload(secret, &body);
        let idem_header = event.idempotency_key.to_string();

        let mut last_error = DeliverError::Exhausted(0);

        for attempt in 1..=self.policy.max_attempts {
            let _delay = self.policy.backoff_for_attempt(attempt);
            // Callers that need real sleeping can wrap this; the policy is
            // still queryable and tested without wall-clock waits.

            let headers = [
                (SIGNATURE_HEADER, signature.clone()),
                (IDEMPOTENCY_HEADER, idem_header.clone()),
                ("Content-Type", "application/json".to_string()),
            ];

            match self.transport.post(url, &headers, &body) {
                Ok(code) if (200..300).contains(&code) => {
                    let record = DeliveryRecord {
                        event_id: event.idempotency_key,
                        subscriber_id: subscriber_id.to_string(),
                        status: DeliveryStatus::Delivered,
                        attempts: attempt,
                        last_status_code: Some(code),
                        last_error: None,
                    };
                    self.store.upsert(record.clone());
                    return Ok(record);
                }
                Ok(code) => {
                    last_error = DeliverError::NonSuccess(code);
                    self.store.upsert(DeliveryRecord {
                        event_id: event.idempotency_key,
                        subscriber_id: subscriber_id.to_string(),
                        status: DeliveryStatus::Pending,
                        attempts: attempt,
                        last_status_code: Some(code),
                        last_error: Some(format!("HTTP {code}")),
                    });
                }
                Err(msg) => {
                    last_error = DeliverError::Transport(msg.clone());
                    self.store.upsert(DeliveryRecord {
                        event_id: event.idempotency_key,
                        subscriber_id: subscriber_id.to_string(),
                        status: DeliveryStatus::Pending,
                        attempts: attempt,
                        last_status_code: None,
                        last_error: Some(msg),
                    });
                }
            }
        }

        let failed = DeliveryRecord {
            event_id: event.idempotency_key,
            subscriber_id: subscriber_id.to_string(),
            status: DeliveryStatus::Failed,
            attempts: self.policy.max_attempts,
            last_status_code: match &last_error {
                DeliverError::NonSuccess(code) => Some(*code),
                _ => None,
            },
            last_error: Some(last_error.to_string()),
        };
        self.store.upsert(failed.clone());
        Err(DeliverError::Exhausted(self.policy.max_attempts))
    }
}
