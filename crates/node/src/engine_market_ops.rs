use super::*;

impl SingleNodeEngine {
    pub fn submit_order(
        &mut self,
        order: OrderRequest,
        timestamp_ms: u64,
    ) -> Result<MatchOutcome, NodeError> {
        self.ensure_account_not_frozen(order.account_id)?;
        self.ensure_market_accepts_new_orders(order.market_id)?;
        self.ensure_market_price_inputs_ready(order.market_id, timestamp_ms)?;
        ensure_order_preserves_initial_margin(
            &self.order_books,
            &self.isolated_collateral,
            &self.markets,
            &self.market_marks,
            &self.positions,
            &order,
            None,
            timestamp_ms,
        )?;
        self.submit_order_after_admission(order, timestamp_ms)
    }

    pub fn preview_order_initial_margin(
        &self,
        order: OrderRequest,
        replace_client_order_id: Option<u64>,
        timestamp_ms: u64,
    ) -> Result<OrderInitialMarginPreview, NodeError> {
        validate_order_request(&order, timestamp_ms).map_err(NodeError::Order)?;
        self.ensure_account_not_frozen(order.account_id)?;
        self.ensure_market_accepts_new_orders(order.market_id)?;
        self.ensure_market_price_inputs_ready(order.market_id, timestamp_ms)?;
        order_initial_margin_preview(
            &self.order_books,
            &self.isolated_collateral,
            &self.markets,
            &self.market_marks,
            &self.positions,
            &order,
            replace_client_order_id,
            timestamp_ms,
        )?
        .ok_or(NodeError::UnknownMarket {
            market_id: order.market_id,
        })
    }

    pub(super) fn submit_order_after_admission(
        &mut self,
        order: OrderRequest,
        timestamp_ms: u64,
    ) -> Result<MatchOutcome, NodeError> {
        let outcome = match self
            .order_books
            .entry(order.market_id)
            .or_insert_with(|| SingleMarketBook::new(order.market_id))
            .submit(order, timestamp_ms)
        {
            Ok(outcome) => outcome,
            Err(reason) => {
                self.sweep_resting_order_mandates_at(timestamp_ms);
                return Err(NodeError::Order(reason));
            }
        };
        let mut events = vec![RuntimeEventKind::OrderAccepted {
            market_id: order.market_id,
            account_id: order.account_id,
            client_order_id: order.client_order_id,
            status: order_lifecycle_status(outcome.status),
            filled_lots: outcome.filled_lots,
            remaining_lots: outcome.remaining_lots,
        }];
        events.extend(order_fill_events(
            order.market_id,
            &outcome.fills,
            None,
            &self.resting_order_mandates,
        ));
        update_resting_order_mandates_from_outcome(
            &mut self.resting_order_mandates,
            &self.order_books,
            order.market_id,
            &order,
            &outcome,
            None,
            timestamp_ms,
        );
        self.sweep_resting_order_mandates_at(timestamp_ms);
        for event in events {
            self.record_runtime_event(None, timestamp_ms, event);
        }
        Ok(outcome)
    }

    pub fn cancel_order(
        &mut self,
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        timestamp_ms: u64,
    ) -> Option<RestingOrder> {
        let cancelled = self
            .order_books
            .get_mut(&market_id)
            .and_then(|book| book.cancel(account_id, client_order_id, timestamp_ms));
        self.sweep_resting_order_mandates_at(timestamp_ms);
        if let Some(order) = &cancelled {
            self.resting_order_mandates.remove(&resting_order_key(
                market_id,
                account_id,
                client_order_id,
            ));
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::OrderCancelled {
                    market_id,
                    account_id,
                    client_order_id,
                    remaining_lots: order.remaining_lots,
                },
            );
        }
        cancelled
    }

    pub fn replace_order(
        &mut self,
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        replacement: OrderRequest,
        timestamp_ms: u64,
    ) -> Result<MatchOutcome, NodeError> {
        self.ensure_account_not_frozen(account_id)?;
        self.ensure_market_accepts_new_orders(market_id)?;
        self.ensure_market_price_inputs_ready(market_id, timestamp_ms)?;
        ensure_order_preserves_initial_margin(
            &self.order_books,
            &self.isolated_collateral,
            &self.markets,
            &self.market_marks,
            &self.positions,
            &replacement,
            Some(client_order_id),
            timestamp_ms,
        )?;
        let outcome = self
            .order_books
            .entry(market_id)
            .or_insert_with(|| SingleMarketBook::new(market_id))
            .replace(account_id, client_order_id, replacement, timestamp_ms)
            .map_err(NodeError::Order)?;
        self.resting_order_mandates.remove(&resting_order_key(
            market_id,
            account_id,
            client_order_id,
        ));
        let mut events = vec![RuntimeEventKind::OrderReplaced {
            market_id,
            account_id,
            target_client_order_id: client_order_id,
            replacement_client_order_id: replacement.client_order_id,
            status: order_lifecycle_status(outcome.status),
            filled_lots: outcome.filled_lots,
            remaining_lots: outcome.remaining_lots,
        }];
        events.extend(order_fill_events(
            market_id,
            &outcome.fills,
            None,
            &self.resting_order_mandates,
        ));
        update_resting_order_mandates_from_outcome(
            &mut self.resting_order_mandates,
            &self.order_books,
            market_id,
            &replacement,
            &outcome,
            None,
            timestamp_ms,
        );
        self.sweep_resting_order_mandates_at(timestamp_ms);
        for event in events {
            self.record_runtime_event(None, timestamp_ms, event);
        }
        Ok(outcome)
    }

    pub fn liquidate_position(
        &mut self,
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
        client_order_id: u64,
        timestamp_ms: u64,
    ) -> Result<MatchOutcome, NodeError> {
        let ledger = self.position(market_id, account_id);
        let open_lots = ledger.open_abs_lots();
        if open_lots == 0 {
            return Err(NodeError::PositionFlat {
                market_id,
                account_id,
            });
        }
        if !self
            .margin_snapshot(market_id, account_id)
            .under_maintenance
        {
            return Err(NodeError::NotUnderMaintenance {
                market_id,
                account_id,
            });
        }
        if !self.liquidation_price_inputs_ready(market_id) {
            return Err(NodeError::StaleMarketData { market_id });
        }

        let mut fills = Vec::new();
        let mut filled_lots = 0_u64;
        let mut remaining_lots = 0_u64;
        let max_steps = open_lots;

        for step in 0..max_steps {
            let step_ledger = self.position(market_id, account_id);
            let step_open_lots = step_ledger.open_abs_lots();
            if step_open_lots == 0 {
                break;
            }
            if !self
                .margin_snapshot(market_id, account_id)
                .under_maintenance
            {
                break;
            }

            let requested_lots = self
                .suggested_liquidation_size(market_id, account_id)
                .map(|suggestion| suggestion.lots)
                .unwrap_or(step_open_lots)
                .min(step_open_lots);
            if requested_lots == 0 {
                break;
            }

            let outcome = self.liquidate_position_with_size(
                market_id,
                account_id,
                liquidator_account_id,
                client_order_id.saturating_add(step),
                Some(requested_lots),
                timestamp_ms,
            )?;
            fills.extend(outcome.fills);
            filled_lots = filled_lots.saturating_add(outcome.filled_lots);
            remaining_lots = remaining_lots.saturating_add(outcome.remaining_lots);
            if outcome.filled_lots == 0 {
                break;
            }
        }

        let status = match (filled_lots, remaining_lots) {
            (0, _) => MatchStatus::Cancelled,
            (_, 0) => MatchStatus::Filled,
            _ => MatchStatus::PartiallyFilledCancelled,
        };
        Ok(MatchOutcome {
            status,
            fills,
            filled_lots,
            remaining_lots,
            posted_order_id: None,
        })
    }

    pub fn liquidate_position_with_size(
        &mut self,
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
        client_order_id: u64,
        requested_lots: Option<u64>,
        timestamp_ms: u64,
    ) -> Result<MatchOutcome, NodeError> {
        let ledger = self.position(market_id, account_id);
        let open_lots = ledger.open_abs_lots();
        if open_lots == 0 {
            return Err(NodeError::PositionFlat {
                market_id,
                account_id,
            });
        }
        if !self
            .margin_snapshot(market_id, account_id)
            .under_maintenance
        {
            return Err(NodeError::NotUnderMaintenance {
                market_id,
                account_id,
            });
        }

        if !self.liquidation_price_inputs_ready(market_id) {
            return Err(NodeError::StaleMarketData { market_id });
        }

        let quantity_lots = requested_lots
            .or_else(|| {
                self.suggested_liquidation_size(market_id, account_id)
                    .map(|suggestion| suggestion.lots)
            })
            .unwrap_or(open_lots)
            .min(open_lots);
        if quantity_lots == 0 {
            return Err(NodeError::Order(OrderRejectReason::ZeroQuantity));
        }

        let side = if ledger.net_lots() > 0 {
            MatchingOrderSide::Ask
        } else {
            MatchingOrderSide::Bid
        };
        let order = OrderRequest {
            market_id,
            account_id,
            client_order_id,
            side,
            quantity_lots,
            kind: kombat_chain_matching::OrderKind::Market,
            time_in_force: kombat_chain_matching::TimeInForce::Ioc,
            post_only: false,
        };
        self.suppress_fill_bankruptcy_reconcile = true;
        let outcome = self.submit_order_after_admission(order, timestamp_ms);
        self.suppress_fill_bankruptcy_reconcile = false;
        let outcome = outcome?;
        let executed_notional = match_fill_notional(&outcome.fills);
        let risk = self.market_risk_parameters(market_id);
        let penalty = proportional_bps_round_up(executed_notional, risk.liquidation_penalty_bps);
        let insurance_penalty_credit =
            proportional_bps_floor(penalty, LIQUIDATION_INSURANCE_SPLIT_BPS);
        let liquidator_reward_budget =
            proportional_bps_floor(penalty, LIQUIDATION_LIQUIDATOR_SPLIT_BPS);
        let mut distributed_liquidator_reward = 0_u128;
        if penalty > 0 {
            let updated_collateral = self
                .isolated_collateral(market_id, account_id)
                .saturating_sub(u128_to_i128_saturating(penalty));
            self.set_isolated_collateral(market_id, account_id, updated_collateral);
            self.record_risk_state_update(None, timestamp_ms, market_id, account_id);
        }
        if liquidator_reward_budget > 0 && executed_notional > 0 {
            let mut rewards_by_account = BTreeMap::<u64, u128>::new();
            for fill in &outcome.fills {
                let fill_notional =
                    u128::from(fill.price_ticks).saturating_mul(u128::from(fill.quantity_lots));
                let reward_share =
                    liquidator_reward_budget.saturating_mul(fill_notional) / executed_notional;
                if reward_share == 0 {
                    continue;
                }
                let entry = rewards_by_account.entry(fill.maker_account_id).or_insert(0);
                *entry = entry.saturating_add(reward_share);
                distributed_liquidator_reward =
                    distributed_liquidator_reward.saturating_add(reward_share);
            }

            for (rewarded_account_id, reward_amount) in rewards_by_account {
                let updated_collateral = self
                    .isolated_collateral(market_id, rewarded_account_id)
                    .saturating_add(u128_to_i128_saturating(reward_amount));
                self.set_isolated_collateral(market_id, rewarded_account_id, updated_collateral);
                self.record_risk_state_update(None, timestamp_ms, market_id, rewarded_account_id);
            }
        }
        if insurance_penalty_credit > 0 {
            let balance = {
                let entry = self.insurance_funds.entry(market_id).or_insert(0);
                *entry = entry.saturating_add(insurance_penalty_credit);
                *entry
            };
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::InsuranceFundUpdated {
                    market_id,
                    delta: u128_to_i128_saturating(insurance_penalty_credit),
                    balance,
                    source: InsuranceFundUpdateSource::LiquidationPenalty,
                },
            );
        }
        let treasury_delta = penalty
            .saturating_sub(insurance_penalty_credit)
            .saturating_sub(distributed_liquidator_reward);
        if treasury_delta > 0 {
            let entry = self.treasury_balances.entry(market_id).or_insert(0);
            *entry = entry.saturating_add(treasury_delta);
        }
        let mut insurance_cover = 0_u128;
        let margin_after_liquidation = self.margin_snapshot(market_id, account_id);
        if margin_after_liquidation.equity < 0 {
            // This is the per-market liquidation insurance fund, not the RFC
            // 049 real-capital bond-slash accumulator.
            let required_cover = margin_after_liquidation.equity.saturating_neg() as u128;
            insurance_cover = self.insurance_fund_balance(market_id).min(required_cover);
            if insurance_cover > 0 {
                let balance = {
                    let entry = self.insurance_funds.entry(market_id).or_insert(0);
                    *entry = entry.saturating_sub(insurance_cover);
                    *entry
                };
                let updated_collateral = self
                    .isolated_collateral(market_id, account_id)
                    .saturating_add(u128_to_i128_saturating(insurance_cover));
                self.set_isolated_collateral(market_id, account_id, updated_collateral);
                self.record_runtime_event(
                    None,
                    timestamp_ms,
                    RuntimeEventKind::InsuranceFundUpdated {
                        market_id,
                        delta: u128_to_i128_saturating(insurance_cover).saturating_neg(),
                        balance,
                        source: InsuranceFundUpdateSource::BankruptcyCover,
                    },
                );
                self.record_risk_state_update(None, timestamp_ms, market_id, account_id);
            }
        }
        let residual_bad_debt = self
            .margin_snapshot(market_id, account_id)
            .equity
            .saturating_neg()
            .max(0) as u128;
        if residual_bad_debt > 0 {
            self.clear_liquidation_escalation(market_id, account_id);
            self.freeze_account(
                account_id,
                timestamp_ms,
                RiskTransitionSource::LiquidationSolvency,
            );
            self.sync_account_market_bad_debt(
                market_id,
                account_id,
                residual_bad_debt,
                timestamp_ms,
                true,
                RiskTransitionSource::LiquidationSolvency,
            );
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::SolvencyBreached {
                    market_id,
                    account_id,
                    residual_bad_debt,
                },
            );
        }
        self.apply_insurance_fund_to_market_bad_debt(market_id, timestamp_ms);
        self.maybe_escalate_stalled_liquidation(
            market_id,
            account_id,
            timestamp_ms,
            self.margin_snapshot(market_id, account_id),
        );
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::LiquidationExecuted {
                market_id,
                account_id,
                liquidator_account_id,
                requested_lots: quantity_lots,
                filled_lots: outcome.filled_lots,
                remaining_lots: outcome.remaining_lots,
                executed_notional,
                penalty,
                insurance_cover,
                liquidator_reward: distributed_liquidator_reward,
                insurance_fund_balance: self.insurance_fund_balance(market_id),
            },
        );
        let post_position = self.position(market_id, account_id);
        let post_margin = self.margin_snapshot(market_id, account_id);
        let post_market_bad_debt_balance = self.market_bad_debt_balance(market_id);
        let liquidation_outcome = if residual_bad_debt > 0 {
            LiquidationExecutionOutcome::CreatesBadDebt
        } else if self.account_frozen(account_id) {
            LiquidationExecutionOutcome::FrozeAccount
        } else if post_margin.under_maintenance {
            LiquidationExecutionOutcome::StillUnderMaintenance
        } else {
            LiquidationExecutionOutcome::ClearedMaintenance
        };
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::LiquidationOutcome {
                market_id,
                account_id,
                liquidator_account_id,
                outcome: liquidation_outcome,
                post_net_lots: post_position.net_lots(),
                post_margin_surplus: post_margin.margin_surplus,
                post_under_maintenance: post_margin.under_maintenance,
                post_account_frozen: self.account_frozen(account_id),
                post_market_bad_debt_balance,
            },
        );
        Ok(outcome)
    }

    pub(super) fn liquidation_execution_proof(
        events: &[RuntimeEventRecord],
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
    ) -> Option<(u64, u128)> {
        events.iter().rev().find_map(|event| match &event.kind {
            RuntimeEventKind::LiquidationExecuted {
                market_id: event_market_id,
                account_id: event_account_id,
                liquidator_account_id: event_liquidator_account_id,
                requested_lots,
                liquidator_reward,
                ..
            } if *event_market_id == market_id
                && *event_account_id == account_id
                && *event_liquidator_account_id == liquidator_account_id =>
            {
                Some((*requested_lots, *liquidator_reward))
            }
            _ => None,
        })
    }

    pub(super) fn suggested_liquidation_size_for_positions(
        &self,
        market_id: u64,
        account_id: u64,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
    ) -> Option<LiquidationSizingSuggestion> {
        let ledger = positions
            .get(&(market_id, account_id))
            .copied()
            .unwrap_or_default();
        let open_lots = ledger.open_abs_lots();
        if open_lots == 0 {
            return None;
        }

        let margin = self.margin_snapshot_for_ledger(market_id, account_id, ledger);
        if !margin.under_maintenance || !self.liquidation_price_inputs_ready(market_id) {
            return None;
        }

        let mut preview_template = self.clone();
        preview_template.positions = positions.clone();
        let mut fallback = None;
        for requested_lots in 1..=open_lots {
            let mut preview_engine = preview_template.clone();
            let Ok(_) = preview_engine.liquidate_position_with_size(
                market_id,
                account_id,
                u64::MAX,
                u64::MAX.saturating_sub(requested_lots),
                Some(requested_lots),
                self.current_time_ms(),
            ) else {
                continue;
            };
            let post_margin = preview_engine.margin_snapshot(market_id, account_id);
            let suggestion = LiquidationSizingSuggestion {
                lots: requested_lots,
                clears_maintenance: !post_margin.under_maintenance,
                post_margin_surplus: post_margin.margin_surplus,
                post_under_maintenance: post_margin.under_maintenance,
                post_account_frozen: preview_engine.account_frozen(account_id),
                post_market_bad_debt_balance: preview_engine.market_bad_debt_balance(market_id),
            };
            if !post_margin.under_maintenance {
                return Some(suggestion);
            }
            fallback = Some(suggestion);
        }

        fallback
    }

    pub fn suggested_liquidation_size(
        &self,
        market_id: u64,
        account_id: u64,
    ) -> Option<LiquidationSizingSuggestion> {
        self.suggested_liquidation_size_for_positions(market_id, account_id, &self.positions)
    }

    pub fn run_liquidation_pass(
        &mut self,
        liquidator_account_id: u64,
        first_client_order_id: u64,
        max_positions: usize,
        timestamp_ms: u64,
    ) -> LiquidationPassReport {
        // Operator/testnet tool. Production liquidation draining must be called
        // from the consensus block path so every validator commits identical
        // order-book, collateral, position, and event-root changes.
        let positions = self.all_positions(None, false, usize::MAX);
        let mut report = LiquidationPassReport {
            scanned_positions: positions.len(),
            ..LiquidationPassReport::default()
        };
        if max_positions == 0 {
            return report;
        }

        let candidates = positions
            .into_iter()
            .filter(|(market_id, account_id, ledger)| {
                ledger.open_abs_lots() > 0
                    && self
                        .margin_snapshot(*market_id, *account_id)
                        .under_maintenance
            })
            .map(|(market_id, account_id, _)| (market_id, account_id))
            .collect::<Vec<_>>();

        for (market_id, account_id) in candidates {
            if report.attempted_positions >= max_positions {
                break;
            }
            self.record_risk_state_update(None, timestamp_ms, market_id, account_id);
            if !self.liquidation_price_inputs_ready(market_id) {
                report.skipped_stale_markets = report.skipped_stale_markets.saturating_add(1);
                continue;
            }
            if self.account_frozen(liquidator_account_id) {
                report.skipped_frozen_accounts = report.skipped_frozen_accounts.saturating_add(1);
                continue;
            }

            let client_order_id =
                first_client_order_id.saturating_add(report.attempted_positions as u64);
            let result = self.liquidate_position(
                market_id,
                account_id,
                liquidator_account_id,
                client_order_id,
                timestamp_ms,
            );
            report.attempted_positions = report.attempted_positions.saturating_add(1);
            if result.is_ok() {
                report.succeeded_positions = report.succeeded_positions.saturating_add(1);
            } else {
                report.failed_positions = report.failed_positions.saturating_add(1);
            }
            report.steps.push(LiquidationPassStep {
                market_id,
                account_id,
                client_order_id,
                result,
            });
        }

        report
    }

    pub(super) fn preview_engine_for_consensus_liquidations(
        &self,
        timestamp_ms: u64,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        order_book_overlays: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandate_overlays: &BTreeMap<(u64, u64, u64), MandateId>,
        collateral_overlays: &BTreeMap<(u64, u64), i128>,
        committed_events: &[RuntimeEventKind],
    ) -> Self {
        let mut preview_engine = self.clone();
        preview_engine.positions = positions.clone();
        preview_engine.market_marks = market_marks.clone();
        preview_engine.market_index_prices = market_index_prices.clone();
        for (market_id, order_book) in order_book_overlays {
            preview_engine
                .order_books
                .insert(*market_id, order_book.clone());
        }
        preview_engine.resting_order_mandates = resting_order_mandate_overlays.clone();
        preview_engine.apply_collateral_overlays(collateral_overlays.clone());
        if let Some(head) = preview_engine.chain.last_mut() {
            head.header.timestamp_ms = head.header.timestamp_ms.max(timestamp_ms);
        }
        for event in committed_events {
            match event {
                RuntimeEventKind::CollateralDeposited {
                    market_id,
                    account_id,
                    collateral,
                    ..
                }
                | RuntimeEventKind::CollateralWithdrawn {
                    market_id,
                    account_id,
                    collateral,
                    ..
                } => preview_engine.set_isolated_collateral(*market_id, *account_id, *collateral),
                _ => {}
            }
            preview_engine.record_runtime_event(None, timestamp_ms, event.clone());
        }
        preview_engine
    }

    pub(super) fn consensus_liquidation_items_with_prices(
        &self,
        block_height: u64,
        timestamp_ms: u64,
        positions: &BTreeMap<(u64, u64), PositionLedger>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        order_book_overlays: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandate_overlays: &BTreeMap<(u64, u64, u64), MandateId>,
        collateral_overlays: &BTreeMap<(u64, u64), i128>,
        committed_events: &[RuntimeEventKind],
    ) -> Vec<BlockItem> {
        // This gate is against committed engine state. If a future same-block
        // path can freeze the consensus liquidator before liquidation planning,
        // that effect must be threaded through this preview as well.
        if self.account_frozen(CONSENSUS_LIQUIDATOR_ACCOUNT_ID) {
            return Vec::new();
        }

        let selection_engine = self.preview_engine_for_consensus_liquidations(
            timestamp_ms,
            positions,
            market_marks,
            market_index_prices,
            order_book_overlays,
            resting_order_mandate_overlays,
            collateral_overlays,
            committed_events,
        );
        let mut execution_engine = selection_engine.clone();
        let mut items = Vec::new();
        for (&(market_id, account_id), ledger) in positions {
            if items.len() >= MAX_CONSENSUS_LIQUIDATIONS_PER_BLOCK {
                break;
            }
            if account_id_is_delegation_reserved(account_id) {
                continue;
            }
            if ledger.open_abs_lots() == 0
                || !selection_engine
                    .margin_snapshot_for_ledger(market_id, account_id, *ledger)
                    .under_maintenance
                || !market_price_inputs_ready_at(
                    market_id,
                    timestamp_ms,
                    market_marks,
                    market_index_prices,
                    &self.market_oracle_config,
                )
            {
                continue;
            }

            let requested_lots = selection_engine
                .suggested_liquidation_size(market_id, account_id)
                .map(|suggestion| suggestion.lots);
            let item_index = items.len() as u64;
            let block_offset = block_height
                .saturating_mul(MAX_CONSENSUS_LIQUIDATIONS_PER_BLOCK as u64)
                .saturating_add(item_index);
            let client_order_id =
                CONSENSUS_LIQUIDATION_CLIENT_ORDER_ID_BASE.saturating_sub(block_offset);

            let mut execution_preview = execution_engine.clone();
            let event_start = execution_preview.runtime_events.len();
            let Ok(outcome) = execution_preview.liquidate_position_with_size(
                market_id,
                account_id,
                CONSENSUS_LIQUIDATOR_ACCOUNT_ID,
                client_order_id,
                requested_lots,
                timestamp_ms,
            ) else {
                continue;
            };
            if outcome.filled_lots == 0 {
                continue;
            }
            // Scan from the tail so we bind to the final liquidation event for
            // this simulated execution if future refactors emit intermediates.
            let Some((_, keeper_reward_micro_usdc)) = Self::liquidation_execution_proof(
                &execution_preview.runtime_events[event_start..],
                market_id,
                account_id,
                CONSENSUS_LIQUIDATOR_ACCOUNT_ID,
            ) else {
                continue;
            };
            // Zero-reward liquidation items remain valid block items; they just
            // omit the keeper payout composition step.
            if keeper_reward_micro_usdc > 0
                && execution_preview
                    .pay_real_capital_keeper_reward_escrow_v1_at(
                        CONSENSUS_LIQUIDATOR_ACCOUNT_ID,
                        keeper_reward_micro_usdc,
                        timestamp_ms,
                        None,
                    )
                    .is_err()
            {
                continue;
            }

            items.push(BlockItem::Liquidation(BlockLiquidation {
                market_id,
                account_id,
                liquidator_account_id: CONSENSUS_LIQUIDATOR_ACCOUNT_ID,
                client_order_id,
                requested_lots,
                filled_lots: outcome.filled_lots,
                remaining_lots: outcome.remaining_lots,
                keeper_reward_micro_usdc,
            }));
            execution_engine = execution_preview;
        }

        items
    }

    pub(super) fn validate_consensus_liquidation_items(
        &self,
        body: &BlockBody,
        block_height: u64,
        timestamp_ms: u64,
        positions_after_funding: &BTreeMap<(u64, u64), PositionLedger>,
        market_marks: &BTreeMap<u64, OraclePrice>,
        market_index_prices: &BTreeMap<u64, OraclePrice>,
        order_book_overlays: &BTreeMap<u64, SingleMarketBook>,
        resting_order_mandate_overlays: &BTreeMap<(u64, u64, u64), MandateId>,
        collateral_overlays: &BTreeMap<(u64, u64), i128>,
        committed_events: &[RuntimeEventKind],
    ) -> Result<(), NodeError> {
        // Liquidation inclusion is liveness, so blocks may supply a strict
        // prefix of the deterministic liquidation replay set after earlier
        // same-block native effects. Any supplied item must still match
        // that prefix exactly, including the positive-fill execution proof
        // fields and the deterministic keeper-reward amount.
        let supplied = body
            .items
            .iter()
            .filter_map(|item| match item {
                BlockItem::Liquidation(liquidation) => {
                    Some(BlockItem::Liquidation(liquidation.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = self.consensus_liquidation_items_with_prices(
            block_height,
            timestamp_ms,
            positions_after_funding,
            market_marks,
            market_index_prices,
            order_book_overlays,
            resting_order_mandate_overlays,
            collateral_overlays,
            committed_events,
        );
        if supplied.len() > expected.len()
            || supplied
                .iter()
                .zip(expected.iter())
                .any(|(supplied, expected)| supplied != expected)
        {
            return Err(NodeError::InvalidBlock(
                BlockValidationError::InvalidBlockBody,
            ));
        }
        Ok(())
    }

    pub(super) fn apply_block_liquidation_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::Liquidation(liquidation) = item else {
                continue;
            };
            // RFC 049: replay validates deterministic selection, positive-fill
            // work proof, and keeper reward amount before liquidation items
            // execute. Keeper payout now transfers from the deterministic RFC
            // 049 insurance wallet escrow into the keeper's policy-USDC
            // balance while leaving the gross accumulator untouched.
            let start = self.runtime_events.len();
            let result = self.liquidate_position_with_size(
                liquidation.market_id,
                liquidation.account_id,
                liquidation.liquidator_account_id,
                liquidation.client_order_id,
                liquidation.requested_lots,
                block.header.timestamp_ms,
            );
            if result.is_err() {
                self.runtime_events.truncate(start);
                self.next_runtime_event_seq = start as u64;
                self.rebuild_block_event_index();
                continue;
            }
            if liquidation.keeper_reward_micro_usdc > 0 {
                self.pay_real_capital_keeper_reward_escrow_v1_at(
                    liquidation.liquidator_account_id,
                    liquidation.keeper_reward_micro_usdc,
                    block.header.timestamp_ms,
                    None,
                )
                .expect("validated consensus liquidation keeper reward should settle");
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
    }

    pub(super) fn apply_block_funding_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::Funding(funding) = item else {
                continue;
            };
            if self.next_funding_index_at(funding.market_id, block.header.timestamp_ms)
                != Some(funding.funding_index)
            {
                if std::env::var_os("KOMBAT_DEBUG_FUNDING_SKIP").is_some() {
                    eprintln!(
                        "funding_skip block_height={} market_id={} proposed_index={} timestamp_ms={}",
                        block.header.height,
                        funding.market_id,
                        funding.funding_index,
                        block.header.timestamp_ms
                    );
                }
                continue;
            }
            let start = self.runtime_events.len();
            self.update_market_funding_index(
                funding.market_id,
                funding.funding_index,
                block.header.timestamp_ms,
            );
            let mut committed_seqs = Vec::new();
            for event in &mut self.runtime_events[start..] {
                event.block_height = Some(block.header.height);
                committed_seqs.push(event.seq);
            }
            for seq in committed_seqs {
                self.note_committed_event_seq(block.header.height, seq);
            }
        }
    }

    pub(super) fn apply_block_faucet_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::Faucet(faucet) = item else {
                continue;
            };
            let policy_target = U256::from_be_bytes(faucet.policy_target_micro_usdc);
            let evm_target = U256::from_be_bytes(faucet.evm_target_micro_usdc);
            let policy_key = policy_usdc_balance_state_key(faucet.address);
            let evm_key = evm_usdc_balance_state_key(faucet.address);
            let total_supply_key = evm_usdc_total_supply_state_key();
            let policy_current = self
                .get_committed_state(&policy_key)
                .map(|bytes| U256::from_be_slice(&bytes))
                .unwrap_or(U256::ZERO);
            let evm_current = self
                .get_committed_state(&evm_key)
                .map(|bytes| U256::from_be_slice(&bytes))
                .unwrap_or(U256::ZERO);
            let total_supply_current = self
                .get_committed_state(&total_supply_key)
                .map(|bytes| U256::from_be_slice(&bytes))
                .unwrap_or(U256::ZERO);

            let policy_next = policy_current.max(policy_target);
            let evm_next = evm_current.max(evm_target);
            if policy_next == policy_current && evm_next == evm_current {
                continue;
            }
            let evm_delta = evm_next.saturating_sub(evm_current);
            let Some(total_supply_next) = total_supply_current.checked_add(evm_delta) else {
                continue;
            };

            let mut sets = BTreeMap::new();
            let mut deletes = BTreeSet::new();
            if policy_next != policy_current {
                if policy_next.is_zero() {
                    deletes.insert(policy_key);
                } else {
                    sets.insert(policy_key, policy_next.to_be_bytes::<32>().to_vec());
                }
            }
            if evm_next != evm_current {
                if evm_next.is_zero() {
                    deletes.insert(evm_key);
                } else {
                    sets.insert(evm_key, evm_next.to_be_bytes::<32>().to_vec());
                }
                if total_supply_next.is_zero() {
                    deletes.insert(total_supply_key);
                } else {
                    sets.insert(
                        total_supply_key,
                        total_supply_next.to_be_bytes::<32>().to_vec(),
                    );
                }
            }
            if self.apply_committed_state_updates(sets, deletes).is_err() {
                continue;
            }
        }
    }

    pub fn book_levels(
        &self,
        market_id: u64,
        depth: usize,
        timestamp_ms: u64,
    ) -> (Vec<BookLevel>, Vec<BookLevel>) {
        match self.order_books.get(&market_id) {
            Some(book) => (
                book.bid_levels(timestamp_ms, depth),
                book.ask_levels(timestamp_ms, depth),
            ),
            None => (Vec::new(), Vec::new()),
        }
    }

    pub fn resting_order(
        &self,
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        timestamp_ms: u64,
    ) -> Option<RestingOrder> {
        self.order_books
            .get(&market_id)
            .and_then(|book| book.resting_order(account_id, client_order_id, timestamp_ms))
            .cloned()
    }

    pub fn account_orders(
        &self,
        market_id: u64,
        account_id: u64,
        timestamp_ms: u64,
        limit: usize,
    ) -> Vec<RestingOrder> {
        self.order_books
            .get(&market_id)
            .map(|book| book.account_orders(account_id, timestamp_ms, limit))
            .unwrap_or_default()
    }

    pub fn position(&self, market_id: u64, account_id: u64) -> PositionLedger {
        self.positions
            .get(&(market_id, account_id))
            .copied()
            .unwrap_or_default()
    }

    pub fn current_time_ms(&self) -> u64 {
        self.runtime_events
            .last()
            .map(|event| event.timestamp_ms)
            .unwrap_or(self.head().header.timestamp_ms)
            .max(self.head().header.timestamp_ms)
    }

    pub fn mark_price(&self, market_id: u64) -> Option<OraclePrice> {
        self.market_marks.get(&market_id).copied()
    }

    pub fn mark_price_ticks(&self, market_id: u64) -> Option<u64> {
        self.mark_price(market_id).map(|price| price.price_ticks)
    }

    pub fn mark_price_status(&self, market_id: u64) -> Option<PriceFeedStatus> {
        price_status(
            self.current_time_ms(),
            self.mark_price(market_id),
            self.market_oracle_config(market_id),
        )
    }

    pub fn index_price(&self, market_id: u64) -> Option<OraclePrice> {
        self.market_index_prices.get(&market_id).copied()
    }

    pub fn index_price_ticks(&self, market_id: u64) -> Option<u64> {
        self.index_price(market_id).map(|price| price.price_ticks)
    }

    pub fn index_price_status(&self, market_id: u64) -> Option<PriceFeedStatus> {
        price_status(
            self.current_time_ms(),
            self.index_price(market_id),
            self.market_oracle_config(market_id),
        )
    }

    pub fn premium_rate_ppm(&self, market_id: u64) -> Option<i64> {
        premium_rate_ppm(
            self.mark_price_ticks(market_id)?,
            self.index_price_ticks(market_id)?,
        )
    }

    pub fn account_frozen(&self, account_id: u64) -> bool {
        self.frozen_accounts.contains(&account_id)
    }

    pub fn liquidation_stall_count(&self, market_id: u64, account_id: u64) -> u8 {
        self.liquidation_escalation
            .get(&(market_id, account_id))
            .map(|state| state.consecutive_non_improving_still_under)
            .unwrap_or(0)
    }

    pub fn liquidation_freeze_threshold(&self) -> u8 {
        LIQUIDATION_STALL_FREEZE_THRESHOLD
    }

    pub fn liquidation_freeze_imminent(&self, market_id: u64, account_id: u64) -> bool {
        !self.account_frozen(account_id)
            && self
                .margin_snapshot(market_id, account_id)
                .under_maintenance
            && self
                .liquidation_stall_count(market_id, account_id)
                .saturating_add(1)
                >= self.liquidation_freeze_threshold()
    }

    pub(super) fn record_liquidation_escalation_event(
        &mut self,
        market_id: u64,
        account_id: u64,
        timestamp_ms: u64,
        action: LiquidationEscalationAction,
        stall_count: u8,
    ) {
        let margin = self.margin_snapshot(market_id, account_id);
        let account_frozen = self.account_frozen(account_id);
        let freeze_threshold = self.liquidation_freeze_threshold();
        let freeze_imminent = !account_frozen
            && margin.under_maintenance
            && stall_count.saturating_add(1) >= freeze_threshold;
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::LiquidationEscalationUpdated {
                market_id,
                account_id,
                action,
                stall_count,
                freeze_threshold,
                freeze_imminent,
                post_margin_surplus: margin.margin_surplus,
                post_under_maintenance: margin.under_maintenance,
                post_account_frozen: account_frozen,
            },
        );
    }

    pub(super) fn freeze_account(
        &mut self,
        account_id: u64,
        timestamp_ms: u64,
        reason: RiskTransitionSource,
    ) -> bool {
        let was_newly_frozen = self.frozen_accounts.insert(account_id);
        if was_newly_frozen {
            if let Some(account) = self.account_store.get(account_id) {
                if let Some(event) = account_update_runtime_event_for_account(
                    account,
                    AccountUpdateAction::Freeze,
                    Some(reason),
                ) {
                    self.record_runtime_event(None, timestamp_ms, event);
                }
            }
        }
        was_newly_frozen
    }

    pub(super) fn clear_liquidation_escalation(&mut self, market_id: u64, account_id: u64) -> bool {
        self.liquidation_escalation
            .remove(&(market_id, account_id))
            .is_some()
    }

    pub(super) fn reset_liquidation_escalation(
        &mut self,
        market_id: u64,
        account_id: u64,
        timestamp_ms: u64,
    ) {
        if self.clear_liquidation_escalation(market_id, account_id) {
            self.record_liquidation_escalation_event(
                market_id,
                account_id,
                timestamp_ms,
                LiquidationEscalationAction::Reset,
                0,
            );
        }
    }

    pub(super) fn maybe_unfreeze_account(
        &mut self,
        account_id: u64,
        timestamp_ms: u64,
        reason: RiskTransitionSource,
    ) {
        let has_remaining_bad_debt = self
            .account_market_bad_debt
            .iter()
            .any(|((_, entry_account_id), amount)| *entry_account_id == account_id && *amount > 0);
        let has_remaining_under_maintenance = self
            .under_maintenance_accounts
            .iter()
            .any(|(_, entry_account_id)| *entry_account_id == account_id);
        if !has_remaining_bad_debt && !has_remaining_under_maintenance {
            let was_frozen = self.frozen_accounts.remove(&account_id);
            if was_frozen {
                if let Some(account) = self.account_store.get(account_id) {
                    if let Some(event) = account_update_runtime_event_for_account(
                        account,
                        AccountUpdateAction::Unfreeze,
                        Some(reason),
                    ) {
                        self.record_runtime_event(None, timestamp_ms, event);
                    }
                }
            }
        }
    }

    pub(super) fn maybe_escalate_stalled_liquidation(
        &mut self,
        market_id: u64,
        account_id: u64,
        timestamp_ms: u64,
        margin: MarginSnapshot,
    ) {
        if self.account_frozen(account_id) || !margin.under_maintenance {
            self.reset_liquidation_escalation(market_id, account_id, timestamp_ms);
            return;
        }

        let key = (market_id, account_id);
        let next_state = match self.liquidation_escalation.get(&key).copied() {
            Some(previous) if margin.margin_surplus <= previous.last_margin_surplus => {
                LiquidationEscalationState {
                    consecutive_non_improving_still_under: previous
                        .consecutive_non_improving_still_under
                        .saturating_add(1),
                    last_margin_surplus: margin.margin_surplus,
                }
            }
            _ => LiquidationEscalationState {
                consecutive_non_improving_still_under: 1,
                last_margin_surplus: margin.margin_surplus,
            },
        };

        if next_state.consecutive_non_improving_still_under >= LIQUIDATION_STALL_FREEZE_THRESHOLD {
            self.clear_liquidation_escalation(market_id, account_id);
            self.freeze_account(
                account_id,
                timestamp_ms,
                RiskTransitionSource::LiquidationStall,
            );
            self.record_liquidation_escalation_event(
                market_id,
                account_id,
                timestamp_ms,
                LiquidationEscalationAction::FrozeAccount,
                next_state.consecutive_non_improving_still_under,
            );
            return;
        }

        self.liquidation_escalation.insert(key, next_state);
        self.record_liquidation_escalation_event(
            market_id,
            account_id,
            timestamp_ms,
            LiquidationEscalationAction::Advanced,
            next_state.consecutive_non_improving_still_under,
        );
    }

    pub(super) fn sync_account_market_bad_debt(
        &mut self,
        market_id: u64,
        account_id: u64,
        current_shortfall: u128,
        timestamp_ms: u64,
        allow_create: bool,
        source: RiskTransitionSource,
    ) -> u128 {
        let recorded = self
            .account_market_bad_debt
            .get(&(market_id, account_id))
            .copied()
            .unwrap_or(0);

        if current_shortfall == recorded || (recorded == 0 && !allow_create) {
            return recorded;
        }

        if current_shortfall > recorded {
            let delta = current_shortfall.saturating_sub(recorded);
            self.account_market_bad_debt
                .insert((market_id, account_id), current_shortfall);
            let bad_debt_balance = {
                let entry = self.market_bad_debt_balances.entry(market_id).or_insert(0);
                *entry = entry.saturating_add(delta);
                *entry
            };
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::BadDebtUpdated {
                    market_id,
                    account_id,
                    delta,
                    balance: bad_debt_balance,
                    source,
                },
            );
            return current_shortfall;
        }

        let recovered = recorded.saturating_sub(current_shortfall);
        if current_shortfall == 0 {
            self.account_market_bad_debt
                .remove(&(market_id, account_id));
        } else {
            self.account_market_bad_debt
                .insert((market_id, account_id), current_shortfall);
        }

        let bad_debt_balance = {
            let entry = self.market_bad_debt_balances.entry(market_id).or_insert(0);
            *entry = entry.saturating_sub(recovered);
            *entry
        };
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::BadDebtRecovered {
                market_id,
                account_id,
                recovered,
                balance: bad_debt_balance,
                source,
            },
        );
        current_shortfall
    }

    pub(super) fn reconcile_bad_debt_after_collateral_update(
        &mut self,
        market_id: u64,
        account_id: u64,
        timestamp_ms: u64,
    ) {
        let current_shortfall = self
            .margin_snapshot(market_id, account_id)
            .equity
            .saturating_neg()
            .max(0) as u128;
        self.sync_account_market_bad_debt(
            market_id,
            account_id,
            current_shortfall,
            timestamp_ms,
            false,
            RiskTransitionSource::CollateralAdjustment,
        );

        self.maybe_unfreeze_account(
            account_id,
            timestamp_ms,
            RiskTransitionSource::CollateralAdjustment,
        );
    }

    pub(super) fn market_bad_debt_account_ids(&self, market_id: u64) -> Vec<u64> {
        self.account_market_bad_debt
            .iter()
            .filter_map(|((entry_market_id, account_id), amount)| {
                (*entry_market_id == market_id && *amount > 0).then_some(*account_id)
            })
            .collect()
    }

    pub(super) fn sync_market_bad_debt_recovery_total(
        &mut self,
        market_id: u64,
        account_ids: &[u64],
        timestamp_ms: u64,
        source: RiskTransitionSource,
    ) -> u128 {
        let mut total = 0_u128;
        for &account_id in account_ids {
            let refreshed_shortfall = self
                .margin_snapshot(market_id, account_id)
                .equity
                .saturating_neg()
                .max(0) as u128;
            total = total.saturating_add(self.sync_account_market_bad_debt(
                market_id,
                account_id,
                refreshed_shortfall,
                timestamp_ms,
                false,
                source,
            ));
        }
        total
    }

    pub(super) fn apply_market_bad_debt_cover_in_account_id_order(
        &mut self,
        market_id: u64,
        account_ids: &[u64],
        mut available_cover: u128,
        timestamp_ms: u64,
        source: RiskTransitionSource,
    ) -> u128 {
        let mut total_cover = 0_u128;
        for &account_id in account_ids {
            if available_cover == 0 {
                break;
            }

            let refreshed_shortfall = self
                .margin_snapshot(market_id, account_id)
                .equity
                .saturating_neg()
                .max(0) as u128;
            let recorded = self.sync_account_market_bad_debt(
                market_id,
                account_id,
                refreshed_shortfall,
                timestamp_ms,
                false,
                source,
            );
            if recorded == 0 {
                continue;
            }

            let cover = recorded.min(available_cover);
            if cover == 0 {
                continue;
            }

            let updated_collateral = self
                .isolated_collateral(market_id, account_id)
                .saturating_add(u128_to_i128_saturating(cover));
            self.set_isolated_collateral(market_id, account_id, updated_collateral);
            self.record_risk_state_update(None, timestamp_ms, market_id, account_id);

            available_cover = available_cover.saturating_sub(cover);
            total_cover = total_cover.saturating_add(cover);

            let current_shortfall = self
                .margin_snapshot(market_id, account_id)
                .equity
                .saturating_neg()
                .max(0) as u128;
            self.sync_account_market_bad_debt(
                market_id,
                account_id,
                current_shortfall,
                timestamp_ms,
                false,
                source,
            );
            self.maybe_unfreeze_account(account_id, timestamp_ms, source);
        }
        total_cover
    }

    pub(super) fn apply_insurance_fund_to_market_bad_debt(
        &mut self,
        market_id: u64,
        timestamp_ms: u64,
    ) {
        if self.market_bad_debt_balance(market_id) == 0 {
            return;
        }
        let account_ids = self.market_bad_debt_account_ids(market_id);
        let total_cover = self.apply_market_bad_debt_cover_in_account_id_order(
            market_id,
            &account_ids,
            self.insurance_fund_balance(market_id),
            timestamp_ms,
            RiskTransitionSource::InsuranceSweep,
        );

        if total_cover > 0 {
            let balance = {
                let entry = self.insurance_funds.entry(market_id).or_insert(0);
                *entry = entry.saturating_sub(total_cover);
                *entry
            };
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::InsuranceFundUpdated {
                    market_id,
                    delta: u128_to_i128_saturating(total_cover).saturating_neg(),
                    balance,
                    source: InsuranceFundUpdateSource::BankruptcyCover,
                },
            );
        }

        let wallet_cover_requirement = self.sync_market_bad_debt_recovery_total(
            market_id,
            &account_ids,
            timestamp_ms,
            RiskTransitionSource::InsuranceSweep,
        );
        let available_cover =
            wallet_cover_requirement.min(self.real_capital_insurance_wallet_micro_usdc());
        if available_cover == 0 {
            return;
        }

        // The EVM escrow debit and market-custody credit must commit before
        // the Rust-side collateral recovery becomes visible.
        self.cover_market_bad_debt_from_real_capital_insurance_wallet_escrow_v1_at(
            market_id,
            available_cover,
            timestamp_ms,
            None,
        )
        .expect("bad-debt cover should settle through market custody");
        let covered_from_wallet = self.apply_market_bad_debt_cover_in_account_id_order(
            market_id,
            &account_ids,
            available_cover,
            timestamp_ms,
            RiskTransitionSource::InsuranceSweep,
        );
        assert_eq!(
            covered_from_wallet, available_cover,
            "real-capital bad-debt cover must credit exactly the committed escrow transfer"
        );
    }

    pub(super) fn reconcile_bad_debt_for_market(
        &mut self,
        market_id: u64,
        timestamp_ms: u64,
        source: RiskTransitionSource,
    ) {
        let mut account_ids = self
            .all_positions(Some(market_id), true, usize::MAX)
            .into_iter()
            .map(|(_, account_id, _)| account_id)
            .collect::<BTreeSet<_>>();
        account_ids.extend(self.account_market_bad_debt.iter().filter_map(
            |((entry_market_id, account_id), amount)| {
                (*entry_market_id == market_id && *amount > 0).then_some(*account_id)
            },
        ));

        for account_id in account_ids {
            let current_shortfall = self
                .margin_snapshot(market_id, account_id)
                .equity
                .saturating_neg()
                .max(0) as u128;
            let recorded = self
                .account_market_bad_debt
                .get(&(market_id, account_id))
                .copied()
                .unwrap_or(0);
            if current_shortfall == recorded {
                continue;
            }

            self.sync_account_market_bad_debt(
                market_id,
                account_id,
                current_shortfall,
                timestamp_ms,
                false,
                source,
            );
            self.maybe_unfreeze_account(account_id, timestamp_ms, source);
            self.record_risk_state_update(None, timestamp_ms, market_id, account_id);
        }
    }

    pub(super) fn market_account_ids(&self, market_id: u64) -> Vec<u64> {
        let mut account_ids = BTreeSet::new();
        for (entry_market_id, account_id) in self.positions.keys() {
            if *entry_market_id == market_id {
                account_ids.insert(*account_id);
            }
        }
        for ((entry_market_id, account_id), amount) in &self.account_market_bad_debt {
            if *entry_market_id == market_id && *amount > 0 {
                account_ids.insert(*account_id);
            }
        }
        for (entry_market_id, account_id) in &self.under_maintenance_accounts {
            if *entry_market_id == market_id {
                account_ids.insert(*account_id);
            }
        }
        account_ids.into_iter().collect()
    }

    pub(super) fn reconcile_frozen_accounts_for_market(
        &mut self,
        market_id: u64,
        timestamp_ms: u64,
        reason: RiskTransitionSource,
    ) {
        for account_id in self.market_account_ids(market_id) {
            self.maybe_unfreeze_account(account_id, timestamp_ms, reason);
        }
    }

    pub fn indicative_funding_rate_ppm(&self, market_id: u64) -> Option<i64> {
        indicative_funding_rate_ppm(
            self.mark_price_ticks(market_id)?,
            self.index_price_ticks(market_id)?,
        )
    }

    pub fn market_oracle_config(&self, market_id: u64) -> OracleConfig {
        self.market_oracle_config
            .get(&market_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn perpetual_market(&self, market_id: u64) -> Option<&PerpetualMarket> {
        self.markets.get(&market_id)
    }

    pub fn perpetual_markets(&self) -> impl Iterator<Item = &PerpetualMarket> {
        self.markets.values()
    }

    pub fn list_perpetual_market(
        &mut self,
        market_id: u64,
        base_symbol: String,
        quote_asset: u32,
        oracle_feed_id: u64,
        config: PerpetualMarketConfig,
        timestamp_ms: u64,
    ) -> Result<PerpetualMarket, NodeError> {
        validate_perpetual_market_config(market_id, &base_symbol, config)?;
        if self.markets.contains_key(&market_id) {
            return Err(NodeError::MarketAlreadyListed { market_id });
        }

        let market = PerpetualMarket {
            market_id,
            kind: MarketKind::Perpetual,
            status: MarketStatus::Active,
            base_symbol,
            quote_asset,
            oracle_feed_id,
            config,
            listed_at_ms: timestamp_ms,
            updated_at_ms: timestamp_ms,
        };
        self.markets.insert(market_id, market.clone());
        self.market_oracle_config.insert(
            market_id,
            OracleConfig {
                max_staleness_ms: config.oracle_max_staleness_ms,
            },
        );
        self.market_risk.insert(
            market_id,
            MarketRiskParameters {
                maintenance_margin_bps: config.maintenance_margin_bps,
                liquidation_penalty_bps: self
                    .market_risk
                    .get(&market_id)
                    .map_or(0, |risk| risk.liquidation_penalty_bps),
                liquidator_fee_bps: self
                    .market_risk
                    .get(&market_id)
                    .map_or(0, |risk| risk.liquidator_fee_bps),
            },
        );
        self.market_funding_index.entry(market_id).or_insert(0);
        self.market_last_funding_ms
            .entry(market_id)
            .or_insert(timestamp_ms);
        self.insurance_funds.entry(market_id).or_insert(0);
        self.order_books
            .entry(market_id)
            .or_insert_with(|| SingleMarketBook::new(market_id));
        self.paused_markets.remove(&market_id);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MarketListed {
                market_id,
                base_symbol: market.base_symbol.clone(),
                quote_asset,
                oracle_feed_id,
                status: market_status_event(market.status),
            },
        );
        Ok(market)
    }

    pub fn set_perpetual_market_status(
        &mut self,
        market_id: u64,
        status: MarketStatus,
        timestamp_ms: u64,
    ) -> Result<PerpetualMarket, NodeError> {
        let market = self
            .markets
            .get_mut(&market_id)
            .ok_or(NodeError::UnknownMarket { market_id })?;
        market.status = status;
        market.updated_at_ms = timestamp_ms;
        match status {
            MarketStatus::Active => {
                self.paused_markets.remove(&market_id);
            }
            MarketStatus::Paused | MarketStatus::ReduceOnly | MarketStatus::Delisted => {
                self.paused_markets.insert(market_id);
            }
        }
        if status == MarketStatus::Delisted {
            self.oracle_feed_submissions
                .retain(|(submission_market_id, _, _), _| *submission_market_id != market_id);
            self.market_last_funding_ms.remove(&market_id);
        }
        let updated = market.clone();
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MarketStatusUpdated {
                market_id,
                status: market_status_event(status),
            },
        );
        Ok(updated)
    }
}
