//! Client IP resolution from `X-Forwarded-For`.
//!
//! `X-Forwarded-For` is a comma-separated list of addresses, appended to by
//! each proxy as the request traverses the chain. The leftmost entry is the
//! original client as reported by the first proxy, but every entry to the
//! left of the trusted proxies is attacker-controlled. The correct client IP
//! is therefore found by walking the chain from the trusted end: skip the
//! entries appended by trusted proxies and take the first address that a
//! trusted proxy actually observed.

use std::net::IpAddr;

use super::trusted_hops::TrustedHops;

/// Resolve the client IP for a request.
///
/// * `peer` is the immediate network peer (the address the connection was
///   accepted from).
/// * `forwarded_for` is the raw `X-Forwarded-For` header value, if present.
/// * `trusted_hops` is the configured number of trusted proxy hops.
///
/// If no proxy hop is trusted, or the header is absent/unparseable, the
/// immediate peer is returned. Otherwise the forwarded chain is walked from
/// the trusted end and the first address observed by a trusted proxy is
/// returned. A header supplied directly by an untrusted peer is ignored.
pub fn resolve_client_ip(
    peer: IpAddr,
    forwarded_for: Option<&str>,
    trusted_hops: TrustedHops,
) -> IpAddr {
    if !trusted_hops.is_trusted() {
        return peer;
    }

    let header = match forwarded_for {
        Some(value) if !value.trim().is_empty() => value,
        _ => return peer,
    };

    let chain: Vec<IpAddr> = header
        .split(',')
        .filter_map(|entry| entry.trim().parse::<IpAddr>().ok())
        .collect();

    if chain.is_empty() {
        return peer;
    }

    // The chain lists addresses from the original client (leftmost) to the
    // most recent proxy (rightmost). The rightmost `trusted_hops` entries
    // were appended by trusted proxies; the entry immediately to their left
    // is the address the outermost trusted proxy observed, i.e. the client.
    let hops = trusted_hops.count() as usize;
    if chain.len() <= hops {
        // Fewer forwarded entries than trusted hops: the chain is shorter
        // than the configured topology, so it cannot be trusted. Fall back to
        // the immediate peer rather than trusting a partial chain.
        return peer;
    }

    chain[chain.len() - hops - 1]
}
