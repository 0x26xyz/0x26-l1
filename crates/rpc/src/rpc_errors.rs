use crate::*;

pub(crate) fn local_bridge_mutations_disabled_error() -> JsonRpcErrorObject {
    JsonRpcErrorObject {
        code: -32012,
        message: "local bridge mutations disabled".into(),
        data: Some(json!({
            "reason": "local_bridge_mutations_disabled",
            "required_env": LOCAL_BRIDGE_MUTATIONS_ENABLED_ENV,
        })),
    }
}

pub(crate) fn intent_submission_rpc_error(
    signed_intent: &SignedIntent,
    timestamp_ms: u64,
    error: &NodeError,
) -> RpcError {
    let intent_id_hex = encode_hex(&intent_id(signed_intent));
    let mut data = json!({
        "kind": "intent_submission",
        "reason": intent_submission_reason(error),
        "intent_id_hex": intent_id_hex,
        "policy_id": signed_intent.intent.policy_id,
        "nonce": signed_intent.intent.nonce,
        "timestamp_ms": timestamp_ms,
    });

    match error {
        NodeError::DuplicatePolicyNonce { policy_id, nonce } => {
            data["policy_id"] = json!(policy_id);
            data["nonce"] = json!(nonce);
        }
        NodeError::StaleIntentNonce {
            policy_id,
            expected,
            actual,
        } => {
            data["policy_id"] = json!(policy_id);
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        NodeError::IntentExpired {
            expires_at,
            timestamp_ms,
            ..
        } => {
            data["expires_at"] = json!(expires_at);
            data["timestamp_ms"] = json!(timestamp_ms);
        }
        NodeError::Policy(policy_error) => {
            attach_policy_error_details(&mut data, policy_error);
        }
        NodeError::AccountFrozen { account_id } => {
            data["account_id"] = json!(account_id);
        }
        NodeError::MarketTradingPaused { market_id } => {
            data["market_id"] = json!(market_id);
        }
        NodeError::StaleMarketData { market_id } => {
            data["market_id"] = json!(market_id);
        }
        NodeError::UnknownMarket { market_id } => {
            data["market_id"] = json!(market_id);
        }
        NodeError::InvalidOracleSubmission { market_id, .. }
        | NodeError::OracleAggregationFailed { market_id, .. }
        | NodeError::InvalidCollateralMovement { market_id, .. } => {
            data["market_id"] = json!(market_id);
        }
        NodeError::InvalidDelegation { agent, reason } => {
            data["agent"] = json!(agent);
            data["delegation_reason"] = json!(reason);
        }
        NodeError::InvalidMarketMakerProgram { program_id, reason } => {
            data["program_id_hex"] = json!(encode_hex(program_id));
            data["market_maker_program_reason"] = json!(reason);
        }
        NodeError::InvalidTeeGovernance { action_id, reason } => {
            data["tee_governance_action_id_hex"] = json!(encode_hex(action_id));
            data["tee_governance_reason"] = json!(reason);
        }
        NodeError::InvalidRealCapitalGovernance { action_id, reason } => {
            data["real_capital_governance_action_id_hex"] = json!(encode_hex(action_id));
            data["real_capital_governance_reason"] = json!(reason);
        }
        NodeError::InvalidMandate { agent, reason } => {
            data["agent"] = json!(agent);
            data["mandate_reason"] = json!(reason);
        }
        NodeError::InvalidIdentity { account, reason } => {
            data["account"] = json!(account);
            data["identity_reason"] = json!(reason);
        }
        NodeError::InvalidIdentityAction { account, reason } => {
            data["account"] = json!(account);
            data["identity_action_reason"] = json!(reason);
        }
        NodeError::InvalidIdentityActionNonce {
            account,
            expected,
            actual,
        } => {
            data["account"] = json!(account);
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        NodeError::InvalidIdentityActionSignature { account } => {
            data["account"] = json!(account);
        }
        NodeError::InvalidMandateAction { owner, reason } => {
            data["owner_hex"] = json!(encode_hex(owner));
            data["mandate_action_reason"] = json!(reason);
        }
        NodeError::InvalidMandateActionNonce {
            owner,
            expected,
            actual,
        } => {
            data["owner_hex"] = json!(encode_hex(owner));
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        NodeError::InvalidMandateActionSignature { owner } => {
            data["owner_hex"] = json!(encode_hex(owner));
        }
        NodeError::Order(order_error) => {
            data["order_rejection_reason"] = json!(order_reject_reason_name(*order_error));
        }
        NodeError::DuplicateIntent(_)
        | NodeError::LocalValidatorUnavailable
        | NodeError::InvalidBlock(_)
        | NodeError::InvalidConsensusPubKey { .. }
        | NodeError::MissingConsensusSigningKey { .. }
        | NodeError::InvalidClusterSize { .. }
        | NodeError::BootstrapConfig(_)
        | NodeError::MissingLeader { .. }
        | NodeError::ZeroStake
        | NodeError::Persistence { .. }
        | NodeError::Execution(_)
        | NodeError::Account(_)
        | NodeError::PositionFlat { .. }
        | NodeError::NotUnderMaintenance { .. }
        | NodeError::MarketAlreadyListed { .. }
        | NodeError::InvalidMarketConfig { .. } => {}
    }

    RpcError::Submission(JsonRpcErrorObject {
        code: -32020,
        message: "intent submission rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn owner_call_submission_rpc_error(
    signed_owner_call: &SignedOwnerCall,
    error: &NodeError,
) -> RpcError {
    owner_call_submission_rpc_error_from_parts(
        signed_owner_call.account_id,
        &signed_owner_call.call,
        error,
    )
}

pub(crate) fn owner_call_submission_rpc_error_from_parts(
    account_id: u64,
    call: &OwnerCall,
    error: &NodeError,
) -> RpcError {
    let mut data = json!({
        "kind": "owner_call_submission",
        "reason": owner_call_submission_reason(error),
        "account_id": account_id,
        "nonce": call.nonce,
        "action": owner_action_name(&call.action),
    });

    match error {
        NodeError::Account(account_error) => {
            attach_account_error_details(&mut data, account_error);
        }
        NodeError::Policy(policy_error) => {
            attach_policy_error_details(&mut data, policy_error);
        }
        NodeError::AccountFrozen { account_id } => {
            data["account_id"] = json!(account_id);
        }
        NodeError::MarketTradingPaused { market_id } => {
            data["market_id"] = json!(market_id);
        }
        NodeError::LocalValidatorUnavailable
        | NodeError::InvalidBlock(_)
        | NodeError::InvalidConsensusPubKey { .. }
        | NodeError::MissingConsensusSigningKey { .. }
        | NodeError::InvalidClusterSize { .. }
        | NodeError::BootstrapConfig(_)
        | NodeError::MissingLeader { .. }
        | NodeError::ZeroStake
        | NodeError::Persistence { .. }
        | NodeError::Execution(_)
        | NodeError::Order(_)
        | NodeError::DuplicateIntent(_)
        | NodeError::DuplicatePolicyNonce { .. }
        | NodeError::StaleIntentNonce { .. }
        | NodeError::IntentExpired { .. }
        | NodeError::PositionFlat { .. }
        | NodeError::NotUnderMaintenance { .. }
        | NodeError::UnknownMarket { .. }
        | NodeError::MarketAlreadyListed { .. }
        | NodeError::InvalidMarketConfig { .. }
        | NodeError::InvalidCollateralMovement { .. }
        | NodeError::InvalidOracleSubmission { .. }
        | NodeError::OracleAggregationFailed { .. }
        | NodeError::InvalidDelegation { .. }
        | NodeError::InvalidMarketMakerProgram { .. }
        | NodeError::InvalidTeeGovernance { .. }
        | NodeError::InvalidRealCapitalGovernance { .. }
        | NodeError::InvalidMandate { .. }
        | NodeError::InvalidIdentity { .. }
        | NodeError::InvalidIdentityAction { .. }
        | NodeError::InvalidIdentityActionNonce { .. }
        | NodeError::InvalidIdentityActionSignature { .. }
        | NodeError::InvalidMandateAction { .. }
        | NodeError::InvalidMandateActionNonce { .. }
        | NodeError::InvalidMandateActionSignature { .. }
        | NodeError::StaleMarketData { .. } => {}
    }

    RpcError::Submission(JsonRpcErrorObject {
        code: -32021,
        message: "owner call rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn order_submission_rpc_error(timestamp_ms: u64, error: &NodeError) -> RpcError {
    let mut data = json!({
        "kind": "order_submission",
        "reason": order_submission_reason(error),
        "timestamp_ms": timestamp_ms,
    });

    if let NodeError::Order(order_error) = error {
        data["order_reason"] = json!(order_reject_reason_name(*order_error));
    }
    if let NodeError::AccountFrozen { account_id } = error {
        data["account_id"] = json!(account_id);
    }
    if let NodeError::MarketTradingPaused { market_id } = error {
        data["market_id"] = json!(market_id);
    }
    if let NodeError::StaleMarketData { market_id } = error {
        data["market_id"] = json!(market_id);
    }
    if let NodeError::UnknownMarket { market_id } = error {
        data["market_id"] = json!(market_id);
    }
    if let NodeError::InvalidCollateralMovement {
        market_id,
        account_id,
        ..
    } = error
    {
        data["market_id"] = json!(market_id);
        data["account_id"] = json!(account_id);
    }

    RpcError::Submission(JsonRpcErrorObject {
        code: -32023,
        message: "order submission rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn market_lifecycle_rpc_error(error: NodeError) -> RpcError {
    let (reason, market_id) = match &error {
        NodeError::UnknownMarket { market_id } => ("unknown_market", Some(*market_id)),
        NodeError::MarketAlreadyListed { market_id } => ("market_already_listed", Some(*market_id)),
        NodeError::InvalidMarketConfig { market_id, reason } => (*reason, Some(*market_id)),
        _ => ("internal_error", None),
    };
    let mut data = json!({
        "kind": "market_lifecycle",
        "reason": reason,
    });
    if let Some(market_id) = market_id {
        data["market_id"] = json!(market_id);
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32024,
        message: "market lifecycle rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn oracle_submission_rpc_error(error: NodeError) -> RpcError {
    let (reason, market_id) = match &error {
        NodeError::UnknownMarket { market_id } => ("unknown_market", Some(*market_id)),
        NodeError::InvalidOracleSubmission { market_id, reason }
        | NodeError::OracleAggregationFailed { market_id, reason } => (*reason, Some(*market_id)),
        _ => ("internal_error", None),
    };
    let mut data = json!({
        "kind": "oracle_submission",
        "reason": reason,
    });
    if let Some(market_id) = market_id {
        data["market_id"] = json!(market_id);
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32025,
        message: "oracle submission rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn delegation_rpc_error(error: NodeError) -> RpcError {
    let (reason, agent) = match &error {
        NodeError::InvalidDelegation { agent, reason } => (*reason, Some(*agent)),
        _ => ("internal_error", None),
    };
    let mut data = json!({
        "kind": "delegation",
        "reason": reason,
    });
    if let Some(agent) = agent {
        data["agent"] = json!(agent);
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32026,
        message: "delegation rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn market_maker_program_rpc_error(error: NodeError) -> RpcError {
    let (reason, program_id) = match &error {
        NodeError::InvalidMarketMakerProgram { program_id, reason } => (*reason, Some(*program_id)),
        _ => ("internal_error", None),
    };
    let mut data = json!({
        "kind": "market_maker_program",
        "reason": reason,
    });
    if let Some(program_id) = program_id {
        data["program_id_hex"] = json!(encode_hex(&program_id));
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32028,
        message: "market maker program rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn real_capital_governance_rpc_error(error: NodeError) -> RpcError {
    // This enqueue-only surface should only return InvalidRealCapitalGovernance
    // from node admission. Other variants are internal faults, not caller-fixable
    // governance rejections.
    let (reason, action_id) = match &error {
        NodeError::InvalidRealCapitalGovernance { action_id, reason } => {
            (*reason, Some(*action_id))
        }
        _ => ("internal_error", None),
    };
    let mut data = json!({
        "kind": "real_capital_governance",
        "reason": reason,
    });
    if let Some(action_id) = action_id {
        data["action_id_hex"] = json!(encode_hex(&action_id));
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32030,
        message: "real capital governance rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn mandate_rpc_error(error: NodeError) -> RpcError {
    let reason = mandate_submission_reason(&error);
    let mut data = json!({
        "kind": "mandate",
        "reason": reason,
    });
    match &error {
        NodeError::InvalidMandate { agent, .. } => {
            data["agent"] = json!(agent);
        }
        NodeError::InvalidMandateAction { owner, .. }
        | NodeError::InvalidMandateActionSignature { owner } => {
            data["owner_hex"] = json!(encode_hex(owner));
        }
        NodeError::InvalidMandateActionNonce {
            owner,
            expected,
            actual,
        } => {
            data["owner_hex"] = json!(encode_hex(owner));
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        _ => {}
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32027,
        message: "mandate rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn mandate_submission_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::InvalidMandate { reason, .. }
        | NodeError::InvalidMandateAction { reason, .. } => reason,
        NodeError::InvalidMandateActionNonce { .. } => "invalid_nonce",
        NodeError::InvalidMandateActionSignature { .. } => "invalid_signature",
        _ => "internal_error",
    }
}

pub(crate) fn mandate_activation_tier_rpc_error(
    agent: u64,
    current_tier: IdentityTierResponse,
    required_tier: IdentityTierResponse,
) -> RpcError {
    RpcError::Submission(JsonRpcErrorObject {
        code: -32027,
        message: "mandate rejected".into(),
        data: Some(json!({
            "kind": "mandate",
            "reason": "mandate_activation_below_required_tier",
            "agent": agent,
            "current_tier": identity_tier_name(current_tier),
            "required_tier": identity_tier_name(required_tier),
        })),
    })
}

pub(crate) fn identity_rpc_error(error: NodeError) -> RpcError {
    let reason = identity_submission_reason(&error);
    let mut data = json!({
        "kind": "identity",
        "reason": reason,
    });
    match &error {
        NodeError::InvalidIdentity { account, .. }
        | NodeError::InvalidIdentityAction { account, .. }
        | NodeError::InvalidIdentityActionSignature { account } => {
            data["account"] = json!(account);
        }
        NodeError::InvalidIdentityActionNonce {
            account,
            expected,
            actual,
        } => {
            data["account"] = json!(account);
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        _ => {}
    }
    RpcError::Submission(JsonRpcErrorObject {
        code: -32029,
        message: "identity rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn identity_submission_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::InvalidIdentity { reason, .. }
        | NodeError::InvalidIdentityAction { reason, .. } => reason,
        NodeError::InvalidIdentityActionNonce { .. } => "invalid_nonce",
        NodeError::InvalidIdentityActionSignature { .. } => "invalid_signature",
        _ => "internal_error",
    }
}

pub(crate) fn account_creation_rpc_error(
    account_id: u64,
    owner: [u8; 32],
    code_hash: [u8; 32],
    timestamp_ms: u64,
    error: &NodeError,
) -> RpcError {
    let reason = match error {
        NodeError::Account(account_error) => account_error_reason(account_error),
        _ => "internal_error",
    };
    let mut data = json!({
        "kind": "account_creation",
        "reason": reason,
        "account_id": account_id,
        "owner_hex": encode_hex(&owner),
        "code_hash_hex": encode_hex(&code_hash),
        "timestamp_ms": timestamp_ms,
    });

    if let NodeError::Account(account_error) = error {
        attach_account_error_details(&mut data, account_error);
    }

    RpcError::Submission(JsonRpcErrorObject {
        code: -32022,
        message: "account creation rejected".into(),
        data: Some(data),
    })
}

pub(crate) fn intent_submission_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::AccountFrozen { .. } => "account_frozen",
        NodeError::MarketTradingPaused { .. } => "market_trading_paused",
        NodeError::StaleMarketData { .. } => "stale_market_data",
        NodeError::DuplicateIntent(_) => "duplicate_intent",
        NodeError::DuplicatePolicyNonce { .. } => "duplicate_policy_nonce",
        NodeError::StaleIntentNonce { .. } => "stale_intent_nonce",
        NodeError::IntentExpired { .. } => "intent_expired",
        NodeError::UnknownMarket { .. } => "unknown_market",
        NodeError::InvalidOracleSubmission { reason, .. }
        | NodeError::OracleAggregationFailed { reason, .. } => reason,
        NodeError::InvalidCollateralMovement { reason, .. } => reason,
        NodeError::InvalidDelegation { reason, .. } => reason,
        NodeError::InvalidMarketMakerProgram { reason, .. } => reason,
        NodeError::InvalidTeeGovernance { reason, .. } => reason,
        NodeError::InvalidRealCapitalGovernance { reason, .. } => reason,
        NodeError::InvalidMandate { reason, .. } => reason,
        NodeError::InvalidIdentity { reason, .. } => reason,
        NodeError::InvalidIdentityAction { reason, .. } => reason,
        NodeError::InvalidIdentityActionNonce { .. } => "invalid_nonce",
        NodeError::InvalidIdentityActionSignature { .. } => "invalid_signature",
        NodeError::InvalidMandateAction { reason, .. } => reason,
        NodeError::InvalidMandateActionNonce { .. } => "invalid_nonce",
        NodeError::InvalidMandateActionSignature { .. } => "invalid_signature",
        NodeError::Policy(policy_error) => policy_error_reason(policy_error),
        NodeError::Order(_) => "order_rejected",
        NodeError::LocalValidatorUnavailable
        | NodeError::InvalidBlock(_)
        | NodeError::InvalidConsensusPubKey { .. }
        | NodeError::MissingConsensusSigningKey { .. }
        | NodeError::InvalidClusterSize { .. }
        | NodeError::BootstrapConfig(_)
        | NodeError::MissingLeader { .. }
        | NodeError::ZeroStake
        | NodeError::Persistence { .. }
        | NodeError::Execution(_)
        | NodeError::Account(_)
        | NodeError::MarketAlreadyListed { .. }
        | NodeError::InvalidMarketConfig { .. } => "internal_error",
        NodeError::PositionFlat { .. } | NodeError::NotUnderMaintenance { .. } => "internal_error",
    }
}

pub(crate) fn owner_call_submission_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::Account(account_error) => account_error_reason(account_error),
        NodeError::Policy(policy_error) => policy_error_reason(policy_error),
        NodeError::LocalValidatorUnavailable
        | NodeError::InvalidBlock(_)
        | NodeError::InvalidConsensusPubKey { .. }
        | NodeError::MissingConsensusSigningKey { .. }
        | NodeError::InvalidClusterSize { .. }
        | NodeError::BootstrapConfig(_)
        | NodeError::MissingLeader { .. }
        | NodeError::ZeroStake
        | NodeError::Persistence { .. }
        | NodeError::Execution(_)
        | NodeError::Order(_)
        | NodeError::DuplicateIntent(_)
        | NodeError::DuplicatePolicyNonce { .. }
        | NodeError::StaleIntentNonce { .. }
        | NodeError::IntentExpired { .. }
        | NodeError::PositionFlat { .. }
        | NodeError::NotUnderMaintenance { .. }
        | NodeError::AccountFrozen { .. }
        | NodeError::MarketTradingPaused { .. }
        | NodeError::UnknownMarket { .. }
        | NodeError::MarketAlreadyListed { .. }
        | NodeError::InvalidMarketConfig { .. }
        | NodeError::InvalidOracleSubmission { .. }
        | NodeError::InvalidCollateralMovement { .. }
        | NodeError::OracleAggregationFailed { .. }
        | NodeError::InvalidDelegation { .. }
        | NodeError::InvalidMarketMakerProgram { .. }
        | NodeError::InvalidTeeGovernance { .. }
        | NodeError::InvalidRealCapitalGovernance { .. }
        | NodeError::InvalidMandate { .. }
        | NodeError::InvalidIdentity { .. }
        | NodeError::InvalidIdentityAction { .. }
        | NodeError::InvalidIdentityActionNonce { .. }
        | NodeError::InvalidIdentityActionSignature { .. }
        | NodeError::InvalidMandateAction { .. }
        | NodeError::InvalidMandateActionNonce { .. }
        | NodeError::InvalidMandateActionSignature { .. }
        | NodeError::StaleMarketData { .. } => "internal_error",
    }
}

pub(crate) fn order_submission_reason(error: &NodeError) -> &'static str {
    match error {
        NodeError::AccountFrozen { .. } => "account_frozen",
        NodeError::MarketTradingPaused { .. } => "market_trading_paused",
        NodeError::Order(order_error) => order_reject_reason_name(*order_error),
        NodeError::PositionFlat { .. } => "position_flat",
        NodeError::NotUnderMaintenance { .. } => "not_under_maintenance",
        NodeError::StaleMarketData { .. } => "stale_market_data",
        NodeError::UnknownMarket { .. } => "unknown_market",
        NodeError::InvalidOracleSubmission { reason, .. }
        | NodeError::OracleAggregationFailed { reason, .. } => reason,
        NodeError::InvalidCollateralMovement { reason, .. } => reason,
        NodeError::InvalidDelegation { reason, .. } => reason,
        NodeError::InvalidMarketMakerProgram { reason, .. } => reason,
        NodeError::InvalidTeeGovernance { reason, .. } => reason,
        NodeError::InvalidRealCapitalGovernance { reason, .. } => reason,
        NodeError::InvalidMandate { reason, .. } => reason,
        NodeError::InvalidIdentity { reason, .. } => reason,
        NodeError::InvalidIdentityAction { reason, .. } => reason,
        NodeError::InvalidIdentityActionNonce { .. } => "invalid_nonce",
        NodeError::InvalidIdentityActionSignature { .. } => "invalid_signature",
        NodeError::InvalidMandateAction { reason, .. } => reason,
        NodeError::InvalidMandateActionNonce { .. } => "invalid_nonce",
        NodeError::InvalidMandateActionSignature { .. } => "invalid_signature",
        NodeError::LocalValidatorUnavailable
        | NodeError::InvalidBlock(_)
        | NodeError::InvalidConsensusPubKey { .. }
        | NodeError::MissingConsensusSigningKey { .. }
        | NodeError::InvalidClusterSize { .. }
        | NodeError::BootstrapConfig(_)
        | NodeError::MissingLeader { .. }
        | NodeError::ZeroStake
        | NodeError::Persistence { .. }
        | NodeError::Execution(_)
        | NodeError::Account(_)
        | NodeError::Policy(_)
        | NodeError::DuplicateIntent(_)
        | NodeError::DuplicatePolicyNonce { .. }
        | NodeError::StaleIntentNonce { .. }
        | NodeError::MarketAlreadyListed { .. }
        | NodeError::InvalidMarketConfig { .. }
        | NodeError::IntentExpired { .. } => "internal_error",
    }
}

pub(crate) fn account_error_reason(error: &AccountError) -> &'static str {
    match error {
        AccountError::DuplicateAccount(_) => "duplicate_account",
        AccountError::UnknownAccount(_) => "unknown_account",
        AccountError::Unauthorized => "unauthorized",
        AccountError::InvalidNonce { .. } => "invalid_nonce",
        AccountError::ChainMismatch { .. } => "chain_mismatch",
        AccountError::NoPendingOwner => "no_pending_owner",
        AccountError::PendingOwnerMismatch => "pending_owner_mismatch",
        AccountError::DuplicatePolicy(_) => "duplicate_policy",
        AccountError::UnknownPolicy(_) => "unknown_policy",
        AccountError::InvalidOwnerProposal => "invalid_owner_proposal",
        AccountError::InvalidOwnerSignature => "invalid_owner_signature",
        AccountError::UpgradeNotApproved(_) => "upgrade_not_approved",
        AccountError::UpgradeNoop => "upgrade_noop",
        AccountError::Policy(policy_error) => policy_error_reason(policy_error),
    }
}

pub(crate) fn policy_error_reason(error: &PolicyError) -> &'static str {
    match error {
        PolicyError::DuplicatePolicyId(_) => "duplicate_policy",
        PolicyError::UnknownPolicy(_) => "unknown_policy",
        PolicyError::UnauthorizedGrantee => "unauthorized_grantee",
        PolicyError::SessionPolicyMismatch { .. } => "session_policy_mismatch",
        PolicyError::SessionChainMismatch { .. } => "session_chain_mismatch",
        PolicyError::InvalidSessionSignature | PolicyError::InvalidSessionKeyMaterial => {
            "invalid_session_signature"
        }
        PolicyError::SessionKeyRevoked { .. }
        | PolicyError::SessionKeyAlreadyActive
        | PolicyError::SessionKeyAlreadyRevoked => "session_key_revoked",
        PolicyError::InvalidIntentNonce { .. } => "invalid_intent_nonce",
        PolicyError::PolicyNotActive(_) => "policy_not_active",
        PolicyError::PolicyExpired => "policy_expired",
        PolicyError::GasBudgetExceeded => "gas_budget_exceeded",
        PolicyError::IntentRateLimitExceeded => "intent_rate_limit_exceeded",
        PolicyError::MinuteIntentLimitExceeded => "minute_intent_limit_exceeded",
        PolicyError::DailyIntentLimitExceeded => "daily_intent_limit_exceeded",
        PolicyError::DailyVolumeExceeded => "daily_volume_exceeded",
        PolicyError::MarketNotAllowed => "market_not_allowed",
        PolicyError::TradeNotionalExceeded => "trade_notional_exceeded",
        PolicyError::NotionalOutstandingExceeded => "notional_outstanding_exceeded",
        PolicyError::SubPolicyScopeMismatch => "sub_policy_scope_mismatch",
        PolicyError::SubPolicyTermsExceedParent(_) => "sub_policy_terms_exceed_parent",
        PolicyError::DelegationDepthExceeded { .. } => "delegation_depth_exceeded",
        PolicyError::DrawdownExceededAndRevoked => "drawdown_exceeded_and_revoked",
        PolicyError::Overflow => "overflow",
    }
}

pub(crate) fn attach_policy_error_details(data: &mut Value, error: &PolicyError) {
    match error {
        PolicyError::UnknownPolicy(policy_id) | PolicyError::DuplicatePolicyId(policy_id) => {
            data["policy_id"] = json!(policy_id);
        }
        PolicyError::SessionPolicyMismatch { expected, actual } => {
            data["expected_policy_id"] = json!(expected);
            data["actual_policy_id"] = json!(actual);
        }
        PolicyError::SessionChainMismatch { expected, actual } => {
            data["expected_chain_id"] = json!(expected);
            data["actual_chain_id"] = json!(actual);
        }
        PolicyError::SessionKeyRevoked {
            revoked_at,
            block_number,
        } => {
            data["revoked_at"] = json!(revoked_at);
            data["block_number"] = json!(block_number);
        }
        PolicyError::InvalidIntentNonce { expected, actual } => {
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        PolicyError::PolicyNotActive(state) => {
            data["policy_state"] = json!(policy_state_name(*state));
        }
        PolicyError::SubPolicyTermsExceedParent(field) => {
            data["field"] = json!(field);
        }
        PolicyError::DelegationDepthExceeded {
            max_depth,
            attempted_depth,
        } => {
            data["max_depth"] = json!(max_depth);
            data["attempted_depth"] = json!(attempted_depth);
        }
        PolicyError::UnauthorizedGrantee
        | PolicyError::InvalidSessionSignature
        | PolicyError::InvalidSessionKeyMaterial
        | PolicyError::SessionKeyAlreadyActive
        | PolicyError::SessionKeyAlreadyRevoked
        | PolicyError::PolicyExpired
        | PolicyError::GasBudgetExceeded
        | PolicyError::IntentRateLimitExceeded
        | PolicyError::MinuteIntentLimitExceeded
        | PolicyError::DailyIntentLimitExceeded
        | PolicyError::DailyVolumeExceeded
        | PolicyError::MarketNotAllowed
        | PolicyError::TradeNotionalExceeded
        | PolicyError::NotionalOutstandingExceeded
        | PolicyError::SubPolicyScopeMismatch
        | PolicyError::DrawdownExceededAndRevoked
        | PolicyError::Overflow => {}
    }
}

pub(crate) fn attach_account_error_details(data: &mut Value, error: &AccountError) {
    match error {
        AccountError::DuplicateAccount(account_id) | AccountError::UnknownAccount(account_id) => {
            data["account_id"] = json!(account_id);
        }
        AccountError::InvalidNonce { expected, actual } => {
            data["expected_nonce"] = json!(expected);
            data["actual_nonce"] = json!(actual);
        }
        AccountError::ChainMismatch { expected, actual } => {
            data["expected_chain_id"] = json!(expected);
            data["actual_chain_id"] = json!(actual);
        }
        AccountError::DuplicatePolicy(policy_id) | AccountError::UnknownPolicy(policy_id) => {
            data["policy_id"] = json!(policy_id);
        }
        AccountError::UpgradeNotApproved(code_hash) => {
            data["code_hash_hex"] = json!(encode_hex(code_hash));
        }
        AccountError::Policy(policy_error) => attach_policy_error_details(data, policy_error),
        AccountError::Unauthorized
        | AccountError::NoPendingOwner
        | AccountError::PendingOwnerMismatch
        | AccountError::InvalidOwnerProposal
        | AccountError::InvalidOwnerSignature
        | AccountError::UpgradeNoop => {}
    }
}

pub(crate) fn owner_action_name(action: &OwnerAction) -> &'static str {
    match action {
        OwnerAction::ProposeOwner { .. } => "propose_owner",
        OwnerAction::AcceptOwner => "accept_owner",
        OwnerAction::Pause => "pause",
        OwnerAction::Unpause => "unpause",
        OwnerAction::CreatePolicy { .. } => "create_policy",
        OwnerAction::AddPolicy { .. } => "add_policy",
        OwnerAction::RemovePolicy { .. } => "remove_policy",
        OwnerAction::RevokePolicy { .. } => "revoke_policy",
        OwnerAction::Upgrade { .. } => "upgrade",
    }
}

pub(crate) fn owner_call_response_from_parts(
    account_id: u64,
    chain_id: u64,
    call: &OwnerCall,
) -> SubmitOwnerCallResponse {
    let (policy_id, new_owner, code_hash) = match &call.action {
        OwnerAction::ProposeOwner { new_owner } => (None, Some(*new_owner), None),
        OwnerAction::CreatePolicy { policy_id, .. }
        | OwnerAction::AddPolicy { policy_id }
        | OwnerAction::RemovePolicy { policy_id }
        | OwnerAction::RevokePolicy { policy_id } => (Some(*policy_id), None, None),
        OwnerAction::Upgrade { code_hash } => (None, None, Some(*code_hash)),
        OwnerAction::AcceptOwner | OwnerAction::Pause | OwnerAction::Unpause => (None, None, None),
    };

    SubmitOwnerCallResponse {
        accepted: true,
        account_id,
        chain_id,
        nonce: call.nonce,
        action: owner_action_name(&call.action),
        policy_id,
        new_owner,
        code_hash,
    }
}

pub(crate) fn policy_state_name(state: PolicyState) -> &'static str {
    match state {
        PolicyState::PendingActivation => "pending_activation",
        PolicyState::Active => "active",
        PolicyState::Suspended => "suspended",
        PolicyState::Revoked => "revoked",
        PolicyState::Expired => "expired",
    }
}

pub(crate) fn map_rpc_error_to_json_rpc_error(error: RpcError) -> JsonRpcErrorObject {
    match error {
        RpcError::HeadNotSet => JsonRpcErrorObject {
            code: -32001,
            message: "head not set".into(),
            data: None,
        },
        RpcError::InvalidRequest(message) => JsonRpcErrorObject {
            code: -32602,
            message: message.into(),
            data: None,
        },
        RpcError::UnknownAccount(account_id) => JsonRpcErrorObject {
            code: -32003,
            message: format!("unknown account {account_id}"),
            data: None,
        },
        RpcError::UnknownPolicy(policy_id) => JsonRpcErrorObject {
            code: -32004,
            message: format!("unknown policy {policy_id}"),
            data: None,
        },
        RpcError::Submission(error) => error,
        RpcError::Node(message) => JsonRpcErrorObject {
            code: -32000,
            message,
            data: None,
        },
    }
}

pub(crate) fn durable_rpc_write_domains() -> &'static [LockDomain] {
    DURABLE_RPC_WRITE_DOMAINS
}

pub(crate) fn durable_rpc_write_error(error: NodeError) -> JsonRpcErrorObject {
    map_rpc_error_to_json_rpc_error(RpcError::Node(format!("{error:?}")))
}
