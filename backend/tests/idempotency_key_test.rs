use std::sync::{Arc, Mutex};

use stellar_insights_backend::webhooks::{
    deliver::{DeliveryPolicy, HttpTransport, WebhookDeliverer},
    event::{WebhookEvent, WebhookEventKind},
    sign::IDEMPOTENCY_HEADER,
};
use uuid::Uuid;

#[derive(Default)]
struct RecordingTransport {
    calls: Arc<Mutex<Vec<(String, Vec<(String, String)>, Vec<u8>)>>>,
    /// Return codes per call; last code repeats if exhausted.
    responses: Vec<Result<u16, String>>,
}

impl HttpTransport for RecordingTransport {
    fn post(
        &mut self,
        url: &str,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<u16, String> {
        let owned_headers: Vec<(String, String)> = headers
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        self.calls
            .lock()
            .unwrap()
            .push((url.to_string(), owned_headers, body.to_vec()));
        let idx = self.calls.lock().unwrap().len() - 1;
        if idx < self.responses.len() {
            self.responses[idx].clone()
        } else {
            self.responses
                .last()
                .cloned()
                .unwrap_or(Ok(200))
        }
    }
}

#[test]
fn idempotency_key_stable_across_retries() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let transport = RecordingTransport {
        calls: Arc::clone(&calls),
        responses: vec![Ok(500), Ok(502), Ok(200)],
    };
    let policy = DeliveryPolicy {
        max_attempts: 5,
        ..DeliveryPolicy::default()
    };
    let mut deliverer = WebhookDeliverer::new(transport, policy);

    let event = WebhookEvent::new(
        WebhookEventKind::CorridorAlert,
        serde_json::json!({"corridor": "US-MX", "score": 0.12}),
    );
    let key_before = event.idempotency_key;

    let record = deliverer
        .deliver("sub_1", "https://example.test/hook", b"secret", &event)
        .expect("eventually delivered");

    assert_eq!(record.event_id, key_before);
    assert_eq!(record.attempts, 3);

    let captured = calls.lock().unwrap();
    assert_eq!(captured.len(), 3);
    let keys: Vec<String> = captured
        .iter()
        .map(|(_, headers, _)| {
            headers
                .iter()
                .find(|(k, _)| k == IDEMPOTENCY_HEADER)
                .map(|(_, v)| v.clone())
                .expect("idempotency header present")
        })
        .collect();
    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[1], keys[2]);
    assert_eq!(keys[0], key_before.to_string());

    // Rebuilding the same logical event keeps the key.
    let replayed = WebhookEvent::with_idempotency_key(
        key_before,
        WebhookEventKind::CorridorAlert,
        serde_json::json!({"corridor": "US-MX", "score": 0.12}),
    );
    assert_eq!(replayed.idempotency_key, key_before);
    assert_ne!(Uuid::nil(), key_before);
}
