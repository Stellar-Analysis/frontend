# `access-control` — Shared Role & Permission Source of Truth

`contracts/access-control` is the single source of truth for administrative roles, operator permissions, and emergency pause state across every contract in the Stellar Analysis suite.

## Mandatory Live-Authorization Rule for Dependent Contracts

Every dependent contract in this repository **must** enforce authorization by calling `access-control` live at the entry point of each privileged operation — **never** by caching a boolean role flag or copying role membership at initialization time:

| Dependent Crate | Privileged Role | Enforcement Rule |
| --- | --- | --- |
| `analytics` | `SNAPSHOT_SUBMITTER` | Must call `access_control.require_role(caller, Role::SnapshotSubmitter)` live before accepting any snapshot submission. |
| `escrow` | `ESCROW_ARBITER` | Must call `access_control.require_role(caller, Role::EscrowArbiter)` live before resolving disputes or releasing funds. |
| `governance` | `GOVERNANCE_EXECUTOR` | Must call `access_control.require_role(caller, Role::GovernanceExecutor)` live before queueing or executing proposals. |
| `governance-voting` | `VOTING_FINALIZER` | Must call `access_control.require_role(caller, Role::VotingFinalizer)` live before finalising vote tallies. |
| `multi-sig-wallet` | `MULTISIG_COSIGNER` | Must call `access_control.require_role(caller, Role::MultisigCosigner)` live before recording approvals or executing threshold transactions. |
| `time-locked-transactions` | `TIMELOCK_ADMIN` | Must call `access_control.require_role(caller, Role::TimelockAdmin)` live before scheduling or cancelling time-locked operations. |
| `token-swap` | `SWAP_OPERATOR` | Must call `access_control.require_role(caller, Role::SwapOperator)` live before updating pool parameters or pause state. |
| `upgrade` | `UPGRADE_MANAGER` | Must call `access_control.require_role(caller, Role::UpgradeManager)` live before staging or applying contract upgrades. |

### Epoch-Invalidated Caching Policy

If cross-contract call overhead ever requires memoizing authorization state within a bounded execution context, the dependent contract **must** use [`RevocationAwareCache`](./src/lib.rs), which binds every cached entry to `access-control`'s monotonic `revocation_epoch` and invalidates immediately upon receiving a [`RevocationEvent`](./src/grant_revoke.rs). A cached authorization entry must never outlive a role revocation.
