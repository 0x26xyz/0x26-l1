use super::*;

impl SingleNodeEngine {
    pub(super) fn prepared_commit_from_plan(
        &self,
        prepared_block: PreparedBlock<InMemoryCheckpoint>,
        plan: BlockAssemblyPlan,
    ) -> PreparedCommittedBlock {
        PreparedCommittedBlock {
            prepared_block,
            policy_store: plan.policy_store_after,
            order_book_overlays: plan.order_book_overlays,
            resting_order_mandate_overlays: plan.resting_order_mandate_overlays,
            collateral_overlays: plan.collateral_overlays,
            rejected_intents: plan.rejected,
            committed_events: plan.committed_events,
        }
    }

    pub(super) fn install_prepared_block_state(
        &mut self,
        prepared: PreparedCommittedBlock,
        produced_qc: QuorumCertificate,
    ) -> Result<&Block, NodeError> {
        let PreparedCommittedBlock {
            prepared_block,
            policy_store,
            order_book_overlays,
            resting_order_mandate_overlays,
            collateral_overlays,
            rejected_intents,
            committed_events,
        } = prepared;
        let pre_block_oi = self.market_open_interest_snapshot();
        let block = prepared_block.block.clone();
        let evm_transactions = prepared_block.result.evm_transactions.clone();
        self.executor
            .commit_prepared_block(prepared_block)
            .map_err(NodeError::Execution)?;
        // EVM custody settlement commits before the Rust-side collateral
        // overlays install. If a crash lands between the two phases, RFC 053's
        // snapshot validator rejects the resulting custody/ledger divergence on
        // restart rather than silently loading torn state.
        self.policy_store = policy_store;
        for (market_id, order_book) in order_book_overlays {
            self.order_books.insert(market_id, order_book);
        }
        self.resting_order_mandates = resting_order_mandate_overlays;
        self.apply_collateral_overlays(collateral_overlays);
        self.intent_mempool.remove_block(&block.body);
        self.remove_pending_faucet_grants_in_block(&block.body);
        self.remove_pending_delegation_actions_in_block(&block.body);
        self.remove_pending_market_maker_program_actions_in_block(&block.body);
        self.remove_pending_tee_governance_actions_in_block(&block.body);
        self.remove_pending_real_capital_governance_actions_in_block(&block.body);
        self.intent_mempool.remove_rejected(&rejected_intents);
        self.refresh_proposer_frontier_for_block(&block.body);
        self.refresh_proposer_frontier_for_rejected(&rejected_intents);
        self.pacemaker.advance_on_qc(produced_qc);
        let local_validator_id = self.local_validator_id();
        self.telemetry.on_block_produced(
            &block,
            &self.pacemaker,
            local_validator_id,
            block.header.timestamp_ms,
        );
        self.telemetry.on_round_started(
            &self.pacemaker,
            local_validator_id,
            block.header.timestamp_ms,
        );
        let previous_timestamp_ms = self.head().header.timestamp_ms;
        self.chain.push(block);
        self.note_committed_block(self.head().header.height);
        let committed_head = self.head().clone();
        self.index_evm_transaction_receipts(&committed_head, &evm_transactions);
        let committed_head = self.head().clone();
        self.apply_block_oracle_price_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_funding_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_faucet_items(&committed_head);
        self.record_rejected_intents(
            Some(self.head().header.height),
            self.head().header.timestamp_ms,
            &rejected_intents,
        );
        for event in committed_events {
            self.record_runtime_event(
                Some(self.head().header.height),
                self.head().header.timestamp_ms,
                event,
            );
        }
        let committed_head = self.head().clone();
        self.apply_block_liquidation_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_real_capital_governance_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_delegation_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_due_winding_grace_failures(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_market_maker_program_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_tee_governance_items(&committed_head);
        self.crystallize_due_delegations_at_head()?;
        self.evaluate_post_mode_mandates_at_head()?;
        self.evaluate_mandate_objectives_at_head();
        self.expire_due_mandates_at_head()?;
        self.expire_due_operator_attestations_at_head(previous_timestamp_ms);
        self.sweep_resting_order_mandates_at(self.head().header.timestamp_ms);

        // RFC 046 — emit per-market OI gauge events after every fill /
        // liquidation event in this block, sorted by ascending market_id.
        let block_height = self.head().header.height;
        let block_timestamp_ms = self.head().header.timestamp_ms;
        self.record_market_open_interest_diff(&pre_block_oi, block_height, block_timestamp_ms);
        self.evaluate_market_maker_compliance_at_head();
        self.sample_account_equity_at_head();

        Ok(self.head())
    }

    pub(super) fn install_mahi_prepared_block_state(
        &mut self,
        prepared: PreparedCommittedBlock,
    ) -> Result<&Block, NodeError> {
        let PreparedCommittedBlock {
            prepared_block,
            policy_store,
            order_book_overlays,
            resting_order_mandate_overlays,
            collateral_overlays,
            rejected_intents,
            committed_events,
        } = prepared;
        let block = prepared_block.block.clone();
        let evm_transactions = prepared_block.result.evm_transactions.clone();
        self.executor
            .commit_prepared_block(prepared_block)
            .map_err(NodeError::Execution)?;
        self.install_mahi_committed_block_state(
            block,
            policy_store,
            order_book_overlays,
            resting_order_mandate_overlays,
            collateral_overlays,
            rejected_intents,
            committed_events,
            evm_transactions,
            false,
        )
    }

    pub(super) fn install_mahi_committed_block_state(
        &mut self,
        block: Block,
        policy_store: PolicyStore,
        order_book_overlays: BTreeMap<u64, SingleMarketBook>,
        resting_order_mandates: BTreeMap<(u64, u64, u64), MandateId>,
        collateral_overlays: BTreeMap<(u64, u64), i128>,
        rejected_intents: Vec<RejectedIntent>,
        committed_events: Vec<RuntimeEventKind>,
        evm_transactions: Vec<EvmTransactionExecution>,
        settle_native_collateral_custody: bool,
    ) -> Result<&Block, NodeError> {
        if settle_native_collateral_custody {
            self.apply_native_collateral_custody_settlement(&block.body)?;
        }
        self.policy_store = policy_store;
        for (market_id, order_book) in order_book_overlays {
            self.order_books.insert(market_id, order_book);
        }
        self.resting_order_mandates = resting_order_mandates;
        self.apply_collateral_overlays(collateral_overlays);
        self.install_mahi_committed_block_metadata(
            block,
            rejected_intents,
            committed_events,
            evm_transactions,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn install_native_local_block_state(
        &mut self,
        parent: &Block,
        round: u64,
        proposer: ValidatorId,
        body: BlockBody,
        policy_store: PolicyStore,
        order_book_overlays: BTreeMap<u64, SingleMarketBook>,
        resting_order_mandates: BTreeMap<(u64, u64, u64), MandateId>,
        collateral_overlays: BTreeMap<(u64, u64), i128>,
        rejected_intents: Vec<RejectedIntent>,
        committed_events: Vec<RuntimeEventKind>,
        evm_transactions: Vec<EvmTransactionExecution>,
        event_root_override: Option<Hash>,
        timestamp_ms: u64,
    ) -> Result<&Block, NodeError> {
        if timestamp_ms < parent.header.timestamp_ms {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::NonMonotonicTimestamp,
            ));
        }
        let tx_root = body_root(&body);
        let event_root =
            event_root_override.unwrap_or_else(|| mahi_runtime_event_root(&committed_events));
        let state_root = mahi_native_state_root(
            &parent.header.state_root,
            &tx_root,
            &event_root,
            round,
            timestamp_ms,
        );
        let block = Block {
            header: BlockHeader {
                chain_id: parent.header.chain_id,
                height: parent.header.height.saturating_add(1),
                round,
                proposer,
                parent_hash: block_hash(parent),
                parent_state_root: parent.header.state_root,
                tx_root,
                event_root,
                state_root,
                qc: synthetic_mahi_quorum_certificate(parent),
                timestamp_ms,
            },
            body,
        };
        self.install_mahi_committed_block_state(
            block,
            policy_store,
            order_book_overlays,
            resting_order_mandates,
            collateral_overlays,
            rejected_intents,
            committed_events,
            evm_transactions,
            true,
        )
    }

    pub(super) fn install_mahi_committed_block_metadata(
        &mut self,
        block: Block,
        rejected_intents: Vec<RejectedIntent>,
        committed_events: Vec<RuntimeEventKind>,
        evm_transactions: Vec<EvmTransactionExecution>,
    ) -> Result<&Block, NodeError> {
        // RFC 046 — snapshot OI before any committed events apply state;
        // diffed and emitted at end-of-block.
        let pre_block_oi = self.market_open_interest_snapshot();
        self.intent_mempool.remove_block(&block.body);
        self.remove_pending_faucet_grants_in_block(&block.body);
        self.remove_pending_delegation_actions_in_block(&block.body);
        self.remove_pending_market_maker_program_actions_in_block(&block.body);
        self.remove_pending_tee_governance_actions_in_block(&block.body);
        self.remove_pending_real_capital_governance_actions_in_block(&block.body);
        self.intent_mempool.remove_rejected(&rejected_intents);
        self.refresh_proposer_frontier_for_block(&block.body);
        self.refresh_proposer_frontier_for_rejected(&rejected_intents);
        let produced_qc = synthetic_mahi_quorum_certificate(&block);
        self.pacemaker.advance_on_qc(produced_qc);
        let local_validator_id = self.local_validator_id();
        self.telemetry.on_block_produced(
            &block,
            &self.pacemaker,
            local_validator_id,
            block.header.timestamp_ms,
        );
        self.telemetry.on_round_started(
            &self.pacemaker,
            local_validator_id,
            block.header.timestamp_ms,
        );
        let previous_timestamp_ms = self.head().header.timestamp_ms;
        self.chain.push(block);
        self.note_committed_block(self.head().header.height);
        let committed_head = self.head().clone();
        self.index_evm_transaction_receipts(&committed_head, &evm_transactions);
        let committed_head = self.head().clone();
        self.apply_block_oracle_price_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_funding_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_faucet_items(&committed_head);
        self.record_rejected_intents(
            Some(self.head().header.height),
            self.head().header.timestamp_ms,
            &rejected_intents,
        );
        for event in committed_events {
            self.record_runtime_event(
                Some(self.head().header.height),
                self.head().header.timestamp_ms,
                event,
            );
        }
        let committed_head = self.head().clone();
        self.apply_block_liquidation_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_real_capital_governance_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_delegation_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_due_winding_grace_failures(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_market_maker_program_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_tee_governance_items(&committed_head);
        self.crystallize_due_delegations_at_head()?;
        self.evaluate_post_mode_mandates_at_head()?;
        self.evaluate_mandate_objectives_at_head();
        self.expire_due_mandates_at_head()?;
        self.expire_due_operator_attestations_at_head(previous_timestamp_ms);
        self.sweep_resting_order_mandates_at(self.head().header.timestamp_ms);

        // RFC 046 — emit per-market OI gauge events after every fill /
        // liquidation event in this block, sorted by ascending market_id.
        let block_height = self.head().header.height;
        let block_timestamp_ms = self.head().header.timestamp_ms;
        self.record_market_open_interest_diff(&pre_block_oi, block_height, block_timestamp_ms);
        self.evaluate_market_maker_compliance_at_head();
        self.sample_account_equity_at_head();

        Ok(self.head())
    }

    pub(super) fn apply_native_collateral_custody_settlement(
        &mut self,
        body: &BlockBody,
    ) -> Result<(), NodeError> {
        let mut sets = Vec::new();
        let mut deletes = Vec::new();

        for item in &body.items {
            let BlockItem::Intent(signed_intent) = item else {
                continue;
            };
            let (market_id, amount_usdc, side) = match &signed_intent.intent.kind {
                IntentKind::DepositCollateral {
                    market_id,
                    amount_usdc,
                } => (*market_id, *amount_usdc, CollateralIntentSide::Deposit),
                IntentKind::WithdrawCollateral {
                    market_id,
                    amount_usdc,
                } => (*market_id, *amount_usdc, CollateralIntentSide::Withdraw),
                _ => continue,
            };
            if amount_usdc == 0 {
                continue;
            }

            let account_id = signed_intent.intent.account;
            let account_address = self
                .get_committed_state(&account_evm_address_state_key(account_id))
                .and_then(|bytes| bytes.as_slice().try_into().ok())
                .ok_or(NodeError::InvalidCollateralMovement {
                    market_id,
                    account_id,
                    reason: "account_evm_address_missing",
                })?;
            let custody_address = isolated_margin_market_custody_address(market_id);
            let account_balance = self.policy_usdc_balance_for_collateral(
                account_address,
                market_id,
                account_id,
                "policy_usdc_balance_malformed",
            )?;
            let custody_balance = self.policy_usdc_balance_for_collateral(
                custody_address,
                market_id,
                account_id,
                "market_custody_balance_malformed",
            )?;
            let amount = U256::from(amount_usdc);
            let (next_account_balance, next_custody_balance) = match side {
                CollateralIntentSide::Deposit => (
                    account_balance.checked_sub(amount).ok_or(
                        NodeError::InvalidCollateralMovement {
                            market_id,
                            account_id,
                            reason: "insufficient_policy_usdc_balance",
                        },
                    )?,
                    custody_balance.checked_add(amount).ok_or(
                        NodeError::InvalidCollateralMovement {
                            market_id,
                            account_id,
                            reason: "market_custody_balance_overflow",
                        },
                    )?,
                ),
                CollateralIntentSide::Withdraw => (
                    account_balance.checked_add(amount).ok_or(
                        NodeError::InvalidCollateralMovement {
                            market_id,
                            account_id,
                            reason: "policy_usdc_balance_overflow",
                        },
                    )?,
                    custody_balance.checked_sub(amount).ok_or(
                        NodeError::InvalidCollateralMovement {
                            market_id,
                            account_id,
                            reason: "insufficient_market_custody_balance",
                        },
                    )?,
                ),
            };

            Self::apply_policy_usdc_balance_update(
                &mut sets,
                &mut deletes,
                account_address,
                next_account_balance,
            );
            Self::apply_policy_usdc_balance_update(
                &mut sets,
                &mut deletes,
                custody_address,
                next_custody_balance,
            );
        }

        if sets.is_empty() && deletes.is_empty() {
            return Ok(());
        }
        self.executor
            .apply_state_updates(sets, deletes)
            .map(|_| ())
            .map_err(NodeError::Execution)
    }

    pub(super) fn policy_usdc_balance_for_collateral(
        &self,
        address: [u8; 20],
        market_id: u64,
        account_id: u64,
        malformed_reason: &'static str,
    ) -> Result<U256, NodeError> {
        self.get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes.as_slice())
                    .map(U256::from_be_bytes)
                    .map_err(|_| NodeError::InvalidCollateralMovement {
                        market_id,
                        account_id,
                        reason: malformed_reason,
                    })
            })
            .transpose()
            .map(|balance| balance.unwrap_or(U256::ZERO))
    }

    pub fn prepare_network_block_from_mempool(
        &mut self,
        proposer: ValidatorId,
        validator_verifiers: &[ValidatorVerifier],
        timestamp_ms: u64,
        max_items: usize,
        max_clock_skew_ms: u64,
    ) -> Result<Block, NodeError> {
        self.prepare_network_block_commit_from_mempool(
            proposer,
            validator_verifiers,
            timestamp_ms,
            max_items,
            max_clock_skew_ms,
        )
        .map(|prepared| prepared.block().clone())
    }

    pub fn prepare_network_block_commit_from_mempool(
        &mut self,
        proposer: ValidatorId,
        validator_verifiers: &[ValidatorVerifier],
        timestamp_ms: u64,
        max_items: usize,
        max_clock_skew_ms: u64,
    ) -> Result<PreparedCommittedBlock, NodeError> {
        let _expired_conditional = self.purge_expired_conditional_once(timestamp_ms);
        let promoted = self.conditional_pool.promote_ready(
            &self.policy_store,
            &mut self.intent_mempool,
            timestamp_ms,
        )?;
        self.track_proposer_frontier_admitted_intents(&promoted);
        let expired_direct = self.purge_expired_direct_once(timestamp_ms);
        self.refresh_proposer_frontier_for_signed_intents(&expired_direct);
        let next_block_number = self.head().header.height.saturating_add(1);
        let funding_items = self.consensus_funding_items(timestamp_ms);
        let positions_after_funding =
            self.positions_after_funding_items(&funding_items, timestamp_ms);
        let mut plan = self.intent_mempool.plan_block_body_from_frontier(
            &self.policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &self.order_books,
            &self.resting_order_mandates,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
            &self.isolated_collateral,
            &positions_after_funding,
            self.head().header.chain_id,
            next_block_number,
            timestamp_ms,
            max_items,
            &self.proposer_frontier,
        );
        let faucet_items =
            self.pending_faucet_block_items(max_items.saturating_sub(plan.body.items.len()));
        let delegation_items = self.pending_delegation_block_items(
            max_items.saturating_sub(plan.body.items.len() + faucet_items.len()),
        );
        let market_maker_program_items =
            self.pending_market_maker_program_block_items(max_items.saturating_sub(
                plan.body.items.len() + faucet_items.len() + delegation_items.len(),
            ));
        let tee_governance_items =
            self.pending_tee_governance_block_items(max_items.saturating_sub(
                plan.body.items.len()
                    + faucet_items.len()
                    + delegation_items.len()
                    + market_maker_program_items.len(),
            ));
        let real_capital_governance_items =
            self.pending_real_capital_governance_block_items(max_items.saturating_sub(
                plan.body.items.len()
                    + faucet_items.len()
                    + delegation_items.len()
                    + market_maker_program_items.len()
                    + tee_governance_items.len(),
            ));
        plan.body.items.splice(0..0, funding_items);
        plan.body
            .items
            .extend(self.consensus_liquidation_items_with_prices(
                next_block_number,
                timestamp_ms,
                &positions_after_funding,
                &self.market_marks,
                &self.market_index_prices,
                &plan.order_book_overlays,
                &plan.resting_order_mandate_overlays,
                &plan.collateral_overlays,
                &plan.committed_events,
            ));
        plan.body.items.extend(faucet_items);
        plan.body.items.extend(real_capital_governance_items);
        plan.body.items.extend(delegation_items);
        plan.body.items.extend(market_maker_program_items);
        plan.body.items.extend(tee_governance_items);
        self.validate_consensus_liquidation_items(
            &plan.body,
            next_block_number,
            timestamp_ms,
            &positions_after_funding,
            &self.market_marks,
            &self.market_index_prices,
            &plan.order_book_overlays,
            &plan.resting_order_mandate_overlays,
            &plan.collateral_overlays,
            &plan.committed_events,
        )?;
        let parent = self.head().clone();
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        let executor = self.executor.clone();
        let high_qc = self.pacemaker.highest_qc.clone();
        let prepared_block = executor
            .prepare_block_with_checkpoint(
                &parent,
                round,
                proposer,
                high_qc.clone(),
                plan.body.clone(),
                timestamp_ms,
            )
            .map_err(NodeError::Execution)?;
        let block = prepared_block.block.clone();
        let block = Block {
            header: BlockHeader {
                chain_id: parent.header.chain_id,
                height: parent.header.height.saturating_add(1),
                round,
                proposer,
                parent_hash: block_hash(&parent),
                parent_state_root: parent.header.state_root,
                tx_root: body_root(&block.body),
                event_root: block.header.event_root,
                state_root: block.header.state_root,
                qc: high_qc,
                timestamp_ms,
            },
            body: block.body,
        };
        validate_block_against_parent_and_membership(
            &block,
            &parent.header,
            validator_verifiers,
            proposer,
            timestamp_ms,
            max_clock_skew_ms,
        )
        .map_err(NodeError::InvalidBlock)?;
        Ok(self.prepared_commit_from_plan(
            PreparedBlock {
                block,
                result: prepared_block.result,
                checkpoint: prepared_block.checkpoint,
            },
            plan,
        ))
    }

    pub fn sign_network_block_qc(
        &self,
        block: &Block,
        validator_verifiers: &[ValidatorVerifier],
        expected_proposer: ValidatorId,
        max_clock_skew_ms: u64,
    ) -> Result<Signature, NodeError> {
        let parent_header = self
            .chain
            .iter()
            .rev()
            .find(|candidate| {
                candidate.header.height.saturating_add(1) == block.header.height
                    && block_hash(candidate) == block.header.parent_hash
            })
            .map(|candidate| &candidate.header)
            .unwrap_or(&self.head().header);
        validate_block_against_parent_and_membership(
            block,
            parent_header,
            validator_verifiers,
            expected_proposer,
            block.header.timestamp_ms,
            max_clock_skew_ms,
        )
        .map_err(NodeError::InvalidBlock)?;
        let signer = self
            .local_validator_id()
            .ok_or(NodeError::LocalValidatorUnavailable)?;
        let signing_key = self.consensus_signing_keys.get(&signer).copied().ok_or(
            NodeError::MissingConsensusSigningKey {
                validator_id: signer,
            },
        )?;
        let signing_bytes = quorum_certificate_signing_bytes(&QuorumCertificate {
            view: block.header.round,
            block_hash: block_hash(block),
            signatures: vec![],
        })
        .map_err(NodeError::InvalidBlock)?;
        Ok(Signature {
            validator: signer,
            bytes: consensus_signature_bytes(signing_key, &signing_bytes),
        })
    }

    pub fn sign_current_head_qc(
        &self,
        expected_head_hash: Hash,
        expected_head_round: u64,
    ) -> Result<Signature, NodeError> {
        let target_block = self
            .chain
            .iter()
            .rev()
            .find(|block| {
                block.header.round == expected_head_round && block_hash(block) == expected_head_hash
            })
            .or_else(|| {
                let head = self.head();
                (head.header.round == expected_head_round && block_hash(head) == expected_head_hash)
                    .then_some(head)
            });
        let Some(target_block) = target_block else {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::InvalidQuorumCertificate,
            ));
        };
        let signer = self
            .local_validator_id()
            .ok_or(NodeError::LocalValidatorUnavailable)?;
        let signing_key = self.consensus_signing_keys.get(&signer).copied().ok_or(
            NodeError::MissingConsensusSigningKey {
                validator_id: signer,
            },
        )?;
        let signing_bytes = quorum_certificate_signing_bytes(&QuorumCertificate {
            view: target_block.header.round,
            block_hash: expected_head_hash,
            signatures: vec![],
        })
        .map_err(NodeError::InvalidBlock)?;
        Ok(Signature {
            validator: signer,
            bytes: consensus_signature_bytes(signing_key, &signing_bytes),
        })
    }

    pub fn sign_timeout_message(
        &self,
        validator_verifiers: &[ValidatorVerifier],
    ) -> Result<TimeoutMessage, NodeError> {
        let signer = self
            .local_validator_id()
            .ok_or(NodeError::LocalValidatorUnavailable)?;
        let signing_key = self.consensus_signing_keys.get(&signer).copied().ok_or(
            NodeError::MissingConsensusSigningKey {
                validator_id: signer,
            },
        )?;
        let mut timeout =
            self.pacemaker
                .local_timeout(self.chain_id(), self.head().header.height, signer);
        let signing_bytes =
            timeout_message_signing_bytes(&timeout).map_err(NodeError::InvalidBlock)?;
        timeout.signature = consensus_signature_bytes(signing_key, &signing_bytes);
        validate_timeout_message(&timeout, validator_verifiers).map_err(NodeError::InvalidBlock)?;
        Ok(timeout)
    }

    pub fn install_head_qc(
        &mut self,
        qc: QuorumCertificate,
        validator_verifiers: &[ValidatorVerifier],
    ) -> Result<(), NodeError> {
        let head_hash = block_hash(self.head());
        validate_quorum_certificate_with_membership(&qc, &head_hash, validator_verifiers)
            .map_err(NodeError::InvalidBlock)?;
        if qc.view != self.head().header.round {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::InvalidQuorumCertificate,
            ));
        }
        self.pacemaker.advance_on_qc(qc);
        Ok(())
    }

    pub fn install_timeout_messages(
        &mut self,
        messages: &[TimeoutMessage],
        validator_verifiers: &[ValidatorVerifier],
        timestamp_ms: u64,
    ) -> Result<u64, NodeError> {
        if messages.is_empty() {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::InvalidTimeoutCertificate,
            ));
        }
        let expected_round = messages[0].round;
        let expected_chain_id = self.chain_id();
        for message in messages {
            if message.chain_id != expected_chain_id || message.round != expected_round {
                return Err(NodeError::InvalidBlock(
                    BlockValidationError::InvalidTimeoutCertificate,
                ));
            }
        }
        let timeout_certificate =
            build_timeout_certificate(expected_round, messages, validator_verifiers)
                .map_err(NodeError::InvalidBlock)?;
        self.pacemaker
            .advance_on_timeout_certificate(&timeout_certificate);
        let local_validator_id = self.local_validator_id();
        self.telemetry
            .on_timeout(&self.pacemaker, local_validator_id, timestamp_ms);
        self.telemetry
            .on_view_change(&self.pacemaker, local_validator_id, timestamp_ms);
        self.telemetry
            .on_round_started(&self.pacemaker, local_validator_id, timestamp_ms);
        Ok(self.current_round())
    }

    pub fn apply_external_block(
        &mut self,
        block: Block,
        produced_qc: QuorumCertificate,
        validator_verifiers: &[ValidatorVerifier],
        expected_proposer: ValidatorId,
        max_clock_skew_ms: u64,
    ) -> Result<&Block, NodeError> {
        validate_block_against_parent_and_membership(
            &block,
            &self.head().header,
            validator_verifiers,
            expected_proposer,
            block.header.timestamp_ms,
            max_clock_skew_ms,
        )
        .map_err(NodeError::InvalidBlock)?;
        validate_quorum_certificate_with_membership(
            &produced_qc,
            &block_hash(&block),
            validator_verifiers,
        )
        .map_err(NodeError::InvalidBlock)?;

        let oracle_prices = block
            .body
            .items
            .iter()
            .filter_map(|item| match item {
                BlockItem::OraclePrice(observation) => Some(observation.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (market_marks, market_index_prices) =
            self.oracle_price_maps_for_mahi_block(&oracle_prices);
        let positions_after_funding = self.positions_after_funding_items_with_prices(
            &block.body.items,
            block.header.timestamp_ms,
            &market_marks,
            &market_index_prices,
        );
        let mut executor = self.executor.clone();
        let execution_result = executor
            .execute_block(&block)
            .map_err(NodeError::Execution)?;
        let evm_transactions = execution_result.evm_transactions.clone();
        let mut policy_store = self.policy_store.clone();
        let mut order_books = self.order_books.clone();
        let mut resting_order_mandates = self.resting_order_mandates.clone();
        let mut isolated_collateral = self.isolated_collateral.clone();
        let committed_events = commit_block_intents(
            &mut policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &mut order_books,
            &mut resting_order_mandates,
            &mut isolated_collateral,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &market_marks,
            &market_index_prices,
            &self.market_oracle_config,
            &positions_after_funding,
            block.header.chain_id,
            block.header.height,
            &block.body,
            block.header.height,
            block.header.timestamp_ms,
        )?;
        self.validate_consensus_liquidation_items(
            &block.body,
            block.header.height,
            block.header.timestamp_ms,
            &positions_after_funding,
            &market_marks,
            &market_index_prices,
            &order_books,
            &resting_order_mandates,
            &isolated_collateral,
            &committed_events,
        )?;

        self.executor = executor;
        self.policy_store = policy_store;
        self.order_books = order_books;
        self.resting_order_mandates = resting_order_mandates;
        self.isolated_collateral = isolated_collateral;
        self.intent_mempool.remove_block(&block.body);
        self.remove_pending_faucet_grants_in_block(&block.body);
        self.remove_pending_delegation_actions_in_block(&block.body);
        self.remove_pending_market_maker_program_actions_in_block(&block.body);
        self.remove_pending_tee_governance_actions_in_block(&block.body);
        self.remove_pending_real_capital_governance_actions_in_block(&block.body);
        self.refresh_proposer_frontier_for_block(&block.body);
        self.pacemaker.advance_on_qc(produced_qc);
        let local_validator_id = self.local_validator_id();
        self.telemetry.on_block_produced(
            &block,
            &self.pacemaker,
            local_validator_id,
            block.header.timestamp_ms,
        );
        self.telemetry.on_round_started(
            &self.pacemaker,
            local_validator_id,
            block.header.timestamp_ms,
        );
        let previous_timestamp_ms = self.head().header.timestamp_ms;
        self.chain.push(block);
        self.note_committed_block(self.head().header.height);
        let committed_head = self.head().clone();
        self.index_evm_transaction_receipts(&committed_head, &evm_transactions);
        let committed_head = self.head().clone();
        self.apply_block_oracle_price_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_funding_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_faucet_items(&committed_head);
        for event in committed_events {
            self.record_runtime_event(
                Some(self.head().header.height),
                self.head().header.timestamp_ms,
                event,
            );
        }
        let committed_head = self.head().clone();
        self.apply_block_liquidation_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_real_capital_governance_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_delegation_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_due_winding_grace_failures(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_market_maker_program_items(&committed_head);
        let committed_head = self.head().clone();
        self.apply_block_tee_governance_items(&committed_head);
        self.crystallize_due_delegations_at_head()?;
        self.evaluate_post_mode_mandates_at_head()?;
        self.evaluate_mandate_objectives_at_head();
        self.expire_due_mandates_at_head()?;
        self.expire_due_operator_attestations_at_head(previous_timestamp_ms);
        self.sweep_resting_order_mandates_at(self.head().header.timestamp_ms);
        self.sample_account_equity_at_head();

        Ok(self.head())
    }

    pub fn apply_prepared_block(
        &mut self,
        prepared: PreparedCommittedBlock,
        produced_qc: QuorumCertificate,
        validator_verifiers: &[ValidatorVerifier],
        expected_proposer: ValidatorId,
        max_clock_skew_ms: u64,
    ) -> Result<&Block, NodeError> {
        let block = prepared.block();
        validate_block_against_parent_and_membership(
            block,
            &self.head().header,
            validator_verifiers,
            expected_proposer,
            block.header.timestamp_ms,
            max_clock_skew_ms,
        )
        .map_err(NodeError::InvalidBlock)?;
        validate_quorum_certificate_with_membership(
            &produced_qc,
            &block_hash(block),
            validator_verifiers,
        )
        .map_err(NodeError::InvalidBlock)?;
        let oracle_prices = block
            .body
            .items
            .iter()
            .filter_map(|item| match item {
                BlockItem::OraclePrice(observation) => Some(observation.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let (market_marks, market_index_prices) =
            self.oracle_price_maps_for_mahi_block(&oracle_prices);
        let positions_after_funding = self.positions_after_funding_items_with_prices(
            &block.body.items,
            block.header.timestamp_ms,
            &market_marks,
            &market_index_prices,
        );
        self.validate_consensus_liquidation_items(
            &block.body,
            block.header.height,
            block.header.timestamp_ms,
            &positions_after_funding,
            &market_marks,
            &market_index_prices,
            &prepared.order_book_overlays,
            &prepared.resting_order_mandate_overlays,
            &prepared.collateral_overlays,
            &prepared.committed_events,
        )?;
        self.install_prepared_block_state(prepared, produced_qc)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn filter_mahi_intent_payload<'a, I>(
        &self,
        intents: I,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        positions_after_funding: &BTreeMap<(u64, u64), PositionLedger>,
        block_number: u64,
        timestamp_ms: u64,
    ) -> (Vec<BlockItem>, Vec<RejectedIntent>)
    where
        I: IntoIterator<Item = &'a SignedIntent>,
    {
        let mut expected_nonces = BTreeMap::new();
        let mut burned_policies = BTreeSet::new();
        let mut simulated_policy_store = self.policy_store.clone();
        let mut simulated_order_books = BTreeMap::new();
        let mut simulated_resting_order_mandates = self.resting_order_mandates.clone();
        let mut simulated_isolated_collateral = BTreeMap::new();
        let mut items = Vec::new();
        let mut rejected = Vec::new();

        for signed_intent in intents {
            let policy_id = signed_intent.intent.policy_id;
            if burned_policies.contains(&policy_id) {
                rejected.push(RejectedIntent {
                    signed_intent: signed_intent.clone(),
                    reason: IntentRejectionReason::PredecessorIntentRejected,
                    order_reason: None,
                });
                continue;
            }

            let expected_nonce = expected_nonces
                .entry(policy_id)
                .or_insert_with(|| self.policy_store.next_intent_nonce(policy_id).unwrap_or(1));
            if signed_intent.intent.nonce != *expected_nonce {
                continue;
            }

            let mut next_policy_store = simulated_policy_store.clone();
            let mut next_order_books = simulated_order_books.clone();
            let mut next_resting_order_mandates = simulated_resting_order_mandates.clone();
            let mut next_isolated_collateral = simulated_isolated_collateral.clone();
            match simulate_intent_on_runtime_state(
                &mut next_policy_store,
                &self.mandates,
                &self.delegations,
                &self.account_operators,
                simulation_order_books_for_intent(
                    &mut next_order_books,
                    &self.order_books,
                    signed_intent,
                ),
                &mut next_resting_order_mandates,
                simulation_collateral_for_intent(
                    &mut next_isolated_collateral,
                    &self.isolated_collateral,
                    signed_intent,
                ),
                &self.frozen_accounts,
                &self.markets,
                &self.paused_markets,
                market_marks,
                market_index_prices,
                &self.market_oracle_config,
                positions_after_funding,
                signed_intent,
                self.head().header.chain_id,
                block_number,
                timestamp_ms,
                PolicyAdmissionCommitMode::PreverifiedSession,
            ) {
                Ok(_) => {
                    simulated_policy_store = next_policy_store;
                    simulated_order_books = next_order_books;
                    simulated_resting_order_mandates = next_resting_order_mandates;
                    simulated_isolated_collateral = next_isolated_collateral;
                    items.push(BlockItem::Intent(signed_intent.clone()));
                    *expected_nonce = expected_nonce.saturating_add(1);
                }
                Err(error) if transient_intent_blocker(&error) => {}
                Err(error) => {
                    let (reason, order_reason) = rejection_context_from_node_error(&error);
                    rejected.push(RejectedIntent {
                        signed_intent: signed_intent.clone(),
                        reason,
                        order_reason,
                    });
                    burned_policies.insert(policy_id);
                }
            }
        }

        (items, rejected)
    }

    pub fn apply_mahi_committed_block(
        &mut self,
        mahi_block: &MahiBlock,
    ) -> Result<&Block, NodeError> {
        if !env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)? {
            return self.apply_mahi_committed_blocks_merged(std::slice::from_ref(mahi_block));
        }
        let parent = self.head().clone();
        let effective_round = mahi_block.round.max(parent.header.round.saturating_add(1));
        let timestamp_ms = mahi_block.timestamp_ms.max(parent.header.timestamp_ms);
        let (market_marks, market_index_prices) =
            self.oracle_price_maps_for_mahi_block(&mahi_block.oracle_prices);
        let oracle_items = compact_block_oracle_prices(&mahi_block.oracle_prices)
            .into_iter()
            .map(BlockItem::OraclePrice)
            .collect::<Vec<_>>();
        let funding_items = self.consensus_funding_items_with_prices(
            timestamp_ms,
            &market_marks,
            &market_index_prices,
        );
        let block_number = parent.header.height.saturating_add(1);
        let positions_after_funding = self.positions_after_funding_items_with_prices(
            &funding_items,
            timestamp_ms,
            &market_marks,
            &market_index_prices,
        );
        let (mut items, rejected_intents) = self.filter_mahi_intent_payload(
            mahi_block.intents.iter(),
            &market_marks,
            &market_index_prices,
            &positions_after_funding,
            block_number,
            timestamp_ms,
        );
        items.extend(mahi_block.faucets.iter().cloned().map(BlockItem::Faucet));
        items.extend(
            mahi_block
                .real_capital_governance
                .iter()
                .cloned()
                .map(BlockItem::RealCapitalGovernance),
        );
        items.extend(
            mahi_block
                .delegations
                .iter()
                .cloned()
                .map(BlockItem::Delegation),
        );
        items.extend(
            mahi_block
                .market_maker_programs
                .iter()
                .cloned()
                .map(BlockItem::MarketMakerProgram),
        );
        items.extend(
            mahi_block
                .tee_governance
                .iter()
                .cloned()
                .map(BlockItem::TeeGovernance),
        );
        let mut body = BlockBody { items };
        body.items.splice(0..0, funding_items);
        body.items.splice(0..0, oracle_items);
        let parent_qc = synthetic_mahi_quorum_certificate(&parent);
        let prepared_block = self
            .executor
            .prepare_block_with_checkpoint(
                &parent,
                effective_round,
                mahi_block.author,
                parent_qc,
                body.clone(),
                timestamp_ms,
            )
            .map_err(NodeError::Execution)?;

        let mut policy_store = self.policy_store.clone();
        let mut order_books = self.order_books.clone();
        let mut resting_order_mandates = self.resting_order_mandates.clone();
        let mut isolated_collateral = self.isolated_collateral.clone();
        let commit_output = commit_block_intents_preverified_with_stats(
            &mut policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &mut order_books,
            &mut resting_order_mandates,
            &mut isolated_collateral,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &market_marks,
            &market_index_prices,
            &self.market_oracle_config,
            &positions_after_funding,
            parent.header.chain_id,
            block_number,
            &prepared_block.block.body,
            block_number,
            timestamp_ms,
        )?;
        let resting_batch_stats = commit_output.resting_batch_stats;
        let prepared = PreparedCommittedBlock {
            prepared_block,
            policy_store,
            order_book_overlays: order_books,
            resting_order_mandate_overlays: resting_order_mandates,
            collateral_overlays: isolated_collateral,
            rejected_intents,
            committed_events: commit_output.events,
        };
        self.install_mahi_prepared_block_state(prepared)?;
        self.record_phase6_resting_batch_stats(resting_batch_stats);
        Ok(self.head())
    }

    pub fn apply_mahi_committed_blocks_merged(
        &mut self,
        mahi_blocks: &[MahiBlock],
    ) -> Result<&Block, NodeError> {
        let profile_execution = std::env::var_os(MAHI_EXEC_PROFILE_ENV).is_some();
        let total_started = Instant::now();
        assert!(
            !mahi_blocks.is_empty(),
            "merged Mahi execution requires at least one block"
        );
        let parent = self.head().clone();
        let effective_round = mahi_blocks
            .iter()
            .fold(parent.header.round.saturating_add(1), |round, block| {
                round.max(block.round)
            });
        let timestamp_ms = mahi_blocks
            .iter()
            .fold(parent.header.timestamp_ms, |timestamp, block| {
                timestamp.max(block.timestamp_ms)
            });
        let proposer = mahi_blocks
            .last()
            .map(|block| block.author)
            .expect("merged Mahi execution requires a last block");
        let body_started = Instant::now();
        let oracle_prices = mahi_blocks
            .iter()
            .flat_map(|block| block.oracle_prices.iter().cloned())
            .collect::<Vec<_>>();
        let (market_marks, market_index_prices) =
            self.oracle_price_maps_for_mahi_block(&oracle_prices);
        let oracle_items = compact_block_oracle_prices(&oracle_prices)
            .into_iter()
            .map(BlockItem::OraclePrice)
            .collect::<Vec<_>>();
        let funding_items = self.consensus_funding_items_with_prices(
            timestamp_ms,
            &market_marks,
            &market_index_prices,
        );
        let block_number = parent.header.height.saturating_add(1);
        let positions_after_funding = self.positions_after_funding_items_with_prices(
            &funding_items,
            timestamp_ms,
            &market_marks,
            &market_index_prices,
        );
        let (mut items, rejected_intents) = self.filter_mahi_intent_payload(
            mahi_blocks.iter().flat_map(|block| block.intents.iter()),
            &market_marks,
            &market_index_prices,
            &positions_after_funding,
            block_number,
            timestamp_ms,
        );
        for mahi_block in mahi_blocks {
            items.extend(mahi_block.faucets.iter().cloned().map(BlockItem::Faucet));
            items.extend(
                mahi_block
                    .real_capital_governance
                    .iter()
                    .cloned()
                    .map(BlockItem::RealCapitalGovernance),
            );
            items.extend(
                mahi_block
                    .delegations
                    .iter()
                    .cloned()
                    .map(BlockItem::Delegation),
            );
            items.extend(
                mahi_block
                    .market_maker_programs
                    .iter()
                    .cloned()
                    .map(BlockItem::MarketMakerProgram),
            );
            items.extend(
                mahi_block
                    .tee_governance
                    .iter()
                    .cloned()
                    .map(BlockItem::TeeGovernance),
            );
        }
        let mut body = BlockBody { items };
        body.items.splice(0..0, funding_items);
        body.items.splice(0..0, oracle_items);
        let body_elapsed = body_started.elapsed();
        let legacy_exec = env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)?;
        if !legacy_exec {
            if body.items.is_empty() {
                let roots_started = Instant::now();
                let committed_events = Vec::new();
                let tx_root = body_root(&body);
                let event_root = mahi_runtime_event_root(&committed_events);
                let state_root = mahi_native_state_root(
                    &parent.header.state_root,
                    &tx_root,
                    &event_root,
                    effective_round,
                    timestamp_ms,
                );
                let roots_elapsed = roots_started.elapsed();
                let block = Block {
                    header: BlockHeader {
                        chain_id: parent.header.chain_id,
                        height: parent.header.height.saturating_add(1),
                        round: effective_round,
                        proposer,
                        parent_hash: block_hash(&parent),
                        parent_state_root: parent.header.state_root,
                        tx_root,
                        event_root,
                        state_root,
                        qc: synthetic_mahi_quorum_certificate(&parent),
                        timestamp_ms,
                    },
                    body,
                };
                let install_started = Instant::now();
                self.install_mahi_committed_block_metadata(
                    block,
                    rejected_intents,
                    committed_events,
                    Vec::new(),
                )?;
                let install_elapsed = install_started.elapsed();
                if profile_execution {
                    eprintln!(
                        "mahi_exec_profile mode=native_empty body_items=0 body_ms={} commit_ms=0 roots_ms={} install_ms={} total_ms={}",
                        body_elapsed.as_millis(),
                        roots_elapsed.as_millis(),
                        install_elapsed.as_millis(),
                        total_started.elapsed().as_millis(),
                    );
                }
                return Ok(self.head());
            }
            if phase6_inplace_resting_commit_enabled()
                && self.try_apply_mahi_resting_body_in_place(
                    &parent,
                    effective_round,
                    timestamp_ms,
                    proposer,
                    body.clone(),
                    &market_marks,
                    &market_index_prices,
                    &positions_after_funding,
                    body_elapsed,
                    total_started,
                    profile_execution,
                )?
            {
                return Ok(self.head());
            }
            let commit_started = Instant::now();
            let mut policy_store = self.policy_store.clone();
            let mut order_books = self.order_books.clone();
            let mut resting_order_mandates = self.resting_order_mandates.clone();
            let mut isolated_collateral = self.isolated_collateral.clone();
            let commit_output = commit_block_intents_preverified_with_stats(
                &mut policy_store,
                &self.mandates,
                &self.delegations,
                &self.account_operators,
                &mut order_books,
                &mut resting_order_mandates,
                &mut isolated_collateral,
                &self.frozen_accounts,
                &self.markets,
                &self.paused_markets,
                &market_marks,
                &market_index_prices,
                &self.market_oracle_config,
                &positions_after_funding,
                parent.header.chain_id,
                block_number,
                &body,
                block_number,
                timestamp_ms,
            )?;
            let resting_batch_stats = commit_output.resting_batch_stats;
            let committed_events = commit_output.events;
            let commit_elapsed = commit_started.elapsed();
            let roots_started = Instant::now();
            let tx_root = body_root(&body);
            let event_root = mahi_runtime_event_root(&committed_events);
            let state_root = mahi_native_state_root(
                &parent.header.state_root,
                &tx_root,
                &event_root,
                effective_round,
                timestamp_ms,
            );
            let roots_elapsed = roots_started.elapsed();
            let block = Block {
                header: BlockHeader {
                    chain_id: parent.header.chain_id,
                    height: parent.header.height.saturating_add(1),
                    round: effective_round,
                    proposer,
                    parent_hash: block_hash(&parent),
                    parent_state_root: parent.header.state_root,
                    tx_root,
                    event_root,
                    state_root,
                    qc: synthetic_mahi_quorum_certificate(&parent),
                    timestamp_ms,
                },
                body,
            };
            let install_started = Instant::now();
            self.install_mahi_committed_block_state(
                block,
                policy_store,
                order_books,
                resting_order_mandates,
                isolated_collateral,
                rejected_intents,
                committed_events,
                Vec::new(),
                true,
            )?;
            let install_elapsed = install_started.elapsed();
            self.record_phase6_resting_batch_stats(resting_batch_stats);
            if profile_execution {
                eprintln!(
                    "mahi_exec_profile mode=native blocks={} body_items={} body_ms={} commit_ms={} roots_ms={} install_ms={} total_ms={}",
                    mahi_blocks.len(),
                    self.head().body.items.len(),
                    body_elapsed.as_millis(),
                    commit_elapsed.as_millis(),
                    roots_elapsed.as_millis(),
                    install_elapsed.as_millis(),
                    total_started.elapsed().as_millis(),
                );
            }
            return Ok(self.head());
        }
        let parent_qc = synthetic_mahi_quorum_certificate(&parent);
        let legacy_prepare_started = Instant::now();
        let prepared_block = self
            .executor
            .prepare_block_with_checkpoint(
                &parent,
                effective_round,
                proposer,
                parent_qc,
                body.clone(),
                timestamp_ms,
            )
            .map_err(NodeError::Execution)?;
        let legacy_prepare_elapsed = legacy_prepare_started.elapsed();
        let legacy_commit_started = Instant::now();

        let mut policy_store = self.policy_store.clone();
        let mut order_books = self.order_books.clone();
        let mut resting_order_mandates = self.resting_order_mandates.clone();
        let mut isolated_collateral = self.isolated_collateral.clone();
        let commit_output = commit_block_intents_preverified_with_stats(
            &mut policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &mut order_books,
            &mut resting_order_mandates,
            &mut isolated_collateral,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &market_marks,
            &market_index_prices,
            &self.market_oracle_config,
            &positions_after_funding,
            parent.header.chain_id,
            block_number,
            &prepared_block.block.body,
            block_number,
            timestamp_ms,
        )?;
        let resting_batch_stats = commit_output.resting_batch_stats;
        let legacy_commit_elapsed = legacy_commit_started.elapsed();
        let prepared = PreparedCommittedBlock {
            prepared_block,
            policy_store,
            order_book_overlays: order_books,
            resting_order_mandate_overlays: resting_order_mandates,
            collateral_overlays: isolated_collateral,
            rejected_intents,
            committed_events: commit_output.events,
        };
        let legacy_install_started = Instant::now();
        self.install_mahi_prepared_block_state(prepared)?;
        let legacy_install_elapsed = legacy_install_started.elapsed();
        self.record_phase6_resting_batch_stats(resting_batch_stats);
        if profile_execution {
            eprintln!(
                "mahi_exec_profile mode=legacy blocks={} body_ms={} prepare_ms={} commit_ms={} install_ms={} total_ms={}",
                mahi_blocks.len(),
                body_elapsed.as_millis(),
                legacy_prepare_elapsed.as_millis(),
                legacy_commit_elapsed.as_millis(),
                legacy_install_elapsed.as_millis(),
                total_started.elapsed().as_millis(),
            );
        }
        Ok(self.head())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn try_apply_mahi_resting_body_in_place(
        &mut self,
        parent: &Block,
        effective_round: u64,
        timestamp_ms: u64,
        proposer: ValidatorId,
        body: BlockBody,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        body_elapsed: std::time::Duration,
        total_started: Instant,
        profile_execution: bool,
    ) -> Result<bool, NodeError> {
        let commit_started = Instant::now();
        let Some(commit_output) =
            try_commit_market_sharded_resting_place_order_batch_including_single_shard(
                &mut self.policy_store,
                &self.mandates,
                &self.delegations,
                &mut self.order_books,
                &mut self.resting_order_mandates,
                &self.isolated_collateral,
                &self.frozen_accounts,
                &self.markets,
                &self.paused_markets,
                &market_marks,
                &market_index_prices,
                &self.market_oracle_config,
                positions,
                parent.header.chain_id,
                &body,
                parent.header.height.saturating_add(1),
                timestamp_ms,
            )?
        else {
            return Ok(false);
        };
        let resting_batch_stats = commit_output.resting_batch_stats;
        let committed_events = commit_output.events;
        let commit_elapsed = commit_started.elapsed();
        let roots_started = Instant::now();
        let tx_root = body_root(&body);
        let event_root = mahi_runtime_event_root(&committed_events);
        let state_root = mahi_native_state_root(
            &parent.header.state_root,
            &tx_root,
            &event_root,
            effective_round,
            timestamp_ms,
        );
        let roots_elapsed = roots_started.elapsed();
        let block = Block {
            header: BlockHeader {
                chain_id: parent.header.chain_id,
                height: parent.header.height.saturating_add(1),
                round: effective_round,
                proposer,
                parent_hash: block_hash(parent),
                parent_state_root: parent.header.state_root,
                tx_root,
                event_root,
                state_root,
                qc: synthetic_mahi_quorum_certificate(parent),
                timestamp_ms,
            },
            body,
        };
        let install_started = Instant::now();
        self.install_mahi_committed_block_metadata(
            block,
            Vec::new(),
            committed_events,
            Vec::new(),
        )?;
        let install_elapsed = install_started.elapsed();
        self.record_phase6_resting_batch_stats(resting_batch_stats);
        if profile_execution {
            eprintln!(
                "mahi_exec_profile mode=native_inplace body_items={} body_ms={} commit_ms={} roots_ms={} install_ms={} total_ms={}",
                self.head().body.items.len(),
                body_elapsed.as_millis(),
                commit_elapsed.as_millis(),
                roots_elapsed.as_millis(),
                install_elapsed.as_millis(),
                total_started.elapsed().as_millis(),
            );
        }
        Ok(true)
    }

    pub fn produce_empty_block(&mut self, timestamp_ms: u64) -> Result<&Block, NodeError> {
        let body = BlockBody {
            items: self.consensus_funding_items(timestamp_ms),
        };
        let parent = self.head().clone();
        if timestamp_ms < parent.header.timestamp_ms {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::NonMonotonicTimestamp,
            ));
        }
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        if !env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)? {
            return self.install_native_local_block_state(
                &parent,
                round,
                self.validator_set[0],
                body,
                self.policy_store.clone(),
                self.order_books.clone(),
                self.resting_order_mandates.clone(),
                self.isolated_collateral.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                timestamp_ms,
            );
        }
        let qc = build_quorum_certificate(&parent, &self.consensus_signing_keys)?;
        let prepared_block = self
            .executor
            .prepare_block_with_checkpoint(
                &parent,
                round,
                self.validator_set[0],
                qc,
                body.clone(),
                timestamp_ms,
            )
            .map_err(NodeError::Execution)?;
        let prepared = self.prepared_commit_from_plan(
            prepared_block,
            BlockAssemblyPlan {
                body: body.clone(),
                rejected: Vec::new(),
                policy_store_after: self.policy_store.clone(),
                order_book_overlays: BTreeMap::new(),
                resting_order_mandate_overlays: self.resting_order_mandates.clone(),
                collateral_overlays: BTreeMap::new(),
                committed_events: Vec::new(),
            },
        );
        let produced_qc = build_quorum_certificate(prepared.block(), &self.consensus_signing_keys)?;
        self.install_prepared_block_state(prepared, produced_qc)
    }

    pub fn submit_intent(
        &mut self,
        signed_intent: SignedIntent,
        timestamp_ms: u64,
    ) -> Result<IntentId, NodeError> {
        let expired_direct = self.purge_expired_direct_once(timestamp_ms);
        self.record_expired_intents(None, timestamp_ms, expired_direct);
        let expired_conditional = self.purge_expired_conditional_once(timestamp_ms);
        self.record_expired_intents(None, timestamp_ms, expired_conditional);
        let next_block_number = self.head().header.height.saturating_add(1);
        let intent_id = match validate_submitted_intent(
            &self.policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &self.order_books,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
            &self.isolated_collateral,
            &self.positions,
            &self.resting_order_mandates,
            &self.intent_mempool,
            &self.conditional_pool,
            &signed_intent,
            self.head().header.chain_id,
            next_block_number,
            timestamp_ms,
        ) {
            Ok(intent_id) => intent_id,
            Err(error) => {
                let (reason, _) = rejection_context_from_node_error(&error);
                self.record_mandate_admission_rejection(
                    None,
                    timestamp_ms,
                    next_block_number,
                    &signed_intent,
                    reason,
                );
                return Err(error);
            }
        };
        self.ensure_collateral_intent_policy_usdc_available(&signed_intent)?;

        if let Some(not_before_ms) = intent_not_before_ms(&signed_intent.intent) {
            if timestamp_ms < not_before_ms {
                let policy_id = signed_intent.intent.policy_id;
                let nonce = signed_intent.intent.nonce;
                self.conditional_pool
                    .submit(signed_intent, intent_id, not_before_ms)?;
                self.record_runtime_event(
                    None,
                    timestamp_ms,
                    RuntimeEventKind::IntentAccepted {
                        intent_id,
                        policy_id,
                        nonce,
                        class: IntentAdmissionClass::Conditional,
                    },
                );
                return Ok(intent_id);
            }
        }

        let policy_id = signed_intent.intent.policy_id;
        let nonce = signed_intent.intent.nonce;
        self.intent_mempool.submit(signed_intent, intent_id)?;
        self.proposer_frontier.on_intent_admitted(
            &self.intent_mempool,
            &self.policy_store,
            policy_id,
            nonce,
        );
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::IntentAccepted {
                intent_id,
                policy_id,
                nonce,
                class: IntentAdmissionClass::Direct,
            },
        );
        Ok(intent_id)
    }

    pub fn preview_submit_intent(
        &self,
        signed_intent: &SignedIntent,
        timestamp_ms: u64,
    ) -> Result<IntentAdmissionPreview, NodeError> {
        let next_block_number = self.head().header.height.saturating_add(1);
        let intent_id = validate_submitted_intent(
            &self.policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &self.order_books,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
            &self.isolated_collateral,
            &self.positions,
            &self.resting_order_mandates,
            &self.intent_mempool,
            &self.conditional_pool,
            signed_intent,
            self.head().header.chain_id,
            next_block_number,
            timestamp_ms,
        )?;
        self.ensure_collateral_intent_policy_usdc_available(signed_intent)?;
        let class = if intent_not_before_ms(&signed_intent.intent)
            .is_some_and(|not_before_ms| timestamp_ms < not_before_ms)
        {
            IntentAdmissionClass::Conditional
        } else {
            IntentAdmissionClass::Direct
        };
        let current_policy_nonce = self
            .policy_store
            .next_intent_nonce(signed_intent.intent.policy_id)
            .ok();
        let pending_reason = queue_pending_reason(
            self,
            signed_intent,
            if class == IntentAdmissionClass::Conditional {
                IntentStatusStage::AcceptedConditional
            } else {
                IntentStatusStage::AcceptedDirect
            },
        );
        Ok(IntentAdmissionPreview {
            intent_id,
            class,
            pending_reason,
            next_eligible_at_ms: match pending_reason {
                Some(IntentPendingReason::TimeAfter) => intent_not_before_ms(&signed_intent.intent),
                _ => None,
            },
            current_policy_nonce,
        })
    }

    pub fn produce_block_from_mempool(
        &mut self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> Result<&Block, NodeError> {
        if timestamp_ms < self.head().header.timestamp_ms {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::NonMonotonicTimestamp,
            ));
        }
        let expired_conditional = self.purge_expired_conditional_once(timestamp_ms);
        self.record_expired_intents(None, timestamp_ms, expired_conditional);
        let promoted = self.conditional_pool.promote_ready(
            &self.policy_store,
            &mut self.intent_mempool,
            timestamp_ms,
        )?;
        self.track_proposer_frontier_admitted_intents(&promoted);
        for promoted_intent in promoted {
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::IntentPromoted {
                    intent_id: intent_id(&promoted_intent),
                    policy_id: promoted_intent.intent.policy_id,
                    nonce: promoted_intent.intent.nonce,
                },
            );
        }
        let expired_direct = self.purge_expired_direct_once(timestamp_ms);
        self.refresh_proposer_frontier_for_signed_intents(&expired_direct);
        self.record_expired_intents(None, timestamp_ms, expired_direct);
        let next_block_number = self.head().header.height.saturating_add(1);
        let funding_items = self.consensus_funding_items(timestamp_ms);
        let positions_after_funding =
            self.positions_after_funding_items(&funding_items, timestamp_ms);
        let mut plan = self.intent_mempool.plan_block_body_from_frontier(
            &self.policy_store,
            &self.mandates,
            &self.delegations,
            &self.account_operators,
            &self.order_books,
            &self.resting_order_mandates,
            &self.frozen_accounts,
            &self.markets,
            &self.paused_markets,
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
            &self.isolated_collateral,
            &positions_after_funding,
            self.head().header.chain_id,
            next_block_number,
            timestamp_ms,
            max_items,
            &self.proposer_frontier,
        );
        let faucet_items =
            self.pending_faucet_block_items(max_items.saturating_sub(plan.body.items.len()));
        let delegation_items = self.pending_delegation_block_items(
            max_items.saturating_sub(plan.body.items.len() + faucet_items.len()),
        );
        let market_maker_program_items =
            self.pending_market_maker_program_block_items(max_items.saturating_sub(
                plan.body.items.len() + faucet_items.len() + delegation_items.len(),
            ));
        let tee_governance_items =
            self.pending_tee_governance_block_items(max_items.saturating_sub(
                plan.body.items.len()
                    + faucet_items.len()
                    + delegation_items.len()
                    + market_maker_program_items.len(),
            ));
        let real_capital_governance_items =
            self.pending_real_capital_governance_block_items(max_items.saturating_sub(
                plan.body.items.len()
                    + faucet_items.len()
                    + delegation_items.len()
                    + market_maker_program_items.len()
                    + tee_governance_items.len(),
            ));
        plan.body.items.splice(0..0, funding_items);
        plan.body
            .items
            .extend(self.consensus_liquidation_items_with_prices(
                next_block_number,
                timestamp_ms,
                &positions_after_funding,
                &self.market_marks,
                &self.market_index_prices,
                &plan.order_book_overlays,
                &plan.resting_order_mandate_overlays,
                &plan.collateral_overlays,
                &plan.committed_events,
            ));
        plan.body.items.extend(faucet_items);
        plan.body.items.extend(real_capital_governance_items);
        plan.body.items.extend(delegation_items);
        plan.body.items.extend(market_maker_program_items);
        plan.body.items.extend(tee_governance_items);
        self.validate_consensus_liquidation_items(
            &plan.body,
            next_block_number,
            timestamp_ms,
            &positions_after_funding,
            &self.market_marks,
            &self.market_index_prices,
            &plan.order_book_overlays,
            &plan.resting_order_mandate_overlays,
            &plan.collateral_overlays,
            &plan.committed_events,
        )?;
        let parent = self.head().clone();
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        if !env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)? {
            return self.install_native_local_block_state(
                &parent,
                round,
                self.validator_set[0],
                plan.body,
                plan.policy_store_after,
                plan.order_book_overlays,
                plan.resting_order_mandate_overlays,
                plan.collateral_overlays,
                plan.rejected,
                plan.committed_events,
                Vec::new(),
                None,
                timestamp_ms,
            );
        }
        self.intent_mempool.remove_rejected(&plan.rejected);
        self.refresh_proposer_frontier_for_rejected(&plan.rejected);
        self.record_rejected_intents(None, timestamp_ms, &plan.rejected);
        let qc = build_quorum_certificate(&parent, &self.consensus_signing_keys)?;

        let prepared_block = self
            .executor
            .prepare_block_with_checkpoint(
                &parent,
                round,
                self.validator_set[0],
                qc,
                plan.body.clone(),
                timestamp_ms,
            )
            .map_err(NodeError::Execution)?;
        let prepared = self.prepared_commit_from_plan(prepared_block, plan);
        let produced_qc = build_quorum_certificate(prepared.block(), &self.consensus_signing_keys)?;
        self.install_prepared_block_state(prepared, produced_qc)
    }

    pub(super) fn record_expired_intents(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        expired: Vec<SignedIntent>,
    ) {
        for signed_intent in expired {
            self.record_runtime_event(
                block_height,
                timestamp_ms,
                RuntimeEventKind::IntentExpired {
                    intent_id: intent_id(&signed_intent),
                    policy_id: signed_intent.intent.policy_id,
                    nonce: signed_intent.intent.nonce,
                },
            );
        }
    }

    pub(super) fn record_rejected_intents(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        rejected: &[RejectedIntent],
    ) {
        for rejected_intent in rejected {
            self.record_mandate_admission_rejection(
                block_height,
                timestamp_ms,
                block_height.unwrap_or_else(|| self.head().header.height.saturating_add(1)),
                &rejected_intent.signed_intent,
                rejected_intent.reason,
            );
            self.record_runtime_event(
                block_height,
                timestamp_ms,
                RuntimeEventKind::IntentRejected {
                    intent_id: intent_id(&rejected_intent.signed_intent),
                    policy_id: rejected_intent.signed_intent.intent.policy_id,
                    nonce: rejected_intent.signed_intent.intent.nonce,
                    reason: rejected_intent.reason,
                    order_reason: rejected_intent.order_reason,
                },
            );
        }
    }

    pub(super) fn record_mandate_admission_rejection(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        block_number: u64,
        signed_intent: &SignedIntent,
        reason: IntentRejectionReason,
    ) {
        let Some(event) = self.mandate_admission_rejection_event(
            signed_intent,
            block_number,
            timestamp_ms,
            reason,
        ) else {
            return;
        };
        self.record_runtime_event(block_height, timestamp_ms, event);
    }

    pub(super) fn mandate_admission_rejection_event(
        &self,
        signed_intent: &SignedIntent,
        block_number: u64,
        timestamp_ms: u64,
        reason: IntentRejectionReason,
    ) -> Option<RuntimeEventKind> {
        if !mandate_admission_rejection_reason(reason) {
            return None;
        }
        let origin_policy = self.policy_store.get(signed_intent.intent.policy_id)?;
        if !matches!(origin_policy.origin, PolicyOrigin::MandateGenerated { .. }) {
            return None;
        }
        if reason == IntentRejectionReason::MandateConstraintViolated {
            if let Some((mandate_id, agent, constraint_index)) =
                mandate_max_leverage_admission_violation(
                    &self.policy_store,
                    &self.mandates,
                    &self.order_books,
                    &self.isolated_collateral,
                    &self.markets,
                    &self.market_marks,
                    &self.positions,
                    signed_intent,
                    timestamp_ms,
                )
            {
                return Some(RuntimeEventKind::MandateConstraintTriggered {
                    mandate_id,
                    agent,
                    constraint_index,
                    action: MandateEnforcementAction::AdmissionRejected,
                    triggered_at_ms: timestamp_ms,
                });
            }
            let PolicyOrigin::MandateGenerated { mandate_id, .. } = &origin_policy.origin else {
                return None;
            };
            let mandate = self.mandates.get(mandate_id)?;
            return Some(RuntimeEventKind::MandateConstraintTriggered {
                mandate_id: *mandate_id,
                agent: mandate.agent,
                constraint_index: u16::MAX,
                action: MandateEnforcementAction::AdmissionRejected,
                triggered_at_ms: timestamp_ms,
            });
        }
        let admission = policy_admission_for_intent(
            signed_intent,
            &self.markets,
            self.head().header.chain_id,
            block_number,
            timestamp_ms,
        )
        .ok()?;

        let policy_account_scope =
            policy_account_scope_for_intent(&self.policy_store, &self.delegations, signed_intent)
                .ok()?;
        for (mandate_id, (_, representative_policy_id)) in
            representative_mandate_policies(&self.policy_store, policy_account_scope)
        {
            let Err(error) = self
                .policy_store
                .preview_admission_limits(representative_policy_id, &admission)
            else {
                continue;
            };
            if rejection_reason_from_policy_error(&error) != reason {
                continue;
            }
            let mandate = self.mandates.get(&mandate_id)?;
            return Some(RuntimeEventKind::MandateConstraintTriggered {
                mandate_id,
                agent: mandate.agent,
                constraint_index: mandate_constraint_index_for_policy_error(
                    mandate, &error, &admission,
                ),
                action: MandateEnforcementAction::AdmissionRejected,
                triggered_at_ms: timestamp_ms,
            });
        }

        None
    }

    pub fn produce_block(
        &mut self,
        body: BlockBody,
        state_root: Hash,
        event_root: Hash,
        timestamp_ms: u64,
    ) -> Result<&Block, NodeError> {
        let parent = self.head().clone();
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        let block = build_block(BuildBlockRequest {
            chain_id: parent.header.chain_id,
            proposer: self.validator_set[0],
            parent: &parent,
            validator_verifiers: &self.validator_verifiers,
            consensus_signing_keys: &self.consensus_signing_keys,
            body,
            state_root,
            event_root,
            timestamp_ms,
            round,
            max_clock_skew_ms: self.pacemaker_config.max_clock_skew_ms,
        })?;

        let produced_qc = build_quorum_certificate(&block, &self.consensus_signing_keys)?;
        self.pacemaker.advance_on_qc(produced_qc);
        self.telemetry.on_block_produced(
            &block,
            &self.pacemaker,
            Some(self.validator_set[0]),
            timestamp_ms,
        );
        self.telemetry
            .on_round_started(&self.pacemaker, Some(self.validator_set[0]), timestamp_ms);
        let previous_timestamp_ms = self.head().header.timestamp_ms;
        self.chain.push(block);
        self.note_committed_block(self.head().header.height);
        self.expire_due_operator_attestations_at_head(previous_timestamp_ms);
        self.sample_account_equity_at_head();

        Ok(self.head())
    }
}
