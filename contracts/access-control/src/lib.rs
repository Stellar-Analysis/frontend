//! # Access Control
//!
//! Shared role/permission source of truth for every other contract in this
//! repository (`analytics`, `escrow`, `governance`, `governance-voting`,
//! `multi-sig-wallet`, `time-locked-transactions`, `token-swap`, `upgrade`).
//!
//! ## Hard rule: authorization is always checked live, never cached
//!
//! Every privileged entry point in every dependent contract MUST call this
//! contract live (a fresh cross-contract call) on each invocation. A cached
//! authorization result is only acceptable when it is explicitly invalidated
//! by this contract's revocation event, and a test must prove the cache cannot
//! outlive a revocation.
//!
//! This module is the single source of truth for roles and permissions. It
//! deliberately keeps no per-caller cache of its own: `has_role` reads the
//! current storage state on every call so that a revocation is observable
//! immediately by any dependent contract that re-checks.

#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Symbol};

pub mod grant_revoke;
pub mod roles;

pub use grant_revoke::{grant_role, revoke_role};
pub use roles::{has_role, require_role, Role, RoleError};

/// Storage keys used by the access-control contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Role assignment for a given (account, role) pair.
    Role(Address, Symbol),
    /// Monotonically increasing counter bumped on every revocation.
    /// Dependent contracts that cache may subscribe to this value to
    /// invalidate their cache; it can never decrease, so a stale cache is
    /// always detectable.
    RevocationNonce,
}

/// Errors surfaced by the access-control contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AccessControlError {
    /// The caller does not hold the required role.
    Unauthorized = 1,
    /// The role symbol was empty or otherwise invalid.
    InvalidRole = 2,
}

#[contract]
pub struct AccessControl;

#[contractimpl]
impl AccessControl {
    /// Grant `role` to `account`. Only an existing admin may grant roles.
    pub fn grant(env: Env, admin: Address, account: Address, role: Symbol) -> Result<(), AccessControlError> {
        grant_revoke::grant_role(&env, &admin, &account, &role)
    }

    /// Revoke `role` from `account`. Only an existing admin may revoke roles.
    ///
    /// Revocation bumps the revocation nonce so that any dependent contract
    /// holding a cache can detect that its cached authorization is stale.
    pub fn revoke(env: Env, admin: Address, account: Address, role: Symbol) -> Result<(), AccessControlError> {
        grant_revoke::revoke_role(&env, &admin, &account, &role)
    }

    /// Live role check. Reads current storage on every call; never cached.
    pub fn has_role(env: Env, account: Address, role: Symbol) -> bool {
        roles::has_role(&env, &account, &role)
    }

    /// Live role check that traps when the account lacks the role.
    pub fn require_role(env: Env, account: Address, role: Symbol) -> Result<(), AccessControlError> {
        roles::require_role(&env, &account, &role)
    }

    /// Current revocation nonce. Dependent contracts that cache authorization
    /// MUST compare this value against the nonce observed when the cache was
    /// populated and treat any difference as a cache miss.
    pub fn revocation_nonce(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::RevocationNonce)
            .unwrap_or(0)
    }
}
