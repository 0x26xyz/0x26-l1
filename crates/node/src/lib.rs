mod consensus_helpers;
mod delegation;
mod engine_block_flow;
mod engine_delegation;
mod engine_delegation_lifecycle;
mod engine_events;
mod engine_frontier;
mod engine_handle;
mod engine_history;
mod engine_identity;
mod engine_mandate;
mod engine_market_maker;
mod engine_market_ops;
mod engine_market_state;
mod engine_pending_actions;
mod engine_persisted_validation;
mod engine_runtime;
mod engine_runtime_helpers;
mod engine_snapshot;
mod errors;
pub mod execution;
mod history;
mod identity;
mod intent_pool;
mod intent_types;
mod local_cluster;
mod mandate;
mod market_maker;
mod market_state;
pub mod migrations;
mod pending_actions;
mod persistence;
mod runtime_types;
mod snapshots;
mod telemetry;

use borsh::{BorshDeserialize, BorshSerialize};
pub use consensus_helpers::{
    assemble_quorum_certificate, select_leader_for_validator_set,
    validator_verifiers_for_validator_set,
};
use consensus_helpers::{
    build_block, build_genesis_block, build_quorum_certificate, consensus_pubkey_from_signing_key,
    consensus_signature_bytes, select_leader, synthetic_mahi_quorum_certificate,
    validate_consensus_key_material, validator_verifiers,
};
pub(crate) use delegation::{
    account_id_is_delegation_reserved, agent_identity_tier_from_block,
    delegation_accepts_trading_intents, delegation_block_error_reason,
    delegation_forced_liquidation_client_order_id, delegation_lifecycle_status,
    delegation_terminate_reason_allows_agent_signature, delegation_termination_slash_policy,
    delegation_trading_address, live_delegation_for_trading_account,
    real_capital_bond_escrow_address, real_capital_compensation_escrow_address,
    real_capital_insurance_wallet_address, real_capital_listing_mode, real_capital_mode_from_block,
    validate_open_delegation_config, DelegationDepositCapAccounting,
    DelegationForcedLiquidationPreview, DelegationInsurancePayoutPreview,
    DelegationInsurancePayoutRecipient, DelegationRedemptionCapAccounting,
    DelegationSlashDistribution,
};
pub use delegation::{
    delegation_action_fingerprint, delegation_action_id_for_submission, delegation_action_scope,
    delegation_action_signing_bytes, delegation_trading_account_id,
    derive_delegation_action_id_from_nonce, derive_delegation_id,
    derive_legacy_delegation_action_id, derive_redemption_request_id, vault_id_for_delegation,
    Delegation, DelegationCrystallization, DelegationDeposit, DelegationForcedLiquidation,
    DelegationId, DelegationInsurancePayout, DelegationRedemptionCancellation,
    DelegationRedemptionFinalization, DelegationRedemptionRequest, DelegationShare,
    DelegationStatus, DelegationTermination, ListingCaps, ListingControlState, MarginSnapshot,
    MarketFeeParameters, MarketRiskParameters, ProtocolDelegationCap,
    RealCapitalInsuranceWalletPayout, RealCapitalKeeperRewardCandidate, RealCapitalListing,
    RedemptionRequest, RedemptionRequestId, RedemptionRequestStatus, SignedDelegationAction,
    VaultId, VaultRealCapitalMode,
};
use ed25519_dalek::{Signature as Ed25519Signature, VerifyingKey};
pub use engine_handle::{
    ChainTailDomain, DagDomain, EngineHandle, EngineLockProfileSnapshot, EventsDomain,
    HeadSnapshot, LockDomain, LockOrderGuard, LockOrderTracker, LockTimingSamples,
    MahiProducerSnapshot, MarketStateDomain, MempoolDomain, PacemakerDomain, PolicyDomain,
};
use engine_runtime::*;
pub use engine_runtime::{signed_session_payload_for_intent, verify_intent_signature};
use engine_runtime_helpers::*;
pub use errors::{component_name, NodeError, VerifyIntentError};
pub use history::{
    all_equity_samples_from_txn, all_mandate_conformance_samples_from_txn,
    decode_equity_history_key, decode_mandate_conformance_history_key,
    equity_history_account_prefix, equity_history_key, mandate_conformance_history_key,
    mandate_conformance_history_prefix, put_equity_sample, put_mandate_conformance_sample,
    range_equity_samples_from_txn, range_mandate_conformance_samples_from_txn, AccountEquitySample,
    MandateConformanceSample, ACCOUNT_EQUITY_HISTORY_ACTIVE_TAIL_MS,
    ACCOUNT_EQUITY_HISTORY_DAILY_WINDOW_MS, ACCOUNT_EQUITY_HISTORY_DENSE_WINDOW_MS,
    ACCOUNT_EQUITY_HISTORY_IDLE_SAMPLE_MS, ACCOUNT_EQUITY_HISTORY_KEY_LEN,
    MANDATE_CONFORMANCE_HISTORY_KEY_LEN, MANDATE_CONFORMANCE_HISTORY_LIMIT,
};
use history::{AccountEquitySampleChangeKey, MandateConformanceSampleChangeKey};
#[cfg(test)]
pub(crate) use identity::vendor_root_payload_to_block;
pub(crate) use identity::{
    account_id_event_value, agent_identity_tier_downgraded, agent_identity_tier_rank,
    agent_kind_event_value, attestation_claim_code_hash, attestation_claim_matches_mandate,
    attestation_is_active, derive_vendor_root_id, execution_attestation_vendor_roots_state,
    execution_tee_vendor, identity_agent_kind, identity_attestation_claim,
    identity_attestation_kind, identity_attestation_revocation_reason,
    identity_attestation_vendor_for_tee, identity_tier_status, identity_tier_transition_reason,
    launch_vendor_cap_profile, launch_vendor_cap_profiles, tee_vendor_from_block,
    tee_verifier_outcome_matches_claim, vendor_root_payload_from_block,
    vendor_root_payload_matches_vendor, vendor_root_payload_within_bounds, IdentityNonceNamespace,
    VendorRootReverseIndexKey, VendorRootReverseIndexValue,
};
pub use identity::{
    derive_attestation_record_id, derive_tee_attestation_record_id, identity_action_signing_bytes,
    max_tee_quote_bytes, real_capital_governance_action_fingerprint,
    real_capital_governance_action_id_for_submission, tee_attestation_signing_bytes,
    tee_cap_multiplier_decimal_string, tee_governance_action_fingerprint,
    tee_governance_action_id_for_submission, AgentIdentityRecord, AgentIdentityTier, AgentKind,
    AttestationClaim, AttestationEvidence, AttestationKind, AttestationRecord, AttestationRecordId,
    AttestationRevocationReason, DeleteAgentIdentityInput, DescriptiveUpdate, GovernanceActionId,
    IdentityAction, IdentityActionKind, OperatorAttestationActionKind,
    RefreshOperatorAttestationInput, RefreshTeeAttestationInput, RegisterAgentIdentityInput,
    RegisterOperatorAttestationInput, RegisterTeeAttestationInput, RevokeAttestationInput,
    SignedIdentityAction, SignedTeeAttestationAction, TeeAttestationAction,
    TeeAttestationActionKind, TeeAttestationSigners, TeeVendor, TierTransitionReason,
    UpdateAgentIdentityInput, VendorCapProfile, VendorRoot, VendorRootId, VendorRootPayload,
    ATTESTATION_QUOTE_FRESHNESS_BLOCKS, MAX_AGENT_IDENTITY_CONTACT_URI_BYTES,
    MAX_AGENT_IDENTITY_DESCRIPTION_BYTES, MAX_AGENT_IDENTITY_DISPLAY_NAME_BYTES,
    MAX_PENDING_REAL_CAPITAL_GOVERNANCE_ACTIONS, MAX_PENDING_TEE_GOVERNANCE_ACTIONS,
    MAX_TEE_QUOTE_BYTES_AMD_SEV_SNP, MAX_TEE_QUOTE_BYTES_APPLE_SECURE_ENCLAVE,
    MAX_TEE_QUOTE_BYTES_AWS_NITRO, MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP,
    OPERATOR_ATTESTATION_MAX_EXPIRY_MS, OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
    REAL_CAPITAL_GOVERNANCE_ACTION_RECEIPT_RETENTION_MS,
    REAL_CAPITAL_GOVERNANCE_SUBMISSION_TIMESTAMP_WINDOW_MS, TEE_ATTESTATION_MAX_EXPIRY_MS,
    TEE_ATTESTATION_MIN_EXPIRY_MS, TEE_CAP_MULTIPLIER_APPLE_SECURE_ENCLAVE_UNITS,
    TEE_CAP_MULTIPLIER_BASELINE_UNITS, TEE_CAP_MULTIPLIER_SCALE_UNITS,
    TEE_GOVERNANCE_ACTION_RECEIPT_RETENTION_MS, TEE_GOVERNANCE_NOTICE_BLOCKS,
    VENDOR_ROOT_NOTICE_BLOCKS,
};
use intent_pool::{
    intent_status_from_event, intent_status_from_queue, rejection_context_from_node_error,
    rejection_reason_from_policy_error,
};
pub(crate) use intent_types::{
    ActivationOrderKey, BlockAssemblyPlan, IntentOrderKey, ProposerReadyFrontier, RejectedIntent,
};
pub use intent_types::{ConditionalIntentPool, DirectIntentMempool, ProposerFrontierStats};
use kombat_chain_consensus::{
    block_hash, body_root, build_timeout_certificate, delegation_block_action_kind,
    intent_expires_at_ms, intent_id, intent_not_before_ms, intent_signing_hash, mahi::MahiBlock,
    market_maker_program_block_action_kind, quorum_certificate_signing_bytes,
    real_capital_governance_block_action_kind, tee_governance_block_action_kind,
    timeout_message_signing_bytes, validate_block_against_parent,
    validate_block_against_parent_and_membership, validate_quorum_certificate,
    validate_quorum_certificate_with_membership, validate_timeout_message, Block, BlockBody,
    BlockDelegation, BlockDelegationAction, BlockDelegationFaultReason, BlockDelegationSlashPolicy,
    BlockFaucet, BlockFunding, BlockHeader, BlockItem, BlockLiquidation, BlockMarketMakerProgram,
    BlockMarketMakerProgramAction, BlockOraclePrice, BlockOracleValue, BlockRealCapitalGovernance,
    BlockRealCapitalGovernanceAction, BlockTeeGovernance, BlockTeeGovernanceAction,
    BlockValidationError, ConsensusPubKey, DelegationActionId, Hash, IntentId, IntentKind,
    IntentOrderKind, IntentOrderSide, IntentTimeInForce, MarketMakerIdentity,
    MarketMakerProgramActionId, PacemakerConfig, PacemakerState, PolicyTermsWire,
    QuorumCertificate, RealCapitalGovernanceActionId, Signature, SignedIntent, SignedTransaction,
    TeeGovernanceActionId, TimeoutMessage, ValidatorId, ValidatorVerifier,
};
use kombat_chain_execution::{
    account_evm_address_state_key, attestation_vendor_roots_state_key, decode_raw_evm_transaction,
    evm_address_account_state_key, evm_usdc_balance_state_key, evm_usdc_total_supply_state_key,
    isolated_margin_market_custody_address, policy_usdc_balance_state_key,
    verify_attestation_quote, AttestationVerifyOutcome, AttestationVerifyRequest, BlockError,
    BlockExecutor, EvmLogRecord, EvmTransactionExecution, ExecutionContext, ExecutionPipeline,
    PrecompileOutput, PreparedBlock,
};
use kombat_chain_matching::{
    pre_match_check, validate_order_request, BookLevel, MatchFill, MatchOutcome, MatchStatus,
    OrderKind as MatchingOrderKind, OrderRejectReason, OrderRequest,
    OrderSide as MatchingOrderSide, RestingBatchError, RestingOrder, SingleMarketBook,
};
#[cfg(test)]
use kombat_chain_network::ValidatorPeer;
use kombat_chain_network::{bootstrap_validator, ValidatorSet};
use kombat_chain_oracle::{
    aggregate_median_price, funding_index_delta, indicative_funding_rate_ppm, premium_rate_ppm,
    price_status, OracleAggregationConfig, OracleConfig, OraclePrice, OracleSample,
    PriceFeedStatus,
};
use kombat_chain_policy::{
    AccountError, AccountStore, OwnerCall, PolicyAdmission, PolicyError, PolicyExecutionScope,
    PolicyOrigin, PolicyRecord, PolicyState, PolicyStore, PolicyTerms, Principal, SessionPayload,
    SignedOwnerCall, SignedSessionPayload, StandardAgentAccount,
};
use kombat_chain_state_db::{
    CheckpointableStateStore, InMemoryCheckpoint, InMemoryStateStore, StateView,
};
#[cfg(test)]
use kombat_chain_streams::MarketLifecycleStatus;
use kombat_chain_streams::{
    AccountUpdateAction, DelegationStatusReason, EventStream, IdentityAttestationVendor,
    InsuranceFundUpdateSource, IntentAdmissionClass, IntentPendingReason, IntentRejectionReason,
    IntentStatus, IntentStatusStage, LiquidationEscalationAction, LiquidationExecutionOutcome,
    MandateEnforcementAction, OrderFillRole, OrderFillSide, OrderLifecycleStatus,
    OrderRejectionReason, RealCapitalInsuranceWalletSpendSource, RiskTransitionSource,
    RuntimeEventKind, RuntimeEventRecord,
};
#[cfg(test)]
pub(crate) use local_cluster::LocalClusterNodeSnapshot;
pub use local_cluster::{LocalCluster, LocalClusterConfig, LocalClusterNode};
pub(crate) use mandate::{
    aggregate_objective_scores, compile_mandate_policies,
    mandate_constraint_index_for_policy_error, mandate_objective_window_ms,
    mandate_state_event_name, mandate_trade_count_window_ms, mandate_u256_to_u128,
    validate_mandate_record, MandateObjectiveEvaluation, MandateObjectiveEventState,
};
pub use mandate::{
    derive_mandate_id, mandate_action_body_hash, mandate_action_signing_bytes, ConformanceState,
    CreateMandateInput, Mandate, MandateAction, MandateActionKind, MandateConstraint, MandateId,
    MandateObjective, MandateSessionKeyRef, MandateState, MandateU256, SignedMandateAction,
};
pub(crate) use market_maker::{
    aggregate_market_maker_compliance, evaluate_market_maker_enrollment_transition,
    market_maker_compliance_history_retention_ms, market_maker_enrollment_key,
    market_maker_enrollment_status_is_live, market_maker_enrollment_status_is_multiplier_eligible,
    market_maker_enrollment_status_selection_priority, market_maker_identity_economic_account,
    market_maker_identity_for_economic_account, market_maker_identity_signer_account,
    market_maker_program_action_program_id, market_maker_spread_bps,
    next_market_maker_observation_slot, validate_market_maker_enrollment_record,
    validate_market_maker_program_input, validate_market_maker_program_record,
};
pub use market_maker::{
    derive_legacy_market_maker_program_action_id, derive_market_maker_program_action_id_from_nonce,
    derive_market_maker_program_id, market_maker_program_action_fingerprint,
    market_maker_program_action_id_for_submission, market_maker_program_action_signing_bytes,
    DefineMarketMakerProgramInput, EnrollMarketMakerInput, MarketMakerBreachKind,
    MarketMakerComplianceState, MarketMakerEnrollment, MarketMakerEnrollmentKey,
    MarketMakerEnrollmentStatus, MarketMakerObservationSample, MarketMakerProgram,
    MarketMakerProgramId, MarketMakerProgramStatus, MarketMakerU256,
    SignedMarketMakerProgramAction,
};
pub(crate) use market_state::{
    market_status_event, oracle_aggregation_error_reason, oracle_feeder_allowed,
    oracle_missing_feeder_count, unrealized_pnl, validate_perpetual_market_config,
};
pub use market_state::{
    MarketKind, MarketStatus, OracleFeedAggregation, OracleFeedSample, OracleFeedSubmission,
    PerpetualMarket, PerpetualMarketConfig, PositionLedger,
};
pub(crate) use pending_actions::{
    pending_delegation_action_order, pending_faucet_grant_order,
    pending_market_maker_program_action_order, pending_real_capital_governance_action_order,
    pending_tee_governance_action_order, remove_pending_delegation_index_order,
    PendingDelegationActionOrder, PendingFaucetGrantOrder, PendingMarketMakerProgramActionOrder,
    PendingRealCapitalGovernanceActionOrder, PendingTeeGovernanceActionOrder,
};
pub use pending_actions::{
    DelegationActionReceipt, DelegationActionSubmissionStatus, FaucetGrantSubmissionStatus,
    MarketMakerProgramActionReceipt, MarketMakerProgramActionSubmissionStatus,
    PendingDelegationAction, PendingFaucetGrant, PendingMarketMakerProgramAction,
    PendingRealCapitalGovernanceAction, PendingTeeGovernanceAction, ProcessedDelegationAction,
    ProcessedMarketMakerProgramAction, ProcessedRealCapitalGovernanceAction,
    ProcessedTeeGovernanceAction, RealCapitalGovernanceActionReceipt,
    RealCapitalGovernanceActionSubmissionStatus, ScheduledTeeVendorCapProfileUpdate,
    TeeGovernanceActionReceipt, TeeGovernanceActionSubmissionStatus,
};
use revm::primitives::{Address, U256};
pub use runtime_types::{
    BlockEventCursor, EventSeqRange, EvmTransactionReceiptRecord, IntentAdmissionPreview,
    IsolatedCollateralAuditRow, IsolatedCollateralRepairAdjustment, IsolatedCollateralRepairReport,
    IsolatedCollateralRepairTarget, LiquidationPassReport, LiquidationPassStep,
    LiquidationSizingSuggestion, MarketCustodyAuditMarket, MarketCustodyAuditReport,
    MarketCustodyRepairAdjustment, MarketCustodyRepairReport, OrderInitialMarginPreview,
    PreparedCommittedBlock, SingleNodeConfig,
};
use sha2::{Digest, Sha256};
#[cfg(test)]
pub(crate) use snapshots::PersistedQuarantineReason;
pub use snapshots::SingleNodeEngineSnapshot;
pub(crate) use snapshots::{
    write_snapshot_file, PersistedQuarantinedBlock, PersistedSingleNodeEngineSnapshot,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use telemetry::ConsensusTelemetry;
pub use telemetry::{ConsensusLogEntry, ConsensusLogEventKind, ConsensusMetricsSnapshot};

pub const CRATE_NAME: &str = "node";
const BPS_DENOMINATOR: u128 = 10_000;
const RESTING_ORDER_COMMIT_BATCH_MAX: usize = 16_384;
const DEFAULT_MAINTENANCE_MARGIN_BPS: u32 = 1_000;
const VALIDATOR_REWARD_SPLIT_BPS: u128 = 4_000;
pub const DEFAULT_ORACLE_MIN_SOURCES: u8 = 2;
pub const DEFAULT_ORACLE_MIN_CONFIDENCE: u8 = 50;
pub const DEFAULT_ORACLE_MIN_SAMPLES: u16 = 3;
pub const MAX_ORACLE_FEEDERS: usize = 8;
pub const DEFAULT_ORACLE_FEEDER_COUNT: u8 = 4;
pub const DEFAULT_ORACLE_FEEDERS: [[u8; 32]; MAX_ORACLE_FEEDERS] = [
    [1; 32], [2; 32], [3; 32], [4; 32], [0; 32], [0; 32], [0; 32], [0; 32],
];
const ORACLE_FEED_RETAINED_TIMESTAMPS: usize = 64;
const MARKET_MAKER_REBATE_SPLIT_BPS: u128 = 3_000;
const INSURANCE_FUND_SPLIT_BPS: u128 = 1_000;
const LIQUIDATION_INSURANCE_SPLIT_BPS: u128 = 5_000;
const LIQUIDATION_LIQUIDATOR_SPLIT_BPS: u128 = 3_000;
const LIQUIDATION_STALL_FREEZE_THRESHOLD: u8 = 2;
const CONSENSUS_LIQUIDATOR_ACCOUNT_ID: u64 = u64::MAX;
const CONSENSUS_LIQUIDATION_CLIENT_ORDER_ID_BASE: u64 = u64::MAX;
const MAX_CONSENSUS_LIQUIDATIONS_PER_BLOCK: usize = 4;
// RFC 027 governance defaults. Mirrors the table in
// docs/v1/rfcs/027-formal-market-maker-program.md §"Governance parameters".
pub const MAX_MM_PROGRAMS: usize = 64;
pub const MAX_MM_PROGRAM_MARKETS: usize = 64;
pub const MAX_ENROLLMENTS_PER_MARKET_MAKER: usize = 256;
pub const MAX_ENROLLMENTS_PER_MARKET: usize = 32;
pub const PROGRAM_BOND_MIN_MICRO_USDC: u128 = 10_000_000_000;
pub const PROGRAM_GRACE_WINDOW_MS: u64 = 3_600 * 1_000;
pub const PROGRAM_REENTER_COOLDOWN_MS: u64 = 24 * 3_600 * 1_000;
pub const COMPLIANCE_WINDOW_MAX_MS: u64 = 24 * 3_600 * 1_000;
pub const OBSERVATION_CADENCE_MIN_MS: u64 = 10_000;
pub const OBSERVATION_CADENCE_MAX_MS: u64 = 3_600 * 1_000;
pub const REBATE_MULTIPLIER_BASE_UNITS: u16 = 10_000;
pub const REBATE_MULTIPLIER_CAP_UNITS: u16 = 30_000;
pub const MARKET_MAKER_PROGRAM_ACTION_RECEIPT_RETENTION_MS: u64 = 30 * 86_400 * 1_000;
pub const MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS: usize = 1_024;
pub const MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS_PER_SIGNER: usize = 64;
// RFC 026 governance defaults. Mirrors the table in
// docs/v1/rfcs/026-performance-share-delegation.md §"Governance parameters".
pub const PERFORMANCE_SHARE_CAP_UNITS: u16 = 5_000;
pub const AGENT_BOND_MIN: u128 = 5_000_000_000;
pub const CRYSTALLIZATION_CADENCE_MIN_MS: u64 = 7 * 86_400 * 1_000;
pub const CRYSTALLIZATION_CADENCE_MAX_MS: u64 = 365 * 86_400 * 1_000;
pub const NOTICE_PERIOD_MIN_MS: u64 = 24 * 3_600 * 1_000;
pub const NOTICE_PERIOD_MAX_MS: u64 = 30 * 86_400 * 1_000;
pub const MAX_DELEGATIONS_PER_AGENT_LIVE: usize = 8;
pub const MAX_ACTIVE_DELEGATIONS: usize = 2_048;
pub const DELEGATION_MIN_DEPOSIT: u128 = 100_000_000;
pub const WINDING_GRACE_WINDOW_MS: u64 = 30 * 86_400 * 1_000;
pub const DELEGATION_ACTION_RECEIPT_RETENTION_MS: u64 = 30 * 86_400 * 1_000;
pub const MAX_PENDING_DELEGATION_ACTIONS: usize = 4_096;
pub const MAX_PENDING_DELEGATION_ACTIONS_PER_SIGNER: usize = 64;
pub const SHARE_SCALE_FACTOR: u128 = 1_000_000;
const PERFORMANCE_SHARE_DENOMINATOR: u128 = BPS_DENOMINATOR;
pub const LIQUIDATION_DEADLINE_MS: u64 = 24 * 3_600 * 1_000;
pub const MAX_MANDATE_CONSTRAINTS: usize = 32;
pub const MAX_MANDATE_OBJECTIVES: usize = 16;
pub const MAX_MANDATE_SESSION_KEYS: usize = 16;
pub const MAX_MANDATE_DESCRIPTION_URI_BYTES: usize = 256;
pub const MAX_MANDATE_SET_SIZE: usize = 64;
pub const MAX_ACTIVE_MANDATES_PER_AGENT: usize = 8;
pub const MAX_NON_TERMINAL_MANDATES_PER_AGENT: usize = 16;
pub const MIN_MANDATE_CONFORMANCE_WINDOW_MS: u64 = 86_400_000;
pub const MAX_MANDATE_CONFORMANCE_WINDOW_MS: u64 = 365 * 86_400_000;
const UTC_DAY_MS: u64 = 86_400_000;
const MAHI_LEGACY_EXEC_ENV: &str = "KOMBAT_MAHI_LEGACY_EXEC";
const MAHI_EXEC_PROFILE_ENV: &str = "KOMBAT_MAHI_EXEC_PROFILE";
const PHASE6_MARKET_SHARDED_RESTING_BATCH_ENV: &str = "KOMBAT_PHASE6_MARKET_SHARDED_RESTING_BATCH";
const PHASE6_WORKER_MARKET_MATCHING_ENV: &str = "KOMBAT_PHASE6_WORKER_MARKET_MATCHING";
const PHASE6_INPLACE_RESTING_COMMIT_ENV: &str = "KOMBAT_PHASE6_INPLACE_RESTING_COMMIT";

fn parse_env_bool_strict(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn env_bool_with_default(name: &str, default: bool) -> Result<bool, NodeError> {
    match std::env::var(name) {
        Ok(value) => parse_env_bool_strict(&value).ok_or_else(|| NodeError::Persistence {
            reason: format!(
                "{name} must be a boolean value: true values are 1,true,yes,on; false values are 0,false,no,off"
            ),
        }),
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(NodeError::Persistence {
            reason: format!("{name} must be a valid UTF-8 boolean value"),
        }),
    }
}

#[derive(Clone, Debug)]
pub struct SingleNodeEngine {
    configured_validator_verifiers: Vec<ValidatorVerifier>,
    validator_set: Vec<ValidatorId>,
    validator_verifiers: Vec<ValidatorVerifier>,
    consensus_signing_keys: BTreeMap<ValidatorId, [u8; 32]>,
    pacemaker_config: PacemakerConfig,
    pacemaker: PacemakerState,
    chain: Vec<Block>,
    executor: BlockExecutor<InMemoryStateStore>,
    account_store: AccountStore,
    policy_store: PolicyStore,
    order_books: BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: BTreeMap<(u64, u64, u64), MandateId>,
    markets: BTreeMap<u64, PerpetualMarket>,
    market_marks: BTreeMap<u64, OraclePrice>,
    market_index_prices: BTreeMap<u64, OraclePrice>,
    oracle_feed_submissions: BTreeMap<(u64, u64, [u8; 32]), OracleFeedSubmission>,
    mandates: BTreeMap<MandateId, Mandate>,
    mandates_by_agent: BTreeMap<u64, BTreeSet<MandateId>>,
    mandate_action_nonces: BTreeMap<Principal, u64>,
    mandate_fill_timestamps: BTreeMap<MandateId, Vec<u64>>,
    mandate_objective_event_cache: BTreeMap<(MandateId, u16), MandateObjectiveEventState>,
    mandate_realized_pnl: BTreeMap<MandateId, i128>,
    mandate_daily_pnl_baselines: BTreeMap<(MandateId, u64), i128>,
    account_operators: BTreeMap<u64, u64>,
    agent_identities: BTreeMap<u64, AgentIdentityRecord>,
    agent_identity_tombstones: BTreeSet<u64>,
    identity_action_nonces: BTreeMap<u64, u64>,
    identity_attestor_nonces: BTreeMap<u64, u64>,
    agent_attestations: BTreeMap<AttestationRecordId, AttestationRecord>,
    agent_attestations_by_account: BTreeMap<u64, BTreeSet<AttestationRecordId>>,
    tee_attestations_by_vendor_root:
        BTreeMap<VendorRootReverseIndexKey, VendorRootReverseIndexValue>,
    // TODO(state pruning): cap retained inactive vendor-root audit entries once
    // RFC 051 governance retention semantics are settled.
    tee_vendor_roots: BTreeMap<TeeVendor, Vec<VendorRoot>>,
    tee_vendor_cap_profiles: BTreeMap<TeeVendor, VendorCapProfile>,
    market_maker_programs: BTreeMap<MarketMakerProgramId, MarketMakerProgram>,
    market_maker_enrollments: BTreeMap<MarketMakerEnrollmentKey, MarketMakerEnrollment>,
    live_market_maker_bonds_micro_usdc: u128,
    processed_market_maker_program_actions:
        BTreeMap<MarketMakerProgramActionId, ProcessedMarketMakerProgramAction>,
    pending_market_maker_program_actions:
        BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>,
    pending_market_maker_program_action_order: BTreeSet<PendingMarketMakerProgramActionOrder>,
    processed_tee_governance_actions: BTreeMap<TeeGovernanceActionId, ProcessedTeeGovernanceAction>,
    pending_tee_governance_actions: BTreeMap<TeeGovernanceActionId, PendingTeeGovernanceAction>,
    pending_tee_governance_action_order: BTreeSet<PendingTeeGovernanceActionOrder>,
    scheduled_tee_vendor_cap_profile_updates:
        BTreeMap<TeeGovernanceActionId, ScheduledTeeVendorCapProfileUpdate>,
    processed_real_capital_governance_actions:
        BTreeMap<RealCapitalGovernanceActionId, ProcessedRealCapitalGovernanceAction>,
    pending_real_capital_governance_actions:
        BTreeMap<RealCapitalGovernanceActionId, PendingRealCapitalGovernanceAction>,
    pending_real_capital_governance_action_order: BTreeSet<PendingRealCapitalGovernanceActionOrder>,
    pending_faucet_grants: BTreeMap<[u8; 20], PendingFaucetGrant>,
    pending_faucet_grant_order: BTreeSet<PendingFaucetGrantOrder>,
    delegations: BTreeMap<DelegationId, Delegation>,
    delegation_shares: BTreeMap<(DelegationId, u64), DelegationShare>,
    real_capital_listings: BTreeMap<VaultId, RealCapitalListing>,
    real_capital_listings_by_agent: BTreeMap<u64, BTreeSet<VaultId>>,
    vault_deposited_micro_usdc: BTreeMap<VaultId, u128>,
    vault_principal_deposited_micro_usdc: BTreeMap<(VaultId, u64), u128>,
    // RFC 049 audit state mirrored into deterministic EVM escrow addresses.
    // The Rust ledger remains the consensus-facing accounting view while the
    // policy-USDC state root carries the matching closed-loop balances.
    delegation_compensation_claims: BTreeMap<(DelegationId, u64), u128>,
    delegation_termination_payout_weights: BTreeMap<(DelegationId, u64), u128>,
    real_capital_bond_escrow_micro_usdc: BTreeMap<DelegationId, u128>,
    real_capital_compensation_escrow_micro_usdc: u128,
    real_capital_insurance_wallet_micro_usdc: u128,
    // Gross append-only accounting for real-capital bond amounts assigned to
    // insurance. Spend paths debit the deterministic insurance wallet escrow,
    // never this gross accumulator.
    real_capital_insurance_fund_micro_usdc: u128,
    protocol_delegation_cap: Option<ProtocolDelegationCap>,
    listing_control_state: ListingControlState,
    redemption_requests: BTreeMap<RedemptionRequestId, RedemptionRequest>,
    processed_delegation_actions: BTreeMap<DelegationActionId, ProcessedDelegationAction>,
    pending_delegation_actions: BTreeMap<DelegationActionId, PendingDelegationAction>,
    pending_delegation_action_order: BTreeSet<PendingDelegationActionOrder>,
    pending_open_delegation_index: BTreeMap<DelegationId, BTreeSet<PendingDelegationActionOrder>>,
    pending_open_delegation_agent_index: BTreeMap<u64, BTreeSet<PendingDelegationActionOrder>>,
    pending_open_delegation_order: BTreeSet<PendingDelegationActionOrder>,
    pending_finalize_delegation_index:
        BTreeMap<DelegationId, BTreeSet<PendingDelegationActionOrder>>,
    market_oracle_config: BTreeMap<u64, OracleConfig>,
    market_risk: BTreeMap<u64, MarketRiskParameters>,
    market_fees: BTreeMap<u64, MarketFeeParameters>,
    paused_markets: BTreeSet<u64>,
    market_funding_index: BTreeMap<u64, i128>,
    market_last_funding_ms: BTreeMap<u64, u64>,
    // Per-market margin/liquidation insurance fund. This is separate from
    // RFC 049's real_capital_insurance_fund_micro_usdc accumulator.
    insurance_funds: BTreeMap<u64, u128>,
    market_bad_debt_balances: BTreeMap<u64, u128>,
    account_market_bad_debt: BTreeMap<(u64, u64), u128>,
    fee_vaults: BTreeMap<u64, u128>,
    validator_reward_pools: BTreeMap<u64, u128>,
    market_maker_rebate_pools: BTreeMap<u64, u128>,
    treasury_balances: BTreeMap<u64, u128>,
    frozen_accounts: BTreeSet<u64>,
    under_maintenance_accounts: BTreeSet<(u64, u64)>,
    liquidation_escalation: BTreeMap<(u64, u64), LiquidationEscalationState>,
    isolated_collateral: BTreeMap<(u64, u64), i128>,
    positions: BTreeMap<(u64, u64), PositionLedger>,
    intent_mempool: DirectIntentMempool,
    conditional_pool: ConditionalIntentPool,
    direct_expiry_purged_through_ms: u64,
    conditional_expiry_purged_through_ms: u64,
    proposer_frontier: ProposerReadyFrontier,
    runtime_events: Vec<RuntimeEventRecord>,
    block_event_index: BTreeMap<u64, BlockEventCursor>,
    equity_history: BTreeMap<(u64, u64), AccountEquitySample>,
    latest_equity_sample_by_account: BTreeMap<u64, AccountEquitySample>,
    mandate_conformance_history: BTreeMap<(MandateId, u64, u64), MandateConformanceSample>,
    latest_conformance_sample_by_mandate: BTreeMap<MandateId, MandateConformanceSample>,
    evm_transaction_receipts: BTreeMap<Hash, EvmTransactionReceiptRecord>,
    next_runtime_event_seq: u64,
    telemetry: ConsensusTelemetry,
    suppress_fill_bankruptcy_reconcile: bool,
}

fn duration_nanos_u64(duration: std::time::Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), std::io::Error> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "snapshot path missing parent directory",
        )
    })?;
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LiquidationEscalationState {
    consecutive_non_improving_still_under: u8,
    last_margin_surplus: i128,
}

struct BuildBlockRequest<'a> {
    chain_id: u64,
    proposer: ValidatorId,
    parent: &'a Block,
    validator_verifiers: &'a [ValidatorVerifier],
    consensus_signing_keys: &'a BTreeMap<ValidatorId, [u8; 32]>,
    body: BlockBody,
    state_root: Hash,
    event_root: Hash,
    timestamp_ms: u64,
    round: u64,
    max_clock_skew_ms: u64,
}

fn validate_persisted_snapshot_block_link(
    block: &Block,
    parent: &Block,
    chain_validator_verifiers: &[ValidatorVerifier],
    expected_proposer: ValidatorId,
    max_clock_skew_ms: u64,
) -> Result<(), BlockValidationError> {
    let primary_validation = validate_block_against_parent_and_membership(
        block,
        &parent.header,
        chain_validator_verifiers,
        expected_proposer,
        block.header.timestamp_ms,
        max_clock_skew_ms,
    );
    let validation = match primary_validation {
        Ok(()) => Ok(()),
        Err(BlockValidationError::InsufficientQuorum) if block.header.qc.signatures.is_empty() => {
            validate_block_against_parent(
                block,
                &parent.header,
                expected_proposer,
                block.header.timestamp_ms,
                max_clock_skew_ms,
            )
        }
        Err(error) if chain_validator_verifiers.len() > 1 => chain_validator_verifiers
            .iter()
            .find(|verifier| verifier.validator == block.header.proposer)
            .ok_or(error)
            .and_then(|proposer_verifier| {
                validate_block_against_parent_and_membership(
                    block,
                    &parent.header,
                    std::slice::from_ref(proposer_verifier),
                    block.header.proposer,
                    block.header.timestamp_ms,
                    max_clock_skew_ms,
                )
            }),
        Err(error) => Err(error),
    };
    match validation {
        Ok(()) => Ok(()),
        Err(BlockValidationError::NonMonotonicTimestamp) => {
            validate_historical_native_mahi_block_link_allowing_timestamp_regression(
                block,
                parent,
                chain_validator_verifiers,
                expected_proposer,
            )
        }
        Err(error) => Err(error),
    }
}

fn validate_historical_native_mahi_block_link_allowing_timestamp_regression(
    block: &Block,
    parent: &Block,
    chain_validator_verifiers: &[ValidatorVerifier],
    expected_proposer: ValidatorId,
) -> Result<(), BlockValidationError> {
    // Compatibility for redb state produced by the early native-Mahi local
    // install path, which failed to enforce the legacy builder's timestamp
    // monotonicity check. New local native blocks are rejected before install;
    // hydration still preserves already-committed history after verifying every
    // other parent-link and QC invariant.
    if block.header.timestamp_ms >= parent.header.timestamp_ms {
        return Err(BlockValidationError::NonMonotonicTimestamp);
    }
    if block.header.chain_id != parent.header.chain_id {
        return Err(BlockValidationError::InvalidChainId);
    }
    if block.header.height != parent.header.height.saturating_add(1) {
        return Err(BlockValidationError::InvalidHeight);
    }
    if block.header.round <= parent.header.round {
        return Err(BlockValidationError::NonIncreasingRound);
    }
    if block.header.parent_hash != block_hash(parent) {
        return Err(BlockValidationError::InvalidParentHash);
    }
    if block.header.parent_state_root != parent.header.state_root {
        return Err(BlockValidationError::InvalidParentStateRoot);
    }
    if block.header.proposer != expected_proposer {
        return Err(BlockValidationError::UnexpectedProposer);
    }
    if block.header.qc.view != parent.header.round {
        return Err(BlockValidationError::InvalidQuorumCertificate);
    }

    match validate_quorum_certificate_with_membership(
        &block.header.qc,
        &block.header.parent_hash,
        chain_validator_verifiers,
    ) {
        Ok(()) => Ok(()),
        Err(BlockValidationError::InsufficientQuorum) if block.header.qc.signatures.is_empty() => {
            validate_quorum_certificate(&block.header.qc, &block.header.parent_hash)
        }
        Err(error) if chain_validator_verifiers.len() > 1 => chain_validator_verifiers
            .iter()
            .find(|verifier| verifier.validator == block.header.proposer)
            .ok_or(error)
            .and_then(|proposer_verifier| {
                validate_quorum_certificate_with_membership(
                    &block.header.qc,
                    &block.header.parent_hash,
                    std::slice::from_ref(proposer_verifier),
                )
            }),
        Err(error) => Err(error),
    }
}

impl SingleNodeEngine {
    fn purge_expired_direct_once(&mut self, timestamp_ms: u64) -> Vec<SignedIntent> {
        if timestamp_ms <= self.direct_expiry_purged_through_ms {
            return Vec::new();
        }
        self.direct_expiry_purged_through_ms = timestamp_ms;
        self.intent_mempool.purge_expired(timestamp_ms)
    }

    fn purge_expired_conditional_once(&mut self, timestamp_ms: u64) -> Vec<SignedIntent> {
        if timestamp_ms <= self.conditional_expiry_purged_through_ms {
            return Vec::new();
        }
        self.conditional_expiry_purged_through_ms = timestamp_ms;
        self.conditional_pool.purge_expired(timestamp_ms)
    }

    fn ensure_account_not_frozen(&self, account_id: u64) -> Result<(), NodeError> {
        if self.account_frozen(account_id) {
            return Err(NodeError::AccountFrozen { account_id });
        }
        Ok(())
    }

    fn ensure_market_accepts_new_orders(&self, market_id: u64) -> Result<(), NodeError> {
        ensure_market_accepts_new_orders(&self.markets, &self.paused_markets, market_id)
    }

    fn ensure_market_price_inputs_ready(
        &self,
        market_id: u64,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        if !market_price_inputs_ready_at(
            market_id,
            timestamp_ms,
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
        ) {
            return Err(NodeError::StaleMarketData { market_id });
        }
        Ok(())
    }

    pub fn new(config: SingleNodeConfig) -> Self {
        Self::new_with_validator_set(config, None)
    }

    pub fn new_with_validator_set(
        config: SingleNodeConfig,
        configured_validator_set: Option<&ValidatorSet>,
    ) -> Self {
        let genesis_proposer = configured_validator_set
            .and_then(|validator_set| validator_set.active_validators().first().cloned())
            .map(|validator| validator.validator_id)
            .unwrap_or(config.validator_id);
        let genesis = build_genesis_block(
            config.chain_id,
            genesis_proposer,
            config.genesis_state_root,
            config.genesis_timestamp_ms,
        );
        let local_validator_verifier = ValidatorVerifier {
            validator: config.validator_id,
            consensus_pubkey: consensus_pubkey_from_signing_key(config.consensus_signing_key),
        };
        let configured_validator_verifiers = configured_validator_set
            .map(|validator_set| {
                validator_verifiers(&validator_set.active_validators())
                    .expect("configured validator set should produce verifiers")
            })
            .unwrap_or_else(|| vec![local_validator_verifier]);
        let configured_peer_count = configured_validator_verifiers.len().saturating_sub(1);
        let validator_verifiers = vec![local_validator_verifier];
        let consensus_signing_keys =
            BTreeMap::from([(config.validator_id, config.consensus_signing_key)]);
        let genesis_qc = build_quorum_certificate(&genesis, &consensus_signing_keys)
            .expect("single-node validator signing key should exist");
        let pacemaker = PacemakerState::new(1, genesis_qc);
        let telemetry = ConsensusTelemetry::new(
            configured_peer_count,
            &pacemaker,
            config.genesis_timestamp_ms,
        );

        let mut engine = Self {
            configured_validator_verifiers,
            validator_set: vec![config.validator_id],
            validator_verifiers,
            consensus_signing_keys,
            pacemaker_config: config.pacemaker,
            pacemaker,
            chain: vec![genesis],
            executor: BlockExecutor::new(
                config.chain_id,
                InMemoryStateStore::with_committed_root(config.genesis_state_root),
            ),
            account_store: AccountStore::default(),
            policy_store: PolicyStore::default(),
            order_books: BTreeMap::new(),
            resting_order_mandates: BTreeMap::new(),
            markets: BTreeMap::new(),
            market_marks: BTreeMap::new(),
            market_index_prices: BTreeMap::new(),
            oracle_feed_submissions: BTreeMap::new(),
            mandates: BTreeMap::new(),
            mandates_by_agent: BTreeMap::new(),
            mandate_action_nonces: BTreeMap::new(),
            mandate_fill_timestamps: BTreeMap::new(),
            mandate_objective_event_cache: BTreeMap::new(),
            mandate_realized_pnl: BTreeMap::new(),
            mandate_daily_pnl_baselines: BTreeMap::new(),
            account_operators: BTreeMap::new(),
            agent_identities: BTreeMap::new(),
            agent_identity_tombstones: BTreeSet::new(),
            identity_action_nonces: BTreeMap::new(),
            identity_attestor_nonces: BTreeMap::new(),
            agent_attestations: BTreeMap::new(),
            agent_attestations_by_account: BTreeMap::new(),
            tee_attestations_by_vendor_root: BTreeMap::new(),
            tee_vendor_roots: BTreeMap::new(),
            tee_vendor_cap_profiles: launch_vendor_cap_profiles(),
            market_maker_programs: BTreeMap::new(),
            market_maker_enrollments: BTreeMap::new(),
            live_market_maker_bonds_micro_usdc: 0,
            processed_market_maker_program_actions: BTreeMap::new(),
            pending_market_maker_program_actions: BTreeMap::new(),
            pending_market_maker_program_action_order: BTreeSet::new(),
            processed_tee_governance_actions: BTreeMap::new(),
            pending_tee_governance_actions: BTreeMap::new(),
            pending_tee_governance_action_order: BTreeSet::new(),
            scheduled_tee_vendor_cap_profile_updates: BTreeMap::new(),
            processed_real_capital_governance_actions: BTreeMap::new(),
            pending_real_capital_governance_actions: BTreeMap::new(),
            pending_real_capital_governance_action_order: BTreeSet::new(),
            pending_faucet_grants: BTreeMap::new(),
            pending_faucet_grant_order: BTreeSet::new(),
            delegations: BTreeMap::new(),
            delegation_shares: BTreeMap::new(),
            real_capital_listings: BTreeMap::new(),
            real_capital_listings_by_agent: BTreeMap::new(),
            vault_deposited_micro_usdc: BTreeMap::new(),
            vault_principal_deposited_micro_usdc: BTreeMap::new(),
            delegation_compensation_claims: BTreeMap::new(),
            delegation_termination_payout_weights: BTreeMap::new(),
            real_capital_bond_escrow_micro_usdc: BTreeMap::new(),
            real_capital_compensation_escrow_micro_usdc: 0,
            real_capital_insurance_wallet_micro_usdc: 0,
            real_capital_insurance_fund_micro_usdc: 0,
            protocol_delegation_cap: None,
            listing_control_state: ListingControlState::default(),
            redemption_requests: BTreeMap::new(),
            processed_delegation_actions: BTreeMap::new(),
            pending_delegation_actions: BTreeMap::new(),
            pending_delegation_action_order: BTreeSet::new(),
            pending_open_delegation_index: BTreeMap::new(),
            pending_open_delegation_agent_index: BTreeMap::new(),
            pending_open_delegation_order: BTreeSet::new(),
            pending_finalize_delegation_index: BTreeMap::new(),
            market_oracle_config: BTreeMap::new(),
            market_risk: BTreeMap::new(),
            market_fees: BTreeMap::new(),
            paused_markets: BTreeSet::new(),
            market_funding_index: BTreeMap::new(),
            market_last_funding_ms: BTreeMap::new(),
            insurance_funds: BTreeMap::new(),
            market_bad_debt_balances: BTreeMap::new(),
            account_market_bad_debt: BTreeMap::new(),
            fee_vaults: BTreeMap::new(),
            validator_reward_pools: BTreeMap::new(),
            market_maker_rebate_pools: BTreeMap::new(),
            treasury_balances: BTreeMap::new(),
            frozen_accounts: BTreeSet::new(),
            under_maintenance_accounts: BTreeSet::new(),
            liquidation_escalation: BTreeMap::new(),
            isolated_collateral: BTreeMap::new(),
            positions: BTreeMap::new(),
            intent_mempool: DirectIntentMempool::default(),
            conditional_pool: ConditionalIntentPool::default(),
            direct_expiry_purged_through_ms: 0,
            conditional_expiry_purged_through_ms: 0,
            proposer_frontier: ProposerReadyFrontier::default(),
            runtime_events: Vec::new(),
            block_event_index: BTreeMap::new(),
            equity_history: BTreeMap::new(),
            latest_equity_sample_by_account: BTreeMap::new(),
            mandate_conformance_history: BTreeMap::new(),
            latest_conformance_sample_by_mandate: BTreeMap::new(),
            evm_transaction_receipts: BTreeMap::new(),
            next_runtime_event_seq: 0,
            telemetry,
            suppress_fill_bankruptcy_reconcile: false,
        };
        engine.rebuild_block_event_index();
        engine
    }

    pub fn validator_set(&self) -> &[ValidatorId] {
        &self.validator_set
    }

    pub fn local_validator_id(&self) -> Option<ValidatorId> {
        self.validator_set.first().copied()
    }

    pub fn local_consensus_signing_key(&self) -> Option<[u8; 32]> {
        let validator_id = self.local_validator_id()?;
        self.consensus_signing_keys.get(&validator_id).copied()
    }

    pub fn clear_local_validator_binding(&mut self) {
        self.validator_set.clear();
        self.validator_verifiers.clear();
        self.consensus_signing_keys.clear();
        self.telemetry.remap_local_validator_id(None);
    }

    pub fn configured_validator_ids(&self) -> Vec<ValidatorId> {
        self.configured_validator_verifiers
            .iter()
            .map(|verifier| verifier.validator)
            .collect()
    }

    pub fn configured_validator_consensus_pubkey(
        &self,
        validator_id: ValidatorId,
    ) -> Option<ConsensusPubKey> {
        self.configured_validator_verifiers
            .iter()
            .find(|verifier| verifier.validator == validator_id)
            .map(|verifier| verifier.consensus_pubkey)
    }

    pub fn validator_consensus_pubkey(&self, validator_id: ValidatorId) -> Option<ConsensusPubKey> {
        self.validator_verifiers
            .iter()
            .find(|verifier| verifier.validator == validator_id)
            .map(|verifier| verifier.consensus_pubkey)
    }

    pub fn rebind_local_validator(
        &mut self,
        validator_id: ValidatorId,
        consensus_signing_key: [u8; 32],
    ) -> Result<(), std::io::Error> {
        let consensus_pubkey = consensus_pubkey_from_signing_key(consensus_signing_key);
        let configured_verifier = self
            .configured_validator_verifiers
            .iter()
            .find(|verifier| verifier.validator == validator_id)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "configured validator id missing from bootstrapped snapshot",
                )
            })?;
        if configured_verifier.consensus_pubkey != consensus_pubkey {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "configured validator consensus key mismatch",
            ));
        }

        self.validator_set = vec![validator_id];
        self.validator_verifiers = vec![*configured_verifier];
        self.consensus_signing_keys = BTreeMap::from([(validator_id, consensus_signing_key)]);
        self.telemetry.remap_local_validator_id(Some(validator_id));
        self.telemetry
            .rewrite_peer_count(self.configured_validator_verifiers.len().saturating_sub(1));
        Ok(())
    }

    pub fn blocks(&self) -> &[Block] {
        &self.chain
    }

    pub fn head(&self) -> &Block {
        self.chain
            .last()
            .expect("single-node chain must have genesis")
    }

    pub fn chain_id(&self) -> u64 {
        self.head().header.chain_id
    }

    pub fn current_round(&self) -> u64 {
        self.pacemaker.current_round
    }

    pub fn round_timeout_ms(&self) -> u64 {
        self.pacemaker.round_timeout_ms(&self.pacemaker_config)
    }

    pub fn metrics(&self) -> &ConsensusMetricsSnapshot {
        self.telemetry.metrics()
    }

    fn record_phase6_resting_batch_stats(&mut self, stats: RestingBatchCommitStats) {
        if stats.attempts == 0 && stats.accepted == 0 && stats.items == 0 {
            return;
        }
        self.telemetry.on_phase6_resting_batch(
            stats.attempts,
            stats.accepted,
            stats.items,
            stats.unsupported,
            stats.rejected,
            stats.gather_ns,
            stats.precheck_ns,
            stats.admission_ns,
            stats.policy_ns,
            stats.matching_ns,
            stats.event_ns,
        );
    }

    pub fn policy_store(&self) -> &PolicyStore {
        &self.policy_store
    }

    pub fn policy_store_mut(&mut self) -> &mut PolicyStore {
        &mut self.policy_store
    }

    pub fn account_store(&self) -> &AccountStore {
        &self.account_store
    }

    pub fn account_store_mut(&mut self) -> &mut AccountStore {
        &mut self.account_store
    }

    // RFC 027 — Formal Market-Maker Program.

    pub fn intent_mempool(&self) -> &DirectIntentMempool {
        &self.intent_mempool
    }

    pub fn conditional_intent_pool(&self) -> &ConditionalIntentPool {
        &self.conditional_pool
    }

    pub fn proposer_frontier_stats(&self) -> ProposerFrontierStats {
        let tracked_policy_count = self.proposer_frontier.contiguous_by_policy.len();
        let mut tracked_ready_items = 0_usize;
        let mut largest_tracked_policy_ready_items = 0_usize;
        let mut stale_policy_count = 0_usize;
        let mut stale_ready_items = 0_usize;

        for keys in self.proposer_frontier.contiguous_by_policy.values() {
            tracked_ready_items = tracked_ready_items.saturating_add(keys.len());
            largest_tracked_policy_ready_items = largest_tracked_policy_ready_items.max(keys.len());
        }

        for policy_id in self.intent_mempool.policy_ids() {
            let expected_nonce = self.policy_store.next_intent_nonce(policy_id).unwrap_or(1);
            let actual_ready_items = self
                .intent_mempool
                .contiguous_policy_frontier_keys(policy_id, expected_nonce)
                .len();
            let tracked_items = self
                .proposer_frontier
                .contiguous_by_policy
                .get(&policy_id)
                .map_or(0, Vec::len);
            if actual_ready_items > tracked_items {
                stale_policy_count = stale_policy_count.saturating_add(1);
                stale_ready_items =
                    stale_ready_items.saturating_add(actual_ready_items - tracked_items);
            }
        }

        ProposerFrontierStats {
            direct_mempool_items: self.intent_mempool.len(),
            conditional_mempool_items: self.conditional_pool.len(),
            tracked_policy_count,
            tracked_ready_items,
            largest_tracked_policy_ready_items,
            stale_policy_count,
            stale_ready_items,
        }
    }

    pub fn runtime_events(&self) -> &[RuntimeEventRecord] {
        &self.runtime_events
    }

    pub fn block_event_cursor(&self, block_height: u64) -> Option<BlockEventCursor> {
        self.block_event_index.get(&block_height).copied()
    }

    pub fn next_runtime_event_seq(&self) -> u64 {
        self.next_runtime_event_seq
    }

    pub fn equity_history_samples(
        &self,
        account_id: u64,
        from_height: u64,
        before_height: u64,
    ) -> Vec<AccountEquitySample> {
        if from_height >= before_height {
            return Vec::new();
        }
        self.equity_history
            .range((account_id, from_height)..(account_id, before_height))
            .map(|(_, sample)| *sample)
            .collect()
    }

    pub fn all_equity_history_samples(&self) -> Vec<(u64, AccountEquitySample)> {
        self.equity_history
            .iter()
            .map(|((account_id, _), sample)| (*account_id, *sample))
            .collect()
    }

    pub fn set_equity_history_samples<I>(&mut self, samples: I)
    where
        I: IntoIterator<Item = (u64, AccountEquitySample)>,
    {
        self.equity_history.clear();
        for (account_id, sample) in samples {
            self.equity_history
                .insert((account_id, sample.block_height), sample);
        }
        self.rebuild_latest_equity_sample_index();
    }

    pub fn mandate_conformance_history_samples(
        &self,
        mandate_id: MandateId,
    ) -> Vec<MandateConformanceSample> {
        self.mandate_conformance_history
            .range((mandate_id, 0, 0)..=(mandate_id, u64::MAX, u64::MAX))
            .map(|(_, sample)| *sample)
            .collect()
    }

    pub fn all_mandate_conformance_history_samples(
        &self,
    ) -> Vec<(MandateId, MandateConformanceSample)> {
        self.mandate_conformance_history
            .iter()
            .map(|((mandate_id, _, _), sample)| (*mandate_id, *sample))
            .collect()
    }

    pub fn set_mandate_conformance_history_samples<I>(&mut self, samples: I)
    where
        I: IntoIterator<Item = (MandateId, MandateConformanceSample)>,
    {
        self.mandate_conformance_history.clear();
        for (mandate_id, sample) in samples {
            self.mandate_conformance_history.insert(
                (mandate_id, sample.evaluated_at_ms, sample.block_height),
                sample,
            );
        }
        self.rebuild_latest_mandate_conformance_sample_index();
    }

    pub fn latest_mandate_conformance_delta(&self, mandate_id: &MandateId) -> Option<i16> {
        self.latest_conformance_sample_by_mandate
            .get(mandate_id)
            .and_then(|sample| sample.delta)
    }

    pub fn seq_range_for_timestamps(&self, from_ms: u64, effective_to_ms: u64) -> EventSeqRange {
        let from_height = self.first_block_height_at_or_after_timestamp(from_ms);
        let before_height = self.first_block_height_at_or_after_timestamp(effective_to_ms);
        let from_seq = self.seq_lower_bound_for_height(from_height);
        let before_seq = self.seq_lower_bound_for_height(before_height);
        EventSeqRange {
            from_height,
            before_height,
            from_seq,
            before_seq,
        }
    }

    fn first_block_height_at_or_after_timestamp(&self, timestamp_ms: u64) -> u64 {
        self.chain
            .iter()
            // Historical native-Mahi redb state may contain small timestamp
            // regressions, so this cannot use `partition_point`.
            .find(|block| block.header.timestamp_ms >= timestamp_ms)
            .map(|block| block.header.height)
            .unwrap_or_else(|| self.head().header.height.saturating_add(1))
    }

    fn seq_lower_bound_for_height(&self, block_height: u64) -> u64 {
        if block_height > self.head().header.height {
            return self.next_runtime_event_seq;
        }
        self.block_event_index
            .get(&block_height)
            .map(|cursor| cursor.lower_bound_seq)
            .unwrap_or(self.next_runtime_event_seq)
    }

    pub fn evm_transaction_receipt(&self, tx_hash: &Hash) -> Option<&EvmTransactionReceiptRecord> {
        self.evm_transaction_receipts.get(tx_hash)
    }

    pub fn evm_transaction_raw_bytes(&self, tx_hash: &Hash) -> Option<Vec<u8>> {
        let receipt = self.evm_transaction_receipt(tx_hash)?;
        self.get_committed_state(
            format!("tx/{}/{}", receipt.block_number, receipt.transaction_index).as_bytes(),
        )
    }

    pub fn intent_status(&self, intent_id: IntentId) -> IntentStatus {
        let event_stream = EventStream::new(&self.runtime_events);
        let latest_event = event_stream.latest_intent_event(intent_id);

        if let Some(signed_intent) = self.conditional_pool.get_intent(intent_id) {
            let current_policy_nonce = self
                .policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .ok();
            let pending_reason =
                queue_pending_reason(self, signed_intent, IntentStatusStage::AcceptedConditional);
            return intent_status_from_queue(
                intent_id,
                IntentStatusStage::AcceptedConditional,
                signed_intent,
                current_policy_nonce,
                pending_reason,
                latest_event,
            );
        }

        if let Some(signed_intent) = self.intent_mempool.get_intent(intent_id) {
            let stage = match latest_event.and_then(|event| event.kind.intent_status_stage()) {
                Some(IntentStatusStage::Promoted) => IntentStatusStage::Promoted,
                _ => IntentStatusStage::AcceptedDirect,
            };
            let current_policy_nonce = self
                .policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .ok();
            let pending_reason = queue_pending_reason(self, signed_intent, stage);
            return intent_status_from_queue(
                intent_id,
                stage,
                signed_intent,
                current_policy_nonce,
                pending_reason,
                latest_event,
            );
        }

        if let Some(event) = latest_event {
            return intent_status_from_event(intent_id, event);
        }

        IntentStatus {
            intent_id,
            stage: IntentStatusStage::Unknown,
            rejection_reason: None,
            order_rejection_reason: None,
            pending_reason: None,
            next_eligible_at_ms: None,
            current_policy_nonce: None,
            policy_id: None,
            nonce: None,
            currently_queued: false,
            latest_event_seq: None,
            block_height: None,
            timestamp_ms: None,
        }
    }

    pub fn committed_state_root(&self) -> Hash {
        self.executor.committed_root()
    }

    pub fn quarantined_block_count(&self) -> usize {
        self.executor.quarantined_blocks().len()
    }

    pub fn get_committed_state(&self, key: &[u8]) -> Option<Vec<u8>> {
        let checkpoint = self.executor.state_store().checkpoint();
        checkpoint.get(key)
    }

    fn sync_tee_vendor_roots_execution_mirror(&mut self) {
        // Called from snapshot restore and committed RFC 051 vendor-root block
        // application; submission paths must never call this directly. This is
        // infallible for the in-memory execution mirror; panicking is preferable
        // to returning a rejected governance receipt after consensus roots have
        // already changed.
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        for vendor in TeeVendor::all() {
            let execution_vendor = execution_tee_vendor(vendor);
            let key = attestation_vendor_roots_state_key(execution_vendor);
            let roots = self
                .tee_vendor_roots
                .get(&vendor)
                .cloned()
                .unwrap_or_default();
            if roots.is_empty() {
                // The delete branch is dormant during normal snapshot restore,
                // where consensus roots and the mirror already agree. Governance
                // revocation exercises it for running engines.
                if self.get_committed_state(&key).is_some() {
                    deletes.push(key);
                }
                continue;
            }
            let state = execution_attestation_vendor_roots_state(vendor, &roots);
            sets.push((
                key,
                borsh::to_vec(&state).expect("tee vendor roots mirror should serialize"),
            ));
        }
        if sets.is_empty() && deletes.is_empty() {
            return;
        }
        self.apply_committed_state_updates(sets, deletes)
            .expect("tee vendor roots execution mirror should apply");
    }

    fn ensure_collateral_intent_policy_usdc_available(
        &self,
        signed_intent: &SignedIntent,
    ) -> Result<(), NodeError> {
        let (market_id, amount_usdc, settlement_side) = match &signed_intent.intent.kind {
            IntentKind::DepositCollateral {
                market_id,
                amount_usdc,
            } => (*market_id, *amount_usdc, CollateralIntentSide::Deposit),
            IntentKind::WithdrawCollateral {
                market_id,
                amount_usdc,
            } => (*market_id, *amount_usdc, CollateralIntentSide::Withdraw),
            _ => return Ok(()),
        };
        if amount_usdc == 0 {
            return Ok(());
        }
        let account_id = signed_intent.intent.account;
        let address = self
            .get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or(NodeError::InvalidCollateralMovement {
                market_id,
                account_id,
                reason: "account_evm_address_missing",
            })?;
        match settlement_side {
            CollateralIntentSide::Deposit => {
                let balance = self
                    .get_committed_state(&policy_usdc_balance_state_key(address))
                    .map(|bytes| {
                        <[u8; 32]>::try_from(bytes.as_slice())
                            .map(U256::from_be_bytes)
                            .map_err(|_| NodeError::InvalidCollateralMovement {
                                market_id,
                                account_id,
                                reason: "policy_usdc_balance_malformed",
                            })
                    })
                    .transpose()?
                    .unwrap_or(U256::ZERO);
                if balance < U256::from(amount_usdc) {
                    return Err(NodeError::InvalidCollateralMovement {
                        market_id,
                        account_id,
                        reason: "insufficient_policy_usdc_balance",
                    });
                }
            }
            CollateralIntentSide::Withdraw => {
                // RFC 053 v1 intentionally chooses the coarse market-wide
                // withdraw gate while any bad debt remains on the market.
                if self.market_bad_debt_balance(market_id) > 0 {
                    return Err(NodeError::InvalidCollateralMovement {
                        market_id,
                        account_id,
                        reason: "market_bad_debt_outstanding",
                    });
                }
                let custody_balance = self
                    .get_committed_state(&policy_usdc_balance_state_key(
                        isolated_margin_market_custody_address(market_id),
                    ))
                    .map(|bytes| {
                        <[u8; 32]>::try_from(bytes.as_slice())
                            .map(U256::from_be_bytes)
                            .map_err(|_| NodeError::InvalidCollateralMovement {
                                market_id,
                                account_id,
                                reason: "market_custody_balance_malformed",
                            })
                    })
                    .transpose()?
                    .unwrap_or(U256::ZERO);
                if custody_balance < U256::from(amount_usdc) {
                    return Err(NodeError::InvalidCollateralMovement {
                        market_id,
                        account_id,
                        reason: "insufficient_market_custody_balance",
                    });
                }
            }
        }
        Ok(())
    }

    pub fn apply_committed_state_updates<I, D>(
        &mut self,
        sets: I,
        deletes: D,
    ) -> Result<Hash, BlockError>
    where
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
        D: IntoIterator<Item = Vec<u8>>,
    {
        let new_root = self.executor.apply_state_updates(sets, deletes)?;
        if let Some(head) = self.chain.last_mut() {
            head.header.state_root = new_root;
        }
        Ok(new_root)
    }

    pub fn execute_contract_call(
        &self,
        caller: [u8; 20],
        to: [u8; 20],
        data: Vec<u8>,
        gas_limit: u64,
    ) -> Result<PrecompileOutput, BlockError> {
        let head = self.head();
        let context = ExecutionContext {
            chain_id: head.header.chain_id,
            height: head.header.height.saturating_add(1),
            timestamp_ms: head.header.timestamp_ms,
            parent_state_root: self.executor.committed_root(),
        };
        self.executor.execute_contract_call(
            &context,
            Address::from(caller),
            Address::from(to),
            data,
            gas_limit,
        )
    }

    pub fn simulate_contract_transaction(
        &self,
        caller: [u8; 20],
        to: [u8; 20],
        data: Vec<u8>,
        gas_limit: u64,
    ) -> Result<PrecompileOutput, BlockError> {
        let head = self.head();
        let context = ExecutionContext {
            chain_id: head.header.chain_id,
            height: head.header.height.saturating_add(1),
            timestamp_ms: head.header.timestamp_ms,
            parent_state_root: self.executor.committed_root(),
        };
        self.executor.simulate_contract_transaction(
            &context,
            Address::from(caller),
            Address::from(to),
            data,
            gas_limit,
        )
    }

    pub fn submit_raw_evm_transaction(
        &mut self,
        bytes: Vec<u8>,
        timestamp_ms: u64,
    ) -> Result<Hash, NodeError> {
        let decoded = decode_raw_evm_transaction(&bytes)
            .map_err(|reason| NodeError::Execution(BlockError::ExecutionRevert(reason)))?;
        let parent = self.head().clone();
        if timestamp_ms < parent.header.timestamp_ms {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::NonMonotonicTimestamp,
            ));
        }
        let proposer = self
            .local_validator_id()
            .or_else(|| self.validator_set.first().copied())
            .ok_or(NodeError::LocalValidatorUnavailable)?;
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        let body = BlockBody {
            items: vec![BlockItem::Transaction(SignedTransaction {
                signer: proposer,
                bytes,
            })],
        };
        if !env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)? {
            let result = self
                .executor
                .execute_and_commit_body_from_committed_root(
                    parent.header.height.saturating_add(1),
                    timestamp_ms,
                    block_hash(&parent),
                    &body,
                )
                .map_err(NodeError::Execution)?;
            self.install_native_local_block_state(
                &parent,
                round,
                proposer,
                body,
                self.policy_store.clone(),
                self.order_books.clone(),
                self.resting_order_mandates.clone(),
                self.isolated_collateral.clone(),
                Vec::new(),
                Vec::new(),
                result.evm_transactions,
                Some(result.event_root),
                timestamp_ms,
            )?;
            return Ok(decoded.tx_hash);
        }
        let qc = build_quorum_certificate(&parent, &self.consensus_signing_keys)?;
        let prepared_block = self
            .executor
            .prepare_block_with_checkpoint(&parent, round, proposer, qc, body, timestamp_ms)
            .map_err(NodeError::Execution)?;
        let prepared = self.prepared_commit_from_plan(
            prepared_block,
            BlockAssemblyPlan {
                body: BlockBody { items: vec![] },
                rejected: Vec::new(),
                policy_store_after: self.policy_store.clone(),
                order_book_overlays: BTreeMap::new(),
                resting_order_mandate_overlays: BTreeMap::new(),
                collateral_overlays: BTreeMap::new(),
                committed_events: Vec::new(),
            },
        );
        let produced_qc = build_quorum_certificate(prepared.block(), &self.consensus_signing_keys)?;
        self.install_prepared_block_state(prepared, produced_qc)?;
        Ok(decoded.tx_hash)
    }

    pub fn create_account(
        &mut self,
        account_id: u64,
        owner: [u8; 32],
        code_hash: Hash,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        self.account_store
            .insert(StandardAgentAccount::new(
                account_id,
                owner,
                timestamp_ms,
                code_hash,
            ))
            .map_err(NodeError::Account)?;
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AccountCreated { account_id, owner },
        );
        Ok(())
    }

    pub fn submit_owner_call(
        &mut self,
        signed_call: SignedOwnerCall,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let block_number = self.head().header.height.saturating_add(1);
        let account_id = signed_call.account_id;
        let call = signed_call.call.clone();
        let mut runtime_events = owner_call_runtime_events(&self.policy_store, account_id, &call)?;
        self.account_store
            .apply_signed_owner_call(
                &signed_call,
                self.chain_id(),
                block_number,
                timestamp_ms,
                &[],
                &mut self.policy_store,
            )
            .map_err(NodeError::Account)?;
        if let Some(event) = account_update_runtime_event(&self.account_store, account_id, &call)? {
            runtime_events.push(event);
        }
        for event in runtime_events {
            self.record_runtime_event(None, timestamp_ms, event);
        }
        Ok(())
    }

    pub fn submit_verified_owner_call(
        &mut self,
        account_id: u64,
        chain_id: u64,
        call: OwnerCall,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let block_number = self.head().header.height.saturating_add(1);
        let mut runtime_events = owner_call_runtime_events(&self.policy_store, account_id, &call)?;
        self.account_store
            .apply_verified_owner_call(
                account_id,
                chain_id,
                call.clone(),
                self.chain_id(),
                block_number,
                timestamp_ms,
                &[],
                &mut self.policy_store,
            )
            .map_err(NodeError::Account)?;
        if let Some(event) = account_update_runtime_event(&self.account_store, account_id, &call)? {
            runtime_events.push(event);
        }
        for event in runtime_events {
            self.record_runtime_event(None, timestamp_ms, event);
        }
        Ok(())
    }

    pub fn logs(&self) -> &[ConsensusLogEntry] {
        self.telemetry.logs()
    }

    pub fn submit_oracle_price_batch(
        &mut self,
        market_id: u64,
        samples: Vec<OracleFeedSample>,
        aggregation_config: OracleAggregationConfig,
        timestamp_ms: u64,
    ) -> Result<OracleFeedAggregation, NodeError> {
        if samples.is_empty() {
            return Err(NodeError::InvalidOracleSubmission {
                market_id,
                reason: "empty_oracle_submission",
            });
        }
        let oracle_feed_id = self.market_oracle_feed_id(market_id)?;
        let (oracle_feeder_count, oracle_feeder_allowlist) =
            self.market_oracle_feeder_allowlist(market_id)?;
        for sample in &samples {
            if sample.feeder_id == [0; 32] || sample.price_ticks == 0 || sample.confidence > 100 {
                return Err(NodeError::InvalidOracleSubmission {
                    market_id,
                    reason: "invalid_oracle_sample",
                });
            }
            if !oracle_feeder_allowed(
                oracle_feeder_count,
                &oracle_feeder_allowlist,
                &sample.feeder_id,
            ) {
                return Err(NodeError::InvalidOracleSubmission {
                    market_id,
                    reason: "unauthorized_oracle_feeder",
                });
            }
        }
        for sample in samples {
            self.oracle_feed_submissions.insert(
                (market_id, timestamp_ms, sample.feeder_id),
                OracleFeedSubmission {
                    market_id,
                    oracle_feed_id,
                    feeder_id: sample.feeder_id,
                    price_ticks: sample.price_ticks,
                    sources_used: sample.sources_used,
                    confidence: sample.confidence,
                    submitted_at_ms: timestamp_ms,
                },
            );
        }
        self.prune_oracle_feed_submissions(market_id, timestamp_ms);

        let submitted = self
            .oracle_feed_submissions
            .values()
            .filter(|submission| {
                submission.market_id == market_id && submission.submitted_at_ms == timestamp_ms
            })
            .copied()
            .collect::<Vec<_>>();
        let expected_feeders = usize::from(oracle_feeder_count);
        let missing_feeders =
            oracle_missing_feeder_count(oracle_feeder_count, &oracle_feeder_allowlist, &submitted);
        let aggregation = aggregate_median_price(
            submitted.iter().map(|submission| OracleSample {
                price_ticks: submission.price_ticks,
                sources_used: submission.sources_used,
                confidence: submission.confidence,
            }),
            aggregation_config,
        )
        .map_err(|error| NodeError::OracleAggregationFailed {
            market_id,
            reason: oracle_aggregation_error_reason(error),
        })?;

        self.update_market_index_price(market_id, aggregation.price_ticks, timestamp_ms);
        // Perp mark formation will eventually include order-book premium/TWAP; this MVP commits
        // the aggregated index as mark so margin gates have deterministic price inputs.
        self.update_market_mark_price(market_id, aggregation.price_ticks, timestamp_ms);
        Ok(OracleFeedAggregation {
            market_id,
            oracle_feed_id,
            price_ticks: aggregation.price_ticks,
            submitted_samples: submitted.len(),
            accepted_samples: aggregation.accepted_samples,
            rejected_samples: aggregation.rejected_samples,
            expected_feeders,
            missing_feeders,
            timestamp_ms,
        })
    }

    fn market_oracle_feed_id(&self, market_id: u64) -> Result<u64, NodeError> {
        if let Some(market) = self.markets.get(&market_id) {
            return Ok(market.oracle_feed_id);
        }
        if self.markets.is_empty() {
            return Ok(market_id);
        }
        Err(NodeError::UnknownMarket { market_id })
    }

    fn market_oracle_feeder_allowlist(
        &self,
        market_id: u64,
    ) -> Result<(u8, [[u8; 32]; MAX_ORACLE_FEEDERS]), NodeError> {
        if let Some(market) = self.markets.get(&market_id) {
            return Ok((
                market.config.oracle_feeder_count,
                market.config.oracle_feeder_allowlist,
            ));
        }
        if self.markets.is_empty() {
            return Ok((DEFAULT_ORACLE_FEEDER_COUNT, DEFAULT_ORACLE_FEEDERS));
        }
        Err(NodeError::UnknownMarket { market_id })
    }

    pub fn oracle_aggregation_config(
        &self,
        market_id: u64,
    ) -> Result<OracleAggregationConfig, NodeError> {
        if let Some(market) = self.markets.get(&market_id) {
            return Ok(OracleAggregationConfig {
                min_sources: market.config.oracle_min_sources,
                min_confidence: market.config.oracle_min_confidence,
                min_samples: usize::from(market.config.oracle_min_samples),
            });
        }
        if self.markets.is_empty() {
            return Ok(OracleAggregationConfig {
                min_sources: DEFAULT_ORACLE_MIN_SOURCES,
                min_confidence: DEFAULT_ORACLE_MIN_CONFIDENCE,
                min_samples: usize::from(DEFAULT_ORACLE_MIN_SAMPLES),
            });
        }
        Err(NodeError::UnknownMarket { market_id })
    }

    fn prune_oracle_feed_submissions(&mut self, market_id: u64, current_timestamp_ms: u64) {
        let timestamps = self
            .oracle_feed_submissions
            .keys()
            .filter_map(|(submission_market_id, timestamp_ms, _)| {
                (*submission_market_id == market_id).then_some(*timestamp_ms)
            })
            .collect::<BTreeSet<_>>();
        if timestamps.len() <= ORACLE_FEED_RETAINED_TIMESTAMPS {
            return;
        }

        let mut retained = timestamps
            .iter()
            .rev()
            .filter(|timestamp_ms| **timestamp_ms != current_timestamp_ms)
            .take(ORACLE_FEED_RETAINED_TIMESTAMPS.saturating_sub(1))
            .copied()
            .collect::<BTreeSet<_>>();
        retained.insert(current_timestamp_ms);
        self.oracle_feed_submissions
            .retain(|(submission_market_id, timestamp_ms, _), _| {
                *submission_market_id != market_id || retained.contains(timestamp_ms)
            });
    }
}

#[cfg(test)]
mod tests;
