use crate::*;
use std::thread;

impl JsonRpcHttpServer {
    pub(super) fn ensure_local_submit_worker(&self) {
        if self
            .local_submit_worker_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let server = self.clone();
        thread::spawn(move || loop {
            let processed = server.drain_local_submit_queue();
            let pending = server
                .local_submit_queue
                .lock()
                .map(|queue| !queue.items.is_empty())
                .unwrap_or(false);
            if !pending {
                server
                    .local_submit_worker_running
                    .store(false, Ordering::SeqCst);
                let still_pending = server
                    .local_submit_queue
                    .lock()
                    .map(|queue| !queue.items.is_empty())
                    .unwrap_or(false);
                if still_pending
                    && server
                        .local_submit_worker_running
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    continue;
                }
                break;
            }
            if processed == 0 {
                thread::sleep(LOCAL_SUBMIT_WORKER_IDLE_SLEEP);
            }
        });
    }

    pub(super) fn enqueue_local_submit(
        &self,
        params: SubmitIntentParams,
        signed_intent: SignedIntent,
        preview_response: SubmitIntentResponse,
        relay_after_admit: bool,
        result_tx: Option<SyncSender<Result<SubmitIntentResponse, RpcError>>>,
    ) -> Result<(), JsonRpcErrorObject> {
        let mut queue = self
            .local_submit_queue
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "local submit queue lock poisoned".into(),
                data: None,
            })?;
        if queue.items.len() >= MAX_LOCAL_SUBMIT_QUEUE_ITEMS {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "local submit queue full".into(),
                data: Some(json!({
                    "kind": "intent_submission",
                    "reason": "local_submit_queue_full",
                    "queue_limit": MAX_LOCAL_SUBMIT_QUEUE_ITEMS,
                })),
            });
        }
        queue.items.push_back(QueuedLocalSubmit {
            params,
            signed_intent,
            preview_response,
            relay_after_admit,
            result_tx,
        });
        drop(queue);
        self.ensure_local_submit_worker();
        Ok(())
    }

    pub(super) fn preview_local_submit(
        &self,
        signed_intent: &SignedIntent,
        timestamp_ms: u64,
    ) -> Result<SubmitIntentResponse, RpcError> {
        let preview = self
            .engine
            .preview_submit_intent(signed_intent, timestamp_ms)
            .map_err(|_| RpcError::Node("engine lock poisoned".into()))?
            .map_err(|error| intent_submission_rpc_error(signed_intent, timestamp_ms, &error))?;
        Ok(SubmitIntentResponse {
            intent_id: preview.intent_id,
            class: preview.class,
            pending_reason: preview.pending_reason,
            next_eligible_at_ms: preview.next_eligible_at_ms,
            current_policy_nonce: preview.current_policy_nonce,
        })
    }

    pub(super) fn drain_local_submit_queue(&self) -> usize {
        let queued = {
            let Ok(mut queue) = self.local_submit_queue.lock() else {
                return 0;
            };
            let mut batch = Vec::new();
            for _ in 0..LOCAL_SUBMIT_DRAIN_BATCH_ITEMS {
                let Some(item) = queue.items.pop_front() else {
                    break;
                };
                batch.push(item);
            }
            batch
        };
        if queued.is_empty() {
            return 0;
        }

        let submissions = queued
            .iter()
            .map(|queued| (queued.signed_intent.clone(), queued.params.timestamp_ms))
            .collect::<Vec<_>>();
        let submit_results = match self.engine.submit_intents(submissions) {
            Ok(results) => results,
            Err(_) => {
                let processed = queued.len();
                for queued in queued {
                    if let Some(result_tx) = queued.result_tx {
                        let _ = result_tx.send(Err(RpcError::Node("engine lock poisoned".into())));
                    }
                }
                return processed;
            }
        };

        let local_validator_id = self
            .config
            .network_bootstrap
            .as_ref()
            .and_then(NetworkBootstrapConfig::local_validator_id);
        let processed = queued.len();
        for (queued, submit_result) in queued.into_iter().zip(submit_results.into_iter()) {
            let result = match submit_result {
                Ok(_) => Ok(queued.preview_response.clone()),
                Err(error) => Err(intent_submission_rpc_error(
                    &queued.signed_intent,
                    queued.params.timestamp_ms,
                    &error,
                )),
            };

            if let Ok(response) = &result {
                if queued.relay_after_admit {
                    if let Some(local_validator_id) = local_validator_id {
                        if let Err(error) = self.enqueue_post_admit_submit_relay(
                            local_validator_id,
                            queued.params.clone(),
                        ) {
                            eprintln!(
                                "kombat_chain submit-intent relay enqueue failed: {}",
                                error.message
                            );
                        }
                    }
                }
                if let Some(result_tx) = queued.result_tx {
                    let _ = result_tx.send(Ok(response.clone()));
                }
            } else if let Err(error) = result {
                if let Some(result_tx) = queued.result_tx {
                    let _ = result_tx.send(Err(error));
                }
            }
        }

        processed
    }

    pub(super) fn wait_for_local_submit_idle(&self) -> bool {
        let deadline = Instant::now() + LOCAL_SUBMIT_QUIESCE_TIMEOUT;
        loop {
            let pending = self
                .local_submit_queue
                .lock()
                .map(|queue| !queue.items.is_empty())
                .unwrap_or(false);
            let worker_running = self.local_submit_worker_running.load(Ordering::SeqCst);
            if !pending && !worker_running {
                return true;
            }
            if pending && !worker_running {
                if self.drain_local_submit_queue() == 0 {
                    self.ensure_local_submit_worker();
                    thread::sleep(LOCAL_SUBMIT_WORKER_IDLE_SLEEP);
                }
                continue;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(LOCAL_SUBMIT_WORKER_IDLE_SLEEP);
        }
    }

    pub(super) fn queue_submit_intent_public(
        &self,
        params: SubmitIntentParams,
        relay_after_admit: bool,
    ) -> Result<Value, JsonRpcErrorObject> {
        let signed_intent = signed_intent_from_json(&params.signed_intent).map_err(|message| {
            JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            }
        })?;
        let response = self
            .preview_local_submit(&signed_intent, params.timestamp_ms)
            .map_err(|error| match error {
                RpcError::Submission(json_error) => json_error,
                other => JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{other:?}"),
                    data: None,
                },
            })?;
        let intent_id = response.intent_id;
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        self.enqueue_local_submit(
            params,
            signed_intent,
            response,
            relay_after_admit,
            Some(result_tx),
        )?;
        match result_rx.recv_timeout(LOCAL_SUBMIT_QUIESCE_TIMEOUT) {
            Ok(Ok(response)) => Ok(json!(submit_intent_response_to_json(&response))),
            Ok(Err(error)) => Err(map_rpc_error_to_json_rpc_error(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(JsonRpcErrorObject {
                code: -32000,
                message: "local submit admission timed out".into(),
                data: Some(json!({
                    "kind": "intent_submission",
                    "reason": "local_submit_admission_timeout",
                    "intent_id_hex": encode_hex(&intent_id),
                })),
            }),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(JsonRpcErrorObject {
                code: -32000,
                message: "local submit admission worker disconnected".into(),
                data: Some(json!({
                    "kind": "intent_submission",
                    "reason": "local_submit_worker_disconnected",
                    "intent_id_hex": encode_hex(&intent_id),
                })),
            }),
        }
    }

    pub(super) fn has_pending_intent_relays(&self) -> bool {
        let legacy_pending = self
            .pending_intent_relays
            .lock()
            .map(|queue| !queue.is_empty())
            .unwrap_or(false);
        if legacy_pending {
            return true;
        }
        self.pending_mahi_owner_relays
            .lock()
            .map(|queue| !queue.is_empty())
            .unwrap_or(false)
    }

    pub(super) fn ensure_pending_intent_relay_worker(&self) {
        if self
            .intent_relay_worker_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let server = self.clone();
        thread::spawn(move || loop {
            let relayed = server.drain_pending_intent_relays();
            let pending = server.has_pending_intent_relays();
            if !pending {
                server
                    .intent_relay_worker_running
                    .store(false, Ordering::SeqCst);
                let still_pending = server.has_pending_intent_relays();
                if still_pending
                    && server
                        .intent_relay_worker_running
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    continue;
                }
                break;
            }
            if relayed == 0 {
                thread::sleep(Duration::from_millis(10));
            }
        });
    }

    pub(super) fn enqueue_post_admit_submit_relay(
        &self,
        local_validator_id: ValidatorId,
        params: SubmitIntentParams,
    ) -> Result<(), JsonRpcErrorObject> {
        let queued = self.queue_post_admit_submit_relay(local_validator_id, params)?;
        if queued {
            self.ensure_pending_intent_relay_worker();
        }
        Ok(())
    }

    pub(crate) fn queue_post_admit_submit_relay(
        &self,
        local_validator_id: ValidatorId,
        params: SubmitIntentParams,
    ) -> Result<bool, JsonRpcErrorObject> {
        if self.config.mahi_enabled {
            let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
                return Ok(false);
            };
            let signed_intent =
                signed_intent_from_json(&params.signed_intent).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
            let active_validators = network_bootstrap.validator_set.active_validators();
            let Some(owner_id) = mahi_payload_primary_owner_id_for_policy(
                &active_validators,
                signed_intent.intent.policy_id,
            ) else {
                return Ok(false);
            };
            if owner_id == local_validator_id {
                return Ok(false);
            }
            self.pending_mahi_owner_relays
                .lock()
                .map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "pending mahi owner relay lock poisoned".into(),
                    data: None,
                })?
                .insert(owner_id, intent_id(&signed_intent), params);
            return Ok(true);
        }

        self.enqueue_submit_intent_relay(local_validator_id, params)?;
        Ok(true)
    }

    pub(super) fn enqueue_submit_intent_relay(
        &self,
        local_validator_id: ValidatorId,
        params: SubmitIntentParams,
    ) -> Result<(), JsonRpcErrorObject> {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(());
        };
        let pending_validators = network_bootstrap
            .validator_set
            .active_validators()
            .iter()
            .filter_map(|validator| {
                (validator.validator_id != local_validator_id).then_some(validator.validator_id)
            })
            .collect::<BTreeSet<_>>();
        if pending_validators.is_empty() {
            return Ok(());
        }
        let signed_intent = signed_intent_from_json(&params.signed_intent).map_err(|message| {
            JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            }
        })?;
        self.pending_intent_relays
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "pending intent relay lock poisoned".into(),
                data: None,
            })?
            .insert(
                intent_id(&signed_intent),
                PendingIntentRelay {
                    params,
                    policy_id: signed_intent.intent.policy_id,
                    nonce: signed_intent.intent.nonce,
                    pending_validators,
                },
            );
        self.ensure_pending_intent_relay_worker();
        Ok(())
    }

    pub(crate) fn enqueue_mahi_observer_owner_relay_batch(
        &self,
        active_validators: &[ValidatorConfig],
        params: SubmitIntentBatchParams,
    ) -> Result<SubmitIntentBatchResponse, JsonRpcErrorObject> {
        if active_validators.is_empty() {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "validator set has no active validators".into(),
                data: None,
            });
        }

        let mut relays = Vec::with_capacity(params.items.len());
        let mut results = Vec::with_capacity(params.items.len());
        for submit_params in params.items {
            let signed_intent =
                signed_intent_from_json(&submit_params.signed_intent).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
            let intent_id = intent_id(&signed_intent);
            let owner_id = mahi_payload_primary_owner_id_for_policy(
                active_validators,
                signed_intent.intent.policy_id,
            );
            let response = SubmitIntentResponse {
                intent_id,
                class: if signed_intent.intent.predicate.is_some() {
                    IntentAdmissionClass::Conditional
                } else {
                    IntentAdmissionClass::Direct
                },
                pending_reason: None,
                next_eligible_at_ms: None,
                current_policy_nonce: None,
            };
            if let Some(owner_id) = owner_id {
                relays.push((owner_id, intent_id, submit_params.clone()));
            }
            results.push(SubmitIntentBatchItemResponse {
                accepted: true,
                intent_id_hex: Some(encode_hex(&intent_id)),
                response: Some(submit_intent_response_to_json(&response)),
                error: None,
            });
        }

        let mut queue = self
            .pending_mahi_owner_relays
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "pending mahi owner relay lock poisoned".into(),
                data: None,
            })?;
        for (owner_id, intent_id, params) in relays {
            queue.insert(owner_id, intent_id, params);
        }
        drop(queue);
        self.ensure_pending_intent_relay_worker();

        Ok(SubmitIntentBatchResponse { results })
    }

    pub(crate) fn local_mahi_payload_owner_for_policy(&self, policy_id: u64) -> bool {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return false;
        };
        let Some(local_validator_id) = network_bootstrap.local_validator_id() else {
            return false;
        };
        let active_validators = network_bootstrap.validator_set.active_validators();
        if active_validators.is_empty() {
            return false;
        }
        mahi_payload_owner_ids_for_policy(&active_validators, policy_id)
            .contains(&local_validator_id)
    }

    pub(crate) fn local_mahi_owner_fallback_enabled(&self) -> bool {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return false;
        };
        let active_validators = network_bootstrap.validator_set.active_validators();
        parse_mahi_owner_relay_redundancy(active_validators.len()) > 1
    }

    pub(crate) fn local_mahi_payload_owner_lagging(&self) -> bool {
        let lag_rounds = parse_mahi_owner_relay_lag_rounds();
        if lag_rounds == 0 {
            return false;
        }
        let Ok(state_guard) = self.mahi_state.lock() else {
            return false;
        };
        let Some(state) = state_guard.as_ref() else {
            return false;
        };
        let max_round = state.dag.by_round.keys().next_back().copied().unwrap_or(0);
        let last_produced_round = state
            .last_produced
            .map(|reference| reference.round)
            .unwrap_or(0);
        max_round.saturating_sub(last_produced_round) > lag_rounds
    }

    pub(super) fn enqueue_mahi_pre_admitted_relays(
        &self,
        decoded: Vec<(IntentId, SubmitIntentParams, SignedIntent)>,
    ) -> Result<Vec<NetworkRelayIntentResult>, JsonRpcErrorObject> {
        let mut results = Vec::with_capacity(decoded.len());
        let retry_for_local_lag =
            self.local_mahi_owner_fallback_enabled() && self.local_mahi_payload_owner_lagging();
        let mut pool = self
            .mahi_pre_admitted_pool
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "mahi pre-admitted pool lock poisoned".into(),
                data: None,
            })?;

        for (intent_id, _params, signed_intent) in decoded {
            if !self.local_mahi_payload_owner_for_policy(signed_intent.intent.policy_id) {
                results.push(NetworkRelayIntentResult {
                    intent_id_hex: encode_hex(&intent_id),
                    status: "retry".into(),
                });
                continue;
            }
            if retry_for_local_lag {
                results.push(NetworkRelayIntentResult {
                    intent_id_hex: encode_hex(&intent_id),
                    status: "retry_lagging_owner".into(),
                });
                continue;
            }

            let status = match pool.insert(signed_intent) {
                MahiPreAdmittedInsertResult::Accepted => "accepted",
                MahiPreAdmittedInsertResult::Duplicate => "benign",
                MahiPreAdmittedInsertResult::Full => "retry",
            };
            results.push(NetworkRelayIntentResult {
                intent_id_hex: encode_hex(&intent_id),
                status: status.into(),
            });
        }

        Ok(results)
    }

    pub(super) fn submit_intent_batch_locally(
        &self,
        params: SubmitIntentBatchParams,
    ) -> Result<(Vec<SubmitIntentBatchItemResponse>, Vec<SubmitIntentParams>), JsonRpcErrorObject>
    {
        let items = params.items;
        let mut results = vec![None::<SubmitIntentBatchItemResponse>; items.len()];
        let mut pending_results = Vec::new();
        let mut accepted_relays = Vec::new();
        let relay_after_admit = false;
        for (position, submit_params) in items.into_iter().enumerate() {
            let signed_intent =
                signed_intent_from_json(&submit_params.signed_intent).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
            let response =
                match self.preview_local_submit(&signed_intent, submit_params.timestamp_ms) {
                    Ok(response) => response,
                    Err(error) => {
                        results[position] = Some(SubmitIntentBatchItemResponse {
                            accepted: false,
                            intent_id_hex: Some(encode_hex(&intent_id(&signed_intent))),
                            response: None,
                            error: Some(match error {
                                RpcError::Submission(json_error) => json_error,
                                other => JsonRpcErrorObject {
                                    code: -32000,
                                    message: format!("{other:?}"),
                                    data: None,
                                },
                            }),
                        });
                        continue;
                    }
                };
            let intent_id = response.intent_id;
            let (result_tx, result_rx) = mpsc::sync_channel(1);
            match self.enqueue_local_submit(
                submit_params.clone(),
                signed_intent,
                response.clone(),
                relay_after_admit,
                Some(result_tx),
            ) {
                Ok(()) => {
                    accepted_relays.push(submit_params);
                    pending_results.push((position, intent_id, result_rx));
                }
                Err(error) => {
                    results[position] = Some(SubmitIntentBatchItemResponse {
                        accepted: false,
                        intent_id_hex: Some(encode_hex(&intent_id)),
                        response: None,
                        error: Some(error),
                    });
                }
            }
        }
        for (position, intent_id, result_rx) in pending_results {
            results[position] = Some(match result_rx.recv_timeout(LOCAL_SUBMIT_QUIESCE_TIMEOUT) {
                Ok(Ok(response)) => SubmitIntentBatchItemResponse {
                    accepted: true,
                    intent_id_hex: Some(encode_hex(&response.intent_id)),
                    response: Some(submit_intent_response_to_json(&response)),
                    error: None,
                },
                Ok(Err(error)) => SubmitIntentBatchItemResponse {
                    accepted: false,
                    intent_id_hex: Some(encode_hex(&intent_id)),
                    response: None,
                    error: Some(map_rpc_error_to_json_rpc_error(error)),
                },
                Err(mpsc::RecvTimeoutError::Timeout) => SubmitIntentBatchItemResponse {
                    accepted: false,
                    intent_id_hex: Some(encode_hex(&intent_id)),
                    response: None,
                    error: Some(JsonRpcErrorObject {
                        code: -32000,
                        message: "local submit admission timed out".into(),
                        data: Some(json!({
                            "kind": "intent_submission",
                            "reason": "local_submit_admission_timeout",
                            "intent_id_hex": encode_hex(&intent_id),
                        })),
                    }),
                },
                Err(mpsc::RecvTimeoutError::Disconnected) => SubmitIntentBatchItemResponse {
                    accepted: false,
                    intent_id_hex: Some(encode_hex(&intent_id)),
                    response: None,
                    error: Some(JsonRpcErrorObject {
                        code: -32000,
                        message: "local submit admission worker disconnected".into(),
                        data: Some(json!({
                            "kind": "intent_submission",
                            "reason": "local_submit_worker_disconnected",
                            "intent_id_hex": encode_hex(&intent_id),
                        })),
                    }),
                },
            });
        }
        let results = results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| SubmitIntentBatchItemResponse {
                    accepted: false,
                    intent_id_hex: None,
                    response: None,
                    error: Some(JsonRpcErrorObject {
                        code: -32000,
                        message: "local submit admission result missing".into(),
                        data: Some(json!({
                            "kind": "intent_submission",
                            "reason": "local_submit_result_missing",
                        })),
                    }),
                })
            })
            .collect();
        Ok((results, accepted_relays))
    }

    pub(super) fn drain_pending_mahi_owner_relays(
        &self,
        active_validators: &[ValidatorConfig],
        validator_endpoints: &BTreeMap<ValidatorId, Vec<String>>,
    ) -> usize {
        let batches = {
            let Ok(mut queue) = self.pending_mahi_owner_relays.lock() else {
                return 0;
            };
            if queue.is_empty() {
                return 0;
            }
            queue.take_batches(MAX_INTENT_RELAY_BATCH_ITEMS)
        };
        if batches.is_empty() {
            return 0;
        }

        let relay_threads = batches
            .into_iter()
            .filter_map(|(validator_id, batch_items)| {
                let endpoints = validator_endpoints.get(&validator_id)?.clone();
                let server = self.clone();
                Some(thread::spawn(move || {
                    let request = NetworkRelayIntentsRequest {
                        items: batch_items
                            .iter()
                            .map(|(_, params)| params.clone())
                            .collect::<Vec<_>>(),
                    };
                    let completed = server
                        .send_network_json_request::<_, NetworkRelayIntentsResponse>(
                            &endpoints,
                            "relay-intents",
                            &request,
                        )
                        .ok()
                        .map(|response| {
                            response
                                .results
                                .into_iter()
                                .filter(|result| {
                                    matches!(result.status.as_str(), "accepted" | "benign")
                                })
                                .filter_map(|result| decode_hex_array(&result.intent_id_hex).ok())
                                .collect::<BTreeSet<_>>()
                        })
                        .unwrap_or_default();
                    (validator_id, batch_items, completed)
                }))
            })
            .collect::<Vec<_>>();

        let mut relayed = 0_usize;
        for relay_thread in relay_threads {
            let Ok((validator_id, batch_items, completed)) = relay_thread.join() else {
                continue;
            };
            relayed = relayed.saturating_add(completed.len());
            let retry_items = batch_items
                .into_iter()
                .filter(|(intent_id, _)| !completed.contains(intent_id))
                .collect::<Vec<_>>();
            if retry_items.is_empty() {
                continue;
            }
            if let Ok(mut queue) = self.pending_mahi_owner_relays.lock() {
                let mut same_owner = Vec::new();
                for (intent_id, params) in retry_items {
                    let next_owner = signed_intent_from_json(&params.signed_intent)
                        .ok()
                        .and_then(|signed_intent| {
                            mahi_payload_next_owner_id_for_policy(
                                active_validators,
                                signed_intent.intent.policy_id,
                                validator_id,
                            )
                        });
                    if let Some(next_owner) = next_owner {
                        queue.insert(next_owner, intent_id, params);
                    } else {
                        same_owner.push((intent_id, params));
                    }
                }
                queue.requeue_front(validator_id, same_owner);
            }
        }

        relayed
    }

    pub(super) fn drain_pending_legacy_intent_relays(
        &self,
        validator_endpoints: &BTreeMap<ValidatorId, Vec<String>>,
    ) -> usize {
        let batches = {
            let Ok(queue) = self.pending_intent_relays.lock() else {
                return 0;
            };
            if queue.is_empty() {
                return 0;
            }

            let mut batches = BTreeMap::<ValidatorId, Vec<(IntentId, SubmitIntentParams)>>::new();
            let mut full_targets = BTreeSet::<ValidatorId>::new();
            for key in &queue.ordered {
                let Some(relay) = queue.intents.get(&key.intent_id) else {
                    continue;
                };
                for validator_id in &relay.pending_validators {
                    if full_targets.contains(validator_id) {
                        continue;
                    }
                    let batch = batches.entry(*validator_id).or_default();
                    if batch.len() < MAX_INTENT_RELAY_BATCH_ITEMS {
                        batch.push((key.intent_id, relay.params.clone()));
                        if batch.len() == MAX_INTENT_RELAY_BATCH_ITEMS {
                            full_targets.insert(*validator_id);
                        }
                    }
                    if full_targets.len() == validator_endpoints.len() {
                        break;
                    }
                }
                if full_targets.len() == validator_endpoints.len() {
                    break;
                }
            }
            batches
        };

        let relay_threads = batches
            .into_iter()
            .filter_map(|(validator_id, batch_items)| {
                let endpoints = validator_endpoints.get(&validator_id)?.clone();
                if batch_items.is_empty() {
                    return None;
                }
                let server = self.clone();
                Some(thread::spawn(move || {
                    let request = NetworkRelayIntentsRequest {
                        items: batch_items
                            .iter()
                            .map(|(_, params)| params.clone())
                            .collect::<Vec<_>>(),
                    };
                    let response = server
                        .send_network_json_request::<_, NetworkRelayIntentsResponse>(
                            &endpoints,
                            "relay-intents",
                            &request,
                        )
                        .ok()?;
                    let completed = response
                        .results
                        .into_iter()
                        .filter(|result| matches!(result.status.as_str(), "accepted" | "benign"))
                        .filter_map(|result| decode_hex_array(&result.intent_id_hex).ok())
                        .collect::<Vec<_>>();
                    (!completed.is_empty()).then_some((validator_id, completed))
                }))
            })
            .collect::<Vec<_>>();

        let mut relayed = 0_usize;
        for relay_thread in relay_threads {
            let Ok(Some((validator_id, completed))) = relay_thread.join() else {
                continue;
            };
            let Ok(mut queue) = self.pending_intent_relays.lock() else {
                continue;
            };
            for intent_id in completed {
                if let Some(relay) = queue.intents.get_mut(&intent_id) {
                    relay.pending_validators.remove(&validator_id);
                    if relay.pending_validators.is_empty() {
                        queue.remove(&intent_id);
                    }
                    relayed = relayed.saturating_add(1);
                }
            }
        }

        relayed
    }

    pub(super) fn drain_pending_intent_relays(&self) -> usize {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return 0;
        };
        let validator_endpoints = network_bootstrap
            .validator_set
            .active_validators()
            .iter()
            .map(|validator| (validator.validator_id, validator.rpc_endpoints.clone()))
            .collect::<BTreeMap<_, _>>();
        let active_validators = network_bootstrap.validator_set.active_validators();

        self.drain_pending_mahi_owner_relays(&active_validators, &validator_endpoints)
            .saturating_add(self.drain_pending_legacy_intent_relays(&validator_endpoints))
    }
}
