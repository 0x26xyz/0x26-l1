use super::*;

impl SingleNodeEngine {
    pub fn mandates_for_agent(&self, agent: u64) -> impl Iterator<Item = &Mandate> {
        self.mandates_by_agent
            .get(&agent)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|id| self.mandates.get(id))
    }

    fn active_mandate_count_for_agent(&self, agent: u64) -> usize {
        self.mandates_for_agent(agent)
            .filter(|mandate| mandate.state == MandateState::Active)
            .count()
    }

    fn non_terminal_mandate_count_for_agent(&self, agent: u64) -> usize {
        self.mandates_for_agent(agent)
            .filter(|mandate| !mandate.state.is_terminal())
            .count()
    }

    pub fn create_mandate(
        &mut self,
        input: CreateMandateInput,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        if self
            .mandates
            .values()
            .filter(|mandate| mandate.owner == input.owner && mandate.agent == input.agent)
            .any(|mandate| mandate.version >= input.version)
        {
            return Err(NodeError::InvalidMandate {
                agent: input.agent,
                reason: "version_not_monotonic",
            });
        }
        if self.active_mandate_count_for_agent(input.agent) >= MAX_ACTIVE_MANDATES_PER_AGENT {
            return Err(NodeError::InvalidMandate {
                agent: input.agent,
                reason: "max_active_mandates_per_agent",
            });
        }
        if self.non_terminal_mandate_count_for_agent(input.agent)
            >= MAX_NON_TERMINAL_MANDATES_PER_AGENT
        {
            return Err(NodeError::InvalidMandate {
                agent: input.agent,
                reason: "max_non_terminal_mandates_per_agent",
            });
        }
        let mandate_id = derive_mandate_id(
            input.owner,
            input.agent,
            input.version,
            input.constraints.clone(),
            input.objectives.clone(),
            input.objective_weights.clone(),
            input.conformance_window_ms,
            input.session_keys.clone(),
            input.expires_at,
            input.description_hash,
        );
        if self.mandates.contains_key(&mandate_id) {
            return Err(NodeError::InvalidMandate {
                agent: input.agent,
                reason: "mandate_id_collision",
            });
        }
        let pending_objectives =
            u16::try_from(input.objectives.len()).map_err(|_| NodeError::InvalidMandate {
                agent: input.agent,
                reason: "too_many_objectives",
            })?;
        let mut mandate = Mandate {
            id: mandate_id,
            owner: input.owner,
            agent: input.agent,
            version: input.version,
            constraints: input.constraints,
            objectives: input.objectives,
            objective_weights: input.objective_weights,
            conformance_window_ms: input.conformance_window_ms,
            policies: Vec::new(),
            session_keys: input.session_keys,
            activated_at: Some(timestamp_ms),
            expires_at: input.expires_at,
            state: MandateState::Active,
            description_uri: input.description_uri,
            description_hash: input.description_hash,
            conformance: ConformanceState {
                score: 50,
                last_evaluated_at_ms: timestamp_ms,
                pending_objectives,
            },
        };
        validate_mandate_record(&mandate).map_err(|reason| NodeError::InvalidMandate {
            agent: mandate.agent,
            reason,
        })?;
        let generated_policies =
            compile_mandate_policies(&mandate, &self.policy_store, timestamp_ms).map_err(
                |reason| NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason,
                },
            )?;
        mandate.policies = generated_policies.iter().map(|policy| policy.id).collect();
        validate_mandate_record(&mandate).map_err(|reason| NodeError::InvalidMandate {
            agent: mandate.agent,
            reason,
        })?;

        for policy in generated_policies {
            self.policy_store
                .insert(policy)
                .map_err(NodeError::Policy)?;
        }
        self.mandates.insert(mandate_id, mandate.clone());
        self.mandates_by_agent
            .entry(mandate.agent)
            .or_default()
            .insert(mandate_id);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MandateCreated {
                mandate_id,
                owner: mandate.owner,
                agent: mandate.agent,
                version: mandate.version,
                expires_at: mandate.expires_at,
            },
        );
        for policy_id in &mandate.policies {
            self.record_runtime_event(
                None,
                timestamp_ms,
                RuntimeEventKind::PolicyCreated {
                    account_id: mandate.agent,
                    parent_policy_id: None,
                    policy_id: *policy_id,
                },
            );
        }
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MandateActivated {
                mandate_id,
                agent: mandate.agent,
                activated_at_ms: timestamp_ms,
            },
        );
        Ok(mandate)
    }

    pub fn mandate_action_nonce(&self, owner: &Principal) -> u64 {
        self.mandate_action_nonces.get(owner).copied().unwrap_or(0)
    }

    pub fn submit_mandate_action(
        &mut self,
        signed_action: SignedMandateAction,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        if signed_action.chain_id != self.head().header.chain_id {
            return Err(NodeError::InvalidMandateAction {
                owner: signed_action.owner,
                reason: "chain_mismatch",
            });
        }
        let expected_nonce = self.mandate_action_nonce(&signed_action.owner);
        if signed_action.nonce != expected_nonce {
            return Err(NodeError::InvalidMandateActionNonce {
                owner: signed_action.owner,
                expected: expected_nonce,
                actual: signed_action.nonce,
            });
        }
        let verifying_key = VerifyingKey::from_bytes(&signed_action.owner).map_err(|_| {
            NodeError::InvalidMandateActionSignature {
                owner: signed_action.owner,
            }
        })?;
        let signature = Ed25519Signature::from_bytes(&signed_action.signature);
        let signing_bytes = mandate_action_signing_bytes(
            signed_action.chain_id,
            signed_action.owner,
            signed_action.nonce,
            &signed_action.action,
        );
        verifying_key
            .verify_strict(&signing_bytes, &signature)
            .map_err(|_| NodeError::InvalidMandateActionSignature {
                owner: signed_action.owner,
            })?;

        self.submit_verified_mandate_action(
            signed_action.chain_id,
            signed_action.owner,
            signed_action.nonce,
            signed_action.action,
            timestamp_ms,
        )
    }

    pub fn submit_verified_mandate_action(
        &mut self,
        chain_id: u64,
        owner: Principal,
        nonce: u64,
        action: MandateAction,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        if chain_id != self.head().header.chain_id {
            return Err(NodeError::InvalidMandateAction {
                owner,
                reason: "chain_mismatch",
            });
        }
        let expected_nonce = self.mandate_action_nonce(&owner);
        if nonce != expected_nonce {
            return Err(NodeError::InvalidMandateActionNonce {
                owner,
                expected: expected_nonce,
                actual: nonce,
            });
        }

        let result = match action {
            MandateAction::CreateMandate(input) => {
                if input.owner != owner {
                    return Err(NodeError::InvalidMandateAction {
                        owner,
                        reason: "owner_mismatch",
                    });
                }
                self.create_mandate(input, timestamp_ms)
            }
            MandateAction::SuspendMandate { mandate_id } => {
                self.ensure_mandate_owner(mandate_id, owner)?;
                self.suspend_mandate(mandate_id, timestamp_ms)
            }
            MandateAction::ResumeMandate { mandate_id } => {
                self.ensure_mandate_owner(mandate_id, owner)?;
                self.resume_mandate(mandate_id, timestamp_ms)
            }
            MandateAction::RevokeMandate { mandate_id, reason } => {
                self.ensure_mandate_owner(mandate_id, owner)?;
                self.revoke_mandate(mandate_id, reason, timestamp_ms)
            }
        }?;
        self.mandate_action_nonces
            .insert(owner, expected_nonce.saturating_add(1));
        Ok(result)
    }

    fn ensure_mandate_owner(
        &self,
        mandate_id: MandateId,
        owner: Principal,
    ) -> Result<(), NodeError> {
        let mandate = self
            .mandates
            .get(&mandate_id)
            .ok_or(NodeError::InvalidMandateAction {
                owner,
                reason: "unknown_mandate",
            })?;
        if mandate.owner != owner {
            return Err(NodeError::InvalidMandateAction {
                owner,
                reason: "owner_mismatch",
            });
        }
        Ok(())
    }

    pub fn suspend_mandate(
        &mut self,
        mandate_id: MandateId,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        let (agent, owner, policy_ids) = {
            let mandate = self
                .mandates
                .get_mut(&mandate_id)
                .ok_or(NodeError::InvalidMandate {
                    agent: 0,
                    reason: "unknown_mandate",
                })?;
            if mandate.state != MandateState::Active {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_not_active",
                });
            }
            mandate.state = MandateState::Suspended;
            (mandate.agent, mandate.owner, mandate.policies.clone())
        };
        for policy_id in policy_ids {
            self.policy_store
                .suspend(policy_id)
                .map_err(NodeError::Policy)?;
        }
        self.clear_mandate_resting_order_metadata(mandate_id);
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MandateSuspended {
                mandate_id,
                agent,
                by: owner,
                suspended_at_ms: timestamp_ms,
            },
        );
        Ok(self
            .mandates
            .get(&mandate_id)
            .expect("mandate existence checked before suspend")
            .clone())
    }

    pub fn resume_mandate(
        &mut self,
        mandate_id: MandateId,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        let (agent, owner, policy_ids) = {
            let mandate = self
                .mandates
                .get_mut(&mandate_id)
                .ok_or(NodeError::InvalidMandate {
                    agent: 0,
                    reason: "unknown_mandate",
                })?;
            if mandate.state != MandateState::Suspended {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_not_suspended",
                });
            }
            if timestamp_ms >= mandate.expires_at {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_expired",
                });
            }
            mandate.state = MandateState::Active;
            (mandate.agent, mandate.owner, mandate.policies.clone())
        };
        for policy_id in policy_ids {
            self.policy_store
                .resume(policy_id)
                .map_err(NodeError::Policy)?;
        }
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MandateResumed {
                mandate_id,
                agent,
                by: owner,
                resumed_at_ms: timestamp_ms,
            },
        );
        Ok(self
            .mandates
            .get(&mandate_id)
            .expect("mandate existence checked before resume")
            .clone())
    }

    pub fn revoke_mandate(
        &mut self,
        mandate_id: MandateId,
        reason: String,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        let (agent, owner, policy_ids) = {
            let mandate = self
                .mandates
                .get_mut(&mandate_id)
                .ok_or(NodeError::InvalidMandate {
                    agent: 0,
                    reason: "unknown_mandate",
                })?;
            if mandate.state.is_terminal() {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_terminal",
                });
            }
            mandate.state = MandateState::Revoked;
            (mandate.agent, mandate.owner, mandate.policies.clone())
        };
        let block_number = self.head().header.height;
        let policy_events = self.policy_revocation_events(&policy_ids, agent)?;
        for policy_id in policy_ids {
            self.policy_store
                .revoke(policy_id, block_number, timestamp_ms)
                .map_err(NodeError::Policy)?;
        }
        self.record_runtime_event(
            None,
            timestamp_ms,
            RuntimeEventKind::MandateRevoked {
                mandate_id,
                agent,
                by: owner,
                reason,
                revoked_at_ms: timestamp_ms,
            },
        );
        for event in policy_events {
            self.record_runtime_event(None, timestamp_ms, event);
        }
        self.clear_mandate_resting_order_metadata(mandate_id);
        self.clear_mandate_derived_indexes(mandate_id);
        Ok(self
            .mandates
            .get(&mandate_id)
            .expect("mandate existence checked before revoke")
            .clone())
    }

    pub fn breach_mandate(
        &mut self,
        mandate_id: MandateId,
        constraint_index: u16,
        timestamp_ms: u64,
    ) -> Result<Mandate, NodeError> {
        self.breach_mandate_with_block_height(mandate_id, constraint_index, timestamp_ms, None)
    }

    fn breach_mandate_with_block_height(
        &mut self,
        mandate_id: MandateId,
        constraint_index: u16,
        timestamp_ms: u64,
        block_height: Option<u64>,
    ) -> Result<Mandate, NodeError> {
        let (agent, policy_ids) = {
            let mandate = self
                .mandates
                .get_mut(&mandate_id)
                .ok_or(NodeError::InvalidMandate {
                    agent: 0,
                    reason: "unknown_mandate",
                })?;
            if mandate.state.is_terminal() {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_terminal",
                });
            }
            if usize::from(constraint_index) >= mandate.constraints.len() {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "invalid_constraint_index",
                });
            }
            mandate.state = MandateState::Breached;
            (mandate.agent, mandate.policies.clone())
        };
        let block_number = self.head().header.height;
        let policy_events = self.policy_revocation_events(&policy_ids, agent)?;
        for policy_id in policy_ids {
            self.policy_store
                .revoke(policy_id, block_number, timestamp_ms)
                .map_err(NodeError::Policy)?;
        }
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::MandateConstraintTriggered {
                mandate_id,
                agent,
                constraint_index,
                action: MandateEnforcementAction::BreachApplied,
                triggered_at_ms: timestamp_ms,
            },
        );
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::MandateBreached {
                mandate_id,
                agent,
                constraint_index,
                detected_at_ms: timestamp_ms,
            },
        );
        for event in policy_events {
            self.record_runtime_event(block_height, timestamp_ms, event);
        }
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::MandateConstraintTriggered {
                mandate_id,
                agent,
                constraint_index,
                action: MandateEnforcementAction::PoliciesCascadeRevoked,
                triggered_at_ms: timestamp_ms,
            },
        );
        self.clear_mandate_resting_order_metadata(mandate_id);
        self.clear_mandate_derived_indexes(mandate_id);
        Ok(self
            .mandates
            .get(&mandate_id)
            .expect("mandate existence checked before breach")
            .clone())
    }

    pub(crate) fn evaluate_post_mode_mandates_at_head(
        &mut self,
    ) -> Result<Vec<Mandate>, NodeError> {
        let head = self.head();
        let timestamp_ms = head.header.timestamp_ms;
        let block_height = Some(head.header.height);
        self.prune_mandate_daily_pnl_baselines(timestamp_ms);
        let breaches = self.post_mode_mandate_breaches();
        let mut breached = Vec::with_capacity(breaches.len());
        for (mandate_id, constraint_index) in breaches {
            breached.push(self.breach_mandate_with_block_height(
                mandate_id,
                constraint_index,
                timestamp_ms,
                block_height,
            )?);
        }
        Ok(breached)
    }

    fn prune_mandate_daily_pnl_baselines(&mut self, timestamp_ms: u64) {
        let current_day = utc_day_bucket_ms(timestamp_ms);
        let min_retained_day = current_day.saturating_sub(UTC_DAY_MS);
        self.mandate_daily_pnl_baselines
            .retain(|(_, day_bucket_ms), _| *day_bucket_ms >= min_retained_day);
    }

    pub(crate) fn evaluate_mandate_objectives_at_head(&mut self) {
        let head = self.head();
        let timestamp_ms = head.header.timestamp_ms;
        let block_height = Some(head.header.height);
        let evaluations = self.mandate_objective_evaluations(timestamp_ms);
        let mut touched_mandates = BTreeSet::new();
        for evaluation in evaluations {
            let next_state = MandateObjectiveEventState {
                score: evaluation.score,
                conformance_score: evaluation.conformance_score,
                pending_objectives: evaluation.pending_objectives,
            };
            if self
                .mandate_objective_event_cache
                .get(&(evaluation.mandate_id, evaluation.objective_index))
                == Some(&next_state)
            {
                continue;
            }
            if let Some(mandate) = self.mandates.get_mut(&evaluation.mandate_id) {
                mandate.conformance.score = evaluation.conformance_score;
                mandate.conformance.pending_objectives = evaluation.pending_objectives;
                mandate.conformance.last_evaluated_at_ms = timestamp_ms;
                touched_mandates.insert(evaluation.mandate_id);
            }
            self.record_runtime_event(
                block_height,
                timestamp_ms,
                RuntimeEventKind::MandateObjectiveEvaluated {
                    mandate_id: evaluation.mandate_id,
                    agent: evaluation.agent,
                    objective_index: evaluation.objective_index,
                    score: evaluation.score,
                    conformance_score: evaluation.conformance_score,
                    pending_objectives: evaluation.pending_objectives,
                    evaluated_at_ms: timestamp_ms,
                },
            );
        }
        for mandate_id in touched_mandates {
            self.record_mandate_conformance_sample_at_head(mandate_id);
        }
    }

    fn mandate_objective_evaluations(&self, timestamp_ms: u64) -> Vec<MandateObjectiveEvaluation> {
        let mut evaluations = Vec::new();
        for mandate in self.mandates.values() {
            if !matches!(
                mandate.state,
                MandateState::Active | MandateState::Suspended
            ) {
                continue;
            }
            if mandate.objectives.is_empty() {
                continue;
            }
            let mut scored = Vec::with_capacity(mandate.objectives.len());
            let mut pending_objectives = 0_u16;
            for (index, objective) in mandate.objectives.iter().enumerate() {
                let (score, pending) =
                    self.evaluate_mandate_objective(mandate, objective, timestamp_ms);
                if pending {
                    pending_objectives = pending_objectives.saturating_add(1);
                }
                scored.push((u16::try_from(index).unwrap_or(u16::MAX), score));
            }
            let conformance_score = aggregate_objective_scores(&scored, &mandate.objective_weights);
            for (objective_index, score) in scored {
                evaluations.push(MandateObjectiveEvaluation {
                    mandate_id: mandate.id,
                    agent: mandate.agent,
                    objective_index,
                    score,
                    conformance_score,
                    pending_objectives,
                });
            }
        }
        evaluations
    }

    fn evaluate_mandate_objective(
        &self,
        mandate: &Mandate,
        objective: &MandateObjective,
        timestamp_ms: u64,
    ) -> (u8, bool) {
        let window_ms =
            mandate_objective_window_ms(objective).unwrap_or(mandate.conformance_window_ms);
        let Some(activated_at) = mandate.activated_at else {
            return (50, true);
        };
        if timestamp_ms < activated_at.saturating_add(window_ms) {
            return (50, true);
        }
        match objective {
            MandateObjective::MinTradeCount { count, over_ms } => {
                let observed = self.mandate_trade_count(mandate.id, timestamp_ms, *over_ms);
                if observed >= u64::from(*count) {
                    (100, false)
                } else {
                    (
                        observed
                            .saturating_mul(100)
                            .checked_div(u64::from(*count).max(1))
                            .unwrap_or(0)
                            .min(100) as u8,
                        false,
                    )
                }
            }
            MandateObjective::MaxTradeCount { count, over_ms } => {
                let observed = self.mandate_trade_count(mandate.id, timestamp_ms, *over_ms);
                if observed <= u64::from(*count) {
                    (100, false)
                } else {
                    (
                        u64::from(*count)
                            .saturating_mul(100)
                            .checked_div(observed.max(1))
                            .unwrap_or(0)
                            .min(100) as u8,
                        false,
                    )
                }
            }
            // Richer objectives need additional per-action metric state. Until
            // that lands, they evaluate to the documented neutral score.
            MandateObjective::TargetSharpe { .. }
            | MandateObjective::TargetReturnPct { .. }
            | MandateObjective::MaxDrawdownPct { .. }
            | MandateObjective::MaxSlippageBps { .. }
            | MandateObjective::MaxFundingPaidPctOfEquity { .. } => (50, false),
        }
    }

    fn mandate_trade_count(&self, mandate_id: MandateId, timestamp_ms: u64, over_ms: u64) -> u64 {
        let from_ms = timestamp_ms.saturating_sub(over_ms);
        let Some(timestamps) = self.mandate_fill_timestamps.get(&mandate_id) else {
            return 0;
        };
        let start = timestamps.partition_point(|entry| *entry < from_ms);
        let end = timestamps.partition_point(|entry| *entry <= timestamp_ms);
        end.saturating_sub(start) as u64
    }

    pub(crate) fn prune_all_mandate_fill_timestamps(&mut self, timestamp_ms: u64) {
        let retention_by_mandate: BTreeMap<MandateId, Option<u64>> = self
            .mandates
            .iter()
            .map(|(mandate_id, mandate)| (*mandate_id, mandate_trade_count_window_ms(mandate)))
            .collect();
        self.mandate_fill_timestamps
            .retain(|mandate_id, timestamps| {
                let Some(Some(window_ms)) = retention_by_mandate.get(mandate_id) else {
                    return false;
                };
                prune_timestamps_before(timestamps, timestamp_ms.saturating_sub(*window_ms));
                !timestamps.is_empty()
            });
    }

    fn clear_mandate_derived_indexes(&mut self, mandate_id: MandateId) {
        self.mandate_fill_timestamps.remove(&mandate_id);
        self.mandate_objective_event_cache
            .retain(|(entry_mandate_id, _), _| *entry_mandate_id != mandate_id);
        self.mandate_realized_pnl.remove(&mandate_id);
        self.mandate_daily_pnl_baselines
            .retain(|(entry_mandate_id, _), _| *entry_mandate_id != mandate_id);
    }

    fn clear_mandate_resting_order_metadata(&mut self, mandate_id: MandateId) {
        self.resting_order_mandates
            .retain(|_, resting_mandate_id| *resting_mandate_id != mandate_id);
    }

    pub(crate) fn sweep_resting_order_mandates_at(&mut self, timestamp_ms: u64) {
        self.resting_order_mandates.retain(
            |(market_id, account_id, client_order_id), mandate_id| {
                self.mandates
                    .get(mandate_id)
                    .is_some_and(|mandate| mandate.state == MandateState::Active)
                    && self
                        .order_books
                        .get(market_id)
                        .and_then(|book| {
                            book.resting_order(*account_id, *client_order_id, timestamp_ms)
                        })
                        .is_some()
            },
        );
    }

    fn post_mode_mandate_breaches(&mut self) -> Vec<(MandateId, u16)> {
        let mut breaches = Vec::new();
        let timestamp_ms = self.head().header.timestamp_ms;
        let mandates: Vec<Mandate> = self.mandates.values().cloned().collect();
        for mandate in &mandates {
            if mandate.state != MandateState::Active {
                continue;
            }
            for (index, constraint) in mandate.constraints.iter().enumerate() {
                if self.post_mode_constraint_breached(mandate, constraint, timestamp_ms) {
                    breaches.push((mandate.id, u16::try_from(index).unwrap_or(u16::MAX)));
                    break;
                }
            }
        }
        breaches
    }

    fn post_mode_constraint_breached(
        &mut self,
        mandate: &Mandate,
        constraint: &MandateConstraint,
        timestamp_ms: u64,
    ) -> bool {
        match constraint {
            MandateConstraint::MaxConcentrationInMarketPct { market, pct_bps } => {
                self.mandate_market_concentration_breached(mandate.id, *market, *pct_bps)
            }
            MandateConstraint::MaxDailyLossUsdc { limit_usdc } => {
                self.mandate_daily_loss_breached(mandate.id, *limit_usdc, timestamp_ms)
            }
            MandateConstraint::NoPositionHeldLongerThan { duration_ms } => self
                .mandate_position_holding_period_breached(mandate.id, *duration_ms, timestamp_ms),
            _ => false,
        }
    }

    fn mandate_daily_loss_breached(
        &mut self,
        mandate_id: MandateId,
        limit_usdc: MandateU256,
        timestamp_ms: u64,
    ) -> bool {
        let Ok(limit) = mandate_u256_to_u128(limit_usdc) else {
            return true;
        };
        let day_bucket_ms = utc_day_bucket_ms(timestamp_ms);
        let Some(current_pnl) = self.mandate_current_pnl_usdc(mandate_id) else {
            return true;
        };
        let baseline = self
            .mandate_daily_pnl_baselines
            .entry((mandate_id, day_bucket_ms))
            .or_insert(current_pnl);
        if current_pnl >= *baseline {
            return false;
        }
        i128_abs_to_u128_saturating(baseline.saturating_sub(current_pnl)) > limit
    }

    fn mandate_current_pnl_usdc(&self, mandate_id: MandateId) -> Option<i128> {
        let mut total = self
            .mandate_realized_pnl
            .get(&mandate_id)
            .copied()
            .unwrap_or(0);
        for ((market_id, _), ledger) in &self.positions {
            if ledger.open_abs_lots() == 0 || ledger.mandate_id != Some(mandate_id) {
                continue;
            }
            let market = self.markets.get(market_id)?;
            let unrealized_raw = unrealized_pnl(
                ledger,
                self.market_marks
                    .get(market_id)
                    .map(|price| price.price_ticks),
            );
            total =
                total.saturating_add(scaled_signed_raw_tick_lot_pnl_usdc(market, unrealized_raw));
        }
        Some(total)
    }

    pub(crate) fn add_mandate_realized_pnl(&mut self, mandate_id: MandateId, delta_usdc: i128) {
        if delta_usdc == 0 {
            return;
        }
        let entry = self.mandate_realized_pnl.entry(mandate_id).or_insert(0);
        *entry = entry.saturating_add(delta_usdc);
    }

    fn mandate_position_holding_period_breached(
        &self,
        mandate_id: MandateId,
        duration_ms: u64,
        timestamp_ms: u64,
    ) -> bool {
        self.positions
            .iter()
            .filter(|(_, ledger)| {
                ledger.open_abs_lots() > 0 && ledger.mandate_id == Some(mandate_id)
            })
            .any(|(_, ledger)| {
                ledger
                    .opened_at_timestamp_ms
                    .is_none_or(|opened_at| timestamp_ms > opened_at.saturating_add(duration_ms))
            })
    }

    fn mandate_market_concentration_breached(
        &self,
        mandate_id: MandateId,
        target_market: u64,
        pct_bps: u32,
    ) -> bool {
        let mut total_notional = 0_u128;
        let mut target_notional = 0_u128;
        for ((market_id, _), ledger) in &self.positions {
            if ledger.open_abs_lots() == 0 || ledger.mandate_id != Some(mandate_id) {
                continue;
            }
            let notional = self.position_scaled_mark_notional(*market_id, ledger);
            total_notional = total_notional.saturating_add(notional);
            if *market_id == target_market {
                target_notional = target_notional.saturating_add(notional);
            }
        }
        if total_notional == 0 || target_notional == 0 {
            return false;
        }
        target_notional.saturating_mul(BPS_DENOMINATOR)
            > total_notional.saturating_mul(u128::from(pct_bps))
    }

    fn position_scaled_mark_notional(&self, market_id: u64, ledger: &PositionLedger) -> u128 {
        let lots = ledger.open_abs_lots();
        let raw_tick_lots = match self.mark_price_ticks(market_id) {
            Some(mark_price) => u128::from(mark_price).saturating_mul(u128::from(lots)),
            // Without an oracle mark, fall back to entry-weighted exposure in
            // the same raw tick-lot units.
            None => ledger.open_notional,
        };
        let Some(market) = self.markets.get(&market_id) else {
            return 0;
        };
        // Concentration compares across markets, so raw tick-lot units must
        // be weighted by each market's tick and lot scale first.
        raw_tick_lots
            .saturating_mul(u128::from(market.config.tick_size_bps))
            .saturating_mul(u128::from(market.config.lot_size_microunits))
    }

    pub fn expire_due_mandates(&mut self, timestamp_ms: u64) -> Result<Vec<Mandate>, NodeError> {
        // Operator-triggered expiry is intentionally not block-attributed; the
        // commit-path expiry below records the concrete head height.
        self.expire_due_mandates_with_block_height(timestamp_ms, None)
    }

    pub(crate) fn expire_due_mandates_at_head(&mut self) -> Result<Vec<Mandate>, NodeError> {
        let head = self.head();
        self.expire_due_mandates_with_block_height(
            head.header.timestamp_ms,
            Some(head.header.height),
        )
    }

    pub(crate) fn crystallize_due_delegations_at_head(
        &mut self,
    ) -> Result<Vec<DelegationCrystallization>, NodeError> {
        let head = self.head();
        self.crystallize_due_delegations_with_block_height(
            head.header.timestamp_ms,
            Some(head.header.height),
        )
    }

    fn expire_due_mandates_with_block_height(
        &mut self,
        timestamp_ms: u64,
        block_height: Option<u64>,
    ) -> Result<Vec<Mandate>, NodeError> {
        let due: Vec<_> = self
            .mandates
            .values()
            .filter(|mandate| {
                matches!(
                    mandate.state,
                    MandateState::Active | MandateState::Suspended
                ) && timestamp_ms >= mandate.expires_at
            })
            .map(|mandate| mandate.id)
            .collect();
        let mut expired = Vec::with_capacity(due.len());
        for mandate_id in due {
            expired.push(self.expire_mandate(mandate_id, timestamp_ms, block_height)?);
        }
        Ok(expired)
    }

    fn expire_mandate(
        &mut self,
        mandate_id: MandateId,
        timestamp_ms: u64,
        block_height: Option<u64>,
    ) -> Result<Mandate, NodeError> {
        let (agent, policy_ids) = {
            let mandate = self
                .mandates
                .get_mut(&mandate_id)
                .ok_or(NodeError::InvalidMandate {
                    agent: 0,
                    reason: "unknown_mandate",
                })?;
            if mandate.state.is_terminal() {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_terminal",
                });
            }
            if timestamp_ms < mandate.expires_at {
                return Err(NodeError::InvalidMandate {
                    agent: mandate.agent,
                    reason: "mandate_not_expired",
                });
            }
            mandate.state = MandateState::Expired;
            (mandate.agent, mandate.policies.clone())
        };
        let block_number = self.head().header.height;
        for policy_id in policy_ids {
            self.policy_store
                .expire(policy_id, block_number, timestamp_ms)
                .map_err(NodeError::Policy)?;
        }
        self.record_runtime_event(
            block_height,
            timestamp_ms,
            RuntimeEventKind::MandateExpired {
                mandate_id,
                agent,
                expired_at_ms: timestamp_ms,
            },
        );
        self.clear_mandate_resting_order_metadata(mandate_id);
        self.clear_mandate_derived_indexes(mandate_id);
        Ok(self
            .mandates
            .get(&mandate_id)
            .expect("mandate existence checked before expire")
            .clone())
    }

    pub(crate) fn policy_revocation_events(
        &self,
        policy_ids: &[u64],
        agent: u64,
    ) -> Result<Vec<RuntimeEventKind>, NodeError> {
        let mut seen = BTreeSet::new();
        let mut events = Vec::new();
        for policy_id in policy_ids {
            // Lifecycle revocations affect only the rooted policy graph under
            // the supplied ids. Other roots remain independent even if they
            // share the same authority account.
            let scope = self
                .policy_store
                .revocation_scope(*policy_id)
                .map_err(NodeError::Policy)?;
            for (index, scoped_policy_id) in scope.into_iter().enumerate() {
                let Some(policy) = self.policy_store.get(scoped_policy_id) else {
                    continue;
                };
                if matches!(policy.state, PolicyState::Revoked | PolicyState::Expired) {
                    continue;
                }
                if seen.insert(scoped_policy_id) {
                    events.push(RuntimeEventKind::PolicyRevoked {
                        account_id: agent,
                        policy_id: scoped_policy_id,
                        cascaded: index > 0,
                    });
                }
            }
        }
        Ok(events)
    }

    // RFC 026 — Performance-Share Delegation.
}
