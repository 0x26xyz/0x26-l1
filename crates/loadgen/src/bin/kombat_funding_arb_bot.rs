//! Funding-arb bot: pushes mark price toward index price by submitting
//! IOC orders sized to the configured arb parameters. Optionally
//! emits a small noise IOC tick so candles aren't dead between arb
//! events.
//!
//! This bot is the candle/liquidity workhorse for the testnet. It
//! does NOT carry alpha; it implements the same mean-revert-to-index
//! pressure that real perp arbs apply. See RFC 045 (StrategyMandate)
//! for the on-chain typed-operator equivalent — this bot is the
//! reference implementation that informs that operator's parameters.

use and_chain_sdk::{
    AndChainClient, IntentKind, IntentRequest, Keypair, OrderKind, OrderSide, OwnerAction,
    OwnerCallRequest, PolicyTermsBuilder, RpcError, SubmitError, TimeInForce,
};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Instant};

#[derive(Clone, Debug, Parser)]
#[command(name = "kombat_funding_arb_bot")]
#[command(
    about = "Mean-revert-to-index taker bot. Submits IOC orders when mark drifts from index by more than threshold_bps."
)]
struct Cli {
    #[arg(long, env = "KOMBAT_FUNDING_ARB_RPC_URL")]
    rpc_url: String,

    #[arg(long, env = "KOMBAT_OPERATOR_TOKEN")]
    operator_token: Option<String>,

    #[arg(long, env = "KOMBAT_FUNDING_ARB_CHAIN_ID", default_value_t = 7)]
    chain_id: u64,

    #[arg(long, env = "KOMBAT_FUNDING_ARB_ACCOUNT_ID")]
    account_id: u64,

    #[arg(long, env = "KOMBAT_FUNDING_ARB_OWNER_SECRET_HEX")]
    owner_secret_hex: String,

    #[arg(long, env = "KOMBAT_FUNDING_ARB_SESSION_SECRET_HEX")]
    session_secret_hex: String,

    #[arg(long, env = "KOMBAT_FUNDING_ARB_POLICY_ID")]
    policy_id: u64,

    #[arg(long, env = "KOMBAT_FUNDING_ARB_MARKET_IDS", value_delimiter = ',')]
    market_ids: Vec<u64>,

    /// Submit an IOC when |mark - index| / index >= this threshold (bps).
    #[arg(long, env = "KOMBAT_FUNDING_ARB_THRESHOLD_BPS", default_value_t = 10)]
    threshold_bps: u64,

    /// IOC notional per arb fire, in lots.
    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_SIZE_LOTS",
        default_value_t = 1_000_000
    )]
    size_lots: u64,

    /// Cap the IOC limit price relative to index, so a stale mark
    /// doesn't let a runaway IOC sweep the entire book.
    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_MAX_SLIPPAGE_BPS",
        default_value_t = 50
    )]
    max_slippage_bps: u64,

    /// Poll every N ms across all configured markets.
    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_POLL_INTERVAL_MS",
        default_value_t = 5_000
    )]
    poll_interval_ms: u64,

    /// Optional. If non-zero, every poll has `noise_probability_pct`
    /// chance of submitting a `noise_size_lots` IOC in a deterministic
    /// pseudo-random direction — fills in candle wicks so flat bars
    /// between arb events still show micro-volume.
    #[arg(long, env = "KOMBAT_FUNDING_ARB_NOISE_SIZE_LOTS", default_value_t = 0)]
    noise_size_lots: u64,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_NOISE_PROBABILITY_PCT",
        default_value_t = 25
    )]
    noise_probability_pct: u8,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_INTENT_TTL_MS",
        default_value_t = 60_000
    )]
    intent_ttl_ms: u64,

    /// Bot-side throttle: don't submit if the previous intent for
    /// this market hasn't finalized yet (avoids stacking unfilled
    /// IOCs when the chain is slow to commit).
    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_INTENT_STATUS_TIMEOUT_MS",
        default_value_t = 15_000
    )]
    intent_status_timeout_ms: u64,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_INTENT_STATUS_POLL_MS",
        default_value_t = 250
    )]
    intent_status_poll_ms: u64,

    /// If set, ensure the policy exists before quoting (CreatePolicy
    /// owner call). Idempotent: skips if the policy already exists.
    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_ENSURE_POLICY",
        default_value_t = false
    )]
    ensure_policy: bool,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_POLICY_EXPIRES_AT",
        default_value_t = 4_102_444_800_000
    )]
    policy_expires_at: u64,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_POLICY_MAX_INTENTS_PER_SECOND",
        default_value_t = 5
    )]
    policy_max_intents_per_second: u32,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_POLICY_MAX_INTENTS_PER_DAY",
        default_value_t = 100_000
    )]
    policy_max_intents_per_day: u32,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_POLICY_MAX_VOLUME_PER_DAY_MICRO_USDC",
        default_value_t = 1_000_000_000_000_000_u128
    )]
    policy_max_volume_per_day_micro_usdc: u128,

    #[arg(
        long,
        env = "KOMBAT_FUNDING_ARB_BOT_LABEL",
        default_value = "funding-arb"
    )]
    bot_label: String,

    /// Run a single poll then exit. For tests and ad-hoc invocations.
    #[arg(long, default_value_t = false)]
    loop_once: bool,
}

#[derive(Clone, Debug)]
struct PendingFundingIntent {
    intent_id_hex: String,
    nonce: u64,
}

#[derive(Debug)]
enum FundingIntentOutcome {
    Executed,
    Rejected {
        message: String,
    },
    TimedOut {
        intent_id_hex: String,
        last_stage: String,
        pending_reason: Option<String>,
    },
}

#[derive(Debug)]
enum PendingFundingOutcome {
    StillPending,
    Executed,
    Rejected { message: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.market_ids.is_empty() {
        return Err(anyhow!("at least one market id is required"));
    }
    if cli.threshold_bps == 0 {
        return Err(anyhow!("threshold_bps must be greater than zero"));
    }
    if cli.size_lots == 0 {
        return Err(anyhow!("size_lots must be greater than zero"));
    }
    if cli.noise_probability_pct > 100 {
        return Err(anyhow!("noise_probability_pct must be in [0, 100]"));
    }

    let owner = Keypair::from_secret_hex(&cli.owner_secret_hex)
        .context("invalid funding-arb owner secret")?;
    let session = Keypair::from_secret_hex(&cli.session_secret_hex)
        .context("invalid funding-arb session secret")?;
    let client = build_client(&cli.rpc_url, cli.operator_token.as_deref());

    if cli.ensure_policy {
        ensure_policy(&client, &cli, &owner, &session).await?;
    }
    let mut next_nonce = client
        .next_intent_nonce(cli.policy_id)
        .await
        .with_context(|| format!("fetch next nonce for policy {}", cli.policy_id))?;

    eprintln!(
        "kombat_funding_arb_bot[{}]: markets={:?} threshold_bps={} size_lots={} \
         max_slippage_bps={} poll_ms={} noise_size_lots={}",
        cli.bot_label,
        cli.market_ids,
        cli.threshold_bps,
        cli.size_lots,
        cli.max_slippage_bps,
        cli.poll_interval_ms,
        cli.noise_size_lots,
    );

    let mut tick: u64 = 0;
    let mut pending_intent: Option<PendingFundingIntent> = None;
    loop {
        if let Some(pending) = pending_intent.clone() {
            match poll_pending_intent(&client, &pending).await {
                Ok(PendingFundingOutcome::StillPending) => {
                    sleep(Duration::from_millis(cli.poll_interval_ms)).await;
                    continue;
                }
                Ok(PendingFundingOutcome::Executed) => {
                    next_nonce = refresh_next_nonce(&client, &cli, pending.nonce + 1).await?;
                    eprintln!(
                        "kombat_funding_arb_bot[{}]: pending intent {} executed nonce={} refreshed_next_nonce={}",
                        cli.bot_label, pending.intent_id_hex, pending.nonce, next_nonce
                    );
                    pending_intent = None;
                }
                Ok(PendingFundingOutcome::Rejected { message }) => {
                    next_nonce = refresh_next_nonce(&client, &cli, pending.nonce).await?;
                    eprintln!(
                        "kombat_funding_arb_bot[{}]: pending intent {} terminal {}; refreshed_next_nonce={}",
                        cli.bot_label, pending.intent_id_hex, message, next_nonce
                    );
                    pending_intent = None;
                }
                Err(error) => {
                    eprintln!(
                        "kombat_funding_arb_bot[{}]: pending intent {} status check failed: {error:#}",
                        cli.bot_label, pending.intent_id_hex
                    );
                    sleep(Duration::from_millis(cli.poll_interval_ms)).await;
                    continue;
                }
            }
        }

        for &market_id in &cli.market_ids {
            if let Err(error) = evaluate_market(
                &client,
                &cli,
                &session,
                market_id,
                &mut next_nonce,
                &mut pending_intent,
                tick,
            )
            .await
            {
                eprintln!(
                    "kombat_funding_arb_bot[{}]: market={} tick={} error={error:#}",
                    cli.bot_label, market_id, tick
                );
            }
            if pending_intent.is_some() {
                break;
            }
        }
        tick = tick.saturating_add(1);
        if cli.loop_once {
            break;
        }
        sleep(Duration::from_millis(cli.poll_interval_ms)).await;
    }
    Ok(())
}

async fn evaluate_market(
    client: &AndChainClient,
    cli: &Cli,
    session: &Keypair,
    market_id: u64,
    next_nonce: &mut u64,
    pending_intent: &mut Option<PendingFundingIntent>,
    tick: u64,
) -> Result<()> {
    let state = client
        .get_market_state(market_id)
        .await
        .with_context(|| format!("get market state for {market_id}"))?;
    let mark = state
        .mark_price_ticks
        .ok_or_else(|| anyhow!("market {market_id} has no mark price"))?;
    let index = state
        .index_price_ticks
        .ok_or_else(|| anyhow!("market {market_id} has no index price"))?;
    if state.mark_price_stale || state.index_price_stale {
        return Err(anyhow!(
            "stale prices on market {market_id}: mark_stale={} index_stale={}",
            state.mark_price_stale,
            state.index_price_stale
        ));
    }
    if mark == 0 || index == 0 {
        return Err(anyhow!("zero price on market {market_id}"));
    }

    let deviation_bps = signed_deviation_bps(mark, index);
    let threshold = cli.threshold_bps as i64;

    if deviation_bps >= threshold {
        // Mark > index → sell to push mark down.
        let limit_price = limit_price_for(OrderSide::Ask, index, cli.max_slippage_bps);
        submit_ioc(
            client,
            cli,
            session,
            market_id,
            OrderSide::Ask,
            cli.size_lots,
            limit_price,
            next_nonce,
            pending_intent,
            "arb-sell",
            deviation_bps,
        )
        .await?;
    } else if deviation_bps <= -threshold {
        // Mark < index → buy to push mark up.
        let limit_price = limit_price_for(OrderSide::Bid, index, cli.max_slippage_bps);
        submit_ioc(
            client,
            cli,
            session,
            market_id,
            OrderSide::Bid,
            cli.size_lots,
            limit_price,
            next_nonce,
            pending_intent,
            "arb-buy",
            deviation_bps,
        )
        .await?;
    } else if cli.noise_size_lots > 0 && noise_fires_this_tick(cli, market_id, tick) {
        let side = noise_side(market_id, tick);
        let limit_price = limit_price_for(side, index, cli.max_slippage_bps);
        submit_ioc(
            client,
            cli,
            session,
            market_id,
            side,
            cli.noise_size_lots,
            limit_price,
            next_nonce,
            pending_intent,
            "noise",
            deviation_bps,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn submit_ioc(
    client: &AndChainClient,
    cli: &Cli,
    session: &Keypair,
    market_id: u64,
    side: OrderSide,
    quantity_lots: u64,
    limit_price_ticks: u64,
    next_nonce: &mut u64,
    pending_intent: &mut Option<PendingFundingIntent>,
    reason: &str,
    deviation_bps: i64,
) -> Result<()> {
    let ts_ms = now_ms();
    let nonce = *next_nonce;
    let request = IntentRequest {
        chain_id: cli.chain_id,
        account_id: cli.account_id,
        policy_id: cli.policy_id,
        nonce,
        expires_at: ts_ms.saturating_add(cli.intent_ttl_ms),
        predicate: None,
        kind: IntentKind::PlaceOrder {
            market_id,
            client_order_id: derive_client_order_id(market_id, nonce),
            side,
            quantity_lots,
            kind: OrderKind::Limit {
                price_ticks: limit_price_ticks,
            },
            time_in_force: TimeInForce::Ioc,
            post_only: false,
        },
    };
    match client.submit_typed_intent(session, request, ts_ms).await {
        Ok(response) => {
            match wait_for_intent_terminal(client, &response.intent_id_hex, cli).await? {
                FundingIntentOutcome::Executed => {
                    *next_nonce = refresh_next_nonce(client, cli, nonce.saturating_add(1)).await?;
                    eprintln!(
                        "kombat_funding_arb_bot[{}]: executed reason={} market={} side={:?} qty={} \
                         limit_ticks={} deviation_bps={} nonce={} intent_id={} next_nonce={}",
                        cli.bot_label,
                        reason,
                        market_id,
                        side,
                        quantity_lots,
                        limit_price_ticks,
                        deviation_bps,
                        nonce,
                        response.intent_id_hex,
                        *next_nonce,
                    );
                }
                FundingIntentOutcome::Rejected { message } => {
                    *next_nonce = refresh_next_nonce(client, cli, nonce).await?;
                    return Err(anyhow!(
                        "intent {} terminal after submit: {}; refreshed_next_nonce={}",
                        response.intent_id_hex,
                        message,
                        *next_nonce
                    ));
                }
                FundingIntentOutcome::TimedOut {
                    intent_id_hex,
                    last_stage,
                    pending_reason,
                } => {
                    *pending_intent = Some(PendingFundingIntent {
                        intent_id_hex: intent_id_hex.clone(),
                        nonce,
                    });
                    return Err(anyhow!(
                        "intent {} still pending after timeout; last_stage={} pending_reason={:?}; policy lane paused",
                        intent_id_hex,
                        last_stage,
                        pending_reason
                    ));
                }
            }
        }
        Err(SubmitError::Rpc(rpc_err))
            if rpc_reason(&rpc_err).as_deref() == Some("nonce_mismatch") =>
        {
            // Recover from nonce drift (another submission won the
            // race). Re-fetch and let the next tick try again.
            *next_nonce = client
                .next_intent_nonce(cli.policy_id)
                .await
                .with_context(|| format!("re-fetch nonce after drift on market {market_id}"))?;
            eprintln!(
                "kombat_funding_arb_bot[{}]: nonce drift on market={} reason={} \
                 refreshed_next_nonce={}",
                cli.bot_label, market_id, reason, *next_nonce
            );
        }
        Err(error) if ambiguous_submit_intent_id(&error).is_some() => {
            let intent_id_hex = ambiguous_submit_intent_id(&error).expect("checked above");
            *pending_intent = Some(PendingFundingIntent {
                intent_id_hex: intent_id_hex.clone(),
                nonce,
            });
            return Err(anyhow!(
                "ambiguous submit policy={} nonce={} market={} reason={} intent_id={} details={}; policy lane paused",
                cli.policy_id,
                nonce,
                market_id,
                reason,
                intent_id_hex,
                submit_error_details(&error)
            ));
        }
        Err(error) if duplicate_policy_nonce_intent_id(&error).is_some() => {
            let intent_id_hex = duplicate_policy_nonce_intent_id(&error).expect("checked above");
            match wait_for_intent_terminal(client, &intent_id_hex, cli).await? {
                FundingIntentOutcome::Executed => {
                    *next_nonce = refresh_next_nonce(client, cli, nonce.saturating_add(1)).await?;
                    eprintln!(
                        "kombat_funding_arb_bot[{}]: duplicate nonce policy={} nonce={} resolved by existing executed intent {}; refreshed_next_nonce={}",
                        cli.bot_label, cli.policy_id, nonce, intent_id_hex, *next_nonce
                    );
                }
                FundingIntentOutcome::Rejected { message } => {
                    *next_nonce = refresh_next_nonce(client, cli, nonce).await?;
                    return Err(anyhow!(
                        "duplicate nonce existing intent {} terminal {}; refreshed_next_nonce={}",
                        intent_id_hex,
                        message,
                        *next_nonce
                    ));
                }
                FundingIntentOutcome::TimedOut {
                    intent_id_hex,
                    last_stage,
                    pending_reason,
                } => {
                    *pending_intent = Some(PendingFundingIntent {
                        intent_id_hex: intent_id_hex.clone(),
                        nonce,
                    });
                    return Err(anyhow!(
                        "duplicate nonce existing intent {} still pending; last_stage={} pending_reason={:?}; policy lane paused",
                        intent_id_hex,
                        last_stage,
                        pending_reason
                    ));
                }
            }
        }
        Err(error) => {
            return Err(anyhow!(
                "submit IOC failed market={market_id} reason={reason}: {}",
                submit_error_details(&error)
            ));
        }
    }
    Ok(())
}

async fn wait_for_intent_terminal(
    client: &AndChainClient,
    intent_id_hex: &str,
    cli: &Cli,
) -> Result<FundingIntentOutcome> {
    let deadline = Instant::now() + Duration::from_millis(cli.intent_status_timeout_ms);
    loop {
        let status = client.get_intent_status(intent_id_hex).await?;
        match status.stage.as_str() {
            "executed" => return Ok(FundingIntentOutcome::Executed),
            "rejected" | "expired" => {
                return Ok(FundingIntentOutcome::Rejected {
                    message: format!(
                        "stage={} rejection={:?} order_rejection={:?}",
                        status.stage, status.rejection_reason, status.order_rejection_reason
                    ),
                });
            }
            _ if Instant::now() >= deadline => {
                return Ok(FundingIntentOutcome::TimedOut {
                    intent_id_hex: intent_id_hex.to_owned(),
                    last_stage: status.stage,
                    pending_reason: status.pending_reason,
                });
            }
            _ => sleep(Duration::from_millis(cli.intent_status_poll_ms)).await,
        }
    }
}

async fn poll_pending_intent(
    client: &AndChainClient,
    pending: &PendingFundingIntent,
) -> Result<PendingFundingOutcome> {
    let status = client.get_intent_status(&pending.intent_id_hex).await?;
    match status.stage.as_str() {
        "executed" => Ok(PendingFundingOutcome::Executed),
        "rejected" | "expired" => Ok(PendingFundingOutcome::Rejected {
            message: format!(
                "stage={} rejection={:?} order_rejection={:?}",
                status.stage, status.rejection_reason, status.order_rejection_reason
            ),
        }),
        _ => Ok(PendingFundingOutcome::StillPending),
    }
}

async fn refresh_next_nonce(client: &AndChainClient, cli: &Cli, fallback: u64) -> Result<u64> {
    client
        .next_intent_nonce(cli.policy_id)
        .await
        .with_context(|| format!("refresh next nonce for policy {}", cli.policy_id))
        .map(|nonce| nonce.max(fallback))
}

fn build_client(url: &str, token: Option<&str>) -> AndChainClient {
    let client = AndChainClient::new(url).with_timeout(Duration::from_secs(20));
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        client.with_bearer_token(token)
    } else {
        client
    }
}

async fn ensure_policy(
    client: &AndChainClient,
    cli: &Cli,
    owner: &Keypair,
    session: &Keypair,
) -> Result<()> {
    if client.get_policy(cli.policy_id).await.is_ok() {
        return Ok(());
    }
    let terms = PolicyTermsBuilder::new()
        .gas_budget_per_day_microusdc(10_000_000_000)
        .gas_refill_per_day_microusdc(10_000_000_000)
        .max_intents_per_second(cli.policy_max_intents_per_second)
        .max_intents_per_day(cli.policy_max_intents_per_day)
        .max_volume_per_day_microusdc(cli.policy_max_volume_per_day_micro_usdc)
        .max_drawdown_pct(10_000)
        .build();
    let account = client
        .get_account(cli.account_id)
        .await
        .context("read funding-arb account before policy bootstrap")?;
    let request = OwnerCallRequest {
        account_id: cli.account_id,
        chain_id: cli.chain_id,
        nonce: account.nonce,
        action: OwnerAction::CreatePolicy {
            policy_id: cli.policy_id,
            grantee: session.public_bytes(),
            expires_at: cli.policy_expires_at,
            terms,
        },
    };
    match client
        .submit_typed_owner_call(owner, request, now_ms())
        .await
    {
        Ok(_) => Ok(()),
        Err(error) if rpc_reason_submit(&error).as_deref() == Some("duplicate_policy_id") => Ok(()),
        Err(error) => Err(anyhow!(
            "create funding-arb policy {} failed: {error}",
            cli.policy_id
        )),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Signed deviation in bps where positive means mark > index.
/// Saturates on overflow so degenerate prices don't panic.
fn signed_deviation_bps(mark: u64, index: u64) -> i64 {
    if index == 0 {
        return 0;
    }
    let mark_i = mark as i128;
    let index_i = index as i128;
    let diff = mark_i.saturating_sub(index_i);
    let scaled = diff.saturating_mul(10_000);
    let bps = scaled / index_i;
    bps.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// Derive the IOC limit price from the index price plus a slippage
/// buffer. For a buy (Bid), pay up to index * (1 + slippage_bps).
/// For a sell (Ask), accept down to index * (1 - slippage_bps).
fn limit_price_for(side: OrderSide, index: u64, slippage_bps: u64) -> u64 {
    let index_i = index as u128;
    let scaled = index_i.saturating_mul(slippage_bps as u128) / 10_000;
    match side {
        OrderSide::Bid => index.saturating_add(scaled as u64),
        OrderSide::Ask => index.saturating_sub(scaled as u64),
    }
}

/// Stable client_order_id per (market_id, nonce). Stable so retries
/// of the same (policy, nonce) can deduplicate downstream; unique
/// across markets so two markets don't collide in the same lane.
fn derive_client_order_id(market_id: u64, nonce: u64) -> u64 {
    // High 24 bits = market id (truncated), low 40 bits = nonce.
    // Plenty of headroom for testnet load.
    let market_lo = market_id & 0xFF_FFFF;
    let nonce_lo = nonce & 0xFF_FFFF_FFFF;
    (market_lo << 40) | nonce_lo
}

/// Deterministic noise: hash of (market_id, tick) mod 100 < pct.
/// Deterministic so multiple bots with the same config don't all
/// fire on the same tick (operators run them with different
/// `bot_label` -> different `KOMBAT_FUNDING_ARB_BOT_LABEL` env, but
/// the noise predicate is per-(market, tick) which is the same set
/// across bots; in practice run noise on exactly one bot per market
/// if you want bounded fan-out).
fn noise_fires_this_tick(cli: &Cli, market_id: u64, tick: u64) -> bool {
    let mix = market_id
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(tick.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    let bucket = (mix % 100) as u8;
    bucket < cli.noise_probability_pct
}

fn noise_side(market_id: u64, tick: u64) -> OrderSide {
    let mix = market_id
        .wrapping_mul(0x94D0_49BB_1331_11EB)
        .wrapping_add(tick.wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    if mix & 1 == 0 {
        OrderSide::Bid
    } else {
        OrderSide::Ask
    }
}

fn rpc_reason(error: &RpcError) -> Option<String> {
    match error {
        RpcError::JsonRpc { data, .. } => data
            .as_ref()
            .and_then(|value| value.get("reason"))
            .and_then(|value| value.as_str())
            .map(|reason: &str| reason.to_string()),
        _ => None,
    }
}

fn rpc_reason_submit(error: &SubmitError) -> Option<String> {
    match error {
        SubmitError::Rpc(rpc_err) => rpc_reason(rpc_err),
        _ => None,
    }
}

fn submit_error_details(error: &SubmitError) -> String {
    match error {
        SubmitError::Rpc(RpcError::JsonRpc {
            code,
            message,
            data,
        }) => match data {
            Some(data) => format!("rpc error {code}: {message} data={data}"),
            None => format!("rpc error {code}: {message}"),
        },
        _ => error.to_string(),
    }
}

fn rpc_data_string(error: &SubmitError, key: &str) -> Option<String> {
    match error {
        SubmitError::Rpc(RpcError::JsonRpc { data, .. }) => {
            data.as_ref().and_then(|data| data_string(data, key))
        }
        _ => None,
    }
}

fn data_string(data: &Value, key: &str) -> Option<String> {
    data.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

fn ambiguous_submit_intent_id(error: &SubmitError) -> Option<String> {
    match rpc_reason_submit(error).as_deref() {
        Some("local_submit_admission_timeout") | Some("local_submit_worker_disconnected") => {
            rpc_data_string(error, "intent_id_hex")
        }
        _ => None,
    }
}

fn duplicate_policy_nonce_intent_id(error: &SubmitError) -> Option<String> {
    (rpc_reason_submit(error).as_deref() == Some("duplicate_policy_nonce"))
        .then(|| rpc_data_string(error, "intent_id_hex"))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn signed_deviation_bps_zero_when_equal() {
        assert_eq!(signed_deviation_bps(1_000_000, 1_000_000), 0);
    }

    #[test]
    fn signed_deviation_bps_positive_when_mark_above_index() {
        // 10 bps higher
        assert_eq!(signed_deviation_bps(1_001_000, 1_000_000), 10);
    }

    #[test]
    fn signed_deviation_bps_negative_when_mark_below_index() {
        assert_eq!(signed_deviation_bps(999_000, 1_000_000), -10);
    }

    #[test]
    fn signed_deviation_bps_returns_zero_on_zero_index() {
        assert_eq!(signed_deviation_bps(1_000_000, 0), 0);
    }

    #[test]
    fn limit_price_for_bid_pays_up() {
        assert_eq!(limit_price_for(OrderSide::Bid, 1_000_000, 50), 1_005_000);
    }

    #[test]
    fn limit_price_for_ask_accepts_down() {
        assert_eq!(limit_price_for(OrderSide::Ask, 1_000_000, 50), 995_000);
    }

    #[test]
    fn derive_client_order_id_unique_across_markets_at_same_nonce() {
        let a = derive_client_order_id(7, 42);
        let b = derive_client_order_id(8, 42);
        assert_ne!(a, b);
    }

    #[test]
    fn derive_client_order_id_unique_across_nonces_at_same_market() {
        let a = derive_client_order_id(7, 42);
        let b = derive_client_order_id(7, 43);
        assert_ne!(a, b);
    }

    #[test]
    fn ambiguous_submit_extracts_intent_id() {
        let error = SubmitError::Rpc(RpcError::JsonRpc {
            code: -32000,
            message: "local submit admission timed out".to_owned(),
            data: Some(json!({
                "kind": "intent_submission",
                "reason": "local_submit_admission_timeout",
                "intent_id_hex": "0xabc123",
            })),
        });

        assert_eq!(
            ambiguous_submit_intent_id(&error).as_deref(),
            Some("0xabc123")
        );
        assert_eq!(duplicate_policy_nonce_intent_id(&error), None);
    }

    #[test]
    fn duplicate_nonce_extracts_intent_id_but_is_not_ambiguous_submit() {
        let error = SubmitError::Rpc(RpcError::JsonRpc {
            code: -32020,
            message: "intent submission rejected".to_owned(),
            data: Some(json!({
                "kind": "intent_submission",
                "reason": "duplicate_policy_nonce",
                "intent_id_hex": "0xdef456",
            })),
        });

        assert_eq!(ambiguous_submit_intent_id(&error), None);
        assert_eq!(
            duplicate_policy_nonce_intent_id(&error).as_deref(),
            Some("0xdef456")
        );
    }

    #[test]
    fn submit_error_details_preserves_rpc_data() {
        let error = SubmitError::Rpc(RpcError::JsonRpc {
            code: -32020,
            message: "intent submission rejected".to_owned(),
            data: Some(json!({
                "reason": "daily_volume_exceeded",
                "policy_id": 7,
            })),
        });

        let details = submit_error_details(&error);
        assert!(details.contains("daily_volume_exceeded"));
        assert!(details.contains("policy_id"));
    }

    #[test]
    fn noise_fires_zero_pct_never_triggers() {
        let cli = Cli {
            rpc_url: String::new(),
            operator_token: None,
            chain_id: 7,
            account_id: 1,
            owner_secret_hex: String::new(),
            session_secret_hex: String::new(),
            policy_id: 1,
            market_ids: vec![],
            threshold_bps: 10,
            size_lots: 1,
            max_slippage_bps: 50,
            poll_interval_ms: 1000,
            noise_size_lots: 1,
            noise_probability_pct: 0,
            intent_ttl_ms: 60_000,
            intent_status_timeout_ms: 15_000,
            intent_status_poll_ms: 250,
            ensure_policy: false,
            policy_expires_at: 0,
            policy_max_intents_per_second: 1,
            policy_max_intents_per_day: 1,
            policy_max_volume_per_day_micro_usdc: 1,
            bot_label: String::new(),
            loop_once: false,
        };
        for market_id in 0..10u64 {
            for tick in 0..1000u64 {
                assert!(
                    !noise_fires_this_tick(&cli, market_id, tick),
                    "0%% should never fire (m={market_id} t={tick})"
                );
            }
        }
    }

    #[test]
    fn noise_fires_hundred_pct_always_triggers() {
        let mut cli = Cli {
            rpc_url: String::new(),
            operator_token: None,
            chain_id: 7,
            account_id: 1,
            owner_secret_hex: String::new(),
            session_secret_hex: String::new(),
            policy_id: 1,
            market_ids: vec![],
            threshold_bps: 10,
            size_lots: 1,
            max_slippage_bps: 50,
            poll_interval_ms: 1000,
            noise_size_lots: 1,
            noise_probability_pct: 0,
            intent_ttl_ms: 60_000,
            intent_status_timeout_ms: 15_000,
            intent_status_poll_ms: 250,
            ensure_policy: false,
            policy_expires_at: 0,
            policy_max_intents_per_second: 1,
            policy_max_intents_per_day: 1,
            policy_max_volume_per_day_micro_usdc: 1,
            bot_label: String::new(),
            loop_once: false,
        };
        cli.noise_probability_pct = 100;
        for market_id in 0..10u64 {
            for tick in 0..1000u64 {
                assert!(
                    noise_fires_this_tick(&cli, market_id, tick),
                    "100%% should always fire (m={market_id} t={tick})"
                );
            }
        }
    }
}
