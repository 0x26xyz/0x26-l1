use borsh::BorshDeserialize;

use super::{
    accept_owner_request, add_policy_request, cancel_intent_request, cancel_order_intent_request,
    collateral_intent_request, create_account_request, create_policy_request,
    create_sub_policy_intent_request, derive_delegation_action_id_response,
    derive_market_maker_program_action_id_response, encode_delegation_action_response,
    encode_market_maker_program_action_response, pause_account_request, place_order_intent_request,
    propose_owner_request, remove_policy_request, replace_order_intent_request,
    revoke_policy_intent_request, revoke_policy_owner_call_request,
    signed_delegation_action_request, signed_market_maker_program_action_request,
    transfer_intent_request, unpause_account_request, upgrade_account_request, CancelIntentArgs,
    CancelOrderIntentArgs, CollateralIntentArgs, CreatePolicyArgs, CreateSubPolicyIntentArgs,
    DelegationActionCommand, MarketMakerProgramActionCommand, PlaceOrderIntentArgs,
    ReplaceOrderIntentArgs, RevokePolicyIntentArgs, SignedDelegationActionArgs,
    SignedMarketMakerProgramActionArgs, TransferIntentArgs,
};
use kombat_chain_consensus::{
    BlockDelegationAction, BlockMarketMakerProgramAction, MarketMakerIdentity,
};
use kombat_chain_node::{
    delegation_action_id_for_submission, delegation_action_signing_bytes,
    market_maker_program_action_id_for_submission, market_maker_program_action_signing_bytes,
    MarketMakerU256, SignedDelegationAction, SignedMarketMakerProgramAction,
};
use std::collections::BTreeSet;

#[test]
fn create_account_request_matches_expected_shape() {
    let request = create_account_request(
        11,
        "0808080808080808080808080808080808080808080808080808080808080808",
        "0909090909090909090909090909090909090909090909090909090909090909",
        1_000,
        1,
    );

    assert_eq!(request["method"], "kombat_createAccount");
    assert_eq!(request["params"]["account_id"], 11);
    assert_eq!(
        request["params"]["owner_hex"],
        "0x1398f62c6d1a457c51ba6a4b5f3dbd2f69fca93216218dc8997e416bd17d93ca"
    );
}

#[test]
fn create_policy_request_matches_demo_signature() {
    let request = create_policy_request(CreatePolicyArgs {
        owner_secret_hex: "0808080808080808080808080808080808080808080808080808080808080808",
        grantee_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        chain_id: 7,
        nonce: 0,
        policy_id: 42,
        expires_at: 50_000,
        timestamp_ms: 1_050,
        gas_budget_per_day: "100",
        gas_refill_per_day: "100",
        max_intents_per_second: 2,
        max_intents_per_day: 10,
        max_volume_per_day: "1000",
        max_drawdown_pct: 25,
        request_id: 2,
    });

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["signature_hex"],
        "0xd96df2f4c53537be99f62288a58e318415918ed577586d2ece101582f9e3ecba2d0400685eef956642ab6d5a02dbace5751d2dca18476b10c8ebe217490cb806"
    );
}

#[test]
fn propose_owner_request_generates_owner_call_shape() {
    let request = propose_owner_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        "0909090909090909090909090909090909090909090909090909090909090909",
        11,
        7,
        1,
        1_060,
        7,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "propose_owner"
    );
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["new_owner_hex"],
        "0xfd1724385aa0c75b64fb78cd602fa1d991fdebf76b13c58ed702eac835e9f618"
    );
}

#[test]
fn pause_account_request_generates_owner_call_shape() {
    let request = pause_account_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        11,
        7,
        2,
        1_070,
        9,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "pause"
    );
    assert_eq!(request["params"]["signed_owner_call"]["call"]["nonce"], 2);
}

#[test]
fn unpause_account_request_generates_owner_call_shape() {
    let request = unpause_account_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        11,
        7,
        3,
        1_080,
        10,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "unpause"
    );
    assert_eq!(request["params"]["signed_owner_call"]["call"]["nonce"], 3);
}

#[test]
fn accept_owner_request_generates_owner_call_shape() {
    let request = accept_owner_request(
        "0909090909090909090909090909090909090909090909090909090909090909",
        11,
        7,
        3,
        1_080,
        8,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "accept_owner"
    );
}

#[test]
fn add_policy_request_generates_owner_call_shape() {
    let request = add_policy_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        11,
        7,
        4,
        42,
        1_090,
        11,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "add_policy"
    );
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["policy_id"],
        42
    );
}

#[test]
fn remove_policy_request_generates_owner_call_shape() {
    let request = remove_policy_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        11,
        7,
        5,
        42,
        1_100,
        12,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "remove_policy"
    );
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["policy_id"],
        42
    );
}

#[test]
fn revoke_policy_owner_call_request_generates_owner_call_shape() {
    let request = revoke_policy_owner_call_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        11,
        7,
        6,
        42,
        1_110,
        13,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "revoke_policy"
    );
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["policy_id"],
        42
    );
}

#[test]
fn upgrade_account_request_generates_owner_call_shape() {
    let request = upgrade_account_request(
        "0808080808080808080808080808080808080808080808080808080808080808",
        11,
        7,
        5,
        "0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b",
        1_100,
        14,
    );

    assert_eq!(request["method"], "kombat_submitOwnerCall");
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["kind"],
        "upgrade"
    );
    assert_eq!(
        request["params"]["signed_owner_call"]["call"]["code_hash_hex"],
        "0x0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b"
    );
}

#[test]
fn cancel_intent_request_matches_demo_signature() {
    let request = cancel_intent_request(CancelIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 1,
        expires_at: 10_000,
        time_after_ms: None,
        time_before_ms: None,
        order_id: 7,
        timestamp_ms: 1_100,
        request_id: 3,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["signature_hex"],
        "0xa9450bf7ac03bbfcd7c01133e422c359250cd65797387313c2719bb011cb20533218edf384f4bf2068740efc399f3f10bf5ca953eaf641656c1fb4d275621c0a"
    );
}

#[test]
fn transfer_intent_request_generates_transfer_shape() {
    let request = transfer_intent_request(TransferIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 2,
        expires_at: 20_000,
        time_after_ms: None,
        time_before_ms: None,
        to: 17,
        token: 1,
        amount: 250,
        timestamp_ms: 1_150,
        request_id: 4,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "transfer"
    );
    assert_eq!(request["params"]["signed_intent"]["intent"]["to"], 17);
    assert_eq!(request["params"]["signed_intent"]["intent"]["amount"], 250);
}

#[test]
fn place_order_intent_request_generates_order_shape() {
    let request = place_order_intent_request(PlaceOrderIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 3,
        expires_at: 20_000,
        time_after_ms: None,
        time_before_ms: None,
        market_id: 7,
        client_order_id: 99,
        side: "ask",
        quantity_lots: 5,
        order_kind: "limit",
        price_ticks: Some(103),
        tif: "gtc",
        tif_expires_at_ms: None,
        post_only: false,
        timestamp_ms: 1_160,
        request_id: 15,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "place_order"
    );
    assert_eq!(request["params"]["signed_intent"]["intent"]["market_id"], 7);
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["client_order_id"],
        99
    );
    assert_eq!(request["params"]["signed_intent"]["intent"]["side"], "ask");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["price_ticks"],
        103
    );
}

#[test]
fn cancel_order_intent_request_generates_cancel_order_shape() {
    let request = cancel_order_intent_request(CancelOrderIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 4,
        expires_at: 20_000,
        time_after_ms: None,
        time_before_ms: None,
        market_id: 7,
        client_order_id: 99,
        timestamp_ms: 1_170,
        request_id: 16,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "cancel_order"
    );
    assert_eq!(request["params"]["signed_intent"]["intent"]["market_id"], 7);
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["client_order_id"],
        99
    );
}

#[test]
fn replace_order_intent_request_generates_replace_shape() {
    let request = replace_order_intent_request(ReplaceOrderIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 5,
        expires_at: 20_000,
        time_after_ms: None,
        time_before_ms: None,
        market_id: 7,
        target_client_order_id: 99,
        replacement_client_order_id: 100,
        side: "ask",
        quantity_lots: 7,
        order_kind: "limit",
        price_ticks: Some(102),
        tif: "gtc",
        tif_expires_at_ms: None,
        post_only: false,
        timestamp_ms: 1_180,
        request_id: 17,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "replace_order"
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["target_client_order_id"],
        99
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["replacement_client_order_id"],
        100
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["price_ticks"],
        102
    );
}

#[test]
fn create_sub_policy_intent_request_generates_child_policy_shape() {
    let request = create_sub_policy_intent_request(CreateSubPolicyIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        child_grantee_secret_hex:
            "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 3,
        expires_at: 30_000,
        time_after_ms: None,
        time_before_ms: None,
        child_policy_id: 77,
        child_expires_at: 40_000,
        timestamp_ms: 1_200,
        gas_budget_per_day: "100",
        gas_refill_per_day: "100",
        max_intents_per_second: 2,
        max_intents_per_day: 10,
        max_volume_per_day: "1000",
        max_drawdown_pct: 25,
        request_id: 5,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "create_sub_policy"
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["child_policy_id"],
        77
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["child_terms"]["max_volume_per_day"],
        "1000"
    );
}

#[test]
fn revoke_policy_intent_request_generates_revoke_shape() {
    let request = revoke_policy_intent_request(RevokePolicyIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 4,
        expires_at: 40_000,
        time_after_ms: None,
        time_before_ms: None,
        target_policy_id: 77,
        timestamp_ms: 1_250,
        request_id: 6,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "revoke_policy"
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["target_policy_id"],
        77
    );
}

#[test]
fn collateral_intent_request_generates_collateral_shape() {
    let request = collateral_intent_request(CollateralIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 6,
        expires_at: 50_000,
        time_after_ms: None,
        time_before_ms: None,
        market_id: 7,
        amount_usdc: 1_000,
        timestamp_ms: 1_260,
        request_id: 18,
        withdraw: false,
    });

    assert_eq!(request["method"], "kombat_submitIntent");
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["kind"],
        "deposit_collateral"
    );
    assert_eq!(request["params"]["signed_intent"]["intent"]["market_id"], 7);
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["amount_usdc"],
        1_000
    );
}

#[test]
fn encode_delegation_action_generates_canonical_borsh_for_all_variants() {
    let delegation_id_hex = super::encode_hex(&[8; 32]);
    let mandate_id_hex = super::encode_hex(&[7; 32]);
    let request_id_hex = super::encode_hex(&[9; 32]);
    let cases = vec![
        (
            DelegationActionCommand::Open {
                agent: 22,
                mandate_id_hex: mandate_id_hex.clone(),
                performance_share_units: 1_000,
                crystallization_cadence_ms: 604_800_000,
                notice_period_ms: 86_400_000,
                agent_bond: 5_000_000_000,
            },
            BlockDelegationAction::Open {
                agent: 22,
                mandate_id: [7; 32],
                performance_share_units: 1_000,
                crystallization_cadence_ms: 604_800_000,
                notice_period_ms: 86_400_000,
                agent_bond: 5_000_000_000,
            },
            "open",
        ),
        (
            DelegationActionCommand::Deposit {
                principal: 33,
                delegation_id_hex: delegation_id_hex.clone(),
                amount: 100_000_000,
            },
            BlockDelegationAction::Deposit {
                principal: 33,
                delegation_id: [8; 32],
                amount: 100_000_000,
            },
            "deposit",
        ),
        (
            DelegationActionCommand::RequestRedemption {
                principal: 33,
                delegation_id_hex: delegation_id_hex.clone(),
                shares: 50_000_000,
            },
            BlockDelegationAction::RequestRedemption {
                principal: 33,
                delegation_id: [8; 32],
                shares: 50_000_000,
            },
            "request_redemption",
        ),
        (
            DelegationActionCommand::FinalizeRedemption {
                request_id_hex: request_id_hex.clone(),
            },
            BlockDelegationAction::FinalizeRedemption {
                request_id: [9; 32],
            },
            "finalize_redemption",
        ),
        (
            DelegationActionCommand::CancelRedemption {
                principal: 33,
                request_id_hex: request_id_hex.clone(),
            },
            BlockDelegationAction::CancelRedemption {
                principal: 33,
                request_id: [9; 32],
            },
            "cancel_redemption",
        ),
        (
            DelegationActionCommand::CloseToDeposits {
                delegation_id_hex: delegation_id_hex.clone(),
            },
            BlockDelegationAction::CloseToDeposits {
                delegation_id: [8; 32],
            },
            "close_to_deposits",
        ),
        (
            DelegationActionCommand::BeginWinding {
                delegation_id_hex: delegation_id_hex.clone(),
            },
            BlockDelegationAction::BeginWinding {
                delegation_id: [8; 32],
            },
            "begin_winding",
        ),
        (
            DelegationActionCommand::Close { delegation_id_hex },
            BlockDelegationAction::Close {
                delegation_id: [8; 32],
            },
            "close",
        ),
    ];

    for (command, expected_action, expected_kind) in cases {
        let response = encode_delegation_action_response(command);
        let action_hex = response["action_hex"]
            .as_str()
            .expect("action hex should be a string");
        let action_bytes = super::decode_hex_bytes(action_hex, "action hex");
        let decoded_action = BlockDelegationAction::try_from_slice(&action_bytes)
            .expect("encoded action should decode");

        assert_eq!(response["action_kind"], expected_kind);
        assert_eq!(decoded_action, expected_action);
        assert_eq!(
            action_hex,
            super::encode_hex(
                &borsh::to_vec(&expected_action)
                    .expect("expected delegation action should serialize")
            )
        );
    }
}

#[test]
fn derive_delegation_action_id_response_supports_legacy_and_nonce_ids() {
    let action = BlockDelegationAction::Deposit {
        principal: 33,
        delegation_id: [8; 32],
        amount: 100_000_000,
    };
    let action_hex =
        super::encode_hex(&borsh::to_vec(&action).expect("delegation action should serialize"));
    let timestamp_ms = 1_050;

    for submitter_nonce in [None, Some(42)] {
        let response =
            derive_delegation_action_id_response(&action_hex, timestamp_ms, submitter_nonce);
        let action_id = delegation_action_id_for_submission(&action, timestamp_ms, submitter_nonce);

        assert_eq!(response["action_kind"], "deposit");
        assert_eq!(response["action_id_hex"], super::encode_hex(&action_id));
        assert_eq!(response["action_hex"], action_hex);
        assert_eq!(response["timestamp_ms"], timestamp_ms);
        match submitter_nonce {
            Some(nonce) => assert_eq!(response["submitter_nonce"], nonce),
            None => assert_eq!(response["submitter_nonce"], serde_json::Value::Null),
        }
    }
}

#[test]
fn signed_delegation_action_request_generates_signed_submit_shape() {
    let actions = vec![
        BlockDelegationAction::Open {
            agent: 22,
            mandate_id: [7; 32],
            performance_share_units: 1_000,
            crystallization_cadence_ms: 604_800_000,
            notice_period_ms: 86_400_000,
            agent_bond: 5_000_000_000,
        },
        BlockDelegationAction::Deposit {
            principal: 33,
            delegation_id: [8; 32],
            amount: 100_000_000,
        },
        BlockDelegationAction::RequestRedemption {
            principal: 33,
            delegation_id: [8; 32],
            shares: 50_000_000,
        },
        BlockDelegationAction::FinalizeRedemption {
            request_id: [9; 32],
        },
        BlockDelegationAction::CancelRedemption {
            principal: 33,
            request_id: [9; 32],
        },
        BlockDelegationAction::CloseToDeposits {
            delegation_id: [8; 32],
        },
        BlockDelegationAction::BeginWinding {
            delegation_id: [8; 32],
        },
        BlockDelegationAction::Close {
            delegation_id: [8; 32],
        },
    ];

    for (index, action) in actions.into_iter().enumerate() {
        for (nonce_case, submitter_nonce) in [None, Some(42 + index as u64)].into_iter().enumerate()
        {
            let timestamp_ms = 1_050 + index as u64;
            let request_id = 20 + (index as u64 * 2) + nonce_case as u64;
            let action_hex = super::encode_hex(
                &borsh::to_vec(&action).expect("delegation action should serialize"),
            );
            let request = signed_delegation_action_request(SignedDelegationActionArgs {
                signer_secret_hex:
                    "1616161616161616161616161616161616161616161616161616161616161616",
                signer_account_id: 22,
                action_hex: &action_hex,
                chain_id: 7,
                timestamp_ms,
                submitter_nonce,
                wait_timeout_ms: Some(1_000),
                request_id,
            });
            let action_id =
                delegation_action_id_for_submission(&action, timestamp_ms, submitter_nonce);
            let emitted_action_hex = request["params"]["signed_delegation_action"]["action_hex"]
                .as_str()
                .expect("action hex should be a string");
            let emitted_action_bytes = super::decode_hex_bytes(emitted_action_hex, "action hex");
            let emitted_action = BlockDelegationAction::try_from_slice(&emitted_action_bytes)
                .expect("emitted action should decode");
            let signature = super::decode_hex_array::<64>(
                request["params"]["signed_delegation_action"]["signature_hex"]
                    .as_str()
                    .expect("signature should be a string"),
                "signature",
            );
            let signed_action = SignedDelegationAction {
                chain_id: 7,
                signer_account_id: 22,
                action_id,
                timestamp_ms,
                submitter_nonce,
                action: emitted_action.clone(),
                signature,
            };

            assert_eq!(emitted_action, action);
            assert_eq!(request["method"], "kombat_submitSignedDelegationAction");
            assert_eq!(request["id"], request_id);
            assert_eq!(
                request["params"]["signed_delegation_action"]["action_id_hex"],
                super::encode_hex(&action_id)
            );
            assert_eq!(
                request["params"]["signed_delegation_action"]["action_hex"],
                action_hex
            );
            match submitter_nonce {
                Some(nonce) => assert_eq!(
                    request["params"]["signed_delegation_action"]["submitter_nonce"],
                    nonce
                ),
                None => assert_eq!(
                    request["params"]["signed_delegation_action"]["submitter_nonce"],
                    serde_json::Value::Null
                ),
            }
            assert_eq!(request["params"]["wait_timeout_ms"], 1_000);
            super::signing_key_from_secret_hex(
                "1616161616161616161616161616161616161616161616161616161616161616",
            )
            .verifying_key()
            .verify_strict(
                &delegation_action_signing_bytes(&signed_action),
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .expect("signature should verify against the RFC 026 payload");
        }
    }
}

#[test]
fn encode_market_maker_program_action_generates_canonical_borsh_for_all_variants() {
    let program_id_hex = super::encode_hex(&[7; 32]);
    let cases = vec![
        (
            MarketMakerProgramActionCommand::Define {
                name: "prime-mm".into(),
                markets: vec![7, 8],
                min_uptime_bps: 9_000,
                max_spread_bps: 250,
                min_quote_size_micro_usdc: 1_000_000,
                min_two_sided_windows_per_hour: 12,
                rebate_multiplier_units: 20_000,
                enrollment_bond_micro_usdc: 10_000_000_000,
                compliance_window_ms: 3_600_000,
                observation_cadence_ms: 60_000,
            },
            BlockMarketMakerProgramAction::Define {
                name: "prime-mm".into(),
                markets: BTreeSet::from([7, 8]),
                min_uptime_bps: 9_000,
                max_spread_bps: 250,
                min_quote_size_micro_usdc: MarketMakerU256::from_u128(1_000_000).0,
                min_two_sided_windows_per_hour: 12,
                rebate_multiplier_units: 20_000,
                enrollment_bond_micro_usdc: MarketMakerU256::from_u128(10_000_000_000).0,
                compliance_window_ms: 3_600_000,
                observation_cadence_ms: 60_000,
            },
            "define",
        ),
        (
            MarketMakerProgramActionCommand::Retire {
                program_id_hex: program_id_hex.clone(),
            },
            BlockMarketMakerProgramAction::Retire {
                program_id: [7; 32],
            },
            "retire",
        ),
        (
            MarketMakerProgramActionCommand::Enroll {
                market_maker: Some(31),
                market_maker_identity_account: None,
                market_maker_identity_delegation_id_hex: None,
                market_maker_identity_serving_agent: None,
                market_id: 7,
                program_id_hex: program_id_hex.clone(),
                bond_micro_usdc: 10_000_000_000,
            },
            BlockMarketMakerProgramAction::Enroll {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: [7; 32],
                bond_micro_usdc: MarketMakerU256::from_u128(10_000_000_000).0,
            },
            "enroll",
        ),
        (
            MarketMakerProgramActionCommand::Exit {
                market_maker: Some(31),
                market_maker_identity_account: None,
                market_maker_identity_delegation_id_hex: None,
                market_maker_identity_serving_agent: None,
                market_id: 7,
                program_id_hex: program_id_hex.clone(),
            },
            BlockMarketMakerProgramAction::Exit {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: [7; 32],
            },
            "exit",
        ),
        (
            MarketMakerProgramActionCommand::Enroll {
                market_maker: None,
                market_maker_identity_account: None,
                market_maker_identity_delegation_id_hex: Some("08".repeat(32)),
                market_maker_identity_serving_agent: Some(31),
                market_id: 7,
                program_id_hex: program_id_hex.clone(),
                bond_micro_usdc: 10_000_000_000,
            },
            BlockMarketMakerProgramAction::Enroll {
                market_maker_identity: MarketMakerIdentity::Delegation {
                    delegation_id: [8; 32],
                    serving_agent: 31,
                },
                market_id: 7,
                program_id: [7; 32],
                bond_micro_usdc: MarketMakerU256::from_u128(10_000_000_000).0,
            },
            "enroll",
        ),
        (
            MarketMakerProgramActionCommand::Exit {
                market_maker: None,
                market_maker_identity_account: None,
                market_maker_identity_delegation_id_hex: Some("08".repeat(32)),
                market_maker_identity_serving_agent: Some(31),
                market_id: 7,
                program_id_hex,
            },
            BlockMarketMakerProgramAction::Exit {
                market_maker_identity: MarketMakerIdentity::Delegation {
                    delegation_id: [8; 32],
                    serving_agent: 31,
                },
                market_id: 7,
                program_id: [7; 32],
            },
            "exit",
        ),
    ];

    for (command, expected_action, expected_kind) in cases {
        let response = encode_market_maker_program_action_response(command);
        let action_hex = response["action_hex"]
            .as_str()
            .expect("action hex should be a string");
        let action_bytes = super::decode_hex_bytes(action_hex, "action hex");
        let decoded_action = BlockMarketMakerProgramAction::try_from_slice(&action_bytes)
            .expect("encoded action should decode");

        assert_eq!(response["action_kind"], expected_kind);
        assert_eq!(decoded_action, expected_action);
        assert_eq!(
            action_hex,
            super::encode_hex(
                &borsh::to_vec(&expected_action)
                    .expect("expected market-maker program action should serialize")
            )
        );
    }
}

#[test]
fn market_maker_program_action_id_and_signing_requests_support_all_variants() {
    let actions = vec![
        BlockMarketMakerProgramAction::Define {
            name: "prime-mm".into(),
            markets: BTreeSet::from([7]),
            min_uptime_bps: 9_000,
            max_spread_bps: 250,
            min_quote_size_micro_usdc: MarketMakerU256::from_u128(1_000_000).0,
            min_two_sided_windows_per_hour: 12,
            rebate_multiplier_units: 20_000,
            enrollment_bond_micro_usdc: MarketMakerU256::from_u128(10_000_000_000).0,
            compliance_window_ms: 3_600_000,
            observation_cadence_ms: 60_000,
        },
        BlockMarketMakerProgramAction::Retire {
            program_id: [7; 32],
        },
        BlockMarketMakerProgramAction::Enroll {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id: [7; 32],
            bond_micro_usdc: MarketMakerU256::from_u128(10_000_000_000).0,
        },
        BlockMarketMakerProgramAction::Exit {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id: [7; 32],
        },
    ];

    for (index, action) in actions.into_iter().enumerate() {
        for (nonce_case, submitter_nonce) in [None, Some(90 + index as u64)].into_iter().enumerate()
        {
            let timestamp_ms = 2_050 + index as u64;
            let request_id = 40 + (index as u64 * 2) + nonce_case as u64;
            let action_hex = super::encode_hex(
                &borsh::to_vec(&action).expect("market-maker program action should serialize"),
            );
            let id_response = derive_market_maker_program_action_id_response(
                &action_hex,
                timestamp_ms,
                submitter_nonce,
            );
            let action_id = market_maker_program_action_id_for_submission(
                &action,
                timestamp_ms,
                submitter_nonce,
            );
            assert_eq!(id_response["action_id_hex"], super::encode_hex(&action_id));
            assert_eq!(id_response["action_hex"], action_hex);

            let request =
                signed_market_maker_program_action_request(SignedMarketMakerProgramActionArgs {
                    signer_secret_hex:
                        "1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f",
                    signer_account_id: 31,
                    action_hex: &action_hex,
                    chain_id: 7,
                    timestamp_ms,
                    submitter_nonce,
                    wait_timeout_ms: Some(1_000),
                    request_id,
                });
            let emitted_action_hex = request["params"]["signed_market_maker_program_action"]
                ["action_hex"]
                .as_str()
                .expect("action hex should be a string");
            let emitted_action_bytes = super::decode_hex_bytes(emitted_action_hex, "action hex");
            let emitted_action =
                BlockMarketMakerProgramAction::try_from_slice(&emitted_action_bytes)
                    .expect("emitted action should decode");
            let signature = super::decode_hex_array::<64>(
                request["params"]["signed_market_maker_program_action"]["signature_hex"]
                    .as_str()
                    .expect("signature should be a string"),
                "signature",
            );
            let signed_action = SignedMarketMakerProgramAction {
                chain_id: 7,
                signer_account_id: 31,
                action_id,
                timestamp_ms,
                submitter_nonce,
                action: emitted_action.clone(),
                signature,
            };

            assert_eq!(emitted_action, action);
            assert_eq!(
                request["method"],
                "kombat_submitSignedMarketMakerProgramAction"
            );
            assert_eq!(request["id"], request_id);
            assert_eq!(
                request["params"]["signed_market_maker_program_action"]["action_id_hex"],
                super::encode_hex(&action_id)
            );
            assert_eq!(
                request["params"]["signed_market_maker_program_action"]["action_hex"],
                action_hex
            );
            assert_eq!(request["params"]["wait_timeout_ms"], 1_000);
            super::signing_key_from_secret_hex(
                "1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f1f",
            )
            .verifying_key()
            .verify_strict(
                &market_maker_program_action_signing_bytes(&signed_action),
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .expect("signature should verify against the RFC 027 payload");
        }
    }
}

#[test]
fn transfer_intent_request_supports_time_after_predicate() {
    let request = transfer_intent_request(TransferIntentArgs {
        session_secret_hex: "0909090909090909090909090909090909090909090909090909090909090909",
        account_id: 11,
        policy_id: 42,
        chain_id: 7,
        nonce: 5,
        expires_at: 50_000,
        time_after_ms: Some(12_345),
        time_before_ms: None,
        to: 17,
        token: 1,
        amount: 250,
        timestamp_ms: 1_300,
        request_id: 7,
    });

    assert_eq!(
        request["params"]["signed_intent"]["intent"]["predicate"]["predicate_kind"],
        "time_after"
    );
    assert_eq!(
        request["params"]["signed_intent"]["intent"]["predicate"]["not_before_ms"],
        12_345
    );
}
