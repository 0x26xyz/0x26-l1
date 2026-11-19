use super::*;

type PolicyAuthorityScope = (u64, PolicyExecutionScope);

pub(super) fn ensure_market_accepts_new_orders(
    markets: &BTreeMap<u64, PerpetualMarket>,
    paused_markets: &BTreeSet<u64>,
    market_id: u64,
) -> Result<(), NodeError> {
    if let Some(market) = markets.get(&market_id) {
        // Full reduce-only order classification is position-aware; until that lands,
        // ReduceOnly is conservatively treated like Paused for new order admission.
        if market.status == MarketStatus::Active && !paused_markets.contains(&market_id) {
            return Ok(());
        }
        return Err(NodeError::MarketTradingPaused { market_id });
    }
    if markets.is_empty() && !paused_markets.contains(&market_id) {
        return Ok(());
    }
    if paused_markets.contains(&market_id) {
        return Err(NodeError::MarketTradingPaused { market_id });
    }
    Err(NodeError::UnknownMarket { market_id })
}

pub(super) fn ensure_market_accepts_collateral_movement(
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_id: u64,
) -> Result<(), NodeError> {
    if let Some(market) = markets.get(&market_id) {
        return match market.status {
            MarketStatus::Active | MarketStatus::Paused | MarketStatus::ReduceOnly => Ok(()),
            MarketStatus::Delisted => Err(NodeError::UnknownMarket { market_id }),
        };
    }
    if markets.is_empty() {
        return Ok(());
    }
    Err(NodeError::UnknownMarket { market_id })
}

pub(super) fn new_order_market_id(signed_intent: &SignedIntent) -> Option<u64> {
    let market_id = order_book_market_id(signed_intent)?;

    if !matches!(
        &signed_intent.intent.kind,
        IntentKind::PlaceOrder { .. } | IntentKind::ReplaceOrder { .. }
    ) {
        return None;
    }

    Some(market_id)
}

pub(super) fn paused_order_market_id(
    signed_intent: &SignedIntent,
    paused_markets: &BTreeSet<u64>,
) -> Option<u64> {
    let market_id = new_order_market_id(signed_intent)?;

    paused_markets.contains(&market_id).then_some(market_id)
}

pub(super) fn stale_order_market_id(
    signed_intent: &SignedIntent,
    timestamp_ms: u64,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
) -> Option<u64> {
    let market_id = order_book_market_id(signed_intent)?;

    if !matches!(
        &signed_intent.intent.kind,
        IntentKind::PlaceOrder { .. } | IntentKind::ReplaceOrder { .. }
    ) {
        return None;
    }

    (!market_price_inputs_ready_at(
        market_id,
        timestamp_ms,
        market_marks,
        market_index_prices,
        market_oracle_config,
    ))
    .then_some(market_id)
}

pub(super) fn order_book_market_id(signed_intent: &SignedIntent) -> Option<u64> {
    match &signed_intent.intent.kind {
        IntentKind::PlaceOrder { market_id, .. }
        | IntentKind::CancelOrder { market_id, .. }
        | IntentKind::ReplaceOrder { market_id, .. } => Some(*market_id),
        _ => None,
    }
}

pub(super) fn collateral_market_id(signed_intent: &SignedIntent) -> Option<u64> {
    match &signed_intent.intent.kind {
        IntentKind::DepositCollateral { market_id, .. }
        | IntentKind::WithdrawCollateral { market_id, .. } => Some(*market_id),
        _ => None,
    }
}

pub(super) fn queue_pending_reason(
    engine: &SingleNodeEngine,
    signed_intent: &SignedIntent,
    stage: IntentStatusStage,
) -> Option<IntentPendingReason> {
    if stage == IntentStatusStage::AcceptedConditional
        && intent_not_before_ms(&signed_intent.intent).is_some()
    {
        return Some(IntentPendingReason::TimeAfter);
    }

    let current_policy_nonce = engine
        .policy_store
        .next_intent_nonce(signed_intent.intent.policy_id)
        .ok();
    if current_policy_nonce.is_some_and(|next_nonce| signed_intent.intent.nonce > next_nonce) {
        return Some(IntentPendingReason::NonceFrontier);
    }

    if paused_order_market_id(signed_intent, &engine.paused_markets).is_some() {
        return Some(IntentPendingReason::MarketTradingPaused);
    }

    if stale_order_market_id(
        signed_intent,
        engine.current_time_ms(),
        &engine.market_marks,
        &engine.market_index_prices,
        &engine.market_oracle_config,
    )
    .is_some()
    {
        return Some(IntentPendingReason::StaleMarketData);
    }

    None
}

pub(super) fn transient_intent_blocker(error: &NodeError) -> bool {
    matches!(
        error,
        NodeError::MarketTradingPaused { .. } | NodeError::StaleMarketData { .. }
    )
}

pub(super) fn market_price_inputs_ready_at(
    market_id: u64,
    timestamp_ms: u64,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_index_prices: &BTreeMap<u64, OraclePrice>,
    market_oracle_config: &BTreeMap<u64, OracleConfig>,
) -> bool {
    let oracle_config = market_oracle_config
        .get(&market_id)
        .copied()
        .unwrap_or_default();
    let mark_ready = price_status(
        timestamp_ms,
        market_marks.get(&market_id).copied(),
        oracle_config,
    )
    .is_none_or(|status| !status.stale);
    let index_ready = price_status(
        timestamp_ms,
        market_index_prices.get(&market_id).copied(),
        oracle_config,
    )
    .is_none_or(|status| !status.stale);

    mark_ready && index_ready
}

pub(super) fn block_oracle_value_from_price(price: &OraclePrice) -> BlockOracleValue {
    BlockOracleValue {
        price_ticks: price.price_ticks,
        timestamp_ms: price.timestamp_ms,
    }
}

pub(super) fn oracle_price_from_block_value(value: BlockOracleValue) -> Option<OraclePrice> {
    (value.price_ticks > 0 && value.timestamp_ms > 0).then_some(OraclePrice {
        price_ticks: value.price_ticks,
        timestamp_ms: value.timestamp_ms,
    })
}

pub(super) fn insert_newer_oracle_price(
    prices: &mut BTreeMap<u64, OraclePrice>,
    market_id: u64,
    price: OraclePrice,
) {
    let should_insert = prices
        .get(&market_id)
        .is_none_or(|existing| price.timestamp_ms >= existing.timestamp_ms);
    if should_insert {
        prices.insert(market_id, price);
    }
}

pub(super) fn oracle_price_maps_from_block_observations(
    oracle_prices: &[BlockOraclePrice],
) -> (BTreeMap<u64, OraclePrice>, BTreeMap<u64, OraclePrice>) {
    let mut market_marks = BTreeMap::new();
    let mut market_index_prices = BTreeMap::new();
    for observation in oracle_prices {
        if observation.market_id == 0 {
            continue;
        }
        if let Some(mark) = observation.mark.and_then(oracle_price_from_block_value) {
            insert_newer_oracle_price(&mut market_marks, observation.market_id, mark);
        }
        if let Some(index) = observation.index.and_then(oracle_price_from_block_value) {
            insert_newer_oracle_price(&mut market_index_prices, observation.market_id, index);
        }
    }
    (market_marks, market_index_prices)
}

pub(super) fn compact_block_oracle_prices(
    oracle_prices: &[BlockOraclePrice],
) -> Vec<BlockOraclePrice> {
    let (market_marks, market_index_prices) =
        oracle_price_maps_from_block_observations(oracle_prices);
    let mut market_ids = BTreeSet::new();
    market_ids.extend(market_marks.keys().copied());
    market_ids.extend(market_index_prices.keys().copied());
    market_ids
        .into_iter()
        .map(|market_id| BlockOraclePrice {
            market_id,
            mark: market_marks
                .get(&market_id)
                .map(block_oracle_value_from_price),
            index: market_index_prices
                .get(&market_id)
                .map(block_oracle_value_from_price),
        })
        .collect()
}

pub(super) fn policy_admission_for_intent(
    signed_intent: &SignedIntent,
    markets: &BTreeMap<u64, PerpetualMarket>,
    expected_chain_id: u64,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<PolicyAdmission, NodeError> {
    let (gas_cost, volume, notional_outstanding) =
        direct_intent_costs(&signed_intent.intent.kind, markets);

    Ok(PolicyAdmission {
        signed_session: signed_session_payload_for_intent(signed_intent),
        intent_nonce: signed_intent.intent.nonce,
        block_number,
        expected_chain_id,
        timestamp_ms,
        gas_cost,
        volume,
        notional_outstanding,
        intent_notional: volume,
        market_id: intent_market_id(&signed_intent.intent.kind),
        realized_pnl_delta: 0,
        current_drawdown_pct: 0,
    })
}

pub(super) fn policy_account_scope_for_intent(
    policy_store: &PolicyStore,
    delegations: &BTreeMap<DelegationId, Delegation>,
    signed_intent: &SignedIntent,
) -> Result<PolicyAuthorityScope, NodeError> {
    let Some(policy) = policy_store.get(signed_intent.intent.policy_id) else {
        return Err(NodeError::Policy(PolicyError::UnknownPolicy(
            signed_intent.intent.policy_id,
        )));
    };

    if account_id_is_delegation_reserved(policy.authority_account) {
        return Err(NodeError::InvalidDelegation {
            agent: policy.authority_account,
            reason: "reserved_delegation_account_policy",
        });
    }

    match policy.execution_scope {
        PolicyExecutionScope::Account(execution_account) => {
            if signed_intent.intent.account == execution_account {
                return Ok((policy.authority_account, policy.execution_scope));
            }
        }
        PolicyExecutionScope::DelegationTrading { delegation_id } => {
            let Some(delegation) = delegations.get(&delegation_id) else {
                return Err(NodeError::InvalidDelegation {
                    agent: policy.authority_account,
                    reason: "unknown_delegation",
                });
            };
            if !delegation_accepts_trading_intents(delegation.status) {
                return Err(NodeError::InvalidDelegation {
                    agent: policy.authority_account,
                    reason: "delegation_not_live",
                });
            }
            let trading_account = delegation_trading_account_id(&delegation_id);
            if signed_intent.intent.account != trading_account {
                return Err(NodeError::InvalidDelegation {
                    agent: policy.authority_account,
                    reason: "intent_execution_scope_mismatch",
                });
            }
            if delegation.agent != policy.authority_account {
                return Err(NodeError::InvalidDelegation {
                    agent: policy.authority_account,
                    reason: "delegation_agent_mismatch",
                });
            }
            return Ok((policy.authority_account, policy.execution_scope));
        }
    }

    let PolicyOrigin::MandateGenerated { mandate_id, .. } = policy.origin else {
        return Err(NodeError::InvalidDelegation {
            agent: policy.authority_account,
            reason: "intent_policy_account_mismatch",
        });
    };

    let Some(delegation) =
        live_delegation_for_trading_account(delegations, signed_intent.intent.account)
    else {
        return Err(NodeError::InvalidDelegation {
            agent: policy.authority_account,
            reason: "unknown_delegation_trading_account",
        });
    };

    if !delegation_accepts_trading_intents(delegation.status) {
        return Err(NodeError::InvalidDelegation {
            agent: policy.authority_account,
            reason: "delegation_not_live",
        });
    }
    if delegation.agent != policy.authority_account {
        return Err(NodeError::InvalidDelegation {
            agent: policy.authority_account,
            reason: "delegation_agent_mismatch",
        });
    }
    if delegation.mandate_id != mandate_id {
        return Err(NodeError::InvalidDelegation {
            agent: policy.authority_account,
            reason: "delegation_mandate_mismatch",
        });
    }

    Ok((policy.authority_account, policy.execution_scope))
}

pub(super) fn commit_stacked_mandate_admission(
    policy_store: &mut PolicyStore,
    signed_intent: &SignedIntent,
    admission: &PolicyAdmission,
    policy_account_scope: PolicyAuthorityScope,
) -> Result<(), NodeError> {
    let Some(origin_policy) = policy_store.get(signed_intent.intent.policy_id) else {
        return Err(NodeError::Policy(PolicyError::UnknownPolicy(
            signed_intent.intent.policy_id,
        )));
    };
    let PolicyOrigin::MandateGenerated {
        mandate_id: origin_mandate_id,
        ..
    } = &origin_policy.origin
    else {
        return Ok(());
    };
    let origin_mandate_id = *origin_mandate_id;

    // The lowest generated policy index is the mandate-level counter bucket.
    // If an intent is signed by another session key, both that key's policy
    // counters and the representative counters advance. Callers run this on a
    // cloned policy store during block planning; if a later admission step
    // fails, they must discard the clone to avoid leaking partial advances.
    for (mandate_id, (_, representative_policy_id)) in
        representative_mandate_policies(policy_store, policy_account_scope)
    {
        if mandate_id == origin_mandate_id
            && representative_policy_id == signed_intent.intent.policy_id
        {
            continue;
        }
        policy_store
            .commit_admission_limits(representative_policy_id, admission)
            .map_err(NodeError::Policy)?;
    }

    Ok(())
}

pub(super) fn representative_mandate_policies(
    policy_store: &PolicyStore,
    scope: PolicyAuthorityScope,
) -> BTreeMap<MandateId, (u16, u64)> {
    let mut representative_policies = BTreeMap::<MandateId, (u16, u64)>::new();
    for policy in policy_store.records() {
        if policy.authority_account != scope.0
            || policy.execution_scope != scope.1
            || policy.state != PolicyState::Active
        {
            continue;
        }
        let PolicyOrigin::MandateGenerated {
            mandate_id,
            generated_policy_index,
            ..
        } = &policy.origin
        else {
            continue;
        };
        representative_policies
            .entry(*mandate_id)
            .and_modify(|current| {
                if *generated_policy_index < current.0 {
                    *current = (*generated_policy_index, policy.id);
                }
            })
            .or_insert((*generated_policy_index, policy.id));
    }
    representative_policies
}

pub(super) fn mandate_admission_rejection_reason(reason: IntentRejectionReason) -> bool {
    matches!(
        reason,
        IntentRejectionReason::MarketNotAllowed
            | IntentRejectionReason::TradeNotionalExceeded
            | IntentRejectionReason::NotionalOutstandingExceeded
            | IntentRejectionReason::MinuteIntentLimitExceeded
            | IntentRejectionReason::DrawdownExceededAndRevoked
            | IntentRejectionReason::MandateConstraintViolated
    )
}

pub(super) fn prune_timestamps_before(timestamps: &mut Vec<u64>, retain_from_ms: u64) {
    let first_retained = timestamps.partition_point(|entry| *entry < retain_from_ms);
    if first_retained > 0 {
        timestamps.drain(0..first_retained);
    }
}

pub(super) fn intent_market_id(kind: &IntentKind) -> Option<u64> {
    match kind {
        IntentKind::PlaceOrder { market_id, .. }
        | IntentKind::CancelOrder { market_id, .. }
        | IntentKind::ReplaceOrder { market_id, .. }
        | IntentKind::DepositCollateral { market_id, .. }
        | IntentKind::WithdrawCollateral { market_id, .. } => Some(*market_id),
        IntentKind::Transfer { .. }
        | IntentKind::Cancel { .. }
        | IntentKind::CreateSubPolicy { .. }
        | IntentKind::RevokePolicy { .. } => None,
    }
}

pub(super) fn policy_terms_from_wire(terms: &PolicyTermsWire) -> PolicyTerms {
    PolicyTerms {
        gas_budget_per_day: terms.gas_budget_per_day,
        gas_refill_per_day: terms.gas_refill_per_day,
        max_intents_per_second: terms.max_intents_per_second,
        max_intents_per_day: terms.max_intents_per_day,
        max_volume_per_day: terms.max_volume_per_day,
        max_drawdown_pct: terms.max_drawdown_pct,
        ..PolicyTerms::default()
    }
}

pub(super) fn owner_call_runtime_events(
    policy_store: &PolicyStore,
    account_id: u64,
    call: &OwnerCall,
) -> Result<Vec<RuntimeEventKind>, NodeError> {
    match &call.action {
        kombat_chain_policy::OwnerAction::CreatePolicy { policy_id, .. } => {
            Ok(vec![RuntimeEventKind::PolicyCreated {
                account_id,
                parent_policy_id: None,
                policy_id: *policy_id,
            }])
        }
        kombat_chain_policy::OwnerAction::RevokePolicy { policy_id } => {
            let revocation_scope = policy_store
                .revocation_scope(*policy_id)
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
        _ => Ok(vec![]),
    }
}

pub(super) fn account_update_runtime_event(
    account_store: &AccountStore,
    account_id: u64,
    call: &OwnerCall,
) -> Result<Option<RuntimeEventKind>, NodeError> {
    let action = match &call.action {
        kombat_chain_policy::OwnerAction::ProposeOwner { .. } => AccountUpdateAction::ProposeOwner,
        kombat_chain_policy::OwnerAction::AcceptOwner => AccountUpdateAction::AcceptOwner,
        kombat_chain_policy::OwnerAction::Pause => AccountUpdateAction::Pause,
        kombat_chain_policy::OwnerAction::Unpause => AccountUpdateAction::Unpause,
        kombat_chain_policy::OwnerAction::AddPolicy { .. } => AccountUpdateAction::AddPolicy,
        kombat_chain_policy::OwnerAction::RemovePolicy { .. } => AccountUpdateAction::RemovePolicy,
        kombat_chain_policy::OwnerAction::Upgrade { .. } => AccountUpdateAction::Upgrade,
        kombat_chain_policy::OwnerAction::CreatePolicy { .. }
        | kombat_chain_policy::OwnerAction::RevokePolicy { .. } => return Ok(None),
    };

    let account = account_store
        .get(account_id)
        .ok_or(NodeError::Account(AccountError::UnknownAccount(account_id)))?;

    Ok(account_update_runtime_event_for_account(
        account, action, None,
    ))
}

pub(super) fn account_update_runtime_event_for_account(
    account: &StandardAgentAccount,
    action: AccountUpdateAction,
    reason: Option<RiskTransitionSource>,
) -> Option<RuntimeEventKind> {
    Some(RuntimeEventKind::AccountUpdated {
        account_id: account.id(),
        action,
        reason,
        owner: account.owner(),
        pending_owner: account.pending_owner(),
        paused: account.paused(),
        nonce: account.nonce(),
        code_hash: account.code_hash(),
        controllers: account.controllers().to_vec(),
    })
}

pub(super) fn direct_intent_costs(
    kind: &IntentKind,
    markets: &BTreeMap<u64, PerpetualMarket>,
) -> (u128, u128, u128) {
    match kind {
        IntentKind::Transfer { amount, .. } => {
            let amount = u128::from(*amount);
            (10, amount, amount)
        }
        IntentKind::Cancel { .. } => (5, 0, 0),
        IntentKind::PlaceOrder {
            market_id,
            quantity_lots,
            kind,
            ..
        } => {
            let notional = match kind {
                IntentOrderKind::Limit { price_ticks } => {
                    order_notional_usdc(markets, *market_id, *price_ticks, *quantity_lots)
                }
                IntentOrderKind::Market => u128::from(*quantity_lots),
            };
            (12, notional, notional)
        }
        IntentKind::CancelOrder { .. } => (5, 0, 0),
        IntentKind::ReplaceOrder {
            market_id,
            quantity_lots,
            kind,
            ..
        } => {
            let notional = match kind {
                IntentOrderKind::Limit { price_ticks } => {
                    order_notional_usdc(markets, *market_id, *price_ticks, *quantity_lots)
                }
                IntentOrderKind::Market => u128::from(*quantity_lots),
            };
            (12, notional, notional)
        }
        IntentKind::CreateSubPolicy { .. } => (15, 0, 0),
        IntentKind::RevokePolicy { .. } => (5, 0, 0),
        IntentKind::DepositCollateral { amount_usdc, .. }
        | IntentKind::WithdrawCollateral { amount_usdc, .. } => {
            let amount = u128::from(*amount_usdc);
            (10, amount, amount)
        }
    }
}

pub(super) fn order_notional_usdc(
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_id: u64,
    price_ticks: u64,
    quantity_lots: u64,
) -> u128 {
    let Some(market) = markets.get(&market_id) else {
        if markets.is_empty() {
            return u128::from(price_ticks).saturating_mul(u128::from(quantity_lots));
        }
        return u128::MAX;
    };
    scaled_tick_lot_notional_usdc(market, price_ticks, quantity_lots)
}

pub(super) fn order_lifecycle_status(status: MatchStatus) -> OrderLifecycleStatus {
    match status {
        MatchStatus::Filled => OrderLifecycleStatus::Filled,
        MatchStatus::PartiallyFilledCancelled => OrderLifecycleStatus::PartiallyFilledCancelled,
        MatchStatus::PartiallyFilledResting => OrderLifecycleStatus::PartiallyFilledResting,
        MatchStatus::Cancelled => OrderLifecycleStatus::Cancelled,
        MatchStatus::Resting => OrderLifecycleStatus::Resting,
    }
}

pub(super) fn order_fill_events(
    market_id: u64,
    fills: &[MatchFill],
    taker_mandate_id: Option<MandateId>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
) -> Vec<RuntimeEventKind> {
    let mut events = Vec::with_capacity(fills.len().saturating_mul(2));
    for fill in fills {
        events.push(RuntimeEventKind::OrderFilled {
            market_id,
            account_id: fill.maker_account_id,
            client_order_id: fill.maker_client_order_id,
            counterparty_account_id: fill.taker_account_id,
            counterparty_client_order_id: fill.taker_client_order_id,
            mandate_id: resting_order_mandates
                .get(&resting_order_key(
                    market_id,
                    fill.maker_account_id,
                    fill.maker_client_order_id,
                ))
                .copied(),
            role: OrderFillRole::Maker,
            side: order_fill_side(fill.maker_side),
            price_ticks: fill.price_ticks,
            quantity_lots: fill.quantity_lots,
        });
        events.push(RuntimeEventKind::OrderFilled {
            market_id,
            account_id: fill.taker_account_id,
            client_order_id: fill.taker_client_order_id,
            counterparty_account_id: fill.maker_account_id,
            counterparty_client_order_id: fill.maker_client_order_id,
            mandate_id: taker_mandate_id,
            role: OrderFillRole::Taker,
            side: order_fill_side(fill.taker_side),
            price_ticks: fill.price_ticks,
            quantity_lots: fill.quantity_lots,
        });
    }
    events
}

pub(super) fn resting_order_key(
    market_id: u64,
    account_id: u64,
    client_order_id: u64,
) -> (u64, u64, u64) {
    (market_id, account_id, client_order_id)
}

pub(super) fn update_resting_order_mandates_from_outcome(
    resting_order_mandates: &mut BTreeMap<(u64, u64, u64), MandateId>,
    order_books: &BTreeMap<u64, SingleMarketBook>,
    market_id: u64,
    order: &OrderRequest,
    outcome: &MatchOutcome,
    taker_mandate_id: Option<MandateId>,
    timestamp_ms: u64,
) {
    for fill in &outcome.fills {
        let key = resting_order_key(market_id, fill.maker_account_id, fill.maker_client_order_id);
        let still_resting = order_books
            .get(&market_id)
            .and_then(|book| {
                book.resting_order(
                    fill.maker_account_id,
                    fill.maker_client_order_id,
                    timestamp_ms,
                )
            })
            .is_some();
        if !still_resting {
            resting_order_mandates.remove(&key);
        }
    }

    let taker_key = resting_order_key(market_id, order.account_id, order.client_order_id);
    if outcome.posted_order_id == Some(order.client_order_id) {
        if let Some(mandate_id) = taker_mandate_id {
            resting_order_mandates.insert(taker_key, mandate_id);
        } else {
            resting_order_mandates.remove(&taker_key);
        }
    } else {
        resting_order_mandates.remove(&taker_key);
    }
}

pub(super) fn order_fill_side(side: MatchingOrderSide) -> OrderFillSide {
    match side {
        MatchingOrderSide::Bid => OrderFillSide::Buy,
        MatchingOrderSide::Ask => OrderFillSide::Sell,
    }
}

pub(super) fn restore_order_book(
    order_books: &mut BTreeMap<u64, SingleMarketBook>,
    market_id: u64,
    prior_book: Option<SingleMarketBook>,
) {
    match prior_book {
        Some(book) => {
            order_books.insert(market_id, book);
        }
        None => {
            order_books.remove(&market_id);
        }
    }
}

pub(super) fn intent_origin_mandate_id(
    policy_store: &PolicyStore,
    signed_intent: &SignedIntent,
) -> Option<MandateId> {
    policy_store
        .get(signed_intent.intent.policy_id)
        .and_then(|policy| match &policy.origin {
            PolicyOrigin::MandateGenerated { mandate_id, .. } => Some(*mandate_id),
            PolicyOrigin::Direct => None,
        })
}

pub(super) fn mandate_fill_rollback_events(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    account_operators: &BTreeMap<u64, u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    market_id: u64,
    signed_intent: &SignedIntent,
    fills: &[MatchFill],
    timestamp_ms: u64,
) -> Vec<RuntimeEventKind> {
    if fills.is_empty() {
        return Vec::new();
    }
    let mut checked_mandates = BTreeSet::new();

    if let Some(mandate_id) = intent_origin_mandate_id(policy_store, signed_intent) {
        if let Some(event) = mandate_fill_rollback_event_for_account(
            mandates,
            account_operators,
            markets,
            market_marks,
            isolated_collateral,
            positions,
            market_id,
            mandate_id,
            signed_intent.intent.account,
            fills,
            timestamp_ms,
        ) {
            return vec![event];
        }
        checked_mandates.insert(mandate_id);
    }

    for fill in fills {
        let Some(mandate_id) = resting_order_mandates
            .get(&resting_order_key(
                market_id,
                fill.maker_account_id,
                fill.maker_client_order_id,
            ))
            .copied()
        else {
            continue;
        };
        if !checked_mandates.insert(mandate_id) {
            continue;
        }
        if let Some(event) = mandate_fill_rollback_event_for_account(
            mandates,
            account_operators,
            markets,
            market_marks,
            isolated_collateral,
            positions,
            market_id,
            mandate_id,
            fill.maker_account_id,
            fills,
            timestamp_ms,
        ) {
            return vec![event];
        }
    }

    Vec::new()
}

pub(super) fn mandate_fill_rollback_event_for_account(
    mandates: &BTreeMap<MandateId, Mandate>,
    account_operators: &BTreeMap<u64, u64>,
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    market_id: u64,
    mandate_id: MandateId,
    account_id: u64,
    fills: &[MatchFill],
    timestamp_ms: u64,
) -> Option<RuntimeEventKind> {
    let mandate = mandates.get(&mandate_id)?;
    if mandate.state != MandateState::Active {
        return None;
    }

    for (index, constraint) in mandate.constraints.iter().enumerate() {
        match constraint {
            MandateConstraint::MaxTradeSizeUsdc { limit_usdc } => {
                let notional =
                    match_fill_notional_usdc_for_account(markets, market_id, fills, account_id);
                if mandate_u256_to_u128(*limit_usdc).is_ok_and(|limit| notional > limit) {
                    return Some(RuntimeEventKind::MandateConstraintTriggered {
                        mandate_id,
                        agent: mandate.agent,
                        constraint_index: u16::try_from(index).unwrap_or(u16::MAX),
                        action: MandateEnforcementAction::FillRolledBack,
                        triggered_at_ms: timestamp_ms,
                    });
                }
            }
            MandateConstraint::MaxLeverage { mult } => {
                if mandate_fill_exceeds_max_leverage(
                    markets,
                    market_marks,
                    isolated_collateral,
                    positions,
                    market_id,
                    account_id,
                    fills,
                    mandate_id,
                    *mult,
                    timestamp_ms,
                ) {
                    return Some(RuntimeEventKind::MandateConstraintTriggered {
                        mandate_id,
                        agent: mandate.agent,
                        constraint_index: u16::try_from(index).unwrap_or(u16::MAX),
                        action: MandateEnforcementAction::FillRolledBack,
                        triggered_at_ms: timestamp_ms,
                    });
                }
            }
            MandateConstraint::AllowedCounterparties { accounts } => {
                if fills.iter().any(|fill| {
                    fill_counterparty_for_account(fill, account_id)
                        .is_some_and(|counterparty| !accounts.contains(&counterparty))
                }) {
                    return Some(RuntimeEventKind::MandateConstraintTriggered {
                        mandate_id,
                        agent: mandate.agent,
                        constraint_index: u16::try_from(index).unwrap_or(u16::MAX),
                        action: MandateEnforcementAction::FillRolledBack,
                        triggered_at_ms: timestamp_ms,
                    });
                }
            }
            MandateConstraint::NoSameOperatorCounterparty => {
                if fills.iter().any(|fill| {
                    fill_counterparty_for_account(fill, account_id).is_some_and(|counterparty| {
                        same_nonzero_operator(account_operators, account_id, counterparty)
                    })
                }) {
                    return Some(RuntimeEventKind::MandateConstraintTriggered {
                        mandate_id,
                        agent: mandate.agent,
                        constraint_index: u16::try_from(index).unwrap_or(u16::MAX),
                        action: MandateEnforcementAction::FillRolledBack,
                        triggered_at_ms: timestamp_ms,
                    });
                }
            }
            _ => {}
        }
    }

    None
}

pub(super) fn same_nonzero_operator(
    account_operators: &BTreeMap<u64, u64>,
    first_account: u64,
    second_account: u64,
) -> bool {
    let Some(first_operator) = account_operators.get(&first_account).copied() else {
        return false;
    };
    let Some(second_operator) = account_operators.get(&second_account).copied() else {
        return false;
    };
    first_operator != 0 && first_operator == second_operator
}

pub(super) fn match_fill_notional_usdc_for_account(
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_id: u64,
    fills: &[MatchFill],
    account_id: u64,
) -> u128 {
    let Some(market) = markets.get(&market_id) else {
        return u128::MAX;
    };
    fills.iter().fold(0_u128, |total, fill| {
        if fill.maker_account_id == account_id || fill.taker_account_id == account_id {
            total.saturating_add(scaled_fill_notional_usdc(market, fill))
        } else {
            total
        }
    })
}

pub(super) fn scaled_fill_notional_usdc(market: &PerpetualMarket, fill: &MatchFill) -> u128 {
    scaled_tick_lot_notional_usdc(market, fill.price_ticks, fill.quantity_lots)
}

pub(super) fn scaled_tick_lot_notional_usdc(
    market: &PerpetualMarket,
    price_ticks: u64,
    quantity_lots: u64,
) -> u128 {
    let raw_tick_lots = u128::from(price_ticks).saturating_mul(u128::from(quantity_lots));
    scaled_raw_tick_lot_notional_usdc(market, raw_tick_lots)
}

pub(super) fn scaled_raw_tick_lot_notional_usdc(
    market: &PerpetualMarket,
    raw_tick_lots: u128,
) -> u128 {
    let scaled = raw_tick_lots
        .saturating_mul(u128::from(market.config.tick_size_bps))
        .saturating_mul(u128::from(market.config.lot_size_microunits));
    let denominator = BPS_DENOMINATOR.saturating_mul(1_000_000);
    if scaled == 0 {
        0
    } else {
        scaled.saturating_sub(1) / denominator + 1
    }
}

pub(super) fn scaled_raw_tick_lot_notional_usdc_floor(
    market: &PerpetualMarket,
    raw_tick_lots: u128,
) -> u128 {
    raw_tick_lots
        .saturating_mul(u128::from(market.config.tick_size_bps))
        .saturating_mul(u128::from(market.config.lot_size_microunits))
        / BPS_DENOMINATOR.saturating_mul(1_000_000)
}

pub(super) fn scaled_signed_raw_tick_lot_pnl_usdc(market: &PerpetualMarket, raw_pnl: i128) -> i128 {
    let magnitude = raw_pnl.unsigned_abs();
    if raw_pnl < 0 {
        -u128_to_i128_saturating(scaled_raw_tick_lot_notional_usdc(market, magnitude))
    } else {
        u128_to_i128_saturating(scaled_raw_tick_lot_notional_usdc_floor(market, magnitude))
    }
}

pub(super) fn scaled_ledger_unrealized_pnl_usdc(
    market: &PerpetualMarket,
    ledger: &PositionLedger,
    mark_price_ticks: Option<u64>,
) -> i128 {
    scaled_signed_raw_tick_lot_pnl_usdc(market, unrealized_pnl(ledger, mark_price_ticks))
}

pub(super) fn scaled_ledger_equity_usdc(
    market: &PerpetualMarket,
    collateral: i128,
    ledger: &PositionLedger,
    mark_price_ticks: Option<u64>,
) -> i128 {
    collateral
        .saturating_add(scaled_signed_raw_tick_lot_pnl_usdc(
            market,
            ledger.realized_pnl,
        ))
        .saturating_add(scaled_ledger_unrealized_pnl_usdc(
            market,
            ledger,
            mark_price_ticks,
        ))
}

pub(super) fn utc_day_bucket_ms(timestamp_ms: u64) -> u64 {
    timestamp_ms / UTC_DAY_MS * UTC_DAY_MS
}

pub(super) fn liquidation_gap_for_equity_sample(
    equity: i128,
    maintenance_requirement: u128,
) -> u128 {
    i128::try_from(maintenance_requirement)
        .unwrap_or(i128::MAX)
        .saturating_sub(equity)
        .max(0) as u128
}

pub(super) fn i128_abs_to_u128_saturating(value: i128) -> u128 {
    value.unsigned_abs()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn mandate_fill_exceeds_max_leverage(
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    isolated_collateral: &BTreeMap<(u64, u64), i128>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    market_id: u64,
    account_id: u64,
    fills: &[MatchFill],
    mandate_id: MandateId,
    max_leverage: u32,
    timestamp_ms: u64,
) -> bool {
    let current = positions
        .get(&(market_id, account_id))
        .copied()
        .unwrap_or_default();
    let mut candidate = current;
    for fill in fills {
        let side = if fill.maker_account_id == account_id {
            Some(fill.maker_side)
        } else if fill.taker_account_id == account_id {
            Some(fill.taker_side)
        } else {
            None
        };
        let Some(side) = side else {
            continue;
        };
        candidate.apply_fill(
            order_fill_side(side),
            fill.price_ticks,
            fill.quantity_lots,
            Some(mandate_id),
            candidate.funding_index,
            0,
            timestamp_ms,
        );
    }

    let current_notional = position_mark_notional_usdc(markets, market_marks, market_id, &current);
    let candidate_notional =
        position_mark_notional_usdc(markets, market_marks, market_id, &candidate);
    if candidate_notional <= current_notional {
        return false;
    }

    let Some(mark_price) = market_marks.get(&market_id).map(|price| price.price_ticks) else {
        return candidate_notional > 0;
    };
    let collateral = isolated_collateral
        .get(&(market_id, account_id))
        .copied()
        .unwrap_or(0);
    let Some(market) = markets.get(&market_id) else {
        return true;
    };
    let equity = scaled_ledger_equity_usdc(market, collateral, &candidate, Some(mark_price));
    leverage_exceeded(candidate_notional, equity, max_leverage)
}

pub(super) fn position_mark_notional_usdc(
    markets: &BTreeMap<u64, PerpetualMarket>,
    market_marks: &BTreeMap<u64, OraclePrice>,
    market_id: u64,
    ledger: &PositionLedger,
) -> u128 {
    let Some(market) = markets.get(&market_id) else {
        return u128::MAX;
    };
    let raw_tick_lots = match market_marks.get(&market_id) {
        Some(mark) => {
            u128::from(mark.price_ticks).saturating_mul(u128::from(ledger.open_abs_lots()))
        }
        None => ledger.open_notional,
    };
    scaled_raw_tick_lot_notional_usdc(market, raw_tick_lots)
}

pub(super) fn leverage_exceeded(notional_usdc: u128, equity: i128, max_leverage: u32) -> bool {
    if notional_usdc == 0 {
        return false;
    }
    if equity <= 0 {
        return true;
    }
    notional_usdc > (equity as u128).saturating_mul(u128::from(max_leverage))
}

pub(super) fn mandate_position_attribution_rollback_events(
    policy_store: &PolicyStore,
    mandates: &BTreeMap<MandateId, Mandate>,
    resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    signed_intent: &SignedIntent,
    market_id: u64,
    fills: &[MatchFill],
    timestamp_ms: u64,
) -> Vec<RuntimeEventKind> {
    if fills.is_empty() {
        return Vec::new();
    }
    if let Some(mandate_id) = intent_origin_mandate_id(policy_store, signed_intent) {
        if let Some(event) = mandate_position_attribution_rollback_event_for_account(
            mandates,
            positions,
            mandate_id,
            signed_intent.intent.account,
            market_id,
            fills,
            OrderFillRole::Taker,
            timestamp_ms,
        ) {
            return vec![event];
        }
    }

    let mut checked_makers = BTreeSet::new();
    for fill in fills {
        let Some(mandate_id) = resting_order_mandates
            .get(&resting_order_key(
                market_id,
                fill.maker_account_id,
                fill.maker_client_order_id,
            ))
            .copied()
        else {
            continue;
        };
        if !checked_makers.insert((mandate_id, fill.maker_account_id)) {
            continue;
        }
        if let Some(event) = mandate_position_attribution_rollback_event_for_account(
            mandates,
            positions,
            mandate_id,
            fill.maker_account_id,
            market_id,
            fills,
            OrderFillRole::Maker,
            timestamp_ms,
        ) {
            return vec![event];
        }
    }

    Vec::new()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn mandate_position_attribution_rollback_event_for_account(
    mandates: &BTreeMap<MandateId, Mandate>,
    positions: &BTreeMap<(u64, u64), PositionLedger>,
    mandate_id: MandateId,
    account_id: u64,
    market_id: u64,
    fills: &[MatchFill],
    role: OrderFillRole,
    timestamp_ms: u64,
) -> Option<RuntimeEventKind> {
    let mandate = mandates.get(&mandate_id)?;
    if mandate.state != MandateState::Active {
        return None;
    }
    let existing = positions.get(&(market_id, account_id))?;
    if existing.open_abs_lots() == 0 || existing.mandate_id == Some(mandate_id) {
        return None;
    }

    let mut candidate = *existing;
    for fill in fills {
        let side = match role {
            OrderFillRole::Maker if fill.maker_account_id == account_id => fill.maker_side,
            OrderFillRole::Taker if fill.taker_account_id == account_id => fill.taker_side,
            _ => continue,
        };
        candidate.apply_fill(
            order_fill_side(side),
            fill.price_ticks,
            fill.quantity_lots,
            Some(mandate_id),
            candidate.funding_index,
            0,
            timestamp_ms,
        );
    }
    if candidate.open_abs_lots() == 0 {
        return None;
    }

    Some(RuntimeEventKind::MandateConstraintTriggered {
        mandate_id,
        agent: mandate.agent,
        constraint_index: u16::MAX,
        action: MandateEnforcementAction::FillRolledBack,
        triggered_at_ms: timestamp_ms,
    })
}

pub(super) fn fill_counterparty_for_account(fill: &MatchFill, account_id: u64) -> Option<u64> {
    if fill.maker_account_id == account_id {
        Some(fill.taker_account_id)
    } else if fill.taker_account_id == account_id {
        Some(fill.maker_account_id)
    } else {
        None
    }
}

pub(super) fn mahi_runtime_event_root(events: &[RuntimeEventKind]) -> Hash {
    let bytes = borsh::to_vec(events).expect("runtime events must serialize");
    Sha256::digest(bytes).into()
}

pub(super) fn mahi_native_state_root(
    parent_state_root: &Hash,
    tx_root: &Hash,
    event_root: &Hash,
    round: u64,
    timestamp_ms: u64,
) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(parent_state_root);
    hasher.update(tx_root);
    hasher.update(event_root);
    hasher.update(round.to_le_bytes());
    hasher.update(timestamp_ms.to_le_bytes());
    hasher.finalize().into()
}

pub(super) fn tee_governance_block_error_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::InvalidTeeGovernance { reason, .. } => reason,
        _ => "internal_error",
    }
}

pub(super) fn real_capital_governance_block_error_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::InvalidRealCapitalGovernance { reason, .. }
        | NodeError::InvalidDelegation { reason, .. } => reason,
        _ => "internal_error",
    }
}

pub(super) fn market_maker_program_block_error_reason(error: &NodeError) -> &'static str {
    delegation_block_error_reason(error)
}

pub(super) fn proportional_bps_round_up(notional: u128, bps: u32) -> u128 {
    if notional == 0 || bps == 0 {
        return 0;
    }

    notional
        .saturating_mul(u128::from(bps))
        .saturating_add(BPS_DENOMINATOR.saturating_sub(1))
        / BPS_DENOMINATOR
}

pub(super) fn proportional_bps_floor(notional: u128, bps: u128) -> u128 {
    if notional == 0 || bps == 0 {
        return 0;
    }

    notional.saturating_mul(bps) / BPS_DENOMINATOR
}

pub(super) fn u128_to_i128_saturating(value: u128) -> i128 {
    value.min(i128::MAX as u128) as i128
}

pub(super) fn match_fill_notional(fills: &[MatchFill]) -> u128 {
    fills.iter().fold(0_u128, |acc, fill| {
        acc.saturating_add(u128::from(fill.price_ticks) * u128::from(fill.quantity_lots))
    })
}
