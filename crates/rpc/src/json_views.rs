use crate::*;

pub(crate) fn produce_block_response_to_json(response: &ProduceBlockResponse) -> Value {
    json!({
        "height": response.height,
        "hash_hex": encode_hex(&response.hash),
        "state_root_hex": encode_hex(&response.state_root),
        "item_count": response.item_count,
    })
}

pub(crate) fn policy_usdc_balance_response_to_json(response: &PolicyUsdcBalanceResponse) -> Value {
    json!({
        "address_hex": encode_hex(&response.address),
        "balance": response.balance,
        "state_root_hex": encode_hex(&response.state_root),
    })
}

pub(crate) fn faucet_fund_account_response_to_json(response: &FaucetFundAccountResponse) -> Value {
    json!({
        "address_hex": encode_hex(&response.address),
        "policy_usdc_balance": response.policy_usdc_balance,
        "evm_usdc_balance": response.evm_usdc_balance,
        "evm_usdc_total_supply": response.evm_usdc_total_supply,
        "evm_usdc_deposit_tx_hash": response
            .evm_usdc_deposit_tx_hash
            .as_ref()
            .map(|tx_hash| encode_hex(tx_hash)),
        "state_root_hex": encode_hex(&response.state_root),
    })
}

pub(crate) fn wallet_balances_response_to_json(response: &WalletBalancesResponse) -> Value {
    json!({
        "address_hex": encode_hex(&response.address),
        "policy_usdc_balance": response.policy_usdc_balance,
        "evm_usdc_balance": response.evm_usdc_balance,
        "evm_usdc_total_supply": response.evm_usdc_total_supply,
        "next_seq": response.next_seq,
    })
}

pub(crate) fn wallet_balances_stream_payload(response: &WalletBalancesResponse, seq: u64) -> Value {
    json!({
        "seq": seq,
        "address_hex": encode_hex(&response.address),
        "policy_usdc_balance": response.policy_usdc_balance,
        "evm_usdc_balance": response.evm_usdc_balance,
        "evm_usdc_total_supply": response.evm_usdc_total_supply,
        "next_seq": seq.saturating_add(1),
    })
}

pub(crate) fn wallet_balances_changed(
    left: &WalletBalancesResponse,
    right: &WalletBalancesResponse,
) -> bool {
    left.address != right.address
        || left.policy_usdc_balance != right.policy_usdc_balance
        || left.evm_usdc_balance != right.evm_usdc_balance
        || left.evm_usdc_total_supply != right.evm_usdc_total_supply
}

pub(crate) fn evm_usdc_minter_role_response_to_json(response: &EvmUsdcMinterRoleResponse) -> Value {
    json!({
        "address_hex": encode_hex(&response.address),
        "enabled": response.enabled,
        "state_root_hex": encode_hex(&response.state_root),
    })
}

pub(crate) fn snapshot_file_response(
    path: String,
    snapshot_size_bytes: u64,
    chain_info: ChainInfoResponse,
) -> SnapshotFileResponse {
    SnapshotFileResponse {
        path,
        chain_id: chain_info.chain_id,
        head_height: chain_info.head_height,
        head_hash: chain_info.head_hash,
        committed_state_root: chain_info.committed_state_root,
        quarantined_block_count: chain_info.quarantined_block_count,
        snapshot_size_bytes,
    }
}

pub(crate) fn snapshot_file_response_to_json(response: &SnapshotFileResponse) -> Value {
    json!({
        "path": response.path,
        "chain_id": response.chain_id,
        "head_height": response.head_height,
        "head_hash_hex": encode_hex(&response.head_hash),
        "committed_state_root_hex": encode_hex(&response.committed_state_root),
        "quarantined_block_count": response.quarantined_block_count,
        "snapshot_size_bytes": response.snapshot_size_bytes,
    })
}

pub(crate) fn state_query_to_json(response: &StateQueryResponse) -> Value {
    json!({
        "key_hex": encode_hex(&response.key),
        "value_hex": response.value.as_ref().map(|value| encode_hex(value)),
    })
}

pub(crate) fn state_root_at_height_to_json(response: &StateRootAtHeightResponse) -> Value {
    json!({
        "height": response.height,
        "block_hash_hex": encode_hex(&response.block_hash),
        "state_root_hex": encode_hex(&response.state_root),
    })
}

pub(crate) fn policy_response_to_json(response: &PolicyResponse) -> Value {
    json!({
        "id": response.id,
        "authority_account": response.authority_account,
        "account": response.account,
        "execution_account_id": response.execution_account_id,
        "execution_scope": {
            "kind": match response.execution_scope.kind {
                PolicyExecutionScopeKindResponse::Account => "account",
                PolicyExecutionScopeKindResponse::DelegationTrading => "delegation_trading",
            },
            "delegation_id_hex": response.execution_scope.delegation_id_hex,
        },
        "parent_policy_id": response.parent_policy_id,
        "state": match response.state {
            PolicyState::PendingActivation => "pending_activation",
            PolicyState::Active => "active",
            PolicyState::Suspended => "suspended",
            PolicyState::Revoked => "revoked",
            PolicyState::Expired => "expired",
        },
        "next_intent_nonce": response.next_intent_nonce,
        "delegation_depth": response.delegation_depth,
        "revoked_at": response.revoked_at,
        "terms": {
            "gas_budget_per_day": response.terms.gas_budget_per_day.to_string(),
            "gas_refill_per_day": response.terms.gas_refill_per_day.to_string(),
            "max_intents_per_second": response.terms.max_intents_per_second,
            "max_intents_per_day": response.terms.max_intents_per_day,
            "max_volume_per_day": response.terms.max_volume_per_day.to_string(),
            "max_drawdown_pct": response.terms.max_drawdown_pct,
        },
    })
}

pub(crate) fn account_response_to_json(response: &AccountResponse) -> Value {
    json!({
        "id": response.id,
        "owner_hex": encode_hex(&response.owner),
        "pending_owner_hex": response.pending_owner.as_ref().map(|owner| encode_hex(owner)),
        "created_at": response.created_at,
        "paused": response.paused,
        "frozen": response.frozen,
        "controllers": response.controllers,
        "nonce": response.nonce,
        "code_hash_hex": encode_hex(&response.code_hash),
        "evm_address_hex": response.evm_address.as_ref().map(|address| encode_hex(address)),
    })
}

pub(crate) fn mandate_enforcement_action_name(
    action: kombat_chain_streams::MandateEnforcementAction,
) -> &'static str {
    match action {
        kombat_chain_streams::MandateEnforcementAction::AdmissionRejected => "admission_rejected",
        kombat_chain_streams::MandateEnforcementAction::FillRolledBack => "fill_rolled_back",
        kombat_chain_streams::MandateEnforcementAction::BreachApplied => "breach_applied",
        kombat_chain_streams::MandateEnforcementAction::PoliciesCascadeRevoked => {
            "policies_cascade_revoked"
        }
    }
}

pub(crate) fn parse_mandate_u256(value: &str) -> Result<MandateU256, RpcError> {
    value
        .parse::<u128>()
        .map(MandateU256::from_u128)
        .map_err(|_| RpcError::InvalidRequest("invalid mandate u256 decimal".into()))
}

pub(crate) fn parse_market_maker_u256(
    value: &str,
    _field: &'static str,
) -> Result<MarketMakerU256, RpcError> {
    value
        .parse::<u128>()
        .map(MarketMakerU256::from_u128)
        .map_err(|_| RpcError::InvalidRequest("invalid market maker u256 decimal"))
}

pub(crate) fn market_maker_identity_from_params(
    legacy_account: Option<u64>,
    identity: Option<&MarketMakerIdentityParams>,
) -> Result<MarketMakerIdentity, RpcError> {
    match (legacy_account, identity) {
        (Some(account_id), None) => Ok(MarketMakerIdentity::Account(account_id)),
        (None, Some(MarketMakerIdentityParams::Account { account_id })) => {
            Ok(MarketMakerIdentity::Account(*account_id))
        }
        (
            None,
            Some(MarketMakerIdentityParams::Delegation {
                delegation_id_hex,
                serving_agent,
            }),
        ) => Ok(MarketMakerIdentity::Delegation {
            delegation_id: decode_hex_array::<32>(delegation_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?,
            serving_agent: *serving_agent,
        }),
        (Some(_), Some(_)) => Err(RpcError::InvalidRequest(
            "specify either market_maker or market_maker_identity".into(),
        )),
        (None, None) => Err(RpcError::InvalidRequest(
            "missing market_maker_identity".into(),
        )),
    }
}

pub(crate) fn market_maker_identity_to_json(identity: MarketMakerIdentity) -> Value {
    match identity {
        MarketMakerIdentity::Account(account_id) => json!({
            "kind": "account",
            "account_id": account_id,
        }),
        MarketMakerIdentity::Delegation {
            delegation_id,
            serving_agent,
        } => json!({
            "kind": "delegation",
            "delegation_id_hex": encode_hex(&delegation_id),
            "serving_agent": serving_agent,
        }),
    }
}

pub(crate) fn define_market_maker_program_action_from_params(
    params: &DefineMarketMakerProgramParams,
) -> Result<BlockMarketMakerProgramAction, RpcError> {
    let min_quote_size_micro_usdc = parse_market_maker_u256(
        &params.min_quote_size_micro_usdc,
        "min_quote_size_micro_usdc",
    )?;
    let enrollment_bond_micro_usdc = parse_market_maker_u256(
        &params.enrollment_bond_micro_usdc,
        "enrollment_bond_micro_usdc",
    )?;
    Ok(BlockMarketMakerProgramAction::Define {
        name: params.name.clone(),
        markets: params.markets.iter().copied().collect(),
        min_uptime_bps: params.min_uptime_bps,
        max_spread_bps: params.max_spread_bps,
        min_quote_size_micro_usdc: min_quote_size_micro_usdc.0,
        min_two_sided_windows_per_hour: params.min_two_sided_windows_per_hour,
        rebate_multiplier_units: params.rebate_multiplier_units,
        enrollment_bond_micro_usdc: enrollment_bond_micro_usdc.0,
        compliance_window_ms: params.compliance_window_ms,
        observation_cadence_ms: params.observation_cadence_ms,
    })
}

pub(crate) fn enroll_market_maker_program_action_from_params(
    params: &EnrollMarketMakerProgramParams,
) -> Result<BlockMarketMakerProgramAction, RpcError> {
    let program_id = decode_hex_array::<32>(&params.program_id_hex)
        .map_err(|_| RpcError::InvalidRequest("invalid program_id_hex".into()))?;
    let bond_micro_usdc = parse_market_maker_u256(&params.bond_micro_usdc, "bond_micro_usdc")?;
    let market_maker_identity = market_maker_identity_from_params(
        params.market_maker,
        params.market_maker_identity.as_ref(),
    )?;
    Ok(BlockMarketMakerProgramAction::Enroll {
        market_maker_identity,
        market_id: params.market_id,
        program_id,
        bond_micro_usdc: bond_micro_usdc.0,
    })
}

pub(crate) fn mandate_constraint_from_param(
    param: MandateConstraintParam,
) -> Result<MandateConstraint, RpcError> {
    Ok(match param {
        MandateConstraintParam::MaxNotionalPerMarket { market, limit_usdc } => {
            MandateConstraint::MaxNotionalPerMarket {
                market,
                limit_usdc: parse_mandate_u256(&limit_usdc)?,
            }
        }
        MandateConstraintParam::MaxNotionalTotal { limit_usdc } => {
            MandateConstraint::MaxNotionalTotal {
                limit_usdc: parse_mandate_u256(&limit_usdc)?,
            }
        }
        MandateConstraintParam::MaxConcentrationInMarketPct { market, pct_bps } => {
            MandateConstraint::MaxConcentrationInMarketPct { market, pct_bps }
        }
        MandateConstraintParam::AllowedMarkets { set } => MandateConstraint::AllowedMarkets {
            set: set.into_iter().collect(),
        },
        MandateConstraintParam::MaxLeverage { mult } => MandateConstraint::MaxLeverage { mult },
        MandateConstraintParam::MaxDrawdownPctFromPeak { pct_bps, window_ms } => {
            MandateConstraint::MaxDrawdownPctFromPeak { pct_bps, window_ms }
        }
        MandateConstraintParam::MaxDailyLossUsdc { limit_usdc } => {
            MandateConstraint::MaxDailyLossUsdc {
                limit_usdc: parse_mandate_u256(&limit_usdc)?,
            }
        }
        MandateConstraintParam::NoPositionHeldLongerThan { duration_ms } => {
            MandateConstraint::NoPositionHeldLongerThan { duration_ms }
        }
        MandateConstraintParam::MaxTradeSizeUsdc { limit_usdc } => {
            MandateConstraint::MaxTradeSizeUsdc {
                limit_usdc: parse_mandate_u256(&limit_usdc)?,
            }
        }
        MandateConstraintParam::MaxTradeRatePerMinute { count } => {
            MandateConstraint::MaxTradeRatePerMinute { count }
        }
        MandateConstraintParam::NoSameOperatorCounterparty => {
            MandateConstraint::NoSameOperatorCounterparty
        }
        MandateConstraintParam::AllowedCounterparties { accounts } => {
            MandateConstraint::AllowedCounterparties {
                accounts: accounts.into_iter().collect(),
            }
        }
    })
}

pub(crate) fn mandate_objective_from_param(param: MandateObjectiveParam) -> MandateObjective {
    match param {
        MandateObjectiveParam::TargetSharpe { min, over_ms } => {
            MandateObjective::TargetSharpe { min, over_ms }
        }
        MandateObjectiveParam::TargetReturnPct { min_bps, over_ms } => {
            MandateObjective::TargetReturnPct { min_bps, over_ms }
        }
        MandateObjectiveParam::MaxDrawdownPct { max_bps, over_ms } => {
            MandateObjective::MaxDrawdownPct { max_bps, over_ms }
        }
        MandateObjectiveParam::MinTradeCount { count, over_ms } => {
            MandateObjective::MinTradeCount { count, over_ms }
        }
        MandateObjectiveParam::MaxTradeCount { count, over_ms } => {
            MandateObjective::MaxTradeCount { count, over_ms }
        }
        MandateObjectiveParam::MaxSlippageBps { limit } => {
            MandateObjective::MaxSlippageBps { limit }
        }
        MandateObjectiveParam::MaxFundingPaidPctOfEquity { pct_bps, over_ms } => {
            MandateObjective::MaxFundingPaidPctOfEquity { pct_bps, over_ms }
        }
    }
}

pub(crate) fn mandate_u256_to_decimal(value: MandateU256) -> String {
    U256::from_be_bytes(value.0).to_string()
}

pub(crate) fn mandate_constraint_to_json(constraint: &MandateConstraint) -> Value {
    match constraint {
        MandateConstraint::MaxNotionalPerMarket { market, limit_usdc } => json!({
            "kind": "max_notional_per_market",
            "market": market,
            "limit_usdc": mandate_u256_to_decimal(*limit_usdc),
        }),
        MandateConstraint::MaxNotionalTotal { limit_usdc } => json!({
            "kind": "max_notional_total",
            "limit_usdc": mandate_u256_to_decimal(*limit_usdc),
        }),
        MandateConstraint::MaxConcentrationInMarketPct { market, pct_bps } => json!({
            "kind": "max_concentration_in_market_pct",
            "market": market,
            "pct_bps": pct_bps,
        }),
        MandateConstraint::AllowedMarkets { set } => json!({
            "kind": "allowed_markets",
            "set": set.iter().copied().collect::<Vec<_>>(),
        }),
        MandateConstraint::MaxLeverage { mult } => json!({
            "kind": "max_leverage",
            "mult": mult,
        }),
        MandateConstraint::MaxDrawdownPctFromPeak { pct_bps, window_ms } => json!({
            "kind": "max_drawdown_pct_from_peak",
            "pct_bps": pct_bps,
            "window_ms": window_ms,
        }),
        MandateConstraint::MaxDailyLossUsdc { limit_usdc } => json!({
            "kind": "max_daily_loss_usdc",
            "limit_usdc": mandate_u256_to_decimal(*limit_usdc),
        }),
        MandateConstraint::NoPositionHeldLongerThan { duration_ms } => json!({
            "kind": "no_position_held_longer_than",
            "duration_ms": duration_ms,
        }),
        MandateConstraint::MaxTradeSizeUsdc { limit_usdc } => json!({
            "kind": "max_trade_size_usdc",
            "limit_usdc": mandate_u256_to_decimal(*limit_usdc),
        }),
        MandateConstraint::MaxTradeRatePerMinute { count } => json!({
            "kind": "max_trade_rate_per_minute",
            "count": count,
        }),
        MandateConstraint::NoSameOperatorCounterparty => json!({
            "kind": "no_same_operator_counterparty",
        }),
        MandateConstraint::AllowedCounterparties { accounts } => json!({
            "kind": "allowed_counterparties",
            "accounts": accounts.iter().copied().collect::<Vec<_>>(),
        }),
    }
}

pub(crate) fn mandate_objective_to_json(objective: &MandateObjective) -> Value {
    match objective {
        MandateObjective::TargetSharpe { min, over_ms } => json!({
            "kind": "target_sharpe",
            "min": min,
            "over_ms": over_ms,
        }),
        MandateObjective::TargetReturnPct { min_bps, over_ms } => json!({
            "kind": "target_return_pct",
            "min_bps": min_bps,
            "over_ms": over_ms,
        }),
        MandateObjective::MaxDrawdownPct { max_bps, over_ms } => json!({
            "kind": "max_drawdown_pct",
            "max_bps": max_bps,
            "over_ms": over_ms,
        }),
        MandateObjective::MinTradeCount { count, over_ms } => json!({
            "kind": "min_trade_count",
            "count": count,
            "over_ms": over_ms,
        }),
        MandateObjective::MaxTradeCount { count, over_ms } => json!({
            "kind": "max_trade_count",
            "count": count,
            "over_ms": over_ms,
        }),
        MandateObjective::MaxSlippageBps { limit } => json!({
            "kind": "max_slippage_bps",
            "limit": limit,
        }),
        MandateObjective::MaxFundingPaidPctOfEquity { pct_bps, over_ms } => json!({
            "kind": "max_funding_paid_pct_of_equity",
            "pct_bps": pct_bps,
            "over_ms": over_ms,
        }),
    }
}

pub(crate) fn mandate_response_to_json(response: &MandateResponse) -> Value {
    let mandate = &response.mandate;
    json!({
        "mandate_id_hex": encode_hex(&mandate.id),
        "owner_hex": encode_hex(&mandate.owner),
        "agent": mandate.agent,
        "version": mandate.version,
        "state": mandate_state_name(mandate.state),
        "constraints": mandate.constraints.iter().map(mandate_constraint_to_json).collect::<Vec<_>>(),
        "objectives": mandate.objectives.iter().map(mandate_objective_to_json).collect::<Vec<_>>(),
        "objective_weights": mandate.objective_weights,
        "conformance_window_ms": mandate.conformance_window_ms,
        "policies": mandate.policies,
        "session_keys_hex": mandate
            .session_keys
            .iter()
            .map(|session_key| encode_hex(session_key))
            .collect::<Vec<_>>(),
        "activated_at": mandate.activated_at,
        "expires_at": mandate.expires_at,
        "description_uri": mandate.description_uri,
        "description_hash_hex": mandate.description_hash.map(|hash| encode_hex(&hash)),
        "conformance": {
            "score": mandate.conformance.score,
            "last_evaluated_at_ms": mandate.conformance.last_evaluated_at_ms,
            "pending_objectives": mandate.conformance.pending_objectives,
            "latest_delta": response.latest_delta,
        },
    })
}

pub(crate) fn liquidation_pressure_summary_to_json(
    response: &LiquidationPressureSummaryResponse,
) -> Value {
    json!({
        "market_count": response.market_count,
        "stressed_market_count": response.stressed_market_count,
        "stressed_account_count": response.stressed_account_count,
        "imminent_freeze_market_count": response.imminent_freeze_market_count,
        "imminent_freeze_account_count": response.imminent_freeze_account_count,
        "total_liquidation_candidate_count": response.total_liquidation_candidate_count,
        "total_liquidation_gap": response.total_liquidation_gap.to_string(),
        "worst_market_id": response.worst_market_id,
        "worst_market_gap": response.worst_market_gap.to_string(),
        "worst_account_id": response.worst_account_id,
        "worst_account_gap": response.worst_account_gap.to_string(),
        "suggested_clear_count": response.suggested_clear_count,
        "suggested_still_under_count": response.suggested_still_under_count,
        "suggested_freeze_count": response.suggested_freeze_count,
        "suggested_bad_debt_count": response.suggested_bad_debt_count,
        "total_suggested_bad_debt": response.total_suggested_bad_debt.to_string(),
        "latest_market_risk_transition": response
            .latest_market_risk_transition
            .as_ref()
            .map(latest_market_risk_transition_to_json),
        "recent_market_risk_transitions": response
            .recent_market_risk_transitions
            .iter()
            .map(latest_market_risk_transition_to_json)
            .collect::<Vec<_>>(),
        "top_markets": response
            .top_markets
            .iter()
            .map(market_state_response_to_json)
            .collect::<Vec<_>>(),
        "top_accounts": response
            .top_accounts
            .iter()
            .map(account_risk_summary_to_json)
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn account_summary_matches_outcome(
    summary: &AccountRiskSummaryResponse,
    outcome: SuggestedLiquidationOutcome,
) -> bool {
    match outcome {
        SuggestedLiquidationOutcome::ClearsMaintenance => {
            summary.markets_cleared_by_suggested_liquidation > 0
        }
        SuggestedLiquidationOutcome::StillUnderMaintenance => {
            summary.markets_still_under_after_suggested_liquidation > 0
        }
        SuggestedLiquidationOutcome::FreezesAccount => {
            summary.markets_freezing_after_suggested_liquidation > 0
        }
        SuggestedLiquidationOutcome::CreatesBadDebt => {
            summary.markets_creating_bad_debt_after_suggested_liquidation > 0
        }
    }
}

pub(crate) fn market_state_matches_outcome(
    state: &MarketStateResponse,
    outcome: SuggestedLiquidationOutcome,
) -> bool {
    match outcome {
        SuggestedLiquidationOutcome::ClearsMaintenance => state.suggested_clear_count > 0,
        SuggestedLiquidationOutcome::StillUnderMaintenance => {
            state.liquidation_candidate_count
                > state.suggested_clear_count
                    + state.suggested_freeze_count
                    + state.suggested_bad_debt_count
        }
        SuggestedLiquidationOutcome::FreezesAccount => state.suggested_freeze_count > 0,
        SuggestedLiquidationOutcome::CreatesBadDebt => state.suggested_bad_debt_count > 0,
    }
}

pub(crate) fn suggested_liquidation_outcome_name(
    outcome: SuggestedLiquidationOutcome,
) -> &'static str {
    match outcome {
        SuggestedLiquidationOutcome::ClearsMaintenance => "clears_maintenance",
        SuggestedLiquidationOutcome::StillUnderMaintenance => "still_under_maintenance",
        SuggestedLiquidationOutcome::FreezesAccount => "freezes_account",
        SuggestedLiquidationOutcome::CreatesBadDebt => "creates_bad_debt",
    }
}

pub(crate) fn order_status_response_to_json(response: &OrderStatusResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "client_order_id": response.client_order_id,
        "stage": order_status_stage_name(response.stage),
        "filled_lots": response.filled_lots,
        "remaining_lots": response.remaining_lots,
        "replacement_client_order_id": response.replacement_client_order_id,
        "latest_event_seq": response.latest_event_seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn book_level_to_json(level: &BookLevelResponse) -> Value {
    json!({
        "price_ticks": level.price_ticks,
        "total_lots": level.total_lots,
        "order_count": level.order_count,
    })
}

pub(crate) fn resting_order_response_to_json(order: &RestingOrderResponse) -> Value {
    json!({
        "market_id": order.market_id,
        "account_id": order.account_id,
        "client_order_id": order.client_order_id,
        "side": match order.side {
            OrderSide::Bid => "bid",
            OrderSide::Ask => "ask",
        },
        "price_ticks": order.price_ticks,
        "remaining_lots": order.remaining_lots,
        "expires_at_ms": order.expires_at_ms,
        "sequence": order.sequence,
    })
}

pub(crate) fn candle_interval_ms(interval: &str) -> Option<u64> {
    match interval {
        "1m" => Some(60_000),
        "5m" => Some(5 * 60_000),
        "15m" => Some(15 * 60_000),
        "1h" => Some(60 * 60_000),
        "1d" => Some(24 * 60 * 60_000),
        _ => None,
    }
}

pub(crate) fn match_fill_to_json(fill: &MatchFill) -> Value {
    json!({
        "maker_account_id": fill.maker_account_id,
        "maker_client_order_id": fill.maker_client_order_id,
        "taker_account_id": fill.taker_account_id,
        "taker_client_order_id": fill.taker_client_order_id,
        "price_ticks": fill.price_ticks,
        "quantity_lots": fill.quantity_lots,
    })
}

pub(crate) fn book_level_response(level: BookLevel) -> BookLevelResponse {
    BookLevelResponse {
        price_ticks: level.price_ticks,
        total_lots: level.total_lots,
        order_count: level.order_count,
    }
}

pub(crate) fn resting_order_response(order: RestingOrder) -> RestingOrderResponse {
    RestingOrderResponse {
        market_id: order.market_id,
        account_id: order.account_id,
        client_order_id: order.client_order_id,
        side: order.side,
        price_ticks: order.price_ticks,
        remaining_lots: order.remaining_lots,
        expires_at_ms: order.expires_at_ms,
        sequence: order.sequence,
    }
}

pub(crate) fn submit_order_response(outcome: MatchOutcome) -> SubmitOrderResponse {
    SubmitOrderResponse {
        status: outcome.status,
        fills: outcome.fills,
        filled_lots: outcome.filled_lots,
        remaining_lots: outcome.remaining_lots,
        posted_order_id: outcome.posted_order_id,
    }
}

pub(crate) fn preview_intent_response(preview: OrderInitialMarginPreview) -> PreviewIntentResponse {
    PreviewIntentResponse {
        market_id: preview.market_id,
        account_id: preview.account_id,
        would_succeed: preview.would_succeed,
        reason: preview.reason,
        current_raw_notional: preview.current_raw_notional,
        candidate_raw_notional: preview.candidate_raw_notional,
        current_notional: preview.current_notional,
        candidate_notional: preview.candidate_notional,
        required_initial: preview.required_initial,
        equity_for_market: preview.equity_for_market,
        margin_surplus: preview.margin_surplus,
    }
}

pub(crate) fn submit_intent_response_to_json(response: &SubmitIntentResponse) -> Value {
    json!({
        "intent_id_hex": encode_hex(&response.intent_id),
        "class": match response.class {
            IntentAdmissionClass::Direct => "direct",
            IntentAdmissionClass::Conditional => "conditional",
        },
        "pending_reason": response.pending_reason.map(intent_pending_reason_name),
        "next_eligible_at_ms": response.next_eligible_at_ms,
        "current_policy_nonce": response.current_policy_nonce,
    })
}

pub(crate) fn submit_order_response_to_json(response: &SubmitOrderResponse) -> Value {
    json!({
        "status": match_status_name(response.status),
        "fills": response.fills.iter().map(match_fill_to_json).collect::<Vec<_>>(),
        "filled_lots": response.filled_lots,
        "remaining_lots": response.remaining_lots,
        "posted_order_id": response.posted_order_id,
    })
}

pub(crate) fn preview_intent_response_to_json(response: &PreviewIntentResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "would_succeed": response.would_succeed,
        "reason": response.reason,
        "current_raw_notional": response.current_raw_notional.to_string(),
        "candidate_raw_notional": response.candidate_raw_notional.to_string(),
        "current_notional": response.current_notional.to_string(),
        "candidate_notional": response.candidate_notional.to_string(),
        "required_initial": response.required_initial.to_string(),
        "equity_for_market": response.equity_for_market.to_string(),
        "margin_surplus": response.margin_surplus.to_string(),
    })
}

pub(crate) fn match_status_name(status: MatchStatus) -> &'static str {
    match status {
        MatchStatus::Filled => "filled",
        MatchStatus::PartiallyFilledCancelled => "partially_filled_cancelled",
        MatchStatus::PartiallyFilledResting => "partially_filled_resting",
        MatchStatus::Cancelled => "cancelled",
        MatchStatus::Resting => "resting",
    }
}

pub(crate) fn liquidation_response_to_json(response: &LiquidationResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "liquidator_account_id": response.liquidator_account_id,
        "step_count": response.step_count,
        "status": match_status_name(response.status),
        "fills": response.fills.iter().map(match_fill_to_json).collect::<Vec<_>>(),
        "requested_lots": response.requested_lots,
        "filled_lots": response.filled_lots,
        "remaining_lots": response.remaining_lots,
        "executed_notional": response.executed_notional.to_string(),
        "penalty": response.penalty.to_string(),
        "insurance_cover": response.insurance_cover.to_string(),
        "liquidator_reward": response.liquidator_reward.to_string(),
        "insurance_fund_balance": response.insurance_fund_balance.to_string(),
        "post_net_lots": response.post_net_lots,
        "post_equity": response.post_equity.to_string(),
        "post_maintenance_requirement": response.post_maintenance_requirement.to_string(),
        "post_margin_surplus": response.post_margin_surplus.to_string(),
        "post_under_maintenance": response.post_under_maintenance,
        "post_account_frozen": response.post_account_frozen,
        "post_market_bad_debt_balance": response.post_market_bad_debt_balance.to_string(),
    })
}

pub(crate) fn liquidation_pass_response_to_json(response: &LiquidationPassResponse) -> Value {
    json!({
        "liquidator_account_id": response.liquidator_account_id,
        "scanned_positions": response.scanned_positions,
        "attempted_positions": response.attempted_positions,
        "succeeded_positions": response.succeeded_positions,
        "failed_positions": response.failed_positions,
        "skipped_stale_markets": response.skipped_stale_markets,
        "skipped_frozen_accounts": response.skipped_frozen_accounts,
        "steps": response.steps.iter().map(liquidation_pass_step_to_json).collect::<Vec<_>>(),
    })
}

pub(crate) fn liquidation_pass_step_to_json(response: &LiquidationPassStepResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "client_order_id": response.client_order_id,
        "status": response.status.map(match_status_name),
        "filled_lots": response.filled_lots,
        "remaining_lots": response.remaining_lots,
        "reason": response.reason,
    })
}

pub(crate) fn oracle_history_record_to_json(response: &OracleHistoryRecordResponse) -> Value {
    let kind = match &response.kind {
        OracleHistoryKindResponse::OracleConfigUpdated { max_staleness_ms } => json!({
            "topic": "oracle_config_updated",
            "max_staleness_ms": max_staleness_ms,
        }),
        OracleHistoryKindResponse::MarkPriceUpdated { mark_price_ticks } => json!({
            "topic": "mark_price_updated",
            "mark_price_ticks": mark_price_ticks,
        }),
        OracleHistoryKindResponse::IndexPriceUpdated { index_price_ticks } => json!({
            "topic": "index_price_updated",
            "index_price_ticks": index_price_ticks,
        }),
    };

    json!({
        "market_id": response.market_id,
        "kind": kind,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn market_trading_state_record_to_json(
    response: &MarketTradingStateRecordResponse,
) -> Value {
    json!({
        "market_id": response.market_id,
        "paused": response.paused,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn liquidation_record_to_json(response: &LiquidationRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "liquidator_account_id": response.liquidator_account_id,
        "requested_lots": response.requested_lots,
        "filled_lots": response.filled_lots,
        "remaining_lots": response.remaining_lots,
        "executed_notional": response.executed_notional.to_string(),
        "penalty": response.penalty.to_string(),
        "insurance_cover": response.insurance_cover.to_string(),
        "liquidator_reward": response.liquidator_reward.to_string(),
        "insurance_fund_balance": response.insurance_fund_balance.to_string(),
        "outcome": response.outcome.map(liquidation_execution_outcome_name),
        "post_net_lots": response.post_net_lots,
        "post_margin_surplus": response.post_margin_surplus.map(|value| value.to_string()),
        "post_under_maintenance": response.post_under_maintenance,
        "post_account_frozen": response.post_account_frozen,
        "post_market_bad_debt_balance": response.post_market_bad_debt_balance.map(|value| value.to_string()),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn liquidation_escalation_record_to_json(
    response: &LiquidationEscalationRecordResponse,
) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "action": liquidation_escalation_action_name(response.action),
        "stall_count": response.stall_count,
        "freeze_threshold": response.freeze_threshold,
        "freeze_imminent": response.freeze_imminent,
        "post_margin_surplus": response.post_margin_surplus.to_string(),
        "post_under_maintenance": response.post_under_maintenance,
        "post_account_frozen": response.post_account_frozen,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn insurance_fund_record_to_json(response: &InsuranceFundUpdateRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "delta": response.delta.to_string(),
        "balance": response.balance.to_string(),
        "source": insurance_fund_update_source_name(response.source),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn bad_debt_record_to_json(response: &BadDebtRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "kind": match response.kind {
            BadDebtRecordKind::Updated => "updated",
            BadDebtRecordKind::Recovered => "recovered",
        },
        "delta": response.delta.to_string(),
        "balance": response.balance.to_string(),
        "source": risk_transition_source_name(response.source),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn solvency_breach_record_to_json(response: &SolvencyBreachRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "residual_bad_debt": response.residual_bad_debt.to_string(),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn risk_state_history_record_to_json(
    response: &RiskStateHistoryRecordResponse,
) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "net_lots": response.net_lots,
        "collateral": response.collateral.to_string(),
        "equity": response.equity.to_string(),
        "maintenance_requirement": response.maintenance_requirement.to_string(),
        "liquidation_gap": response.liquidation_gap.to_string(),
        "margin_surplus": response.margin_surplus.to_string(),
        "under_maintenance": response.under_maintenance,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn liquidation_candidate_history_record_to_json(
    response: &LiquidationCandidateHistoryRecordResponse,
) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "net_lots": response.net_lots,
        "equity": response.equity.to_string(),
        "maintenance_requirement": response.maintenance_requirement.to_string(),
        "liquidation_gap": response.liquidation_gap.to_string(),
        "margin_surplus": response.margin_surplus.to_string(),
        "under_maintenance": response.under_maintenance,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn funding_record_to_json(response: &FundingRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "funding_index": response.funding_index.to_string(),
        "funding_delta": response.funding_delta.map(|value| value.to_string()),
        "kind": funding_record_kind_name(response.kind),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn funding_state_response_to_json(response: &FundingStateResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "funding_index": response.funding_index.to_string(),
        "premium_rate_ppm": response.premium_rate_ppm,
        "indicative_funding_rate_ppm": response.indicative_funding_rate_ppm,
        "mark_price_ticks": response.mark_price_ticks,
        "index_price_ticks": response.index_price_ticks,
    })
}

pub(crate) fn insurance_fund_response_to_json(response: &InsuranceFundResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "insurance_fund_balance": response.insurance_fund_balance.to_string(),
        "bad_debt_balance": response.bad_debt_balance.to_string(),
    })
}

pub(crate) fn fee_record_to_json(response: &FeeRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "client_order_id": response.client_order_id,
        "role": order_fill_role_name(response.role),
        "fee_amount": response.fee_amount.to_string(),
        "fee_vault_balance": response.fee_vault_balance.to_string(),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn revenue_split_record_to_json(response: &RevenueSplitRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "source_fee_amount": response.source_fee_amount.to_string(),
        "validator_rewards_delta": response.validator_rewards_delta.to_string(),
        "validator_reward_balance": response.validator_reward_balance.to_string(),
        "market_maker_rebate_delta": response.market_maker_rebate_delta.to_string(),
        "market_maker_rebate_balance": response.market_maker_rebate_balance.to_string(),
        "insurance_fund_delta": response.insurance_fund_delta.to_string(),
        "insurance_fund_balance": response.insurance_fund_balance.to_string(),
        "treasury_delta": response.treasury_delta.to_string(),
        "treasury_balance": response.treasury_balance.to_string(),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn cancel_order_response_to_json(response: &CancelOrderResponse) -> Value {
    json!({
        "cancelled": response.cancelled,
        "order": response.order.as_ref().map(resting_order_response_to_json),
    })
}

pub(crate) fn submit_owner_call_response_to_json(response: &SubmitOwnerCallResponse) -> Value {
    json!({
        "accepted": response.accepted,
        "account_id": response.account_id,
        "chain_id": response.chain_id,
        "nonce": response.nonce,
        "action": response.action,
        "policy_id": response.policy_id,
        "new_owner_hex": response.new_owner.map(|principal| encode_hex(&principal)),
        "code_hash_hex": response.code_hash.map(|hash| encode_hex(&hash)),
    })
}

pub(crate) fn intent_status_to_json(response: &IntentStatusResponse) -> Value {
    json!({
        "intent_id_hex": encode_hex(&response.intent_id),
        "stage": match response.stage {
            IntentStatusStage::Unknown => "unknown",
            IntentStatusStage::AcceptedDirect => "accepted_direct",
            IntentStatusStage::AcceptedConditional => "accepted_conditional",
            IntentStatusStage::Promoted => "promoted",
            IntentStatusStage::Executed => "executed",
            IntentStatusStage::Expired => "expired",
            IntentStatusStage::Rejected => "rejected",
        },
        "rejection_reason": response.rejection_reason.map(intent_rejection_reason_name),
        "order_rejection_reason": response.order_rejection_reason.map(order_rejection_reason_name),
        "pending_reason": response.pending_reason.map(intent_pending_reason_name),
        "next_eligible_at_ms": response.next_eligible_at_ms,
        "current_policy_nonce": response.current_policy_nonce,
        "policy_id": response.policy_id,
        "nonce": response.nonce,
        "currently_queued": response.currently_queued,
        "latest_event_seq": response.latest_event_seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn intent_pending_reason_name(reason: IntentPendingReason) -> &'static str {
    match reason {
        IntentPendingReason::TimeAfter => "time_after",
        IntentPendingReason::NonceFrontier => "nonce_frontier",
        IntentPendingReason::MarketTradingPaused => "market_trading_paused",
        IntentPendingReason::StaleMarketData => "stale_market_data",
    }
}

pub(crate) fn account_update_action_name(action: AccountUpdateAction) -> &'static str {
    match action {
        AccountUpdateAction::ProposeOwner => "propose_owner",
        AccountUpdateAction::AcceptOwner => "accept_owner",
        AccountUpdateAction::Freeze => "freeze",
        AccountUpdateAction::Unfreeze => "unfreeze",
        AccountUpdateAction::Pause => "pause",
        AccountUpdateAction::Unpause => "unpause",
        AccountUpdateAction::AddPolicy => "add_policy",
        AccountUpdateAction::RemovePolicy => "remove_policy",
        AccountUpdateAction::Upgrade => "upgrade",
    }
}

pub(crate) fn insurance_fund_update_source_name(
    source: kombat_chain_streams::InsuranceFundUpdateSource,
) -> &'static str {
    match source {
        kombat_chain_streams::InsuranceFundUpdateSource::LiquidationPenalty => {
            "liquidation_penalty"
        }
        kombat_chain_streams::InsuranceFundUpdateSource::BankruptcyCover => "bankruptcy_cover",
    }
}

pub(crate) fn real_capital_insurance_wallet_spend_source_name(
    source: kombat_chain_streams::RealCapitalInsuranceWalletSpendSource,
) -> &'static str {
    match source {
        kombat_chain_streams::RealCapitalInsuranceWalletSpendSource::KeeperReward => {
            "keeper_reward"
        }
        kombat_chain_streams::RealCapitalInsuranceWalletSpendSource::ForcedLiquidationCost => {
            "forced_liquidation_cost"
        }
        kombat_chain_streams::RealCapitalInsuranceWalletSpendSource::BadDebtCover => {
            "bad_debt_cover"
        }
        kombat_chain_streams::RealCapitalInsuranceWalletSpendSource::UserPayout => "user_payout",
    }
}

pub(crate) fn risk_transition_source_name(
    source: kombat_chain_streams::RiskTransitionSource,
) -> &'static str {
    match source {
        kombat_chain_streams::RiskTransitionSource::LiquidationSolvency => "liquidation_solvency",
        kombat_chain_streams::RiskTransitionSource::LiquidationStall => "liquidation_stall",
        kombat_chain_streams::RiskTransitionSource::CollateralAdjustment => "collateral_adjustment",
        kombat_chain_streams::RiskTransitionSource::InsuranceSweep => "insurance_sweep",
        kombat_chain_streams::RiskTransitionSource::MarkPriceReprice => "mark_price_reprice",
        kombat_chain_streams::RiskTransitionSource::FundingUpdate => "funding_update",
        kombat_chain_streams::RiskTransitionSource::RiskParameterUpdate => "risk_parameter_update",
    }
}

pub(crate) fn latest_risk_transition_to_json(response: &LatestRiskTransitionResponse) -> Value {
    json!({
        "kind": risk_transition_kind_name(response.kind),
        "source": risk_transition_source_name(response.source),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn latest_market_risk_transition_to_json(
    response: &LatestMarketRiskTransitionResponse,
) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "kind": risk_transition_kind_name(response.kind),
        "source": risk_transition_source_name(response.source),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn risk_transition_kind_name(kind: RiskTransitionKindResponse) -> &'static str {
    match kind {
        RiskTransitionKindResponse::Freeze => "freeze",
        RiskTransitionKindResponse::Unfreeze => "unfreeze",
        RiskTransitionKindResponse::BadDebtUpdated => "bad_debt_updated",
        RiskTransitionKindResponse::BadDebtRecovered => "bad_debt_recovered",
    }
}

pub(crate) fn liquidation_execution_outcome_name(
    outcome: LiquidationExecutionOutcome,
) -> &'static str {
    match outcome {
        LiquidationExecutionOutcome::ClearedMaintenance => "cleared_maintenance",
        LiquidationExecutionOutcome::StillUnderMaintenance => "still_under_maintenance",
        LiquidationExecutionOutcome::FrozeAccount => "froze_account",
        LiquidationExecutionOutcome::CreatesBadDebt => "creates_bad_debt",
    }
}

pub(crate) fn liquidation_escalation_action_name(
    action: LiquidationEscalationAction,
) -> &'static str {
    match action {
        LiquidationEscalationAction::Advanced => "advanced",
        LiquidationEscalationAction::Reset => "reset",
        LiquidationEscalationAction::FrozeAccount => "froze_account",
    }
}

pub(crate) fn funding_record_kind_name(kind: FundingRecordKind) -> &'static str {
    match kind {
        FundingRecordKind::IndexUpdated => "index_updated",
        FundingRecordKind::Applied => "applied",
    }
}

pub(crate) fn intent_rejection_reason_name(reason: IntentRejectionReason) -> &'static str {
    match reason {
        IntentRejectionReason::UnknownPolicy => "unknown_policy",
        IntentRejectionReason::PolicyNotActive => "policy_not_active",
        IntentRejectionReason::PolicyExpired => "policy_expired",
        IntentRejectionReason::AccountFrozen => "account_frozen",
        IntentRejectionReason::MarketTradingPaused => "market_trading_paused",
        IntentRejectionReason::StaleMarketData => "stale_market_data",
        IntentRejectionReason::InvalidIntentNonce => "invalid_intent_nonce",
        IntentRejectionReason::InvalidSessionSignature => "invalid_session_signature",
        IntentRejectionReason::SessionPolicyMismatch => "session_policy_mismatch",
        IntentRejectionReason::SessionChainMismatch => "session_chain_mismatch",
        IntentRejectionReason::SessionKeyRevoked => "session_key_revoked",
        IntentRejectionReason::UnauthorizedGrantee => "unauthorized_grantee",
        IntentRejectionReason::GasBudgetExceeded => "gas_budget_exceeded",
        IntentRejectionReason::IntentRateLimitExceeded => "intent_rate_limit_exceeded",
        IntentRejectionReason::MinuteIntentLimitExceeded => "minute_intent_limit_exceeded",
        IntentRejectionReason::DailyIntentLimitExceeded => "daily_intent_limit_exceeded",
        IntentRejectionReason::DailyVolumeExceeded => "daily_volume_exceeded",
        IntentRejectionReason::MarketNotAllowed => "market_not_allowed",
        IntentRejectionReason::TradeNotionalExceeded => "trade_notional_exceeded",
        IntentRejectionReason::NotionalOutstandingExceeded => "notional_outstanding_exceeded",
        IntentRejectionReason::MandateConstraintViolated => "mandate_constraint_violated",
        IntentRejectionReason::DrawdownExceededAndRevoked => "drawdown_exceeded_and_revoked",
        IntentRejectionReason::SubPolicyScopeMismatch => "sub_policy_scope_mismatch",
        IntentRejectionReason::SubPolicyTermsExceedParent => "sub_policy_terms_exceed_parent",
        IntentRejectionReason::DelegationDepthExceeded => "delegation_depth_exceeded",
        IntentRejectionReason::Overflow => "overflow",
        IntentRejectionReason::InternalError => "internal_error",
        IntentRejectionReason::PredecessorIntentRejected => "predecessor_intent_rejected",
    }
}

pub(crate) fn order_rejection_reason_name(reason: OrderRejectionReason) -> &'static str {
    match reason {
        OrderRejectionReason::ZeroMarketId => "zero_market_id",
        OrderRejectionReason::ZeroAccountId => "zero_account_id",
        OrderRejectionReason::ZeroQuantity => "zero_quantity",
        OrderRejectionReason::ZeroClientOrderId => "zero_client_order_id",
        OrderRejectionReason::ZeroLimitPrice => "zero_limit_price",
        OrderRejectionReason::MarketMismatch => "market_mismatch",
        OrderRejectionReason::MarketOrderRequiresImmediateTif => {
            "market_order_requires_immediate_tif"
        }
        OrderRejectionReason::PostOnlyRequiresLimitOrder => "post_only_requires_limit_order",
        OrderRejectionReason::PostOnlyIncompatibleWithImmediateTif => {
            "post_only_incompatible_with_immediate_tif"
        }
        OrderRejectionReason::GttRequiresFutureExpiry => "gtt_requires_future_expiry",
        OrderRejectionReason::PostOnlyWouldCross => "post_only_would_cross",
        OrderRejectionReason::FillOrKillInsufficientLiquidity => {
            "fill_or_kill_insufficient_liquidity"
        }
        OrderRejectionReason::SelfTradePrevented => "self_trade_prevented",
        OrderRejectionReason::DuplicateClientOrderId => "duplicate_client_order_id",
        OrderRejectionReason::UnknownRestingOrder => "unknown_resting_order",
        OrderRejectionReason::ReplaceAccountMismatch => "replace_account_mismatch",
    }
}

pub(crate) fn order_reject_reason_name(reason: OrderRejectReason) -> &'static str {
    match reason {
        OrderRejectReason::ZeroMarketId => "zero_market_id",
        OrderRejectReason::ZeroAccountId => "zero_account_id",
        OrderRejectReason::ZeroQuantity => "zero_quantity",
        OrderRejectReason::ZeroClientOrderId => "zero_client_order_id",
        OrderRejectReason::ZeroLimitPrice => "zero_limit_price",
        OrderRejectReason::MarketMismatch { .. } => "market_mismatch",
        OrderRejectReason::MarketOrderRequiresImmediateTif => "market_order_requires_immediate_tif",
        OrderRejectReason::PostOnlyRequiresLimitOrder => "post_only_requires_limit_order",
        OrderRejectReason::PostOnlyIncompatibleWithImmediateTif => {
            "post_only_incompatible_with_immediate_tif"
        }
        OrderRejectReason::GttRequiresFutureExpiry { .. } => "gtt_requires_future_expiry",
        OrderRejectReason::PostOnlyWouldCross => "post_only_would_cross",
        OrderRejectReason::FillOrKillInsufficientLiquidity => "fill_or_kill_insufficient_liquidity",
        OrderRejectReason::SelfTradePrevented => "self_trade_prevented",
        OrderRejectReason::DuplicateClientOrderId => "duplicate_client_order_id",
        OrderRejectReason::UnknownRestingOrder => "unknown_resting_order",
        OrderRejectReason::ReplaceAccountMismatch { .. } => "replace_account_mismatch",
    }
}

pub(crate) fn order_lifecycle_status_name(status: OrderLifecycleStatus) -> &'static str {
    match status {
        OrderLifecycleStatus::Filled => "filled",
        OrderLifecycleStatus::PartiallyFilledCancelled => "partially_filled_cancelled",
        OrderLifecycleStatus::PartiallyFilledResting => "partially_filled_resting",
        OrderLifecycleStatus::Cancelled => "cancelled",
        OrderLifecycleStatus::Resting => "resting",
    }
}

pub(crate) fn order_status_stage_name(stage: OrderStatusStage) -> &'static str {
    match stage {
        OrderStatusStage::Unknown => "unknown",
        OrderStatusStage::Resting => "resting",
        OrderStatusStage::Filled => "filled",
        OrderStatusStage::PartiallyFilledResting => "partially_filled_resting",
        OrderStatusStage::PartiallyFilledCancelled => "partially_filled_cancelled",
        OrderStatusStage::Cancelled => "cancelled",
        OrderStatusStage::Replaced => "replaced",
    }
}

pub(crate) fn order_status_stage_from_lifecycle(status: OrderLifecycleStatus) -> OrderStatusStage {
    match status {
        OrderLifecycleStatus::Filled => OrderStatusStage::Filled,
        OrderLifecycleStatus::PartiallyFilledCancelled => {
            OrderStatusStage::PartiallyFilledCancelled
        }
        OrderLifecycleStatus::PartiallyFilledResting => OrderStatusStage::PartiallyFilledResting,
        OrderLifecycleStatus::Cancelled => OrderStatusStage::Cancelled,
        OrderLifecycleStatus::Resting => OrderStatusStage::Resting,
    }
}

pub(crate) fn order_fill_role_name(role: OrderFillRole) -> &'static str {
    match role {
        OrderFillRole::Maker => "maker",
        OrderFillRole::Taker => "taker",
    }
}

pub(crate) fn order_fill_side_name(side: OrderFillSide) -> &'static str {
    match side {
        OrderFillSide::Buy => "buy",
        OrderFillSide::Sell => "sell",
    }
}

pub(crate) fn position_side_name(side: PositionSide) -> &'static str {
    match side {
        PositionSide::Flat => "flat",
        PositionSide::Long => "long",
        PositionSide::Short => "short",
    }
}

pub(crate) fn chain_info_to_json(response: &ChainInfoResponse) -> Value {
    json!({
        "chain_id": response.chain_id,
        "source_network_id": response.source_network_id,
        "head_height": response.head_height,
        "head_timestamp_ms": response.head_timestamp_ms,
        "head_hash_hex": encode_hex(&response.head_hash),
        "committed_state_root_hex": encode_hex(&response.committed_state_root),
        "quarantined_block_count": response.quarantined_block_count,
    })
}

pub(crate) fn source_network_id_for_chain_id(chain_id: u64) -> String {
    format!("nockchain-{chain_id}")
}

pub(crate) fn consensus_log_kind_to_str(
    kind: kombat_chain_node::ConsensusLogEventKind,
) -> &'static str {
    match kind {
        kombat_chain_node::ConsensusLogEventKind::PeerCountUpdated => "peer_count_updated",
        kombat_chain_node::ConsensusLogEventKind::RoundStarted => "round_started",
        kombat_chain_node::ConsensusLogEventKind::BlockProposed => "block_proposed",
        kombat_chain_node::ConsensusLogEventKind::BlockCommitted => "block_committed",
        kombat_chain_node::ConsensusLogEventKind::TimeoutObserved => "timeout_observed",
        kombat_chain_node::ConsensusLogEventKind::ViewChanged => "view_changed",
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn consensus_metrics_to_json(
    response: &kombat_chain_node::ConsensusMetricsSnapshot,
    frontier: &kombat_chain_node::ProposerFrontierStats,
    engine_lock_profile: &kombat_chain_node::EngineLockProfileSnapshot,
    network_round_lock_profile: &NetworkRoundLockProfileSnapshot,
    mahi: Option<&MahiStateSnapshot>,
    reachability: Option<&NetworkPeerReachability>,
    peer_sessions: &[PeerTransportSessionSnapshot],
    peer_backoff: &[PeerTransportEndpointBackoffSnapshot],
    inbound_peer_sessions: &[InboundPeerTransportSessionSnapshot],
    peer_snapshot_repair: &PeerSnapshotRepairStateSnapshot,
    rpc_submit_metrics: &RpcSubmitPathMetrics,
    submit_queues: &SubmitQueueStatsSnapshot,
) -> Value {
    let reachable_peer_count = reachability.map(|summary| summary.reachable_peer_count);
    let unreachable_peer_urls = reachability
        .map(|summary| summary.unreachable_peer_urls.clone())
        .unwrap_or_default();
    json!({
        "current_round": response.current_round,
        "current_view": response.current_view,
        "last_committed_round": response.last_committed_round,
        "timeouts_total": response.timeouts_total,
        "view_changes_total": response.view_changes_total,
        "phase6_resting_batch": {
            "attempts": response.phase6_resting_batch_attempts,
            "accepted": response.phase6_resting_batch_accepted,
            "items": response.phase6_resting_batch_items,
            "unsupported": response.phase6_resting_batch_unsupported,
            "rejected": response.phase6_resting_batch_rejected,
            "gather_ns": response.phase6_resting_batch_gather_ns,
            "precheck_ns": response.phase6_resting_batch_precheck_ns,
            "admission_ns": response.phase6_resting_batch_admission_ns,
            "policy_ns": response.phase6_resting_batch_policy_ns,
            "matching_ns": response.phase6_resting_batch_matching_ns,
            "event_ns": response.phase6_resting_batch_event_ns,
        },
        "peer_count": response.peer_count,
        "reachable_peer_count": reachable_peer_count,
        "unreachable_peer_urls": unreachable_peer_urls,
        "peer_transport_session_count": peer_sessions.len(),
        "peer_transport_sessions": peer_sessions.iter().map(|session| json!({
            "endpoint": session.endpoint,
            "age_ms": session.age_ms,
            "idle_ms": session.idle_ms,
            "request_count": session.request_count,
            "remote_session_id": session.remote_session_id,
            "remote_chain_id": session.remote_chain_id,
            "remote_validator_id_hex": session.remote_validator_id_hex,
            "remote_current_round": session.remote_current_round,
            "remote_current_view": session.remote_current_view,
            "remote_head_height": session.remote_head_height,
            "remote_head_hash_hex": session.remote_head_hash_hex,
            "remote_committed_state_root_hex": session.remote_committed_state_root_hex,
            "remote_last_committed_round": session.remote_last_committed_round,
            "remote_peer_count": session.remote_peer_count,
        })).collect::<Vec<_>>(),
        "peer_transport_endpoint_backoff": peer_backoff.iter().map(|endpoint| json!({
            "endpoint": endpoint.endpoint,
            "consecutive_failures": endpoint.consecutive_failures,
            "retry_in_ms": endpoint.retry_in_ms,
            "last_error": endpoint.last_error,
        })).collect::<Vec<_>>(),
        "inbound_peer_transport_session_count": inbound_peer_sessions.len(),
        "inbound_peer_transport_sessions": inbound_peer_sessions.iter().map(|session| json!({
            "session_id": session.session_id,
            "remote_addr": session.remote_addr,
            "age_ms": session.age_ms,
            "idle_ms": session.idle_ms,
            "request_count": session.request_count,
            "remote_chain_id": session.remote_chain_id,
            "remote_validator_id_hex": session.remote_validator_id_hex,
            "remote_current_round": session.remote_current_round,
            "remote_current_view": session.remote_current_view,
            "remote_head_height": session.remote_head_height,
            "remote_head_hash_hex": session.remote_head_hash_hex,
            "remote_committed_state_root_hex": session.remote_committed_state_root_hex,
            "remote_last_committed_round": session.remote_last_committed_round,
            "remote_peer_count": session.remote_peer_count,
        })).collect::<Vec<_>>(),
        "peer_snapshot_repair": {
            "stalled_head_height": peer_snapshot_repair.stalled_head_height,
            "stalled_head_hash_hex": peer_snapshot_repair.stalled_head_hash_hex,
            "stalled_for_ms": peer_snapshot_repair.stalled_for_ms,
            "next_retry_in_ms": peer_snapshot_repair.next_retry_in_ms,
            "attempts_total": peer_snapshot_repair.attempts_total,
            "repaired_total": peer_snapshot_repair.repaired_total,
            "repair_err_total": peer_snapshot_repair.repair_err_total,
            "last_error": peer_snapshot_repair.last_error,
            "last_repaired_head_height": peer_snapshot_repair.last_repaired_head_height,
            "last_repaired_head_hash_hex": peer_snapshot_repair.last_repaired_head_hash_hex,
        },
        "rpc_submit_metrics": {
            "local_submit_total": rpc_submit_metrics.local_submit_total,
            "local_submit_ok": rpc_submit_metrics.local_submit_ok,
            "local_submit_err": rpc_submit_metrics.local_submit_err,
            "forwarded_submit_total": rpc_submit_metrics.forwarded_submit_total,
            "forwarded_submit_ok": rpc_submit_metrics.forwarded_submit_ok,
            "forwarded_submit_err": rpc_submit_metrics.forwarded_submit_err,
            "timeout_recovery_total": rpc_submit_metrics.timeout_recovery_total,
            "timeout_recovery_ok": rpc_submit_metrics.timeout_recovery_ok,
            "timeout_recovery_err": rpc_submit_metrics.timeout_recovery_err,
            "timeout_recovery_attempts_on_forwarded_submit": rpc_submit_metrics.timeout_recovery_attempts_on_forwarded_submit,
            "local_submit_latency_ms": duration_summary_to_json(&rpc_submit_metrics.local_submit_ms),
            "forwarded_submit_latency_ms": duration_summary_to_json(&rpc_submit_metrics.forwarded_submit_ms),
            "leader_preflight_latency_ms": duration_summary_to_json(&rpc_submit_metrics.leader_preflight_ms),
            "leader_forward_latency_ms": duration_summary_to_json(&rpc_submit_metrics.leader_forward_ms),
            "timeout_recovery_latency_ms": duration_summary_to_json(&rpc_submit_metrics.timeout_recovery_ms),
            "forwarded_timeout_recovery_latency_ms": duration_summary_to_json(&rpc_submit_metrics.forwarded_timeout_recovery_ms),
        },
        "submit_queue_stats": {
            "local_submit_queue_items": submit_queues.local_submit_queue_items,
            "pending_relay_intents": submit_queues.pending_relay_intents,
            "pending_relay_targets": submit_queues.pending_relay_targets,
            "pending_mahi_block_broadcasts": submit_queues.pending_mahi_block_broadcasts,
            "mahi_pre_admitted_pool_items": submit_queues.mahi_pre_admitted_pool_items,
            "mahi_pre_admitted_pool_accepted_total": submit_queues.mahi_pre_admitted_pool_accepted_total,
            "mahi_pre_admitted_pool_duplicate_total": submit_queues.mahi_pre_admitted_pool_duplicate_total,
            "mahi_pre_admitted_pool_drained_total": submit_queues.mahi_pre_admitted_pool_drained_total,
            "mahi_pre_admitted_pool_dropped_oldest_total": submit_queues.mahi_pre_admitted_pool_dropped_oldest_total,
            "mahi_pre_admitted_pool_dropped_stale_total": submit_queues.mahi_pre_admitted_pool_dropped_stale_total,
        },
        "phase1_lock_profile": {
            "engine": {
                "compat_raw": lock_timing_summary_to_json(&engine_lock_profile.compat_raw),
                "admission_read": lock_timing_summary_to_json(&engine_lock_profile.admission_read),
                "admission_write": lock_timing_summary_to_json(&engine_lock_profile.admission_write),
            },
            "network_round": {
                "head_open": json!({
                    "wait_ns": nanos_summary_to_json(&network_round_lock_profile.head_open_wait_ns),
                    "hold_ns": nanos_summary_to_json(&network_round_lock_profile.head_open_hold_ns),
                }),
                "build": json!({
                    "wait_ns": nanos_summary_to_json(&network_round_lock_profile.build_wait_ns),
                    "hold_ns": nanos_summary_to_json(&network_round_lock_profile.build_hold_ns),
                }),
            },
        },
        "proposer_frontier": {
            "direct_mempool_items": frontier.direct_mempool_items,
            "conditional_mempool_items": frontier.conditional_mempool_items,
            "tracked_policy_count": frontier.tracked_policy_count,
            "tracked_ready_items": frontier.tracked_ready_items,
            "largest_tracked_policy_ready_items": frontier.largest_tracked_policy_ready_items,
            "stale_policy_count": frontier.stale_policy_count,
            "stale_ready_items": frontier.stale_ready_items,
        },
        "mahi": mahi_to_json(mahi),
        "qc_formation_ms": response.qc_formation_ms,
        "round_duration_ms": response.round_duration_ms,
        "block_size_bytes": response.block_size_bytes,
    })
}

fn mahi_to_json(snapshot: Option<&MahiStateSnapshot>) -> Value {
    let Some(snapshot) = snapshot else {
        return json!({ "enabled": false });
    };
    let mut value = json!({
        "enabled": snapshot.enabled,
        "epoch": snapshot.epoch,
        "commit_rule": snapshot.commit_rule,
        "next_round": snapshot.next_round,
        "next_leader_hex": encode_hex(&snapshot.next_leader),
        "block_count": snapshot.block_count,
        "max_round": snapshot.max_round,
        "pending_fetch_count": snapshot.pending_fetch_count,
        "decided_leader_count": snapshot.decided_leader_count,
        "committed_leader_count": snapshot.committed_leader_count,
        "skipped_leader_count": snapshot.skipped_leader_count,
        "committed_block_count": snapshot.committed_block_count,
        "last_decided_round": snapshot.last_decided_round,
        "last_decided_author_hex": encode_hex(&snapshot.last_decided_author),
        "commit_log_hash_hex": encode_hex(&snapshot.commit_log_hash),
        "producer_attempts": snapshot.producer_attempts,
        "producer_skip_ancestor_quorum": snapshot.producer_skip_ancestor_quorum,
        "producer_skip_peer_reachability": snapshot.producer_skip_peer_reachability,
        "producer_skip_snapshot_unavailable": snapshot.producer_skip_snapshot_unavailable,
        "producer_skip_empty_idle": snapshot.producer_skip_empty_idle,
        "producer_skip_round_race": snapshot.producer_skip_round_race,
        "produced_blocks": snapshot.produced_blocks,
        "produced_empty_blocks": snapshot.produced_empty_blocks,
        "produced_intent_count": snapshot.produced_intent_count,
        "received_blocks": snapshot.received_blocks,
        "received_intent_count": snapshot.received_intent_count,
        "execution_pending_count": snapshot.execution_pending_count,
        "executed_block_count": snapshot.executed_block_count,
        "executed_intent_count": snapshot.executed_intent_count,
        "verification_pending_count": snapshot.verification_pending_count,
        "verified_block_count": snapshot.verified_block_count,
        "verification_failed_count": snapshot.verification_failed_count,
        "dropped_blocks": snapshot.dropped_blocks,
        "sigverify_pool_depth": snapshot.sigverify_pool_depth,
        "sigverify_batch_hist": duration_summary_to_json(&snapshot.sigverify_batch_hist),
        "sigverify_per_intent_verified_total": snapshot.sigverify_per_intent_verified_total,
        "sigverify_failures_total": snapshot.sigverify_failures_total,
        "last_produced_round": snapshot.last_produced_round,
        "last_produced_hash_hex": snapshot.last_produced_hash.map(|hash| encode_hex(&hash)),
        "dag_snapshot_hash_hex": encode_hex(&snapshot.dag_snapshot_hash),
        "last_update_age_ms": snapshot.last_update_age_ms,
    });
    value["restart_boundary_round"] = json!(snapshot.restart_boundary_round);
    if let Some(object) = value.as_object_mut() {
        object.insert("pipeline".into(), json!(snapshot.pipeline));
        object.insert(
            "number_of_leaders".into(),
            json!(snapshot.number_of_leaders),
        );
        object.insert(
            "producer_ancestor_wait_round".into(),
            json!(snapshot.producer_ancestor_wait_round),
        );
        object.insert(
            "producer_ancestor_wait_available".into(),
            json!(snapshot.producer_ancestor_wait_available),
        );
        object.insert(
            "producer_ancestor_wait_quorum".into(),
            json!(snapshot.producer_ancestor_wait_quorum),
        );
        object.insert(
            "producer_ancestor_wait_age_ms".into(),
            json!(snapshot.producer_ancestor_wait_age_ms),
        );
        object.insert(
            "producer_ancestor_wait_first_arrival_age_ms".into(),
            json!(snapshot.producer_ancestor_wait_first_arrival_age_ms),
        );
        object.insert(
            "producer_ancestor_wait_latest_arrival_age_ms".into(),
            json!(snapshot.producer_ancestor_wait_latest_arrival_age_ms),
        );
        object.insert(
            "producer_ancestor_wait_author_hexes".into(),
            json!(snapshot
                .producer_ancestor_wait_authors
                .iter()
                .map(|author| encode_hex(author))
                .collect::<Vec<_>>()),
        );
        object.insert(
            "producer_ancestor_wait_missing_author_hexes".into(),
            json!(snapshot
                .producer_ancestor_wait_missing_authors
                .iter()
                .map(|author| encode_hex(author))
                .collect::<Vec<_>>()),
        );
        object.insert(
            "tail_round_block_counts".into(),
            json!(snapshot
                .tail_round_block_counts
                .iter()
                .map(|(round, block_count)| json!({
                    "round": round,
                    "block_count": block_count,
                }))
                .collect::<Vec<_>>()),
        );
        object.insert(
            "producer_initial_lock_wait_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_initial_lock_wait_ms),
        );
        object.insert(
            "producer_initial_lock_hold_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_initial_lock_hold_ms),
        );
        object.insert(
            "producer_snapshot_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_snapshot_ms),
        );
        object.insert(
            "producer_pre_admitted_drain_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_pre_admitted_drain_ms),
        );
        object.insert(
            "producer_final_lock_wait_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_final_lock_wait_ms),
        );
        object.insert(
            "producer_final_lock_hold_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_final_lock_hold_ms),
        );
        object.insert(
            "producer_total_latency_ms".into(),
            duration_summary_to_json(&snapshot.producer_total_ms),
        );
        object.insert(
            "inbound_block_decode_latency_ms".into(),
            duration_summary_to_json(&snapshot.inbound_block_decode_ms),
        );
        object.insert(
            "inbound_block_accept_latency_ms".into(),
            duration_summary_to_json(&snapshot.inbound_block_accept_ms),
        );
        object.insert(
            "inbound_block_sigverify_enqueue_latency_ms".into(),
            duration_summary_to_json(&snapshot.inbound_block_sigverify_enqueue_ms),
        );
        object.insert(
            "inbound_block_total_latency_ms".into(),
            duration_summary_to_json(&snapshot.inbound_block_total_ms),
        );
        object.insert(
            "commit_executor_wake_count".into(),
            json!(snapshot.commit_executor_wake_count),
        );
        object.insert(
            "commit_executor_empty_wake_count".into(),
            json!(snapshot.commit_executor_empty_wake_count),
        );
        object.insert(
            "commit_execution_batches".into(),
            json!(snapshot.commit_execution_batches),
        );
        object.insert(
            "commit_execution_blocks".into(),
            json!(snapshot.commit_execution_blocks),
        );
        object.insert(
            "commit_execution_intents".into(),
            json!(snapshot.commit_execution_intents),
        );
        object.insert(
            "commit_execution_batch_latency_ms".into(),
            duration_summary_to_json(&snapshot.commit_execution_batch_ms),
        );
        object.insert(
            "phase6_schedule_profile_enabled".into(),
            json!(phase6_schedule_profile_enabled()),
        );
        object.insert(
            "phase6_schedule_profile_sample_every".into(),
            json!(phase6_schedule_profile_sample_every()),
        );
        object.insert(
            "phase6_schedule_profile_max_items".into(),
            json!(phase6_schedule_profile_max_items()),
        );
        object.insert(
            "phase6_schedule_profiled_batches".into(),
            json!(snapshot.phase6_schedule_profiled_batches),
        );
        object.insert(
            "phase6_schedule_truncated_batches".into(),
            json!(snapshot.phase6_schedule_truncated_batches),
        );
        object.insert(
            "phase6_schedule_total_item_count".into(),
            sample_summary_to_json(&snapshot.phase6_schedule_total_item_count),
        );
        object.insert(
            "phase6_schedule_item_count".into(),
            sample_summary_to_json(&snapshot.phase6_schedule_item_count),
        );
        object.insert(
            "phase6_schedule_layer_count".into(),
            sample_summary_to_json(&snapshot.phase6_schedule_layer_count),
        );
        object.insert(
            "phase6_schedule_max_layer_width".into(),
            sample_summary_to_json(&snapshot.phase6_schedule_max_layer_width),
        );
        object.insert(
            "phase6_schedule_conflict_pair_count".into(),
            sample_summary_to_json(&snapshot.phase6_schedule_conflict_pair_count),
        );
        object.insert(
            "phase6_schedule_parallelism_milli".into(),
            sample_summary_to_json(&snapshot.phase6_schedule_parallelism_milli),
        );
    }
    value
}

pub(crate) fn consensus_logs_to_json(response: &[kombat_chain_node::ConsensusLogEntry]) -> Value {
    json!(response
        .iter()
        .map(|entry| json!({
            "kind": consensus_log_kind_to_str(entry.kind),
            "round": entry.round,
            "height": entry.height,
            "validator_id_hex": entry.validator_id.as_ref().map(|validator_id| encode_hex(validator_id)),
            "timestamp_ms": entry.timestamp_ms,
            "peer_count": entry.peer_count,
        }))
        .collect::<Vec<_>>())
}
