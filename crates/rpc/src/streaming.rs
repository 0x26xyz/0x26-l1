use crate::codec::decode_hex_array;
use crate::{
    order_book_response_to_json, position_response_to_json, runtime_event_to_json,
    trade_fill_from_event, trade_fill_response_to_json, NodeRpcService, TransportError,
};
use kombat_chain_consensus::IntentId;
use kombat_chain_streams::{EventFilter, OrderFillRole, RuntimeEventRecord, RuntimeEventTopic};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct AxumEventStreamQuery {
    pub(crate) from_seq: Option<u64>,
    pub(crate) limit: Option<usize>,
    pub(crate) market_id: Option<u64>,
    pub(crate) account_id: Option<u64>,
    pub(crate) policy_id: Option<u64>,
    pub(crate) intent_id_hex: Option<String>,
    pub(crate) client_order_id: Option<u64>,
    pub(crate) topic: Option<String>,
    pub(crate) follow: Option<bool>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct AxumMarketStreamQuery {
    pub(crate) market_id: u64,
    pub(crate) from_seq: Option<u64>,
    pub(crate) depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) follow: Option<bool>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct AxumAccountStreamQuery {
    pub(crate) account_id: u64,
    pub(crate) market_id: Option<u64>,
    pub(crate) from_seq: Option<u64>,
    pub(crate) depth: Option<usize>,
    pub(crate) limit: Option<usize>,
    pub(crate) follow: Option<bool>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct AxumWalletStreamQuery {
    pub(crate) address_hex: String,
    pub(crate) from_seq: Option<u64>,
    pub(crate) limit: Option<usize>,
    pub(crate) follow: Option<bool>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EventStreamRequest {
    pub(crate) from_seq: u64,
    pub(crate) from_seq_explicit: bool,
    pub(crate) limit: usize,
    pub(crate) filter: EventFilter,
    pub(crate) follow: bool,
    pub(crate) timeout_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MarketStreamRequest {
    pub(crate) market_id: u64,
    pub(crate) from_seq: u64,
    pub(crate) from_seq_explicit: bool,
    pub(crate) depth: usize,
    pub(crate) limit: usize,
    pub(crate) follow: bool,
    pub(crate) timeout_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AccountStreamRequest {
    pub(crate) account_id: u64,
    pub(crate) market_id: Option<u64>,
    pub(crate) from_seq: u64,
    pub(crate) from_seq_explicit: bool,
    pub(crate) depth: usize,
    pub(crate) limit: usize,
    pub(crate) follow: bool,
    pub(crate) timeout_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalletStreamRequest {
    pub(crate) address: [u8; 20],
    pub(crate) from_seq: u64,
    pub(crate) from_seq_explicit: bool,
    pub(crate) limit: usize,
    pub(crate) follow: bool,
    pub(crate) timeout_ms: u64,
}

pub(crate) fn event_topic_name(topic: RuntimeEventTopic) -> &'static str {
    match topic {
        RuntimeEventTopic::AccountCreated => "account_created",
        RuntimeEventTopic::AccountUpdated => "account_updated",
        RuntimeEventTopic::AccountOperatorUpdated => "account_operator_updated",
        RuntimeEventTopic::IntentAccepted => "intent_accepted",
        RuntimeEventTopic::IntentPromoted => "intent_promoted",
        RuntimeEventTopic::IntentExecuted => "intent_executed",
        RuntimeEventTopic::IntentExpired => "intent_expired",
        RuntimeEventTopic::IntentRejected => "intent_rejected",
        RuntimeEventTopic::OrderAccepted => "order_accepted",
        RuntimeEventTopic::OrderFilled => "order_filled",
        RuntimeEventTopic::FeeCharged => "fee_charged",
        RuntimeEventTopic::FeeRevenueDistributed => "fee_revenue_distributed",
        RuntimeEventTopic::MarketListed => "market_listed",
        RuntimeEventTopic::MarketStatusUpdated => "market_status_updated",
        RuntimeEventTopic::MarketMakerProgramDefined => "market_maker_program_defined",
        RuntimeEventTopic::MarketMakerProgramRetired => "market_maker_program_retired",
        RuntimeEventTopic::MarketMakerProgramActionRejected => {
            "market_maker_program_action_rejected"
        }
        RuntimeEventTopic::MarketMakerEnrolled => "market_maker_enrolled",
        RuntimeEventTopic::MarketMakerEnrollmentExited => "market_maker_enrollment_exited",
        RuntimeEventTopic::MmProgramBonusPaid => "mm_program_bonus_paid",
        RuntimeEventTopic::MmProgramBonusForfeited => "mm_program_bonus_forfeited",
        RuntimeEventTopic::MarketMakerEnrollmentGraceEntered => {
            "market_maker_enrollment_grace_entered"
        }
        RuntimeEventTopic::MarketMakerEnrollmentRecovered => "market_maker_enrollment_recovered",
        RuntimeEventTopic::MarketMakerEnrollmentNonCompliant => {
            "market_maker_enrollment_non_compliant"
        }
        RuntimeEventTopic::DelegationOpened => "delegation_opened",
        RuntimeEventTopic::DelegationDeposited => "delegation_deposited",
        RuntimeEventTopic::DelegationRedemptionRequested => "delegation_redemption_requested",
        RuntimeEventTopic::DelegationRedemptionFinalized => "delegation_redemption_finalized",
        RuntimeEventTopic::DelegationRedemptionCancelled => "delegation_redemption_cancelled",
        RuntimeEventTopic::DelegationCrystallized => "delegation_crystallized",
        RuntimeEventTopic::DelegationStatusChanged => "delegation_status_changed",
        RuntimeEventTopic::DelegationActionRejected => "delegation_action_rejected",
        RuntimeEventTopic::MandateCreated => "mandate_created",
        RuntimeEventTopic::MandateActivated => "mandate_activated",
        RuntimeEventTopic::MandateObjectiveEvaluated => "mandate_objective_evaluated",
        RuntimeEventTopic::MandateConformanceEvaluated => "mandate_conformance_evaluated",
        RuntimeEventTopic::MandateConstraintTriggered => "mandate_constraint_triggered",
        RuntimeEventTopic::MandateBreached => "mandate_breached",
        RuntimeEventTopic::MandateSuspended => "mandate_suspended",
        RuntimeEventTopic::MandateResumed => "mandate_resumed",
        RuntimeEventTopic::MandateRevoked => "mandate_revoked",
        RuntimeEventTopic::MandateExpired => "mandate_expired",
        RuntimeEventTopic::MarketTradingStateUpdated => "market_trading_state_updated",
        RuntimeEventTopic::OracleConfigUpdated => "oracle_config_updated",
        RuntimeEventTopic::PositionUpdated => "position_updated",
        RuntimeEventTopic::MarkPriceUpdated => "mark_price_updated",
        RuntimeEventTopic::IndexPriceUpdated => "index_price_updated",
        RuntimeEventTopic::MarketOpenInterestUpdated => "market_open_interest_updated",
        RuntimeEventTopic::FundingIndexUpdated => "funding_index_updated",
        RuntimeEventTopic::FundingApplied => "funding_applied",
        RuntimeEventTopic::CollateralDeposited => "collateral_deposited",
        RuntimeEventTopic::CollateralWithdrawn => "collateral_withdrawn",
        RuntimeEventTopic::RiskStateUpdated => "risk_state_updated",
        RuntimeEventTopic::LiquidationCandidateUpdated => "liquidation_candidate_updated",
        RuntimeEventTopic::InsuranceFundUpdated => "insurance_fund_updated",
        RuntimeEventTopic::BadDebtUpdated => "bad_debt_updated",
        RuntimeEventTopic::BadDebtRecovered => "bad_debt_recovered",
        RuntimeEventTopic::SolvencyBreached => "solvency_breached",
        RuntimeEventTopic::LiquidationExecuted => "liquidation_executed",
        RuntimeEventTopic::LiquidationOutcome => "liquidation_outcome",
        RuntimeEventTopic::LiquidationEscalationUpdated => "liquidation_escalation_updated",
        RuntimeEventTopic::OrderReplaced => "order_replaced",
        RuntimeEventTopic::OrderCancelled => "order_cancelled",
        RuntimeEventTopic::PolicyCreated => "policy_created",
        RuntimeEventTopic::PolicyRevoked => "policy_revoked",
        RuntimeEventTopic::IdentityRegistered => "identity_registered",
        RuntimeEventTopic::IdentityUpdated => "identity_updated",
        RuntimeEventTopic::IdentityDeleted => "identity_deleted",
        RuntimeEventTopic::AttestationRegistered => "attestation_registered",
        RuntimeEventTopic::AttestationVerified => "attestation_verified",
        RuntimeEventTopic::AttestationRefreshed => "attestation_refreshed",
        RuntimeEventTopic::AttestationRevoked => "attestation_revoked",
        RuntimeEventTopic::AttestationExpired => "attestation_expired",
        RuntimeEventTopic::TierChanged => "tier_changed",
        RuntimeEventTopic::VendorRootIntroduced => "vendor_root_introduced",
        RuntimeEventTopic::VendorRootRevoked => "vendor_root_revoked",
        RuntimeEventTopic::VendorCapProfileUpdated => "vendor_cap_profile_updated",
        RuntimeEventTopic::TeeGovernanceActionRejected => "tee_governance_action_rejected",
        RuntimeEventTopic::RealCapitalListingUpserted => "real_capital_listing_upserted",
        RuntimeEventTopic::RealCapitalListingDepositEnabledUpdated => {
            "real_capital_listing_deposit_enabled_updated"
        }
        RuntimeEventTopic::RealCapitalListingDiscoveryUpdated => {
            "real_capital_listing_discovery_updated"
        }
        RuntimeEventTopic::RealCapitalProtocolCapUpdated => "real_capital_protocol_cap_updated",
        RuntimeEventTopic::RealCapitalGlobalKillSwitchUpdated => {
            "real_capital_global_kill_switch_updated"
        }
        RuntimeEventTopic::RealCapitalGovernanceActionRejected => {
            "real_capital_governance_action_rejected"
        }
        RuntimeEventTopic::RealCapitalInsuranceWalletSpent => "real_capital_insurance_wallet_spent",
        RuntimeEventTopic::BridgeDepositLocked => "bridge_deposit_locked",
    }
}

pub(crate) fn parse_event_stream_request(
    query: Option<&str>,
) -> Result<EventStreamRequest, &'static str> {
    let mut from_seq = 0_u64;
    let mut from_seq_explicit = false;
    let mut limit = 100_usize;
    let mut market_id = None;
    let mut account_id = None;
    let mut policy_id = None;
    let mut intent_id = None;
    let mut client_order_id = None;
    let mut topic = None;
    let mut follow = false;
    let mut timeout_ms = 30_000_u64;

    if let Some(query) = query {
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "from_seq" => {
                    from_seq = value.parse().map_err(|_| "invalid from_seq")?;
                    from_seq_explicit = true;
                }
                "limit" => {
                    let parsed = value.parse::<usize>().map_err(|_| "invalid limit")?;
                    limit = parsed.clamp(1, 1_000);
                }
                "market_id" => {
                    market_id = Some(value.parse().map_err(|_| "invalid market_id")?);
                }
                "account_id" => {
                    account_id = Some(value.parse().map_err(|_| "invalid account_id")?);
                }
                "policy_id" => {
                    policy_id = Some(value.parse().map_err(|_| "invalid policy_id")?);
                }
                "intent_id_hex" => {
                    intent_id = Some(parse_intent_id_query(value)?);
                }
                "client_order_id" => {
                    client_order_id = Some(value.parse().map_err(|_| "invalid client_order_id")?);
                }
                "topic" => {
                    topic = Some(parse_runtime_event_topic(value)?);
                }
                "follow" => {
                    follow = matches!(value, "1" | "true" | "yes");
                }
                "timeout_ms" => {
                    timeout_ms = value.parse().map_err(|_| "invalid timeout_ms")?;
                }
                _ => {}
            }
        }
    }

    Ok(EventStreamRequest {
        from_seq,
        from_seq_explicit,
        limit,
        filter: EventFilter {
            block_height: None,
            market_id,
            account_id,
            policy_id,
            intent_id,
            client_order_id,
            topic,
        },
        follow,
        timeout_ms,
    })
}

pub(crate) fn parse_market_stream_request(
    query: Option<&str>,
) -> Result<MarketStreamRequest, &'static str> {
    let mut market_id = None;
    let mut from_seq = 0_u64;
    let mut from_seq_explicit = false;
    let mut depth = 20_usize;
    let mut limit = 100_usize;
    let mut follow = true;
    let mut timeout_ms = 30_000_u64;

    if let Some(query) = query {
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "market_id" => {
                    market_id = Some(value.parse().map_err(|_| "invalid market_id")?);
                }
                "from_seq" => {
                    from_seq = value.parse().map_err(|_| "invalid from_seq")?;
                    from_seq_explicit = true;
                }
                "depth" => {
                    depth = value
                        .parse::<usize>()
                        .map_err(|_| "invalid depth")?
                        .clamp(1, 200);
                }
                "limit" => {
                    limit = value
                        .parse::<usize>()
                        .map_err(|_| "invalid limit")?
                        .clamp(1, 1_000);
                }
                "follow" => {
                    follow = matches!(value, "1" | "true" | "yes");
                }
                "timeout_ms" => {
                    timeout_ms = value.parse().map_err(|_| "invalid timeout_ms")?;
                }
                _ => {}
            }
        }
    }

    let market_id = market_id.ok_or("market_id is required")?;
    if market_id == 0 {
        return Err("market_id is required");
    }
    Ok(MarketStreamRequest {
        market_id,
        from_seq,
        from_seq_explicit,
        depth,
        limit,
        follow,
        timeout_ms,
    })
}

pub(crate) fn parse_account_stream_request(
    query: Option<&str>,
) -> Result<AccountStreamRequest, &'static str> {
    let mut account_id = None;
    let mut market_id = None;
    let mut from_seq = 0_u64;
    let mut from_seq_explicit = false;
    let mut depth = 100_usize;
    let mut limit = 100_usize;
    let mut follow = true;
    let mut timeout_ms = 30_000_u64;

    if let Some(query) = query {
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "account_id" => {
                    account_id = Some(value.parse().map_err(|_| "invalid account_id")?);
                }
                "market_id" => {
                    market_id = Some(value.parse().map_err(|_| "invalid market_id")?);
                }
                "from_seq" => {
                    from_seq = value.parse().map_err(|_| "invalid from_seq")?;
                    from_seq_explicit = true;
                }
                "depth" => {
                    depth = value
                        .parse::<usize>()
                        .map_err(|_| "invalid depth")?
                        .clamp(1, 1_000);
                }
                "limit" => {
                    limit = value
                        .parse::<usize>()
                        .map_err(|_| "invalid limit")?
                        .clamp(1, 1_000);
                }
                "follow" => {
                    follow = matches!(value, "1" | "true" | "yes");
                }
                "timeout_ms" => {
                    timeout_ms = value.parse().map_err(|_| "invalid timeout_ms")?;
                }
                _ => {}
            }
        }
    }

    let account_id = account_id.ok_or("account_id is required")?;
    if account_id == 0 {
        return Err("account_id is required");
    }
    Ok(AccountStreamRequest {
        account_id,
        market_id,
        from_seq,
        from_seq_explicit,
        depth,
        limit,
        follow,
        timeout_ms,
    })
}

pub(crate) fn parse_wallet_stream_request(
    query: Option<&str>,
) -> Result<WalletStreamRequest, &'static str> {
    let mut address = None;
    let mut from_seq = 0_u64;
    let mut from_seq_explicit = false;
    let mut limit = 100_usize;
    let mut follow = true;
    let mut timeout_ms = 30_000_u64;

    if let Some(query) = query {
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            match key {
                "address_hex" => {
                    address =
                        Some(decode_hex_array::<20>(value).map_err(|_| "invalid address_hex")?);
                }
                "from_seq" => {
                    from_seq = value.parse().map_err(|_| "invalid from_seq")?;
                    from_seq_explicit = true;
                }
                "limit" => {
                    limit = value
                        .parse::<usize>()
                        .map_err(|_| "invalid limit")?
                        .clamp(1, 1_000);
                }
                "follow" => {
                    follow = matches!(value, "1" | "true" | "yes");
                }
                "timeout_ms" => {
                    timeout_ms = value.parse().map_err(|_| "invalid timeout_ms")?;
                }
                _ => {}
            }
        }
    }

    Ok(WalletStreamRequest {
        address: address.ok_or("address_hex is required")?,
        from_seq,
        from_seq_explicit,
        limit,
        follow,
        timeout_ms,
    })
}

pub(crate) fn parse_intent_id_query(value: &str) -> Result<IntentId, &'static str> {
    decode_hex_array(value).map_err(|_| "invalid intent_id_hex")
}

pub(crate) fn parse_runtime_event_topic(value: &str) -> Result<RuntimeEventTopic, &'static str> {
    match value {
        "account_created" => Ok(RuntimeEventTopic::AccountCreated),
        "account_updated" => Ok(RuntimeEventTopic::AccountUpdated),
        "account_operator_updated" => Ok(RuntimeEventTopic::AccountOperatorUpdated),
        "identity_registered" => Ok(RuntimeEventTopic::IdentityRegistered),
        "identity_updated" => Ok(RuntimeEventTopic::IdentityUpdated),
        "identity_deleted" => Ok(RuntimeEventTopic::IdentityDeleted),
        "attestation_registered" => Ok(RuntimeEventTopic::AttestationRegistered),
        "attestation_verified" => Ok(RuntimeEventTopic::AttestationVerified),
        "attestation_refreshed" => Ok(RuntimeEventTopic::AttestationRefreshed),
        "attestation_revoked" => Ok(RuntimeEventTopic::AttestationRevoked),
        "attestation_expired" => Ok(RuntimeEventTopic::AttestationExpired),
        "tier_changed" => Ok(RuntimeEventTopic::TierChanged),
        "vendor_root_introduced" => Ok(RuntimeEventTopic::VendorRootIntroduced),
        "vendor_root_revoked" => Ok(RuntimeEventTopic::VendorRootRevoked),
        "vendor_cap_profile_updated" => Ok(RuntimeEventTopic::VendorCapProfileUpdated),
        "tee_governance_action_rejected" => Ok(RuntimeEventTopic::TeeGovernanceActionRejected),
        "real_capital_listing_upserted" => Ok(RuntimeEventTopic::RealCapitalListingUpserted),
        "real_capital_listing_deposit_enabled_updated" => {
            Ok(RuntimeEventTopic::RealCapitalListingDepositEnabledUpdated)
        }
        "real_capital_listing_discovery_updated" => {
            Ok(RuntimeEventTopic::RealCapitalListingDiscoveryUpdated)
        }
        "real_capital_protocol_cap_updated" => Ok(RuntimeEventTopic::RealCapitalProtocolCapUpdated),
        "real_capital_global_kill_switch_updated" => {
            Ok(RuntimeEventTopic::RealCapitalGlobalKillSwitchUpdated)
        }
        "real_capital_governance_action_rejected" => {
            Ok(RuntimeEventTopic::RealCapitalGovernanceActionRejected)
        }
        "bridge_deposit_locked" => Ok(RuntimeEventTopic::BridgeDepositLocked),
        "intent_accepted" => Ok(RuntimeEventTopic::IntentAccepted),
        "intent_promoted" => Ok(RuntimeEventTopic::IntentPromoted),
        "intent_executed" => Ok(RuntimeEventTopic::IntentExecuted),
        "intent_expired" => Ok(RuntimeEventTopic::IntentExpired),
        "intent_rejected" => Ok(RuntimeEventTopic::IntentRejected),
        "order_accepted" => Ok(RuntimeEventTopic::OrderAccepted),
        "order_filled" => Ok(RuntimeEventTopic::OrderFilled),
        "fee_charged" => Ok(RuntimeEventTopic::FeeCharged),
        "fee_revenue_distributed" => Ok(RuntimeEventTopic::FeeRevenueDistributed),
        "market_listed" => Ok(RuntimeEventTopic::MarketListed),
        "market_status_updated" => Ok(RuntimeEventTopic::MarketStatusUpdated),
        "market_maker_program_defined" => Ok(RuntimeEventTopic::MarketMakerProgramDefined),
        "market_maker_program_retired" => Ok(RuntimeEventTopic::MarketMakerProgramRetired),
        "market_maker_program_action_rejected" => {
            Ok(RuntimeEventTopic::MarketMakerProgramActionRejected)
        }
        "market_maker_enrolled" => Ok(RuntimeEventTopic::MarketMakerEnrolled),
        "market_maker_enrollment_exited" => Ok(RuntimeEventTopic::MarketMakerEnrollmentExited),
        "mm_program_bonus_paid" => Ok(RuntimeEventTopic::MmProgramBonusPaid),
        "mm_program_bonus_forfeited" => Ok(RuntimeEventTopic::MmProgramBonusForfeited),
        "market_maker_enrollment_grace_entered" => {
            Ok(RuntimeEventTopic::MarketMakerEnrollmentGraceEntered)
        }
        "market_maker_enrollment_recovered" => {
            Ok(RuntimeEventTopic::MarketMakerEnrollmentRecovered)
        }
        "market_maker_enrollment_non_compliant" => {
            Ok(RuntimeEventTopic::MarketMakerEnrollmentNonCompliant)
        }
        "delegation_opened" => Ok(RuntimeEventTopic::DelegationOpened),
        "delegation_deposited" => Ok(RuntimeEventTopic::DelegationDeposited),
        "delegation_redemption_requested" => Ok(RuntimeEventTopic::DelegationRedemptionRequested),
        "delegation_redemption_finalized" => Ok(RuntimeEventTopic::DelegationRedemptionFinalized),
        "delegation_redemption_cancelled" => Ok(RuntimeEventTopic::DelegationRedemptionCancelled),
        "delegation_crystallized" => Ok(RuntimeEventTopic::DelegationCrystallized),
        "delegation_status_changed" => Ok(RuntimeEventTopic::DelegationStatusChanged),
        "delegation_action_rejected" => Ok(RuntimeEventTopic::DelegationActionRejected),
        "mandate_created" => Ok(RuntimeEventTopic::MandateCreated),
        "mandate_activated" => Ok(RuntimeEventTopic::MandateActivated),
        "mandate_objective_evaluated" => Ok(RuntimeEventTopic::MandateObjectiveEvaluated),
        "mandate_conformance_evaluated" => Ok(RuntimeEventTopic::MandateConformanceEvaluated),
        "mandate_constraint_triggered" => Ok(RuntimeEventTopic::MandateConstraintTriggered),
        "mandate_breached" => Ok(RuntimeEventTopic::MandateBreached),
        "mandate_suspended" => Ok(RuntimeEventTopic::MandateSuspended),
        "mandate_resumed" => Ok(RuntimeEventTopic::MandateResumed),
        "mandate_revoked" => Ok(RuntimeEventTopic::MandateRevoked),
        "mandate_expired" => Ok(RuntimeEventTopic::MandateExpired),
        "market_trading_state_updated" => Ok(RuntimeEventTopic::MarketTradingStateUpdated),
        "oracle_config_updated" => Ok(RuntimeEventTopic::OracleConfigUpdated),
        "position_updated" => Ok(RuntimeEventTopic::PositionUpdated),
        "mark_price_updated" => Ok(RuntimeEventTopic::MarkPriceUpdated),
        "index_price_updated" => Ok(RuntimeEventTopic::IndexPriceUpdated),
        "market_open_interest_updated" => Ok(RuntimeEventTopic::MarketOpenInterestUpdated),
        "funding_index_updated" => Ok(RuntimeEventTopic::FundingIndexUpdated),
        "funding_applied" => Ok(RuntimeEventTopic::FundingApplied),
        "collateral_deposited" => Ok(RuntimeEventTopic::CollateralDeposited),
        "collateral_withdrawn" => Ok(RuntimeEventTopic::CollateralWithdrawn),
        "risk_state_updated" => Ok(RuntimeEventTopic::RiskStateUpdated),
        "liquidation_candidate_updated" => Ok(RuntimeEventTopic::LiquidationCandidateUpdated),
        "insurance_fund_updated" => Ok(RuntimeEventTopic::InsuranceFundUpdated),
        "bad_debt_updated" => Ok(RuntimeEventTopic::BadDebtUpdated),
        "bad_debt_recovered" => Ok(RuntimeEventTopic::BadDebtRecovered),
        "solvency_breached" => Ok(RuntimeEventTopic::SolvencyBreached),
        "liquidation_executed" => Ok(RuntimeEventTopic::LiquidationExecuted),
        "liquidation_outcome" => Ok(RuntimeEventTopic::LiquidationOutcome),
        "liquidation_escalation_updated" => Ok(RuntimeEventTopic::LiquidationEscalationUpdated),
        "order_replaced" => Ok(RuntimeEventTopic::OrderReplaced),
        "order_cancelled" => Ok(RuntimeEventTopic::OrderCancelled),
        "policy_created" => Ok(RuntimeEventTopic::PolicyCreated),
        "policy_revoked" => Ok(RuntimeEventTopic::PolicyRevoked),
        _ => Err("invalid topic"),
    }
}

pub(crate) fn resume_seq(from_seq: Option<u64>, headers: &BTreeMap<String, String>) -> u64 {
    from_seq.unwrap_or_else(|| {
        headers
            .get("last-event-id")
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seq| seq.saturating_add(1))
            .unwrap_or(0)
    })
}

pub(crate) fn market_stream_payload_for_event(
    service: &NodeRpcService<'_>,
    event: &RuntimeEventRecord,
    depth: usize,
) -> Vec<(&'static str, u64, Value)> {
    let seq = event.seq;
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::MarkPriceUpdated {
            market_id,
            mark_price_ticks,
        } => vec![(
            "mark_price",
            seq,
            json!({
                "seq": seq,
                "market_id": market_id,
                "mark_price_ticks": mark_price_ticks,
                "timestamp_ms": event.timestamp_ms,
            }),
        )],
        kombat_chain_streams::RuntimeEventKind::IndexPriceUpdated {
            market_id,
            index_price_ticks,
        } => vec![(
            "index_price",
            seq,
            json!({
                "seq": seq,
                "market_id": market_id,
                "index_price_ticks": index_price_ticks,
                "timestamp_ms": event.timestamp_ms,
            }),
        )],
        kombat_chain_streams::RuntimeEventKind::MarketOpenInterestUpdated {
            market_id,
            open_interest_lots,
        } => vec![(
            "open_interest",
            seq,
            json!({
                "seq": seq,
                "market_id": market_id,
                "open_interest_lots": open_interest_lots.to_string(),
                "timestamp_ms": event.timestamp_ms,
            }),
        )],
        kombat_chain_streams::RuntimeEventKind::FundingIndexUpdated {
            market_id,
            funding_index,
        } => vec![(
            "funding",
            seq,
            json!({
                "seq": seq,
                "market_id": market_id,
                "funding_index": funding_index.to_string(),
                "timestamp_ms": event.timestamp_ms,
            }),
        )],
        kombat_chain_streams::RuntimeEventKind::OrderFilled { role, .. }
            if *role == OrderFillRole::Taker =>
        {
            trade_fill_from_event(event.clone())
                .map(|fill| {
                    vec![
                        (
                            "trade",
                            seq,
                            json!({
                                "seq": seq,
                                "trade": trade_fill_response_to_json(&fill),
                            }),
                        ),
                        (
                            "book",
                            seq,
                            json!({
                                "seq": seq,
                                "market_id": fill.market_id,
                                "order_book": order_book_response_to_json(
                                    &service.get_order_book(fill.market_id, depth),
                                ),
                            }),
                        ),
                    ]
                })
                .unwrap_or_default()
        }
        kombat_chain_streams::RuntimeEventKind::OrderAccepted { market_id, .. }
        | kombat_chain_streams::RuntimeEventKind::OrderCancelled { market_id, .. }
        | kombat_chain_streams::RuntimeEventKind::OrderReplaced { market_id, .. } => vec![(
            "book",
            seq,
            json!({
                "seq": seq,
                "market_id": market_id,
                "order_book": order_book_response_to_json(&service.get_order_book(*market_id, depth)),
            }),
        )],
        _ => Vec::new(),
    }
}

pub(crate) fn account_stream_payload_for_event(
    service: &NodeRpcService<'_>,
    event: &RuntimeEventRecord,
) -> Option<(&'static str, u64, Value)> {
    let seq = event.seq;
    match &event.kind {
        kombat_chain_streams::RuntimeEventKind::PositionUpdated {
            market_id,
            account_id,
            ..
        } => Some((
            "position",
            seq,
            json!({
                "seq": seq,
                "position": position_response_to_json(&service.get_position(*market_id, *account_id)),
                "runtime_event": runtime_event_to_json(event),
            }),
        )),
        kombat_chain_streams::RuntimeEventKind::OrderAccepted { .. }
        | kombat_chain_streams::RuntimeEventKind::OrderCancelled { .. }
        | kombat_chain_streams::RuntimeEventKind::OrderReplaced { .. } => Some((
            "order",
            seq,
            json!({
                "seq": seq,
                "runtime_event": runtime_event_to_json(event),
            }),
        )),
        kombat_chain_streams::RuntimeEventKind::OrderFilled { .. } => Some((
            "fill",
            seq,
            json!({
                "seq": seq,
                "fill": trade_fill_from_event(event.clone()).map(|fill| trade_fill_response_to_json(&fill)),
                "runtime_event": runtime_event_to_json(event),
            }),
        )),
        kombat_chain_streams::RuntimeEventKind::CollateralDeposited { .. }
        | kombat_chain_streams::RuntimeEventKind::CollateralWithdrawn { .. } => Some((
            "collateral",
            seq,
            json!({
                "seq": seq,
                "runtime_event": runtime_event_to_json(event),
            }),
        )),
        kombat_chain_streams::RuntimeEventKind::RiskStateUpdated { .. }
        | kombat_chain_streams::RuntimeEventKind::LiquidationCandidateUpdated { .. }
        | kombat_chain_streams::RuntimeEventKind::SolvencyBreached { .. } => Some((
            "risk",
            seq,
            json!({
                "seq": seq,
                "runtime_event": runtime_event_to_json(event),
            }),
        )),
        _ => None,
    }
}

pub(crate) fn write_sse_event(
    stream: &mut TcpStream,
    event: &RuntimeEventRecord,
) -> Result<(), TransportError> {
    let payload = runtime_event_to_json(event);
    let frame = format!(
        "id: {}\nevent: {}\ndata: {}\n\n",
        event.seq,
        event_topic_name(event.kind.topic()),
        payload
    );
    stream
        .write_all(frame.as_bytes())
        .map_err(TransportError::Write)?;
    stream.flush().map_err(TransportError::Write)
}

pub(crate) fn write_sse_json_event(
    stream: &mut TcpStream,
    event_name: &str,
    id: Option<u64>,
    payload: &Value,
) -> Result<(), TransportError> {
    let mut frame = String::new();
    if let Some(id) = id {
        frame.push_str(&format!("id: {id}\n"));
    }
    frame.push_str(&format!("event: {event_name}\ndata: {payload}\n\n"));
    stream
        .write_all(frame.as_bytes())
        .map_err(TransportError::Write)?;
    stream.flush().map_err(TransportError::Write)
}

pub(crate) fn write_sse_cursor(
    stream: &mut TcpStream,
    next_seq: u64,
) -> Result<(), TransportError> {
    let frame = format!("event: cursor\ndata: {{\"next_seq\":{next_seq}}}\n\n");
    stream
        .write_all(frame.as_bytes())
        .map_err(TransportError::Write)?;
    stream.flush().map_err(TransportError::Write)
}
