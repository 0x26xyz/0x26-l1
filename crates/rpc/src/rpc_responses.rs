use crate::vaults;
use crate::wire_json::PolicyTermsWireJson;
use kombat_chain_consensus::{Hash, IntentId, ValidatorId};
use kombat_chain_matching::{MatchFill, MatchStatus, OrderSide};
use kombat_chain_node::{
    AccountEquitySample, Delegation, DelegationActionReceipt, DelegationDeposit, DelegationId,
    DelegationRedemptionCancellation, DelegationRedemptionFinalization,
    DelegationRedemptionRequest, DelegationShare, LiquidationSizingSuggestion, Mandate,
    MandateConformanceSample, MandateId, PerpetualMarket, RedemptionRequest,
};
use kombat_chain_policy::PolicyState;
use kombat_chain_streams::{
    AccountUpdateAction, IntentAdmissionClass, IntentPendingReason, IntentStatus,
    LiquidationEscalationAction, LiquidationExecutionOutcome, OrderFillRole, OrderFillSide,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainInfoResponse {
    pub chain_id: u64,
    pub source_network_id: String,
    pub head_height: u64,
    pub head_timestamp_ms: u64,
    pub head_hash: Hash,
    pub committed_state_root: Hash,
    pub quarantined_block_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractCallRequest {
    pub caller: [u8; 20],
    pub to: [u8; 20],
    pub data: Vec<u8>,
    pub gas_limit: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractCallResponse {
    pub returned: Vec<u8>,
    pub gas_used: u64,
    pub state_root: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EthCallObject {
    pub from: Option<String>,
    pub to: String,
    pub data: Option<String>,
    pub gas: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyUsdcBalanceResponse {
    pub address: [u8; 20],
    pub balance: String,
    pub state_root: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaucetFundAccountResponse {
    pub address: [u8; 20],
    pub policy_usdc_balance: String,
    pub evm_usdc_balance: String,
    pub evm_usdc_total_supply: String,
    pub evm_usdc_deposit_tx_hash: Option<Hash>,
    pub state_root: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletBalancesResponse {
    pub address: [u8; 20],
    pub policy_usdc_balance: String,
    pub evm_usdc_balance: String,
    pub evm_usdc_total_supply: String,
    pub next_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmUsdcMinterRoleResponse {
    pub address: [u8; 20],
    pub enabled: bool,
    pub state_root: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateQueryResponse {
    pub key: Vec<u8>,
    pub value: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateRootAtHeightResponse {
    pub height: u64,
    pub block_hash: Hash,
    pub state_root: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitIntentResponse {
    pub intent_id: IntentId,
    pub class: IntentAdmissionClass,
    pub pending_reason: Option<IntentPendingReason>,
    pub next_eligible_at_ms: Option<u64>,
    pub current_policy_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitOwnerCallResponse {
    pub accepted: bool,
    pub account_id: u64,
    pub chain_id: u64,
    pub nonce: u64,
    pub action: &'static str,
    pub policy_id: Option<u64>,
    pub new_owner: Option<[u8; 32]>,
    pub code_hash: Option<Hash>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitOrderResponse {
    pub status: MatchStatus,
    pub fills: Vec<MatchFill>,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub posted_order_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewIntentResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub would_succeed: bool,
    pub reason: Option<&'static str>,
    pub current_raw_notional: u128,
    pub candidate_raw_notional: u128,
    pub current_notional: u128,
    pub candidate_notional: u128,
    pub required_initial: u128,
    pub equity_for_market: i128,
    pub margin_surplus: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub liquidator_account_id: u64,
    pub step_count: usize,
    pub status: MatchStatus,
    pub fills: Vec<MatchFill>,
    pub requested_lots: u64,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub executed_notional: u128,
    pub penalty: u128,
    pub insurance_cover: u128,
    pub liquidator_reward: u128,
    pub insurance_fund_balance: u128,
    pub post_net_lots: i64,
    pub post_equity: i128,
    pub post_maintenance_requirement: u128,
    pub post_margin_surplus: i128,
    pub post_under_maintenance: bool,
    pub post_account_frozen: bool,
    pub post_market_bad_debt_balance: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationPassStepResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub status: Option<MatchStatus>,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationPassResponse {
    pub liquidator_account_id: u64,
    pub scanned_positions: usize,
    pub attempted_positions: usize,
    pub succeeded_positions: usize,
    pub failed_positions: usize,
    pub skipped_stale_markets: usize,
    pub skipped_frozen_accounts: usize,
    pub steps: Vec<LiquidationPassStepResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub liquidator_account_id: u64,
    pub requested_lots: u64,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub executed_notional: u128,
    pub penalty: u128,
    pub insurance_cover: u128,
    pub liquidator_reward: u128,
    pub insurance_fund_balance: u128,
    pub outcome: Option<LiquidationExecutionOutcome>,
    pub post_net_lots: Option<i64>,
    pub post_margin_surplus: Option<i128>,
    pub post_under_maintenance: Option<bool>,
    pub post_account_frozen: Option<bool>,
    pub post_market_bad_debt_balance: Option<u128>,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationEscalationRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub action: LiquidationEscalationAction,
    pub stall_count: u8,
    pub freeze_threshold: u8,
    pub freeze_imminent: bool,
    pub post_margin_surplus: i128,
    pub post_under_maintenance: bool,
    pub post_account_frozen: bool,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OracleHistoryKindResponse {
    OracleConfigUpdated { max_staleness_ms: u64 },
    MarkPriceUpdated { mark_price_ticks: u64 },
    IndexPriceUpdated { index_price_ticks: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OracleHistoryRecordResponse {
    pub market_id: u64,
    pub kind: OracleHistoryKindResponse,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketTradingStateRecordResponse {
    pub market_id: u64,
    pub paused: bool,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsuranceFundUpdateRecordResponse {
    pub market_id: u64,
    pub delta: i128,
    pub balance: u128,
    pub source: kombat_chain_streams::InsuranceFundUpdateSource,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BadDebtRecordKind {
    Updated,
    Recovered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadDebtRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub kind: BadDebtRecordKind,
    pub delta: u128,
    pub balance: u128,
    pub source: kombat_chain_streams::RiskTransitionSource,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolvencyBreachRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub residual_bad_debt: u128,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FundingRecordKind {
    IndexUpdated,
    Applied,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundingRecordResponse {
    pub market_id: u64,
    pub account_id: Option<u64>,
    pub funding_index: i128,
    pub funding_delta: Option<i128>,
    pub kind: FundingRecordKind,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundingStateResponse {
    pub market_id: u64,
    pub funding_index: i128,
    pub premium_rate_ppm: Option<i64>,
    pub indicative_funding_rate_ppm: Option<i64>,
    pub mark_price_ticks: Option<u64>,
    pub index_price_ticks: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsuranceFundResponse {
    pub market_id: u64,
    pub insurance_fund_balance: u128,
    pub bad_debt_balance: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub role: OrderFillRole,
    pub fee_amount: u128,
    pub fee_vault_balance: u128,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevenueSplitRecordResponse {
    pub market_id: u64,
    pub source_fee_amount: u128,
    pub validator_rewards_delta: u128,
    pub validator_reward_balance: u128,
    pub market_maker_rebate_delta: u128,
    pub market_maker_rebate_balance: u128,
    pub insurance_fund_delta: u128,
    pub insurance_fund_balance: u128,
    pub treasury_delta: u128,
    pub treasury_balance: u128,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancelOrderResponse {
    pub cancelled: bool,
    pub order: Option<RestingOrderResponse>,
}

pub type IntentStatusResponse = IntentStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProduceBlockResponse {
    pub height: u64,
    pub hash: Hash,
    pub state_root: Hash,
    pub item_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotFileResponse {
    pub path: String,
    pub chain_id: u64,
    pub head_height: u64,
    pub head_hash: Hash,
    pub committed_state_root: Hash,
    pub quarantined_block_count: usize,
    pub snapshot_size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyResponse {
    pub id: u64,
    pub authority_account: u64,
    pub account: u64,
    pub execution_account_id: u64,
    pub execution_scope: PolicyExecutionScopeResponse,
    pub parent_policy_id: Option<u64>,
    pub state: PolicyState,
    pub next_intent_nonce: u64,
    pub delegation_depth: u8,
    pub revoked_at: Option<u64>,
    pub terms: PolicyTermsWireJson,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyExecutionScopeResponse {
    pub kind: PolicyExecutionScopeKindResponse,
    pub delegation_id_hex: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyExecutionScopeKindResponse {
    Account,
    DelegationTrading,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountResponse {
    pub id: u64,
    pub owner: [u8; 32],
    pub pending_owner: Option<[u8; 32]>,
    pub created_at: u64,
    pub paused: bool,
    pub frozen: bool,
    pub controllers: Vec<u64>,
    pub nonce: u64,
    pub code_hash: [u8; 32],
    pub evm_address: Option<[u8; 20]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountHistoryKindResponse {
    Created {
        owner: [u8; 32],
    },
    Updated {
        action: AccountUpdateAction,
        reason: Option<kombat_chain_streams::RiskTransitionSource>,
        owner: [u8; 32],
        pending_owner: Option<[u8; 32]>,
        paused: bool,
        nonce: u64,
        code_hash: [u8; 32],
        controllers: Vec<u64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountHistoryRecordResponse {
    pub account_id: u64,
    pub kind: AccountHistoryKindResponse,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BookLevelResponse {
    pub price_ticks: u64,
    pub total_lots: u64,
    pub order_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderBookResponse {
    pub market_id: u64,
    pub bids: Vec<BookLevelResponse>,
    pub asks: Vec<BookLevelResponse>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderStatusStage {
    Unknown,
    Resting,
    Filled,
    PartiallyFilledResting,
    PartiallyFilledCancelled,
    Cancelled,
    Replaced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderStatusResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub stage: OrderStatusStage,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub replacement_client_order_id: Option<u64>,
    pub latest_event_seq: Option<u64>,
    pub block_height: Option<u64>,
    pub timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TradeFillResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub counterparty_account_id: u64,
    pub counterparty_client_order_id: u64,
    pub mandate_id: Option<[u8; 32]>,
    pub role: OrderFillRole,
    pub side: OrderFillSide,
    pub price_ticks: u64,
    pub quantity_lots: u64,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandleResponse {
    pub market_id: u64,
    pub interval: String,
    pub timestamp_ms: u64,
    pub open: u64,
    pub high: u64,
    pub low: u64,
    pub close: u64,
    pub volume_lots: u64,
    pub trade_count: u64,
    pub first_seq: u64,
    pub last_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketSnapshotResponse {
    pub market: Option<PerpetualMarketResponse>,
    pub state: MarketStateResponse,
    pub order_book: OrderBookResponse,
    pub latest_trade: Option<TradeFillResponse>,
    pub next_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountTradingStateResponse {
    pub account_id: u64,
    pub market_id: Option<u64>,
    pub positions: Vec<PositionResponse>,
    pub open_orders: Vec<RestingOrderResponse>,
    pub risk_summary: AccountRiskSummaryResponse,
    pub recent_fills: Vec<TradeFillResponse>,
    pub next_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionSide {
    Flat,
    Long,
    Short,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub mandate_id: Option<[u8; 32]>,
    pub side: PositionSide,
    pub net_lots: i64,
    pub open_notional: u128,
    pub realized_pnl: i128,
    pub average_entry_price_ticks: Option<u64>,
    pub buy_lots: u64,
    pub sell_lots: u64,
    pub buy_notional: u128,
    pub sell_notional: u128,
    pub last_fill_seq: Option<u64>,
    pub last_fill_timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionHistoryRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub net_lots: i64,
    pub open_notional: u128,
    pub realized_pnl: i128,
    pub average_entry_price_ticks: Option<u64>,
    pub buy_lots: u64,
    pub sell_lots: u64,
    pub buy_notional: u128,
    pub sell_notional: u128,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskSnapshotResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub side: PositionSide,
    pub net_lots: i64,
    pub max_liquidatable_lots: u64,
    pub liquidation_stall_count: u8,
    pub liquidation_freeze_threshold: u8,
    pub liquidation_freeze_imminent: bool,
    pub suggested_liquidation_lots: Option<u64>,
    pub suggested_liquidation_outcome: Option<SuggestedLiquidationOutcome>,
    pub suggested_liquidation_clears_maintenance: bool,
    pub suggested_post_margin_surplus: Option<i128>,
    pub suggested_post_under_maintenance: Option<bool>,
    pub suggested_post_account_frozen: Option<bool>,
    pub suggested_post_market_bad_debt_balance: Option<u128>,
    pub open_notional: u128,
    pub realized_pnl: i128,
    pub average_entry_price_ticks: Option<u64>,
    pub collateral: i128,
    pub mark_price_ticks: Option<u64>,
    pub mark_notional: u128,
    pub unrealized_pnl: i128,
    pub equity: i128,
    pub maintenance_margin_bps: u32,
    pub maintenance_requirement: u128,
    pub liquidation_gap: u128,
    pub margin_surplus: i128,
    pub under_maintenance: bool,
    pub latest_account_risk_transition: Option<LatestRiskTransitionResponse>,
    pub last_fill_seq: Option<u64>,
    pub last_fill_timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestedLiquidationPlanResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub current_liquidation_gap: u128,
    pub suggested_liquidation_lots: u64,
    pub suggested_liquidation_outcome: SuggestedLiquidationOutcome,
    pub clears_maintenance: bool,
    pub post_margin_surplus: i128,
    pub post_under_maintenance: bool,
    pub post_account_frozen: bool,
    pub post_market_bad_debt_balance: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountRiskSummaryResponse {
    pub account_id: u64,
    pub market_count: usize,
    pub frozen: bool,
    pub latest_account_risk_transition: Option<LatestRiskTransitionResponse>,
    pub max_liquidation_stall_count: u8,
    pub imminent_freeze_market_count: usize,
    pub worst_market_id: Option<u64>,
    pub worst_liquidation_gap: u128,
    pub markets_cleared_by_suggested_liquidation: usize,
    pub markets_still_under_after_suggested_liquidation: usize,
    pub markets_freezing_after_suggested_liquidation: usize,
    pub markets_creating_bad_debt_after_suggested_liquidation: usize,
    pub total_suggested_bad_debt: u128,
    pub total_bad_debt: u128,
    pub total_collateral: i128,
    pub total_realized_pnl: i128,
    pub total_unrealized_pnl: i128,
    pub total_equity: i128,
    pub total_open_notional: u128,
    pub total_initial_margin_required: u128,
    pub total_maintenance_requirement: u128,
    pub total_liquidation_gap: u128,
    pub total_margin_surplus: i128,
    pub under_maintenance_market_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountPnlWindowResponse {
    pub account_id: u64,
    pub market_id: Option<u64>,
    pub from_ms: u64,
    pub to_ms: u64,
    pub total_realized_pnl: i128,
    pub funding_paid: i128,
    pub trading_realized_pnl: i128,
    pub fees_paid: u128,
    pub fill_count: usize,
    pub fee_count: usize,
    pub funding_count: usize,
    pub first_seq: Option<u64>,
    pub last_seq: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountEquityHistoryResponse {
    pub account_id: u64,
    pub from_ms: u64,
    pub to_ms: u64,
    pub samples: Vec<AccountEquitySample>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MandateConformanceHistoryResponse {
    pub mandate_id: MandateId,
    pub samples: Vec<MandateConformanceSample>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityTierResponse {
    Tier0Unattested,
    Tier1OperatorAttested,
    Tier2TeeAttested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeeVendorResponse {
    IntelSgxDcap,
    AmdSevSnp,
    AwsNitro,
    AppleSecureEnclave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentIdentityResponse {
    pub account: u64,
    pub tier: IdentityTierResponse,
    pub operator: Option<u64>,
    pub kind: &'static str,
    pub display_name: String,
    pub description: String,
    pub contact_uri: Option<String>,
    pub active_attestation_count: usize,
    pub registered_at_block: Option<u64>,
    pub registered_at_timestamp_ms: Option<u64>,
    pub last_updated_block: Option<u64>,
    pub last_updated_timestamp_ms: Option<u64>,
    pub virtual_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestationRecordResponse {
    pub record_id_hex: String,
    pub account: u64,
    pub kind: &'static str,
    pub attestor: u64,
    pub claim: Value,
    pub evidence: Value,
    pub issued_at_block: u64,
    pub issued_at_timestamp_ms: u64,
    pub expires_at_timestamp_ms: u64,
    pub revoked_at_block: Option<u64>,
    pub verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttestationListResponse {
    pub items: Vec<AttestationRecordResponse>,
    pub next_cursor: Option<String>,
    pub scaffold: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionAccessResponse {
    pub account: u64,
    pub performance_share_delegator: bool,
    pub performance_share_delegatee: bool,
    pub reputation_collateral: bool,
    pub a2a_contract: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReputationMultiplierResponse {
    pub account: u64,
    pub multiplier: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorRootRecordResponse {
    pub vendor: TeeVendorResponse,
    pub root_id_hex: String,
    pub valid_from_block: u64,
    pub valid_until_block: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingVendorGovernanceActionResponse {
    IntroduceVendorRoot {
        vendor: TeeVendorResponse,
        root_id_hex: String,
        activates_at_block: u64,
        governance_action_hex: String,
    },
    RevokeVendorRoot {
        vendor: TeeVendorResponse,
        root_id_hex: String,
        deactivates_at_block: u64,
        // None only for snapshot-restored pre-governance records; current
        // RFC 051 apply path always records the revoking action id.
        governance_action_hex: Option<String>,
    },
    UpdateVendorCapProfile {
        vendor: TeeVendorResponse,
        cap_multiplier: String,
        composition_eligibility: bool,
        activates_at_block: u64,
        governance_action_hex: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorRootsResponse {
    pub vendor: Option<TeeVendorResponse>,
    pub roots: Vec<VendorRootRecordResponse>,
    pub pending_governance_actions: Vec<PendingVendorGovernanceActionResponse>,
    pub tee_attestation_enabled: bool,
    pub scaffold: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VendorCapProfileResponse {
    pub vendor: TeeVendorResponse,
    pub cap_multiplier: String,
    pub composition_eligibility: bool,
    pub tee_attestation_enabled: bool,
    pub scaffold: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskStateHistoryRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub net_lots: i64,
    pub collateral: i128,
    pub equity: i128,
    pub maintenance_requirement: u128,
    pub liquidation_gap: u128,
    pub margin_surplus: i128,
    pub under_maintenance: bool,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationCandidateHistoryRecordResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub net_lots: i64,
    pub equity: i128,
    pub maintenance_requirement: u128,
    pub liquidation_gap: u128,
    pub margin_surplus: i128,
    pub under_maintenance: bool,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketStateResponse {
    pub market_id: u64,
    pub trading_paused: bool,
    pub liquidation_candidate_count: usize,
    pub imminent_freeze_count: usize,
    pub max_liquidation_stall_count: u8,
    pub total_liquidation_gap: u128,
    pub worst_liquidation_gap: u128,
    pub suggested_clear_count: usize,
    pub suggested_freeze_count: usize,
    pub suggested_bad_debt_count: usize,
    pub total_suggested_bad_debt: u128,
    pub oracle_max_staleness_ms: u64,
    pub order_price_inputs_ready: bool,
    pub liquidation_price_inputs_ready: bool,
    pub mark_price_ticks: Option<u64>,
    pub mark_updated_at_ms: Option<u64>,
    pub mark_price_stale: Option<bool>,
    pub index_price_ticks: Option<u64>,
    pub index_updated_at_ms: Option<u64>,
    pub index_price_stale: Option<bool>,
    pub premium_rate_ppm: Option<i64>,
    pub indicative_funding_rate_ppm: Option<i64>,
    pub funding_index: i128,
    pub maker_fee_bps: u32,
    pub maker_rebate_bps: u32,
    pub taker_fee_bps: u32,
    pub fee_vault_balance: u128,
    pub validator_reward_pool_balance: u128,
    pub market_maker_rebate_pool_balance: u128,
    pub treasury_balance: u128,
    pub maintenance_margin_bps: u32,
    pub liquidation_penalty_bps: u32,
    pub liquidator_fee_bps: u32,
    pub insurance_fund_balance: u128,
    pub bad_debt_balance: u128,
    pub open_interest_lots: u128,
    pub latest_market_risk_transition: Option<LatestMarketRiskTransitionResponse>,
    pub recent_market_risk_transitions: Vec<LatestMarketRiskTransitionResponse>,
    pub top_liquidation_accounts: Vec<AccountRiskSummaryResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerpetualMarketResponse {
    pub market: PerpetualMarket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationResponse {
    pub delegation: Delegation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PeerFanoutFailure {
    pub(crate) validator_id: ValidatorId,
    pub(crate) message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DelegationCommitOutcome {
    Committed { block_height: u64 },
    Duplicate { receipt: DelegationActionReceipt },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationShareResponse {
    pub delegation_id: DelegationId,
    pub principal: u64,
    pub share: DelegationShare,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationDepositResponse {
    pub deposit: DelegationDeposit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedemptionRequestResponse {
    pub request: RedemptionRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedemptionRequestListItem {
    pub request: RedemptionRequest,
    pub vault_id: vaults::VaultId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRedemptionRequestsResponse {
    pub items: Vec<RedemptionRequestListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultPositionListItem {
    pub vault_id: vaults::VaultId,
    pub kind: vaults::VaultKind,
    pub delegation_id: DelegationId,
    pub principal: u64,
    pub share: DelegationShare,
    pub principal_deposited_micro_usdc: u128,
    pub authoritative_nav_per_share: u128,
    pub authoritative_nav_source: &'static str,
    pub indicative_nav_per_share: u128,
    pub indicative_nav_source: &'static str,
    pub live_trading: Option<vaults::VaultLiveTradingInfo>,
    pub live_nav_per_share: Option<u128>,
    pub live_estimated_value_micro_usdc: Option<i128>,
    pub compensation_claim_micro_usdc: u128,
    pub estimated_value_micro_usdc: u128,
    pub status: vaults::VaultStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListVaultPositionsResponse {
    pub items: Vec<VaultPositionListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRedemptionRequestResponse {
    pub redemption: DelegationRedemptionRequest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRedemptionFinalizationResponse {
    pub finalization: DelegationRedemptionFinalization,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationRedemptionCancellationResponse {
    pub cancellation: DelegationRedemptionCancellation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MandateResponse {
    pub mandate: Mandate,
    pub latest_delta: Option<i16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountOperatorResponse {
    pub account_id: u64,
    pub operator_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationPressureSummaryResponse {
    pub market_count: usize,
    pub stressed_market_count: usize,
    pub stressed_account_count: usize,
    pub imminent_freeze_market_count: usize,
    pub imminent_freeze_account_count: usize,
    pub total_liquidation_candidate_count: usize,
    pub total_liquidation_gap: u128,
    pub worst_market_id: Option<u64>,
    pub worst_market_gap: u128,
    pub worst_account_id: Option<u64>,
    pub worst_account_gap: u128,
    pub suggested_clear_count: usize,
    pub suggested_still_under_count: usize,
    pub suggested_freeze_count: usize,
    pub suggested_bad_debt_count: usize,
    pub total_suggested_bad_debt: u128,
    pub latest_market_risk_transition: Option<LatestMarketRiskTransitionResponse>,
    pub recent_market_risk_transitions: Vec<LatestMarketRiskTransitionResponse>,
    pub top_markets: Vec<MarketStateResponse>,
    pub top_accounts: Vec<AccountRiskSummaryResponse>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskTransitionKindResponse {
    Freeze,
    Unfreeze,
    BadDebtUpdated,
    BadDebtRecovered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatestRiskTransitionResponse {
    pub kind: RiskTransitionKindResponse,
    pub source: kombat_chain_streams::RiskTransitionSource,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LatestMarketRiskTransitionResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub kind: RiskTransitionKindResponse,
    pub source: kombat_chain_streams::RiskTransitionSource,
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct RiskSnapshotSupplement {
    pub(crate) liquidation_stall_count: u8,
    pub(crate) liquidation_freeze_threshold: u8,
    pub(crate) liquidation_freeze_imminent: bool,
    pub(crate) suggested_liquidation_lots: Option<LiquidationSizingSuggestion>,
    pub(crate) latest_account_risk_transition: Option<LatestRiskTransitionResponse>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RiskSnapshotListOptions {
    pub(crate) market_id: Option<u64>,
    pub(crate) stressed_only: bool,
    pub(crate) freeze_imminent_only: bool,
    pub(crate) outcome: Option<SuggestedLiquidationOutcome>,
    pub(crate) include_flat: bool,
    pub(crate) limit: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedLiquidationOutcome {
    ClearsMaintenance,
    StillUnderMaintenance,
    FreezesAccount,
    CreatesBadDebt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestingOrderResponse {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub side: OrderSide,
    pub price_ticks: u64,
    pub remaining_lots: u64,
    pub expires_at_ms: Option<u64>,
    pub sequence: u64,
}
