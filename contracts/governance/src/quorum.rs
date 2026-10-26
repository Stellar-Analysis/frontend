//! Quorum and passing-threshold evaluation for on-chain governance.
//!
//! # Core Invariant
//!
//! Quorum is **always** measured against `snapshotted_total_eligible_weight`
//! captured immutably when a proposal is created — **never** against votes
//! actually cast (`for + against + abstain`) and **never** against live
//! post-proposal weight. Measuring quorum against votes cast would allow a
//! proposal with 3 votes in favor out of 10,000 registered voting weight
//! (100% of votes cast, 0.03% turnout) to pass.

/// Basis-points denominator (`10_000` = 100.00%).
pub const BPS_DENOMINATOR: u128 = 10_000;

/// Governance quorum and approval rules expressed in basis points (`1..=10_000`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuorumConfig {
    /// Minimum participating weight (`for + against + abstain`) as basis points
    /// of the proposal's snapshotted total eligible weight.
    pub quorum_bps: u32,
    /// Minimum `for_weight` as basis points of decisive weight (`for + against`).
    pub approval_bps: u32,
}

impl Default for QuorumConfig {
    /// Default governance rule: 20% (`2_000` bps) of total eligible weight must
    /// participate, and strictly more than 50% (or >= `5_001` bps) of decisive
    /// weight must vote in favor.
    fn default() -> Self {
        Self {
            quorum_bps: 2_000,
            approval_bps: 5_001,
        }
    }
}

/// Detailed result of evaluating a proposal's vote tallies against [`QuorumConfig`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuorumEvaluation {
    /// Immutable denominator: total eligible voting weight snapshotted at proposal creation.
    pub snapshotted_total_eligible_weight: u128,
    /// Total weight of ballots actually cast (`for_weight + against_weight + abstain_weight`).
    pub participating_weight: u128,
    /// Minimum participating weight required to satisfy `quorum_bps` against
    /// `snapshotted_total_eligible_weight` (rounded up).
    pub required_quorum_weight: u128,
    /// Whether `participating_weight >= required_quorum_weight`.
    pub quorum_met: bool,
    /// Whether `for_weight` meets `approval_bps` out of decisive votes (`for + against`).
    pub approval_met: bool,
    /// Whether both `quorum_met` and `approval_met` hold.
    pub passed: bool,
}

/// Compute the minimum participating weight required to meet `quorum_bps`
/// against `snapshotted_total_eligible_weight` using ceiling integer math.
pub const fn required_quorum_weight(
    snapshotted_total_eligible_weight: u128,
    quorum_bps: u32,
) -> u128 {
    if snapshotted_total_eligible_weight == 0 || quorum_bps == 0 {
        return 0;
    }
    let numerator = snapshotted_total_eligible_weight.saturating_mul(quorum_bps as u128);
    numerator.div_ceil(BPS_DENOMINATOR)
}

/// Evaluate quorum and approval against the **snapshotted total eligible weight**.
///
/// Crucially, the quorum denominator is `snapshotted_total_eligible_weight`,
/// never `participating_weight`.
pub const fn evaluate_quorum(
    config: QuorumConfig,
    snapshotted_total_eligible_weight: u128,
    for_weight: u128,
    against_weight: u128,
    abstain_weight: u128,
) -> QuorumEvaluation {
    let participating_weight = for_weight
        .saturating_add(against_weight)
        .saturating_add(abstain_weight);
    let required = required_quorum_weight(snapshotted_total_eligible_weight, config.quorum_bps);

    let quorum_met = snapshotted_total_eligible_weight > 0
        && config.quorum_bps > 0
        && participating_weight >= required;

    let decisive_weight = for_weight.saturating_add(against_weight);
    let approval_met = if decisive_weight == 0 {
        false
    } else {
        for_weight.saturating_mul(BPS_DENOMINATOR)
            >= decisive_weight.saturating_mul(config.approval_bps as u128)
    };

    QuorumEvaluation {
        snapshotted_total_eligible_weight,
        participating_weight,
        required_quorum_weight: required,
        quorum_met,
        approval_met,
        passed: quorum_met && approval_met,
    }
}
