use crate::{MandateId, NodeError, BPS_DENOMINATOR, MAX_ORACLE_FEEDERS};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_oracle::OracleAggregationError;
use kombat_chain_streams::MarketLifecycleStatus;
use kombat_chain_streams::OrderFillSide;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PositionLedger {
    pub open_net_lots: i64,
    pub open_notional: u128,
    pub mandate_id: Option<MandateId>,
    pub opened_at_timestamp_ms: Option<u64>,
    pub realized_pnl: i128,
    pub funding_index: i128,
    pub buy_lots: u64,
    pub sell_lots: u64,
    pub buy_notional: u128,
    pub sell_notional: u128,
    pub last_fill_seq: Option<u64>,
    pub last_fill_timestamp_ms: Option<u64>,
}

impl PositionLedger {
    pub fn net_lots(&self) -> i64 {
        self.open_net_lots
    }

    pub fn open_abs_lots(&self) -> u64 {
        self.open_net_lots.unsigned_abs()
    }

    pub fn average_entry_price_ticks(&self) -> Option<u64> {
        let open_lots = self.open_abs_lots();
        if open_lots == 0 {
            None
        } else {
            Some((self.open_notional / u128::from(open_lots)) as u64)
        }
    }

    pub(crate) fn apply_fill(
        &mut self,
        side: OrderFillSide,
        price_ticks: u64,
        quantity_lots: u64,
        mandate_id: Option<MandateId>,
        funding_index: i128,
        seq: u64,
        timestamp_ms: u64,
    ) {
        let notional = u128::from(price_ticks) * u128::from(quantity_lots);
        let prior_net_lots = self.open_net_lots;
        let was_flat = prior_net_lots == 0;
        self.apply_funding(funding_index);
        match side {
            OrderFillSide::Buy => {
                self.buy_lots = self.buy_lots.saturating_add(quantity_lots);
                self.buy_notional = self.buy_notional.saturating_add(notional);
                self.apply_open_delta(true, price_ticks, quantity_lots);
            }
            OrderFillSide::Sell => {
                self.sell_lots = self.sell_lots.saturating_add(quantity_lots);
                self.sell_notional = self.sell_notional.saturating_add(notional);
                self.apply_open_delta(false, price_ticks, quantity_lots);
            }
        }
        if self.open_abs_lots() == 0 {
            self.mandate_id = None;
            self.opened_at_timestamp_ms = None;
        } else if was_flat {
            self.mandate_id = mandate_id;
            self.opened_at_timestamp_ms = Some(timestamp_ms);
        } else if prior_net_lots.signum() != self.open_net_lots.signum() {
            // A single oversized fill can close and reopen in the opposite
            // direction; that new exposure gets a fresh origin timestamp.
            self.mandate_id = mandate_id;
            self.opened_at_timestamp_ms = Some(timestamp_ms);
        }
        // The current open exposure owns a stable origin timestamp. Adds do
        // not reset it; flat-to-open and flip-through-zero transitions do.
        debug_assert!(self.open_abs_lots() != 0 || self.mandate_id.is_none());
        debug_assert_eq!(
            self.open_abs_lots() == 0,
            self.opened_at_timestamp_ms.is_none()
        );
        self.funding_index = funding_index;
        self.last_fill_seq = Some(seq);
        self.last_fill_timestamp_ms = Some(timestamp_ms);
    }

    pub(crate) fn apply_funding(&mut self, funding_index: i128) -> i128 {
        let funding_delta = funding_delta(self.open_net_lots, self.funding_index, funding_index);
        self.realized_pnl = self.realized_pnl.saturating_add(funding_delta);
        self.funding_index = funding_index;
        funding_delta
    }

    fn apply_open_delta(&mut self, is_buy: bool, price_ticks: u64, quantity_lots: u64) {
        let fill_notional = u128::from(price_ticks) * u128::from(quantity_lots);
        if is_buy {
            if self.open_net_lots >= 0 {
                self.open_net_lots = self
                    .open_net_lots
                    .saturating_add(i64::try_from(quantity_lots).unwrap_or(i64::MAX));
                self.open_notional = self.open_notional.saturating_add(fill_notional);
                return;
            }

            let current_short = self.open_abs_lots();
            let closing = current_short.min(quantity_lots);
            self.realized_pnl = self.realized_pnl.saturating_add(realized_pnl_delta(
                self.open_notional,
                current_short,
                closing,
                price_ticks,
                true,
            ));
            let remaining_short = current_short.saturating_sub(closing);
            self.open_notional =
                proportional_open_notional(self.open_notional, current_short, remaining_short);

            if remaining_short == 0 {
                let opening = quantity_lots.saturating_sub(closing);
                self.open_net_lots = i64::try_from(opening).unwrap_or(i64::MAX);
                self.open_notional = u128::from(price_ticks) * u128::from(opening);
            } else {
                self.open_net_lots = -i64::try_from(remaining_short).unwrap_or(i64::MAX);
            }
        } else {
            if self.open_net_lots <= 0 {
                self.open_net_lots = self
                    .open_net_lots
                    .saturating_sub(i64::try_from(quantity_lots).unwrap_or(i64::MAX));
                self.open_notional = self.open_notional.saturating_add(fill_notional);
                return;
            }

            let current_long = self.open_abs_lots();
            let closing = current_long.min(quantity_lots);
            self.realized_pnl = self.realized_pnl.saturating_add(realized_pnl_delta(
                self.open_notional,
                current_long,
                closing,
                price_ticks,
                false,
            ));
            let remaining_long = current_long.saturating_sub(closing);
            self.open_notional =
                proportional_open_notional(self.open_notional, current_long, remaining_long);

            if remaining_long == 0 {
                let opening = quantity_lots.saturating_sub(closing);
                self.open_net_lots = -i64::try_from(opening).unwrap_or(i64::MAX);
                self.open_notional = u128::from(price_ticks) * u128::from(opening);
            } else {
                self.open_net_lots = i64::try_from(remaining_long).unwrap_or(i64::MAX);
            }
        }
    }
}

fn realized_pnl_delta(
    current_notional: u128,
    current_lots: u64,
    closing_lots: u64,
    exit_price_ticks: u64,
    closing_short: bool,
) -> i128 {
    if current_lots == 0 || closing_lots == 0 {
        return 0;
    }

    let average_entry_price_ticks = current_notional / u128::from(current_lots);
    let price_delta = if closing_short {
        i128::try_from(average_entry_price_ticks).unwrap_or(i128::MAX)
            - i128::from(exit_price_ticks)
    } else {
        i128::from(exit_price_ticks)
            - i128::try_from(average_entry_price_ticks).unwrap_or(i128::MAX)
    };

    price_delta.saturating_mul(i128::from(closing_lots))
}

fn funding_delta(open_net_lots: i64, previous_index: i128, next_index: i128) -> i128 {
    if open_net_lots == 0 || previous_index == next_index {
        return 0;
    }

    let index_delta = next_index.saturating_sub(previous_index);
    index_delta
        .saturating_mul(i128::from(open_net_lots))
        .saturating_neg()
}

fn proportional_open_notional(
    current_notional: u128,
    current_lots: u64,
    remaining_lots: u64,
) -> u128 {
    if remaining_lots == 0 || current_lots == 0 {
        0
    } else {
        current_notional.saturating_mul(u128::from(remaining_lots)) / u128::from(current_lots)
    }
}

pub(crate) fn unrealized_pnl(ledger: &PositionLedger, mark_price_ticks: Option<u64>) -> i128 {
    match (
        ledger.net_lots().cmp(&0),
        ledger.average_entry_price_ticks(),
        mark_price_ticks,
    ) {
        (std::cmp::Ordering::Greater, Some(avg), Some(mark)) => {
            { i128::from(mark) - i128::from(avg) }
                .saturating_mul(i128::from(ledger.open_abs_lots()))
        }
        (std::cmp::Ordering::Less, Some(avg), Some(mark)) => { i128::from(avg) - i128::from(mark) }
            .saturating_mul(i128::from(ledger.open_abs_lots())),
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MarketKind {
    Perpetual,
    Spot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MarketStatus {
    Active,
    Paused,
    ReduceOnly,
    Delisted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PerpetualMarketConfig {
    pub initial_margin_bps: u32,
    pub maintenance_margin_bps: u32,
    pub max_leverage: u32,
    pub lot_size_microunits: u64,
    pub tick_size_bps: u32,
    pub funding_interval_ms: u64,
    pub oracle_max_staleness_ms: u64,
    pub breaker_price_band_bps: u32,
    pub oracle_min_sources: u8,
    /// Percentage-like confidence floor in the inclusive range 1..=100.
    pub oracle_min_confidence: u8,
    pub oracle_min_samples: u16,
    pub oracle_feeder_count: u8,
    pub oracle_feeder_allowlist: [[u8; 32]; MAX_ORACLE_FEEDERS],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PerpetualMarket {
    pub market_id: u64,
    pub kind: MarketKind,
    pub status: MarketStatus,
    pub base_symbol: String,
    pub quote_asset: u32,
    pub oracle_feed_id: u64,
    pub config: PerpetualMarketConfig,
    pub listed_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct OracleFeedSubmission {
    pub market_id: u64,
    pub oracle_feed_id: u64,
    pub feeder_id: [u8; 32],
    pub price_ticks: u64,
    pub sources_used: u8,
    pub confidence: u8,
    pub submitted_at_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OracleFeedSample {
    pub feeder_id: [u8; 32],
    pub price_ticks: u64,
    pub sources_used: u8,
    pub confidence: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OracleFeedAggregation {
    pub market_id: u64,
    pub oracle_feed_id: u64,
    pub price_ticks: u64,
    pub submitted_samples: usize,
    pub accepted_samples: usize,
    pub rejected_samples: usize,
    pub expected_feeders: usize,
    pub missing_feeders: usize,
    pub timestamp_ms: u64,
}

pub(crate) fn validate_perpetual_market_config(
    market_id: u64,
    base_symbol: &str,
    config: PerpetualMarketConfig,
) -> Result<(), NodeError> {
    if market_id == 0 {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "market_id_zero",
        });
    }
    if !(2..=16).contains(&base_symbol.len())
        || !base_symbol
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_base_symbol",
        });
    }
    if config.initial_margin_bps == 0
        || config.initial_margin_bps > BPS_DENOMINATOR as u32
        || config.maintenance_margin_bps == 0
        || config.maintenance_margin_bps > config.initial_margin_bps
    {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_margin_bps",
        });
    }
    if config.max_leverage == 0 || config.lot_size_microunits == 0 || config.tick_size_bps == 0 {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_trading_quantum",
        });
    }
    if config.funding_interval_ms == 0 || config.oracle_max_staleness_ms == 0 {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_time_config",
        });
    }
    if config.breaker_price_band_bps > BPS_DENOMINATOR as u32 {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_breaker_config",
        });
    }
    if config.oracle_min_sources == 0
        || config.oracle_min_confidence == 0
        || config.oracle_min_confidence > 100
        || config.oracle_min_samples == 0
    {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_oracle_quorum_config",
        });
    }
    if usize::from(config.oracle_feeder_count) > MAX_ORACLE_FEEDERS
        || config.oracle_feeder_count == 0
        || usize::from(config.oracle_min_samples) > usize::from(config.oracle_feeder_count)
    {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_oracle_feeder_config",
        });
    }
    let active_feeders = &config.oracle_feeder_allowlist[..usize::from(config.oracle_feeder_count)];
    let mut seen_feeders = BTreeSet::new();
    for feeder_id in active_feeders {
        if *feeder_id == [0; 32] || !seen_feeders.insert(*feeder_id) {
            return Err(NodeError::InvalidMarketConfig {
                market_id,
                reason: "invalid_oracle_feeder_config",
            });
        }
    }
    if config.oracle_feeder_allowlist[usize::from(config.oracle_feeder_count)..]
        .iter()
        .any(|feeder_id| *feeder_id != [0; 32])
    {
        return Err(NodeError::InvalidMarketConfig {
            market_id,
            reason: "invalid_oracle_feeder_config",
        });
    }
    Ok(())
}

pub(crate) fn oracle_feeder_allowed(
    oracle_feeder_count: u8,
    oracle_feeder_allowlist: &[[u8; 32]; MAX_ORACLE_FEEDERS],
    feeder_id: &[u8; 32],
) -> bool {
    oracle_feeder_allowlist[..usize::from(oracle_feeder_count)]
        .iter()
        .any(|allowed| allowed == feeder_id)
}

pub(crate) fn oracle_missing_feeder_count(
    oracle_feeder_count: u8,
    oracle_feeder_allowlist: &[[u8; 32]; MAX_ORACLE_FEEDERS],
    submitted: &[OracleFeedSubmission],
) -> usize {
    let submitted_feeders = submitted
        .iter()
        .map(|submission| submission.feeder_id)
        .collect::<BTreeSet<_>>();
    oracle_feeder_allowlist[..usize::from(oracle_feeder_count)]
        .iter()
        .filter(|feeder_id| !submitted_feeders.contains(*feeder_id))
        .count()
}

pub(crate) fn market_status_event(status: MarketStatus) -> MarketLifecycleStatus {
    match status {
        MarketStatus::Active => MarketLifecycleStatus::Active,
        MarketStatus::Paused => MarketLifecycleStatus::Paused,
        MarketStatus::ReduceOnly => MarketLifecycleStatus::ReduceOnly,
        MarketStatus::Delisted => MarketLifecycleStatus::Delisted,
    }
}

pub(crate) fn oracle_aggregation_error_reason(error: OracleAggregationError) -> &'static str {
    match error {
        OracleAggregationError::InvalidConfig => "invalid_oracle_aggregation_config",
        OracleAggregationError::InsufficientSamples { .. } => "insufficient_oracle_samples",
    }
}
