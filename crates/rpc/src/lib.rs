mod codec;
mod eip712;
mod env_config;
mod event_records;
mod faucet;
mod http;
mod json_views;
mod json_views_delegation;
mod json_views_events;
mod json_views_identity;
mod jsonrpc;
mod mahi_helpers;
mod mahi_schedule;
mod metrics;
mod network_config;
mod network_wire;
mod node_service;
mod node_service_delegation;
mod node_service_market_risk;
mod peer_repair;
mod peer_transport;
mod request_auth;
mod response;
mod rpc_conversions;
mod rpc_errors;
mod rpc_handlers;
mod rpc_responses;
mod rpc_router;
mod rpc_types;
mod server;
mod server_dispatch;
mod server_forwarding;
mod server_mahi;
mod server_network;
mod server_network_process;
mod server_peer_transport;
mod server_submit;
mod sigverify;
mod streaming;
mod submit_queues;
mod vaults;
mod wire_json;
mod x402;

use crate::codec::{
    decode_borsh_hex, decode_hex_array, decode_hex_bytes, encode_borsh_hex, encode_hex,
    encode_hex_quantity_u256, encode_hex_quantity_u64, parse_hex_quantity_u64,
};
pub use crate::eip712::evm_owner_principal;
pub(crate) use crate::eip712::{
    evm_account_claim_eip712_digest, evm_address_from_owner_principal,
    evm_delegation_action_eip712_digest, evm_mandate_action_eip712_digest,
    evm_owner_call_eip712_digest, recover_evm_owner_call_address,
};
pub use crate::env_config::parse_mahi_commit_rule;
pub(crate) use crate::env_config::{
    legacy_delegation_bridge_enabled, local_bridge_mutations_enabled, mahi_accept_profile_enabled,
    mahi_allow_idle_empty_rounds, mahi_producer_profile_enabled, network_submit_forwarding_enabled,
    parse_env_bool_with_default, parse_env_u64_with_default,
    parse_mahi_merged_execution_batch_size, parse_mahi_number_of_leaders,
    parse_mahi_owner_relay_lag_rounds, parse_mahi_owner_relay_redundancy, parse_mahi_pipeline,
    parse_optional_env_u64, phase6_schedule_profile_enabled, phase6_schedule_profile_max_items,
    phase6_schedule_profile_sample_every, signed_delegation_gossip_enabled,
    signed_market_maker_program_gossip_enabled, DEFAULT_MAHI_MERGED_EXECUTION_BATCH_SIZE,
    LOCAL_BRIDGE_MUTATIONS_ENABLED_ENV, MAHI_ASYNC_BLOCK_BROADCAST_ENV,
    MAHI_MERGED_EXECUTION_BATCH_ENV, MAHI_OBSERVER_ASYNC_RELAY_ENV,
};
#[cfg(test)]
pub(crate) use crate::env_config::{
    LEGACY_DELEGATION_BRIDGE_ENABLED_ENV, MAHI_MULTILEADER_ENV, MAHI_OWNER_RELAY_LAG_ROUNDS_ENV,
    MAHI_OWNER_RELAY_REDUNDANCY_ENV, MAHI_PIPELINE_ENV, NETWORK_SUBMIT_FORWARDING_ENV,
    SIGNED_DELEGATION_GOSSIP_ENV, SIGNED_MARKET_MAKER_PROGRAM_GOSSIP_ENV,
};
pub(crate) use crate::event_records::*;
use crate::faucet::{
    sign_evm_usdc_deposit_for_tx, FaucetLimiter, FaucetOperatorSigner,
    DEFAULT_FAUCET_EVM_USDC_MICRO, DEFAULT_FAUCET_POLICY_USDC_MICRO, MAX_FAUCET_EVM_USDC_MICRO,
    MAX_FAUCET_POLICY_USDC_MICRO,
};
#[cfg(test)]
use crate::faucet::{FaucetLimiterMode, FAUCET_OPERATOR_PRIVATE_KEY_ENV};
pub use crate::http::TransportError;
use crate::http::{read_http_request, HttpRequest};
#[cfg(test)]
pub(crate) use crate::http::{MAX_HTTP_BODY_BYTES, MAX_HTTP_HEADER_BYTES};
pub(crate) use crate::json_views::*;
pub(crate) use crate::json_views_delegation::*;
pub(crate) use crate::json_views_events::*;
pub(crate) use crate::json_views_identity::*;
use crate::jsonrpc::serialize_response as serialize_json_rpc_response;
pub use crate::jsonrpc::{JsonRpcErrorObject, JsonRpcRequest, JsonRpcResponse};
pub(crate) use crate::mahi_helpers::{
    hash_bytes, mahi_payload_next_owner_id_for_policy, mahi_payload_owner_ids_for_policy,
    mahi_payload_primary_owner_id_for_policy, mahi_restart_boundary_round_from_head,
};
pub(crate) use crate::mahi_schedule::summarize_mahi_execution_schedule;
pub(crate) use crate::metrics::*;
pub(crate) use crate::network_config::*;
pub(crate) use crate::network_wire::*;
pub(crate) use crate::peer_repair::*;
pub(crate) use crate::peer_transport::*;
use crate::request_auth::{
    faucet_client_id, operator_request_is_authorized, peer_relay_request_is_authorized,
};
use crate::response::{
    header_map_from_axum, http_status_line, insert_browser_sse_headers, json_response,
    json_response_with_browser_headers, network_json_response, plain_response,
    transport_error_message, write_http_response, write_http_response_for_request,
    write_http_response_with_headers_for_request, write_sse_headers, JsonHttpResponse,
};
pub(crate) use crate::rpc_conversions::*;
pub(crate) use crate::rpc_errors::*;
pub(crate) use crate::rpc_handlers::*;
pub use crate::rpc_responses::*;
pub(crate) use crate::rpc_responses::{
    DelegationCommitOutcome, PeerFanoutFailure, RiskSnapshotListOptions, RiskSnapshotSupplement,
};
pub use crate::rpc_router::RpcRouter;
pub(crate) use crate::rpc_router::{account_response, policy_response};
pub use crate::rpc_types::*;
use crate::sigverify::{MahiSigverifyBatch, MahiSigverifyPool, MahiSigverifyResult};
use crate::streaming::{
    account_stream_payload_for_event, event_topic_name, market_stream_payload_for_event,
    parse_account_stream_request, parse_event_stream_request, parse_intent_id_query,
    parse_market_stream_request, parse_runtime_event_topic, parse_wallet_stream_request,
    resume_seq, write_sse_cursor, write_sse_event, write_sse_json_event, AxumAccountStreamQuery,
    AxumEventStreamQuery, AxumMarketStreamQuery, AxumWalletStreamQuery, EventStreamRequest,
};
pub(crate) use crate::submit_queues::*;
pub use crate::wire_json::*;
use crate::x402::{
    decode_header_value as decode_x402_header_value,
    encode_header_value as encode_x402_header_value,
    parse_config_from_env as parse_x402_config_from_env, payment_header as x402_payment_header,
    response_error as x402_response_error, response_success as x402_response_success,
    select_requirement as x402_select_requirement, X402_PAYMENT_REQUIRED_HEADER,
    X402_PAYMENT_RESPONSE_HEADER, X402_VERSION,
};
pub use crate::x402::{X402Config, X402PaymentRequired, X402PaymentRequirement, X402ResourceInfo};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use borsh::BorshSerialize;
use kombat_chain_consensus::{
    block_hash, build_timeout_certificate, canonical_delegation_block_action_kind,
    delegation_block_action_kind, intent_id,
    mahi::{
        detects_equivocation, BlockRef as MahiBlockRef, CommitDagView as MahiCommitDagView,
        CommitRule, DagState, LeaderSchedule, LeaderStatus as MahiLeaderStatus, Linearizer,
        MahiBlock, ThresholdClock, UniversalCommitterBuilder,
    },
    market_maker_program_block_action_kind, quorum_threshold,
    real_capital_governance_block_action_kind, validate_timeout_message, Block,
    BlockDelegationAction, BlockDelegationFaultReason, BlockIdentityTier,
    BlockMarketMakerProgramAction, BlockRealCapitalGovernanceAction, BlockVaultRealCapitalMode,
    DelegationActionId, Hash, Intent, IntentId, IntentKind, IntentOrderKind, IntentOrderSide,
    IntentPredicate, IntentTimeInForce, MarketMakerIdentity, MarketMakerProgramActionId,
    PolicyTermsWire, QuorumCertificate, RealCapitalGovernanceActionId, SignedIntent,
    TeeGovernanceActionId, TimeoutMessage, ValidatorId, ValidatorVerifier,
    DELEGATION_ACTION_KIND_BEGIN_WINDING, DELEGATION_ACTION_KIND_CANCEL_REDEMPTION,
    DELEGATION_ACTION_KIND_CLOSE, DELEGATION_ACTION_KIND_CLOSE_TO_DEPOSITS,
    DELEGATION_ACTION_KIND_DEPOSIT, DELEGATION_ACTION_KIND_FINALIZE_REDEMPTION,
    DELEGATION_ACTION_KIND_FORCED_LIQUIDATION, DELEGATION_ACTION_KIND_INSURANCE_PAYOUT,
    DELEGATION_ACTION_KIND_OPEN, DELEGATION_ACTION_KIND_REQUEST_REDEMPTION,
    DELEGATION_ACTION_KIND_TERMINATE, MARKET_MAKER_PROGRAM_ACTION_KIND_DEFINE,
    MARKET_MAKER_PROGRAM_ACTION_KIND_ENROLL, MARKET_MAKER_PROGRAM_ACTION_KIND_EXIT,
    MARKET_MAKER_PROGRAM_ACTION_KIND_RETIRE,
    REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DEPOSIT_ENABLED,
    REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DISCOVERY,
    REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_GLOBAL_KILL_SWITCH,
    REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_PROTOCOL_CAP,
    REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPSERT_LISTING,
};
use kombat_chain_execution::{
    account_evm_address_state_key, decode_raw_evm_transaction, evm_account_balance_state_key,
    evm_account_code_state_key, evm_account_nonce_state_key, evm_address_account_state_key,
    evm_usdc_balance_state_key, evm_usdc_minter_role_state_key, evm_usdc_system_address_bytes,
    evm_usdc_system_code_bytes, evm_usdc_total_supply_state_key, policy_usdc_balance_state_key,
    protocol_precompiles, BlockExecutor, EvmLogRecord, ExecutionContext, PrecompileDescriptor,
};
use kombat_chain_matching::{
    BookLevel, MatchFill, MatchOutcome, MatchStatus, OrderKind, OrderRejectReason, OrderRequest,
    OrderSide, RestingOrder, TimeInForce,
};
use kombat_chain_network::{BootstrapPeers, ValidatorConfig, ValidatorPeer, ValidatorSet};
use kombat_chain_node::{
    assemble_quorum_certificate, delegation_action_fingerprint,
    delegation_action_id_for_submission, delegation_trading_account_id, derive_delegation_id,
    derive_mandate_id, derive_market_maker_program_id, derive_redemption_request_id,
    market_maker_program_action_id_for_submission,
    real_capital_governance_action_id_for_submission, select_leader_for_validator_set,
    tee_cap_multiplier_decimal_string, AccountEquitySample, AgentIdentityRecord, AgentIdentityTier,
    AgentKind, AttestationClaim, AttestationEvidence, AttestationKind, AttestationRecord,
    CreateMandateInput, DefineMarketMakerProgramInput, Delegation, DelegationActionReceipt,
    DelegationActionSubmissionStatus, DelegationDeposit, DelegationId,
    DelegationRedemptionCancellation, DelegationRedemptionFinalization,
    DelegationRedemptionRequest, DelegationStatus, DeleteAgentIdentityInput, DescriptiveUpdate,
    EngineHandle, IdentityAction, LockDomain, Mandate, MandateAction, MandateConformanceSample,
    MandateConstraint, MandateId, MandateObjective, MandateState, MandateU256, MarketFeeParameters,
    MarketMakerBreachKind, MarketMakerEnrollment, MarketMakerEnrollmentStatus, MarketMakerProgram,
    MarketMakerProgramActionReceipt, MarketMakerProgramActionSubmissionStatus,
    MarketMakerProgramStatus, MarketMakerU256, MarketRiskParameters, MarketStatus, NodeError,
    OracleFeedAggregation, OracleFeedSample, OrderInitialMarginPreview, PerpetualMarketConfig,
    RealCapitalGovernanceActionReceipt, RealCapitalGovernanceActionSubmissionStatus,
    RealCapitalListing, RedemptionRequest, RedemptionRequestId, RedemptionRequestStatus,
    RefreshOperatorAttestationInput, RefreshTeeAttestationInput, RegisterAgentIdentityInput,
    RegisterOperatorAttestationInput, RegisterTeeAttestationInput, RevokeAttestationInput,
    ScheduledTeeVendorCapProfileUpdate, SignedDelegationAction, SignedIdentityAction,
    SignedMandateAction, SignedMarketMakerProgramAction, SignedTeeAttestationAction,
    SingleNodeEngine, TeeAttestationAction, TeeAttestationSigners, TeeVendor,
    UpdateAgentIdentityInput, VaultRealCapitalMode, VendorCapProfile, VendorRoot,
    DEFAULT_ORACLE_FEEDERS, DEFAULT_ORACLE_FEEDER_COUNT, DEFAULT_ORACLE_MIN_CONFIDENCE,
    DEFAULT_ORACLE_MIN_SAMPLES, DEFAULT_ORACLE_MIN_SOURCES, MANDATE_CONFORMANCE_HISTORY_LIMIT,
    MAX_ORACLE_FEEDERS, SHARE_SCALE_FACTOR,
};
use kombat_chain_oracle::OracleConfig;
use kombat_chain_policy::{
    AccountError, OwnerAction, OwnerCall, PolicyError, PolicyExecutionScope, PolicyRecord,
    PolicyState, Principal, SignedOwnerCall,
};
use kombat_chain_state_db::{CheckpointableStateStore, StateView, SyncMode};
use kombat_chain_streams::{
    AccountUpdateAction, DelegationLifecycleStatus, DelegationStatusReason, EventFilter, EventPage,
    EventStream, IdentityAgentKind, IdentityAttestationClaim, IdentityAttestationKind,
    IdentityAttestationRevocationReason, IdentityAttestationVendor, IdentityTierStatus,
    IdentityTierTransitionReason, IntentAdmissionClass, IntentPendingReason, IntentRejectionReason,
    IntentStatusStage, LiquidationEscalationAction, LiquidationExecutionOutcome,
    MarketLifecycleStatus, OrderFillRole, OrderFillSide, OrderLifecycleStatus,
    OrderRejectionReason, RealCapitalListingMode, RuntimeEventKind, RuntimeEventRecord,
    RuntimeEventTopic, StreamQuery,
};
use reqwest::blocking::Client;
use revm::primitives::U256;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::cmp::Reverse;
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque};
use std::env;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Builder as TokioRuntimeBuilder;

pub const CRATE_NAME: &str = "rpc";
const MAHI_ANCESTOR_QUORUM_SELF_HEAL_TIMEOUT: Duration = Duration::from_secs(30);
const MAHI_EXECUTION_BATCH_SIZE: usize = 32;
const NETWORK_RELAY_HEADER: &str = "x-and-network-relay";
const NETWORK_CREATE_ACCOUNT_RELAY: &str = "create-account";
const NETWORK_OWNER_CALL_RELAY: &str = "owner-call";
const NETWORK_DELEGATION_RELAY: &str = "delegation-action";
const NETWORK_SIGNED_DELEGATION_RELAY: &str = "signed-delegation-action";
const NETWORK_SIGNED_MARKET_MAKER_PROGRAM_RELAY: &str = "signed-market-maker-program-action";
const DELEGATION_ACTION_WAIT_MAX_MS: u64 = 60_000;
const DELEGATION_ACTION_WAIT_POLL_MS: u64 = 25;
const MAX_DELEGATION_IDEMPOTENCY_EVENT_SCAN: usize = 10_000;
const NETWORK_SUBMIT_INTENT_FORWARD: &str = "submit-intent-forward";
const NETWORK_SUBMIT_INTENT_RELAY: &str = "submit-intent-relay";
const NETWORK_FORWARD_LEADER_STALE_TOLERANCE_ROUNDS: u64 = 1;
const MAX_LOCAL_SUBMIT_QUEUE_ITEMS: usize = 65_536;
const LOCAL_SUBMIT_DRAIN_BATCH_ITEMS: usize = 32;
const LOCAL_SUBMIT_WORKER_IDLE_SLEEP: Duration = Duration::from_millis(1);
const LOCAL_SUBMIT_QUIESCE_TIMEOUT: Duration = Duration::from_secs(1);
const DURABLE_RPC_WRITE_DOMAINS: &[LockDomain] = &[
    LockDomain::Policy,
    LockDomain::Dag,
    LockDomain::Mempool,
    LockDomain::MarketState,
    LockDomain::ChainTail,
    LockDomain::Pacemaker,
    LockDomain::Events,
];
const MAX_INTENT_RELAY_BATCH_ITEMS: usize = 4096;
const MAX_SIGNED_DELEGATION_RELAY_ITEMS: usize = 32;
const MAX_SIGNED_MARKET_MAKER_PROGRAM_RELAY_ITEMS: usize = 32;
const MAX_ACCOUNT_PNL_WINDOW_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_ACCOUNT_PNL_WINDOW_EVENTS: usize = 200_000;
const MAX_ACCOUNT_EQUITY_HISTORY_LIMIT: usize = 1024;
const DEFAULT_ACCOUNT_EQUITY_HISTORY_LIMIT: usize = 512;
const DEFAULT_MANDATE_CONFORMANCE_HISTORY_LIMIT: usize = 80;

#[derive(Clone, Debug, Default)]
struct TimeoutMessagePool {
    rounds: BTreeMap<u64, BTreeMap<ValidatorId, TimeoutMessage>>,
}

#[derive(Debug)]
struct MahiState {
    epoch: u64,
    leader_schedule: LeaderSchedule,
    validator_verifiers: BTreeMap<ValidatorId, ValidatorVerifier>,
    dag: DagState,
    threshold_clock: ThresholdClock,
    commit_rule: CommitRule,
    pipeline: bool,
    number_of_leaders: usize,
    last_decided_leader: MahiBlockRef,
    decided_leaders: Vec<MahiCommittedLeader>,
    linearizer: Linearizer,
    pending_execution: VecDeque<MahiBlock>,
    inflight_intents: BTreeSet<IntentId>,
    inflight_faucets: BTreeSet<[u8; 20]>,
    inflight_delegation_actions: BTreeSet<DelegationActionId>,
    inflight_market_maker_program_actions: BTreeSet<MarketMakerProgramActionId>,
    inflight_tee_governance_actions: BTreeSet<TeeGovernanceActionId>,
    inflight_real_capital_governance_actions: BTreeSet<RealCapitalGovernanceActionId>,
    inflight_policy_nonces: BTreeMap<u64, BTreeSet<u64>>,
    restart_boundary_round: u64,
    next_local_round: u64,
    last_produced: Option<MahiBlockRef>,
    producer_attempts: u64,
    producer_skip_ancestor_quorum: u64,
    producer_skip_peer_reachability: u64,
    producer_skip_snapshot_unavailable: u64,
    producer_skip_empty_idle: u64,
    producer_skip_round_race: u64,
    producer_ancestor_wait: Option<MahiAncestorWait>,
    round_first_arrival_at: BTreeMap<u64, Instant>,
    round_latest_arrival_at: BTreeMap<u64, Instant>,
    producer_initial_lock_wait_ms: Vec<u64>,
    producer_initial_lock_hold_ms: Vec<u64>,
    producer_snapshot_ms: Vec<u64>,
    producer_pre_admitted_drain_ms: Vec<u64>,
    producer_final_lock_wait_ms: Vec<u64>,
    producer_final_lock_hold_ms: Vec<u64>,
    producer_total_ms: Vec<u64>,
    inbound_block_decode_ms: Vec<u64>,
    inbound_block_accept_ms: Vec<u64>,
    inbound_block_sigverify_enqueue_ms: Vec<u64>,
    inbound_block_total_ms: Vec<u64>,
    commit_executor_wake_count: u64,
    commit_executor_empty_wake_count: u64,
    commit_execution_batches: u64,
    commit_execution_blocks: u64,
    commit_execution_intents: u64,
    commit_execution_batch_ms: Vec<u64>,
    phase6_schedule_profiled_batches: u64,
    phase6_schedule_truncated_batches: u64,
    phase6_schedule_total_item_count: Vec<u64>,
    phase6_schedule_item_count: Vec<u64>,
    phase6_schedule_layer_count: Vec<u64>,
    phase6_schedule_max_layer_width: Vec<u64>,
    phase6_schedule_conflict_pair_count: Vec<u64>,
    phase6_schedule_parallelism_milli: Vec<u64>,
    produced_blocks: u64,
    produced_empty_blocks: u64,
    produced_intents: u64,
    received_blocks: u64,
    received_intents: u64,
    executed_blocks: u64,
    executed_intents: u64,
    verification_status: BTreeMap<MahiBlockRef, MahiBlockVerificationStatus>,
    dropped_blocks: u64,
    last_update_at: Option<Instant>,
}

#[derive(Debug)]
struct MahiAncestorWait {
    target_round: u64,
    quorum: usize,
    available: usize,
    since: Instant,
}

#[derive(Clone, Copy, Debug)]
struct MahiProducerTimingSample {
    initial_lock_wait: Duration,
    initial_lock_hold: Duration,
    snapshot: Duration,
    pre_admitted_drain: Duration,
    final_lock_wait: Duration,
    final_lock_hold: Duration,
    total: Duration,
}

#[derive(BorshSerialize)]
struct MahiDagSnapshotDigest {
    block_refs: Vec<MahiBlockRef>,
    pending_fetch: Vec<MahiBlockRef>,
    committed_refs: Vec<MahiBlockRef>,
    last_decided_leader: MahiBlockRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MahiStateSnapshot {
    enabled: bool,
    epoch: u64,
    commit_rule: &'static str,
    pipeline: bool,
    number_of_leaders: usize,
    next_round: u64,
    next_leader: ValidatorId,
    block_count: usize,
    max_round: u64,
    pending_fetch_count: usize,
    decided_leader_count: usize,
    committed_leader_count: usize,
    skipped_leader_count: usize,
    committed_block_count: usize,
    last_decided_round: u64,
    last_decided_author: ValidatorId,
    restart_boundary_round: u64,
    commit_log_hash: Hash,
    producer_attempts: u64,
    producer_skip_ancestor_quorum: u64,
    producer_skip_peer_reachability: u64,
    producer_skip_snapshot_unavailable: u64,
    producer_skip_empty_idle: u64,
    producer_skip_round_race: u64,
    producer_ancestor_wait_round: Option<u64>,
    producer_ancestor_wait_available: Option<u64>,
    producer_ancestor_wait_quorum: Option<u64>,
    producer_ancestor_wait_age_ms: Option<u64>,
    producer_ancestor_wait_first_arrival_age_ms: Option<u64>,
    producer_ancestor_wait_latest_arrival_age_ms: Option<u64>,
    producer_ancestor_wait_authors: Vec<ValidatorId>,
    producer_ancestor_wait_missing_authors: Vec<ValidatorId>,
    tail_round_block_counts: Vec<(u64, u64)>,
    producer_initial_lock_wait_ms: Vec<u64>,
    producer_initial_lock_hold_ms: Vec<u64>,
    producer_snapshot_ms: Vec<u64>,
    producer_pre_admitted_drain_ms: Vec<u64>,
    producer_final_lock_wait_ms: Vec<u64>,
    producer_final_lock_hold_ms: Vec<u64>,
    producer_total_ms: Vec<u64>,
    inbound_block_decode_ms: Vec<u64>,
    inbound_block_accept_ms: Vec<u64>,
    inbound_block_sigverify_enqueue_ms: Vec<u64>,
    inbound_block_total_ms: Vec<u64>,
    commit_executor_wake_count: u64,
    commit_executor_empty_wake_count: u64,
    commit_execution_batches: u64,
    commit_execution_blocks: u64,
    commit_execution_intents: u64,
    commit_execution_batch_ms: Vec<u64>,
    phase6_schedule_profiled_batches: u64,
    phase6_schedule_truncated_batches: u64,
    phase6_schedule_total_item_count: Vec<u64>,
    phase6_schedule_item_count: Vec<u64>,
    phase6_schedule_layer_count: Vec<u64>,
    phase6_schedule_max_layer_width: Vec<u64>,
    phase6_schedule_conflict_pair_count: Vec<u64>,
    phase6_schedule_parallelism_milli: Vec<u64>,
    produced_blocks: u64,
    produced_empty_blocks: u64,
    produced_intent_count: u64,
    received_blocks: u64,
    received_intent_count: u64,
    execution_pending_count: usize,
    executed_block_count: u64,
    executed_intent_count: u64,
    verification_pending_count: usize,
    verified_block_count: usize,
    verification_failed_count: usize,
    dropped_blocks: u64,
    sigverify_pool_depth: usize,
    sigverify_batch_hist: Vec<u64>,
    sigverify_per_intent_verified_total: u64,
    sigverify_failures_total: u64,
    last_produced_round: Option<u64>,
    last_produced_hash: Option<Hash>,
    /// Hash of validator-local MAHI DAG/liveness state. This is useful for
    /// debugging DAG sync, but it is not an application-state consensus hash.
    dag_snapshot_hash: Hash,
    last_update_age_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MahiBlockVerificationStatus {
    Pending,
    Verified,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
struct MahiCommittedLeader {
    round: u64,
    author: ValidatorId,
    decision: u8,
    hash: Hash,
}

#[derive(Debug, Default)]
struct MahiCommitExecutorControl {
    pending: Mutex<bool>,
    wake: Condvar,
    running: AtomicBool,
}

impl MahiState {
    fn new(
        epoch: u64,
        validator_verifiers: Vec<ValidatorVerifier>,
        commit_rule: CommitRule,
        pipeline: bool,
        number_of_leaders: usize,
        restart_boundary_round: u64,
    ) -> Option<Self> {
        let validators = validator_verifiers
            .iter()
            .map(|verifier| verifier.validator)
            .collect::<Vec<_>>();
        if validators.is_empty() {
            return None;
        }
        let number_of_leaders = number_of_leaders.max(1).min(validators.len());
        let restart_boundary_round = restart_boundary_round.max(1);
        // Mahi DAG state is process-local today. On a full offline
        // restart, resume consensus at durable_head.round + 1 and treat
        // that one round as the new local DAG genesis. The block executor
        // still preserves chain-round monotonicity through the durable head.
        Some(Self {
            epoch,
            leader_schedule: LeaderSchedule::new(validators),
            validator_verifiers: validator_verifiers
                .into_iter()
                .map(|verifier| (verifier.validator, verifier))
                .collect(),
            dag: DagState::default(),
            threshold_clock: ThresholdClock::new(restart_boundary_round),
            commit_rule,
            pipeline,
            number_of_leaders,
            last_decided_leader: MahiBlockRef {
                round: restart_boundary_round.saturating_sub(1),
                author: [0; 32],
                hash: [0; 32],
            },
            decided_leaders: Vec::new(),
            linearizer: Linearizer::new(),
            pending_execution: VecDeque::new(),
            inflight_intents: BTreeSet::new(),
            inflight_faucets: BTreeSet::new(),
            inflight_delegation_actions: BTreeSet::new(),
            inflight_market_maker_program_actions: BTreeSet::new(),
            inflight_tee_governance_actions: BTreeSet::new(),
            inflight_real_capital_governance_actions: BTreeSet::new(),
            inflight_policy_nonces: BTreeMap::new(),
            restart_boundary_round,
            next_local_round: restart_boundary_round,
            last_produced: None,
            producer_attempts: 0,
            producer_skip_ancestor_quorum: 0,
            producer_skip_peer_reachability: 0,
            producer_skip_snapshot_unavailable: 0,
            producer_skip_empty_idle: 0,
            producer_skip_round_race: 0,
            producer_ancestor_wait: None,
            round_first_arrival_at: BTreeMap::new(),
            round_latest_arrival_at: BTreeMap::new(),
            producer_initial_lock_wait_ms: Vec::new(),
            producer_initial_lock_hold_ms: Vec::new(),
            producer_snapshot_ms: Vec::new(),
            producer_pre_admitted_drain_ms: Vec::new(),
            producer_final_lock_wait_ms: Vec::new(),
            producer_final_lock_hold_ms: Vec::new(),
            producer_total_ms: Vec::new(),
            inbound_block_decode_ms: Vec::new(),
            inbound_block_accept_ms: Vec::new(),
            inbound_block_sigverify_enqueue_ms: Vec::new(),
            inbound_block_total_ms: Vec::new(),
            commit_executor_wake_count: 0,
            commit_executor_empty_wake_count: 0,
            commit_execution_batches: 0,
            commit_execution_blocks: 0,
            commit_execution_intents: 0,
            commit_execution_batch_ms: Vec::new(),
            phase6_schedule_profiled_batches: 0,
            phase6_schedule_truncated_batches: 0,
            phase6_schedule_total_item_count: Vec::new(),
            phase6_schedule_item_count: Vec::new(),
            phase6_schedule_layer_count: Vec::new(),
            phase6_schedule_max_layer_width: Vec::new(),
            phase6_schedule_conflict_pair_count: Vec::new(),
            phase6_schedule_parallelism_milli: Vec::new(),
            produced_blocks: 0,
            produced_empty_blocks: 0,
            produced_intents: 0,
            received_blocks: 0,
            received_intents: 0,
            executed_blocks: 0,
            executed_intents: 0,
            verification_status: BTreeMap::new(),
            dropped_blocks: 0,
            last_update_at: None,
        })
    }

    fn validator_count(&self) -> usize {
        self.validator_verifiers.len()
    }

    fn validators(&self) -> Vec<ValidatorId> {
        self.validator_verifiers.keys().copied().collect()
    }

    fn contains_author_round(&self, author: ValidatorId, round: u64) -> bool {
        self.dag
            .by_round
            .get(&round)
            .is_some_and(|entries| entries.contains_key(&author))
    }

    fn validator_verifier(&self, validator_id: ValidatorId) -> Option<&ValidatorVerifier> {
        self.validator_verifiers.get(&validator_id)
    }

    fn round_ancestors(&self, round: u64) -> Vec<MahiBlockRef> {
        if round <= self.restart_boundary_round {
            Vec::new()
        } else {
            self.dag.blocks_at_round(round - 1)
        }
    }

    fn validate_block_restart_boundary(&self, block: &MahiBlock) -> Result<(), String> {
        if block.round < self.restart_boundary_round {
            return Err(format!(
                "mahi block round {} below local restart boundary {}",
                block.round, self.restart_boundary_round
            ));
        }
        if block.round == self.restart_boundary_round {
            if block.ancestors.is_empty() {
                return Ok(());
            }
            return Err(format!(
                "mahi restart-boundary block round {} must not carry ancestors",
                block.round
            ));
        }
        if block
            .ancestors
            .iter()
            .any(|ancestor| ancestor.round < self.restart_boundary_round)
        {
            return Err(format!(
                "mahi block round {} references ancestor below local restart boundary {}",
                block.round, self.restart_boundary_round
            ));
        }
        block
            .validate_ancestor_rules(self.validator_count())
            .map_err(|error| format!("mahi block ancestor rules invalid: {error:?}"))
    }

    fn has_payload_backlog(&self) -> bool {
        self.produced_intents.saturating_add(self.received_intents) > self.executed_intents
            || self.pending_execution.iter().any(|block| {
                !block.intents.is_empty()
                    || !block.faucets.is_empty()
                    || !block.real_capital_governance.is_empty()
                    || !block.delegations.is_empty()
                    || !block.market_maker_programs.is_empty()
                    || !block.tee_governance.is_empty()
            })
    }

    fn accept_block(
        &mut self,
        block: MahiBlock,
        produced_locally: bool,
    ) -> Result<MahiBlockRef, String> {
        let Some(verifier) = self.validator_verifier(block.author) else {
            return Err(format!(
                "mahi block author 0x{} not in validator set",
                encode_hex(&block.author)
            ));
        };
        if block.epoch != self.epoch {
            return Err(format!(
                "mahi block epoch mismatch: expected {}, got {}",
                self.epoch, block.epoch
            ));
        }
        block
            .validate_author_signature(&verifier.consensus_pubkey)
            .map_err(|error| format!("mahi block signature invalid: {error:?}"))?;
        self.validate_block_restart_boundary(&block)?;
        if let Some(existing) = self
            .dag
            .by_round
            .get(&block.round)
            .and_then(|entries| entries.get(&block.author))
            .copied()
        {
            let reference = block.block_ref();
            if existing == reference {
                return Ok(existing);
            }
            if detects_equivocation(existing, reference) {
                return Err(format!(
                    "mahi equivocation detected for author=0x{} round={}",
                    encode_hex(&block.author),
                    block.round
                ));
            }
        }
        let block_round = block.round;
        let now = Instant::now();
        self.dag.note_missing_ancestors(&block);
        let reference = self.dag.insert_block(block);
        self.round_first_arrival_at
            .entry(block_round)
            .or_insert(now);
        self.round_latest_arrival_at.insert(block_round, now);
        self.verification_status
            .entry(reference)
            .or_insert(MahiBlockVerificationStatus::Pending);
        self.threshold_clock
            .observe_block(reference, self.validator_count());
        self.last_update_at = Some(now);
        if produced_locally {
            let intent_count = self.dag.blocks[&reference].intents.len() as u64;
            let payload_count = (self.dag.blocks[&reference].oracle_prices.len() as u64)
                .saturating_add(intent_count)
                .saturating_add(self.dag.blocks[&reference].faucets.len() as u64)
                .saturating_add(self.dag.blocks[&reference].real_capital_governance.len() as u64)
                .saturating_add(self.dag.blocks[&reference].delegations.len() as u64)
                .saturating_add(self.dag.blocks[&reference].market_maker_programs.len() as u64)
                .saturating_add(self.dag.blocks[&reference].tee_governance.len() as u64);
            for faucet in &self.dag.blocks[&reference].faucets {
                self.inflight_faucets.insert(faucet.address);
            }
            for governance in &self.dag.blocks[&reference].real_capital_governance {
                self.inflight_real_capital_governance_actions
                    .insert(governance.action_id);
            }
            for delegation in &self.dag.blocks[&reference].delegations {
                self.inflight_delegation_actions
                    .insert(delegation.action_id);
            }
            for program in &self.dag.blocks[&reference].market_maker_programs {
                self.inflight_market_maker_program_actions
                    .insert(program.action_id);
            }
            for governance in &self.dag.blocks[&reference].tee_governance {
                self.inflight_tee_governance_actions
                    .insert(governance.action_id);
            }
            for intent in self.dag.blocks[&reference].intents.iter() {
                self.inflight_intents.insert(intent_id(intent));
                self.inflight_policy_nonces
                    .entry(intent.intent.policy_id)
                    .or_default()
                    .insert(intent.intent.nonce);
            }
            self.last_produced = Some(reference);
            self.produced_blocks = self.produced_blocks.saturating_add(1);
            self.produced_intents = self.produced_intents.saturating_add(intent_count);
            if payload_count == 0 {
                self.produced_empty_blocks = self.produced_empty_blocks.saturating_add(1);
            }
        } else {
            self.received_blocks = self.received_blocks.saturating_add(1);
            self.received_intents = self
                .received_intents
                .saturating_add(self.dag.blocks[&reference].intents.len() as u64);
        }
        Ok(reference)
    }

    fn mark_block_verified(&mut self, reference: MahiBlockRef) {
        self.verification_status
            .insert(reference, MahiBlockVerificationStatus::Verified);
        self.last_update_at = Some(Instant::now());
    }

    fn mark_block_failed(&mut self, reference: MahiBlockRef) {
        self.verification_status
            .insert(reference, MahiBlockVerificationStatus::Failed);
        self.last_update_at = Some(Instant::now());
    }

    fn mark_block_dropped(&mut self) {
        self.dropped_blocks = self.dropped_blocks.saturating_add(1);
        self.last_update_at = Some(Instant::now());
    }

    fn record_inbound_block_timing(
        &mut self,
        decode: Duration,
        accept: Duration,
        sigverify_enqueue: Duration,
        total: Duration,
    ) {
        push_duration_sample(&mut self.inbound_block_decode_ms, decode);
        push_duration_sample(&mut self.inbound_block_accept_ms, accept);
        push_duration_sample(
            &mut self.inbound_block_sigverify_enqueue_ms,
            sigverify_enqueue,
        );
        push_duration_sample(&mut self.inbound_block_total_ms, total);
    }

    fn record_producer_timing(&mut self, sample: MahiProducerTimingSample) {
        push_duration_sample(
            &mut self.producer_initial_lock_wait_ms,
            sample.initial_lock_wait,
        );
        push_duration_sample(
            &mut self.producer_initial_lock_hold_ms,
            sample.initial_lock_hold,
        );
        push_duration_sample(&mut self.producer_snapshot_ms, sample.snapshot);
        push_duration_sample(
            &mut self.producer_pre_admitted_drain_ms,
            sample.pre_admitted_drain,
        );
        push_duration_sample(
            &mut self.producer_final_lock_wait_ms,
            sample.final_lock_wait,
        );
        push_duration_sample(
            &mut self.producer_final_lock_hold_ms,
            sample.final_lock_hold,
        );
        push_duration_sample(&mut self.producer_total_ms, sample.total);
    }

    fn mark_commit_executor_wake(&mut self) {
        self.commit_executor_wake_count = self.commit_executor_wake_count.saturating_add(1);
    }

    fn mark_commit_executor_empty_wake(&mut self) {
        self.commit_executor_empty_wake_count =
            self.commit_executor_empty_wake_count.saturating_add(1);
    }

    fn record_commit_execution_batch(&mut self, blocks: &[MahiBlock], duration: Duration) {
        self.commit_execution_batches = self.commit_execution_batches.saturating_add(1);
        self.commit_execution_blocks = self
            .commit_execution_blocks
            .saturating_add(blocks.len() as u64);
        self.commit_execution_intents = self.commit_execution_intents.saturating_add(
            blocks
                .iter()
                .map(|block| block.intents.len() as u64)
                .sum::<u64>(),
        );
        push_duration_sample(&mut self.commit_execution_batch_ms, duration);
        if phase6_schedule_profile_enabled()
            && self
                .commit_execution_batches
                .saturating_sub(1)
                .is_multiple_of(phase6_schedule_profile_sample_every())
        {
            let profile =
                summarize_mahi_execution_schedule(blocks, phase6_schedule_profile_max_items());
            let summary = profile.schedule;
            self.phase6_schedule_profiled_batches =
                self.phase6_schedule_profiled_batches.saturating_add(1);
            if profile.truncated {
                self.phase6_schedule_truncated_batches =
                    self.phase6_schedule_truncated_batches.saturating_add(1);
            }
            push_u64_sample(
                &mut self.phase6_schedule_total_item_count,
                profile.total_item_count as u64,
            );
            push_u64_sample(
                &mut self.phase6_schedule_item_count,
                summary.item_count as u64,
            );
            push_u64_sample(
                &mut self.phase6_schedule_layer_count,
                summary.layer_count as u64,
            );
            push_u64_sample(
                &mut self.phase6_schedule_max_layer_width,
                summary.max_layer_width as u64,
            );
            push_u64_sample(
                &mut self.phase6_schedule_conflict_pair_count,
                summary.conflict_pair_count as u64,
            );
            push_u64_sample(
                &mut self.phase6_schedule_parallelism_milli,
                summary.estimated_parallelism_milli,
            );
        }
    }

    fn block_for_ref(&self, reference: MahiBlockRef) -> Option<MahiBlock> {
        self.dag.blocks.get(&reference).cloned()
    }

    fn advance_commit_detector(&mut self) -> usize {
        let threshold_round = self.threshold_clock.round();
        let mut committer =
            UniversalCommitterBuilder::new(self.validators(), MahiCommitDagView::from(&self.dag))
                .with_commit_rule(self.commit_rule)
                .with_pipeline(self.pipeline)
                .with_number_of_leaders(self.number_of_leaders)
                .build();
        let sequence = committer.try_commit(self.last_decided_leader, threshold_round);
        if sequence.is_empty() {
            return 0;
        }

        let committed_refs: Vec<_> = sequence
            .iter()
            .filter_map(|status| match status {
                MahiLeaderStatus::Commit(reference) => Some(*reference),
                MahiLeaderStatus::Skip(..) | MahiLeaderStatus::Undecided(..) => None,
            })
            .collect();
        let committed = self.linearizer.handle_commit(&self.dag, committed_refs);
        let mut newly_queued = 0;
        for sub_dag in committed {
            for block in sub_dag.blocks {
                self.pending_execution.push_back(block);
                newly_queued += 1;
            }
        }

        for status in sequence {
            let (author, round, decision, hash) = match status {
                MahiLeaderStatus::Commit(reference) => {
                    self.last_decided_leader = reference;
                    (reference.author, reference.round, 1_u8, reference.hash)
                }
                MahiLeaderStatus::Skip(author, round) => {
                    self.last_decided_leader = MahiBlockRef {
                        round,
                        author,
                        hash: [0; 32],
                    };
                    (author, round, 2_u8, [0; 32])
                }
                MahiLeaderStatus::Undecided(..) => continue,
            };
            self.decided_leaders.push(MahiCommittedLeader {
                round,
                author,
                decision,
                hash,
            });
        }
        self.last_update_at = Some(Instant::now());
        newly_queued
    }

    fn drain_ready_execution(&mut self) -> Vec<MahiBlock> {
        let mut ready = Vec::new();
        loop {
            let Some(block) = self.pending_execution.front() else {
                break;
            };
            let status = self
                .verification_status
                .get(&block.block_ref())
                .copied()
                .unwrap_or(MahiBlockVerificationStatus::Pending);
            match status {
                MahiBlockVerificationStatus::Verified => {
                    ready.push(
                        self.pending_execution
                            .pop_front()
                            .expect("front block should exist"),
                    );
                }
                MahiBlockVerificationStatus::Pending | MahiBlockVerificationStatus::Failed => break,
            }
        }
        ready
    }

    fn drain_ready_execution_for_batch(&mut self, execution_batch_size: usize) -> Vec<MahiBlock> {
        let execution_batch_size = execution_batch_size.max(1);
        if execution_batch_size == 1 {
            return self.drain_ready_execution();
        }

        let mut verified = 0_usize;
        for block in &self.pending_execution {
            let status = self
                .verification_status
                .get(&block.block_ref())
                .copied()
                .unwrap_or(MahiBlockVerificationStatus::Pending);
            if status != MahiBlockVerificationStatus::Verified {
                break;
            }
            verified = verified.saturating_add(1);
        }

        let executable = verified - (verified % execution_batch_size);
        let mut ready = Vec::with_capacity(executable);
        for _ in 0..executable {
            if let Some(block) = self.pending_execution.pop_front() {
                ready.push(block);
            }
        }
        ready
    }

    fn default_execution_batch_size(&self) -> usize {
        if env::var(MAHI_MERGED_EXECUTION_BATCH_ENV)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .is_some()
        {
            return parse_mahi_merged_execution_batch_size();
        }
        if self.leader_schedule.validator_count() > 1 {
            DEFAULT_MAHI_MERGED_EXECUTION_BATCH_SIZE
        } else {
            1
        }
    }

    fn requeue_execution_front(&mut self, blocks: Vec<MahiBlock>) {
        for block in blocks.into_iter().rev() {
            self.pending_execution.push_front(block);
        }
        self.last_update_at = Some(Instant::now());
    }

    fn mark_block_executed(&mut self, block_ref: MahiBlockRef) {
        if let Some(block) = self.block_for_ref(block_ref) {
            self.executed_intents = self
                .executed_intents
                .saturating_add(block.intents.len() as u64);
            for intent in block.intents {
                self.inflight_intents.remove(&intent_id(&intent));
                if let Entry::Occupied(mut entry) =
                    self.inflight_policy_nonces.entry(intent.intent.policy_id)
                {
                    entry.get_mut().remove(&intent.intent.nonce);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
            }
            for faucet in block.faucets {
                self.inflight_faucets.remove(&faucet.address);
            }
            for governance in block.real_capital_governance {
                self.inflight_real_capital_governance_actions
                    .remove(&governance.action_id);
            }
            for delegation in block.delegations {
                self.inflight_delegation_actions
                    .remove(&delegation.action_id);
            }
            for program in block.market_maker_programs {
                self.inflight_market_maker_program_actions
                    .remove(&program.action_id);
            }
            for governance in block.tee_governance {
                self.inflight_tee_governance_actions
                    .remove(&governance.action_id);
            }
        }
        self.executed_blocks = self.executed_blocks.saturating_add(1);
        self.last_update_at = Some(Instant::now());
    }

    fn metrics_dag_snapshot_hash(&self) -> Hash {
        let digest = MahiDagSnapshotDigest {
            block_refs: self.dag.blocks.keys().copied().collect(),
            pending_fetch: self.dag.pending_fetch.iter().copied().collect(),
            committed_refs: self.linearizer.committed.iter().copied().collect(),
            last_decided_leader: self.last_decided_leader,
        };
        hash_bytes(&borsh::to_vec(&digest).expect("mahi dag snapshot digest should serialize"))
    }

    fn snapshot(&self) -> MahiStateSnapshot {
        let dag_snapshot_hash = self.metrics_dag_snapshot_hash();
        let max_round = self.dag.by_round.keys().next_back().copied().unwrap_or(0);
        let next_round = self.threshold_clock.round();
        let verification_pending_count = self
            .verification_status
            .values()
            .filter(|status| matches!(status, MahiBlockVerificationStatus::Pending))
            .count();
        let verified_block_count = self
            .verification_status
            .values()
            .filter(|status| matches!(status, MahiBlockVerificationStatus::Verified))
            .count();
        let verification_failed_count = self
            .verification_status
            .values()
            .filter(|status| matches!(status, MahiBlockVerificationStatus::Failed))
            .count();
        let committed_leader_count = self
            .decided_leaders
            .iter()
            .filter(|leader| leader.decision == 1)
            .count();
        let skipped_leader_count = self
            .decided_leaders
            .iter()
            .filter(|leader| leader.decision == 2)
            .count();
        let commit_log_hash = hash_bytes(
            &borsh::to_vec(&self.decided_leaders).expect("mahi commit log should serialize"),
        );
        let ancestor_wait = self.producer_ancestor_wait.as_ref();
        let ancestor_wait_prev_round = ancestor_wait
            .map(|wait| wait.target_round.saturating_sub(1))
            .filter(|round| *round > 0);
        let producer_ancestor_wait_authors = ancestor_wait_prev_round
            .and_then(|round| self.dag.by_round.get(&round))
            .map(|entries| entries.keys().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        let producer_ancestor_wait_missing_authors = if ancestor_wait_prev_round.is_some() {
            self.validator_verifiers
                .keys()
                .filter(|validator| !producer_ancestor_wait_authors.contains(validator))
                .copied()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let tail_round_block_counts = self
            .dag
            .by_round
            .iter()
            .rev()
            .take(8)
            .map(|(round, entries)| (*round, entries.len() as u64))
            .collect::<Vec<_>>();
        let elapsed_ms =
            |instant: Instant| instant.elapsed().as_millis().min(u64::MAX as u128) as u64;
        MahiStateSnapshot {
            enabled: true,
            epoch: self.epoch,
            commit_rule: match self.commit_rule {
                CommitRule::Mahi => "mahi",
                CommitRule::Mysticeti => "mysticeti",
            },
            pipeline: self.pipeline,
            number_of_leaders: self.number_of_leaders,
            next_round,
            next_leader: self.leader_schedule.leader_for_round(next_round),
            block_count: self.dag.blocks.len(),
            max_round,
            pending_fetch_count: self.dag.pending_fetch.len(),
            decided_leader_count: self.decided_leaders.len(),
            committed_leader_count,
            skipped_leader_count,
            committed_block_count: self.linearizer.committed.len(),
            last_decided_round: self.last_decided_leader.round,
            last_decided_author: self.last_decided_leader.author,
            restart_boundary_round: self.restart_boundary_round,
            commit_log_hash,
            producer_attempts: self.producer_attempts,
            producer_skip_ancestor_quorum: self.producer_skip_ancestor_quorum,
            producer_skip_peer_reachability: self.producer_skip_peer_reachability,
            producer_skip_snapshot_unavailable: self.producer_skip_snapshot_unavailable,
            producer_skip_empty_idle: self.producer_skip_empty_idle,
            producer_skip_round_race: self.producer_skip_round_race,
            producer_ancestor_wait_round: ancestor_wait.map(|wait| wait.target_round),
            producer_ancestor_wait_available: ancestor_wait.map(|wait| wait.available as u64),
            producer_ancestor_wait_quorum: ancestor_wait.map(|wait| wait.quorum as u64),
            producer_ancestor_wait_age_ms: ancestor_wait.map(|wait| elapsed_ms(wait.since)),
            producer_ancestor_wait_first_arrival_age_ms: ancestor_wait_prev_round
                .and_then(|round| self.round_first_arrival_at.get(&round).copied())
                .map(elapsed_ms),
            producer_ancestor_wait_latest_arrival_age_ms: ancestor_wait_prev_round
                .and_then(|round| self.round_latest_arrival_at.get(&round).copied())
                .map(elapsed_ms),
            producer_ancestor_wait_authors,
            producer_ancestor_wait_missing_authors,
            tail_round_block_counts,
            producer_initial_lock_wait_ms: self.producer_initial_lock_wait_ms.clone(),
            producer_initial_lock_hold_ms: self.producer_initial_lock_hold_ms.clone(),
            producer_snapshot_ms: self.producer_snapshot_ms.clone(),
            producer_pre_admitted_drain_ms: self.producer_pre_admitted_drain_ms.clone(),
            producer_final_lock_wait_ms: self.producer_final_lock_wait_ms.clone(),
            producer_final_lock_hold_ms: self.producer_final_lock_hold_ms.clone(),
            producer_total_ms: self.producer_total_ms.clone(),
            inbound_block_decode_ms: self.inbound_block_decode_ms.clone(),
            inbound_block_accept_ms: self.inbound_block_accept_ms.clone(),
            inbound_block_sigverify_enqueue_ms: self.inbound_block_sigverify_enqueue_ms.clone(),
            inbound_block_total_ms: self.inbound_block_total_ms.clone(),
            commit_executor_wake_count: self.commit_executor_wake_count,
            commit_executor_empty_wake_count: self.commit_executor_empty_wake_count,
            commit_execution_batches: self.commit_execution_batches,
            commit_execution_blocks: self.commit_execution_blocks,
            commit_execution_intents: self.commit_execution_intents,
            commit_execution_batch_ms: self.commit_execution_batch_ms.clone(),
            phase6_schedule_profiled_batches: self.phase6_schedule_profiled_batches,
            phase6_schedule_truncated_batches: self.phase6_schedule_truncated_batches,
            phase6_schedule_total_item_count: self.phase6_schedule_total_item_count.clone(),
            phase6_schedule_item_count: self.phase6_schedule_item_count.clone(),
            phase6_schedule_layer_count: self.phase6_schedule_layer_count.clone(),
            phase6_schedule_max_layer_width: self.phase6_schedule_max_layer_width.clone(),
            phase6_schedule_conflict_pair_count: self.phase6_schedule_conflict_pair_count.clone(),
            phase6_schedule_parallelism_milli: self.phase6_schedule_parallelism_milli.clone(),
            produced_blocks: self.produced_blocks,
            produced_empty_blocks: self.produced_empty_blocks,
            produced_intent_count: self.produced_intents,
            received_blocks: self.received_blocks,
            received_intent_count: self.received_intents,
            execution_pending_count: self.pending_execution.len(),
            executed_block_count: self.executed_blocks,
            executed_intent_count: self.executed_intents,
            verification_pending_count,
            verified_block_count,
            verification_failed_count,
            dropped_blocks: self.dropped_blocks,
            sigverify_pool_depth: 0,
            sigverify_batch_hist: Vec::new(),
            sigverify_per_intent_verified_total: 0,
            sigverify_failures_total: 0,
            last_produced_round: self.last_produced.map(|reference| reference.round),
            last_produced_hash: self.last_produced.map(|reference| reference.hash),
            dag_snapshot_hash,
            last_update_age_ms: self
                .last_update_at
                .map(|updated| updated.elapsed().as_millis().min(u64::MAX as u128) as u64),
        }
    }
}

#[derive(Clone, Debug)]
pub struct JsonRpcHttpServer {
    engine: Arc<EngineHandle>,
    network_control: Arc<Mutex<()>>,
    mahi_state: Arc<Mutex<Option<MahiState>>>,
    mahi_sigverify_pool: Option<MahiSigverifyPool>,
    mahi_commit_executor: Arc<MahiCommitExecutorControl>,
    mahi_missing_fetch_worker_running: Arc<AtomicBool>,
    mahi_inbound_drop_pct: u8,
    mahi_async_block_broadcast: bool,
    local_submit_queue: Arc<Mutex<LocalSubmitQueue>>,
    local_submit_worker_running: Arc<AtomicBool>,
    network_http_client: Arc<Client>,
    mahi_pre_admitted_pool: Arc<Mutex<MahiPreAdmittedPool>>,
    pending_intent_relays: Arc<Mutex<PendingIntentRelayQueue>>,
    pending_mahi_owner_relays: Arc<Mutex<PendingMahiOwnerRelayQueues>>,
    intent_relay_worker_running: Arc<AtomicBool>,
    pending_mahi_block_broadcasts: Arc<Mutex<PendingMahiBlockBroadcastQueue>>,
    mahi_block_broadcast_worker_running: Arc<AtomicBool>,
    network_peer_streams: Arc<Mutex<BTreeMap<String, PeerTransportSession>>>,
    peer_transport_endpoint_states: Arc<Mutex<BTreeMap<String, PeerTransportEndpointState>>>,
    active_peer_transport_sessions: Arc<Mutex<BTreeMap<u64, ActivePeerTransportSession>>>,
    timeout_message_pool: Arc<Mutex<TimeoutMessagePool>>,
    peer_snapshot_repair_state: Arc<Mutex<PeerSnapshotRepairState>>,
    rpc_submit_metrics: Arc<Mutex<RpcSubmitPathMetrics>>,
    network_round_lock_profile: Arc<Mutex<NetworkRoundLockProfileSnapshot>>,
    next_peer_session_id: Arc<AtomicU64>,
    faucet_limiter: FaucetLimiter,
    config: JsonRpcServerConfig,
    x402_config: Option<X402Config>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonRpcServerConfig {
    pub manual_block_production_enabled: bool,
    pub mahi_enabled: bool,
    pub operator_bearer_token: Option<String>,
    pub peer_relay_bearer_token: Option<String>,
    pub network_bootstrap: Option<NetworkBootstrapConfig>,
    pub peer_transport_enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkBootstrapConfig {
    pub validator_set: ValidatorSet,
    pub bootstrap_peers: BootstrapPeers,
    pub local_validator: Option<ValidatorConfig>,
    pub public_rpc_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RpcError {
    HeadNotSet,
    InvalidRequest(&'static str),
    UnknownAccount(u64),
    UnknownPolicy(u64),
    Submission(JsonRpcErrorObject),
    Node(String),
}

pub trait RpcBackend {
    fn chain_info(&self) -> Result<ChainInfoResponse, RpcError>;
    fn get_state(&self, key: &[u8]) -> Result<StateQueryResponse, RpcError>;
    fn get_state_root_at_height(&self, height: u64) -> Result<StateRootAtHeightResponse, RpcError>;
    fn eth_call(&self, request: &ContractCallRequest) -> Result<ContractCallResponse, RpcError>;
    fn list_precompiles(&self) -> Vec<PrecompileDescriptor>;
}

#[derive(Debug)]
pub struct NodeRpcService<'a> {
    engine: &'a mut SingleNodeEngine,
}

#[derive(Clone, Debug)]
pub struct RpcService<Store> {
    chain_id: u64,
    head: Option<Block>,
    executor: BlockExecutor<Store>,
}

impl Default for JsonRpcServerConfig {
    fn default() -> Self {
        Self {
            manual_block_production_enabled: true,
            mahi_enabled: false,
            operator_bearer_token: None,
            peer_relay_bearer_token: None,
            network_bootstrap: None,
            peer_transport_enabled: false,
        }
    }
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

fn build_network_http_client() -> Arc<Client> {
    Arc::new(
        Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(16)
            .tcp_keepalive(Duration::from_secs(30))
            .build()
            .expect("network http client should build"),
    )
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn build_mahi_state(
    enabled: bool,
    network_bootstrap: Option<&NetworkBootstrapConfig>,
    commit_rule: CommitRule,
    restart_boundary_round: u64,
) -> Option<MahiState> {
    if !enabled {
        return None;
    }
    let network_bootstrap = network_bootstrap?;
    let validator_verifiers = network_validator_verifiers(network_bootstrap).ok()?;
    let pipeline = parse_mahi_pipeline();
    let number_of_leaders = parse_mahi_number_of_leaders(validator_verifiers.len());
    MahiState::new(
        network_bootstrap.validator_set.epoch,
        validator_verifiers,
        commit_rule,
        pipeline,
        number_of_leaders,
        restart_boundary_round,
    )
}

#[cfg(test)]
mod tests;
