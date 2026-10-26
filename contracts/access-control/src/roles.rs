//! Canonical roles and dependent contract permissions for `access-control`.
//!
//! Every dependent contract (`analytics`, `escrow`, `governance`,
//! `governance-voting`, `multi-sig-wallet`, `time-locked-transactions`,
//! `token-swap`, `upgrade`) maps its privileged entry points to a [`Role`]
//! managed exclusively by the central `access-control` contract.

use std::fmt;

/// Roles enforced across the Stellar Analysis smart-contract suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// Super-administrator allowed to grant/revoke roles and toggle pause state.
    Admin,
    /// Authorised to submit ledger analytics snapshots in `analytics`.
    SnapshotSubmitter,
    /// Authorised to resolve disputes and release funds in `escrow`.
    EscrowArbiter,
    /// Authorised to queue and execute passed proposals in `governance`.
    GovernanceExecutor,
    /// Authorised to finalise tallies and register talliers in `governance-voting`.
    VotingFinalizer,
    /// Authorised cosigner and threshold operator in `multi-sig-wallet`.
    MultisigCosigner,
    /// Authorised to schedule or cancel operations in `time-locked-transactions`.
    TimelockAdmin,
    /// Authorised to configure pools and pause swaps in `token-swap`.
    SwapOperator,
    /// Authorised to stage and apply WASM upgrades in `upgrade`.
    UpgradeManager,
}

impl Role {
    /// Stable identifier used in contract events and storage keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "ADMIN",
            Role::SnapshotSubmitter => "SNAPSHOT_SUBMITTER",
            Role::EscrowArbiter => "ESCROW_ARBITER",
            Role::GovernanceExecutor => "GOVERNANCE_EXECUTOR",
            Role::VotingFinalizer => "VOTING_FINALIZER",
            Role::MultisigCosigner => "MULTISIG_COSIGNER",
            Role::TimelockAdmin => "TIMELOCK_ADMIN",
            Role::SwapOperator => "SWAP_OPERATOR",
            Role::UpgradeManager => "UPGRADE_MANAGER",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every dependent contract in the repository whose privileged entry points
/// rely on `access-control` as the single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DependentContract {
    Analytics,
    Escrow,
    Governance,
    GovernanceVoting,
    MultiSigWallet,
    TimeLockedTransactions,
    TokenSwap,
    Upgrade,
}

impl DependentContract {
    /// Complete list of dependent contracts in the protocol.
    pub const ALL: [DependentContract; 8] = [
        DependentContract::Analytics,
        DependentContract::Escrow,
        DependentContract::Governance,
        DependentContract::GovernanceVoting,
        DependentContract::MultiSigWallet,
        DependentContract::TimeLockedTransactions,
        DependentContract::TokenSwap,
        DependentContract::Upgrade,
    ];

    /// Canonical crate name of the dependent contract.
    pub const fn crate_name(self) -> &'static str {
        match self {
            DependentContract::Analytics => "analytics",
            DependentContract::Escrow => "escrow",
            DependentContract::Governance => "governance",
            DependentContract::GovernanceVoting => "governance-voting",
            DependentContract::MultiSigWallet => "multi-sig-wallet",
            DependentContract::TimeLockedTransactions => "time-locked-transactions",
            DependentContract::TokenSwap => "token-swap",
            DependentContract::Upgrade => "upgrade",
        }
    }

    /// The role required to invoke privileged entry points on this contract.
    pub const fn required_role(self) -> Role {
        match self {
            DependentContract::Analytics => Role::SnapshotSubmitter,
            DependentContract::Escrow => Role::EscrowArbiter,
            DependentContract::Governance => Role::GovernanceExecutor,
            DependentContract::GovernanceVoting => Role::VotingFinalizer,
            DependentContract::MultiSigWallet => Role::MultisigCosigner,
            DependentContract::TimeLockedTransactions => Role::TimelockAdmin,
            DependentContract::TokenSwap => Role::SwapOperator,
            DependentContract::Upgrade => Role::UpgradeManager,
        }
    }
}

impl fmt::Display for DependentContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.crate_name())
    }
}
