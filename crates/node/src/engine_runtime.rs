use super::*;

pub fn signed_session_payload_for_intent(signed_intent: &SignedIntent) -> SignedSessionPayload {
    SignedSessionPayload {
        payload: SessionPayload {
            intent_hash: intent_signing_hash(&signed_intent.intent),
            policy_id: signed_intent.intent.policy_id,
            chain_id: signed_intent.intent.chain_id,
        },
        signature: signed_intent.signature,
    }
}

pub fn verify_intent_signature(
    policy_store: &PolicyStore,
    signed_intent: &SignedIntent,
    expected_chain_id: u64,
    block_number: u64,
) -> Result<(), VerifyIntentError> {
    let policy_id = signed_intent.intent.policy_id;
    let signed_session = signed_session_payload_for_intent(signed_intent);
    policy_store
        .verify_session(policy_id, &signed_session, block_number, expected_chain_id)
        .map_err(VerifyIntentError::Policy)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_submitted_intent(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_operators: &BTreeMap<u64, u64>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    direct_mempool: &DirectIntentMempool,
    conditional_pool: &ConditionalIntentPool,
    signed_intent: &SignedIntent,
    chain_id: u64,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<IntentId, NodeError> {
    if frozen_accounts.contains(&signed_intent.intent.account) {
        return Err(NodeError::AccountFrozen {
            account_id: signed_intent.intent.account,
        });
    }

    let intent_id = intent_id(signed_intent);
    if direct_mempool.contains_intent(intent_id) || conditional_pool.contains_intent(intent_id) {
        return Err(NodeError::DuplicateIntent(intent_id));
    }

    let expires_at = intent_expires_at_ms(&signed_intent.intent);
    if expires_at <= timestamp_ms {
        return Err(NodeError::IntentExpired {
            intent_id,
            expires_at,
            timestamp_ms,
        });
    }

    let committed_next_nonce = policy_store
        .next_intent_nonce(signed_intent.intent.policy_id)
        .map_err(NodeError::Policy)?;
    if signed_intent.intent.nonce < committed_next_nonce {
        return Err(NodeError::StaleIntentNonce {
            policy_id: signed_intent.intent.policy_id,
            expected: committed_next_nonce,
            actual: signed_intent.intent.nonce,
        });
    }

    if direct_mempool
        .contains_policy_nonce(signed_intent.intent.policy_id, signed_intent.intent.nonce)
        || conditional_pool
            .contains_policy_nonce(signed_intent.intent.policy_id, signed_intent.intent.nonce)
    {
        return Err(NodeError::DuplicatePolicyNonce {
            policy_id: signed_intent.intent.policy_id,
            nonce: signed_intent.intent.nonce,
        });
    }

    if signed_intent.intent.nonce == committed_next_nonce {
        verify_intent_signature(policy_store, signed_intent, chain_id, block_number)?;
        let mut simulated_store = policy_store.clone();
        let mut simulated_order_books = BTreeMap::new();
        let mut simulated_resting_order_mandates = BTreeMap::new();
        let mut simulated_isolated_collateral = BTreeMap::new();
        simulate_intent_on_runtime_state(
            &mut simulated_store,
            mandates,
            delegations,
            account_operators,
            simulation_order_books_for_intent(
                &mut simulated_order_books,
                order_books,
                signed_intent,
            ),
            simulation_resting_order_mandates_for_intent(
                &mut simulated_resting_order_mandates,
                resting_order_mandates,
                signed_intent,
            ),
            simulation_collateral_for_intent(
                &mut simulated_isolated_collateral,
                isolated_collateral,
                signed_intent,
            ),
            frozen_accounts,
            markets,
            paused_markets,
            market_marks,
            market_index_prices,
            market_oracle_config,
            positions,
            signed_intent,
            chain_id,
            block_number,
            timestamp_ms,
            PolicyAdmissionCommitMode::VerifySession,
        )
        .or_else(|error| {
            if transient_intent_blocker(&error) {
                Ok(Vec::new())
            } else {
                Err(error)
            }
        })?;
    }

    Ok(intent_id)
}

pub(super) fn simulation_order_books_for_intent<'a>(
    simulated_order_books: &'a mut BTreeMap<u64, SingleMarketBook>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    signed_intent: &SignedIntent,
) -> &'a mut BTreeMap<u64, SingleMarketBook> {
    let Some(market_id) =
        order_book_market_id(signed_intent).or_else(|| collateral_market_id(signed_intent))
    else {
        return simulated_order_books;
    };

    simulation_order_book_for_market(simulated_order_books, order_books, market_id);

    simulated_order_books
}

pub(super) fn simulation_order_book_for_market(
    simulated_order_books: &mut BTreeMap<u64, SingleMarketBook>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    market_id: u64,
) {
    if let std::collections::btree_map::Entry::Vacant(entry) =
        simulated_order_books.entry(market_id)
    {
        if let Some(order_book) = order_books.get(&market_id) {
            entry.insert(order_book.clone());
        }
    }
}

pub(super) fn simulation_resting_order_mandates_for_intent<'a>(
    simulated_resting_order_mandates: &'a mut BTreeMap<(u64, u64, u64), MandateId>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    signed_intent: &SignedIntent,
) -> &'a mut BTreeMap<(u64, u64, u64), MandateId> {
    let Some(market_id) =
        order_book_market_id(signed_intent).or_else(|| collateral_market_id(signed_intent))
    else {
        return simulated_resting_order_mandates;
    };

    simulated_resting_order_mandates.extend(
        resting_order_mandates
            .iter()
            .filter(|((entry_market_id, _, _), _)| *entry_market_id == market_id)
            .map(|(key, mandate_id)| (*key, *mandate_id)),
    );
    simulated_resting_order_mandates
}

pub(super) fn simulation_collateral_for_intent<'a>(
    simulated_isolated_collateral: &'a mut BTreeMap<(u64, u64), i128>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    signed_intent: &SignedIntent,
) -> &'a mut BTreeMap<(u64, u64), i128> {
    let Some(market_id) =
        collateral_market_id(signed_intent).or_else(|| new_order_market_id(signed_intent))
    else {
        return simulated_isolated_collateral;
    };
    let key = (market_id, signed_intent.intent.account);
    if let std::collections::btree_map::Entry::Vacant(entry) =
        simulated_isolated_collateral.entry(key)
    {
        if let Some(collateral) = isolated_collateral.get(&key) {
            entry.insert(*collateral);
        }
    }
    simulated_isolated_collateral
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PolicyAdmissionCommitMode {
    VerifySession,
    PreverifiedSession,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct RestingBatchCommitStats {
    pub(super) attempts: u64,
    pub(super) accepted: u64,
    pub(super) items: u64,
    pub(super) unsupported: u64,
    pub(super) rejected: u64,
    pub(super) gather_ns: u64,
    pub(super) precheck_ns: u64,
    pub(super) admission_ns: u64,
    pub(super) policy_ns: u64,
    pub(super) matching_ns: u64,
    pub(super) event_ns: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct CommitBlockIntentsOutput {
    pub(super) events: Vec<RuntimeEventKind>,
    pub(super) resting_batch_stats: RestingBatchCommitStats,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn commit_block_intents(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_operators: &BTreeMap<u64, u64>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &mut BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    _block_height: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<Vec<RuntimeEventKind>, NodeError> {
    Ok(commit_block_intents_with_mode(
        policy_store,
        mandates,
        delegations,
        account_operators,
        order_books,
        resting_order_mandates,
        isolated_collateral,
        frozen_accounts,
        markets,
        paused_markets,
        market_marks,
        market_index_prices,
        market_oracle_config,
        positions,
        chain_id,
        _block_height,
        body,
        block_number,
        timestamp_ms,
        PolicyAdmissionCommitMode::VerifySession,
    )?
    .events)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn commit_block_intents_preverified_with_stats(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_operators: &BTreeMap<u64, u64>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &mut BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    block_height: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<CommitBlockIntentsOutput, NodeError> {
    commit_block_intents_with_mode(
        policy_store,
        mandates,
        delegations,
        account_operators,
        order_books,
        resting_order_mandates,
        isolated_collateral,
        frozen_accounts,
        markets,
        paused_markets,
        market_marks,
        market_index_prices,
        market_oracle_config,
        positions,
        chain_id,
        block_height,
        body,
        block_number,
        timestamp_ms,
        PolicyAdmissionCommitMode::PreverifiedSession,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn commit_block_intents_with_mode(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_operators: &BTreeMap<u64, u64>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &mut BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    _block_height: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
    commit_mode: PolicyAdmissionCommitMode,
) -> Result<CommitBlockIntentsOutput, NodeError> {
    if commit_mode == PolicyAdmissionCommitMode::PreverifiedSession
        && phase6_market_sharded_resting_batch_enabled()
    {
        if let Some(output) = try_commit_market_sharded_resting_place_order_batch(
            policy_store,
            mandates,
            delegations,
            order_books,
            resting_order_mandates,
            isolated_collateral,
            frozen_accounts,
            markets,
            paused_markets,
            market_marks,
            market_index_prices,
            market_oracle_config,
            positions,
            chain_id,
            body,
            block_number,
            timestamp_ms,
        )? {
            return Ok(output);
        }
    }

    let mut events = Vec::new();
    let mut resting_batch_stats = RestingBatchCommitStats::default();
    let mut item_index = 0;
    while item_index < body.items.len() {
        if let Some((consumed, mut batch_events)) = try_commit_resting_place_order_batch(
            policy_store,
            mandates,
            delegations,
            order_books,
            resting_order_mandates,
            isolated_collateral,
            frozen_accounts,
            markets,
            paused_markets,
            market_marks,
            market_index_prices,
            market_oracle_config,
            positions,
            chain_id,
            &body.items,
            item_index,
            block_number,
            timestamp_ms,
            commit_mode,
            &mut resting_batch_stats,
        )? {
            events.append(&mut batch_events);
            item_index += consumed;
            continue;
        }

        let item = &body.items[item_index];
        if let BlockItem::Intent(signed_intent) = item {
            let mut intent_events = simulate_intent_on_runtime_state(
                policy_store,
                mandates,
                delegations,
                account_operators,
                order_books,
                resting_order_mandates,
                isolated_collateral,
                frozen_accounts,
                markets,
                paused_markets,
                market_marks,
                market_index_prices,
                market_oracle_config,
                positions,
                signed_intent,
                chain_id,
                block_number,
                timestamp_ms,
                commit_mode,
            )?;
            events.push(RuntimeEventKind::IntentExecuted {
                intent_id: intent_id(signed_intent),
                policy_id: signed_intent.intent.policy_id,
                nonce: signed_intent.intent.nonce,
            });
            events.append(&mut intent_events);
        }
        item_index += 1;
    }

    Ok(CommitBlockIntentsOutput {
        events,
        resting_batch_stats,
    })
}

pub(super) fn phase6_market_sharded_resting_batch_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(PHASE6_MARKET_SHARDED_RESTING_BATCH_ENV).is_some())
}

#[derive(Clone, Copy)]
pub(super) struct MarketShardEntry {
    item_index: usize,
    order: OrderRequest,
}

pub(super) struct MarketShard {
    side: MatchingOrderSide,
    entries: Vec<MarketShardEntry>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_commit_market_sharded_resting_place_order_batch(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<Option<CommitBlockIntentsOutput>, NodeError> {
    try_commit_market_sharded_resting_place_order_batch_with_matching_mode(
        policy_store,
        mandates,
        delegations,
        order_books,
        resting_order_mandates,
        isolated_collateral,
        frozen_accounts,
        markets,
        paused_markets,
        market_marks,
        market_index_prices,
        market_oracle_config,
        positions,
        chain_id,
        body,
        block_number,
        timestamp_ms,
        phase6_worker_market_matching_enabled(),
    )
}

pub(super) fn phase6_worker_market_matching_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(PHASE6_WORKER_MARKET_MATCHING_ENV).is_some())
}

pub(super) fn phase6_inplace_resting_commit_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(PHASE6_INPLACE_RESTING_COMMIT_ENV)
            .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
            .unwrap_or(true)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_commit_market_sharded_resting_place_order_batch_including_single_shard(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<Option<CommitBlockIntentsOutput>, NodeError> {
    try_commit_market_sharded_resting_place_order_batch_inner(
        policy_store,
        mandates,
        delegations,
        order_books,
        resting_order_mandates,
        isolated_collateral,
        frozen_accounts,
        markets,
        paused_markets,
        market_marks,
        market_index_prices,
        market_oracle_config,
        positions,
        chain_id,
        body,
        block_number,
        timestamp_ms,
        phase6_worker_market_matching_enabled(),
        true,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_commit_market_sharded_resting_place_order_batch_with_matching_mode(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
    worker_market_matching: bool,
) -> Result<Option<CommitBlockIntentsOutput>, NodeError> {
    try_commit_market_sharded_resting_place_order_batch_inner(
        policy_store,
        mandates,
        delegations,
        order_books,
        resting_order_mandates,
        isolated_collateral,
        frozen_accounts,
        markets,
        paused_markets,
        market_marks,
        market_index_prices,
        market_oracle_config,
        positions,
        chain_id,
        body,
        block_number,
        timestamp_ms,
        worker_market_matching,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_commit_market_sharded_resting_place_order_batch_inner(
    policy_store: &mut PolicyStore,
    // AllowedCounterparties cannot fire here: validate_resting_batch rejects
    // crossing orders as Unsupported, so batch outcomes have no fills.
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    body: &BlockBody,
    block_number: u64,
    timestamp_ms: u64,
    worker_market_matching: bool,
    allow_single_shard: bool,
) -> Result<Option<CommitBlockIntentsOutput>, NodeError> {
    if body.items.len() < 2 {
        return Ok(None);
    }

    let mut stats = RestingBatchCommitStats::default();
    let gather_started = Instant::now();
    let mut shards = BTreeMap::<u64, MarketShard>::new();
    let mut signed_by_item = vec![None; body.items.len()];

    for (item_index, item) in body.items.iter().enumerate() {
        let BlockItem::Intent(signed_intent) = item else {
            return Ok(None);
        };
        let Some(order) = order_request_from_place_order_intent(signed_intent) else {
            return Ok(None);
        };
        signed_by_item[item_index] = Some(signed_intent);
        match shards.get_mut(&order.market_id) {
            Some(shard) => {
                if shard.side != order.side {
                    return Ok(None);
                }
                shard.entries.push(MarketShardEntry { item_index, order });
            }
            None => {
                shards.insert(
                    order.market_id,
                    MarketShard {
                        side: order.side,
                        entries: vec![MarketShardEntry { item_index, order }],
                    },
                );
            }
        }
    }

    if !allow_single_shard && shards.len() < 2 {
        return Ok(None);
    }
    stats.gather_ns = duration_nanos_u64(gather_started.elapsed());
    stats.attempts = shards.len() as u64;

    let precheck_started = Instant::now();
    for (market_id, shard) in shards.iter() {
        let orders = shard
            .entries
            .iter()
            .map(|entry| entry.order)
            .collect::<Vec<_>>();
        let batch_check = order_books.get(market_id).map_or_else(
            || SingleMarketBook::new(*market_id).can_submit_resting_batch(&orders, timestamp_ms),
            |book| book.can_submit_resting_batch(&orders, timestamp_ms),
        );
        match batch_check {
            Ok(()) => {}
            Err(RestingBatchError::Unsupported) => {
                return Ok(None);
            }
            Err(RestingBatchError::Reject(reason)) => {
                return Err(NodeError::Order(reason));
            }
        }
        let signed_intents = shard
            .entries
            .iter()
            .map(|entry| {
                signed_by_item[entry.item_index]
                    .expect("sharded entry should have matching signed intent")
            })
            .collect::<Vec<_>>();
        ensure_resting_order_batch_preserves_mandate_position_attribution(
            policy_store,
            order_books,
            resting_order_mandates,
            positions,
            &orders,
            &signed_intents,
            timestamp_ms,
        )?;
        ensure_resting_order_batch_preserves_mandate_max_leverage(
            policy_store,
            mandates,
            order_books,
            isolated_collateral,
            markets,
            market_marks,
            positions,
            &orders,
            &signed_intents,
            timestamp_ms,
        )?;
        ensure_resting_order_batch_preserves_initial_margin(
            order_books,
            isolated_collateral,
            markets,
            market_marks,
            positions,
            &orders,
            timestamp_ms,
        )?;
    }
    stats.precheck_ns = duration_nanos_u64(precheck_started.elapsed());

    let admission_started = Instant::now();
    let admissions = body
        .items
        .iter()
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
        .map(|signed_intent| {
            validate_intent_runtime_gates(
                signed_intent,
                frozen_accounts,
                markets,
                paused_markets,
                market_marks,
                market_index_prices,
                market_oracle_config,
                timestamp_ms,
            )?;
            let policy_account_scope =
                policy_account_scope_for_intent(policy_store, delegations, signed_intent)?;
            Ok((
                signed_intent,
                policy_admission_for_intent(
                    signed_intent,
                    markets,
                    chain_id,
                    block_number,
                    timestamp_ms,
                )?,
                policy_account_scope,
            ))
        })
        .collect::<Result<Vec<_>, NodeError>>()?;
    stats.admission_ns = duration_nanos_u64(admission_started.elapsed());

    let policy_started = Instant::now();
    for (signed_intent, admission, policy_account_scope) in admissions.iter() {
        commit_stacked_mandate_admission(
            policy_store,
            signed_intent,
            admission,
            *policy_account_scope,
        )?;
        policy_store
            .commit_preverified_admission(signed_intent.intent.policy_id, admission)
            .map_err(NodeError::Policy)?;
    }
    stats.policy_ns = duration_nanos_u64(policy_started.elapsed());

    let matching_started = Instant::now();
    let mut outcomes_by_item = vec![None; body.items.len()];
    if worker_market_matching {
        for output in submit_market_shards_parallel(order_books, &shards, timestamp_ms)? {
            let shard = shards
                .get(&output.market_id)
                .expect("parallel matching should only return known market shards");
            for (entry, outcome) in shard.entries.iter().zip(output.outcomes) {
                outcomes_by_item[entry.item_index] = Some((output.market_id, outcome));
            }
            order_books.insert(output.market_id, output.book);
        }
    } else {
        for (market_id, shard) in shards.iter() {
            let orders = shard
                .entries
                .iter()
                .map(|entry| entry.order)
                .collect::<Vec<_>>();
            let outcomes = order_books
                .entry(*market_id)
                .or_insert_with(|| SingleMarketBook::new(*market_id))
                .submit_resting_batch(&orders, timestamp_ms)
                .map_err(node_error_from_resting_batch_error)?;
            for (entry, outcome) in shard.entries.iter().zip(outcomes) {
                outcomes_by_item[entry.item_index] = Some((*market_id, outcome));
            }
        }
    }
    stats.matching_ns = duration_nanos_u64(matching_started.elapsed());

    let event_started = Instant::now();
    let mut events = Vec::with_capacity(body.items.len().saturating_mul(2));
    for item_index in 0..body.items.len() {
        let signed_intent = signed_by_item[item_index]
            .expect("market-sharded resting batch only stores intent items");
        let (market_id, outcome) = outcomes_by_item[item_index]
            .as_ref()
            .expect("market-sharded resting batch should produce one outcome per item");
        events.push(RuntimeEventKind::IntentExecuted {
            intent_id: intent_id(signed_intent),
            policy_id: signed_intent.intent.policy_id,
            nonce: signed_intent.intent.nonce,
        });
        events.push(order_accepted_event_from_outcome(
            signed_intent,
            *market_id,
            outcome,
        ));
        let order = order_request_from_place_order_intent(signed_intent)
            .expect("market-sharded resting batch only stores place-order intents");
        events.extend(order_fill_events(
            *market_id,
            &outcome.fills,
            intent_origin_mandate_id(policy_store, signed_intent),
            resting_order_mandates,
        ));
        update_resting_order_mandates_from_outcome(
            resting_order_mandates,
            order_books,
            *market_id,
            &order,
            outcome,
            intent_origin_mandate_id(policy_store, signed_intent),
            timestamp_ms,
        );
    }
    stats.event_ns = duration_nanos_u64(event_started.elapsed());
    stats.accepted = shards.len() as u64;
    stats.items = body.items.len() as u64;

    Ok(Some(CommitBlockIntentsOutput {
        events,
        resting_batch_stats: stats,
    }))
}

pub(super) struct MarketShardMatchTask {
    market_id: u64,
    book: SingleMarketBook,
    orders: Vec<OrderRequest>,
}

pub(super) struct MarketShardMatchOutput {
    market_id: u64,
    book: SingleMarketBook,
    outcomes: Vec<MatchOutcome>,
}

pub(super) fn submit_market_shards_parallel(
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    shards: &BTreeMap<u64, MarketShard>,
    timestamp_ms: u64,
) -> Result<Vec<MarketShardMatchOutput>, NodeError> {
    let tasks = shards
        .iter()
        .map(|(market_id, shard)| MarketShardMatchTask {
            market_id: *market_id,
            book: order_books
                .remove(market_id)
                .unwrap_or_else(|| SingleMarketBook::new(*market_id)),
            orders: shard.entries.iter().map(|entry| entry.order).collect(),
        })
        .collect::<Vec<_>>();

    let outputs = thread::scope(|scope| {
        let handles = tasks
            .into_iter()
            .map(|task| {
                scope.spawn(move || {
                    let mut book = task.book;
                    let outcomes = book.submit_resting_batch(&task.orders, timestamp_ms)?;
                    Ok::<_, RestingBatchError>(MarketShardMatchOutput {
                        market_id: task.market_id,
                        book,
                        outcomes,
                    })
                })
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .expect("market matching worker should not panic")
            })
            .collect::<Result<Vec<_>, RestingBatchError>>()
    })
    .map_err(node_error_from_resting_batch_error)?;

    Ok(outputs)
}

pub(super) fn node_error_from_resting_batch_error(error: RestingBatchError) -> NodeError {
    match error {
        RestingBatchError::Reject(reason) => NodeError::Order(reason),
        RestingBatchError::Unsupported => NodeError::Order(OrderRejectReason::UnknownRestingOrder),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn try_commit_resting_place_order_batch(
    policy_store: &mut PolicyStore,
    // AllowedCounterparties cannot fire here: validate_resting_batch rejects
    // crossing orders as Unsupported, so batch outcomes have no fills.
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    chain_id: u64,
    items: &[BlockItem],
    start_index: usize,
    block_number: u64,
    timestamp_ms: u64,
    commit_mode: PolicyAdmissionCommitMode,
    stats: &mut RestingBatchCommitStats,
) -> Result<Option<(usize, Vec<RuntimeEventKind>)>, NodeError> {
    if commit_mode != PolicyAdmissionCommitMode::PreverifiedSession {
        return Ok(None);
    }

    let Some(BlockItem::Intent(first_intent)) = items.get(start_index) else {
        return Ok(None);
    };
    let Some(first_order) = order_request_from_place_order_intent(first_intent) else {
        return Ok(None);
    };

    let market_id = first_order.market_id;
    let side = first_order.side;
    let gather_started = Instant::now();
    let mut signed_intents = Vec::new();
    let mut orders = Vec::new();

    for item in items[start_index..]
        .iter()
        .take(RESTING_ORDER_COMMIT_BATCH_MAX)
    {
        let BlockItem::Intent(signed_intent) = item else {
            break;
        };
        let Some(order) = order_request_from_place_order_intent(signed_intent) else {
            break;
        };
        if order.market_id != market_id || order.side != side {
            break;
        }
        signed_intents.push(signed_intent);
        orders.push(order);
    }

    if signed_intents.len() < 2 {
        return Ok(None);
    }
    stats.gather_ns = stats
        .gather_ns
        .saturating_add(duration_nanos_u64(gather_started.elapsed()));

    stats.attempts = stats.attempts.saturating_add(1);
    let precheck_started = Instant::now();
    let batch_check = order_books.get(&market_id).map_or_else(
        || SingleMarketBook::new(market_id).can_submit_resting_batch(&orders, timestamp_ms),
        |book| book.can_submit_resting_batch(&orders, timestamp_ms),
    );
    stats.precheck_ns = stats
        .precheck_ns
        .saturating_add(duration_nanos_u64(precheck_started.elapsed()));
    match batch_check {
        Ok(()) => {}
        Err(RestingBatchError::Unsupported) => {
            stats.unsupported = stats.unsupported.saturating_add(1);
            return Ok(None);
        }
        Err(RestingBatchError::Reject(reason)) => {
            stats.rejected = stats.rejected.saturating_add(1);
            return Err(NodeError::Order(reason));
        }
    }
    ensure_resting_order_batch_preserves_initial_margin(
        order_books,
        isolated_collateral,
        markets,
        market_marks,
        positions,
        &orders,
        timestamp_ms,
    )?;
    ensure_resting_order_batch_preserves_mandate_position_attribution(
        policy_store,
        order_books,
        resting_order_mandates,
        positions,
        &orders,
        &signed_intents,
        timestamp_ms,
    )?;
    ensure_resting_order_batch_preserves_mandate_max_leverage(
        policy_store,
        mandates,
        order_books,
        isolated_collateral,
        markets,
        market_marks,
        positions,
        &orders,
        &signed_intents,
        timestamp_ms,
    )?;

    let admission_started = Instant::now();
    let admissions = signed_intents
        .iter()
        .map(|signed_intent| {
            validate_intent_runtime_gates(
                signed_intent,
                frozen_accounts,
                markets,
                paused_markets,
                market_marks,
                market_index_prices,
                market_oracle_config,
                timestamp_ms,
            )?;
            Ok((
                policy_admission_for_intent(
                    signed_intent,
                    markets,
                    chain_id,
                    block_number,
                    timestamp_ms,
                )?,
                policy_account_scope_for_intent(policy_store, delegations, signed_intent)?,
            ))
        })
        .collect::<Result<Vec<_>, NodeError>>()?;
    stats.admission_ns = stats
        .admission_ns
        .saturating_add(duration_nanos_u64(admission_started.elapsed()));

    let policy_started = Instant::now();
    debug_assert_eq!(signed_intents.len(), admissions.len());
    for (signed_intent, (admission, policy_account_scope)) in
        signed_intents.iter().zip(admissions.iter())
    {
        commit_stacked_mandate_admission(
            policy_store,
            signed_intent,
            admission,
            *policy_account_scope,
        )?;
        policy_store
            .commit_preverified_admission(signed_intent.intent.policy_id, admission)
            .map_err(NodeError::Policy)?;
    }
    stats.policy_ns = stats
        .policy_ns
        .saturating_add(duration_nanos_u64(policy_started.elapsed()));

    let matching_started = Instant::now();
    let outcomes = order_books
        .entry(market_id)
        .or_insert_with(|| SingleMarketBook::new(market_id))
        .submit_resting_batch(&orders, timestamp_ms)
        .map_err(|error| match error {
            RestingBatchError::Reject(reason) => NodeError::Order(reason),
            RestingBatchError::Unsupported => {
                NodeError::Order(OrderRejectReason::UnknownRestingOrder)
            }
        })?;
    stats.matching_ns = stats
        .matching_ns
        .saturating_add(duration_nanos_u64(matching_started.elapsed()));

    let event_started = Instant::now();
    let mut events = Vec::with_capacity(signed_intents.len().saturating_mul(2));
    for (signed_intent, outcome) in signed_intents.iter().zip(outcomes.iter()) {
        events.push(RuntimeEventKind::IntentExecuted {
            intent_id: intent_id(signed_intent),
            policy_id: signed_intent.intent.policy_id,
            nonce: signed_intent.intent.nonce,
        });
        events.push(order_accepted_event_from_outcome(
            signed_intent,
            market_id,
            outcome,
        ));
        let order = order_request_from_place_order_intent(signed_intent)
            .expect("resting batch only stores place-order intents");
        events.extend(order_fill_events(
            market_id,
            &outcome.fills,
            intent_origin_mandate_id(policy_store, signed_intent),
            resting_order_mandates,
        ));
        update_resting_order_mandates_from_outcome(
            resting_order_mandates,
            order_books,
            market_id,
            &order,
            outcome,
            intent_origin_mandate_id(policy_store, signed_intent),
            timestamp_ms,
        );
    }
    stats.event_ns = stats
        .event_ns
        .saturating_add(duration_nanos_u64(event_started.elapsed()));

    stats.accepted = stats.accepted.saturating_add(1);
    stats.items = stats.items.saturating_add(signed_intents.len() as u64);
    Ok(Some((signed_intents.len(), events)))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_intent_on_runtime_state(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_operators: &BTreeMap<u64, u64>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &mut BTreeMap<(u64, u64), i128>,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    chain_id: u64,
    block_number: u64,
    timestamp_ms: u64,
    commit_mode: PolicyAdmissionCommitMode,
) -> Result<Vec<RuntimeEventKind>, NodeError> {
    validate_intent_runtime_gates(
        signed_intent,
        frozen_accounts,
        markets,
        paused_markets,
        market_marks,
        market_index_prices,
        market_oracle_config,
        timestamp_ms,
    )?;
    let policy_account_scope =
        policy_account_scope_for_intent(policy_store, delegations, signed_intent)?;

    let admission =
        policy_admission_for_intent(signed_intent, markets, chain_id, block_number, timestamp_ms)?;
    commit_stacked_mandate_admission(
        policy_store,
        signed_intent,
        &admission,
        policy_account_scope,
    )?;
    match commit_mode {
        PolicyAdmissionCommitMode::VerifySession => policy_store
            .commit_admission(signed_intent.intent.policy_id, &admission)
            .map_err(NodeError::Policy)?,
        PolicyAdmissionCommitMode::PreverifiedSession => policy_store
            .commit_preverified_admission(signed_intent.intent.policy_id, &admission)
            .map_err(NodeError::Policy)?,
    }
    apply_intent_side_effect(
        policy_store,
        mandates,
        account_operators,
        order_books,
        resting_order_mandates,
        isolated_collateral,
        markets,
        market_marks,
        positions,
        signed_intent,
        block_number,
        timestamp_ms,
    )
}

pub(super) fn validate_intent_runtime_gates(
    signed_intent: &SignedIntent,
    frozen_accounts: &BTreeSet<u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    if frozen_accounts.contains(&signed_intent.intent.account) {
        return Err(NodeError::AccountFrozen {
            account_id: signed_intent.intent.account,
        });
    }
    if let Some(market_id) = new_order_market_id(signed_intent) {
        ensure_market_accepts_new_orders(markets, paused_markets, market_id)?;
    }
    if let Some(market_id) = collateral_market_id(signed_intent) {
        ensure_market_accepts_collateral_movement(markets, market_id)?;
    }
    if let Some(market_id) = paused_order_market_id(signed_intent, paused_markets) {
        return Err(NodeError::MarketTradingPaused { market_id });
    }
    if let Some(market_id) = stale_order_market_id(
        signed_intent,
        timestamp_ms,
        market_marks,
        market_index_prices,
        market_oracle_config,
    ) {
        return Err(NodeError::StaleMarketData { market_id });
    }

    Ok(())
}

pub(super) fn order_request_from_place_order_intent(
    signed_intent: &SignedIntent,
) -> Option<OrderRequest> {
    match &signed_intent.intent.kind {
        IntentKind::PlaceOrder {
            market_id,
            client_order_id,
            side,
            quantity_lots,
            kind,
            time_in_force,
            post_only,
        } => Some(OrderRequest {
            market_id: *market_id,
            account_id: signed_intent.intent.account,
            client_order_id: *client_order_id,
            side: match side {
                IntentOrderSide::Bid => kombat_chain_matching::OrderSide::Bid,
                IntentOrderSide::Ask => kombat_chain_matching::OrderSide::Ask,
            },
            quantity_lots: *quantity_lots,
            kind: match kind {
                IntentOrderKind::Limit { price_ticks } => kombat_chain_matching::OrderKind::Limit {
                    price_ticks: *price_ticks,
                },
                IntentOrderKind::Market => kombat_chain_matching::OrderKind::Market,
            },
            time_in_force: match time_in_force {
                IntentTimeInForce::Gtc => kombat_chain_matching::TimeInForce::Gtc,
                IntentTimeInForce::Ioc => kombat_chain_matching::TimeInForce::Ioc,
                IntentTimeInForce::Fok => kombat_chain_matching::TimeInForce::Fok,
                IntentTimeInForce::Gtt { expires_at_ms } => {
                    kombat_chain_matching::TimeInForce::Gtt {
                        expires_at_ms: *expires_at_ms,
                    }
                }
            },
            post_only: *post_only,
        }),
        _ => None,
    }
}

pub(super) fn order_accepted_event_from_outcome(
    signed_intent: &SignedIntent,
    market_id: u64,
    outcome: &MatchOutcome,
) -> RuntimeEventKind {
    let IntentKind::PlaceOrder {
        client_order_id, ..
    } = &signed_intent.intent.kind
    else {
        unreachable!("order accepted events are only emitted for place-order intents");
    };
    RuntimeEventKind::OrderAccepted {
        market_id,
        account_id: signed_intent.intent.account,
        client_order_id: *client_order_id,
        status: order_lifecycle_status(outcome.status),
        filled_lots: outcome.filled_lots,
        remaining_lots: outcome.remaining_lots,
    }
}

pub(super) fn apply_intent_side_effect(
    policy_store: &mut PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    account_operators: &BTreeMap<u64, u64>,
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    isolated_collateral: &mut BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<Vec<RuntimeEventKind>, NodeError> {
    match &signed_intent.intent.kind {
        IntentKind::Transfer { .. } | IntentKind::Cancel { .. } => Ok(vec![]),
        IntentKind::PlaceOrder { market_id, .. } => {
            let order = order_request_from_place_order_intent(signed_intent)
                .expect("place-order intent should produce an order request");
            ensure_order_preserves_mandate_position_attribution(
                policy_store,
                order_books,
                resting_order_mandates,
                positions,
                signed_intent,
                &order,
                None,
                timestamp_ms,
            )?;
            ensure_order_preserves_initial_margin(
                order_books,
                isolated_collateral,
                markets,
                market_marks,
                positions,
                &order,
                None,
                timestamp_ms,
            )?;
            ensure_order_preserves_mandate_max_leverage(
                policy_store,
                mandates,
                order_books,
                isolated_collateral,
                markets,
                market_marks,
                positions,
                signed_intent,
                &order,
                None,
                timestamp_ms,
            )?;
            let prior_book = order_books.get(market_id).cloned();
            let outcome = order_books
                .entry(*market_id)
                .or_insert_with(|| SingleMarketBook::new(*market_id))
                .submit(order, timestamp_ms)
                .map_err(NodeError::Order)?;
            let mut rollback_events = mandate_fill_rollback_events(
                policy_store,
                mandates,
                account_operators,
                markets,
                market_marks,
                isolated_collateral,
                positions,
                resting_order_mandates,
                *market_id,
                signed_intent,
                &outcome.fills,
                timestamp_ms,
            );
            rollback_events.extend(mandate_position_attribution_rollback_events(
                policy_store,
                mandates,
                resting_order_mandates,
                positions,
                signed_intent,
                *market_id,
                &outcome.fills,
                timestamp_ms,
            ));
            if !rollback_events.is_empty() {
                restore_order_book(order_books, *market_id, prior_book);
                // Policy counters and nonce are intentionally retained: the
                // intent reached Fill-mode and cannot probe books for free.
                return Ok(rollback_events);
            }
            let mut events = vec![order_accepted_event_from_outcome(
                signed_intent,
                *market_id,
                &outcome,
            )];
            events.extend(order_fill_events(
                *market_id,
                &outcome.fills,
                intent_origin_mandate_id(policy_store, signed_intent),
                resting_order_mandates,
            ));
            update_resting_order_mandates_from_outcome(
                resting_order_mandates,
                order_books,
                *market_id,
                &order,
                &outcome,
                intent_origin_mandate_id(policy_store, signed_intent),
                timestamp_ms,
            );
            Ok(events)
        }
        IntentKind::CancelOrder {
            market_id,
            client_order_id,
        } => {
            let cancelled = order_books
                .get_mut(market_id)
                .and_then(|book| {
                    book.cancel(signed_intent.intent.account, *client_order_id, timestamp_ms)
                })
                .ok_or(NodeError::Order(OrderRejectReason::UnknownRestingOrder))?;
            resting_order_mandates.remove(&resting_order_key(
                *market_id,
                signed_intent.intent.account,
                *client_order_id,
            ));
            Ok(vec![RuntimeEventKind::OrderCancelled {
                market_id: *market_id,
                account_id: signed_intent.intent.account,
                client_order_id: *client_order_id,
                remaining_lots: cancelled.remaining_lots,
            }])
        }
        IntentKind::ReplaceOrder {
            market_id,
            target_client_order_id,
            replacement_client_order_id,
            side,
            quantity_lots,
            kind,
            time_in_force,
            post_only,
        } => {
            let replacement = OrderRequest {
                market_id: *market_id,
                account_id: signed_intent.intent.account,
                client_order_id: *replacement_client_order_id,
                side: match side {
                    IntentOrderSide::Bid => kombat_chain_matching::OrderSide::Bid,
                    IntentOrderSide::Ask => kombat_chain_matching::OrderSide::Ask,
                },
                quantity_lots: *quantity_lots,
                kind: match kind {
                    IntentOrderKind::Limit { price_ticks } => {
                        kombat_chain_matching::OrderKind::Limit {
                            price_ticks: *price_ticks,
                        }
                    }
                    IntentOrderKind::Market => kombat_chain_matching::OrderKind::Market,
                },
                time_in_force: match time_in_force {
                    IntentTimeInForce::Gtc => kombat_chain_matching::TimeInForce::Gtc,
                    IntentTimeInForce::Ioc => kombat_chain_matching::TimeInForce::Ioc,
                    IntentTimeInForce::Fok => kombat_chain_matching::TimeInForce::Fok,
                    IntentTimeInForce::Gtt { expires_at_ms } => {
                        kombat_chain_matching::TimeInForce::Gtt {
                            expires_at_ms: *expires_at_ms,
                        }
                    }
                },
                post_only: *post_only,
            };
            ensure_order_preserves_initial_margin(
                order_books,
                isolated_collateral,
                markets,
                market_marks,
                positions,
                &replacement,
                Some(*target_client_order_id),
                timestamp_ms,
            )?;
            ensure_order_preserves_mandate_position_attribution(
                policy_store,
                order_books,
                resting_order_mandates,
                positions,
                signed_intent,
                &replacement,
                Some(*target_client_order_id),
                timestamp_ms,
            )?;
            ensure_order_preserves_mandate_max_leverage(
                policy_store,
                mandates,
                order_books,
                isolated_collateral,
                markets,
                market_marks,
                positions,
                signed_intent,
                &replacement,
                Some(*target_client_order_id),
                timestamp_ms,
            )?;
            let prior_book = order_books.get(market_id).cloned();
            let outcome = order_books
                .entry(*market_id)
                .or_insert_with(|| SingleMarketBook::new(*market_id))
                .replace(
                    signed_intent.intent.account,
                    *target_client_order_id,
                    replacement,
                    timestamp_ms,
                )
                .map_err(NodeError::Order)?;
            let mut rollback_events = mandate_fill_rollback_events(
                policy_store,
                mandates,
                account_operators,
                markets,
                market_marks,
                isolated_collateral,
                positions,
                resting_order_mandates,
                *market_id,
                signed_intent,
                &outcome.fills,
                timestamp_ms,
            );
            rollback_events.extend(mandate_position_attribution_rollback_events(
                policy_store,
                mandates,
                resting_order_mandates,
                positions,
                signed_intent,
                *market_id,
                &outcome.fills,
                timestamp_ms,
            ));
            if !rollback_events.is_empty() {
                restore_order_book(order_books, *market_id, prior_book);
                // Policy counters and nonce are intentionally retained: the
                // intent reached Fill-mode and cannot probe books for free.
                return Ok(rollback_events);
            }
            resting_order_mandates.remove(&resting_order_key(
                *market_id,
                signed_intent.intent.account,
                *target_client_order_id,
            ));
            let mut events = vec![RuntimeEventKind::OrderReplaced {
                market_id: *market_id,
                account_id: signed_intent.intent.account,
                target_client_order_id: *target_client_order_id,
                replacement_client_order_id: *replacement_client_order_id,
                status: order_lifecycle_status(outcome.status),
                filled_lots: outcome.filled_lots,
                remaining_lots: outcome.remaining_lots,
            }];
            events.extend(order_fill_events(
                *market_id,
                &outcome.fills,
                intent_origin_mandate_id(policy_store, signed_intent),
                resting_order_mandates,
            ));
            update_resting_order_mandates_from_outcome(
                resting_order_mandates,
                order_books,
                *market_id,
                &replacement,
                &outcome,
                intent_origin_mandate_id(policy_store, signed_intent),
                timestamp_ms,
            );
            Ok(events)
        }
        IntentKind::CreateSubPolicy {
            child_policy_id,
            child_grantee,
            child_expires_at,
            child_terms,
        } => {
            let parent =
                policy_store
                    .get(signed_intent.intent.policy_id)
                    .ok_or(NodeError::Policy(PolicyError::UnknownPolicy(
                        signed_intent.intent.policy_id,
                    )))?;
            let parent_account = parent.authority_account;
            let mut child = PolicyRecord::new(
                *child_policy_id,
                parent_account,
                *child_grantee,
                timestamp_ms,
                *child_expires_at,
                PolicyState::Active,
                policy_terms_from_wire(child_terms),
            )
            .with_execution_scope(parent.execution_scope);
            child.admission_rules = parent.admission_rules.clone();
            policy_store
                .create_sub_policy(signed_intent.intent.policy_id, child)
                .map_err(NodeError::Policy)?;
            Ok(vec![RuntimeEventKind::PolicyCreated {
                account_id: parent_account,
                parent_policy_id: Some(signed_intent.intent.policy_id),
                policy_id: *child_policy_id,
            }])
        }
        IntentKind::RevokePolicy { target_policy_id } => {
            let revocation_scope = policy_store
                .revocation_scope(*target_policy_id)
                .map_err(NodeError::Policy)?;
            policy_store
                .revoke(*target_policy_id, block_number, timestamp_ms)
                .map_err(NodeError::Policy)?;
            let events = revocation_scope
                .into_iter()
                .enumerate()
                .map(|(index, policy_id)| {
                    let account_id = policy_store
                        .get(policy_id)
                        .map(|policy| policy.authority_account)
                        .ok_or(NodeError::Policy(PolicyError::UnknownPolicy(policy_id)))?;
                    Ok(RuntimeEventKind::PolicyRevoked {
                        account_id,
                        policy_id,
                        cascaded: index > 0,
                    })
                })
                .collect::<Result<Vec<_>, NodeError>>()?;
            Ok(events)
        }
        IntentKind::DepositCollateral {
            market_id,
            amount_usdc,
        } => apply_collateral_intent_side_effect(
            order_books,
            isolated_collateral,
            markets,
            market_marks,
            positions,
            signed_intent.intent.account,
            *market_id,
            *amount_usdc,
            CollateralIntentSide::Deposit,
            timestamp_ms,
        ),
        IntentKind::WithdrawCollateral {
            market_id,
            amount_usdc,
        } => apply_collateral_intent_side_effect(
            order_books,
            isolated_collateral,
            markets,
            market_marks,
            positions,
            signed_intent.intent.account,
            *market_id,
            *amount_usdc,
            CollateralIntentSide::Withdraw,
            timestamp_ms,
        ),
    }
}

#[derive(Clone, Copy)]
pub(super) enum CollateralIntentSide {
    Deposit,
    Withdraw,
}

pub(super) fn apply_collateral_intent_side_effect(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &mut BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    account_id: u64,
    market_id: u64,
    amount_usdc: u64,
    side: CollateralIntentSide,
    timestamp_ms: u64,
) -> Result<Vec<RuntimeEventKind>, NodeError> {
    if amount_usdc == 0 {
        return Err(NodeError::InvalidCollateralMovement {
            market_id,
            account_id,
            reason: "zero_collateral_amount",
        });
    }
    ensure_market_accepts_collateral_movement(markets, market_id)?;
    let amount = i128::from(amount_usdc);
    let key = (market_id, account_id);
    let current = isolated_collateral.get(&key).copied().unwrap_or(0);
    let next = match side {
        CollateralIntentSide::Deposit => current.saturating_add(amount),
        CollateralIntentSide::Withdraw => {
            if current < amount {
                return Err(NodeError::InvalidCollateralMovement {
                    market_id,
                    account_id,
                    reason: "insufficient_collateral",
                });
            }
            let next = current.saturating_sub(amount);
            ensure_withdrawal_preserves_initial_margin(
                order_books,
                markets,
                market_marks,
                positions,
                market_id,
                account_id,
                next,
                timestamp_ms,
            )?;
            next
        }
    };

    if next == 0 {
        isolated_collateral.remove(&key);
    } else {
        isolated_collateral.insert(key, next);
    }

    let event = match side {
        CollateralIntentSide::Deposit => RuntimeEventKind::CollateralDeposited {
            market_id,
            account_id,
            amount_usdc,
            collateral: next,
        },
        CollateralIntentSide::Withdraw => RuntimeEventKind::CollateralWithdrawn {
            market_id,
            account_id,
            amount_usdc,
            collateral: next,
        },
    };
    Ok(vec![event])
}

pub(super) fn ensure_withdrawal_preserves_initial_margin(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    market_id: u64,
    account_id: u64,
    next_collateral: i128,
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    let ledger = positions
        .get(&(market_id, account_id))
        .copied()
        .unwrap_or_default();
    let has_active_orders = order_books
        .get(&market_id)
        .is_some_and(|book| !book.account_orders(account_id, timestamp_ms, 1).is_empty());
    if ledger.open_abs_lots() == 0 && !has_active_orders {
        return Ok(());
    }
    let mark_price = market_marks
        .get(&market_id)
        .map(|price| price.price_ticks)
        .ok_or(NodeError::StaleMarketData { market_id })?;
    let Some(market) = markets.get(&market_id) else {
        return Ok(());
    };
    let raw_notional = initial_margin_notional_at_risk(
        order_books,
        &ledger,
        market_id,
        account_id,
        mark_price,
        timestamp_ms,
        None,
        &[],
    );
    let notional = scaled_raw_tick_lot_notional_usdc(market, raw_notional);
    if notional == 0 {
        return Ok(());
    }
    let required_initial = proportional_bps_round_up(notional, market.config.initial_margin_bps);
    let equity_after =
        scaled_ledger_equity_usdc(market, next_collateral, &ledger, Some(mark_price));
    if equity_after < u128_to_i128_saturating(required_initial) {
        return Err(NodeError::InvalidCollateralMovement {
            market_id,
            account_id,
            reason: "insufficient_initial_margin",
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ensure_order_preserves_initial_margin(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    let Some(preview) = order_initial_margin_preview(
        order_books,
        isolated_collateral,
        markets,
        market_marks,
        positions,
        order,
        replace_client_order_id,
        timestamp_ms,
    )?
    else {
        return Ok(());
    };
    if preview.would_succeed {
        return Ok(());
    }
    Err(NodeError::InvalidCollateralMovement {
        market_id: order.market_id,
        account_id: order.account_id,
        reason: preview.reason.unwrap_or("insufficient_initial_margin"),
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn order_initial_margin_preview(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> Result<Option<OrderInitialMarginPreview>, NodeError> {
    let Some(market) = markets.get(&order.market_id) else {
        return Ok(None);
    };
    let mark_price = market_marks
        .get(&order.market_id)
        .map(|price| price.price_ticks)
        .ok_or(NodeError::StaleMarketData {
            market_id: order.market_id,
        })?;
    let ledger = positions
        .get(&(order.market_id, order.account_id))
        .copied()
        .unwrap_or_default();
    let current_raw_notional = initial_margin_notional_at_risk(
        order_books,
        &ledger,
        order.market_id,
        order.account_id,
        mark_price,
        timestamp_ms,
        None,
        &[],
    );
    let candidate_raw_notional = initial_margin_notional_at_risk(
        order_books,
        &ledger,
        order.market_id,
        order.account_id,
        mark_price,
        timestamp_ms,
        replace_client_order_id,
        &[*order],
    );

    let current_notional = scaled_raw_tick_lot_notional_usdc(market, current_raw_notional);
    let candidate_notional = scaled_raw_tick_lot_notional_usdc(market, candidate_raw_notional);
    let required_initial =
        proportional_bps_round_up(candidate_notional, market.config.initial_margin_bps);
    let collateral = isolated_collateral
        .get(&(order.market_id, order.account_id))
        .copied()
        .unwrap_or(0);
    let equity = scaled_ledger_equity_usdc(market, collateral, &ledger, Some(mark_price));
    let required_initial_i128 = u128_to_i128_saturating(required_initial);
    let would_succeed =
        candidate_raw_notional <= current_raw_notional || equity >= required_initial_i128;
    Ok(Some(OrderInitialMarginPreview {
        market_id: order.market_id,
        account_id: order.account_id,
        would_succeed,
        reason: if would_succeed {
            None
        } else {
            Some("insufficient_initial_margin")
        },
        current_raw_notional,
        candidate_raw_notional,
        current_notional,
        candidate_notional,
        required_initial,
        equity_for_market: equity,
        margin_surplus: equity.saturating_sub(required_initial_i128),
    }))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ensure_order_preserves_mandate_max_leverage(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    if let Some((_, agent, _)) = mandate_max_leverage_order_violation(
        policy_store,
        mandates,
        order_books,
        isolated_collateral,
        markets,
        market_marks,
        positions,
        signed_intent,
        order,
        replace_client_order_id,
        timestamp_ms,
    ) {
        return Err(NodeError::InvalidMandate {
            agent,
            reason: "max_leverage_exceeded",
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn mandate_max_leverage_admission_violation(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    timestamp_ms: u64,
) -> Option<(MandateId, u64, u16)> {
    let (order, replace_client_order_id) = mandate_leverage_order_from_intent(signed_intent)?;
    mandate_max_leverage_order_violation(
        policy_store,
        mandates,
        order_books,
        isolated_collateral,
        markets,
        market_marks,
        positions,
        signed_intent,
        &order,
        replace_client_order_id,
        timestamp_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn mandate_max_leverage_order_violation(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> Option<(MandateId, u64, u16)> {
    let policy = policy_store.get(signed_intent.intent.policy_id)?;
    if !matches!(policy.origin, PolicyOrigin::MandateGenerated { .. }) {
        return None;
    }
    for (mandate_id, _) in representative_mandate_policies(
        policy_store,
        (policy.authority_account, policy.execution_scope),
    ) {
        let Some(mandate) = mandates.get(&mandate_id) else {
            continue;
        };
        if mandate.state != MandateState::Active {
            continue;
        }
        for (constraint_index, constraint) in mandate.constraints.iter().enumerate() {
            let MandateConstraint::MaxLeverage { mult } = constraint else {
                continue;
            };
            if order_exceeds_max_leverage(
                order_books,
                isolated_collateral,
                markets,
                market_marks,
                positions,
                order,
                replace_client_order_id,
                *mult,
                timestamp_ms,
            ) {
                return Some((
                    mandate_id,
                    mandate.agent,
                    u16::try_from(constraint_index).unwrap_or(u16::MAX),
                ));
            }
        }
    }
    None
}

pub(super) fn mandate_leverage_order_from_intent(
    signed_intent: &SignedIntent,
) -> Option<(OrderRequest, Option<u64>)> {
    match &signed_intent.intent.kind {
        IntentKind::PlaceOrder { .. } => {
            order_request_from_place_order_intent(signed_intent).map(|order| (order, None))
        }
        IntentKind::ReplaceOrder {
            market_id,
            target_client_order_id,
            replacement_client_order_id,
            side,
            quantity_lots,
            kind,
            time_in_force,
            post_only,
        } => Some((
            OrderRequest {
                market_id: *market_id,
                account_id: signed_intent.intent.account,
                client_order_id: *replacement_client_order_id,
                side: match side {
                    IntentOrderSide::Bid => kombat_chain_matching::OrderSide::Bid,
                    IntentOrderSide::Ask => kombat_chain_matching::OrderSide::Ask,
                },
                quantity_lots: *quantity_lots,
                kind: match kind {
                    IntentOrderKind::Limit { price_ticks } => {
                        kombat_chain_matching::OrderKind::Limit {
                            price_ticks: *price_ticks,
                        }
                    }
                    IntentOrderKind::Market => kombat_chain_matching::OrderKind::Market,
                },
                time_in_force: match time_in_force {
                    IntentTimeInForce::Gtc => kombat_chain_matching::TimeInForce::Gtc,
                    IntentTimeInForce::Ioc => kombat_chain_matching::TimeInForce::Ioc,
                    IntentTimeInForce::Fok => kombat_chain_matching::TimeInForce::Fok,
                    IntentTimeInForce::Gtt { expires_at_ms } => {
                        kombat_chain_matching::TimeInForce::Gtt {
                            expires_at_ms: *expires_at_ms,
                        }
                    }
                },
                post_only: *post_only,
            },
            Some(*target_client_order_id),
        )),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn order_exceeds_max_leverage(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    max_leverage: u32,
    timestamp_ms: u64,
) -> bool {
    if matches!(&order.kind, kombat_chain_matching::OrderKind::Market) {
        return false;
    }
    let Some(market) = markets.get(&order.market_id) else {
        return true;
    };
    let Some(mark_price) = market_marks
        .get(&order.market_id)
        .map(|price| price.price_ticks)
    else {
        return true;
    };
    let ledger = positions
        .get(&(order.market_id, order.account_id))
        .copied()
        .unwrap_or_default();
    let current_raw_notional = initial_margin_notional_at_risk(
        order_books,
        &ledger,
        order.market_id,
        order.account_id,
        mark_price,
        timestamp_ms,
        None,
        &[],
    );
    let candidate_raw_notional = initial_margin_notional_at_risk(
        order_books,
        &ledger,
        order.market_id,
        order.account_id,
        mark_price,
        timestamp_ms,
        replace_client_order_id,
        &[*order],
    );
    if candidate_raw_notional <= current_raw_notional {
        return false;
    }
    let candidate_notional = scaled_raw_tick_lot_notional_usdc(market, candidate_raw_notional);
    let collateral = isolated_collateral
        .get(&(order.market_id, order.account_id))
        .copied()
        .unwrap_or(0);
    let equity = scaled_ledger_equity_usdc(market, collateral, &ledger, Some(mark_price));
    leverage_exceeded(candidate_notional, equity, max_leverage)
}

pub(super) fn ensure_order_preserves_mandate_position_attribution(
    policy_store: &PolicyStore,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    let Some(origin_mandate_id) = intent_origin_mandate_id(policy_store, signed_intent) else {
        return Ok(());
    };
    let ledger = positions
        .get(&(order.market_id, order.account_id))
        .copied()
        .unwrap_or_default();
    let Some(existing_mandate_id) = ledger.mandate_id else {
        return Ok(());
    };
    if existing_mandate_id == origin_mandate_id || ledger.open_abs_lots() == 0 {
        return Ok(());
    }
    if !order_may_post_after_matching(order_books, order, replace_client_order_id, timestamp_ms)? {
        return Ok(());
    }
    if !order_side_reduces_position(order.side, &ledger) {
        return Err(NodeError::InvalidMandate {
            agent: order.account_id,
            reason: "mixed_mandate_resting_order",
        });
    }

    let existing_foreign_reducing_lots = active_foreign_mandate_reducing_order_lots(
        order_books,
        resting_order_mandates,
        order.market_id,
        order.account_id,
        existing_mandate_id,
        order.side,
        replace_client_order_id,
        timestamp_ms,
    );
    if existing_foreign_reducing_lots.saturating_add(order.quantity_lots) > ledger.open_abs_lots() {
        return Err(NodeError::InvalidMandate {
            agent: order.account_id,
            reason: "mixed_mandate_resting_order",
        });
    }
    Ok(())
}

pub(super) fn order_may_post_after_matching(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    order: &OrderRequest,
    replace_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> Result<bool, NodeError> {
    let mut book = order_books
        .get(&order.market_id)
        .cloned()
        .unwrap_or_else(|| SingleMarketBook::new(order.market_id));
    if let Some(client_order_id) = replace_client_order_id {
        let _ = book.cancel(order.account_id, client_order_id, timestamp_ms);
    }
    let preview = book
        .preview(order, timestamp_ms)
        .map_err(NodeError::Order)?;
    let pre_match = pre_match_check(order, preview, timestamp_ms).map_err(NodeError::Order)?;
    Ok(pre_match.may_post && !preview.fully_fillable)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn active_foreign_mandate_reducing_order_lots(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    market_id: u64,
    account_id: u64,
    existing_mandate_id: MandateId,
    reducing_side: MatchingOrderSide,
    exclude_client_order_id: Option<u64>,
    timestamp_ms: u64,
) -> u64 {
    let Some(book) = order_books.get(&market_id) else {
        return 0;
    };
    book.account_orders(account_id, timestamp_ms, usize::MAX)
        .into_iter()
        .filter(|order| order.side == reducing_side)
        .filter(|order| {
            !exclude_client_order_id.is_some_and(|client_id| client_id == order.client_order_id)
        })
        .filter(|order| {
            resting_order_mandates
                .get(&resting_order_key(
                    market_id,
                    account_id,
                    order.client_order_id,
                ))
                .is_some_and(|mandate_id| *mandate_id != existing_mandate_id)
        })
        .fold(0_u64, |total, order| {
            total.saturating_add(order.remaining_lots)
        })
}

pub(super) fn order_side_reduces_position(
    side: MatchingOrderSide,
    ledger: &PositionLedger,
) -> bool {
    match ledger.net_lots().cmp(&0) {
        std::cmp::Ordering::Greater => side == MatchingOrderSide::Ask,
        std::cmp::Ordering::Less => side == MatchingOrderSide::Bid,
        std::cmp::Ordering::Equal => false,
    }
}

pub(super) fn ensure_resting_order_batch_preserves_mandate_position_attribution(
    policy_store: &PolicyStore,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    orders: &[OrderRequest],
    signed_intents: &[&SignedIntent],
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    debug_assert_eq!(orders.len(), signed_intents.len());
    let mut pending_foreign_reducing_lots = BTreeMap::<(u64, u64, MandateId, u8), u64>::new();
    for (order, signed_intent) in orders.iter().zip(signed_intents.iter()) {
        ensure_order_preserves_mandate_position_attribution(
            policy_store,
            order_books,
            resting_order_mandates,
            positions,
            signed_intent,
            order,
            None,
            timestamp_ms,
        )?;
        let Some(origin_mandate_id) = intent_origin_mandate_id(policy_store, signed_intent) else {
            continue;
        };
        let ledger = positions
            .get(&(order.market_id, order.account_id))
            .copied()
            .unwrap_or_default();
        let Some(existing_mandate_id) = ledger.mandate_id else {
            continue;
        };
        if existing_mandate_id == origin_mandate_id
            || ledger.open_abs_lots() == 0
            || !order_side_reduces_position(order.side, &ledger)
            || !order_may_post_after_matching(order_books, order, None, timestamp_ms)?
        {
            continue;
        }
        let side_key = match order.side {
            MatchingOrderSide::Bid => 0,
            MatchingOrderSide::Ask => 1,
        };
        let key = (
            order.market_id,
            order.account_id,
            existing_mandate_id,
            side_key,
        );
        let existing_foreign_reducing_lots = active_foreign_mandate_reducing_order_lots(
            order_books,
            resting_order_mandates,
            order.market_id,
            order.account_id,
            existing_mandate_id,
            order.side,
            None,
            timestamp_ms,
        );
        let pending = pending_foreign_reducing_lots
            .get(&key)
            .copied()
            .unwrap_or(0);
        if existing_foreign_reducing_lots
            .saturating_add(pending)
            .saturating_add(order.quantity_lots)
            > ledger.open_abs_lots()
        {
            return Err(NodeError::InvalidMandate {
                agent: order.account_id,
                reason: "mixed_mandate_resting_order",
            });
        }
        pending_foreign_reducing_lots.insert(key, pending.saturating_add(order.quantity_lots));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ensure_resting_order_batch_preserves_initial_margin(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    orders: &[OrderRequest],
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    let mut simulated_order_books = BTreeMap::new();
    for order in orders {
        simulation_order_book_for_market(&mut simulated_order_books, order_books, order.market_id);
        ensure_order_preserves_initial_margin(
            &simulated_order_books,
            isolated_collateral,
            markets,
            market_marks,
            positions,
            order,
            None,
            timestamp_ms,
        )?;
        simulated_order_books
            .entry(order.market_id)
            .or_insert_with(|| SingleMarketBook::new(order.market_id))
            .submit(*order, timestamp_ms)
            .map_err(NodeError::Order)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ensure_resting_order_batch_preserves_mandate_max_leverage(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    orders: &[OrderRequest],
    signed_intents: &[&SignedIntent],
    timestamp_ms: u64,
) -> Result<(), NodeError> {
    debug_assert_eq!(orders.len(), signed_intents.len());
    let mut simulated_order_books = BTreeMap::new();
    for (order, signed_intent) in orders.iter().zip(signed_intents.iter()) {
        simulation_order_book_for_market(&mut simulated_order_books, order_books, order.market_id);
        ensure_order_preserves_mandate_max_leverage(
            policy_store,
            mandates,
            &simulated_order_books,
            isolated_collateral,
            markets,
            market_marks,
            positions,
            signed_intent,
            order,
            None,
            timestamp_ms,
        )?;
        simulated_order_books
            .entry(order.market_id)
            .or_insert_with(|| SingleMarketBook::new(order.market_id))
            .submit(*order, timestamp_ms)
            .map_err(NodeError::Order)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn initial_margin_notional_at_risk(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    ledger: &PositionLedger,
    market_id: u64,
    account_id: u64,
    mark_price: u64,
    timestamp_ms: u64,
    exclude_client_order_id: Option<u64>,
    additional_orders: &[OrderRequest],
) -> u128 {
    let position_notional =
        u128::from(mark_price).saturating_mul(u128::from(ledger.open_abs_lots()));
    let (mut bid_orders, mut ask_orders) = active_account_order_prices(
        order_books,
        market_id,
        account_id,
        mark_price,
        timestamp_ms,
        exclude_client_order_id,
        additional_orders,
    );
    let bid_notional = exposure_increasing_notional(
        &mut bid_orders,
        if ledger.net_lots() < 0 {
            ledger.open_abs_lots()
        } else {
            0
        },
    );
    let ask_notional = exposure_increasing_notional(
        &mut ask_orders,
        if ledger.net_lots() > 0 {
            ledger.open_abs_lots()
        } else {
            0
        },
    );
    position_notional
        .saturating_add(bid_notional)
        .saturating_add(ask_notional)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn active_account_order_prices(
    order_books: &BTreeMap<u64, SingleMarketBook>,
    market_id: u64,
    account_id: u64,
    mark_price: u64,
    timestamp_ms: u64,
    exclude_client_order_id: Option<u64>,
    additional_orders: &[OrderRequest],
) -> (Vec<(u64, u64)>, Vec<(u64, u64)>) {
    let mut bids = Vec::new();
    let mut asks = Vec::new();
    if let Some(book) = order_books.get(&market_id) {
        for order in book.account_orders(account_id, timestamp_ms, usize::MAX) {
            if exclude_client_order_id.is_some_and(|client_id| client_id == order.client_order_id) {
                continue;
            }
            match order.side {
                MatchingOrderSide::Bid => bids.push((order.price_ticks, order.remaining_lots)),
                MatchingOrderSide::Ask => asks.push((order.price_ticks, order.remaining_lots)),
            }
        }
    }
    for order in additional_orders {
        if order.market_id != market_id || order.account_id != account_id {
            continue;
        }
        let price_ticks = match order.kind {
            MatchingOrderKind::Limit { price_ticks } => price_ticks,
            MatchingOrderKind::Market => mark_price,
        };
        match order.side {
            MatchingOrderSide::Bid => bids.push((price_ticks, order.quantity_lots)),
            MatchingOrderSide::Ask => asks.push((price_ticks, order.quantity_lots)),
        }
    }
    (bids, asks)
}

pub(super) fn exposure_increasing_notional(orders: &mut [(u64, u64)], reducing_lots: u64) -> u128 {
    // Cheapest-first reduction is exact for asks reducing longs and conservative for
    // bids reducing shorts: higher-priced excess bids remain margin-reserved.
    orders.sort_unstable_by_key(|(price_ticks, _)| *price_ticks);
    let mut remaining_reduction = reducing_lots;
    orders.iter().fold(0_u128, |total, (price_ticks, lots)| {
        let reducing = remaining_reduction.min(*lots);
        remaining_reduction = remaining_reduction.saturating_sub(reducing);
        let increasing_lots = lots.saturating_sub(reducing);
        total.saturating_add(u128::from(*price_ticks).saturating_mul(u128::from(increasing_lots)))
    })
}
