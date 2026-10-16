use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Header name carrying the hex-encoded HMAC-SHA256 signature.
pub const SIGNATURE_HEADER: &str = "X-Stellar-Insights-Signature";

/// Header name carrying the stable per-event idempotency key.
pub const IDEMPOTENCY_HEADER: &str = "X-Stellar-Insights-Idempotency-Key";

/// HMAC-SHA256 sign `payload` with the per-subscriber `secret`.
/// Returns a lowercase hex digest suitable for the signature header.
pub fn sign_payload(secret: &[u8], payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any size");
    mac.update(payload);
    let result = mac.finalize().into_bytes();
    hex::encode(result)
}

/// Constant-time verification of a hex-encoded signature against `payload`.
pub fn verify_signature(secret: &[u8], payload: &[u8], signature_hex: &str) -> bool {
    let expected = sign_payload(secret, payload);
    // Timing-safe compare via hmac's own verifier when possible; fall back
    // to length-checked equality on decoded bytes.
    match (hex::decode(signature_hex), hex::decode(&expected)) {
        (Ok(provided), Ok(exp)) if provided.len() == exp.len() => {
            let mut mac =
                HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any size");
            mac.update(payload);
            mac.verify_slice(&provided).is_ok()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_signature() {
        let secret = b"subscriber-secret";
        let payload = br#"{"hello":"world"}"#;
        let sig = sign_payload(secret, payload);
        assert!(verify_signature(secret, payload, &sig));
        assert!(!verify_signature(secret, payload, "00"));
        assert!(!verify_signature(b"other", payload, &sig));
    }
}
