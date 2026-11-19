use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use and_chain_sdk::{AndChainClient, EventFilter, ListEventsParams, RuntimeEventRecord};
use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use clap::Parser;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{net::TcpListener, time::sleep};
use tower_http::cors::{Any, CorsLayer};

const DEFAULT_INTERVALS: &str = "1m,5m,15m,1h,1d";
const DEFAULT_BATCH_LIMIT: usize = 1_000;
const MAX_CANDLE_LIMIT: usize = 5_000;

#[derive(Clone, Debug, Parser)]
#[command(
    name = "kombat_market_data",
    about = "QuestDB-backed market-data read model for the 0x26 perps UI"
)]
struct Cli {
    #[arg(
        long,
        env = "KOMBAT_MARKET_DATA_RPC_URL",
        default_value = "http://127.0.0.1:8545"
    )]
    rpc_url: String,

    #[arg(
        long,
        env = "KOMBAT_MARKET_DATA_QUESTDB_URL",
        default_value = "http://127.0.0.1:9000"
    )]
    questdb_url: String,

    #[arg(
        long,
        env = "KOMBAT_MARKET_DATA_BIND",
        default_value = "127.0.0.1:8787"
    )]
    bind: SocketAddr,

    #[arg(long, env = "KOMBAT_MARKET_DATA_SOURCE", default_value = "testnet")]
    source: String,

    #[arg(long, env = "KOMBAT_MARKET_DATA_POLL_MS", default_value_t = 500)]
    poll_ms: u64,

    #[arg(long, env = "KOMBAT_MARKET_DATA_BATCH_LIMIT", default_value_t = DEFAULT_BATCH_LIMIT)]
    batch_limit: usize,

    #[arg(long, env = "KOMBAT_MARKET_DATA_INTERVALS", value_delimiter = ',', default_value = DEFAULT_INTERVALS)]
    intervals: Vec<String>,

    #[arg(
        long,
        env = "KOMBAT_MARKET_DATA_DISABLE_INGEST",
        default_value_t = false
    )]
    disable_ingest: bool,
}

#[derive(Clone)]
struct AppState {
    questdb: QuestDbClient,
}

#[derive(Clone)]
struct QuestDbClient {
    base_url: String,
    source: String,
    client: Client,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CandleKey {
    market_id: u64,
    interval: CandleInterval,
    bucket_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct CandleInterval {
    name: &'static str,
    ms: u64,
}

#[derive(Clone, Debug)]
struct Trade {
    seq: u64,
    block_height: Option<u64>,
    timestamp_ms: u64,
    market_id: u64,
    account_id: u64,
    client_order_id: u64,
    counterparty_account_id: u64,
    counterparty_client_order_id: u64,
    side: String,
    price_ticks: u64,
    quantity_lots: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct CandleQuery {
    market_id: u64,
    interval: String,
    from_ms: Option<u64>,
    to_ms: Option<u64>,
    limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
struct CandleResponse {
    market_id: u64,
    interval: String,
    timestamp_ms: u64,
    open: u64,
    high: u64,
    low: u64,
    close: u64,
    volume_lots: u64,
    trade_count: u64,
    first_seq: u64,
    last_seq: u64,
}

#[derive(Clone, Debug)]
struct CandleState {
    market_id: u64,
    interval: CandleInterval,
    timestamp_ms: u64,
    open: u64,
    high: u64,
    low: u64,
    close: u64,
    volume_lots: u64,
    trade_count: u64,
    first_seq: u64,
    last_seq: u64,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

#[derive(Debug, Deserialize)]
struct QuestExecResponse {
    #[serde(default)]
    dataset: Vec<Vec<Value>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let intervals = parse_intervals(&cli.intervals)?;
    let questdb = QuestDbClient::new(cli.questdb_url.clone(), cli.source.clone())?;
    initialize_schema_with_retry(&questdb, cli.poll_ms.max(500)).await;

    if !cli.disable_ingest {
        let ingest_questdb = questdb.clone();
        let ingest_cli = cli.clone();
        tokio::spawn(async move {
            if let Err(error) = run_ingestor(ingest_cli, ingest_questdb, intervals).await {
                eprintln!("kombat_market_data ingestor stopped: {error:#}");
            }
        });
    }

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/market-data/candles", get(handle_candles))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(Arc::new(AppState { questdb }));

    let listener = TcpListener::bind(cli.bind)
        .await
        .with_context(|| format!("failed to bind market-data API on {}", cli.bind))?;
    println!("kombat_market_data serving on http://{}", cli.bind);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("market-data API server failed")?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn healthz() -> &'static str {
    "ok"
}

async fn initialize_schema_with_retry(questdb: &QuestDbClient, retry_ms: u64) {
    loop {
        match questdb.initialize_schema().await {
            Ok(()) => return,
            Err(error) => {
                eprintln!("kombat_market_data waiting for QuestDB schema init: {error:#}");
                sleep(Duration::from_millis(retry_ms)).await;
            }
        }
    }
}

async fn handle_candles(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CandleQuery>,
) -> Result<Json<Vec<CandleResponse>>, ApiError> {
    let interval = CandleInterval::parse(&query.interval)
        .ok_or_else(|| ApiError::bad_request("unsupported candle interval"))?;
    let limit = query.limit.unwrap_or(500).min(MAX_CANDLE_LIMIT);
    if limit == 0 {
        return Err(ApiError::bad_request("limit must be positive"));
    }
    let to_ms = query.to_ms.unwrap_or_else(now_ms);
    let from_ms = query.from_ms.unwrap_or(0);
    if from_ms >= to_ms {
        return Err(ApiError::bad_request("from_ms must be less than to_ms"));
    }

    state
        .questdb
        .query_candles(query.market_id, interval, from_ms, to_ms, limit)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

impl QuestDbClient {
    fn new(base_url: String, source: String) -> Result<Self> {
        if base_url.trim().is_empty() {
            return Err(anyhow!("QuestDB URL must not be empty"));
        }
        if source.trim().is_empty() {
            return Err(anyhow!("market-data source must not be empty"));
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            source,
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .context("failed to build QuestDB HTTP client")?,
        })
    }

    async fn initialize_schema(&self) -> Result<()> {
        self.exec(
            "CREATE TABLE IF NOT EXISTS kombat_trades (
                timestamp TIMESTAMP,
                source SYMBOL,
                market_id SYMBOL,
                account_id SYMBOL,
                counterparty_account_id SYMBOL,
                seq LONG,
                block_height LONG,
                timestamp_ms LONG,
                client_order_id LONG,
                counterparty_client_order_id LONG,
                side SYMBOL,
                price_ticks LONG,
                quantity_lots LONG
            ) TIMESTAMP(timestamp) PARTITION BY DAY DEDUP UPSERT KEYS(timestamp, source, market_id, seq)",
        )
        .await?;
        self.exec(
            "CREATE TABLE IF NOT EXISTS kombat_candles (
                timestamp TIMESTAMP,
                source SYMBOL,
                market_id SYMBOL,
                interval SYMBOL,
                timestamp_ms LONG,
                open_ticks LONG,
                high_ticks LONG,
                low_ticks LONG,
                close_ticks LONG,
                volume_lots LONG,
                trade_count LONG,
                first_seq LONG,
                last_seq LONG
            ) TIMESTAMP(timestamp) PARTITION BY DAY DEDUP UPSERT KEYS(timestamp, source, market_id, interval)",
        )
        .await?;
        self.exec(
            "CREATE TABLE IF NOT EXISTS kombat_market_data_cursor (
                timestamp TIMESTAMP,
                source SYMBOL,
                next_seq LONG
            ) TIMESTAMP(timestamp) PARTITION BY DAY",
        )
        .await?;
        Ok(())
    }

    async fn exec(&self, query: &str) -> Result<QuestExecResponse> {
        let url = format!("{}/exec", self.base_url);
        let response = self
            .client
            .get(url)
            .query(&[("query", query)])
            .send()
            .await
            .context("QuestDB /exec request failed")?;
        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read QuestDB /exec response body")?;
        if !status.is_success() {
            return Err(anyhow!("QuestDB /exec returned {status}: {body}"));
        }
        serde_json::from_str(&body).with_context(|| format!("invalid QuestDB /exec JSON: {body}"))
    }

    async fn write_ilp(&self, body: String) -> Result<()> {
        if body.trim().is_empty() {
            return Ok(());
        }
        let url = format!("{}/write", self.base_url);
        let response = self
            .client
            .post(url)
            .header("content-type", "text/plain")
            .body(body)
            .send()
            .await
            .context("QuestDB ILP write failed")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("QuestDB ILP write returned {status}: {body}"));
        }
        Ok(())
    }

    async fn latest_cursor(&self) -> Result<Option<u64>> {
        let query = format!(
            "SELECT next_seq FROM kombat_market_data_cursor WHERE source = {} ORDER BY timestamp DESC LIMIT 1",
            sql_string(&self.source)
        );
        let response = self.exec(&query).await?;
        let Some(row) = response.dataset.first() else {
            return Ok(None);
        };
        Ok(Some(value_as_u64(&row[0], "next_seq")?))
    }

    async fn write_cursor(&self, next_seq: u64) -> Result<()> {
        let line = format!(
            "kombat_market_data_cursor,source={} next_seq={}i {}\n",
            ilp_tag_value(&self.source),
            next_seq,
            now_ms().saturating_mul(1_000_000)
        );
        self.write_ilp(line).await
    }

    async fn write_trades(&self, trades: &[Trade]) -> Result<()> {
        let mut body = String::new();
        for trade in trades {
            let block_height = trade.block_height.unwrap_or(0);
            body.push_str(&format!(
                "kombat_trades,source={},market_id={},side={},account_id={},counterparty_account_id={} seq={}i,block_height={}i,timestamp_ms={}i,client_order_id={}i,counterparty_client_order_id={}i,price_ticks={}i,quantity_lots={}i {}\n",
                ilp_tag_value(&self.source),
                trade.market_id,
                ilp_tag_value(&trade.side),
                trade.account_id,
                trade.counterparty_account_id,
                trade.seq,
                block_height,
                trade.timestamp_ms,
                trade.client_order_id,
                trade.counterparty_client_order_id,
                trade.price_ticks,
                trade.quantity_lots,
                trade.timestamp_ms.saturating_mul(1_000_000)
            ));
        }
        self.write_ilp(body).await
    }

    async fn fetch_candle(&self, key: CandleKey) -> Result<Option<CandleState>> {
        let query = format!(
            "SELECT timestamp_ms, open_ticks, high_ticks, low_ticks, close_ticks, volume_lots, trade_count, first_seq, last_seq \
             FROM kombat_candles WHERE source = {} AND market_id = '{}' AND interval = '{}' AND timestamp_ms = {} LIMIT 1",
            sql_string(&self.source),
            key.market_id,
            key.interval.name,
            key.bucket_ms
        );
        let response = self.exec(&query).await?;
        let Some(row) = response.dataset.first() else {
            return Ok(None);
        };
        Ok(Some(CandleState {
            market_id: key.market_id,
            interval: key.interval,
            timestamp_ms: value_as_u64(&row[0], "timestamp_ms")?,
            open: value_as_u64(&row[1], "open_ticks")?,
            high: value_as_u64(&row[2], "high_ticks")?,
            low: value_as_u64(&row[3], "low_ticks")?,
            close: value_as_u64(&row[4], "close_ticks")?,
            volume_lots: value_as_u64(&row[5], "volume_lots")?,
            trade_count: value_as_u64(&row[6], "trade_count")?,
            first_seq: value_as_u64(&row[7], "first_seq")?,
            last_seq: value_as_u64(&row[8], "last_seq")?,
        }))
    }

    async fn write_candles(&self, candles: &[CandleState]) -> Result<()> {
        let mut body = String::new();
        for candle in candles {
            body.push_str(&format!(
                "kombat_candles,source={},market_id={},interval={} timestamp_ms={}i,open_ticks={}i,high_ticks={}i,low_ticks={}i,close_ticks={}i,volume_lots={}i,trade_count={}i,first_seq={}i,last_seq={}i {}\n",
                ilp_tag_value(&self.source),
                candle.market_id,
                candle.interval.name,
                candle.timestamp_ms,
                candle.open,
                candle.high,
                candle.low,
                candle.close,
                candle.volume_lots,
                candle.trade_count,
                candle.first_seq,
                candle.last_seq,
                candle.timestamp_ms.saturating_mul(1_000_000)
            ));
        }
        self.write_ilp(body).await
    }

    async fn query_candles(
        &self,
        market_id: u64,
        interval: CandleInterval,
        from_ms: u64,
        to_ms: u64,
        limit: usize,
    ) -> Result<Vec<CandleResponse>> {
        let query = format!(
            "SELECT timestamp_ms, open_ticks, high_ticks, low_ticks, close_ticks, volume_lots, trade_count, first_seq, last_seq \
             FROM kombat_candles WHERE source = {} AND market_id = '{}' AND interval = '{}' AND timestamp_ms >= {} AND timestamp_ms < {} \
             ORDER BY timestamp DESC LIMIT {}",
            sql_string(&self.source),
            market_id,
            interval.name,
            from_ms,
            to_ms,
            limit
        );
        let response = self.exec(&query).await?;
        let mut candles = Vec::with_capacity(response.dataset.len());
        for row in response.dataset {
            candles.push(CandleResponse {
                market_id,
                interval: interval.name.to_string(),
                timestamp_ms: value_as_u64(&row[0], "timestamp_ms")?,
                open: value_as_u64(&row[1], "open_ticks")?,
                high: value_as_u64(&row[2], "high_ticks")?,
                low: value_as_u64(&row[3], "low_ticks")?,
                close: value_as_u64(&row[4], "close_ticks")?,
                volume_lots: value_as_u64(&row[5], "volume_lots")?,
                trade_count: value_as_u64(&row[6], "trade_count")?,
                first_seq: value_as_u64(&row[7], "first_seq")?,
                last_seq: value_as_u64(&row[8], "last_seq")?,
            });
        }
        candles.sort_by_key(|candle| candle.timestamp_ms);
        Ok(candles)
    }
}

async fn run_ingestor(
    cli: Cli,
    questdb: QuestDbClient,
    intervals: Vec<CandleInterval>,
) -> Result<()> {
    if cli.batch_limit == 0 {
        return Err(anyhow!("KOMBAT_MARKET_DATA_BATCH_LIMIT must be positive"));
    }
    let client = AndChainClient::new(cli.rpc_url.clone()).with_timeout(Duration::from_secs(10));
    let mut next_seq = questdb.latest_cursor().await?.unwrap_or(0);
    let retry_delay = Duration::from_millis(cli.poll_ms.max(5_000));
    println!(
        "kombat_market_data ingesting from {} source={} next_seq={}",
        cli.rpc_url, cli.source, next_seq
    );

    loop {
        let page = match client
            .list_events(ListEventsParams {
                from_seq: Some(next_seq),
                before_seq: None,
                limit: Some(cli.batch_limit),
                descending: Some(false),
                filter: Some(EventFilter {
                    block_height: None,
                    account_id: None,
                    market_id: None,
                    policy_id: None,
                    intent_id_hex: None,
                    client_order_id: None,
                    topic: Some("order_filled".to_string()),
                }),
            })
            .await
        {
            Ok(page) => page,
            Err(error) => {
                eprintln!(
                    "kombat_market_data event fetch failed from {} next_seq={}: {error:#}",
                    cli.rpc_url, next_seq
                );
                sleep(retry_delay).await;
                continue;
            }
        };

        let event_count = page.events.len();
        let new_next_seq = page.next_cursor.next_seq;
        if event_count == 0 && new_next_seq == next_seq {
            match latest_event_seq(&client).await {
                Ok(Some(latest_seq)) if cursor_is_ahead_of_chain(next_seq, latest_seq) => {
                    eprintln!(
                        "kombat_market_data resetting stale cursor source={} stored_next_seq={} latest_chain_seq={}",
                        cli.source, next_seq, latest_seq
                    );
                    next_seq = 0;
                    if let Err(error) = questdb.write_cursor(next_seq).await {
                        eprintln!(
                            "kombat_market_data cursor reset write failed source={} next_seq={}: {error:#}",
                            cli.source, next_seq
                        );
                        sleep(retry_delay).await;
                    }
                    continue;
                }
                Ok(None) if next_seq > 0 => {
                    eprintln!(
                        "kombat_market_data resetting stale cursor source={} stored_next_seq={} empty_chain_events=true",
                        cli.source, next_seq
                    );
                    next_seq = 0;
                    if let Err(error) = questdb.write_cursor(next_seq).await {
                        eprintln!(
                            "kombat_market_data cursor reset write failed source={} next_seq={}: {error:#}",
                            cli.source, next_seq
                        );
                        sleep(retry_delay).await;
                    }
                    continue;
                }
                Err(error) => {
                    eprintln!(
                        "kombat_market_data latest event check failed source={} next_seq={}: {error:#}",
                        cli.source, next_seq
                    );
                }
                _ => {}
            }
        }
        let mut trades = Vec::new();
        for event in page.events {
            if let Some(trade) = Trade::from_event(event)? {
                trades.push(trade);
            }
        }
        if !trades.is_empty() {
            if let Err(error) = questdb.write_trades(&trades).await {
                eprintln!(
                    "kombat_market_data trade write failed source={} next_seq={}: {error:#}",
                    cli.source, next_seq
                );
                sleep(retry_delay).await;
                continue;
            }
            if let Err(error) = update_candles(&questdb, &intervals, &trades).await {
                eprintln!(
                    "kombat_market_data candle update failed source={} next_seq={}: {error:#}",
                    cli.source, next_seq
                );
                sleep(retry_delay).await;
                continue;
            }
        }

        if let Err(error) = questdb.write_cursor(new_next_seq).await {
            eprintln!(
                "kombat_market_data cursor write failed source={} next_seq={}: {error:#}",
                cli.source, new_next_seq
            );
            sleep(retry_delay).await;
            continue;
        }
        let made_progress = new_next_seq > next_seq;
        next_seq = new_next_seq;

        if event_count < cli.batch_limit || !made_progress {
            sleep(Duration::from_millis(cli.poll_ms)).await;
        }
    }
}

async fn latest_event_seq(client: &AndChainClient) -> Result<Option<u64>> {
    let page = client
        .list_events(ListEventsParams {
            from_seq: None,
            before_seq: None,
            limit: Some(1),
            descending: Some(true),
            filter: None,
        })
        .await
        .context("failed to fetch latest event sequence")?;
    Ok(page.events.first().map(|event| event.seq))
}

fn cursor_is_ahead_of_chain(next_seq: u64, latest_seq: u64) -> bool {
    latest_seq.saturating_add(1) < next_seq
}

async fn update_candles(
    questdb: &QuestDbClient,
    intervals: &[CandleInterval],
    trades: &[Trade],
) -> Result<()> {
    let mut keyed_trades = BTreeMap::<CandleKey, Vec<&Trade>>::new();
    for trade in trades {
        for interval in intervals {
            let bucket_ms = (trade.timestamp_ms / interval.ms) * interval.ms;
            keyed_trades
                .entry(CandleKey {
                    market_id: trade.market_id,
                    interval: *interval,
                    bucket_ms,
                })
                .or_default()
                .push(trade);
        }
    }

    let mut candles = Vec::with_capacity(keyed_trades.len());
    for (key, mut bucket_trades) in keyed_trades {
        bucket_trades.sort_by_key(|trade| (trade.seq, trade.timestamp_ms));
        let mut candle = questdb.fetch_candle(key).await?.unwrap_or_else(|| {
            let first = bucket_trades[0];
            CandleState {
                market_id: key.market_id,
                interval: key.interval,
                timestamp_ms: key.bucket_ms,
                open: first.price_ticks,
                high: first.price_ticks,
                low: first.price_ticks,
                close: first.price_ticks,
                volume_lots: 0,
                trade_count: 0,
                first_seq: first.seq,
                last_seq: 0,
            }
        });
        for trade in bucket_trades {
            if candle.trade_count > 0 && trade.seq <= candle.last_seq {
                continue;
            }
            if candle.trade_count == 0 {
                candle.open = trade.price_ticks;
                candle.high = trade.price_ticks;
                candle.low = trade.price_ticks;
                candle.first_seq = trade.seq;
            }
            candle.high = candle.high.max(trade.price_ticks);
            candle.low = candle.low.min(trade.price_ticks);
            candle.close = trade.price_ticks;
            candle.volume_lots = candle.volume_lots.saturating_add(trade.quantity_lots);
            candle.trade_count = candle.trade_count.saturating_add(1);
            candle.first_seq = candle.first_seq.min(trade.seq);
            candle.last_seq = candle.last_seq.max(trade.seq);
        }
        candles.push(candle);
    }
    questdb.write_candles(&candles).await
}

impl Trade {
    fn from_event(event: RuntimeEventRecord) -> Result<Option<Self>> {
        let Some(kind) = event.kind.as_object() else {
            return Ok(None);
        };
        if kind.get("topic").and_then(Value::as_str) != Some("order_filled") {
            return Ok(None);
        }
        if kind.get("role").and_then(Value::as_str) != Some("taker") {
            return Ok(None);
        }
        Ok(Some(Self {
            seq: event.seq,
            block_height: event.block_height,
            timestamp_ms: event.timestamp_ms,
            market_id: object_u64(kind, "market_id")?,
            account_id: object_u64(kind, "account_id")?,
            client_order_id: object_u64(kind, "client_order_id")?,
            counterparty_account_id: object_u64(kind, "counterparty_account_id")?,
            counterparty_client_order_id: object_u64(kind, "counterparty_client_order_id")?,
            side: kind
                .get("side")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("order_filled event missing side"))?
                .to_string(),
            price_ticks: object_u64(kind, "price_ticks")?,
            quantity_lots: object_u64(kind, "quantity_lots")?,
        }))
    }
}

impl CandleInterval {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "1m" => Some(Self {
                name: "1m",
                ms: 60_000,
            }),
            "5m" => Some(Self {
                name: "5m",
                ms: 5 * 60_000,
            }),
            "15m" => Some(Self {
                name: "15m",
                ms: 15 * 60_000,
            }),
            "1h" => Some(Self {
                name: "1h",
                ms: 60 * 60_000,
            }),
            "1d" => Some(Self {
                name: "1d",
                ms: 24 * 60 * 60_000,
            }),
            _ => None,
        }
    }
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}

fn parse_intervals(values: &[String]) -> Result<Vec<CandleInterval>> {
    let mut seen = BTreeSet::new();
    let mut intervals = Vec::new();
    for value in values {
        let interval = CandleInterval::parse(value)
            .ok_or_else(|| anyhow!("unsupported candle interval configured: {value}"))?;
        if seen.insert(interval.name) {
            intervals.push(interval);
        }
    }
    if intervals.is_empty() {
        return Err(anyhow!("at least one candle interval is required"));
    }
    Ok(intervals)
}

fn object_u64(object: &serde_json::Map<String, Value>, key: &str) -> Result<u64> {
    let value = object
        .get(key)
        .ok_or_else(|| anyhow!("order_filled event missing {key}"))?;
    value_as_u64(value, key)
}

fn value_as_u64(value: &Value, field: &str) -> Result<u64> {
    if let Some(value) = value.as_u64() {
        return Ok(value);
    }
    if let Some(value) = value.as_i64() {
        if value >= 0 {
            return Ok(value as u64);
        }
    }
    if let Some(value) = value.as_f64() {
        if value.is_finite() && value >= 0.0 && value.fract() == 0.0 {
            return Ok(value as u64);
        }
    }
    if let Some(value) = value.as_str() {
        return value
            .parse::<u64>()
            .with_context(|| format!("invalid numeric {field}: {value}"));
    }
    Err(anyhow!("invalid numeric {field}: {value}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn ilp_tag_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(' ', "\\ ")
        .replace('=', "\\=")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_taker_fill_event() {
        let event = RuntimeEventRecord {
            seq: 42,
            block_height: Some(7),
            timestamp_ms: 1_700_000,
            kind: json!({
                "topic": "order_filled",
                "market_id": 7,
                "account_id": 11,
                "client_order_id": 12,
                "counterparty_account_id": 13,
                "counterparty_client_order_id": 14,
                "role": "taker",
                "side": "buy",
                "price_ticks": 7617575,
                "quantity_lots": 4
            }),
        };

        let trade = Trade::from_event(event).unwrap().unwrap();
        assert_eq!(trade.seq, 42);
        assert_eq!(trade.market_id, 7);
        assert_eq!(trade.side, "buy");
        assert_eq!(trade.price_ticks, 7617575);
        assert_eq!(trade.quantity_lots, 4);
    }

    #[test]
    fn ignores_maker_fill_event() {
        let event = RuntimeEventRecord {
            seq: 42,
            block_height: None,
            timestamp_ms: 1_700_000,
            kind: json!({
                "topic": "order_filled",
                "role": "maker"
            }),
        };

        assert!(Trade::from_event(event).unwrap().is_none());
    }

    #[test]
    fn parses_configured_intervals_once() {
        let intervals = parse_intervals(&["1m".into(), "5m".into(), "1m".into()]).unwrap();
        assert_eq!(intervals.len(), 2);
        assert_eq!(intervals[0].ms, 60_000);
        assert_eq!(intervals[1].ms, 300_000);
    }

    #[test]
    fn detects_cursor_from_previous_chain_epoch() {
        assert!(cursor_is_ahead_of_chain(28_106, 3_750));
        assert!(!cursor_is_ahead_of_chain(3_751, 3_750));
        assert!(!cursor_is_ahead_of_chain(3_750, 3_750));
    }

    #[test]
    fn trade_ilp_uses_symbol_tags_for_large_account_ids() {
        let trade = Trade {
            seq: 42,
            block_height: Some(7),
            timestamp_ms: 1_700_000,
            market_id: 7,
            account_id: 12_686_431_697_233_121_432,
            client_order_id: 12,
            counterparty_account_id: 260_002,
            counterparty_client_order_id: 14,
            side: "buy".to_string(),
            price_ticks: 7_617_575,
            quantity_lots: 4,
        };

        let line = format!(
            "kombat_trades,source={},market_id={},side={},account_id={},counterparty_account_id={} seq={}i,block_height={}i,timestamp_ms={}i,client_order_id={}i,counterparty_client_order_id={}i,price_ticks={}i,quantity_lots={}i {}\n",
            ilp_tag_value("live-testnet-current"),
            trade.market_id,
            ilp_tag_value(&trade.side),
            trade.account_id,
            trade.counterparty_account_id,
            trade.seq,
            trade.block_height.unwrap_or(0),
            trade.timestamp_ms,
            trade.client_order_id,
            trade.counterparty_client_order_id,
            trade.price_ticks,
            trade.quantity_lots,
            trade.timestamp_ms.saturating_mul(1_000_000)
        );

        assert!(line.contains(",account_id=12686431697233121432,"));
        assert!(line.contains(",counterparty_account_id=260002 "));
        assert!(!line.contains("account_id=12686431697233121432i"));
        assert!(!line.contains("counterparty_account_id=260002i"));
    }
}
