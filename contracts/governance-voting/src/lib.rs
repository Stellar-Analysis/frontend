//! Weighted vote tallying contract with delegation double-count prevention and
//! immutable deadline finalization.

pub mod delegation;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub use delegation::{DelegationError, DelegationRegistry};

/// Direction of a ballot cast in `governance-voting`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallotDirection {
    For,
    Against,
    Abstain,
}

/// Errors returned by [`GovernanceVotingContract`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VotingError {
    Delegation(DelegationError),
    BallotRoundNotFound(u64),
    RoundAlreadyFinalized(u64),
    DeadlinePassed {
        round_id: u64,
        deadline: u64,
        now: u64,
    },
    DeadlineNotReached {
        round_id: u64,
        deadline: u64,
        now: u64,
    },
    /// The caller delegated their voting weight to `delegate` and therefore
    /// cannot also cast a direct ballot with that weight.
    WeightDelegatedTo {
        voter: String,
        delegate: String,
    },
    /// The caller has already cast a ballot in this voting round.
    AlreadyVoted {
        voter: String,
    },
    /// One of the underlying principals represented by this ballot has already
    /// had its weight counted in this round.
    UnderlyingWeightAlreadyCounted {
        principal: String,
    },
    /// The caller has zero effective voting weight in the round snapshot.
    ZeroVotingWeight {
        voter: String,
    },
}

impl fmt::Display for VotingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VotingError::Delegation(e) => write!(f, "delegation error: {e}"),
            VotingError::BallotRoundNotFound(id) => write!(f, "voting round {id} not found"),
            VotingError::RoundAlreadyFinalized(id) => {
                write!(f, "voting round {id} is already finalized")
            }
            VotingError::DeadlinePassed {
                round_id,
                deadline,
                now,
            } => write!(
                f,
                "voting round {round_id} deadline {deadline} has passed (now={now})"
            ),
            VotingError::DeadlineNotReached {
                round_id,
                deadline,
                now,
            } => write!(
                f,
                "voting round {round_id} cannot finalize before deadline {deadline} (now={now})"
            ),
            VotingError::WeightDelegatedTo { voter, delegate } => write!(
                f,
                "voter '{voter}' delegated weight to '{delegate}' and cannot also vote directly"
            ),
            VotingError::AlreadyVoted { voter } => {
                write!(f, "voter '{voter}' has already voted in this round")
            }
            VotingError::UnderlyingWeightAlreadyCounted { principal } => write!(
                f,
                "underlying voting weight for '{principal}' was already counted in this round"
            ),
            VotingError::ZeroVotingWeight { voter } => {
                write!(f, "voter '{voter}' has zero effective voting weight")
            }
        }
    }
}

impl std::error::Error for VotingError {}

impl From<DelegationError> for VotingError {
    fn from(err: DelegationError) -> Self {
        VotingError::Delegation(err)
    }
}

/// Snapshot and tally state for a single proposal voting round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VotingRound {
    pub round_id: u64,
    pub deadline: u64,
    /// Snapshot of base weights and delegations frozen when the round opened.
    pub snapshot: DelegationRegistry,
    pub snapshotted_total_eligible_weight: u128,
    pub for_weight: u128,
    pub against_weight: u128,
    pub abstain_weight: u128,
    /// Accounts that have submitted a ballot transaction in this round.
    pub ballot_cast_by: BTreeSet<String>,
    /// Underlying principals whose base weight has already been counted in this round.
    pub consumed_principals: BTreeSet<String>,
    pub finalized: bool,
}

impl VotingRound {
    /// Total participating weight counted so far.
    pub const fn total_votes_cast(&self) -> u128 {
        self.for_weight
            .saturating_add(self.against_weight)
            .saturating_add(self.abstain_weight)
    }
}

/// Contract coordinating voter weight registration, delegation, and per-proposal
/// ballot tallying where each unit of weight is counted at most once.
#[derive(Debug, Default, Clone)]
pub struct GovernanceVotingContract {
    registry: DelegationRegistry,
    rounds: BTreeMap<u64, VotingRound>,
    next_round_id: u64,
}

impl GovernanceVotingContract {
    pub fn new() -> Self {
        Self {
            registry: DelegationRegistry::new(),
            rounds: BTreeMap::new(),
            next_round_id: 1,
        }
    }

    /// Register or update `account`'s base voting weight for future voting rounds.
    pub fn register_weight(&mut self, account: impl Into<String>, weight: u128) {
        self.registry.set_base_weight(account, weight);
    }

    /// Delegate `delegator`'s voting weight to `delegate` for future voting rounds.
    pub fn delegate(
        &mut self,
        delegator: impl Into<String>,
        delegate: impl Into<String>,
    ) -> Result<(), VotingError> {
        self.registry.delegate(delegator, delegate)?;
        Ok(())
    }

    /// Revoke active delegation for `delegator` for future voting rounds.
    pub fn undelegate(&mut self, delegator: &str) -> bool {
        self.registry.undelegate(delegator)
    }

    /// Inspect the live delegation registry.
    pub const fn registry(&self) -> &DelegationRegistry {
        &self.registry
    }

    /// Open a new voting round, freezing the current [`DelegationRegistry`] and
    /// total eligible weight snapshot.
    pub fn open_round(&mut self, deadline: u64) -> u64 {
        let round_id = self.next_round_id;
        self.next_round_id = self.next_round_id.saturating_add(1);
        let snapshotted_total_eligible_weight = self.registry.total_eligible_weight();
        let round = VotingRound {
            round_id,
            deadline,
            snapshot: self.registry.clone(),
            snapshotted_total_eligible_weight,
            for_weight: 0,
            against_weight: 0,
            abstain_weight: 0,
            ballot_cast_by: BTreeSet::new(),
            consumed_principals: BTreeSet::new(),
            finalized: false,
        };
        self.rounds.insert(round_id, round);
        round_id
    }

    /// Cast a weighted ballot in `round_id`.
    ///
    /// Enforces:
    /// 1. Round is open and not finalized (`now < deadline`).
    /// 2. If `voter` delegated their weight in the round snapshot, rejects with
    ///    [`VotingError::WeightDelegatedTo`].
    /// 3. `voter` can vote at most once, and every underlying principal whose
    ///    weight is represented by `voter` is marked in `consumed_principals`
    ///    so no unit of weight can ever be counted twice.
    pub fn cast_ballot(
        &mut self,
        round_id: u64,
        voter: &str,
        direction: BallotDirection,
        now: u64,
    ) -> Result<u128, VotingError> {
        let round = self
            .rounds
            .get_mut(&round_id)
            .ok_or(VotingError::BallotRoundNotFound(round_id))?;

        if round.finalized {
            return Err(VotingError::RoundAlreadyFinalized(round_id));
        }
        if now >= round.deadline {
            return Err(VotingError::DeadlinePassed {
                round_id,
                deadline: round.deadline,
                now,
            });
        }
        if round.ballot_cast_by.contains(voter) {
            return Err(VotingError::AlreadyVoted {
                voter: voter.to_string(),
            });
        }
        if let Some(delegate) = round.snapshot.delegate_of(voter) {
            return Err(VotingError::WeightDelegatedTo {
                voter: voter.to_string(),
                delegate: delegate.to_string(),
            });
        }

        let principals = round.snapshot.represented_principals(voter);
        let weight = round.snapshot.effective_weight(voter);
        if weight == 0 || principals.is_empty() {
            return Err(VotingError::ZeroVotingWeight {
                voter: voter.to_string(),
            });
        }

        for (principal, _) in &principals {
            if round.consumed_principals.contains(principal) {
                return Err(VotingError::UnderlyingWeightAlreadyCounted {
                    principal: principal.clone(),
                });
            }
        }

        round.ballot_cast_by.insert(voter.to_string());
        for (principal, _) in principals {
            round.consumed_principals.insert(principal);
        }

        match direction {
            BallotDirection::For => round.for_weight = round.for_weight.saturating_add(weight),
            BallotDirection::Against => {
                round.against_weight = round.against_weight.saturating_add(weight);
            }
            BallotDirection::Abstain => {
                round.abstain_weight = round.abstain_weight.saturating_add(weight);
            }
        }

        Ok(weight)
    }

    /// Finalize a voting round after `deadline`. Once finalized, tallies are immutable.
    pub fn finalize_round(&mut self, round_id: u64, now: u64) -> Result<&VotingRound, VotingError> {
        let round = self
            .rounds
            .get_mut(&round_id)
            .ok_or(VotingError::BallotRoundNotFound(round_id))?;

        if round.finalized {
            return Err(VotingError::RoundAlreadyFinalized(round_id));
        }
        if now < round.deadline {
            return Err(VotingError::DeadlineNotReached {
                round_id,
                deadline: round.deadline,
                now,
            });
        }

        round.finalized = true;
        Ok(round)
    }

    /// Inspect a voting round by ID.
    pub fn round(&self, round_id: u64) -> Option<&VotingRound> {
        self.rounds.get(&round_id)
    }
}
