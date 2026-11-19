use futures_util::{SinkExt, StreamExt};
use kombat_chain_feeder::{
    build_testnet_bridge_tick, live_adapter_targets, parse_binance_book_ticker,
    parse_coinbase_ticker, parse_okx_ticker, ChainRpcClient, FeederConfig, LiveAdapterTarget,
    PriceUpdate,
};
use std::collections::BTreeMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const DEFAULT_SUBMIT_INTERVAL_MS: u64 = 10_000;
const MAX_RECONNECT_BACKOFF_MS: u64 = 30_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CliArgs {
    config_path: String,
    feeder_id_hex: Option<String>,
    live: bool,
    once: bool,
    prices_json_path: Option<String>,
    submit: bool,
    submit_interval_ms: u64,
}

#[tokio::main]
async fn main() {
    let args = match parse_args(std::env::args().collect::<Vec<_>>()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };

    let config_text = match fs::read_to_string(&args.config_path) {
        Ok(config_text) => config_text,
        Err(error) => {
            eprintln!("failed to read feeder config: {error}");
            std::process::exit(1);
        }
    };

    let mut config = match FeederConfig::from_toml_str(&config_text) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("invalid feeder config: {error:?}");
            std::process::exit(1);
        }
    };

    if let Err(error) = config.apply_env_overrides(std::env::vars()) {
        eprintln!("invalid feeder env override: {error:?}");
        std::process::exit(1);
    }

    if args.once {
        if let Err(error) = run_once(&config, &args) {
            eprintln!("feeder one-shot failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    if args.live {
        if let Err(error) = run_live(config, args).await {
            eprintln!("feeder live mode failed: {error}");
            std::process::exit(1);
        }
        return;
    }

    println!(
        "kombat-feeder config ok (dry-run) chain_id={} metrics={}",
        config.feeder.chain_id, config.feeder.metrics_listen
    );
}

fn parse_args(args: Vec<String>) -> Result<CliArgs, String> {
    let mut config_path = None;
    let mut feeder_id_hex = None;
    let mut live = false;
    let mut once = false;
    let mut prices_json_path = None;
    let mut submit = false;
    let mut submit_interval_ms = DEFAULT_SUBMIT_INTERVAL_MS;

    let mut iter = args.into_iter();
    let _program = iter.next();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--config" => {
                config_path = Some(
                    iter.next()
                        .ok_or_else(|| "--config requires a path".to_string())?,
                );
            }
            "--feeder-id" => {
                feeder_id_hex = Some(
                    iter.next()
                        .ok_or_else(|| "--feeder-id requires a 32-byte hex id".to_string())?,
                );
            }
            "--live" => live = true,
            "--once" => once = true,
            "--prices-json" => {
                prices_json_path = Some(
                    iter.next()
                        .ok_or_else(|| "--prices-json requires a path".to_string())?,
                );
            }
            "--submit" => submit = true,
            "--submit-interval-ms" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--submit-interval-ms requires a value".to_string())?;
                submit_interval_ms = value
                    .parse::<u64>()
                    .map_err(|_| "--submit-interval-ms must be a positive integer".to_string())?;
                if submit_interval_ms == 0 {
                    return Err("--submit-interval-ms must be nonzero".into());
                }
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let config_path = config_path.ok_or_else(|| "--config is required".to_string())?;
    if once && live {
        return Err("--once and --live are mutually exclusive".into());
    }
    if once {
        if feeder_id_hex.is_none() {
            return Err("--once requires --feeder-id".into());
        }
        if prices_json_path.is_none() {
            return Err("--once requires --prices-json".into());
        }
    }
    if live && feeder_id_hex.is_none() {
        return Err("--live requires --feeder-id".into());
    }

    Ok(CliArgs {
        config_path,
        feeder_id_hex,
        live,
        once,
        prices_json_path,
        submit,
        submit_interval_ms,
    })
}

fn usage() -> &'static str {
    "usage: kombat_feeder --config /etc/kombat/feeder.toml [--once --prices-json updates.json --feeder-id 0x... [--submit]] [--live --feeder-id 0x... [--submit] [--submit-interval-ms 10000]]"
}

fn run_once(config: &FeederConfig, args: &CliArgs) -> Result<(), String> {
    if !config.submission.testnet_bridge.enabled {
        return Err("testnet bridge submission is disabled in config".into());
    }
    let prices_path = args
        .prices_json_path
        .as_deref()
        .ok_or_else(|| "--prices-json is required".to_string())?;
    let feeder_id_hex = args
        .feeder_id_hex
        .as_deref()
        .ok_or_else(|| "--feeder-id is required".to_string())?;
    let price_text = fs::read_to_string(prices_path)
        .map_err(|error| format!("failed to read price updates: {error}"))?;
    let updates = serde_json::from_str::<Vec<PriceUpdate>>(&price_text)
        .map_err(|error| format!("invalid price updates json: {error}"))?;

    let client = ChainRpcClient::from_config(config).map_err(|error| format!("{error:?}"))?;
    let markets = client
        .list_perpetual_markets()
        .map_err(|error| format!("{error:?}"))?;
    let timestamp_ms = now_ms()?;
    let plan = build_testnet_bridge_tick(config, &markets, &updates, feeder_id_hex, timestamp_ms)
        .map_err(|error| format!("{error:?}"))?;

    println!(
        "kombat-feeder one-shot markets={} aggregates={} requests={} omitted_assets={} unmapped_assets={}",
        markets.len(),
        plan.aggregates.len(),
        plan.requests.len(),
        plan.omitted_assets.len(),
        plan.unmapped_assets.len()
    );
    if !plan.omitted_assets.is_empty() {
        eprintln!("omitted_assets={}", plan.omitted_assets.join(","));
    }
    if !plan.unmapped_assets.is_empty() {
        eprintln!("unmapped_assets={}", plan.unmapped_assets.join(","));
    }

    if args.submit {
        let responses = client
            .submit_bridge_requests(&plan.requests)
            .map_err(|error| format!("{error:?}"))?;
        println!("submitted_requests={}", responses.len());
    } else {
        let body = serde_json::to_string_pretty(&plan.requests)
            .map_err(|error| format!("failed to encode dry-run requests: {error}"))?;
        println!("{body}");
    }

    Ok(())
}

async fn run_live(config: FeederConfig, args: CliArgs) -> Result<(), String> {
    if !config.submission.testnet_bridge.enabled {
        return Err("testnet bridge submission is disabled in config".into());
    }
    let feeder_id_hex = args
        .feeder_id_hex
        .as_deref()
        .ok_or_else(|| "--feeder-id is required".to_string())?
        .to_string();
    let targets = live_adapter_targets(&config);
    if targets.is_empty() {
        return Err("no enabled live adapter targets in config".into());
    }

    let (tx, mut rx) = mpsc::channel::<PriceUpdate>(1024);
    for target in targets.clone() {
        let tx = tx.clone();
        tokio::spawn(async move {
            run_live_adapter_forever(target, tx).await;
        });
    }
    drop(tx);

    println!(
        "kombat-feeder live targets={} submit={} interval_ms={}",
        targets.len(),
        args.submit,
        args.submit_interval_ms
    );

    let mut latest = BTreeMap::<(String, String), PriceUpdate>::new();
    let mut tick = interval(Duration::from_millis(args.submit_interval_ms));
    loop {
        tokio::select! {
            Some(update) = rx.recv() => {
                latest.insert((update.adapter.clone(), update.asset.clone()), update);
            }
            _ = tick.tick() => {
                let updates = latest.values().cloned().collect::<Vec<_>>();
                if updates.is_empty() {
                    eprintln!("live tick skipped: no exchange updates received yet");
                    continue;
                }
                submit_live_tick(&config, &args, &feeder_id_hex, &updates)?;
            }
            else => return Err("all live adapter tasks stopped".into()),
        }
    }
}

async fn run_live_adapter_forever(target: LiveAdapterTarget, tx: mpsc::Sender<PriceUpdate>) {
    let mut backoff_ms = target.reconnect_backoff_ms.max(100);
    loop {
        match run_live_adapter_once(&target, &tx).await {
            Ok(()) => {
                eprintln!(
                    "adapter {} {} stream closed; reconnecting",
                    target.adapter, target.asset
                );
                backoff_ms = target.reconnect_backoff_ms.max(100);
            }
            Err(error) => {
                eprintln!(
                    "adapter {} {} failed: {error}; reconnecting in {}ms",
                    target.adapter, target.asset, backoff_ms
                );
            }
        }
        sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = backoff_ms.saturating_mul(2).min(MAX_RECONNECT_BACKOFF_MS);
    }
}

async fn run_live_adapter_once(
    target: &LiveAdapterTarget,
    tx: &mpsc::Sender<PriceUpdate>,
) -> Result<(), String> {
    match target.adapter.as_str() {
        "binance" => run_binance_stream(target, tx).await,
        "okx" => run_okx_stream(target, tx).await,
        "coinbase" => run_coinbase_stream(target, tx).await,
        adapter => Err(format!("unsupported live adapter: {adapter}")),
    }
}

async fn run_binance_stream(
    target: &LiveAdapterTarget,
    tx: &mpsc::Sender<PriceUpdate>,
) -> Result<(), String> {
    let pair = target.pair.replace(['-', '/'], "").to_ascii_lowercase();
    let url = format!(
        "{}/{}@bookTicker",
        target.ws_url.trim_end_matches('/'),
        pair
    );
    let (mut ws, _) = connect_async(&url)
        .await
        .map_err(|error| format!("connect {url}: {error}"))?;
    println!("adapter binance {} connected {}", target.asset, url);
    let mut source_seq = 0_u64;
    while let Some(message) = ws.next().await {
        let message = message.map_err(|error| error.to_string())?;
        if let Some(text) = message_text_or_pong(message, &mut ws).await? {
            source_seq = source_seq.saturating_add(1);
            let received_at_ms = now_ms()?;
            if let Some(update) =
                parse_binance_book_ticker(&target.asset, &text, received_at_ms, source_seq)
            {
                send_update(tx, update).await?;
            }
        }
    }
    Ok(())
}

async fn run_okx_stream(
    target: &LiveAdapterTarget,
    tx: &mpsc::Sender<PriceUpdate>,
) -> Result<(), String> {
    let (mut ws, _) = connect_async(&target.ws_url)
        .await
        .map_err(|error| format!("connect {}: {error}", target.ws_url))?;
    ws.send(Message::Text(
        serde_json::json!({
            "op": "subscribe",
            "args": [{"channel": "tickers", "instId": target.pair}],
        })
        .to_string()
        .into(),
    ))
    .await
    .map_err(|error| format!("okx subscribe failed: {error}"))?;
    println!("adapter okx {} connected {}", target.asset, target.ws_url);
    let mut source_seq = 0_u64;
    while let Some(message) = ws.next().await {
        let message = message.map_err(|error| error.to_string())?;
        if let Some(text) = message_text_or_pong(message, &mut ws).await? {
            source_seq = source_seq.saturating_add(1);
            let received_at_ms = now_ms()?;
            if let Some(update) = parse_okx_ticker(&target.asset, &text, received_at_ms, source_seq)
            {
                send_update(tx, update).await?;
            }
        }
    }
    Ok(())
}

async fn run_coinbase_stream(
    target: &LiveAdapterTarget,
    tx: &mpsc::Sender<PriceUpdate>,
) -> Result<(), String> {
    let (mut ws, _) = connect_async(&target.ws_url)
        .await
        .map_err(|error| format!("connect {}: {error}", target.ws_url))?;
    ws.send(Message::Text(
        serde_json::json!({
            "type": "subscribe",
            "product_ids": [target.pair],
            "channel": "ticker",
        })
        .to_string()
        .into(),
    ))
    .await
    .map_err(|error| format!("coinbase subscribe failed: {error}"))?;
    println!(
        "adapter coinbase {} connected {}",
        target.asset, target.ws_url
    );
    let mut source_seq = 0_u64;
    while let Some(message) = ws.next().await {
        let message = message.map_err(|error| error.to_string())?;
        if let Some(text) = message_text_or_pong(message, &mut ws).await? {
            source_seq = source_seq.saturating_add(1);
            let received_at_ms = now_ms()?;
            if let Some(update) =
                parse_coinbase_ticker(&target.asset, &text, received_at_ms, source_seq)
            {
                send_update(tx, update).await?;
            }
        }
    }
    Ok(())
}

async fn message_text_or_pong<S>(message: Message, ws: &mut S) -> Result<Option<String>, String>
where
    S: SinkExt<Message> + Unpin,
    <S as futures_util::Sink<Message>>::Error: std::fmt::Display,
{
    match message {
        Message::Text(text) => Ok(Some(text.to_string())),
        Message::Binary(_) => Ok(None),
        Message::Ping(payload) => {
            ws.send(Message::Pong(payload))
                .await
                .map_err(|error| error.to_string())?;
            Ok(None)
        }
        Message::Pong(_) => Ok(None),
        Message::Close(_) => Err("websocket closed".into()),
        Message::Frame(_) => Ok(None),
    }
}

async fn send_update(tx: &mpsc::Sender<PriceUpdate>, update: PriceUpdate) -> Result<(), String> {
    tx.send(update)
        .await
        .map_err(|_| "live update channel closed".to_string())
}

fn submit_live_tick(
    config: &FeederConfig,
    args: &CliArgs,
    feeder_id_hex: &str,
    updates: &[PriceUpdate],
) -> Result<(), String> {
    let client = ChainRpcClient::from_config(config).map_err(|error| format!("{error:?}"))?;
    let markets = client
        .list_perpetual_markets()
        .map_err(|error| format!("{error:?}"))?;
    let timestamp_ms = now_ms()?;
    let plan = build_testnet_bridge_tick(config, &markets, updates, feeder_id_hex, timestamp_ms)
        .map_err(|error| format!("{error:?}"))?;

    if !plan.omitted_assets.is_empty() {
        eprintln!("live tick omitted_assets={}", plan.omitted_assets.join(","));
    }
    if !plan.unmapped_assets.is_empty() {
        eprintln!(
            "live tick unmapped_assets={}",
            plan.unmapped_assets.join(",")
        );
    }
    if plan.requests.is_empty() {
        eprintln!(
            "live tick no bridge requests markets={} aggregates={} updates={}",
            markets.len(),
            plan.aggregates.len(),
            updates.len()
        );
        return Ok(());
    }

    if args.submit {
        let responses = client
            .submit_bridge_requests(&plan.requests)
            .map_err(|error| format!("{error:?}"))?;
        println!(
            "live tick submitted requests={} aggregates={} updates={}",
            responses.len(),
            plan.aggregates.len(),
            updates.len()
        );
    } else {
        let body = serde_json::to_string(&plan.requests)
            .map_err(|error| format!("failed to encode dry-run requests: {error}"))?;
        println!("live tick dry-run {body}");
    }

    Ok(())
}

fn now_ms() -> Result<u64, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before unix epoch: {error}"))?;
    u64::try_from(elapsed.as_millis()).map_err(|_| "system time overflows u64 millis".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_check_args() {
        let args = parse_args(vec![
            "kombat_feeder".into(),
            "--config".into(),
            "/tmp/feeder.toml".into(),
        ])
        .expect("args should parse");

        assert_eq!(args.config_path, "/tmp/feeder.toml");
        assert!(!args.live);
        assert!(!args.once);
        assert!(!args.submit);
        assert_eq!(args.submit_interval_ms, DEFAULT_SUBMIT_INTERVAL_MS);
    }

    #[test]
    fn parses_once_submit_args() {
        let args = parse_args(vec![
            "kombat_feeder".into(),
            "--config".into(),
            "/tmp/feeder.toml".into(),
            "--once".into(),
            "--prices-json".into(),
            "/tmp/prices.json".into(),
            "--feeder-id".into(),
            "0x01".into(),
            "--submit".into(),
        ])
        .expect("args should parse");

        assert!(args.once);
        assert!(args.submit);
        assert_eq!(args.prices_json_path.as_deref(), Some("/tmp/prices.json"));
        assert_eq!(args.feeder_id_hex.as_deref(), Some("0x01"));
    }

    #[test]
    fn parses_live_args() {
        let args = parse_args(vec![
            "kombat_feeder".into(),
            "--config".into(),
            "/tmp/feeder.toml".into(),
            "--live".into(),
            "--feeder-id".into(),
            "0x01".into(),
            "--submit-interval-ms".into(),
            "2500".into(),
        ])
        .expect("args should parse");

        assert!(args.live);
        assert!(!args.once);
        assert_eq!(args.submit_interval_ms, 2_500);
    }

    #[test]
    fn once_requires_prices_and_feeder_id() {
        let error = parse_args(vec![
            "kombat_feeder".into(),
            "--config".into(),
            "/tmp/feeder.toml".into(),
            "--once".into(),
        ])
        .expect_err("one-shot mode should require its inputs");

        assert_eq!(error, "--once requires --feeder-id");
    }

    #[test]
    fn live_requires_feeder_id() {
        let error = parse_args(vec![
            "kombat_feeder".into(),
            "--config".into(),
            "/tmp/feeder.toml".into(),
            "--live".into(),
        ])
        .expect_err("live mode should require a feeder id");

        assert_eq!(error, "--live requires --feeder-id");
    }
}
