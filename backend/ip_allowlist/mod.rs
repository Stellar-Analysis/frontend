//! IP allowlisting for admin routes and internal metrics endpoints.
//!
//! # Proxy topology
//!
//! This service is intended to run behind a reverse proxy / load balancer
//! (e.g. nginx or an L7 LB) that terminates client connections and appends
//! the peer address to `X-Forwarded-For`. The application itself cannot
//! verify the network topology, so the number of trusted proxy hops in front
//! of it is configured explicitly (see [`trusted_hops`]).
//!
//! `X-Forwarded-For` is only honored when the immediate peer is a trusted
//! proxy; the client IP is then derived by walking the forwarded chain from
//! the trusted end (see [`resolve`]). A header supplied directly by an
//! untrusted peer is ignored, so an external client cannot spoof an
//! allowlisted address.

pub mod resolve;
pub mod trusted_hops;

pub use resolve::resolve_client_ip;
pub use trusted_hops::TrustedHops;

use std::net::IpAddr;

/// Allowlist of source IPs permitted to reach protected routes.
#[derive(Debug, Clone)]
pub struct IpAllowlist {
    allowed: Vec<IpAddr>,
    trusted_hops: TrustedHops,
}

impl IpAllowlist {
    /// Build an allowlist from a set of permitted IPs and an explicit
    /// trusted-proxy-hop count.
    pub fn new(allowed: Vec<IpAddr>, trusted_hops: TrustedHops) -> Self {
        Self {
            allowed,
            trusted_hops,
        }
    }

    /// Resolve the client IP for a request given the immediate peer address
    /// and the raw `X-Forwarded-For` header value (if any), then check it
    /// against the allowlist.
    pub fn is_allowed(&self, peer: IpAddr, forwarded_for: Option<&str>) -> bool {
        let client = resolve_client_ip(peer, forwarded_for, self.trusted_hops);
        self.allowed.contains(&client)
    }
}
