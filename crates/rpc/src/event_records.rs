use crate::rpc_responses::*;
use crate::RpcError;
use kombat_chain_node::{
    AccountEquitySample, EventSeqRange, MandateConformanceSample, MarginSnapshot, PerpetualMarket,
    PositionLedger, SingleNodeEngine,
};
use kombat_chain_streams::{
    AccountUpdateAction, EventFilter, EventStream, LiquidationExecutionOutcome, RuntimeEventKind,
    RuntimeEventRecord, RuntimeEventTopic, StreamQuery,
};
use std::collections::BTreeSet;

pub(crate) fn trade_fill_from_event(event: RuntimeEventRecord) -> Option<TradeFillResponse> {
    match event.kind {
        RuntimeEventKind::OrderFilled {
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
        } => Some(TradeFillResponse {
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
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn event_is_in_committed_seq_range(
    event: &RuntimeEventRecord,
    from_height: u64,
    before_height: u64,
    from_seq: u64,
    before_seq: u64,
) -> bool {
    if event.seq < from_seq || event.seq >= before_seq {
        return false;
    }
    let Some(block_height) = event.block_height else {
        return false;
    };
    block_height >= from_height && block_height < before_height
}

pub(crate) fn pnl_window_events_for_topic(
    events: &[RuntimeEventRecord],
    range: EventSeqRange,
    account_id: u64,
    market_id: Option<u64>,
    topic: RuntimeEventTopic,
    remaining_limit: usize,
) -> Result<Vec<RuntimeEventRecord>, RpcError> {
    let page = EventStream::new(events).query(StreamQuery {
        from_seq: range.from_seq,
        before_seq: Some(range.before_seq),
        limit: remaining_limit.saturating_add(1),
        filter: EventFilter {
            block_height: None,
            account_id: Some(account_id),
            market_id,
            policy_id: None,
            intent_id: None,
            client_order_id: None,
            topic: Some(topic),
        },
        descending: false,
    });

    if page.events.len() > remaining_limit {
        return Err(RpcError::InvalidRequest(
            "pnl window event count exceeds limit",
        ));
    }

    let mut committed = Vec::new();
    for event in page.events {
        if !event_is_in_committed_seq_range(
            &event,
            range.from_height,
            range.before_height,
            range.from_seq,
            range.before_seq,
        ) {
            continue;
        }
        committed.push(event);
    }
    Ok(committed)
}

pub(crate) fn downsample_equity_samples(
    samples: Vec<AccountEquitySample>,
    limit: usize,
) -> Vec<AccountEquitySample> {
    if samples.len() <= limit {
        return samples;
    }
    if limit == 1 {
        return samples.last().copied().into_iter().collect();
    }

    let last = samples.len() - 1;
    let mut out = Vec::with_capacity(limit);
    for i in 0..limit {
        let idx = i * last / (limit - 1);
        out.push(samples[idx]);
    }
    out
}

pub(crate) fn downsample_mandate_conformance_samples(
    samples: Vec<MandateConformanceSample>,
    limit: usize,
) -> Vec<MandateConformanceSample> {
    if samples.len() <= limit {
        return samples;
    }
    if limit == 1 {
        return samples.last().copied().into_iter().collect();
    }

    let last = samples.len() - 1;
    let mut out = Vec::with_capacity(limit);
    for i in 0..limit {
        let idx = i * last / (limit - 1);
        out.push(samples[idx]);
    }
    out
}

pub(crate) fn note_pnl_window_seq(
    seq: u64,
    first_seq: &mut Option<u64>,
    last_seq: &mut Option<u64>,
) {
    if first_seq.is_none_or(|current| seq < current) {
        *first_seq = Some(seq);
    }
    if last_seq.is_none_or(|current| seq > current) {
        *last_seq = Some(seq);
    }
}

pub(crate) fn realized_pnl_at_seq(
    engine: &SingleNodeEngine,
    account_id: u64,
    market_id: u64,
    seq_at: u64,
) -> i128 {
    let mut anchor_seq = None;
    let mut realized_pnl = 0_i128;

    for event in engine.runtime_events().iter().rev() {
        if event.seq >= seq_at || event.block_height.is_none() {
            continue;
        }
        let RuntimeEventKind::PositionUpdated {
            market_id: event_market_id,
            account_id: event_account_id,
            realized_pnl: event_realized_pnl,
            ..
        } = &event.kind
        else {
            continue;
        };
        if *event_market_id == market_id && *event_account_id == account_id {
            anchor_seq = Some(event.seq);
            realized_pnl = *event_realized_pnl;
            break;
        }
    }

    for event in engine.runtime_events() {
        if event.seq >= seq_at {
            break;
        }
        if event.block_height.is_none() || anchor_seq.is_some_and(|seq| event.seq <= seq) {
            continue;
        }
        let RuntimeEventKind::FundingApplied {
            market_id: event_market_id,
            account_id: event_account_id,
            funding_delta,
            ..
        } = &event.kind
        else {
            continue;
        };
        if *event_market_id == market_id && *event_account_id == account_id {
            realized_pnl = realized_pnl.saturating_add(*funding_delta);
        }
    }

    realized_pnl
}

pub(crate) fn scaled_raw_tick_lot_notional_usdc_for_rpc(
    market: Option<&PerpetualMarket>,
    raw_tick_lots: u128,
) -> u128 {
    let Some(market) = market else {
        return raw_tick_lots;
    };
    let scaled = raw_tick_lots
        .saturating_mul(u128::from(market.config.tick_size_bps))
        .saturating_mul(u128::from(market.config.lot_size_microunits));
    let denominator = 10_000_u128.saturating_mul(1_000_000);
    if scaled == 0 {
        0
    } else {
        scaled.saturating_sub(1) / denominator + 1
    }
}

pub(crate) fn scaled_raw_tick_lot_notional_usdc_floor_for_rpc(
    market: Option<&PerpetualMarket>,
    raw_tick_lots: u128,
) -> u128 {
    let Some(market) = market else {
        return raw_tick_lots;
    };
    raw_tick_lots
        .saturating_mul(u128::from(market.config.tick_size_bps))
        .saturating_mul(u128::from(market.config.lot_size_microunits))
        / 10_000_u128.saturating_mul(1_000_000)
}

pub(crate) fn u128_to_i128_saturating_for_rpc(value: u128) -> i128 {
    if value > i128::MAX as u128 {
        i128::MAX
    } else {
        value as i128
    }
}

pub(crate) fn proportional_bps_round_up_for_rpc(notional: u128, bps: u32) -> u128 {
    let numerator = notional.saturating_mul(u128::from(bps));
    if numerator == 0 {
        0
    } else {
        numerator.saturating_sub(1) / 10_000 + 1
    }
}

pub(crate) fn scaled_signed_raw_tick_lot_pnl_usdc_for_rpc(
    market: Option<&PerpetualMarket>,
    raw_pnl: i128,
) -> i128 {
    let magnitude = raw_pnl.unsigned_abs();
    if raw_pnl < 0 {
        -u128_to_i128_saturating_for_rpc(scaled_raw_tick_lot_notional_usdc_for_rpc(
            market, magnitude,
        ))
    } else {
        u128_to_i128_saturating_for_rpc(scaled_raw_tick_lot_notional_usdc_floor_for_rpc(
            market, magnitude,
        ))
    }
}

pub(crate) fn position_history_record_from_event(
    engine: &SingleNodeEngine,
    event: &RuntimeEventRecord,
) -> Option<PositionHistoryRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::PositionUpdated {
            market_id,
            account_id,
            net_lots,
            open_notional,
            realized_pnl,
            average_entry_price_ticks,
            buy_lots,
            sell_lots,
            buy_notional,
            sell_notional,
        } => Some(PositionHistoryRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            net_lots: *net_lots,
            open_notional: scaled_raw_tick_lot_notional_usdc_for_rpc(
                engine.perpetual_market(*market_id),
                *open_notional,
            ),
            realized_pnl: scaled_signed_raw_tick_lot_pnl_usdc_for_rpc(
                engine.perpetual_market(*market_id),
                *realized_pnl,
            ),
            average_entry_price_ticks: *average_entry_price_ticks,
            buy_lots: *buy_lots,
            sell_lots: *sell_lots,
            buy_notional: scaled_raw_tick_lot_notional_usdc_for_rpc(
                engine.perpetual_market(*market_id),
                *buy_notional,
            ),
            sell_notional: scaled_raw_tick_lot_notional_usdc_for_rpc(
                engine.perpetual_market(*market_id),
                *sell_notional,
            ),
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn liquidation_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<LiquidationRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_account_id,
            requested_lots,
            filled_lots,
            remaining_lots,
            executed_notional,
            penalty,
            insurance_cover,
            liquidator_reward,
            insurance_fund_balance,
        } => Some(LiquidationRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            liquidator_account_id: *liquidator_account_id,
            requested_lots: *requested_lots,
            filled_lots: *filled_lots,
            remaining_lots: *remaining_lots,
            executed_notional: *executed_notional,
            penalty: *penalty,
            insurance_cover: *insurance_cover,
            liquidator_reward: *liquidator_reward,
            insurance_fund_balance: *insurance_fund_balance,
            outcome: None,
            post_net_lots: None,
            post_margin_surplus: None,
            post_under_maintenance: None,
            post_account_frozen: None,
            post_market_bad_debt_balance: None,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) struct LiquidationOutcomeRecord {
    pub(crate) market_id: u64,
    pub(crate) account_id: u64,
    pub(crate) liquidator_account_id: u64,
    pub(crate) outcome: LiquidationExecutionOutcome,
    pub(crate) post_net_lots: i64,
    pub(crate) post_margin_surplus: i128,
    pub(crate) post_under_maintenance: bool,
    pub(crate) post_account_frozen: bool,
    pub(crate) post_market_bad_debt_balance: u128,
}

pub(crate) fn liquidation_outcome_from_event(
    event: &RuntimeEventRecord,
) -> Option<LiquidationOutcomeRecord> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::LiquidationOutcome {
            market_id,
            account_id,
            liquidator_account_id,
            outcome,
            post_net_lots,
            post_margin_surplus,
            post_under_maintenance,
            post_account_frozen,
            post_market_bad_debt_balance,
        } => Some(LiquidationOutcomeRecord {
            market_id: *market_id,
            account_id: *account_id,
            liquidator_account_id: *liquidator_account_id,
            outcome: *outcome,
            post_net_lots: *post_net_lots,
            post_margin_surplus: *post_margin_surplus,
            post_under_maintenance: *post_under_maintenance,
            post_account_frozen: *post_account_frozen,
            post_market_bad_debt_balance: *post_market_bad_debt_balance,
        }),
        _ => None,
    }
}

pub(crate) fn liquidation_escalation_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<LiquidationEscalationRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::LiquidationEscalationUpdated {
            market_id,
            account_id,
            action,
            stall_count,
            freeze_threshold,
            freeze_imminent,
            post_margin_surplus,
            post_under_maintenance,
            post_account_frozen,
        } => Some(LiquidationEscalationRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            action: *action,
            stall_count: *stall_count,
            freeze_threshold: *freeze_threshold,
            freeze_imminent: *freeze_imminent,
            post_margin_surplus: *post_margin_surplus,
            post_under_maintenance: *post_under_maintenance,
            post_account_frozen: *post_account_frozen,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn oracle_history_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<OracleHistoryRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::OracleConfigUpdated {
            market_id,
            max_staleness_ms,
        } => Some(OracleHistoryRecordResponse {
            market_id: *market_id,
            kind: OracleHistoryKindResponse::OracleConfigUpdated {
                max_staleness_ms: *max_staleness_ms,
            },
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MarkPriceUpdated {
            market_id,
            mark_price_ticks,
        } => Some(OracleHistoryRecordResponse {
            market_id: *market_id,
            kind: OracleHistoryKindResponse::MarkPriceUpdated {
                mark_price_ticks: *mark_price_ticks,
            },
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::IndexPriceUpdated {
            market_id,
            index_price_ticks,
        } => Some(OracleHistoryRecordResponse {
            market_id: *market_id,
            kind: OracleHistoryKindResponse::IndexPriceUpdated {
                index_price_ticks: *index_price_ticks,
            },
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn liquidation_event_totals(
    events: &[RuntimeEventRecord],
    market_id: u64,
    account_id: u64,
    liquidator_account_id: u64,
) -> (usize, u64, u128, u128, u128, u128, u128) {
    let mut step_count = 0_usize;
    let mut requested_lots = 0_u64;
    let mut executed_notional = 0_u128;
    let mut penalty = 0_u128;
    let mut insurance_cover = 0_u128;
    let mut liquidator_reward = 0_u128;
    let mut insurance_fund_balance = 0_u128;

    for event in events {
        match &event.kind {
            kombat_chain_streams::RuntimeEventKind::LiquidationExecuted {
                market_id: event_market_id,
                account_id: event_account_id,
                liquidator_account_id: event_liquidator_account_id,
                requested_lots: event_requested_lots,
                executed_notional: event_executed_notional,
                penalty: event_penalty,
                insurance_cover: event_insurance_cover,
                liquidator_reward: event_liquidator_reward,
                insurance_fund_balance: event_insurance_fund_balance,
                ..
            } if *event_market_id == market_id
                && *event_account_id == account_id
                && *event_liquidator_account_id == liquidator_account_id =>
            {
                step_count = step_count.saturating_add(1);
                requested_lots = requested_lots.saturating_add(*event_requested_lots);
                executed_notional = executed_notional.saturating_add(*event_executed_notional);
                penalty = penalty.saturating_add(*event_penalty);
                insurance_cover = insurance_cover.saturating_add(*event_insurance_cover);
                liquidator_reward = liquidator_reward.saturating_add(*event_liquidator_reward);
                insurance_fund_balance = *event_insurance_fund_balance;
            }
            _ => {}
        }
    }

    (
        step_count,
        requested_lots,
        executed_notional,
        penalty,
        insurance_cover,
        liquidator_reward,
        insurance_fund_balance,
    )
}

pub(crate) fn market_trading_state_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<MarketTradingStateRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::MarketTradingStateUpdated { market_id, paused } => {
            Some(MarketTradingStateRecordResponse {
                market_id: *market_id,
                paused: *paused,
                seq: event.seq,
                block_height: event.block_height,
                timestamp_ms: event.timestamp_ms,
            })
        }
        _ => None,
    }
}

pub(crate) fn insurance_fund_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<InsuranceFundUpdateRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::InsuranceFundUpdated {
            market_id,
            delta,
            balance,
            source,
        } => Some(InsuranceFundUpdateRecordResponse {
            market_id: *market_id,
            delta: *delta,
            balance: *balance,
            source: *source,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn bad_debt_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<BadDebtRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } => Some(BadDebtRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            kind: BadDebtRecordKind::Updated,
            delta: *delta,
            balance: *balance,
            source: *source,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::BadDebtRecovered {
            market_id,
            account_id,
            recovered,
            balance,
            source,
        } => Some(BadDebtRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            kind: BadDebtRecordKind::Recovered,
            delta: *recovered,
            balance: *balance,
            source: *source,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn solvency_breach_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<SolvencyBreachRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::SolvencyBreached {
            market_id,
            account_id,
            residual_bad_debt,
        } => Some(SolvencyBreachRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            residual_bad_debt: *residual_bad_debt,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn risk_state_history_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<RiskStateHistoryRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::RiskStateUpdated {
            market_id,
            account_id,
            net_lots,
            collateral,
            equity,
            maintenance_requirement,
            margin_surplus,
            under_maintenance,
        } => Some(RiskStateHistoryRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            net_lots: *net_lots,
            collateral: *collateral,
            equity: *equity,
            maintenance_requirement: *maintenance_requirement,
            liquidation_gap: liquidation_gap(*equity, *maintenance_requirement),
            margin_surplus: *margin_surplus,
            under_maintenance: *under_maintenance,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn liquidation_candidate_history_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<LiquidationCandidateHistoryRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::LiquidationCandidateUpdated {
            market_id,
            account_id,
            net_lots,
            equity,
            maintenance_requirement,
            margin_surplus,
            under_maintenance,
        } => Some(LiquidationCandidateHistoryRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            net_lots: *net_lots,
            equity: *equity,
            maintenance_requirement: *maintenance_requirement,
            liquidation_gap: liquidation_gap(*equity, *maintenance_requirement),
            margin_surplus: *margin_surplus,
            under_maintenance: *under_maintenance,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn funding_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<FundingRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::FundingIndexUpdated {
            market_id,
            funding_index,
        } => Some(FundingRecordResponse {
            market_id: *market_id,
            account_id: None,
            funding_index: *funding_index,
            funding_delta: None,
            kind: FundingRecordKind::IndexUpdated,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::FundingApplied {
            market_id,
            account_id,
            funding_delta,
            funding_index,
        } => Some(FundingRecordResponse {
            market_id: *market_id,
            account_id: Some(*account_id),
            funding_index: *funding_index,
            funding_delta: Some(*funding_delta),
            kind: FundingRecordKind::Applied,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn fee_record_from_event(event: &RuntimeEventRecord) -> Option<FeeRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::FeeCharged {
            market_id,
            account_id,
            client_order_id,
            role,
            fee_amount,
            fee_vault_balance,
        } => Some(FeeRecordResponse {
            market_id: *market_id,
            account_id: *account_id,
            client_order_id: *client_order_id,
            role: *role,
            fee_amount: *fee_amount,
            fee_vault_balance: *fee_vault_balance,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn revenue_split_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<RevenueSplitRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::FeeRevenueDistributed {
            market_id,
            source_fee_amount,
            validator_rewards_delta,
            validator_reward_balance,
            market_maker_rebate_delta,
            market_maker_rebate_balance,
            insurance_fund_delta,
            insurance_fund_balance,
            treasury_delta,
            treasury_balance,
        } => Some(RevenueSplitRecordResponse {
            market_id: *market_id,
            source_fee_amount: *source_fee_amount,
            validator_rewards_delta: *validator_rewards_delta,
            validator_reward_balance: *validator_reward_balance,
            market_maker_rebate_delta: *market_maker_rebate_delta,
            market_maker_rebate_balance: *market_maker_rebate_balance,
            insurance_fund_delta: *insurance_fund_delta,
            insurance_fund_balance: *insurance_fund_balance,
            treasury_delta: *treasury_delta,
            treasury_balance: *treasury_balance,
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn account_history_record_from_event(
    event: &RuntimeEventRecord,
) -> Option<AccountHistoryRecordResponse> {
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::AccountCreated { account_id, owner } => {
            Some(AccountHistoryRecordResponse {
                account_id: *account_id,
                kind: AccountHistoryKindResponse::Created { owner: *owner },
                seq: event.seq,
                block_height: event.block_height,
                timestamp_ms: event.timestamp_ms,
            })
        }
        kombat_chain_streams::RuntimeEventKind::AccountUpdated {
            account_id,
            action,
            reason,
            owner,
            pending_owner,
            paused,
            nonce,
            code_hash,
            controllers,
        } => Some(AccountHistoryRecordResponse {
            account_id: *account_id,
            kind: AccountHistoryKindResponse::Updated {
                action: *action,
                reason: *reason,
                owner: *owner,
                pending_owner: *pending_owner,
                paused: *paused,
                nonce: *nonce,
                code_hash: *code_hash,
                controllers: controllers.clone(),
            },
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
        }),
        _ => None,
    }
}

pub(crate) fn position_response_from_ledger(
    engine: &SingleNodeEngine,
    market_id: u64,
    account_id: u64,
    ledger: PositionLedger,
) -> PositionResponse {
    let net_lots = ledger.net_lots();
    let side = if net_lots > 0 {
        PositionSide::Long
    } else if net_lots < 0 {
        PositionSide::Short
    } else {
        PositionSide::Flat
    };
    let market = engine.perpetual_market(market_id);
    PositionResponse {
        market_id,
        account_id,
        mandate_id: ledger.mandate_id,
        side,
        net_lots,
        open_notional: scaled_raw_tick_lot_notional_usdc_for_rpc(market, ledger.open_notional),
        realized_pnl: scaled_signed_raw_tick_lot_pnl_usdc_for_rpc(market, ledger.realized_pnl),
        average_entry_price_ticks: ledger.average_entry_price_ticks(),
        buy_lots: ledger.buy_lots,
        sell_lots: ledger.sell_lots,
        buy_notional: scaled_raw_tick_lot_notional_usdc_for_rpc(market, ledger.buy_notional),
        sell_notional: scaled_raw_tick_lot_notional_usdc_for_rpc(market, ledger.sell_notional),
        last_fill_seq: ledger.last_fill_seq,
        last_fill_timestamp_ms: ledger.last_fill_timestamp_ms,
    }
}

pub(crate) fn risk_snapshot_from_ledger(
    engine: &SingleNodeEngine,
    market_id: u64,
    account_id: u64,
    ledger: PositionLedger,
    margin: MarginSnapshot,
    mark_price_ticks: Option<u64>,
    supplement: RiskSnapshotSupplement,
) -> RiskSnapshotResponse {
    let position = position_response_from_ledger(engine, market_id, account_id, ledger);
    let max_liquidatable_lots = ledger.open_abs_lots();
    let suggested_liquidation_outcome =
        suggested_liquidation_outcome(supplement.suggested_liquidation_lots);

    RiskSnapshotResponse {
        market_id,
        account_id,
        side: position.side,
        net_lots: position.net_lots,
        max_liquidatable_lots,
        liquidation_stall_count: supplement.liquidation_stall_count,
        liquidation_freeze_threshold: supplement.liquidation_freeze_threshold,
        liquidation_freeze_imminent: supplement.liquidation_freeze_imminent,
        suggested_liquidation_lots: supplement
            .suggested_liquidation_lots
            .map(|suggestion| suggestion.lots),
        suggested_liquidation_outcome,
        suggested_liquidation_clears_maintenance: supplement
            .suggested_liquidation_lots
            .is_some_and(|suggestion| suggestion.clears_maintenance),
        suggested_post_margin_surplus: supplement
            .suggested_liquidation_lots
            .map(|suggestion| suggestion.post_margin_surplus),
        suggested_post_under_maintenance: supplement
            .suggested_liquidation_lots
            .map(|suggestion| suggestion.post_under_maintenance),
        suggested_post_account_frozen: supplement
            .suggested_liquidation_lots
            .map(|suggestion| suggestion.post_account_frozen),
        suggested_post_market_bad_debt_balance: supplement
            .suggested_liquidation_lots
            .map(|suggestion| suggestion.post_market_bad_debt_balance),
        open_notional: position.open_notional,
        realized_pnl: margin.realized_pnl,
        average_entry_price_ticks: position.average_entry_price_ticks,
        collateral: margin.collateral,
        mark_price_ticks,
        mark_notional: margin.mark_notional,
        unrealized_pnl: margin.unrealized_pnl,
        equity: margin.equity,
        maintenance_margin_bps: margin.maintenance_margin_bps,
        maintenance_requirement: margin.maintenance_requirement,
        liquidation_gap: liquidation_gap(margin.equity, margin.maintenance_requirement),
        margin_surplus: margin.margin_surplus,
        under_maintenance: margin.under_maintenance,
        latest_account_risk_transition: supplement.latest_account_risk_transition,
        last_fill_seq: position.last_fill_seq,
        last_fill_timestamp_ms: position.last_fill_timestamp_ms,
    }
}

pub(crate) fn liquidation_gap(equity: i128, maintenance_requirement: u128) -> u128 {
    i128::try_from(maintenance_requirement)
        .unwrap_or(i128::MAX)
        .saturating_sub(equity)
        .max(0) as u128
}

pub(crate) fn suggested_liquidation_outcome(
    suggestion: Option<kombat_chain_node::LiquidationSizingSuggestion>,
) -> Option<SuggestedLiquidationOutcome> {
    suggestion.map(|suggestion| {
        if suggestion.clears_maintenance {
            SuggestedLiquidationOutcome::ClearsMaintenance
        } else if suggestion.post_market_bad_debt_balance > 0 {
            SuggestedLiquidationOutcome::CreatesBadDebt
        } else if suggestion.post_account_frozen {
            SuggestedLiquidationOutcome::FreezesAccount
        } else {
            SuggestedLiquidationOutcome::StillUnderMaintenance
        }
    })
}

pub(crate) fn suggested_liquidation_lots(
    engine: &SingleNodeEngine,
    market_id: u64,
    account_id: u64,
) -> Option<kombat_chain_node::LiquidationSizingSuggestion> {
    engine.suggested_liquidation_size(market_id, account_id)
}

pub(crate) fn latest_account_risk_transition(
    engine: &SingleNodeEngine,
    account_id: u64,
    market_id: Option<u64>,
) -> Option<LatestRiskTransitionResponse> {
    engine
        .runtime_events()
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            kombat_chain_streams::RuntimeEventKind::AccountUpdated {
                account_id: event_account_id,
                action,
                reason: Some(reason),
                ..
            } if *event_account_id == account_id => match action {
                AccountUpdateAction::Freeze => Some(LatestRiskTransitionResponse {
                    kind: RiskTransitionKindResponse::Freeze,
                    source: *reason,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                }),
                AccountUpdateAction::Unfreeze => Some(LatestRiskTransitionResponse {
                    kind: RiskTransitionKindResponse::Unfreeze,
                    source: *reason,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                }),
                _ => None,
            },
            kombat_chain_streams::RuntimeEventKind::BadDebtUpdated {
                market_id: event_market_id,
                account_id: event_account_id,
                source,
                ..
            } if *event_account_id == account_id
                && market_id.is_none_or(|expected| *event_market_id == expected) =>
            {
                Some(LatestRiskTransitionResponse {
                    kind: RiskTransitionKindResponse::BadDebtUpdated,
                    source: *source,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                })
            }
            kombat_chain_streams::RuntimeEventKind::BadDebtRecovered {
                market_id: event_market_id,
                account_id: event_account_id,
                source,
                ..
            } if *event_account_id == account_id
                && market_id.is_none_or(|expected| *event_market_id == expected) =>
            {
                Some(LatestRiskTransitionResponse {
                    kind: RiskTransitionKindResponse::BadDebtRecovered,
                    source: *source,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                })
            }
            _ => None,
        })
}

pub(crate) fn latest_market_risk_transition(
    engine: &SingleNodeEngine,
    market_id: u64,
    market_account_ids: &BTreeSet<u64>,
) -> Option<LatestMarketRiskTransitionResponse> {
    engine
        .runtime_events()
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            kombat_chain_streams::RuntimeEventKind::AccountUpdated {
                account_id,
                action,
                reason: Some(reason),
                ..
            } if market_account_ids.contains(account_id) => match action {
                AccountUpdateAction::Freeze => Some(LatestMarketRiskTransitionResponse {
                    market_id,
                    account_id: *account_id,
                    kind: RiskTransitionKindResponse::Freeze,
                    source: *reason,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                }),
                AccountUpdateAction::Unfreeze => Some(LatestMarketRiskTransitionResponse {
                    market_id,
                    account_id: *account_id,
                    kind: RiskTransitionKindResponse::Unfreeze,
                    source: *reason,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                }),
                _ => None,
            },
            kombat_chain_streams::RuntimeEventKind::BadDebtUpdated {
                market_id: event_market_id,
                account_id,
                source,
                ..
            } if *event_market_id == market_id => Some(LatestMarketRiskTransitionResponse {
                market_id,
                account_id: *account_id,
                kind: RiskTransitionKindResponse::BadDebtUpdated,
                source: *source,
                seq: event.seq,
                block_height: event.block_height,
                timestamp_ms: event.timestamp_ms,
            }),
            kombat_chain_streams::RuntimeEventKind::BadDebtRecovered {
                market_id: event_market_id,
                account_id,
                source,
                ..
            } if *event_market_id == market_id => Some(LatestMarketRiskTransitionResponse {
                market_id,
                account_id: *account_id,
                kind: RiskTransitionKindResponse::BadDebtRecovered,
                source: *source,
                seq: event.seq,
                block_height: event.block_height,
                timestamp_ms: event.timestamp_ms,
            }),
            _ => None,
        })
}

pub(crate) fn recent_market_risk_transitions(
    engine: &SingleNodeEngine,
    market_id: u64,
    market_account_ids: &BTreeSet<u64>,
    limit: usize,
) -> Vec<LatestMarketRiskTransitionResponse> {
    engine
        .runtime_events()
        .iter()
        .rev()
        .filter_map(|event| match &event.kind {
            kombat_chain_streams::RuntimeEventKind::AccountUpdated {
                account_id,
                action,
                reason: Some(reason),
                ..
            } if market_account_ids.contains(account_id) => match action {
                AccountUpdateAction::Freeze => Some(LatestMarketRiskTransitionResponse {
                    market_id,
                    account_id: *account_id,
                    kind: RiskTransitionKindResponse::Freeze,
                    source: *reason,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                }),
                AccountUpdateAction::Unfreeze => Some(LatestMarketRiskTransitionResponse {
                    market_id,
                    account_id: *account_id,
                    kind: RiskTransitionKindResponse::Unfreeze,
                    source: *reason,
                    seq: event.seq,
                    block_height: event.block_height,
                    timestamp_ms: event.timestamp_ms,
                }),
                _ => None,
            },
            kombat_chain_streams::RuntimeEventKind::BadDebtUpdated {
                market_id: event_market_id,
                account_id,
                source,
                ..
            } if *event_market_id == market_id => Some(LatestMarketRiskTransitionResponse {
                market_id,
                account_id: *account_id,
                kind: RiskTransitionKindResponse::BadDebtUpdated,
                source: *source,
                seq: event.seq,
                block_height: event.block_height,
                timestamp_ms: event.timestamp_ms,
            }),
            kombat_chain_streams::RuntimeEventKind::BadDebtRecovered {
                market_id: event_market_id,
                account_id,
                source,
                ..
            } if *event_market_id == market_id => Some(LatestMarketRiskTransitionResponse {
                market_id,
                account_id: *account_id,
                kind: RiskTransitionKindResponse::BadDebtRecovered,
                source: *source,
                seq: event.seq,
                block_height: event.block_height,
                timestamp_ms: event.timestamp_ms,
            }),
            _ => None,
        })
        .take(limit)
        .collect()
}
