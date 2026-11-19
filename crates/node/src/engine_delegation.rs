use super::*;

impl SingleNodeEngine {
    pub fn mandate(&self, mandate_id: &MandateId) -> Option<&Mandate> {
        self.mandates.get(mandate_id)
    }

    pub fn mandates(&self) -> impl Iterator<Item = &Mandate> {
        self.mandates.values()
    }

    pub fn delegation(&self, delegation_id: &DelegationId) -> Option<&Delegation> {
        self.delegations.get(delegation_id)
    }

    pub fn delegations(&self) -> impl Iterator<Item = &Delegation> {
        self.delegations.values()
    }

    pub fn real_capital_listing(&self, vault_id: &VaultId) -> Option<&RealCapitalListing> {
        self.real_capital_listings.get(vault_id)
    }

    pub fn real_capital_listings(&self) -> impl Iterator<Item = &RealCapitalListing> {
        self.real_capital_listings.values()
    }

    pub fn real_capital_vault_deposited_micro_usdc(&self, vault_id: &VaultId) -> u128 {
        self.vault_deposited_micro_usdc
            .get(vault_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn real_capital_principal_deposited_micro_usdc(
        &self,
        vault_id: &VaultId,
        principal: u64,
    ) -> u128 {
        self.vault_principal_deposited_micro_usdc
            .get(&(*vault_id, principal))
            .copied()
            .unwrap_or(0)
    }

    pub fn delegation_compensation_claim_micro_usdc(
        &self,
        delegation_id: &DelegationId,
        principal: u64,
    ) -> u128 {
        self.delegation_compensation_claims
            .get(&(*delegation_id, principal))
            .copied()
            .unwrap_or(0)
    }

    pub fn real_capital_bond_escrow_micro_usdc(&self, delegation_id: &DelegationId) -> u128 {
        self.real_capital_bond_escrow_micro_usdc
            .get(delegation_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn real_capital_total_bond_escrow_micro_usdc(&self) -> u128 {
        self.real_capital_bond_escrow_micro_usdc
            .values()
            .copied()
            .fold(0_u128, u128::saturating_add)
    }

    pub fn real_capital_compensation_escrow_micro_usdc(&self) -> u128 {
        self.real_capital_compensation_escrow_micro_usdc
    }

    /// RFC 049 insurance wallet ledger mirrored into a deterministic EVM
    /// policy-USDC escrow address.
    pub fn real_capital_insurance_wallet_micro_usdc(&self) -> u128 {
        self.real_capital_insurance_wallet_micro_usdc
    }

    /// RFC 049 gross accumulator for slashed delegation-bond amounts assigned
    /// to insurance. Future spend paths must not debit this append-only
    /// accounting value.
    pub fn real_capital_insurance_fund_micro_usdc(&self) -> u128 {
        self.real_capital_insurance_fund_micro_usdc
    }

    pub(super) fn real_capital_compensation_escrow_policy_balance(
        &self,
    ) -> Result<U256, NodeError> {
        self.policy_usdc_balance_at_address(
            real_capital_compensation_escrow_address(),
            0,
            "real_capital_compensation_escrow_balance_malformed",
        )
    }

    pub(super) fn real_capital_insurance_wallet_policy_balance(&self) -> Result<U256, NodeError> {
        self.policy_usdc_balance_at_address(
            real_capital_insurance_wallet_address(),
            0,
            "real_capital_insurance_wallet_balance_malformed",
        )
    }

    /// Crate-internal debit-only RFC 049 insurance-wallet primitive. This
    /// debits the ledger and matching deterministic EVM escrow address but
    /// does not perform any recipient credit. Payout flows must use a
    /// debit+credit composer in the same deterministic transition. Purely
    /// Rust-side credit destinations are forbidden; callers must pair this
    /// only with an atomic EVM credit or leave the wallet untouched.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spend_real_capital_insurance_wallet_escrow_v1(
        &mut self,
        amount: u128,
        source: RealCapitalInsuranceWalletSpendSource,
        timestamp_ms: u64,
    ) -> Result<u128, NodeError> {
        self.spend_real_capital_insurance_wallet_escrow_v1_at(amount, source, timestamp_ms, None)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn spend_real_capital_insurance_wallet_escrow_v1_at(
        &mut self,
        amount: u128,
        source: RealCapitalInsuranceWalletSpendSource,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<u128, NodeError> {
        let next_balance = self.real_capital_insurance_wallet_balance_after_spend(amount)?;
        let escrow_balance = self.real_capital_insurance_wallet_policy_balance()?;
        if escrow_balance != U256::from(self.real_capital_insurance_wallet_micro_usdc) {
            return Err(NodeError::InvalidDelegation {
                agent: 0,
                reason: "real_capital_insurance_wallet_escrow_mismatch",
            });
        }
        let next_escrow_balance =
            escrow_balance
                .checked_sub(U256::from(amount))
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "real_capital_insurance_wallet_escrow_underflow",
                })?;
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_insurance_wallet_address(),
            next_escrow_balance,
        );
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: 0,
                reason: "real_capital_insurance_wallet_state_update_failed",
            })?;
        self.apply_real_capital_insurance_wallet_spend_effect(
            amount,
            next_balance,
            source,
            timestamp_ms,
            event_block_height,
        );
        Ok(next_balance)
    }

    /// Settles an already-authorized RFC 049 insurance-wallet payout by
    /// debiting the deterministic insurance escrow and crediting the
    /// recipient's policy-USDC balance in one committed state transition. This
    /// lower-level composer does not perform a frozen-recipient gate; keeper
    /// rewards must enter through the keeper-specific wrapper, and other
    /// payout classes must enforce their own eligibility and authority.
    pub fn pay_real_capital_insurance_wallet_escrow_v1(
        &mut self,
        recipient_account: u64,
        amount: u128,
        source: RealCapitalInsuranceWalletSpendSource,
        timestamp_ms: u64,
    ) -> Result<RealCapitalInsuranceWalletPayout, NodeError> {
        let payout = self.pay_real_capital_insurance_wallet_escrow_v1_at(
            recipient_account,
            amount,
            source,
            timestamp_ms,
            None,
        )?;
        self.advance_pacemaker_on_current_head_qc()?;
        Ok(payout)
    }

    /// Deterministic preflight for a keeper reward candidate. This validates
    /// only consensus-local eligibility needed before payout composition; it
    /// does not select a keeper, prove work, or authorize forced liquidation.
    pub fn validate_real_capital_keeper_reward_candidate(
        &self,
        keeper_account: u64,
        amount: u128,
    ) -> Result<RealCapitalKeeperRewardCandidate, NodeError> {
        let wallet_balance_after =
            self.real_capital_insurance_wallet_balance_after_spend(amount)?;
        if self.account_frozen(keeper_account) {
            return Err(NodeError::AccountFrozen {
                account_id: keeper_account,
            });
        }
        let keeper_address = self
            .get_committed_state(&account_evm_address_state_key(keeper_account))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or(NodeError::InvalidDelegation {
                agent: keeper_account,
                reason: "real_capital_keeper_evm_address_missing",
            })?;
        self.get_committed_state(&policy_usdc_balance_state_key(keeper_address))
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes.as_slice())
                    .map(U256::from_be_bytes)
                    .map_err(|_| NodeError::InvalidDelegation {
                        agent: keeper_account,
                        reason: "real_capital_keeper_policy_usdc_balance_malformed",
                    })
            })
            .transpose()?;
        Ok(RealCapitalKeeperRewardCandidate {
            keeper_account,
            amount,
            wallet_balance_after,
            keeper_address,
        })
    }

    /// Pays a deterministically selected keeper after candidate preflight. The
    /// caller still owns keeper selection and work authorization. Keeper
    /// rewards intentionally credit the keeper's policy-USDC balance directly
    /// rather than any market custody address because the keeper is not a
    /// market-internal balance bucket.
    pub fn pay_real_capital_keeper_reward_escrow_v1(
        &mut self,
        keeper_account: u64,
        amount: u128,
        timestamp_ms: u64,
    ) -> Result<RealCapitalInsuranceWalletPayout, NodeError> {
        let payout = self.pay_real_capital_keeper_reward_escrow_v1_at(
            keeper_account,
            amount,
            timestamp_ms,
            None,
        )?;
        self.advance_pacemaker_on_current_head_qc()?;
        Ok(payout)
    }

    pub(super) fn pay_real_capital_insurance_wallet_escrow_v1_at(
        &mut self,
        recipient_account: u64,
        amount: u128,
        source: RealCapitalInsuranceWalletSpendSource,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<RealCapitalInsuranceWalletPayout, NodeError> {
        self.apply_real_capital_insurance_wallet_payout_effect_at(
            recipient_account,
            amount,
            source,
            timestamp_ms,
            event_block_height,
        )
    }

    pub(super) fn pay_real_capital_keeper_reward_escrow_v1_at(
        &mut self,
        keeper_account: u64,
        amount: u128,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<RealCapitalInsuranceWalletPayout, NodeError> {
        self.validate_real_capital_keeper_reward_candidate(keeper_account, amount)?;
        self.apply_real_capital_insurance_wallet_payout_effect_at(
            keeper_account,
            amount,
            RealCapitalInsuranceWalletSpendSource::KeeperReward,
            timestamp_ms,
            event_block_height,
        )
    }

    pub(super) fn pay_real_capital_forced_liquidation_cost_escrow_v1_at(
        &mut self,
        keeper_account: u64,
        amount: u128,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<RealCapitalInsuranceWalletPayout, NodeError> {
        self.validate_real_capital_keeper_reward_candidate(keeper_account, amount)?;
        self.apply_real_capital_insurance_wallet_payout_effect_at(
            keeper_account,
            amount,
            RealCapitalInsuranceWalletSpendSource::ForcedLiquidationCost,
            timestamp_ms,
            event_block_height,
        )
    }

    pub(super) fn apply_real_capital_insurance_wallet_payout_effect_at(
        &mut self,
        recipient_account: u64,
        amount: u128,
        source: RealCapitalInsuranceWalletSpendSource,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<RealCapitalInsuranceWalletPayout, NodeError> {
        let wallet_balance = self.real_capital_insurance_wallet_balance_after_spend(amount)?;
        let escrow_balance = self.real_capital_insurance_wallet_policy_balance()?;
        if escrow_balance != U256::from(self.real_capital_insurance_wallet_micro_usdc) {
            return Err(NodeError::InvalidDelegation {
                agent: recipient_account,
                reason: "real_capital_insurance_wallet_escrow_mismatch",
            });
        }
        let recipient_address = self
            .get_committed_state(&account_evm_address_state_key(recipient_account))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or(NodeError::InvalidDelegation {
                agent: recipient_account,
                reason: "real_capital_insurance_payout_recipient_evm_address_missing",
            })?;
        let recipient_balance = self
            .get_committed_state(&policy_usdc_balance_state_key(recipient_address))
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes.as_slice())
                    .map(U256::from_be_bytes)
                    .map_err(|_| NodeError::InvalidDelegation {
                        agent: recipient_account,
                        reason: "policy_usdc_balance_malformed",
                    })
            })
            .transpose()?
            .unwrap_or(U256::ZERO);
        let next_recipient_balance = recipient_balance.checked_add(U256::from(amount)).ok_or(
            NodeError::InvalidDelegation {
                agent: recipient_account,
                reason: "real_capital_insurance_payout_balance_overflow",
            },
        )?;
        if next_recipient_balance > U256::from(u128::MAX) {
            return Err(NodeError::InvalidDelegation {
                agent: recipient_account,
                reason: "real_capital_insurance_payout_balance_too_large",
            });
        }
        let next_escrow_balance =
            escrow_balance
                .checked_sub(U256::from(amount))
                .ok_or(NodeError::InvalidDelegation {
                    agent: recipient_account,
                    reason: "real_capital_insurance_wallet_escrow_underflow",
                })?;
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            recipient_address,
            next_recipient_balance,
        );
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_insurance_wallet_address(),
            next_escrow_balance,
        );
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: recipient_account,
                reason: "real_capital_insurance_payout_state_update_failed",
            })?;
        // Keep the EVM transfer and Rust wallet debit adjacent. QC rebuild is
        // fallible; it must not sit between the two accounting mutations.
        self.apply_real_capital_insurance_wallet_spend_effect(
            amount,
            wallet_balance,
            source,
            timestamp_ms,
            event_block_height,
        );
        Ok(RealCapitalInsuranceWalletPayout {
            recipient_account,
            amount,
            wallet_balance,
            recipient_balance: next_recipient_balance.to::<u128>(),
            source,
        })
    }

    pub(super) fn cover_market_bad_debt_from_real_capital_insurance_wallet_escrow_v1_at(
        &mut self,
        market_id: u64,
        amount: u128,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<u128, NodeError> {
        let next_balance = self.real_capital_insurance_wallet_balance_after_spend(amount)?;
        let escrow_balance = self.real_capital_insurance_wallet_policy_balance()?;
        if escrow_balance != U256::from(self.real_capital_insurance_wallet_micro_usdc) {
            return Err(NodeError::InvalidDelegation {
                agent: 0,
                reason: "real_capital_insurance_wallet_escrow_mismatch",
            });
        }
        let market_custody_address = isolated_margin_market_custody_address(market_id);
        let market_custody_balance = self.policy_usdc_balance_at_address(
            market_custody_address,
            0,
            "market_custody_balance_malformed",
        )?;
        let next_market_custody_balance = market_custody_balance
            .checked_add(U256::from(amount))
            .ok_or(NodeError::InvalidDelegation {
                agent: 0,
                reason: "market_custody_balance_overflow",
            })?;
        let next_escrow_balance =
            escrow_balance
                .checked_sub(U256::from(amount))
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "real_capital_insurance_wallet_escrow_underflow",
                })?;
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            market_custody_address,
            next_market_custody_balance,
        );
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_insurance_wallet_address(),
            next_escrow_balance,
        );
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: 0,
                reason: "real_capital_bad_debt_cover_state_update_failed",
            })?;
        self.apply_real_capital_insurance_wallet_spend_effect(
            amount,
            next_balance,
            RealCapitalInsuranceWalletSpendSource::BadDebtCover,
            timestamp_ms,
            event_block_height,
        );
        Ok(next_balance)
    }

    pub(super) fn delegation_termination_payout_weight_rows(
        &self,
        delegation_id: DelegationId,
    ) -> Vec<(u64, u128)> {
        self.delegation_termination_payout_weights
            .iter()
            .filter_map(|((weight_delegation_id, principal), weight)| {
                (*weight_delegation_id == delegation_id && *weight > 0)
                    .then_some((*principal, *weight))
            })
            .collect()
    }

    pub(super) fn preview_delegation_insurance_payout(
        &self,
        delegation_id: DelegationId,
        requested_amount_micro_usdc: u128,
    ) -> Result<DelegationInsurancePayoutPreview, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if delegation.status != DelegationStatus::Terminated {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_not_terminated",
            });
        }
        if requested_amount_micro_usdc == 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "zero_delegation_insurance_payout",
            });
        }

        let weights = self.delegation_termination_payout_weight_rows(delegation_id);
        if weights.is_empty() {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_insurance_payout_no_recipients",
            });
        }
        let total_weight = weights.iter().try_fold(0_u128, |sum, (_, weight)| {
            sum.checked_add(*weight)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_insurance_payout_weight_overflow",
                })
        })?;
        if total_weight == 0 {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_insurance_payout_no_recipients",
            });
        }

        let distributed_amount_micro_usdc =
            requested_amount_micro_usdc.min(self.real_capital_insurance_wallet_micro_usdc);
        let wallet_balance_after =
            self.real_capital_insurance_wallet_micro_usdc - distributed_amount_micro_usdc;
        if distributed_amount_micro_usdc == 0 {
            return Ok(DelegationInsurancePayoutPreview {
                requested_amount_micro_usdc,
                distributed_amount_micro_usdc,
                wallet_balance_after,
                recipients: Vec::new(),
            });
        }

        let mut provisional = Vec::with_capacity(weights.len());
        let mut allocated_total = 0_u128;
        for (principal, weight) in &weights {
            let amount = (U256::from(distributed_amount_micro_usdc) * U256::from(*weight)
                / U256::from(total_weight))
            .to::<u128>();
            allocated_total =
                allocated_total
                    .checked_add(amount)
                    .ok_or(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "delegation_insurance_payout_overflow",
                    })?;
            provisional.push((*principal, *weight, amount));
        }
        let mut leftover = distributed_amount_micro_usdc
            .checked_sub(allocated_total)
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_insurance_payout_overflow",
            })?;
        for (_, weight, amount) in &mut provisional {
            if leftover == 0 {
                break;
            }
            if *weight == 0 {
                continue;
            }
            *amount = amount.saturating_add(1);
            leftover -= 1;
        }

        let mut recipients = Vec::new();
        for (principal, _, amount_micro_usdc) in provisional {
            if amount_micro_usdc == 0 {
                continue;
            }
            let policy_address = self.account_policy_address_for_delegation(
                principal,
                delegation.agent,
                "real_capital_insurance_payout_recipient_evm_address_missing",
            )?;
            let balance =
                self.policy_usdc_balance_for_delegation(policy_address, delegation.agent)?;
            let next_policy_balance = balance.checked_add(U256::from(amount_micro_usdc)).ok_or(
                NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "real_capital_insurance_payout_balance_overflow",
                },
            )?;
            if next_policy_balance > U256::from(u128::MAX) {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "real_capital_insurance_payout_balance_too_large",
                });
            }
            recipients.push(DelegationInsurancePayoutRecipient {
                principal,
                amount_micro_usdc,
                policy_address,
                next_policy_balance,
            });
        }

        Ok(DelegationInsurancePayoutPreview {
            requested_amount_micro_usdc,
            distributed_amount_micro_usdc,
            wallet_balance_after,
            recipients,
        })
    }

    pub(super) fn pay_delegation_insurance_payout_escrow_v1_at(
        &mut self,
        delegation_id: DelegationId,
        preview: &DelegationInsurancePayoutPreview,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<(), NodeError> {
        if preview.distributed_amount_micro_usdc == 0 {
            return Ok(());
        }
        let delegation_agent = self
            .delegations
            .get(&delegation_id)
            .map(|delegation| delegation.agent)
            .ok_or(NodeError::InvalidDelegation {
                agent: 0,
                reason: "unknown_delegation",
            })?;
        let escrow_balance = self.real_capital_insurance_wallet_policy_balance()?;
        if escrow_balance != U256::from(self.real_capital_insurance_wallet_micro_usdc) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation_agent,
                reason: "real_capital_insurance_wallet_escrow_mismatch",
            });
        }
        let next_escrow_balance = escrow_balance
            .checked_sub(U256::from(preview.distributed_amount_micro_usdc))
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation_agent,
                reason: "real_capital_insurance_wallet_escrow_underflow",
            })?;
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        for recipient in &preview.recipients {
            Self::apply_policy_usdc_balance_update(
                &mut sets,
                &mut deletes,
                recipient.policy_address,
                recipient.next_policy_balance,
            );
        }
        Self::apply_policy_usdc_balance_update(
            &mut sets,
            &mut deletes,
            real_capital_insurance_wallet_address(),
            next_escrow_balance,
        );
        self.apply_committed_state_updates(sets, deletes)
            .map_err(|_| NodeError::InvalidDelegation {
                agent: delegation_agent,
                reason: "real_capital_insurance_payout_state_update_failed",
            })?;
        self.apply_real_capital_insurance_wallet_spend_effect(
            preview.distributed_amount_micro_usdc,
            preview.wallet_balance_after,
            RealCapitalInsuranceWalletSpendSource::UserPayout,
            timestamp_ms,
            event_block_height,
        );
        Ok(())
    }

    pub(super) fn apply_delegation_insurance_payout_at(
        &mut self,
        delegation_id: DelegationId,
        requested_amount_micro_usdc: u128,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<DelegationInsurancePayout, NodeError> {
        let delegation =
            self.delegations
                .get(&delegation_id)
                .cloned()
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        let preview =
            self.preview_delegation_insurance_payout(delegation_id, requested_amount_micro_usdc)?;
        self.pay_delegation_insurance_payout_escrow_v1_at(
            delegation_id,
            &preview,
            timestamp_ms,
            event_block_height,
        )?;
        Ok(DelegationInsurancePayout {
            delegation,
            requested_amount_micro_usdc: preview.requested_amount_micro_usdc,
            distributed_amount_micro_usdc: preview.distributed_amount_micro_usdc,
            recipient_count: preview.recipients.len() as u64,
        })
    }

    pub(super) fn advance_pacemaker_on_current_head_qc(&mut self) -> Result<(), NodeError> {
        let updated_head_qc = build_quorum_certificate(self.head(), &self.consensus_signing_keys)?;
        self.pacemaker.advance_on_qc(updated_head_qc);
        Ok(())
    }

    pub(super) fn real_capital_insurance_wallet_balance_after_spend(
        &self,
        amount: u128,
    ) -> Result<u128, NodeError> {
        if amount == 0 {
            return Err(NodeError::InvalidDelegation {
                agent: 0,
                reason: "zero_real_capital_insurance_wallet_spend",
            });
        }
        let next_balance = self
            .real_capital_insurance_wallet_micro_usdc
            .checked_sub(amount)
            .ok_or(NodeError::InvalidDelegation {
                agent: 0,
                reason: "real_capital_insurance_wallet_underflow",
            })?;
        Ok(next_balance)
    }

    pub(super) fn apply_real_capital_insurance_wallet_spend_effect(
        &mut self,
        amount: u128,
        next_balance: u128,
        source: RealCapitalInsuranceWalletSpendSource,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) {
        self.real_capital_insurance_wallet_micro_usdc = next_balance;
        self.record_runtime_event(
            event_block_height,
            timestamp_ms,
            RuntimeEventKind::RealCapitalInsuranceWalletSpent {
                amount,
                balance: next_balance,
                source,
            },
        );
    }

    pub fn protocol_delegation_cap(&self) -> Option<ProtocolDelegationCap> {
        self.protocol_delegation_cap
    }

    pub fn listing_control_state(&self) -> ListingControlState {
        self.listing_control_state
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn test_upsert_real_capital_listing(
        &mut self,
        listing: RealCapitalListing,
    ) -> Result<(), NodeError> {
        self.validate_real_capital_listing(&listing)?;
        self.upsert_real_capital_listing(listing);
        Ok(())
    }

    pub(super) fn upsert_real_capital_listing(&mut self, listing: RealCapitalListing) {
        if let Some(existing) = self.real_capital_listings.get(&listing.vault_id) {
            if existing.serving_agent != listing.serving_agent {
                if let Some(vaults) = self
                    .real_capital_listings_by_agent
                    .get_mut(&existing.serving_agent)
                {
                    vaults.remove(&listing.vault_id);
                    if vaults.is_empty() {
                        self.real_capital_listings_by_agent
                            .remove(&existing.serving_agent);
                    }
                }
            }
        }
        self.real_capital_listings_by_agent
            .entry(listing.serving_agent)
            .or_default()
            .insert(listing.vault_id);
        self.real_capital_listings.insert(listing.vault_id, listing);
    }

    pub(super) fn real_capital_listing_below_required_tier(
        &self,
        listing: &RealCapitalListing,
        tier: AgentIdentityTier,
    ) -> bool {
        match listing.mode {
            VaultRealCapitalMode::Disabled => false,
            VaultRealCapitalMode::CappedPilot => {
                listing
                    .pilot_min_serving_agent_tier
                    .is_some_and(|required| {
                        agent_identity_tier_rank(tier) < agent_identity_tier_rank(required)
                    })
            }
            VaultRealCapitalMode::Public => tier != AgentIdentityTier::Tier2TeeAttested,
        }
    }

    pub(super) fn apply_real_capital_tier_loss_sweep(
        &mut self,
        account: u64,
        new_tier: AgentIdentityTier,
        block_height: u64,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) {
        let Some(vault_ids) = self.real_capital_listings_by_agent.get(&account).cloned() else {
            return;
        };
        for vault_id in vault_ids {
            let Some(listing) = self.real_capital_listings.get(&vault_id).cloned() else {
                continue;
            };
            if !self.real_capital_listing_below_required_tier(&listing, new_tier) {
                continue;
            }
            // A listing may already be deposit-disabled by governance while
            // its delegation remains live. Tier loss still terminates the
            // delegation; the deposit-toggle event inside Terminate only fires when the
            // listing flag actually flips.
            if self
                .delegations
                .get(&listing.delegation_id)
                .is_none_or(|delegation| {
                    matches!(
                        delegation.status,
                        DelegationStatus::Closed | DelegationStatus::Terminated
                    )
                })
            {
                continue;
            }
            self.terminate_delegation_at(
                listing.delegation_id,
                BlockDelegationFaultReason::AgentAttestationRevoked,
                None,
                timestamp_ms,
                block_height,
                event_block_height,
            )
            .expect("tier-loss sweep should only target valid live listed delegations");
        }
    }

    pub(super) fn delegation_has_winding_residual(&self, delegation: &Delegation) -> bool {
        let trading_account = delegation_trading_account_id(&delegation.delegation_id);
        if !self.positions(trading_account, false, 1).is_empty() {
            return true;
        }
        // Active redemption locks remain represented by delegation shares;
        // finalization burns those shares only while releasing free balance
        // or compensation. The automatic grace fault is about residual
        // capital/risk, so the sweep follows the RFC predicate here and lets
        // share/redemption-only cases continue through normal close/redeem
        // flows.
        match self.delegation_free_usdc_balance(&delegation.delegation_id) {
            Ok(balance) => balance > 0,
            // Malformed balance state should fail closed: the delegation
            // cannot prove it has fully flattened and returned capital.
            Err(_) => true,
        }
    }

    pub(super) fn apply_due_winding_grace_failures(&mut self, block: &Block) {
        let timestamp_ms = block.header.timestamp_ms;
        let due: Vec<DelegationId> = self
            .delegations
            .values()
            .filter(|delegation| delegation.status == DelegationStatus::Winding)
            .filter(|delegation| {
                delegation.winding_started_at_ms.is_some_and(|started_at| {
                    timestamp_ms >= started_at.saturating_add(WINDING_GRACE_WINDOW_MS)
                })
            })
            .filter(|delegation| self.delegation_has_winding_residual(delegation))
            .map(|delegation| delegation.delegation_id)
            .collect();
        for delegation_id in due {
            // This sweep only runs during committed block application, so
            // the event height is always the block being applied. Delegation-
            // backed MM auto-exit and MM-bond refund already flow through
            // terminate_delegation_at, so this sweep must not add a second
            // refund path of its own.
            let result = self.terminate_delegation_at(
                delegation_id,
                BlockDelegationFaultReason::WindingGraceFailure,
                None,
                timestamp_ms,
                block.header.height,
                Some(block.header.height),
            );
            // Invariants above guarantee a live Winding delegation with a
            // recorded winding start and residual consensus state. Any
            // failure here is snapshot/state corruption and should halt
            // rather than silently skip a deterministic fault.
            result.expect("winding-grace sweep should only target valid live winding delegations");
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn test_set_protocol_delegation_cap(&mut self, cap: Option<ProtocolDelegationCap>) {
        self.protocol_delegation_cap = cap;
    }

    pub fn delegation_share(
        &self,
        delegation_id: &DelegationId,
        principal: u64,
    ) -> DelegationShare {
        self.delegation_shares
            .get(&(*delegation_id, principal))
            .copied()
            .unwrap_or_default()
    }

    pub fn redemption_request(
        &self,
        request_id: &RedemptionRequestId,
    ) -> Option<&RedemptionRequest> {
        self.redemption_requests.get(request_id)
    }

    pub fn redemption_requests(&self) -> impl Iterator<Item = &RedemptionRequest> {
        self.redemption_requests.values()
    }

    pub(super) fn active_delegation_count(&self) -> usize {
        self.delegations
            .values()
            .filter(|d| {
                !matches!(
                    d.status,
                    DelegationStatus::Closed | DelegationStatus::Terminated
                )
            })
            .count()
    }

    pub(super) fn live_delegations_for_agent(&self, agent: u64) -> usize {
        self.delegations
            .values()
            .filter(|d| {
                d.agent == agent
                    && !matches!(
                        d.status,
                        DelegationStatus::Closed | DelegationStatus::Terminated
                    )
            })
            .count()
    }

    pub(super) fn validate_real_capital_listing(
        &self,
        listing: &RealCapitalListing,
    ) -> Result<(), NodeError> {
        let reject = |reason| NodeError::InvalidDelegation {
            agent: listing.serving_agent,
            reason,
        };
        if listing.vault_id != vault_id_for_delegation(&listing.delegation_id) {
            return Err(reject("vault_listing_id_mismatch"));
        }
        let delegation = self
            .delegations
            .get(&listing.delegation_id)
            .ok_or(reject("vault_listing_delegation_missing"))?;
        if delegation.agent != listing.serving_agent {
            return Err(reject("vault_listing_agent_mismatch"));
        }
        if listing.deposit_enabled
            && (listing.caps.per_principal_micro_usdc == 0
                || listing.caps.total_vault_tvl_micro_usdc == 0)
        {
            return Err(reject("vault_listing_cap_zero"));
        }
        match listing.mode {
            VaultRealCapitalMode::Disabled => {}
            VaultRealCapitalMode::CappedPilot => match listing.pilot_min_serving_agent_tier {
                Some(tier)
                    if agent_identity_tier_rank(tier)
                        >= agent_identity_tier_rank(AgentIdentityTier::Tier1OperatorAttested) => {}
                Some(_) => return Err(reject("vault_listing_tier_requirement_too_low")),
                None if listing.tier0_pilot_exception => {}
                None => return Err(reject("vault_listing_tier_exception_required")),
            },
            VaultRealCapitalMode::Public => {
                if listing.pilot_min_serving_agent_tier.is_some() {
                    return Err(reject("vault_listing_public_tier_field_set"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn ensure_delegation_deposit_real_capital_admission(
        &self,
        principal: u64,
        delegation: &Delegation,
        amount: u128,
    ) -> Result<DelegationDepositCapAccounting, NodeError> {
        let vault_id = vault_id_for_delegation(&delegation.delegation_id);
        let listing =
            self.real_capital_listings
                .get(&vault_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_not_listed",
                })?;
        if listing.delegation_id != delegation.delegation_id
            || listing.serving_agent != delegation.agent
            || listing.vault_id != vault_id
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_listing_invariant_violation",
            });
        }
        if self.listing_control_state.global_deposit_kill_switch
            || !listing.deposit_enabled
            || listing.mode == VaultRealCapitalMode::Disabled
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_real_capital_disabled",
            });
        }
        if listing.mode == VaultRealCapitalMode::CappedPilot
            && listing
                .pilot_principal_allowlist
                .as_ref()
                .is_some_and(|set| !set.contains(&principal))
        {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_pilot_principal_not_allowlisted",
            });
        }
        let agent_tier = self
            .agent_identities
            .get(&delegation.agent)
            .map(|identity| identity.tier)
            .unwrap_or(AgentIdentityTier::Tier0Unattested);
        match listing.mode {
            VaultRealCapitalMode::Disabled => unreachable!("handled above"),
            VaultRealCapitalMode::CappedPilot => {
                if let Some(required_tier) = listing.pilot_min_serving_agent_tier {
                    if agent_identity_tier_rank(agent_tier)
                        < agent_identity_tier_rank(required_tier)
                    {
                        return Err(NodeError::InvalidDelegation {
                            agent: delegation.agent,
                            reason: "vault_serving_agent_below_tier",
                        });
                    }
                } else if !listing.tier0_pilot_exception {
                    return Err(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "vault_listing_invariant_violation",
                    });
                }
            }
            VaultRealCapitalMode::Public => {
                if agent_tier != AgentIdentityTier::Tier2TeeAttested {
                    return Err(NodeError::InvalidDelegation {
                        agent: delegation.agent,
                        reason: "vault_serving_agent_below_tier",
                    });
                }
            }
        }
        self.delegation_deposit_cap_accounting(principal, delegation, listing, amount)
    }

    pub(super) fn delegation_deposit_cap_accounting(
        &self,
        principal: u64,
        delegation: &Delegation,
        listing: &RealCapitalListing,
        amount: u128,
    ) -> Result<DelegationDepositCapAccounting, NodeError> {
        let prior_principal_deposited = self
            .vault_principal_deposited_micro_usdc
            .get(&(listing.vault_id, principal))
            .copied()
            .unwrap_or_default();
        let principal_deposited_micro_usdc =
            prior_principal_deposited
                .checked_add(amount)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_principal_cap_exceeded",
                })?;
        if principal_deposited_micro_usdc > listing.caps.per_principal_micro_usdc {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_principal_cap_exceeded",
            });
        }
        let prior_vault_deposited = self
            .vault_deposited_micro_usdc
            .get(&listing.vault_id)
            .copied()
            .unwrap_or_default();
        let vault_deposited_micro_usdc =
            prior_vault_deposited
                .checked_add(amount)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_tvl_cap_exceeded",
                })?;
        if vault_deposited_micro_usdc > listing.caps.total_vault_tvl_micro_usdc {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_tvl_cap_exceeded",
            });
        }
        if let Some(protocol_cap) = self.protocol_delegation_cap {
            let protocol_tvl = self.total_protocol_delegation_tvl_micro_usdc()?;
            if protocol_tvl
                .checked_add(amount)
                .is_none_or(|value| value > protocol_cap.total_protocol_tvl_micro_usdc)
            {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_protocol_cap_exceeded",
                });
            }
        }
        Ok(DelegationDepositCapAccounting {
            vault_id: listing.vault_id,
            principal,
            principal_deposited_micro_usdc,
            vault_deposited_micro_usdc,
        })
    }

    pub(super) fn apply_delegation_deposit_cap_accounting(
        &mut self,
        accounting: DelegationDepositCapAccounting,
    ) {
        self.vault_principal_deposited_micro_usdc.insert(
            (accounting.vault_id, accounting.principal),
            accounting.principal_deposited_micro_usdc,
        );
        self.vault_deposited_micro_usdc
            .insert(accounting.vault_id, accounting.vault_deposited_micro_usdc);
    }

    pub(super) fn delegation_redemption_cap_accounting(
        &self,
        delegation: &Delegation,
        principal: u64,
        shares_requested: u128,
        prior_shares: u128,
    ) -> Result<Option<DelegationRedemptionCapAccounting>, NodeError> {
        let vault_id = vault_id_for_delegation(&delegation.delegation_id);
        let Some(principal_deposited) = self
            .vault_principal_deposited_micro_usdc
            .get(&(vault_id, principal))
            .copied()
        else {
            return Ok(None);
        };
        if prior_shares == 0 || shares_requested > prior_shares {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_cap_accounting_invalid",
            });
        }
        let released = if shares_requested == prior_shares {
            principal_deposited
        } else {
            principal_deposited
                .checked_mul(shares_requested)
                .and_then(|value| value.checked_add(prior_shares - 1))
                .and_then(|value| value.checked_div(prior_shares))
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_cap_accounting_invalid",
                })?
        };
        let new_principal_deposited =
            principal_deposited
                .checked_sub(released)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_cap_accounting_invalid",
                })?;
        let vault_deposited = self
            .vault_deposited_micro_usdc
            .get(&vault_id)
            .copied()
            .ok_or(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "vault_cap_accounting_invalid",
            })?;
        let new_vault_deposited =
            vault_deposited
                .checked_sub(released)
                .ok_or(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "vault_cap_accounting_invalid",
                })?;
        Ok(Some(DelegationRedemptionCapAccounting {
            vault_id,
            principal,
            principal_deposited_micro_usdc: new_principal_deposited,
            vault_deposited_micro_usdc: new_vault_deposited,
        }))
    }

    pub(super) fn apply_delegation_redemption_cap_accounting(
        &mut self,
        accounting: Option<DelegationRedemptionCapAccounting>,
    ) {
        let Some(accounting) = accounting else {
            return;
        };
        if accounting.principal_deposited_micro_usdc == 0 {
            self.vault_principal_deposited_micro_usdc
                .remove(&(accounting.vault_id, accounting.principal));
        } else {
            self.vault_principal_deposited_micro_usdc.insert(
                (accounting.vault_id, accounting.principal),
                accounting.principal_deposited_micro_usdc,
            );
        }
        if accounting.vault_deposited_micro_usdc == 0 {
            self.vault_deposited_micro_usdc.remove(&accounting.vault_id);
        } else {
            self.vault_deposited_micro_usdc
                .insert(accounting.vault_id, accounting.vault_deposited_micro_usdc);
        }
    }

    pub(super) fn delegation_compensation_release(
        &self,
        delegation_id: DelegationId,
        principal: u64,
        shares_requested: u128,
        prior_shares: u128,
    ) -> Result<u128, NodeError> {
        let Some(claim) = self
            .delegation_compensation_claims
            .get(&(delegation_id, principal))
            .copied()
        else {
            return Ok(0);
        };
        let delegation =
            self.delegations
                .get(&delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        if prior_shares == 0 || shares_requested > prior_shares {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_compensation_accounting_invalid",
            });
        }
        if shares_requested == prior_shares {
            // Full-share redemption releases the whole remaining claim so
            // the persisted claim->share invariant is cleared atomically
            // with the final share removal.
            if claim > self.real_capital_compensation_escrow_micro_usdc {
                return Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "real_capital_compensation_escrow_underflow",
                });
            }
            return Ok(claim);
        }
        let release = (U256::from(claim) * U256::from(shares_requested) / U256::from(prior_shares))
            .to::<u128>();
        if release > self.real_capital_compensation_escrow_micro_usdc {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "real_capital_compensation_escrow_underflow",
            });
        }
        Ok(release)
    }

    pub(super) fn apply_delegation_compensation_release(
        &mut self,
        delegation_id: DelegationId,
        principal: u64,
        compensation_out: u128,
    ) {
        // Caller must first compute compensation_out with
        // delegation_compensation_release, which checks claim and escrow
        // availability against the same share burn.
        if compensation_out == 0 {
            return;
        }
        let key = (delegation_id, principal);
        let Some(prior_claim) = self.delegation_compensation_claims.get(&key).copied() else {
            return;
        };
        let next_claim = prior_claim.saturating_sub(compensation_out);
        if next_claim == 0 {
            self.delegation_compensation_claims.remove(&key);
        } else {
            self.delegation_compensation_claims.insert(key, next_claim);
        }
        self.real_capital_compensation_escrow_micro_usdc = self
            .real_capital_compensation_escrow_micro_usdc
            .checked_sub(compensation_out)
            .expect("compensation release preflight should cover escrow");
    }

    pub(super) fn delegation_bond_escrow_amount(
        &self,
        delegation: &Delegation,
    ) -> Result<u128, NodeError> {
        let evm_balance = self.policy_usdc_balance_at_address(
            real_capital_bond_escrow_address(&delegation.delegation_id),
            delegation.agent,
            "real_capital_bond_escrow_balance_malformed",
        )?;
        if delegation.agent_bond == 0 {
            return if evm_balance.is_zero() {
                Ok(0)
            } else {
                Err(NodeError::InvalidDelegation {
                    agent: delegation.agent,
                    reason: "delegation_bond_escrow_mismatch",
                })
            };
        }
        match self
            .real_capital_bond_escrow_micro_usdc
            .get(&delegation.delegation_id)
            .copied()
        {
            Some(amount)
                if amount == delegation.agent_bond && evm_balance == U256::from(amount) =>
            {
                Ok(amount)
            }
            _ => Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_bond_escrow_mismatch",
            }),
        }
    }

    pub fn total_protocol_delegation_tvl_micro_usdc(&self) -> Result<u128, NodeError> {
        // TODO(performance): cache this running total once real-capital
        // delegation counts grow beyond the pilot scale.
        let mut total = 0_u128;
        for deposited in self.vault_deposited_micro_usdc.values() {
            total = total
                .checked_add(*deposited)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "vault_protocol_cap_exceeded",
                })?;
        }
        Ok(total)
    }

    pub(super) fn ensure_delegation_mandate_binding(
        &self,
        agent: u64,
        mandate_id: MandateId,
        timestamp_ms: u64,
    ) -> Result<(), NodeError> {
        let mandate = self
            .mandates
            .get(&mandate_id)
            .ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "unknown_mandate",
            })?;
        if mandate.agent != agent {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "mandate_agent_mismatch",
            });
        }
        if mandate.state != MandateState::Active {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "mandate_not_active",
            });
        }
        if timestamp_ms >= mandate.expires_at {
            return Err(NodeError::InvalidDelegation {
                agent,
                reason: "mandate_expired",
            });
        }
        Ok(())
    }

    pub(super) fn account_policy_address_for_delegation(
        &self,
        account_id: u64,
        agent: u64,
        reason: &'static str,
    ) -> Result<[u8; 20], NodeError> {
        self.get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or(NodeError::InvalidDelegation { agent, reason })
    }

    pub(super) fn policy_usdc_balance_at_address(
        &self,
        address: [u8; 20],
        agent: u64,
        reason: &'static str,
    ) -> Result<U256, NodeError> {
        self.get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes.as_slice())
                    .map(U256::from_be_bytes)
                    .map_err(|_| NodeError::InvalidDelegation { agent, reason })
            })
            .transpose()
            .map(|balance| balance.unwrap_or(U256::ZERO))
    }

    pub(super) fn policy_usdc_balance_for_delegation(
        &self,
        address: [u8; 20],
        agent: u64,
    ) -> Result<U256, NodeError> {
        self.policy_usdc_balance_at_address(address, agent, "policy_usdc_balance_malformed")
    }

    pub(super) fn apply_policy_usdc_balance_update(
        sets: &mut Vec<(Vec<u8>, Vec<u8>)>,
        deletes: &mut Vec<Vec<u8>>,
        address: [u8; 20],
        next_balance: U256,
    ) {
        if next_balance.is_zero() {
            deletes.push(policy_usdc_balance_state_key(address));
        } else {
            sets.push((
                policy_usdc_balance_state_key(address),
                next_balance.to_be_bytes::<32>().to_vec(),
            ));
        }
    }

    pub fn delegation_free_usdc_balance(
        &self,
        delegation_id: &DelegationId,
    ) -> Result<u128, NodeError> {
        let delegation =
            self.delegations
                .get(delegation_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent: 0,
                    reason: "unknown_delegation",
                })?;
        let balance = self.policy_usdc_balance_for_delegation(
            delegation_trading_address(delegation_id),
            delegation.agent,
        )?;
        if balance > U256::from(u128::MAX) {
            return Err(NodeError::InvalidDelegation {
                agent: delegation.agent,
                reason: "delegation_balance_too_large",
            });
        }
        Ok(balance.to::<u128>())
    }

    pub(super) fn has_active_delegation_redemption(&self, delegation_id: &DelegationId) -> bool {
        self.redemption_requests.values().any(|request| {
            request.delegation_id == *delegation_id
                && matches!(
                    request.status,
                    RedemptionRequestStatus::Pending | RedemptionRequestStatus::Ready
                )
        })
    }

    pub(super) fn live_delegation_id_for_mandate(
        &self,
        mandate_id: &MandateId,
    ) -> Option<DelegationId> {
        self.delegations
            .iter()
            .find_map(|(delegation_id, delegation)| {
                (delegation.mandate_id == *mandate_id
                    && matches!(
                        delegation.status,
                        DelegationStatus::Open
                            | DelegationStatus::ClosedToDeposits
                            | DelegationStatus::Winding
                    ))
                .then_some(*delegation_id)
            })
    }

    pub(super) fn bindable_mandate_policy_ids_for_delegation(
        &self,
        mandate_id: MandateId,
        agent: u64,
    ) -> Result<Vec<u64>, NodeError> {
        let mandate = self
            .mandates
            .get(&mandate_id)
            .ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "unknown_mandate",
            })?;
        for policy_id in &mandate.policies {
            let policy = self
                .policy_store
                .get(*policy_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent,
                    reason: "mandate_policy_missing",
                })?;
            if policy.authority_account != agent {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "mandate_policy_authority_mismatch",
                });
            }
            if policy.state != PolicyState::Active {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "mandate_policy_not_active",
                });
            }
            if policy.execution_scope != PolicyExecutionScope::Account(agent) {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "mandate_policy_scope_mismatch",
                });
            }
            match policy.origin {
                PolicyOrigin::MandateGenerated {
                    mandate_id: origin_mandate_id,
                    ..
                } if origin_mandate_id == mandate_id => {}
                _ => {
                    return Err(NodeError::InvalidDelegation {
                        agent,
                        reason: "mandate_policy_origin_mismatch",
                    });
                }
            }
        }
        Ok(mandate.policies.clone())
    }

    pub(super) fn delegated_mandate_policy_ids(
        &self,
        delegation_id: &DelegationId,
        mandate_id: MandateId,
        agent: u64,
    ) -> Result<Vec<u64>, NodeError> {
        let mandate = self
            .mandates
            .get(&mandate_id)
            .ok_or(NodeError::InvalidDelegation {
                agent,
                reason: "unknown_mandate",
            })?;
        let expected_scope = PolicyExecutionScope::DelegationTrading {
            delegation_id: *delegation_id,
        };
        for policy_id in &mandate.policies {
            let policy = self
                .policy_store
                .get(*policy_id)
                .ok_or(NodeError::InvalidDelegation {
                    agent,
                    reason: "mandate_policy_missing",
                })?;
            if policy.authority_account != agent {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "mandate_policy_authority_mismatch",
                });
            }
            if policy.execution_scope != expected_scope {
                return Err(NodeError::InvalidDelegation {
                    agent,
                    reason: "delegation_policy_scope_mismatch",
                });
            }
            match policy.origin {
                PolicyOrigin::MandateGenerated {
                    mandate_id: origin_mandate_id,
                    ..
                } if origin_mandate_id == mandate_id => {}
                _ => {
                    return Err(NodeError::InvalidDelegation {
                        agent,
                        reason: "mandate_policy_origin_mismatch",
                    });
                }
            }
        }
        Ok(mandate.policies.clone())
    }

    pub(super) fn bind_mandate_policy_scopes_to_delegation(
        &mut self,
        policy_ids: &[u64],
        delegation_id: DelegationId,
    ) {
        let execution_scope = PolicyExecutionScope::DelegationTrading { delegation_id };
        for policy_id in policy_ids {
            let policy = self
                .policy_store
                .get_mut(*policy_id)
                .expect("validated mandate policies should remain present");
            policy.execution_scope = execution_scope;
        }
    }

    pub(super) fn apply_delegation_policy_lifecycle_state(
        &mut self,
        delegation: &Delegation,
        old_status: DelegationStatus,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<(), NodeError> {
        let policy_ids = self.delegated_mandate_policy_ids(
            &delegation.delegation_id,
            delegation.mandate_id,
            delegation.agent,
        )?;
        match delegation.status {
            DelegationStatus::Open | DelegationStatus::ClosedToDeposits => {
                if old_status == DelegationStatus::Winding {
                    for policy_id in policy_ids {
                        self.policy_store
                            .resume(policy_id)
                            .map_err(NodeError::Policy)?;
                    }
                }
            }
            DelegationStatus::Winding => {
                for policy_id in policy_ids {
                    self.policy_store
                        .suspend(policy_id)
                        .map_err(NodeError::Policy)?;
                }
            }
            DelegationStatus::Closed | DelegationStatus::Terminated => {
                let policy_events = self.policy_revocation_events(&policy_ids, delegation.agent)?;
                let block_number = self.head().header.height;
                for policy_id in policy_ids {
                    if self.policy_store.get(policy_id).is_some_and(|policy| {
                        matches!(policy.state, PolicyState::Revoked | PolicyState::Expired)
                    }) {
                        continue;
                    }
                    self.policy_store
                        .revoke(policy_id, block_number, timestamp_ms)
                        .map_err(NodeError::Policy)?;
                }
                for event in policy_events {
                    self.record_runtime_event(event_block_height, timestamp_ms, event);
                }
            }
        }
        Ok(())
    }

    pub(super) fn record_delegation_status_changed(
        &mut self,
        delegation: &Delegation,
        old_status: DelegationStatus,
        reason: DelegationStatusReason,
        bond_returned: u128,
        bond_slashed: u128,
        principal_compensation: u128,
        insurance_fund_delta: u128,
        timestamp_ms: u64,
    ) {
        self.record_delegation_status_changed_at(
            None,
            delegation,
            old_status,
            reason,
            bond_returned,
            bond_slashed,
            principal_compensation,
            insurance_fund_delta,
            timestamp_ms,
        );
    }

    pub(super) fn record_delegation_status_changed_at(
        &mut self,
        block_height: Option<u64>,
        delegation: &Delegation,
        old_status: DelegationStatus,
        reason: DelegationStatusReason,
        bond_returned: u128,
        bond_slashed: u128,
        principal_compensation: u128,
        insurance_fund_delta: u128,
        timestamp_ms: u64,
    ) {
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::DelegationStatusChanged {
                delegation_id: delegation.delegation_id,
                agent: delegation.agent,
                old_status: delegation_lifecycle_status(old_status),
                new_status: delegation_lifecycle_status(delegation.status),
                reason,
                bond_returned,
                bond_slashed,
                principal_compensation,
                insurance_fund_delta,
            },
        );
    }

    pub(super) fn delegation_fault_status_reason(
        reason: BlockDelegationFaultReason,
    ) -> DelegationStatusReason {
        match reason {
            BlockDelegationFaultReason::MandateBreached => DelegationStatusReason::MandateBreached,
            BlockDelegationFaultReason::UnauthorizedTradingAttempt => {
                DelegationStatusReason::UnauthorizedTradingAttempt
            }
            BlockDelegationFaultReason::RiskLimitBreach => DelegationStatusReason::RiskLimitBreach,
            BlockDelegationFaultReason::AgentAttestationRevoked => {
                DelegationStatusReason::AgentAttestationRevoked
            }
            BlockDelegationFaultReason::WindingGraceFailure => {
                DelegationStatusReason::WindingGraceFailure
            }
            BlockDelegationFaultReason::GovernanceFault => DelegationStatusReason::GovernanceFault,
        }
    }
}
