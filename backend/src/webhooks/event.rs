use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Kind of outbound notification a subscriber may receive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventKind {
    CorridorAlert,
    AnomalyDetected,
    SnapshotCompleted,
}

/// A logical webhook event. The `idempotency_key` is assigned once and
/// reused for every delivery attempt of this event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub idempotency_key: Uuid,
    pub kind: WebhookEventKind,
    pub payload: serde_json::Value,
}

impl WebhookEvent {
    /// Create a new logical event with a freshly generated idempotency key.
    pub fn new(kind: WebhookEventKind, payload: serde_json::Value) -> Self {
        Self {
            idempotency_key: Uuid::new_v4(),
            kind,
            payload,
        }
    }

    /// Rebuild an event that must keep a previously issued idempotency key
    /// (e.g. when loading from durable storage before a retry).
    pub fn with_idempotency_key(
        idempotency_key: Uuid,
        kind: WebhookEventKind,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            idempotency_key,
            kind,
            payload,
        }
    }
}
