use std::sync::{Arc, Mutex};
use std::time::Duration;

use stellar_insights_backend::webhooks::{
    deliver::{DeliverError, DeliveryPolicy, HttpTransport, RetrySleeper, WebhookDeliverer},
    event::{WebhookEvent, WebhookEventKind},
    sign::IDEMPOTENCY_HEADER,
    status::DeliveryStatus,
};
use uuid::Uuid;

#[derive(Default)]
struct RecordingTransport {
    calls: Arc<Mutex<Vec<(String, Vec<(String, String)>, Vec<u8>)>>>,
    /// Return codes per call; last code repeats if exhausted.
    responses: Vec<Result<u16, String>>,
    clock: Arc<Mutex<Duration>>,
    call_times: Arc<Mutex<Vec<Duration>>>,
}

impl HttpTransport for RecordingTransport {
    fn post(&mut self, url: &str, headers: &[(&str, String)], body: &[u8]) -> Result<u16, String> {
        self.call_times
            .lock()
            .unwrap()
            .push(*self.clock.lock().unwrap());
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
            self.responses.last().cloned().unwrap_or(Ok(200))
        }
    }
}

#[test]
fn idempotency_key_stable_across_retries() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let transport = RecordingTransport {
        calls: Arc::clone(&calls),
        responses: vec![Ok(500), Ok(502), Ok(200)],
        ..RecordingTransport::default()
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

struct RecordingSleeper {
    clock: Arc<Mutex<Duration>>,
}

impl RetrySleeper for RecordingSleeper {
    fn sleep(&mut self, delay: Duration) {
        *self.clock.lock().unwrap() += delay;
    }
}

#[test]
fn transport_attempts_observe_backoff_and_stop_after_success() {
    let clock = Arc::new(Mutex::new(Duration::ZERO));
    let call_times = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let transport = RecordingTransport {
        calls: Arc::clone(&calls),
        responses: vec![Ok(503), Err("connection lost".to_string()), Ok(204)],
        clock: Arc::clone(&clock),
        call_times: Arc::clone(&call_times),
    };
    let sleeper = RecordingSleeper { clock };
    let policy = DeliveryPolicy {
        max_attempts: 5,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(15),
    };
    let mut deliverer = WebhookDeliverer::with_sleeper(transport, policy, sleeper);
    let event = WebhookEvent::new(
        WebhookEventKind::AnomalyDetected,
        serde_json::json!({"score": 0.12}),
    );

    let record = deliverer
        .deliver("sub_1", "https://example.test/hook", b"secret", &event)
        .unwrap();

    assert_eq!(
        *call_times.lock().unwrap(),
        vec![
            Duration::ZERO,
            Duration::from_millis(10),
            Duration::from_millis(25),
        ]
    );
    assert_eq!(record.status, DeliveryStatus::Delivered);
    assert_eq!(record.attempts, 3);
    let captured = calls.lock().unwrap();
    assert_eq!(captured[0].1, captured[1].1);
    assert_eq!(captured[1].1, captured[2].1);
    assert_eq!(captured[0].2, captured[1].2);
    assert_eq!(captured[1].2, captured[2].2);
}

#[test]
fn failed_subscriber_does_not_replace_another_subscribers_success() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let transport = RecordingTransport {
        calls: Arc::clone(&calls),
        responses: vec![Ok(200), Ok(503)],
        ..RecordingTransport::default()
    };
    let policy = DeliveryPolicy {
        max_attempts: 2,
        initial_backoff: Duration::ZERO,
        max_backoff: Duration::ZERO,
    };
    let mut deliverer = WebhookDeliverer::new(transport, policy);
    let event = WebhookEvent::new(
        WebhookEventKind::SnapshotCompleted,
        serde_json::json!({"snapshot": "snapshot-1"}),
    );
    let delivered = deliverer
        .deliver("sub_a", "https://example.test/a", b"a-secret", &event)
        .unwrap();
    assert_eq!(
        deliverer.deliver("sub_b", "https://example.test/b", b"b-secret", &event),
        Err(DeliverError::Exhausted(2))
    );

    assert_eq!(calls.lock().unwrap().len(), 3);
    assert_eq!(
        deliverer
            .store
            .by_subscriber(event.idempotency_key, "sub_a"),
        Some(&delivered)
    );
    let failed = deliverer
        .store
        .by_subscriber(event.idempotency_key, "sub_b")
        .unwrap();
    assert_eq!(failed.status, DeliveryStatus::Failed);
    assert_eq!(failed.attempts, 2);
    assert_eq!(failed.last_status_code, Some(503));
    assert_eq!(deliverer.store.len(), 2);
    let outcomes = deliverer.store.by_event_id(event.idempotency_key);
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].subscriber_id, "sub_a");
    assert_eq!(outcomes[1].subscriber_id, "sub_b");
    assert!(deliverer.store.by_event_id(Uuid::new_v4()).is_empty());
}

#[test]
fn backoff_preserves_fractional_delays_and_observed_waits() {
    let clock = Arc::new(Mutex::new(Duration::ZERO));
    let call_times = Arc::new(Mutex::new(Vec::new()));
    let transport = RecordingTransport {
        responses: vec![Ok(503), Ok(503), Ok(503), Ok(204)],
        clock: Arc::clone(&clock),
        call_times: Arc::clone(&call_times),
        ..RecordingTransport::default()
    };
    let policy = DeliveryPolicy {
        max_attempts: 4,
        initial_backoff: Duration::from_micros(250),
        max_backoff: Duration::from_micros(750),
    };
    let mut deliverer =
        WebhookDeliverer::with_sleeper(transport, policy, RecordingSleeper { clock });
    let event = WebhookEvent::new(
        WebhookEventKind::SnapshotCompleted,
        serde_json::json!({"snapshot": "fractional-backoff"}),
    );
    let record = deliverer
        .deliver("sub_1", "https://example.test/hook", b"secret", &event)
        .unwrap();

    assert_eq!(record.attempts, 4);
    assert_eq!(
        *call_times.lock().unwrap(),
        vec![
            Duration::ZERO,
            Duration::from_micros(250),
            Duration::from_micros(750),
            Duration::from_micros(1500),
        ]
    );
}

#[test]
fn backoff_scales_to_the_cap_without_exponent_or_duration_overflow() {
    let policy = DeliveryPolicy {
        max_attempts: 25,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_secs(300),
    };
    assert_eq!(policy.backoff_for_attempt(18), Duration::from_millis(65_536));
    assert_eq!(policy.backoff_for_attempt(19), Duration::from_millis(131_072));
    assert_eq!(policy.backoff_for_attempt(20), Duration::from_millis(262_144));
    assert_eq!(policy.backoff_for_attempt(21), Duration::from_secs(300));
    assert_eq!(policy.backoff_for_attempt(u32::MAX), Duration::from_secs(300));

    let largest = DeliveryPolicy {
        initial_backoff: Duration::from_nanos(1),
        max_backoff: Duration::MAX,
        ..policy.clone()
    };
    assert_eq!(largest.backoff_for_attempt(u32::MAX), Duration::MAX);
    let overflowing_product = DeliveryPolicy {
        initial_backoff: Duration::MAX,
        ..largest.clone()
    };
    assert_eq!(overflowing_product.backoff_for_attempt(129), Duration::MAX);
    let zero = DeliveryPolicy {
        initial_backoff: Duration::ZERO,
        ..largest
    };
    assert_eq!(zero.backoff_for_attempt(u32::MAX), Duration::ZERO);
    let no_wait = DeliveryPolicy {
        max_backoff: Duration::ZERO,
        ..policy
    };
    assert_eq!(no_wait.backoff_for_attempt(u32::MAX), Duration::ZERO);

    let default_policy = DeliveryPolicy::default();
    assert_eq!(default_policy.backoff_for_attempt(0), Duration::ZERO);
    for (attempt, millis) in [(1, 0), (2, 100), (3, 200), (4, 400), (5, 800)] {
        assert_eq!(
            default_policy.backoff_for_attempt(attempt),
            Duration::from_millis(millis)
        );
    }
}
