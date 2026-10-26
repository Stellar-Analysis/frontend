//! Role grant, revocation, and revocation-event emission for `access-control`.
//!
//! Core invariants:
//! - Only an account holding [`Role::Admin`] can grant or revoke roles or
//!   change the emergency pause state.
//! - Role membership is idempotent: granting an already-held role or revoking
//!   an absent role leaves membership state well-formed without duplicate
//!   entries.
//! - Every revocation increments a monotonic `revocation_epoch` and emits a
//!   [`RevocationEvent`] so any opt-in cache tied to `access-control` is
//!   deterministically invalidated immediately upon revocation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::roles::Role;

/// Event emitted whenever a role is revoked from a principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevocationEvent {
    /// Monotonically increasing revocation epoch after this revocation.
    pub revocation_epoch: u64,
    /// Administrator that executed the revocation.
    pub admin: String,
    /// Principal whose role was revoked.
    pub principal: String,
    /// The revoked role.
    pub role: Role,
}

/// Errors returned by `access-control` and dependent contract authorization checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessControlError {
    /// The caller does not hold `required_role` in `access-control`.
    Unauthorized { caller: String, required_role: Role },
    /// The contract is currently paused and rejects privileged state mutations.
    Paused,
    /// Attempted to revoke the last remaining [`Role::Admin`] account.
    CannotRevokeLastAdmin,
    /// Principal identifier is empty.
    InvalidPrincipal,
}

impl fmt::Display for AccessControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessControlError::Unauthorized {
                caller,
                required_role,
            } => write!(
                f,
                "principal '{caller}' lacks required role {required_role}"
            ),
            AccessControlError::Paused => write!(f, "access-control is paused"),
            AccessControlError::CannotRevokeLastAdmin => {
                write!(f, "cannot revoke the last remaining ADMIN principal")
            }
            AccessControlError::InvalidPrincipal => {
                write!(f, "principal address must not be empty")
            }
        }
    }
}

impl std::error::Error for AccessControlError {}

/// Storage state for role assignments, pause state, and revocation events.
#[derive(Debug, Clone)]
pub struct RoleRegistry {
    assignments: BTreeMap<Role, BTreeSet<String>>,
    paused: bool,
    revocation_epoch: u64,
    events: Vec<RevocationEvent>,
}

impl RoleRegistry {
    /// Initialise the registry with an initial administrator account.
    pub fn new(initial_admin: impl Into<String>) -> Result<Self, AccessControlError> {
        let admin = initial_admin.into();
        if admin.trim().is_empty() {
            return Err(AccessControlError::InvalidPrincipal);
        }
        let mut assignments: BTreeMap<Role, BTreeSet<String>> = BTreeMap::new();
        let mut admins = BTreeSet::new();
        admins.insert(admin);
        assignments.insert(Role::Admin, admins);
        Ok(Self {
            assignments,
            paused: false,
            revocation_epoch: 0,
            events: Vec::new(),
        })
    }

    /// Check live whether `principal` currently holds `role`.
    pub fn has_role(&self, principal: &str, role: Role) -> bool {
        self.assignments
            .get(&role)
            .is_some_and(|members| members.contains(principal))
    }

    /// Current monotonic revocation epoch. Increments on every role revocation.
    pub const fn revocation_epoch(&self) -> u64 {
        self.revocation_epoch
    }

    /// Whether the system is currently paused.
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Set emergency pause state. Restricted to [`Role::Admin`].
    pub fn set_paused(&mut self, caller: &str, paused: bool) -> Result<(), AccessControlError> {
        self.ensure_admin(caller)?;
        self.paused = paused;
        Ok(())
    }

    /// Grant `role` to `principal`. Restricted to [`Role::Admin`].
    ///
    /// Idempotent: returns `Ok(true)` if the role was newly added, or
    /// `Ok(false)` if `principal` already held `role`.
    pub fn grant_role(
        &mut self,
        caller: &str,
        principal: impl Into<String>,
        role: Role,
    ) -> Result<bool, AccessControlError> {
        self.ensure_admin(caller)?;
        if self.paused {
            return Err(AccessControlError::Paused);
        }
        let principal = principal.into();
        if principal.trim().is_empty() {
            return Err(AccessControlError::InvalidPrincipal);
        }
        let members = self.assignments.entry(role).or_default();
        Ok(members.insert(principal))
    }

    /// Revoke `role` from `principal`. Restricted to [`Role::Admin`].
    ///
    /// Idempotent: if `principal` held `role`, removes it, increments
    /// `revocation_epoch`, appends a [`RevocationEvent`], and returns
    /// `Ok(Some(event))`. If `principal` did not hold `role`, returns `Ok(None)`.
    pub fn revoke_role(
        &mut self,
        caller: &str,
        principal: &str,
        role: Role,
    ) -> Result<Option<RevocationEvent>, AccessControlError> {
        self.ensure_admin(caller)?;
        if principal.trim().is_empty() {
            return Err(AccessControlError::InvalidPrincipal);
        }

        if role == Role::Admin {
            let admin_count = self.assignments.get(&Role::Admin).map_or(0, BTreeSet::len);
            if admin_count <= 1 && self.has_role(principal, Role::Admin) {
                return Err(AccessControlError::CannotRevokeLastAdmin);
            }
        }

        let removed = self
            .assignments
            .get_mut(&role)
            .is_some_and(|members| members.remove(principal));

        if !removed {
            return Ok(None);
        }

        self.revocation_epoch = self.revocation_epoch.saturating_add(1);
        let event = RevocationEvent {
            revocation_epoch: self.revocation_epoch,
            admin: caller.to_string(),
            principal: principal.to_string(),
            role,
        };
        self.events.push(event.clone());
        Ok(Some(event))
    }

    /// Return the history of emitted [`RevocationEvent`]s.
    pub fn revocation_events(&self) -> &[RevocationEvent] {
        &self.events
    }

    fn ensure_admin(&self, caller: &str) -> Result<(), AccessControlError> {
        if !self.has_role(caller, Role::Admin) {
            return Err(AccessControlError::Unauthorized {
                caller: caller.to_string(),
                required_role: Role::Admin,
            });
        }
        Ok(())
    }
}
