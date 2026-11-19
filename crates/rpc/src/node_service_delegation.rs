use crate::*;

impl<'a> NodeRpcService<'a> {
    // RFC 026 — Performance-Share Delegation read + open bridge.

    pub fn get_delegation(&self, delegation_id: &DelegationId) -> Option<DelegationResponse> {
        self.engine
            .delegation(delegation_id)
            .cloned()
            .map(|delegation| DelegationResponse { delegation })
    }

    pub fn get_delegation_share(
        &self,
        delegation_id: &DelegationId,
        principal: u64,
    ) -> DelegationShareResponse {
        DelegationShareResponse {
            delegation_id: *delegation_id,
            principal,
            share: self.engine.delegation_share(delegation_id, principal),
        }
    }

    pub fn list_delegations(
        &self,
        agent: Option<u64>,
        status: Option<DelegationStatus>,
        limit: usize,
    ) -> Vec<DelegationResponse> {
        self.engine
            .delegations()
            .filter(|delegation| agent.is_none_or(|value| delegation.agent == value))
            .filter(|delegation| status.is_none_or(|value| delegation.status == value))
            .take(limit)
            .cloned()
            .map(|delegation| DelegationResponse { delegation })
            .collect()
    }

    // RFC 049 — Vault Real-Capital Readiness Gate.

    pub fn list_redemption_requests(
        &self,
        principal: u64,
        vault_id: Option<vaults::VaultId>,
        status: Option<RedemptionRequestStatus>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ListRedemptionRequestsResponse, RpcError> {
        if limit == 0 {
            return Err(RpcError::InvalidRequest(
                "limit must be between 1 and 1000".into(),
            ));
        }
        let cursor_pair = match cursor {
            None => None,
            Some(value) => match decode_redemption_request_cursor(value) {
                Some(pair) => Some(pair),
                None => return Err(RpcError::InvalidRequest("invalid cursor".into())),
            },
        };
        let now_ms = self.engine.head().header.timestamp_ms;
        let mut items = self
            .engine
            .redemption_requests()
            .filter(|request| request.principal == principal)
            .filter_map(|request| {
                let request_vault_id = vaults::vault_id_for_delegation(&request.delegation_id);
                if vault_id.is_some_and(|filter| filter != request_vault_id) {
                    return None;
                }
                let effective_status = redemption_request_effective_status(request, now_ms);
                if status.is_some_and(|filter| filter != effective_status) {
                    return None;
                }
                let mut request = *request;
                request.status = effective_status;
                Some(RedemptionRequestListItem {
                    request,
                    vault_id: request_vault_id,
                })
            })
            .collect::<Vec<_>>();

        order_redemption_request_items(&mut items);

        if let Some((cursor_requested_at_ms, cursor_request_id)) = cursor_pair {
            items.retain(|item| {
                redemption_request_cursor_strictly_after(
                    item.request.requested_at_ms,
                    &item.request.request_id,
                    cursor_requested_at_ms,
                    &cursor_request_id,
                )
            });
        }

        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = if has_more {
            items.last().map(|tail| {
                encode_redemption_request_cursor(
                    tail.request.requested_at_ms,
                    tail.request.request_id,
                )
            })
        } else {
            None
        };
        Ok(ListRedemptionRequestsResponse { items, next_cursor })
    }

    pub fn list_vault_positions(
        &self,
        principal: u64,
        kind: Option<vaults::VaultKind>,
        status: Option<vaults::VaultStatus>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ListVaultPositionsResponse, RpcError> {
        if limit == 0 {
            return Err(RpcError::InvalidRequest(
                "limit must be between 1 and 1000".into(),
            ));
        }
        let cursor_vault_id = match cursor {
            None => None,
            Some(value) => match decode_vault_position_cursor(value) {
                Some(vault_id) => Some(vault_id),
                None => return Err(RpcError::InvalidRequest("invalid cursor".into())),
            },
        };
        let mut items = Vec::new();
        if matches!(kind, None | Some(vaults::VaultKind::Delegation)) {
            for delegation in self.engine.delegations() {
                let vault_id = vaults::vault_id_for_delegation(&delegation.delegation_id);
                let mapped_status = vaults::delegation_status_to_vault_status(delegation.status);
                if status.is_some_and(|filter| filter != mapped_status) {
                    continue;
                }
                let share = self
                    .engine
                    .delegation_share(&delegation.delegation_id, principal);
                if share.shares == 0 && share.locked_for_redemption == 0 {
                    continue;
                }
                let nav_equity = self
                    .engine
                    .current_delegation_equity_micro_usdc(delegation)
                    .map_err(delegation_rpc_error)?;
                let nav_per_share = if nav_equity <= 0 {
                    0
                } else {
                    delegation_nav_per_share_for_rpc(delegation, nav_equity as u128)?
                };
                let share_value_micro_usdc = share
                    .shares
                    .checked_mul(nav_per_share)
                    .and_then(|scaled| scaled.checked_div(SHARE_SCALE_FACTOR))
                    .ok_or_else(|| RpcError::Node("vault_position_value_overflow".into()))?;
                let compensation_claim_micro_usdc = self
                    .engine
                    .delegation_compensation_claim_micro_usdc(&delegation.delegation_id, principal);
                let estimated_value_micro_usdc = share_value_micro_usdc
                    .checked_add(compensation_claim_micro_usdc)
                    .ok_or_else(|| RpcError::Node("vault_position_value_overflow".into()))?;
                let live_trading = self.delegation_live_trading_info(delegation);
                let live_nav_per_share = live_trading
                    .as_ref()
                    .and_then(|info| u128::try_from(info.total_equity_micro_usdc).ok())
                    .map(|equity| delegation_nav_per_share_for_rpc(delegation, equity))
                    .transpose()?;
                let live_estimated_value_micro_usdc = live_nav_per_share
                    .map(|live_nav| {
                        share
                            .shares
                            .checked_mul(live_nav)
                            .and_then(|scaled| scaled.checked_div(SHARE_SCALE_FACTOR))
                            .ok_or_else(|| RpcError::Node("vault_position_value_overflow".into()))
                    })
                    .transpose()?
                    .map(|value| {
                        u128_to_i128_saturating_for_rpc(value).saturating_add(
                            u128_to_i128_saturating_for_rpc(compensation_claim_micro_usdc),
                        )
                    });
                items.push(VaultPositionListItem {
                    vault_id,
                    kind: vaults::VaultKind::Delegation,
                    delegation_id: delegation.delegation_id,
                    principal,
                    share,
                    principal_deposited_micro_usdc: self
                        .engine
                        .real_capital_principal_deposited_micro_usdc(&vault_id, principal),
                    authoritative_nav_per_share: nav_per_share,
                    authoritative_nav_source: "current_delegation_equity_nav",
                    indicative_nav_per_share: nav_per_share,
                    indicative_nav_source: "current_delegation_equity_nav",
                    live_trading,
                    live_nav_per_share,
                    live_estimated_value_micro_usdc,
                    compensation_claim_micro_usdc,
                    estimated_value_micro_usdc,
                    status: mapped_status,
                });
            }
        }

        order_vault_position_items(&mut items);

        if let Some(cursor_vault_id) = cursor_vault_id {
            items.retain(|item| item.vault_id > cursor_vault_id);
        }

        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = if has_more {
            items
                .last()
                .map(|tail| encode_vault_position_cursor(tail.vault_id))
        } else {
            None
        };
        Ok(ListVaultPositionsResponse { items, next_cursor })
    }

    pub(super) fn delegation_live_trading_info(
        &self,
        delegation: &Delegation,
    ) -> Option<vaults::VaultLiveTradingInfo> {
        let trading_account_id = delegation_trading_account_id(&delegation.delegation_id);
        let summary =
            self.get_account_risk_summary(trading_account_id, None, None, false, usize::MAX);
        let total_pnl = summary
            .total_realized_pnl
            .saturating_add(summary.total_unrealized_pnl);
        let has_live_trading_state = summary.market_count > 0
            || summary.total_collateral != 0
            || summary.total_open_notional > 0
            || total_pnl != 0;
        has_live_trading_state.then_some(vaults::VaultLiveTradingInfo {
            trading_account_id,
            market_count: summary.market_count,
            total_collateral_micro_usdc: summary.total_collateral,
            total_realized_pnl_micro_usdc: summary.total_realized_pnl,
            total_unrealized_pnl_micro_usdc: summary.total_unrealized_pnl,
            total_pnl_micro_usdc: total_pnl,
            total_equity_micro_usdc: summary.total_equity,
            total_open_notional_micro_usdc: summary.total_open_notional,
            total_initial_margin_required_micro_usdc: summary.total_initial_margin_required,
            total_maintenance_requirement_micro_usdc: summary.total_maintenance_requirement,
            total_margin_surplus_micro_usdc: summary.total_margin_surplus,
        })
    }

    // RFC 043 — Vaults Discovery Surface.

    /// Project every live delegation into a `VaultRecord`, applying the
    /// caller's filters. Default ordering is `tvl_micro_usdc` desc,
    /// `vault_id` asc as the deterministic tiebreaker. Other `VaultKind`s
    /// (RFC 028 / 029) wire up when those primitives land.
    ///
    /// Errors propagate from the underlying balance read; a TVL the engine
    /// cannot compute deterministically is an `RpcError`, not a silent zero.
    pub fn list_vaults(
        &self,
        kind: Option<vaults::VaultKind>,
        status: Option<vaults::VaultStatus>,
        operator: Option<u64>,
        currencies: Option<Vec<u64>>,
        include_unlisted: bool,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ListVaultsResponse, RpcError> {
        // `currencies = None` ⇒ default USDC-only per RFC 043. Empty list ⇒
        // any currency. The Delegation kind is always USDC, so the filter
        // matters once SupplyBorrowPool lands; honor it now for consistency.
        let currency_filter = currencies.as_deref();
        let want_currency = |asset_id: u64| match currency_filter {
            None => asset_id == vaults::VAULT_USDC_ASSET_ID,
            Some(list) if list.is_empty() => true,
            Some(list) => list.contains(&asset_id),
        };

        let cursor_pair = match cursor {
            None => None,
            Some(value) => match vaults::decode_vault_cursor(value) {
                Some(pair) => Some(pair),
                None => return Err(RpcError::InvalidRequest("invalid cursor".into())),
            },
        };

        let mut records: Vec<vaults::VaultRecord> = Vec::new();

        if matches!(kind, None | Some(vaults::VaultKind::Delegation)) {
            for delegation in self.engine.delegations() {
                if let Some(filter) = operator {
                    if delegation.agent != filter {
                        continue;
                    }
                }
                let mapped_status = vaults::delegation_status_to_vault_status(delegation.status);
                if let Some(filter) = status {
                    if mapped_status != filter {
                        continue;
                    }
                }
                if !want_currency(vaults::VAULT_USDC_ASSET_ID) {
                    continue;
                }
                let tvl = self
                    .engine
                    .delegation_free_usdc_balance(&delegation.delegation_id)
                    .map_err(delegation_rpc_error)?;
                let vault_id = vaults::vault_id_for_delegation(&delegation.delegation_id);
                let listing = self.engine.real_capital_listing(&vault_id);
                let discovery = listing
                    .map(|listing| vaults::DiscoveryFlags {
                        listed: listing.listed_for_discovery,
                        featured: false,
                    })
                    .unwrap_or(vaults::DiscoveryFlags {
                        listed: false,
                        featured: false,
                    });
                let real_capital = listing.map(|listing| self.vault_real_capital_info(listing));
                let record = vaults::delegation_to_vault_record(
                    delegation.clone(),
                    tvl,
                    discovery,
                    real_capital,
                    self.delegation_live_trading_info(delegation),
                );
                if !include_unlisted && !record.discovery.listed {
                    continue;
                }
                records.push(record);
            }
        }

        vaults::order_vault_records(&mut records);

        // Apply the cursor: drop everything not strictly-after the cursor.
        if let Some((tvl_c, id_c)) = cursor_pair {
            records.retain(|record| {
                vaults::cursor_strictly_after(record.tvl_micro_usdc, &record.vault_id, tvl_c, &id_c)
            });
        }

        // Take limit + 1 to detect overflow → emit cursor.
        let has_more = records.len() > limit;
        records.truncate(limit);
        let next_cursor = if has_more {
            records
                .last()
                .map(|tail| vaults::encode_vault_cursor(tail.tvl_micro_usdc, tail.vault_id))
        } else {
            None
        };
        Ok(ListVaultsResponse {
            items: records,
            next_cursor,
        })
    }

    /// Resolve a `VaultId` to its underlying primitive. `kombat_getVault`
    /// returns the record regardless of `discovery.listed`.
    pub fn get_vault(&self, vault_id: &vaults::VaultId) -> Result<GetVaultResponse, RpcError> {
        for delegation in self.engine.delegations() {
            if vaults::vault_id_for_delegation(&delegation.delegation_id) == *vault_id {
                let tvl = self
                    .engine
                    .delegation_free_usdc_balance(&delegation.delegation_id)
                    .map_err(delegation_rpc_error)?;
                let listing = self.engine.real_capital_listing(vault_id);
                let record = vaults::delegation_to_vault_record(
                    delegation.clone(),
                    tvl,
                    listing
                        .map(|listing| vaults::DiscoveryFlags {
                            listed: listing.listed_for_discovery,
                            featured: false,
                        })
                        .unwrap_or(vaults::DiscoveryFlags {
                            listed: false,
                            featured: false,
                        }),
                    listing.map(|listing| self.vault_real_capital_info(listing)),
                    self.delegation_live_trading_info(delegation),
                );
                return Ok(GetVaultResponse {
                    record: Some(record),
                    present: true,
                });
            }
        }
        Ok(GetVaultResponse {
            record: None,
            present: false,
        })
    }

    pub fn list_real_capital_listings(
        &self,
        agent: Option<u64>,
        mode: Option<VaultRealCapitalMode>,
        include_unlisted: bool,
        include_disabled: bool,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ListRealCapitalListingsResponse, RpcError> {
        let cursor_vault_id = match cursor {
            None => None,
            Some(value) => Some(
                decode_hex_array::<16>(value)
                    .map_err(|_| RpcError::InvalidRequest("invalid cursor".into()))?,
            ),
        };
        let mut items = self
            .engine
            .real_capital_listings()
            .filter(|listing| agent.is_none_or(|filter| listing.serving_agent == filter))
            .filter(|listing| mode.is_none_or(|filter| listing.mode == filter))
            .filter(|listing| include_unlisted || listing.listed_for_discovery)
            .filter(|listing| include_disabled || listing.deposit_enabled)
            .filter(|listing| cursor_vault_id.is_none_or(|cursor| listing.vault_id > cursor))
            .map(|listing| self.real_capital_listing_response(listing))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.vault_id.cmp(&b.vault_id));
        let has_more = items.len() > limit;
        items.truncate(limit);
        let next_cursor = has_more
            .then(|| items.last().map(|item| encode_hex(&item.vault_id)))
            .flatten();
        Ok(ListRealCapitalListingsResponse {
            items,
            next_cursor,
            control: self.real_capital_control_response()?,
        })
    }

    pub fn get_real_capital_listing(
        &self,
        vault_id: &vaults::VaultId,
    ) -> Result<GetRealCapitalListingResponse, RpcError> {
        Ok(GetRealCapitalListingResponse {
            listing: self
                .engine
                .real_capital_listing(vault_id)
                .map(|listing| self.real_capital_listing_response(listing)),
            present: self.engine.real_capital_listing(vault_id).is_some(),
            control: self.real_capital_control_response()?,
        })
    }

    pub fn submit_real_capital_governance_action(
        &mut self,
        params: SubmitRealCapitalGovernanceActionParams,
    ) -> Result<SubmitRealCapitalGovernanceActionResponse, RpcError> {
        let action = real_capital_governance_action_from_json(params.action)?;
        let action_id =
            real_capital_governance_action_id_for_submission(&action, params.timestamp_ms);
        let status = self
            .engine
            .submit_real_capital_governance_action(action_id, action, params.timestamp_ms)
            .map_err(real_capital_governance_rpc_error)?;
        Ok(SubmitRealCapitalGovernanceActionResponse {
            action_id,
            status,
            pending_real_capital_governance_actions: self
                .engine
                .pending_real_capital_governance_action_count(),
        })
    }

    pub(super) fn real_capital_governance_action_status_for_id(
        &self,
        action_id: RealCapitalGovernanceActionId,
    ) -> RealCapitalGovernanceActionStatusResponse {
        if let Some(processed) = self
            .engine
            .processed_real_capital_governance_action(action_id)
        {
            return RealCapitalGovernanceActionStatusResponse {
                action_id,
                status: if matches!(
                    &processed.receipt,
                    RealCapitalGovernanceActionReceipt::Rejected { .. }
                ) {
                    RealCapitalGovernanceActionStatus::Rejected
                } else {
                    RealCapitalGovernanceActionStatus::Processed
                },
                action_kind: real_capital_governance_action_kind_for_receipt(&processed.receipt),
                submitted_at_ms: Some(processed.submitted_at_ms),
                processed_at_ms: Some(processed.processed_at_ms),
                receipt: Some(processed.receipt.clone()),
            };
        }
        if let Some(pending) = self
            .engine
            .pending_real_capital_governance_action(action_id)
        {
            return RealCapitalGovernanceActionStatusResponse {
                action_id,
                status: RealCapitalGovernanceActionStatus::Pending,
                action_kind: Some(real_capital_governance_block_action_kind(&pending.action)),
                submitted_at_ms: Some(pending.submitted_at_ms),
                processed_at_ms: None,
                receipt: None,
            };
        }
        RealCapitalGovernanceActionStatusResponse {
            action_id,
            status: RealCapitalGovernanceActionStatus::Unknown,
            action_kind: None,
            submitted_at_ms: None,
            processed_at_ms: None,
            receipt: None,
        }
    }

    pub fn get_real_capital_governance_action_status(
        &self,
        params: GetRealCapitalGovernanceActionStatusParams,
    ) -> Result<RealCapitalGovernanceActionStatusResponse, RpcError> {
        let action_id = decode_hex_array::<32>(&params.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex"))?;
        Ok(self.real_capital_governance_action_status_for_id(action_id))
    }

    pub(super) fn real_capital_listing_response(
        &self,
        listing: &RealCapitalListing,
    ) -> RealCapitalListingResponse {
        RealCapitalListingResponse {
            vault_id: listing.vault_id,
            delegation_id: listing.delegation_id,
            serving_agent: listing.serving_agent,
            mode: listing.mode,
            pilot_min_serving_agent_tier: listing
                .pilot_min_serving_agent_tier
                .map(identity_tier_response),
            tier0_pilot_exception: listing.tier0_pilot_exception,
            deposit_enabled: listing.deposit_enabled,
            listed_for_discovery: listing.listed_for_discovery,
            per_principal_cap_micro_usdc: listing.caps.per_principal_micro_usdc,
            total_vault_tvl_cap_micro_usdc: listing.caps.total_vault_tvl_micro_usdc,
            protocol_cap_micro_usdc: self
                .engine
                .protocol_delegation_cap()
                .map(|cap| cap.total_protocol_tvl_micro_usdc),
            vault_deposited_micro_usdc: self
                .engine
                .real_capital_vault_deposited_micro_usdc(&listing.vault_id),
            pilot_principal_allowlist: listing
                .pilot_principal_allowlist
                .as_ref()
                .map(|set| set.iter().copied().collect()),
            listed_at_block: listing.listed_at_block,
            last_updated_block: listing.last_updated_block,
            last_updated_governance_action: listing.last_updated_governance_action,
        }
    }

    pub(super) fn real_capital_control_response(
        &self,
    ) -> Result<RealCapitalControlResponse, RpcError> {
        let control = self.engine.listing_control_state();
        Ok(RealCapitalControlResponse {
            global_deposit_kill_switch: control.global_deposit_kill_switch,
            last_updated_block: control.last_updated_block,
            last_updated_governance_action: control.last_updated_governance_action,
            protocol_cap_micro_usdc: self
                .engine
                .protocol_delegation_cap()
                .map(|cap| cap.total_protocol_tvl_micro_usdc),
            total_protocol_delegation_tvl_micro_usdc: self
                .engine
                .total_protocol_delegation_tvl_micro_usdc()
                .map_err(delegation_rpc_error)?,
        })
    }

    pub(super) fn vault_real_capital_info(
        &self,
        listing: &RealCapitalListing,
    ) -> vaults::RealCapitalInfo {
        vaults::RealCapitalInfo {
            mode: real_capital_mode_name(listing.mode).to_string(),
            pilot_min_serving_agent_tier: listing
                .pilot_min_serving_agent_tier
                .map(identity_tier_response)
                .map(identity_tier_name)
                .map(ToString::to_string),
            tier0_pilot_exception: listing.tier0_pilot_exception,
            deposit_enabled: listing.deposit_enabled,
            listed_for_discovery: listing.listed_for_discovery,
            global_deposit_kill_switch: self
                .engine
                .listing_control_state()
                .global_deposit_kill_switch,
            per_principal_cap_micro_usdc: listing.caps.per_principal_micro_usdc,
            total_vault_tvl_cap_micro_usdc: listing.caps.total_vault_tvl_micro_usdc,
            protocol_cap_micro_usdc: self
                .engine
                .protocol_delegation_cap()
                .map(|cap| cap.total_protocol_tvl_micro_usdc),
            vault_deposited_micro_usdc: self
                .engine
                .real_capital_vault_deposited_micro_usdc(&listing.vault_id),
            listed_at_block: listing.listed_at_block,
            last_updated_block: listing.last_updated_block,
            last_updated_governance_action_hex: encode_hex(&listing.last_updated_governance_action),
        }
    }

    pub(super) fn commit_delegation_action(
        &mut self,
        action_id: DelegationActionId,
        action: BlockDelegationAction,
        timestamp_ms: u64,
    ) -> Result<DelegationCommitOutcome, RpcError> {
        let action_kind = delegation_block_action_kind(&action);
        let fingerprint = delegation_action_fingerprint(&action);
        if let Some(processed) = self.engine.processed_delegation_action(action_id) {
            if processed.fingerprint != fingerprint {
                return Err(RpcError::Node("delegation_action_id_mismatch".into()));
            }
            return match &processed.receipt {
                DelegationActionReceipt::Rejected { reason, .. } => {
                    Err(RpcError::Node(reason.clone()))
                }
                receipt => Ok(DelegationCommitOutcome::Duplicate {
                    receipt: receipt.clone(),
                }),
            };
        }
        self.engine
            .commit_delegation_action_block(action_id, action, timestamp_ms)
            .map_err(delegation_rpc_error)?;
        let block_height = self.engine.head().header.height;
        if let Some(reason) = self
            .runtime_events_in_block_rev(block_height)
            .find_map(|event| match &event.kind {
                RuntimeEventKind::DelegationActionRejected {
                    action_kind: rejected_kind,
                    reason,
                } if rejected_kind == action_kind => Some(reason.clone()),
                _ => None,
            })
        {
            return Err(RpcError::Node(reason));
        }
        Ok(DelegationCommitOutcome::Committed { block_height })
    }

    pub fn submit_delegation_action(
        &mut self,
        params: SubmitDelegationActionParams,
    ) -> Result<DelegationActionSubmissionResponse, RpcError> {
        let action_id = decode_hex_array::<32>(&params.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?;
        let action = decode_borsh_hex::<BlockDelegationAction>(&params.action_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_hex".into()))?;
        let expected_action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        if action_id != expected_action_id {
            return Err(RpcError::Node("delegation_action_id_mismatch".into()));
        }
        let status = self
            .engine
            .submit_delegation_action(action_id, action, params.timestamp_ms)
            .map_err(delegation_rpc_error)?;
        Ok(DelegationActionSubmissionResponse {
            action_id,
            status,
            pending_delegation_actions: self.engine.pending_delegation_action_count(),
        })
    }

    pub fn submit_signed_delegation_action(
        &mut self,
        params: SubmitSignedDelegationActionParams,
    ) -> Result<DelegationActionSubmissionResponse, RpcError> {
        let signed_action = signed_delegation_action_from_json(&params.signed_delegation_action)?;
        let action_id = signed_action.action_id;
        let status = self
            .engine
            .submit_signed_delegation_action(signed_action)
            .map_err(delegation_rpc_error)?;
        Ok(DelegationActionSubmissionResponse {
            action_id,
            status,
            pending_delegation_actions: self.engine.pending_delegation_action_count(),
        })
    }

    pub fn submit_evm_delegation_action(
        &mut self,
        params: SubmitEvmDelegationActionParams,
    ) -> Result<DelegationActionSubmissionResponse, RpcError> {
        let signed_action =
            signed_evm_delegation_action_from_json(&params.signed_delegation_action)?;
        let signer_account_id = self.verify_evm_delegation_action(&signed_action)?;
        let action_id = signed_action.action_id;
        let status = self
            .engine
            .submit_verified_delegation_action(
                signer_account_id,
                signed_action.action_id,
                signed_action.action,
                signed_action.timestamp_ms,
            )
            .map_err(delegation_rpc_error)?;
        Ok(DelegationActionSubmissionResponse {
            action_id,
            status,
            pending_delegation_actions: self.engine.pending_delegation_action_count(),
        })
    }

    pub(super) fn verify_evm_delegation_action(
        &self,
        signed_action: &SignedEvmDelegationAction,
    ) -> Result<u64, RpcError> {
        if signed_action.chain_id != self.engine.chain_id() {
            return Err(delegation_rpc_error(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "chain_mismatch",
            }));
        }
        if signed_action.valid_until_ms < current_unix_ms() {
            return Err(delegation_rpc_error(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "signature_expired",
            }));
        }
        if !matches!(
            signed_action.action,
            BlockDelegationAction::Deposit { .. }
                | BlockDelegationAction::RequestRedemption { .. }
                | BlockDelegationAction::CancelRedemption { .. }
        ) {
            return Err(delegation_rpc_error(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "wallet_write_not_supported",
            }));
        }
        let expected_action_id = delegation_action_id_for_submission(
            &signed_action.action,
            signed_action.timestamp_ms,
            Some(signed_action.submitter_nonce),
        );
        if signed_action.action_id != expected_action_id {
            return Err(delegation_rpc_error(NodeError::InvalidDelegation {
                agent: signed_action.signer_account_id,
                reason: "delegation_action_id_mismatch",
            }));
        }
        let expected_signer = self
            .engine
            .delegation_action_signer_account(&signed_action.action)
            .map_err(delegation_rpc_error)?;
        if signed_action.signer_account_id != expected_signer {
            return Err(delegation_rpc_error(NodeError::InvalidDelegation {
                agent: expected_signer,
                reason: "evm_signer_mismatch",
            }));
        }
        let linked_address = self.account_evm_address(expected_signer).ok_or_else(|| {
            delegation_rpc_error(NodeError::InvalidDelegation {
                agent: expected_signer,
                reason: "evm_signer_not_supported",
            })
        })?;
        let digest = evm_delegation_action_eip712_digest(signed_action).map_err(|_| {
            delegation_rpc_error(NodeError::InvalidDelegation {
                agent: expected_signer,
                reason: "invalid_evm_signature",
            })
        })?;
        let recovered =
            recover_evm_owner_call_address(&digest, &signed_action.signature).map_err(|_| {
                delegation_rpc_error(NodeError::InvalidDelegation {
                    agent: expected_signer,
                    reason: "invalid_evm_signature",
                })
            })?;
        if recovered != linked_address {
            return Err(delegation_rpc_error(NodeError::InvalidDelegation {
                agent: expected_signer,
                reason: "evm_signer_mismatch",
            }));
        }
        Ok(expected_signer)
    }

    pub(crate) fn delegation_action_status_for_id(
        &self,
        action_id: DelegationActionId,
    ) -> DelegationActionStatusResponse {
        if let Some(processed) = self.engine.processed_delegation_action(action_id) {
            return DelegationActionStatusResponse {
                action_id,
                status: DelegationActionStatus::Processed,
                action_kind: delegation_action_kind_for_receipt(&processed.receipt),
                submitted_at_ms: None,
                processed_at_ms: Some(processed.processed_at_ms),
                receipt: Some(processed.receipt.clone()),
            };
        }
        if let Some(pending) = self.engine.pending_delegation_action(action_id) {
            return DelegationActionStatusResponse {
                action_id,
                status: DelegationActionStatus::Pending,
                action_kind: Some(delegation_block_action_kind(&pending.action)),
                submitted_at_ms: Some(pending.submitted_at_ms),
                processed_at_ms: None,
                receipt: None,
            };
        }
        DelegationActionStatusResponse {
            action_id,
            status: DelegationActionStatus::Unknown,
            action_kind: None,
            submitted_at_ms: None,
            processed_at_ms: None,
            receipt: None,
        }
    }

    pub fn get_delegation_action_status(
        &self,
        params: GetDelegationActionStatusParams,
    ) -> Result<DelegationActionStatusResponse, RpcError> {
        let action_id = decode_hex_array::<32>(&params.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?;
        Ok(self.delegation_action_status_for_id(action_id))
    }

    pub fn submit_signed_market_maker_program_action(
        &mut self,
        params: SubmitSignedMarketMakerProgramActionParams,
    ) -> Result<MarketMakerProgramActionSubmissionResponse, RpcError> {
        let signed_action = signed_market_maker_program_action_from_json(
            &params.signed_market_maker_program_action,
        )?;
        let action_id = signed_action.action_id;
        let status = self
            .engine
            .submit_signed_market_maker_program_action(signed_action)
            .map_err(market_maker_program_rpc_error)?;
        Ok(MarketMakerProgramActionSubmissionResponse {
            action_id,
            status,
            pending_market_maker_program_actions: self
                .engine
                .pending_market_maker_program_action_count(),
        })
    }

    pub fn submit_market_maker_program_action(
        &mut self,
        params: SubmitMarketMakerProgramActionParams,
    ) -> Result<MarketMakerProgramActionSubmissionResponse, RpcError> {
        let action_id = decode_hex_array::<32>(&params.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?;
        let action = decode_borsh_hex::<BlockMarketMakerProgramAction>(&params.action_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_hex".into()))?;
        let expected_action_id = market_maker_program_action_id_for_submission(
            &action,
            params.timestamp_ms,
            params.submitter_nonce,
        );
        if action_id != expected_action_id {
            return Err(RpcError::Node(
                "market_maker_program_action_id_mismatch".into(),
            ));
        }
        self.submit_market_maker_program_action_inner(action_id, action, params.timestamp_ms)
    }

    pub(super) fn submit_market_maker_program_action_inner(
        &mut self,
        action_id: MarketMakerProgramActionId,
        action: BlockMarketMakerProgramAction,
        timestamp_ms: u64,
    ) -> Result<MarketMakerProgramActionSubmissionResponse, RpcError> {
        let status = self
            .engine
            .submit_market_maker_program_action(action_id, action, timestamp_ms)
            .map_err(market_maker_program_rpc_error)?;
        Ok(MarketMakerProgramActionSubmissionResponse {
            action_id,
            status,
            pending_market_maker_program_actions: self
                .engine
                .pending_market_maker_program_action_count(),
        })
    }

    pub fn define_market_maker_program(
        &mut self,
        params: DefineMarketMakerProgramParams,
    ) -> Result<(MarketMakerProgramActionSubmissionResponse, [u8; 32]), RpcError> {
        let min_quote_size_micro_usdc = parse_market_maker_u256(
            &params.min_quote_size_micro_usdc,
            "min_quote_size_micro_usdc",
        )?;
        let enrollment_bond_micro_usdc = parse_market_maker_u256(
            &params.enrollment_bond_micro_usdc,
            "enrollment_bond_micro_usdc",
        )?;
        let action = BlockMarketMakerProgramAction::Define {
            name: params.name,
            markets: params.markets.into_iter().collect(),
            min_uptime_bps: params.min_uptime_bps,
            max_spread_bps: params.max_spread_bps,
            min_quote_size_micro_usdc: min_quote_size_micro_usdc.0,
            min_two_sided_windows_per_hour: params.min_two_sided_windows_per_hour,
            rebate_multiplier_units: params.rebate_multiplier_units,
            enrollment_bond_micro_usdc: enrollment_bond_micro_usdc.0,
            compliance_window_ms: params.compliance_window_ms,
            observation_cadence_ms: params.observation_cadence_ms,
        };
        let program_id = match &action {
            BlockMarketMakerProgramAction::Define {
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
            } => derive_market_maker_program_id(&DefineMarketMakerProgramInput {
                name: name.clone(),
                markets: markets.clone(),
                min_uptime_bps: *min_uptime_bps,
                max_spread_bps: *max_spread_bps,
                min_quote_size_micro_usdc: MarketMakerU256(*min_quote_size_micro_usdc),
                min_two_sided_windows_per_hour: *min_two_sided_windows_per_hour,
                rebate_multiplier_units: *rebate_multiplier_units,
                enrollment_bond_micro_usdc: MarketMakerU256(*enrollment_bond_micro_usdc),
                compliance_window_ms: *compliance_window_ms,
                observation_cadence_ms: *observation_cadence_ms,
            }),
            _ => unreachable!("define action constructed above"),
        };
        let action_id = market_maker_program_action_id_for_submission(
            &action,
            params.timestamp_ms,
            params.submitter_nonce,
        );
        self.submit_market_maker_program_action_inner(action_id, action, params.timestamp_ms)
            .map(|response| (response, program_id))
    }

    pub fn enroll_market_maker_program(
        &mut self,
        params: EnrollMarketMakerProgramParams,
    ) -> Result<MarketMakerProgramActionSubmissionResponse, RpcError> {
        let program_id = decode_hex_array::<32>(&params.program_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid program_id_hex".into()))?;
        let bond_micro_usdc = parse_market_maker_u256(&params.bond_micro_usdc, "bond_micro_usdc")?;
        let market_maker_identity = market_maker_identity_from_params(
            params.market_maker,
            params.market_maker_identity.as_ref(),
        )?;
        let action = BlockMarketMakerProgramAction::Enroll {
            market_maker_identity,
            market_id: params.market_id,
            program_id,
            bond_micro_usdc: bond_micro_usdc.0,
        };
        let action_id = market_maker_program_action_id_for_submission(
            &action,
            params.timestamp_ms,
            params.submitter_nonce,
        );
        self.submit_market_maker_program_action_inner(action_id, action, params.timestamp_ms)
    }

    pub(crate) fn market_maker_program_action_status_for_id(
        &self,
        action_id: MarketMakerProgramActionId,
    ) -> MarketMakerProgramActionStatusResponse {
        if let Some(processed) = self.engine.processed_market_maker_program_action(action_id) {
            return MarketMakerProgramActionStatusResponse {
                action_id,
                status: MarketMakerProgramActionStatus::Processed,
                action_kind: market_maker_program_action_kind_for_receipt(&processed.receipt),
                submitted_at_ms: None,
                processed_at_ms: Some(processed.processed_at_ms),
                receipt: Some(processed.receipt.clone()),
            };
        }
        if let Some(pending) = self.engine.pending_market_maker_program_action(action_id) {
            return MarketMakerProgramActionStatusResponse {
                action_id,
                status: MarketMakerProgramActionStatus::Pending,
                action_kind: Some(market_maker_program_block_action_kind(&pending.action)),
                submitted_at_ms: Some(pending.submitted_at_ms),
                processed_at_ms: None,
                receipt: None,
            };
        }
        MarketMakerProgramActionStatusResponse {
            action_id,
            status: MarketMakerProgramActionStatus::Unknown,
            action_kind: None,
            submitted_at_ms: None,
            processed_at_ms: None,
            receipt: None,
        }
    }

    pub fn get_market_maker_program_action_status(
        &self,
        params: GetMarketMakerProgramActionStatusParams,
    ) -> Result<MarketMakerProgramActionStatusResponse, RpcError> {
        let action_id = decode_hex_array::<32>(&params.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?;
        Ok(self.market_maker_program_action_status_for_id(action_id))
    }

    pub fn get_market_maker_program(
        &self,
        params: GetMarketMakerProgramParams,
    ) -> Result<Option<MarketMakerProgram>, RpcError> {
        let program_id = decode_hex_array::<32>(&params.program_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid program_id_hex".into()))?;
        Ok(self.engine.market_maker_program(&program_id).cloned())
    }

    pub fn get_market_maker_enrollment(
        &self,
        params: GetMarketMakerEnrollmentParams,
    ) -> Result<Option<MarketMakerEnrollment>, RpcError> {
        let program_id = decode_hex_array::<32>(&params.program_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid program_id_hex".into()))?;
        let market_maker_identity = market_maker_identity_from_params(
            params.market_maker,
            params.market_maker_identity.as_ref(),
        )?;
        Ok(self
            .engine
            .market_maker_enrollment(&(market_maker_identity, params.market_id, program_id))
            .cloned())
    }

    pub(super) fn runtime_events_in_block_rev(
        &self,
        block_height: u64,
    ) -> impl Iterator<Item = &RuntimeEventRecord> {
        // Delegation RPC responses only need events from the block just
        // committed by the bridge. Stop at the first older/non-block event
        // instead of scanning the full append-only runtime event history.
        self.engine
            .runtime_events()
            .iter()
            .rev()
            .take_while(move |event| event.block_height == Some(block_height))
    }

    pub(super) fn policy_usdc_balance_for_account(
        &self,
        account_id: u64,
    ) -> Result<u128, RpcError> {
        let address = self
            .engine
            .get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or_else(|| RpcError::Node("account_evm_address_missing".into()))?;
        let balance = self
            .engine
            .get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes.as_slice())
                    .map(U256::from_be_bytes)
                    .map_err(|_| RpcError::Node("policy_usdc_balance_malformed".into()))
            })
            .transpose()?
            .unwrap_or(U256::ZERO);
        Ok(balance.min(U256::from(u128::MAX)).to::<u128>())
    }

    pub(super) fn delegation_balance(
        &self,
        delegation_id: &DelegationId,
    ) -> Result<u128, RpcError> {
        let trading_account = self
            .engine
            .delegation_trading_account(delegation_id)
            .ok_or_else(|| RpcError::Node("unknown_delegation".into()))?;
        self.policy_usdc_balance_for_account(trading_account)
    }

    pub(super) fn delegation_opened_in_block(&self, block_height: u64) -> Option<DelegationId> {
        self.runtime_events_in_block_rev(block_height)
            .find_map(|event| match event.kind {
                RuntimeEventKind::DelegationOpened { delegation_id, .. } => Some(delegation_id),
                _ => None,
            })
    }

    pub(super) fn redemption_requested_in_block(
        &self,
        block_height: u64,
    ) -> Option<RedemptionRequestId> {
        self.runtime_events_in_block_rev(block_height)
            .find_map(|event| match event.kind {
                RuntimeEventKind::DelegationRedemptionRequested { request_id, .. } => {
                    Some(request_id)
                }
                _ => None,
            })
    }

    pub(super) fn deposit_shares_minted_in_block(
        &self,
        block_height: u64,
        delegation_id: DelegationId,
        principal: u64,
    ) -> Option<u128> {
        self.runtime_events_in_block_rev(block_height)
            .find_map(|event| match event.kind {
                RuntimeEventKind::DelegationDeposited {
                    delegation_id: event_delegation_id,
                    principal: event_principal,
                    shares_minted,
                    ..
                } if event_delegation_id == delegation_id && event_principal == principal => {
                    Some(shares_minted)
                }
                _ => None,
            })
    }

    pub(super) fn finalized_redemption_outputs_in_block(
        &self,
        block_height: u64,
        request_id: RedemptionRequestId,
    ) -> Option<(u128, u128)> {
        self.runtime_events_in_block_rev(block_height)
            .find_map(|event| match event.kind {
                RuntimeEventKind::DelegationRedemptionFinalized {
                    request_id: event_request_id,
                    usdc_out,
                    compensation_out,
                    ..
                } if event_request_id == request_id => Some((usdc_out, compensation_out)),
                _ => None,
            })
    }

    pub(super) fn delegation_deposit_shares_minted(
        &self,
        delegation_id: DelegationId,
        principal: u64,
        amount: u128,
        timestamp_ms: u64,
    ) -> Option<u128> {
        self.engine
            .runtime_events()
            .iter()
            .rev()
            .take(MAX_DELEGATION_IDEMPOTENCY_EVENT_SCAN)
            .find_map(|event| match event.kind {
                RuntimeEventKind::DelegationDeposited {
                    delegation_id: event_delegation_id,
                    principal: event_principal,
                    amount: event_amount,
                    shares_minted,
                    ..
                } if event_delegation_id == delegation_id
                    && event_principal == principal
                    && event_amount == amount
                    && event.timestamp_ms == timestamp_ms =>
                {
                    Some(shares_minted)
                }
                _ => None,
            })
    }

    pub(super) fn finalized_redemption_outputs_for_request(
        &self,
        request_id: RedemptionRequestId,
    ) -> Option<(u128, u128)> {
        self.engine
            .runtime_events()
            .iter()
            .rev()
            .take(MAX_DELEGATION_IDEMPOTENCY_EVENT_SCAN)
            .find_map(|event| match event.kind {
                RuntimeEventKind::DelegationRedemptionFinalized {
                    request_id: event_request_id,
                    usdc_out,
                    compensation_out,
                    ..
                } if event_request_id == request_id => Some((usdc_out, compensation_out)),
                _ => None,
            })
    }

    pub(super) fn delegation_deposit_response(
        &self,
        delegation_id: DelegationId,
        principal: u64,
        shares_minted: u128,
    ) -> Result<DelegationDepositResponse, RpcError> {
        let delegation = self
            .engine
            .delegation(&delegation_id)
            .cloned()
            .ok_or_else(|| RpcError::Node("delegation_missing_after_block".into()))?;
        let share = self.engine.delegation_share(&delegation_id, principal);
        let delegation_balance = self.delegation_balance(&delegation_id)?;
        let principal_balance = self.policy_usdc_balance_for_account(principal)?;
        Ok(DelegationDepositResponse {
            deposit: DelegationDeposit {
                delegation,
                principal,
                share,
                shares_minted,
                principal_balance,
                delegation_balance,
                trading_account: self
                    .engine
                    .delegation_trading_account(&delegation_id)
                    .ok_or_else(|| RpcError::Node("delegation_trading_account_missing".into()))?,
            },
        })
    }

    pub(super) fn delegation_redemption_request_response(
        &self,
        request: RedemptionRequest,
    ) -> DelegationRedemptionRequestResponse {
        let share = self
            .engine
            .delegation_share(&request.delegation_id, request.principal);
        DelegationRedemptionRequestResponse {
            redemption: DelegationRedemptionRequest { request, share },
        }
    }

    pub(super) fn delegation_redemption_finalization_response(
        &self,
        request: RedemptionRequest,
        usdc_out: u128,
        compensation_out: u128,
    ) -> Result<DelegationRedemptionFinalizationResponse, RpcError> {
        let delegation = self
            .engine
            .delegation(&request.delegation_id)
            .cloned()
            .ok_or_else(|| RpcError::Node("delegation_missing_after_block".into()))?;
        let share = self
            .engine
            .delegation_share(&request.delegation_id, request.principal);
        Ok(DelegationRedemptionFinalizationResponse {
            finalization: DelegationRedemptionFinalization {
                request,
                delegation,
                share,
                usdc_out,
                compensation_out,
                principal_balance: self.policy_usdc_balance_for_account(request.principal)?,
                delegation_balance: self.delegation_balance(&request.delegation_id)?,
            },
        })
    }

    pub(super) fn delegation_redemption_cancellation_response(
        &self,
        request: RedemptionRequest,
    ) -> DelegationRedemptionCancellationResponse {
        let share = self
            .engine
            .delegation_share(&request.delegation_id, request.principal);
        DelegationRedemptionCancellationResponse {
            cancellation: DelegationRedemptionCancellation { request, share },
        }
    }

    pub fn open_delegation(
        &mut self,
        params: OpenDelegationParams,
    ) -> Result<DelegationResponse, RpcError> {
        let mandate_id = decode_hex_array::<32>(&params.mandate_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?;
        let agent_bond = params
            .agent_bond
            .parse::<u128>()
            .map_err(|_| RpcError::InvalidRequest("invalid agent_bond".into()))?;
        let delegation_id = derive_delegation_id(params.agent, &mandate_id, params.timestamp_ms);
        if let Some(delegation) = self.engine.delegation(&delegation_id).cloned() {
            if delegation.agent == params.agent
                && delegation.mandate_id == mandate_id
                && delegation.opened_at_ms == params.timestamp_ms
                && delegation.performance_share_units == params.performance_share_units
                && delegation.crystallization_cadence_ms == params.crystallization_cadence_ms
                && delegation.notice_period_ms == params.notice_period_ms
                && delegation.agent_bond == agent_bond
            {
                return Ok(DelegationResponse { delegation });
            }
        }
        let action = BlockDelegationAction::Open {
            agent: params.agent,
            mandate_id,
            performance_share_units: params.performance_share_units,
            crystallization_cadence_ms: params.crystallization_cadence_ms,
            notice_period_ms: params.notice_period_ms,
            agent_bond,
        };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let delegation_id =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { block_height } => self
                    .delegation_opened_in_block(block_height)
                    .ok_or_else(|| RpcError::Node("delegation_open_event_missing".into()))?,
                DelegationCommitOutcome::Duplicate {
                    receipt: DelegationActionReceipt::Open { delegation_id },
                } => delegation_id,
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        self.get_delegation(&delegation_id)
            .ok_or_else(|| RpcError::Node("delegation_missing_after_block".into()))
    }

    pub fn deposit_to_delegation(
        &mut self,
        params: DepositToDelegationParams,
    ) -> Result<DelegationDepositResponse, RpcError> {
        let delegation_id = decode_hex_array::<32>(&params.delegation_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?;
        let amount = params
            .amount
            .parse::<u128>()
            .map_err(|_| RpcError::InvalidRequest("invalid amount".into()))?;
        if let Some(shares_minted) = self.delegation_deposit_shares_minted(
            delegation_id,
            params.principal,
            amount,
            params.timestamp_ms,
        ) {
            return self.delegation_deposit_response(
                delegation_id,
                params.principal,
                shares_minted,
            );
        }
        let action = BlockDelegationAction::Deposit {
            principal: params.principal,
            delegation_id,
            amount,
        };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let shares_minted =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { block_height } => self
                    .deposit_shares_minted_in_block(block_height, delegation_id, params.principal)
                    .ok_or_else(|| RpcError::Node("delegation_deposit_event_missing".into()))?,
                DelegationCommitOutcome::Duplicate {
                    receipt:
                        DelegationActionReceipt::Deposit {
                            delegation_id: receipt_delegation_id,
                            principal,
                            shares_minted,
                        },
                } if receipt_delegation_id == delegation_id && principal == params.principal => {
                    shares_minted
                }
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        self.delegation_deposit_response(delegation_id, params.principal, shares_minted)
    }

    pub fn submit_deposit_to_delegation(
        &mut self,
        params: SubmitDepositToDelegationParams,
    ) -> Result<DelegationActionSubmissionResponse, RpcError> {
        let delegation_id = decode_hex_array::<32>(&params.delegation_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?;
        let amount = params
            .amount
            .parse::<u128>()
            .map_err(|_| RpcError::InvalidRequest("invalid amount".into()))?;
        let action = BlockDelegationAction::Deposit {
            principal: params.principal,
            delegation_id,
            amount,
        };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let status = self
            .engine
            .submit_delegation_action(action_id, action, params.timestamp_ms)
            .map_err(delegation_rpc_error)?;
        Ok(DelegationActionSubmissionResponse {
            action_id,
            status,
            pending_delegation_actions: self.engine.pending_delegation_action_count(),
        })
    }

    pub fn request_delegation_redemption(
        &mut self,
        params: RequestDelegationRedemptionParams,
    ) -> Result<DelegationRedemptionRequestResponse, RpcError> {
        let delegation_id = decode_hex_array::<32>(&params.delegation_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?;
        let shares = params
            .shares
            .parse::<u128>()
            .map_err(|_| RpcError::InvalidRequest("invalid shares".into()))?;
        let request_id =
            derive_redemption_request_id(params.principal, &delegation_id, params.timestamp_ms);
        if let Some(request) = self.engine.redemption_request(&request_id).copied() {
            if request.principal == params.principal
                && request.delegation_id == delegation_id
                && request.requested_at_ms == params.timestamp_ms
                && request.shares_requested == shares
            {
                return Ok(self.delegation_redemption_request_response(request));
            }
        }
        let action = BlockDelegationAction::RequestRedemption {
            principal: params.principal,
            delegation_id,
            shares,
        };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let request_id =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { block_height } => self
                    .redemption_requested_in_block(block_height)
                    .ok_or_else(|| RpcError::Node("redemption_request_event_missing".into()))?,
                DelegationCommitOutcome::Duplicate {
                    receipt: DelegationActionReceipt::RequestRedemption { request_id },
                } => request_id,
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        let request = *self
            .engine
            .redemption_request(&request_id)
            .ok_or_else(|| RpcError::Node("redemption_request_missing_after_block".into()))?;
        Ok(self.delegation_redemption_request_response(request))
    }

    pub fn finalize_delegation_redemption(
        &mut self,
        params: FinalizeDelegationRedemptionParams,
    ) -> Result<DelegationRedemptionFinalizationResponse, RpcError> {
        let request_id = decode_hex_array::<32>(&params.request_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid request_id_hex".into()))?;
        if let Some(request) = self.engine.redemption_request(&request_id).copied() {
            if request.status == RedemptionRequestStatus::Finalized {
                let (usdc_out, compensation_out) = self
                    .finalized_redemption_outputs_for_request(request_id)
                    .unwrap_or((0, 0));
                return self.delegation_redemption_finalization_response(
                    request,
                    usdc_out,
                    compensation_out,
                );
            }
        }
        let action = BlockDelegationAction::FinalizeRedemption { request_id };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let outcome = self.commit_delegation_action(action_id, action, params.timestamp_ms)?;
        let request = *self
            .engine
            .redemption_request(&request_id)
            .ok_or_else(|| RpcError::Node("redemption_request_missing_after_block".into()))?;
        let (usdc_out, compensation_out) = match outcome {
            DelegationCommitOutcome::Committed { block_height } => self
                .finalized_redemption_outputs_in_block(block_height, request_id)
                .unwrap_or((0, 0)),
            DelegationCommitOutcome::Duplicate {
                receipt:
                    DelegationActionReceipt::FinalizeRedemption {
                        request_id: receipt_request_id,
                        usdc_out,
                        compensation_out,
                    },
            } if receipt_request_id == request_id => (usdc_out, compensation_out),
            DelegationCommitOutcome::Duplicate {
                receipt:
                    DelegationActionReceipt::FinalizeRedemptionReady {
                        request_id: receipt_request_id,
                    },
            } if receipt_request_id == request_id => {
                return Err(RpcError::Node("redemption_pending_use_fresh_nonce".into()));
            }
            DelegationCommitOutcome::Duplicate { .. } => {
                return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
            }
        };
        self.delegation_redemption_finalization_response(request, usdc_out, compensation_out)
    }

    pub fn cancel_delegation_redemption(
        &mut self,
        params: CancelDelegationRedemptionParams,
    ) -> Result<DelegationRedemptionCancellationResponse, RpcError> {
        let request_id = decode_hex_array::<32>(&params.request_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid request_id_hex".into()))?;
        if let Some(request) = self.engine.redemption_request(&request_id).copied() {
            if request.status == RedemptionRequestStatus::Cancelled
                && request.principal == params.principal
            {
                return Ok(self.delegation_redemption_cancellation_response(request));
            }
        }
        let action = BlockDelegationAction::CancelRedemption {
            principal: params.principal,
            request_id,
        };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let request_id =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { .. } => request_id,
                DelegationCommitOutcome::Duplicate {
                    receipt: DelegationActionReceipt::CancelRedemption { request_id },
                } => request_id,
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        let request = *self
            .engine
            .redemption_request(&request_id)
            .ok_or_else(|| RpcError::Node("redemption_request_missing_after_block".into()))?;
        Ok(self.delegation_redemption_cancellation_response(request))
    }

    pub fn close_delegation_to_deposits(
        &mut self,
        params: DelegationLifecycleParams,
    ) -> Result<DelegationResponse, RpcError> {
        let delegation_id = decode_hex_array::<32>(&params.delegation_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?;
        if let Some(delegation) = self.engine.delegation(&delegation_id).cloned() {
            if delegation.status == DelegationStatus::ClosedToDeposits {
                return Ok(DelegationResponse { delegation });
            }
        }
        let action = BlockDelegationAction::CloseToDeposits { delegation_id };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let delegation_id =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { .. } => delegation_id,
                DelegationCommitOutcome::Duplicate {
                    receipt: DelegationActionReceipt::CloseToDeposits { delegation_id },
                } => delegation_id,
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        self.get_delegation(&delegation_id)
            .ok_or_else(|| RpcError::Node("delegation_missing_after_block".into()))
    }

    pub fn begin_delegation_winding(
        &mut self,
        params: DelegationLifecycleParams,
    ) -> Result<DelegationResponse, RpcError> {
        let delegation_id = decode_hex_array::<32>(&params.delegation_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?;
        if let Some(delegation) = self.engine.delegation(&delegation_id).cloned() {
            if delegation.status == DelegationStatus::Winding {
                return Ok(DelegationResponse { delegation });
            }
        }
        let action = BlockDelegationAction::BeginWinding { delegation_id };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let delegation_id =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { .. } => delegation_id,
                DelegationCommitOutcome::Duplicate {
                    receipt: DelegationActionReceipt::BeginWinding { delegation_id },
                } => delegation_id,
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        self.get_delegation(&delegation_id)
            .ok_or_else(|| RpcError::Node("delegation_missing_after_block".into()))
    }

    pub fn close_delegation(
        &mut self,
        params: DelegationLifecycleParams,
    ) -> Result<DelegationResponse, RpcError> {
        let delegation_id = decode_hex_array::<32>(&params.delegation_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid delegation_id_hex".into()))?;
        if let Some(delegation) = self.engine.delegation(&delegation_id).cloned() {
            if delegation.status == DelegationStatus::Closed {
                return Ok(DelegationResponse { delegation });
            }
        }
        let action = BlockDelegationAction::Close { delegation_id };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let delegation_id =
            match self.commit_delegation_action(action_id, action, params.timestamp_ms)? {
                DelegationCommitOutcome::Committed { .. } => delegation_id,
                DelegationCommitOutcome::Duplicate {
                    receipt: DelegationActionReceipt::Close { delegation_id },
                } => delegation_id,
                DelegationCommitOutcome::Duplicate { .. } => {
                    return Err(RpcError::Node("delegation_action_receipt_mismatch".into()));
                }
            };
        self.get_delegation(&delegation_id)
            .ok_or_else(|| RpcError::Node("delegation_missing_after_block".into()))
    }
}
