use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Outcome of an outbound webhook delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
}

/// Queryable delivery history row keyed by logical event id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRecord {
    pub event_id: Uuid,
    pub subscriber_id: String,
    pub status: DeliveryStatus,
    pub attempts: u32,
    pub last_status_code: Option<u16>,
    pub last_error: Option<String>,
}

/// In-memory delivery history. Production would back this with durable storage;
/// the query surface (`by_event_id`) stays the same.
#[derive(Debug, Default, Clone)]
pub struct DeliveryStore {
    by_event: HashMap<Uuid, DeliveryRecord>,
}

impl DeliveryStore {
    pub fn upsert(&mut self, record: DeliveryRecord) {
        self.by_event.insert(record.event_id, record);
    }

    pub fn by_event_id(&self, event_id: Uuid) -> Option<&DeliveryRecord> {
        self.by_event.get(&event_id)
    }

    pub fn len(&self) -> usize {
        self.by_event.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_event.is_empty()
    }
}
