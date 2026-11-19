//! `kombat_loadgen` — a simple load generator for the & chain JSON-RPC API.
//!
//! Two workloads are supported:
//!
//! - `chain-info` — read-only `kombat_chainInfo` calls. Measures raw
//!   JSON-RPC serve throughput, no state setup required.
//! - `place-order` — signed `kombat_submitIntent` (PlaceOrder) calls.
//!   Requires a pre-existing account + policy + session key. Workers
//!   share an atomic nonce counter; the chain enforces strict monotonic
//!   nonces per policy, so concurrency > 1 will produce out-of-order
//!   rejections that are surfaced as a metric.
//!
//! Blocks are only produced by the RPC if auto-produce is on. To drive
//! the matching engine end-to-end, start the RPC with:
//!
//! ```text
//! KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS=50
//! KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS=512
//! ```
//!
//! Head-height delta is polled before/after the run to report engine-observed TPS.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use and_chain_sdk::{
    intent::{sign_intent, IntentKind, IntentRequest, OrderKind, OrderSide, TimeInForce},
    AndChainClient, JsonRpcErrorBody, Keypair, RpcError, SubmitError,
    SubmitIntentBatchItemResponse, SubmitIntentParams,
};
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{sync::Mutex, task::JoinSet, time::sleep};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Workload {
    /// Unauthenticated `kombat_chainInfo` reads.
    ChainInfo,
    /// Signed `kombat_submitIntent` PlaceOrder calls.
    PlaceOrder,
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum SideMode {
    /// All orders post on the bid side.
    Bid,
    /// All orders post on the ask side.
    Ask,
    /// Alternate bid/ask at non-crossing prices (bids priced below asks).
    /// Exercises book insertion on both sides without self-crossing the same
    /// account, which would otherwise trip self-trade prevention and stall
    /// the policy nonce frontier.
    Spread,
    /// Alternate bid/ask at the same price. Every other intent self-trades
    /// against its predecessor; the chain marks the self-trader's frontier
    /// nonce as `InternalError + SelfTradePrevented` and cascade-rejects any
    /// later queued nonces for that policy with `PredecessorIntentRejected`.
    /// Useful only for exercising the STP + cascade paths — not for
    /// measuring committed TPS.
    CrossSelf,
}

#[derive(Parser, Debug)]
#[command(
    name = "kombat_loadgen",
    about = "Load generator for the & chain JSON-RPC API",
    version
)]
struct Cli {
    /// Base URL of the RPC (no trailing slash).
    #[arg(
        long,
        default_value = "http://127.0.0.1:18545",
        env = "KOMBAT_LOADGEN_URL"
    )]
    url: String,

    /// Optional read/state URL. When set, bootstrap reads like
    /// `kombat_chainInfo` and `next_intent_nonce` are fetched from this URL
    /// instead of the write target. Useful when writes go through a dedicated
    /// ingress node while state remains freshest on validators/read nodes.
    #[arg(long, env = "KOMBAT_LOADGEN_STATE_URL")]
    state_url: Option<String>,

    /// Workload to run.
    #[arg(long, value_enum, default_value_t = Workload::ChainInfo)]
    workload: Workload,

    /// Target aggregate requests per second across all workers.
    /// `0` means unlimited — workers fire as fast as responses come back.
    #[arg(long, default_value_t = 0)]
    rps: u64,

    /// Number of concurrent workers.
    #[arg(long, default_value_t = 16)]
    concurrency: usize,

    /// Maximum in-flight submit requests per worker. Values greater than 1
    /// make `place-order` open-loop enough to build backlog instead of
    /// waiting for each response before issuing the next nonce.
    #[arg(long, default_value_t = 1)]
    pipeline_depth: usize,

    /// Number of intents to send in each submit RPC. Values greater than 1
    /// use `kombat_submitIntentBatch` and amortize one HTTP/JSON-RPC roundtrip
    /// across multiple intents.
    #[arg(long, default_value_t = 1)]
    batch_size: usize,

    /// Preserve per-writer nonce continuity by retrying the same signed batch
    /// on transport errors instead of advancing to later nonces. Intended for
    /// live benchmarks, where nonce holes make committed TPS meaningless.
    /// Requires `--pipeline-depth 1`.
    #[arg(long, default_value_t = false)]
    strict_nonce_retry: bool,

    /// Maximum strict nonce retry attempts per batch, including the first
    /// send. Only used with `--strict-nonce-retry`.
    #[arg(long, default_value_t = 4)]
    strict_nonce_retry_attempts: usize,

    /// Exit non-zero if the run records any nonce, RPC, or network error.
    /// Live benchmark wrappers use this to avoid keeping invalid artifacts.
    #[arg(long, default_value_t = false)]
    fail_on_errors: bool,

    /// How long to run, in seconds.
    #[arg(long, default_value_t = 30)]
    duration_secs: u64,

    /// Warmup seconds — excluded from the final stats.
    #[arg(long, default_value_t = 3)]
    warmup_secs: u64,

    /// Print a live progress line every N seconds.
    #[arg(long, default_value_t = 5)]
    progress_every_secs: u64,

    /// Optional per-request HTTP timeout for each RPC call.
    #[arg(long)]
    request_timeout_secs: Option<u64>,

    // --- Signing flags (place-order only) ---
    /// Chain id for signed intents.
    #[arg(long, default_value_t = 7)]
    chain_id: u64,

    /// Account id (owner of the policy).
    #[arg(long)]
    account_id: Option<u64>,

    /// Policy id to reuse. Session key must already be the grantee of this policy.
    #[arg(long)]
    policy_id: Option<u64>,

    /// Session-key secret seed, 32-byte hex (with or without `0x`).
    #[arg(long, env = "KOMBAT_LOADGEN_SESSION_SECRET_HEX")]
    session_secret_hex: Option<String>,

    /// JSON file containing an array of writer specs for `place-order`
    /// workloads. Each entry must contain `account_id`, `policy_id`, and
    /// `session_secret_hex`. When set, each worker uses a distinct writer so
    /// live committed load can scale without per-policy nonce reordering.
    #[arg(long, env = "KOMBAT_LOADGEN_WRITERS_JSON")]
    writers_json: Option<String>,

    /// Market id to post PlaceOrder intents on.
    #[arg(long, default_value_t = 1)]
    market_id: u64,

    /// Number of consecutive markets to distribute PlaceOrder workers across.
    /// `1` preserves the historical single-market benchmark. Values greater
    /// than one assign worker N to `market_id + (N % market_count)`.
    #[arg(long, default_value_t = 1)]
    market_count: u64,

    /// Side mode for generated PlaceOrder intents. Default `bid` produces a
    /// one-sided resting book on a single account, which avoids self-trade
    /// prevention and keeps the policy nonce frontier advancing.
    #[arg(long, value_enum, default_value_t = SideMode::Bid)]
    side: SideMode,

    /// Limit price (in price ticks) for all generated orders. For `spread`
    /// mode, this is the mid — bids post at `price_ticks - spread_ticks`
    /// and asks at `price_ticks + spread_ticks`.
    #[arg(long, default_value_t = 1_000)]
    price_ticks: u64,

    /// Half-spread (in price ticks) used only when `--side spread`.
    #[arg(long, default_value_t = 100)]
    spread_ticks: u64,

    /// Order quantity in lots.
    #[arg(long, default_value_t = 1)]
    quantity_lots: u64,

    /// Per-intent expires-at offset from now, in ms.
    #[arg(long, default_value_t = 60_000)]
    intent_ttl_ms: u64,

    /// Directory for reusable prebuilt JSON-RPC payload bodies. Only supported
    /// for non-strict batched place-order runs.
    #[arg(long, env = "KOMBAT_LOADGEN_PAYLOAD_CACHE_DIR")]
    payload_cache_dir: Option<PathBuf>,

    /// Optional deterministic base for generated client_order_id values.
    /// Useful with payload caching because cached signed intents include the
    /// client order id.
    #[arg(long, env = "KOMBAT_LOADGEN_CLIENT_ORDER_ID_BASE")]
    client_order_id_base: Option<u64>,
}

#[derive(Default)]
struct Counters {
    sent: AtomicU64,
    ok: AtomicU64,
    rpc_err: AtomicU64,
    nonce_err: AtomicU64,
    net_err: AtomicU64,
}

struct Recorder {
    counters: Counters,
    // Only latencies from requests issued after warmup are recorded here.
    latencies_ns: Mutex<Vec<u64>>,
    error_samples: Mutex<ErrorSamples>,
    warmup_deadline: Instant,
}

#[derive(Default)]
struct ErrorSamples {
    nonce_err: Vec<String>,
    rpc_err: Vec<String>,
    net_err: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PlaceOrderWriterSpec {
    account_id: u64,
    policy_id: u64,
    session_secret_hex: String,
}

struct PlaceOrderWriter {
    account_id: u64,
    policy_id: u64,
    signer: Arc<Keypair>,
    nonce_cursor: Arc<AtomicU64>,
    client_order_cursor: Arc<AtomicU64>,
}

#[derive(Clone)]
struct RawBatchClient {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawJsonRpcResponse<T> {
    Success { result: T },
    Failure { error: JsonRpcErrorBody },
}

impl RawBatchClient {
    fn new(base_url: impl Into<String>, timeout: Option<Duration>) -> Self {
        let mut builder = reqwest::Client::builder();
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        Self {
            base_url: base_url.into(),
            client: builder
                .build()
                .expect("raw loadgen http client should build"),
        }
    }

    async fn submit_cached_batch(
        &self,
        body: Vec<u8>,
    ) -> Result<and_chain_sdk::SubmitIntentBatchResponse, RpcError> {
        let response = self
            .client
            .post(&self.base_url)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await?;
        let payload: RawJsonRpcResponse<and_chain_sdk::SubmitIntentBatchResponse> =
            response.json().await?;
        match payload {
            RawJsonRpcResponse::Success { result } => Ok(result),
            RawJsonRpcResponse::Failure { error } => Err(RpcError::JsonRpc {
                code: error.code,
                message: error.message,
                data: error.data,
            }),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct PayloadCacheKey {
    version: u32,
    chain_id: u64,
    account_id: u64,
    policy_id: u64,
    start_nonce: u64,
    start_client_order_id: u64,
    market_id: u64,
    side: SideMode,
    price_ticks: u64,
    spread_ticks: u64,
    quantity_lots: u64,
    intent_ttl_ms: u64,
    batch_size: usize,
}

#[derive(Debug, Deserialize, Serialize)]
struct PayloadCacheFile {
    key: PayloadCacheKey,
    created_at_ms: u64,
    expires_at_ms: u64,
    bodies: Vec<String>,
}

impl Recorder {
    fn new(warmup_deadline: Instant) -> Self {
        Self {
            counters: Counters::default(),
            latencies_ns: Mutex::new(Vec::with_capacity(1 << 16)),
            error_samples: Mutex::new(ErrorSamples::default()),
            warmup_deadline,
        }
    }

    async fn observe(&self, started_at: Instant, outcome: Outcome, error_detail: Option<String>) {
        self.counters.sent.fetch_add(1, Ordering::Relaxed);
        match outcome {
            Outcome::Ok => {
                self.counters.ok.fetch_add(1, Ordering::Relaxed);
            }
            Outcome::NonceErr => {
                self.counters.nonce_err.fetch_add(1, Ordering::Relaxed);
            }
            Outcome::RpcErr => {
                self.counters.rpc_err.fetch_add(1, Ordering::Relaxed);
            }
            Outcome::NetErr => {
                self.counters.net_err.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Some(error_detail) = error_detail {
            self.record_error_sample(outcome, error_detail).await;
        }
        if started_at >= self.warmup_deadline {
            let dt = Instant::now().saturating_duration_since(started_at);
            let ns = dt.as_nanos().min(u64::MAX as u128) as u64;
            self.latencies_ns.lock().await.push(ns);
        }
    }

    async fn record_error_sample(&self, outcome: Outcome, detail: String) {
        const MAX_SAMPLES: usize = 5;

        let mut samples = self.error_samples.lock().await;
        let bucket = match outcome {
            Outcome::NonceErr => &mut samples.nonce_err,
            Outcome::RpcErr => &mut samples.rpc_err,
            Outcome::NetErr => &mut samples.net_err,
            Outcome::Ok => return,
        };
        if bucket.len() < MAX_SAMPLES {
            bucket.push(detail);
        }
    }
}

#[derive(Copy, Clone)]
enum Outcome {
    Ok,
    NonceErr,
    RpcErr,
    NetErr,
}

fn classify(err: &RpcError) -> Outcome {
    match err {
        RpcError::Http(_) => Outcome::NetErr,
        RpcError::JsonRpc { data, message, .. } => {
            let reason = data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(|r| r.as_str())
                .unwrap_or("");
            let msg = message.as_str();
            if reason.contains("nonce") || msg.contains("nonce") {
                Outcome::NonceErr
            } else {
                Outcome::RpcErr
            }
        }
    }
}

fn classify_submit(err: &SubmitError) -> Outcome {
    match err {
        SubmitError::Rpc(rpc) => classify(rpc),
        // Signing errors are client-side bugs, not chain responses — bucket as rpc_err.
        _ => Outcome::RpcErr,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn summarize_rpc_error(err: &RpcError) -> String {
    match err {
        RpcError::Http(error) => format!(
            "http: {error} timeout={} connect={} request={} body={} decode={}",
            error.is_timeout(),
            error.is_connect(),
            error.is_request(),
            error.is_body(),
            error.is_decode(),
        ),
        RpcError::JsonRpc {
            code,
            message,
            data,
        } => match data {
            Some(data) => format!("jsonrpc code={code} message={message} data={data}"),
            None => format!("jsonrpc code={code} message={message}"),
        },
    }
}

fn summarize_submit_error(err: &SubmitError) -> String {
    match err {
        SubmitError::Rpc(rpc) => summarize_rpc_error(rpc),
        SubmitError::SignIntent(error) => format!("sign intent: {error}"),
        SubmitError::SignOwnerCall(error) => format!("sign owner call: {error}"),
    }
}

fn classify_jsonrpc_error_body(error: &JsonRpcErrorBody) -> Outcome {
    let rpc = RpcError::JsonRpc {
        code: error.code,
        message: error.message.clone(),
        data: error.data.clone(),
    };
    classify(&rpc)
}

fn summarize_jsonrpc_error_body(error: &JsonRpcErrorBody) -> String {
    let rpc = RpcError::JsonRpc {
        code: error.code,
        message: error.message.clone(),
        data: error.data.clone(),
    };
    summarize_rpc_error(&rpc)
}

fn classify_batch_item(item: SubmitIntentBatchItemResponse) -> (Outcome, Option<String>) {
    if item.accepted {
        return (Outcome::Ok, None);
    }
    match item.error {
        Some(error) => {
            let outcome = classify_jsonrpc_error_body(&error);
            let detail = summarize_jsonrpc_error_body(&error);
            (outcome, Some(detail))
        }
        None => (
            Outcome::RpcErr,
            Some("jsonrpc batch item missing error payload".into()),
        ),
    }
}

fn jsonrpc_error_reason(error: &JsonRpcErrorBody) -> Option<&str> {
    error
        .data
        .as_ref()
        .and_then(|data| data.get("reason"))
        .and_then(|reason| reason.as_str())
}

fn strict_retry_benign_item(item: &SubmitIntentBatchItemResponse) -> bool {
    if item.accepted {
        return true;
    }
    let Some(error) = item.error.as_ref() else {
        return false;
    };
    matches!(
        jsonrpc_error_reason(error),
        Some("duplicate_intent" | "stale_intent_nonce")
    )
}

fn observe_batch_as_ok(
    results: &[SubmitIntentBatchItemResponse],
) -> Vec<(Outcome, Option<String>)> {
    results.iter().map(|_| (Outcome::Ok, None)).collect()
}

fn load_place_order_writer_specs(cli: &Cli) -> Result<Vec<PlaceOrderWriterSpec>> {
    if let Some(path) = cli.writers_json.as_deref() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read writers json: {path}"))?;
        let specs: Vec<PlaceOrderWriterSpec> =
            serde_json::from_str(&raw).with_context(|| format!("invalid writers json: {path}"))?;
        return Ok(specs);
    }

    let account_id = cli
        .account_id
        .context("--account-id is required for place-order")?;
    let policy_id = cli
        .policy_id
        .context("--policy-id is required for place-order")?;
    let session_secret_hex = cli
        .session_secret_hex
        .clone()
        .context("--session-secret-hex is required for place-order")?;
    Ok(vec![PlaceOrderWriterSpec {
        account_id,
        policy_id,
        session_secret_hex,
    }])
}

async fn run_chain_info_worker(
    client: Arc<AndChainClient>,
    recorder: Arc<Recorder>,
    run_until: Instant,
    per_worker_interval: Option<Duration>,
) {
    loop {
        let now = Instant::now();
        if now >= run_until {
            break;
        }
        let started = Instant::now();
        let (outcome, error_detail) = match client.chain_info().await {
            Ok(_) => (Outcome::Ok, None),
            Err(err) => (classify(&err), Some(summarize_rpc_error(&err))),
        };
        recorder.observe(started, outcome, error_detail).await;

        if let Some(interval) = per_worker_interval {
            let elapsed = started.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    }
}

fn resolve_side_and_price(
    mode: SideMode,
    client_order_id: u64,
    price_ticks: u64,
    spread_ticks: u64,
) -> (OrderSide, u64) {
    match mode {
        SideMode::Bid => (OrderSide::Bid, price_ticks),
        SideMode::Ask => (OrderSide::Ask, price_ticks),
        SideMode::Spread => {
            if client_order_id & 1 == 0 {
                (OrderSide::Bid, price_ticks.saturating_sub(spread_ticks))
            } else {
                (OrderSide::Ask, price_ticks.saturating_add(spread_ticks))
            }
        }
        SideMode::CrossSelf => {
            if client_order_id & 1 == 0 {
                (OrderSide::Bid, price_ticks)
            } else {
                (OrderSide::Ask, price_ticks)
            }
        }
    }
}

fn required_cached_batch_count(
    total_run: Duration,
    per_worker_interval: Duration,
    pipeline_depth: usize,
) -> usize {
    let scheduled = (total_run.as_secs_f64() / per_worker_interval.as_secs_f64()).ceil() as usize;
    scheduled.saturating_add(pipeline_depth).saturating_add(8)
}

fn payload_cache_path(cache_dir: &Path, worker_id: usize, policy_id: u64) -> PathBuf {
    cache_dir.join(format!("worker-{worker_id:04}-policy-{policy_id}.json"))
}

#[allow(clippy::too_many_arguments)]
fn build_cached_payload_bodies(
    key: &PayloadCacheKey,
    batch_count: usize,
    timestamp_ms: u64,
    signer: &Keypair,
) -> Result<Vec<String>> {
    let expires_at = timestamp_ms.saturating_add(key.intent_ttl_ms);
    let mut bodies = Vec::with_capacity(batch_count);
    for batch_index in 0..batch_count {
        let mut items = Vec::with_capacity(key.batch_size);
        for offset in 0..key.batch_size {
            let sequence = batch_index
                .saturating_mul(key.batch_size)
                .saturating_add(offset);
            let nonce = key.start_nonce.saturating_add(sequence as u64);
            let client_order_id = key.start_client_order_id.saturating_add(sequence as u64);
            let (side, order_price_ticks) = resolve_side_and_price(
                key.side,
                client_order_id,
                key.price_ticks,
                key.spread_ticks,
            );
            let request = IntentRequest {
                chain_id: key.chain_id,
                account_id: key.account_id,
                policy_id: key.policy_id,
                nonce,
                expires_at,
                predicate: None,
                kind: IntentKind::PlaceOrder {
                    market_id: key.market_id,
                    client_order_id,
                    side,
                    quantity_lots: key.quantity_lots,
                    kind: OrderKind::Limit {
                        price_ticks: order_price_ticks,
                    },
                    time_in_force: TimeInForce::Gtc,
                    post_only: false,
                },
            };
            items.push(SubmitIntentParams {
                signed_intent: sign_intent(signer, request)?,
                timestamp_ms,
            });
        }
        bodies.push(serde_json::to_string(&json!({
            "jsonrpc": "2.0",
            "id": 1u64,
            "method": "kombat_submitIntentBatch",
            "params": {
                "items": items,
            },
        }))?);
    }
    Ok(bodies)
}

fn load_or_build_cached_payload_bodies(
    cache_dir: &Path,
    worker_id: usize,
    key: PayloadCacheKey,
    batch_count: usize,
    signer: &Keypair,
) -> Result<Vec<String>> {
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create payload cache dir {}", cache_dir.display()))?;
    let path = payload_cache_path(cache_dir, worker_id, key.policy_id);
    let now = now_ms();
    let min_expires_at = now.saturating_add(120_000);

    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(file) = serde_json::from_str::<PayloadCacheFile>(&raw) {
            if file.key == key
                && file.bodies.len() >= batch_count
                && file.expires_at_ms > min_expires_at
            {
                eprintln!(
                    "payload-cache hit: worker={} policy={} path={} bodies={} using={}",
                    worker_id,
                    key.policy_id,
                    path.display(),
                    file.bodies.len(),
                    batch_count
                );
                return Ok(file.bodies.into_iter().take(batch_count).collect());
            }
        }
    }

    let timestamp_ms = now;
    let bodies = build_cached_payload_bodies(&key, batch_count, timestamp_ms, signer)?;
    let file = PayloadCacheFile {
        key: key.clone(),
        created_at_ms: timestamp_ms,
        expires_at_ms: timestamp_ms.saturating_add(key.intent_ttl_ms),
        bodies: bodies.clone(),
    };
    std::fs::write(&path, serde_json::to_vec(&file)?)
        .with_context(|| format!("failed to write payload cache {}", path.display()))?;
    eprintln!(
        "payload-cache built: worker={} policy={} path={} bodies={} expires_at_ms={}",
        worker_id,
        key.policy_id,
        path.display(),
        batch_count,
        file.expires_at_ms
    );
    Ok(bodies)
}

async fn run_cached_place_order_worker(
    client: Arc<RawBatchClient>,
    recorder: Arc<Recorder>,
    run_until: Instant,
    per_worker_interval: Duration,
    pipeline_depth: usize,
    batch_size: usize,
    bodies: Arc<Vec<String>>,
) {
    let mut in_flight = JoinSet::new();
    let mut next_dispatch_at = Instant::now();
    let mut next_body = 0usize;

    loop {
        let now = Instant::now();
        let can_dispatch = now < run_until && in_flight.len() < pipeline_depth;
        if can_dispatch && now >= next_dispatch_at {
            let Some(body) = bodies.get(next_body).cloned() else {
                recorder
                    .observe(
                        Instant::now(),
                        Outcome::RpcErr,
                        Some("payload cache exhausted before run completed".into()),
                    )
                    .await;
                break;
            };
            next_body = next_body.saturating_add(1);
            let client = client.clone();
            in_flight.spawn(async move {
                let started = Instant::now();
                let outcomes = match client.submit_cached_batch(body.into_bytes()).await {
                    Ok(response) => response
                        .results
                        .into_iter()
                        .map(classify_batch_item)
                        .collect::<Vec<_>>(),
                    Err(err) => {
                        let outcome = classify(&err);
                        let detail = summarize_rpc_error(&err);
                        vec![(outcome, Some(detail)); batch_size]
                    }
                };
                (started, outcomes)
            });
            next_dispatch_at += per_worker_interval;
            continue;
        }

        if now >= run_until && in_flight.is_empty() {
            break;
        }

        if in_flight.is_empty() {
            let now = Instant::now();
            if next_dispatch_at > now {
                sleep(next_dispatch_at - now).await;
            } else {
                tokio::task::yield_now().await;
            }
            continue;
        }

        if can_dispatch {
            let now = Instant::now();
            let wait_for_dispatch = if next_dispatch_at > now {
                next_dispatch_at - now
            } else {
                Duration::ZERO
            };
            tokio::select! {
                joined = in_flight.join_next() => {
                    if let Some(Ok((started, outcomes))) = joined {
                        for (outcome, error_detail) in outcomes {
                            recorder.observe(started, outcome, error_detail).await;
                        }
                    }
                }
                _ = sleep(wait_for_dispatch) => {}
            }
            continue;
        }

        if let Some(Ok((started, outcomes))) = in_flight.join_next().await {
            for (outcome, error_detail) in outcomes {
                recorder.observe(started, outcome, error_detail).await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_place_order_worker(
    client: Arc<AndChainClient>,
    recorder: Arc<Recorder>,
    run_until: Instant,
    per_worker_interval: Option<Duration>,
    signer: Arc<Keypair>,
    nonce_cursor: Arc<AtomicU64>,
    client_order_cursor: Arc<AtomicU64>,
    chain_id: u64,
    account_id: u64,
    policy_id: u64,
    market_id: u64,
    side_mode: SideMode,
    price_ticks: u64,
    spread_ticks: u64,
    quantity_lots: u64,
    intent_ttl_ms: u64,
    pipeline_depth: usize,
    batch_size: usize,
) {
    let mut in_flight = JoinSet::new();
    let mut next_dispatch_at = Instant::now();

    loop {
        let now = Instant::now();
        let can_dispatch = now < run_until && in_flight.len() < pipeline_depth;
        let pacing_ready = per_worker_interval.is_none_or(|_| now >= next_dispatch_at);
        if can_dispatch && pacing_ready {
            let ts_ms = now_ms();
            let expires_at = ts_ms.saturating_add(intent_ttl_ms);
            let mut requests = Vec::with_capacity(batch_size);
            for _ in 0..batch_size {
                let nonce = nonce_cursor.fetch_add(1, Ordering::Relaxed);
                let client_order_id = client_order_cursor.fetch_add(1, Ordering::Relaxed);
                let (side, order_price_ticks) =
                    resolve_side_and_price(side_mode, client_order_id, price_ticks, spread_ticks);
                requests.push(IntentRequest {
                    chain_id,
                    account_id,
                    policy_id,
                    nonce,
                    expires_at,
                    predicate: None,
                    kind: IntentKind::PlaceOrder {
                        market_id,
                        client_order_id,
                        side,
                        quantity_lots,
                        kind: OrderKind::Limit {
                            price_ticks: order_price_ticks,
                        },
                        time_in_force: TimeInForce::Gtc,
                        post_only: false,
                    },
                });
            }
            let client = client.clone();
            let signer = signer.clone();
            in_flight.spawn(async move {
                let started = Instant::now();
                let outcomes = if requests.len() == 1 {
                    let request = requests.into_iter().next().expect("single request");
                    match client
                        .submit_typed_intent(signer.as_ref(), request, ts_ms)
                        .await
                    {
                        Ok(_) => vec![(Outcome::Ok, None)],
                        Err(err) => {
                            vec![(classify_submit(&err), Some(summarize_submit_error(&err)))]
                        }
                    }
                } else {
                    let mut items = Vec::with_capacity(requests.len());
                    for request in requests {
                        match sign_intent(signer.as_ref(), request) {
                            Ok(signed_intent) => items.push(SubmitIntentParams {
                                signed_intent,
                                timestamp_ms: ts_ms,
                            }),
                            Err(err) => {
                                return (
                                    started,
                                    vec![(Outcome::RpcErr, Some(format!("sign intent: {err}")))],
                                );
                            }
                        }
                    }
                    match client.submit_intent_batch(items).await {
                        Ok(response) => response
                            .results
                            .into_iter()
                            .map(classify_batch_item)
                            .collect::<Vec<_>>(),
                        Err(err) => {
                            let outcome = classify(&err);
                            let detail = summarize_rpc_error(&err);
                            vec![(outcome, Some(detail)); batch_size]
                        }
                    }
                };
                (started, outcomes)
            });
            if let Some(interval) = per_worker_interval {
                next_dispatch_at += interval;
            }
            continue;
        }

        if now >= run_until && in_flight.is_empty() {
            break;
        }

        if in_flight.is_empty() {
            if per_worker_interval.is_some() {
                let now = Instant::now();
                if next_dispatch_at > now {
                    sleep(next_dispatch_at - now).await;
                }
            } else {
                tokio::task::yield_now().await;
            }
            continue;
        }

        if can_dispatch && per_worker_interval.is_some() {
            let now = Instant::now();
            let wait_for_dispatch = if next_dispatch_at > now {
                next_dispatch_at - now
            } else {
                Duration::ZERO
            };
            tokio::select! {
                joined = in_flight.join_next() => {
                    if let Some(Ok((started, outcomes))) = joined {
                        for (outcome, error_detail) in outcomes {
                            recorder.observe(started, outcome, error_detail).await;
                        }
                    }
                }
                _ = sleep(wait_for_dispatch) => {}
            }
            continue;
        }

        if let Some(Ok((started, outcomes))) = in_flight.join_next().await {
            for (outcome, error_detail) in outcomes {
                recorder.observe(started, outcome, error_detail).await;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_strict_place_order_worker(
    client: Arc<AndChainClient>,
    recorder: Arc<Recorder>,
    run_until: Instant,
    per_worker_interval: Option<Duration>,
    signer: Arc<Keypair>,
    nonce_cursor: Arc<AtomicU64>,
    client_order_cursor: Arc<AtomicU64>,
    chain_id: u64,
    account_id: u64,
    policy_id: u64,
    market_id: u64,
    side_mode: SideMode,
    price_ticks: u64,
    spread_ticks: u64,
    quantity_lots: u64,
    intent_ttl_ms: u64,
    batch_size: usize,
    retry_attempts: usize,
) {
    let retry_attempts = retry_attempts.max(1);
    let mut next_dispatch_at = Instant::now();

    loop {
        let now = Instant::now();
        if now >= run_until {
            break;
        }

        if let Some(interval) = per_worker_interval {
            if next_dispatch_at > now {
                sleep(next_dispatch_at - now).await;
            }
            next_dispatch_at += interval;
        }

        let started = Instant::now();
        let ts_ms = now_ms();
        let expires_at = ts_ms.saturating_add(intent_ttl_ms);
        let base_nonce = nonce_cursor.load(Ordering::Relaxed);
        let mut items = Vec::with_capacity(batch_size);

        for offset in 0..batch_size {
            let nonce = base_nonce.saturating_add(offset as u64);
            let client_order_id = client_order_cursor.fetch_add(1, Ordering::Relaxed);
            let (side, order_price_ticks) =
                resolve_side_and_price(side_mode, client_order_id, price_ticks, spread_ticks);
            let request = IntentRequest {
                chain_id,
                account_id,
                policy_id,
                nonce,
                expires_at,
                predicate: None,
                kind: IntentKind::PlaceOrder {
                    market_id,
                    client_order_id,
                    side,
                    quantity_lots,
                    kind: OrderKind::Limit {
                        price_ticks: order_price_ticks,
                    },
                    time_in_force: TimeInForce::Gtc,
                    post_only: false,
                },
            };
            match sign_intent(signer.as_ref(), request) {
                Ok(signed_intent) => items.push(SubmitIntentParams {
                    signed_intent,
                    timestamp_ms: ts_ms,
                }),
                Err(err) => {
                    recorder
                        .observe(
                            started,
                            Outcome::RpcErr,
                            Some(format!("sign intent: {err}")),
                        )
                        .await;
                    return;
                }
            }
        }

        let mut attempt = 0usize;
        loop {
            attempt += 1;
            match client.submit_intent_batch(items.clone()).await {
                Ok(response) => {
                    if response.results.len() != batch_size {
                        let detail = format!(
                            "jsonrpc batch response length mismatch: expected={} actual={}",
                            batch_size,
                            response.results.len()
                        );
                        for _ in 0..batch_size {
                            recorder
                                .observe(started, Outcome::RpcErr, Some(detail.clone()))
                                .await;
                        }
                        return;
                    }

                    let all_accepted = response.results.iter().all(|item| item.accepted);
                    let all_benign_replay =
                        attempt > 1 && response.results.iter().all(strict_retry_benign_item);
                    if all_accepted || all_benign_replay {
                        nonce_cursor.store(
                            base_nonce.saturating_add(batch_size as u64),
                            Ordering::Relaxed,
                        );
                        for (outcome, detail) in observe_batch_as_ok(&response.results) {
                            recorder.observe(started, outcome, detail).await;
                        }
                        break;
                    }

                    for (outcome, detail) in response.results.into_iter().map(classify_batch_item) {
                        recorder.observe(started, outcome, detail).await;
                    }
                    return;
                }
                Err(err) => {
                    let outcome = classify(&err);
                    let detail = summarize_rpc_error(&err);
                    if matches!(outcome, Outcome::NetErr) && attempt < retry_attempts {
                        eprintln!(
                            "strict nonce retry: policy_id={} nonce_start={} batch_size={} attempt={}/{} error={}",
                            policy_id,
                            base_nonce,
                            batch_size,
                            attempt,
                            retry_attempts,
                            detail
                        );
                        sleep(Duration::from_millis((25 * attempt as u64).min(250))).await;
                        continue;
                    }

                    for _ in 0..batch_size {
                        recorder
                            .observe(started, outcome, Some(detail.clone()))
                            .await;
                    }
                    return;
                }
            }
        }
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64) * p).ceil() as usize;
    let idx = idx.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

fn fmt_ns(ns: u64) -> String {
    let ms = (ns as f64) / 1_000_000.0;
    format!("{ms:.3}ms")
}

async fn print_progress(
    recorder: Arc<Recorder>,
    every: Duration,
    start: Instant,
    run_until: Instant,
) {
    let mut last_sent: u64 = 0;
    let mut last_tick = start;
    loop {
        sleep(every).await;
        let now = Instant::now();
        if now >= run_until {
            break;
        }
        let sent = recorder.counters.sent.load(Ordering::Relaxed);
        let ok = recorder.counters.ok.load(Ordering::Relaxed);
        let nonce_err = recorder.counters.nonce_err.load(Ordering::Relaxed);
        let rpc_err = recorder.counters.rpc_err.load(Ordering::Relaxed);
        let net_err = recorder.counters.net_err.load(Ordering::Relaxed);

        let dt = now
            .saturating_duration_since(last_tick)
            .as_secs_f64()
            .max(0.001);
        let inst_rps = (sent.saturating_sub(last_sent)) as f64 / dt;
        last_sent = sent;
        last_tick = now;

        let elapsed = now.saturating_duration_since(start).as_secs_f64();
        eprintln!(
            "[{elapsed:>5.1}s] sent={sent} ok={ok} nonce_err={nonce_err} rpc_err={rpc_err} net_err={net_err} inst_rps={inst_rps:.1}"
        );
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    anyhow::ensure!(cli.concurrency > 0, "--concurrency must be greater than 0");
    anyhow::ensure!(
        cli.pipeline_depth > 0,
        "--pipeline-depth must be greater than 0"
    );
    anyhow::ensure!(cli.batch_size > 0, "--batch-size must be greater than 0");
    if cli.strict_nonce_retry {
        anyhow::ensure!(
            cli.pipeline_depth == 1,
            "--strict-nonce-retry requires --pipeline-depth 1"
        );
        anyhow::ensure!(
            cli.strict_nonce_retry_attempts > 0,
            "--strict-nonce-retry-attempts must be greater than 0"
        );
    }

    let client = Arc::new(match cli.request_timeout_secs {
        Some(timeout_secs) => AndChainClient::new(cli.url.clone())
            .with_timeout(Duration::from_secs(timeout_secs.max(1))),
        None => AndChainClient::new(cli.url.clone()),
    });
    let state_url = cli.state_url.clone().unwrap_or_else(|| cli.url.clone());
    let state_client = Arc::new(match cli.request_timeout_secs {
        Some(timeout_secs) => AndChainClient::new(state_url.clone())
            .with_timeout(Duration::from_secs(timeout_secs.max(1))),
        None => AndChainClient::new(state_url.clone()),
    });
    let raw_batch_client = Arc::new(RawBatchClient::new(
        cli.url.clone(),
        cli.request_timeout_secs
            .map(|timeout_secs| Duration::from_secs(timeout_secs.max(1))),
    ));

    // Sanity check: talk to the server before we spin up workers.
    let pre_info = state_client
        .chain_info()
        .await
        .with_context(|| format!("chain_info probe against {} failed", state_url))?;
    let head_height_before = pre_info.head_height;

    let total_run = Duration::from_secs(cli.duration_secs + cli.warmup_secs);
    let start = Instant::now();
    let run_until = start + total_run;
    let warmup_deadline = start + Duration::from_secs(cli.warmup_secs);

    // Per-worker interval to achieve the aggregate target rps.
    let per_worker_interval: Option<Duration> = if cli.rps == 0 {
        None
    } else {
        let aggregate = cli.rps as f64;
        let per_worker =
            aggregate / (cli.concurrency as f64).max(1.0) / (cli.batch_size as f64).max(1.0);
        if per_worker <= 0.0 {
            None
        } else {
            Some(Duration::from_secs_f64(1.0 / per_worker))
        }
    };

    let recorder = Arc::new(Recorder::new(warmup_deadline));

    let progress = tokio::spawn(print_progress(
        recorder.clone(),
        Duration::from_secs(cli.progress_every_secs.max(1)),
        start,
        run_until,
    ));

    eprintln!(
        "kombat_loadgen starting: url={} state_url={} workload={:?} concurrency={} pipeline_depth={} batch_size={} strict_nonce_retry={} retry_attempts={} fail_on_errors={} rps={} warmup={}s duration={}s head_height_before={}",
        cli.url,
        state_url,
        cli.workload,
        cli.concurrency,
        cli.pipeline_depth,
        cli.batch_size,
        cli.strict_nonce_retry,
        cli.strict_nonce_retry_attempts,
        cli.fail_on_errors,
        cli.rps,
        cli.warmup_secs,
        cli.duration_secs,
        head_height_before
    );

    let mut handles = Vec::with_capacity(cli.concurrency);

    match cli.workload {
        Workload::ChainInfo => {
            for _ in 0..cli.concurrency {
                let client = client.clone();
                let recorder = recorder.clone();
                handles.push(tokio::spawn(async move {
                    run_chain_info_worker(client, recorder, run_until, per_worker_interval).await;
                }));
            }
        }
        Workload::PlaceOrder => {
            let writer_specs = load_place_order_writer_specs(&cli)?;
            let payload_cache_dir = cli.payload_cache_dir.clone();
            if payload_cache_dir.is_some() {
                anyhow::ensure!(
                    !cli.strict_nonce_retry,
                    "--payload-cache-dir is only supported for non-strict place-order runs"
                );
                anyhow::ensure!(
                    cli.batch_size > 1,
                    "--payload-cache-dir is intended for batched place-order runs"
                );
                anyhow::ensure!(
                    cli.rps > 0,
                    "--payload-cache-dir requires a finite --rps so the cache size is bounded"
                );
                anyhow::ensure!(
                    cli.intent_ttl_ms >= 86_400_000,
                    "--payload-cache-dir requires --intent-ttl-ms >= 86400000 so cached intents survive later reset/replay runs"
                );
            }
            anyhow::ensure!(
                cli.market_count > 0,
                "--market-count must be greater than zero"
            );
            if cli.writers_json.is_some() {
                anyhow::ensure!(
                    writer_specs.len() >= cli.concurrency,
                    "writers json only provides {} writers but --concurrency={} requires one distinct writer per worker",
                    writer_specs.len(),
                    cli.concurrency
                );
            }
            if cli.strict_nonce_retry {
                anyhow::ensure!(
                    cli.writers_json.is_some() || cli.concurrency == 1,
                    "--strict-nonce-retry with --concurrency > 1 requires --writers-json so each worker owns one policy nonce stream"
                );
            }

            let mut writers = Vec::with_capacity(writer_specs.len());
            for (idx, spec) in writer_specs.into_iter().enumerate() {
                let signer = Arc::new(
                    Keypair::from_secret_hex(&spec.session_secret_hex)
                        .with_context(|| format!("failed to parse writer[{idx}] session secret"))?,
                );
                let start_nonce = state_client
                    .next_intent_nonce(spec.policy_id)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to fetch next_intent_nonce for writer[{idx}] policy {}",
                            spec.policy_id
                        )
                    })?;
                writers.push(PlaceOrderWriter {
                    account_id: spec.account_id,
                    policy_id: spec.policy_id,
                    signer,
                    nonce_cursor: Arc::new(AtomicU64::new(start_nonce)),
                    client_order_cursor: Arc::new(AtomicU64::new(
                        cli.client_order_id_base
                            .unwrap_or(if payload_cache_dir.is_some() {
                                1_000_000_000_000
                            } else {
                                now_ms()
                            })
                            .saturating_add((idx as u64) << 20),
                    )),
                });
            }

            eprintln!(
                "place-order bootstrap: writers={} market_id={} market_count={} side={:?} price_ticks={} spread_ticks={}",
                writers.len(),
                cli.market_id,
                cli.market_count,
                cli.side,
                cli.price_ticks,
                cli.spread_ticks
            );
            for (idx, writer) in writers.iter().enumerate().take(cli.concurrency) {
                eprintln!(
                    "  writer[{idx}]: account_id={} policy_id={} start_nonce={}",
                    writer.account_id,
                    writer.policy_id,
                    writer.nonce_cursor.load(Ordering::Relaxed)
                );
            }

            for worker_id in 0..cli.concurrency {
                let writer = &writers[worker_id % writers.len()];
                let client = client.clone();
                let recorder = recorder.clone();
                let signer = writer.signer.clone();
                let nonce_cursor = writer.nonce_cursor.clone();
                let client_order_cursor = writer.client_order_cursor.clone();
                let account_id = writer.account_id;
                let policy_id = writer.policy_id;
                let worker_market_id = cli
                    .market_id
                    .saturating_add((worker_id as u64) % cli.market_count);
                if let Some(payload_cache_dir) = payload_cache_dir.as_ref() {
                    let interval = per_worker_interval
                        .expect("payload cache requires finite rps and therefore an interval");
                    let batch_count =
                        required_cached_batch_count(total_run, interval, cli.pipeline_depth);
                    let key = PayloadCacheKey {
                        version: 1,
                        chain_id: cli.chain_id,
                        account_id,
                        policy_id,
                        start_nonce: writer.nonce_cursor.load(Ordering::Relaxed),
                        start_client_order_id: writer.client_order_cursor.load(Ordering::Relaxed),
                        market_id: worker_market_id,
                        side: cli.side,
                        price_ticks: cli.price_ticks,
                        spread_ticks: cli.spread_ticks,
                        quantity_lots: cli.quantity_lots,
                        intent_ttl_ms: cli.intent_ttl_ms,
                        batch_size: cli.batch_size,
                    };
                    let bodies = Arc::new(load_or_build_cached_payload_bodies(
                        payload_cache_dir,
                        worker_id,
                        key,
                        batch_count,
                        signer.as_ref(),
                    )?);
                    let raw_client = raw_batch_client.clone();
                    handles.push(tokio::spawn(async move {
                        run_cached_place_order_worker(
                            raw_client,
                            recorder,
                            run_until,
                            interval,
                            cli.pipeline_depth,
                            cli.batch_size,
                            bodies,
                        )
                        .await;
                    }));
                } else if cli.strict_nonce_retry {
                    handles.push(tokio::spawn(async move {
                        run_strict_place_order_worker(
                            client,
                            recorder,
                            run_until,
                            per_worker_interval,
                            signer,
                            nonce_cursor,
                            client_order_cursor,
                            cli.chain_id,
                            account_id,
                            policy_id,
                            worker_market_id,
                            cli.side,
                            cli.price_ticks,
                            cli.spread_ticks,
                            cli.quantity_lots,
                            cli.intent_ttl_ms,
                            cli.batch_size,
                            cli.strict_nonce_retry_attempts,
                        )
                        .await;
                    }));
                } else {
                    handles.push(tokio::spawn(async move {
                        run_place_order_worker(
                            client,
                            recorder,
                            run_until,
                            per_worker_interval,
                            signer,
                            nonce_cursor,
                            client_order_cursor,
                            cli.chain_id,
                            account_id,
                            policy_id,
                            worker_market_id,
                            cli.side,
                            cli.price_ticks,
                            cli.spread_ticks,
                            cli.quantity_lots,
                            cli.intent_ttl_ms,
                            cli.pipeline_depth,
                            cli.batch_size,
                        )
                        .await;
                    }));
                }
            }
        }
    }

    for h in handles {
        let _ = h.await;
    }
    progress.abort();

    // Final stats.
    let total_elapsed = start.elapsed();
    let measured_elapsed = run_until
        .saturating_duration_since(warmup_deadline)
        .as_secs_f64()
        .max(0.001);

    let sent = recorder.counters.sent.load(Ordering::Relaxed);
    let ok = recorder.counters.ok.load(Ordering::Relaxed);
    let nonce_err = recorder.counters.nonce_err.load(Ordering::Relaxed);
    let rpc_err = recorder.counters.rpc_err.load(Ordering::Relaxed);
    let net_err = recorder.counters.net_err.load(Ordering::Relaxed);

    let mut latencies = recorder.latencies_ns.lock().await.clone();
    latencies.sort_unstable();
    let measured_count = latencies.len() as u64;
    let achieved_rps = (measured_count as f64) / measured_elapsed;

    let head_height_after = state_client
        .chain_info()
        .await
        .map(|r| r.head_height)
        .unwrap_or(head_height_before);
    let head_delta = head_height_after.saturating_sub(head_height_before);

    println!("---");
    println!("kombat_loadgen report");
    println!("  url: {}", cli.url);
    println!("  state_url: {}", state_url);
    println!("  workload: {:?}", cli.workload);
    println!("  concurrency: {}", cli.concurrency);
    println!("  pipeline_depth: {}", cli.pipeline_depth);
    println!("  batch_size: {}", cli.batch_size);
    println!("  strict_nonce_retry: {}", cli.strict_nonce_retry);
    println!(
        "  strict_nonce_retry_attempts: {}",
        cli.strict_nonce_retry_attempts
    );
    println!("  fail_on_errors: {}", cli.fail_on_errors);
    println!("  market_id: {}", cli.market_id);
    println!("  market_count: {}", cli.market_count);
    println!("  target_rps: {}", cli.rps);
    println!("  wall_elapsed_secs: {:.3}", total_elapsed.as_secs_f64());
    println!("  measured_window_secs: {measured_elapsed:.3}");
    println!();
    println!("  requests_total: {sent}");
    println!("  requests_measured (post-warmup): {measured_count}");
    println!("  ok: {ok}");
    println!("  nonce_err: {nonce_err}");
    println!("  rpc_err: {rpc_err}");
    println!("  net_err: {net_err}");
    println!();
    println!("  achieved_rps_measured: {achieved_rps:.1}");
    if let Some(&max) = latencies.last() {
        println!("  latency_p50: {}", fmt_ns(percentile(&latencies, 0.50)));
        println!("  latency_p90: {}", fmt_ns(percentile(&latencies, 0.90)));
        println!("  latency_p99: {}", fmt_ns(percentile(&latencies, 0.99)));
        println!("  latency_max: {}", fmt_ns(max));
    }
    println!();
    println!("  head_height_before: {head_height_before}");
    println!("  head_height_after:  {head_height_after}");
    println!("  blocks_produced:    {head_delta}");

    let error_samples = recorder.error_samples.lock().await;
    if !error_samples.nonce_err.is_empty() {
        println!();
        println!("  nonce_err_samples:");
        for sample in &error_samples.nonce_err {
            println!("    - {sample}");
        }
    }
    if !error_samples.rpc_err.is_empty() {
        println!();
        println!("  rpc_err_samples:");
        for sample in &error_samples.rpc_err {
            println!("    - {sample}");
        }
    }
    if !error_samples.net_err.is_empty() {
        println!();
        println!("  net_err_samples:");
        for sample in &error_samples.net_err {
            println!("    - {sample}");
        }
    }

    if cli.fail_on_errors && (nonce_err > 0 || rpc_err > 0 || net_err > 0) {
        anyhow::bail!(
            "loadgen recorded errors: nonce_err={} rpc_err={} net_err={}",
            nonce_err,
            rpc_err,
            net_err
        );
    }

    Ok(())
}
