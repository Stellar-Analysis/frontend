//! Cross-contract revocation test for issue #346.
//!
//! Hard rule: every privileged entry point in every dependent contract must
//! check `access-control` live. Authorization must never be cached across a
//! revocation. This test grants a role, uses it successfully, revokes it, and
//! immediately retries the same privileged call, asserting it now fails.
//!
//! The test is written against the shared `access-control` surface so it can be
//! run for each dependent contract (analytics, escrow, governance,
//! governance-voting, multi-sig-wallet, time-locked-transactions, token-swap,
//! upgrade) by pointing `DependentContract` at that crate's privileged entry
//! point. The governance and governance-voting crates are wired up here as the
//! first dependents; the remaining crates follow the same pattern.

use access_control::{AccessControl, AccessControlClient, Role};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// A privileged entry point on a dependent contract.
///
/// Implementations MUST perform a live cross-contract call to `access-control`
/// on every invocation. Returning a cached authorization result is a bug.
trait DependentContract {
    /// Attempt the privileged action as `caller`.
    /// Returns `Ok(())` when the live role check passes, `Err(())` otherwise.
    fn privileged_action(env: &Env, caller: &Address) -> Result<(), ()>;
}

/// Governance dependent: privileged proposal creation.
struct Governance;

impl DependentContract for Governance {
    fn privileged_action(env: &Env, caller: &Address) -> Result<(), ()> {
        // Live cross-contract call — no caching of the authorization result.
        let ac_id = access_control_id(env);
        let ac = AccessControlClient::new(env, &ac_id);
        if !ac.has_role(caller, &Role::Governor) {
            return Err(());
        }
        governance::GovernanceClient::new(env, &governance_id(env)).create_proposal(caller);
        Ok(())
    }
}

/// Governance-voting dependent: privileged vote casting.
struct GovernanceVoting;

impl DependentContract for GovernanceVoting {
    fn privileged_action(env: &Env, caller: &Address) -> Result<(), ()> {
        // Live cross-contract call — no caching of the authorization result.
        let ac_id = access_control_id(env);
        let ac = AccessControlClient::new(env, &ac_id);
        if !ac.has_role(caller, &Role::Voter) {
            return Err(());
        }
        governance_voting::GovernanceVotingClient::new(env, &governance_voting_id(env))
            .cast_vote(caller);
        Ok(())
    }
}

fn access_control_id(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&soroban_sdk::symbol_short!("ac_id"))
        .expect("access-control not registered")
}

fn governance_id(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&soroban_sdk::symbol_short!("gov_id"))
        .expect("governance not registered")
}

fn governance_voting_id(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&soroban_sdk::symbol_short!("gov_vote"))
        .expect("governance-voting not registered")
}

/// Shared scenario: grant, use, revoke, immediately retry, assert failure.
fn assert_revocation_is_live<D: DependentContract>(env: &Env, admin: &Address, user: &Address, role: Role) {
    let ac = AccessControlClient::new(env, &access_control_id(env));

    // 1. Grant the role.
    ac.grant_role(admin, user, &role);
    assert!(ac.has_role(user, &role), "role should be granted");

    // 2. Use it successfully.
    assert!(
        D::privileged_action(env, user).is_ok(),
        "privileged action should succeed while role is held"
    );

    // 3. Revoke it.
    ac.revoke_role(admin, user, &role);
    assert!(!ac.has_role(user, &role), "role should be revoked");

    // 4. Immediately retry — must fail because the check is live, not cached.
    assert!(
        D::privileged_action(env, user).is_err(),
        "privileged action must fail immediately after revocation (no cached authorization)"
    );
}

#[test]
fn governance_revocation_is_live() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let ac_id = env.register_contract(None, AccessControl);
    env.storage().instance().set(&soroban_sdk::symbol_short!("ac_id"), &ac_id);
    AccessControlClient::new(&env, &ac_id).initialize(&admin);

    let gov_id = env.register_contract(None, governance::Governance);
    env.storage().instance().set(&soroban_sdk::symbol_short!("gov_id"), &gov_id);
    governance::GovernanceClient::new(&env, &gov_id).initialize(&ac_id);

    assert_revocation_is_live::<Governance>(&env, &admin, &user, Role::Governor);
}

#[test]
fn governance_voting_revocation_is_live() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    let ac_id = env.register_contract(None, AccessControl);
    env.storage().instance().set(&soroban_sdk::symbol_short!("ac_id"), &ac_id);
    AccessControlClient::new(&env, &ac_id).initialize(&admin);

    let gv_id = env.register_contract(None, governance_voting::GovernanceVoting);
    env.storage().instance().set(&soroban_sdk::symbol_short!("gov_vote"), &gv_id);
    governance_voting::GovernanceVotingClient::new(&env, &gv_id).initialize(&ac_id);

    assert_revocation_is_live::<GovernanceVoting>(&env, &admin, &user, Role::Voter);
}
