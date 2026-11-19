use super::*;

impl SingleNodeEngine {
    pub(crate) fn rebuild_block_event_index(&mut self) {
        let mut first_seq_by_height: BTreeMap<u64, u64> = BTreeMap::new();
        let mut count_by_height: BTreeMap<u64, u64> = BTreeMap::new();
        for event in &self.runtime_events {
            let Some(block_height) = event.block_height else {
                continue;
            };
            first_seq_by_height
                .entry(block_height)
                .and_modify(|seq| *seq = (*seq).min(event.seq))
                .or_insert(event.seq);
            count_by_height
                .entry(block_height)
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }

        self.block_event_index.clear();
        let mut next_lower_bound = self.next_runtime_event_seq;
        for block in self.chain.iter().rev() {
            let height = block.header.height;
            let count = count_by_height.get(&height).copied().unwrap_or(0);
            let lower_bound_seq = if let Some(first_seq) = first_seq_by_height.get(&height).copied()
            {
                next_lower_bound = first_seq;
                first_seq
            } else {
                next_lower_bound
            };
            self.block_event_index.insert(
                height,
                BlockEventCursor {
                    lower_bound_seq,
                    count,
                },
            );
        }
    }

    pub(crate) fn note_committed_block(&mut self, block_height: u64) {
        let lower_bound_seq = self.next_runtime_event_seq;
        self.block_event_index
            .entry(block_height)
            .or_insert(BlockEventCursor {
                lower_bound_seq,
                count: 0,
            });

        let empty_predecessors = self
            .block_event_index
            .range(..block_height)
            .rev()
            .take_while(|(_, cursor)| cursor.count == 0)
            .map(|(height, _)| *height)
            .collect::<Vec<_>>();
        for height in empty_predecessors {
            if let Some(cursor) = self.block_event_index.get_mut(&height) {
                if cursor.lower_bound_seq < lower_bound_seq {
                    cursor.lower_bound_seq = lower_bound_seq;
                }
            }
        }
    }

    pub(crate) fn note_committed_event_seq(&mut self, block_height: u64, seq: u64) {
        let entry = self
            .block_event_index
            .entry(block_height)
            .or_insert(BlockEventCursor {
                lower_bound_seq: seq,
                count: 0,
            });
        if entry.count == 0 || seq < entry.lower_bound_seq {
            entry.lower_bound_seq = seq;
        }
        entry.count = entry.count.saturating_add(1);

        let empty_predecessors = self
            .block_event_index
            .range(..block_height)
            .rev()
            .take_while(|(_, cursor)| cursor.count == 0)
            .map(|(height, _)| *height)
            .collect::<Vec<_>>();
        for height in empty_predecessors {
            if let Some(cursor) = self.block_event_index.get_mut(&height) {
                if cursor.lower_bound_seq > seq {
                    cursor.lower_bound_seq = seq;
                }
            }
        }
    }

    pub(crate) fn rebuild_latest_equity_sample_index(&mut self) {
        self.latest_equity_sample_by_account.clear();
        for ((account_id, _), sample) in &self.equity_history {
            self.latest_equity_sample_by_account
                .entry(*account_id)
                .and_modify(|current| {
                    if (sample.timestamp_ms, sample.block_height)
                        > (current.timestamp_ms, current.block_height)
                    {
                        *current = *sample;
                    }
                })
                .or_insert(*sample);
        }
    }

    pub(crate) fn rebuild_latest_mandate_conformance_sample_index(&mut self) {
        self.latest_conformance_sample_by_mandate.clear();
        for ((mandate_id, _, _), sample) in &self.mandate_conformance_history {
            self.latest_conformance_sample_by_mandate
                .entry(*mandate_id)
                .and_modify(|current| {
                    if (sample.evaluated_at_ms, sample.block_height)
                        > (current.evaluated_at_ms, current.block_height)
                    {
                        *current = *sample;
                    }
                })
                .or_insert(*sample);
        }
    }

    pub(crate) fn record_mandate_conformance_sample_at_head(&mut self, mandate_id: MandateId) {
        let block_height = self.head().header.height;
        let timestamp_ms = self.head().header.timestamp_ms;
        let Some(mandate) = self.mandates.get(&mandate_id) else {
            return;
        };
        let previous = self
            .latest_conformance_sample_by_mandate
            .get(&mandate_id)
            .copied();
        let current_key = MandateConformanceSampleChangeKey {
            score: mandate.conformance.score,
            pending_objectives: mandate.conformance.pending_objectives,
            state: mandate.state,
        };
        if previous
            .as_ref()
            .is_some_and(|sample| MandateConformanceSampleChangeKey::from(sample) == current_key)
        {
            return;
        }
        let prev_score = previous.map(|sample| sample.score);
        let delta = prev_score.map(|score| i16::from(mandate.conformance.score) - i16::from(score));
        let sample = MandateConformanceSample {
            mandate_id,
            block_height,
            evaluated_at_ms: timestamp_ms,
            score: mandate.conformance.score,
            pending_objectives: mandate.conformance.pending_objectives,
            state: mandate.state,
            prev_score,
            delta,
        };
        let agent = mandate.agent;
        self.mandate_conformance_history.insert(
            (mandate_id, sample.evaluated_at_ms, sample.block_height),
            sample,
        );
        self.latest_conformance_sample_by_mandate
            .insert(mandate_id, sample);
        self.prune_mandate_conformance_history(mandate_id);
        self.record_runtime_event(
            Some(block_height),
            timestamp_ms,
            RuntimeEventKind::MandateConformanceEvaluated {
                mandate_id,
                agent,
                evaluated_at_ms: timestamp_ms,
                score: sample.score,
                pending_objectives: sample.pending_objectives,
                state: mandate_state_event_name(sample.state).to_string(),
                prev_score: sample.prev_score,
                delta: sample.delta,
            },
        );
    }

    pub(crate) fn prune_mandate_conformance_history(&mut self, mandate_id: MandateId) {
        let excess = self
            .mandate_conformance_history
            .range((mandate_id, 0, 0)..=(mandate_id, u64::MAX, u64::MAX))
            .count()
            .saturating_sub(MANDATE_CONFORMANCE_HISTORY_LIMIT);
        if excess == 0 {
            return;
        }
        let keys = self
            .mandate_conformance_history
            .range((mandate_id, 0, 0)..=(mandate_id, u64::MAX, u64::MAX))
            .take(excess)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        for key in keys {
            self.mandate_conformance_history.remove(&key);
        }
        self.rebuild_latest_mandate_conformance_sample_index();
    }

    fn active_equity_sample_accounts(&self, block_height: u64, timestamp_ms: u64) -> BTreeSet<u64> {
        let mut accounts = BTreeSet::new();
        for ((_, account_id), ledger) in &self.positions {
            if ledger.net_lots() != 0 {
                accounts.insert(*account_id);
            }
        }
        for ((_, account_id), collateral) in &self.isolated_collateral {
            if *collateral != 0 {
                accounts.insert(*account_id);
            }
        }
        for (market_id, account_id) in self.under_maintenance_accounts.iter().copied() {
            let margin = self.margin_snapshot(market_id, account_id);
            if margin.maintenance_requirement > 0
                || liquidation_gap_for_equity_sample(margin.equity, margin.maintenance_requirement)
                    > 0
            {
                accounts.insert(account_id);
            }
        }
        if let Some(cursor) = self.block_event_index.get(&block_height) {
            if cursor.count > 0 {
                for event in self.runtime_events.iter().rev() {
                    if event.seq < cursor.lower_bound_seq {
                        break;
                    }
                    if event.block_height != Some(block_height) {
                        continue;
                    }
                    if let RuntimeEventKind::OrderFilled { account_id, .. } = &event.kind {
                        accounts.insert(*account_id);
                    }
                }
            }
        }

        let active_tail_cutoff = timestamp_ms.saturating_sub(ACCOUNT_EQUITY_HISTORY_ACTIVE_TAIL_MS);
        for (account_id, sample) in &self.latest_equity_sample_by_account {
            if sample.timestamp_ms >= active_tail_cutoff {
                accounts.insert(*account_id);
            }
        }
        accounts
    }

    fn account_equity_sample(
        &self,
        account_id: u64,
        block_height: u64,
        timestamp_ms: u64,
    ) -> AccountEquitySample {
        let mut sample = AccountEquitySample {
            block_height,
            timestamp_ms,
            ..AccountEquitySample::default()
        };

        for (market_id, ledger) in self.risk_positions(account_id, false, usize::MAX) {
            let margin = self.margin_snapshot(market_id, account_id);
            sample.total_collateral = sample.total_collateral.saturating_add(margin.collateral);
            sample.total_realized_pnl = sample
                .total_realized_pnl
                .saturating_add(margin.realized_pnl);
            sample.total_unrealized_pnl = sample
                .total_unrealized_pnl
                .saturating_add(margin.unrealized_pnl);
            sample.total_equity = sample.total_equity.saturating_add(margin.equity);
            sample.total_maintenance_requirement = sample
                .total_maintenance_requirement
                .saturating_add(margin.maintenance_requirement);
            sample.total_liquidation_gap =
                sample
                    .total_liquidation_gap
                    .saturating_add(liquidation_gap_for_equity_sample(
                        margin.equity,
                        margin.maintenance_requirement,
                    ));
            sample.total_margin_surplus = sample
                .total_margin_surplus
                .saturating_add(margin.margin_surplus);
            if ledger.net_lots() != 0 {
                sample.position_count = sample.position_count.saturating_add(1);
            }
            sample.under_maintenance |= margin.under_maintenance;
        }

        sample
    }

    fn should_persist_equity_sample(&self, account_id: u64, sample: &AccountEquitySample) -> bool {
        let Some(previous) = self.latest_equity_sample_by_account.get(&account_id) else {
            return true;
        };
        if AccountEquitySampleChangeKey::from(previous)
            != AccountEquitySampleChangeKey::from(sample)
        {
            return true;
        }
        sample.timestamp_ms.saturating_sub(previous.timestamp_ms)
            >= ACCOUNT_EQUITY_HISTORY_IDLE_SAMPLE_MS
    }

    pub(crate) fn sample_account_equity_at_head(&mut self) {
        let block_height = self.head().header.height;
        let timestamp_ms = self.head().header.timestamp_ms;
        let accounts = self.active_equity_sample_accounts(block_height, timestamp_ms);
        for account_id in accounts {
            let sample = self.account_equity_sample(account_id, block_height, timestamp_ms);
            if self.should_persist_equity_sample(account_id, &sample) {
                self.equity_history
                    .insert((account_id, sample.block_height), sample);
                self.latest_equity_sample_by_account
                    .insert(account_id, sample);
            }
        }
        self.prune_account_equity_history(timestamp_ms);
    }

    pub(crate) fn prune_account_equity_history(&mut self, timestamp_ms: u64) {
        let dense_cutoff = timestamp_ms.saturating_sub(ACCOUNT_EQUITY_HISTORY_DENSE_WINDOW_MS);
        let tail_cutoff = dense_cutoff.saturating_sub(ACCOUNT_EQUITY_HISTORY_DAILY_WINDOW_MS);
        let mut daily_retained = BTreeMap::<(u64, u64), (u64, u64)>::new();

        for ((account_id, block_height), sample) in &self.equity_history {
            if sample.timestamp_ms >= dense_cutoff || sample.timestamp_ms < tail_cutoff {
                continue;
            }
            let day = utc_day_bucket_ms(sample.timestamp_ms);
            daily_retained
                .entry((*account_id, day))
                .and_modify(|current| {
                    if (sample.timestamp_ms, *block_height) > (current.0, current.1) {
                        *current = (sample.timestamp_ms, *block_height);
                    }
                })
                .or_insert((sample.timestamp_ms, *block_height));
        }

        self.equity_history
            .retain(|(account_id, block_height), sample| {
                if sample.timestamp_ms >= dense_cutoff {
                    return true;
                }
                if sample.timestamp_ms < tail_cutoff {
                    return false;
                }
                let day = utc_day_bucket_ms(sample.timestamp_ms);
                daily_retained
                    .get(&(*account_id, day))
                    .is_some_and(|(_, retained_height)| retained_height == block_height)
            });
        self.rebuild_latest_equity_sample_index();
    }
}
