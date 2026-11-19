use super::{
    component_name, pre_match_check, validate_order_request, BookLevel, LiquidityPreview,
    MatchFill, MatchOutcome, MatchStatus, OrderKind, OrderRejectReason, OrderRequest, OrderSide,
    PreMatchOutcome, RestingBatchError, SingleMarketBook, TimeInForce,
};

fn limit_order(side: OrderSide, tif: TimeInForce) -> OrderRequest {
    OrderRequest {
        market_id: 1,
        account_id: 11,
        client_order_id: 99,
        side,
        quantity_lots: 5,
        kind: OrderKind::Limit { price_ticks: 100 },
        time_in_force: tif,
        post_only: false,
    }
}

#[test]
fn reports_component_name() {
    assert_eq!(component_name(), "matching");
}

#[test]
fn validates_basic_order_shape() {
    let mut order = limit_order(OrderSide::Bid, TimeInForce::Gtc);
    order.market_id = 0;
    assert_eq!(
        validate_order_request(&order, 1_000),
        Err(OrderRejectReason::ZeroMarketId)
    );

    order.market_id = 1;
    order.account_id = 0;
    assert_eq!(
        validate_order_request(&order, 1_000),
        Err(OrderRejectReason::ZeroAccountId)
    );

    order.account_id = 11;
    order.client_order_id = 0;
    assert_eq!(
        validate_order_request(&order, 1_000),
        Err(OrderRejectReason::ZeroClientOrderId)
    );

    order.client_order_id = 9;
    order.quantity_lots = 0;
    assert_eq!(
        validate_order_request(&order, 1_000),
        Err(OrderRejectReason::ZeroQuantity)
    );
}

#[test]
fn market_orders_require_immediate_tif() {
    let order = OrderRequest {
        kind: OrderKind::Market,
        time_in_force: TimeInForce::Gtc,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    assert_eq!(
        validate_order_request(&order, 1_000),
        Err(OrderRejectReason::MarketOrderRequiresImmediateTif)
    );
}

#[test]
fn post_only_requires_non_immediate_limit_order() {
    let market_post_only = OrderRequest {
        kind: OrderKind::Market,
        time_in_force: TimeInForce::Ioc,
        post_only: true,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    assert_eq!(
        validate_order_request(&market_post_only, 1_000),
        Err(OrderRejectReason::PostOnlyRequiresLimitOrder)
    );

    let fok_post_only = OrderRequest {
        post_only: true,
        time_in_force: TimeInForce::Fok,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    assert_eq!(
        validate_order_request(&fok_post_only, 1_000),
        Err(OrderRejectReason::PostOnlyIncompatibleWithImmediateTif)
    );
}

#[test]
fn gtt_requires_future_expiry() {
    let order = limit_order(OrderSide::Bid, TimeInForce::Gtt { expires_at_ms: 999 });
    assert_eq!(
        validate_order_request(&order, 1_000),
        Err(OrderRejectReason::GttRequiresFutureExpiry {
            now_ms: 1_000,
            expires_at_ms: 999,
        })
    );
}

#[test]
fn post_only_rejects_crossing_orders() {
    let order = OrderRequest {
        post_only: true,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    assert_eq!(
        pre_match_check(
            &order,
            LiquidityPreview {
                would_cross: true,
                fully_fillable: true,
            },
            1_000
        ),
        Err(OrderRejectReason::PostOnlyWouldCross)
    );
}

#[test]
fn fok_requires_full_liquidity() {
    let order = limit_order(OrderSide::Bid, TimeInForce::Fok);
    assert_eq!(
        pre_match_check(
            &order,
            LiquidityPreview {
                would_cross: true,
                fully_fillable: false,
            },
            1_000
        ),
        Err(OrderRejectReason::FillOrKillInsufficientLiquidity)
    );
}

#[test]
fn gtc_limit_orders_can_cross_and_rest() {
    let order = limit_order(OrderSide::Bid, TimeInForce::Gtc);
    assert_eq!(
        pre_match_check(
            &order,
            LiquidityPreview {
                would_cross: true,
                fully_fillable: false,
            },
            1_000
        ),
        Ok(PreMatchOutcome {
            crosses_book: true,
            must_execute_immediately: false,
            requires_full_fill: false,
            may_post: true,
        })
    );
}

#[test]
fn market_orders_are_immediate_and_never_post() {
    let order = OrderRequest {
        kind: OrderKind::Market,
        time_in_force: TimeInForce::Ioc,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    assert_eq!(
        pre_match_check(
            &order,
            LiquidityPreview {
                would_cross: false,
                fully_fillable: false,
            },
            1_000
        ),
        Ok(PreMatchOutcome {
            crosses_book: false,
            must_execute_immediately: true,
            requires_full_fill: false,
            may_post: false,
        })
    );
}

#[test]
fn post_only_limit_orders_can_rest() {
    let order = OrderRequest {
        post_only: true,
        time_in_force: TimeInForce::Gtt {
            expires_at_ms: 2_000,
        },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    assert_eq!(
        pre_match_check(
            &order,
            LiquidityPreview {
                would_cross: false,
                fully_fillable: false,
            },
            1_000
        ),
        Ok(PreMatchOutcome {
            crosses_book: false,
            must_execute_immediately: false,
            requires_full_fill: false,
            may_post: true,
        })
    );
}

#[test]
fn preview_and_submit_resting_limit_order() {
    let mut book = SingleMarketBook::new(1);
    let order = limit_order(OrderSide::Bid, TimeInForce::Gtc);

    assert_eq!(
        book.preview(&order, 1_000),
        Ok(LiquidityPreview {
            would_cross: false,
            fully_fillable: false,
        })
    );
    assert_eq!(
        book.submit(order, 1_000),
        Ok(MatchOutcome {
            status: MatchStatus::Resting,
            fills: vec![],
            filled_lots: 0,
            remaining_lots: 5,
            posted_order_id: Some(99),
        })
    );
    assert_eq!(book.bids().len(), 1);
    assert!(book.asks().is_empty());
}

#[test]
fn resting_batch_matches_sequential_resting_submits() {
    let orders: Vec<OrderRequest> = (0..4)
        .map(|index| OrderRequest {
            account_id: 20 + index,
            client_order_id: 200 + index,
            side: OrderSide::Bid,
            kind: OrderKind::Limit { price_ticks: 100 },
            quantity_lots: 5 + index,
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        })
        .collect();
    let mut sequential = SingleMarketBook::new(1);
    let expected: Vec<MatchOutcome> = orders
        .iter()
        .copied()
        .map(|order| {
            sequential
                .submit(order, 1_000)
                .expect("sequential order should rest")
        })
        .collect();

    let mut batched = SingleMarketBook::new(1);
    assert_eq!(batched.submit_resting_batch(&orders, 1_000), Ok(expected));
    assert_eq!(batched, sequential);
}

#[test]
fn resting_batch_rejects_duplicate_client_order_atomically() {
    let mut book = SingleMarketBook::new(1);
    let before = book.clone();
    let orders = vec![
        OrderRequest {
            account_id: 20,
            client_order_id: 200,
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        },
        OrderRequest {
            account_id: 20,
            client_order_id: 200,
            quantity_lots: 7,
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        },
    ];

    assert_eq!(
        book.submit_resting_batch(&orders, 1_000),
        Err(RestingBatchError::Reject(
            OrderRejectReason::DuplicateClientOrderId
        ))
    );
    assert_eq!(book, before);
}

#[test]
fn resting_batch_reports_crossing_orders_as_unsupported() {
    let mut book = SingleMarketBook::new(1);
    let ask = OrderRequest {
        account_id: 22,
        client_order_id: 200,
        side: OrderSide::Ask,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    book.submit(ask, 1_000).expect("ask should rest");
    let before = book.clone();
    let bid = OrderRequest {
        account_id: 33,
        client_order_id: 300,
        side: OrderSide::Bid,
        quantity_lots: 3,
        kind: OrderKind::Limit { price_ticks: 101 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    assert_eq!(
        book.submit_resting_batch(&[bid], 1_100),
        Err(RestingBatchError::Unsupported)
    );
    assert_eq!(book, before);
}

#[test]
fn resting_batch_reports_non_append_orders_as_unsupported() {
    let mut book = SingleMarketBook::new(1);
    book.submit(
        OrderRequest {
            account_id: 20,
            client_order_id: 200,
            side: OrderSide::Bid,
            kind: OrderKind::Limit { price_ticks: 100 },
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        },
        1_000,
    )
    .expect("first bid should rest");
    let before = book.clone();
    let better_bid = OrderRequest {
        account_id: 21,
        client_order_id: 201,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 101 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    assert_eq!(
        book.submit_resting_batch(&[better_bid], 1_100),
        Err(RestingBatchError::Unsupported)
    );
    assert_eq!(book, before);
}

#[test]
fn submits_crossing_order_and_generates_fill() {
    let mut book = SingleMarketBook::new(1);
    let ask = OrderRequest {
        account_id: 22,
        client_order_id: 200,
        side: OrderSide::Ask,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    book.submit(ask, 1_000).expect("ask should rest");

    let bid = OrderRequest {
        account_id: 33,
        client_order_id: 300,
        side: OrderSide::Bid,
        quantity_lots: 3,
        kind: OrderKind::Limit { price_ticks: 101 },
        ..limit_order(OrderSide::Bid, TimeInForce::Ioc)
    };

    assert_eq!(
        book.submit(bid, 1_100),
        Ok(MatchOutcome {
            status: MatchStatus::Filled,
            fills: vec![MatchFill {
                maker_account_id: 22,
                maker_client_order_id: 200,
                maker_side: OrderSide::Ask,
                taker_account_id: 33,
                taker_client_order_id: 300,
                taker_side: OrderSide::Bid,
                price_ticks: 100,
                quantity_lots: 3,
            }],
            filled_lots: 3,
            remaining_lots: 0,
            posted_order_id: None,
        })
    );
    assert_eq!(book.asks()[0].remaining_lots, 2);
}

#[test]
fn preserves_price_time_priority_on_insert() {
    let mut book = SingleMarketBook::new(1);
    let best = OrderRequest {
        account_id: 21,
        client_order_id: 1,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 105 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let first_at_level = OrderRequest {
        account_id: 22,
        client_order_id: 2,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let second_at_level = OrderRequest {
        account_id: 23,
        client_order_id: 3,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    book.submit(first_at_level, 1_000)
        .expect("first order should rest");
    book.submit(best, 1_001).expect("best order should rest");
    book.submit(second_at_level, 1_002)
        .expect("second order should rest");

    assert_eq!(book.bids()[0].client_order_id, 1);
    assert_eq!(book.bids()[1].client_order_id, 2);
    assert_eq!(book.bids()[2].client_order_id, 3);
}

#[test]
fn self_trade_prevention_rejects_crossing_self_order() {
    let mut book = SingleMarketBook::new(1);
    let ask = OrderRequest {
        account_id: 11,
        client_order_id: 200,
        side: OrderSide::Ask,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    book.submit(ask, 1_000).expect("ask should rest");

    let bid = OrderRequest {
        account_id: 11,
        client_order_id: 201,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Ioc)
    };

    assert_eq!(
        book.submit(bid, 1_100),
        Err(OrderRejectReason::SelfTradePrevented)
    );
    assert_eq!(book.asks()[0].remaining_lots, 5);
}

#[test]
fn market_mismatch_is_rejected() {
    let book = SingleMarketBook::new(7);
    let order = limit_order(OrderSide::Bid, TimeInForce::Gtc);
    assert_eq!(
        book.preview(&order, 1_000),
        Err(OrderRejectReason::MarketMismatch {
            expected: 7,
            actual: 1,
        })
    );
}

#[test]
fn purges_expired_gtt_orders_before_matching() {
    let mut book = SingleMarketBook::new(1);
    let expiring_ask = OrderRequest {
        account_id: 22,
        client_order_id: 200,
        side: OrderSide::Ask,
        time_in_force: TimeInForce::Gtt {
            expires_at_ms: 1_050,
        },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    book.submit(expiring_ask, 1_000)
        .expect("gtt ask should rest");

    let bid = OrderRequest {
        account_id: 33,
        client_order_id: 300,
        side: OrderSide::Bid,
        kind: OrderKind::Market,
        time_in_force: TimeInForce::Ioc,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    assert_eq!(
        book.submit(bid, 1_100),
        Ok(MatchOutcome {
            status: MatchStatus::Cancelled,
            fills: vec![],
            filled_lots: 0,
            remaining_lots: 5,
            posted_order_id: None,
        })
    );
    assert!(book.asks().is_empty());
}

#[test]
fn best_levels_reflect_price_time_priority() {
    let mut book = SingleMarketBook::new(1);
    let worse_bid = OrderRequest {
        account_id: 21,
        client_order_id: 1,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 99 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let better_bid = OrderRequest {
        account_id: 22,
        client_order_id: 2,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 101 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let best_ask = OrderRequest {
        account_id: 31,
        client_order_id: 3,
        side: OrderSide::Ask,
        kind: OrderKind::Limit { price_ticks: 103 },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    let worse_ask = OrderRequest {
        account_id: 32,
        client_order_id: 4,
        side: OrderSide::Ask,
        kind: OrderKind::Limit { price_ticks: 105 },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };

    book.submit(worse_bid, 1_000)
        .expect("worse bid should rest");
    book.submit(better_bid, 1_001)
        .expect("better bid should rest");
    book.submit(worse_ask, 1_002)
        .expect("worse ask should rest");
    book.submit(best_ask, 1_003).expect("best ask should rest");

    assert_eq!(
        book.best_bid(1_100).map(|order| order.client_order_id),
        Some(2)
    );
    assert_eq!(
        book.best_ask(1_100).map(|order| order.client_order_id),
        Some(3)
    );
}

#[test]
fn best_levels_skip_expired_orders() {
    let mut book = SingleMarketBook::new(1);
    let expiring_bid = OrderRequest {
        account_id: 21,
        client_order_id: 1,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 101 },
        time_in_force: TimeInForce::Gtt {
            expires_at_ms: 1_050,
        },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let durable_bid = OrderRequest {
        account_id: 22,
        client_order_id: 2,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    book.submit(durable_bid, 1_000)
        .expect("durable bid should rest");
    book.submit(expiring_bid, 1_001)
        .expect("expiring bid should rest");

    assert_eq!(
        book.best_bid(1_010).map(|order| order.client_order_id),
        Some(1)
    );
    assert_eq!(
        book.best_bid(1_100).map(|order| order.client_order_id),
        Some(2)
    );
}

#[test]
fn cancel_removes_resting_order_and_promotes_next_best_level() {
    let mut book = SingleMarketBook::new(1);
    let best_bid = OrderRequest {
        account_id: 21,
        client_order_id: 1,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 101 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let next_bid = OrderRequest {
        account_id: 22,
        client_order_id: 2,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    book.submit(next_bid, 1_000).expect("next bid should rest");
    book.submit(best_bid, 1_001).expect("best bid should rest");

    let cancelled = book.cancel(21, 1, 1_100).expect("order should cancel");
    assert_eq!(cancelled.account_id, 21);
    assert_eq!(cancelled.client_order_id, 1);
    assert_eq!(
        book.best_bid(1_100).map(|order| order.client_order_id),
        Some(2)
    );
}

#[test]
fn cancel_is_account_scoped() {
    let mut book = SingleMarketBook::new(1);
    let ask = OrderRequest {
        account_id: 22,
        client_order_id: 200,
        side: OrderSide::Ask,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    book.submit(ask, 1_000).expect("ask should rest");

    assert!(book.cancel(99, 200, 1_100).is_none());
    assert_eq!(
        book.best_ask(1_100).map(|order| order.client_order_id),
        Some(200)
    );
}

#[test]
fn aggregates_book_levels_by_price() {
    let mut book = SingleMarketBook::new(1);
    for (account_id, client_order_id) in [(21, 1), (22, 2)] {
        let bid = OrderRequest {
            account_id,
            client_order_id,
            side: OrderSide::Bid,
            quantity_lots: 3,
            kind: OrderKind::Limit { price_ticks: 101 },
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        };
        book.submit(bid, 1_000 + client_order_id)
            .expect("bid should rest");
    }

    let lower_bid = OrderRequest {
        account_id: 23,
        client_order_id: 3,
        side: OrderSide::Bid,
        quantity_lots: 4,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    book.submit(lower_bid, 1_010)
        .expect("lower bid should rest");

    assert_eq!(
        book.bid_levels(1_100, 10),
        vec![
            BookLevel {
                price_ticks: 101,
                total_lots: 6,
                order_count: 2,
            },
            BookLevel {
                price_ticks: 100,
                total_lots: 4,
                order_count: 1,
            },
        ]
    );
}

#[test]
fn book_levels_skip_expired_orders_and_respect_depth() {
    let mut book = SingleMarketBook::new(1);
    let expired_ask = OrderRequest {
        account_id: 31,
        client_order_id: 1,
        side: OrderSide::Ask,
        quantity_lots: 2,
        kind: OrderKind::Limit { price_ticks: 103 },
        time_in_force: TimeInForce::Gtt {
            expires_at_ms: 1_050,
        },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    let best_live_ask = OrderRequest {
        account_id: 32,
        client_order_id: 2,
        side: OrderSide::Ask,
        quantity_lots: 5,
        kind: OrderKind::Limit { price_ticks: 104 },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };
    let next_live_ask = OrderRequest {
        account_id: 33,
        client_order_id: 3,
        side: OrderSide::Ask,
        quantity_lots: 7,
        kind: OrderKind::Limit { price_ticks: 105 },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };

    book.submit(best_live_ask, 1_000)
        .expect("best live ask should rest");
    book.submit(expired_ask, 1_001)
        .expect("expired ask should rest");
    book.submit(next_live_ask, 1_002)
        .expect("next live ask should rest");

    assert_eq!(
        book.ask_levels(1_100, 1),
        vec![BookLevel {
            price_ticks: 104,
            total_lots: 5,
            order_count: 1,
        }]
    );
}

#[test]
fn duplicate_client_order_id_is_rejected_while_resting() {
    let mut book = SingleMarketBook::new(1);
    let first = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Bid,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let duplicate = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Ask,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };

    book.submit(first, 1_000).expect("first order should rest");
    assert_eq!(
        book.preview(&duplicate, 1_100),
        Err(OrderRejectReason::DuplicateClientOrderId)
    );
    assert_eq!(
        book.submit(duplicate, 1_100),
        Err(OrderRejectReason::DuplicateClientOrderId)
    );
}

#[test]
fn cancelled_order_id_can_be_reused() {
    let mut book = SingleMarketBook::new(1);
    let first = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Bid,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let replacement = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Ask,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };

    book.submit(first, 1_000).expect("first order should rest");
    book.cancel(22, 500, 1_050).expect("order should cancel");

    assert_eq!(
        book.submit(replacement, 1_100),
        Ok(MatchOutcome {
            status: MatchStatus::Resting,
            fills: vec![],
            filled_lots: 0,
            remaining_lots: 5,
            posted_order_id: Some(500),
        })
    );
}

#[test]
fn replace_updates_resting_order_atomically() {
    let mut book = SingleMarketBook::new(1);
    let original = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    book.submit(original, 1_000)
        .expect("original order should rest");

    let replacement = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 102 },
        quantity_lots: 7,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    assert_eq!(
        book.replace(22, 500, replacement, 1_100),
        Ok(MatchOutcome {
            status: MatchStatus::Resting,
            fills: vec![],
            filled_lots: 0,
            remaining_lots: 7,
            posted_order_id: Some(500),
        })
    );
    let best_bid = book.best_bid(1_100).expect("replacement should rest");
    assert_eq!(best_bid.price_ticks, 102);
    assert_eq!(best_bid.remaining_lots, 7);
}

#[test]
fn failed_replace_keeps_original_order_live() {
    let mut book = SingleMarketBook::new(1);
    let resting_bid = OrderRequest {
        account_id: 33,
        client_order_id: 600,
        side: OrderSide::Bid,
        kind: OrderKind::Limit { price_ticks: 100 },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    let original_ask = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Ask,
        kind: OrderKind::Limit { price_ticks: 102 },
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };

    book.submit(resting_bid, 1_000).expect("bid should rest");
    book.submit(original_ask, 1_001)
        .expect("original ask should rest");

    let invalid_replacement = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Ask,
        kind: OrderKind::Limit { price_ticks: 100 },
        post_only: true,
        ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
    };

    assert_eq!(
        book.replace(22, 500, invalid_replacement, 1_100),
        Err(OrderRejectReason::PostOnlyWouldCross)
    );

    let best_ask = book.best_ask(1_100).expect("original ask should remain");
    assert_eq!(best_ask.client_order_id, 500);
    assert_eq!(best_ask.price_ticks, 102);
    assert_eq!(
        book.best_bid(1_100).map(|order| order.client_order_id),
        Some(600)
    );
}

#[test]
fn replace_rejects_missing_resting_order() {
    let mut book = SingleMarketBook::new(1);
    let replacement = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Bid,
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };

    assert_eq!(
        book.replace(22, 500, replacement, 1_100),
        Err(OrderRejectReason::UnknownRestingOrder)
    );
}

#[test]
fn resting_order_lookup_skips_expired_entries() {
    let mut book = SingleMarketBook::new(1);
    let expiring = OrderRequest {
        account_id: 22,
        client_order_id: 500,
        side: OrderSide::Bid,
        time_in_force: TimeInForce::Gtt {
            expires_at_ms: 1_050,
        },
        ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
    };
    book.submit(expiring, 1_000).expect("order should rest");

    assert!(book.resting_order(22, 500, 1_010).is_some());
    assert!(book.resting_order(22, 500, 1_100).is_none());
}

#[test]
fn account_orders_are_returned_in_book_priority() {
    let mut book = SingleMarketBook::new(1);
    let orders = [
        OrderRequest {
            account_id: 22,
            client_order_id: 1,
            side: OrderSide::Ask,
            kind: OrderKind::Limit { price_ticks: 105 },
            ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
        },
        OrderRequest {
            account_id: 22,
            client_order_id: 2,
            side: OrderSide::Bid,
            kind: OrderKind::Limit { price_ticks: 101 },
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        },
        OrderRequest {
            account_id: 22,
            client_order_id: 3,
            side: OrderSide::Bid,
            kind: OrderKind::Limit { price_ticks: 100 },
            ..limit_order(OrderSide::Bid, TimeInForce::Gtc)
        },
        OrderRequest {
            account_id: 22,
            client_order_id: 4,
            side: OrderSide::Ask,
            kind: OrderKind::Limit { price_ticks: 103 },
            ..limit_order(OrderSide::Ask, TimeInForce::Gtc)
        },
    ];

    for (index, order) in orders.into_iter().enumerate() {
        book.submit(order, 1_000 + index as u64)
            .expect("order should rest");
    }

    assert_eq!(
        book.account_orders(22, 1_100, 3)
            .into_iter()
            .map(|order| order.client_order_id)
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
}
