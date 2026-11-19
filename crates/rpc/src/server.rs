use crate::*;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

impl JsonRpcHttpServer {
    pub fn new(engine: Arc<Mutex<SingleNodeEngine>>) -> Self {
        Self::new_with_config(engine, JsonRpcServerConfig::default())
    }

    pub fn new_with_config(
        engine: Arc<Mutex<SingleNodeEngine>>,
        config: JsonRpcServerConfig,
    ) -> Self {
        let engine = Arc::new(
            EngineHandle::from_legacy_mutex(engine)
                .expect("engine lock should not be poisoned during server init"),
        );
        Self::new_with_engine_handle(engine, config)
    }

    pub fn new_with_engine_handle(engine: Arc<EngineHandle>, config: JsonRpcServerConfig) -> Self {
        let mahi_commit_rule = parse_mahi_commit_rule();
        let x402_config = parse_x402_config_from_env()
            .unwrap_or_else(|error| panic!("invalid x402 config: {error}"));
        let head = engine.head_snapshot();
        let mahi_restart_boundary_round = mahi_restart_boundary_round_from_head(&head);
        let mahi_state = build_mahi_state(
            config.mahi_enabled,
            config.network_bootstrap.as_ref(),
            mahi_commit_rule,
            mahi_restart_boundary_round,
        );
        let mahi_sigverify_pool_enabled =
            parse_env_bool_with_default("KOMBAT_MAHI_SIGVERIFY_POOL", config.mahi_enabled);
        let mahi_inbound_drop_pct = parse_optional_env_u64("KOMBAT_MAHI_DROP_INBOUND_PCT")
            .unwrap_or(0)
            .min(100) as u8;
        let mahi_async_block_broadcast =
            parse_env_bool_with_default(MAHI_ASYNC_BLOCK_BROADCAST_ENV, false);
        let mahi_sigverify_pool = if config.mahi_enabled && mahi_sigverify_pool_enabled {
            Some(MahiSigverifyPool::new(engine.clone()))
        } else {
            None
        };
        let server = Self {
            engine,
            network_control: Arc::new(Mutex::new(())),
            mahi_state: Arc::new(Mutex::new(mahi_state)),
            mahi_sigverify_pool,
            mahi_commit_executor: Arc::new(MahiCommitExecutorControl::default()),
            mahi_missing_fetch_worker_running: Arc::new(AtomicBool::new(false)),
            mahi_inbound_drop_pct,
            mahi_async_block_broadcast,
            local_submit_queue: Arc::new(Mutex::new(LocalSubmitQueue::default())),
            local_submit_worker_running: Arc::new(AtomicBool::new(false)),
            network_http_client: build_network_http_client(),
            mahi_pre_admitted_pool: Arc::new(Mutex::new(MahiPreAdmittedPool::default())),
            pending_intent_relays: Arc::new(Mutex::new(PendingIntentRelayQueue::default())),
            pending_mahi_owner_relays: Arc::new(Mutex::new(PendingMahiOwnerRelayQueues::default())),
            intent_relay_worker_running: Arc::new(AtomicBool::new(false)),
            pending_mahi_block_broadcasts: Arc::new(Mutex::new(
                PendingMahiBlockBroadcastQueue::default(),
            )),
            mahi_block_broadcast_worker_running: Arc::new(AtomicBool::new(false)),
            network_peer_streams: Arc::new(Mutex::new(BTreeMap::new())),
            peer_transport_endpoint_states: Arc::new(Mutex::new(BTreeMap::new())),
            active_peer_transport_sessions: Arc::new(Mutex::new(BTreeMap::new())),
            timeout_message_pool: Arc::new(Mutex::new(TimeoutMessagePool::default())),
            peer_snapshot_repair_state: Arc::new(Mutex::new(PeerSnapshotRepairState::default())),
            rpc_submit_metrics: Arc::new(Mutex::new(RpcSubmitPathMetrics::default())),
            network_round_lock_profile: Arc::new(Mutex::new(
                NetworkRoundLockProfileSnapshot::default(),
            )),
            next_peer_session_id: Arc::new(AtomicU64::new(1)),
            faucet_limiter: FaucetLimiter::from_env(),
            config,
            x402_config,
        };
        server.spawn_mahi_sigverify_result_worker();
        server.spawn_mahi_commit_executor_worker();
        server
    }

    pub fn with_x402_config(mut self, x402_config: Option<X402Config>) -> Self {
        self.x402_config = x402_config;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_faucet_limiter(mut self, faucet_limiter: FaucetLimiter) -> Self {
        self.faucet_limiter = faucet_limiter;
        self
    }

    pub fn engine(&self) -> &Arc<EngineHandle> {
        &self.engine
    }

    pub(super) fn with_durable_engine_write<T, F>(
        &self,
        timestamp_ms: u64,
        f: F,
    ) -> Result<T, JsonRpcErrorObject>
    where
        F: FnOnce(&mut SingleNodeEngine) -> Result<T, JsonRpcErrorObject>,
    {
        self.engine
            .with_durable_write(
                timestamp_ms,
                SyncMode::Sync,
                durable_rpc_write_domains(),
                f,
                |error| Err(durable_rpc_write_error(error)),
            )
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?
    }

    pub(super) fn with_durable_node_rpc_write<T, F>(
        &self,
        timestamp_ms: u64,
        f: F,
    ) -> Result<T, JsonRpcErrorObject>
    where
        F: FnOnce(&mut NodeRpcService<'_>) -> Result<T, JsonRpcErrorObject>,
    {
        self.with_durable_engine_write(timestamp_ms, |engine| {
            let mut service = NodeRpcService::new(engine);
            f(&mut service)
        })
    }

    pub(super) fn create_account_rpc(
        &self,
        params: CreateAccountParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        let owner = decode_hex_array(&params.owner_hex).map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?;
        let code_hash =
            decode_hex_array(&params.code_hash_hex).map_err(|message| JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            })?;
        self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
            create_account_locally(
                service,
                params.account_id,
                owner,
                code_hash,
                params.evm_address_hex.as_deref(),
                params.timestamp_ms,
            )
        })
    }

    pub(super) fn create_evm_account_rpc(
        &self,
        params: CreateEvmAccountParams,
    ) -> Result<Value, JsonRpcErrorObject> {
        self.with_durable_node_rpc_write(params.timestamp_ms, |service| {
            create_evm_account_locally(service, params)
        })
    }

    pub fn handle_json_request(&self, body: &str) -> String {
        self.handle_json_request_with_headers(body, &BTreeMap::new())
    }

    pub(crate) fn handle_json_request_with_headers(
        &self,
        body: &str,
        headers: &BTreeMap<String, String>,
    ) -> String {
        self.handle_json_http_request_with_headers(body, headers)
            .body
    }

    pub(crate) fn handle_json_http_request_with_headers(
        &self,
        body: &str,
        headers: &BTreeMap<String, String>,
    ) -> JsonHttpResponse {
        let request = match serde_json::from_str::<JsonRpcRequest>(body) {
            Ok(request) => request,
            Err(_) => {
                let body = serialize_json_rpc_response(JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    result: None,
                    error: Some(JsonRpcErrorObject {
                        code: -32700,
                        message: "parse error".into(),
                        data: None,
                    }),
                    id: Value::Null,
                });
                return JsonHttpResponse {
                    status: StatusCode::OK,
                    headers: BTreeMap::new(),
                    content_type: "application/json",
                    body,
                };
            }
        };

        let mut response_headers = BTreeMap::new();
        if let Some(config) = self.x402_config_for_method(&request.method) {
            match self.process_x402_payment(config, headers) {
                Ok(Some(payment_response)) => {
                    response_headers.insert(X402_PAYMENT_RESPONSE_HEADER.into(), payment_response);
                }
                Ok(None) => {}
                Err(error) => {
                    return self.x402_payment_required_response(request.id, config, error)
                }
            }
        }

        let response = match self.dispatch_json_rpc(request.clone(), headers) {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: Some(result),
                error: None,
                id: request.id,
            },
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                result: None,
                error: Some(error),
                id: request.id,
            },
        };

        JsonHttpResponse {
            status: StatusCode::OK,
            headers: response_headers,
            content_type: "application/json",
            body: serialize_json_rpc_response(response),
        }
    }

    fn x402_config_for_method(&self, method: &str) -> Option<&X402Config> {
        self.x402_config
            .as_ref()
            .filter(|config| config.protects_method(method))
    }

    fn x402_payment_required_response(
        &self,
        request_id: Value,
        config: &X402Config,
        error: String,
    ) -> JsonHttpResponse {
        let mut headers = BTreeMap::new();
        let challenge =
            match encode_x402_header_value(&config.payment_required(Some(error.clone()))) {
                Ok(challenge) => challenge,
                Err(error) => {
                    let body = serialize_json_rpc_response(JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        result: None,
                        error: Some(JsonRpcErrorObject {
                            code: -32000,
                            message: error,
                            data: None,
                        }),
                        id: request_id,
                    });
                    return JsonHttpResponse {
                        status: StatusCode::INTERNAL_SERVER_ERROR,
                        headers,
                        content_type: "application/json",
                        body,
                    };
                }
            };
        headers.insert(X402_PAYMENT_REQUIRED_HEADER.into(), challenge);
        let body = serialize_json_rpc_response(JsonRpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(JsonRpcErrorObject {
                code: -32042,
                message: "payment required".into(),
                data: Some(json!({
                    "x402": true,
                    "reason": error,
                })),
            }),
            id: request_id,
        });
        JsonHttpResponse {
            status: StatusCode::PAYMENT_REQUIRED,
            headers,
            content_type: "application/json",
            body,
        }
    }

    fn process_x402_payment(
        &self,
        config: &X402Config,
        headers: &BTreeMap<String, String>,
    ) -> Result<Option<String>, String> {
        let Some(payment_header) = x402_payment_header(headers) else {
            return Err("payment required".into());
        };
        let payment_payload = decode_x402_header_value(payment_header)?;
        let version = payment_payload
            .get("x402Version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "missing x402Version".to_string())?;
        if version != X402_VERSION as u64 {
            return Err(format!("unsupported x402Version {version}"));
        }
        let requirement = x402_select_requirement(config, &payment_payload)?;

        if config.allow_unverified {
            return encode_x402_header_value(&json!({
                "success": true,
                "verified": false,
                "settled": false,
                "x402Version": X402_VERSION,
            }))
            .map(Some);
        }

        let facilitator_url = config
            .facilitator_url
            .as_deref()
            .ok_or_else(|| "x402 facilitator url is not configured".to_string())?;
        let facilitator_body = json!({
            "x402Version": X402_VERSION,
            "paymentPayload": payment_payload,
            "paymentRequirements": requirement,
        });
        let verify_response =
            self.post_x402_facilitator(facilitator_url, "verify", &facilitator_body)?;
        if !x402_response_success(&verify_response) {
            return Err(x402_response_error(
                &verify_response,
                "x402 payment verification failed",
            ));
        }
        if !config.settle {
            return encode_x402_header_value(&verify_response).map(Some);
        }

        let settle_response =
            self.post_x402_facilitator(facilitator_url, "settle", &facilitator_body)?;
        if !x402_response_success(&settle_response) {
            return Err(x402_response_error(
                &settle_response,
                "x402 payment settlement failed",
            ));
        }
        encode_x402_header_value(&settle_response).map(Some)
    }

    fn post_x402_facilitator(
        &self,
        facilitator_url: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, String> {
        let endpoint = format!(
            "{}/{}",
            facilitator_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        let response = self
            .network_http_client
            .post(endpoint)
            .header("content-type", "application/json")
            .json(body)
            .send()
            .map_err(|error| format!("x402 facilitator {path} request failed: {error}"))?;
        let status = response.status();
        let body_text = response
            .text()
            .map_err(|error| format!("x402 facilitator {path} response read failed: {error}"))?;
        if !status.is_success() {
            let detail = body_text.trim();
            return if detail.is_empty() {
                Err(format!("x402 facilitator {path} returned {status}"))
            } else {
                Err(format!(
                    "x402 facilitator {path} returned {status}: {detail}"
                ))
            };
        }
        serde_json::from_str(&body_text)
            .map_err(|error| format!("x402 facilitator {path} response decode failed: {error}"))
    }

    pub fn serve(&self, address: &str) -> Result<(), TransportError> {
        if !self.config.peer_transport_enabled {
            return self.serve_axum(address);
        }
        self.serve_legacy(address)
    }

    fn serve_legacy(&self, address: &str) -> Result<(), TransportError> {
        let listener = TcpListener::bind(address).map_err(TransportError::Bind)?;
        for stream in listener.incoming() {
            let mut stream = stream.map_err(TransportError::Accept)?;
            let server = self.clone();
            thread::spawn(move || {
                let _ = server.handle_http_connection(&mut stream);
            });
        }
        Ok(())
    }

    fn serve_axum(&self, address: &str) -> Result<(), TransportError> {
        let runtime = TokioRuntimeBuilder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(TransportError::Bind)?;
        let server = Arc::new(self.clone());
        let serve_network_routes = self
            .config
            .network_bootstrap
            .as_ref()
            .is_some_and(NetworkBootstrapConfig::serves_network_routes);
        let network_bind = env::var("KOMBAT_NETWORK_RPC_BIND")
            .ok()
            .filter(|bind| bind != address && !bind.trim().is_empty());
        runtime.block_on(async move {
            let public_listener = tokio::net::TcpListener::bind(address)
                .await
                .map_err(TransportError::Bind)?;
            let public_routes = Router::new()
                .route("/", post(axum_handle_json_rpc).options(axum_handle_options))
                .route(
                    "/events",
                    get(axum_handle_events).options(axum_handle_options),
                )
                .route(
                    "/market-data/markets/stream",
                    get(axum_handle_market_stream).options(axum_handle_options),
                )
                .route(
                    "/market-data/accounts/stream",
                    get(axum_handle_account_stream).options(axum_handle_options),
                )
                .route(
                    "/market-data/wallets/stream",
                    get(axum_handle_wallet_stream).options(axum_handle_options),
                );
            if serve_network_routes {
                if let Some(network_bind) = network_bind {
                    let network_listener = tokio::net::TcpListener::bind(&network_bind)
                        .await
                        .map_err(TransportError::Bind)?;
                    let public_app = public_routes.with_state(server.clone());
                    let network_app = Router::new()
                        .route("/", post(axum_handle_json_rpc))
                        .route("/network/bootstrap", get(axum_handle_network_bootstrap))
                        .route("/network/ping", get(axum_handle_network_ping))
                        .route("/network/snapshot", get(axum_handle_network_snapshot))
                        .route(
                            "/network/head-qc-signature",
                            post(axum_handle_network_head_qc_signature),
                        )
                        .route(
                            "/network/timeout-message",
                            post(axum_handle_network_timeout_message),
                        )
                        .route(
                            "/network/qc-signature",
                            post(axum_handle_network_qc_signature),
                        )
                        .route(
                            "/network/relay-intents",
                            post(axum_handle_network_relay_intents),
                        )
                        .route(
                            "/network/relay-delegation-actions",
                            post(axum_handle_network_relay_delegation_actions),
                        )
                        .route(
                            "/network/relay-evm-delegation-actions",
                            post(axum_handle_network_relay_evm_delegation_actions),
                        )
                        .route(
                            "/network/relay-market-maker-program-actions",
                            post(axum_handle_network_relay_market_maker_program_actions),
                        )
                        .route("/network/mahi/block", post(axum_handle_network_mahi_block))
                        .route(
                            "/network/mahi/blocks",
                            post(axum_handle_network_mahi_blocks),
                        )
                        .route("/network/mahi/fetch", post(axum_handle_network_mahi_fetch))
                        .route("/network/commit", post(axum_handle_network_commit))
                        .route(
                            "/network/install-timeout-certificate",
                            post(axum_handle_network_timeout_certificate),
                        )
                        .with_state(server);
                    tokio::try_join!(
                        axum::serve(public_listener, public_app),
                        axum::serve(network_listener, network_app)
                    )
                    .map(|_| ())
                    .map_err(TransportError::Accept)
                } else {
                    let app = public_routes
                        .route("/network/bootstrap", get(axum_handle_network_bootstrap))
                        .route("/network/ping", get(axum_handle_network_ping))
                        .route("/network/snapshot", get(axum_handle_network_snapshot))
                        .route(
                            "/network/head-qc-signature",
                            post(axum_handle_network_head_qc_signature),
                        )
                        .route(
                            "/network/timeout-message",
                            post(axum_handle_network_timeout_message),
                        )
                        .route(
                            "/network/qc-signature",
                            post(axum_handle_network_qc_signature),
                        )
                        .route(
                            "/network/relay-intents",
                            post(axum_handle_network_relay_intents),
                        )
                        .route(
                            "/network/relay-delegation-actions",
                            post(axum_handle_network_relay_delegation_actions),
                        )
                        .route(
                            "/network/relay-evm-delegation-actions",
                            post(axum_handle_network_relay_evm_delegation_actions),
                        )
                        .route(
                            "/network/relay-market-maker-program-actions",
                            post(axum_handle_network_relay_market_maker_program_actions),
                        )
                        .route("/network/mahi/block", post(axum_handle_network_mahi_block))
                        .route(
                            "/network/mahi/blocks",
                            post(axum_handle_network_mahi_blocks),
                        )
                        .route("/network/mahi/fetch", post(axum_handle_network_mahi_fetch))
                        .route("/network/commit", post(axum_handle_network_commit))
                        .route(
                            "/network/install-timeout-certificate",
                            post(axum_handle_network_timeout_certificate),
                        )
                        .with_state(server);
                    axum::serve(public_listener, app)
                        .await
                        .map_err(TransportError::Accept)
                }
            } else {
                axum::serve(public_listener, public_routes.with_state(server))
                    .await
                    .map_err(TransportError::Accept)
            }
        })
    }

    pub fn maybe_run_network_round_as_leader(
        &self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> Result<Option<(u64, usize)>, String> {
        let network_bootstrap = match self.config.network_bootstrap.as_ref() {
            Some(network_bootstrap) => network_bootstrap,
            None => return Ok(None),
        };

        let should_run = {
            let engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            if engine.intent_mempool().is_empty() && engine.conditional_intent_pool().is_empty() {
                return Ok(None);
            }

            let local_validator_id = engine
                .local_validator_id()
                .ok_or_else(|| "local validator id unavailable".to_string())?;
            let expected_leader = select_leader_for_validator_set(
                engine.current_round(),
                engine.chain_id(),
                &network_bootstrap.validator_set,
            )
            .map_err(|error| format!("{error:?}"))?;
            expected_leader == local_validator_id
        };

        if !should_run {
            return Ok(None);
        }

        self.run_network_round(timestamp_ms, max_items)
            .map(|response| Some((response.height, response.item_count)))
            .map_err(|error| {
                error
                    .data
                    .as_ref()
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned)
                    .unwrap_or(error.message)
            })
    }

    pub fn maybe_run_network_timeout_recovery_as_follower(
        &self,
        timestamp_ms: u64,
    ) -> Result<Option<u64>, String> {
        let network_bootstrap = match self.config.network_bootstrap.as_ref() {
            Some(network_bootstrap) => network_bootstrap,
            None => return Ok(None),
        };

        let (local_validator_id, expected_leader, last_progress_timestamp_ms, round_timeout_ms) = {
            let engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            let local_validator_id = engine
                .local_validator_id()
                .ok_or_else(|| "local validator id unavailable".to_string())?;
            let expected_leader = select_leader_for_validator_set(
                engine.current_round(),
                engine.chain_id(),
                &network_bootstrap.validator_set,
            )
            .map_err(|error| format!("{error:?}"))?;
            if expected_leader == local_validator_id {
                return Ok(None);
            }
            (
                local_validator_id,
                expected_leader,
                engine.head().header.timestamp_ms,
                engine.round_timeout_ms(),
            )
        };
        if timestamp_ms < last_progress_timestamp_ms.saturating_add(round_timeout_ms) {
            return Ok(None);
        }

        self.maybe_drive_timeout_control_plane(timestamp_ms)
            .map(|response| response.map(|response| response.round))
            .map_err(|error| {
                error
                    .data
                    .as_ref()
                    .and_then(|value| value.as_str())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| {
                        format!(
                            "validator {} failed timeout control plane for leader {}: {}",
                            encode_hex(&local_validator_id),
                            encode_hex(&expected_leader),
                            error.message
                        )
                    })
            })
    }

    pub(crate) fn handle_http_connection(
        &self,
        stream: &mut TcpStream,
    ) -> Result<(), TransportError> {
        if self.config.peer_transport_enabled {
            let mut prefix = [0_u8; PEER_TRANSPORT_MAGIC.len()];
            if stream.peek(&mut prefix).map_err(TransportError::Read)? == PEER_TRANSPORT_MAGIC.len()
                && prefix == *PEER_TRANSPORT_MAGIC
            {
                return self.handle_peer_transport_connection(stream);
            }
        }
        let request = match read_http_request(stream) {
            Ok(request) => request,
            Err(TransportError::RequestTooLarge) => {
                return write_http_response(
                    stream,
                    "413 Payload Too Large",
                    "text/plain",
                    "request too large",
                );
            }
            Err(error) => return Err(error),
        };
        if request.method == "OPTIONS" {
            return write_http_response_for_request(
                stream,
                "204 No Content",
                "text/plain",
                &request.headers,
                "",
            );
        }
        if request.method == "GET" && request.path == "/network/bootstrap" {
            return self.handle_network_bootstrap_request(stream);
        }
        if request.method == "GET" && request.path == "/network/ping" {
            return self.handle_network_ping_request(stream);
        }
        if request.method == "GET" && request.path == "/network/snapshot" {
            return self.handle_network_snapshot_request(stream, &request.headers);
        }
        if request.method == "POST" && request.path == "/network/head-qc-signature" {
            return match self.handle_network_head_qc_signature_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/timeout-message" {
            return match self.handle_network_timeout_message_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/qc-signature" {
            return match self.handle_network_qc_signature_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/relay-intents" {
            return match self.handle_network_relay_intents_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/relay-delegation-actions" {
            return match self.handle_network_relay_delegation_actions_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/relay-evm-delegation-actions" {
            return match self.handle_network_relay_evm_delegation_actions_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/relay-market-maker-program-actions"
        {
            return match self.handle_network_relay_market_maker_program_actions_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/mahi/block" {
            return match self.handle_network_mahi_block_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/mahi/blocks" {
            return match self.handle_network_mahi_blocks_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/mahi/fetch" {
            return match self.handle_network_mahi_fetch_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/commit" {
            return match self.handle_network_commit_request(stream, &request.headers, &request.body)
            {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "POST" && request.path == "/network/install-timeout-certificate" {
            return match self.handle_network_timeout_certificate_request(
                stream,
                &request.headers,
                &request.body,
            ) {
                Ok(()) => Ok(()),
                Err(error) => write_network_handler_error(stream, error),
            };
        }
        if request.method == "GET" && request.path == "/events" {
            return self.handle_event_stream_connection(stream, &request);
        }
        if request.method == "GET" && request.path == "/market-data/markets/stream" {
            return self.handle_market_stream_connection(stream, &request);
        }
        if request.method == "GET" && request.path == "/market-data/accounts/stream" {
            return self.handle_account_stream_connection(stream, &request);
        }
        if request.method == "GET" && request.path == "/market-data/wallets/stream" {
            return self.handle_wallet_stream_connection(stream, &request);
        }

        if request.method != "POST" {
            return write_http_response_for_request(
                stream,
                "405 Method Not Allowed",
                "text/plain",
                &request.headers,
                "method not allowed",
            );
        }

        let response = self.handle_json_http_request_with_headers(&request.body, &request.headers);
        write_http_response_with_headers_for_request(
            stream,
            &http_status_line(response.status),
            response.content_type,
            &response.headers,
            &request.headers,
            &response.body,
        )
    }

    pub(super) fn run_network_round(
        &self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> Result<NetworkRoundResponse, JsonRpcErrorObject> {
        // Serialize operator bridge fanout so concurrent submissions do not
        // apply in one order locally and a different order on peers.
        let _network_control = self
            .network_control
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "network control lock poisoned".into(),
                data: None,
            })?;
        let network_bootstrap =
            self.config
                .network_bootstrap
                .as_ref()
                .ok_or(JsonRpcErrorObject {
                    code: -32000,
                    message: "network bootstrap not configured".into(),
                    data: None,
                })?;
        let validator_verifiers = network_validator_verifiers(network_bootstrap)?;
        let required_signatures =
            quorum_threshold(validator_verifiers.len()).ok_or(JsonRpcErrorObject {
                code: -32000,
                message: "network validator set has no quorum threshold".into(),
                data: None,
            })?;
        let local_validator_id =
            network_bootstrap
                .local_validator_id()
                .ok_or(JsonRpcErrorObject {
                    code: -32000,
                    message: "local validator id unavailable".into(),
                    data: None,
                })?;
        let head_open_wait_started = Instant::now();
        let (head, local_head_signature) = {
            let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            let acquired_at = Instant::now();
            let round = engine
                .current_round()
                .max(engine.head().header.round.saturating_add(1));
            let expected_leader = select_leader_for_validator_set(
                round,
                engine.chain_id(),
                &network_bootstrap.validator_set,
            )
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;
            if expected_leader != local_validator_id {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: format!("local validator is not leader for round {}", round),
                    data: None,
                });
            }
            let signature = engine
                .sign_current_head_qc(block_hash(engine.head()), engine.head().header.round)
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{error:?}"),
                    data: None,
                })?;
            let result = (engine.head().clone(), signature);
            self.record_network_round_lock_profile(
                NetworkRoundLockProfilePhase::HeadOpen,
                duration_as_nanos_u64(acquired_at.duration_since(head_open_wait_started)),
                duration_as_nanos_u64(acquired_at.elapsed()),
            );
            result
        };

        let head_signature_body = NetworkHeadSignatureRequest {
            head_hash_hex: encode_hex(&block_hash(&head)),
            round: head.header.round,
        };
        let mut head_signatures = vec![local_head_signature];
        for peer in &network_bootstrap.bootstrap_peers.peers {
            let signature_response: NetworkSignatureResponse = match self.send_network_json_request(
                &peer.rpc_endpoints,
                "head-qc-signature",
                &head_signature_body,
            ) {
                Ok(response) => response,
                Err(NetworkRequestError::Unreachable(_)) => continue,
                Err(NetworkRequestError::Rejected(message)) => {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!(
                            "peer {} head-qc-signature request failed: {message}",
                            encode_hex(&peer.validator_id)
                        ),
                        data: None,
                    });
                }
            };
            head_signatures.push(decode_borsh_hex(&signature_response.signature_hex).map_err(
                |message| JsonRpcErrorObject {
                    code: -32000,
                    message: format!(
                        "peer {} returned invalid head signature payload: {message}",
                        encode_hex(&peer.validator_id)
                    ),
                    data: None,
                },
            )?);
            if head_signatures.len() >= required_signatures {
                break;
            }
        }
        if head_signatures.len() < required_signatures {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: format!(
                    "network head qc quorum unavailable: collected {} of {} required signatures",
                    head_signatures.len(),
                    required_signatures
                ),
                data: None,
            });
        }
        let head_qc = assemble_quorum_certificate(&head, head_signatures, &validator_verifiers)
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;

        let build_wait_started = Instant::now();
        let prepared_block = {
            let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            let acquired_at = Instant::now();
            engine
                .install_head_qc(head_qc, &validator_verifiers)
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{error:?}"),
                    data: None,
                })?;
            let round = engine
                .current_round()
                .max(engine.head().header.round.saturating_add(1));
            let expected_leader = select_leader_for_validator_set(
                round,
                engine.chain_id(),
                &network_bootstrap.validator_set,
            )
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;
            if expected_leader != local_validator_id {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: format!(
                        "local validator lost leadership after head qc install for round {}",
                        round
                    ),
                    data: None,
                });
            }
            let result = engine
                .prepare_network_block_commit_from_mempool(
                    local_validator_id,
                    &validator_verifiers,
                    timestamp_ms,
                    max_items,
                    1_000,
                )
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{error:?}"),
                    data: None,
                })?;
            self.record_network_round_lock_profile(
                NetworkRoundLockProfilePhase::Build,
                duration_as_nanos_u64(acquired_at.duration_since(build_wait_started)),
                duration_as_nanos_u64(acquired_at.elapsed()),
            );
            result
        };
        let block = prepared_block.block().clone();

        let local_signature = {
            let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            engine
                .sign_network_block_qc(&block, &validator_verifiers, local_validator_id, 1_000)
                .map_err(|error| JsonRpcErrorObject {
                    code: -32000,
                    message: format!("{error:?}"),
                    data: None,
                })?
        };

        let mut signatures = vec![local_signature];
        let proposal_body = NetworkProposalRequest {
            block_hex: encode_borsh_hex(&block),
        };
        for peer in &network_bootstrap.bootstrap_peers.peers {
            let signature_response: NetworkSignatureResponse = match self.send_network_json_request(
                &peer.rpc_endpoints,
                "qc-signature",
                &proposal_body,
            ) {
                Ok(response) => response,
                Err(NetworkRequestError::Unreachable(_)) => continue,
                Err(NetworkRequestError::Rejected(message)) => {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!(
                            "peer {} qc-signature request failed for block_round={} proposer={}: {message}",
                            encode_hex(&peer.validator_id),
                            block.header.round,
                            encode_hex(&block.header.proposer)
                        ),
                        data: None,
                    });
                }
            };
            signatures.push(decode_borsh_hex(&signature_response.signature_hex).map_err(
                |message| JsonRpcErrorObject {
                    code: -32000,
                    message: format!(
                        "peer {} returned invalid signature payload: {message}",
                        encode_hex(&peer.validator_id)
                    ),
                    data: None,
                },
            )?);
            if signatures.len() >= required_signatures {
                break;
            }
        }
        if signatures.len() < required_signatures {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: format!(
                    "network proposal quorum unavailable: collected {} of {} required signatures",
                    signatures.len(),
                    required_signatures
                ),
                data: None,
            });
        }

        let qc = assemble_quorum_certificate(&block, signatures.clone(), &validator_verifiers)
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;
        drop(prepared_block);
        let result = {
            let head = self.engine.head_snapshot();
            NetworkRoundResponse {
                height: head.head_height,
                hash_hex: encode_hex(&head.head_hash),
                item_count: block.body.items.len(),
                peer_count: network_bootstrap.bootstrap_peers.peers.len(),
                signature_count: signatures.len(),
            }
        };

        let commit_body = NetworkCommitRequest {
            block_hex: encode_borsh_hex(&block),
            qc_hex: encode_borsh_hex(&qc),
        };
        let commit_body =
            serde_json::to_string(&commit_body).map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("serialize commit request failed: {error}"),
                data: None,
            })?;
        for (validator_id, result) in self
            .collect_network_json_request_results::<NetworkRoundResponse>(
                &network_bootstrap.bootstrap_peers.peers,
                "commit",
                commit_body,
                network_bootstrap.bootstrap_peers.peers.len(),
            )
        {
            match result {
                Ok(_) | Err(NetworkRequestError::Unreachable(_)) => continue,
                Err(NetworkRequestError::Rejected(message)) => {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!(
                            "peer {} commit request failed: {message}",
                            encode_hex(&validator_id)
                        ),
                        data: None,
                    });
                }
            }
        }

        Ok(result)
    }

    pub(super) fn run_network_timeout_recovery(
        &self,
        timestamp_ms: u64,
    ) -> Result<NetworkTimeoutRecoveryResponse, JsonRpcErrorObject> {
        self.maybe_drive_timeout_control_plane(timestamp_ms)?
            .ok_or(JsonRpcErrorObject {
                code: -32000,
                message: "timeout recovery quorum unavailable".into(),
                data: None,
            })
    }

    fn maybe_drive_timeout_control_plane(
        &self,
        timestamp_ms: u64,
    ) -> Result<Option<NetworkTimeoutRecoveryResponse>, JsonRpcErrorObject> {
        let started = Instant::now();
        let result = (|| {
            let _network_control = self
                .network_control
                .lock()
                .map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "network control lock poisoned".into(),
                    data: None,
                })?;
            let network_bootstrap =
                self.config
                    .network_bootstrap
                    .as_ref()
                    .ok_or(JsonRpcErrorObject {
                        code: -32000,
                        message: "network bootstrap not configured".into(),
                        data: None,
                    })?;
            let validator_verifiers = network_validator_verifiers(network_bootstrap)?;

            let (chain_id, round, local_validator_id) = {
                let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "engine lock poisoned".into(),
                    data: None,
                })?;
                (
                    engine.chain_id(),
                    engine.current_round(),
                    engine.local_validator_id().ok_or(JsonRpcErrorObject {
                        code: -32000,
                        message: "local validator id unavailable".into(),
                        data: None,
                    })?,
                )
            };
            let Some(local_timeout_message) =
                self.local_timeout_for_round(round, &validator_verifiers)?
            else {
                return Ok(None);
            };

            let timeout_request = NetworkTimeoutRequest {
                head_hash_hex: None,
                head_height: None,
                round,
                timeout_hex: Some(encode_borsh_hex(&local_timeout_message)),
            };
            let required_messages =
                quorum_threshold(validator_verifiers.len()).ok_or(JsonRpcErrorObject {
                    code: -32000,
                    message: "network validator set has no quorum threshold".into(),
                    data: None,
                })?;
            let mut timeout_request_errors = Vec::new();
            for sweep in 0..3 {
                if self.timeout_messages_for_round(round)?.len() >= required_messages {
                    break;
                }
                for peer in &network_bootstrap.bootstrap_peers.peers {
                    if peer.validator_id == local_validator_id {
                        continue;
                    }
                    let timeout_response: NetworkTimeoutResponse = match self
                        .send_network_json_request(
                            &peer.rpc_endpoints,
                            "timeout-message",
                            &timeout_request,
                        ) {
                        Ok(response) => response,
                        Err(NetworkRequestError::Unreachable(_)) => continue,
                        Err(NetworkRequestError::Rejected(message)) => {
                            timeout_request_errors.push(format!(
                                "peer {} timeout-message request failed: {message}",
                                encode_hex(&peer.validator_id)
                            ));
                            continue;
                        }
                    };
                    if let Some(timeout_hex) = timeout_response.timeout_hex {
                        let timeout: TimeoutMessage =
                            decode_borsh_hex(&timeout_hex).map_err(|message| {
                                JsonRpcErrorObject {
                                    code: -32000,
                                    message: format!(
                                        "peer {} returned invalid timeout payload: {message}",
                                        encode_hex(&peer.validator_id)
                                    ),
                                    data: None,
                                }
                            })?;
                        if timeout.round != round {
                            timeout_request_errors.push(format!(
                                "peer {} returned timeout for round {} instead of {}",
                                encode_hex(&peer.validator_id),
                                timeout.round,
                                round
                            ));
                            continue;
                        }
                        self.store_timeout_message(timeout, &validator_verifiers)?;
                    }
                }
                if sweep < 2 && self.timeout_messages_for_round(round)?.len() < required_messages {
                    thread::sleep(Duration::from_millis(100));
                }
            }
            let timeout_messages = self.timeout_messages_for_round(round)?;
            if timeout_messages.len() < required_messages {
                return Ok(None);
            }
            let timeout_certificate =
                build_timeout_certificate(round, &timeout_messages, &validator_verifiers).map_err(
                    |error| JsonRpcErrorObject {
                        code: -32000,
                        message: format!("{error:?}"),
                        data: None,
                    },
                )?;
            let local_head_matches_timeout_qc = {
                let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "engine lock poisoned".into(),
                    data: None,
                })?;
                block_hash(engine.head()) == timeout_certificate.highest_qc.block_hash
            };
            if !local_head_matches_timeout_qc {
                if let Some(source) = timeout_messages.iter().find(|message| {
                    message.high_qc.view == timeout_certificate.highest_qc.view
                        && message.high_qc.block_hash == timeout_certificate.highest_qc.block_hash
                }) {
                    self.maybe_force_repair_from_validator_snapshot(
                        source.sender,
                        Some(source.height),
                        None,
                        Some(timeout_certificate.highest_qc.block_hash),
                        "timeout_certificate_high_qc_mismatch",
                    )
                    .map_err(|error| JsonRpcErrorObject {
                        code: -32000,
                        message: error,
                        data: None,
                    })?;
                }
            }

            let new_round = {
                let mut engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                    code: -32000,
                    message: "engine lock poisoned".into(),
                    data: None,
                })?;
                engine
                    .install_timeout_messages(&timeout_messages, &validator_verifiers, timestamp_ms)
                    .map_err(|error| JsonRpcErrorObject {
                        code: -32000,
                        message: format!("{error:?}"),
                        data: None,
                    })?
            };

            let leader = select_leader_for_validator_set(
                new_round,
                chain_id,
                &network_bootstrap.validator_set,
            )
            .map_err(|error| JsonRpcErrorObject {
                code: -32000,
                message: format!("{error:?}"),
                data: None,
            })?;

            let certificate_request = NetworkTimeoutCertificateRequest {
                timeout_messages_hex: timeout_messages.iter().map(encode_borsh_hex).collect(),
                timestamp_ms,
            };
            let timeout_participants: BTreeSet<ValidatorId> = timeout_messages
                .iter()
                .map(|message| message.sender)
                .collect();
            let mut leader_confirmed = leader == local_validator_id;
            for peer in &network_bootstrap.bootstrap_peers.peers {
                if peer.validator_id == local_validator_id {
                    continue;
                }
                let requires_confirmation = peer.validator_id == leader
                    || timeout_participants.contains(&peer.validator_id);
                let mut last_error: Option<String> = None;
                let mut install_response: Option<NetworkTimeoutCertificateResponse> = None;
                for attempt in 0..3 {
                    match self.send_network_json_request(
                        &peer.rpc_endpoints,
                        "install-timeout-certificate",
                        &certificate_request,
                    ) {
                        Ok(decoded) => {
                            install_response = Some(decoded);
                            break;
                        }
                        Err(NetworkRequestError::Unreachable(error))
                        | Err(NetworkRequestError::Rejected(error)) => {
                            last_error = Some(format!(
                                "peer {} install-timeout-certificate request failed: {error}",
                                encode_hex(&peer.validator_id)
                            ));
                            if attempt < 2 {
                                thread::sleep(Duration::from_millis(100));
                            }
                        }
                    }
                }

                let Some(install_response) = install_response else {
                    if requires_confirmation {
                        return Err(JsonRpcErrorObject {
                            code: -32000,
                            message: last_error.unwrap_or_else(|| {
                                format!(
                                    "peer {} did not confirm timeout-certificate installation",
                                    encode_hex(&peer.validator_id)
                                )
                            }),
                            data: None,
                        });
                    }
                    continue;
                };

                if requires_confirmation && install_response.round < new_round {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!(
                            "peer {} failed to advance to recovered round {}",
                            encode_hex(&peer.validator_id),
                            new_round
                        ),
                        data: None,
                    });
                }
                if peer.validator_id == leader {
                    leader_confirmed = true;
                }
            }
            if !leader_confirmed {
                return Err(JsonRpcErrorObject {
                    code: -32000,
                    message: format!(
                        "new leader {} did not confirm timeout-certificate installation",
                        encode_hex(&leader)
                    ),
                    data: None,
                });
            }
            if leader != local_validator_id {
                let leader_endpoints =
                    self.network_validator_rpc_endpoints(&network_bootstrap.validator_set, leader)?;
                let leader_ping: NetworkPingResponse = self
                    .send_network_ping_request(&leader_endpoints)
                    .map_err(|error| JsonRpcErrorObject {
                        code: -32000,
                        message: format!(
                            "new leader {} ping verification failed: {}",
                            encode_hex(&leader),
                            match error {
                                NetworkRequestError::Unreachable(message)
                                | NetworkRequestError::Rejected(message) => message,
                            }
                        ),
                        data: None,
                    })?;
                let leader_round = leader_ping.current_round;
                if leader_round < new_round {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!(
                            "new leader {} did not advance to recovered round {} (actual round {})",
                            encode_hex(&leader),
                            new_round,
                            leader_round
                        ),
                        data: None,
                    });
                }
            }

            Ok(Some(NetworkTimeoutRecoveryResponse {
                round: new_round,
                peer_count: network_bootstrap.bootstrap_peers.peers.len(),
                message_count: timeout_messages.len(),
                leader_hex: encode_hex(&leader),
            }))
        })();
        let recovered = result
            .as_ref()
            .ok()
            .and_then(|response| response.as_ref())
            .is_some();
        self.record_timeout_recovery_metric(started.elapsed(), recovered);
        result
    }

    fn handle_event_stream_connection(
        &self,
        stream: &mut TcpStream,
        request: &HttpRequest,
    ) -> Result<(), TransportError> {
        let params = parse_event_stream_request(request.query.as_deref()).map_err(|message| {
            TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                message,
            ))
        })?;
        write_sse_headers(stream, &request.headers)?;

        let started_at = Instant::now();
        let timeout = Duration::from_millis(params.timeout_ms.min(60_000));
        let mut next_seq = params.from_seq;
        if !params.from_seq_explicit {
            if let Some(last_event_id) = request.headers.get("last-event-id") {
                if let Ok(resume_from) = last_event_id.parse::<u64>() {
                    next_seq = resume_from.saturating_add(1);
                }
            }
        }

        loop {
            let page = {
                let engine = self.engine.lock().map_err(|_| {
                    TransportError::Read(std::io::Error::other("engine lock poisoned"))
                })?;
                EventStream::new(engine.runtime_events()).query(StreamQuery {
                    from_seq: next_seq,
                    before_seq: None,
                    limit: params.limit,
                    filter: params.filter,
                    descending: false,
                })
            };

            if !page.events.is_empty() {
                for event in &page.events {
                    write_sse_event(stream, event)?;
                }
                next_seq = page.next_cursor.next_seq;
                write_sse_cursor(stream, next_seq)?;
                if !params.follow {
                    break;
                }
                continue;
            }

            if !params.follow || started_at.elapsed() >= timeout {
                write_sse_cursor(stream, next_seq)?;
                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }

    fn handle_market_stream_connection(
        &self,
        stream: &mut TcpStream,
        request: &HttpRequest,
    ) -> Result<(), TransportError> {
        let params = parse_market_stream_request(request.query.as_deref()).map_err(|message| {
            TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                message,
            ))
        })?;
        write_sse_headers(stream, &request.headers)?;

        let mut next_seq = params.from_seq;
        if !params.from_seq_explicit {
            if let Some(last_event_id) = request.headers.get("last-event-id") {
                if let Ok(resume_from) = last_event_id.parse::<u64>() {
                    next_seq = resume_from.saturating_add(1);
                }
            }
        }

        let snapshot = {
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| TransportError::Read(std::io::Error::other("engine lock poisoned")))?;
            let service = NodeRpcService::new(&mut engine);
            market_snapshot_response_to_json(
                &service.get_market_snapshot(params.market_id, params.depth),
            )
        };
        write_sse_json_event(stream, "market_snapshot", None, &snapshot)?;

        let started_at = Instant::now();
        let timeout = Duration::from_millis(params.timeout_ms.min(60_000));
        loop {
            let (items, cursor) = {
                let mut engine = self.engine.lock().map_err(|_| {
                    TransportError::Read(std::io::Error::other("engine lock poisoned"))
                })?;
                let service = NodeRpcService::new(&mut engine);
                let page = EventStream::new(service.engine.runtime_events()).query(StreamQuery {
                    from_seq: next_seq,
                    before_seq: None,
                    limit: params.limit,
                    filter: EventFilter {
                        block_height: None,
                        market_id: Some(params.market_id),
                        account_id: None,
                        policy_id: None,
                        intent_id: None,
                        client_order_id: None,
                        topic: None,
                    },
                    descending: false,
                });
                let cursor = page.next_cursor.next_seq;
                let items = page
                    .events
                    .iter()
                    .flat_map(|event| {
                        market_stream_payload_for_event(&service, event, params.depth)
                    })
                    .collect::<Vec<_>>();
                (items, cursor)
            };

            if !items.is_empty() {
                for (name, seq, payload) in &items {
                    write_sse_json_event(stream, name, Some(*seq), payload)?;
                }
                next_seq = cursor;
                write_sse_cursor(stream, next_seq)?;
                if !params.follow {
                    break;
                }
                continue;
            }

            next_seq = cursor;
            if !params.follow || started_at.elapsed() >= timeout {
                write_sse_cursor(stream, next_seq)?;
                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }

    fn handle_account_stream_connection(
        &self,
        stream: &mut TcpStream,
        request: &HttpRequest,
    ) -> Result<(), TransportError> {
        let params = parse_account_stream_request(request.query.as_deref()).map_err(|message| {
            TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                message,
            ))
        })?;
        write_sse_headers(stream, &request.headers)?;

        let mut next_seq = params.from_seq;
        if !params.from_seq_explicit {
            if let Some(last_event_id) = request.headers.get("last-event-id") {
                if let Ok(resume_from) = last_event_id.parse::<u64>() {
                    next_seq = resume_from.saturating_add(1);
                }
            }
        }

        let snapshot = {
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| TransportError::Read(std::io::Error::other("engine lock poisoned")))?;
            let service = NodeRpcService::new(&mut engine);
            account_trading_state_response_to_json(&service.get_account_trading_state(
                params.account_id,
                params.market_id,
                params.depth,
                params.limit,
            ))
        };
        write_sse_json_event(stream, "account_trading_state", None, &snapshot)?;

        let started_at = Instant::now();
        let timeout = Duration::from_millis(params.timeout_ms.min(60_000));
        loop {
            let (items, cursor) = {
                let mut engine = self.engine.lock().map_err(|_| {
                    TransportError::Read(std::io::Error::other("engine lock poisoned"))
                })?;
                let service = NodeRpcService::new(&mut engine);
                let page = EventStream::new(service.engine.runtime_events()).query(StreamQuery {
                    from_seq: next_seq,
                    before_seq: None,
                    limit: params.limit,
                    filter: EventFilter {
                        block_height: None,
                        market_id: params.market_id,
                        account_id: Some(params.account_id),
                        policy_id: None,
                        intent_id: None,
                        client_order_id: None,
                        topic: None,
                    },
                    descending: false,
                });
                let cursor = page.next_cursor.next_seq;
                let items = page
                    .events
                    .iter()
                    .filter_map(|event| account_stream_payload_for_event(&service, event))
                    .collect::<Vec<_>>();
                (items, cursor)
            };

            if !items.is_empty() {
                for (name, seq, payload) in &items {
                    write_sse_json_event(stream, name, Some(*seq), payload)?;
                }
                next_seq = cursor;
                write_sse_cursor(stream, next_seq)?;
                if !params.follow {
                    break;
                }
                continue;
            }

            next_seq = cursor;
            if !params.follow || started_at.elapsed() >= timeout {
                write_sse_cursor(stream, next_seq)?;
                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }

    fn handle_wallet_stream_connection(
        &self,
        stream: &mut TcpStream,
        request: &HttpRequest,
    ) -> Result<(), TransportError> {
        let params = parse_wallet_stream_request(request.query.as_deref()).map_err(|message| {
            TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                message,
            ))
        })?;
        write_sse_headers(stream, &request.headers)?;

        let mut next_seq = params.from_seq;
        if !params.from_seq_explicit {
            if let Some(last_event_id) = request.headers.get("last-event-id") {
                if let Ok(resume_from) = last_event_id.parse::<u64>() {
                    next_seq = resume_from.saturating_add(1);
                }
            }
        }

        let mut last_snapshot = {
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| TransportError::Read(std::io::Error::other("engine lock poisoned")))?;
            let service = NodeRpcService::new(&mut engine);
            service.get_wallet_balances(params.address)
        };
        next_seq = next_seq.max(last_snapshot.next_seq);
        write_sse_json_event(
            stream,
            "wallet_balances",
            None,
            &wallet_balances_response_to_json(&last_snapshot),
        )?;

        let started_at = Instant::now();
        let timeout = Duration::from_millis(params.timeout_ms.min(60_000));
        let mut emitted_count = 0_usize;
        loop {
            let snapshot = {
                let mut engine = self.engine.lock().map_err(|_| {
                    TransportError::Read(std::io::Error::other("engine lock poisoned"))
                })?;
                let service = NodeRpcService::new(&mut engine);
                service.get_wallet_balances(params.address)
            };
            let cursor = next_seq.max(snapshot.next_seq);

            if wallet_balances_changed(&last_snapshot, &snapshot) {
                let event_seq = cursor;
                next_seq = event_seq.saturating_add(1);
                emitted_count = emitted_count.saturating_add(1);
                write_sse_json_event(
                    stream,
                    "wallet_balances",
                    Some(event_seq),
                    &wallet_balances_stream_payload(&snapshot, event_seq),
                )?;
                write_sse_cursor(stream, next_seq)?;
                last_snapshot = snapshot;
                if !params.follow || emitted_count >= params.limit {
                    break;
                }
                continue;
            }

            next_seq = cursor;
            if !params.follow || started_at.elapsed() >= timeout {
                write_sse_cursor(stream, next_seq)?;
                break;
            }

            thread::sleep(Duration::from_millis(100));
        }

        Ok(())
    }
}
