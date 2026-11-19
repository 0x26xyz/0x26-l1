use crate::*;
use async_stream::stream;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, Response, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) async fn axum_handle_options(headers: HeaderMap) -> Response<Body> {
    let request_headers = header_map_from_axum(&headers);
    json_response_with_browser_headers(
        StatusCode::NO_CONTENT,
        "text/plain",
        &BTreeMap::new(),
        &request_headers,
        String::new(),
    )
}

pub(crate) async fn axum_handle_json_rpc(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let request_headers = header_map_from_axum(&headers);
    let handler_headers = request_headers.clone();
    match tokio::task::spawn_blocking(move || {
        server.handle_json_http_request_with_headers(&body, &handler_headers)
    })
    .await
    {
        Ok(response) => json_response_with_browser_headers(
            response.status,
            response.content_type,
            &response.headers,
            &request_headers,
            response.body,
        ),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("json-rpc handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_bootstrap(
    State(server): State<Arc<JsonRpcHttpServer>>,
) -> Response<Body> {
    match tokio::task::spawn_blocking(move || server.network_bootstrap_response_body()).await {
        Ok(Ok(body)) => json_response(StatusCode::OK, "application/json", body),
        Ok(Err(error)) => plain_response(StatusCode::BAD_REQUEST, &transport_error_message(&error)),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network bootstrap handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_ping(
    State(server): State<Arc<JsonRpcHttpServer>>,
) -> Response<Body> {
    match tokio::task::spawn_blocking(move || server.network_ping_response_body()).await {
        Ok(Ok(body)) => json_response(StatusCode::OK, "application/json", body),
        Ok(Err(error)) => plain_response(StatusCode::BAD_REQUEST, &transport_error_message(&error)),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network ping handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_snapshot(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    if !operator_request_is_authorized(&headers, &server.config) {
        return plain_response(StatusCode::UNAUTHORIZED, "operator authorization required");
    }
    let snapshot = match tokio::task::spawn_blocking(move || {
        let engine = server
            .engine
            .lock()
            .map_err(|_| "engine lock poisoned".to_string())?;
        Ok::<_, String>((
            engine.chain_id(),
            engine.head().header.height,
            engine.current_round(),
            engine.snapshot_bytes(),
        ))
    })
    .await
    {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(error)) => return plain_response(StatusCode::INTERNAL_SERVER_ERROR, &error),
        Err(error) => {
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("network snapshot handler failed: {error}"),
            )
        }
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("x-and-chain-id", snapshot.0.to_string())
        .header("x-and-head-height", snapshot.1.to_string())
        .header("x-and-current-round", snapshot.2.to_string())
        .body(Body::from(snapshot.3))
        .unwrap_or_else(|_| Response::new(Body::from("internal server error")))
}

pub(crate) async fn axum_handle_network_head_qc_signature(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || {
        server.process_network_head_qc_signature(&headers, &body)
    })
    .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network head qc handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_timeout_message(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || {
        server.process_network_timeout_message(&headers, &body)
    })
    .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network timeout handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_qc_signature(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || server.process_network_qc_signature(&headers, &body))
        .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network qc handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_relay_intents(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || server.process_network_relay_intents(&headers, &body))
        .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network relay handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_relay_delegation_actions(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || {
        server.process_network_relay_delegation_actions(&headers, &body)
    })
    .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network delegation relay handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_relay_evm_delegation_actions(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || {
        server.process_network_relay_evm_delegation_actions(&headers, &body)
    })
    .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network EVM delegation relay handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_relay_market_maker_program_actions(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || {
        server.process_network_relay_market_maker_program_actions(&headers, &body)
    })
    .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network market-maker program relay handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_mahi_block(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || server.process_network_mahi_block(&headers, &body))
        .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network mahi block handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_mahi_blocks(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || server.process_network_mahi_blocks(&headers, &body))
        .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network mahi blocks handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_mahi_fetch(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || server.process_network_mahi_fetch(&headers, &body))
        .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network mahi fetch handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_commit(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || server.process_network_commit(&headers, &body)).await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network commit handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_network_timeout_certificate(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    body: String,
) -> Response<Body> {
    let headers = header_map_from_axum(&headers);
    match tokio::task::spawn_blocking(move || {
        server.process_network_timeout_certificate(&headers, &body)
    })
    .await
    {
        Ok(result) => network_json_response(result),
        Err(error) => plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("network timeout certificate handler failed: {error}"),
        ),
    }
}

pub(crate) async fn axum_handle_events(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    Query(query): Query<AxumEventStreamQuery>,
) -> impl IntoResponse {
    let mut next_seq = query.from_seq.unwrap_or(0);
    let follow = query.follow.unwrap_or(false);
    let timeout = Duration::from_millis(query.timeout_ms.unwrap_or(30_000).min(60_000));
    let started_at = Instant::now();
    let headers = header_map_from_axum(&headers);
    if query.from_seq.is_none() {
        if let Some(last_event_id) = headers.get("last-event-id") {
            if let Ok(resume_from) = last_event_id.parse::<u64>() {
                next_seq = resume_from.saturating_add(1);
            }
        }
    }
    let intent_id = query
        .intent_id_hex
        .as_deref()
        .map(parse_intent_id_query)
        .transpose();
    let topic = query
        .topic
        .as_deref()
        .map(parse_runtime_event_topic)
        .transpose();
    let parsed = match (intent_id, topic) {
        (Ok(intent_id), Ok(topic)) => Ok(EventStreamRequest {
            from_seq: next_seq,
            from_seq_explicit: query.from_seq.is_some(),
            limit: query.limit.unwrap_or(100).clamp(1, 1_000),
            filter: EventFilter {
                block_height: None,
                market_id: query.market_id,
                account_id: query.account_id,
                policy_id: query.policy_id,
                intent_id,
                client_order_id: query.client_order_id,
                topic,
            },
            follow,
            timeout_ms: timeout.as_millis() as u64,
        }),
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    match parsed {
        Ok(params) => {
            let event_stream = stream! {
                let mut cursor = params.from_seq;
                loop {
                    let page = {
                        let engine = match server.engine.lock() {
                            Ok(engine) => engine,
                            Err(_) => break,
                        };
                        EventStream::new(engine.runtime_events()).query(StreamQuery {
                            from_seq: cursor,
                            before_seq: None,
                            limit: params.limit,
                            filter: params.filter,
                            descending: false,
                        })
                    };
                    if !page.events.is_empty() {
                        for event in &page.events {
                            let payload = runtime_event_to_json(event).to_string();
                            yield Ok::<SseEvent, std::convert::Infallible>(
                                SseEvent::default()
                                    .id(event.seq.to_string())
                                    .event(event_topic_name(event.kind.topic()))
                                    .data(payload)
                            );
                        }
                        cursor = page.next_cursor.next_seq;
                        yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{cursor}}}")));
                        if !params.follow {
                            break;
                        }
                        continue;
                    }
                    if !params.follow || started_at.elapsed() >= timeout {
                        yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{cursor}}}")));
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            };
            let mut response = Sse::new(event_stream)
                .keep_alive(KeepAlive::default())
                .into_response();
            insert_browser_sse_headers(response.headers_mut(), &headers);
            response
        }
        Err(message) => plain_response(StatusCode::BAD_REQUEST, message).into_response(),
    }
}

pub(crate) async fn axum_handle_market_stream(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    Query(query): Query<AxumMarketStreamQuery>,
) -> impl IntoResponse {
    if query.market_id == 0 {
        return plain_response(StatusCode::BAD_REQUEST, "market_id is required").into_response();
    }
    let headers = header_map_from_axum(&headers);
    let mut from_seq = resume_seq(query.from_seq, &headers);
    let follow = query.follow.unwrap_or(true);
    let depth = query.depth.unwrap_or(20).clamp(1, 200);
    let limit = query.limit.unwrap_or(100).clamp(1, 1_000);
    let timeout = Duration::from_millis(query.timeout_ms.unwrap_or(30_000).min(60_000));
    let started_at = Instant::now();
    let market_id = query.market_id;
    let event_stream = stream! {
        let snapshot = {
            let mut engine = match server.engine.lock() {
                Ok(engine) => engine,
                Err(_) => return,
            };
            let service = NodeRpcService::new(&mut engine);
            market_snapshot_response_to_json(&service.get_market_snapshot(market_id, depth))
        };
        yield Ok::<SseEvent, std::convert::Infallible>(
            SseEvent::default()
                .event("market_snapshot")
                .data(snapshot.to_string())
        );
        loop {
            let items = {
                let mut engine = match server.engine.lock() {
                    Ok(engine) => engine,
                    Err(_) => break,
                };
                let service = NodeRpcService::new(&mut engine);
                let page = EventStream::new(service.engine.runtime_events()).query(StreamQuery {
                    from_seq,
                    before_seq: None,
                    limit,
                    filter: EventFilter {
                        block_height: None,
                        account_id: None,
                        market_id: Some(market_id),
                        policy_id: None,
                        intent_id: None,
                        client_order_id: None,
                        topic: None,
                    },
                    descending: false,
                });
                let next = page.next_cursor.next_seq;
                let payloads = page
                    .events
                    .iter()
                    .flat_map(|event| market_stream_payload_for_event(&service, event, depth))
                    .collect::<Vec<_>>();
                (payloads, next)
            };
            if !items.0.is_empty() {
                for (name, seq, payload) in items.0 {
                    yield Ok(SseEvent::default().id(seq.to_string()).event(name).data(payload.to_string()));
                }
                from_seq = items.1;
                yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{from_seq}}}")));
                if !follow {
                    break;
                }
                continue;
            }
            from_seq = items.1;
            if !follow || started_at.elapsed() >= timeout {
                yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{from_seq}}}")));
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let mut response = Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response();
    insert_browser_sse_headers(response.headers_mut(), &headers);
    response
}

pub(crate) async fn axum_handle_account_stream(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    Query(query): Query<AxumAccountStreamQuery>,
) -> impl IntoResponse {
    if query.account_id == 0 {
        return plain_response(StatusCode::BAD_REQUEST, "account_id is required").into_response();
    }
    let headers = header_map_from_axum(&headers);
    let mut from_seq = resume_seq(query.from_seq, &headers);
    let follow = query.follow.unwrap_or(true);
    let depth = query.depth.unwrap_or(100).clamp(1, 1_000);
    let limit = query.limit.unwrap_or(100).clamp(1, 1_000);
    let timeout = Duration::from_millis(query.timeout_ms.unwrap_or(30_000).min(60_000));
    let started_at = Instant::now();
    let account_id = query.account_id;
    let market_id = query.market_id;
    let event_stream = stream! {
        let snapshot = {
            let mut engine = match server.engine.lock() {
                Ok(engine) => engine,
                Err(_) => return,
            };
            let service = NodeRpcService::new(&mut engine);
            account_trading_state_response_to_json(&service.get_account_trading_state(account_id, market_id, depth, 100))
        };
        yield Ok::<SseEvent, std::convert::Infallible>(
            SseEvent::default()
                .event("account_trading_state")
                .data(snapshot.to_string())
        );
        loop {
            let items = {
                let mut engine = match server.engine.lock() {
                    Ok(engine) => engine,
                    Err(_) => break,
                };
                let service = NodeRpcService::new(&mut engine);
                let page = EventStream::new(service.engine.runtime_events()).query(StreamQuery {
                    from_seq,
                    before_seq: None,
                    limit,
                    filter: EventFilter {
                        block_height: None,
                        account_id: Some(account_id),
                        market_id,
                        policy_id: None,
                        intent_id: None,
                        client_order_id: None,
                        topic: None,
                    },
                    descending: false,
                });
                let next = page.next_cursor.next_seq;
                let payloads = page
                    .events
                    .iter()
                    .filter_map(|event| account_stream_payload_for_event(&service, event))
                    .collect::<Vec<_>>();
                (payloads, next)
            };
            if !items.0.is_empty() {
                for (name, seq, payload) in items.0 {
                    yield Ok(SseEvent::default().id(seq.to_string()).event(name).data(payload.to_string()));
                }
                from_seq = items.1;
                yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{from_seq}}}")));
                if !follow {
                    break;
                }
                continue;
            }
            from_seq = items.1;
            if !follow || started_at.elapsed() >= timeout {
                yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{from_seq}}}")));
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let mut response = Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response();
    insert_browser_sse_headers(response.headers_mut(), &headers);
    response
}

pub(crate) async fn axum_handle_wallet_stream(
    State(server): State<Arc<JsonRpcHttpServer>>,
    headers: HeaderMap,
    Query(query): Query<AxumWalletStreamQuery>,
) -> impl IntoResponse {
    let address = match decode_hex_array::<20>(&query.address_hex) {
        Ok(address) => address,
        Err(message) => return plain_response(StatusCode::BAD_REQUEST, &message).into_response(),
    };
    let headers = header_map_from_axum(&headers);
    let mut next_seq = resume_seq(query.from_seq, &headers);
    let follow = query.follow.unwrap_or(true);
    let limit = query.limit.unwrap_or(100).clamp(1, 1_000);
    let timeout = Duration::from_millis(query.timeout_ms.unwrap_or(30_000).min(60_000));
    let started_at = Instant::now();
    let event_stream = stream! {
        let mut last_snapshot = {
            let mut engine = match server.engine.lock() {
                Ok(engine) => engine,
                Err(_) => return,
            };
            let service = NodeRpcService::new(&mut engine);
            service.get_wallet_balances(address)
        };
        next_seq = next_seq.max(last_snapshot.next_seq);
        yield Ok::<SseEvent, std::convert::Infallible>(
            SseEvent::default()
                .event("wallet_balances")
                .data(wallet_balances_response_to_json(&last_snapshot).to_string())
        );
        let mut emitted_count = 0_usize;
        loop {
            let snapshot = {
                let mut engine = match server.engine.lock() {
                    Ok(engine) => engine,
                    Err(_) => break,
                };
                let service = NodeRpcService::new(&mut engine);
                service.get_wallet_balances(address)
            };
            let cursor = next_seq.max(snapshot.next_seq);
            if wallet_balances_changed(&last_snapshot, &snapshot) {
                let event_seq = cursor;
                next_seq = event_seq.saturating_add(1);
                emitted_count = emitted_count.saturating_add(1);
                yield Ok(SseEvent::default()
                    .id(event_seq.to_string())
                    .event("wallet_balances")
                    .data(wallet_balances_stream_payload(&snapshot, event_seq).to_string()));
                yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{next_seq}}}")));
                last_snapshot = snapshot;
                if !follow || emitted_count >= limit {
                    break;
                }
                continue;
            }
            next_seq = cursor;
            if !follow || started_at.elapsed() >= timeout {
                yield Ok(SseEvent::default().event("cursor").data(format!("{{\"next_seq\":{next_seq}}}")));
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    let mut response = Sse::new(event_stream)
        .keep_alive(KeepAlive::default())
        .into_response();
    insert_browser_sse_headers(response.headers_mut(), &headers);
    response
}

pub(crate) fn submit_owner_call_locally(
    service: &mut NodeRpcService<'_>,
    params: SubmitOwnerCallParams,
) -> Result<Value, JsonRpcErrorObject> {
    let signed_owner_call =
        signed_owner_call_from_json(&params.signed_owner_call).map_err(|message| {
            JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            }
        })?;
    service
        .submit_owner_call(signed_owner_call, params.timestamp_ms)
        .map(|response| json!(submit_owner_call_response_to_json(&response)))
        .map_err(map_rpc_error_to_json_rpc_error)
}

pub(crate) fn submit_evm_owner_call_locally(
    service: &mut NodeRpcService<'_>,
    params: SubmitEvmOwnerCallParams,
) -> Result<Value, JsonRpcErrorObject> {
    let signed_owner_call =
        signed_evm_owner_call_from_json(&params.signed_owner_call).map_err(|message| {
            JsonRpcErrorObject {
                code: -32602,
                message,
                data: None,
            }
        })?;
    service
        .submit_evm_owner_call(signed_owner_call, params.timestamp_ms)
        .map(|response| json!(submit_owner_call_response_to_json(&response)))
        .map_err(map_rpc_error_to_json_rpc_error)
}

pub(crate) fn is_delegation_write_method(method: &str) -> bool {
    matches!(method, "kombat_submitDepositToDelegation")
        || is_legacy_delegation_bridge_method(method)
}

pub(crate) fn delegation_write_timestamp_ms(
    method: &str,
    params: &Value,
) -> Result<u64, JsonRpcErrorObject> {
    if !is_delegation_write_method(method) {
        return Err(JsonRpcErrorObject {
            code: -32601,
            message: "method not found".into(),
            data: None,
        });
    }
    params
        .get("timestamp_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| JsonRpcErrorObject {
            code: -32602,
            message: "invalid params".into(),
            data: None,
        })
}

pub(crate) fn is_legacy_delegation_bridge_method(method: &str) -> bool {
    matches!(
        method,
        "kombat_openDelegation"
            | "kombat_depositToDelegation"
            | "kombat_requestDelegationRedemption"
            | "kombat_finalizeDelegationRedemption"
            | "kombat_cancelDelegationRedemption"
            | "kombat_closeDelegationToDeposits"
            | "kombat_beginDelegationWinding"
            | "kombat_closeDelegation"
            | "kombat_submitDelegationAction"
    )
}

pub(crate) fn legacy_delegation_bridge_disabled_error(method: &str) -> JsonRpcErrorObject {
    JsonRpcErrorObject {
        code: -32026,
        message: "delegation rejected".into(),
        data: Some(json!({
            "kind": "delegation",
            "reason": "legacy_delegation_bridge_disabled",
            "method": method,
        })),
    }
}

pub(crate) fn attach_peer_failures(mut value: Value, failures: &[PeerFanoutFailure]) -> Value {
    if failures.is_empty() {
        return value;
    }
    let peer_failures = failures
        .iter()
        .map(|failure| {
            json!({
                "validator_id_hex": encode_hex(&failure.validator_id),
                "message": failure.message,
            })
        })
        .collect::<Vec<_>>();
    if let Value::Object(object) = &mut value {
        object.insert("peer_failures".into(), Value::Array(peer_failures));
        return value;
    }
    json!({
        "result": value,
        "peer_failures": peer_failures,
    })
}

pub(crate) fn validate_delegation_action_wait_timeout(
    wait_timeout_ms: Option<u64>,
) -> Result<(), JsonRpcErrorObject> {
    if wait_timeout_ms.is_some_and(|timeout_ms| timeout_ms > DELEGATION_ACTION_WAIT_MAX_MS) {
        return Err(JsonRpcErrorObject {
            code: -32602,
            message: "wait_timeout_ms exceeds maximum".into(),
            data: Some(json!({
                "reason": "wait_timeout_ms_exceeds_max",
                "max_wait_timeout_ms": DELEGATION_ACTION_WAIT_MAX_MS,
            })),
        });
    }
    Ok(())
}

pub(crate) fn delegation_action_wait_rejection_error(
    status: &DelegationActionStatusResponse,
    action_kind: &str,
    reason: &str,
) -> JsonRpcErrorObject {
    JsonRpcErrorObject {
        code: -32026,
        message: "delegation rejected".into(),
        data: Some(json!({
            "kind": "delegation",
            "reason": reason,
            "action_id_hex": encode_hex(&status.action_id),
            "action_kind": action_kind,
            "processed_at_ms": status.processed_at_ms,
        })),
    }
}

pub(crate) fn market_maker_program_action_wait_rejection_error(
    status: &MarketMakerProgramActionStatusResponse,
    action_kind: &str,
    reason: &str,
) -> JsonRpcErrorObject {
    JsonRpcErrorObject {
        code: -32028,
        message: "market maker program rejected".into(),
        data: Some(json!({
            "kind": "market_maker_program",
            "reason": reason,
            "action_id_hex": encode_hex(&status.action_id),
            "action_kind": action_kind,
            "processed_at_ms": status.processed_at_ms,
        })),
    }
}

pub(crate) fn attach_delegation_action_status(
    mut value: Value,
    status: &DelegationActionStatusResponse,
) -> Value {
    let action_status = delegation_action_status_response_to_json(status);
    if let Value::Object(object) = &mut value {
        object.insert("action_status".into(), action_status);
        return value;
    }
    json!({
        "result": value,
        "action_status": action_status,
    })
}

pub(crate) fn attach_market_maker_program_action_status(
    mut value: Value,
    status: &MarketMakerProgramActionStatusResponse,
) -> Value {
    let action_status = market_maker_program_action_status_response_to_json(status);
    if let Value::Object(object) = &mut value {
        object.insert("action_status".into(), action_status);
        return value;
    }
    json!({
        "result": value,
        "action_status": action_status,
    })
}

pub(crate) fn submit_delegation_action_locally(
    service: &mut NodeRpcService<'_>,
    method: &str,
    params: Value,
) -> Result<Value, JsonRpcErrorObject> {
    match method {
        "kombat_openDelegation" => {
            let params: OpenDelegationParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .open_delegation(params)
                .map(|response| delegation_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_depositToDelegation" => {
            let params: DepositToDelegationParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .deposit_to_delegation(params)
                .map(|response| delegation_deposit_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_submitDepositToDelegation" => {
            let params: SubmitDepositToDelegationParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .submit_deposit_to_delegation(params)
                .map(|response| delegation_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_requestDelegationRedemption" => {
            let params: RequestDelegationRedemptionParams = serde_json::from_value(params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .request_delegation_redemption(params)
                .map(|response| delegation_redemption_request_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_finalizeDelegationRedemption" => {
            let params: FinalizeDelegationRedemptionParams = serde_json::from_value(params)
                .map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .finalize_delegation_redemption(params)
                .map(|response| delegation_redemption_finalization_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_cancelDelegationRedemption" => {
            let params: CancelDelegationRedemptionParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .cancel_delegation_redemption(params)
                .map(|response| delegation_redemption_cancellation_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_closeDelegationToDeposits" => {
            let params: DelegationLifecycleParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .close_delegation_to_deposits(params)
                .map(|response| delegation_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_beginDelegationWinding" => {
            let params: DelegationLifecycleParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .begin_delegation_winding(params)
                .map(|response| delegation_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_closeDelegation" => {
            let params: DelegationLifecycleParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .close_delegation(params)
                .map(|response| delegation_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        "kombat_submitDelegationAction" => {
            let params: SubmitDelegationActionParams =
                serde_json::from_value(params).map_err(|_| JsonRpcErrorObject {
                    code: -32602,
                    message: "invalid params".into(),
                    data: None,
                })?;
            service
                .submit_delegation_action(params)
                .map(|response| delegation_action_submission_response_to_json(&response))
                .map_err(map_rpc_error_to_json_rpc_error)
        }
        _ => Err(JsonRpcErrorObject {
            code: -32601,
            message: "method not found".into(),
            data: None,
        }),
    }
}

pub(crate) fn create_account_locally(
    service: &mut NodeRpcService<'_>,
    account_id: u64,
    owner: [u8; 32],
    code_hash: Hash,
    evm_address_hex: Option<&str>,
    timestamp_ms: u64,
) -> Result<Value, JsonRpcErrorObject> {
    if evm_address_hex.is_some() {
        return Err(map_rpc_error_to_json_rpc_error(RpcError::InvalidRequest(
            "evm address claim required",
        )));
    }
    let evm_address = evm_address_hex
        .map(decode_hex_array::<20>)
        .transpose()
        .map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?;
    service
        .create_account(account_id, owner, code_hash, evm_address, timestamp_ms)
        .map(|response| account_response_to_json(&response))
        .map_err(map_rpc_error_to_json_rpc_error)
}

pub(crate) fn create_evm_account_locally(
    service: &mut NodeRpcService<'_>,
    params: CreateEvmAccountParams,
) -> Result<Value, JsonRpcErrorObject> {
    let signed_account_claim = signed_evm_account_claim_from_json(&params.signed_account_claim)
        .map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?;
    service
        .create_evm_account(signed_account_claim, params.timestamp_ms)
        .map(|response| account_response_to_json(&response))
        .map_err(map_rpc_error_to_json_rpc_error)
}
