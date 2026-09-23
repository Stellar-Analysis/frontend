//! Governance voting: weighted vote tallying with single-count delegation.

pub mod delegation;

pub use delegation::{Delegation, DelegationError};

/// Identifier for a voter.
pub type VoterId = u64;

/// A weighted vote tally that counts each unit of weight exactly once.
#[derive(Debug, Default)]
pub struct VoteTally {
    /// Weight cast per voter.
    weights: std::collections::BTreeMap<VoterId, u128>,
    /// Sum of all weight cast.
    total: u128,
}

impl VoteTally {
    /// Create an empty tally.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `weight` for `voter`, rejecting duplicate voters.
    pub fn record(&mut self, voter: VoterId, weight: u128) -> Result<(), DelegationError> {
        if self.weights.contains_key(&voter) {
            return Err(DelegationError::AlreadyVotedDirectly);
        }
        self.weights.insert(voter, weight);
        self.total = self.total.saturating_add(weight);
        Ok(())
    }

    /// Total weight cast.
    pub fn total(&self) -> u128 {
        self.total
    }
}
