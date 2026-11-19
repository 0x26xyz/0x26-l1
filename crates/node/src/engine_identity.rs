use super::*;

impl SingleNodeEngine {
    pub fn set_account_operator(
        &mut self,
        account_id: u64,
        operator_id: u64,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        if account_id == 0 || operator_id == 0 {
            return Err(NodeError::InvalidMandate {
                agent: account_id,
                reason: "invalid_account_operator",
            });
        }
        let previous_operator_id = self.account_operators.insert(account_id, operator_id);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AccountOperatorUpdated {
                account_id,
                previous_operator_id,
                operator_id,
            },
        );
        Ok(())
    }

    pub fn account_operator(&self, account_id: u64) -> Option<u64> {
        self.account_operators.get(&account_id).copied()
    }

    pub fn agent_identity(&self, account: u64) -> Option<&AgentIdentityRecord> {
        self.agent_identities.get(&account)
    }

    pub fn attestation(&self, record_id: &AttestationRecordId) -> Option<&AttestationRecord> {
        self.agent_attestations.get(record_id)
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn test_seed_tee_attestation_with_root(
        &mut self,
        root: VendorRoot,
        record: AttestationRecord,
        timestamp_ms: u64,
    ) {
        // Test fixture only. This bypasses RFC 051 admission and must not be
        // used by production RPC paths or local submission flows.
        let account = record.account;
        self.tee_vendor_roots
            .entry(root.vendor)
            .or_default()
            .push(root);
        self.sync_tee_vendor_roots_execution_mirror();
        self.index_tee_attestation_record(&record);
        self.agent_attestations_by_account
            .entry(record.account)
            .or_default()
            .insert(record.record_id);
        self.agent_attestations.insert(record.record_id, record);
        self.recompute_agent_identity_tier(account, timestamp_ms, self.head().header.height);
    }

    pub fn attestations_for_account(
        &self,
        account: u64,
    ) -> impl Iterator<Item = &AttestationRecord> {
        self.agent_attestations_by_account
            .get(&account)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.agent_attestations.get(id))
    }

    pub fn active_attestation_count_for_account(&self, account: u64, timestamp_ms: u64) -> usize {
        self.attestations_for_account(account)
            .filter(|record| attestation_is_active(record, timestamp_ms))
            .count()
    }

    pub fn vendor_roots(
        &self,
        vendor: Option<TeeVendor>,
        include_inactive: bool,
    ) -> Vec<VendorRoot> {
        let head_height = self.head().header.height;
        self.tee_vendor_roots
            .iter()
            .filter(|(entry_vendor, _)| vendor.is_none_or(|vendor| vendor == **entry_vendor))
            .flat_map(|(_, roots)| roots.iter())
            .filter(|root| {
                include_inactive
                    || (root.valid_from_block <= head_height
                        && root
                            .valid_until_block
                            .is_none_or(|valid_until| head_height < valid_until))
            })
            .cloned()
            .collect()
    }

    pub fn pending_vendor_cap_profile_updates(
        &self,
        vendor: Option<TeeVendor>,
    ) -> Vec<ScheduledTeeVendorCapProfileUpdate> {
        let mut updates = self
            .scheduled_tee_vendor_cap_profile_updates
            .values()
            .filter(|update| vendor.is_none_or(|vendor| update.vendor == vendor))
            .cloned()
            .collect::<Vec<_>>();
        updates.sort_by_key(|update| {
            (
                update.valid_from_block,
                update.vendor,
                update.governance_action,
            )
        });
        updates
    }

    pub fn vendor_cap_profile(&self, vendor: TeeVendor) -> Option<&VendorCapProfile> {
        self.tee_vendor_cap_profiles.get(&vendor)
    }

    pub fn tee_attestation_enabled(&self) -> bool {
        // RFC 051 quote verification and TEE identity actions are not live
        // until the verifier slice lands, even though the registries are now
        // consensus state.
        false
    }

    pub fn has_matching_operator_attestation(
        &self,
        account: u64,
        mandate_id: Option<&MandateId>,
        timestamp_ms: u64,
    ) -> bool {
        let Some(identity) = self.agent_identities.get(&account) else {
            return false;
        };
        let Some(operator) = identity.operator else {
            return false;
        };
        self.attestations_for_account(account).any(|record| {
            record.kind == AttestationKind::OperatorSelf
                && record.attestor == operator
                && attestation_is_active(record, timestamp_ms)
                && mandate_id.is_none_or(|id| attestation_claim_matches_mandate(&record.claim, id))
        })
    }

    pub fn has_matching_tee_attestation(
        &self,
        account: u64,
        timestamp_ms: u64,
        block_height: u64,
    ) -> bool {
        self.has_matching_tee_attestation_for_claim(account, None, timestamp_ms, block_height)
    }

    pub fn has_matching_tee_attestation_for_mandate(
        &self,
        account: u64,
        mandate_id: &MandateId,
        timestamp_ms: u64,
        block_height: u64,
    ) -> bool {
        self.has_matching_tee_attestation_for_claim(
            account,
            Some(mandate_id),
            timestamp_ms,
            block_height,
        )
    }

    fn has_matching_tee_attestation_for_claim(
        &self,
        account: u64,
        mandate_id: Option<&MandateId>,
        timestamp_ms: u64,
        block_height: u64,
    ) -> bool {
        // TEE validity crosses timestamp-scoped attestation expiry with
        // block-height-scoped vendor-root activation.
        self.attestations_for_account(account).any(|record| {
            record.kind == AttestationKind::TeeEnclave
                && attestation_is_active(record, timestamp_ms)
                && mandate_id.is_none_or(|id| attestation_claim_matches_mandate(&record.claim, id))
                && matches!(
                    record.evidence,
                    AttestationEvidence::TeeQuoteVerified {
                        vendor,
                        vendor_root_id,
                        ..
                    } if self.tee_vendor_root_active(vendor, vendor_root_id, block_height)
                )
        })
    }

    fn tee_vendor_root_active(
        &self,
        vendor: TeeVendor,
        root_id: VendorRootId,
        block_height: u64,
    ) -> bool {
        self.tee_vendor_roots.get(&vendor).is_some_and(|roots| {
            roots.iter().any(|root| {
                root.root_id == root_id
                    && root.valid_from_block <= block_height
                    && root
                        .valid_until_block
                        .is_none_or(|valid_until| block_height < valid_until)
            })
        })
    }

    fn tee_vendor_root_inactive_reason(
        &self,
        vendor: TeeVendor,
        root_id: VendorRootId,
        block_height: u64,
    ) -> &'static str {
        let Some(root) = self
            .tee_vendor_roots
            .get(&vendor)
            .and_then(|roots| roots.iter().find(|root| root.root_id == root_id))
        else {
            return "identity_tee_attestation_unknown_vendor_root";
        };
        if root
            .valid_until_block
            .is_some_and(|valid_until| block_height >= valid_until)
        {
            "identity_tee_attestation_revoked"
        } else {
            "identity_tee_attestation_unknown_vendor_root"
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn index_tee_attestation_record(&mut self, record: &AttestationRecord) {
        if record.kind != AttestationKind::TeeEnclave || record.revoked_at_block.is_some() {
            return;
        }
        let AttestationEvidence::TeeQuoteVerified {
            vendor,
            vendor_root_id,
            ..
        } = record.evidence
        else {
            return;
        };
        self.tee_attestations_by_vendor_root
            .entry((vendor, vendor_root_id))
            .or_default()
            .insert((record.account, record.record_id));
    }

    fn move_tee_attestation_reverse_index(
        &mut self,
        account: u64,
        record_id: AttestationRecordId,
        vendor: TeeVendor,
        old_root_id: VendorRootId,
        new_root_id: VendorRootId,
    ) {
        if old_root_id == new_root_id {
            return;
        }
        let old_key = (vendor, old_root_id);
        let remove_old_key =
            if let Some(entries) = self.tee_attestations_by_vendor_root.get_mut(&old_key) {
                entries.remove(&(account, record_id));
                entries.is_empty()
            } else {
                false
            };
        if remove_old_key {
            self.tee_attestations_by_vendor_root.remove(&old_key);
        }
        self.tee_attestations_by_vendor_root
            .entry((vendor, new_root_id))
            .or_default()
            .insert((account, record_id));
    }

    pub fn register_agent_identity(
        &mut self,
        input: RegisterAgentIdentityInput,
        timestamp_ms: u64,
    ) -> Result<AgentIdentityRecord, NodeError> {
        self.validate_register_agent_identity_input(&input)?;
        let block_height = self.head().header.height;
        let record = AgentIdentityRecord {
            account: input.account,
            tier: AgentIdentityTier::Tier0Unattested,
            operator: input.operator,
            kind: input.kind,
            display_name: input.display_name,
            description: input.description,
            contact_uri: input.contact_uri,
            registered_at_block: block_height,
            registered_at_timestamp_ms: timestamp_ms,
            last_updated_block: block_height,
            last_updated_timestamp_ms: timestamp_ms,
        };
        self.agent_identities.insert(record.account, record.clone());
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::IdentityRegistered {
                account: record.account,
                tier: identity_tier_status(record.tier),
                display_name: record.display_name.clone(),
                kind: identity_agent_kind(record.kind),
                operator: record.operator,
            },
        );
        Ok(record)
    }

    fn validate_register_agent_identity_input(
        &self,
        input: &RegisterAgentIdentityInput,
    ) -> Result<(), NodeError> {
        let reject = |reason| NodeError::InvalidIdentity {
            account: input.account,
            reason,
        };
        if input.account == 0 {
            return Err(reject("invalid_account"));
        }
        if self.account_store.get(input.account).is_none() {
            return Err(reject("unknown_account"));
        }
        if self.agent_identities.contains_key(&input.account)
            || self.agent_identity_tombstones.contains(&input.account)
        {
            return Err(reject("identity_already_registered"));
        }
        if input.display_name.len() > MAX_AGENT_IDENTITY_DISPLAY_NAME_BYTES {
            return Err(reject("display_name_too_long"));
        }
        if input.description.len() > MAX_AGENT_IDENTITY_DESCRIPTION_BYTES {
            return Err(reject("description_too_long"));
        }
        if input
            .contact_uri
            .as_ref()
            .is_some_and(|value| value.len() > MAX_AGENT_IDENTITY_CONTACT_URI_BYTES)
        {
            return Err(reject("contact_uri_too_long"));
        }
        if input.operator == Some(0) {
            return Err(reject("invalid_operator"));
        }
        if input
            .operator
            .is_some_and(|operator| self.account_store.get(operator).is_none())
        {
            return Err(reject("unknown_operator"));
        }
        Ok(())
    }

    pub fn update_agent_identity(
        &mut self,
        input: UpdateAgentIdentityInput,
        timestamp_ms: u64,
    ) -> Result<AgentIdentityRecord, NodeError> {
        self.validate_update_agent_identity_input(&input)?;
        let block_height = self.head().header.height;
        let tier_before = self
            .agent_identities
            .get(&input.account)
            .map(|identity| identity.tier)
            .unwrap_or(AgentIdentityTier::Tier0Unattested);
        let (field, before, after, operator_changed, updated) = {
            let record = self.agent_identities.get_mut(&input.account).ok_or(
                NodeError::InvalidIdentity {
                    account: input.account,
                    reason: "unknown_identity",
                },
            )?;
            let (field, before, after, operator_changed) = match input.update {
                DescriptiveUpdate::DisplayName(value) => {
                    let before = Some(record.display_name.clone());
                    record.display_name = value.clone();
                    ("display_name", before, Some(value), false)
                }
                DescriptiveUpdate::Description(value) => {
                    let before = Some(record.description.clone());
                    record.description = value.clone();
                    ("description", before, Some(value), false)
                }
                DescriptiveUpdate::Kind(value) => {
                    let before = Some(agent_kind_event_value(record.kind));
                    record.kind = value;
                    ("kind", before, Some(agent_kind_event_value(value)), false)
                }
                DescriptiveUpdate::Operator(value) => {
                    let before = account_id_event_value(record.operator);
                    record.operator = value;
                    ("operator", before, account_id_event_value(value), true)
                }
                DescriptiveUpdate::ContactUri(value) => {
                    let before = record.contact_uri.clone();
                    record.contact_uri = value.clone();
                    ("contact_uri", before, value, false)
                }
            };
            record.last_updated_block = block_height;
            record.last_updated_timestamp_ms = timestamp_ms;
            (
                field.to_string(),
                before,
                after,
                operator_changed,
                record.clone(),
            )
        };
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::IdentityUpdated {
                account: updated.account,
                field,
                before,
                after,
            },
        );
        if operator_changed {
            // Old-operator attestations remain in the audit trail but become
            // dormant while the declared operator differs. Re-pointing back to
            // that operator before expiry can satisfy Tier 1 again.
            let tier_after =
                self.recompute_agent_identity_tier(updated.account, timestamp_ms, block_height);
            if tier_before != tier_after {
                self.record_runtime_event(
                    None,
                    timestamp_ms,
                    RuntimeEventKind::TierChanged {
                        account: updated.account,
                        from: identity_tier_status(tier_before),
                        to: identity_tier_status(tier_after),
                        reason: identity_tier_transition_reason(
                            TierTransitionReason::OperatorChanged,
                        ),
                    },
                );
                if agent_identity_tier_downgraded(tier_before, tier_after) {
                    self.apply_real_capital_tier_loss_sweep(
                        updated.account,
                        tier_after,
                        block_height,
                        timestamp_ms,
                        None,
                    );
                }
            }
            return self.agent_identities.get(&updated.account).cloned().ok_or(
                NodeError::InvalidIdentity {
                    account: updated.account,
                    reason: "unknown_identity",
                },
            );
        }
        Ok(updated)
    }

    fn validate_update_agent_identity_input(
        &self,
        input: &UpdateAgentIdentityInput,
    ) -> Result<(), NodeError> {
        let reject = |reason| NodeError::InvalidIdentity {
            account: input.account,
            reason,
        };
        if input.account == 0 {
            return Err(reject("invalid_account"));
        }
        if self.account_store.get(input.account).is_none() {
            return Err(reject("unknown_account"));
        }
        let record = self
            .agent_identities
            .get(&input.account)
            .ok_or(reject("unknown_identity"))?;
        match &input.update {
            DescriptiveUpdate::DisplayName(value) => {
                if value.len() > MAX_AGENT_IDENTITY_DISPLAY_NAME_BYTES {
                    return Err(reject("display_name_too_long"));
                }
                if value == &record.display_name {
                    return Err(reject("identity_update_noop"));
                }
            }
            DescriptiveUpdate::Description(value) => {
                if value.len() > MAX_AGENT_IDENTITY_DESCRIPTION_BYTES {
                    return Err(reject("description_too_long"));
                }
                if value == &record.description {
                    return Err(reject("identity_update_noop"));
                }
            }
            DescriptiveUpdate::Kind(value) => {
                if value == &record.kind {
                    return Err(reject("identity_update_noop"));
                }
            }
            DescriptiveUpdate::Operator(value) => {
                if *value == Some(0) {
                    return Err(reject("invalid_operator"));
                }
                if value.is_some_and(|operator| self.account_store.get(operator).is_none()) {
                    return Err(reject("unknown_operator"));
                }
                if value == &record.operator {
                    return Err(reject("identity_update_noop"));
                }
            }
            DescriptiveUpdate::ContactUri(value) => {
                if value
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_AGENT_IDENTITY_CONTACT_URI_BYTES)
                {
                    return Err(reject("contact_uri_too_long"));
                }
                if value == &record.contact_uri {
                    return Err(reject("identity_update_noop"));
                }
            }
        }
        Ok(())
    }

    pub fn delete_agent_identity(
        &mut self,
        input: DeleteAgentIdentityInput,
        timestamp_ms: u64,
    ) -> Result<AgentIdentityRecord, NodeError> {
        self.validate_delete_agent_identity_input(&input)?;
        let block_height = self.head().header.height;
        let removed =
            self.agent_identities
                .remove(&input.account)
                .ok_or(NodeError::InvalidIdentity {
                    account: input.account,
                    reason: "unknown_identity",
                })?;
        self.agent_identity_tombstones.insert(input.account);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::IdentityDeleted {
                account: input.account,
            },
        );
        if agent_identity_tier_downgraded(removed.tier, AgentIdentityTier::Tier0Unattested) {
            self.apply_real_capital_tier_loss_sweep(
                input.account,
                AgentIdentityTier::Tier0Unattested,
                block_height,
                timestamp_ms,
                None,
            );
        }
        Ok(removed)
    }

    fn validate_delete_agent_identity_input(
        &self,
        input: &DeleteAgentIdentityInput,
    ) -> Result<(), NodeError> {
        let reject = |reason| NodeError::InvalidIdentity {
            account: input.account,
            reason,
        };
        if input.account == 0 {
            return Err(reject("invalid_account"));
        }
        if self.account_store.get(input.account).is_none() {
            return Err(reject("unknown_account"));
        }
        if !self.agent_identities.contains_key(&input.account) {
            return Err(reject("unknown_identity"));
        }
        Ok(())
    }

    pub fn register_operator_attestation(
        &mut self,
        input: RegisterOperatorAttestationInput,
        signature: [u8; 64],
        timestamp_ms: u64,
    ) -> Result<AttestationRecord, NodeError> {
        self.validate_register_operator_attestation_input(&input, timestamp_ms)?;
        let block_height = self.head().header.height;
        let tier_before = self
            .agent_identities
            .get(&input.subject)
            .map(|identity| identity.tier)
            .unwrap_or(AgentIdentityTier::Tier0Unattested);
        let record_id =
            derive_attestation_record_id(&input, block_height, timestamp_ms, &signature);
        let record = AttestationRecord {
            record_id,
            account: input.subject,
            kind: AttestationKind::OperatorSelf,
            attestor: input.attestor,
            claim: input.claim,
            evidence: AttestationEvidence::OperatorSignature,
            issued_at_block: block_height,
            issued_at_timestamp_ms: timestamp_ms,
            expires_at_timestamp_ms: input.expires_at_timestamp_ms,
            signature,
            revoked_at_block: None,
            verified: true,
        };
        self.agent_attestations
            .insert(record.record_id, record.clone());
        self.agent_attestations_by_account
            .entry(record.account)
            .or_default()
            .insert(record.record_id);
        let tier_after =
            self.recompute_agent_identity_tier(record.account, timestamp_ms, block_height);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationRegistered {
                account: record.account,
                record_id: record.record_id,
                kind: identity_attestation_kind(record.kind),
                attestor: record.attestor,
                claim: identity_attestation_claim(&record.claim),
                expires_at_timestamp_ms: record.expires_at_timestamp_ms,
                tier_before: identity_tier_status(tier_before),
                tier_after: identity_tier_status(tier_after),
            },
        );
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationVerified {
                account: record.account,
                record_id: record.record_id,
                vendor: IdentityAttestationVendor::OperatorSelf,
                code_hash: attestation_claim_code_hash(&record.claim),
                enclave_id: [0_u8; 32],
            },
        );
        if tier_before != tier_after {
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::TierChanged {
                    account: record.account,
                    from: identity_tier_status(tier_before),
                    to: identity_tier_status(tier_after),
                    reason: identity_tier_transition_reason(
                        TierTransitionReason::AttestationRegistered,
                    ),
                },
            );
        }
        Ok(record)
    }

    fn validate_register_operator_attestation_input(
        &self,
        input: &RegisterOperatorAttestationInput,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let reject = |reason| NodeError::InvalidIdentityAction {
            account: input.attestor,
            reason,
        };
        if input.attestor == 0 || input.subject == 0 {
            return Err(reject("invalid_account"));
        }
        if self.account_store.get(input.attestor).is_none() {
            return Err(reject("unknown_attestor"));
        }
        if self.account_store.get(input.subject).is_none() {
            return Err(reject("unknown_subject"));
        }
        let subject_identity =
            self.agent_identities
                .get(&input.subject)
                .ok_or(NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason: "unknown_subject_identity",
                })?;
        if subject_identity.operator != Some(input.attestor) {
            return Err(reject("identity_attestor_mismatch"));
        }
        let min_expires_at = timestamp_ms.saturating_add(OPERATOR_ATTESTATION_MIN_EXPIRY_MS);
        let max_expires_at = timestamp_ms.saturating_add(OPERATOR_ATTESTATION_MAX_EXPIRY_MS);
        if input.expires_at_timestamp_ms < min_expires_at {
            return Err(reject("attestation_expiry_too_soon"));
        }
        if input.expires_at_timestamp_ms > max_expires_at {
            return Err(reject("attestation_expiry_too_late"));
        }
        Ok(())
    }

    pub fn refresh_operator_attestation(
        &mut self,
        input: RefreshOperatorAttestationInput,
        timestamp_ms: u64,
    ) -> Result<AttestationRecord, NodeError> {
        self.validate_refresh_operator_attestation_input(&input, timestamp_ms)?;
        let updated = {
            let record = self.agent_attestations.get_mut(&input.record_id).ok_or(
                NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason: "unknown_attestation",
                },
            )?;
            record.expires_at_timestamp_ms = input.new_expires_at_timestamp_ms;
            record.clone()
        };
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationRefreshed {
                account: updated.account,
                record_id: updated.record_id,
                new_expires_at_timestamp_ms: updated.expires_at_timestamp_ms,
            },
        );
        Ok(updated)
    }

    fn validate_refresh_operator_attestation_input(
        &self,
        input: &RefreshOperatorAttestationInput,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let reject = |reason| NodeError::InvalidIdentityAction {
            account: input.attestor,
            reason,
        };
        if input.attestor == 0 {
            return Err(reject("invalid_account"));
        }
        if self.account_store.get(input.attestor).is_none() {
            return Err(reject("unknown_attestor"));
        }
        let record = self.agent_attestations.get(&input.record_id).ok_or(
            NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: "unknown_attestation",
            },
        )?;
        // Kind check comes first so callers get the schema-level fix before
        // attestor, verification, or expiry details.
        if record.kind != AttestationKind::OperatorSelf {
            return Err(reject("invalid_attestation_kind"));
        }
        if record.attestor != input.attestor {
            return Err(reject("identity_attestor_mismatch"));
        }
        if !record.verified {
            return Err(reject("attestation_unverified"));
        }
        if record.revoked_at_block.is_some() {
            return Err(reject("attestation_revoked"));
        }
        if record.expires_at_timestamp_ms <= timestamp_ms {
            return Err(reject("attestation_expired"));
        }
        if input.new_expires_at_timestamp_ms <= record.expires_at_timestamp_ms {
            return Err(reject("attestation_expiry_not_extended"));
        }
        let min_expires_at = timestamp_ms.saturating_add(TEE_ATTESTATION_MIN_EXPIRY_MS);
        let max_expires_at = timestamp_ms.saturating_add(TEE_ATTESTATION_MAX_EXPIRY_MS);
        if input.new_expires_at_timestamp_ms < min_expires_at {
            return Err(reject("attestation_expiry_too_soon"));
        }
        if input.new_expires_at_timestamp_ms > max_expires_at {
            return Err(reject("attestation_expiry_too_late"));
        }
        Ok(())
    }

    pub fn revoke_attestation(
        &mut self,
        input: RevokeAttestationInput,
        timestamp_ms: u64,
    ) -> Result<AttestationRecord, NodeError> {
        let reason = self.validate_revoke_attestation_input(&input)?;
        let block_height = self.head().header.height;
        let (account, tier_before) = {
            let record = self.agent_attestations.get(&input.record_id).ok_or(
                NodeError::InvalidIdentityAction {
                    account: input.revoker,
                    reason: "unknown_attestation",
                },
            )?;
            let account = record.account;
            let tier_before = self
                .agent_identities
                .get(&account)
                .map(|identity| identity.tier)
                .unwrap_or(AgentIdentityTier::Tier0Unattested);
            (account, tier_before)
        };
        let updated = {
            let record = self.agent_attestations.get_mut(&input.record_id).ok_or(
                NodeError::InvalidIdentityAction {
                    account: input.revoker,
                    reason: "unknown_attestation",
                },
            )?;
            record.revoked_at_block = Some(block_height);
            record.clone()
        };
        let tier_after = self.recompute_agent_identity_tier(account, timestamp_ms, block_height);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationRevoked {
                account,
                record_id: updated.record_id,
                reason: identity_attestation_revocation_reason(reason),
                tier_before: identity_tier_status(tier_before),
                tier_after: identity_tier_status(tier_after),
            },
        );
        if tier_before != tier_after {
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::TierChanged {
                    account,
                    from: identity_tier_status(tier_before),
                    to: identity_tier_status(tier_after),
                    reason: identity_tier_transition_reason(
                        TierTransitionReason::AttestationRevoked,
                    ),
                },
            );
            if agent_identity_tier_downgraded(tier_before, tier_after) {
                self.apply_real_capital_tier_loss_sweep(
                    account,
                    tier_after,
                    block_height,
                    timestamp_ms,
                    None,
                );
            }
        }
        Ok(updated)
    }

    fn validate_revoke_attestation_input(
        &self,
        input: &RevokeAttestationInput,
    ) -> Result<AttestationRevocationReason, NodeError> {
        let reject = |reason| NodeError::InvalidIdentityAction {
            account: input.revoker,
            reason,
        };
        if input.revoker == 0 {
            return Err(reject("invalid_account"));
        }
        if self.account_store.get(input.revoker).is_none() {
            return Err(reject("unknown_revoker"));
        }
        let record = self.agent_attestations.get(&input.record_id).ok_or(
            NodeError::InvalidIdentityAction {
                account: input.revoker,
                reason: "unknown_attestation",
            },
        )?;
        if record.kind != AttestationKind::OperatorSelf {
            return Err(reject("invalid_attestation_kind"));
        }
        if record.revoked_at_block.is_some() {
            return Err(reject("attestation_revoked"));
        }
        if input.revoker == record.attestor {
            Ok(AttestationRevocationReason::AttestorRevoked)
        } else if input.revoker == record.account {
            Ok(AttestationRevocationReason::SubjectRequested)
        } else {
            Err(reject("invalid_revoker"))
        }
    }

    pub(crate) fn recompute_agent_identity_tier(
        &mut self,
        account: u64,
        timestamp_ms: u64,
        block_height: u64,
    ) -> AgentIdentityTier {
        // Caller is responsible for emitting TierChanged and applying the RFC
        // 049 tier-loss sweep when the tier is downgraded.
        let new_tier = if self.has_matching_tee_attestation(account, timestamp_ms, block_height) {
            AgentIdentityTier::Tier2TeeAttested
        } else if self.has_matching_operator_attestation(account, None, timestamp_ms) {
            AgentIdentityTier::Tier1OperatorAttested
        } else {
            AgentIdentityTier::Tier0Unattested
        };
        if let Some(identity) = self.agent_identities.get_mut(&account) {
            if identity.tier != new_tier {
                identity.tier = new_tier;
                identity.last_updated_block = block_height;
                identity.last_updated_timestamp_ms = timestamp_ms;
            }
        }
        new_tier
    }

    pub(crate) fn expire_due_operator_attestations_at_head(&mut self, previous_timestamp_ms: u64) {
        let block_height = self.head().header.height;
        let timestamp_ms = self.head().header.timestamp_ms;
        let mut expired_by_account = BTreeMap::<u64, Vec<AttestationRecordId>>::new();
        for record in self.agent_attestations.values() {
            if record.kind == AttestationKind::OperatorSelf
                && record.verified
                && record.revoked_at_block.is_none()
                && record.expires_at_timestamp_ms > previous_timestamp_ms
                && record.expires_at_timestamp_ms <= timestamp_ms
            {
                expired_by_account
                    .entry(record.account)
                    .or_default()
                    .push(record.record_id);
            }
        }

        for (account, expired_record_ids) in expired_by_account {
            let tier_before = self
                .agent_identities
                .get(&account)
                .map(|identity| identity.tier)
                .unwrap_or(AgentIdentityTier::Tier0Unattested);
            let tier_after =
                self.recompute_agent_identity_tier(account, timestamp_ms, block_height);
            for record_id in expired_record_ids {
                self.record_runtime_event(
                    Some(block_height),
                    timestamp_ms,
                    RuntimeEventKind::AttestationExpired {
                        account,
                        record_id,
                        tier_before: identity_tier_status(tier_before),
                        tier_after: identity_tier_status(tier_after),
                    },
                );
            }
            if tier_before == tier_after {
                continue;
            }
            self.record_runtime_event(
                Some(block_height),
                timestamp_ms,
                RuntimeEventKind::TierChanged {
                    account,
                    from: identity_tier_status(tier_before),
                    to: identity_tier_status(tier_after),
                    reason: identity_tier_transition_reason(
                        TierTransitionReason::AttestationExpired,
                    ),
                },
            );
            if agent_identity_tier_downgraded(tier_before, tier_after) {
                self.apply_real_capital_tier_loss_sweep(
                    account,
                    tier_after,
                    block_height,
                    timestamp_ms,
                    Some(block_height),
                );
            }
        }
    }

    pub fn identity_action_nonce(&self, account: u64) -> u64 {
        self.identity_action_nonces
            .get(&account)
            .copied()
            .unwrap_or(0)
    }

    pub fn identity_attestor_nonce(&self, account: u64) -> u64 {
        self.identity_attestor_nonces
            .get(&account)
            .copied()
            .unwrap_or(0)
    }

    pub fn submit_identity_action(
        &mut self,
        signed_action: SignedIdentityAction,
        timestamp_ms: u64,
    ) -> Result<AgentIdentityRecord, NodeError> {
        if signed_action.chain_id != self.chain_id() {
            return Err(NodeError::InvalidIdentityAction {
                account: signed_action.account,
                reason: "chain_mismatch",
            });
        }
        if signed_action.action.account() != signed_action.account {
            return Err(NodeError::InvalidIdentityAction {
                account: signed_action.account,
                reason: "account_mismatch",
            });
        }
        let nonce_namespace =
            self.identity_nonce_namespace_for_action(signed_action.account, &signed_action.action)?;
        let expected_nonce =
            self.identity_action_nonce_for_namespace(signed_action.account, nonce_namespace);
        if signed_action.nonce != expected_nonce {
            return Err(NodeError::InvalidIdentityActionNonce {
                account: signed_action.account,
                expected: expected_nonce,
                actual: signed_action.nonce,
            });
        }
        let owner = self
            .account_store
            .get(signed_action.account)
            .map(StandardAgentAccount::owner)
            .ok_or(NodeError::InvalidIdentityAction {
                account: signed_action.account,
                reason: "unknown_account",
            })?;
        let verifying_key = VerifyingKey::from_bytes(&owner).map_err(|_| {
            NodeError::InvalidIdentityActionSignature {
                account: signed_action.account,
            }
        })?;
        let signature = Ed25519Signature::from_bytes(&signed_action.signature);
        let signing_bytes = identity_action_signing_bytes(
            signed_action.chain_id,
            signed_action.account,
            signed_action.nonce,
            &signed_action.action,
        );
        verifying_key
            .verify_strict(&signing_bytes, &signature)
            .map_err(|_| NodeError::InvalidIdentityActionSignature {
                account: signed_action.account,
            })?;

        self.submit_verified_identity_action(
            signed_action.chain_id,
            signed_action.account,
            signed_action.nonce,
            signed_action.action,
            signed_action.signature,
            timestamp_ms,
        )
    }

    fn submit_verified_identity_action(
        &mut self,
        chain_id: u64,
        account: u64,
        nonce: u64,
        action: IdentityAction,
        signature: [u8; 64],
        timestamp_ms: u64,
    ) -> Result<AgentIdentityRecord, NodeError> {
        // Re-check state-derived guards so future consensus block replay can
        // call this path after signature verification has already happened.
        if chain_id != self.chain_id() {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "chain_mismatch",
            });
        }
        if action.account() != account {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "account_mismatch",
            });
        }
        let nonce_namespace = self.identity_nonce_namespace_for_action(account, &action)?;
        let expected_nonce = self.identity_action_nonce_for_namespace(account, nonce_namespace);
        if nonce != expected_nonce {
            return Err(NodeError::InvalidIdentityActionNonce {
                account,
                expected: expected_nonce,
                actual: nonce,
            });
        }

        let record = match action {
            IdentityAction::Register(input) => {
                // TODO(RFC 050 fees): charge the identity registration fee once fee sinks land.
                self.register_agent_identity(input, timestamp_ms)
            }
            IdentityAction::UpdateDescriptive(input) => {
                // TODO(RFC 050 fees): charge the operator-change fee once fee sinks land.
                self.update_agent_identity(input, timestamp_ms)
            }
            IdentityAction::RegisterOperatorAttestation(input) => {
                let subject = input.subject;
                self.register_operator_attestation(input, signature, timestamp_ms)?;
                self.agent_identities.get(&subject).cloned().ok_or(
                    NodeError::InvalidIdentityAction {
                        account,
                        reason: "unknown_subject_identity",
                    },
                )
            }
            IdentityAction::RefreshOperatorAttestation(input) => {
                let record = self.refresh_operator_attestation(input, timestamp_ms)?;
                self.agent_identities.get(&record.account).cloned().ok_or(
                    NodeError::InvalidIdentityAction {
                        account,
                        reason: "unknown_subject_identity",
                    },
                )
            }
            IdentityAction::RevokeAttestation(input) => {
                let record = self.revoke_attestation(input, timestamp_ms)?;
                self.agent_identities.get(&record.account).cloned().ok_or(
                    NodeError::InvalidIdentityAction {
                        account,
                        reason: "unknown_subject_identity",
                    },
                )
            }
            IdentityAction::Delete(input) => {
                // TODO(RFC 050 fees): refund the identity deletion fee once fee sinks land.
                self.delete_agent_identity(input, timestamp_ms)
            }
        }?;
        self.increment_identity_nonce_for_namespace(account, expected_nonce, nonce_namespace);
        Ok(record)
    }

    pub fn submit_tee_attestation_action(
        &mut self,
        signed_action: SignedTeeAttestationAction,
        timestamp_ms: u64,
    ) -> Result<AgentIdentityRecord, NodeError> {
        let attestor = signed_action.action.attestor();
        if signed_action.chain_id != self.chain_id() {
            return Err(NodeError::InvalidIdentityAction {
                account: attestor,
                reason: "chain_mismatch",
            });
        }
        let (subject, vendor) =
            self.validate_tee_attestation_action_input(&signed_action.action, timestamp_ms)?;
        let (attestor_nonce, subject_nonce, attestor_signature, subject_signature) =
            match (&signed_action.signers, attestor == subject) {
                (
                    TeeAttestationSigners::SelfAttest {
                        attestor_nonce,
                        attestor_signature,
                    },
                    true,
                ) => (*attestor_nonce, None, attestor_signature, None),
                (
                    TeeAttestationSigners::OperatorAttest {
                        attestor_nonce,
                        attestor_signature,
                        subject_nonce,
                        subject_signature,
                    },
                    false,
                ) => (
                    *attestor_nonce,
                    Some(*subject_nonce),
                    attestor_signature,
                    Some(subject_signature),
                ),
                _ => {
                    return Err(NodeError::InvalidIdentityAction {
                        account: attestor,
                        reason: "invalid_signer_shape",
                    })
                }
            };
        let expected_attestor_nonce = self.identity_attestor_nonce(attestor);
        if attestor_nonce != expected_attestor_nonce {
            return Err(NodeError::InvalidIdentityActionNonce {
                account: attestor,
                expected: expected_attestor_nonce,
                actual: attestor_nonce,
            });
        }
        if let Some(actual_subject_nonce) = subject_nonce {
            let expected_subject_nonce = self.identity_action_nonce(subject);
            if actual_subject_nonce != expected_subject_nonce {
                return Err(NodeError::InvalidIdentityActionNonce {
                    account: subject,
                    expected: expected_subject_nonce,
                    actual: actual_subject_nonce,
                });
            }
        }
        let signing_bytes = tee_attestation_signing_bytes(
            signed_action.chain_id,
            &signed_action.action,
            attestor_nonce,
            subject_nonce,
        );
        self.verify_identity_action_signature(attestor, &signing_bytes, attestor_signature)?;
        if let Some(signature) = subject_signature {
            self.verify_identity_action_signature(subject, &signing_bytes, signature)?;
        }
        if !self.tee_attestation_enabled() {
            return Err(NodeError::InvalidIdentityAction {
                account: attestor,
                reason: "identity_tee_attestation_not_enabled",
            });
        }
        let verifier_outcome =
            self.verify_tee_attestation_action_quote(&signed_action.action, vendor)?;
        if !verifier_outcome.verified {
            return Err(NodeError::InvalidIdentityAction {
                account: attestor,
                reason: verifier_outcome
                    .failure_reason
                    .identity_action_rejection_reason()
                    .expect("failed verifier outcomes have rejection reasons"),
            });
        }
        let record = match &signed_action.action {
            TeeAttestationAction::Register(input) => self.materialize_registered_tee_attestation(
                input,
                verifier_outcome,
                attestor_signature,
                subject_signature,
                timestamp_ms,
            )?,
            TeeAttestationAction::Refresh(input) => {
                self.materialize_refreshed_tee_attestation(input, verifier_outcome, timestamp_ms)?
            }
        };
        self.increment_identity_nonce_for_namespace(
            attestor,
            expected_attestor_nonce,
            IdentityNonceNamespace::Attestor,
        );
        if let Some(expected_subject_nonce) = subject_nonce {
            self.increment_identity_nonce_for_namespace(
                subject,
                expected_subject_nonce,
                IdentityNonceNamespace::AccountSelf,
            );
        }
        self.agent_identities.get(&record.account).cloned().ok_or(
            NodeError::InvalidIdentityAction {
                account: attestor,
                reason: "unknown_subject_identity",
            },
        )
    }

    pub(crate) fn verify_tee_attestation_action_quote(
        &self,
        action: &TeeAttestationAction,
        vendor: TeeVendor,
    ) -> Result<AttestationVerifyOutcome, NodeError> {
        let expected_claim = self.tee_attestation_expected_claim_bytes(action)?;
        let head = self.head();
        let context = ExecutionContext {
            chain_id: head.header.chain_id,
            height: head.header.height,
            timestamp_ms: head.header.timestamp_ms,
            parent_state_root: self.executor.committed_root(),
        };
        let mut checkpoint = self.executor.state_store().checkpoint();
        Ok(verify_attestation_quote(
            &context,
            &mut checkpoint,
            AttestationVerifyRequest {
                vendor: execution_tee_vendor(vendor),
                quote_bytes: action.quote_bytes(),
                nonce: action.nonce(),
                freshness_anchor_block: action.freshness_anchor_block(),
                expected_claim: &expected_claim,
            },
        ))
    }

    fn tee_attestation_expected_claim_bytes(
        &self,
        action: &TeeAttestationAction,
    ) -> Result<Vec<u8>, NodeError> {
        let claim = match action {
            TeeAttestationAction::Register(input) => &input.claim,
            TeeAttestationAction::Refresh(input) => {
                &self
                    .agent_attestations
                    .get(&input.record_id)
                    .ok_or(NodeError::InvalidIdentityAction {
                        account: input.attestor,
                        reason: "unknown_attestation",
                    })?
                    .claim
            }
        };
        Ok(borsh::to_vec(claim).expect("attestation claim should serialize"))
    }

    pub(crate) fn materialize_registered_tee_attestation(
        &mut self,
        input: &RegisterTeeAttestationInput,
        verifier_outcome: AttestationVerifyOutcome,
        attestor_signature: &[u8; 64],
        subject_signature: Option<&[u8; 64]>,
        timestamp_ms: u64,
    ) -> Result<AttestationRecord, NodeError> {
        if !verifier_outcome.verified {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: verifier_outcome
                    .failure_reason
                    .identity_action_rejection_reason()
                    .expect("failed verifier outcomes have rejection reasons"),
            });
        }
        let block_height = self.head().header.height;
        if !self.tee_vendor_root_active(input.vendor, verifier_outcome.vendor_root_id, block_height)
        {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: self.tee_vendor_root_inactive_reason(
                    input.vendor,
                    verifier_outcome.vendor_root_id,
                    block_height,
                ),
            });
        }
        if !tee_verifier_outcome_matches_claim(&input.claim, &verifier_outcome) {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: "identity_tee_attestation_claim_mismatch",
            });
        }
        let tier_before = self
            .agent_identities
            .get(&input.subject)
            .map(|identity| identity.tier)
            .unwrap_or(AgentIdentityTier::Tier0Unattested);
        let record_id = derive_tee_attestation_record_id(
            input,
            block_height,
            timestamp_ms,
            attestor_signature,
            subject_signature,
        );
        let record = AttestationRecord {
            record_id,
            account: input.subject,
            kind: AttestationKind::TeeEnclave,
            attestor: input.attestor,
            claim: input.claim.clone(),
            evidence: AttestationEvidence::TeeQuoteVerified {
                vendor: input.vendor,
                vendor_root_id: verifier_outcome.vendor_root_id,
                enclave_id: verifier_outcome.enclave_id,
                signer: verifier_outcome.signer,
                verified_at_block: block_height,
            },
            issued_at_block: block_height,
            issued_at_timestamp_ms: timestamp_ms,
            expires_at_timestamp_ms: input.expires_at_timestamp_ms,
            signature: *attestor_signature,
            revoked_at_block: None,
            verified: true,
        };
        self.agent_attestations
            .insert(record.record_id, record.clone());
        self.agent_attestations_by_account
            .entry(record.account)
            .or_default()
            .insert(record.record_id);
        self.index_tee_attestation_record(&record);
        let tier_after =
            self.recompute_agent_identity_tier(record.account, timestamp_ms, block_height);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationRegistered {
                account: record.account,
                record_id: record.record_id,
                kind: identity_attestation_kind(record.kind),
                attestor: record.attestor,
                claim: identity_attestation_claim(&record.claim),
                expires_at_timestamp_ms: record.expires_at_timestamp_ms,
                tier_before: identity_tier_status(tier_before),
                tier_after: identity_tier_status(tier_after),
            },
        );
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationVerified {
                account: record.account,
                record_id: record.record_id,
                vendor: identity_attestation_vendor_for_tee(input.vendor),
                code_hash: verifier_outcome.code_hash,
                enclave_id: verifier_outcome.enclave_id,
            },
        );
        if tier_before != tier_after {
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::TierChanged {
                    account: record.account,
                    from: identity_tier_status(tier_before),
                    to: identity_tier_status(tier_after),
                    reason: identity_tier_transition_reason(
                        TierTransitionReason::AttestationRegistered,
                    ),
                },
            );
        }
        Ok(record)
    }

    pub(crate) fn materialize_refreshed_tee_attestation(
        &mut self,
        input: &RefreshTeeAttestationInput,
        verifier_outcome: AttestationVerifyOutcome,
        timestamp_ms: u64,
    ) -> Result<AttestationRecord, NodeError> {
        if !verifier_outcome.verified {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: verifier_outcome
                    .failure_reason
                    .identity_action_rejection_reason()
                    .expect("failed verifier outcomes have rejection reasons"),
            });
        }
        let block_height = self.head().header.height;
        let (account, vendor, old_vendor_root_id, old_enclave_id, old_signer, claim) = {
            let record = self.agent_attestations.get(&input.record_id).ok_or(
                NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason: "unknown_attestation",
                },
            )?;
            let AttestationEvidence::TeeQuoteVerified {
                vendor,
                vendor_root_id,
                enclave_id,
                signer,
                ..
            } = record.evidence
            else {
                return Err(NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason: "invalid_attestation_evidence",
                });
            };
            (
                record.account,
                vendor,
                vendor_root_id,
                enclave_id,
                signer,
                record.claim.clone(),
            )
        };
        if !self.tee_vendor_root_active(vendor, verifier_outcome.vendor_root_id, block_height) {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: self.tee_vendor_root_inactive_reason(
                    vendor,
                    verifier_outcome.vendor_root_id,
                    block_height,
                ),
            });
        }
        if !tee_verifier_outcome_matches_claim(&claim, &verifier_outcome) {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: "identity_tee_attestation_claim_mismatch",
            });
        }
        if verifier_outcome.vendor_root_id == old_vendor_root_id
            && (verifier_outcome.enclave_id != old_enclave_id
                || verifier_outcome.signer != old_signer)
        {
            return Err(NodeError::InvalidIdentityAction {
                account: input.attestor,
                reason: "identity_tee_evidence_changed_under_same_root",
            });
        }
        if verifier_outcome.vendor_root_id != old_vendor_root_id {
            self.move_tee_attestation_reverse_index(
                account,
                input.record_id,
                vendor,
                old_vendor_root_id,
                verifier_outcome.vendor_root_id,
            );
        }
        let updated = {
            let record = self.agent_attestations.get_mut(&input.record_id).ok_or(
                NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason: "unknown_attestation",
                },
            )?;
            record.expires_at_timestamp_ms = input.new_expires_at_timestamp_ms;
            record.evidence = AttestationEvidence::TeeQuoteVerified {
                vendor,
                vendor_root_id: verifier_outcome.vendor_root_id,
                enclave_id: verifier_outcome.enclave_id,
                signer: verifier_outcome.signer,
                verified_at_block: block_height,
            };
            record.clone()
        };
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::AttestationRefreshed {
                account: updated.account,
                record_id: updated.record_id,
                new_expires_at_timestamp_ms: updated.expires_at_timestamp_ms,
            },
        );
        Ok(updated)
    }

    pub(crate) fn validate_tee_attestation_action_input(
        &self,
        action: &TeeAttestationAction,
        timestamp_ms: u64,
    ) -> Result<(u64, TeeVendor), NodeError> {
        match action {
            TeeAttestationAction::Register(input) => {
                let reject = |reason| NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason,
                };
                if input.attestor == 0 || input.subject == 0 {
                    return Err(reject("invalid_account"));
                }
                if self.account_store.get(input.attestor).is_none() {
                    return Err(reject("unknown_attestor"));
                }
                if self.account_store.get(input.subject).is_none() {
                    return Err(reject("unknown_subject"));
                }
                let subject_identity = self.agent_identities.get(&input.subject).ok_or(
                    NodeError::InvalidIdentityAction {
                        account: input.attestor,
                        reason: "unknown_subject_identity",
                    },
                )?;
                if input.attestor != input.subject
                    && subject_identity.operator != Some(input.attestor)
                {
                    return Err(reject("identity_attestor_mismatch"));
                }
                self.validate_tee_attestation_expiry(
                    input.attestor,
                    input.expires_at_timestamp_ms,
                    timestamp_ms,
                )?;
                self.validate_tee_attestation_quote_size(
                    input.attestor,
                    input.vendor,
                    &input.quote_bytes,
                )?;
                self.validate_tee_attestation_freshness(
                    input.attestor,
                    input.nonce,
                    input.freshness_anchor_block,
                )?;
                Ok((input.subject, input.vendor))
            }
            TeeAttestationAction::Refresh(input) => {
                let reject = |reason| NodeError::InvalidIdentityAction {
                    account: input.attestor,
                    reason,
                };
                if input.attestor == 0 {
                    return Err(reject("invalid_account"));
                }
                if self.account_store.get(input.attestor).is_none() {
                    return Err(reject("unknown_attestor"));
                }
                let record = self.agent_attestations.get(&input.record_id).ok_or(
                    NodeError::InvalidIdentityAction {
                        account: input.attestor,
                        reason: "unknown_attestation",
                    },
                )?;
                // Validation order is intentional and mirrored by branch
                // coverage tests: kind -> subject identity -> attestor ->
                // verified -> revoked -> expired -> extension -> evidence ->
                // expiry bounds.
                if record.kind != AttestationKind::TeeEnclave {
                    return Err(reject("invalid_attestation_kind"));
                }
                if !self.agent_identities.contains_key(&record.account) {
                    return Err(reject("unknown_subject_identity"));
                }
                if record.attestor != input.attestor {
                    return Err(reject("identity_attestor_mismatch"));
                }
                if !record.verified {
                    return Err(reject("attestation_unverified"));
                }
                if record.revoked_at_block.is_some() {
                    return Err(reject("attestation_revoked"));
                }
                if record.expires_at_timestamp_ms <= timestamp_ms {
                    return Err(reject("attestation_expired"));
                }
                if input.new_expires_at_timestamp_ms <= record.expires_at_timestamp_ms {
                    return Err(reject("attestation_expiry_not_extended"));
                }
                let vendor = match record.evidence {
                    AttestationEvidence::TeeQuoteVerified { vendor, .. } => vendor,
                    AttestationEvidence::OperatorSignature => {
                        return Err(reject("invalid_attestation_evidence"))
                    }
                };
                self.validate_tee_attestation_expiry(
                    input.attestor,
                    input.new_expires_at_timestamp_ms,
                    timestamp_ms,
                )?;
                self.validate_tee_attestation_quote_size(
                    input.attestor,
                    vendor,
                    &input.quote_bytes,
                )?;
                self.validate_tee_attestation_freshness(
                    input.attestor,
                    input.nonce,
                    input.freshness_anchor_block,
                )?;
                Ok((record.account, vendor))
            }
        }
    }

    fn validate_tee_attestation_expiry(
        &self,
        account: u64,
        expires_at_timestamp_ms: u64,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let min_expires_at = timestamp_ms.saturating_add(TEE_ATTESTATION_MIN_EXPIRY_MS);
        let max_expires_at = timestamp_ms.saturating_add(TEE_ATTESTATION_MAX_EXPIRY_MS);
        if expires_at_timestamp_ms < min_expires_at {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "attestation_expiry_too_soon",
            });
        }
        if expires_at_timestamp_ms > max_expires_at {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "attestation_expiry_too_late",
            });
        }
        Ok(())
    }

    fn validate_tee_attestation_quote_size(
        &self,
        account: u64,
        vendor: TeeVendor,
        quote_bytes: &[u8],
    ) -> Result<(), NodeError> {
        if quote_bytes.len() > max_tee_quote_bytes(vendor) {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "identity_tee_attestation_quote_too_large",
            });
        }
        Ok(())
    }

    pub(crate) fn validate_tee_attestation_freshness(
        &self,
        account: u64,
        nonce: Hash,
        freshness_anchor_block: u64,
    ) -> Result<(), NodeError> {
        let current_block = self.head().header.height;
        let min_anchor = current_block.saturating_sub(ATTESTATION_QUOTE_FRESHNESS_BLOCKS);
        if freshness_anchor_block >= current_block || freshness_anchor_block < min_anchor {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "identity_tee_attestation_stale_nonce",
            });
        }
        let Some(anchor_block) = self.chain.get(freshness_anchor_block as usize) else {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "identity_tee_attestation_stale_nonce",
            });
        };
        if block_hash(anchor_block) != nonce {
            return Err(NodeError::InvalidIdentityAction {
                account,
                reason: "identity_tee_attestation_stale_nonce",
            });
        }
        Ok(())
    }

    fn verify_identity_action_signature(
        &self,
        account: u64,
        signing_bytes: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), NodeError> {
        let owner = self
            .account_store
            .get(account)
            .map(StandardAgentAccount::owner)
            .ok_or(NodeError::InvalidIdentityAction {
                account,
                reason: "unknown_account",
            })?;
        let verifying_key = VerifyingKey::from_bytes(&owner)
            .map_err(|_| NodeError::InvalidIdentityActionSignature { account })?;
        let signature = Ed25519Signature::from_bytes(signature);
        verifying_key
            .verify_strict(signing_bytes, &signature)
            .map_err(|_| NodeError::InvalidIdentityActionSignature { account })
    }

    fn identity_nonce_namespace_for_action(
        &self,
        account: u64,
        action: &IdentityAction,
    ) -> Result<IdentityNonceNamespace, NodeError> {
        match action {
            IdentityAction::Register(_)
            | IdentityAction::UpdateDescriptive(_)
            | IdentityAction::Delete(_) => Ok(IdentityNonceNamespace::AccountSelf),
            IdentityAction::RegisterOperatorAttestation(_)
            | IdentityAction::RefreshOperatorAttestation(_) => Ok(IdentityNonceNamespace::Attestor),
            IdentityAction::RevokeAttestation(input) => {
                let record = self.agent_attestations.get(&input.record_id).ok_or(
                    NodeError::InvalidIdentityAction {
                        account,
                        reason: "unknown_attestation",
                    },
                )?;
                if account == record.attestor {
                    Ok(IdentityNonceNamespace::Attestor)
                } else if account == record.account {
                    Ok(IdentityNonceNamespace::AccountSelf)
                } else {
                    Err(NodeError::InvalidIdentityAction {
                        account,
                        reason: "invalid_revoker",
                    })
                }
            }
        }
    }

    fn identity_action_nonce_for_namespace(
        &self,
        account: u64,
        namespace: IdentityNonceNamespace,
    ) -> u64 {
        match namespace {
            IdentityNonceNamespace::AccountSelf => self.identity_action_nonce(account),
            IdentityNonceNamespace::Attestor => self.identity_attestor_nonce(account),
        }
    }

    fn increment_identity_nonce_for_namespace(
        &mut self,
        account: u64,
        expected_nonce: u64,
        namespace: IdentityNonceNamespace,
    ) {
        match namespace {
            IdentityNonceNamespace::AccountSelf => {
                self.identity_action_nonces
                    .insert(account, expected_nonce.saturating_add(1));
            }
            IdentityNonceNamespace::Attestor => {
                self.identity_attestor_nonces
                    .insert(account, expected_nonce.saturating_add(1));
            }
        }
    }
}
