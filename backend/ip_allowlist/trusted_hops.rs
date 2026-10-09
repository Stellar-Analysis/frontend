//! Trusted-proxy-hop configuration for client IP resolution.
//!
//! # Proxy topology
//!
//! This service is intended to run behind a single reverse proxy / load
//! balancer (e.g. nginx, an ALB, or an ingress controller) that terminates
//! client connections and appends the peer address to `X-Forwarded-For`.
//! The proxy is expected to *strip* any inbound `X-Forwarded-For` header and
//! re-set it with the address it actually observed, so the header cannot be
//! spoofed by an external client.
//!
//! ```text
//! client ──▶ reverse proxy (trusted) ──▶ this service
//! ```
//!
//! Because the application cannot verify the network topology for itself, the
//! number of trusted hops is configured explicitly here rather than inferred.
//! A value of `0` means "no proxy in front of us": the direct peer address is
//! the client and `X-Forwarded-For` is ignored entirely.

/// Number of trusted proxy hops in front of this service.
///
/// This is the count of proxies between the client and this service whose
/// `X-Forwarded-For` entries may be trusted. It must be configured to match
/// the real deployment topology; it is never derived from request data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedHops(usize);

impl TrustedHops {
    /// No proxy in front of the service. The direct peer is the client and
    /// `X-Forwarded-For` is not trusted at all.
    pub const NONE: TrustedHops = TrustedHops(0);

    /// A single trusted reverse proxy / load balancer in front of the service.
    pub const SINGLE_PROXY: TrustedHops = TrustedHops(1);

    /// Construct from an explicit, configured hop count.
    pub const fn new(hops: usize) -> Self {
        TrustedHops(hops)
    }

    /// The configured number of trusted hops.
    pub const fn get(self) -> usize {
        self.0
    }

    /// Whether any proxy hop is trusted. When `false`, forwarded headers must
    /// be ignored and the direct peer address used instead.
    pub const fn trusts_any_proxy(self) -> bool {
        self.0 > 0
    }
}

impl Default for TrustedHops {
    /// Defaults to trusting a single proxy, matching the documented topology.
    /// Deployments without a proxy must set [`TrustedHops::NONE`] explicitly.
    fn default() -> Self {
        TrustedHops::SINGLE_PROXY
    }
}

impl From<usize> for TrustedHops {
    fn from(hops: usize) -> Self {
        TrustedHops::new(hops)
    }
}
