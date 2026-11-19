use crate::{
    AgentIdentityTier, NodeError, AGENT_BOND_MIN, CRYSTALLIZATION_CADENCE_MAX_MS,
    CRYSTALLIZATION_CADENCE_MIN_MS, NOTICE_PERIOD_MAX_MS, NOTICE_PERIOD_MIN_MS,
    PERFORMANCE_SHARE_CAP_UNITS,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    delegation_block_action_kind, BlockDelegationAction, BlockDelegationFaultReason,
    BlockDelegationSlashPolicy, BlockIdentityTier, BlockVaultRealCapitalMode, DelegationActionId,
    Hash,
};
use kombat_chain_streams::{
    DelegationLifecycleStatus, RealCapitalInsuranceWalletSpendSource, RealCapitalListingMode,
};
use revm::primitives::{keccak256, U256};
use std::collections::{BTreeMap, BTreeSet};

// RFC 026 — Performance-Share Delegation.

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedDelegationAction {
    pub chain_id: u64,
    pub signer_account_id: u64,
    pub action_id: DelegationActionId,
    pub timestamp_ms: u64,
    pub submitter_nonce: Option<u64>,
    pub action: BlockDelegationAction,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, BorshSerialize)]
pub(crate) struct DelegationActionPayload {
    pub(crate) domain: String,
    pub(crate) chain_id: u64,
    pub(crate) signer_account_id: u64,
    pub(crate) action_id: DelegationActionId,
    pub(crate) action_kind: String,
    pub(crate) action_scope: Vec<u8>,
    pub(crate) timestamp_ms: u64,
    pub(crate) submitter_nonce: Option<u64>,
    pub(crate) body_hash: Hash,
}

pub type DelegationId = [u8; 32];
pub type VaultId = [u8; 16];
pub type RedemptionRequestId = [u8; 32];

pub fn vault_id_for_delegation(delegation_id: &DelegationId) -> VaultId {
    let mut buf = Vec::with_capacity(5 + 1 + 32);
    buf.extend_from_slice(b"vault");
    buf.push(0); // RFC 043 VaultKind::Delegation discriminator.
    buf.extend_from_slice(delegation_id);
    let hash = keccak256(&buf);
    let mut out = [0_u8; 16];
    out.copy_from_slice(&hash[..16]);
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum VaultRealCapitalMode {
    Disabled,
    CappedPilot,
    Public,
}

pub(crate) fn real_capital_mode_from_block(
    mode: BlockVaultRealCapitalMode,
) -> VaultRealCapitalMode {
    match mode {
        BlockVaultRealCapitalMode::Disabled => VaultRealCapitalMode::Disabled,
        BlockVaultRealCapitalMode::CappedPilot => VaultRealCapitalMode::CappedPilot,
        BlockVaultRealCapitalMode::Public => VaultRealCapitalMode::Public,
    }
}

pub(crate) fn real_capital_listing_mode(mode: VaultRealCapitalMode) -> RealCapitalListingMode {
    match mode {
        VaultRealCapitalMode::Disabled => RealCapitalListingMode::Disabled,
        VaultRealCapitalMode::CappedPilot => RealCapitalListingMode::CappedPilot,
        VaultRealCapitalMode::Public => RealCapitalListingMode::Public,
    }
}

pub(crate) fn agent_identity_tier_from_block(tier: BlockIdentityTier) -> AgentIdentityTier {
    match tier {
        BlockIdentityTier::Tier0Unattested => AgentIdentityTier::Tier0Unattested,
        BlockIdentityTier::Tier1OperatorAttested => AgentIdentityTier::Tier1OperatorAttested,
        BlockIdentityTier::Tier2TeeAttested => AgentIdentityTier::Tier2TeeAttested,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ListingCaps {
    pub per_principal_micro_usdc: u128,
    pub total_vault_tvl_micro_usdc: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RealCapitalListing {
    pub vault_id: VaultId,
    pub delegation_id: DelegationId,
    pub serving_agent: u64,
    pub mode: VaultRealCapitalMode,
    pub pilot_min_serving_agent_tier: Option<AgentIdentityTier>,
    pub tier0_pilot_exception: bool,
    pub deposit_enabled: bool,
    pub listed_for_discovery: bool,
    pub caps: ListingCaps,
    pub pilot_principal_allowlist: Option<BTreeSet<u64>>,
    pub listed_at_block: u64,
    pub last_updated_block: u64,
    pub last_updated_governance_action: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProtocolDelegationCap {
    pub total_protocol_tvl_micro_usdc: u128,
    pub last_updated_governance_action: Hash,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ListingControlState {
    pub global_deposit_kill_switch: bool,
    pub last_updated_block: u64,
    pub last_updated_governance_action: Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DelegationDepositCapAccounting {
    pub(crate) vault_id: VaultId,
    pub(crate) principal: u64,
    pub(crate) principal_deposited_micro_usdc: u128,
    pub(crate) vault_deposited_micro_usdc: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DelegationRedemptionCapAccounting {
    pub(crate) vault_id: VaultId,
    pub(crate) principal: u64,
    pub(crate) principal_deposited_micro_usdc: u128,
    pub(crate) vault_deposited_micro_usdc: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DelegationSlashDistribution {
    pub(crate) bond_returned: u128,
    pub(crate) bond_slashed: u128,
    pub(crate) principal_compensation: u128,
    pub(crate) insurance_fund_delta: u128,
    pub(crate) principal_allocations: Vec<(u64, u128)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum DelegationStatus {
    Open,
    ClosedToDeposits,
    Winding,
    Closed,
    Terminated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RedemptionRequestStatus {
    Pending,
    Ready,
    Finalized,
    Cancelled,
}

pub(crate) fn delegation_lifecycle_status(status: DelegationStatus) -> DelegationLifecycleStatus {
    match status {
        DelegationStatus::Open => DelegationLifecycleStatus::Open,
        DelegationStatus::ClosedToDeposits => DelegationLifecycleStatus::ClosedToDeposits,
        DelegationStatus::Winding => DelegationLifecycleStatus::Winding,
        DelegationStatus::Closed => DelegationLifecycleStatus::Closed,
        DelegationStatus::Terminated => DelegationLifecycleStatus::Terminated,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Delegation {
    pub delegation_id: DelegationId,
    pub agent: u64,
    pub mandate_id: [u8; 32],
    pub opened_at_ms: u64,
    pub last_updated_at_ms: u64,
    pub winding_started_at_ms: Option<u64>,
    pub status: DelegationStatus,
    pub performance_share_units: u16,
    pub crystallization_cadence_ms: u64,
    pub notice_period_ms: u64,
    pub total_shares: u128,
    pub high_water_mark_per_share: u128,
    pub last_crystallized_at_ms: u64,
    pub agent_bond: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationDeposit {
    pub delegation: Delegation,
    pub principal: u64,
    pub share: DelegationShare,
    pub shares_minted: u128,
    pub principal_balance: u128,
    pub delegation_balance: u128,
    pub trading_account: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelegationRedemptionRequest {
    pub request: RedemptionRequest,
    pub share: DelegationShare,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRedemptionFinalization {
    pub request: RedemptionRequest,
    pub delegation: Delegation,
    pub share: DelegationShare,
    pub usdc_out: u128,
    pub compensation_out: u128,
    pub principal_balance: u128,
    pub delegation_balance: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelegationRedemptionCancellation {
    pub request: RedemptionRequest,
    pub share: DelegationShare,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationTermination {
    pub delegation: Delegation,
    pub bond_returned: u128,
    pub bond_slashed: u128,
    pub principal_compensation: u128,
    pub insurance_fund_delta: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DelegationInsurancePayoutRecipient {
    pub(crate) principal: u64,
    pub(crate) amount_micro_usdc: u128,
    pub(crate) policy_address: [u8; 20],
    pub(crate) next_policy_balance: U256,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DelegationInsurancePayoutPreview {
    pub(crate) requested_amount_micro_usdc: u128,
    pub(crate) distributed_amount_micro_usdc: u128,
    pub(crate) wallet_balance_after: u128,
    pub(crate) recipients: Vec<DelegationInsurancePayoutRecipient>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationInsurancePayout {
    pub delegation: Delegation,
    pub requested_amount_micro_usdc: u128,
    pub distributed_amount_micro_usdc: u128,
    pub recipient_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DelegationForcedLiquidationPreview {
    pub(crate) requested_lots: u64,
    pub(crate) filled_lots: u64,
    pub(crate) remaining_lots: u64,
    pub(crate) keeper_reward_micro_usdc: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationForcedLiquidation {
    pub delegation: Delegation,
    pub market_id: u64,
    pub keeper_account: u64,
    pub requested_lots: u64,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub keeper_reward_micro_usdc: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RealCapitalInsuranceWalletPayout {
    pub recipient_account: u64,
    pub amount: u128,
    pub wallet_balance: u128,
    pub recipient_balance: u128,
    pub source: RealCapitalInsuranceWalletSpendSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RealCapitalKeeperRewardCandidate {
    pub keeper_account: u64,
    pub amount: u128,
    pub wallet_balance_after: u128,
    pub keeper_address: [u8; 20],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationCrystallization {
    pub delegation: Delegation,
    pub agent_fee: u128,
    pub nav_pre_fee: u128,
    pub nav_post_fee: u128,
    pub agent_balance: u128,
    pub delegation_balance: u128,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DelegationShare {
    pub shares: u128,
    pub locked_for_redemption: u128,
    pub avg_entry_nav_per_share: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RedemptionRequest {
    pub request_id: RedemptionRequestId,
    pub delegation_id: DelegationId,
    pub principal: u64,
    pub shares_requested: u128,
    pub requested_at_ms: u64,
    pub ready_at_ms: u64,
    pub nav_at_request: u128,
    pub status: RedemptionRequestStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketRiskParameters {
    pub maintenance_margin_bps: u32,
    pub liquidation_penalty_bps: u32,
    pub liquidator_fee_bps: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketFeeParameters {
    pub maker_fee_bps: u32,
    pub maker_rebate_bps: u32,
    pub taker_fee_bps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarginSnapshot {
    pub collateral: i128,
    pub maintenance_margin_bps: u32,
    pub maintenance_requirement: u128,
    pub mark_notional: u128,
    pub realized_pnl: i128,
    pub unrealized_pnl: i128,
    pub equity: i128,
    pub margin_surplus: i128,
    pub under_maintenance: bool,
}

/// Stable bridge id contract. Changing this breaks RPC retry idempotency for
/// in-flight RFC 026 delegation opens.
pub fn derive_delegation_id(agent: u64, mandate_id: &[u8; 32], opened_at_ms: u64) -> DelegationId {
    let mut payload = Vec::with_capacity(10 + 8 + mandate_id.len() + 8);
    payload.extend_from_slice(b"delegation");
    payload.extend_from_slice(&agent.to_be_bytes());
    payload.extend_from_slice(mandate_id);
    payload.extend_from_slice(&opened_at_ms.to_be_bytes());
    *keccak256(payload)
}

/// Stable bridge id contract. Changing this breaks RPC retry idempotency for
/// in-flight RFC 026 redemption requests.
pub fn derive_redemption_request_id(
    principal: u64,
    delegation_id: &DelegationId,
    requested_at_ms: u64,
) -> RedemptionRequestId {
    let mut payload = Vec::with_capacity(10 + 8 + delegation_id.len() + 8);
    payload.extend_from_slice(b"redemption");
    payload.extend_from_slice(&principal.to_be_bytes());
    payload.extend_from_slice(delegation_id);
    payload.extend_from_slice(&requested_at_ms.to_be_bytes());
    *keccak256(payload)
}

/// Stable bridge id contract. Changing this breaks RPC retry idempotency for
/// in-flight RFC 026 delegation actions.
pub fn derive_delegation_action_id_from_nonce(
    action_kind: &str,
    scope: &[u8],
    submitter_nonce: u64,
) -> DelegationActionId {
    let mut payload =
        Vec::with_capacity(20 + action_kind.len() + scope.len() + std::mem::size_of::<u64>());
    payload.extend_from_slice(b"delegation-action-id");
    payload.extend_from_slice(action_kind.as_bytes());
    payload.extend_from_slice(scope);
    payload.extend_from_slice(&submitter_nonce.to_be_bytes());
    *keccak256(payload)
}

/// Stable bridge id contract for callers that have not adopted
/// `submitter_nonce`. Changing this breaks legacy retry idempotency.
pub fn derive_legacy_delegation_action_id(
    action: &BlockDelegationAction,
    timestamp_ms: u64,
) -> DelegationActionId {
    let action_bytes = borsh::to_vec(action).expect("delegation action should serialize");
    let mut payload = Vec::with_capacity(27 + action_bytes.len() + std::mem::size_of::<u64>());
    payload.extend_from_slice(b"legacy-delegation-action-id");
    payload.extend_from_slice(&action_bytes);
    payload.extend_from_slice(&timestamp_ms.to_be_bytes());
    *keccak256(payload)
}

pub fn delegation_action_scope(action: &BlockDelegationAction) -> Vec<u8> {
    match action {
        BlockDelegationAction::Open { agent, .. } => agent.to_be_bytes().to_vec(),
        BlockDelegationAction::Deposit { principal, .. }
        | BlockDelegationAction::RequestRedemption { principal, .. } => {
            principal.to_be_bytes().to_vec()
        }
        BlockDelegationAction::CancelRedemption { request_id, .. }
        | BlockDelegationAction::FinalizeRedemption { request_id } => request_id.to_vec(),
        BlockDelegationAction::CloseToDeposits { delegation_id }
        | BlockDelegationAction::BeginWinding { delegation_id }
        | BlockDelegationAction::InsurancePayout { delegation_id, .. }
        | BlockDelegationAction::Close { delegation_id }
        | BlockDelegationAction::Terminate { delegation_id, .. }
        | BlockDelegationAction::ForcedLiquidation { delegation_id, .. } => delegation_id.to_vec(),
    }
}

/// Stable delegation action id contract. Changing this breaks signed and
/// bridge retry idempotency for in-flight RFC 026 delegation actions.
pub fn delegation_action_id_for_submission(
    action: &BlockDelegationAction,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> DelegationActionId {
    if let Some(nonce) = submitter_nonce {
        derive_delegation_action_id_from_nonce(
            delegation_block_action_kind(action),
            &delegation_action_scope(action),
            nonce,
        )
    } else {
        derive_legacy_delegation_action_id(action, timestamp_ms)
    }
}

/// Stable action-body fingerprint. Changing this breaks replay protection for
/// persisted delegation action receipts.
pub fn delegation_action_fingerprint(action: &BlockDelegationAction) -> Hash {
    let action_bytes = borsh::to_vec(action).expect("delegation action should serialize");
    let mut payload = Vec::with_capacity(29 + action_bytes.len());
    payload.extend_from_slice(b"delegation-action-fingerprint");
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

pub fn delegation_action_signing_bytes(signed_action: &SignedDelegationAction) -> Vec<u8> {
    let payload = DelegationActionPayload {
        domain: "0x26:delegation-action:v1".into(),
        chain_id: signed_action.chain_id,
        signer_account_id: signed_action.signer_account_id,
        action_id: signed_action.action_id,
        action_kind: delegation_block_action_kind(&signed_action.action).to_string(),
        action_scope: delegation_action_scope(&signed_action.action),
        timestamp_ms: signed_action.timestamp_ms,
        submitter_nonce: signed_action.submitter_nonce,
        body_hash: delegation_action_fingerprint(&signed_action.action),
    };
    borsh::to_vec(&payload).expect("delegation action payload should serialize")
}

pub fn delegation_trading_account_id(delegation_id: &DelegationId) -> u64 {
    let hash = keccak256([b"delegation-trading-account".as_slice(), delegation_id].concat());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&hash[..8]);
    u64::from_be_bytes(bytes) | (1_u64 << 63)
}

pub(crate) fn delegation_forced_liquidation_client_order_id(action_id: &DelegationActionId) -> u64 {
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&action_id[..8]);
    u64::from_be_bytes(bytes)
}

pub(crate) fn account_id_is_delegation_reserved(account_id: u64) -> bool {
    account_id & (1_u64 << 63) != 0
}

pub(crate) fn delegation_accepts_trading_intents(status: DelegationStatus) -> bool {
    matches!(
        status,
        DelegationStatus::Open | DelegationStatus::ClosedToDeposits | DelegationStatus::Winding
    )
}

pub(crate) fn live_delegation_for_trading_account(
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_id: u64,
) -> Option<&Delegation> {
    // TODO: add a persisted trading_account -> delegation_id index if
    // delegation counts make this admission-path scan hot.
    delegations.values().find(|delegation| {
        delegation_trading_account_id(&delegation.delegation_id) == account_id
            && matches!(
                delegation.status,
                DelegationStatus::Open
                    | DelegationStatus::ClosedToDeposits
                    | DelegationStatus::Winding
            )
    })
}

pub(crate) fn delegation_trading_address(delegation_id: &DelegationId) -> [u8; 20] {
    let hash = keccak256([b"delegation-trading-address".as_slice(), delegation_id].concat());
    let mut address = [0_u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

pub(crate) fn real_capital_bond_escrow_address(delegation_id: &DelegationId) -> [u8; 20] {
    let hash = keccak256([b"real-capital-bond-escrow-v1".as_slice(), delegation_id].concat());
    let mut address = [0_u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

pub(crate) fn real_capital_compensation_escrow_address() -> [u8; 20] {
    let hash = keccak256(b"real-capital-compensation-escrow-v1");
    let mut address = [0_u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

pub(crate) fn real_capital_insurance_wallet_address() -> [u8; 20] {
    let hash = keccak256(b"real-capital-insurance-wallet-v1");
    let mut address = [0_u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

pub(crate) fn delegation_terminate_reason_allows_agent_signature(
    reason: BlockDelegationFaultReason,
) -> bool {
    // TODO(RFC 049 multi-validator): raw-submit currently implies trusted operator authority.
    // When external delegation actions are admitted over P2P, require an operator/governance
    // signature for non-agent-self termination reasons before they reach block proposal.
    matches!(reason, BlockDelegationFaultReason::MandateBreached)
}

pub(crate) fn delegation_termination_slash_policy(
    reason: BlockDelegationFaultReason,
    slash_policy: Option<BlockDelegationSlashPolicy>,
    agent: u64,
) -> Result<BlockDelegationSlashPolicy, NodeError> {
    match reason {
        BlockDelegationFaultReason::WindingGraceFailure => {
            if slash_policy.is_some() {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "delegation_slash_policy_unexpected",
                });
            }
            Ok(BlockDelegationSlashPolicy::WindingGrace)
        }
        BlockDelegationFaultReason::GovernanceFault => {
            slash_policy.ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_slash_policy_required",
            })
        }
        BlockDelegationFaultReason::MandateBreached
        | BlockDelegationFaultReason::UnauthorizedTradingAttempt
        | BlockDelegationFaultReason::RiskLimitBreach
        | BlockDelegationFaultReason::AgentAttestationRevoked => {
            if slash_policy.is_some() {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "delegation_slash_policy_unexpected",
                });
            }
            Ok(BlockDelegationSlashPolicy::PrincipalProRata)
        }
    }
}

pub(crate) fn delegation_block_error_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::InvalidDelegation { reason, .. }
        | NodeError::InvalidMarketMakerProgram { reason, .. }
        | NodeError::InvalidTeeGovernance { reason, .. }
        | NodeError::InvalidRealCapitalGovernance { reason, .. }
        | NodeError::InvalidMandate { reason, .. }
        | NodeError::InvalidIdentity { reason, .. }
        | NodeError::InvalidIdentityAction { reason, .. }
        | NodeError::InvalidMandateAction { reason, .. }
        | NodeError::InvalidCollateralMovement { reason, .. }
        | NodeError::InvalidMarketConfig { reason, .. }
        | NodeError::InvalidOracleSubmission { reason, .. }
        | NodeError::OracleAggregationFailed { reason, .. } => reason,
        NodeError::InvalidIdentityActionNonce { .. } => "invalid_identity_action_nonce",
        NodeError::InvalidIdentityActionSignature { .. } => "invalid_identity_action_signature",
        NodeError::InvalidMandateActionNonce { .. } => "invalid_mandate_action_nonce",
        NodeError::InvalidMandateActionSignature { .. } => "invalid_mandate_action_signature",
        NodeError::InvalidBlock(_) => "invalid_block",
        NodeError::LocalValidatorUnavailable => "local_validator_unavailable",
        NodeError::InvalidConsensusPubKey { .. } => "invalid_consensus_pubkey",
        NodeError::MissingConsensusSigningKey { .. } => "missing_consensus_signing_key",
        NodeError::InvalidClusterSize { .. } => "invalid_cluster_size",
        NodeError::BootstrapConfig(_) => "bootstrap_config",
        NodeError::MissingLeader { .. } => "missing_leader",
        NodeError::ZeroStake => "zero_stake",
        NodeError::Persistence { .. } => "persistence_error",
        NodeError::Execution(_) => "execution_error",
        NodeError::Order(_) => "order_error",
        NodeError::Account(_) => "account_error",
        NodeError::Policy(_) => "policy_error",
        NodeError::PositionFlat { .. } => "position_flat",
        NodeError::NotUnderMaintenance { .. } => "not_under_maintenance",
        NodeError::StaleMarketData { .. } => "stale_market_data",
        NodeError::UnknownMarket { .. } => "unknown_market",
        NodeError::MarketAlreadyListed { .. } => "market_already_listed",
        NodeError::MarketTradingPaused { .. } => "market_trading_paused",
        NodeError::AccountFrozen { .. } => "account_frozen",
        NodeError::DuplicateIntent(_) => "duplicate_intent",
        NodeError::DuplicatePolicyNonce { .. } => "duplicate_policy_nonce",
        NodeError::StaleIntentNonce { .. } => "stale_intent_nonce",
        NodeError::IntentExpired { .. } => "intent_expired",
    }
}

pub(crate) fn validate_open_delegation_config(
    agent: u64,
    mandate_id: &[u8; 32],
    performance_share_units: u16,
    crystallization_cadence_ms: u64,
    notice_period_ms: u64,
    agent_bond: u128,
) -> Result<(), NodeError> {
    if agent == 0 {
        return Err(NodeError::InvalidDelegation {
            agent,
            reason: "agent_zero",
        });
    }
    if mandate_id == &[0u8; 32] {
        return Err(NodeError::InvalidDelegation {
            agent,
            reason: "mandate_zero",
        });
    }
    if performance_share_units == 0 || performance_share_units > PERFORMANCE_SHARE_CAP_UNITS {
        return Err(NodeError::InvalidDelegation {
            agent,
            reason: "performance_share_out_of_range",
        });
    }
    if !(CRYSTALLIZATION_CADENCE_MIN_MS..=CRYSTALLIZATION_CADENCE_MAX_MS)
        .contains(&crystallization_cadence_ms)
    {
        return Err(NodeError::InvalidDelegation {
            agent,
            reason: "crystallization_cadence_out_of_range",
        });
    }
    if !(NOTICE_PERIOD_MIN_MS..=NOTICE_PERIOD_MAX_MS).contains(&notice_period_ms) {
        return Err(NodeError::InvalidDelegation {
            agent,
            reason: "notice_period_out_of_range",
        });
    }
    if agent_bond < AGENT_BOND_MIN {
        return Err(NodeError::InvalidDelegation {
            agent,
            reason: "agent_bond_below_min",
        });
    }
    Ok(())
}
