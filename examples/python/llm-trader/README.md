# `and-llm-trader` — example LLM-driven trading agent

A minimal, end-to-end example agent that runs on the `&` chain and uses any
model on [OpenRouter](https://openrouter.ai) to drive directional trading
decisions on a single market.

This is **not** a market maker. It takes one-sided positions (long or short),
manages a single open position with target / stop logic via subsequent
decisions, and uses IOC takes by default. Use it as a fork-and-modify starting
point for your own agents (momentum, news-driven, sentiment, alpha
combinators, etc.).

> The on-chain formal market maker program (RFC-027) is a separate, rebated
> rebate-eligible role; this example intentionally complements rather than
> overlaps with it.

## What it shows

- Reading the order book, recent fills, and account risk via the existing
  `and-chain-sdk` Python client
- Asking an LLM (any OpenRouter model) for a structured JSON decision
- Translating the decision into a signed `place_order` intent
- A **session-key policy** with on-chain hard limits on volume, drawdown,
  and intent rate — so even a buggy or jailbroken model cannot exceed the
  mandate the owner key signed
- Local guardrails on top (max trade size, max position) for belt-and-braces
  safety

## Layout

```
llm-trader/
  pyproject.toml
  README.md
  llm_trader/
    __init__.py
    __main__.py        # CLI: keygen / setup / run
    openrouter.py      # tiny OpenRouter chat client (stdlib only)
    bootstrap.py       # one-shot account + policy mint
    agent.py           # observe → reason → act loop
```

## Install

```bash
cd chain/examples/python/llm-trader
pip install git+https://github.com/0x26xyz/0x26-python-sdk.git@v0.1.0
pip install -e .
```

## Configure

Everything is environment-driven so the example stays easy to script.

| Variable                          | Required | Default                          | Notes                                                       |
| --------------------------------- | :------: | -------------------------------- | ----------------------------------------------------------- |
| `AND_RPC_URL`                     |    ✓     | —                                | e.g. `https://testnet.0x26.xyz/api`                         |
| `AND_BEARER_TOKEN`                |          | —                                | optional, for hosted endpoints                              |
| `AND_OWNER_SECRET_HEX`            |  setup   | —                                | 32-byte ed25519 owner key (hex)                             |
| `AND_SESSION_SECRET_HEX`          |    ✓     | —                                | 32-byte ed25519 session key (hex)                           |
| `AND_ACCOUNT_ID`                  |          | `1`                              | numeric account id                                          |
| `AND_POLICY_ID`                   |          | `1`                              | numeric policy id                                           |
| `AND_MARKET_ID`                   |          | `1`                              | market the agent trades                                     |
| `AND_CHAIN_ID`                    |          | (autodetect)                     | overrides `chain_info()`                                    |
| `AND_MAX_VOLUME_PER_DAY_USDC`     |  setup   | `5000`                           | on-chain hard cap                                           |
| `AND_MAX_DRAWDOWN_BPS`            |  setup   | `500`                            | on-chain hard cap (5.00%)                                   |
| `AND_MAX_INTENTS_PER_SECOND`      |  setup   | `1`                              | on-chain hard cap                                           |
| `AND_MAX_INTENTS_PER_DAY`         |  setup   | `1000`                           | on-chain hard cap                                           |
| `AND_GAS_BUDGET_PER_DAY_MICROUSDC`|  setup   | `10_000_000`                     | initial gas budget                                          |
| `AND_GAS_REFILL_PER_DAY_MICROUSDC`|  setup   | `10_000_000`                     | daily refill                                                |
| `AND_POLICY_TTL_MS`               |  setup   | `604800000` (7d)                 | policy expiry                                               |
| `AND_CODE_HASH_HEX`               |  setup   | zeros                            | 32-byte account code hash                                   |
| `AND_MAX_TRADE_LOTS`              |          | `5`                              | local clamp on a single trade                               |
| `AND_MAX_POSITION_LOTS`           |          | `20`                             | local clamp on net position                                 |
| `AND_POLL_INTERVAL_S`             |          | `10`                             | seconds between decisions                                   |
| `AND_BOOK_DEPTH`                  |          | `5`                              | book levels per side in snapshot                            |
| `AND_EVENT_WINDOW`                |          | `50`                             | recent fills shown to the LLM                               |
| `AND_INTENT_TTL_MS`               |          | `30000`                          | intent expiry                                               |
| `AND_TESTNET_TAKER_CADENCE_ENABLED` |        | `false`                          | testnet-only: if the model says hold, submit a tiny IOC taker order for candle/fill generation |
| `AND_TESTNET_TAKER_SIZE_LOTS`     |          | `1`                              | lots per cadence order, still clamped by `AND_MAX_TRADE_LOTS` |
| `AND_TESTNET_TAKER_EVERY_N_HOLDS` |          | `1`                              | submit one cadence order every N hold decisions when enabled |
| `OPENROUTER_API_KEY`              |   run    | —                                | from <https://openrouter.ai/keys>                           |
| `OPENROUTER_MODEL`                |          | `anthropic/claude-sonnet-4.5`    | any OpenRouter model id works                               |
| `AND_LOG_LEVEL`                   |          | `INFO`                           | python logging level                                        |

## Run-through

### 1. Make keys

```bash
and-llm-trader keygen      # owner key
and-llm-trader keygen      # session key
```

Save those hex strings. The owner key authorises the account; the session key
signs intents and is bound by the on-chain policy.

### 2. Bootstrap the account + policy (one shot)

```bash
export AND_RPC_URL=https://testnet.0x26.xyz/api
export AND_OWNER_SECRET_HEX=<from step 1>
export AND_SESSION_SECRET_HEX=<from step 1>
export AND_ACCOUNT_ID=12345
export AND_POLICY_ID=42
export AND_MAX_VOLUME_PER_DAY_USDC=2000
export AND_MAX_DRAWDOWN_BPS=300

and-llm-trader setup
```

The session key now exists on-chain with a hard ceiling: the chain itself
will reject any intent above 2,000 USDC of daily volume or 3% drawdown,
no matter what the model says.

### 3. Dry-run with any OpenRouter model

```bash
export OPENROUTER_API_KEY=sk-or-...
export OPENROUTER_MODEL=openai/gpt-5            # or anthropic/claude-sonnet-4.5,
                                                # google/gemini-2.5-pro,
                                                # meta-llama/llama-3.3-70b-instruct, etc.
export AND_MARKET_ID=7
export AND_MAX_TRADE_LOTS=2
export AND_MAX_POSITION_LOTS=10
export AND_POLL_INTERVAL_S=15

and-llm-trader run --dry-run
```

You will see one `llm decision` line per cycle and a `DRY RUN intent=...`
line for every would-be order. Nothing is signed or submitted.

### 4. Go live

Drop the `--dry-run` flag:

```bash
and-llm-trader run
```

Watch fills land via the chain's event stream:

```bash
curl -N "${AND_RPC_URL%/api}/api/events?market_id=7&topic=match_filled&follow=true&timeout_ms=30000"
```

For resettable testnet demos that need guaranteed taker fills and chart candles,
enable the explicit cadence fallback:

```bash
export AND_TESTNET_TAKER_CADENCE_ENABLED=1
export AND_TESTNET_TAKER_SIZE_LOTS=1
export AND_TESTNET_TAKER_EVERY_N_HOLDS=1
and-llm-trader run
```

This does not change the default strategy. It only fires after the model returns
`hold`, alternates tiny IOC buys/sells at the current touch, and remains bounded
by the on-chain policy plus local max trade / max position limits.
For live resettable testnet chart generation, size this above dust and raise
`AND_MAX_TRADE_LOTS` / `AND_MAX_POSITION_LOTS` to match. A 1-lot BTC cadence
order is intentionally minimal and will barely move visible candles.

## How the loop works

```
┌──────────────────────────┐
│ get_order_book           │
│ list_events(match_filled)│  ──►  market snapshot (JSON)
│ get_account_risk_summary │
└──────────────────────────┘
              │
              ▼
   ┌──────────────────────┐
   │  OpenRouter chat     │   system prompt fixes the JSON schema:
   │  /chat/completions   │   {action, size_lots, time_in_force,
   │  response_format=    │    limit_price_ticks, reasoning}
   │  json_object         │
   └──────────────────────┘
              │
              ▼
   ┌──────────────────────┐
   │ local guardrails     │   clamp size to AND_MAX_TRADE_LOTS,
   │                      │   refuse to exceed AND_MAX_POSITION_LOTS
   └──────────────────────┘
              │
              ▼
   ┌──────────────────────┐
   │ sign + submit        │   sign_intent() with the session key,
   │ kombat_submitIntent  │   chain enforces policy mandate
   └──────────────────────┘
              │
              ▼
        sleep poll_interval_s
              │
              └────► loop
```

## Safety model

There are three nested layers:

1. **Owner key, off-chain.** Owns the account. Used only to mint / revoke
   policies. Should live on a hardware wallet or air-gapped machine for
   real deployments.
2. **Policy terms, on-chain.** `max_volume_per_day`, `max_drawdown_pct`,
   `max_intents_per_second`, `max_intents_per_day`, gas budget. The chain
   admission verifier rejects intents that would exceed these. The model
   cannot override them.
3. **Local guardrails, in-process.** `AND_MAX_TRADE_LOTS` and
   `AND_MAX_POSITION_LOTS` clamp every decision before it is signed.
   Use these for fast, cheap rejections.

If the LLM hallucinates a 1,000-lot trade, the local clamp drops it to
`AND_MAX_TRADE_LOTS`. If you misconfigure that too high, the on-chain
policy still bounces the intent. Two independent failures have to occur
before any unwanted order ever reaches the matcher.

## Extending

- **Different strategy**: rewrite `SYSTEM_PROMPT` in `agent.py`. The
  snapshot already contains everything a momentum, mean-reversion, or
  news-aware agent needs; only the prompt changes.
- **Multiple markets**: instantiate one `Agent` per `(market_id, policy_id)`
  pair in your own driver. Use a child policy per market for isolated risk.
- **Tool use**: extend `OpenRouterClient.chat_json` to expose function
  calling. The agent could ask for `get_market_state`,
  `get_liquidation_pressure_summary`, or off-chain feeds before deciding.
- **Predicates**: populate `intent["predicate"]` with `time_after` /
  `time_before` to schedule conditional execution.
- **Stop / target as resting orders**: switch the close path to `gtc`
  limit orders on the opposite side instead of IOC market closes.

## Files at a glance

- `llm_trader/openrouter.py` — 100 LOC HTTP client, JSON-mode, retries on 429/5xx
- `llm_trader/agent.py` — observe / decide / act loop and local guardrails
- `llm_trader/bootstrap.py` — owner-signed account + policy creation
- `llm_trader/__main__.py` — `keygen` / `setup` / `run` CLI
