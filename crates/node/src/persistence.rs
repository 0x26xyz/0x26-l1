//! Engine durable persistence plan and inventory.
//!
//! This module is the engine-side companion of RFC 042 (Durable State
//! Persistence). It pins which fields of `SingleNodeEngine` are
//! `canonical`, `wal`, or `derived`, and points each non-derived field at
//! the backend `Namespace` it lives in.
//!
//! Anything classified `canonical` must participate in the atomic
//! block-commit batch (Phase 5/6). Anything classified `wal` lives in a
//! per-queue WAL namespace and is tombstoned in the same atomic batch as
//! the block that consumes it (Phase 7). Anything classified `derived`
//! must have a deterministic rebuild routine in `from_persisted_snapshot`
//! / Phase 4 hydration, and a byte-identical rebuild test.
//!
//! # Inventory of `PersistedSingleNodeEngineSnapshot` (lib.rs)
//!
//! Canonical, WAL, and derived fields are grouped below. Additional
//! engine-only fields (already absent from the snapshot today) are listed
//! under "engine-only derived" because Phase 4 must keep rebuilding them
//! from the new canonical sources.
//!
//! ## Canonical (atomic with block commit)
//!
//! Consensus / validator-set, namespace `ConsensusSafety`:
//! - `configured_validator_verifiers`
//! - `validator_set`
//! - `validator_verifiers`
//! - `consensus_signing_keys`
//! - `pacemaker_config`
//! - `pacemaker`
//!
//! Block chain / executor state, namespaces `Blocks`, `BlockHashIndex`,
//! `State`, `Quarantine`:
//! - `chain` -> `Blocks` (by height) plus `BlockHashIndex` (hash -> height)
//! - `committed_state_root` -> `Meta`
//! - `committed_entries` -> `State` (executor's `InMemoryStateStore`)
//! - `quarantined_blocks` -> `Quarantine`
//!
//! Engine economic state, namespace `State` with typed prefixes
//! (Phase 4 schema):
//! - `account_store`, `policy_store`
//! - `mandates`, `mandate_action_nonces`, `mandate_realized_pnl`,
//!   `mandate_daily_pnl_baselines`
//! - `account_operators`
//! - `agent_identities`, `agent_identity_tombstones`,
//!   `identity_action_nonces`, `identity_attestor_nonces`,
//!   `agent_attestations`, `agent_attestations_by_account`,
//!   `tee_attestations_by_vendor_root`
//! - `tee_vendor_roots`, `tee_vendor_cap_profiles`,
//!   `scheduled_tee_vendor_cap_profile_updates`
//! - `order_books`, `resting_order_mandates`
//! - `markets`, `market_marks`, `market_index_prices`,
//!   `oracle_feed_submissions`, `market_oracle_config`, `market_risk`,
//!   `market_fees`, `paused_markets`, `market_funding_index`,
//!   `market_last_funding_ms`
//! - `insurance_funds`, `market_bad_debt_balances`,
//!   `account_market_bad_debt`, `fee_vaults`, `validator_reward_pools`,
//!   `market_maker_rebate_pools`, `treasury_balances`,
//!   `frozen_accounts`, `liquidation_escalation`, `isolated_collateral`,
//!   `positions`
//! - `market_maker_programs`, `market_maker_enrollments`,
//!   `live_market_maker_bonds_micro_usdc`,
//!   `processed_market_maker_program_actions`
//! - `delegations`, `delegation_shares`, `real_capital_listings`,
//!   `real_capital_listings_by_agent`, `vault_deposited_micro_usdc`,
//!   `vault_principal_deposited_micro_usdc`,
//!   `delegation_compensation_claims`,
//!   `real_capital_insurance_fund_micro_usdc`,
//!   `protocol_delegation_cap`, `listing_control_state`, `redemption_requests`,
//!   `processed_delegation_actions`,
//!   `processed_real_capital_governance_actions`
//! - `suppress_fill_bankruptcy_reconcile`
//!
//! Events / receipts, namespaces `Events`, `Receipts`, `Meta`:
//! - `runtime_events` -> `Events`
//! - `evm_transaction_receipts` -> `Receipts`
//! - `next_runtime_event_seq` -> `Meta` (authoritative)
//!
//! Local observability, namespace `Telemetry`:
//! - `telemetry` — canonical *local observability*, **non-state-rooted**.
//!   Updated atomically with block/consensus commits where applicable.
//!   Continuity preserved across restart for RPC continuity, but never
//!   feeds the state root.
//!
//! ## WAL (admitted, not yet committed)
//!
//! - `intent_mempool` -> `DirectMempoolWal`
//! - `conditional_pool` -> `ConditionalMempoolWal`
//! - `pending_faucet_grants` -> `PendingFaucetWal`
//! - `pending_delegation_actions` -> `PendingDelegationWal`
//! - `pending_market_maker_program_actions` -> `PendingMmProgramWal`
//! - `pending_tee_governance_actions` -> future `PendingTeeGovernanceWal`
//! - `pending_real_capital_governance_actions` -> future
//!   `PendingRealCapitalGovernanceWal`
//!
//! WAL entries are deleted with exact-key deletes (not prefix scans) in
//! the same atomic batch as the block that consumes them.
//!
//! ## Derived (rebuilt at hydration; must have byte-identical rebuild
//! tests in Phase 4)
//!
//! | Field | Canonical source(s) |
//! |---|---|
//! | `mandates_by_agent` | `mandates` (group by agent) |
//! | `pending_market_maker_program_action_order` | `pending_market_maker_program_actions` |
//! | `pending_real_capital_governance_action_order` | `pending_real_capital_governance_actions` |
//! | `pending_faucet_grant_order` | `pending_faucet_grants` |
//! | `pending_delegation_action_order` | `pending_delegation_actions` |
//! | `under_maintenance_accounts` | `positions` + `market_marks` + `isolated_collateral` + `market_risk` (matches `margin_snapshot(...).under_maintenance`) |
//!
//! ## Engine-only derived (already rebuilt by `from_persisted_snapshot`
//! today; Phase 4 keeps them rebuilt from the new canonical sources)
//!
//! - `mandate_fill_timestamps` <- `runtime_events`
//! - `mandate_objective_event_cache` <- `runtime_events`
//! - `pending_open_delegation_index`,
//!   `pending_open_delegation_agent_index`,
//!   `pending_open_delegation_order`,
//!   `pending_finalize_delegation_index` <- `pending_delegation_actions`
//! - `direct_expiry_purged_through_ms`,
//!   `conditional_expiry_purged_through_ms` -> reset to 0 on hydration
//! - `proposer_frontier` <- via `rebuild_proposer_frontier()`
//!
//! # Phase mapping
//!
//! - Phase 0 (this comment): inventory.
//! - Phase 1: `state-db::backend` trait + in-memory impl.
//! - Phase 2: redb impl behind the same trait.
//! - Phase 3: logical export/import bridge.
//! - Phase 4 (this module): hydration / plan / engine round-trip.
//! - Phase 5: `DurableCommitPlan` produced before fsync.
//! - Phase 6: block-commit wiring (sync commit, then cache install).
//! - Phase 7: WAL admission (sync / group-commit).
//! - Phase 8: `kombat_saveSnapshot`/`loadSnapshot` -> logical export/import.
//! - Phase 9: corpus + bench across enabled backends.

use crate::{
    all_equity_samples_from_txn, all_mandate_conformance_samples_from_txn, equity_history_key,
    mandate_conformance_history_key, pending_delegation_action_order, pending_faucet_grant_order,
    pending_market_maker_program_action_order, ActivationOrderKey, ConditionalIntentPool,
    DirectIntentMempool, IntentOrderKey, IsolatedCollateralAuditRow,
    IsolatedCollateralRepairAdjustment, IsolatedCollateralRepairReport,
    IsolatedCollateralRepairTarget, MarketCustodyAuditMarket, MarketCustodyAuditReport,
    MarketCustodyRepairAdjustment, MarketCustodyRepairReport, PendingDelegationAction,
    PendingFaucetGrant, PendingMarketMakerProgramAction, PersistedSingleNodeEngineSnapshot,
    SingleNodeEngine,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    block_hash, intent_id, DelegationActionId, Hash, MarketMakerProgramActionId, SignedIntent,
};
use kombat_chain_execution::{
    isolated_margin_market_custody_address, policy_usdc_balance_state_key,
};
use kombat_chain_state_db::{
    AnyReadTxn, AnyStateBackend, AnyWriteBatch, BackendError, CheckpointableStateStore, CommitMeta,
    InMemoryStateStore, Namespace, ReadTxn, SyncMode, WriteBatch,
};
use revm::primitives::U256;
use std::collections::{BTreeMap, BTreeSet};

/// Fixed key under `Namespace::State` that holds the engine's canonical
/// borsh-blob record. The transitional `_v1` suffix is part of the
/// schema; bumping it is an explicit migration.
pub(crate) const ENGINE_CANONICAL_KEY: &[u8] = b"engine_canonical_v1";

/// Schema version embedded inside `EngineCanonicalRecord`. Distinct
/// from `CommitMeta.schema_version` so the bridge record can evolve
/// independently of the namespace layout.
pub(crate) const ENGINE_CANONICAL_RECORD_VERSION: u32 = 1;

/// Single-key entry under `Namespace::Telemetry` carrying the
/// borsh-encoded `ConsensusTelemetry`. Local observability only —
/// non-state-rooted.
pub(crate) const TELEMETRY_KEY: &[u8] = b"telemetry_v1";

/// Bridge canonical record. v1 stores the entire
/// `PersistedSingleNodeEngineSnapshot` as a single borsh blob inside
/// `Namespace::State[ENGINE_CANONICAL_KEY]`. WAL fields are explicitly
/// **zeroed** at write time (their truth lives in WAL namespaces);
/// derived fields are also zeroed and rebuilt at hydrate.
///
/// Phase 7 will trim WAL fields off the inner shape entirely; the
/// migration is a `version` bump + a one-shot upgrade routine.
#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub(crate) struct EngineCanonicalRecord {
    pub version: u32,
    pub inner: PersistedSingleNodeEngineSnapshot,
}

/// A staged plan ready to be committed to a backend. Contains every
/// per-namespace write the block commit must perform. Phase 5 promotes
/// this into `DurableCommitPlan`; Phase 6 wires it into the live block
/// commit path.
#[derive(Debug)]
pub(crate) struct EngineDurablePlan {
    pub canonical_bytes: Vec<u8>,
    pub direct_wal: Vec<(Vec<u8>, Vec<u8>)>,
    pub conditional_wal: Vec<(Vec<u8>, Vec<u8>)>,
    pub faucet_wal: Vec<(Vec<u8>, Vec<u8>)>,
    pub delegation_wal: Vec<(Vec<u8>, Vec<u8>)>,
    pub mm_wal: Vec<(Vec<u8>, Vec<u8>)>,
    pub telemetry_bytes: Vec<u8>,
    pub meta: CommitMeta,
}

impl EngineDurablePlan {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn wal_entry_count(&self) -> usize {
        self.direct_wal.len()
            + self.conditional_wal.len()
            + self.faucet_wal.len()
            + self.delegation_wal.len()
            + self.mm_wal.len()
    }
}

/// Persistence-layer errors.
#[derive(Debug)]
pub(crate) enum PersistenceError {
    Backend(BackendError),
    Encode(String),
    Decode(String),
    /// Supplied `CommitMeta` does not match what the engine would have
    /// derived. Refuses to plan or hydrate to keep "block lost / state
    /// kept" mismatches from ever entering a batch.
    MetaMismatch(String),
    /// On hydrate: the canonical record's state root or rebuilt root
    /// disagrees with the committed two-root `CommitMeta`.
    StateRootMismatch {
        expected: Hash,
        actual: Hash,
    },
    /// Schema version on disk doesn't match this binary's expectation.
    SchemaMismatch {
        expected: u32,
        actual: u32,
    },
    /// Required record absent from backend (e.g. fresh open with no
    /// committed canonical record).
    MissingRecord(&'static str),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PersistenceError::Backend(e) => write!(f, "persistence backend: {e}"),
            PersistenceError::Encode(d) => write!(f, "persistence encode: {d}"),
            PersistenceError::Decode(d) => write!(f, "persistence decode: {d}"),
            PersistenceError::MetaMismatch(d) => write!(f, "persistence meta mismatch: {d}"),
            PersistenceError::StateRootMismatch { expected, actual } => write!(
                f,
                "persistence state root mismatch: expected {expected:?}, got {actual:?}"
            ),
            PersistenceError::SchemaMismatch { expected, actual } => write!(
                f,
                "persistence schema mismatch: expected v{expected}, got v{actual}"
            ),
            PersistenceError::MissingRecord(name) => {
                write!(f, "persistence missing record: {name}")
            }
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<BackendError> for PersistenceError {
    fn from(e: BackendError) -> Self {
        PersistenceError::Backend(e)
    }
}

/// Derive the `CommitMeta` an engine should be persisted with at this
/// instant. `timestamp_ms` is provided by the caller (the block commit
/// path knows the wall-clock); everything else comes from the engine.
///
/// Native Mahi deliberately permits the chain-head block root and the
/// executor/economic root to diverge. Persist both roots explicitly so
/// restart validation can check the correct invariant for each side.
pub(crate) fn engine_commit_meta(
    engine: &SingleNodeEngine,
    schema_version: u32,
    timestamp_ms: u64,
) -> Result<CommitMeta, PersistenceError> {
    let head = engine
        .chain
        .last()
        .ok_or_else(|| PersistenceError::MetaMismatch("engine has no chain head".into()))?;
    let executor_root = engine.executor.state_store().committed_root();
    Ok(CommitMeta {
        schema_version,
        height: head.header.height,
        block_hash: block_hash(head),
        chain_head_state_root: head.header.state_root,
        executor_state_root: executor_root,
        event_seq: engine.next_runtime_event_seq,
        timestamp_ms,
    })
}

/// Plan a write of the full engine state plus WAL entries against the
/// supplied `meta`. Validates that `meta` reflects the engine's current
/// head/state-root/event-seq before producing the plan. Returns a plan
/// that the caller can hand to `apply_plan` (Phase 4) or fold into a
/// `DurableCommitPlan` (Phase 5).
pub(crate) fn plan_engine_write(
    engine: &SingleNodeEngine,
    meta: CommitMeta,
) -> Result<EngineDurablePlan, PersistenceError> {
    // Validate the supplied meta matches engine state. The meta tuple
    // is consensus-critical; mismatch here would risk a "block in,
    // state out" recovery footprint.
    let derived = engine_commit_meta(engine, meta.schema_version, meta.timestamp_ms)?;
    if derived.height != meta.height {
        return Err(PersistenceError::MetaMismatch(format!(
            "height: derived {} vs meta {}",
            derived.height, meta.height
        )));
    }
    if derived.block_hash != meta.block_hash {
        return Err(PersistenceError::MetaMismatch(format!(
            "block_hash: derived {:?} vs meta {:?}",
            derived.block_hash, meta.block_hash
        )));
    }
    if derived.chain_head_state_root != meta.chain_head_state_root {
        return Err(PersistenceError::MetaMismatch(format!(
            "chain_head_state_root: derived {:?} vs meta {:?}",
            derived.chain_head_state_root, meta.chain_head_state_root
        )));
    }
    if derived.executor_state_root != meta.executor_state_root {
        return Err(PersistenceError::MetaMismatch(format!(
            "executor_state_root: derived {:?} vs meta {:?}",
            derived.executor_state_root, meta.executor_state_root
        )));
    }
    if derived.event_seq != meta.event_seq {
        return Err(PersistenceError::MetaMismatch(format!(
            "event_seq: derived {} vs meta {}",
            derived.event_seq, meta.event_seq
        )));
    }

    // Build canonical record.
    let mut inner = engine.persisted_snapshot();

    // Phase 4 transitional: WAL fields zeroed in the canonical record.
    // Their truth lives per-id in WAL namespaces.
    inner.intent_mempool = DirectIntentMempool::default();
    inner.conditional_pool = ConditionalIntentPool::default();
    inner.pending_faucet_grants = BTreeMap::new();
    inner.pending_faucet_grant_order = BTreeSet::new();
    inner.pending_delegation_actions = BTreeMap::new();
    inner.pending_delegation_action_order = BTreeSet::new();
    inner.pending_market_maker_program_actions = BTreeMap::new();
    inner.pending_market_maker_program_action_order = BTreeSet::new();

    // Derived fields zeroed; rebuilt at hydrate.
    inner.mandates_by_agent = BTreeMap::new();
    inner.under_maintenance_accounts = BTreeSet::new();

    let record = EngineCanonicalRecord {
        version: ENGINE_CANONICAL_RECORD_VERSION,
        inner,
    };
    let canonical_bytes = borsh::to_vec(&record)
        .map_err(|e| PersistenceError::Encode(format!("EngineCanonicalRecord: {e}")))?;

    // WAL: per-id binary keys, values include enough to rebuild the
    // exact in-memory queues.
    let direct_wal = build_direct_wal(&engine.intent_mempool)?;
    let conditional_wal = build_conditional_wal(&engine.conditional_pool)?;
    let faucet_wal = build_faucet_wal(&engine.pending_faucet_grants)?;
    let delegation_wal = build_delegation_wal(&engine.pending_delegation_actions)?;
    let mm_wal = build_mm_wal(&engine.pending_market_maker_program_actions)?;

    // Telemetry — canonical local observability, non-state-rooted.
    let telemetry_bytes = borsh::to_vec(&engine.telemetry)
        .map_err(|e| PersistenceError::Encode(format!("ConsensusTelemetry: {e}")))?;

    Ok(EngineDurablePlan {
        canonical_bytes,
        direct_wal,
        conditional_wal,
        faucet_wal,
        delegation_wal,
        mm_wal,
        telemetry_bytes,
        meta,
    })
}

/// Apply a planned engine write to the backend in one atomic batch.
///
/// **Phase 4 transitional helper, fresh-backend only.** This emits the
/// current-snapshot WAL state as bare `put`s; it does NOT delete stale
/// WAL entries. Re-applying against a non-empty backend can therefore
/// resurrect WAL rows whose actions were already committed, since
/// nothing scrubs them.
///
/// Phase 5/6 promotes this into `DurableCommitPlan`, which:
/// - performs exact-key WAL deletes for actions consumed by the block,
/// - is bound to the live block-commit path, and
/// - is the only caller in production.
///
/// Until then, callers MUST use `apply_plan` only on backends whose
/// `latest_committed_meta()` is `None` (i.e. fresh) or that they have
/// just produced via a freshly-flushed plan.
pub(crate) fn apply_plan(
    backend: &AnyStateBackend,
    plan: EngineDurablePlan,
) -> Result<(), PersistenceError> {
    let mut batch = backend.batch();
    write_plan_into_batch(&mut batch, &plan);
    backend
        .commit_batch(batch, plan.meta, SyncMode::Sync)
        .map_err(PersistenceError::Backend)
}

/// Initialize a fresh backend from an already-built engine.
///
/// Runtime startup uses this when no durable state exists yet and the
/// node bootstraps from genesis, a local snapshot, or a remote snapshot.
/// Unlike `apply_plan`, this also materializes the current chain/event
/// indexes so the backend immediately satisfies the durable namespace
/// contract before the first post-start block commit.
pub(crate) fn initialize_fresh_backend_from_engine(
    backend: &AnyStateBackend,
    engine: &SingleNodeEngine,
    timestamp_ms: u64,
) -> Result<CommitMeta, PersistenceError> {
    if backend.latest_committed_meta()?.is_some() {
        return Err(PersistenceError::MetaMismatch(
            "refusing to initialize a backend that already has committed meta".into(),
        ));
    }

    let meta = engine_commit_meta(engine, ENGINE_CANONICAL_RECORD_VERSION, timestamp_ms)?;
    let plan = plan_engine_write(engine, meta.clone())?;
    let mut batch = backend.batch();
    write_plan_into_batch(&mut batch, &plan);

    for block in &engine.chain {
        let block_bytes =
            borsh::to_vec(block).map_err(|e| PersistenceError::Encode(format!("block: {e}")))?;
        batch.put(
            Namespace::Blocks,
            &block.header.height.to_be_bytes(),
            &block_bytes,
        );
        batch.put(
            Namespace::BlockHashIndex,
            &block_hash(block),
            &block.header.height.to_be_bytes(),
        );
    }

    for record in &engine.runtime_events {
        let value =
            borsh::to_vec(record).map_err(|e| PersistenceError::Encode(format!("event: {e}")))?;
        batch.put(Namespace::Events, &record.seq.to_be_bytes(), &value);
    }

    for (tx_hash, receipt) in &engine.evm_transaction_receipts {
        let value = borsh::to_vec(receipt)
            .map_err(|e| PersistenceError::Encode(format!("receipt: {e}")))?;
        batch.put(Namespace::Receipts, tx_hash, &value);
    }

    for ((account_id, block_height), sample) in &engine.equity_history {
        let value = borsh::to_vec(sample)
            .map_err(|e| PersistenceError::Encode(format!("equity history: {e}")))?;
        batch.put(
            Namespace::EquityHistory,
            equity_history_key(*account_id, *block_height).as_ref(),
            &value,
        );
    }

    for ((mandate_id, evaluated_at_ms, block_height), sample) in &engine.mandate_conformance_history
    {
        let value = borsh::to_vec(sample)
            .map_err(|e| PersistenceError::Encode(format!("mandate conformance: {e}")))?;
        batch.put(
            Namespace::MandateConformanceHistory,
            mandate_conformance_history_key(*mandate_id, *evaluated_at_ms, *block_height).as_ref(),
            &value,
        );
    }

    backend
        .commit_batch(batch, meta.clone(), SyncMode::Sync)
        .map_err(PersistenceError::Backend)?;
    Ok(meta)
}

fn write_plan_into_batch(batch: &mut AnyWriteBatch, plan: &EngineDurablePlan) {
    batch.put(
        Namespace::State,
        ENGINE_CANONICAL_KEY,
        &plan.canonical_bytes,
    );
    batch.put(Namespace::Telemetry, TELEMETRY_KEY, &plan.telemetry_bytes);
    for (k, v) in &plan.direct_wal {
        batch.put(Namespace::DirectMempoolWal, k, v);
    }
    for (k, v) in &plan.conditional_wal {
        batch.put(Namespace::ConditionalMempoolWal, k, v);
    }
    for (k, v) in &plan.faucet_wal {
        batch.put(Namespace::PendingFaucetWal, k, v);
    }
    for (k, v) in &plan.delegation_wal {
        batch.put(Namespace::PendingDelegationWal, k, v);
    }
    for (k, v) in &plan.mm_wal {
        batch.put(Namespace::PendingMmProgramWal, k, v);
    }
}

// =============================================================================
// Phase 5 — DurableCommitPlan
// =============================================================================
//
// `DurableCommitPlan` is the production block-commit shape that Phase 6 wires
// into the live commit path. Unlike `EngineDurablePlan` (Phase 4 transitional
// helper, fresh-backend only), it carries:
//
//   - All canonical / events / receipts / WAL writes that block commit must
//     produce, computed BEFORE the fsync.
//   - **Exact-key WAL deletes** for every action consumed by THIS block.
//     These tombstones are the difference between "committed action stays in
//     WAL forever" (the Phase 4 footgun) and a real WAL replay model.
//   - The `CommitMeta` tuple bound to this commit.
//
// Apply rule: every plan must be applied via `apply_durable_commit_plan`,
// which writes the entire batch — puts AND deletes — atomically with one
// `SyncMode::Sync` commit. No part of a plan ever fans out across multiple
// commits.

/// Per-namespace WAL mutations: rows newly admitted in this block tick
/// (`puts`), and rows whose action / intent was consumed by THIS block
/// (`deletes`). Both lists use raw binary keys (intent_id, action_id,
/// 20-byte address) — no string hex.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WalMutation {
    pub puts: Vec<(Vec<u8>, Vec<u8>)>,
    pub deletes: Vec<Vec<u8>>,
}

/// Per-namespace puts only (events, receipts, quarantine, consensus
/// safety). Deletes for these namespaces are not part of the block
/// commit critical path; retention/maintenance handles them out of
/// band.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WriteSet {
    pub puts: Vec<(Vec<u8>, Vec<u8>)>,
}

/// Quarantine deltas for this commit. A block can both add a new
/// quarantine record (rejected-block reason) and clear stale ones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QuarantineDelta {
    pub puts: Vec<(Vec<u8>, Vec<u8>)>,
    pub deletes: Vec<Vec<u8>>,
}

/// Concrete production block-commit plan. Computed in full before the
/// backend fsync; applied atomically. After Phase 6 wires this into
/// `install_prepared_block_state`, no post-fsync code may compute new
/// consensus or economic side effects — installing caches becomes a
/// pure projection of the plan.
#[derive(Clone, Debug)]
pub(crate) struct DurableCommitPlan {
    /// `Namespace::State[ENGINE_CANONICAL_KEY]` — borsh-encoded
    /// `EngineCanonicalRecord`. Phase 7 may slice this into typed
    /// prefixes; Phase 5 carries the bridge blob.
    pub canonical_bytes: Vec<u8>,

    /// `Namespace::Blocks[height_be]` and `Namespace::BlockHashIndex
    /// [block_hash]`. Block-record bytes are borsh-encoded `Block`.
    pub block_height: u64,
    pub block_hash: Hash,
    pub block_bytes: Vec<u8>,

    /// `Namespace::Events[event_seq_be]` puts for events emitted by
    /// this block.
    pub events: WriteSet,

    /// `Namespace::Receipts[tx_hash]` puts for EVM receipts emitted by
    /// this block.
    pub receipts: WriteSet,

    /// `Namespace::Quarantine` puts/deletes.
    pub quarantine: QuarantineDelta,

    /// `Namespace::ConsensusSafety` puts (validator set, verifiers,
    /// signing keys, pacemaker config / state). Phase 5 carries them
    /// as opaque (key, value) pairs; Phase 6 schema can split them.
    pub consensus_safety: WriteSet,

    /// Per-id WAL mutations for each WAL namespace.
    pub direct_mempool_wal: WalMutation,
    pub conditional_mempool_wal: WalMutation,
    pub faucet_wal: WalMutation,
    pub delegation_wal: WalMutation,
    pub mm_program_wal: WalMutation,
    pub equity_history: WalMutation,
    pub mandate_conformance_history: WalMutation,

    /// `Namespace::Telemetry[TELEMETRY_KEY]` blob, if updated by this
    /// commit. None = no telemetry change in this batch.
    pub telemetry_bytes: Option<Vec<u8>>,

    /// Authoritative meta tuple bound to this commit. Atomic with
    /// every put/delete in the plan.
    pub meta: CommitMeta,
}

impl DurableCommitPlan {
    /// Number of explicit `put` operations the plan contributes to the
    /// commit batch. **Excludes** the implicit `Namespace::Meta` write
    /// performed by `backend.commit_batch` (the meta tuple is part of
    /// the atomic txn but not counted as a plan put). Total batch put
    /// size = `plan_put_count() + 1`. Used by tests and Phase 9 bench
    /// instrumentation.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn plan_put_count(&self) -> usize {
        let mut n = 0;
        n += 1; // canonical
        n += 2; // block + block-hash-index
        n += self.events.puts.len();
        n += self.receipts.puts.len();
        n += self.quarantine.puts.len();
        n += self.consensus_safety.puts.len();
        n += self.direct_mempool_wal.puts.len();
        n += self.conditional_mempool_wal.puts.len();
        n += self.faucet_wal.puts.len();
        n += self.delegation_wal.puts.len();
        n += self.mm_program_wal.puts.len();
        n += self.equity_history.puts.len();
        n += self.mandate_conformance_history.puts.len();
        if self.telemetry_bytes.is_some() {
            n += 1;
        }
        n
    }

    /// Number of `delete` operations the plan contributes. Exact-key
    /// deletes only — never prefix scans.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn plan_delete_count(&self) -> usize {
        self.quarantine.deletes.len()
            + self.direct_mempool_wal.deletes.len()
            + self.conditional_mempool_wal.deletes.len()
            + self.faucet_wal.deletes.len()
            + self.delegation_wal.deletes.len()
            + self.mm_program_wal.deletes.len()
            + self.equity_history.deletes.len()
            + self.mandate_conformance_history.deletes.len()
    }
}

/// Validate a `DurableCommitPlan` for internal coherence before any
/// batch is built. Rejects:
///
/// - block_height / block_hash that disagree with `meta`
/// - block_bytes that don't decode as a `Block`, or whose decoded
///   header height / hash / chain-head root don't match the plan / meta
/// - canonical_bytes that don't decode as `EngineCanonicalRecord`,
///   carry the wrong schema version, embed an empty chain, or whose
///   embedded head / committed executor root / event seq disagree
///   with the meta. If the embedded `committed_entries` are non-empty,
///   the rebuilt root must match `meta.executor_state_root`.
/// - any WAL namespace whose `puts` and `deletes` overlap on the same
///   key (a malformed plan: the put would silently disappear under
///   put-then-delete batch ordering)
///
/// Phase 6 calls this before `apply_durable_commit_plan` so the live
/// commit path never produces a torn `Blocks` / `State` / `Meta` view.
pub(crate) fn validate_durable_commit_plan(
    plan: &DurableCommitPlan,
) -> Result<(), PersistenceError> {
    if plan.block_height != plan.meta.height {
        return Err(PersistenceError::MetaMismatch(format!(
            "plan.block_height ({}) != meta.height ({})",
            plan.block_height, plan.meta.height
        )));
    }
    if plan.block_hash != plan.meta.block_hash {
        return Err(PersistenceError::MetaMismatch(format!(
            "plan.block_hash ({:?}) != meta.block_hash ({:?})",
            plan.block_hash, plan.meta.block_hash
        )));
    }

    // Decode block_bytes and cross-check height / hash / state_root.
    let decoded: kombat_chain_consensus::Block =
        borsh::from_slice(&plan.block_bytes).map_err(|e| {
            PersistenceError::Decode(format!(
                "DurableCommitPlan.block_bytes failed to decode: {e}"
            ))
        })?;
    if decoded.header.height != plan.block_height {
        return Err(PersistenceError::MetaMismatch(format!(
            "decoded block height {} != plan.block_height {}",
            decoded.header.height, plan.block_height
        )));
    }
    let decoded_hash = block_hash(&decoded);
    if decoded_hash != plan.block_hash {
        return Err(PersistenceError::MetaMismatch(format!(
            "decoded block hash {:?} != plan.block_hash {:?}",
            decoded_hash, plan.block_hash
        )));
    }
    if decoded.header.state_root != plan.meta.chain_head_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: plan.meta.chain_head_state_root,
            actual: decoded.header.state_root,
        });
    }

    // Decode canonical_bytes and cross-check against meta + block.
    // Mirrors the post-fsync hydration validators so a plan can never
    // commit an undecodable / stale State[engine_canonical_v1]
    // alongside a coherent Blocks/BlockHashIndex/Meta view.
    let record: EngineCanonicalRecord = borsh::from_slice(&plan.canonical_bytes).map_err(|e| {
        PersistenceError::Decode(format!(
            "DurableCommitPlan.canonical_bytes failed to decode as EngineCanonicalRecord: {e}"
        ))
    })?;
    if record.version != ENGINE_CANONICAL_RECORD_VERSION {
        return Err(PersistenceError::SchemaMismatch {
            expected: ENGINE_CANONICAL_RECORD_VERSION,
            actual: record.version,
        });
    }
    if record.inner.chain.is_empty() {
        return Err(PersistenceError::Decode(
            "canonical record has empty chain; expected at least a genesis block".into(),
        ));
    }
    let canonical_head = record.inner.chain.last().expect("non-empty chain");
    if canonical_head.header.height != plan.meta.height {
        return Err(PersistenceError::MetaMismatch(format!(
            "canonical head height {} != meta.height {}",
            canonical_head.header.height, plan.meta.height
        )));
    }
    let canonical_head_hash = block_hash(canonical_head);
    if canonical_head_hash != plan.meta.block_hash {
        return Err(PersistenceError::MetaMismatch(format!(
            "canonical head hash {:?} != meta.block_hash {:?}",
            canonical_head_hash, plan.meta.block_hash
        )));
    }
    if canonical_head.header.state_root != plan.meta.chain_head_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: plan.meta.chain_head_state_root,
            actual: canonical_head.header.state_root,
        });
    }
    if record.inner.committed_state_root != plan.meta.executor_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: plan.meta.executor_state_root,
            actual: record.inner.committed_state_root,
        });
    }
    if record.inner.next_runtime_event_seq != plan.meta.event_seq {
        return Err(PersistenceError::MetaMismatch(format!(
            "canonical next_runtime_event_seq {} != meta.event_seq {}",
            record.inner.next_runtime_event_seq, plan.meta.event_seq
        )));
    }
    // If the canonical record materialises committed entries, the
    // rebuilt root must agree with meta. Empty entries are legal at
    // any height (genesis sentinel root or a no-op block keeping the
    // prior root); the cross-checks above already constrain the
    // trusted value in that case.
    if !record.inner.committed_entries.is_empty() {
        let rebuilt_root = InMemoryStateStore::from_entries(record.inner.committed_entries.clone())
            .committed_root();
        if rebuilt_root != plan.meta.executor_state_root {
            return Err(PersistenceError::StateRootMismatch {
                expected: plan.meta.executor_state_root,
                actual: rebuilt_root,
            });
        }
    }

    // WAL puts/deletes must not overlap per-namespace. Same-key
    // put+delete in one batch would silently lose the put under the
    // current batch ordering; we treat overlap as a malformed plan.
    check_wal_no_overlap("DirectMempoolWal", &plan.direct_mempool_wal)?;
    check_wal_no_overlap("ConditionalMempoolWal", &plan.conditional_mempool_wal)?;
    check_wal_no_overlap("PendingFaucetWal", &plan.faucet_wal)?;
    check_wal_no_overlap("PendingDelegationWal", &plan.delegation_wal)?;
    check_wal_no_overlap("PendingMmProgramWal", &plan.mm_program_wal)?;
    check_wal_no_overlap("EquityHistory", &plan.equity_history)?;
    check_wal_no_overlap(
        "MandateConformanceHistory",
        &plan.mandate_conformance_history,
    )?;

    Ok(())
}

fn check_wal_no_overlap(
    namespace: &'static str,
    mutation: &WalMutation,
) -> Result<(), PersistenceError> {
    if mutation.puts.is_empty() || mutation.deletes.is_empty() {
        return Ok(());
    }
    let put_keys: BTreeSet<&[u8]> = mutation.puts.iter().map(|(k, _)| k.as_slice()).collect();
    for delete_key in &mutation.deletes {
        if put_keys.contains(delete_key.as_slice()) {
            return Err(PersistenceError::MetaMismatch(format!(
                "{namespace} plan has overlapping put+delete on key {delete_key:?}"
            )));
        }
    }
    Ok(())
}

/// Apply a `DurableCommitPlan` with `SyncMode::Sync` durability. This
/// is the production block-commit applier; block commit ALWAYS uses
/// `Sync`. Wraps `apply_durable_commit_plan_with_mode`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn apply_durable_commit_plan(
    backend: &AnyStateBackend,
    plan: DurableCommitPlan,
) -> Result<(), PersistenceError> {
    apply_durable_commit_plan_with_mode(backend, plan, SyncMode::Sync)
}

/// Apply a `DurableCommitPlan` against the backend in a single atomic
/// commit, with caller-specified durability `mode`. Phase 7 admission
/// paths use this to opt into `GroupCommit` for testnet throughput;
/// block commit always uses `Sync`.
///
/// Calls `validate_durable_commit_plan` first; an incoherent plan
/// (height/hash/meta drift, mis-decoding `block_bytes`, overlapping
/// WAL put+delete, canonical-record drift) errors out **before** any
/// batch is built. The commit path therefore never produces a torn
/// Blocks / State / Meta view.
///
/// Atomicity contract: every put and delete in the plan happens or
/// none of them happens. The `CommitMeta` tuple is durable iff the
/// rest of the plan is.
///
/// Mode semantics (per RFC 042):
/// - `Sync`: caller blocks until the backend has fsync'd this commit.
///   On `redb`, that maps to `Durability::Immediate` (the only redb
///   mode guaranteed persistent on return). **RPC ack with `Sync`
///   means crash-durable.**
/// - `GroupCommit { max_delay_ms }`: queued durability. The commit is
///   queued for fsync but may NOT be flushed to disk by the time the
///   call returns. Visible after a clean shutdown; **NOT
///   crash-durable at ack**. On `redb` this maps to
///   `Durability::Eventual`. Phase 7 baseline has no real group
///   coordination yet — `max_delay_ms` is a forward-looking knob the
///   backend currently ignores.
/// - `AsyncBatchedTestnet`: testnet only. Same crash-durability tier
///   as `GroupCommit`. The caller MUST NOT claim "ack means durable"
///   to clients.
///
/// Callers that need RPC ack to imply crash-durability MUST use
/// `SyncMode::Sync`.
pub(crate) fn apply_durable_commit_plan_with_mode(
    backend: &AnyStateBackend,
    plan: DurableCommitPlan,
    mode: SyncMode,
) -> Result<(), PersistenceError> {
    validate_durable_commit_plan(&plan)?;
    let mut batch = backend.batch();

    // 1. Canonical engine record.
    batch.put(
        Namespace::State,
        ENGINE_CANONICAL_KEY,
        &plan.canonical_bytes,
    );

    // 2. Block record + hash -> height index.
    batch.put(
        Namespace::Blocks,
        &plan.block_height.to_be_bytes(),
        &plan.block_bytes,
    );
    batch.put(
        Namespace::BlockHashIndex,
        &plan.block_hash,
        &plan.block_height.to_be_bytes(),
    );

    // 3. Events / receipts (puts only; retention out of band).
    for (k, v) in &plan.events.puts {
        batch.put(Namespace::Events, k, v);
    }
    for (k, v) in &plan.receipts.puts {
        batch.put(Namespace::Receipts, k, v);
    }

    // 4. Quarantine deltas.
    for (k, v) in &plan.quarantine.puts {
        batch.put(Namespace::Quarantine, k, v);
    }
    for k in &plan.quarantine.deletes {
        batch.delete(Namespace::Quarantine, k);
    }

    // 5. Consensus safety puts.
    for (k, v) in &plan.consensus_safety.puts {
        batch.put(Namespace::ConsensusSafety, k, v);
    }

    // 6. Per-namespace WAL puts AND exact-key deletes.
    apply_wal_mutation(
        &mut batch,
        Namespace::DirectMempoolWal,
        &plan.direct_mempool_wal,
    );
    apply_wal_mutation(
        &mut batch,
        Namespace::ConditionalMempoolWal,
        &plan.conditional_mempool_wal,
    );
    apply_wal_mutation(&mut batch, Namespace::PendingFaucetWal, &plan.faucet_wal);
    apply_wal_mutation(
        &mut batch,
        Namespace::PendingDelegationWal,
        &plan.delegation_wal,
    );
    apply_wal_mutation(
        &mut batch,
        Namespace::PendingMmProgramWal,
        &plan.mm_program_wal,
    );
    apply_wal_mutation(&mut batch, Namespace::EquityHistory, &plan.equity_history);
    apply_wal_mutation(
        &mut batch,
        Namespace::MandateConformanceHistory,
        &plan.mandate_conformance_history,
    );

    // 7. Telemetry, if updated.
    if let Some(bytes) = plan.telemetry_bytes.as_ref() {
        batch.put(Namespace::Telemetry, TELEMETRY_KEY, bytes);
    }

    // Atomic durable commit — meta lands or nothing lands.
    backend
        .commit_batch(batch, plan.meta, mode)
        .map_err(PersistenceError::Backend)
}

fn apply_wal_mutation(batch: &mut AnyWriteBatch, namespace: Namespace, mutation: &WalMutation) {
    for (k, v) in &mutation.puts {
        batch.put(namespace, k, v);
    }
    for k in &mutation.deletes {
        batch.delete(namespace, k);
    }
}

// =============================================================================
// Phase 6 — Block-commit durability beachhead
// =============================================================================
//
// Phase 6 introduces the production-shape commit path: the engine clones
// itself, installs the prepared block on the clone (using the existing
// memory-install code), builds a `DurableCommitPlan` from the
// before/after diff, fsyncs that plan via the backend, and **only then**
// promotes the clone in place. Kill between fsync and in-memory promote
// recovers the new head from disk on restart; kill before fsync recovers
// the old head. Either way, the database never observes a torn
// state/block/meta view.
//
// Phase 6 ships only the empty-block path — that exercises the full
// durable beachhead (clone → install → diff → commit → promote) with
// zero block-body items. Phase 7 extends to admission-driven blocks
// where real WAL deletes are exercised.

/// Wrapper error for the durable commit path. Wraps both engine-side
/// `NodeError` (block-validation, executor, account, policy, ...) and
/// `PersistenceError` (backend, encode, plan-validation drift).
#[derive(Debug)]
pub enum DurableCommitError {
    Node(crate::NodeError),
    Persistence(PersistenceError),
}

impl std::fmt::Display for DurableCommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DurableCommitError::Node(e) => write!(f, "durable commit (node): {e:?}"),
            DurableCommitError::Persistence(e) => write!(f, "durable commit: {e}"),
        }
    }
}

impl std::error::Error for DurableCommitError {}

impl From<PersistenceError> for DurableCommitError {
    fn from(e: PersistenceError) -> Self {
        DurableCommitError::Persistence(e)
    }
}

impl From<crate::NodeError> for DurableCommitError {
    fn from(e: crate::NodeError) -> Self {
        DurableCommitError::Node(e)
    }
}

/// Build a `DurableCommitPlan` from a before/after engine pair plus
/// authoritative `meta`. Diffs the engine state to produce exact-key
/// WAL deletes for actions consumed by the new block.
///
/// Phase 6 emits:
/// - `canonical_bytes` from `after` (full bridge blob)
/// - block record from `after.head()`
/// - events at `seq >= before.next_runtime_event_seq`
/// - receipts present in `after` but not in `before`
/// - WAL puts: keys in `after` but not in `before`
/// - WAL deletes: keys in `before` but not in `after`
/// - `telemetry_bytes` always carried (continuity across restart)
///
/// `quarantine` and `consensus_safety` are left empty for Phase 6 —
/// neither changes during empty-block production. Phase 7+ wires them
/// when they become non-trivial.
pub(crate) fn build_durable_commit_plan(
    before: &SingleNodeEngine,
    after: &SingleNodeEngine,
    meta: CommitMeta,
) -> Result<DurableCommitPlan, PersistenceError> {
    // 1. Block record from after.
    let head = after
        .chain
        .last()
        .ok_or_else(|| PersistenceError::MetaMismatch("after-engine has empty chain".into()))?;
    let block_bytes =
        borsh::to_vec(head).map_err(|e| PersistenceError::Encode(format!("block: {e}")))?;
    let block_hash_v = block_hash(head);

    // 2. Canonical record bytes from after (reuses Phase 4 builder).
    let canonical_plan = plan_engine_write(after, meta.clone())?;
    let canonical_bytes = canonical_plan.canonical_bytes;

    // 3. Events: any seq >= before.next_runtime_event_seq.
    let event_floor = before.next_runtime_event_seq;
    let mut event_puts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for record in &after.runtime_events {
        if record.seq >= event_floor {
            let key = record.seq.to_be_bytes().to_vec();
            let value = borsh::to_vec(record)
                .map_err(|e| PersistenceError::Encode(format!("event: {e}")))?;
            event_puts.push((key, value));
        }
    }

    // 4. Receipts: any in after.evm_transaction_receipts not in before.
    let mut receipt_puts: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for (tx_hash, receipt) in &after.evm_transaction_receipts {
        if !before.evm_transaction_receipts.contains_key(tx_hash) {
            let value = borsh::to_vec(receipt)
                .map_err(|e| PersistenceError::Encode(format!("receipt: {e}")))?;
            receipt_puts.push((tx_hash.to_vec(), value));
        }
    }

    // 5. WAL diffs per namespace.
    let direct_mempool_wal = wal_diff_intent_pool(
        &before.intent_mempool.intents_by_order,
        &after.intent_mempool.intents_by_order,
        |order_key| order_key.intent_id.to_vec(),
    )?;
    let conditional_mempool_wal = wal_diff_conditional_pool(
        &before.conditional_pool.time_after_by_order,
        &after.conditional_pool.time_after_by_order,
    )?;
    let faucet_wal = wal_diff_faucet(&before.pending_faucet_grants, &after.pending_faucet_grants)?;
    let delegation_wal = wal_diff_delegation(
        &before.pending_delegation_actions,
        &after.pending_delegation_actions,
    )?;
    let mm_program_wal = wal_diff_mm(
        &before.pending_market_maker_program_actions,
        &after.pending_market_maker_program_actions,
    )?;
    let equity_history = wal_diff_value_aware(
        &before.equity_history,
        &after.equity_history,
        |(account_id, block_height)| equity_history_key(*account_id, *block_height).to_vec(),
        |_key, sample| {
            borsh::to_vec(sample)
                .map_err(|e| PersistenceError::Encode(format!("equity history put: {e}")))
        },
    )?;
    let mandate_conformance_history = wal_diff_value_aware(
        &before.mandate_conformance_history,
        &after.mandate_conformance_history,
        |(mandate_id, evaluated_at_ms, block_height)| {
            mandate_conformance_history_key(*mandate_id, *evaluated_at_ms, *block_height).to_vec()
        },
        |_key, sample| {
            borsh::to_vec(sample).map_err(|e| {
                PersistenceError::Encode(format!("mandate conformance history put: {e}"))
            })
        },
    )?;

    // 6. Telemetry continuity.
    let telemetry_bytes = Some(
        borsh::to_vec(&after.telemetry)
            .map_err(|e| PersistenceError::Encode(format!("telemetry: {e}")))?,
    );

    Ok(DurableCommitPlan {
        canonical_bytes,
        block_height: meta.height,
        block_hash: block_hash_v,
        block_bytes,
        events: WriteSet { puts: event_puts },
        receipts: WriteSet { puts: receipt_puts },
        quarantine: QuarantineDelta::default(),
        consensus_safety: WriteSet::default(),
        direct_mempool_wal,
        conditional_mempool_wal,
        faucet_wal,
        delegation_wal,
        mm_program_wal,
        equity_history,
        mandate_conformance_history,
        telemetry_bytes,
        meta,
    })
}

/// Per-namespace WAL diff. Value-aware: a key in both `before` and
/// `after` whose values differ is emitted as a `put` (overwriting the
/// stale row). A key in `before` only is a `delete`. Same key + same
/// value is a no-op. Defensive for any namespace where same-key value
/// updates are possible (e.g. faucet re-submissions); harmless for
/// hash-keyed namespaces where values are immutable per key.
fn wal_diff_value_aware<K, V>(
    before: &BTreeMap<K, V>,
    after: &BTreeMap<K, V>,
    key_bytes: impl Fn(&K) -> Vec<u8>,
    encode_value: impl Fn(&K, &V) -> Result<Vec<u8>, PersistenceError>,
) -> Result<WalMutation, PersistenceError>
where
    K: Ord,
    V: PartialEq,
{
    let mut puts = Vec::new();
    for (key, after_value) in after {
        match before.get(key) {
            None => puts.push((key_bytes(key), encode_value(key, after_value)?)),
            Some(before_value) if before_value != after_value => {
                puts.push((key_bytes(key), encode_value(key, after_value)?));
            }
            _ => {} // unchanged
        }
    }
    let mut deletes = Vec::new();
    for key in before.keys() {
        if !after.contains_key(key) {
            deletes.push(key_bytes(key));
        }
    }
    Ok(WalMutation { puts, deletes })
}

fn wal_diff_intent_pool(
    before: &BTreeMap<IntentOrderKey, SignedIntent>,
    after: &BTreeMap<IntentOrderKey, SignedIntent>,
    key_bytes: impl Fn(&IntentOrderKey) -> Vec<u8>,
) -> Result<WalMutation, PersistenceError> {
    wal_diff_value_aware(
        before,
        after,
        |key| key_bytes(key),
        |key, value| {
            borsh::to_vec(&(*key, value.clone()))
                .map_err(|e| PersistenceError::Encode(format!("direct WAL put: {e}")))
        },
    )
}

fn wal_diff_conditional_pool(
    before: &BTreeMap<ActivationOrderKey, SignedIntent>,
    after: &BTreeMap<ActivationOrderKey, SignedIntent>,
) -> Result<WalMutation, PersistenceError> {
    wal_diff_value_aware(
        before,
        after,
        |key| key.order_key.intent_id.to_vec(),
        |key, value| {
            borsh::to_vec(&(*key, value.clone()))
                .map_err(|e| PersistenceError::Encode(format!("conditional WAL put: {e}")))
        },
    )
}

fn wal_diff_faucet(
    before: &BTreeMap<[u8; 20], PendingFaucetGrant>,
    after: &BTreeMap<[u8; 20], PendingFaucetGrant>,
) -> Result<WalMutation, PersistenceError> {
    wal_diff_value_aware(
        before,
        after,
        |addr| addr.to_vec(),
        |_addr, grant| {
            borsh::to_vec(grant)
                .map_err(|e| PersistenceError::Encode(format!("faucet WAL put: {e}")))
        },
    )
}

fn wal_diff_delegation(
    before: &BTreeMap<DelegationActionId, PendingDelegationAction>,
    after: &BTreeMap<DelegationActionId, PendingDelegationAction>,
) -> Result<WalMutation, PersistenceError> {
    wal_diff_value_aware(
        before,
        after,
        |id| id.to_vec(),
        |_id, action| {
            borsh::to_vec(action)
                .map_err(|e| PersistenceError::Encode(format!("delegation WAL put: {e}")))
        },
    )
}

fn wal_diff_mm(
    before: &BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>,
    after: &BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>,
) -> Result<WalMutation, PersistenceError> {
    wal_diff_value_aware(
        before,
        after,
        |id| id.to_vec(),
        |_id, action| {
            borsh::to_vec(action).map_err(|e| PersistenceError::Encode(format!("MM WAL put: {e}")))
        },
    )
}

/// Produce an empty block on `engine` durably. The contract:
///
/// 1. Clone the engine; install the empty block on the clone (full
///    existing memory-install path runs there).
/// 2. Build a `DurableCommitPlan` from the before/after diff.
/// 3. `apply_durable_commit_plan` on `backend` — `SyncMode::Sync`
///    fsync; meta tuple lands atomically with state/block/events.
/// 4. **Only after** the fsync returns: promote `clone` into `*engine`
///    (the in-memory install).
///
/// A kill between (3) and (4) is recoverable: hydration from `backend`
/// will see the new head and rebuild the engine to match the clone we
/// would have promoted. A kill before (3) is recoverable: the backend
/// is unchanged; restart sees the previous head.
///
/// Phase 6 baseline only wires the empty-block path. Phase 7 extends
/// to admission-driven blocks (real WAL deletes for consumed intents
/// / faucet / delegation / MM-program actions).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn produce_empty_block_durable(
    engine: &mut SingleNodeEngine,
    backend: &AnyStateBackend,
    timestamp_ms: u64,
) -> Result<(), DurableCommitError> {
    // 1. Snapshot pre-state by cloning. Cloning is acceptable for
    //    Phase 6 baseline; Phase 9 may revisit if profiling requires.
    let mut clone = engine.clone();

    // 2. Install on the clone using the existing memory-install path.
    clone
        .produce_empty_block(timestamp_ms)
        .map_err(DurableCommitError::Node)?;

    // 3. Build plan from before/after.
    let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, timestamp_ms)?;
    let plan = build_durable_commit_plan(engine, &clone, meta)?;

    // 4. Durable Sync commit. Disk is the source of truth from here.
    apply_durable_commit_plan(backend, plan)?;

    // 5. Memory install: promote the clone in place.
    *engine = clone;

    Ok(())
}

/// Submit a signed intent durably. Phase 7 admission entry point.
///
/// Order of operations mirrors the Phase 6 block-commit beachhead:
/// 1. Clone the engine.
/// 2. Run `submit_intent` on the clone — this performs full admission
///    validation (signature, nonce, caps, expiry, predicate, etc.).
///    On rejection, the original `engine` is untouched.
/// 3. Build a `DurableCommitPlan` from the before/after diff. The plan
///    carries: an updated canonical record (new event + updated
///    mempool), one new event (`IntentAccepted`), the new WAL row in
///    the appropriate namespace (Direct or Conditional), and meta
///    with `event_seq + 1`. `block_*` references the unchanged head.
/// 4. Submit the plan to the backend per `mode`. Crash durability of
///    the WAL row at the moment (4) returns depends entirely on
///    `mode` — see "Restart semantics" below.
/// 5. Memory install: promote the clone into `*engine`.
///
/// Restart semantics, mode-qualified:
/// - With `SyncMode::Sync`: the WAL row is fsync'd before (4)
///   returns, atomically with the canonical record update. A kill
///   between (4) and (5) recovers the admitted intent on hydrate.
/// - With `SyncMode::GroupCommit` or `AsyncBatchedTestnet`: the WAL
///   row is **not** crash-durable when (4) returns — only queued for
///   the next group/background fsync. Clean shutdown visibility is
///   expected (the row reaches disk before the database file is
///   closed), but **a kill between (4) and (5) may lose the admitted
///   intent**. Do not assume hydrate will recover it. Treat these
///   modes as visibility-only at ack time.
/// - In all modes: a kill before (4) leaves disk unchanged; the
///   in-flight intent is dropped, and the caller did not receive a
///   successful ack.
///
/// `mode` mapping (full crash-durability matrix in `SyncMode` docs):
/// - `Sync`: caller blocks until fsync done. **RPC ack means
///   crash-durable.** Use this when admission durability is part of
///   the user-facing contract.
/// - `GroupCommit { .. }`: queued durability. Visible after clean
///   shutdown; **NOT crash-durable at ack**. Real group coordination
///   is future work — Phase 7 baseline maps this to redb's
///   `Eventual`.
/// - `AsyncBatchedTestnet`: testnet only. RPC ack does NOT imply
///   durability of any kind.
#[allow(dead_code)]
pub(crate) fn submit_intent_durable(
    engine: &mut SingleNodeEngine,
    backend: &AnyStateBackend,
    signed_intent: SignedIntent,
    timestamp_ms: u64,
    mode: SyncMode,
) -> Result<kombat_chain_consensus::IntentId, DurableCommitError> {
    let mut clone = engine.clone();
    let intent_id = clone
        .submit_intent(signed_intent, timestamp_ms)
        .map_err(DurableCommitError::Node)?;

    let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, timestamp_ms)?;
    let plan = build_durable_commit_plan(engine, &clone, meta)?;

    apply_durable_commit_plan_with_mode(backend, plan, mode)?;

    *engine = clone;
    Ok(intent_id)
}

/// Apply an arbitrary engine mutation through the durable clone/diff/fsync
/// path, then promote the clone in memory only after the backend commit
/// succeeds.
///
/// The closure result is deliberately unconstrained. Callers that return
/// `Result<_, NodeError>` still get rejection-side effects persisted when
/// the closure records them before returning `Err`.
pub(crate) fn apply_engine_mutation_durable<T, F>(
    engine: &mut SingleNodeEngine,
    backend: &AnyStateBackend,
    timestamp_ms: u64,
    mode: SyncMode,
    f: F,
) -> Result<T, DurableCommitError>
where
    F: FnOnce(&mut SingleNodeEngine) -> T,
{
    let mut clone = engine.clone();
    let result = f(&mut clone);
    let meta = engine_commit_meta(&clone, ENGINE_CANONICAL_RECORD_VERSION, timestamp_ms)?;
    let plan = build_durable_commit_plan(engine, &clone, meta)?;
    apply_durable_commit_plan_with_mode(backend, plan, mode)?;
    *engine = clone;
    Ok(result)
}

// =============================================================================
// Phase 8 — Snapshot RPC bridge: logical export/import
// =============================================================================
//
// The Phase 3 logical-snapshot format (`logical_snapshot::export` /
// `logical_snapshot::import`) operates on a `StateBackend`. The
// engine doesn't hold a backend in production yet (Phase 4 design
// call), so the snapshot RPC bridge composes Phase 3 + Phase 4 via a
// transient `InMemoryBackend`:
//
//   save: engine -> plan_engine_write -> InMemoryBackend
//                -> logical_snapshot::export -> file
//   load: file   -> logical_snapshot::import -> InMemoryBackend
//                -> hydrate_engine_from_backend -> engine
//
// The transient backend pattern is the simplest way to honour the
// "snapshots use the canonical durable contract" invariant without
// forcing every RPC caller to thread a backend handle.

/// Errors specific to the snapshot RPC bridge.
#[derive(Debug)]
pub(crate) enum SnapshotBridgeError {
    Persistence(PersistenceError),
    Export(kombat_chain_state_db::LogicalSnapshotError),
    Import(kombat_chain_state_db::ImportError),
    /// Engine holds state that loading a snapshot would clobber.
    /// Caller must pass `force_reset = true` to override. The
    /// `reason` distinguishes between post-genesis chain advancement
    /// and admitted-but-uncommitted state at genesis (mempool /
    /// pending WALs). All such buckets must be empty for the load to
    /// be allowed without an explicit reset, otherwise admitted
    /// intents / faucet grants / program actions / delegations would
    /// be silently lost.
    EngineNotFresh {
        reason: EngineNotFreshReason,
    },
}

/// What specifically makes the engine non-fresh. Each variant maps
/// to a kind of mutable state that an unaccompanied snapshot load
/// would destroy. Pattern-matchable so tests can pin the exact
/// branch that triggered.
///
/// Coverage spans the three classes of height-0 mutation that RPC
/// can drive without producing a block:
/// - admitted-but-uncommitted (mempool / Phase 7 WALs),
/// - canonical state mutations (`account_store`, `policy_store` —
///   reachable from `create_account` and `submit_owner_call`), and
/// - the runtime event log itself (`runtime_events` is bumped by
///   *every* state-mutating call — chosen as the catch-all so a
///   future code path that mutates state without obviously hitting
///   one of the buckets above still flips this check).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EngineNotFreshReason {
    /// `engine.head().height > 0`: a block has been committed.
    PostGenesisChain { head_height: u64 },
    /// `submit_intent` admitted at least one signed intent that
    /// is sitting in the direct intent mempool.
    DirectIntentMempoolNonEmpty { count: usize },
    /// At least one conditional intent (`time_after`) is queued.
    ConditionalPoolNonEmpty { count: usize },
    /// `submit_faucet_grant` admitted at least one pending faucet
    /// grant (Phase 7 WAL).
    PendingFaucetGrantsNonEmpty { count: usize },
    /// At least one pending delegation action (deposit / redeem /
    /// finalize) is queued (Phase 7 WAL).
    PendingDelegationActionsNonEmpty { count: usize },
    /// At least one pending market-maker program action is queued
    /// (Phase 7 WAL).
    PendingMarketMakerProgramActionsNonEmpty { count: usize },
    /// `account_store` has at least one account installed at
    /// height 0 (e.g. via `create_account`).
    AccountStoreNonEmpty { count: usize },
    /// `policy_store` has at least one policy registered at
    /// height 0 (e.g. via `submit_owner_call`).
    PolicyStoreNonEmpty { count: usize },
    /// `runtime_events` is non-empty: at least one state-mutating
    /// call has executed at height 0. Catch-all for any mutation
    /// that goes through `record_runtime_event` (the universal
    /// write path for height-0 RPC mutations).
    RuntimeEventsNonEmpty { count: usize },
}

impl std::fmt::Display for EngineNotFreshReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineNotFreshReason::PostGenesisChain { head_height } => {
                write!(f, "engine head height = {head_height} (post-genesis)")
            }
            EngineNotFreshReason::DirectIntentMempoolNonEmpty { count } => {
                write!(f, "{count} admitted direct intents in mempool")
            }
            EngineNotFreshReason::ConditionalPoolNonEmpty { count } => {
                write!(f, "{count} admitted conditional intents")
            }
            EngineNotFreshReason::PendingFaucetGrantsNonEmpty { count } => {
                write!(f, "{count} pending faucet grants")
            }
            EngineNotFreshReason::PendingDelegationActionsNonEmpty { count } => {
                write!(f, "{count} pending delegation actions")
            }
            EngineNotFreshReason::PendingMarketMakerProgramActionsNonEmpty { count } => {
                write!(f, "{count} pending market-maker program actions")
            }
            EngineNotFreshReason::AccountStoreNonEmpty { count } => {
                write!(f, "{count} accounts in account_store")
            }
            EngineNotFreshReason::PolicyStoreNonEmpty { count } => {
                write!(f, "{count} policies in policy_store")
            }
            EngineNotFreshReason::RuntimeEventsNonEmpty { count } => {
                write!(f, "{count} runtime events recorded at height 0")
            }
        }
    }
}

/// Inspect the engine and return the first reason it would be
/// unsafe to clobber via a snapshot load. Order matters: chain
/// advancement is checked first because it's the strongest signal,
/// then the WAL/mempool buckets (admitted-but-uncommitted at height
/// 0), then the canonical stores (`account_store`, `policy_store`),
/// and finally `runtime_events` as the universal catch-all for any
/// other state-mutating call at height 0. Returns `None` only when
/// the engine is genuinely fresh, in which case a load is safe
/// without `force_reset`.
fn engine_not_fresh_reason(engine: &SingleNodeEngine) -> Option<EngineNotFreshReason> {
    let head_height = engine.chain.last().map(|b| b.header.height).unwrap_or(0);
    if head_height != 0 {
        return Some(EngineNotFreshReason::PostGenesisChain { head_height });
    }
    if !engine.intent_mempool.is_empty() {
        return Some(EngineNotFreshReason::DirectIntentMempoolNonEmpty {
            count: engine.intent_mempool.len(),
        });
    }
    if !engine.conditional_pool.is_empty() {
        return Some(EngineNotFreshReason::ConditionalPoolNonEmpty {
            count: engine.conditional_pool.len(),
        });
    }
    if !engine.pending_faucet_grants.is_empty() {
        return Some(EngineNotFreshReason::PendingFaucetGrantsNonEmpty {
            count: engine.pending_faucet_grants.len(),
        });
    }
    if !engine.pending_delegation_actions.is_empty() {
        return Some(EngineNotFreshReason::PendingDelegationActionsNonEmpty {
            count: engine.pending_delegation_actions.len(),
        });
    }
    if !engine.pending_market_maker_program_actions.is_empty() {
        return Some(
            EngineNotFreshReason::PendingMarketMakerProgramActionsNonEmpty {
                count: engine.pending_market_maker_program_actions.len(),
            },
        );
    }
    if !engine.account_store.is_empty() {
        return Some(EngineNotFreshReason::AccountStoreNonEmpty {
            count: engine.account_store.len(),
        });
    }
    if !engine.policy_store.is_empty() {
        return Some(EngineNotFreshReason::PolicyStoreNonEmpty {
            count: engine.policy_store.len(),
        });
    }
    if !engine.runtime_events.is_empty() {
        return Some(EngineNotFreshReason::RuntimeEventsNonEmpty {
            count: engine.runtime_events.len(),
        });
    }
    None
}

impl std::fmt::Display for SnapshotBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotBridgeError::Persistence(e) => write!(f, "snapshot bridge: {e}"),
            SnapshotBridgeError::Export(e) => write!(f, "snapshot bridge export: {e}"),
            SnapshotBridgeError::Import(e) => write!(f, "snapshot bridge import: {e}"),
            SnapshotBridgeError::EngineNotFresh { reason } => write!(
                f,
                "snapshot load refuses populated backend: {reason}; \
                 caller must pass force_reset to override"
            ),
        }
    }
}

impl std::error::Error for SnapshotBridgeError {}

impl From<PersistenceError> for SnapshotBridgeError {
    fn from(e: PersistenceError) -> Self {
        SnapshotBridgeError::Persistence(e)
    }
}

impl From<kombat_chain_state_db::LogicalSnapshotError> for SnapshotBridgeError {
    fn from(e: kombat_chain_state_db::LogicalSnapshotError) -> Self {
        SnapshotBridgeError::Export(e)
    }
}

impl From<kombat_chain_state_db::ImportError> for SnapshotBridgeError {
    fn from(e: kombat_chain_state_db::ImportError) -> Self {
        SnapshotBridgeError::Import(e)
    }
}

/// Export the engine state as a logical snapshot to `writer`.
///
/// Internally: build a Phase 4 canonical write plan from the engine,
/// apply it to a transient `InMemoryBackend`, then run Phase 3
/// `logical_snapshot::export` on that backend. The resulting file is
/// the canonical Phase 3 format with magic, schema version, and
/// CommitMeta header.
///
/// The caller's `timestamp_ms` is the wall-clock timestamp the meta
/// is anchored at — typically `SystemTime::now()` for the snapshot
/// service path.
pub(crate) fn save_engine_logical_snapshot<W: std::io::Write>(
    engine: &SingleNodeEngine,
    timestamp_ms: u64,
    writer: W,
) -> Result<CommitMeta, SnapshotBridgeError> {
    let backend = AnyStateBackend::InMemory(kombat_chain_state_db::InMemoryBackend::new());

    let meta = engine_commit_meta(engine, ENGINE_CANONICAL_RECORD_VERSION, timestamp_ms)?;
    let plan = plan_engine_write(engine, meta.clone())?;
    apply_plan(&backend, plan)?;

    // Phase 3 `export` is generic over `B: StateBackend`. The
    // `AnyStateBackend` enum does not itself implement `StateBackend`
    // (the GAT lifetime doesn't compose across variants — see the
    // comment on `AnyStateBackend`), so call through the concrete
    // inner backend.
    let exported_meta = match &backend {
        AnyStateBackend::InMemory(b) => kombat_chain_state_db::export(b, writer)?,
        AnyStateBackend::Redb(b) => kombat_chain_state_db::export(b, writer)?,
    };
    debug_assert_eq!(exported_meta, meta);
    Ok(exported_meta)
}

/// Import a logical snapshot from `reader` into a fresh engine state.
///
/// Refuses if the current engine holds any state that the load
/// would clobber and `force_reset` is `false`. "Clobber-able state"
/// is not just `head_height > 0` — at height 0 the engine can still
/// hold admitted-but-uncommitted work in the direct mempool, the
/// conditional pool, or any of the Phase 7 pending WALs (faucet
/// grants, delegation actions, market-maker program actions). All
/// of those must be empty for the load to proceed without an
/// explicit reset. This is the "load refuses populated backend
/// without --reset" guarantee from RFC 042.
///
/// Internally: read into a transient `InMemoryBackend` via Phase 3
/// `logical_snapshot::import`, then hydrate a fresh engine via
/// Phase 4 `hydrate_engine_from_backend`. On success, replaces
/// `*engine` with the hydrated engine.
pub(crate) fn load_engine_logical_snapshot<R: std::io::Read>(
    engine: &mut SingleNodeEngine,
    reader: R,
    force_reset: bool,
) -> Result<CommitMeta, SnapshotBridgeError> {
    if !force_reset {
        if let Some(reason) = engine_not_fresh_reason(engine) {
            return Err(SnapshotBridgeError::EngineNotFresh { reason });
        }
    }

    let (hydrated, meta) = load_engine_from_logical_snapshot(reader)?;
    *engine = hydrated;
    Ok(meta)
}

/// Hydrate a fresh engine directly from a Phase 3 logical snapshot stream.
///
/// This is the startup/bootstrap counterpart to `load_engine_logical_snapshot`:
/// the caller has already wiped local state, so there is no populated-engine
/// safety check to perform. It exists so `KOMBAT_LOAD_SNAPSHOT_PATH` can load
/// snapshots produced by `kombat_saveSnapshot`, whose wire format switched from
/// legacy borsh blobs to RFC 042 logical snapshots.
pub(crate) fn load_engine_from_logical_snapshot<R: std::io::Read>(
    reader: R,
) -> Result<(SingleNodeEngine, CommitMeta), SnapshotBridgeError> {
    let backend = AnyStateBackend::InMemory(kombat_chain_state_db::InMemoryBackend::new());
    let meta = match &backend {
        AnyStateBackend::InMemory(b) => kombat_chain_state_db::import(b, reader)?,
        AnyStateBackend::Redb(b) => kombat_chain_state_db::import(b, reader)?,
    };
    let hydrated = hydrate_engine_from_backend(&backend)?;
    Ok((hydrated, meta))
}

/// Hydrate a fresh engine from a populated backend. Validates head /
/// state-root / event-seq against `latest_committed_meta`, replays WAL
/// namespaces into the in-memory queues, rebuilds derived caches, and
/// then runs the same persisted-invariant validators that
/// `SingleNodeEngine::try_from_persisted_snapshot` runs for file
/// snapshots — durable hydration cannot accept any state the snapshot
/// loader would reject.
pub(crate) fn hydrate_engine_from_backend(
    backend: &AnyStateBackend,
) -> Result<SingleNodeEngine, PersistenceError> {
    backend.check_persistence_fingerprint()?.into_result()?;
    let meta = backend
        .latest_committed_meta()?
        .ok_or(PersistenceError::MissingRecord("CommitMeta"))?;
    let txn = backend.read_txn()?;

    // 1. Read canonical record.
    let bytes = txn
        .get(Namespace::State, ENGINE_CANONICAL_KEY)?
        .ok_or(PersistenceError::MissingRecord("engine_canonical_v1"))?;
    let record: EngineCanonicalRecord = borsh::from_slice(&bytes)
        .map_err(|e| PersistenceError::Decode(format!("EngineCanonicalRecord: {e}")))?;
    if record.version != ENGINE_CANONICAL_RECORD_VERSION {
        return Err(PersistenceError::SchemaMismatch {
            expected: ENGINE_CANONICAL_RECORD_VERSION,
            actual: record.version,
        });
    }

    // 2. Reject an empty chain up-front. Hydration always needs at
    //    least a genesis block; without one, the temp-engine
    //    construction below would panic in `head()`. The canonical
    //    snapshot validator (`try_from_persisted_snapshot`) also
    //    requires non-empty chain — we mirror that contract here so
    //    the error path stays in `PersistenceError`.
    if record.inner.chain.is_empty() {
        return Err(PersistenceError::Decode(
            "canonical record has empty chain; expected at least a genesis block".into(),
        ));
    }

    // 3. Cross-validate the two durable roots independently:
    //    chain-head root from meta/head block, and executor root from
    //    meta/canonical committed state/(when present) rebuilt entries.
    //    Any disagreement = refuse to start.
    if record.inner.committed_state_root != meta.executor_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: meta.executor_state_root,
            actual: record.inner.committed_state_root,
        });
    }
    let head_block = record.inner.chain.last().expect("non-empty chain");
    if head_block.header.state_root != meta.chain_head_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: meta.chain_head_state_root,
            actual: head_block.header.state_root,
        });
    }
    if head_block.header.height != meta.height {
        return Err(PersistenceError::MetaMismatch(format!(
            "head height: record {} vs meta {}",
            head_block.header.height, meta.height
        )));
    }
    let head_hash = block_hash(head_block);
    if head_hash != meta.block_hash {
        return Err(PersistenceError::MetaMismatch(format!(
            "head hash: record {:?} vs meta {:?}",
            head_hash, meta.block_hash
        )));
    }
    // Empty `committed_entries` is legal at any height. Two cases:
    // (a) genesis: executor initialised with `with_committed_root(...)`
    //     and no entries materialise a root.
    // (b) post-genesis no-op blocks: state-db's no-op commit semantics
    //     keep the same committed root without producing entries.
    // In both cases the two executor-root sources already
    // cross-checked above constrain the trusted sentinel value. When
    // entries ARE present, we additionally rebuild the root and
    // require it to match — catching any drift in `committed_entries`.
    if !record.inner.committed_entries.is_empty() {
        let rebuilt_root = InMemoryStateStore::from_entries(record.inner.committed_entries.clone())
            .committed_root();
        if rebuilt_root != meta.executor_state_root {
            return Err(PersistenceError::StateRootMismatch {
                expected: meta.executor_state_root,
                actual: rebuilt_root,
            });
        }
    }
    if record.inner.next_runtime_event_seq != meta.event_seq {
        return Err(PersistenceError::MetaMismatch(format!(
            "event_seq: record {} vs meta {}",
            record.inner.next_runtime_event_seq, meta.event_seq
        )));
    }

    // 3. Replay WAL namespaces back into the canonical-record holes.
    let mut inner = record.inner;
    inner.intent_mempool = read_direct_wal(&txn)?;
    inner.conditional_pool = read_conditional_wal(&txn)?;
    prune_committed_intent_wal_replay(&mut inner);
    inner.pending_faucet_grants = read_faucet_wal(&txn)?;
    inner.pending_delegation_actions = read_delegation_wal(&txn)?;
    inner.pending_market_maker_program_actions = read_mm_wal(&txn)?;
    inner.pending_faucet_grant_order = inner
        .pending_faucet_grants
        .values()
        .map(pending_faucet_grant_order)
        .collect();
    inner.pending_delegation_action_order = inner
        .pending_delegation_actions
        .values()
        .map(pending_delegation_action_order)
        .collect();
    inner.pending_market_maker_program_action_order = inner
        .pending_market_maker_program_actions
        .values()
        .map(pending_market_maker_program_action_order)
        .collect();

    // 4. Telemetry (optional; absent on a brand-new backend).
    if let Some(tb) = txn.get(Namespace::Telemetry, TELEMETRY_KEY)? {
        inner.telemetry = borsh::from_slice(&tb)
            .map_err(|e| PersistenceError::Decode(format!("ConsensusTelemetry: {e}")))?;
    }

    // 5. Rebuild derived fields IN the snapshot before validation.
    //    `mandates_by_agent` is a pure rebuild over snapshot.mandates;
    //    `under_maintenance_accounts` needs an engine (margin_snapshot
    //    is an engine method), so we construct a temp engine, compute
    //    the set, and write it back into the snapshot.
    inner.mandates_by_agent = rebuild_mandates_by_agent_pure(&inner.mandates);
    let temp_engine = SingleNodeEngine::from_persisted_snapshot(inner.clone());
    inner.under_maintenance_accounts = compute_under_maintenance_accounts(&temp_engine);
    drop(temp_engine);

    // 6. Run the same validation surface as the file-snapshot path.
    //    `try_from_persisted_snapshot` does:
    //      - persisted-snapshot header validation (validator set,
    //        chain non-empty, configured set, telemetry vs head, ...)
    //      - construction via `from_persisted_snapshot`
    //      - `validate_persisted_order_books` / `_positions`
    //        / `_market_state` / `_mandates` / `_risk_state`
    //    Any drift here is rejected at hydration, identical to a
    //    bad file snapshot.
    let mut engine = SingleNodeEngine::try_from_persisted_snapshot(inner)
        .map_err(|e| PersistenceError::Decode(format!("validate_persisted: {e}")))?;
    let equity_samples = all_equity_samples_from_txn(&txn).map_err(PersistenceError::Backend)?;
    engine.set_equity_history_samples(equity_samples);
    let conformance_samples =
        all_mandate_conformance_samples_from_txn(&txn).map_err(PersistenceError::Backend)?;
    engine.set_mandate_conformance_history_samples(conformance_samples);
    Ok(engine)
}

/// One-shot RFC 053 compatibility repair for stopped redb backends that
/// already contain a market-custody EVM balance but whose canonical
/// Rust market ledgers predate the custody invariant.
///
/// The committed EVM state root and block hash are deliberately left
/// untouched. A repair is allowed only when the custody address has
/// excess backing; the excess is moved into the market treasury bucket
/// inside the canonical engine record so the RFC 053 invariant becomes
/// true without rewriting consensus block metadata. Deficits fail
/// closed and require an explicit recapitalization/reset path.
pub(crate) fn repair_market_custody_in_backend(
    backend: &AnyStateBackend,
    dry_run: bool,
) -> Result<MarketCustodyRepairReport, PersistenceError> {
    let (meta, mut inner) = read_canonical_inner_for_maintenance(backend)?;
    let mut engine = SingleNodeEngine::from_persisted_snapshot(inner.clone());
    let mut report = MarketCustodyRepairReport {
        dry_run,
        adjustments: Vec::new(),
    };

    for market_id in market_ids_for_custody_repair(&engine) {
        let Some(custody_balance) = market_custody_balance_from_entries(&inner, market_id)? else {
            continue;
        };
        if custody_balance == 0 {
            return Err(PersistenceError::Decode(format!(
                "market custody repair: zero custody entry for market {market_id}"
            )));
        }
        let required_before = engine
            .market_custody_required_balance(market_id)
            .map_err(|reason| PersistenceError::Decode(reason.to_string()))?;
        let required_before = u128::try_from(required_before).map_err(|_| {
            PersistenceError::Decode(format!(
                "market custody repair: negative required balance for market {market_id}"
            ))
        })?;
        if required_before == custody_balance {
            continue;
        }
        if required_before > custody_balance {
            return Err(PersistenceError::Decode(format!(
                "market custody repair: market {market_id} has custody deficit \
                 required={required_before} custody={custody_balance}"
            )));
        }

        let treasury_before = inner
            .treasury_balances
            .get(&market_id)
            .copied()
            .unwrap_or(0);
        let delta = custody_balance - required_before;
        let treasury_after = treasury_before.checked_add(delta).ok_or_else(|| {
            PersistenceError::Decode(format!(
                "market custody repair: treasury overflow for market {market_id}"
            ))
        })?;
        inner.treasury_balances.insert(market_id, treasury_after);
        engine.treasury_balances.insert(market_id, treasury_after);
        report.adjustments.push(MarketCustodyRepairAdjustment {
            market_id,
            custody_balance_micro_usdc: custody_balance,
            required_before_micro_usdc: required_before,
            treasury_before_micro_usdc: treasury_before,
            treasury_after_micro_usdc: treasury_after,
        });
    }

    let mut validated_inner = inner.clone();
    canonicalize_inner_for_backend_write(&mut validated_inner);
    SingleNodeEngine::try_from_persisted_snapshot(inner).map_err(|error| {
        PersistenceError::Decode(format!("validate repaired snapshot: {error}"))
    })?;

    if dry_run || report.adjustments.is_empty() {
        return Ok(report);
    }

    let record = EngineCanonicalRecord {
        version: ENGINE_CANONICAL_RECORD_VERSION,
        inner: validated_inner,
    };
    let canonical_bytes = borsh::to_vec(&record)
        .map_err(|error| PersistenceError::Encode(format!("EngineCanonicalRecord: {error}")))?;
    let mut batch = backend.batch();
    batch.put(Namespace::State, ENGINE_CANONICAL_KEY, &canonical_bytes);
    backend
        .commit_batch(batch, meta, SyncMode::Sync)
        .map_err(PersistenceError::Backend)?;
    hydrate_engine_from_backend(backend)?;
    Ok(report)
}

pub(crate) fn audit_market_custody_in_backend(
    backend: &AnyStateBackend,
    top_rows: usize,
) -> Result<MarketCustodyAuditReport, PersistenceError> {
    let (_meta, inner) = read_canonical_inner_for_maintenance(backend)?;
    let engine = SingleNodeEngine::from_persisted_snapshot(inner.clone());
    audit_market_custody_from_inner(&inner, &engine, top_rows)
}

pub(crate) fn repair_isolated_collateral_in_backend(
    backend: &AnyStateBackend,
    dry_run: bool,
    targets: &[IsolatedCollateralRepairTarget],
    top_rows: usize,
) -> Result<IsolatedCollateralRepairReport, PersistenceError> {
    if targets.is_empty() {
        return Err(PersistenceError::Decode(
            "isolated collateral repair: at least one --set target is required".into(),
        ));
    }

    let (meta, mut inner) = read_canonical_inner_for_maintenance(backend)?;
    let mut engine = SingleNodeEngine::from_persisted_snapshot(inner.clone());
    let mut collateral_adjustments = Vec::new();

    for target in targets {
        let key = (target.market_id, target.account_id);
        let before = inner.isolated_collateral.get(&key).copied().unwrap_or(0);
        let after = target.collateral_micro_usdc;
        if before == after {
            continue;
        }
        if after == 0 {
            inner.isolated_collateral.remove(&key);
            engine.isolated_collateral.remove(&key);
        } else {
            inner.isolated_collateral.insert(key, after);
            engine.isolated_collateral.insert(key, after);
        }
        collateral_adjustments.push(IsolatedCollateralRepairAdjustment {
            market_id: target.market_id,
            account_id: target.account_id,
            before_micro_usdc: before,
            after_micro_usdc: after,
        });
    }

    let mut custody_adjustments = Vec::new();
    for market_id in market_ids_for_custody_repair(&engine) {
        let required_after = engine
            .market_custody_required_balance(market_id)
            .map_err(|reason| PersistenceError::Decode(reason.to_string()))?;
        let required_after = u128::try_from(required_after).map_err(|_| {
            PersistenceError::Decode(format!(
                "isolated collateral repair: negative required balance for market {market_id}"
            ))
        })?;
        let Some(custody_balance) = market_custody_balance_from_entries(&inner, market_id)? else {
            if required_after == 0 {
                continue;
            }
            return Err(PersistenceError::Decode(format!(
                "isolated collateral repair: market {market_id} has no custody entry \
                 but requires {required_after}"
            )));
        };
        if custody_balance == 0 && required_after != 0 {
            return Err(PersistenceError::Decode(format!(
                "isolated collateral repair: zero custody entry for market {market_id}"
            )));
        }
        if required_after > custody_balance {
            return Err(PersistenceError::Decode(format!(
                "isolated collateral repair: market {market_id} has custody deficit \
                 required={required_after} custody={custody_balance}"
            )));
        }
        if custody_balance == required_after {
            continue;
        }

        let treasury_before = inner
            .treasury_balances
            .get(&market_id)
            .copied()
            .unwrap_or(0);
        let delta = custody_balance - required_after;
        let treasury_after = treasury_before.checked_add(delta).ok_or_else(|| {
            PersistenceError::Decode(format!(
                "isolated collateral repair: treasury overflow for market {market_id}"
            ))
        })?;
        inner.treasury_balances.insert(market_id, treasury_after);
        engine.treasury_balances.insert(market_id, treasury_after);
        custody_adjustments.push(MarketCustodyRepairAdjustment {
            market_id,
            custody_balance_micro_usdc: custody_balance,
            required_before_micro_usdc: required_after,
            treasury_before_micro_usdc: treasury_before,
            treasury_after_micro_usdc: treasury_after,
        });
    }

    inner.under_maintenance_accounts = compute_under_maintenance_accounts(&engine);
    let after_audit = audit_market_custody_from_inner(&inner, &engine, top_rows)?;
    let mut validated_inner = inner.clone();
    canonicalize_inner_for_backend_write(&mut validated_inner);
    SingleNodeEngine::try_from_persisted_snapshot(inner).map_err(|error| {
        PersistenceError::Decode(format!(
            "validate isolated collateral repair snapshot: {error}"
        ))
    })?;

    if dry_run || (collateral_adjustments.is_empty() && custody_adjustments.is_empty()) {
        return Ok(IsolatedCollateralRepairReport {
            dry_run,
            collateral_adjustments,
            custody_adjustments,
            after_audit,
        });
    }

    let record = EngineCanonicalRecord {
        version: ENGINE_CANONICAL_RECORD_VERSION,
        inner: validated_inner,
    };
    let canonical_bytes = borsh::to_vec(&record)
        .map_err(|error| PersistenceError::Encode(format!("EngineCanonicalRecord: {error}")))?;
    let mut batch = backend.batch();
    batch.put(Namespace::State, ENGINE_CANONICAL_KEY, &canonical_bytes);
    backend
        .commit_batch(batch, meta, SyncMode::Sync)
        .map_err(PersistenceError::Backend)?;
    hydrate_engine_from_backend(backend)?;
    Ok(IsolatedCollateralRepairReport {
        dry_run,
        collateral_adjustments,
        custody_adjustments,
        after_audit,
    })
}

fn read_canonical_inner_for_maintenance(
    backend: &AnyStateBackend,
) -> Result<(CommitMeta, PersistedSingleNodeEngineSnapshot), PersistenceError> {
    backend.check_persistence_fingerprint()?.into_result()?;
    let meta = backend
        .latest_committed_meta()?
        .ok_or(PersistenceError::MissingRecord("CommitMeta"))?;
    let txn = backend.read_txn()?;
    let bytes = txn
        .get(Namespace::State, ENGINE_CANONICAL_KEY)?
        .ok_or(PersistenceError::MissingRecord("engine_canonical_v1"))?;
    let record: EngineCanonicalRecord = borsh::from_slice(&bytes)
        .map_err(|error| PersistenceError::Decode(format!("EngineCanonicalRecord: {error}")))?;
    if record.version != ENGINE_CANONICAL_RECORD_VERSION {
        return Err(PersistenceError::SchemaMismatch {
            expected: ENGINE_CANONICAL_RECORD_VERSION,
            actual: record.version,
        });
    }
    if record.inner.chain.is_empty() {
        return Err(PersistenceError::Decode(
            "canonical record has empty chain; expected at least a genesis block".into(),
        ));
    }
    if record.inner.committed_state_root != meta.executor_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: meta.executor_state_root,
            actual: record.inner.committed_state_root,
        });
    }
    let head_block = record.inner.chain.last().expect("non-empty chain");
    if head_block.header.state_root != meta.chain_head_state_root {
        return Err(PersistenceError::StateRootMismatch {
            expected: meta.chain_head_state_root,
            actual: head_block.header.state_root,
        });
    }
    if head_block.header.height != meta.height {
        return Err(PersistenceError::MetaMismatch(format!(
            "head height: record {} vs meta {}",
            head_block.header.height, meta.height
        )));
    }
    let head_hash = block_hash(head_block);
    if head_hash != meta.block_hash {
        return Err(PersistenceError::MetaMismatch(format!(
            "head hash: record {:?} vs meta {:?}",
            head_hash, meta.block_hash
        )));
    }
    if !record.inner.committed_entries.is_empty() {
        let rebuilt_root = InMemoryStateStore::from_entries(record.inner.committed_entries.clone())
            .committed_root();
        if rebuilt_root != meta.executor_state_root {
            return Err(PersistenceError::StateRootMismatch {
                expected: meta.executor_state_root,
                actual: rebuilt_root,
            });
        }
    }
    if record.inner.next_runtime_event_seq != meta.event_seq {
        return Err(PersistenceError::MetaMismatch(format!(
            "event_seq: record {} vs meta {}",
            record.inner.next_runtime_event_seq, meta.event_seq
        )));
    }

    let mut inner = record.inner;
    inner.intent_mempool = read_direct_wal(&txn)?;
    inner.conditional_pool = read_conditional_wal(&txn)?;
    prune_committed_intent_wal_replay(&mut inner);
    inner.pending_faucet_grants = read_faucet_wal(&txn)?;
    inner.pending_delegation_actions = read_delegation_wal(&txn)?;
    inner.pending_market_maker_program_actions = read_mm_wal(&txn)?;
    inner.pending_faucet_grant_order = inner
        .pending_faucet_grants
        .values()
        .map(pending_faucet_grant_order)
        .collect();
    inner.pending_delegation_action_order = inner
        .pending_delegation_actions
        .values()
        .map(pending_delegation_action_order)
        .collect();
    inner.pending_market_maker_program_action_order = inner
        .pending_market_maker_program_actions
        .values()
        .map(pending_market_maker_program_action_order)
        .collect();
    if let Some(tb) = txn.get(Namespace::Telemetry, TELEMETRY_KEY)? {
        inner.telemetry = borsh::from_slice(&tb)
            .map_err(|error| PersistenceError::Decode(format!("ConsensusTelemetry: {error}")))?;
    }
    inner.mandates_by_agent = rebuild_mandates_by_agent_pure(&inner.mandates);
    let temp_engine = SingleNodeEngine::from_persisted_snapshot(inner.clone());
    inner.under_maintenance_accounts = compute_under_maintenance_accounts(&temp_engine);
    Ok((meta, inner))
}

fn prune_committed_intent_wal_replay(inner: &mut PersistedSingleNodeEngineSnapshot) {
    let stale_direct_keys = inner
        .intent_mempool
        .intents_by_order
        .iter()
        .filter_map(|(key, signed_intent)| {
            let committed_next_nonce = inner
                .policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .ok()?;
            (signed_intent.intent.nonce < committed_next_nonce).then_some(*key)
        })
        .collect::<Vec<_>>();
    for key in stale_direct_keys {
        inner.intent_mempool.intents_by_order.remove(&key);
    }
    inner.intent_mempool.known_intents = inner
        .intent_mempool
        .intents_by_order
        .values()
        .map(intent_id)
        .collect();

    let stale_conditional_keys = inner
        .conditional_pool
        .time_after_by_order
        .iter()
        .filter_map(|(key, signed_intent)| {
            let committed_next_nonce = inner
                .policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .ok()?;
            (signed_intent.intent.nonce < committed_next_nonce).then_some(*key)
        })
        .collect::<Vec<_>>();
    for key in stale_conditional_keys {
        inner.conditional_pool.time_after_by_order.remove(&key);
    }

    let mut queued_policy_nonces = BTreeSet::new();
    let duplicate_direct_keys = inner
        .intent_mempool
        .intents_by_order
        .iter()
        .filter_map(|(key, signed_intent)| {
            if key.intent_id != intent_id(signed_intent)
                || key.policy_id != signed_intent.intent.policy_id
                || key.nonce != signed_intent.intent.nonce
            {
                return None;
            }
            let policy_nonce = (signed_intent.intent.policy_id, signed_intent.intent.nonce);
            (!queued_policy_nonces.insert(policy_nonce)).then_some(*key)
        })
        .collect::<Vec<_>>();
    for key in duplicate_direct_keys {
        inner.intent_mempool.intents_by_order.remove(&key);
    }

    let duplicate_conditional_keys = inner
        .conditional_pool
        .time_after_by_order
        .iter()
        .filter_map(|(key, signed_intent)| {
            if key.order_key.intent_id != intent_id(signed_intent)
                || key.order_key.policy_id != signed_intent.intent.policy_id
                || key.order_key.nonce != signed_intent.intent.nonce
            {
                return None;
            }
            let policy_nonce = (signed_intent.intent.policy_id, signed_intent.intent.nonce);
            (!queued_policy_nonces.insert(policy_nonce)).then_some(*key)
        })
        .collect::<Vec<_>>();
    for key in duplicate_conditional_keys {
        inner.conditional_pool.time_after_by_order.remove(&key);
    }

    inner.intent_mempool.known_intents = inner
        .intent_mempool
        .intents_by_order
        .values()
        .map(intent_id)
        .collect();
    inner.conditional_pool.known_intents = inner
        .conditional_pool
        .time_after_by_order
        .values()
        .map(intent_id)
        .collect();
}

fn canonicalize_inner_for_backend_write(inner: &mut PersistedSingleNodeEngineSnapshot) {
    inner.intent_mempool = DirectIntentMempool::default();
    inner.conditional_pool = ConditionalIntentPool::default();
    inner.pending_faucet_grants = BTreeMap::new();
    inner.pending_faucet_grant_order = BTreeSet::new();
    inner.pending_delegation_actions = BTreeMap::new();
    inner.pending_delegation_action_order = BTreeSet::new();
    inner.pending_market_maker_program_actions = BTreeMap::new();
    inner.pending_market_maker_program_action_order = BTreeSet::new();
    inner.mandates_by_agent = BTreeMap::new();
    inner.under_maintenance_accounts = BTreeSet::new();
}

fn audit_market_custody_from_inner(
    inner: &PersistedSingleNodeEngineSnapshot,
    engine: &SingleNodeEngine,
    top_rows: usize,
) -> Result<MarketCustodyAuditReport, PersistenceError> {
    let mut markets = Vec::new();
    for market_id in market_ids_for_custody_repair(engine) {
        let custody_balance = market_custody_balance_from_entries(inner, market_id)?;
        let required_result = engine.market_custody_required_balance(market_id);
        let (required_balance, required_error) = match required_result {
            Ok(required) => (Some(required), None),
            Err(reason) => (None, Some(reason.to_string())),
        };
        let custody_surplus = match (custody_balance, required_balance) {
            (Some(custody), Some(required)) if custody <= i128::MAX as u128 => {
                Some((custody as i128).saturating_sub(required))
            }
            _ => None,
        };

        let mut positive_isolated_collateral = 0_i128;
        let mut negative_isolated_collateral = 0_i128;
        let mut top_isolated_collateral = Vec::new();
        for ((entry_market_id, account_id), collateral) in engine
            .isolated_collateral
            .range((market_id, 0)..=(market_id, u64::MAX))
        {
            if *entry_market_id != market_id {
                continue;
            }
            if *collateral >= 0 {
                positive_isolated_collateral = positive_isolated_collateral
                    .checked_add(*collateral)
                    .ok_or_else(|| {
                        PersistenceError::Decode(format!(
                            "market custody audit: positive collateral overflow for market {market_id}"
                        ))
                    })?;
            } else {
                negative_isolated_collateral = negative_isolated_collateral
                    .checked_add(*collateral)
                    .ok_or_else(|| {
                        PersistenceError::Decode(format!(
                            "market custody audit: negative collateral overflow for market {market_id}"
                        ))
                    })?;
            }
            top_isolated_collateral.push(IsolatedCollateralAuditRow {
                market_id: *entry_market_id,
                account_id: *account_id,
                collateral_micro_usdc: *collateral,
            });
        }
        top_isolated_collateral.sort_by(|left, right| {
            right
                .collateral_micro_usdc
                .unsigned_abs()
                .cmp(&left.collateral_micro_usdc.unsigned_abs())
                .then_with(|| left.market_id.cmp(&right.market_id))
                .then_with(|| left.account_id.cmp(&right.account_id))
        });
        top_isolated_collateral.truncate(top_rows);

        markets.push(MarketCustodyAuditMarket {
            market_id,
            custody_balance_micro_usdc: custody_balance,
            required_balance_micro_usdc: required_balance,
            custody_surplus_micro_usdc: custody_surplus,
            required_error,
            positive_isolated_collateral_micro_usdc: positive_isolated_collateral,
            negative_isolated_collateral_micro_usdc: negative_isolated_collateral,
            fee_vault_micro_usdc: engine.fee_vault_balance(market_id),
            insurance_fund_micro_usdc: engine.insurance_fund_balance(market_id),
            validator_reward_pool_micro_usdc: engine.validator_reward_pool_balance(market_id),
            market_maker_rebate_pool_micro_usdc: engine.market_maker_rebate_pool_balance(market_id),
            treasury_micro_usdc: engine.treasury_balance(market_id),
            top_isolated_collateral,
        });
    }

    Ok(MarketCustodyAuditReport { markets })
}

fn market_ids_for_custody_repair(engine: &SingleNodeEngine) -> BTreeSet<u64> {
    engine
        .market_marks
        .keys()
        .copied()
        .chain(engine.markets.keys().copied())
        .chain(engine.market_index_prices.keys().copied())
        .chain(
            engine
                .oracle_feed_submissions
                .keys()
                .map(|(market_id, _, _)| *market_id),
        )
        .chain(engine.market_oracle_config.keys().copied())
        .chain(engine.market_risk.keys().copied())
        .chain(engine.market_fees.keys().copied())
        .chain(engine.paused_markets.iter().copied())
        .chain(engine.market_funding_index.keys().copied())
        .chain(engine.market_last_funding_ms.keys().copied())
        .chain(engine.insurance_funds.keys().copied())
        .chain(engine.market_bad_debt_balances.keys().copied())
        .chain(engine.fee_vaults.keys().copied())
        .chain(engine.validator_reward_pools.keys().copied())
        .chain(engine.market_maker_rebate_pools.keys().copied())
        .chain(engine.treasury_balances.keys().copied())
        .chain(engine.order_books.keys().copied())
        .chain(engine.positions.keys().map(|(market_id, _)| *market_id))
        .chain(
            engine
                .isolated_collateral
                .keys()
                .map(|(market_id, _)| *market_id),
        )
        .collect()
}

fn market_custody_balance_from_entries(
    inner: &PersistedSingleNodeEngineSnapshot,
    market_id: u64,
) -> Result<Option<u128>, PersistenceError> {
    let key = policy_usdc_balance_state_key(isolated_margin_market_custody_address(market_id));
    let Some((_, bytes)) = inner
        .committed_entries
        .iter()
        .find(|(entry_key, _)| *entry_key == key)
    else {
        return Ok(None);
    };
    let bytes = <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| {
        PersistenceError::Decode(format!(
            "market custody repair: malformed custody balance for market {market_id}"
        ))
    })?;
    let balance = U256::from_be_bytes(bytes);
    if balance > U256::from(u128::MAX) {
        return Err(PersistenceError::Decode(format!(
            "market custody repair: custody balance too large for market {market_id}"
        )));
    }
    let mut low = [0_u8; 16];
    low.copy_from_slice(&balance.to_be_bytes::<32>()[16..]);
    Ok(Some(u128::from_be_bytes(low)))
}

// ---------------------------------------------------------------- WAL build

fn build_direct_wal(
    pool: &DirectIntentMempool,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PersistenceError> {
    let mut out = Vec::with_capacity(pool.intents_by_order.len());
    for (order_key, signed) in &pool.intents_by_order {
        let key = order_key.intent_id.to_vec();
        let value = borsh::to_vec(&(*order_key, signed.clone()))
            .map_err(|e| PersistenceError::Encode(format!("direct WAL entry: {e}")))?;
        out.push((key, value));
    }
    Ok(out)
}

fn build_conditional_wal(
    pool: &ConditionalIntentPool,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PersistenceError> {
    let mut out = Vec::with_capacity(pool.time_after_by_order.len());
    for (order_key, signed) in &pool.time_after_by_order {
        let key = order_key.order_key.intent_id.to_vec();
        let value = borsh::to_vec(&(*order_key, signed.clone()))
            .map_err(|e| PersistenceError::Encode(format!("conditional WAL entry: {e}")))?;
        out.push((key, value));
    }
    Ok(out)
}

fn build_faucet_wal(
    grants: &BTreeMap<[u8; 20], PendingFaucetGrant>,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PersistenceError> {
    let mut out = Vec::with_capacity(grants.len());
    for (addr, grant) in grants {
        let key = addr.to_vec();
        let value = borsh::to_vec(grant)
            .map_err(|e| PersistenceError::Encode(format!("faucet WAL entry: {e}")))?;
        out.push((key, value));
    }
    Ok(out)
}

fn build_delegation_wal(
    actions: &BTreeMap<DelegationActionId, PendingDelegationAction>,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PersistenceError> {
    let mut out = Vec::with_capacity(actions.len());
    for (id, action) in actions {
        let key = id.to_vec();
        let value = borsh::to_vec(action)
            .map_err(|e| PersistenceError::Encode(format!("delegation WAL entry: {e}")))?;
        out.push((key, value));
    }
    Ok(out)
}

fn build_mm_wal(
    actions: &BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>,
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, PersistenceError> {
    let mut out = Vec::with_capacity(actions.len());
    for (id, action) in actions {
        let key = id.to_vec();
        let value = borsh::to_vec(action)
            .map_err(|e| PersistenceError::Encode(format!("MM WAL entry: {e}")))?;
        out.push((key, value));
    }
    Ok(out)
}

// ---------------------------------------------------------------- WAL read

/// Helper: WAL key bytes must equal the canonical id bytes embedded
/// in the value. Catches stale or malformed rows on hydrate (e.g. a
/// row written under one id but whose payload claims another).
fn check_id_key(
    namespace: &'static str,
    key: &[u8],
    expected: &[u8],
) -> Result<(), PersistenceError> {
    if key != expected {
        return Err(PersistenceError::Decode(format!(
            "{namespace} WAL key/value identity mismatch: key={key:?}, value-derived={expected:?}"
        )));
    }
    Ok(())
}

fn read_direct_wal(txn: &AnyReadTxn) -> Result<DirectIntentMempool, PersistenceError> {
    let mut pool = DirectIntentMempool::default();
    for entry in txn.iter_prefix(Namespace::DirectMempoolWal, b"")? {
        let (key, value) = entry?;
        let (order_key, signed): (IntentOrderKey, SignedIntent) = borsh::from_slice(&value)
            .map_err(|e| PersistenceError::Decode(format!("direct WAL entry: {e}")))?;
        check_id_key("direct mempool", &key, &order_key.intent_id)?;
        pool.known_intents.insert(order_key.intent_id);
        pool.intents_by_order.insert(order_key, signed);
    }
    Ok(pool)
}

fn read_conditional_wal(txn: &AnyReadTxn) -> Result<ConditionalIntentPool, PersistenceError> {
    let mut pool = ConditionalIntentPool::default();
    for entry in txn.iter_prefix(Namespace::ConditionalMempoolWal, b"")? {
        let (key, value) = entry?;
        let (order_key, signed): (ActivationOrderKey, SignedIntent) = borsh::from_slice(&value)
            .map_err(|e| PersistenceError::Decode(format!("conditional WAL entry: {e}")))?;
        check_id_key("conditional mempool", &key, &order_key.order_key.intent_id)?;
        pool.known_intents.insert(order_key.order_key.intent_id);
        pool.time_after_by_order.insert(order_key, signed);
    }
    Ok(pool)
}

fn read_faucet_wal(
    txn: &AnyReadTxn,
) -> Result<BTreeMap<[u8; 20], PendingFaucetGrant>, PersistenceError> {
    let mut out = BTreeMap::new();
    for entry in txn.iter_prefix(Namespace::PendingFaucetWal, b"")? {
        let (key, value) = entry?;
        let grant: PendingFaucetGrant = borsh::from_slice(&value)
            .map_err(|e| PersistenceError::Decode(format!("faucet WAL entry: {e}")))?;
        check_id_key("faucet", &key, &grant.address)?;
        out.insert(grant.address, grant);
    }
    Ok(out)
}

fn read_delegation_wal(
    txn: &AnyReadTxn,
) -> Result<BTreeMap<DelegationActionId, PendingDelegationAction>, PersistenceError> {
    let mut out = BTreeMap::new();
    for entry in txn.iter_prefix(Namespace::PendingDelegationWal, b"")? {
        let (key, value) = entry?;
        let action: PendingDelegationAction = borsh::from_slice(&value)
            .map_err(|e| PersistenceError::Decode(format!("delegation WAL entry: {e}")))?;
        check_id_key("delegation", &key, &action.action_id)?;
        out.insert(action.action_id, action);
    }
    Ok(out)
}

fn read_mm_wal(
    txn: &AnyReadTxn,
) -> Result<BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>, PersistenceError>
{
    let mut out = BTreeMap::new();
    for entry in txn.iter_prefix(Namespace::PendingMmProgramWal, b"")? {
        let (key, value) = entry?;
        let action: PendingMarketMakerProgramAction = borsh::from_slice(&value)
            .map_err(|e| PersistenceError::Decode(format!("MM WAL entry: {e}")))?;
        check_id_key("MM program", &key, &action.action_id)?;
        out.insert(action.action_id, action);
    }
    Ok(out)
}

// ------------------------------------------------------------- Derived rebuilds

/// Pure rebuild of `mandates_by_agent` from a `mandates` map. Used at
/// hydration time before validation so the snapshot's secondary index
/// matches what the live engine maintains.
fn rebuild_mandates_by_agent_pure(
    mandates: &BTreeMap<[u8; 32], crate::Mandate>,
) -> BTreeMap<u64, BTreeSet<[u8; 32]>> {
    let mut by_agent: BTreeMap<u64, BTreeSet<[u8; 32]>> = BTreeMap::new();
    for (mandate_id, mandate) in mandates {
        by_agent
            .entry(mandate.agent)
            .or_default()
            .insert(*mandate_id);
    }
    by_agent
}

/// Compute `under_maintenance_accounts` by querying `margin_snapshot`
/// over the SAME key set that `validate_persisted_risk_state` uses:
/// `positions`, `isolated_collateral`, `account_market_bad_debt`, and
/// `liquidation_escalation`. An account with no position can still be
/// under maintenance — for example after collateral has been drawn
/// negative or moved out — so positions alone is not enough.
fn compute_under_maintenance_accounts(engine: &SingleNodeEngine) -> BTreeSet<(u64, u64)> {
    let mut keys: BTreeSet<(u64, u64)> = BTreeSet::new();
    keys.extend(engine.positions.keys().copied());
    keys.extend(engine.isolated_collateral.keys().copied());
    keys.extend(engine.account_market_bad_debt.keys().copied());
    keys.extend(engine.liquidation_escalation.keys().copied());

    let mut set: BTreeSet<(u64, u64)> = BTreeSet::new();
    for (market_id, account_id) in keys {
        if engine
            .margin_snapshot(market_id, account_id)
            .under_maintenance
        {
            set.insert((market_id, account_id));
        }
    }
    set
}

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod persistence_tests;
