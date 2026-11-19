use crate::MandateId;
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{BlockBody, IntentId, SignedIntent};
use kombat_chain_matching::SingleMarketBook;
use kombat_chain_policy::PolicyStore;
use kombat_chain_streams::{IntentRejectionReason, OrderRejectionReason, RuntimeEventKind};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DirectIntentMempool {
    pub(crate) intents_by_order: BTreeMap<IntentOrderKey, SignedIntent>,
    pub(crate) known_intents: BTreeSet<IntentId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConditionalIntentPool {
    pub(crate) time_after_by_order: BTreeMap<ActivationOrderKey, SignedIntent>,
    pub(crate) known_intents: BTreeSet<IntentId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct IntentOrderKey {
    pub(crate) policy_id: u64,
    pub(crate) nonce: u64,
    pub(crate) intent_id: IntentId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub(crate) struct ActivationOrderKey {
    pub(crate) not_before_ms: u64,
    pub(crate) order_key: IntentOrderKey,
}

pub(crate) struct BlockAssemblyPlan {
    pub(crate) body: BlockBody,
    pub(crate) rejected: Vec<RejectedIntent>,
    pub(crate) policy_store_after: PolicyStore,
    pub(crate) order_book_overlays: BTreeMap<u64, SingleMarketBook>,
    pub(crate) resting_order_mandate_overlays: BTreeMap<(u64, u64, u64), MandateId>,
    pub(crate) collateral_overlays: BTreeMap<(u64, u64), i128>,
    pub(crate) committed_events: Vec<RuntimeEventKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ProposerFrontierStats {
    pub direct_mempool_items: usize,
    pub conditional_mempool_items: usize,
    pub tracked_policy_count: usize,
    pub tracked_ready_items: usize,
    pub largest_tracked_policy_ready_items: usize,
    pub stale_policy_count: usize,
    pub stale_ready_items: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProposerReadyFrontier {
    pub(crate) contiguous_by_policy: BTreeMap<u64, Vec<IntentOrderKey>>,
}

#[derive(Debug)]
pub(crate) struct RejectedIntent {
    pub(crate) signed_intent: SignedIntent,
    pub(crate) reason: IntentRejectionReason,
    pub(crate) order_reason: Option<OrderRejectionReason>,
}
