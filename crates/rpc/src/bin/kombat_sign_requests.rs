use super::*;

pub(crate) struct CreatePolicyArgs<'a> {
    pub(crate) owner_secret_hex: &'a str,
    pub(crate) grantee_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) policy_id: u64,
    pub(crate) expires_at: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) gas_budget_per_day: &'a str,
    pub(crate) gas_refill_per_day: &'a str,
    pub(crate) max_intents_per_second: u32,
    pub(crate) max_intents_per_day: u32,
    pub(crate) max_volume_per_day: &'a str,
    pub(crate) max_drawdown_pct: u16,
    pub(crate) request_id: u64,
}

pub(crate) struct CancelIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) order_id: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
}

pub(crate) struct PlaceOrderIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) market_id: u64,
    pub(crate) client_order_id: u64,
    pub(crate) side: &'a str,
    pub(crate) quantity_lots: u64,
    pub(crate) order_kind: &'a str,
    pub(crate) price_ticks: Option<u64>,
    pub(crate) tif: &'a str,
    pub(crate) tif_expires_at_ms: Option<u64>,
    pub(crate) post_only: bool,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
}

pub(crate) struct CancelOrderIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) market_id: u64,
    pub(crate) client_order_id: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
}

pub(crate) struct ReplaceOrderIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) market_id: u64,
    pub(crate) target_client_order_id: u64,
    pub(crate) replacement_client_order_id: u64,
    pub(crate) side: &'a str,
    pub(crate) quantity_lots: u64,
    pub(crate) order_kind: &'a str,
    pub(crate) price_ticks: Option<u64>,
    pub(crate) tif: &'a str,
    pub(crate) tif_expires_at_ms: Option<u64>,
    pub(crate) post_only: bool,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
}

pub(crate) struct TransferIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) to: u64,
    pub(crate) token: u32,
    pub(crate) amount: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
}

pub(crate) struct CreateSubPolicyIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) child_grantee_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) child_policy_id: u64,
    pub(crate) child_expires_at: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) gas_budget_per_day: &'a str,
    pub(crate) gas_refill_per_day: &'a str,
    pub(crate) max_intents_per_second: u32,
    pub(crate) max_intents_per_day: u32,
    pub(crate) max_volume_per_day: &'a str,
    pub(crate) max_drawdown_pct: u16,
    pub(crate) request_id: u64,
}

pub(crate) struct RevokePolicyIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) target_policy_id: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
}

pub(crate) struct CollateralIntentArgs<'a> {
    pub(crate) session_secret_hex: &'a str,
    pub(crate) account_id: u64,
    pub(crate) policy_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) nonce: u64,
    pub(crate) expires_at: u64,
    pub(crate) time_after_ms: Option<u64>,
    pub(crate) time_before_ms: Option<u64>,
    pub(crate) market_id: u64,
    pub(crate) amount_usdc: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) request_id: u64,
    pub(crate) withdraw: bool,
}

pub(crate) struct SignedDelegationActionArgs<'a> {
    pub(crate) signer_secret_hex: &'a str,
    pub(crate) signer_account_id: u64,
    pub(crate) action_hex: &'a str,
    pub(crate) chain_id: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) submitter_nonce: Option<u64>,
    pub(crate) wait_timeout_ms: Option<u64>,
    pub(crate) request_id: u64,
}

pub(crate) struct SignedMarketMakerProgramActionArgs<'a> {
    pub(crate) signer_secret_hex: &'a str,
    pub(crate) signer_account_id: u64,
    pub(crate) action_hex: &'a str,
    pub(crate) chain_id: u64,
    pub(crate) timestamp_ms: u64,
    pub(crate) submitter_nonce: Option<u64>,
    pub(crate) wait_timeout_ms: Option<u64>,
    pub(crate) request_id: u64,
}

pub(crate) fn create_account_request(
    account_id: u64,
    owner_secret_hex: &str,
    code_hash_hex: &str,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    json_rpc_request(
        "kombat_createAccount",
        CreateAccountParams {
            account_id,
            owner_hex: encode_hex(owner.verifying_key().as_bytes()),
            code_hash_hex: normalize_hex(code_hash_hex),
            evm_address_hex: None,
            timestamp_ms,
        },
        request_id,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn owner_call_request(
    signer: &SigningKey,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    action: OwnerAction,
    action_json: OwnerActionJson,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner_call = OwnerCall {
        signer: signer.verifying_key().to_bytes(),
        nonce,
        action,
    };
    let signing_bytes =
        to_vec(&(account_id, chain_id, &owner_call)).expect("owner call payload should serialize");
    let signature_hex = encode_hex(&signer.sign(&signing_bytes).to_bytes());

    json_rpc_request(
        "kombat_submitOwnerCall",
        SubmitOwnerCallParams {
            signed_owner_call: SignedOwnerCallJson {
                account_id,
                chain_id,
                call: OwnerCallJson {
                    signer_hex: encode_hex(signer.verifying_key().as_bytes()),
                    nonce,
                    action: action_json,
                },
                signature_hex,
            },
            timestamp_ms,
        },
        request_id,
    )
}

pub(crate) fn create_policy_request(args: CreatePolicyArgs<'_>) -> Value {
    let owner = signing_key_from_secret_hex(args.owner_secret_hex);
    let grantee = signing_key_from_secret_hex(args.grantee_secret_hex);
    let terms_json = policy_terms_wire_json(
        args.gas_budget_per_day,
        args.gas_refill_per_day,
        args.max_intents_per_second,
        args.max_intents_per_day,
        args.max_volume_per_day,
        args.max_drawdown_pct,
    );
    let owner_action = OwnerAction::CreatePolicy {
        policy_id: args.policy_id,
        grantee: grantee.verifying_key().to_bytes(),
        expires_at: args.expires_at,
        terms: policy_terms_from_wire(&terms_json),
    };
    owner_call_request(
        &owner,
        args.account_id,
        args.chain_id,
        args.nonce,
        owner_action,
        OwnerActionJson::CreatePolicy {
            policy_id: args.policy_id,
            grantee_hex: encode_hex(grantee.verifying_key().as_bytes()),
            expires_at: args.expires_at,
            terms: terms_json,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn propose_owner_request(
    owner_secret_hex: &str,
    new_owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    let new_owner = signing_key_from_secret_hex(new_owner_secret_hex);
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::ProposeOwner {
            new_owner: new_owner.verifying_key().to_bytes(),
        },
        OwnerActionJson::ProposeOwner {
            new_owner_hex: encode_hex(new_owner.verifying_key().as_bytes()),
        },
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn accept_owner_request(
    pending_owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let pending_owner = signing_key_from_secret_hex(pending_owner_secret_hex);
    owner_call_request(
        &pending_owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::AcceptOwner,
        OwnerActionJson::AcceptOwner,
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn pause_account_request(
    owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::Pause,
        OwnerActionJson::Pause,
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn unpause_account_request(
    owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::Unpause,
        OwnerActionJson::Unpause,
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn add_policy_request(
    owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    policy_id: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::AddPolicy { policy_id },
        OwnerActionJson::AddPolicy { policy_id },
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn remove_policy_request(
    owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    policy_id: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::RemovePolicy { policy_id },
        OwnerActionJson::RemovePolicy { policy_id },
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn revoke_policy_owner_call_request(
    owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    policy_id: u64,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::RevokePolicy { policy_id },
        OwnerActionJson::RevokePolicy { policy_id },
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn upgrade_account_request(
    owner_secret_hex: &str,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    code_hash_hex: &str,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let owner = signing_key_from_secret_hex(owner_secret_hex);
    let code_hash = decode_hex_array::<32>(code_hash_hex, "code hash");
    owner_call_request(
        &owner,
        account_id,
        chain_id,
        nonce,
        OwnerAction::Upgrade { code_hash },
        OwnerActionJson::Upgrade {
            code_hash_hex: encode_hex(&code_hash),
        },
        timestamp_ms,
        request_id,
    )
}

pub(crate) fn cancel_intent_request(args: CancelIntentArgs<'_>) -> Value {
    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::Cancel {
            order_id: args.order_id,
        },
        IntentKindJson::Cancel {
            order_id: args.order_id,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn place_order_intent_request(args: PlaceOrderIntentArgs<'_>) -> Value {
    let side = match args.side {
        "bid" => IntentOrderSide::Bid,
        "ask" => IntentOrderSide::Ask,
        _ => panic!("side must be bid or ask"),
    };
    let order_kind = match args.order_kind {
        "limit" => IntentOrderKind::Limit {
            price_ticks: args
                .price_ticks
                .expect("price_ticks required for limit order"),
        },
        "market" => IntentOrderKind::Market,
        _ => panic!("order_kind must be limit or market"),
    };
    let time_in_force = match args.tif {
        "gtc" => IntentTimeInForce::Gtc,
        "ioc" => IntentTimeInForce::Ioc,
        "fok" => IntentTimeInForce::Fok,
        "gtt" => IntentTimeInForce::Gtt {
            expires_at_ms: args
                .tif_expires_at_ms
                .expect("tif_expires_at_ms required for gtt"),
        },
        _ => panic!("tif must be gtc, ioc, fok, or gtt"),
    };

    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::PlaceOrder {
            market_id: args.market_id,
            client_order_id: args.client_order_id,
            side: side.clone(),
            quantity_lots: args.quantity_lots,
            kind: order_kind.clone(),
            time_in_force: time_in_force.clone(),
            post_only: args.post_only,
        },
        IntentKindJson::PlaceOrder {
            market_id: args.market_id,
            client_order_id: args.client_order_id,
            side: args.side.to_string(),
            quantity_lots: args.quantity_lots,
            order_kind: match order_kind {
                IntentOrderKind::Limit { price_ticks } => OrderKindJson::Limit { price_ticks },
                IntentOrderKind::Market => OrderKindJson::Market,
            },
            time_in_force: match time_in_force {
                IntentTimeInForce::Gtc => TimeInForceJson::Gtc,
                IntentTimeInForce::Ioc => TimeInForceJson::Ioc,
                IntentTimeInForce::Fok => TimeInForceJson::Fok,
                IntentTimeInForce::Gtt { expires_at_ms } => TimeInForceJson::Gtt { expires_at_ms },
            },
            post_only: args.post_only,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn cancel_order_intent_request(args: CancelOrderIntentArgs<'_>) -> Value {
    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::CancelOrder {
            market_id: args.market_id,
            client_order_id: args.client_order_id,
        },
        IntentKindJson::CancelOrder {
            market_id: args.market_id,
            client_order_id: args.client_order_id,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn replace_order_intent_request(args: ReplaceOrderIntentArgs<'_>) -> Value {
    let side = match args.side {
        "bid" => IntentOrderSide::Bid,
        "ask" => IntentOrderSide::Ask,
        _ => panic!("side must be bid or ask"),
    };
    let order_kind = match args.order_kind {
        "limit" => IntentOrderKind::Limit {
            price_ticks: args
                .price_ticks
                .expect("price_ticks required for limit order"),
        },
        "market" => IntentOrderKind::Market,
        _ => panic!("order_kind must be limit or market"),
    };
    let time_in_force = match args.tif {
        "gtc" => IntentTimeInForce::Gtc,
        "ioc" => IntentTimeInForce::Ioc,
        "fok" => IntentTimeInForce::Fok,
        "gtt" => IntentTimeInForce::Gtt {
            expires_at_ms: args
                .tif_expires_at_ms
                .expect("tif_expires_at_ms required for gtt"),
        },
        _ => panic!("tif must be gtc, ioc, fok, or gtt"),
    };

    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::ReplaceOrder {
            market_id: args.market_id,
            target_client_order_id: args.target_client_order_id,
            replacement_client_order_id: args.replacement_client_order_id,
            side: side.clone(),
            quantity_lots: args.quantity_lots,
            kind: order_kind.clone(),
            time_in_force: time_in_force.clone(),
            post_only: args.post_only,
        },
        IntentKindJson::ReplaceOrder {
            market_id: args.market_id,
            target_client_order_id: args.target_client_order_id,
            replacement_client_order_id: args.replacement_client_order_id,
            side: args.side.to_string(),
            quantity_lots: args.quantity_lots,
            order_kind: match order_kind {
                IntentOrderKind::Limit { price_ticks } => OrderKindJson::Limit { price_ticks },
                IntentOrderKind::Market => OrderKindJson::Market,
            },
            time_in_force: match time_in_force {
                IntentTimeInForce::Gtc => TimeInForceJson::Gtc,
                IntentTimeInForce::Ioc => TimeInForceJson::Ioc,
                IntentTimeInForce::Fok => TimeInForceJson::Fok,
                IntentTimeInForce::Gtt { expires_at_ms } => TimeInForceJson::Gtt { expires_at_ms },
            },
            post_only: args.post_only,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn transfer_intent_request(args: TransferIntentArgs<'_>) -> Value {
    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::Transfer {
            to: args.to,
            token: args.token,
            amount: args.amount,
        },
        IntentKindJson::Transfer {
            to: args.to,
            token: args.token,
            amount: args.amount,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn create_sub_policy_intent_request(args: CreateSubPolicyIntentArgs<'_>) -> Value {
    let child_grantee = signing_key_from_secret_hex(args.child_grantee_secret_hex);
    let child_terms = policy_terms_wire_json(
        args.gas_budget_per_day,
        args.gas_refill_per_day,
        args.max_intents_per_second,
        args.max_intents_per_day,
        args.max_volume_per_day,
        args.max_drawdown_pct,
    );
    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::CreateSubPolicy {
            child_policy_id: args.child_policy_id,
            child_grantee: child_grantee.verifying_key().to_bytes(),
            child_expires_at: args.child_expires_at,
            child_terms: kombat_chain_consensus::PolicyTermsWire {
                gas_budget_per_day: child_terms.gas_budget_per_day,
                gas_refill_per_day: child_terms.gas_refill_per_day,
                max_intents_per_second: child_terms.max_intents_per_second,
                max_intents_per_day: child_terms.max_intents_per_day,
                max_volume_per_day: child_terms.max_volume_per_day,
                max_drawdown_pct: child_terms.max_drawdown_pct,
            },
        },
        IntentKindJson::CreateSubPolicy {
            child_policy_id: args.child_policy_id,
            child_grantee_hex: encode_hex(child_grantee.verifying_key().as_bytes()),
            child_expires_at: args.child_expires_at,
            child_terms,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn revoke_policy_intent_request(args: RevokePolicyIntentArgs<'_>) -> Value {
    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        IntentKind::RevokePolicy {
            target_policy_id: args.target_policy_id,
        },
        IntentKindJson::RevokePolicy {
            target_policy_id: args.target_policy_id,
        },
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn collateral_intent_request(args: CollateralIntentArgs<'_>) -> Value {
    let kind = if args.withdraw {
        IntentKind::WithdrawCollateral {
            market_id: args.market_id,
            amount_usdc: args.amount_usdc,
        }
    } else {
        IntentKind::DepositCollateral {
            market_id: args.market_id,
            amount_usdc: args.amount_usdc,
        }
    };
    let kind_json = if args.withdraw {
        IntentKindJson::WithdrawCollateral {
            market_id: args.market_id,
            amount_usdc: args.amount_usdc,
        }
    } else {
        IntentKindJson::DepositCollateral {
            market_id: args.market_id,
            amount_usdc: args.amount_usdc,
        }
    };

    signed_intent_request(
        args.session_secret_hex,
        args.account_id,
        args.policy_id,
        args.chain_id,
        args.nonce,
        args.expires_at,
        intent_predicate_pair(args.time_after_ms, args.time_before_ms),
        kind,
        kind_json,
        args.timestamp_ms,
        args.request_id,
    )
}

pub(crate) fn encode_delegation_action_response(command: DelegationActionCommand) -> Value {
    let action = delegation_action_from_command(command);
    json!({
        "action_kind": delegation_block_action_kind(&action),
        "action_hex": encode_hex(&to_vec(&action).expect("delegation action should encode")),
    })
}

pub(crate) fn delegation_action_from_command(
    command: DelegationActionCommand,
) -> BlockDelegationAction {
    match command {
        DelegationActionCommand::Open {
            agent,
            mandate_id_hex,
            performance_share_units,
            crystallization_cadence_ms,
            notice_period_ms,
            agent_bond,
        } => BlockDelegationAction::Open {
            agent,
            mandate_id: decode_hex_array::<32>(&mandate_id_hex, "mandate id"),
            performance_share_units,
            crystallization_cadence_ms,
            notice_period_ms,
            agent_bond,
        },
        DelegationActionCommand::Deposit {
            principal,
            delegation_id_hex,
            amount,
        } => BlockDelegationAction::Deposit {
            principal,
            delegation_id: decode_hex_array::<32>(&delegation_id_hex, "delegation id"),
            amount,
        },
        DelegationActionCommand::RequestRedemption {
            principal,
            delegation_id_hex,
            shares,
        } => BlockDelegationAction::RequestRedemption {
            principal,
            delegation_id: decode_hex_array::<32>(&delegation_id_hex, "delegation id"),
            shares,
        },
        DelegationActionCommand::FinalizeRedemption { request_id_hex } => {
            BlockDelegationAction::FinalizeRedemption {
                request_id: decode_hex_array::<32>(&request_id_hex, "redemption request id"),
            }
        }
        DelegationActionCommand::CancelRedemption {
            principal,
            request_id_hex,
        } => BlockDelegationAction::CancelRedemption {
            principal,
            request_id: decode_hex_array::<32>(&request_id_hex, "redemption request id"),
        },
        DelegationActionCommand::CloseToDeposits { delegation_id_hex } => {
            BlockDelegationAction::CloseToDeposits {
                delegation_id: decode_hex_array::<32>(&delegation_id_hex, "delegation id"),
            }
        }
        DelegationActionCommand::BeginWinding { delegation_id_hex } => {
            BlockDelegationAction::BeginWinding {
                delegation_id: decode_hex_array::<32>(&delegation_id_hex, "delegation id"),
            }
        }
        DelegationActionCommand::Close { delegation_id_hex } => BlockDelegationAction::Close {
            delegation_id: decode_hex_array::<32>(&delegation_id_hex, "delegation id"),
        },
    }
}

pub(crate) fn encode_market_maker_program_action_response(
    command: MarketMakerProgramActionCommand,
) -> Value {
    let action = market_maker_program_action_from_command(command);
    json!({
        "action_kind": market_maker_program_block_action_kind(&action),
        "action_hex": encode_hex(&to_vec(&action).expect("market-maker program action should encode")),
    })
}

pub(crate) fn market_maker_program_action_from_command(
    command: MarketMakerProgramActionCommand,
) -> BlockMarketMakerProgramAction {
    match command {
        MarketMakerProgramActionCommand::Define {
            name,
            markets,
            min_uptime_bps,
            max_spread_bps,
            min_quote_size_micro_usdc,
            min_two_sided_windows_per_hour,
            rebate_multiplier_units,
            enrollment_bond_micro_usdc,
            compliance_window_ms,
            observation_cadence_ms,
        } => BlockMarketMakerProgramAction::Define {
            name,
            markets: markets.into_iter().collect::<BTreeSet<_>>(),
            min_uptime_bps,
            max_spread_bps,
            min_quote_size_micro_usdc: MarketMakerU256::from_u128(min_quote_size_micro_usdc).0,
            min_two_sided_windows_per_hour,
            rebate_multiplier_units,
            enrollment_bond_micro_usdc: MarketMakerU256::from_u128(enrollment_bond_micro_usdc).0,
            compliance_window_ms,
            observation_cadence_ms,
        },
        MarketMakerProgramActionCommand::Retire { program_id_hex } => {
            BlockMarketMakerProgramAction::Retire {
                program_id: decode_hex_array::<32>(&program_id_hex, "program id"),
            }
        }
        MarketMakerProgramActionCommand::Enroll {
            market_maker,
            market_maker_identity_account,
            market_maker_identity_delegation_id_hex,
            market_maker_identity_serving_agent,
            market_id,
            program_id_hex,
            bond_micro_usdc,
        } => BlockMarketMakerProgramAction::Enroll {
            market_maker_identity: market_maker_identity_from_args(
                market_maker,
                market_maker_identity_account,
                market_maker_identity_delegation_id_hex,
                market_maker_identity_serving_agent,
            ),
            market_id,
            program_id: decode_hex_array::<32>(&program_id_hex, "program id"),
            bond_micro_usdc: MarketMakerU256::from_u128(bond_micro_usdc).0,
        },
        MarketMakerProgramActionCommand::Exit {
            market_maker,
            market_maker_identity_account,
            market_maker_identity_delegation_id_hex,
            market_maker_identity_serving_agent,
            market_id,
            program_id_hex,
        } => BlockMarketMakerProgramAction::Exit {
            market_maker_identity: market_maker_identity_from_args(
                market_maker,
                market_maker_identity_account,
                market_maker_identity_delegation_id_hex,
                market_maker_identity_serving_agent,
            ),
            market_id,
            program_id: decode_hex_array::<32>(&program_id_hex, "program id"),
        },
    }
}

pub(crate) fn market_maker_identity_from_args(
    legacy_market_maker: Option<u64>,
    market_maker_identity_account: Option<u64>,
    market_maker_identity_delegation_id_hex: Option<String>,
    market_maker_identity_serving_agent: Option<u64>,
) -> MarketMakerIdentity {
    let account = legacy_market_maker.or(market_maker_identity_account);
    match (
        account,
        market_maker_identity_delegation_id_hex,
        market_maker_identity_serving_agent,
    ) {
        (Some(account_id), None, None) => MarketMakerIdentity::Account(account_id),
        (None, Some(delegation_id_hex), Some(serving_agent)) => MarketMakerIdentity::Delegation {
            delegation_id: decode_hex_array::<32>(&delegation_id_hex, "delegation id"),
            serving_agent,
        },
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) => {
            panic!("specify either an account market maker identity or a delegation market maker identity")
        }
        (None, None, Some(_)) | (None, Some(_), None) => {
            panic!("delegation market maker identity requires both delegation id and serving agent")
        }
        (None, None, None) => {
            panic!("market maker identity is required")
        }
    }
}

pub(crate) fn derive_delegation_action_id_response(
    action_hex: &str,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> Value {
    let action_bytes = decode_hex_bytes(action_hex, "delegation action");
    let action = BlockDelegationAction::try_from_slice(&action_bytes)
        .expect("delegation action must be Borsh-encoded BlockDelegationAction");
    let action_id = delegation_action_id_for_submission(&action, timestamp_ms, submitter_nonce);
    json!({
        "action_kind": delegation_block_action_kind(&action),
        "action_id_hex": encode_hex(&action_id),
        "action_hex": encode_hex(&to_vec(&action).expect("delegation action should encode")),
        "timestamp_ms": timestamp_ms,
        "submitter_nonce": submitter_nonce,
    })
}

pub(crate) fn derive_market_maker_program_action_id_response(
    action_hex: &str,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> Value {
    let action_bytes = decode_hex_bytes(action_hex, "market-maker program action");
    let action = BlockMarketMakerProgramAction::try_from_slice(&action_bytes)
        .expect("market-maker program action must be Borsh-encoded BlockMarketMakerProgramAction");
    let action_id =
        market_maker_program_action_id_for_submission(&action, timestamp_ms, submitter_nonce);
    json!({
        "action_kind": market_maker_program_block_action_kind(&action),
        "action_id_hex": encode_hex(&action_id),
        "action_hex": encode_hex(&to_vec(&action).expect("market-maker program action should encode")),
        "timestamp_ms": timestamp_ms,
        "submitter_nonce": submitter_nonce,
    })
}

pub(crate) fn signed_delegation_action_request(args: SignedDelegationActionArgs<'_>) -> Value {
    let signer = signing_key_from_secret_hex(args.signer_secret_hex);
    let action_bytes = decode_hex_bytes(args.action_hex, "delegation action");
    let action = BlockDelegationAction::try_from_slice(&action_bytes)
        .expect("delegation action must be Borsh-encoded BlockDelegationAction");
    let action_id =
        delegation_action_id_for_submission(&action, args.timestamp_ms, args.submitter_nonce);
    let canonical_action_hex =
        encode_hex(&to_vec(&action).expect("delegation action should encode"));
    let mut signed_action = SignedDelegationAction {
        chain_id: args.chain_id,
        signer_account_id: args.signer_account_id,
        action_id,
        timestamp_ms: args.timestamp_ms,
        submitter_nonce: args.submitter_nonce,
        action,
        signature: [0; 64],
    };
    signed_action.signature = signer
        .sign(&delegation_action_signing_bytes(&signed_action))
        .to_bytes();

    json_rpc_request(
        "kombat_submitSignedDelegationAction",
        SubmitSignedDelegationActionParams {
            signed_delegation_action: SignedDelegationActionJson {
                chain_id: signed_action.chain_id,
                signer_account_id: signed_action.signer_account_id,
                action_id_hex: encode_hex(&signed_action.action_id),
                action_hex: canonical_action_hex,
                timestamp_ms: signed_action.timestamp_ms,
                submitter_nonce: signed_action.submitter_nonce,
                signature_hex: encode_hex(&signed_action.signature),
            },
            wait_timeout_ms: args.wait_timeout_ms,
        },
        args.request_id,
    )
}

pub(crate) fn signed_market_maker_program_action_request(
    args: SignedMarketMakerProgramActionArgs<'_>,
) -> Value {
    let signer = signing_key_from_secret_hex(args.signer_secret_hex);
    let action_bytes = decode_hex_bytes(args.action_hex, "market-maker program action");
    let action = BlockMarketMakerProgramAction::try_from_slice(&action_bytes)
        .expect("market-maker program action must be Borsh-encoded BlockMarketMakerProgramAction");
    let action_id = market_maker_program_action_id_for_submission(
        &action,
        args.timestamp_ms,
        args.submitter_nonce,
    );
    let canonical_action_hex =
        encode_hex(&to_vec(&action).expect("market-maker program action should encode"));
    let mut signed_action = SignedMarketMakerProgramAction {
        chain_id: args.chain_id,
        signer_account_id: args.signer_account_id,
        action_id,
        timestamp_ms: args.timestamp_ms,
        submitter_nonce: args.submitter_nonce,
        action,
        signature: [0; 64],
    };
    signed_action.signature = signer
        .sign(&market_maker_program_action_signing_bytes(&signed_action))
        .to_bytes();

    json_rpc_request(
        "kombat_submitSignedMarketMakerProgramAction",
        SubmitSignedMarketMakerProgramActionParams {
            signed_market_maker_program_action: SignedMarketMakerProgramActionJson {
                chain_id: signed_action.chain_id,
                signer_account_id: signed_action.signer_account_id,
                action_id_hex: encode_hex(&signed_action.action_id),
                action_hex: canonical_action_hex,
                timestamp_ms: signed_action.timestamp_ms,
                submitter_nonce: signed_action.submitter_nonce,
                signature_hex: encode_hex(&signed_action.signature),
            },
            wait_timeout_ms: args.wait_timeout_ms,
        },
        args.request_id,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn signed_intent_request(
    session_secret_hex: &str,
    account_id: u64,
    policy_id: u64,
    chain_id: u64,
    nonce: u64,
    expires_at: u64,
    predicate: (Option<IntentPredicate>, Option<IntentPredicateJson>),
    kind: IntentKind,
    kind_json: IntentKindJson,
    timestamp_ms: u64,
    request_id: u64,
) -> Value {
    let signer = signing_key_from_secret_hex(session_secret_hex);
    let intent = Intent {
        chain_id,
        account: account_id,
        policy_id,
        nonce,
        expires_at,
        predicate: predicate.0,
        kind,
    };
    let session_payload = SessionPayload {
        intent_hash: intent_signing_hash(&intent),
        policy_id,
        chain_id,
    };
    let signing_bytes = to_vec(&session_payload).expect("session payload should serialize");
    let signed_intent = SignedIntent {
        intent,
        signature: signer.sign(&signing_bytes).to_bytes(),
    };

    json_rpc_request(
        "kombat_submitIntent",
        SubmitIntentParams {
            signed_intent: SignedIntentJson {
                intent: IntentJson {
                    chain_id: signed_intent.intent.chain_id,
                    account: signed_intent.intent.account,
                    policy_id: signed_intent.intent.policy_id,
                    nonce: signed_intent.intent.nonce,
                    expires_at: signed_intent.intent.expires_at,
                    predicate: predicate.1,
                    kind: kind_json,
                },
                signature_hex: encode_hex(&signed_intent.signature),
            },
            timestamp_ms,
        },
        request_id,
    )
}

pub(crate) fn policy_terms_wire_json(
    gas_budget_per_day: &str,
    gas_refill_per_day: &str,
    max_intents_per_second: u32,
    max_intents_per_day: u32,
    max_volume_per_day: &str,
    max_drawdown_pct: u16,
) -> PolicyTermsWireJson {
    PolicyTermsWireJson {
        gas_budget_per_day: parse_u128(gas_budget_per_day, "gas_budget_per_day"),
        gas_refill_per_day: parse_u128(gas_refill_per_day, "gas_refill_per_day"),
        max_intents_per_second,
        max_intents_per_day,
        max_volume_per_day: parse_u128(max_volume_per_day, "max_volume_per_day"),
        max_drawdown_pct,
    }
}

pub(crate) fn policy_terms_from_wire(terms: &PolicyTermsWireJson) -> PolicyTerms {
    PolicyTerms {
        gas_budget_per_day: terms.gas_budget_per_day,
        gas_refill_per_day: terms.gas_refill_per_day,
        max_intents_per_second: terms.max_intents_per_second,
        max_intents_per_day: terms.max_intents_per_day,
        max_volume_per_day: terms.max_volume_per_day,
        max_drawdown_pct: terms.max_drawdown_pct,
        ..PolicyTerms::default()
    }
}

pub(crate) fn intent_predicate_pair(
    time_after_ms: Option<u64>,
    time_before_ms: Option<u64>,
) -> (Option<IntentPredicate>, Option<IntentPredicateJson>) {
    match (time_after_ms, time_before_ms) {
        (Some(_), Some(_)) => {
            panic!("only one of --time-after-ms or --time-before-ms may be set")
        }
        (Some(not_before_ms), None) => (
            Some(IntentPredicate::TimeAfter { not_before_ms }),
            Some(IntentPredicateJson::TimeAfter { not_before_ms }),
        ),
        (None, Some(not_after_ms)) => (
            Some(IntentPredicate::TimeBefore { not_after_ms }),
            Some(IntentPredicateJson::TimeBefore { not_after_ms }),
        ),
        (None, None) => (None, None),
    }
}

pub(crate) fn json_rpc_request<T: serde::Serialize>(
    method: &str,
    params: T,
    request_id: u64,
) -> Value {
    serde_json::to_value(JsonRpcRequest {
        jsonrpc: "2.0".into(),
        method: method.into(),
        params: serde_json::to_value(params).expect("params should serialize"),
        id: json!(request_id),
    })
    .expect("request should serialize")
}

pub(crate) fn signing_key_from_secret_hex(input: &str) -> SigningKey {
    SigningKey::from_bytes(&decode_hex_array::<32>(input, "secret key"))
}

pub(crate) fn decode_hex_array<const N: usize>(input: &str, label: &str) -> [u8; N] {
    let normalized = input.strip_prefix("0x").unwrap_or(input);
    let bytes = hex::decode(normalized).unwrap_or_else(|_| panic!("{label} must be valid hex"));
    bytes
        .try_into()
        .unwrap_or_else(|_| panic!("{label} must decode to {N} bytes"))
}

pub(crate) fn normalize_hex(input: &str) -> String {
    encode_hex(&decode_hex_bytes(input, "hex value"))
}

pub(crate) fn decode_hex_bytes(input: &str, label: &str) -> Vec<u8> {
    let normalized = input.strip_prefix("0x").unwrap_or(input);
    hex::decode(normalized).unwrap_or_else(|_| panic!("{label} must be valid hex"))
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub(crate) fn parse_u128(input: &str, label: &str) -> u128 {
    input
        .parse::<u128>()
        .unwrap_or_else(|_| panic!("{label} must parse as u128"))
}
