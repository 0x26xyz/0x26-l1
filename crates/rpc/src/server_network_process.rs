use crate::*;
use std::time::{Duration, Instant};

impl JsonRpcHttpServer {
    pub(crate) fn process_network_head_qc_signature(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        if self.config.network_bootstrap.is_none() {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "network bootstrap not configured".into(),
                data: None,
            });
        }
        let request: NetworkHeadSignatureRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid head qc signature body: {error}"),
                data: None,
            })?;
        let head_hash = decode_hex_array::<32>(&request.head_hash_hex).map_err(|message| {
            JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            }
        })?;
        let (chain_id, initial_signature_result) = {
            let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            (
                engine.chain_id(),
                engine.sign_current_head_qc(head_hash, request.round),
            )
        };
        let signature = match initial_signature_result {
            Ok(signature) => signature,
            Err(initial_error) => {
                if should_force_snapshot_catch_up(&initial_error) {
                    if let Ok(network_bootstrap) =
                        self.config
                            .network_bootstrap
                            .as_ref()
                            .ok_or(JsonRpcErrorObject {
                                code: -32000,
                                message: "network bootstrap not configured".into(),
                                data: None,
                            })
                    {
                        if let Ok(leader) = select_leader_for_validator_set(
                            request.round.saturating_add(1),
                            chain_id,
                            &network_bootstrap.validator_set,
                        ) {
                            if let Err(error) = self.maybe_force_repair_from_validator_snapshot(
                                leader,
                                None,
                                None,
                                Some(head_hash),
                                "head_qc_signature_mismatch",
                            ) {
                                eprintln!(
                                    "kombat_chain forced repair from leader {} failed during head QC request: {}",
                                    encode_hex(&leader),
                                    error
                                );
                            }
                        }
                    }
                    let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                        code: -32000,
                        message: "engine lock poisoned".into(),
                        data: None,
                    })?;
                    engine
                        .sign_current_head_qc(head_hash, request.round)
                        .map_err(json_rpc_error_from_node_error)?
                } else {
                    return Err(json_rpc_error_from_node_error(initial_error));
                }
            }
        };
        serde_json::to_string(&NetworkSignatureResponse {
            validator_id_hex: encode_hex(&signature.validator),
            signature_hex: encode_borsh_hex(&signature),
        })
        .map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("network head signature response should serialize: {error}"),
            data: None,
        })
    }

    pub(crate) fn process_network_timeout_message(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "network bootstrap not configured".into(),
                data: None,
            });
        };
        let request: NetworkTimeoutRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid timeout request body: {error}"),
                data: None,
            })?;
        let validator_verifiers = network_validator_verifiers(network_bootstrap)?;
        if let Some(timeout_hex) = request.timeout_hex {
            let timeout: TimeoutMessage =
                decode_borsh_hex(&timeout_hex).map_err(|message| JsonRpcErrorObject {
                    code: -32602,
                    message,
                    data: None,
                })?;
            if timeout.round != request.round {
                return Err(JsonRpcErrorObject {
                    code: -32602,
                    message: format!(
                        "timeout payload round {} does not match request round {}",
                        timeout.round, request.round
                    ),
                    data: None,
                });
            }
            self.store_timeout_message(timeout, &validator_verifiers)?;
        }
        let timeout = self.local_timeout_for_round(request.round, &validator_verifiers)?;
        let message_count = self.timeout_messages_for_round(request.round)?.len();
        serde_json::to_string(&NetworkTimeoutResponse {
            validator_id_hex: timeout.as_ref().map(|timeout| encode_hex(&timeout.sender)),
            timeout_hex: timeout.as_ref().map(encode_borsh_hex),
            message_count,
        })
        .map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("network timeout response should serialize: {error}"),
            data: None,
        })
    }

    pub(crate) fn process_network_relay_intents(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let request: NetworkRelayIntentsRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid relay body: {error}"),
                data: None,
            })?;
        let decoded = request
            .items
            .into_iter()
            .map(|params| {
                let signed_intent =
                    signed_intent_from_json(&params.signed_intent).map_err(|message| {
                        JsonRpcErrorObject {
                            code: -32602,
                            message,
                            data: None,
                        }
                    })?;
                Ok((intent_id(&signed_intent), params, signed_intent))
            })
            .collect::<Result<Vec<_>, JsonRpcErrorObject>>()?;
        let results = self.enqueue_mahi_pre_admitted_relays(decoded)?;
        serde_json::to_string(&NetworkRelayIntentsResponse { results }).map_err(|error| {
            JsonRpcErrorObject {
                code: -32000,
                message: format!("network relay response should serialize: {error}"),
                data: None,
            }
        })
    }

    pub(crate) fn process_network_relay_delegation_actions(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !peer_relay_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let request: NetworkRelayDelegationActionsRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid delegation relay body: {error}"),
                data: None,
            })?;
        if request.items.len() > MAX_SIGNED_DELEGATION_RELAY_ITEMS {
            return Err(JsonRpcErrorObject {
                code: -32602,
                message: format!(
                    "too many signed delegation relay items: max={MAX_SIGNED_DELEGATION_RELAY_ITEMS}"
                ),
                data: Some(json!({
                    "reason": "signed_delegation_relay_batch_too_large",
                    "max_items": MAX_SIGNED_DELEGATION_RELAY_ITEMS,
                    "actual_items": request.items.len(),
                })),
            });
        }
        let mut results = Vec::with_capacity(request.items.len());
        for signed_delegation_action in request.items {
            let action_id_hex = signed_delegation_action.action_id_hex.clone();
            let timestamp_ms = signed_delegation_action.timestamp_ms;
            let params = SubmitSignedDelegationActionParams {
                signed_delegation_action,
                wait_timeout_ms: None,
            };
            let result = match self.with_durable_node_rpc_write(timestamp_ms, |service| {
                service
                    .submit_signed_delegation_action(params)
                    .map_err(map_rpc_error_to_json_rpc_error)
            }) {
                Ok(response) => NetworkRelayDelegationActionResult {
                    action_id_hex: encode_hex(&response.action_id),
                    status: delegation_action_submission_status_name(response.status).into(),
                    error: None,
                },
                Err(error) => NetworkRelayDelegationActionResult {
                    action_id_hex,
                    status: "rejected".into(),
                    error: Some(error),
                },
            };
            results.push(result);
        }
        serde_json::to_string(&NetworkRelayDelegationActionsResponse { results }).map_err(|error| {
            JsonRpcErrorObject {
                code: -32000,
                message: format!("network delegation relay response should serialize: {error}"),
                data: None,
            }
        })
    }

    pub(crate) fn process_network_relay_evm_delegation_actions(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !peer_relay_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let request: NetworkRelayEvmDelegationActionsRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid EVM delegation relay body: {error}"),
                data: None,
            })?;
        if request.items.len() > MAX_SIGNED_DELEGATION_RELAY_ITEMS {
            return Err(JsonRpcErrorObject {
                code: -32602,
                message: format!(
                    "too many EVM delegation relay items: max={MAX_SIGNED_DELEGATION_RELAY_ITEMS}"
                ),
                data: Some(json!({
                    "reason": "evm_delegation_relay_batch_too_large",
                    "max_items": MAX_SIGNED_DELEGATION_RELAY_ITEMS,
                    "actual_items": request.items.len(),
                })),
            });
        }
        let mut results = Vec::with_capacity(request.items.len());
        for signed_delegation_action in request.items {
            let action_id_hex = signed_delegation_action.action_id_hex.clone();
            let timestamp_ms = signed_delegation_action.timestamp_ms;
            let params = SubmitEvmDelegationActionParams {
                signed_delegation_action,
                wait_timeout_ms: None,
            };
            let result = match self.with_durable_node_rpc_write(timestamp_ms, |service| {
                service
                    .submit_evm_delegation_action(params)
                    .map_err(map_rpc_error_to_json_rpc_error)
            }) {
                Ok(response) => NetworkRelayDelegationActionResult {
                    action_id_hex: encode_hex(&response.action_id),
                    status: delegation_action_submission_status_name(response.status).into(),
                    error: None,
                },
                Err(error) => NetworkRelayDelegationActionResult {
                    action_id_hex,
                    status: "rejected".into(),
                    error: Some(error),
                },
            };
            results.push(result);
        }
        serde_json::to_string(&NetworkRelayDelegationActionsResponse { results }).map_err(|error| {
            JsonRpcErrorObject {
                code: -32000,
                message: format!("network EVM delegation relay response should serialize: {error}"),
                data: None,
            }
        })
    }

    pub(crate) fn process_network_relay_market_maker_program_actions(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !peer_relay_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let request: NetworkRelayMarketMakerProgramActionsRequest = serde_json::from_str(body)
            .map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid market-maker program relay body: {error}"),
                data: None,
            })?;
        if request.items.len() > MAX_SIGNED_MARKET_MAKER_PROGRAM_RELAY_ITEMS {
            return Err(JsonRpcErrorObject {
                code: -32602,
                message: format!(
                    "too many signed market-maker program relay items: max={MAX_SIGNED_MARKET_MAKER_PROGRAM_RELAY_ITEMS}"
                ),
                data: Some(json!({
                    "reason": "signed_market_maker_program_relay_batch_too_large",
                    "max_items": MAX_SIGNED_MARKET_MAKER_PROGRAM_RELAY_ITEMS,
                    "actual_items": request.items.len(),
                })),
            });
        }
        let mut results = Vec::with_capacity(request.items.len());
        for signed_market_maker_program_action in request.items {
            let action_id_hex = signed_market_maker_program_action.action_id_hex.clone();
            let timestamp_ms = signed_market_maker_program_action.timestamp_ms;
            let params = SubmitSignedMarketMakerProgramActionParams {
                signed_market_maker_program_action,
                wait_timeout_ms: None,
            };
            let result = match self.with_durable_node_rpc_write(timestamp_ms, |service| {
                service
                    .submit_signed_market_maker_program_action(params)
                    .map_err(map_rpc_error_to_json_rpc_error)
            }) {
                Ok(response) => NetworkRelayMarketMakerProgramActionResult {
                    action_id_hex: encode_hex(&response.action_id),
                    status: market_maker_program_action_submission_status_name(response.status)
                        .into(),
                    error: None,
                },
                Err(error) => NetworkRelayMarketMakerProgramActionResult {
                    action_id_hex,
                    status: "rejected".into(),
                    error: Some(error),
                },
            };
            results.push(result);
        }
        serde_json::to_string(&NetworkRelayMarketMakerProgramActionsResponse { results }).map_err(
            |error| JsonRpcErrorObject {
                code: -32000,
                message: format!(
                    "network market-maker program relay response should serialize: {error}"
                ),
                data: None,
            },
        )
    }

    pub(crate) fn process_network_mahi_block(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let total_started = Instant::now();
        let decode_started = Instant::now();
        let request: NetworkMahiBlockRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid mahi block body: {error}"),
                data: None,
            })?;
        let block: MahiBlock =
            decode_borsh_hex(&request.block_hex).map_err(|message| JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            })?;
        let decode_elapsed = decode_started.elapsed();
        let response =
            self.process_decoded_network_mahi_block(block, decode_elapsed, total_started)?;
        serde_json::to_string(&response).map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("network mahi block response should serialize: {error}"),
            data: None,
        })
    }

    pub(crate) fn process_network_mahi_blocks(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let request: NetworkMahiBlockBatchRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid mahi block batch body: {error}"),
                data: None,
            })?;
        let mut results = Vec::with_capacity(request.blocks_hex.len());
        for block_hex in request.blocks_hex {
            let total_started = Instant::now();
            let decode_started = Instant::now();
            let block: MahiBlock =
                decode_borsh_hex(&block_hex).map_err(|message| JsonRpcErrorObject {
                    code: -32602,
                    message,
                    data: None,
                })?;
            let decode_elapsed = decode_started.elapsed();
            results.push(self.process_decoded_network_mahi_block(
                block,
                decode_elapsed,
                total_started,
            )?);
        }
        serde_json::to_string(&NetworkMahiBlockBatchResponse { results }).map_err(|error| {
            JsonRpcErrorObject {
                code: -32000,
                message: format!("network mahi block batch response should serialize: {error}"),
                data: None,
            }
        })
    }

    fn process_decoded_network_mahi_block(
        &self,
        block: MahiBlock,
        decode_elapsed: Duration,
        total_started: Instant,
    ) -> Result<NetworkMahiBlockResponse, JsonRpcErrorObject> {
        if self.should_drop_inbound_mahi_block(&block) {
            let reference = block.block_ref();
            let block_count = {
                let _dag = self.engine.dag_domain().track_lock();
                let mut state_guard = self.mahi_state.lock().map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "mahi lock poisoned".into(),
                    data: None,
                })?;
                let Some(state) = state_guard.as_mut() else {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: "mahi disabled".into(),
                        data: None,
                    });
                };
                state.mark_block_dropped();
                state.dag.blocks.len()
            };
            return Ok(NetworkMahiBlockResponse {
                block_ref_hex: encode_borsh_hex(&reference),
                block_count,
            });
        }
        let accept_started = Instant::now();
        let reference = self.accept_mahi_block_into_state(block.clone(), false)?;
        let accept_elapsed = accept_started.elapsed();
        let sigverify_started = Instant::now();
        self.enqueue_mahi_sigverify(reference, &block)
            .map_err(|message| JsonRpcErrorObject {
                code: -32000,
                message,
                data: None,
            })?;
        let sigverify_elapsed = sigverify_started.elapsed();
        if let Ok(mut state_guard) = self.mahi_state.lock() {
            if let Some(state) = state_guard.as_mut() {
                state.record_inbound_block_timing(
                    decode_elapsed,
                    accept_elapsed,
                    sigverify_elapsed,
                    total_started.elapsed(),
                );
            }
        }
        self.ensure_mahi_missing_fetch_worker();
        let block_count = self
            .mahi_state
            .lock()
            .ok()
            .and_then(|state| state.as_ref().map(|state| state.dag.blocks.len()))
            .unwrap_or_default();
        Ok(NetworkMahiBlockResponse {
            block_ref_hex: encode_borsh_hex(&reference),
            block_count,
        })
    }

    pub(crate) fn process_network_mahi_fetch(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let request: NetworkMahiFetchRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid mahi fetch body: {error}"),
                data: None,
            })?;
        let refs = request
            .refs_hex
            .into_iter()
            .map(|reference_hex| {
                decode_borsh_hex::<MahiBlockRef>(&reference_hex).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let blocks_hex = {
            let _dag = self.engine.dag_domain().track_lock();
            let state_guard = self.mahi_state.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "mahi lock poisoned".into(),
                data: None,
            })?;
            let Some(state) = state_guard.as_ref() else {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "mahi disabled".into(),
                    data: None,
                });
            };
            refs.into_iter()
                .filter_map(|reference| state.block_for_ref(reference))
                .map(|block| encode_borsh_hex(&block))
                .collect::<Vec<_>>()
        };
        serde_json::to_string(&NetworkMahiFetchResponse { blocks_hex }).map_err(|error| {
            JsonRpcErrorObject {
                code: -32000,
                message: format!("network mahi fetch response should serialize: {error}"),
                data: None,
            }
        })
    }

    pub(crate) fn process_network_qc_signature(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "network bootstrap not configured".into(),
                data: None,
            });
        };
        let request: NetworkProposalRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid proposal body: {error}"),
                data: None,
            })?;
        let block: Block =
            decode_borsh_hex(&request.block_hex).map_err(|message| JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            })?;
        let expected_proposer = select_leader_for_validator_set(
            block.header.round,
            block.header.chain_id,
            &network_bootstrap.validator_set,
        )
        .map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("{error:?}"),
            data: None,
        })?;
        let validator_verifiers = network_validator_verifiers(network_bootstrap)?;
        let initial_signature_result = {
            let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            engine.sign_network_block_qc(&block, &validator_verifiers, expected_proposer, 1_000)
        };
        let signature = match initial_signature_result {
            Ok(signature) => signature,
            Err(initial_error) => {
                if should_force_snapshot_catch_up(&initial_error) {
                    if let Err(error) = self.maybe_force_repair_from_validator_snapshot(
                        expected_proposer,
                        Some(block.header.height.saturating_sub(1)),
                        Some((
                            block.header.height.saturating_sub(1),
                            block.header.parent_hash,
                        )),
                        None,
                        "proposal_qc_signature_mismatch",
                    ) {
                        eprintln!(
                            "kombat_chain forced repair from proposer {} failed during proposal vote: {}",
                            encode_hex(&expected_proposer),
                            error
                        );
                    }
                    let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                        code: -32000,
                        message: "engine lock poisoned".into(),
                        data: None,
                    })?;
                    engine
                        .sign_network_block_qc(
                            &block,
                            &validator_verifiers,
                            expected_proposer,
                            1_000,
                        )
                        .map_err(json_rpc_error_from_node_error)?
                } else {
                    return Err(json_rpc_error_from_node_error(initial_error));
                }
            }
        };
        serde_json::to_string(&NetworkSignatureResponse {
            validator_id_hex: encode_hex(&signature.validator),
            signature_hex: encode_borsh_hex(&signature),
        })
        .map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("network signature response should serialize: {error}"),
            data: None,
        })
    }

    pub(crate) fn process_network_commit(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "network bootstrap not configured".into(),
                data: None,
            });
        };
        let request: NetworkCommitRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid commit body: {error}"),
                data: None,
            })?;
        let block: Block =
            decode_borsh_hex(&request.block_hex).map_err(|message| JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            })?;
        let _qc: QuorumCertificate =
            decode_borsh_hex(&request.qc_hex).map_err(|message| JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            })?;
        let _expected_proposer = select_leader_for_validator_set(
            block.header.round,
            block.header.chain_id,
            &network_bootstrap.validator_set,
        )
        .map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("{error:?}"),
            data: None,
        })?;
        let response = {
            let head = self.engine.head_snapshot();
            NetworkRoundResponse {
                height: head.head_height,
                hash_hex: encode_hex(&head.head_hash),
                item_count: block.body.items.len(),
                peer_count: network_bootstrap.bootstrap_peers.peers.len(),
                signature_count: 0,
            }
        };
        serde_json::to_string(&response).map_err(|error| JsonRpcErrorObject {
            code: -32000,
            message: format!("network commit response should serialize: {error}"),
            data: None,
        })
    }

    pub(crate) fn process_network_timeout_certificate(
        &self,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: "network bootstrap not configured".into(),
                data: None,
            });
        };
        let request: NetworkTimeoutCertificateRequest =
            serde_json::from_str(body).map_err(|error| JsonRpcErrorObject {
                code: -32602,
                message: format!("invalid timeout certificate body: {error}"),
                data: None,
            })?;
        let validator_verifiers = network_validator_verifiers(network_bootstrap)?;
        let timeout_messages = request
            .timeout_messages_hex
            .iter()
            .map(|timeout_hex| {
                decode_borsh_hex::<TimeoutMessage>(timeout_hex).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for timeout in &timeout_messages {
            self.store_timeout_message(timeout.clone(), &validator_verifiers)?;
        }
        let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "engine lock poisoned".into(),
            data: None,
        })?;
        let round = engine
            .install_timeout_messages(
                &timeout_messages,
                &validator_verifiers,
                request.timestamp_ms,
            )
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;
        serde_json::to_string(&NetworkTimeoutCertificateResponse { round }).map_err(|error| {
            JsonRpcErrorObject {
                code: -32000,
                message: format!("timeout certificate response should serialize: {error}"),
                data: None,
            }
        })
    }
}
