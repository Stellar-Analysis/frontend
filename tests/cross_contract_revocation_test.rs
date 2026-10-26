//! Cross-contract role revocation integration suite covering every dependent
//! contract (`analytics`, `escrow`, `governance`, `governance-voting`,
//! `multi-sig-wallet`, `time-locked-transactions`, `token-swap`, `upgrade`).
//!
//! For each dependent contract:
//! 1. Grant its required role in `access-control`.
//! 2. Invoke its privileged entry point and assert success.
//! 3. Revoke the role in `access-control`.
//! 4. Immediately retry the privileged entry point and assert `Unauthorized`.

#[path = "../contracts/access-control/src/lib.rs"]
mod access_control;

use access_control::{
    all_dependent_contracts, AccessControlContract, AccessControlError, DependentContract,
    PrivilegedDependentContract, RevocationAwareCache, Role,
};

#[test]
fn revoke_then_immediately_retry_fails_in_every_dependent_contract() {
    let admin = "GADMIN00000000000000000000000000000000000000000000000001";
    let operator = "GOPER000000000000000000000000000000000000000000000000002";

    let mut ac = AccessControlContract::new(admin).expect("valid admin");
    let mut dependents = all_dependent_contracts();
    assert_eq!(dependents.len(), DependentContract::ALL.len());

    for dependent in &mut dependents {
        let contract_kind = dependent.contract_id();
        let required_role = contract_kind.required_role();

        // 0. Before granting the role, a privileged call must fail with Unauthorized.
        let pre_grant = dependent.execute_privileged(&ac, operator, "initial-attempt");
        assert_eq!(
            pre_grant,
            Err(AccessControlError::Unauthorized {
                caller: operator.to_string(),
                required_role,
            }),
            "dependent contract {contract_kind} must reject ungranted role {required_role}"
        );

        // 1. Grant the role in access-control (and verify idempotency on repeat grant).
        assert_eq!(ac.grant_role(admin, operator, required_role), Ok(true));
        assert_eq!(ac.grant_role(admin, operator, required_role), Ok(false));
        assert!(ac.has_role(operator, required_role));

        // 2. Use the role in the dependent contract -> must succeed.
        let receipt = dependent
            .execute_privileged(&ac, operator, "authorized-op-1")
            .expect("privileged operation must succeed while role is granted");
        assert_eq!(receipt.contract, contract_kind);
        assert_eq!(receipt.proof.role, required_role);
        assert_eq!(dependent.committed_operations(), 1);

        // 3. Revoke the role in access-control (and verify idempotency on repeat revoke).
        let epoch_before = ac.revocation_epoch();
        let event = ac
            .revoke_role(admin, operator, required_role)
            .expect("admin can revoke")
            .expect("role was active and must emit RevocationEvent");
        assert_eq!(event.revocation_epoch, epoch_before + 1);
        assert_eq!(event.principal, operator);
        assert_eq!(event.role, required_role);
        assert_eq!(ac.revoke_role(admin, operator, required_role), Ok(None));
        assert!(!ac.has_role(operator, required_role));

        // 4. Immediately retry in the dependent contract -> must fail with Unauthorized!
        let post_revoke = dependent.execute_privileged(&ac, operator, "post-revoke-retry");
        assert_eq!(
            post_revoke,
            Err(AccessControlError::Unauthorized {
                caller: operator.to_string(),
                required_role,
            }),
            "dependent contract {contract_kind} must not cache authorization after revocation"
        );
        assert_eq!(
            dependent.committed_operations(),
            1,
            "failed retry after revocation must not mutate state in {contract_kind}"
        );
    }

    assert_eq!(ac.revocation_events().len(), DependentContract::ALL.len());
}

#[test]
fn revocation_aware_cache_cannot_outlive_role_revocation() {
    let admin = "GADMIN00000000000000000000000000000000000000000000000001";
    let submitter = "GSUBMIT0000000000000000000000000000000000000000000000003";

    let mut ac = AccessControlContract::new(admin).expect("valid admin");
    let mut cache = RevocationAwareCache::new();

    ac.grant_role(admin, submitter, Role::SnapshotSubmitter)
        .expect("grant SnapshotSubmitter");

    // Populate cache via live check.
    let proof = cache
        .check_or_revalidate(&ac, submitter, Role::SnapshotSubmitter)
        .expect("live check populates cache");
    assert_eq!(proof.revocation_epoch, 0);

    // Revoke the role and deliver the revocation event to the cache.
    let event = ac
        .revoke_role(admin, submitter, Role::SnapshotSubmitter)
        .expect("revoke succeeds")
        .expect("revocation event emitted");
    cache.on_revocation_event(&event);

    // Immediately retrying through the cache must fail.
    let retry = cache.check_or_revalidate(&ac, submitter, Role::SnapshotSubmitter);
    assert_eq!(
        retry,
        Err(AccessControlError::Unauthorized {
            caller: submitter.to_string(),
            required_role: Role::SnapshotSubmitter,
        })
    );
}

#[test]
fn pause_and_last_admin_invariants_are_enforced() {
    let admin = "GADMIN00000000000000000000000000000000000000000000000001";
    let attacker = "GATTACK0000000000000000000000000000000000000000000000009";

    let mut ac = AccessControlContract::new(admin).expect("valid admin");

    // Non-admin cannot grant or revoke roles.
    assert_eq!(
        ac.grant_role(attacker, attacker, Role::Admin),
        Err(AccessControlError::Unauthorized {
            caller: attacker.to_string(),
            required_role: Role::Admin,
        })
    );

    // Last remaining admin cannot revoke their own ADMIN role.
    assert_eq!(
        ac.revoke_role(admin, admin, Role::Admin),
        Err(AccessControlError::CannotRevokeLastAdmin)
    );

    // Pausing blocks privileged operations across dependents even for role holders.
    ac.grant_role(admin, attacker, Role::EscrowArbiter)
        .expect("grant EscrowArbiter");
    ac.set_paused(admin, true).expect("admin pauses contract");
    assert!(ac.is_paused());
    assert_eq!(
        ac.require_role(attacker, Role::EscrowArbiter),
        Err(AccessControlError::Paused)
    );
    assert!(!AccessControlError::Paused.to_string().is_empty());
}
