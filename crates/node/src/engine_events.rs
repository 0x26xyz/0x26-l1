use super::*;

impl SingleNodeEngine {
    pub(crate) fn record_runtime_event(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        kind: RuntimeEventKind,
    ) {
        let seq = self.next_runtime_event_seq;
        let mut position_update = None;
        let mut mark_update = None;
        let mut fee_updates = Vec::new();
        let mut fee_distribution_update = None;
        let mut insurance_sweep_market_id = None;
        let mut pause_market_id = None;
        let mut bad_debt_reconcile_market_id = None;
        let mut risk_updates = Vec::new();
        if let RuntimeEventKind::OrderFilled {
            market_id,
            account_id,
            client_order_id,
            counterparty_account_id,
            counterparty_client_order_id,
            mandate_id,
            role,
            side,
            price_ticks,
            quantity_lots,
            ..
        } = &kind
        {
            let funding_index = self.market_funding_index(*market_id);
            if *role == OrderFillRole::Maker {
                // Fill-level fee accounting anchors on the maker-side event
                // so both sides' fees and the maker rebate settle exactly once
                // against a single fill context before later MM multiplier
                // settlement extends this path.
                let (
                    settled_fee_updates,
                    settled_distribution,
                    settled_sweep_market_id,
                    settled_pause_market_id,
                ) = self.settle_order_fill_fees(
                    *market_id,
                    *account_id,
                    *client_order_id,
                    *counterparty_account_id,
                    *counterparty_client_order_id,
                    *price_ticks,
                    *quantity_lots,
                );
                fee_updates.extend(settled_fee_updates);
                fee_distribution_update = settled_distribution;
                insurance_sweep_market_id = settled_sweep_market_id;
                pause_market_id = settled_pause_market_id;
            }
            self.market_marks.insert(
                *market_id,
                OraclePrice {
                    price_ticks: *price_ticks,
                    timestamp_ms,
                },
            );
            mark_update = Some(RuntimeEventKind::MarkPriceUpdated {
                market_id: *market_id,
                mark_price_ticks: *price_ticks,
            });
            bad_debt_reconcile_market_id = Some(*market_id);
            let (pnl_mandate_id, realized_delta_raw) = {
                let ledger = self.positions.entry((*market_id, *account_id)).or_default();
                let prior_mandate_id = ledger.mandate_id;
                let prior_realized_pnl = ledger.realized_pnl;
                ledger.apply_fill(
                    *side,
                    *price_ticks,
                    *quantity_lots,
                    *mandate_id,
                    funding_index,
                    seq,
                    timestamp_ms,
                );
                (
                    prior_mandate_id.or(*mandate_id),
                    ledger.realized_pnl.saturating_sub(prior_realized_pnl),
                )
            };
            if let (Some(mandate_id), Some(market)) = (pnl_mandate_id, self.markets.get(market_id))
            {
                let scaled_delta = scaled_signed_raw_tick_lot_pnl_usdc(market, realized_delta_raw);
                self.add_mandate_realized_pnl(mandate_id, scaled_delta);
            }
            let current_shortfall = self
                .margin_snapshot(*market_id, *account_id)
                .equity
                .saturating_neg()
                .max(0) as u128;
            if current_shortfall > 0 && !self.suppress_fill_bankruptcy_reconcile {
                self.sync_account_market_bad_debt(
                    *market_id,
                    *account_id,
                    current_shortfall,
                    timestamp_ms,
                    true,
                    RiskTransitionSource::CollateralAdjustment,
                );
            }
            let ledger = self.position(*market_id, *account_id);
            position_update = Some(RuntimeEventKind::PositionUpdated {
                market_id: *market_id,
                account_id: *account_id,
                net_lots: ledger.net_lots(),
                open_notional: ledger.open_notional,
                realized_pnl: ledger.realized_pnl,
                average_entry_price_ticks: ledger.average_entry_price_ticks(),
                buy_lots: ledger.buy_lots,
                sell_lots: ledger.sell_lots,
                buy_notional: ledger.buy_notional,
                sell_notional: ledger.sell_notional,
            });
            risk_updates = self.risk_state_events(*market_id, *account_id);
        }
        if let RuntimeEventKind::CollateralDeposited {
            market_id,
            account_id,
            ..
        }
        | RuntimeEventKind::CollateralWithdrawn {
            market_id,
            account_id,
            ..
        } = &kind
        {
            risk_updates = self.risk_state_events(*market_id, *account_id);
        }
        self.index_runtime_event_for_mandates(timestamp_ms, &kind);
        self.push_runtime_event_record(block_height, timestamp_ms, kind);
        if let Some(kind) = mark_update {
            self.push_runtime_event_record(block_height, timestamp_ms, kind);
        }
        for kind in fee_updates {
            self.push_runtime_event_record(block_height, timestamp_ms, kind);
        }
        if let Some(kind) = fee_distribution_update {
            self.push_runtime_event_record(block_height, timestamp_ms, kind);
        }
        if let Some(market_id) = pause_market_id {
            self.set_market_trading_paused_at(block_height, market_id, true, timestamp_ms);
        }
        if let Some(kind) = position_update {
            self.push_runtime_event_record(block_height, timestamp_ms, kind);
        }
        for kind in risk_updates {
            self.push_runtime_event_record(block_height, timestamp_ms, kind);
        }
        if let Some(market_id) = bad_debt_reconcile_market_id {
            self.reconcile_bad_debt_for_market(
                market_id,
                timestamp_ms,
                RiskTransitionSource::CollateralAdjustment,
            );
            if insurance_sweep_market_id.is_none()
                && self.market_bad_debt_balance(market_id) > 0
                && self.insurance_fund_balance(market_id) > 0
            {
                insurance_sweep_market_id = Some(market_id);
            }
        }
        if let Some(market_id) = insurance_sweep_market_id {
            self.apply_insurance_fund_to_market_bad_debt(market_id, timestamp_ms);
        }
    }

    fn push_runtime_event_record(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        kind: RuntimeEventKind,
    ) {
        let seq = self.next_runtime_event_seq;
        self.runtime_events.push(RuntimeEventRecord {
            seq,
            block_height,
            timestamp_ms,
            kind,
        });
        if let Some(block_height) = block_height {
            self.note_committed_event_seq(block_height, seq);
        }
        self.next_runtime_event_seq = self.next_runtime_event_seq.saturating_add(1);
    }

    fn index_runtime_event_for_mandates(&mut self, timestamp_ms: u64, kind: &RuntimeEventKind) {
        match kind {
            RuntimeEventKind::OrderFilled {
                mandate_id: Some(mandate_id),
                ..
            } => {
                let Some(window_ms) = self
                    .mandates
                    .get(mandate_id)
                    .and_then(mandate_trade_count_window_ms)
                else {
                    return;
                };
                let timestamps = self.mandate_fill_timestamps.entry(*mandate_id).or_default();
                timestamps.push(timestamp_ms);
                prune_timestamps_before(timestamps, timestamp_ms.saturating_sub(window_ms));
            }
            RuntimeEventKind::MandateObjectiveEvaluated {
                mandate_id,
                objective_index,
                score,
                conformance_score,
                pending_objectives,
                ..
            } => {
                self.mandate_objective_event_cache.insert(
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

    pub(crate) fn record_risk_state_update(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        market_id: u64,
        account_id: u64,
    ) {
        for kind in self.risk_state_events(market_id, account_id) {
            self.record_runtime_event(block_height, timestamp_ms, kind);
        }
    }

    pub(crate) fn record_risk_state_updates_for_market(
        &mut self,
        block_height: Option<u64>,
        timestamp_ms: u64,
        market_id: u64,
    ) {
        let account_ids: Vec<u64> = self
            .positions
            .keys()
            .filter_map(|(entry_market_id, account_id)| {
                (*entry_market_id == market_id).then_some(*account_id)
            })
            .collect();

        for account_id in account_ids {
            self.record_risk_state_update(block_height, timestamp_ms, market_id, account_id);
        }
    }

    fn risk_state_event_from_margin(
        &self,
        market_id: u64,
        account_id: u64,
        margin: &MarginSnapshot,
    ) -> RuntimeEventKind {
        RuntimeEventKind::RiskStateUpdated {
            market_id,
            account_id,
            net_lots: self.position(market_id, account_id).net_lots(),
            collateral: margin.collateral,
            equity: margin.equity,
            maintenance_requirement: margin.maintenance_requirement,
            margin_surplus: margin.margin_surplus,
            under_maintenance: margin.under_maintenance,
        }
    }

    fn risk_state_events(&mut self, market_id: u64, account_id: u64) -> Vec<RuntimeEventKind> {
        let margin = self.margin_snapshot(market_id, account_id);
        let mut events = vec![self.risk_state_event_from_margin(market_id, account_id, &margin)];
        let key = (market_id, account_id);
        let was_under_maintenance = self.under_maintenance_accounts.contains(&key);

        if margin.under_maintenance {
            self.under_maintenance_accounts.insert(key);
        } else {
            self.under_maintenance_accounts.remove(&key);
        }

        if was_under_maintenance != margin.under_maintenance {
            events.push(RuntimeEventKind::LiquidationCandidateUpdated {
                market_id,
                account_id,
                net_lots: self.position(market_id, account_id).net_lots(),
                equity: margin.equity,
                maintenance_requirement: margin.maintenance_requirement,
                margin_surplus: margin.margin_surplus,
                under_maintenance: margin.under_maintenance,
            });
        }

        events
    }

    pub fn record_timeout(&mut self, timestamp_ms: u64) {
        self.telemetry
            .on_timeout(&self.pacemaker, self.local_validator_id(), timestamp_ms);
    }

    pub fn record_view_change(&mut self, timestamp_ms: u64) {
        self.telemetry
            .on_view_change(&self.pacemaker, self.local_validator_id(), timestamp_ms);
    }

    pub(crate) fn index_evm_transaction_receipts(
        &mut self,
        block: &Block,
        evm_transactions: &[EvmTransactionExecution],
    ) {
        let block_hash = block_hash(block);
        let mut cumulative_gas_used = 0_u64;
        let mut next_log_index = 0_u64;
        for (transaction_index, transaction) in evm_transactions.iter().enumerate() {
            cumulative_gas_used = cumulative_gas_used.saturating_add(transaction.gas_used);
            let log_index_start = next_log_index;
            next_log_index =
                next_log_index.saturating_add(u64::try_from(transaction.logs.len()).unwrap_or(0));
            self.evm_transaction_receipts.insert(
                transaction.tx_hash,
                EvmTransactionReceiptRecord {
                    tx_hash: transaction.tx_hash,
                    block_hash,
                    block_number: block.header.height,
                    transaction_index: transaction_index as u64,
                    from: transaction.from,
                    to: transaction.to,
                    nonce: transaction.nonce,
                    gas_used: transaction.gas_used,
                    cumulative_gas_used,
                    success: transaction.success,
                    log_index_start,
                    logs: transaction.logs.clone(),
                },
            );
        }
    }
}
