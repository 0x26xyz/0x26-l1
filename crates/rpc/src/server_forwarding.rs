use crate::*;
use std::thread;
use std::time::{Duration, Instant};

impl JsonRpcHttpServer {
    pub(super) fn maybe_fanout_submit_intent_to_network(
        &self,
        request: &JsonRpcRequest,
        headers: &BTreeMap<String, String>,
    ) -> Result<Option<Value>, JsonRpcErrorObject> {
        if request.method != "kombat_submitIntent" {
            return Ok(None);
        }

        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(None);
        };

        let relay_mode = headers.get(NETWORK_RELAY_HEADER).map(String::as_str);
        let submit_forwarding_enabled = network_submit_forwarding_enabled();
        let submit_params: SubmitIntentParams = serde_json::from_value(request.params.clone())
            .map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid params".into(),
                data: None,
            })?;
        let head = self.engine.head_snapshot();
        let Some(local_validator_id) = head.local_validator_id else {
            return Ok(None);
        };
        let (_, routing_leader) = self
            .network_submit_leader_snapshot(head.chain_id, &network_bootstrap.validator_set)
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;
        let local_is_leader = routing_leader == local_validator_id;

        if submit_forwarding_enabled
            && relay_mode != Some(NETWORK_SUBMIT_INTENT_FORWARD)
            && relay_mode != Some(NETWORK_SUBMIT_INTENT_RELAY)
            && !local_is_leader
        {
            return Ok(None);
        }

        let relay_after_admit = self.config.mahi_enabled;
        let started = Instant::now();
        let local_result = self.queue_submit_intent_public(submit_params, relay_after_admit)?;
        self.record_local_submit_metric(started.elapsed(), true);

        Ok(Some(local_result))
    }

    pub(super) fn maybe_forward_write_to_network_leader(
        &self,
        request: &JsonRpcRequest,
        headers: &BTreeMap<String, String>,
    ) -> Result<Option<Value>, JsonRpcErrorObject> {
        if !matches!(
            request.method.as_str(),
            "kombat_submitIntent"
                | "kombat_submitIntentBatch"
                | "kombat_submitSignedDelegationAction"
                | "kombat_submitEvmDelegationAction"
                | "kombat_submitSignedMarketMakerProgramAction"
                | "kombat_submitMarketMakerProgramAction"
                | "kombat_submitRealCapitalGovernanceAction"
                | "kombat_defineMarketMakerProgram"
                | "kombat_enrollMarketMakerProgram"
                | "kombat_faucetFundAccount"
        ) {
            return Ok(None);
        }

        if !network_submit_forwarding_enabled() {
            return Ok(None);
        }

        if matches!(
            request.method.as_str(),
            "kombat_submitMarketMakerProgramAction"
                | "kombat_submitRealCapitalGovernanceAction"
                | "kombat_defineMarketMakerProgram"
                | "kombat_enrollMarketMakerProgram"
        ) && !operator_request_is_authorized(headers, &self.config)
        {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }

        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(None);
        };

        if matches!(
            headers.get(NETWORK_RELAY_HEADER).map(String::as_str),
            Some(NETWORK_SUBMIT_INTENT_FORWARD)
                | Some(NETWORK_SUBMIT_INTENT_RELAY)
                | Some(NETWORK_SIGNED_DELEGATION_RELAY)
                | Some(NETWORK_SIGNED_MARKET_MAKER_PROGRAM_RELAY)
        ) {
            return Ok(None);
        }

        let (request_timestamp_ms, shard_key) = match request.method.as_str() {
            "kombat_submitIntent" => {
                serde_json::from_value::<SubmitIntentParams>(request.params.clone())
                    .map(|params| {
                        (
                            params.timestamp_ms,
                            Some(params.signed_intent.intent.policy_id),
                        )
                    })
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?
            }
            "kombat_submitIntentBatch" => {
                let params =
                    serde_json::from_value::<SubmitIntentBatchParams>(request.params.clone())
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                (
                    params
                        .items
                        .iter()
                        .map(|item| item.timestamp_ms)
                        .max()
                        .unwrap_or(0),
                    params
                        .items
                        .first()
                        .map(|item| item.signed_intent.intent.policy_id),
                )
            }
            "kombat_submitSignedDelegationAction" => {
                let params = serde_json::from_value::<SubmitSignedDelegationActionParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action_id =
                    decode_hex_array::<32>(&params.signed_delegation_action.action_id_hex)
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid action_id_hex".into(),
                            data: None,
                        })?;
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (
                    params.signed_delegation_action.timestamp_ms,
                    Some(u64::from_be_bytes(shard_bytes)),
                )
            }
            "kombat_submitEvmDelegationAction" => {
                let params = serde_json::from_value::<SubmitEvmDelegationActionParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action_id =
                    decode_hex_array::<32>(&params.signed_delegation_action.action_id_hex)
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid action_id_hex".into(),
                            data: None,
                        })?;
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (
                    params.signed_delegation_action.timestamp_ms,
                    Some(u64::from_be_bytes(shard_bytes)),
                )
            }
            "kombat_submitSignedMarketMakerProgramAction" => {
                let params = serde_json::from_value::<SubmitSignedMarketMakerProgramActionParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action_id = decode_hex_array::<32>(
                    &params.signed_market_maker_program_action.action_id_hex,
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid action_id_hex".into(),
                    data: None,
                })?;
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (
                    params.signed_market_maker_program_action.timestamp_ms,
                    Some(u64::from_be_bytes(shard_bytes)),
                )
            }
            "kombat_submitMarketMakerProgramAction" => {
                let params = serde_json::from_value::<SubmitMarketMakerProgramActionParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action_id = decode_hex_array::<32>(&params.action_id_hex).map_err(|_| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid action_id_hex".into(),
                        data: None,
                    }
                })?;
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (params.timestamp_ms, Some(u64::from_be_bytes(shard_bytes)))
            }
            "kombat_submitRealCapitalGovernanceAction" => {
                let params = serde_json::from_value::<SubmitRealCapitalGovernanceActionParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                let action = real_capital_governance_action_from_json(params.action)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                let action_id =
                    real_capital_governance_action_id_for_submission(&action, params.timestamp_ms);
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (params.timestamp_ms, Some(u64::from_be_bytes(shard_bytes)))
            }
            "kombat_faucetFundAccount" => {
                let params =
                    serde_json::from_value::<FaucetFundAccountParams>(request.params.clone())
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                let address = decode_hex_array::<20>(&params.address_hex).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&address[..8]);
                (current_unix_ms(), Some(u64::from_be_bytes(shard_bytes)))
            }
            "kombat_defineMarketMakerProgram" => {
                let params = serde_json::from_value::<DefineMarketMakerProgramParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action = define_market_maker_program_action_from_params(&params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                let action_id = market_maker_program_action_id_for_submission(
                    &action,
                    params.timestamp_ms,
                    params.submitter_nonce,
                );
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (params.timestamp_ms, Some(u64::from_be_bytes(shard_bytes)))
            }
            "kombat_enrollMarketMakerProgram" => {
                let params = serde_json::from_value::<EnrollMarketMakerProgramParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action = enroll_market_maker_program_action_from_params(&params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                let action_id = market_maker_program_action_id_for_submission(
                    &action,
                    params.timestamp_ms,
                    params.submitter_nonce,
                );
                let mut shard_bytes = [0_u8; 8];
                shard_bytes.copy_from_slice(&action_id[..8]);
                (params.timestamp_ms, Some(u64::from_be_bytes(shard_bytes)))
            }
            _ => unreachable!("request method already filtered"),
        };

        let resolve_leader_endpoint =
            || -> Result<Option<(Vec<String>, ValidatorId, u64)>, JsonRpcErrorObject> {
                let head = self.engine.head_snapshot();
                let (current_round, leader) = self
                    .network_submit_leader_snapshot(head.chain_id, &network_bootstrap.validator_set)
                    .map_err(|error| JsonRpcErrorObject {
                        code: -32000,
                        message: format!("{error:?}"),
                        data: None,
                    })?;
                if head
                    .local_validator_id
                    .is_some_and(|local_validator_id| leader == local_validator_id)
                {
                    return Ok(None);
                }

                self.network_validator_rpc_endpoints(&network_bootstrap.validator_set, leader)
                    .map(|endpoints| Some((endpoints, leader, current_round)))
            };
        let decode_forwarded_response = |response: reqwest::blocking::Response,
                                         request_context: &str|
         -> Result<Value, JsonRpcErrorObject> {
            let response = response
                .error_for_status()
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{request_context}: {error}"),
                    data: None,
                })?;
            let forwarded: JsonRpcResponse =
                response.json().map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("leader forwarding decode failed: {error}"),
                    data: None,
                })?;

            match (forwarded.result, forwarded.error) {
                (Some(result), None) => Ok(result),
                (None, Some(error)) => Err(error),
                _ => Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "leader returned invalid json-rpc response".into(),
                    data: None,
                }),
            }
        };

        let observer_ingress = self.engine.head_snapshot().local_validator_id.is_none();
        let forwarded_authorization = headers.get("authorization").cloned();

        if observer_ingress
            && !matches!(
                request.method.as_str(),
                "kombat_submitSignedDelegationAction"
                    | "kombat_submitEvmDelegationAction"
                    | "kombat_submitSignedMarketMakerProgramAction"
                    | "kombat_submitMarketMakerProgramAction"
                    | "kombat_submitRealCapitalGovernanceAction"
                    | "kombat_defineMarketMakerProgram"
                    | "kombat_enrollMarketMakerProgram"
                    | "kombat_faucetFundAccount"
            )
        {
            let active_validators = network_bootstrap.validator_set.active_validators();
            if active_validators.is_empty() {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "validator set has no active validators".into(),
                    data: None,
                });
            }

            let total_started = Instant::now();
            let mut forward_elapsed = Duration::ZERO;
            match request.method.as_str() {
                "kombat_submitIntent" => {
                    let policy_id = shard_key.unwrap_or(request_timestamp_ms);
                    let start_index = (policy_id as usize) % active_validators.len();
                    for attempt in 0..active_validators.len() {
                        let index = (start_index + attempt) % active_validators.len();
                        let validator = &active_validators[index];
                        let forward_started = Instant::now();
                        let send_result = self.send_network_request(
                            &validator.rpc_endpoints,
                            |client, endpoint| {
                                let builder = client
                                    .post(endpoint)
                                    .header(NETWORK_RELAY_HEADER, NETWORK_SUBMIT_INTENT_FORWARD);
                                let builder = if let Some(authorization) =
                                    forwarded_authorization.as_deref()
                                {
                                    builder.header("authorization", authorization)
                                } else {
                                    builder
                                };
                                builder.json(request)
                            },
                        );
                        forward_elapsed = forward_elapsed.saturating_add(forward_started.elapsed());
                        match send_result {
                            Ok(response) => {
                                let decoded = decode_forwarded_response(
                                    response,
                                    "mahi owner forwarding request failed",
                                )
                                .map(Some);
                                self.record_forwarded_submit_metric(
                                    total_started.elapsed(),
                                    Duration::ZERO,
                                    forward_elapsed,
                                    Duration::ZERO,
                                    0,
                                    decoded.is_ok(),
                                );
                                return decoded;
                            }
                            Err(error) => {
                                if attempt + 1 >= active_validators.len() {
                                    self.record_forwarded_submit_metric(
                                        total_started.elapsed(),
                                        Duration::ZERO,
                                        forward_elapsed,
                                        Duration::ZERO,
                                        0,
                                        false,
                                    );
                                    return Err(JsonRpcErrorObject {
                                        code: -32000,
                                        message: format!(
                                            "mahi owner forwarding retry failed: {error}"
                                        ),
                                        data: None,
                                    });
                                }
                            }
                        }
                    }
                }
                "kombat_submitIntentBatch" => {
                    let params =
                        serde_json::from_value::<SubmitIntentBatchParams>(request.params.clone())
                            .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                    if parse_env_bool_with_default(MAHI_OBSERVER_ASYNC_RELAY_ENV, false) {
                        let response = self
                            .enqueue_mahi_observer_owner_relay_batch(&active_validators, params)?;
                        self.record_forwarded_submit_metric(
                            total_started.elapsed(),
                            Duration::ZERO,
                            forward_elapsed,
                            Duration::ZERO,
                            0,
                            response.results.iter().all(|result| result.accepted),
                        );
                        return Ok(Some(json!(response)));
                    }
                    let mut grouped = BTreeMap::<usize, Vec<(usize, SubmitIntentParams)>>::new();
                    for (position, item) in params.items.into_iter().enumerate() {
                        let index = (item.signed_intent.intent.policy_id as usize)
                            % active_validators.len();
                        grouped.entry(index).or_default().push((position, item));
                    }
                    let mut merged_results = vec![
                        None::<SubmitIntentBatchItemResponse>;
                        grouped.values().map(Vec::len).sum()
                    ];

                    for (index, items) in grouped {
                        let validator = &active_validators[index];
                        let sub_items = items
                            .iter()
                            .map(|(_, item)| item.clone())
                            .collect::<Vec<_>>();
                        let sub_request = JsonRpcRequest {
                            jsonrpc: request.jsonrpc.clone(),
                            method: request.method.clone(),
                            params: serde_json::to_value(SubmitIntentBatchParams {
                                items: sub_items,
                            })
                            .map_err(|error| JsonRpcErrorObject {
                                code: -32000,
                                message: format!(
                                    "mahi owner batch forwarding encode failed: {error}"
                                ),
                                data: None,
                            })?,
                            id: request.id.clone(),
                        };
                        let forward_started = Instant::now();
                        let response = self
                            .send_network_request(&validator.rpc_endpoints, |client, endpoint| {
                                let builder = client
                                    .post(endpoint)
                                    .header(NETWORK_RELAY_HEADER, NETWORK_SUBMIT_INTENT_FORWARD);
                                let builder = if let Some(authorization) =
                                    forwarded_authorization.as_deref()
                                {
                                    builder.header("authorization", authorization)
                                } else {
                                    builder
                                };
                                builder.json(&sub_request)
                            })
                            .map_err(|error| JsonRpcErrorObject {
                                code: -32000,
                                message: format!(
                                    "mahi owner batch forwarding request failed: {error}"
                                ),
                                data: None,
                            })?;
                        forward_elapsed = forward_elapsed.saturating_add(forward_started.elapsed());
                        let value = decode_forwarded_response(
                            response,
                            "mahi owner batch forwarding decode failed",
                        )?;
                        let forwarded = serde_json::from_value::<SubmitIntentBatchResponse>(value)
                            .map_err(|error| JsonRpcErrorObject {
                                code: -32000,
                                message: format!(
                                    "mahi owner batch response decode failed: {error}"
                                ),
                                data: None,
                            })?;
                        if forwarded.results.len() != items.len() {
                            return Err(JsonRpcErrorObject {
                                code: -32000,
                                message: "mahi owner batch response length mismatch".into(),
                                data: None,
                            });
                        }
                        for ((position, _), result) in
                            items.into_iter().zip(forwarded.results.into_iter())
                        {
                            merged_results[position] = Some(result);
                        }
                    }

                    let results = merged_results
                        .into_iter()
                        .map(|result| {
                            result.ok_or_else(|| JsonRpcErrorObject {
                                code: -32000,
                                message: "mahi owner batch response missing item".into(),
                                data: None,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    self.record_forwarded_submit_metric(
                        total_started.elapsed(),
                        Duration::ZERO,
                        forward_elapsed,
                        Duration::ZERO,
                        0,
                        results.iter().all(|result| result.accepted),
                    );
                    return Ok(Some(json!(SubmitIntentBatchResponse { results })));
                }
                _ => unreachable!("observer ingress request method already filtered"),
            }
            unreachable!("mahi observer forwarding loop must return")
        }

        let max_failover_attempts = network_bootstrap
            .validator_set
            .active_validators()
            .len()
            .max(1);
        let total_started = Instant::now();
        let mut preflight_elapsed = Duration::ZERO;
        let mut forward_elapsed = Duration::ZERO;
        let mut timeout_recovery_elapsed = Duration::ZERO;
        let mut timeout_recovery_attempts = 0_u64;
        for attempt in 0..max_failover_attempts {
            let Some((leader_endpoints, leader, local_round)) = resolve_leader_endpoint()? else {
                return Ok(None);
            };
            let preflight_started = Instant::now();
            let preflight = self
                .send_network_request(&leader_endpoints, |client, endpoint| {
                    client.get(network_peer_url(endpoint, "bootstrap"))
                })
                .and_then(|response| {
                    response
                        .error_for_status()
                        .map_err(|error| error.to_string())
                });
            preflight_elapsed = preflight_elapsed.saturating_add(preflight_started.elapsed());
            let send_result: Result<reqwest::blocking::Response, String> = match preflight {
                Ok(response) => match response.json::<Value>() {
                    Ok(bootstrap) => {
                        if let Some(leader_round) =
                            bootstrap.get("current_round").and_then(Value::as_u64)
                        {
                            if leader_round
                                .saturating_add(NETWORK_FORWARD_LEADER_STALE_TOLERANCE_ROUNDS)
                                < local_round
                            {
                                Err(format!(
                                    "current leader {} is stale at round {} (local round {}, tolerance {} rounds)",
                                    encode_hex(&leader),
                                    leader_round,
                                    local_round,
                                    NETWORK_FORWARD_LEADER_STALE_TOLERANCE_ROUNDS
                                ))
                            } else {
                                let forward_started = Instant::now();
                                self.send_network_request(&leader_endpoints, |client, endpoint| {
                                    // Real-capital governance uses the generic JSON-RPC
                                    // leader-forward bucket; committed block application remains
                                    // the only production mutation path.
                                    let builder = client.post(endpoint).header(
                                        NETWORK_RELAY_HEADER,
                                        NETWORK_SUBMIT_INTENT_FORWARD,
                                    );
                                    let builder = if let Some(authorization) =
                                        forwarded_authorization.as_deref()
                                    {
                                        builder.header("authorization", authorization)
                                    } else {
                                        builder
                                    };
                                    builder.json(request)
                                })
                                .inspect(|_| {
                                    forward_elapsed =
                                        forward_elapsed.saturating_add(forward_started.elapsed())
                                })
                                .inspect_err(|_| {
                                    forward_elapsed =
                                        forward_elapsed.saturating_add(forward_started.elapsed())
                                })
                            }
                        } else {
                            let forward_started = Instant::now();
                            self.send_network_request(&leader_endpoints, |client, endpoint| {
                                let builder = client
                                    .post(endpoint)
                                    .header(NETWORK_RELAY_HEADER, NETWORK_SUBMIT_INTENT_FORWARD);
                                let builder = if let Some(authorization) =
                                    forwarded_authorization.as_deref()
                                {
                                    builder.header("authorization", authorization)
                                } else {
                                    builder
                                };
                                builder.json(request)
                            })
                            .inspect(|_| {
                                forward_elapsed =
                                    forward_elapsed.saturating_add(forward_started.elapsed())
                            })
                            .inspect_err(|_| {
                                forward_elapsed =
                                    forward_elapsed.saturating_add(forward_started.elapsed())
                            })
                        }
                    }
                    Err(_) => {
                        let forward_started = Instant::now();
                        self.send_network_request(&leader_endpoints, |client, endpoint| {
                            let builder = client
                                .post(endpoint)
                                .header(NETWORK_RELAY_HEADER, NETWORK_SUBMIT_INTENT_FORWARD);
                            let builder =
                                if let Some(authorization) = forwarded_authorization.as_deref() {
                                    builder.header("authorization", authorization)
                                } else {
                                    builder
                                };
                            builder.json(request)
                        })
                        .inspect(|_| {
                            forward_elapsed =
                                forward_elapsed.saturating_add(forward_started.elapsed())
                        })
                        .inspect_err(|_| {
                            forward_elapsed =
                                forward_elapsed.saturating_add(forward_started.elapsed())
                        })
                    }
                },
                Err(_) => {
                    let forward_started = Instant::now();
                    self.send_network_request(&leader_endpoints, |client, endpoint| {
                        let builder = client
                            .post(endpoint)
                            .header(NETWORK_RELAY_HEADER, NETWORK_SUBMIT_INTENT_FORWARD);
                        let builder =
                            if let Some(authorization) = forwarded_authorization.as_deref() {
                                builder.header("authorization", authorization)
                            } else {
                                builder
                            };
                        builder.json(request)
                    })
                    .inspect(|_| {
                        forward_elapsed = forward_elapsed.saturating_add(forward_started.elapsed())
                    })
                    .inspect_err(|_| {
                        forward_elapsed = forward_elapsed.saturating_add(forward_started.elapsed())
                    })
                }
            };

            match send_result {
                Ok(response) => {
                    let decoded =
                        decode_forwarded_response(response, "leader forwarding request failed")
                            .map(Some);
                    self.record_forwarded_submit_metric(
                        total_started.elapsed(),
                        preflight_elapsed,
                        forward_elapsed,
                        timeout_recovery_elapsed,
                        timeout_recovery_attempts,
                        decoded.is_ok(),
                    );
                    return decoded;
                }
                Err(error) => {
                    if attempt + 1 >= max_failover_attempts {
                        self.record_forwarded_submit_metric(
                            total_started.elapsed(),
                            preflight_elapsed,
                            forward_elapsed,
                            timeout_recovery_elapsed,
                            timeout_recovery_attempts,
                            false,
                        );
                        return Err(JsonRpcErrorObject {
                            code: -32000,
                            message: format!("leader forwarding retry failed: {error}"),
                            data: None,
                        });
                    }
                    timeout_recovery_attempts = timeout_recovery_attempts.saturating_add(1);
                    let recovery_started = Instant::now();
                    let recovery = self.run_network_timeout_recovery(request_timestamp_ms);
                    timeout_recovery_elapsed =
                        timeout_recovery_elapsed.saturating_add(recovery_started.elapsed());
                    if let Err(recovery_error) = recovery {
                        println!(
	                            "kombat_chain leader forwarding timeout recovery unavailable; retrying forwarded write method={} attempt={} error={}",
	                            request.method,
	                            attempt + 1,
	                            recovery_error.message
	                        );
                    }
                }
            }
        }

        unreachable!("bounded failover loop must return")
    }

    pub(super) fn maybe_fanout_owner_call_to_network(
        &self,
        request: &JsonRpcRequest,
        headers: &BTreeMap<String, String>,
    ) -> Result<Option<Value>, JsonRpcErrorObject> {
        if !matches!(
            request.method.as_str(),
            "kombat_submitOwnerCall" | "kombat_submitEvmOwnerCall"
        ) {
            return Ok(None);
        }

        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(None);
        };

        if headers.get(NETWORK_RELAY_HEADER).map(String::as_str) == Some(NETWORK_OWNER_CALL_RELAY) {
            return Ok(None);
        }

        #[derive(Clone)]
        enum OwnerFanoutParams {
            Ed25519(SubmitOwnerCallParams),
            Evm(SubmitEvmOwnerCallParams),
        }
        let submit_params = match request.method.as_str() {
            "kombat_submitOwnerCall" => OwnerFanoutParams::Ed25519(
                serde_json::from_value(request.params.clone()).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?,
            ),
            "kombat_submitEvmOwnerCall" => OwnerFanoutParams::Evm(
                serde_json::from_value(request.params.clone()).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?,
            ),
            _ => unreachable!("owner fanout method checked above"),
        };
        let decode_forwarded_response = |response: reqwest::blocking::Response,
                                         request_context: &str|
         -> Result<Value, JsonRpcErrorObject> {
            let response = response
                .error_for_status()
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{request_context}: {error}"),
                    data: None,
                })?;
            let forwarded: JsonRpcResponse =
                response.json().map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("owner call fanout decode failed: {error}"),
                    data: None,
                })?;

            match (forwarded.result, forwarded.error) {
                (Some(result), None) => Ok(result),
                (None, Some(error)) => Err(error),
                _ => Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "peer returned invalid json-rpc response".into(),
                    data: None,
                }),
            }
        };

        let _network_control = self
            .network_control
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "network control lock poisoned".into(),
                data: None,
            })?;

        let timestamp_ms = match &submit_params {
            OwnerFanoutParams::Ed25519(params) => params.timestamp_ms,
            OwnerFanoutParams::Evm(params) => params.timestamp_ms,
        };
        let (local_validator_id, local_result) =
            self.with_durable_node_rpc_write(timestamp_ms, |service| {
                let local_validator_id =
                    service
                        .engine
                        .local_validator_id()
                        .ok_or(JsonRpcErrorObject {
                            code: -32000,
                            message: "local validator id unavailable".into(),
                            data: None,
                        })?;
                let local_result = match submit_params.clone() {
                    OwnerFanoutParams::Ed25519(params) => {
                        submit_owner_call_locally(service, params)?
                    }
                    OwnerFanoutParams::Evm(params) => {
                        submit_evm_owner_call_locally(service, params)?
                    }
                };
                Ok((local_validator_id, local_result))
            })?;

        for validator in network_bootstrap.validator_set.active_validators() {
            if validator.validator_id == local_validator_id {
                continue;
            }
            let response = self
                .send_network_request(&validator.rpc_endpoints, |client, endpoint| {
                    client
                        .post(endpoint)
                        .header(NETWORK_RELAY_HEADER, NETWORK_OWNER_CALL_RELAY)
                        .json(request)
                })
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!(
                        "owner call fanout request to validator {} failed: {error}",
                        encode_hex(&validator.validator_id)
                    ),
                    data: None,
                })?;
            decode_forwarded_response(response, "owner call fanout failed")?;
        }

        Ok(Some(local_result))
    }

    pub(super) fn maybe_fanout_delegation_action_to_network(
        &self,
        request: &JsonRpcRequest,
        headers: &BTreeMap<String, String>,
    ) -> Result<Option<Value>, JsonRpcErrorObject> {
        if !is_delegation_write_method(&request.method) {
            return Ok(None);
        }

        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(None);
        };

        if headers.get(NETWORK_RELAY_HEADER).map(String::as_str) == Some(NETWORK_DELEGATION_RELAY) {
            return Ok(None);
        }

        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32010,
                message: "operator authorization required".into(),
                data: None,
            });
        }

        let wait_request = match request.method.as_str() {
            "kombat_submitDelegationAction" => {
                let params =
                    serde_json::from_value::<SubmitDelegationActionParams>(request.params.clone())
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let action_id = decode_hex_array::<32>(&params.action_id_hex).map_err(|_| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid action_id_hex".into(),
                        data: None,
                    }
                })?;
                Some((action_id, params.wait_timeout_ms))
            }
            "kombat_submitDepositToDelegation" => {
                let params = serde_json::from_value::<SubmitDepositToDelegationParams>(
                    request.params.clone(),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
                let delegation_id =
                    decode_hex_array::<32>(&params.delegation_id_hex).map_err(|_| {
                        JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid delegation_id_hex".into(),
                            data: None,
                        }
                    })?;
                let amount = params
                    .amount
                    .parse::<u128>()
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid amount".into(),
                        data: None,
                    })?;
                let action = BlockDelegationAction::Deposit {
                    principal: params.principal,
                    delegation_id,
                    amount,
                };
                let action_id = delegation_action_id_for_rpc(
                    &action,
                    params.timestamp_ms,
                    params.submitter_nonce,
                );
                Some((action_id, params.wait_timeout_ms))
            }
            _ => None,
        };

        let decode_forwarded_response = |response: reqwest::blocking::Response,
                                         request_context: &str|
         -> Result<Value, JsonRpcErrorObject> {
            let response = response
                .error_for_status()
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{request_context}: {error}"),
                    data: None,
                })?;
            let forwarded: JsonRpcResponse =
                response.json().map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("delegation action fanout decode failed: {error}"),
                    data: None,
                })?;

            match (forwarded.result, forwarded.error) {
                (Some(result), None) => Ok(result),
                (None, Some(error)) => Err(error),
                _ => Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "peer returned invalid json-rpc response".into(),
                    data: None,
                }),
            }
        };

        let (local_result, peer_failures) = {
            let _network_control = self
                .network_control
                .lock()
                .map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "network control lock poisoned".into(),
                    data: None,
                })?;

            let timestamp_ms = delegation_write_timestamp_ms(&request.method, &request.params)?;
            let (local_validator_id, local_result) =
                self.with_durable_node_rpc_write(timestamp_ms, |service| {
                    let local_validator_id =
                        service
                            .engine
                            .local_validator_id()
                            .ok_or(JsonRpcErrorObject {
                                code: -32000,
                                message: "local validator id unavailable".into(),
                                data: None,
                            })?;
                    let local_result = submit_delegation_action_locally(
                        service,
                        &request.method,
                        request.params.clone(),
                    )?;
                    Ok((local_validator_id, local_result))
                })?;

            let forwarded_authorization = headers.get("authorization").cloned();
            let mut peer_failures = Vec::new();
            for validator in network_bootstrap.validator_set.active_validators() {
                if validator.validator_id == local_validator_id {
                    continue;
                }
                let send_result =
                    self.send_network_request(&validator.rpc_endpoints, |client, endpoint| {
                        let builder = client
                            .post(endpoint)
                            .header(NETWORK_RELAY_HEADER, NETWORK_DELEGATION_RELAY);
                        let builder =
                            if let Some(authorization) = forwarded_authorization.as_deref() {
                                builder.header("authorization", authorization)
                            } else {
                                builder
                            };
                        builder.json(request)
                    });
                match send_result {
                    Ok(response) => {
                        if let Err(error) =
                            decode_forwarded_response(response, "delegation action fanout failed")
                        {
                            peer_failures.push(PeerFanoutFailure {
                                validator_id: validator.validator_id,
                                message: error.message,
                            });
                        }
                    }
                    Err(error) => peer_failures.push(PeerFanoutFailure {
                        validator_id: validator.validator_id,
                        message: format!("delegation action fanout request failed: {error}"),
                    }),
                }
            }
            (local_result, peer_failures)
        };

        let local_result = if let Some((action_id, wait_timeout_ms)) = wait_request {
            self.attach_delegation_action_wait_result_for_id(
                local_result,
                action_id,
                wait_timeout_ms,
            )?
        } else {
            local_result
        };
        Ok(Some(attach_peer_failures(local_result, &peer_failures)))
    }

    pub(super) fn submit_delegation_action_rpc(
        &self,
        params: SubmitDelegationActionParams,
        allow_wait: bool,
    ) -> Result<Value, JsonRpcErrorObject> {
        if allow_wait {
            validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        }
        let timestamp_ms = params.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_delegation_action(params.clone())
                .map(|response| delegation_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        if allow_wait {
            self.attach_delegation_action_wait_result(response, &params)
        } else {
            Ok(response)
        }
    }

    pub(super) fn submit_deposit_to_delegation_rpc(
        &self,
        params: SubmitDepositToDelegationParams,
        allow_wait: bool,
    ) -> Result<Value, JsonRpcErrorObject> {
        if allow_wait {
            validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        }
        let delegation_id =
            decode_hex_array::<32>(&params.delegation_id_hex).map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid delegation_id_hex".into(),
                data: None,
            })?;
        let amount = params
            .amount
            .parse::<u128>()
            .map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid amount".into(),
                data: None,
            })?;
        let action = BlockDelegationAction::Deposit {
            principal: params.principal,
            delegation_id,
            amount,
        };
        let action_id =
            delegation_action_id_for_rpc(&action, params.timestamp_ms, params.submitter_nonce);
        let timestamp_ms = params.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_deposit_to_delegation(params.clone())
                .map(|response| delegation_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        if allow_wait {
            self.attach_delegation_action_wait_result_for_id(
                response,
                action_id,
                params.wait_timeout_ms,
            )
        } else {
            Ok(response)
        }
    }

    pub(super) fn submit_signed_delegation_action_rpc(
        &self,
        params: SubmitSignedDelegationActionParams,
        allow_gossip: bool,
    ) -> Result<Value, JsonRpcErrorObject> {
        validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        let action_id = decode_hex_array::<32>(&params.signed_delegation_action.action_id_hex)
            .map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid action_id_hex".into(),
                data: None,
            })?;
        let timestamp_ms = params.signed_delegation_action.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_signed_delegation_action(params.clone())
                .map(|response| delegation_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        let peer_failures = if allow_gossip && signed_delegation_gossip_enabled() {
            self.relay_signed_delegation_action_to_network(&params.signed_delegation_action)
        } else {
            Vec::new()
        };
        let response = self.attach_delegation_action_wait_result_for_id(
            response,
            action_id,
            params.wait_timeout_ms,
        )?;
        Ok(attach_peer_failures(response, &peer_failures))
    }

    pub(super) fn submit_evm_delegation_action_rpc(
        &self,
        params: SubmitEvmDelegationActionParams,
        allow_gossip: bool,
    ) -> Result<Value, JsonRpcErrorObject> {
        validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        let action_id = decode_hex_array::<32>(&params.signed_delegation_action.action_id_hex)
            .map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid action_id_hex".into(),
                data: None,
            })?;
        let timestamp_ms = params.signed_delegation_action.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_evm_delegation_action(params.clone())
                .map(|response| delegation_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        let peer_failures = if allow_gossip && signed_delegation_gossip_enabled() {
            self.relay_evm_delegation_action_to_network(&params.signed_delegation_action)
        } else {
            Vec::new()
        };
        let response = self.attach_delegation_action_wait_result_for_id(
            response,
            action_id,
            params.wait_timeout_ms,
        )?;
        Ok(attach_peer_failures(response, &peer_failures))
    }

    pub(super) fn submit_signed_market_maker_program_action_rpc(
        &self,
        params: SubmitSignedMarketMakerProgramActionParams,
        allow_gossip: bool,
    ) -> Result<Value, JsonRpcErrorObject> {
        validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        let action_id =
            decode_hex_array::<32>(&params.signed_market_maker_program_action.action_id_hex)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid action_id_hex".into(),
                    data: None,
                })?;
        let timestamp_ms = params.signed_market_maker_program_action.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_signed_market_maker_program_action(params.clone())
                .map(|response| market_maker_program_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        let peer_failures = if allow_gossip && signed_market_maker_program_gossip_enabled() {
            self.relay_signed_market_maker_program_action_to_network(
                &params.signed_market_maker_program_action,
            )
        } else {
            Vec::new()
        };
        let response = self.attach_market_maker_program_action_wait_result_for_id(
            response,
            action_id,
            params.wait_timeout_ms,
        )?;
        Ok(attach_peer_failures(response, &peer_failures))
    }

    pub(super) fn submit_market_maker_program_action_rpc(
        &self,
        params: SubmitMarketMakerProgramActionParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        let action_id =
            decode_hex_array::<32>(&params.action_id_hex).map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid action_id_hex".into(),
                data: None,
            })?;
        let timestamp_ms = params.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_market_maker_program_action(params.clone())
                .map(|response| market_maker_program_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        self.attach_market_maker_program_action_wait_result_for_id(
            response,
            action_id,
            params.wait_timeout_ms,
        )
    }

    pub(super) fn submit_real_capital_governance_action_rpc(
        &self,
        params: SubmitRealCapitalGovernanceActionParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        let timestamp_ms = params.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .submit_real_capital_governance_action(params)
                .map(|response| submit_real_capital_governance_action_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        Ok(response)
    }

    pub(super) fn define_market_maker_program_rpc(
        &self,
        params: DefineMarketMakerProgramParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        let wait_timeout_ms = params.wait_timeout_ms;
        let timestamp_ms = params.timestamp_ms;
        let (response, program_id) = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .define_market_maker_program(params)
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        let action_id = response.action_id;
        let mut value = market_maker_program_action_submission_response_to_json(&response);
        if let Value::Object(object) = &mut value {
            object.insert("program_id_hex".into(), json!(encode_hex(&program_id)));
        }
        self.attach_market_maker_program_action_wait_result_for_id(
            value,
            action_id,
            wait_timeout_ms,
        )
    }

    pub(super) fn enroll_market_maker_program_rpc(
        &self,
        params: EnrollMarketMakerProgramParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        validate_delegation_action_wait_timeout(params.wait_timeout_ms)?;
        let wait_timeout_ms = params.wait_timeout_ms;
        let timestamp_ms = params.timestamp_ms;
        let response = self.with_durable_node_rpc_write(timestamp_ms, |service| {
            service
                .enroll_market_maker_program(params)
                .map_err(map_rpc_error_to_json_rpc_error)
        })?;
        let action_id = response.action_id;
        self.attach_market_maker_program_action_wait_result_for_id(
            market_maker_program_action_submission_response_to_json(&response),
            action_id,
            wait_timeout_ms,
        )
    }

    fn relay_signed_delegation_action_to_network(
        &self,
        signed_delegation_action: &SignedDelegationActionJson,
    ) -> Vec<PeerFanoutFailure> {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Vec::new();
        };
        let Some(local_validator_id) = self.engine.head_snapshot().local_validator_id else {
            return Vec::new();
        };
        let request = NetworkRelayDelegationActionsRequest {
            items: vec![signed_delegation_action.clone()],
        };
        let mut peer_failures = Vec::new();
        for validator in network_bootstrap.validator_set.active_validators() {
            if validator.validator_id == local_validator_id {
                continue;
            }
            match self.send_network_json_request::<_, NetworkRelayDelegationActionsResponse>(
                &validator.rpc_endpoints,
                "relay-delegation-actions",
                &request,
            ) {
                Ok(response) => {
                    let Some(result) = response.results.into_iter().next() else {
                        peer_failures.push(PeerFanoutFailure {
                            validator_id: validator.validator_id,
                            message: "signed delegation relay returned no result".into(),
                        });
                        continue;
                    };
                    if let Some(error) = result.error {
                        peer_failures.push(PeerFanoutFailure {
                            validator_id: validator.validator_id,
                            message: error.message,
                        });
                    }
                }
                Err(error) => peer_failures.push(PeerFanoutFailure {
                    validator_id: validator.validator_id,
                    message: format!("signed delegation relay request failed: {error:?}"),
                }),
            }
        }
        peer_failures
    }

    fn relay_evm_delegation_action_to_network(
        &self,
        signed_delegation_action: &SignedEvmDelegationActionJson,
    ) -> Vec<PeerFanoutFailure> {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Vec::new();
        };
        let Some(local_validator_id) = self.engine.head_snapshot().local_validator_id else {
            return Vec::new();
        };
        let request = NetworkRelayEvmDelegationActionsRequest {
            items: vec![signed_delegation_action.clone()],
        };
        let mut peer_failures = Vec::new();
        for validator in network_bootstrap.validator_set.active_validators() {
            if validator.validator_id == local_validator_id {
                continue;
            }
            match self.send_network_json_request::<_, NetworkRelayDelegationActionsResponse>(
                &validator.rpc_endpoints,
                "relay-evm-delegation-actions",
                &request,
            ) {
                Ok(response) => {
                    let Some(result) = response.results.into_iter().next() else {
                        peer_failures.push(PeerFanoutFailure {
                            validator_id: validator.validator_id,
                            message: "EVM delegation relay returned no result".into(),
                        });
                        continue;
                    };
                    if let Some(error) = result.error {
                        peer_failures.push(PeerFanoutFailure {
                            validator_id: validator.validator_id,
                            message: error.message,
                        });
                    }
                }
                Err(error) => peer_failures.push(PeerFanoutFailure {
                    validator_id: validator.validator_id,
                    message: format!("EVM delegation relay request failed: {error:?}"),
                }),
            }
        }
        peer_failures
    }

    fn relay_signed_market_maker_program_action_to_network(
        &self,
        signed_action: &SignedMarketMakerProgramActionJson,
    ) -> Vec<PeerFanoutFailure> {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Vec::new();
        };
        let Some(local_validator_id) = self.engine.head_snapshot().local_validator_id else {
            return Vec::new();
        };
        let request = NetworkRelayMarketMakerProgramActionsRequest {
            items: vec![signed_action.clone()],
        };
        let mut peer_failures = Vec::new();
        for validator in network_bootstrap.validator_set.active_validators() {
            if validator.validator_id == local_validator_id {
                continue;
            }
            match self
                .send_network_json_request::<_, NetworkRelayMarketMakerProgramActionsResponse>(
                    &validator.rpc_endpoints,
                    "relay-market-maker-program-actions",
                    &request,
                ) {
                Ok(response) => {
                    let Some(result) = response.results.into_iter().next() else {
                        peer_failures.push(PeerFanoutFailure {
                            validator_id: validator.validator_id,
                            message: "signed market-maker program relay returned no result".into(),
                        });
                        continue;
                    };
                    if let Some(error) = result.error {
                        peer_failures.push(PeerFanoutFailure {
                            validator_id: validator.validator_id,
                            message: error.message,
                        });
                    }
                }
                Err(error) => peer_failures.push(PeerFanoutFailure {
                    validator_id: validator.validator_id,
                    message: format!("signed market-maker program relay request failed: {error:?}"),
                }),
            }
        }
        peer_failures
    }

    fn attach_delegation_action_wait_result(
        &self,
        response: Value,
        params: &SubmitDelegationActionParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        let Some(timeout_ms) = params.wait_timeout_ms else {
            return Ok(response);
        };
        let action_id =
            decode_hex_array::<32>(&params.action_id_hex).map_err(|_| JsonRpcErrorObject {
                code: -32602,
                message: "invalid action_id_hex".into(),
                data: None,
            })?;
        self.attach_delegation_action_wait_result_for_id(response, action_id, Some(timeout_ms))
    }

    fn attach_delegation_action_wait_result_for_id(
        &self,
        response: Value,
        action_id: DelegationActionId,
        timeout_ms: Option<u64>,
    ) -> Result<Value, JsonRpcErrorObject> {
        let Some(timeout_ms) = timeout_ms else {
            return Ok(response);
        };
        let status = self.wait_for_delegation_action_status(action_id, timeout_ms)?;
        if let Some(DelegationActionReceipt::Rejected {
            action_kind,
            reason,
        }) = status.receipt.as_ref()
        {
            return Err(delegation_action_wait_rejection_error(
                &status,
                action_kind,
                reason,
            ));
        }
        Ok(attach_delegation_action_status(response, &status))
    }

    fn delegation_action_status_snapshot(
        &self,
        action_id: DelegationActionId,
    ) -> Result<DelegationActionStatusResponse, JsonRpcErrorObject> {
        let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "engine lock poisoned".into(),
            data: None,
        })?;
        let service = NodeRpcService::new(&mut engine);
        Ok(service.delegation_action_status_for_id(action_id))
    }

    fn wait_for_delegation_action_status(
        &self,
        action_id: DelegationActionId,
        timeout_ms: u64,
    ) -> Result<DelegationActionStatusResponse, JsonRpcErrorObject> {
        let timeout = Duration::from_millis(timeout_ms.min(DELEGATION_ACTION_WAIT_MAX_MS));
        let started_at = Instant::now();
        loop {
            let status = self.delegation_action_status_snapshot(action_id)?;
            if status.status == DelegationActionStatus::Processed || started_at.elapsed() >= timeout
            {
                return Ok(status);
            }
            let remaining = timeout.saturating_sub(started_at.elapsed());
            thread::sleep(remaining.min(Duration::from_millis(DELEGATION_ACTION_WAIT_POLL_MS)));
        }
    }

    fn attach_market_maker_program_action_wait_result_for_id(
        &self,
        response: Value,
        action_id: MarketMakerProgramActionId,
        timeout_ms: Option<u64>,
    ) -> Result<Value, JsonRpcErrorObject> {
        let Some(timeout_ms) = timeout_ms else {
            return Ok(response);
        };
        let status = self.wait_for_market_maker_program_action_status(action_id, timeout_ms)?;
        if let Some(MarketMakerProgramActionReceipt::Rejected {
            action_kind,
            reason,
        }) = status.receipt.as_ref()
        {
            return Err(market_maker_program_action_wait_rejection_error(
                &status,
                action_kind,
                reason,
            ));
        }
        Ok(attach_market_maker_program_action_status(response, &status))
    }

    fn market_maker_program_action_status_snapshot(
        &self,
        action_id: MarketMakerProgramActionId,
    ) -> Result<MarketMakerProgramActionStatusResponse, JsonRpcErrorObject> {
        let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "engine lock poisoned".into(),
            data: None,
        })?;
        let service = NodeRpcService::new(&mut engine);
        Ok(service.market_maker_program_action_status_for_id(action_id))
    }

    fn wait_for_market_maker_program_action_status(
        &self,
        action_id: MarketMakerProgramActionId,
        timeout_ms: u64,
    ) -> Result<MarketMakerProgramActionStatusResponse, JsonRpcErrorObject> {
        let timeout = Duration::from_millis(timeout_ms.min(DELEGATION_ACTION_WAIT_MAX_MS));
        let started_at = Instant::now();
        loop {
            let status = self.market_maker_program_action_status_snapshot(action_id)?;
            if status.status == MarketMakerProgramActionStatus::Processed
                || started_at.elapsed() >= timeout
            {
                return Ok(status);
            }
            let remaining = timeout.saturating_sub(started_at.elapsed());
            thread::sleep(remaining.min(Duration::from_millis(DELEGATION_ACTION_WAIT_POLL_MS)));
        }
    }

    fn faucet_snapshot_for_address(
        &self,
        address: [u8; 20],
    ) -> Result<FaucetFundAccountResponse, JsonRpcErrorObject> {
        let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "engine lock poisoned".into(),
            data: None,
        })?;
        let service = NodeRpcService::new(&mut engine);
        Ok(service.faucet_fund_account_snapshot(address))
    }

    pub(super) fn wait_for_faucet_targets(
        &self,
        address: [u8; 20],
        policy_target: U256,
        evm_target: U256,
        timeout_ms: u64,
    ) -> Result<FaucetFundAccountResponse, JsonRpcErrorObject> {
        let timeout = Duration::from_millis(timeout_ms.min(DELEGATION_ACTION_WAIT_MAX_MS));
        let started_at = Instant::now();
        loop {
            let response = self.faucet_snapshot_for_address(address)?;
            let policy = response
                .policy_usdc_balance
                .parse::<U256>()
                .unwrap_or(U256::ZERO);
            let evm = response
                .evm_usdc_balance
                .parse::<U256>()
                .unwrap_or(U256::ZERO);
            if policy >= policy_target && evm >= evm_target {
                return Ok(response);
            }
            if started_at.elapsed() >= timeout {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "faucet funding pending".into(),
                    data: Some(json!({
                        "reason": "faucet_funding_pending",
                        "address_hex": encode_hex(&address),
                        "policy_usdc_balance": response.policy_usdc_balance,
                        "evm_usdc_balance": response.evm_usdc_balance,
                    })),
                });
            }
            let remaining = timeout.saturating_sub(started_at.elapsed());
            thread::sleep(remaining.min(Duration::from_millis(DELEGATION_ACTION_WAIT_POLL_MS)));
        }
    }

    pub(super) fn maybe_fanout_create_account_to_network(
        &self,
        request: &JsonRpcRequest,
        headers: &BTreeMap<String, String>,
    ) -> Result<Option<Value>, JsonRpcErrorObject> {
        if request.method != "kombat_createAccount" && request.method != "kombat_createEvmAccount" {
            return Ok(None);
        }

        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Ok(None);
        };

        if headers.get(NETWORK_RELAY_HEADER).map(String::as_str)
            == Some(NETWORK_CREATE_ACCOUNT_RELAY)
        {
            return Ok(None);
        }

        let decode_forwarded_response = |response: reqwest::blocking::Response,
                                         request_context: &str|
         -> Result<Value, JsonRpcErrorObject> {
            let response = response
                .error_for_status()
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{request_context}: {error}"),
                    data: None,
                })?;
            let forwarded: JsonRpcResponse =
                response.json().map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("create account fanout decode failed: {error}"),
                    data: None,
                })?;

            match (forwarded.result, forwarded.error) {
                (Some(result), None) => Ok(result),
                (None, Some(error)) => Err(error),
                _ => Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "peer returned invalid json-rpc response".into(),
                    data: None,
                }),
            }
        };

        let _network_control = self
            .network_control
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "network control lock poisoned".into(),
                data: None,
            })?;

        let (local_validator_id, local_result) = {
            let local_validator_id =
                self.engine
                    .head_snapshot()
                    .local_validator_id
                    .ok_or(JsonRpcErrorObject {
                        code: -32000,
                        message: "local validator id unavailable".into(),
                        data: None,
                    })?;
            let local_result = match request.method.as_str() {
                "kombat_createAccount" => {
                    let params: CreateAccountParams =
                        serde_json::from_value(request.params.clone()).map_err(|_| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid params".into(),
                                data: None,
                            }
                        })?;
                    self.create_account_rpc(params)?
                }
                "kombat_createEvmAccount" => {
                    let params: CreateEvmAccountParams =
                        serde_json::from_value(request.params.clone()).map_err(|_| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid params".into(),
                                data: None,
                            }
                        })?;
                    self.create_evm_account_rpc(params)?
                }
                _ => unreachable!("method checked above"),
            };
            (local_validator_id, local_result)
        };

        for validator in network_bootstrap.validator_set.active_validators() {
            if validator.validator_id == local_validator_id {
                continue;
            }
            let response = self
                .send_network_request(&validator.rpc_endpoints, |client, endpoint| {
                    client
                        .post(endpoint)
                        .header(NETWORK_RELAY_HEADER, NETWORK_CREATE_ACCOUNT_RELAY)
                        .json(request)
                })
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!(
                        "create account fanout request to validator {} failed: {error}",
                        encode_hex(&validator.validator_id)
                    ),
                    data: None,
                })?;
            decode_forwarded_response(response, "create account fanout failed")?;
        }

        Ok(Some(local_result))
    }
}
