use super::*;

impl ProposerReadyFrontier {
    pub(crate) fn on_intent_admitted(
        &mut self,
        mempool: &DirectIntentMempool,
        policy_store: &PolicyStore,
        policy_id: u64,
        nonce: u64,
    ) {
        let expected_nonce = policy_store.next_intent_nonce(policy_id).unwrap_or(1);
        let mut remove_policy = false;
        {
            let frontier = self.contiguous_by_policy.entry(policy_id).or_default();
            if frontier
                .first()
                .is_some_and(|first| first.nonce != expected_nonce)
            {
                frontier.clear();
            }

            if frontier.is_empty() {
                if nonce != expected_nonce {
                    remove_policy = true;
                } else {
                    frontier
                        .extend(mempool.contiguous_policy_frontier_keys(policy_id, expected_nonce));
                    remove_policy = frontier.is_empty();
                }
            } else {
                let last_nonce = frontier
                    .last()
                    .map(|key| key.nonce)
                    .unwrap_or(expected_nonce);
                if nonce == last_nonce.saturating_add(1) {
                    frontier.extend(mempool.contiguous_policy_frontier_keys(policy_id, nonce));
                }
            }
        }

        if remove_policy {
            self.contiguous_by_policy.remove(&policy_id);
        }
    }

    pub(crate) fn rebuild_all(
        &mut self,
        mempool: &DirectIntentMempool,
        policy_store: &PolicyStore,
    ) {
        self.contiguous_by_policy.clear();
        for policy_id in mempool.policy_ids() {
            self.rebuild_policy(mempool, policy_store, policy_id);
        }
    }

    fn rebuild_policy(
        &mut self,
        mempool: &DirectIntentMempool,
        policy_store: &PolicyStore,
        policy_id: u64,
    ) {
        let expected_nonce = policy_store.next_intent_nonce(policy_id).unwrap_or(1);
        let keys = mempool.contiguous_policy_frontier_keys(policy_id, expected_nonce);
        if keys.is_empty() {
            self.contiguous_by_policy.remove(&policy_id);
        } else {
            self.contiguous_by_policy.insert(policy_id, keys);
        }
    }

    pub(crate) fn refresh_for_block(
        &mut self,
        mempool: &DirectIntentMempool,
        policy_store: &PolicyStore,
        body: &BlockBody,
    ) {
        let mut policies = BTreeSet::new();
        for item in &body.items {
            if let BlockItem::Intent(signed_intent) = item {
                policies.insert(signed_intent.intent.policy_id);
            }
        }
        for policy_id in policies {
            self.rebuild_policy(mempool, policy_store, policy_id);
        }
    }

    pub(crate) fn refresh_for_rejected(
        &mut self,
        mempool: &DirectIntentMempool,
        policy_store: &PolicyStore,
        rejected: &[RejectedIntent],
    ) {
        let mut policies = BTreeSet::new();
        for rejected_intent in rejected {
            policies.insert(rejected_intent.signed_intent.intent.policy_id);
        }
        for policy_id in policies {
            self.rebuild_policy(mempool, policy_store, policy_id);
        }
    }

    pub(crate) fn refresh_for_signed_intents<'a, I>(
        &mut self,
        mempool: &DirectIntentMempool,
        policy_store: &PolicyStore,
        intents: I,
    ) where
        I: IntoIterator<Item = &'a SignedIntent>,
    {
        let mut policies = BTreeSet::new();
        for signed_intent in intents {
            policies.insert(signed_intent.intent.policy_id);
        }
        for policy_id in policies {
            self.rebuild_policy(mempool, policy_store, policy_id);
        }
    }
}

impl DirectIntentMempool {
    pub(super) fn policy_ids(&self) -> BTreeSet<u64> {
        self.intents_by_order
            .keys()
            .map(|key| key.policy_id)
            .collect()
    }

    pub(super) fn contiguous_policy_frontier_keys(
        &self,
        policy_id: u64,
        expected_nonce: u64,
    ) -> Vec<IntentOrderKey> {
        let mut frontier = Vec::new();
        let mut next_nonce = expected_nonce;
        let start = IntentOrderKey {
            policy_id,
            nonce: 0,
            intent_id: [0; 32],
        };

        for (key, _) in self.intents_by_order.range(start..) {
            if key.policy_id != policy_id {
                break;
            }
            if key.nonce < next_nonce {
                continue;
            }
            if key.nonce > next_nonce {
                break;
            }
            frontier.push(*key);
            next_nonce = next_nonce.saturating_add(1);
        }

        frontier
    }

    fn get_by_order_key(&self, key: &IntentOrderKey) -> Option<&SignedIntent> {
        self.intents_by_order.get(key)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn plan_block_body_from_frontier(
        &self,
        policy_store: &PolicyStore,
        mandates: &BTreeMap<MandateId, Mandate>,
        delegations: &BTreeMap<DelegationId, Delegation>,
        account_operators: &BTreeMap<u64, u64>,
        order_books: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
        frozen_accounts: &BTreeSet<u64>,
        markets: &BTreeMap<u64, PerpetualMarket>,
        paused_markets: &BTreeSet<u64>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        market_oracle_config: &BTreeMap<u64, OracleConfig>,
        isolated_collateral: &BTreeMap<(u64, u64), i128>,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        chain_id: u64,
        block_number: u64,
        timestamp_ms: u64,
        max_items: usize,
        frontier: &ProposerReadyFrontier,
    ) -> BlockAssemblyPlan {
        self.plan_block_body_from_frontier_filtered(
            policy_store,
            mandates,
            delegations,
            account_operators,
            order_books,
            resting_order_mandates,
            frozen_accounts,
            markets,
            paused_markets,
            market_marks,
            market_index_prices,
            market_oracle_config,
            isolated_collateral,
            positions,
            chain_id,
            block_number,
            timestamp_ms,
            max_items,
            frontier,
            |_| true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn select_mahi_block_body_from_frontier(
        &self,
        policy_store: &PolicyStore,
        timestamp_ms: u64,
        max_items: usize,
        frontier: &ProposerReadyFrontier,
        shard_index: usize,
        shard_count: usize,
        inflight_policy_nonces: &BTreeMap<u64, BTreeSet<u64>>,
    ) -> BlockBody {
        let shard_index = shard_index as u64;
        let shard_count = shard_count.max(1) as u64;
        let mut items = Vec::new();

        for (&policy_id, frontier_keys) in &frontier.contiguous_by_policy {
            if items.len() >= max_items {
                break;
            }
            if policy_id % shard_count != shard_index {
                continue;
            }

            let mut expected_nonce =
                mahi_effective_next_nonce(policy_store, policy_id, inflight_policy_nonces);
            for key in frontier_keys {
                if items.len() >= max_items {
                    break;
                }
                if key.nonce < expected_nonce {
                    continue;
                }
                if key.nonce > expected_nonce {
                    break;
                }

                let Some(signed_intent) = self.get_by_order_key(key).cloned() else {
                    break;
                };
                if intent_expires_at_ms(&signed_intent.intent) <= timestamp_ms {
                    continue;
                }
                items.push(BlockItem::Intent(signed_intent));
                expected_nonce = expected_nonce.saturating_add(1);
            }
        }

        BlockBody { items }
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_block_body_from_frontier_filtered<F>(
        &self,
        policy_store: &PolicyStore,
        mandates: &BTreeMap<MandateId, Mandate>,
        delegations: &BTreeMap<DelegationId, Delegation>,
        account_operators: &BTreeMap<u64, u64>,
        order_books: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
        frozen_accounts: &BTreeSet<u64>,
        markets: &BTreeMap<u64, PerpetualMarket>,
        paused_markets: &BTreeSet<u64>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        market_oracle_config: &BTreeMap<u64, OracleConfig>,
        isolated_collateral: &BTreeMap<(u64, u64), i128>,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        chain_id: u64,
        block_number: u64,
        timestamp_ms: u64,
        max_items: usize,
        frontier: &ProposerReadyFrontier,
        policy_filter: F,
    ) -> BlockAssemblyPlan
    where
        F: Fn(u64) -> bool + Copy,
    {
        if frontier.contiguous_by_policy.is_empty() {
            return self.plan_block_body_filtered(
                policy_store,
                mandates,
                delegations,
                account_operators,
                order_books,
                resting_order_mandates,
                frozen_accounts,
                markets,
                paused_markets,
                market_marks,
                market_index_prices,
                market_oracle_config,
                isolated_collateral,
                positions,
                chain_id,
                block_number,
                timestamp_ms,
                max_items,
                policy_filter,
            );
        }

        let mut simulated_policy_store = policy_store.clone();
        let mut simulated_order_books = BTreeMap::new();
        let mut simulated_resting_order_mandates = resting_order_mandates.clone();
        let mut simulated_isolated_collateral = BTreeMap::new();
        let mut items = Vec::new();
        let mut rejected = Vec::new();
        let mut committed_events = Vec::new();

        for (&policy_id, frontier_keys) in &frontier.contiguous_by_policy {
            if items.len() >= max_items {
                break;
            }
            if !policy_filter(policy_id) {
                continue;
            }

            let mut expected_nonce = policy_store.next_intent_nonce(policy_id).unwrap_or(1);
            let mut burned_policy = false;

            for key in frontier_keys {
                if items.len() >= max_items {
                    break;
                }

                let Some(signed_intent) = self.get_by_order_key(key).cloned() else {
                    break;
                };

                if burned_policy {
                    rejected.push(RejectedIntent {
                        signed_intent,
                        reason: IntentRejectionReason::PredecessorIntentRejected,
                        order_reason: None,
                    });
                    continue;
                }

                if intent_expires_at_ms(&signed_intent.intent) <= timestamp_ms {
                    continue;
                }

                if key.nonce < expected_nonce {
                    continue;
                }

                if key.nonce > expected_nonce {
                    break;
                }

                let mut next_policy_store = simulated_policy_store.clone();
                let mut next_order_books = simulated_order_books.clone();
                let mut next_resting_order_mandates = simulated_resting_order_mandates.clone();
                let mut next_isolated_collateral = simulated_isolated_collateral.clone();
                let intent_events = match simulate_intent_on_runtime_state(
                    &mut next_policy_store,
                    mandates,
                    delegations,
                    account_operators,
                    simulation_order_books_for_intent(
                        &mut next_order_books,
                        order_books,
                        &signed_intent,
                    ),
                    &mut next_resting_order_mandates,
                    simulation_collateral_for_intent(
                        &mut next_isolated_collateral,
                        isolated_collateral,
                        &signed_intent,
                    ),
                    frozen_accounts,
                    markets,
                    paused_markets,
                    market_marks,
                    market_index_prices,
                    market_oracle_config,
                    positions,
                    &signed_intent,
                    chain_id,
                    block_number,
                    timestamp_ms,
                    PolicyAdmissionCommitMode::VerifySession,
                ) {
                    Ok(events) => {
                        simulated_policy_store = next_policy_store;
                        simulated_order_books = next_order_books;
                        simulated_resting_order_mandates = next_resting_order_mandates;
                        simulated_isolated_collateral = next_isolated_collateral;
                        events
                    }
                    Err(error) => {
                        if transient_intent_blocker(&error) {
                            continue;
                        }
                        let (reason, order_reason) = rejection_context_from_node_error(&error);
                        rejected.push(RejectedIntent {
                            signed_intent,
                            reason,
                            order_reason,
                        });
                        burned_policy = true;
                        continue;
                    }
                };

                items.push(BlockItem::Intent(signed_intent));
                committed_events.push(RuntimeEventKind::IntentExecuted {
                    intent_id: key.intent_id,
                    policy_id,
                    nonce: expected_nonce,
                });
                committed_events.extend(intent_events);
                expected_nonce = expected_nonce.saturating_add(1);
            }
        }

        BlockAssemblyPlan {
            body: BlockBody { items },
            rejected,
            policy_store_after: simulated_policy_store,
            order_book_overlays: simulated_order_books,
            resting_order_mandate_overlays: simulated_resting_order_mandates,
            collateral_overlays: simulated_isolated_collateral,
            committed_events,
        }
    }

    pub fn len(&self) -> usize {
        self.intents_by_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.intents_by_order.is_empty()
    }

    pub fn submit(
        &mut self,
        signed_intent: SignedIntent,
        intent_id: IntentId,
    ) -> Result<(), NodeError> {
        let order_key = IntentOrderKey {
            policy_id: signed_intent.intent.policy_id,
            nonce: signed_intent.intent.nonce,
            intent_id,
        };

        self.known_intents.insert(intent_id);
        self.intents_by_order.insert(order_key, signed_intent);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_block_body(
        &self,
        policy_store: &PolicyStore,
        mandates: &BTreeMap<MandateId, Mandate>,
        delegations: &BTreeMap<DelegationId, Delegation>,
        account_operators: &BTreeMap<u64, u64>,
        order_books: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
        frozen_accounts: &BTreeSet<u64>,
        markets: &BTreeMap<u64, PerpetualMarket>,
        paused_markets: &BTreeSet<u64>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        market_oracle_config: &BTreeMap<u64, OracleConfig>,
        isolated_collateral: &BTreeMap<(u64, u64), i128>,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        chain_id: u64,
        block_number: u64,
        timestamp_ms: u64,
        max_items: usize,
    ) -> BlockAssemblyPlan {
        self.plan_block_body_filtered(
            policy_store,
            mandates,
            delegations,
            account_operators,
            order_books,
            resting_order_mandates,
            frozen_accounts,
            markets,
            paused_markets,
            market_marks,
            market_index_prices,
            market_oracle_config,
            isolated_collateral,
            positions,
            chain_id,
            block_number,
            timestamp_ms,
            max_items,
            |_| true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_block_body_filtered<F>(
        &self,
        policy_store: &PolicyStore,
        mandates: &BTreeMap<MandateId, Mandate>,
        delegations: &BTreeMap<DelegationId, Delegation>,
        account_operators: &BTreeMap<u64, u64>,
        order_books: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandates: &BTreeMap<(u64, u64, u64), MandateId>,
        frozen_accounts: &BTreeSet<u64>,
        markets: &BTreeMap<u64, PerpetualMarket>,
        paused_markets: &BTreeSet<u64>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        market_oracle_config: &BTreeMap<u64, OracleConfig>,
        isolated_collateral: &BTreeMap<(u64, u64), i128>,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        chain_id: u64,
        block_number: u64,
        timestamp_ms: u64,
        max_items: usize,
        policy_filter: F,
    ) -> BlockAssemblyPlan
    where
        F: Fn(u64) -> bool,
    {
        let mut simulated_policy_store = policy_store.clone();
        let mut simulated_order_books = BTreeMap::new();
        let mut simulated_resting_order_mandates = resting_order_mandates.clone();
        let mut simulated_isolated_collateral = BTreeMap::new();
        let mut items = Vec::new();
        let mut rejected = Vec::new();
        let mut committed_events = Vec::new();
        let mut cursor = self.intents_by_order.iter().peekable();

        while let Some((&first_key, _)) = cursor.peek() {
            if items.len() >= max_items {
                break;
            }

            let policy_id = first_key.policy_id;
            if !policy_filter(policy_id) {
                while let Some((&key, _)) = cursor.peek() {
                    if key.policy_id != policy_id {
                        break;
                    }
                    cursor.next();
                }
                continue;
            }
            let mut expected_nonce = policy_store.next_intent_nonce(policy_id).unwrap_or(1);
            let mut burned_policy = false;

            while let Some((&key, signed_intent)) = cursor.peek() {
                if key.policy_id != policy_id || items.len() >= max_items {
                    break;
                }

                let signed_intent = (*signed_intent).clone();
                cursor.next();

                if burned_policy {
                    rejected.push(RejectedIntent {
                        signed_intent,
                        reason: IntentRejectionReason::PredecessorIntentRejected,
                        order_reason: None,
                    });
                    continue;
                }

                if intent_expires_at_ms(&signed_intent.intent) <= timestamp_ms {
                    continue;
                }

                if key.nonce < expected_nonce {
                    continue;
                }

                if key.nonce > expected_nonce {
                    break;
                }

                let mut next_policy_store = simulated_policy_store.clone();
                let mut next_order_books = simulated_order_books.clone();
                let mut next_resting_order_mandates = simulated_resting_order_mandates.clone();
                let mut next_isolated_collateral = simulated_isolated_collateral.clone();
                let intent_events = match simulate_intent_on_runtime_state(
                    &mut next_policy_store,
                    mandates,
                    delegations,
                    account_operators,
                    simulation_order_books_for_intent(
                        &mut next_order_books,
                        order_books,
                        &signed_intent,
                    ),
                    &mut next_resting_order_mandates,
                    simulation_collateral_for_intent(
                        &mut next_isolated_collateral,
                        isolated_collateral,
                        &signed_intent,
                    ),
                    frozen_accounts,
                    markets,
                    paused_markets,
                    market_marks,
                    market_index_prices,
                    market_oracle_config,
                    positions,
                    &signed_intent,
                    chain_id,
                    block_number,
                    timestamp_ms,
                    PolicyAdmissionCommitMode::VerifySession,
                ) {
                    Ok(events) => {
                        simulated_policy_store = next_policy_store;
                        simulated_order_books = next_order_books;
                        simulated_resting_order_mandates = next_resting_order_mandates;
                        simulated_isolated_collateral = next_isolated_collateral;
                        events
                    }
                    Err(error) => {
                        if transient_intent_blocker(&error) {
                            continue;
                        }
                        let (reason, order_reason) = rejection_context_from_node_error(&error);
                        rejected.push(RejectedIntent {
                            signed_intent,
                            reason,
                            order_reason,
                        });
                        burned_policy = true;
                        continue;
                    }
                };

                items.push(BlockItem::Intent(signed_intent));
                committed_events.push(RuntimeEventKind::IntentExecuted {
                    intent_id: key.intent_id,
                    policy_id,
                    nonce: expected_nonce,
                });
                committed_events.extend(intent_events);
                expected_nonce = expected_nonce.saturating_add(1);
            }
        }

        BlockAssemblyPlan {
            body: BlockBody { items },
            rejected,
            policy_store_after: simulated_policy_store,
            order_book_overlays: simulated_order_books,
            resting_order_mandate_overlays: simulated_resting_order_mandates,
            collateral_overlays: simulated_isolated_collateral,
            committed_events,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_block_body(
        &self,
        policy_store: &PolicyStore,
        order_books: &BTreeMap<u64, SingleMarketBook>,
        frozen_accounts: &BTreeSet<u64>,
        markets: &BTreeMap<u64, PerpetualMarket>,
        paused_markets: &BTreeSet<u64>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        market_oracle_config: &BTreeMap<u64, OracleConfig>,
        chain_id: u64,
        block_number: u64,
        timestamp_ms: u64,
        max_items: usize,
    ) -> BlockBody {
        let mandates = BTreeMap::new();
        let delegations = BTreeMap::new();
        let account_operators = BTreeMap::new();
        let resting_order_mandates = BTreeMap::new();
        let isolated_collateral = BTreeMap::new();
        let positions = BTreeMap::new();
        self.plan_block_body(
            policy_store,
            &mandates,
            &delegations,
            &account_operators,
            order_books,
            &resting_order_mandates,
            frozen_accounts,
            markets,
            paused_markets,
            market_marks,
            market_index_prices,
            market_oracle_config,
            &isolated_collateral,
            &positions,
            chain_id,
            block_number,
            timestamp_ms,
            max_items,
        )
        .body
    }

    pub fn remove_block(&mut self, body: &BlockBody) {
        for item in &body.items {
            if let BlockItem::Intent(signed_intent) = item {
                let id = intent_id(signed_intent);
                self.known_intents.remove(&id);
                self.intents_by_order.remove(&IntentOrderKey {
                    policy_id: signed_intent.intent.policy_id,
                    nonce: signed_intent.intent.nonce,
                    intent_id: id,
                });
            }
        }
    }

    pub(super) fn remove_rejected(&mut self, rejected: &[RejectedIntent]) {
        for rejected_intent in rejected {
            let signed_intent = &rejected_intent.signed_intent;
            let id = intent_id(signed_intent);
            self.known_intents.remove(&id);
            self.intents_by_order.remove(&IntentOrderKey {
                policy_id: signed_intent.intent.policy_id,
                nonce: signed_intent.intent.nonce,
                intent_id: id,
            });
        }
    }

    pub fn purge_expired(&mut self, timestamp_ms: u64) -> Vec<SignedIntent> {
        let expired: Vec<_> = self
            .intents_by_order
            .iter()
            .filter_map(|(key, signed_intent)| {
                (intent_expires_at_ms(&signed_intent.intent) <= timestamp_ms).then_some((
                    *key,
                    intent_id(signed_intent),
                    signed_intent.clone(),
                ))
            })
            .collect();
        let mut removed = Vec::with_capacity(expired.len());

        for (key, intent_id, signed_intent) in expired {
            self.intents_by_order.remove(&key);
            self.known_intents.remove(&intent_id);
            removed.push(signed_intent);
        }

        removed
    }

    pub fn contains_intent(&self, intent_id: IntentId) -> bool {
        self.known_intents.contains(&intent_id)
    }

    pub fn contains_policy_nonce(&self, policy_id: u64, nonce: u64) -> bool {
        self.intents_by_order
            .keys()
            .any(|key| key.policy_id == policy_id && key.nonce == nonce)
    }

    pub fn get_intent(&self, expected_intent_id: IntentId) -> Option<&SignedIntent> {
        self.intents_by_order
            .values()
            .find(|signed_intent| intent_id(signed_intent) == expected_intent_id)
    }
}

fn mahi_effective_next_nonce(
    policy_store: &PolicyStore,
    policy_id: u64,
    inflight_policy_nonces: &BTreeMap<u64, BTreeSet<u64>>,
) -> u64 {
    let mut expected_nonce = policy_store.next_intent_nonce(policy_id).unwrap_or(1);
    if let Some(inflight_nonces) = inflight_policy_nonces.get(&policy_id) {
        while inflight_nonces.contains(&expected_nonce) {
            expected_nonce = expected_nonce.saturating_add(1);
        }
    }
    expected_nonce
}

impl ConditionalIntentPool {
    pub fn len(&self) -> usize {
        self.time_after_by_order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.time_after_by_order.is_empty()
    }

    pub fn submit(
        &mut self,
        signed_intent: SignedIntent,
        intent_id: IntentId,
        not_before_ms: u64,
    ) -> Result<(), NodeError> {
        let order_key = IntentOrderKey {
            policy_id: signed_intent.intent.policy_id,
            nonce: signed_intent.intent.nonce,
            intent_id,
        };
        let activation_key = ActivationOrderKey {
            not_before_ms,
            order_key,
        };

        self.known_intents.insert(intent_id);
        self.time_after_by_order
            .insert(activation_key, signed_intent);
        Ok(())
    }

    pub fn purge_expired(&mut self, timestamp_ms: u64) -> Vec<SignedIntent> {
        let expired: Vec<_> = self
            .time_after_by_order
            .iter()
            .filter_map(|(key, signed_intent)| {
                (intent_expires_at_ms(&signed_intent.intent) <= timestamp_ms).then_some((
                    *key,
                    intent_id(signed_intent),
                    signed_intent.clone(),
                ))
            })
            .collect();
        let mut removed = Vec::with_capacity(expired.len());

        for (key, intent_id, signed_intent) in expired {
            self.time_after_by_order.remove(&key);
            self.known_intents.remove(&intent_id);
            removed.push(signed_intent);
        }

        removed
    }

    pub fn promote_ready(
        &mut self,
        policy_store: &PolicyStore,
        direct_mempool: &mut DirectIntentMempool,
        timestamp_ms: u64,
    ) -> Result<Vec<SignedIntent>, NodeError> {
        let ready: Vec<_> = self
            .time_after_by_order
            .iter()
            .filter_map(|(key, signed_intent)| {
                (key.not_before_ms <= timestamp_ms).then_some((*key, signed_intent.clone()))
            })
            .collect();
        let mut promoted = Vec::new();

        for (key, signed_intent) in ready {
            self.time_after_by_order.remove(&key);
            self.known_intents.remove(&key.order_key.intent_id);

            let committed_next_nonce = policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .map_err(NodeError::Policy)?;
            if signed_intent.intent.nonce < committed_next_nonce {
                continue;
            }

            direct_mempool.submit(signed_intent.clone(), key.order_key.intent_id)?;
            promoted.push(signed_intent);
        }

        Ok(promoted)
    }

    pub fn contains_intent(&self, intent_id: IntentId) -> bool {
        self.known_intents.contains(&intent_id)
    }

    pub fn contains_policy_nonce(&self, policy_id: u64, nonce: u64) -> bool {
        self.time_after_by_order
            .keys()
            .any(|key| key.order_key.policy_id == policy_id && key.order_key.nonce == nonce)
    }

    pub fn get_intent(&self, expected_intent_id: IntentId) -> Option<&SignedIntent> {
        self.time_after_by_order
            .values()
            .find(|signed_intent| intent_id(signed_intent) == expected_intent_id)
    }
}

pub(super) fn intent_status_from_queue(
    intent_id: IntentId,
    stage: IntentStatusStage,
    signed_intent: &SignedIntent,
    current_policy_nonce: Option<u64>,
    pending_reason: Option<IntentPendingReason>,
    latest_event: Option<&RuntimeEventRecord>,
) -> IntentStatus {
    let next_eligible_at_ms = match pending_reason {
        Some(IntentPendingReason::TimeAfter) => intent_not_before_ms(&signed_intent.intent),
        Some(IntentPendingReason::NonceFrontier)
        | Some(IntentPendingReason::MarketTradingPaused)
        | Some(IntentPendingReason::StaleMarketData)
        | None => None,
    };

    IntentStatus {
        intent_id,
        stage,
        rejection_reason: None,
        order_rejection_reason: None,
        pending_reason,
        next_eligible_at_ms,
        current_policy_nonce,
        policy_id: Some(signed_intent.intent.policy_id),
        nonce: Some(signed_intent.intent.nonce),
        currently_queued: true,
        latest_event_seq: latest_event.map(|event| event.seq),
        block_height: latest_event.and_then(|event| event.block_height),
        timestamp_ms: latest_event.map(|event| event.timestamp_ms),
    }
}

pub(super) fn intent_status_from_event(
    intent_id: IntentId,
    event: &RuntimeEventRecord,
) -> IntentStatus {
    IntentStatus {
        intent_id,
        stage: event
            .kind
            .intent_status_stage()
            .unwrap_or(IntentStatusStage::Unknown),
        rejection_reason: event.kind.rejection_reason(),
        order_rejection_reason: event.kind.order_rejection_reason(),
        pending_reason: None,
        next_eligible_at_ms: None,
        current_policy_nonce: None,
        policy_id: event.kind.policy_id(),
        nonce: event.kind.intent_nonce(),
        currently_queued: false,
        latest_event_seq: Some(event.seq),
        block_height: event.block_height,
        timestamp_ms: Some(event.timestamp_ms),
    }
}

pub(super) fn rejection_context_from_node_error(
    error: &NodeError,
) -> (IntentRejectionReason, Option<OrderRejectionReason>) {
    match error {
        NodeError::Policy(policy_error) => (rejection_reason_from_policy_error(policy_error), None),
        NodeError::AccountFrozen { .. } => (IntentRejectionReason::AccountFrozen, None),
        NodeError::MarketTradingPaused { .. } => (IntentRejectionReason::MarketTradingPaused, None),
        NodeError::StaleMarketData { .. } => (IntentRejectionReason::StaleMarketData, None),
        NodeError::InvalidCollateralMovement { .. } => (IntentRejectionReason::InternalError, None),
        NodeError::InvalidMandate {
            reason: "mixed_mandate_resting_order",
            ..
        }
        | NodeError::InvalidMandate {
            reason: "max_leverage_exceeded",
            ..
        } => (IntentRejectionReason::MandateConstraintViolated, None),
        NodeError::Order(order_error) => (
            IntentRejectionReason::InternalError,
            Some(order_rejection_reason_from_order_error(order_error)),
        ),
        NodeError::LocalValidatorUnavailable
        | NodeError::InvalidBlock(_)
        | NodeError::InvalidConsensusPubKey { .. }
        | NodeError::MissingConsensusSigningKey { .. }
        | NodeError::InvalidClusterSize { .. }
        | NodeError::BootstrapConfig(_)
        | NodeError::MissingLeader { .. }
        | NodeError::ZeroStake
        | NodeError::Persistence { .. }
        | NodeError::Execution(_)
        | NodeError::Account(_)
        | NodeError::DuplicateIntent(_)
        | NodeError::DuplicatePolicyNonce { .. }
        | NodeError::StaleIntentNonce { .. }
        | NodeError::IntentExpired { .. }
        | NodeError::PositionFlat { .. }
        | NodeError::NotUnderMaintenance { .. }
        | NodeError::UnknownMarket { .. }
        | NodeError::MarketAlreadyListed { .. }
        | NodeError::InvalidMarketConfig { .. }
        | NodeError::InvalidOracleSubmission { .. }
        | NodeError::OracleAggregationFailed { .. }
        | NodeError::InvalidDelegation { .. }
        | NodeError::InvalidMarketMakerProgram { .. }
        | NodeError::InvalidTeeGovernance { .. }
        | NodeError::InvalidRealCapitalGovernance { .. }
        | NodeError::InvalidMandate { .. }
        | NodeError::InvalidIdentity { .. }
        | NodeError::InvalidIdentityAction { .. }
        | NodeError::InvalidIdentityActionNonce { .. }
        | NodeError::InvalidIdentityActionSignature { .. }
        | NodeError::InvalidMandateAction { .. }
        | NodeError::InvalidMandateActionNonce { .. }
        | NodeError::InvalidMandateActionSignature { .. } => {
            (IntentRejectionReason::InternalError, None)
        }
    }
}

pub(super) fn rejection_reason_from_policy_error(error: &PolicyError) -> IntentRejectionReason {
    match error {
        PolicyError::UnknownPolicy(_) => IntentRejectionReason::UnknownPolicy,
        PolicyError::UnauthorizedGrantee => IntentRejectionReason::UnauthorizedGrantee,
        PolicyError::SessionPolicyMismatch { .. } => IntentRejectionReason::SessionPolicyMismatch,
        PolicyError::SessionChainMismatch { .. } => IntentRejectionReason::SessionChainMismatch,
        PolicyError::InvalidSessionSignature | PolicyError::InvalidSessionKeyMaterial => {
            IntentRejectionReason::InvalidSessionSignature
        }
        PolicyError::SessionKeyRevoked { .. }
        | PolicyError::SessionKeyAlreadyActive
        | PolicyError::SessionKeyAlreadyRevoked => IntentRejectionReason::SessionKeyRevoked,
        PolicyError::InvalidIntentNonce { .. } => IntentRejectionReason::InvalidIntentNonce,
        PolicyError::PolicyNotActive(_) => IntentRejectionReason::PolicyNotActive,
        PolicyError::PolicyExpired => IntentRejectionReason::PolicyExpired,
        PolicyError::GasBudgetExceeded => IntentRejectionReason::GasBudgetExceeded,
        PolicyError::IntentRateLimitExceeded => IntentRejectionReason::IntentRateLimitExceeded,
        PolicyError::MinuteIntentLimitExceeded => IntentRejectionReason::MinuteIntentLimitExceeded,
        PolicyError::DailyIntentLimitExceeded => IntentRejectionReason::DailyIntentLimitExceeded,
        PolicyError::DailyVolumeExceeded => IntentRejectionReason::DailyVolumeExceeded,
        PolicyError::MarketNotAllowed => IntentRejectionReason::MarketNotAllowed,
        PolicyError::TradeNotionalExceeded => IntentRejectionReason::TradeNotionalExceeded,
        PolicyError::NotionalOutstandingExceeded => {
            IntentRejectionReason::NotionalOutstandingExceeded
        }
        PolicyError::SubPolicyScopeMismatch => IntentRejectionReason::SubPolicyScopeMismatch,
        PolicyError::SubPolicyTermsExceedParent(_) => {
            IntentRejectionReason::SubPolicyTermsExceedParent
        }
        PolicyError::DelegationDepthExceeded { .. } => {
            IntentRejectionReason::DelegationDepthExceeded
        }
        PolicyError::DrawdownExceededAndRevoked => {
            IntentRejectionReason::DrawdownExceededAndRevoked
        }
        PolicyError::Overflow | PolicyError::DuplicatePolicyId(_) => {
            IntentRejectionReason::Overflow
        }
    }
}

fn order_rejection_reason_from_order_error(error: &OrderRejectReason) -> OrderRejectionReason {
    match error {
        OrderRejectReason::ZeroMarketId => OrderRejectionReason::ZeroMarketId,
        OrderRejectReason::ZeroAccountId => OrderRejectionReason::ZeroAccountId,
        OrderRejectReason::ZeroQuantity => OrderRejectionReason::ZeroQuantity,
        OrderRejectReason::ZeroClientOrderId => OrderRejectionReason::ZeroClientOrderId,
        OrderRejectReason::ZeroLimitPrice => OrderRejectionReason::ZeroLimitPrice,
        OrderRejectReason::MarketMismatch { .. } => OrderRejectionReason::MarketMismatch,
        OrderRejectReason::MarketOrderRequiresImmediateTif => {
            OrderRejectionReason::MarketOrderRequiresImmediateTif
        }
        OrderRejectReason::PostOnlyRequiresLimitOrder => {
            OrderRejectionReason::PostOnlyRequiresLimitOrder
        }
        OrderRejectReason::PostOnlyIncompatibleWithImmediateTif => {
            OrderRejectionReason::PostOnlyIncompatibleWithImmediateTif
        }
        OrderRejectReason::GttRequiresFutureExpiry { .. } => {
            OrderRejectionReason::GttRequiresFutureExpiry
        }
        OrderRejectReason::PostOnlyWouldCross => OrderRejectionReason::PostOnlyWouldCross,
        OrderRejectReason::FillOrKillInsufficientLiquidity => {
            OrderRejectionReason::FillOrKillInsufficientLiquidity
        }
        OrderRejectReason::SelfTradePrevented => OrderRejectionReason::SelfTradePrevented,
        OrderRejectReason::DuplicateClientOrderId => OrderRejectionReason::DuplicateClientOrderId,
        OrderRejectReason::UnknownRestingOrder => OrderRejectionReason::UnknownRestingOrder,
        OrderRejectReason::ReplaceAccountMismatch { .. } => {
            OrderRejectionReason::ReplaceAccountMismatch
        }
    }
}
