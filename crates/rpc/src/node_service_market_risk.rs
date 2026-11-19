use crate::*;

impl<'a> NodeRpcService<'a> {
    pub fn get_market_state(&self, market_id: u64) -> MarketStateResponse {
        let risk = self.engine.market_risk_parameters(market_id);
        let fees = self.engine.market_fee_parameters(market_id);
        let mark = self.engine.mark_price_status(market_id);
        let index = self.engine.index_price_status(market_id);
        let oracle = self.engine.market_oracle_config(market_id);
        let market_positions = self
            .engine
            .all_positions(Some(market_id), false, usize::MAX);
        let market_account_ids = market_positions
            .iter()
            .map(|(_, account_id, _)| *account_id)
            .collect::<BTreeSet<_>>();
        let liquidation_candidates: Vec<_> = market_positions
            .into_iter()
            .filter_map(|(entry_market_id, account_id, ledger)| {
                if !self.engine.liquidation_price_inputs_ready(entry_market_id) {
                    return None;
                }
                let snapshot = risk_snapshot_from_ledger(
                    self.engine,
                    entry_market_id,
                    account_id,
                    ledger,
                    self.engine.margin_snapshot(entry_market_id, account_id),
                    self.engine.mark_price_ticks(entry_market_id),
                    RiskSnapshotSupplement {
                        liquidation_stall_count: self
                            .engine
                            .liquidation_stall_count(entry_market_id, account_id),
                        liquidation_freeze_threshold: self.engine.liquidation_freeze_threshold(),
                        liquidation_freeze_imminent: self
                            .engine
                            .liquidation_freeze_imminent(entry_market_id, account_id),
                        suggested_liquidation_lots: suggested_liquidation_lots(
                            self.engine,
                            entry_market_id,
                            account_id,
                        ),
                        latest_account_risk_transition: latest_account_risk_transition(
                            self.engine,
                            account_id,
                            Some(entry_market_id),
                        ),
                    },
                );
                snapshot.under_maintenance.then_some(snapshot)
            })
            .collect();
        let total_liquidation_gap = liquidation_candidates.iter().fold(0_u128, |acc, snapshot| {
            acc.saturating_add(snapshot.liquidation_gap)
        });
        let imminent_freeze_count = liquidation_candidates
            .iter()
            .filter(|snapshot| snapshot.liquidation_freeze_imminent)
            .count();
        let max_liquidation_stall_count = liquidation_candidates
            .iter()
            .map(|snapshot| snapshot.liquidation_stall_count)
            .max()
            .unwrap_or(0);
        let worst_liquidation_gap = liquidation_candidates
            .iter()
            .map(|snapshot| snapshot.liquidation_gap)
            .max()
            .unwrap_or(0);
        let suggested_clear_count = liquidation_candidates
            .iter()
            .filter(|snapshot| {
                snapshot.suggested_liquidation_outcome
                    == Some(SuggestedLiquidationOutcome::ClearsMaintenance)
            })
            .count();
        let suggested_freeze_count = liquidation_candidates
            .iter()
            .filter(|snapshot| {
                matches!(
                    snapshot.suggested_liquidation_outcome,
                    Some(SuggestedLiquidationOutcome::FreezesAccount)
                        | Some(SuggestedLiquidationOutcome::CreatesBadDebt)
                )
            })
            .count();
        let suggested_bad_debt_count = liquidation_candidates
            .iter()
            .filter(|snapshot| {
                snapshot.suggested_liquidation_outcome
                    == Some(SuggestedLiquidationOutcome::CreatesBadDebt)
            })
            .count();
        let total_suggested_bad_debt =
            liquidation_candidates.iter().fold(0_u128, |acc, snapshot| {
                acc.saturating_add(snapshot.suggested_post_market_bad_debt_balance.unwrap_or(0))
            });
        let latest_market_risk_transition =
            latest_market_risk_transition(self.engine, market_id, &market_account_ids);
        let recent_market_risk_transitions =
            recent_market_risk_transitions(self.engine, market_id, &market_account_ids, 5);
        let top_liquidation_accounts =
            self.list_account_risk_summaries(Some(market_id), false, None, false, 3);
        MarketStateResponse {
            market_id,
            trading_paused: self.engine.market_trading_paused(market_id),
            liquidation_candidate_count: liquidation_candidates.len(),
            imminent_freeze_count,
            max_liquidation_stall_count,
            total_liquidation_gap,
            worst_liquidation_gap,
            suggested_clear_count,
            suggested_freeze_count,
            suggested_bad_debt_count,
            total_suggested_bad_debt,
            oracle_max_staleness_ms: oracle.max_staleness_ms,
            order_price_inputs_ready: self.engine.order_price_inputs_ready(market_id),
            liquidation_price_inputs_ready: self.engine.liquidation_price_inputs_ready(market_id),
            mark_price_ticks: mark.map(|status| status.price_ticks),
            mark_updated_at_ms: mark.map(|status| status.timestamp_ms),
            mark_price_stale: mark.map(|status| status.stale),
            index_price_ticks: index.map(|status| status.price_ticks),
            index_updated_at_ms: index.map(|status| status.timestamp_ms),
            index_price_stale: index.map(|status| status.stale),
            premium_rate_ppm: self.engine.premium_rate_ppm(market_id),
            indicative_funding_rate_ppm: self.engine.indicative_funding_rate_ppm(market_id),
            funding_index: self.engine.market_funding_index(market_id),
            maker_fee_bps: fees.maker_fee_bps,
            maker_rebate_bps: fees.maker_rebate_bps,
            taker_fee_bps: fees.taker_fee_bps,
            fee_vault_balance: self.engine.fee_vault_balance(market_id),
            validator_reward_pool_balance: self.engine.validator_reward_pool_balance(market_id),
            market_maker_rebate_pool_balance: self
                .engine
                .market_maker_rebate_pool_balance(market_id),
            treasury_balance: self.engine.treasury_balance(market_id),
            maintenance_margin_bps: risk.maintenance_margin_bps,
            liquidation_penalty_bps: risk.liquidation_penalty_bps,
            liquidator_fee_bps: risk.liquidator_fee_bps,
            insurance_fund_balance: self.engine.insurance_fund_balance(market_id),
            bad_debt_balance: self.engine.market_bad_debt_balance(market_id),
            open_interest_lots: self.engine.market_open_interest_lots(market_id),
            latest_market_risk_transition,
            recent_market_risk_transitions,
            top_liquidation_accounts,
        }
    }

    pub fn list_market_states(
        &self,
        stressed_only: bool,
        imminent_freeze_only: bool,
        outcome: Option<SuggestedLiquidationOutcome>,
        limit: usize,
    ) -> Vec<MarketStateResponse> {
        let market_ids: BTreeSet<u64> = self
            .engine
            .perpetual_markets()
            .map(|market| market.market_id)
            .chain(
                self.engine
                    .all_positions(None, true, usize::MAX)
                    .into_iter()
                    .map(|(market_id, _, _)| market_id),
            )
            .collect();
        let mut states: Vec<_> = market_ids
            .into_iter()
            .map(|market_id| self.get_market_state(market_id))
            .filter(|state| !stressed_only || state.total_liquidation_gap > 0)
            .filter(|state| !imminent_freeze_only || state.imminent_freeze_count > 0)
            .filter(|state| outcome.is_none_or(|value| market_state_matches_outcome(state, value)))
            .collect();
        states.sort_by_key(|state| {
            (
                Reverse(state.worst_liquidation_gap),
                Reverse(state.total_liquidation_gap),
                state.market_id,
            )
        });
        states.truncate(limit);
        states
    }

    pub fn get_liquidation_pressure_summary(
        &self,
        market_id: Option<u64>,
        outcome: Option<SuggestedLiquidationOutcome>,
    ) -> LiquidationPressureSummaryResponse {
        let market_states = self
            .list_market_states(false, false, outcome, usize::MAX)
            .into_iter()
            .filter(|state| market_id.is_none_or(|value| state.market_id == value))
            .collect::<Vec<_>>();
        let account_summaries =
            self.list_account_risk_summaries(market_id, false, outcome, false, usize::MAX);

        let stressed_market_count = market_states
            .iter()
            .filter(|state| state.total_liquidation_gap > 0)
            .count();
        let imminent_freeze_market_count = market_states
            .iter()
            .filter(|state| state.imminent_freeze_count > 0)
            .count();
        let imminent_freeze_account_count = account_summaries
            .iter()
            .filter(|summary| summary.imminent_freeze_market_count > 0)
            .count();
        let total_liquidation_candidate_count = market_states
            .iter()
            .map(|state| state.liquidation_candidate_count)
            .sum();
        let total_liquidation_gap = market_states
            .iter()
            .map(|state| state.total_liquidation_gap)
            .sum();
        let total_suggested_bad_debt = market_states
            .iter()
            .map(|state| state.total_suggested_bad_debt)
            .sum();
        let suggested_clear_count = market_states
            .iter()
            .map(|state| state.suggested_clear_count)
            .sum();
        let suggested_freeze_count = market_states
            .iter()
            .map(|state| state.suggested_freeze_count)
            .sum();
        let suggested_bad_debt_count = market_states
            .iter()
            .map(|state| state.suggested_bad_debt_count)
            .sum();
        let suggested_still_under_count = market_states
            .iter()
            .map(|state| {
                state
                    .liquidation_candidate_count
                    .saturating_sub(state.suggested_clear_count)
                    .saturating_sub(state.suggested_freeze_count)
                    .saturating_sub(state.suggested_bad_debt_count)
            })
            .sum();
        let latest_market_risk_transition = market_states
            .iter()
            .filter_map(|state| state.latest_market_risk_transition.clone())
            .max_by_key(|transition| transition.seq);
        let mut recent_market_risk_transitions = market_states
            .iter()
            .flat_map(|state| state.recent_market_risk_transitions.iter().cloned())
            .collect::<Vec<_>>();
        recent_market_risk_transitions.sort_by_key(|transition| Reverse(transition.seq));
        recent_market_risk_transitions.truncate(5);
        let worst_market = market_states.iter().max_by_key(|state| {
            (
                state.worst_liquidation_gap,
                state.total_liquidation_gap,
                Reverse(state.market_id),
            )
        });
        let worst_account = account_summaries.iter().max_by_key(|summary| {
            (
                summary.total_liquidation_gap,
                summary.worst_liquidation_gap,
                Reverse(summary.account_id),
            )
        });
        let top_markets = market_states.iter().take(3).cloned().collect::<Vec<_>>();
        let top_accounts = account_summaries
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>();

        LiquidationPressureSummaryResponse {
            market_count: market_states.len(),
            stressed_market_count,
            stressed_account_count: account_summaries.len(),
            imminent_freeze_market_count,
            imminent_freeze_account_count,
            total_liquidation_candidate_count,
            total_liquidation_gap,
            worst_market_id: worst_market.map(|state| state.market_id),
            worst_market_gap: worst_market.map_or(0, |state| state.worst_liquidation_gap),
            worst_account_id: worst_account.map(|summary| summary.account_id),
            worst_account_gap: worst_account.map_or(0, |summary| summary.total_liquidation_gap),
            suggested_clear_count,
            suggested_still_under_count,
            suggested_freeze_count,
            suggested_bad_debt_count,
            total_suggested_bad_debt,
            latest_market_risk_transition,
            recent_market_risk_transitions,
            top_markets,
            top_accounts,
        }
    }

    pub fn set_isolated_collateral(
        &mut self,
        market_id: u64,
        account_id: u64,
        collateral: i128,
        timestamp_ms: u64,
    ) -> RiskSnapshotResponse {
        self.engine
            .update_isolated_collateral(market_id, account_id, collateral, timestamp_ms);
        self.get_risk_snapshot(market_id, account_id)
    }

    pub fn get_wallet_balances(&self, address: [u8; 20]) -> WalletBalancesResponse {
        let policy_usdc_balance = self
            .engine
            .get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let evm_usdc_balance = self
            .engine
            .get_committed_state(&evm_usdc_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let evm_usdc_total_supply = self
            .engine
            .get_committed_state(&evm_usdc_total_supply_state_key())
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        WalletBalancesResponse {
            address,
            policy_usdc_balance: policy_usdc_balance.to_string(),
            evm_usdc_balance: evm_usdc_balance.to_string(),
            evm_usdc_total_supply: evm_usdc_total_supply.to_string(),
            next_seq: self.next_event_seq(),
        }
    }

    pub fn set_policy_usdc_balance(
        &mut self,
        address: [u8; 20],
        amount: U256,
    ) -> Result<PolicyUsdcBalanceResponse, RpcError> {
        let balance_key = policy_usdc_balance_state_key(address);
        let sets = (!amount.is_zero())
            .then(|| (balance_key.clone(), amount.to_be_bytes::<32>().to_vec()))
            .into_iter();
        let deletes = amount.is_zero().then(|| balance_key).into_iter();
        let state_root = self
            .engine
            .apply_committed_state_updates(sets, deletes)
            .map_err(|error| {
                RpcError::Node(format!("failed to set policy usdc balance: {error:?}"))
            })?;
        Ok(PolicyUsdcBalanceResponse {
            address,
            balance: amount.to_string(),
            state_root,
        })
    }

    pub fn faucet_fund_account(
        &mut self,
        address: [u8; 20],
        policy_target: U256,
        evm_target: U256,
    ) -> Result<FaucetFundAccountResponse, RpcError> {
        if policy_target > U256::from(MAX_FAUCET_POLICY_USDC_MICRO) {
            return Err(RpcError::InvalidRequest(
                "policy_usdc exceeds faucet cap".into(),
            ));
        }
        if evm_target > U256::from(MAX_FAUCET_EVM_USDC_MICRO) {
            return Err(RpcError::InvalidRequest(
                "evm_usdc exceeds faucet cap".into(),
            ));
        }

        let policy_key = policy_usdc_balance_state_key(address);
        let evm_key = evm_usdc_balance_state_key(address);
        let total_supply_key = evm_usdc_total_supply_state_key();
        let policy_current = self
            .engine
            .get_committed_state(&policy_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let evm_current = self
            .engine
            .get_committed_state(&evm_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let total_supply_current = self
            .engine
            .get_committed_state(&total_supply_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);

        let policy_next = policy_current.max(policy_target);
        let evm_delta = evm_target.saturating_sub(evm_current);

        let faucet_operator = if evm_delta.is_zero() {
            None
        } else {
            Some(FaucetOperatorSigner::from_env()?)
        };

        let mut sets = BTreeMap::new();
        let mut deletes = BTreeSet::new();
        if policy_next != policy_current {
            if policy_next.is_zero() {
                deletes.insert(policy_key.clone());
            } else {
                sets.insert(policy_key.clone(), policy_next.to_be_bytes::<32>().to_vec());
            }
        }

        if let Some(signer) = faucet_operator {
            let operator_policy_key = policy_usdc_balance_state_key(signer.address);
            let operator_policy_current = if signer.address == address {
                policy_next
            } else {
                self.engine
                    .get_committed_state(&operator_policy_key)
                    .map(|bytes| U256::from_be_slice(&bytes))
                    .unwrap_or(U256::ZERO)
            };
            let operator_policy_next =
                operator_policy_current
                    .checked_add(evm_delta)
                    .ok_or_else(|| {
                        RpcError::InvalidRequest("faucet operator policy balance overflow")
                    })?;
            sets.insert(
                operator_policy_key,
                operator_policy_next.to_be_bytes::<32>().to_vec(),
            );
            sets.insert(evm_usdc_minter_role_state_key(signer.address), vec![1_u8]);
        }

        if !sets.is_empty() || !deletes.is_empty() {
            self.engine
                .apply_committed_state_updates(sets, deletes)
                .map_err(|error| {
                    RpcError::Node(format!("failed to seed faucet state: {error:?}"))
                })?;
        }

        let evm_usdc_deposit_tx_hash = if let Some(signer) = faucet_operator {
            let nonce = self
                .engine
                .get_committed_state(&evm_account_nonce_state_key(signer.address))
                .map(|bytes| U256::from_be_slice(&bytes).to::<u64>())
                .unwrap_or(0);
            let (expected_tx_hash, raw_tx) = sign_evm_usdc_deposit_for_tx(
                self.engine.chain_id(),
                nonce,
                signer,
                address,
                evm_delta,
            )?;
            let submitted_tx_hash = self
                .engine
                .submit_raw_evm_transaction(raw_tx, self.engine.current_time_ms())
                .map_err(|error| {
                    RpcError::Node(format!(
                        "faucet evmDepositFor transaction failed: {error:?}"
                    ))
                })?;
            if submitted_tx_hash != expected_tx_hash {
                return Err(RpcError::Node(
                    "faucet evmDepositFor transaction hash mismatch".into(),
                ));
            }
            let receipt = self
                .engine
                .evm_transaction_receipt(&submitted_tx_hash)
                .ok_or_else(|| {
                    RpcError::Node("faucet evmDepositFor receipt was not indexed".into())
                })?;
            if !receipt.success {
                return Err(RpcError::Node(
                    "faucet evmDepositFor transaction reverted".into(),
                ));
            }
            Some(submitted_tx_hash)
        } else {
            None
        };

        let policy_final = self
            .engine
            .get_committed_state(&policy_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let evm_final = self
            .engine
            .get_committed_state(&evm_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let total_supply_final = self
            .engine
            .get_committed_state(&total_supply_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(total_supply_current);

        Ok(FaucetFundAccountResponse {
            address,
            policy_usdc_balance: policy_final.to_string(),
            evm_usdc_balance: evm_final.to_string(),
            evm_usdc_total_supply: total_supply_final.to_string(),
            evm_usdc_deposit_tx_hash,
            state_root: self.engine.committed_state_root(),
        })
    }

    pub fn faucet_fund_account_snapshot(&self, address: [u8; 20]) -> FaucetFundAccountResponse {
        let policy_key = policy_usdc_balance_state_key(address);
        let evm_key = evm_usdc_balance_state_key(address);
        let total_supply_key = evm_usdc_total_supply_state_key();
        let policy_final = self
            .engine
            .get_committed_state(&policy_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let evm_final = self
            .engine
            .get_committed_state(&evm_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let total_supply_final = self
            .engine
            .get_committed_state(&total_supply_key)
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        FaucetFundAccountResponse {
            address,
            policy_usdc_balance: policy_final.to_string(),
            evm_usdc_balance: evm_final.to_string(),
            evm_usdc_total_supply: total_supply_final.to_string(),
            evm_usdc_deposit_tx_hash: None,
            state_root: self.engine.committed_state_root(),
        }
    }

    pub fn set_evm_usdc_minter(
        &mut self,
        address: [u8; 20],
        enabled: bool,
    ) -> Result<EvmUsdcMinterRoleResponse, RpcError> {
        let sets = enabled
            .then(|| (evm_usdc_minter_role_state_key(address), vec![1_u8]))
            .into_iter();
        let deletes = (!enabled)
            .then(|| evm_usdc_minter_role_state_key(address))
            .into_iter();
        let state_root = self
            .engine
            .apply_committed_state_updates(sets, deletes)
            .map_err(|error| {
                RpcError::Node(format!("failed to set evm usdc minter role: {error:?}"))
            })?;
        Ok(EvmUsdcMinterRoleResponse {
            address,
            enabled,
            state_root,
        })
    }

    pub fn set_market_risk(
        &mut self,
        market_id: u64,
        maintenance_margin_bps: u32,
        liquidation_penalty_bps: Option<u32>,
        liquidator_fee_bps: Option<u32>,
        timestamp_ms: u64,
    ) -> MarketStateResponse {
        let current = self.engine.market_risk_parameters(market_id);
        self.engine.update_market_risk_parameters(
            market_id,
            MarketRiskParameters {
                maintenance_margin_bps,
                liquidation_penalty_bps: liquidation_penalty_bps
                    .unwrap_or(current.liquidation_penalty_bps),
                liquidator_fee_bps: liquidator_fee_bps.unwrap_or(current.liquidator_fee_bps),
            },
            timestamp_ms,
        );
        self.get_market_state(market_id)
    }

    pub fn set_funding_index(
        &mut self,
        market_id: u64,
        funding_index: i128,
        timestamp_ms: u64,
    ) -> MarketStateResponse {
        self.engine
            .update_market_funding_index(market_id, funding_index, timestamp_ms);
        self.get_market_state(market_id)
    }

    pub fn set_index_price(
        &mut self,
        market_id: u64,
        index_price_ticks: u64,
        timestamp_ms: u64,
    ) -> MarketStateResponse {
        self.engine
            .update_market_index_price(market_id, index_price_ticks, timestamp_ms);
        self.get_market_state(market_id)
    }

    pub fn set_mark_price(
        &mut self,
        market_id: u64,
        mark_price_ticks: u64,
        timestamp_ms: u64,
    ) -> MarketStateResponse {
        self.engine
            .update_market_mark_price(market_id, mark_price_ticks, timestamp_ms);
        self.get_market_state(market_id)
    }

    pub fn submit_oracle_prices(
        &mut self,
        market_id: u64,
        samples: Vec<OracleFeedSample>,
        timestamp_ms: u64,
    ) -> Result<OracleFeedAggregation, RpcError> {
        let aggregation_config = self
            .engine
            .oracle_aggregation_config(market_id)
            .map_err(oracle_submission_rpc_error)?;
        self.engine
            .submit_oracle_price_batch(market_id, samples, aggregation_config, timestamp_ms)
            .map_err(oracle_submission_rpc_error)
    }

    pub fn advance_funding_interval(
        &mut self,
        market_id: u64,
        timestamp_ms: u64,
    ) -> Result<MarketStateResponse, RpcError> {
        self.engine
            .advance_market_funding(market_id, timestamp_ms)
            .map_err(|error| order_submission_rpc_error(timestamp_ms, &error))?;
        Ok(self.get_market_state(market_id))
    }

    pub fn set_market_fees(
        &mut self,
        market_id: u64,
        maker_fee_bps: u32,
        maker_rebate_bps: u32,
        taker_fee_bps: u32,
    ) -> MarketStateResponse {
        self.engine.update_market_fee_parameters(
            market_id,
            MarketFeeParameters {
                maker_fee_bps,
                maker_rebate_bps,
                taker_fee_bps,
            },
        );
        self.get_market_state(market_id)
    }

    pub fn set_market_trading_state(
        &mut self,
        market_id: u64,
        paused: bool,
        timestamp_ms: u64,
    ) -> MarketStateResponse {
        self.engine
            .set_market_trading_paused(market_id, paused, timestamp_ms);
        self.get_market_state(market_id)
    }

    pub fn set_market_oracle_config(
        &mut self,
        market_id: u64,
        max_staleness_ms: u64,
        timestamp_ms: u64,
    ) -> MarketStateResponse {
        self.engine.update_market_oracle_config(
            market_id,
            OracleConfig { max_staleness_ms },
            timestamp_ms,
        );
        self.get_market_state(market_id)
    }

    pub(crate) fn list_risk_snapshots(
        &self,
        account_id: u64,
        options: RiskSnapshotListOptions,
    ) -> Vec<RiskSnapshotResponse> {
        self.engine
            .risk_positions(account_id, options.include_flat, options.limit)
            .into_iter()
            .map(|(market_id, ledger)| {
                risk_snapshot_from_ledger(
                    self.engine,
                    market_id,
                    account_id,
                    ledger,
                    self.engine.margin_snapshot(market_id, account_id),
                    self.engine.mark_price_ticks(market_id),
                    RiskSnapshotSupplement {
                        liquidation_stall_count: self
                            .engine
                            .liquidation_stall_count(market_id, account_id),
                        liquidation_freeze_threshold: self.engine.liquidation_freeze_threshold(),
                        liquidation_freeze_imminent: self
                            .engine
                            .liquidation_freeze_imminent(market_id, account_id),
                        suggested_liquidation_lots: suggested_liquidation_lots(
                            self.engine,
                            market_id,
                            account_id,
                        ),
                        latest_account_risk_transition: latest_account_risk_transition(
                            self.engine,
                            account_id,
                            Some(market_id),
                        ),
                    },
                )
            })
            .filter(|snapshot| {
                options
                    .market_id
                    .is_none_or(|value| snapshot.market_id == value)
            })
            .filter(|snapshot| !options.stressed_only || snapshot.under_maintenance)
            .filter(|snapshot| {
                !options.freeze_imminent_only || snapshot.liquidation_freeze_imminent
            })
            .filter(|snapshot| {
                options
                    .outcome
                    .is_none_or(|value| snapshot.suggested_liquidation_outcome == Some(value))
            })
            .collect()
    }

    pub fn get_account_risk_summary(
        &self,
        account_id: u64,
        market_id: Option<u64>,
        outcome: Option<SuggestedLiquidationOutcome>,
        include_flat: bool,
        limit: usize,
    ) -> AccountRiskSummaryResponse {
        let snapshots = self
            .list_risk_snapshots(
                account_id,
                RiskSnapshotListOptions {
                    market_id,
                    stressed_only: false,
                    freeze_imminent_only: false,
                    outcome,
                    include_flat,
                    limit: usize::MAX,
                },
            )
            .into_iter()
            .take(limit)
            .collect::<Vec<_>>();
        let total_collateral = snapshots.iter().fold(0_i128, |acc, snapshot| {
            acc.saturating_add(snapshot.collateral)
        });
        let total_realized_pnl = snapshots.iter().fold(0_i128, |acc, snapshot| {
            acc.saturating_add(snapshot.realized_pnl)
        });
        let total_unrealized_pnl = snapshots.iter().fold(0_i128, |acc, snapshot| {
            acc.saturating_add(snapshot.unrealized_pnl)
        });
        let total_equity = snapshots
            .iter()
            .fold(0_i128, |acc, snapshot| acc.saturating_add(snapshot.equity));
        let total_open_notional = snapshots.iter().fold(0_u128, |acc, snapshot| {
            acc.saturating_add(snapshot.open_notional)
        });
        let total_initial_margin_required = snapshots.iter().fold(0_u128, |acc, snapshot| {
            let initial_margin_bps = self
                .engine
                .perpetual_market(snapshot.market_id)
                .map(|market| market.config.initial_margin_bps)
                .unwrap_or(snapshot.maintenance_margin_bps);
            acc.saturating_add(proportional_bps_round_up_for_rpc(
                snapshot.mark_notional,
                initial_margin_bps,
            ))
        });
        let total_maintenance_requirement = snapshots.iter().fold(0_u128, |acc, snapshot| {
            acc.saturating_add(snapshot.maintenance_requirement)
        });
        let total_liquidation_gap = snapshots.iter().fold(0_u128, |acc, snapshot| {
            acc.saturating_add(snapshot.liquidation_gap)
        });
        let total_margin_surplus = snapshots.iter().fold(0_i128, |acc, snapshot| {
            acc.saturating_add(snapshot.margin_surplus)
        });
        let (worst_market_id, worst_liquidation_gap) = snapshots
            .iter()
            .map(|snapshot| (snapshot.market_id, snapshot.liquidation_gap))
            .max_by_key(|(market_id, gap)| (*gap, std::cmp::Reverse(*market_id)))
            .map(|(market_id, gap)| (Some(market_id), gap))
            .unwrap_or((None, 0));
        let max_liquidation_stall_count = snapshots
            .iter()
            .map(|snapshot| snapshot.liquidation_stall_count)
            .max()
            .unwrap_or(0);
        let imminent_freeze_market_count = snapshots
            .iter()
            .filter(|snapshot| snapshot.liquidation_freeze_imminent)
            .count();
        let under_maintenance_market_count = snapshots
            .iter()
            .filter(|snapshot| snapshot.under_maintenance)
            .count();
        let markets_cleared_by_suggested_liquidation = snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.suggested_liquidation_outcome
                    == Some(SuggestedLiquidationOutcome::ClearsMaintenance)
            })
            .count();
        let markets_still_under_after_suggested_liquidation = snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.suggested_liquidation_outcome
                    == Some(SuggestedLiquidationOutcome::StillUnderMaintenance)
            })
            .count();
        let markets_freezing_after_suggested_liquidation = snapshots
            .iter()
            .filter(|snapshot| {
                matches!(
                    snapshot.suggested_liquidation_outcome,
                    Some(SuggestedLiquidationOutcome::FreezesAccount)
                        | Some(SuggestedLiquidationOutcome::CreatesBadDebt)
                )
            })
            .count();
        let markets_creating_bad_debt_after_suggested_liquidation = snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.suggested_liquidation_outcome
                    == Some(SuggestedLiquidationOutcome::CreatesBadDebt)
            })
            .count();
        let total_suggested_bad_debt = snapshots.iter().fold(0_u128, |acc, snapshot| {
            acc.saturating_add(snapshot.suggested_post_market_bad_debt_balance.unwrap_or(0))
        });
        let total_bad_debt = snapshots.iter().fold(0_u128, |acc, snapshot| {
            acc.saturating_add(snapshot.equity.saturating_neg().max(0) as u128)
        });

        AccountRiskSummaryResponse {
            account_id,
            market_count: snapshots.len(),
            frozen: self.engine.account_frozen(account_id),
            latest_account_risk_transition: latest_account_risk_transition(
                self.engine,
                account_id,
                market_id,
            ),
            max_liquidation_stall_count,
            imminent_freeze_market_count,
            worst_market_id,
            worst_liquidation_gap,
            markets_cleared_by_suggested_liquidation,
            markets_still_under_after_suggested_liquidation,
            markets_freezing_after_suggested_liquidation,
            markets_creating_bad_debt_after_suggested_liquidation,
            total_suggested_bad_debt,
            total_bad_debt,
            total_collateral,
            total_realized_pnl,
            total_unrealized_pnl,
            total_equity,
            total_open_notional,
            total_initial_margin_required,
            total_maintenance_requirement,
            total_liquidation_gap,
            total_margin_surplus,
            under_maintenance_market_count,
        }
    }

    pub fn list_account_risk_summaries(
        &self,
        market_id: Option<u64>,
        imminent_freeze_only: bool,
        outcome: Option<SuggestedLiquidationOutcome>,
        include_flat: bool,
        limit: usize,
    ) -> Vec<AccountRiskSummaryResponse> {
        let account_ids: BTreeSet<u64> = self
            .engine
            .all_positions(market_id, include_flat, usize::MAX)
            .into_iter()
            .map(|(_, account_id, _)| account_id)
            .collect();
        let mut summaries: Vec<_> = account_ids
            .into_iter()
            .map(|account_id| {
                self.get_account_risk_summary(account_id, market_id, outcome, include_flat, limit)
            })
            .filter(|summary| include_flat || summary.total_liquidation_gap > 0)
            .filter(|summary| !imminent_freeze_only || summary.imminent_freeze_market_count > 0)
            .filter(|summary| {
                outcome.is_none_or(|value| account_summary_matches_outcome(summary, value))
            })
            .collect();
        summaries.sort_by_key(|summary| {
            (
                Reverse(summary.total_liquidation_gap),
                Reverse(summary.worst_liquidation_gap),
                summary.account_id,
            )
        });
        summaries.truncate(limit);
        summaries
    }

    pub fn list_liquidation_candidates(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        freeze_imminent_only: bool,
        outcome: Option<SuggestedLiquidationOutcome>,
        limit: usize,
    ) -> Vec<RiskSnapshotResponse> {
        let mut candidates: Vec<_> = self
            .engine
            .all_positions(market_id, false, limit.saturating_mul(4).max(limit))
            .into_iter()
            .filter_map(|(entry_market_id, account_id, ledger)| {
                if !self.engine.liquidation_price_inputs_ready(entry_market_id) {
                    return None;
                }
                let snapshot = risk_snapshot_from_ledger(
                    self.engine,
                    entry_market_id,
                    account_id,
                    ledger,
                    self.engine.margin_snapshot(entry_market_id, account_id),
                    self.engine.mark_price_ticks(entry_market_id),
                    RiskSnapshotSupplement {
                        liquidation_stall_count: self
                            .engine
                            .liquidation_stall_count(entry_market_id, account_id),
                        liquidation_freeze_threshold: self.engine.liquidation_freeze_threshold(),
                        liquidation_freeze_imminent: self
                            .engine
                            .liquidation_freeze_imminent(entry_market_id, account_id),
                        suggested_liquidation_lots: suggested_liquidation_lots(
                            self.engine,
                            entry_market_id,
                            account_id,
                        ),
                        latest_account_risk_transition: latest_account_risk_transition(
                            self.engine,
                            account_id,
                            Some(entry_market_id),
                        ),
                    },
                );
                snapshot.under_maintenance.then_some(snapshot)
            })
            .filter(|snapshot| account_id.is_none_or(|value| snapshot.account_id == value))
            .filter(|snapshot| !freeze_imminent_only || snapshot.liquidation_freeze_imminent)
            .filter(|snapshot| {
                outcome.is_none_or(|value| snapshot.suggested_liquidation_outcome == Some(value))
            })
            .collect();
        candidates.sort_by_key(|snapshot| {
            (
                snapshot.margin_surplus,
                snapshot.market_id,
                snapshot.account_id,
            )
        });
        candidates.truncate(limit);
        candidates
    }

    pub fn get_risk_state_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<RiskStateHistoryRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(risk_state_history_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_liquidation_candidate_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<LiquidationCandidateHistoryRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(liquidation_candidate_history_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .take(limit)
            .collect()
    }
}
