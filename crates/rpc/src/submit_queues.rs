use crate::{NetworkMahiBlockRequest, RpcError, SubmitIntentParams, SubmitIntentResponse};
use kombat_chain_consensus::{
    intent_expires_at_ms, intent_id, IntentId, SignedIntent, ValidatorId,
};
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::SyncSender;

const MAX_MAHI_PRE_ADMITTED_POOL_ITEMS: usize = 524_288;

#[derive(Clone, Debug)]
pub(crate) struct PendingIntentRelay {
    pub(crate) params: SubmitIntentParams,
    pub(crate) policy_id: u64,
    pub(crate) nonce: u64,
    pub(crate) pending_validators: BTreeSet<ValidatorId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PendingIntentRelayKey {
    pub(crate) nonce: u64,
    pub(crate) policy_id: u64,
    pub(crate) intent_id: IntentId,
}

impl PendingIntentRelay {
    fn key(&self, intent_id: IntentId) -> PendingIntentRelayKey {
        PendingIntentRelayKey {
            nonce: self.nonce,
            policy_id: self.policy_id,
            intent_id,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingIntentRelayQueue {
    pub(crate) intents: BTreeMap<IntentId, PendingIntentRelay>,
    pub(crate) ordered: BTreeSet<PendingIntentRelayKey>,
}

impl PendingIntentRelayQueue {
    pub(crate) fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }

    pub(crate) fn insert(&mut self, intent_id: IntentId, relay: PendingIntentRelay) {
        let key = relay.key(intent_id);
        if let Some(previous) = self.intents.insert(intent_id, relay) {
            self.ordered.remove(&previous.key(intent_id));
        }
        self.ordered.insert(key);
    }

    pub(crate) fn remove(&mut self, intent_id: &IntentId) -> Option<PendingIntentRelay> {
        let relay = self.intents.remove(intent_id)?;
        self.ordered.remove(&relay.key(*intent_id));
        Some(relay)
    }

    pub(crate) fn clear(&mut self) {
        self.intents.clear();
        self.ordered.clear();
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingMahiOwnerRelayQueues {
    pub(crate) by_validator: BTreeMap<ValidatorId, VecDeque<(IntentId, SubmitIntentParams)>>,
}

impl PendingMahiOwnerRelayQueues {
    pub(crate) fn is_empty(&self) -> bool {
        self.by_validator.values().all(VecDeque::is_empty)
    }

    pub(crate) fn insert(
        &mut self,
        validator_id: ValidatorId,
        intent_id: IntentId,
        params: SubmitIntentParams,
    ) {
        self.by_validator
            .entry(validator_id)
            .or_default()
            .push_back((intent_id, params));
    }

    pub(crate) fn take_batches(
        &mut self,
        max_items_per_validator: usize,
    ) -> BTreeMap<ValidatorId, Vec<(IntentId, SubmitIntentParams)>> {
        let validator_ids = self.by_validator.keys().copied().collect::<Vec<_>>();
        let mut batches = BTreeMap::new();
        for validator_id in validator_ids {
            let Some(queue) = self.by_validator.get_mut(&validator_id) else {
                continue;
            };
            let mut batch = Vec::new();
            while batch.len() < max_items_per_validator {
                let Some(item) = queue.pop_front() else {
                    break;
                };
                batch.push(item);
            }
            if queue.is_empty() {
                self.by_validator.remove(&validator_id);
            }
            if !batch.is_empty() {
                batches.insert(validator_id, batch);
            }
        }
        batches
    }

    pub(crate) fn requeue_front(
        &mut self,
        validator_id: ValidatorId,
        items: Vec<(IntentId, SubmitIntentParams)>,
    ) {
        if items.is_empty() {
            return;
        }
        let queue = self.by_validator.entry(validator_id).or_default();
        for item in items.into_iter().rev() {
            queue.push_front(item);
        }
    }

    pub(crate) fn pending_intents(&self) -> usize {
        self.by_validator.values().map(VecDeque::len).sum()
    }

    pub(crate) fn pending_targets(&self) -> usize {
        self.by_validator
            .values()
            .filter(|queue| !queue.is_empty())
            .count()
    }

    pub(crate) fn clear(&mut self) {
        self.by_validator.clear();
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PendingMahiBlockBroadcast {
    pub(crate) local_validator_id: ValidatorId,
    pub(crate) request: NetworkMahiBlockRequest,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingMahiBlockBroadcastQueue {
    pub(crate) blocks: VecDeque<PendingMahiBlockBroadcast>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubmitQueueStatsSnapshot {
    pub(crate) local_submit_queue_items: usize,
    pub(crate) pending_relay_intents: usize,
    pub(crate) pending_relay_targets: usize,
    pub(crate) pending_mahi_block_broadcasts: usize,
    pub(crate) mahi_pre_admitted_pool_items: usize,
    pub(crate) mahi_pre_admitted_pool_accepted_total: u64,
    pub(crate) mahi_pre_admitted_pool_duplicate_total: u64,
    pub(crate) mahi_pre_admitted_pool_drained_total: u64,
    pub(crate) mahi_pre_admitted_pool_dropped_oldest_total: u64,
    pub(crate) mahi_pre_admitted_pool_dropped_stale_total: u64,
}

#[derive(Debug)]
pub(crate) struct QueuedLocalSubmit {
    pub(crate) params: SubmitIntentParams,
    pub(crate) signed_intent: SignedIntent,
    pub(crate) preview_response: SubmitIntentResponse,
    pub(crate) relay_after_admit: bool,
    pub(crate) result_tx: Option<SyncSender<Result<SubmitIntentResponse, RpcError>>>,
}

#[derive(Debug, Default)]
pub(crate) struct LocalSubmitQueue {
    pub(crate) items: VecDeque<QueuedLocalSubmit>,
}

#[derive(Clone, Debug)]
struct MahiPreAdmittedIntent {
    signed_intent: SignedIntent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MahiPreAdmittedPoolStats {
    pub(crate) items: usize,
    pub(crate) accepted_total: u64,
    pub(crate) duplicate_total: u64,
    pub(crate) drained_total: u64,
    pub(crate) dropped_oldest_total: u64,
    pub(crate) dropped_stale_total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MahiPreAdmittedInsertResult {
    Accepted,
    Duplicate,
    Full,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MahiPreAdmittedPool {
    by_intent: BTreeMap<IntentId, MahiPreAdmittedIntent>,
    by_policy_nonce: BTreeMap<u64, BTreeMap<u64, IntentId>>,
    insertion_order: VecDeque<IntentId>,
    next_policy_cursor: Option<u64>,
    accepted_total: u64,
    duplicate_total: u64,
    drained_total: u64,
    dropped_oldest_total: u64,
    dropped_stale_total: u64,
}

impl MahiPreAdmittedPool {
    pub(crate) fn insert(&mut self, signed_intent: SignedIntent) -> MahiPreAdmittedInsertResult {
        self.insert_with_limit(signed_intent, MAX_MAHI_PRE_ADMITTED_POOL_ITEMS)
    }

    pub(crate) fn insert_with_limit(
        &mut self,
        signed_intent: SignedIntent,
        max_items: usize,
    ) -> MahiPreAdmittedInsertResult {
        let intent_id = intent_id(&signed_intent);
        if self.by_intent.contains_key(&intent_id)
            || self
                .by_policy_nonce
                .get(&signed_intent.intent.policy_id)
                .is_some_and(|by_nonce| by_nonce.contains_key(&signed_intent.intent.nonce))
        {
            self.duplicate_total = self.duplicate_total.saturating_add(1);
            return MahiPreAdmittedInsertResult::Duplicate;
        }
        if self.by_intent.len() >= max_items {
            return MahiPreAdmittedInsertResult::Full;
        }

        self.by_policy_nonce
            .entry(signed_intent.intent.policy_id)
            .or_default()
            .insert(signed_intent.intent.nonce, intent_id);
        self.by_intent
            .insert(intent_id, MahiPreAdmittedIntent { signed_intent });
        self.insertion_order.push_back(intent_id);
        self.accepted_total = self.accepted_total.saturating_add(1);
        MahiPreAdmittedInsertResult::Accepted
    }

    pub(crate) fn policy_ids(&self) -> Vec<u64> {
        self.by_policy_nonce.keys().copied().collect()
    }

    fn rotated_policy_ids(&self) -> Vec<u64> {
        let mut policy_ids = self.policy_ids();
        let Some(cursor) = self.next_policy_cursor else {
            return policy_ids;
        };
        if let Some(index) = policy_ids.iter().position(|policy_id| *policy_id >= cursor) {
            policy_ids.rotate_left(index);
        }
        policy_ids
    }

    pub(crate) fn drain_for_block(
        &mut self,
        timestamp_ms: u64,
        max_items: usize,
        policy_next_nonces: &BTreeMap<u64, u64>,
        inflight_policy_nonces: &BTreeMap<u64, BTreeSet<u64>>,
        excluded_intents: &BTreeSet<IntentId>,
    ) -> Vec<SignedIntent> {
        if max_items == 0 || self.by_intent.is_empty() {
            return Vec::new();
        }

        let mut selected = Vec::with_capacity(max_items);
        let mut stale = Vec::new();
        let mut last_selected_policy = None;
        let policy_ids = self.rotated_policy_ids();
        let mut expected_nonces = policy_ids
            .iter()
            .map(|policy_id| {
                let mut expected_nonce = policy_next_nonces.get(policy_id).copied().unwrap_or(1);
                if let Some(inflight_nonces) = inflight_policy_nonces.get(policy_id) {
                    while inflight_nonces.contains(&expected_nonce) {
                        expected_nonce = expected_nonce.saturating_add(1);
                    }
                }
                (*policy_id, expected_nonce)
            })
            .collect::<BTreeMap<_, _>>();
        let mut exhausted = BTreeSet::new();

        while selected.len() < max_items && exhausted.len() < policy_ids.len() {
            let mut made_progress = false;
            for policy_id in policy_ids.iter().copied() {
                if selected.len() >= max_items {
                    break;
                }
                if exhausted.contains(&policy_id) {
                    continue;
                }
                let Some(expected_nonce) = expected_nonces.get_mut(&policy_id) else {
                    exhausted.insert(policy_id);
                    continue;
                };

                let next = self.by_policy_nonce.get(&policy_id).and_then(|by_nonce| {
                    by_nonce
                        .range(*expected_nonce..)
                        .next()
                        .map(|(nonce, intent_id)| (*nonce, *intent_id))
                });
                let Some((nonce, candidate_intent_id)) = next else {
                    exhausted.insert(policy_id);
                    continue;
                };
                if nonce > *expected_nonce {
                    exhausted.insert(policy_id);
                    continue;
                }
                if excluded_intents.contains(&candidate_intent_id) {
                    stale.push(candidate_intent_id);
                    *expected_nonce = expected_nonce.saturating_add(1);
                    made_progress = true;
                    continue;
                }
                let Some(candidate) = self.by_intent.get(&candidate_intent_id) else {
                    stale.push(candidate_intent_id);
                    *expected_nonce = expected_nonce.saturating_add(1);
                    made_progress = true;
                    continue;
                };
                if intent_expires_at_ms(&candidate.signed_intent.intent) <= timestamp_ms {
                    stale.push(candidate_intent_id);
                    *expected_nonce = expected_nonce.saturating_add(1);
                    made_progress = true;
                    continue;
                }

                selected.push(candidate_intent_id);
                *expected_nonce = expected_nonce.saturating_add(1);
                last_selected_policy = Some(policy_id);
                made_progress = true;
            }

            if !made_progress {
                break;
            }
        }

        if let Some(last_selected_policy) = last_selected_policy {
            let next_cursor = policy_ids
                .iter()
                .position(|policy_id| *policy_id == last_selected_policy)
                .map(|index| policy_ids[(index + 1) % policy_ids.len()]);
            self.next_policy_cursor = next_cursor;
        }

        for intent_id in stale {
            if self.remove_intent(&intent_id).is_some() {
                self.dropped_stale_total = self.dropped_stale_total.saturating_add(1);
            }
        }

        let mut drained = Vec::with_capacity(selected.len());
        for intent_id in selected {
            if let Some(intent) = self.remove_intent(&intent_id) {
                self.drained_total = self.drained_total.saturating_add(1);
                drained.push(intent.signed_intent);
            }
        }
        drained
    }

    pub(crate) fn clear(&mut self) {
        self.by_intent.clear();
        self.by_policy_nonce.clear();
        self.insertion_order.clear();
        self.next_policy_cursor = None;
    }

    pub(crate) fn stats(&self) -> MahiPreAdmittedPoolStats {
        MahiPreAdmittedPoolStats {
            items: self.by_intent.len(),
            accepted_total: self.accepted_total,
            duplicate_total: self.duplicate_total,
            drained_total: self.drained_total,
            dropped_oldest_total: self.dropped_oldest_total,
            dropped_stale_total: self.dropped_stale_total,
        }
    }

    fn remove_intent(&mut self, intent_id: &IntentId) -> Option<MahiPreAdmittedIntent> {
        let intent = self.by_intent.remove(intent_id)?;
        if let Entry::Occupied(mut entry) = self
            .by_policy_nonce
            .entry(intent.signed_intent.intent.policy_id)
        {
            entry.get_mut().remove(&intent.signed_intent.intent.nonce);
            if entry.get().is_empty() {
                entry.remove();
            }
        }
        Some(intent)
    }
}
