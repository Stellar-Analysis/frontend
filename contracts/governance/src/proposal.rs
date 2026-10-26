//! Proposal lifecycle and proposal-creation weight snapshotting.
//!
//! When a proposal is created, the current eligible weight of every registered
//! voter and their sum (`snapshotted_total_eligible_weight`) are frozen onto
//! the [`Proposal`]. Late weight acquisition after proposal creation cannot
//! inflate a voter's ballot or alter the proposal's quorum denominator.

use std::collections::{BTreeMap, BTreeSet};

use super::quorum::{evaluate_quorum, QuorumConfig, QuorumEvaluation};

/// Vote choice for a governance ballot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteChoice {
    For,
    Against,
    Abstain,
}

/// Terminal or active state of a governance proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    Active,
    DefeatedQuorumNotMet,
    DefeatedRejected,
    Succeeded,
    Executed,
}

/// A governance proposal with its frozen eligible-weight snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub id: u64,
    pub proposer: String,
    pub description: String,
    pub created_at: u64,
    pub voting_ends_at: u64,
    /// Total eligible voting weight frozen at `created_at`.
    pub snapshotted_total_eligible_weight: u128,
    /// Per-voter eligible weight frozen at `created_at`.
    pub snapshotted_voter_weights: BTreeMap<String, u128>,
    pub for_weight: u128,
    pub against_weight: u128,
    pub abstain_weight: u128,
    pub voters: BTreeSet<String>,
    pub state: ProposalState,
}

impl Proposal {
    /// Snapshot the current voter weight map at proposal creation time.
    pub fn new(
        id: u64,
        proposer: impl Into<String>,
        description: impl Into<String>,
        created_at: u64,
        voting_ends_at: u64,
        eligible_weights: &BTreeMap<String, u128>,
    ) -> Self {
        let mut snapshotted_voter_weights = BTreeMap::new();
        let mut total: u128 = 0;
        for (voter, &weight) in eligible_weights {
            if weight > 0 {
                snapshotted_voter_weights.insert(voter.clone(), weight);
                total = total.saturating_add(weight);
            }
        }

        Self {
            id,
            proposer: proposer.into(),
            description: description.into(),
            created_at,
            voting_ends_at,
            snapshotted_total_eligible_weight: total,
            snapshotted_voter_weights,
            for_weight: 0,
            against_weight: 0,
            abstain_weight: 0,
            voters: BTreeSet::new(),
            state: ProposalState::Active,
        }
    }

    /// Evaluate the current tallies against `config` using the frozen
    /// `snapshotted_total_eligible_weight` denominator.
    pub const fn evaluate(&self, config: QuorumConfig) -> QuorumEvaluation {
        evaluate_quorum(
            config,
            self.snapshotted_total_eligible_weight,
            self.for_weight,
            self.against_weight,
            self.abstain_weight,
        )
    }
}
