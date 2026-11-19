use super::{
    derive_delegation_id, derive_redemption_request_id,
    market_maker_program_action_id_for_submission, market_maker_program_action_signing_bytes,
    tee_verifier_outcome_matches_claim, utc_day_bucket_ms, verify_intent_signature,
    AgentIdentityRecord, AgentIdentityTier, AgentKind, AttestationClaim, AttestationEvidence,
    AttestationKind, AttestationRecord, DelegationActionReceipt, DelegationActionSubmissionStatus,
    DelegationId, DeleteAgentIdentityInput, DescriptiveUpdate, FaucetGrantSubmissionStatus,
    MarketKind, MarketLifecycleStatus, MarketMakerIdentity, MarketMakerProgramActionReceipt,
    MarketMakerProgramActionSubmissionStatus, MarketStatus, OracleFeedAggregation,
    OracleFeedSample, PerpetualMarketConfig, PolicyExecutionScope, PreparedCommittedBlock,
    ProcessedDelegationAction, ProcessedMarketMakerProgramAction, RefreshOperatorAttestationInput,
    RefreshTeeAttestationInput, RegisterAgentIdentityInput, RegisterOperatorAttestationInput,
    RegisterTeeAttestationInput, RevokeAttestationInput, SignedTeeAttestationAction,
    TeeAttestationAction, TeeAttestationSigners, TeeVendor, UpdateAgentIdentityInput, VendorRoot,
    VendorRootPayload, VerifyIntentError, ACCOUNT_EQUITY_HISTORY_DAILY_WINDOW_MS,
    ACCOUNT_EQUITY_HISTORY_DENSE_WINDOW_MS, ATTESTATION_QUOTE_FRESHNESS_BLOCKS,
    COMPLIANCE_WINDOW_MAX_MS, DEFAULT_ORACLE_FEEDERS, DEFAULT_ORACLE_FEEDER_COUNT,
    DEFAULT_ORACLE_MIN_CONFIDENCE, DEFAULT_ORACLE_MIN_SAMPLES, DEFAULT_ORACLE_MIN_SOURCES,
    DELEGATION_ACTION_RECEIPT_RETENTION_MS, MANDATE_CONFORMANCE_HISTORY_LIMIT,
    MAX_MM_PROGRAM_MARKETS, MAX_PENDING_DELEGATION_ACTIONS,
    MAX_PENDING_DELEGATION_ACTIONS_PER_SIGNER, MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS,
    MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS_PER_SIGNER, MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP,
    OBSERVATION_CADENCE_MAX_MS, OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
    ORACLE_FEED_RETAINED_TIMESTAMPS, TEE_ATTESTATION_MAX_EXPIRY_MS, TEE_ATTESTATION_MIN_EXPIRY_MS,
    UTC_DAY_MS, WINDING_GRACE_WINDOW_MS,
};
use borsh::BorshDeserialize;
use ed25519_dalek::{Signer, SigningKey};
use kombat_chain_consensus::{
    block_hash, body_root, intent_id, intent_signing_hash, Block, BlockBody, BlockDelegation,
    BlockDelegationAction, BlockDelegationFaultReason, BlockDelegationSlashPolicy, BlockFaucet,
    BlockFunding, BlockHeader, BlockItem, BlockLiquidation, BlockMarketMakerProgram,
    BlockMarketMakerProgramAction, BlockOraclePrice, BlockOracleValue,
    BlockRealCapitalGovernanceAction, BlockTeeGovernanceAction, BlockTeeVendor,
    BlockValidationError, BlockVaultRealCapitalMode, Hash, Intent, IntentKind, IntentOrderKind,
    IntentOrderSide, IntentPredicate, IntentTimeInForce, PolicyTermsWire, SignedIntent,
};
use kombat_chain_execution::{
    account_evm_address_state_key, attestation_vendor_roots_state_key,
    evm_address_account_state_key, evm_usdc_balance_state_key, evm_usdc_total_supply_state_key,
    policy_usdc_balance_state_key, AttestationVendorRootsState, AttestationVerifyFailure,
    AttestationVerifyOutcome, AttestationVerifyVendor, EvmPrecompileCall, ExecutionContext,
    PreparedBlock,
};
use kombat_chain_network::{load_validator_set_from_str, ValidatorSet};
use kombat_chain_oracle::OracleAggregationConfig;
use kombat_chain_policy::{
    OwnerAction, OwnerCall, PolicyAdmission, PolicyError, PolicyOrigin, PolicyRecord, PolicyState,
    PolicyStore, PolicyTerms, SessionPayload, SignedOwnerCall, SignedSessionPayload,
};
use kombat_chain_state_db::{CheckpointableStateStore, InMemoryStateStore, StateView};
use kombat_chain_streams::{
    AccountUpdateAction, DelegationLifecycleStatus, DelegationStatusReason,
    IdentityAttestationVendor, IdentityTierStatus, IdentityTierTransitionReason,
    IntentRejectionReason, IntentStatus, IntentStatusStage, LiquidationEscalationAction,
    LiquidationExecutionOutcome, MandateEnforcementAction, OrderFillRole, OrderFillSide,
    OrderRejectionReason, RealCapitalInsuranceWalletSpendSource, RealCapitalListingMode,
    RiskTransitionSource, RuntimeEventKind,
};
use revm::{
    interpreter::CallScheme,
    primitives::{Address, U256},
};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    build_quorum_certificate, component_name, delegation_action_id_for_submission,
    delegation_action_signing_bytes, delegation_trading_account_id, delegation_trading_address,
    derive_mandate_id, derive_market_maker_program_id, derive_tee_attestation_record_id,
    derive_vendor_root_id, identity_action_signing_bytes, mandate_action_signing_bytes,
    parse_env_bool_strict, real_capital_governance_action_id_for_submission,
    select_leader_for_validator_set, tee_attestation_signing_bytes,
    tee_governance_action_id_for_submission, vendor_root_payload_to_block, AccountEquitySample,
    ConformanceState, ConsensusLogEventKind, CreateMandateInput, DefineMarketMakerProgramInput,
    Delegation, DelegationShare, DelegationStatus, EngineHandle, EnrollMarketMakerInput,
    IdentityAction, ListingCaps, LocalCluster, LocalClusterConfig, MahiBlock, Mandate,
    MandateAction, MandateConformanceSample, MandateConstraint, MandateObjective, MandateState,
    MandateU256, MarketFeeParameters, MarketMakerComplianceState, MarketMakerEnrollment,
    MarketMakerEnrollmentStatus, MarketMakerProgram, MarketMakerProgramStatus, MarketMakerU256,
    MarketRiskParameters, NodeError, PacemakerState, PositionLedger,
    ProcessedRealCapitalGovernanceAction, ProcessedTeeGovernanceAction, ProtocolDelegationCap,
    RealCapitalGovernanceActionReceipt, RealCapitalGovernanceActionSubmissionStatus,
    RealCapitalListing, RedemptionRequest, RedemptionRequestStatus,
    ScheduledTeeVendorCapProfileUpdate, SignedDelegationAction, SignedIdentityAction,
    SignedMandateAction, SignedMarketMakerProgramAction, SingleNodeConfig, SingleNodeEngine,
    TeeGovernanceActionReceipt, TeeGovernanceActionSubmissionStatus, VaultRealCapitalMode,
    AGENT_BOND_MIN, CRYSTALLIZATION_CADENCE_MAX_MS, CRYSTALLIZATION_CADENCE_MIN_MS,
    DELEGATION_MIN_DEPOSIT, MAX_DELEGATIONS_PER_AGENT_LIVE, MAX_ENROLLMENTS_PER_MARKET,
    MAX_MM_PROGRAMS, NOTICE_PERIOD_MIN_MS, PERFORMANCE_SHARE_CAP_UNITS,
    PROGRAM_BOND_MIN_MICRO_USDC, SHARE_SCALE_FACTOR, TEE_GOVERNANCE_NOTICE_BLOCKS,
    VENDOR_ROOT_NOTICE_BLOCKS,
};

#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct MirrorSingleMarketBook {
    market_id: u64,
    next_sequence: u64,
    bids: Vec<kombat_chain_matching::RestingOrder>,
    asks: Vec<kombat_chain_matching::RestingOrder>,
}

fn validator(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn signing_key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn evm_address_from_u64(value: u64) -> Address {
    let mut bytes = [0_u8; 20];
    bytes[12..20].copy_from_slice(&value.to_be_bytes());
    Address::from(bytes)
}

fn abi_word_from_u64(value: u64) -> [u8; 32] {
    let mut word = [0_u8; 32];
    word[24..32].copy_from_slice(&value.to_be_bytes());
    word
}

fn decode_abi_word_u64(encoded: &[u8], word_index: usize) -> u64 {
    let start = word_index * 32;
    u64::from_be_bytes(
        encoded[start + 24..start + 32]
            .try_into()
            .expect("abi word should contain u64"),
    )
}

#[test]
fn mahi_legacy_exec_env_uses_boolean_values() {
    for value in ["1", "true", "TRUE", "yes", "on", " on "] {
        assert_eq!(
            parse_env_bool_strict(value),
            Some(true),
            "{value:?} should enable the flag"
        );
    }
    for value in ["0", "false", "FALSE", "no", "off", " off "] {
        assert_eq!(
            parse_env_bool_strict(value),
            Some(false),
            "{value:?} should disable the flag"
        );
    }
    for value in ["", "native", "treu", "enabled", "2"] {
        assert_eq!(
            parse_env_bool_strict(value),
            None,
            "{value:?} should be rejected"
        );
    }
}

fn encode_attestation_verify_calldata(
    vendor: AttestationVerifyVendor,
    quote_bytes: &[u8],
    nonce: Hash,
    freshness_anchor_block: u64,
    expected_claim: &[u8],
) -> Vec<u8> {
    fn push_dynamic_bytes(encoded: &mut Vec<u8>, bytes: &[u8]) {
        encoded.extend_from_slice(&abi_word_from_u64(bytes.len() as u64));
        encoded.extend_from_slice(bytes);
        let padding = (32 - (bytes.len() % 32)) % 32;
        encoded.resize(encoded.len() + padding, 0);
    }

    let quote_offset = 32 * 5;
    let quote_tail_len = 32 + quote_bytes.len().div_ceil(32) * 32;
    let expected_claim_offset = quote_offset + quote_tail_len;
    let mut calldata =
        Vec::with_capacity(4 + expected_claim_offset + 32 + expected_claim.len().div_ceil(32) * 32);
    calldata.extend_from_slice(&[0xe8, 0xe1, 0x5c, 0x5e]);
    calldata.extend_from_slice(&abi_word_from_u64(execution_vendor_abi(vendor)));
    calldata.extend_from_slice(&abi_word_from_u64(quote_offset as u64));
    calldata.extend_from_slice(&nonce);
    calldata.extend_from_slice(&abi_word_from_u64(freshness_anchor_block));
    calldata.extend_from_slice(&abi_word_from_u64(expected_claim_offset as u64));
    push_dynamic_bytes(&mut calldata, quote_bytes);
    push_dynamic_bytes(&mut calldata, expected_claim);
    calldata
}

fn execution_vendor_abi(vendor: AttestationVerifyVendor) -> u64 {
    match vendor {
        AttestationVerifyVendor::IntelSgxDcap => 0,
        AttestationVerifyVendor::AmdSevSnp => 1,
        AttestationVerifyVendor::AwsNitro => 2,
        AttestationVerifyVendor::AppleSecureEnclave => 3,
    }
}

fn evm_block_hash_state_key(number: u64) -> Vec<u8> {
    format!("evm/block-hash/{number}").into_bytes()
}

fn seed_account_policy_usdc(engine: &mut SingleNodeEngine, account_id: u64, amount: u64) {
    let mut address = [0_u8; 20];
    address[12..].copy_from_slice(&account_id.to_be_bytes());
    engine
        .apply_committed_state_updates(
            [
                (account_evm_address_state_key(account_id), address.to_vec()),
                (
                    evm_address_account_state_key(address),
                    account_id.to_be_bytes().to_vec(),
                ),
                (
                    policy_usdc_balance_state_key(address),
                    U256::from(amount).to_be_bytes::<32>().to_vec(),
                ),
            ],
            std::iter::empty(),
        )
        .expect("policy USDC seed should update committed state");
}

fn policy_usdc_balance_for_test(engine: &SingleNodeEngine, address: [u8; 20]) -> u128 {
    engine
        .get_committed_state(&policy_usdc_balance_state_key(address))
        .map(|bytes| U256::from_be_slice(&bytes).to::<u128>())
        .unwrap_or(0)
}

fn seed_real_capital_insurance_wallet(
    engine: &mut SingleNodeEngine,
    gross_fund_micro_usdc: u128,
    wallet_micro_usdc: u128,
) {
    engine.real_capital_insurance_fund_micro_usdc = gross_fund_micro_usdc;
    engine.real_capital_insurance_wallet_micro_usdc = wallet_micro_usdc;
    let mut sets = Vec::new();
    let mut deletes = Vec::new();
    SingleNodeEngine::apply_policy_usdc_balance_update(
        &mut sets,
        &mut deletes,
        super::real_capital_insurance_wallet_address(),
        U256::from(wallet_micro_usdc),
    );
    engine
        .apply_committed_state_updates(sets, deletes)
        .expect("insurance wallet seed should update committed state");
}

fn seed_isolated_margin_market_custody(
    engine: &mut SingleNodeEngine,
    market_id: u64,
    balance_micro_usdc: u128,
) {
    let mut sets = Vec::new();
    let mut deletes = Vec::new();
    SingleNodeEngine::apply_policy_usdc_balance_update(
        &mut sets,
        &mut deletes,
        super::isolated_margin_market_custody_address(market_id),
        U256::from(balance_micro_usdc),
    );
    engine
        .apply_committed_state_updates(sets, deletes)
        .expect("market custody seed should update committed state");
}

fn set_snapshot_policy_usdc_balance(
    snapshot: &mut super::PersistedSingleNodeEngineSnapshot,
    address: [u8; 20],
    amount: u128,
) {
    let key = policy_usdc_balance_state_key(address);
    snapshot
        .committed_entries
        .retain(|(entry_key, _)| *entry_key != key);
    if amount > 0 {
        snapshot
            .committed_entries
            .push((key, U256::from(amount).to_be_bytes::<32>().to_vec()));
    }
    snapshot
        .committed_entries
        .sort_by(|left, right| left.0.cmp(&right.0));
    snapshot.committed_state_root =
        InMemoryStateStore::from_entries(snapshot.committed_entries.clone()).committed_root();
}

fn sample_real_capital_listing(
    delegation: &Delegation,
    mode: VaultRealCapitalMode,
    pilot_min_serving_agent_tier: Option<AgentIdentityTier>,
    tier0_pilot_exception: bool,
) -> RealCapitalListing {
    RealCapitalListing {
        vault_id: super::vault_id_for_delegation(&delegation.delegation_id),
        delegation_id: delegation.delegation_id,
        serving_agent: delegation.agent,
        mode,
        pilot_min_serving_agent_tier,
        tier0_pilot_exception,
        deposit_enabled: true,
        listed_for_discovery: true,
        caps: ListingCaps {
            per_principal_micro_usdc: u128::MAX,
            total_vault_tvl_micro_usdc: u128::MAX,
        },
        pilot_principal_allowlist: None,
        listed_at_block: 0,
        last_updated_block: 0,
        last_updated_governance_action: [0; 32],
    }
}

fn list_delegation_for_tier0_capped_pilot(engine: &mut SingleNodeEngine, delegation: &Delegation) {
    engine
        .test_upsert_real_capital_listing(sample_real_capital_listing(
            delegation,
            VaultRealCapitalMode::CappedPilot,
            None,
            true,
        ))
        .expect("real-capital listing should seed");
}

fn list_delegation_for_tier1_capped_pilot(engine: &mut SingleNodeEngine, delegation: &Delegation) {
    engine
        .test_upsert_real_capital_listing(sample_real_capital_listing(
            delegation,
            VaultRealCapitalMode::CappedPilot,
            Some(AgentIdentityTier::Tier1OperatorAttested),
            false,
        ))
        .expect("real-capital listing should seed");
}

fn promote_agent_to_tier1(engine: &mut SingleNodeEngine, account_id: u64, timestamp_ms: u64) {
    let signer = signing_key(account_id as u8);
    if engine.account_store.get(account_id).is_none() {
        engine
            .create_account(
                account_id,
                signer.verifying_key().to_bytes(),
                [0; 32],
                timestamp_ms,
            )
            .expect("account should create");
    }
    if engine.agent_identity(account_id).is_none() {
        engine
            .register_agent_identity(
                RegisterAgentIdentityInput {
                    account: account_id,
                    display_name: format!("agent-{account_id}"),
                    description: String::new(),
                    kind: AgentKind::Unknown,
                    operator: Some(account_id),
                    contact_uri: None,
                },
                timestamp_ms,
            )
            .expect("identity should register");
    }
    engine
        .register_operator_attestation(
            RegisterOperatorAttestationInput {
                attestor: account_id,
                subject: account_id,
                claim: AttestationClaim::RunsCodeHash {
                    code_hash: [account_id as u8; 32],
                    model_hash: None,
                },
                expires_at_timestamp_ms: timestamp_ms + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            [account_id as u8; 64],
            timestamp_ms,
        )
        .expect("operator attestation should register");
    assert_eq!(
        engine
            .agent_identity(account_id)
            .map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier1OperatorAttested)
    );
}

fn intel_vendor_root(root_id: [u8; 32], valid_until_block: Option<u64>) -> VendorRoot {
    VendorRoot {
        vendor: TeeVendor::IntelSgxDcap,
        root_id,
        payload: VendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![1, 2, 3],
            tcb_info_signing_cert_der: vec![4, 5, 6],
            tcb_info_url: "https://example.invalid/tcb.json".into(),
            crl_der: vec![7, 8, 9],
        },
        valid_from_block: 0,
        valid_until_block,
        introduced_by_governance_action: [1; 32],
        revoked_by_governance_action: valid_until_block.map(|_| [2; 32]),
    }
}

fn vendor_root_fixture(vendor: TeeVendor, root_id: [u8; 32]) -> VendorRoot {
    let payload = match vendor {
        TeeVendor::IntelSgxDcap => VendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![1, 2, 3],
            tcb_info_signing_cert_der: vec![4, 5, 6],
            tcb_info_url: "https://example.invalid/tcb.json".into(),
            crl_der: vec![7, 8, 9],
        },
        TeeVendor::AmdSevSnp => VendorRootPayload::AmdSevRoot {
            ark_cert_der: vec![1, 2, 3],
            ask_cert_der: vec![4, 5, 6],
            crl_der: vec![7, 8, 9],
        },
        TeeVendor::AwsNitro => VendorRootPayload::AwsNitroRoot {
            root_cert_der: vec![1, 2, 3],
        },
        TeeVendor::AppleSecureEnclave => VendorRootPayload::AppleSecureEnclaveRoot {
            root_cert_der: vec![1, 2, 3],
        },
    };
    VendorRoot {
        vendor,
        root_id,
        payload,
        valid_from_block: 0,
        valid_until_block: None,
        introduced_by_governance_action: [1; 32],
        revoked_by_governance_action: None,
    }
}

fn execution_vendor_for(vendor: TeeVendor) -> AttestationVerifyVendor {
    match vendor {
        TeeVendor::IntelSgxDcap => AttestationVerifyVendor::IntelSgxDcap,
        TeeVendor::AmdSevSnp => AttestationVerifyVendor::AmdSevSnp,
        TeeVendor::AwsNitro => AttestationVerifyVendor::AwsNitro,
        TeeVendor::AppleSecureEnclave => AttestationVerifyVendor::AppleSecureEnclave,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut encoded, "{byte:02x}").expect("hex encoding should write");
    }
    encoded
}

fn tamper_snapshot_book<F>(
    snapshot: &mut super::PersistedSingleNodeEngineSnapshot,
    market_id: u64,
    mutate: F,
) where
    F: FnOnce(&mut MirrorSingleMarketBook),
{
    let book = snapshot
        .order_books
        .remove(&market_id)
        .expect("snapshot should contain target market book");
    let mut mirror = MirrorSingleMarketBook::try_from_slice(
        &borsh::to_vec(&book).expect("book should serialize"),
    )
    .expect("book should deserialize into mirror");
    mutate(&mut mirror);
    let mutated_book = kombat_chain_matching::SingleMarketBook::try_from_slice(
        &borsh::to_vec(&mirror).expect("mirror should serialize"),
    )
    .expect("mirror should deserialize into market book");
    snapshot.order_books.insert(market_id, mutated_book);
}

fn pacemaker() -> kombat_chain_consensus::PacemakerConfig {
    kombat_chain_consensus::PacemakerConfig {
        base_timeout_ms: 300,
        max_timeout_ms: 20_000,
        max_exponent: 6,
        max_clock_skew_ms: 1_000,
    }
}

fn single_node_config() -> SingleNodeConfig {
    SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(9),
        consensus_signing_key: [9; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    }
}

fn btc_perp_config() -> PerpetualMarketConfig {
    PerpetualMarketConfig {
        initial_margin_bps: 1_000,
        maintenance_margin_bps: 500,
        max_leverage: 20,
        lot_size_microunits: 1_000_000,
        tick_size_bps: 10_000,
        funding_interval_ms: 3_600_000,
        oracle_max_staleness_ms: 5_000,
        breaker_price_band_bps: 500,
        oracle_min_sources: DEFAULT_ORACLE_MIN_SOURCES,
        oracle_min_confidence: DEFAULT_ORACLE_MIN_CONFIDENCE,
        oracle_min_samples: DEFAULT_ORACLE_MIN_SAMPLES,
        oracle_feeder_count: DEFAULT_ORACLE_FEEDER_COUNT,
        oracle_feeder_allowlist: DEFAULT_ORACLE_FEEDERS,
    }
}

fn sample_market_maker_program_input(
    markets: impl IntoIterator<Item = u64>,
) -> DefineMarketMakerProgramInput {
    DefineMarketMakerProgramInput {
        name: "perps-gold".into(),
        markets: markets.into_iter().collect(),
        min_uptime_bps: 9_000,
        max_spread_bps: 50,
        min_quote_size_micro_usdc: MarketMakerU256::from_u128(100_000_000),
        min_two_sided_windows_per_hour: 40,
        rebate_multiplier_units: 20_000,
        enrollment_bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
        compliance_window_ms: 3_600_000,
        observation_cadence_ms: 60_000,
    }
}

fn sample_market_maker_compliance_program_input(
    markets: impl IntoIterator<Item = u64>,
) -> DefineMarketMakerProgramInput {
    let mut input = sample_market_maker_program_input(markets);
    input.min_uptime_bps = 5_000;
    input.max_spread_bps = 200;
    input.min_quote_size_micro_usdc = MarketMakerU256::from_u128(1);
    input.min_two_sided_windows_per_hour = 1;
    input.compliance_window_ms = crate::OBSERVATION_CADENCE_MIN_MS;
    input.observation_cadence_ms = crate::OBSERVATION_CADENCE_MIN_MS;
    input
}

fn refresh_scaled_test_market_prices(
    engine: &mut SingleNodeEngine,
    market_id: u64,
    timestamp_ms: u64,
) {
    engine.update_market_index_price(market_id, 100, timestamp_ms);
    engine.update_market_mark_price(market_id, 100, timestamp_ms);
}

fn submit_two_sided_market_maker_quotes(
    engine: &mut SingleNodeEngine,
    market_id: u64,
    account_id: u64,
    base_timestamp_ms: u64,
) {
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id,
                account_id,
                client_order_id: 10_000 + base_timestamp_ms,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            base_timestamp_ms,
        )
        .expect("bid quote should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id,
                account_id,
                client_order_id: 20_000 + base_timestamp_ms,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 101 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            base_timestamp_ms.saturating_add(1),
        )
        .expect("ask quote should rest");
}

fn seed_market_maker_compliance_engine() -> (SingleNodeEngine, MarketMakerIdentity, [u8; 32]) {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    refresh_scaled_test_market_prices(&mut engine, 7, 1_000);
    engine
        .create_account(22, [22; 32], [9; 32], 1_010)
        .expect("market maker account should create");
    seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_compliance_program_input([7]), 1_020)
        .expect("program should define");
    let identity = MarketMakerIdentity::Account(22);
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_030,
            None,
        )
        .expect("market maker enrollment should succeed");
    engine.set_isolated_collateral(7, 22, 1_000_000);
    (engine, identity, program.program_id)
}

fn sample_market_maker_program_define_action(
    markets: impl IntoIterator<Item = u64>,
) -> BlockMarketMakerProgramAction {
    let input = sample_market_maker_program_input(markets);
    BlockMarketMakerProgramAction::Define {
        name: input.name,
        markets: input.markets,
        min_uptime_bps: input.min_uptime_bps,
        max_spread_bps: input.max_spread_bps,
        min_quote_size_micro_usdc: input.min_quote_size_micro_usdc.0,
        min_two_sided_windows_per_hour: input.min_two_sided_windows_per_hour,
        rebate_multiplier_units: input.rebate_multiplier_units,
        enrollment_bond_micro_usdc: input.enrollment_bond_micro_usdc.0,
        compliance_window_ms: input.compliance_window_ms,
        observation_cadence_ms: input.observation_cadence_ms,
    }
}

fn sample_mandate(agent: u64, version: u32) -> Mandate {
    let input = sample_mandate_input(agent, version);
    let id = derive_mandate_id(
        input.owner,
        input.agent,
        input.version,
        input.constraints.clone(),
        input.objectives.clone(),
        input.objective_weights.clone(),
        input.conformance_window_ms,
        input.session_keys.clone(),
        input.expires_at,
        input.description_hash,
    );
    Mandate {
        id,
        owner: input.owner,
        agent: input.agent,
        version: input.version,
        constraints: input.constraints,
        objectives: input.objectives,
        objective_weights: input.objective_weights,
        conformance_window_ms: input.conformance_window_ms,
        policies: Vec::new(),
        session_keys: input.session_keys,
        activated_at: Some(1_000),
        expires_at: input.expires_at,
        state: MandateState::Active,
        description_uri: input.description_uri,
        description_hash: input.description_hash,
        conformance: ConformanceState {
            score: 50,
            last_evaluated_at_ms: 1_000,
            pending_objectives: 1,
        },
    }
}

fn sample_mandate_input(agent: u64, version: u32) -> CreateMandateInput {
    let owner = [7_u8; 32];
    let constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7]),
        },
        MandateConstraint::MaxNotionalPerMarket {
            market: 7,
            limit_usdc: MandateU256::from_u128(1_000_000),
        },
    ];
    let objectives = vec![MandateObjective::MaxSlippageBps { limit: 50 }];
    let objective_weights = vec![100];
    let conformance_window_ms = 86_400_000;
    let session_keys = vec![signing_key(8).verifying_key().to_bytes()];
    let expires_at = 1_000 + 86_400_000;
    let description_hash = Some([9_u8; 32]);
    CreateMandateInput {
        owner,
        agent,
        version,
        constraints,
        objectives,
        objective_weights,
        conformance_window_ms,
        session_keys,
        expires_at,
        description_uri: Some("ipfs://mandate".into()),
        description_hash,
    }
}

fn create_sample_mandate(
    engine: &mut SingleNodeEngine,
    agent: u64,
    version: u32,
    timestamp_ms: u64,
) -> Mandate {
    engine
        .create_mandate(sample_mandate_input(agent, version), timestamp_ms)
        .expect("mandate should create")
}

fn setup_two_mandates_with_long_position() -> (SingleNodeEngine, SigningKey, Mandate, Mandate) {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44, 45] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker ask should rest");
    let open_long = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_long, 1_030)
        .expect("opening mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("opening block should produce");
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);

    (engine, signer, first, second)
}

fn create_no_same_operator_mandate(engine: &mut SingleNodeEngine, signer: &SigningKey) -> Mandate {
    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::NoSameOperatorCounterparty);
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    engine
        .create_mandate(input, 1_010)
        .expect("same-operator mandate should create")
}

fn signed_mandate_action(
    signer: &SigningKey,
    chain_id: u64,
    nonce: u64,
    action: MandateAction,
) -> SignedMandateAction {
    let owner = signer.verifying_key().to_bytes();
    SignedMandateAction {
        chain_id,
        owner,
        nonce,
        signature: signer
            .sign(&mandate_action_signing_bytes(
                chain_id, owner, nonce, &action,
            ))
            .to_bytes(),
        action,
    }
}

fn signed_identity_action(
    signer: &SigningKey,
    chain_id: u64,
    nonce: u64,
    action: IdentityAction,
) -> SignedIdentityAction {
    let account = action.account();
    SignedIdentityAction {
        chain_id,
        account,
        nonce,
        signature: signer
            .sign(&identity_action_signing_bytes(
                chain_id, account, nonce, &action,
            ))
            .to_bytes(),
        action,
    }
}

fn signed_tee_self_attestation_action(
    signer: &SigningKey,
    chain_id: u64,
    attestor_nonce: u64,
    action: TeeAttestationAction,
) -> SignedTeeAttestationAction {
    let signing_bytes = tee_attestation_signing_bytes(chain_id, &action, attestor_nonce, None);
    SignedTeeAttestationAction {
        chain_id,
        action,
        signers: TeeAttestationSigners::SelfAttest {
            attestor_nonce,
            attestor_signature: signer.sign(&signing_bytes).to_bytes(),
        },
    }
}

fn signed_tee_operator_attestation_action(
    attestor_signer: &SigningKey,
    subject_signer: &SigningKey,
    chain_id: u64,
    attestor_nonce: u64,
    subject_nonce: u64,
    action: TeeAttestationAction,
) -> SignedTeeAttestationAction {
    let signing_bytes =
        tee_attestation_signing_bytes(chain_id, &action, attestor_nonce, Some(subject_nonce));
    SignedTeeAttestationAction {
        chain_id,
        action,
        signers: TeeAttestationSigners::OperatorAttest {
            attestor_nonce,
            attestor_signature: attestor_signer.sign(&signing_bytes).to_bytes(),
            subject_nonce,
            subject_signature: subject_signer.sign(&signing_bytes).to_bytes(),
        },
    }
}

fn seed_tee_identity_fixture() -> (SingleNodeEngine, SigningKey, SigningKey) {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: "Scout".into(),
                description: String::new(),
                kind: AgentKind::Unknown,
                operator: Some(22),
                contact_uri: None,
            },
            1_110,
        )
        .expect("identity should register");
    (engine, subject_signer, operator_signer)
}

fn tee_attestation_record(record_id: [u8; 32]) -> AttestationRecord {
    AttestationRecord {
        record_id,
        account: 11,
        kind: AttestationKind::TeeEnclave,
        attestor: 22,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: None,
        },
        evidence: AttestationEvidence::TeeQuoteVerified {
            vendor: TeeVendor::IntelSgxDcap,
            vendor_root_id: [5_u8; 32],
            enclave_id: [6_u8; 32],
            signer: [7_u8; 32],
            verified_at_block: 1,
        },
        issued_at_block: 1,
        issued_at_timestamp_ms: 1_100,
        expires_at_timestamp_ms: 1_200 + TEE_ATTESTATION_MIN_EXPIRY_MS,
        signature: [8_u8; 64],
        revoked_at_block: None,
        verified: true,
    }
}

fn signed_delegation_action(
    signer: &SigningKey,
    chain_id: u64,
    signer_account_id: u64,
    action: BlockDelegationAction,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> SignedDelegationAction {
    let action_id = delegation_action_id_for_submission(&action, timestamp_ms, submitter_nonce);
    let mut signed = SignedDelegationAction {
        chain_id,
        signer_account_id,
        action_id,
        timestamp_ms,
        submitter_nonce,
        action,
        signature: [0; 64],
    };
    signed.signature = signer
        .sign(&delegation_action_signing_bytes(&signed))
        .to_bytes();
    signed
}

fn signed_market_maker_program_action(
    signer: &SigningKey,
    chain_id: u64,
    signer_account_id: u64,
    action: BlockMarketMakerProgramAction,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> SignedMarketMakerProgramAction {
    let action_id =
        market_maker_program_action_id_for_submission(&action, timestamp_ms, submitter_nonce);
    let mut signed = SignedMarketMakerProgramAction {
        chain_id,
        signer_account_id,
        action_id,
        timestamp_ms,
        submitter_nonce,
        action,
        signature: [0; 64],
    };
    signed.signature = signer
        .sign(&market_maker_program_action_signing_bytes(&signed))
        .to_bytes();
    signed
}

fn market_maker_enroll_action(program_id: [u8; 32]) -> BlockMarketMakerProgramAction {
    BlockMarketMakerProgramAction::Enroll {
        market_maker_identity: MarketMakerIdentity::Account(31),
        market_id: 7,
        program_id,
        bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC).0,
    }
}

fn delegation_market_maker_enroll_action(
    delegation_id: DelegationId,
    serving_agent: u64,
    program_id: [u8; 32],
) -> BlockMarketMakerProgramAction {
    BlockMarketMakerProgramAction::Enroll {
        market_maker_identity: MarketMakerIdentity::Delegation {
            delegation_id,
            serving_agent,
        },
        market_id: 7,
        program_id,
        bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC).0,
    }
}

fn setup_signed_market_maker_enrollment_fixture() -> (SingleNodeEngine, SigningKey, [u8; 32]) {
    let signer = signing_key(31);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, signer.verifying_key().to_bytes(), [9; 32], 1_150)
        .expect("market maker account should create");
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    (engine, signer, program.program_id)
}

fn setup_signed_delegation_market_maker_enrollment_fixture(
) -> (SingleNodeEngine, SigningKey, Delegation, [u8; 32]) {
    let signer = signing_key(31);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, signer.verifying_key().to_bytes(), [9; 32], 1_150)
        .expect("serving agent account should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_200);
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_250)
        .expect("program should define");
    (engine, signer, delegation, program.program_id)
}

fn indexed_action_id(index: usize) -> [u8; 32] {
    let mut action_id = [0_u8; 32];
    action_id[24..].copy_from_slice(&(index as u64).to_be_bytes());
    action_id
}

mod market_maker;

#[test]
fn single_node_engine_persists_and_indexes_mandate_state() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = sample_mandate(22, 1);
    let mandate_id = mandate.id;
    engine.mandates.insert(mandate_id, mandate.clone());
    engine
        .mandates_by_agent
        .entry(mandate.agent)
        .or_default()
        .insert(mandate_id);

    assert_eq!(engine.mandate(&mandate_id), Some(&mandate));
    assert_eq!(
        engine
            .mandates_for_agent(22)
            .map(|mandate| mandate.id)
            .collect::<Vec<_>>(),
        vec![mandate_id]
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(restored.mandate(&mandate_id), Some(&mandate));
    assert_eq!(
        restored
            .mandates_for_agent(22)
            .map(|mandate| mandate.id)
            .collect::<Vec<_>>(),
        vec![mandate_id]
    );
}

#[test]
fn snapshot_bytes_reject_mandate_agent_index_mismatch() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mandate = sample_mandate(22, 1);
    let mut snapshot = engine.persisted_snapshot();
    snapshot.mandates.insert(mandate.id, mandate);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject missing mandate index");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("mandate agent index mismatch"));
}

#[test]
fn snapshot_bytes_reject_mandate_id_mismatch() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut mandate = sample_mandate(22, 1);
    let mandate_id = mandate.id;
    mandate.version += 1;
    let mut snapshot = engine.persisted_snapshot();
    snapshot.mandates.insert(mandate_id, mandate);
    snapshot
        .mandates_by_agent
        .entry(22)
        .or_default()
        .insert(mandate_id);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject tampered mandate id material");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("mandate id mismatch"));
}

#[test]
fn snapshot_bytes_reject_invalid_account_operator_linkage() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut snapshot = engine.persisted_snapshot();
    snapshot.account_operators.insert(0, 1);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject invalid account operator linkage");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("account operator mismatch"));
}

#[test]
fn snapshot_bytes_reject_identity_timestamp_regression() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            signing_key(1).verifying_key().to_bytes(),
            [0; 32],
            1_000,
        )
        .expect("account should create");
    let mut snapshot = engine.persisted_snapshot();
    snapshot.agent_identities.insert(
        11,
        AgentIdentityRecord {
            account: 11,
            tier: AgentIdentityTier::Tier0Unattested,
            operator: None,
            kind: AgentKind::Unknown,
            display_name: String::new(),
            description: String::new(),
            contact_uri: None,
            registered_at_block: 0,
            registered_at_timestamp_ms: 900,
            last_updated_block: 0,
            last_updated_timestamp_ms: 800,
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject identity timestamp regression");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("identity timestamp invalid"));
}

#[test]
fn snapshot_bytes_reject_identity_action_nonce_unknown_account() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut snapshot = engine.persisted_snapshot();
    snapshot.identity_action_nonces.insert(11, 1);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject unknown identity nonce account");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("identity action nonce references unknown account"));
}

mod identity;

#[test]
fn single_node_engine_creates_tier0_mandate_and_events() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = engine
        .create_mandate(sample_mandate_input(22, 1), 1_000)
        .expect("tier0 mandate should create and activate");

    assert_eq!(mandate.agent, 22);
    assert_eq!(mandate.version, 1);
    assert_eq!(mandate.state, MandateState::Active);
    assert_eq!(mandate.activated_at, Some(1_000));
    assert_eq!(mandate.conformance.score, 50);
    assert_eq!(mandate.policies.len(), 1);
    let generated_policy_id = mandate.policies[0];
    let generated_policy = engine
        .policy_store()
        .get(generated_policy_id)
        .expect("generated policy should exist");
    assert_eq!(generated_policy.authority_account, 22);
    assert_eq!(generated_policy.admission_rules.allowed_markets, vec![7]);
    assert_eq!(
        generated_policy.admission_rules.max_notional_outstanding,
        1_000_000
    );
    assert!(matches!(
        &generated_policy.origin,
        PolicyOrigin::MandateGenerated {
            mandate_id,
            constraint_index: u16::MAX,
            generated_policy_index: 0,
        } if mandate_id == &mandate.id
    ));
    assert_eq!(
        engine
            .mandates_for_agent(22)
            .map(|mandate| mandate.id)
            .collect::<Vec<_>>(),
        vec![mandate.id]
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateCreated {
            mandate_id,
            agent: 22,
            version: 1,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::PolicyCreated {
            account_id: 22,
            parent_policy_id: None,
            policy_id,
        } if policy_id == generated_policy_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateActivated {
            mandate_id,
            agent: 22,
            activated_at_ms: 1_000,
        } if mandate_id == mandate.id
    )));

    let err = engine
        .create_mandate(sample_mandate_input(22, 1), 2_000)
        .expect_err("same version should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            reason: "version_not_monotonic",
            ..
        }
    ));
}

#[test]
fn signed_mandate_action_creates_and_consumes_owner_nonce() {
    let owner_signer = signing_key(7);
    let owner = owner_signer.verifying_key().to_bytes();
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.owner = owner;
    let action = MandateAction::CreateMandate(input);
    let signed = signed_mandate_action(&owner_signer, 7, 0, action);

    let mandate = engine
        .submit_mandate_action(signed.clone(), 1_000)
        .expect("signed mandate action should create");
    assert_eq!(mandate.owner, owner);
    assert_eq!(mandate.state, MandateState::Active);
    assert_eq!(engine.mandate_action_nonce(&owner), 1);

    let err = engine
        .submit_mandate_action(signed, 1_100)
        .expect_err("replayed nonce should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandateActionNonce {
            owner: actual_owner,
            expected: 1,
            actual: 0,
        } if actual_owner == owner
    ));

    let restored = SingleNodeEngine::from_snapshot(engine.snapshot());
    assert_eq!(restored.mandate_action_nonce(&owner), 1);
}

#[test]
fn signed_mandate_action_rejects_chain_and_signature_mismatches() {
    let owner_signer = signing_key(7);
    let owner = owner_signer.verifying_key().to_bytes();
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.owner = owner;
    let action = MandateAction::CreateMandate(input);

    let err = engine
        .submit_mandate_action(
            signed_mandate_action(&owner_signer, 8, 0, action.clone()),
            1_000,
        )
        .expect_err("wrong chain id should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandateAction {
            owner: actual_owner,
            reason: "chain_mismatch",
        } if actual_owner == owner
    ));

    let wrong_signer = signing_key(9);
    let bad_signature = SignedMandateAction {
        chain_id: 7,
        owner,
        nonce: 0,
        signature: wrong_signer
            .sign(&mandate_action_signing_bytes(7, owner, 0, &action))
            .to_bytes(),
        action,
    };
    let err = engine
        .submit_mandate_action(bad_signature, 1_000)
        .expect_err("bad signature should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandateActionSignature { owner: actual_owner } if actual_owner == owner
    ));
    assert_eq!(engine.mandate_action_nonce(&owner), 0);
}

#[test]
fn mandate_generated_policy_admission_and_composes_active_sibling_mandates() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut wide = sample_mandate_input(22, 1);
    wide.constraints = vec![MandateConstraint::AllowedMarkets {
        set: BTreeSet::from([7, 8]),
    }];
    wide.session_keys = vec![signing_key(8).verifying_key().to_bytes()];
    let wide = engine
        .create_mandate(wide, 1_000)
        .expect("wide mandate should create");

    let mut tight = sample_mandate_input(22, 2);
    tight.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([8]),
        },
        MandateConstraint::MaxTradeRatePerMinute { count: 1 },
    ];
    tight.session_keys = vec![signing_key(9).verifying_key().to_bytes()];
    let tight = engine
        .create_mandate(tight, 1_000)
        .expect("tight mandate should create");

    let blocked = signed_intent_for_policy(
        &signing_key(8),
        wide.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(blocked, 1_100)
        .expect_err("sibling mandate should reject market 7");
    assert_eq!(err, NodeError::Policy(PolicyError::MarketNotAllowed));

    let allowed = signed_intent_for_policy(
        &signing_key(8),
        wide.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 8,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(allowed, 1_200)
        .expect("market allowed by every active mandate should admit");
    engine
        .produce_block_from_mempool(1_250, 10)
        .expect("first mandate intent should commit");

    let rate_limited = signed_intent_for_policy(
        &signing_key(8),
        wide.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 8,
            client_order_id: 3,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(rate_limited, 1_300)
        .expect_err("sibling mandate rate counter should reject second same-minute intent");
    assert_eq!(
        err,
        NodeError::Policy(PolicyError::MinuteIntentLimitExceeded)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 1,
            action: MandateEnforcementAction::AdmissionRejected,
            ..
        } if mandate_id == tight.id
    )));
}

#[test]
fn fill_mode_allowed_counterparties_rolls_back_disallowed_match() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::AllowedCounterparties {
            accounts: BTreeSet::from([33]),
        });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("fill-mode mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("disallowed maker should rest");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 101 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(intent, 1_030)
        .expect("fill-mode rollback should not reject admission");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("rollback block should produce");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert!(engine.resting_order(7, 44, 44, 1_050).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            ..
        }
    )));
}

#[test]
fn fill_mode_rollback_reports_lowest_failing_constraint_index() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(50),
    });
    input
        .constraints
        .push(MandateConstraint::AllowedCounterparties {
            accounts: BTreeSet::from([33]),
        });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("combined fill-mode mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 60 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("disallowed maker should rest");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Market,
            time_in_force: IntentTimeInForce::Ioc,
            post_only: false,
        },
    );
    engine
        .submit_intent(intent, 1_030)
        .expect("fill-mode rollback should not reject admission");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("combined rollback block should produce");

    let rollback_events = engine
        .runtime_events()
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::MandateConstraintTriggered {
                    mandate_id,
                    action: MandateEnforcementAction::FillRolledBack,
                    ..
                } if mandate_id == mandate.id
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(rollback_events.len(), 1);
    assert!(matches!(
        rollback_events[0].kind,
        RuntimeEventKind::MandateConstraintTriggered {
            constraint_index: 2,
            ..
        }
    ));
}

#[test]
fn fill_mode_max_trade_size_rolls_back_market_fill_over_limit() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);
    engine.set_isolated_collateral(7, 45, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(100),
    });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("fill-mode trade-size mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 40 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("first maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 45,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 70 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_021,
        )
        .expect("second maker ask should rest");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 2,
            kind: IntentOrderKind::Market,
            time_in_force: IntentTimeInForce::Ioc,
            post_only: false,
        },
    );
    engine
        .submit_intent(intent, 1_030)
        .expect("market order admission uses conservative placeholder notional");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("oversized actual fill should roll back");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert_eq!(engine.position(7, 45).net_lots(), 0);
    assert!(engine.resting_order(7, 44, 44, 1_050).is_some());
    assert!(engine.resting_order(7, 45, 45, 1_050).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            ..
        }
    )));
}

#[test]
fn fill_mode_max_trade_size_allows_fill_at_limit() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(50),
    });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("fill-mode trade-size mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 50 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker ask should rest");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 50 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(intent, 1_030)
        .expect("at-limit trade-size order should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("at-limit fill should commit");

    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert_eq!(engine.position(7, 22).mandate_id, Some(mandate.id));
    assert_eq!(engine.position(7, 44).net_lots(), -1);
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn fill_mode_max_trade_size_rolls_back_scaled_maker_fill_over_limit() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 30, 1_001);
    engine.update_market_mark_price(7, 30, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(50),
    });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("fill-mode trade-size mandate should create");

    let maker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 15 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(maker, 1_020)
        .expect("first maker order is individually below admission limit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("first maker order should rest");
    let second_maker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 15 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(second_maker, 1_031)
        .expect("second maker order is individually below admission limit");
    engine
        .produce_block_from_mempool(1_035, 10)
        .expect("second maker order should rest");

    let mut taker_policy = sample_policy_for_account(44);
    taker_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(taker_policy)
        .expect("taker policy should insert");
    let taker = signed_intent_for_policy(
        &signer,
        99,
        44,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 44,
            side: IntentOrderSide::Bid,
            quantity_lots: 2,
            kind: IntentOrderKind::Limit { price_ticks: 15 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_040)
        .expect("direct taker should admit");
    engine
        .produce_block_from_mempool(1_050, 10)
        .expect("scaled maker fill should roll back");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert!(engine.resting_order(7, 22, 1, 1_060).is_some());
    assert!(engine.resting_order(7, 22, 2, 1_060).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn fill_mode_allowed_counterparties_rolls_back_disallowed_maker_match() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::AllowedCounterparties {
            accounts: BTreeSet::from([33]),
        });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("fill-mode mandate should create");

    let maker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(maker, 1_020)
        .expect("maker should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("maker block should produce");

    let mut taker_policy = sample_policy_for_account(44);
    taker_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(taker_policy)
        .expect("taker policy should insert");
    let taker = signed_intent_for_policy(
        &signer,
        99,
        44,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 44,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 101 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_040)
        .expect("maker-side fill-mode rollback should not reject admission");
    engine
        .produce_block_from_mempool(1_050, 10)
        .expect("rollback block should produce");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert!(engine.resting_order(7, 22, 1, 1_060).is_some());
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(7, 22, 1)),
        Some(&mandate.id)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            ..
        }
    )));
}

#[test]
fn fill_mode_no_same_operator_rolls_back_same_operator_taker_match() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);
    engine.set_account_operator(22, 1, 1_012).unwrap();
    engine.set_account_operator(44, 1, 1_013).unwrap();
    let mandate = create_no_same_operator_mandate(&mut engine, &signer);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");

    let taker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_030)
        .expect("fill-mode rollback should not reject admission");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("rollback block should produce");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert!(engine.resting_order(7, 44, 44, 1_050).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn fill_mode_no_same_operator_allows_different_operator_match() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);
    engine.set_account_operator(22, 1, 1_012).unwrap();
    engine.set_account_operator(44, 2, 1_013).unwrap();
    let mandate = create_no_same_operator_mandate(&mut engine, &signer);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");

    let taker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_030)
        .expect("different-operator taker should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("different-operator fill should commit");

    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert_eq!(engine.position(7, 22).mandate_id, Some(mandate.id));
    assert_eq!(engine.position(7, 44).net_lots(), -1);
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn fill_mode_no_same_operator_allows_missing_operator_linkage() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);
    engine.set_account_operator(22, 1, 1_012).unwrap();
    let mandate = create_no_same_operator_mandate(&mut engine, &signer);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");

    let taker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_030)
        .expect("missing counterparty operator should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("missing counterparty operator should commit");

    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert_eq!(engine.position(7, 22).mandate_id, Some(mandate.id));
    assert_eq!(engine.position(7, 44).net_lots(), -1);
}

#[test]
fn fill_mode_no_same_operator_rolls_back_same_operator_maker_match() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);
    engine.set_account_operator(22, 1, 1_012).unwrap();
    engine.set_account_operator(44, 1, 1_013).unwrap();
    let mandate = create_no_same_operator_mandate(&mut engine, &signer);

    let maker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(maker, 1_020)
        .expect("maker should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("maker order should rest");

    let mut taker_policy = sample_policy_for_account(44);
    taker_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(taker_policy)
        .expect("taker policy should insert");
    let taker = signed_intent_for_policy(
        &signer,
        99,
        44,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 44,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_040)
        .expect("direct taker should admit");
    engine
        .produce_block_from_mempool(1_050, 10)
        .expect("maker-side same-operator fill should roll back");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert!(engine.resting_order(7, 22, 1, 1_060).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn fill_mode_no_same_operator_is_scoped_to_origin_mandate() {
    let signer = signing_key(8);
    let build_engine = |constrained_nonce: u64| {
        let mut engine = SingleNodeEngine::new(single_node_config());
        list_test_perpetual_market(&mut engine, 7);
        engine.update_market_index_price(7, 100, 1_001);
        engine.update_market_mark_price(7, 100, 1_001);
        engine.set_isolated_collateral(7, 22, 1_000);
        engine.set_isolated_collateral(7, 44, 1_000);
        engine.set_account_operator(22, 1, 1_012).unwrap();
        engine.set_account_operator(44, 1, 1_013).unwrap();

        let constrained = create_no_same_operator_mandate(&mut engine, &signer);
        let mut unconstrained_input = sample_mandate_input(22, 2);
        unconstrained_input.session_keys = vec![signer.verifying_key().to_bytes()];
        let unconstrained = engine
            .create_mandate(unconstrained_input, 1_014)
            .expect("unconstrained mandate should create");

        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id: 7,
                    account_id: 44,
                    client_order_id: 44,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots: 1,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_020,
            )
            .expect("maker order should rest");

        (engine, constrained, unconstrained, constrained_nonce)
    };

    let (mut constrained_engine, constrained, _, constrained_nonce) = build_engine(1);
    let constrained_taker = signed_intent_for_policy(
        &signer,
        constrained.policies[0],
        22,
        constrained_nonce,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    constrained_engine
        .submit_intent(constrained_taker, 1_030)
        .expect("constrained taker should admit");
    constrained_engine
        .produce_block_from_mempool(1_040, 10)
        .expect("constrained rollback block should produce");
    assert_eq!(constrained_engine.position(7, 22).net_lots(), 0);
    assert!(constrained_engine
        .runtime_events()
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::MandateConstraintTriggered {
                mandate_id,
                action: MandateEnforcementAction::FillRolledBack,
                ..
            } if mandate_id == constrained.id
        )));

    let (mut unconstrained_engine, _, unconstrained, _) = build_engine(1);
    let unconstrained_taker = signed_intent_for_policy(
        &signer,
        unconstrained.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    unconstrained_engine
        .submit_intent(unconstrained_taker, 1_030)
        .expect("unconstrained taker should admit");
    unconstrained_engine
        .produce_block_from_mempool(1_040, 10)
        .expect("unconstrained fill should commit");
    assert_eq!(unconstrained_engine.position(7, 22).net_lots(), 1);
    assert_eq!(
        unconstrained_engine.position(7, 22).mandate_id,
        Some(unconstrained.id)
    );
    assert!(!unconstrained_engine
        .runtime_events()
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::MandateConstraintTriggered {
                mandate_id,
                action: MandateEnforcementAction::FillRolledBack,
                ..
            } if mandate_id == unconstrained.id
        )));
}

#[test]
fn mandate_fill_attributes_taker_position_and_event() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(intent, 1_030)
        .expect("mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("mandate fill block should produce");

    assert_eq!(engine.position(7, 22).mandate_id, Some(mandate.id));
    assert_eq!(engine.position(7, 44).mandate_id, None);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            mandate_id,
            ..
        } if mandate_id == Some(mandate.id)
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 44,
            mandate_id: None,
            ..
        }
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(restored.position(7, 22).mandate_id, Some(mandate.id));
}

#[test]
fn mandate_fill_attributes_resting_maker_position_after_snapshot() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    input.objectives = vec![MandateObjective::MinTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    let maker_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(maker_intent, 1_020)
        .expect("maker intent should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("maker resting block should produce");
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(7, 22, 1)),
        Some(&mandate.id)
    );

    let mut restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    restored
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("direct taker should fill resting maker");

    assert_eq!(restored.position(7, 22).mandate_id, Some(mandate.id));
    assert_eq!(restored.position(7, 44).mandate_id, None);
    assert!(!restored
        .resting_order_mandates
        .contains_key(&super::resting_order_key(7, 22, 1)));
    assert!(restored.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            role: OrderFillRole::Maker,
            mandate_id,
            ..
        } if mandate_id == Some(mandate.id)
    )));
    restored
        .produce_empty_block(1_050)
        .expect("objective block should produce");
    assert_eq!(
        restored
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .conformance
            .score,
        100
    );
}

#[test]
fn expired_gtt_order_prunes_resting_mandate_metadata_before_snapshot() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");
    let maker_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtt {
                expires_at_ms: 1_035,
            },
            post_only: false,
        },
    );
    engine
        .submit_intent(maker_intent, 1_020)
        .expect("maker intent should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("maker resting block should produce");
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(7, 22, 1)),
        Some(&mandate.id)
    );

    engine
        .produce_empty_block(1_040)
        .expect("expiry sweep block should produce");
    assert!(!engine
        .resting_order_mandates
        .contains_key(&super::resting_order_key(7, 22, 1)));
    SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should not retain stale mandate metadata");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 99 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("later order should purge expired book entries");
    assert!(engine.resting_order(7, 22, 1, 1_050).is_none());
    SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("post-purge snapshot should stay valid");
}

#[test]
fn terminal_mandate_clears_resting_order_attribution() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");
    let maker_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(maker_intent, 1_020)
        .expect("maker intent should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("maker resting block should produce");
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(7, 22, 1)),
        Some(&mandate.id)
    );

    engine
        .revoke_mandate(mandate.id, "test revoke".into(), 1_040)
        .expect("revoke should clear resting attribution");
    assert!(!engine
        .resting_order_mandates
        .contains_key(&super::resting_order_key(7, 22, 1)));
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("direct taker should fill unattributed resting order");
    assert_eq!(engine.position(7, 22).mandate_id, None);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            role: OrderFillRole::Maker,
            mandate_id: None,
            ..
        }
    )));
}

#[test]
fn post_mode_concentration_counts_mandate_maker_fills() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxConcentrationInMarketPct {
        market: 7,
        pct_bps: 6_000,
    }];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");

    let maker_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(maker_intent, 1_020)
        .expect("maker intent should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("maker resting block should produce");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("direct taker should fill resting maker");
    engine
        .produce_empty_block(1_050)
        .expect("post-mode block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn fill_mode_rolls_back_mixed_mandate_position_attribution() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44, 45] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("first maker order should rest");
    let first_intent = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(first_intent, 1_030)
        .expect("first mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("first fill block should produce");
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 45,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("second maker order should rest");
    let second_intent = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(second_intent, 1_060)
        .expect("second mandate intent should admit");
    engine
        .produce_block_from_mempool(1_070, 10)
        .expect("rollback block should produce");

    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.resting_order(7, 45, 45, 1_080).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: u16::MAX,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == second.id
    )));
}

#[test]
fn fill_mode_allows_sibling_mandate_to_flatten_attributed_position() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44, 45] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker ask should rest");
    let open_intent = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_intent, 1_030)
        .expect("opening mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("opening block should produce");
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 45,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("maker bid should rest");
    let flatten_intent = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(flatten_intent, 1_060)
        .expect("clean flatten should admit");
    engine
        .produce_block_from_mempool(1_070, 10)
        .expect("flatten block should produce");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 22).mandate_id, None);
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            constraint_index: u16::MAX,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == second.id
    )));
}

#[test]
fn mandate_admission_blocks_cross_mandate_resting_order_that_increases_exposure() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker ask should rest");
    let open_long = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_long, 1_030)
        .expect("opening mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("opening block should produce");
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);

    let increasing = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(increasing, 1_050)
        .expect_err("sibling mandate must not rest exposure-increasing order");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "mixed_mandate_resting_order",
        }
    ));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: u16::MAX,
            action: MandateEnforcementAction::AdmissionRejected,
            ..
        } if mandate_id == second.id
    )));
    assert_eq!(
        super::rejection_context_from_node_error(&err).0,
        IntentRejectionReason::MandateConstraintViolated
    );
}

#[test]
fn mandate_admission_allows_cross_mandate_resting_order_only_up_to_flat() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker ask should rest");
    let open_long = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_long, 1_030)
        .expect("opening mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("opening block should produce");

    let reducing = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 110 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(reducing, 1_050)
        .expect("sibling mandate may rest a flattening order");
    engine
        .produce_block_from_mempool(1_060, 10)
        .expect("reducing resting order should commit");
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(7, 22, 2)),
        Some(&second.id)
    );

    let excess_reducing = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 3,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 111 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(excess_reducing, 1_070)
        .expect_err("aggregate sibling resting orders must not exceed flat");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "mixed_mandate_resting_order",
        }
    ));

    let widening_replace = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        2,
        10_000,
        IntentKind::ReplaceOrder {
            market_id: 7,
            target_client_order_id: 2,
            replacement_client_order_id: 4,
            side: IntentOrderSide::Ask,
            quantity_lots: 2,
            kind: IntentOrderKind::Limit { price_ticks: 109 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(widening_replace, 1_080)
        .expect_err("replace must not widen sibling resting order beyond flat");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "mixed_mandate_resting_order",
        }
    ));
}

#[test]
fn preexisting_foreign_mandate_resting_order_rolls_back_on_later_position_conflict() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44, 45] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    let preexisting_bid = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(preexisting_bid, 1_020)
        .expect("flat account may rest a mandate bid");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("preexisting bid should rest");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("maker ask should rest");
    let open_long = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_long, 1_050)
        .expect("primary mandate should open long");
    engine
        .produce_block_from_mempool(1_060, 10)
        .expect("primary fill should commit");
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);

    let mut taker_policy = sample_policy_for_account(45);
    taker_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(taker_policy)
        .expect("taker policy should insert");
    let displacing_taker = signed_intent_for_policy(
        &signer,
        99,
        45,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 45,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(displacing_taker, 1_070)
        .expect("direct taker should admit");
    engine
        .produce_block_from_mempool(1_080, 10)
        .expect("conflicting maker fill should roll back");

    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert_eq!(engine.position(7, 45).net_lots(), 0);
    assert!(engine.resting_order(7, 22, 2, 1_090).is_some());
    assert!(engine.resting_order(7, 45, 45, 1_090).is_none());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: u16::MAX,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == second.id
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            client_order_id: 2,
            mandate_id,
            ..
        } if mandate_id == Some(second.id)
    )));
}

#[test]
fn foreign_mandate_fill_rolls_back_against_terminal_mandate_position() {
    let (mut engine, signer, first, second) = setup_two_mandates_with_long_position();
    engine
        .revoke_mandate(first.id, "terminal attribution test".into(), 1_050)
        .expect("first mandate should revoke");
    assert_eq!(
        engine.mandate(&first.id).map(|mandate| mandate.state),
        Some(MandateState::Revoked)
    );
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 45,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_060,
        )
        .expect("maker ask should rest");

    let increasing_fill = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(increasing_fill, 1_070)
        .expect("fully filling sibling order should admit");
    engine
        .produce_block_from_mempool(1_080, 10)
        .expect("terminal-attribution rollback block should produce");

    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert_eq!(engine.position(7, 45).net_lots(), 0);
    assert!(engine.resting_order(7, 45, 45, 1_090).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: u16::MAX,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == second.id
    )));
}

#[test]
fn mandate_admission_allows_cross_mandate_resting_bid_to_reduce_short() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    for account_id in [22, 44] {
        engine.set_isolated_collateral(7, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker bid should rest");
    let open_short = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_short, 1_030)
        .expect("opening mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("opening block should produce");
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), -1);

    let reducing_bid = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(reducing_bid, 1_050)
        .expect("sibling mandate may rest a short-flattening bid");
    engine
        .produce_block_from_mempool(1_055, 10)
        .expect("reducing bid should commit");

    let increasing_ask = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 3,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 110 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(increasing_ask, 1_060)
        .expect_err("sibling mandate must not rest short-increasing ask");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "mixed_mandate_resting_order",
        }
    ));
}

#[test]
fn mandate_admission_allows_direct_policy_to_rest_against_mandate_position() {
    let (mut engine, signer, first, _second) = setup_two_mandates_with_long_position();
    let mut direct_policy = sample_policy_for_account(22);
    direct_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(direct_policy)
        .expect("direct policy should insert");

    let direct_resting = signed_intent_for_policy(
        &signer,
        99,
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(direct_resting, 1_050)
        .expect("direct policy should bypass mandate attribution admission");
    engine
        .produce_block_from_mempool(1_060, 10)
        .expect("direct resting order should commit");

    assert!(engine.resting_order(7, 22, 99, 1_070).is_some());
    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::IntentRejected {
            reason: IntentRejectionReason::MandateConstraintViolated,
            ..
        }
    )));
}

#[test]
fn mandate_admission_allows_same_mandate_replacing_resting_order() {
    let (mut engine, signer, first, _second) = setup_two_mandates_with_long_position();
    let reducing = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 110 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(reducing, 1_050)
        .expect("same-mandate reducing order should admit");
    engine
        .produce_block_from_mempool(1_060, 10)
        .expect("same-mandate reducing order should rest");

    let widening_replace = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        3,
        10_000,
        IntentKind::ReplaceOrder {
            market_id: 7,
            target_client_order_id: 2,
            replacement_client_order_id: 3,
            side: IntentOrderSide::Ask,
            quantity_lots: 2,
            kind: IntentOrderKind::Limit { price_ticks: 109 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(widening_replace, 1_070)
        .expect("same mandate may replace without cross-mandate attribution gate");
    engine
        .produce_block_from_mempool(1_080, 10)
        .expect("same-mandate replace should commit");

    assert!(engine.resting_order(7, 22, 2, 1_090).is_none());
    assert!(engine.resting_order(7, 22, 3, 1_090).is_some());
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(7, 22, 3)),
        Some(&first.id)
    );
}

#[test]
fn mandate_admission_allows_fully_filling_cross_mandate_order_then_fill_rolls_back() {
    let (mut engine, signer, first, second) = setup_two_mandates_with_long_position();
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 45,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("maker ask should rest");

    let fully_filling = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(fully_filling, 1_060)
        .expect("fully filling order should not hit resting admission gate");
    engine
        .produce_block_from_mempool(1_070, 10)
        .expect("fill-attribution rollback block should produce");

    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.resting_order(7, 45, 45, 1_080).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: u16::MAX,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == second.id
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::IntentRejected {
            reason: IntentRejectionReason::MandateConstraintViolated,
            ..
        }
    )));
}

#[test]
fn mandate_admission_scope_is_per_market_for_cross_mandate_resting_orders() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    list_test_perpetual_market(&mut engine, 8);
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
    }
    for account_id in [22, 44] {
        engine.set_isolated_collateral(7, account_id, 1_000);
        engine.set_isolated_collateral(8, account_id, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.session_keys = vec![signer.verifying_key().to_bytes()];
    first_input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7, 8]),
        },
        MandateConstraint::MaxNotionalTotal {
            limit_usdc: MandateU256::from_u128(1_000_000),
        },
    ];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first mandate should create");
    let mut second_input = sample_mandate_input(22, 2);
    second_input.session_keys = vec![signer.verifying_key().to_bytes()];
    second_input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7, 8]),
        },
        MandateConstraint::MaxNotionalTotal {
            limit_usdc: MandateU256::from_u128(1_000_000),
        },
    ];
    let second = engine
        .create_mandate(second_input, 1_011)
        .expect("second mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("market 7 maker ask should rest");
    let open_long = signed_intent_for_policy(
        &signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open_long, 1_030)
        .expect("market 7 opening mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("market 7 opening block should produce");

    let market8_order = signed_intent_for_policy(
        &signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 8,
            client_order_id: 8,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 90 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(market8_order, 1_050)
        .expect("cross-market sibling order should not hit market 7 attribution gate");
    engine
        .produce_block_from_mempool(1_060, 10)
        .expect("cross-market sibling order should commit");

    assert_eq!(engine.position(7, 22).mandate_id, Some(first.id));
    assert_eq!(engine.position(8, 22).mandate_id, None);
    assert!(engine.resting_order(8, 22, 8, 1_070).is_some());
    assert_eq!(
        engine
            .resting_order_mandates
            .get(&super::resting_order_key(8, 22, 8)),
        Some(&second.id)
    );
}

#[test]
fn block_end_evaluates_mandate_trade_count_objectives() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![MandateObjective::MinTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");
    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(intent, 1_030)
        .expect("mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("objective evaluation block should produce");

    let updated = engine.mandate(&mandate.id).expect("mandate should exist");
    assert_eq!(updated.conformance.score, 100);
    assert_eq!(updated.conformance.pending_objectives, 0);
    assert_eq!(updated.conformance.last_evaluated_at_ms, 1_040);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateObjectiveEvaluated {
            mandate_id,
            agent: 22,
            objective_index: 0,
            score: 100,
            conformance_score: 100,
            pending_objectives: 0,
            evaluated_at_ms: 1_040,
        } if mandate_id == mandate.id && event.block_height == Some(1)
    )));
    let objective_event_count = engine
        .runtime_events()
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::MandateObjectiveEvaluated {
                    mandate_id,
                    ..
                } if mandate_id == mandate.id
            )
        })
        .count();

    engine
        .produce_block_from_mempool(1_050, 10)
        .expect("empty objective block should produce");

    let unchanged = engine.mandate(&mandate.id).expect("mandate should exist");
    assert_eq!(unchanged.conformance.last_evaluated_at_ms, 1_040);
    assert_eq!(
        engine
            .runtime_events()
            .iter()
            .filter(|event| {
                matches!(
                    event.kind,
                    RuntimeEventKind::MandateObjectiveEvaluated {
                        mandate_id,
                        ..
                    } if mandate_id == mandate.id
                )
            })
            .count(),
        objective_event_count
    );
    assert!(engine.mandate_fill_timestamps.contains_key(&mandate.id));
    assert!(engine
        .mandate_objective_event_cache
        .contains_key(&(mandate.id, 0)));

    engine
        .revoke_mandate(mandate.id, "test cleanup".into(), 1_060)
        .expect("revoke should succeed");
    assert!(!engine.mandate_fill_timestamps.contains_key(&mandate.id));
    assert!(!engine
        .mandate_objective_event_cache
        .contains_key(&(mandate.id, 0)));
}

#[test]
fn restored_mandate_trade_count_index_counts_prior_fills() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![MandateObjective::MaxTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("first maker order should rest");
    let first_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(first_intent, 1_030)
        .expect("first mandate intent should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("first objective block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .conformance
            .score,
        100
    );

    let mut restored = SingleNodeEngine::from_snapshot(engine.snapshot());
    restored
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_041,
        )
        .expect("second maker order should rest");
    let second_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    restored
        .submit_intent(second_intent, 1_042)
        .expect("second mandate intent should admit");
    restored
        .produce_block_from_mempool(1_045, 10)
        .expect("second objective block should produce");

    let updated = restored.mandate(&mandate.id).expect("mandate should exist");
    assert_eq!(updated.conformance.score, 50);
    assert!(restored.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateObjectiveEvaluated {
            mandate_id,
            agent: 22,
            objective_index: 0,
            score: 50,
            conformance_score: 50,
            pending_objectives: 0,
            evaluated_at_ms: 1_045,
        } if mandate_id == mandate.id && event.block_height == Some(2)
    )));

    restored
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 46,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("third maker order should rest");
    let third_intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        3,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 3,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    restored
        .submit_intent(third_intent, 1_051)
        .expect("third mandate intent should admit");
    restored
        .produce_block_from_mempool(1_056, 10)
        .expect("third objective block should produce");

    assert_eq!(
        restored
            .mandate_fill_timestamps
            .get(&mandate.id)
            .map(Vec::as_slice),
        Some(&[1_056][..])
    );
}

#[test]
fn mandate_objective_pending_to_scored_transition_emits_event() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![MandateObjective::MinTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    engine
        .produce_block_from_mempool(1_015, 10)
        .expect("pending objective block should produce");
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateObjectiveEvaluated {
            mandate_id,
            agent: 22,
            objective_index: 0,
            score: 50,
            conformance_score: 50,
            pending_objectives: 1,
            evaluated_at_ms: 1_015,
        } if mandate_id == mandate.id && event.block_height == Some(1)
    )));

    engine
        .produce_block_from_mempool(1_020, 10)
        .expect("scored objective block should produce");
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateObjectiveEvaluated {
            mandate_id,
            agent: 22,
            objective_index: 0,
            score: 0,
            conformance_score: 0,
            pending_objectives: 0,
            evaluated_at_ms: 1_020,
        } if mandate_id == mandate.id && event.block_height == Some(2)
    )));
}

#[test]
fn mandate_conformance_history_records_first_delta_and_skips_unchanged() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![MandateObjective::MinTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    engine
        .produce_block_from_mempool(1_015, 10)
        .expect("pending objective block should produce");
    let samples = engine.mandate_conformance_history_samples(mandate.id);
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].block_height, 1);
    assert_eq!(samples[0].evaluated_at_ms, 1_015);
    assert_eq!(samples[0].score, 50);
    assert_eq!(samples[0].pending_objectives, 1);
    assert_eq!(samples[0].state, MandateState::Active);
    assert_eq!(samples[0].prev_score, None);
    assert_eq!(samples[0].delta, None);
    assert_eq!(engine.latest_mandate_conformance_delta(&mandate.id), None);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MandateConformanceEvaluated {
            mandate_id,
            agent: 22,
            evaluated_at_ms: 1_015,
            score: 50,
            pending_objectives: 1,
            state,
            prev_score: None,
            delta: None,
        } if *mandate_id == mandate.id && state == "active" && event.block_height == Some(1)
    )));

    engine
        .produce_block_from_mempool(1_016, 10)
        .expect("unchanged pending block should produce");
    assert_eq!(
        engine.mandate_conformance_history_samples(mandate.id).len(),
        1
    );

    engine
        .produce_block_from_mempool(1_020, 10)
        .expect("scored objective block should produce");
    let samples = engine.mandate_conformance_history_samples(mandate.id);
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[1].score, 0);
    assert_eq!(samples[1].pending_objectives, 0);
    assert_eq!(samples[1].prev_score, Some(50));
    assert_eq!(samples[1].delta, Some(-50));
    assert_eq!(
        engine.latest_mandate_conformance_delta(&mandate.id),
        Some(-50)
    );
    assert!(samples.iter().all(|sample| sample.score <= 100));
    assert!(samples
        .iter()
        .filter_map(|sample| sample.delta)
        .all(|delta| (-100..=100).contains(&delta)));
}

#[test]
fn mandate_conformance_history_emits_once_for_multiple_changed_objectives() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![
        MandateObjective::MinTradeCount {
            count: 1,
            over_ms: 10,
        },
        MandateObjective::MaxTradeCount {
            count: 1,
            over_ms: 10,
        },
    ];
    input.objective_weights = vec![50, 50];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    engine
        .produce_block_from_mempool(1_015, 10)
        .expect("objective evaluation block should produce");

    let objective_events = engine
        .runtime_events()
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::MandateObjectiveEvaluated {
                    mandate_id,
                    ..
                } if mandate_id == mandate.id
            )
        })
        .count();
    let conformance_events = engine
        .runtime_events()
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                RuntimeEventKind::MandateConformanceEvaluated {
                    mandate_id,
                    ..
                } if mandate_id == mandate.id
            )
        })
        .count();

    assert_eq!(objective_events, 2);
    assert_eq!(conformance_events, 1);
    let samples = engine.mandate_conformance_history_samples(mandate.id);
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].score, 50);
    assert_eq!(samples[0].pending_objectives, 2);
}

#[test]
fn mandate_conformance_history_events_match_ring_buffer_across_breach_recover() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![MandateObjective::MaxTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");

    for (nonce, client_order_id, timestamp_ms) in [(1_u64, 1_u64, 1_040_u64), (2, 2, 1_045)] {
        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id: 7,
                    account_id: 44,
                    client_order_id: 100 + client_order_id,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots: 1,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                timestamp_ms.saturating_sub(2),
            )
            .expect("maker order should rest");
        let intent = signed_intent_for_policy(
            &signer,
            mandate.policies[0],
            22,
            nonce,
            10_000,
            IntentKind::PlaceOrder {
                market_id: 7,
                client_order_id,
                side: IntentOrderSide::Bid,
                quantity_lots: 1,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        );
        engine
            .submit_intent(intent, timestamp_ms.saturating_sub(1))
            .expect("mandate intent should admit");
        engine
            .produce_block_from_mempool(timestamp_ms, 10)
            .expect("fill block should produce");
    }
    engine
        .produce_block_from_mempool(1_056, 10)
        .expect("recovery block should produce");

    let samples = engine.mandate_conformance_history_samples(mandate.id);
    assert_eq!(samples.len(), 3);
    assert_eq!(
        samples
            .iter()
            .map(|sample| (sample.score, sample.delta))
            .collect::<Vec<_>>(),
        vec![(100, None), (50, Some(-50)), (100, Some(50))]
    );

    let events = engine
        .runtime_events()
        .iter()
        .filter_map(|event| {
            if let RuntimeEventKind::MandateConformanceEvaluated {
                mandate_id,
                evaluated_at_ms,
                score,
                pending_objectives,
                state,
                prev_score,
                delta,
                ..
            } = &event.kind
            {
                if *mandate_id == mandate.id {
                    return Some((
                        event.block_height,
                        *evaluated_at_ms,
                        *score,
                        *pending_objectives,
                        state.clone(),
                        *prev_score,
                        *delta,
                    ));
                }
            }
            None
        })
        .collect::<Vec<_>>();
    let sample_projection = samples
        .iter()
        .map(|sample| {
            (
                Some(sample.block_height),
                sample.evaluated_at_ms,
                sample.score,
                sample.pending_objectives,
                "active".to_string(),
                sample.prev_score,
                sample.delta,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(events, sample_projection);
}

#[test]
fn mandate_conformance_history_prunes_to_ring_limit() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate_id = [9_u8; 32];
    let rows = (0..(MANDATE_CONFORMANCE_HISTORY_LIMIT + 2))
        .map(|index| {
            (
                mandate_id,
                MandateConformanceSample {
                    mandate_id,
                    block_height: index as u64,
                    evaluated_at_ms: 1_000 + index as u64,
                    score: (index % 101) as u8,
                    pending_objectives: 0,
                    state: MandateState::Active,
                    prev_score: None,
                    delta: None,
                },
            )
        })
        .collect::<Vec<_>>();
    engine.set_mandate_conformance_history_samples(rows);
    engine.prune_mandate_conformance_history(mandate_id);

    let samples = engine.mandate_conformance_history_samples(mandate_id);
    assert_eq!(samples.len(), MANDATE_CONFORMANCE_HISTORY_LIMIT);
    assert_eq!(samples[0].block_height, 2);
    assert_eq!(
        samples.last().map(|sample| sample.block_height),
        Some((MANDATE_CONFORMANCE_HISTORY_LIMIT + 1) as u64)
    );
}

#[test]
fn mandate_conformance_history_file_snapshot_restore_starts_empty() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.objectives = vec![MandateObjective::MinTradeCount {
        count: 1,
        over_ms: 10,
    }];
    input.objective_weights = vec![100];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("mandate should create");
    engine
        .produce_block_from_mempool(1_015, 10)
        .expect("objective block should produce");
    assert_eq!(
        engine.mandate_conformance_history_samples(mandate.id).len(),
        1
    );

    let mut restored = SingleNodeEngine::from_snapshot(engine.snapshot());
    assert!(restored
        .all_mandate_conformance_history_samples()
        .is_empty());
    restored
        .produce_block_from_mempool(1_016, 10)
        .expect("restored block should produce");
    assert!(restored
        .all_mandate_conformance_history_samples()
        .is_empty());
}

#[test]
fn generated_mandate_policy_enforces_supported_admission_constraints() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(50),
    });
    input
        .constraints
        .push(MandateConstraint::MaxTradeRatePerMinute { count: 1 });
    let mandate = engine
        .create_mandate(input, 1_000)
        .expect("mandate should create");
    let policy_id = mandate.policies[0];

    let admission =
        |nonce, timestamp_ms, market_id, intent_notional, notional_outstanding| PolicyAdmission {
            signed_session: SignedSessionPayload {
                payload: SessionPayload {
                    intent_hash: [nonce as u8; 32],
                    policy_id,
                    chain_id: 7,
                },
                signature: signer
                    .sign(
                        &borsh::to_vec(&SessionPayload {
                            intent_hash: [nonce as u8; 32],
                            policy_id,
                            chain_id: 7,
                        })
                        .expect("session payload should serialize"),
                    )
                    .to_bytes(),
            },
            intent_nonce: nonce,
            block_number: nonce,
            expected_chain_id: 7,
            timestamp_ms,
            gas_cost: 10,
            volume: intent_notional,
            notional_outstanding,
            intent_notional,
            market_id,
            realized_pnl_delta: 0,
            current_drawdown_pct: 0,
        };

    let err = engine
        .policy_store()
        .preview_admission(policy_id, &admission(1, 1_100, Some(8), 10, 10))
        .expect_err("disallowed market should reject");
    assert_eq!(err, PolicyError::MarketNotAllowed);
    let err = engine
        .policy_store()
        .preview_admission(policy_id, &admission(1, 1_100, Some(7), 51, 51))
        .expect_err("oversized trade should reject");
    assert_eq!(err, PolicyError::TradeNotionalExceeded);
    let err = engine
        .policy_store()
        .preview_admission(policy_id, &admission(1, 1_100, Some(7), 50, 1_000_001))
        .expect_err("notional cap should reject");
    assert_eq!(err, PolicyError::NotionalOutstandingExceeded);

    engine
        .policy_store_mut()
        .admit(policy_id, &admission(1, 1_100, Some(7), 50, 50))
        .expect("first admission should fit");
    let err = engine
        .policy_store_mut()
        .admit(policy_id, &admission(2, 2_100, Some(7), 10, 60))
        .expect_err("same-minute admission should reject");
    assert_eq!(err, PolicyError::MinuteIntentLimitExceeded);
}

#[test]
fn scaled_limit_order_notional_is_used_for_mandate_admission() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 30, 1_001);
    engine.update_market_mark_price(7, 30, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(50),
    });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("scaled admission mandate should create");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 30 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(intent, 1_020)
        .expect_err("scaled limit notional should reject at admission");
    assert_eq!(err, NodeError::Policy(PolicyError::TradeNotionalExceeded));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::AdmissionRejected,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn scaled_replace_order_notional_is_used_for_mandate_admission() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 30, 1_001);
    engine.update_market_mark_price(7, 30, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints.push(MandateConstraint::MaxTradeSizeUsdc {
        limit_usdc: MandateU256::from_u128(50),
    });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("scaled replace mandate should create");

    let initial = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 20 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(initial, 1_020)
        .expect("initial scaled order should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("initial order should rest");

    let replacement = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::ReplaceOrder {
            market_id: 7,
            target_client_order_id: 1,
            replacement_client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 30 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(replacement, 1_040)
        .expect_err("scaled replace notional should reject at admission");
    assert_eq!(err, NodeError::Policy(PolicyError::TradeNotionalExceeded));
    assert!(engine.resting_order(7, 22, 1, 1_050).is_some());
    assert!(engine.resting_order(7, 22, 2, 1_050).is_none());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::AdmissionRejected,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn max_leverage_rejects_oversized_limit_order_at_admission() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 60, 1_001);
    engine.update_market_mark_price(7, 60, 1_001);
    engine.set_isolated_collateral(7, 22, 50);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 2 });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("max leverage mandate should create");

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 2,
            kind: IntentOrderKind::Limit { price_ticks: 60 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(intent, 1_020)
        .expect_err("order above mandate leverage should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "max_leverage_exceeded",
        }
    ));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::AdmissionRejected,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn max_leverage_rolls_back_market_fill_over_limit() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 60, 1_001);
    engine.update_market_mark_price(7, 60, 1_001);
    engine.set_isolated_collateral(7, 22, 50);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 2 });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("max leverage fill mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 2,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 60 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");

    let taker = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 2,
            kind: IntentOrderKind::Market,
            time_in_force: IntentTimeInForce::Ioc,
            post_only: false,
        },
    );
    engine
        .submit_intent(taker, 1_030)
        .expect("market order leverage check is fill-time authoritative");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("over-levered fill should roll back");

    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert_eq!(engine.position(7, 44).net_lots(), 0);
    assert!(engine.resting_order(7, 44, 44, 1_050).is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::FillRolledBack,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::OrderFilled {
            market_id: 7,
            account_id: 22,
            ..
        }
    )));
}

#[test]
fn max_leverage_rejects_widening_replace_order_at_admission() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 60, 1_001);
    engine.update_market_mark_price(7, 60, 1_001);
    engine.set_isolated_collateral(7, 22, 50);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 2 });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("max leverage replace mandate should create");

    let initial = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 40 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(initial, 1_020)
        .expect("initial order should admit");
    engine
        .produce_block_from_mempool(1_030, 10)
        .expect("initial order should rest");

    let replacement = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::ReplaceOrder {
            market_id: 7,
            target_client_order_id: 1,
            replacement_client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 2,
            kind: IntentOrderKind::Limit { price_ticks: 60 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(replacement, 1_040)
        .expect_err("widening replace should reject on leverage");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "max_leverage_exceeded",
        }
    ));
    assert!(engine.resting_order(7, 22, 1, 1_050).is_some());
    assert!(engine.resting_order(7, 22, 2, 1_050).is_none());
}

#[test]
fn max_leverage_resting_batch_accumulates_candidate_exposure() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 100);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 2 });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("max leverage batch mandate should create");

    let first = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 150 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let second = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let items = vec![BlockItem::Intent(first), BlockItem::Intent(second)];
    let mut policy_store = engine.policy_store.clone();
    let mut order_books = engine.order_books.clone();
    let mut resting_order_mandates = engine.resting_order_mandates.clone();
    let mut stats = super::RestingBatchCommitStats::default();

    let err = super::try_commit_resting_place_order_batch(
        &mut policy_store,
        &engine.mandates,
        &engine.delegations,
        &mut order_books,
        &mut resting_order_mandates,
        &engine.isolated_collateral,
        &engine.frozen_accounts,
        &engine.markets,
        &engine.paused_markets,
        &engine.market_marks,
        &engine.market_index_prices,
        &engine.market_oracle_config,
        &engine.positions,
        7,
        &items,
        0,
        1,
        1_020,
        super::PolicyAdmissionCommitMode::PreverifiedSession,
        &mut stats,
    )
    .expect_err("cumulative batch exposure should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "max_leverage_exceeded",
        }
    ));
    assert!(order_books
        .get(&7)
        .is_none_or(|book| book.bids().is_empty()));
}

#[test]
fn max_leverage_stacks_across_sibling_mandates() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 150, 1_001);
    engine.update_market_mark_price(7, 150, 1_001);
    engine.set_isolated_collateral(7, 22, 50);

    let mut wide_input = sample_mandate_input(22, 1);
    wide_input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 5 });
    wide_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let wide = engine
        .create_mandate(wide_input, 1_010)
        .expect("wide leverage mandate should create");
    let mut tight_input = sample_mandate_input(22, 2);
    tight_input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 2 });
    tight_input.session_keys = vec![signer.verifying_key().to_bytes()];
    let tight = engine
        .create_mandate(tight_input, 1_011)
        .expect("tight leverage mandate should create");

    let intent = signed_intent_for_policy(
        &signer,
        wide.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 150 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let err = engine
        .submit_intent(intent, 1_020)
        .expect_err("tight sibling mandate should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "max_leverage_exceeded",
        }
    ));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 2,
            action: MandateEnforcementAction::AdmissionRejected,
            ..
        } if mandate_id == tight.id
    )));
}

#[test]
fn max_leverage_allows_direct_policy_bypass() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 60, 1_001);
    engine.update_market_mark_price(7, 60, 1_001);
    engine.set_isolated_collateral(7, 22, 50);

    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::MaxLeverage { mult: 2 });
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    engine
        .create_mandate(input, 1_010)
        .expect("max leverage mandate should create");
    let mut direct_policy = sample_policy_for_account(22);
    direct_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(direct_policy)
        .expect("direct policy should insert");

    let direct = signed_intent_for_policy(
        &signer,
        99,
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 2,
            kind: IntentOrderKind::Limit { price_ticks: 60 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(direct, 1_020)
        .expect("direct policy should bypass mandate leverage gate");
}

#[test]
fn suspend_and_resume_mandate_controls_generated_policy_admission() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = engine
        .create_mandate(sample_mandate_input(22, 1), 1_000)
        .expect("mandate should create");
    let mandate_id = mandate.id;
    let policy_id = mandate.policies[0];

    let admission = |nonce, timestamp_ms| PolicyAdmission {
        signed_session: SignedSessionPayload {
            payload: SessionPayload {
                intent_hash: [nonce as u8; 32],
                policy_id,
                chain_id: 7,
            },
            signature: signer
                .sign(
                    &borsh::to_vec(&SessionPayload {
                        intent_hash: [nonce as u8; 32],
                        policy_id,
                        chain_id: 7,
                    })
                    .expect("session payload should serialize"),
                )
                .to_bytes(),
        },
        intent_nonce: nonce,
        block_number: nonce,
        expected_chain_id: 7,
        timestamp_ms,
        gas_cost: 10,
        volume: 10,
        notional_outstanding: 10,
        intent_notional: 10,
        market_id: Some(7),
        realized_pnl_delta: 0,
        current_drawdown_pct: 0,
    };

    let suspended = engine
        .suspend_mandate(mandate_id, 1_100)
        .expect("mandate should suspend");
    assert_eq!(suspended.state, MandateState::Suspended);
    assert_eq!(
        engine
            .policy_store()
            .get(policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Suspended
    );
    let err = engine
        .policy_store()
        .preview_admission(policy_id, &admission(1, 1_200))
        .expect_err("suspended mandate policy should not admit");
    assert_eq!(err, PolicyError::PolicyNotActive(PolicyState::Suspended));

    let resumed = engine
        .resume_mandate(mandate_id, 1_300)
        .expect("mandate should resume");
    assert_eq!(resumed.state, MandateState::Active);
    assert_eq!(
        engine
            .policy_store()
            .get(policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Active
    );
    engine
        .policy_store()
        .preview_admission(policy_id, &admission(1, 1_400))
        .expect("resumed mandate policy should admit");
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateSuspended {
            mandate_id: id,
            agent: 22,
            ..
        } if id == mandate_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateResumed {
            mandate_id: id,
            agent: 22,
            ..
        } if id == mandate_id
    )));
}

#[test]
fn revoke_and_expire_mandates_terminally_disable_generated_policies() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let revoked = engine
        .create_mandate(sample_mandate_input(22, 1), 1_000)
        .expect("mandate should create");
    let revoked_policy_id = revoked.policies[0];
    let revoked = engine
        .revoke_mandate(revoked.id, "owner_revoked".into(), 1_100)
        .expect("mandate should revoke");
    assert_eq!(revoked.state, MandateState::Revoked);
    assert_eq!(
        engine
            .policy_store()
            .get(revoked_policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Revoked
    );

    let mut expiring_input = sample_mandate_input(33, 1);
    expiring_input.expires_at = 2_000;
    let expiring = engine
        .create_mandate(expiring_input, 1_200)
        .expect("expiring mandate should create");
    let expired_policy_id = expiring.policies[0];
    let expired = engine
        .expire_due_mandates(2_000)
        .expect("due mandates should expire");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, expiring.id);
    assert_eq!(expired[0].state, MandateState::Expired);
    assert_eq!(
        engine
            .policy_store()
            .get(expired_policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Expired
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MandateRevoked {
            mandate_id,
            reason,
            ..
        } if *mandate_id == revoked.id && reason == "owner_revoked"
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::PolicyRevoked {
            policy_id,
            account_id: 22,
            ..
        } if policy_id == revoked_policy_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateExpired {
            mandate_id,
            agent: 33,
            ..
        } if mandate_id == expiring.id
    )));
}

#[test]
fn breach_mandate_terminally_disables_generated_policies_and_emits_events() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = engine
        .create_mandate(sample_mandate_input(22, 1), 1_000)
        .expect("mandate should create");
    let policy_id = mandate.policies[0];

    let breached = engine
        .breach_mandate(mandate.id, 0, 1_200)
        .expect("mandate should breach");

    assert_eq!(breached.state, MandateState::Breached);
    assert_eq!(
        engine
            .policy_store()
            .get(policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Revoked
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 0,
            action: MandateEnforcementAction::BreachApplied,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateBreached {
            mandate_id,
            agent: 22,
            constraint_index: 0,
            ..
        } if mandate_id == mandate.id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::PolicyRevoked {
            account_id: 22,
            policy_id: revoked_policy_id,
            ..
        } if revoked_policy_id == policy_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 0,
            action: MandateEnforcementAction::PoliciesCascadeRevoked,
            ..
        } if mandate_id == mandate.id
    )));

    let terminal_err = engine
        .breach_mandate(mandate.id, 0, 1_300)
        .expect_err("terminal mandate should not breach twice");
    assert_eq!(
        terminal_err,
        NodeError::InvalidMandate {
            agent: 22,
            reason: "mandate_terminal",
        }
    );

    let other = engine
        .create_mandate(sample_mandate_input(33, 1), 1_400)
        .expect("second mandate should create");
    let index_err = engine
        .breach_mandate(other.id, 99, 1_500)
        .expect_err("invalid constraint index should reject");
    assert_eq!(
        index_err,
        NodeError::InvalidMandate {
            agent: 33,
            reason: "invalid_constraint_index",
        }
    );
}

#[test]
fn post_mode_concentration_breach_revokes_mandate_at_block_end() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    list_test_perpetual_market(&mut engine, 8);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.update_market_index_price(8, 100, 1_001);
    engine.update_market_mark_price(8, 100, 1_001);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxConcentrationInMarketPct {
        market: 7,
        pct_bps: 6_000,
    }];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");
    let policy_id = mandate.policies[0];
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(8, 22, 1_000);

    engine.positions.insert(
        (7, 22),
        PositionLedger {
            open_net_lots: 5,
            open_notional: 500,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_020),
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (8, 22),
        PositionLedger {
            open_net_lots: 5,
            open_notional: 500,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_020),
            ..PositionLedger::default()
        },
    );
    engine
        .produce_empty_block(1_100)
        .expect("healthy concentration block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );
    assert_eq!(
        engine
            .policy_store()
            .get(policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Active
    );

    engine.positions.insert(
        (7, 22),
        PositionLedger {
            open_net_lots: 8,
            open_notional: 800,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_020),
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (8, 22),
        PositionLedger {
            open_net_lots: 2,
            open_notional: 200,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_020),
            ..PositionLedger::default()
        },
    );
    engine
        .produce_empty_block(1_200)
        .expect("breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
    assert_eq!(
        engine
            .policy_store()
            .get(policy_id)
            .expect("generated policy should exist")
            .state,
        PolicyState::Revoked
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent: 22,
            constraint_index: 0,
            action: MandateEnforcementAction::BreachApplied,
            ..
        } if mandate_id == mandate.id && event.block_height == Some(2)
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateBreached {
            mandate_id,
            agent: 22,
            constraint_index: 0,
            ..
        } if mandate_id == mandate.id && event.block_height == Some(2)
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(
        restored
            .mandate(&mandate.id)
            .expect("mandate should restore")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_concentration_ignores_unattributed_direct_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    list_test_perpetual_market(&mut engine, 8);
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
    }

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxConcentrationInMarketPct {
        market: 7,
        pct_bps: 1_000,
    }];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");

    engine.positions.insert(
        (7, 22),
        PositionLedger {
            open_net_lots: 1_000,
            open_notional: 100_000,
            mandate_id: None,
            opened_at_timestamp_ms: Some(1_020),
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (8, 22),
        PositionLedger {
            open_net_lots: 1,
            open_notional: 100,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_020),
            ..PositionLedger::default()
        },
    );
    engine
        .produce_empty_block(1_100)
        .expect("unattributed direct position should not breach mandate");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );
}

#[test]
fn post_mode_concentration_counts_delegation_trading_account_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    list_test_perpetual_market(&mut engine, 8);
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
    }

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxConcentrationInMarketPct {
        market: 7,
        pct_bps: 6_000,
    }];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_020,
        )
        .expect("delegation should open");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    engine.positions.insert(
        (7, trading_account),
        PositionLedger {
            open_net_lots: 8,
            open_notional: 800,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (8, trading_account),
        PositionLedger {
            open_net_lots: 2,
            open_notional: 200,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );

    engine
        .produce_empty_block(1_100)
        .expect("delegation concentration breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_concentration_aggregates_delegated_and_direct_accounts() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    list_test_perpetual_market(&mut engine, 8);
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
    }

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxConcentrationInMarketPct {
        market: 7,
        pct_bps: 6_000,
    }];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let first = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_020,
        )
        .expect("first delegation should open");
    let first_account = delegation_trading_account_id(&first.delegation_id);
    let second_account = 22;
    engine.positions.insert(
        (7, first_account),
        PositionLedger {
            open_net_lots: 4,
            open_notional: 400,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (7, second_account),
        PositionLedger {
            open_net_lots: 4,
            open_notional: 400,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (8, first_account),
        PositionLedger {
            open_net_lots: 5,
            open_notional: 500,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );

    engine
        .produce_empty_block(1_100)
        .expect("multi-delegation concentration breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_concentration_uses_market_scaled_fill_notional() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let mut large_lot_config = btc_perp_config();
    large_lot_config.lot_size_microunits = 20_000_000;
    engine
        .list_perpetual_market(8, "ETH".into(), 1, 78, large_lot_config, 1_000)
        .expect("second market should list");
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
    }
    for account_id in [22, 44, 45, 46] {
        engine.set_isolated_collateral(7, account_id, 1_000_000);
        engine.set_isolated_collateral(8, account_id, 1_000_000);
    }

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7, 8]),
        },
        MandateConstraint::MaxConcentrationInMarketPct {
            market: 7,
            pct_bps: 1_000,
        },
    ];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");

    for (market_id, maker, client_order_id, quantity_lots) in
        [(7, 44, 44, 5), (8, 45, 45, 5), (7, 46, 46, 1_000)]
    {
        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id,
                    account_id: maker,
                    client_order_id,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_020 + client_order_id,
            )
            .expect("maker order should rest");
    }

    for (nonce, market_id, client_order_id, quantity_lots) in [(1, 7, 1, 5), (2, 8, 2, 5)] {
        let intent = signed_intent_for_policy(
            &signer,
            mandate.policies[0],
            22,
            nonce,
            10_000,
            IntentKind::PlaceOrder {
                market_id,
                client_order_id,
                side: IntentOrderSide::Bid,
                quantity_lots,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        );
        engine
            .submit_intent(intent, 1_100 + nonce)
            .expect("mandate intent should admit");
    }
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("scaled healthy block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );

    let oversized = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        3,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 3,
            side: IntentOrderSide::Bid,
            quantity_lots: 1_000,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(oversized, 1_300)
        .expect("breaching mandate intent should admit");
    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("scaled breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateBreached {
            mandate_id,
            agent: 22,
            constraint_index: 1,
            ..
        } if mandate_id == mandate.id
    )));
}

#[test]
fn post_mode_concentration_uses_market_tick_size_scaling() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let mut wide_tick_config = btc_perp_config();
    wide_tick_config.tick_size_bps = 200_000;
    engine
        .list_perpetual_market(8, "ETH".into(), 1, 78, wide_tick_config, 1_000)
        .expect("second market should list");
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
    }
    for account_id in [22, 44, 45, 46] {
        engine.set_isolated_collateral(7, account_id, 1_000_000);
        engine.set_isolated_collateral(8, account_id, 1_000_000);
    }

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7, 8]),
        },
        MandateConstraint::MaxConcentrationInMarketPct {
            market: 7,
            pct_bps: 1_000,
        },
    ];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("post-mode mandate should create");

    for (market_id, maker, client_order_id, quantity_lots) in
        [(7, 44, 44, 5), (8, 45, 45, 5), (7, 46, 46, 1_000)]
    {
        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id,
                    account_id: maker,
                    client_order_id,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_020 + client_order_id,
            )
            .expect("maker order should rest");
    }

    for (nonce, market_id, client_order_id, quantity_lots) in [(1, 7, 1, 5), (2, 8, 2, 5)] {
        let intent = signed_intent_for_policy(
            &signer,
            mandate.policies[0],
            22,
            nonce,
            10_000,
            IntentKind::PlaceOrder {
                market_id,
                client_order_id,
                side: IntentOrderSide::Bid,
                quantity_lots,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        );
        engine
            .submit_intent(intent, 1_100 + nonce)
            .expect("mandate intent should admit");
    }
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("scaled healthy block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );

    let oversized = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        3,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 3,
            side: IntentOrderSide::Bid,
            quantity_lots: 1_000,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(oversized, 1_300)
        .expect("breaching mandate intent should admit");
    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("scaled breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_holding_period_breaches_at_block_end() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7]),
        },
        MandateConstraint::NoPositionHeldLongerThan { duration_ms: 100 },
    ];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("holding-period mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");
    let open = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open, 1_030)
        .expect("mandate open should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("open block should produce");
    let opened_position = engine.position(7, 22);
    assert_eq!(opened_position.mandate_id, Some(mandate.id));
    assert_eq!(opened_position.opened_at_timestamp_ms, Some(1_040));

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_080,
        )
        .expect("second maker order should rest");
    let top_up = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(top_up, 1_100)
        .expect("same-mandate top-up should admit");
    engine
        .produce_block_from_mempool(1_120, 10)
        .expect("top-up block should produce");
    let topped_up_position = engine.position(7, 22);
    assert_eq!(topped_up_position.opened_at_timestamp_ms, Some(1_040));
    assert_eq!(topped_up_position.last_fill_timestamp_ms, Some(1_120));

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );

    engine
        .produce_empty_block(1_200)
        .expect("expired holding window should breach");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateBreached {
            mandate_id,
            agent: 22,
            constraint_index: 1,
            ..
        } if mandate_id == mandate.id && event.block_height == Some(3)
    )));
}

#[test]
fn post_mode_holding_period_ignores_unattributed_direct_positions() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::NoPositionHeldLongerThan { duration_ms: 1 }];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("holding-period mandate should create");
    let mut direct_policy = sample_policy_for_account(22);
    direct_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(direct_policy)
        .expect("direct policy should insert");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");
    let direct = signed_intent_for_policy(
        &signer,
        99,
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(direct, 1_030)
        .expect("direct open should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("direct open block should produce");
    assert_eq!(engine.position(7, 22).mandate_id, None);

    engine
        .produce_empty_block(1_200)
        .expect("direct position should not breach mandate");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );
}

#[test]
fn post_mode_holding_period_counts_delegation_trading_account_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::NoPositionHeldLongerThan { duration_ms: 100 }];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("holding-period mandate should create");
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_020,
        )
        .expect("delegation should open");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    engine.positions.insert(
        (7, trading_account),
        PositionLedger {
            open_net_lots: 1,
            open_notional: 100,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );

    engine
        .produce_empty_block(1_200)
        .expect("delegation holding-period breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_daily_loss_breaches_on_unrealized_day_loss() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7]),
        },
        MandateConstraint::MaxDailyLossUsdc {
            limit_usdc: MandateU256::from_u128(10),
        },
    ];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    // Keep the mandate live across the UTC-day boundary used by this test.
    input.expires_at = UTC_DAY_MS * 3;
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("daily-loss mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");
    let open = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        UTC_DAY_MS * 3,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open, 1_030)
        .expect("mandate open should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("open block should produce");
    engine
        .produce_empty_block(1_100)
        .expect("baseline block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );

    engine.update_market_mark_price(7, 89, 1_120);
    engine
        .produce_empty_block(1_130)
        .expect("loss block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MandateBreached {
            mandate_id,
            agent: 22,
            constraint_index: 1,
            ..
        } if mandate_id == mandate.id && event.block_height == Some(3)
    )));
}

#[test]
fn post_mode_daily_loss_counts_realized_loss_after_flattening() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7]),
        },
        MandateConstraint::MaxDailyLossUsdc {
            limit_usdc: MandateU256::from_u128(10),
        },
    ];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("daily-loss mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker ask should rest");
    let open = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open, 1_030)
        .expect("mandate open should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("open block should produce");
    engine
        .produce_empty_block(1_100)
        .expect("baseline block should produce");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 45,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 80 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_110,
        )
        .expect("maker bid should rest");
    let close = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 2,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 80 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(close, 1_120)
        .expect("mandate close should admit");
    engine
        .produce_block_from_mempool(1_130, 10)
        .expect("realized loss block should produce");

    assert_eq!(engine.position(7, 22).mandate_id, None);
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_daily_loss_ignores_unattributed_direct_positions() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxDailyLossUsdc {
        limit_usdc: MandateU256::from_u128(1),
    }];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("daily-loss mandate should create");
    let mut direct_policy = sample_policy_for_account(22);
    direct_policy.id = 99;
    engine
        .policy_store_mut()
        .insert(direct_policy)
        .expect("direct policy should insert");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");
    let direct = signed_intent_for_policy(
        &signer,
        99,
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(direct, 1_030)
        .expect("direct open should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("direct open block should produce");
    engine
        .produce_empty_block(1_100)
        .expect("baseline block should produce");

    engine.update_market_mark_price(7, 50, 1_120);
    engine
        .produce_empty_block(1_130)
        .expect("direct loss block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );
}

#[test]
fn post_mode_daily_loss_counts_delegation_trading_account_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxDailyLossUsdc {
        limit_usdc: MandateU256::from_u128(10),
    }];
    // Keep the mandate live across the UTC-day boundary used by this test.
    input.expires_at = UTC_DAY_MS * 3;
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("daily-loss mandate should create");
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_020,
        )
        .expect("delegation should open");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    engine.positions.insert(
        (7, trading_account),
        PositionLedger {
            open_net_lots: 1,
            open_notional: 100,
            mandate_id: Some(mandate.id),
            opened_at_timestamp_ms: Some(1_030),
            ..PositionLedger::default()
        },
    );
    engine
        .produce_empty_block(1_100)
        .expect("baseline block should produce");

    engine.update_market_mark_price(7, 89, 1_120);
    engine
        .produce_empty_block(1_130)
        .expect("delegation daily-loss breach block should produce");

    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_daily_loss_resets_baseline_across_utc_day() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7]),
        },
        MandateConstraint::MaxDailyLossUsdc {
            limit_usdc: MandateU256::from_u128(10),
        },
    ];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("daily-loss mandate should create");

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 44,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_020,
        )
        .expect("maker order should rest");
    let open = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(open, 1_030)
        .expect("mandate open should admit");
    engine
        .produce_block_from_mempool(1_040, 10)
        .expect("open block should produce");
    engine
        .produce_empty_block(1_100)
        .expect("day-zero baseline block should produce");

    engine.update_market_mark_price(7, 95, UTC_DAY_MS + 10);
    engine
        .produce_empty_block(UTC_DAY_MS + 20)
        .expect("post-midnight baseline block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Active
    );

    engine.update_market_mark_price(7, 84, UTC_DAY_MS + 30);
    engine
        .produce_empty_block(UTC_DAY_MS + 40)
        .expect("post-midnight loss block should produce");
    assert_eq!(
        engine
            .mandate(&mandate.id)
            .expect("mandate should exist")
            .state,
        MandateState::Breached
    );
}

#[test]
fn post_mode_daily_loss_tracks_sibling_mandates_independently() {
    let first_signer = signing_key(8);
    let second_signer = signing_key(9);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    list_scaled_test_perpetual_market(&mut engine, 8, 10_000, 1_000_000);
    for market_id in [7, 8] {
        engine.update_market_index_price(market_id, 100, 1_001);
        engine.update_market_mark_price(market_id, 100, 1_001);
        engine.set_isolated_collateral(market_id, 22, 1_000);
        engine.set_isolated_collateral(market_id, 44, 1_000);
    }

    let mut first_input = sample_mandate_input(22, 1);
    first_input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7, 8]),
        },
        MandateConstraint::MaxDailyLossUsdc {
            limit_usdc: MandateU256::from_u128(10),
        },
    ];
    first_input.session_keys = vec![first_signer.verifying_key().to_bytes()];
    let first = engine
        .create_mandate(first_input, 1_010)
        .expect("first daily-loss mandate should create");

    let mut second_input = sample_mandate_input(22, 2);
    second_input.constraints = vec![
        MandateConstraint::AllowedMarkets {
            set: BTreeSet::from([7, 8]),
        },
        MandateConstraint::MaxDailyLossUsdc {
            limit_usdc: MandateU256::from_u128(10),
        },
    ];
    second_input.session_keys = vec![second_signer.verifying_key().to_bytes()];
    let second = engine
        .create_mandate(second_input, 1_020)
        .expect("second daily-loss mandate should create");

    for (market_id, client_order_id) in [(7, 44), (8, 45)] {
        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id,
                    account_id: 44,
                    client_order_id,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots: 1,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_030,
            )
            .expect("maker order should rest");
    }

    let first_open = signed_intent_for_policy(
        &first_signer,
        first.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let second_open = signed_intent_for_policy(
        &second_signer,
        second.policies[0],
        22,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 8,
            client_order_id: 2,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(first_open, 1_040)
        .expect("first mandate open should admit");
    engine
        .submit_intent(second_open, 1_040)
        .expect("second mandate open should admit");
    engine
        .produce_block_from_mempool(1_050, 10)
        .expect("open block should produce");
    engine
        .produce_empty_block(1_100)
        .expect("baseline block should produce");

    engine.update_market_mark_price(7, 89, 1_120);
    engine
        .produce_empty_block(1_130)
        .expect("loss block should produce");

    assert_eq!(
        engine
            .mandate(&first.id)
            .expect("first mandate should exist")
            .state,
        MandateState::Breached
    );
    assert_eq!(
        engine
            .mandate(&second.id)
            .expect("second mandate should exist")
            .state,
        MandateState::Active
    );
}

#[test]
fn post_mode_daily_loss_prunes_old_baselines() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 10_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.set_isolated_collateral(7, 22, 1_000);

    let mut input = sample_mandate_input(22, 1);
    input.constraints = vec![MandateConstraint::MaxDailyLossUsdc {
        limit_usdc: MandateU256::from_u128(10),
    }];
    input.session_keys = vec![signer.verifying_key().to_bytes()];
    input.expires_at = UTC_DAY_MS * 3;
    let mandate = engine
        .create_mandate(input, 1_010)
        .expect("daily-loss mandate should create");
    engine
        .produce_empty_block(1_100)
        .expect("day-zero baseline block should produce");
    assert!(engine
        .mandate_daily_pnl_baselines
        .contains_key(&(mandate.id, 0)));

    engine
        .produce_empty_block(UTC_DAY_MS + 100)
        .expect("day-one baseline block should produce");
    assert!(engine
        .mandate_daily_pnl_baselines
        .contains_key(&(mandate.id, 0)));
    assert!(engine
        .mandate_daily_pnl_baselines
        .contains_key(&(mandate.id, UTC_DAY_MS)));

    engine
        .produce_empty_block(UTC_DAY_MS * 2 + 100)
        .expect("day-two baseline block should produce");
    assert!(!engine
        .mandate_daily_pnl_baselines
        .contains_key(&(mandate.id, 0)));
    assert!(engine
        .mandate_daily_pnl_baselines
        .contains_key(&(mandate.id, UTC_DAY_MS)));
    assert!(engine
        .mandate_daily_pnl_baselines
        .contains_key(&(mandate.id, UTC_DAY_MS * 2)));
}

#[test]
fn create_mandate_accepts_no_same_operator_counterparty_constraint() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut input = sample_mandate_input(22, 1);
    input
        .constraints
        .push(MandateConstraint::NoSameOperatorCounterparty);

    let mandate = engine
        .create_mandate(input, 1_000)
        .expect("fill-mode operator constraint should compile");
    assert!(mandate
        .constraints
        .iter()
        .any(|constraint| matches!(constraint, MandateConstraint::NoSameOperatorCounterparty)));
    assert_eq!(mandate.policies.len(), 1);
}

#[test]
fn single_node_engine_rejects_invalid_perpetual_market_config() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut config = btc_perp_config();
    config.maintenance_margin_bps = 1_500;

    let err = engine
        .list_perpetual_market(7, "btc".into(), 1, 77, config, 1_100)
        .expect_err("invalid market should reject");

    assert!(matches!(
        err,
        NodeError::InvalidMarketConfig {
            reason: "invalid_base_symbol",
            ..
        }
    ));

    let mut config = btc_perp_config();
    config.oracle_feeder_allowlist[1] = config.oracle_feeder_allowlist[0];
    let err = engine
        .list_perpetual_market(8, "ETH".into(), 1, 78, config, 1_100)
        .expect_err("duplicate oracle feeders should reject");
    assert!(matches!(
        err,
        NodeError::InvalidMarketConfig {
            reason: "invalid_oracle_feeder_config",
            ..
        }
    ));
}

fn sample_open_delegation(
    engine: &mut SingleNodeEngine,
    agent: u64,
    opened_at_ms: u64,
) -> Delegation {
    let mandate_version = engine
        .mandates_for_agent(agent)
        .map(|mandate| mandate.version)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let mandate = create_sample_mandate(
        engine,
        agent,
        mandate_version,
        opened_at_ms.saturating_sub(1),
    );
    seed_account_policy_usdc(engine, agent, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            agent,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            opened_at_ms,
        )
        .expect("open_delegation should succeed");
    list_delegation_for_tier0_capped_pilot(engine, &delegation);
    delegation
}

#[test]
fn single_node_engine_opens_performance_share_delegation() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);

    assert_eq!(delegation.agent, 22);
    assert_eq!(delegation.status, DelegationStatus::Open);
    assert_eq!(delegation.performance_share_units, 1_000);
    assert_eq!(delegation.total_shares, 0);
    assert_eq!(delegation.high_water_mark_per_share, 0);
    assert_eq!(delegation.last_crystallized_at_ms, 1_000);
    let mandate = engine
        .mandate(&delegation.mandate_id)
        .expect("mandate should exist");
    let generated_policy = engine
        .policy_store()
        .get(mandate.policies[0])
        .expect("generated policy should exist");
    assert_eq!(
        generated_policy.execution_scope,
        PolicyExecutionScope::DelegationTrading {
            delegation_id: delegation.delegation_id,
        }
    );
    assert_eq!(
        engine.real_capital_bond_escrow_micro_usdc(&delegation.delegation_id),
        AGENT_BOND_MIN
    );
    assert_eq!(
        engine.real_capital_total_bond_escrow_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(engine.real_capital_compensation_escrow_micro_usdc(), 0);
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("agent address should exist");
    let bond_escrow_address = super::real_capital_bond_escrow_address(&delegation.delegation_id);
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(agent_address)),
        None
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, bond_escrow_address),
        AGENT_BOND_MIN
    );
    assert_eq!(
        engine.delegation(&delegation.delegation_id),
        Some(&delegation)
    );
    assert_eq!(engine.delegations().count(), 1);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::DelegationOpened {
            agent: 22,
            performance_share_units: 1_000,
            ..
        }
    )));
}

#[test]
fn single_node_engine_rejects_second_live_delegation_for_mandate() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 999);
    seed_account_policy_usdc(&mut engine, 22, (AGENT_BOND_MIN as u64).saturating_mul(2));
    engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("first delegation should open");

    let err = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            2_000,
        )
        .expect_err("second live delegation for mandate should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "mandate_live_delegation_exists",
            ..
        }
    ));
}

#[test]
fn single_node_engine_rejects_invalid_open_delegation_config() {
    let mut engine = SingleNodeEngine::new(single_node_config());

    // performance share above the cap
    let err = engine
        .open_delegation(
            22,
            [9; 32],
            PERFORMANCE_SHARE_CAP_UNITS + 1,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("performance share over cap should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "performance_share_out_of_range",
            ..
        }
    ));

    // mandate id zero
    let err = engine
        .open_delegation(
            22,
            [0; 32],
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("zero mandate id should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "mandate_zero",
            ..
        }
    ));

    // bond below minimum
    let err = engine
        .open_delegation(
            22,
            [9; 32],
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN - 1,
            1_000,
        )
        .expect_err("under-min bond should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "agent_bond_below_min",
            ..
        }
    ));

    // notice period below floor
    let err = engine
        .open_delegation(
            22,
            [9; 32],
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS - 1,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("notice below floor should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "notice_period_out_of_range",
            ..
        }
    ));

    // crystallization cadence above ceiling
    let err = engine
        .open_delegation(
            22,
            [9; 32],
            1_000,
            CRYSTALLIZATION_CADENCE_MAX_MS + 1,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("cadence above ceiling should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "crystallization_cadence_out_of_range",
            ..
        }
    ));

    assert_eq!(engine.delegations().count(), 0);
}

#[test]
fn single_node_engine_rejects_underfunded_delegation_bond() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, (AGENT_BOND_MIN - 1) as u64);

    let err = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("underfunded bond should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "insufficient_agent_policy_usdc_bond",
            ..
        }
    ));
    assert_eq!(engine.delegations().count(), 0);
}

#[test]
fn single_node_engine_requires_active_agent_mandate_for_delegation() {
    let mut engine = SingleNodeEngine::new(single_node_config());

    let err = engine
        .open_delegation(
            22,
            [9; 32],
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("unknown mandate should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "unknown_mandate",
            ..
        }
    ));

    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    let err = engine
        .open_delegation(
            33,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("wrong agent should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "mandate_agent_mismatch",
            ..
        }
    ));

    engine
        .suspend_mandate(mandate.id, 950)
        .expect("mandate should suspend");
    let err = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("suspended mandate should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "mandate_not_active",
            ..
        }
    ));

    let mut expiring_input = sample_mandate_input(44, 1);
    expiring_input.expires_at = 1_000;
    let expired = engine
        .create_mandate(expiring_input, 900)
        .expect("expiring mandate should create");
    let err = engine
        .open_delegation(
            44,
            expired.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect_err("expired mandate should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "mandate_expired",
            ..
        }
    ));
}

#[test]
fn single_node_engine_open_delegation_id_is_deterministic() {
    let mut left = SingleNodeEngine::new(single_node_config());
    let mut right = SingleNodeEngine::new(single_node_config());
    let l = sample_open_delegation(&mut left, 22, 1_000);
    let r = sample_open_delegation(&mut right, 22, 1_000);
    assert_eq!(l.delegation_id, r.delegation_id);

    // Same agent + same mandate + different opened_at must produce a
    // different id.
    let later = sample_open_delegation(&mut left, 33, 2_000);
    assert_ne!(l.delegation_id, later.delegation_id);
}

#[test]
fn single_node_engine_rejects_excess_delegations_per_agent() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut mandate_ids = Vec::new();
    seed_account_policy_usdc(
        &mut engine,
        22,
        (AGENT_BOND_MIN * MAX_DELEGATIONS_PER_AGENT_LIVE as u128) as u64,
    );
    for offset in 0..MAX_DELEGATIONS_PER_AGENT_LIVE {
        let mandate =
            create_sample_mandate(&mut engine, 22, offset as u32 + 1, 900 + offset as u64);
        mandate_ids.push(mandate.id);
        engine
            .open_delegation(
                22,
                mandate.id,
                1_000,
                CRYSTALLIZATION_CADENCE_MIN_MS,
                NOTICE_PERIOD_MIN_MS,
                AGENT_BOND_MIN,
                1_000 + offset as u64,
            )
            .expect("each open should succeed under cap");
    }
    engine
        .mandates
        .get_mut(&mandate_ids[0])
        .expect("mandate should exist")
        .state = MandateState::Revoked;
    let over_cap_mandate = create_sample_mandate(
        &mut engine,
        22,
        MAX_DELEGATIONS_PER_AGENT_LIVE as u32 + 1,
        1_999,
    );
    let err = engine
        .open_delegation(
            22,
            over_cap_mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            2_000,
        )
        .expect_err("over-cap open should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "max_delegations_per_agent",
            ..
        }
    ));
}

#[test]
fn single_node_engine_persists_delegation_through_snapshot() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(
        restored.delegation(&delegation.delegation_id),
        Some(&delegation)
    );
    assert_eq!(restored.delegations().count(), 1);
    assert_eq!(
        restored.real_capital_bond_escrow_micro_usdc(&delegation.delegation_id),
        AGENT_BOND_MIN
    );
}

#[test]
fn single_node_engine_closes_delegation_to_deposits() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);

    let closed = engine
        .close_delegation_to_deposits(delegation.delegation_id, 1_100)
        .expect("delegation should close to deposits");

    assert_eq!(closed.status, DelegationStatus::ClosedToDeposits);
    assert_eq!(closed.last_updated_at_ms, 1_100);
    let err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_200)
        .expect_err("closed-to-deposits delegation should reject deposits");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_not_open",
            ..
        }
    ));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent: 22,
            old_status: DelegationLifecycleStatus::Open,
            new_status: DelegationLifecycleStatus::ClosedToDeposits,
            reason: DelegationStatusReason::ClosedToDeposits,
            bond_returned: 0,
            ..
        } if delegation_id == delegation.delegation_id
    )));
}

#[test]
fn single_node_engine_winding_delegation_pauses_crystallization() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("deposit should succeed");

    let winding = engine
        .begin_delegation_winding(delegation.delegation_id, 1_200)
        .expect("delegation should begin winding");
    let crystallizations = engine
        .crystallize_due_delegations(1_000 + CRYSTALLIZATION_CADENCE_MIN_MS)
        .expect("crystallization scan should succeed");
    let mandate = engine
        .mandate(&delegation.mandate_id)
        .expect("delegation mandate should remain present");
    let generated_policy = engine
        .policy_store()
        .get(mandate.policies[0])
        .expect("generated policy should remain present");

    assert_eq!(winding.status, DelegationStatus::Winding);
    assert!(crystallizations.is_empty());
    assert_eq!(generated_policy.state, PolicyState::Suspended);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent: 22,
            old_status: DelegationLifecycleStatus::Open,
            new_status: DelegationLifecycleStatus::Winding,
            reason: DelegationStatusReason::WindingBegun,
            bond_returned: 0,
            ..
        } if delegation_id == delegation.delegation_id
    )));
}

#[test]
fn single_node_engine_closes_empty_delegation_and_returns_agent_bond() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);

    let closed = engine
        .close_delegation(delegation.delegation_id, 1_100)
        .expect("empty delegation should close");
    let mandate = engine
        .mandate(&delegation.mandate_id)
        .expect("delegation mandate should remain present");
    let generated_policy = engine
        .policy_store()
        .get(mandate.policies[0])
        .expect("generated policy should remain present");

    assert_eq!(closed.status, DelegationStatus::Closed);
    assert_eq!(generated_policy.state, PolicyState::Revoked);
    assert_eq!(
        engine.real_capital_bond_escrow_micro_usdc(&delegation.delegation_id),
        0
    );
    assert_eq!(engine.real_capital_total_bond_escrow_micro_usdc(), 0);
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("agent address should exist");
    let bond_escrow_address = super::real_capital_bond_escrow_address(&delegation.delegation_id);
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(agent_address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>()),
        Some(AGENT_BOND_MIN)
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, bond_escrow_address),
        0
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent: 22,
            old_status: DelegationLifecycleStatus::Open,
            new_status: DelegationLifecycleStatus::Closed,
            reason: DelegationStatusReason::WindingComplete,
            bond_returned: AGENT_BOND_MIN,
            ..
        } if delegation_id == delegation.delegation_id
    )));
}

#[test]
fn single_node_engine_terminates_delegation_and_preserves_redemptions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should mint shares before termination");

    let action_id = [44_u8; 32];
    engine
        .submit_delegation_action(
            action_id,
            BlockDelegationAction::Terminate {
                delegation_id: delegation.delegation_id,
                reason: BlockDelegationFaultReason::RiskLimitBreach,
                slash_policy: None,
            },
            1_200,
        )
        .expect("terminate action should queue");
    engine
        .produce_block_from_mempool(1_300, 1)
        .expect("terminate action should commit");

    let processed = engine
        .processed_delegation_action(action_id)
        .expect("terminate action receipt should persist");
    assert!(matches!(
        processed.receipt,
        DelegationActionReceipt::Terminate {
            delegation_id,
            reason: BlockDelegationFaultReason::RiskLimitBreach,
            bond_returned: 0,
            bond_slashed: AGENT_BOND_MIN,
            principal_compensation: AGENT_BOND_MIN,
            insurance_fund_delta: 0,
        } if delegation_id == delegation.delegation_id
    ));
    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .map(|delegation| delegation.status),
        Some(DelegationStatus::Terminated)
    );
    let mandate = engine
        .mandate(&delegation.mandate_id)
        .expect("delegation mandate should remain present");
    let generated_policy = engine
        .policy_store()
        .get(mandate.policies[0])
        .expect("generated policy should remain present");
    assert_eq!(generated_policy.state, PolicyState::Revoked);
    assert_eq!(
        engine.real_capital_bond_escrow_micro_usdc(&delegation.delegation_id),
        0
    );
    assert_eq!(engine.real_capital_total_bond_escrow_micro_usdc(), 0);
    assert_eq!(
        engine.real_capital_compensation_escrow_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_compensation_escrow_address(),),
        AGENT_BOND_MIN
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address(),),
        0
    );

    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);
    let listing = engine
        .real_capital_listing(&vault_id)
        .expect("listing should remain visible after termination");
    assert!(!listing.deposit_enabled);
    assert_eq!(listing.last_updated_governance_action, [0_u8; 32]);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalListingDepositEnabledUpdated {
            vault_id: event_vault_id,
            delegation_id,
            serving_agent,
            deposit_enabled,
            governance_action,
        } if event_vault_id == &vault_id
            && delegation_id == &delegation.delegation_id
            && *serving_agent == 22
            && !*deposit_enabled
            && *governance_action == [0_u8; 32]
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent,
            old_status,
            new_status,
            reason,
            bond_returned,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta,
        } if delegation_id == &delegation.delegation_id
            && *agent == 22
            && *old_status == DelegationLifecycleStatus::Open
            && *new_status == DelegationLifecycleStatus::Terminated
            && *reason == DelegationStatusReason::RiskLimitBreach
            && *bond_returned == 0
            && *bond_slashed == AGENT_BOND_MIN
            && *principal_compensation == AGENT_BOND_MIN
            && *insurance_fund_delta == 0
    )));

    let deposit_err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 1, 1_400)
        .expect_err("terminated delegation should reject new deposits");
    assert!(matches!(
        deposit_err,
        NodeError::InvalidDelegation {
            reason: "delegation_not_open",
            ..
        }
    ));
    let redemption = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_500)
        .expect("terminated delegation should preserve principal redemption rights");
    assert_eq!(redemption.request.shares_requested, 60_000_000);
    assert_eq!(
        engine.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 11),
        AGENT_BOND_MIN
    );
    let finalized = engine
        .finalize_delegation_redemption(
            redemption.request.request_id,
            redemption.request.ready_at_ms,
        )
        .expect("redemption finalization should release proportional compensation");
    assert_eq!(finalized.usdc_out, 60_000_000);
    assert_eq!(finalized.compensation_out, 2_000_000_000);
    assert_eq!(finalized.principal_balance, 2_160_000_000);
    assert_eq!(
        engine.real_capital_compensation_escrow_micro_usdc(),
        3_000_000_000
    );
    assert_eq!(
        engine.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 11),
        3_000_000_000
    );
    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve remaining compensation claim");
    assert_eq!(
        restored.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 11),
        3_000_000_000
    );
    assert_eq!(
        restored.real_capital_compensation_escrow_micro_usdc(),
        3_000_000_000
    );
}

#[test]
fn single_node_engine_rejects_reopening_closed_delegation_mandate() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let mandate_id = delegation.mandate_id;
    engine
        .close_delegation(delegation.delegation_id, 1_100)
        .expect("empty delegation should close");

    let err = engine
        .open_delegation(
            22,
            mandate_id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_200,
        )
        .expect_err("closed delegation mandate roots should not be reusable");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "mandate_policy_not_active",
            ..
        }
    ));
}

#[test]
fn single_node_engine_compensation_release_conserves_real_capital_escrow_v1() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should mint shares before termination");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::RiskLimitBreach,
            None,
            1_200,
        )
        .expect("termination should move bond into Rust-side compensation escrow");

    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("agent address should exist");
    let principal_address = engine
        .get_committed_state(&account_evm_address_state_key(11))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("principal address should exist");
    let trading_address = delegation_trading_address(&delegation.delegation_id);
    let compensation_escrow_address = super::real_capital_compensation_escrow_address();
    let evm_total_before_finalize = policy_usdc_balance_for_test(&engine, agent_address)
        + policy_usdc_balance_for_test(&engine, principal_address)
        + policy_usdc_balance_for_test(&engine, trading_address)
        + policy_usdc_balance_for_test(&engine, compensation_escrow_address);

    let redemption = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_300)
        .expect("terminated delegation should preserve redemption rights");
    let finalized = engine
        .finalize_delegation_redemption(
            redemption.request.request_id,
            redemption.request.ready_at_ms,
        )
        .expect("redemption finalization should release compensation");
    let evm_total_after_finalize = policy_usdc_balance_for_test(&engine, agent_address)
        + policy_usdc_balance_for_test(&engine, principal_address)
        + policy_usdc_balance_for_test(&engine, trading_address)
        + policy_usdc_balance_for_test(&engine, compensation_escrow_address);

    assert_eq!(finalized.compensation_out, 2_000_000_000);
    assert_eq!(evm_total_after_finalize, evm_total_before_finalize);
    assert_eq!(
        engine.real_capital_compensation_escrow_micro_usdc(),
        AGENT_BOND_MIN - finalized.compensation_out
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, compensation_escrow_address),
        AGENT_BOND_MIN - finalized.compensation_out
    );
}

#[test]
fn single_node_engine_terminates_unlisted_delegation_without_listing_event() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open without a real-capital listing");
    let event_start = engine.runtime_events().len();

    let termination = engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("unlisted delegation should still terminate");

    assert_eq!(termination.delegation.status, DelegationStatus::Terminated);
    assert_eq!(termination.bond_returned, 0);
    assert_eq!(termination.bond_slashed, AGENT_BOND_MIN);
    assert_eq!(termination.principal_compensation, 0);
    assert_eq!(termination.insurance_fund_delta, AGENT_BOND_MIN);
    assert_eq!(
        engine.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(engine.real_capital_compensation_escrow_micro_usdc(), 0);
    assert_eq!(
        engine.real_capital_bond_escrow_micro_usdc(&delegation.delegation_id),
        0
    );
    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);
    assert!(engine.real_capital_listing(&vault_id).is_none());
    let new_events = &engine.runtime_events()[event_start..];
    assert!(!new_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
    )));
    assert!(new_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            old_status,
            new_status,
            reason,
            bond_returned,
            ..
        } if delegation_id == &delegation.delegation_id
            && *old_status == DelegationLifecycleStatus::Open
            && *new_status == DelegationLifecycleStatus::Terminated
            && *reason == DelegationStatusReason::GovernanceFault
            && *bond_returned == 0
    )));
}

#[test]
fn single_node_engine_spends_real_capital_insurance_wallet_escrow_v1() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("termination should credit insurance wallet");

    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("agent address should exist");
    let trading_address = delegation_trading_address(&delegation.delegation_id);
    let insurance_wallet_address = super::real_capital_insurance_wallet_address();
    let evm_state_root_before_spend = engine.head().header.state_root;
    let evm_total_before_spend = policy_usdc_balance_for_test(&engine, agent_address)
        + policy_usdc_balance_for_test(&engine, trading_address)
        + policy_usdc_balance_for_test(&engine, insurance_wallet_address);
    let event_start = engine.runtime_events().len();
    let spend = 1_000_000_u128;
    let balance = engine
        .spend_real_capital_insurance_wallet_escrow_v1(
            spend,
            RealCapitalInsuranceWalletSpendSource::KeeperReward,
            1_200,
        )
        .expect("spend should debit wallet ledger");

    assert_eq!(balance, AGENT_BOND_MIN - spend);
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), balance);
    assert_eq!(
        engine.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, insurance_wallet_address),
        balance
    );
    let evm_total_after_spend = policy_usdc_balance_for_test(&engine, agent_address)
        + policy_usdc_balance_for_test(&engine, trading_address)
        + policy_usdc_balance_for_test(&engine, insurance_wallet_address);
    assert_eq!(evm_total_after_spend + spend, evm_total_before_spend);
    assert_ne!(engine.head().header.state_root, evm_state_root_before_spend);
    assert!(engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent {
                amount,
                balance: event_balance,
                source: RealCapitalInsuranceWalletSpendSource::KeeperReward,
            } if amount == spend && event_balance == balance
        )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve spent wallet below gross accumulator");
    assert_eq!(restored.real_capital_insurance_wallet_micro_usdc(), balance);
    assert_eq!(
        restored.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN
    );
}

#[test]
fn single_node_engine_rejects_real_capital_insurance_wallet_spend_underflow() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("termination should credit insurance wallet");

    let err = engine
        .spend_real_capital_insurance_wallet_escrow_v1(
            AGENT_BOND_MIN + 1,
            RealCapitalInsuranceWalletSpendSource::KeeperReward,
            1_200,
        )
        .expect_err("spend above wallet should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "real_capital_insurance_wallet_underflow",
            ..
        }
    ));
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
    )));
}

#[test]
fn single_node_engine_pays_real_capital_keeper_reward_escrow_v1() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    seed_account_policy_usdc(&mut engine, 33, 500_000);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("termination should credit insurance wallet");

    let policy_balance = |engine: &SingleNodeEngine, account_id: u64| -> u128 {
        let address = engine
            .get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .expect("account address should exist");
        policy_usdc_balance_for_test(engine, address)
    };
    let insurance_wallet_address = super::real_capital_insurance_wallet_address();
    let evm_total_before_payout = policy_balance(&engine, 22)
        + policy_balance(&engine, 33)
        + policy_usdc_balance_for_test(&engine, insurance_wallet_address);
    let keeper_balance_before = policy_balance(&engine, 33);
    let state_root_before_payout = engine.head().header.state_root;
    let payout_amount = 1_000_000_u128;
    let keeper_address: [u8; 20] = engine
        .get_committed_state(&account_evm_address_state_key(33))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("keeper address should exist");
    let candidate = engine
        .validate_real_capital_keeper_reward_candidate(33, payout_amount)
        .expect("keeper reward candidate should validate");
    assert_eq!(candidate.keeper_account, 33);
    assert_eq!(candidate.amount, payout_amount);
    assert_eq!(
        candidate.wallet_balance_after,
        AGENT_BOND_MIN - payout_amount
    );
    assert_eq!(candidate.keeper_address, keeper_address);
    let event_start = engine.runtime_events().len();

    let payout = engine
        .pay_real_capital_keeper_reward_escrow_v1(33, payout_amount, 1_200)
        .expect("authorized keeper payout should settle");

    assert_eq!(payout.recipient_account, 33);
    assert_eq!(payout.amount, payout_amount);
    assert_eq!(payout.wallet_balance, AGENT_BOND_MIN - payout_amount);
    assert_eq!(
        payout.recipient_balance,
        keeper_balance_before + payout_amount
    );
    assert_eq!(
        payout.source,
        RealCapitalInsuranceWalletSpendSource::KeeperReward
    );
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN - payout_amount
    );
    assert_eq!(
        engine.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, insurance_wallet_address),
        AGENT_BOND_MIN - payout_amount
    );
    let evm_total_after_payout = policy_balance(&engine, 22)
        + policy_balance(&engine, 33)
        + policy_usdc_balance_for_test(&engine, insurance_wallet_address);
    assert_eq!(evm_total_after_payout, evm_total_before_payout);
    assert_ne!(engine.head().header.state_root, state_root_before_payout);
    assert!(engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent {
                amount,
                balance,
                source: RealCapitalInsuranceWalletSpendSource::KeeperReward,
            } if amount == payout_amount && balance == AGENT_BOND_MIN - payout_amount
        )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve post-payout wallet divergence");
    assert_eq!(
        restored.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN - payout_amount
    );
    assert_eq!(
        restored.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(
        policy_balance(&restored, 33),
        keeper_balance_before + payout_amount
    );
}

#[test]
fn single_node_engine_rejects_real_capital_keeper_reward_wallet_underflow() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    seed_account_policy_usdc(&mut engine, 33, 500_000);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("termination should credit insurance wallet");
    let keeper_address = engine
        .get_committed_state(&account_evm_address_state_key(33))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("keeper address should exist");
    let keeper_balance_before = engine
        .get_committed_state(&policy_usdc_balance_state_key(keeper_address))
        .map(|bytes| U256::from_be_slice(&bytes).to::<u128>())
        .unwrap_or(0);
    let state_root_before_payout = engine.head().header.state_root;
    let event_start = engine.runtime_events().len();

    let err = engine
        .pay_real_capital_keeper_reward_escrow_v1(33, AGENT_BOND_MIN + 1, 1_200)
        .expect_err("payout above wallet should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "real_capital_insurance_wallet_underflow",
            ..
        }
    ));
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(keeper_address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>())
            .unwrap_or(0),
        keeper_balance_before
    );
    assert_eq!(engine.head().header.state_root, state_root_before_payout);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn single_node_engine_rejects_real_capital_keeper_reward_missing_recipient_wallet() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("termination should credit insurance wallet");
    let state_root_before_payout = engine.head().header.state_root;
    let event_start = engine.runtime_events().len();

    let err = engine
        .pay_real_capital_keeper_reward_escrow_v1(33, 1_000_000, 1_200)
        .expect_err("missing recipient EVM wallet should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            agent: 33,
            reason: "real_capital_keeper_evm_address_missing",
        }
    ));
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(engine.head().header.state_root, state_root_before_payout);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn single_node_engine_rejects_real_capital_keeper_reward_frozen_keeper() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    seed_account_policy_usdc(&mut engine, 33, 500_000);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::GovernanceFault,
            Some(BlockDelegationSlashPolicy::PrincipalProRata),
            1_100,
        )
        .expect("termination should credit insurance wallet");
    let keeper_address = engine
        .get_committed_state(&account_evm_address_state_key(33))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("keeper address should exist");
    let keeper_balance_before = engine
        .get_committed_state(&policy_usdc_balance_state_key(keeper_address))
        .map(|bytes| U256::from_be_slice(&bytes).to::<u128>())
        .unwrap_or(0);
    engine.frozen_accounts.insert(33);
    let state_root_before_payout = engine.head().header.state_root;
    let event_start = engine.runtime_events().len();

    let err = engine
        .pay_real_capital_keeper_reward_escrow_v1(33, 1_000_000, 1_200)
        .expect_err("frozen keeper should reject");

    assert!(matches!(err, NodeError::AccountFrozen { account_id: 33 }));
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN
    );
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(keeper_address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>())
            .unwrap_or(0),
        keeper_balance_before
    );
    assert_eq!(engine.head().header.state_root, state_root_before_payout);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn single_node_engine_applies_late_delegation_insurance_payout_after_full_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    seed_account_policy_usdc(&mut engine, 12, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 200_000_000, 1_100)
        .expect("first principal should deposit");
    engine
        .deposit_to_delegation(12, delegation.delegation_id, 100_000_000, 1_101)
        .expect("second principal should deposit");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::RiskLimitBreach,
            None,
            1_200,
        )
        .expect("termination should snapshot payout weights");

    let second_redemption = engine
        .request_delegation_redemption(12, delegation.delegation_id, 100_000_000, 1_300)
        .expect("second principal redemption should queue");
    engine
        .finalize_delegation_redemption(
            second_redemption.request.request_id,
            second_redemption.request.ready_at_ms,
        )
        .expect("second principal should fully redeem before payout");
    assert!(!engine
        .delegation_shares
        .contains_key(&(delegation.delegation_id, 12)));
    assert_eq!(
        engine.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 12),
        0
    );

    let mut restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve termination payout weights");
    assert_eq!(
        restored
            .delegation_termination_payout_weights
            .get(&(delegation.delegation_id, 11)),
        Some(&200_000_000)
    );
    assert_eq!(
        restored
            .delegation_termination_payout_weights
            .get(&(delegation.delegation_id, 12)),
        Some(&100_000_000)
    );
    seed_real_capital_insurance_wallet(&mut restored, 1_000, 300);

    let first_principal_address = restored
        .get_committed_state(&account_evm_address_state_key(11))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("first principal address should exist");
    let second_principal_address = restored
        .get_committed_state(&account_evm_address_state_key(12))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("second principal address should exist");
    let wallet_address = super::real_capital_insurance_wallet_address();
    let first_balance_before = policy_usdc_balance_for_test(&restored, first_principal_address);
    let second_balance_before = policy_usdc_balance_for_test(&restored, second_principal_address);
    let wallet_balance_before = policy_usdc_balance_for_test(&restored, wallet_address);
    let evm_total_before = first_balance_before + second_balance_before + wallet_balance_before;
    let action_id = [0x61; 32];

    restored
        .submit_delegation_action(
            action_id,
            BlockDelegationAction::InsurancePayout {
                delegation_id: delegation.delegation_id,
                requested_amount_micro_usdc: 500,
            },
            1_400,
        )
        .expect("insurance payout action should queue");

    let block = restored
        .produce_block_from_mempool(1_400, 1)
        .expect("insurance payout block should produce")
        .clone();

    assert_eq!(
        restored
            .processed_delegation_action(action_id)
            .map(|processed| processed.receipt.clone()),
        Some(DelegationActionReceipt::InsurancePayout {
            delegation_id: delegation.delegation_id,
            requested_amount_micro_usdc: 500,
            distributed_amount_micro_usdc: 300,
            recipient_count: 2,
        })
    );
    assert_eq!(
        policy_usdc_balance_for_test(&restored, first_principal_address),
        first_balance_before + 200
    );
    assert_eq!(
        policy_usdc_balance_for_test(&restored, second_principal_address),
        second_balance_before + 100
    );
    assert_eq!(restored.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(restored.real_capital_insurance_fund_micro_usdc(), 1_000);
    assert_eq!(policy_usdc_balance_for_test(&restored, wallet_address), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&restored, first_principal_address)
            + policy_usdc_balance_for_test(&restored, second_principal_address)
            + policy_usdc_balance_for_test(&restored, wallet_address),
        evm_total_before
    );
    assert!(restored.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::RealCapitalInsuranceWalletSpent {
            amount,
            balance,
            source: RealCapitalInsuranceWalletSpendSource::UserPayout,
        } if event.block_height == Some(block.header.height)
            && amount == 300
            && balance == 0
    )));
    assert!(!restored
        .delegation_shares
        .contains_key(&(delegation.delegation_id, 12)));

    let second_restored = SingleNodeEngine::from_snapshot_bytes(&restored.snapshot_bytes())
        .expect("post-payout snapshot should restore");
    assert_eq!(
        policy_usdc_balance_for_test(&second_restored, first_principal_address),
        first_balance_before + 200
    );
    assert_eq!(
        policy_usdc_balance_for_test(&second_restored, second_principal_address),
        second_balance_before + 100
    );
    assert_eq!(
        second_restored.real_capital_insurance_wallet_micro_usdc(),
        0
    );
    assert_eq!(
        second_restored
            .processed_delegation_action(action_id)
            .map(|processed| processed.receipt.clone()),
        Some(DelegationActionReceipt::InsurancePayout {
            delegation_id: delegation.delegation_id,
            requested_amount_micro_usdc: 500,
            distributed_amount_micro_usdc: 300,
            recipient_count: 2,
        })
    );
}

#[test]
fn single_node_engine_delegation_insurance_payout_with_zero_wallet_skips_spend_event() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("principal should deposit before termination");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::RiskLimitBreach,
            None,
            1_200,
        )
        .expect("termination should snapshot payout weights");
    seed_real_capital_insurance_wallet(&mut engine, 500, 0);

    let wallet_balance_before =
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());
    let principal_address = engine
        .get_committed_state(&account_evm_address_state_key(11))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("principal address should exist");
    let principal_balance_before = policy_usdc_balance_for_test(&engine, principal_address);
    let action_id = [0x62; 32];

    engine
        .submit_delegation_action(
            action_id,
            BlockDelegationAction::InsurancePayout {
                delegation_id: delegation.delegation_id,
                requested_amount_micro_usdc: 500,
            },
            1_300,
        )
        .expect("zero-wallet payout action should queue");
    let block = engine
        .produce_block_from_mempool(1_300, 1)
        .expect("zero-wallet payout block should produce")
        .clone();

    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(engine.real_capital_insurance_fund_micro_usdc(), 500);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address()),
        wallet_balance_before
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, principal_address),
        principal_balance_before
    );
    assert_eq!(
        engine
            .processed_delegation_action(action_id)
            .map(|processed| processed.receipt.clone()),
        Some(DelegationActionReceipt::InsurancePayout {
            delegation_id: delegation.delegation_id,
            requested_amount_micro_usdc: 500,
            distributed_amount_micro_usdc: 0,
            recipient_count: 0,
        })
    );
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::RealCapitalInsuranceWalletSpent {
            source: RealCapitalInsuranceWalletSpendSource::UserPayout,
            ..
        } if event.block_height == Some(block.header.height)
    )));
}

#[test]
fn single_node_engine_forced_liquidates_delegation_and_pays_cost_from_insurance_wallet() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = seed_delegation_forced_liquidation_candidate(&mut engine);
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    seed_account_policy_usdc(&mut engine, 77, 0);
    seed_real_capital_insurance_wallet(&mut engine, 1_000_000, 1_000_000);
    let keeper_address = engine
        .get_committed_state(&account_evm_address_state_key(77))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("keeper address should exist");
    let market_custody_address = super::isolated_margin_market_custody_address(7);
    let keeper_balance_before = policy_usdc_balance_for_test(&engine, keeper_address);
    let market_custody_balance_before =
        policy_usdc_balance_for_test(&engine, market_custody_address);
    let wallet_balance_before =
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());
    let action_id = [0x51; 32];

    engine
        .submit_delegation_action(
            action_id,
            BlockDelegationAction::ForcedLiquidation {
                delegation_id: delegation.delegation_id,
                market_id: 7,
                keeper_account: 77,
                requested_lots: None,
            },
            1_004,
        )
        .expect("forced liquidation action should queue");

    let block = engine
        .produce_block_from_mempool(1_004, 1)
        .expect("forced liquidation block should produce")
        .clone();

    let processed = engine
        .processed_delegation_action(action_id)
        .expect("forced liquidation receipt should persist");
    assert!(matches!(
        processed.receipt,
        DelegationActionReceipt::ForcedLiquidation {
            delegation_id,
            market_id: 7,
            keeper_account: 77,
            requested_lots: 3,
            filled_lots: 1,
            remaining_lots: 2,
            keeper_reward_micro_usdc: 2,
        } if delegation_id == delegation.delegation_id
    ));
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 999_998);
    assert_eq!(engine.real_capital_insurance_fund_micro_usdc(), 1_000_000);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, keeper_address),
        keeper_balance_before + 2
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address()),
        wallet_balance_before - 2
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, market_custody_address),
        market_custody_balance_before
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, keeper_address)
            + policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address(),),
        keeper_balance_before + wallet_balance_before
    );
    assert!(engine.margin_snapshot(7, trading_account).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_account_id,
            requested_lots,
            filled_lots,
            liquidator_reward,
            ..
        } if event.block_height == Some(block.header.height)
            && *market_id == 7
            && *account_id == delegation_trading_account_id(&delegation.delegation_id)
            && *liquidator_account_id == 77
            && *requested_lots == 3
            && *filled_lots == 1
            && *liquidator_reward == 2
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::RealCapitalInsuranceWalletSpent {
            amount,
            balance,
            source: RealCapitalInsuranceWalletSpendSource::ForcedLiquidationCost,
        } if event.block_height == Some(block.header.height)
            && amount == 2
            && balance == 999_998
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve forced liquidation payout");
    assert_eq!(restored.real_capital_insurance_wallet_micro_usdc(), 999_998);
    assert_eq!(
        policy_usdc_balance_for_test(&restored, keeper_address),
        keeper_balance_before + 2
    );
    assert_eq!(
        restored
            .processed_delegation_action(action_id)
            .map(|processed| processed.receipt.clone()),
        Some(DelegationActionReceipt::ForcedLiquidation {
            delegation_id: delegation.delegation_id,
            market_id: 7,
            keeper_account: 77,
            requested_lots: 3,
            filled_lots: 1,
            remaining_lots: 2,
            keeper_reward_micro_usdc: 2,
        })
    );
}

#[test]
fn single_node_engine_forced_liquidation_with_zero_keeper_reward_skips_wallet_spend() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = seed_delegation_forced_liquidation_candidate(&mut engine);
    seed_account_policy_usdc(&mut engine, 77, 0);
    seed_real_capital_insurance_wallet(&mut engine, 1_000_000, 1_000_000);
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    let wallet_balance_before =
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());
    let action_id = [0x52; 32];

    engine
        .submit_delegation_action(
            action_id,
            BlockDelegationAction::ForcedLiquidation {
                delegation_id: delegation.delegation_id,
                market_id: 7,
                keeper_account: 77,
                requested_lots: None,
            },
            1_004,
        )
        .expect("zero-reward forced liquidation action should queue");

    let block = engine
        .produce_block_from_mempool(1_004, 1)
        .expect("zero-reward forced liquidation block should produce")
        .clone();

    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 1_000_000);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address()),
        wallet_balance_before
    );
    assert_eq!(
        engine
            .processed_delegation_action(action_id)
            .map(|processed| processed.receipt.clone()),
        Some(DelegationActionReceipt::ForcedLiquidation {
            delegation_id: delegation.delegation_id,
            market_id: 7,
            keeper_account: 77,
            requested_lots: 1,
            filled_lots: 1,
            remaining_lots: 0,
            keeper_reward_micro_usdc: 0,
        })
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_account_id,
            requested_lots,
            filled_lots,
            liquidator_reward,
            ..
        } if event.block_height == Some(block.header.height)
            && *market_id == 7
            && *account_id == delegation_trading_account_id(&delegation.delegation_id)
            && *liquidator_account_id == 77
            && *requested_lots == 1
            && *filled_lots == 1
            && *liquidator_reward == 0
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::RealCapitalInsuranceWalletSpent {
            source: RealCapitalInsuranceWalletSpendSource::ForcedLiquidationCost,
            ..
        } if event.block_height == Some(block.header.height)
    )));
}

#[test]
fn forced_liquidation_action_rejects_positive_reward_keeper_without_evm_address() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = seed_delegation_forced_liquidation_candidate(&mut engine);
    seed_real_capital_insurance_wallet(&mut engine, 1_000_000, 1_000_000);
    let action_id = [0x52; 32];

    let err = engine
        .submit_delegation_action(
            action_id,
            BlockDelegationAction::ForcedLiquidation {
                delegation_id: delegation.delegation_id,
                market_id: 7,
                keeper_account: 77,
                requested_lots: None,
            },
            1_004,
        )
        .expect_err("missing keeper EVM address should reject preflight");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "real_capital_keeper_evm_address_missing",
            ..
        }
    ));
    assert_eq!(engine.pending_delegation_action_count(), 0);
}

#[test]
fn single_node_engine_winding_grace_termination_splits_bond_with_insurance() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should mint shares before termination");

    let termination = engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::WindingGraceFailure,
            None,
            1_200,
        )
        .expect("winding-grace termination should slash bond");

    assert_eq!(termination.bond_returned, 0);
    assert_eq!(termination.bond_slashed, AGENT_BOND_MIN);
    assert_eq!(termination.principal_compensation, AGENT_BOND_MIN / 2);
    assert_eq!(termination.insurance_fund_delta, AGENT_BOND_MIN / 2);
    assert_eq!(
        engine.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 11),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_compensation_escrow_micro_usdc(),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_bond_escrow_micro_usdc(&delegation.delegation_id),
        0
    );
}

#[test]
fn single_node_engine_winding_grace_sweep_terminates_overdue_residual_delegation() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should leave residual capital during winding");
    engine
        .begin_delegation_winding(delegation.delegation_id, 1_200)
        .expect("delegation should begin winding");

    engine
        .produce_empty_block(1_200 + WINDING_GRACE_WINDOW_MS - 1)
        .expect("pre-deadline block should commit");
    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .map(|delegation| delegation.status),
        Some(DelegationStatus::Winding)
    );

    let terminated_block_height = engine
        .produce_empty_block(1_200 + WINDING_GRACE_WINDOW_MS)
        .expect("deadline block should commit")
        .header
        .height;

    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .map(|delegation| delegation.status),
        Some(DelegationStatus::Terminated)
    );
    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .and_then(|delegation| delegation.winding_started_at_ms),
        None
    );
    assert_eq!(
        engine.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 11),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_insurance_fund_micro_usdc(),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_insurance_wallet_micro_usdc(),
        AGENT_BOND_MIN / 2
    );
    assert_eq!(
        engine.real_capital_compensation_escrow_micro_usdc(),
        AGENT_BOND_MIN / 2
    );

    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);
    let listing = engine
        .real_capital_listing(&vault_id)
        .expect("listing should remain visible after automatic termination");
    assert!(!listing.deposit_enabled);
    assert_eq!(listing.last_updated_block, terminated_block_height);
    assert_eq!(listing.last_updated_governance_action, [0_u8; 32]);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalListingDepositEnabledUpdated {
            vault_id: event_vault_id,
            delegation_id,
            serving_agent,
            deposit_enabled,
            governance_action,
        } if event.block_height == Some(terminated_block_height)
            && event_vault_id == &vault_id
            && delegation_id == &delegation.delegation_id
            && *serving_agent == 22
            && !*deposit_enabled
            && *governance_action == [0_u8; 32]
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent,
            old_status,
            new_status,
            reason,
            bond_returned,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta,
        } if event.block_height == Some(terminated_block_height)
            && delegation_id == &delegation.delegation_id
            && *agent == 22
            && *old_status == DelegationLifecycleStatus::Winding
            && *new_status == DelegationLifecycleStatus::Terminated
            && *reason == DelegationStatusReason::WindingGraceFailure
            && *bond_returned == 0
            && *bond_slashed == AGENT_BOND_MIN
            && *principal_compensation == AGENT_BOND_MIN / 2
            && *insurance_fund_delta == AGENT_BOND_MIN / 2
    )));
}

#[test]
fn single_node_engine_winding_grace_deadline_uses_winding_start() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should leave residual capital during winding");
    engine
        .begin_delegation_winding(delegation.delegation_id, 1_200)
        .expect("delegation should begin winding");
    let request = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_300)
        .expect("winding delegation should permit redemptions")
        .request;
    engine
        .finalize_delegation_redemption(request.request_id, request.ready_at_ms)
        .expect("redemption should finalize before grace deadline");

    let updated = engine
        .delegation(&delegation.delegation_id)
        .expect("delegation should still exist");
    assert_eq!(updated.status, DelegationStatus::Winding);
    assert_eq!(updated.winding_started_at_ms, Some(1_200));
    assert_eq!(updated.last_updated_at_ms, request.ready_at_ms);
    assert!(updated.last_updated_at_ms > 1_200);

    engine
        .produce_empty_block(1_200 + WINDING_GRACE_WINDOW_MS)
        .expect("deadline block should commit");

    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .map(|delegation| delegation.status),
        Some(DelegationStatus::Terminated)
    );
    assert_eq!(
        engine.delegation_compensation_claim_micro_usdc(&delegation.delegation_id, 11),
        AGENT_BOND_MIN / 2
    );
}

#[test]
fn single_node_engine_winding_grace_sweep_ignores_flat_empty_delegation() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    engine
        .begin_delegation_winding(delegation.delegation_id, 1_200)
        .expect("empty delegation should begin winding");
    let event_start = engine.runtime_events().len();

    engine
        .produce_empty_block(1_200 + WINDING_GRACE_WINDOW_MS)
        .expect("deadline block should commit");

    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .map(|delegation| delegation.status),
        Some(DelegationStatus::Winding)
    );
    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .and_then(|delegation| delegation.winding_started_at_ms),
        Some(1_200)
    );
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            &event.kind,
            RuntimeEventKind::DelegationStatusChanged {
                delegation_id,
                reason: DelegationStatusReason::WindingGraceFailure,
                ..
            } if delegation_id == &delegation.delegation_id
        )));

    let closed = engine
        .close_delegation(
            delegation.delegation_id,
            1_200 + WINDING_GRACE_WINDOW_MS + 1,
        )
        .expect("flat empty delegation should still close after the grace deadline");
    assert_eq!(closed.status, DelegationStatus::Closed);
}

#[test]
fn single_node_engine_rejects_invalid_terminate_slash_policy_shape() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);

    let missing_policy = engine
        .submit_delegation_action(
            [45_u8; 32],
            BlockDelegationAction::Terminate {
                delegation_id: delegation.delegation_id,
                reason: BlockDelegationFaultReason::GovernanceFault,
                slash_policy: None,
            },
            1_100,
        )
        .expect_err("governance fault should specify a slash policy");
    assert!(matches!(
        missing_policy,
        NodeError::InvalidDelegation {
            reason: "delegation_slash_policy_required",
            ..
        }
    ));

    let unexpected_policy = engine
        .submit_delegation_action(
            [46_u8; 32],
            BlockDelegationAction::Terminate {
                delegation_id: delegation.delegation_id,
                reason: BlockDelegationFaultReason::RiskLimitBreach,
                slash_policy: Some(BlockDelegationSlashPolicy::PrincipalProRata),
            },
            1_101,
        )
        .expect_err("non-governance fault should not carry an override policy");
    assert!(matches!(
        unexpected_policy,
        NodeError::InvalidDelegation {
            reason: "delegation_slash_policy_unexpected",
            ..
        }
    ));
}

#[test]
fn single_node_engine_rejects_closing_delegation_with_shares() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("deposit should succeed");

    let err = engine
        .close_delegation(delegation.delegation_id, 1_200)
        .expect_err("delegation with shares should reject close");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_has_outstanding_shares",
            ..
        }
    ));
}

#[test]
fn single_node_engine_rejects_closing_delegation_with_active_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let request = RedemptionRequest {
        request_id: [7; 32],
        delegation_id: delegation.delegation_id,
        principal: 11,
        shares_requested: 1,
        requested_at_ms: 1_100,
        ready_at_ms: 1_200,
        nav_at_request: SHARE_SCALE_FACTOR,
        status: RedemptionRequestStatus::Pending,
    };
    engine
        .redemption_requests
        .insert(request.request_id, request);

    let err = engine
        .close_delegation(delegation.delegation_id, 1_200)
        .expect_err("delegation with active redemption should reject close");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_has_active_redemptions",
            ..
        }
    ));
}

#[test]
fn single_node_engine_rejects_closing_delegation_with_open_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    let mut ledger = PositionLedger::default();
    ledger.apply_fill(OrderFillSide::Buy, 100, 1, None, 0, 1, 1_100);
    engine.positions.insert((7, trading_account), ledger);

    let err = engine
        .close_delegation(delegation.delegation_id, 1_200)
        .expect_err("delegation with open position should reject close");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_has_open_positions",
            ..
        }
    ));
}

#[test]
fn single_node_engine_rejects_closing_delegation_with_free_balance() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    engine
        .apply_committed_state_updates(
            [(
                policy_usdc_balance_state_key(delegation_trading_address(
                    &delegation.delegation_id,
                )),
                U256::from(1_u64).to_be_bytes::<32>().to_vec(),
            )],
            std::iter::empty(),
        )
        .expect("delegation balance should seed");

    let err = engine
        .close_delegation(delegation.delegation_id, 1_200)
        .expect_err("delegation with free balance should reject close");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_has_free_balance",
            ..
        }
    ));
}

#[test]
fn single_node_engine_runs_full_delegation_lifecycle_to_close() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("deposit should succeed");
    engine
        .close_delegation_to_deposits(delegation.delegation_id, 1_200)
        .expect("delegation should close to deposits");
    engine
        .begin_delegation_winding(delegation.delegation_id, 1_300)
        .expect("delegation should begin winding");
    let request = engine
        .request_delegation_redemption(11, delegation.delegation_id, 100_000_000, 1_400)
        .expect("full redemption should request")
        .request;
    engine
        .finalize_delegation_redemption(request.request_id, request.ready_at_ms)
        .expect("full redemption should finalize");

    let closed = engine
        .close_delegation(delegation.delegation_id, request.ready_at_ms + 1)
        .expect("fully redeemed delegation should close");

    assert_eq!(closed.status, DelegationStatus::Closed);
    assert_eq!(closed.total_shares, 0);
    assert_eq!(
        engine.delegation_share(&delegation.delegation_id, 11),
        DelegationShare::default()
    );
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("agent address should exist");
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(agent_address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>()),
        Some(AGENT_BOND_MIN)
    );
}

#[test]
fn single_node_engine_deposits_to_delegation_and_mints_shares() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);

    let deposit = engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");

    assert_eq!(deposit.principal, 11);
    assert_eq!(deposit.shares_minted, 150_000_000);
    assert_eq!(deposit.share.shares, 150_000_000);
    assert_eq!(deposit.share.avg_entry_nav_per_share, SHARE_SCALE_FACTOR);
    assert_eq!(deposit.delegation.total_shares, 150_000_000);
    assert_eq!(
        deposit.delegation.high_water_mark_per_share,
        SHARE_SCALE_FACTOR
    );
    assert_eq!(deposit.principal_balance, 100_000_000);
    assert_eq!(deposit.delegation_balance, 150_000_000);
    assert_eq!(
        engine.delegation_share(&delegation.delegation_id, 11),
        deposit.share
    );

    let principal_address = engine
        .get_committed_state(&account_evm_address_state_key(11))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("principal address should exist");
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(principal_address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>()),
        Some(100_000_000)
    );
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(delegation_trading_address(
                &delegation.delegation_id
            )))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>()),
        Some(150_000_000)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationDeposited {
            delegation_id,
            principal: 11,
            amount: 150_000_000,
            shares_minted: 150_000_000,
            total_shares: 150_000_000,
        } if delegation_id == delegation.delegation_id
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(
        restored
            .delegation(&delegation.delegation_id)
            .expect("delegation should restore")
            .total_shares,
        150_000_000
    );
    assert_eq!(
        restored.delegation_share(&delegation.delegation_id, 11),
        deposit.share
    );
}

#[test]
fn delegation_deposit_prices_against_deployed_trading_equity() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);
    seed_account_policy_usdc(&mut engine, 12, 100_000_000);

    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("initial deposit should succeed");

    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    engine.set_isolated_collateral(7, trading_account, 200_000_000);
    engine
        .apply_committed_state_updates(
            Vec::<(Vec<u8>, Vec<u8>)>::new(),
            vec![policy_usdc_balance_state_key(delegation_trading_address(
                &delegation.delegation_id,
            ))],
        )
        .expect("test should simulate fully deployed delegation cash");

    let follow_on = engine
        .deposit_to_delegation(12, delegation.delegation_id, 100_000_000, 1_200)
        .expect("follow-on deposit should price against deployed equity");

    assert_eq!(follow_on.shares_minted, 50_000_000);
    assert_eq!(
        follow_on.share.avg_entry_nav_per_share,
        2 * SHARE_SCALE_FACTOR
    );
    assert_eq!(follow_on.delegation.total_shares, 150_000_000);
}

#[test]
fn mandate_policy_trades_from_live_delegation_trading_account() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut engine, &delegation);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("delegation deposit should succeed");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);

    let collateral_deposit = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        trading_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    engine
        .submit_intent(collateral_deposit, 1_200)
        .expect("delegation collateral deposit should admit");
    engine
        .produce_block_from_mempool(1_210, 10)
        .expect("collateral block should produce");
    assert_eq!(engine.isolated_collateral(7, trading_account), 100);

    let order = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        trading_account,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 77,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(order, 1_220)
        .expect("delegation order should admit");
    engine
        .produce_block_from_mempool(1_230, 10)
        .expect("order block should produce");

    assert!(engine
        .resting_order(7, trading_account, 77, 1_230)
        .is_some());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::CollateralDeposited {
            market_id: 7,
            account_id,
            amount_usdc: 100,
            collateral: 100,
        } if account_id == trading_account
    )));
}

#[test]
fn mandate_policy_rejects_closed_delegation_trading_account() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    engine
        .close_delegation(delegation.delegation_id, 1_100)
        .expect("empty delegation should close");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        trading_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let err = engine
        .submit_intent(intent, 1_200)
        .expect_err("closed delegation trading account should reject");

    assert!(matches!(
        err,
        NodeError::Policy(PolicyError::PolicyNotActive(PolicyState::Revoked))
    ));
}

#[test]
fn mandate_policy_trades_from_closed_to_deposits_delegation() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut engine, &delegation);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("delegation deposit should succeed");
    engine
        .close_delegation_to_deposits(delegation.delegation_id, 1_200)
        .expect("delegation should close to deposits");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        trading_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    engine
        .submit_intent(intent, 1_300)
        .expect("closed-to-deposits delegation trading intent should admit");
}

#[test]
fn mandate_policy_rejects_winding_delegation() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut engine, &delegation);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("delegation deposit should succeed");
    engine
        .begin_delegation_winding(delegation.delegation_id, 1_200)
        .expect("delegation should begin winding");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);

    let intent = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        trading_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    let err = engine
        .submit_intent(intent, 1_300)
        .expect_err("winding delegation trading intent should reject");

    assert!(matches!(
        err,
        NodeError::Policy(PolicyError::PolicyNotActive(PolicyState::Suspended))
    ));
}

#[test]
fn mandate_policy_rejects_foreign_delegation_mandate() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let first = create_sample_mandate(&mut engine, 22, 1, 900);
    let second = create_sample_mandate(&mut engine, 22, 2, 901);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            first.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);

    let intent = signed_intent_for_policy(
        &signer,
        second.policies[0],
        trading_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let err = engine
        .submit_intent(intent, 1_100)
        .expect_err("wrong mandate should reject delegation trading account");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            agent: 22,
            reason: "delegation_mandate_mismatch",
        }
    ));
}

#[test]
fn delegated_child_policy_trades_from_live_delegation_trading_account() {
    let signer = signing_key(8);
    let child_signer = signing_key(12);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut engine, &delegation);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("delegation deposit should succeed");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);

    let create_child = signed_intent_for_policy(
        &signer,
        mandate.policies[0],
        trading_account,
        1,
        10_000,
        IntentKind::CreateSubPolicy {
            child_policy_id: 43,
            child_grantee: child_signer.verifying_key().to_bytes(),
            child_expires_at: 50_000,
            child_terms: PolicyTermsWire {
                gas_budget_per_day: 40,
                gas_refill_per_day: 40,
                max_intents_per_second: 1,
                max_intents_per_day: 3,
                max_volume_per_day: 120,
                max_drawdown_pct: 20,
            },
        },
    );
    engine
        .submit_intent(create_child, 1_200)
        .expect("delegated child policy should admit");
    engine
        .produce_block_from_mempool(1_210, 10)
        .expect("delegated child policy creation should commit");

    let collateral_deposit = signed_intent_for_policy(
        &child_signer,
        43,
        trading_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    engine
        .submit_intent(collateral_deposit, 1_220)
        .expect("delegated child collateral deposit should admit");
    engine
        .produce_block_from_mempool(1_230, 10)
        .expect("delegated child collateral deposit should commit");

    assert_eq!(engine.isolated_collateral(7, trading_account), 100);
}

#[test]
fn direct_policy_rejects_foreign_account_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine
        .policy_store_mut()
        .insert(sample_policy_for_account(22))
        .expect("direct policy should insert");

    let intent = signed_intent_for_policy(
        &signer,
        42,
        33,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let err = engine
        .submit_intent(intent, 1_100)
        .expect_err("direct policy must not sign for foreign accounts");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            agent: 22,
            reason: "intent_policy_account_mismatch",
        }
    ));
}

#[test]
fn policy_authorization_rejects_reserved_delegation_account_policy() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let reserved_account = 1_u64 << 63;
    // Insert directly to exercise the admission-side guard even if
    // higher-level policy creation rejects reserved accounts.
    engine
        .policy_store_mut()
        .insert(sample_policy_for_account(reserved_account))
        .expect("direct policy should insert");

    let intent = signed_intent_for_policy(
        &signer,
        42,
        reserved_account,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let err = engine
        .submit_intent(intent, 1_100)
        .expect_err("reserved delegation account policy should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            agent,
            reason: "reserved_delegation_account_policy",
        } if agent == reserved_account
    ));
}

#[test]
fn create_mandate_rejects_reserved_delegation_account_agent() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let reserved_account = 1_u64 << 63;
    let err = engine
        .create_mandate(sample_mandate_input(reserved_account, 1), 900)
        .expect_err("reserved delegation account agent should reject");

    assert!(matches!(
        err,
        NodeError::InvalidMandate {
            agent,
            reason: "reserved_delegation_account_policy",
        } if agent == reserved_account
    ));
}

#[test]
fn policy_authorization_allows_self_account_intent() {
    let signer = signing_key(8);
    let mut policy_store = PolicyStore::default();
    policy_store
        .insert(sample_policy_for_account(22))
        .expect("direct policy should insert");
    let signed_intent = signed_intent_for_policy(
        &signer,
        42,
        22,
        1,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 1,
        },
    );

    assert_eq!(
        super::policy_account_scope_for_intent(&policy_store, &BTreeMap::new(), &signed_intent),
        Ok((22, kombat_chain_policy::PolicyExecutionScope::Account(22)))
    );
}

#[test]
fn single_node_engine_rejects_unfunded_delegation_deposit() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);

    let err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect_err("underfunded deposit should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "insufficient_principal_policy_usdc",
            ..
        }
    ));
    assert_eq!(
        engine.delegation_share(&delegation.delegation_id, 11),
        DelegationShare::default()
    );
}

#[test]
fn real_capital_deposit_requires_listing() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);

    let err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect_err("unlisted delegation should reject deposits");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_not_listed",
            ..
        }
    ));
    assert_eq!(
        engine.delegation_share(&delegation.delegation_id, 11),
        DelegationShare::default()
    );
}

#[test]
fn real_capital_governance_listing_commits_via_block_items() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);
    let action = BlockRealCapitalGovernanceAction::UpsertListing {
        vault_id,
        delegation_id: delegation.delegation_id,
        serving_agent: delegation.agent,
        mode: BlockVaultRealCapitalMode::CappedPilot,
        pilot_min_serving_agent_tier: None,
        tier0_pilot_exception: true,
        deposit_enabled: true,
        listed_for_discovery: true,
        per_principal_micro_usdc: 500_000_000,
        total_vault_tvl_micro_usdc: 500_000_000,
        pilot_principal_allowlist: Some(BTreeSet::from([11])),
    };
    let action_id = real_capital_governance_action_id_for_submission(&action, 1_050);

    assert_eq!(
        engine
            .submit_real_capital_governance_action(action_id, action.clone(), 1_050)
            .expect("listing governance action should queue"),
        RealCapitalGovernanceActionSubmissionStatus::Accepted
    );
    assert_eq!(engine.pending_real_capital_governance_action_count(), 1);
    assert_eq!(
        engine
            .submit_real_capital_governance_action(action_id, action.clone(), 1_000_000_000)
            .expect("duplicate action id should dedupe before timestamp freshness"),
        RealCapitalGovernanceActionSubmissionStatus::DuplicatePending
    );
    let stale = BlockRealCapitalGovernanceAction::UpdateGlobalKillSwitch { disabled: true };
    let stale_id = real_capital_governance_action_id_for_submission(&stale, 1_000_000_000);
    assert!(matches!(
        engine
            .submit_real_capital_governance_action(stale_id, stale, 1_000_000_000)
            .expect_err("fresh queue admissions require a bounded timestamp"),
        NodeError::InvalidRealCapitalGovernance {
            action_id,
            reason: "real_capital_governance_timestamp_stale",
        } if action_id == stale_id
    ));
    assert_eq!(engine.pending_real_capital_governance_action_count(), 1);
    assert!(engine.real_capital_listing(&vault_id).is_none());
    assert!(matches!(
        engine
            .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_060)
            .expect_err("queued listing should not affect admission"),
        NodeError::InvalidDelegation {
            reason: "vault_not_listed",
            ..
        }
    ));
    let mut engine = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("pending real-capital governance should survive snapshot");

    let block = engine
        .produce_block_from_mempool(1_070, 10)
        .expect("governance block should produce")
        .clone();
    assert!(block.body.items.iter().any(|item| matches!(
        item,
        BlockItem::RealCapitalGovernance(governance)
            if governance.action_id == action_id
    )));
    let listing = engine
        .real_capital_listing(&vault_id)
        .expect("listing should commit");
    assert_eq!(listing.last_updated_governance_action, action_id);
    assert_eq!(listing.last_updated_block, block.header.height);
    assert_eq!(
        engine
            .processed_real_capital_governance_actions
            .get(&action_id)
            .map(|processed| &processed.receipt),
        Some(&RealCapitalGovernanceActionReceipt::UpsertListing {
            vault_id,
            delegation_id: delegation.delegation_id,
            serving_agent: 22,
        })
    );
    assert!(
        engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::RealCapitalListingUpserted {
                vault_id: event_vault_id,
                delegation_id,
                serving_agent,
                old_serving_agent,
                mode,
                deposit_enabled,
                listed_for_discovery,
                per_principal_cap_micro_usdc,
                total_vault_tvl_cap_micro_usdc,
                governance_action,
            } if event.block_height == Some(block.header.height)
                && event_vault_id == &vault_id
                && delegation_id == &delegation.delegation_id
                && *serving_agent == 22
                && old_serving_agent.is_none()
                && *mode == RealCapitalListingMode::CappedPilot
                && *deposit_enabled
                && *listed_for_discovery
                && *per_principal_cap_micro_usdc == 500_000_000
                && *total_vault_tvl_cap_micro_usdc == 500_000_000
                && governance_action == &action_id
        )),
        "listing upsert event should be committed"
    );
    assert_eq!(
        engine
            .submit_real_capital_governance_action(action_id, action, 1_080)
            .expect("processed action should be idempotent"),
        RealCapitalGovernanceActionSubmissionStatus::AlreadyProcessed
    );

    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_090)
        .expect("committed listing should admit deposit");
}

#[test]
fn real_capital_governance_caps_and_kill_switch_commit_via_block_items() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 500_000_000);

    let cap_action = BlockRealCapitalGovernanceAction::UpdateProtocolCap {
        total_protocol_tvl_micro_usdc: Some(100_000_000),
    };
    let cap_action_id = real_capital_governance_action_id_for_submission(&cap_action, 1_050);
    engine
        .submit_real_capital_governance_action(cap_action_id, cap_action, 1_050)
        .expect("protocol cap governance action should queue");
    engine
        .produce_block_from_mempool(1_060, 10)
        .expect("protocol cap block should produce");
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalProtocolCapUpdated {
            old_total_protocol_tvl_micro_usdc,
            new_total_protocol_tvl_micro_usdc,
            governance_action,
        } if old_total_protocol_tvl_micro_usdc.is_none()
            && *new_total_protocol_tvl_micro_usdc == Some(100_000_000)
            && governance_action == &cap_action_id
            && event.block_height == Some(engine.head().header.height)
    )));
    assert!(matches!(
        engine
            .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_070)
            .expect_err("protocol cap should reject oversized deposit"),
        NodeError::InvalidDelegation {
            reason: "vault_protocol_cap_exceeded",
            ..
        }
    ));

    let clear_cap = BlockRealCapitalGovernanceAction::UpdateProtocolCap {
        total_protocol_tvl_micro_usdc: None,
    };
    let clear_cap_id = real_capital_governance_action_id_for_submission(&clear_cap, 1_080);
    engine
        .submit_real_capital_governance_action(clear_cap_id, clear_cap, 1_080)
        .expect("clear cap governance action should queue");
    let kill = BlockRealCapitalGovernanceAction::UpdateGlobalKillSwitch { disabled: true };
    let kill_id = real_capital_governance_action_id_for_submission(&kill, 1_081);
    engine
        .submit_real_capital_governance_action(kill_id, kill, 1_081)
        .expect("kill-switch governance action should queue");
    engine
        .produce_block_from_mempool(1_090, 10)
        .expect("control block should produce");
    assert_eq!(engine.protocol_delegation_cap, None);
    assert!(engine.listing_control_state.global_deposit_kill_switch);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalProtocolCapUpdated {
            old_total_protocol_tvl_micro_usdc,
            new_total_protocol_tvl_micro_usdc,
            governance_action,
        } if *old_total_protocol_tvl_micro_usdc == Some(100_000_000)
            && new_total_protocol_tvl_micro_usdc.is_none()
            && governance_action == &clear_cap_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated {
            old_disabled,
            new_disabled,
            governance_action,
        } if !*old_disabled && *new_disabled && governance_action == &kill_id
    )));
    assert!(matches!(
        engine
            .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
            .expect_err("global kill switch should reject deposits"),
        NodeError::InvalidDelegation {
            reason: "vault_real_capital_disabled",
            ..
        }
    ));
}

#[test]
fn capped_pilot_deposit_requires_configured_tier_unless_exception() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    list_delegation_for_tier1_capped_pilot(&mut engine, &delegation);
    seed_account_policy_usdc(&mut engine, 11, 500_000_000);

    let err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect_err("Tier 0 serving agent should reject standard CappedPilot");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_serving_agent_below_tier",
            ..
        }
    ));

    promote_agent_to_tier1(&mut engine, 22, 1_110);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_120)
        .expect("Tier 1 serving agent should satisfy CappedPilot");

    let exception_delegation = sample_open_delegation(&mut engine, 33, 2_000);
    seed_account_policy_usdc(&mut engine, 12, 250_000_000);
    engine
        .deposit_to_delegation(12, exception_delegation.delegation_id, 150_000_000, 2_100)
        .expect("explicit Tier 0 pilot exception should admit under caps");
}

#[test]
fn real_capital_tier_loss_sweep_terminates_tier_required_listing() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine
        .create_account(22, signing_key(22).verifying_key().to_bytes(), [0; 32], 990)
        .expect("serving agent account should create");
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 22,
    };
    seed_account_policy_usdc(&mut engine, 22, PROGRAM_BOND_MIN_MICRO_USDC as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_050)
        .expect("program should define");
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("serving agent address should exist");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_060,
            None,
        )
        .expect("delegation-backed MM should enroll before tier loss");
    let mut listing = sample_real_capital_listing(
        &delegation,
        VaultRealCapitalMode::CappedPilot,
        Some(AgentIdentityTier::Tier1OperatorAttested),
        false,
    );
    listing.last_updated_governance_action = [9_u8; 32];
    engine
        .test_upsert_real_capital_listing(listing)
        .expect("tier-required listing should seed");
    promote_agent_to_tier1(&mut engine, 22, 1_100);
    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);

    let expires_at = 1_100 + OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
    let block = engine
        .produce_empty_block(expires_at)
        .expect("attestation expiry should produce a block")
        .clone();

    let listing = engine
        .real_capital_listing(&vault_id)
        .expect("listing should remain visible after tier loss");
    assert!(!listing.deposit_enabled);
    assert_eq!(listing.last_updated_block, block.header.height);
    assert_eq!(listing.last_updated_governance_action, [0_u8; 32]);
    assert_eq!(
        engine
            .delegation(&delegation.delegation_id)
            .map(|delegation| delegation.status),
        Some(DelegationStatus::Terminated)
    );
    assert_eq!(
        engine
            .market_maker_enrollment(&(identity, 7, program.program_id))
            .expect("delegation-backed MM enrollment should persist")
            .status,
        MarketMakerEnrollmentStatus::Exited
    );
    assert_eq!(engine.live_market_maker_bonds_micro_usdc(), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, agent_address),
        PROGRAM_BOND_MIN_MICRO_USDC
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RealCapitalListingDepositEnabledUpdated {
            vault_id: event_vault_id,
            delegation_id,
            serving_agent,
            deposit_enabled,
            governance_action,
        } if event.block_height == Some(block.header.height)
            && event_vault_id == &vault_id
            && delegation_id == &delegation.delegation_id
            && *serving_agent == 22
            && !*deposit_enabled
            && *governance_action == [0_u8; 32]
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent,
            old_status,
            new_status,
            reason,
            bond_returned,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta,
        } if event.block_height == Some(block.header.height)
            && delegation_id == &delegation.delegation_id
            && *agent == 22
            && *old_status == DelegationLifecycleStatus::Open
            && *new_status == DelegationLifecycleStatus::Terminated
            && *reason == DelegationStatusReason::AgentAttestationRevoked
            && *bond_returned == 0
            && *bond_slashed == AGENT_BOND_MIN
            && *principal_compensation == 0
            && *insurance_fund_delta == AGENT_BOND_MIN
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerEnrollmentExited {
            market_maker_identity,
            market_id: 7,
            program_id: event_program_id,
            bond_refunded_micro_usdc,
        } if event.block_height == Some(block.header.height)
            && *market_maker_identity == identity
            && *event_program_id == program.program_id
            && *bond_refunded_micro_usdc == PROGRAM_BOND_MIN_MICRO_USDC
    )));
}

#[test]
fn real_capital_listing_enforces_allowlist_and_caps() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let mut listing =
        sample_real_capital_listing(&delegation, VaultRealCapitalMode::CappedPilot, None, true);
    listing.pilot_principal_allowlist = Some(BTreeSet::from([11]));
    listing.caps = ListingCaps {
        per_principal_micro_usdc: 200_000_000,
        total_vault_tvl_micro_usdc: 220_000_000,
    };
    engine
        .test_upsert_real_capital_listing(listing)
        .expect("listing should update");
    seed_account_policy_usdc(&mut engine, 11, 500_000_000);
    seed_account_policy_usdc(&mut engine, 12, 500_000_000);

    let err = engine
        .deposit_to_delegation(12, delegation.delegation_id, 100_000_000, 1_100)
        .expect_err("principal outside pilot allowlist should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_pilot_principal_not_allowlisted",
            ..
        }
    ));

    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_110)
        .expect("allowlisted principal should deposit under caps");
    let err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_120)
        .expect_err("second deposit should exceed principal cap");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_principal_cap_exceeded",
            ..
        }
    ));

    let mut listing =
        sample_real_capital_listing(&delegation, VaultRealCapitalMode::CappedPilot, None, true);
    listing.caps = ListingCaps {
        per_principal_micro_usdc: u128::MAX,
        total_vault_tvl_micro_usdc: 160_000_000,
    };
    engine
        .test_upsert_real_capital_listing(listing)
        .expect("listing should update");
    let err = engine
        .deposit_to_delegation(12, delegation.delegation_id, 100_000_000, 1_130)
        .expect_err("deposit should exceed vault cap");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_tvl_cap_exceeded",
            ..
        }
    ));
}

#[test]
fn real_capital_caps_use_deposited_counters_and_release_on_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let mut listing =
        sample_real_capital_listing(&delegation, VaultRealCapitalMode::CappedPilot, None, true);
    listing.caps = ListingCaps {
        per_principal_micro_usdc: 200_000_000,
        total_vault_tvl_micro_usdc: 200_000_000,
    };
    engine
        .test_upsert_real_capital_listing(listing)
        .expect("listing should update");
    seed_account_policy_usdc(&mut engine, 11, 500_000_000);

    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should fill deposited-capital counters");
    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);
    engine
        .apply_committed_state_updates(
            [(
                policy_usdc_balance_state_key(delegation_trading_address(
                    &delegation.delegation_id,
                )),
                U256::ZERO.to_be_bytes::<32>().to_vec(),
            )],
            std::iter::empty(),
        )
        .expect("test should be able to reduce free balance");

    let err = engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_110)
        .expect_err("cap should use deposited counter, not free balance");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_principal_cap_exceeded",
            ..
        }
    ));
    assert_eq!(
        engine
            .vault_principal_deposited_micro_usdc
            .get(&(vault_id, 11)),
        Some(&150_000_000)
    );

    engine
        .apply_committed_state_updates(
            [(
                policy_usdc_balance_state_key(delegation_trading_address(
                    &delegation.delegation_id,
                )),
                U256::from(150_000_000_u128).to_be_bytes::<32>().to_vec(),
            )],
            std::iter::empty(),
        )
        .expect("test should restore free balance for redemption");
    let redemption = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_200)
        .expect("redemption request should lock shares");
    engine
        .finalize_delegation_redemption(
            redemption.request.request_id,
            redemption.request.ready_at_ms,
        )
        .expect("redemption finalization should release cap counters");
    assert_eq!(
        engine
            .vault_principal_deposited_micro_usdc
            .get(&(vault_id, 11)),
        Some(&90_000_000)
    );
    assert_eq!(
        engine.vault_deposited_micro_usdc.get(&vault_id),
        Some(&90_000_000)
    );

    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_300)
        .expect("released cap headroom should admit a later deposit");
    assert_eq!(
        engine
            .vault_principal_deposited_micro_usdc
            .get(&(vault_id, 11)),
        Some(&190_000_000)
    );
}

#[test]
fn real_capital_listing_rejects_public_tier_field_and_protocol_cap_overflow() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    let err = engine
        .test_upsert_real_capital_listing(sample_real_capital_listing(
            &delegation,
            VaultRealCapitalMode::Public,
            Some(AgentIdentityTier::Tier1OperatorAttested),
            false,
        ))
        .expect_err("Public listing must not carry the CappedPilot tier field");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_listing_public_tier_field_set",
            ..
        }
    ));

    engine.test_set_protocol_delegation_cap(Some(ProtocolDelegationCap {
        total_protocol_tvl_micro_usdc: 180_000_000,
        last_updated_governance_action: [9; 32],
    }));
    seed_account_policy_usdc(&mut engine, 11, 500_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("first deposit should fit protocol cap");

    let second_delegation = sample_open_delegation(&mut engine, 33, 2_000);
    seed_account_policy_usdc(&mut engine, 12, 500_000_000);
    let err = engine
        .deposit_to_delegation(12, second_delegation.delegation_id, 100_000_000, 2_100)
        .expect_err("second deposit should exceed protocol cap");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_protocol_cap_exceeded",
            ..
        }
    ));
}

#[test]
fn snapshot_bytes_reject_invalid_real_capital_listing_state() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should seed cap counters");
    let vault_id = super::vault_id_for_delegation(&delegation.delegation_id);

    let mut broken_counter = engine.clone();
    broken_counter
        .vault_deposited_micro_usdc
        .insert(vault_id, 149_000_000);
    assert!(SingleNodeEngine::from_snapshot_bytes(&broken_counter.snapshot_bytes()).is_err());

    let mut broken_listing_cap = engine.clone();
    let mut listing = broken_listing_cap
        .real_capital_listings
        .get(&vault_id)
        .cloned()
        .expect("listing should exist");
    listing.caps.per_principal_micro_usdc = 0;
    broken_listing_cap
        .real_capital_listings
        .insert(vault_id, listing);
    assert!(SingleNodeEngine::from_snapshot_bytes(&broken_listing_cap.snapshot_bytes()).is_err());

    let mut broken_compensation = engine.clone();
    broken_compensation
        .delegation_compensation_claims
        .insert((delegation.delegation_id, 11), 1);
    assert!(SingleNodeEngine::from_snapshot_bytes(&broken_compensation.snapshot_bytes()).is_err());

    let mut broken_protocol_cap = engine.clone();
    broken_protocol_cap.protocol_delegation_cap = Some(ProtocolDelegationCap {
        total_protocol_tvl_micro_usdc: 0,
        last_updated_governance_action: [9; 32],
    });
    assert!(SingleNodeEngine::from_snapshot_bytes(&broken_protocol_cap.snapshot_bytes()).is_err());

    let mut broken_kill_switch = engine;
    broken_kill_switch
        .listing_control_state
        .global_deposit_kill_switch = true;
    assert!(SingleNodeEngine::from_snapshot_bytes(&broken_kill_switch.snapshot_bytes()).is_err());
}

#[test]
fn single_node_engine_requests_delegation_redemption_and_locks_shares() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");

    let redemption = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_200)
        .expect("redemption request should succeed");

    assert_eq!(redemption.request.delegation_id, delegation.delegation_id);
    assert_eq!(redemption.request.principal, 11);
    assert_eq!(redemption.request.shares_requested, 60_000_000);
    assert_eq!(redemption.request.requested_at_ms, 1_200);
    assert_eq!(
        redemption.request.ready_at_ms,
        1_200 + delegation.notice_period_ms
    );
    assert_eq!(redemption.request.nav_at_request, SHARE_SCALE_FACTOR);
    assert_eq!(redemption.request.status, RedemptionRequestStatus::Pending);
    assert_eq!(redemption.share.shares, 150_000_000);
    assert_eq!(redemption.share.locked_for_redemption, 60_000_000);
    assert_eq!(
        engine.redemption_request(&redemption.request.request_id),
        Some(&redemption.request)
    );
    assert_eq!(
        engine.delegation_share(&delegation.delegation_id, 11),
        redemption.share
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationRedemptionRequested {
            delegation_id,
            principal: 11,
            request_id,
            shares_requested: 60_000_000,
            ready_at_ms,
        } if delegation_id == delegation.delegation_id
            && request_id == redemption.request.request_id
            && ready_at_ms == redemption.request.ready_at_ms
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(
        restored.redemption_request(&redemption.request.request_id),
        Some(&redemption.request)
    );
    assert_eq!(
        restored.delegation_share(&delegation.delegation_id, 11),
        redemption.share
    );
}

#[test]
fn single_node_engine_rejects_excess_delegation_redemption_shares() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");
    engine
        .request_delegation_redemption(11, delegation.delegation_id, 100_000_000, 1_200)
        .expect("first request should lock shares");

    let err = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_001, 1_201)
        .expect_err("over-lock should reject");

    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "insufficient_delegation_shares",
            ..
        }
    ));
    assert_eq!(
        engine
            .delegation_share(&delegation.delegation_id, 11)
            .locked_for_redemption,
        100_000_000
    );
}

#[test]
fn single_node_engine_finalizes_ready_delegation_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");
    let request = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_200)
        .expect("redemption request should succeed")
        .request;

    let finalized = engine
        .finalize_delegation_redemption(request.request_id, request.ready_at_ms)
        .expect("ready redemption should finalize");

    assert_eq!(finalized.request.status, RedemptionRequestStatus::Finalized);
    assert_eq!(finalized.usdc_out, 60_000_000);
    assert_eq!(finalized.compensation_out, 0);
    assert_eq!(finalized.principal_balance, 160_000_000);
    assert_eq!(finalized.delegation_balance, 90_000_000);
    assert_eq!(finalized.delegation.total_shares, 90_000_000);
    assert_eq!(finalized.share.shares, 90_000_000);
    assert_eq!(finalized.share.locked_for_redemption, 0);
    assert_eq!(
        engine.redemption_request(&request.request_id),
        Some(&finalized.request)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationRedemptionFinalized {
            delegation_id,
            principal: 11,
            request_id,
            shares_burned: 60_000_000,
            usdc_out: 60_000_000,
            compensation_out: 0,
            total_shares: 90_000_000,
        } if delegation_id == delegation.delegation_id && request_id == request.request_id
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(
        restored.redemption_request(&request.request_id),
        Some(&finalized.request)
    );
    assert_eq!(
        restored.delegation_share(&delegation.delegation_id, 11),
        finalized.share
    );
}

#[test]
fn single_node_engine_retries_ready_delegation_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");
    let request = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_200)
        .expect("redemption request should succeed")
        .request;
    engine
        .redemption_requests
        .get_mut(&request.request_id)
        .expect("request should exist")
        .status = RedemptionRequestStatus::Ready;

    let finalized = engine
        .finalize_delegation_redemption(request.request_id, request.ready_at_ms + 1)
        .expect("ready redemption should retry and finalize");

    assert_eq!(finalized.request.status, RedemptionRequestStatus::Finalized);
    assert_eq!(finalized.usdc_out, 60_000_000);
    assert_eq!(finalized.delegation.total_shares, 90_000_000);
    assert_eq!(finalized.share.locked_for_redemption, 0);
}

#[test]
fn single_node_engine_cancels_pending_delegation_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");
    let request = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_200)
        .expect("redemption request should succeed")
        .request;

    let cancelled = engine
        .cancel_delegation_redemption(11, request.request_id, 1_250)
        .expect("pending redemption should cancel");

    assert_eq!(cancelled.request.status, RedemptionRequestStatus::Cancelled);
    assert_eq!(cancelled.share.shares, 150_000_000);
    assert_eq!(cancelled.share.locked_for_redemption, 0);
    assert_eq!(
        engine.redemption_request(&request.request_id),
        Some(&cancelled.request)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationRedemptionCancelled {
            delegation_id,
            principal: 11,
            request_id,
            shares_unlocked: 60_000_000,
        } if delegation_id == delegation.delegation_id && request_id == request.request_id
    )));
}

#[test]
fn single_node_engine_cancels_ready_delegation_redemption() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should succeed");
    let request = engine
        .request_delegation_redemption(11, delegation.delegation_id, 60_000_000, 1_200)
        .expect("redemption request should succeed")
        .request;
    engine
        .redemption_requests
        .get_mut(&request.request_id)
        .expect("request should exist")
        .status = RedemptionRequestStatus::Ready;

    let cancelled = engine
        .cancel_delegation_redemption(11, request.request_id, request.ready_at_ms + 1)
        .expect("ready redemption should cancel");

    assert_eq!(cancelled.request.status, RedemptionRequestStatus::Cancelled);
    assert_eq!(cancelled.share.shares, 150_000_000);
    assert_eq!(cancelled.share.locked_for_redemption, 0);
}

#[test]
fn single_node_engine_crystallizes_delegation_profit_above_hwm() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("deposit should succeed");
    engine
        .apply_committed_state_updates(
            [(
                policy_usdc_balance_state_key(delegation_trading_address(
                    &delegation.delegation_id,
                )),
                U256::from(200_000_000).to_be_bytes::<32>().to_vec(),
            )],
            std::iter::empty(),
        )
        .expect("profit balance should update");

    let crystallizations = engine
        .crystallize_due_delegations(1_000 + CRYSTALLIZATION_CADENCE_MIN_MS)
        .expect("crystallization should succeed");

    assert_eq!(crystallizations.len(), 1);
    let crystallized = &crystallizations[0];
    assert_eq!(crystallized.agent_fee, 10_000_000);
    assert_eq!(crystallized.nav_pre_fee, 2_000_000);
    assert_eq!(crystallized.nav_post_fee, 1_900_000);
    assert_eq!(crystallized.delegation.high_water_mark_per_share, 1_900_000);
    assert_eq!(
        crystallized.delegation.last_crystallized_at_ms,
        1_000 + CRYSTALLIZATION_CADENCE_MIN_MS
    );
    assert_eq!(crystallized.agent_balance, 10_000_000);
    assert_eq!(crystallized.delegation_balance, 190_000_000);
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(22))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("agent address should exist");
    assert_eq!(
        engine
            .get_committed_state(&policy_usdc_balance_state_key(agent_address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>()),
        Some(10_000_000)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationCrystallized {
            delegation_id,
            agent: 22,
            agent_fee: 10_000_000,
            nav_pre_fee: 2_000_000,
            nav_post_fee: 1_900_000,
            hwm_new: 1_900_000,
        } if delegation_id == delegation.delegation_id
    )));
}

#[test]
fn single_node_engine_crystallizes_delegation_no_profit_without_lowering_hwm() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 100_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 100_000_000, 1_100)
        .expect("deposit should succeed");

    let crystallizations = engine
        .crystallize_due_delegations(1_000 + CRYSTALLIZATION_CADENCE_MIN_MS)
        .expect("crystallization should succeed");

    assert_eq!(crystallizations.len(), 1);
    let crystallized = &crystallizations[0];
    assert_eq!(crystallized.agent_fee, 0);
    assert_eq!(crystallized.nav_pre_fee, SHARE_SCALE_FACTOR);
    assert_eq!(crystallized.nav_post_fee, SHARE_SCALE_FACTOR);
    assert_eq!(
        crystallized.delegation.high_water_mark_per_share,
        SHARE_SCALE_FACTOR
    );
    assert_eq!(
        crystallized.delegation.last_crystallized_at_ms,
        1_000 + CRYSTALLIZATION_CADENCE_MIN_MS
    );
}

#[test]
fn snapshot_bytes_reject_delegation_without_mandate() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    sample_open_delegation(&mut engine, 22, 1_000);
    let mut snapshot = engine.persisted_snapshot();
    snapshot.mandates.clear();
    snapshot.mandates_by_agent.clear();

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject delegation with missing mandate");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("delegation mandate mismatch"));
}

#[test]
fn single_node_engine_rejects_relisting_delisted_perpetual_market() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .submit_oracle_price_batch(
            7,
            vec![
                OracleFeedSample {
                    feeder_id: [1; 32],
                    price_ticks: 105,
                    sources_used: 2,
                    confidence: 80,
                },
                OracleFeedSample {
                    feeder_id: [2; 32],
                    price_ticks: 100,
                    sources_used: 3,
                    confidence: 90,
                },
                OracleFeedSample {
                    feeder_id: [3; 32],
                    price_ticks: 110,
                    sources_used: 2,
                    confidence: 60,
                },
            ],
            engine
                .oracle_aggregation_config(7)
                .expect("market config should exist"),
            1_150,
        )
        .expect("oracle batch should aggregate");
    assert!(!engine.oracle_feed_submissions.is_empty());
    engine
        .set_perpetual_market_status(7, MarketStatus::Delisted, 1_200)
        .expect("delist should succeed");
    assert!(engine.oracle_feed_submissions.is_empty());

    let error = engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_300)
        .expect_err("market ids should not be reused after delist");
    assert_eq!(error, NodeError::MarketAlreadyListed { market_id: 7 });
}

#[test]
fn single_node_engine_uses_perpetual_registry_for_new_order_admission() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 103, 1_001);
    engine.update_market_mark_price(7, 103, 1_001);
    engine.set_isolated_collateral(7, 22, 100);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        )
        .expect("active listed market should accept new orders");

    let unknown = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 9,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        )
        .expect_err("unknown market should reject once registry is live");
    assert_eq!(unknown, NodeError::UnknownMarket { market_id: 9 });

    engine
        .set_perpetual_market_status(7, MarketStatus::ReduceOnly, 1_004)
        .expect("market status should update");
    let paused = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_005,
        )
        .expect_err("inactive market should block new orders");
    assert_eq!(paused, NodeError::MarketTradingPaused { market_id: 7 });

    let cancelled = engine
        .cancel_order(7, 22, 1, 1_006)
        .expect("cancel should remain available while inactive");
    assert_eq!(cancelled.client_order_id, 1);
}

#[test]
fn single_node_engine_rejects_unknown_market_intent_when_registry_is_live() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);

    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 9,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let error = engine
        .submit_intent(intent, 1_100)
        .expect_err("unknown market intent should reject once registry is live");
    assert_eq!(error, NodeError::UnknownMarket { market_id: 9 });
}

#[test]
fn commit_block_intents_enforces_perpetual_registry() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 9,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let body = BlockBody {
        items: vec![BlockItem::Intent(intent)],
    };
    let mut policy_store = engine.policy_store.clone();
    let mut order_books = engine.order_books.clone();
    let mut resting_order_mandates = engine.resting_order_mandates.clone();
    let mut isolated_collateral = engine.isolated_collateral.clone();

    let error = super::commit_block_intents(
        &mut policy_store,
        &engine.mandates,
        &engine.delegations,
        &engine.account_operators,
        &mut order_books,
        &mut resting_order_mandates,
        &mut isolated_collateral,
        &engine.frozen_accounts,
        &engine.markets,
        &engine.paused_markets,
        &engine.market_marks,
        &engine.market_index_prices,
        &engine.market_oracle_config,
        &engine.positions,
        7,
        1,
        &body,
        1,
        1_100,
    )
    .expect_err("unknown market should reject at block commit");
    assert_eq!(error, NodeError::UnknownMarket { market_id: 9 });
}

#[test]
fn single_node_engine_executes_collateral_movement_intents() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 100);
    let address = account_evm_address_state_key(11);
    let linked_address: [u8; 20] = engine
        .get_committed_state(&address)
        .expect("account should be linked")
        .as_slice()
        .try_into()
        .expect("account link should be 20 bytes");
    let market_custody_address = super::isolated_margin_market_custody_address(7);

    let deposit = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    engine
        .submit_intent(deposit, 1_100)
        .expect("deposit intent should admit");
    let policy_custody_sum_before_deposit = policy_usdc_balance_for_test(&engine, linked_address)
        + policy_usdc_balance_for_test(&engine, market_custody_address);
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("deposit block should produce");
    assert_eq!(engine.isolated_collateral(7, 11), 100);
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(linked_address)),
        None
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, market_custody_address),
        100
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, linked_address)
            + policy_usdc_balance_for_test(&engine, market_custody_address),
        policy_custody_sum_before_deposit
    );
    assert_eq!(
        engine
            .market_custody_required_balance(7)
            .expect("market custody requirement should compute"),
        100
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::CollateralDeposited {
            market_id: 7,
            account_id: 11,
            amount_usdc: 100,
            collateral: 100,
        }
    )));

    let withdraw = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 40,
        },
    );
    engine
        .submit_intent(withdraw, 1_300)
        .expect("withdraw intent should admit");
    let policy_custody_sum_before_withdraw = policy_usdc_balance_for_test(&engine, linked_address)
        + policy_usdc_balance_for_test(&engine, market_custody_address);
    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("withdraw block should produce");
    assert_eq!(engine.isolated_collateral(7, 11), 60);
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(linked_address)),
        Some(U256::from(40_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, market_custody_address),
        60
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, linked_address)
            + policy_usdc_balance_for_test(&engine, market_custody_address),
        policy_custody_sum_before_withdraw
    );
    assert_eq!(
        engine
            .market_custody_required_balance(7)
            .expect("market custody requirement should compute"),
        60
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::CollateralWithdrawn {
            market_id: 7,
            account_id: 11,
            amount_usdc: 40,
            collateral: 60,
        }
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve custody-backed collateral");
    assert_eq!(restored.isolated_collateral(7, 11), 60);
    assert_eq!(policy_usdc_balance_for_test(&restored, linked_address), 40);
    assert_eq!(
        policy_usdc_balance_for_test(&restored, market_custody_address),
        60
    );
}

#[test]
fn single_node_engine_rejects_unbacked_collateral_withdrawal_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 1);
    engine.set_isolated_collateral(7, 11, 10);

    let withdraw = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let error = engine
        .submit_intent(withdraw, 1_100)
        .expect_err("collateral without custody backing should reject");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_market_custody_balance",
        }
    );
}

#[test]
fn single_node_engine_rejects_collateral_withdrawal_while_market_bad_debt_outstanding() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 100);

    let deposit = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    engine
        .submit_intent(deposit, 1_100)
        .expect("deposit intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("deposit block should produce");
    engine.market_bad_debt_balances.insert(7, 3);

    let state_root_before_reject = engine.head().header.state_root;
    let event_count_before_reject = engine.runtime_events().len();
    let mempool_len_before_reject = engine.intent_mempool().len();
    let withdraw = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let error = engine
        .submit_intent(withdraw, 1_300)
        .expect_err("market bad debt should block all withdrawals");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "market_bad_debt_outstanding",
        }
    );
    assert_eq!(engine.head().header.state_root, state_root_before_reject);
    assert_eq!(engine.runtime_events().len(), event_count_before_reject);
    assert_eq!(engine.intent_mempool().len(), mempool_len_before_reject);
    assert_eq!(engine.isolated_collateral(7, 11), 100);
    assert_eq!(engine.market_bad_debt_balance(7), 3);
}

#[test]
fn single_node_engine_allows_collateral_deposit_while_market_bad_debt_outstanding() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 40);
    engine.market_bad_debt_balances.insert(7, 9);
    let market_custody_address = super::isolated_margin_market_custody_address(7);

    let deposit = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 40,
        },
    );
    engine
        .submit_intent(deposit, 1_100)
        .expect("deposits should remain admissible during bad debt recovery");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("deposit block should produce");

    assert_eq!(engine.isolated_collateral(7, 11), 40);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, market_custody_address),
        40
    );
    assert_eq!(engine.market_bad_debt_balance(7), 9);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::CollateralDeposited {
            market_id: 7,
            account_id: 11,
            amount_usdc: 40,
            collateral: 40,
        }
    )));
}

#[test]
fn single_node_engine_rejects_invalid_collateral_withdrawal_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 100);

    let withdraw = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let error = engine
        .submit_intent(withdraw, 1_100)
        .expect_err("empty collateral should not withdraw");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_collateral",
        }
    );
}

#[test]
fn single_node_engine_counts_resting_orders_against_withdrawal_headroom() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 12);
    engine.update_market_index_price(7, 100, 1_050);
    engine.update_market_mark_price(7, 100, 1_050);

    let deposit = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 12,
        },
    );
    engine
        .submit_intent(deposit, 1_100)
        .expect("deposit intent should admit");
    engine
        .produce_block_from_mempool(1_150, 10)
        .expect("deposit block should produce");

    let resting_order = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 77,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(resting_order, 2_200)
        .expect("resting order should admit");
    engine
        .produce_block_from_mempool(2_250, 10)
        .expect("resting order block should produce");
    assert!(
        engine
            .resting_order(7, 11, 77, 3_300)
            .expect("order should rest")
            .remaining_lots
            == 1
    );

    let over_withdraw = signed_intent(
        &signer,
        3,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 3,
        },
    );
    let error = engine
        .submit_intent(over_withdraw, 3_300)
        .expect_err("resting order should reserve initial margin");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_initial_margin",
        }
    );

    engine
        .cancel_order(7, 11, 77, 3_350)
        .expect("cancel should release open-order headroom");
    let allowed_withdraw = signed_intent(
        &signer,
        4,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 3,
        },
    );
    engine
        .submit_intent(allowed_withdraw, 4_400)
        .expect("withdraw should admit once the order is cancelled");
}

#[test]
fn single_node_engine_rejects_unmargined_order_on_listed_market() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_050);
    engine.update_market_mark_price(7, 100, 1_050);

    let error = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_100,
        )
        .expect_err("unfunded listed-market order should reject");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_initial_margin",
        }
    );
}

#[test]
fn single_node_engine_previews_initial_margin_for_candidate_order() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_050);
    engine.update_market_mark_price(7, 100, 1_050);

    let order = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 11,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let preview = engine
        .preview_order_initial_margin(order, None, 1_100)
        .expect("margin preview should run");
    assert_eq!(preview.market_id, 7);
    assert_eq!(preview.account_id, 11);
    assert!(!preview.would_succeed);
    assert_eq!(preview.reason, Some("insufficient_initial_margin"));
    assert_eq!(preview.current_raw_notional, 0);
    assert_eq!(preview.candidate_raw_notional, 100);
    assert_eq!(preview.current_notional, 0);
    assert_eq!(preview.candidate_notional, 100);
    assert_eq!(preview.required_initial, 10);
    assert_eq!(preview.equity_for_market, 0);
    assert_eq!(preview.margin_surplus, -10);

    engine.set_isolated_collateral(7, 11, 10);
    let funded_preview = engine
        .preview_order_initial_margin(order, None, 1_101)
        .expect("funded margin preview should run");
    assert!(funded_preview.would_succeed);
    assert_eq!(funded_preview.reason, None);
    assert_eq!(funded_preview.margin_surplus, 0);
}

#[test]
fn single_node_engine_rejects_unmargined_place_order_intent_on_listed_market() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_050);
    engine.update_market_mark_price(7, 100, 1_050);

    let order = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let error = engine
        .submit_intent(order, 1_100)
        .expect_err("unfunded listed-market order intent should reject");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_initial_margin",
        }
    );
}

#[test]
fn single_node_engine_scales_initial_margin_notional_for_live_tick_lot_config() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut policy = sample_policy();
    policy.terms.max_volume_per_day = 1_000_000;
    engine
        .policy_store_mut()
        .insert(policy)
        .expect("policy should insert");
    list_scaled_test_perpetual_market(&mut engine, 7, 100, 1);
    engine.update_market_index_price(7, 7_800_000_000_000, 1_050);
    engine.update_market_mark_price(7, 7_800_000_000_000, 1_050);
    engine.set_isolated_collateral(7, 11, 90_000_000);

    let order = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 1,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit {
                price_ticks: 7_800_000_000_000,
            },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );

    engine
        .submit_intent(order, 1_100)
        .expect("scaled live-sized order should fit within deposited collateral");
}

#[test]
fn single_node_engine_scales_margin_snapshot_for_live_tick_lot_config() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 100, 1);
    engine.update_market_index_price(7, 7_800_000_000_000, 1_050);
    engine.update_market_mark_price(7, 7_800_000_000_000, 1_050);
    engine.set_isolated_collateral(7, 11, 90_000_000);
    engine.set_isolated_collateral(7, 22, 90_000_000);

    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 11,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Limit {
            price_ticks: 7_800_000_000_000,
        },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Market,
        time_in_force: kombat_chain_matching::TimeInForce::Ioc,
        post_only: false,
    };

    engine.submit_order(ask, 1_100).expect("ask should rest");
    engine.submit_order(bid, 1_101).expect("bid should fill");

    let snapshot = engine.margin_snapshot(7, 22);
    assert_eq!(snapshot.collateral, 90_000_000);
    assert_eq!(snapshot.mark_notional, 78_000);
    assert_eq!(snapshot.maintenance_requirement, 3_900);
    assert_eq!(snapshot.equity, 90_000_000);
    assert_eq!(snapshot.margin_surplus, 89_996_100);
    assert!(!snapshot.under_maintenance);

    engine.update_market_mark_price(7, 7_790_000_000_000, 1_102);
    let repriced = engine.margin_snapshot(7, 22);
    assert_eq!(repriced.unrealized_pnl, -100);
    assert_eq!(repriced.equity, 89_999_900);
    assert!(!repriced.under_maintenance);
}

#[test]
fn single_node_engine_allows_reducing_order_below_initial_margin() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_050);
    engine.update_market_mark_price(7, 100, 1_050);
    engine.set_isolated_collateral(7, 11, 100);
    engine.set_isolated_collateral(7, 22, 100);
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_100,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_200,
        )
        .expect("taker bid should open a long");
    assert_eq!(engine.position(7, 11).net_lots(), 1);
    engine.set_isolated_collateral(7, 11, 0);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 110 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_300,
        )
        .expect("opposite-side order that only reduces exposure should admit");

    let error = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_400,
        )
        .expect_err("same-side order should increase exposure and reject");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_initial_margin",
        }
    );
}

#[test]
fn single_node_engine_gates_replace_order_on_margin_delta() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 100, 1_050);
    engine.update_market_mark_price(7, 100, 1_050);
    engine.set_isolated_collateral(7, 11, 10);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_100,
        )
        .expect("initially margined order should rest");
    engine.set_isolated_collateral(7, 11, 0);

    engine
        .replace_order(
            7,
            11,
            1,
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_200,
        )
        .expect("narrowing replace should not require fresh margin");

    let error = engine
        .replace_order(
            7,
            11,
            2,
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 11,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 2,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_300,
        )
        .expect_err("widening replace should require initial margin");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_initial_margin",
        }
    );
}

#[test]
fn single_node_engine_rejects_unfunded_collateral_deposit_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 99);

    let deposit = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    let error = engine
        .submit_intent(deposit, 1_100)
        .expect_err("underfunded collateral deposit should reject");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "insufficient_policy_usdc_balance",
        }
    );
}

#[test]
fn single_node_engine_aggregates_oracle_feed_samples() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);

    let aggregation = engine
        .submit_oracle_price_batch(
            7,
            vec![
                OracleFeedSample {
                    feeder_id: [1; 32],
                    price_ticks: 105,
                    sources_used: 2,
                    confidence: 80,
                },
                OracleFeedSample {
                    feeder_id: [2; 32],
                    price_ticks: 100,
                    sources_used: 3,
                    confidence: 90,
                },
                OracleFeedSample {
                    feeder_id: [3; 32],
                    price_ticks: 110,
                    sources_used: 2,
                    confidence: 60,
                },
                OracleFeedSample {
                    feeder_id: [4; 32],
                    price_ticks: 10_000,
                    sources_used: 1,
                    confidence: 90,
                },
            ],
            OracleAggregationConfig {
                min_sources: 2,
                min_confidence: 50,
                min_samples: 3,
            },
            1_200,
        )
        .expect("oracle samples should aggregate");

    assert_eq!(
        aggregation,
        OracleFeedAggregation {
            market_id: 7,
            oracle_feed_id: 77,
            price_ticks: 105,
            submitted_samples: 4,
            accepted_samples: 3,
            rejected_samples: 1,
            expected_feeders: 4,
            missing_feeders: 0,
            timestamp_ms: 1_200,
        }
    );
    assert_eq!(engine.index_price_ticks(7), Some(105));
    assert_eq!(engine.mark_price_ticks(7), Some(105));
    assert_eq!(engine.oracle_feed_submissions.len(), 4);

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    assert_eq!(restored.index_price_ticks(7), Some(105));
    assert_eq!(restored.mark_price_ticks(7), Some(105));
    assert_eq!(restored.oracle_feed_submissions.len(), 4);
}

#[test]
fn single_node_engine_rejects_oracle_batch_without_quorum() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);

    let error = engine
        .submit_oracle_price_batch(
            7,
            vec![OracleFeedSample {
                feeder_id: [1; 32],
                price_ticks: 105,
                sources_used: 1,
                confidence: 80,
            }],
            OracleAggregationConfig {
                min_sources: 2,
                min_confidence: 50,
                min_samples: 1,
            },
            1_200,
        )
        .expect_err("filtered batch should not aggregate");
    assert_eq!(
        error,
        NodeError::OracleAggregationFailed {
            market_id: 7,
            reason: "insufficient_oracle_samples",
        }
    );
    assert_eq!(engine.index_price_ticks(7), None);
    assert_eq!(engine.mark_price_ticks(7), None);
}

#[test]
fn single_node_engine_rejects_invalid_oracle_batch_atomically() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);

    let error = engine
        .submit_oracle_price_batch(
            7,
            vec![
                OracleFeedSample {
                    feeder_id: [1; 32],
                    price_ticks: 105,
                    sources_used: 2,
                    confidence: 80,
                },
                OracleFeedSample {
                    feeder_id: [0; 32],
                    price_ticks: 110,
                    sources_used: 2,
                    confidence: 80,
                },
            ],
            OracleAggregationConfig::default(),
            1_200,
        )
        .expect_err("invalid batch should reject");
    assert_eq!(
        error,
        NodeError::InvalidOracleSubmission {
            market_id: 7,
            reason: "invalid_oracle_sample",
        }
    );
    assert!(engine.oracle_feed_submissions.is_empty());
    assert_eq!(engine.index_price_ticks(7), None);
    assert_eq!(engine.mark_price_ticks(7), None);
}

#[test]
fn single_node_engine_rejects_unauthorized_oracle_feeder_atomically() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);

    let error = engine
        .submit_oracle_price_batch(
            7,
            vec![OracleFeedSample {
                feeder_id: [9; 32],
                price_ticks: 105,
                sources_used: 2,
                confidence: 80,
            }],
            engine
                .oracle_aggregation_config(7)
                .expect("market config should exist"),
            1_200,
        )
        .expect_err("unknown feeder should reject");

    assert_eq!(
        error,
        NodeError::InvalidOracleSubmission {
            market_id: 7,
            reason: "unauthorized_oracle_feeder",
        }
    );
    assert!(engine.oracle_feed_submissions.is_empty());
    assert_eq!(engine.index_price_ticks(7), None);
    assert_eq!(engine.mark_price_ticks(7), None);
}

#[test]
fn single_node_engine_reports_missing_oracle_feeders() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);

    let aggregation = engine
        .submit_oracle_price_batch(
            7,
            vec![
                OracleFeedSample {
                    feeder_id: [1; 32],
                    price_ticks: 105,
                    sources_used: 2,
                    confidence: 80,
                },
                OracleFeedSample {
                    feeder_id: [2; 32],
                    price_ticks: 100,
                    sources_used: 3,
                    confidence: 90,
                },
                OracleFeedSample {
                    feeder_id: [3; 32],
                    price_ticks: 110,
                    sources_used: 2,
                    confidence: 60,
                },
            ],
            engine
                .oracle_aggregation_config(7)
                .expect("market config should exist"),
            1_200,
        )
        .expect("oracle samples should aggregate");

    assert_eq!(aggregation.expected_feeders, 4);
    assert_eq!(aggregation.missing_feeders, 1);
    assert_eq!(aggregation.price_ticks, 105);
}

#[test]
fn single_node_engine_prunes_oracle_feed_submission_timestamps() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);

    for offset in 0..(ORACLE_FEED_RETAINED_TIMESTAMPS + 6) {
        engine
            .submit_oracle_price_batch(
                7,
                vec![
                    OracleFeedSample {
                        feeder_id: [1; 32],
                        price_ticks: 100 + offset as u64,
                        sources_used: 2,
                        confidence: 80,
                    },
                    OracleFeedSample {
                        feeder_id: [2; 32],
                        price_ticks: 101 + offset as u64,
                        sources_used: 2,
                        confidence: 80,
                    },
                    OracleFeedSample {
                        feeder_id: [3; 32],
                        price_ticks: 102 + offset as u64,
                        sources_used: 2,
                        confidence: 80,
                    },
                ],
                engine
                    .oracle_aggregation_config(7)
                    .expect("market config should exist"),
                1_200 + offset as u64,
            )
            .expect("oracle batch should aggregate");
    }

    let retained_timestamps = engine
        .oracle_feed_submissions
        .keys()
        .map(|(_, timestamp_ms, _)| *timestamp_ms)
        .collect::<BTreeSet<_>>();
    assert_eq!(retained_timestamps.len(), ORACLE_FEED_RETAINED_TIMESTAMPS);
    assert!(!retained_timestamps.contains(&1_200));
    assert!(retained_timestamps.contains(&(1_200 + ORACLE_FEED_RETAINED_TIMESTAMPS as u64 + 5)));
}

fn four_node_validator_set() -> ValidatorSet {
    let pubkey_1 = encode_hex(&signing_key(1).verifying_key().to_bytes());
    let pubkey_2 = encode_hex(&signing_key(2).verifying_key().to_bytes());
    let pubkey_3 = encode_hex(&signing_key(3).verifying_key().to_bytes());
    let pubkey_4 = encode_hex(&signing_key(4).verifying_key().to_bytes());
    load_validator_set_from_str(&format!(
        r#"
epoch = 1

[[validators]]
validator_id = "0101010101010101010101010101010101010101010101010101010101010101"
consensus_pubkey = "{pubkey_1}"
network_pubkey = "212121212121212121212121212121212121212121212121"
rpc_endpoints = ["http://127.0.0.1:7101"]
region = "local-a"
operator_name = "validator-a"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0202020202020202020202020202020202020202020202020202020202020202"
consensus_pubkey = "{pubkey_2}"
network_pubkey = "222222222222222222222222222222222222222222222222"
rpc_endpoints = ["http://127.0.0.1:7102"]
region = "local-b"
operator_name = "validator-b"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0303030303030303030303030303030303030303030303030303030303030303"
consensus_pubkey = "{pubkey_3}"
network_pubkey = "232323232323232323232323232323232323232323232323"
rpc_endpoints = ["http://127.0.0.1:7103"]
region = "local-c"
operator_name = "validator-c"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0404040404040404040404040404040404040404040404040404040404040404"
consensus_pubkey = "{pubkey_4}"
network_pubkey = "242424242424242424242424242424242424242424242424"
rpc_endpoints = ["http://127.0.0.1:7104"]
region = "local-d"
operator_name = "validator-d"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"
"#,
    ))
    .expect("validator set should parse")
}

fn configured_validator_set_for(
    local: &SingleNodeConfig,
    peer_validator_id: [u8; 32],
    peer_signing_key_byte: u8,
) -> ValidatorSet {
    let local_validator_id = encode_hex(&local.validator_id);
    let local_consensus_pubkey = encode_hex(
        &SigningKey::from_bytes(&local.consensus_signing_key)
            .verifying_key()
            .to_bytes(),
    );
    let peer_validator_id = encode_hex(&peer_validator_id);
    let peer_consensus_pubkey = encode_hex(
        &signing_key(peer_signing_key_byte)
            .verifying_key()
            .to_bytes(),
    );
    load_validator_set_from_str(&format!(
        r#"
epoch = 1

[[validators]]
validator_id = "{local_validator_id}"
consensus_pubkey = "{local_consensus_pubkey}"
network_pubkey = "111111111111111111111111111111111111111111111111"
rpc_endpoints = ["http://127.0.0.1:8101"]
region = "local-a"
operator_name = "validator-a"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "{peer_validator_id}"
consensus_pubkey = "{peer_consensus_pubkey}"
network_pubkey = "222222222222222222222222222222222222222222222222"
rpc_endpoints = ["http://127.0.0.1:8102"]
region = "local-b"
operator_name = "validator-b"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"
"#
    ))
    .expect("validator set should parse")
}

fn sign_current_test_head(
    engine: &mut SingleNodeEngine,
    validator_verifiers: &[kombat_chain_consensus::ValidatorVerifier],
    signing_keys: &BTreeMap<[u8; 32], [u8; 32]>,
) {
    let head = engine.head().clone();
    let head_qc = build_quorum_certificate(&head, signing_keys).expect("head qc should build");
    engine
        .install_head_qc(head_qc, validator_verifiers)
        .expect("head qc should install");
}

fn apply_test_network_block(
    engine: &mut SingleNodeEngine,
    proposer: [u8; 32],
    timestamp_ms: u64,
    validator_verifiers: &[kombat_chain_consensus::ValidatorVerifier],
    signing_keys: &BTreeMap<[u8; 32], [u8; 32]>,
) -> Block {
    let block = engine
        .prepare_network_block_from_mempool(proposer, validator_verifiers, timestamp_ms, 10, 1_000)
        .expect("network block should prepare");
    let qc = build_quorum_certificate(&block, signing_keys).expect("network qc should build");
    engine
        .apply_external_block(block.clone(), qc, validator_verifiers, proposer, 1_000)
        .expect("network block should apply");
    block
}

#[test]
fn configured_validator_set_uses_canonical_genesis_across_local_validators() {
    let configured_validator_set = four_node_validator_set();
    let mut config_one = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let engine_one = SingleNodeEngine::new_with_validator_set(
        config_one.clone(),
        Some(&configured_validator_set),
    );

    config_one.validator_id = validator(3);
    config_one.consensus_signing_key = [3; 32];
    let engine_three =
        SingleNodeEngine::new_with_validator_set(config_one, Some(&configured_validator_set));

    assert_eq!(
        block_hash(engine_one.head()),
        block_hash(engine_three.head())
    );
    assert_eq!(engine_one.head().header.proposer, validator(1));
    assert_eq!(engine_three.head().header.proposer, validator(1));
    assert_eq!(engine_one.local_validator_id(), Some(validator(1)));
    assert_eq!(engine_three.local_validator_id(), Some(validator(3)));
}

fn local_cluster_config() -> LocalClusterConfig {
    LocalClusterConfig {
        chain_id: 7,
        validator_set: four_node_validator_set(),
        consensus_signing_keys: BTreeMap::from([
            (validator(1), [1; 32]),
            (validator(2), [2; 32]),
            (validator(3), [3; 32]),
            (validator(4), [4; 32]),
        ]),
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [6; 32],
        pacemaker: pacemaker(),
    }
}

fn sample_policy() -> PolicyRecord {
    sample_policy_for_account(11)
}

fn sample_policy_for_account(account_id: u64) -> PolicyRecord {
    let signer = signing_key(8);
    PolicyRecord::new(
        42,
        account_id,
        signer.verifying_key().to_bytes(),
        1_000,
        172_800_000,
        PolicyState::Active,
        PolicyTerms {
            gas_budget_per_day: 100,
            gas_refill_per_day: 100,
            max_intents_per_second: 2,
            max_intents_per_day: 10,
            max_volume_per_day: 1_000,
            max_drawdown_pct: 25,
            ..PolicyTerms::default()
        },
    )
}

fn list_test_perpetual_market(engine: &mut SingleNodeEngine, market_id: u64) {
    engine
        .list_perpetual_market(
            market_id,
            "BTC".into(),
            1,
            market_id + 70,
            btc_perp_config(),
            1_000,
        )
        .expect("test market should list");
}

fn list_scaled_test_perpetual_market(
    engine: &mut SingleNodeEngine,
    market_id: u64,
    tick_size_bps: u32,
    lot_size_microunits: u64,
) {
    let mut config = btc_perp_config();
    config.tick_size_bps = tick_size_bps;
    config.lot_size_microunits = lot_size_microunits;
    engine
        .list_perpetual_market(market_id, "BTC".into(), 1, market_id + 70, config, 1_000)
        .expect("scaled test market should list");
}

fn list_fast_funding_test_market(engine: &mut SingleNodeEngine, market_id: u64) {
    let mut config = btc_perp_config();
    config.funding_interval_ms = 100;
    engine
        .list_perpetual_market(market_id, "BTC".into(), 1, market_id + 70, config, 1_000)
        .expect("test market should list");
}

fn seed_zero_equity_open_position(engine: &mut SingleNodeEngine, account_id: u64) {
    list_test_perpetual_market(engine, 7);
    engine.update_market_index_price(7, 100, 1_001);
    engine.update_market_mark_price(7, 100, 1_001);
    engine.positions.insert(
        (7, account_id),
        PositionLedger {
            open_net_lots: 1,
            open_notional: 100,
            opened_at_timestamp_ms: Some(1_001),
            ..PositionLedger::default()
        },
    );
    if engine.margin_snapshot(7, account_id).under_maintenance {
        engine.under_maintenance_accounts.insert((7, account_id));
    }
}

fn signed_intent(
    signer: &SigningKey,
    nonce: u64,
    expires_at: u64,
    kind: IntentKind,
) -> SignedIntent {
    signed_intent_for_policy(signer, 42, 11, nonce, expires_at, kind)
}

fn signed_intent_for_policy(
    signer: &SigningKey,
    policy_id: u64,
    account: u64,
    nonce: u64,
    expires_at: u64,
    kind: IntentKind,
) -> SignedIntent {
    let intent = Intent {
        chain_id: 7,
        account,
        policy_id,
        nonce,
        expires_at,
        predicate: None,
        kind,
    };
    let session_payload = SessionPayload {
        intent_hash: intent_signing_hash(&intent),
        policy_id: intent.policy_id,
        chain_id: 7,
    };
    let signature = signer
        .sign(&borsh::to_vec(&session_payload).expect("session payload should serialize"))
        .to_bytes();

    SignedIntent { intent, signature }
}

fn signed_time_after_intent(
    signer: &SigningKey,
    nonce: u64,
    not_before_ms: u64,
    expires_at: u64,
    kind: IntentKind,
) -> SignedIntent {
    let mut signed_intent = signed_intent(signer, nonce, expires_at, kind);
    signed_intent.intent.predicate = Some(IntentPredicate::TimeAfter { not_before_ms });
    let session_payload = SessionPayload {
        intent_hash: intent_signing_hash(&signed_intent.intent),
        policy_id: signed_intent.intent.policy_id,
        chain_id: 7,
    };
    signed_intent.signature = signer
        .sign(&borsh::to_vec(&session_payload).expect("session payload should serialize"))
        .to_bytes();
    signed_intent
}

fn signed_time_before_intent(
    signer: &SigningKey,
    nonce: u64,
    not_after_ms: u64,
    expires_at: u64,
    kind: IntentKind,
) -> SignedIntent {
    let mut signed_intent = signed_intent(signer, nonce, expires_at, kind);
    signed_intent.intent.predicate = Some(IntentPredicate::TimeBefore { not_after_ms });
    let session_payload = SessionPayload {
        intent_hash: intent_signing_hash(&signed_intent.intent),
        policy_id: signed_intent.intent.policy_id,
        chain_id: 7,
    };
    signed_intent.signature = signer
        .sign(&borsh::to_vec(&session_payload).expect("session payload should serialize"))
        .to_bytes();
    signed_intent
}

fn signed_owner_call(
    signer: &SigningKey,
    account_id: u64,
    nonce: u64,
    action: OwnerAction,
) -> SignedOwnerCall {
    let owner_call = OwnerCall {
        signer: signer.verifying_key().to_bytes(),
        nonce,
        action,
    };
    let signature = signer
        .sign(
            &borsh::to_vec(&(account_id, 7_u64, &owner_call))
                .expect("owner call payload should serialize"),
        )
        .to_bytes();

    SignedOwnerCall {
        account_id,
        chain_id: 7,
        call: owner_call,
        signature,
    }
}

#[test]
fn equity_history_samples_zero_equity_open_position() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_zero_equity_open_position(&mut engine, 11);
    assert!(engine.all_equity_history_samples().is_empty());

    engine
        .produce_empty_block(1_100)
        .expect("block should sample active account");

    let samples = engine.equity_history_samples(11, 1, 2);
    assert_eq!(samples.len(), 1);
    let sample = samples[0];
    assert_eq!(sample.block_height, 1);
    assert_eq!(sample.timestamp_ms, 1_100);
    assert_eq!(sample.total_equity, 0);
    assert_eq!(sample.total_collateral, 0);
    assert_eq!(sample.position_count, 1);
    assert!(sample.total_maintenance_requirement > 0);
    assert!(sample.total_liquidation_gap > 0);
    assert!(sample.under_maintenance);
}

#[test]
fn equity_history_does_not_persist_idle_account_every_block() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_zero_equity_open_position(&mut engine, 11);

    engine.produce_empty_block(1_100).expect("first sample");
    engine
        .produce_empty_block(1_110)
        .expect("unchanged idle block");
    engine
        .produce_empty_block(1_120)
        .expect("unchanged idle block");
    assert_eq!(engine.equity_history_samples(11, 1, 4).len(), 1);

    engine
        .produce_empty_block(61_100)
        .expect("60s cadence sample");
    assert_eq!(engine.equity_history_samples(11, 1, 5).len(), 2);
}

#[test]
fn equity_history_file_snapshot_restore_starts_empty() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_zero_equity_open_position(&mut engine, 11);
    engine.produce_empty_block(1_100).expect("first sample");
    assert_eq!(engine.equity_history_samples(11, 1, 2).len(), 1);

    let mut restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    assert!(restored.all_equity_history_samples().is_empty());
    restored
        .produce_empty_block(61_100)
        .expect("restored engine should resample without prior history");
    assert_eq!(restored.equity_history_samples(11, 2, 3).len(), 1);
}

#[test]
fn equity_history_is_deterministic_across_snapshot_restore_and_replay() {
    let mut base = SingleNodeEngine::new(single_node_config());
    seed_zero_equity_open_position(&mut base, 11);
    let mut fresh = base.clone();
    let mut restored = SingleNodeEngine::from_snapshot_bytes(&base.snapshot_bytes())
        .expect("snapshot should restore");

    for timestamp in [1_100, 61_100, 121_100] {
        fresh
            .produce_empty_block(timestamp)
            .expect("fresh block should produce");
        restored
            .produce_empty_block(timestamp)
            .expect("restored block should produce");
        assert_eq!(
            restored.all_equity_history_samples(),
            fresh.all_equity_history_samples()
        );
    }
}

#[test]
fn equity_history_prunes_to_dense_window_plus_daily_tail() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let now = 1_700_000_000_000_u64;
    let dense_cutoff = now.saturating_sub(ACCOUNT_EQUITY_HISTORY_DENSE_WINDOW_MS);
    let tail_cutoff = dense_cutoff.saturating_sub(ACCOUNT_EQUITY_HISTORY_DAILY_WINDOW_MS);
    let mut rows = Vec::new();

    rows.push((
        11,
        AccountEquitySample {
            block_height: 1,
            timestamp_ms: tail_cutoff.saturating_sub(1),
            total_equity: 1,
            ..AccountEquitySample::default()
        },
    ));
    for day in 1..=3_u64 {
        let base = dense_cutoff.saturating_sub(day * UTC_DAY_MS);
        rows.push((
            11,
            AccountEquitySample {
                block_height: 10 + day * 2,
                timestamp_ms: base.saturating_add(1),
                total_equity: day as i128,
                ..AccountEquitySample::default()
            },
        ));
        rows.push((
            11,
            AccountEquitySample {
                block_height: 11 + day * 2,
                timestamp_ms: base.saturating_add(2),
                total_equity: (day as i128) + 10,
                ..AccountEquitySample::default()
            },
        ));
    }
    rows.push((
        11,
        AccountEquitySample {
            block_height: 100,
            timestamp_ms: dense_cutoff,
            total_equity: 100,
            ..AccountEquitySample::default()
        },
    ));
    rows.push((
        11,
        AccountEquitySample {
            block_height: 101,
            timestamp_ms: now,
            total_equity: 101,
            ..AccountEquitySample::default()
        },
    ));

    engine.set_equity_history_samples(rows);
    engine.prune_account_equity_history(now);

    let samples = engine.equity_history_samples(11, 0, u64::MAX);
    assert_eq!(samples.len(), 5);
    assert!(samples
        .iter()
        .all(|sample| sample.timestamp_ms >= tail_cutoff));
    assert!(samples.iter().any(|sample| sample.block_height == 100));
    assert!(samples.iter().any(|sample| sample.block_height == 101));
    for day in 1..=3_u64 {
        let base = dense_cutoff.saturating_sub(day * UTC_DAY_MS);
        let retained = samples
            .iter()
            .filter(|sample| utc_day_bucket_ms(sample.timestamp_ms) == utc_day_bucket_ms(base))
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].timestamp_ms, base.saturating_add(2));
    }
}

#[test]
fn reports_component_name() {
    assert_eq!(component_name(), "node");
}

#[test]
fn produces_single_node_chain() {
    let mut engine = SingleNodeEngine::new(single_node_config());

    let first = engine
        .produce_empty_block(1_100)
        .expect("first block should produce")
        .clone();
    let second = engine
        .produce_empty_block(1_200)
        .expect("second block should produce")
        .clone();

    assert_eq!(engine.blocks().len(), 3);
    assert_eq!(first.header.height, 1);
    assert_eq!(second.header.height, 2);
    assert_eq!(first.header.round, 1);
    assert_eq!(second.header.round, 2);
    assert_eq!(first.header.parent_hash, block_hash(&engine.blocks()[0]));
    assert_eq!(second.header.parent_hash, block_hash(&first));
    assert_eq!(engine.current_round(), 3);
}

#[test]
fn snapshot_preserves_configured_validator_membership_metadata() {
    let config = single_node_config();
    let configured_validator_set = configured_validator_set_for(&config, validator(2), 2);
    let engine =
        SingleNodeEngine::new_with_validator_set(config.clone(), Some(&configured_validator_set));

    assert_eq!(engine.validator_set(), &[config.validator_id]);
    assert_eq!(
        engine.configured_validator_ids(),
        vec![validator(2), config.validator_id]
    );
    assert_eq!(
        engine.configured_validator_consensus_pubkey(validator(2)),
        Some(signing_key(2).verifying_key().to_bytes())
    );
    assert_eq!(engine.metrics().peer_count, 1);

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    assert_eq!(restored.validator_set(), &[config.validator_id]);
    assert_eq!(
        restored.configured_validator_ids(),
        vec![validator(2), config.validator_id]
    );
    assert_eq!(
        restored.configured_validator_consensus_pubkey(validator(2)),
        Some(signing_key(2).verifying_key().to_bytes())
    );
    assert_eq!(restored.metrics().peer_count, 1);
}

#[test]
fn rebinds_bootstrapped_snapshot_to_new_local_validator_identity() {
    let config = single_node_config();
    let configured_validator_set = configured_validator_set_for(&config, validator(2), 2);
    let mut restored = SingleNodeEngine::from_snapshot_bytes(
        &SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set))
            .snapshot_bytes(),
    )
    .expect("snapshot should restore");

    restored
        .rebind_local_validator(validator(2), [2; 32])
        .expect("rebind should succeed");

    assert_eq!(restored.validator_set(), &[validator(2)]);
    assert_eq!(
        restored.validator_consensus_pubkey(validator(2)),
        Some(signing_key(2).verifying_key().to_bytes())
    );
    assert_eq!(
        restored.configured_validator_consensus_pubkey(validator(2)),
        Some(signing_key(2).verifying_key().to_bytes())
    );
    assert_eq!(restored.metrics().peer_count, 1);
}

#[test]
fn snapshot_restore_normalizes_legacy_single_node_peer_count() {
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let mut snapshot =
        SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set))
            .persisted_snapshot();
    snapshot.telemetry.rewrite_peer_count(0);

    let restored = SingleNodeEngine::try_from_persisted_snapshot(snapshot)
        .expect("legacy peer-count snapshot should restore");

    assert_eq!(restored.metrics().peer_count, 3);
    assert!(restored.logs().iter().all(|entry| entry.peer_count == 3));
}

#[test]
fn snapshot_restore_accepts_committed_network_proposer_history() {
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut engine =
        SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set));
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    apply_test_network_block(
        &mut engine,
        validator(1),
        1_100,
        &validator_verifiers,
        &signing_keys,
    );
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    apply_test_network_block(
        &mut engine,
        validator(2),
        1_200,
        &validator_verifiers,
        &signing_keys,
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot with network proposer history should restore");
    assert_eq!(restored.head().header.height, 2);
    assert_eq!(restored.head().header.proposer, validator(2));
    assert_eq!(restored.current_round(), 3);
}

#[test]
fn snapshot_restore_accepts_rebound_bootstrap_history_from_other_validator() {
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut engine =
        SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set));
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    apply_test_network_block(
        &mut engine,
        validator(1),
        1_100,
        &validator_verifiers,
        &signing_keys,
    );
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    apply_test_network_block(
        &mut engine,
        validator(2),
        1_200,
        &validator_verifiers,
        &signing_keys,
    );

    let mut rebound = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("original snapshot should restore");
    rebound
        .rebind_local_validator(validator(2), [2; 32])
        .expect("rebind should succeed");

    let restored = SingleNodeEngine::from_snapshot_bytes(&rebound.snapshot_bytes())
        .expect("rebound snapshot with bootstrap history should restore");
    assert_eq!(restored.validator_set(), &[validator(2)]);
    assert_eq!(restored.head().header.height, 2);
    assert_eq!(restored.head().header.proposer, validator(2));
    assert_eq!(restored.current_round(), 3);
}

#[test]
fn snapshot_restore_accepts_timeout_history_before_external_block() {
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut engine =
        SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set));
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    apply_test_network_block(
        &mut engine,
        validator(1),
        1_100,
        &validator_verifiers,
        &signing_keys,
    );
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    engine.record_timeout(10_000);
    engine.record_view_change(10_000);
    apply_test_network_block(
        &mut engine,
        validator(2),
        1_200,
        &validator_verifiers,
        &signing_keys,
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot with timeout history before network block should restore");
    let timestamps: Vec<u64> = restored
        .logs()
        .iter()
        .map(|entry| entry.timestamp_ms)
        .collect();
    assert!(timestamps.windows(2).all(|window| window[1] >= window[0]));
    assert_eq!(restored.head().header.height, 2);
    assert_eq!(restored.head().header.proposer, validator(2));
    assert_eq!(restored.current_round(), 3);
}

#[test]
fn snapshot_restore_normalizes_legacy_telemetry_timestamp_regression() {
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut engine =
        SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set));
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    apply_test_network_block(
        &mut engine,
        validator(1),
        1_100,
        &validator_verifiers,
        &signing_keys,
    );
    sign_current_test_head(&mut engine, &validator_verifiers, &signing_keys);
    engine.record_timeout(10_000);
    engine.record_view_change(10_000);
    apply_test_network_block(
        &mut engine,
        validator(2),
        1_200,
        &validator_verifiers,
        &signing_keys,
    );

    let mut snapshot = engine.persisted_snapshot();
    let last_index = snapshot.telemetry.logs().len().saturating_sub(1);
    let prior_timestamp = snapshot.telemetry.logs()[last_index - 1].timestamp_ms;
    snapshot.telemetry.logs_mut()[last_index].timestamp_ms = prior_timestamp.saturating_sub(1);

    let restored = SingleNodeEngine::try_from_persisted_snapshot(snapshot)
        .expect("legacy telemetry timestamp regression should normalize on restore");
    let timestamps: Vec<u64> = restored
        .logs()
        .iter()
        .map(|entry| entry.timestamp_ms)
        .collect();
    assert!(timestamps.windows(2).all(|window| window[1] >= window[0]));
    assert_eq!(restored.head().header.height, 2);
    assert_eq!(restored.current_round(), 3);
}

#[test]
fn carries_custom_body_roots() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let body = kombat_chain_consensus::BlockBody {
        items: vec![BlockItem::Intent(signed_intent(
            &signing_key(8),
            1,
            10_000,
            IntentKind::Transfer {
                to: 22,
                token: 1,
                amount: 25,
            },
        ))],
    };

    let block = engine
        .produce_block(body.clone(), [8; 32], [9; 32], 1_050)
        .expect("block should produce");

    assert_eq!(block.body, body);
    assert_eq!(block.header.state_root, [8; 32]);
    assert_eq!(block.header.event_root, [9; 32]);
    assert_eq!(engine.current_round(), 2);
}

#[test]
fn rejects_timestamp_regression() {
    let mut engine = SingleNodeEngine::new(single_node_config());

    let err = engine
        .produce_empty_block(999)
        .expect_err("timestamp regression should fail");

    assert_eq!(
        err,
        NodeError::InvalidBlock(
            kombat_chain_consensus::BlockValidationError::NonMonotonicTimestamp
        )
    );
    assert_eq!(engine.blocks().len(), 1);
}

#[test]
fn rejects_mempool_timestamp_regression_before_native_install() {
    let mut engine = SingleNodeEngine::new(single_node_config());

    let err = engine
        .produce_block_from_mempool(999, 10)
        .expect_err("timestamp regression should fail");

    assert_eq!(
        err,
        NodeError::InvalidBlock(
            kombat_chain_consensus::BlockValidationError::NonMonotonicTimestamp
        )
    );
    assert_eq!(engine.blocks().len(), 1);
}

#[test]
fn exposes_timeout_from_pacemaker() {
    let engine = SingleNodeEngine::new(single_node_config());

    assert_eq!(engine.round_timeout_ms(), 300);
    assert_eq!(engine.validator_set(), &[validator(9)]);
}

#[test]
fn single_node_engine_submits_and_queries_book_state() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 5,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 4,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 101 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(ask, 1_000).expect("ask should rest");
    engine.submit_order(bid, 1_001).expect("bid should rest");

    let (bids, asks) = engine.book_levels(7, 5, 1_100);
    assert_eq!(bids[0].price_ticks, 101);
    assert_eq!(asks[0].price_ticks, 103);
    assert_eq!(
        engine
            .resting_order(7, 22, 1, 1_100)
            .expect("ask should be queryable")
            .price_ticks,
        103
    );
    assert_eq!(
        engine
            .account_orders(7, 33, 1_100, 10)
            .into_iter()
            .map(|order| order.client_order_id)
            .collect::<Vec<_>>(),
        vec![2]
    );
}

#[test]
fn single_node_engine_emits_fill_events_for_both_sides() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 5,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let crossing_bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 104 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(ask, 1_000).expect("ask should rest");
    engine
        .submit_order(crossing_bid, 1_001)
        .expect("crossing bid should fill");

    let fill_events: Vec<_> = engine
        .runtime_events()
        .iter()
        .filter(|event| matches!(event.kind, RuntimeEventKind::OrderFilled { .. }))
        .collect();
    assert_eq!(fill_events.len(), 2);
    assert!(fill_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::OrderFilled {
            market_id,
            account_id,
            client_order_id,
            counterparty_account_id,
            counterparty_client_order_id,
            mandate_id,
            role,
            side,
            price_ticks,
            quantity_lots,
        } if *market_id == 7
            && *account_id == 22
            && *client_order_id == 1
            && *counterparty_account_id == 33
            && *counterparty_client_order_id == 2
            && mandate_id.is_none()
            && *role == OrderFillRole::Maker
            && *side == OrderFillSide::Sell
            && *price_ticks == 103
            && *quantity_lots == 3
    )));
    assert!(fill_events.iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::OrderFilled {
            market_id,
            account_id,
            client_order_id,
            counterparty_account_id,
            counterparty_client_order_id,
            mandate_id,
            role,
            side,
            price_ticks,
            quantity_lots,
        } if *market_id == 7
            && *account_id == 33
            && *client_order_id == 2
            && *counterparty_account_id == 22
            && *counterparty_client_order_id == 1
            && mandate_id.is_none()
            && *role == OrderFillRole::Taker
            && *side == OrderFillSide::Buy
            && *price_ticks == 103
            && *quantity_lots == 3
    )));
}

#[test]
fn single_node_engine_tracks_positions_from_fills() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 5,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let crossing_bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 104 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(ask, 1_000).expect("ask should rest");
    engine
        .submit_order(crossing_bid, 1_001)
        .expect("crossing bid should fill");

    let maker = engine.position(7, 22);
    assert_eq!(maker.net_lots(), -3);
    assert_eq!(maker.open_notional, 309);
    assert_eq!(maker.average_entry_price_ticks(), Some(103));
    assert_eq!(maker.sell_lots, 3);
    assert_eq!(maker.sell_notional, 309);

    let taker = engine.position(7, 33);
    assert_eq!(taker.net_lots(), 3);
    assert_eq!(taker.open_notional, 309);
    assert_eq!(taker.average_entry_price_ticks(), Some(103));
    assert_eq!(taker.buy_lots, 3);
    assert_eq!(taker.buy_notional, 309);

    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarkPriceUpdated {
            market_id,
            mark_price_ticks,
        } if *market_id == 7 && *mark_price_ticks == 103
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::PositionUpdated {
            market_id,
            account_id,
            net_lots,
            open_notional,
            buy_lots,
            sell_lots,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *net_lots == -3
            && *open_notional == 309
            && *buy_lots == 0
            && *sell_lots == 3
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RiskStateUpdated {
            market_id,
            account_id,
            under_maintenance,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *under_maintenance
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationCandidateUpdated {
            market_id,
            account_id,
            under_maintenance,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *under_maintenance
    )));
}

#[test]
fn position_ledger_tracks_close_and_flip_exposure() {
    let mut ledger = PositionLedger::default();
    ledger.apply_fill(OrderFillSide::Buy, 100, 10, None, 0, 1, 1_000);
    assert_eq!(ledger.net_lots(), 10);
    assert_eq!(ledger.open_notional, 1_000);
    assert_eq!(ledger.realized_pnl, 0);
    assert_eq!(ledger.average_entry_price_ticks(), Some(100));
    assert_eq!(ledger.opened_at_timestamp_ms, Some(1_000));

    ledger.apply_fill(OrderFillSide::Sell, 110, 4, None, 0, 2, 1_100);
    assert_eq!(ledger.net_lots(), 6);
    assert_eq!(ledger.open_notional, 600);
    assert_eq!(ledger.realized_pnl, 40);
    assert_eq!(ledger.average_entry_price_ticks(), Some(100));
    assert_eq!(ledger.opened_at_timestamp_ms, Some(1_000));

    ledger.apply_fill(OrderFillSide::Sell, 120, 8, None, 0, 3, 1_200);
    assert_eq!(ledger.net_lots(), -2);
    assert_eq!(ledger.open_notional, 240);
    assert_eq!(ledger.realized_pnl, 160);
    assert_eq!(ledger.average_entry_price_ticks(), Some(120));
    assert_eq!(ledger.opened_at_timestamp_ms, Some(1_200));
}

#[test]
fn single_node_engine_computes_margin_snapshot_from_collateral_and_mark() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 40);
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let reprice_bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 44,
        client_order_id: 3,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let reprice_ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 55,
        client_order_id: 4,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(ask, 1_000).expect("ask should rest");
    engine.submit_order(bid, 1_001).expect("bid should fill");
    engine
        .submit_order(reprice_bid, 1_002)
        .expect("reprice bid should rest");
    engine
        .submit_order(reprice_ask, 1_003)
        .expect("reprice ask should fill");

    let snapshot = engine.margin_snapshot(7, 22);
    assert_eq!(snapshot.collateral, 40);
    assert_eq!(snapshot.mark_notional, 270);
    assert_eq!(snapshot.realized_pnl, 0);
    assert_eq!(snapshot.unrealized_pnl, -30);
    assert_eq!(snapshot.maintenance_margin_bps, 500);
    assert_eq!(snapshot.maintenance_requirement, 14);
    assert_eq!(snapshot.equity, 10);
    assert_eq!(snapshot.margin_surplus, -4);
    assert!(snapshot.under_maintenance);
}

#[test]
fn collateral_updates_emit_liquidation_candidate_clear_event() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let reprice_bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 44,
        client_order_id: 3,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let reprice_ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 55,
        client_order_id: 4,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 1,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(ask, 1_000).expect("ask should rest");
    engine.submit_order(bid, 1_001).expect("bid should fill");
    engine
        .submit_order(reprice_bid, 1_002)
        .expect("reprice bid should rest");
    engine
        .submit_order(reprice_ask, 1_003)
        .expect("reprice ask should fill");
    engine.update_isolated_collateral(7, 22, 60, 1_004);

    let transitions: Vec<_> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::LiquidationCandidateUpdated {
                market_id,
                account_id,
                under_maintenance,
                ..
            } if *market_id == 7 && *account_id == 22 => Some(*under_maintenance),
            _ => None,
        })
        .collect();

    assert_eq!(transitions, vec![true, false]);
}

#[test]
fn funding_update_realizes_pnl_for_open_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(bid, 1_000).expect("bid should rest");
    engine.submit_order(ask, 1_001).expect("ask should fill");
    engine.update_market_funding_index(7, 2, 1_002);

    assert_eq!(engine.position(7, 22).realized_pnl, -6);
    assert_eq!(engine.position(7, 33).realized_pnl, 6);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingIndexUpdated {
            market_id,
            funding_index,
        } if *market_id == 7 && *funding_index == 2
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingApplied {
            market_id,
            account_id,
            funding_delta,
            funding_index,
        } if *market_id == 7
            && *account_id == 22
            && *funding_delta == -6
            && *funding_index == 2
    )));
}

#[test]
fn advance_market_funding_derives_index_from_oracle_inputs() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit {
            price_ticks: 101_000,
        },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit {
            price_ticks: 101_000,
        },
        time_in_force: kombat_chain_matching::TimeInForce::Ioc,
        post_only: false,
    };

    engine.submit_order(bid, 1_000).expect("bid should rest");
    engine.submit_order(ask, 1_001).expect("ask should fill");
    engine.update_market_index_price(7, 100_000, 1_002);

    let next_index = engine
        .advance_market_funding(7, 1_003)
        .expect("funding advance should succeed");

    assert_eq!(next_index, 750);
    assert_eq!(engine.market_funding_index(7), 750);
    assert_eq!(engine.position(7, 22).realized_pnl, -2_250);
    assert_eq!(engine.position(7, 33).realized_pnl, 2_250);
}

fn seed_consensus_funding_pair(engine: &mut SingleNodeEngine) {
    list_fast_funding_test_market(engine, 7);
    engine.set_isolated_collateral(7, 22, 50_000);
    engine.set_isolated_collateral(7, 33, 50_000);
    engine.update_market_index_price(7, 100_000, 1_000);
    engine.update_market_mark_price(7, 101_000, 1_000);
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit {
                    price_ticks: 101_000,
                },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        )
        .expect("bid should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit {
                    price_ticks: 101_000,
                },
                time_in_force: kombat_chain_matching::TimeInForce::Ioc,
                post_only: false,
            },
            1_002,
        )
        .expect("ask should fill");
}

fn seed_funding_induced_liquidation_candidate(engine: &mut SingleNodeEngine) {
    let mut config = btc_perp_config();
    config.initial_margin_bps = 500;
    config.maintenance_margin_bps = 500;
    config.funding_interval_ms = 100;
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, config, 1_000)
        .expect("test market should list");
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 16_000);
    engine.set_isolated_collateral(7, 33, 50_000);
    engine.set_isolated_collateral(7, 44, 50_000);
    engine.update_market_index_price(7, 100_000, 1_000);
    engine.update_market_mark_price(7, 101_000, 1_000);
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit {
                    price_ticks: 101_000,
                },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        )
        .expect("candidate bid should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit {
                    price_ticks: 101_000,
                },
                time_in_force: kombat_chain_matching::TimeInForce::Ioc,
                post_only: false,
            },
            1_002,
        )
        .expect("candidate ask should fill");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit {
                    price_ticks: 101_000,
                },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        )
        .expect("liquidation liquidity should rest");

    assert_eq!(engine.position(7, 22).net_lots(), 3);
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
}

#[test]
fn produce_empty_block_advances_due_funding_interval() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_funding_pair(&mut engine);

    let before_due = engine
        .produce_empty_block(1_099)
        .expect("pre-funding block should produce")
        .clone();
    assert!(before_due.body.items.is_empty());
    assert_eq!(engine.market_funding_index(7), 0);

    let funded = engine
        .produce_empty_block(1_100)
        .expect("funding block should produce")
        .clone();

    assert!(matches!(
        funded.body.items.as_slice(),
        [BlockItem::Funding(BlockFunding {
            market_id: 7,
            funding_index: 750,
        })]
    ));
    assert_eq!(engine.market_funding_index(7), 750);
    assert_eq!(engine.market_last_funding_ms(7), Some(1_100));
    assert_eq!(engine.position(7, 22).realized_pnl, -2_250);
    assert_eq!(engine.position(7, 33).realized_pnl, 2_250);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingIndexUpdated {
            market_id: 7,
            funding_index: 750,
        } if event.block_height == Some(funded.header.height)
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingApplied {
            market_id: 7,
            account_id: 22,
            funding_delta: -2_250,
            funding_index: 750,
        } if event.block_height == Some(funded.header.height)
    )));
}

#[test]
fn due_funding_is_suppressed_when_oracle_inputs_are_stale() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_funding_pair(&mut engine);
    engine.update_market_oracle_config(
        7,
        kombat_chain_oracle::OracleConfig {
            max_staleness_ms: 1,
        },
        1_099,
    );

    let block = engine
        .produce_empty_block(1_100)
        .expect("stale funding block should produce")
        .clone();

    assert!(block.body.items.is_empty());
    assert_eq!(engine.market_funding_index(7), 0);
    assert_eq!(engine.market_last_funding_ms(7), Some(1_000));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingIndexUpdated { market_id: 7, .. }
            if event.block_height == Some(block.header.height)
    )));
}

#[test]
fn same_block_funding_can_trigger_consensus_liquidation() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_funding_induced_liquidation_candidate(&mut engine);

    let block = engine
        .produce_block_from_mempool(1_100, 0)
        .expect("funding liquidation block should produce")
        .clone();

    assert!(matches!(
        block.body.items.as_slice(),
        [
            BlockItem::Funding(BlockFunding {
                market_id: 7,
                funding_index: 750,
            }),
            BlockItem::Liquidation(BlockLiquidation {
                market_id: 7,
                account_id: 22,
                requested_lots: Some(1),
                filled_lots: 1,
                remaining_lots: 0,
                keeper_reward_micro_usdc: 0,
                ..
            })
        ]
    ));
    assert_eq!(engine.market_funding_index(7), 750);
    assert!(engine.position(7, 22).net_lots() < 3);
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingApplied {
            market_id: 7,
            account_id: 22,
            funding_delta: -2_250,
            funding_index: 750,
        } if event.block_height == Some(block.header.height)
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id: 7,
            account_id: 22,
            ..
        } if event.block_height == Some(block.header.height)
    )));
}

#[test]
fn external_block_applies_consensus_funding_item() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    seed_consensus_funding_pair(&mut proposer);
    seed_consensus_funding_pair(&mut follower);

    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_100,
            0,
            1_000,
        )
        .expect("network funding block should prepare");
    let block = prepared.block().clone();
    assert!(matches!(
        block.body.items.as_slice(),
        [BlockItem::Funding(BlockFunding {
            market_id: 7,
            funding_index: 750,
        })]
    ));
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared funding block should apply locally");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external funding block should apply");

    assert_eq!(
        follower.market_funding_index(7),
        proposer.market_funding_index(7)
    );
    assert_eq!(follower.position(7, 22), proposer.position(7, 22));
    assert_eq!(follower.position(7, 33), proposer.position(7, 33));
    assert!(follower.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingApplied {
            market_id: 7,
            account_id: 22,
            funding_delta: -2_250,
            funding_index: 750,
        } if event.block_height == Some(1)
    )));
}

#[test]
fn prepared_block_uses_block_oracle_prices_for_consensus_liquidation_validation() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    for engine in [&mut proposer, &mut follower] {
        seed_funding_induced_liquidation_candidate(engine);
        engine.update_market_index_price(7, 101_000, 1_004);
        engine.update_market_mark_price(7, 101_000, 1_004);
        assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    }

    let oracle_prices = vec![BlockOraclePrice {
        market_id: 7,
        mark: Some(BlockOracleValue {
            price_ticks: 101_000,
            timestamp_ms: 1_100,
        }),
        index: Some(BlockOracleValue {
            price_ticks: 100_000,
            timestamp_ms: 1_100,
        }),
    }];
    let (market_marks, market_index_prices) =
        proposer.oracle_price_maps_for_mahi_block(&oracle_prices);
    let funding_items =
        proposer.consensus_funding_items_with_prices(1_100, &market_marks, &market_index_prices);
    assert!(matches!(
        funding_items.as_slice(),
        [BlockItem::Funding(BlockFunding {
            market_id: 7,
            funding_index: 750,
        })]
    ));
    let positions_after_funding = proposer.positions_after_funding_items_with_prices(
        &funding_items,
        1_100,
        &market_marks,
        &market_index_prices,
    );
    assert!(
        proposer
            .margin_snapshot_for_ledger(
                7,
                22,
                *positions_after_funding
                    .get(&(7, 22))
                    .expect("candidate position should exist"),
            )
            .under_maintenance
    );

    let mut body = BlockBody {
        items: funding_items.clone(),
    };
    let empty_order_book_overlays = BTreeMap::new();
    let empty_resting_order_mandates = BTreeMap::new();
    let empty_collateral_overlays = BTreeMap::new();
    let empty_committed_events = Vec::new();
    body.items
        .extend(proposer.consensus_liquidation_items_with_prices(
            1,
            1_100,
            &positions_after_funding,
            &market_marks,
            &market_index_prices,
            &empty_order_book_overlays,
            &empty_resting_order_mandates,
            &empty_collateral_overlays,
            &empty_committed_events,
        ));
    body.items
        .splice(0..0, oracle_prices.into_iter().map(BlockItem::OraclePrice));
    assert!(matches!(
        body.items.as_slice(),
        [
            BlockItem::OraclePrice(_),
            BlockItem::Funding(BlockFunding {
                market_id: 7,
                funding_index: 750,
            }),
            BlockItem::Liquidation(BlockLiquidation {
                market_id: 7,
                account_id: 22,
                requested_lots: Some(1),
                filled_lots: 1,
                remaining_lots: 0,
                keeper_reward_micro_usdc: 0,
                ..
            })
        ]
    ));

    let parent = proposer.head().clone();
    let round = proposer
        .pacemaker
        .current_round
        .max(parent.header.round.saturating_add(1));
    let high_qc = proposer.pacemaker.highest_qc.clone();
    let prepared_block = proposer
        .executor
        .prepare_block_with_checkpoint(
            &parent,
            round,
            validator(1),
            high_qc.clone(),
            body.clone(),
            1_100,
        )
        .expect("prepared block should build");
    let prepared = PreparedCommittedBlock {
        prepared_block: PreparedBlock {
            block: Block {
                header: BlockHeader {
                    chain_id: parent.header.chain_id,
                    height: parent.header.height.saturating_add(1),
                    round,
                    proposer: validator(1),
                    parent_hash: block_hash(&parent),
                    parent_state_root: parent.header.state_root,
                    tx_root: body_root(&prepared_block.block.body),
                    event_root: prepared_block.block.header.event_root,
                    state_root: prepared_block.block.header.state_root,
                    qc: high_qc,
                    timestamp_ms: 1_100,
                },
                body: prepared_block.block.body.clone(),
            },
            result: prepared_block.result,
            checkpoint: prepared_block.checkpoint,
        },
        policy_store: proposer.policy_store.clone(),
        order_book_overlays: BTreeMap::new(),
        resting_order_mandate_overlays: BTreeMap::new(),
        collateral_overlays: BTreeMap::new(),
        rejected_intents: Vec::new(),
        committed_events: Vec::new(),
    };
    let block = prepared.block().clone();
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");

    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared block should apply with block oracle prices");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external block should apply with block oracle prices");

    assert_eq!(
        follower.market_funding_index(7),
        proposer.market_funding_index(7)
    );
    assert_eq!(follower.position(7, 22), proposer.position(7, 22));
    assert_eq!(follower.position(7, 33), proposer.position(7, 33));
    assert!(!follower.margin_snapshot(7, 22).under_maintenance);
}

#[test]
fn external_block_applies_consensus_faucet_item() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    let address = [0x42; 20];
    assert_eq!(
        proposer
            .submit_faucet_grant(address, U256::from(1_000_u64), U256::from(250_u64), 1_050)
            .expect("faucet grant should queue"),
        FaucetGrantSubmissionStatus::Accepted
    );
    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_100,
            10,
            1_000,
        )
        .expect("network faucet block should prepare");
    let block = prepared.block().clone();
    assert!(matches!(
        block.body.items.as_slice(),
        [BlockItem::Faucet(BlockFaucet { address: item_address, .. })]
            if *item_address == address
    ));
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared faucet block should apply locally");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external faucet block should apply");

    assert_eq!(
        proposer.get_committed_state(&policy_usdc_balance_state_key(address)),
        Some(U256::from(1_000_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        follower.get_committed_state(&policy_usdc_balance_state_key(address)),
        proposer.get_committed_state(&policy_usdc_balance_state_key(address))
    );
    assert_eq!(
        follower.get_committed_state(&evm_usdc_balance_state_key(address)),
        Some(U256::from(250_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        follower.get_committed_state(&evm_usdc_total_supply_state_key()),
        Some(U256::from(250_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        follower.committed_state_root(),
        proposer.committed_state_root()
    );
    assert_eq!(proposer.pending_faucet_grant_count(), 0);
}

#[test]
fn external_block_applies_delegation_items() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    for engine in [&mut proposer, &mut follower] {
        seed_account_policy_usdc(engine, 22, AGENT_BOND_MIN as u64);
        seed_account_policy_usdc(engine, 11, 250_000_000);
    }
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    let proposer_mandate = proposer
        .create_mandate(sample_mandate_input(22, 1), 1_010)
        .expect("proposer mandate should create");
    let follower_mandate = follower
        .create_mandate(sample_mandate_input(22, 1), 1_010)
        .expect("follower mandate should create");
    assert_eq!(proposer_mandate.id, follower_mandate.id);

    let timestamp_ms = 1_100;
    let delegation_id = derive_delegation_id(22, &proposer_mandate.id, timestamp_ms);
    let apply_external_body = |proposer: &mut SingleNodeEngine,
                               follower: &mut SingleNodeEngine,
                               body: BlockBody,
                               timestamp_ms: u64| {
        let parent = proposer.head().clone();
        let round = parent.header.round.saturating_add(1);
        let high_qc =
            build_quorum_certificate(&parent, &signing_keys).expect("parent qc should build");
        let mut prepared_block = proposer
            .executor
            .clone()
            .prepare_block_with_checkpoint(
                &parent,
                round,
                validator(1),
                high_qc.clone(),
                body,
                timestamp_ms,
            )
            .expect("delegation marker block should prepare");
        let execution_block = prepared_block.block.clone();
        let block = Block {
            header: BlockHeader {
                chain_id: parent.header.chain_id,
                height: parent.header.height.saturating_add(1),
                round,
                proposer: validator(1),
                parent_hash: block_hash(&parent),
                parent_state_root: parent.header.state_root,
                tx_root: body_root(&execution_block.body),
                event_root: execution_block.header.event_root,
                state_root: execution_block.header.state_root,
                qc: high_qc,
                timestamp_ms,
            },
            body: execution_block.body,
        };
        prepared_block.block = block.clone();
        let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
        let prepared = PreparedCommittedBlock {
            prepared_block,
            policy_store: proposer.policy_store.clone(),
            order_book_overlays: proposer.order_books.clone(),
            resting_order_mandate_overlays: proposer.resting_order_mandates.clone(),
            collateral_overlays: proposer.isolated_collateral.clone(),
            rejected_intents: Vec::new(),
            committed_events: Vec::new(),
        };

        proposer
            .apply_prepared_block(
                prepared,
                qc.clone(),
                &validator_verifiers,
                validator(1),
                1_000,
            )
            .expect("prepared delegation block should apply locally");
        follower
            .apply_external_block(block.clone(), qc, &validator_verifiers, validator(1), 1_000)
            .expect("external delegation block should apply");
        block
    };

    apply_external_body(
        &mut proposer,
        &mut follower,
        BlockBody {
            items: vec![BlockItem::Delegation(BlockDelegation {
                action_id: [1_u8; 32],
                timestamp_ms,
                action: BlockDelegationAction::Open {
                    agent: 22,
                    mandate_id: proposer_mandate.id,
                    performance_share_units: 1_000,
                    crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                    notice_period_ms: NOTICE_PERIOD_MIN_MS,
                    agent_bond: AGENT_BOND_MIN,
                },
            })],
        },
        timestamp_ms,
    );
    let proposer_delegation = proposer
        .delegation(&delegation_id)
        .cloned()
        .expect("proposer delegation should open");
    let follower_delegation = follower
        .delegation(&delegation_id)
        .cloned()
        .expect("follower delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut proposer, &proposer_delegation);
    list_delegation_for_tier0_capped_pilot(&mut follower, &follower_delegation);
    apply_external_body(
        &mut proposer,
        &mut follower,
        BlockBody {
            items: vec![BlockItem::Delegation(BlockDelegation {
                action_id: [2_u8; 32],
                timestamp_ms: timestamp_ms + 1,
                action: BlockDelegationAction::Deposit {
                    principal: 11,
                    delegation_id,
                    amount: 150_000_000,
                },
            })],
        },
        timestamp_ms + 1,
    );

    assert_eq!(
        proposer.delegation(&delegation_id),
        follower.delegation(&delegation_id)
    );
    assert_eq!(
        proposer.delegation_share(&delegation_id, 11),
        follower.delegation_share(&delegation_id, 11)
    );
    assert_eq!(
        proposer
            .delegation_free_usdc_balance(&delegation_id)
            .expect("proposer delegation balance should read"),
        150_000_000
    );
    assert_eq!(
        follower
            .delegation_free_usdc_balance(&delegation_id)
            .expect("follower delegation balance should read"),
        150_000_000
    );
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );
    assert!(follower.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationOpened {
            delegation_id: event_delegation_id,
            agent: 22,
            ..
        } if event_delegation_id == delegation_id && event.block_height == Some(1)
    )));
    assert!(follower.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::DelegationDeposited {
            delegation_id: event_delegation_id,
            principal: 11,
            amount: 150_000_000,
            ..
        } if event_delegation_id == delegation_id && event.block_height == Some(2)
    )));

    apply_external_body(
        &mut proposer,
        &mut follower,
        BlockBody {
            items: vec![BlockItem::Delegation(BlockDelegation {
                action_id: [3_u8; 32],
                timestamp_ms: timestamp_ms + 2,
                action: BlockDelegationAction::Terminate {
                    delegation_id,
                    reason: BlockDelegationFaultReason::RiskLimitBreach,
                    slash_policy: None,
                },
            })],
        },
        timestamp_ms + 2,
    );
    for engine in [&proposer, &follower] {
        assert_eq!(
            engine.delegation(&delegation_id).map(|d| d.status),
            Some(DelegationStatus::Terminated)
        );
        assert_eq!(
            engine
                .real_capital_listing(&super::vault_id_for_delegation(&delegation_id))
                .map(|listing| listing.deposit_enabled),
            Some(false)
        );
        assert_eq!(
            engine.delegation_compensation_claim_micro_usdc(&delegation_id, 11),
            AGENT_BOND_MIN
        );
        assert_eq!(engine.real_capital_insurance_fund_micro_usdc(), 0);
        assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
        assert_eq!(
            engine.real_capital_compensation_escrow_micro_usdc(),
            AGENT_BOND_MIN
        );
        assert!(matches!(
            engine.processed_delegation_action([3_u8; 32]),
            Some(ProcessedDelegationAction {
                receipt:
                    DelegationActionReceipt::Terminate {
                        delegation_id: receipt_delegation_id,
                        reason: BlockDelegationFaultReason::RiskLimitBreach,
                        bond_returned: 0,
                        bond_slashed: AGENT_BOND_MIN,
                        principal_compensation: AGENT_BOND_MIN,
                        insurance_fund_delta: 0,
                    },
                ..
            }) if *receipt_delegation_id == delegation_id
        ));
    }
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );

    let redemption_requested_at_ms = timestamp_ms + 3;
    let redemption_request_id =
        derive_redemption_request_id(11, &delegation_id, redemption_requested_at_ms);
    apply_external_body(
        &mut proposer,
        &mut follower,
        BlockBody {
            items: vec![BlockItem::Delegation(BlockDelegation {
                action_id: [4_u8; 32],
                timestamp_ms: redemption_requested_at_ms,
                action: BlockDelegationAction::RequestRedemption {
                    principal: 11,
                    delegation_id,
                    shares: 60_000_000,
                },
            })],
        },
        redemption_requested_at_ms,
    );
    let ready_at_ms = proposer
        .redemption_request(&redemption_request_id)
        .expect("proposer redemption request should exist")
        .ready_at_ms;
    assert_eq!(
        follower.redemption_request(&redemption_request_id),
        proposer.redemption_request(&redemption_request_id)
    );
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );

    apply_external_body(
        &mut proposer,
        &mut follower,
        BlockBody {
            items: vec![BlockItem::Delegation(BlockDelegation {
                action_id: [5_u8; 32],
                timestamp_ms: ready_at_ms,
                action: BlockDelegationAction::FinalizeRedemption {
                    request_id: redemption_request_id,
                },
            })],
        },
        ready_at_ms,
    );
    for engine in [&proposer, &follower] {
        assert_eq!(
            engine.delegation_compensation_claim_micro_usdc(&delegation_id, 11),
            3_000_000_000
        );
        assert!(matches!(
            engine.processed_delegation_action([5_u8; 32]),
            Some(ProcessedDelegationAction {
                receipt:
                    DelegationActionReceipt::FinalizeRedemption {
                        request_id,
                        usdc_out: 60_000_000,
                        compensation_out: 2_000_000_000,
                    },
                ..
            }) if *request_id == redemption_request_id
        ));
    }
    let mut principal_address = [0_u8; 20];
    principal_address[12..].copy_from_slice(&11_u64.to_be_bytes());
    for engine in [&proposer, &follower] {
        assert_eq!(
            engine.get_committed_state(&policy_usdc_balance_state_key(principal_address)),
            Some(U256::from(2_160_000_000_u128).to_be_bytes::<32>().to_vec())
        );
    }
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );
}

#[test]
fn external_block_replays_winding_grace_sweep() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    let proposer_delegation = sample_open_delegation(&mut proposer, 22, 1_000);
    let follower_delegation = sample_open_delegation(&mut follower, 22, 1_000);
    assert_eq!(
        proposer_delegation.delegation_id,
        follower_delegation.delegation_id
    );
    let delegation_id = proposer_delegation.delegation_id;
    let identity = MarketMakerIdentity::Delegation {
        delegation_id,
        serving_agent: 22,
    };
    for engine in [&mut proposer, &mut follower] {
        list_test_perpetual_market(engine, 7);
        engine
            .create_account(22, signing_key(22).verifying_key().to_bytes(), [0; 32], 990)
            .expect("serving agent account should create");
        seed_account_policy_usdc(engine, 22, PROGRAM_BOND_MIN_MICRO_USDC as u64);
        let program = engine
            .define_market_maker_program(sample_market_maker_program_input([7]), 1_050)
            .expect("program should define");
        assert_eq!(
            program.program_id,
            derive_market_maker_program_id(&sample_market_maker_program_input([7]))
        );
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: identity,
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_060,
                None,
            )
            .expect("delegation-backed MM should enroll before winding");
        seed_account_policy_usdc(engine, 11, 250_000_000);
        engine
            .deposit_to_delegation(11, delegation_id, 150_000_000, 1_100)
            .expect("deposit should leave residual capital during winding");
        engine
            .begin_delegation_winding(delegation_id, 1_200)
            .expect("delegation should begin winding");
    }
    assert_eq!(
        proposer.delegation(&delegation_id),
        follower.delegation(&delegation_id)
    );
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );

    let timestamp_ms = 1_200 + WINDING_GRACE_WINDOW_MS;
    let parent = proposer.head().clone();
    let round = parent.header.round.saturating_add(1);
    let high_qc = build_quorum_certificate(&parent, &signing_keys).expect("parent qc should build");
    let body = BlockBody { items: Vec::new() };
    let mut prepared_block = proposer
        .executor
        .clone()
        .prepare_block_with_checkpoint(
            &parent,
            round,
            validator(1),
            high_qc.clone(),
            body,
            timestamp_ms,
        )
        .expect("winding-grace marker block should prepare");
    let execution_block = prepared_block.block.clone();
    let block = Block {
        header: BlockHeader {
            chain_id: parent.header.chain_id,
            height: parent.header.height.saturating_add(1),
            round,
            proposer: validator(1),
            parent_hash: block_hash(&parent),
            parent_state_root: parent.header.state_root,
            tx_root: body_root(&execution_block.body),
            event_root: execution_block.header.event_root,
            state_root: execution_block.header.state_root,
            qc: high_qc,
            timestamp_ms,
        },
        body: execution_block.body,
    };
    prepared_block.block = block.clone();
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let prepared = PreparedCommittedBlock {
        prepared_block,
        policy_store: proposer.policy_store.clone(),
        order_book_overlays: proposer.order_books.clone(),
        resting_order_mandate_overlays: proposer.resting_order_mandates.clone(),
        collateral_overlays: proposer.isolated_collateral.clone(),
        rejected_intents: Vec::new(),
        committed_events: Vec::new(),
    };

    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared winding-grace block should apply locally");
    follower
        .apply_external_block(block.clone(), qc, &validator_verifiers, validator(1), 1_000)
        .expect("external winding-grace block should apply");

    for engine in [&proposer, &follower] {
        assert_eq!(
            engine.delegation(&delegation_id).map(|d| d.status),
            Some(DelegationStatus::Terminated)
        );
        assert_eq!(
            engine
                .market_maker_enrollment(&(
                    identity,
                    7,
                    derive_market_maker_program_id(&sample_market_maker_program_input([7]))
                ))
                .expect("delegation-backed MM enrollment should persist")
                .status,
            MarketMakerEnrollmentStatus::Exited
        );
        let agent_address = engine
            .get_committed_state(&account_evm_address_state_key(22))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .expect("serving agent address should exist");
        assert_eq!(
            policy_usdc_balance_for_test(engine, agent_address),
            PROGRAM_BOND_MIN_MICRO_USDC
        );
        assert_eq!(
            engine.delegation_compensation_claim_micro_usdc(&delegation_id, 11),
            AGENT_BOND_MIN / 2
        );
        assert_eq!(
            engine.real_capital_insurance_fund_micro_usdc(),
            AGENT_BOND_MIN / 2
        );
        assert_eq!(
            engine.real_capital_insurance_wallet_micro_usdc(),
            AGENT_BOND_MIN / 2
        );
        assert_eq!(
            engine.real_capital_compensation_escrow_micro_usdc(),
            AGENT_BOND_MIN / 2
        );
        assert!(engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::DelegationStatusChanged {
                delegation_id: event_delegation_id,
                reason: DelegationStatusReason::WindingGraceFailure,
                ..
            } if event_delegation_id == &delegation_id
                && event.block_height == Some(block.header.height)
        )));
        assert!(engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::MarketMakerEnrollmentExited {
                market_maker_identity,
                market_id: 7,
                bond_refunded_micro_usdc,
                ..
            } if event.block_height == Some(block.header.height)
                && *market_maker_identity == identity
                && *bond_refunded_micro_usdc == PROGRAM_BOND_MIN_MICRO_USDC
        )));
    }
    assert_eq!(
        proposer.delegation(&delegation_id),
        follower.delegation(&delegation_id)
    );
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );
}

#[test]
fn external_block_records_failed_delegation_item() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    let invalid_delegation_id = [9_u8; 32];
    let body = BlockBody {
        items: vec![BlockItem::Delegation(BlockDelegation {
            action_id: [3_u8; 32],
            timestamp_ms: 1_100,
            action: BlockDelegationAction::Deposit {
                principal: 11,
                delegation_id: invalid_delegation_id,
                amount: 150_000_000,
            },
        })],
    };
    let parent = proposer.head().clone();
    let round = parent.header.round.saturating_add(1);
    let high_qc = proposer.pacemaker.highest_qc.clone();
    let mut prepared_block = proposer
        .executor
        .clone()
        .prepare_block_with_checkpoint(&parent, round, validator(1), high_qc.clone(), body, 1_100)
        .expect("failed delegation marker block should prepare");
    let execution_block = prepared_block.block.clone();
    let block = Block {
        header: BlockHeader {
            chain_id: parent.header.chain_id,
            height: parent.header.height.saturating_add(1),
            round,
            proposer: validator(1),
            parent_hash: block_hash(&parent),
            parent_state_root: parent.header.state_root,
            tx_root: body_root(&execution_block.body),
            event_root: execution_block.header.event_root,
            state_root: execution_block.header.state_root,
            qc: high_qc,
            timestamp_ms: 1_100,
        },
        body: execution_block.body,
    };
    prepared_block.block = block.clone();
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let prepared = PreparedCommittedBlock {
        prepared_block,
        policy_store: proposer.policy_store.clone(),
        order_book_overlays: proposer.order_books.clone(),
        resting_order_mandate_overlays: proposer.resting_order_mandates.clone(),
        collateral_overlays: proposer.isolated_collateral.clone(),
        rejected_intents: Vec::new(),
        committed_events: Vec::new(),
    };

    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared failed delegation block should apply locally");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external failed delegation block should apply");

    assert!(proposer.delegation(&invalid_delegation_id).is_none());
    assert!(follower.delegation(&invalid_delegation_id).is_none());
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );
    for engine in [&proposer, &follower] {
        assert!(engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::DelegationActionRejected {
                action_kind,
                reason,
            } if action_kind == "deposit"
                && reason == "unknown_delegation"
                && event.block_height == Some(1)
        )));
    }
}

#[test]
fn external_block_applies_market_maker_program_items() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    for engine in [&mut proposer, &mut follower] {
        engine
            .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_050)
            .expect("market should seed");
        engine
            .create_account(31, [31; 32], [9; 32], 1_055)
            .expect("market maker account should seed");
        seed_account_policy_usdc(engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    }
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    let input = sample_market_maker_program_input([7]);
    let program_id = derive_market_maker_program_id(&input);
    let define_action = sample_market_maker_program_define_action([7]);
    let enroll_action = BlockMarketMakerProgramAction::Enroll {
        market_maker_identity: MarketMakerIdentity::Account(31),
        market_id: 7,
        program_id,
        bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC).0,
    };
    let exit_action = BlockMarketMakerProgramAction::Exit {
        market_maker_identity: MarketMakerIdentity::Account(31),
        market_id: 7,
        program_id,
    };
    let retire_action = BlockMarketMakerProgramAction::Retire { program_id };
    let body = BlockBody {
        items: vec![
            BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
                action_id: market_maker_program_action_id_for_submission(
                    &define_action,
                    1_100,
                    None,
                ),
                timestamp_ms: 1_100,
                action: define_action,
            }),
            BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
                action_id: market_maker_program_action_id_for_submission(
                    &enroll_action,
                    1_101,
                    None,
                ),
                timestamp_ms: 1_101,
                action: enroll_action,
            }),
            BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
                action_id: market_maker_program_action_id_for_submission(&exit_action, 1_102, None),
                timestamp_ms: 1_102,
                action: exit_action,
            }),
            BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
                action_id: market_maker_program_action_id_for_submission(
                    &retire_action,
                    1_103,
                    None,
                ),
                timestamp_ms: 1_103,
                action: retire_action,
            }),
        ],
    };
    let parent = proposer.head().clone();
    let round = parent.header.round.saturating_add(1);
    let high_qc = proposer.pacemaker.highest_qc.clone();
    let mut prepared_block = proposer
        .executor
        .clone()
        .prepare_block_with_checkpoint(&parent, round, validator(1), high_qc.clone(), body, 1_100)
        .expect("market-maker marker block should prepare");
    let execution_block = prepared_block.block.clone();
    let block = Block {
        header: BlockHeader {
            chain_id: parent.header.chain_id,
            height: parent.header.height.saturating_add(1),
            round,
            proposer: validator(1),
            parent_hash: block_hash(&parent),
            parent_state_root: parent.header.state_root,
            tx_root: body_root(&execution_block.body),
            event_root: execution_block.header.event_root,
            state_root: execution_block.header.state_root,
            qc: high_qc,
            timestamp_ms: 1_100,
        },
        body: execution_block.body,
    };
    prepared_block.block = block.clone();
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let prepared = PreparedCommittedBlock {
        prepared_block,
        policy_store: proposer.policy_store.clone(),
        order_book_overlays: proposer.order_books.clone(),
        resting_order_mandate_overlays: proposer.resting_order_mandates.clone(),
        collateral_overlays: proposer.isolated_collateral.clone(),
        rejected_intents: Vec::new(),
        committed_events: Vec::new(),
    };

    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared market-maker block should apply locally");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external market-maker block should apply");

    assert_eq!(
        proposer.market_maker_program(&program_id),
        follower.market_maker_program(&program_id)
    );
    assert_eq!(
        follower
            .market_maker_program(&program_id)
            .expect("program should exist")
            .status,
        MarketMakerProgramStatus::Retired
    );
    assert_eq!(
        follower
            .market_maker_enrollment(&(MarketMakerIdentity::Account(31), 7, program_id))
            .expect("enrollment should exist")
            .status,
        MarketMakerEnrollmentStatus::Exited
    );
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );
    assert!(follower.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerProgramDefined {
            program_id: event_program_id,
            reactivated: false,
            ..
        } if event_program_id == &program_id && event.block_height == Some(1)
    )));
    assert!(follower.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerProgramRetired { program_id: event_program_id }
            if event_program_id == program_id && event.block_height == Some(1)
    )));
    assert!(follower.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerEnrolled {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id: event_program_id,
            ..
        } if event_program_id == program_id && event.block_height == Some(1)
    )));
    assert!(follower.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerEnrollmentExited {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id: event_program_id,
            ..
        } if event_program_id == program_id && event.block_height == Some(1)
    )));
}

#[test]
fn external_block_records_failed_market_maker_program_item() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    let action = sample_market_maker_program_define_action([404]);
    let action_id = market_maker_program_action_id_for_submission(&action, 1_100, None);
    let body = BlockBody {
        items: vec![BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
            action_id,
            timestamp_ms: 1_100,
            action,
        })],
    };
    let parent = proposer.head().clone();
    let round = parent.header.round.saturating_add(1);
    let high_qc = proposer.pacemaker.highest_qc.clone();
    let mut prepared_block = proposer
        .executor
        .clone()
        .prepare_block_with_checkpoint(&parent, round, validator(1), high_qc.clone(), body, 1_100)
        .expect("failed market-maker marker block should prepare");
    let execution_block = prepared_block.block.clone();
    let block = Block {
        header: BlockHeader {
            chain_id: parent.header.chain_id,
            height: parent.header.height.saturating_add(1),
            round,
            proposer: validator(1),
            parent_hash: block_hash(&parent),
            parent_state_root: parent.header.state_root,
            tx_root: body_root(&execution_block.body),
            event_root: execution_block.header.event_root,
            state_root: execution_block.header.state_root,
            qc: high_qc,
            timestamp_ms: 1_100,
        },
        body: execution_block.body,
    };
    prepared_block.block = block.clone();
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let prepared = PreparedCommittedBlock {
        prepared_block,
        policy_store: proposer.policy_store.clone(),
        order_book_overlays: proposer.order_books.clone(),
        resting_order_mandate_overlays: proposer.resting_order_mandates.clone(),
        collateral_overlays: proposer.isolated_collateral.clone(),
        rejected_intents: Vec::new(),
        committed_events: Vec::new(),
    };

    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared failed market-maker block should apply locally");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external failed market-maker block should apply");

    assert_eq!(proposer.market_maker_programs().count(), 0);
    assert_eq!(follower.market_maker_programs().count(), 0);
    assert_eq!(
        proposer.head().header.state_root,
        follower.head().header.state_root
    );
    for engine in [&proposer, &follower] {
        assert!(engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::MarketMakerProgramActionRejected {
                action_kind,
                reason,
            } if action_kind == "define"
                && reason == "unknown_market"
                && event.block_height == Some(1)
        )));
        assert!(matches!(
            engine.processed_market_maker_program_action(action_id),
            Some(ProcessedMarketMakerProgramAction {
                receipt: MarketMakerProgramActionReceipt::Rejected {
                    action_kind,
                    reason,
                },
                ..
            }) if action_kind == "define" && reason == "unknown_market"
        ));
    }
}

#[test]
fn delegation_action_receipts_prune_after_retention_window() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let action_id = [7_u8; 32];
    engine
        .commit_delegation_action_block(
            action_id,
            BlockDelegationAction::Deposit {
                principal: 11,
                delegation_id: [9_u8; 32],
                amount: 150_000_000,
            },
            1_100,
        )
        .expect("failed delegation action block should still commit");

    assert!(engine.processed_delegation_actions.contains_key(&action_id));

    engine
        .produce_empty_block(1_100 + DELEGATION_ACTION_RECEIPT_RETENTION_MS + 1)
        .expect("empty block should prune old delegation action receipts");

    assert!(!engine.processed_delegation_actions.contains_key(&action_id));
}

#[test]
fn pending_delegation_actions_are_drained_into_produced_block() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    let open_timestamp_ms = 1_100;
    let deposit_timestamp_ms = 1_101;
    let delegation_id = derive_delegation_id(22, &mandate.id, open_timestamp_ms);

    assert_eq!(
        engine
            .submit_delegation_action(
                [1_u8; 32],
                BlockDelegationAction::Open {
                    agent: 22,
                    mandate_id: mandate.id,
                    performance_share_units: 1_000,
                    crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                    notice_period_ms: NOTICE_PERIOD_MIN_MS,
                    agent_bond: AGENT_BOND_MIN,
                },
                open_timestamp_ms,
            )
            .expect("open action should queue"),
        DelegationActionSubmissionStatus::Accepted
    );
    assert_eq!(
        engine
            .submit_delegation_action(
                [1_u8; 32],
                BlockDelegationAction::Open {
                    agent: 22,
                    mandate_id: mandate.id,
                    performance_share_units: 1_000,
                    crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                    notice_period_ms: NOTICE_PERIOD_MIN_MS,
                    agent_bond: AGENT_BOND_MIN,
                },
                open_timestamp_ms,
            )
            .expect("duplicate pending action should be accepted"),
        DelegationActionSubmissionStatus::DuplicatePending
    );
    let err = engine
        .submit_delegation_action(
            [1_u8; 32],
            BlockDelegationAction::Open {
                agent: 22,
                mandate_id: mandate.id,
                performance_share_units: 2_000,
                crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                notice_period_ms: NOTICE_PERIOD_MIN_MS,
                agent_bond: AGENT_BOND_MIN,
            },
            open_timestamp_ms,
        )
        .expect_err("same pending action id with different body should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_action_id_mismatch",
            ..
        }
    ));
    let err = engine
        .submit_delegation_action(
            [2_u8; 32],
            BlockDelegationAction::Deposit {
                principal: 11,
                delegation_id,
                amount: 150_000_000,
            },
            deposit_timestamp_ms,
        )
        .expect_err("pending-open deposit should reject until the vault is listed");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "vault_not_listed",
            ..
        }
    ));
    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block should include queued open action")
        .clone();

    assert_eq!(engine.pending_delegation_action_count(), 0);
    assert!(matches!(
        block.body.items.as_slice(),
        [BlockItem::Delegation(BlockDelegation {
            action_id: [
                1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
                1, 1, 1, 1
            ],
            timestamp_ms: 1_100,
            ..
        })]
    ));
    assert_eq!(
        engine
            .delegation(&delegation_id)
            .expect("delegation should open")
            .total_shares,
        0
    );
    assert!(matches!(
        engine.processed_delegation_action([1_u8; 32]),
        Some(ProcessedDelegationAction {
            receipt: DelegationActionReceipt::Open {
                delegation_id: processed_delegation_id,
            },
            ..
        }) if *processed_delegation_id == delegation_id
    ));
    let delegation = engine
        .delegation(&delegation_id)
        .cloned()
        .expect("delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut engine, &delegation);
    engine
        .submit_delegation_action(
            [2_u8; 32],
            BlockDelegationAction::Deposit {
                principal: 11,
                delegation_id,
                amount: 150_000_000,
            },
            deposit_timestamp_ms,
        )
        .expect("deposit action should queue after listing");

    let deposit_block = engine
        .produce_block_from_mempool(1_300, 10)
        .expect("block should include queued deposit action")
        .clone();
    assert!(matches!(
        deposit_block.body.items.as_slice(),
        [BlockItem::Delegation(BlockDelegation {
            action_id: [
                2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
                2, 2, 2, 2
            ],
            timestamp_ms: 1_101,
            ..
        })]
    ));
    assert_eq!(
        engine.delegation_share(&delegation_id, 11).shares,
        150_000_000
    );
    assert!(matches!(
        engine.processed_delegation_action([2_u8; 32]),
        Some(ProcessedDelegationAction {
            receipt: DelegationActionReceipt::Deposit {
                delegation_id: processed_delegation_id,
                principal: 11,
                shares_minted: 150_000_000,
            },
            ..
        }) if *processed_delegation_id == delegation_id
    ));
    assert!(engine
        .processed_delegation_actions
        .contains_key(&[1_u8; 32]));
    assert!(engine
        .processed_delegation_actions
        .contains_key(&[2_u8; 32]));
}

#[test]
fn signed_delegation_action_queues_with_account_owner_signature() {
    let signer = signing_key(22);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(22, signer.verifying_key().to_bytes(), [9; 32], 900)
        .expect("agent account should create");
    let mandate = create_sample_mandate(&mut engine, 22, 1, 910);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let timestamp_ms = 1_100;
    let action = BlockDelegationAction::Open {
        agent: 22,
        mandate_id: mandate.id,
        performance_share_units: 1_000,
        crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
        notice_period_ms: NOTICE_PERIOD_MIN_MS,
        agent_bond: AGENT_BOND_MIN,
    };
    let signed = signed_delegation_action(&signer, 7, 22, action, timestamp_ms, Some(9));
    let action_id = signed.action_id;

    assert_eq!(
        engine
            .submit_signed_delegation_action(signed.clone())
            .expect("signed action should queue"),
        DelegationActionSubmissionStatus::Accepted
    );
    assert_eq!(
        engine
            .submit_signed_delegation_action(signed)
            .expect("signed duplicate should be idempotent"),
        DelegationActionSubmissionStatus::DuplicatePending
    );

    engine
        .produce_block_from_mempool(1_200, 1)
        .expect("queued signed action should commit");
    let delegation_id = derive_delegation_id(22, &mandate.id, timestamp_ms);
    assert!(matches!(
        engine.processed_delegation_action(action_id),
        Some(ProcessedDelegationAction {
            receipt: DelegationActionReceipt::Open {
                delegation_id: processed_delegation_id,
            },
            ..
        }) if *processed_delegation_id == delegation_id
    ));
}

#[test]
fn signed_delegation_action_rejects_bad_signature_and_signer_scope() {
    let signer = signing_key(22);
    let wrong_signer = signing_key(23);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(22, signer.verifying_key().to_bytes(), [9; 32], 900)
        .expect("agent account should create");
    engine
        .create_account(23, wrong_signer.verifying_key().to_bytes(), [9; 32], 900)
        .expect("wrong account should create");
    let mandate = create_sample_mandate(&mut engine, 22, 1, 910);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let action = BlockDelegationAction::Open {
        agent: 22,
        mandate_id: mandate.id,
        performance_share_units: 1_000,
        crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
        notice_period_ms: NOTICE_PERIOD_MIN_MS,
        agent_bond: AGENT_BOND_MIN,
    };

    let wrong_scope =
        signed_delegation_action(&wrong_signer, 7, 23, action.clone(), 1_100, Some(1));
    let err = engine
        .submit_signed_delegation_action(wrong_scope)
        .expect_err("wrong signer account should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_action_signer_mismatch",
            ..
        }
    ));

    let mut bad_signature =
        signed_delegation_action(&signer, 7, 22, action.clone(), 1_100, Some(2));
    bad_signature.signature[0] ^= 0xff;
    let err = engine
        .submit_signed_delegation_action(bad_signature)
        .expect_err("bad signature should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "invalid_delegation_action_signature",
            ..
        }
    ));

    let wrong_chain = signed_delegation_action(&signer, 8, 22, action, 1_100, Some(3));
    let err = engine
        .submit_signed_delegation_action(wrong_chain)
        .expect_err("wrong chain should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "chain_mismatch",
            ..
        }
    ));
    assert_eq!(engine.pending_delegation_action_count(), 0);
}

#[test]
fn signed_delegation_terminate_restricts_external_fault_reasons() {
    let signer = signing_key(22);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(22, signer.verifying_key().to_bytes(), [9; 32], 900)
        .expect("agent account should create");
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);

    let external_reason = signed_delegation_action(
        &signer,
        7,
        22,
        BlockDelegationAction::Terminate {
            delegation_id: delegation.delegation_id,
            reason: BlockDelegationFaultReason::GovernanceFault,
            slash_policy: Some(BlockDelegationSlashPolicy::PrincipalProRata),
        },
        1_100,
        Some(1),
    );
    let err = engine
        .submit_signed_delegation_action(external_reason)
        .expect_err("agent-signed external fault reason should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "delegation_terminate_requires_operator_submit",
            ..
        }
    ));
    assert_eq!(engine.pending_delegation_action_count(), 0);

    let self_admitted = signed_delegation_action(
        &signer,
        7,
        22,
        BlockDelegationAction::Terminate {
            delegation_id: delegation.delegation_id,
            reason: BlockDelegationFaultReason::MandateBreached,
            slash_policy: None,
        },
        1_100,
        Some(2),
    );
    assert_eq!(
        engine
            .submit_signed_delegation_action(self_admitted)
            .expect("agent self-admitted mandate breach should queue"),
        DelegationActionSubmissionStatus::Accepted
    );
}

#[test]
fn signed_delegation_action_enforces_per_signer_pending_cap() {
    let signer = signing_key(11);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(11, signer.verifying_key().to_bytes(), [9; 32], 900)
        .expect("principal account should create");
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(
        &mut engine,
        11,
        (DELEGATION_MIN_DEPOSIT * (MAX_PENDING_DELEGATION_ACTIONS_PER_SIGNER as u128 + 1)) as u64,
    );
    let action = BlockDelegationAction::Deposit {
        principal: 11,
        delegation_id: delegation.delegation_id,
        amount: DELEGATION_MIN_DEPOSIT,
    };

    for nonce in 0..MAX_PENDING_DELEGATION_ACTIONS_PER_SIGNER {
        let signed =
            signed_delegation_action(&signer, 7, 11, action.clone(), 1_100, Some(nonce as u64));
        assert_eq!(
            engine
                .submit_signed_delegation_action(signed)
                .expect("signed deposit should queue below signer cap"),
            DelegationActionSubmissionStatus::Accepted
        );
    }

    let duplicate = signed_delegation_action(&signer, 7, 11, action.clone(), 1_100, Some(0));
    assert_eq!(
        engine
            .submit_signed_delegation_action(duplicate)
            .expect("duplicate should bypass signer cap"),
        DelegationActionSubmissionStatus::DuplicatePending
    );
    let overflow = signed_delegation_action(&signer, 7, 11, action, 1_100, Some(9_999));
    let err = engine
        .submit_signed_delegation_action(overflow)
        .expect_err("new action above signer cap should reject");
    assert!(matches!(
        err,
        NodeError::InvalidDelegation {
            reason: "max_pending_delegation_actions_per_signer",
            ..
        }
    ));
    assert_eq!(
        engine.pending_delegation_action_count(),
        MAX_PENDING_DELEGATION_ACTIONS_PER_SIGNER
    );
}

#[test]
fn pending_delegation_action_preflight_rejects_invalid_without_queueing() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let error = engine
        .submit_delegation_action(
            [8_u8; 32],
            BlockDelegationAction::Deposit {
                principal: 11,
                delegation_id: [9_u8; 32],
                amount: 150_000_000,
            },
            1_100,
        )
        .expect_err("unknown delegation should reject before queueing");

    assert!(matches!(
        error,
        NodeError::InvalidDelegation {
            reason: "unknown_delegation",
            ..
        }
    ));
    assert_eq!(engine.pending_delegation_action_count(), 0);

    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, (AGENT_BOND_MIN - 1) as u64);
    let error = engine
        .submit_delegation_action(
            [9_u8; 32],
            BlockDelegationAction::Open {
                agent: 22,
                mandate_id: mandate.id,
                performance_share_units: 1_000,
                crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                notice_period_ms: NOTICE_PERIOD_MIN_MS,
                agent_bond: AGENT_BOND_MIN,
            },
            1_100,
        )
        .expect_err("underfunded agent bond should reject before queueing");

    assert!(matches!(
        error,
        NodeError::InvalidDelegation {
            reason: "insufficient_agent_policy_usdc_bond",
            ..
        }
    ));
    assert_eq!(engine.pending_delegation_action_count(), 0);
}

#[test]
fn pending_delegation_actions_respect_block_item_limit_and_restore() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    let delegation_id = derive_delegation_id(22, &mandate.id, 1_100);
    engine
        .submit_delegation_action(
            [4_u8; 32],
            BlockDelegationAction::Open {
                agent: 22,
                mandate_id: mandate.id,
                performance_share_units: 1_000,
                crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                notice_period_ms: NOTICE_PERIOD_MIN_MS,
                agent_bond: AGENT_BOND_MIN,
            },
            1_100,
        )
        .expect("open action should queue");
    engine
        .produce_empty_block(1_050)
        .expect("empty block should make the snapshot head restorable");
    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore pending delegation action");
    assert_eq!(restored.pending_delegation_action_count(), 1);
    engine = restored;

    engine
        .produce_block_from_mempool(1_200, 0)
        .expect("zero-item block should not drain delegation action");
    assert_eq!(engine.pending_delegation_action_count(), 1);

    engine
        .produce_block_from_mempool(1_300, 1)
        .expect("one item block should drain open action");
    assert_eq!(engine.pending_delegation_action_count(), 0);

    let delegation = engine
        .delegation(&delegation_id)
        .cloned()
        .expect("delegation should open");
    list_delegation_for_tier0_capped_pilot(&mut engine, &delegation);
    engine
        .submit_delegation_action(
            [5_u8; 32],
            BlockDelegationAction::Deposit {
                principal: 11,
                delegation_id,
                amount: 150_000_000,
            },
            1_101,
        )
        .expect("listed delegation deposit should queue");
    engine
        .produce_block_from_mempool(1_400, 1)
        .expect("one item block should drain deposit action");
    assert_eq!(engine.pending_delegation_action_count(), 0);
    assert_eq!(
        engine.delegation_share(&delegation_id, 11).shares,
        150_000_000
    );
}

#[test]
fn pending_delegation_action_queue_rejects_when_full() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    for index in 0..=MAX_PENDING_DELEGATION_ACTIONS {
        let mut action_id = [0_u8; 32];
        action_id[24..].copy_from_slice(&(index as u64).to_be_bytes());
        engine.delegations.insert(
            action_id,
            Delegation {
                delegation_id: action_id,
                agent: 22,
                mandate_id: [7_u8; 32],
                opened_at_ms: 1_000,
                last_updated_at_ms: 1_000,
                winding_started_at_ms: None,
                status: DelegationStatus::Open,
                performance_share_units: 1_000,
                crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                notice_period_ms: NOTICE_PERIOD_MIN_MS,
                total_shares: 0,
                high_water_mark_per_share: 0,
                last_crystallized_at_ms: 1_000,
                agent_bond: 0,
            },
        );
    }
    for index in 0..MAX_PENDING_DELEGATION_ACTIONS {
        let mut action_id = [0_u8; 32];
        action_id[24..].copy_from_slice(&(index as u64).to_be_bytes());
        engine
            .submit_delegation_action(
                action_id,
                BlockDelegationAction::Close {
                    delegation_id: action_id,
                },
                1_100,
            )
            .expect("action should queue before cap");
    }

    let mut overflow_id = [0_u8; 32];
    overflow_id[24..].copy_from_slice(&(MAX_PENDING_DELEGATION_ACTIONS as u64).to_be_bytes());
    let error = engine
        .submit_delegation_action(
            overflow_id,
            BlockDelegationAction::Close {
                delegation_id: overflow_id,
            },
            1_100,
        )
        .expect_err("queue should reject once full");

    assert!(matches!(
        error,
        NodeError::InvalidDelegation {
            reason: "delegation_queue_full",
            ..
        }
    ));
}

#[test]
fn network_block_preparation_includes_pending_delegation_actions() {
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut engine =
        SingleNodeEngine::new_with_validator_set(config, Some(&configured_validator_set));
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let head_qc =
        build_quorum_certificate(engine.head(), &signing_keys).expect("head qc should build");
    engine
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    engine
        .submit_delegation_action(
            [5_u8; 32],
            BlockDelegationAction::Open {
                agent: 22,
                mandate_id: mandate.id,
                performance_share_units: 1_000,
                crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
                notice_period_ms: NOTICE_PERIOD_MIN_MS,
                agent_bond: AGENT_BOND_MIN,
            },
            1_100,
        )
        .expect("delegation action should queue");

    let prepared = engine
        .prepare_network_block_commit_from_mempool(
            validator(2),
            &validator_verifiers,
            1_200,
            1,
            1_000,
        )
        .expect("network block should prepare pending delegation action");
    assert!(matches!(
        prepared.block().body.items.as_slice(),
        [BlockItem::Delegation(BlockDelegation {
            action_id: [
                5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
                5, 5, 5, 5
            ],
            timestamp_ms: 1_100,
            ..
        })]
    ));
    assert_eq!(engine.pending_delegation_action_count(), 1);
}

#[test]
fn external_delegation_block_clears_matching_pending_action() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let action_id = [6_u8; 32];
    let proposer_mandate = create_sample_mandate(&mut proposer, 22, 1, 900);
    let follower_mandate = create_sample_mandate(&mut follower, 22, 1, 900);
    assert_eq!(proposer_mandate.id, follower_mandate.id);
    let action = BlockDelegationAction::Open {
        agent: 22,
        mandate_id: proposer_mandate.id,
        performance_share_units: 1_000,
        crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
        notice_period_ms: NOTICE_PERIOD_MIN_MS,
        agent_bond: AGENT_BOND_MIN,
    };
    for engine in [&mut proposer, &mut follower] {
        seed_account_policy_usdc(engine, 22, AGENT_BOND_MIN as u64);
        engine
            .submit_delegation_action(action_id, action.clone(), 1_100)
            .expect("action should queue");
    }
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");

    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_200,
            1,
            1_000,
        )
        .expect("network block should prepare pending action");
    let block = prepared.block().clone();
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared block should apply");

    assert_eq!(follower.pending_delegation_action_count(), 1);
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external block should apply");

    assert_eq!(follower.pending_delegation_action_count(), 0);
    assert!(follower.processed_delegation_action(action_id).is_some());
}

#[test]
fn fills_charge_market_fees_and_rebucket_fee_vault() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 100,
            maker_rebate_bps: 0,
            taker_fee_bps: 200,
        },
    );
    engine.set_isolated_collateral(7, 22, 50);
    engine.set_isolated_collateral(7, 33, 50);
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(ask, 1_000).expect("ask should rest");
    engine.submit_order(bid, 1_001).expect("bid should fill");

    assert_eq!(engine.isolated_collateral(7, 22), 47);
    assert_eq!(engine.isolated_collateral(7, 33), 44);
    assert_eq!(engine.fee_vault_balance(7), 0);
    assert_eq!(engine.validator_reward_pool_balance(7), 3);
    assert_eq!(engine.market_maker_rebate_pool_balance(7), 2);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert_eq!(engine.treasury_balance(7), 4);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FeeCharged {
            market_id,
            account_id,
            fee_amount,
            fee_vault_balance,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *fee_amount == 3
            && *fee_vault_balance == 3
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FeeCharged {
            market_id,
            account_id,
            fee_amount,
            fee_vault_balance,
            ..
        } if *market_id == 7
            && *account_id == 33
            && *fee_amount == 6
            && *fee_vault_balance == 9
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FeeRevenueDistributed {
            market_id,
            source_fee_amount,
            validator_rewards_delta,
            validator_reward_balance,
            market_maker_rebate_delta,
            market_maker_rebate_balance,
            insurance_fund_delta,
            insurance_fund_balance,
            treasury_delta,
            treasury_balance,
        } if *market_id == 7
            && *source_fee_amount == 9
            && *validator_rewards_delta == 3
            && *validator_reward_balance == 3
            && *market_maker_rebate_delta == 2
            && *market_maker_rebate_balance == 2
            && *insurance_fund_delta == 0
            && *insurance_fund_balance == 0
            && *treasury_delta == 4
            && *treasury_balance == 4
    )));
}

#[test]
fn fills_credit_baseline_maker_rebate_from_scaled_notional() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_000);
    engine.update_market_mark_price(7, 100, 1_000);
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 50,
            taker_fee_bps: 100,
        },
    );
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 33, 1_000);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        )
        .expect("taker bid should fill");

    assert_eq!(engine.isolated_collateral(7, 22), 1_005);
    assert_eq!(engine.isolated_collateral(7, 33), 990);
    assert_eq!(engine.fee_vault_balance(7), 0);
    assert_eq!(engine.validator_reward_pool_balance(7), 2);
    assert_eq!(engine.market_maker_rebate_pool_balance(7), 1);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert_eq!(engine.treasury_balance(7), 2);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FeeRevenueDistributed {
            market_id,
            source_fee_amount,
            validator_rewards_delta,
            validator_reward_balance,
            market_maker_rebate_delta,
            market_maker_rebate_balance,
            insurance_fund_delta,
            insurance_fund_balance,
            treasury_delta,
            treasury_balance,
        } if *market_id == 7
            && *source_fee_amount == 10
            && *validator_rewards_delta == 2
            && *validator_reward_balance == 2
            && *market_maker_rebate_delta == 1
            && *market_maker_rebate_balance == 1
            && *insurance_fund_delta == 0
            && *insurance_fund_balance == 0
            && *treasury_delta == 2
            && *treasury_balance == 2
    )));
}

#[test]
fn fills_credit_market_maker_bonus_to_naked_economic_account() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_000);
    engine.update_market_mark_price(7, 100, 1_000);
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 50,
            taker_fee_bps: 100,
        },
    );
    engine
        .create_account(22, [22; 32], [9; 32], 1_010)
        .expect("naked market maker account should create");
    seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_020)
        .expect("program should define");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(22),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_030,
            None,
        )
        .expect("naked enrollment should succeed");
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 33, 1_000);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_041,
        )
        .expect("taker bid should fill");

    assert_eq!(engine.isolated_collateral(7, 22), 1_010);
    assert_eq!(engine.isolated_collateral(7, 33), 990);
    assert_eq!(engine.fee_vault_balance(7), 0);
    assert_eq!(engine.market_maker_rebate_pool_balance(7), 0);
    assert_eq!(engine.validator_reward_pool_balance(7), 0);
    assert_eq!(engine.treasury_balance(7), 0);
    let enrollment = engine
        .market_maker_enrollments
        .get(&(MarketMakerIdentity::Account(22), 7, program.program_id))
        .expect("naked enrollment should exist");
    assert_eq!(
        enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
        Some(5)
    );
    assert_eq!(
        enrollment.lifetime_forfeited_rebates_micro_usdc.as_u128(),
        Some(0)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MmProgramBonusPaid {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id,
            requested_bonus_micro_usdc,
            paid_bonus_micro_usdc,
        } if *economic_account_id == 22
            && *market_id == 7
            && *program_id == program.program_id
            && *requested_bonus_micro_usdc == 5
            && *paid_bonus_micro_usdc == 5
    )));
    assert!(
        !engine
            .runtime_events()
            .iter()
            .any(|event| matches!(event.kind, RuntimeEventKind::FeeRevenueDistributed { .. })),
        "same-fill bonus should fully consume distributable revenue in the 2x case"
    );
}

#[test]
fn fills_credit_market_maker_bonus_to_delegation_trading_account() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_000);
    engine.update_market_mark_price(7, 100, 1_000);
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 50,
            taker_fee_bps: 100,
        },
    );
    engine
        .create_account(31, [31; 32], [9; 32], 1_010)
        .expect("serving agent should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_020);
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_030)
        .expect("program should define");
    let delegated_identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: delegated_identity,
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_040,
            None,
        )
        .expect("delegated enrollment should succeed");
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    engine.set_isolated_collateral(7, trading_account, 1_000);
    engine.set_isolated_collateral(7, 31, 123);
    engine.set_isolated_collateral(7, 33, 1_000);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: trading_account,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_050,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_051,
        )
        .expect("taker bid should fill");

    assert_eq!(engine.isolated_collateral(7, trading_account), 1_010);
    assert_eq!(engine.isolated_collateral(7, 31), 123);
    assert_eq!(engine.isolated_collateral(7, 33), 990);
    let enrollment = engine
        .market_maker_enrollments
        .get(&(delegated_identity, 7, program.program_id))
        .expect("delegated enrollment should exist");
    assert_eq!(
        enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
        Some(5)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MmProgramBonusPaid {
            market_maker_identity,
            economic_account_id,
            market_id,
            program_id,
            requested_bonus_micro_usdc,
            paid_bonus_micro_usdc,
        } if *market_maker_identity == delegated_identity
            && *economic_account_id == trading_account
            && *market_id == 7
            && *program_id == program.program_id
            && *requested_bonus_micro_usdc == 5
            && *paid_bonus_micro_usdc == 5
    )));
}

#[test]
fn non_active_enrollment_forfeits_market_maker_bonus() {
    for status in [
        MarketMakerEnrollmentStatus::GracePeriod,
        MarketMakerEnrollmentStatus::NonCompliant,
    ] {
        let mut engine = SingleNodeEngine::new(single_node_config());
        list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
        engine.update_market_index_price(7, 100, 1_000);
        engine.update_market_mark_price(7, 100, 1_000);
        engine.update_market_fee_parameters(
            7,
            MarketFeeParameters {
                maker_fee_bps: 0,
                maker_rebate_bps: 50,
                taker_fee_bps: 100,
            },
        );
        engine
            .create_account(22, [22; 32], [9; 32], 1_010)
            .expect("naked market maker account should create");
        seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
        let program = engine
            .define_market_maker_program(sample_market_maker_program_input([7]), 1_020)
            .expect("program should define");
        let identity = MarketMakerIdentity::Account(22);
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: identity,
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_030,
                None,
            )
            .expect("naked enrollment should succeed");
        engine
            .market_maker_enrollments
            .get_mut(&(identity, 7, program.program_id))
            .expect("naked enrollment should exist")
            .status = status;
        engine.set_isolated_collateral(7, 22, 1_000);
        engine.set_isolated_collateral(7, 33, 1_000);

        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id: 7,
                    account_id: 22,
                    client_order_id: 1,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots: 5,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_040,
            )
            .expect("maker ask should rest");
        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id: 7,
                    account_id: 33,
                    client_order_id: 2,
                    side: kombat_chain_matching::OrderSide::Bid,
                    quantity_lots: 5,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_041,
            )
            .expect("taker bid should fill");

        assert_eq!(engine.isolated_collateral(7, 22), 1_005);
        assert_eq!(engine.isolated_collateral(7, 33), 990);
        assert_eq!(engine.market_maker_rebate_pool_balance(7), 1);
        let enrollment = engine
            .market_maker_enrollments
            .get(&(identity, 7, program.program_id))
            .expect("naked enrollment should exist");
        assert_eq!(
            enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
            Some(0)
        );
        assert_eq!(
            enrollment.lifetime_forfeited_rebates_micro_usdc.as_u128(),
            Some(5)
        );
        assert!(engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::MmProgramBonusForfeited {
                market_maker_identity: MarketMakerIdentity::Account(22),
                economic_account_id,
                market_id,
                program_id,
                forfeited_bonus_micro_usdc,
            } if *economic_account_id == 22
                && *market_id == 7
                && *program_id == program.program_id
                && *forfeited_bonus_micro_usdc == 5
        )));
    }
}

#[test]
fn market_maker_bonus_draws_from_existing_pool_without_pausing_when_funded() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_000);
    engine.update_market_mark_price(7, 100, 1_000);
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 50,
            taker_fee_bps: 100,
        },
    );
    engine
        .create_account(22, [22; 32], [9; 32], 1_010)
        .expect("naked market maker account should create");
    seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let mut program_input = sample_market_maker_program_input([7]);
    program_input.rebate_multiplier_units = 30_000;
    let program = engine
        .define_market_maker_program(program_input, 1_020)
        .expect("program should define");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(22),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_030,
            None,
        )
        .expect("naked enrollment should succeed");
    engine.market_maker_rebate_pools.insert(7, 5);
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 33, 1_000);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_041,
        )
        .expect("taker bid should fill");

    assert_eq!(engine.isolated_collateral(7, 22), 1_015);
    assert!(!engine.market_trading_paused(7));
    assert_eq!(engine.market_maker_rebate_pool_balance(7), 0);
    let enrollment = engine
        .market_maker_enrollments
        .get(&(MarketMakerIdentity::Account(22), 7, program.program_id))
        .expect("naked enrollment should exist");
    assert_eq!(
        enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
        Some(10)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MmProgramBonusPaid {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id,
            requested_bonus_micro_usdc,
            paid_bonus_micro_usdc,
        } if *economic_account_id == 22
            && *market_id == 7
            && *program_id == program.program_id
            && *requested_bonus_micro_usdc == 10
            && *paid_bonus_micro_usdc == 10
    )));
}

#[test]
fn maker_market_maker_bonus_does_not_apply_to_taker_fills() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_000);
    engine.update_market_mark_price(7, 100, 1_000);
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 50,
            taker_fee_bps: 100,
        },
    );
    engine
        .create_account(22, [22; 32], [9; 32], 1_010)
        .expect("enrolled taker should create");
    seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_020)
        .expect("program should define");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(22),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_030,
            None,
        )
        .expect("naked enrollment should succeed");
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 44, 1_000);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_041,
        )
        .expect("enrolled taker bid should fill");

    assert_eq!(engine.isolated_collateral(7, 44), 1_005);
    assert_eq!(engine.isolated_collateral(7, 22), 990);
    let enrollment = engine
        .market_maker_enrollments
        .get(&(MarketMakerIdentity::Account(22), 7, program.program_id))
        .expect("naked enrollment should exist");
    assert_eq!(
        enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
        Some(0)
    );
    assert_eq!(
        enrollment.lifetime_forfeited_rebates_micro_usdc.as_u128(),
        Some(0)
    );
    assert!(
        !engine.runtime_events().iter().any(|event| matches!(
            event.kind,
            RuntimeEventKind::MmProgramBonusPaid { .. }
                | RuntimeEventKind::MmProgramBonusForfeited { .. }
        )),
        "taker participation must not trigger MM bonus accounting"
    );
}

#[test]
fn exited_and_slashed_enrollments_are_bonus_noops() {
    for status in [
        MarketMakerEnrollmentStatus::Exited,
        MarketMakerEnrollmentStatus::Slashed,
    ] {
        let mut engine = SingleNodeEngine::new(single_node_config());
        list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
        engine.update_market_index_price(7, 100, 1_000);
        engine.update_market_mark_price(7, 100, 1_000);
        engine.update_market_fee_parameters(
            7,
            MarketFeeParameters {
                maker_fee_bps: 0,
                maker_rebate_bps: 50,
                taker_fee_bps: 100,
            },
        );
        engine
            .create_account(22, [22; 32], [9; 32], 1_010)
            .expect("naked market maker account should create");
        seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
        let program = engine
            .define_market_maker_program(sample_market_maker_program_input([7]), 1_020)
            .expect("program should define");
        let identity = MarketMakerIdentity::Account(22);
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: identity,
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_030,
                None,
            )
            .expect("naked enrollment should succeed");
        engine
            .market_maker_enrollments
            .get_mut(&(identity, 7, program.program_id))
            .expect("naked enrollment should exist")
            .status = status;
        engine.set_isolated_collateral(7, 22, 1_000);
        engine.set_isolated_collateral(7, 33, 1_000);

        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id: 7,
                    account_id: 22,
                    client_order_id: 1,
                    side: kombat_chain_matching::OrderSide::Ask,
                    quantity_lots: 5,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_040,
            )
            .expect("maker ask should rest");
        engine
            .submit_order(
                kombat_chain_matching::OrderRequest {
                    market_id: 7,
                    account_id: 33,
                    client_order_id: 2,
                    side: kombat_chain_matching::OrderSide::Bid,
                    quantity_lots: 5,
                    kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                    time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                    post_only: false,
                },
                1_041,
            )
            .expect("taker bid should fill");

        assert_eq!(engine.isolated_collateral(7, 22), 1_005);
        let enrollment = engine
            .market_maker_enrollments
            .get(&(identity, 7, program.program_id))
            .expect("naked enrollment should exist");
        assert_eq!(
            enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
            Some(0)
        );
        assert_eq!(
            enrollment.lifetime_forfeited_rebates_micro_usdc.as_u128(),
            Some(0)
        );
        assert!(
            !engine.runtime_events().iter().any(|event| matches!(
                event.kind,
                RuntimeEventKind::MmProgramBonusPaid { .. }
                    | RuntimeEventKind::MmProgramBonusForfeited { .. }
            )),
            "non-live enrollments must not emit MM bonus events"
        );
    }
}

#[test]
fn mm_bonus_shortfall_partially_pays_then_pauses_market() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_scaled_test_perpetual_market(&mut engine, 7, 20_000, 1_000_000);
    engine.update_market_index_price(7, 100, 1_000);
    engine.update_market_mark_price(7, 100, 1_000);
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 50,
            taker_fee_bps: 100,
        },
    );
    engine
        .create_account(22, [22; 32], [9; 32], 1_010)
        .expect("naked market maker account should create");
    seed_account_policy_usdc(&mut engine, 22, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let mut program_input = sample_market_maker_program_input([7]);
    program_input.rebate_multiplier_units = 30_000;
    let program = engine
        .define_market_maker_program(program_input, 1_020)
        .expect("program should define");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(22),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_030,
            None,
        )
        .expect("naked enrollment should succeed");
    engine.set_isolated_collateral(7, 22, 1_000);
    engine.set_isolated_collateral(7, 33, 1_000);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_040,
        )
        .expect("maker ask should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_041,
        )
        .expect("taker bid should fill");

    assert_eq!(engine.isolated_collateral(7, 22), 1_010);
    assert!(engine.market_trading_paused(7));
    assert_eq!(engine.market_maker_rebate_pool_balance(7), 0);
    let enrollment = engine
        .market_maker_enrollments
        .get(&(MarketMakerIdentity::Account(22), 7, program.program_id))
        .expect("naked enrollment should exist");
    assert_eq!(
        enrollment.lifetime_compliant_rebates_micro_usdc.as_u128(),
        Some(5)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MmProgramBonusPaid {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id,
            requested_bonus_micro_usdc,
            paid_bonus_micro_usdc,
        } if *economic_account_id == 22
            && *market_id == 7
            && *program_id == program.program_id
            && *requested_bonus_micro_usdc == 10
            && *paid_bonus_micro_usdc == 5
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketTradingStateUpdated {
            market_id: 7,
            paused: true,
        }
    )));
}

#[test]
fn market_maker_compliance_enters_grace_then_recovers_and_round_trips_snapshot() {
    let (mut engine, identity, program_id) = seed_market_maker_compliance_engine();

    engine
        .produce_empty_block(10_000)
        .expect("first compliance sample should produce a breach");

    let enrollment = engine
        .market_maker_enrollments
        .get(&(identity, 7, program_id))
        .expect("enrollment should exist after first sample");
    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::GracePeriod);
    assert_eq!(enrollment.last_compliance_checked_ms, 10_000);
    assert_eq!(enrollment.observation_history.len(), 1);
    assert_eq!(
        enrollment.current_compliance.last_window_breach,
        Some(crate::MarketMakerBreachKind::Uptime)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerEnrollmentGraceEntered {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id: observed_program_id,
            breach: kombat_chain_streams::MarketMakerBreachKind::Uptime,
        } if *economic_account_id == 22
            && *market_id == 7
            && *observed_program_id == program_id
    )));

    engine
        .produce_empty_block(10_500)
        .expect("same-slot block should not duplicate the compliance sample");
    assert_eq!(
        engine
            .market_maker_enrollments
            .get(&(identity, 7, program_id))
            .expect("enrollment should still exist")
            .observation_history
            .len(),
        1
    );

    refresh_scaled_test_market_prices(&mut engine, 7, 19_000);
    submit_two_sided_market_maker_quotes(&mut engine, 7, 22, 19_500);
    engine
        .produce_empty_block(20_000)
        .expect("compliant sample should recover the enrollment");

    let enrollment = engine
        .market_maker_enrollments
        .get(&(identity, 7, program_id))
        .expect("enrollment should exist after recovery");
    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::Active);
    assert_eq!(enrollment.last_compliance_checked_ms, 20_000);
    assert_eq!(enrollment.observation_history.len(), 2);
    assert_eq!(enrollment.current_compliance.last_window_breach, None);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerEnrollmentRecovered {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id: observed_program_id,
            recovered_from: kombat_chain_streams::MarketMakerEnrollmentStatus::GracePeriod,
        } if *economic_account_id == 22
            && *market_id == 7
            && *observed_program_id == program_id
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve compliance history");
    let restored_enrollment = restored
        .market_maker_enrollments
        .get(&(identity, 7, program_id))
        .expect("restored enrollment should exist");
    assert_eq!(
        restored_enrollment.status,
        MarketMakerEnrollmentStatus::Active
    );
    assert_eq!(
        restored_enrollment.observation_history,
        enrollment.observation_history
    );
    assert_eq!(
        restored_enrollment.current_compliance,
        enrollment.current_compliance
    );
    assert_eq!(
        restored_enrollment.last_compliance_checked_ms,
        enrollment.last_compliance_checked_ms
    );
}

#[test]
fn market_maker_compliance_escalates_to_non_compliant_and_recovers_after_cooldown() {
    let (mut engine, identity, program_id) = seed_market_maker_compliance_engine();

    engine
        .produce_empty_block(10_000)
        .expect("first compliance breach should enter grace");
    engine
        .produce_empty_block(3_610_000)
        .expect("persistent breach should escalate to non-compliant");

    let enrollment = engine
        .market_maker_enrollments
        .get(&(identity, 7, program_id))
        .expect("enrollment should exist after escalation");
    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::NonCompliant);
    assert_eq!(enrollment.current_compliance.breach_counter, 1);
    assert_eq!(enrollment.last_compliance_checked_ms, 3_610_000);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerEnrollmentNonCompliant {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id: observed_program_id,
            breach_counter,
        } if *economic_account_id == 22
            && *market_id == 7
            && *observed_program_id == program_id
            && *breach_counter == 1
    )));

    refresh_scaled_test_market_prices(&mut engine, 7, 90_008_000);
    submit_two_sided_market_maker_quotes(&mut engine, 7, 22, 90_008_500);
    engine
        .produce_empty_block(90_009_000)
        .expect("compliant sample during cooldown must stay non-compliant");

    let enrollment = engine
        .market_maker_enrollments
        .get(&(identity, 7, program_id))
        .expect("enrollment should remain present during cooldown");
    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::NonCompliant);
    assert_eq!(enrollment.last_compliance_checked_ms, 90_000_000);
    assert!(
        !engine.runtime_events().iter().any(|event| matches!(
            &event.kind,
            RuntimeEventKind::MarketMakerEnrollmentRecovered {
                market_maker_identity: MarketMakerIdentity::Account(22),
                economic_account_id,
                market_id,
                program_id: observed_program_id,
                recovered_from: kombat_chain_streams::MarketMakerEnrollmentStatus::NonCompliant,
            } if *economic_account_id == 22
                && *market_id == 7
                && *observed_program_id == program_id
        )),
        "cooldown gate must suppress recovery events before re-entry is allowed"
    );

    refresh_scaled_test_market_prices(&mut engine, 7, 90_009_000);
    submit_two_sided_market_maker_quotes(&mut engine, 7, 22, 90_009_500);
    engine
        .produce_empty_block(90_010_000)
        .expect("cooldown-complete compliant sample should recover");

    let enrollment = engine
        .market_maker_enrollments
        .get(&(identity, 7, program_id))
        .expect("enrollment should exist after cooldown recovery");
    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::Active);
    assert_eq!(enrollment.last_compliance_checked_ms, 90_010_000);
    assert_eq!(enrollment.current_compliance.last_window_breach, None);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerEnrollmentRecovered {
            market_maker_identity: MarketMakerIdentity::Account(22),
            economic_account_id,
            market_id,
            program_id: observed_program_id,
            recovered_from: kombat_chain_streams::MarketMakerEnrollmentStatus::NonCompliant,
        } if *economic_account_id == 22
            && *market_id == 7
            && *observed_program_id == program_id
    )));
}

#[test]
fn block_event_cursor_skips_uncommitted_events_before_first_block_event() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        )
        .expect("direct maker ask should rest");
    engine
        .submit_intent(
            signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::PlaceOrder {
                    market_id: 7,
                    client_order_id: 2,
                    side: IntentOrderSide::Bid,
                    quantity_lots: 3,
                    kind: IntentOrderKind::Limit { price_ticks: 104 },
                    time_in_force: IntentTimeInForce::Gtc,
                    post_only: false,
                },
            ),
            1_100,
        )
        .expect("crossing intent should admit");

    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block should commit crossing fill");

    assert_eq!(
        engine
            .block_event_cursor(0)
            .map(|cursor| cursor.lower_bound_seq),
        Some(2)
    );
    assert_eq!(
        engine
            .block_event_cursor(1)
            .map(|cursor| cursor.lower_bound_seq),
        Some(2)
    );
    let range = engine.seq_range_for_timestamps(1_000, 1_201);
    assert_eq!(range.from_seq, 2);
    assert_eq!(range.before_seq, engine.next_runtime_event_seq());
}

#[test]
fn single_node_engine_surfaces_order_rejections() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let invalid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 0,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    assert_eq!(
        engine.submit_order(invalid, 1_000),
        Err(NodeError::Order(
            kombat_chain_matching::OrderRejectReason::ZeroQuantity,
        ))
    );
}

#[test]
fn fee_funded_insurance_auto_covers_existing_bad_debt() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    engine
        .liquidate_position(7, 22, 99, 5, 1_004)
        .expect("liquidation should create bad debt");
    assert_eq!(engine.market_bad_debt_balance(7), 20);
    assert!(engine.account_frozen(22));

    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 0,
            taker_fee_bps: 1_000,
        },
    );
    engine.set_isolated_collateral(7, 66, 1_000);
    engine.set_isolated_collateral(7, 77, 1_000);
    let resting_ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 66,
        client_order_id: 5,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 10,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let taking_bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 77,
        client_order_id: 6,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 10,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine
        .submit_order(resting_ask, 1_005)
        .expect("resting ask should succeed");
    engine
        .submit_order(taking_bid, 1_006)
        .expect("taking bid should succeed");

    engine
        .runtime_events()
        .iter()
        .find_map(|event| match &event.kind {
            RuntimeEventKind::FeeRevenueDistributed {
                market_id,
                insurance_fund_delta,
                ..
            } if *market_id == 7 && *insurance_fund_delta > 0 => Some(*insurance_fund_delta),
            _ => None,
        })
        .expect("fee distribution should credit insurance");
    let post_trade_bad_debt = engine.market_bad_debt_balance(7);
    assert!(post_trade_bad_debt < 20);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert!(engine.account_frozen(22));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtRecovered {
            market_id,
            account_id,
            recovered,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *recovered > 0
            && *balance == post_trade_bad_debt
            && *source == RiskTransitionSource::InsuranceSweep
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::InsuranceFundUpdated {
            market_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *delta < 0
            && *balance == 0
            && *source == super::InsuranceFundUpdateSource::BankruptcyCover
    )));
}

#[test]
fn insurance_sweep_covers_market_bad_debt_in_account_id_order() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, -10);
    engine.set_isolated_collateral(7, 33, -20);
    engine.sync_account_market_bad_debt(
        7,
        22,
        10,
        1_000,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    engine.sync_account_market_bad_debt(
        7,
        33,
        20,
        1_000,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    engine.insurance_funds.insert(7, 15);

    engine.apply_insurance_fund_to_market_bad_debt(7, 1_001);

    assert_eq!(engine.isolated_collateral(7, 22), 0);
    assert_eq!(engine.isolated_collateral(7, 33), -15);
    assert_eq!(engine.market_bad_debt_balance(7), 15);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert!(!engine.account_market_bad_debt.contains_key(&(7, 22)));
    assert_eq!(engine.account_market_bad_debt.get(&(7, 33)), Some(&15));

    let recovered: Vec<(u64, u128, u128)> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance,
                source,
            } if *market_id == 7 && *source == RiskTransitionSource::InsuranceSweep => {
                Some((*account_id, *recovered, *balance))
            }
            _ => None,
        })
        .collect();
    assert_eq!(recovered, vec![(22, 10, 20), (33, 5, 15)]);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::InsuranceFundUpdated {
            market_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *delta == -15
            && *balance == 0
            && *source == super::InsuranceFundUpdateSource::BankruptcyCover
    )));
}

#[test]
fn market_insurance_sweep_spends_real_capital_wallet_through_isolated_margin_custody() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, -20);
    engine.set_isolated_collateral(7, 33, 20);
    engine.sync_account_market_bad_debt(
        7,
        22,
        20,
        1_000,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    engine.insurance_funds.insert(7, 5);
    seed_isolated_margin_market_custody(&mut engine, 7, 5);
    seed_real_capital_insurance_wallet(&mut engine, 7, 7);
    let event_start = engine.runtime_events().len();
    let state_root_before_sweep = engine.head().header.state_root;
    let insurance_wallet_balance_before =
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());
    let market_custody_balance_before =
        policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7));
    let evm_total_before_sweep = insurance_wallet_balance_before + market_custody_balance_before;

    engine.apply_insurance_fund_to_market_bad_debt(7, 1_001);

    assert_eq!(engine.isolated_collateral(7, 22), -8);
    assert_eq!(engine.isolated_collateral(7, 33), 20);
    assert_eq!(engine.market_bad_debt_balance(7), 8);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(engine.real_capital_insurance_fund_micro_usdc(), 7);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address()),
        0
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7)),
        market_custody_balance_before + 7
    );
    let evm_total_after_sweep =
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address())
            + policy_usdc_balance_for_test(
                &engine,
                super::isolated_margin_market_custody_address(7),
            );
    assert_eq!(evm_total_after_sweep, evm_total_before_sweep);
    assert_ne!(engine.head().header.state_root, state_root_before_sweep);
    assert!(engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            &event.kind,
            RuntimeEventKind::InsuranceFundUpdated {
                market_id,
                delta,
                balance,
                source: super::InsuranceFundUpdateSource::BankruptcyCover,
            } if *market_id == 7 && *delta == -5 && *balance == 0
        )));
    assert!(engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            &event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent {
                amount,
                balance,
                source: RealCapitalInsuranceWalletSpendSource::BadDebtCover,
            } if *amount == 7 && *balance == 0
        )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve custody-backed bad-debt cover");
    assert_eq!(restored.isolated_collateral(7, 22), -8);
    assert_eq!(restored.market_bad_debt_balance(7), 8);
    assert_eq!(restored.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&restored, super::real_capital_insurance_wallet_address()),
        0
    );
    assert_eq!(
        policy_usdc_balance_for_test(&restored, super::isolated_margin_market_custody_address(7)),
        market_custody_balance_before + 7
    );
}

#[test]
fn market_insurance_sweep_skips_real_capital_wallet_event_when_wallet_cover_is_zero() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, -20);
    engine.set_isolated_collateral(7, 33, 25);
    engine.sync_account_market_bad_debt(
        7,
        22,
        20,
        1_000,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    engine.insurance_funds.insert(7, 5);
    seed_isolated_margin_market_custody(&mut engine, 7, 5);
    seed_real_capital_insurance_wallet(&mut engine, 7, 0);
    let event_start = engine.runtime_events().len();
    let state_root_before_sweep = engine.head().header.state_root;
    let custody_balance_before =
        policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7));

    engine.apply_insurance_fund_to_market_bad_debt(7, 1_001);

    assert_eq!(engine.market_bad_debt_balance(7), 15);
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(engine.real_capital_insurance_fund_micro_usdc(), 7);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7)),
        custody_balance_before
    );
    assert_eq!(engine.head().header.state_root, state_root_before_sweep);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            &event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent {
                source: RealCapitalInsuranceWalletSpendSource::BadDebtCover,
                ..
            }
        )));
}

#[test]
fn real_capital_bad_debt_cover_unblocks_market_withdrawals_once_resolved() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    list_test_perpetual_market(&mut engine, 7);
    seed_account_policy_usdc(&mut engine, 11, 0);
    engine.set_isolated_collateral(7, 11, 30);
    engine.set_isolated_collateral(7, 22, -20);
    engine.sync_account_market_bad_debt(
        7,
        22,
        20,
        1_000,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    seed_isolated_margin_market_custody(&mut engine, 7, 10);
    seed_real_capital_insurance_wallet(&mut engine, 20, 20);
    let account_address: [u8; 20] = engine
        .get_committed_state(&account_evm_address_state_key(11))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("account address should exist");
    let evm_total_before_cover_and_withdraw =
        policy_usdc_balance_for_test(&engine, account_address)
            + policy_usdc_balance_for_test(
                &engine,
                super::isolated_margin_market_custody_address(7),
            )
            + policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());

    let rejected_withdraw = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    let error = engine
        .submit_intent(rejected_withdraw, 1_100)
        .expect_err("bad debt gate should block pre-cover withdrawal");
    assert_eq!(
        error,
        NodeError::InvalidCollateralMovement {
            market_id: 7,
            account_id: 11,
            reason: "market_bad_debt_outstanding",
        }
    );

    engine.apply_insurance_fund_to_market_bad_debt(7, 1_200);

    assert_eq!(engine.market_bad_debt_balance(7), 0);
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7)),
        30
    );

    let allowed_withdraw = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::WithdrawCollateral {
            market_id: 7,
            amount_usdc: 1,
        },
    );
    engine
        .submit_intent(allowed_withdraw, 1_300)
        .expect("withdraw should admit once bad debt is fully covered");
    let block = engine
        .produce_block_from_mempool(1_400, 10)
        .expect("covered market withdraw should execute")
        .clone();

    assert!(!block.body.items.is_empty());

    assert_eq!(engine.isolated_collateral(7, 11), 29);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7)),
        29
    );
    assert_eq!(policy_usdc_balance_for_test(&engine, account_address), 1);
    let evm_total_after_cover_and_withdraw = policy_usdc_balance_for_test(&engine, account_address)
        + policy_usdc_balance_for_test(&engine, super::isolated_margin_market_custody_address(7))
        + policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());
    assert_eq!(
        evm_total_after_cover_and_withdraw,
        evm_total_before_cover_and_withdraw
    );
}

#[test]
fn funding_updates_finish_recovery_after_partial_insurance_sweep() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(2), 1_004)
        .expect("partial liquidation should succeed");
    assert_eq!(outcome.filled_lots, 2);
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.account_frozen(22));

    let initial_bad_debt = engine.market_bad_debt_balance(7);
    assert!(initial_bad_debt > 1);

    engine
        .insurance_funds
        .insert(7, initial_bad_debt.saturating_sub(1));
    engine.apply_insurance_fund_to_market_bad_debt(7, 1_005);

    assert_eq!(engine.market_bad_debt_balance(7), 1);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert_eq!(engine.account_market_bad_debt.get(&(7, 22)), Some(&1));
    assert!(engine.account_frozen(22));

    engine.update_market_funding_index(7, -100, 1_006);

    assert_eq!(engine.market_bad_debt_balance(7), 0);
    assert!(!engine.account_market_bad_debt.contains_key(&(7, 22)));
    assert!(!engine.account_frozen(22));
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingApplied {
            market_id,
            account_id,
            funding_delta,
            funding_index,
        } if *market_id == 7
            && *account_id == 22
            && *funding_delta == 100
            && *funding_index == -100
    )));

    let recovered: Vec<(u128, u128, RiskTransitionSource)> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance,
                source,
            } if *market_id == 7 && *account_id == 22 => Some((*recovered, *balance, *source)),
            _ => None,
        })
        .collect();
    assert_eq!(
        recovered,
        vec![
            (
                initial_bad_debt.saturating_sub(1),
                1,
                RiskTransitionSource::InsuranceSweep
            ),
            (1, 0, RiskTransitionSource::FundingUpdate),
        ]
    );
}

#[test]
fn mark_updates_finish_recovery_after_partial_insurance_sweep() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(2), 1_004)
        .expect("partial liquidation should succeed");
    assert_eq!(outcome.filled_lots, 2);
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.account_frozen(22));

    let initial_bad_debt = engine.market_bad_debt_balance(7);
    assert!(initial_bad_debt > 1);

    engine
        .insurance_funds
        .insert(7, initial_bad_debt.saturating_sub(1));
    engine.apply_insurance_fund_to_market_bad_debt(7, 1_005);

    assert_eq!(engine.market_bad_debt_balance(7), 1);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert_eq!(engine.account_market_bad_debt.get(&(7, 22)), Some(&1));
    assert!(engine.account_frozen(22));

    engine.update_market_mark_price(7, 200, 1_006);

    assert_eq!(engine.market_bad_debt_balance(7), 0);
    assert!(!engine.account_market_bad_debt.contains_key(&(7, 22)));
    assert!(!engine.account_frozen(22));
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarkPriceUpdated {
            market_id,
            mark_price_ticks,
        } if *market_id == 7 && *mark_price_ticks == 200
    )));

    let recovered: Vec<(u128, u128, RiskTransitionSource)> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance,
                source,
            } if *market_id == 7 && *account_id == 22 => Some((*recovered, *balance, *source)),
            _ => None,
        })
        .collect();
    assert_eq!(
        recovered,
        vec![
            (
                initial_bad_debt.saturating_sub(1),
                1,
                RiskTransitionSource::InsuranceSweep
            ),
            (1, 0, RiskTransitionSource::MarkPriceReprice),
        ]
    );
}

#[test]
fn collateral_updates_finish_recovery_after_partial_insurance_sweep() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(2), 1_004)
        .expect("partial liquidation should succeed");
    assert_eq!(outcome.filled_lots, 2);
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.account_frozen(22));

    let initial_bad_debt = engine.market_bad_debt_balance(7);
    assert!(initial_bad_debt > 1);

    engine
        .insurance_funds
        .insert(7, initial_bad_debt.saturating_sub(1));
    engine.apply_insurance_fund_to_market_bad_debt(7, 1_005);

    assert_eq!(engine.market_bad_debt_balance(7), 1);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert_eq!(engine.account_market_bad_debt.get(&(7, 22)), Some(&1));
    assert!(engine.account_frozen(22));

    engine.update_isolated_collateral(7, 22, 100, 1_006);

    assert_eq!(engine.market_bad_debt_balance(7), 0);
    assert!(!engine.account_market_bad_debt.contains_key(&(7, 22)));
    assert!(!engine.account_frozen(22));
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);

    let recovered: Vec<(u128, u128, RiskTransitionSource)> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance,
                source,
            } if *market_id == 7 && *account_id == 22 => Some((*recovered, *balance, *source)),
            _ => None,
        })
        .collect();
    assert_eq!(
        recovered,
        vec![
            (
                initial_bad_debt.saturating_sub(1),
                1,
                RiskTransitionSource::InsuranceSweep
            ),
            (1, 0, RiskTransitionSource::CollateralAdjustment),
        ]
    );
}

#[test]
fn funding_recovery_reconciles_multi_account_bad_debt_in_account_order() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut account_22 = PositionLedger::default();
    account_22.apply_fill(OrderFillSide::Buy, 100, 1, None, 0, 1, 1_000);
    engine.positions.insert((7, 22), account_22);
    engine.set_isolated_collateral(7, 22, -5);

    let mut account_33 = PositionLedger::default();
    account_33.apply_fill(OrderFillSide::Buy, 100, 1, None, 0, 2, 1_000);
    engine.positions.insert((7, 33), account_33);
    engine.set_isolated_collateral(7, 33, -7);

    engine.sync_account_market_bad_debt(
        7,
        22,
        5,
        1_001,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    engine.sync_account_market_bad_debt(
        7,
        33,
        7,
        1_001,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    assert_eq!(engine.market_bad_debt_balance(7), 12);

    engine.update_market_funding_index(7, -10, 1_002);

    assert_eq!(engine.market_bad_debt_balance(7), 0);
    let recovered: Vec<(u64, u128, u128, RiskTransitionSource)> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance,
                source,
            } if *market_id == 7 && *source == RiskTransitionSource::FundingUpdate => {
                Some((*account_id, *recovered, *balance, *source))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        recovered,
        vec![
            (22, 5, 7, RiskTransitionSource::FundingUpdate),
            (33, 7, 0, RiskTransitionSource::FundingUpdate),
        ]
    );
}

#[test]
fn mark_recovery_reconciles_multi_account_bad_debt_in_account_order() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut account_22 = PositionLedger::default();
    account_22.apply_fill(OrderFillSide::Buy, 100, 1, None, 0, 1, 1_000);
    engine.positions.insert((7, 22), account_22);
    engine.set_isolated_collateral(7, 22, 5);

    let mut account_33 = PositionLedger::default();
    account_33.apply_fill(OrderFillSide::Buy, 100, 1, None, 0, 2, 1_000);
    engine.positions.insert((7, 33), account_33);
    engine.set_isolated_collateral(7, 33, 3);

    engine.update_market_mark_price(7, 90, 1_001);
    engine.sync_account_market_bad_debt(
        7,
        22,
        5,
        1_001,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    engine.sync_account_market_bad_debt(
        7,
        33,
        7,
        1_001,
        true,
        RiskTransitionSource::LiquidationSolvency,
    );
    assert_eq!(engine.market_bad_debt_balance(7), 12);

    engine.update_market_mark_price(7, 100, 1_002);

    assert_eq!(engine.market_bad_debt_balance(7), 0);
    let recovered: Vec<(u64, u128, u128, RiskTransitionSource)> = engine
        .runtime_events()
        .iter()
        .filter_map(|event| match &event.kind {
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance,
                source,
            } if *market_id == 7 && *source == RiskTransitionSource::MarkPriceReprice => {
                Some((*account_id, *recovered, *balance, *source))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        recovered,
        vec![
            (22, 5, 7, RiskTransitionSource::MarkPriceReprice),
            (33, 7, 0, RiskTransitionSource::MarkPriceReprice),
        ]
    );
}

#[test]
fn fee_funded_insurance_covers_same_fill_shortfall() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.update_market_fee_parameters(
        7,
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 0,
            taker_fee_bps: 1_000,
        },
    );
    engine.set_isolated_collateral(7, 22, 5);
    engine.set_isolated_collateral(7, 33, 100);

    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        )
        .expect("resting ask should succeed");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        )
        .expect("taking bid should succeed");

    assert_eq!(engine.margin_snapshot(7, 22).equity, -4);
    assert_eq!(engine.market_bad_debt_balance(7), 4);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *delta == 5
            && *balance == 5
            && *source == RiskTransitionSource::CollateralAdjustment
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtRecovered {
            market_id,
            account_id,
            recovered,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *recovered == 1
            && *balance == 4
            && *source == RiskTransitionSource::InsuranceSweep
    )));
}

#[test]
fn single_node_engine_liquidates_under_margin_position() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let reprice_bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 44,
        client_order_id: 3,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let passive_ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 55,
        client_order_id: 4,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 5,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };

    engine.submit_order(bid, 1_000).expect("bid should rest");
    engine.submit_order(ask, 1_001).expect("ask should fill");
    engine
        .submit_order(reprice_bid, 1_002)
        .expect("reprice bid should rest");
    engine
        .submit_order(passive_ask, 1_003)
        .expect("passive ask should rest");

    let outcome = engine
        .liquidate_position(7, 22, 99, 5, 1_004)
        .expect("liquidation should succeed");
    assert_eq!(outcome.filled_lots, 3);
    assert_eq!(outcome.remaining_lots, 0);
    assert_eq!(engine.position(7, 22).net_lots(), 0);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_account_id,
            requested_lots,
            filled_lots,
            remaining_lots,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_account_id == 99
            && *requested_lots == 3
            && *filled_lots == 3
            && *remaining_lots == 0
    )));
    assert!(engine.account_frozen(22));
    assert_eq!(engine.market_bad_debt_balance(7), 20);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *delta == 20
            && *balance == 20
            && *source == RiskTransitionSource::LiquidationSolvency
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::SolvencyBreached {
            market_id,
            account_id,
            residual_bad_debt,
        } if *market_id == 7 && *account_id == 22 && *residual_bad_debt == 20
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationOutcome {
            market_id,
            account_id,
            liquidator_account_id,
            outcome,
            post_under_maintenance,
            post_account_frozen,
            post_market_bad_debt_balance,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_account_id == 99
            && *outcome == LiquidationExecutionOutcome::CreatesBadDebt
            && *post_under_maintenance
            && *post_account_frozen
            && *post_market_bad_debt_balance == 20
    )));
}

#[test]
fn mark_updates_reconcile_bad_debt_for_partially_liquidated_bankrupt_accounts() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(2), 1_004)
        .expect("partial liquidation should succeed");
    assert_eq!(outcome.filled_lots, 2);
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.account_frozen(22));

    let bad_debt_before = engine.market_bad_debt_balance(7);
    assert!(bad_debt_before > 0);

    engine.update_market_mark_price(7, 80, 1_005);

    let bad_debt_after = engine.market_bad_debt_balance(7);
    assert!(bad_debt_after > bad_debt_before);
    assert_eq!(
        bad_debt_after,
        engine.margin_snapshot(7, 22).equity.saturating_neg().max(0) as u128
    );
    assert!(engine.account_frozen(22));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *delta > 0
            && *balance == bad_debt_after
            && *source == RiskTransitionSource::MarkPriceReprice
    )));
}

#[test]
fn funding_updates_reconcile_bad_debt_for_partially_liquidated_bankrupt_accounts() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(2), 1_004)
        .expect("partial liquidation should succeed");
    assert_eq!(outcome.filled_lots, 2);
    assert_eq!(engine.position(7, 22).net_lots(), 1);
    assert!(engine.account_frozen(22));

    let bad_debt_before = engine.market_bad_debt_balance(7);
    let realized_before = engine.position(7, 22).realized_pnl;
    assert!(bad_debt_before > 0);

    engine.update_market_funding_index(7, 10, 1_005);

    let bad_debt_after = engine.market_bad_debt_balance(7);
    assert!(bad_debt_after > bad_debt_before);
    assert!(engine.position(7, 22).realized_pnl < realized_before);
    assert_eq!(
        bad_debt_after,
        engine.margin_snapshot(7, 22).equity.saturating_neg().max(0) as u128
    );
    assert!(engine.account_frozen(22));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::FundingApplied {
            market_id,
            account_id,
            funding_delta,
            funding_index,
        } if *market_id == 7
            && *account_id == 22
            && *funding_delta == -10
            && *funding_index == 10
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *delta > 0
            && *balance == bad_debt_after
            && *source == RiskTransitionSource::FundingUpdate
    )));
}

#[test]
fn mark_updates_unfreeze_stalled_accounts_once_they_become_healthy() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    engine.submit_order(bid, 1_000).expect("bid should fill");
    engine.submit_order(ask, 1_001).expect("ask should fill");
    engine.update_market_mark_price(7, 98, 1_002);

    engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(1), 1_003)
        .expect("first stalled liquidation should succeed");
    engine
        .liquidate_position_with_size(7, 22, 99, 6, Some(1), 1_004)
        .expect("second stalled liquidation should freeze account");
    assert!(engine.account_frozen(22));
    assert!(!engine.margin_snapshot(7, 22).equity.is_negative());
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    assert_eq!(engine.market_bad_debt_balance(7), 0);

    engine.update_market_mark_price(7, 103, 1_005);

    assert!(!engine.account_frozen(22));
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationCandidateUpdated {
            market_id,
            account_id,
            under_maintenance,
            ..
        } if *market_id == 7 && *account_id == 22 && !*under_maintenance
    )));
}

#[test]
fn single_node_engine_allows_liquidation_for_frozen_account() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 95 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(1), 1_004)
        .expect("first liquidation should create bad debt");
    assert!(engine.account_frozen(22));
    let bad_debt_before = engine.market_bad_debt_balance(7);
    let bad_debt_events_before = engine
        .runtime_events()
        .iter()
        .filter(|event| matches!(event.kind, RuntimeEventKind::BadDebtUpdated { .. }))
        .count();

    let result = engine
        .liquidate_position_with_size(7, 22, 99, 6, Some(1), 1_005)
        .expect("forced liquidation should still be allowed for frozen target");
    assert_eq!(result.filled_lots, 0);
    assert!(engine.market_bad_debt_balance(7) <= bad_debt_before);
    assert!(
        engine
            .runtime_events()
            .iter()
            .filter(|event| matches!(event.kind, RuntimeEventKind::BadDebtUpdated { .. }))
            .count()
            >= bad_debt_events_before
    );
}

#[test]
fn single_node_engine_syncs_bad_debt_after_bankruptcy_collateral_changes() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 95 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    engine
        .liquidate_position(7, 22, 99, 5, 1_004)
        .expect("liquidation should create bad debt");
    let bad_debt_before = engine.market_bad_debt_balance(7);
    assert!(bad_debt_before > 0);
    assert!(engine.account_frozen(22));

    engine.update_isolated_collateral(7, 22, -100, 1_005);
    let worsened_shortfall = engine.margin_snapshot(7, 22).equity.saturating_neg().max(0) as u128;
    assert!(worsened_shortfall > bad_debt_before);
    assert_eq!(engine.market_bad_debt_balance(7), worsened_shortfall);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *delta == worsened_shortfall.saturating_sub(bad_debt_before)
            && *balance == worsened_shortfall
            && *source == RiskTransitionSource::CollateralAdjustment
    )));

    engine.update_isolated_collateral(7, 22, 100, 1_006);
    assert_eq!(engine.market_bad_debt_balance(7), 0);
    assert!(!engine.account_frozen(22));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtRecovered {
            market_id,
            account_id,
            recovered,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *recovered == worsened_shortfall
            && *balance == 0
            && *source == RiskTransitionSource::CollateralAdjustment
    )));
}

#[test]
fn single_node_engine_uses_suggested_size_for_default_liquidations() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 40);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }
    engine.update_market_mark_price(7, 90, 1_003);

    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    let suggestion = engine
        .suggested_liquidation_size(7, 22)
        .expect("suggestion should exist");
    assert_eq!(suggestion.lots, 1);
    assert!(suggestion.clears_maintenance);
    assert_eq!(suggestion.post_margin_surplus, 1);
    assert!(!suggestion.post_under_maintenance);
    assert!(!suggestion.post_account_frozen);
    assert_eq!(suggestion.post_market_bad_debt_balance, 0);

    let outcome = engine
        .liquidate_position(7, 22, 99, 5, 1_004)
        .expect("liquidation should succeed");
    assert_eq!(outcome.filled_lots, 1);
    assert_eq!(outcome.remaining_lots, 0);
    assert_eq!(engine.position(7, 22).net_lots(), 2);
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(!engine.account_frozen(22));
    assert_eq!(engine.market_bad_debt_balance(7), 0);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_account_id,
            requested_lots,
            filled_lots,
            remaining_lots,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_account_id == 99
            && *requested_lots == 1
            && *filled_lots == 1
            && *remaining_lots == 0
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationOutcome {
            market_id,
            account_id,
            liquidator_account_id,
            outcome,
            post_net_lots,
            post_under_maintenance,
            post_account_frozen,
            post_market_bad_debt_balance,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_account_id == 99
            && *outcome == LiquidationExecutionOutcome::ClearedMaintenance
            && *post_net_lots == 2
            && !*post_under_maintenance
            && !*post_account_frozen
            && *post_market_bad_debt_balance == 0
    )));
}

#[test]
fn liquidation_bypasses_non_oracle_market_pause() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 40);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("setup order should succeed");
    }
    engine.update_market_mark_price(7, 90, 1_003);
    assert!(engine.margin_snapshot(7, 22).under_maintenance);

    engine.set_market_trading_paused(7, true, 1_004);
    let paused_error = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 89 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_005,
        )
        .expect_err("ordinary order should still be blocked while paused");
    assert_eq!(
        paused_error,
        NodeError::MarketTradingPaused { market_id: 7 }
    );

    let outcome = engine
        .liquidate_position(7, 22, 99, 5, 1_006)
        .expect("liquidation should bypass non-oracle pause");
    assert_eq!(outcome.filled_lots, 1);
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
}

#[test]
fn liquidation_pass_scans_candidates_in_market_account_order() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 40);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("setup order should succeed");
    }
    engine.update_market_mark_price(7, 90, 1_003);

    let report = engine.run_liquidation_pass(99, 10, 4, 1_004);

    assert_eq!(report.scanned_positions, 2);
    assert_eq!(report.attempted_positions, 1);
    assert_eq!(report.succeeded_positions, 1);
    assert_eq!(report.failed_positions, 0);
    assert_eq!(report.skipped_stale_markets, 0);
    assert_eq!(report.skipped_frozen_accounts, 0);
    assert_eq!(report.steps.len(), 1);
    assert_eq!(report.steps[0].market_id, 7);
    assert_eq!(report.steps[0].account_id, 22);
    assert_eq!(report.steps[0].client_order_id, 10);
    assert!(report.steps[0].result.is_ok());
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
}

#[test]
fn liquidation_pass_reports_stale_market_skip() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        )
        .expect("bid should rest");
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Ioc,
                post_only: false,
            },
            1_001,
        )
        .expect("ask should fill");
    engine.update_market_oracle_config(
        7,
        kombat_chain_oracle::OracleConfig {
            max_staleness_ms: 1,
        },
        1_002,
    );
    engine.update_market_mark_price(7, 90, 1_003);
    assert!(engine.margin_snapshot(7, 22).under_maintenance);

    let report = engine.run_liquidation_pass(99, 10, 4, 1_005);

    assert_eq!(report.scanned_positions, 2);
    assert_eq!(report.attempted_positions, 0);
    assert_eq!(report.succeeded_positions, 0);
    assert_eq!(report.failed_positions, 0);
    assert_eq!(report.skipped_stale_markets, 1);
    assert_eq!(report.skipped_frozen_accounts, 0);
    assert!(report.steps.is_empty());
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
}

fn seed_consensus_liquidation_candidate(engine: &mut SingleNodeEngine) {
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 40);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("setup order should succeed");
    }
    engine.update_market_mark_price(7, 90, 1_003);
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
}

fn seed_consensus_liquidation_keeper_reward_candidate(engine: &mut SingleNodeEngine) {
    seed_consensus_liquidation_candidate(engine);
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 1_000,
            liquidator_fee_bps: 0,
        },
    );
    seed_account_policy_usdc(engine, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID, 0);
    seed_real_capital_insurance_wallet(engine, 1_000_000, 1_000_000);
}

fn seed_delegation_forced_liquidation_candidate(engine: &mut SingleNodeEngine) -> Delegation {
    let delegation = sample_open_delegation(engine, 22, 1_000);
    let trading_account = delegation_trading_account_id(&delegation.delegation_id);
    engine.set_isolated_collateral(7, trading_account, 40);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: trading_account,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("delegation liquidation setup order should succeed");
    }
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 1_000,
            liquidator_fee_bps: 0,
        },
    );
    engine.update_market_mark_price(7, 90, 1_003);
    assert!(engine.margin_snapshot(7, trading_account).under_maintenance);
    delegation
}

fn seed_consensus_liquidation_candidate_account(
    engine: &mut SingleNodeEngine,
    account_id: u64,
    maker_account_id: u64,
    client_order_id_base: u64,
) {
    engine.set_isolated_collateral(7, account_id, 40);
    engine.set_isolated_collateral(7, maker_account_id, 100);
    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: maker_account_id,
                client_order_id: client_order_id_base,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000_u64.saturating_add(client_order_id_base),
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id,
                client_order_id: client_order_id_base.saturating_add(1),
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001_u64.saturating_add(client_order_id_base),
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("candidate setup order should succeed");
    }
}

#[test]
fn produce_block_from_mempool_appends_consensus_liquidation() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_liquidation_candidate(&mut engine);

    let block = engine
        .produce_block_from_mempool(1_004, 0)
        .expect("liquidation-only block should produce")
        .clone();

    assert_eq!(block.body.items.len(), 1);
    assert!(matches!(
        &block.body.items[0],
        BlockItem::Liquidation(liquidation)
            if liquidation.market_id == 7
                && liquidation.account_id == 22
                && liquidation.requested_lots == Some(1)
                && liquidation.filled_lots == 1
                && liquidation.remaining_lots == 0
                && liquidation.keeper_reward_micro_usdc == 0
    ));
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            filled_lots,
            ..
        } if event.block_height == Some(block.header.height)
            && *market_id == 7
            && *account_id == 22
            && *filled_lots == 1
    )));
}

#[test]
fn produce_block_from_mempool_skips_consensus_liquidation_without_executable_work() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_liquidation_candidate(&mut engine);
    engine
        .cancel_order(7, 44, 3, 1_004)
        .expect("resting liquidation liquidity should cancel");

    let event_start = engine.runtime_events().len();
    let block = engine
        .produce_block_from_mempool(1_005, 0)
        .expect("no-work liquidation block should produce")
        .clone();

    assert!(block.body.items.is_empty());
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
}

#[test]
fn produce_block_from_mempool_skips_consensus_liquidation_when_keeper_reward_wallet_underflows() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_liquidation_candidate(&mut engine);
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 1_000,
            liquidator_fee_bps: 0,
        },
    );
    seed_account_policy_usdc(&mut engine, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID, 0);
    seed_real_capital_insurance_wallet(&mut engine, 1, 1);
    let event_start = engine.runtime_events().len();

    let block = engine
        .produce_block_from_mempool(1_004, 0)
        .expect("liquidation-only block should produce")
        .clone();

    assert!(block.body.items.is_empty());
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn produce_block_from_mempool_skips_consensus_liquidation_without_keeper_reward_evm_address() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_liquidation_candidate(&mut engine);
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 1_000,
            liquidator_fee_bps: 0,
        },
    );
    seed_real_capital_insurance_wallet(&mut engine, 1_000_000, 1_000_000);
    let event_start = engine.runtime_events().len();

    let block = engine
        .produce_block_from_mempool(1_004, 0)
        .expect("liquidation-only block should produce")
        .clone();

    assert!(block.body.items.is_empty());
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn produce_block_from_mempool_skips_consensus_liquidation_when_keeper_reward_keeper_frozen() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_liquidation_keeper_reward_candidate(&mut engine);
    engine.freeze_account(
        super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID,
        1_004,
        RiskTransitionSource::LiquidationSolvency,
    );
    let event_start = engine.runtime_events().len();

    let block = engine
        .produce_block_from_mempool(1_004, 0)
        .expect("liquidation-only block should produce")
        .clone();

    assert!(block.body.items.is_empty());
    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn consensus_liquidation_pays_real_capital_keeper_reward_escrow_v1() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    seed_consensus_liquidation_keeper_reward_candidate(&mut engine);
    let policy_balance = |engine: &SingleNodeEngine, account_id: u64| -> u128 {
        let address = engine
            .get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .expect("account EVM address should exist");
        policy_usdc_balance_for_test(engine, address)
    };
    let keeper_balance_before = policy_balance(&engine, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID);
    let insurance_wallet_balance_before =
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address());
    let event_start = engine.runtime_events().len();

    let block = engine
        .produce_block_from_mempool(1_004, 0)
        .expect("liquidation-only block should produce")
        .clone();

    assert!(matches!(
        block.body.items.as_slice(),
        [BlockItem::Liquidation(liquidation)]
            if liquidation.market_id == 7
                && liquidation.account_id == 22
                && liquidation.requested_lots == Some(3)
                && liquidation.filled_lots == 1
                && liquidation.remaining_lots == 2
                && liquidation.keeper_reward_micro_usdc == 2
    ));
    assert!(engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            &event.kind,
            RuntimeEventKind::LiquidationExecuted {
                market_id,
                account_id,
                filled_lots,
                liquidator_reward,
                ..
            } if event.block_height == Some(block.header.height)
                && *market_id == 7
                && *account_id == 22
                && *filled_lots == 1
                && *liquidator_reward == 2
        )));
    assert_eq!(engine.real_capital_insurance_wallet_micro_usdc(), 999_998);
    assert_eq!(engine.real_capital_insurance_fund_micro_usdc(), 1_000_000);
    assert_eq!(
        policy_balance(&engine, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID),
        keeper_balance_before + 2
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address()),
        insurance_wallet_balance_before - 2
    );
    assert_eq!(
        policy_balance(&engine, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID)
            + policy_usdc_balance_for_test(&engine, super::real_capital_insurance_wallet_address(),),
        keeper_balance_before + insurance_wallet_balance_before
    );
    assert!(engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent {
                amount,
                balance,
                source: RealCapitalInsuranceWalletSpendSource::KeeperReward,
            } if amount == 2 && balance == 999_998
        )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve consensus liquidation payout");
    assert_eq!(restored.real_capital_insurance_wallet_micro_usdc(), 999_998);
    assert_eq!(
        policy_balance(&restored, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID),
        keeper_balance_before + 2
    );
}

#[test]
fn consensus_liquidation_skips_delegation_trading_account_positions() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = seed_delegation_forced_liquidation_candidate(&mut engine);
    seed_account_policy_usdc(&mut engine, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID, 0);
    seed_real_capital_insurance_wallet(&mut engine, 1_000_000, 1_000_000);
    let event_start = engine.runtime_events().len();

    let block = engine
        .produce_block_from_mempool(1_004, 0)
        .expect("delegation-only under-maintenance block should produce")
        .clone();

    assert!(block.body.items.is_empty());
    assert!(
        engine
            .margin_snapshot(7, delegation_trading_account_id(&delegation.delegation_id))
            .under_maintenance
    );
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
    assert!(!engine.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn same_block_deposit_cures_before_consensus_liquidation_selection() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy_for_account(22))
        .expect("policy should insert");
    seed_account_policy_usdc(&mut engine, 22, 100);
    seed_consensus_liquidation_candidate(&mut engine);
    list_test_perpetual_market(&mut engine, 7);

    let deposit = signed_intent_for_policy(
        &signer,
        42,
        22,
        1,
        10_000,
        IntentKind::DepositCollateral {
            market_id: 7,
            amount_usdc: 100,
        },
    );
    engine
        .submit_intent(deposit, 1_004)
        .expect("deposit intent should admit");
    let block = engine
        .produce_block_from_mempool(1_005, 10)
        .expect("mixed cure/liquidation block should produce")
        .clone();

    assert!(matches!(
        block.body.items.as_slice(),
        [BlockItem::Intent(_)]
    ));
    assert_eq!(engine.position(7, 22).net_lots(), 3);
    assert_eq!(engine.isolated_collateral(7, 22), 140);
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::CollateralDeposited {
            market_id: 7,
            account_id: 22,
            amount_usdc: 100,
            collateral: 140,
        } if event.block_height == Some(block.header.height)
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            ..
        } if event.block_height == Some(block.header.height)
            && *market_id == 7
            && *account_id == 22
    )));
}

#[test]
fn consensus_liquidation_selection_is_capped_per_block() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    for index in 0..5 {
        seed_consensus_liquidation_candidate_account(
            &mut engine,
            22 + index,
            122 + index,
            10 + index.saturating_mul(10),
        );
    }
    engine.set_isolated_collateral(7, 44, 1_000);
    engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 9,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_200,
        )
        .expect("liquidation liquidity should rest");
    engine.update_market_mark_price(7, 90, 1_300);

    let block = engine
        .produce_block_from_mempool(1_400, 0)
        .expect("liquidation-capped block should produce")
        .clone();
    let liquidations = block
        .body
        .items
        .iter()
        .filter_map(|item| match item {
            BlockItem::Liquidation(liquidation) => Some(liquidation.account_id),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(liquidations, vec![22, 23, 24, 25]);
    assert_eq!(
        liquidations.len(),
        super::MAX_CONSENSUS_LIQUIDATIONS_PER_BLOCK
    );
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.margin_snapshot(7, 26).under_maintenance);
}

#[test]
fn external_block_applies_consensus_liquidation_item() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let follower_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let mut proposer =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let mut follower =
        SingleNodeEngine::new_with_validator_set(follower_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    seed_consensus_liquidation_candidate(&mut proposer);
    seed_consensus_liquidation_candidate(&mut follower);

    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_004,
            0,
            1_000,
        )
        .expect("network liquidation block should prepare");
    let block = prepared.block().clone();
    assert!(matches!(
        block.body.items.as_slice(),
        [BlockItem::Liquidation(BlockLiquidation {
            market_id: 7,
            account_id: 22,
            requested_lots: Some(1),
            filled_lots: 1,
            remaining_lots: 0,
            keeper_reward_micro_usdc: 0,
            ..
        })]
    ));
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    proposer
        .apply_prepared_block(
            prepared,
            qc.clone(),
            &validator_verifiers,
            validator(1),
            1_000,
        )
        .expect("prepared liquidation block should apply locally");
    follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect("external liquidation block should apply");

    assert_eq!(follower.position(7, 22), proposer.position(7, 22));
    assert_eq!(
        follower.isolated_collateral(7, 22),
        proposer.isolated_collateral(7, 22)
    );
    assert!(!follower.margin_snapshot(7, 22).under_maintenance);
    assert!(follower.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            filled_lots,
            ..
        } if event.block_height == Some(1)
            && *market_id == 7
            && *account_id == 22
            && *filled_lots == 1
    )));
}

#[test]
fn external_block_rejects_nondeterministic_consensus_liquidation_item() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let follower_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let mut proposer =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let mut follower =
        SingleNodeEngine::new_with_validator_set(follower_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    seed_consensus_liquidation_candidate(&mut proposer);
    seed_consensus_liquidation_candidate(&mut follower);
    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_004,
            0,
            1_000,
        )
        .expect("network liquidation block should prepare");
    let parent = proposer.head().clone();
    let round = prepared.block().header.round;
    let proposer_id = prepared.block().header.proposer;
    let high_qc = prepared.block().header.qc.clone();
    let timestamp_ms = prepared.block().header.timestamp_ms;
    let mut body = prepared.block().body.clone();
    let Some(BlockItem::Liquidation(liquidation)) = body
        .items
        .iter_mut()
        .find(|item| matches!(item, BlockItem::Liquidation(_)))
    else {
        panic!("prepared block should include liquidation item");
    };
    liquidation.liquidator_account_id = 99;
    let block = proposer
        .executor
        .prepare_block_with_checkpoint(&parent, round, proposer_id, high_qc, body, timestamp_ms)
        .expect("mutated liquidation block should prepare")
        .block;
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let position_before = follower.position(7, 22);
    let event_start = follower.runtime_events().len();

    let err = follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect_err("nondeterministic liquidation item should reject");

    assert!(matches!(
        err,
        NodeError::InvalidBlock(BlockValidationError::InvalidBlockBody)
    ));
    assert_eq!(follower.head().header.height, 0);
    assert_eq!(follower.position(7, 22), position_before);
    assert!(!follower.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
}

#[test]
fn external_block_rejects_mismatched_consensus_liquidation_work_proof() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    seed_consensus_liquidation_candidate(&mut proposer);
    seed_consensus_liquidation_candidate(&mut follower);
    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_004,
            0,
            1_000,
        )
        .expect("network liquidation block should prepare");
    let parent = proposer.head().clone();
    let round = prepared.block().header.round;
    let proposer_id = prepared.block().header.proposer;
    let high_qc = prepared.block().header.qc.clone();
    let timestamp_ms = prepared.block().header.timestamp_ms;
    let mut body = prepared.block().body.clone();
    let Some(BlockItem::Liquidation(liquidation)) = body
        .items
        .iter_mut()
        .find(|item| matches!(item, BlockItem::Liquidation(_)))
    else {
        panic!("prepared block should include liquidation item");
    };
    liquidation.filled_lots = liquidation.filled_lots.saturating_add(1);
    let block = proposer
        .executor
        .prepare_block_with_checkpoint(&parent, round, proposer_id, high_qc, body, timestamp_ms)
        .expect("mutated liquidation work-proof block should prepare")
        .block;
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let position_before = follower.position(7, 22);
    let event_start = follower.runtime_events().len();

    let err = follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect_err("mismatched liquidation work proof should reject");

    assert!(matches!(
        err,
        NodeError::InvalidBlock(BlockValidationError::InvalidBlockBody)
    ));
    assert_eq!(follower.head().header.height, 0);
    assert_eq!(follower.position(7, 22), position_before);
    assert!(!follower.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
}

#[test]
fn external_block_rejects_mismatched_consensus_liquidation_remaining_lots() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    seed_consensus_liquidation_candidate(&mut proposer);
    seed_consensus_liquidation_candidate(&mut follower);
    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_004,
            0,
            1_000,
        )
        .expect("network liquidation block should prepare");
    let parent = proposer.head().clone();
    let round = prepared.block().header.round;
    let proposer_id = prepared.block().header.proposer;
    let high_qc = prepared.block().header.qc.clone();
    let timestamp_ms = prepared.block().header.timestamp_ms;
    let mut body = prepared.block().body.clone();
    let Some(BlockItem::Liquidation(liquidation)) = body
        .items
        .iter_mut()
        .find(|item| matches!(item, BlockItem::Liquidation(_)))
    else {
        panic!("prepared block should include liquidation item");
    };
    liquidation.remaining_lots = liquidation.remaining_lots.saturating_add(1);
    let block = proposer
        .executor
        .prepare_block_with_checkpoint(&parent, round, proposer_id, high_qc, body, timestamp_ms)
        .expect("mutated liquidation remaining-lots block should prepare")
        .block;
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let position_before = follower.position(7, 22);
    let event_start = follower.runtime_events().len();

    let err = follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect_err("mismatched liquidation remaining lots should reject");

    assert!(matches!(
        err,
        NodeError::InvalidBlock(BlockValidationError::InvalidBlockBody)
    ));
    assert_eq!(follower.head().header.height, 0);
    assert_eq!(follower.position(7, 22), position_before);
    assert!(!follower.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
}

#[test]
fn external_block_rejects_mismatched_consensus_liquidation_keeper_reward_amount() {
    let proposer_config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(1),
        consensus_signing_key: [1; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let configured_validator_set = four_node_validator_set();
    let validator_verifiers =
        super::validator_verifiers_for_validator_set(&configured_validator_set)
            .expect("validator verifiers should build");
    let signing_keys = BTreeMap::from([
        (validator(1), [1; 32]),
        (validator(2), [2; 32]),
        (validator(3), [3; 32]),
        (validator(4), [4; 32]),
    ]);
    let mut proposer = SingleNodeEngine::new_with_validator_set(
        proposer_config.clone(),
        Some(&configured_validator_set),
    );
    let mut follower =
        SingleNodeEngine::new_with_validator_set(proposer_config, Some(&configured_validator_set));
    seed_consensus_liquidation_keeper_reward_candidate(&mut proposer);
    seed_consensus_liquidation_keeper_reward_candidate(&mut follower);
    let head_qc =
        build_quorum_certificate(proposer.head(), &signing_keys).expect("head qc should build");
    proposer
        .install_head_qc(head_qc, &validator_verifiers)
        .expect("head qc should install");
    let policy_balance = |engine: &SingleNodeEngine, account_id: u64| -> u128 {
        let address = engine
            .get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .expect("account EVM address should exist");
        engine
            .get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes).to::<u128>())
            .unwrap_or(0)
    };
    let prepared = proposer
        .prepare_network_block_commit_from_mempool(
            validator(1),
            &validator_verifiers,
            1_004,
            0,
            1_000,
        )
        .expect("network liquidation block should prepare");
    let parent = proposer.head().clone();
    let round = prepared.block().header.round;
    let proposer_id = prepared.block().header.proposer;
    let high_qc = prepared.block().header.qc.clone();
    let timestamp_ms = prepared.block().header.timestamp_ms;
    let mut body = prepared.block().body.clone();
    let Some(BlockItem::Liquidation(liquidation)) = body
        .items
        .iter_mut()
        .find(|item| matches!(item, BlockItem::Liquidation(_)))
    else {
        panic!("prepared block should include liquidation item");
    };
    liquidation.keeper_reward_micro_usdc = liquidation.keeper_reward_micro_usdc.saturating_add(1);
    let block = proposer
        .executor
        .prepare_block_with_checkpoint(&parent, round, proposer_id, high_qc, body, timestamp_ms)
        .expect("mutated liquidation reward block should prepare")
        .block;
    let qc = build_quorum_certificate(&block, &signing_keys).expect("block qc should build");
    let position_before = follower.position(7, 22);
    let keeper_balance_before = policy_balance(&follower, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID);
    let wallet_before = follower.real_capital_insurance_wallet_micro_usdc();
    let event_start = follower.runtime_events().len();

    let err = follower
        .apply_external_block(block, qc, &validator_verifiers, validator(1), 1_000)
        .expect_err("mismatched liquidation keeper reward should reject");

    assert!(matches!(
        err,
        NodeError::InvalidBlock(BlockValidationError::InvalidBlockBody)
    ));
    assert_eq!(follower.head().header.height, 0);
    assert_eq!(follower.position(7, 22), position_before);
    assert_eq!(
        policy_balance(&follower, super::CONSENSUS_LIQUIDATOR_ACCOUNT_ID),
        keeper_balance_before
    );
    assert_eq!(
        follower.real_capital_insurance_wallet_micro_usdc(),
        wallet_before
    );
    assert!(!follower.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(event.kind, RuntimeEventKind::LiquidationExecuted { .. })));
    assert!(!follower.runtime_events()[event_start..]
        .iter()
        .any(|event| matches!(
            event.kind,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
        )));
}

#[test]
fn consensus_liquidation_preview_replaces_collateral_overlays() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, 40);

    let positions = engine.positions.clone();
    let market_marks = engine.market_marks.clone();
    let market_index_prices = engine.market_index_prices.clone();
    let order_book_overlays = BTreeMap::new();
    let resting_order_mandate_overlays = BTreeMap::new();
    let collateral_overlays = BTreeMap::new();
    let committed_events = vec![RuntimeEventKind::CollateralWithdrawn {
        market_id: 7,
        account_id: 22,
        amount_usdc: 40,
        collateral: 0,
    }];

    let preview = engine.preview_engine_for_consensus_liquidations(
        1_000,
        &positions,
        &market_marks,
        &market_index_prices,
        &order_book_overlays,
        &resting_order_mandate_overlays,
        &collateral_overlays,
        &committed_events,
    );

    assert_eq!(engine.isolated_collateral(7, 22), 40);
    assert_eq!(preview.isolated_collateral(7, 22), 0);
}

#[test]
fn single_node_engine_routes_liquidation_penalty_to_insurance_makers_and_treasury() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 1_000,
            liquidator_fee_bps: 400,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position(7, 22, 99, 5, 1_004)
        .expect("liquidation should succeed");
    assert_eq!(outcome.filled_lots, 3);
    assert_eq!(engine.isolated_collateral(7, 44), 108);
    assert_eq!(engine.isolated_collateral(7, 99), 0);
    assert_eq!(engine.treasury_balance(7), 6);
    assert_eq!(engine.market_bad_debt_balance(7), 34);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::InsuranceFundUpdated {
            market_id,
            delta,
            balance,
            source,
        } if *market_id == 7
            && *delta == 13
            && *balance == 13
            && *source == super::InsuranceFundUpdateSource::LiquidationPenalty
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::RiskStateUpdated {
            market_id,
            account_id,
            collateral,
            ..
        } if *market_id == 7 && *account_id == 44 && *collateral == 108
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_reward,
            insurance_cover,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_reward == 8
            && *insurance_cover == 13
    )));
}

#[test]
fn single_node_engine_freezes_repeated_stalled_liquidations_and_recovers_on_top_up() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 40);
    engine.set_isolated_collateral(7, 33, 100);
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    engine.submit_order(bid, 1_000).expect("bid should fill");
    engine.submit_order(ask, 1_001).expect("ask should fill");
    engine.update_market_mark_price(7, 90, 1_002);

    assert!(engine.margin_snapshot(7, 22).under_maintenance);
    let first = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(1), 1_003)
        .expect("first stalled liquidation should succeed");
    assert_eq!(first.filled_lots, 0);
    assert!(!engine.account_frozen(22));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationOutcome {
            market_id,
            account_id,
            liquidator_account_id,
            outcome,
            post_under_maintenance,
            post_account_frozen,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_account_id == 99
            && *outcome == LiquidationExecutionOutcome::StillUnderMaintenance
            && *post_under_maintenance
            && !*post_account_frozen
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationEscalationUpdated {
            market_id,
            account_id,
            action,
            stall_count,
            freeze_threshold,
            freeze_imminent,
            post_under_maintenance,
            post_account_frozen,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *action == LiquidationEscalationAction::Advanced
            && *stall_count == 1
            && *freeze_threshold == 2
            && *freeze_imminent
            && *post_under_maintenance
            && !*post_account_frozen
    )));

    let second = engine
        .liquidate_position_with_size(7, 22, 99, 6, Some(1), 1_004)
        .expect("second stalled liquidation should succeed");
    assert_eq!(second.filled_lots, 0);
    assert!(engine.account_frozen(22));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationOutcome {
            market_id,
            account_id,
            liquidator_account_id,
            outcome,
            post_under_maintenance,
            post_account_frozen,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *liquidator_account_id == 99
            && *outcome == LiquidationExecutionOutcome::FrozeAccount
            && *post_under_maintenance
            && *post_account_frozen
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationEscalationUpdated {
            market_id,
            account_id,
            action,
            stall_count,
            freeze_threshold,
            freeze_imminent,
            post_under_maintenance,
            post_account_frozen,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *action == LiquidationEscalationAction::FrozeAccount
            && *stall_count == 2
            && *freeze_threshold == 2
            && !*freeze_imminent
            && *post_under_maintenance
            && *post_account_frozen
    )));

    engine.update_isolated_collateral(7, 22, 100, 1_005);
    assert!(!engine.account_frozen(22));
    assert!(!engine.margin_snapshot(7, 22).under_maintenance);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::LiquidationEscalationUpdated {
            market_id,
            account_id,
            action,
            stall_count,
            freeze_threshold,
            freeze_imminent,
            post_under_maintenance,
            post_account_frozen,
            ..
        } if *market_id == 7
            && *account_id == 22
            && *action == LiquidationEscalationAction::Reset
            && *stall_count == 0
            && *freeze_threshold == 2
            && !*freeze_imminent
            && !*post_under_maintenance
            && !*post_account_frozen
    )));
}

#[test]
fn single_node_engine_blocks_liquidation_on_stale_mark_data() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);
    let bid = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Bid,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let ask = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 33,
        client_order_id: 2,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 3,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
        time_in_force: kombat_chain_matching::TimeInForce::Ioc,
        post_only: false,
    };

    engine.submit_order(bid, 1_000).expect("bid should rest");
    engine.submit_order(ask, 1_001).expect("ask should fill");
    engine.update_market_index_price(7, 100, 7_002);

    assert_eq!(
        engine.liquidate_position(7, 22, 99, 5, 7_003),
        Err(NodeError::StaleMarketData { market_id: 7 })
    );
}

#[test]
fn single_node_engine_cancels_and_replaces_orders() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let resting = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 5,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    engine
        .submit_order(resting, 1_000)
        .expect("order should rest");

    let replacement = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 7,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 102 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    let outcome = engine
        .replace_order(7, 22, 1, replacement, 1_100)
        .expect("replace should succeed");
    assert_eq!(outcome.status, kombat_chain_matching::MatchStatus::Resting);
    assert_eq!(
        engine
            .resting_order(7, 22, 1, 1_100)
            .expect("replacement should exist")
            .price_ticks,
        102
    );

    let cancelled = engine
        .cancel_order(7, 22, 1, 1_200)
        .expect("cancel should succeed");
    assert_eq!(cancelled.client_order_id, 1);
    assert!(engine.resting_order(7, 22, 1, 1_200).is_none());
}

#[test]
fn single_node_engine_pauses_new_trading_but_allows_cancel() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let resting = kombat_chain_matching::OrderRequest {
        market_id: 7,
        account_id: 22,
        client_order_id: 1,
        side: kombat_chain_matching::OrderSide::Ask,
        quantity_lots: 5,
        kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
        time_in_force: kombat_chain_matching::TimeInForce::Gtc,
        post_only: false,
    };
    engine
        .submit_order(resting, 1_000)
        .expect("order should rest before pause");

    engine.set_market_trading_paused(7, true, 1_050);

    let error = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 1,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_100,
        )
        .expect_err("new orders should be blocked while paused");
    assert_eq!(error, NodeError::MarketTradingPaused { market_id: 7 });

    let cancelled = engine
        .cancel_order(7, 22, 1, 1_200)
        .expect("cancel should still succeed while paused");
    assert_eq!(cancelled.client_order_id, 1);
    assert!(engine.resting_order(7, 22, 1, 1_200).is_none());
}

#[test]
fn single_node_engine_blocks_orders_when_market_data_is_stale() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.update_market_mark_price(7, 103, 1_000);
    engine.update_market_oracle_config(
        7,
        kombat_chain_oracle::OracleConfig {
            max_staleness_ms: 100,
        },
        1_000,
    );

    let error = engine
        .submit_order(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 103 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_101,
        )
        .expect_err("stale mark should block new order submission");
    assert_eq!(error, NodeError::StaleMarketData { market_id: 7 });
}

#[test]
fn single_node_exposes_consensus_metrics_and_logs() {
    let mut engine = SingleNodeEngine::new(single_node_config());

    engine.record_timeout(1_010);
    engine.record_view_change(1_020);
    engine
        .produce_empty_block(1_100)
        .expect("first block should produce");
    engine
        .produce_empty_block(1_200)
        .expect("second block should produce");

    let metrics = engine.metrics();
    assert_eq!(metrics.current_round, 3);
    assert_eq!(metrics.current_view, 2);
    assert_eq!(metrics.last_committed_round, 1);
    assert_eq!(metrics.peer_count, 0);
    assert_eq!(metrics.timeouts_total, 1);
    assert_eq!(metrics.view_changes_total, 1);
    assert_eq!(metrics.qc_formation_ms, vec![100, 100]);
    assert_eq!(metrics.round_duration_ms, vec![100, 100]);
    assert_eq!(metrics.block_size_bytes.len(), 2);
    assert_eq!(
        engine.logs()[0].kind,
        ConsensusLogEventKind::PeerCountUpdated
    );
    assert_eq!(
        engine
            .logs()
            .last()
            .expect("logs should include block events")
            .kind,
        ConsensusLogEventKind::RoundStarted
    );
}

#[test]
fn bootstraps_four_node_cluster() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should bootstrap");

    assert_eq!(cluster.nodes().len(), 4);
    for node in cluster.nodes() {
        assert_eq!(node.bootstrap_peers().peers.len(), 3);
        assert_eq!(node.block_count(), 1);
        assert_eq!(node.metrics().peer_count, 3);
        assert_eq!(node.logs()[0].kind, ConsensusLogEventKind::PeerCountUpdated);
    }
}

#[test]
fn equal_stake_leader_selection_rotates_across_rounds() {
    let validator_set = local_cluster_config().validator_set;
    let leaders: std::collections::BTreeSet<_> = (1..=8)
        .map(|round| {
            select_leader_for_validator_set(round, 7, &validator_set)
                .expect("leader selection should succeed")
        })
        .collect();

    assert!(
        leaders.len() > 1,
        "equal-stake leader selection should rotate across nearby rounds"
    );
}

#[test]
fn four_node_cluster_produces_shared_head() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");

    let first = cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce")
        .clone();
    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();

    assert_eq!(first.header.height, 1);
    assert_eq!(second.header.height, 2);
    assert_eq!(
        second.header.proposer,
        cluster
            .leader_for_round(2)
            .expect("leader selection should succeed")
    );

    let expected_head = block_hash(&second);
    for node in cluster.nodes() {
        assert_eq!(node.block_count(), 3);
        assert_eq!(block_hash(node.head()), expected_head);
        assert_eq!(node.current_round(), 3);
    }
}

#[test]
fn sign_current_head_qc_accepts_known_ancestor_head() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let first = engine
        .produce_empty_block(1_100)
        .expect("first block should produce")
        .clone();
    engine
        .produce_empty_block(1_200)
        .expect("second block should produce");

    let signature = engine
        .sign_current_head_qc(block_hash(&first), first.header.round)
        .expect("ancestor head qc should still be signable");
    assert_eq!(signature.validator, engine.validator_set[0]);
}

#[test]
fn sign_network_block_qc_accepts_block_extending_known_ancestor_parent() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let first = engine
        .produce_empty_block(1_100)
        .expect("first block should produce")
        .clone();
    let second = engine
        .produce_empty_block(1_200)
        .expect("second block should produce")
        .clone();

    let validator_verifiers = engine.validator_verifiers.clone();
    let ancestor_qc = build_quorum_certificate(&first, &engine.consensus_signing_keys)
        .expect("ancestor qc should build");
    let proposal = Block {
        header: BlockHeader {
            chain_id: first.header.chain_id,
            height: first.header.height + 1,
            round: second.header.round + 1,
            proposer: engine.validator_set[0],
            parent_hash: block_hash(&first),
            parent_state_root: first.header.state_root,
            tx_root: body_root(&BlockBody { items: vec![] }),
            event_root: [0; 32],
            state_root: first.header.state_root,
            qc: ancestor_qc,
            timestamp_ms: second.header.timestamp_ms + 100,
        },
        body: BlockBody { items: vec![] },
    };

    let signature = engine
        .sign_network_block_qc(
            &proposal,
            &validator_verifiers,
            engine.validator_set[0],
            1_000,
        )
        .expect("proposal extending known ancestor should still be signable");
    assert_eq!(signature.validator, engine.validator_set[0]);
}

#[test]
fn four_node_cluster_catches_up_lagging_node_before_next_block() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");

    let first = cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce")
        .clone();
    let expected_first_hash = block_hash(&first);

    let lagging = cluster
        .nodes
        .last_mut()
        .expect("cluster should have a lagging node");
    lagging.chain.truncate(1);
    lagging.pacemaker = PacemakerState::new(
        1,
        build_quorum_certificate(lagging.head(), &cluster.consensus_signing_keys)
            .expect("genesis qc should rebuild"),
    );
    lagging.telemetry = super::telemetry::ConsensusTelemetry::new(3, &lagging.pacemaker, 1_000);
    assert_eq!(lagging.block_count(), 1);

    let second = cluster
        .produce_empty_block(1_200)
        .expect("cluster should catch up lagging node before next block")
        .clone();
    let expected_second_hash = block_hash(&second);
    let recovered_node = cluster
        .nodes()
        .last()
        .expect("cluster should still have four nodes");

    assert_eq!(recovered_node.block_count(), 3);
    assert_eq!(block_hash(&recovered_node.chain[1]), expected_first_hash);
    assert_eq!(block_hash(recovered_node.head()), expected_second_hash);
    assert_eq!(
        recovered_node
            .logs()
            .iter()
            .filter(|entry| entry.kind == ConsensusLogEventKind::BlockProposed)
            .count(),
        2
    );
    assert_eq!(recovered_node.metrics().qc_formation_ms.len(), 2);
}

#[test]
fn four_node_cluster_rejects_divergent_node_head() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut divergent_cluster =
        LocalCluster::new(local_cluster_config()).expect("divergent cluster should start");
    let divergent_block = divergent_cluster
        .produce_empty_block(1_150)
        .expect("divergent block should produce")
        .clone();

    let divergent = cluster
        .nodes
        .last_mut()
        .expect("cluster should have a divergent node");
    divergent.chain[1] = divergent_block;

    let err = cluster
        .produce_empty_block(1_200)
        .expect_err("divergent node head should block the next cluster block");
    assert_eq!(
        err,
        NodeError::InvalidBlock(BlockValidationError::InvalidParentHash)
    );
}

#[test]
fn four_node_cluster_catches_up_node_restored_from_stale_snapshot() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let stale_snapshot = cluster
        .nodes()
        .last()
        .expect("cluster should have a restorable node")
        .snapshot();

    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let expected_second_hash = block_hash(&second);

    let restored = super::LocalClusterNode::from_snapshot(stale_snapshot);
    assert_eq!(restored.block_count(), 2);
    assert_eq!(restored.current_round(), 2);
    cluster.nodes[3] = restored;

    let third = cluster
        .produce_empty_block(1_300)
        .expect("cluster should catch up a node restored from a stale snapshot")
        .clone();
    let expected_third_hash = block_hash(&third);
    let recovered_node = cluster
        .nodes()
        .last()
        .expect("cluster should still have four nodes");

    assert_eq!(recovered_node.block_count(), 4);
    assert_eq!(block_hash(&recovered_node.chain[2]), expected_second_hash);
    assert_eq!(block_hash(recovered_node.head()), expected_third_hash);
    assert_eq!(recovered_node.current_round(), 4);
    assert_eq!(
        recovered_node
            .logs()
            .iter()
            .filter(|entry| entry.kind == ConsensusLogEventKind::BlockProposed)
            .count(),
        3
    );
    assert_eq!(recovered_node.metrics().qc_formation_ms.len(), 3);
}

#[test]
fn four_node_cluster_catches_up_node_restarted_from_snapshot_file() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-node-snapshot-{}",
        std::process::id()
    ));
    let snapshot_path = snapshot_dir.join("node-3/node.borsh");
    super::write_snapshot_file(
        &snapshot_path,
        &cluster
            .nodes()
            .last()
            .expect("cluster should have a restorable node")
            .snapshot_bytes(),
    )
    .expect("node snapshot file should write");

    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let expected_second_hash = block_hash(&second);

    let restored = super::LocalClusterNode::from_snapshot_bytes(
        &std::fs::read(&snapshot_path).expect("snapshot bytes should read"),
    )
    .expect("snapshot reload");
    assert_eq!(restored.block_count(), 2);
    assert_eq!(restored.current_round(), 2);
    cluster.nodes[3] = restored;

    let third = cluster
        .produce_empty_block(1_300)
        .expect("cluster should catch up a node restarted from disk snapshot")
        .clone();
    let expected_third_hash = block_hash(&third);
    let recovered_node = cluster
        .nodes()
        .last()
        .expect("cluster should still have four nodes");

    assert_eq!(recovered_node.block_count(), 4);
    assert_eq!(block_hash(&recovered_node.chain[2]), expected_second_hash);
    assert_eq!(block_hash(recovered_node.head()), expected_third_hash);
    assert_eq!(recovered_node.current_round(), 4);

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn four_node_cluster_catches_up_node_restarted_from_multi_block_stale_snapshot_file() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");

    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-node-multi-stale-{}",
        std::process::id()
    ));
    let snapshot_path = snapshot_dir.join("node-3/node.borsh");
    super::write_snapshot_file(
        &snapshot_path,
        &cluster
            .nodes()
            .last()
            .expect("cluster should have a restorable node")
            .snapshot_bytes(),
    )
    .expect("node snapshot file should write");

    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");
    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let second_hash = block_hash(&second);

    let restored = super::LocalClusterNode::from_snapshot_bytes(
        &std::fs::read(&snapshot_path).expect("snapshot bytes should read"),
    )
    .expect("snapshot reload");
    assert_eq!(restored.block_count(), 1);
    assert_eq!(restored.current_round(), 1);
    cluster.nodes[3] = restored;

    let third = cluster
        .produce_empty_block(1_300)
        .expect("cluster should catch up a node restarted from a multi-block stale snapshot")
        .clone();
    let third_hash = block_hash(&third);
    let recovered_node = cluster
        .nodes()
        .last()
        .expect("cluster should still have four nodes");

    assert_eq!(recovered_node.block_count(), 4);
    assert_eq!(block_hash(&recovered_node.chain[2]), second_hash);
    assert_eq!(block_hash(recovered_node.head()), third_hash);
    assert_eq!(recovered_node.current_round(), 4);

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn local_cluster_snapshot_file_roundtrip_preserves_shared_head_and_next_block() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");
    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let second_hash = block_hash(&second);

    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-snapshot-{}-{}.borsh",
        std::process::id(),
        second.header.height
    ));
    cluster
        .save_snapshot_file(&snapshot_path)
        .expect("cluster snapshot file should write");

    let mut restored =
        LocalCluster::load_snapshot_file(&snapshot_path).expect("cluster snapshot reload");
    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");

    assert_eq!(block_hash(restored.head()), second_hash);
    assert_eq!(restored.head().header.height, 2);
    assert!(restored
        .nodes()
        .iter()
        .all(|node| block_hash(node.head()) == second_hash));

    let third = restored
        .produce_empty_block(1_300)
        .expect("cluster should continue after snapshot reload")
        .clone();
    let third_hash = block_hash(&third);
    assert_eq!(third.header.height, 3);
    assert!(restored
        .nodes()
        .iter()
        .all(|node| block_hash(node.head()) == third_hash));
}

#[test]
fn local_cluster_snapshot_file_save_creates_missing_parent_directories() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-snapshot-dir-{}",
        std::process::id()
    ));
    let snapshot_path = snapshot_dir.join("nested/cluster.borsh");

    cluster
        .save_snapshot_file(&snapshot_path)
        .expect("cluster snapshot file should write through missing directories");

    let restored =
        LocalCluster::load_snapshot_file(&snapshot_path).expect("cluster snapshot should reload");
    assert_eq!(block_hash(restored.head()), block_hash(cluster.head()));

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn local_cluster_snapshot_file_load_rejects_missing_file() {
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-snapshot-missing-{}.borsh",
        std::process::id()
    ));
    let err = LocalCluster::load_snapshot_file(&snapshot_path)
        .expect_err("missing cluster snapshot file should not reload");

    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn local_cluster_snapshot_file_load_rejects_truncated_bytes() {
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-snapshot-truncated-{}.borsh",
        std::process::id()
    ));
    std::fs::write(&snapshot_path, [1_u8, 2, 3, 4]).expect("truncated snapshot should write");

    let err = LocalCluster::load_snapshot_file(&snapshot_path)
        .expect_err("truncated cluster snapshot file should not reload");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn local_cluster_snapshot_file_roundtrip_survives_multiple_cycles() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-multi-cycle-{}.borsh",
        std::process::id()
    ));

    for (index, timestamp_ms) in [1_100_u64, 1_200, 1_300].into_iter().enumerate() {
        let produced = cluster
            .produce_empty_block(timestamp_ms)
            .expect("cluster block should produce")
            .clone();
        let expected_hash = block_hash(&produced);
        let expected_height = produced.header.height;
        let expected_round = produced.header.round.saturating_add(1);

        cluster
            .save_snapshot_file(&snapshot_path)
            .expect("cluster snapshot file should write");
        cluster =
            LocalCluster::load_snapshot_file(&snapshot_path).expect("cluster snapshot reload");

        assert_eq!(block_hash(cluster.head()), expected_hash, "cycle {index}");
        assert_eq!(
            cluster.head().header.height,
            expected_height,
            "cycle {index}"
        );
        assert!(
            cluster
                .nodes()
                .iter()
                .all(|node| block_hash(node.head()) == expected_hash),
            "cycle {index}"
        );
        assert!(
            cluster
                .nodes()
                .iter()
                .all(|node| node.current_round() == expected_round),
            "cycle {index}"
        );
    }

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
}

#[test]
fn four_node_cluster_recovers_mixed_lag_and_restart_faults() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let stale_snapshot = cluster
        .nodes()
        .last()
        .expect("cluster should have a restorable node")
        .snapshot();

    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let second_hash = block_hash(&second);

    let lagging = cluster
        .nodes
        .get_mut(2)
        .expect("cluster should have a lagging node");
    lagging.chain.truncate(1);
    lagging.pacemaker = PacemakerState::new(
        1,
        build_quorum_certificate(lagging.head(), &cluster.consensus_signing_keys)
            .expect("genesis qc should rebuild"),
    );
    lagging.telemetry = super::telemetry::ConsensusTelemetry::new(3, &lagging.pacemaker, 1_000);

    let restarted = super::LocalClusterNode::from_snapshot(stale_snapshot);
    assert_eq!(restarted.block_count(), 2);
    cluster.nodes[3] = restarted;

    let third = cluster
        .produce_empty_block(1_300)
        .expect("cluster should recover mixed lag and restart faults")
        .clone();
    let third_hash = block_hash(&third);

    let lagging_node = cluster.nodes().get(2).expect("lagging node should remain");
    let restarted_node = cluster
        .nodes()
        .get(3)
        .expect("restarted node should remain");

    assert_eq!(lagging_node.block_count(), 4);
    assert_eq!(restarted_node.block_count(), 4);
    assert_eq!(block_hash(&lagging_node.chain[2]), second_hash);
    assert_eq!(block_hash(&restarted_node.chain[2]), second_hash);
    assert_eq!(block_hash(lagging_node.head()), third_hash);
    assert_eq!(block_hash(restarted_node.head()), third_hash);
    assert_eq!(lagging_node.current_round(), 4);
    assert_eq!(restarted_node.current_round(), 4);
}

#[test]
fn four_node_cluster_recovers_repeated_mixed_fault_cycles() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");

    for (cycle, base_timestamp_ms) in [(0_u64, 1_100_u64), (1, 1_400_u64)] {
        cluster
            .produce_empty_block(base_timestamp_ms)
            .expect("baseline cluster block should produce");

        let stale_snapshot = cluster
            .nodes()
            .last()
            .expect("cluster should have a restorable node")
            .snapshot();

        let pre_recovery = cluster
            .produce_empty_block(base_timestamp_ms + 100)
            .expect("pre-recovery block should produce")
            .clone();
        let expected_pre_recovery_hash = block_hash(&pre_recovery);

        let lagging = cluster
            .nodes
            .get_mut(2)
            .expect("cluster should have a lagging node");
        lagging.chain.truncate(1);
        lagging.pacemaker = PacemakerState::new(
            1,
            build_quorum_certificate(lagging.head(), &cluster.consensus_signing_keys)
                .expect("genesis qc should rebuild"),
        );
        lagging.telemetry = super::telemetry::ConsensusTelemetry::new(3, &lagging.pacemaker, 1_000);

        cluster.nodes[3] = super::LocalClusterNode::from_snapshot(stale_snapshot);

        let recovered = cluster
            .produce_empty_block(base_timestamp_ms + 200)
            .expect("cluster should recover mixed faults repeatedly")
            .clone();
        let recovered_hash = block_hash(&recovered);

        let lagging_node = cluster.nodes().get(2).expect("lagging node should remain");
        let restarted_node = cluster
            .nodes()
            .get(3)
            .expect("restarted node should remain");

        assert_eq!(
            lagging_node.block_count(),
            recovered.header.height as usize + 1
        );
        assert_eq!(
            restarted_node.block_count(),
            recovered.header.height as usize + 1
        );
        assert_eq!(
            block_hash(&lagging_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(&restarted_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(lagging_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(restarted_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
    }
}

#[test]
fn four_node_cluster_repair_from_snapshot_file_clears_divergent_node() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let first = cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce")
        .clone();
    let first_hash = block_hash(&first);

    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-divergent-repair-{}",
        std::process::id()
    ));
    let snapshot_path = snapshot_dir.join("node-3/node.borsh");
    super::write_snapshot_file(
        &snapshot_path,
        &cluster
            .nodes()
            .last()
            .expect("cluster should have a restorable node")
            .snapshot_bytes(),
    )
    .expect("node snapshot file should write");

    let mut divergent_cluster =
        LocalCluster::new(local_cluster_config()).expect("divergent cluster should start");
    let divergent_block = divergent_cluster
        .produce_empty_block(1_150)
        .expect("divergent block should produce")
        .clone();
    let divergent_node = cluster
        .nodes
        .last_mut()
        .expect("cluster should have a divergent node");
    divergent_node.chain[1] = divergent_block;

    let repaired = super::LocalClusterNode::from_snapshot_bytes(
        &std::fs::read(&snapshot_path).expect("snapshot bytes should read"),
    )
    .expect("snapshot reload");
    cluster.nodes[3] = repaired;

    let second = cluster
        .produce_empty_block(1_200)
        .expect("cluster should recover after repairing divergent node from snapshot")
        .clone();
    let second_hash = block_hash(&second);
    let repaired_node = cluster
        .nodes()
        .last()
        .expect("cluster should keep four nodes");

    assert_eq!(block_hash(&repaired_node.chain[1]), first_hash);
    assert_eq!(block_hash(repaired_node.head()), second_hash);
    assert_eq!(repaired_node.current_round(), 3);

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn local_cluster_snapshot_reload_recovers_mixed_fault_state() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let stale_snapshot = cluster
        .nodes()
        .last()
        .expect("cluster should have a restorable node")
        .snapshot();

    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let second_hash = block_hash(&second);

    let lagging = cluster
        .nodes
        .get_mut(2)
        .expect("cluster should have a lagging node");
    lagging.chain.truncate(1);
    lagging.pacemaker = PacemakerState::new(
        1,
        build_quorum_certificate(lagging.head(), &cluster.consensus_signing_keys)
            .expect("genesis qc should rebuild"),
    );
    lagging.telemetry = super::telemetry::ConsensusTelemetry::new(3, &lagging.pacemaker, 1_000);
    cluster.nodes[3] = super::LocalClusterNode::from_snapshot(stale_snapshot);

    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-degraded-reload-{}.borsh",
        std::process::id()
    ));
    cluster
        .save_snapshot_file(&snapshot_path)
        .expect("degraded cluster snapshot should write");

    let mut restored =
        LocalCluster::load_snapshot_file(&snapshot_path).expect("degraded snapshot reload");
    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");

    let third = restored
        .produce_empty_block(1_300)
        .expect("restored degraded cluster should recover and produce")
        .clone();
    let third_hash = block_hash(&third);

    let lagging_node = restored.nodes().get(2).expect("lagging node should remain");
    let restarted_node = restored
        .nodes()
        .get(3)
        .expect("restarted node should remain");

    assert_eq!(block_hash(&lagging_node.chain[2]), second_hash);
    assert_eq!(block_hash(&restarted_node.chain[2]), second_hash);
    assert_eq!(block_hash(lagging_node.head()), third_hash);
    assert_eq!(block_hash(restarted_node.head()), third_hash);
    assert_eq!(lagging_node.current_round(), 4);
    assert_eq!(restarted_node.current_round(), 4);
}

#[test]
fn local_cluster_snapshot_reload_recovers_two_stale_nodes() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");

    let stale_two = cluster
        .nodes()
        .get(2)
        .expect("cluster should have a third node")
        .snapshot();
    let stale_three = cluster
        .nodes()
        .get(3)
        .expect("cluster should have a fourth node")
        .snapshot();

    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");
    let second = cluster
        .produce_empty_block(1_200)
        .expect("second cluster block should produce")
        .clone();
    let second_hash = block_hash(&second);

    cluster.nodes[2] = super::LocalClusterNode::from_snapshot(stale_two);
    cluster.nodes[3] = super::LocalClusterNode::from_snapshot(stale_three);

    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-two-stale-reload-{}.borsh",
        std::process::id()
    ));
    cluster
        .save_snapshot_file(&snapshot_path)
        .expect("two-stale cluster snapshot should write");

    let mut restored =
        LocalCluster::load_snapshot_file(&snapshot_path).expect("two-stale snapshot reload");
    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");

    let third = restored
        .produce_empty_block(1_300)
        .expect("restored cluster should catch up both stale nodes")
        .clone();
    let third_hash = block_hash(&third);

    for index in [2_usize, 3] {
        let node = restored
            .nodes()
            .get(index)
            .expect("stale node should remain");
        assert_eq!(node.block_count(), 4, "node {index}");
        assert_eq!(block_hash(&node.chain[2]), second_hash, "node {index}");
        assert_eq!(block_hash(node.head()), third_hash, "node {index}");
        assert_eq!(node.current_round(), 4, "node {index}");
    }
}

#[test]
fn local_cluster_snapshot_reload_survives_repeated_degraded_cycles() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-degraded-multi-cycle-{}.borsh",
        std::process::id()
    ));

    for (cycle, base_timestamp_ms) in [(0_u64, 1_100_u64), (1, 1_500_u64)] {
        cluster
            .produce_empty_block(base_timestamp_ms)
            .expect("baseline cluster block should produce");

        let stale_snapshot = cluster
            .nodes()
            .last()
            .expect("cluster should have a restorable node")
            .snapshot();

        let pre_recovery = cluster
            .produce_empty_block(base_timestamp_ms + 100)
            .expect("pre-recovery block should produce")
            .clone();
        let expected_pre_recovery_hash = block_hash(&pre_recovery);

        let lagging = cluster
            .nodes
            .get_mut(2)
            .expect("cluster should have a lagging node");
        lagging.chain.truncate(1);
        lagging.pacemaker = PacemakerState::new(
            1,
            build_quorum_certificate(lagging.head(), &cluster.consensus_signing_keys)
                .expect("genesis qc should rebuild"),
        );
        lagging.telemetry = super::telemetry::ConsensusTelemetry::new(3, &lagging.pacemaker, 1_000);

        cluster.nodes[3] = super::LocalClusterNode::from_snapshot(stale_snapshot);

        cluster
            .save_snapshot_file(&snapshot_path)
            .expect("degraded cluster snapshot should write");
        cluster =
            LocalCluster::load_snapshot_file(&snapshot_path).expect("degraded reload should work");

        let recovered = cluster
            .produce_empty_block(base_timestamp_ms + 200)
            .expect("reloaded degraded cluster should recover")
            .clone();
        let recovered_hash = block_hash(&recovered);

        let lagging_node = cluster.nodes().get(2).expect("lagging node should remain");
        let restarted_node = cluster
            .nodes()
            .get(3)
            .expect("restarted node should remain");

        assert_eq!(
            block_hash(&lagging_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(&restarted_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(lagging_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(restarted_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            lagging_node.block_count(),
            recovered.header.height as usize + 1,
            "cycle {cycle}"
        );
        assert_eq!(
            restarted_node.block_count(),
            recovered.header.height as usize + 1,
            "cycle {cycle}"
        );
        assert_eq!(
            lagging_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
        assert_eq!(
            restarted_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
    }

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
}

#[test]
fn local_cluster_snapshot_reload_survives_repeated_two_stale_cycles() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-cluster-two-stale-multi-cycle-{}.borsh",
        std::process::id()
    ));

    for (cycle, base_timestamp_ms) in [(0_u64, 1_100_u64), (1, 1_500_u64)] {
        let stale_two = cluster
            .nodes()
            .get(2)
            .expect("cluster should have a third node")
            .snapshot();
        let stale_three = cluster
            .nodes()
            .get(3)
            .expect("cluster should have a fourth node")
            .snapshot();

        cluster
            .produce_empty_block(base_timestamp_ms)
            .expect("baseline cluster block should produce");
        let pre_recovery = cluster
            .produce_empty_block(base_timestamp_ms + 100)
            .expect("pre-recovery block should produce")
            .clone();
        let expected_pre_recovery_hash = block_hash(&pre_recovery);

        cluster.nodes[2] = super::LocalClusterNode::from_snapshot(stale_two);
        cluster.nodes[3] = super::LocalClusterNode::from_snapshot(stale_three);

        cluster
            .save_snapshot_file(&snapshot_path)
            .expect("two-stale degraded snapshot should write");
        cluster = LocalCluster::load_snapshot_file(&snapshot_path)
            .expect("two-stale degraded reload should work");

        let recovered = cluster
            .produce_empty_block(base_timestamp_ms + 200)
            .expect("reloaded two-stale cluster should recover")
            .clone();
        let recovered_hash = block_hash(&recovered);

        for index in [2_usize, 3] {
            let node = cluster
                .nodes()
                .get(index)
                .expect("stale node should remain");
            assert_eq!(
                block_hash(&node.chain[(recovered.header.height - 1) as usize]),
                expected_pre_recovery_hash,
                "cycle {cycle} node {index}"
            );
            assert_eq!(
                block_hash(node.head()),
                recovered_hash,
                "cycle {cycle} node {index}"
            );
            assert_eq!(
                node.block_count(),
                recovered.header.height as usize + 1,
                "cycle {cycle} node {index}"
            );
            assert_eq!(
                node.current_round(),
                recovered.header.round.saturating_add(1),
                "cycle {cycle} node {index}"
            );
        }
    }

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
}

#[test]
fn four_node_cluster_recovers_repeated_divergent_snapshot_repairs() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-divergent-multi-cycle-{}",
        std::process::id()
    ));
    let snapshot_path = snapshot_dir.join("node-3/node.borsh");

    for (cycle, base_timestamp_ms) in [(0_u64, 1_100_u64), (1, 1_500_u64)] {
        let stable = cluster
            .produce_empty_block(base_timestamp_ms)
            .expect("baseline cluster block should produce")
            .clone();

        super::write_snapshot_file(
            &snapshot_path,
            &cluster
                .nodes()
                .last()
                .expect("cluster should have a restorable node")
                .snapshot_bytes(),
        )
        .expect("node snapshot file should write");

        let pre_repair = cluster
            .produce_empty_block(base_timestamp_ms + 100)
            .expect("pre-repair block should produce")
            .clone();
        let pre_repair_hash = block_hash(&pre_repair);

        let mut divergent_block = pre_repair.clone();
        divergent_block.header.timestamp_ms =
            divergent_block.header.timestamp_ms.saturating_add(50);

        let lagging = cluster
            .nodes
            .get_mut(2)
            .expect("cluster should have a lagging node");
        lagging.chain.truncate(stable.header.height as usize + 1);
        lagging.pacemaker = PacemakerState::new(
            stable.header.round.saturating_add(1),
            build_quorum_certificate(lagging.head(), &cluster.consensus_signing_keys)
                .expect("stable qc should rebuild"),
        );
        lagging.telemetry = super::telemetry::ConsensusTelemetry::new(
            3,
            &lagging.pacemaker,
            stable.header.timestamp_ms,
        );

        let divergent_node = cluster
            .nodes
            .last_mut()
            .expect("cluster should have a divergent node");
        divergent_node.chain[pre_repair.header.height as usize] = divergent_block;

        let repaired = super::LocalClusterNode::from_snapshot_bytes(
            &std::fs::read(&snapshot_path).expect("snapshot bytes should read"),
        )
        .expect("snapshot reload");
        cluster.nodes[3] = repaired;

        let recovered = cluster
            .produce_empty_block(base_timestamp_ms + 200)
            .expect("cluster should recover mixed divergence and restart faults")
            .clone();
        let recovered_hash = block_hash(&recovered);

        let lagging_node = cluster.nodes().get(2).expect("lagging node should remain");
        let repaired_node = cluster.nodes().get(3).expect("repaired node should remain");

        assert_eq!(
            block_hash(&lagging_node.chain[(recovered.header.height - 1) as usize]),
            pre_repair_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(&repaired_node.chain[(recovered.header.height - 1) as usize]),
            pre_repair_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(lagging_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(repaired_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            lagging_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
        assert_eq!(
            repaired_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
    }

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn local_cluster_snapshot_reload_survives_repeated_lagging_and_repaired_cycles() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-lagging-repaired-multi-cycle-{}",
        std::process::id()
    ));
    let node_snapshot_path = snapshot_dir.join("node-3/node.borsh");
    let cluster_snapshot_path = snapshot_dir.join("cluster.borsh");

    for (cycle, base_timestamp_ms) in [(0_u64, 1_100_u64), (1, 1_500_u64)] {
        let _stable = cluster
            .produce_empty_block(base_timestamp_ms)
            .expect("baseline cluster block should produce")
            .clone();
        let lagging_snapshot = cluster
            .nodes()
            .get(2)
            .expect("cluster should have a lagging node")
            .snapshot();

        super::write_snapshot_file(
            &node_snapshot_path,
            &cluster
                .nodes()
                .last()
                .expect("cluster should have a restorable node")
                .snapshot_bytes(),
        )
        .expect("node snapshot file should write");

        let pre_recovery = cluster
            .produce_empty_block(base_timestamp_ms + 100)
            .expect("pre-recovery block should produce")
            .clone();
        let expected_pre_recovery_hash = block_hash(&pre_recovery);

        let mut divergent_block = pre_recovery.clone();
        divergent_block.header.timestamp_ms =
            divergent_block.header.timestamp_ms.saturating_add(50);

        cluster.nodes[2] = super::LocalClusterNode::from_snapshot(lagging_snapshot);
        let divergent_node = cluster
            .nodes
            .last_mut()
            .expect("cluster should have a divergent node");
        divergent_node.chain[pre_recovery.header.height as usize] = divergent_block;

        cluster.nodes[3] = super::LocalClusterNode::from_snapshot_bytes(
            &std::fs::read(&node_snapshot_path).expect("snapshot bytes should read"),
        )
        .expect("snapshot reload");

        cluster
            .save_snapshot_file(&cluster_snapshot_path)
            .expect("degraded cluster snapshot should write");
        cluster = LocalCluster::load_snapshot_file(&cluster_snapshot_path)
            .expect("degraded cluster reload should work");

        let recovered = cluster
            .produce_empty_block(base_timestamp_ms + 200)
            .expect("reloaded lagging and repaired cluster should recover")
            .clone();
        let recovered_hash = block_hash(&recovered);

        let lagging_node = cluster.nodes().get(2).expect("lagging node should remain");
        let repaired_node = cluster.nodes().get(3).expect("repaired node should remain");

        assert_eq!(
            block_hash(&lagging_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(&repaired_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(lagging_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(repaired_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            lagging_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
        assert_eq!(
            repaired_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
    }

    std::fs::remove_file(&cluster_snapshot_path).expect("cluster snapshot should clean up");
    std::fs::remove_file(&node_snapshot_path).expect("node snapshot should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn local_cluster_snapshot_reload_survives_endurance_mixed_fault_cycles() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-cluster-lagging-repaired-endurance-{}",
        std::process::id()
    ));
    let node_snapshot_path = snapshot_dir.join("node-3/node.borsh");
    let cluster_snapshot_path = snapshot_dir.join("cluster.borsh");

    for (cycle, base_timestamp_ms) in [
        (0_u64, 1_100_u64),
        (1, 1_500_u64),
        (2, 1_900_u64),
        (3, 2_300_u64),
    ] {
        let _stable = cluster
            .produce_empty_block(base_timestamp_ms)
            .expect("baseline cluster block should produce")
            .clone();
        let lagging_snapshot = cluster
            .nodes()
            .get(2)
            .expect("cluster should have a lagging node")
            .snapshot();

        super::write_snapshot_file(
            &node_snapshot_path,
            &cluster
                .nodes()
                .last()
                .expect("cluster should have a restorable node")
                .snapshot_bytes(),
        )
        .expect("node snapshot file should write");

        let pre_recovery = cluster
            .produce_empty_block(base_timestamp_ms + 100)
            .expect("pre-recovery block should produce")
            .clone();
        let expected_pre_recovery_hash = block_hash(&pre_recovery);

        let mut divergent_block = pre_recovery.clone();
        divergent_block.header.timestamp_ms =
            divergent_block.header.timestamp_ms.saturating_add(50);

        cluster.nodes[2] = super::LocalClusterNode::from_snapshot(lagging_snapshot);
        let divergent_node = cluster
            .nodes
            .last_mut()
            .expect("cluster should have a divergent node");
        divergent_node.chain[pre_recovery.header.height as usize] = divergent_block;

        cluster.nodes[3] = super::LocalClusterNode::from_snapshot_bytes(
            &std::fs::read(&node_snapshot_path).expect("snapshot bytes should read"),
        )
        .expect("snapshot reload");

        cluster
            .save_snapshot_file(&cluster_snapshot_path)
            .expect("degraded cluster snapshot should write");
        cluster = LocalCluster::load_snapshot_file(&cluster_snapshot_path)
            .expect("degraded cluster reload should work");

        let recovered = cluster
            .produce_empty_block(base_timestamp_ms + 200)
            .expect("reloaded endurance cluster should recover")
            .clone();
        let recovered_hash = block_hash(&recovered);

        let lagging_node = cluster.nodes().get(2).expect("lagging node should remain");
        let repaired_node = cluster.nodes().get(3).expect("repaired node should remain");

        assert_eq!(
            block_hash(&lagging_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(&repaired_node.chain[(recovered.header.height - 1) as usize]),
            expected_pre_recovery_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(lagging_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            block_hash(repaired_node.head()),
            recovered_hash,
            "cycle {cycle}"
        );
        assert_eq!(
            lagging_node.block_count(),
            recovered.header.height as usize + 1,
            "cycle {cycle}"
        );
        assert_eq!(
            repaired_node.block_count(),
            recovered.header.height as usize + 1,
            "cycle {cycle}"
        );
        assert_eq!(
            lagging_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
        assert_eq!(
            repaired_node.current_round(),
            recovered.header.round.saturating_add(1),
            "cycle {cycle}"
        );
    }

    std::fs::remove_file(&cluster_snapshot_path).expect("cluster snapshot should clean up");
    std::fs::remove_file(&node_snapshot_path).expect("node snapshot should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn local_cluster_snapshot_reload_rejects_node_count_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    snapshot.node_snapshots.pop();

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject missing nodes");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("node count mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_node_chain_id_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.chain[0].header.chain_id = 999;
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject wrong node chain id");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn local_cluster_snapshot_reload_rejects_signing_key_material_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let validator_id = snapshot
        .validator_set
        .active_validators()
        .first()
        .expect("cluster should have validators")
        .validator_id;
    snapshot
        .consensus_signing_keys
        .insert(validator_id, [77; 32]);

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject mismatched signing key material");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("signing key material mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_verifier_config_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    snapshot.validator_verifiers[0].consensus_pubkey = [88; 32];

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject mismatched verifier config");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("verifier config mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_verifier_count_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    snapshot.validator_verifiers.pop();

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject verifier count mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("verifier count mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_signing_key_set_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let validator_id = snapshot
        .validator_set
        .active_validators()
        .first()
        .expect("cluster should have validators")
        .validator_id;
    snapshot.consensus_signing_keys.remove(&validator_id);

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject signing key set mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("signing keys do not match active validators"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_active_validator_count_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    snapshot.validator_set.validators[3].enabled = false;

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject too few active validators");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("requires 4 active validators"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_local_validator_config_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot
        .bootstrap_peers
        .local
        .operator_name
        .push_str("-tampered");
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject tampered local validator config");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("local validator config mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_peer_set_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.bootstrap_peers.peers.pop();
    node_snapshot.telemetry.metrics_mut().peer_count = 2;
    for entry in node_snapshot.telemetry.logs_mut() {
        entry.peer_count = 2;
    }
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject tampered peer set");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("peer set mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_peer_config_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.bootstrap_peers.peers[0]
        .rpc_endpoints
        .push("https://tampered.example".into());
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject tampered peer config");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("peer config mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_node_telemetry_round_mismatch() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.telemetry.metrics_mut().last_committed_round = 99;
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded node telemetry round mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("last committed round mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_node_telemetry_validator_mismatch() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.telemetry.logs_mut()[1].validator_id = Some(validator(9));
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded node telemetry validator mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("log validator mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_local_validator_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.validator_id = validator(9);
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded local validator mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("local validator mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_peer_set_with_local_validator() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot
        .bootstrap_peers
        .peers
        .push(super::ValidatorPeer::from(
            &node_snapshot.bootstrap_peers.local,
        ));
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded peer set containing local validator");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("peers contain local validator"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_unknown_validator_id() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    let unknown_validator_id = validator(9);
    node_snapshot.validator_id = unknown_validator_id;
    node_snapshot.bootstrap_peers.local.validator_id = unknown_validator_id;
    for entry in node_snapshot.telemetry.logs_mut().iter_mut().skip(1) {
        entry.validator_id = Some(unknown_validator_id);
    }
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded unknown validator id");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("unknown validator id"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_duplicate_validator_id() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let duplicate_validator = snapshot
        .validator_set
        .active_validators()
        .first()
        .expect("cluster should have an active validator")
        .clone();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[1])
            .expect("node snapshot should deserialize");
    node_snapshot.validator_id = duplicate_validator.validator_id;
    node_snapshot.bootstrap_peers.local = duplicate_validator;
    node_snapshot
        .bootstrap_peers
        .peers
        .retain(|peer| peer.validator_id != node_snapshot.validator_id);
    let peer_count = node_snapshot.bootstrap_peers.peers.len();
    node_snapshot.telemetry.metrics_mut().peer_count = peer_count;
    for entry in node_snapshot.telemetry.logs_mut() {
        entry.peer_count = peer_count;
        if entry.kind != super::ConsensusLogEventKind::PeerCountUpdated {
            entry.validator_id = Some(node_snapshot.validator_id);
        }
    }
    snapshot.node_snapshots[1] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded duplicate validator id");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("duplicate validator id"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_highest_qc_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.pacemaker.highest_qc.block_hash = [77; 32];
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded highest qc mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("highest qc mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_telemetry_peer_count_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.telemetry.metrics_mut().peer_count = 99;
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded telemetry peer count mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("peer count mismatch"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_empty_chain() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.chain.clear();
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded empty chain");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("requires a genesis block"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_embedded_round_regression() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.pacemaker.current_round = 0;
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject embedded round regression");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("round regression"));
}

#[test]
fn local_cluster_snapshot_reload_rejects_unexpected_proposer() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut snapshot = cluster.snapshot();
    let mut node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    node_snapshot.chain[1].header.proposer = validator(4);
    snapshot.node_snapshots[0] =
        borsh::to_vec(&node_snapshot).expect("node snapshot should serialize");

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject unexpected proposer");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn local_cluster_snapshot_reload_rejects_node_prefix_mismatch() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut divergent_cluster =
        LocalCluster::new(local_cluster_config()).expect("divergent cluster should start");
    let divergent_block = divergent_cluster
        .produce_empty_block(1_150)
        .expect("divergent block should produce")
        .clone();

    let mut snapshot = cluster.snapshot();
    let original_node_snapshot =
        super::LocalClusterNodeSnapshot::try_from_slice(&snapshot.node_snapshots[0])
            .expect("node snapshot should deserialize");
    assert_ne!(
        block_hash(
            original_node_snapshot
                .chain
                .last()
                .expect("node snapshot should have a head")
        ),
        block_hash(&divergent_block)
    );
    snapshot.node_snapshots[0] = divergent_cluster
        .nodes()
        .first()
        .expect("divergent cluster should have a node")
        .snapshot_bytes();

    let err = LocalCluster::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("cluster reload should reject divergent node prefix");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("node prefix mismatch"), "{err}");
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_empty_chain() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = super::LocalClusterNodeSnapshot::try_from_slice(
        &cluster
            .nodes()
            .first()
            .expect("cluster should have a node")
            .snapshot_bytes(),
    )
    .expect("node snapshot should deserialize");
    snapshot.chain.clear();

    let err = super::LocalClusterNode::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("node snapshot should serialize"),
    )
    .expect_err("node reload should reject empty chain");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("requires a genesis block"));
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_local_validator_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster
        .nodes()
        .first()
        .expect("cluster should have a node")
        .snapshot();
    snapshot.bootstrap_peers.local.validator_id = validator(99);

    let err = super::LocalClusterNode::try_from_snapshot(snapshot)
        .expect_err("node snapshot reload should reject local validator mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("local validator mismatch"));
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_peer_set_with_local_validator() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster
        .nodes()
        .first()
        .expect("cluster should have a node")
        .snapshot();
    snapshot
        .bootstrap_peers
        .peers
        .push(kombat_chain_network::ValidatorPeer::from(
            &snapshot.bootstrap_peers.local,
        ));

    let err = super::LocalClusterNode::try_from_snapshot(snapshot)
        .expect_err("node snapshot reload should reject local validator in peer set");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("peers contain local validator"));
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_highest_qc_mismatch() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut snapshot = super::LocalClusterNodeSnapshot::try_from_slice(
        &cluster
            .nodes()
            .first()
            .expect("cluster should have a node")
            .snapshot_bytes(),
    )
    .expect("node snapshot should deserialize");
    snapshot.pacemaker.highest_qc.view = 0;

    let err = super::LocalClusterNode::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("node snapshot should serialize"),
    )
    .expect_err("node reload should reject highest qc mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("highest qc mismatch"));
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_round_regression() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let block = cluster
        .produce_empty_block(1_100)
        .expect("cluster block should produce")
        .clone();

    let mut snapshot = super::LocalClusterNodeSnapshot::try_from_slice(
        &cluster
            .nodes()
            .first()
            .expect("cluster should have a node")
            .snapshot_bytes(),
    )
    .expect("node snapshot should deserialize");
    snapshot.pacemaker.current_round = block.header.round;

    let err = super::LocalClusterNode::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("node snapshot should serialize"),
    )
    .expect_err("node reload should reject round regression");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("round regression"));
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_telemetry_validator_mismatch() {
    let mut cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    cluster
        .produce_empty_block(1_100)
        .expect("first cluster block should produce");

    let mut snapshot = super::LocalClusterNodeSnapshot::try_from_slice(
        &cluster
            .nodes()
            .first()
            .expect("cluster should have a node")
            .snapshot_bytes(),
    )
    .expect("node snapshot should deserialize");
    snapshot.telemetry.logs_mut()[1].validator_id = Some(validator(9));

    let err = super::LocalClusterNode::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("node snapshot should serialize"),
    )
    .expect_err("node reload should reject telemetry validator mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("log validator mismatch"));
}

#[test]
fn rejects_cluster_without_four_active_validators() {
    let mut config = local_cluster_config();
    config.validator_set.validators[3].enabled = false;

    let err = LocalCluster::new(config).expect_err("cluster should reject three validators");

    assert_eq!(
        err,
        NodeError::InvalidClusterSize {
            expected: 4,
            actual: 3,
        }
    );
}

mod runtime;
mod paper_differential;
