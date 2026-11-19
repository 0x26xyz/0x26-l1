use and_chain_sdk::keys::decode_hex_fixed;
use and_chain_sdk::{
    delegation_trading_account_id, encode_hex, market_maker_u256_from_u128,
    sign_market_maker_program_action, AndChainClient, DefineMarketMakerProgramParams, IntentKind,
    IntentRequest, Keypair, MarketMakerIdentity, MarketMakerIdentityParams,
    MarketMakerProgramAction, MarketMakerProgramActionRequest, OrderKind, OrderSide, OwnerAction,
    OwnerCallRequest, PolicyTermsBuilder, RpcError, SubmitError, TimeInForce,
};
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;
use tokio::time::{sleep, Instant};

#[derive(Clone, Debug, Parser)]
#[command(name = "kombat_mm_bot")]
#[command(about = "RFC027 market-maker bot with strict signed-intent nonce lanes")]
struct Cli {
    #[arg(long, env = "KOMBAT_MM_BOT_RPC_URL")]
    rpc_url: String,

    #[arg(long, env = "KOMBAT_OPERATOR_TOKEN")]
    operator_token: Option<String>,

    #[arg(long, env = "KOMBAT_MM_BOT_CHAIN_ID", default_value_t = 7)]
    chain_id: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_MAKER_ACCOUNT_ID")]
    maker_account_id: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_DELEGATION_ID_HEX")]
    delegation_id_hex: Option<String>,

    #[arg(long, env = "KOMBAT_MM_BOT_MAKER_OWNER_SECRET_HEX")]
    maker_owner_secret_hex: String,

    #[arg(long, env = "KOMBAT_MM_BOT_MAKER_SESSION_SECRET_HEX")]
    maker_session_secret_hex: String,

    #[arg(long, env = "KOMBAT_MM_BOT_MAKER_POLICY_BASE_ID")]
    maker_policy_base_id: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_MARKET_IDS", value_delimiter = ',')]
    market_ids: Vec<u64>,

    #[arg(long, env = "KOMBAT_MM_BOT_LEVELS", default_value_t = 10)]
    levels: usize,

    #[arg(long, env = "KOMBAT_MM_BOT_SPREAD_BPS", default_value_t = 5)]
    spread_bps: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_LEVEL_STEP_BPS", default_value_t = 5)]
    level_step_bps: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_LEVEL_SIZE_GROWTH_BPS",
        default_value_t = 2500
    )]
    level_size_growth_bps: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_BASE_QUANTITY_LOTS",
        default_value_t = 5_000_000
    )]
    base_quantity_lots: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_QUOTE_INTERVAL_MS",
        default_value_t = 30_000
    )]
    quote_interval_ms: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_QUOTE_CONCURRENCY", default_value_t = 20)]
    quote_concurrency: usize,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_QUOTE_REFRESH_EVERY_CYCLES",
        default_value_t = 1
    )]
    quote_refresh_every_cycles: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_QUOTE_REFRESH_PRICE_MOVE_BPS",
        default_value_t = 0
    )]
    quote_refresh_price_move_bps: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_INTENT_SUBMIT_DELAY_MS",
        default_value_t = 0
    )]
    intent_submit_delay_ms: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_INTENT_TTL_MS", default_value_t = 120_000)]
    intent_ttl_ms: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_INTENT_STATUS_TIMEOUT_MS",
        default_value_t = 30_000
    )]
    intent_status_timeout_ms: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_INTENT_STATUS_POLL_MS",
        default_value_t = 250
    )]
    intent_status_poll_ms: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_BOOTSTRAP_COLLATERAL_MICRO_USDC",
        default_value_t = 0
    )]
    bootstrap_collateral_micro_usdc: u128,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_POLICY_EXPIRES_AT",
        default_value_t = 4_102_444_800_000
    )]
    policy_expires_at: u64,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_POLICY_MAX_INTENTS_PER_SECOND",
        default_value_t = 10
    )]
    policy_max_intents_per_second: u32,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_POLICY_MAX_INTENTS_PER_DAY",
        default_value_t = 1_000_000
    )]
    policy_max_intents_per_day: u32,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_POLICY_MAX_VOLUME_PER_DAY_MICRO_USDC",
        default_value_t = 10_000_000_000_000_u128
    )]
    policy_max_volume_per_day_micro_usdc: u128,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_CREATE_LANE_POLICIES",
        default_value_t = true
    )]
    create_lane_policies: bool,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_CLEANUP_OPEN_ORDERS_ON_START",
        default_value_t = true
    )]
    cleanup_open_orders_on_start: bool,

    #[arg(long, env = "KOMBAT_MM_BOT_BOOTSTRAP_RFC027", default_value_t = true)]
    bootstrap_rfc027: bool,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_MM_PROGRAM_NAME",
        default_value = "testnet-prime-mm"
    )]
    mm_program_name: String,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_MM_MIN_QUOTE_SIZE_MICRO_USDC",
        default_value_t = 1_000_000_000_000_u128
    )]
    mm_min_quote_size_micro_usdc: u128,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_MM_ENROLLMENT_BOND_MICRO_USDC",
        default_value_t = 10_000_000_000_u128
    )]
    mm_enrollment_bond_micro_usdc: u128,

    #[arg(
        long,
        env = "KOMBAT_MM_BOT_MM_WAIT_TIMEOUT_MS",
        default_value_t = 60_000
    )]
    mm_wait_timeout_ms: u64,

    #[arg(long, env = "KOMBAT_MM_BOT_LOOP_ONCE", default_value_t = false)]
    loop_once: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuoteSlot {
    market_id: u64,
    side: OrderSide,
    level: usize,
}

#[derive(Clone, Debug)]
struct Lane {
    slot: QuoteSlot,
    policy_id: u64,
    policy_id_stride: u64,
    next_nonce: u64,
    current_client_order_id: Option<u64>,
    pending_intent: Option<PendingLaneIntent>,
    last_refresh_cycle: u64,
    last_refresh_mark_price_ticks: Option<u64>,
}

#[derive(Clone, Debug)]
struct PendingLaneIntent {
    intent_id_hex: String,
    submitted_nonce: u64,
    replacement_client_order_id: u64,
    unknown_retry_deadline_ms: u64,
}

#[derive(Debug)]
struct LaneQuoteOutcome {
    index: usize,
    policy_id: u64,
    submitted_nonce: Option<u64>,
    next_nonce: u64,
    current_client_order_id: Option<u64>,
    pending_intent: Option<PendingLaneIntent>,
    last_refresh_cycle: u64,
    last_refresh_mark_price_ticks: Option<u64>,
    next_client_order_id: Option<u64>,
    error: Option<String>,
    recovery_action: Option<LaneRecoveryAction>,
}

#[derive(Debug)]
enum LaneIntentOutcome {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneRecoveryAction {
    RotateDelegationPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DelegationLanePolicyCandidate {
    Ready(u64),
    Create(u64),
}

impl DelegationLanePolicyCandidate {
    fn policy_id(self) -> u64 {
        match self {
            Self::Ready(policy_id) | Self::Create(policy_id) => policy_id,
        }
    }
}

#[derive(Debug)]
enum PendingIntentOutcome {
    StillPending,
    Executed,
    Rejected { message: String },
}

#[derive(Clone, Copy, Debug)]
struct MarketQuoteInput {
    mark_price_ticks: u64,
    best_bid_ticks: Option<u64>,
    best_ask_ticks: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TradingIdentity {
    Account {
        account_id: u64,
    },
    Delegation {
        delegation_id: [u8; 32],
        serving_agent: u64,
        trading_account_id: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DelegationGeneratedPolicy {
    policy_id: u64,
    expires_at: u64,
}

impl TradingIdentity {
    fn signer_account_id(self) -> u64 {
        match self {
            TradingIdentity::Account { account_id } => account_id,
            TradingIdentity::Delegation { serving_agent, .. } => serving_agent,
        }
    }

    fn trading_account_id(self) -> u64 {
        match self {
            TradingIdentity::Account { account_id } => account_id,
            TradingIdentity::Delegation {
                trading_account_id, ..
            } => trading_account_id,
        }
    }

    fn market_maker_identity(self) -> MarketMakerIdentity {
        match self {
            TradingIdentity::Account { account_id } => MarketMakerIdentity::Account(account_id),
            TradingIdentity::Delegation {
                delegation_id,
                serving_agent,
                ..
            } => MarketMakerIdentity::Delegation {
                delegation_id,
                serving_agent,
            },
        }
    }

    fn market_maker_identity_params(self) -> MarketMakerIdentityParams {
        match self {
            TradingIdentity::Account { account_id } => {
                MarketMakerIdentityParams::Account { account_id }
            }
            TradingIdentity::Delegation {
                delegation_id,
                serving_agent,
                ..
            } => MarketMakerIdentityParams::Delegation {
                delegation_id_hex: encode_hex(&delegation_id),
                serving_agent,
            },
        }
    }
}

fn trading_identity(cli: &Cli) -> Result<TradingIdentity> {
    match cli.delegation_id_hex.as_deref() {
        Some(hex) => {
            let delegation_id = decode_hex_fixed::<32>(hex)
                .with_context(|| format!("invalid delegation id hex {hex}"))?;
            Ok(trading_identity_from_components(
                cli.maker_account_id,
                Some(delegation_id),
            ))
        }
        None => Ok(trading_identity_from_components(cli.maker_account_id, None)),
    }
}

fn trading_identity_from_components(
    maker_account_id: u64,
    delegation_id: Option<[u8; 32]>,
) -> TradingIdentity {
    match delegation_id {
        Some(delegation_id) => TradingIdentity::Delegation {
            delegation_id,
            serving_agent: maker_account_id,
            trading_account_id: delegation_trading_account_id(&delegation_id),
        },
        None => TradingIdentity::Account {
            account_id: maker_account_id,
        },
    }
}

fn uses_delegation_policies(trading_identity: TradingIdentity) -> bool {
    matches!(trading_identity, TradingIdentity::Delegation { .. })
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.market_ids.is_empty() {
        return Err(anyhow!("at least one market id is required"));
    }
    if cli.levels == 0 {
        return Err(anyhow!("levels must be greater than zero"));
    }
    if cli.quote_concurrency == 0 {
        return Err(anyhow!("quote concurrency must be greater than zero"));
    }
    if cli.quote_refresh_every_cycles == 0 {
        return Err(anyhow!(
            "quote refresh every cycles must be greater than zero"
        ));
    }

    let session = Keypair::from_secret_hex(&cli.maker_session_secret_hex)
        .context("invalid maker session secret")?;
    let owner = Keypair::from_secret_hex(&cli.maker_owner_secret_hex)
        .context("invalid maker owner secret")?;
    let client = build_client(&cli.rpc_url, cli.operator_token.as_deref());
    let trading_identity = trading_identity(&cli)?;

    let delegation_parent_policy = if uses_delegation_policies(trading_identity) {
        Some(resolve_delegation_generated_policy(&client, trading_identity, &session).await?)
    } else {
        None
    };
    let shared_policy_id = if delegation_parent_policy.is_some() && cli.create_lane_policies {
        None
    } else {
        delegation_parent_policy.map(|policy| policy.policy_id)
    };
    let lane_policy_base_id = delegated_lane_policy_base_id(trading_identity, &cli);
    let mut lanes = build_lanes(&cli, lane_policy_base_id, shared_policy_id);
    if let Some(parent_policy) = delegation_parent_policy.filter(|_| cli.create_lane_policies) {
        ensure_delegation_lane_policies(
            &client,
            &cli,
            trading_identity,
            &session,
            parent_policy,
            &mut lanes,
        )
        .await?;
    } else if cli.create_lane_policies && !uses_delegation_policies(trading_identity) {
        ensure_lane_policies(&client, &cli, trading_identity, &owner, &session, &lanes).await?;
    }
    if let Some(policy_id) = shared_policy_id {
        let next_nonce = client
            .next_intent_nonce(policy_id)
            .await
            .with_context(|| format!("fetch next nonce for policy {}", policy_id))?;
        for lane in &mut lanes {
            lane.next_nonce = next_nonce;
        }
    } else {
        for lane in &mut lanes {
            lane.next_nonce = client
                .next_intent_nonce(lane.policy_id)
                .await
                .with_context(|| format!("fetch next nonce for policy {}", lane.policy_id))?;
        }
    }

    if cli.bootstrap_rfc027 {
        bootstrap_rfc027(&client, &cli, trading_identity, &owner).await?;
    }
    ensure_bootstrap_collateral(&client, &cli, trading_identity, &session, &mut lanes).await?;
    if cli.cleanup_open_orders_on_start {
        reconcile_existing_orders(&client, &cli, trading_identity, &session, &mut lanes).await?;
    }

    eprintln!(
        "kombat_mm_bot: quoting markets={:?} levels={} lanes={} concurrency={} base_qty={} spread_bps={} step_bps={} signer_account={} trading_account={}",
        cli.market_ids,
        cli.levels,
        lanes.len(),
        cli.quote_concurrency,
        cli.base_quantity_lots,
        cli.spread_bps,
        cli.level_step_bps,
        trading_identity.signer_account_id(),
        trading_identity.trading_account_id(),
    );

    let mut quote_cycle_index = 0_u64;
    loop {
        quote_cycle(
            &client,
            &cli,
            trading_identity,
            delegation_parent_policy,
            quote_cycle_index,
            &session,
            &mut lanes,
        )
        .await?;
        quote_cycle_index = quote_cycle_index.saturating_add(1);
        if cli.loop_once {
            break;
        }
        sleep(Duration::from_millis(cli.quote_interval_ms)).await;
    }

    Ok(())
}

fn build_client(url: &str, token: Option<&str>) -> AndChainClient {
    let client = AndChainClient::new(url).with_timeout(Duration::from_secs(20));
    if let Some(token) = token.filter(|token| !token.is_empty()) {
        client.with_bearer_token(token)
    } else {
        client
    }
}

async fn ensure_bootstrap_collateral(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    lanes: &mut [Lane],
) -> Result<()> {
    if cli.bootstrap_collateral_micro_usdc == 0 {
        return Ok(());
    }
    let shared_policy = lanes_share_policy(lanes);
    let market_count = cli.market_ids.len() as u128;
    if market_count == 0 {
        return Err(anyhow!("bootstrap collateral requires at least one market"));
    }
    let base_target = cli.bootstrap_collateral_micro_usdc / market_count;
    let remainder = cli.bootstrap_collateral_micro_usdc % market_count;

    for (market_index, market_id) in cli.market_ids.iter().copied().enumerate() {
        let market_target = base_target.saturating_add(if (market_index as u128) < remainder {
            1
        } else {
            0
        });
        if market_target == 0 {
            continue;
        }

        let current_collateral = client
            .get_risk_snapshot(market_id, trading_identity.trading_account_id())
            .await
            .with_context(|| {
                format!(
                    "read risk snapshot before bootstrap collateral market={}",
                    market_id
                )
            })?
            .collateral
            .parse::<u128>()
            .context("parse market collateral")?;
        if current_collateral >= market_target {
            continue;
        }

        let lane_index = lanes
            .iter()
            .position(|lane| lane.slot.market_id == market_id)
            .ok_or_else(|| anyhow!("bootstrap collateral missing lane for market {}", market_id))?;
        let amount_usdc = market_target.saturating_sub(current_collateral);
        let amount_usdc_u64 = u64::try_from(amount_usdc).map_err(|_| {
            anyhow!(
                "bootstrap collateral amount {} does not fit in u64",
                amount_usdc
            )
        })?;
        let outcome = submit_lane_intent(
            client,
            cli,
            trading_identity,
            session,
            &mut lanes[lane_index],
            IntentKind::DepositCollateral {
                market_id,
                amount_usdc: amount_usdc_u64,
            },
        )
        .await
        .with_context(|| {
            format!(
                "bootstrap collateral deposit market={} amount_usdc={}",
                market_id, amount_usdc
            )
        })?;
        if !matches!(outcome, LaneIntentOutcome::Executed) {
            return Err(anyhow!(
                "bootstrap collateral deposit market={} amount_usdc={} did not execute: {}",
                market_id,
                amount_usdc,
                describe_lane_intent_outcome(&outcome)
            ));
        }
        if shared_policy {
            let next_nonce = lanes[lane_index].next_nonce;
            for other in lanes.iter_mut() {
                other.next_nonce = next_nonce;
            }
        }
        eprintln!(
            "kombat_mm_bot: bootstrap collateral deposited market={} amount_usdc={} target_usdc={} signer_account={} trading_account={}",
            market_id,
            amount_usdc,
            market_target,
            trading_identity.signer_account_id(),
            trading_identity.trading_account_id(),
        );
    }
    Ok(())
}

fn build_lanes(cli: &Cli, lane_policy_base_id: u64, shared_policy_id: Option<u64>) -> Vec<Lane> {
    let lane_count = (cli.market_ids.len() * 2 * cli.levels) as u64;
    let mut lanes = Vec::new();
    for market_id in &cli.market_ids {
        for side in [OrderSide::Bid, OrderSide::Ask] {
            for level in 0..cli.levels {
                let index = lanes.len() as u64;
                let policy_id =
                    shared_policy_id.unwrap_or_else(|| lane_policy_base_id.saturating_add(index));
                lanes.push(Lane {
                    slot: QuoteSlot {
                        market_id: *market_id,
                        side,
                        level,
                    },
                    policy_id,
                    policy_id_stride: shared_policy_id.map(|_| 0).unwrap_or(lane_count),
                    next_nonce: 1,
                    current_client_order_id: None,
                    pending_intent: None,
                    last_refresh_cycle: 0,
                    last_refresh_mark_price_ticks: None,
                });
            }
        }
    }
    lanes
}

async fn ensure_lane_policies(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    owner: &Keypair,
    session: &Keypair,
    lanes: &[Lane],
) -> Result<()> {
    let terms = PolicyTermsBuilder::new()
        .gas_budget_per_day_microusdc(10_000_000_000)
        .gas_refill_per_day_microusdc(10_000_000_000)
        .max_intents_per_second(cli.policy_max_intents_per_second)
        .max_intents_per_day(cli.policy_max_intents_per_day)
        .max_volume_per_day_microusdc(cli.policy_max_volume_per_day_micro_usdc)
        .max_drawdown_pct(10_000)
        .build();
    let mut account_nonce = client
        .get_account(trading_identity.signer_account_id())
        .await
        .context("read maker account before policy bootstrap")?
        .nonce;

    for lane in lanes {
        if client.get_policy(lane.policy_id).await.is_ok() {
            continue;
        }
        let ts_ms = now_ms();
        let request = OwnerCallRequest {
            account_id: trading_identity.signer_account_id(),
            chain_id: cli.chain_id,
            nonce: account_nonce,
            action: OwnerAction::CreatePolicy {
                policy_id: lane.policy_id,
                grantee: session.public_bytes(),
                expires_at: cli.policy_expires_at,
                terms: terms.clone(),
            },
        };
        match client.submit_typed_owner_call(owner, request, ts_ms).await {
            Ok(_) => {
                account_nonce = account_nonce.saturating_add(1);
            }
            Err(err) if rpc_reason(&err).as_deref() == Some("duplicate_policy_id") => {}
            Err(err) => {
                if client.get_policy(lane.policy_id).await.is_ok() {
                    account_nonce = account_nonce.saturating_add(1);
                    continue;
                }
                return Err(anyhow!(
                    "create lane policy {} failed: {err}",
                    lane.policy_id
                ));
            }
        }
    }
    Ok(())
}

async fn ensure_delegation_lane_policies(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    parent_policy: DelegationGeneratedPolicy,
    lanes: &mut [Lane],
) -> Result<()> {
    for lane in lanes {
        let candidate = resolve_delegation_lane_policy_candidate(
            client,
            lane,
            lane.policy_id,
            trading_identity,
            parent_policy.policy_id,
        )
        .await?;
        lane.policy_id = candidate.policy_id();
        if matches!(candidate, DelegationLanePolicyCandidate::Create(_)) {
            ensure_delegation_lane_policy(
                client,
                cli,
                trading_identity,
                session,
                parent_policy,
                lane.policy_id,
            )
            .await?;
        }
    }

    Ok(())
}

async fn bootstrap_rfc027(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    owner: &Keypair,
) -> Result<()> {
    let ts_ms = now_ms();
    let define = client
        .define_market_maker_program(DefineMarketMakerProgramParams {
            name: cli.mm_program_name.clone(),
            markets: cli.market_ids.clone(),
            min_uptime_bps: 9_000,
            max_spread_bps: 50,
            min_quote_size_micro_usdc: cli.mm_min_quote_size_micro_usdc.to_string(),
            min_two_sided_windows_per_hour: 40,
            rebate_multiplier_units: 20_000,
            enrollment_bond_micro_usdc: cli.mm_enrollment_bond_micro_usdc.to_string(),
            compliance_window_ms: 3_600_000,
            observation_cadence_ms: 60_000,
            timestamp_ms: ts_ms,
            submitter_nonce: Some(1),
            wait_timeout_ms: Some(cli.mm_wait_timeout_ms),
        })
        .await;
    let program_id_hex = match define {
        Ok(response) => response
            .program_id_hex
            .or_else(|| {
                response.action_status.and_then(|status| {
                    status.receipt.and_then(|receipt| {
                        receipt
                            .get("program_id_hex")
                            .and_then(|value| value.as_str())
                            .map(ToOwned::to_owned)
                    })
                })
            })
            .ok_or_else(|| anyhow!("define response did not include program_id_hex"))?,
        Err(err)
            if rpc_reason_from_rpc_error(&err).as_deref() == Some("program_already_active") =>
        {
            rpc_data_string(&err, "program_id_hex")
                .ok_or_else(|| anyhow!("program_already_active missing program_id_hex"))?
        }
        Err(err) => return Err(anyhow!("define market-maker program failed: {err}")),
    };
    let program_id = decode_hex_fixed::<32>(&program_id_hex).context("decode RFC027 program id")?;
    wait_for_market_maker_program(client, &program_id_hex, cli).await?;

    for market_id in &cli.market_ids {
        if market_maker_enrollment_is_live(client, trading_identity, *market_id, &program_id_hex)
            .await?
        {
            continue;
        }
        let action = MarketMakerProgramAction::Enroll {
            market_maker_identity: trading_identity.market_maker_identity(),
            market_id: *market_id,
            program_id,
            bond_micro_usdc: market_maker_u256_from_u128(cli.mm_enrollment_bond_micro_usdc),
        };
        let signed = sign_market_maker_program_action(
            owner,
            MarketMakerProgramActionRequest {
                chain_id: cli.chain_id,
                signer_account_id: trading_identity.signer_account_id(),
                action,
                timestamp_ms: now_ms(),
                submitter_nonce: Some(100 + *market_id),
            },
        )
        .context("sign RFC027 enrollment")?;
        match client
            .submit_signed_market_maker_program_action(signed, Some(cli.mm_wait_timeout_ms))
            .await
        {
            Ok(_) => {}
            Err(err)
                if rpc_reason_from_rpc_error(&err).as_deref()
                    == Some("enrollment_already_live") => {}
            Err(_err)
                if market_maker_enrollment_is_live(
                    client,
                    trading_identity,
                    *market_id,
                    &program_id_hex,
                )
                .await
                .unwrap_or(false) => {}
            Err(err) => {
                return Err(anyhow!(
                    "enroll market-maker market {} failed: {err}",
                    market_id
                ))
            }
        }
        wait_for_market_maker_enrollment(
            client,
            trading_identity,
            *market_id,
            &program_id_hex,
            cli,
        )
        .await?;
    }

    eprintln!(
        "kombat_mm_bot: RFC027 program={} enrolled identity={:?} markets={:?}",
        program_id_hex, trading_identity, cli.market_ids
    );
    Ok(())
}

async fn resolve_delegation_generated_policy(
    client: &AndChainClient,
    trading_identity: TradingIdentity,
    session: &Keypair,
) -> Result<DelegationGeneratedPolicy> {
    let TradingIdentity::Delegation { delegation_id, .. } = trading_identity else {
        return Err(anyhow!(
            "delegation generated policy resolution requires a delegation trading identity"
        ));
    };
    let delegation_id_hex = encode_hex(&delegation_id);
    let delegation = client
        .get_delegation(delegation_id_hex.clone())
        .await
        .with_context(|| format!("read delegation {}", delegation_id_hex))?
        .ok_or_else(|| anyhow!("delegation {} not found", delegation_id_hex))?;
    let mandate = client
        .get_mandate(delegation.mandate_id_hex.clone())
        .await
        .with_context(|| format!("read mandate {}", delegation.mandate_id_hex))?
        .ok_or_else(|| anyhow!("mandate {} not found", delegation.mandate_id_hex))?;
    let session_pubkey_hex = session.public_hex();
    let policy_index = mandate
        .session_keys_hex
        .iter()
        .position(|value| value.eq_ignore_ascii_case(&session_pubkey_hex))
        .ok_or_else(|| {
            anyhow!(
                "session pubkey {} is not present in mandate {} session_keys_hex",
                session_pubkey_hex,
                mandate.mandate_id_hex
            )
        })?;
    let policy_id = mandate.policies.get(policy_index).copied().ok_or_else(|| {
        anyhow!(
            "mandate {} missing generated policy for session index {}",
            mandate.mandate_id_hex,
            policy_index
        )
    })?;
    Ok(DelegationGeneratedPolicy {
        policy_id,
        expires_at: mandate.expires_at,
    })
}

async fn market_maker_enrollment_is_live(
    client: &AndChainClient,
    trading_identity: TradingIdentity,
    market_id: u64,
    program_id_hex: &str,
) -> Result<bool> {
    Ok(client
        .get_market_maker_enrollment_by_identity(
            trading_identity.market_maker_identity_params(),
            market_id,
            program_id_hex,
        )
        .await
        .with_context(|| {
            format!(
                "read market-maker enrollment identity={:?} market={} program={}",
                trading_identity, market_id, program_id_hex
            )
        })?
        .is_some())
}

async fn wait_for_market_maker_program(
    client: &AndChainClient,
    program_id_hex: &str,
    cli: &Cli,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(cli.mm_wait_timeout_ms);
    loop {
        if client
            .get_market_maker_program(program_id_hex)
            .await
            .with_context(|| format!("read market-maker program {program_id_hex}"))?
            .is_some()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "market-maker program {} was not committed before timeout",
                program_id_hex
            ));
        }
        sleep(Duration::from_millis(cli.intent_status_poll_ms)).await;
    }
}

async fn wait_for_market_maker_enrollment(
    client: &AndChainClient,
    trading_identity: TradingIdentity,
    market_id: u64,
    program_id_hex: &str,
    cli: &Cli,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_millis(cli.mm_wait_timeout_ms);
    loop {
        if market_maker_enrollment_is_live(client, trading_identity, market_id, program_id_hex)
            .await?
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "market-maker enrollment identity={:?} market={} program={} was not committed before timeout",
                trading_identity, market_id, program_id_hex
            ));
        }
        sleep(Duration::from_millis(cli.intent_status_poll_ms)).await;
    }
}

async fn reconcile_existing_orders(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    lanes: &mut [Lane],
) -> Result<()> {
    let mut stale_orders = Vec::new();
    for market_id in &cli.market_ids {
        let open = client
            .get_open_orders(*market_id, trading_identity.trading_account_id(), 1_000)
            .await
            .with_context(|| format!("read open orders for market {market_id}"))?;
        stale_orders.extend(open);
    }

    let mut orphan_orders = Vec::new();
    let mut reconciled = 0_usize;
    for order in stale_orders {
        if let Some(lane) = lanes
            .iter_mut()
            .find(|lane| order_belongs_to_lane(&order, lane))
        {
            if lane.current_client_order_id.is_none() {
                lane.current_client_order_id = Some(order.client_order_id);
                reconciled += 1;
                continue;
            }
        }
        orphan_orders.push(order);
    }

    eprintln!(
        "kombat_mm_bot: reconciled {} existing lane orders; orphan_orders={}",
        reconciled,
        orphan_orders.len()
    );

    for (index, order) in orphan_orders.into_iter().enumerate() {
        let lane_index = index % lanes.len();
        let lane = &mut lanes[lane_index];
        let cancel_result = submit_lane_intent(
            client,
            cli,
            trading_identity,
            session,
            lane,
            IntentKind::CancelOrder {
                market_id: order.market_id,
                client_order_id: order.client_order_id,
            },
        )
        .await;
        match cancel_result {
            Ok(LaneIntentOutcome::Executed) => {}
            Ok(outcome) => {
                eprintln!(
                    "kombat_mm_bot: orphan cancel did not execute market={} client_order_id={} policy={} nonce={} outcome={}",
                    order.market_id,
                    order.client_order_id,
                    lane.policy_id,
                    lane.next_nonce,
                    describe_lane_intent_outcome(&outcome)
                );
            }
            Err(err) => {
                eprintln!(
                    "kombat_mm_bot: orphan cancel failed market={} client_order_id={} policy={} nonce={} error={err:#}",
                    order.market_id, order.client_order_id, lane.policy_id, lane.next_nonce
                );
                lane.next_nonce = client
                    .next_intent_nonce(lane.policy_id)
                    .await
                    .unwrap_or(lane.next_nonce);
            }
        }
        if lanes_share_policy(lanes) {
            let next_nonce = lanes[lane_index].next_nonce;
            for other in lanes.iter_mut() {
                other.next_nonce = next_nonce;
            }
        }
    }
    Ok(())
}

fn order_belongs_to_lane(order: &and_chain_sdk::RestingOrderResponse, lane: &Lane) -> bool {
    order.market_id == lane.slot.market_id
        && order.side.as_str() == order_side_label(lane.slot.side)
        && order.client_order_id % 1_000 == lane_order_suffix(lane.slot)
}

fn lane_order_suffix(slot: QuoteSlot) -> u64 {
    let side_bit = match slot.side {
        OrderSide::Bid => 0_u64,
        OrderSide::Ask => 1_u64,
    };
    slot.market_id
        .saturating_mul(100)
        .saturating_add(side_bit.saturating_mul(50))
        .saturating_add(slot.level as u64)
        % 1_000
}

fn order_side_label(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Bid => "bid",
        OrderSide::Ask => "ask",
    }
}

async fn quote_cycle(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    delegation_parent_policy: Option<DelegationGeneratedPolicy>,
    cycle_index: u64,
    session: &Keypair,
    lanes: &mut [Lane],
) -> Result<()> {
    let shared_delegation_policy = lanes_share_policy(lanes);
    let mut quote_inputs = BTreeMap::new();
    for market_id in &cli.market_ids {
        let snapshot = match client.get_market_snapshot(*market_id, 1).await {
            Ok(snapshot) => snapshot,
            Err(err) => {
                eprintln!(
                    "kombat_mm_bot: quote cycle skipped market={} due to snapshot read error: {:#}",
                    market_id, err
                );
                continue;
            }
        };
        let mark = snapshot
            .state
            .mark_price_ticks
            .or(snapshot.state.index_price_ticks)
            .ok_or_else(|| anyhow!("market {market_id} has no mark or index price"))?;
        quote_inputs.insert(
            *market_id,
            MarketQuoteInput {
                mark_price_ticks: mark,
                best_bid_ticks: snapshot
                    .order_book
                    .bids
                    .first()
                    .map(|level| level.price_ticks),
                best_ask_ticks: snapshot
                    .order_book
                    .asks
                    .first()
                    .map(|level| level.price_ticks),
            },
        );
    }
    if quote_inputs.is_empty() {
        eprintln!("kombat_mm_bot: quote cycle skipped; no market snapshots available");
        return Ok(());
    }

    let mut tasks = JoinSet::new();
    let mut next_lane = 0_usize;
    let mut scheduled_submit_count = 0_u64;
    let concurrency = if shared_delegation_policy {
        1
    } else {
        cli.quote_concurrency.min(lanes.len()).max(1)
    };
    loop {
        while next_lane < lanes.len() && tasks.len() < concurrency {
            let index = next_lane;
            next_lane = next_lane.saturating_add(1);
            let lane = lanes[index].clone();
            let Some(quote_input) = quote_inputs.get(&lane.slot.market_id).copied() else {
                continue;
            };
            let submit_delay_ms = if lane.pending_intent.is_none()
                && lane_refresh_due(&lane, &cli, cycle_index, quote_input.mark_price_ticks)
            {
                let delay_ms = cli
                    .intent_submit_delay_ms
                    .saturating_mul(scheduled_submit_count);
                scheduled_submit_count = scheduled_submit_count.saturating_add(1);
                Some(delay_ms)
            } else {
                None
            };
            let client = client.clone();
            let cli = cli.clone();
            let session = session.clone();
            tasks.spawn(async move {
                quote_lane(
                    client,
                    cli,
                    trading_identity,
                    session,
                    cycle_index,
                    submit_delay_ms,
                    index,
                    lane,
                    quote_input,
                )
                .await
            });
        }

        if tasks.is_empty() {
            break;
        }

        let outcome = tasks
            .join_next()
            .await
            .ok_or_else(|| anyhow!("quote task set ended unexpectedly"))?
            .context("quote task failed")?;
        let (mut next_nonce, mut current_client_order_id);
        {
            let lane = lanes.get_mut(outcome.index).ok_or_else(|| {
                anyhow!("quote task returned invalid lane index {}", outcome.index)
            })?;
            lane.next_nonce = outcome.next_nonce;
            lane.current_client_order_id = outcome.current_client_order_id;
            lane.pending_intent = outcome.pending_intent;
            lane.last_refresh_cycle = outcome.last_refresh_cycle;
            lane.last_refresh_mark_price_ticks = outcome.last_refresh_mark_price_ticks;
            next_nonce = lane.next_nonce;
            current_client_order_id = lane.current_client_order_id;
        }
        if let Some(LaneRecoveryAction::RotateDelegationPolicy) = outcome.recovery_action {
            if let Some(parent_policy) = delegation_parent_policy {
                let lane = lanes.get_mut(outcome.index).ok_or_else(|| {
                    anyhow!(
                        "lane recovery returned invalid lane index {}",
                        outcome.index
                    )
                })?;
                let prior_policy_id = lane.policy_id;
                match rotate_delegation_lane_policy(
                    client,
                    cli,
                    trading_identity,
                    session,
                    parent_policy,
                    lane,
                )
                .await
                {
                    Ok(()) => {
                        next_nonce = lane.next_nonce;
                        current_client_order_id = lane.current_client_order_id;
                        eprintln!(
                            "kombat_mm_bot: rotated exhausted lane policy market={} side={:?} level={} old_policy={} new_policy={} current_order={:?}",
                            lane.slot.market_id,
                            lane.slot.side,
                            lane.slot.level,
                            prior_policy_id,
                            lane.policy_id,
                            lane.current_client_order_id
                        );
                    }
                    Err(err) => {
                        eprintln!(
                            "kombat_mm_bot: lane policy rotation failed market={} side={:?} level={} old_policy={} error={err:#}",
                            lane.slot.market_id,
                            lane.slot.side,
                            lane.slot.level,
                            prior_policy_id
                        );
                    }
                }
            }
        }
        if shared_delegation_policy {
            for other in lanes.iter_mut() {
                other.next_nonce = next_nonce;
            }
        }
        if let Some(error) = outcome.error {
            let lane = &lanes[outcome.index];
            eprintln!(
                "kombat_mm_bot: quote failed market={} side={:?} level={} policy={} nonce={} next_nonce={} current_order={:?} replacement_order={} error={}",
                lane.slot.market_id,
                lane.slot.side,
                lane.slot.level,
                outcome.policy_id,
                outcome.submitted_nonce.unwrap_or(lane.next_nonce),
                next_nonce,
                current_client_order_id,
                outcome.next_client_order_id.unwrap_or(0),
                error
            );
        }
    }
    Ok(())
}

async fn quote_lane(
    client: AndChainClient,
    cli: Cli,
    trading_identity: TradingIdentity,
    session: Keypair,
    cycle_index: u64,
    submit_delay_ms: Option<u64>,
    index: usize,
    mut lane: Lane,
    quote_input: MarketQuoteInput,
) -> LaneQuoteOutcome {
    if let Some(pending) = lane.pending_intent.clone() {
        match poll_pending_lane_intent(&client, &pending).await {
            Ok(PendingIntentOutcome::StillPending) => {
                return LaneQuoteOutcome {
                    index,
                    policy_id: lane.policy_id,
                    submitted_nonce: Some(pending.submitted_nonce),
                    next_nonce: lane.next_nonce,
                    current_client_order_id: lane.current_client_order_id,
                    pending_intent: Some(pending),
                    last_refresh_cycle: lane.last_refresh_cycle,
                    last_refresh_mark_price_ticks: lane.last_refresh_mark_price_ticks,
                    next_client_order_id: None,
                    error: None,
                    recovery_action: None,
                };
            }
            Ok(PendingIntentOutcome::Executed) => {
                lane.pending_intent = None;
                lane.next_nonce = lane
                    .next_nonce
                    .max(pending.submitted_nonce.saturating_add(1));
                if let Ok(next_nonce) = client.next_intent_nonce(lane.policy_id).await {
                    lane.next_nonce = lane.next_nonce.max(next_nonce);
                }
                if let Ok(current) =
                    refresh_lane_current_order(&client, trading_identity, &lane).await
                {
                    lane.current_client_order_id =
                        current.or(Some(pending.replacement_client_order_id));
                } else {
                    lane.current_client_order_id = Some(pending.replacement_client_order_id);
                }
                return LaneQuoteOutcome {
                    index,
                    policy_id: lane.policy_id,
                    submitted_nonce: Some(pending.submitted_nonce),
                    next_nonce: lane.next_nonce,
                    current_client_order_id: lane.current_client_order_id,
                    pending_intent: None,
                    last_refresh_cycle: lane.last_refresh_cycle,
                    last_refresh_mark_price_ticks: lane.last_refresh_mark_price_ticks,
                    next_client_order_id: Some(pending.replacement_client_order_id),
                    error: None,
                    recovery_action: None,
                };
            }
            Ok(PendingIntentOutcome::Rejected { message }) => {
                lane.pending_intent = None;
                if let Ok(next_nonce) = client.next_intent_nonce(lane.policy_id).await {
                    lane.next_nonce = next_nonce;
                }
                if let Ok(current) =
                    refresh_lane_current_order(&client, trading_identity, &lane).await
                {
                    lane.current_client_order_id = current;
                }
                return LaneQuoteOutcome {
                    index,
                    policy_id: lane.policy_id,
                    submitted_nonce: Some(pending.submitted_nonce),
                    next_nonce: lane.next_nonce,
                    current_client_order_id: lane.current_client_order_id,
                    pending_intent: None,
                    last_refresh_cycle: lane.last_refresh_cycle,
                    last_refresh_mark_price_ticks: lane.last_refresh_mark_price_ticks,
                    next_client_order_id: Some(pending.replacement_client_order_id),
                    error: Some(format!(
                        "pending intent {} terminal: {}",
                        pending.intent_id_hex, message
                    )),
                    recovery_action: lane_recovery_action(&lane, trading_identity, &message),
                };
            }
            Err(err) => {
                return LaneQuoteOutcome {
                    index,
                    policy_id: lane.policy_id,
                    submitted_nonce: Some(pending.submitted_nonce),
                    next_nonce: lane.next_nonce,
                    current_client_order_id: lane.current_client_order_id,
                    pending_intent: Some(pending),
                    last_refresh_cycle: lane.last_refresh_cycle,
                    last_refresh_mark_price_ticks: lane.last_refresh_mark_price_ticks,
                    next_client_order_id: None,
                    error: Some(format!("pending intent status check failed: {err:#}")),
                    recovery_action: None,
                };
            }
        }
    }

    if !lane_refresh_due(&lane, &cli, cycle_index, quote_input.mark_price_ticks) {
        return LaneQuoteOutcome {
            index,
            policy_id: lane.policy_id,
            submitted_nonce: None,
            next_nonce: lane.next_nonce,
            current_client_order_id: lane.current_client_order_id,
            pending_intent: lane.pending_intent,
            last_refresh_cycle: lane.last_refresh_cycle,
            last_refresh_mark_price_ticks: lane.last_refresh_mark_price_ticks,
            next_client_order_id: None,
            error: None,
            recovery_action: None,
        };
    }
    if let Some(delay_ms) = submit_delay_ms.filter(|delay_ms| *delay_ms > 0) {
        sleep(Duration::from_millis(delay_ms)).await;
    }

    let submitted_nonce = lane.next_nonce;
    let attempted_replace = lane.current_client_order_id;
    let raw_price_ticks = quote_price_ticks(
        quote_input.mark_price_ticks,
        lane.slot.side,
        cli.spread_bps,
        cli.level_step_bps,
        lane.slot.level,
    );
    let price_ticks = post_only_safe_price_ticks(
        raw_price_ticks,
        lane.slot.side,
        quote_input.best_bid_ticks,
        quote_input.best_ask_ticks,
        cli.level_step_bps,
        lane.slot.level,
    );
    let quantity_lots = quote_quantity_lots(
        cli.base_quantity_lots,
        cli.level_size_growth_bps,
        lane.slot.level,
    );
    let replacement_client_order_id = next_client_order_id(&lane);
    let kind = if let Some(current_client_order_id) = lane.current_client_order_id {
        IntentKind::ReplaceOrder {
            market_id: lane.slot.market_id,
            target_client_order_id: current_client_order_id,
            replacement_client_order_id,
            side: lane.slot.side,
            quantity_lots,
            kind: OrderKind::Limit { price_ticks },
            time_in_force: TimeInForce::Gtc,
            post_only: true,
        }
    } else {
        IntentKind::PlaceOrder {
            market_id: lane.slot.market_id,
            client_order_id: replacement_client_order_id,
            side: lane.slot.side,
            quantity_lots,
            kind: OrderKind::Limit { price_ticks },
            time_in_force: TimeInForce::Gtc,
            post_only: true,
        }
    };

    let result =
        submit_lane_intent(&client, &cli, trading_identity, &session, &mut lane, kind).await;
    lane.last_refresh_cycle = cycle_index;
    lane.last_refresh_mark_price_ticks = Some(quote_input.mark_price_ticks);
    let mut error = None;
    let mut recovery_action = None;
    match result {
        Ok(LaneIntentOutcome::Executed) => {
            lane.current_client_order_id = Some(replacement_client_order_id);
        }
        Ok(LaneIntentOutcome::TimedOut {
            intent_id_hex,
            last_stage,
            pending_reason,
        }) => {
            lane.pending_intent = Some(PendingLaneIntent {
                intent_id_hex: intent_id_hex.clone(),
                submitted_nonce,
                replacement_client_order_id,
                unknown_retry_deadline_ms: pending_unknown_retry_deadline_ms(
                    &last_stage,
                    cli.intent_status_timeout_ms,
                ),
            });
            if let Ok(current) = refresh_lane_current_order(&client, trading_identity, &lane).await
            {
                lane.current_client_order_id = current;
            }
            error = Some(format!(
                "intent {} still pending after timeout; last_stage={} pending_reason={:?}; lane will not submit a successor until terminal",
                intent_id_hex, last_stage, pending_reason
            ));
        }
        Ok(LaneIntentOutcome::Rejected { message }) => {
            let mut reconciled_on_book = false;
            if let Ok(current) = refresh_lane_current_order(&client, trading_identity, &lane).await
            {
                lane.current_client_order_id = current;
                reconciled_on_book = current == Some(replacement_client_order_id);
            }
            if let Ok(next_nonce) = client.next_intent_nonce(lane.policy_id).await {
                lane.next_nonce = next_nonce;
            }
            if reconciled_on_book {
                lane.next_nonce = lane.next_nonce.max(submitted_nonce.saturating_add(1));
            } else if attempted_replace.is_some()
                && lane.current_client_order_id.is_none()
                && is_unknown_resting_order_error(&message)
            {
                let retry_client_order_id = next_client_order_id(&lane);
                let retry_submitted_nonce = lane.next_nonce;
                let retry_kind = IntentKind::PlaceOrder {
                    market_id: lane.slot.market_id,
                    client_order_id: retry_client_order_id,
                    side: lane.slot.side,
                    quantity_lots,
                    kind: OrderKind::Limit { price_ticks },
                    time_in_force: TimeInForce::Gtc,
                    post_only: true,
                };
                match submit_lane_intent(
                    &client,
                    &cli,
                    trading_identity,
                    &session,
                    &mut lane,
                    retry_kind,
                )
                .await
                {
                    Ok(LaneIntentOutcome::Executed) => {
                        lane.current_client_order_id = Some(retry_client_order_id);
                    }
                    Ok(LaneIntentOutcome::TimedOut {
                        intent_id_hex,
                        last_stage,
                        pending_reason,
                    }) => {
                        lane.pending_intent = Some(PendingLaneIntent {
                            intent_id_hex: intent_id_hex.clone(),
                            submitted_nonce: retry_submitted_nonce,
                            replacement_client_order_id: retry_client_order_id,
                            unknown_retry_deadline_ms: pending_unknown_retry_deadline_ms(
                                &last_stage,
                                cli.intent_status_timeout_ms,
                            ),
                        });
                        error = Some(format!(
                            "{message}; retry_after_unknown_resting_order still pending: intent {} last_stage={} pending_reason={:?}",
                            intent_id_hex, last_stage, pending_reason
                        ));
                    }
                    Ok(LaneIntentOutcome::Rejected {
                        message: retry_message,
                    }) => {
                        error = Some(format!(
                            "{message}; retry_after_unknown_resting_order_rejected: {retry_message}"
                        ));
                    }
                    Err(retry_err) => {
                        error = Some(format!(
                            "{message}; retry_after_unknown_resting_order_failed: {retry_err:#}"
                        ));
                    }
                }
            } else {
                error = Some(message);
            }
            if let Some(message) = error.as_deref() {
                recovery_action = lane_recovery_action(&lane, trading_identity, message);
            }
        }
        Err(err) => {
            if let Ok(current) = refresh_lane_current_order(&client, trading_identity, &lane).await
            {
                lane.current_client_order_id = current;
            }
            if let Ok(next_nonce) = client.next_intent_nonce(lane.policy_id).await {
                lane.next_nonce = next_nonce;
            }
            error = Some(format!("{err:#}"));
            if let Some(message) = error.as_deref() {
                recovery_action = lane_recovery_action(&lane, trading_identity, message);
            }
        }
    }

    LaneQuoteOutcome {
        index,
        policy_id: lane.policy_id,
        submitted_nonce: Some(submitted_nonce),
        next_nonce: lane.next_nonce,
        current_client_order_id: lane.current_client_order_id,
        pending_intent: lane.pending_intent,
        last_refresh_cycle: lane.last_refresh_cycle,
        last_refresh_mark_price_ticks: lane.last_refresh_mark_price_ticks,
        next_client_order_id: Some(replacement_client_order_id),
        error,
        recovery_action,
    }
}

async fn refresh_lane_current_order(
    client: &AndChainClient,
    trading_identity: TradingIdentity,
    lane: &Lane,
) -> Result<Option<u64>> {
    let open = client
        .get_open_orders(
            lane.slot.market_id,
            trading_identity.trading_account_id(),
            1_000,
        )
        .await
        .with_context(|| format!("refresh open orders for market {}", lane.slot.market_id))?;
    Ok(open
        .into_iter()
        .filter(|order| order_belongs_to_lane(order, lane))
        .map(|order| order.client_order_id)
        .max())
}

async fn submit_lane_intent(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    lane: &mut Lane,
    kind: IntentKind,
) -> Result<LaneIntentOutcome> {
    let ts_ms = now_ms();
    let submitted_nonce = lane.next_nonce;
    let request = IntentRequest {
        chain_id: cli.chain_id,
        account_id: trading_identity.trading_account_id(),
        policy_id: lane.policy_id,
        nonce: submitted_nonce,
        expires_at: ts_ms.saturating_add(cli.intent_ttl_ms),
        predicate: None,
        kind,
    };
    let response = match client.submit_typed_intent(session, request, ts_ms).await {
        Ok(response) => response,
        Err(err) => {
            if let Some(intent_id_hex) = ambiguous_submit_intent_id(&err) {
                return Ok(LaneIntentOutcome::TimedOut {
                    intent_id_hex,
                    last_stage: "submit_admission".to_owned(),
                    pending_reason: rpc_reason(&err),
                });
            }
            let details = submit_error_details(&err);
            let message = format!(
                "submit intent policy={} nonce={} failed: {}",
                lane.policy_id, submitted_nonce, details
            );
            if is_unknown_resting_order_error(&details) {
                return Ok(LaneIntentOutcome::Rejected { message });
            }
            return Err(anyhow!(message));
        }
    };
    let outcome = wait_for_intent_terminal(client, &response.intent_id_hex, cli).await?;
    if matches!(outcome, LaneIntentOutcome::Executed) {
        lane.next_nonce = lane.next_nonce.max(submitted_nonce.saturating_add(1));
    }
    Ok(outcome)
}

async fn wait_for_intent_terminal(
    client: &AndChainClient,
    intent_id_hex: &str,
    cli: &Cli,
) -> Result<LaneIntentOutcome> {
    let deadline = Instant::now() + Duration::from_millis(cli.intent_status_timeout_ms);
    loop {
        let status = client.get_intent_status(intent_id_hex).await?;
        match status.stage.as_str() {
            "executed" => return Ok(LaneIntentOutcome::Executed),
            "rejected" | "expired" => {
                return Ok(LaneIntentOutcome::Rejected {
                    message: format!(
                        "intent {} terminal stage={} rejection={:?} order_rejection={:?}",
                        intent_id_hex,
                        status.stage,
                        status.rejection_reason,
                        status.order_rejection_reason
                    ),
                });
            }
            _ if Instant::now() >= deadline => {
                return Ok(LaneIntentOutcome::TimedOut {
                    intent_id_hex: intent_id_hex.to_owned(),
                    last_stage: status.stage,
                    pending_reason: status.pending_reason,
                });
            }
            _ => sleep(Duration::from_millis(cli.intent_status_poll_ms)).await,
        }
    }
}

async fn poll_pending_lane_intent(
    client: &AndChainClient,
    pending: &PendingLaneIntent,
) -> Result<PendingIntentOutcome> {
    let status = client.get_intent_status(&pending.intent_id_hex).await?;
    match status.stage.as_str() {
        "executed" => Ok(PendingIntentOutcome::Executed),
        "rejected" | "expired" => Ok(PendingIntentOutcome::Rejected {
            message: format!(
                "stage={} rejection={:?} order_rejection={:?}",
                status.stage, status.rejection_reason, status.order_rejection_reason
            ),
        }),
        "unknown" if pending_intent_unknown_expired(pending) => {
            Ok(PendingIntentOutcome::Rejected {
                message: format!(
                    "stage=unknown after ambiguous submit intent {} exceeded retry deadline",
                    pending.intent_id_hex
                ),
            })
        }
        _ => Ok(PendingIntentOutcome::StillPending),
    }
}

fn describe_lane_intent_outcome(outcome: &LaneIntentOutcome) -> String {
    match outcome {
        LaneIntentOutcome::Executed => "executed".to_owned(),
        LaneIntentOutcome::Rejected { message } => message.clone(),
        LaneIntentOutcome::TimedOut {
            intent_id_hex,
            last_stage,
            pending_reason,
        } => format!(
            "intent {} timed out last_stage={} pending_reason={:?}",
            intent_id_hex, last_stage, pending_reason
        ),
    }
}

fn quote_price_ticks(
    mark: u64,
    side: OrderSide,
    spread_bps: u64,
    level_step_bps: u64,
    level: usize,
) -> u64 {
    let offset_bps = spread_bps.saturating_add(level_step_bps.saturating_mul(level as u64));
    let mark = u128::from(mark);
    let bps = match side {
        OrderSide::Bid => 10_000_u128.saturating_sub(u128::from(offset_bps)),
        OrderSide::Ask => 10_000_u128.saturating_add(u128::from(offset_bps)),
    };
    ((mark.saturating_mul(bps)) / 10_000).max(1) as u64
}

fn post_only_safe_price_ticks(
    price_ticks: u64,
    side: OrderSide,
    best_bid_ticks: Option<u64>,
    best_ask_ticks: Option<u64>,
    level_step_bps: u64,
    level: usize,
) -> u64 {
    match side {
        OrderSide::Bid => best_ask_ticks
            .map(|best_ask| {
                let inside_safe = best_ask.saturating_sub(1).max(1);
                let level_offset = post_only_ladder_offset_ticks(best_ask, level_step_bps, level);
                price_ticks.min(inside_safe.saturating_sub(level_offset).max(1))
            })
            .unwrap_or(price_ticks)
            .max(1),
        OrderSide::Ask => best_bid_ticks
            .map(|best_bid| {
                let inside_safe = best_bid.saturating_add(1);
                let level_offset = post_only_ladder_offset_ticks(best_bid, level_step_bps, level);
                price_ticks.max(inside_safe.saturating_add(level_offset))
            })
            .unwrap_or(price_ticks)
            .max(1),
    }
}

fn post_only_ladder_offset_ticks(reference_ticks: u64, level_step_bps: u64, level: usize) -> u64 {
    if level == 0 || level_step_bps == 0 {
        return 0;
    }
    ((u128::from(reference_ticks)
        .saturating_mul(u128::from(level_step_bps))
        .saturating_mul(level as u128)
        .saturating_add(9_999))
        / 10_000)
        .max(level as u128)
        .min(u128::from(u64::MAX)) as u64
}

fn quote_quantity_lots(base: u64, growth_bps: u64, level: usize) -> u64 {
    let multiplier =
        10_000_u128.saturating_add(u128::from(growth_bps).saturating_mul(level as u128));
    ((u128::from(base).saturating_mul(multiplier)) / 10_000)
        .max(1)
        .min(u128::from(u64::MAX)) as u64
}

fn lane_refresh_due(lane: &Lane, cli: &Cli, cycle_index: u64, mark_price_ticks: u64) -> bool {
    if lane.current_client_order_id.is_none() {
        return true;
    }
    if cli.quote_refresh_price_move_bps > 0
        && lane.last_refresh_mark_price_ticks.is_some_and(|last_mark| {
            price_move_bps(last_mark, mark_price_ticks) >= cli.quote_refresh_price_move_bps
        })
    {
        return true;
    }
    if cli.quote_refresh_every_cycles <= 1 {
        return true;
    }
    cycle_index % cli.quote_refresh_every_cycles
        == lane_refresh_phase(lane, cli.quote_refresh_every_cycles)
}

fn lane_refresh_phase(lane: &Lane, refresh_every_cycles: u64) -> u64 {
    if refresh_every_cycles <= 1 {
        return 0;
    }
    (lane
        .slot
        .market_id
        .saturating_mul(31)
        .saturating_add(match lane.slot.side {
            OrderSide::Bid => 0,
            OrderSide::Ask => 1,
        })
        .saturating_add(lane.slot.level as u64))
        % refresh_every_cycles
}

fn price_move_bps(previous_ticks: u64, current_ticks: u64) -> u64 {
    let base = u128::from(previous_ticks.max(1));
    let delta = u128::from(previous_ticks.abs_diff(current_ticks));
    ((delta.saturating_mul(10_000)).saturating_add(base.saturating_sub(1)) / base)
        .min(u128::from(u64::MAX)) as u64
}

fn next_client_order_id(lane: &Lane) -> u64 {
    let side_bit = match lane.slot.side {
        OrderSide::Bid => 0_u64,
        OrderSide::Ask => 1_u64,
    };
    now_ms()
        .saturating_mul(1_000)
        .saturating_add(lane.slot.market_id.saturating_mul(100))
        .saturating_add(side_bit.saturating_mul(50))
        .saturating_add(lane.slot.level as u64)
}

fn lanes_share_policy(lanes: &[Lane]) -> bool {
    lanes
        .first()
        .is_some_and(|first| lanes.iter().all(|lane| lane.policy_id == first.policy_id))
}

fn delegated_lane_policy_base_id(trading_identity: TradingIdentity, cli: &Cli) -> u64 {
    match trading_identity {
        TradingIdentity::Delegation {
            trading_account_id, ..
        } if cli.create_lane_policies => trading_account_id.saturating_add(10_000),
        _ => cli.maker_policy_base_id,
    }
}

fn delegation_lane_policy_matches(
    policy: &and_chain_sdk::PolicyResponse,
    expected_execution_account_id: u64,
    expected_parent_policy_id: u64,
) -> bool {
    policy.execution_account_id == expected_execution_account_id
        && policy.execution_scope.kind == "delegation_trading"
        && policy.parent_policy_id == Some(expected_parent_policy_id)
}

async fn delegation_lane_policy_ready(
    client: &AndChainClient,
    policy_id: u64,
    trading_identity: TradingIdentity,
    parent_policy_id: u64,
) -> Result<bool> {
    let Ok(policy) = client.get_policy(policy_id).await else {
        return Ok(false);
    };
    if delegation_lane_policy_matches(
        &policy,
        trading_identity.trading_account_id(),
        parent_policy_id,
    ) {
        return Ok(policy_state_is_active(&policy));
    }
    Err(anyhow!(
        "delegation lane policy id {} already exists for account {} parent {:?}, expected account {} parent {}",
        policy_id,
        policy.account,
        policy.parent_policy_id,
        trading_identity.trading_account_id(),
        parent_policy_id
    ))
}

async fn wait_for_delegation_lane_policy_ready(
    client: &AndChainClient,
    cli: &Cli,
    policy_id: u64,
    trading_identity: TradingIdentity,
    parent_policy_id: u64,
) -> Result<bool> {
    for _ in 0..10 {
        if delegation_lane_policy_ready(client, policy_id, trading_identity, parent_policy_id)
            .await?
        {
            return Ok(true);
        }
        sleep(Duration::from_millis(cli.intent_status_poll_ms)).await;
    }
    Ok(false)
}

async fn refresh_parent_policy_nonce(
    client: &AndChainClient,
    policy_id: u64,
    fallback: u64,
) -> u64 {
    // If the previous intent timed out while still pending, the chain frontier
    // can remain below `fallback`. Trust the live policy nonce in that case so
    // the bot resubmits/observes the missing nonce instead of creating a gap.
    client
        .next_intent_nonce(policy_id)
        .await
        .unwrap_or(fallback)
}

fn policy_state_is_active(policy: &and_chain_sdk::PolicyResponse) -> bool {
    policy.state == "active"
}

fn is_daily_volume_exceeded_error(error: &str) -> bool {
    error.contains("daily_volume_exceeded")
}

fn lane_recovery_action(
    lane: &Lane,
    trading_identity: TradingIdentity,
    error: &str,
) -> Option<LaneRecoveryAction> {
    if lane.policy_id_stride == 0 || !uses_delegation_policies(trading_identity) {
        return None;
    }
    is_daily_volume_exceeded_error(error).then_some(LaneRecoveryAction::RotateDelegationPolicy)
}

fn next_rotated_lane_policy_id(current_policy_id: u64, policy_id_stride: u64) -> Result<u64> {
    if policy_id_stride == 0 {
        return Err(anyhow!(
            "lane policy rotation requires a non-zero policy stride"
        ));
    }
    current_policy_id
        .checked_add(policy_id_stride)
        .ok_or_else(|| anyhow!("lane policy id overflow for {current_policy_id}"))
}

async fn resolve_delegation_lane_policy_candidate(
    client: &AndChainClient,
    lane: &Lane,
    start_policy_id: u64,
    trading_identity: TradingIdentity,
    parent_policy_id: u64,
) -> Result<DelegationLanePolicyCandidate> {
    let mut candidate_policy_id = start_policy_id;
    loop {
        let Ok(policy) = client.get_policy(candidate_policy_id).await else {
            return Ok(DelegationLanePolicyCandidate::Create(candidate_policy_id));
        };
        if delegation_lane_policy_matches(
            &policy,
            trading_identity.trading_account_id(),
            parent_policy_id,
        ) && policy_state_is_active(&policy)
        {
            return Ok(DelegationLanePolicyCandidate::Ready(candidate_policy_id));
        }
        candidate_policy_id =
            next_rotated_lane_policy_id(candidate_policy_id, lane.policy_id_stride)?;
    }
}

async fn ensure_delegation_lane_policy(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    parent_policy: DelegationGeneratedPolicy,
    child_policy_id: u64,
) -> Result<()> {
    let terms = PolicyTermsBuilder::new()
        .gas_budget_per_day_microusdc(10_000_000_000)
        .gas_refill_per_day_microusdc(10_000_000_000)
        .max_intents_per_second(cli.policy_max_intents_per_second)
        .max_intents_per_day(cli.policy_max_intents_per_day)
        .max_volume_per_day_microusdc(cli.policy_max_volume_per_day_micro_usdc)
        .max_drawdown_pct(10_000)
        .build();
    let mut parent_nonce = client
        .next_intent_nonce(parent_policy.policy_id)
        .await
        .with_context(|| {
            format!(
                "fetch next nonce for parent policy {}",
                parent_policy.policy_id
            )
        })?;
    let child_expires_at = cli.policy_expires_at.min(parent_policy.expires_at);

    for _ in 0..8 {
        if wait_for_delegation_lane_policy_ready(
            client,
            cli,
            child_policy_id,
            trading_identity,
            parent_policy.policy_id,
        )
        .await?
        {
            return Ok(());
        }

        let submitted_nonce = parent_nonce;
        let ts_ms = now_ms();
        let request = IntentRequest {
            chain_id: cli.chain_id,
            account_id: trading_identity.trading_account_id(),
            policy_id: parent_policy.policy_id,
            nonce: submitted_nonce,
            expires_at: ts_ms.saturating_add(cli.intent_ttl_ms),
            predicate: None,
            kind: IntentKind::CreateSubPolicy {
                child_policy_id,
                child_grantee: session.public_bytes(),
                child_expires_at,
                child_terms: terms.clone(),
            },
        };
        match client.submit_typed_intent(session, request, ts_ms).await {
            Ok(response) => {
                let outcome = wait_for_intent_terminal(client, &response.intent_id_hex, cli)
                    .await
                    .with_context(|| {
                        format!(
                            "wait for delegation lane policy {} intent {}",
                            child_policy_id, response.intent_id_hex
                        )
                    })?;
                if wait_for_delegation_lane_policy_ready(
                    client,
                    cli,
                    child_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                match outcome {
                    LaneIntentOutcome::Executed => {
                        return Err(anyhow!(
                            "delegation lane policy {} intent {} executed but policy was not visible",
                            child_policy_id,
                            response.intent_id_hex
                        ));
                    }
                    LaneIntentOutcome::Rejected { message } => {
                        return Err(anyhow!(
                            "delegation lane policy {} intent rejected: {}",
                            child_policy_id,
                            message
                        ));
                    }
                    LaneIntentOutcome::TimedOut { .. } => {
                        parent_nonce = refresh_parent_policy_nonce(
                            client,
                            parent_policy.policy_id,
                            submitted_nonce.saturating_add(1),
                        )
                        .await;
                    }
                }
            }
            Err(err) if rpc_reason(&err).as_deref() == Some("duplicate_policy_id") => {
                if wait_for_delegation_lane_policy_ready(
                    client,
                    cli,
                    child_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                return Err(anyhow!(
                    "create delegation lane policy {} failed despite duplicate policy response: {}",
                    child_policy_id,
                    submit_error_details(&err)
                ));
            }
            Err(err) if rpc_reason(&err).as_deref() == Some("duplicate_policy_nonce") => {
                if let Some(intent_id_hex) = submit_error_data_string(&err, "intent_id_hex") {
                    let outcome = wait_for_intent_terminal(client, &intent_id_hex, cli).await?;
                    eprintln!(
                        "kombat_mm_bot: delegation lane policy {} saw duplicate parent nonce {}; existing intent outcome={}",
                        child_policy_id,
                        submitted_nonce,
                        describe_lane_intent_outcome(&outcome)
                    );
                }
                if wait_for_delegation_lane_policy_ready(
                    client,
                    cli,
                    child_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                parent_nonce = refresh_parent_policy_nonce(
                    client,
                    parent_policy.policy_id,
                    submitted_nonce.saturating_add(1),
                )
                .await;
            }
            Err(err) if ambiguous_submit_intent_id(&err).is_some() => {
                let intent_id_hex = ambiguous_submit_intent_id(&err).expect("checked above");
                let outcome = wait_for_intent_terminal(client, &intent_id_hex, cli).await?;
                if wait_for_delegation_lane_policy_ready(
                    client,
                    cli,
                    child_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                eprintln!(
                    "kombat_mm_bot: delegation lane policy {} saw ambiguous parent submit nonce {}; intent outcome={}",
                    child_policy_id,
                    submitted_nonce,
                    describe_lane_intent_outcome(&outcome)
                );
                parent_nonce = refresh_parent_policy_nonce(
                    client,
                    parent_policy.policy_id,
                    submitted_nonce.saturating_add(1),
                )
                .await;
            }
            Err(err) => {
                if wait_for_delegation_lane_policy_ready(
                    client,
                    cli,
                    child_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                return Err(anyhow!(
                    "create delegation lane policy {} via parent {} failed: {}",
                    child_policy_id,
                    parent_policy.policy_id,
                    submit_error_details(&err)
                ));
            }
        }
    }

    Err(anyhow!(
        "create delegation lane policy {} via parent {} exhausted retries",
        child_policy_id,
        parent_policy.policy_id
    ))
}

async fn wait_for_delegation_lane_policy_not_active(
    client: &AndChainClient,
    cli: &Cli,
    policy_id: u64,
    trading_identity: TradingIdentity,
    parent_policy_id: u64,
) -> Result<bool> {
    for _ in 0..10 {
        match client.get_policy(policy_id).await {
            Ok(policy) => {
                if !delegation_lane_policy_matches(
                    &policy,
                    trading_identity.trading_account_id(),
                    parent_policy_id,
                ) {
                    return Err(anyhow!(
                        "delegation lane policy id {} changed to account {} parent {:?}, expected account {} parent {}",
                        policy_id,
                        policy.account,
                        policy.parent_policy_id,
                        trading_identity.trading_account_id(),
                        parent_policy_id
                    ));
                }
                if !policy_state_is_active(&policy) {
                    return Ok(true);
                }
            }
            Err(_) => return Ok(true),
        }
        sleep(Duration::from_millis(cli.intent_status_poll_ms)).await;
    }
    Ok(false)
}

async fn revoke_delegation_lane_policy(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    parent_policy: DelegationGeneratedPolicy,
    target_policy_id: u64,
) -> Result<()> {
    let Ok(policy) = client.get_policy(target_policy_id).await else {
        return Ok(());
    };
    if !delegation_lane_policy_matches(
        &policy,
        trading_identity.trading_account_id(),
        parent_policy.policy_id,
    ) {
        return Err(anyhow!(
            "delegation lane policy id {} already exists for account {} parent {:?}, expected account {} parent {}",
            target_policy_id,
            policy.account,
            policy.parent_policy_id,
            trading_identity.trading_account_id(),
            parent_policy.policy_id
        ));
    }
    if !policy_state_is_active(&policy) {
        return Ok(());
    }

    let mut parent_nonce = client
        .next_intent_nonce(parent_policy.policy_id)
        .await
        .with_context(|| {
            format!(
                "fetch next nonce for parent policy {} before revoke",
                parent_policy.policy_id
            )
        })?;
    for _ in 0..8 {
        let submitted_nonce = parent_nonce;
        let ts_ms = now_ms();
        let request = IntentRequest {
            chain_id: cli.chain_id,
            account_id: trading_identity.trading_account_id(),
            policy_id: parent_policy.policy_id,
            nonce: submitted_nonce,
            expires_at: ts_ms.saturating_add(cli.intent_ttl_ms),
            predicate: None,
            kind: IntentKind::RevokePolicy { target_policy_id },
        };

        match client.submit_typed_intent(session, request, ts_ms).await {
            Ok(response) => {
                let outcome = wait_for_intent_terminal(client, &response.intent_id_hex, cli)
                    .await
                    .with_context(|| {
                        format!(
                            "wait for delegation lane revoke {} intent {}",
                            target_policy_id, response.intent_id_hex
                        )
                    })?;
                if wait_for_delegation_lane_policy_not_active(
                    client,
                    cli,
                    target_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                match outcome {
                    LaneIntentOutcome::Executed => {
                        return Err(anyhow!(
                            "delegation lane revoke {} executed but policy remained active",
                            target_policy_id
                        ))
                    }
                    LaneIntentOutcome::Rejected { message } => {
                        return Err(anyhow!(
                            "delegation lane revoke {} rejected: {}",
                            target_policy_id,
                            message
                        ))
                    }
                    LaneIntentOutcome::TimedOut { .. } => {
                        parent_nonce = refresh_parent_policy_nonce(
                            client,
                            parent_policy.policy_id,
                            submitted_nonce.saturating_add(1),
                        )
                        .await;
                    }
                }
            }
            Err(err) if rpc_reason(&err).as_deref() == Some("duplicate_policy_nonce") => {
                if let Some(intent_id_hex) = submit_error_data_string(&err, "intent_id_hex") {
                    let outcome = wait_for_intent_terminal(client, &intent_id_hex, cli).await?;
                    eprintln!(
                        "kombat_mm_bot: delegation lane revoke {} saw duplicate parent nonce {}; existing intent outcome={}",
                        target_policy_id,
                        submitted_nonce,
                        describe_lane_intent_outcome(&outcome)
                    );
                }
                if wait_for_delegation_lane_policy_not_active(
                    client,
                    cli,
                    target_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                parent_nonce = refresh_parent_policy_nonce(
                    client,
                    parent_policy.policy_id,
                    submitted_nonce.saturating_add(1),
                )
                .await;
            }
            Err(err) if ambiguous_submit_intent_id(&err).is_some() => {
                let intent_id_hex = ambiguous_submit_intent_id(&err).expect("checked above");
                let outcome = wait_for_intent_terminal(client, &intent_id_hex, cli).await?;
                if wait_for_delegation_lane_policy_not_active(
                    client,
                    cli,
                    target_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                eprintln!(
                    "kombat_mm_bot: delegation lane revoke {} saw ambiguous parent submit nonce {}; intent outcome={}",
                    target_policy_id,
                    submitted_nonce,
                    describe_lane_intent_outcome(&outcome)
                );
                parent_nonce = refresh_parent_policy_nonce(
                    client,
                    parent_policy.policy_id,
                    submitted_nonce.saturating_add(1),
                )
                .await;
            }
            Err(err) => {
                if wait_for_delegation_lane_policy_not_active(
                    client,
                    cli,
                    target_policy_id,
                    trading_identity,
                    parent_policy.policy_id,
                )
                .await?
                {
                    return Ok(());
                }
                return Err(anyhow!(
                    "revoke delegation lane policy {} via parent {} failed: {}",
                    target_policy_id,
                    parent_policy.policy_id,
                    submit_error_details(&err)
                ));
            }
        }
    }

    Err(anyhow!(
        "revoke delegation lane policy {} via parent {} exhausted retries",
        target_policy_id,
        parent_policy.policy_id
    ))
}

async fn rotate_delegation_lane_policy(
    client: &AndChainClient,
    cli: &Cli,
    trading_identity: TradingIdentity,
    session: &Keypair,
    parent_policy: DelegationGeneratedPolicy,
    lane: &mut Lane,
) -> Result<()> {
    if let Ok(current) = refresh_lane_current_order(client, trading_identity, lane).await {
        lane.current_client_order_id = current;
    }
    let exhausted_policy_id = lane.policy_id;
    revoke_delegation_lane_policy(
        client,
        cli,
        trading_identity,
        session,
        parent_policy,
        exhausted_policy_id,
    )
    .await?;

    let next_policy_id = next_rotated_lane_policy_id(exhausted_policy_id, lane.policy_id_stride)?;
    let candidate = resolve_delegation_lane_policy_candidate(
        client,
        lane,
        next_policy_id,
        trading_identity,
        parent_policy.policy_id,
    )
    .await?;
    lane.policy_id = candidate.policy_id();
    if matches!(candidate, DelegationLanePolicyCandidate::Create(_)) {
        ensure_delegation_lane_policy(
            client,
            cli,
            trading_identity,
            session,
            parent_policy,
            lane.policy_id,
        )
        .await?;
    }
    lane.next_nonce = client
        .next_intent_nonce(lane.policy_id)
        .await
        .with_context(|| {
            format!(
                "fetch next nonce for rotated lane policy {}",
                lane.policy_id
            )
        })?;
    lane.pending_intent = None;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
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

fn is_unknown_resting_order_error(error: &str) -> bool {
    error.contains("unknown_resting_order")
}

fn ambiguous_submit_intent_id(error: &SubmitError) -> Option<String> {
    match rpc_reason(error).as_deref() {
        Some("local_submit_admission_timeout") | Some("local_submit_worker_disconnected") => {
            submit_error_data_string(error, "intent_id_hex")
        }
        _ => None,
    }
}

fn pending_unknown_retry_deadline_ms(last_stage: &str, timeout_ms: u64) -> u64 {
    if last_stage == "submit_admission" {
        now_ms().saturating_add(timeout_ms)
    } else {
        0
    }
}

fn pending_intent_unknown_expired(pending: &PendingLaneIntent) -> bool {
    pending.unknown_retry_deadline_ms != 0 && now_ms() >= pending.unknown_retry_deadline_ms
}

fn rpc_reason(error: &and_chain_sdk::SubmitError) -> Option<String> {
    match error {
        and_chain_sdk::SubmitError::Rpc(error) => rpc_reason_from_rpc_error(error),
        _ => None,
    }
}

fn submit_error_data_string(error: &SubmitError, key: &str) -> Option<String> {
    match error {
        SubmitError::Rpc(error) => rpc_data_string(error, key),
        _ => None,
    }
}

fn rpc_reason_from_rpc_error(error: &RpcError) -> Option<String> {
    match error {
        RpcError::JsonRpc { data, .. } => data
            .as_ref()
            .and_then(|data| data.get("reason"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn rpc_data_string(error: &RpcError, key: &str) -> Option<String> {
    match error {
        RpcError::JsonRpc { data, .. } => data
            .as_ref()
            .and_then(|data| data.get(key))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        _ => None,
    }
}

#[cfg(test)]
#[path = "kombat_mm_bot/tests.rs"]
mod tests;
