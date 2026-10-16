use std::collections::BTreeMap;

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

/// Queryable delivery status for one logical event and subscriber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRecord {
    pub event_id: Uuid,
    pub subscriber_id: String,
    pub status: DeliveryStatus,
    pub attempts: u32,
    pub last_status_code: Option<u16>,
    pub last_error: Option<String>,
}

/// In-memory delivery status, scoped by logical event and subscriber. This
/// library does not persist the records across process restarts.
#[derive(Debug, Default, Clone)]
pub struct DeliveryStore {
    by_event: BTreeMap<Uuid, BTreeMap<String, DeliveryRecord>>,
}

impl DeliveryStore {
    pub fn upsert(&mut self, record: DeliveryRecord) {
        self.by_event
            .entry(record.event_id)
            .or_default()
            .insert(record.subscriber_id.clone(), record);
    }

    /// All subscriber outcomes for an event, ordered by subscriber id.
    pub fn by_event_id(&self, event_id: Uuid) -> Vec<&DeliveryRecord> {
        self.by_event
            .get(&event_id)
            .map(|subscribers| subscribers.values().collect())
            .unwrap_or_default()
    }

    pub fn by_subscriber(&self, event_id: Uuid, subscriber_id: &str) -> Option<&DeliveryRecord> {
        self.by_event
            .get(&event_id)
            .and_then(|subscribers| subscribers.get(subscriber_id))
    }

    pub fn len(&self) -> usize {
        self.by_event.values().map(BTreeMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.by_event.is_empty()
    }
}
