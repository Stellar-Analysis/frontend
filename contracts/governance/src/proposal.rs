//! Proposal state, including the total eligible weight snapshotted at creation.

use std::collections::BTreeMap;

use crate::{ProposalId, VoterId};

/// Errors returned when mutating a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalError {
    /// No proposal exists for the given id.
    UnknownProposal,
    /// The voter has already cast a vote on this proposal.
    AlreadyVoted,
}

/// Lifecycle status of a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Votes are still being accepted.
    Open,
    /// Voting has closed and quorum was met.
    QuorumMet,
    /// Voting has closed and quorum was not met.
    QuorumNotMet,
}

/// A governance proposal.
///
/// `total_eligible_weight` is captured once, at creation time, and is the
/// denominator used for quorum. It is deliberately not recomputed from live
/// state so that late delegation or acquisition cannot influence quorum.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub id: ProposalId,
    /// Total eligible voting weight snapshotted at proposal-creation time.
    pub total_eligible_weight: u128,
    /// Quorum threshold in basis points of `total_eligible_weight`.
    pub quorum_bps: u16,
    /// Weight cast per voter.
    pub votes: BTreeMap<VoterId, u128>,
    /// Sum of all weight cast so far.
    pub votes_cast: u128,
    pub status: ProposalStatus,
}

impl Proposal {
    /// Create a proposal with the given snapshotted eligible weight.
    pub fn new(id: ProposalId, total_eligible_weight: u128, quorum_bps: u16) -> Self {
        Self {
            id,
            total_eligible_weight,
            quorum_bps,
            votes: BTreeMap::new(),
            votes_cast: 0,
            status: ProposalStatus::Open,
        }
    }

    /// Record a vote, rejecting duplicate voters.
    pub fn cast_vote(&mut self, voter: VoterId, weight: u128) -> Result<(), ProposalError> {
        if self.votes.contains_key(&voter) {
            return Err(ProposalError::AlreadyVoted);
        }
        self.votes.insert(voter, weight);
        self.votes_cast = self.votes_cast.saturating_add(weight);
        Ok(())
    }
}
