use super::*;

impl SingleNodeEngine {
    pub(crate) fn validate_persisted_order_books(&self) -> Result<(), std::io::Error> {
        let mut resting_order_keys = BTreeSet::new();
        for (&market_id, book) in &self.order_books {
            if book.market_id() != market_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot order book market mismatch",
                ));
            }

            let mut resting_order_ids = BTreeSet::new();

            for order in book.bids() {
                let key = resting_order_key(market_id, order.account_id, order.client_order_id);
                if order.market_id != market_id
                    || order.side != kombat_chain_matching::OrderSide::Bid
                    || order.remaining_lots == 0
                    || !resting_order_ids.insert((order.account_id, order.client_order_id))
                    || !resting_order_keys.insert(key)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot resting order mismatch",
                    ));
                }
            }

            for order in book.asks() {
                let key = resting_order_key(market_id, order.account_id, order.client_order_id);
                if order.market_id != market_id
                    || order.side != kombat_chain_matching::OrderSide::Ask
                    || order.remaining_lots == 0
                    || !resting_order_ids.insert((order.account_id, order.client_order_id))
                    || !resting_order_keys.insert(key)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot resting order mismatch",
                    ));
                }
            }
        }

        for (key, mandate_id) in &self.resting_order_mandates {
            let active_mandate = self
                .mandates
                .get(mandate_id)
                .is_some_and(|mandate| mandate.state == MandateState::Active);
            if !resting_order_keys.contains(key) || !active_mandate {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot resting order mandate mismatch",
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_positions(&self) -> Result<(), std::io::Error> {
        for (&(market_id, account_id), ledger) in &self.positions {
            if market_id == 0
                || account_id == 0
                || (ledger.open_net_lots == 0) != (ledger.open_notional == 0)
                || ledger.last_fill_seq.is_some() != ledger.last_fill_timestamp_ms.is_some()
                || (ledger.buy_lots == 0) != (ledger.buy_notional == 0)
                || (ledger.sell_lots == 0) != (ledger.sell_notional == 0)
                || (ledger.open_net_lots == 0 && ledger.mandate_id.is_some())
                || (ledger.open_net_lots == 0) != ledger.opened_at_timestamp_ms.is_none()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot position ledger mismatch",
                ));
            }
            if ledger
                .mandate_id
                .is_some_and(|mandate_id| !self.mandates.contains_key(&mandate_id))
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot position mandate mismatch",
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_market_state(&self) -> Result<(), std::io::Error> {
        let market_ids = self
            .market_marks
            .keys()
            .copied()
            .chain(self.markets.keys().copied())
            .chain(self.market_index_prices.keys().copied())
            .chain(
                self.oracle_feed_submissions
                    .keys()
                    .map(|(market_id, _, _)| *market_id),
            )
            .chain(self.market_oracle_config.keys().copied())
            .chain(self.market_risk.keys().copied())
            .chain(self.market_fees.keys().copied())
            .chain(self.paused_markets.iter().copied())
            .chain(self.market_funding_index.keys().copied())
            .chain(self.market_last_funding_ms.keys().copied())
            .chain(self.insurance_funds.keys().copied())
            .chain(self.market_bad_debt_balances.keys().copied())
            .chain(self.fee_vaults.keys().copied())
            .chain(self.validator_reward_pools.keys().copied())
            .chain(self.market_maker_rebate_pools.keys().copied())
            .chain(self.treasury_balances.keys().copied())
            .chain(self.order_books.keys().copied())
            .chain(self.positions.keys().map(|(market_id, _)| *market_id))
            .chain(
                self.isolated_collateral
                    .keys()
                    .map(|(market_id, _)| *market_id),
            )
            .chain(
                self.account_market_bad_debt
                    .keys()
                    .map(|(market_id, _)| *market_id),
            )
            .chain(
                self.under_maintenance_accounts
                    .iter()
                    .map(|(market_id, _)| *market_id),
            )
            .chain(
                self.liquidation_escalation
                    .keys()
                    .map(|(market_id, _)| *market_id),
            )
            .collect::<BTreeSet<_>>();
        if market_ids.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market state mismatch",
            ));
        }

        if self
            .market_marks
            .values()
            .chain(self.market_index_prices.values())
            .any(|price| price.price_ticks == 0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot oracle price mismatch",
            ));
        }

        for ((market_id, submitted_at_ms, feeder_id), submission) in &self.oracle_feed_submissions {
            let expected_feed_id = self
                .markets
                .get(market_id)
                .map(|market| market.oracle_feed_id);
            if submission.market_id != *market_id
                || submission.submitted_at_ms != *submitted_at_ms
                || submission.feeder_id != *feeder_id
                || *feeder_id == [0; 32]
                || submission.price_ticks == 0
                || submission.confidence > 100
                || (expected_feed_id.is_some()
                    && Some(submission.oracle_feed_id) != expected_feed_id)
                || (!self.markets.is_empty() && expected_feed_id.is_none())
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot oracle submission mismatch",
                ));
            }
        }

        if self
            .market_fees
            .values()
            .any(|fees| !Self::market_fee_parameters_are_valid(*fees))
            || self.market_risk.values().any(|risk| {
                risk.maintenance_margin_bps > BPS_DENOMINATOR as u32
                    || risk.liquidation_penalty_bps > BPS_DENOMINATOR as u32
                    || risk.liquidator_fee_bps > BPS_DENOMINATOR as u32
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market parameter mismatch",
            ));
        }

        for market in self.markets.values() {
            if market.kind != MarketKind::Perpetual || market.market_id == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market registry mismatch",
                ));
            }
            validate_perpetual_market_config(market.market_id, &market.base_symbol, market.config)
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot market registry mismatch",
                    )
                })?;
        }

        let active_mm_program_count = self
            .market_maker_programs
            .values()
            .filter(|program| program.status == MarketMakerProgramStatus::Active)
            .count();
        if active_mm_program_count > MAX_MM_PROGRAMS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market-maker program cap mismatch",
            ));
        }
        for (program_id, program) in &self.market_maker_programs {
            if program.program_id != *program_id
                || validate_market_maker_program_record(program).is_err()
                || program
                    .markets
                    .iter()
                    .any(|market_id| !self.markets.contains_key(market_id))
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market-maker program mismatch",
                ));
            }
        }
        let mut live_mm_counts = BTreeMap::<MarketMakerIdentity, usize>::new();
        let mut live_market_counts = BTreeMap::<u64, usize>::new();
        let mut live_bond_total = 0_u128;
        for (key, enrollment) in &self.market_maker_enrollments {
            let (market_maker_identity, market_id, program_id) = *key;
            let Some(program) = self.market_maker_programs.get(&program_id) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market-maker enrollment mismatch",
                ));
            };
            if enrollment.market_maker_identity != market_maker_identity
                || enrollment.market_id != market_id
                || enrollment.program_id != program_id
                || !self.persisted_market_maker_identity_is_valid(market_maker_identity)
                || !self.markets.contains_key(&market_id)
                || !program.markets.contains(&market_id)
                || validate_market_maker_enrollment_record(enrollment, program).is_err()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot market-maker enrollment mismatch",
                ));
            }
            if market_maker_enrollment_status_is_live(enrollment.status)
                && matches!(
                    market_maker_identity,
                    MarketMakerIdentity::Delegation { delegation_id, .. }
                        if self.delegations.get(&delegation_id).is_some_and(|delegation| {
                            matches!(
                                delegation.status,
                                DelegationStatus::Closed | DelegationStatus::Terminated
                            )
                        })
                )
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot live market-maker enrollment on terminal delegation",
                ));
            }
            if market_maker_enrollment_status_is_live(enrollment.status) {
                *live_mm_counts.entry(market_maker_identity).or_default() += 1;
                *live_market_counts.entry(market_id).or_default() += 1;
                let bond = enrollment.bond_micro_usdc.as_u128().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot market-maker enrollment mismatch",
                    )
                })?;
                live_bond_total = live_bond_total.checked_add(bond).ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot market-maker bond total mismatch",
                    )
                })?;
            }
        }
        if live_bond_total != self.live_market_maker_bonds_micro_usdc {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market-maker bond total mismatch",
            ));
        }
        if live_mm_counts
            .values()
            .any(|count| *count > MAX_ENROLLMENTS_PER_MARKET_MAKER)
            || live_market_counts
                .values()
                .any(|count| *count > MAX_ENROLLMENTS_PER_MARKET)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market-maker enrollment cap mismatch",
            ));
        }

        // RFC 053 activates via a clean reset. Before deposit/withdraw settle
        // through deterministic market custody, older snapshots may still
        // carry non-zero internal market balances with no custody entry at all.
        // Once a custody address is present in committed policy-USDC state,
        // treat it as authoritative and require an exact match to the computed
        // market-required balance.
        //
        // TODO(RFC 053 slice 2): once collateral deposit/withdraw settle
        // through these custody addresses, also validate markets whose Rust
        // ledgers are non-zero even when the committed custody entry is absent.
        let invalid_market_custody = || {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market custody mismatch",
            )
        };
        let malformed_market_custody = || {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market custody malformed",
            )
        };
        for &market_id in &market_ids {
            let address = isolated_margin_market_custody_address(market_id);
            let Some(bytes) = self.get_committed_state(&policy_usdc_balance_state_key(address))
            else {
                continue;
            };
            let balance = <[u8; 32]>::try_from(bytes.as_slice())
                .map(U256::from_be_bytes)
                .map_err(|_| malformed_market_custody())?;
            if balance.is_zero() {
                return Err(malformed_market_custody());
            }
            let expected = self
                .market_custody_required_balance(market_id)
                .map_err(|_| invalid_market_custody())?;
            let expected = u128::try_from(expected).map_err(|_| invalid_market_custody())?;
            if balance != U256::from(expected) {
                return Err(invalid_market_custody());
            }
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_mandates(&self) -> Result<(), std::io::Error> {
        let mut expected_by_agent = BTreeMap::<u64, BTreeSet<MandateId>>::new();
        let mut non_terminal_by_agent = BTreeMap::<u64, usize>::new();
        let mut active_by_agent = BTreeMap::<u64, usize>::new();

        for (mandate_id, mandate) in &self.mandates {
            validate_mandate_record(mandate).map_err(|reason| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("persisted node snapshot mandate mismatch: {reason}"),
                )
            })?;
            if mandate_id != &mandate.id || mandate.derived_id() != mandate.id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot mandate id mismatch",
                ));
            }
            expected_by_agent
                .entry(mandate.agent)
                .or_default()
                .insert(*mandate_id);
            if !mandate.state.is_terminal() {
                *non_terminal_by_agent.entry(mandate.agent).or_default() += 1;
            }
            if mandate.state == MandateState::Active {
                *active_by_agent.entry(mandate.agent).or_default() += 1;
            }
            for (index, policy_id) in mandate.policies.iter().enumerate() {
                let policy = self.policy_store.get(*policy_id).ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot mandate generated policy missing",
                    )
                })?;
                let expected_policy_state = match mandate.state {
                    MandateState::Draft => PolicyState::PendingActivation,
                    MandateState::Active => match policy.execution_scope {
                        PolicyExecutionScope::Account(_) => PolicyState::Active,
                        PolicyExecutionScope::DelegationTrading { delegation_id } => {
                            let delegation =
                                self.delegations.get(&delegation_id).ok_or_else(|| {
                                    std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "persisted node snapshot delegated policy missing delegation",
                                )
                                })?;
                            match delegation.status {
                                DelegationStatus::Open | DelegationStatus::ClosedToDeposits => {
                                    PolicyState::Active
                                }
                                DelegationStatus::Winding => PolicyState::Suspended,
                                DelegationStatus::Closed | DelegationStatus::Terminated => {
                                    PolicyState::Revoked
                                }
                            }
                        }
                    },
                    MandateState::Suspended => PolicyState::Suspended,
                    MandateState::Breached | MandateState::Revoked => PolicyState::Revoked,
                    MandateState::Expired => PolicyState::Expired,
                };
                if policy.authority_account != mandate.agent
                    || policy.state != expected_policy_state
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot mandate generated policy invalid",
                    ));
                }
                match &policy.origin {
                    PolicyOrigin::MandateGenerated {
                        mandate_id: origin_mandate_id,
                        generated_policy_index,
                        ..
                    } if origin_mandate_id == mandate_id
                        && usize::from(*generated_policy_index) == index => {}
                    _ => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "persisted node snapshot mandate generated policy origin mismatch",
                        ));
                    }
                }
                match policy.execution_scope {
                    PolicyExecutionScope::Account(account_id) => {
                        if account_id != mandate.agent {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "persisted node snapshot mandate generated policy scope mismatch",
                            ));
                        }
                    }
                    PolicyExecutionScope::DelegationTrading { delegation_id } => {
                        let delegation = self.delegations.get(&delegation_id).ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "persisted node snapshot delegated policy missing delegation",
                            )
                        })?;
                        if delegation.mandate_id != *mandate_id || delegation.agent != mandate.agent
                        {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "persisted node snapshot mandate generated policy scope mismatch",
                            ));
                        }
                    }
                }
            }
        }

        if expected_by_agent != self.mandates_by_agent {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot mandate agent index mismatch",
            ));
        }
        if non_terminal_by_agent
            .values()
            .any(|count| *count > MAX_NON_TERMINAL_MANDATES_PER_AGENT)
            || active_by_agent
                .values()
                .any(|count| *count > MAX_ACTIVE_MANDATES_PER_AGENT)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot mandate per-agent cap mismatch",
            ));
        }
        if self
            .mandate_realized_pnl
            .keys()
            .any(|mandate_id| !self.mandates.contains_key(mandate_id))
            || self
                .mandate_daily_pnl_baselines
                .keys()
                .any(|(mandate_id, _)| !self.mandates.contains_key(mandate_id))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot mandate pnl index mismatch",
            ));
        }
        if self
            .account_operators
            .iter()
            .any(|(account_id, operator_id)| *account_id == 0 || *operator_id == 0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot account operator mismatch",
            ));
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_agent_identities(&self) -> Result<(), std::io::Error> {
        let head_height = self.head().header.height;
        for (account, record) in &self.agent_identities {
            if account != &record.account || record.account == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity account mismatch",
                ));
            }
            if self.account_store.get(record.account).is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity account missing",
                ));
            }
            if self.agent_identity_tombstones.contains(account) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity tombstone conflict",
                ));
            }
            if record.display_name.len() > MAX_AGENT_IDENTITY_DISPLAY_NAME_BYTES
                || record.description.len() > MAX_AGENT_IDENTITY_DESCRIPTION_BYTES
                || record
                    .contact_uri
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_AGENT_IDENTITY_CONTACT_URI_BYTES)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity descriptive field too long",
                ));
            }
            if record.operator == Some(0)
                || record
                    .operator
                    .is_some_and(|operator| self.account_store.get(operator).is_none())
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity operator invalid",
                ));
            }
            if record.registered_at_block > head_height
                || record.last_updated_block > head_height
                || record.last_updated_block < record.registered_at_block
                || record.last_updated_timestamp_ms < record.registered_at_timestamp_ms
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity timestamp invalid",
                ));
            }
        }
        if self.agent_identity_tombstones.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot identity tombstone invalid",
            ));
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_agent_attestations(&self) -> Result<(), std::io::Error> {
        let head_height = self.head().header.height;
        let mut expected_by_account = BTreeMap::<u64, BTreeSet<AttestationRecordId>>::new();
        let mut expected_by_vendor_root =
            BTreeMap::<VendorRootReverseIndexKey, VendorRootReverseIndexValue>::new();
        for (record_id, record) in &self.agent_attestations {
            if record_id != &record.record_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot attestation id mismatch",
                ));
            }
            if record.account == 0 || record.attestor == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot attestation account invalid",
                ));
            }
            let subject_known = self.agent_identities.contains_key(&record.account)
                || self.agent_identity_tombstones.contains(&record.account);
            if !subject_known
                || self.account_store.get(record.account).is_none()
                || self.account_store.get(record.attestor).is_none()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot attestation account missing",
                ));
            }
            if record.issued_at_block > head_height
                || record
                    .revoked_at_block
                    .is_some_and(|height| height > head_height || height < record.issued_at_block)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot attestation block invalid",
                ));
            }
            if record.expires_at_timestamp_ms <= record.issued_at_timestamp_ms {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot attestation expiry invalid",
                ));
            }
            match (record.kind, record.evidence) {
                (AttestationKind::OperatorSelf, AttestationEvidence::OperatorSignature) => {}
                (
                    AttestationKind::TeeEnclave,
                    AttestationEvidence::TeeQuoteVerified {
                        vendor,
                        vendor_root_id,
                        verified_at_block,
                        ..
                    },
                ) if vendor_root_id != [0_u8; 32]
                    && verified_at_block <= head_height
                    && self.tee_vendor_roots.get(&vendor).is_some_and(|roots| {
                        roots.iter().any(|root| root.root_id == vendor_root_id)
                    }) => {}
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot attestation evidence invalid",
                    ));
                }
            }
            if let (
                AttestationKind::TeeEnclave,
                AttestationEvidence::TeeQuoteVerified {
                    vendor,
                    vendor_root_id,
                    ..
                },
            ) = (record.kind, record.evidence)
            {
                if record.revoked_at_block.is_none() {
                    expected_by_vendor_root
                        .entry((vendor, vendor_root_id))
                        .or_default()
                        .insert((record.account, record.record_id));
                }
            }
            expected_by_account
                .entry(record.account)
                .or_default()
                .insert(record.record_id);
        }
        if expected_by_account != self.agent_attestations_by_account {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot attestation account index mismatch",
            ));
        }
        if expected_by_vendor_root != self.tee_attestations_by_vendor_root {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot attestation vendor-root index mismatch",
            ));
        }
        let timestamp_ms = self.head().header.timestamp_ms;
        for (account, identity) in &self.agent_identities {
            let expected_tier =
                if self.has_matching_tee_attestation(*account, timestamp_ms, head_height) {
                    AgentIdentityTier::Tier2TeeAttested
                } else if self.has_matching_operator_attestation(*account, None, timestamp_ms) {
                    AgentIdentityTier::Tier1OperatorAttested
                } else {
                    AgentIdentityTier::Tier0Unattested
                };
            if identity.tier != expected_tier {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity tier mismatch",
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_tee_vendor_state(&self) -> Result<(), std::io::Error> {
        if self.tee_vendor_cap_profiles.len() != TeeVendor::all().len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot tee cap profile set mismatch",
            ));
        }
        for vendor in TeeVendor::all() {
            let Some(profile) = self.tee_vendor_cap_profiles.get(&vendor) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot tee cap profile missing",
                ));
            };
            if profile.vendor != vendor || profile.cap_multiplier_microunits == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot tee cap profile invalid",
                ));
            }
            let launch_profile = launch_vendor_cap_profile(vendor);
            if profile.last_updated_governance_action == [0_u8; 32]
                && (profile.cap_multiplier_microunits != launch_profile.cap_multiplier_microunits
                    || profile.composition_eligibility != launch_profile.composition_eligibility)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot tee cap profile governance action missing",
                ));
            }
        }

        let head_height = self.head().header.height;
        let mut scheduled_cap_profile_vendors = BTreeSet::new();
        for (action_id, update) in &self.scheduled_tee_vendor_cap_profile_updates {
            let Some(current_profile) = self.tee_vendor_cap_profiles.get(&update.vendor) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot tee scheduled cap profile update vendor missing",
                ));
            };
            if action_id != &update.governance_action
                || update.governance_action == [0_u8; 32]
                || update.cap_multiplier_microunits == 0
                || update.valid_from_block <= head_height
                || !scheduled_cap_profile_vendors.insert(update.vendor)
                || (current_profile.cap_multiplier_microunits == update.cap_multiplier_microunits
                    && current_profile.composition_eligibility == update.composition_eligibility)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot tee scheduled cap profile update invalid",
                ));
            }
        }

        let mut root_ids = BTreeSet::new();
        for (vendor, roots) in &self.tee_vendor_roots {
            for root in roots {
                if root.vendor != *vendor || root.root_id == [0_u8; 32] {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot tee vendor root mismatch",
                    ));
                }
                if !root_ids.insert(root.root_id) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot tee vendor root duplicate",
                    ));
                }
                if root
                    .valid_until_block
                    .is_some_and(|valid_until| valid_until <= root.valid_from_block)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot tee vendor root block invalid",
                    ));
                }
                if !vendor_root_payload_matches_vendor(&root.payload, root.vendor)
                    || !vendor_root_payload_within_bounds(&root.payload)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot tee vendor root payload invalid",
                    ));
                }
            }
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_real_capital_listings(&self) -> Result<(), std::io::Error> {
        let invalid = |reason| std::io::Error::new(std::io::ErrorKind::InvalidData, reason);
        let head_height = self.head().header.height;
        let mut expected_by_agent = BTreeMap::<u64, BTreeSet<VaultId>>::new();
        for (vault_id, listing) in &self.real_capital_listings {
            if vault_id != &listing.vault_id {
                return Err(invalid("persisted node snapshot vault listing id mismatch"));
            }
            self.validate_real_capital_listing(listing)
                .map_err(|_| invalid("persisted node snapshot vault listing invalid"))?;
            if listing.listed_at_block > head_height
                || listing.last_updated_block > head_height
                || listing.last_updated_block < listing.listed_at_block
            {
                return Err(invalid(
                    "persisted node snapshot vault listing block invalid",
                ));
            }
            expected_by_agent
                .entry(listing.serving_agent)
                .or_default()
                .insert(listing.vault_id);
        }
        if expected_by_agent != self.real_capital_listings_by_agent {
            return Err(invalid(
                "persisted node snapshot vault listing agent index mismatch",
            ));
        }

        let mut expected_vault_deposited = BTreeMap::<VaultId, u128>::new();
        for ((vault_id, principal), deposited) in &self.vault_principal_deposited_micro_usdc {
            if *principal == 0 || *deposited == 0 {
                return Err(invalid(
                    "persisted node snapshot vault principal deposit counter invalid",
                ));
            }
            if !self.real_capital_listings.contains_key(vault_id) {
                return Err(invalid(
                    "persisted node snapshot vault principal deposit counter unknown vault",
                ));
            }
            let entry = expected_vault_deposited.entry(*vault_id).or_default();
            *entry = entry
                .checked_add(*deposited)
                .ok_or_else(|| invalid("persisted node snapshot vault deposit counter overflow"))?;
        }
        for (vault_id, deposited) in &self.vault_deposited_micro_usdc {
            if *deposited == 0 || !self.real_capital_listings.contains_key(vault_id) {
                return Err(invalid(
                    "persisted node snapshot vault deposit counter invalid",
                ));
            }
        }
        if expected_vault_deposited != self.vault_deposited_micro_usdc {
            return Err(invalid(
                "persisted node snapshot vault deposit counter mismatch",
            ));
        }
        let expected_bond_escrow = self
            .delegations
            .iter()
            .filter_map(|(delegation_id, delegation)| {
                (matches!(
                    delegation.status,
                    DelegationStatus::Open
                        | DelegationStatus::ClosedToDeposits
                        | DelegationStatus::Winding
                ) && delegation.agent_bond > 0)
                    .then_some((*delegation_id, delegation.agent_bond))
            })
            .collect::<BTreeMap<_, _>>();
        if expected_bond_escrow != self.real_capital_bond_escrow_micro_usdc {
            return Err(invalid(
                "persisted node snapshot real-capital bond escrow mismatch",
            ));
        }
        for delegation_id in self.delegations.keys() {
            let address = real_capital_bond_escrow_address(delegation_id);
            let balance = self
                .get_committed_state(&policy_usdc_balance_state_key(address))
                .map(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).map(U256::from_be_bytes))
                .transpose()
                .map_err(|_| invalid("persisted node snapshot real-capital bond escrow malformed"))?
                .unwrap_or(U256::ZERO);
            let expected = expected_bond_escrow
                .get(delegation_id)
                .copied()
                .unwrap_or(0);
            if balance != U256::from(expected) {
                return Err(invalid(
                    "persisted node snapshot real-capital bond escrow mismatch",
                ));
            }
        }
        let mut _payout_weights_by_delegation = BTreeMap::<DelegationId, u128>::new();
        for ((delegation_id, principal), weight) in &self.delegation_termination_payout_weights {
            let delegation = self.delegations.get(delegation_id).ok_or_else(|| {
                invalid("persisted node snapshot delegation payout weights unknown delegation")
            })?;
            if *principal == 0 || *weight == 0 || delegation.status != DelegationStatus::Terminated
            {
                return Err(invalid(
                    "persisted node snapshot delegation payout weights invalid",
                ));
            }
            if let Some(share) = self.delegation_shares.get(&(*delegation_id, *principal)) {
                if share.shares > *weight {
                    return Err(invalid(
                        "persisted node snapshot delegation payout weights mismatch",
                    ));
                }
            }
            let total = _payout_weights_by_delegation
                .entry(*delegation_id)
                .or_default();
            *total = total.checked_add(*weight).ok_or_else(|| {
                invalid("persisted node snapshot delegation payout weights overflow")
            })?;
        }
        for ((delegation_id, principal), share) in &self.delegation_shares {
            let Some(delegation) = self.delegations.get(delegation_id) else {
                continue;
            };
            if delegation.status != DelegationStatus::Terminated || share.shares == 0 {
                continue;
            }
            let Some(weight) = self
                .delegation_termination_payout_weights
                .get(&(*delegation_id, *principal))
            else {
                return Err(invalid(
                    "persisted node snapshot delegation payout weights missing",
                ));
            };
            if *weight < share.shares {
                return Err(invalid(
                    "persisted node snapshot delegation payout weights mismatch",
                ));
            }
        }
        let mut compensation_by_delegation = BTreeMap::<DelegationId, u128>::new();
        let mut total_compensation_claims = 0_u128;
        for ((delegation_id, principal), claim) in &self.delegation_compensation_claims {
            let delegation = self.delegations.get(delegation_id).ok_or_else(|| {
                invalid("persisted node snapshot delegation compensation unknown delegation")
            })?;
            if *principal == 0
                || *claim == 0
                || delegation.status != DelegationStatus::Terminated
                || !self
                    .delegation_shares
                    .contains_key(&(*delegation_id, *principal))
            {
                return Err(invalid(
                    "persisted node snapshot delegation compensation invalid",
                ));
            }
            if !self
                .delegation_termination_payout_weights
                .contains_key(&(*delegation_id, *principal))
            {
                return Err(invalid(
                    "persisted node snapshot delegation compensation payout weight missing",
                ));
            }
            let total = compensation_by_delegation
                .entry(*delegation_id)
                .or_default();
            *total = total.checked_add(*claim).ok_or_else(|| {
                invalid("persisted node snapshot delegation compensation overflow")
            })?;
            total_compensation_claims =
                total_compensation_claims
                    .checked_add(*claim)
                    .ok_or_else(|| {
                        invalid("persisted node snapshot delegation compensation overflow")
                    })?;
        }
        for (delegation_id, total) in compensation_by_delegation {
            let delegation = self.delegations.get(&delegation_id).ok_or_else(|| {
                invalid("persisted node snapshot delegation compensation unknown delegation")
            })?;
            if total > delegation.agent_bond {
                return Err(invalid(
                    "persisted node snapshot delegation compensation exceeds bond",
                ));
            }
        }
        if total_compensation_claims != self.real_capital_compensation_escrow_micro_usdc {
            return Err(invalid(
                "persisted node snapshot real-capital compensation escrow mismatch",
            ));
        }
        let compensation_escrow_balance = self
            .get_committed_state(&policy_usdc_balance_state_key(
                real_capital_compensation_escrow_address(),
            ))
            .map(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).map(U256::from_be_bytes))
            .transpose()
            .map_err(|_| {
                invalid("persisted node snapshot real-capital compensation escrow malformed")
            })?
            .unwrap_or(U256::ZERO);
        if compensation_escrow_balance
            != U256::from(self.real_capital_compensation_escrow_micro_usdc)
        {
            return Err(invalid(
                "persisted node snapshot real-capital compensation escrow mismatch",
            ));
        }
        // The insurance fund is gross append-only history. The wallet ledger
        // may fall below it once an explicitly modeled spend path lands, but
        // it must never exceed the gross amount assigned to insurance.
        if self.real_capital_insurance_wallet_micro_usdc
            > self.real_capital_insurance_fund_micro_usdc
        {
            return Err(invalid(
                "persisted node snapshot real-capital insurance wallet mismatch",
            ));
        }
        let insurance_wallet_balance = self
            .get_committed_state(&policy_usdc_balance_state_key(
                real_capital_insurance_wallet_address(),
            ))
            .map(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).map(U256::from_be_bytes))
            .transpose()
            .map_err(|_| {
                invalid("persisted node snapshot real-capital insurance wallet malformed")
            })?
            .unwrap_or(U256::ZERO);
        if insurance_wallet_balance != U256::from(self.real_capital_insurance_wallet_micro_usdc) {
            return Err(invalid(
                "persisted node snapshot real-capital insurance wallet mismatch",
            ));
        }

        if let Some(protocol_cap) = self.protocol_delegation_cap {
            if protocol_cap.total_protocol_tvl_micro_usdc == 0
                || protocol_cap.last_updated_governance_action == [0_u8; 32]
            {
                return Err(invalid(
                    "persisted node snapshot protocol delegation cap invalid",
                ));
            }
        }
        if self.listing_control_state.last_updated_block > head_height {
            return Err(invalid(
                "persisted node snapshot listing control block invalid",
            ));
        }
        if (self.listing_control_state.global_deposit_kill_switch
            || self.listing_control_state.last_updated_block != 0)
            && self.listing_control_state.last_updated_governance_action == [0_u8; 32]
        {
            return Err(invalid(
                "persisted node snapshot listing control governance action missing",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_persisted_identity_action_nonces(&self) -> Result<(), std::io::Error> {
        for account in self.identity_action_nonces.keys() {
            if *account == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity action nonce uses zero account",
                ));
            }
            if self.account_store.get(*account).is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity action nonce references unknown account",
                ));
            }
        }
        for account in self.identity_attestor_nonces.keys() {
            if *account == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity attestor nonce uses zero account",
                ));
            }
            if self.account_store.get(*account).is_none() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot identity attestor nonce references unknown account",
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn validate_persisted_risk_state(&self) -> Result<(), std::io::Error> {
        let mut risk_keys = BTreeSet::new();
        risk_keys.extend(self.positions.keys().copied());
        risk_keys.extend(self.isolated_collateral.keys().copied());
        risk_keys.extend(self.account_market_bad_debt.keys().copied());
        risk_keys.extend(self.under_maintenance_accounts.iter().copied());
        risk_keys.extend(self.liquidation_escalation.keys().copied());

        for (market_id, account_id) in &risk_keys {
            let tracked_under_maintenance = self
                .under_maintenance_accounts
                .contains(&(*market_id, *account_id));
            if tracked_under_maintenance
                != self
                    .margin_snapshot(*market_id, *account_id)
                    .under_maintenance
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot under-maintenance mismatch",
                ));
            }
        }

        let mut expected_market_bad_debt = BTreeMap::new();
        for ((market_id, _), amount) in &self.account_market_bad_debt {
            if *amount > 0 {
                let entry = expected_market_bad_debt.entry(*market_id).or_insert(0_u128);
                *entry = entry.saturating_add(*amount);
            }
        }
        let recorded_market_bad_debt = self
            .market_bad_debt_balances
            .iter()
            .filter_map(|(market_id, amount)| (*amount > 0).then_some((*market_id, *amount)))
            .collect::<BTreeMap<_, _>>();
        if recorded_market_bad_debt != expected_market_bad_debt {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot market bad debt mismatch",
            ));
        }

        for (&(market_id, account_id), state) in &self.liquidation_escalation {
            let margin = self.margin_snapshot(market_id, account_id);
            if self.account_frozen(account_id)
                || !margin.under_maintenance
                || state.consecutive_non_improving_still_under == 0
                || state.consecutive_non_improving_still_under >= LIQUIDATION_STALL_FREEZE_THRESHOLD
                || state.last_margin_surplus != margin.margin_surplus
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot liquidation escalation mismatch",
                ));
            }
        }

        let mut live_delegations_by_mandate = BTreeMap::<MandateId, usize>::new();
        for (delegation_id, delegation) in &self.delegations {
            let derived = derive_delegation_id(
                delegation.agent,
                &delegation.mandate_id,
                delegation.opened_at_ms,
            );
            let mandate = self.mandates.get(&delegation.mandate_id).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation mandate mismatch",
                )
            })?;
            if delegation.delegation_id != *delegation_id
                || derived != *delegation_id
                || mandate.agent != delegation.agent
                || delegation.last_updated_at_ms < delegation.opened_at_ms
                || !match (delegation.status, delegation.winding_started_at_ms) {
                    (DelegationStatus::Winding, Some(started_at)) => {
                        started_at >= delegation.opened_at_ms
                            && started_at <= delegation.last_updated_at_ms
                    }
                    (DelegationStatus::Winding, None) => false,
                    (_, Some(_)) => false,
                    (_, None) => true,
                }
                || validate_open_delegation_config(
                    delegation.agent,
                    &delegation.mandate_id,
                    delegation.performance_share_units,
                    delegation.crystallization_cadence_ms,
                    delegation.notice_period_ms,
                    delegation.agent_bond,
                )
                .is_err()
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation mismatch",
                ));
            }
            if matches!(
                delegation.status,
                DelegationStatus::Open
                    | DelegationStatus::ClosedToDeposits
                    | DelegationStatus::Winding
            ) {
                *live_delegations_by_mandate
                    .entry(delegation.mandate_id)
                    .or_default() += 1;
            }
        }
        if live_delegations_by_mandate.values().any(|count| *count > 1) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted node snapshot multiple live delegations for mandate",
            ));
        }
        let mut delegation_share_totals = BTreeMap::<DelegationId, u128>::new();
        for ((delegation_id, principal), share) in &self.delegation_shares {
            if !self.delegations.contains_key(delegation_id)
                || *principal == 0
                || share.shares == 0
                || share.locked_for_redemption > share.shares
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation share mismatch",
                ));
            }
            let total = delegation_share_totals.entry(*delegation_id).or_default();
            *total = total.checked_add(share.shares).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation share mismatch",
                )
            })?;
        }
        for (delegation_id, delegation) in &self.delegations {
            let share_total = delegation_share_totals
                .get(delegation_id)
                .copied()
                .unwrap_or_default();
            if share_total != delegation.total_shares {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot delegation share mismatch",
                ));
            }
        }
        let mut active_redemption_locks = BTreeMap::<(DelegationId, u64), u128>::new();
        for (request_id, request) in &self.redemption_requests {
            if request.request_id != *request_id
                || !self.delegations.contains_key(&request.delegation_id)
                || request.principal == 0
                || request.shares_requested == 0
                || request.ready_at_ms < request.requested_at_ms
                || request.nav_at_request == 0
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot redemption request mismatch",
                ));
            }
            if matches!(
                request.status,
                RedemptionRequestStatus::Pending | RedemptionRequestStatus::Ready
            ) {
                let key = (request.delegation_id, request.principal);
                let total = active_redemption_locks.entry(key).or_default();
                *total = total.checked_add(request.shares_requested).ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "persisted node snapshot redemption request mismatch",
                    )
                })?;
            }
        }
        for ((delegation_id, principal), share) in &self.delegation_shares {
            let locked = active_redemption_locks
                .get(&(*delegation_id, *principal))
                .copied()
                .unwrap_or_default();
            if share.locked_for_redemption != locked {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot redemption request mismatch",
                ));
            }
        }
        for key in active_redemption_locks.keys() {
            if !self.delegation_shares.contains_key(key) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot redemption request mismatch",
                ));
            }
        }

        for &account_id in &self.frozen_accounts {
            let has_bad_debt =
                self.account_market_bad_debt
                    .iter()
                    .any(|((_, entry_account_id), amount)| {
                        *entry_account_id == account_id && *amount > 0
                    });
            let has_under_maintenance = self
                .under_maintenance_accounts
                .iter()
                .any(|(_, entry_account_id)| *entry_account_id == account_id);
            if !has_bad_debt && !has_under_maintenance {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted node snapshot frozen account mismatch",
                ));
            }
        }

        Ok(())
    }
}
