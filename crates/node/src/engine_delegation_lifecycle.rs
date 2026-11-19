use super::*;

impl SingleNodeEngine {
    pub fn close_delegation_to_deposits(
        &mut self,
        delegation_id: DelegationId,
        timestamp_ms: u64,
    ) -> Result<Delegation, NodeError> {
        let mut delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
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
        let old_status = delegation.status;
        delegation.status = DelegationStatus::ClosedToDeposits;
        delegation.last_updated_at_ms = timestamp_ms;
        self.delegations.insert(delegation_id, delegation.clone());
        self.record_delegation_status_changed(
            &delegation,
            old_status,
            DelegationStatusReason::ClosedToDeposits,
            0,
            0,
            0,
            0,
            timestamp_ms,
        );
        Ok(delegation)
    }

    pub fn begin_delegation_winding(
        &mut self,
        delegation_id: DelegationId,
        timestamp_ms: u64,
    ) -> Result<Delegation, NodeError> {
        let mut delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
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
        let old_status = delegation.status;
        delegation.status = DelegationStatus::Winding;
        delegation.last_updated_at_ms = timestamp_ms;
        delegation.winding_started_at_ms = Some(timestamp_ms);
        self.delegations.insert(delegation_id, delegation.clone());
        self.apply_delegation_policy_lifecycle_state(&delegation, old_status, timestamp_ms, None)?;
        self.record_delegation_status_changed(
            &delegation,
            old_status,
            DelegationStatusReason::WindingBegun,
            0,
            0,
            0,
            0,
            timestamp_ms,
        );
        Ok(delegation)
    }

    pub(super) fn preview_delegation_forced_liquidation(
        &self,
        action_id: DelegationActionId,
        delegation_id: &DelegationId,
        market_id: u64,
        keeper_account: u64,
        requested_lots: Option<u64>,
        timestamp_ms: u64,
    ) -> Result<DelegationForcedLiquidationPreview, NodeError> {
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if !delegation_accepts_trading_intents(delegation.status) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_forced_liquidatable",
            });
        }
        if account_id_is_delegation_reserved(keeper_account) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "reserved_delegation_keeper_account",
            });
        }
        if self.account_frozen(keeper_account) {
            return Err(NodeError::AccountFrozen {
                account_id: keeper_account,
            });
        }
        let trading_account = delegation_trading_account_id(delegation_id);
        let client_order_id = delegation_forced_liquidation_client_order_id(&action_id);
        let mut preview = self.clone();
        let event_start = preview.runtime_events.len();
        let outcome = preview.liquidate_position_with_size(
            market_id,
            trading_account,
            keeper_account,
            client_order_id,
            requested_lots,
            timestamp_ms,
        )?;
        if outcome.filled_lots == 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_forced_liquidation_no_fill",
            });
        }
        let (requested_lots, keeper_reward_micro_usdc) = Self::liquidation_execution_proof(
            &preview.runtime_events[event_start..],
            market_id,
            trading_account,
            keeper_account,
        )
        .ok_or(NodeError::InvalidDelegation {
            agent: delegation.agent,
            reason: "delegation_forced_liquidation_missing_event",
        })?;
        if keeper_reward_micro_usdc > 0 {
            preview.validate_real_capital_keeper_reward_candidate(
                keeper_account,
                keeper_reward_micro_usdc,
            )?;
        }
        Ok(DelegationForcedLiquidationPreview {
            requested_lots,
            filled_lots: outcome.filled_lots,
            remaining_lots: outcome.remaining_lots,
            keeper_reward_micro_usdc,
        })
    }

    pub(super) fn force_liquidate_delegation_at(
        &mut self,
        action_id: DelegationActionId,
        delegation_id: DelegationId,
        market_id: u64,
        keeper_account: u64,
        requested_lots: Option<u64>,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<DelegationForcedLiquidation, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        let preview = self.preview_delegation_forced_liquidation(
            action_id,
            &delegation_id,
            market_id,
            keeper_account,
            requested_lots,
            timestamp_ms,
        )?;
        let trading_account = delegation_trading_account_id(&delegation_id);
        let client_order_id = delegation_forced_liquidation_client_order_id(&action_id);
        let event_start = self.runtime_events.len();
        let outcome = self.liquidate_position_with_size(
            market_id,
            trading_account,
            keeper_account,
            client_order_id,
            requested_lots,
            timestamp_ms,
        )?;
        let (requested_lots, keeper_reward_micro_usdc) = Self::liquidation_execution_proof(
            &self.runtime_events[event_start..],
            market_id,
            trading_account,
            keeper_account,
        )
        .ok_or(NodeError::InvalidDelegation {
            agent: delegation.agent,
            reason: "delegation_forced_liquidation_missing_event",
        })?;
        if requested_lots != preview.requested_lots
            || outcome.filled_lots != preview.filled_lots
            || outcome.remaining_lots != preview.remaining_lots
            || keeper_reward_micro_usdc != preview.keeper_reward_micro_usdc
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_forced_liquidation_preview_mismatch",
            });
        }
        if keeper_reward_micro_usdc > 0 {
            self.pay_real_capital_forced_liquidation_cost_escrow_v1_at(
                keeper_account,
                keeper_reward_micro_usdc,
                timestamp_ms,
                event_block_height,
            )?;
        }
        Ok(DelegationForcedLiquidation {
            delegation,
            market_id,
            keeper_account,
            requested_lots,
            filled_lots: outcome.filled_lots,
            remaining_lots: outcome.remaining_lots,
            keeper_reward_micro_usdc,
        })
    }

    pub fn close_delegation(
        &mut self,
        delegation_id: DelegationId,
        timestamp_ms: u64,
    ) -> Result<Delegation, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
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
        if delegation.total_shares != 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_outstanding_shares",
            });
        }
        if self.has_active_delegation_redemption(&delegation_id) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_active_redemptions",
            });
        }
        let trading_account = delegation_trading_account_id(&delegation_id);
        if !self.positions(trading_account, false, 1).is_empty() {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_open_positions",
            });
        }
        if self.delegation_free_usdc_balance(&delegation_id)? != 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_has_free_balance",
            });
        }
        let live_mm_exit_keys =
            self.preflight_live_delegation_market_maker_exits(&delegation_id)?;

        let escrowed_bond = self.delegation_bond_escrow_amount(&delegation)?;
        if escrowed_bond != 0 {
            let agent_address = self.account_policy_address_for_delegation(
                delegation.agent,
                delegation.agent,
                "agent_evm_address_missing",
            )?;
            let bond_escrow_address = real_capital_bond_escrow_address(&delegation_id);
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
            let bond_escrow_balance = self.policy_usdc_balance_at_address(
                bond_escrow_address,
                delegation.agent,
                "real_capital_bond_escrow_balance_malformed",
            )?;
            if bond_escrow_balance != U256::from(escrowed_bond) {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_bond_escrow_mismatch",
                });
            }
            let mut sets = Vec::new();
            let mut deletes = Vec::new();
            Self::apply_policy_usdc_balance_update(
                &mut sets,
                &mut deletes,
                agent_address,
                new_agent_balance,
            );
            Self::apply_policy_usdc_balance_update(
                &mut sets,
                &mut deletes,
                bond_escrow_address,
                U256::ZERO,
            );
            // Testnet bridge path: this mutates the committed EVM-state root
            // synchronously. Production multi-validator flow must route
            // delegation bond returns through block contents instead.
            self.apply_committed_state_updates(sets, deletes)
                .map_err(|_| NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_bond_state_update_failed",
                })?;
            let updated_head_qc =
                build_quorum_certificate(self.head(), &self.consensus_signing_keys)?;
            self.pacemaker.advance_on_qc(updated_head_qc);
        }
        self.real_capital_bond_escrow_micro_usdc
            .remove(&delegation_id);
        // Close returns the delegation bond escrow first, then exits any live
        // delegation-backed MM enrollments through the existing MM refund
        // path. A crash between those phases is caught on restart by the
        // terminal-delegation snapshot invariant for live MM enrollments.
        self.exit_preflighted_live_delegation_market_maker_enrollments(
            live_mm_exit_keys,
            timestamp_ms,
            None,
        );

        let mut updated_delegation = delegation;
        let old_status = updated_delegation.status;
        updated_delegation.status = DelegationStatus::Closed;
        updated_delegation.last_updated_at_ms = timestamp_ms;
        updated_delegation.winding_started_at_ms = None;
        self.delegations
            .insert(delegation_id, updated_delegation.clone());
        self.apply_delegation_policy_lifecycle_state(
            &updated_delegation,
            old_status,
            timestamp_ms,
            None,
        )?;
        self.record_delegation_status_changed(
            &updated_delegation,
            old_status,
            DelegationStatusReason::WindingComplete,
            updated_delegation.agent_bond,
            0,
            0,
            0,
            timestamp_ms,
        );
        Ok(updated_delegation)
    }

    pub fn terminate_delegation(
        &mut self,
        delegation_id: DelegationId,
        reason: BlockDelegationFaultReason,
        slash_policy: Option<BlockDelegationSlashPolicy>,
        timestamp_ms: u64,
    ) -> Result<DelegationTermination, NodeError> {
        self.terminate_delegation_at(
            delegation_id,
            reason,
            slash_policy,
            timestamp_ms,
            self.head().header.height,
            None,
        )
    }

    pub(super) fn terminate_delegation_at(
        &mut self,
        delegation_id: DelegationId,
        reason: BlockDelegationFaultReason,
        slash_policy: Option<BlockDelegationSlashPolicy>,
        timestamp_ms: u64,
        block_height: u64,
        event_block_height: Option<u64>,
    ) -> Result<DelegationTermination, NodeError> {
        let mut delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
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
                reason: "delegation_not_terminable",
            });
        }
        let slash_policy =
            delegation_termination_slash_policy(reason, slash_policy, delegation.agent)?;
        let slash_distribution = self.delegation_slash_distribution(&delegation, slash_policy);
        self.apply_delegation_slash_distribution(&delegation, &slash_distribution)?;

        let vault_id = vault_id_for_delegation(&delegation_id);
        let deposit_disable_event =
            if let Some(listing) = self.real_capital_listings.get_mut(&vault_id) {
                if listing.deposit_enabled {
                    listing.deposit_enabled = false;
                    listing.last_updated_block = block_height;
                    listing.last_updated_governance_action = [0_u8; 32];
                    Some((listing.delegation_id, listing.serving_agent))
                } else {
                    None
                }
            } else {
                None
            };
        if let Some((listing_delegation_id, serving_agent)) = deposit_disable_event {
            self.record_runtime_event(
                event_block_height,
                timestamp_ms,
                RuntimeEventKind::RealCapitalListingDepositEnabledUpdated {
                    vault_id,
                    delegation_id: listing_delegation_id,
                    serving_agent,
                    deposit_enabled: false,
                    governance_action: [0_u8; 32],
                },
            );
        }

        let live_mm_exit_keys =
            self.preflight_live_delegation_market_maker_exits(&delegation_id)?;
        let old_status = delegation.status;
        delegation.status = DelegationStatus::Terminated;
        delegation.last_updated_at_ms = timestamp_ms;
        delegation.winding_started_at_ms = None;
        self.delegations.insert(delegation_id, delegation.clone());
        self.apply_delegation_policy_lifecycle_state(
            &delegation,
            old_status,
            timestamp_ms,
            event_block_height,
        )?;
        self.exit_preflighted_live_delegation_market_maker_enrollments(
            live_mm_exit_keys,
            timestamp_ms,
            event_block_height,
        );
        self.record_delegation_status_changed_at(
            event_block_height,
            &delegation,
            old_status,
            Self::delegation_fault_status_reason(reason),
            slash_distribution.bond_returned,
            slash_distribution.bond_slashed,
            slash_distribution.principal_compensation,
            slash_distribution.insurance_fund_delta,
            timestamp_ms,
        );
        Ok(DelegationTermination {
            delegation,
            bond_returned: slash_distribution.bond_returned,
            bond_slashed: slash_distribution.bond_slashed,
            principal_compensation: slash_distribution.principal_compensation,
            insurance_fund_delta: slash_distribution.insurance_fund_delta,
        })
    }

    pub(super) fn delegation_slash_distribution(
        &self,
        delegation: &Delegation,
        policy: BlockDelegationSlashPolicy,
    ) -> DelegationSlashDistribution {
        let bond_slashed = delegation.agent_bond;
        if bond_slashed == 0 {
            return DelegationSlashDistribution::default();
        }
        let principal_pool = match policy {
            BlockDelegationSlashPolicy::PrincipalProRata => bond_slashed,
            BlockDelegationSlashPolicy::WindingGrace => bond_slashed / 2,
        };
        // Integer division dust, plus any pro-rata principal-pool amount that
        // cannot be allocated to live shares, remains in insurance_fund_delta.
        let mut principal_allocations = Vec::new();
        let mut principal_compensation = 0_u128;
        if principal_pool > 0 && delegation.total_shares > 0 {
            for ((share_delegation_id, principal), share) in &self.delegation_shares {
                if share_delegation_id != &delegation.delegation_id {
                    continue;
                }
                let allocation = (U256::from(principal_pool) * U256::from(share.shares)
                    / U256::from(delegation.total_shares))
                .to::<u128>();
                if allocation == 0 {
                    continue;
                }
                principal_compensation = principal_compensation.saturating_add(allocation);
                principal_allocations.push((*principal, allocation));
            }
        }
        DelegationSlashDistribution {
            bond_returned: 0,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta: bond_slashed.saturating_sub(principal_compensation),
            principal_allocations,
        }
    }

    pub(super) fn apply_delegation_slash_distribution(
        &mut self,
        delegation: &Delegation,
        distribution: &DelegationSlashDistribution,
    ) -> Result<(), NodeError> {
        let escrowed_bond = self.delegation_bond_escrow_amount(delegation)?;
        let bond_escrow_address = real_capital_bond_escrow_address(&delegation.delegation_id);
        if distribution.bond_slashed != escrowed_bond {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_slash_distribution_mismatch",
            });
        }
        let next_compensation_escrow = self
            .real_capital_compensation_escrow_micro_usdc
            .checked_add(distribution.principal_compensation)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_compensation_escrow_overflow",
            })?;
        let next_insurance_wallet = self
            .real_capital_insurance_wallet_micro_usdc
            .checked_add(distribution.insurance_fund_delta)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_insurance_wallet_overflow",
            })?;
        let next_insurance_accumulator = self
            .real_capital_insurance_fund_micro_usdc
            .checked_add(distribution.insurance_fund_delta)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_insurance_overflow",
            })?;
        let compensation_escrow_balance = self.real_capital_compensation_escrow_policy_balance()?;
        if compensation_escrow_balance
            != U256::from(self.real_capital_compensation_escrow_micro_usdc)
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_compensation_escrow_mismatch",
            });
        }
        let insurance_wallet_balance = self.real_capital_insurance_wallet_policy_balance()?;
        if insurance_wallet_balance != U256::from(self.real_capital_insurance_wallet_micro_usdc) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_insurance_wallet_escrow_mismatch",
            });
        }
        let next_compensation_escrow_balance = compensation_escrow_balance
            .checked_add(U256::from(distribution.principal_compensation))
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_compensation_escrow_overflow",
            })?;
        let next_insurance_wallet_balance = insurance_wallet_balance
            .checked_add(U256::from(distribution.insurance_fund_delta))
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_insurance_wallet_overflow",
            })?;
        debug_assert_eq!(
            distribution.bond_slashed,
            distribution
                .principal_compensation
                .saturating_add(distribution.insurance_fund_delta),
            "slash distribution must conserve the slashed bond"
        );
        for (principal, amount) in &distribution.principal_allocations {
            let prior = self
                .delegation_compensation_claims
                .get(&(delegation.delegation_id, *principal))
                .copied()
                .unwrap_or(0);
            prior
                .checked_add(*amount)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_compensation_overflow",
                })?;
        }
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        if distribution.bond_slashed > 0 {
            Self::apply_policy_usdc_balance_update(
                &mut sets,
                &mut deletes,
                bond_escrow_address,
                U256::ZERO,
            );
        }
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_compensation_escrow_address(),
            next_compensation_escrow_balance,
        );
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_insurance_wallet_address(),
            next_insurance_wallet_balance,
        );
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_bond_state_update_failed",
            })?;
        // Rust audit ledgers update after the committed EVM escrow movement.
        // If these ever diverge, snapshot validation will reject reload.
        if distribution.bond_slashed > 0 {
            self.real_capital_bond_escrow_micro_usdc
                .remove(&delegation.delegation_id);
        }
        self.real_capital_compensation_escrow_micro_usdc = next_compensation_escrow;
        self.real_capital_insurance_wallet_micro_usdc = next_insurance_wallet;
        self.delegation_termination_payout_weights
            .retain(|(delegation_id, _), _| *delegation_id != delegation.delegation_id);
        for ((share_delegation_id, principal), share) in &self.delegation_shares {
            if *share_delegation_id != delegation.delegation_id || share.shares == 0 {
                continue;
            }
            self.delegation_termination_payout_weights
                .insert((delegation.delegation_id, *principal), share.shares);
        }
        for (principal, amount) in &distribution.principal_allocations {
            let key = (delegation.delegation_id, *principal);
            let entry = self.delegation_compensation_claims.entry(key).or_default();
            *entry = entry
                .checked_add(*amount)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_compensation_overflow",
                })?;
        }
        self.real_capital_insurance_fund_micro_usdc = next_insurance_accumulator;
        Ok(())
    }

    pub fn current_delegation_equity_micro_usdc(
        &self,
        delegation: &Delegation,
    ) -> Result<i128, NodeError> {
        let free_balance = self.delegation_free_usdc_balance(&delegation.delegation_id)?;
        if free_balance > i128::MAX as u128 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_equity_overflow",
            });
        }
        let trading_account = delegation_trading_account_id(&delegation.delegation_id);
        let mut equity = free_balance as i128;
        for (market_id, _) in self.risk_positions(trading_account, false, usize::MAX) {
            equity = equity
                .checked_add(self.margin_snapshot(market_id, trading_account).equity)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_equity_overflow",
                })?;
        }
        Ok(equity)
    }

    pub(super) fn current_delegation_nav_per_share(
        &self,
        delegation: &Delegation,
    ) -> Result<u128, NodeError> {
        if delegation.total_shares == 0 {
            return Ok(SHARE_SCALE_FACTOR);
        }
        let equity = self.current_delegation_equity_micro_usdc(delegation)?;
        let equity = u128::try_from(equity).map_err(|_| NodeError::InvalidDelegation {
            agent: delegation.agent,
            reason: "delegation_nav_invalid",
        })?;
        equity
            .checked_mul(SHARE_SCALE_FACTOR)
            .and_then(|scaled| scaled.checked_div(delegation.total_shares))
            .filter(|nav| *nav > 0)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_nav_invalid",
            })
    }

    pub fn open_delegation(
        &mut self,
        agent: u64,
        mandate_id: [u8; 32],
        performance_share_units: u16,
        crystallization_cadence_ms: u64,
        notice_period_ms: u64,
        agent_bond: u128,
        timestamp_ms: u64,
    ) -> Result<Delegation, NodeError> {
        validate_open_delegation_config(
            agent,
            &mandate_id,
            performance_share_units,
            crystallization_cadence_ms,
            notice_period_ms,
            agent_bond,
        )?;
        self.ensure_delegation_mandate_binding(agent, mandate_id, timestamp_ms)?;
        if self.live_delegation_id_for_mandate(&mandate_id).is_some() {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "mandate_live_delegation_exists",
            });
        }
        let mandate_policy_ids =
            self.bindable_mandate_policy_ids_for_delegation(mandate_id, agent)?;
        if self.active_delegation_count() >= MAX_ACTIVE_DELEGATIONS {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "max_active_delegations",
            });
        }
        if self.live_delegations_for_agent(agent) >= MAX_DELEGATIONS_PER_AGENT_LIVE {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "max_delegations_per_agent",
            });
        }
        let delegation_id = derive_delegation_id(agent, &mandate_id, timestamp_ms);
        if self.delegations.contains_key(&delegation_id) {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_id_collision",
            });
        }
        let agent_address =
            self.account_policy_address_for_delegation(agent, agent, "agent_evm_address_missing")?;
        let agent_balance = self.policy_usdc_balance_for_delegation(agent_address, agent)?;
        let bond_u256 = U256::from(agent_bond);
        if agent_balance < bond_u256 {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "insufficient_agent_policy_usdc_bond",
            });
        }
        let new_agent_balance =
            agent_balance
                .checked_sub(bond_u256)
                .ok_or(NodeError::InvalidDelegation {
                    agent,
                    reason: "insufficient_agent_policy_usdc_bond",
                })?;
        let bond_escrow_address = real_capital_bond_escrow_address(&delegation_id);
        let bond_escrow_balance = self.policy_usdc_balance_at_address(
            bond_escrow_address,
            agent,
            "real_capital_bond_escrow_balance_malformed",
        )?;
        if !bond_escrow_balance.is_zero() {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "delegation_bond_escrow_address_dirty",
            });
        }
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            agent_address,
            new_agent_balance,
        );
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            bond_escrow_address,
            bond_u256,
        );
        // TODO(RFC-026): add slashing instead of full bond return for
        // terminal fault paths once delegation termination has a producer.
        // Testnet bridge path: this mutates the committed EVM-state root
        // synchronously. Production multi-validator flow must route
        // delegation balance movement through block contents instead.
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent,
                reason: "delegation_bond_state_update_failed",
            })?;
        let updated_head_qc = build_quorum_certificate(self.head(), &self.consensus_signing_keys)?;
        self.pacemaker.advance_on_qc(updated_head_qc);

        let delegation = Delegation {
            delegation_id,
            agent,
            mandate_id,
            opened_at_ms: timestamp_ms,
            last_updated_at_ms: timestamp_ms,
            winding_started_at_ms: None,
            status: DelegationStatus::Open,
            performance_share_units,
            crystallization_cadence_ms,
            notice_period_ms,
            total_shares: 0,
            high_water_mark_per_share: 0,
            last_crystallized_at_ms: timestamp_ms,
            agent_bond,
        };
        self.bind_mandate_policy_scopes_to_delegation(&mandate_policy_ids, delegation_id);
        // Rust audit ledgers update after the committed EVM escrow movement.
        // If these ever diverge, snapshot validation will reject reload.
        if agent_bond > 0 {
            self.real_capital_bond_escrow_micro_usdc
                .insert(delegation_id, agent_bond);
        }
        self.delegations.insert(delegation_id, delegation.clone());
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::DelegationOpened {
                delegation_id,
                agent,
                mandate_id,
                performance_share_units,
                crystallization_cadence_ms,
                notice_period_ms,
                agent_bond,
            },
        );
        Ok(delegation)
    }

    pub fn delegation_trading_account(&self, delegation_id: &DelegationId) -> Option<u64> {
        self.delegations
            .contains_key(delegation_id)
            .then(|| delegation_trading_account_id(delegation_id))
    }

    pub fn deposit_to_delegation(
        &mut self,
        principal: u64,
        delegation_id: DelegationId,
        amount: u128,
        timestamp_ms: u64,
    ) -> Result<DelegationDeposit, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
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
        if amount < DELEGATION_MIN_DEPOSIT {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_deposit_below_min",
            });
        }
        let cap_accounting =
            self.ensure_delegation_deposit_real_capital_admission(principal, &delegation, amount)?;
        let principal_address = self.account_policy_address_for_delegation(
            principal,
            delegation.agent,
            "principal_evm_address_missing",
        )?;
        let principal_balance =
            self.policy_usdc_balance_for_delegation(principal_address, delegation.agent)?;
        if principal_balance > U256::from(u128::MAX) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "principal_balance_too_large",
            });
        }
        let amount_u256 = U256::from(amount);
        if principal_balance < amount_u256 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "insufficient_principal_policy_usdc",
            });
        }

        let nav_per_share = self.current_delegation_nav_per_share(&delegation)?;
        let shares_minted = amount
            .checked_mul(SHARE_SCALE_FACTOR)
            .and_then(|scaled| scaled.checked_div(nav_per_share))
            .filter(|shares| *shares > 0)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_mint_zero",
            })?;
        let prior_share = self
            .delegation_shares
            .get(&(delegation_id, principal))
            .copied()
            .unwrap_or_default();
        let new_share_balance =
            prior_share
                .shares
                .checked_add(shares_minted)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_share_overflow",
                })?;
        let new_total_shares = delegation.total_shares.checked_add(shares_minted).ok_or(
            NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_share_overflow",
            },
        )?;
        let avg_entry_nav_per_share = if prior_share.shares == 0 {
            nav_per_share
        } else {
            prior_share
                .avg_entry_nav_per_share
                .checked_mul(prior_share.shares)
                .and_then(|old_cost| {
                    nav_per_share
                        .checked_mul(shares_minted)
                        .and_then(|new_cost| old_cost.checked_add(new_cost))
                })
                .and_then(|total_cost| total_cost.checked_div(new_share_balance))
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_share_overflow",
                })?
        };

        let trading_account = delegation_trading_account_id(&delegation_id);
        let trading_address = delegation_trading_address(&delegation_id);
        let trading_balance =
            self.policy_usdc_balance_for_delegation(trading_address, delegation.agent)?;
        let new_trading_balance =
            trading_balance
                .checked_add(amount_u256)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_balance_overflow",
                })?;
        if new_trading_balance > U256::from(u128::MAX) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_balance_too_large",
            });
        }
        let new_principal_balance =
            principal_balance
                .checked_sub(amount_u256)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "insufficient_principal_policy_usdc",
                })?;

        let mut sets = vec![
            (
                account_evm_address_state_key(trading_account),
                trading_address.to_vec(),
            ),
            (
                evm_address_account_state_key(trading_address),
                trading_account.to_be_bytes().to_vec(),
            ),
            (
                policy_usdc_balance_state_key(trading_address),
                new_trading_balance.to_be_bytes::<32>().to_vec(),
            ),
        ];
        let mut deletes = Vec::new();
        if new_principal_balance.is_zero() {
            deletes.push(policy_usdc_balance_state_key(principal_address));
        } else {
            sets.push((
                policy_usdc_balance_state_key(principal_address),
                new_principal_balance.to_be_bytes::<32>().to_vec(),
            ));
        }
        // Testnet bridge path: this mutates the committed EVM-state root
        // synchronously. Production multi-validator flow must route
        // delegation balance movement through block contents instead.
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_usdc_state_update_failed",
            })?;
        let updated_head_qc = build_quorum_certificate(self.head(), &self.consensus_signing_keys)?;
        self.pacemaker.advance_on_qc(updated_head_qc);

        let mut updated_delegation = delegation;
        updated_delegation.total_shares = new_total_shares;
        if updated_delegation.high_water_mark_per_share == 0 {
            updated_delegation.high_water_mark_per_share = SHARE_SCALE_FACTOR;
        }
        updated_delegation.last_updated_at_ms = timestamp_ms;
        let updated_share = DelegationShare {
            shares: new_share_balance,
            locked_for_redemption: prior_share.locked_for_redemption,
            avg_entry_nav_per_share,
        };
        self.apply_delegation_deposit_cap_accounting(cap_accounting);
        self.delegations
            .insert(delegation_id, updated_delegation.clone());
        self.delegation_shares
            .insert((delegation_id, principal), updated_share);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::DelegationDeposited {
                delegation_id,
                principal,
                amount,
                shares_minted,
                total_shares: new_total_shares,
            },
        );

        Ok(DelegationDeposit {
            delegation: updated_delegation,
            principal,
            share: updated_share,
            shares_minted,
            principal_balance: new_principal_balance.to::<u128>(),
            delegation_balance: new_trading_balance.to::<u128>(),
            trading_account,
        })
    }

    pub fn request_delegation_redemption(
        &mut self,
        principal: u64,
        delegation_id: DelegationId,
        shares: u128,
        timestamp_ms: u64,
    ) -> Result<DelegationRedemptionRequest, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
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
            .get(&(delegation_id, principal))
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
        let ready_at_ms = timestamp_ms
            .checked_add(delegation.notice_period_ms)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_ready_at_overflow",
            })?;
        let request_id = derive_redemption_request_id(principal, &delegation_id, timestamp_ms);
        if self.redemption_requests.contains_key(&request_id) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_request_id_collision",
            });
        }
        let nav_at_request = self.current_delegation_nav_per_share(&delegation)?;
        let updated_share = DelegationShare {
            shares: prior_share.shares,
            locked_for_redemption: prior_share
                .locked_for_redemption
                .checked_add(shares)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_share_lock_overflow",
                })?,
            avg_entry_nav_per_share: prior_share.avg_entry_nav_per_share,
        };
        let request = RedemptionRequest {
            request_id,
            delegation_id,
            principal,
            shares_requested: shares,
            requested_at_ms: timestamp_ms,
            ready_at_ms,
            nav_at_request,
            status: RedemptionRequestStatus::Pending,
        };
        self.delegation_shares
            .insert((delegation_id, principal), updated_share);
        self.redemption_requests.insert(request_id, request);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::DelegationRedemptionRequested {
                delegation_id,
                principal,
                request_id,
                shares_requested: shares,
                ready_at_ms,
            },
        );

        Ok(DelegationRedemptionRequest {
            request,
            share: updated_share,
        })
    }

    pub fn finalize_delegation_redemption(
        &mut self,
        request_id: RedemptionRequestId,
        timestamp_ms: u64,
    ) -> Result<DelegationRedemptionFinalization, NodeError> {
        let request =
            *self
                .redemption_requests
                .get(&request_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_redemption_request",
                })?;
        let delegation = self
            .delegations
            .get(&request.delegation_id)
            .cloned()
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
        let nav_per_share = self.current_delegation_nav_per_share(&delegation)?;
        let usdc_out = request
            .shares_requested
            .checked_mul(nav_per_share)
            .and_then(|scaled| scaled.checked_div(SHARE_SCALE_FACTOR))
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "redemption_value_overflow",
            })?;
        let trading_address = delegation_trading_address(&request.delegation_id);
        let trading_balance =
            self.policy_usdc_balance_for_delegation(trading_address, delegation.agent)?;
        if trading_balance < U256::from(usdc_out) {
            let mut ready_request = request;
            ready_request.status = RedemptionRequestStatus::Ready;
            self.redemption_requests.insert(request_id, ready_request);
            return Ok(DelegationRedemptionFinalization {
                request: ready_request,
                delegation,
                share: prior_share,
                usdc_out: 0,
                compensation_out: 0,
                principal_balance: 0,
                delegation_balance: trading_balance.min(U256::from(u128::MAX)).to::<u128>(),
            });
        }
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
        let cap_accounting = self.delegation_redemption_cap_accounting(
            &delegation,
            request.principal,
            request.shares_requested,
            prior_share.shares,
        )?;
        let new_trading_balance = trading_balance.checked_sub(U256::from(usdc_out)).ok_or(
            NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_balance_underflow",
            },
        )?;
        let compensation_escrow_balance = self.real_capital_compensation_escrow_policy_balance()?;
        if compensation_escrow_balance
            != U256::from(self.real_capital_compensation_escrow_micro_usdc)
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_compensation_escrow_mismatch",
            });
        }
        let new_compensation_escrow_balance = compensation_escrow_balance
            .checked_sub(U256::from(compensation_out))
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_compensation_escrow_underflow",
            })?;
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            principal_address,
            new_principal_balance,
        );
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            trading_address,
            new_trading_balance,
        );
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_compensation_escrow_address(),
            new_compensation_escrow_balance,
        );
        // Testnet bridge path: this mutates the committed EVM-state root
        // synchronously. Production multi-validator flow must route
        // delegation balance movement through block contents instead.
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_usdc_state_update_failed",
            })?;
        let updated_head_qc = build_quorum_certificate(self.head(), &self.consensus_signing_keys)?;
        self.pacemaker.advance_on_qc(updated_head_qc);

        let updated_share = DelegationShare {
            shares: prior_share.shares - request.shares_requested,
            locked_for_redemption: prior_share.locked_for_redemption - request.shares_requested,
            avg_entry_nav_per_share: prior_share.avg_entry_nav_per_share,
        };
        if updated_share.shares == 0 {
            self.delegation_shares
                .remove(&(request.delegation_id, request.principal));
        } else {
            self.delegation_shares
                .insert((request.delegation_id, request.principal), updated_share);
        }
        let mut updated_delegation = delegation;
        updated_delegation.total_shares = updated_delegation
            .total_shares
            .checked_sub(request.shares_requested)
            .ok_or(NodeError::InvalidDelegation {
                agent: updated_delegation.agent,
                reason: "delegation_share_underflow",
            })?;
        updated_delegation.last_updated_at_ms = timestamp_ms;
        self.apply_delegation_redemption_cap_accounting(cap_accounting);
        self.apply_delegation_compensation_release(
            request.delegation_id,
            request.principal,
            compensation_out,
        );
        self.delegations
            .insert(request.delegation_id, updated_delegation.clone());
        let mut finalized_request = request;
        finalized_request.status = RedemptionRequestStatus::Finalized;
        self.redemption_requests
            .insert(request_id, finalized_request);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::DelegationRedemptionFinalized {
                delegation_id: request.delegation_id,
                principal: request.principal,
                request_id,
                shares_burned: request.shares_requested,
                usdc_out,
                compensation_out,
                total_shares: updated_delegation.total_shares,
            },
        );

        Ok(DelegationRedemptionFinalization {
            request: finalized_request,
            delegation: updated_delegation,
            share: updated_share,
            usdc_out,
            compensation_out,
            principal_balance: new_principal_balance.to::<u128>(),
            delegation_balance: new_trading_balance.to::<u128>(),
        })
    }

    pub fn cancel_delegation_redemption(
        &mut self,
        principal: u64,
        request_id: RedemptionRequestId,
        timestamp_ms: u64,
    ) -> Result<DelegationRedemptionCancellation, NodeError> {
        let request =
            *self
                .redemption_requests
                .get(&request_id)
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
        let updated_share = DelegationShare {
            shares: prior_share.shares,
            locked_for_redemption: prior_share.locked_for_redemption - request.shares_requested,
            avg_entry_nav_per_share: prior_share.avg_entry_nav_per_share,
        };
        self.delegation_shares
            .insert((request.delegation_id, principal), updated_share);
        let mut cancelled_request = request;
        cancelled_request.status = RedemptionRequestStatus::Cancelled;
        self.redemption_requests
            .insert(request_id, cancelled_request);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::DelegationRedemptionCancelled {
                delegation_id: request.delegation_id,
                principal,
                request_id,
                shares_unlocked: request.shares_requested,
            },
        );

        Ok(DelegationRedemptionCancellation {
            request: cancelled_request,
            share: updated_share,
        })
    }

    pub fn crystallize_due_delegations(
        &mut self,
        timestamp_ms: u64,
    ) -> Result<Vec<DelegationCrystallization>, NodeError> {
        self.crystallize_due_delegations_with_block_height(timestamp_ms, None)
    }

    pub(super) fn crystallize_due_delegations_with_block_height(
        &mut self,
        timestamp_ms: u64,
        block_height: Option<u64>,
    ) -> Result<Vec<DelegationCrystallization>, NodeError> {
        let due: Vec<_> = self
            .delegations
            .values()
            .filter(|delegation| {
                matches!(
                    delegation.status,
                    DelegationStatus::Open | DelegationStatus::ClosedToDeposits
                ) && delegation.total_shares > 0
                    && timestamp_ms
                        >= delegation
                            .last_crystallized_at_ms
                            .saturating_add(delegation.crystallization_cadence_ms)
            })
            .map(|delegation| delegation.delegation_id)
            .collect();
        let mut crystallizations = Vec::with_capacity(due.len());
        for delegation_id in due {
            crystallizations.push(self.crystallize_delegation_with_block_height(
                delegation_id,
                timestamp_ms,
                block_height,
            )?);
        }
        Ok(crystallizations)
    }

    pub(super) fn crystallize_delegation_with_block_height(
        &mut self,
        delegation_id: DelegationId,
        timestamp_ms: u64,
        block_height: Option<u64>,
    ) -> Result<DelegationCrystallization, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if delegation.total_shares == 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_no_shares",
            });
        }
        let nav_pre_fee = self.current_delegation_nav_per_share(&delegation)?;
        let trading_address = delegation_trading_address(&delegation_id);
        let trading_balance =
            self.policy_usdc_balance_for_delegation(trading_address, delegation.agent)?;
        let hwm = delegation.high_water_mark_per_share;
        let agent_fee = if hwm > 0 && nav_pre_fee > hwm {
            let excess_per_share =
                nav_pre_fee
                    .checked_sub(hwm)
                    .ok_or(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "delegation_crystallization_overflow",
                    })?;
            let excess_total = excess_per_share
                .checked_mul(delegation.total_shares)
                .and_then(|value| value.checked_div(SHARE_SCALE_FACTOR))
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_crystallization_overflow",
                })?;
            excess_total
                .checked_mul(u128::from(delegation.performance_share_units))
                .and_then(|value| value.checked_div(PERFORMANCE_SHARE_DENOMINATOR))
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_crystallization_overflow",
                })?
        } else {
            0
        };

        let (agent_balance_u128, delegation_balance_u128, nav_post_fee) = if agent_fee == 0 {
            (
                self.policy_usdc_balance_for_delegation(
                    self.account_policy_address_for_delegation(
                        delegation.agent,
                        delegation.agent,
                        "agent_evm_address_missing",
                    )?,
                    delegation.agent,
                )?
                .min(U256::from(u128::MAX))
                .to::<u128>(),
                trading_balance.min(U256::from(u128::MAX)).to::<u128>(),
                nav_pre_fee,
            )
        } else {
            let fee_u256 = U256::from(agent_fee);
            if trading_balance < fee_u256 {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_fee_insufficient_balance",
                });
            }
            let agent_address = self.account_policy_address_for_delegation(
                delegation.agent,
                delegation.agent,
                "agent_evm_address_missing",
            )?;
            let agent_balance =
                self.policy_usdc_balance_for_delegation(agent_address, delegation.agent)?;
            let new_agent_balance =
                agent_balance
                    .checked_add(fee_u256)
                    .ok_or(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "agent_balance_overflow",
                    })?;
            if new_agent_balance > U256::from(u128::MAX) {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "agent_balance_too_large",
                });
            }
            let new_trading_balance =
                trading_balance
                    .checked_sub(fee_u256)
                    .ok_or(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "delegation_balance_underflow",
                    })?;
            let mut sets = vec![(
                policy_usdc_balance_state_key(agent_address),
                new_agent_balance.to_be_bytes::<32>().to_vec(),
            )];
            let mut deletes = Vec::new();
            if new_trading_balance.is_zero() {
                deletes.push(policy_usdc_balance_state_key(trading_address));
            } else {
                sets.push((
                    policy_usdc_balance_state_key(trading_address),
                    new_trading_balance.to_be_bytes::<32>().to_vec(),
                ));
            }
            // Testnet bridge path: this mutates the committed EVM-state root
            // synchronously. Production multi-validator flow must route
            // delegation fee movement through block contents instead.
            self.apply_committed_state_updates(sets, deletes)
                .map_err(|_| NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_usdc_state_update_failed",
                })?;
            let updated_head_qc =
                build_quorum_certificate(self.head(), &self.consensus_signing_keys)?;
            self.pacemaker.advance_on_qc(updated_head_qc);
            let nav_post_fee = new_trading_balance
                .to::<u128>()
                .checked_mul(SHARE_SCALE_FACTOR)
                .and_then(|value| value.checked_div(delegation.total_shares))
                .filter(|nav| *nav > 0)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_nav_invalid",
                })?;
            (
                new_agent_balance.to::<u128>(),
                new_trading_balance.to::<u128>(),
                nav_post_fee,
            )
        };

        let mut updated_delegation = delegation;
        updated_delegation.high_water_mark_per_share =
            if nav_pre_fee > hwm { nav_post_fee } else { hwm };
        updated_delegation.last_crystallized_at_ms = timestamp_ms;
        updated_delegation.last_updated_at_ms = timestamp_ms;
        self.delegations
            .insert(delegation_id, updated_delegation.clone());
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::DelegationCrystallized {
                delegation_id,
                agent: updated_delegation.agent,
                agent_fee,
                nav_pre_fee,
                nav_post_fee,
                hwm_new: updated_delegation.high_water_mark_per_share,
            },
        );

        Ok(DelegationCrystallization {
            delegation: updated_delegation,
            agent_fee,
            nav_pre_fee,
            nav_post_fee,
            agent_balance: agent_balance_u128,
            delegation_balance: delegation_balance_u128,
        })
    }
}
