use stellar_insights_backend::webhooks::sign::{sign_payload, verify_signature, SIGNATURE_HEADER};

#[test]
fn tampered_payload_fails_verification() {
    let secret = b"per-subscriber-secret";
    let original = br#"{"id":"evt_1","type":"anomaly_detected","data":{"score":0.9}}"#;
    let signature = sign_payload(secret, original);

    assert!(
        verify_signature(secret, original, &signature),
        "untampered payload must verify"
    );

    let mut tampered = original.to_vec();
    // Flip a byte inside the JSON body.
    let idx = tampered.iter().position(|&b| b == b'9').expect("digit present");
    tampered[idx] = b'1';

    assert!(
        !verify_signature(secret, &tampered, &signature),
        "tampered payload must not verify under the original signature"
    );

    // Wrong secret also fails.
    assert!(!verify_signature(b"other-secret", original, &signature));

    // Documented header name is stable for consumers.
    assert_eq!(SIGNATURE_HEADER, "X-Stellar-Insights-Signature");
}
