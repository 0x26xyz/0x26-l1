use super::*;

impl SingleNodeEngine {
    pub fn update_market_oracle_config(
        &mut self,
        market_id: u64,
        config: OracleConfig,
        timestamp_ms: u64,
    ) {
        self.market_oracle_config.insert(market_id, config);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::OracleConfigUpdated {
                market_id,
                max_staleness_ms: config.max_staleness_ms,
            },
        );
    }

    pub fn liquidation_price_inputs_ready(&self, market_id: u64) -> bool {
        self.mark_price_status(market_id)
            .is_some_and(|status| !status.stale)
            && self
                .index_price_status(market_id)
                .is_none_or(|status| !status.stale)
    }

    pub fn order_price_inputs_ready(&self, market_id: u64) -> bool {
        market_price_inputs_ready_at(
            market_id,
            self.current_time_ms(),
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
        )
    }

    pub fn market_risk_parameters(&self, market_id: u64) -> MarketRiskParameters {
        self.market_risk
            .get(&market_id)
            .copied()
            .unwrap_or(MarketRiskParameters {
                maintenance_margin_bps: DEFAULT_MAINTENANCE_MARGIN_BPS,
                liquidation_penalty_bps: 0,
                liquidator_fee_bps: 0,
            })
    }

    pub fn set_market_risk_parameters(&mut self, market_id: u64, parameters: MarketRiskParameters) {
        self.market_risk.insert(market_id, parameters);
    }

    pub fn market_funding_index(&self, market_id: u64) -> i128 {
        self.market_funding_index
            .get(&market_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn market_last_funding_ms(&self, market_id: u64) -> Option<u64> {
        self.market_last_funding_ms.get(&market_id).copied()
    }

    pub(super) fn funding_interval_due_at(&self, market_id: u64, timestamp_ms: u64) -> bool {
        let Some(market) = self.markets.get(&market_id) else {
            return false;
        };
        if market.status == MarketStatus::Delisted {
            return false;
        }
        let last_funding_ms = self
            .market_last_funding_ms
            .get(&market_id)
            .copied()
            .unwrap_or(market.listed_at_ms);
        timestamp_ms >= last_funding_ms.saturating_add(market.config.funding_interval_ms)
    }

    pub(super) fn indicative_funding_rate_ppm_from_prices(
        market_id: u64,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
    ) -> Option<i64> {
        indicative_funding_rate_ppm(
            market_marks.get(&market_id)?.price_ticks,
            market_index_prices.get(&market_id)?.price_ticks,
        )
    }

    pub(super) fn next_funding_index_at_with_prices(
        &self,
        market_id: u64,
        timestamp_ms: u64,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
    ) -> Option<i128> {
        if !self.funding_interval_due_at(market_id, timestamp_ms)
            || !market_price_inputs_ready_at(
                market_id,
                timestamp_ms,
                market_marks,
                market_index_prices,
                &self.market_oracle_config,
            )
        {
            return None;
        }
        let index_price_ticks = market_index_prices.get(&market_id)?.price_ticks;
        let funding_rate_ppm = Self::indicative_funding_rate_ppm_from_prices(
            market_id,
            market_marks,
            market_index_prices,
        )?;
        Some(
            self.market_funding_index(market_id)
                .saturating_add(funding_index_delta(index_price_ticks, funding_rate_ppm)),
        )
    }

    pub(super) fn next_funding_index_at(&self, market_id: u64, timestamp_ms: u64) -> Option<i128> {
        self.next_funding_index_at_with_prices(
            market_id,
            timestamp_ms,
            &self.market_marks,
            &self.market_index_prices,
        )
    }

    pub(super) fn consensus_funding_items_with_prices(
        &self,
        timestamp_ms: u64,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
    ) -> Vec<BlockItem> {
        self.markets
            .keys()
            .filter_map(|market_id| {
                let funding_index = self.next_funding_index_at_with_prices(
                    *market_id,
                    timestamp_ms,
                    market_marks,
                    market_index_prices,
                )?;
                Some(BlockItem::Funding(BlockFunding {
                    market_id: *market_id,
                    funding_index,
                }))
            })
            .collect()
    }

    pub(super) fn consensus_funding_items(&self, timestamp_ms: u64) -> Vec<BlockItem> {
        self.consensus_funding_items_with_prices(
            timestamp_ms,
            &self.market_marks,
            &self.market_index_prices,
        )
    }

    pub(super) fn positions_after_funding_items_with_prices(
        &self,
        items: &[BlockItem],
        timestamp_ms: u64,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
    ) -> BTreeMap<(u64, u64), PositionLedger> {
        let mut positions = self.positions.clone();
        for item in items {
            let BlockItem::Funding(funding) = item else {
                continue;
            };
            if self.next_funding_index_at_with_prices(
                funding.market_id,
                timestamp_ms,
                market_marks,
                market_index_prices,
            ) != Some(funding.funding_index)
            {
                continue;
            }
            for ((market_id, _), ledger) in &mut positions {
                if *market_id == funding.market_id && ledger.open_abs_lots() > 0 {
                    ledger.apply_funding(funding.funding_index);
                }
            }
        }
        positions
    }

    pub(super) fn positions_after_funding_items(
        &self,
        items: &[BlockItem],
        timestamp_ms: u64,
    ) -> BTreeMap<(u64, u64), PositionLedger> {
        self.positions_after_funding_items_with_prices(
            items,
            timestamp_ms,
            &self.market_marks,
            &self.market_index_prices,
        )
    }

    pub fn market_fee_parameters(&self, market_id: u64) -> MarketFeeParameters {
        self.market_fees
            .get(&market_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn market_trading_paused(&self, market_id: u64) -> bool {
        self.paused_markets.contains(&market_id)
    }

    pub(super) fn market_fee_parameters_are_valid(fees: MarketFeeParameters) -> bool {
        fees.maker_fee_bps <= BPS_DENOMINATOR as u32
            && fees.maker_rebate_bps <= BPS_DENOMINATOR as u32
            && fees.taker_fee_bps <= BPS_DENOMINATOR as u32
            // Baseline maker rebates are conservatively bounded to the
            // same-fill gross fees. RFC 027 multiplier bonuses extend this
            // via the MM rebate pool in a later slice.
            && u64::from(fees.maker_rebate_bps)
                <= u64::from(fees.maker_fee_bps) + u64::from(fees.taker_fee_bps)
    }

    pub fn set_market_trading_paused(&mut self, market_id: u64, paused: bool, timestamp_ms: u64) {
        self.set_market_trading_paused_at(None, market_id, paused, timestamp_ms);
    }

    pub(super) fn set_market_trading_paused_at(
        &mut self,
        block_height: Option<u64>,
        market_id: u64,
        paused: bool,
        timestamp_ms: u64,
    ) {
        if paused {
            self.paused_markets.insert(market_id);
        } else {
            self.paused_markets.remove(&market_id);
        }
        if let Some(market) = self.markets.get_mut(&market_id) {
            let previous_status = market.status;
            market.updated_at_ms = timestamp_ms;
            if paused && market.status != MarketStatus::Delisted {
                market.status = MarketStatus::Paused;
            } else if !paused && market.status == MarketStatus::Paused {
                market.status = MarketStatus::Active;
            }
            let status = market.status;
            if status != previous_status {
                self.record_runtime_event(
                    block_height,
                    timestamp_ms,
                    RuntimeEventKind::MarketStatusUpdated {
                        market_id,
                        status: market_status_event(status),
                    },
                );
            }
        }
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::MarketTradingStateUpdated { market_id, paused },
        );
    }

    pub fn fee_vault_balance(&self, market_id: u64) -> u128 {
        self.fee_vaults.get(&market_id).copied().unwrap_or(0)
    }

    pub fn validator_reward_pool_balance(&self, market_id: u64) -> u128 {
        self.validator_reward_pools
            .get(&market_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn market_maker_rebate_pool_balance(&self, market_id: u64) -> u128 {
        self.market_maker_rebate_pools
            .get(&market_id)
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn best_live_market_maker_enrollment(
        &self,
        market_id: u64,
        economic_account_id: u64,
    ) -> Option<(&MarketMakerEnrollment, &MarketMakerProgram)> {
        self.best_market_maker_enrollment_matching_status(
            market_id,
            economic_account_id,
            market_maker_enrollment_status_is_live,
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn best_multiplier_eligible_market_maker_enrollment(
        &self,
        market_id: u64,
        economic_account_id: u64,
    ) -> Option<(&MarketMakerEnrollment, &MarketMakerProgram)> {
        self.best_market_maker_enrollment_matching_status(
            market_id,
            economic_account_id,
            market_maker_enrollment_status_is_multiplier_eligible,
        )
    }

    pub(super) fn best_market_maker_enrollment_matching_status(
        &self,
        market_id: u64,
        economic_account_id: u64,
        status_filter: fn(MarketMakerEnrollmentStatus) -> bool,
    ) -> Option<(&MarketMakerEnrollment, &MarketMakerProgram)> {
        let identity =
            market_maker_identity_for_economic_account(&self.delegations, economic_account_id)?;
        self.market_maker_enrollments
            .iter()
            .filter(
                |((candidate_identity, candidate_market_id, program_id), enrollment)| {
                    *candidate_identity == identity
                        && *candidate_market_id == market_id
                        && enrollment.market_maker_identity == identity
                        && enrollment.market_id == market_id
                        && enrollment.program_id == *program_id
                        && status_filter(enrollment.status)
                },
            )
            .filter_map(|((_, _, program_id), enrollment)| {
                let program = self.market_maker_programs.get(program_id)?;
                (program.status == MarketMakerProgramStatus::Active)
                    .then_some((enrollment, program))
            })
            .max_by(
                |(left_enrollment, left_program), (right_enrollment, right_program)| {
                    // Fill attribution prefers any Active enrollment over
                    // GracePeriod / NonCompliant. Within the chosen live-status
                    // bucket, highest multiplier wins. Equal multipliers break
                    // ties on lexicographically lowest program_id so the
                    // attribution anchor is explicit rather than an artifact
                    // of map order.
                    market_maker_enrollment_status_selection_priority(left_enrollment.status)
                        .cmp(&market_maker_enrollment_status_selection_priority(
                            right_enrollment.status,
                        ))
                        .then_with(|| {
                            left_program
                                .rebate_multiplier_units
                                .cmp(&right_program.rebate_multiplier_units)
                        })
                        .then_with(|| right_enrollment.program_id.cmp(&left_enrollment.program_id))
                },
            )
    }

    pub fn treasury_balance(&self, market_id: u64) -> u128 {
        self.treasury_balances.get(&market_id).copied().unwrap_or(0)
    }

    pub(super) fn settle_order_fill_fees(
        &mut self,
        market_id: u64,
        maker_account_id: u64,
        maker_client_order_id: u64,
        taker_account_id: u64,
        taker_client_order_id: u64,
        price_ticks: u64,
        quantity_lots: u64,
    ) -> (
        Vec<RuntimeEventKind>,
        Option<RuntimeEventKind>,
        Option<u64>,
        Option<u64>,
    ) {
        let fee_parameters = self.market_fee_parameters(market_id);
        let notional = order_notional_usdc(&self.markets, market_id, price_ticks, quantity_lots);
        let maker_fee_amount = proportional_bps_round_up(notional, fee_parameters.maker_fee_bps);
        let maker_rebate_amount =
            proportional_bps_round_up(notional, fee_parameters.maker_rebate_bps);
        let taker_fee_amount = proportional_bps_round_up(notional, fee_parameters.taker_fee_bps);
        let mut fee_events = Vec::new();
        let mut pause_market_id = None;

        if maker_fee_amount > 0 {
            let updated_collateral = self
                .isolated_collateral(market_id, maker_account_id)
                .saturating_sub(u128_to_i128_saturating(maker_fee_amount));
            self.set_isolated_collateral(market_id, maker_account_id, updated_collateral);
            let fee_vault_balance = {
                let entry = self.fee_vaults.entry(market_id).or_insert(0);
                *entry = entry.saturating_add(maker_fee_amount);
                *entry
            };
            fee_events.push(RuntimeEventKind::FeeCharged {
                market_id,
                account_id: maker_account_id,
                client_order_id: maker_client_order_id,
                role: OrderFillRole::Maker,
                fee_amount: maker_fee_amount,
                fee_vault_balance,
            });
        }

        if taker_fee_amount > 0 {
            let updated_collateral = self
                .isolated_collateral(market_id, taker_account_id)
                .saturating_sub(u128_to_i128_saturating(taker_fee_amount));
            self.set_isolated_collateral(market_id, taker_account_id, updated_collateral);
            let fee_vault_balance = {
                let entry = self.fee_vaults.entry(market_id).or_insert(0);
                *entry = entry.saturating_add(taker_fee_amount);
                *entry
            };
            fee_events.push(RuntimeEventKind::FeeCharged {
                market_id,
                account_id: taker_account_id,
                client_order_id: taker_client_order_id,
                role: OrderFillRole::Taker,
                fee_amount: taker_fee_amount,
                fee_vault_balance,
            });
        }

        let gross_fee_collected = maker_fee_amount.saturating_add(taker_fee_amount);
        assert!(
            maker_rebate_amount <= gross_fee_collected,
            "baseline maker rebate exceeded same-fill gross fees",
        );

        if maker_rebate_amount > 0 {
            let updated_collateral = self
                .isolated_collateral(market_id, maker_account_id)
                .saturating_add(u128_to_i128_saturating(maker_rebate_amount));
            self.set_isolated_collateral(market_id, maker_account_id, updated_collateral);
            let entry = self.fee_vaults.entry(market_id).or_insert(0);
            *entry = entry.saturating_sub(maker_rebate_amount);
        }

        let mut distributable_revenue = gross_fee_collected.saturating_sub(maker_rebate_amount);
        let mut market_maker_pool_draw = 0_u128;

        if let Some((enrollment, program)) =
            self.best_live_market_maker_enrollment(market_id, maker_account_id)
        {
            let market_maker_identity = enrollment.market_maker_identity;
            let enrollment_status = enrollment.status;
            let program_id = program.program_id;
            let rebate_multiplier_units = program.rebate_multiplier_units;
            let requested_bonus_micro_usdc = maker_rebate_amount
                .saturating_mul(u128::from(rebate_multiplier_units))
                .saturating_div(BPS_DENOMINATOR)
                .saturating_sub(maker_rebate_amount);
            if requested_bonus_micro_usdc > 0 {
                match enrollment_status {
                    MarketMakerEnrollmentStatus::Active => {
                        let bonus_from_same_fill =
                            requested_bonus_micro_usdc.min(distributable_revenue);
                        distributable_revenue =
                            distributable_revenue.saturating_sub(bonus_from_same_fill);
                        if bonus_from_same_fill > 0 {
                            let entry = self.fee_vaults.entry(market_id).or_insert(0);
                            *entry = entry.saturating_sub(bonus_from_same_fill);
                        }
                        let bonus_shortfall =
                            requested_bonus_micro_usdc.saturating_sub(bonus_from_same_fill);
                        let available_pool = self.market_maker_rebate_pool_balance(market_id);
                        let bonus_from_pool = bonus_shortfall.min(available_pool);
                        market_maker_pool_draw = bonus_from_pool;
                        let paid_bonus_micro_usdc =
                            bonus_from_same_fill.saturating_add(bonus_from_pool);
                        if paid_bonus_micro_usdc > 0 {
                            let updated_collateral = self
                                .isolated_collateral(market_id, maker_account_id)
                                .saturating_add(u128_to_i128_saturating(paid_bonus_micro_usdc));
                            self.set_isolated_collateral(
                                market_id,
                                maker_account_id,
                                updated_collateral,
                            );
                            if let Some(enrollment) =
                                self.market_maker_enrollments
                                    .get_mut(&market_maker_enrollment_key(
                                        market_maker_identity,
                                        market_id,
                                        program_id,
                                    ))
                            {
                                let current = U256::from_be_slice(
                                    &enrollment.lifetime_compliant_rebates_micro_usdc.0,
                                );
                                let updated =
                                    current.saturating_add(U256::from(paid_bonus_micro_usdc));
                                enrollment.lifetime_compliant_rebates_micro_usdc =
                                    MarketMakerU256(updated.to_be_bytes::<32>());
                            }
                            fee_events.push(RuntimeEventKind::MmProgramBonusPaid {
                                market_maker_identity,
                                economic_account_id: maker_account_id,
                                market_id,
                                program_id,
                                requested_bonus_micro_usdc,
                                paid_bonus_micro_usdc,
                            });
                        }
                        if paid_bonus_micro_usdc < requested_bonus_micro_usdc {
                            // RFC 014 pool insolvency edge: drain all available
                            // MM subsidy funding for the triggering fill, then
                            // pause the market before later fills can assume the
                            // multiplier remains fundable.
                            pause_market_id = Some(market_id);
                        }
                    }
                    MarketMakerEnrollmentStatus::GracePeriod
                    | MarketMakerEnrollmentStatus::NonCompliant => {
                        if let Some(enrollment) =
                            self.market_maker_enrollments
                                .get_mut(&market_maker_enrollment_key(
                                    market_maker_identity,
                                    market_id,
                                    program_id,
                                ))
                        {
                            let current = U256::from_be_slice(
                                &enrollment.lifetime_forfeited_rebates_micro_usdc.0,
                            );
                            let updated =
                                current.saturating_add(U256::from(requested_bonus_micro_usdc));
                            enrollment.lifetime_forfeited_rebates_micro_usdc =
                                MarketMakerU256(updated.to_be_bytes::<32>());
                        }
                        fee_events.push(RuntimeEventKind::MmProgramBonusForfeited {
                            market_maker_identity,
                            economic_account_id: maker_account_id,
                            market_id,
                            program_id,
                            forfeited_bonus_micro_usdc: requested_bonus_micro_usdc,
                        });
                    }
                    MarketMakerEnrollmentStatus::Exited | MarketMakerEnrollmentStatus::Slashed => {}
                }
            }
        }

        if market_maker_pool_draw > 0 {
            // A single fill may draw from the pre-existing MM rebate pool here
            // and later contribute back into that pool through the revenue
            // split below. Event consumers need both sides of that churn to
            // reconcile the pool over the fill.
            let entry = self.market_maker_rebate_pools.entry(market_id).or_insert(0);
            *entry = entry.saturating_sub(market_maker_pool_draw);
        }

        if distributable_revenue == 0 {
            return (fee_events, None, None, pause_market_id);
        }

        let validator_rewards_delta =
            proportional_bps_floor(distributable_revenue, VALIDATOR_REWARD_SPLIT_BPS);
        let market_maker_rebate_delta =
            proportional_bps_floor(distributable_revenue, MARKET_MAKER_REBATE_SPLIT_BPS);
        let insurance_fund_delta =
            proportional_bps_floor(distributable_revenue, INSURANCE_FUND_SPLIT_BPS);
        let treasury_delta = distributable_revenue
            .saturating_sub(validator_rewards_delta)
            .saturating_sub(market_maker_rebate_delta)
            .saturating_sub(insurance_fund_delta);
        {
            let entry = self.fee_vaults.entry(market_id).or_insert(0);
            *entry = entry.saturating_sub(distributable_revenue);
        }
        let validator_reward_balance = {
            let entry = self.validator_reward_pools.entry(market_id).or_insert(0);
            *entry = entry.saturating_add(validator_rewards_delta);
            *entry
        };
        let market_maker_rebate_balance = {
            let entry = self.market_maker_rebate_pools.entry(market_id).or_insert(0);
            *entry = entry.saturating_add(market_maker_rebate_delta);
            *entry
        };
        let insurance_fund_balance = {
            let entry = self.insurance_funds.entry(market_id).or_insert(0);
            *entry = entry.saturating_add(insurance_fund_delta);
            *entry
        };
        let treasury_balance = {
            let entry = self.treasury_balances.entry(market_id).or_insert(0);
            *entry = entry.saturating_add(treasury_delta);
            *entry
        };
        (
            fee_events,
            Some(RuntimeEventKind::FeeRevenueDistributed {
                market_id,
                source_fee_amount: gross_fee_collected,
                validator_rewards_delta,
                validator_reward_balance,
                market_maker_rebate_delta,
                market_maker_rebate_balance,
                insurance_fund_delta,
                insurance_fund_balance,
                treasury_delta,
                treasury_balance,
            }),
            (insurance_fund_delta > 0).then_some(market_id),
            pause_market_id,
        )
    }

    pub(super) fn evaluate_market_maker_compliance_at_head(&mut self) {
        let block_height = self.head().header.height;
        let timestamp_ms = self.head().header.timestamp_ms;
        let live_keys: Vec<MarketMakerEnrollmentKey> = self
            .market_maker_enrollments
            .iter()
            .filter_map(|(key, enrollment)| {
                market_maker_enrollment_status_is_live(enrollment.status).then_some(*key)
            })
            .collect();

        for key in live_keys {
            let Some(enrollment) = self.market_maker_enrollments.get(&key).cloned() else {
                continue;
            };
            let Some(program) = self.market_maker_programs.get(&key.2).cloned() else {
                continue;
            };
            let Some(sample_slot_ms) =
                next_market_maker_observation_slot(&enrollment, &program, timestamp_ms)
            else {
                continue;
            };
            let economic_account_id = market_maker_identity_economic_account(key.0);
            let sample = self.observe_market_maker_compliance_sample(
                key.1,
                economic_account_id,
                sample_slot_ms,
            );

            let events = {
                let Some(enrollment) = self.market_maker_enrollments.get_mut(&key) else {
                    continue;
                };
                enrollment.observation_history.push(sample);
                let retention_cutoff = sample_slot_ms
                    .saturating_sub(market_maker_compliance_history_retention_ms(&program));
                enrollment
                    .observation_history
                    .retain(|entry| entry.observed_at_ms > retention_cutoff);
                let prior_breach_counter = enrollment.current_compliance.breach_counter;
                enrollment.current_compliance = aggregate_market_maker_compliance(
                    &enrollment.observation_history,
                    &program,
                    sample_slot_ms,
                );
                enrollment.current_compliance.breach_counter = prior_breach_counter;
                enrollment.last_compliance_checked_ms = sample_slot_ms;
                evaluate_market_maker_enrollment_transition(
                    enrollment,
                    &program,
                    sample_slot_ms,
                    economic_account_id,
                )
            };
            for event in events {
                self.record_runtime_event(Some(block_height), timestamp_ms, event);
            }
        }
    }

    pub(super) fn observe_market_maker_compliance_sample(
        &self,
        market_id: u64,
        economic_account_id: u64,
        observed_at_ms: u64,
    ) -> MarketMakerObservationSample {
        let orders =
            self.account_orders(market_id, economic_account_id, observed_at_ms, usize::MAX);
        let mut best_bid_price_ticks = None;
        let mut best_bid_lots = 0_u64;
        let mut best_ask_price_ticks = None;
        let mut best_ask_lots = 0_u64;

        for order in orders {
            match order.side {
                MatchingOrderSide::Bid => match best_bid_price_ticks {
                    Some(price_ticks) if order.price_ticks < price_ticks => {}
                    Some(price_ticks) if order.price_ticks == price_ticks => {
                        best_bid_lots = best_bid_lots.saturating_add(order.remaining_lots);
                    }
                    _ => {
                        best_bid_price_ticks = Some(order.price_ticks);
                        best_bid_lots = order.remaining_lots;
                    }
                },
                MatchingOrderSide::Ask => match best_ask_price_ticks {
                    Some(price_ticks) if order.price_ticks > price_ticks => {}
                    Some(price_ticks) if order.price_ticks == price_ticks => {
                        best_ask_lots = best_ask_lots.saturating_add(order.remaining_lots);
                    }
                    _ => {
                        best_ask_price_ticks = Some(order.price_ticks);
                        best_ask_lots = order.remaining_lots;
                    }
                },
            }
        }

        let bid_present = best_bid_price_ticks.is_some();
        let ask_present = best_ask_price_ticks.is_some();
        let two_sided = bid_present && ask_present;
        let spread_bps = match (best_bid_price_ticks, best_ask_price_ticks) {
            (Some(bid_price_ticks), Some(ask_price_ticks)) => {
                market_maker_spread_bps(bid_price_ticks, ask_price_ticks)
            }
            _ => 0,
        };
        let quote_size_micro_usdc = match (best_bid_price_ticks, best_ask_price_ticks) {
            (Some(bid_price_ticks), Some(ask_price_ticks)) => {
                let bid_size =
                    order_notional_usdc(&self.markets, market_id, bid_price_ticks, best_bid_lots);
                let ask_size =
                    order_notional_usdc(&self.markets, market_id, ask_price_ticks, best_ask_lots);
                MarketMakerU256::from_u128(bid_size.min(ask_size))
            }
            _ => MarketMakerU256::default(),
        };

        MarketMakerObservationSample {
            observed_at_ms,
            bid_present,
            ask_present,
            two_sided,
            spread_bps,
            quote_size_micro_usdc,
        }
    }

    pub(super) fn market_custody_required_balance(
        &self,
        market_id: u64,
    ) -> Result<i128, &'static str> {
        let mut total = 0_i128;
        for ((_entry_market_id, _account_id), collateral) in self
            .isolated_collateral
            .range((market_id, 0)..=(market_id, u64::MAX))
        {
            total = total
                .checked_add(*collateral)
                .ok_or("market_custody_required_balance_overflow")?;
        }

        for bucket_balance in [
            self.fee_vault_balance(market_id),
            self.insurance_fund_balance(market_id),
            self.validator_reward_pool_balance(market_id),
            self.market_maker_rebate_pool_balance(market_id),
            self.treasury_balance(market_id),
        ] {
            total = total
                .checked_add(
                    i128::try_from(bucket_balance)
                        .map_err(|_| "market_custody_required_balance_overflow")?,
                )
                .ok_or("market_custody_required_balance_overflow")?;
        }

        Ok(total)
    }

    pub fn update_market_funding_index(
        &mut self,
        market_id: u64,
        funding_index: i128,
        timestamp_ms: u64,
    ) {
        self.market_funding_index.insert(market_id, funding_index);
        self.market_last_funding_ms.insert(market_id, timestamp_ms);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::FundingIndexUpdated {
                market_id,
                funding_index,
            },
        );

        let account_ids: Vec<u64> = self
            .positions
            .iter()
            .filter_map(|((entry_market_id, account_id), ledger)| {
                (*entry_market_id == market_id && ledger.open_abs_lots() > 0).then_some(*account_id)
            })
            .collect();

        for account_id in account_ids {
            let (mandate_id, funding_delta) = {
                let ledger = self.positions.entry((market_id, account_id)).or_default();
                let mandate_id = ledger.mandate_id;
                let funding_delta = ledger.apply_funding(funding_index);
                (mandate_id, funding_delta)
            };
            if let (Some(mandate_id), Some(market)) = (mandate_id, self.markets.get(&market_id)) {
                let scaled_delta = scaled_signed_raw_tick_lot_pnl_usdc(market, funding_delta);
                self.add_mandate_realized_pnl(mandate_id, scaled_delta);
            }
            if funding_delta != 0 {
                self.record_runtime_event(
                    None,
                    timestamp_ms,
                    RuntimeEventKind::FundingApplied {
                        market_id,
                        account_id,
                        funding_delta,
                        funding_index,
                    },
                );
            }
            self.record_risk_state_update(None, timestamp_ms, market_id, account_id);
        }
        self.reconcile_bad_debt_for_market(
            market_id,
            timestamp_ms,
            RiskTransitionSource::FundingUpdate,
        );
        self.reconcile_frozen_accounts_for_market(
            market_id,
            timestamp_ms,
            RiskTransitionSource::FundingUpdate,
        );
    }

    pub fn update_market_index_price(
        &mut self,
        market_id: u64,
        index_price_ticks: u64,
        timestamp_ms: u64,
    ) {
        self.market_index_prices.insert(
            market_id,
            OraclePrice {
                price_ticks: index_price_ticks,
                timestamp_ms,
            },
        );
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::IndexPriceUpdated {
                market_id,
                index_price_ticks,
            },
        );
    }

    pub fn update_market_mark_price(
        &mut self,
        market_id: u64,
        mark_price_ticks: u64,
        timestamp_ms: u64,
    ) {
        self.market_marks.insert(
            market_id,
            OraclePrice {
                price_ticks: mark_price_ticks,
                timestamp_ms,
            },
        );
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MarkPriceUpdated {
                market_id,
                mark_price_ticks,
            },
        );
        self.record_risk_state_updates_for_market(None, timestamp_ms, market_id);
        self.reconcile_bad_debt_for_market(
            market_id,
            timestamp_ms,
            RiskTransitionSource::MarkPriceReprice,
        );
        self.reconcile_frozen_accounts_for_market(
            market_id,
            timestamp_ms,
            RiskTransitionSource::MarkPriceReprice,
        );
    }

    pub fn mahi_oracle_price_payload(&self) -> Vec<BlockOraclePrice> {
        let mut market_ids = BTreeSet::new();
        market_ids.extend(self.market_marks.keys().copied());
        market_ids.extend(self.market_index_prices.keys().copied());
        market_ids
            .into_iter()
            .filter_map(|market_id| {
                let mark = self
                    .market_marks
                    .get(&market_id)
                    .map(block_oracle_value_from_price);
                let index = self
                    .market_index_prices
                    .get(&market_id)
                    .map(block_oracle_value_from_price);
                (mark.is_some() || index.is_some()).then_some(BlockOraclePrice {
                    market_id,
                    mark,
                    index,
                })
            })
            .collect()
    }

    pub(super) fn oracle_price_maps_for_mahi_block(
        &self,
        oracle_prices: &[BlockOraclePrice],
    ) -> (BTreeMap<u64, OraclePrice>, BTreeMap<u64, OraclePrice>) {
        if oracle_prices.is_empty() {
            return (self.market_marks.clone(), self.market_index_prices.clone());
        }
        oracle_price_maps_from_block_observations(oracle_prices)
    }

    pub(super) fn apply_block_oracle_price_items(&mut self, block: &Block) {
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
            oracle_price_maps_from_block_observations(&oracle_prices);
        // Reject rollbacks: a committed oracle payload with an older
        // `timestamp_ms` than the engine already has must not demote the
        // node's mark/index. `oracle_price_maps_from_block_observations`
        // already picks the freshest entry within the block; this guard
        // does the same against the engine's existing state. Without it,
        // a follower replaying a stale oracle item would briefly trip
        // `mark_price_stale` and reject IOC market orders that were
        // perfectly legal at the leader's admission time.
        for (market_id, mark) in market_marks {
            insert_newer_oracle_price(&mut self.market_marks, market_id, mark);
        }
        for (market_id, index) in market_index_prices {
            insert_newer_oracle_price(&mut self.market_index_prices, market_id, index);
        }
    }

    pub fn advance_market_funding(
        &mut self,
        market_id: u64,
        timestamp_ms: u64,
    ) -> Result<i128, NodeError> {
        if !market_price_inputs_ready_at(
            market_id,
            timestamp_ms,
            &self.market_marks,
            &self.market_index_prices,
            &self.market_oracle_config,
        ) {
            return Err(NodeError::StaleMarketData { market_id });
        }

        let index_price_ticks = self
            .index_price_ticks(market_id)
            .ok_or(NodeError::StaleMarketData { market_id })?;
        let funding_rate_ppm = self
            .indicative_funding_rate_ppm(market_id)
            .ok_or(NodeError::StaleMarketData { market_id })?;
        let next_funding_index = self
            .market_funding_index(market_id)
            .saturating_add(funding_index_delta(index_price_ticks, funding_rate_ppm));
        self.update_market_funding_index(market_id, next_funding_index, timestamp_ms);
        Ok(next_funding_index)
    }

    pub fn update_market_risk_parameters(
        &mut self,
        market_id: u64,
        parameters: MarketRiskParameters,
        timestamp_ms: u64,
    ) {
        self.set_market_risk_parameters(market_id, parameters);
        self.record_risk_state_updates_for_market(None, timestamp_ms, market_id);
        self.reconcile_frozen_accounts_for_market(
            market_id,
            timestamp_ms,
            RiskTransitionSource::RiskParameterUpdate,
        );
    }

    pub fn update_market_fee_parameters(
        &mut self,
        market_id: u64,
        parameters: MarketFeeParameters,
    ) {
        assert!(
            Self::market_fee_parameters_are_valid(parameters),
            "invalid_market_fee_parameters",
        );
        self.market_fees.insert(market_id, parameters);
    }

    pub fn isolated_collateral(&self, market_id: u64, account_id: u64) -> i128 {
        self.isolated_collateral
            .get(&(market_id, account_id))
            .copied()
            .unwrap_or(0)
    }

    pub fn insurance_fund_balance(&self, market_id: u64) -> u128 {
        self.insurance_funds.get(&market_id).copied().unwrap_or(0)
    }

    /// Open interest in lots for a perpetual market: sum of all positive
    /// `net_lots` across accounts. Equivalent to `sum(|net_lots|) / 2`
    /// since perps positions are zero-sum, but the positive-sum form
    /// avoids the divide. RFC 046.
    pub fn market_open_interest_lots(&self, market_id: u64) -> u128 {
        let mut total: u128 = 0;
        for ((entry_market, _account), ledger) in &self.positions {
            if *entry_market != market_id {
                continue;
            }
            let net = ledger.net_lots();
            if net > 0 {
                total = total.saturating_add(net as u128);
            }
        }
        total
    }

    /// Snapshot of OI per market. Used by the block-apply path to emit
    /// the coalesced `MarketOpenInterestUpdated` runtime event when the
    /// gauge changes across a block (RFC 046). Only markets with at
    /// least one open position appear in the map; absent markets are
    /// implicitly zero.
    pub(super) fn market_open_interest_snapshot(&self) -> BTreeMap<u64, u128> {
        let mut snapshot: BTreeMap<u64, u128> = BTreeMap::new();
        for ((entry_market, _account), ledger) in &self.positions {
            let net = ledger.net_lots();
            if net <= 0 {
                continue;
            }
            let entry = snapshot.entry(*entry_market).or_insert(0);
            *entry = entry.saturating_add(net as u128);
        }
        snapshot
    }

    /// Compare a pre-block OI snapshot to the engine's current state and
    /// emit `MarketOpenInterestUpdated` events for every market whose
    /// gauge changed. Events sequence in ascending `market_id` order to
    /// keep replay deterministic across producers and followers.
    pub(super) fn record_market_open_interest_diff(
        &mut self,
        pre: &BTreeMap<u64, u128>,
        block_height: u64,
        timestamp_ms: u64,
    ) {
        let post = self.market_open_interest_snapshot();
        let mut market_ids: BTreeSet<u64> = BTreeSet::new();
        market_ids.extend(pre.keys().copied());
        market_ids.extend(post.keys().copied());
        for market_id in market_ids {
            let pre_value = pre.get(&market_id).copied().unwrap_or(0);
            let post_value = post.get(&market_id).copied().unwrap_or(0);
            if pre_value == post_value {
                continue;
            }
            self.record_runtime_event(
                Some(block_height),
                timestamp_ms,
                RuntimeEventKind::MarketOpenInterestUpdated {
                    market_id,
                    open_interest_lots: post_value,
                },
            );
        }
    }

    pub fn market_bad_debt_balance(&self, market_id: u64) -> u128 {
        self.market_bad_debt_balances
            .get(&market_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn set_isolated_collateral(&mut self, market_id: u64, account_id: u64, collateral: i128) {
        if collateral == 0 {
            self.isolated_collateral.remove(&(market_id, account_id));
        } else {
            self.isolated_collateral
                .insert((market_id, account_id), collateral);
        }
    }

    pub(super) fn apply_collateral_overlays(&mut self, overlays: BTreeMap<(u64, u64), i128>) {
        for ((market_id, account_id), collateral) in overlays {
            self.set_isolated_collateral(market_id, account_id, collateral);
        }
    }

    pub fn update_isolated_collateral(
        &mut self,
        market_id: u64,
        account_id: u64,
        collateral: i128,
        timestamp_ms: u64,
    ) {
        self.set_isolated_collateral(market_id, account_id, collateral);
        let should_emit_reset = self.clear_liquidation_escalation(market_id, account_id)
            || self.account_frozen(account_id);
        self.record_risk_state_update(None, timestamp_ms, market_id, account_id);
        self.reconcile_bad_debt_after_collateral_update(market_id, account_id, timestamp_ms);
        if should_emit_reset {
            self.record_liquidation_escalation_event(
                market_id,
                account_id,
                timestamp_ms,
                LiquidationEscalationAction::Reset,
                0,
            );
        }
    }

    pub fn margin_snapshot(&self, market_id: u64, account_id: u64) -> MarginSnapshot {
        let ledger = self.position(market_id, account_id);
        self.margin_snapshot_for_ledger(market_id, account_id, ledger)
    }

    pub(super) fn margin_snapshot_for_ledger(
        &self,
        market_id: u64,
        account_id: u64,
        ledger: PositionLedger,
    ) -> MarginSnapshot {
        let collateral = self.isolated_collateral(market_id, account_id);
        let mark_price_ticks = self.mark_price_ticks(market_id);
        let open_abs_lots = ledger.open_abs_lots();
        let raw_mark_notional = match mark_price_ticks {
            Some(mark_price) => u128::from(mark_price).saturating_mul(u128::from(open_abs_lots)),
            None => ledger.open_notional,
        };
        let (mark_notional, realized_pnl, unrealized_pnl) =
            if let Some(market) = self.markets.get(&market_id) {
                (
                    scaled_raw_tick_lot_notional_usdc(market, raw_mark_notional),
                    scaled_signed_raw_tick_lot_pnl_usdc(market, ledger.realized_pnl),
                    scaled_ledger_unrealized_pnl_usdc(market, &ledger, mark_price_ticks),
                )
            } else {
                (
                    raw_mark_notional,
                    ledger.realized_pnl,
                    unrealized_pnl(&ledger, mark_price_ticks),
                )
            };
        let maintenance_margin_bps = self
            .market_risk_parameters(market_id)
            .maintenance_margin_bps;
        let maintenance_requirement =
            proportional_bps_round_up(mark_notional, maintenance_margin_bps);
        let equity = collateral
            .saturating_add(realized_pnl)
            .saturating_add(unrealized_pnl);
        let margin_surplus =
            equity.saturating_sub(u128_to_i128_saturating(maintenance_requirement));

        MarginSnapshot {
            collateral,
            maintenance_margin_bps,
            maintenance_requirement,
            mark_notional,
            realized_pnl,
            unrealized_pnl,
            equity,
            margin_surplus,
            under_maintenance: equity < u128_to_i128_saturating(maintenance_requirement),
        }
    }

    pub fn positions(
        &self,
        account_id: u64,
        include_flat: bool,
        limit: usize,
    ) -> Vec<(u64, PositionLedger)> {
        let mut positions: Vec<(u64, PositionLedger)> = self
            .positions
            .iter()
            .filter(|((_, entry_account_id), ledger)| {
                *entry_account_id == account_id && (include_flat || ledger.net_lots() != 0)
            })
            .map(|((market_id, _), ledger)| (*market_id, *ledger))
            .collect();
        positions.sort_by_key(|(market_id, _)| *market_id);
        positions.truncate(limit);
        positions
    }

    pub fn risk_positions(
        &self,
        account_id: u64,
        include_flat: bool,
        limit: usize,
    ) -> Vec<(u64, PositionLedger)> {
        let mut markets = BTreeSet::new();
        for ((market_id, entry_account_id), ledger) in &self.positions {
            if *entry_account_id == account_id && (include_flat || ledger.net_lots() != 0) {
                markets.insert(*market_id);
            }
        }
        for ((market_id, entry_account_id), collateral) in &self.isolated_collateral {
            if *entry_account_id == account_id && *collateral != 0 {
                markets.insert(*market_id);
            }
        }
        for ((market_id, entry_account_id), amount) in &self.account_market_bad_debt {
            if *entry_account_id == account_id && *amount > 0 {
                markets.insert(*market_id);
            }
        }
        for (market_id, entry_account_id) in &self.under_maintenance_accounts {
            if *entry_account_id == account_id {
                markets.insert(*market_id);
            }
        }
        for (market_id, entry_account_id) in self.liquidation_escalation.keys() {
            if *entry_account_id == account_id {
                markets.insert(*market_id);
            }
        }

        markets
            .into_iter()
            .take(limit)
            .map(|market_id| (market_id, self.position(market_id, account_id)))
            .collect()
    }

    pub fn all_positions(
        &self,
        market_id: Option<u64>,
        include_flat: bool,
        limit: usize,
    ) -> Vec<(u64, u64, PositionLedger)> {
        let mut positions: Vec<(u64, u64, PositionLedger)> = self
            .positions
            .iter()
            .filter(|((entry_market_id, _), ledger)| {
                market_id.is_none_or(|expected| *entry_market_id == expected)
                    && (include_flat || ledger.net_lots() != 0)
            })
            .map(|((entry_market_id, account_id), ledger)| (*entry_market_id, *account_id, *ledger))
            .collect();
        positions.sort_by_key(|(entry_market_id, account_id, _)| (*entry_market_id, *account_id));
        positions.truncate(limit);
        positions
    }
}
