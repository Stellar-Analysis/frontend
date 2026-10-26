//! Delegation double-count tests proving that each unit of voting weight is
//! counted at most once when both a delegator and delegate attempt to vote or
//! change delegations across a voting round.

#[path = "../contracts/governance-voting/src/lib.rs"]
mod governance_voting;

use governance_voting::{BallotDirection, DelegationError, GovernanceVotingContract, VotingError};

#[test]
fn delegator_and_delegate_cannot_both_vote_same_weight() {
    let mut voting = GovernanceVotingContract::new();
    voting.register_weight("alice", 400);
    voting.register_weight("bob", 600);
    voting.register_weight("carol", 1_000);

    // Alice delegates her 400 weight to Bob.
    voting
        .delegate("alice", "bob")
        .expect("alice delegates to bob");

    // Verify weight conservation in the registry:
    // Alice effective = 0, Bob effective = 1,000 (600 own + 400 delegated), Carol = 1,000.
    assert_eq!(voting.registry().base_weight("alice"), 400);
    assert_eq!(voting.registry().effective_weight("alice"), 0);
    assert_eq!(voting.registry().effective_weight("bob"), 1_000);
    assert_eq!(voting.registry().total_eligible_weight(), 2_000);

    let round_id = voting.open_round(5_000);

    // 1. Alice attempts to vote directly even though her weight is delegated to Bob -> rejected!
    assert_eq!(
        voting.cast_ballot(round_id, "alice", BallotDirection::For, 1_000),
        Err(VotingError::WeightDelegatedTo {
            voter: "alice".to_string(),
            delegate: "bob".to_string(),
        })
    );

    // 2. Bob casts his ballot -> exercises combined 1,000 weight (his 600 + Alice's 400) once.
    let counted = voting
        .cast_ballot(round_id, "bob", BallotDirection::For, 1_100)
        .expect("bob casts combined ballot");
    assert_eq!(counted, 1_000);
    assert_eq!(
        voting
            .cast_ballot(round_id, "carol", BallotDirection::Abstain, 1_150)
            .expect("carol casts abstain ballot"),
        1_000
    );
    assert!(voting.round(round_id).is_some());

    // 3. Bob cannot vote a second time, and Alice still cannot vote even if she undelegates live!
    assert_eq!(
        voting.cast_ballot(round_id, "bob", BallotDirection::For, 1_200),
        Err(VotingError::AlreadyVoted {
            voter: "bob".to_string(),
        })
    );
    assert!(voting.undelegate("alice"));
    assert_eq!(
        voting.cast_ballot(round_id, "alice", BallotDirection::Against, 1_300),
        Err(VotingError::WeightDelegatedTo {
            voter: "alice".to_string(),
            delegate: "bob".to_string(),
        })
    );

    let round = voting
        .finalize_round(round_id, 5_000)
        .expect("finalize after deadline");
    assert!(round.finalized);
    assert_eq!(round.for_weight, 1_000);
    assert_eq!(round.against_weight, 0);
    assert_eq!(round.abstain_weight, 1_000);
    assert_eq!(round.total_votes_cast(), 2_000);
    assert!(round.total_votes_cast() <= round.snapshotted_total_eligible_weight);
}

#[test]
fn transitive_and_cyclic_delegations_are_rejected_to_preserve_single_count_invariant() {
    let mut voting = GovernanceVotingContract::new();
    voting.register_weight("alice", 300);
    voting.register_weight("bob", 500);
    voting.register_weight("carol", 700);

    // Self-delegation is rejected.
    assert_eq!(
        voting.delegate("alice", "alice"),
        Err(VotingError::Delegation(DelegationError::SelfDelegation {
            account: "alice".to_string(),
        }))
    );

    // Alice -> Bob succeeds.
    voting
        .delegate("alice", "bob")
        .expect("alice delegates to bob");

    // Bob now holds incoming delegation from Alice, so Bob cannot delegate out to Carol
    // (nor back to Alice to form a cycle).
    assert_eq!(
        voting.delegate("bob", "carol"),
        Err(VotingError::Delegation(
            DelegationError::AccountHasIncomingDelegations {
                account: "bob".to_string(),
            }
        ))
    );
    assert_eq!(
        voting.delegate("bob", "alice"),
        Err(VotingError::Delegation(
            DelegationError::TargetIsDelegating {
                delegate: "alice".to_string(),
                upstream_delegate: "bob".to_string(),
            }
        ))
    );

    // Carol cannot delegate to Alice because Alice is already delegating to Bob.
    assert_eq!(
        voting.delegate("carol", "alice"),
        Err(VotingError::Delegation(
            DelegationError::TargetIsDelegating {
                delegate: "alice".to_string(),
                upstream_delegate: "bob".to_string(),
            }
        ))
    );

    // Sum of effective weights across all principals equals total eligible weight (1,500).
    let effective_sum = voting.registry().effective_weight("alice")
        + voting.registry().effective_weight("bob")
        + voting.registry().effective_weight("carol");
    assert_eq!(effective_sum, voting.registry().total_eligible_weight());
    assert_eq!(effective_sum, 1_500);
    assert!(!VotingError::BallotRoundNotFound(99).to_string().is_empty());
}
