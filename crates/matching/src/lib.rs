use std::collections::BTreeSet;

pub const CRATE_NAME: &str = "matching";

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OrderSide {
    Bid,
    Ask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OrderKind {
    Limit { price_ticks: u64 },
    Market,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
    Gtt { expires_at_ms: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderRequest {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub side: OrderSide,
    pub quantity_lots: u64,
    pub kind: OrderKind,
    pub time_in_force: TimeInForce,
    pub post_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidityPreview {
    pub would_cross: bool,
    pub fully_fillable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreMatchOutcome {
    pub crosses_book: bool,
    pub must_execute_immediately: bool,
    pub requires_full_fill: bool,
    pub may_post: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderRejectReason {
    ZeroMarketId,
    ZeroAccountId,
    ZeroQuantity,
    ZeroClientOrderId,
    ZeroLimitPrice,
    MarketMismatch { expected: u64, actual: u64 },
    MarketOrderRequiresImmediateTif,
    PostOnlyRequiresLimitOrder,
    PostOnlyIncompatibleWithImmediateTif,
    GttRequiresFutureExpiry { now_ms: u64, expires_at_ms: u64 },
    PostOnlyWouldCross,
    FillOrKillInsufficientLiquidity,
    SelfTradePrevented,
    DuplicateClientOrderId,
    UnknownRestingOrder,
    ReplaceAccountMismatch { expected: u64, actual: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestingBatchError {
    Reject(OrderRejectReason),
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct RestingOrder {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub side: OrderSide,
    pub price_ticks: u64,
    pub remaining_lots: u64,
    pub expires_at_ms: Option<u64>,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BookLevel {
    pub price_ticks: u64,
    pub total_lots: u64,
    pub order_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchFill {
    pub maker_account_id: u64,
    pub maker_client_order_id: u64,
    pub maker_side: OrderSide,
    pub taker_account_id: u64,
    pub taker_client_order_id: u64,
    pub taker_side: OrderSide,
    pub price_ticks: u64,
    pub quantity_lots: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchStatus {
    Filled,
    PartiallyFilledCancelled,
    PartiallyFilledResting,
    Cancelled,
    Resting,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchOutcome {
    pub status: MatchStatus,
    pub fills: Vec<MatchFill>,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub posted_order_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingleMarketBook {
    market_id: u64,
    next_sequence: u64,
    bids: Vec<RestingOrder>,
    asks: Vec<RestingOrder>,
    resting_order_keys: BTreeSet<(u64, u64)>,
    expiring_order_count: usize,
}

impl borsh::BorshSerialize for SingleMarketBook {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.market_id, writer)?;
        borsh::BorshSerialize::serialize(&self.next_sequence, writer)?;
        borsh::BorshSerialize::serialize(&self.bids, writer)?;
        borsh::BorshSerialize::serialize(&self.asks, writer)?;
        Ok(())
    }
}

impl borsh::BorshDeserialize for SingleMarketBook {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let market_id = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let next_sequence = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let bids = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let asks = borsh::BorshDeserialize::deserialize_reader(reader)?;
        let mut book = Self {
            market_id,
            next_sequence,
            bids,
            asks,
            resting_order_keys: BTreeSet::new(),
            expiring_order_count: 0,
        };
        book.rebuild_runtime_indexes();
        Ok(book)
    }
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

pub fn validate_order_request(order: &OrderRequest, now_ms: u64) -> Result<(), OrderRejectReason> {
    if order.market_id == 0 {
        return Err(OrderRejectReason::ZeroMarketId);
    }
    if order.account_id == 0 {
        return Err(OrderRejectReason::ZeroAccountId);
    }
    if order.client_order_id == 0 {
        return Err(OrderRejectReason::ZeroClientOrderId);
    }
    if order.quantity_lots == 0 {
        return Err(OrderRejectReason::ZeroQuantity);
    }
    if matches!(order.kind, OrderKind::Limit { price_ticks: 0 }) {
        return Err(OrderRejectReason::ZeroLimitPrice);
    }

    if matches!(order.kind, OrderKind::Market)
        && !matches!(order.time_in_force, TimeInForce::Ioc | TimeInForce::Fok)
    {
        return Err(OrderRejectReason::MarketOrderRequiresImmediateTif);
    }

    if order.post_only && matches!(order.kind, OrderKind::Market) {
        return Err(OrderRejectReason::PostOnlyRequiresLimitOrder);
    }

    if order.post_only && matches!(order.time_in_force, TimeInForce::Ioc | TimeInForce::Fok) {
        return Err(OrderRejectReason::PostOnlyIncompatibleWithImmediateTif);
    }

    if let TimeInForce::Gtt { expires_at_ms } = order.time_in_force {
        if expires_at_ms <= now_ms {
            return Err(OrderRejectReason::GttRequiresFutureExpiry {
                now_ms,
                expires_at_ms,
            });
        }
    }

    Ok(())
}

pub fn pre_match_check(
    order: &OrderRequest,
    preview: LiquidityPreview,
    now_ms: u64,
) -> Result<PreMatchOutcome, OrderRejectReason> {
    validate_order_request(order, now_ms)?;

    if order.post_only && preview.would_cross {
        return Err(OrderRejectReason::PostOnlyWouldCross);
    }

    if matches!(order.time_in_force, TimeInForce::Fok) && !preview.fully_fillable {
        return Err(OrderRejectReason::FillOrKillInsufficientLiquidity);
    }

    let must_execute_immediately = matches!(order.kind, OrderKind::Market)
        || matches!(order.time_in_force, TimeInForce::Ioc | TimeInForce::Fok);
    let requires_full_fill = matches!(order.time_in_force, TimeInForce::Fok);
    let may_post = matches!(order.kind, OrderKind::Limit { .. })
        && !matches!(order.time_in_force, TimeInForce::Ioc | TimeInForce::Fok);

    Ok(PreMatchOutcome {
        crosses_book: preview.would_cross,
        must_execute_immediately,
        requires_full_fill,
        may_post,
    })
}

impl SingleMarketBook {
    pub fn new(market_id: u64) -> Self {
        Self {
            market_id,
            next_sequence: 0,
            bids: Vec::new(),
            asks: Vec::new(),
            resting_order_keys: BTreeSet::new(),
            expiring_order_count: 0,
        }
    }

    pub fn market_id(&self) -> u64 {
        self.market_id
    }

    pub fn bids(&self) -> &[RestingOrder] {
        &self.bids
    }

    pub fn asks(&self) -> &[RestingOrder] {
        &self.asks
    }

    pub fn best_bid(&self, now_ms: u64) -> Option<&RestingOrder> {
        self.bids.iter().find(|order| order.is_active(now_ms))
    }

    pub fn best_ask(&self, now_ms: u64) -> Option<&RestingOrder> {
        self.asks.iter().find(|order| order.is_active(now_ms))
    }

    pub fn bid_levels(&self, now_ms: u64, depth: usize) -> Vec<BookLevel> {
        collect_levels(&self.bids, now_ms, depth)
    }

    pub fn ask_levels(&self, now_ms: u64, depth: usize) -> Vec<BookLevel> {
        collect_levels(&self.asks, now_ms, depth)
    }

    pub fn resting_order(
        &self,
        account_id: u64,
        client_order_id: u64,
        now_ms: u64,
    ) -> Option<&RestingOrder> {
        self.bids.iter().chain(self.asks.iter()).find(|order| {
            order.account_id == account_id
                && order.client_order_id == client_order_id
                && order.is_active(now_ms)
        })
    }

    pub fn account_orders(&self, account_id: u64, now_ms: u64, limit: usize) -> Vec<RestingOrder> {
        let mut orders: Vec<&RestingOrder> = self
            .bids
            .iter()
            .chain(self.asks.iter())
            .filter(|order| order.account_id == account_id && order.is_active(now_ms))
            .collect();
        orders.sort_by(|left, right| resting_order_cmp(left, right));
        orders.truncate(limit);
        orders.into_iter().cloned().collect()
    }

    pub fn preview(
        &self,
        order: &OrderRequest,
        now_ms: u64,
    ) -> Result<LiquidityPreview, OrderRejectReason> {
        self.validate_market(order)?;
        validate_order_request(order, now_ms)?;
        if self.has_active_resting_order(order.account_id, order.client_order_id, now_ms) {
            return Err(OrderRejectReason::DuplicateClientOrderId);
        }

        let opposing = self.opposing_side(order.side);
        let mut would_cross = false;
        let mut remaining = order.quantity_lots;

        for resting in opposing.iter().filter(|resting| resting.is_active(now_ms)) {
            if !order_matches_resting(order, resting) {
                break;
            }

            would_cross = true;

            if resting.account_id == order.account_id {
                return Ok(LiquidityPreview {
                    would_cross,
                    fully_fillable: false,
                });
            }

            remaining = remaining.saturating_sub(resting.remaining_lots);
            if remaining == 0 {
                return Ok(LiquidityPreview {
                    would_cross,
                    fully_fillable: true,
                });
            }
        }

        Ok(LiquidityPreview {
            would_cross,
            fully_fillable: remaining == 0,
        })
    }

    pub fn submit(
        &mut self,
        order: OrderRequest,
        now_ms: u64,
    ) -> Result<MatchOutcome, OrderRejectReason> {
        self.validate_market(&order)?;
        self.purge_expired(now_ms);

        let preview = self.preview(&order, now_ms)?;
        let pre_match = pre_match_check(&order, preview, now_ms)?;

        let mut remaining = order.quantity_lots;
        let mut fills = Vec::new();

        if preview.would_cross {
            let mut removed_resting = Vec::new();
            {
                let opposing = self.opposing_side_mut(order.side);
                let mut index = 0;

                while index < opposing.len() && remaining > 0 {
                    if !opposing[index].is_active(now_ms) {
                        removed_resting.push(opposing.remove(index));
                        continue;
                    }

                    if !order_matches_resting(&order, &opposing[index]) {
                        break;
                    }

                    if opposing[index].account_id == order.account_id {
                        return Err(OrderRejectReason::SelfTradePrevented);
                    }

                    let fill_qty = remaining.min(opposing[index].remaining_lots);
                    fills.push(MatchFill {
                        maker_account_id: opposing[index].account_id,
                        maker_client_order_id: opposing[index].client_order_id,
                        maker_side: opposing[index].side,
                        taker_account_id: order.account_id,
                        taker_client_order_id: order.client_order_id,
                        taker_side: order.side,
                        price_ticks: opposing[index].price_ticks,
                        quantity_lots: fill_qty,
                    });

                    remaining -= fill_qty;
                    opposing[index].remaining_lots -= fill_qty;

                    if opposing[index].remaining_lots == 0 {
                        removed_resting.push(opposing.remove(index));
                    } else {
                        index += 1;
                    }

                    if pre_match.requires_full_fill && remaining == 0 {
                        break;
                    }
                }
            }
            for removed in &removed_resting {
                self.untrack_resting(removed);
            }
        }

        let filled_lots = order.quantity_lots - remaining;
        let should_post = remaining > 0
            && matches!(order.kind, OrderKind::Limit { .. })
            && matches!(
                order.time_in_force,
                TimeInForce::Gtc | TimeInForce::Gtt { .. }
            );

        let posted_order_id = if should_post {
            let resting = RestingOrder {
                market_id: order.market_id,
                account_id: order.account_id,
                client_order_id: order.client_order_id,
                side: order.side,
                price_ticks: match order.kind {
                    OrderKind::Limit { price_ticks } => price_ticks,
                    OrderKind::Market => unreachable!("market orders never rest"),
                },
                remaining_lots: remaining,
                expires_at_ms: match order.time_in_force {
                    TimeInForce::Gtt { expires_at_ms } => Some(expires_at_ms),
                    _ => None,
                },
                sequence: self.allocate_sequence(),
            };
            let client_order_id = resting.client_order_id;
            self.insert_resting(resting);
            Some(client_order_id)
        } else {
            None
        };

        let status = match (filled_lots, remaining, posted_order_id.is_some()) {
            (_, 0, _) if filled_lots == order.quantity_lots => MatchStatus::Filled,
            (0, _, true) => MatchStatus::Resting,
            (0, _, false) => MatchStatus::Cancelled,
            (_, _, true) => MatchStatus::PartiallyFilledResting,
            _ => MatchStatus::PartiallyFilledCancelled,
        };

        Ok(MatchOutcome {
            status,
            fills,
            filled_lots,
            remaining_lots: remaining,
            posted_order_id,
        })
    }

    pub fn submit_resting_batch(
        &mut self,
        orders: &[OrderRequest],
        now_ms: u64,
    ) -> Result<Vec<MatchOutcome>, RestingBatchError> {
        if orders.is_empty() {
            return Ok(Vec::new());
        }

        self.purge_expired(now_ms);
        self.validate_resting_batch(orders, now_ms)?;

        let side = orders[0].side;
        let mut restings = Vec::with_capacity(orders.len());
        let mut outcomes = Vec::with_capacity(orders.len());
        let mut keys = Vec::with_capacity(orders.len());

        for order in orders {
            let price_ticks = match order.kind {
                OrderKind::Limit { price_ticks } => price_ticks,
                OrderKind::Market => unreachable!("batch validation rejects market orders"),
            };
            let expires_at_ms = match order.time_in_force {
                TimeInForce::Gtt { expires_at_ms } => Some(expires_at_ms),
                _ => None,
            };
            let sequence = self.allocate_sequence();
            restings.push(RestingOrder {
                market_id: order.market_id,
                account_id: order.account_id,
                client_order_id: order.client_order_id,
                side: order.side,
                price_ticks,
                remaining_lots: order.quantity_lots,
                expires_at_ms,
                sequence,
            });
            keys.push((order.account_id, order.client_order_id, expires_at_ms));
            outcomes.push(MatchOutcome {
                status: MatchStatus::Resting,
                fills: Vec::new(),
                filled_lots: 0,
                remaining_lots: order.quantity_lots,
                posted_order_id: Some(order.client_order_id),
            });
        }

        self.same_side_mut(side).extend(restings);
        for (account_id, client_order_id, expires_at_ms) in keys {
            self.track_resting_key(account_id, client_order_id, expires_at_ms);
        }

        Ok(outcomes)
    }

    pub fn can_submit_resting_batch(
        &self,
        orders: &[OrderRequest],
        now_ms: u64,
    ) -> Result<(), RestingBatchError> {
        self.validate_resting_batch(orders, now_ms)
    }

    pub fn purge_expired(&mut self, now_ms: u64) {
        if self.expiring_order_count == 0 {
            return;
        }
        self.bids.retain(|order| order.is_active(now_ms));
        self.asks.retain(|order| order.is_active(now_ms));
        self.rebuild_runtime_indexes();
    }

    pub fn cancel(
        &mut self,
        account_id: u64,
        client_order_id: u64,
        now_ms: u64,
    ) -> Option<RestingOrder> {
        self.purge_expired(now_ms);

        if let Some(index) = self.bids.iter().position(|order| {
            order.account_id == account_id && order.client_order_id == client_order_id
        }) {
            let removed = self.bids.remove(index);
            self.untrack_resting(&removed);
            return Some(removed);
        }

        if let Some(index) = self.asks.iter().position(|order| {
            order.account_id == account_id && order.client_order_id == client_order_id
        }) {
            let removed = self.asks.remove(index);
            self.untrack_resting(&removed);
            return Some(removed);
        }

        None
    }

    pub fn replace(
        &mut self,
        account_id: u64,
        client_order_id: u64,
        replacement: OrderRequest,
        now_ms: u64,
    ) -> Result<MatchOutcome, OrderRejectReason> {
        if replacement.account_id != account_id {
            return Err(OrderRejectReason::ReplaceAccountMismatch {
                expected: account_id,
                actual: replacement.account_id,
            });
        }

        let mut working = self.clone();
        if working
            .cancel(account_id, client_order_id, now_ms)
            .is_none()
        {
            return Err(OrderRejectReason::UnknownRestingOrder);
        }

        let outcome = working.submit(replacement, now_ms)?;
        *self = working;
        Ok(outcome)
    }

    fn validate_market(&self, order: &OrderRequest) -> Result<(), OrderRejectReason> {
        if order.market_id != self.market_id {
            return Err(OrderRejectReason::MarketMismatch {
                expected: self.market_id,
                actual: order.market_id,
            });
        }
        Ok(())
    }

    fn validate_resting_batch(
        &self,
        orders: &[OrderRequest],
        now_ms: u64,
    ) -> Result<(), RestingBatchError> {
        if orders.is_empty() {
            return Ok(());
        }

        let side = orders[0].side;
        let mut seen_keys = BTreeSet::new();
        let mut next_sequence = self.next_sequence;
        let mut previous_resting = self.same_side(side).last().cloned();

        for order in orders {
            self.validate_market(order)
                .map_err(RestingBatchError::Reject)?;
            validate_order_request(order, now_ms).map_err(RestingBatchError::Reject)?;

            if order.side != side {
                return Err(RestingBatchError::Unsupported);
            }
            if self.has_active_resting_order(order.account_id, order.client_order_id, now_ms)
                || !seen_keys.insert((order.account_id, order.client_order_id))
            {
                return Err(RestingBatchError::Reject(
                    OrderRejectReason::DuplicateClientOrderId,
                ));
            }
            if !matches!(order.kind, OrderKind::Limit { .. })
                || matches!(order.time_in_force, TimeInForce::Ioc | TimeInForce::Fok)
            {
                return Err(RestingBatchError::Unsupported);
            }

            let preview = self
                .preview(order, now_ms)
                .map_err(RestingBatchError::Reject)?;
            let pre_match =
                pre_match_check(order, preview, now_ms).map_err(RestingBatchError::Reject)?;
            if pre_match.crosses_book || pre_match.must_execute_immediately || !pre_match.may_post {
                return Err(RestingBatchError::Unsupported);
            }

            let resting = RestingOrder {
                market_id: order.market_id,
                account_id: order.account_id,
                client_order_id: order.client_order_id,
                side: order.side,
                price_ticks: match order.kind {
                    OrderKind::Limit { price_ticks } => price_ticks,
                    OrderKind::Market => return Err(RestingBatchError::Unsupported),
                },
                remaining_lots: order.quantity_lots,
                expires_at_ms: match order.time_in_force {
                    TimeInForce::Gtt { expires_at_ms } => Some(expires_at_ms),
                    _ => None,
                },
                sequence: next_sequence,
            };

            if previous_resting
                .as_ref()
                .is_some_and(|previous| resting_precedes(&resting, previous))
            {
                return Err(RestingBatchError::Unsupported);
            }

            previous_resting = Some(resting);
            next_sequence = next_sequence.saturating_add(1);
        }

        Ok(())
    }

    fn has_active_resting_order(&self, account_id: u64, client_order_id: u64, now_ms: u64) -> bool {
        if !self
            .resting_order_keys
            .contains(&(account_id, client_order_id))
        {
            return false;
        }
        if self.expiring_order_count == 0 {
            return true;
        }
        self.bids.iter().chain(self.asks.iter()).any(|order| {
            order.account_id == account_id
                && order.client_order_id == client_order_id
                && order.is_active(now_ms)
        })
    }

    fn allocate_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }

    fn insert_resting(&mut self, resting: RestingOrder) {
        let account_id = resting.account_id;
        let client_order_id = resting.client_order_id;
        let expires_at_ms = resting.expires_at_ms;
        {
            let side = self.same_side_mut(resting.side);
            if side
                .last()
                .is_none_or(|current| !resting_precedes(&resting, current))
            {
                side.push(resting);
                self.track_resting_key(account_id, client_order_id, expires_at_ms);
                return;
            }
        }
        {
            let side = self.same_side_mut(resting.side);
            let mut insert_at = side.len();
            for (index, current) in side.iter().enumerate() {
                if resting_precedes(&resting, current) {
                    insert_at = index;
                    break;
                }
            }
            side.insert(insert_at, resting);
        }
        self.track_resting_key(account_id, client_order_id, expires_at_ms);
    }

    fn same_side_mut(&mut self, side: OrderSide) -> &mut Vec<RestingOrder> {
        match side {
            OrderSide::Bid => &mut self.bids,
            OrderSide::Ask => &mut self.asks,
        }
    }

    fn same_side(&self, side: OrderSide) -> &[RestingOrder] {
        match side {
            OrderSide::Bid => &self.bids,
            OrderSide::Ask => &self.asks,
        }
    }

    fn opposing_side(&self, side: OrderSide) -> &[RestingOrder] {
        match side {
            OrderSide::Bid => &self.asks,
            OrderSide::Ask => &self.bids,
        }
    }

    fn opposing_side_mut(&mut self, side: OrderSide) -> &mut Vec<RestingOrder> {
        match side {
            OrderSide::Bid => &mut self.asks,
            OrderSide::Ask => &mut self.bids,
        }
    }

    fn track_resting_key(
        &mut self,
        account_id: u64,
        client_order_id: u64,
        expires_at_ms: Option<u64>,
    ) {
        self.resting_order_keys
            .insert((account_id, client_order_id));
        if expires_at_ms.is_some() {
            self.expiring_order_count = self.expiring_order_count.saturating_add(1);
        }
    }

    fn untrack_resting(&mut self, order: &RestingOrder) {
        self.resting_order_keys
            .remove(&(order.account_id, order.client_order_id));
        if order.expires_at_ms.is_some() {
            self.expiring_order_count = self.expiring_order_count.saturating_sub(1);
        }
    }

    fn rebuild_runtime_indexes(&mut self) {
        self.resting_order_keys.clear();
        self.expiring_order_count = 0;
        for order in self.bids.iter().chain(self.asks.iter()) {
            self.resting_order_keys
                .insert((order.account_id, order.client_order_id));
            if order.expires_at_ms.is_some() {
                self.expiring_order_count = self.expiring_order_count.saturating_add(1);
            }
        }
    }
}

impl RestingOrder {
    fn is_active(&self, now_ms: u64) -> bool {
        self.expires_at_ms
            .is_none_or(|expires_at_ms| expires_at_ms > now_ms)
    }
}

fn order_matches_resting(order: &OrderRequest, resting: &RestingOrder) -> bool {
    match (order.side, order.kind) {
        (OrderSide::Bid, OrderKind::Limit { price_ticks }) => price_ticks >= resting.price_ticks,
        (OrderSide::Ask, OrderKind::Limit { price_ticks }) => price_ticks <= resting.price_ticks,
        (_, OrderKind::Market) => true,
    }
}

fn resting_precedes(candidate: &RestingOrder, current: &RestingOrder) -> bool {
    match candidate.side {
        OrderSide::Bid => {
            candidate.price_ticks > current.price_ticks
                || (candidate.price_ticks == current.price_ticks
                    && candidate.sequence < current.sequence)
        }
        OrderSide::Ask => {
            candidate.price_ticks < current.price_ticks
                || (candidate.price_ticks == current.price_ticks
                    && candidate.sequence < current.sequence)
        }
    }
}

fn resting_order_cmp(left: &RestingOrder, right: &RestingOrder) -> std::cmp::Ordering {
    match (left.side, right.side) {
        (OrderSide::Bid, OrderSide::Ask) => std::cmp::Ordering::Less,
        (OrderSide::Ask, OrderSide::Bid) => std::cmp::Ordering::Greater,
        (OrderSide::Bid, OrderSide::Bid) => right
            .price_ticks
            .cmp(&left.price_ticks)
            .then_with(|| left.sequence.cmp(&right.sequence)),
        (OrderSide::Ask, OrderSide::Ask) => left
            .price_ticks
            .cmp(&right.price_ticks)
            .then_with(|| left.sequence.cmp(&right.sequence)),
    }
}

fn collect_levels(resting_orders: &[RestingOrder], now_ms: u64, depth: usize) -> Vec<BookLevel> {
    let mut levels: Vec<BookLevel> = Vec::new();

    for order in resting_orders
        .iter()
        .filter(|order| order.is_active(now_ms))
    {
        if let Some(level) = levels.last_mut() {
            if level.price_ticks == order.price_ticks {
                level.total_lots = level.total_lots.saturating_add(order.remaining_lots);
                level.order_count = level.order_count.saturating_add(1);
                continue;
            }
        }

        if levels.len() == depth {
            break;
        }

        levels.push(BookLevel {
            price_ticks: order.price_ticks,
            total_lots: order.remaining_lots,
            order_count: 1,
        });
    }

    levels
}

#[cfg(test)]
#[path = "matching_tests.rs"]
mod tests;
