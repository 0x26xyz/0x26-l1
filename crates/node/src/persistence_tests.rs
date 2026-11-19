use super::*;
use crate::migrations::legacy::rfc056_policy_authority_scope::{
    migrate_rfc056_policy_authority_scope_record, LegacyRfc056EngineCanonicalRecord,
    LegacyRfc056PersistedSingleNodeEngineSnapshot, LegacyRfc056PolicyRecord,
    LegacyRfc056PolicyStore,
};
use crate::{
    AccountEquitySample, MandateConformanceSample, MandateState, SingleNodeConfig, SingleNodeEngine,
};
use kombat_chain_consensus::{mahi::MahiBlock, PacemakerConfig, ValidatorId};
use kombat_chain_policy::{PolicyExecutionScope, PolicyRecord, PolicyState, PolicyTerms};
use kombat_chain_state_db::{InMemoryBackend, OpenOptions, RedbBackend, StateBackend};
use tempfile::TempDir;

fn validator(id: u8) -> ValidatorId {
    let mut v = [0u8; 32];
    v[0] = id;
    v
}

fn pacemaker_config() -> PacemakerConfig {
    PacemakerConfig {
        base_timeout_ms: 300,
        max_timeout_ms: 20_000,
        max_exponent: 6,
        max_clock_skew_ms: 1_000,
    }
}

fn config() -> SingleNodeConfig {
    SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(9),
        consensus_signing_key: [9; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker_config(),
    }
}

/// `PersistedSingleNodeEngineSnapshot` doesn't implement `PartialEq`,
/// so we compare by Borsh-serialised bytes. Equal bytes mean equal
/// state for hydration purposes.
fn snapshot_bytes(engine: &SingleNodeEngine) -> Vec<u8> {
    borsh::to_vec(&engine.persisted_snapshot()).expect("encode snapshot")
}

fn assert_snapshot_bytes_eq(left: &SingleNodeEngine, right: &SingleNodeEngine) {
    let left_snapshot = left.persisted_snapshot();
    let right_snapshot = right.persisted_snapshot();
    let left_bytes = borsh::to_vec(&left_snapshot).expect("encode left snapshot");
    let right_bytes = borsh::to_vec(&right_snapshot).expect("encode right snapshot");
    if left_bytes != right_bytes {
        let left_debug = format!("{left_snapshot:#?}");
        let right_debug = format!("{right_snapshot:#?}");
        let left_lines = left_debug.lines().collect::<Vec<_>>();
        let right_lines = right_debug.lines().collect::<Vec<_>>();
        for (idx, (left_line, right_line)) in left_lines.iter().zip(right_lines.iter()).enumerate()
        {
            if left_line != right_line {
                let start = idx.saturating_sub(4);
                let end = (idx + 5).min(left_lines.len()).min(right_lines.len());
                let mut context = String::new();
                for line_idx in start..end {
                    context.push_str(&format!(
                        "\n{line_idx}: L={} | R={}",
                        left_lines[line_idx], right_lines[line_idx]
                    ));
                }
                panic!(
                        "snapshot mismatch at debug line {idx}\nleft:  {left_line}\nright: {right_line}\nleft_head_round={} right_head_round={} left_pacemaker_round={} right_pacemaker_round={} left_snapshot_round={} right_snapshot_round={}{}",
                        left.head().header.round,
                        right.head().header.round,
                        left.pacemaker.current_round,
                        right.pacemaker.current_round,
                        left_snapshot.pacemaker.current_round,
                        right_snapshot.pacemaker.current_round,
                        context,
                    );
            }
        }
        panic!(
            "snapshot byte mismatch with identical debug output: left={} right={}",
            left_bytes.len(),
            right_bytes.len()
        );
    }
}

fn legacy_rfc056_canonical_bytes(engine: &SingleNodeEngine) -> Vec<u8> {
    let snapshot = engine.persisted_snapshot();
    let mut legacy_policies = BTreeMap::new();
    for policy in snapshot.policy_store.records() {
        legacy_policies.insert(
            policy.id,
            LegacyRfc056PolicyRecord {
                id: policy.id,
                account: policy.authority_account,
                parent_policy_id: policy.parent_policy_id,
                delegation_depth: policy.delegation_depth,
                key_state: policy.key_state.clone(),
                next_intent_nonce: policy.next_intent_nonce,
                created_at: policy.created_at,
                expires_at: policy.expires_at,
                state: policy.state,
                terms: policy.terms.clone(),
                admission_rules: policy.admission_rules.clone(),
                origin: policy.origin.clone(),
                reserved: policy.reserved.clone(),
                counters: policy.counters.clone(),
                revoked_at: policy.revoked_at,
            },
        );
    }

    let legacy = LegacyRfc056PersistedSingleNodeEngineSnapshot {
        configured_validator_verifiers: snapshot.configured_validator_verifiers,
        validator_set: snapshot.validator_set,
        validator_verifiers: snapshot.validator_verifiers,
        consensus_signing_keys: snapshot.consensus_signing_keys,
        pacemaker_config: snapshot.pacemaker_config,
        pacemaker: snapshot.pacemaker,
        chain: snapshot.chain,
        committed_state_root: snapshot.committed_state_root,
        committed_entries: snapshot.committed_entries,
        quarantined_blocks: snapshot.quarantined_blocks,
        account_store: snapshot.account_store,
        policy_store: LegacyRfc056PolicyStore {
            policies: legacy_policies,
        },
        order_books: snapshot.order_books,
        resting_order_mandates: snapshot.resting_order_mandates,
        markets: snapshot.markets,
        market_marks: snapshot.market_marks,
        market_index_prices: snapshot.market_index_prices,
        oracle_feed_submissions: snapshot.oracle_feed_submissions,
        mandates: snapshot.mandates,
        mandates_by_agent: snapshot.mandates_by_agent,
        mandate_action_nonces: snapshot.mandate_action_nonces,
        mandate_realized_pnl: snapshot.mandate_realized_pnl,
        mandate_daily_pnl_baselines: snapshot.mandate_daily_pnl_baselines,
        account_operators: snapshot.account_operators,
        agent_identities: snapshot.agent_identities,
        agent_identity_tombstones: snapshot.agent_identity_tombstones,
        identity_action_nonces: snapshot.identity_action_nonces,
        identity_attestor_nonces: snapshot.identity_attestor_nonces,
        agent_attestations: snapshot.agent_attestations,
        agent_attestations_by_account: snapshot.agent_attestations_by_account,
        tee_attestations_by_vendor_root: snapshot.tee_attestations_by_vendor_root,
        tee_vendor_roots: snapshot.tee_vendor_roots,
        tee_vendor_cap_profiles: snapshot.tee_vendor_cap_profiles,
        market_maker_programs: snapshot.market_maker_programs,
        market_maker_enrollments: snapshot.market_maker_enrollments,
        live_market_maker_bonds_micro_usdc: snapshot.live_market_maker_bonds_micro_usdc,
        processed_market_maker_program_actions: snapshot.processed_market_maker_program_actions,
        pending_market_maker_program_actions: snapshot.pending_market_maker_program_actions,
        pending_market_maker_program_action_order: snapshot
            .pending_market_maker_program_action_order,
        processed_tee_governance_actions: snapshot.processed_tee_governance_actions,
        pending_tee_governance_actions: snapshot.pending_tee_governance_actions,
        pending_tee_governance_action_order: snapshot.pending_tee_governance_action_order,
        scheduled_tee_vendor_cap_profile_updates: snapshot.scheduled_tee_vendor_cap_profile_updates,
        processed_real_capital_governance_actions: snapshot
            .processed_real_capital_governance_actions,
        pending_real_capital_governance_actions: snapshot.pending_real_capital_governance_actions,
        pending_real_capital_governance_action_order: snapshot
            .pending_real_capital_governance_action_order,
        pending_faucet_grants: snapshot.pending_faucet_grants,
        pending_faucet_grant_order: snapshot.pending_faucet_grant_order,
        delegations: snapshot.delegations,
        delegation_shares: snapshot.delegation_shares,
        real_capital_listings: snapshot.real_capital_listings,
        real_capital_listings_by_agent: snapshot.real_capital_listings_by_agent,
        vault_deposited_micro_usdc: snapshot.vault_deposited_micro_usdc,
        vault_principal_deposited_micro_usdc: snapshot.vault_principal_deposited_micro_usdc,
        delegation_compensation_claims: snapshot.delegation_compensation_claims,
        delegation_termination_payout_weights: snapshot.delegation_termination_payout_weights,
        real_capital_bond_escrow_micro_usdc: snapshot.real_capital_bond_escrow_micro_usdc,
        real_capital_compensation_escrow_micro_usdc: snapshot
            .real_capital_compensation_escrow_micro_usdc,
        real_capital_insurance_wallet_micro_usdc: snapshot.real_capital_insurance_wallet_micro_usdc,
        real_capital_insurance_fund_micro_usdc: snapshot.real_capital_insurance_fund_micro_usdc,
        protocol_delegation_cap: snapshot.protocol_delegation_cap,
        listing_control_state: snapshot.listing_control_state,
        redemption_requests: snapshot.redemption_requests,
        processed_delegation_actions: snapshot.processed_delegation_actions,
        pending_delegation_actions: snapshot.pending_delegation_actions,
        pending_delegation_action_order: snapshot.pending_delegation_action_order,
        market_oracle_config: snapshot.market_oracle_config,
        market_risk: snapshot.market_risk,
        market_fees: snapshot.market_fees,
        paused_markets: snapshot.paused_markets,
        market_funding_index: snapshot.market_funding_index,
        market_last_funding_ms: snapshot.market_last_funding_ms,
        insurance_funds: snapshot.insurance_funds,
        market_bad_debt_balances: snapshot.market_bad_debt_balances,
        account_market_bad_debt: snapshot.account_market_bad_debt,
        fee_vaults: snapshot.fee_vaults,
        validator_reward_pools: snapshot.validator_reward_pools,
        market_maker_rebate_pools: snapshot.market_maker_rebate_pools,
        treasury_balances: snapshot.treasury_balances,
        frozen_accounts: snapshot.frozen_accounts,
        under_maintenance_accounts: snapshot.under_maintenance_accounts,
        liquidation_escalation: snapshot.liquidation_escalation,
        isolated_collateral: snapshot.isolated_collateral,
        positions: snapshot.positions,
        intent_mempool: snapshot.intent_mempool,
        conditional_pool: snapshot.conditional_pool,
        runtime_events: snapshot.runtime_events,
        evm_transaction_receipts: snapshot.evm_transaction_receipts,
        next_runtime_event_seq: snapshot.next_runtime_event_seq,
        telemetry: snapshot.telemetry,
        suppress_fill_bankruptcy_reconcile: snapshot.suppress_fill_bankruptcy_reconcile,
    };
    borsh::to_vec(&LegacyRfc056EngineCanonicalRecord {
        version: ENGINE_CANONICAL_RECORD_VERSION,
        inner: legacy,
    })
    .expect("encode legacy canonical")
}

/// Minimal `Mandate` builder for tests that only care about the
/// `agent` field (e.g. the `mandates_by_agent` rebuild).
fn make_minimal_mandate(id: [u8; 32], agent: u64) -> crate::Mandate {
    crate::Mandate {
        id,
        owner: [0u8; 32],
        agent,
        version: 1,
        constraints: Vec::new(),
        objectives: Vec::new(),
        objective_weights: Vec::new(),
        conformance_window_ms: 0,
        policies: Vec::new(),
        session_keys: Vec::new(),
        activated_at: None,
        expires_at: 0,
        state: crate::MandateState::Active,
        description_uri: None,
        description_hash: None,
        conformance: crate::ConformanceState {
            score: 0,
            last_evaluated_at_ms: 0,
            pending_objectives: 0,
        },
    }
}

/// Genesis-only round-trip: plan -> apply -> hydrate, assert the
/// hydrated engine produces the same persisted snapshot as the
/// original. The engine is at genesis (no blocks past genesis,
/// empty WAL queues, no mandates/positions), so this exercises
/// the canonical record + derived-rebuild paths but not the WAL
/// replay paths.
#[test]
fn engine_backend_roundtrip_at_genesis_in_memory() {
    let engine = SingleNodeEngine::new(config());
    let meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    let plan = plan_engine_write(&engine, meta.clone()).expect("plan");
    assert_eq!(plan.wal_entry_count(), 0);

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_plan(&backend, plan).expect("apply");

    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(
        snapshot_bytes(&hydrated),
        snapshot_bytes(&engine),
        "hydrated snapshot must match original byte-for-byte"
    );
    assert_eq!(
        backend.latest_committed_meta().expect("meta").as_ref(),
        Some(&meta)
    );
}

#[test]
fn rfc056_policy_migration_converts_legacy_account_to_authority_scope() {
    let mut engine = SingleNodeEngine::new(config());
    engine
        .policy_store_mut()
        .insert(PolicyRecord::new(
            42,
            11,
            [4; 32],
            1_000,
            2_000,
            PolicyState::Active,
            PolicyTerms::default(),
        ))
        .expect("policy insert");

    let legacy_bytes = legacy_rfc056_canonical_bytes(&engine);
    let migrated = migrate_rfc056_policy_authority_scope_record(
        Namespace::State,
        ENGINE_CANONICAL_KEY,
        &legacy_bytes,
    )
    .expect("migrate canonical");
    let record: EngineCanonicalRecord =
        borsh::from_slice(&migrated).expect("decode migrated canonical");
    let migrated_policy = record.inner.policy_store.get(42).expect("policy");

    assert_eq!(migrated_policy.authority_account, 11);
    assert_eq!(
        migrated_policy.execution_scope,
        PolicyExecutionScope::Account(11)
    );
    assert_eq!(migrated_policy.key_state.grantee, [4; 32]);
    assert_eq!(migrated_policy.next_intent_nonce, 1);

    let untouched = migrate_rfc056_policy_authority_scope_record(
        Namespace::State,
        b"other_state_key",
        b"unchanged",
    )
    .expect("non canonical copy");
    assert_eq!(untouched, b"unchanged");
}

/// Same round-trip against redb, with a drop+reopen between apply
/// and hydrate to prove durability through a backend close.
#[test]
fn engine_backend_rebuild_without_snapshot_redb() {
    let engine = SingleNodeEngine::new(config());
    let meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    let plan = plan_engine_write(&engine, meta.clone()).expect("plan");

    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("engine.redb");

    // Write session.
    {
        let backend =
            AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path.clone())).expect("open"));
        apply_plan(&backend, plan).expect("apply");
    }

    // Reopen + hydrate (no snapshot file involved).
    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(snapshot_bytes(&hydrated), snapshot_bytes(&engine));
    assert_eq!(
        backend.latest_committed_meta().expect("meta").as_ref(),
        Some(&meta)
    );
}

#[test]
fn hydrate_rejects_backend_fingerprint_mismatch_before_decode() {
    let engine = SingleNodeEngine::new(config());
    let meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    let plan = plan_engine_write(&engine, meta).expect("plan");
    let source_fingerprint = [0x11; 32];
    let target_fingerprint = [0x22; 32];

    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("engine.redb");
    {
        let backend = AnyStateBackend::Redb(
            RedbBackend::open(
                OpenOptions::new(path.clone()).with_persistence_fingerprint(source_fingerprint),
            )
            .expect("open"),
        );
        apply_plan(&backend, plan).expect("apply");
    }

    let backend = AnyStateBackend::Redb(
        RedbBackend::open(OpenOptions::new(path).with_persistence_fingerprint(target_fingerprint))
            .expect("reopen"),
    );
    let err = hydrate_engine_from_backend(&backend).expect_err("must reject");
    assert!(
        matches!(
            err,
            PersistenceError::Backend(BackendError::SchemaMismatch(ref reason))
                if reason.contains("schema_fingerprint_mismatch")
        ),
        "expected schema fingerprint mismatch before hydrate decode, got {err:?}"
    );
}

/// Plan_engine_write rejects a meta tuple that disagrees with the
/// engine's head, state root, or event seq. Mismatched meta can
/// never enter a batch.
#[test]
fn plan_engine_write_rejects_meta_mismatch() {
    let engine = SingleNodeEngine::new(config());
    let mut bad_meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    bad_meta.height = 999; // wrong
    let err = plan_engine_write(&engine, bad_meta).expect_err("must reject");
    assert!(matches!(err, PersistenceError::MetaMismatch(_)));
}

/// Phase 4 derived rebuild: `mandates_by_agent` is correctly
/// reproduced from the pure mandate map, including grouping
/// multiple mandates per agent.
#[test]
fn rebuild_mandates_by_agent_groups_per_agent() {
    let mut mandates: BTreeMap<[u8; 32], crate::Mandate> = BTreeMap::new();
    let m1 = make_minimal_mandate([1; 32], 22);
    let m2 = make_minimal_mandate([2; 32], 22);
    let m3 = make_minimal_mandate([3; 32], 44);
    mandates.insert([1; 32], m1);
    mandates.insert([2; 32], m2);
    mandates.insert([3; 32], m3);

    let by_agent = rebuild_mandates_by_agent_pure(&mandates);
    assert_eq!(by_agent.len(), 2);
    assert_eq!(by_agent.get(&22), Some(&BTreeSet::from([[1; 32], [2; 32]])));
    assert_eq!(by_agent.get(&44), Some(&BTreeSet::from([[3; 32]])));
}

/// Phase 4 derived rebuild: `compute_under_maintenance_accounts`
/// must walk the same key set as `validate_persisted_risk_state` —
/// not just `positions`. An account with negative
/// `isolated_collateral` and **no position** is still under
/// maintenance (equity < 0 < required). The old positions-only
/// rebuild would have missed this.
#[test]
fn compute_under_maintenance_walks_collateral_keys_too() {
    let mut engine = SingleNodeEngine::new(config());
    // Pretend market 7 is registered with a 5% maintenance floor.
    engine.market_risk.insert(
        7,
        crate::MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    // Account 22 has -100 isolated collateral on market 7 with NO
    // position. The canonical risk validator considers this under
    // maintenance because equity < required (0).
    engine.isolated_collateral.insert((7, 22), -100i128);

    let computed = compute_under_maintenance_accounts(&engine);
    assert!(
        computed.contains(&(7, 22)),
        "compute_under_maintenance must include collateral-only accounts"
    );

    // Sanity: a positions-only walk would have returned empty.
    let positions_only: BTreeSet<(u64, u64)> = engine
        .positions
        .keys()
        .copied()
        .filter(|(m, a)| engine.margin_snapshot(*m, *a).under_maintenance)
        .collect();
    assert!(
        positions_only.is_empty(),
        "positions-only walk would have missed (7, 22)"
    );
}

/// Phase 4 WAL replay: a per-id binary-keyed WAL row written via
/// the plan path must round-trip exactly through `read_direct_wal`,
/// including byte-identical `IntentOrderKey` and `SignedIntent`.
#[test]
fn direct_mempool_wal_replays_byte_identical() {
    use kombat_chain_consensus::{
        Intent, IntentKind, IntentOrderKind, IntentOrderSide, IntentTimeInForce,
    };

    // Synthesise a SignedIntent and stash it in a fresh pool.
    let intent_id: [u8; 32] = [0xab; 32];
    let order_key = IntentOrderKey {
        policy_id: 42,
        nonce: 7,
        intent_id,
    };
    let signed = SignedIntent {
        intent: Intent {
            chain_id: 7,
            account: 22,
            policy_id: 42,
            nonce: 7,
            expires_at: 1_900_000_000_000,
            predicate: None,
            kind: IntentKind::PlaceOrder {
                market_id: 7,
                client_order_id: 1,
                side: IntentOrderSide::Bid,
                quantity_lots: 5,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        },
        signature: [0xcd; 64],
    };
    let mut pool = DirectIntentMempool::default();
    pool.intents_by_order.insert(order_key, signed.clone());
    pool.known_intents.insert(intent_id);

    // Plan -> apply.
    let pairs = build_direct_wal(&pool).expect("build wal");
    assert_eq!(pairs.len(), 1);
    assert_eq!(
        pairs[0].0,
        intent_id.to_vec(),
        "key must be raw intent_id bytes"
    );

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    let mut batch = backend.batch();
    for (k, v) in &pairs {
        batch.put(Namespace::DirectMempoolWal, k, v);
    }
    backend
        .commit_batch(
            batch,
            CommitMeta {
                schema_version: 1,
                height: 0,
                block_hash: [0; 32],
                chain_head_state_root: [0; 32],
                executor_state_root: [0; 32],
                event_seq: 0,
                timestamp_ms: 0,
            },
            SyncMode::Sync,
        )
        .expect("commit");

    // Read back.
    let txn = backend.read_txn().expect("read_txn");
    let replayed = read_direct_wal(&txn).expect("read wal");
    assert_eq!(
        replayed.intents_by_order.len(),
        1,
        "exactly one intent must replay"
    );
    let (replayed_key, replayed_signed) = replayed
        .intents_by_order
        .iter()
        .next()
        .expect("replayed entry");
    assert_eq!(*replayed_key, order_key);
    assert_eq!(*replayed_signed, signed);
    assert!(replayed.known_intents.contains(&intent_id));
}

/// Phase 4 WAL identity check: a WAL row whose key disagrees with
/// the embedded id is rejected at hydrate. Catches stale or
/// hand-tampered rows.
#[test]
fn wal_key_value_identity_check_rejects_mismatch() {
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());

    // Write a faucet WAL row under address [9; 20], but encode a
    // value claiming address [1; 20].
    let grant = crate::PendingFaucetGrant {
        address: [1; 20],
        policy_target_micro_usdc: [0; 32],
        evm_target_micro_usdc: [0; 32],
        submitted_at_ms: 1,
    };
    let value = borsh::to_vec(&grant).expect("encode grant");
    let mut batch = backend.batch();
    batch.put(Namespace::PendingFaucetWal, &[9u8; 20], &value);
    backend
        .commit_batch(
            batch,
            CommitMeta {
                schema_version: 1,
                height: 0,
                block_hash: [0; 32],
                chain_head_state_root: [0; 32],
                executor_state_root: [0; 32],
                event_seq: 0,
                timestamp_ms: 0,
            },
            SyncMode::Sync,
        )
        .expect("commit");

    let txn = backend.read_txn().expect("read_txn");
    let err = read_faucet_wal(&txn).expect_err("must reject");
    assert!(matches!(err, PersistenceError::Decode(_)));
}

/// Post-genesis empty-block roundtrip. After `produce_empty_block`
/// the executor's `committed_entries` is still empty (state-db's
/// no-op commit semantics) but height has advanced. Hydration
/// must accept this case.
#[test]
fn engine_backend_roundtrip_after_empty_block() {
    let mut engine = SingleNodeEngine::new(config());
    engine
        .produce_empty_block(2_000)
        .expect("produce empty block");
    assert_eq!(engine.head().header.height, 1);

    let meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    let plan = plan_engine_write(&engine, meta.clone()).expect("plan");

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_plan(&backend, plan).expect("apply");

    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(hydrated.head().header.height, 1);
    assert_snapshot_bytes_eq(&hydrated, &engine);
}

#[test]
fn engine_backend_native_mahi_root_split_roundtrip() {
    let cfg = config();
    let mut engine = SingleNodeEngine::new(cfg.clone());
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    engine
        .initialize_state_backend(&backend, 1_700_000_000_000)
        .expect("initialize backend");

    let block = MahiBlock::signed(
        cfg.validator_id,
        1,
        7,
        1_700_000_000_100,
        Vec::new(),
        Vec::new(),
        cfg.consensus_signing_key,
    );
    apply_engine_mutation_durable(
        &mut engine,
        &backend,
        1_700_000_000_100,
        SyncMode::Sync,
        |engine| engine.apply_mahi_committed_block(&block).map(|_| ()),
    )
    .expect("native Mahi block should persist")
    .expect("native Mahi block should apply");

    let meta = backend
        .latest_committed_meta()
        .expect("read meta")
        .expect("meta exists");
    assert_eq!(meta.chain_head_state_root, engine.head().header.state_root);
    assert_eq!(meta.executor_state_root, engine.committed_state_root());
    assert_ne!(
        meta.chain_head_state_root, meta.executor_state_root,
        "native Mahi empty blocks deliberately split chain-head and executor roots"
    );

    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(hydrated.head().header.height, engine.head().header.height);
    assert_eq!(block_hash(hydrated.head()), block_hash(engine.head()));
    assert_eq!(
        hydrated.head().header.state_root,
        meta.chain_head_state_root
    );
    assert_eq!(hydrated.committed_state_root(), meta.executor_state_root);
}

/// Hydrate refuses (cleanly, no panic) if the canonical record
/// has an empty chain. Without a genesis block the temp-engine
/// construction would panic in `head()`; we must reject earlier.
#[test]
fn hydrate_rejects_empty_chain_without_panic() {
    let engine = SingleNodeEngine::new(config());
    let meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    let mut plan = plan_engine_write(&engine, meta.clone()).expect("plan");

    // Tamper: rewrite the canonical record with an empty chain.
    let mut record: EngineCanonicalRecord =
        borsh::from_slice(&plan.canonical_bytes).expect("decode");
    record.inner.chain.clear();
    plan.canonical_bytes = borsh::to_vec(&record).expect("encode");

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_plan(&backend, plan).expect("apply");

    let err = hydrate_engine_from_backend(&backend).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::Decode(_)),
        "expected Decode, got {err:?}"
    );
}

/// Hydrate refuses if `latest_committed_meta` says one state root
/// but the canonical record says a different one. This is the
/// "block in / state out" guardrail.
#[test]
fn hydrate_rejects_state_root_mismatch() {
    let engine = SingleNodeEngine::new(config());
    let meta = engine_commit_meta(&engine, 1, 1_700_000_000_000).expect("meta");
    let mut plan = plan_engine_write(&engine, meta.clone()).expect("plan");

    // Tamper: rewrite the canonical record with a wrong committed
    // state root, then commit with the original meta.
    let mut record: EngineCanonicalRecord =
        borsh::from_slice(&plan.canonical_bytes).expect("decode");
    record.inner.committed_state_root = [9; 32];
    plan.canonical_bytes = borsh::to_vec(&record).expect("encode");

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_plan(&backend, plan).expect("apply");

    let err = hydrate_engine_from_backend(&backend).expect_err("must reject");
    assert!(matches!(err, PersistenceError::StateRootMismatch { .. }));
}

/// Hydrate refuses if no canonical record is present.
#[test]
fn hydrate_rejects_missing_canonical_record() {
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    // Commit some unrelated meta so latest_committed_meta is Some
    // but the canonical record is absent.
    let mut batch = backend.batch();
    batch.put(Namespace::Telemetry, b"unrelated", b"x");
    backend
        .commit_batch(
            batch,
            CommitMeta {
                schema_version: 1,
                height: 0,
                block_hash: [0; 32],
                chain_head_state_root: [0; 32],
                executor_state_root: [0; 32],
                event_seq: 0,
                timestamp_ms: 0,
            },
            SyncMode::Sync,
        )
        .expect("commit");

    let err = hydrate_engine_from_backend(&backend).expect_err("must reject");
    assert!(matches!(err, PersistenceError::MissingRecord(_)));
}

// ========================================================================
// Phase 5 — DurableCommitPlan tests
// ========================================================================

use kombat_chain_consensus::{Block, BlockBody, BlockHeader, QuorumCertificate};

/// Build coherent (canonical_bytes, block_bytes, block_hash, meta)
/// from a real engine. Used by Phase 5 positive plan tests so the
/// validator's canonical-record cross-checks pass.
fn real_plan_components(engine: &SingleNodeEngine) -> (Vec<u8>, Vec<u8>, [u8; 32], CommitMeta) {
    let meta = engine_commit_meta(engine, 1, 1_700_000_000_000).expect("meta");
    let phase4_plan = plan_engine_write(engine, meta.clone()).expect("phase4 plan");
    let head = engine.chain.last().expect("non-empty chain").clone();
    let block_bytes = borsh::to_vec(&head).expect("encode block");
    let head_hash = block_hash(&head);
    (phase4_plan.canonical_bytes, block_bytes, head_hash, meta)
}

/// Build a synthetic but well-formed `Block` with a known
/// `state_root` and return `(block, encoded_bytes, hash)`. Used by
/// Phase 5 negative plan tests where coherence with a canonical
/// record is intentionally broken.
fn make_synthetic_block(height: u64, state_root: [u8; 32]) -> (Block, Vec<u8>, [u8; 32]) {
    let block = Block {
        header: BlockHeader {
            chain_id: 7,
            height,
            round: height,
            proposer: [0u8; 32],
            parent_hash: [0u8; 32],
            parent_state_root: [0u8; 32],
            tx_root: [0u8; 32],
            event_root: [0u8; 32],
            state_root,
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0u8; 32],
                signatures: vec![],
            },
            timestamp_ms: 1_000_000 + height,
        },
        body: BlockBody { items: vec![] },
    };
    let bytes = borsh::to_vec(&block).expect("encode synthetic block");
    let hash = block_hash(&block);
    (block, bytes, hash)
}

/// Build a synthetic plan for a non-empty block, apply it, and
/// assert the backend exposes every expected put across every
/// namespace touched by block commit. Also asserts the meta tuple
/// is durable.
#[test]
fn durable_commit_plan_writes_all_namespaces_atomically() {
    let engine = SingleNodeEngine::new(config());
    let (canonical_bytes, block_bytes, block_hash, mut meta) = real_plan_components(&engine);
    // Override event_seq so the test exercises a non-zero value.
    // Canonical record has event_seq = 0 (genesis); the plan uses
    // a value that matches what the canonical record encodes.
    meta.event_seq = 0;
    let block_height = meta.height;
    let canonical_clone = canonical_bytes.clone();
    let block_bytes_clone = block_bytes.clone();
    let plan = DurableCommitPlan {
        canonical_bytes,
        block_height,
        block_hash,
        block_bytes,
        events: WriteSet {
            puts: vec![
                (100u64.to_be_bytes().to_vec(), b"event-100".to_vec()),
                (101u64.to_be_bytes().to_vec(), b"event-101".to_vec()),
            ],
        },
        receipts: WriteSet {
            puts: vec![([0xcc; 32].to_vec(), b"receipt-cc".to_vec())],
        },
        quarantine: QuarantineDelta {
            puts: vec![([0xdd; 32].to_vec(), b"q-reason".to_vec())],
            deletes: vec![[0xee; 32].to_vec()],
        },
        consensus_safety: WriteSet {
            puts: vec![(b"validator-set".to_vec(), b"v-set-bytes".to_vec())],
        },
        direct_mempool_wal: WalMutation {
            puts: vec![([0x01; 32].to_vec(), b"new-direct".to_vec())],
            deletes: vec![[0x02; 32].to_vec()],
        },
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation {
            puts: vec![],
            deletes: vec![[0xff; 20].to_vec()],
        },
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: Some(b"telemetry-blob".to_vec()),
        meta,
    };

    // Sanity: counters reflect what we composed.
    assert_eq!(
        plan.plan_put_count(),
        // canonical(1) + block(1) + block-hash-index(1) + events(2)
        // + receipts(1) + quarantine.puts(1) + consensus_safety(1)
        // + direct_wal.puts(1) + telemetry(1)
        10
    );
    assert_eq!(
        plan.plan_delete_count(),
        // quarantine(1) + direct_wal(1) + faucet_wal(1)
        3
    );

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_durable_commit_plan(&backend, plan.clone()).expect("apply");

    // Backend now contains everything in one consistent snapshot.
    let txn = backend.read_txn().expect("read_txn");
    assert_eq!(
        txn.get(Namespace::State, ENGINE_CANONICAL_KEY)
            .expect("get canonical"),
        Some(canonical_clone)
    );
    assert_eq!(
        txn.get(Namespace::Blocks, &block_height.to_be_bytes())
            .expect("get block"),
        Some(block_bytes_clone)
    );
    assert_eq!(
        txn.get(Namespace::BlockHashIndex, &block_hash)
            .expect("get block-hash-index"),
        Some(block_height.to_be_bytes().to_vec())
    );
    assert_eq!(
        txn.get(Namespace::Events, &100u64.to_be_bytes())
            .expect("get evt100"),
        Some(b"event-100".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::Receipts, &[0xcc; 32]).expect("get rec"),
        Some(b"receipt-cc".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::Quarantine, &[0xdd; 32])
            .expect("get q-puts"),
        Some(b"q-reason".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::ConsensusSafety, b"validator-set")
            .expect("get cs"),
        Some(b"v-set-bytes".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::DirectMempoolWal, &[0x01; 32])
            .expect("get direct-put"),
        Some(b"new-direct".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::Telemetry, TELEMETRY_KEY)
            .expect("get telem"),
        Some(b"telemetry-blob".to_vec())
    );

    // Meta is durable iff the rest of the plan landed.
    assert_eq!(
        backend.latest_committed_meta().expect("meta").as_ref(),
        Some(&plan.meta)
    );
}

#[test]
fn hydrate_engine_restores_node_local_history_namespaces() {
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    let engine = SingleNodeEngine::new(config());
    let seed_meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("seed meta");
    let seed_plan = plan_engine_write(&engine, seed_meta).expect("seed plan");
    apply_plan(&backend, seed_plan).expect("seed apply");

    let mut after = engine.clone();
    after.set_equity_history_samples([(
        11,
        AccountEquitySample {
            block_height: 1,
            timestamp_ms: 1_700_000_000_100,
            total_equity: -7,
            total_collateral: 3,
            total_maintenance_requirement: 10,
            total_margin_surplus: -17,
            position_count: 1,
            under_maintenance: true,
            ..AccountEquitySample::default()
        },
    )]);
    let mandate_id = [7u8; 32];
    after.set_mandate_conformance_history_samples([(
        mandate_id,
        MandateConformanceSample {
            mandate_id,
            block_height: 1,
            evaluated_at_ms: 1_700_000_000_100,
            score: 64,
            pending_objectives: 1,
            state: MandateState::Active,
            prev_score: Some(50),
            delta: Some(14),
        },
    )]);
    let meta = engine_commit_meta(&after, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_100)
        .expect("meta");
    let plan = build_durable_commit_plan(&engine, &after, meta).expect("durable plan");
    assert_eq!(plan.equity_history.puts.len(), 1);
    assert_eq!(plan.mandate_conformance_history.puts.len(), 1);
    apply_durable_commit_plan(&backend, plan).expect("apply");

    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(
        hydrated.all_equity_history_samples(),
        after.all_equity_history_samples()
    );
    assert_eq!(
        hydrated.all_mandate_conformance_history_samples(),
        after.all_mandate_conformance_history_samples()
    );
}

/// **Phase 5 core invariant:** exact WAL deletes scrub stale rows.
/// Pre-populate a backend with three direct-mempool WAL entries,
/// apply a `DurableCommitPlan` that explicitly deletes one of
/// them, and assert the deleted row is gone while the other two
/// remain.
#[test]
fn durable_commit_plan_deletes_consumed_wal_rows() {
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());

    // Seed three WAL entries in DirectMempoolWal under different
    // intent-id keys.
    let id_keep_a: [u8; 32] = [0x0a; 32];
    let id_consumed: [u8; 32] = [0x0b; 32];
    let id_keep_c: [u8; 32] = [0x0c; 32];
    let mut seed = backend.batch();
    seed.put(Namespace::DirectMempoolWal, &id_keep_a, b"signed-a");
    seed.put(Namespace::DirectMempoolWal, &id_consumed, b"signed-b");
    seed.put(Namespace::DirectMempoolWal, &id_keep_c, b"signed-c");
    backend
        .commit_batch(
            seed,
            CommitMeta {
                schema_version: 1,
                height: 0,
                block_hash: [0; 32],
                chain_head_state_root: [0; 32],
                executor_state_root: [0; 32],
                event_seq: 0,
                timestamp_ms: 0,
            },
            SyncMode::Sync,
        )
        .expect("seed");

    // Apply a plan that deletes the consumed id and adds a new one.
    // Use real coherent canonical/block/meta so the validator
    // accepts the plan; the test's WAL deletion semantics are
    // independent of the block height.
    let id_new: [u8; 32] = [0x0d; 32];
    let engine = SingleNodeEngine::new(config());
    let (canonical_bytes, block_bytes, block_hash, meta) = real_plan_components(&engine);
    let plan = DurableCommitPlan {
        canonical_bytes,
        block_height: meta.height,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation {
            puts: vec![(id_new.to_vec(), b"signed-d".to_vec())],
            deletes: vec![id_consumed.to_vec()],
        },
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta,
    };
    apply_durable_commit_plan(&backend, plan).expect("apply");

    // Verify: kept rows are still there, consumed row is gone,
    // newly-admitted row is present.
    let txn = backend.read_txn().expect("read_txn");
    assert_eq!(
        txn.get(Namespace::DirectMempoolWal, &id_keep_a)
            .expect("get a"),
        Some(b"signed-a".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::DirectMempoolWal, &id_consumed)
            .expect("get b"),
        None,
        "consumed WAL row must be tombstoned"
    );
    assert_eq!(
        txn.get(Namespace::DirectMempoolWal, &id_keep_c)
            .expect("get c"),
        Some(b"signed-c".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::DirectMempoolWal, &id_new)
            .expect("get new"),
        Some(b"signed-d".to_vec())
    );
}

/// `DurableCommitPlan::plan_put_count` / `plan_delete_count` are
/// accurate for an empty block. Also confirms a coherent plan
/// passes the Phase 5 validator and applies cleanly. Used as a
/// smoke test for the counters (Phase 9 bench instrumentation
/// will consume them).
#[test]
fn durable_commit_plan_counters_handle_empty_block() {
    let engine = SingleNodeEngine::new(config());
    let (canonical_bytes, block_bytes, block_hash, meta) = real_plan_components(&engine);
    let plan = DurableCommitPlan {
        canonical_bytes,
        block_height: meta.height,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta,
    };
    // canonical(1) + block(1) + block-hash-index(1) = 3
    assert_eq!(plan.plan_put_count(), 3);
    assert_eq!(plan.plan_delete_count(), 0);
    // Coherent plan applies without rejection.
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_durable_commit_plan(&backend, plan).expect("apply");
}

/// **Phase 5 guardrail:** `validate_durable_commit_plan` rejects a
/// plan whose `block_height` / `block_hash` disagree with `meta`.
/// Apply must error before any batch is built.
#[test]
fn validate_rejects_plan_block_meta_mismatch() {
    let state_root: [u8; 32] = [0x11; 32];
    let (_block, block_bytes, block_hash) = make_synthetic_block(5, state_root);

    // Mismatch height.
    let plan = DurableCommitPlan {
        canonical_bytes: b"x".to_vec(),
        block_height: 5,
        block_hash,
        block_bytes: block_bytes.clone(),
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta: CommitMeta {
            schema_version: 1,
            height: 999, // mismatch
            block_hash,
            chain_head_state_root: state_root,
            executor_state_root: state_root,
            event_seq: 0,
            timestamp_ms: 0,
        },
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::MetaMismatch(ref m) if m.contains("plan.block_height")),
        "expected MetaMismatch on height, got {err:?}"
    );

    // Mismatch hash.
    let plan = DurableCommitPlan {
        canonical_bytes: b"x".to_vec(),
        block_height: 5,
        block_hash: [0xaa; 32], // bogus, doesn't match decoded
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta: CommitMeta {
            schema_version: 1,
            height: 5,
            block_hash: [0xaa; 32],
            chain_head_state_root: state_root,
            executor_state_root: state_root,
            event_seq: 0,
            timestamp_ms: 0,
        },
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::MetaMismatch(ref m) if m.contains("decoded block hash")),
        "expected MetaMismatch on decoded-hash, got {err:?}"
    );
}

/// `validate_durable_commit_plan` rejects a plan whose
/// `block_bytes` cannot be decoded as a `Block`.
#[test]
fn validate_rejects_plan_with_bad_block_bytes() {
    let state_root: [u8; 32] = [0; 32];
    let (_block, _bytes, block_hash) = make_synthetic_block(3, state_root);
    let plan = DurableCommitPlan {
        canonical_bytes: b"x".to_vec(),
        block_height: 3,
        block_hash,
        block_bytes: b"this is not a borsh-encoded Block".to_vec(),
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta: CommitMeta {
            schema_version: 1,
            height: 3,
            block_hash,
            chain_head_state_root: state_root,
            executor_state_root: state_root,
            event_seq: 0,
            timestamp_ms: 0,
        },
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::Decode(ref m) if m.contains("block_bytes")),
        "expected Decode on bad block bytes, got {err:?}"
    );
}

/// `validate_durable_commit_plan` rejects a plan whose decoded
/// block's `state_root` disagrees with `meta.chain_head_state_root`. This is
/// the "block in / state out" guardrail at plan time.
#[test]
fn validate_rejects_plan_with_block_state_root_drift() {
    let block_state_root: [u8; 32] = [0x77; 32];
    let meta_state_root: [u8; 32] = [0x88; 32]; // different
    let (_block, block_bytes, block_hash) = make_synthetic_block(4, block_state_root);
    let plan = DurableCommitPlan {
        canonical_bytes: b"x".to_vec(),
        block_height: 4,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta: CommitMeta {
            schema_version: 1,
            height: 4,
            block_hash,
            chain_head_state_root: meta_state_root,
            executor_state_root: meta_state_root,
            event_seq: 0,
            timestamp_ms: 0,
        },
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::StateRootMismatch { .. }),
        "expected StateRootMismatch, got {err:?}"
    );
}

/// **Phase 5 guardrail:** `validate_durable_commit_plan` rejects a
/// plan with overlapping put+delete on the same WAL key — under
/// put-then-delete batch ordering the put would silently disappear.
/// Uses real coherent canonical/block/meta so the validator
/// reaches the WAL check.
#[test]
fn validate_rejects_plan_with_wal_put_delete_overlap() {
    let engine = SingleNodeEngine::new(config());
    let (canonical_bytes, block_bytes, block_hash, meta) = real_plan_components(&engine);
    let collision_key: [u8; 32] = [0xee; 32];
    let plan = DurableCommitPlan {
        canonical_bytes,
        block_height: meta.height,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation {
            puts: vec![(collision_key.to_vec(), b"new".to_vec())],
            deletes: vec![collision_key.to_vec()], // same key in both
        },
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta,
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::MetaMismatch(ref m) if m.contains("overlapping put+delete")),
        "expected MetaMismatch on overlap, got {err:?}"
    );

    // Apply path also rejects (same validator).
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    let err = apply_durable_commit_plan(&backend, plan).expect_err("apply must reject");
    assert!(
        matches!(err, PersistenceError::MetaMismatch(_)),
        "apply must reject overlap, got {err:?}"
    );
}

/// **Phase 5 guardrail:** `validate_durable_commit_plan` rejects a
/// plan whose `canonical_bytes` cannot be decoded as
/// `EngineCanonicalRecord`. The block, meta, and WAL sections of
/// the plan are intentionally coherent so the validator reaches
/// the canonical-decode step and fails there.
#[test]
fn validate_rejects_plan_with_bad_canonical_bytes() {
    let engine = SingleNodeEngine::new(config());
    let (_real_canonical, block_bytes, block_hash, meta) = real_plan_components(&engine);
    let plan = DurableCommitPlan {
        canonical_bytes: b"not a borsh-encoded EngineCanonicalRecord".to_vec(),
        block_height: meta.height,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta,
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::Decode(ref m) if m.contains("canonical_bytes")),
        "expected Decode on bad canonical bytes, got {err:?}"
    );
}

/// `validate_durable_commit_plan` rejects a plan whose canonical
/// record's head disagrees with `meta` (height, hash, or
/// state_root) — even when the block_bytes side of the plan is
/// coherent. Mutates a real canonical record before re-encoding.
#[test]
fn validate_rejects_plan_with_canonical_meta_drift() {
    let engine = SingleNodeEngine::new(config());
    let (canonical_bytes, block_bytes, block_hash, meta) = real_plan_components(&engine);

    // Drift case 1: tamper canonical record's committed_state_root.
    let mut record: EngineCanonicalRecord = borsh::from_slice(&canonical_bytes).expect("decode");
    record.inner.committed_state_root = [0xee; 32];
    let tampered = borsh::to_vec(&record).expect("encode");
    let plan = DurableCommitPlan {
        canonical_bytes: tampered,
        block_height: meta.height,
        block_hash,
        block_bytes: block_bytes.clone(),
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta: meta.clone(),
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::StateRootMismatch { .. }),
        "expected StateRootMismatch on canonical state root drift, got {err:?}"
    );

    // Drift case 2: tamper canonical record's next_runtime_event_seq.
    let mut record: EngineCanonicalRecord = borsh::from_slice(&canonical_bytes).expect("decode");
    record.inner.next_runtime_event_seq = 9999;
    let tampered = borsh::to_vec(&record).expect("encode");
    let plan = DurableCommitPlan {
        canonical_bytes: tampered,
        block_height: meta.height,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta,
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::MetaMismatch(ref m) if m.contains("next_runtime_event_seq")),
        "expected MetaMismatch on canonical event_seq drift, got {err:?}"
    );
}

/// `validate_durable_commit_plan` rejects a plan whose canonical
/// record carries the wrong schema version.
#[test]
fn validate_rejects_plan_with_wrong_canonical_schema_version() {
    let engine = SingleNodeEngine::new(config());
    let (canonical_bytes, block_bytes, block_hash, meta) = real_plan_components(&engine);
    let mut record: EngineCanonicalRecord = borsh::from_slice(&canonical_bytes).expect("decode");
    record.version = 999;
    let tampered = borsh::to_vec(&record).expect("encode");
    let plan = DurableCommitPlan {
        canonical_bytes: tampered,
        block_height: meta.height,
        block_hash,
        block_bytes,
        events: WriteSet::default(),
        receipts: WriteSet::default(),
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal: WalMutation::default(),
        conditional_mempool_wal: WalMutation::default(),
        faucet_wal: WalMutation::default(),
        delegation_wal: WalMutation::default(),
        mm_program_wal: WalMutation::default(),
        equity_history: WalMutation::default(),
        mandate_conformance_history: WalMutation::default(),
        telemetry_bytes: None,
        meta,
    };
    let err = validate_durable_commit_plan(&plan).expect_err("must reject");
    assert!(
        matches!(err, PersistenceError::SchemaMismatch { actual: 999, .. }),
        "expected SchemaMismatch with actual=999, got {err:?}"
    );
}

// ========================================================================
// Phase 6 — Block-commit durability beachhead
// ========================================================================

/// **Phase 6 round-trip:** produce an empty block durably, drop the
/// engine handle, reopen the redb backend, hydrate, assert the
/// hydrated engine matches what the durable commit produced. This
/// is the load-bearing restart/hydrate check — it crosses a real
/// process-style boundary (drop both engine and Database, reopen
/// from disk).
#[test]
fn produce_empty_block_durable_round_trips_through_redb() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase6.redb");

    let expected_snapshot_bytes = {
        let mut engine = SingleNodeEngine::new(config());
        let backend =
            AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path.clone())).expect("open"));

        // Pre-block durable seed: write the genesis canonical record
        // so the backend has a baseline. Phase 6 only handles
        // block-commit-time durability; the initial seed is a Phase
        // 4 helper used here as a test convenience.
        let seed_meta =
            engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
                .expect("seed meta");
        let seed_plan = plan_engine_write(&engine, seed_meta).expect("seed plan");
        apply_plan(&backend, seed_plan).expect("seed apply");

        // Now produce one empty block durably.
        produce_empty_block_durable(&mut engine, &backend, 2_000).expect("durable empty block");
        assert_eq!(engine.head().header.height, 1);

        snapshot_bytes(&engine)
        // engine + backend dropped at end of scope.
    };

    // Reopen backend. Hydrate. Compare.
    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(hydrated.head().header.height, 1);
    assert_eq!(snapshot_bytes(&hydrated), expected_snapshot_bytes);
}

/// **Phase 6 invariant:** durable Sync commit succeeds first, then
/// in-memory install. Simulate a kill between fsync and the
/// `*engine = clone` step by performing the durable apply manually
/// without touching `engine`. Drop the engine. Reopen the backend.
/// Hydrate. The hydrated engine MUST be at the new head — proving
/// that even when memory install never finished, the new head is
/// recoverable from disk.
#[test]
fn kill_after_fsync_before_install_recovers_new_head() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("kill_after.redb");

    // Pre-seed at genesis.
    let engine = SingleNodeEngine::new(config());
    let backend =
        AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path.clone())).expect("open"));
    let seed_meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("seed meta");
    let seed_plan = plan_engine_write(&engine, seed_meta).expect("seed plan");
    apply_plan(&backend, seed_plan).expect("seed apply");

    // Manually run produce_empty_block_durable's first 4 steps (no
    // *engine = clone). The clone has the new block; engine is
    // unchanged. The fsync has completed.
    let mut clone = engine.clone();
    clone.produce_empty_block(2_000).expect("install on clone");
    let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("meta");
    let plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan");
    apply_durable_commit_plan(&backend, plan).expect("apply");

    // Capture what the post-install snapshot WOULD have been.
    let expected_snapshot = snapshot_bytes(&clone);

    // Drop engine + clone + backend WITHOUT promoting clone in place
    // (== kill between fsync and `*engine = clone`).
    drop(engine);
    drop(clone);
    drop(backend);

    // Reopen and hydrate.
    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(
        hydrated.head().header.height,
        1,
        "hydrate must surface the new head even though in-memory install never ran"
    );
    assert_eq!(
        snapshot_bytes(&hydrated),
        expected_snapshot,
        "hydrated state must equal the post-install state"
    );
}

/// **Phase 6 invariant (other direction):** durable Sync commit
/// must come *before* in-memory install. Simulate a kill before
/// fsync by failing the apply step. The engine should be left
/// untouched (no in-memory install happened). Backend should
/// reflect the previous head only.
#[test]
fn kill_before_fsync_recovers_old_head() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("kill_before.redb");

    let engine = SingleNodeEngine::new(config());
    let backend =
        AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path.clone())).expect("open"));
    let seed_meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("seed meta");
    let seed_plan = plan_engine_write(&engine, seed_meta).expect("seed plan");
    apply_plan(&backend, seed_plan).expect("seed apply");

    let pre_snapshot = snapshot_bytes(&engine);
    assert_eq!(engine.head().header.height, 0);

    // Build a plan but inject pre-fsync corruption: tamper the
    // canonical_bytes so the validator (run inside
    // apply_durable_commit_plan) rejects before any batch hits the
    // backend. This stands in for "the fsync never completed."
    let mut clone = engine.clone();
    clone.produce_empty_block(2_000).expect("install on clone");
    let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("meta");
    let mut plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan");
    plan.canonical_bytes = b"corrupt-pre-fsync".to_vec();

    let err = apply_durable_commit_plan(&backend, plan).expect_err("must reject corrupt plan");
    assert!(matches!(err, PersistenceError::Decode(_)));

    // Engine never advanced (we manually built/applied the plan,
    // bypassing produce_empty_block_durable's `*engine = clone`).
    assert_eq!(snapshot_bytes(&engine), pre_snapshot);
    drop(engine);
    drop(clone);
    drop(backend);

    // Reopen and hydrate. Must see the OLD head (the seed).
    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(hydrated.head().header.height, 0);
    assert_eq!(snapshot_bytes(&hydrated), pre_snapshot);
}

// ========================================================================
// Phase 7 — Durable WAL admission
// ========================================================================

use kombat_chain_consensus::{
    Intent, IntentKind, IntentOrderKind, IntentOrderSide, IntentTimeInForce,
};

/// Construct a synthetic SignedIntent suitable for direct-mempool
/// poking in Phase 7 tests. Validation is bypassed; only the
/// durable WAL pipeline is exercised.
fn synthetic_signed_intent_for_test(intent_id_byte: u8, nonce: u64) -> SignedIntent {
    SignedIntent {
        intent: Intent {
            chain_id: 7,
            account: 22,
            policy_id: 42,
            nonce,
            expires_at: 1_900_000_000_000,
            predicate: None,
            kind: IntentKind::PlaceOrder {
                market_id: 7,
                client_order_id: 1,
                side: IntentOrderSide::Bid,
                quantity_lots: 5,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        },
        signature: [intent_id_byte; 64],
    }
}

/// Pre-seed a redb-backed engine at genesis so subsequent durable
/// writes have a baseline canonical record + meta to extend.
fn seed_genesis_redb(path: &std::path::Path) -> AnyStateBackend {
    let engine = SingleNodeEngine::new(config());
    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("open"));
    let seed_meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("seed meta");
    let seed_plan = plan_engine_write(&engine, seed_meta).expect("seed plan");
    apply_plan(&backend, seed_plan).expect("seed apply");
    backend
}

#[test]
fn repair_market_custody_credits_excess_to_treasury_without_rewriting_root() {
    let mut engine = SingleNodeEngine::new(config());
    engine.set_isolated_collateral(7, 22, 50);
    engine.fee_vaults.insert(7, 3);
    engine.insurance_funds.insert(7, 4);
    let custody_key = policy_usdc_balance_state_key(isolated_margin_market_custody_address(7));
    let original_root = engine
        .apply_committed_state_updates(
            vec![(custody_key, U256::from(60_u64).to_be_bytes::<32>().to_vec())],
            Vec::<Vec<u8>>::new(),
        )
        .expect("custody state should apply");
    let original_head_hash = block_hash(engine.head());

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    initialize_fresh_backend_from_engine(&backend, &engine, 1_700_000_000_000)
        .expect("backend should initialize");
    let err = hydrate_engine_from_backend(&backend).expect_err("custody mismatch should reject");
    assert!(
        err.to_string().contains("market custody mismatch"),
        "unexpected error: {err:?}"
    );

    let dry_run = repair_market_custody_in_backend(&backend, true).expect("dry-run repair");
    assert_eq!(dry_run.adjustments.len(), 1);
    assert!(
        hydrate_engine_from_backend(&backend).is_err(),
        "dry-run must not mutate backend"
    );

    let repair = repair_market_custody_in_backend(&backend, false).expect("repair");
    assert_eq!(repair.adjustments.len(), 1);
    let adjustment = &repair.adjustments[0];
    assert_eq!(adjustment.market_id, 7);
    assert_eq!(adjustment.custody_balance_micro_usdc, 60);
    assert_eq!(adjustment.required_before_micro_usdc, 57);
    assert_eq!(adjustment.treasury_before_micro_usdc, 0);
    assert_eq!(adjustment.treasury_after_micro_usdc, 3);

    let restored = hydrate_engine_from_backend(&backend).expect("repaired backend hydrates");
    assert_eq!(restored.committed_state_root(), original_root);
    assert_eq!(block_hash(restored.head()), original_head_hash);
    assert_eq!(restored.treasury_balance(7), 3);
    assert_eq!(
        restored
            .market_custody_required_balance(7)
            .expect("requirement computes"),
        60
    );
}

#[test]
fn repair_isolated_collateral_caps_row_and_reconciles_custody_excess() {
    let mut engine = SingleNodeEngine::new(config());
    engine.set_isolated_collateral(7, 260001, 1_000_000);
    let custody_key = policy_usdc_balance_state_key(isolated_margin_market_custody_address(7));
    let original_root = engine
        .apply_committed_state_updates(
            vec![(
                custody_key,
                U256::from(200_u64).to_be_bytes::<32>().to_vec(),
            )],
            Vec::<Vec<u8>>::new(),
        )
        .expect("custody state should apply");
    let original_head_hash = block_hash(engine.head());

    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    initialize_fresh_backend_from_engine(&backend, &engine, 1_700_000_000_000)
        .expect("backend should initialize");
    let err = hydrate_engine_from_backend(&backend).expect_err("custody mismatch should reject");
    assert!(
        err.to_string().contains("market custody mismatch"),
        "unexpected error: {err:?}"
    );

    let targets = vec![IsolatedCollateralRepairTarget {
        market_id: 7,
        account_id: 260001,
        collateral_micro_usdc: 100,
    }];
    let dry_run = repair_isolated_collateral_in_backend(&backend, true, &targets, 10)
        .expect("dry-run repair");
    assert_eq!(dry_run.collateral_adjustments.len(), 1);
    assert_eq!(dry_run.custody_adjustments.len(), 1);
    assert!(
        hydrate_engine_from_backend(&backend).is_err(),
        "dry-run must not mutate backend"
    );

    let repair =
        repair_isolated_collateral_in_backend(&backend, false, &targets, 10).expect("repair");
    assert_eq!(repair.collateral_adjustments.len(), 1);
    assert_eq!(repair.custody_adjustments.len(), 1);
    assert_eq!(
        repair.collateral_adjustments[0].before_micro_usdc,
        1_000_000
    );
    assert_eq!(repair.collateral_adjustments[0].after_micro_usdc, 100);
    assert_eq!(repair.custody_adjustments[0].treasury_after_micro_usdc, 100);

    let restored = hydrate_engine_from_backend(&backend).expect("repaired backend hydrates");
    assert_eq!(restored.committed_state_root(), original_root);
    assert_eq!(block_hash(restored.head()), original_head_hash);
    assert_eq!(restored.isolated_collateral(7, 260001), 100);
    assert_eq!(restored.treasury_balance(7), 100);
    assert_eq!(
        restored
            .market_custody_required_balance(7)
            .expect("requirement computes"),
        200
    );
}

/// `mempool_wal_replays_durable_direct_intents`: a synthetic
/// admitted-but-not-committed direct intent survives a process
/// boundary. Pokes the intent into a clone's mempool, runs the
/// durable plan + apply path with `SyncMode::Sync`, drops engine
/// + backend, reopens, and reads the `DirectMempoolWal` namespace
/// directly. The bytes MUST round-trip via `read_direct_wal`.
///
/// We deliberately read the WAL via `read_direct_wal` rather than
/// `hydrate_engine_from_backend` to keep this test focused on the
/// durable WAL pipeline (Phase 7 owns this) without dragging in
/// admission-side validators that require a real policy + signing
/// key. End-to-end admission integration with full hydrate-side
/// validation lives in a follow-up phase that has access to
/// `submit_intent`'s test scaffolding.
#[test]
fn mempool_wal_replays_durable_direct_intents() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase7_direct.redb");

    let intent_id: [u8; 32] = [0xa1; 32];
    let order_key = IntentOrderKey {
        policy_id: 42,
        nonce: 1,
        intent_id,
    };
    let signed = synthetic_signed_intent_for_test(0xa1, 1);

    {
        let backend = seed_genesis_redb(&path);
        let engine = hydrate_engine_from_backend(&backend).expect("hydrate seed");

        let mut clone = engine.clone();
        clone
            .intent_mempool
            .intents_by_order
            .insert(order_key, signed.clone());
        clone.intent_mempool.known_intents.insert(intent_id);

        let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_001)
            .expect("meta");
        let plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan");
        // Plan carries exactly one direct-mempool put with the
        // raw intent_id as key.
        assert_eq!(plan.direct_mempool_wal.puts.len(), 1);
        assert_eq!(plan.direct_mempool_wal.puts[0].0, intent_id.to_vec());
        assert!(plan.direct_mempool_wal.deletes.is_empty());

        apply_durable_commit_plan_with_mode(&backend, plan, SyncMode::Sync).expect("apply");
        // engine + backend dropped at scope end (no in-memory
        // promote — Phase 7 cares about the WAL surviving disk).
    }

    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let txn = backend.read_txn().expect("read_txn");
    let replayed = read_direct_wal(&txn).expect("read direct WAL");
    assert_eq!(replayed.intents_by_order.len(), 1);
    assert_eq!(replayed.intents_by_order.get(&order_key), Some(&signed));
    assert!(replayed.known_intents.contains(&intent_id));
}

#[test]
fn hydrate_prunes_committed_intent_wal_rows() {
    let mut engine = SingleNodeEngine::new(config());
    let mut policy = PolicyRecord::new(
        42,
        22,
        [4; 32],
        1_000,
        1_900_000_000_000,
        PolicyState::Active,
        PolicyTerms::default(),
    );
    policy.next_intent_nonce = 2;
    engine
        .policy_store_mut()
        .insert(policy)
        .expect("policy insert");

    let meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("meta");
    let plan = plan_engine_write(&engine, meta.clone()).expect("plan");
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_plan(&backend, plan).expect("apply");

    let direct_stale = synthetic_signed_intent_for_test(0xa1, 1);
    let direct_stale_key = IntentOrderKey {
        policy_id: direct_stale.intent.policy_id,
        nonce: direct_stale.intent.nonce,
        intent_id: intent_id(&direct_stale),
    };
    let direct_live = synthetic_signed_intent_for_test(0xa2, 2);
    let direct_live_key = IntentOrderKey {
        policy_id: direct_live.intent.policy_id,
        nonce: direct_live.intent.nonce,
        intent_id: intent_id(&direct_live),
    };
    let mut conditional_stale = synthetic_signed_intent_for_test(0xb1, 1);
    conditional_stale.intent.predicate = Some(kombat_chain_consensus::IntentPredicate::TimeAfter {
        not_before_ms: 1_800_000_000_000,
    });
    let conditional_stale_key = ActivationOrderKey {
        not_before_ms: 1_800_000_000_000,
        order_key: IntentOrderKey {
            policy_id: conditional_stale.intent.policy_id,
            nonce: conditional_stale.intent.nonce,
            intent_id: intent_id(&conditional_stale),
        },
    };
    let mut conditional_live = synthetic_signed_intent_for_test(0xb3, 3);
    conditional_live.intent.predicate = Some(kombat_chain_consensus::IntentPredicate::TimeAfter {
        not_before_ms: 1_800_000_000_000,
    });
    let conditional_live_key = ActivationOrderKey {
        not_before_ms: 1_800_000_000_000,
        order_key: IntentOrderKey {
            policy_id: conditional_live.intent.policy_id,
            nonce: conditional_live.intent.nonce,
            intent_id: intent_id(&conditional_live),
        },
    };

    let mut batch = backend.batch();
    for (key, signed) in [
        (direct_stale_key, direct_stale),
        (direct_live_key, direct_live.clone()),
    ] {
        let value = borsh::to_vec(&(key, signed)).expect("encode direct wal row");
        batch.put(Namespace::DirectMempoolWal, &key.intent_id, &value);
    }
    for (key, signed) in [
        (conditional_stale_key, conditional_stale),
        (conditional_live_key, conditional_live.clone()),
    ] {
        let value = borsh::to_vec(&(key, signed)).expect("encode conditional wal row");
        batch.put(
            Namespace::ConditionalMempoolWal,
            &key.order_key.intent_id,
            &value,
        );
    }
    backend
        .commit_batch(batch, meta, SyncMode::Sync)
        .expect("commit stale wal rows");

    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    assert_eq!(hydrated.intent_mempool.intents_by_order.len(), 1);
    assert_eq!(
        hydrated
            .intent_mempool
            .intents_by_order
            .get(&direct_live_key),
        Some(&direct_live)
    );
    assert!(!hydrated
        .intent_mempool
        .known_intents
        .contains(&direct_stale_key.intent_id));
    assert_eq!(hydrated.conditional_pool.time_after_by_order.len(), 1);
    assert_eq!(
        hydrated
            .conditional_pool
            .time_after_by_order
            .get(&conditional_live_key),
        Some(&conditional_live)
    );
    assert!(!hydrated
        .conditional_pool
        .known_intents
        .contains(&conditional_stale_key.order_key.intent_id));
}

#[test]
fn hydrate_prunes_duplicate_policy_nonce_wal_rows() {
    let mut engine = SingleNodeEngine::new(config());
    let mut policy = PolicyRecord::new(
        42,
        22,
        [4; 32],
        1_000,
        1_900_000_000_000,
        PolicyState::Active,
        PolicyTerms::default(),
    );
    policy.next_intent_nonce = 2;
    engine
        .policy_store_mut()
        .insert(policy)
        .expect("policy insert");

    let meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("meta");
    let plan = plan_engine_write(&engine, meta.clone()).expect("plan");
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    apply_plan(&backend, plan).expect("apply");

    let duplicate_a = synthetic_signed_intent_for_test(0xa1, 2);
    let duplicate_a_key = IntentOrderKey {
        policy_id: duplicate_a.intent.policy_id,
        nonce: duplicate_a.intent.nonce,
        intent_id: intent_id(&duplicate_a),
    };
    let duplicate_b = synthetic_signed_intent_for_test(0xa2, 2);
    let duplicate_b_key = IntentOrderKey {
        policy_id: duplicate_b.intent.policy_id,
        nonce: duplicate_b.intent.nonce,
        intent_id: intent_id(&duplicate_b),
    };
    let later_direct = synthetic_signed_intent_for_test(0xa3, 3);
    let later_direct_key = IntentOrderKey {
        policy_id: later_direct.intent.policy_id,
        nonce: later_direct.intent.nonce,
        intent_id: intent_id(&later_direct),
    };
    let mut conditional_duplicate = synthetic_signed_intent_for_test(0xb1, 2);
    conditional_duplicate.intent.predicate =
        Some(kombat_chain_consensus::IntentPredicate::TimeAfter {
            not_before_ms: 1_800_000_000_000,
        });
    let conditional_duplicate_key = ActivationOrderKey {
        not_before_ms: 1_800_000_000_000,
        order_key: IntentOrderKey {
            policy_id: conditional_duplicate.intent.policy_id,
            nonce: conditional_duplicate.intent.nonce,
            intent_id: intent_id(&conditional_duplicate),
        },
    };

    let mut batch = backend.batch();
    for (key, signed) in [
        (duplicate_a_key, duplicate_a.clone()),
        (duplicate_b_key, duplicate_b.clone()),
        (later_direct_key, later_direct.clone()),
    ] {
        let value = borsh::to_vec(&(key, signed)).expect("encode direct wal row");
        batch.put(Namespace::DirectMempoolWal, &key.intent_id, &value);
    }
    let value = borsh::to_vec(&(conditional_duplicate_key, conditional_duplicate.clone()))
        .expect("encode conditional wal row");
    batch.put(
        Namespace::ConditionalMempoolWal,
        &conditional_duplicate_key.order_key.intent_id,
        &value,
    );
    backend
        .commit_batch(batch, meta, SyncMode::Sync)
        .expect("commit duplicate wal rows");

    let hydrated = hydrate_engine_from_backend(&backend).expect("hydrate");
    let (kept_key, kept_intent, dropped_key) = if duplicate_a_key < duplicate_b_key {
        (duplicate_a_key, duplicate_a, duplicate_b_key)
    } else {
        (duplicate_b_key, duplicate_b, duplicate_a_key)
    };
    assert_eq!(hydrated.intent_mempool.intents_by_order.len(), 2);
    assert_eq!(
        hydrated.intent_mempool.intents_by_order.get(&kept_key),
        Some(&kept_intent)
    );
    assert_eq!(
        hydrated
            .intent_mempool
            .intents_by_order
            .get(&later_direct_key),
        Some(&later_direct)
    );
    assert!(!hydrated
        .intent_mempool
        .known_intents
        .contains(&dropped_key.intent_id));
    assert!(hydrated.conditional_pool.time_after_by_order.is_empty());
    assert!(!hydrated
        .conditional_pool
        .known_intents
        .contains(&conditional_duplicate_key.order_key.intent_id));
}

/// `conditional_wal_replays_durable_intents`: same as direct but
/// for the time-after conditional pool.
#[test]
fn conditional_wal_replays_durable_intents() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase7_conditional.redb");

    let intent_id: [u8; 32] = [0xb2; 32];
    let activation_key = ActivationOrderKey {
        not_before_ms: 2_000_000_000_000,
        order_key: IntentOrderKey {
            policy_id: 42,
            nonce: 2,
            intent_id,
        },
    };
    let mut signed = synthetic_signed_intent_for_test(0xb2, 2);
    signed.intent.predicate = Some(kombat_chain_consensus::IntentPredicate::TimeAfter {
        not_before_ms: 2_000_000_000_000,
    });

    {
        let backend = seed_genesis_redb(&path);
        let engine = hydrate_engine_from_backend(&backend).expect("hydrate seed");

        let mut clone = engine.clone();
        clone
            .conditional_pool
            .time_after_by_order
            .insert(activation_key, signed.clone());
        clone.conditional_pool.known_intents.insert(intent_id);

        let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_002)
            .expect("meta");
        let plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan");
        assert_eq!(plan.conditional_mempool_wal.puts.len(), 1);
        assert_eq!(
            plan.conditional_mempool_wal.puts[0].0,
            intent_id.to_vec(),
            "conditional WAL key must be the inner intent_id"
        );

        apply_durable_commit_plan_with_mode(&backend, plan, SyncMode::Sync).expect("apply");
    }

    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let txn = backend.read_txn().expect("read_txn");
    let replayed = read_conditional_wal(&txn).expect("read conditional WAL");
    assert_eq!(replayed.time_after_by_order.len(), 1);
    assert_eq!(
        replayed.time_after_by_order.get(&activation_key),
        Some(&signed)
    );
}

/// `pending_action_wal_replays`: a pending faucet grant survives
/// a process boundary. Same shape as the mempool tests but for
/// `PendingFaucetWal`.
#[test]
fn pending_action_wal_replays() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase7_pending.redb");

    let address: [u8; 20] = [0xc3; 20];
    let grant = PendingFaucetGrant {
        address,
        policy_target_micro_usdc: [0; 32],
        evm_target_micro_usdc: [0; 32],
        submitted_at_ms: 1_700_000_000_500,
    };

    {
        let backend = seed_genesis_redb(&path);
        let engine = hydrate_engine_from_backend(&backend).expect("hydrate seed");

        let mut clone = engine.clone();
        clone.pending_faucet_grants.insert(address, grant.clone());

        let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_500)
            .expect("meta");
        let plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan");
        assert_eq!(plan.faucet_wal.puts.len(), 1);
        assert_eq!(plan.faucet_wal.puts[0].0, address.to_vec());
        assert!(plan.faucet_wal.deletes.is_empty());

        apply_durable_commit_plan_with_mode(&backend, plan, SyncMode::Sync).expect("apply");
    }

    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let txn = backend.read_txn().expect("read_txn");
    let replayed = read_faucet_wal(&txn).expect("read faucet WAL");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed.get(&address), Some(&grant));
}

/// `group_commit_visible_after_clean_shutdown`: weakened version
/// of the original Phase 7 named test. `SyncMode::GroupCommit`
/// only guarantees **queued durability** — visibility after a
/// clean shutdown, NOT crash-durability at ack (per redb's
/// `Eventual` contract). The test proves the visibility property
/// only: drop engine + backend cleanly (no crash simulation),
/// reopen, the WAL row IS readable. Callers needing
/// crash-durability at ack must use `SyncMode::Sync`.
#[test]
fn group_commit_visible_after_clean_shutdown() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase7_group_commit.redb");

    let intent_id: [u8; 32] = [0xd4; 32];
    let order_key = IntentOrderKey {
        policy_id: 42,
        nonce: 3,
        intent_id,
    };
    let signed = synthetic_signed_intent_for_test(0xd4, 3);

    {
        let backend = seed_genesis_redb(&path);
        let engine = hydrate_engine_from_backend(&backend).expect("hydrate seed");
        let mut clone = engine.clone();
        clone
            .intent_mempool
            .intents_by_order
            .insert(order_key, signed.clone());
        clone.intent_mempool.known_intents.insert(intent_id);

        let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_001_000)
            .expect("meta");
        let plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan");

        // Apply with GroupCommit. The call returns once redb has
        // queued the transaction; the actual fsync may complete
        // later. Subsequent clean drop guarantees the queued
        // commit reaches disk before reopen — that's what we
        // assert below. We do NOT claim crash-durability here.
        apply_durable_commit_plan_with_mode(
            &backend,
            plan,
            SyncMode::GroupCommit { max_delay_ms: 50 },
        )
        .expect("apply with group_commit");

        // Clean drop: no promote, but no crash simulation either.
        // redb's drop path flushes outstanding transactions.
        drop(clone);
        drop(engine);
        drop(backend);
    }

    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let txn = backend.read_txn().expect("read_txn");
    let replayed = read_direct_wal(&txn).expect("read direct WAL");
    assert_eq!(
        replayed.intents_by_order.get(&order_key),
        Some(&signed),
        "GroupCommit row must be visible after clean shutdown"
    );
}

/// **Phase 7 invariant:** consumed WAL rows are exact-key deleted
/// after the consuming block commits. We seed a WAL row, then
/// apply a durable plan whose `direct_mempool_wal.deletes` lists
/// the row's key. After drop+reopen, the row MUST be gone — no
/// stale entries linger.
#[test]
fn consumed_direct_intent_wal_row_is_exact_key_deleted() {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase7_consumed.redb");

    let intent_id: [u8; 32] = [0xe5; 32];
    let order_key = IntentOrderKey {
        policy_id: 42,
        nonce: 4,
        intent_id,
    };
    let signed = synthetic_signed_intent_for_test(0xe5, 4);

    {
        let backend = seed_genesis_redb(&path);
        let engine = hydrate_engine_from_backend(&backend).expect("hydrate seed");

        // Step 1: admit (poke into mempool, commit durably).
        let mut clone = engine.clone();
        clone
            .intent_mempool
            .intents_by_order
            .insert(order_key, signed.clone());
        clone.intent_mempool.known_intents.insert(intent_id);
        let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_002_000)
            .expect("meta-admit");
        let plan = build_durable_commit_plan(&engine, &clone, meta).expect("plan-admit");
        apply_durable_commit_plan_with_mode(&backend, plan, SyncMode::Sync).expect("apply-admit");

        // Step 2: simulate block consumption — remove the row from
        // the engine and durably commit the resulting state. The
        // diff produces an exact-key WAL delete.
        let engine_after_admission = clone.clone();
        let mut clone_after = clone;
        clone_after
            .intent_mempool
            .intents_by_order
            .remove(&order_key);
        clone_after.intent_mempool.known_intents.remove(&intent_id);
        let meta = engine_commit_meta(
            &clone_after,
            ENGINE_CANONICAL_RECORD_VERSION,
            1_700_000_003_000,
        )
        .expect("meta-consume");
        let plan = build_durable_commit_plan(&engine_after_admission, &clone_after, meta)
            .expect("plan-consume");
        assert_eq!(
            plan.direct_mempool_wal.deletes,
            vec![intent_id.to_vec()],
            "consumption plan must delete by exact intent_id"
        );
        apply_durable_commit_plan_with_mode(&backend, plan, SyncMode::Sync).expect("apply-consume");
    }

    // After drop+reopen, the WAL row is gone.
    let backend = AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("reopen"));
    let txn = backend.read_txn().expect("read_txn");
    let replayed = read_direct_wal(&txn).expect("read direct WAL");
    assert!(
        replayed.intents_by_order.is_empty(),
        "consumed WAL row must NOT replay; got {:?}",
        replayed.intents_by_order
    );
}

/// `wal_diff_value_aware_emits_put_for_changed_value`: same key,
/// different value across before/after must emit a put (not a
/// no-op). Key-only diff would silently drop the update; the
/// value-aware diff catches it and rewrites the WAL row.
/// Faucet WAL is the canonical example — re-submission for the
/// same EVM address can update target amounts without changing
/// the key.
#[test]
fn wal_diff_value_aware_emits_put_for_changed_value() {
    let address: [u8; 20] = [0xf6; 20];
    let mut before = BTreeMap::new();
    let mut after = BTreeMap::new();

    let original = PendingFaucetGrant {
        address,
        policy_target_micro_usdc: [0; 32],
        evm_target_micro_usdc: [0; 32],
        submitted_at_ms: 1_000,
    };
    let updated = PendingFaucetGrant {
        address,
        policy_target_micro_usdc: [0; 32],
        // different value: bump the EVM target.
        evm_target_micro_usdc: [9; 32],
        submitted_at_ms: 2_000,
    };
    before.insert(address, original.clone());
    after.insert(address, updated.clone());

    let mutation = wal_diff_faucet(&before, &after).expect("diff");
    assert_eq!(
        mutation.puts.len(),
        1,
        "same-key/different-value MUST emit one put"
    );
    assert_eq!(mutation.puts[0].0, address.to_vec());
    // Decode the put value back to PendingFaucetGrant and verify
    // it matches `updated` (NOT `original`).
    let decoded: PendingFaucetGrant = borsh::from_slice(&mutation.puts[0].1).expect("decode");
    assert_eq!(decoded, updated);
    assert!(
        mutation.deletes.is_empty(),
        "same-key update must not produce a delete"
    );

    // No-op case: same key, same value → no put, no delete.
    let mut same_before = BTreeMap::new();
    let mut same_after = BTreeMap::new();
    same_before.insert(address, original.clone());
    same_after.insert(address, original);
    let mutation = wal_diff_faucet(&same_before, &same_after).expect("diff");
    assert!(
        mutation.puts.is_empty() && mutation.deletes.is_empty(),
        "same key + same value must be a no-op"
    );
}

// ========================================================================
// Phase 6 — Block-commit durability beachhead (continued)
// ========================================================================

/// `produce_empty_block_durable` propagates engine errors as
/// `DurableCommitError::Node` and never advances the engine when
/// the install on the clone fails. We deliberately use a
/// timestamp of `0` (before genesis at `1_000`) to trigger
/// `BlockValidationError::NonMonotonicTimestamp` from the engine.
#[test]
fn produce_empty_block_durable_does_not_promote_clone_on_install_error() {
    // The simplest reproducible install error: timestamp before
    // parent's timestamp. Genesis timestamp is 1_000; passing
    // `0` triggers `NodeError::InvalidBlock(NonMonotonicTimestamp)`.
    let mut engine = SingleNodeEngine::new(config());
    let backend = AnyStateBackend::InMemory(InMemoryBackend::new());
    let seed_meta = engine_commit_meta(&engine, ENGINE_CANONICAL_RECORD_VERSION, 1_700_000_000_000)
        .expect("seed meta");
    let seed_plan = plan_engine_write(&engine, seed_meta).expect("seed plan");
    apply_plan(&backend, seed_plan).expect("seed apply");
    let pre_snapshot = snapshot_bytes(&engine);

    let err = produce_empty_block_durable(&mut engine, &backend, 0)
        .expect_err("must reject monotonic-timestamp violation");
    assert!(
        matches!(err, DurableCommitError::Node(_)),
        "expected Node error, got {err:?}"
    );

    // Engine is untouched — clone-promote never happened.
    assert_eq!(snapshot_bytes(&engine), pre_snapshot);
}

/// Phase 8 named test: a logical snapshot load over an engine
/// that has been advanced past genesis must be refused unless
/// the caller passes `force_reset = true`. This is the operator
/// safety guarantee that prevents `kombat_loadSnapshot` from
/// silently clobbering durable state on a running node.
#[test]
fn load_snapshot_refuses_populated_backend_without_reset() {
    // Build a populated engine.
    let mut source = SingleNodeEngine::new(config());
    source
        .produce_empty_block(1_700_000_000_000)
        .expect("genesis +1");
    let pre_save_snapshot = snapshot_bytes(&source);

    // Save logical snapshot to a buffer.
    let mut snapshot_bytes_buf: Vec<u8> = Vec::new();
    let saved_meta =
        save_engine_logical_snapshot(&source, 1_700_000_000_001, &mut snapshot_bytes_buf)
            .expect("save logical snapshot");
    assert_eq!(saved_meta.height, 1);

    // Build a target engine, advance it past genesis (so
    // head_height > 0), and try to load WITHOUT force_reset.
    let mut target = SingleNodeEngine::new(config());
    target
        .produce_empty_block(1_700_000_000_500)
        .expect("target +1");
    let pre_load_snapshot = snapshot_bytes(&target);
    let err = load_engine_logical_snapshot(&mut target, snapshot_bytes_buf.as_slice(), false)
        .expect_err("load on populated engine without reset must fail");
    assert!(
        matches!(
            err,
            SnapshotBridgeError::EngineNotFresh {
                reason: EngineNotFreshReason::PostGenesisChain { head_height: 1 }
            }
        ),
        "expected EngineNotFresh{{PostGenesisChain{{head_height:1}}}}, got {err:?}"
    );
    // Engine state must be untouched after the rejected load.
    assert_eq!(snapshot_bytes(&target), pre_load_snapshot);

    // Same load with force_reset = true succeeds and replaces
    // the engine with the source's state.
    load_engine_logical_snapshot(&mut target, snapshot_bytes_buf.as_slice(), true)
        .expect("force_reset load must succeed");
    assert_eq!(snapshot_bytes(&target), pre_save_snapshot);

    // Loading on a *fresh* (genesis-only, height 0, all
    // mempool/WAL buckets empty) engine is allowed without
    // force_reset.
    let mut fresh = SingleNodeEngine::new(config());
    load_engine_logical_snapshot(&mut fresh, snapshot_bytes_buf.as_slice(), false)
        .expect("load on fresh engine without reset must succeed");
    assert_eq!(snapshot_bytes(&fresh), pre_save_snapshot);
}

/// Phase 8: at height 0, an engine that has admitted a signed
/// intent (via `submit_intent`) holds it in the direct mempool +
/// the Phase 7 WAL — that is durable / admitted state that a
/// snapshot load would silently destroy. The freshness check
/// must reject the load without `force_reset` even though
/// `head_height == 0`.
#[test]
fn load_snapshot_refuses_height_zero_with_admitted_mempool_intent() {
    // Source: produce one block so we have something to save.
    let mut source = SingleNodeEngine::new(config());
    source
        .produce_empty_block(1_700_000_000_000)
        .expect("genesis +1");
    let mut snapshot_bytes_buf: Vec<u8> = Vec::new();
    save_engine_logical_snapshot(&source, 1_700_000_000_001, &mut snapshot_bytes_buf)
        .expect("save logical snapshot");

    // Target: brand-new engine at height 0, then simulate the
    // post-admission state by populating `intent_mempool`
    // directly (the same way Phase 7 mempool tests do — going
    // through `submit_intent` here would require registering a
    // policy, but the freshness check only inspects the
    // resulting in-memory state, so direct insertion is
    // equivalent for this test's purpose).
    let mut target = SingleNodeEngine::new(config());
    let intent_id: [u8; 32] = [0xab; 32];
    let signed = synthetic_signed_intent_for_test(0xab, 1);
    target.intent_mempool.intents_by_order.insert(
        IntentOrderKey {
            policy_id: 42,
            nonce: 1,
            intent_id,
        },
        signed,
    );
    target.intent_mempool.known_intents.insert(intent_id);
    assert_eq!(target.head().header.height, 0);
    assert!(!target.intent_mempool.is_empty());
    let pre_load_snapshot = snapshot_bytes(&target);

    // Without force_reset, load must refuse with the mempool
    // reason — not the post-genesis reason.
    let err = load_engine_logical_snapshot(&mut target, snapshot_bytes_buf.as_slice(), false)
        .expect_err("load on height-0 engine with admitted intent must fail");
    assert!(
        matches!(
            err,
            SnapshotBridgeError::EngineNotFresh {
                reason: EngineNotFreshReason::DirectIntentMempoolNonEmpty { count: 1 }
            }
        ),
        "expected DirectIntentMempoolNonEmpty{{count:1}}, got {err:?}"
    );
    assert_eq!(snapshot_bytes(&target), pre_load_snapshot);

    // With force_reset=true, the load proceeds.
    load_engine_logical_snapshot(&mut target, snapshot_bytes_buf.as_slice(), true)
        .expect("force_reset load must succeed even with admitted intent");
    assert!(target.intent_mempool.is_empty());
}

/// Phase 8: at height 0, an engine that has applied a canonical
/// mutation (`create_account`) holds account_store + runtime
/// event state that a snapshot load would silently destroy. The
/// freshness check must reject the load even though height is 0
/// and all WAL/mempool buckets are empty. This guards the
/// `kombat_createAccount` / `kombat_submitOwnerCall` RPC paths
/// against being clobbered by a stray `kombat_loadSnapshot`.
#[test]
fn load_snapshot_refuses_height_zero_with_canonical_account_mutation() {
    // Source: produce one block so we have something to save.
    let mut source = SingleNodeEngine::new(config());
    source
        .produce_empty_block(1_700_000_000_000)
        .expect("genesis +1");
    let mut snapshot_bytes_buf: Vec<u8> = Vec::new();
    save_engine_logical_snapshot(&source, 1_700_000_000_001, &mut snapshot_bytes_buf)
        .expect("save logical snapshot");

    // Target: brand-new engine at height 0, then create an
    // account. This mutates `account_store` and pushes a
    // `RuntimeEventKind::AccountCreated` onto `runtime_events`,
    // both at height 0.
    let mut target = SingleNodeEngine::new(config());
    target
        .create_account(31, [31; 32], [9; 32], 1_150)
        .expect("create_account at height 0");
    assert_eq!(target.head().header.height, 0);
    assert!(!target.account_store.is_empty());
    assert!(!target.runtime_events.is_empty());
    let pre_load_snapshot = snapshot_bytes(&target);

    // Without force_reset, load must refuse. The check fires
    // on `AccountStoreNonEmpty` because that's the first
    // canonical bucket in the predicate's check order — the
    // important property is that we *do* reject and *don't*
    // touch the engine, not which specific reason fires.
    let err = load_engine_logical_snapshot(&mut target, snapshot_bytes_buf.as_slice(), false)
        .expect_err("load on height-0 engine with canonical mutation must fail");
    assert!(
        matches!(
            err,
            SnapshotBridgeError::EngineNotFresh {
                reason: EngineNotFreshReason::AccountStoreNonEmpty { count: 1 }
            }
        ),
        "expected AccountStoreNonEmpty{{count:1}}, got {err:?}"
    );
    assert_eq!(snapshot_bytes(&target), pre_load_snapshot);

    // With force_reset=true, the load proceeds and the
    // canonical mutation is replaced with the source's state.
    load_engine_logical_snapshot(&mut target, snapshot_bytes_buf.as_slice(), true)
        .expect("force_reset load must succeed even with canonical mutation");
    assert!(target.account_store.is_empty());
}
