//! Hash-chained administrative audit log entries.
//!
//! Each [`AuditEntry`] commits to the hash of the preceding entry (`prev_hash`),
//! its monotonic `sequence` number, the structured [`AdminAction`], the `actor`
//! identifier, and the UTC `timestamp_ms`. Any retroactive modification to an
//! existing row invalidates its `entry_hash` and breaks the `prev_hash` link of
//! the following entry.

use std::fmt;

/// The canonical 64-character hex string representing the genesis `prev_hash`
/// (32 zero bytes) used by the very first entry in the chain (`sequence == 1`).
pub const GENESIS_HASH_HEX: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Administrative actions recorded in the tamper-evident audit log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminAction {
    /// Manual correction applied to a ledger analytics snapshot.
    ManualSnapshotCorrection {
        epoch: u64,
        corridor_id: String,
        reason: String,
    },
    /// Rotation of an operational or signing secret.
    SecretRotation {
        secret_name: String,
        new_key_version: u32,
    },
    /// Administrative toggle of a runtime feature flag.
    FeatureFlagToggle { flag_key: String, enabled: bool },
    /// Grant or revocation of an administrative or operator role.
    RoleChange {
        principal: String,
        role: String,
        granted: bool,
    },
}

impl AdminAction {
    /// Canonical, deterministic string encoding of the action for hashing.
    ///
    /// Fields are length-prefixed so that delimiter characters inside user
    /// strings can never cause canonicalization collisions across fields.
    pub fn canonical_encoding(&self) -> String {
        match self {
            AdminAction::ManualSnapshotCorrection {
                epoch,
                corridor_id,
                reason,
            } => format!(
                "manual_snapshot_correction:{epoch}:{}:{corridor_id}:{}:{reason}",
                corridor_id.len(),
                reason.len()
            ),
            AdminAction::SecretRotation {
                secret_name,
                new_key_version,
            } => format!(
                "secret_rotation:{}:{secret_name}:{new_key_version}",
                secret_name.len()
            ),
            AdminAction::FeatureFlagToggle { flag_key, enabled } => format!(
                "feature_flag_toggle:{}:{flag_key}:{enabled}",
                flag_key.len()
            ),
            AdminAction::RoleChange {
                principal,
                role,
                granted,
            } => format!(
                "role_change:{}:{principal}:{}:{role}:{granted}",
                principal.len(),
                role.len()
            ),
        }
    }
}

impl fmt::Display for AdminAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical_encoding())
    }
}

/// A single immutable, hash-chained entry in the administrative audit log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// Monotonically increasing 1-based sequence number.
    pub sequence: u64,
    /// Hex-encoded SHA-256 digest of the immediately preceding entry, or
    /// [`GENESIS_HASH_HEX`] when `sequence == 1`.
    pub prev_hash: String,
    /// The administrative action performed.
    pub action: AdminAction,
    /// Identifier of the authenticated administrator or service principal.
    pub actor: String,
    /// UTC timestamp in milliseconds when the entry was recorded.
    pub timestamp_ms: u64,
    /// Hex-encoded SHA-256 digest committing to `(sequence, prev_hash, action, actor, timestamp_ms)`.
    pub entry_hash: String,
}

impl AuditEntry {
    /// Construct a new entry chained to `prev_hash` and compute its cryptographic
    /// `entry_hash`.
    pub fn new(
        sequence: u64,
        prev_hash: impl Into<String>,
        action: AdminAction,
        actor: impl Into<String>,
        timestamp_ms: u64,
    ) -> Self {
        let prev_hash = prev_hash.into();
        let actor = actor.into();
        let entry_hash = compute_entry_hash(sequence, &prev_hash, &action, &actor, timestamp_ms);
        Self {
            sequence,
            prev_hash,
            action,
            actor,
            timestamp_ms,
            entry_hash,
        }
    }

    /// Recompute the expected hash for this entry's current fields.
    pub fn recompute_hash(&self) -> String {
        compute_entry_hash(
            self.sequence,
            &self.prev_hash,
            &self.action,
            &self.actor,
            self.timestamp_ms,
        )
    }
}

/// Compute the canonical SHA-256 hex digest over `(sequence, prev_hash, action, actor, timestamp_ms)`.
///
/// Variable-length components are prefixed with their byte length so that
/// shifting characters between adjacent fields always produces a distinct
/// preimage.
pub fn compute_entry_hash(
    sequence: u64,
    prev_hash: &str,
    action: &AdminAction,
    actor: &str,
    timestamp_ms: u64,
) -> String {
    let action_encoded = action.canonical_encoding();
    let canonical = format!(
        "v1|seq={sequence}|prev={}:{prev_hash}|action={}:{action_encoded}|actor={}:{actor}|ts={timestamp_ms}",
        prev_hash.len(),
        action_encoded.len(),
        actor.len(),
    );
    sha256_hex(canonical.as_bytes())
}

/// Compute the lowercase hexadecimal SHA-256 digest of `data` (FIPS 180-4 / RFC 6234).
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = sha256_bytes(data);
    let mut out = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

/// Pure-Rust FIPS 180-4 SHA-256 implementation.
pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() + 8) % 64 != 0 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word_bytes) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word_bytes[0], word_bytes[1], word_bytes[2], word_bytes[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        let bytes = word.to_be_bytes();
        out[i * 4] = bytes[0];
        out[i * 4 + 1] = bytes[1];
        out[i * 4 + 2] = bytes[2];
        out[i * 4 + 3] = bytes[3];
    }
    out
}
