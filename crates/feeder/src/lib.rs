use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

pub const CRATE_NAME: &str = "feeder";

const DEFAULT_CHAIN_STATE_ENDPOINT: &str = "unix:///run/kombat/node.sock";
const DEFAULT_FEEDER_KEY_FILE: &str = "/var/lib/kombat-feeder/feeder.key";
const DEFAULT_METRICS_LISTEN: &str = "127.0.0.1:9921";
const DEFAULT_LOG_LEVEL: &str = "info";
const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8545";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeederConfig {
    pub feeder: FeederSection,
    #[serde(default)]
    pub aggregation: AggregationSection,
    #[serde(default)]
    pub submission: SubmissionSection,
    #[serde(default)]
    pub adapters: BTreeMap<String, AdapterSection>,
    #[serde(default)]
    pub asset_pairs: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeederSection {
    pub chain_id: u64,
    #[serde(default = "default_chain_state_endpoint")]
    pub chain_state_endpoint: String,
    #[serde(default = "default_feeder_key_file")]
    pub feeder_key_file: String,
    #[serde(default = "default_metrics_listen")]
    pub metrics_listen: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregationSection {
    #[serde(default = "default_max_staleness_ms")]
    pub max_staleness_ms: u64,
    #[serde(default = "default_min_sources_per_asset")]
    pub min_sources_per_asset: u8,
    #[serde(default = "default_max_dropout_ms")]
    pub max_dropout_ms: u64,
    #[serde(default = "default_spread_reject_pct")]
    pub spread_reject_pct: u32,
    #[serde(default = "default_local_drift_alarm_bps")]
    pub local_drift_alarm_bps: u32,
}

impl Default for AggregationSection {
    fn default() -> Self {
        Self {
            max_staleness_ms: default_max_staleness_ms(),
            min_sources_per_asset: default_min_sources_per_asset(),
            max_dropout_ms: default_max_dropout_ms(),
            spread_reject_pct: default_spread_reject_pct(),
            local_drift_alarm_bps: default_local_drift_alarm_bps(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionSection {
    #[serde(default)]
    pub gossip: GossipSubmissionSection,
    #[serde(default)]
    pub testnet_bridge: TestnetBridgeSection,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GossipSubmissionSection {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_gossip_topic")]
    pub topic: String,
}

impl Default for GossipSubmissionSection {
    fn default() -> Self {
        Self {
            enabled: false,
            topic: default_gossip_topic(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestnetBridgeSection {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_rpc_url")]
    pub node_rpc_url: String,
    pub operator_token_file: Option<String>,
    #[serde(skip)]
    pub operator_token: Option<String>,
    #[serde(default)]
    pub stand_in_feeder_ids_hex: Vec<String>,
}

impl Default for TestnetBridgeSection {
    fn default() -> Self {
        Self {
            enabled: true,
            node_rpc_url: default_rpc_url(),
            operator_token_file: None,
            operator_token: None,
            stand_in_feeder_ids_hex: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterSection {
    #[serde(default)]
    pub enabled: bool,
    pub ws_url: Option<String>,
    pub rest_url: Option<String>,
    #[serde(default = "default_rate_limit_per_sec")]
    pub rate_limit_per_sec: u32,
    #[serde(default = "default_reconnect_backoff_ms")]
    pub reconnect_backoff_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    Toml(String),
    InvalidFeederId(String),
    DuplicateStandInFeederId,
    ZeroStandInFeederId,
    InvalidAggregation(String),
    UnknownEnvVar(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub adapter: String,
    pub asset: String,
    pub mid_price_e8: u64,
    pub received_at_ms: u64,
    pub source_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAdapterTarget {
    pub adapter: String,
    pub asset: String,
    pub pair: String,
    pub ws_url: String,
    pub reconnect_backoff_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAggregate {
    pub asset: String,
    pub price_ticks: u64,
    pub sources_used: u8,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggregationError {
    InvalidConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BridgeTickError {
    Aggregation(AggregationError),
    Config(ConfigError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum RpcClientError {
    Config(ConfigError),
    Http(String),
    HttpStatus {
        status: u16,
        body: String,
    },
    InvalidMarketStatus(String),
    InvalidResponse(String),
    Io(String),
    Json(String),
    MissingOperatorToken,
    Rpc {
        code: i64,
        message: String,
        data: Option<Value>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketStatus {
    Active,
    Paused,
    ReduceOnly,
    Delisted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarketSnapshot {
    pub market_id: u64,
    pub oracle_feed_id: u64,
    pub base_symbol: String,
    pub status: MarketStatus,
    pub oracle_feeder_ids_hex: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BridgeOracleSample {
    pub feeder_id_hex: String,
    pub price_ticks: u64,
    pub sources_used: u8,
    pub confidence: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BridgeOracleRequest {
    pub market_id: u64,
    pub samples: Vec<BridgeOracleSample>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BridgeTickPlan {
    pub aggregates: Vec<LocalAggregate>,
    pub omitted_assets: Vec<String>,
    pub requests: Vec<BridgeOracleRequest>,
    pub unmapped_assets: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ChainRpcClient {
    node_rpc_url: String,
    operator_token: Option<String>,
    http: reqwest::blocking::Client,
}

#[derive(Clone, Debug, Deserialize)]
struct JsonRpcResponse {
    result: Option<Value>,
    error: Option<JsonRpcErrorPayload>,
}

#[derive(Clone, Debug, Deserialize)]
struct JsonRpcErrorPayload {
    code: i64,
    message: String,
    data: Option<Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct PerpetualMarketJson {
    market_id: u64,
    oracle_feed_id: u64,
    base_symbol: String,
    status: String,
    oracle_feeder_ids_hex: Vec<String>,
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

impl FeederConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        let config =
            toml::from_str::<Self>(input).map_err(|error| ConfigError::Toml(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn apply_env_overrides<I, K, V>(&mut self, vars: I) -> Result<(), ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        for (key, value) in vars {
            let value = value.into();
            match key.as_ref() {
                "KOMBAT_FEEDER_LOG" => self.feeder.log_level = value,
                "KOMBAT_FEEDER_KEY" => self.feeder.feeder_key_file = value,
                "KOMBAT_FEEDER_METRICS_ADDR" => self.feeder.metrics_listen = value,
                "KOMBAT_FEEDER_CHAIN_STATE_ENDPOINT" => self.feeder.chain_state_endpoint = value,
                "KOMBAT_FEEDER_RPC_URL" => self.submission.testnet_bridge.node_rpc_url = value,
                "KOMBAT_FEEDER_OPERATOR_TOKEN" => {
                    self.submission.testnet_bridge.operator_token = Some(value)
                }
                unknown if unknown.starts_with("KOMBAT_FEEDER_") => {
                    return Err(ConfigError::UnknownEnvVar(unknown.into()));
                }
                _ => {}
            }
        }
        self.validate()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.aggregation.max_staleness_ms == 0 {
            return Err(ConfigError::InvalidAggregation("max_staleness_ms".into()));
        }
        if self.aggregation.min_sources_per_asset == 0 {
            return Err(ConfigError::InvalidAggregation(
                "min_sources_per_asset".into(),
            ));
        }
        validate_stand_in_feeder_ids(&self.submission.testnet_bridge.stand_in_feeder_ids_hex)
    }
}

impl ChainRpcClient {
    pub fn new(node_rpc_url: impl Into<String>, operator_token: Option<String>) -> Self {
        Self {
            node_rpc_url: node_rpc_url.into(),
            operator_token,
            http: reqwest::blocking::Client::new(),
        }
    }

    pub fn from_config(config: &FeederConfig) -> Result<Self, RpcClientError> {
        let bridge = &config.submission.testnet_bridge;
        Ok(Self::new(
            bridge.node_rpc_url.clone(),
            effective_operator_token(bridge)?,
        ))
    }

    pub fn list_perpetual_markets(&self) -> Result<Vec<MarketSnapshot>, RpcClientError> {
        let result = self.call_json_rpc(
            "kombat_listPerpetualMarkets",
            json!({
                "limit": 1_000,
            }),
            false,
        )?;
        market_snapshots_from_json(&result)
    }

    pub fn submit_bridge_request(
        &self,
        request: &BridgeOracleRequest,
    ) -> Result<Value, RpcClientError> {
        let params = serde_json::to_value(request).map_err(|error| {
            RpcClientError::Json(format!("failed to encode oracle request: {error}"))
        })?;
        self.call_json_rpc("kombat_submitOraclePrices", params, true)
    }

    pub fn submit_bridge_requests(
        &self,
        requests: &[BridgeOracleRequest],
    ) -> Result<Vec<Value>, RpcClientError> {
        requests
            .iter()
            .map(|request| self.submit_bridge_request(request))
            .collect()
    }

    fn call_json_rpc(
        &self,
        method: &str,
        params: Value,
        requires_operator: bool,
    ) -> Result<Value, RpcClientError> {
        let mut request = self
            .http
            .post(&self.node_rpc_url)
            .json(&json_rpc_request_value(method, params));
        if requires_operator {
            let Some(token) = self.operator_token.as_deref() else {
                return Err(RpcClientError::MissingOperatorToken);
            };
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .map_err(|error| RpcClientError::Http(error.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|error| RpcClientError::Http(error.to_string()))?;
        if !(200..300).contains(&status) {
            return Err(RpcClientError::HttpStatus { status, body });
        }
        json_rpc_result_from_str(&body)
    }
}

pub fn effective_operator_token(
    bridge: &TestnetBridgeSection,
) -> Result<Option<String>, RpcClientError> {
    if let Some(token) = bridge.operator_token.as_deref() {
        return Ok(Some(token.to_string()));
    }
    let Some(path) = bridge.operator_token_file.as_deref() else {
        return Ok(None);
    };
    let token = fs::read_to_string(path)
        .map_err(|error| {
            RpcClientError::Io(format!("failed to read operator token file: {error}"))
        })?
        .trim()
        .to_string();
    Ok((!token.is_empty()).then_some(token))
}

pub fn json_rpc_request_value(method: &str, params: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
}

pub fn json_rpc_result_from_str(body: &str) -> Result<Value, RpcClientError> {
    let response = serde_json::from_str::<JsonRpcResponse>(body)
        .map_err(|error| RpcClientError::Json(error.to_string()))?;
    json_rpc_result(response)
}

fn json_rpc_result(response: JsonRpcResponse) -> Result<Value, RpcClientError> {
    if let Some(error) = response.error {
        return Err(RpcClientError::Rpc {
            code: error.code,
            message: error.message,
            data: error.data,
        });
    }
    response
        .result
        .ok_or_else(|| RpcClientError::InvalidResponse("missing json-rpc result".into()))
}

pub fn market_snapshots_from_json(value: &Value) -> Result<Vec<MarketSnapshot>, RpcClientError> {
    let markets = serde_json::from_value::<Vec<PerpetualMarketJson>>(value.clone())
        .map_err(|error| RpcClientError::Json(error.to_string()))?;
    markets
        .into_iter()
        .map(|market| {
            let status = parse_market_status(&market.status)
                .ok_or_else(|| RpcClientError::InvalidMarketStatus(market.status.clone()))?;
            Ok(MarketSnapshot {
                market_id: market.market_id,
                oracle_feed_id: market.oracle_feed_id,
                base_symbol: market.base_symbol,
                status,
                oracle_feeder_ids_hex: market.oracle_feeder_ids_hex,
            })
        })
        .collect()
}

fn parse_market_status(value: &str) -> Option<MarketStatus> {
    match value {
        "active" => Some(MarketStatus::Active),
        "paused" => Some(MarketStatus::Paused),
        "reduce_only" => Some(MarketStatus::ReduceOnly),
        "delisted" => Some(MarketStatus::Delisted),
        _ => None,
    }
}

pub fn live_adapter_targets(config: &FeederConfig) -> Vec<LiveAdapterTarget> {
    let mut targets = Vec::new();
    for (asset, pairs) in &config.asset_pairs {
        for (adapter, pair) in pairs {
            let Some(section) = config.adapters.get(adapter) else {
                continue;
            };
            if !section.enabled {
                continue;
            }
            let Some(ws_url) = section
                .ws_url
                .clone()
                .or_else(|| default_adapter_ws_url(adapter).map(str::to_string))
            else {
                continue;
            };
            targets.push(LiveAdapterTarget {
                adapter: adapter.clone(),
                asset: asset.clone(),
                pair: pair.clone(),
                ws_url,
                reconnect_backoff_ms: section.reconnect_backoff_ms,
            });
        }
    }
    targets
}

pub fn default_adapter_ws_url(adapter: &str) -> Option<&'static str> {
    match adapter {
        "binance" => Some("wss://stream.binance.com:9443/ws"),
        "okx" => Some("wss://ws.okx.com:8443/ws/v5/public"),
        "coinbase" => Some("wss://advanced-trade-ws.coinbase.com"),
        _ => None,
    }
}

pub fn decimal_str_to_e8(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.starts_with('+') {
        return None;
    }
    let mut parts = value.split('.');
    let whole = parts.next()?;
    let frac = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return None;
    }
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !frac.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let whole_value = if whole.is_empty() {
        0
    } else {
        whole.parse::<u64>().ok()?
    };
    let mut frac_e8 = 0_u64;
    let mut multiplier = 10_000_000_u64;
    for byte in frac.bytes().take(8) {
        frac_e8 = frac_e8.checked_add(u64::from(byte - b'0').checked_mul(multiplier)?)?;
        multiplier /= 10;
    }
    whole_value.checked_mul(100_000_000)?.checked_add(frac_e8)
}

pub fn parse_binance_book_ticker(
    asset: &str,
    text: &str,
    received_at_ms: u64,
    source_seq: u64,
) -> Option<PriceUpdate> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    price_update_from_bid_ask(
        "binance",
        asset,
        value.get("b")?.as_str()?,
        value.get("a")?.as_str()?,
        received_at_ms,
        source_seq,
    )
}

pub fn parse_okx_ticker(
    asset: &str,
    text: &str,
    received_at_ms: u64,
    source_seq: u64,
) -> Option<PriceUpdate> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let data = value.get("data")?.as_array()?.first()?;
    price_update_from_bid_ask(
        "okx",
        asset,
        data.get("bidPx")?.as_str()?,
        data.get("askPx")?.as_str()?,
        received_at_ms,
        source_seq,
    )
}

pub fn parse_coinbase_ticker(
    asset: &str,
    text: &str,
    received_at_ms: u64,
    source_seq: u64,
) -> Option<PriceUpdate> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    if let Some(events) = value.get("events").and_then(Value::as_array) {
        for event in events {
            let Some(tickers) = event.get("tickers").and_then(Value::as_array) else {
                continue;
            };
            for ticker in tickers {
                let Some(update) =
                    coinbase_ticker_value_to_update(asset, ticker, received_at_ms, source_seq)
                else {
                    continue;
                };
                return Some(update);
            }
        }
    }
    coinbase_ticker_value_to_update(asset, &value, received_at_ms, source_seq)
}

fn coinbase_ticker_value_to_update(
    asset: &str,
    value: &Value,
    received_at_ms: u64,
    source_seq: u64,
) -> Option<PriceUpdate> {
    let bid = value
        .get("best_bid")
        .or_else(|| value.get("bid"))
        .or_else(|| value.get("best_bid_price"))?
        .as_str()?;
    let ask = value
        .get("best_ask")
        .or_else(|| value.get("ask"))
        .or_else(|| value.get("best_ask_price"))?
        .as_str()?;
    price_update_from_bid_ask("coinbase", asset, bid, ask, received_at_ms, source_seq)
}

fn price_update_from_bid_ask(
    adapter: &str,
    asset: &str,
    bid: &str,
    ask: &str,
    received_at_ms: u64,
    source_seq: u64,
) -> Option<PriceUpdate> {
    let bid = decimal_str_to_e8(bid)?;
    let ask = decimal_str_to_e8(ask)?;
    if bid == 0 || ask == 0 || bid > ask {
        return None;
    }
    let mid_price_e8 = ((u128::from(bid) + u128::from(ask)) / 2) as u64;
    Some(PriceUpdate {
        adapter: adapter.into(),
        asset: asset.into(),
        mid_price_e8,
        received_at_ms,
        source_seq,
    })
}

/// Builds the local-per-validator median. The chain performs a second
/// cross-validator median in kombat_chain_oracle; keep round-down median and
/// confidence semantics aligned between the two implementations.
pub fn aggregate_asset_price<'a, I>(
    asset: &str,
    updates: I,
    now_ms: u64,
    config: &AggregationSection,
) -> Result<Option<LocalAggregate>, AggregationError>
where
    I: IntoIterator<Item = &'a PriceUpdate>,
{
    if config.max_staleness_ms == 0 || config.min_sources_per_asset == 0 {
        return Err(AggregationError::InvalidConfig);
    }

    let mut prices = updates
        .into_iter()
        .filter(|update| {
            update.asset == asset
                && update.mid_price_e8 > 0
                && now_ms.saturating_sub(update.received_at_ms) <= config.max_staleness_ms
        })
        .map(|update| update.mid_price_e8)
        .collect::<Vec<_>>();
    if prices.len() < usize::from(config.min_sources_per_asset) {
        return Ok(None);
    }

    prices.sort_unstable();
    let midpoint = prices.len() / 2;
    let price_ticks = if prices.len() % 2 == 1 {
        prices[midpoint]
    } else {
        ((u128::from(prices[midpoint - 1]) + u128::from(prices[midpoint])) / 2) as u64
    };
    let min_price = prices.first().copied().unwrap_or(price_ticks);
    let max_price = prices.last().copied().unwrap_or(price_ticks);
    let max_minus_min_bps = if price_ticks == 0 {
        u64::MAX
    } else {
        ((u128::from(max_price.saturating_sub(min_price)) * 10_000) / u128::from(price_ticks))
            as u64
    };
    let confidence = 100_u8.saturating_sub((max_minus_min_bps / 50).min(100) as u8);

    Ok(Some(LocalAggregate {
        asset: asset.to_string(),
        price_ticks,
        sources_used: prices.len().min(u8::MAX as usize) as u8,
        confidence,
    }))
}

pub fn build_bridge_requests(
    markets: &[MarketSnapshot],
    aggregates_by_asset: &BTreeMap<String, LocalAggregate>,
    local_feeder_id_hex: &str,
    stand_in_feeder_ids_hex: &[String],
    timestamp_ms: u64,
) -> Result<Vec<BridgeOracleRequest>, ConfigError> {
    let local_feeder_id = normalize_feeder_id_hex(local_feeder_id_hex)?;
    validate_stand_in_feeder_ids(stand_in_feeder_ids_hex)?;
    let stand_ins = stand_in_feeder_ids_hex
        .iter()
        .map(|id| normalize_feeder_id_hex(id))
        .collect::<Result<Vec<_>, _>>()?;

    let mut requests = Vec::new();
    for market in markets {
        if market.status == MarketStatus::Delisted {
            continue;
        }
        let Some(aggregate) = aggregates_by_asset.get(&market.base_symbol) else {
            continue;
        };
        let allowed = market
            .oracle_feeder_ids_hex
            .iter()
            .map(|id| normalize_feeder_id_hex(id))
            .collect::<Result<BTreeSet<_>, _>>()?;

        let mut feeder_ids = if stand_ins.is_empty() {
            if allowed.contains(&local_feeder_id) {
                vec![local_feeder_id.clone()]
            } else {
                Vec::new()
            }
        } else {
            stand_ins
                .iter()
                .filter(|id| allowed.contains(*id))
                .cloned()
                .collect::<Vec<_>>()
        };
        feeder_ids.sort();
        if feeder_ids.is_empty() {
            continue;
        }

        requests.push(BridgeOracleRequest {
            market_id: market.market_id,
            samples: feeder_ids
                .into_iter()
                .map(|feeder_id_hex| BridgeOracleSample {
                    feeder_id_hex,
                    price_ticks: aggregate.price_ticks,
                    sources_used: aggregate.sources_used,
                    confidence: aggregate.confidence,
                })
                .collect(),
            timestamp_ms,
        });
    }

    Ok(requests)
}

pub fn build_testnet_bridge_tick<'a, I>(
    config: &FeederConfig,
    markets: &[MarketSnapshot],
    updates: I,
    local_feeder_id_hex: &str,
    timestamp_ms: u64,
) -> Result<BridgeTickPlan, BridgeTickError>
where
    I: IntoIterator<Item = &'a PriceUpdate>,
{
    let local_feeder_id =
        normalize_feeder_id_hex(local_feeder_id_hex).map_err(BridgeTickError::Config)?;
    let stand_in_ids = config
        .submission
        .testnet_bridge
        .stand_in_feeder_ids_hex
        .iter()
        .map(|id| normalize_feeder_id_hex(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(BridgeTickError::Config)?;
    validate_stand_in_feeder_ids(&config.submission.testnet_bridge.stand_in_feeder_ids_hex)
        .map_err(BridgeTickError::Config)?;

    let mut required_assets = BTreeSet::new();
    let mut unmapped_assets = BTreeSet::new();
    for market in markets {
        if !market_is_bridge_allowed(market, &local_feeder_id, &stand_in_ids)? {
            continue;
        }
        if config.asset_pairs.contains_key(&market.base_symbol) {
            required_assets.insert(market.base_symbol.clone());
        } else {
            unmapped_assets.insert(market.base_symbol.clone());
        }
    }

    let updates = updates.into_iter().collect::<Vec<_>>();
    let mut aggregates_by_asset = BTreeMap::new();
    let mut omitted_assets = Vec::new();
    for asset in required_assets {
        match aggregate_asset_price(
            &asset,
            updates.iter().copied(),
            timestamp_ms,
            &config.aggregation,
        )
        .map_err(BridgeTickError::Aggregation)?
        {
            Some(aggregate) => {
                aggregates_by_asset.insert(asset, aggregate);
            }
            None => omitted_assets.push(asset),
        }
    }
    let aggregates = aggregates_by_asset.values().cloned().collect::<Vec<_>>();
    let requests = build_bridge_requests(
        markets,
        &aggregates_by_asset,
        local_feeder_id_hex,
        &config.submission.testnet_bridge.stand_in_feeder_ids_hex,
        timestamp_ms,
    )
    .map_err(BridgeTickError::Config)?;

    Ok(BridgeTickPlan {
        aggregates,
        omitted_assets,
        requests,
        unmapped_assets: unmapped_assets.into_iter().collect(),
    })
}

fn market_is_bridge_allowed(
    market: &MarketSnapshot,
    local_feeder_id: &str,
    stand_in_ids: &[String],
) -> Result<bool, BridgeTickError> {
    if market.status == MarketStatus::Delisted {
        return Ok(false);
    }
    let allowed = market
        .oracle_feeder_ids_hex
        .iter()
        .map(|id| normalize_feeder_id_hex(id))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(BridgeTickError::Config)?;
    if stand_in_ids.is_empty() {
        return Ok(allowed.contains(local_feeder_id));
    }
    Ok(stand_in_ids.iter().any(|id| allowed.contains(id)))
}

pub fn normalize_feeder_id_hex(value: &str) -> Result<String, ConfigError> {
    let stripped = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(stripped).map_err(|_| ConfigError::InvalidFeederId(value.into()))?;
    if bytes.len() != 32 {
        return Err(ConfigError::InvalidFeederId(value.into()));
    }
    Ok(format!("0x{}", hex::encode(bytes)))
}

fn validate_stand_in_feeder_ids(ids: &[String]) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        let normalized = normalize_feeder_id_hex(id)?;
        if normalized == format!("0x{}", "00".repeat(32)) {
            return Err(ConfigError::ZeroStandInFeederId);
        }
        if !seen.insert(normalized) {
            return Err(ConfigError::DuplicateStandInFeederId);
        }
    }
    Ok(())
}

fn default_chain_state_endpoint() -> String {
    DEFAULT_CHAIN_STATE_ENDPOINT.into()
}

fn default_feeder_key_file() -> String {
    DEFAULT_FEEDER_KEY_FILE.into()
}

fn default_metrics_listen() -> String {
    DEFAULT_METRICS_LISTEN.into()
}

fn default_log_level() -> String {
    DEFAULT_LOG_LEVEL.into()
}

fn default_gossip_topic() -> String {
    "chain/oracle-submission/v1".into()
}

fn default_rpc_url() -> String {
    DEFAULT_RPC_URL.into()
}

fn default_true() -> bool {
    true
}

fn default_max_staleness_ms() -> u64 {
    5_000
}

fn default_min_sources_per_asset() -> u8 {
    3
}

fn default_max_dropout_ms() -> u64 {
    30_000
}

fn default_spread_reject_pct() -> u32 {
    5
}

fn default_local_drift_alarm_bps() -> u32 {
    200
}

fn default_rate_limit_per_sec() -> u32 {
    10
}

fn default_reconnect_backoff_ms() -> u64 {
    500
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feeder_id(byte: u8) -> String {
        format!("0x{}", hex::encode([byte; 32]))
    }

    fn bridge_config() -> FeederConfig {
        FeederConfig::from_toml_str(
            r#"
[feeder]
chain_id = 7

[aggregation]
min_sources_per_asset = 2

[asset_pairs."BTC"]
binance = "BTCUSDT"
okx = "BTC-USDT"

[asset_pairs."ETH"]
binance = "ETHUSDT"
okx = "ETH-USDT"
"#,
        )
        .expect("bridge config should parse")
    }

    fn price_update(adapter: &str, asset: &str, price: u64) -> PriceUpdate {
        PriceUpdate {
            adapter: adapter.into(),
            asset: asset.into(),
            mid_price_e8: price,
            received_at_ms: 1_000,
            source_seq: 1,
        }
    }

    #[test]
    fn builds_enabled_live_adapter_targets() {
        let config = FeederConfig::from_toml_str(
            r#"
[feeder]
chain_id = 7

[adapters.binance]
enabled = true

[adapters.okx]
enabled = false

[asset_pairs."BTC"]
binance = "BTCUSDT"
okx = "BTC-USDT"
"#,
        )
        .expect("config should parse");

        let targets = live_adapter_targets(&config);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].adapter, "binance");
        assert_eq!(targets[0].asset, "BTC");
        assert_eq!(targets[0].pair, "BTCUSDT");
        assert_eq!(targets[0].ws_url, "wss://stream.binance.com:9443/ws");
    }

    #[test]
    fn parses_decimal_prices_to_e8() {
        assert_eq!(decimal_str_to_e8("95000"), Some(9_500_000_000_000));
        assert_eq!(decimal_str_to_e8("95000.12345678"), Some(9_500_012_345_678));
        assert_eq!(
            decimal_str_to_e8("95000.123456789"),
            Some(9_500_012_345_678)
        );
        assert_eq!(decimal_str_to_e8(".5"), Some(50_000_000));
        assert_eq!(decimal_str_to_e8("-1"), None);
        assert_eq!(decimal_str_to_e8("1.2.3"), None);
        assert_eq!(decimal_str_to_e8("abc"), None);
    }

    #[test]
    fn parses_exchange_ticker_messages() {
        let binance = parse_binance_book_ticker(
            "BTC",
            r#"{"u":1,"s":"BTCUSDT","b":"95000.00","B":"1","a":"95000.10","A":"1"}"#,
            1_000,
            10,
        )
        .expect("binance book ticker should parse");
        assert_eq!(binance.adapter, "binance");
        assert_eq!(binance.mid_price_e8, 9_500_005_000_000);

        let okx = parse_okx_ticker(
            "BTC",
            r#"{"arg":{"channel":"tickers","instId":"BTC-USDT"},"data":[{"bidPx":"94999.9","askPx":"95000.1","ts":"1"}]}"#,
            1_001,
            11,
        )
        .expect("okx ticker should parse");
        assert_eq!(okx.adapter, "okx");
        assert_eq!(okx.mid_price_e8, 9_500_000_000_000);

        let coinbase = parse_coinbase_ticker(
            "BTC",
            r#"{"channel":"ticker","events":[{"type":"update","tickers":[{"product_id":"BTC-USD","best_bid":"94999.50","best_ask":"95000.50"}]}]}"#,
            1_002,
            12,
        )
        .expect("coinbase ticker should parse");
        assert_eq!(coinbase.adapter, "coinbase");
        assert_eq!(coinbase.mid_price_e8, 9_500_000_000_000);
    }

    #[test]
    fn parses_config_and_env_overrides() {
        let mut config = FeederConfig::from_toml_str(
            r#"
[feeder]
chain_id = 7

[aggregation]
min_sources_per_asset = 2

[submission.testnet_bridge]
enabled = true
stand_in_feeder_ids_hex = ["0x0101010101010101010101010101010101010101010101010101010101010101"]

[asset_pairs."BTC"]
binance = "BTCUSDT"
okx = "BTC-USDT"
"#,
        )
        .expect("config should parse");

        config
            .apply_env_overrides([
                ("KOMBAT_FEEDER_RPC_URL", "http://127.0.0.1:9545"),
                ("KOMBAT_FEEDER_LOG", "debug"),
                ("KOMBAT_FEEDER_OPERATOR_TOKEN", "inline-secret"),
                ("PATH", "/usr/bin"),
            ])
            .expect("env overrides should validate");

        assert_eq!(config.feeder.chain_id, 7);
        assert_eq!(config.feeder.log_level, "debug");
        assert_eq!(config.aggregation.min_sources_per_asset, 2);
        assert_eq!(
            config.submission.testnet_bridge.node_rpc_url,
            "http://127.0.0.1:9545"
        );
        assert_eq!(
            config.submission.testnet_bridge.operator_token.as_deref(),
            Some("inline-secret")
        );
        assert_eq!(config.submission.testnet_bridge.operator_token_file, None);
        assert_eq!(config.asset_pairs["BTC"]["binance"], "BTCUSDT");
    }

    #[test]
    fn rejects_unknown_config_and_env_fields() {
        let unknown_toml = FeederConfig::from_toml_str(
            r#"
[feeder]
chain_id = 7
typo = "ignored"
"#,
        )
        .expect_err("unknown config fields should be rejected");
        assert!(matches!(unknown_toml, ConfigError::Toml(_)));

        let mut config = FeederConfig::from_toml_str(
            r#"
[feeder]
chain_id = 7
"#,
        )
        .expect("config should parse");
        let unknown_env = config
            .apply_env_overrides([("KOMBAT_FEEDER_UNKNOWN", "value")])
            .expect_err("unknown feeder env vars should be rejected");
        assert_eq!(
            unknown_env,
            ConfigError::UnknownEnvVar("KOMBAT_FEEDER_UNKNOWN".into())
        );
    }

    #[test]
    fn rejects_invalid_stand_in_feeder_config() {
        let zero_id = validate_stand_in_feeder_ids(&[feeder_id(0)])
            .expect_err("zero stand-in feeder id should be rejected");
        assert_eq!(zero_id, ConfigError::ZeroStandInFeederId);

        let duplicate = validate_stand_in_feeder_ids(&[feeder_id(1), feeder_id(1)])
            .expect_err("duplicate stand-in feeder id should be rejected");
        assert_eq!(duplicate, ConfigError::DuplicateStandInFeederId);
    }

    #[test]
    fn rejects_invalid_aggregation_config() {
        let error = FeederConfig::from_toml_str(
            r#"
[feeder]
chain_id = 7

[aggregation]
max_staleness_ms = 0
"#,
        )
        .expect_err("zero staleness should be rejected");
        assert_eq!(
            error,
            ConfigError::InvalidAggregation("max_staleness_ms".into())
        );
    }

    #[test]
    fn normalizes_and_rejects_feeder_ids() {
        let normalized = normalize_feeder_id_hex(&hex::encode([9_u8; 32]))
            .expect("bare 32-byte hex should normalize");
        assert_eq!(normalized, feeder_id(9));

        assert!(matches!(
            normalize_feeder_id_hex("0x12"),
            Err(ConfigError::InvalidFeederId(_))
        ));
        assert!(matches!(
            normalize_feeder_id_hex(&format!("0x{}", "zz".repeat(32))),
            Err(ConfigError::InvalidFeederId(_))
        ));
    }

    #[test]
    fn parses_market_snapshots_from_rpc_json() {
        let markets = market_snapshots_from_json(&json!([
            {
                "market_id": 7,
                "kind": "perpetual",
                "status": "active",
                "base_symbol": "BTC",
                "quote_asset": 1,
                "oracle_feed_id": 77,
                "oracle_feeder_ids_hex": [feeder_id(1), feeder_id(2)],
                "listed_at_ms": 1_000,
                "updated_at_ms": 1_000
            },
            {
                "market_id": 8,
                "kind": "perpetual",
                "status": "reduce_only",
                "base_symbol": "ETH",
                "quote_asset": 1,
                "oracle_feed_id": 78,
                "oracle_feeder_ids_hex": [feeder_id(3)],
                "listed_at_ms": 1_000,
                "updated_at_ms": 1_000
            }
        ]))
        .expect("rpc market list should parse");

        assert_eq!(markets.len(), 2);
        assert_eq!(markets[0].market_id, 7);
        assert_eq!(markets[0].status, MarketStatus::Active);
        assert_eq!(
            markets[0].oracle_feeder_ids_hex,
            vec![feeder_id(1), feeder_id(2)]
        );
        assert_eq!(markets[1].market_id, 8);
        assert_eq!(markets[1].status, MarketStatus::ReduceOnly);
    }

    #[test]
    fn rejects_unknown_market_status_from_rpc_json() {
        let error = market_snapshots_from_json(&json!([
            {
                "market_id": 7,
                "status": "halted",
                "base_symbol": "BTC",
                "oracle_feed_id": 77,
                "oracle_feeder_ids_hex": [feeder_id(1)]
            }
        ]))
        .expect_err("unknown rpc market status should be rejected");

        assert_eq!(error, RpcClientError::InvalidMarketStatus("halted".into()));
    }

    #[test]
    fn builds_json_rpc_request_and_unwraps_result() {
        let body = json_rpc_request_value(
            "kombat_listPerpetualMarkets",
            json!({
                "limit": 1_000,
            }),
        );

        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["id"], 1);
        assert_eq!(body["method"], "kombat_listPerpetualMarkets");
        assert_eq!(body["params"]["limit"], 1_000);

        let result =
            json_rpc_result_from_str(r#"{"jsonrpc":"2.0","id":1,"result":[{"market_id":7}]}"#)
                .expect("json-rpc result should parse");
        assert_eq!(result, json!([{"market_id": 7}]));
    }

    #[test]
    fn surfaces_json_rpc_error_payload() {
        let error = json_rpc_result_from_str(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32010,"message":"operator authorization required","data":{"kind":"auth"}}}"#,
        )
        .expect_err("json-rpc error should surface");

        assert_eq!(
            error,
            RpcClientError::Rpc {
                code: -32010,
                message: "operator authorization required".into(),
                data: Some(json!({"kind": "auth"})),
            }
        );
    }

    #[test]
    fn effective_operator_token_prefers_inline_env_token() {
        let bridge = TestnetBridgeSection {
            operator_token_file: Some("/does/not/exist".into()),
            operator_token: Some("inline-token".into()),
            ..TestnetBridgeSection::default()
        };

        let token =
            effective_operator_token(&bridge).expect("inline token should not read token file");

        assert_eq!(token.as_deref(), Some("inline-token"));
    }

    #[test]
    fn aggregates_asset_price_deterministically() {
        let config = AggregationSection {
            min_sources_per_asset: 3,
            ..AggregationSection::default()
        };
        let updates = vec![
            PriceUpdate {
                adapter: "okx".into(),
                asset: "BTC".into(),
                mid_price_e8: 100_000,
                received_at_ms: 1_000,
                source_seq: 1,
            },
            PriceUpdate {
                adapter: "binance".into(),
                asset: "BTC".into(),
                mid_price_e8: 101_000,
                received_at_ms: 1_000,
                source_seq: 1,
            },
            PriceUpdate {
                adapter: "kraken".into(),
                asset: "BTC".into(),
                mid_price_e8: 99_000,
                received_at_ms: 1_000,
                source_seq: 1,
            },
            PriceUpdate {
                adapter: "stale".into(),
                asset: "BTC".into(),
                mid_price_e8: 1,
                received_at_ms: 1,
                source_seq: 1,
            },
        ];

        let aggregate = aggregate_asset_price("BTC", &updates, 6_000, &config)
            .expect("aggregation should be valid")
            .expect("aggregate should exist");

        assert_eq!(aggregate.price_ticks, 100_000);
        assert_eq!(aggregate.sources_used, 3);
        assert!(aggregate.confidence < 100);
    }

    #[test]
    fn aggregate_asset_price_returns_none_without_quorum() {
        let config = AggregationSection {
            min_sources_per_asset: 2,
            ..AggregationSection::default()
        };
        let updates = vec![PriceUpdate {
            adapter: "okx".into(),
            asset: "BTC".into(),
            mid_price_e8: 100_000,
            received_at_ms: 1_000,
            source_seq: 1,
        }];

        let aggregate = aggregate_asset_price("BTC", &updates, 1_000, &config)
            .expect("aggregation config should be valid");

        assert_eq!(aggregate, None);
    }

    #[test]
    fn bridge_requests_filter_markets_and_stand_in_feeders() {
        let markets = vec![
            MarketSnapshot {
                market_id: 7,
                oracle_feed_id: 77,
                base_symbol: "BTC".into(),
                status: MarketStatus::Active,
                oracle_feeder_ids_hex: vec![feeder_id(1), feeder_id(2), feeder_id(3)],
            },
            MarketSnapshot {
                market_id: 8,
                oracle_feed_id: 78,
                base_symbol: "ETH".into(),
                status: MarketStatus::Delisted,
                oracle_feeder_ids_hex: vec![feeder_id(1)],
            },
        ];
        let aggregates = BTreeMap::from([(
            "BTC".into(),
            LocalAggregate {
                asset: "BTC".into(),
                price_ticks: 100_000,
                sources_used: 3,
                confidence: 95,
            },
        )]);

        let requests = build_bridge_requests(
            &markets,
            &aggregates,
            &feeder_id(9),
            &[feeder_id(3), feeder_id(1), feeder_id(4)],
            1_200,
        )
        .expect("bridge request should build");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].market_id, 7);
        assert_eq!(requests[0].samples.len(), 2);
        assert_eq!(requests[0].samples[0].feeder_id_hex, feeder_id(1));
        assert_eq!(requests[0].samples[1].feeder_id_hex, feeder_id(3));
        assert_eq!(requests[0].timestamp_ms, 1_200);
    }

    #[test]
    fn bridge_requests_skip_unauthorized_local_feeder() {
        let markets = vec![MarketSnapshot {
            market_id: 7,
            oracle_feed_id: 77,
            base_symbol: "BTC".into(),
            status: MarketStatus::Paused,
            oracle_feeder_ids_hex: vec![feeder_id(1)],
        }];
        let aggregates = BTreeMap::from([(
            "BTC".into(),
            LocalAggregate {
                asset: "BTC".into(),
                price_ticks: 100_000,
                sources_used: 3,
                confidence: 95,
            },
        )]);

        let requests = build_bridge_requests(&markets, &aggregates, &feeder_id(2), &[], 1_200)
            .expect("bridge request should build");

        assert!(requests.is_empty());
    }

    #[test]
    fn bridge_requests_skip_empty_stand_in_intersection() {
        let markets = vec![MarketSnapshot {
            market_id: 7,
            oracle_feed_id: 77,
            base_symbol: "BTC".into(),
            status: MarketStatus::Active,
            oracle_feeder_ids_hex: vec![feeder_id(1)],
        }];
        let aggregates = BTreeMap::from([(
            "BTC".into(),
            LocalAggregate {
                asset: "BTC".into(),
                price_ticks: 100_000,
                sources_used: 3,
                confidence: 95,
            },
        )]);

        let requests =
            build_bridge_requests(&markets, &aggregates, &feeder_id(1), &[feeder_id(2)], 1_200)
                .expect("bridge request should build");

        assert!(requests.is_empty());
    }

    #[test]
    fn bridge_tick_plans_requests_for_allowed_mapped_markets() {
        let config = bridge_config();
        let markets = vec![
            MarketSnapshot {
                market_id: 7,
                oracle_feed_id: 77,
                base_symbol: "BTC".into(),
                status: MarketStatus::Paused,
                oracle_feeder_ids_hex: vec![feeder_id(1)],
            },
            MarketSnapshot {
                market_id: 8,
                oracle_feed_id: 78,
                base_symbol: "ETH".into(),
                status: MarketStatus::ReduceOnly,
                oracle_feeder_ids_hex: vec![feeder_id(1)],
            },
            MarketSnapshot {
                market_id: 9,
                oracle_feed_id: 79,
                base_symbol: "SOL".into(),
                status: MarketStatus::Active,
                oracle_feeder_ids_hex: vec![feeder_id(2)],
            },
        ];
        let updates = vec![
            price_update("binance", "BTC", 100_000),
            price_update("okx", "BTC", 101_000),
            price_update("binance", "ETH", 10_000),
            price_update("okx", "ETH", 11_000),
            price_update("binance", "SOL", 1_000),
            price_update("okx", "SOL", 1_100),
        ];

        let plan = build_testnet_bridge_tick(&config, &markets, &updates, &feeder_id(1), 1_200)
            .expect("bridge tick should plan");

        assert_eq!(plan.unmapped_assets, Vec::<String>::new());
        assert_eq!(plan.omitted_assets, Vec::<String>::new());
        assert_eq!(plan.aggregates.len(), 2);
        assert_eq!(
            plan.requests
                .iter()
                .map(|request| request.market_id)
                .collect::<Vec<_>>(),
            vec![7, 8]
        );
        assert_eq!(plan.requests[0].samples[0].feeder_id_hex, feeder_id(1));
    }

    #[test]
    fn bridge_tick_reports_allowed_unmapped_assets() {
        let config = bridge_config();
        let markets = vec![
            MarketSnapshot {
                market_id: 7,
                oracle_feed_id: 77,
                base_symbol: "BTC".into(),
                status: MarketStatus::Active,
                oracle_feeder_ids_hex: vec![feeder_id(1)],
            },
            MarketSnapshot {
                market_id: 8,
                oracle_feed_id: 78,
                base_symbol: "SOL".into(),
                status: MarketStatus::Active,
                oracle_feeder_ids_hex: vec![feeder_id(1)],
            },
            MarketSnapshot {
                market_id: 9,
                oracle_feed_id: 79,
                base_symbol: "DOGE".into(),
                status: MarketStatus::Active,
                oracle_feeder_ids_hex: vec![feeder_id(2)],
            },
        ];
        let updates = vec![
            price_update("binance", "BTC", 100_000),
            price_update("okx", "BTC", 101_000),
            price_update("binance", "SOL", 1_000),
            price_update("okx", "SOL", 1_100),
        ];

        let plan = build_testnet_bridge_tick(&config, &markets, &updates, &feeder_id(1), 1_200)
            .expect("bridge tick should plan");

        assert_eq!(plan.unmapped_assets, vec!["SOL".to_string()]);
        assert_eq!(plan.requests.len(), 1);
        assert_eq!(plan.requests[0].market_id, 7);
    }

    #[test]
    fn bridge_tick_omits_assets_without_local_quorum() {
        let config = bridge_config();
        let markets = vec![MarketSnapshot {
            market_id: 7,
            oracle_feed_id: 77,
            base_symbol: "BTC".into(),
            status: MarketStatus::Active,
            oracle_feeder_ids_hex: vec![feeder_id(1)],
        }];
        let updates = vec![price_update("binance", "BTC", 100_000)];

        let plan = build_testnet_bridge_tick(&config, &markets, &updates, &feeder_id(1), 1_200)
            .expect("bridge tick should plan");

        assert_eq!(plan.omitted_assets, vec!["BTC".to_string()]);
        assert!(plan.requests.is_empty());
    }
}
