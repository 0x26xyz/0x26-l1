use crate::*;
use std::thread;
use std::time::{Duration, Instant};

impl JsonRpcHttpServer {
    pub fn maybe_run_mahi_round(
        &self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> Result<Option<(u64, usize, usize)>, String> {
        if !self.config.mahi_enabled {
            return Ok(None);
        }
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(None);
        };
        let producer_profile = mahi_producer_profile_enabled();
        let producer_total_start = producer_profile.then(Instant::now);
        let mut producer_initial_lock_wait = Duration::ZERO;
        let mut producer_initial_lock_hold = Duration::ZERO;
        let mut producer_snapshot = Duration::ZERO;
        let mut producer_pre_admitted_drain = Duration::ZERO;
        let mut producer_final_lock_wait = Duration::ZERO;
        let (
            target_round,
            restart_boundary_round,
            epoch,
            ancestors,
            inflight_intents,
            inflight_faucets,
            inflight_real_capital_governance_actions,
            inflight_delegation_actions,
            inflight_market_maker_program_actions,
            inflight_tee_governance_actions,
            inflight_policy_nonces,
        ) = {
            let _dag = self.engine.dag_domain().track_lock();
            let initial_lock_wait_start = Instant::now();
            let mut state_guard = self
                .mahi_state
                .lock()
                .map_err(|_| "mahi lock poisoned".to_string())?;
            if producer_profile {
                producer_initial_lock_wait = initial_lock_wait_start.elapsed();
            }
            let initial_lock_hold_start = Instant::now();
            let Some(state) = state_guard.as_mut() else {
                return Ok(None);
            };
            state.producer_attempts = state.producer_attempts.saturating_add(1);
            let round = state.next_local_round;
            let ancestors = state.round_ancestors(round);
            let quorum = quorum_threshold(state.validator_count()).unwrap_or(usize::MAX);
            if round > state.restart_boundary_round && ancestors.len() < quorum {
                state.producer_skip_ancestor_quorum =
                    state.producer_skip_ancestor_quorum.saturating_add(1);
                let now = Instant::now();
                match state.producer_ancestor_wait.as_mut() {
                    Some(wait) if wait.target_round == round => {
                        wait.available = ancestors.len();
                        wait.quorum = quorum;
                    }
                    _ => {
                        state.producer_ancestor_wait = Some(MahiAncestorWait {
                            target_round: round,
                            quorum,
                            available: ancestors.len(),
                            since: now,
                        });
                    }
                }
                let should_reset_from_durable_head =
                    state.producer_ancestor_wait.as_ref().is_some_and(|wait| {
                        wait.target_round == round
                            && now.saturating_duration_since(wait.since)
                                >= MAHI_ANCESTOR_QUORUM_SELF_HEAL_TIMEOUT
                    });
                if should_reset_from_durable_head {
                    drop(state_guard);
                    self.reset_mahi_state_from_durable_head("ancestor_quorum_stall");
                }
                return Ok(None);
            }
            let result = (
                round,
                state.restart_boundary_round,
                state.epoch,
                ancestors,
                state.inflight_intents.clone(),
                state.inflight_faucets.clone(),
                state.inflight_real_capital_governance_actions.clone(),
                state.inflight_delegation_actions.clone(),
                state.inflight_market_maker_program_actions.clone(),
                state.inflight_tee_governance_actions.clone(),
                state.inflight_policy_nonces.clone(),
            );
            if producer_profile {
                producer_initial_lock_hold = initial_lock_hold_start.elapsed();
            }
            result
        };
        if target_round == restart_boundary_round {
            let expected_peers = network_bootstrap
                .validator_set
                .active_validators()
                .len()
                .saturating_sub(1);
            let reachable_peers = self
                .network_peer_reachability()
                .map(|summary| summary.reachable_peer_count)
                .unwrap_or(0);
            if reachable_peers < expected_peers {
                let _dag = self.engine.dag_domain().track_lock();
                if let Ok(mut state_guard) = self.mahi_state.lock() {
                    if let Some(state) = state_guard.as_mut() {
                        state.producer_skip_peer_reachability =
                            state.producer_skip_peer_reachability.saturating_add(1);
                    }
                }
                return Ok(None);
            }
        }
        let snapshot_start = Instant::now();
        let snapshot = self
            .engine
            .mahi_snapshot(timestamp_ms, max_items, &inflight_policy_nonces)
            .map_err(|_| "engine lock poisoned".to_string())?;
        if producer_profile {
            producer_snapshot = snapshot_start.elapsed();
        }
        let Some(mut snapshot) = snapshot else {
            let _dag = self.engine.dag_domain().track_lock();
            if let Ok(mut state_guard) = self.mahi_state.lock() {
                if let Some(state) = state_guard.as_mut() {
                    state.producer_skip_snapshot_unavailable =
                        state.producer_skip_snapshot_unavailable.saturating_add(1);
                }
            }
            return Ok(None);
        };
        snapshot
            .intents
            .retain(|intent| !inflight_intents.contains(&intent_id(intent)));
        if snapshot.intents.len() < max_items {
            let pre_admitted_drain_start = Instant::now();
            let remaining = max_items.saturating_sub(snapshot.intents.len());
            let excluded_intents = snapshot
                .intents
                .iter()
                .map(intent_id)
                .chain(inflight_intents.iter().copied())
                .collect::<BTreeSet<_>>();
            let pre_admitted = self.drain_mahi_pre_admitted_for_block(
                timestamp_ms,
                remaining,
                &inflight_policy_nonces,
                &excluded_intents,
            );
            snapshot.intents.extend(pre_admitted);
            if producer_profile {
                producer_pre_admitted_drain = pre_admitted_drain_start.elapsed();
            }
        }
        if snapshot.intents.len() < max_items {
            let remaining = max_items.saturating_sub(snapshot.intents.len());
            snapshot.faucets = self
                .engine
                .pending_faucet_payload(remaining)
                .map_err(|_| "engine lock poisoned".to_string())?;
            snapshot
                .faucets
                .retain(|faucet| !inflight_faucets.contains(&faucet.address));
        }
        if snapshot
            .intents
            .len()
            .saturating_add(snapshot.faucets.len())
            < max_items
        {
            let remaining =
                max_items.saturating_sub(snapshot.intents.len() + snapshot.faucets.len());
            snapshot.real_capital_governance = self
                .engine
                .pending_real_capital_governance_payload(
                    remaining,
                    &inflight_real_capital_governance_actions,
                )
                .map_err(|_| "engine lock poisoned".to_string())?;
        }
        if snapshot
            .intents
            .len()
            .saturating_add(snapshot.faucets.len())
            .saturating_add(snapshot.real_capital_governance.len())
            < max_items
        {
            let remaining = max_items.saturating_sub(
                snapshot.intents.len()
                    + snapshot.faucets.len()
                    + snapshot.real_capital_governance.len(),
            );
            snapshot.delegations = self
                .engine
                .pending_delegation_payload(remaining, &inflight_delegation_actions)
                .map_err(|_| "engine lock poisoned".to_string())?;
        }
        if snapshot
            .intents
            .len()
            .saturating_add(snapshot.faucets.len())
            .saturating_add(snapshot.real_capital_governance.len())
            .saturating_add(snapshot.delegations.len())
            < max_items
        {
            let remaining = max_items.saturating_sub(
                snapshot.intents.len()
                    + snapshot.faucets.len()
                    + snapshot.real_capital_governance.len()
                    + snapshot.delegations.len(),
            );
            snapshot.market_maker_programs = self
                .engine
                .pending_market_maker_program_payload(
                    remaining,
                    &inflight_market_maker_program_actions,
                )
                .map_err(|_| "engine lock poisoned".to_string())?;
        }
        if snapshot
            .intents
            .len()
            .saturating_add(snapshot.faucets.len())
            .saturating_add(snapshot.real_capital_governance.len())
            .saturating_add(snapshot.delegations.len())
            .saturating_add(snapshot.market_maker_programs.len())
            < max_items
        {
            let remaining = max_items.saturating_sub(
                snapshot.intents.len()
                    + snapshot.faucets.len()
                    + snapshot.real_capital_governance.len()
                    + snapshot.delegations.len()
                    + snapshot.market_maker_programs.len(),
            );
            snapshot.tee_governance = self
                .engine
                .pending_tee_governance_payload(remaining, &inflight_tee_governance_actions)
                .map_err(|_| "engine lock poisoned".to_string())?;
        }
        if snapshot.intents.is_empty()
            && snapshot.faucets.is_empty()
            && snapshot.real_capital_governance.is_empty()
            && snapshot.delegations.is_empty()
            && snapshot.market_maker_programs.is_empty()
            && snapshot.tee_governance.is_empty()
            && !mahi_allow_idle_empty_rounds()
        {
            let _dag = self.engine.dag_domain().track_lock();
            if let Ok(mut state_guard) = self.mahi_state.lock() {
                if let Some(state) = state_guard.as_mut() {
                    if !state.has_payload_backlog() {
                        state.producer_skip_empty_idle =
                            state.producer_skip_empty_idle.saturating_add(1);
                        return Ok(None);
                    }
                }
            }
        }

        let block = {
            let _dag = self.engine.dag_domain().track_lock();
            let final_lock_wait_start = Instant::now();
            let mut state_guard = self
                .mahi_state
                .lock()
                .map_err(|_| "mahi lock poisoned".to_string())?;
            if producer_profile {
                producer_final_lock_wait = final_lock_wait_start.elapsed();
            }
            let final_lock_hold_start = Instant::now();
            let Some(state) = state_guard.as_mut() else {
                return Ok(None);
            };
            if state.next_local_round != target_round
                || state.contains_author_round(snapshot.local_validator_id, target_round)
            {
                state.producer_skip_round_race = state.producer_skip_round_race.saturating_add(1);
                return Ok(None);
            }
            let block = MahiBlock::signed_with_all_payloads(
                snapshot.local_validator_id,
                target_round,
                epoch,
                timestamp_ms,
                ancestors,
                snapshot.oracle_prices.clone(),
                snapshot.intents.clone(),
                snapshot.faucets.clone(),
                snapshot.delegations.clone(),
                snapshot.market_maker_programs.clone(),
                snapshot.tee_governance.clone(),
                snapshot.real_capital_governance.clone(),
                snapshot.local_consensus_signing_key,
            );
            state
                .accept_block(block.clone(), true)
                .map_err(|error| format!("mahi local insert failed: {error}"))?;
            if state
                .producer_ancestor_wait
                .as_ref()
                .is_some_and(|wait| wait.target_round == target_round)
            {
                state.producer_ancestor_wait = None;
            }
            state.next_local_round = target_round.saturating_add(1);
            self.maybe_advance_mahi_commits_locked(state);
            if producer_profile {
                let producer_final_lock_hold = final_lock_hold_start.elapsed();
                state.record_producer_timing(MahiProducerTimingSample {
                    initial_lock_wait: producer_initial_lock_wait,
                    initial_lock_hold: producer_initial_lock_hold,
                    snapshot: producer_snapshot,
                    pre_admitted_drain: producer_pre_admitted_drain,
                    final_lock_wait: producer_final_lock_wait,
                    final_lock_hold: producer_final_lock_hold,
                    total: producer_total_start
                        .map(|started_at| started_at.elapsed())
                        .unwrap_or_default(),
                });
            }
            block
        };
        self.enqueue_mahi_sigverify(block.block_ref(), &block)?;
        self.ensure_mahi_missing_fetch_worker();

        let request = NetworkMahiBlockRequest {
            block_hex: encode_borsh_hex(&block),
        };
        if self.mahi_async_block_broadcast {
            self.enqueue_mahi_block_broadcast(snapshot.local_validator_id, request);
        } else {
            thread::scope(|scope| {
                for peer in network_bootstrap.validator_set.active_validators() {
                    if peer.validator_id == snapshot.local_validator_id {
                        continue;
                    }
                    let endpoints = peer.rpc_endpoints.clone();
                    let request = request.clone();
                    scope.spawn(move || {
                        let _ = self.send_network_json_request::<_, NetworkMahiBlockResponse>(
                            &endpoints,
                            "mahi/block",
                            &request,
                        );
                    });
                }
            });
        }

        let block_count = self
            .mahi_state
            .lock()
            .ok()
            .and_then(|state| state.as_ref().map(|state| state.dag.blocks.len()))
            .unwrap_or_default();
        Ok(Some((block.round, block.intents.len(), block_count)))
    }

    #[cfg(test)]
    /// Test-only convenience for E2E fixtures that need to identify the
    /// current MAHI leader before submitting a forwarded request.
    pub(crate) fn current_mahi_leader_snapshot(&self) -> Option<ValidatorId> {
        let state_guard = self.mahi_state.lock().ok()?;
        let state = state_guard.as_ref()?;
        Some(
            state
                .leader_schedule
                .leader_for_round(state.threshold_clock.round()),
        )
    }

    pub(super) fn drain_mahi_pre_admitted_for_block(
        &self,
        timestamp_ms: u64,
        max_items: usize,
        inflight_policy_nonces: &BTreeMap<u64, BTreeSet<u64>>,
        excluded_intents: &BTreeSet<IntentId>,
    ) -> Vec<SignedIntent> {
        if max_items == 0 {
            return Vec::new();
        }
        let policy_ids = self
            .mahi_pre_admitted_pool
            .lock()
            .map(|pool| pool.policy_ids())
            .unwrap_or_default();
        if policy_ids.is_empty() {
            return Vec::new();
        }

        let policy_next_nonces = self
            .engine
            .with_policy_read(|policy_store| {
                policy_ids
                    .iter()
                    .map(|policy_id| {
                        (
                            *policy_id,
                            policy_store.next_intent_nonce(*policy_id).unwrap_or(1),
                        )
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();

        self.mahi_pre_admitted_pool
            .lock()
            .map(|mut pool| {
                pool.drain_for_block(
                    timestamp_ms,
                    max_items,
                    &policy_next_nonces,
                    inflight_policy_nonces,
                    excluded_intents,
                )
            })
            .unwrap_or_default()
    }

    pub(super) fn enqueue_mahi_block_broadcast(
        &self,
        local_validator_id: ValidatorId,
        request: NetworkMahiBlockRequest,
    ) {
        if let Ok(mut queue) = self.pending_mahi_block_broadcasts.lock() {
            queue.blocks.push_back(PendingMahiBlockBroadcast {
                local_validator_id,
                request,
            });
        }
        self.ensure_mahi_block_broadcast_worker();
    }

    pub(super) fn ensure_mahi_block_broadcast_worker(&self) {
        if self
            .mahi_block_broadcast_worker_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let server = self.clone();
        thread::spawn(move || loop {
            let relayed = server.drain_pending_mahi_block_broadcasts();
            let pending = server
                .pending_mahi_block_broadcasts
                .lock()
                .map(|queue| !queue.blocks.is_empty())
                .unwrap_or(false);
            if !pending {
                server
                    .mahi_block_broadcast_worker_running
                    .store(false, Ordering::SeqCst);
                let still_pending = server
                    .pending_mahi_block_broadcasts
                    .lock()
                    .map(|queue| !queue.blocks.is_empty())
                    .unwrap_or(false);
                if still_pending
                    && server
                        .mahi_block_broadcast_worker_running
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    continue;
                }
                break;
            }
            if relayed == 0 {
                thread::sleep(Duration::from_millis(1));
            }
        });
    }

    pub(super) fn drain_pending_mahi_block_broadcasts(&self) -> usize {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return 0;
        };
        let pending = {
            let Ok(mut queue) = self.pending_mahi_block_broadcasts.lock() else {
                return 0;
            };
            let mut pending = Vec::new();
            for _ in 0..64 {
                let Some(block) = queue.blocks.pop_front() else {
                    break;
                };
                pending.push(block);
            }
            pending
        };
        if pending.is_empty() {
            return 0;
        }

        thread::scope(|scope| {
            for peer in network_bootstrap.validator_set.active_validators() {
                let requests = pending
                    .iter()
                    .filter(|broadcast| peer.validator_id != broadcast.local_validator_id)
                    .map(|broadcast| broadcast.request.clone())
                    .collect::<Vec<_>>();
                if requests.is_empty() {
                    continue;
                }
                let endpoints = peer.rpc_endpoints.clone();
                scope.spawn(move || {
                    let batch_request = NetworkMahiBlockBatchRequest {
                        blocks_hex: requests
                            .iter()
                            .map(|request| request.block_hex.clone())
                            .collect(),
                    };
                    if self
                        .send_network_json_request::<_, NetworkMahiBlockBatchResponse>(
                            &endpoints,
                            "mahi/blocks",
                            &batch_request,
                        )
                        .is_err()
                    {
                        // Rolling deploy compatibility: older peers only understand single-block
                        // broadcast. The fallback preserves ordered delivery for that peer.
                        for request in requests {
                            let _ = self.send_network_json_request::<_, NetworkMahiBlockResponse>(
                                &endpoints,
                                "mahi/block",
                                &request,
                            );
                        }
                    }
                });
            }
        });
        pending.len()
    }

    pub(super) fn mahi_state_snapshot(&self) -> Option<MahiStateSnapshot> {
        let mut snapshot = self
            .mahi_state
            .lock()
            .ok()
            .and_then(|state| state.as_ref().map(MahiState::snapshot))?;
        if let Some(pool) = &self.mahi_sigverify_pool {
            let sigverify = pool.snapshot();
            snapshot.sigverify_pool_depth = sigverify.queue_depth;
            snapshot.sigverify_batch_hist = sigverify.batch_latency_ms;
            snapshot.sigverify_per_intent_verified_total = sigverify.per_intent_verified_total;
            snapshot.sigverify_failures_total = sigverify.failures_total;
        }
        Some(snapshot)
    }

    pub(super) fn maybe_advance_mahi_commits_locked(&self, state: &mut MahiState) -> usize {
        let queued = state.advance_commit_detector();
        if queued > 0 {
            self.notify_mahi_commit_executor();
        }
        queued
    }

    pub(super) fn reset_mahi_state_after_engine_repair(&self, restart_boundary_round: u64) {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return;
        };
        let rebuilt = build_mahi_state(
            self.config.mahi_enabled,
            Some(network_bootstrap),
            parse_mahi_commit_rule(),
            restart_boundary_round,
        );
        if let Ok(mut state_guard) = self.mahi_state.lock() {
            *state_guard = rebuilt;
        }
    }

    pub(super) fn reset_mahi_state_from_durable_head(&self, reason: &str) -> Option<u64> {
        let head = self.engine.head_snapshot();
        let restart_boundary_round = mahi_restart_boundary_round_from_head(&head);
        self.reset_mahi_state_after_engine_repair(restart_boundary_round);
        println!(
            "kombat_chain reset mahi state from durable head reason={} height={} round={} hash={} restart_boundary_round={}",
            reason,
            head.head_height,
            head.head_round,
            encode_hex(&head.head_hash),
            restart_boundary_round,
        );
        Some(restart_boundary_round)
    }

    pub(super) fn spawn_mahi_sigverify_result_worker(&self) {
        let Some(pool) = self.mahi_sigverify_pool.clone() else {
            return;
        };
        let server = self.clone();
        thread::spawn(move || {
            let results = pool.results();
            while let Ok(result) = results.recv() {
                server.apply_mahi_sigverify_result(result);
            }
        });
    }

    pub(super) fn spawn_mahi_commit_executor_worker(&self) {
        if !self.config.mahi_enabled {
            return;
        }
        if self
            .mahi_commit_executor
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let server = self.clone();
        thread::spawn(move || server.run_mahi_commit_executor());
    }

    pub(super) fn notify_mahi_commit_executor(&self) {
        let Ok(mut pending) = self.mahi_commit_executor.pending.lock() else {
            return;
        };
        *pending = true;
        self.mahi_commit_executor.wake.notify_one();
    }

    pub(super) fn run_mahi_commit_executor(&self) {
        loop {
            let mut pending = match self.mahi_commit_executor.pending.lock() {
                Ok(pending) => pending,
                Err(_) => {
                    self.mahi_commit_executor
                        .running
                        .store(false, Ordering::SeqCst);
                    return;
                }
            };
            while !*pending {
                pending = match self.mahi_commit_executor.wake.wait(pending) {
                    Ok(pending) => pending,
                    Err(_) => {
                        self.mahi_commit_executor
                            .running
                            .store(false, Ordering::SeqCst);
                        return;
                    }
                };
            }
            *pending = false;
            drop(pending);

            let (blocks, merged_execution_batch_size) = {
                let _dag = self.engine.dag_domain().track_lock();
                let Ok(mut state_guard) = self.mahi_state.lock() else {
                    self.mahi_commit_executor
                        .running
                        .store(false, Ordering::SeqCst);
                    return;
                };
                let Some(state) = state_guard.as_mut() else {
                    continue;
                };
                state.mark_commit_executor_wake();
                state.advance_commit_detector();
                let merged_execution_batch_size = state.default_execution_batch_size();
                (
                    state.drain_ready_execution_for_batch(merged_execution_batch_size),
                    merged_execution_batch_size,
                )
            };
            if blocks.is_empty() {
                let _dag = self.engine.dag_domain().track_lock();
                if let Ok(mut state_guard) = self.mahi_state.lock() {
                    if let Some(state) = state_guard.as_mut() {
                        state.mark_commit_executor_empty_wake();
                    }
                }
                continue;
            }

            let mut failed = false;
            let execution_chunk_size = if merged_execution_batch_size > 1 {
                merged_execution_batch_size
            } else {
                MAHI_EXECUTION_BATCH_SIZE
            };
            for chunk_start in (0..blocks.len()).step_by(execution_chunk_size) {
                let chunk_end = (chunk_start + execution_chunk_size).min(blocks.len());
                let chunk = &blocks[chunk_start..chunk_end];
                if merged_execution_batch_size > 1 {
                    let execution_started = Instant::now();
                    match self.engine.apply_mahi_committed_blocks_merged(chunk) {
                        Ok(Ok(_)) => {
                            let execution_elapsed = execution_started.elapsed();
                            let _dag = self.engine.dag_domain().track_lock();
                            if let Ok(mut state_guard) = self.mahi_state.lock() {
                                if let Some(state) = state_guard.as_mut() {
                                    state.record_commit_execution_batch(chunk, execution_elapsed);
                                    for block in chunk {
                                        state.mark_block_executed(block.block_ref());
                                    }
                                }
                            }
                        }
                        Ok(Err(error)) => {
                            eprintln!(
                                "kombat_chain merged mahi commit execution failed for {} blocks: {:?}",
                                chunk.len(),
                                error
                            );
                            let remaining = blocks[chunk_start..].to_vec();
                            let _dag = self.engine.dag_domain().track_lock();
                            if let Ok(mut state_guard) = self.mahi_state.lock() {
                                if let Some(state) = state_guard.as_mut() {
                                    state.requeue_execution_front(remaining);
                                }
                            }
                            self.notify_mahi_commit_executor();
                            thread::sleep(Duration::from_millis(10));
                            failed = true;
                        }
                        Err(_) => {
                            self.mahi_commit_executor
                                .running
                                .store(false, Ordering::SeqCst);
                            return;
                        }
                    }
                } else {
                    let execution_started = Instant::now();
                    let results = match self.engine.apply_mahi_committed_blocks(chunk) {
                        Ok(results) => results,
                        Err(_) => {
                            self.mahi_commit_executor
                                .running
                                .store(false, Ordering::SeqCst);
                            return;
                        }
                    };
                    let execution_elapsed = execution_started.elapsed();
                    if results.iter().all(Result::is_ok) {
                        let _dag = self.engine.dag_domain().track_lock();
                        if let Ok(mut state_guard) = self.mahi_state.lock() {
                            if let Some(state) = state_guard.as_mut() {
                                state.record_commit_execution_batch(chunk, execution_elapsed);
                            }
                        }
                    }
                    for (index, (block, result)) in
                        chunk.iter().zip(results.into_iter()).enumerate()
                    {
                        let block_ref = block.block_ref();
                        match result {
                            Ok(_) => {
                                let _dag = self.engine.dag_domain().track_lock();
                                if let Ok(mut state_guard) = self.mahi_state.lock() {
                                    if let Some(state) = state_guard.as_mut() {
                                        state.mark_block_executed(block_ref);
                                    }
                                }
                            }
                            Err(error) => {
                                eprintln!(
                                    "kombat_chain mahi commit execution failed for round {} author {}: {:?}",
                                    block.round,
                                    encode_hex(&block.author),
                                    error
                                );
                                let remaining = blocks[(chunk_start + index)..].to_vec();
                                let _dag = self.engine.dag_domain().track_lock();
                                if let Ok(mut state_guard) = self.mahi_state.lock() {
                                    if let Some(state) = state_guard.as_mut() {
                                        state.requeue_execution_front(remaining);
                                    }
                                }
                                self.notify_mahi_commit_executor();
                                thread::sleep(Duration::from_millis(10));
                                failed = true;
                                break;
                            }
                        }
                    }
                }
                if failed {
                    break;
                }
            }
        }
    }

    pub(super) fn apply_mahi_sigverify_result(&self, result: MahiSigverifyResult) {
        let _dag = self.engine.dag_domain().track_lock();
        let Ok(mut state_guard) = self.mahi_state.lock() else {
            return;
        };
        let Some(state) = state_guard.as_mut() else {
            return;
        };
        match result {
            MahiSigverifyResult::Verified { block_ref, .. } => {
                state.mark_block_verified(block_ref);
                self.notify_mahi_commit_executor();
            }
            MahiSigverifyResult::Failed { block_ref, .. } => state.mark_block_failed(block_ref),
        }
    }

    pub(crate) fn enqueue_mahi_sigverify(
        &self,
        block_ref: MahiBlockRef,
        block: &MahiBlock,
    ) -> Result<(), String> {
        if block.intents.is_empty() || self.mahi_sigverify_pool.is_none() {
            let _dag = self.engine.dag_domain().track_lock();
            let mut state_guard = self
                .mahi_state
                .lock()
                .map_err(|_| "mahi lock poisoned".to_string())?;
            let state = state_guard
                .as_mut()
                .ok_or_else(|| "mahi disabled".to_string())?;
            state.mark_block_verified(block_ref);
            self.notify_mahi_commit_executor();
            return Ok(());
        }

        let head = self.engine.head_snapshot();
        let batch = MahiSigverifyBatch {
            block_ref,
            intents: block.intents.clone(),
            expected_chain_id: head.chain_id,
            block_number: head.head_height.saturating_add(1),
        };
        self.mahi_sigverify_pool
            .as_ref()
            .expect("mahi sigverify pool should exist")
            .enqueue(batch)
    }

    pub(crate) fn ensure_mahi_missing_fetch_worker(&self) {
        if !self.config.mahi_enabled || self.config.network_bootstrap.is_none() {
            return;
        }
        if self
            .mahi_missing_fetch_worker_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let server = self.clone();
        thread::spawn(move || server.run_mahi_missing_fetch_worker());
    }

    pub(super) fn run_mahi_missing_fetch_worker(&self) {
        loop {
            let missing = {
                let _dag = self.engine.dag_domain().track_lock();
                let Ok(state_guard) = self.mahi_state.lock() else {
                    self.mahi_missing_fetch_worker_running
                        .store(false, Ordering::SeqCst);
                    return;
                };
                let Some(state) = state_guard.as_ref() else {
                    self.mahi_missing_fetch_worker_running
                        .store(false, Ordering::SeqCst);
                    return;
                };
                state.dag.pending_fetch.iter().copied().collect::<Vec<_>>()
            };
            if missing.is_empty() {
                self.mahi_missing_fetch_worker_running
                    .store(false, Ordering::SeqCst);
                let still_missing = self
                    .mahi_state
                    .lock()
                    .ok()
                    .and_then(|state| {
                        state
                            .as_ref()
                            .map(|state| !state.dag.pending_fetch.is_empty())
                    })
                    .unwrap_or(false);
                if still_missing
                    && self
                        .mahi_missing_fetch_worker_running
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    continue;
                }
                break;
            }
            let mut fetched_any = false;
            for reference in missing {
                if self.fetch_mahi_missing_block(reference) {
                    fetched_any = true;
                }
            }
            if !fetched_any {
                thread::sleep(Duration::from_millis(25));
            }
        }
    }

    pub(super) fn fetch_mahi_missing_block(&self, reference: MahiBlockRef) -> bool {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return false;
        };
        let validators = network_bootstrap.validator_set.active_validators();
        let Some(peer) = validators
            .iter()
            .find(|validator| validator.validator_id == reference.author)
        else {
            return false;
        };
        let request = NetworkMahiFetchRequest {
            refs_hex: vec![encode_borsh_hex(&reference)],
        };
        let Ok(response) = self.send_network_json_request::<_, NetworkMahiFetchResponse>(
            &peer.rpc_endpoints,
            "mahi/fetch",
            &request,
        ) else {
            return false;
        };
        let mut fetched_any = false;
        for block_hex in response.blocks_hex {
            let Ok(block) = decode_borsh_hex::<MahiBlock>(&block_hex) else {
                continue;
            };
            let Ok(block_ref) = self.accept_mahi_block_into_state(block.clone(), false) else {
                continue;
            };
            let _ = self.enqueue_mahi_sigverify(block_ref, &block);
            fetched_any = true;
        }
        fetched_any
    }

    pub(crate) fn accept_mahi_block_into_state(
        &self,
        block: MahiBlock,
        produced_locally: bool,
    ) -> Result<MahiBlockRef, JsonRpcErrorObject> {
        let profile_accept = mahi_accept_profile_enabled();
        let profile_started = Instant::now();
        let block_round = block.round;
        let block_intents = block.intents.len();
        let _dag = self.engine.dag_domain().track_lock();
        let mut state_guard = self.mahi_state.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "mahi lock poisoned".into(),
            data: None,
        })?;
        let lock_elapsed = profile_started.elapsed();
        let Some(state) = state_guard.as_mut() else {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "mahi disabled".into(),
                data: None,
            });
        };
        let accept_started = Instant::now();
        let reference = state
            .accept_block(block, produced_locally)
            .map_err(|message| JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            })?;
        let accept_elapsed = accept_started.elapsed();
        let detector_started = Instant::now();
        let queued = self.maybe_advance_mahi_commits_locked(state);
        let detector_elapsed = detector_started.elapsed();
        if profile_accept {
            eprintln!(
                "mahi_accept_profile produced={} round={} intents={} lock_ms={} accept_ms={} detector_ms={} queued={} total_ms={}",
                produced_locally,
                block_round,
                block_intents,
                lock_elapsed.as_millis(),
                accept_elapsed.as_millis(),
                detector_elapsed.as_millis(),
                queued,
                profile_started.elapsed().as_millis(),
            );
        }
        Ok(reference)
    }

    pub(super) fn should_drop_inbound_mahi_block(&self, block: &MahiBlock) -> bool {
        let drop_pct = self.mahi_inbound_drop_pct.min(100);
        if drop_pct == 0 {
            return false;
        }
        let hash = block.hash();
        let mut sample = [0_u8; 8];
        sample.copy_from_slice(&hash[..8]);
        (u64::from_le_bytes(sample) % 100) < u64::from(drop_pct)
    }
}
