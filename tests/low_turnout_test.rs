//! Low-turnout quorum tests proving that quorum is measured against the
//! proposal-time snapshotted total eligible weight — never against votes cast
//! and never against live post-proposal weight.

#[path = "../contracts/governance/src/lib.rs"]
mod governance;

use governance::{
    evaluate_quorum, required_quorum_weight, GovernanceContract, GovernanceError, ProposalState,
    QuorumConfig, VoteChoice,
};

#[test]
fn three_votes_out_of_ten_thousand_eligible_weight_fails_quorum() {
    // 20% quorum (2,000 bps), >50% approval (5,001 bps).
    let config = QuorumConfig {
        quorum_bps: 2_000,
        approval_bps: 5_001,
    };
    let mut gov = GovernanceContract::new(config).expect("valid config");

    // Register 10,000 total eligible voting weight across the electorate.
    gov.set_voter_weight("whale_1", 4_997);
    gov.set_voter_weight("whale_2", 5_000);
    gov.set_voter_weight("small_voter_1", 1);
    gov.set_voter_weight("small_voter_2", 1);
    gov.set_voter_weight("small_voter_3", 1);
    assert_eq!(gov.live_total_eligible_weight(), 10_000);

    let proposal_id = gov
        .create_proposal("small_voter_1", "Upgrade parameter X", 1_000, 2_000)
        .expect("create proposal");

    // Only the 3 small voters participate, casting 3 votes out of 10,000 eligible
    // weight (100% of votes cast are FOR, but turnout is only 0.03%).
    assert_eq!(
        gov.cast_vote(proposal_id, "small_voter_1", VoteChoice::For, 1_100),
        Ok(1)
    );
    assert_eq!(
        gov.cast_vote(proposal_id, "small_voter_2", VoteChoice::For, 1_200),
        Ok(1)
    );
    assert_eq!(
        gov.cast_vote(proposal_id, "small_voter_3", VoteChoice::For, 1_300),
        Ok(1)
    );

    // Finalize after the voting deadline: even though 100% of votes cast were FOR,
    // quorum (2,000 weight required out of 10,000 eligible) was NOT met.
    let (state, eval) = gov
        .finalize_proposal(proposal_id, 2_000)
        .expect("finalize after deadline");
    assert_eq!(eval.snapshotted_total_eligible_weight, 10_000);
    assert_eq!(eval.participating_weight, 3);
    assert_eq!(eval.required_quorum_weight, 2_000);
    assert!(
        eval.approval_met,
        "100% of votes cast were in favor, so approval_met is true"
    );
    assert!(
        !eval.quorum_met,
        "3 out of 10,000 eligible weight must NOT meet 20% quorum"
    );
    assert!(!eval.passed);
    assert_eq!(state, ProposalState::DefeatedQuorumNotMet);

    // Attempting to execute the defeated proposal must fail.
    assert_eq!(
        gov.execute_proposal(proposal_id, 2_001),
        Err(GovernanceError::ProposalNotExecutable {
            proposal_id,
            state: ProposalState::DefeatedQuorumNotMet,
        })
    );
}

#[test]
fn quorum_denominator_is_snapshotted_at_proposal_creation_and_immune_to_late_weight_changes() {
    let mut gov = GovernanceContract::new(QuorumConfig::default()).expect("valid config");

    gov.set_voter_weight("alice", 1_500);
    gov.set_voter_weight("bob", 8_500);
    assert_eq!(gov.live_total_eligible_weight(), 10_000);

    let proposal_id = gov
        .create_proposal("alice", "Treasury allocation", 100, 500)
        .expect("create proposal");

    // After the proposal is created:
    // 1. `bob` exits (weight set to 0) to try to shrink the live denominator so
    //    `alice`'s 1,500 weight would exceed 20% of a smaller live denominator.
    // 2. `late_entrant` acquires 50,000 weight after seeing how the vote is trending.
    gov.set_voter_weight("bob", 0);
    gov.set_voter_weight("late_entrant", 50_000);

    // `late_entrant` had 0 weight at proposal creation and must be rejected.
    assert_eq!(
        gov.cast_vote(proposal_id, "late_entrant", VoteChoice::For, 200),
        Err(GovernanceError::NoSnapshottedWeight {
            proposal_id,
            voter: "late_entrant".to_string(),
        })
    );

    // `alice` votes FOR with her snapshotted 1,500 weight.
    assert_eq!(
        gov.cast_vote(proposal_id, "alice", VoteChoice::For, 250),
        Ok(1_500)
    );

    // Because the denominator was snapshotted at 10,000 (requiring 2,000 for 20% quorum),
    // `bob`'s post-creation exit does NOT lower the required quorum below 2,000.
    let (state, eval) = gov
        .finalize_proposal(proposal_id, 500)
        .expect("finalize after deadline");
    assert_eq!(eval.snapshotted_total_eligible_weight, 10_000);
    assert_eq!(eval.required_quorum_weight, 2_000);
    assert_eq!(eval.participating_weight, 1_500);
    assert!(!eval.quorum_met);
    assert_eq!(state, ProposalState::DefeatedQuorumNotMet);
}

#[test]
fn proposal_meeting_both_snapshotted_quorum_and_approval_succeeds_and_executes() {
    let mut gov = GovernanceContract::new(QuorumConfig::default()).expect("valid config");
    gov.set_voter_weight("voter_a", 1_200);
    gov.set_voter_weight("voter_b", 800);
    gov.set_voter_weight("voter_c", 8_000);

    let proposal_id = gov
        .create_proposal("voter_a", "Enable corridor analytics v2", 10, 100)
        .expect("create proposal");
    assert!(gov.proposal(proposal_id).is_some());

    // 1,200 FOR + 800 ABSTAIN = 2,000 participating weight (exact 20% quorum of 10,000),
    // and 1,200 / 1,200 decisive votes are FOR (100% approval).
    assert_eq!(required_quorum_weight(10_000, 2_000), 2_000);
    assert!(evaluate_quorum(QuorumConfig::default(), 10_000, 1_200, 0, 800).passed);
    gov.cast_vote(proposal_id, "voter_a", VoteChoice::For, 20)
        .expect("voter_a votes FOR");
    gov.cast_vote(proposal_id, "voter_b", VoteChoice::Abstain, 30)
        .expect("voter_b votes ABSTAIN");

    let eval = gov.evaluate_proposal(proposal_id).expect("evaluate");
    assert!(eval.quorum_met);
    assert!(eval.approval_met);
    assert!(eval.passed);

    let executed = gov
        .execute_proposal(proposal_id, 100)
        .expect("execute after deadline");
    assert_eq!(executed, ProposalState::Executed);

    // Also verify a proposal with sufficient quorum where AGAINST outweighs FOR is defeated.
    let rejected_id = gov
        .create_proposal("voter_a", "Rejected proposal", 200, 300)
        .expect("create proposal 2");
    gov.cast_vote(rejected_id, "voter_c", VoteChoice::Against, 210)
        .expect("voter_c votes AGAINST");
    let (rejected_state, _) = gov
        .finalize_proposal(rejected_id, 300)
        .expect("finalize rejected proposal");
    assert_eq!(rejected_state, ProposalState::DefeatedRejected);
    assert!(!GovernanceError::InvalidQuorumConfig.to_string().is_empty());
}
