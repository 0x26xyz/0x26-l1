use crate::*;
use reqwest::Url;
use std::cmp::Reverse;
use std::collections::{btree_map::Entry, BTreeMap};
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

impl JsonRpcHttpServer {
    pub(super) fn retain_recent_timeout_rounds(
        pool: &mut TimeoutMessagePool,
        keep_from_round: u64,
    ) {
        pool.rounds.retain(|round, _| *round >= keep_from_round);
    }

    pub(super) fn store_timeout_message(
        &self,
        timeout: TimeoutMessage,
        validator_verifiers: &[ValidatorVerifier],
    ) -> Result<usize, JsonRpcErrorObject> {
        let local_chain_id = self
            .engine
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?
            .chain_id();
        if timeout.chain_id != local_chain_id {
            return Err(JsonRpcErrorObject {
                code: -32602,
                message: format!(
                    "timeout payload chain_id {} does not match local chain_id {}",
                    timeout.chain_id, local_chain_id
                ),
                data: None,
            });
        }
        validate_timeout_message(&timeout, validator_verifiers).map_err(|error| {
            JsonRpcErrorObject {
                code: -32602,
                message: format!("{error:?}"),
                data: None,
            }
        })?;
        let round = timeout.round;
        let sender = timeout.sender;
        let mut pool = self
            .timeout_message_pool
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "timeout message pool lock poisoned".into(),
                data: None,
            })?;
        Self::retain_recent_timeout_rounds(&mut pool, round.saturating_sub(4));
        let round_pool = pool.rounds.entry(round).or_default();
        round_pool.insert(sender, timeout);
        Ok(round_pool.len())
    }

    pub(super) fn timeout_messages_for_round(
        &self,
        round: u64,
    ) -> Result<Vec<TimeoutMessage>, JsonRpcErrorObject> {
        let pool = self
            .timeout_message_pool
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "timeout message pool lock poisoned".into(),
                data: None,
            })?;
        Ok(pool
            .rounds
            .get(&round)
            .map(|messages| messages.values().cloned().collect())
            .unwrap_or_default())
    }

    pub(super) fn local_timeout_for_round(
        &self,
        round: u64,
        validator_verifiers: &[ValidatorVerifier],
    ) -> Result<Option<TimeoutMessage>, JsonRpcErrorObject> {
        let local_validator_id = {
            let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            engine.local_validator_id()
        };
        let Some(local_validator_id) = local_validator_id else {
            return Ok(None);
        };

        if let Some(timeout) = self
            .timeout_message_pool
            .lock()
            .map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "timeout message pool lock poisoned".into(),
                data: None,
            })?
            .rounds
            .get(&round)
            .and_then(|messages| messages.get(&local_validator_id))
            .cloned()
        {
            return Ok(Some(timeout));
        }

        let timeout = {
            let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
                code: -32000,
                message: "engine lock poisoned".into(),
                data: None,
            })?;
            if engine.current_round() != round {
                return Ok(None);
            }
            match engine.sign_timeout_message(validator_verifiers) {
                Ok(timeout) => timeout,
                Err(NodeError::InvalidBlock(
                    kombat_chain_consensus::BlockValidationError::InsufficientQuorum
                    | kombat_chain_consensus::BlockValidationError::InvalidQuorumCertificate,
                )) => return Ok(None),
                Err(other) => {
                    return Err(JsonRpcErrorObject {
                        code: -32000,
                        message: format!("{other:?}"),
                        data: None,
                    })
                }
            }
        };
        self.store_timeout_message(timeout.clone(), validator_verifiers)?;
        Ok(Some(timeout))
    }

    pub(super) fn network_validator_rpc_endpoints(
        &self,
        validator_set: &ValidatorSet,
        validator_id: ValidatorId,
    ) -> Result<Vec<String>, JsonRpcErrorObject> {
        let endpoints = validator_set
            .active_validators()
            .into_iter()
            .find(|validator| validator.validator_id == validator_id)
            .map(|validator| validator.rpc_endpoints)
            .ok_or(JsonRpcErrorObject {
                code: -32000,
                message: format!(
                    "validator {} is missing from validator set",
                    encode_hex(&validator_id)
                ),
                data: None,
            })?;

        if endpoints.is_empty() {
            return Err(JsonRpcErrorObject {
                code: -32000,
                message: format!(
                    "validator {} has no rpc endpoints",
                    encode_hex(&validator_id)
                ),
                data: None,
            });
        }

        Ok(endpoints)
    }

    pub(super) fn fetch_network_bootstrap_metadata(
        &self,
        endpoints: &[String],
    ) -> Result<NetworkBootstrapMetadataResponse, String> {
        let response = self
            .send_network_request(endpoints, |client, endpoint| {
                let request = client.get(network_peer_url(endpoint, "bootstrap"));
                if let Some(token) = self.config.operator_bearer_token.as_deref() {
                    request.bearer_auth(token)
                } else {
                    request
                }
            })
            .map_err(|error| format!("failed to fetch network bootstrap metadata: {error}"))?;
        let response = response
            .error_for_status()
            .map_err(|error| format!("network bootstrap request failed: {error}"))?;
        response
            .json::<NetworkBootstrapMetadataResponse>()
            .map_err(|error| format!("failed to decode network bootstrap metadata: {error}"))
    }

    pub(super) fn resolve_forced_repair_sources(
        &self,
        network_bootstrap: &NetworkBootstrapConfig,
        preferred_validator_id: ValidatorId,
        minimum_head_height: Option<u64>,
        expected_head_hash: Option<Hash>,
    ) -> Result<Vec<(ValidatorId, Vec<String>)>, String> {
        #[derive(Clone)]
        struct RepairSourceCandidate {
            validator_id: ValidatorId,
            endpoints: Vec<String>,
            metadata: Option<NetworkBootstrapMetadataResponse>,
        }

        let preferred_endpoints = self
            .network_validator_rpc_endpoints(
                &network_bootstrap.validator_set,
                preferred_validator_id,
            )
            .map_err(|error| error.message)?;
        let expected_head_hash_hex = expected_head_hash.map(|hash| encode_hex(&hash));
        let Some(local_validator_id) = network_bootstrap.local_validator_id() else {
            return Ok(vec![(preferred_validator_id, preferred_endpoints)]);
        };
        let mut candidates = network_bootstrap
            .validator_set
            .active_validators()
            .iter()
            .filter(|validator| validator.validator_id != local_validator_id)
            .filter(|validator| !validator.rpc_endpoints.is_empty())
            .map(|validator| RepairSourceCandidate {
                validator_id: validator.validator_id,
                endpoints: validator.rpc_endpoints.clone(),
                metadata: self
                    .fetch_network_bootstrap_metadata(&validator.rpc_endpoints)
                    .ok(),
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|candidate| {
            let metadata = candidate.metadata.as_ref();
            let matches_expected = expected_head_hash_hex.as_ref().is_some_and(|expected| {
                metadata.is_some_and(|metadata| &metadata.head_hash_hex == expected)
            });
            let meets_minimum = minimum_head_height.is_none_or(|minimum| {
                metadata.is_some_and(|metadata| metadata.head_height >= minimum)
            });
            let head_height = metadata
                .map(|metadata| metadata.head_height)
                .unwrap_or_default();
            (
                Reverse(matches_expected),
                Reverse(meets_minimum),
                Reverse(head_height),
                Reverse(candidate.validator_id == preferred_validator_id),
            )
        });

        let mut ordered = candidates
            .into_iter()
            .map(|candidate| (candidate.validator_id, candidate.endpoints))
            .collect::<Vec<_>>();
        if !ordered
            .iter()
            .any(|(validator_id, _)| *validator_id == preferred_validator_id)
        {
            ordered.push((preferred_validator_id, preferred_endpoints));
        }
        Ok(ordered)
    }

    pub(super) fn maybe_force_repair_from_validator_snapshot(
        &self,
        validator_id: ValidatorId,
        minimum_head_height: Option<u64>,
        required_block: Option<(u64, Hash)>,
        expected_head_hash: Option<Hash>,
        reason: &str,
    ) -> Result<bool, String> {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Err("network bootstrap not configured".into());
        };
        let now = Instant::now();
        let (local_head_height, local_head_hash, local_has_required_block) = {
            let engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            let local_has_required_block = required_block.is_some_and(|(height, expected_hash)| {
                engine
                    .blocks()
                    .get(height as usize)
                    .is_some_and(|block| block_hash(block) == expected_hash)
            });
            (
                engine.head().header.height,
                block_hash(engine.head()),
                local_has_required_block,
            )
        };
        let local_matches_expected_head =
            expected_head_hash.is_none_or(|expected_hash| local_head_hash == expected_hash);
        if minimum_head_height.is_some_and(|height| height <= local_head_height)
            && (required_block.is_none() || local_has_required_block)
            && local_matches_expected_head
        {
            return Ok(false);
        }
        let repair_sources = self.resolve_forced_repair_sources(
            network_bootstrap,
            validator_id,
            minimum_head_height,
            expected_head_hash,
        )?;
        if let Ok(mut repair_state) = self.peer_snapshot_repair_state.lock() {
            repair_state.attempts_total = repair_state.attempts_total.saturating_add(1);
            repair_state.last_error = None;
        }
        let record_error = |message: String| {
            if let Ok(mut repair_state) = self.peer_snapshot_repair_state.lock() {
                repair_state.repair_err_total = repair_state.repair_err_total.saturating_add(1);
                repair_state.last_error = Some(message.clone());
            }
            message
        };
        let (local_validator_id, local_signing_key) = {
            let engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            (
                engine
                    .local_validator_id()
                    .ok_or_else(|| "local validator id unavailable".to_string())?,
                engine
                    .local_consensus_signing_key()
                    .ok_or_else(|| "local validator signing key unavailable".to_string())?,
            )
        };
        let mut restored_bundle = None;
        let mut last_error = None;
        for (repair_validator_id, endpoints) in repair_sources {
            let snapshot_bytes = match self.fetch_network_snapshot_bytes(&endpoints) {
                Ok(snapshot_bytes) => snapshot_bytes,
                Err(error) => {
                    last_error = Some(record_error(format!(
                        "forced peer snapshot repair failed to fetch validator {} snapshot for {reason}: {error}",
                        encode_hex(&repair_validator_id)
                    )));
                    continue;
                }
            };
            let mut restored = match SingleNodeEngine::from_snapshot_bytes(&snapshot_bytes) {
                Ok(restored) => restored,
                Err(error) => {
                    last_error = Some(record_error(format!(
                        "forced peer snapshot repair failed to decode validator {} snapshot for {reason}: {error}",
                        encode_hex(&repair_validator_id)
                    )));
                    continue;
                }
            };
            if let Err(error) =
                restored.rebind_local_validator(local_validator_id, local_signing_key)
            {
                last_error = Some(record_error(format!(
                    "forced peer snapshot repair failed to rebind validator {} snapshot for {reason}: {error}",
                    encode_hex(&repair_validator_id)
                )));
                continue;
            }
            let restored_head_height = restored.head().header.height;
            let restored_head_hash = block_hash(restored.head());
            let restored_head_hash_hex = encode_hex(&restored_head_hash);
            let restored_has_required_block =
                required_block.is_some_and(|(height, expected_hash)| {
                    restored
                        .blocks()
                        .get(height as usize)
                        .is_some_and(|block| block_hash(block) == expected_hash)
                });
            let restored_matches_expected_head =
                expected_head_hash.is_none_or(|expected_hash| restored_head_hash == expected_hash);
            let repair_wrong_lineage = (required_block.is_some()
                && !local_has_required_block
                && restored_has_required_block)
                || (expected_head_hash.is_some()
                    && !local_matches_expected_head
                    && restored_matches_expected_head);
            if restored_head_height <= local_head_height && !repair_wrong_lineage {
                continue;
            }
            if let Some(minimum_head_height) = minimum_head_height {
                if restored_head_height < minimum_head_height {
                    last_error = Some(record_error(format!(
                        "forced peer snapshot repair from validator {} for {reason} restored height {} below required {}",
                        encode_hex(&repair_validator_id),
                        restored_head_height,
                        minimum_head_height
                    )));
                    continue;
                }
            }
            restored_bundle = Some((
                repair_validator_id,
                restored,
                restored_head_height,
                restored_head_hash_hex,
                repair_wrong_lineage,
            ));
            break;
        }
        let Some((
            repair_validator_id,
            restored,
            restored_head_height,
            restored_head_hash_hex,
            repair_wrong_lineage,
        )) = restored_bundle
        else {
            return Err(last_error.unwrap_or_else(|| {
                "forced peer snapshot repair could not find a usable validator snapshot".into()
            }));
        };

        let _network_control = self
            .network_control
            .lock()
            .map_err(|_| "network control lock poisoned".to_string())?;
        {
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            if restored_head_height <= engine.head().header.height && !repair_wrong_lineage {
                return Ok(false);
            }
            let restart_boundary_round = restored
                .head()
                .header
                .round
                .saturating_add(1)
                .max(restored.current_round())
                .max(1);
            *engine = restored;
            drop(engine);
            self.reset_mahi_state_after_engine_repair(restart_boundary_round);
        }
        self.clear_local_submit_backlog();
        self.reset_peer_transport_session_cache();
        if let Ok(mut repair_state) = self.peer_snapshot_repair_state.lock() {
            repair_state.repaired_total = repair_state.repaired_total.saturating_add(1);
            repair_state.last_error = None;
            repair_state.last_repaired_head_height = Some(restored_head_height);
            repair_state.last_repaired_head_hash_hex = Some(restored_head_hash_hex.clone());
            repair_state.stalled_head_height = Some(restored_head_height);
            repair_state.stalled_head_hash_hex = Some(restored_head_hash_hex.clone());
            repair_state.stalled_since = Some(now);
        }
        println!(
            "kombat_chain forced validator snapshot repair reason={} validator={} height={} hash={}",
            reason,
            encode_hex(&repair_validator_id),
            restored_head_height,
            restored_head_hash_hex,
        );
        Ok(true)
    }

    pub(super) fn send_network_request<F>(
        &self,
        endpoints: &[String],
        mut build: F,
    ) -> Result<reqwest::blocking::Response, String>
    where
        F: FnMut(&Client, &str) -> reqwest::blocking::RequestBuilder,
    {
        if endpoints.is_empty() {
            return Err("validator has no rpc endpoints".into());
        }

        let mut errors = Vec::with_capacity(endpoints.len());
        for endpoint in endpoints {
            match build(self.network_http_client.as_ref(), endpoint).send() {
                Ok(response) => return Ok(response),
                Err(error) => errors.push(format!("{endpoint}: {error}")),
            }
        }

        Err(errors.join("; "))
    }

    pub(crate) fn send_network_json_request<Request, Response>(
        &self,
        endpoints: &[String],
        path: &str,
        request: &Request,
    ) -> Result<Response, NetworkRequestError>
    where
        Request: Serialize,
        Response: for<'de> Deserialize<'de>,
    {
        let body = serde_json::to_string(request).map_err(|error| {
            NetworkRequestError::Rejected(format!("serialize network request failed: {error}"))
        })?;

        if self.config.peer_transport_enabled {
            return self.send_network_peer_transport_request(endpoints, path, &body);
        }

        self.send_network_http_json_request(endpoints, path, &body)
    }

    pub(super) fn collect_network_json_request_results<Response>(
        &self,
        peers: &[ValidatorPeer],
        path: &'static str,
        body: String,
        required_successes: usize,
    ) -> Vec<(ValidatorId, Result<Response, NetworkRequestError>)>
    where
        Response: DeserializeOwned + Send + 'static,
    {
        let mut collected = Vec::new();
        let mut offset = 0;
        let target = required_successes.min(peers.len());
        let mut successes = 0;

        while successes < target && offset < peers.len() {
            let batch_size = (target - successes).min(peers.len().saturating_sub(offset));
            let batch = &peers[offset..offset + batch_size];
            let (tx, rx) = mpsc::channel();

            let mut batch_results = thread::scope(|scope| {
                for peer in batch {
                    let tx = tx.clone();
                    let validator_id = peer.validator_id;
                    let endpoints = peer.rpc_endpoints.clone();
                    let body = body.clone();
                    scope.spawn(move || {
                        let result = if self.config.peer_transport_enabled {
                            self.send_network_peer_transport_request(&endpoints, path, &body)
                        } else {
                            self.send_network_http_json_request(&endpoints, path, &body)
                        };
                        let _ = tx.send((validator_id, result));
                    });
                }
                drop(tx);

                let mut results_by_validator = BTreeMap::new();
                while let Ok((validator_id, result)) = rx.recv() {
                    results_by_validator.insert(validator_id, result);
                    if results_by_validator.len() >= batch.len() {
                        break;
                    }
                }

                batch
                    .iter()
                    .filter_map(|peer| {
                        results_by_validator
                            .remove(&peer.validator_id)
                            .map(|result| (peer.validator_id, result))
                    })
                    .collect::<Vec<_>>()
            });
            successes += batch_results
                .iter()
                .filter(|(_, result)| result.is_ok())
                .count();
            collected.append(&mut batch_results);
            offset += batch_size;
        }

        collected
    }

    pub(super) fn send_network_http_json_request<Response>(
        &self,
        endpoints: &[String],
        path: &str,
        body: &str,
    ) -> Result<Response, NetworkRequestError>
    where
        Response: for<'de> Deserialize<'de>,
    {
        let response = self
            .send_network_request(endpoints, |client, endpoint| {
                let mut request = client.post(network_peer_url(endpoint, path));
                if let Some(token) = self.network_bearer_token_for_path(path) {
                    request = request.bearer_auth(token);
                }
                request
                    .header("content-type", "application/json")
                    .body(body.to_owned())
            })
            .map_err(NetworkRequestError::Unreachable)?;
        if !response.status().is_success() {
            let status = response.status();
            let body_text = response
                .text()
                .unwrap_or_else(|error| format!("<failed to read error body: {error}>"));
            let trimmed_body = body_text.trim();
            let detail = if trimmed_body.is_empty() {
                String::new()
            } else {
                format!(" body={trimmed_body}")
            };
            return Err(NetworkRequestError::Rejected(format!(
                "network request failed: status={status}{detail}"
            )));
        }
        response.json::<Response>().map_err(|error| {
            NetworkRequestError::Rejected(format!("network response decode failed: {error}"))
        })
    }

    pub(super) fn send_network_peer_transport_request<Response>(
        &self,
        endpoints: &[String],
        path: &str,
        body: &str,
    ) -> Result<Response, NetworkRequestError>
    where
        Response: for<'de> Deserialize<'de>,
    {
        if endpoints.is_empty() {
            return Err(NetworkRequestError::Unreachable(
                "validator has no rpc endpoints".into(),
            ));
        }

        let mut unreachable_errors = Vec::with_capacity(endpoints.len());
        let mut rejected_errors = Vec::new();
        let mut headers = BTreeMap::new();
        if let Some(token) = self.network_bearer_token_for_path(path) {
            headers.insert("authorization".into(), format!("Bearer {token}"));
        }
        let request = PeerTransportRequest {
            path: format!("/network/{path}"),
            headers,
            body: body.to_owned(),
        };

        for endpoint in endpoints {
            match self.send_peer_transport_frame(endpoint, &request) {
                Ok(response) => {
                    if let Some(error) = response.error {
                        rejected_errors.push(format!("{endpoint}: {}", error.message));
                        continue;
                    }
                    let body = response.body.ok_or_else(|| {
                        NetworkRequestError::Rejected(format!(
                            "{endpoint}: peer transport response missing body"
                        ))
                    })?;
                    match serde_json::from_str::<Response>(&body) {
                        Ok(decoded) => return Ok(decoded),
                        Err(error) => rejected_errors.push(format!(
                            "{endpoint}: peer transport response decode failed: {error}"
                        )),
                    }
                }
                Err(error) => unreachable_errors.push(format!("{endpoint}: {error}")),
            }
        }

        if !rejected_errors.is_empty() {
            if !unreachable_errors.is_empty() {
                rejected_errors.extend(unreachable_errors);
            }
            return Err(NetworkRequestError::Rejected(rejected_errors.join("; ")));
        }

        Err(NetworkRequestError::Unreachable(
            unreachable_errors.join("; "),
        ))
    }

    pub(super) fn network_bearer_token_for_path(&self, path: &str) -> Option<&str> {
        if matches!(
            path,
            "relay-delegation-actions"
                | "relay-evm-delegation-actions"
                | "relay-market-maker-program-actions"
        ) {
            return self
                .config
                .peer_relay_bearer_token
                .as_deref()
                .or(self.config.operator_bearer_token.as_deref());
        }
        self.config.operator_bearer_token.as_deref()
    }

    pub(super) fn send_network_ping_request(
        &self,
        endpoints: &[String],
    ) -> Result<NetworkPingResponse, NetworkRequestError> {
        if self.config.peer_transport_enabled {
            return self.send_network_peer_transport_request(endpoints, "ping", "");
        }

        let response = self
            .send_network_request(endpoints, |client, endpoint| {
                client.get(network_peer_url(endpoint, "ping"))
            })
            .map_err(NetworkRequestError::Unreachable)?;
        let response = response.error_for_status().map_err(|error| {
            NetworkRequestError::Rejected(format!("network request failed: {error}"))
        })?;
        response.json::<NetworkPingResponse>().map_err(|error| {
            NetworkRequestError::Rejected(format!("network response decode failed: {error}"))
        })
    }

    pub(super) fn send_network_session_heartbeat_request(
        &self,
        endpoints: &[String],
    ) -> Result<PeerTransportSessionHeartbeatResponse, NetworkRequestError> {
        let (local_chain_id, local_validator_id_hex, local_metadata) =
            self.local_peer_transport_session_metadata();
        let body = serde_json::to_string(&PeerTransportSessionHeartbeatRequest {
            chain_id: local_chain_id,
            validator_id_hex: local_validator_id_hex,
            current_round: local_metadata.remote_current_round,
            current_view: local_metadata.remote_current_view,
            head_height: local_metadata.remote_head_height,
            head_hash_hex: local_metadata.remote_head_hash_hex,
            committed_state_root_hex: local_metadata.remote_committed_state_root_hex,
            last_committed_round: local_metadata.remote_last_committed_round,
            peer_count: local_metadata.remote_peer_count,
        })
        .map_err(|error| {
            NetworkRequestError::Rejected(format!(
                "serialize peer transport heartbeat failed: {error}"
            ))
        })?;
        self.send_network_peer_transport_request(endpoints, "session/heartbeat", &body)
    }

    pub(super) fn send_peer_transport_frame(
        &self,
        endpoint: &str,
        request: &PeerTransportRequest,
    ) -> Result<PeerTransportResponse, String> {
        if let Some(backoff_error) = self.peer_transport_endpoint_backoff_error(endpoint) {
            return Err(backoff_error);
        }

        let mut session = match {
            let mut cached = self
                .network_peer_streams
                .lock()
                .map_err(|_| "peer transport cache lock poisoned".to_string())?;
            cached.remove(endpoint)
        }
        .map(Ok)
        .unwrap_or_else(|| self.connect_peer_transport_stream(endpoint))
        {
            Ok(session) => session,
            Err(error) => {
                self.record_peer_transport_endpoint_failure(endpoint, &error);
                return Err(error);
            }
        };

        if session.last_used_at.elapsed() > PEER_TRANSPORT_SESSION_IDLE_TIMEOUT {
            self.close_peer_transport_session(&mut session);
            session = match self.connect_peer_transport_stream(endpoint) {
                Ok(session) => session,
                Err(error) => {
                    self.record_peer_transport_endpoint_failure(endpoint, &error);
                    return Err(error);
                }
            };
        }

        let result = write_peer_transport_request(&mut session.stream, request)
            .and_then(|()| read_peer_transport_response(&mut session.stream))
            .map_err(|error| format!("{error:?}"));

        match result {
            Ok(response) => {
                session.last_used_at = Instant::now();
                session.request_count = session.request_count.saturating_add(1);
                if request.path == "/network/session/heartbeat" {
                    if let Some(body) = response.body.as_deref() {
                        if let Ok(heartbeat) =
                            serde_json::from_str::<PeerTransportSessionHeartbeatResponse>(body)
                        {
                            session.remote_session_id = heartbeat.session_id;
                            self.apply_outbound_peer_transport_session_metadata(
                                &mut session,
                                &PeerTransportSessionMetadata {
                                    remote_chain_id: heartbeat.chain_id,
                                    remote_validator_id_hex: heartbeat.validator_id_hex,
                                    remote_current_round: heartbeat.current_round,
                                    remote_current_view: heartbeat.current_view,
                                    remote_head_height: heartbeat.head_height,
                                    remote_head_hash_hex: heartbeat.head_hash_hex,
                                    remote_committed_state_root_hex: heartbeat
                                        .committed_state_root_hex,
                                    remote_last_committed_round: heartbeat.last_committed_round,
                                    remote_peer_count: heartbeat.peer_count,
                                },
                            );
                        }
                    }
                }
                self.clear_peer_transport_endpoint_failure(endpoint);
                let mut cached = self
                    .network_peer_streams
                    .lock()
                    .map_err(|_| "peer transport cache lock poisoned".to_string())?;
                cached.insert(endpoint.to_owned(), session);
                Ok(response)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn close_peer_transport_session(&self, session: &mut PeerTransportSession) {
        let request = PeerTransportRequest {
            path: "/network/session/close".into(),
            headers: BTreeMap::new(),
            body: String::new(),
        };
        let _ = write_peer_transport_request(&mut session.stream, &request)
            .and_then(|()| read_peer_transport_response(&mut session.stream));
    }

    pub(super) fn local_peer_transport_session_metadata(
        &self,
    ) -> (Option<u64>, Option<String>, PeerTransportSessionMetadata) {
        let current_round = self.network_round_snapshot();
        self.engine
            .lock()
            .map(|engine| {
                let metrics = engine.metrics();
                let chain_id = Some(engine.chain_id());
                let validator_id_hex = engine.local_validator_id().map(|id| encode_hex(&id));
                let metadata = PeerTransportSessionMetadata {
                    remote_chain_id: chain_id,
                    remote_validator_id_hex: validator_id_hex.clone(),
                    remote_current_round: Some(current_round),
                    remote_current_view: Some(metrics.current_view),
                    remote_head_height: Some(engine.head().header.height),
                    remote_head_hash_hex: Some(encode_hex(&kombat_chain_consensus::block_hash(
                        engine.head(),
                    ))),
                    remote_committed_state_root_hex: Some(encode_hex(
                        &engine.committed_state_root(),
                    )),
                    remote_last_committed_round: Some(metrics.last_committed_round),
                    remote_peer_count: Some(metrics.peer_count),
                };
                (chain_id, validator_id_hex, metadata)
            })
            .unwrap_or((None, None, PeerTransportSessionMetadata::default()))
    }

    pub(crate) fn network_round_snapshot(&self) -> u64 {
        if self.config.mahi_enabled {
            if let Ok(state_guard) = self.mahi_state.lock() {
                if let Some(state) = state_guard.as_ref() {
                    return state.threshold_clock.round();
                }
            }
        }
        self.engine.current_round_snapshot()
    }

    pub(super) fn network_submit_leader_snapshot(
        &self,
        chain_id: u64,
        validator_set: &ValidatorSet,
    ) -> Result<(u64, ValidatorId), NodeError> {
        if self.config.mahi_enabled {
            if let Ok(state_guard) = self.mahi_state.lock() {
                if let Some(state) = state_guard.as_ref() {
                    let round = state.threshold_clock.round();
                    return Ok((round, state.leader_schedule.leader_for_round(round)));
                }
            }
        }
        let round = self.engine.current_round_snapshot();
        let leader = select_leader_for_validator_set(round, chain_id, validator_set)?;
        Ok((round, leader))
    }

    pub(super) fn apply_outbound_peer_transport_session_metadata(
        &self,
        session: &mut PeerTransportSession,
        metadata: &PeerTransportSessionMetadata,
    ) {
        if let Some(remote_chain_id) = metadata.remote_chain_id {
            session.remote_chain_id = remote_chain_id;
        }
        if let Some(remote_validator_id_hex) = metadata.remote_validator_id_hex.clone() {
            session.remote_validator_id_hex = Some(remote_validator_id_hex);
        }
        if let Some(remote_current_round) = metadata.remote_current_round {
            session.remote_current_round = Some(remote_current_round);
        }
        if let Some(remote_current_view) = metadata.remote_current_view {
            session.remote_current_view = Some(remote_current_view);
        }
        if let Some(remote_head_height) = metadata.remote_head_height {
            session.remote_head_height = Some(remote_head_height);
        }
        if let Some(remote_head_hash_hex) = metadata.remote_head_hash_hex.clone() {
            session.remote_head_hash_hex = Some(remote_head_hash_hex);
        }
        if let Some(remote_committed_state_root_hex) =
            metadata.remote_committed_state_root_hex.clone()
        {
            session.remote_committed_state_root_hex = Some(remote_committed_state_root_hex);
        }
        if let Some(remote_last_committed_round) = metadata.remote_last_committed_round {
            session.remote_last_committed_round = Some(remote_last_committed_round);
        }
        if let Some(remote_peer_count) = metadata.remote_peer_count {
            session.remote_peer_count = Some(remote_peer_count);
        }
    }

    pub(super) fn connect_peer_transport_stream(
        &self,
        endpoint: &str,
    ) -> Result<PeerTransportSession, String> {
        let url = Url::parse(endpoint).map_err(|error| format!("invalid endpoint: {error}"))?;
        let host = url
            .host_str()
            .ok_or_else(|| "endpoint missing host".to_string())?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| "endpoint missing port".to_string())?;
        let address = format!("{host}:{port}");
        let stream = TcpStream::connect(&address)
            .map_err(|error| format!("connect {address} failed: {error}"))?;
        stream
            .set_nodelay(true)
            .map_err(|error| format!("set_nodelay failed: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| format!("set_read_timeout failed: {error}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| format!("set_write_timeout failed: {error}"))?;
        let mut stream = stream;
        stream
            .write_all(PEER_TRANSPORT_MAGIC)
            .map_err(|error| format!("write peer transport magic failed: {error}"))?;
        let (local_chain_id, local_validator_id_hex, local_metadata) =
            self.local_peer_transport_session_metadata();
        let session_open_response = {
            let request = PeerTransportRequest {
                path: "/network/session/open".into(),
                headers: BTreeMap::new(),
                body: serde_json::to_string(&PeerTransportSessionOpenRequest {
                    chain_id: local_chain_id,
                    validator_id_hex: local_validator_id_hex,
                    current_round: local_metadata.remote_current_round,
                    current_view: local_metadata.remote_current_view,
                    head_height: local_metadata.remote_head_height,
                    head_hash_hex: local_metadata.remote_head_hash_hex,
                    committed_state_root_hex: local_metadata.remote_committed_state_root_hex,
                    last_committed_round: local_metadata.remote_last_committed_round,
                    peer_count: local_metadata.remote_peer_count,
                })
                .map_err(|error| format!("peer transport session open encode failed: {error}"))?,
            };
            write_peer_transport_request(&mut stream, &request)
                .and_then(|()| read_peer_transport_response(&mut stream))
                .map_err(|error| format!("{error:?}"))?
        };
        if let Some(error) = session_open_response.error {
            return Err(format!(
                "peer transport session open failed: {}",
                error.message
            ));
        }
        let open_body = session_open_response
            .body
            .ok_or_else(|| "peer transport session open response missing body".to_string())?;
        let open: PeerTransportSessionOpenResponse = serde_json::from_str(&open_body)
            .map_err(|error| format!("peer transport session open decode failed: {error}"))?;
        let now = Instant::now();
        Ok(PeerTransportSession {
            stream,
            connected_at: now,
            last_used_at: now,
            request_count: 0,
            remote_session_id: open.session_id,
            remote_chain_id: open.chain_id,
            remote_validator_id_hex: open.validator_id_hex,
            remote_current_round: open.current_round,
            remote_current_view: open.current_view,
            remote_head_height: open.head_height,
            remote_head_hash_hex: open.head_hash_hex,
            remote_committed_state_root_hex: open.committed_state_root_hex,
            remote_last_committed_round: open.last_committed_round,
            remote_peer_count: open.peer_count,
        })
    }

    pub(super) fn network_peer_reachability(&self) -> Option<NetworkPeerReachability> {
        let network_bootstrap = self.config.network_bootstrap.as_ref()?;
        let mut reachable_peer_count = 0_usize;
        let mut unreachable_peer_urls = Vec::new();

        for peer in &network_bootstrap.bootstrap_peers.peers {
            match self.send_network_ping_request(&peer.rpc_endpoints) {
                Ok(_) => {
                    reachable_peer_count = reachable_peer_count.saturating_add(1);
                }
                Err(_) => unreachable_peer_urls.extend(peer.rpc_endpoints.iter().cloned()),
            }
        }

        Some(NetworkPeerReachability {
            reachable_peer_count,
            unreachable_peer_urls,
        })
    }

    pub fn maintain_peer_transport_sessions(&self) -> usize {
        if !self.config.peer_transport_enabled {
            return 0;
        }
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return 0;
        };

        let (_, _, local_metadata) = self.local_peer_transport_session_metadata();
        let repair_enabled = local_metadata.remote_head_height.is_some()
            && local_metadata.remote_head_hash_hex.is_some();
        if repair_enabled {
            self.observe_peer_snapshot_repair_local_head(&local_metadata);
        }
        let mut warmed_peers = 0_usize;
        let mut repair_candidate = None;
        let mut repair_observations = Vec::new();
        for peer in &network_bootstrap.bootstrap_peers.peers {
            if let Ok(heartbeat) = self.send_network_session_heartbeat_request(&peer.rpc_endpoints)
            {
                warmed_peers = warmed_peers.saturating_add(1);
                if repair_enabled {
                    if let Some(candidate) = peer_snapshot_repair_candidate(
                        &local_metadata,
                        &peer.rpc_endpoints,
                        &heartbeat,
                    ) {
                        if candidate.remote_head_height
                            >= local_metadata
                                .remote_head_height
                                .unwrap_or_default()
                                .saturating_add(PEER_SNAPSHOT_REPAIR_MIN_HEAD_GAP)
                        {
                            let should_replace = repair_candidate.as_ref().is_none_or(
                                |current: &PeerSnapshotRepairCandidate| {
                                    peer_snapshot_repair_candidate_is_better(&candidate, current)
                                },
                            );
                            if should_replace {
                                repair_candidate = Some(candidate.clone());
                            }
                        }
                        repair_observations.push(candidate);
                    }
                }
            }
        }
        if repair_candidate.is_none() && repair_enabled {
            repair_candidate = peer_snapshot_repair_authoritative_same_height_candidate(
                &local_metadata,
                &repair_observations,
                network_bootstrap.validator_set.active_validators().len(),
            );
        }
        if let Some(candidate) = repair_candidate {
            if let Err(error) = self.maybe_repair_from_peer_snapshot(&candidate, &local_metadata) {
                if let Ok(mut repair_state) = self.peer_snapshot_repair_state.lock() {
                    repair_state.repair_err_total = repair_state.repair_err_total.saturating_add(1);
                    repair_state.last_error = Some(error.clone());
                }
                eprintln!("kombat_chain peer snapshot repair failed: {error}");
            }
        }
        warmed_peers
    }

    pub(super) fn observe_peer_snapshot_repair_local_head(
        &self,
        local_metadata: &PeerTransportSessionMetadata,
    ) {
        let now = Instant::now();
        let Ok(mut repair_state) = self.peer_snapshot_repair_state.lock() else {
            return;
        };
        let local_head_height = local_metadata.remote_head_height;
        let local_head_hash_hex = local_metadata.remote_head_hash_hex.as_ref();
        let head_changed = repair_state.stalled_head_height != local_head_height
            || repair_state.stalled_head_hash_hex.as_ref() != local_head_hash_hex;
        if head_changed {
            repair_state.stalled_head_height = local_head_height;
            repair_state.stalled_head_hash_hex = local_head_hash_hex.cloned();
            repair_state.stalled_since = Some(now);
        } else if repair_state.stalled_since.is_none() {
            repair_state.stalled_since = Some(now);
        }
    }

    pub(super) fn maybe_repair_from_peer_snapshot(
        &self,
        candidate: &PeerSnapshotRepairCandidate,
        local_metadata: &PeerTransportSessionMetadata,
    ) -> Result<bool, String> {
        let now = Instant::now();
        {
            let mut repair_state = self
                .peer_snapshot_repair_state
                .lock()
                .map_err(|_| "peer snapshot repair state lock poisoned".to_string())?;
            if repair_state.next_attempt_at.is_some_and(|at| now < at) {
                return Ok(false);
            }
            let local_head_height = local_metadata
                .remote_head_height
                .ok_or_else(|| "local head height unavailable".to_string())?;
            let local_head_hash_hex = local_metadata
                .remote_head_hash_hex
                .as_deref()
                .ok_or_else(|| "local head hash unavailable".to_string())?;
            let candidate_head_hash_hex = candidate
                .remote_head_hash_hex
                .as_deref()
                .ok_or_else(|| "candidate head hash unavailable".to_string())?;
            let candidate_advances_height = candidate.remote_head_height > local_head_height;
            let candidate_authoritative_same_root = candidate.authoritative_same_height
                && candidate.remote_head_height >= local_head_height
                && candidate_head_hash_hex != local_head_hash_hex
                && candidate
                    .remote_committed_state_root_hex
                    .as_deref()
                    .zip(local_metadata.remote_committed_state_root_hex.as_deref())
                    .is_some_and(|(candidate_root, local_root)| candidate_root == local_root);
            if !candidate_advances_height && !candidate_authoritative_same_root {
                return Ok(false);
            }
            let local_is_observer = local_metadata.remote_validator_id_hex.is_none();
            let Some(stalled_since) = repair_state.stalled_since else {
                return Ok(false);
            };
            let materially_behind = candidate.remote_head_height
                >= local_head_height.saturating_add(PEER_SNAPSHOT_REPAIR_FORCE_HEAD_GAP);
            if !candidate_authoritative_same_root
                && !local_is_observer
                && !materially_behind
                && now.saturating_duration_since(stalled_since) < PEER_SNAPSHOT_REPAIR_STALL_TIMEOUT
            {
                return Ok(false);
            }
            let cooldown = if local_is_observer {
                PEER_SNAPSHOT_OBSERVER_REPAIR_COOLDOWN
            } else {
                PEER_SNAPSHOT_REPAIR_COOLDOWN
            };
            repair_state.next_attempt_at = Some(now + cooldown);
            repair_state.attempts_total = repair_state.attempts_total.saturating_add(1);
            repair_state.last_error = None;
        }

        let local_head_height = local_metadata
            .remote_head_height
            .ok_or_else(|| "local head height unavailable".to_string())?;
        let candidate_head_hash_hex = candidate
            .remote_head_hash_hex
            .as_deref()
            .ok_or_else(|| "candidate head hash unavailable".to_string())?;
        let local_validator_binding = {
            let engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            match engine.local_validator_id() {
                Some(local_validator_id) => Some((
                    local_validator_id,
                    engine
                        .local_consensus_signing_key()
                        .ok_or_else(|| "local validator signing key unavailable".to_string())?,
                )),
                None => None,
            }
        };

        let snapshot_bytes = self.fetch_network_snapshot_bytes(&candidate.endpoints)?;
        let mut restored = SingleNodeEngine::from_snapshot_bytes(&snapshot_bytes)
            .map_err(|error| format!("failed to load peer snapshot: {error}"))?;
        match local_validator_binding {
            Some((local_validator_id, local_signing_key)) => restored
                .rebind_local_validator(local_validator_id, local_signing_key)
                .map_err(|error| format!("failed to rebind peer snapshot: {error}"))?,
            None => restored.clear_local_validator_binding(),
        }
        let restored_head_height = restored.head().header.height;
        let restored_head_hash_hex = encode_hex(&block_hash(restored.head()));
        let restored_committed_state_root_hex = encode_hex(&restored.committed_state_root());
        if restored_head_hash_hex != candidate_head_hash_hex {
            return Err(format!(
                "peer snapshot head hash {} did not match advertised {}",
                restored_head_hash_hex, candidate_head_hash_hex
            ));
        }
        if candidate
            .remote_committed_state_root_hex
            .as_deref()
            .is_some_and(|root| root != restored_committed_state_root_hex)
        {
            return Err(format!(
                "peer snapshot committed root {} did not match advertised {}",
                restored_committed_state_root_hex,
                candidate
                    .remote_committed_state_root_hex
                    .as_deref()
                    .unwrap_or_default()
            ));
        }
        let repair_authoritative_same_height = candidate.authoritative_same_height
            && restored_head_height == local_head_height
            && candidate
                .remote_committed_state_root_hex
                .as_deref()
                .zip(local_metadata.remote_committed_state_root_hex.as_deref())
                .is_some_and(|(candidate_root, local_root)| candidate_root == local_root);
        if restored_head_height < local_head_height
            || (restored_head_height == local_head_height && !repair_authoritative_same_height)
        {
            return Ok(false);
        }

        let _network_control = self
            .network_control
            .lock()
            .map_err(|_| "network control lock poisoned".to_string())?;
        {
            let mut engine = self
                .engine
                .lock()
                .map_err(|_| "engine lock poisoned".to_string())?;
            let current_head_height = engine.head().header.height;
            let current_head_hash_hex = encode_hex(&block_hash(engine.head()));
            let current_committed_state_root_hex = encode_hex(&engine.committed_state_root());
            let still_repairs_authoritative_same_height = candidate.authoritative_same_height
                && restored_head_height == current_head_height
                && restored_head_hash_hex != current_head_hash_hex
                && candidate
                    .remote_committed_state_root_hex
                    .as_deref()
                    .is_some_and(|root| root == current_committed_state_root_hex);
            let still_repairs = restored_head_height > current_head_height
                || still_repairs_authoritative_same_height;
            if !still_repairs {
                return Ok(false);
            }
            let restart_boundary_round = restored
                .head()
                .header
                .round
                .saturating_add(1)
                .max(restored.current_round())
                .max(1);
            *engine = restored;
            drop(engine);
            self.reset_mahi_state_after_engine_repair(restart_boundary_round);
        }
        self.clear_local_submit_backlog();
        self.reset_peer_transport_session_cache();
        if let Ok(mut repair_state) = self.peer_snapshot_repair_state.lock() {
            repair_state.repaired_total = repair_state.repaired_total.saturating_add(1);
            repair_state.last_error = None;
            repair_state.last_repaired_head_height = Some(restored_head_height);
            repair_state.last_repaired_head_hash_hex = Some(restored_head_hash_hex.clone());
            repair_state.stalled_head_height = Some(restored_head_height);
            repair_state.stalled_head_hash_hex = Some(restored_head_hash_hex.clone());
            repair_state.stalled_since = Some(now);
        }
        println!(
            "kombat_chain repaired lagging validator from peer snapshot endpoints={} validator={} height={} hash={}",
            candidate.endpoints.join(","),
            candidate
                .remote_validator_id_hex
                .as_deref()
                .unwrap_or("unknown"),
            restored_head_height,
            candidate
                .remote_head_hash_hex
                .as_deref()
                .unwrap_or(&restored_head_hash_hex),
        );
        Ok(true)
    }

    pub(super) fn fetch_network_snapshot_bytes(
        &self,
        endpoints: &[String],
    ) -> Result<Vec<u8>, String> {
        let response = self
            .send_network_request(endpoints, |client, endpoint| {
                let request = client.get(network_peer_url(endpoint, "snapshot"));
                if let Some(token) = self.config.operator_bearer_token.as_deref() {
                    request.bearer_auth(token)
                } else {
                    request
                }
            })
            .map_err(|error| format!("failed to fetch peer snapshot: {error}"))?;
        let response = response
            .error_for_status()
            .map_err(|error| format!("peer snapshot request failed: {error}"))?;
        response
            .bytes()
            .map(|bytes| bytes.to_vec())
            .map_err(|error| format!("failed to read peer snapshot bytes: {error}"))
    }

    pub(super) fn reset_peer_transport_session_cache(&self) {
        let Ok(mut cached) = self.network_peer_streams.lock() else {
            return;
        };
        let sessions = std::mem::take(&mut *cached);
        drop(cached);
        for (_, mut session) in sessions {
            self.close_peer_transport_session(&mut session);
        }
        if let Ok(mut states) = self.peer_transport_endpoint_states.lock() {
            states.clear();
        }
    }

    pub(super) fn clear_local_submit_backlog(&self) {
        if let Ok(mut queue) = self.local_submit_queue.lock() {
            queue.items.clear();
        }
        if let Ok(mut pool) = self.mahi_pre_admitted_pool.lock() {
            pool.clear();
        }
        if let Ok(mut relays) = self.pending_intent_relays.lock() {
            relays.clear();
        }
        if let Ok(mut relays) = self.pending_mahi_owner_relays.lock() {
            relays.clear();
        }
        if let Ok(mut broadcasts) = self.pending_mahi_block_broadcasts.lock() {
            broadcasts.blocks.clear();
        }
    }

    pub(super) fn rpc_submit_metrics_snapshot(&self) -> RpcSubmitPathMetrics {
        self.rpc_submit_metrics
            .lock()
            .map(|metrics| metrics.clone())
            .unwrap_or_default()
    }

    pub(super) fn network_round_lock_profile_snapshot(&self) -> NetworkRoundLockProfileSnapshot {
        self.network_round_lock_profile
            .lock()
            .map(|metrics| metrics.clone())
            .unwrap_or_default()
    }

    pub(crate) fn submit_queue_stats_snapshot(&self) -> SubmitQueueStatsSnapshot {
        let local_submit_queue_items = self
            .local_submit_queue
            .lock()
            .map(|queue| queue.items.len())
            .unwrap_or(0);
        let pending_mahi_block_broadcasts = self
            .pending_mahi_block_broadcasts
            .lock()
            .map(|queue| queue.blocks.len())
            .unwrap_or(0);
        let mahi_pre_admitted = self
            .mahi_pre_admitted_pool
            .lock()
            .map(|pool| pool.stats())
            .unwrap_or_default();
        let legacy_relay = self
            .pending_intent_relays
            .lock()
            .map(|queue| {
                (
                    queue.intents.len(),
                    queue
                        .intents
                        .values()
                        .map(|relay| relay.pending_validators.len())
                        .sum::<usize>(),
                )
            })
            .unwrap_or_default();
        let mahi_owner_relay = self
            .pending_mahi_owner_relays
            .lock()
            .map(|queue| (queue.pending_intents(), queue.pending_targets()))
            .unwrap_or_default();
        SubmitQueueStatsSnapshot {
            local_submit_queue_items,
            pending_relay_intents: legacy_relay.0.saturating_add(mahi_owner_relay.0),
            pending_relay_targets: legacy_relay.1.saturating_add(mahi_owner_relay.1),
            pending_mahi_block_broadcasts,
            mahi_pre_admitted_pool_items: mahi_pre_admitted.items,
            mahi_pre_admitted_pool_accepted_total: mahi_pre_admitted.accepted_total,
            mahi_pre_admitted_pool_duplicate_total: mahi_pre_admitted.duplicate_total,
            mahi_pre_admitted_pool_drained_total: mahi_pre_admitted.drained_total,
            mahi_pre_admitted_pool_dropped_oldest_total: mahi_pre_admitted.dropped_oldest_total,
            mahi_pre_admitted_pool_dropped_stale_total: mahi_pre_admitted.dropped_stale_total,
        }
    }

    pub(super) fn peer_snapshot_repair_state_snapshot(&self) -> PeerSnapshotRepairStateSnapshot {
        let now = Instant::now();
        self.peer_snapshot_repair_state
            .lock()
            .map(|state| PeerSnapshotRepairStateSnapshot {
                stalled_head_height: state.stalled_head_height,
                stalled_head_hash_hex: state.stalled_head_hash_hex.clone(),
                stalled_for_ms: state.stalled_since.map(|started| {
                    now.saturating_duration_since(started)
                        .as_millis()
                        .min(u64::MAX as u128) as u64
                }),
                next_retry_in_ms: state.next_attempt_at.and_then(|retry_at| {
                    (retry_at > now).then(|| {
                        retry_at
                            .saturating_duration_since(now)
                            .as_millis()
                            .min(u64::MAX as u128) as u64
                    })
                }),
                attempts_total: state.attempts_total,
                repaired_total: state.repaired_total,
                repair_err_total: state.repair_err_total,
                last_error: state.last_error.clone(),
                last_repaired_head_height: state.last_repaired_head_height,
                last_repaired_head_hash_hex: state.last_repaired_head_hash_hex.clone(),
            })
            .unwrap_or_default()
    }

    pub(super) fn record_local_submit_metric(&self, duration: Duration, success: bool) {
        let Ok(mut metrics) = self.rpc_submit_metrics.lock() else {
            return;
        };
        metrics.local_submit_total = metrics.local_submit_total.saturating_add(1);
        if success {
            metrics.local_submit_ok = metrics.local_submit_ok.saturating_add(1);
        } else {
            metrics.local_submit_err = metrics.local_submit_err.saturating_add(1);
        }
        push_duration_sample(&mut metrics.local_submit_ms, duration);
    }

    pub(super) fn record_forwarded_submit_metric(
        &self,
        total: Duration,
        preflight: Duration,
        forward: Duration,
        timeout_recovery: Duration,
        timeout_recovery_attempts: u64,
        success: bool,
    ) {
        let Ok(mut metrics) = self.rpc_submit_metrics.lock() else {
            return;
        };
        metrics.forwarded_submit_total = metrics.forwarded_submit_total.saturating_add(1);
        if success {
            metrics.forwarded_submit_ok = metrics.forwarded_submit_ok.saturating_add(1);
        } else {
            metrics.forwarded_submit_err = metrics.forwarded_submit_err.saturating_add(1);
        }
        metrics.timeout_recovery_attempts_on_forwarded_submit = metrics
            .timeout_recovery_attempts_on_forwarded_submit
            .saturating_add(timeout_recovery_attempts);
        push_duration_sample(&mut metrics.forwarded_submit_ms, total);
        push_duration_sample(&mut metrics.leader_preflight_ms, preflight);
        push_duration_sample(&mut metrics.leader_forward_ms, forward);
        if !timeout_recovery.is_zero() {
            push_duration_sample(&mut metrics.forwarded_timeout_recovery_ms, timeout_recovery);
        }
    }

    pub(super) fn record_timeout_recovery_metric(&self, duration: Duration, success: bool) {
        let Ok(mut metrics) = self.rpc_submit_metrics.lock() else {
            return;
        };
        metrics.timeout_recovery_total = metrics.timeout_recovery_total.saturating_add(1);
        if success {
            metrics.timeout_recovery_ok = metrics.timeout_recovery_ok.saturating_add(1);
        } else {
            metrics.timeout_recovery_err = metrics.timeout_recovery_err.saturating_add(1);
        }
        push_duration_sample(&mut metrics.timeout_recovery_ms, duration);
    }

    pub(super) fn record_network_round_lock_profile(
        &self,
        phase: NetworkRoundLockProfilePhase,
        wait_ns: u64,
        hold_ns: u64,
    ) {
        let Ok(mut profile) = self.network_round_lock_profile.lock() else {
            return;
        };
        match phase {
            NetworkRoundLockProfilePhase::HeadOpen => {
                push_u64_sample(&mut profile.head_open_wait_ns, wait_ns);
                push_u64_sample(&mut profile.head_open_hold_ns, hold_ns);
            }
            NetworkRoundLockProfilePhase::Build => {
                push_u64_sample(&mut profile.build_wait_ns, wait_ns);
                push_u64_sample(&mut profile.build_hold_ns, hold_ns);
            }
        }
    }

    pub(super) fn peer_transport_endpoint_backoff_error(&self, endpoint: &str) -> Option<String> {
        let Ok(mut states) = self.peer_transport_endpoint_states.lock() else {
            return None;
        };
        let (consecutive_failures, next_retry_at, last_error) =
            states.get(endpoint).map(|state| {
                (
                    state.consecutive_failures,
                    state.next_retry_at,
                    state.last_error.clone(),
                )
            })?;
        let now = Instant::now();
        if now >= next_retry_at {
            states.remove(endpoint);
            return None;
        }
        let retry_in_ms = next_retry_at
            .saturating_duration_since(now)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        Some(format!(
            "peer transport backoff active: retry_in_ms={} consecutive_failures={} last_error={}",
            retry_in_ms, consecutive_failures, last_error
        ))
    }

    pub(super) fn record_peer_transport_endpoint_failure(&self, endpoint: &str, error: &str) {
        let Ok(mut states) = self.peer_transport_endpoint_states.lock() else {
            return;
        };
        let now = Instant::now();
        let state =
            states
                .entry(endpoint.to_owned())
                .or_insert_with(|| PeerTransportEndpointState {
                    consecutive_failures: 0,
                    last_error: String::new(),
                    last_failure_at: now,
                    next_retry_at: now,
                });
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_error = error.to_owned();
        state.last_failure_at = now;
        let exponent = state.consecutive_failures.saturating_sub(1).min(8);
        let multiplier = 1_u32 << exponent;
        let backoff = PEER_TRANSPORT_RECONNECT_BACKOFF_BASE
            .checked_mul(multiplier)
            .unwrap_or(PEER_TRANSPORT_RECONNECT_BACKOFF_MAX)
            .min(PEER_TRANSPORT_RECONNECT_BACKOFF_MAX);
        state.next_retry_at = now + backoff;
    }

    pub(super) fn clear_peer_transport_endpoint_failure(&self, endpoint: &str) {
        let Ok(mut states) = self.peer_transport_endpoint_states.lock() else {
            return;
        };
        states.remove(endpoint);
    }

    pub(super) fn peer_transport_endpoint_backoff_snapshot(
        &self,
    ) -> Vec<PeerTransportEndpointBackoffSnapshot> {
        let Ok(mut states) = self.peer_transport_endpoint_states.lock() else {
            return Vec::new();
        };
        let now = Instant::now();
        states.retain(|_, state| now < state.next_retry_at);
        states
            .iter()
            .map(|(endpoint, state)| PeerTransportEndpointBackoffSnapshot {
                endpoint: endpoint.clone(),
                consecutive_failures: state.consecutive_failures,
                retry_in_ms: state
                    .next_retry_at
                    .saturating_duration_since(now)
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
                last_error: state.last_error.clone(),
            })
            .collect()
    }

    pub(crate) fn peer_transport_sessions_snapshot(&self) -> Vec<PeerTransportSessionSnapshot> {
        let Ok(cached) = self.network_peer_streams.lock() else {
            return Vec::new();
        };
        cached
            .iter()
            .map(|(endpoint, session)| PeerTransportSessionSnapshot {
                endpoint: endpoint.clone(),
                age_ms: session
                    .connected_at
                    .elapsed()
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
                idle_ms: session
                    .last_used_at
                    .elapsed()
                    .as_millis()
                    .min(u64::MAX as u128) as u64,
                request_count: session.request_count,
                remote_session_id: session.remote_session_id,
                remote_chain_id: session.remote_chain_id,
                remote_validator_id_hex: session.remote_validator_id_hex.clone(),
                remote_current_round: session.remote_current_round,
                remote_current_view: session.remote_current_view,
                remote_head_height: session.remote_head_height,
                remote_head_hash_hex: session.remote_head_hash_hex.clone(),
                remote_committed_state_root_hex: session.remote_committed_state_root_hex.clone(),
                remote_last_committed_round: session.remote_last_committed_round,
                remote_peer_count: session.remote_peer_count,
            })
            .collect()
    }

    pub(super) fn touch_inbound_peer_transport_session(
        &self,
        session_id: u64,
        remote_addr: Option<&str>,
    ) {
        self.touch_inbound_peer_transport_session_with_metadata(
            session_id,
            remote_addr,
            &PeerTransportSessionMetadata::default(),
        );
    }

    pub(crate) fn touch_inbound_peer_transport_session_with_metadata(
        &self,
        session_id: u64,
        remote_addr: Option<&str>,
        metadata: &PeerTransportSessionMetadata,
    ) {
        let Ok(mut sessions) = self.active_peer_transport_sessions.lock() else {
            return;
        };
        let now = Instant::now();
        match sessions.entry(session_id) {
            Entry::Occupied(mut entry) => {
                let session = entry.get_mut();
                session.last_used_at = now;
                session.request_count = session.request_count.saturating_add(1);
                if let Some(remote_addr) = remote_addr {
                    session.remote_addr = Some(remote_addr.to_owned());
                } else if session.remote_addr.is_none() {
                    session.remote_addr = remote_addr.map(|addr| addr.to_owned());
                }
                if let Some(remote_chain_id) = metadata.remote_chain_id {
                    session.remote_chain_id = Some(remote_chain_id);
                }
                if let Some(remote_validator_id_hex) = metadata.remote_validator_id_hex.clone() {
                    session.remote_validator_id_hex = Some(remote_validator_id_hex);
                }
                if let Some(remote_current_round) = metadata.remote_current_round {
                    session.remote_current_round = Some(remote_current_round);
                }
                if let Some(remote_current_view) = metadata.remote_current_view {
                    session.remote_current_view = Some(remote_current_view);
                }
                if let Some(remote_head_height) = metadata.remote_head_height {
                    session.remote_head_height = Some(remote_head_height);
                }
                if let Some(remote_head_hash_hex) = metadata.remote_head_hash_hex.clone() {
                    session.remote_head_hash_hex = Some(remote_head_hash_hex);
                }
                if let Some(remote_committed_state_root_hex) =
                    metadata.remote_committed_state_root_hex.clone()
                {
                    session.remote_committed_state_root_hex = Some(remote_committed_state_root_hex);
                }
                if let Some(remote_last_committed_round) = metadata.remote_last_committed_round {
                    session.remote_last_committed_round = Some(remote_last_committed_round);
                }
                if let Some(remote_peer_count) = metadata.remote_peer_count {
                    session.remote_peer_count = Some(remote_peer_count);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(ActivePeerTransportSession {
                    connected_at: now,
                    last_used_at: now,
                    request_count: 1,
                    remote_addr: remote_addr.map(|addr| addr.to_owned()),
                    remote_chain_id: metadata.remote_chain_id,
                    remote_validator_id_hex: metadata.remote_validator_id_hex.clone(),
                    remote_current_round: metadata.remote_current_round,
                    remote_current_view: metadata.remote_current_view,
                    remote_head_height: metadata.remote_head_height,
                    remote_head_hash_hex: metadata.remote_head_hash_hex.clone(),
                    remote_committed_state_root_hex: metadata
                        .remote_committed_state_root_hex
                        .clone(),
                    remote_last_committed_round: metadata.remote_last_committed_round,
                    remote_peer_count: metadata.remote_peer_count,
                });
            }
        }
    }

    pub(super) fn remove_inbound_peer_transport_session(&self, session_id: u64) {
        let Ok(mut sessions) = self.active_peer_transport_sessions.lock() else {
            return;
        };
        sessions.remove(&session_id);
    }

    pub(super) fn inbound_peer_transport_sessions_snapshot(
        &self,
    ) -> Vec<InboundPeerTransportSessionSnapshot> {
        let Ok(mut sessions) = self.active_peer_transport_sessions.lock() else {
            return Vec::new();
        };
        sessions.retain(|_, session| {
            session.last_used_at.elapsed() <= PEER_TRANSPORT_SESSION_STALE_TIMEOUT
        });
        sessions
            .iter()
            .map(
                |(session_id, session)| InboundPeerTransportSessionSnapshot {
                    session_id: *session_id,
                    remote_addr: session.remote_addr.clone(),
                    age_ms: session
                        .connected_at
                        .elapsed()
                        .as_millis()
                        .min(u64::MAX as u128) as u64,
                    idle_ms: session
                        .last_used_at
                        .elapsed()
                        .as_millis()
                        .min(u64::MAX as u128) as u64,
                    request_count: session.request_count,
                    remote_chain_id: session.remote_chain_id,
                    remote_validator_id_hex: session.remote_validator_id_hex.clone(),
                    remote_current_round: session.remote_current_round,
                    remote_current_view: session.remote_current_view,
                    remote_head_height: session.remote_head_height,
                    remote_head_hash_hex: session.remote_head_hash_hex.clone(),
                    remote_committed_state_root_hex: session
                        .remote_committed_state_root_hex
                        .clone(),
                    remote_last_committed_round: session.remote_last_committed_round,
                    remote_peer_count: session.remote_peer_count,
                },
            )
            .collect()
    }
}
