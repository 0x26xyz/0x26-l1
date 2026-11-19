use super::{
    ambiguous_submit_intent_id, delegated_lane_policy_base_id, is_unknown_resting_order_error,
    lane_recovery_action, lane_refresh_due, lane_refresh_phase, lanes_share_policy,
    next_rotated_lane_policy_id, pending_intent_unknown_expired, pending_unknown_retry_deadline_ms,
    post_only_safe_price_ticks, price_move_bps, trading_identity_from_components, Cli, Lane,
    LaneRecoveryAction, MarketMakerIdentityParams, PendingLaneIntent, QuoteSlot, TradingIdentity,
};
use and_chain_sdk::{
    MarketMakerIdentity, OrderSide, PolicyExecutionScopeResponse, PolicyResponse, RpcError,
    SubmitError,
};
use clap::Parser;
use serde_json::json;

#[test]
fn trading_identity_without_delegation_uses_naked_account() {
    let identity = trading_identity_from_components(260_001, None);
    assert_eq!(
        identity,
        TradingIdentity::Account {
            account_id: 260_001
        }
    );
    assert_eq!(identity.signer_account_id(), 260_001);
    assert_eq!(identity.trading_account_id(), 260_001);
    assert_eq!(
        identity.market_maker_identity(),
        MarketMakerIdentity::Account(260_001)
    );
    assert!(matches!(
        identity.market_maker_identity_params(),
        MarketMakerIdentityParams::Account { account_id } if account_id == 260_001
    ));
}

#[test]
fn detects_unknown_resting_order_from_rpc_payloads() {
    assert!(is_unknown_resting_order_error(
        "rpc error -32020: intent submission rejected data={\"order_rejection_reason\":\"unknown_resting_order\"}"
    ));
    assert!(is_unknown_resting_order_error(
        "intent 0xabc terminal stage=rejected rejection=Some(\"internal_error\") order_rejection=Some(\"unknown_resting_order\")"
    ));
    assert!(!is_unknown_resting_order_error(
        "rpc error -32020: intent submission rejected data={\"order_rejection_reason\":\"post_only_would_cross\"}"
    ));
}

#[test]
fn local_submit_timeout_extracts_ambiguous_pending_intent_id() {
    let error = SubmitError::Rpc(RpcError::JsonRpc {
        code: -32000,
        message: "local submit admission timed out".to_owned(),
        data: Some(json!({
            "kind": "intent_submission",
            "reason": "local_submit_admission_timeout",
            "intent_id_hex": "0xabc123",
        })),
    });

    assert_eq!(
        ambiguous_submit_intent_id(&error).as_deref(),
        Some("0xabc123")
    );
}

#[test]
fn duplicate_policy_nonce_is_not_treated_as_ambiguous_submit() {
    let error = SubmitError::Rpc(RpcError::JsonRpc {
        code: -32020,
        message: "intent submission rejected".to_owned(),
        data: Some(json!({
            "kind": "intent_submission",
            "reason": "duplicate_policy_nonce",
            "intent_id_hex": "0xabc123",
            "policy_id": 42,
            "nonce": 7,
        })),
    });

    assert_eq!(ambiguous_submit_intent_id(&error), None);
}

#[test]
fn submit_admission_pending_unknown_retry_deadline_is_bounded() {
    assert!(pending_unknown_retry_deadline_ms("submit_admission", 500) > 0);
    assert_eq!(pending_unknown_retry_deadline_ms("accepted_direct", 500), 0);
}

#[test]
fn unknown_pending_expiry_only_applies_to_ambiguous_submit_tracking() {
    let expired = PendingLaneIntent {
        intent_id_hex: "0xabc".to_owned(),
        submitted_nonce: 7,
        replacement_client_order_id: 99,
        unknown_retry_deadline_ms: 1,
    };
    let durable = PendingLaneIntent {
        intent_id_hex: "0xdef".to_owned(),
        submitted_nonce: 8,
        replacement_client_order_id: 100,
        unknown_retry_deadline_ms: 0,
    };

    assert!(pending_intent_unknown_expired(&expired));
    assert!(!pending_intent_unknown_expired(&durable));
}

#[test]
fn quote_refresh_staggers_existing_lanes_by_cycle() {
    let cli = Cli::parse_from([
        "kombat_mm_bot",
        "--rpc-url",
        "http://localhost:8545",
        "--maker-account-id",
        "260001",
        "--maker-owner-secret-hex",
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "--maker-session-secret-hex",
        "0x0202020202020202020202020202020202020202020202020202020202020202",
        "--maker-policy-base-id",
        "26000101",
        "--market-ids",
        "7",
        "--quote-refresh-every-cycles",
        "4",
    ]);
    let lane = Lane {
        slot: QuoteSlot {
            market_id: 7,
            side: OrderSide::Bid,
            level: 0,
        },
        policy_id: 42,
        policy_id_stride: 100,
        next_nonce: 1,
        current_client_order_id: Some(777),
        pending_intent: None,
        last_refresh_cycle: 0,
        last_refresh_mark_price_ticks: None,
    };
    let phase = lane_refresh_phase(&lane, cli.quote_refresh_every_cycles);

    assert!(lane_refresh_due(&lane, &cli, phase, 1_000_000));
    assert!(!lane_refresh_due(
        &lane,
        &cli,
        (phase + 1) % cli.quote_refresh_every_cycles,
        1_000_000
    ));
}

#[test]
fn quote_refresh_forces_update_on_price_move() {
    let cli = Cli::parse_from([
        "kombat_mm_bot",
        "--rpc-url",
        "http://localhost:8545",
        "--maker-account-id",
        "260001",
        "--maker-owner-secret-hex",
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "--maker-session-secret-hex",
        "0x0202020202020202020202020202020202020202020202020202020202020202",
        "--maker-policy-base-id",
        "26000101",
        "--market-ids",
        "7",
        "--quote-refresh-every-cycles",
        "4",
        "--quote-refresh-price-move-bps",
        "2",
    ]);
    let lane = Lane {
        slot: QuoteSlot {
            market_id: 7,
            side: OrderSide::Bid,
            level: 0,
        },
        policy_id: 42,
        policy_id_stride: 100,
        next_nonce: 1,
        current_client_order_id: Some(777),
        pending_intent: None,
        last_refresh_cycle: 0,
        last_refresh_mark_price_ticks: Some(1_000_000),
    };
    let off_phase = (lane_refresh_phase(&lane, cli.quote_refresh_every_cycles) + 1)
        % cli.quote_refresh_every_cycles;

    assert_eq!(price_move_bps(1_000_000, 1_000_100), 1);
    assert!(lane_refresh_due(&lane, &cli, off_phase, 1_000_200));
    assert!(!lane_refresh_due(&lane, &cli, off_phase, 1_000_100));
}

#[test]
fn post_only_clamp_preserves_ladder_levels_when_raw_bid_would_cross() {
    let best_ask = 11_465_385;
    let level0 = post_only_safe_price_ticks(
        best_ask + 100_000,
        OrderSide::Bid,
        None,
        Some(best_ask),
        5,
        0,
    );
    let level1 = post_only_safe_price_ticks(
        best_ask + 100_000,
        OrderSide::Bid,
        None,
        Some(best_ask),
        5,
        1,
    );
    let level2 = post_only_safe_price_ticks(
        best_ask + 100_000,
        OrderSide::Bid,
        None,
        Some(best_ask),
        5,
        2,
    );

    assert_eq!(level0, best_ask - 1);
    assert!(level1 < level0);
    assert!(level2 < level1);
}

#[test]
fn post_only_clamp_preserves_ladder_levels_when_raw_ask_would_cross() {
    let best_bid = 11_465_384;
    let level0 = post_only_safe_price_ticks(
        best_bid - 100_000,
        OrderSide::Ask,
        Some(best_bid),
        None,
        5,
        0,
    );
    let level1 = post_only_safe_price_ticks(
        best_bid - 100_000,
        OrderSide::Ask,
        Some(best_bid),
        None,
        5,
        1,
    );
    let level2 = post_only_safe_price_ticks(
        best_bid - 100_000,
        OrderSide::Ask,
        Some(best_bid),
        None,
        5,
        2,
    );

    assert_eq!(level0, best_bid + 1);
    assert!(level1 > level0);
    assert!(level2 > level1);
}

#[test]
fn lanes_share_policy_only_when_all_lane_policy_ids_match() {
    let mut lanes = vec![
        Lane {
            slot: QuoteSlot {
                market_id: 7,
                side: OrderSide::Bid,
                level: 0,
            },
            policy_id: 42,
            policy_id_stride: 2,
            next_nonce: 1,
            current_client_order_id: None,
            pending_intent: None,
            last_refresh_cycle: 0,
            last_refresh_mark_price_ticks: None,
        },
        Lane {
            slot: QuoteSlot {
                market_id: 7,
                side: OrderSide::Ask,
                level: 0,
            },
            policy_id: 42,
            policy_id_stride: 2,
            next_nonce: 1,
            current_client_order_id: None,
            pending_intent: None,
            last_refresh_cycle: 0,
            last_refresh_mark_price_ticks: None,
        },
    ];
    assert!(lanes_share_policy(&lanes));
    lanes[1].policy_id = 43;
    assert!(!lanes_share_policy(&lanes));
}

#[test]
fn delegated_lane_policy_base_avoids_naked_agent_policy_namespace() {
    let cli = Cli::parse_from([
        "kombat_mm_bot",
        "--rpc-url",
        "http://localhost:8545",
        "--maker-account-id",
        "260001",
        "--delegation-id-hex",
        "0xabababababababababababababababababababababababababababababababab",
        "--maker-owner-secret-hex",
        "0x0101010101010101010101010101010101010101010101010101010101010101",
        "--maker-session-secret-hex",
        "0x0202020202020202020202020202020202020202020202020202020202020202",
        "--maker-policy-base-id",
        "26000101",
        "--market-ids",
        "7",
    ]);
    let identity = trading_identity_from_components(260_001, Some([0xAB; 32]));
    assert_ne!(
        delegated_lane_policy_base_id(identity, &cli),
        cli.maker_policy_base_id
    );
}

#[test]
fn trading_identity_with_delegation_routes_orders_to_trading_account() {
    let delegation_id = [0xAB; 32];
    let identity = trading_identity_from_components(260_001, Some(delegation_id));
    let TradingIdentity::Delegation {
        serving_agent,
        trading_account_id,
        ..
    } = identity
    else {
        panic!("expected delegation trading identity");
    };
    assert_eq!(serving_agent, 260_001);
    assert_ne!(trading_account_id, 260_001);
    assert_eq!(trading_account_id >> 63, 1);
    assert_eq!(
        identity.market_maker_identity(),
        MarketMakerIdentity::Delegation {
            delegation_id,
            serving_agent: 260_001,
        }
    );
    assert!(matches!(
        identity.market_maker_identity_params(),
        MarketMakerIdentityParams::Delegation {
            delegation_id_hex,
            serving_agent
        } if delegation_id_hex == format!("0x{}", "ab".repeat(32)) && serving_agent == 260_001
    ));
}

#[test]
fn delegated_lane_policy_match_uses_execution_account_surface() {
    let policy = PolicyResponse {
        id: 43,
        authority_account: 260_001,
        account: 260_001,
        execution_account_id: 9_999,
        execution_scope: PolicyExecutionScopeResponse {
            kind: "delegation_trading".to_owned(),
            delegation_id_hex: Some(format!("0x{}", "ab".repeat(32))),
        },
        parent_policy_id: Some(42),
        state: "active".to_owned(),
        next_intent_nonce: 1,
        delegation_depth: 1,
        revoked_at: None,
    };

    assert!(super::delegation_lane_policy_matches(&policy, 9_999, 42));
    assert!(!super::delegation_lane_policy_matches(&policy, 260_001, 42));
    assert!(!super::delegation_lane_policy_matches(&policy, 9_999, 41));
}

#[test]
fn daily_volume_rejection_requests_delegated_lane_rotation() {
    let lane = Lane {
        slot: QuoteSlot {
            market_id: 7,
            side: OrderSide::Bid,
            level: 0,
        },
        policy_id: 42,
        policy_id_stride: 100,
        next_nonce: 1,
        current_client_order_id: None,
        pending_intent: None,
        last_refresh_cycle: 0,
        last_refresh_mark_price_ticks: None,
    };
    let identity = trading_identity_from_components(260_001, Some([0xAB; 32]));

    assert_eq!(
        lane_recovery_action(
            &lane,
            identity,
            "intent 0xabc terminal stage=rejected rejection=Some(\"daily_volume_exceeded\") order_rejection=None"
        ),
        Some(LaneRecoveryAction::RotateDelegationPolicy)
    );
}

#[test]
fn naked_lane_does_not_rotate_on_daily_volume_rejection() {
    let lane = Lane {
        slot: QuoteSlot {
            market_id: 7,
            side: OrderSide::Bid,
            level: 0,
        },
        policy_id: 42,
        policy_id_stride: 100,
        next_nonce: 1,
        current_client_order_id: None,
        pending_intent: None,
        last_refresh_cycle: 0,
        last_refresh_mark_price_ticks: None,
    };
    let identity = trading_identity_from_components(260_001, None);

    assert_eq!(
        lane_recovery_action(
            &lane,
            identity,
            "intent 0xabc terminal stage=rejected rejection=Some(\"daily_volume_exceeded\") order_rejection=None"
        ),
        None
    );
}

#[test]
fn rotated_lane_policy_ids_advance_by_stride() {
    assert_eq!(next_rotated_lane_policy_id(42, 100).unwrap(), 142);
}
