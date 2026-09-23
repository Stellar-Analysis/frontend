//! Role definitions and live role-check helpers for the shared access-control contract.
//!
//! # Hard rule: authorization is always checked live, never cached
//!
//! Every privileged entry point in every dependent contract (`analytics`,
//! `escrow`, `governance`, `governance-voting`, `multi-sig-wallet`,
//! `time-locked-transactions`, `token-swap`, `upgrade`) MUST call
//! `access-control` live on each invocation. A role check result must never be
//! stored and reused across calls, because a revocation performed in this
//! contract would otherwise be invisible to a dependent that trusted a stale
//! cached answer.
//!
//! If a dependent genuinely needs to cache a cross-contract call for
//! performance, the cache MUST be explicitly invalidated on this contract's
//! revocation event, and a test must prove the cache cannot outlive a
//! revocation. Absent such a test, no caching is permitted.

use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Storage key namespace for role membership.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoleKey {
    /// Membership flag for `(role, account)`.
    Member(Symbol, Address),
    /// Monotonic counter bumped on every grant/revoke so dependents can detect
    /// that a cached authorization is stale and must be re-fetched live.
    RevocationEpoch,
}

/// Returns the current revocation epoch.
///
/// Dependents that cache an authorization MUST record the epoch observed at
/// cache time and re-check it live before trusting the cached value. A change
/// in epoch means a grant or revocation happened and the cache is invalid.
pub fn revocation_epoch(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&RoleKey::RevocationEpoch)
        .unwrap_or(0u64)
}

/// Bumps the revocation epoch. Called by `grant_revoke` on every grant/revoke.
pub fn bump_revocation_epoch(env: &Env) -> u64 {
    let next = revocation_epoch(env) + 1;
    env.storage()
        .instance()
        .set(&RoleKey::RevocationEpoch, &next);
    next
}

/// Live role check. Reads current membership from storage on every call.
///
/// This is the only sanctioned way for a dependent contract to answer "does
/// `account` hold `role`?". It never consults a cache.
pub fn has_role(env: &Env, role: &Symbol, account: &Address) -> bool {
    env.storage()
        .instance()
        .get(&RoleKey::Member(role.clone(), account.clone()))
        .unwrap_or(false)
}

/// Live role check that panics when the account does not hold the role.
///
/// Privileged entry points in dependent contracts should call this (or
/// [`has_role`]) directly on each invocation rather than trusting a previously
/// observed result.
pub fn require_role(env: &Env, role: &Symbol, account: &Address) {
    if !has_role(env, role, account) {
        panic!("access-control: account does not hold required role");
    }
}

/// Writes membership for `(role, account)` and bumps the revocation epoch so
/// any dependent cache is provably invalidated.
pub fn set_role(env: &Env, role: &Symbol, account: &Address, member: bool) {
    env.storage()
        .instance()
        .set(&RoleKey::Member(role.clone(), account.clone()), &member);
    bump_revocation_epoch(env);
}
