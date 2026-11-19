//! Phase 6 parallel-execution scaffolding.
//!
//! Block-STM needs a conservative, deterministic conflict surface before it can
//! schedule intents speculatively. This module classifies the runtime resources
//! touched by each block item without changing the sequential executor.

use kombat_chain_consensus::{BlockItem, IntentKind, PolicyId, SignedIntent};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecutionResource {
    /// Unknown or arbitrary state access. Conflicts with every other access.
    Global,
    /// Per-policy record: nonce, budgets, revocation state, hierarchy metadata.
    Policy(PolicyId),
    /// Policy hierarchy operations whose full descendant set is not statically known.
    PolicyGraph,
    /// Frozen-account admission gate.
    FrozenAccount(u64),
    /// Market paused/stale admission gates for order-producing intents.
    MarketGate(u64),
    /// Market order book and per-account resting order indexes.
    OrderBook(u64),
    /// Per-account isolated collateral and risk state for one market.
    IsolatedCollateral { market_id: u64, account_id: u64 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionAccessSet {
    pub reads: BTreeSet<ExecutionResource>,
    pub writes: BTreeSet<ExecutionResource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionScheduleLayer {
    pub item_indices: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionScheduleSummary {
    pub item_count: usize,
    pub layer_count: usize,
    pub max_layer_width: usize,
    pub conflict_pair_count: usize,
    /// Equal-cost speedup estimate in thousandths, e.g. 2500 means 2.5x.
    pub estimated_parallelism_milli: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionScheduleValidationError {
    OutOfBoundsItem {
        item_index: usize,
        item_count: usize,
    },
    DuplicateItem {
        item_index: usize,
    },
    MissingItem {
        item_index: usize,
    },
    IntraLayerConflict {
        layer_index: usize,
        left_item_index: usize,
        right_item_index: usize,
    },
    DependencyViolation {
        prior_item_index: usize,
        later_item_index: usize,
        prior_layer_index: usize,
        later_layer_index: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionedWrite<V> {
    pub item_index: usize,
    pub value: V,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VersionedWriteBuffer<V> {
    writes: BTreeMap<ExecutionResource, Vec<VersionedWrite<V>>>,
}

impl ExecutionAccessSet {
    pub fn conflicts_with(&self, other: &Self) -> bool {
        if self.touches_global() || other.touches_global() {
            return true;
        }

        intersects(&self.writes, &other.writes)
            || intersects(&self.writes, &other.reads)
            || intersects(&self.reads, &other.writes)
    }

    fn touches_global(&self) -> bool {
        self.reads.contains(&ExecutionResource::Global)
            || self.writes.contains(&ExecutionResource::Global)
    }

    fn read(&mut self, resource: ExecutionResource) {
        self.reads.insert(resource);
    }

    fn write(&mut self, resource: ExecutionResource) {
        self.writes.insert(resource);
    }
}

impl<V> VersionedWriteBuffer<V> {
    pub fn write(&mut self, resource: ExecutionResource, item_index: usize, value: V) {
        self.writes
            .entry(resource)
            .or_default()
            .push(VersionedWrite { item_index, value });
    }

    pub fn writes_for(&self, resource: &ExecutionResource) -> &[VersionedWrite<V>] {
        self.writes.get(resource).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
}

impl<V: Clone> VersionedWriteBuffer<V> {
    pub fn read_latest_before(&self, resource: &ExecutionResource, item_index: usize) -> Option<V> {
        self.writes
            .get(resource)?
            .iter()
            .filter(|write| write.item_index < item_index)
            .max_by_key(|write| write.item_index)
            .map(|write| write.value.clone())
    }

    pub fn materialize_committed(&self) -> BTreeMap<ExecutionResource, V> {
        self.writes
            .iter()
            .filter_map(|(resource, writes)| {
                writes
                    .iter()
                    .max_by_key(|write| write.item_index)
                    .map(|write| (resource.clone(), write.value.clone()))
            })
            .collect()
    }
}

pub fn access_set_for_block_item(item: &BlockItem) -> ExecutionAccessSet {
    match item {
        BlockItem::Intent(signed_intent) => access_set_for_signed_intent(signed_intent),
        BlockItem::Transaction(_) => {
            let mut access_set = ExecutionAccessSet::default();
            access_set.write(ExecutionResource::Global);
            access_set
        }
        BlockItem::Funding(funding) => {
            let mut access_set = ExecutionAccessSet::default();
            access_set.write(ExecutionResource::OrderBook(funding.market_id));
            access_set
        }
        BlockItem::OraclePrice(_) => {
            let mut access_set = ExecutionAccessSet::default();
            access_set.write(ExecutionResource::Global);
            access_set
        }
        BlockItem::Faucet(_) => {
            let mut access_set = ExecutionAccessSet::default();
            access_set.write(ExecutionResource::Global);
            access_set
        }
        BlockItem::Liquidation(liquidation) => {
            let mut access_set = ExecutionAccessSet::default();
            access_set.write(ExecutionResource::OrderBook(liquidation.market_id));
            access_set.write(ExecutionResource::IsolatedCollateral {
                market_id: liquidation.market_id,
                account_id: liquidation.account_id,
            });
            access_set
        }
        BlockItem::Delegation(_)
        | BlockItem::MarketMakerProgram(_)
        | BlockItem::TeeGovernance(_)
        | BlockItem::RealCapitalGovernance(_) => {
            let mut access_set = ExecutionAccessSet::default();
            access_set.write(ExecutionResource::Global);
            access_set
        }
    }
}

pub fn schedule_block_items(items: &[BlockItem]) -> Vec<ExecutionScheduleLayer> {
    let access_sets = items
        .iter()
        .map(access_set_for_block_item)
        .collect::<Vec<_>>();
    schedule_access_sets(&access_sets)
}

pub fn summarize_block_item_schedule(items: &[BlockItem]) -> ExecutionScheduleSummary {
    let access_sets = items
        .iter()
        .map(access_set_for_block_item)
        .collect::<Vec<_>>();
    let layers = schedule_access_sets(&access_sets);
    summarize_access_set_schedule(&access_sets, &layers)
}

pub fn summarize_access_set_schedule(
    access_sets: &[ExecutionAccessSet],
    layers: &[ExecutionScheduleLayer],
) -> ExecutionScheduleSummary {
    let item_count = access_sets.len();
    let layer_count = layers.len();
    let max_layer_width = layers
        .iter()
        .map(|layer| layer.item_indices.len())
        .max()
        .unwrap_or(0);
    let conflict_pair_count = access_sets
        .iter()
        .enumerate()
        .map(|(left_index, left)| {
            access_sets[left_index + 1..]
                .iter()
                .filter(|right| left.conflicts_with(right))
                .count()
        })
        .sum();
    let estimated_parallelism_milli = if layer_count == 0 {
        0
    } else {
        ((item_count as u64) * 1_000) / (layer_count as u64)
    };

    ExecutionScheduleSummary {
        item_count,
        layer_count,
        max_layer_width,
        conflict_pair_count,
        estimated_parallelism_milli,
    }
}

pub fn schedule_access_sets(access_sets: &[ExecutionAccessSet]) -> Vec<ExecutionScheduleLayer> {
    let mut layers: Vec<ExecutionScheduleLayer> = Vec::new();
    let mut item_layers = Vec::with_capacity(access_sets.len());

    for (item_index, access_set) in access_sets.iter().enumerate() {
        let min_layer = access_sets[..item_index]
            .iter()
            .enumerate()
            .filter_map(|(prior_index, prior_access_set)| {
                access_set
                    .conflicts_with(prior_access_set)
                    .then_some(item_layers[prior_index] + 1)
            })
            .max()
            .unwrap_or(0);

        let mut selected_layer = None;
        for (layer_index, layer) in layers.iter().enumerate().skip(min_layer) {
            if layer
                .item_indices
                .iter()
                .all(|prior_index| !access_set.conflicts_with(&access_sets[*prior_index]))
            {
                selected_layer = Some(layer_index);
                break;
            }
        }

        let layer_index = selected_layer.unwrap_or_else(|| {
            layers.push(ExecutionScheduleLayer {
                item_indices: Vec::new(),
            });
            layers.len() - 1
        });
        layers[layer_index].item_indices.push(item_index);
        item_layers.push(layer_index);
    }

    layers
}

pub fn validate_schedule(
    access_sets: &[ExecutionAccessSet],
    layers: &[ExecutionScheduleLayer],
) -> Result<(), ExecutionScheduleValidationError> {
    let mut item_layers = vec![None; access_sets.len()];

    for (layer_index, layer) in layers.iter().enumerate() {
        for &item_index in &layer.item_indices {
            if item_index >= access_sets.len() {
                return Err(ExecutionScheduleValidationError::OutOfBoundsItem {
                    item_index,
                    item_count: access_sets.len(),
                });
            }
            if item_layers[item_index].replace(layer_index).is_some() {
                return Err(ExecutionScheduleValidationError::DuplicateItem { item_index });
            }
        }

        for (position, &left_item_index) in layer.item_indices.iter().enumerate() {
            for &right_item_index in layer.item_indices.iter().skip(position + 1) {
                if access_sets[left_item_index].conflicts_with(&access_sets[right_item_index]) {
                    return Err(ExecutionScheduleValidationError::IntraLayerConflict {
                        layer_index,
                        left_item_index,
                        right_item_index,
                    });
                }
            }
        }
    }

    for (item_index, layer_index) in item_layers.iter().enumerate() {
        if layer_index.is_none() {
            return Err(ExecutionScheduleValidationError::MissingItem { item_index });
        }
    }

    for (prior_item_index, prior_access_set) in access_sets.iter().enumerate() {
        for (later_offset, later_access_set) in
            access_sets[prior_item_index + 1..].iter().enumerate()
        {
            let later_item_index = prior_item_index + 1 + later_offset;
            if !prior_access_set.conflicts_with(later_access_set) {
                continue;
            }

            let prior_layer_index = item_layers[prior_item_index]
                .expect("missing item layers must be rejected before dependency validation");
            let later_layer_index = item_layers[later_item_index]
                .expect("missing item layers must be rejected before dependency validation");
            if prior_layer_index >= later_layer_index {
                return Err(ExecutionScheduleValidationError::DependencyViolation {
                    prior_item_index,
                    later_item_index,
                    prior_layer_index,
                    later_layer_index,
                });
            }
        }
    }

    Ok(())
}

pub fn access_set_for_signed_intent(signed_intent: &SignedIntent) -> ExecutionAccessSet {
    let intent = &signed_intent.intent;
    let mut access_set = ExecutionAccessSet::default();

    access_set.read(ExecutionResource::FrozenAccount(intent.account));
    access_set.read(ExecutionResource::Policy(intent.policy_id));
    access_set.write(ExecutionResource::Policy(intent.policy_id));

    match &intent.kind {
        IntentKind::Transfer { .. } | IntentKind::Cancel { .. } => {}
        IntentKind::PlaceOrder { market_id, .. } => {
            add_order_execution_access(&mut access_set, *market_id, true);
        }
        IntentKind::CancelOrder { market_id, .. } => {
            add_order_execution_access(&mut access_set, *market_id, false);
        }
        IntentKind::ReplaceOrder { market_id, .. } => {
            add_order_execution_access(&mut access_set, *market_id, true);
        }
        IntentKind::CreateSubPolicy {
            child_policy_id, ..
        } => {
            access_set.read(ExecutionResource::PolicyGraph);
            access_set.write(ExecutionResource::PolicyGraph);
            access_set.write(ExecutionResource::Policy(*child_policy_id));
        }
        IntentKind::RevokePolicy { target_policy_id } => {
            access_set.read(ExecutionResource::PolicyGraph);
            access_set.write(ExecutionResource::PolicyGraph);
            access_set.write(ExecutionResource::Policy(*target_policy_id));
        }
        IntentKind::DepositCollateral { market_id, .. }
        | IntentKind::WithdrawCollateral { market_id, .. } => {
            access_set.read(ExecutionResource::MarketGate(*market_id));
            access_set.write(ExecutionResource::IsolatedCollateral {
                market_id: *market_id,
                account_id: intent.account,
            });
        }
    }

    access_set
}

fn add_order_execution_access(
    access_set: &mut ExecutionAccessSet,
    market_id: u64,
    reads_market_gates: bool,
) {
    if reads_market_gates {
        access_set.read(ExecutionResource::MarketGate(market_id));
    }
    access_set.read(ExecutionResource::OrderBook(market_id));
    access_set.write(ExecutionResource::OrderBook(market_id));
}

fn intersects<T: Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> bool {
    left.iter().any(|value| right.contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{simulate_intent_on_runtime_state, PolicyAdmissionCommitMode};
    use kombat_chain_consensus::{
        Intent, IntentOrderKind, IntentOrderSide, IntentTimeInForce, PolicyTermsWire,
        SignedTransaction,
    };
    use kombat_chain_matching::SingleMarketBook;
    use kombat_chain_oracle::{OracleConfig, OraclePrice};
    use kombat_chain_policy::{PolicyRecord, PolicyState, PolicyStore, PolicyTerms};
    use kombat_chain_streams::RuntimeEventKind;

    fn signed_intent(policy_id: PolicyId, nonce: u64, kind: IntentKind) -> SignedIntent {
        SignedIntent {
            intent: Intent {
                chain_id: 1,
                account: policy_id + 1_000,
                policy_id,
                nonce,
                expires_at: 10_000,
                predicate: None,
                kind,
            },
            signature: [7; 64],
        }
    }

    fn transfer(policy_id: PolicyId) -> SignedIntent {
        signed_intent(
            policy_id,
            1,
            IntentKind::Transfer {
                to: 9000 + policy_id,
                token: 1,
                amount: 10,
            },
        )
    }

    fn transfer_with_nonce(policy_id: PolicyId, nonce: u64) -> SignedIntent {
        signed_intent(
            policy_id,
            nonce,
            IntentKind::Transfer {
                to: 9000 + policy_id,
                token: 1,
                amount: 10,
            },
        )
    }

    fn cancel_with_nonce(policy_id: PolicyId, nonce: u64) -> SignedIntent {
        signed_intent(policy_id, nonce, IntentKind::Cancel { order_id: nonce })
    }

    fn place_order(policy_id: PolicyId, market_id: u64) -> SignedIntent {
        limit_order_with_nonce(policy_id, 1, market_id, 42, IntentOrderSide::Bid, 100, 5)
    }

    fn limit_order_with_nonce(
        policy_id: PolicyId,
        nonce: u64,
        market_id: u64,
        client_order_id: u64,
        side: IntentOrderSide,
        price_ticks: u64,
        quantity_lots: u64,
    ) -> SignedIntent {
        signed_intent(
            policy_id,
            nonce,
            IntentKind::PlaceOrder {
                market_id,
                client_order_id,
                side,
                quantity_lots,
                kind: IntentOrderKind::Limit { price_ticks },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        )
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RealExecutionSnapshot {
        policies: BTreeMap<PolicyId, PolicyRecord>,
        order_books: BTreeMap<u64, SingleMarketBook>,
        events: Vec<RuntimeEventKind>,
    }

    fn policy_for_real_intent(policy_id: PolicyId) -> PolicyRecord {
        PolicyRecord::new(
            policy_id,
            policy_id + 1_000,
            [policy_id as u8; 32],
            1_000,
            10_000,
            PolicyState::Active,
            PolicyTerms {
                gas_budget_per_day: 1_000_000,
                gas_refill_per_day: 1_000_000,
                max_intents_per_second: 10_000,
                max_intents_per_day: 100_000,
                max_volume_per_day: 1_000_000,
                max_drawdown_pct: 10_000,
                ..PolicyTerms::default()
            },
        )
    }

    fn policy_store_for_intents(intents: &[SignedIntent]) -> PolicyStore {
        let mut policy_store = PolicyStore::default();
        for intent in intents {
            if policy_store.get(intent.intent.policy_id).is_none() {
                policy_store
                    .insert(policy_for_real_intent(intent.intent.policy_id))
                    .expect("test policy ids should be unique");
            }
        }
        policy_store
    }

    fn policy_state_snapshot(
        policy_store: &PolicyStore,
        policy_ids: &[PolicyId],
    ) -> BTreeMap<PolicyId, PolicyRecord> {
        policy_ids
            .iter()
            .map(|policy_id| {
                (
                    *policy_id,
                    policy_store
                        .get(*policy_id)
                        .expect("test policy should exist")
                        .clone(),
                )
            })
            .collect()
    }

    fn execute_real_intents_sequentially(intents: &[SignedIntent]) -> RealExecutionSnapshot {
        let mut policy_store = policy_store_for_intents(intents);
        let mut order_books = BTreeMap::<u64, SingleMarketBook>::new();
        let mut events = Vec::new();
        for intent in intents {
            events.extend(execute_real_intent(
                &mut policy_store,
                &mut order_books,
                intent,
            ));
        }
        RealExecutionSnapshot {
            policies: policy_state_snapshot(&policy_store, &policy_ids_for_intents(intents)),
            order_books,
            events,
        }
    }

    fn execute_real_intents_by_schedule(
        intents: &[SignedIntent],
        layers: &[ExecutionScheduleLayer],
    ) -> RealExecutionSnapshot {
        let mut policy_store = policy_store_for_intents(intents);
        let mut order_books = BTreeMap::<u64, SingleMarketBook>::new();
        let mut events_by_item = BTreeMap::<usize, Vec<RuntimeEventKind>>::new();
        for layer in layers {
            for &item_index in layer.item_indices.iter().rev() {
                let events =
                    execute_real_intent(&mut policy_store, &mut order_books, &intents[item_index]);
                events_by_item.insert(item_index, events);
            }
        }
        let events = events_by_item.into_values().flatten().collect();
        RealExecutionSnapshot {
            policies: policy_state_snapshot(&policy_store, &policy_ids_for_intents(intents)),
            order_books,
            events,
        }
    }

    fn policy_ids_for_intents(intents: &[SignedIntent]) -> Vec<PolicyId> {
        intents
            .iter()
            .map(|intent| intent.intent.policy_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn execute_real_intent(
        policy_store: &mut PolicyStore,
        order_books: &mut BTreeMap<u64, SingleMarketBook>,
        intent: &SignedIntent,
    ) -> Vec<RuntimeEventKind> {
        let mut isolated_collateral = BTreeMap::new();
        let mut resting_order_mandates = BTreeMap::new();
        simulate_intent_on_runtime_state(
            policy_store,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            order_books,
            &mut resting_order_mandates,
            &mut isolated_collateral,
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeSet::new(),
            &BTreeMap::<u64, OraclePrice>::new(),
            &BTreeMap::<u64, OraclePrice>::new(),
            &BTreeMap::<u64, OracleConfig>::new(),
            &BTreeMap::new(),
            intent,
            1,
            1,
            1_200,
            PolicyAdmissionCommitMode::PreverifiedSession,
        )
        .expect("real test intent should execute")
    }

    #[test]
    fn transfers_on_distinct_policies_do_not_conflict() {
        let left = access_set_for_signed_intent(&transfer(10));
        let right = access_set_for_signed_intent(&transfer(11));

        assert!(!left.conflicts_with(&right));
    }

    #[test]
    fn same_policy_intents_conflict_on_nonce_state() {
        let left = access_set_for_signed_intent(&transfer(10));
        let right =
            access_set_for_signed_intent(&signed_intent(10, 2, IntentKind::Cancel { order_id: 7 }));

        assert!(left.conflicts_with(&right));
    }

    #[test]
    fn order_intents_on_same_market_conflict() {
        let left = access_set_for_signed_intent(&place_order(10, 7));
        let right = access_set_for_signed_intent(&place_order(11, 7));

        assert!(left.conflicts_with(&right));
    }

    #[test]
    fn order_intents_on_distinct_markets_and_policies_do_not_conflict() {
        let left = access_set_for_signed_intent(&place_order(10, 7));
        let right = access_set_for_signed_intent(&place_order(11, 8));

        assert!(!left.conflicts_with(&right));
    }

    #[test]
    fn policy_graph_mutations_conflict_even_when_policy_ids_differ() {
        let create = access_set_for_signed_intent(&signed_intent(
            10,
            1,
            IntentKind::CreateSubPolicy {
                child_policy_id: 20,
                child_grantee: [2; 32],
                child_expires_at: 10_000,
                child_terms: PolicyTermsWire {
                    gas_budget_per_day: 1_000,
                    gas_refill_per_day: 1_000,
                    max_intents_per_second: 100,
                    max_intents_per_day: 10_000,
                    max_volume_per_day: 1_000_000,
                    max_drawdown_pct: 1_000,
                },
            },
        ));
        let revoke = access_set_for_signed_intent(&signed_intent(
            11,
            1,
            IntentKind::RevokePolicy {
                target_policy_id: 21,
            },
        ));

        assert!(create.conflicts_with(&revoke));
    }

    #[test]
    fn transaction_items_conflict_with_everything() {
        let transaction = BlockItem::Transaction(SignedTransaction {
            signer: [3; 32],
            bytes: vec![1, 2, 3],
        });
        let transaction_access = access_set_for_block_item(&transaction);
        let transfer_access = access_set_for_signed_intent(&transfer(10));

        assert!(transaction_access.conflicts_with(&transfer_access));
    }

    #[test]
    fn schedule_groups_independent_transfers() {
        let items = vec![
            BlockItem::Intent(transfer(10)),
            BlockItem::Intent(transfer(11)),
            BlockItem::Intent(transfer(12)),
        ];

        let layers = schedule_block_items(&items);
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();

        assert_eq!(
            layers,
            vec![ExecutionScheduleLayer {
                item_indices: vec![0, 1, 2],
            }]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
    }

    #[test]
    fn schedule_serializes_same_policy_conflicts() {
        let items = vec![
            BlockItem::Intent(transfer(10)),
            BlockItem::Intent(signed_intent(10, 2, IntentKind::Cancel { order_id: 7 })),
            BlockItem::Intent(transfer(11)),
        ];

        let layers = schedule_block_items(&items);
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();

        assert_eq!(
            layers,
            vec![
                ExecutionScheduleLayer {
                    item_indices: vec![0, 2],
                },
                ExecutionScheduleLayer {
                    item_indices: vec![1],
                },
            ]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
    }

    #[test]
    fn schedule_preserves_conflicting_happens_before_after_global_item() {
        let items = vec![
            BlockItem::Intent(transfer(10)),
            BlockItem::Transaction(SignedTransaction {
                signer: [3; 32],
                bytes: vec![1, 2, 3],
            }),
            BlockItem::Intent(transfer(11)),
        ];

        let layers = schedule_block_items(&items);
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();

        assert_eq!(
            layers,
            vec![
                ExecutionScheduleLayer {
                    item_indices: vec![0],
                },
                ExecutionScheduleLayer {
                    item_indices: vec![1],
                },
                ExecutionScheduleLayer {
                    item_indices: vec![2],
                },
            ]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
    }

    #[test]
    fn schedule_parallelizes_around_order_book_conflicts() {
        let items = vec![
            BlockItem::Intent(place_order(10, 7)),
            BlockItem::Intent(place_order(11, 8)),
            BlockItem::Intent(place_order(12, 7)),
            BlockItem::Intent(place_order(13, 8)),
        ];

        let layers = schedule_block_items(&items);
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();

        assert_eq!(
            layers,
            vec![
                ExecutionScheduleLayer {
                    item_indices: vec![0, 1],
                },
                ExecutionScheduleLayer {
                    item_indices: vec![2, 3],
                },
            ]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
    }

    #[test]
    fn validator_rejects_duplicate_and_missing_items() {
        let access_sets = vec![
            access_set_for_signed_intent(&transfer(10)),
            access_set_for_signed_intent(&transfer(11)),
        ];

        let duplicate = vec![ExecutionScheduleLayer {
            item_indices: vec![0, 0],
        }];
        assert_eq!(
            validate_schedule(&access_sets, &duplicate),
            Err(ExecutionScheduleValidationError::DuplicateItem { item_index: 0 })
        );

        let missing = vec![ExecutionScheduleLayer {
            item_indices: vec![0],
        }];
        assert_eq!(
            validate_schedule(&access_sets, &missing),
            Err(ExecutionScheduleValidationError::MissingItem { item_index: 1 })
        );
    }

    #[test]
    fn validator_rejects_intra_layer_conflicts_and_reordered_dependencies() {
        let access_sets = vec![
            access_set_for_signed_intent(&transfer(10)),
            access_set_for_signed_intent(&signed_intent(10, 2, IntentKind::Cancel { order_id: 7 })),
        ];

        let intra_layer = vec![ExecutionScheduleLayer {
            item_indices: vec![0, 1],
        }];
        assert_eq!(
            validate_schedule(&access_sets, &intra_layer),
            Err(ExecutionScheduleValidationError::IntraLayerConflict {
                layer_index: 0,
                left_item_index: 0,
                right_item_index: 1,
            })
        );

        let reordered = vec![
            ExecutionScheduleLayer {
                item_indices: vec![1],
            },
            ExecutionScheduleLayer {
                item_indices: vec![0],
            },
        ];
        assert_eq!(
            validate_schedule(&access_sets, &reordered),
            Err(ExecutionScheduleValidationError::DependencyViolation {
                prior_item_index: 0,
                later_item_index: 1,
                prior_layer_index: 1,
                later_layer_index: 0,
            })
        );
    }

    #[derive(Clone)]
    struct MockOp {
        access_set: ExecutionAccessSet,
        writes: Vec<(ExecutionResource, u64)>,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct MockLayerExecutionStats {
        layer_count: usize,
        executed_items: usize,
        conflict_retries: usize,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MockLayerExecutionResult {
        writes: VersionedWriteBuffer<u64>,
        stats: MockLayerExecutionStats,
    }

    fn execute_mock_ops_sequentially(ops: &[MockOp]) -> BTreeMap<ExecutionResource, u64> {
        let mut state = BTreeMap::new();
        for op in ops {
            apply_mock_op(&mut state, op);
        }
        state
    }

    fn execute_mock_ops_by_schedule(
        ops: &[MockOp],
        layers: &[ExecutionScheduleLayer],
    ) -> BTreeMap<ExecutionResource, u64> {
        execute_mock_layers(ops, layers)
            .expect("test schedule should execute")
            .writes
            .materialize_committed()
    }

    fn execute_mock_layers(
        ops: &[MockOp],
        layers: &[ExecutionScheduleLayer],
    ) -> Result<MockLayerExecutionResult, ExecutionScheduleValidationError> {
        let access_sets = ops
            .iter()
            .map(|op| op.access_set.clone())
            .collect::<Vec<_>>();
        validate_schedule(&access_sets, layers)?;

        let mut writes = VersionedWriteBuffer::default();
        let mut stats = MockLayerExecutionStats {
            layer_count: layers.len(),
            ..MockLayerExecutionStats::default()
        };
        for layer in layers {
            for &item_index in &layer.item_indices {
                apply_mock_op_to_write_buffer(&mut writes, item_index, &ops[item_index]);
                stats.executed_items += 1;
            }
        }
        Ok(MockLayerExecutionResult { writes, stats })
    }

    fn execute_mock_layers_with_retry(
        ops: &[MockOp],
        candidate_layers: &[ExecutionScheduleLayer],
    ) -> MockLayerExecutionResult {
        match execute_mock_layers(ops, candidate_layers) {
            Ok(result) => result,
            Err(_) => {
                let access_sets = ops
                    .iter()
                    .map(|op| op.access_set.clone())
                    .collect::<Vec<_>>();
                let repaired_layers = schedule_access_sets(&access_sets);
                let mut result =
                    execute_mock_layers(ops, &repaired_layers).expect("repaired schedule is valid");
                result.stats.conflict_retries += 1;
                result
            }
        }
    }

    fn apply_mock_op(state: &mut BTreeMap<ExecutionResource, u64>, op: &MockOp) {
        for (resource, value) in &op.writes {
            state.insert(resource.clone(), *value);
        }
    }

    fn apply_mock_op_to_write_buffer(
        writes: &mut VersionedWriteBuffer<u64>,
        item_index: usize,
        op: &MockOp,
    ) {
        for (resource, value) in &op.writes {
            writes.write(resource.clone(), item_index, *value);
        }
    }

    #[test]
    fn scheduled_serial_mock_execution_matches_sequential_result() {
        let ops = vec![
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(10, 7)),
                writes: vec![(ExecutionResource::OrderBook(7), 1)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(11, 8)),
                writes: vec![(ExecutionResource::OrderBook(8), 2)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(12, 7)),
                writes: vec![(ExecutionResource::OrderBook(7), 3)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&transfer(13)),
                writes: vec![(ExecutionResource::Policy(13), 4)],
            },
        ];
        let access_sets = ops
            .iter()
            .map(|op| op.access_set.clone())
            .collect::<Vec<_>>();
        let layers = schedule_access_sets(&access_sets);

        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
        assert_eq!(
            execute_mock_ops_by_schedule(&ops, &layers),
            execute_mock_ops_sequentially(&ops)
        );
    }

    #[test]
    fn versioned_write_buffer_materializes_by_item_order() {
        let resource = ExecutionResource::Policy(10);
        let mut buffer = VersionedWriteBuffer::default();

        buffer.write(resource.clone(), 7, 70);
        buffer.write(resource.clone(), 3, 30);
        buffer.write(ExecutionResource::OrderBook(4), 5, 50);

        assert_eq!(buffer.read_latest_before(&resource, 7), Some(30));
        assert_eq!(buffer.read_latest_before(&resource, 8), Some(70));
        assert_eq!(buffer.read_latest_before(&resource, 3), None);
        assert_eq!(
            buffer.writes_for(&resource),
            &[
                VersionedWrite {
                    item_index: 7,
                    value: 70,
                },
                VersionedWrite {
                    item_index: 3,
                    value: 30,
                },
            ]
        );

        let materialized = buffer.materialize_committed();
        assert_eq!(materialized.get(&resource), Some(&70));
        assert_eq!(
            materialized.get(&ExecutionResource::OrderBook(4)),
            Some(&50)
        );
    }

    #[test]
    fn scheduled_mock_writes_materialize_like_sequential_execution() {
        let ops = vec![
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(10, 7)),
                writes: vec![(ExecutionResource::OrderBook(7), 1)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&transfer(11)),
                writes: vec![(ExecutionResource::Policy(11), 2)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(12, 7)),
                writes: vec![(ExecutionResource::OrderBook(7), 3)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&transfer(13)),
                writes: vec![(ExecutionResource::Policy(13), 4)],
            },
        ];
        let access_sets = ops
            .iter()
            .map(|op| op.access_set.clone())
            .collect::<Vec<_>>();
        let layers = schedule_access_sets(&access_sets);

        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
        assert_eq!(
            execute_mock_layers(&ops, &layers)
                .expect("valid schedule should execute")
                .writes
                .materialize_committed(),
            execute_mock_ops_sequentially(&ops)
        );
    }

    #[test]
    fn mock_layer_executor_reports_stats_for_valid_schedule() {
        let ops = vec![
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(10, 7)),
                writes: vec![(ExecutionResource::OrderBook(7), 1)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(11, 8)),
                writes: vec![(ExecutionResource::OrderBook(8), 2)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&place_order(12, 7)),
                writes: vec![(ExecutionResource::OrderBook(7), 3)],
            },
        ];
        let access_sets = ops
            .iter()
            .map(|op| op.access_set.clone())
            .collect::<Vec<_>>();
        let layers = schedule_access_sets(&access_sets);

        let result = execute_mock_layers(&ops, &layers).expect("valid schedule should execute");

        assert_eq!(
            result.stats,
            MockLayerExecutionStats {
                layer_count: 2,
                executed_items: 3,
                conflict_retries: 0,
            }
        );
        assert_eq!(
            result.writes.materialize_committed(),
            execute_mock_ops_sequentially(&ops)
        );
    }

    #[test]
    fn mock_layer_executor_repairs_invalid_candidate_schedule() {
        let ops = vec![
            MockOp {
                access_set: access_set_for_signed_intent(&transfer(10)),
                writes: vec![(ExecutionResource::Policy(10), 1)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&signed_intent(
                    10,
                    2,
                    IntentKind::Cancel { order_id: 7 },
                )),
                writes: vec![(ExecutionResource::Policy(10), 2)],
            },
            MockOp {
                access_set: access_set_for_signed_intent(&transfer(11)),
                writes: vec![(ExecutionResource::Policy(11), 3)],
            },
        ];
        let invalid_layers = vec![ExecutionScheduleLayer {
            item_indices: vec![0, 1, 2],
        }];

        let result = execute_mock_layers_with_retry(&ops, &invalid_layers);

        assert_eq!(
            result.stats,
            MockLayerExecutionStats {
                layer_count: 2,
                executed_items: 3,
                conflict_retries: 1,
            }
        );
        assert_eq!(
            result.writes.materialize_committed(),
            execute_mock_ops_sequentially(&ops)
        );
    }

    #[test]
    fn scheduled_real_policy_intents_match_sequential_for_independent_policies() {
        let intents = vec![
            transfer_with_nonce(10, 1),
            cancel_with_nonce(11, 1),
            transfer_with_nonce(12, 1),
        ];
        let items = intents
            .iter()
            .cloned()
            .map(BlockItem::Intent)
            .collect::<Vec<_>>();
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();
        let layers = schedule_block_items(&items);

        assert_eq!(
            layers,
            vec![ExecutionScheduleLayer {
                item_indices: vec![0, 1, 2],
            }]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
        assert_eq!(
            execute_real_intents_by_schedule(&intents, &layers),
            execute_real_intents_sequentially(&intents)
        );
    }

    #[test]
    fn scheduled_real_policy_intents_preserve_same_policy_nonce_order() {
        let intents = vec![
            transfer_with_nonce(10, 1),
            cancel_with_nonce(10, 2),
            transfer_with_nonce(11, 1),
        ];
        let items = intents
            .iter()
            .cloned()
            .map(BlockItem::Intent)
            .collect::<Vec<_>>();
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();
        let layers = schedule_block_items(&items);

        assert_eq!(
            layers,
            vec![
                ExecutionScheduleLayer {
                    item_indices: vec![0, 2],
                },
                ExecutionScheduleLayer {
                    item_indices: vec![1],
                },
            ]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
        assert_eq!(
            execute_real_intents_by_schedule(&intents, &layers),
            execute_real_intents_sequentially(&intents)
        );
    }

    #[test]
    fn scheduled_real_order_intents_match_sequential_for_distinct_markets() {
        let intents = vec![
            limit_order_with_nonce(10, 1, 7, 1, IntentOrderSide::Bid, 100, 5),
            limit_order_with_nonce(11, 1, 8, 1, IntentOrderSide::Ask, 101, 6),
            limit_order_with_nonce(12, 1, 9, 1, IntentOrderSide::Bid, 99, 7),
        ];
        let items = intents
            .iter()
            .cloned()
            .map(BlockItem::Intent)
            .collect::<Vec<_>>();
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();
        let layers = schedule_block_items(&items);

        assert_eq!(
            layers,
            vec![ExecutionScheduleLayer {
                item_indices: vec![0, 1, 2],
            }]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
        assert_eq!(
            execute_real_intents_by_schedule(&intents, &layers),
            execute_real_intents_sequentially(&intents)
        );
    }

    #[test]
    fn scheduled_real_order_intents_preserve_same_market_match_order() {
        let intents = vec![
            limit_order_with_nonce(10, 1, 7, 1, IntentOrderSide::Ask, 101, 5),
            limit_order_with_nonce(11, 1, 7, 1, IntentOrderSide::Bid, 102, 3),
        ];
        let items = intents
            .iter()
            .cloned()
            .map(BlockItem::Intent)
            .collect::<Vec<_>>();
        let access_sets = items
            .iter()
            .map(access_set_for_block_item)
            .collect::<Vec<_>>();
        let layers = schedule_block_items(&items);

        assert_eq!(
            layers,
            vec![
                ExecutionScheduleLayer {
                    item_indices: vec![0],
                },
                ExecutionScheduleLayer {
                    item_indices: vec![1],
                },
            ]
        );
        assert_eq!(validate_schedule(&access_sets, &layers), Ok(()));
        assert_eq!(
            execute_real_intents_by_schedule(&intents, &layers),
            execute_real_intents_sequentially(&intents)
        );
    }

    #[test]
    fn schedule_summary_reports_parallel_independent_items() {
        let items = vec![
            BlockItem::Intent(transfer(10)),
            BlockItem::Intent(transfer(11)),
            BlockItem::Intent(transfer(12)),
            BlockItem::Intent(transfer(13)),
        ];

        assert_eq!(
            summarize_block_item_schedule(&items),
            ExecutionScheduleSummary {
                item_count: 4,
                layer_count: 1,
                max_layer_width: 4,
                conflict_pair_count: 0,
                estimated_parallelism_milli: 4_000,
            }
        );
    }

    #[test]
    fn schedule_summary_counts_market_conflict_pairs() {
        let items = vec![
            BlockItem::Intent(place_order(10, 7)),
            BlockItem::Intent(place_order(11, 8)),
            BlockItem::Intent(place_order(12, 7)),
            BlockItem::Intent(place_order(13, 8)),
        ];

        assert_eq!(
            summarize_block_item_schedule(&items),
            ExecutionScheduleSummary {
                item_count: 4,
                layer_count: 2,
                max_layer_width: 2,
                conflict_pair_count: 2,
                estimated_parallelism_milli: 2_000,
            }
        );
    }

    #[test]
    fn schedule_summary_counts_global_barrier_conflicts() {
        let items = vec![
            BlockItem::Intent(transfer(10)),
            BlockItem::Transaction(SignedTransaction {
                signer: [3; 32],
                bytes: vec![1, 2, 3],
            }),
            BlockItem::Intent(transfer(11)),
        ];

        assert_eq!(
            summarize_block_item_schedule(&items),
            ExecutionScheduleSummary {
                item_count: 3,
                layer_count: 3,
                max_layer_width: 1,
                conflict_pair_count: 2,
                estimated_parallelism_milli: 1_000,
            }
        );
    }
}
