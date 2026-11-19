use super::*;

impl SingleNodeEngine {
    pub(crate) fn rebuild_proposer_frontier(&mut self) {
        self.proposer_frontier
            .rebuild_all(&self.intent_mempool, &self.policy_store);
    }

    pub(crate) fn refresh_proposer_frontier_for_block(&mut self, body: &BlockBody) {
        self.proposer_frontier
            .refresh_for_block(&self.intent_mempool, &self.policy_store, body);
    }

    pub(crate) fn refresh_proposer_frontier_for_rejected(&mut self, rejected: &[RejectedIntent]) {
        self.proposer_frontier.refresh_for_rejected(
            &self.intent_mempool,
            &self.policy_store,
            rejected,
        );
    }

    pub(crate) fn refresh_proposer_frontier_for_signed_intents<'a, I>(&mut self, intents: I)
    where
        I: IntoIterator<Item = &'a SignedIntent>,
    {
        self.proposer_frontier.refresh_for_signed_intents(
            &self.intent_mempool,
            &self.policy_store,
            intents,
        );
    }

    pub(crate) fn track_proposer_frontier_admitted_intents<'a, I>(&mut self, intents: I)
    where
        I: IntoIterator<Item = &'a SignedIntent>,
    {
        for signed_intent in intents {
            self.proposer_frontier.on_intent_admitted(
                &self.intent_mempool,
                &self.policy_store,
                signed_intent.intent.policy_id,
                signed_intent.intent.nonce,
            );
        }
    }
}
