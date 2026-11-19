use crate::*;
use std::io::{Read, Write};
use std::net::TcpStream;

impl JsonRpcHttpServer {
    pub(super) fn handle_peer_transport_connection(
        &self,
        stream: &mut TcpStream,
    ) -> Result<(), TransportError> {
        let mut magic = [0_u8; PEER_TRANSPORT_MAGIC.len()];
        stream
            .read_exact(&mut magic)
            .map_err(TransportError::Read)?;
        if magic != *PEER_TRANSPORT_MAGIC {
            return Err(TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid peer transport magic",
            )));
        }
        let session_id = self.next_peer_session_id.fetch_add(1, Ordering::Relaxed);

        loop {
            let request = match read_peer_transport_request(stream) {
                Ok(request) => request,
                Err(TransportError::Read(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    self.remove_inbound_peer_transport_session(session_id);
                    return Ok(());
                }
                Err(error) => {
                    self.remove_inbound_peer_transport_session(session_id);
                    return Err(error);
                }
            };
            let remote_addr = stream.peer_addr().ok().map(|addr| addr.to_string());
            if request.path == "/network/session/open" {
                let open_request = if request.body.trim().is_empty() {
                    PeerTransportSessionOpenRequest {
                        chain_id: None,
                        validator_id_hex: None,
                        current_round: None,
                        current_view: None,
                        head_height: None,
                        head_hash_hex: None,
                        committed_state_root_hex: None,
                        last_committed_round: None,
                        peer_count: None,
                    }
                } else {
                    match serde_json::from_str::<PeerTransportSessionOpenRequest>(&request.body) {
                        Ok(open_request) => open_request,
                        Err(error) => {
                            let response = PeerTransportResponse {
                                body: None,
                                error: Some(JsonRpcErrorObject {
                                    code: -32602,
                                    message: format!(
                                        "invalid peer transport session open request: {error}"
                                    ),
                                    data: None,
                                }),
                            };
                            write_peer_transport_response(stream, &response)?;
                            self.remove_inbound_peer_transport_session(session_id);
                            return Ok(());
                        }
                    }
                };
                self.touch_inbound_peer_transport_session_with_metadata(
                    session_id,
                    remote_addr.as_deref(),
                    &PeerTransportSessionMetadata {
                        remote_chain_id: open_request.chain_id,
                        remote_validator_id_hex: open_request.validator_id_hex,
                        remote_current_round: open_request.current_round,
                        remote_current_view: open_request.current_view,
                        remote_head_height: open_request.head_height,
                        remote_head_hash_hex: open_request.head_hash_hex,
                        remote_committed_state_root_hex: open_request.committed_state_root_hex,
                        remote_last_committed_round: open_request.last_committed_round,
                        remote_peer_count: open_request.peer_count,
                    },
                );
            } else if request.path == "/network/session/heartbeat" {
                let heartbeat_request = if request.body.trim().is_empty() {
                    PeerTransportSessionHeartbeatRequest {
                        chain_id: None,
                        validator_id_hex: None,
                        current_round: None,
                        current_view: None,
                        head_height: None,
                        head_hash_hex: None,
                        committed_state_root_hex: None,
                        last_committed_round: None,
                        peer_count: None,
                    }
                } else {
                    match serde_json::from_str::<PeerTransportSessionHeartbeatRequest>(
                        &request.body,
                    ) {
                        Ok(heartbeat_request) => heartbeat_request,
                        Err(error) => {
                            let response = PeerTransportResponse {
                                body: None,
                                error: Some(JsonRpcErrorObject {
                                    code: -32602,
                                    message: format!(
                                        "invalid peer transport session heartbeat request: {error}"
                                    ),
                                    data: None,
                                }),
                            };
                            write_peer_transport_response(stream, &response)?;
                            self.remove_inbound_peer_transport_session(session_id);
                            return Ok(());
                        }
                    }
                };
                self.touch_inbound_peer_transport_session_with_metadata(
                    session_id,
                    remote_addr.as_deref(),
                    &PeerTransportSessionMetadata {
                        remote_chain_id: heartbeat_request.chain_id,
                        remote_validator_id_hex: heartbeat_request.validator_id_hex,
                        remote_current_round: heartbeat_request.current_round,
                        remote_current_view: heartbeat_request.current_view,
                        remote_head_height: heartbeat_request.head_height,
                        remote_head_hash_hex: heartbeat_request.head_hash_hex,
                        remote_committed_state_root_hex: heartbeat_request.committed_state_root_hex,
                        remote_last_committed_round: heartbeat_request.last_committed_round,
                        remote_peer_count: heartbeat_request.peer_count,
                    },
                );
            } else {
                self.touch_inbound_peer_transport_session(session_id, remote_addr.as_deref());
            }
            let close_session = request.path == "/network/session/close";
            let response = match request.path.as_str() {
                "/network/session/open" => {
                    match self.network_session_open_response_body(session_id) {
                        Ok(body) => PeerTransportResponse {
                            body: Some(body),
                            error: None,
                        },
                        Err(error) => PeerTransportResponse {
                            body: None,
                            error: Some(JsonRpcErrorObject {
                                code: -32000,
                                message: format!("{error:?}"),
                                data: None,
                            }),
                        },
                    }
                }
                "/network/session/close" => {
                    match self.network_session_close_response_body(session_id) {
                        Ok(body) => PeerTransportResponse {
                            body: Some(body),
                            error: None,
                        },
                        Err(error) => PeerTransportResponse {
                            body: None,
                            error: Some(JsonRpcErrorObject {
                                code: -32000,
                                message: format!("{error:?}"),
                                data: None,
                            }),
                        },
                    }
                }
                "/network/session/heartbeat" => {
                    match self.network_session_heartbeat_response_body(session_id) {
                        Ok(body) => PeerTransportResponse {
                            body: Some(body),
                            error: None,
                        },
                        Err(error) => PeerTransportResponse {
                            body: None,
                            error: Some(JsonRpcErrorObject {
                                code: -32000,
                                message: format!("{error:?}"),
                                data: None,
                            }),
                        },
                    }
                }
                "/network/bootstrap" => match self.network_bootstrap_response_body() {
                    Ok(body) => PeerTransportResponse {
                        body: Some(body),
                        error: None,
                    },
                    Err(TransportError::Read(error))
                        if error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        PeerTransportResponse {
                            body: None,
                            error: Some(JsonRpcErrorObject {
                                code: -32601,
                                message: "network bootstrap not configured".into(),
                                data: None,
                            }),
                        }
                    }
                    Err(error) => PeerTransportResponse {
                        body: None,
                        error: Some(JsonRpcErrorObject {
                            code: -32000,
                            message: format!("{error:?}"),
                            data: None,
                        }),
                    },
                },
                "/network/ping" => match self.network_ping_response_body() {
                    Ok(body) => PeerTransportResponse {
                        body: Some(body),
                        error: None,
                    },
                    Err(error) => PeerTransportResponse {
                        body: None,
                        error: Some(JsonRpcErrorObject {
                            code: -32000,
                            message: format!("{error:?}"),
                            data: None,
                        }),
                    },
                },
                "/network/snapshot" => peer_transport_response_from_result(
                    self.process_network_snapshot(&request.headers),
                ),
                "/network/head-qc-signature" => peer_transport_response_from_result(
                    self.process_network_head_qc_signature(&request.headers, &request.body),
                ),
                "/network/timeout-message" => peer_transport_response_from_result(
                    self.process_network_timeout_message(&request.headers, &request.body),
                ),
                "/network/qc-signature" => peer_transport_response_from_result(
                    self.process_network_qc_signature(&request.headers, &request.body),
                ),
                "/network/relay-intents" => peer_transport_response_from_result(
                    self.process_network_relay_intents(&request.headers, &request.body),
                ),
                "/network/relay-delegation-actions" => peer_transport_response_from_result(
                    self.process_network_relay_delegation_actions(&request.headers, &request.body),
                ),
                "/network/relay-evm-delegation-actions" => peer_transport_response_from_result(
                    self.process_network_relay_evm_delegation_actions(
                        &request.headers,
                        &request.body,
                    ),
                ),
                "/network/relay-market-maker-program-actions" => {
                    peer_transport_response_from_result(
                        self.process_network_relay_market_maker_program_actions(
                            &request.headers,
                            &request.body,
                        ),
                    )
                }
                "/network/mahi/block" => peer_transport_response_from_result(
                    self.process_network_mahi_block(&request.headers, &request.body),
                ),
                "/network/mahi/blocks" => peer_transport_response_from_result(
                    self.process_network_mahi_blocks(&request.headers, &request.body),
                ),
                "/network/mahi/fetch" => peer_transport_response_from_result(
                    self.process_network_mahi_fetch(&request.headers, &request.body),
                ),
                "/network/commit" => peer_transport_response_from_result(
                    self.process_network_commit(&request.headers, &request.body),
                ),
                "/network/install-timeout-certificate" => peer_transport_response_from_result(
                    self.process_network_timeout_certificate(&request.headers, &request.body),
                ),
                _ => PeerTransportResponse {
                    body: None,
                    error: Some(JsonRpcErrorObject {
                        code: -32601,
                        message: "peer transport method not found".into(),
                        data: None,
                    }),
                },
            };
            if let Err(error) = write_peer_transport_response(stream, &response) {
                self.remove_inbound_peer_transport_session(session_id);
                return Err(error);
            }
            if close_session {
                self.remove_inbound_peer_transport_session(session_id);
                return Ok(());
            }
        }
    }

    pub(super) fn handle_network_bootstrap_request(
        &self,
        stream: &mut TcpStream,
    ) -> Result<(), TransportError> {
        let body = match self.network_bootstrap_response_body() {
            Ok(body) => body,
            Err(TransportError::Read(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return write_http_response(
                    stream,
                    "404 Not Found",
                    "text/plain",
                    "network bootstrap not configured",
                );
            }
            Err(error) => return Err(error),
        };
        write_http_response(stream, "200 OK", "application/json", &body)
    }

    pub(super) fn handle_network_ping_request(
        &self,
        stream: &mut TcpStream,
    ) -> Result<(), TransportError> {
        let body = self.network_ping_response_body()?;
        write_http_response(stream, "200 OK", "application/json", &body)
    }

    pub(super) fn network_session_open_response_body(
        &self,
        session_id: u64,
    ) -> Result<String, TransportError> {
        let head = self.engine.head_snapshot();
        let metrics = self.engine.consensus_metrics_snapshot();
        let current_round = self.network_round_snapshot();
        let validator_id_hex = head
            .local_validator_id
            .map(|validator_id| encode_hex(&validator_id));
        serde_json::to_string(&PeerTransportSessionOpenResponse {
            session_id,
            chain_id: head.chain_id,
            validator_id_hex,
            current_round: Some(current_round),
            current_view: Some(metrics.current_view),
            head_height: Some(head.head_height),
            head_hash_hex: Some(encode_hex(&head.head_hash)),
            committed_state_root_hex: Some(encode_hex(&head.committed_state_root)),
            last_committed_round: Some(metrics.last_committed_round),
            peer_count: Some(metrics.peer_count),
        })
        .map_err(|error| TransportError::Read(std::io::Error::other(error)))
    }

    pub(super) fn network_session_close_response_body(
        &self,
        session_id: u64,
    ) -> Result<String, TransportError> {
        serde_json::to_string(&PeerTransportSessionCloseResponse {
            closed: true,
            session_id,
        })
        .map_err(|error| TransportError::Read(std::io::Error::other(error)))
    }

    pub(super) fn network_session_heartbeat_response_body(
        &self,
        session_id: u64,
    ) -> Result<String, TransportError> {
        let (chain_id, validator_id_hex, metadata) = self.local_peer_transport_session_metadata();
        serde_json::to_string(&PeerTransportSessionHeartbeatResponse {
            alive: true,
            session_id,
            chain_id,
            validator_id_hex,
            current_round: metadata.remote_current_round,
            current_view: metadata.remote_current_view,
            head_height: metadata.remote_head_height,
            head_hash_hex: metadata.remote_head_hash_hex,
            committed_state_root_hex: metadata.remote_committed_state_root_hex,
            last_committed_round: metadata.remote_last_committed_round,
            peer_count: metadata.remote_peer_count,
        })
        .map_err(|error| TransportError::Read(std::io::Error::other(error)))
    }

    pub(crate) fn network_bootstrap_response_body(&self) -> Result<String, TransportError> {
        let Some(network_bootstrap) = self.config.network_bootstrap.as_ref() else {
            return Err(TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "network bootstrap not configured",
            )));
        };
        let Some(local_validator) = network_bootstrap.local_validator.as_ref() else {
            return Err(TransportError::Read(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "network bootstrap not available for observer role",
            )));
        };
        let engine = self
            .engine
            .lock()
            .map_err(|_| TransportError::Read(std::io::Error::other("engine lock poisoned")))?;
        let local_consensus_pubkey = engine
            .validator_consensus_pubkey(local_validator.validator_id)
            .ok_or_else(|| {
                TransportError::Read(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "local validator missing consensus pubkey",
                ))
            })?;
        let split_network_listener = env::var("KOMBAT_NETWORK_RPC_BIND")
            .ok()
            .map(|bind| !bind.trim().is_empty())
            .unwrap_or(false);
        let head = self.engine.head_snapshot();
        let current_round = self.network_round_snapshot();
        serde_json::to_string(&serde_json::json!({
            "chain_id": head.chain_id,
            "validator_set_epoch": network_bootstrap.validator_set.epoch,
            "validator_id_hex": encode_hex(&local_validator.validator_id),
            "consensus_pubkey_hex": encode_hex(&local_consensus_pubkey),
            "current_round": current_round,
            "head_height": head.head_height,
            "head_hash_hex": encode_hex(&head.head_hash),
            "committed_state_root_hex": encode_hex(&head.committed_state_root),
            "public_rpc_url": network_bootstrap.public_rpc_url,
            "snapshot_path": if split_network_listener {
                None::<String>
            } else {
                network_bootstrap
                    .public_rpc_url
                    .as_ref()
                    .map(|base| format!("{}/network/snapshot", base.trim_end_matches('/')))
            },
            "local": validator_config_json(local_validator),
            "peers": network_bootstrap
                .bootstrap_peers
                .peers
                .iter()
                .map(validator_peer_json)
                .collect::<Vec<_>>(),
        }))
        .map_err(|error| TransportError::Read(std::io::Error::other(error)))
    }

    pub(crate) fn network_ping_response_body(&self) -> Result<String, TransportError> {
        let head = self.engine.head_snapshot();
        let current_round = self.network_round_snapshot();
        serde_json::to_string(&NetworkPingResponse {
            chain_id: head.chain_id,
            current_round,
            head_height: head.head_height,
            head_hash_hex: encode_hex(&head.head_hash),
        })
        .map_err(|error| TransportError::Read(std::io::Error::other(error)))
    }

    pub(super) fn handle_network_snapshot_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
    ) -> Result<(), TransportError> {
        if !operator_request_is_authorized(headers, &self.config) {
            return write_http_response(
                stream,
                "401 Unauthorized",
                "text/plain",
                "operator authorization required",
            );
        }

        let current_round = self.network_round_snapshot();
        let engine = self
            .engine
            .lock()
            .map_err(|_| TransportError::Read(std::io::Error::other("engine lock poisoned")))?;
        let snapshot = engine.snapshot_bytes();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nX-And-Chain-Id: {}\r\nX-And-Head-Height: {}\r\nX-And-Current-Round: {}\r\nConnection: close\r\n\r\n",
            snapshot.len(),
            engine.chain_id(),
            engine.head().header.height,
            current_round,
        );
        stream
            .write_all(response.as_bytes())
            .map_err(TransportError::Write)?;
        stream.write_all(&snapshot).map_err(TransportError::Write)?;
        stream.flush().map_err(TransportError::Write)
    }

    pub(super) fn process_network_snapshot(
        &self,
        headers: &BTreeMap<String, String>,
    ) -> Result<String, JsonRpcErrorObject> {
        if !operator_request_is_authorized(headers, &self.config) {
            return Err(JsonRpcErrorObject {
                code: -32600,
                message: "operator authorization required".into(),
                data: None,
            });
        }

        let engine = self.engine.lock().map_err(|_| JsonRpcErrorObject {
            code: -32000,
            message: "engine lock poisoned".into(),
            data: None,
        })?;
        Ok(BASE64_STANDARD.encode(engine.snapshot_bytes()))
    }

    pub(super) fn handle_network_timeout_message_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_timeout_message(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_timeout_certificate_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_timeout_certificate(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_qc_signature_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_qc_signature(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_relay_intents_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_relay_intents(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_relay_delegation_actions_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_relay_delegation_actions(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_relay_evm_delegation_actions_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_relay_evm_delegation_actions(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_relay_market_maker_program_actions_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_relay_market_maker_program_actions(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_mahi_block_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_mahi_block(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_mahi_blocks_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_mahi_blocks(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_mahi_fetch_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_mahi_fetch(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_head_qc_signature_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_head_qc_signature(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }

    pub(super) fn handle_network_commit_request(
        &self,
        stream: &mut TcpStream,
        headers: &BTreeMap<String, String>,
        body: &str,
    ) -> Result<(), TransportError> {
        match self.process_network_commit(headers, body) {
            Ok(response) => write_http_response(stream, "200 OK", "application/json", &response),
            Err(error) => Err(json_rpc_error_to_transport_error(error)),
        }
    }
}
