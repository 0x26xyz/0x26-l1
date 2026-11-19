use crate::jsonrpc::JsonRpcErrorObject;
use crate::rpc_responses::{IdentityTierResponse, SuggestedLiquidationOutcome};
use crate::vaults;
use crate::wire_json::{
    EventFilterJson, OrderRequestJson, SignedEvmAccountClaimJson, SignedEvmOwnerCallJson,
    SignedIntentJson, SignedOwnerCallJson,
};
use kombat_chain_consensus::{
    DelegationActionId, MarketMakerProgramActionId, RealCapitalGovernanceActionId,
};
use kombat_chain_node::{
    DelegationActionReceipt, DelegationActionSubmissionStatus, DelegationId,
    MarketMakerProgramActionReceipt, MarketMakerProgramActionSubmissionStatus,
    RealCapitalGovernanceActionReceipt, RealCapitalGovernanceActionSubmissionStatus,
    VaultRealCapitalMode,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateParams {
    pub key_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateRootAtHeightParams {
    pub height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAccountParams {
    pub account_id: u64,
    pub owner_hex: String,
    pub code_hash_hex: String,
    pub evm_address_hex: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateEvmAccountParams {
    pub signed_account_claim: SignedEvmAccountClaimJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountParams {
    pub account_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountByEvmAddressParams {
    pub address_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountHistoryParams {
    pub account_id: u64,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPolicyParams {
    pub policy_id: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetConsensusLogsParams {
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetIntentStatusParams {
    pub intent_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOrderBookParams {
    pub market_id: u64,
    pub depth: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOpenOrdersParams {
    pub market_id: u64,
    pub account_id: u64,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetTradeHistoryParams {
    pub market_id: u64,
    pub account_id: Option<u64>,
    pub client_order_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetCandlesParams {
    pub market_id: u64,
    pub interval: String,
    #[serde(rename = "from")]
    pub from_ms: u64,
    #[serde(rename = "to")]
    pub to_ms: u64,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetMarketSnapshotParams {
    pub market_id: u64,
    pub depth: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountTradingStateParams {
    pub account_id: u64,
    pub market_id: Option<u64>,
    pub depth: Option<usize>,
    pub fill_limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAccountPnlWindowParams {
    pub account_id: u64,
    pub from_ms: u64,
    pub to_ms: Option<u64>,
    pub market_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAccountEquityHistoryParams {
    pub account_id: u64,
    pub from_ms: u64,
    pub to_ms: u64,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMandateConformanceHistoryParams {
    pub mandate_id_hex: String,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetWalletBalancesParams {
    pub address_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPositionParams {
    pub market_id: u64,
    pub account_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRiskSnapshotParams {
    pub market_id: u64,
    pub account_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSuggestedLiquidationParams {
    pub market_id: u64,
    pub account_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListSuggestedLiquidationsParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetMarketStateParams {
    pub market_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetFundingStateParams {
    pub market_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInsuranceFundParams {
    pub market_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetMarketParams {
    pub market_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPerpetualMarketParams {
    pub market_id: u64,
    pub base_symbol: String,
    pub quote_asset: Option<u32>,
    pub oracle_feed_id: u64,
    pub initial_margin_bps: u32,
    pub maintenance_margin_bps: u32,
    pub max_leverage: u32,
    pub lot_size_microunits: u64,
    pub tick_size_bps: u32,
    pub funding_interval_ms: u64,
    pub oracle_max_staleness_ms: Option<u64>,
    pub breaker_price_band_bps: Option<u32>,
    pub oracle_min_sources: Option<u8>,
    pub oracle_min_confidence: Option<u8>,
    pub oracle_min_samples: Option<u16>,
    pub oracle_feeder_ids_hex: Option<Vec<String>>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetPerpetualMarketStatusParams {
    pub market_id: u64,
    pub status: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetAccountOperatorParams {
    pub account_id: u64,
    pub operator_id: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAccountOperatorParams {
    pub account_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenDelegationParams {
    pub agent: u64,
    pub mandate_id_hex: String,
    pub performance_share_units: u16,
    pub crystallization_cadence_ms: u64,
    pub notice_period_ms: u64,
    pub agent_bond: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepositToDelegationParams {
    pub principal: u64,
    pub delegation_id_hex: String,
    pub amount: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitDepositToDelegationParams {
    pub principal: u64,
    pub delegation_id_hex: String,
    pub amount: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestDelegationRedemptionParams {
    pub principal: u64,
    pub delegation_id_hex: String,
    pub shares: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalizeDelegationRedemptionParams {
    pub request_id_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelDelegationRedemptionParams {
    pub principal: u64,
    pub request_id_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelegationLifecycleParams {
    pub delegation_id_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitDelegationActionParams {
    pub action_id_hex: String,
    pub action_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitSignedDelegationActionParams {
    pub signed_delegation_action: SignedDelegationActionJson,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitEvmDelegationActionParams {
    pub signed_delegation_action: SignedEvmDelegationActionJson,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedDelegationActionJson {
    pub chain_id: u64,
    pub signer_account_id: u64,
    pub action_id_hex: String,
    pub action_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEvmDelegationActionJson {
    pub chain_id: u64,
    pub signer_account_id: u64,
    pub action_id_hex: String,
    pub action_hex: String,
    pub timestamp_ms: u64,
    pub submitter_nonce: u64,
    pub valid_until_ms: u64,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitSignedMarketMakerProgramActionParams {
    pub signed_market_maker_program_action: SignedMarketMakerProgramActionJson,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitMarketMakerProgramActionParams {
    pub action_id_hex: String,
    pub action_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefineMarketMakerProgramParams {
    pub name: String,
    pub markets: Vec<u64>,
    pub min_uptime_bps: u16,
    pub max_spread_bps: u16,
    pub min_quote_size_micro_usdc: String,
    pub min_two_sided_windows_per_hour: u16,
    pub rebate_multiplier_units: u16,
    pub enrollment_bond_micro_usdc: String,
    pub compliance_window_ms: u64,
    pub observation_cadence_ms: u64,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MarketMakerIdentityParams {
    Account {
        account_id: u64,
    },
    Delegation {
        delegation_id_hex: String,
        serving_agent: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollMarketMakerProgramParams {
    #[serde(default)]
    pub market_maker: Option<u64>,
    #[serde(default)]
    pub market_maker_identity: Option<MarketMakerIdentityParams>,
    pub market_id: u64,
    pub program_id_hex: String,
    pub bond_micro_usdc: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedMarketMakerProgramActionJson {
    pub chain_id: u64,
    pub signer_account_id: u64,
    pub action_id_hex: String,
    pub action_hex: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub submitter_nonce: Option<u64>,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMarketMakerProgramActionStatusParams {
    pub action_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMarketMakerProgramParams {
    pub program_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMarketMakerEnrollmentParams {
    #[serde(default)]
    pub market_maker: Option<u64>,
    #[serde(default)]
    pub market_maker_identity: Option<MarketMakerIdentityParams>,
    pub market_id: u64,
    pub program_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetDelegationActionStatusParams {
    pub action_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationActionSubmissionResponse {
    pub action_id: DelegationActionId,
    pub status: DelegationActionSubmissionStatus,
    pub pending_delegation_actions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationActionStatusResponse {
    pub action_id: DelegationActionId,
    pub status: DelegationActionStatus,
    pub action_kind: Option<&'static str>,
    pub submitted_at_ms: Option<u64>,
    pub processed_at_ms: Option<u64>,
    pub receipt: Option<DelegationActionReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelegationActionStatus {
    Pending,
    Processed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketMakerProgramActionSubmissionResponse {
    pub action_id: MarketMakerProgramActionId,
    pub status: MarketMakerProgramActionSubmissionStatus,
    pub pending_market_maker_program_actions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketMakerProgramActionStatusResponse {
    pub action_id: MarketMakerProgramActionId,
    pub status: MarketMakerProgramActionStatus,
    pub action_kind: Option<&'static str>,
    pub submitted_at_ms: Option<u64>,
    pub processed_at_ms: Option<u64>,
    pub receipt: Option<MarketMakerProgramActionReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketMakerProgramActionStatus {
    Pending,
    Processed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMandateParams {
    pub owner_hex: String,
    pub agent: u64,
    pub version: u32,
    pub constraints: Vec<MandateConstraintParam>,
    #[serde(default)]
    pub objectives: Vec<MandateObjectiveParam>,
    #[serde(default)]
    pub objective_weights: Vec<u8>,
    pub conformance_window_ms: u64,
    #[serde(default)]
    pub session_keys_hex: Vec<String>,
    pub expires_at: u64,
    #[serde(default)]
    pub description_uri: Option<String>,
    #[serde(default)]
    pub description_hash_hex: Option<String>,
    #[serde(default)]
    pub activation_tier: Option<u8>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MandateConstraintParam {
    MaxNotionalPerMarket { market: u64, limit_usdc: String },
    MaxNotionalTotal { limit_usdc: String },
    MaxConcentrationInMarketPct { market: u64, pct_bps: u32 },
    AllowedMarkets { set: Vec<u64> },
    MaxLeverage { mult: u32 },
    MaxDrawdownPctFromPeak { pct_bps: u32, window_ms: u64 },
    MaxDailyLossUsdc { limit_usdc: String },
    NoPositionHeldLongerThan { duration_ms: u64 },
    MaxTradeSizeUsdc { limit_usdc: String },
    MaxTradeRatePerMinute { count: u32 },
    NoSameOperatorCounterparty,
    AllowedCounterparties { accounts: Vec<u64> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MandateObjectiveParam {
    TargetSharpe { min: i64, over_ms: u64 },
    TargetReturnPct { min_bps: i32, over_ms: u64 },
    MaxDrawdownPct { max_bps: u32, over_ms: u64 },
    MinTradeCount { count: u32, over_ms: u64 },
    MaxTradeCount { count: u32, over_ms: u64 },
    MaxSlippageBps { limit: u32 },
    MaxFundingPaidPctOfEquity { pct_bps: u32, over_ms: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMandateParams {
    pub mandate_id_hex: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListMandatesParams {
    #[serde(default)]
    pub agent: Option<u64>,
    #[serde(default)]
    pub owner_hex: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitMandateActionParams {
    pub signed_mandate_action: SignedMandateActionJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitEvmMandateActionParams {
    pub signed_mandate_action: SignedEvmMandateActionJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedMandateActionJson {
    pub chain_id: u64,
    pub owner_hex: String,
    pub nonce: u64,
    pub action: MandateActionJson,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEvmMandateActionJson {
    pub chain_id: u64,
    pub owner_hex: String,
    pub nonce: u64,
    pub valid_until_ms: u64,
    pub action: MandateActionJson,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MandateActionJson {
    CreateMandate {
        agent: u64,
        version: u32,
        constraints: Vec<MandateConstraintParam>,
        #[serde(default)]
        objectives: Vec<MandateObjectiveParam>,
        #[serde(default)]
        objective_weights: Vec<u8>,
        conformance_window_ms: u64,
        #[serde(default)]
        session_keys_hex: Vec<String>,
        expires_at: u64,
        #[serde(default)]
        description_uri: Option<String>,
        #[serde(default)]
        description_hash_hex: Option<String>,
    },
    SuspendMandate {
        mandate_id_hex: String,
    },
    ResumeMandate {
        mandate_id_hex: String,
    },
    RevokeMandate {
        mandate_id_hex: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMandateActionNonceParams {
    pub owner_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MandateLifecycleParams {
    pub mandate_id_hex: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeMandateParams {
    pub mandate_id_hex: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpireDueMandatesParams {
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAgentIdentityParams {
    pub account: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListAttestationsParams {
    pub account: u64,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub include_revoked: bool,
    #[serde(default)]
    pub include_expired: bool,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetCompositionAccessParams {
    pub account: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetReputationMultiplierParams {
    pub account: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetIdentityActionNonceParams {
    pub account: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetIdentityAttestorNonceParams {
    pub account: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitIdentityActionParams {
    pub signed_identity_action: SignedIdentityActionJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitTeeAttestationParams {
    pub signed_tee_attestation_action: SignedTeeAttestationActionJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedIdentityActionJson {
    pub chain_id: u64,
    pub account: u64,
    pub nonce: u64,
    pub action: IdentityActionJson,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedTeeAttestationActionJson {
    pub chain_id: u64,
    pub action: TeeAttestationActionJson,
    pub signers: TeeAttestationSignersJson,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IdentityActionJson {
    Register {
        account: u64,
        display_name: String,
        description: String,
        #[serde(default)]
        agent_kind: Option<String>,
        #[serde(default)]
        operator: Option<u64>,
        #[serde(default)]
        contact_uri: Option<String>,
    },
    UpdateDescriptive {
        account: u64,
        update: DescriptiveUpdateJson,
    },
    RegisterOperatorAttestation {
        attestor: u64,
        subject: u64,
        claim: AttestationClaimJson,
        expires_at_timestamp_ms: u64,
    },
    RefreshOperatorAttestation {
        attestor: u64,
        record_id_hex: String,
        new_expires_at_timestamp_ms: u64,
    },
    RevokeAttestation {
        revoker: u64,
        record_id_hex: String,
    },
    Delete {
        account: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeeAttestationActionJson {
    RegisterTeeAttestation {
        attestor: u64,
        subject: u64,
        claim: AttestationClaimJson,
        expires_at_timestamp_ms: u64,
        quote_hex: String,
        nonce_hex: String,
        freshness_anchor_block: u64,
        vendor: String,
    },
    RefreshTeeAttestation {
        attestor: u64,
        record_id_hex: String,
        new_expires_at_timestamp_ms: u64,
        quote_hex: String,
        nonce_hex: String,
        freshness_anchor_block: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeeAttestationSignersJson {
    SelfAttest {
        attestor_nonce: u64,
        attestor_signature_hex: String,
    },
    OperatorAttest {
        attestor_nonce: u64,
        attestor_signature_hex: String,
        subject_nonce: u64,
        subject_signature_hex: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
// Uses `field` rather than `kind` so the outer action keeps the `kind`
// discriminator and the payload names the updated identity field directly.
#[serde(tag = "field", rename_all = "snake_case", deny_unknown_fields)]
pub enum DescriptiveUpdateJson {
    DisplayName { value: String },
    Description { value: String },
    Kind { value: String },
    Operator { value: Option<u64> },
    ContactUri { value: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttestationClaimJson {
    RunsCodeHash {
        code_hash_hex: String,
        #[serde(default)]
        model_hash_hex: Option<String>,
    },
    BoundToMandate {
        mandate_id_hex: String,
    },
    RunsCodeAndBoundToMandate {
        code_hash_hex: String,
        #[serde(default)]
        model_hash_hex: Option<String>,
        mandate_id_hex: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListVendorRootsParams {
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub include_inactive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetVendorCapProfileParams {
    pub vendor: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetDelegationParams {
    pub delegation_id_hex: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListDelegationsParams {
    #[serde(default)]
    pub agent: Option<u64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRedemptionRequestsParams {
    pub principal: u64,
    #[serde(default)]
    pub vault_id_hex: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListVaultPositionsParams {
    pub principal: u64,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

// RFC 043 — Vaults Discovery Surface.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListVaultsParams {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub operator: Option<u64>,
    /// `None` ⇒ default to USDC-only. `Some(empty)` ⇒ "any currency".
    #[serde(default)]
    pub currencies: Option<Vec<u64>>,
    #[serde(default)]
    pub include_unlisted: bool,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetVaultParams {
    pub vault_id_hex: String,
}

/// `kombat_listVaults` response envelope per RFC 043 §"`kombat_listVaults`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListVaultsResponse {
    pub items: Vec<vaults::VaultRecord>,
    pub next_cursor: Option<String>,
}

/// `kombat_getVault` response envelope per RFC 043 §"`kombat_getVault`".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetVaultResponse {
    pub record: Option<vaults::VaultRecord>,
    pub present: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRealCapitalListingsParams {
    #[serde(default)]
    pub agent: Option<u64>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub include_unlisted: bool,
    #[serde(default)]
    pub include_disabled: bool,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRealCapitalListingParams {
    pub vault_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitRealCapitalGovernanceActionParams {
    pub action: RealCapitalGovernanceActionJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RealCapitalGovernanceActionJson {
    UpsertListing {
        vault_id_hex: String,
        delegation_id_hex: String,
        serving_agent: u64,
        mode: String,
        #[serde(default)]
        pilot_min_serving_agent_tier: Option<String>,
        #[serde(default)]
        tier0_pilot_exception: bool,
        deposit_enabled: bool,
        listed_for_discovery: bool,
        per_principal_micro_usdc: String,
        total_vault_tvl_micro_usdc: String,
        #[serde(default)]
        pilot_principal_allowlist: Option<BTreeSet<u64>>,
    },
    SetListingDepositEnabled {
        vault_id_hex: String,
        deposit_enabled: bool,
    },
    SetListingDiscovery {
        vault_id_hex: String,
        listed_for_discovery: bool,
    },
    UpdateProtocolCap {
        total_protocol_tvl_micro_usdc: Option<String>,
    },
    UpdateGlobalKillSwitch {
        disabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitRealCapitalGovernanceActionResponse {
    pub action_id: [u8; 32],
    pub status: RealCapitalGovernanceActionSubmissionStatus,
    pub pending_real_capital_governance_actions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRealCapitalGovernanceActionStatusParams {
    pub action_id_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealCapitalGovernanceActionStatusResponse {
    pub action_id: RealCapitalGovernanceActionId,
    pub status: RealCapitalGovernanceActionStatus,
    pub action_kind: Option<&'static str>,
    pub submitted_at_ms: Option<u64>,
    pub processed_at_ms: Option<u64>,
    pub receipt: Option<RealCapitalGovernanceActionReceipt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealCapitalGovernanceActionStatus {
    Pending,
    Processed,
    Rejected,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealCapitalListingResponse {
    pub vault_id: vaults::VaultId,
    pub delegation_id: DelegationId,
    pub serving_agent: u64,
    pub mode: VaultRealCapitalMode,
    pub pilot_min_serving_agent_tier: Option<IdentityTierResponse>,
    pub tier0_pilot_exception: bool,
    pub deposit_enabled: bool,
    pub listed_for_discovery: bool,
    pub per_principal_cap_micro_usdc: u128,
    pub total_vault_tvl_cap_micro_usdc: u128,
    pub protocol_cap_micro_usdc: Option<u128>,
    pub vault_deposited_micro_usdc: u128,
    pub pilot_principal_allowlist: Option<Vec<u64>>,
    pub listed_at_block: u64,
    pub last_updated_block: u64,
    pub last_updated_governance_action: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealCapitalControlResponse {
    pub global_deposit_kill_switch: bool,
    pub last_updated_block: u64,
    pub last_updated_governance_action: [u8; 32],
    pub protocol_cap_micro_usdc: Option<u128>,
    pub total_protocol_delegation_tvl_micro_usdc: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRealCapitalListingsResponse {
    pub items: Vec<RealCapitalListingResponse>,
    pub next_cursor: Option<String>,
    pub control: RealCapitalControlResponse,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetRealCapitalListingResponse {
    pub listing: Option<RealCapitalListingResponse>,
    pub present: bool,
    pub control: RealCapitalControlResponse,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPerpetualMarketsParams {
    pub status: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLiquidationPressureSummaryParams {
    pub market_id: Option<u64>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetIsolatedCollateralParams {
    pub market_id: u64,
    pub account_id: u64,
    pub collateral: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMarketRiskParams {
    pub market_id: u64,
    pub maintenance_margin_bps: u32,
    pub liquidation_penalty_bps: Option<u32>,
    pub liquidator_fee_bps: Option<u32>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetFundingIndexParams {
    pub market_id: u64,
    pub funding_index: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetIndexPriceParams {
    pub market_id: u64,
    pub index_price_ticks: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMarkPriceParams {
    pub market_id: u64,
    pub mark_price_ticks: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleFeedSampleJson {
    pub feeder_id_hex: String,
    pub price_ticks: u64,
    pub sources_used: u8,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitOraclePricesParams {
    pub market_id: u64,
    pub samples: Vec<OracleFeedSampleJson>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceFundingIntervalParams {
    pub market_id: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMarketFeesParams {
    pub market_id: u64,
    pub maker_fee_bps: u32,
    #[serde(default)]
    pub maker_rebate_bps: u32,
    pub taker_fee_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMarketTradingStateParams {
    pub market_id: u64,
    pub paused: bool,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetMarketOracleConfigParams {
    pub market_id: u64,
    pub max_staleness_ms: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetPolicyUsdcBalanceParams {
    pub address_hex: String,
    pub amount: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaucetFundAccountParams {
    pub address_hex: String,
    pub policy_usdc: Option<String>,
    pub evm_usdc: Option<String>,
    #[serde(default)]
    pub wait_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetEvmUsdcMinterParams {
    pub address_hex: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidatePositionParams {
    pub market_id: u64,
    pub account_id: u64,
    pub liquidator_account_id: u64,
    pub client_order_id: u64,
    pub requested_lots: Option<u64>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLiquidationPassParams {
    pub liquidator_account_id: u64,
    pub first_client_order_id: u64,
    pub max_positions: usize,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLiquidationHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub liquidator_account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLiquidationEscalationHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOracleHistoryParams {
    pub market_id: Option<u64>,
    pub topic: Option<String>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetMarketTradingHistoryParams {
    pub market_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetInsuranceFundHistoryParams {
    pub market_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetFundingHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetBadDebtHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetSolvencyHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRiskStateHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetLiquidationCandidateHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetFeeHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub client_order_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRevenueSplitHistoryParams {
    pub market_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRiskSnapshotsParams {
    pub account_id: u64,
    pub market_id: Option<u64>,
    pub stressed_only: Option<bool>,
    pub freeze_imminent_only: Option<bool>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
    pub include_flat: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountRiskSummaryParams {
    pub account_id: u64,
    pub market_id: Option<u64>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
    pub include_flat: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAccountRiskSummariesParams {
    pub market_id: Option<u64>,
    pub imminent_freeze_only: Option<bool>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
    pub include_flat: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListMarketStatesParams {
    pub stressed_only: Option<bool>,
    pub imminent_freeze_only: Option<bool>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLiquidationCandidatesParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub freeze_imminent_only: Option<bool>,
    pub outcome: Option<SuggestedLiquidationOutcome>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPositionsParams {
    pub account_id: u64,
    pub include_flat: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetPositionHistoryParams {
    pub market_id: Option<u64>,
    pub account_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetAccountTradeHistoryParams {
    pub account_id: u64,
    pub market_id: Option<u64>,
    pub from_seq: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetOrderStatusParams {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEventsParams {
    pub from_seq: Option<u64>,
    pub before_seq: Option<u64>,
    pub limit: Option<usize>,
    pub descending: Option<bool>,
    pub filter: Option<EventFilterJson>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitIntentParams {
    pub signed_intent: SignedIntentJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitIntentBatchParams {
    pub items: Vec<SubmitIntentParams>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitIntentBatchItemResponse {
    pub accepted: bool,
    pub intent_id_hex: Option<String>,
    pub response: Option<Value>,
    pub error: Option<JsonRpcErrorObject>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitIntentBatchResponse {
    pub results: Vec<SubmitIntentBatchItemResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitOrderParams {
    pub order: OrderRequestJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewIntentParams {
    pub order: OrderRequestJson,
    #[serde(default)]
    pub replace_client_order_id: Option<u64>,
    #[serde(default)]
    pub timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelOrderParams {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceOrderParams {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub replacement: OrderRequestJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitOwnerCallParams {
    pub signed_owner_call: SignedOwnerCallJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitEvmOwnerCallParams {
    pub signed_owner_call: SignedEvmOwnerCallJson,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProduceBlockParams {
    pub timestamp_ms: u64,
    pub max_items: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunNetworkRoundParams {
    pub timestamp_ms: u64,
    pub max_items: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunNetworkTimeoutRecoveryParams {
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotFileParams {
    pub path: String,
    /// When true, `kombat_loadSnapshot` is allowed to overwrite a
    /// node whose engine has been advanced past genesis. Defaults to
    /// `false`: a load against a populated node is rejected so an
    /// operator cannot accidentally clobber durable state by pointing
    /// `kombat_loadSnapshot` at a running node. Ignored by
    /// `kombat_saveSnapshot`. Field is omitted from RPC payloads
    /// when false to keep the wire shape stable for legacy callers.
    #[serde(default, skip_serializing_if = "is_false")]
    pub force_reset: bool,
}

fn is_false(v: &bool) -> bool {
    !*v
}
