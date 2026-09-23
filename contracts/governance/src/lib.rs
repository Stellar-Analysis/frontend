//! Governance contract: proposal creation and weighted vote tallying.
//!
//! Quorum is measured against the total eligible voting weight snapshotted at
//! proposal-creation time. It is never measured against votes actually cast and
//! is never read live, so late delegation or token acquisition cannot be used to
//! influence quorum after seeing how a vote is trending.

pub mod proposal;
pub mod quorum;

pub use proposal::{Proposal, ProposalError, ProposalStatus};
pub use quorum::{QuorumError, QuorumResult};

use std::collections::BTreeMap;

/// Identifier for a proposal.
pub type ProposalId = u64;

/// Identifier for a voter.
pub type VoterId = u64;

/// Governance contract holding proposals and the registered voting weight.
///
/// The registered weight map is the source of truth for the quorum
/// denominator. It is snapshotted into each proposal at creation time and is
/// never consulted live during tallying.
#[derive(Debug, Default)]
pub struct Governance {
    next_proposal_id: ProposalId,
    proposals: BTreeMap<ProposalId, Proposal>,
    /// Total registered voting weight per voter.
    registered_weight: BTreeMap<VoterId, u128>,
}

impl Governance {
    /// Create an empty governance contract.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the voting weight for a voter.
    ///
    /// This only affects proposals created afterwards; existing proposals keep
    /// the total eligible weight snapshotted when they were created.
    pub fn register_weight(&mut self, voter: VoterId, weight: u128) {
        self.registered_weight.insert(voter, weight);
    }

    /// Total eligible voting weight currently registered.
    pub fn total_eligible_weight(&self) -> u128 {
        self.registered_weight.values().copied().sum()
    }

    /// Create a proposal, snapshotting the total eligible weight at this moment.
    pub fn create_proposal(&mut self, quorum_bps: u16) -> ProposalId {
        let id = self.next_proposal_id;
        self.next_proposal_id += 1;
        let proposal = Proposal::new(id, self.total_eligible_weight(), quorum_bps);
        self.proposals.insert(id, proposal);
        id
    }

    /// Borrow a proposal by id.
    pub fn proposal(&self, id: ProposalId) -> Option<&Proposal> {
        self.proposals.get(&id)
    }

    /// Mutably borrow a proposal by id.
    pub fn proposal_mut(&mut self, id: ProposalId) -> Option<&mut Proposal> {
        self.proposals.get_mut(&id)
    }

    /// Cast a vote of `weight` for a proposal.
    ///
    /// The weight is recorded against the proposal's tally. Quorum is evaluated
    /// against the proposal's snapshotted total eligible weight, not the weight
    /// cast here.
    pub fn vote(
        &mut self,
        id: ProposalId,
        voter: VoterId,
        weight: u128,
    ) -> Result<(), ProposalError> {
        let proposal = self
            .proposals
            .get_mut(&id)
            .ok_or(ProposalError::UnknownProposal)?;
        proposal.cast_vote(voter, weight)
    }

    /// Evaluate whether a proposal has met quorum.
    pub fn check_quorum(&self, id: ProposalId) -> Result<QuorumResult, QuorumError> {
        let proposal = self
            .proposals
            .get(&id)
            .ok_or(QuorumError::UnknownProposal)?;
        quorum::evaluate(proposal)
    }
}
