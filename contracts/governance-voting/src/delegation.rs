//! Voting weight delegation where each unit of weight is counted at most once.
//!
//! # Core Invariants
//!
//! 1. **Exact weight conservation**: Across all registered accounts, the sum of
//!    `effective_voting_weight(account)` is always strictly equal to the sum of
//!    `base_weight(account)`.
//! 2. **No double voting by delegator + delegate**: Once `delegator` delegates
//!    their weight to `delegate`, `delegator`'s effective voting weight becomes
//!    `0`, while `delegate`'s effective voting weight increases by
//!    `base_weight(delegator)`.
//! 3. **Single-hop acyclic delegation**: A delegator cannot self-delegate,
//!    cannot delegate to an account that has itself delegated out, and an
//!    account that currently receives delegations cannot delegate out without
//!    first having its delegators undelegate. This guarantees `O(1)` resolution
//!    with zero cycle or transitive double-count risk.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Errors returned when configuring or exercising vote delegation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegationError {
    /// An account attempted to delegate to itself.
    SelfDelegation { account: String },
    /// The delegator has no registered voting weight.
    NoRegisteredWeight { account: String },
    /// Target delegate has itself delegated to another account (transitive chain).
    TargetIsDelegating {
        delegate: String,
        upstream_delegate: String,
    },
    /// Delegator currently holds incoming delegations from other voters and
    /// therefore cannot re-delegate their combined power downstream.
    AccountHasIncomingDelegations { account: String },
}

impl fmt::Display for DelegationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DelegationError::SelfDelegation { account } => {
                write!(f, "account '{account}' cannot delegate to itself")
            }
            DelegationError::NoRegisteredWeight { account } => {
                write!(f, "account '{account}' has no registered base weight")
            }
            DelegationError::TargetIsDelegating {
                delegate,
                upstream_delegate,
            } => write!(
                f,
                "cannot delegate to '{delegate}' because it already delegates to '{upstream_delegate}'"
            ),
            DelegationError::AccountHasIncomingDelegations { account } => write!(
                f,
                "account '{account}' receives incoming delegations and cannot delegate out"
            ),
        }
    }
}

impl std::error::Error for DelegationError {}

/// Ledger of registered base weights and single-hop delegation assignments.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DelegationRegistry {
    base_weights: BTreeMap<String, u128>,
    /// Maps `delegator -> delegate`.
    delegated_to: BTreeMap<String, String>,
    /// Maps `delegate -> set of delegators`.
    incoming_from: BTreeMap<String, BTreeSet<String>>,
}

impl DelegationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the registered base voting weight for `account`.
    pub fn set_base_weight(&mut self, account: impl Into<String>, weight: u128) {
        let account = account.into();
        if weight == 0 {
            if let Some(delegate) = self.delegated_to.remove(&account) {
                if let Some(set) = self.incoming_from.get_mut(&delegate) {
                    set.remove(&account);
                }
            }
            self.base_weights.remove(&account);
        } else {
            self.base_weights.insert(account, weight);
        }
    }

    /// Raw registered weight for `account` before delegation.
    pub fn base_weight(&self, account: &str) -> u128 {
        self.base_weights.get(account).copied().unwrap_or(0)
    }

    /// Total registered eligible weight across all accounts.
    pub fn total_eligible_weight(&self) -> u128 {
        self.base_weights
            .values()
            .copied()
            .fold(0u128, u128::saturating_add)
    }

    /// Return the delegate to whom `delegator` has assigned their weight, if any.
    pub fn delegate_of(&self, delegator: &str) -> Option<&str> {
        self.delegated_to.get(delegator).map(String::as_str)
    }

    /// Delegate `delegator`'s entire base voting weight to `delegate`.
    pub fn delegate(
        &mut self,
        delegator: impl Into<String>,
        delegate: impl Into<String>,
    ) -> Result<(), DelegationError> {
        let delegator = delegator.into();
        let delegate = delegate.into();

        if delegator == delegate {
            return Err(DelegationError::SelfDelegation { account: delegator });
        }
        if self.base_weight(&delegator) == 0 {
            return Err(DelegationError::NoRegisteredWeight { account: delegator });
        }
        if let Some(upstream) = self.delegated_to.get(&delegate) {
            return Err(DelegationError::TargetIsDelegating {
                delegate,
                upstream_delegate: upstream.clone(),
            });
        }
        if self
            .incoming_from
            .get(&delegator)
            .is_some_and(|incoming| !incoming.is_empty())
        {
            return Err(DelegationError::AccountHasIncomingDelegations { account: delegator });
        }

        // Remove any previous delegation from `delegator` before re-pointing.
        if let Some(prev_delegate) = self
            .delegated_to
            .insert(delegator.clone(), delegate.clone())
        {
            if let Some(set) = self.incoming_from.get_mut(&prev_delegate) {
                set.remove(&delegator);
            }
        }
        self.incoming_from
            .entry(delegate)
            .or_default()
            .insert(delegator);

        Ok(())
    }

    /// Revoke any active delegation from `delegator`, restoring its own base weight.
    pub fn undelegate(&mut self, delegator: &str) -> bool {
        let Some(prev_delegate) = self.delegated_to.remove(delegator) else {
            return false;
        };
        if let Some(set) = self.incoming_from.get_mut(&prev_delegate) {
            set.remove(delegator);
        }
        true
    }

    /// Compute the effective voting weight `voter` can exercise directly.
    ///
    /// - If `voter` has delegated their weight to someone else, returns `0`.
    /// - Otherwise returns `base_weight(voter)` plus the `base_weight` of every
    ///   account currently delegating to `voter`.
    pub fn effective_weight(&self, voter: &str) -> u128 {
        if self.delegated_to.contains_key(voter) {
            return 0;
        }
        let own = self.base_weight(voter);
        let delegated_in = self
            .incoming_from
            .get(voter)
            .map(|delegators| {
                delegators
                    .iter()
                    .map(|d| self.base_weight(d))
                    .fold(0u128, u128::saturating_add)
            })
            .unwrap_or(0);
        own.saturating_add(delegated_in)
    }

    /// Return the list of underlying weight owners whose weight is represented
    /// by `voter`'s ballot (including `voter` if `base_weight(voter) > 0` and
    /// not delegated out, plus all active delegators to `voter`).
    pub fn represented_principals(&self, voter: &str) -> Vec<(String, u128)> {
        if self.delegated_to.contains_key(voter) {
            return Vec::new();
        }
        let mut out = Vec::new();
        let own = self.base_weight(voter);
        if own > 0 {
            out.push((voter.to_string(), own));
        }
        if let Some(delegators) = self.incoming_from.get(voter) {
            for d in delegators {
                let w = self.base_weight(d);
                if w > 0 {
                    out.push((d.clone(), w));
                }
            }
        }
        out
    }
}
