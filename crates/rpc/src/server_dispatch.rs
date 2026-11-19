use crate::*;

impl JsonRpcHttpServer {
    pub(super) fn dispatch_json_rpc(
        &self,
        request: JsonRpcRequest,
        headers: &BTreeMap<String, String>,
    ) -> Result<Value, JsonRpcErrorObject> {
        if request.jsonrpc != "2.0" {
            return Err(JsonRpcErrorObject {
                code: -32600,
                message: "invalid jsonrpc version".into(),
                data: None,
            });
        }

        if request.method == "kombat_runNetworkRound" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: RunNetworkRoundParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self
                .run_network_round(params.timestamp_ms, params.max_items.unwrap_or(100))
                .map(|response| {
                    json!({
                        "height": response.height,
                        "hash_hex": response.hash_hex,
                        "item_count": response.item_count,
                        "peer_count": response.peer_count,
                        "signature_count": response.signature_count,
                    })
                });
        }

        if request.method == "kombat_runNetworkTimeoutRecovery" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: RunNetworkTimeoutRecoveryParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self
                .run_network_timeout_recovery(params.timestamp_ms)
                .map(|response| {
                    json!({
                        "round": response.round,
                        "peer_count": response.peer_count,
                        "message_count": response.message_count,
                        "leader_hex": response.leader_hex,
                    })
                });
        }

        if let Some(result) = self.maybe_fanout_create_account_to_network(&request, headers)? {
            return Ok(result);
        }

        if request.method == "kombat_createAccount" {
            let params: CreateAccountParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.create_account_rpc(params);
        }

        if request.method == "kombat_createEvmAccount" {
            let params: CreateEvmAccountParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.create_evm_account_rpc(params);
        }

        if request.method == "kombat_createMandate" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: CreateMandateParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .create_mandate(params)
                    .map(|response| mandate_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if request.method == "kombat_submitMandateAction" {
            let params: SubmitMandateActionParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .submit_mandate_action(params)
                    .map(|response| mandate_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if request.method == "kombat_submitEvmMandateAction" {
            let params: SubmitEvmMandateActionParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .submit_evm_mandate_action(params)
                    .map(|response| mandate_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if request.method == "kombat_suspendMandate" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: MandateLifecycleParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .suspend_mandate(params)
                    .map(|response| mandate_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if request.method == "kombat_resumeMandate" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: MandateLifecycleParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .resume_mandate(params)
                    .map(|response| mandate_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if request.method == "kombat_revokeMandate" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: RevokeMandateParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .revoke_mandate(params)
                    .map(|response| mandate_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if request.method == "kombat_expireDueMandates" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: ExpireDueMandatesParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
                service
                    .expire_due_mandates(params.timestamp_ms)
                    .map(|response| {
                        json!(response
                            .iter()
                            .map(mandate_response_to_json)
                            .collect::<Vec<_>>())
                    })
                    .map_err(map_rpc_error_to_json_rpc_error)
            });
        }

        if let Some(result) = self.maybe_fanout_owner_call_to_network(&request, headers)? {
            return Ok(result);
        }

        if is_legacy_delegation_bridge_method(&request.method)
            && !legacy_delegation_bridge_enabled()
        {
            return Err(legacy_delegation_bridge_disabled_error(&request.method));
        }

        if let Some(result) = self.maybe_fanout_delegation_action_to_network(&request, headers)? {
            return Ok(result);
        }

        if let Some(result) = self.maybe_fanout_submit_intent_to_network(&request, headers)? {
            return Ok(result);
        }

        if let Some(result) = self.maybe_forward_write_to_network_leader(&request, headers)? {
            return Ok(result);
        }

        if request.method == "kombat_submitDelegationAction" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: SubmitDelegationActionParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            let allow_wait = headers.get(NETWORK_RELAY_HEADER).map(String::as_str)
                != Some(NETWORK_DELEGATION_RELAY);
            return self.submit_delegation_action_rpc(params, allow_wait);
        }

        if request.method == "kombat_submitDepositToDelegation" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: SubmitDepositToDelegationParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            let allow_wait = headers.get(NETWORK_RELAY_HEADER).map(String::as_str)
                != Some(NETWORK_DELEGATION_RELAY);
            return self.submit_deposit_to_delegation_rpc(params, allow_wait);
        }

        if request.method == "kombat_submitSignedDelegationAction" {
            let params: SubmitSignedDelegationActionParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            // Direct /network/relay-delegation-actions submissions bypass this JSON-RPC handler
            // entirely. This header check is defense-in-depth for relayed requests that re-enter
            // through the public JSON-RPC method.
            let allow_gossip = headers.get(NETWORK_RELAY_HEADER).map(String::as_str)
                != Some(NETWORK_SIGNED_DELEGATION_RELAY);
            return self.submit_signed_delegation_action_rpc(params, allow_gossip);
        }

        if request.method == "kombat_submitEvmDelegationAction" {
            let params: SubmitEvmDelegationActionParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            let allow_gossip = headers.get(NETWORK_RELAY_HEADER).map(String::as_str)
                != Some(NETWORK_SIGNED_DELEGATION_RELAY);
            return self.submit_evm_delegation_action_rpc(params, allow_gossip);
        }

        if request.method == "kombat_submitSignedMarketMakerProgramAction" {
            let params: SubmitSignedMarketMakerProgramActionParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            // Direct /network/relay-market-maker-program-actions submissions bypass this JSON-RPC
            // handler entirely. This header check is defense-in-depth for relayed requests that
            // re-enter through the public JSON-RPC method.
            let allow_gossip = headers.get(NETWORK_RELAY_HEADER).map(String::as_str)
                != Some(NETWORK_SIGNED_MARKET_MAKER_PROGRAM_RELAY);
            return self.submit_signed_market_maker_program_action_rpc(params, allow_gossip);
        }

        if request.method == "kombat_submitMarketMakerProgramAction" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: SubmitMarketMakerProgramActionParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.submit_market_maker_program_action_rpc(params);
        }

        if request.method == "kombat_submitRealCapitalGovernanceAction" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: SubmitRealCapitalGovernanceActionParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.submit_real_capital_governance_action_rpc(params);
        }

        if request.method == "kombat_defineMarketMakerProgram" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: DefineMarketMakerProgramParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.define_market_maker_program_rpc(params);
        }

        if request.method == "kombat_enrollMarketMakerProgram" {
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            let params: EnrollMarketMakerProgramParams = serde_json::from_value(request.params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            return self.enroll_market_maker_program_rpc(params);
        }

        if request.method == "kombat_getConsensusMetrics" {
            // Slice 1 reads these snapshots from separate domain read models, so the payload is
            // intentionally not cross-domain atomic.
            let metrics = self.engine.consensus_metrics_snapshot();
            let frontier = self.engine.proposer_frontier_stats_snapshot();
            let engine_lock_profile = self.engine.lock_profile_snapshot();
            let network_round_lock_profile = self.network_round_lock_profile_snapshot();
            return Ok(consensus_metrics_to_json(
                &metrics,
                &frontier,
                &engine_lock_profile,
                &network_round_lock_profile,
                self.mahi_state_snapshot().as_ref(),
                self.network_peer_reachability().as_ref(),
                &self.peer_transport_sessions_snapshot(),
                &self.peer_transport_endpoint_backoff_snapshot(),
                &self.inbound_peer_transport_sessions_snapshot(),
                &self.peer_snapshot_repair_state_snapshot(),
                &self.rpc_submit_metrics_snapshot(),
                &self.submit_queue_stats_snapshot(),
            ));
        }

        if request.method == "kombat_submitIntentBatch" {
            let params: SubmitIntentBatchParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            let started = Instant::now();
            let (results, _accepted_relays) = self.submit_intent_batch_locally(params)?;
            let ok = results.iter().all(|result| result.accepted);
            self.record_local_submit_metric(started.elapsed(), ok);
            return Ok(json!(SubmitIntentBatchResponse { results }));
        }

        if request.method == "kombat_submitIntent" {
            let params: SubmitIntentParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            let started = Instant::now();
            let result = self.queue_submit_intent_public(params, false);
            self.record_local_submit_metric(started.elapsed(), result.is_ok());
            return result;
        }

        if request.method == "kombat_produceBlock" {
            if !self.config.manual_block_production_enabled {
                return Err(JsonRpcErrorObject {
                    code: -32601,
                    message: "method not found".into(),
                    data: None,
                });
            }
            if !operator_request_is_authorized(headers, &self.config) {
                return Err(JsonRpcErrorObject {
                    code: -32010,
                    message: "operator authorization required".into(),
                    data: None,
                });
            }
            if !self.wait_for_local_submit_idle() {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: "timed out waiting for local submit queue to drain".into(),
                    data: None,
                });
            }
            let params: ProduceBlockParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            let block = self
                .engine
                .produce_block_from_mempool_forced(
                    params.timestamp_ms,
                    params.max_items.unwrap_or(100),
                )
                .map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "engine lock poisoned".into(),
                    data: None,
                })?
                .map_err(|error| {
                    map_rpc_error_to_json_rpc_error(RpcError::Node(format!("{error:?}")))
                })?;
            return Ok(produce_block_response_to_json(&ProduceBlockResponse {
                height: block.height,
                hash: block.hash,
                state_root: block.state_root,
                item_count: block.item_count,
            }));
        }

        if request.method == "kombat_faucetFundAccount" {
            let params: FaucetFundAccountParams =
                serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
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
            let policy_target = match params.policy_usdc {
                Some(amount) => amount.parse::<U256>().map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "policy_usdc must parse as uint256".into(),
                    data: None,
                })?,
                None => U256::from(DEFAULT_FAUCET_POLICY_USDC_MICRO),
            };
            let evm_target = match params.evm_usdc {
                Some(amount) => amount.parse::<U256>().map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "evm_usdc must parse as uint256".into(),
                    data: None,
                })?,
                None => U256::from(DEFAULT_FAUCET_EVM_USDC_MICRO),
            };
            if policy_target > U256::from(MAX_FAUCET_POLICY_USDC_MICRO) {
                return Err(map_rpc_error_to_json_rpc_error(RpcError::InvalidRequest(
                    "policy_usdc exceeds faucet cap",
                )));
            }
            if evm_target > U256::from(MAX_FAUCET_EVM_USDC_MICRO) {
                return Err(map_rpc_error_to_json_rpc_error(RpcError::InvalidRequest(
                    "evm_usdc exceeds faucet cap",
                )));
            }
            self.faucet_limiter
                .check_and_record(&faucet_client_id(headers), &encode_hex(&address))
                .map_err(|message| JsonRpcErrorObject {
                    code: -32011,
                    message: "faucet rate limit".into(),
                    data: Some(json!({ "reason": message })),
                })?;
            let submitted_at_ms =
                current_unix_ms().max(self.engine.head_snapshot().head_timestamp_ms);
            self.with_durable_engine_write(submitted_at_ms, |engine| {
                engine
                    .submit_faucet_grant(address, policy_target, evm_target, submitted_at_ms)
                    .map_err(|reason| JsonRpcErrorObject {
                        code: -32000,
                        message: reason.into(),
                        data: Some(json!({ "reason": reason })),
                    })
            })?;
            let wait_timeout_ms = params
                .wait_timeout_ms
                .unwrap_or(DELEGATION_ACTION_WAIT_MAX_MS);
            let response =
                self.wait_for_faucet_targets(address, policy_target, evm_target, wait_timeout_ms)?;
            return Ok(faucet_fund_account_response_to_json(&response));
        }

        let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "engine lock poisoned".into(),
            data: None,
        })?;
        let mut service = NodeRpcService::new(&mut engine);

        match request.method.as_str() {
            "eth_chainId" => Ok(Value::String(encode_hex_quantity_u64(
                service.eth_chain_id(),
            ))),
            "eth_blockNumber" => Ok(Value::String(encode_hex_quantity_u64(
                service.eth_block_number(),
            ))),
            "eth_getCode" => {
                let values = match request.params {
                    Value::Array(values) => values,
                    Value::Null => Vec::new(),
                    value => vec![value],
                };
                let address_hex =
                    values
                        .first()
                        .and_then(Value::as_str)
                        .ok_or_else(|| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                let address =
                    decode_hex_array::<20>(address_hex).map_err(|message| JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    })?;
                Ok(Value::String(encode_hex(&service.eth_get_code(address))))
            }
            "eth_getBalance" => {
                let values = match request.params {
                    Value::Array(values) => values,
                    Value::Null => Vec::new(),
                    value => vec![value],
                };
                let address_hex =
                    values
                        .first()
                        .and_then(Value::as_str)
                        .ok_or_else(|| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                let address =
                    decode_hex_array::<20>(address_hex).map_err(|message| JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    })?;
                Ok(Value::String(encode_hex_quantity_u256(
                    service.eth_get_balance(address),
                )))
            }
            "eth_getTransactionCount" => {
                let values = match request.params {
                    Value::Array(values) => values,
                    Value::Null => Vec::new(),
                    value => vec![value],
                };
                let address_hex =
                    values
                        .first()
                        .and_then(Value::as_str)
                        .ok_or_else(|| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid params".into(),
                            data: None,
                        })?;
                let address =
                    decode_hex_array::<20>(address_hex).map_err(|message| JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    })?;
                Ok(Value::String(encode_hex_quantity_u256(
                    service.eth_get_transaction_count(address),
                )))
            }
            "eth_call" => {
                let call = parse_eth_call_request(request.params)?;
                service
                    .eth_call(&call)
                    .map(|response| Value::String(encode_hex(&response.returned)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "eth_estimateGas" => {
                let call = parse_eth_call_request(request.params)?;
                service
                    .eth_estimate_gas(&call)
                    .map(|response| Value::String(encode_hex_quantity_u64(response.gas_used)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "eth_gasPrice" => Ok(Value::String(encode_hex_quantity_u256(
                service.eth_gas_price(),
            ))),
            "eth_sendRawTransaction" => {
                let raw_tx = parse_single_hex_bytes_param(request.params)?;
                service
                    .eth_send_raw_transaction(raw_tx)
                    .map(|tx_hash| Value::String(encode_hex(&tx_hash)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "eth_getTransactionReceipt" => {
                let tx_hash = parse_single_tx_hash_param(request.params)?;
                match service
                    .eth_get_transaction_receipt(tx_hash)
                    .map_err(map_rpc_error_to_json_rpc_error)?
                {
                    Some((receipt, raw_tx)) => eth_transaction_receipt_to_json(&receipt, &raw_tx),
                    None => Ok(Value::Null),
                }
            }
            "eth_getTransactionByHash" => {
                let tx_hash = parse_single_tx_hash_param(request.params)?;
                match service
                    .eth_get_transaction_by_hash(tx_hash)
                    .map_err(map_rpc_error_to_json_rpc_error)?
                {
                    Some((receipt, raw_tx)) => eth_transaction_to_json(&receipt, &raw_tx),
                    None => Ok(Value::Null),
                }
            }
            "kombat_chainInfo" => Ok(chain_info_to_json(&service.chain_info())),
            "kombat_getConsensusLogs" => {
                let params: GetConsensusLogsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(consensus_logs_to_json(
                    service.consensus_logs(params.limit.unwrap_or(20).min(1_000)),
                ))
            }
            "kombat_cancelOrder" => {
                let params: CancelOrderParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(cancel_order_response_to_json(&service.cancel_order(
                    params.market_id,
                    params.account_id,
                    params.client_order_id,
                    params.timestamp_ms,
                )))
            }
            "kombat_getAccount" => {
                let params: GetAccountParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                service
                    .get_account(params.account_id)
                    .map(|response| account_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_getAccountByEvmAddress" => {
                let params: GetAccountByEvmAddressParams = serde_json::from_value(request.params)
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
                service
                    .get_account_by_evm_address(address)
                    .map(|response| {
                        response
                            .as_ref()
                            .map(account_response_to_json)
                            .unwrap_or(Value::Null)
                    })
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_getAgentIdentity" => {
                let params: GetAgentIdentityParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(agent_identity_response_to_json(
                    &service.get_agent_identity(params.account),
                ))
            }
            "kombat_listAttestations" => {
                let params: ListAttestationsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let kind =
                    match params.kind.as_deref() {
                        None => None,
                        Some(value) => Some(parse_attestation_kind(value).ok_or_else(|| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid attestation kind".into(),
                                data: None,
                            }
                        })?),
                    };
                let limit = match params.limit {
                    None => 100usize,
                    Some(0) => {
                        return Err(JsonRpcErrorObject {
                            code: -32602,
                            message: "limit must be between 1 and 1000".into(),
                            data: None,
                        });
                    }
                    Some(value) => value.min(1_000),
                };
                Ok(attestation_list_response_to_json(
                    &service.list_attestations(
                        params.account,
                        kind,
                        params.include_revoked,
                        params.include_expired,
                        limit,
                        params.cursor.as_deref(),
                    ),
                ))
            }
            "kombat_getCompositionAccess" => {
                let params: GetCompositionAccessParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(composition_access_response_to_json(
                    &service.get_composition_access(params.account),
                ))
            }
            "kombat_getReputationMultiplier" => {
                let params: GetReputationMultiplierParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(reputation_multiplier_response_to_json(
                    &service.get_reputation_multiplier(params.account),
                ))
            }
            "kombat_getIdentityActionNonce" => {
                let params: GetIdentityActionNonceParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                Ok(json!({
                    "account": params.account,
                    "nonce": service.get_identity_action_nonce(params.account),
                }))
            }
            "kombat_getIdentityAttestorNonce" => {
                let params: GetIdentityAttestorNonceParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!({
                    "account": params.account,
                    "nonce": service.get_identity_attestor_nonce(params.account),
                }))
            }
            "kombat_submitIdentityAction" => {
                let params: SubmitIdentityActionParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .submit_identity_action(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(agent_identity_response_to_json(&response))
            }
            "kombat_submitTeeAttestation" => {
                let params: SubmitTeeAttestationParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .submit_tee_attestation(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(agent_identity_response_to_json(&response))
            }
            "kombat_listVendorRoots" => {
                let params: ListVendorRootsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let vendor = match params.vendor.as_deref() {
                    None => None,
                    Some(value) => {
                        Some(parse_tee_vendor(value).ok_or_else(|| JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid tee vendor".into(),
                            data: None,
                        })?)
                    }
                };
                Ok(vendor_roots_response_to_json(
                    &service.list_vendor_roots(vendor, params.include_inactive),
                ))
            }
            "kombat_getVendorCapProfile" => {
                let params: GetVendorCapProfileParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let vendor =
                    parse_tee_vendor(&params.vendor).ok_or_else(|| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid tee vendor".into(),
                        data: None,
                    })?;
                Ok(vendor_cap_profile_response_to_json(
                    &service.get_vendor_cap_profile(vendor),
                ))
            }
            "kombat_getWalletBalances" => {
                let params: GetWalletBalancesParams = serde_json::from_value(request.params)
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
                Ok(wallet_balances_response_to_json(
                    &service.get_wallet_balances(address),
                ))
            }
            "kombat_getState" => {
                let params: StateParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let key =
                    decode_hex_bytes(&params.key_hex).map_err(|message| JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    })?;
                Ok(state_query_to_json(&service.get_state(&key)))
            }
            "kombat_getStateRootAtHeight" => {
                let params: StateRootAtHeightParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                service
                    .get_state_root_at_height(params.height)
                    .map(|response| state_root_at_height_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_getPolicy" => {
                let params: GetPolicyParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                service
                    .get_policy(params.policy_id)
                    .map(|response| policy_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_getIntentStatus" => {
                let params: GetIntentStatusParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let intent_id = decode_hex_array(&params.intent_id_hex).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
                Ok(intent_status_to_json(&service.get_intent_status(intent_id)))
            }
            "kombat_getOrderBook" => {
                let params: GetOrderBookParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(order_book_response_to_json(&service.get_order_book(
                    params.market_id,
                    params.depth.unwrap_or(20).min(1_000),
                )))
            }
            "kombat_getOrderStatus" => {
                let params: GetOrderStatusParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(order_status_response_to_json(&service.get_order_status(
                    params.market_id,
                    params.account_id,
                    params.client_order_id,
                )))
            }
            "kombat_getTradeHistory" => {
                let params: GetTradeHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_trade_history(
                        params.market_id,
                        params.account_id,
                        params.client_order_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(trade_fill_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getCandles" => {
                let params: GetCandlesParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_candles(
                        params.market_id,
                        &params.interval,
                        params.from_ms,
                        params.to_ms,
                        params.limit.unwrap_or(500).min(5_000),
                    )
                    .map_err(map_rpc_error_to_json_rpc_error)?
                    .iter()
                    .map(candle_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getMarketSnapshot" => {
                let params: GetMarketSnapshotParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(market_snapshot_response_to_json(
                    &service
                        .get_market_snapshot(params.market_id, params.depth.unwrap_or(20).min(200)),
                ))
            }
            "kombat_getAccountTradingState" => {
                let params: GetAccountTradingStateParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                Ok(account_trading_state_response_to_json(
                    &service.get_account_trading_state(
                        params.account_id,
                        params.market_id,
                        params.depth.unwrap_or(100).min(1_000),
                        params.fill_limit.unwrap_or(100).min(1_000),
                    ),
                ))
            }
            "kombat_previewIntent" => {
                let params: PreviewIntentParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let order = order_request_from_json(&params.order).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
                service
                    .preview_intent(order, params.replace_client_order_id, params.timestamp_ms)
                    .map(|response| preview_intent_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_getAccountPnlWindow" => {
                let params: GetAccountPnlWindowParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(account_pnl_window_to_json(
                    &service
                        .get_account_pnl_window(
                            params.account_id,
                            params.market_id,
                            params.from_ms,
                            params.to_ms,
                        )
                        .map_err(map_rpc_error_to_json_rpc_error)?,
                ))
            }
            "kombat_getAccountEquityHistory" => {
                let params: GetAccountEquityHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(account_equity_history_to_json(
                    &service
                        .get_account_equity_history(
                            params.account_id,
                            params.from_ms,
                            params.to_ms,
                            params.limit,
                        )
                        .map_err(map_rpc_error_to_json_rpc_error)?,
                ))
            }
            "kombat_getMandateConformanceHistory" => {
                let params: GetMandateConformanceHistoryParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let mandate_id = decode_hex_array::<32>(&params.mandate_id_hex).map_err(|_| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid mandate_id_hex".into(),
                        data: None,
                    }
                })?;
                Ok(mandate_conformance_history_to_json(
                    &service
                        .get_mandate_conformance_history(mandate_id, params.limit)
                        .map_err(map_rpc_error_to_json_rpc_error)?,
                ))
            }
            "kombat_getLiquidationHistory" => {
                let params: GetLiquidationHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_liquidation_history(
                        params.market_id,
                        params.account_id,
                        params.liquidator_account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(liquidation_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getLiquidationEscalationHistory" => {
                let params: GetLiquidationEscalationHistoryParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_liquidation_escalation_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(liquidation_escalation_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getOracleHistory" => {
                let params: GetOracleHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let topic = match params.topic.as_deref() {
                    None => None,
                    Some(topic) => {
                        let parsed = parse_runtime_event_topic(topic).map_err(|message| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: message.into(),
                                data: None,
                            }
                        })?;
                        match parsed {
                            RuntimeEventTopic::OracleConfigUpdated
                            | RuntimeEventTopic::MarkPriceUpdated
                            | RuntimeEventTopic::IndexPriceUpdated => Some(parsed),
                            _ => {
                                return Err(JsonRpcErrorObject {
                                    code: -32602,
                                    message: "topic must be oracle_config_updated, mark_price_updated, or index_price_updated".into(),
                                    data: None,
                                })
                            }
                        }
                    }
                };
                Ok(json!(service
                    .get_oracle_history(
                        params.market_id,
                        topic,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(oracle_history_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getMarketTradingHistory" => {
                let params: GetMarketTradingHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_market_trading_history(
                        params.market_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(market_trading_state_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getInsuranceFundHistory" => {
                let params: GetInsuranceFundHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_insurance_fund_history(
                        params.market_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(insurance_fund_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getBadDebtHistory" => {
                let params: GetBadDebtHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_bad_debt_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(bad_debt_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getSolvencyHistory" => {
                let params: GetSolvencyHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_solvency_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(solvency_breach_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getRiskStateHistory" => {
                let params: GetRiskStateHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_risk_state_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(risk_state_history_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getLiquidationCandidateHistory" => {
                let params: GetLiquidationCandidateHistoryParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_liquidation_candidate_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(liquidation_candidate_history_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getFundingHistory" => {
                let params: GetFundingHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_funding_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(funding_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getFundingState" => {
                let params: GetFundingStateParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(funding_state_response_to_json(
                    &service.get_funding_state(params.market_id),
                ))
            }
            "kombat_getInsuranceFund" => {
                let params: GetInsuranceFundParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(insurance_fund_response_to_json(
                    &service.get_insurance_fund(params.market_id),
                ))
            }
            "kombat_getFeeHistory" => {
                let params: GetFeeHistoryParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_fee_history(
                        params.market_id,
                        params.account_id,
                        params.client_order_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(fee_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getRevenueSplitHistory" => {
                let params: GetRevenueSplitHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                Ok(json!(service
                    .get_revenue_split_history(
                        params.market_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(revenue_split_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getPosition" => {
                let params: GetPositionParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(position_response_to_json(
                    &service.get_position(params.market_id, params.account_id),
                ))
            }
            "kombat_getAccountHistory" => {
                let params: GetAccountHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_account_history(
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(account_history_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getPositionHistory" => {
                let params: GetPositionHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_position_history(
                        params.market_id,
                        params.account_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(position_history_record_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getRiskSnapshot" => {
                let params: GetRiskSnapshotParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(risk_snapshot_response_to_json(
                    &service.get_risk_snapshot(params.market_id, params.account_id),
                ))
            }
            "kombat_getSuggestedLiquidation" => {
                let params: GetSuggestedLiquidationParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(
                    match service.get_suggested_liquidation(params.market_id, params.account_id) {
                        Some(response) => suggested_liquidation_plan_response_to_json(&response),
                        None => Value::Null,
                    },
                )
            }
            "kombat_listSuggestedLiquidations" => {
                let params: ListSuggestedLiquidationsParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_suggested_liquidations(
                        params.market_id,
                        params.account_id,
                        params.outcome,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(suggested_liquidation_plan_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getMarketState" => {
                let params: GetMarketStateParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(market_state_response_to_json(
                    &service.get_market_state(params.market_id),
                ))
            }
            "kombat_getMarket" => {
                let params: GetMarketParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(service
                    .get_perpetual_market(params.market_id)
                    .as_ref()
                    .map(perpetual_market_response_to_json)
                    .unwrap_or(Value::Null))
            }
            "kombat_listPerpetualMarkets" => {
                let params: ListPerpetualMarketsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let status = match params.status.as_deref() {
                    Some(value) => {
                        Some(
                            parse_market_status(value).ok_or_else(|| JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid market status".into(),
                                data: None,
                            })?,
                        )
                    }
                    None => None,
                };
                Ok(json!(service
                    .list_perpetual_markets(status, params.limit.unwrap_or(100).min(1_000))
                    .iter()
                    .map(perpetual_market_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getLiquidationPressureSummary" => {
                let _params: GetLiquidationPressureSummaryParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(liquidation_pressure_summary_to_json(
                    &service.get_liquidation_pressure_summary(_params.market_id, _params.outcome),
                ))
            }
            "kombat_listMarketStates" => {
                let params: ListMarketStatesParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_market_states(
                        params.stressed_only.unwrap_or(false),
                        params.imminent_freeze_only.unwrap_or(false),
                        params.outcome,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(market_state_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_setIsolatedCollateral" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                if !local_bridge_mutations_enabled() {
                    return Err(local_bridge_mutations_disabled_error());
                }
                let params: SetIsolatedCollateralParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let collateral =
                    params
                        .collateral
                        .parse::<i128>()
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "collateral must parse as i128".into(),
                            data: None,
                        })?;
                Ok(risk_snapshot_response_to_json(
                    &service.set_isolated_collateral(
                        params.market_id,
                        params.account_id,
                        collateral,
                        params.timestamp_ms,
                    ),
                ))
            }
            "kombat_setMarketRisk" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetMarketRiskParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let current_market = service.get_market_state(params.market_id);
                if params.maintenance_margin_bps > 10_000 {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "maintenance_margin_bps must be <= 10000".into(),
                        data: None,
                    });
                }
                if params.liquidation_penalty_bps.unwrap_or(0) > 10_000 {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "liquidation_penalty_bps must be <= 10000".into(),
                        data: None,
                    });
                }
                if params.liquidator_fee_bps.unwrap_or(0) > 10_000 {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "liquidator_fee_bps must be <= 10000".into(),
                        data: None,
                    });
                }
                let effective_penalty_bps = params
                    .liquidation_penalty_bps
                    .unwrap_or(current_market.liquidation_penalty_bps);
                let effective_reward_bps = params
                    .liquidator_fee_bps
                    .unwrap_or(current_market.liquidator_fee_bps);
                if effective_reward_bps > effective_penalty_bps {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "liquidator_fee_bps must be <= liquidation_penalty_bps".into(),
                        data: None,
                    });
                }
                Ok(market_state_response_to_json(&service.set_market_risk(
                    params.market_id,
                    params.maintenance_margin_bps,
                    params.liquidation_penalty_bps,
                    params.liquidator_fee_bps,
                    params.timestamp_ms,
                )))
            }
            "kombat_setFundingIndex" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetFundingIndexParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let funding_index =
                    params
                        .funding_index
                        .parse::<i128>()
                        .map_err(|_| JsonRpcErrorObject {
                            code: -32602,
                            message: "funding_index must parse as i128".into(),
                            data: None,
                        })?;
                Ok(market_state_response_to_json(&service.set_funding_index(
                    params.market_id,
                    funding_index,
                    params.timestamp_ms,
                )))
            }
            "kombat_setIndexPrice" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetIndexPriceParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                if params.index_price_ticks == 0 {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "index_price_ticks must be non-zero".into(),
                        data: None,
                    });
                }
                Ok(market_state_response_to_json(&service.set_index_price(
                    params.market_id,
                    params.index_price_ticks,
                    params.timestamp_ms,
                )))
            }
            "kombat_setMarkPrice" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetMarkPriceParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                if params.mark_price_ticks == 0 {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "mark_price_ticks must be non-zero".into(),
                        data: None,
                    });
                }
                Ok(market_state_response_to_json(&service.set_mark_price(
                    params.market_id,
                    params.mark_price_ticks,
                    params.timestamp_ms,
                )))
            }
            "kombat_submitOraclePrices" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SubmitOraclePricesParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let samples = params
                    .samples
                    .into_iter()
                    .map(oracle_feed_sample_from_json)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|message| JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    })?;
                Ok(oracle_feed_aggregation_to_json(
                    &service
                        .submit_oracle_prices(params.market_id, samples, params.timestamp_ms)
                        .map_err(map_rpc_error_to_json_rpc_error)?,
                ))
            }
            "kombat_advanceFundingInterval" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: AdvanceFundingIntervalParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                match service.advance_funding_interval(params.market_id, params.timestamp_ms) {
                    Ok(response) => Ok(market_state_response_to_json(&response)),
                    Err(RpcError::Submission(error)) => Err(error),
                    Err(RpcError::InvalidRequest(message)) => Err(JsonRpcErrorObject {
                        code: -32602,
                        message: message.into(),
                        data: None,
                    }),
                    Err(RpcError::UnknownAccount(account_id)) => Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!("unknown account {account_id}"),
                        data: None,
                    }),
                    Err(RpcError::UnknownPolicy(policy_id)) => Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!("unknown policy {policy_id}"),
                        data: None,
                    }),
                    Err(RpcError::HeadNotSet) => Err(JsonRpcErrorObject {
                        code: -32000,
                        message: "head not set".into(),
                        data: None,
                    }),
                    Err(RpcError::Node(message)) => Err(JsonRpcErrorObject {
                        code: -32000,
                        message,
                        data: None,
                    }),
                }
            }
            "kombat_setMarketFees" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetMarketFeesParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                if params.maker_fee_bps > 10_000
                    || params.maker_rebate_bps > 10_000
                    || params.taker_fee_bps > 10_000
                {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message:
                            "maker_fee_bps, maker_rebate_bps, and taker_fee_bps must be <= 10000"
                                .into(),
                        data: None,
                    });
                }
                if u64::from(params.maker_rebate_bps)
                    > u64::from(params.maker_fee_bps) + u64::from(params.taker_fee_bps)
                {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "maker_rebate_bps must be <= maker_fee_bps + taker_fee_bps".into(),
                        data: None,
                    });
                }
                Ok(market_state_response_to_json(&service.set_market_fees(
                    params.market_id,
                    params.maker_fee_bps,
                    params.maker_rebate_bps,
                    params.taker_fee_bps,
                )))
            }
            "kombat_setMarketTradingState" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetMarketTradingStateParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(market_state_response_to_json(
                    &service.set_market_trading_state(
                        params.market_id,
                        params.paused,
                        params.timestamp_ms,
                    ),
                ))
            }
            "kombat_setMarketOracleConfig" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetMarketOracleConfigParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                if params.max_staleness_ms == 0 {
                    return Err(JsonRpcErrorObject {
                        code: -32602,
                        message: "max_staleness_ms must be non-zero".into(),
                        data: None,
                    });
                }
                Ok(market_state_response_to_json(
                    &service.set_market_oracle_config(
                        params.market_id,
                        params.max_staleness_ms,
                        params.timestamp_ms,
                    ),
                ))
            }
            "kombat_listPerpetualMarket" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: ListPerpetualMarketParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .list_perpetual_market(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(perpetual_market_response_to_json(&response))
            }
            "kombat_setPerpetualMarketStatus" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetPerpetualMarketStatusParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let status =
                    parse_market_status(&params.status).ok_or_else(|| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid market status".into(),
                        data: None,
                    })?;
                let response = service
                    .set_perpetual_market_status(params.market_id, status, params.timestamp_ms)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(perpetual_market_response_to_json(&response))
            }
            "kombat_openDelegation" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: OpenDelegationParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .open_delegation(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_response_to_json(&response))
            }
            "kombat_depositToDelegation" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: DepositToDelegationParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .deposit_to_delegation(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_deposit_response_to_json(&response))
            }
            "kombat_submitDepositToDelegation" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SubmitDepositToDelegationParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .submit_deposit_to_delegation(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_action_submission_response_to_json(&response))
            }
            "kombat_requestDelegationRedemption" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: RequestDelegationRedemptionParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .request_delegation_redemption(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_redemption_request_response_to_json(&response))
            }
            "kombat_finalizeDelegationRedemption" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: FinalizeDelegationRedemptionParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .finalize_delegation_redemption(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_redemption_finalization_response_to_json(
                    &response,
                ))
            }
            "kombat_cancelDelegationRedemption" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: CancelDelegationRedemptionParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .cancel_delegation_redemption(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_redemption_cancellation_response_to_json(
                    &response,
                ))
            }
            "kombat_closeDelegationToDeposits" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: DelegationLifecycleParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .close_delegation_to_deposits(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_response_to_json(&response))
            }
            "kombat_beginDelegationWinding" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: DelegationLifecycleParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .begin_delegation_winding(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_response_to_json(&response))
            }
            "kombat_closeDelegation" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: DelegationLifecycleParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .close_delegation(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_response_to_json(&response))
            }
            "kombat_getDelegationActionStatus" => {
                let params: GetDelegationActionStatusParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .get_delegation_action_status(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(delegation_action_status_response_to_json(&response))
            }
            "kombat_getMarketMakerProgramActionStatus" => {
                let params: GetMarketMakerProgramActionStatusParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .get_market_maker_program_action_status(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(market_maker_program_action_status_response_to_json(
                    &response,
                ))
            }
            "kombat_getMarketMakerProgram" => {
                let params: GetMarketMakerProgramParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .get_market_maker_program(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(response
                    .as_ref()
                    .map(market_maker_program_to_json)
                    .unwrap_or(Value::Null))
            }
            "kombat_getMarketMakerEnrollment" => {
                let params: GetMarketMakerEnrollmentParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .get_market_maker_enrollment(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(response
                    .as_ref()
                    .map(market_maker_enrollment_to_json)
                    .unwrap_or(Value::Null))
            }
            "kombat_createMandate" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: CreateMandateParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .create_mandate(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(mandate_response_to_json(&response))
            }
            "kombat_submitMandateAction" => {
                let params: SubmitMandateActionParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .submit_mandate_action(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(mandate_response_to_json(&response))
            }
            "kombat_submitEvmMandateAction" => {
                let params: SubmitEvmMandateActionParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                let response = service
                    .submit_evm_mandate_action(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(mandate_response_to_json(&response))
            }
            "kombat_setAccountOperator" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SetAccountOperatorParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .set_account_operator(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(account_operator_response_to_json(&response))
            }
            "kombat_getAccountOperator" => {
                let params: GetAccountOperatorParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(account_operator_response_to_json(
                    &service.get_account_operator(params.account_id),
                ))
            }
            "kombat_getMandateActionNonce" => {
                let params: GetMandateActionNonceParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let owner =
                    decode_hex_array::<32>(&params.owner_hex).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid owner_hex".into(),
                        data: None,
                    })?;
                Ok(
                    json!({ "owner_hex": encode_hex(&owner), "nonce": service.get_mandate_action_nonce(&owner) }),
                )
            }
            "kombat_suspendMandate" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: MandateLifecycleParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .suspend_mandate(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(mandate_response_to_json(&response))
            }
            "kombat_resumeMandate" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: MandateLifecycleParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .resume_mandate(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(mandate_response_to_json(&response))
            }
            "kombat_revokeMandate" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: RevokeMandateParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .revoke_mandate(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(mandate_response_to_json(&response))
            }
            "kombat_expireDueMandates" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: ExpireDueMandatesParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .expire_due_mandates(params.timestamp_ms)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(json!(response
                    .iter()
                    .map(mandate_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getMandate" => {
                let params: GetMandateParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let mandate_id = decode_hex_array::<32>(&params.mandate_id_hex).map_err(|_| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid mandate_id_hex".into(),
                        data: None,
                    }
                })?;
                Ok(service
                    .get_mandate(&mandate_id)
                    .as_ref()
                    .map(mandate_response_to_json)
                    .unwrap_or(Value::Null))
            }
            "kombat_listMandates" => {
                let params: ListMandatesParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let state = match params.state.as_deref() {
                    Some(value) => {
                        Some(
                            parse_mandate_state(value).ok_or_else(|| JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid mandate state".into(),
                                data: None,
                            })?,
                        )
                    }
                    None => None,
                };
                let owner = params
                    .owner_hex
                    .as_deref()
                    .map(decode_hex_array::<32>)
                    .transpose()
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid owner_hex".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_mandates(
                        owner,
                        params.agent,
                        state,
                        params.limit.unwrap_or(100).min(1_000)
                    )
                    .iter()
                    .map(mandate_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getDelegation" => {
                let params: GetDelegationParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let delegation_id =
                    decode_hex_array::<32>(&params.delegation_id_hex).map_err(|_| {
                        JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid delegation_id_hex".into(),
                            data: None,
                        }
                    })?;
                Ok(service
                    .get_delegation(&delegation_id)
                    .as_ref()
                    .map(delegation_response_to_json)
                    .unwrap_or(Value::Null))
            }
            "kombat_listDelegations" => {
                let params: ListDelegationsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let status =
                    match params.status.as_deref() {
                        Some(value) => Some(parse_delegation_status(value).ok_or_else(|| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid delegation status".into(),
                                data: None,
                            }
                        })?),
                        None => None,
                    };
                Ok(json!(service
                    .list_delegations(params.agent, status, params.limit.unwrap_or(100).min(1_000))
                    .iter()
                    .map(delegation_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_listRedemptionRequests" => {
                let params: ListRedemptionRequestsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                let vault_id = match params.vault_id_hex.as_deref() {
                    None => None,
                    Some(value) => {
                        Some(
                            decode_hex_array::<16>(value).map_err(|_| JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid vault_id_hex".into(),
                                data: None,
                            })?,
                        )
                    }
                };
                let status = match params.status.as_deref() {
                    None => None,
                    Some(value) => {
                        Some(parse_redemption_request_status(value).ok_or_else(|| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid redemption request status".into(),
                                data: None,
                            }
                        })?)
                    }
                };
                let limit = match params.limit {
                    None => 100usize,
                    Some(0) => {
                        return Err(JsonRpcErrorObject {
                            code: -32602,
                            message: "limit must be between 1 and 1000".into(),
                            data: None,
                        });
                    }
                    Some(value) => value.min(1_000),
                };
                let response = service
                    .list_redemption_requests(
                        params.principal,
                        vault_id,
                        status,
                        params.cursor.as_deref(),
                        limit,
                    )
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(list_redemption_requests_response_to_json(&response))
            }
            "kombat_listVaultPositions" => {
                let params: ListVaultPositionsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let kind =
                    match params.kind.as_deref() {
                        None => None,
                        Some(value) => Some(vaults::VaultKind::parse(value).ok_or_else(|| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid vault kind".into(),
                                data: None,
                            }
                        })?),
                    };
                let status = match params.status.as_deref() {
                    None => None,
                    Some(value) => Some(vaults::VaultStatus::parse(value).ok_or_else(|| {
                        JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid vault status".into(),
                            data: None,
                        }
                    })?),
                };
                let limit = match params.limit {
                    None => 100usize,
                    Some(0) => {
                        return Err(JsonRpcErrorObject {
                            code: -32602,
                            message: "limit must be between 1 and 1000".into(),
                            data: None,
                        });
                    }
                    Some(value) => value.min(1_000),
                };
                let response = service
                    .list_vault_positions(
                        params.principal,
                        kind,
                        status,
                        params.cursor.as_deref(),
                        limit,
                    )
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(list_vault_positions_response_to_json(&response))
            }
            "kombat_listRealCapitalListings" => {
                let params: ListRealCapitalListingsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let mode =
                    match params.mode.as_deref() {
                        None => None,
                        Some(value) => Some(parse_real_capital_mode(value).ok_or_else(|| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid real-capital mode".into(),
                                data: None,
                            }
                        })?),
                    };
                let limit = match params.limit {
                    None => 100usize,
                    Some(0) => {
                        return Err(JsonRpcErrorObject {
                            code: -32602,
                            message: "limit must be between 1 and 1000".into(),
                            data: None,
                        });
                    }
                    Some(value) => value.min(1_000),
                };
                let response = service
                    .list_real_capital_listings(
                        params.agent,
                        mode,
                        params.include_unlisted,
                        params.include_disabled,
                        params.cursor.as_deref(),
                        limit,
                    )
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(list_real_capital_listings_response_to_json(&response))
            }
            "kombat_getRealCapitalListing" => {
                let params: GetRealCapitalListingParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let vault_id = decode_hex_array::<16>(&params.vault_id_hex).map_err(|_| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid vault_id_hex".into(),
                        data: None,
                    }
                })?;
                let response = service
                    .get_real_capital_listing(&vault_id)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(get_real_capital_listing_response_to_json(&response))
            }
            "kombat_getRealCapitalGovernanceActionStatus" => {
                // Read surface: no operator authorization required. Submit
                // authorization is enforced on kombat_submitRealCapitalGovernanceAction.
                let params: GetRealCapitalGovernanceActionStatusParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service
                    .get_real_capital_governance_action_status(params)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(real_capital_governance_action_status_response_to_json(
                    &response,
                ))
            }
            "kombat_listVaults" => {
                let params: ListVaultsParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let kind =
                    match params.kind.as_deref() {
                        None => None,
                        Some(value) => Some(vaults::VaultKind::parse(value).ok_or_else(|| {
                            JsonRpcErrorObject {
                                code: -32602,
                                message: "invalid vault kind".into(),
                                data: None,
                            }
                        })?),
                    };
                let status = match params.status.as_deref() {
                    None => None,
                    Some(value) => Some(vaults::VaultStatus::parse(value).ok_or_else(|| {
                        JsonRpcErrorObject {
                            code: -32602,
                            message: "invalid vault status".into(),
                            data: None,
                        }
                    })?),
                };
                let limit = match params.limit {
                    None => 100usize,
                    Some(0) => {
                        return Err(JsonRpcErrorObject {
                            code: -32602,
                            message: "limit must be between 1 and 1000".into(),
                            data: None,
                        });
                    }
                    Some(value) => value.min(1_000),
                };
                let response = service
                    .list_vaults(
                        kind,
                        status,
                        params.operator,
                        params.currencies,
                        params.include_unlisted,
                        params.cursor.as_deref(),
                        limit,
                    )
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(json!({
                    "items": response
                        .items
                        .iter()
                        .map(vaults::vault_record_to_json)
                        .collect::<Vec<_>>(),
                    "next_cursor": response.next_cursor,
                }))
            }
            "kombat_getVault" => {
                let params: GetVaultParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let vault_id = decode_hex_array::<16>(&params.vault_id_hex).map_err(|_| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid vault_id_hex".into(),
                        data: None,
                    }
                })?;
                let response = service
                    .get_vault(&vault_id)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(json!({
                    "record": response.record.as_ref().map(vaults::vault_record_to_json),
                    "present": response.present,
                }))
            }
            "kombat_setPolicyUsdcBalance" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                if !local_bridge_mutations_enabled() {
                    return Err(local_bridge_mutations_disabled_error());
                }
                let params: SetPolicyUsdcBalanceParams = serde_json::from_value(request.params)
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
                let amount = params
                    .amount
                    .parse::<U256>()
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "amount must parse as uint256".into(),
                        data: None,
                    })?;
                let response = service
                    .set_policy_usdc_balance(address, amount)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(policy_usdc_balance_response_to_json(&response))
            }
            "kombat_setEvmUsdcMinter" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                if !local_bridge_mutations_enabled() {
                    return Err(local_bridge_mutations_disabled_error());
                }
                let params: SetEvmUsdcMinterParams = serde_json::from_value(request.params)
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
                let response = service
                    .set_evm_usdc_minter(address, params.enabled)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                Ok(evm_usdc_minter_role_response_to_json(&response))
            }
            "kombat_listRiskSnapshots" => {
                let params: ListRiskSnapshotsParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_risk_snapshots(
                        params.account_id,
                        RiskSnapshotListOptions {
                            market_id: params.market_id,
                            stressed_only: params.stressed_only.unwrap_or(false),
                            freeze_imminent_only: params.freeze_imminent_only.unwrap_or(false),
                            outcome: params.outcome,
                            include_flat: params.include_flat.unwrap_or(false),
                            limit: params.limit.unwrap_or(100).min(1_000),
                        },
                    )
                    .iter()
                    .map(risk_snapshot_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getAccountRiskSummary" => {
                let params: GetAccountRiskSummaryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(account_risk_summary_to_json(
                    &service.get_account_risk_summary(
                        params.account_id,
                        params.market_id,
                        params.outcome,
                        params.include_flat.unwrap_or(false),
                        params.limit.unwrap_or(100).min(1_000),
                    ),
                ))
            }
            "kombat_listAccountRiskSummaries" => {
                let params: ListAccountRiskSummariesParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_account_risk_summaries(
                        params.market_id,
                        params.imminent_freeze_only.unwrap_or(false),
                        params.outcome,
                        params.include_flat.unwrap_or(false),
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(account_risk_summary_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_listLiquidationCandidates" => {
                let params: ListLiquidationCandidatesParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_liquidation_candidates(
                        params.market_id,
                        params.account_id,
                        params.freeze_imminent_only.unwrap_or(false),
                        params.outcome,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(risk_snapshot_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getAccountTradeHistory" => {
                let params: GetAccountTradeHistoryParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
                Ok(json!(service
                    .get_account_trade_history(
                        params.account_id,
                        params.market_id,
                        params.from_seq,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(trade_fill_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_listPositions" => {
                let params: ListPositionsParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .list_positions(
                        params.account_id,
                        params.include_flat.unwrap_or(false),
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(position_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_getOpenOrders" => {
                let params: GetOpenOrdersParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                Ok(json!(service
                    .get_open_orders(
                        params.market_id,
                        params.account_id,
                        params.limit.unwrap_or(100).min(1_000),
                    )
                    .iter()
                    .map(resting_order_response_to_json)
                    .collect::<Vec<_>>()))
            }
            "kombat_listEvents" => {
                let params: ListEventsParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let filter = params
                    .filter
                    .map(event_filter_from_json)
                    .transpose()?
                    .unwrap_or_default();
                let limit = params.limit.unwrap_or(100).min(1_000);
                Ok(event_page_to_json(&service.list_events(
                    params.from_seq,
                    params.before_seq,
                    limit,
                    filter,
                    params.descending.unwrap_or(false),
                )))
            }
            "kombat_saveSnapshot" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SnapshotFileParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                service
                    .save_snapshot_file(params.path)
                    .map(|response| snapshot_file_response_to_json(&response))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_loadSnapshot" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: SnapshotFileParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                if !self.wait_for_local_submit_idle() {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: "timed out waiting for local submit queue to drain".into(),
                        data: None,
                    });
                }
                let response = service
                    .load_snapshot_file(params.path, params.force_reset)
                    .map_err(map_rpc_error_to_json_rpc_error)?;
                drop(service);
                drop(engine);
                self.clear_local_submit_backlog();
                self.reset_peer_transport_session_cache();
                Ok(snapshot_file_response_to_json(&response))
            }
            "kombat_submitOrder" => {
                let params: SubmitOrderParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let order = order_request_from_json(&params.order).map_err(|message| {
                    JsonRpcErrorObject {
                        code: -32602,
                        message,
                        data: None,
                    }
                })?;
                service
                    .submit_order(order, params.timestamp_ms)
                    .map(|response| json!(submit_order_response_to_json(&response)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_liquidatePosition" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: LiquidatePositionParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                service
                    .liquidate_position(
                        params.market_id,
                        params.account_id,
                        params.liquidator_account_id,
                        params.client_order_id,
                        params.requested_lots,
                        params.timestamp_ms,
                    )
                    .map(|response| json!(liquidation_response_to_json(&response)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_runLiquidationPass" => {
                if !operator_request_is_authorized(headers, &self.config) {
                    return Err(JsonRpcErrorObject {
                        code: -32010,
                        message: "operator authorization required".into(),
                        data: None,
                    });
                }
                let params: RunLiquidationPassParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let response = service.run_liquidation_pass(
                    params.liquidator_account_id,
                    params.first_client_order_id,
                    params.max_positions,
                    params.timestamp_ms,
                );
                Ok(json!(liquidation_pass_response_to_json(&response)))
            }
            "kombat_previewLiquidation" => {
                let params: LiquidatePositionParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                service
                    .preview_liquidation(
                        params.market_id,
                        params.account_id,
                        params.liquidator_account_id,
                        params.client_order_id,
                        params.requested_lots,
                        params.timestamp_ms,
                    )
                    .map(|response| json!(liquidation_response_to_json(&response)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_replaceOrder" => {
                let params: ReplaceOrderParams =
                    serde_json::from_value(request.params).map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                let replacement =
                    order_request_from_json(&params.replacement).map_err(|message| {
                        JsonRpcErrorObject {
                            code: -32602,
                            message,
                            data: None,
                        }
                    })?;
                service
                    .replace_order(
                        params.market_id,
                        params.account_id,
                        params.client_order_id,
                        replacement,
                        params.timestamp_ms,
                    )
                    .map(|response| json!(submit_order_response_to_json(&response)))
                    .map_err(map_rpc_error_to_json_rpc_error)
            }
            "kombat_submitOwnerCall" => {
                let params: SubmitOwnerCallParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                submit_owner_call_locally(&mut service, params)
            }
            "kombat_submitEvmOwnerCall" => {
                let params: SubmitEvmOwnerCallParams = serde_json::from_value(request.params)
                    .map_err(|_| JsonRpcErrorObject {
                        code: -32602,
                        message: "invalid params".into(),
                        data: None,
                    })?;
                submit_evm_owner_call_locally(&mut service, params)
            }
            _ => Err(JsonRpcErrorObject {
                code: -32601,
                message: "method not found".into(),
                data: None,
            }),
        }
    }
}
