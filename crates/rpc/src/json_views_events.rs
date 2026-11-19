use crate::*;

pub(crate) fn event_page_to_json(page: &EventPage) -> Value {
    json!({
        "events": page.events.iter().map(runtime_event_to_json).collect::<Vec<_>>(),
        "next_cursor": {
            "next_seq": page.next_cursor.next_seq,
        }
    })
}

pub(crate) fn runtime_event_to_json(event: &RuntimeEventRecord) -> Value {
    json!({
        "seq": event.seq,
        "block_height": event.block_height,
        "timestamp_ms": event.timestamp_ms,
        "kind": runtime_event_kind_to_json(&event.kind),
    })
}

pub(crate) fn tip5_hash_to_json(hash: &kombat_chain_streams::Tip5Hash) -> Value {
    json!({
        "limb0": hash.limb0.to_string(),
        "limb1": hash.limb1.to_string(),
        "limb2": hash.limb2.to_string(),
        "limb3": hash.limb3.to_string(),
        "limb4": hash.limb4.to_string(),
    })
}

pub(crate) fn runtime_event_kind_to_json(kind: &kombat_chain_streams::RuntimeEventKind) -> Value {
    match kind {
        kombat_chain_streams::RuntimeEventKind::AccountCreated { account_id, owner } => json!({
            "topic": "account_created",
            "account_id": account_id,
            "owner_hex": encode_hex(owner),
        }),
        kombat_chain_streams::RuntimeEventKind::AccountUpdated {
            account_id,
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
            "account_id": account_id,
            "action": account_update_action_name(*action),
            "reason": reason.map(risk_transition_source_name),
            "owner_hex": encode_hex(owner),
            "pending_owner_hex": pending_owner.map(|principal| encode_hex(&principal)),
            "paused": paused,
            "nonce": nonce,
            "code_hash_hex": encode_hex(code_hash),
            "controllers": controllers,
        }),
        kombat_chain_streams::RuntimeEventKind::AccountOperatorUpdated {
            account_id,
            previous_operator_id,
            operator_id,
        } => json!({
            "topic": "account_operator_updated",
            "account_id": account_id,
            "previous_operator_id": previous_operator_id,
            "operator_id": operator_id,
        }),
        kombat_chain_streams::RuntimeEventKind::BridgeDepositLocked {
            deposit_nonce,
            tx_order_in_block,
            recipient,
            amount_nicks,
            nock_tx_id,
            note_name_first,
            note_name_last,
            nock_as_of,
        } => json!({
            "topic": "bridge_deposit_locked",
            "deposit_nonce": deposit_nonce.to_string(),
            "tx_order_in_block": tx_order_in_block.to_string(),
            "recipient_hex": encode_hex(recipient),
            "amount_nicks": amount_nicks.to_string(),
            "nock_tx_id": tip5_hash_to_json(nock_tx_id),
            "note_name_first": tip5_hash_to_json(note_name_first),
            "note_name_last": tip5_hash_to_json(note_name_last),
            "nock_as_of": tip5_hash_to_json(nock_as_of),
        }),
        kombat_chain_streams::RuntimeEventKind::IdentityRegistered {
            account,
            tier,
            display_name,
            kind,
            operator,
        } => json!({
            "topic": "identity_registered",
            "account": account,
            "tier": identity_tier_status_name(*tier),
            "display_name": display_name,
            "kind": identity_agent_kind_name(*kind),
            "operator": operator,
        }),
        kombat_chain_streams::RuntimeEventKind::IdentityUpdated {
            account,
            field,
            before,
            after,
        } => json!({
            "topic": "identity_updated",
            "account": account,
            "field": field,
            "before": before,
            "after": after,
        }),
        kombat_chain_streams::RuntimeEventKind::IdentityDeleted { account } => json!({
            "topic": "identity_deleted",
            "account": account,
        }),
        kombat_chain_streams::RuntimeEventKind::AttestationRegistered {
            account,
            record_id,
            kind,
            attestor,
            claim,
            expires_at_timestamp_ms,
            tier_before,
            tier_after,
        } => json!({
            "topic": "attestation_registered",
            "account": account,
            "record_id_hex": encode_hex(record_id),
            "kind": identity_attestation_kind_name(*kind),
            "attestor": attestor,
            "claim": identity_attestation_claim_to_json(claim),
            "expires_at_timestamp_ms": expires_at_timestamp_ms,
            "tier_before": identity_tier_status_name(*tier_before),
            "tier_after": identity_tier_status_name(*tier_after),
        }),
        kombat_chain_streams::RuntimeEventKind::AttestationVerified {
            account,
            record_id,
            vendor,
            code_hash,
            enclave_id,
        } => json!({
            "topic": "attestation_verified",
            "account": account,
            "record_id_hex": encode_hex(record_id),
            "vendor": identity_attestation_vendor_name(*vendor),
            "code_hash_hex": encode_hex(code_hash),
            "enclave_id_hex": encode_hex(enclave_id),
        }),
        kombat_chain_streams::RuntimeEventKind::AttestationRefreshed {
            account,
            record_id,
            new_expires_at_timestamp_ms,
        } => json!({
            "topic": "attestation_refreshed",
            "account": account,
            "record_id_hex": encode_hex(record_id),
            "new_expires_at_timestamp_ms": new_expires_at_timestamp_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::AttestationRevoked {
            account,
            record_id,
            reason,
            tier_before,
            tier_after,
        } => json!({
            "topic": "attestation_revoked",
            "account": account,
            "record_id_hex": encode_hex(record_id),
            "reason": identity_attestation_revocation_reason_name(*reason),
            "tier_before": identity_tier_status_name(*tier_before),
            "tier_after": identity_tier_status_name(*tier_after),
        }),
        kombat_chain_streams::RuntimeEventKind::AttestationExpired {
            account,
            record_id,
            tier_before,
            tier_after,
        } => json!({
            "topic": "attestation_expired",
            "account": account,
            "record_id_hex": encode_hex(record_id),
            "tier_before": identity_tier_status_name(*tier_before),
            "tier_after": identity_tier_status_name(*tier_after),
        }),
        kombat_chain_streams::RuntimeEventKind::TierChanged {
            account,
            from,
            to,
            reason,
        } => json!({
            "topic": "tier_changed",
            "account": account,
            "from": identity_tier_status_name(*from),
            "to": identity_tier_status_name(*to),
            "reason": identity_tier_transition_reason_name(*reason),
        }),
        kombat_chain_streams::RuntimeEventKind::VendorRootIntroduced {
            vendor,
            root_id,
            valid_from_block,
            governance_action,
        } => json!({
            "topic": "vendor_root_introduced",
            "vendor": identity_attestation_vendor_name(*vendor),
            "root_id_hex": encode_hex(root_id),
            "valid_from_block": valid_from_block,
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::VendorRootRevoked {
            vendor,
            root_id,
            valid_until_block,
            governance_action,
        } => json!({
            "topic": "vendor_root_revoked",
            "vendor": identity_attestation_vendor_name(*vendor),
            "root_id_hex": encode_hex(root_id),
            "valid_until_block": valid_until_block,
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::VendorCapProfileUpdated {
            vendor,
            old_cap_multiplier_microunits,
            new_cap_multiplier_microunits,
            old_composition_eligibility,
            new_composition_eligibility,
            governance_action,
        } => json!({
            "topic": "vendor_cap_profile_updated",
            "vendor": identity_attestation_vendor_name(*vendor),
            "old_cap_multiplier": tee_cap_multiplier_decimal_string(*old_cap_multiplier_microunits),
            "new_cap_multiplier": tee_cap_multiplier_decimal_string(*new_cap_multiplier_microunits),
            "old_composition_eligibility": old_composition_eligibility,
            "new_composition_eligibility": new_composition_eligibility,
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::TeeGovernanceActionRejected {
            governance_action,
            action_kind,
            reason,
        } => json!({
            "topic": "tee_governance_action_rejected",
            "governance_action_hex": encode_hex(governance_action),
            "action_kind": action_kind,
            "reason": reason,
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalListingUpserted {
            vault_id,
            delegation_id,
            serving_agent,
            old_serving_agent,
            mode,
            deposit_enabled,
            listed_for_discovery,
            per_principal_cap_micro_usdc,
            total_vault_tvl_cap_micro_usdc,
            governance_action,
        } => json!({
            "topic": "real_capital_listing_upserted",
            "vault_id_hex": encode_hex(vault_id),
            "delegation_id_hex": encode_hex(delegation_id),
            "serving_agent": serving_agent,
            "old_serving_agent": old_serving_agent,
            "mode": real_capital_listing_mode_name(*mode),
            "deposit_enabled": deposit_enabled,
            "listed_for_discovery": listed_for_discovery,
            "per_principal_cap_micro_usdc": per_principal_cap_micro_usdc.to_string(),
            "total_vault_tvl_cap_micro_usdc": total_vault_tvl_cap_micro_usdc.to_string(),
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalListingDepositEnabledUpdated {
            vault_id,
            delegation_id,
            serving_agent,
            deposit_enabled,
            governance_action,
        } => json!({
            "topic": "real_capital_listing_deposit_enabled_updated",
            "vault_id_hex": encode_hex(vault_id),
            "delegation_id_hex": encode_hex(delegation_id),
            "serving_agent": serving_agent,
            "deposit_enabled": deposit_enabled,
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalListingDiscoveryUpdated {
            vault_id,
            delegation_id,
            serving_agent,
            listed_for_discovery,
            governance_action,
        } => json!({
            "topic": "real_capital_listing_discovery_updated",
            "vault_id_hex": encode_hex(vault_id),
            "delegation_id_hex": encode_hex(delegation_id),
            "serving_agent": serving_agent,
            "listed_for_discovery": listed_for_discovery,
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalProtocolCapUpdated {
            old_total_protocol_tvl_micro_usdc,
            new_total_protocol_tvl_micro_usdc,
            governance_action,
        } => json!({
            "topic": "real_capital_protocol_cap_updated",
            "old_total_protocol_tvl_micro_usdc": old_total_protocol_tvl_micro_usdc.map(|value| value.to_string()),
            "new_total_protocol_tvl_micro_usdc": new_total_protocol_tvl_micro_usdc.map(|value| value.to_string()),
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated {
            old_disabled,
            new_disabled,
            governance_action,
        } => json!({
            "topic": "real_capital_global_kill_switch_updated",
            "old_disabled": old_disabled,
            "new_disabled": new_disabled,
            "governance_action_hex": encode_hex(governance_action),
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalGovernanceActionRejected {
            governance_action,
            action_kind,
            reason,
        } => json!({
            "topic": "real_capital_governance_action_rejected",
            "governance_action_hex": encode_hex(governance_action),
            "action_kind": action_kind,
            "reason": reason,
        }),
        kombat_chain_streams::RuntimeEventKind::RealCapitalInsuranceWalletSpent {
            amount,
            balance,
            source,
        } => json!({
            "topic": "real_capital_insurance_wallet_spent",
            "amount": amount.to_string(),
            "balance": balance.to_string(),
            "source": real_capital_insurance_wallet_spend_source_name(*source),
        }),
        kombat_chain_streams::RuntimeEventKind::IntentAccepted {
            intent_id,
            policy_id,
            nonce,
            class,
        } => json!({
            "topic": "intent_accepted",
            "intent_id_hex": encode_hex(intent_id),
            "policy_id": policy_id,
            "nonce": nonce,
            "class": match class {
                IntentAdmissionClass::Direct => "direct",
                IntentAdmissionClass::Conditional => "conditional",
            },
        }),
        kombat_chain_streams::RuntimeEventKind::IntentPromoted {
            intent_id,
            policy_id,
            nonce,
        } => json!({
            "topic": "intent_promoted",
            "intent_id_hex": encode_hex(intent_id),
            "policy_id": policy_id,
            "nonce": nonce,
        }),
        kombat_chain_streams::RuntimeEventKind::IntentExecuted {
            intent_id,
            policy_id,
            nonce,
        } => json!({
            "topic": "intent_executed",
            "intent_id_hex": encode_hex(intent_id),
            "policy_id": policy_id,
            "nonce": nonce,
        }),
        kombat_chain_streams::RuntimeEventKind::IntentExpired {
            intent_id,
            policy_id,
            nonce,
        } => json!({
            "topic": "intent_expired",
            "intent_id_hex": encode_hex(intent_id),
            "policy_id": policy_id,
            "nonce": nonce,
        }),
        kombat_chain_streams::RuntimeEventKind::IntentRejected {
            intent_id,
            policy_id,
            nonce,
            reason,
            order_reason,
        } => json!({
            "topic": "intent_rejected",
            "intent_id_hex": encode_hex(intent_id),
            "policy_id": policy_id,
            "nonce": nonce,
            "reason": intent_rejection_reason_name(*reason),
            "order_reason": order_reason.map(order_rejection_reason_name),
        }),
        kombat_chain_streams::RuntimeEventKind::OrderAccepted {
            market_id,
            account_id,
            client_order_id,
            status,
            filled_lots,
            remaining_lots,
        } => json!({
            "topic": "order_accepted",
            "market_id": market_id,
            "account_id": account_id,
            "client_order_id": client_order_id,
            "status": order_lifecycle_status_name(*status),
            "filled_lots": filled_lots,
            "remaining_lots": remaining_lots,
        }),
        kombat_chain_streams::RuntimeEventKind::OrderFilled {
            market_id,
            account_id,
            client_order_id,
            counterparty_account_id,
            counterparty_client_order_id,
            mandate_id,
            role,
            side,
            price_ticks,
            quantity_lots,
        } => json!({
            "topic": "order_filled",
            "market_id": market_id,
            "account_id": account_id,
            "client_order_id": client_order_id,
            "counterparty_account_id": counterparty_account_id,
            "counterparty_client_order_id": counterparty_client_order_id,
            "mandate_id_hex": mandate_id.as_ref().map(|id| encode_hex(id)),
            "role": order_fill_role_name(*role),
            "side": order_fill_side_name(*side),
            "price_ticks": price_ticks,
            "quantity_lots": quantity_lots,
        }),
        kombat_chain_streams::RuntimeEventKind::FeeCharged {
            market_id,
            account_id,
            client_order_id,
            role,
            fee_amount,
            fee_vault_balance,
        } => json!({
            "topic": "fee_charged",
            "market_id": market_id,
            "account_id": account_id,
            "client_order_id": client_order_id,
            "role": order_fill_role_name(*role),
            "fee_amount": fee_amount.to_string(),
            "fee_vault_balance": fee_vault_balance.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::FeeRevenueDistributed {
            market_id,
            source_fee_amount,
            validator_rewards_delta,
            validator_reward_balance,
            market_maker_rebate_delta,
            market_maker_rebate_balance,
            insurance_fund_delta,
            insurance_fund_balance,
            treasury_delta,
            treasury_balance,
        } => json!({
            "topic": "fee_revenue_distributed",
            "market_id": market_id,
            "source_fee_amount": source_fee_amount.to_string(),
            "validator_rewards_delta": validator_rewards_delta.to_string(),
            "validator_reward_balance": validator_reward_balance.to_string(),
            "market_maker_rebate_delta": market_maker_rebate_delta.to_string(),
            "market_maker_rebate_balance": market_maker_rebate_balance.to_string(),
            "insurance_fund_delta": insurance_fund_delta.to_string(),
            "insurance_fund_balance": insurance_fund_balance.to_string(),
            "treasury_delta": treasury_delta.to_string(),
            "treasury_balance": treasury_balance.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::MarketListed {
            market_id,
            base_symbol,
            quote_asset,
            oracle_feed_id,
            status,
        } => json!({
            "topic": "market_listed",
            "market_id": market_id,
            "base_symbol": base_symbol,
            "quote_asset": quote_asset,
            "oracle_feed_id": oracle_feed_id,
            "status": market_lifecycle_status_name(*status),
        }),
        kombat_chain_streams::RuntimeEventKind::MarketStatusUpdated { market_id, status } => {
            json!({
                "topic": "market_status_updated",
                "market_id": market_id,
                "status": market_lifecycle_status_name(*status),
            })
        }
        kombat_chain_streams::RuntimeEventKind::MarketMakerProgramDefined {
            program_id,
            name,
            markets,
            rebate_multiplier_units,
            compliance_window_ms,
            reactivated,
        } => json!({
            "topic": "market_maker_program_defined",
            "program_id_hex": encode_hex(program_id),
            "name": name,
            "markets": markets,
            "rebate_multiplier_units": rebate_multiplier_units,
            "compliance_window_ms": compliance_window_ms,
            "reactivated": reactivated,
        }),
        kombat_chain_streams::RuntimeEventKind::MarketMakerProgramRetired { program_id } => {
            json!({
                "topic": "market_maker_program_retired",
                "program_id_hex": encode_hex(program_id),
            })
        }
        kombat_chain_streams::RuntimeEventKind::MarketMakerProgramActionRejected {
            action_kind,
            reason,
        } => json!({
            "topic": "market_maker_program_action_rejected",
            "action_kind": action_kind,
            "reason": reason,
        }),
        kombat_chain_streams::RuntimeEventKind::MarketMakerEnrolled {
            market_maker_identity,
            market_id,
            program_id,
            bond_micro_usdc,
        } => json!({
            "topic": "market_maker_enrolled",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "bond_micro_usdc": bond_micro_usdc,
        }),
        kombat_chain_streams::RuntimeEventKind::MarketMakerEnrollmentExited {
            market_maker_identity,
            market_id,
            program_id,
            bond_refunded_micro_usdc,
        } => json!({
            "topic": "market_maker_enrollment_exited",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "bond_refunded_micro_usdc": bond_refunded_micro_usdc,
        }),
        kombat_chain_streams::RuntimeEventKind::MmProgramBonusPaid {
            market_maker_identity,
            economic_account_id,
            market_id,
            program_id,
            requested_bonus_micro_usdc,
            paid_bonus_micro_usdc,
        } => json!({
            "topic": "mm_program_bonus_paid",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "economic_account_id": economic_account_id,
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "requested_bonus_micro_usdc": requested_bonus_micro_usdc.to_string(),
            "paid_bonus_micro_usdc": paid_bonus_micro_usdc.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::MmProgramBonusForfeited {
            market_maker_identity,
            economic_account_id,
            market_id,
            program_id,
            forfeited_bonus_micro_usdc,
        } => json!({
            "topic": "mm_program_bonus_forfeited",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "economic_account_id": economic_account_id,
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "forfeited_bonus_micro_usdc": forfeited_bonus_micro_usdc.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::MarketMakerEnrollmentGraceEntered {
            market_maker_identity,
            economic_account_id,
            market_id,
            program_id,
            breach,
        } => json!({
            "topic": "market_maker_enrollment_grace_entered",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "economic_account_id": economic_account_id,
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "breach": market_maker_event_breach_kind_name(*breach),
        }),
        kombat_chain_streams::RuntimeEventKind::MarketMakerEnrollmentRecovered {
            market_maker_identity,
            economic_account_id,
            market_id,
            program_id,
            recovered_from,
        } => json!({
            "topic": "market_maker_enrollment_recovered",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "economic_account_id": economic_account_id,
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "recovered_from": market_maker_event_enrollment_status_name(*recovered_from),
        }),
        kombat_chain_streams::RuntimeEventKind::MarketMakerEnrollmentNonCompliant {
            market_maker_identity,
            economic_account_id,
            market_id,
            program_id,
            breach_counter,
        } => json!({
            "topic": "market_maker_enrollment_non_compliant",
            "market_maker_identity": market_maker_identity_to_json(*market_maker_identity),
            "economic_account_id": economic_account_id,
            "market_id": market_id,
            "program_id_hex": encode_hex(program_id),
            "breach_counter": breach_counter,
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationOpened {
            delegation_id,
            agent,
            mandate_id,
            performance_share_units,
            crystallization_cadence_ms,
            notice_period_ms,
            agent_bond,
        } => json!({
            "topic": "delegation_opened",
            "delegation_id_hex": encode_hex(delegation_id),
            "agent": agent,
            "mandate_id_hex": encode_hex(mandate_id),
            "performance_share_units": performance_share_units,
            "crystallization_cadence_ms": crystallization_cadence_ms,
            "notice_period_ms": notice_period_ms,
            "agent_bond": agent_bond.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationDeposited {
            delegation_id,
            principal,
            amount,
            shares_minted,
            total_shares,
        } => json!({
            "topic": "delegation_deposited",
            "delegation_id_hex": encode_hex(delegation_id),
            "principal": principal,
            "amount": amount.to_string(),
            "shares_minted": shares_minted.to_string(),
            "total_shares": total_shares.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationRedemptionRequested {
            delegation_id,
            principal,
            request_id,
            shares_requested,
            ready_at_ms,
        } => json!({
            "topic": "delegation_redemption_requested",
            "delegation_id_hex": encode_hex(delegation_id),
            "principal": principal,
            "request_id_hex": encode_hex(request_id),
            "shares_requested": shares_requested.to_string(),
            "ready_at_ms": ready_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationRedemptionFinalized {
            delegation_id,
            principal,
            request_id,
            shares_burned,
            usdc_out,
            compensation_out,
            total_shares,
        } => json!({
            "topic": "delegation_redemption_finalized",
            "delegation_id_hex": encode_hex(delegation_id),
            "principal": principal,
            "request_id_hex": encode_hex(request_id),
            "shares_burned": shares_burned.to_string(),
            "usdc_out": usdc_out.to_string(),
            "compensation_out": compensation_out.to_string(),
            "total_shares": total_shares.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationRedemptionCancelled {
            delegation_id,
            principal,
            request_id,
            shares_unlocked,
        } => json!({
            "topic": "delegation_redemption_cancelled",
            "delegation_id_hex": encode_hex(delegation_id),
            "principal": principal,
            "request_id_hex": encode_hex(request_id),
            "shares_unlocked": shares_unlocked.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationCrystallized {
            delegation_id,
            agent,
            agent_fee,
            nav_pre_fee,
            nav_post_fee,
            hwm_new,
        } => json!({
            "topic": "delegation_crystallized",
            "delegation_id_hex": encode_hex(delegation_id),
            "agent": agent,
            "agent_fee": agent_fee.to_string(),
            "nav_pre_fee": nav_pre_fee.to_string(),
            "nav_post_fee": nav_post_fee.to_string(),
            "hwm_new": hwm_new.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationStatusChanged {
            delegation_id,
            agent,
            old_status,
            new_status,
            reason,
            bond_returned,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta,
        } => json!({
            "topic": "delegation_status_changed",
            "delegation_id_hex": encode_hex(delegation_id),
            "agent": agent,
            "old_status": delegation_lifecycle_status_name(*old_status),
            "new_status": delegation_lifecycle_status_name(*new_status),
            "reason": delegation_status_reason_name(*reason),
            "bond_returned": bond_returned.to_string(),
            "bond_slashed": bond_slashed.to_string(),
            "principal_compensation": principal_compensation.to_string(),
            "insurance_fund_delta": insurance_fund_delta.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::DelegationActionRejected {
            action_kind,
            reason,
        } => json!({
            "topic": "delegation_action_rejected",
            "action_kind": action_kind,
            "reason": reason,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateCreated {
            mandate_id,
            owner,
            agent,
            version,
            expires_at,
        } => json!({
            "topic": "mandate_created",
            "mandate_id_hex": encode_hex(mandate_id),
            "owner_hex": encode_hex(owner),
            "agent": agent,
            "version": version,
            "expires_at": expires_at,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateActivated {
            mandate_id,
            agent,
            activated_at_ms,
        } => json!({
            "topic": "mandate_activated",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "activated_at_ms": activated_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateObjectiveEvaluated {
            mandate_id,
            agent,
            objective_index,
            score,
            conformance_score,
            pending_objectives,
            evaluated_at_ms,
        } => json!({
            "topic": "mandate_objective_evaluated",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "objective_index": objective_index,
            "score": score,
            "conformance_score": conformance_score,
            "pending_objectives": pending_objectives,
            "evaluated_at_ms": evaluated_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateConformanceEvaluated {
            mandate_id,
            agent,
            evaluated_at_ms,
            score,
            pending_objectives,
            state,
            prev_score,
            delta,
        } => json!({
            "topic": "mandate_conformance_evaluated",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "evaluated_at_ms": evaluated_at_ms,
            "score": score,
            "pending_objectives": pending_objectives,
            "state": state,
            "prev_score": prev_score,
            "delta": delta,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateConstraintTriggered {
            mandate_id,
            agent,
            constraint_index,
            action,
            triggered_at_ms,
        } => json!({
            "topic": "mandate_constraint_triggered",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "constraint_index": constraint_index,
            "action": mandate_enforcement_action_name(*action),
            "triggered_at_ms": triggered_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateBreached {
            mandate_id,
            agent,
            constraint_index,
            detected_at_ms,
        } => json!({
            "topic": "mandate_breached",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "constraint_index": constraint_index,
            "detected_at_ms": detected_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateSuspended {
            mandate_id,
            agent,
            by,
            suspended_at_ms,
        } => json!({
            "topic": "mandate_suspended",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "by_hex": encode_hex(by),
            "suspended_at_ms": suspended_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateResumed {
            mandate_id,
            agent,
            by,
            resumed_at_ms,
        } => json!({
            "topic": "mandate_resumed",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "by_hex": encode_hex(by),
            "resumed_at_ms": resumed_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateRevoked {
            mandate_id,
            agent,
            by,
            reason,
            revoked_at_ms,
        } => json!({
            "topic": "mandate_revoked",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "by_hex": encode_hex(by),
            "reason": reason,
            "revoked_at_ms": revoked_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MandateExpired {
            mandate_id,
            agent,
            expired_at_ms,
        } => json!({
            "topic": "mandate_expired",
            "mandate_id_hex": encode_hex(mandate_id),
            "agent": agent,
            "expired_at_ms": expired_at_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::MarketTradingStateUpdated { market_id, paused } => {
            json!({
                "topic": "market_trading_state_updated",
                "market_id": market_id,
                "paused": paused,
            })
        }
        kombat_chain_streams::RuntimeEventKind::OracleConfigUpdated {
            market_id,
            max_staleness_ms,
        } => json!({
            "topic": "oracle_config_updated",
            "market_id": market_id,
            "max_staleness_ms": max_staleness_ms,
        }),
        kombat_chain_streams::RuntimeEventKind::PositionUpdated {
            market_id,
            account_id,
            net_lots,
            open_notional,
            realized_pnl,
            average_entry_price_ticks,
            buy_lots,
            sell_lots,
            buy_notional,
            sell_notional,
        } => json!({
            "topic": "position_updated",
            "market_id": market_id,
            "account_id": account_id,
            "net_lots": net_lots,
            "open_notional": open_notional.to_string(),
            "realized_pnl": realized_pnl.to_string(),
            "average_entry_price_ticks": average_entry_price_ticks,
            "buy_lots": buy_lots,
            "sell_lots": sell_lots,
            "buy_notional": buy_notional.to_string(),
            "sell_notional": sell_notional.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::MarkPriceUpdated {
            market_id,
            mark_price_ticks,
        } => json!({
            "topic": "mark_price_updated",
            "market_id": market_id,
            "mark_price_ticks": mark_price_ticks,
        }),
        kombat_chain_streams::RuntimeEventKind::IndexPriceUpdated {
            market_id,
            index_price_ticks,
        } => json!({
            "topic": "index_price_updated",
            "market_id": market_id,
            "index_price_ticks": index_price_ticks,
        }),
        kombat_chain_streams::RuntimeEventKind::MarketOpenInterestUpdated {
            market_id,
            open_interest_lots,
        } => json!({
            "topic": "market_open_interest_updated",
            "market_id": market_id,
            "open_interest_lots": open_interest_lots.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::FundingIndexUpdated {
            market_id,
            funding_index,
        } => json!({
            "topic": "funding_index_updated",
            "market_id": market_id,
            "funding_index": funding_index.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::FundingApplied {
            market_id,
            account_id,
            funding_delta,
            funding_index,
        } => json!({
            "topic": "funding_applied",
            "market_id": market_id,
            "account_id": account_id,
            "funding_delta": funding_delta.to_string(),
            "funding_index": funding_index.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::CollateralDeposited {
            market_id,
            account_id,
            amount_usdc,
            collateral,
        } => json!({
            "topic": "collateral_deposited",
            "market_id": market_id,
            "account_id": account_id,
            "amount_usdc": amount_usdc.to_string(),
            "collateral": collateral.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::CollateralWithdrawn {
            market_id,
            account_id,
            amount_usdc,
            collateral,
        } => json!({
            "topic": "collateral_withdrawn",
            "market_id": market_id,
            "account_id": account_id,
            "amount_usdc": amount_usdc.to_string(),
            "collateral": collateral.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::RiskStateUpdated {
            market_id,
            account_id,
            net_lots,
            collateral,
            equity,
            maintenance_requirement,
            margin_surplus,
            under_maintenance,
        } => json!({
            "topic": "risk_state_updated",
            "market_id": market_id,
            "account_id": account_id,
            "net_lots": net_lots,
            "collateral": collateral.to_string(),
            "equity": equity.to_string(),
            "maintenance_requirement": maintenance_requirement.to_string(),
            "margin_surplus": margin_surplus.to_string(),
            "under_maintenance": under_maintenance,
        }),
        kombat_chain_streams::RuntimeEventKind::LiquidationCandidateUpdated {
            market_id,
            account_id,
            net_lots,
            equity,
            maintenance_requirement,
            margin_surplus,
            under_maintenance,
        } => json!({
            "topic": "liquidation_candidate_updated",
            "market_id": market_id,
            "account_id": account_id,
            "net_lots": net_lots,
            "equity": equity.to_string(),
            "maintenance_requirement": maintenance_requirement.to_string(),
            "margin_surplus": margin_surplus.to_string(),
            "under_maintenance": under_maintenance,
        }),
        kombat_chain_streams::RuntimeEventKind::InsuranceFundUpdated {
            market_id,
            delta,
            balance,
            source,
        } => json!({
            "topic": "insurance_fund_updated",
            "market_id": market_id,
            "delta": delta.to_string(),
            "balance": balance.to_string(),
            "source": insurance_fund_update_source_name(*source),
        }),
        kombat_chain_streams::RuntimeEventKind::BadDebtUpdated {
            market_id,
            account_id,
            delta,
            balance,
            source,
        } => json!({
            "topic": "bad_debt_updated",
            "market_id": market_id,
            "account_id": account_id,
            "delta": delta.to_string(),
            "balance": balance.to_string(),
            "source": risk_transition_source_name(*source),
        }),
        kombat_chain_streams::RuntimeEventKind::BadDebtRecovered {
            market_id,
            account_id,
            recovered,
            balance,
            source,
        } => json!({
            "topic": "bad_debt_recovered",
            "market_id": market_id,
            "account_id": account_id,
            "recovered": recovered.to_string(),
            "balance": balance.to_string(),
            "source": risk_transition_source_name(*source),
        }),
        kombat_chain_streams::RuntimeEventKind::SolvencyBreached {
            market_id,
            account_id,
            residual_bad_debt,
        } => json!({
            "topic": "solvency_breached",
            "market_id": market_id,
            "account_id": account_id,
            "residual_bad_debt": residual_bad_debt.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::LiquidationExecuted {
            market_id,
            account_id,
            liquidator_account_id,
            requested_lots,
            filled_lots,
            remaining_lots,
            executed_notional,
            penalty,
            insurance_cover,
            liquidator_reward,
            insurance_fund_balance,
        } => json!({
            "topic": "liquidation_executed",
            "market_id": market_id,
            "account_id": account_id,
            "liquidator_account_id": liquidator_account_id,
            "requested_lots": requested_lots,
            "filled_lots": filled_lots,
            "remaining_lots": remaining_lots,
            "executed_notional": executed_notional.to_string(),
            "penalty": penalty.to_string(),
            "insurance_cover": insurance_cover.to_string(),
            "liquidator_reward": liquidator_reward.to_string(),
            "insurance_fund_balance": insurance_fund_balance.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::LiquidationOutcome {
            market_id,
            account_id,
            liquidator_account_id,
            outcome,
            post_net_lots,
            post_margin_surplus,
            post_under_maintenance,
            post_account_frozen,
            post_market_bad_debt_balance,
        } => json!({
            "topic": "liquidation_outcome",
            "market_id": market_id,
            "account_id": account_id,
            "liquidator_account_id": liquidator_account_id,
            "outcome": liquidation_execution_outcome_name(*outcome),
            "post_net_lots": post_net_lots,
            "post_margin_surplus": post_margin_surplus.to_string(),
            "post_under_maintenance": post_under_maintenance,
            "post_account_frozen": post_account_frozen,
            "post_market_bad_debt_balance": post_market_bad_debt_balance.to_string(),
        }),
        kombat_chain_streams::RuntimeEventKind::LiquidationEscalationUpdated {
            market_id,
            account_id,
            action,
            stall_count,
            freeze_threshold,
            freeze_imminent,
            post_margin_surplus,
            post_under_maintenance,
            post_account_frozen,
        } => json!({
            "topic": "liquidation_escalation_updated",
            "market_id": market_id,
            "account_id": account_id,
            "action": liquidation_escalation_action_name(*action),
            "stall_count": stall_count,
            "freeze_threshold": freeze_threshold,
            "freeze_imminent": freeze_imminent,
            "post_margin_surplus": post_margin_surplus.to_string(),
            "post_under_maintenance": post_under_maintenance,
            "post_account_frozen": post_account_frozen,
        }),
        kombat_chain_streams::RuntimeEventKind::OrderReplaced {
            market_id,
            account_id,
            target_client_order_id,
            replacement_client_order_id,
            status,
            filled_lots,
            remaining_lots,
        } => json!({
            "topic": "order_replaced",
            "market_id": market_id,
            "account_id": account_id,
            "target_client_order_id": target_client_order_id,
            "replacement_client_order_id": replacement_client_order_id,
            "status": order_lifecycle_status_name(*status),
            "filled_lots": filled_lots,
            "remaining_lots": remaining_lots,
        }),
        kombat_chain_streams::RuntimeEventKind::OrderCancelled {
            market_id,
            account_id,
            client_order_id,
            remaining_lots,
        } => json!({
            "topic": "order_cancelled",
            "market_id": market_id,
            "account_id": account_id,
            "client_order_id": client_order_id,
            "remaining_lots": remaining_lots,
        }),
        kombat_chain_streams::RuntimeEventKind::PolicyCreated {
            account_id,
            parent_policy_id,
            policy_id,
        } => json!({
            "topic": "policy_created",
            "account_id": account_id,
            "parent_policy_id": parent_policy_id,
            "policy_id": policy_id,
        }),
        kombat_chain_streams::RuntimeEventKind::PolicyRevoked {
            account_id,
            policy_id,
            cascaded,
        } => json!({
            "topic": "policy_revoked",
            "account_id": account_id,
            "policy_id": policy_id,
            "cascaded": cascaded,
        }),
    }
}

pub(crate) fn event_filter_from_json(
    input: EventFilterJson,
) -> Result<EventFilter, JsonRpcErrorObject> {
    let topic = match input.topic.as_deref() {
        None => None,
        Some("intent_accepted") => Some(RuntimeEventTopic::IntentAccepted),
        Some("intent_promoted") => Some(RuntimeEventTopic::IntentPromoted),
        Some("intent_executed") => Some(RuntimeEventTopic::IntentExecuted),
        Some("intent_expired") => Some(RuntimeEventTopic::IntentExpired),
        Some("intent_rejected") => Some(RuntimeEventTopic::IntentRejected),
        Some("order_accepted") => Some(RuntimeEventTopic::OrderAccepted),
        Some("order_filled") => Some(RuntimeEventTopic::OrderFilled),
        Some("fee_charged") => Some(RuntimeEventTopic::FeeCharged),
        Some("fee_revenue_distributed") => Some(RuntimeEventTopic::FeeRevenueDistributed),
        Some("market_listed") => Some(RuntimeEventTopic::MarketListed),
        Some("market_status_updated") => Some(RuntimeEventTopic::MarketStatusUpdated),
        Some("market_maker_program_defined") => Some(RuntimeEventTopic::MarketMakerProgramDefined),
        Some("market_maker_program_retired") => Some(RuntimeEventTopic::MarketMakerProgramRetired),
        Some("market_maker_program_action_rejected") => {
            Some(RuntimeEventTopic::MarketMakerProgramActionRejected)
        }
        Some("market_maker_enrolled") => Some(RuntimeEventTopic::MarketMakerEnrolled),
        Some("market_maker_enrollment_exited") => {
            Some(RuntimeEventTopic::MarketMakerEnrollmentExited)
        }
        Some("mm_program_bonus_paid") => Some(RuntimeEventTopic::MmProgramBonusPaid),
        Some("mm_program_bonus_forfeited") => Some(RuntimeEventTopic::MmProgramBonusForfeited),
        Some("market_maker_enrollment_grace_entered") => {
            Some(RuntimeEventTopic::MarketMakerEnrollmentGraceEntered)
        }
        Some("market_maker_enrollment_recovered") => {
            Some(RuntimeEventTopic::MarketMakerEnrollmentRecovered)
        }
        Some("market_maker_enrollment_non_compliant") => {
            Some(RuntimeEventTopic::MarketMakerEnrollmentNonCompliant)
        }
        Some("market_trading_state_updated") => Some(RuntimeEventTopic::MarketTradingStateUpdated),
        Some("oracle_config_updated") => Some(RuntimeEventTopic::OracleConfigUpdated),
        Some("position_updated") => Some(RuntimeEventTopic::PositionUpdated),
        Some("mark_price_updated") => Some(RuntimeEventTopic::MarkPriceUpdated),
        Some("index_price_updated") => Some(RuntimeEventTopic::IndexPriceUpdated),
        Some("funding_index_updated") => Some(RuntimeEventTopic::FundingIndexUpdated),
        Some("funding_applied") => Some(RuntimeEventTopic::FundingApplied),
        Some("collateral_deposited") => Some(RuntimeEventTopic::CollateralDeposited),
        Some("collateral_withdrawn") => Some(RuntimeEventTopic::CollateralWithdrawn),
        Some("risk_state_updated") => Some(RuntimeEventTopic::RiskStateUpdated),
        Some("liquidation_candidate_updated") => {
            Some(RuntimeEventTopic::LiquidationCandidateUpdated)
        }
        Some("insurance_fund_updated") => Some(RuntimeEventTopic::InsuranceFundUpdated),
        Some("bad_debt_updated") => Some(RuntimeEventTopic::BadDebtUpdated),
        Some("bad_debt_recovered") => Some(RuntimeEventTopic::BadDebtRecovered),
        Some("solvency_breached") => Some(RuntimeEventTopic::SolvencyBreached),
        Some("liquidation_executed") => Some(RuntimeEventTopic::LiquidationExecuted),
        Some("liquidation_outcome") => Some(RuntimeEventTopic::LiquidationOutcome),
        Some("liquidation_escalation_updated") => {
            Some(RuntimeEventTopic::LiquidationEscalationUpdated)
        }
        Some("order_replaced") => Some(RuntimeEventTopic::OrderReplaced),
        Some("order_cancelled") => Some(RuntimeEventTopic::OrderCancelled),
        Some("account_created") => Some(RuntimeEventTopic::AccountCreated),
        Some("account_updated") => Some(RuntimeEventTopic::AccountUpdated),
        Some("account_operator_updated") => Some(RuntimeEventTopic::AccountOperatorUpdated),
        Some("identity_registered") => Some(RuntimeEventTopic::IdentityRegistered),
        Some("identity_updated") => Some(RuntimeEventTopic::IdentityUpdated),
        Some("identity_deleted") => Some(RuntimeEventTopic::IdentityDeleted),
        Some("attestation_registered") => Some(RuntimeEventTopic::AttestationRegistered),
        Some("attestation_verified") => Some(RuntimeEventTopic::AttestationVerified),
        Some("attestation_refreshed") => Some(RuntimeEventTopic::AttestationRefreshed),
        Some("attestation_revoked") => Some(RuntimeEventTopic::AttestationRevoked),
        Some("attestation_expired") => Some(RuntimeEventTopic::AttestationExpired),
        Some("tier_changed") => Some(RuntimeEventTopic::TierChanged),
        Some("vendor_root_introduced") => Some(RuntimeEventTopic::VendorRootIntroduced),
        Some("vendor_root_revoked") => Some(RuntimeEventTopic::VendorRootRevoked),
        Some("vendor_cap_profile_updated") => Some(RuntimeEventTopic::VendorCapProfileUpdated),
        Some("tee_governance_action_rejected") => {
            Some(RuntimeEventTopic::TeeGovernanceActionRejected)
        }
        Some("real_capital_listing_upserted") => {
            Some(RuntimeEventTopic::RealCapitalListingUpserted)
        }
        Some("real_capital_listing_deposit_enabled_updated") => {
            Some(RuntimeEventTopic::RealCapitalListingDepositEnabledUpdated)
        }
        Some("real_capital_listing_discovery_updated") => {
            Some(RuntimeEventTopic::RealCapitalListingDiscoveryUpdated)
        }
        Some("real_capital_protocol_cap_updated") => {
            Some(RuntimeEventTopic::RealCapitalProtocolCapUpdated)
        }
        Some("real_capital_global_kill_switch_updated") => {
            Some(RuntimeEventTopic::RealCapitalGlobalKillSwitchUpdated)
        }
        Some("real_capital_governance_action_rejected") => {
            Some(RuntimeEventTopic::RealCapitalGovernanceActionRejected)
        }
        Some("delegation_opened") => Some(RuntimeEventTopic::DelegationOpened),
        Some("delegation_deposited") => Some(RuntimeEventTopic::DelegationDeposited),
        Some("delegation_redemption_requested") => {
            Some(RuntimeEventTopic::DelegationRedemptionRequested)
        }
        Some("delegation_redemption_finalized") => {
            Some(RuntimeEventTopic::DelegationRedemptionFinalized)
        }
        Some("delegation_redemption_cancelled") => {
            Some(RuntimeEventTopic::DelegationRedemptionCancelled)
        }
        Some("delegation_crystallized") => Some(RuntimeEventTopic::DelegationCrystallized),
        Some("delegation_status_changed") => Some(RuntimeEventTopic::DelegationStatusChanged),
        Some("delegation_action_rejected") => Some(RuntimeEventTopic::DelegationActionRejected),
        Some("mandate_created") => Some(RuntimeEventTopic::MandateCreated),
        Some("mandate_activated") => Some(RuntimeEventTopic::MandateActivated),
        Some("mandate_objective_evaluated") => Some(RuntimeEventTopic::MandateObjectiveEvaluated),
        Some("mandate_conformance_evaluated") => {
            Some(RuntimeEventTopic::MandateConformanceEvaluated)
        }
        Some("mandate_constraint_triggered") => Some(RuntimeEventTopic::MandateConstraintTriggered),
        Some("mandate_breached") => Some(RuntimeEventTopic::MandateBreached),
        Some("mandate_suspended") => Some(RuntimeEventTopic::MandateSuspended),
        Some("mandate_resumed") => Some(RuntimeEventTopic::MandateResumed),
        Some("mandate_revoked") => Some(RuntimeEventTopic::MandateRevoked),
        Some("mandate_expired") => Some(RuntimeEventTopic::MandateExpired),
        Some("policy_created") => Some(RuntimeEventTopic::PolicyCreated),
        Some("policy_revoked") => Some(RuntimeEventTopic::PolicyRevoked),
        Some(_) => {
            return Err(JsonRpcErrorObject {
                code: -32602,
                message: "invalid event topic".into(),
                data: None,
            })
        }
    };

    let intent_id = input
        .intent_id_hex
        .as_deref()
        .map(decode_hex_array::<32>)
        .transpose()
        .map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?;

    Ok(EventFilter {
        block_height: input.block_height,
        account_id: input.account_id,
        market_id: input.market_id,
        policy_id: input.policy_id,
        intent_id,
        client_order_id: input.client_order_id,
        topic,
    })
}
