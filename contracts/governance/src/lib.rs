//! Governance contract with quorum measured strictly against snapshotted total
//! eligible weight.

pub mod proposal;
pub mod quorum;

use std::collections::BTreeMap;
use std::fmt;

pub use proposal::{Proposal, ProposalState, VoteChoice};
pub use quorum::{
    evaluate_quorum, required_quorum_weight, QuorumConfig, QuorumEvaluation, BPS_DENOMINATOR,
};

/// Errors returned by [`GovernanceContract`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernanceError {
    InvalidQuorumConfig,
    ZeroTotalEligibleWeight,
    InvalidVotingWindow,
    ProposalNotFound(u64),
    VotingClosed {
        proposal_id: u64,
        voting_ends_at: u64,
        now: u64,
    },
    VotingStillOpen {
        proposal_id: u64,
        voting_ends_at: u64,
        now: u64,
    },
    NoSnapshottedWeight {
        proposal_id: u64,
        voter: String,
    },
    AlreadyVoted {
        proposal_id: u64,
        voter: String,
    },
    ProposalNotExecutable {
        proposal_id: u64,
        state: ProposalState,
    },
}

impl fmt::Display for GovernanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GovernanceError::InvalidQuorumConfig => write!(f, "invalid quorum configuration"),
            GovernanceError::ZeroTotalEligibleWeight => {
                write!(f, "cannot create proposal with zero total eligible weight")
            }
            GovernanceError::InvalidVotingWindow => {
                write!(f, "voting_ends_at must be strictly greater than created_at")
            }
            GovernanceError::ProposalNotFound(id) => write!(f, "proposal {id} not found"),
            GovernanceError::VotingClosed {
                proposal_id,
                voting_ends_at,
                now,
            } => write!(
                f,
                "proposal {proposal_id} voting closed at {voting_ends_at} (now={now})"
            ),
            GovernanceError::VotingStillOpen {
                proposal_id,
                voting_ends_at,
                now,
            } => write!(
                f,
                "proposal {proposal_id} voting still open until {voting_ends_at} (now={now})"
            ),
            GovernanceError::NoSnapshottedWeight { proposal_id, voter } => write!(
                f,
                "voter '{voter}' had no eligible weight when proposal {proposal_id} was created"
            ),
            GovernanceError::AlreadyVoted { proposal_id, voter } => {
                write!(f, "voter '{voter}' already voted on proposal {proposal_id}")
            }
            GovernanceError::ProposalNotExecutable { proposal_id, state } => write!(
                f,
                "proposal {proposal_id} cannot be executed in state {state:?}"
            ),
        }
    }
}

impl std::error::Error for GovernanceError {}

/// On-chain governance contract managing voter weight registration, proposal
/// creation with eligible-weight snapshots, voting, finalization, and execution.
#[derive(Debug, Clone)]
pub struct GovernanceContract {
    config: QuorumConfig,
    eligible_weights: BTreeMap<String, u128>,
    proposals: BTreeMap<u64, Proposal>,
    next_proposal_id: u64,
}

impl GovernanceContract {
    /// Initialise a governance contract with `config`.
    pub fn new(config: QuorumConfig) -> Result<Self, GovernanceError> {
        if config.quorum_bps == 0
            || config.quorum_bps as u128 > BPS_DENOMINATOR
            || config.approval_bps == 0
            || config.approval_bps as u128 > BPS_DENOMINATOR
        {
            return Err(GovernanceError::InvalidQuorumConfig);
        }
        Ok(Self {
            config,
            eligible_weights: BTreeMap::new(),
            proposals: BTreeMap::new(),
            next_proposal_id: 1,
        })
    }

    /// Register or update a voter's eligible weight for *future* proposals.
    ///
    /// Existing proposals retain their immutable creation-time snapshot.
    pub fn set_voter_weight(&mut self, voter: impl Into<String>, weight: u128) {
        let voter = voter.into();
        if weight == 0 {
            self.eligible_weights.remove(&voter);
        } else {
            self.eligible_weights.insert(voter, weight);
        }
    }

    /// Current live total eligible weight across all registered voters.
    pub fn live_total_eligible_weight(&self) -> u128 {
        self.eligible_weights
            .values()
            .copied()
            .fold(0u128, u128::saturating_add)
    }

    /// Create a new proposal, snapshotting the current eligible voting weights
    /// and total eligible weight denominator.
    pub fn create_proposal(
        &mut self,
        proposer: impl Into<String>,
        description: impl Into<String>,
        created_at: u64,
        voting_ends_at: u64,
    ) -> Result<u64, GovernanceError> {
        if voting_ends_at <= created_at {
            return Err(GovernanceError::InvalidVotingWindow);
        }
        if self.live_total_eligible_weight() == 0 {
            return Err(GovernanceError::ZeroTotalEligibleWeight);
        }

        let id = self.next_proposal_id;
        self.next_proposal_id = self.next_proposal_id.saturating_add(1);
        let proposal = Proposal::new(
            id,
            proposer,
            description,
            created_at,
            voting_ends_at,
            &self.eligible_weights,
        );
        self.proposals.insert(id, proposal);
        Ok(id)
    }

    /// Cast a vote on `proposal_id` using the voter's creation-time snapshotted weight.
    pub fn cast_vote(
        &mut self,
        proposal_id: u64,
        voter: &str,
        choice: VoteChoice,
        now: u64,
    ) -> Result<u128, GovernanceError> {
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(proposal_id))?;

        if proposal.state != ProposalState::Active || now >= proposal.voting_ends_at {
            return Err(GovernanceError::VotingClosed {
                proposal_id,
                voting_ends_at: proposal.voting_ends_at,
                now,
            });
        }

        if proposal.voters.contains(voter) {
            return Err(GovernanceError::AlreadyVoted {
                proposal_id,
                voter: voter.to_string(),
            });
        }

        let weight = proposal
            .snapshotted_voter_weights
            .get(voter)
            .copied()
            .unwrap_or(0);
        if weight == 0 {
            return Err(GovernanceError::NoSnapshottedWeight {
                proposal_id,
                voter: voter.to_string(),
            });
        }

        proposal.voters.insert(voter.to_string());
        match choice {
            VoteChoice::For => proposal.for_weight = proposal.for_weight.saturating_add(weight),
            VoteChoice::Against => {
                proposal.against_weight = proposal.against_weight.saturating_add(weight);
            }
            VoteChoice::Abstain => {
                proposal.abstain_weight = proposal.abstain_weight.saturating_add(weight);
            }
        }

        Ok(weight)
    }

    /// Evaluate a proposal's current quorum and approval metrics.
    pub fn evaluate_proposal(&self, proposal_id: u64) -> Result<QuorumEvaluation, GovernanceError> {
        let proposal = self
            .proposals
            .get(&proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(proposal_id))?;
        Ok(proposal.evaluate(self.config))
    }

    /// Finalize a proposal after its voting deadline.
    pub fn finalize_proposal(
        &mut self,
        proposal_id: u64,
        now: u64,
    ) -> Result<(ProposalState, QuorumEvaluation), GovernanceError> {
        let config = self.config;
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(proposal_id))?;

        if now < proposal.voting_ends_at {
            return Err(GovernanceError::VotingStillOpen {
                proposal_id,
                voting_ends_at: proposal.voting_ends_at,
                now,
            });
        }

        let eval = proposal.evaluate(config);
        if proposal.state == ProposalState::Active {
            proposal.state = if !eval.quorum_met {
                ProposalState::DefeatedQuorumNotMet
            } else if !eval.approval_met {
                ProposalState::DefeatedRejected
            } else {
                ProposalState::Succeeded
            };
        }

        Ok((proposal.state, eval))
    }

    /// Execute a proposal that has been finalized as [`ProposalState::Succeeded`].
    pub fn execute_proposal(
        &mut self,
        proposal_id: u64,
        now: u64,
    ) -> Result<ProposalState, GovernanceError> {
        let (state, _) = self.finalize_proposal(proposal_id, now)?;
        let proposal = self
            .proposals
            .get_mut(&proposal_id)
            .ok_or(GovernanceError::ProposalNotFound(proposal_id))?;

        if state != ProposalState::Succeeded {
            return Err(GovernanceError::ProposalNotExecutable { proposal_id, state });
        }

        proposal.state = ProposalState::Executed;
        Ok(ProposalState::Executed)
    }

    /// Inspect a proposal by ID.
    pub fn proposal(&self, proposal_id: u64) -> Option<&Proposal> {
        self.proposals.get(&proposal_id)
    }
}
