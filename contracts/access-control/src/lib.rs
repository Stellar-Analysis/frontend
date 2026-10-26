//! Shared `access-control` contract and live-authorization enforcement across
//! all dependent contracts (`analytics`, `escrow`, `governance`,
//! `governance-voting`, `multi-sig-wallet`, `time-locked-transactions`,
//! `token-swap`, and `upgrade`).
//!
//! # Hard Architectural Rule: No Uninvalidated Cached Authorization
//!
//! 1. **Live cross-contract check by default**: Every privileged entry point in
//!    every dependent contract must invoke [`AccessControlContract::require_role`]
//!    at execution time rather than storing a local `is_authorized` flag or
//!    copying role state during initialization.
//! 2. **Epoch-bound invalidation if caching is ever required**: When a dependent
//!    must memoize a check within a bounded context, it must use
//!    [`RevocationAwareCache`], which binds every cached entry to
//!    [`AccessControlContract::revocation_epoch`] and is immediately invalidated
//!    whenever `access-control` emits a [`RevocationEvent`].

pub mod grant_revoke;
pub mod roles;

use std::collections::BTreeMap;

pub use grant_revoke::{AccessControlError, RevocationEvent, RoleRegistry};
pub use roles::{DependentContract, Role};

/// Proof token produced only by a live [`AccessControlContract::require_role`]
/// check at the current `revocation_epoch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveAuthProof {
    pub caller: String,
    pub role: Role,
    pub revocation_epoch: u64,
}

/// Shared role and permission source-of-truth contract.
#[derive(Debug, Clone)]
pub struct AccessControlContract {
    registry: RoleRegistry,
}

impl AccessControlContract {
    /// Deploy `access-control` with an initial administrator account.
    pub fn new(initial_admin: impl Into<String>) -> Result<Self, AccessControlError> {
        Ok(Self {
            registry: RoleRegistry::new(initial_admin)?,
        })
    }

    /// Query whether `principal` currently holds `role`.
    pub fn has_role(&self, principal: &str, role: Role) -> bool {
        self.registry.has_role(principal, role)
    }

    /// Current monotonic revocation epoch.
    pub const fn revocation_epoch(&self) -> u64 {
        self.registry.revocation_epoch()
    }

    /// Whether the contract is currently paused.
    pub const fn is_paused(&self) -> bool {
        self.registry.is_paused()
    }

    /// Perform a live cross-contract authorization check.
    ///
    /// Rejects immediately if the system is paused or if `caller` does not
    /// currently hold `required_role`.
    pub fn require_role(
        &self,
        caller: &str,
        required_role: Role,
    ) -> Result<LiveAuthProof, AccessControlError> {
        if self.registry.is_paused() {
            return Err(AccessControlError::Paused);
        }
        if !self.registry.has_role(caller, required_role) {
            return Err(AccessControlError::Unauthorized {
                caller: caller.to_string(),
                required_role,
            });
        }
        Ok(LiveAuthProof {
            caller: caller.to_string(),
            role: required_role,
            revocation_epoch: self.registry.revocation_epoch(),
        })
    }

    /// Grant `role` to `principal`. Caller must hold [`Role::Admin`].
    pub fn grant_role(
        &mut self,
        caller: &str,
        principal: impl Into<String>,
        role: Role,
    ) -> Result<bool, AccessControlError> {
        self.registry.grant_role(caller, principal, role)
    }

    /// Revoke `role` from `principal`. Caller must hold [`Role::Admin`].
    pub fn revoke_role(
        &mut self,
        caller: &str,
        principal: &str,
        role: Role,
    ) -> Result<Option<RevocationEvent>, AccessControlError> {
        self.registry.revoke_role(caller, principal, role)
    }

    /// Toggle pause state. Caller must hold [`Role::Admin`].
    pub fn set_paused(&mut self, caller: &str, paused: bool) -> Result<(), AccessControlError> {
        self.registry.set_paused(caller, paused)
    }

    /// Emitted revocation events.
    pub fn revocation_events(&self) -> &[RevocationEvent] {
        self.registry.revocation_events()
    }
}

/// Revocation-event-invalidated authorization cache for high-frequency callers.
///
/// Any cached entry is keyed by `(principal, role)` and tagged with the
/// `revocation_epoch` at the moment it was populated. Both explicit
/// [`RevocationAwareCache::on_revocation_event`] notifications and live epoch
/// checks ensure a cached grant can never outlive a revocation in
/// [`AccessControlContract`].
#[derive(Debug, Default, Clone)]
pub struct RevocationAwareCache {
    cached_epoch: u64,
    entries: BTreeMap<(String, Role), u64>,
}

impl RevocationAwareCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle a [`RevocationEvent`] emitted by `access-control` by immediately
    /// evicting the revoked `(principal, role)` and advancing the observed epoch.
    pub fn on_revocation_event(&mut self, event: &RevocationEvent) {
        self.entries.remove(&(event.principal.clone(), event.role));
        if event.revocation_epoch > self.cached_epoch {
            self.cached_epoch = event.revocation_epoch;
        }
    }

    /// Verify `caller` holds `role` in `access_control`, invalidating any stale
    /// cached state if `access_control.revocation_epoch()` has advanced or if a
    /// revocation event was emitted.
    pub fn check_or_revalidate(
        &mut self,
        access_control: &AccessControlContract,
        caller: &str,
        role: Role,
    ) -> Result<LiveAuthProof, AccessControlError> {
        let live_epoch = access_control.revocation_epoch();
        if live_epoch != self.cached_epoch {
            self.entries.clear();
            self.cached_epoch = live_epoch;
        }

        let key = (caller.to_string(), role);
        if let Some(&entry_epoch) = self.entries.get(&key) {
            if entry_epoch == live_epoch && !access_control.is_paused() {
                return Ok(LiveAuthProof {
                    caller: caller.to_string(),
                    role,
                    revocation_epoch: live_epoch,
                });
            }
            self.entries.remove(&key);
        }

        let proof = access_control.require_role(caller, role)?;
        self.entries.insert(key, proof.revocation_epoch);
        Ok(proof)
    }
}

/// Receipt returned when a dependent contract executes a privileged entry point
/// after verifying authorization live against [`AccessControlContract`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivilegedExecutionReceipt {
    pub contract: DependentContract,
    pub action: String,
    pub proof: LiveAuthProof,
}

/// Common interface implemented by every dependent contract in the workspace to
/// guarantee live cross-contract authorization checks on privileged entry points.
pub trait PrivilegedDependentContract {
    /// Identify which dependent contract this instance represents.
    fn contract_id(&self) -> DependentContract;

    /// Execute a privileged entry point after performing a fresh, uncached
    /// cross-contract check against `access_control`.
    fn execute_privileged(
        &mut self,
        access_control: &AccessControlContract,
        caller: &str,
        action_payload: &str,
    ) -> Result<PrivilegedExecutionReceipt, AccessControlError>;

    /// Number of privileged operations successfully committed on this contract.
    fn committed_operations(&self) -> usize;
}

/// Dependent contract implementation enforcing live `access-control` checks on
/// every privileged call (no cached role booleans).
#[derive(Debug, Clone)]
pub struct LiveCheckedDependent {
    contract: DependentContract,
    committed_actions: Vec<String>,
}

impl LiveCheckedDependent {
    pub const fn new(contract: DependentContract) -> Self {
        Self {
            contract,
            committed_actions: Vec::new(),
        }
    }
}

impl PrivilegedDependentContract for LiveCheckedDependent {
    fn contract_id(&self) -> DependentContract {
        self.contract
    }

    fn execute_privileged(
        &mut self,
        access_control: &AccessControlContract,
        caller: &str,
        action_payload: &str,
    ) -> Result<PrivilegedExecutionReceipt, AccessControlError> {
        let required_role = self.contract.required_role();
        let proof = access_control.require_role(caller, required_role)?;
        self.committed_actions.push(action_payload.to_string());
        Ok(PrivilegedExecutionReceipt {
            contract: self.contract,
            action: action_payload.to_string(),
            proof,
        })
    }

    fn committed_operations(&self) -> usize {
        self.committed_actions.len()
    }
}

/// Construct the full suite of all 8 dependent contracts (`analytics`, `escrow`,
/// `governance`, `governance-voting`, `multi-sig-wallet`,
/// `time-locked-transactions`, `token-swap`, `upgrade`) wired to check
/// [`AccessControlContract`] live on every privileged call.
pub fn all_dependent_contracts() -> [LiveCheckedDependent; 8] {
    DependentContract::ALL.map(LiveCheckedDependent::new)
}
