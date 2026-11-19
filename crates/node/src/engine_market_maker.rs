use super::*;

impl SingleNodeEngine {
    pub fn market_maker_program(
        &self,
        program_id: &MarketMakerProgramId,
    ) -> Option<&MarketMakerProgram> {
        self.market_maker_programs.get(program_id)
    }

    pub fn market_maker_programs(&self) -> impl Iterator<Item = &MarketMakerProgram> {
        self.market_maker_programs.values()
    }

    pub fn market_maker_enrollment(
        &self,
        key: &MarketMakerEnrollmentKey,
    ) -> Option<&MarketMakerEnrollment> {
        self.market_maker_enrollments.get(key)
    }

    pub fn market_maker_enrollments(&self) -> impl Iterator<Item = &MarketMakerEnrollment> {
        self.market_maker_enrollments.values()
    }

    pub fn live_market_maker_bonds_micro_usdc(&self) -> u128 {
        self.live_market_maker_bonds_micro_usdc
    }

    pub fn define_market_maker_program(
        &mut self,
        input: DefineMarketMakerProgramInput,
        timestamp_ms: u64,
    ) -> Result<MarketMakerProgram, NodeError> {
        validate_market_maker_program_input(&input, &self.markets)?;
        let program_id = derive_market_maker_program_id(&input);
        let previous_status = self
            .market_maker_programs
            .get(&program_id)
            .map(|program| program.status);
        if matches!(previous_status, Some(MarketMakerProgramStatus::Active)) {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "program_already_active",
            });
        }
        let active_program_count = self
            .market_maker_programs
            .values()
            .filter(|program| program.status == MarketMakerProgramStatus::Active)
            .count();
        if active_program_count >= MAX_MM_PROGRAMS {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "too_many_mm_programs",
            });
        }

        let defined_at_ms = self
            .market_maker_programs
            .get(&program_id)
            .map(|program| program.defined_at_ms)
            .unwrap_or(timestamp_ms);
        let program = MarketMakerProgram {
            program_id,
            name: input.name,
            markets: input.markets,
            defined_at_ms,
            last_updated_at_ms: timestamp_ms,
            status: MarketMakerProgramStatus::Active,
            min_uptime_bps: input.min_uptime_bps,
            max_spread_bps: input.max_spread_bps,
            min_quote_size_micro_usdc: input.min_quote_size_micro_usdc,
            min_two_sided_windows_per_hour: input.min_two_sided_windows_per_hour,
            rebate_multiplier_units: input.rebate_multiplier_units,
            enrollment_bond_micro_usdc: input.enrollment_bond_micro_usdc,
            compliance_window_ms: input.compliance_window_ms,
            observation_cadence_ms: input.observation_cadence_ms,
        };
        self.market_maker_programs
            .insert(program_id, program.clone());
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MarketMakerProgramDefined {
                program_id,
                name: program.name.clone(),
                markets: program.markets.clone(),
                rebate_multiplier_units: program.rebate_multiplier_units,
                compliance_window_ms: program.compliance_window_ms,
                reactivated: matches!(previous_status, Some(MarketMakerProgramStatus::Retired)),
            },
        );
        Ok(program)
    }

    pub fn retire_market_maker_program(
        &mut self,
        program_id: MarketMakerProgramId,
        timestamp_ms: u64,
    ) -> Result<MarketMakerProgram, NodeError> {
        let program = self.market_maker_programs.get_mut(&program_id).ok_or(
            NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "unknown_program",
            },
        )?;
        if program.status != MarketMakerProgramStatus::Active {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "program_not_active",
            });
        }
        program.status = MarketMakerProgramStatus::Retired;
        program.last_updated_at_ms = timestamp_ms;
        let retired = program.clone();
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MarketMakerProgramRetired { program_id },
        );
        Ok(retired)
    }

    pub fn enroll_market_maker(
        &mut self,
        input: EnrollMarketMakerInput,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<MarketMakerEnrollment, NodeError> {
        self.validate_market_maker_enrollment_input(&input)?;
        let bond = input
            .bond_micro_usdc
            .as_u128()
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "bond_overflow",
            })?;
        let address = self.account_policy_address_for_market_maker_identity(
            input.market_maker_identity,
            input.program_id,
            "market_maker_evm_address_missing",
        )?;
        let balance = self.policy_usdc_balance_for_market_maker(address, input.program_id)?;
        if balance < U256::from(bond) {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "insufficient_market_maker_policy_usdc_bond",
            });
        }
        let new_bond_total = self
            .live_market_maker_bonds_micro_usdc
            .checked_add(bond)
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "market_maker_bond_total_overflow",
            })?;
        let new_balance = balance - U256::from(bond);
        let mut sets = Vec::new();
        let mut deletes = Vec::new();
        if new_balance.is_zero() {
            deletes.push(policy_usdc_balance_state_key(address));
        } else {
            sets.push((
                policy_usdc_balance_state_key(address),
                new_balance.to_be_bytes::<32>().to_vec(),
            ));
        }
        // This method must be called through BlockItem::MarketMakerProgram in multi-validator mode;
        // direct callers would mutate only the local committed state root.
        self.apply_committed_state_updates(sets, deletes)
            .map_err(NodeError::Execution)?;
        self.live_market_maker_bonds_micro_usdc = new_bond_total;

        let enrollment = MarketMakerEnrollment {
            market_maker_identity: input.market_maker_identity,
            market_id: input.market_id,
            program_id: input.program_id,
            enrolled_at_ms: timestamp_ms,
            last_updated_at_ms: timestamp_ms,
            status: MarketMakerEnrollmentStatus::Active,
            bond_micro_usdc: input.bond_micro_usdc,
            current_compliance: MarketMakerComplianceState {
                window_start_ms: timestamp_ms,
                window_end_ms: timestamp_ms,
                ..MarketMakerComplianceState::default()
            },
            observation_history: Vec::new(),
            last_compliance_checked_ms: 0,
            lifetime_compliant_rebates_micro_usdc: MarketMakerU256::default(),
            lifetime_forfeited_rebates_micro_usdc: MarketMakerU256::default(),
        };
        self.market_maker_enrollments.insert(
            market_maker_enrollment_key(
                input.market_maker_identity,
                input.market_id,
                input.program_id,
            ),
            enrollment.clone(),
        );
        self.record_runtime_event(
            event_block_height,
            timestamp_ms,
            RuntimeEventKind::MarketMakerEnrolled {
                market_maker_identity: input.market_maker_identity,
                market_id: input.market_id,
                program_id: input.program_id,
                bond_micro_usdc: bond,
            },
        );
        Ok(enrollment)
    }

    pub fn exit_market_maker_enrollment(
        &mut self,
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: MarketMakerProgramId,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<MarketMakerEnrollment, NodeError> {
        let key = market_maker_enrollment_key(market_maker_identity, market_id, program_id);
        let existing = self
            .market_maker_enrollments
            .get(&key)
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "unknown_enrollment",
            })?
            .clone();
        if !market_maker_enrollment_status_is_live(existing.status) {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "enrollment_not_live",
            });
        }
        let bond =
            existing
                .bond_micro_usdc
                .as_u128()
                .ok_or(NodeError::InvalidMarketMakerProgram {
                    program_id,
                    reason: "bond_overflow",
                })?;
        let address = self.account_policy_address_for_market_maker_identity(
            market_maker_identity,
            program_id,
            "market_maker_evm_address_missing",
        )?;
        let balance = self.policy_usdc_balance_for_market_maker(address, program_id)?;
        let new_balance =
            balance
                .checked_add(U256::from(bond))
                .ok_or(NodeError::InvalidMarketMakerProgram {
                    program_id,
                    reason: "market_maker_bond_refund_overflow",
                })?;
        let new_bond_total = self
            .live_market_maker_bonds_micro_usdc
            .checked_sub(bond)
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id,
                reason: "market_maker_bond_total_underflow",
            })?;
        // This method must be called through BlockItem::MarketMakerProgram in multi-validator mode;
        // direct callers would mutate only the local committed state root.
        self.apply_committed_state_updates(
            [(
                policy_usdc_balance_state_key(address),
                new_balance.to_be_bytes::<32>().to_vec(),
            )],
            std::iter::empty(),
        )
        .map_err(NodeError::Execution)?;
        self.live_market_maker_bonds_micro_usdc = new_bond_total;

        let mut exited = existing;
        exited.status = MarketMakerEnrollmentStatus::Exited;
        exited.last_updated_at_ms = timestamp_ms;
        self.market_maker_enrollments.insert(key, exited.clone());
        self.record_runtime_event(
            event_block_height,
            timestamp_ms,
            RuntimeEventKind::MarketMakerEnrollmentExited {
                market_maker_identity,
                market_id,
                program_id,
                bond_refunded_micro_usdc: bond,
            },
        );
        Ok(exited)
    }

    pub(super) fn validate_market_maker_enrollment_input(
        &self,
        input: &EnrollMarketMakerInput,
    ) -> Result<(), NodeError> {
        self.validate_market_maker_identity_for_enrollment(
            input.market_maker_identity,
            input.program_id,
        )?;
        let program = self.market_maker_programs.get(&input.program_id).ok_or(
            NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "unknown_program",
            },
        )?;
        if program.status != MarketMakerProgramStatus::Active {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "program_not_active",
            });
        }
        if !program.markets.contains(&input.market_id) {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "market_not_in_program",
            });
        }
        let market =
            self.markets
                .get(&input.market_id)
                .ok_or(NodeError::InvalidMarketMakerProgram {
                    program_id: input.program_id,
                    reason: "unknown_market",
                })?;
        if market.status == MarketStatus::Delisted {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "market_not_tradable",
            });
        }
        let bond = input
            .bond_micro_usdc
            .as_u128()
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "bond_overflow",
            })?;
        let program_bond = program.enrollment_bond_micro_usdc.as_u128().ok_or(
            NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "program_bond_overflow",
            },
        )?;
        if bond < program_bond {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "insufficient_enrollment_bond",
            });
        }
        let key = market_maker_enrollment_key(
            input.market_maker_identity,
            input.market_id,
            input.program_id,
        );
        if self
            .market_maker_enrollments
            .get(&key)
            .is_some_and(|enrollment| market_maker_enrollment_status_is_live(enrollment.status))
        {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "enrollment_already_live",
            });
        }
        let live_for_market_maker = self
            .market_maker_enrollments
            .values()
            .filter(|enrollment| {
                enrollment.market_maker_identity == input.market_maker_identity
                    && market_maker_enrollment_status_is_live(enrollment.status)
            })
            .count();
        if live_for_market_maker >= MAX_ENROLLMENTS_PER_MARKET_MAKER {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "too_many_market_maker_enrollments",
            });
        }
        let live_for_market = self
            .market_maker_enrollments
            .values()
            .filter(|enrollment| {
                enrollment.market_id == input.market_id
                    && market_maker_enrollment_status_is_live(enrollment.status)
            })
            .count();
        if live_for_market >= MAX_ENROLLMENTS_PER_MARKET {
            return Err(NodeError::InvalidMarketMakerProgram {
                program_id: input.program_id,
                reason: "too_many_market_enrollments",
            });
        }
        Ok(())
    }

    pub(super) fn validate_market_maker_identity_for_enrollment(
        &self,
        identity: MarketMakerIdentity,
        program_id: MarketMakerProgramId,
    ) -> Result<u64, NodeError> {
        match identity {
            MarketMakerIdentity::Account(account_id) => {
                if self.account_store.get(account_id).is_none() {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "unknown_market_maker",
                    });
                }
                Ok(account_id)
            }
            MarketMakerIdentity::Delegation {
                delegation_id,
                serving_agent,
            } => {
                if self.account_store.get(serving_agent).is_none() {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "unknown_market_maker_serving_agent",
                    });
                }
                let delegation = self.delegations.get(&delegation_id).ok_or(
                    NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "unknown_market_maker_delegation",
                    },
                )?;
                if delegation.agent != serving_agent {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "delegation_market_maker_serving_agent_mismatch",
                    });
                }
                if !matches!(
                    delegation.status,
                    DelegationStatus::Open | DelegationStatus::ClosedToDeposits
                ) {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "delegation_market_maker_not_enrollable",
                    });
                }
                let mandate = self.mandates.get(&delegation.mandate_id).ok_or(
                    NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "delegation_market_maker_mandate_missing",
                    },
                )?;
                if mandate.state != MandateState::Active {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "delegation_market_maker_mandate_not_active",
                    });
                }
                // RFC 054 also reserves a `NoMarketMakerProgram` mandate gate.
                // The current mandate schema has no such constraint variant yet,
                // so delegated MM admission can only enforce the existing live
                // delegation/mandate/serving-agent boundary in this slice.
                Ok(serving_agent)
            }
        }
    }

    pub(super) fn account_policy_address_for_market_maker_identity(
        &self,
        identity: MarketMakerIdentity,
        program_id: MarketMakerProgramId,
        reason: &'static str,
    ) -> Result<[u8; 20], NodeError> {
        let account_id = match identity {
            MarketMakerIdentity::Account(account_id) => {
                if self.account_store.get(account_id).is_none() {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "unknown_market_maker",
                    });
                }
                account_id
            }
            MarketMakerIdentity::Delegation {
                delegation_id,
                serving_agent,
            } => {
                let delegation = self.delegations.get(&delegation_id).ok_or(
                    NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "unknown_market_maker_delegation",
                    },
                )?;
                if delegation.agent != serving_agent {
                    return Err(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "delegation_market_maker_serving_agent_mismatch",
                    });
                }
                serving_agent
            }
        };
        self.get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .ok_or(NodeError::InvalidMarketMakerProgram { program_id, reason })
    }

    pub(super) fn policy_usdc_balance_for_market_maker(
        &self,
        address: [u8; 20],
        program_id: MarketMakerProgramId,
    ) -> Result<U256, NodeError> {
        self.get_committed_state(&policy_usdc_balance_state_key(address))
            .map(|bytes| {
                <[u8; 32]>::try_from(bytes.as_slice())
                    .map(U256::from_be_bytes)
                    .map_err(|_| NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "policy_usdc_balance_malformed",
                    })
            })
            .transpose()
            .map(|balance| balance.unwrap_or(U256::ZERO))
    }

    pub(super) fn live_delegation_market_maker_enrollment_keys(
        &self,
        delegation_id: &DelegationId,
    ) -> Vec<MarketMakerEnrollmentKey> {
        self.market_maker_enrollments
            .iter()
            .filter_map(|(key, enrollment)| {
                let (identity, _, _) = *key;
                match identity {
                    MarketMakerIdentity::Delegation {
                        delegation_id: enrolled_delegation_id,
                        ..
                    } if enrolled_delegation_id == *delegation_id
                        && market_maker_enrollment_status_is_live(enrollment.status) =>
                    {
                        Some(*key)
                    }
                    _ => None,
                }
            })
            .collect()
    }

    pub(super) fn preflight_live_delegation_market_maker_exits(
        &self,
        delegation_id: &DelegationId,
    ) -> Result<Vec<MarketMakerEnrollmentKey>, NodeError> {
        let keys = self.live_delegation_market_maker_enrollment_keys(delegation_id);
        let mut projected_balances = BTreeMap::<[u8; 20], U256>::new();
        let mut total_bond = 0_u128;
        for key in &keys {
            let (identity, _market_id, program_id) = *key;
            let enrollment = self.market_maker_enrollments.get(key).ok_or(
                NodeError::InvalidMarketMakerProgram {
                    program_id,
                    reason: "unknown_enrollment",
                },
            )?;
            let bond = enrollment.bond_micro_usdc.as_u128().ok_or(
                NodeError::InvalidMarketMakerProgram {
                    program_id,
                    reason: "bond_overflow",
                },
            )?;
            total_bond =
                total_bond
                    .checked_add(bond)
                    .ok_or(NodeError::InvalidMarketMakerProgram {
                        program_id,
                        reason: "market_maker_bond_total_overflow",
                    })?;
            let address = self.account_policy_address_for_market_maker_identity(
                identity,
                program_id,
                "market_maker_evm_address_missing",
            )?;
            let current = projected_balances
                .get(&address)
                .copied()
                .unwrap_or(self.policy_usdc_balance_for_market_maker(address, program_id)?);
            let next = current.checked_add(U256::from(bond)).ok_or(
                NodeError::InvalidMarketMakerProgram {
                    program_id,
                    reason: "market_maker_bond_refund_overflow",
                },
            )?;
            projected_balances.insert(address, next);
        }
        self.live_market_maker_bonds_micro_usdc
            .checked_sub(total_bond)
            .ok_or(NodeError::InvalidMarketMakerProgram {
                program_id: [0_u8; 32],
                reason: "market_maker_bond_total_underflow",
            })?;
        Ok(keys)
    }

    pub(super) fn exit_preflighted_live_delegation_market_maker_enrollments(
        &mut self,
        keys: Vec<MarketMakerEnrollmentKey>,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) {
        for (identity, market_id, program_id) in keys {
            self.exit_market_maker_enrollment(
                identity,
                market_id,
                program_id,
                timestamp_ms,
                event_block_height,
            )
            .expect("preflighted delegation-backed MM exit should succeed");
        }
    }

    pub(super) fn persisted_market_maker_identity_is_valid(
        &self,
        identity: MarketMakerIdentity,
    ) -> bool {
        match identity {
            MarketMakerIdentity::Account(account_id) => {
                self.account_store.get(account_id).is_some()
            }
            MarketMakerIdentity::Delegation {
                delegation_id,
                serving_agent,
            } => {
                self.account_store.get(serving_agent).is_some()
                    && self
                        .delegations
                        .get(&delegation_id)
                        .is_some_and(|delegation| delegation.agent == serving_agent)
            }
        }
    }

    pub(super) fn apply_block_market_maker_program_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::MarketMakerProgram(program) = item else {
                continue;
            };
            let start = self.runtime_events.len();
            let fingerprint = market_maker_program_action_fingerprint(&program.action);
            if let Some(processed) = self
                .processed_market_maker_program_actions
                .get(&program.action_id)
                .cloned()
            {
                if processed.fingerprint != fingerprint {
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::MarketMakerProgramActionRejected {
                            action_kind: market_maker_program_block_action_kind(&program.action)
                                .to_string(),
                            reason: "market_maker_program_action_id_mismatch".to_string(),
                        },
                    );
                }
                continue;
            }
            let result = self.apply_block_market_maker_program_action(
                program,
                program.timestamp_ms,
                Some(block.header.height),
            );
            match result {
                Ok(receipt) => {
                    self.processed_market_maker_program_actions.insert(
                        program.action_id,
                        ProcessedMarketMakerProgramAction {
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt,
                        },
                    );
                }
                Err(error) => {
                    let reason = market_maker_program_block_error_reason(&error).to_string();
                    let action_kind =
                        market_maker_program_block_action_kind(&program.action).to_string();
                    self.processed_market_maker_program_actions.insert(
                        program.action_id,
                        ProcessedMarketMakerProgramAction {
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt: MarketMakerProgramActionReceipt::Rejected {
                                action_kind: action_kind.clone(),
                                reason: reason.clone(),
                            },
                        },
                    );
                    self.runtime_events.truncate(start);
                    self.next_runtime_event_seq = start as u64;
                    self.rebuild_block_event_index();
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::MarketMakerProgramActionRejected {
                            action_kind,
                            reason,
                        },
                    );
                    continue;
                }
            }
            let mut committed_seqs = Vec::new();
            for event in &mut self.runtime_events[start..] {
                event.block_height = Some(block.header.height);
                committed_seqs.push(event.seq);
            }
            for seq in committed_seqs {
                self.note_committed_event_seq(block.header.height, seq);
            }
        }
        self.prune_processed_market_maker_program_actions(block.header.timestamp_ms);
    }

    pub(super) fn prune_processed_market_maker_program_actions(&mut self, timestamp_ms: u64) {
        let min_timestamp =
            timestamp_ms.saturating_sub(MARKET_MAKER_PROGRAM_ACTION_RECEIPT_RETENTION_MS);
        self.processed_market_maker_program_actions
            .retain(|_, processed| processed.processed_at_ms >= min_timestamp);
    }

    pub(super) fn apply_block_market_maker_program_action(
        &mut self,
        program: &BlockMarketMakerProgram,
        timestamp_ms: u64,
        event_block_height: Option<u64>,
    ) -> Result<MarketMakerProgramActionReceipt, NodeError> {
        match &program.action {
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
            } => self
                .define_market_maker_program(
                    DefineMarketMakerProgramInput {
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
                    },
                    timestamp_ms,
                )
                .map(|program| MarketMakerProgramActionReceipt::Define {
                    program_id: program.program_id,
                }),
            BlockMarketMakerProgramAction::Retire { program_id } => self
                .retire_market_maker_program(*program_id, timestamp_ms)
                .map(|program| MarketMakerProgramActionReceipt::Retire {
                    program_id: program.program_id,
                }),
            BlockMarketMakerProgramAction::Enroll {
                market_maker_identity,
                market_id,
                program_id,
                bond_micro_usdc,
            } => self
                .enroll_market_maker(
                    EnrollMarketMakerInput {
                        market_maker_identity: *market_maker_identity,
                        market_id: *market_id,
                        program_id: *program_id,
                        bond_micro_usdc: MarketMakerU256(*bond_micro_usdc),
                    },
                    timestamp_ms,
                    event_block_height,
                )
                .map(|enrollment| MarketMakerProgramActionReceipt::Enroll {
                    market_maker_identity: enrollment.market_maker_identity,
                    market_id: enrollment.market_id,
                    program_id: enrollment.program_id,
                }),
            BlockMarketMakerProgramAction::Exit {
                market_maker_identity,
                market_id,
                program_id,
            } => self
                .exit_market_maker_enrollment(
                    *market_maker_identity,
                    *market_id,
                    *program_id,
                    timestamp_ms,
                    event_block_height,
                )
                .map(|enrollment| MarketMakerProgramActionReceipt::Exit {
                    market_maker_identity: enrollment.market_maker_identity,
                    market_id: enrollment.market_id,
                    program_id: enrollment.program_id,
                }),
        }
    }

    pub(super) fn apply_block_tee_governance_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::TeeGovernance(governance) = item else {
                continue;
            };
            let start = self.runtime_events.len();
            let fingerprint = tee_governance_action_fingerprint(&governance.action);
            if let Some(processed) = self
                .processed_tee_governance_actions
                .get(&governance.action_id)
                .cloned()
            {
                if processed.fingerprint != fingerprint {
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::TeeGovernanceActionRejected {
                            governance_action: governance.action_id,
                            action_kind: tee_governance_block_action_kind(&governance.action)
                                .to_string(),
                            reason: "tee_governance_action_id_mismatch".to_string(),
                        },
                    );
                }
                continue;
            }
            let result = self.apply_block_tee_governance_action(governance, block.header.height);
            match result {
                Ok(receipt) => {
                    self.processed_tee_governance_actions.insert(
                        governance.action_id,
                        ProcessedTeeGovernanceAction {
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt,
                        },
                    );
                }
                Err(error) => {
                    let reason = tee_governance_block_error_reason(&error).to_string();
                    let action_kind =
                        tee_governance_block_action_kind(&governance.action).to_string();
                    self.processed_tee_governance_actions.insert(
                        governance.action_id,
                        ProcessedTeeGovernanceAction {
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt: TeeGovernanceActionReceipt::Rejected {
                                action_kind: action_kind.clone(),
                                reason: reason.clone(),
                            },
                        },
                    );
                    // Defensive rollback for future handlers that might emit
                    // before their final validation. Current TEE governance
                    // handlers decide success before emitting.
                    self.runtime_events.truncate(start);
                    self.next_runtime_event_seq = start as u64;
                    self.rebuild_block_event_index();
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::TeeGovernanceActionRejected {
                            governance_action: governance.action_id,
                            action_kind,
                            reason,
                        },
                    );
                    continue;
                }
            }
            let mut committed_seqs = Vec::new();
            for event in &mut self.runtime_events[start..] {
                event.block_height = Some(block.header.height);
                committed_seqs.push(event.seq);
            }
            for seq in committed_seqs {
                self.note_committed_event_seq(block.header.height, seq);
            }
        }
        self.apply_due_tee_vendor_cap_profile_updates(
            block.header.height,
            block.header.timestamp_ms,
        );
        self.apply_due_tee_vendor_root_revocations(block.header.height, block.header.timestamp_ms);
        self.prune_processed_tee_governance_actions(block.header.timestamp_ms);
    }

    pub(super) fn apply_due_tee_vendor_cap_profile_updates(
        &mut self,
        block_height: u64,
        timestamp_ms: u64,
    ) {
        let due_action_ids = self
            .scheduled_tee_vendor_cap_profile_updates
            .iter()
            .filter_map(|(action_id, update)| {
                (update.valid_from_block <= block_height).then_some(*action_id)
            })
            .collect::<Vec<_>>();
        for action_id in due_action_ids {
            let Some(update) = self
                .scheduled_tee_vendor_cap_profile_updates
                .remove(&action_id)
            else {
                continue;
            };
            let Some(profile) = self.tee_vendor_cap_profiles.get_mut(&update.vendor) else {
                continue;
            };
            let old_cap_multiplier_microunits = profile.cap_multiplier_microunits;
            let old_composition_eligibility = profile.composition_eligibility;
            profile.cap_multiplier_microunits = update.cap_multiplier_microunits;
            profile.composition_eligibility = update.composition_eligibility;
            profile.last_updated_governance_action = update.governance_action;
            self.record_runtime_event(
                Some(block_height),
                timestamp_ms,
                RuntimeEventKind::VendorCapProfileUpdated {
                    vendor: identity_attestation_vendor_for_tee(update.vendor),
                    old_cap_multiplier_microunits,
                    new_cap_multiplier_microunits: update.cap_multiplier_microunits,
                    old_composition_eligibility,
                    new_composition_eligibility: update.composition_eligibility,
                    governance_action: update.governance_action,
                },
            );
        }
    }

    pub(super) fn apply_due_tee_vendor_root_revocations(
        &mut self,
        block_height: u64,
        timestamp_ms: u64,
    ) {
        // Due roots are scanned cumulatively; removing the reverse-index entry
        // after processing makes replay idempotent without a separate height window.
        let due_roots = self
            .tee_vendor_roots
            .iter()
            .flat_map(|(vendor, roots)| {
                roots.iter().filter_map(|root| {
                    root.valid_until_block
                        .is_some_and(|valid_until| valid_until <= block_height)
                        .then_some((*vendor, root.root_id))
                })
            })
            .collect::<Vec<_>>();

        for (vendor, root_id) in due_roots {
            let Some(entries) = self
                .tee_attestations_by_vendor_root
                .remove(&(vendor, root_id))
            else {
                continue;
            };
            let mut revoked_by_account = BTreeMap::<u64, Vec<AttestationRecordId>>::new();
            for (account, record_id) in entries {
                let Some(record) = self.agent_attestations.get_mut(&record_id) else {
                    continue;
                };
                if record.account != account
                    || record.kind != AttestationKind::TeeEnclave
                    || record.revoked_at_block.is_some()
                    || !matches!(
                        record.evidence,
                        AttestationEvidence::TeeQuoteVerified {
                            vendor: record_vendor,
                            vendor_root_id,
                            ..
                        } if record_vendor == vendor && vendor_root_id == root_id
                    )
                {
                    continue;
                }
                record.revoked_at_block = Some(block_height);
                revoked_by_account
                    .entry(account)
                    .or_default()
                    .push(record_id);
            }

            for (account, mut record_ids) in revoked_by_account {
                record_ids.sort();
                let tier_before = self
                    .agent_identities
                    .get(&account)
                    .map(|identity| identity.tier)
                    .unwrap_or(AgentIdentityTier::Tier0Unattested);
                let tier_after =
                    self.recompute_agent_identity_tier(account, timestamp_ms, block_height);
                for record_id in record_ids {
                    self.record_runtime_event(
                        Some(block_height),
                        timestamp_ms,
                        RuntimeEventKind::AttestationRevoked {
                            account,
                            record_id,
                            reason: identity_attestation_revocation_reason(
                                AttestationRevocationReason::VendorRootRevoked,
                            ),
                            tier_before: identity_tier_status(tier_before),
                            tier_after: identity_tier_status(tier_after),
                        },
                    );
                }
                if tier_before != tier_after {
                    self.record_runtime_event(
                        Some(block_height),
                        timestamp_ms,
                        RuntimeEventKind::TierChanged {
                            account,
                            from: identity_tier_status(tier_before),
                            to: identity_tier_status(tier_after),
                            reason: identity_tier_transition_reason(
                                TierTransitionReason::VendorRootChanged,
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
        }
    }

    pub(super) fn prune_processed_tee_governance_actions(&mut self, timestamp_ms: u64) {
        let min_timestamp = timestamp_ms.saturating_sub(TEE_GOVERNANCE_ACTION_RECEIPT_RETENTION_MS);
        self.processed_tee_governance_actions
            .retain(|_, processed| processed.processed_at_ms >= min_timestamp);
    }

    pub(super) fn apply_block_tee_governance_action(
        &mut self,
        governance: &BlockTeeGovernance,
        block_height: u64,
    ) -> Result<TeeGovernanceActionReceipt, NodeError> {
        self.validate_block_tee_governance_action(
            governance.action_id,
            &governance.action,
            block_height,
        )?;
        match &governance.action {
            BlockTeeGovernanceAction::IntroduceVendorRoot {
                vendor,
                payload,
                valid_from_block,
            } => {
                let vendor = tee_vendor_from_block(*vendor);
                let payload = vendor_root_payload_from_block(payload);
                let root_id = derive_vendor_root_id(vendor, &payload);
                self.tee_vendor_roots
                    .entry(vendor)
                    .or_default()
                    .push(VendorRoot {
                        vendor,
                        root_id,
                        payload,
                        valid_from_block: *valid_from_block,
                        valid_until_block: None,
                        introduced_by_governance_action: governance.action_id,
                        revoked_by_governance_action: None,
                    });
                self.sync_tee_vendor_roots_execution_mirror();
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::VendorRootIntroduced {
                        vendor: identity_attestation_vendor_for_tee(vendor),
                        root_id,
                        valid_from_block: *valid_from_block,
                        governance_action: governance.action_id,
                    },
                );
                Ok(TeeGovernanceActionReceipt::IntroduceVendorRoot {
                    vendor,
                    root_id,
                    valid_from_block: *valid_from_block,
                })
            }
            BlockTeeGovernanceAction::RevokeVendorRoot {
                vendor,
                root_id,
                valid_until_block,
            } => {
                let vendor = tee_vendor_from_block(*vendor);
                let roots = self.tee_vendor_roots.get_mut(&vendor).ok_or(
                    NodeError::InvalidTeeGovernance {
                        action_id: governance.action_id,
                        reason: "unknown_vendor_root",
                    },
                )?;
                let root = roots
                    .iter_mut()
                    .find(|root| root.root_id == *root_id)
                    .ok_or(NodeError::InvalidTeeGovernance {
                        action_id: governance.action_id,
                        reason: "unknown_vendor_root",
                    })?;
                root.valid_until_block = Some(*valid_until_block);
                root.revoked_by_governance_action = Some(governance.action_id);
                self.sync_tee_vendor_roots_execution_mirror();
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::VendorRootRevoked {
                        vendor: identity_attestation_vendor_for_tee(vendor),
                        root_id: *root_id,
                        valid_until_block: *valid_until_block,
                        governance_action: governance.action_id,
                    },
                );
                Ok(TeeGovernanceActionReceipt::RevokeVendorRoot {
                    vendor,
                    root_id: *root_id,
                    valid_until_block: *valid_until_block,
                })
            }
            BlockTeeGovernanceAction::UpdateVendorCapProfile {
                vendor,
                cap_multiplier_microunits,
                composition_eligibility,
                valid_from_block,
            } => {
                let vendor = tee_vendor_from_block(*vendor);
                self.scheduled_tee_vendor_cap_profile_updates.insert(
                    governance.action_id,
                    ScheduledTeeVendorCapProfileUpdate {
                        governance_action: governance.action_id,
                        vendor,
                        cap_multiplier_microunits: *cap_multiplier_microunits,
                        composition_eligibility: *composition_eligibility,
                        valid_from_block: *valid_from_block,
                    },
                );
                Ok(TeeGovernanceActionReceipt::UpdateVendorCapProfile {
                    vendor,
                    cap_multiplier_microunits: *cap_multiplier_microunits,
                    composition_eligibility: *composition_eligibility,
                    valid_from_block: *valid_from_block,
                })
            }
        }
    }

    pub(super) fn validate_block_tee_governance_action(
        &self,
        action_id: TeeGovernanceActionId,
        action: &BlockTeeGovernanceAction,
        block_height: u64,
    ) -> Result<(), NodeError> {
        match action {
            BlockTeeGovernanceAction::IntroduceVendorRoot {
                vendor,
                payload,
                valid_from_block,
            } => {
                let vendor = tee_vendor_from_block(*vendor);
                let payload = vendor_root_payload_from_block(payload);
                if !vendor_root_payload_matches_vendor(&payload, vendor)
                    || !vendor_root_payload_within_bounds(&payload)
                {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_root_payload_invalid",
                    });
                }
                if *valid_from_block < block_height.saturating_add(VENDOR_ROOT_NOTICE_BLOCKS) {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_root_notice_too_short",
                    });
                }
                let root_id = derive_vendor_root_id(vendor, &payload);
                if self
                    .tee_vendor_roots
                    .values()
                    .flatten()
                    .any(|root| root.root_id == root_id)
                {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_root_already_exists",
                    });
                }
                Ok(())
            }
            BlockTeeGovernanceAction::RevokeVendorRoot {
                vendor,
                root_id,
                valid_until_block,
            } => {
                let vendor = tee_vendor_from_block(*vendor);
                let root = self
                    .tee_vendor_roots
                    .get(&vendor)
                    .and_then(|roots| roots.iter().find(|root| root.root_id == *root_id))
                    .ok_or(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "unknown_vendor_root",
                    })?;
                if root.revoked_by_governance_action.is_some() {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_root_already_revoked",
                    });
                }
                if *valid_until_block < block_height.saturating_add(VENDOR_ROOT_NOTICE_BLOCKS) {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_root_notice_too_short",
                    });
                }
                if *valid_until_block <= root.valid_from_block {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_root_valid_until_before_valid_from",
                    });
                }
                Ok(())
            }
            BlockTeeGovernanceAction::UpdateVendorCapProfile {
                vendor,
                cap_multiplier_microunits,
                composition_eligibility,
                valid_from_block,
            } => {
                let vendor = tee_vendor_from_block(*vendor);
                let profile = self.tee_vendor_cap_profiles.get(&vendor).ok_or(
                    NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "unknown_vendor_cap_profile",
                    },
                )?;
                if self
                    .scheduled_tee_vendor_cap_profile_updates
                    .values()
                    .any(|update| update.vendor == vendor)
                {
                    // Consensus guard: a previously committed update for this
                    // vendor is already in its notice window.
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_cap_profile_update_already_pending",
                    });
                }
                if *cap_multiplier_microunits == 0 {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_cap_multiplier_zero",
                    });
                }
                if *valid_from_block < block_height.saturating_add(TEE_GOVERNANCE_NOTICE_BLOCKS) {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_cap_profile_notice_too_short",
                    });
                }
                if profile.cap_multiplier_microunits == *cap_multiplier_microunits
                    && profile.composition_eligibility == *composition_eligibility
                {
                    return Err(NodeError::InvalidTeeGovernance {
                        action_id,
                        reason: "vendor_cap_profile_noop",
                    });
                }
                Ok(())
            }
        }
    }

    pub(super) fn apply_block_real_capital_governance_items(&mut self, block: &Block) {
        for item in &block.body.items {
            let BlockItem::RealCapitalGovernance(governance) = item else {
                continue;
            };
            let start = self.runtime_events.len();
            let fingerprint = real_capital_governance_action_fingerprint(&governance.action);
            if let Some(processed) = self
                .processed_real_capital_governance_actions
                .get(&governance.action_id)
                .cloned()
            {
                if processed.fingerprint != fingerprint {
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::RealCapitalGovernanceActionRejected {
                            governance_action: governance.action_id,
                            action_kind: real_capital_governance_block_action_kind(
                                &governance.action,
                            )
                            .to_string(),
                            reason: "real_capital_governance_action_id_mismatch".to_string(),
                        },
                    );
                }
                continue;
            }
            match self.apply_block_real_capital_governance_action(governance, block.header.height) {
                Ok(receipt) => {
                    self.processed_real_capital_governance_actions.insert(
                        governance.action_id,
                        ProcessedRealCapitalGovernanceAction {
                            submitted_at_ms: governance.timestamp_ms,
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt,
                        },
                    );
                }
                Err(error) => {
                    let reason = real_capital_governance_block_error_reason(&error).to_string();
                    let action_kind =
                        real_capital_governance_block_action_kind(&governance.action).to_string();
                    self.processed_real_capital_governance_actions.insert(
                        governance.action_id,
                        ProcessedRealCapitalGovernanceAction {
                            submitted_at_ms: governance.timestamp_ms,
                            processed_at_ms: block.header.timestamp_ms,
                            fingerprint,
                            receipt: RealCapitalGovernanceActionReceipt::Rejected {
                                action_kind: action_kind.clone(),
                                reason: reason.clone(),
                            },
                        },
                    );
                    // Defensive rollback for future handlers that might emit
                    // before final validation. Current RFC 049 governance
                    // handlers validate before emitting success events.
                    self.runtime_events.truncate(start);
                    self.next_runtime_event_seq = start as u64;
                    self.rebuild_block_event_index();
                    self.record_runtime_event(
                        Some(block.header.height),
                        block.header.timestamp_ms,
                        RuntimeEventKind::RealCapitalGovernanceActionRejected {
                            governance_action: governance.action_id,
                            action_kind,
                            reason,
                        },
                    );
                    continue;
                }
            }
            let mut committed_seqs = Vec::new();
            for event in &mut self.runtime_events[start..] {
                event.block_height = Some(block.header.height);
                committed_seqs.push(event.seq);
            }
            for seq in committed_seqs {
                self.note_committed_event_seq(block.header.height, seq);
            }
        }
        self.prune_processed_real_capital_governance_actions(block.header.timestamp_ms);
    }

    pub(super) fn prune_processed_real_capital_governance_actions(&mut self, timestamp_ms: u64) {
        let min_timestamp =
            timestamp_ms.saturating_sub(REAL_CAPITAL_GOVERNANCE_ACTION_RECEIPT_RETENTION_MS);
        self.processed_real_capital_governance_actions
            .retain(|_, processed| processed.processed_at_ms >= min_timestamp);
    }

    pub(super) fn apply_block_real_capital_governance_action(
        &mut self,
        governance: &BlockRealCapitalGovernance,
        block_height: u64,
    ) -> Result<RealCapitalGovernanceActionReceipt, NodeError> {
        self.validate_block_real_capital_governance_action(
            governance.action_id,
            &governance.action,
            block_height,
        )?;
        match &governance.action {
            BlockRealCapitalGovernanceAction::UpsertListing {
                vault_id,
                delegation_id,
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
                let old_serving_agent =
                    self.real_capital_listings
                        .get(vault_id)
                        .and_then(|existing| {
                            (existing.serving_agent != *serving_agent)
                                .then_some(existing.serving_agent)
                        });
                let listed_at_block = self
                    .real_capital_listings
                    .get(vault_id)
                    .map(|listing| listing.listed_at_block)
                    .unwrap_or(block_height);
                let listing = RealCapitalListing {
                    vault_id: *vault_id,
                    delegation_id: *delegation_id,
                    serving_agent: *serving_agent,
                    mode: real_capital_mode_from_block(*mode),
                    pilot_min_serving_agent_tier: pilot_min_serving_agent_tier
                        .map(agent_identity_tier_from_block),
                    tier0_pilot_exception: *tier0_pilot_exception,
                    deposit_enabled: *deposit_enabled,
                    listed_for_discovery: *listed_for_discovery,
                    caps: ListingCaps {
                        per_principal_micro_usdc: *per_principal_micro_usdc,
                        total_vault_tvl_micro_usdc: *total_vault_tvl_micro_usdc,
                    },
                    pilot_principal_allowlist: pilot_principal_allowlist.clone(),
                    listed_at_block,
                    last_updated_block: block_height,
                    last_updated_governance_action: governance.action_id,
                };
                let mode = listing.mode;
                self.upsert_real_capital_listing(listing);
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::RealCapitalListingUpserted {
                        vault_id: *vault_id,
                        delegation_id: *delegation_id,
                        serving_agent: *serving_agent,
                        old_serving_agent,
                        mode: real_capital_listing_mode(mode),
                        deposit_enabled: *deposit_enabled,
                        listed_for_discovery: *listed_for_discovery,
                        per_principal_cap_micro_usdc: *per_principal_micro_usdc,
                        total_vault_tvl_cap_micro_usdc: *total_vault_tvl_micro_usdc,
                        governance_action: governance.action_id,
                    },
                );
                Ok(RealCapitalGovernanceActionReceipt::UpsertListing {
                    vault_id: *vault_id,
                    delegation_id: *delegation_id,
                    serving_agent: *serving_agent,
                })
            }
            BlockRealCapitalGovernanceAction::SetListingDepositEnabled {
                vault_id,
                deposit_enabled,
            } => {
                let listing = self.real_capital_listings.get_mut(vault_id).ok_or(
                    NodeError::InvalidRealCapitalGovernance {
                        action_id: governance.action_id,
                        reason: "unknown_vault_listing",
                    },
                )?;
                let delegation_id = listing.delegation_id;
                let serving_agent = listing.serving_agent;
                listing.deposit_enabled = *deposit_enabled;
                listing.last_updated_block = block_height;
                listing.last_updated_governance_action = governance.action_id;
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::RealCapitalListingDepositEnabledUpdated {
                        vault_id: *vault_id,
                        delegation_id,
                        serving_agent,
                        deposit_enabled: *deposit_enabled,
                        governance_action: governance.action_id,
                    },
                );
                Ok(
                    RealCapitalGovernanceActionReceipt::SetListingDepositEnabled {
                        vault_id: *vault_id,
                        deposit_enabled: *deposit_enabled,
                    },
                )
            }
            BlockRealCapitalGovernanceAction::SetListingDiscovery {
                vault_id,
                listed_for_discovery,
            } => {
                let listing = self.real_capital_listings.get_mut(vault_id).ok_or(
                    NodeError::InvalidRealCapitalGovernance {
                        action_id: governance.action_id,
                        reason: "unknown_vault_listing",
                    },
                )?;
                let delegation_id = listing.delegation_id;
                let serving_agent = listing.serving_agent;
                listing.listed_for_discovery = *listed_for_discovery;
                listing.last_updated_block = block_height;
                listing.last_updated_governance_action = governance.action_id;
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::RealCapitalListingDiscoveryUpdated {
                        vault_id: *vault_id,
                        delegation_id,
                        serving_agent,
                        listed_for_discovery: *listed_for_discovery,
                        governance_action: governance.action_id,
                    },
                );
                Ok(RealCapitalGovernanceActionReceipt::SetListingDiscovery {
                    vault_id: *vault_id,
                    listed_for_discovery: *listed_for_discovery,
                })
            }
            BlockRealCapitalGovernanceAction::UpdateProtocolCap {
                total_protocol_tvl_micro_usdc,
            } => {
                let old_total_protocol_tvl_micro_usdc = self
                    .protocol_delegation_cap
                    .map(|cap| cap.total_protocol_tvl_micro_usdc);
                self.protocol_delegation_cap =
                    total_protocol_tvl_micro_usdc.map(|cap| ProtocolDelegationCap {
                        total_protocol_tvl_micro_usdc: cap,
                        last_updated_governance_action: governance.action_id,
                    });
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::RealCapitalProtocolCapUpdated {
                        old_total_protocol_tvl_micro_usdc,
                        new_total_protocol_tvl_micro_usdc: *total_protocol_tvl_micro_usdc,
                        governance_action: governance.action_id,
                    },
                );
                Ok(RealCapitalGovernanceActionReceipt::UpdateProtocolCap {
                    total_protocol_tvl_micro_usdc: *total_protocol_tvl_micro_usdc,
                })
            }
            BlockRealCapitalGovernanceAction::UpdateGlobalKillSwitch { disabled } => {
                let old_disabled = self.listing_control_state.global_deposit_kill_switch;
                self.listing_control_state.global_deposit_kill_switch = *disabled;
                self.listing_control_state.last_updated_block = block_height;
                self.listing_control_state.last_updated_governance_action = governance.action_id;
                self.record_runtime_event(
                    None,
                    governance.timestamp_ms,
                    RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated {
                        old_disabled,
                        new_disabled: *disabled,
                        governance_action: governance.action_id,
                    },
                );
                Ok(RealCapitalGovernanceActionReceipt::UpdateGlobalKillSwitch {
                    disabled: *disabled,
                })
            }
        }
    }

    pub(super) fn validate_block_real_capital_governance_action(
        &self,
        action_id: RealCapitalGovernanceActionId,
        action: &BlockRealCapitalGovernanceAction,
        block_height: u64,
    ) -> Result<(), NodeError> {
        match action {
            BlockRealCapitalGovernanceAction::UpsertListing {
                vault_id,
                delegation_id,
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
                let listed_at_block = self
                    .real_capital_listings
                    .get(vault_id)
                    .map(|listing| listing.listed_at_block)
                    .unwrap_or(block_height);
                let listing = RealCapitalListing {
                    vault_id: *vault_id,
                    delegation_id: *delegation_id,
                    serving_agent: *serving_agent,
                    mode: real_capital_mode_from_block(*mode),
                    pilot_min_serving_agent_tier: pilot_min_serving_agent_tier
                        .map(agent_identity_tier_from_block),
                    tier0_pilot_exception: *tier0_pilot_exception,
                    deposit_enabled: *deposit_enabled,
                    listed_for_discovery: *listed_for_discovery,
                    caps: ListingCaps {
                        per_principal_micro_usdc: *per_principal_micro_usdc,
                        total_vault_tvl_micro_usdc: *total_vault_tvl_micro_usdc,
                    },
                    pilot_principal_allowlist: pilot_principal_allowlist.clone(),
                    listed_at_block,
                    last_updated_block: block_height,
                    last_updated_governance_action: action_id,
                };
                self.validate_real_capital_listing(&listing)
                    .map_err(|error| match error {
                        NodeError::InvalidDelegation { reason, .. } => {
                            NodeError::InvalidRealCapitalGovernance { action_id, reason }
                        }
                        other => other,
                    })
            }
            BlockRealCapitalGovernanceAction::SetListingDepositEnabled { vault_id, .. }
            | BlockRealCapitalGovernanceAction::SetListingDiscovery { vault_id, .. } => {
                if !self.real_capital_listings.contains_key(vault_id) {
                    return Err(NodeError::InvalidRealCapitalGovernance {
                        action_id,
                        reason: "unknown_vault_listing",
                    });
                }
                Ok(())
            }
            BlockRealCapitalGovernanceAction::UpdateProtocolCap {
                total_protocol_tvl_micro_usdc,
            } => {
                if total_protocol_tvl_micro_usdc.is_some_and(|cap| cap == 0) {
                    return Err(NodeError::InvalidRealCapitalGovernance {
                        action_id,
                        reason: "vault_protocol_cap_zero",
                    });
                }
                Ok(())
            }
            BlockRealCapitalGovernanceAction::UpdateGlobalKillSwitch { .. } => Ok(()),
        }
    }

    pub fn commit_market_maker_program_action_block(
        &mut self,
        action: BlockMarketMakerProgramAction,
        timestamp_ms: u64,
    ) -> Result<&Block, NodeError> {
        let parent = self.head().clone();
        let round = self
            .pacemaker
            .current_round
            .max(parent.header.round.saturating_add(1));
        let proposer = self
            .local_validator_id()
            .ok_or(NodeError::LocalValidatorUnavailable)?;
        let action_id = market_maker_program_action_id_for_submission(&action, timestamp_ms, None);
        let body = BlockBody {
            items: vec![BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
                action_id,
                timestamp_ms,
                action,
            })],
        };
        if !env_bool_with_default(MAHI_LEGACY_EXEC_ENV, false)? {
            return self.install_native_local_block_state(
                &parent,
                round,
                proposer,
                body,
                self.policy_store.clone(),
                self.order_books.clone(),
                self.resting_order_mandates.clone(),
                self.isolated_collateral.clone(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                timestamp_ms,
            );
        }
        let parent_qc = build_quorum_certificate(&parent, &self.consensus_signing_keys)?;
        let prepared_block = self
            .executor
            .clone()
            .prepare_block_with_checkpoint(&parent, round, proposer, parent_qc, body, timestamp_ms)
            .map_err(NodeError::Execution)?;
        let prepared = PreparedCommittedBlock {
            prepared_block,
            policy_store: self.policy_store.clone(),
            order_book_overlays: self.order_books.clone(),
            resting_order_mandate_overlays: self.resting_order_mandates.clone(),
            collateral_overlays: self.isolated_collateral.clone(),
            rejected_intents: Vec::new(),
            committed_events: Vec::new(),
        };
        let produced_qc = build_quorum_certificate(prepared.block(), &self.consensus_signing_keys)?;
        self.install_prepared_block_state(prepared, produced_qc)
    }

    // v1 doc 05 — Mandates.
}
