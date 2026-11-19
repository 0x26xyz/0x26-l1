use crate::*;

pub(crate) fn account_operator_response_to_json(response: &AccountOperatorResponse) -> Value {
    json!({
        "account_id": response.account_id,
        "operator_id": response.operator_id,
    })
}

pub(crate) fn delegation_status_name(status: DelegationStatus) -> &'static str {
    match status {
        DelegationStatus::Open => "open",
        DelegationStatus::ClosedToDeposits => "closed_to_deposits",
        DelegationStatus::Winding => "winding",
        DelegationStatus::Closed => "closed",
        DelegationStatus::Terminated => "terminated",
    }
}

pub(crate) fn delegation_lifecycle_status_name(status: DelegationLifecycleStatus) -> &'static str {
    match status {
        DelegationLifecycleStatus::Open => "open",
        DelegationLifecycleStatus::ClosedToDeposits => "closed_to_deposits",
        DelegationLifecycleStatus::Winding => "winding",
        DelegationLifecycleStatus::Closed => "closed",
        DelegationLifecycleStatus::Terminated => "terminated",
    }
}

pub(crate) fn delegation_status_reason_name(reason: DelegationStatusReason) -> &'static str {
    match reason {
        DelegationStatusReason::ClosedToDeposits => "closed_to_deposits",
        DelegationStatusReason::WindingBegun => "winding_begun",
        DelegationStatusReason::WindingComplete => "winding_complete",
        DelegationStatusReason::MandateBreached => "mandate_breached",
        DelegationStatusReason::UnauthorizedTradingAttempt => "unauthorized_trading_attempt",
        DelegationStatusReason::RiskLimitBreach => "risk_limit_breach",
        DelegationStatusReason::AgentAttestationRevoked => "agent_attestation_revoked",
        DelegationStatusReason::WindingGraceFailure => "winding_grace_failure",
        DelegationStatusReason::GovernanceFault => "governance_fault",
    }
}

pub(crate) fn delegation_fault_reason_name(reason: BlockDelegationFaultReason) -> &'static str {
    match reason {
        BlockDelegationFaultReason::MandateBreached => "mandate_breached",
        BlockDelegationFaultReason::UnauthorizedTradingAttempt => "unauthorized_trading_attempt",
        BlockDelegationFaultReason::RiskLimitBreach => "risk_limit_breach",
        BlockDelegationFaultReason::AgentAttestationRevoked => "agent_attestation_revoked",
        BlockDelegationFaultReason::WindingGraceFailure => "winding_grace_failure",
        BlockDelegationFaultReason::GovernanceFault => "governance_fault",
    }
}

pub(crate) fn delegation_action_kind_for_receipt(
    receipt: &DelegationActionReceipt,
) -> Option<&'static str> {
    match receipt {
        DelegationActionReceipt::Open { .. } => Some(DELEGATION_ACTION_KIND_OPEN),
        DelegationActionReceipt::Deposit { .. } => Some(DELEGATION_ACTION_KIND_DEPOSIT),
        DelegationActionReceipt::RequestRedemption { .. } => {
            Some(DELEGATION_ACTION_KIND_REQUEST_REDEMPTION)
        }
        DelegationActionReceipt::FinalizeRedemption { .. }
        | DelegationActionReceipt::FinalizeRedemptionReady { .. } => {
            Some(DELEGATION_ACTION_KIND_FINALIZE_REDEMPTION)
        }
        DelegationActionReceipt::CancelRedemption { .. } => {
            Some(DELEGATION_ACTION_KIND_CANCEL_REDEMPTION)
        }
        DelegationActionReceipt::CloseToDeposits { .. } => {
            Some(DELEGATION_ACTION_KIND_CLOSE_TO_DEPOSITS)
        }
        DelegationActionReceipt::BeginWinding { .. } => Some(DELEGATION_ACTION_KIND_BEGIN_WINDING),
        DelegationActionReceipt::InsurancePayout { .. } => {
            Some(DELEGATION_ACTION_KIND_INSURANCE_PAYOUT)
        }
        DelegationActionReceipt::ForcedLiquidation { .. } => {
            Some(DELEGATION_ACTION_KIND_FORCED_LIQUIDATION)
        }
        DelegationActionReceipt::Close { .. } => Some(DELEGATION_ACTION_KIND_CLOSE),
        DelegationActionReceipt::Terminate { .. } => Some(DELEGATION_ACTION_KIND_TERMINATE),
        DelegationActionReceipt::Rejected { action_kind, .. } => {
            canonical_delegation_block_action_kind(action_kind)
        }
    }
}

pub(crate) fn delegation_action_id_for_rpc(
    action: &BlockDelegationAction,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> DelegationActionId {
    delegation_action_id_for_submission(action, timestamp_ms, submitter_nonce)
}

pub(crate) fn parse_delegation_status(value: &str) -> Option<DelegationStatus> {
    match value {
        "open" => Some(DelegationStatus::Open),
        "closed_to_deposits" => Some(DelegationStatus::ClosedToDeposits),
        "winding" => Some(DelegationStatus::Winding),
        "closed" => Some(DelegationStatus::Closed),
        "terminated" => Some(DelegationStatus::Terminated),
        _ => None,
    }
}

pub(crate) fn parse_redemption_request_status(value: &str) -> Option<RedemptionRequestStatus> {
    match value {
        "pending" => Some(RedemptionRequestStatus::Pending),
        "ready" => Some(RedemptionRequestStatus::Ready),
        "finalized" => Some(RedemptionRequestStatus::Finalized),
        "cancelled" => Some(RedemptionRequestStatus::Cancelled),
        _ => None,
    }
}

pub(crate) fn redemption_request_effective_status(
    request: &RedemptionRequest,
    now_ms: u64,
) -> RedemptionRequestStatus {
    match request.status {
        RedemptionRequestStatus::Pending if now_ms >= request.ready_at_ms => {
            RedemptionRequestStatus::Ready
        }
        status => status,
    }
}

pub(crate) fn redemption_request_status_name(status: RedemptionRequestStatus) -> &'static str {
    match status {
        RedemptionRequestStatus::Pending => "pending",
        RedemptionRequestStatus::Ready => "ready",
        RedemptionRequestStatus::Finalized => "finalized",
        RedemptionRequestStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn order_redemption_request_items(items: &mut [RedemptionRequestListItem]) {
    items.sort_by(|a, b| {
        b.request
            .requested_at_ms
            .cmp(&a.request.requested_at_ms)
            .then_with(|| a.request.request_id.cmp(&b.request.request_id))
    });
}

pub(crate) fn order_vault_position_items(items: &mut [VaultPositionListItem]) {
    items.sort_by(|a, b| a.vault_id.cmp(&b.vault_id));
}

pub(crate) fn delegation_nav_per_share_for_rpc(
    delegation: &Delegation,
    tvl: u128,
) -> Result<u128, RpcError> {
    if delegation.total_shares == 0 {
        return Ok(SHARE_SCALE_FACTOR);
    }
    // Cash NAV can legitimately be zero while capital is deployed into isolated
    // positions; the live-trading fields carry that current marked value.
    tvl.checked_mul(SHARE_SCALE_FACTOR)
        .and_then(|scaled| scaled.checked_div(delegation.total_shares))
        .ok_or_else(|| RpcError::Node("delegation_nav_invalid".into()))
}

pub(crate) fn encode_vault_position_cursor(vault_id: vaults::VaultId) -> String {
    encode_hex(&vault_id)
}

pub(crate) fn decode_vault_position_cursor(value: &str) -> Option<vaults::VaultId> {
    decode_hex_array::<16>(value).ok()
}

pub(crate) fn encode_redemption_request_cursor(
    requested_at_ms: u64,
    request_id: RedemptionRequestId,
) -> String {
    let mut bytes = [0u8; 40];
    bytes[..8].copy_from_slice(&requested_at_ms.to_be_bytes());
    bytes[8..].copy_from_slice(&request_id);
    encode_hex(&bytes)
}

pub(crate) fn decode_redemption_request_cursor(value: &str) -> Option<(u64, RedemptionRequestId)> {
    let raw = decode_hex_bytes(value).ok()?;
    if raw.len() != 40 {
        return None;
    }
    let mut requested_at_ms = [0u8; 8];
    requested_at_ms.copy_from_slice(&raw[..8]);
    let mut request_id = [0u8; 32];
    request_id.copy_from_slice(&raw[8..]);
    Some((u64::from_be_bytes(requested_at_ms), request_id))
}

pub(crate) fn redemption_request_cursor_strictly_after(
    requested_at_ms: u64,
    request_id: &RedemptionRequestId,
    cursor_requested_at_ms: u64,
    cursor_request_id: &RedemptionRequestId,
) -> bool {
    requested_at_ms < cursor_requested_at_ms
        || (requested_at_ms == cursor_requested_at_ms && request_id > cursor_request_id)
}

pub(crate) fn delegation_action_submission_status_name(
    status: DelegationActionSubmissionStatus,
) -> &'static str {
    match status {
        DelegationActionSubmissionStatus::Accepted => "accepted",
        DelegationActionSubmissionStatus::DuplicatePending => "duplicate_pending",
        DelegationActionSubmissionStatus::AlreadyProcessed => "already_processed",
    }
}

pub(crate) fn delegation_action_status_name(status: DelegationActionStatus) -> &'static str {
    match status {
        DelegationActionStatus::Pending => "pending",
        DelegationActionStatus::Processed => "processed",
        DelegationActionStatus::Unknown => "unknown",
    }
}

pub(crate) fn delegation_action_submission_response_to_json(
    response: &DelegationActionSubmissionResponse,
) -> Value {
    json!({
        "action_id_hex": encode_hex(&response.action_id),
        "status": delegation_action_submission_status_name(response.status),
        "pending_delegation_actions": response.pending_delegation_actions,
    })
}

pub(crate) fn delegation_action_receipt_to_json(receipt: &DelegationActionReceipt) -> Value {
    match receipt {
        DelegationActionReceipt::Open { delegation_id } => json!({
            "kind": "open",
            "delegation_id_hex": encode_hex(delegation_id),
        }),
        DelegationActionReceipt::Deposit {
            delegation_id,
            principal,
            shares_minted,
        } => json!({
            "kind": "deposit",
            "delegation_id_hex": encode_hex(delegation_id),
            "principal": principal,
            "shares_minted": shares_minted.to_string(),
        }),
        DelegationActionReceipt::RequestRedemption { request_id } => json!({
            "kind": "request_redemption",
            "request_id_hex": encode_hex(request_id),
        }),
        DelegationActionReceipt::FinalizeRedemption {
            request_id,
            usdc_out,
            compensation_out,
        } => json!({
            "kind": "finalize_redemption",
            "request_id_hex": encode_hex(request_id),
            "usdc_out": usdc_out.to_string(),
            "compensation_out": compensation_out.to_string(),
        }),
        DelegationActionReceipt::FinalizeRedemptionReady { request_id } => json!({
            "kind": "finalize_redemption_ready",
            "request_id_hex": encode_hex(request_id),
        }),
        DelegationActionReceipt::CancelRedemption { request_id } => json!({
            "kind": "cancel_redemption",
            "request_id_hex": encode_hex(request_id),
        }),
        DelegationActionReceipt::CloseToDeposits { delegation_id } => json!({
            "kind": "close_to_deposits",
            "delegation_id_hex": encode_hex(delegation_id),
        }),
        DelegationActionReceipt::BeginWinding { delegation_id } => json!({
            "kind": "begin_winding",
            "delegation_id_hex": encode_hex(delegation_id),
        }),
        DelegationActionReceipt::InsurancePayout {
            delegation_id,
            requested_amount_micro_usdc,
            distributed_amount_micro_usdc,
            recipient_count,
        } => json!({
            "kind": "insurance_payout",
            "delegation_id_hex": encode_hex(delegation_id),
            "requested_amount_micro_usdc": requested_amount_micro_usdc.to_string(),
            "distributed_amount_micro_usdc": distributed_amount_micro_usdc.to_string(),
            "recipient_count": recipient_count,
        }),
        DelegationActionReceipt::ForcedLiquidation {
            delegation_id,
            market_id,
            keeper_account,
            requested_lots,
            filled_lots,
            remaining_lots,
            keeper_reward_micro_usdc,
        } => json!({
            "kind": "forced_liquidation",
            "delegation_id_hex": encode_hex(delegation_id),
            "market_id": market_id,
            "keeper_account": keeper_account,
            "requested_lots": requested_lots,
            "filled_lots": filled_lots,
            "remaining_lots": remaining_lots,
            "keeper_reward_micro_usdc": keeper_reward_micro_usdc.to_string(),
        }),
        DelegationActionReceipt::Close { delegation_id } => json!({
            "kind": "close",
            "delegation_id_hex": encode_hex(delegation_id),
        }),
        DelegationActionReceipt::Terminate {
            delegation_id,
            reason,
            bond_returned,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta,
        } => json!({
            "kind": "terminate",
            "delegation_id_hex": encode_hex(delegation_id),
            "reason": delegation_fault_reason_name(*reason),
            "bond_returned": bond_returned.to_string(),
            "bond_slashed": bond_slashed.to_string(),
            "principal_compensation": principal_compensation.to_string(),
            "insurance_fund_delta": insurance_fund_delta.to_string(),
        }),
        DelegationActionReceipt::Rejected {
            action_kind,
            reason,
        } => json!({
            "kind": "rejected",
            "action_kind": action_kind,
            "reason": reason,
        }),
    }
}

pub(crate) fn delegation_action_status_response_to_json(
    response: &DelegationActionStatusResponse,
) -> Value {
    json!({
        "action_id_hex": encode_hex(&response.action_id),
        "status": delegation_action_status_name(response.status),
        "action_kind": response.action_kind,
        "submitted_at_ms": response.submitted_at_ms,
        "processed_at_ms": response.processed_at_ms,
        "receipt": response.receipt.as_ref().map(delegation_action_receipt_to_json),
    })
}

pub(crate) fn market_maker_program_action_submission_status_name(
    status: MarketMakerProgramActionSubmissionStatus,
) -> &'static str {
    match status {
        MarketMakerProgramActionSubmissionStatus::Accepted => "accepted",
        MarketMakerProgramActionSubmissionStatus::DuplicatePending => "duplicate_pending",
        MarketMakerProgramActionSubmissionStatus::AlreadyProcessed => "already_processed",
    }
}

pub(crate) fn market_maker_program_action_status_name(
    status: MarketMakerProgramActionStatus,
) -> &'static str {
    match status {
        MarketMakerProgramActionStatus::Pending => "pending",
        MarketMakerProgramActionStatus::Processed => "processed",
        MarketMakerProgramActionStatus::Unknown => "unknown",
    }
}

pub(crate) fn market_maker_program_action_submission_response_to_json(
    response: &MarketMakerProgramActionSubmissionResponse,
) -> Value {
    json!({
        "action_id_hex": encode_hex(&response.action_id),
        "status": market_maker_program_action_submission_status_name(response.status),
        "pending_market_maker_program_actions": response.pending_market_maker_program_actions,
    })
}

pub(crate) fn market_maker_program_action_kind_for_receipt(
    receipt: &MarketMakerProgramActionReceipt,
) -> Option<&'static str> {
    match receipt {
        MarketMakerProgramActionReceipt::Define { .. } => {
            Some(MARKET_MAKER_PROGRAM_ACTION_KIND_DEFINE)
        }
        MarketMakerProgramActionReceipt::Retire { .. } => {
            Some(MARKET_MAKER_PROGRAM_ACTION_KIND_RETIRE)
        }
        MarketMakerProgramActionReceipt::Enroll { .. } => {
            Some(MARKET_MAKER_PROGRAM_ACTION_KIND_ENROLL)
        }
        MarketMakerProgramActionReceipt::Exit { .. } => Some(MARKET_MAKER_PROGRAM_ACTION_KIND_EXIT),
        MarketMakerProgramActionReceipt::Rejected { action_kind, .. } => match action_kind.as_str()
        {
            MARKET_MAKER_PROGRAM_ACTION_KIND_DEFINE => {
                Some(MARKET_MAKER_PROGRAM_ACTION_KIND_DEFINE)
            }
            MARKET_MAKER_PROGRAM_ACTION_KIND_RETIRE => {
                Some(MARKET_MAKER_PROGRAM_ACTION_KIND_RETIRE)
            }
            MARKET_MAKER_PROGRAM_ACTION_KIND_ENROLL => {
                Some(MARKET_MAKER_PROGRAM_ACTION_KIND_ENROLL)
            }
            MARKET_MAKER_PROGRAM_ACTION_KIND_EXIT => Some(MARKET_MAKER_PROGRAM_ACTION_KIND_EXIT),
            _ => None,
        },
    }
}

pub(crate) fn market_maker_program_action_receipt_to_json(
    receipt: &MarketMakerProgramActionReceipt,
) -> Value {
    match receipt {
        MarketMakerProgramActionReceipt::Define { program_id } => json!({
            "kind": "define",
            "program_id_hex": encode_hex(program_id),
        }),
        MarketMakerProgramActionReceipt::Retire { program_id } => json!({
            "kind": "retire",
            "program_id_hex": encode_hex(program_id),
        }),
        MarketMakerProgramActionReceipt::Enroll {
            market_maker_identity,
            market_id,
            program_id,
        } => json!({
            "kind": "enroll",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
        }),
        MarketMakerProgramActionReceipt::Exit {
            market_maker_identity,
            market_id,
            program_id,
        } => json!({
            "kind": "exit",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
        }),
        MarketMakerProgramActionReceipt::Rejected {
            action_kind,
            reason,
        } => json!({
            "kind": "rejected",
            "action_kind": action_kind,
            "reason": reason,
        }),
    }
}

pub(crate) fn market_maker_program_action_status_response_to_json(
    response: &MarketMakerProgramActionStatusResponse,
) -> Value {
    json!({
        "action_id_hex": encode_hex(&response.action_id),
        "status": market_maker_program_action_status_name(response.status),
        "action_kind": response.action_kind,
        "submitted_at_ms": response.submitted_at_ms,
        "processed_at_ms": response.processed_at_ms,
        "receipt": response
            .receipt
            .as_ref()
            .map(market_maker_program_action_receipt_to_json),
    })
}

pub(crate) fn market_maker_u256_to_json(value: MarketMakerU256) -> Value {
    if value.0[..16].iter().all(|byte| *byte == 0) {
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&value.0[16..]);
        return Value::String(u128::from_be_bytes(bytes).to_string());
    }
    Value::String(encode_hex(&value.0))
}

pub(crate) fn market_maker_program_status_name(status: MarketMakerProgramStatus) -> &'static str {
    match status {
        MarketMakerProgramStatus::Active => "active",
        MarketMakerProgramStatus::Retired => "retired",
    }
}

pub(crate) fn market_maker_enrollment_status_name(
    status: MarketMakerEnrollmentStatus,
) -> &'static str {
    match status {
        MarketMakerEnrollmentStatus::Active => "active",
        MarketMakerEnrollmentStatus::GracePeriod => "grace_period",
        MarketMakerEnrollmentStatus::NonCompliant => "non_compliant",
        MarketMakerEnrollmentStatus::Exited => "exited",
        MarketMakerEnrollmentStatus::Slashed => "slashed",
    }
}

pub(crate) fn market_maker_breach_kind_name(kind: MarketMakerBreachKind) -> &'static str {
    match kind {
        MarketMakerBreachKind::Uptime => "uptime",
        MarketMakerBreachKind::Spread => "spread",
        MarketMakerBreachKind::Size => "size",
        MarketMakerBreachKind::TwoSidedWindows => "two_sided_windows",
    }
}

pub(crate) fn market_maker_event_enrollment_status_name(
    status: kombat_chain_streams::MarketMakerEnrollmentStatus,
) -> &'static str {
    match status {
        kombat_chain_streams::MarketMakerEnrollmentStatus::Active => "active",
        kombat_chain_streams::MarketMakerEnrollmentStatus::GracePeriod => "grace_period",
        kombat_chain_streams::MarketMakerEnrollmentStatus::NonCompliant => "non_compliant",
        kombat_chain_streams::MarketMakerEnrollmentStatus::Exited => "exited",
        kombat_chain_streams::MarketMakerEnrollmentStatus::Slashed => "slashed",
    }
}

pub(crate) fn market_maker_event_breach_kind_name(
    kind: kombat_chain_streams::MarketMakerBreachKind,
) -> &'static str {
    match kind {
        kombat_chain_streams::MarketMakerBreachKind::Uptime => "uptime",
        kombat_chain_streams::MarketMakerBreachKind::Spread => "spread",
        kombat_chain_streams::MarketMakerBreachKind::Size => "size",
        kombat_chain_streams::MarketMakerBreachKind::TwoSidedWindows => "two_sided_windows",
    }
}

pub(crate) fn market_maker_program_to_json(program: &MarketMakerProgram) -> Value {
    json!({
        "program_id_hex": encode_hex(&program.program_id),
        "name": program.name,
        "markets": program.markets.iter().copied().collect::<Vec<_>>(),
        "defined_at_ms": program.defined_at_ms,
        "last_updated_at_ms": program.last_updated_at_ms,
        "status": market_maker_program_status_name(program.status),
        "min_uptime_bps": program.min_uptime_bps,
        "max_spread_bps": program.max_spread_bps,
        "min_quote_size_micro_usdc": market_maker_u256_to_json(program.min_quote_size_micro_usdc),
        "min_quote_size_micro_usdc_hex": encode_hex(&program.min_quote_size_micro_usdc.0),
        "min_two_sided_windows_per_hour": program.min_two_sided_windows_per_hour,
        "rebate_multiplier_units": program.rebate_multiplier_units,
        "enrollment_bond_micro_usdc": market_maker_u256_to_json(program.enrollment_bond_micro_usdc),
        "enrollment_bond_micro_usdc_hex": encode_hex(&program.enrollment_bond_micro_usdc.0),
        "compliance_window_ms": program.compliance_window_ms,
        "observation_cadence_ms": program.observation_cadence_ms,
    })
}

pub(crate) fn market_maker_compliance_state_to_json(
    state: kombat_chain_node::MarketMakerComplianceState,
) -> Value {
    json!({
        "window_start_ms": state.window_start_ms,
        "window_end_ms": state.window_end_ms,
        "uptime_bps_current": state.uptime_bps_current,
        "avg_spread_bps_current": state.avg_spread_bps_current,
        "median_quote_size_current": market_maker_u256_to_json(state.median_quote_size_current),
        "median_quote_size_current_hex": encode_hex(&state.median_quote_size_current.0),
        "two_sided_windows_last_h": state.two_sided_windows_last_h,
        "last_window_breach": state.last_window_breach.map(market_maker_breach_kind_name),
        "breach_counter": state.breach_counter,
    })
}

pub(crate) fn market_maker_enrollment_to_json(enrollment: &MarketMakerEnrollment) -> Value {
    json!({
        "market_maker_identity": market_maker_identity_to_json(enrollment.market_maker_identity),
        "market_id": enrollment.market_id,
        "program_id_hex": encode_hex(&enrollment.program_id),
        "enrolled_at_ms": enrollment.enrolled_at_ms,
        "last_updated_at_ms": enrollment.last_updated_at_ms,
        "status": market_maker_enrollment_status_name(enrollment.status),
        "bond_micro_usdc": market_maker_u256_to_json(enrollment.bond_micro_usdc),
        "bond_micro_usdc_hex": encode_hex(&enrollment.bond_micro_usdc.0),
        "current_compliance": market_maker_compliance_state_to_json(enrollment.current_compliance),
        "last_compliance_checked_ms": enrollment.last_compliance_checked_ms,
        "lifetime_compliant_rebates_micro_usdc": market_maker_u256_to_json(
            enrollment.lifetime_compliant_rebates_micro_usdc,
        ),
        "lifetime_compliant_rebates_micro_usdc_hex": encode_hex(
            &enrollment.lifetime_compliant_rebates_micro_usdc.0,
        ),
        "lifetime_forfeited_rebates_micro_usdc": market_maker_u256_to_json(
            enrollment.lifetime_forfeited_rebates_micro_usdc,
        ),
        "lifetime_forfeited_rebates_micro_usdc_hex": encode_hex(
            &enrollment.lifetime_forfeited_rebates_micro_usdc.0,
        ),
    })
}

pub(crate) fn delegation_response_to_json(response: &DelegationResponse) -> Value {
    let delegation = &response.delegation;
    json!({
        "delegation_id_hex": encode_hex(&delegation.delegation_id),
        "agent": delegation.agent,
        "mandate_id_hex": encode_hex(&delegation.mandate_id),
        "opened_at_ms": delegation.opened_at_ms,
        "last_updated_at_ms": delegation.last_updated_at_ms,
        "winding_started_at_ms": delegation.winding_started_at_ms,
        "status": delegation_status_name(delegation.status),
        "performance_share_units": delegation.performance_share_units,
        "crystallization_cadence_ms": delegation.crystallization_cadence_ms,
        "notice_period_ms": delegation.notice_period_ms,
        "total_shares": delegation.total_shares.to_string(),
        "high_water_mark_per_share": delegation.high_water_mark_per_share.to_string(),
        "last_crystallized_at_ms": delegation.last_crystallized_at_ms,
        "agent_bond": delegation.agent_bond.to_string(),
    })
}

pub(crate) fn delegation_share_response_to_json(response: &DelegationShareResponse) -> Value {
    json!({
        "delegation_id_hex": encode_hex(&response.delegation_id),
        "principal": response.principal,
        "shares": response.share.shares.to_string(),
        "locked_for_redemption": response.share.locked_for_redemption.to_string(),
        "avg_entry_nav_per_share": response.share.avg_entry_nav_per_share.to_string(),
    })
}

pub(crate) fn delegation_deposit_response_to_json(response: &DelegationDepositResponse) -> Value {
    json!({
        "delegation": delegation_response_to_json(&DelegationResponse {
            delegation: response.deposit.delegation.clone(),
        }),
        "share": delegation_share_response_to_json(&DelegationShareResponse {
            delegation_id: response.deposit.delegation.delegation_id,
            principal: response.deposit.principal,
            share: response.deposit.share,
        }),
        "shares_minted": response.deposit.shares_minted.to_string(),
        "principal_balance": response.deposit.principal_balance.to_string(),
        "delegation_balance": response.deposit.delegation_balance.to_string(),
        "trading_account": response.deposit.trading_account,
    })
}

pub(crate) fn redemption_request_response_to_json(response: &RedemptionRequestResponse) -> Value {
    let request = &response.request;
    json!({
        "request_id_hex": encode_hex(&request.request_id),
        "delegation_id_hex": encode_hex(&request.delegation_id),
        "principal": request.principal,
        "shares_requested": request.shares_requested.to_string(),
        "requested_at_ms": request.requested_at_ms,
        "ready_at_ms": request.ready_at_ms,
        "nav_at_request": request.nav_at_request.to_string(),
        "status": redemption_request_status_name(request.status),
    })
}

pub(crate) fn redemption_request_list_item_to_json(item: &RedemptionRequestListItem) -> Value {
    let request = &item.request;
    json!({
        "request_id_hex": encode_hex(&request.request_id),
        "vault_id_hex": encode_hex(&item.vault_id),
        "kind": "delegation",
        "delegation_id_hex": encode_hex(&request.delegation_id),
        "principal": request.principal,
        "shares_requested": request.shares_requested.to_string(),
        "requested_at_ms": request.requested_at_ms,
        "ready_at_ms": request.ready_at_ms,
        "nav_at_request": request.nav_at_request.to_string(),
        "status": redemption_request_status_name(request.status),
    })
}

pub(crate) fn list_redemption_requests_response_to_json(
    response: &ListRedemptionRequestsResponse,
) -> Value {
    json!({
        "items": response
            .items
            .iter()
            .map(redemption_request_list_item_to_json)
            .collect::<Vec<_>>(),
        "next_cursor": response.next_cursor,
    })
}

pub(crate) fn vault_position_list_item_to_json(item: &VaultPositionListItem) -> Value {
    json!({
        "vault_id_hex": encode_hex(&item.vault_id),
        "kind": item.kind.name(),
        "delegation_id_hex": encode_hex(&item.delegation_id),
        "principal": item.principal,
        "shares": item.share.shares.to_string(),
        "locked_shares_for_redemption": item.share.locked_for_redemption.to_string(),
        "avg_entry_nav_per_share": item.share.avg_entry_nav_per_share.to_string(),
        "principal_deposited_micro_usdc": item.principal_deposited_micro_usdc.to_string(),
        "authoritative_nav_per_share": item.authoritative_nav_per_share.to_string(),
        "authoritative_nav_source": item.authoritative_nav_source,
        "indicative_nav_per_share": item.indicative_nav_per_share.to_string(),
        "indicative_nav_source": item.indicative_nav_source,
        "live_trading": item.live_trading.as_ref().map(|info| json!({
            "trading_account_id": info.trading_account_id.to_string(),
            "market_count": info.market_count,
            "source": "delegation_trading_account_risk_summary",
            "total_collateral_micro_usdc": info.total_collateral_micro_usdc.to_string(),
            "total_realized_pnl_micro_usdc": info.total_realized_pnl_micro_usdc.to_string(),
            "total_unrealized_pnl_micro_usdc": info.total_unrealized_pnl_micro_usdc.to_string(),
            "total_pnl_micro_usdc": info.total_pnl_micro_usdc.to_string(),
            "total_equity_micro_usdc": info.total_equity_micro_usdc.to_string(),
            "total_open_notional_micro_usdc": info.total_open_notional_micro_usdc.to_string(),
            "total_initial_margin_required_micro_usdc": info.total_initial_margin_required_micro_usdc.to_string(),
            "total_maintenance_requirement_micro_usdc": info.total_maintenance_requirement_micro_usdc.to_string(),
            "total_margin_surplus_micro_usdc": info.total_margin_surplus_micro_usdc.to_string(),
        })),
        "live_nav_per_share": item.live_nav_per_share.map(|value| value.to_string()),
        "live_estimated_value_micro_usdc": item.live_estimated_value_micro_usdc.map(|value| value.to_string()),
        "compensation_claim_micro_usdc": item.compensation_claim_micro_usdc.to_string(),
        "estimated_value_micro_usdc": item.estimated_value_micro_usdc.to_string(),
        "status": item.status.name(),
    })
}

pub(crate) fn list_vault_positions_response_to_json(
    response: &ListVaultPositionsResponse,
) -> Value {
    json!({
        "items": response
            .items
            .iter()
            .map(vault_position_list_item_to_json)
            .collect::<Vec<_>>(),
        "next_cursor": response.next_cursor,
    })
}

pub(crate) fn real_capital_listing_response_to_json(
    response: &RealCapitalListingResponse,
) -> Value {
    json!({
        "vault_id_hex": encode_hex(&response.vault_id),
        "delegation_id_hex": encode_hex(&response.delegation_id),
        "serving_agent": response.serving_agent,
        "mode": real_capital_mode_name(response.mode),
        "pilot_min_serving_agent_tier": response.pilot_min_serving_agent_tier.map(identity_tier_name),
        "tier0_pilot_exception": response.tier0_pilot_exception,
        "deposit_enabled": response.deposit_enabled,
        "listed_for_discovery": response.listed_for_discovery,
        "caps": {
            "per_principal_micro_usdc": response.per_principal_cap_micro_usdc.to_string(),
            "total_vault_tvl_micro_usdc": response.total_vault_tvl_cap_micro_usdc.to_string(),
        },
        "protocol_cap_micro_usdc": response.protocol_cap_micro_usdc.map(|value| value.to_string()),
        "vault_deposited_micro_usdc": response.vault_deposited_micro_usdc.to_string(),
        "pilot_principal_allowlist": response.pilot_principal_allowlist,
        "listed_at_block": response.listed_at_block,
        "last_updated_block": response.last_updated_block,
        "last_updated_governance_action_hex": encode_hex(&response.last_updated_governance_action),
    })
}

pub(crate) fn real_capital_control_response_to_json(
    response: &RealCapitalControlResponse,
) -> Value {
    json!({
        "global_deposit_kill_switch": response.global_deposit_kill_switch,
        "last_updated_block": response.last_updated_block,
        "last_updated_governance_action_hex": encode_hex(&response.last_updated_governance_action),
        "protocol_cap_micro_usdc": response.protocol_cap_micro_usdc.map(|value| value.to_string()),
        "total_protocol_delegation_tvl_micro_usdc": response.total_protocol_delegation_tvl_micro_usdc.to_string(),
    })
}

pub(crate) fn list_real_capital_listings_response_to_json(
    response: &ListRealCapitalListingsResponse,
) -> Value {
    json!({
        "items": response
            .items
            .iter()
            .map(real_capital_listing_response_to_json)
            .collect::<Vec<_>>(),
        "next_cursor": response.next_cursor,
        "control": real_capital_control_response_to_json(&response.control),
    })
}

pub(crate) fn get_real_capital_listing_response_to_json(
    response: &GetRealCapitalListingResponse,
) -> Value {
    json!({
        "listing": response
            .listing
            .as_ref()
            .map(real_capital_listing_response_to_json),
        "present": response.present,
        "control": real_capital_control_response_to_json(&response.control),
    })
}

pub(crate) fn real_capital_governance_submission_status_name(
    status: RealCapitalGovernanceActionSubmissionStatus,
) -> &'static str {
    match status {
        RealCapitalGovernanceActionSubmissionStatus::Accepted => "accepted",
        RealCapitalGovernanceActionSubmissionStatus::DuplicatePending => "duplicate_pending",
        RealCapitalGovernanceActionSubmissionStatus::AlreadyProcessed => "already_processed",
    }
}

pub(crate) fn submit_real_capital_governance_action_response_to_json(
    response: &SubmitRealCapitalGovernanceActionResponse,
) -> Value {
    json!({
        "action_id_hex": encode_hex(&response.action_id),
        "status": real_capital_governance_submission_status_name(response.status),
        "pending_real_capital_governance_actions": response.pending_real_capital_governance_actions,
    })
}

pub(crate) fn real_capital_governance_action_status_name(
    status: RealCapitalGovernanceActionStatus,
) -> &'static str {
    match status {
        RealCapitalGovernanceActionStatus::Pending => "pending",
        RealCapitalGovernanceActionStatus::Processed => "processed",
        RealCapitalGovernanceActionStatus::Rejected => "rejected",
        RealCapitalGovernanceActionStatus::Unknown => "unknown",
    }
}

pub(crate) fn real_capital_governance_action_kind_for_receipt(
    receipt: &RealCapitalGovernanceActionReceipt,
) -> Option<&'static str> {
    match receipt {
        RealCapitalGovernanceActionReceipt::UpsertListing { .. } => {
            Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPSERT_LISTING)
        }
        RealCapitalGovernanceActionReceipt::SetListingDepositEnabled { .. } => {
            Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DEPOSIT_ENABLED)
        }
        RealCapitalGovernanceActionReceipt::SetListingDiscovery { .. } => {
            Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DISCOVERY)
        }
        RealCapitalGovernanceActionReceipt::UpdateProtocolCap { .. } => {
            Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_PROTOCOL_CAP)
        }
        RealCapitalGovernanceActionReceipt::UpdateGlobalKillSwitch { .. } => {
            Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_GLOBAL_KILL_SWITCH)
        }
        RealCapitalGovernanceActionReceipt::Rejected { action_kind, .. } => {
            match action_kind.as_str() {
                REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPSERT_LISTING => {
                    Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPSERT_LISTING)
                }
                REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DEPOSIT_ENABLED => {
                    Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DEPOSIT_ENABLED)
                }
                REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DISCOVERY => {
                    Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DISCOVERY)
                }
                REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_PROTOCOL_CAP => {
                    Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_PROTOCOL_CAP)
                }
                REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_GLOBAL_KILL_SWITCH => {
                    Some(REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_GLOBAL_KILL_SWITCH)
                }
                _ => None,
            }
        }
    }
}

pub(crate) fn real_capital_governance_action_receipt_to_json(
    receipt: &RealCapitalGovernanceActionReceipt,
) -> Value {
    match receipt {
        RealCapitalGovernanceActionReceipt::UpsertListing {
            vault_id,
            delegation_id,
            serving_agent,
        } => json!({
            "kind": "upsert_listing",
            "vault_id_hex": encode_hex(vault_id),
            "delegation_id_hex": encode_hex(delegation_id),
            "serving_agent": serving_agent,
        }),
        RealCapitalGovernanceActionReceipt::SetListingDepositEnabled {
            vault_id,
            deposit_enabled,
        } => json!({
            "kind": "set_listing_deposit_enabled",
            "vault_id_hex": encode_hex(vault_id),
            "deposit_enabled": deposit_enabled,
        }),
        RealCapitalGovernanceActionReceipt::SetListingDiscovery {
            vault_id,
            listed_for_discovery,
        } => json!({
            "kind": "set_listing_discovery",
            "vault_id_hex": encode_hex(vault_id),
            "listed_for_discovery": listed_for_discovery,
        }),
        RealCapitalGovernanceActionReceipt::UpdateProtocolCap {
            total_protocol_tvl_micro_usdc,
        } => json!({
            "kind": "update_protocol_cap",
            "total_protocol_tvl_micro_usdc": total_protocol_tvl_micro_usdc.map(|value| value.to_string()),
        }),
        RealCapitalGovernanceActionReceipt::UpdateGlobalKillSwitch { disabled } => json!({
            "kind": "update_global_kill_switch",
            "disabled": disabled,
        }),
        RealCapitalGovernanceActionReceipt::Rejected {
            action_kind,
            reason,
        } => json!({
            "kind": "rejected",
            "action_kind": action_kind,
            "reason": reason,
        }),
    }
}

pub(crate) fn real_capital_governance_action_status_response_to_json(
    response: &RealCapitalGovernanceActionStatusResponse,
) -> Value {
    json!({
        "action_id_hex": encode_hex(&response.action_id),
        "status": real_capital_governance_action_status_name(response.status),
        "action_kind": response.action_kind,
        "submitted_at_ms": response.submitted_at_ms,
        "processed_at_ms": response.processed_at_ms,
        "receipt": response
            .receipt
            .as_ref()
            .map(real_capital_governance_action_receipt_to_json),
    })
}

pub(crate) fn delegation_redemption_request_response_to_json(
    response: &DelegationRedemptionRequestResponse,
) -> Value {
    json!({
        "redemption": redemption_request_response_to_json(&RedemptionRequestResponse {
            request: response.redemption.request,
        }),
        "share": delegation_share_response_to_json(&DelegationShareResponse {
            delegation_id: response.redemption.request.delegation_id,
            principal: response.redemption.request.principal,
            share: response.redemption.share,
        }),
    })
}

pub(crate) fn delegation_redemption_finalization_response_to_json(
    response: &DelegationRedemptionFinalizationResponse,
) -> Value {
    json!({
        "redemption": redemption_request_response_to_json(&RedemptionRequestResponse {
            request: response.finalization.request,
        }),
        "delegation": delegation_response_to_json(&DelegationResponse {
            delegation: response.finalization.delegation.clone(),
        }),
        "share": delegation_share_response_to_json(&DelegationShareResponse {
            delegation_id: response.finalization.request.delegation_id,
            principal: response.finalization.request.principal,
            share: response.finalization.share,
        }),
        "usdc_out": response.finalization.usdc_out.to_string(),
        "compensation_out": response.finalization.compensation_out.to_string(),
        "principal_balance": response.finalization.principal_balance.to_string(),
        "delegation_balance": response.finalization.delegation_balance.to_string(),
    })
}

pub(crate) fn delegation_redemption_cancellation_response_to_json(
    response: &DelegationRedemptionCancellationResponse,
) -> Value {
    json!({
        "redemption": redemption_request_response_to_json(&RedemptionRequestResponse {
            request: response.cancellation.request,
        }),
        "share": delegation_share_response_to_json(&DelegationShareResponse {
            delegation_id: response.cancellation.request.delegation_id,
            principal: response.cancellation.request.principal,
            share: response.cancellation.share,
        }),
    })
}
