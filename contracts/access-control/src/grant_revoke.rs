//! Grant and revoke entry points for the access-control contract.
//!
//! # Live authorization rule
//!
//! Every privileged entry point in every dependent contract (`analytics`,
//! `escrow`, `governance`, `governance-voting`, `multi-sig-wallet`,
//! `time-locked-transactions`, `token-swap`, `upgrade`) MUST call
//! [`has_role`] (or [`require_role`]) live on every invocation. Authorization
//! results MUST NOT be cached across calls: a cached `true` would outlive a
//! revocation performed here and silently keep a revoked principal privileged.
//!
//! If a dependent genuinely needs to cache for performance, the cache MUST be
//! invalidated on the revocation event emitted by [`revoke_role`], and a test
//! MUST prove the cache cannot outlive a revocation. Absent such a test, the
//! only correct implementation is a fresh cross-contract call.

use soroban_sdk::{contracterror, contractevent, Address, Env};

use crate::roles::{self, Role};

/// Errors returned by the grant/revoke surface.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AccessControlError {
    /// The caller is not authorized to grant or revoke roles.
    Unauthorized = 1,
    /// The target already holds the role being granted.
    RoleAlreadyGranted = 2,
    /// The target does not hold the role being revoked.
    RoleNotGranted = 3,
}

/// Emitted whenever a role is granted. Dependents that cache authorization
/// MUST treat this as an invalidation signal.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleGranted {
    #[topic]
    pub role: Role,
    #[topic]
    pub account: Address,
}

/// Emitted whenever a role is revoked. Dependents that cache authorization
/// MUST invalidate the affected entry on this event; a cache that can outlive
/// this event is a security bug.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleRevoked {
    #[topic]
    pub role: Role,
    #[topic]
    pub account: Address,
}

/// Grant `role` to `account`.
///
/// Only an account already holding the admin role may grant roles. Granting a
/// role the account already holds is rejected so that the emitted
/// [`RoleGranted`] event is always a real state transition.
///
/// # Errors
///
/// * [`AccessControlError::Unauthorized`] if `caller` is not an admin.
/// * [`AccessControlError::RoleAlreadyGranted`] if `account` already has `role`.
pub fn grant_role(
    env: &Env,
    caller: &Address,
    account: &Address,
    role: Role,
) -> Result<(), AccessControlError> {
    caller.require_auth();

    if !roles::has_role(env, caller, Role::Admin) {
        return Err(AccessControlError::Unauthorized);
    }

    if roles::has_role(env, account, role.clone()) {
        return Err(AccessControlError::RoleAlreadyGranted);
    }

    roles::set_role(env, account, role.clone(), true);
    RoleGranted {
        role,
        account: account.clone(),
    }
    .publish(env);

    Ok(())
}

/// Revoke `role` from `account`.
///
/// Only an account already holding the admin role may revoke roles. Revoking a
/// role the account does not hold is rejected so that the emitted
/// [`RoleRevoked`] event is always a real state transition. Dependents that
/// cache authorization MUST invalidate on this event.
///
/// # Errors
///
/// * [`AccessControlError::Unauthorized`] if `caller` is not an admin.
/// * [`AccessControlError::RoleNotGranted`] if `account` does not have `role`.
pub fn revoke_role(
    env: &Env,
    caller: &Address,
    account: &Address,
    role: Role,
) -> Result<(), AccessControlError> {
    caller.require_auth();

    if !roles::has_role(env, caller, Role::Admin) {
        return Err(AccessControlError::Unauthorized);
    }

    if !roles::has_role(env, account, role.clone()) {
        return Err(AccessControlError::RoleNotGranted);
    }

    roles::set_role(env, account, role.clone(), false);
    RoleRevoked {
        role,
        account: account.clone(),
    }
    .publish(env);

    Ok(())
}

/// Live authorization check for dependent contracts.
///
/// This is the entry point every privileged function in a dependent contract
/// MUST call on every invocation. It reads current state directly and never
/// consults a cache, so a revocation performed via [`revoke_role`] takes effect
/// on the very next call.
///
/// # Errors
///
/// * [`AccessControlError::Unauthorized`] if `account` does not currently hold
///   `role`.
pub fn require_role(
    env: &Env,
    account: &Address,
    role: Role,
) -> Result<(), AccessControlError> {
    if roles::has_role(env, account, role) {
        Ok(())
    } else {
        Err(AccessControlError::Unauthorized)
    }
}
