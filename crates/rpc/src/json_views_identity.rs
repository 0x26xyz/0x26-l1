use crate::*;

pub(crate) fn agent_identity_record_response(
    record: &AgentIdentityRecord,
    active_attestation_count: usize,
) -> AgentIdentityResponse {
    AgentIdentityResponse {
        account: record.account,
        tier: identity_tier_response(record.tier),
        operator: record.operator,
        kind: agent_kind_name(record.kind),
        display_name: record.display_name.clone(),
        description: record.description.clone(),
        contact_uri: record.contact_uri.clone(),
        active_attestation_count,
        registered_at_block: Some(record.registered_at_block),
        registered_at_timestamp_ms: Some(record.registered_at_timestamp_ms),
        last_updated_block: Some(record.last_updated_block),
        last_updated_timestamp_ms: Some(record.last_updated_timestamp_ms),
        virtual_default: false,
    }
}

pub(crate) fn agent_identity_response_to_json(response: &AgentIdentityResponse) -> Value {
    json!({
        "account": response.account,
        "tier": identity_tier_name(response.tier),
        "operator": response.operator,
        "kind": response.kind,
        "display_name": response.display_name,
        "description": response.description,
        "contact_uri": response.contact_uri,
        "active_attestation_count": response.active_attestation_count,
        "registered_at_block": response.registered_at_block,
        "registered_at_timestamp_ms": response.registered_at_timestamp_ms,
        "last_updated_block": response.last_updated_block,
        "last_updated_timestamp_ms": response.last_updated_timestamp_ms,
        "virtual_default": response.virtual_default,
    })
}

pub(crate) fn attestation_list_response_to_json(response: &AttestationListResponse) -> Value {
    json!({
        "items": response.items.iter().map(attestation_record_response_to_json).collect::<Vec<_>>(),
        "next_cursor": response.next_cursor,
        "scaffold": response.scaffold,
    })
}

pub(crate) fn attestation_record_response_to_json(response: &AttestationRecordResponse) -> Value {
    json!({
        "record_id_hex": response.record_id_hex,
        "account": response.account,
        "kind": response.kind,
        "attestor": response.attestor,
        "claim": response.claim,
        "evidence": response.evidence,
        "issued_at_block": response.issued_at_block,
        "issued_at_timestamp_ms": response.issued_at_timestamp_ms,
        "expires_at_timestamp_ms": response.expires_at_timestamp_ms,
        "revoked_at_block": response.revoked_at_block,
        "verified": response.verified,
    })
}

pub(crate) fn attestation_record_response(record: &AttestationRecord) -> AttestationRecordResponse {
    AttestationRecordResponse {
        record_id_hex: encode_hex(&record.record_id),
        account: record.account,
        kind: attestation_kind_name(record.kind),
        attestor: record.attestor,
        claim: attestation_claim_to_json(&record.claim),
        evidence: attestation_evidence_to_json(record.evidence),
        issued_at_block: record.issued_at_block,
        issued_at_timestamp_ms: record.issued_at_timestamp_ms,
        expires_at_timestamp_ms: record.expires_at_timestamp_ms,
        revoked_at_block: record.revoked_at_block,
        verified: record.verified,
    }
}

pub(crate) fn composition_access_response_to_json(response: &CompositionAccessResponse) -> Value {
    json!({
        "account": response.account,
        "performance_share_delegator": response.performance_share_delegator,
        "performance_share_delegatee": response.performance_share_delegatee,
        "reputation_collateral": response.reputation_collateral,
        "a2a_contract": response.a2a_contract,
    })
}

pub(crate) fn reputation_multiplier_response_to_json(
    response: &ReputationMultiplierResponse,
) -> Value {
    json!({
        "account": response.account,
        "multiplier": response.multiplier,
    })
}

pub(crate) fn vendor_roots_response_to_json(response: &VendorRootsResponse) -> Value {
    json!({
        "vendor": response.vendor.map(tee_vendor_name),
        "roots": response.roots.iter().map(vendor_root_record_response_to_json).collect::<Vec<_>>(),
        "pending_governance_actions": response.pending_governance_actions
            .iter()
            .map(pending_vendor_governance_action_response_to_json)
            .collect::<Vec<_>>(),
        "tee_attestation_enabled": response.tee_attestation_enabled,
        "scaffold": response.scaffold,
    })
}

pub(crate) fn vendor_root_record_response_to_json(response: &VendorRootRecordResponse) -> Value {
    json!({
        "vendor": tee_vendor_name(response.vendor),
        "root_id_hex": response.root_id_hex,
        "valid_from_block": response.valid_from_block,
        "valid_until_block": response.valid_until_block,
    })
}

pub(crate) fn pending_vendor_governance_action_response_to_json(
    response: &PendingVendorGovernanceActionResponse,
) -> Value {
    match response {
        PendingVendorGovernanceActionResponse::IntroduceVendorRoot {
            vendor,
            root_id_hex,
            activates_at_block,
            governance_action_hex,
        } => json!({
            "kind": "introduce_vendor_root",
            "vendor": tee_vendor_name(*vendor),
            "root_id_hex": root_id_hex,
            "activates_at_block": activates_at_block,
            "governance_action_hex": governance_action_hex,
        }),
        PendingVendorGovernanceActionResponse::RevokeVendorRoot {
            vendor,
            root_id_hex,
            deactivates_at_block,
            governance_action_hex,
        } => json!({
            "kind": "revoke_vendor_root",
            "vendor": tee_vendor_name(*vendor),
            "root_id_hex": root_id_hex,
            "deactivates_at_block": deactivates_at_block,
            "governance_action_hex": governance_action_hex,
        }),
        PendingVendorGovernanceActionResponse::UpdateVendorCapProfile {
            vendor,
            cap_multiplier,
            composition_eligibility,
            activates_at_block,
            governance_action_hex,
        } => json!({
            "kind": "update_vendor_cap_profile",
            "vendor": tee_vendor_name(*vendor),
            "cap_multiplier": cap_multiplier,
            "composition_eligibility": composition_eligibility,
            "activates_at_block": activates_at_block,
            "governance_action_hex": governance_action_hex,
        }),
    }
}

pub(crate) fn pending_vendor_root_governance_actions(
    root: &VendorRoot,
    head_height: u64,
) -> Vec<PendingVendorGovernanceActionResponse> {
    let mut actions = Vec::new();
    if root.valid_from_block > head_height {
        actions.push(PendingVendorGovernanceActionResponse::IntroduceVendorRoot {
            vendor: tee_vendor_from_node(root.vendor),
            root_id_hex: encode_hex(&root.root_id),
            activates_at_block: root.valid_from_block,
            governance_action_hex: encode_hex(&root.introduced_by_governance_action),
        });
    }
    if let Some(valid_until_block) = root.valid_until_block {
        if valid_until_block > head_height {
            actions.push(PendingVendorGovernanceActionResponse::RevokeVendorRoot {
                vendor: tee_vendor_from_node(root.vendor),
                root_id_hex: encode_hex(&root.root_id),
                deactivates_at_block: valid_until_block,
                governance_action_hex: root
                    .revoked_by_governance_action
                    .map(|action_id| encode_hex(&action_id)),
            });
        }
    }
    actions
}

pub(crate) fn pending_vendor_cap_profile_update_response(
    update: &ScheduledTeeVendorCapProfileUpdate,
) -> PendingVendorGovernanceActionResponse {
    PendingVendorGovernanceActionResponse::UpdateVendorCapProfile {
        vendor: tee_vendor_from_node(update.vendor),
        cap_multiplier: tee_cap_multiplier_decimal_string(update.cap_multiplier_microunits),
        composition_eligibility: update.composition_eligibility,
        activates_at_block: update.valid_from_block,
        governance_action_hex: encode_hex(&update.governance_action),
    }
}

pub(crate) fn vendor_cap_profile_response_to_json(response: &VendorCapProfileResponse) -> Value {
    json!({
        "vendor": tee_vendor_name(response.vendor),
        "cap_multiplier": response.cap_multiplier,
        "composition_eligibility": response.composition_eligibility,
        "tee_attestation_enabled": response.tee_attestation_enabled,
        "scaffold": response.scaffold,
    })
}

pub(crate) fn vendor_root_record_response(root: &VendorRoot) -> VendorRootRecordResponse {
    VendorRootRecordResponse {
        vendor: tee_vendor_from_node(root.vendor),
        root_id_hex: encode_hex(&root.root_id),
        valid_from_block: root.valid_from_block,
        valid_until_block: root.valid_until_block,
    }
}

pub(crate) fn vendor_cap_profile_response(
    profile: &VendorCapProfile,
    tee_attestation_enabled: bool,
) -> VendorCapProfileResponse {
    VendorCapProfileResponse {
        vendor: tee_vendor_from_node(profile.vendor),
        cap_multiplier: profile.cap_multiplier_decimal_string(),
        composition_eligibility: profile.composition_eligibility,
        tee_attestation_enabled,
        scaffold: false,
    }
}

pub(crate) fn account_history_record_to_json(response: &AccountHistoryRecordResponse) -> Value {
    let kind = match &response.kind {
        AccountHistoryKindResponse::Created { owner } => json!({
            "topic": "account_created",
            "owner_hex": encode_hex(owner),
        }),
        AccountHistoryKindResponse::Updated {
            action,
            reason,
            owner,
            pending_owner,
            paused,
            nonce,
            code_hash,
            controllers,
        } => json!({
            "topic": "account_updated",
            "action": account_update_action_name(*action),
            "reason": reason.map(risk_transition_source_name),
            "owner_hex": encode_hex(owner),
            "pending_owner_hex": pending_owner.map(|principal| encode_hex(&principal)),
            "paused": paused,
            "nonce": nonce,
            "code_hash_hex": encode_hex(code_hash),
            "controllers": controllers,
        }),
    };

    json!({
        "account_id": response.account_id,
        "kind": kind,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn order_book_response_to_json(response: &OrderBookResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "bids": response.bids.iter().map(book_level_to_json).collect::<Vec<_>>(),
        "asks": response.asks.iter().map(book_level_to_json).collect::<Vec<_>>(),
    })
}

pub(crate) fn trade_fill_response_to_json(response: &TradeFillResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "client_order_id": response.client_order_id,
        "counterparty_account_id": response.counterparty_account_id,
        "counterparty_client_order_id": response.counterparty_client_order_id,
        "mandate_id_hex": response.mandate_id.as_ref().map(|id| encode_hex(id)),
        "role": order_fill_role_name(response.role),
        "side": order_fill_side_name(response.side),
        "price_ticks": response.price_ticks,
        "quantity_lots": response.quantity_lots,
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn candle_response_to_json(response: &CandleResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "interval": response.interval,
        "timestamp_ms": response.timestamp_ms,
        "open": response.open,
        "high": response.high,
        "low": response.low,
        "close": response.close,
        "volume_lots": response.volume_lots,
        "trade_count": response.trade_count,
        "first_seq": response.first_seq,
        "last_seq": response.last_seq,
    })
}

pub(crate) fn market_snapshot_response_to_json(response: &MarketSnapshotResponse) -> Value {
    json!({
        "market": response.market.as_ref().map(perpetual_market_response_to_json),
        "state": market_state_response_to_json(&response.state),
        "order_book": order_book_response_to_json(&response.order_book),
        "latest_trade": response.latest_trade.as_ref().map(trade_fill_response_to_json),
        "next_seq": response.next_seq,
    })
}

pub(crate) fn account_trading_state_response_to_json(
    response: &AccountTradingStateResponse,
) -> Value {
    json!({
        "account_id": response.account_id,
        "market_id": response.market_id,
        "positions": response.positions.iter().map(position_response_to_json).collect::<Vec<_>>(),
        "open_orders": response.open_orders.iter().map(resting_order_response_to_json).collect::<Vec<_>>(),
        "risk_summary": account_risk_summary_to_json(&response.risk_summary),
        "recent_fills": response.recent_fills.iter().map(trade_fill_response_to_json).collect::<Vec<_>>(),
        "next_seq": response.next_seq,
    })
}

pub(crate) fn position_response_to_json(response: &PositionResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "mandate_id_hex": response.mandate_id.as_ref().map(|id| encode_hex(id)),
        "side": position_side_name(response.side),
        "net_lots": response.net_lots,
        "open_notional": response.open_notional.to_string(),
        "realized_pnl": response.realized_pnl.to_string(),
        "average_entry_price_ticks": response.average_entry_price_ticks,
        "buy_lots": response.buy_lots,
        "sell_lots": response.sell_lots,
        "buy_notional": response.buy_notional.to_string(),
        "sell_notional": response.sell_notional.to_string(),
        "last_fill_seq": response.last_fill_seq,
        "last_fill_timestamp_ms": response.last_fill_timestamp_ms,
    })
}

pub(crate) fn position_history_record_to_json(response: &PositionHistoryRecordResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "net_lots": response.net_lots,
        "open_notional": response.open_notional.to_string(),
        "realized_pnl": response.realized_pnl.to_string(),
        "average_entry_price_ticks": response.average_entry_price_ticks,
        "buy_lots": response.buy_lots,
        "sell_lots": response.sell_lots,
        "buy_notional": response.buy_notional.to_string(),
        "sell_notional": response.sell_notional.to_string(),
        "seq": response.seq,
        "block_height": response.block_height,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn risk_snapshot_response_to_json(response: &RiskSnapshotResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "side": position_side_name(response.side),
        "net_lots": response.net_lots,
        "max_liquidatable_lots": response.max_liquidatable_lots,
        "liquidation_stall_count": response.liquidation_stall_count,
        "liquidation_freeze_threshold": response.liquidation_freeze_threshold,
        "liquidation_freeze_imminent": response.liquidation_freeze_imminent,
        "suggested_liquidation_lots": response.suggested_liquidation_lots,
        "suggested_liquidation_outcome": response.suggested_liquidation_outcome.map(suggested_liquidation_outcome_name),
        "suggested_liquidation_clears_maintenance": response.suggested_liquidation_clears_maintenance,
        "suggested_post_margin_surplus": response.suggested_post_margin_surplus.map(|value| value.to_string()),
        "suggested_post_under_maintenance": response.suggested_post_under_maintenance,
        "suggested_post_account_frozen": response.suggested_post_account_frozen,
        "suggested_post_market_bad_debt_balance": response.suggested_post_market_bad_debt_balance.map(|value| value.to_string()),
        "open_notional": response.open_notional.to_string(),
        "realized_pnl": response.realized_pnl.to_string(),
        "average_entry_price_ticks": response.average_entry_price_ticks,
        "collateral": response.collateral.to_string(),
        "mark_price_ticks": response.mark_price_ticks,
        "mark_notional": response.mark_notional.to_string(),
        "unrealized_pnl": response.unrealized_pnl.to_string(),
        "equity": response.equity.to_string(),
        "maintenance_margin_bps": response.maintenance_margin_bps,
        "maintenance_requirement": response.maintenance_requirement.to_string(),
        "liquidation_gap": response.liquidation_gap.to_string(),
        "margin_surplus": response.margin_surplus.to_string(),
        "under_maintenance": response.under_maintenance,
        "latest_account_risk_transition": response
            .latest_account_risk_transition
            .as_ref()
            .map(latest_risk_transition_to_json),
        "last_fill_seq": response.last_fill_seq,
        "last_fill_timestamp_ms": response.last_fill_timestamp_ms,
    })
}

pub(crate) fn suggested_liquidation_plan_response_to_json(
    response: &SuggestedLiquidationPlanResponse,
) -> Value {
    json!({
        "market_id": response.market_id,
        "account_id": response.account_id,
        "current_liquidation_gap": response.current_liquidation_gap.to_string(),
        "suggested_liquidation_lots": response.suggested_liquidation_lots,
        "suggested_liquidation_outcome": suggested_liquidation_outcome_name(response.suggested_liquidation_outcome),
        "clears_maintenance": response.clears_maintenance,
        "post_margin_surplus": response.post_margin_surplus.to_string(),
        "post_under_maintenance": response.post_under_maintenance,
        "post_account_frozen": response.post_account_frozen,
        "post_market_bad_debt_balance": response.post_market_bad_debt_balance.to_string(),
    })
}

pub(crate) fn account_risk_summary_to_json(response: &AccountRiskSummaryResponse) -> Value {
    json!({
        "account_id": response.account_id,
        "market_count": response.market_count,
        "frozen": response.frozen,
        "latest_account_risk_transition": response
            .latest_account_risk_transition
            .as_ref()
            .map(latest_risk_transition_to_json),
        "max_liquidation_stall_count": response.max_liquidation_stall_count,
        "imminent_freeze_market_count": response.imminent_freeze_market_count,
        "worst_market_id": response.worst_market_id,
        "worst_liquidation_gap": response.worst_liquidation_gap.to_string(),
        "markets_cleared_by_suggested_liquidation": response.markets_cleared_by_suggested_liquidation,
        "markets_still_under_after_suggested_liquidation": response.markets_still_under_after_suggested_liquidation,
        "markets_freezing_after_suggested_liquidation": response.markets_freezing_after_suggested_liquidation,
        "markets_creating_bad_debt_after_suggested_liquidation": response.markets_creating_bad_debt_after_suggested_liquidation,
        "total_suggested_bad_debt": response.total_suggested_bad_debt.to_string(),
        "total_bad_debt": response.total_bad_debt.to_string(),
        "total_collateral": response.total_collateral.to_string(),
        "total_realized_pnl": response.total_realized_pnl.to_string(),
        "total_unrealized_pnl": response.total_unrealized_pnl.to_string(),
        "total_equity": response.total_equity.to_string(),
        "total_open_notional": response.total_open_notional.to_string(),
        "total_initial_margin_required": response.total_initial_margin_required.to_string(),
        "total_maintenance_requirement": response.total_maintenance_requirement.to_string(),
        "total_liquidation_gap": response.total_liquidation_gap.to_string(),
        "total_margin_surplus": response.total_margin_surplus.to_string(),
        "under_maintenance_market_count": response.under_maintenance_market_count,
    })
}

pub(crate) fn account_pnl_window_to_json(response: &AccountPnlWindowResponse) -> Value {
    json!({
        "account_id": response.account_id,
        "market_id": response.market_id,
        "from_ms": response.from_ms,
        "to_ms": response.to_ms,
        "total_realized_pnl": response.total_realized_pnl.to_string(),
        "funding_paid": response.funding_paid.to_string(),
        "trading_realized_pnl": response.trading_realized_pnl.to_string(),
        "fees_paid": response.fees_paid.to_string(),
        "fill_count": response.fill_count,
        "fee_count": response.fee_count,
        "funding_count": response.funding_count,
        "first_seq": response.first_seq,
        "last_seq": response.last_seq,
    })
}

pub(crate) fn account_equity_sample_to_json(sample: &AccountEquitySample) -> Value {
    json!({
        "block_height": sample.block_height,
        "timestamp_ms": sample.timestamp_ms,
        "total_equity": sample.total_equity.to_string(),
        "total_collateral": sample.total_collateral.to_string(),
        "total_realized_pnl": sample.total_realized_pnl.to_string(),
        "total_unrealized_pnl": sample.total_unrealized_pnl.to_string(),
        "total_maintenance_requirement": sample.total_maintenance_requirement.to_string(),
        "total_liquidation_gap": sample.total_liquidation_gap.to_string(),
        "total_margin_surplus": sample.total_margin_surplus.to_string(),
        "position_count": sample.position_count,
        "under_maintenance": sample.under_maintenance,
    })
}

pub(crate) fn account_equity_history_to_json(response: &AccountEquityHistoryResponse) -> Value {
    json!({
        "account_id": response.account_id,
        "from_ms": response.from_ms,
        "to_ms": response.to_ms,
        "samples": response
            .samples
            .iter()
            .map(account_equity_sample_to_json)
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn mandate_conformance_sample_to_json(sample: &MandateConformanceSample) -> Value {
    json!({
        "mandate_id_hex": encode_hex(&sample.mandate_id),
        "block_height": sample.block_height,
        "evaluated_at_ms": sample.evaluated_at_ms,
        "score": sample.score,
        "pending_objectives": sample.pending_objectives,
        "state": mandate_state_name(sample.state),
        "prev_score": sample.prev_score,
        "delta": sample.delta,
    })
}

pub(crate) fn mandate_conformance_history_to_json(
    response: &MandateConformanceHistoryResponse,
) -> Value {
    json!({
        "mandate_id_hex": encode_hex(&response.mandate_id),
        "samples": response
            .samples
            .iter()
            .map(mandate_conformance_sample_to_json)
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn market_state_response_to_json(response: &MarketStateResponse) -> Value {
    json!({
        "market_id": response.market_id,
        "trading_paused": response.trading_paused,
        "liquidation_candidate_count": response.liquidation_candidate_count,
        "imminent_freeze_count": response.imminent_freeze_count,
        "max_liquidation_stall_count": response.max_liquidation_stall_count,
        "total_liquidation_gap": response.total_liquidation_gap.to_string(),
        "worst_liquidation_gap": response.worst_liquidation_gap.to_string(),
        "suggested_clear_count": response.suggested_clear_count,
        "suggested_freeze_count": response.suggested_freeze_count,
        "suggested_bad_debt_count": response.suggested_bad_debt_count,
        "total_suggested_bad_debt": response.total_suggested_bad_debt.to_string(),
        "oracle_max_staleness_ms": response.oracle_max_staleness_ms,
        "order_price_inputs_ready": response.order_price_inputs_ready,
        "liquidation_price_inputs_ready": response.liquidation_price_inputs_ready,
        "mark_price_ticks": response.mark_price_ticks,
        "mark_updated_at_ms": response.mark_updated_at_ms,
        "mark_price_stale": response.mark_price_stale,
        "index_price_ticks": response.index_price_ticks,
        "index_updated_at_ms": response.index_updated_at_ms,
        "index_price_stale": response.index_price_stale,
        "premium_rate_ppm": response.premium_rate_ppm,
        "indicative_funding_rate_ppm": response.indicative_funding_rate_ppm,
        "funding_index": response.funding_index.to_string(),
        "maker_fee_bps": response.maker_fee_bps,
        "maker_rebate_bps": response.maker_rebate_bps,
        "taker_fee_bps": response.taker_fee_bps,
        "fee_vault_balance": response.fee_vault_balance.to_string(),
        "validator_reward_pool_balance": response.validator_reward_pool_balance.to_string(),
        "market_maker_rebate_pool_balance": response.market_maker_rebate_pool_balance.to_string(),
        "treasury_balance": response.treasury_balance.to_string(),
        "maintenance_margin_bps": response.maintenance_margin_bps,
        "liquidation_penalty_bps": response.liquidation_penalty_bps,
        "liquidator_fee_bps": response.liquidator_fee_bps,
        "insurance_fund_balance": response.insurance_fund_balance.to_string(),
        "bad_debt_balance": response.bad_debt_balance.to_string(),
        "open_interest_lots": response.open_interest_lots.to_string(),
        "latest_market_risk_transition": response
            .latest_market_risk_transition
            .as_ref()
            .map(latest_market_risk_transition_to_json),
        "recent_market_risk_transitions": response
            .recent_market_risk_transitions
            .iter()
            .map(latest_market_risk_transition_to_json)
            .collect::<Vec<_>>(),
        "top_liquidation_accounts": response
            .top_liquidation_accounts
            .iter()
            .map(account_risk_summary_to_json)
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn oracle_feed_aggregation_to_json(response: &OracleFeedAggregation) -> Value {
    json!({
        "market_id": response.market_id,
        "oracle_feed_id": response.oracle_feed_id,
        "price_ticks": response.price_ticks,
        "submitted_samples": response.submitted_samples,
        "accepted_samples": response.accepted_samples,
        "rejected_samples": response.rejected_samples,
        "expected_feeders": response.expected_feeders,
        "missing_feeders": response.missing_feeders,
        "timestamp_ms": response.timestamp_ms,
    })
}

pub(crate) fn perpetual_market_response_to_json(response: &PerpetualMarketResponse) -> Value {
    let market = &response.market;
    json!({
        "market_id": market.market_id,
        "kind": "perpetual",
        "status": market_status_name(market.status),
        "base_symbol": market.base_symbol,
        "quote_asset": market.quote_asset,
        "oracle_feed_id": market.oracle_feed_id,
        "initial_margin_bps": market.config.initial_margin_bps,
        "maintenance_margin_bps": market.config.maintenance_margin_bps,
        "max_leverage": market.config.max_leverage,
        "lot_size_microunits": market.config.lot_size_microunits,
        "tick_size_bps": market.config.tick_size_bps,
        "funding_interval_ms": market.config.funding_interval_ms,
        "oracle_max_staleness_ms": market.config.oracle_max_staleness_ms,
        "breaker_price_band_bps": market.config.breaker_price_band_bps,
        "oracle_min_sources": market.config.oracle_min_sources,
        "oracle_min_confidence": market.config.oracle_min_confidence,
        "oracle_min_samples": market.config.oracle_min_samples,
        "oracle_feeder_ids_hex": oracle_feeder_ids_to_json(&market.config),
        "listed_at_ms": market.listed_at_ms,
        "updated_at_ms": market.updated_at_ms,
    })
}

pub(crate) fn market_status_name(status: MarketStatus) -> &'static str {
    match status {
        MarketStatus::Active => "active",
        MarketStatus::Paused => "paused",
        MarketStatus::ReduceOnly => "reduce_only",
        MarketStatus::Delisted => "delisted",
    }
}

pub(crate) fn market_lifecycle_status_name(status: MarketLifecycleStatus) -> &'static str {
    match status {
        MarketLifecycleStatus::Active => "active",
        MarketLifecycleStatus::Paused => "paused",
        MarketLifecycleStatus::ReduceOnly => "reduce_only",
        MarketLifecycleStatus::Delisted => "delisted",
    }
}

pub(crate) fn parse_market_status(value: &str) -> Option<MarketStatus> {
    match value {
        "active" => Some(MarketStatus::Active),
        "paused" => Some(MarketStatus::Paused),
        "reduce_only" => Some(MarketStatus::ReduceOnly),
        "delisted" => Some(MarketStatus::Delisted),
        _ => None,
    }
}

pub(crate) fn mandate_state_name(state: MandateState) -> &'static str {
    match state {
        MandateState::Draft => "draft",
        MandateState::Active => "active",
        MandateState::Breached => "breached",
        MandateState::Suspended => "suspended",
        MandateState::Expired => "expired",
        MandateState::Revoked => "revoked",
    }
}

pub(crate) fn parse_mandate_state(value: &str) -> Option<MandateState> {
    match value {
        "draft" => Some(MandateState::Draft),
        "active" => Some(MandateState::Active),
        "breached" => Some(MandateState::Breached),
        "suspended" => Some(MandateState::Suspended),
        "expired" => Some(MandateState::Expired),
        "revoked" => Some(MandateState::Revoked),
        _ => None,
    }
}

pub(crate) fn identity_tier_name(tier: IdentityTierResponse) -> &'static str {
    match tier {
        IdentityTierResponse::Tier0Unattested => "tier0_unattested",
        IdentityTierResponse::Tier1OperatorAttested => "tier1_operator_attested",
        IdentityTierResponse::Tier2TeeAttested => "tier2_tee_attested",
    }
}

pub(crate) fn real_capital_mode_name(mode: VaultRealCapitalMode) -> &'static str {
    match mode {
        VaultRealCapitalMode::Disabled => "disabled",
        VaultRealCapitalMode::CappedPilot => "capped_pilot",
        VaultRealCapitalMode::Public => "public",
    }
}

pub(crate) fn parse_real_capital_mode(value: &str) -> Option<VaultRealCapitalMode> {
    match value {
        "disabled" => Some(VaultRealCapitalMode::Disabled),
        "capped_pilot" => Some(VaultRealCapitalMode::CappedPilot),
        "public" => Some(VaultRealCapitalMode::Public),
        _ => None,
    }
}

pub(crate) fn block_real_capital_mode_from_name(value: &str) -> Option<BlockVaultRealCapitalMode> {
    match value {
        "disabled" => Some(BlockVaultRealCapitalMode::Disabled),
        "capped_pilot" => Some(BlockVaultRealCapitalMode::CappedPilot),
        "public" => Some(BlockVaultRealCapitalMode::Public),
        _ => None,
    }
}

pub(crate) fn block_identity_tier_from_name(value: &str) -> Option<BlockIdentityTier> {
    match value {
        "tier0_unattested" => Some(BlockIdentityTier::Tier0Unattested),
        "tier1_operator_attested" => Some(BlockIdentityTier::Tier1OperatorAttested),
        "tier2_tee_attested" => Some(BlockIdentityTier::Tier2TeeAttested),
        _ => None,
    }
}

pub(crate) fn parse_real_capital_micro_usdc(
    value: &str,
    field: &'static str,
) -> Result<u128, RpcError> {
    value
        .parse::<u128>()
        .map_err(|_| RpcError::InvalidRequest(field))
}

pub(crate) fn real_capital_governance_action_from_json(
    action: RealCapitalGovernanceActionJson,
) -> Result<BlockRealCapitalGovernanceAction, RpcError> {
    match action {
        RealCapitalGovernanceActionJson::UpsertListing {
            vault_id_hex,
            delegation_id_hex,
            serving_agent,
            mode,
            pilot_min_serving_agent_tier,
            tier0_pilot_exception,
            deposit_enabled,
            listed_for_discovery,
            per_principal_micro_usdc,
            total_vault_tvl_micro_usdc,
            pilot_principal_allowlist,
        } => {
            let vault_id = decode_hex_array::<16>(&vault_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid vault_id_hex"))?;
            let delegation_id = decode_hex_array::<32>(&delegation_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex"))?;
            let mode = block_real_capital_mode_from_name(&mode)
                .ok_or(RpcError::InvalidRequest("invalid real-capital mode"))?;
            let pilot_min_serving_agent_tier = match pilot_min_serving_agent_tier {
                None => None,
                Some(value) => Some(block_identity_tier_from_name(&value).ok_or(
                    RpcError::InvalidRequest("invalid pilot_min_serving_agent_tier"),
                )?),
            };
            Ok(BlockRealCapitalGovernanceAction::UpsertListing {
                vault_id,
                delegation_id,
                serving_agent,
                mode,
                pilot_min_serving_agent_tier,
                tier0_pilot_exception,
                deposit_enabled,
                listed_for_discovery,
                per_principal_micro_usdc: parse_real_capital_micro_usdc(
                    &per_principal_micro_usdc,
                    "invalid per_principal_micro_usdc",
                )?,
                total_vault_tvl_micro_usdc: parse_real_capital_micro_usdc(
                    &total_vault_tvl_micro_usdc,
                    "invalid total_vault_tvl_micro_usdc",
                )?,
                pilot_principal_allowlist,
            })
        }
        RealCapitalGovernanceActionJson::SetListingDepositEnabled {
            vault_id_hex,
            deposit_enabled,
        } => Ok(BlockRealCapitalGovernanceAction::SetListingDepositEnabled {
            vault_id: decode_hex_array::<16>(&vault_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid vault_id_hex"))?,
            deposit_enabled,
        }),
        RealCapitalGovernanceActionJson::SetListingDiscovery {
            vault_id_hex,
            listed_for_discovery,
        } => Ok(BlockRealCapitalGovernanceAction::SetListingDiscovery {
            vault_id: decode_hex_array::<16>(&vault_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid vault_id_hex"))?,
            listed_for_discovery,
        }),
        RealCapitalGovernanceActionJson::UpdateProtocolCap {
            total_protocol_tvl_micro_usdc,
        } => {
            let total_protocol_tvl_micro_usdc = total_protocol_tvl_micro_usdc
                .as_deref()
                .map(|value| {
                    parse_real_capital_micro_usdc(value, "invalid total_protocol_tvl_micro_usdc")
                })
                .transpose()?;
            Ok(BlockRealCapitalGovernanceAction::UpdateProtocolCap {
                total_protocol_tvl_micro_usdc,
            })
        }
        RealCapitalGovernanceActionJson::UpdateGlobalKillSwitch { disabled } => {
            Ok(BlockRealCapitalGovernanceAction::UpdateGlobalKillSwitch { disabled })
        }
    }
}

pub(crate) fn real_capital_listing_mode_name(mode: RealCapitalListingMode) -> &'static str {
    match mode {
        RealCapitalListingMode::Disabled => "disabled",
        RealCapitalListingMode::CappedPilot => "capped_pilot",
        RealCapitalListingMode::Public => "public",
    }
}

pub(crate) fn identity_tier_for_activation(value: u8) -> Result<IdentityTierResponse, RpcError> {
    match value {
        0 => Ok(IdentityTierResponse::Tier0Unattested),
        1 => Ok(IdentityTierResponse::Tier1OperatorAttested),
        2 => Ok(IdentityTierResponse::Tier2TeeAttested),
        _ => Err(RpcError::InvalidRequest("invalid activation_tier".into())),
    }
}

pub(crate) fn identity_tier_rank(tier: IdentityTierResponse) -> u8 {
    match tier {
        IdentityTierResponse::Tier0Unattested => 0,
        IdentityTierResponse::Tier1OperatorAttested => 1,
        IdentityTierResponse::Tier2TeeAttested => 2,
    }
}

pub(crate) fn identity_tier_response(tier: AgentIdentityTier) -> IdentityTierResponse {
    match tier {
        AgentIdentityTier::Tier0Unattested => IdentityTierResponse::Tier0Unattested,
        AgentIdentityTier::Tier1OperatorAttested => IdentityTierResponse::Tier1OperatorAttested,
        AgentIdentityTier::Tier2TeeAttested => IdentityTierResponse::Tier2TeeAttested,
    }
}

pub(crate) fn agent_kind_name(kind: AgentKind) -> &'static str {
    kind.as_str()
}

pub(crate) fn parse_agent_kind(value: &str) -> Option<AgentKind> {
    match value {
        "unknown" => Some(AgentKind::Unknown),
        _ => None,
    }
}

pub(crate) fn identity_tier_status_name(tier: IdentityTierStatus) -> &'static str {
    match tier {
        IdentityTierStatus::Tier0Unattested => "tier0_unattested",
        IdentityTierStatus::Tier1OperatorAttested => "tier1_operator_attested",
        IdentityTierStatus::Tier2TeeAttested => "tier2_tee_attested",
    }
}

pub(crate) fn identity_agent_kind_name(kind: IdentityAgentKind) -> &'static str {
    match kind {
        IdentityAgentKind::Unknown => "unknown",
    }
}

pub(crate) fn identity_attestation_kind_name(kind: IdentityAttestationKind) -> &'static str {
    match kind {
        IdentityAttestationKind::OperatorSelf => "operator_self",
        IdentityAttestationKind::TeeEnclave => "tee_enclave",
        IdentityAttestationKind::ThirdParty => "third_party",
    }
}

pub(crate) fn identity_attestation_claim_to_json(claim: &IdentityAttestationClaim) -> Value {
    match claim {
        IdentityAttestationClaim::RunsCodeHash {
            code_hash,
            model_hash,
        } => json!({
            "kind": "runs_code_hash",
            "code_hash_hex": encode_hex(code_hash),
            "model_hash_hex": model_hash.as_ref().map(|hash| encode_hex(hash)),
        }),
        IdentityAttestationClaim::BoundToMandate { mandate_id } => json!({
            "kind": "bound_to_mandate",
            "mandate_id_hex": encode_hex(mandate_id),
        }),
        IdentityAttestationClaim::RunsCodeAndBoundToMandate {
            code_hash,
            model_hash,
            mandate_id,
        } => json!({
            "kind": "runs_code_and_bound_to_mandate",
            "code_hash_hex": encode_hex(code_hash),
            "model_hash_hex": model_hash.as_ref().map(|hash| encode_hex(hash)),
            "mandate_id_hex": encode_hex(mandate_id),
        }),
    }
}

pub(crate) fn identity_attestation_vendor_name(vendor: IdentityAttestationVendor) -> &'static str {
    match vendor {
        IdentityAttestationVendor::OperatorSelf => "operator_self",
        IdentityAttestationVendor::IntelSgxDcap => "intel_sgx_dcap",
        IdentityAttestationVendor::AmdSevSnp => "amd_sev_snp",
        IdentityAttestationVendor::AwsNitro => "aws_nitro",
        IdentityAttestationVendor::AppleSecureEnclave => "apple_secure_enclave",
    }
}

pub(crate) fn identity_attestation_revocation_reason_name(
    reason: IdentityAttestationRevocationReason,
) -> &'static str {
    match reason {
        IdentityAttestationRevocationReason::AttestorRevoked => "attestor_revoked",
        IdentityAttestationRevocationReason::SubjectRequested => "subject_requested",
        IdentityAttestationRevocationReason::VendorRootRevoked => "vendor_root_revoked",
    }
}

pub(crate) fn identity_tier_transition_reason_name(
    reason: IdentityTierTransitionReason,
) -> &'static str {
    match reason {
        IdentityTierTransitionReason::AttestationRegistered => "attestation_registered",
        IdentityTierTransitionReason::AttestationRevoked => "attestation_revoked",
        IdentityTierTransitionReason::AttestationExpired => "attestation_expired",
        IdentityTierTransitionReason::OperatorChanged => "operator_changed",
        IdentityTierTransitionReason::VendorRootChanged => "vendor_root_changed",
    }
}

pub(crate) fn attestation_kind_name(kind: AttestationKind) -> &'static str {
    match kind {
        AttestationKind::OperatorSelf => "operator_self",
        AttestationKind::TeeEnclave => "tee_enclave",
        AttestationKind::ThirdParty => "third_party",
    }
}

pub(crate) fn parse_attestation_kind(value: &str) -> Option<&'static str> {
    match value {
        "operator_self" => Some("operator_self"),
        "tee_enclave" => Some("tee_enclave"),
        "third_party" => Some("third_party"),
        _ => None,
    }
}

pub(crate) fn attestation_claim_to_json(claim: &AttestationClaim) -> Value {
    match claim {
        AttestationClaim::RunsCodeHash {
            code_hash,
            model_hash,
        } => json!({
            "kind": "runs_code_hash",
            "code_hash_hex": encode_hex(code_hash),
            "model_hash_hex": model_hash.as_ref().map(|hash| encode_hex(hash)),
        }),
        AttestationClaim::BoundToMandate { mandate_id } => json!({
            "kind": "bound_to_mandate",
            "mandate_id_hex": encode_hex(mandate_id),
        }),
        AttestationClaim::RunsCodeAndBoundToMandate {
            code_hash,
            model_hash,
            mandate_id,
        } => json!({
            "kind": "runs_code_and_bound_to_mandate",
            "code_hash_hex": encode_hex(code_hash),
            "model_hash_hex": model_hash.as_ref().map(|hash| encode_hex(hash)),
            "mandate_id_hex": encode_hex(mandate_id),
        }),
    }
}

pub(crate) fn attestation_evidence_to_json(evidence: AttestationEvidence) -> Value {
    match evidence {
        AttestationEvidence::OperatorSignature => json!({
            "kind": "operator_signature",
        }),
        AttestationEvidence::TeeQuoteVerified {
            vendor,
            vendor_root_id,
            enclave_id,
            signer,
            verified_at_block,
        } => json!({
            "kind": "tee_quote_verified",
            "vendor": tee_vendor_name(tee_vendor_from_node(vendor)),
            "vendor_root_id_hex": encode_hex(&vendor_root_id),
            "enclave_id_hex": encode_hex(&enclave_id),
            "signer_hex": encode_hex(&signer),
            "verified_at_block": verified_at_block,
        }),
    }
}

pub(crate) fn attestation_claim_from_json(
    input: &AttestationClaimJson,
) -> Result<AttestationClaim, RpcError> {
    match input {
        AttestationClaimJson::RunsCodeHash {
            code_hash_hex,
            model_hash_hex,
        } => Ok(AttestationClaim::RunsCodeHash {
            code_hash: decode_hex_array::<32>(code_hash_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid code_hash_hex".into()))?,
            model_hash: model_hash_hex
                .as_deref()
                .map(decode_hex_array::<32>)
                .transpose()
                .map_err(|_| RpcError::InvalidRequest("invalid model_hash_hex".into()))?,
        }),
        AttestationClaimJson::BoundToMandate { mandate_id_hex } => {
            Ok(AttestationClaim::BoundToMandate {
                mandate_id: decode_hex_array::<32>(mandate_id_hex)
                    .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?,
            })
        }
        AttestationClaimJson::RunsCodeAndBoundToMandate {
            code_hash_hex,
            model_hash_hex,
            mandate_id_hex,
        } => Ok(AttestationClaim::RunsCodeAndBoundToMandate {
            code_hash: decode_hex_array::<32>(code_hash_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid code_hash_hex".into()))?,
            model_hash: model_hash_hex
                .as_deref()
                .map(decode_hex_array::<32>)
                .transpose()
                .map_err(|_| RpcError::InvalidRequest("invalid model_hash_hex".into()))?,
            mandate_id: decode_hex_array::<32>(mandate_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?,
        }),
    }
}

pub(crate) fn tee_vendor_name(vendor: TeeVendorResponse) -> &'static str {
    match vendor {
        TeeVendorResponse::IntelSgxDcap => "intel_sgx_dcap",
        TeeVendorResponse::AmdSevSnp => "amd_sev_snp",
        TeeVendorResponse::AwsNitro => "aws_nitro",
        TeeVendorResponse::AppleSecureEnclave => "apple_secure_enclave",
    }
}

pub(crate) fn parse_tee_vendor(value: &str) -> Option<TeeVendorResponse> {
    match value {
        "intel_sgx_dcap" => Some(TeeVendorResponse::IntelSgxDcap),
        "amd_sev_snp" => Some(TeeVendorResponse::AmdSevSnp),
        "aws_nitro" => Some(TeeVendorResponse::AwsNitro),
        "apple_secure_enclave" => Some(TeeVendorResponse::AppleSecureEnclave),
        _ => None,
    }
}

pub(crate) fn tee_vendor_to_node(vendor: TeeVendorResponse) -> TeeVendor {
    match vendor {
        TeeVendorResponse::IntelSgxDcap => TeeVendor::IntelSgxDcap,
        TeeVendorResponse::AmdSevSnp => TeeVendor::AmdSevSnp,
        TeeVendorResponse::AwsNitro => TeeVendor::AwsNitro,
        TeeVendorResponse::AppleSecureEnclave => TeeVendor::AppleSecureEnclave,
    }
}

pub(crate) fn tee_vendor_from_node(vendor: TeeVendor) -> TeeVendorResponse {
    match vendor {
        TeeVendor::IntelSgxDcap => TeeVendorResponse::IntelSgxDcap,
        TeeVendor::AmdSevSnp => TeeVendorResponse::AmdSevSnp,
        TeeVendor::AwsNitro => TeeVendorResponse::AwsNitro,
        TeeVendor::AppleSecureEnclave => TeeVendorResponse::AppleSecureEnclave,
    }
}
