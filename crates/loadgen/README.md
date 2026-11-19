# kombat_loadgen

A simple load generator for the `&` chain JSON-RPC API. Measures
throughput and latency percentiles against a running node.

## Workloads

- `chain-info` — unauthenticated `kombat_chainInfo` calls. Measures raw
  JSON-RPC serve throughput. No state setup required.
- `place-order` — signed `kombat_submitIntent` (PlaceOrder) calls.
  Requires a pre-existing account + policy + session key. Workers
  share an atomic nonce counter; the chain enforces strict monotonic
  nonces per policy, so `concurrency > 1` will produce out-of-order
  rejections that surface as `nonce_err` in the report unless you shard
  writers with `--writers-json`.

## Build

```sh
cargo build --release -p kombat-chain-loadgen
```

Binary lands at `target/release/kombat_loadgen`.

## Running against a dev RPC

To drive the matching engine end-to-end, start the RPC with
auto-produce on:

```sh
KOMBAT_RPC_BIND="127.0.0.1:19600" \
KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100 \
KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS=50 \
KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS=1000 \
cargo run --release -p kombat-chain-rpc --bin kombat_chain
```

Then:

```sh
target/release/kombat_loadgen \
  --url http://127.0.0.1:19600 \
  --workload chain-info \
  --concurrency 32 \
  --duration-secs 30
```

## Choosing a `--side` mode

Self-trade prevention plus the per-policy frontier-nonce rule means the
workload shape matters a lot for whether intents commit.

- `--side bid` (default) or `--side ask` — all orders on one side,
  single account, single policy. Orders rest in the book, nothing
  self-crosses. This is the correct mode for measuring committed TPS
  on a single writer.
- `--side spread` — alternate bid/ask at non-crossing prices
  (`price_ticks ± spread_ticks`). Exercises book-insertion on both
  sides without self-crossing. Note: this mode has been observed to
  stall block assembly in some configurations; use for specific
  diagnostics, not baseline TPS.
- `--side cross-self` — alternate bid/ask at the same price, one
  account. This is a **test-the-STP-path** mode. The first crossing
  intent is terminally rejected, later nonces get queued but never
  commit because they're above the frontier. `next_intent_nonce`
  stays pinned. See `chain/crates/node/src/lib.rs:11543` for the
  regression that covers this.

## `place-order` setup

You need an account + policy whose grantee is a session key you hold the
secret for. `scripts/smoke_rpc.sh` already does this end-to-end; the
public `and` CLI (https://github.com/0x26xyz/0x26-cli) has
`agent bootstrap` and `policy mint` for a more surgical setup.

Once you have `ACCOUNT_ID`, `POLICY_ID`, `CHAIN_ID`, and
`SESSION_SECRET_HEX`:

```sh
target/release/kombat_loadgen \
  --url http://127.0.0.1:19600 \
  --workload place-order \
  --concurrency 1 \
  --rps 200 \
  --duration-secs 30 \
  --chain-id 7 \
  --account-id 11 \
  --policy-id 42 \
  --market-id 1 \
  --session-secret-hex 0909090909090909090909090909090909090909090909090909090909090909
```

The starting nonce is fetched live via `kombat_getPolicy`, so repeated
runs don't need manual bookkeeping.

### Multi-writer committed load

For real write-load runs, seed one account + policy + session key per
worker and pass them as JSON:

```json
[
  {
    "account_id": 880001,
    "policy_id": 890001,
    "session_secret_hex": "1111..."
  },
  {
    "account_id": 880002,
    "policy_id": 890002,
    "session_secret_hex": "2222..."
  }
]
```

Then:

```sh
target/release/kombat_loadgen \
  --url http://127.0.0.1:19600 \
  --workload place-order \
  --writers-json /tmp/kombat-loadgen-writers.json \
  --concurrency 2 \
  --rps 200 \
  --duration-secs 30 \
  --request-timeout-secs 12 \
  --chain-id 7 \
  --market-id 1
```

With `--writers-json`, the tool expects at least one distinct writer per
worker and each worker advances its own policy nonce frontier.

If you need a bounded live diagnostic run, set `--request-timeout-secs`
so slow follower-to-leader submits surface as `net_err_samples` instead
of leaving workers parked indefinitely on a single request.

For live committed-TPS runs, also use `--strict-nonce-retry` with
`--pipeline-depth 1` and `--fail-on-errors`. Strict retry keeps a writer
on the same signed nonce batch after a transport error instead of
skipping to later nonces. This prevents the benchmark from creating
per-policy nonce holes that Mahi cannot safely drain. `--fail-on-errors`
exits non-zero if any nonce, RPC, or network error remains, so invalid
artifacts are not mistaken for throughput data. The live benchmark
wrapper scripts enable both by default.

### Reusable payload cache

For reset-before-each-run live benchmarks, `place-order` can prebuild and
reuse signed `kombat_submitIntentBatch` JSON bodies:

```sh
target/release/kombat_loadgen \
  --workload place-order \
  --writers-json /tmp/kombat-loadgen-writers.json \
  --concurrency 32 \
  --pipeline-depth 4 \
  --batch-size 8 \
  --rps 5000 \
  --duration-secs 60 \
  --intent-ttl-ms 315360000000 \
  --client-order-id-base 1000000000000 \
  --payload-cache-dir /var/tmp/kombat-loadgen-cache/validator-1-p8645
```

The cache is only supported for non-strict batched runs. It removes
per-request signing, Borsh hashing, and JSON body construction from the
measured window. Cached payloads are safe to reuse only after the chain
state has been reset and reseeded with the same deterministic writers.

### Concurrency and nonce rejection

Per-policy nonces are strictly monotonic. With `--concurrency 1` all
intents land in order and `nonce_err` should be 0. With higher
concurrency the atomic counter still hands out unique nonces but
network reordering means later workers may arrive before earlier
ones, producing `nonce_err`. This is expected and is a fidelity
measurement of back-pressure under contention — it does not indicate
a bug in the tool.

To fan out without that rejection, bootstrap one policy per worker and
use `--writers-json` so the tool shards by `worker_id`.

## Report

```
kombat_loadgen report
  url: http://127.0.0.1:19600
  workload: ChainInfo
  concurrency: 32
  target_rps: 0
  wall_elapsed_secs: 30.003
  measured_window_secs: 27.000

  requests_total: 289344
  requests_measured (post-warmup): 260478
  ok: 289344
  nonce_err: 0
  rpc_err: 0
  net_err: 0

  achieved_rps_measured: 9647.3
  latency_p50: 3.276ms
  latency_p90: 3.441ms
  latency_p99: 3.623ms
  latency_max: 40.122ms

  head_height_before: 1
  head_height_after:  1
  blocks_produced:    0
```

- `requests_measured` excludes the warmup window (`--warmup-secs`).
- `achieved_rps_measured = requests_measured / measured_window_secs`.
- `blocks_produced` is the `head_height` delta observed while the
  loadgen ran — useful for `place-order` workloads to see how many
  intents actually made it into blocks vs. were queued in the mempool.
- `*_err_samples` include the first few concrete RPC/network failures
  so live runs are diagnosable without rerunning under a debugger.
