use super::*;

impl SingleNodeEngine {
    pub fn snapshot(&self) -> SingleNodeEngineSnapshot {
        SingleNodeEngineSnapshot {
            configured_validator_verifiers: self.configured_validator_verifiers.clone(),
            validator_set: self.validator_set.clone(),
            validator_verifiers: self.validator_verifiers.clone(),
            consensus_signing_keys: self.consensus_signing_keys.clone(),
            pacemaker_config: self.pacemaker_config.clone(),
            pacemaker: self.pacemaker.clone(),
            chain: self.chain.clone(),
            executor: self.executor.clone(),
            account_store: self.account_store.clone(),
            policy_store: self.policy_store.clone(),
            order_books: self.order_books.clone(),
            resting_order_mandates: self.resting_order_mandates.clone(),
            markets: self.markets.clone(),
            market_marks: self.market_marks.clone(),
            market_index_prices: self.market_index_prices.clone(),
            oracle_feed_submissions: self.oracle_feed_submissions.clone(),
            mandates: self.mandates.clone(),
            mandates_by_agent: self.mandates_by_agent.clone(),
            mandate_action_nonces: self.mandate_action_nonces.clone(),
            mandate_realized_pnl: self.mandate_realized_pnl.clone(),
            mandate_daily_pnl_baselines: self.mandate_daily_pnl_baselines.clone(),
            account_operators: self.account_operators.clone(),
            agent_identities: self.agent_identities.clone(),
            agent_identity_tombstones: self.agent_identity_tombstones.clone(),
            identity_action_nonces: self.identity_action_nonces.clone(),
            identity_attestor_nonces: self.identity_attestor_nonces.clone(),
            agent_attestations: self.agent_attestations.clone(),
            agent_attestations_by_account: self.agent_attestations_by_account.clone(),
            tee_attestations_by_vendor_root: self.tee_attestations_by_vendor_root.clone(),
            tee_vendor_roots: self.tee_vendor_roots.clone(),
            tee_vendor_cap_profiles: self.tee_vendor_cap_profiles.clone(),
            market_maker_programs: self.market_maker_programs.clone(),
            market_maker_enrollments: self.market_maker_enrollments.clone(),
            live_market_maker_bonds_micro_usdc: self.live_market_maker_bonds_micro_usdc,
            processed_market_maker_program_actions: self
                .processed_market_maker_program_actions
                .clone(),
            pending_market_maker_program_actions: self.pending_market_maker_program_actions.clone(),
            pending_market_maker_program_action_order: self
                .pending_market_maker_program_action_order
                .clone(),
            processed_tee_governance_actions: self.processed_tee_governance_actions.clone(),
            pending_tee_governance_actions: self.pending_tee_governance_actions.clone(),
            pending_tee_governance_action_order: self.pending_tee_governance_action_order.clone(),
            scheduled_tee_vendor_cap_profile_updates: self
                .scheduled_tee_vendor_cap_profile_updates
                .clone(),
            processed_real_capital_governance_actions: self
                .processed_real_capital_governance_actions
                .clone(),
            pending_real_capital_governance_actions: self
                .pending_real_capital_governance_actions
                .clone(),
            pending_real_capital_governance_action_order: self
                .pending_real_capital_governance_action_order
                .clone(),
            pending_faucet_grants: self.pending_faucet_grants.clone(),
            pending_faucet_grant_order: self.pending_faucet_grant_order.clone(),
            delegations: self.delegations.clone(),
            delegation_shares: self.delegation_shares.clone(),
            real_capital_listings: self.real_capital_listings.clone(),
            real_capital_listings_by_agent: self.real_capital_listings_by_agent.clone(),
            vault_deposited_micro_usdc: self.vault_deposited_micro_usdc.clone(),
            vault_principal_deposited_micro_usdc: self.vault_principal_deposited_micro_usdc.clone(),
            delegation_compensation_claims: self.delegation_compensation_claims.clone(),
            delegation_termination_payout_weights: self
                .delegation_termination_payout_weights
                .clone(),
            real_capital_bond_escrow_micro_usdc: self.real_capital_bond_escrow_micro_usdc.clone(),
            real_capital_compensation_escrow_micro_usdc: self
                .real_capital_compensation_escrow_micro_usdc,
            real_capital_insurance_wallet_micro_usdc: self.real_capital_insurance_wallet_micro_usdc,
            real_capital_insurance_fund_micro_usdc: self.real_capital_insurance_fund_micro_usdc,
            protocol_delegation_cap: self.protocol_delegation_cap,
            listing_control_state: self.listing_control_state,
            redemption_requests: self.redemption_requests.clone(),
            processed_delegation_actions: self.processed_delegation_actions.clone(),
            pending_delegation_actions: self.pending_delegation_actions.clone(),
            pending_delegation_action_order: self.pending_delegation_action_order.clone(),
            market_oracle_config: self.market_oracle_config.clone(),
            market_risk: self.market_risk.clone(),
            market_fees: self.market_fees.clone(),
            paused_markets: self.paused_markets.clone(),
            market_funding_index: self.market_funding_index.clone(),
            market_last_funding_ms: self.market_last_funding_ms.clone(),
            insurance_funds: self.insurance_funds.clone(),
            market_bad_debt_balances: self.market_bad_debt_balances.clone(),
            account_market_bad_debt: self.account_market_bad_debt.clone(),
            fee_vaults: self.fee_vaults.clone(),
            validator_reward_pools: self.validator_reward_pools.clone(),
            market_maker_rebate_pools: self.market_maker_rebate_pools.clone(),
            treasury_balances: self.treasury_balances.clone(),
            frozen_accounts: self.frozen_accounts.clone(),
            under_maintenance_accounts: self.under_maintenance_accounts.clone(),
            liquidation_escalation: self.liquidation_escalation.clone(),
            isolated_collateral: self.isolated_collateral.clone(),
            positions: self.positions.clone(),
            intent_mempool: self.intent_mempool.clone(),
            conditional_pool: self.conditional_pool.clone(),
            runtime_events: self.runtime_events.clone(),
            evm_transaction_receipts: self.evm_transaction_receipts.clone(),
            next_runtime_event_seq: self.next_runtime_event_seq,
            telemetry: self.telemetry.clone(),
            suppress_fill_bankruptcy_reconcile: self.suppress_fill_bankruptcy_reconcile,
        }
    }

    pub fn from_snapshot(snapshot: SingleNodeEngineSnapshot) -> Self {
        let mut engine = Self {
            configured_validator_verifiers: snapshot.configured_validator_verifiers,
            validator_set: snapshot.validator_set,
            validator_verifiers: snapshot.validator_verifiers,
            consensus_signing_keys: snapshot.consensus_signing_keys,
            pacemaker_config: snapshot.pacemaker_config,
            pacemaker: snapshot.pacemaker,
            chain: snapshot.chain,
            executor: snapshot.executor,
            account_store: snapshot.account_store,
            policy_store: snapshot.policy_store,
            order_books: snapshot.order_books,
            resting_order_mandates: snapshot.resting_order_mandates,
            markets: snapshot.markets,
            market_marks: snapshot.market_marks,
            market_index_prices: snapshot.market_index_prices,
            oracle_feed_submissions: snapshot.oracle_feed_submissions,
            mandates: snapshot.mandates,
            mandates_by_agent: snapshot.mandates_by_agent,
            mandate_action_nonces: snapshot.mandate_action_nonces,
            mandate_fill_timestamps: BTreeMap::new(),
            mandate_objective_event_cache: BTreeMap::new(),
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
            scheduled_tee_vendor_cap_profile_updates: snapshot
                .scheduled_tee_vendor_cap_profile_updates,
            processed_real_capital_governance_actions: snapshot
                .processed_real_capital_governance_actions,
            pending_real_capital_governance_actions: snapshot
                .pending_real_capital_governance_actions,
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
            real_capital_insurance_wallet_micro_usdc: snapshot
                .real_capital_insurance_wallet_micro_usdc,
            real_capital_insurance_fund_micro_usdc: snapshot.real_capital_insurance_fund_micro_usdc,
            protocol_delegation_cap: snapshot.protocol_delegation_cap,
            listing_control_state: snapshot.listing_control_state,
            redemption_requests: snapshot.redemption_requests,
            processed_delegation_actions: snapshot.processed_delegation_actions,
            pending_delegation_actions: snapshot.pending_delegation_actions,
            pending_delegation_action_order: snapshot.pending_delegation_action_order,
            pending_open_delegation_index: BTreeMap::new(),
            pending_open_delegation_agent_index: BTreeMap::new(),
            pending_open_delegation_order: BTreeSet::new(),
            pending_finalize_delegation_index: BTreeMap::new(),
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
            direct_expiry_purged_through_ms: 0,
            conditional_expiry_purged_through_ms: 0,
            proposer_frontier: ProposerReadyFrontier::default(),
            runtime_events: snapshot.runtime_events,
            block_event_index: BTreeMap::new(),
            equity_history: BTreeMap::new(),
            latest_equity_sample_by_account: BTreeMap::new(),
            mandate_conformance_history: BTreeMap::new(),
            latest_conformance_sample_by_mandate: BTreeMap::new(),
            evm_transaction_receipts: snapshot.evm_transaction_receipts,
            next_runtime_event_seq: snapshot.next_runtime_event_seq,
            telemetry: snapshot.telemetry,
            suppress_fill_bankruptcy_reconcile: snapshot.suppress_fill_bankruptcy_reconcile,
        };
        engine.rebuild_block_event_index();
        engine.rebuild_proposer_frontier();
        engine.rebuild_mandate_derived_indexes();
        engine.rebuild_pending_delegation_action_indexes();
        engine.sync_tee_vendor_roots_execution_mirror();
        engine
    }

    pub(crate) fn persisted_snapshot(&self) -> PersistedSingleNodeEngineSnapshot {
        let mut pacemaker = self.pacemaker.clone();
        if let Some(head) = self.chain.last() {
            pacemaker.highest_qc = synthetic_mahi_quorum_certificate(head);
            pacemaker.current_round = pacemaker
                .current_round
                .max(head.header.round.saturating_add(1));
        }
        PersistedSingleNodeEngineSnapshot {
            configured_validator_verifiers: self.configured_validator_verifiers.clone(),
            validator_set: self.validator_set.clone(),
            validator_verifiers: self.validator_verifiers.clone(),
            consensus_signing_keys: self.consensus_signing_keys.clone(),
            pacemaker_config: self.pacemaker_config.clone(),
            pacemaker,
            chain: self.chain.clone(),
            committed_state_root: self.executor.state_store().committed_root(),
            committed_entries: self
                .executor
                .state_store()
                .committed_entries()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            quarantined_blocks: self
                .executor
                .quarantined_blocks()
                .iter()
                .map(|(hash, block)| (*hash, PersistedQuarantinedBlock::from_runtime(block)))
                .collect(),
            account_store: self.account_store.clone(),
            policy_store: self.policy_store.clone(),
            order_books: self.order_books.clone(),
            resting_order_mandates: self.resting_order_mandates.clone(),
            markets: self.markets.clone(),
            market_marks: self.market_marks.clone(),
            market_index_prices: self.market_index_prices.clone(),
            oracle_feed_submissions: self.oracle_feed_submissions.clone(),
            mandates: self.mandates.clone(),
            mandates_by_agent: self.mandates_by_agent.clone(),
            mandate_action_nonces: self.mandate_action_nonces.clone(),
            mandate_realized_pnl: self.mandate_realized_pnl.clone(),
            mandate_daily_pnl_baselines: self.mandate_daily_pnl_baselines.clone(),
            account_operators: self.account_operators.clone(),
            agent_identities: self.agent_identities.clone(),
            agent_identity_tombstones: self.agent_identity_tombstones.clone(),
            identity_action_nonces: self.identity_action_nonces.clone(),
            identity_attestor_nonces: self.identity_attestor_nonces.clone(),
            agent_attestations: self.agent_attestations.clone(),
            agent_attestations_by_account: self.agent_attestations_by_account.clone(),
            tee_attestations_by_vendor_root: self.tee_attestations_by_vendor_root.clone(),
            tee_vendor_roots: self.tee_vendor_roots.clone(),
            tee_vendor_cap_profiles: self.tee_vendor_cap_profiles.clone(),
            market_maker_programs: self.market_maker_programs.clone(),
            market_maker_enrollments: self.market_maker_enrollments.clone(),
            live_market_maker_bonds_micro_usdc: self.live_market_maker_bonds_micro_usdc,
            processed_market_maker_program_actions: self
                .processed_market_maker_program_actions
                .clone(),
            pending_market_maker_program_actions: self.pending_market_maker_program_actions.clone(),
            pending_market_maker_program_action_order: self
                .pending_market_maker_program_action_order
                .clone(),
            processed_tee_governance_actions: self.processed_tee_governance_actions.clone(),
            pending_tee_governance_actions: self.pending_tee_governance_actions.clone(),
            pending_tee_governance_action_order: self.pending_tee_governance_action_order.clone(),
            scheduled_tee_vendor_cap_profile_updates: self
                .scheduled_tee_vendor_cap_profile_updates
                .clone(),
            processed_real_capital_governance_actions: self
                .processed_real_capital_governance_actions
                .clone(),
            pending_real_capital_governance_actions: self
                .pending_real_capital_governance_actions
                .clone(),
            pending_real_capital_governance_action_order: self
                .pending_real_capital_governance_action_order
                .clone(),
            pending_faucet_grants: self.pending_faucet_grants.clone(),
            pending_faucet_grant_order: self.pending_faucet_grant_order.clone(),
            delegations: self.delegations.clone(),
            delegation_shares: self.delegation_shares.clone(),
            real_capital_listings: self.real_capital_listings.clone(),
            real_capital_listings_by_agent: self.real_capital_listings_by_agent.clone(),
            vault_deposited_micro_usdc: self.vault_deposited_micro_usdc.clone(),
            vault_principal_deposited_micro_usdc: self.vault_principal_deposited_micro_usdc.clone(),
            delegation_compensation_claims: self.delegation_compensation_claims.clone(),
            delegation_termination_payout_weights: self
                .delegation_termination_payout_weights
                .clone(),
            real_capital_bond_escrow_micro_usdc: self.real_capital_bond_escrow_micro_usdc.clone(),
            real_capital_compensation_escrow_micro_usdc: self
                .real_capital_compensation_escrow_micro_usdc,
            real_capital_insurance_wallet_micro_usdc: self.real_capital_insurance_wallet_micro_usdc,
            real_capital_insurance_fund_micro_usdc: self.real_capital_insurance_fund_micro_usdc,
            protocol_delegation_cap: self.protocol_delegation_cap,
            listing_control_state: self.listing_control_state,
            redemption_requests: self.redemption_requests.clone(),
            processed_delegation_actions: self.processed_delegation_actions.clone(),
            pending_delegation_actions: self.pending_delegation_actions.clone(),
            pending_delegation_action_order: self.pending_delegation_action_order.clone(),
            market_oracle_config: self.market_oracle_config.clone(),
            market_risk: self.market_risk.clone(),
            market_fees: self.market_fees.clone(),
            paused_markets: self.paused_markets.clone(),
            market_funding_index: self.market_funding_index.clone(),
            market_last_funding_ms: self.market_last_funding_ms.clone(),
            insurance_funds: self.insurance_funds.clone(),
            market_bad_debt_balances: self.market_bad_debt_balances.clone(),
            account_market_bad_debt: self.account_market_bad_debt.clone(),
            fee_vaults: self.fee_vaults.clone(),
            validator_reward_pools: self.validator_reward_pools.clone(),
            market_maker_rebate_pools: self.market_maker_rebate_pools.clone(),
            treasury_balances: self.treasury_balances.clone(),
            frozen_accounts: self.frozen_accounts.clone(),
            under_maintenance_accounts: self.under_maintenance_accounts.clone(),
            liquidation_escalation: self.liquidation_escalation.clone(),
            isolated_collateral: self.isolated_collateral.clone(),
            positions: self.positions.clone(),
            intent_mempool: self.intent_mempool.clone(),
            conditional_pool: self.conditional_pool.clone(),
            runtime_events: self.runtime_events.clone(),
            evm_transaction_receipts: self.evm_transaction_receipts.clone(),
            next_runtime_event_seq: self.next_runtime_event_seq,
            telemetry: self.telemetry.clone(),
            suppress_fill_bankruptcy_reconcile: self.suppress_fill_bankruptcy_reconcile,
        }
    }

    pub(crate) fn from_persisted_snapshot(snapshot: PersistedSingleNodeEngineSnapshot) -> Self {
        let state_store = if snapshot.committed_entries.is_empty() {
            InMemoryStateStore::with_committed_root(snapshot.committed_state_root)
        } else {
            InMemoryStateStore::from_entries(snapshot.committed_entries.clone())
        };
        let executor = BlockExecutor::from_parts(
            snapshot
                .chain
                .first()
                .map(|block| block.header.chain_id)
                .unwrap_or(0),
            state_store,
            snapshot
                .quarantined_blocks
                .into_iter()
                .map(|(hash, block)| (hash, block.into_runtime()))
                .collect(),
        );

        let mut engine = Self {
            configured_validator_verifiers: snapshot.configured_validator_verifiers,
            validator_set: snapshot.validator_set,
            validator_verifiers: snapshot.validator_verifiers,
            consensus_signing_keys: snapshot.consensus_signing_keys,
            pacemaker_config: snapshot.pacemaker_config,
            pacemaker: snapshot.pacemaker,
            chain: snapshot.chain,
            executor,
            account_store: snapshot.account_store,
            policy_store: snapshot.policy_store,
            order_books: snapshot.order_books,
            resting_order_mandates: snapshot.resting_order_mandates,
            markets: snapshot.markets,
            market_marks: snapshot.market_marks,
            market_index_prices: snapshot.market_index_prices,
            oracle_feed_submissions: snapshot.oracle_feed_submissions,
            mandates: snapshot.mandates,
            mandates_by_agent: snapshot.mandates_by_agent,
            mandate_action_nonces: snapshot.mandate_action_nonces,
            mandate_fill_timestamps: BTreeMap::new(),
            mandate_objective_event_cache: BTreeMap::new(),
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
            scheduled_tee_vendor_cap_profile_updates: snapshot
                .scheduled_tee_vendor_cap_profile_updates,
            processed_real_capital_governance_actions: snapshot
                .processed_real_capital_governance_actions,
            pending_real_capital_governance_actions: snapshot
                .pending_real_capital_governance_actions,
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
            real_capital_insurance_wallet_micro_usdc: snapshot
                .real_capital_insurance_wallet_micro_usdc,
            real_capital_insurance_fund_micro_usdc: snapshot.real_capital_insurance_fund_micro_usdc,
            protocol_delegation_cap: snapshot.protocol_delegation_cap,
            listing_control_state: snapshot.listing_control_state,
            redemption_requests: snapshot.redemption_requests,
            processed_delegation_actions: snapshot.processed_delegation_actions,
            pending_delegation_actions: snapshot.pending_delegation_actions,
            pending_delegation_action_order: snapshot.pending_delegation_action_order,
            pending_open_delegation_index: BTreeMap::new(),
            pending_open_delegation_agent_index: BTreeMap::new(),
            pending_open_delegation_order: BTreeSet::new(),
            pending_finalize_delegation_index: BTreeMap::new(),
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
            direct_expiry_purged_through_ms: 0,
            conditional_expiry_purged_through_ms: 0,
            proposer_frontier: ProposerReadyFrontier::default(),
            runtime_events: snapshot.runtime_events,
            block_event_index: BTreeMap::new(),
            equity_history: BTreeMap::new(),
            latest_equity_sample_by_account: BTreeMap::new(),
            mandate_conformance_history: BTreeMap::new(),
            latest_conformance_sample_by_mandate: BTreeMap::new(),
            evm_transaction_receipts: snapshot.evm_transaction_receipts,
            next_runtime_event_seq: snapshot.next_runtime_event_seq,
            telemetry: snapshot.telemetry,
            suppress_fill_bankruptcy_reconcile: snapshot.suppress_fill_bankruptcy_reconcile,
        };
        engine.rebuild_proposer_frontier();
        engine.rebuild_block_event_index();
        engine.rebuild_mandate_derived_indexes();
        engine.rebuild_pending_delegation_action_indexes();
        engine.sync_tee_vendor_roots_execution_mirror();
        engine
    }

    fn rebuild_mandate_derived_indexes(&mut self) {
        let mut fill_timestamps: BTreeMap<MandateId, Vec<u64>> = BTreeMap::new();
        let mut objective_cache: BTreeMap<(MandateId, u16), MandateObjectiveEventState> =
            BTreeMap::new();
        for event in &self.runtime_events {
            match &event.kind {
                RuntimeEventKind::OrderFilled {
                    mandate_id: Some(mandate_id),
                    ..
                } => {
                    fill_timestamps
                        .entry(*mandate_id)
                        .or_default()
                        .push(event.timestamp_ms);
                }
                RuntimeEventKind::MandateObjectiveEvaluated {
                    mandate_id,
                    objective_index,
                    score,
                    conformance_score,
                    pending_objectives,
                    ..
                } => {
                    objective_cache.insert(
                        (*mandate_id, *objective_index),
                        MandateObjectiveEventState {
                            score: *score,
                            conformance_score: *conformance_score,
                            pending_objectives: *pending_objectives,
                        },
                    );
                }
                _ => {}
            }
        }
        for timestamps in fill_timestamps.values_mut() {
            timestamps.sort_unstable();
        }
        self.mandate_fill_timestamps = fill_timestamps;
        self.mandate_objective_event_cache = objective_cache;
        self.prune_all_mandate_fill_timestamps(self.head().header.timestamp_ms);
    }

    pub(crate) fn try_from_persisted_snapshot(
        mut snapshot: PersistedSingleNodeEngineSnapshot,
    ) -> Result<Self, std::io::Error> {
        if snapshot.validator_set.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot requires active validators",
            ));
        }
        if snapshot.chain.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot requires a genesis block",
            ));
        }

        let configured_validator_ids = snapshot
            .configured_validator_verifiers
            .iter()
            .map(|verifier| verifier.validator)
            .collect::<BTreeSet<_>>();
        if configured_validator_ids.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot configured validator set is empty",
            ));
        }
        if configured_validator_ids.len() != snapshot.configured_validator_verifiers.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot configured validator ids are duplicated",
            ));
        }

        let verifier_ids = snapshot
            .validator_verifiers
            .iter()
            .map(|verifier| verifier.validator)
            .collect::<BTreeSet<_>>();
        let local_validator_ids = snapshot
            .validator_set
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if verifier_ids != local_validator_ids {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot verifier set mismatch",
            ));
        }
        if !local_validator_ids.is_subset(&configured_validator_ids) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot local validators missing from configured validator set",
            ));
        }

        let signing_key_ids = snapshot
            .consensus_signing_keys
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        if signing_key_ids != local_validator_ids {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot signing keys do not match validators",
            ));
        }
        for verifier in &snapshot.validator_verifiers {
            let configured_verifier = snapshot
                .configured_validator_verifiers
                .iter()
                .find(|configured| configured.validator == verifier.validator)
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot local verifier missing from configured validator set",
                    )
                })?;
            if configured_verifier.consensus_pubkey != verifier.consensus_pubkey {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot local verifier key mismatch",
                ));
            }
        }
        validate_consensus_key_material(
            &snapshot.validator_verifiers,
            &snapshot.consensus_signing_keys,
            "persisted node snapshot",
        )?;

        let chain_id = snapshot.chain[0].header.chain_id;
        for block in &snapshot.chain {
            if block.header.chain_id != chain_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot chain id mismatch",
                ));
            }
        }
        let chain_validator_verifiers =
            if snapshot.configured_validator_verifiers.len() > snapshot.validator_verifiers.len() {
                &snapshot.configured_validator_verifiers
            } else {
                &snapshot.validator_verifiers
            };
        for window in snapshot.chain.windows(2) {
            let parent = &window[0];
            let block = &window[1];
            let expected_proposer = if chain_validator_verifiers.len() == 1 {
                chain_validator_verifiers[0].validator
            } else {
                block.header.proposer
            };
            validate_persisted_snapshot_block_link(
                block,
                parent,
                chain_validator_verifiers,
                expected_proposer,
                snapshot.pacemaker_config.max_clock_skew_ms,
            )
            .map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "persisted node snapshot block validation failed at height {} parent_height {} parent_ts {} block_ts {}: {error:?}",
                        block.header.height,
                        parent.header.height,
                        parent.header.timestamp_ms,
                        block.header.timestamp_ms,
                    ),
                )
            })?;
        }

        let computed_committed_root =
            InMemoryStateStore::from_entries(snapshot.committed_entries.clone()).committed_root();
        if snapshot.committed_entries.is_empty() {
            let genesis_root = snapshot
                .chain
                .first()
                .expect("checked above")
                .header
                .state_root;
            if snapshot.committed_state_root != computed_committed_root
                && snapshot.committed_state_root != genesis_root
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot committed state root mismatch",
                ));
            }
        } else if computed_committed_root != snapshot.committed_state_root {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot committed state root mismatch",
            ));
        }

        let head = snapshot.chain.last().expect("checked above");
        if head.header.qc.signatures.is_empty() {
            snapshot.pacemaker.highest_qc = synthetic_mahi_quorum_certificate(head);
            snapshot.pacemaker.current_round = snapshot
                .pacemaker
                .current_round
                .max(head.header.round.saturating_add(1));
        }
        let mut direct_ids = BTreeSet::new();
        let mut queued_policy_nonces = BTreeSet::new();
        for (key, signed_intent) in &snapshot.intent_mempool.intents_by_order {
            let queued_intent_id = intent_id(signed_intent);
            if key.intent_id != queued_intent_id
                || key.policy_id != signed_intent.intent.policy_id
                || key.nonce != signed_intent.intent.nonce
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot direct intent key mismatch",
                ));
            }
            if signed_intent.intent.chain_id != head.header.chain_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot direct intent chain mismatch",
                ));
            }
            if intent_not_before_ms(&signed_intent.intent).is_some() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot direct pool contains time-after intent",
                ));
            }
            let committed_next_nonce = snapshot
                .policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot direct intent policy missing",
                    )
                })?;
            if signed_intent.intent.nonce < committed_next_nonce {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot direct intent nonce regression",
                ));
            }
            direct_ids.insert(queued_intent_id);
            if !queued_policy_nonces
                .insert((signed_intent.intent.policy_id, signed_intent.intent.nonce))
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot queued policy nonce duplicate",
                ));
            }
        }
        if direct_ids != snapshot.intent_mempool.known_intents {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot direct known intent set mismatch",
            ));
        }

        let mut conditional_ids = BTreeSet::new();
        for (key, signed_intent) in &snapshot.conditional_pool.time_after_by_order {
            let queued_intent_id = intent_id(signed_intent);
            if key.order_key.intent_id != queued_intent_id
                || key.order_key.policy_id != signed_intent.intent.policy_id
                || key.order_key.nonce != signed_intent.intent.nonce
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot conditional intent key mismatch",
                ));
            }
            if signed_intent.intent.chain_id != head.header.chain_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot conditional intent chain mismatch",
                ));
            }
            let not_before_ms = intent_not_before_ms(&signed_intent.intent).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot conditional intent missing time-after predicate",
                )
            })?;
            if key.not_before_ms != not_before_ms {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot conditional activation mismatch",
                ));
            }
            let committed_next_nonce = snapshot
                .policy_store
                .next_intent_nonce(signed_intent.intent.policy_id)
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot conditional intent policy missing",
                    )
                })?;
            if signed_intent.intent.nonce < committed_next_nonce {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot conditional intent nonce regression",
                ));
            }
            conditional_ids.insert(queued_intent_id);
            if !queued_policy_nonces
                .insert((signed_intent.intent.policy_id, signed_intent.intent.nonce))
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot queued policy nonce duplicate",
                ));
            }
        }
        if conditional_ids != snapshot.conditional_pool.known_intents {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot conditional known intent set mismatch",
            ));
        }
        let mut expected_delegation_action_order = BTreeSet::new();
        for (action_id, pending) in &snapshot.pending_delegation_actions {
            if pending.action_id != *action_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation action key mismatch",
                ));
            }
            if snapshot
                .processed_delegation_actions
                .contains_key(action_id)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation action pending processed",
                ));
            }
            if pending.fingerprint != delegation_action_fingerprint(&pending.action) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation action fingerprint mismatch",
                ));
            }
            expected_delegation_action_order.insert(PendingDelegationActionOrder {
                submitted_at_ms: pending.submitted_at_ms,
                action_id: *action_id,
            });
        }
        if expected_delegation_action_order != snapshot.pending_delegation_action_order {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot delegation action order mismatch",
            ));
        }

        let mut expected_mm_program_action_order = BTreeSet::new();
        for (action_id, pending) in &snapshot.pending_market_maker_program_actions {
            if pending.action_id != *action_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market-maker program action key mismatch",
                ));
            }
            if snapshot
                .processed_market_maker_program_actions
                .contains_key(action_id)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market-maker program action pending processed",
                ));
            }
            if pending.fingerprint != market_maker_program_action_fingerprint(&pending.action) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market-maker program action fingerprint mismatch",
                ));
            }
            expected_mm_program_action_order.insert(PendingMarketMakerProgramActionOrder {
                submitted_at_ms: pending.submitted_at_ms,
                action_id: *action_id,
            });
        }
        if expected_mm_program_action_order != snapshot.pending_market_maker_program_action_order {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market-maker program action order mismatch",
            ));
        }

        let mut expected_faucet_order = BTreeSet::new();
        for (address, pending) in &snapshot.pending_faucet_grants {
            if pending.address != *address {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot faucet grant key mismatch",
                ));
            }
            expected_faucet_order.insert(pending_faucet_grant_order(pending));
        }
        if expected_faucet_order != snapshot.pending_faucet_grant_order {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot faucet grant order mismatch",
            ));
        }

        for (hash, block) in &snapshot.quarantined_blocks {
            if *hash != block.block_hash {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot quarantined block hash mismatch",
                ));
            }
            if block.height == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot quarantines genesis height",
                ));
            }
            if let Some(committed_block) = snapshot.chain.get(block.height as usize) {
                if *hash == block_hash(committed_block) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot quarantines committed block",
                    ));
                }
            }
        }
        for (expected_seq, event) in snapshot.runtime_events.iter().enumerate() {
            let expected_seq = expected_seq as u64;
            if event.seq != expected_seq {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot runtime event sequence mismatch",
                ));
            }
            if let Some(block_height) = event.block_height {
                if block_height > head.header.height {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot runtime event block height exceeds head",
                    ));
                }
            }
        }
        let expected_next_seq = snapshot.runtime_events.len() as u64;
        if snapshot.next_runtime_event_seq != expected_next_seq {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot next runtime event sequence mismatch",
            ));
        }
        let event_stream = EventStream::new(&snapshot.runtime_events);
        for queued_intent_id in &direct_ids {
            if let Some(event) = event_stream.latest_intent_event(*queued_intent_id) {
                match event.kind.intent_status_stage() {
                    Some(IntentStatusStage::AcceptedDirect)
                    | Some(IntentStatusStage::Promoted)
                    | None => {}
                    Some(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "persisted node snapshot direct queued intent event mismatch",
                        ));
                    }
                }
            }
        }
        for queued_intent_id in &conditional_ids {
            if let Some(event) = event_stream.latest_intent_event(*queued_intent_id) {
                match event.kind.intent_status_stage() {
                    Some(IntentStatusStage::AcceptedConditional) | None => {}
                    Some(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "persisted node snapshot conditional queued intent event mismatch",
                        ));
                    }
                }
            }
        }
        if snapshot.pacemaker.highest_qc.block_hash != block_hash(head)
            || snapshot.pacemaker.highest_qc.view != head.header.round
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot highest qc mismatch",
            ));
        }
        if snapshot.pacemaker.current_round < head.header.round.saturating_add(1) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot round regression",
            ));
        }
        let expected_peer_count = snapshot
            .configured_validator_verifiers
            .len()
            .saturating_sub(1);
        snapshot.telemetry.normalize_snapshot_timestamps();
        if head.header.qc.signatures.is_empty() {
            snapshot.telemetry.repair_synthetic_mahi_snapshot(
                &snapshot.pacemaker,
                head,
                expected_peer_count,
            );
        }
        let legacy_peer_count = snapshot.validator_set.len().saturating_sub(1);
        if expected_peer_count != legacy_peer_count
            && snapshot.telemetry.metrics().peer_count == legacy_peer_count
            && snapshot
                .telemetry
                .logs()
                .iter()
                .all(|entry| entry.peer_count == legacy_peer_count)
        {
            snapshot.telemetry.rewrite_peer_count(expected_peer_count);
        }
        snapshot.telemetry.validate_snapshot(
            &snapshot.pacemaker,
            head,
            expected_peer_count,
            snapshot.validator_set.first().copied(),
        )?;

        let engine = Self::from_persisted_snapshot(snapshot);
        engine.validate_persisted_order_books()?;
        engine.validate_persisted_positions()?;
        engine.validate_persisted_market_state()?;
        engine.validate_persisted_mandates()?;
        engine.validate_persisted_agent_identities()?;
        engine.validate_persisted_agent_attestations()?;
        engine.validate_persisted_tee_vendor_state()?;
        engine.validate_persisted_real_capital_listings()?;
        engine.validate_persisted_identity_action_nonces()?;
        engine.validate_persisted_risk_state()?;
        Ok(engine)
    }

    pub fn snapshot_bytes(&self) -> Vec<u8> {
        borsh::to_vec(&self.persisted_snapshot()).expect("persisted node snapshot should serialize")
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        if bytes.starts_with(&kombat_chain_state_db::LOGICAL_SNAPSHOT_MAGIC) {
            return crate::persistence::load_engine_from_logical_snapshot(bytes)
                .map(|(engine, _meta)| engine)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error));
        }
        let snapshot = PersistedSingleNodeEngineSnapshot::try_from_slice(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        Self::try_from_persisted_snapshot(snapshot)
    }

    pub fn save_snapshot_file(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        write_snapshot_file(path, &self.snapshot_bytes())
    }

    pub fn load_snapshot_file(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let bytes = fs::read(path)?;
        Self::from_snapshot_bytes(&bytes)
    }

    /// Hydrate and validate an RFC 042 durable backend with the same
    /// path used by node startup. Migration tooling uses this against
    /// staged backends before they are allowed to replace live state.
    pub fn from_state_backend(
        backend: &kombat_chain_state_db::AnyStateBackend,
    ) -> Result<Self, std::io::Error> {
        crate::persistence::hydrate_engine_from_backend(backend)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    pub fn repair_market_custody_in_state_backend(
        backend: &kombat_chain_state_db::AnyStateBackend,
        dry_run: bool,
    ) -> Result<MarketCustodyRepairReport, std::io::Error> {
        crate::persistence::repair_market_custody_in_backend(backend, dry_run)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    pub fn audit_market_custody_in_state_backend(
        backend: &kombat_chain_state_db::AnyStateBackend,
        top_rows: usize,
    ) -> Result<MarketCustodyAuditReport, std::io::Error> {
        crate::persistence::audit_market_custody_in_backend(backend, top_rows)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    pub fn repair_isolated_collateral_in_state_backend(
        backend: &kombat_chain_state_db::AnyStateBackend,
        dry_run: bool,
        targets: &[IsolatedCollateralRepairTarget],
        top_rows: usize,
    ) -> Result<IsolatedCollateralRepairReport, std::io::Error> {
        crate::persistence::repair_isolated_collateral_in_backend(
            backend, dry_run, targets, top_rows,
        )
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    /// Persist this engine into a fresh RFC 042 durable backend.
    ///
    /// Startup uses this after bootstrapping from genesis or a snapshot
    /// when the selected backend has no committed state yet.
    pub fn initialize_state_backend(
        &self,
        backend: &kombat_chain_state_db::AnyStateBackend,
        timestamp_ms: u64,
    ) -> Result<kombat_chain_state_db::CommitMeta, std::io::Error> {
        crate::persistence::initialize_fresh_backend_from_engine(backend, self, timestamp_ms)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    /// Save a Phase 3 logical snapshot to `path`.
    ///
    /// Unlike `save_snapshot_file` (legacy borsh blob of
    /// `PersistedSingleNodeEngineSnapshot`), this writes the canonical
    /// RFC 042 logical-snapshot format: magic prefix, schema version,
    /// CommitMeta header, then borsh-encoded namespace entries. The
    /// resulting file is the same shape any backend produces from
    /// `state_db::export`, so it can be loaded into either backend.
    ///
    /// `timestamp_ms` is the wall-clock time anchored in the
    /// CommitMeta header — typically `SystemTime::now()` from the RPC
    /// service path.
    pub fn save_logical_snapshot_file(
        &self,
        path: impl AsRef<Path>,
        timestamp_ms: u64,
    ) -> Result<(), std::io::Error> {
        let mut buf: Vec<u8> = Vec::new();
        crate::persistence::save_engine_logical_snapshot(self, timestamp_ms, &mut buf)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        write_snapshot_file(path, &buf)
    }

    /// Load a Phase 3 logical snapshot from `path`, replacing `self`.
    ///
    /// Refuses to load over a populated engine (head height > 0)
    /// unless `force_reset` is true. This is the RFC 042
    /// "load-refuses-populated-backend-without-reset" guarantee:
    /// operators cannot accidentally clobber durable state by
    /// pointing `kombat_loadSnapshot` at a running node.
    pub fn load_logical_snapshot_file(
        &mut self,
        path: impl AsRef<Path>,
        force_reset: bool,
    ) -> Result<(), std::io::Error> {
        let bytes = fs::read(path)?;
        crate::persistence::load_engine_logical_snapshot(self, bytes.as_slice(), force_reset)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    }
}
