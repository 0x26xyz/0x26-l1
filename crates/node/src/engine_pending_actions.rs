use super::*;

impl SingleNodeEngine {
    pub(crate) fn apply_block_delegation_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::Delegation(delegation) = item else {
                continue;
            };
            let start = self.runtime_events.len();
            let fingerprint = delegation_action_fingerprint(&delegation.action);
            if let Some(processed) = self
                .processed_delegation_actions
                .get(&delegation.action_id)
                .cloned()
            {
                if processed.fingerprint != fingerprint {
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::DelegationActionRejected {
                            action_kind: delegation_block_action_kind(&delegation.action)
                                .to_string(),
                            reason: "delegation_action_id_mismatch".to_string(),
                        },
                    );
                }
                continue;
            }
            let result = self.apply_block_delegation_action(delegation, delegation.timestamp_ms);
            match result {
                Ok(receipt) => {
                    self.processed_delegation_actions.insert(
                        delegation.action_id,
                        ProcessedDelegationAction {
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt,
                        },
                    );
                }
                Err(error) => {
                    let reason = delegation_block_error_reason(&error).to_string();
                    let action_kind = delegation_block_action_kind(&delegation.action).to_string();
                    self.processed_delegation_actions.insert(
                        delegation.action_id,
                        ProcessedDelegationAction {
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt: DelegationActionReceipt::Rejected {
                                action_kind: action_kind.clone(),
                                reason: reason.clone(),
                            },
                        },
                    );
                    self.runtime_events.truncate(start);
                    self.next_runtime_event_seq = start as u64;
                    self.rebuild_block_event_index();
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::DelegationActionRejected {
                            action_kind,
                            reason,
                        },
                    );
                    continue;
                }
            }
            let mut committed_seqs = Vec::new();
            for event in &mut self.runtime_events[start..] {
                event.block_height = Some(block.header.height);
                committed_seqs.push(event.seq);
            }
            for seq in committed_seqs {
                self.note_committed_event_seq(block.header.height, seq);
            }
        }
        self.prune_processed_delegation_actions(block.header.timestamp_ms);
    }

    fn prune_processed_delegation_actions(&mut self, timestamp_ms: u64) {
        let min_timestamp = timestamp_ms.saturating_sub(DELEGATION_ACTION_RECEIPT_RETENTION_MS);
        self.processed_delegation_actions
            .retain(|_, processed| processed.processed_at_ms >= min_timestamp);
    }

    // The RPC bridge reconstructs action responses from runtime events plus
    // engine state after this block carrier runs. Any new response field must
    // therefore be emitted or be derivable from committed state.
    fn apply_block_delegation_action(
        &mut self,
        delegation: &BlockDelegation,
        timestamp_ms: u64,
    ) -> Result<DelegationActionReceipt, NodeError> {
        match &delegation.action {
            BlockDelegationAction::Open {
                agent,
                mandate_id,
                performance_share_units,
                crystallization_cadence_ms,
                notice_period_ms,
                agent_bond,
            } => self
                .open_delegation(
                    *agent,
                    *mandate_id,
                    *performance_share_units,
                    *crystallization_cadence_ms,
                    *notice_period_ms,
                    *agent_bond,
                    timestamp_ms,
                )
                .map(|delegation| DelegationActionReceipt::Open {
                    delegation_id: delegation.delegation_id,
                }),
            BlockDelegationAction::Deposit {
                principal,
                delegation_id,
                amount,
            } => self
                .deposit_to_delegation(*principal, *delegation_id, *amount, timestamp_ms)
                .map(|deposit| DelegationActionReceipt::Deposit {
                    delegation_id: deposit.delegation.delegation_id,
                    principal: deposit.principal,
                    shares_minted: deposit.shares_minted,
                }),
            BlockDelegationAction::RequestRedemption {
                principal,
                delegation_id,
                shares,
            } => self
                .request_delegation_redemption(*principal, *delegation_id, *shares, timestamp_ms)
                .map(|redemption| DelegationActionReceipt::RequestRedemption {
                    request_id: redemption.request.request_id,
                }),
            BlockDelegationAction::FinalizeRedemption { request_id } => self
                .finalize_delegation_redemption(*request_id, timestamp_ms)
                .map(|finalization| {
                    if finalization.request.status == RedemptionRequestStatus::Ready
                        && finalization.usdc_out == 0
                        && finalization.compensation_out == 0
                    {
                        DelegationActionReceipt::FinalizeRedemptionReady {
                            request_id: finalization.request.request_id,
                        }
                    } else {
                        DelegationActionReceipt::FinalizeRedemption {
                            request_id: finalization.request.request_id,
                            usdc_out: finalization.usdc_out,
                            compensation_out: finalization.compensation_out,
                        }
                    }
                }),
            BlockDelegationAction::CancelRedemption {
                principal,
                request_id,
            } => self
                .cancel_delegation_redemption(*principal, *request_id, timestamp_ms)
                .map(|cancellation| DelegationActionReceipt::CancelRedemption {
                    request_id: cancellation.request.request_id,
                }),
            BlockDelegationAction::CloseToDeposits { delegation_id } => self
                .close_delegation_to_deposits(*delegation_id, timestamp_ms)
                .map(|delegation| DelegationActionReceipt::CloseToDeposits {
                    delegation_id: delegation.delegation_id,
                }),
            BlockDelegationAction::BeginWinding { delegation_id } => self
                .begin_delegation_winding(*delegation_id, timestamp_ms)
                .map(|delegation| DelegationActionReceipt::BeginWinding {
                    delegation_id: delegation.delegation_id,
                }),
            BlockDelegationAction::InsurancePayout {
                delegation_id,
                requested_amount_micro_usdc,
            } => self
                .apply_delegation_insurance_payout_at(
                    *delegation_id,
                    *requested_amount_micro_usdc,
                    timestamp_ms,
                    None,
                )
                .map(|payout| DelegationActionReceipt::InsurancePayout {
                    delegation_id: payout.delegation.delegation_id,
                    requested_amount_micro_usdc: payout.requested_amount_micro_usdc,
                    distributed_amount_micro_usdc: payout.distributed_amount_micro_usdc,
                    recipient_count: payout.recipient_count,
                }),
            BlockDelegationAction::ForcedLiquidation {
                delegation_id,
                market_id,
                keeper_account,
                requested_lots,
            } => self
                .force_liquidate_delegation_at(
                    delegation.action_id,
                    *delegation_id,
                    *market_id,
                    *keeper_account,
                    *requested_lots,
                    timestamp_ms,
                    None,
                )
                .map(|liquidation| DelegationActionReceipt::ForcedLiquidation {
                    delegation_id: liquidation.delegation.delegation_id,
                    market_id: liquidation.market_id,
                    keeper_account: liquidation.keeper_account,
                    requested_lots: liquidation.requested_lots,
                    filled_lots: liquidation.filled_lots,
                    remaining_lots: liquidation.remaining_lots,
                    keeper_reward_micro_usdc: liquidation.keeper_reward_micro_usdc,
                }),
            BlockDelegationAction::Close { delegation_id } => self
                .close_delegation(*delegation_id, timestamp_ms)
                .map(|delegation| DelegationActionReceipt::Close {
                    delegation_id: delegation.delegation_id,
                }),
            BlockDelegationAction::Terminate {
                delegation_id,
                reason,
                slash_policy,
            } => self
                .terminate_delegation(*delegation_id, *reason, *slash_policy, timestamp_ms)
                .map(|termination| DelegationActionReceipt::Terminate {
                    delegation_id: termination.delegation.delegation_id,
                    reason: *reason,
                    bond_returned: termination.bond_returned,
                    bond_slashed: termination.bond_slashed,
                    principal_compensation: termination.principal_compensation,
                    insurance_fund_delta: termination.insurance_fund_delta,
                }),
        }
    }

    pub fn commit_delegation_action_block(
        &mut self,
        action_id: DelegationActionId,
        action: BlockDelegationAction,
        timestamp_ms: u64,
    ) -> Result<&Block, NodeError> {
        let parent = self.head().clone();
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        let proposer = self
            .local_validator_id()
            .ok_or(NodeError::LocalValidatorUnavailable)?;
        let body = BlockBody {
            items: vec![BlockItem::Delegation(BlockDelegation {
                action_id,
                timestamp_ms,
                action,
            })],
        };
        if !env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)? {
            return self.install_native_local_block_state(
                &parent,
                round,
                proposer,
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
        let parent_qc = build_quorum_certificate(&parent, &self.consensus_signing_keys)?;
        let prepared_block = self
            .executor
            .clone()
            .prepare_block_with_checkpoint(&parent, round, proposer, parent_qc, body, timestamp_ms)
            .map_err(NodeError::Execution)?;
        let prepared = PreparedCommittedBlock {
            prepared_block,
            policy_store: self.policy_store.clone(),
            order_book_overlays: self.order_books.clone(),
            resting_order_mandate_overlays: self.resting_order_mandates.clone(),
            collateral_overlays: self.isolated_collateral.clone(),
            rejected_intents: Vec::new(),
            committed_events: Vec::new(),
        };
        let produced_qc = build_quorum_certificate(prepared.block(), &self.consensus_signing_keys)?;
        self.install_prepared_block_state(prepared, produced_qc)
    }

    pub fn processed_delegation_action(
        &self,
        action_id: DelegationActionId,
    ) -> Option<&ProcessedDelegationAction> {
        self.processed_delegation_actions.get(&action_id)
    }

    pub fn submit_delegation_action(
        &mut self,
        action_id: DelegationActionId,
        action: BlockDelegationAction,
        submitted_at_ms: u64,
    ) -> Result<DelegationActionSubmissionStatus, NodeError> {
        // SAFETY: callers must canonicalize the action id before entering the
        // engine. The RPC bridge enforces both legacy timestamp-keyed ids and
        // nonce-keyed ids; future signed/gossiped submission paths must enforce
        // their own domain before queueing.
        let fingerprint = delegation_action_fingerprint(&action);
        if let Some(processed) = self.processed_delegation_actions.get(&action_id) {
            if processed.fingerprint != fingerprint {
                return Err(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "delegation_action_id_mismatch",
                });
            }
            return Ok(DelegationActionSubmissionStatus::AlreadyProcessed);
        }
        if let Some(pending) = self.pending_delegation_actions.get(&action_id) {
            if pending.fingerprint != fingerprint {
                return Err(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "delegation_action_id_mismatch",
                });
            }
            return Ok(DelegationActionSubmissionStatus::DuplicatePending);
        }
        self.preflight_pending_delegation_action(action_id, &action, submitted_at_ms)?;
        if self.pending_delegation_actions.len() >= MAX_PENDING_DELEGATION_ACTIONS {
            return Err(NodeError::InvalidDelegation {
                agent: 0,
                reason: "delegation_queue_full",
            });
        }

        let pending = PendingDelegationAction {
            action_id,
            action,
            fingerprint,
            submitted_at_ms,
        };
        self.index_pending_delegation_action(&pending);
        self.pending_delegation_action_order
            .insert(pending_delegation_action_order(&pending));
        self.pending_delegation_actions.insert(action_id, pending);
        self.debug_assert_pending_delegation_indexes();
        Ok(DelegationActionSubmissionStatus::Accepted)
    }

    pub fn submit_signed_delegation_action(
        &mut self,
        signed_action: SignedDelegationAction,
    ) -> Result<DelegationActionSubmissionStatus, NodeError> {
        self.verify_signed_delegation_action(&signed_action)?;
        self.submit_verified_delegation_action(
            signed_action.signer_account_id,
            signed_action.action_id,
            signed_action.action,
            signed_action.timestamp_ms,
        )
    }

    pub fn submit_verified_delegation_action(
        &mut self,
        signer_account_id: u64,
        action_id: DelegationActionId,
        action: BlockDelegationAction,
        timestamp_ms: u64,
    ) -> Result<DelegationActionSubmissionStatus, NodeError> {
        if !self.pending_delegation_actions.contains_key(&action_id)
            && !self.processed_delegation_actions.contains_key(&action_id)
            && self.pending_delegation_action_count_for_signer(signer_account_id)
                >= MAX_PENDING_DELEGATION_ACTIONS_PER_SIGNER
        {
            return Err(NodeError::InvalidDelegation {
                agent: signer_account_id,
                reason: "max_pending_delegation_actions_per_signer",
            });
        }
        self.submit_delegation_action(action_id, action, timestamp_ms)
    }

    fn pending_delegation_action_count_for_signer(&self, signer_account_id: u64) -> usize {
        self.pending_delegation_actions
            .values()
            .filter(|pending| {
                self.delegation_action_signer_account(&pending.action)
                    .is_ok_and(|account_id| account_id == signer_account_id)
            })
            .count()
    }

    fn verify_signed_delegation_action(
        &self,
        signed_action: &SignedDelegationAction,
    ) -> Result<(), NodeError> {
        if signed_action.chain_id != self.chain_id() {
            return Err(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "chain_mismatch",
            });
        }
        let expected_action_id = delegation_action_id_for_submission(
            &signed_action.action,
            signed_action.timestamp_ms,
            signed_action.submitter_nonce,
        );
        if signed_action.action_id != expected_action_id {
            return Err(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "delegation_action_id_mismatch",
            });
        }
        let expected_signer = self.delegation_action_signer_account(&signed_action.action)?;
        if signed_action.signer_account_id != expected_signer {
            return Err(NodeError::InvalidDelegation {
                agent: expected_signer,
                reason: "delegation_action_signer_mismatch",
            });
        }
        let account = self
            .account_store
            .get(signed_action.signer_account_id)
            .ok_or(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "unknown_delegation_action_signer",
            })?;
        let verifying_key = VerifyingKey::from_bytes(&account.owner()).map_err(|_| {
            NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "invalid_delegation_action_signer",
            }
        })?;
        let signature = Ed25519Signature::from_bytes(&signed_action.signature);
        verifying_key
            .verify_strict(&delegation_action_signing_bytes(signed_action), &signature)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "invalid_delegation_action_signature",
            })?;
        Ok(())
    }

    pub fn delegation_action_signer_account(
        &self,
        action: &BlockDelegationAction,
    ) -> Result<u64, NodeError> {
        match action {
            BlockDelegationAction::Open { agent, .. } => Ok(*agent),
            BlockDelegationAction::Deposit { principal, .. }
            | BlockDelegationAction::RequestRedemption { principal, .. }
            | BlockDelegationAction::CancelRedemption { principal, .. } => Ok(*principal),
            BlockDelegationAction::FinalizeRedemption { request_id } => self
                .redemption_requests
                .get(request_id)
                .map(|request| request.principal)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_redemption_request",
                }),
            BlockDelegationAction::CloseToDeposits { delegation_id }
            | BlockDelegationAction::BeginWinding { delegation_id }
            | BlockDelegationAction::Close { delegation_id } => self
                .delegations
                .get(delegation_id)
                .map(|delegation| delegation.agent)
                .or_else(|| {
                    self.pending_open_delegation_any(delegation_id)
                        .map(|(agent, _)| agent)
                })
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                }),
            BlockDelegationAction::InsurancePayout { delegation_id, .. } => {
                let delegation =
                    self.delegations
                        .get(delegation_id)
                        .ok_or(NodeError::InvalidDelegation {
                            agent: 0,
                            reason: "unknown_delegation",
                        })?;
                Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_insurance_payout_requires_operator_submit",
                })
            }
            BlockDelegationAction::ForcedLiquidation { delegation_id, .. } => {
                let delegation =
                    self.delegations
                        .get(delegation_id)
                        .ok_or(NodeError::InvalidDelegation {
                            agent: 0,
                            reason: "unknown_delegation",
                        })?;
                Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_forced_liquidation_requires_operator_submit",
                })
            }
            BlockDelegationAction::Terminate {
                delegation_id,
                reason,
                ..
            } => {
                let delegation =
                    self.delegations
                        .get(delegation_id)
                        .ok_or(NodeError::InvalidDelegation {
                            agent: 0,
                            reason: "unknown_delegation",
                        })?;
                if !delegation_terminate_reason_allows_agent_signature(*reason) {
                    return Err(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "delegation_terminate_requires_operator_submit",
                    });
                }
                Ok(delegation.agent)
            }
        }
    }

    fn preflight_pending_delegation_action(
        &self,
        action_id: DelegationActionId,
        action: &BlockDelegationAction,
        submitted_at_ms: u64,
    ) -> Result<(), NodeError> {
        let candidate_order = PendingDelegationActionOrder {
            submitted_at_ms,
            action_id,
        };
        match action {
            BlockDelegationAction::Open {
                agent,
                mandate_id,
                performance_share_units,
                crystallization_cadence_ms,
                notice_period_ms,
                agent_bond,
            } => self.preflight_open_delegation(
                *agent,
                mandate_id,
                *performance_share_units,
                *crystallization_cadence_ms,
                *notice_period_ms,
                *agent_bond,
                submitted_at_ms,
                &candidate_order,
            ),
            BlockDelegationAction::Deposit {
                principal,
                delegation_id,
                amount,
            } => self.preflight_deposit_to_delegation(
                *principal,
                delegation_id,
                *amount,
                &candidate_order,
            ),
            BlockDelegationAction::RequestRedemption {
                principal,
                delegation_id,
                shares,
            } => self.preflight_request_delegation_redemption(
                *principal,
                delegation_id,
                *shares,
                submitted_at_ms,
            ),
            BlockDelegationAction::FinalizeRedemption { request_id } => {
                self.preflight_finalize_delegation_redemption(request_id, submitted_at_ms)
            }
            BlockDelegationAction::CancelRedemption {
                principal,
                request_id,
            } => self.preflight_cancel_delegation_redemption(*principal, request_id),
            BlockDelegationAction::CloseToDeposits { delegation_id } => {
                self.preflight_close_delegation_to_deposits(delegation_id, &candidate_order)
            }
            BlockDelegationAction::BeginWinding { delegation_id } => {
                self.preflight_begin_delegation_winding(delegation_id, &candidate_order)
            }
            BlockDelegationAction::InsurancePayout {
                delegation_id,
                requested_amount_micro_usdc,
            } => self
                .preflight_delegation_insurance_payout(delegation_id, *requested_amount_micro_usdc),
            BlockDelegationAction::ForcedLiquidation {
                delegation_id,
                market_id,
                keeper_account,
                requested_lots,
            } => self.preflight_force_liquidate_delegation(
                action_id,
                delegation_id,
                *market_id,
                *keeper_account,
                *requested_lots,
                submitted_at_ms,
            ),
            BlockDelegationAction::Close { delegation_id } => {
                self.preflight_close_delegation(delegation_id, &candidate_order)
            }
            BlockDelegationAction::Terminate {
                delegation_id,
                reason,
                slash_policy,
            } => self.preflight_terminate_delegation(delegation_id, *reason, *slash_policy),
        }
    }

    fn preflight_open_delegation(
        &self,
        agent: u64,
        mandate_id: &MandateId,
        performance_share_units: u16,
        crystallization_cadence_ms: u64,
        notice_period_ms: u64,
        agent_bond: u128,
        timestamp_ms: u64,
        candidate_order: &PendingDelegationActionOrder,
    ) -> Result<(), NodeError> {
        validate_open_delegation_config(
            agent,
            mandate_id,
            performance_share_units,
            crystallization_cadence_ms,
            notice_period_ms,
            agent_bond,
        )?;
        self.ensure_delegation_mandate_binding(agent, *mandate_id, timestamp_ms)?;
        if self.live_delegation_id_for_mandate(mandate_id).is_some()
            || self.pending_open_delegation_for_mandate_before(mandate_id, candidate_order)
        {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "mandate_live_delegation_exists",
            });
        }
        self.bindable_mandate_policy_ids_for_delegation(*mandate_id, agent)?;
        let pending_active_opens = self.pending_open_delegation_count_before(candidate_order, None);
        if self
            .active_delegation_count()
            .saturating_add(pending_active_opens)
            >= MAX_ACTIVE_DELEGATIONS
        {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "max_active_delegations",
            });
        }
        let pending_agent_opens =
            self.pending_open_delegation_count_before(candidate_order, Some(agent));
        if self
            .live_delegations_for_agent(agent)
            .saturating_add(pending_agent_opens)
            >= MAX_DELEGATIONS_PER_AGENT_LIVE
        {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "max_delegations_per_agent",
            });
        }
        let delegation_id = derive_delegation_id(agent, mandate_id, timestamp_ms);
        if self.delegations.contains_key(&delegation_id)
            || self.pending_open_delegation_any(&delegation_id).is_some()
        {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_id_collision",
            });
        }
        let agent_address =
            self.account_policy_address_for_delegation(agent, agent, "agent_evm_address_missing")?;
        let agent_balance = self.policy_usdc_balance_for_delegation(agent_address, agent)?;
        if agent_balance < U256::from(agent_bond) {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "insufficient_agent_policy_usdc_bond",
            });
        }
        Ok(())
    }

    fn preflight_deposit_to_delegation(
        &self,
        principal: u64,
        delegation_id: &DelegationId,
        amount: u128,
        candidate_order: &PendingDelegationActionOrder,
    ) -> Result<(), NodeError> {
        let delegation = self.delegations.get(delegation_id);
        let pending_open = delegation
            .is_none()
            .then(|| self.pending_open_delegation_before(delegation_id, candidate_order))
            .flatten();
        let agent = delegation
            .map(|delegation| delegation.agent)
            .or(pending_open.map(|(agent, _)| agent))
            .ok_or(NodeError::InvalidDelegation {
                agent: 0,
                reason: "unknown_delegation",
            })?;
        if delegation.is_some_and(|delegation| delegation.status != DelegationStatus::Open) {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_not_open",
            });
        }
        if amount < DELEGATION_MIN_DEPOSIT {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_deposit_below_min",
            });
        }
        let delegation = delegation.ok_or(NodeError::InvalidDelegation {
            agent,
            reason: "vault_not_listed",
        })?;
        self.ensure_delegation_deposit_real_capital_admission(principal, delegation, amount)?;
        let principal_address = self.account_policy_address_for_delegation(
            principal,
            agent,
            "principal_evm_address_missing",
        )?;
        let principal_balance =
            self.policy_usdc_balance_for_delegation(principal_address, agent)?;
        if principal_balance > U256::from(u128::MAX) {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "principal_balance_too_large",
            });
        }
        if principal_balance < U256::from(amount) {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "insufficient_principal_policy_usdc",
            });
        }
        let nav_per_share = self.current_delegation_nav_per_share(delegation)?;
        let shares_minted = amount
            .checked_mul(SHARE_SCALE_FACTOR)
            .and_then(|scaled| scaled.checked_div(nav_per_share))
            .filter(|shares| *shares > 0)
            .ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_share_mint_zero",
            })?;
        let prior_share = self
            .delegation_shares
            .get(&(*delegation_id, principal))
            .copied()
            .unwrap_or_default();
        prior_share
            .shares
            .checked_add(shares_minted)
            .ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_share_overflow",
            })?;
        delegation
            .total_shares
            .checked_add(shares_minted)
            .ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_share_overflow",
            })?;
        let trading_balance = self
            .policy_usdc_balance_for_delegation(delegation_trading_address(delegation_id), agent)?;
        let new_trading_balance = trading_balance.checked_add(U256::from(amount)).ok_or(
            NodeError::InvalidDelegation {
                agent,
                reason: "delegation_balance_overflow",
            },
        )?;
        if new_trading_balance > U256::from(u128::MAX) {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_balance_too_large",
            });
        }
        Ok(())
    }

    fn preflight_request_delegation_redemption(
        &self,
        principal: u64,
        delegation_id: &DelegationId,
        shares: u128,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if !matches!(
            delegation.status,
            DelegationStatus::Open
                | DelegationStatus::ClosedToDeposits
                | DelegationStatus::Winding
                | DelegationStatus::Terminated
        ) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_redeemable",
            });
        }
        if shares == 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "zero_delegation_redemption_shares",
            });
        }
        let prior_share = self
            .delegation_shares
            .get(&(*delegation_id, principal))
            .copied()
            .unwrap_or_default();
        let available = prior_share
            .shares
            .checked_sub(prior_share.locked_for_redemption)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_lock_invalid",
            })?;
        if available < shares {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "insufficient_delegation_shares",
            });
        }
        timestamp_ms
            .checked_add(delegation.notice_period_ms)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_ready_at_overflow",
            })?;
        let request_id = derive_redemption_request_id(principal, delegation_id, timestamp_ms);
        if self.redemption_requests.contains_key(&request_id) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_request_id_collision",
            });
        }
        self.current_delegation_nav_per_share(delegation)?;
        Ok(())
    }

    fn preflight_finalize_delegation_redemption(
        &self,
        request_id: &RedemptionRequestId,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let request =
            self.redemption_requests
                .get(request_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_redemption_request",
                })?;
        let delegation =
            self.delegations
                .get(&request.delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if !matches!(
            request.status,
            RedemptionRequestStatus::Pending | RedemptionRequestStatus::Ready
        ) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_request_not_pending",
            });
        }
        if timestamp_ms < request.ready_at_ms {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_request_not_ready",
            });
        }
        let prior_share = self
            .delegation_shares
            .get(&(request.delegation_id, request.principal))
            .copied()
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_missing",
            })?;
        if prior_share.locked_for_redemption < request.shares_requested
            || prior_share.shares < request.shares_requested
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_lock_invalid",
            });
        }
        let nav_per_share = self.current_delegation_nav_per_share(delegation)?;
        let usdc_out = request
            .shares_requested
            .checked_mul(nav_per_share)
            .and_then(|scaled| scaled.checked_div(SHARE_SCALE_FACTOR))
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_value_overflow",
            })?;
        let trading_balance = self.policy_usdc_balance_for_delegation(
            delegation_trading_address(&request.delegation_id),
            delegation.agent,
        )?;
        if trading_balance >= U256::from(usdc_out) {
            let compensation_out = self.delegation_compensation_release(
                request.delegation_id,
                request.principal,
                request.shares_requested,
                prior_share.shares,
            )?;
            let total_principal_out =
                usdc_out
                    .checked_add(compensation_out)
                    .ok_or(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "redemption_value_overflow",
                    })?;
            let principal_address = self.account_policy_address_for_delegation(
                request.principal,
                delegation.agent,
                "principal_evm_address_missing",
            )?;
            let principal_balance =
                self.policy_usdc_balance_for_delegation(principal_address, delegation.agent)?;
            let new_principal_balance = principal_balance
                .checked_add(U256::from(total_principal_out))
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "principal_balance_overflow",
                })?;
            if new_principal_balance > U256::from(u128::MAX) {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "principal_balance_too_large",
                });
            }
        }
        Ok(())
    }

    fn preflight_cancel_delegation_redemption(
        &self,
        principal: u64,
        request_id: &RedemptionRequestId,
    ) -> Result<(), NodeError> {
        let request =
            self.redemption_requests
                .get(request_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_redemption_request",
                })?;
        let delegation =
            self.delegations
                .get(&request.delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if !matches!(
            request.status,
            RedemptionRequestStatus::Pending | RedemptionRequestStatus::Ready
        ) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_request_not_pending",
            });
        }
        if request.principal != principal {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_principal_mismatch",
            });
        }
        let prior_share = self
            .delegation_shares
            .get(&(request.delegation_id, principal))
            .copied()
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_missing",
            })?;
        if prior_share.locked_for_redemption < request.shares_requested {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_lock_invalid",
            });
        }
        Ok(())
    }

    fn preflight_close_delegation_to_deposits(
        &self,
        delegation_id: &DelegationId,
        candidate_order: &PendingDelegationActionOrder,
    ) -> Result<(), NodeError> {
        if self
            .pending_open_delegation_before(delegation_id, candidate_order)
            .is_some()
        {
            return Ok(());
        }
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if delegation.status != DelegationStatus::Open {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_open",
            });
        }
        Ok(())
    }

    fn preflight_begin_delegation_winding(
        &self,
        delegation_id: &DelegationId,
        candidate_order: &PendingDelegationActionOrder,
    ) -> Result<(), NodeError> {
        if self
            .pending_open_delegation_before(delegation_id, candidate_order)
            .is_some()
        {
            return Ok(());
        }
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if !matches!(
            delegation.status,
            DelegationStatus::Open | DelegationStatus::ClosedToDeposits
        ) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_windable",
            });
        }
        Ok(())
    }

    fn preflight_delegation_insurance_payout(
        &self,
        delegation_id: &DelegationId,
        requested_amount_micro_usdc: u128,
    ) -> Result<(), NodeError> {
        self.preview_delegation_insurance_payout(*delegation_id, requested_amount_micro_usdc)?;
        Ok(())
    }

    fn preflight_force_liquidate_delegation(
        &self,
        action_id: DelegationActionId,
        delegation_id: &DelegationId,
        market_id: u64,
        keeper_account: u64,
        requested_lots: Option<u64>,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        self.preview_delegation_forced_liquidation(
            action_id,
            delegation_id,
            market_id,
            keeper_account,
            requested_lots,
            timestamp_ms,
        )?;
        Ok(())
    }

    fn preflight_close_delegation(
        &self,
        delegation_id: &DelegationId,
        candidate_order: &PendingDelegationActionOrder,
    ) -> Result<(), NodeError> {
        if self
            .pending_open_delegation_before(delegation_id, candidate_order)
            .is_some()
        {
            return Ok(());
        }
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if !matches!(
            delegation.status,
            DelegationStatus::Open | DelegationStatus::ClosedToDeposits | DelegationStatus::Winding
        ) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_closable",
            });
        }
        let pending_finalize =
            self.pending_finalize_for_delegation_before(delegation_id, candidate_order);
        if delegation.total_shares != 0 && !pending_finalize {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_outstanding_shares",
            });
        }
        if self.has_active_delegation_redemption(delegation_id) && !pending_finalize {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_active_redemptions",
            });
        }
        let trading_account = delegation_trading_account_id(delegation_id);
        if !self.positions(trading_account, false, 1).is_empty() {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_open_positions",
            });
        }
        if self.delegation_free_usdc_balance(delegation_id)? != 0 && !pending_finalize {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_free_balance",
            });
        }
        if delegation.agent_bond != 0 {
            let escrowed_bond = self.delegation_bond_escrow_amount(delegation)?;
            let agent_address = self.account_policy_address_for_delegation(
                delegation.agent,
                delegation.agent,
                "agent_evm_address_missing",
            )?;
            let agent_balance =
                self.policy_usdc_balance_for_delegation(agent_address, delegation.agent)?;
            let new_agent_balance = agent_balance.checked_add(U256::from(escrowed_bond)).ok_or(
                NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "agent_balance_overflow",
                },
            )?;
            if new_agent_balance > U256::from(u128::MAX) {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "agent_balance_too_large",
                });
            }
        }
        self.preflight_live_delegation_market_maker_exits(delegation_id)?;
        Ok(())
    }

    fn preflight_terminate_delegation(
        &self,
        delegation_id: &DelegationId,
        reason: BlockDelegationFaultReason,
        slash_policy: Option<BlockDelegationSlashPolicy>,
    ) -> Result<(), NodeError> {
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        delegation_termination_slash_policy(reason, slash_policy, delegation.agent)?;
        if !matches!(
            delegation.status,
            DelegationStatus::Open | DelegationStatus::ClosedToDeposits | DelegationStatus::Winding
        ) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_terminable",
            });
        }
        self.delegation_bond_escrow_amount(delegation)?;
        self.preflight_live_delegation_market_maker_exits(delegation_id)?;
        Ok(())
    }

    fn pending_open_delegation_before(
        &self,
        delegation_id: &DelegationId,
        candidate_order: &PendingDelegationActionOrder,
    ) -> Option<(u64, MandateId)> {
        self.pending_open_delegation_index
            .get(delegation_id)?
            .range(..*candidate_order)
            .next_back()
            .and_then(|order| self.pending_delegation_actions.get(&order.action_id))
            .and_then(|pending| match &pending.action {
                BlockDelegationAction::Open {
                    agent, mandate_id, ..
                } => Some((*agent, *mandate_id)),
                _ => None,
            })
    }

    fn pending_open_delegation_any(
        &self,
        delegation_id: &DelegationId,
    ) -> Option<(u64, MandateId)> {
        self.pending_open_delegation_index
            .get(delegation_id)?
            .iter()
            .next()
            .and_then(|order| self.pending_delegation_actions.get(&order.action_id))
            .and_then(|pending| match &pending.action {
                BlockDelegationAction::Open {
                    agent, mandate_id, ..
                } => Some((*agent, *mandate_id)),
                _ => None,
            })
    }

    fn pending_open_delegation_count_before(
        &self,
        candidate_order: &PendingDelegationActionOrder,
        agent: Option<u64>,
    ) -> usize {
        let Some(agent) = agent else {
            return self
                .pending_open_delegation_order
                .range(..*candidate_order)
                .count();
        };
        self.pending_open_delegation_agent_index
            .get(&agent)
            .map(|orders| orders.range(..*candidate_order).count())
            .unwrap_or(0)
    }

    fn pending_open_delegation_for_mandate_before(
        &self,
        mandate_id: &MandateId,
        candidate_order: &PendingDelegationActionOrder,
    ) -> bool {
        self.pending_open_delegation_order
            .range(..*candidate_order)
            .filter_map(|order| self.pending_delegation_actions.get(&order.action_id))
            .any(|pending| {
                matches!(
                    &pending.action,
                    BlockDelegationAction::Open { mandate_id: pending_mandate_id, .. }
                        if pending_mandate_id == mandate_id
                )
            })
    }

    fn pending_finalize_for_delegation_before(
        &self,
        delegation_id: &DelegationId,
        candidate_order: &PendingDelegationActionOrder,
    ) -> bool {
        self.pending_finalize_delegation_index
            .get(delegation_id)
            .is_some_and(|orders| orders.range(..*candidate_order).next().is_some())
    }

    pub(crate) fn rebuild_pending_delegation_action_indexes(&mut self) {
        self.pending_open_delegation_index.clear();
        self.pending_open_delegation_agent_index.clear();
        self.pending_open_delegation_order.clear();
        self.pending_finalize_delegation_index.clear();
        let pending_actions = self
            .pending_delegation_actions
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for pending in &pending_actions {
            self.index_pending_delegation_action(pending);
        }
    }

    fn index_pending_delegation_action(&mut self, pending: &PendingDelegationAction) {
        let order = pending_delegation_action_order(pending);
        match &pending.action {
            BlockDelegationAction::Open {
                agent, mandate_id, ..
            } => {
                let delegation_id =
                    derive_delegation_id(*agent, mandate_id, pending.submitted_at_ms);
                self.pending_open_delegation_index
                    .entry(delegation_id)
                    .or_default()
                    .insert(order);
                self.pending_open_delegation_agent_index
                    .entry(*agent)
                    .or_default()
                    .insert(order);
                self.pending_open_delegation_order.insert(order);
            }
            BlockDelegationAction::FinalizeRedemption { request_id } => {
                let request = self.redemption_requests.get(request_id);
                debug_assert!(
                    request.is_some(),
                    "pending FinalizeRedemption must reference a known request"
                );
                if let Some(request) = request {
                    self.pending_finalize_delegation_index
                        .entry(request.delegation_id)
                        .or_default()
                        .insert(order);
                }
            }
            _ => {}
        }
    }

    fn unindex_pending_delegation_action(&mut self, pending: &PendingDelegationAction) {
        let order = pending_delegation_action_order(pending);
        match &pending.action {
            BlockDelegationAction::Open {
                agent, mandate_id, ..
            } => {
                let delegation_id =
                    derive_delegation_id(*agent, mandate_id, pending.submitted_at_ms);
                remove_pending_delegation_index_order(
                    &mut self.pending_open_delegation_index,
                    &delegation_id,
                    &order,
                );
                remove_pending_delegation_index_order(
                    &mut self.pending_open_delegation_agent_index,
                    agent,
                    &order,
                );
                self.pending_open_delegation_order.remove(&order);
            }
            BlockDelegationAction::FinalizeRedemption { request_id } => {
                let request = self.redemption_requests.get(request_id);
                debug_assert!(
                    request.is_some(),
                    "pending FinalizeRedemption must reference a known request"
                );
                if let Some(request) = request {
                    remove_pending_delegation_index_order(
                        &mut self.pending_finalize_delegation_index,
                        &request.delegation_id,
                        &order,
                    );
                }
            }
            _ => {}
        }
    }

    fn debug_assert_pending_delegation_indexes(&self) {
        #[cfg(debug_assertions)]
        {
            let mut expected_order = BTreeSet::new();
            let mut expected_open_index: BTreeMap<
                DelegationId,
                BTreeSet<PendingDelegationActionOrder>,
            > = BTreeMap::new();
            let mut expected_open_agent_index: BTreeMap<
                u64,
                BTreeSet<PendingDelegationActionOrder>,
            > = BTreeMap::new();
            let mut expected_open_order = BTreeSet::new();
            let mut expected_finalize_index: BTreeMap<
                DelegationId,
                BTreeSet<PendingDelegationActionOrder>,
            > = BTreeMap::new();
            for pending in self.pending_delegation_actions.values() {
                let order = pending_delegation_action_order(pending);
                expected_order.insert(order);
                match &pending.action {
                    BlockDelegationAction::Open {
                        agent, mandate_id, ..
                    } => {
                        let delegation_id =
                            derive_delegation_id(*agent, mandate_id, pending.submitted_at_ms);
                        expected_open_index
                            .entry(delegation_id)
                            .or_default()
                            .insert(order);
                        expected_open_agent_index
                            .entry(*agent)
                            .or_default()
                            .insert(order);
                        expected_open_order.insert(order);
                    }
                    BlockDelegationAction::FinalizeRedemption { request_id } => {
                        let request = self
                            .redemption_requests
                            .get(request_id)
                            .expect("pending FinalizeRedemption must reference a known request");
                        expected_finalize_index
                            .entry(request.delegation_id)
                            .or_default()
                            .insert(order);
                    }
                    _ => {}
                }
            }
            debug_assert_eq!(self.pending_delegation_action_order, expected_order);
            debug_assert_eq!(self.pending_open_delegation_index, expected_open_index);
            debug_assert_eq!(
                self.pending_open_delegation_agent_index,
                expected_open_agent_index
            );
            debug_assert_eq!(self.pending_open_delegation_order, expected_open_order);
            debug_assert_eq!(
                self.pending_finalize_delegation_index,
                expected_finalize_index
            );
        }
    }

    pub fn pending_delegation_action_count(&self) -> usize {
        self.pending_delegation_actions.len()
    }

    pub fn pending_delegation_action(
        &self,
        action_id: DelegationActionId,
    ) -> Option<&PendingDelegationAction> {
        self.pending_delegation_actions.get(&action_id)
    }

    pub(crate) fn pending_delegation_block_items(&self, limit: usize) -> Vec<BlockItem> {
        self.pending_delegation_action_order
            .iter()
            .take(limit)
            .filter_map(|order| self.pending_delegation_actions.get(&order.action_id))
            .map(|pending| {
                BlockItem::Delegation(BlockDelegation {
                    action_id: pending.action_id,
                    timestamp_ms: pending.submitted_at_ms,
                    action: pending.action.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn remove_pending_delegation_actions_in_block(&mut self, body: &BlockBody) {
        for item in &body.items {
            let BlockItem::Delegation(delegation) = item else {
                continue;
            };
            if let Some(pending) = self
                .pending_delegation_actions
                .remove(&delegation.action_id)
            {
                self.unindex_pending_delegation_action(&pending);
                self.pending_delegation_action_order
                    .remove(&PendingDelegationActionOrder {
                        submitted_at_ms: pending.submitted_at_ms,
                        action_id: pending.action_id,
                    });
                self.debug_assert_pending_delegation_indexes();
            }
        }
    }

    pub fn processed_market_maker_program_action(
        &self,
        action_id: MarketMakerProgramActionId,
    ) -> Option<&ProcessedMarketMakerProgramAction> {
        self.processed_market_maker_program_actions.get(&action_id)
    }

    pub fn pending_market_maker_program_action_count(&self) -> usize {
        self.pending_market_maker_program_actions.len()
    }

    pub fn pending_market_maker_program_action(
        &self,
        action_id: MarketMakerProgramActionId,
    ) -> Option<&PendingMarketMakerProgramAction> {
        self.pending_market_maker_program_actions.get(&action_id)
    }

    pub fn submit_market_maker_program_action(
        &mut self,
        action_id: MarketMakerProgramActionId,
        action: BlockMarketMakerProgramAction,
        submitted_at_ms: u64,
    ) -> Result<MarketMakerProgramActionSubmissionStatus, NodeError> {
        let fingerprint = market_maker_program_action_fingerprint(&action);
        if let Some(processed) = self.processed_market_maker_program_actions.get(&action_id) {
            if processed.fingerprint != fingerprint {
                return Err(NodeError::InvalidMarketMakerProgram {
                    program_id: market_maker_program_action_program_id(&action),
                    reason: "market_maker_program_action_id_mismatch",
                });
            }
            return Ok(MarketMakerProgramActionSubmissionStatus::AlreadyProcessed);
        }
        if let Some(pending) = self.pending_market_maker_program_actions.get(&action_id) {
            if pending.fingerprint != fingerprint {
                return Err(NodeError::InvalidMarketMakerProgram {
                    program_id: market_maker_program_action_program_id(&action),
                    reason: "market_maker_program_action_id_mismatch",
                });
            }
            return Ok(MarketMakerProgramActionSubmissionStatus::DuplicatePending);
        }
        self.preflight_pending_market_maker_program_action(&action)?;
        if self.pending_market_maker_program_actions.len()
            >= MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS
        {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&action),
                reason: "market_maker_program_action_queue_full",
            });
        }
        let pending = PendingMarketMakerProgramAction {
            action_id,
            action,
            fingerprint,
            submitted_at_ms,
        };
        self.pending_market_maker_program_action_order
            .insert(pending_market_maker_program_action_order(&pending));
        self.pending_market_maker_program_actions
            .insert(action_id, pending);
        Ok(MarketMakerProgramActionSubmissionStatus::Accepted)
    }

    pub fn submit_signed_market_maker_program_action(
        &mut self,
        signed_action: SignedMarketMakerProgramAction,
    ) -> Result<MarketMakerProgramActionSubmissionStatus, NodeError> {
        self.verify_signed_market_maker_program_action(&signed_action)?;
        if !self
            .pending_market_maker_program_actions
            .contains_key(&signed_action.action_id)
            && !self
                .processed_market_maker_program_actions
                .contains_key(&signed_action.action_id)
            && self.pending_market_maker_program_action_count_for_signer(
                signed_action.signer_account_id,
            ) >= MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS_PER_SIGNER
        {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "max_pending_market_maker_program_actions_per_signer",
            });
        }
        self.submit_market_maker_program_action(
            signed_action.action_id,
            signed_action.action,
            signed_action.timestamp_ms,
        )
    }

    fn pending_market_maker_program_action_count_for_signer(
        &self,
        signer_account_id: u64,
    ) -> usize {
        self.pending_market_maker_program_actions
            .values()
            .filter(|pending| {
                self.market_maker_program_action_signer_account(&pending.action)
                    .is_ok_and(|account_id| account_id == signer_account_id)
            })
            .count()
    }

    fn verify_signed_market_maker_program_action(
        &self,
        signed_action: &SignedMarketMakerProgramAction,
    ) -> Result<(), NodeError> {
        if signed_action.chain_id != self.chain_id() {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "chain_mismatch",
            });
        }
        let expected_action_id = market_maker_program_action_id_for_submission(
            &signed_action.action,
            signed_action.timestamp_ms,
            signed_action.submitter_nonce,
        );
        if signed_action.action_id != expected_action_id {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "market_maker_program_action_id_mismatch",
            });
        }
        let expected_signer =
            self.market_maker_program_action_signer_account(&signed_action.action)?;
        if signed_action.signer_account_id != expected_signer {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "market_maker_program_action_signer_mismatch",
            });
        }
        let account = self
            .account_store
            .get(signed_action.signer_account_id)
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "unknown_market_maker_program_action_signer",
            })?;
        let verifying_key = VerifyingKey::from_bytes(&account.owner()).map_err(|_| {
            NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "invalid_market_maker_program_action_signer",
            }
        })?;
        let signature = Ed25519Signature::from_bytes(&signed_action.signature);
        verifying_key
            .verify_strict(
                &market_maker_program_action_signing_bytes(signed_action),
                &signature,
            )
            .map_err(|_| NodeError::InvalidMarketMakerProgram {
                program_id: market_maker_program_action_program_id(&signed_action.action),
                reason: "invalid_market_maker_program_action_signature",
            })?;
        Ok(())
    }

    fn market_maker_program_action_signer_account(
        &self,
        action: &BlockMarketMakerProgramAction,
    ) -> Result<u64, NodeError> {
        match action {
            BlockMarketMakerProgramAction::Enroll {
                market_maker_identity,
                ..
            }
            | BlockMarketMakerProgramAction::Exit {
                market_maker_identity,
                ..
            } => Ok(market_maker_identity_signer_account(*market_maker_identity)),
            BlockMarketMakerProgramAction::Define { .. }
            | BlockMarketMakerProgramAction::Retire { .. } => {
                Err(NodeError::InvalidMarketMakerProgram {
                    program_id: market_maker_program_action_program_id(action),
                    reason: "market_maker_governance_signer_not_configured",
                })
            }
        }
    }

    fn preflight_pending_market_maker_program_action(
        &self,
        action: &BlockMarketMakerProgramAction,
    ) -> Result<(), NodeError> {
        match action {
            BlockMarketMakerProgramAction::Define {
                name,
                markets,
                min_uptime_bps,
                max_spread_bps,
                min_quote_size_micro_usdc,
                min_two_sided_windows_per_hour,
                rebate_multiplier_units,
                enrollment_bond_micro_usdc,
                compliance_window_ms,
                observation_cadence_ms,
            } => {
                let input = DefineMarketMakerProgramInput {
                    name: name.clone(),
                    markets: markets.clone(),
                    min_uptime_bps: *min_uptime_bps,
                    max_spread_bps: *max_spread_bps,
                    min_quote_size_micro_usdc: MarketMakerU256(*min_quote_size_micro_usdc),
                    min_two_sided_windows_per_hour: *min_two_sided_windows_per_hour,
                    rebate_multiplier_units: *rebate_multiplier_units,
                    enrollment_bond_micro_usdc: MarketMakerU256(*enrollment_bond_micro_usdc),
                    compliance_window_ms: *compliance_window_ms,
                    observation_cadence_ms: *observation_cadence_ms,
                };
                validate_market_maker_program_input(&input, &self.markets)?;
                let program_id = derive_market_maker_program_id(&input);
                if matches!(
                    self.market_maker_programs
                        .get(&program_id)
                        .map(|program| program.status),
                    Some(MarketMakerProgramStatus::Active)
                ) {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "program_already_active",
                    });
                }
                if self
                    .market_maker_programs
                    .values()
                    .filter(|program| program.status == MarketMakerProgramStatus::Active)
                    .count()
                    >= MAX_MM_PROGRAMS
                {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "max_market_maker_programs",
                    });
                }
                Ok(())
            }
            BlockMarketMakerProgramAction::Retire { program_id } => {
                let program = self.market_maker_programs.get(program_id).ok_or(
                    NodeError::InvalidMarketMakerProgram {
                        program_id: *program_id,
                        reason: "unknown_program",
                    },
                )?;
                if program.status != MarketMakerProgramStatus::Active {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id: *program_id,
                        reason: "program_not_active",
                    });
                }
                Ok(())
            }
            BlockMarketMakerProgramAction::Enroll {
                market_maker_identity,
                market_id,
                program_id,
                bond_micro_usdc,
            } => {
                let input = EnrollMarketMakerInput {
                    market_maker_identity: *market_maker_identity,
                    market_id: *market_id,
                    program_id: *program_id,
                    bond_micro_usdc: MarketMakerU256(*bond_micro_usdc),
                };
                self.validate_market_maker_enrollment_input(&input)?;
                let address = self.account_policy_address_for_market_maker_identity(
                    *market_maker_identity,
                    *program_id,
                    "market_maker_evm_address_missing",
                )?;
                let balance = self.policy_usdc_balance_for_market_maker(address, *program_id)?;
                let bond = input.bond_micro_usdc.as_u128().ok_or(
                    NodeError::InvalidMarketMakerProgram {
                        program_id: *program_id,
                        reason: "enrollment_bond_overflow",
                    },
                )?;
                if balance < U256::from(bond) {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id: *program_id,
                        reason: "insufficient_market_maker_policy_usdc_bond",
                    });
                }
                Ok(())
            }
            BlockMarketMakerProgramAction::Exit {
                market_maker_identity,
                market_id,
                program_id,
            } => {
                let key =
                    market_maker_enrollment_key(*market_maker_identity, *market_id, *program_id);
                let enrollment = self.market_maker_enrollments.get(&key).ok_or(
                    NodeError::InvalidMarketMakerProgram {
                        program_id: *program_id,
                        reason: "unknown_enrollment",
                    },
                )?;
                if !market_maker_enrollment_status_is_live(enrollment.status) {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id: *program_id,
                        reason: "enrollment_not_live",
                    });
                }
                Ok(())
            }
        }
    }

    // Contract for EngineHandle::pending_market_maker_program_payload:
    // this helper returns only MarketMakerProgram block items.
    pub(crate) fn pending_market_maker_program_block_items(&self, limit: usize) -> Vec<BlockItem> {
        self.pending_market_maker_program_action_order
            .iter()
            .take(limit)
            .filter_map(|order| {
                self.pending_market_maker_program_actions
                    .get(&order.action_id)
            })
            .map(|pending| {
                BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
                    action_id: pending.action_id,
                    timestamp_ms: pending.submitted_at_ms,
                    action: pending.action.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn remove_pending_market_maker_program_actions_in_block(
        &mut self,
        body: &BlockBody,
    ) {
        for item in &body.items {
            let BlockItem::MarketMakerProgram(program) = item else {
                continue;
            };
            if let Some(pending) = self
                .pending_market_maker_program_actions
                .remove(&program.action_id)
            {
                self.pending_market_maker_program_action_order.remove(
                    &PendingMarketMakerProgramActionOrder {
                        submitted_at_ms: pending.submitted_at_ms,
                        action_id: pending.action_id,
                    },
                );
            }
        }
    }

    pub fn submit_tee_governance_action(
        &mut self,
        action_id: TeeGovernanceActionId,
        action: BlockTeeGovernanceAction,
        submitted_at_ms: u64,
    ) -> Result<TeeGovernanceActionSubmissionStatus, NodeError> {
        let fingerprint = tee_governance_action_fingerprint(&action);
        if let Some(processed) = self.processed_tee_governance_actions.get(&action_id) {
            if processed.fingerprint != fingerprint {
                return Err(NodeError::InvalidTeeGovernance {
                    action_id,
                    reason: "tee_governance_action_id_mismatch",
                });
            }
            return Ok(TeeGovernanceActionSubmissionStatus::AlreadyProcessed);
        }
        if let Some(pending) = self.pending_tee_governance_actions.get(&action_id) {
            if pending.fingerprint != fingerprint {
                return Err(NodeError::InvalidTeeGovernance {
                    action_id,
                    reason: "tee_governance_action_id_mismatch",
                });
            }
            return Ok(TeeGovernanceActionSubmissionStatus::DuplicatePending);
        }
        self.preflight_pending_tee_governance_action(action_id, &action)?;
        if self.pending_tee_governance_actions.len() >= MAX_PENDING_TEE_GOVERNANCE_ACTIONS {
            return Err(NodeError::InvalidTeeGovernance {
                action_id,
                reason: "tee_governance_action_queue_full",
            });
        }
        let pending = PendingTeeGovernanceAction {
            action_id,
            action,
            fingerprint,
            submitted_at_ms,
        };
        self.pending_tee_governance_action_order
            .insert(pending_tee_governance_action_order(&pending));
        self.pending_tee_governance_actions
            .insert(action_id, pending);
        Ok(TeeGovernanceActionSubmissionStatus::Accepted)
    }

    fn preflight_pending_tee_governance_action(
        &self,
        action_id: TeeGovernanceActionId,
        action: &BlockTeeGovernanceAction,
    ) -> Result<(), NodeError> {
        if let BlockTeeGovernanceAction::UpdateVendorCapProfile { vendor, .. } = action {
            // Admission guard: another cap-profile update for this vendor is
            // queued locally but has not committed yet. Committed block
            // validation separately checks scheduled notice-window updates.
            let vendor = tee_vendor_from_block(*vendor);
            if self.pending_tee_governance_actions.values().any(|pending| {
                matches!(
                    &pending.action,
                    BlockTeeGovernanceAction::UpdateVendorCapProfile {
                        vendor: pending_vendor,
                        ..
                    } if tee_vendor_from_block(*pending_vendor) == vendor
                )
            }) {
                return Err(NodeError::InvalidTeeGovernance {
                    action_id,
                    reason: "vendor_cap_profile_update_already_pending",
                });
            }
        }
        self.validate_block_tee_governance_action(
            action_id,
            action,
            self.head().header.height.saturating_add(1),
        )
    }

    pub(crate) fn pending_tee_governance_block_items(&self, limit: usize) -> Vec<BlockItem> {
        self.pending_tee_governance_action_order
            .iter()
            .take(limit)
            .filter_map(|order| self.pending_tee_governance_actions.get(&order.action_id))
            .map(|pending| {
                BlockItem::TeeGovernance(BlockTeeGovernance {
                    action_id: pending.action_id,
                    timestamp_ms: pending.submitted_at_ms,
                    action: pending.action.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn remove_pending_tee_governance_actions_in_block(&mut self, body: &BlockBody) {
        for item in &body.items {
            let BlockItem::TeeGovernance(governance) = item else {
                continue;
            };
            if let Some(pending) = self
                .pending_tee_governance_actions
                .remove(&governance.action_id)
            {
                self.pending_tee_governance_action_order
                    .remove(&PendingTeeGovernanceActionOrder {
                        submitted_at_ms: pending.submitted_at_ms,
                        action_id: pending.action_id,
                    });
            }
        }
    }

    pub fn submit_real_capital_governance_action(
        &mut self,
        action_id: RealCapitalGovernanceActionId,
        action: BlockRealCapitalGovernanceAction,
        submitted_at_ms: u64,
    ) -> Result<RealCapitalGovernanceActionSubmissionStatus, NodeError> {
        let fingerprint = real_capital_governance_action_fingerprint(&action);
        if let Some(processed) = self
            .processed_real_capital_governance_actions
            .get(&action_id)
        {
            if processed.fingerprint != fingerprint {
                return Err(NodeError::InvalidRealCapitalGovernance {
                    action_id,
                    reason: "real_capital_governance_action_id_mismatch",
                });
            }
            return Ok(RealCapitalGovernanceActionSubmissionStatus::AlreadyProcessed);
        }
        if let Some(pending) = self.pending_real_capital_governance_actions.get(&action_id) {
            if pending.fingerprint != fingerprint {
                return Err(NodeError::InvalidRealCapitalGovernance {
                    action_id,
                    reason: "real_capital_governance_action_id_mismatch",
                });
            }
            return Ok(RealCapitalGovernanceActionSubmissionStatus::DuplicatePending);
        }
        let head_timestamp_ms = self.head().header.timestamp_ms;
        if submitted_at_ms.saturating_add(REAL_CAPITAL_GOVERNANCE_SUBMISSION_TIMESTAMP_WINDOW_MS)
            < head_timestamp_ms
            || submitted_at_ms
                > head_timestamp_ms
                    .saturating_add(REAL_CAPITAL_GOVERNANCE_SUBMISSION_TIMESTAMP_WINDOW_MS)
        {
            return Err(NodeError::InvalidRealCapitalGovernance {
                action_id,
                reason: "real_capital_governance_timestamp_stale",
            });
        }
        self.validate_block_real_capital_governance_action(
            action_id,
            &action,
            self.head().header.height.saturating_add(1),
        )?;
        if self.pending_real_capital_governance_actions.len()
            >= MAX_PENDING_REAL_CAPITAL_GOVERNANCE_ACTIONS
        {
            return Err(NodeError::InvalidRealCapitalGovernance {
                action_id,
                reason: "real_capital_governance_action_queue_full",
            });
        }
        let pending = PendingRealCapitalGovernanceAction {
            action_id,
            action,
            fingerprint,
            submitted_at_ms,
        };
        self.pending_real_capital_governance_action_order
            .insert(pending_real_capital_governance_action_order(&pending));
        self.pending_real_capital_governance_actions
            .insert(action_id, pending);
        Ok(RealCapitalGovernanceActionSubmissionStatus::Accepted)
    }

    pub fn pending_real_capital_governance_action_count(&self) -> usize {
        self.pending_real_capital_governance_actions.len()
    }

    pub fn processed_real_capital_governance_action(
        &self,
        action_id: RealCapitalGovernanceActionId,
    ) -> Option<&ProcessedRealCapitalGovernanceAction> {
        self.processed_real_capital_governance_actions
            .get(&action_id)
    }

    pub fn pending_real_capital_governance_action(
        &self,
        action_id: RealCapitalGovernanceActionId,
    ) -> Option<&PendingRealCapitalGovernanceAction> {
        self.pending_real_capital_governance_actions.get(&action_id)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn test_apply_real_capital_governance_block_item(
        &mut self,
        action_id: RealCapitalGovernanceActionId,
        timestamp_ms: u64,
        action: BlockRealCapitalGovernanceAction,
    ) {
        // Test fixture only. This bypasses RPC admission and committed-block
        // construction so tests can exercise block-apply rejection receipts.
        // It must not be used by production RPC paths or local submission flows.
        let parent = self.head().clone();
        let body = BlockBody {
            items: vec![BlockItem::RealCapitalGovernance(
                BlockRealCapitalGovernance {
                    action_id,
                    timestamp_ms,
                    action,
                },
            )],
        };
        let block = Block {
            header: BlockHeader {
                chain_id: parent.header.chain_id,
                height: parent.header.height.saturating_add(1),
                round: parent.header.round.saturating_add(1),
                proposer: self.local_validator_id().unwrap_or(parent.header.proposer),
                parent_hash: block_hash(&parent),
                parent_state_root: parent.header.state_root,
                tx_root: body_root(&body),
                event_root: parent.header.event_root,
                state_root: parent.header.state_root,
                qc: parent.header.qc,
                timestamp_ms,
            },
            body,
        };
        self.apply_block_real_capital_governance_items(&block);
    }

    pub(crate) fn pending_real_capital_governance_block_items(
        &self,
        limit: usize,
    ) -> Vec<BlockItem> {
        self.pending_real_capital_governance_action_order
            .iter()
            .take(limit)
            .filter_map(|order| {
                self.pending_real_capital_governance_actions
                    .get(&order.action_id)
            })
            .map(|pending| {
                BlockItem::RealCapitalGovernance(BlockRealCapitalGovernance {
                    action_id: pending.action_id,
                    timestamp_ms: pending.submitted_at_ms,
                    action: pending.action.clone(),
                })
            })
            .collect()
    }

    pub(crate) fn remove_pending_real_capital_governance_actions_in_block(
        &mut self,
        body: &BlockBody,
    ) {
        for item in &body.items {
            let BlockItem::RealCapitalGovernance(governance) = item else {
                continue;
            };
            if let Some(pending) = self
                .pending_real_capital_governance_actions
                .remove(&governance.action_id)
            {
                self.pending_real_capital_governance_action_order.remove(
                    &PendingRealCapitalGovernanceActionOrder {
                        submitted_at_ms: pending.submitted_at_ms,
                        action_id: pending.action_id,
                    },
                );
            }
        }
    }

    pub fn submit_faucet_grant(
        &mut self,
        address: [u8; 20],
        policy_target: U256,
        evm_target: U256,
        submitted_at_ms: u64,
    ) -> Result<FaucetGrantSubmissionStatus, &'static str> {
        if self.faucet_targets_reached(address, policy_target, evm_target) {
            return Ok(FaucetGrantSubmissionStatus::AlreadyFunded);
        }
        if self.pending_faucet_grants.contains_key(&address) {
            return Ok(FaucetGrantSubmissionStatus::DuplicatePending);
        }
        let pending = PendingFaucetGrant {
            address,
            policy_target_micro_usdc: policy_target.to_be_bytes::<32>(),
            evm_target_micro_usdc: evm_target.to_be_bytes::<32>(),
            submitted_at_ms,
        };
        self.pending_faucet_grant_order
            .insert(pending_faucet_grant_order(&pending));
        self.pending_faucet_grants.insert(address, pending);
        Ok(FaucetGrantSubmissionStatus::Accepted)
    }

    pub fn pending_faucet_grant_count(&self) -> usize {
        self.pending_faucet_grants.len()
    }

    pub fn faucet_targets_reached(
        &self,
        address: [u8; 20],
        policy_target: U256,
        evm_target: U256,
    ) -> bool {
        let policy_current = self
            .get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let evm_current = self
            .get_committed_state(&evm_usdc_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        policy_current >= policy_target && evm_current >= evm_target
    }

    // Contract for EngineHandle::pending_faucet_payload:
    // this helper returns only Faucet block items.
    pub(crate) fn pending_faucet_block_items(&self, limit: usize) -> Vec<BlockItem> {
        self.pending_faucet_grant_order
            .iter()
            .take(limit)
            .filter_map(|order| self.pending_faucet_grants.get(&order.address))
            .map(|pending| {
                BlockItem::Faucet(BlockFaucet {
                    address: pending.address,
                    policy_target_micro_usdc: pending.policy_target_micro_usdc,
                    evm_target_micro_usdc: pending.evm_target_micro_usdc,
                })
            })
            .collect()
    }

    pub(crate) fn remove_pending_faucet_grants_in_block(&mut self, body: &BlockBody) {
        for item in &body.items {
            let BlockItem::Faucet(faucet) = item else {
                continue;
            };
            if let Some(pending) = self.pending_faucet_grants.remove(&faucet.address) {
                self.pending_faucet_grant_order
                    .remove(&pending_faucet_grant_order(&pending));
            }
        }
    }
}
