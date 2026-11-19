use super::*;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct EvmTransactionReceiptRecord {
    pub tx_hash: Hash,
    pub block_hash: Hash,
    pub block_number: u64,
    pub transaction_index: u64,
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub gas_used: u64,
    pub cumulative_gas_used: u64,
    pub success: bool,
    pub log_index_start: u64,
    pub logs: Vec<EvmLogRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingleNodeConfig {
    pub chain_id: u64,
    pub validator_id: ValidatorId,
    pub consensus_signing_key: [u8; 32],
    pub genesis_timestamp_ms: u64,
    pub genesis_state_root: Hash,
    pub pacemaker: PacemakerConfig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockEventCursor {
    pub lower_bound_seq: u64,
    pub count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventSeqRange {
    pub from_height: u64,
    pub before_height: u64,
    pub from_seq: u64,
    pub before_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentAdmissionPreview {
    pub intent_id: IntentId,
    pub class: IntentAdmissionClass,
    pub pending_reason: Option<IntentPendingReason>,
    pub next_eligible_at_ms: Option<u64>,
    pub current_policy_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderInitialMarginPreview {
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
pub struct MarketCustodyRepairAdjustment {
    pub market_id: u64,
    pub custody_balance_micro_usdc: u128,
    pub required_before_micro_usdc: u128,
    pub treasury_before_micro_usdc: u128,
    pub treasury_after_micro_usdc: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketCustodyRepairReport {
    pub dry_run: bool,
    pub adjustments: Vec<MarketCustodyRepairAdjustment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketCustodyAuditReport {
    pub markets: Vec<MarketCustodyAuditMarket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketCustodyAuditMarket {
    pub market_id: u64,
    pub custody_balance_micro_usdc: Option<u128>,
    pub required_balance_micro_usdc: Option<i128>,
    pub custody_surplus_micro_usdc: Option<i128>,
    pub required_error: Option<String>,
    pub positive_isolated_collateral_micro_usdc: i128,
    pub negative_isolated_collateral_micro_usdc: i128,
    pub fee_vault_micro_usdc: u128,
    pub insurance_fund_micro_usdc: u128,
    pub validator_reward_pool_micro_usdc: u128,
    pub market_maker_rebate_pool_micro_usdc: u128,
    pub treasury_micro_usdc: u128,
    pub top_isolated_collateral: Vec<IsolatedCollateralAuditRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolatedCollateralAuditRow {
    pub market_id: u64,
    pub account_id: u64,
    pub collateral_micro_usdc: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolatedCollateralRepairTarget {
    pub market_id: u64,
    pub account_id: u64,
    pub collateral_micro_usdc: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolatedCollateralRepairAdjustment {
    pub market_id: u64,
    pub account_id: u64,
    pub before_micro_usdc: i128,
    pub after_micro_usdc: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsolatedCollateralRepairReport {
    pub dry_run: bool,
    pub collateral_adjustments: Vec<IsolatedCollateralRepairAdjustment>,
    pub custody_adjustments: Vec<MarketCustodyRepairAdjustment>,
    pub after_audit: MarketCustodyAuditReport,
}

#[derive(Debug)]
pub struct PreparedCommittedBlock {
    pub(crate) prepared_block: PreparedBlock<InMemoryCheckpoint>,
    pub(crate) policy_store: PolicyStore,
    pub(crate) order_book_overlays: BTreeMap<u64, SingleMarketBook>,
    pub(crate) resting_order_mandate_overlays: BTreeMap<(u64, u64, u64), MandateId>,
    pub(crate) collateral_overlays: BTreeMap<(u64, u64), i128>,
    pub(crate) rejected_intents: Vec<RejectedIntent>,
    pub(crate) committed_events: Vec<RuntimeEventKind>,
}

impl PreparedCommittedBlock {
    pub fn block(&self) -> &Block {
        &self.prepared_block.block
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LiquidationSizingSuggestion {
    pub lots: u64,
    pub clears_maintenance: bool,
    pub post_margin_surplus: i128,
    pub post_under_maintenance: bool,
    pub post_account_frozen: bool,
    pub post_market_bad_debt_balance: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidationPassStep {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub result: Result<MatchOutcome, NodeError>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiquidationPassReport {
    pub scanned_positions: usize,
    pub attempted_positions: usize,
    pub succeeded_positions: usize,
    pub failed_positions: usize,
    pub skipped_stale_markets: usize,
    // Frozen target accounts remain liquidatable; this counts candidates
    // skipped because the requested liquidator account is frozen.
    pub skipped_frozen_accounts: usize,
    pub steps: Vec<LiquidationPassStep>,
}

impl SingleNodeConfig {
    pub fn genesis_block(&self) -> Block {
        build_genesis_block(
            self.chain_id,
            self.validator_id,
            self.genesis_state_root,
            self.genesis_timestamp_ms,
        )
    }
}
