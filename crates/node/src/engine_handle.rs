use crate::{
    Block, BlockItem, ConsensusMetricsSnapshot, IntentAdmissionPreview, ProposerFrontierStats,
    SingleNodeEngine,
};
use arc_swap::ArcSwap;
use kombat_chain_consensus::{
    block_hash, mahi::MahiBlock, BlockDelegation, BlockFaucet, BlockMarketMakerProgram,
    BlockOraclePrice, BlockRealCapitalGovernance, BlockTeeGovernance, DelegationActionId, Hash,
    IntentId, MarketMakerProgramActionId, RealCapitalGovernanceActionId, SignedIntent,
    TeeGovernanceActionId, ValidatorId, ValidatorVerifier,
};
use kombat_chain_policy::PolicyStore;
use kombat_chain_state_db::{AnyStateBackend, SyncMode};
use parking_lot::{Mutex as ParkingMutex, RwLock};
#[cfg(debug_assertions)]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, LockResult, Mutex as StdMutex, MutexGuard as StdMutexGuard, PoisonError};
use std::time::{Duration, Instant};

const MAHI_PAYLOAD_SHARDING_ENV: &str = "KOMBAT_MAHI_PAYLOAD_SHARDING";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockDomain {
    Policy,
    Dag,
    Mempool,
    MarketState,
    ChainTail,
    Pacemaker,
    Events,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadSnapshot {
    pub chain_id: u64,
    pub head_height: u64,
    pub head_round: u64,
    pub head_hash: Hash,
    pub committed_state_root: Hash,
    pub head_timestamp_ms: u64,
    pub current_round: u64,
    pub local_validator_id: Option<ValidatorId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProducedBlockSnapshot {
    pub height: u64,
    pub hash: Hash,
    pub state_root: Hash,
    pub timestamp_ms: u64,
    pub item_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MahiProducerSnapshot {
    pub chain_id: u64,
    pub head_height: u64,
    pub local_validator_id: ValidatorId,
    pub local_consensus_signing_key: [u8; 32],
    pub validator_set: Vec<ValidatorId>,
    pub validator_verifiers: Vec<ValidatorVerifier>,
    pub oracle_prices: Vec<BlockOraclePrice>,
    pub intents: Vec<SignedIntent>,
    pub faucets: Vec<BlockFaucet>,
    pub delegations: Vec<BlockDelegation>,
    pub market_maker_programs: Vec<BlockMarketMakerProgram>,
    pub tee_governance: Vec<BlockTeeGovernance>,
    pub real_capital_governance: Vec<BlockRealCapitalGovernance>,
}

#[derive(Debug, Default)]
pub struct PolicyDomain {
    lock: ParkingMutex<()>,
}

#[derive(Debug, Default)]
pub struct DagDomain {
    lock: ParkingMutex<()>,
}

#[derive(Debug)]
pub struct MempoolDomain {
    lock: ParkingMutex<()>,
    proposer_frontier_stats: RwLock<ProposerFrontierStats>,
}

#[derive(Debug, Default)]
pub struct MarketStateDomain {
    lock: ParkingMutex<()>,
}

#[derive(Debug)]
pub struct ChainTailDomain {
    lock: ParkingMutex<()>,
    head_snapshot: ArcSwap<HeadSnapshot>,
}

#[derive(Debug)]
pub struct PacemakerDomain {
    lock: ParkingMutex<()>,
    current_round: RwLock<u64>,
    consensus_metrics: RwLock<ConsensusMetricsSnapshot>,
}

#[derive(Debug, Default)]
pub struct EventsDomain {
    lock: ParkingMutex<()>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LockTimingSamples {
    pub wait_ns: Vec<u64>,
    pub hold_ns: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineLockProfileSnapshot {
    pub compat_raw: LockTimingSamples,
    pub admission_read: LockTimingSamples,
    pub admission_write: LockTimingSamples,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineLockProfileKind {
    CompatRaw,
    AdmissionRead,
    AdmissionWrite,
}

#[derive(Debug)]
enum EngineCompat {
    ParkingLot(Arc<ParkingMutex<SingleNodeEngine>>),
    Legacy(Arc<StdMutex<SingleNodeEngine>>),
}

#[derive(Debug)]
pub struct EngineHandle {
    compat: EngineCompat,
    durable_backend: Option<Arc<AnyStateBackend>>,
    admission_sync_mode: SyncMode,
    policy: PolicyDomain,
    dag: DagDomain,
    mempool: MempoolDomain,
    market_state: MarketStateDomain,
    chain_tail: ChainTailDomain,
    pacemaker: PacemakerDomain,
    events: EventsDomain,
    lock_profile: RwLock<EngineLockProfileSnapshot>,
}

impl EngineHandle {
    pub fn new(engine: SingleNodeEngine) -> Self {
        Self::from_compat(EngineCompat::ParkingLot(Arc::new(ParkingMutex::new(
            engine,
        ))))
    }

    pub fn new_durable(
        engine: SingleNodeEngine,
        backend: Arc<AnyStateBackend>,
        admission_sync_mode: SyncMode,
    ) -> Self {
        let mut handle = Self::new(engine);
        handle.durable_backend = Some(backend);
        handle.admission_sync_mode = admission_sync_mode;
        handle
    }

    pub fn from_legacy_mutex(
        engine: Arc<StdMutex<SingleNodeEngine>>,
    ) -> Result<Self, &'static str> {
        match engine.lock() {
            Ok(guard) => Ok(Self::from_snapshots(
                EngineCompat::Legacy(engine.clone()),
                build_head_snapshot(&guard),
                guard.metrics().clone(),
                guard.proposer_frontier_stats(),
                guard.current_round(),
            )),
            Err(_) => Err("legacy engine mutex poisoned"),
        }
    }

    pub fn lock(&self) -> LockResult<EngineCompatGuard<'_>> {
        self.compat_lock(Some(EngineLockProfileKind::CompatRaw))
    }

    pub fn lock_profile_snapshot(&self) -> EngineLockProfileSnapshot {
        self.lock_profile.read().clone()
    }

    fn compat_lock(
        &self,
        profile_kind: Option<EngineLockProfileKind>,
    ) -> LockResult<EngineCompatGuard<'_>> {
        let wait_started = Instant::now();
        match &self.compat {
            EngineCompat::ParkingLot(engine) => {
                let guard = engine.lock();
                let acquired_at = Instant::now();
                Ok(EngineCompatGuard {
                    handle: self,
                    guard: EngineCompatGuardInner::ParkingLot(guard),
                    acquired_at,
                    wait_ns: duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                    profile_kind,
                })
            }
            EngineCompat::Legacy(engine) => match engine.lock() {
                Ok(guard) => {
                    let acquired_at = Instant::now();
                    Ok(EngineCompatGuard {
                        handle: self,
                        guard: EngineCompatGuardInner::Legacy(guard),
                        acquired_at,
                        wait_ns: duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                        profile_kind,
                    })
                }
                Err(poisoned) => {
                    let acquired_at = Instant::now();
                    Err(PoisonError::new(EngineCompatGuard {
                        handle: self,
                        guard: EngineCompatGuardInner::Legacy(poisoned.into_inner()),
                        acquired_at,
                        wait_ns: duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                        profile_kind,
                    }))
                }
            },
        }
    }

    pub fn head_snapshot(&self) -> Arc<HeadSnapshot> {
        self.chain_tail.head_snapshot.load_full()
    }

    pub fn current_round_snapshot(&self) -> u64 {
        *self.pacemaker.current_round.read()
    }

    pub fn consensus_metrics_snapshot(&self) -> ConsensusMetricsSnapshot {
        self.pacemaker.consensus_metrics.read().clone()
    }

    pub fn proposer_frontier_stats_snapshot(&self) -> ProposerFrontierStats {
        *self.mempool.proposer_frontier_stats.read()
    }

    pub fn preview_submit_intent(
        &self,
        signed_intent: &SignedIntent,
        timestamp_ms: u64,
    ) -> LockResult<Result<IntentAdmissionPreview, crate::NodeError>> {
        self.with_admission_read(|engine| engine.preview_submit_intent(signed_intent, timestamp_ms))
    }

    pub fn submit_intent(
        &self,
        signed_intent: SignedIntent,
        timestamp_ms: u64,
    ) -> LockResult<Result<IntentId, crate::NodeError>> {
        let _policy = self.policy.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        self.with_engine_mutation(
            timestamp_ms,
            self.admission_sync_mode,
            Some(EngineLockProfileKind::AdmissionWrite),
            |engine| engine.submit_intent(signed_intent, timestamp_ms),
            durable_error_result,
        )
    }

    pub fn submit_intents(
        &self,
        submissions: Vec<(SignedIntent, u64)>,
    ) -> LockResult<Vec<Result<IntentId, crate::NodeError>>> {
        let mutation_timestamp_ms = submissions
            .iter()
            .map(|(_, timestamp_ms)| *timestamp_ms)
            .max()
            .unwrap_or(0);
        let submission_count = submissions.len();
        let _policy = self.policy.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        self.with_engine_mutation(
            mutation_timestamp_ms,
            self.admission_sync_mode,
            Some(EngineLockProfileKind::AdmissionWrite),
            |engine| {
                submissions
                    .into_iter()
                    .map(|(signed_intent, timestamp_ms)| {
                        engine.submit_intent(signed_intent, timestamp_ms)
                    })
                    .collect()
            },
            |error| {
                let node_error = durable_node_error(error);
                (0..submission_count)
                    .map(|_| Err(node_error.clone()))
                    .collect()
            },
        )
    }

    pub fn with_durable_write<T, F, P>(
        &self,
        timestamp_ms: u64,
        mode: SyncMode,
        domains: &[LockDomain],
        f: F,
        on_durable_error: P,
    ) -> LockResult<T>
    where
        F: FnOnce(&mut SingleNodeEngine) -> T,
        P: FnOnce(crate::NodeError) -> T,
    {
        let _policy = domains
            .contains(&LockDomain::Policy)
            .then(|| self.policy.track_lock());
        let _dag = domains
            .contains(&LockDomain::Dag)
            .then(|| self.dag.track_lock());
        let _mempool = domains
            .contains(&LockDomain::Mempool)
            .then(|| self.mempool.track_lock());
        let _market_state = domains
            .contains(&LockDomain::MarketState)
            .then(|| self.market_state.track_lock());
        let _chain_tail = domains
            .contains(&LockDomain::ChainTail)
            .then(|| self.chain_tail.track_lock());
        let _pacemaker = domains
            .contains(&LockDomain::Pacemaker)
            .then(|| self.pacemaker.track_lock());
        let _events = domains
            .contains(&LockDomain::Events)
            .then(|| self.events.track_lock());
        self.with_engine_mutation(timestamp_ms, mode, None, f, |error| {
            on_durable_error(durable_node_error(error))
        })
    }

    pub fn with_policy_read<T, F>(&self, f: F) -> LockResult<T>
    where
        F: FnOnce(&PolicyStore) -> T,
    {
        let _policy = self.policy.track_lock();
        match self.compat_lock(None) {
            Ok(engine) => Ok(f(&engine.policy_store)),
            Err(poisoned) => Err(PoisonError::new(f(&poisoned.into_inner().policy_store))),
        }
    }

    pub fn policy_domain(&self) -> &PolicyDomain {
        &self.policy
    }

    pub fn mempool_domain(&self) -> &MempoolDomain {
        &self.mempool
    }

    pub fn dag_domain(&self) -> &DagDomain {
        &self.dag
    }

    pub fn market_state_domain(&self) -> &MarketStateDomain {
        &self.market_state
    }

    pub fn chain_tail_domain(&self) -> &ChainTailDomain {
        &self.chain_tail
    }

    pub fn pacemaker_domain(&self) -> &PacemakerDomain {
        &self.pacemaker
    }

    pub fn events_domain(&self) -> &EventsDomain {
        &self.events
    }

    #[allow(clippy::result_large_err)]
    pub fn mahi_snapshot(
        &self,
        timestamp_ms: u64,
        max_items: usize,
        inflight_policy_nonces: &BTreeMap<u64, BTreeSet<u64>>,
    ) -> LockResult<Option<MahiProducerSnapshot>> {
        self.with_admission_read(|engine| {
            let local_validator_id = engine.local_validator_id()?;
            let local_consensus_signing_key = engine.local_consensus_signing_key()?;
            let block_number = engine.head().header.height.saturating_add(1);
            let configured_validator_set = engine.configured_validator_ids();
            let validator_set = if configured_validator_set.len() > 1 {
                configured_validator_set
            } else {
                engine.validator_set().to_vec()
            };
            let shard_index = validator_set
                .iter()
                .position(|validator_id| *validator_id == local_validator_id)
                .unwrap_or(0);
            let body = if mahi_payload_sharding_enabled(validator_set.len()) {
                engine.intent_mempool.select_mahi_block_body_from_frontier(
                    &engine.policy_store,
                    timestamp_ms,
                    max_items,
                    &engine.proposer_frontier,
                    shard_index,
                    validator_set.len(),
                    inflight_policy_nonces,
                )
            } else {
                engine
                    .intent_mempool
                    .plan_block_body_from_frontier(
                        &engine.policy_store,
                        &engine.mandates,
                        &engine.delegations,
                        &engine.account_operators,
                        &engine.order_books,
                        &engine.resting_order_mandates,
                        &engine.frozen_accounts,
                        &engine.markets,
                        &engine.paused_markets,
                        &engine.market_marks,
                        &engine.market_index_prices,
                        &engine.market_oracle_config,
                        &engine.isolated_collateral,
                        &engine.positions,
                        engine.head().header.chain_id,
                        block_number,
                        timestamp_ms,
                        max_items,
                        &engine.proposer_frontier,
                    )
                    .body
            };
            let intents = body
                .items
                .into_iter()
                .filter_map(|item| match item {
                    BlockItem::Intent(signed_intent) => Some(signed_intent),
                    BlockItem::Transaction(_)
                    | BlockItem::OraclePrice(_)
                    | BlockItem::Funding(_)
                    | BlockItem::Faucet(_)
                    | BlockItem::Liquidation(_)
                    | BlockItem::Delegation(_)
                    | BlockItem::MarketMakerProgram(_)
                    | BlockItem::TeeGovernance(_)
                    | BlockItem::RealCapitalGovernance(_) => None,
                })
                .collect();
            Some(MahiProducerSnapshot {
                chain_id: engine.head().header.chain_id,
                head_height: engine.head().header.height,
                local_validator_id,
                local_consensus_signing_key,
                validator_set,
                validator_verifiers: engine.validator_verifiers.clone(),
                oracle_prices: engine.mahi_oracle_price_payload(),
                intents,
                faucets: Vec::new(),
                delegations: Vec::new(),
                market_maker_programs: Vec::new(),
                tee_governance: Vec::new(),
                real_capital_governance: Vec::new(),
            })
        })
    }

    pub fn pending_delegation_payload(
        &self,
        max_items: usize,
        excluded: &BTreeSet<DelegationActionId>,
    ) -> LockResult<Vec<BlockDelegation>> {
        self.with_admission_read(|engine| {
            engine
                .pending_delegation_block_items(max_items)
                .into_iter()
                .filter_map(|item| match item {
                    BlockItem::Delegation(delegation)
                        if !excluded.contains(&delegation.action_id) =>
                    {
                        Some(delegation)
                    }
                    BlockItem::Delegation(_) => None,
                    BlockItem::Intent(_)
                    | BlockItem::Transaction(_)
                    | BlockItem::OraclePrice(_)
                    | BlockItem::Funding(_)
                    | BlockItem::Faucet(_)
                    | BlockItem::Liquidation(_)
                    | BlockItem::MarketMakerProgram(_)
                    | BlockItem::TeeGovernance(_)
                    | BlockItem::RealCapitalGovernance(_) => {
                        unreachable!("pending delegation payload contained non-delegation item")
                    }
                })
                .collect()
        })
    }

    pub fn pending_faucet_payload(&self, max_items: usize) -> LockResult<Vec<BlockFaucet>> {
        self.with_admission_read(|engine| {
            engine
                .pending_faucet_block_items(max_items)
                .into_iter()
                .filter_map(|item| match item {
                    BlockItem::Faucet(faucet) => Some(faucet),
                    BlockItem::Intent(_)
                    | BlockItem::Transaction(_)
                    | BlockItem::OraclePrice(_)
                    | BlockItem::Funding(_)
                    | BlockItem::Liquidation(_)
                    | BlockItem::Delegation(_)
                    | BlockItem::MarketMakerProgram(_)
                    | BlockItem::TeeGovernance(_)
                    | BlockItem::RealCapitalGovernance(_) => {
                        unreachable!("pending faucet payload contained non-faucet item")
                    }
                })
                .collect()
        })
    }

    pub fn pending_market_maker_program_payload(
        &self,
        max_items: usize,
        excluded: &BTreeSet<MarketMakerProgramActionId>,
    ) -> LockResult<Vec<BlockMarketMakerProgram>> {
        self.with_admission_read(|engine| {
            engine
                .pending_market_maker_program_block_items(max_items)
                .into_iter()
                .filter_map(|item| match item {
                    BlockItem::MarketMakerProgram(program)
                        if !excluded.contains(&program.action_id) =>
                    {
                        Some(program)
                    }
                    BlockItem::MarketMakerProgram(_) => None,
                    BlockItem::Intent(_)
                    | BlockItem::Transaction(_)
                    | BlockItem::OraclePrice(_)
                    | BlockItem::Funding(_)
                    | BlockItem::Faucet(_)
                    | BlockItem::Liquidation(_)
                    | BlockItem::Delegation(_)
                    | BlockItem::TeeGovernance(_)
                    | BlockItem::RealCapitalGovernance(_) => {
                        unreachable!(
                            "pending market-maker program payload contained non-market-maker item"
                        )
                    }
                })
                .collect()
        })
    }

    pub fn pending_tee_governance_payload(
        &self,
        max_items: usize,
        excluded: &BTreeSet<TeeGovernanceActionId>,
    ) -> LockResult<Vec<BlockTeeGovernance>> {
        self.with_admission_read(|engine| {
            engine
                .pending_tee_governance_block_items(max_items)
                .into_iter()
                .filter_map(|item| match item {
                    BlockItem::TeeGovernance(governance)
                        if !excluded.contains(&governance.action_id) =>
                    {
                        Some(governance)
                    }
                    BlockItem::TeeGovernance(_) => None,
                    BlockItem::Intent(_)
                    | BlockItem::Transaction(_)
                    | BlockItem::OraclePrice(_)
                    | BlockItem::Funding(_)
                    | BlockItem::Faucet(_)
                    | BlockItem::Liquidation(_)
                    | BlockItem::Delegation(_)
                    | BlockItem::MarketMakerProgram(_)
                    | BlockItem::RealCapitalGovernance(_) => {
                        unreachable!(
                            "pending tee governance payload contained non-tee-governance item"
                        )
                    }
                })
                .collect()
        })
    }

    pub fn pending_real_capital_governance_payload(
        &self,
        max_items: usize,
        excluded: &BTreeSet<RealCapitalGovernanceActionId>,
    ) -> LockResult<Vec<BlockRealCapitalGovernance>> {
        self.with_admission_read(|engine| {
            engine
                .pending_real_capital_governance_block_items(max_items)
                .into_iter()
                .filter_map(|item| match item {
                    BlockItem::RealCapitalGovernance(governance)
                        if !excluded.contains(&governance.action_id) =>
                    {
                        Some(governance)
                    }
                    BlockItem::RealCapitalGovernance(_) => None,
                    BlockItem::Intent(_)
                    | BlockItem::Transaction(_)
                    | BlockItem::OraclePrice(_)
                    | BlockItem::Funding(_)
                    | BlockItem::Faucet(_)
                    | BlockItem::Liquidation(_)
                    | BlockItem::Delegation(_)
                    | BlockItem::MarketMakerProgram(_)
                    | BlockItem::TeeGovernance(_) => {
                        unreachable!(
                            "pending real-capital governance payload contained non-governance item"
                        )
                    }
                })
                .collect()
        })
    }

    pub fn apply_mahi_committed_block(
        &self,
        block: &MahiBlock,
    ) -> LockResult<Result<Hash, crate::NodeError>> {
        let timestamp_ms = block.timestamp_ms;
        let _policy = self.policy.track_lock();
        let _dag = self.dag.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        let _chain_tail = self.chain_tail.track_lock();
        let _events = self.events.track_lock();
        self.with_engine_mutation(
            timestamp_ms,
            SyncMode::Sync,
            None,
            |engine| engine.apply_mahi_committed_block(block).map(block_hash),
            durable_error_result,
        )
    }

    pub fn produce_block_from_mempool(
        &self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> LockResult<Option<Result<ProducedBlockSnapshot, crate::NodeError>>> {
        let _policy = self.policy.track_lock();
        let _dag = self.dag.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        let _chain_tail = self.chain_tail.track_lock();
        let _events = self.events.track_lock();
        let backend = self.durable_backend.clone();
        match self.compat_lock(None) {
            Ok(mut engine) => {
                if engine.intent_mempool().is_empty() && engine.conditional_intent_pool().is_empty()
                {
                    return Ok(None);
                }
                let result = if let Some(backend) = backend {
                    match crate::persistence::apply_engine_mutation_durable(
                        &mut engine,
                        &backend,
                        timestamp_ms,
                        SyncMode::Sync,
                        |engine| {
                            engine
                                .produce_block_from_mempool(timestamp_ms, max_items)
                                .map(produced_block_snapshot)
                        },
                    ) {
                        Ok(result) => result,
                        Err(error) => Err(durable_node_error(error)),
                    }
                } else {
                    engine
                        .produce_block_from_mempool(timestamp_ms, max_items)
                        .map(produced_block_snapshot)
                };
                Ok(Some(result))
            }
            Err(poisoned) => {
                let mut engine = poisoned.into_inner();
                if engine.intent_mempool().is_empty() && engine.conditional_intent_pool().is_empty()
                {
                    return Err(PoisonError::new(None));
                }
                let result = if let Some(backend) = backend {
                    match crate::persistence::apply_engine_mutation_durable(
                        &mut engine,
                        &backend,
                        timestamp_ms,
                        SyncMode::Sync,
                        |engine| {
                            engine
                                .produce_block_from_mempool(timestamp_ms, max_items)
                                .map(produced_block_snapshot)
                        },
                    ) {
                        Ok(result) => result,
                        Err(error) => Err(durable_node_error(error)),
                    }
                } else {
                    engine
                        .produce_block_from_mempool(timestamp_ms, max_items)
                        .map(produced_block_snapshot)
                };
                Err(PoisonError::new(Some(result)))
            }
        }
    }

    pub fn produce_block_from_mempool_forced(
        &self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> LockResult<Result<ProducedBlockSnapshot, crate::NodeError>> {
        let _policy = self.policy.track_lock();
        let _dag = self.dag.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        let _chain_tail = self.chain_tail.track_lock();
        let _events = self.events.track_lock();
        self.with_engine_mutation(
            timestamp_ms,
            SyncMode::Sync,
            None,
            |engine| {
                engine
                    .produce_block_from_mempool(timestamp_ms, max_items)
                    .map(produced_block_snapshot)
            },
            durable_error_result,
        )
    }

    pub fn apply_mahi_committed_blocks(
        &self,
        blocks: &[MahiBlock],
    ) -> LockResult<Vec<Result<Hash, crate::NodeError>>> {
        let _policy = self.policy.track_lock();
        let _dag = self.dag.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        let _chain_tail = self.chain_tail.track_lock();
        let _events = self.events.track_lock();
        let mutation_timestamp_ms = blocks
            .iter()
            .map(|block| block.timestamp_ms)
            .max()
            .unwrap_or(0);
        let block_count = blocks.len();
        self.with_engine_mutation(
            mutation_timestamp_ms,
            SyncMode::Sync,
            None,
            |engine| {
                blocks
                    .iter()
                    .map(|block| engine.apply_mahi_committed_block(block).map(block_hash))
                    .collect()
            },
            |error| {
                let node_error = durable_node_error(error);
                (0..block_count).map(|_| Err(node_error.clone())).collect()
            },
        )
    }

    pub fn apply_mahi_committed_blocks_merged(
        &self,
        blocks: &[MahiBlock],
    ) -> LockResult<Result<Hash, crate::NodeError>> {
        let _policy = self.policy.track_lock();
        let _dag = self.dag.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        let _chain_tail = self.chain_tail.track_lock();
        let _events = self.events.track_lock();
        let mutation_timestamp_ms = blocks
            .iter()
            .map(|block| block.timestamp_ms)
            .max()
            .unwrap_or(0);
        self.with_engine_mutation(
            mutation_timestamp_ms,
            SyncMode::Sync,
            None,
            |engine| {
                engine
                    .apply_mahi_committed_blocks_merged(blocks)
                    .map(block_hash)
            },
            durable_error_result,
        )
    }

    fn from_compat(compat: EngineCompat) -> Self {
        let (head, metrics, frontier, current_round) = match &compat {
            EngineCompat::ParkingLot(engine) => {
                let guard = engine.lock();
                (
                    build_head_snapshot(&guard),
                    guard.metrics().clone(),
                    guard.proposer_frontier_stats(),
                    guard.current_round(),
                )
            }
            EngineCompat::Legacy(engine) => {
                let guard = engine
                    .lock()
                    .expect("legacy engine handle should not initialize from poisoned mutex");
                (
                    build_head_snapshot(&guard),
                    guard.metrics().clone(),
                    guard.proposer_frontier_stats(),
                    guard.current_round(),
                )
            }
        };
        Self::from_snapshots(compat, head, metrics, frontier, current_round)
    }

    fn from_snapshots(
        compat: EngineCompat,
        head: HeadSnapshot,
        metrics: ConsensusMetricsSnapshot,
        frontier: ProposerFrontierStats,
        current_round: u64,
    ) -> Self {
        Self {
            compat,
            durable_backend: None,
            admission_sync_mode: SyncMode::Sync,
            policy: PolicyDomain::default(),
            dag: DagDomain::default(),
            mempool: MempoolDomain {
                lock: ParkingMutex::new(()),
                proposer_frontier_stats: RwLock::new(frontier),
            },
            market_state: MarketStateDomain::default(),
            chain_tail: ChainTailDomain {
                lock: ParkingMutex::new(()),
                head_snapshot: ArcSwap::from_pointee(head),
            },
            pacemaker: PacemakerDomain {
                lock: ParkingMutex::new(()),
                current_round: RwLock::new(current_round),
                consensus_metrics: RwLock::new(metrics),
            },
            events: EventsDomain::default(),
            lock_profile: RwLock::new(EngineLockProfileSnapshot::default()),
        }
    }

    fn refresh_views_locked(&self, engine: &SingleNodeEngine) {
        self.chain_tail
            .head_snapshot
            .store(Arc::new(build_head_snapshot(engine)));
        *self.pacemaker.current_round.write() = engine.current_round();
        *self.pacemaker.consensus_metrics.write() = engine.metrics().clone();
        *self.mempool.proposer_frontier_stats.write() = engine.proposer_frontier_stats();
    }

    fn with_admission_read<T, F>(&self, f: F) -> LockResult<T>
    where
        F: FnOnce(&SingleNodeEngine) -> T,
    {
        let _policy = self.policy.track_lock();
        let _mempool = self.mempool.track_lock();
        let _market_state = self.market_state.track_lock();
        let wait_started = Instant::now();
        match self.compat_lock(None) {
            Ok(engine) => {
                let acquired_at = Instant::now();
                let result = f(&engine);
                let hold_ns = duration_as_nanos_u64(acquired_at.elapsed());
                drop(engine);
                self.record_lock_profile(
                    EngineLockProfileKind::AdmissionRead,
                    duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                    hold_ns,
                );
                Ok(result)
            }
            Err(poisoned) => {
                let acquired_at = Instant::now();
                let result = f(&poisoned.into_inner());
                let hold_ns = duration_as_nanos_u64(acquired_at.elapsed());
                self.record_lock_profile(
                    EngineLockProfileKind::AdmissionRead,
                    duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                    hold_ns,
                );
                Err(PoisonError::new(result))
            }
        }
    }

    fn with_engine_mutation<T, F, P>(
        &self,
        timestamp_ms: u64,
        mode: SyncMode,
        profile_kind: Option<EngineLockProfileKind>,
        f: F,
        on_durable_error: P,
    ) -> LockResult<T>
    where
        F: FnOnce(&mut SingleNodeEngine) -> T,
        P: FnOnce(crate::persistence::DurableCommitError) -> T,
    {
        let backend = self.durable_backend.clone();
        let wait_started = Instant::now();
        match self.compat_lock(None) {
            Ok(mut engine) => {
                let acquired_at = Instant::now();
                let result = if let Some(backend) = backend {
                    match crate::persistence::apply_engine_mutation_durable(
                        &mut engine,
                        &backend,
                        timestamp_ms,
                        mode,
                        f,
                    ) {
                        Ok(result) => result,
                        Err(error) => on_durable_error(error),
                    }
                } else {
                    f(&mut engine)
                };
                let hold_ns = duration_as_nanos_u64(acquired_at.elapsed());
                drop(engine);
                if let Some(profile_kind) = profile_kind {
                    self.record_lock_profile(
                        profile_kind,
                        duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                        hold_ns,
                    );
                }
                Ok(result)
            }
            Err(poisoned) => {
                let acquired_at = Instant::now();
                let mut engine = poisoned.into_inner();
                let result = if let Some(backend) = backend {
                    match crate::persistence::apply_engine_mutation_durable(
                        &mut engine,
                        &backend,
                        timestamp_ms,
                        mode,
                        f,
                    ) {
                        Ok(result) => result,
                        Err(error) => on_durable_error(error),
                    }
                } else {
                    f(&mut engine)
                };
                let hold_ns = duration_as_nanos_u64(acquired_at.elapsed());
                if let Some(profile_kind) = profile_kind {
                    self.record_lock_profile(
                        profile_kind,
                        duration_as_nanos_u64(acquired_at.duration_since(wait_started)),
                        hold_ns,
                    );
                }
                Err(PoisonError::new(result))
            }
        }
    }

    fn record_lock_profile(&self, kind: EngineLockProfileKind, wait_ns: u64, hold_ns: u64) {
        let mut profile = self.lock_profile.write();
        let samples = match kind {
            EngineLockProfileKind::CompatRaw => &mut profile.compat_raw,
            EngineLockProfileKind::AdmissionRead => &mut profile.admission_read,
            EngineLockProfileKind::AdmissionWrite => &mut profile.admission_write,
        };
        push_capped_sample(&mut samples.wait_ns, wait_ns);
        push_capped_sample(&mut samples.hold_ns, hold_ns);
    }
}

pub struct EngineCompatGuard<'a> {
    handle: &'a EngineHandle,
    guard: EngineCompatGuardInner<'a>,
    acquired_at: Instant,
    wait_ns: u64,
    profile_kind: Option<EngineLockProfileKind>,
}

enum EngineCompatGuardInner<'a> {
    ParkingLot(parking_lot::MutexGuard<'a, SingleNodeEngine>),
    Legacy(StdMutexGuard<'a, SingleNodeEngine>),
}

impl Deref for EngineCompatGuard<'_> {
    type Target = SingleNodeEngine;

    fn deref(&self) -> &Self::Target {
        match &self.guard {
            EngineCompatGuardInner::ParkingLot(guard) => guard.deref(),
            EngineCompatGuardInner::Legacy(guard) => guard.deref(),
        }
    }
}

impl DerefMut for EngineCompatGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.guard {
            EngineCompatGuardInner::ParkingLot(guard) => guard.deref_mut(),
            EngineCompatGuardInner::Legacy(guard) => guard.deref_mut(),
        }
    }
}

impl Drop for EngineCompatGuard<'_> {
    fn drop(&mut self) {
        if let Some(kind) = self.profile_kind {
            self.handle.record_lock_profile(
                kind,
                self.wait_ns,
                duration_as_nanos_u64(self.acquired_at.elapsed()),
            );
        }
        self.handle.refresh_views_locked(&self.guard);
    }
}

fn durable_node_error(error: crate::persistence::DurableCommitError) -> crate::NodeError {
    match error {
        crate::persistence::DurableCommitError::Node(error) => error,
        crate::persistence::DurableCommitError::Persistence(error) => {
            crate::NodeError::Persistence {
                reason: error.to_string(),
            }
        }
    }
}

fn durable_error_result<T>(
    error: crate::persistence::DurableCommitError,
) -> Result<T, crate::NodeError> {
    Err(durable_node_error(error))
}

fn produced_block_snapshot(block: &Block) -> ProducedBlockSnapshot {
    ProducedBlockSnapshot {
        height: block.header.height,
        hash: block_hash(block),
        state_root: block.header.state_root,
        timestamp_ms: block.header.timestamp_ms,
        item_count: block.body.items.len(),
    }
}

impl Deref for EngineCompatGuardInner<'_> {
    type Target = SingleNodeEngine;

    fn deref(&self) -> &Self::Target {
        match self {
            EngineCompatGuardInner::ParkingLot(guard) => guard.deref(),
            EngineCompatGuardInner::Legacy(guard) => guard.deref(),
        }
    }
}

fn build_head_snapshot(engine: &SingleNodeEngine) -> HeadSnapshot {
    let head = engine.head();
    HeadSnapshot {
        chain_id: head.header.chain_id,
        head_height: head.header.height,
        head_round: head.header.round,
        head_hash: block_hash(head),
        committed_state_root: engine.committed_state_root(),
        head_timestamp_ms: head.header.timestamp_ms,
        current_round: engine.current_round(),
        local_validator_id: engine.local_validator_id(),
    }
}

const MAX_LOCK_PROFILE_SAMPLES: usize = 2048;

fn push_capped_sample(samples: &mut Vec<u64>, sample: u64) {
    if samples.len() >= MAX_LOCK_PROFILE_SAMPLES {
        samples.remove(0);
    }
    samples.push(sample);
}

fn duration_as_nanos_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

pub struct LockOrderTracker;

pub struct LockOrderGuard {
    #[cfg(debug_assertions)]
    domain: LockDomain,
}

impl LockOrderTracker {
    pub fn enter(domain: LockDomain) -> LockOrderGuard {
        #[cfg(not(debug_assertions))]
        let _ = domain;
        #[cfg(debug_assertions)]
        LOCK_ORDER_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(previous) = stack.last().copied() {
                assert!(
                    previous <= domain,
                    "lock order violation: attempted {:?} after {:?}",
                    domain,
                    previous
                );
            }
            stack.push(domain);
        });
        LockOrderGuard {
            #[cfg(debug_assertions)]
            domain,
        }
    }
}

impl Drop for LockOrderGuard {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        LOCK_ORDER_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let popped = stack.pop();
            assert_eq!(popped, Some(self.domain), "lock order tracker pop mismatch");
        });
    }
}

#[cfg(debug_assertions)]
thread_local! {
    static LOCK_ORDER_STACK: RefCell<Vec<LockDomain>> = const { RefCell::new(Vec::new()) };
}

impl PolicyDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::Policy)
    }
}

impl DagDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::Dag)
    }
}

impl MempoolDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::Mempool)
    }
}

impl MarketStateDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::MarketState)
    }
}

impl ChainTailDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::ChainTail)
    }
}

impl PacemakerDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::Pacemaker)
    }
}

impl EventsDomain {
    pub fn track_lock(&self) -> LockOrderGuard {
        let _ = &self.lock;
        LockOrderTracker::enter(LockDomain::Events)
    }
}

fn mahi_payload_sharding_enabled(validator_count: usize) -> bool {
    if validator_count <= 1 {
        return false;
    }
    env::var(MAHI_PAYLOAD_SHARDING_ENV)
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::{EngineHandle, LockDomain, LockOrderTracker};
    use crate::{SingleNodeConfig, SingleNodeEngine};
    use kombat_chain_consensus::{block_hash, mahi::MahiBlock, PacemakerConfig};
    use kombat_chain_state_db::{
        AnyStateBackend, OpenOptions, RedbBackend, StateBackend, SyncMode,
    };
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn single_node_config() -> SingleNodeConfig {
        SingleNodeConfig {
            chain_id: 7,
            validator_id: [9; 32],
            consensus_signing_key: [9; 32],
            genesis_timestamp_ms: 1_000,
            genesis_state_root: [0; 32],
            pacemaker: PacemakerConfig {
                base_timeout_ms: 100,
                max_timeout_ms: 1_600,
                max_exponent: 4,
                max_clock_skew_ms: 1_000,
            },
        }
    }

    fn temp_state_dir(label: &str) -> std::path::PathBuf {
        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic")
            .as_nanos();
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "kombat-engine-handle-{label}-{}-{unique}-{sequence}",
            std::process::id(),
        ))
    }

    #[test]
    fn engine_handle_refreshes_cached_views_after_mutation() {
        let handle = EngineHandle::new(SingleNodeEngine::new(single_node_config()));
        {
            let mut engine = handle.lock().expect("engine lock should succeed");
            engine.record_timeout(1_010);
            engine
                .produce_empty_block(1_100)
                .expect("block production should succeed");
        }

        let head = handle.head_snapshot();
        assert_eq!(head.head_height, 1);
        assert_eq!(head.head_timestamp_ms, 1_100);
        assert_eq!(head.current_round, 2);
        assert_eq!(handle.current_round_snapshot(), 2);
        assert_eq!(handle.consensus_metrics_snapshot().timeouts_total, 1);
    }

    #[test]
    fn durable_handle_persists_committed_mahi_block() {
        let dir = temp_state_dir("redb-commit");
        std::fs::create_dir_all(&dir).expect("state dir should create");
        let backend = Arc::new(AnyStateBackend::Redb(
            RedbBackend::open(OpenOptions::new(dir.join("state.redb"))).expect("redb should open"),
        ));
        let config = single_node_config();
        let engine = SingleNodeEngine::new(config.clone());
        engine
            .initialize_state_backend(&backend, 1_000)
            .expect("backend should initialize");
        let handle = EngineHandle::new_durable(engine, backend.clone(), SyncMode::Sync);
        let block = MahiBlock::signed(
            config.validator_id,
            1,
            1,
            1_200,
            Vec::new(),
            Vec::new(),
            config.consensus_signing_key,
        );

        let applied_hash = handle
            .apply_mahi_committed_block(&block)
            .expect("engine lock should succeed")
            .expect("block should apply");

        assert_eq!(handle.head_snapshot().head_height, 1);
        let hydrated = SingleNodeEngine::from_state_backend(&backend).expect("hydrate redb");
        assert_eq!(hydrated.head().header.height, 1);
        assert_eq!(block_hash(hydrated.head()), applied_hash);
        drop(handle);
        drop(backend);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "lock order violation")]
    fn lock_order_tracker_panics_on_reversed_domain_order() {
        let _pacemaker = LockOrderTracker::enter(LockDomain::Pacemaker);
        let _mempool = LockOrderTracker::enter(LockDomain::Mempool);
    }
}
