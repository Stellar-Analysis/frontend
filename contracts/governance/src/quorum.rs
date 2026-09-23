//! Quorum evaluation.
//!
//! The denominator is always the total eligible/registered voting weight
//! snapshotted at proposal-creation time. It is never the number of votes
//! actually cast, and it is never read live from current registration state.

use crate::proposal::{Proposal, ProposalStatus};

/// Errors returned when evaluating quorum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumError {
    /// No proposal exists for the given id.
    UnknownProposal,
}

/// Result of a quorum evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuorumResult {
    /// Weight cast in favour of the proposal.
    pub votes_cast: u128,
    /// Total eligible weight snapshotted at proposal-creation time.
    pub total_eligible_weight: u128,
    /// Weight required to meet quorum.
    pub required_weight: u128,
    /// Whether quorum was met.
    pub met: bool,
}

/// Compute the weight required to meet quorum.
///
/// `quorum_bps` is expressed in basis points (1 bps = 0.01%). The denominator
/// is the proposal's snapshotted total eligible weight.
fn required_weight(total_eligible_weight: u128, quorum_bps: u16) -> u128 {
    // Round up so that a threshold is never met by rounding down.
    let numerator = total_eligible_weight.saturating_mul(quorum_bps as u128);
    numerator.div_ceil(10_000)
}

/// Evaluate quorum for a proposal.
///
/// The denominator is `proposal.total_eligible_weight`, snapshotted at
/// proposal-creation time. Votes actually cast are only the numerator.
pub fn evaluate(proposal: &Proposal) -> Result<QuorumResult, QuorumError> {
    let total_eligible_weight = proposal.total_eligible_weight;
    let required = required_weight(total_eligible_weight, proposal.quorum_bps);
    let met = proposal.votes_cast >= required;
    Ok(QuorumResult {
        votes_cast: proposal.votes_cast,
        total_eligible_weight,
        required_weight: required,
        met,
    })
}

/// Finalize a proposal's status based on quorum.
///
/// The denominator remains the snapshotted total eligible weight.
pub fn finalize(proposal: &mut Proposal) -> Result<QuorumResult, QuorumError> {
    let result = evaluate(proposal)?;
    proposal.status = if result.met {
        ProposalStatus::QuorumMet
    } else {
        ProposalStatus::QuorumNotMet
    };
    Ok(result)
}
