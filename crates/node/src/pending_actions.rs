use crate::{
    DelegationId, MarketMakerProgramId, RedemptionRequestId, TeeVendor, VaultId, VendorRootId,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    BlockDelegationAction, BlockDelegationFaultReason, BlockMarketMakerProgramAction,
    BlockRealCapitalGovernanceAction, BlockTeeGovernanceAction, DelegationActionId, Hash,
    MarketMakerIdentity, MarketMakerProgramActionId, RealCapitalGovernanceActionId,
    TeeGovernanceActionId,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProcessedDelegationAction {
    pub processed_at_ms: u64,
    pub fingerprint: Hash,
    pub receipt: DelegationActionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum DelegationActionReceipt {
    Open {
        delegation_id: DelegationId,
    },
    Deposit {
        delegation_id: DelegationId,
        principal: u64,
        shares_minted: u128,
    },
    RequestRedemption {
        request_id: RedemptionRequestId,
    },
    FinalizeRedemption {
        request_id: RedemptionRequestId,
        usdc_out: u128,
        compensation_out: u128,
    },
    FinalizeRedemptionReady {
        request_id: RedemptionRequestId,
    },
    CancelRedemption {
        request_id: RedemptionRequestId,
    },
    CloseToDeposits {
        delegation_id: DelegationId,
    },
    BeginWinding {
        delegation_id: DelegationId,
    },
    InsurancePayout {
        delegation_id: DelegationId,
        requested_amount_micro_usdc: u128,
        distributed_amount_micro_usdc: u128,
        recipient_count: u64,
    },
    ForcedLiquidation {
        delegation_id: DelegationId,
        market_id: u64,
        keeper_account: u64,
        requested_lots: u64,
        filled_lots: u64,
        remaining_lots: u64,
        keeper_reward_micro_usdc: u128,
    },
    Close {
        delegation_id: DelegationId,
    },
    Terminate {
        delegation_id: DelegationId,
        reason: BlockDelegationFaultReason,
        bond_returned: u128,
        bond_slashed: u128,
        principal_compensation: u128,
        insurance_fund_delta: u128,
    },
    Rejected {
        action_kind: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingDelegationAction {
    pub action_id: DelegationActionId,
    pub action: BlockDelegationAction,
    pub fingerprint: Hash,
    pub submitted_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelegationActionSubmissionStatus {
    Accepted,
    DuplicatePending,
    AlreadyProcessed,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProcessedMarketMakerProgramAction {
    pub processed_at_ms: u64,
    pub fingerprint: Hash,
    pub receipt: MarketMakerProgramActionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MarketMakerProgramActionReceipt {
    Define {
        program_id: MarketMakerProgramId,
    },
    Retire {
        program_id: MarketMakerProgramId,
    },
    Enroll {
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: MarketMakerProgramId,
    },
    Exit {
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: MarketMakerProgramId,
    },
    Rejected {
        action_kind: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingMarketMakerProgramAction {
    pub action_id: MarketMakerProgramActionId,
    pub action: BlockMarketMakerProgramAction,
    pub fingerprint: Hash,
    pub submitted_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProcessedTeeGovernanceAction {
    pub processed_at_ms: u64,
    pub fingerprint: Hash,
    pub receipt: TeeGovernanceActionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TeeGovernanceActionReceipt {
    IntroduceVendorRoot {
        vendor: TeeVendor,
        root_id: VendorRootId,
        valid_from_block: u64,
    },
    RevokeVendorRoot {
        vendor: TeeVendor,
        root_id: VendorRootId,
        valid_until_block: u64,
    },
    UpdateVendorCapProfile {
        vendor: TeeVendor,
        cap_multiplier_microunits: u128,
        composition_eligibility: bool,
        valid_from_block: u64,
    },
    Rejected {
        action_kind: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingTeeGovernanceAction {
    pub action_id: TeeGovernanceActionId,
    pub action: BlockTeeGovernanceAction,
    pub fingerprint: Hash,
    pub submitted_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProcessedRealCapitalGovernanceAction {
    pub submitted_at_ms: u64,
    pub processed_at_ms: u64,
    pub fingerprint: Hash,
    pub receipt: RealCapitalGovernanceActionReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RealCapitalGovernanceActionReceipt {
    UpsertListing {
        vault_id: VaultId,
        delegation_id: DelegationId,
        serving_agent: u64,
    },
    SetListingDepositEnabled {
        vault_id: VaultId,
        deposit_enabled: bool,
    },
    SetListingDiscovery {
        vault_id: VaultId,
        listed_for_discovery: bool,
    },
    UpdateProtocolCap {
        total_protocol_tvl_micro_usdc: Option<u128>,
    },
    UpdateGlobalKillSwitch {
        disabled: bool,
    },
    Rejected {
        action_kind: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingRealCapitalGovernanceAction {
    pub action_id: RealCapitalGovernanceActionId,
    pub action: BlockRealCapitalGovernanceAction,
    pub fingerprint: Hash,
    pub submitted_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ScheduledTeeVendorCapProfileUpdate {
    pub governance_action: TeeGovernanceActionId,
    pub vendor: TeeVendor,
    pub cap_multiplier_microunits: u128,
    pub composition_eligibility: bool,
    pub valid_from_block: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketMakerProgramActionSubmissionStatus {
    Accepted,
    DuplicatePending,
    AlreadyProcessed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeeGovernanceActionSubmissionStatus {
    Accepted,
    DuplicatePending,
    AlreadyProcessed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealCapitalGovernanceActionSubmissionStatus {
    Accepted,
    DuplicatePending,
    AlreadyProcessed,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PendingFaucetGrant {
    pub address: [u8; 20],
    pub policy_target_micro_usdc: [u8; 32],
    pub evm_target_micro_usdc: [u8; 32],
    pub submitted_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaucetGrantSubmissionStatus {
    Accepted,
    DuplicatePending,
    AlreadyFunded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct PendingFaucetGrantOrder {
    pub(crate) submitted_at_ms: u64,
    pub(crate) address: [u8; 20],
}

pub(crate) fn pending_faucet_grant_order(pending: &PendingFaucetGrant) -> PendingFaucetGrantOrder {
    PendingFaucetGrantOrder {
        submitted_at_ms: pending.submitted_at_ms,
        address: pending.address,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct PendingMarketMakerProgramActionOrder {
    pub(crate) submitted_at_ms: u64,
    pub(crate) action_id: MarketMakerProgramActionId,
}

pub(crate) fn pending_market_maker_program_action_order(
    pending: &PendingMarketMakerProgramAction,
) -> PendingMarketMakerProgramActionOrder {
    PendingMarketMakerProgramActionOrder {
        submitted_at_ms: pending.submitted_at_ms,
        action_id: pending.action_id,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct PendingTeeGovernanceActionOrder {
    pub(crate) submitted_at_ms: u64,
    pub(crate) action_id: TeeGovernanceActionId,
}

pub(crate) fn pending_tee_governance_action_order(
    pending: &PendingTeeGovernanceAction,
) -> PendingTeeGovernanceActionOrder {
    PendingTeeGovernanceActionOrder {
        submitted_at_ms: pending.submitted_at_ms,
        action_id: pending.action_id,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct PendingRealCapitalGovernanceActionOrder {
    pub(crate) submitted_at_ms: u64,
    pub(crate) action_id: RealCapitalGovernanceActionId,
}

pub(crate) fn pending_real_capital_governance_action_order(
    pending: &PendingRealCapitalGovernanceAction,
) -> PendingRealCapitalGovernanceActionOrder {
    PendingRealCapitalGovernanceActionOrder {
        submitted_at_ms: pending.submitted_at_ms,
        action_id: pending.action_id,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct PendingDelegationActionOrder {
    pub(crate) submitted_at_ms: u64,
    pub(crate) action_id: DelegationActionId,
}

pub(crate) fn pending_delegation_action_order(
    pending: &PendingDelegationAction,
) -> PendingDelegationActionOrder {
    PendingDelegationActionOrder {
        submitted_at_ms: pending.submitted_at_ms,
        action_id: pending.action_id,
    }
}

pub(crate) fn remove_pending_delegation_index_order<K: Ord>(
    index: &mut BTreeMap<K, BTreeSet<PendingDelegationActionOrder>>,
    key: &K,
    order: &PendingDelegationActionOrder,
) {
    if let Some(orders) = index.get_mut(key) {
        orders.remove(order);
        if orders.is_empty() {
            index.remove(key);
        }
    }
}
