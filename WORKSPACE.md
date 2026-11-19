# Chain Workspace

This directory is the Rust workspace for the agent-native chain
implementation described in [`docs/chain/`](../docs/chain/).

## Purpose

The existing repository already contains the application stack. The chain
runtime lives in its own top-level namespace so the Rust implementation can
evolve without colliding with the existing .NET, frontend, and deployment
code.

## Layout

- `Cargo.toml` — workspace definition for all chain crates
- `rust-toolchain.toml` — stable Rust toolchain plus `rustfmt` and `clippy`
- `CRATE_MAP.md` — crate responsibilities and boundaries
- `OWNERS.md` — lane ownership for the first implementation wave
- `CONSENSUS_RUNBOOK.md` — M1 operator playbook for validator outage, stall, and restart
- `TESTNET_OPERATIONS_RUNBOOK.md` — controlled testnet bootstrap, snapshot, and restart guide
- `crates/consensus` — HotStuff-2 / validator coordination
- `crates/execution` — block execution driver and `revm` integration shell
- `crates/state-db` — state storage abstractions and root pipeline shell
- `crates/network` — peer management and transport
- `crates/rpc` — chain-facing RPC surfaces
- `crates/policy` — account authority, policies, session-key admission hooks
- `crates/matching` — deterministic matching and exchange accounting shell
- `crates/oracle` — marks, aggregation, and market-safety inputs
- `crates/streams` — event stream and subscriber surfaces
- `crates/node` — binary entrypoint that wires the crates together

## Scope

The workspace is past scaffold-only status. The current branch includes:

- `CH-001`: workspace layout and CI
- `CH-004` to `CH-007`: block types, validator config, message/QC validation, pacemaker shell
- `CH-008`: single-node block production
- `CH-009`: in-process four-validator local cluster harness
- `CH-010`: consensus telemetry primitives and first-pass outage runbook

This is still pre-execution for `revm`, policy, and exchange logic. The
implemented pieces are the consensus and node shells needed to keep M1
moving.

## Quickstart

Run the local runtime with dev auto-production enabled:

```bash
cd chain
./scripts/kombat_demo.sh start
```

Equivalent direct command:

```bash
cd chain
KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100 \
KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS=100 \
KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS=10 \
cargo run -p kombat-chain-rpc --bin kombat_chain
```

Watch executed events:

```bash
curl -N 'http://127.0.0.1:8545/events?from_seq=0&policy_id=42&topic=intent_executed&follow=true&timeout_ms=30000'
```

Create an account:

```bash
cargo run -p kombat-chain-rpc --bin kombat_sign -- create-account \
  --account-id 11 \
  --owner-secret-hex 0808080808080808080808080808080808080808080808080808080808080808 \
  --code-hash-hex 0909090909090909090909090909090909090909090909090909090909090909 \
  --timestamp-ms 1000 \
| curl -s http://127.0.0.1:8545 -H 'content-type: application/json' -d @-
```

Create a policy:

```bash
cargo run -p kombat-chain-rpc --bin kombat_sign -- create-policy \
  --owner-secret-hex 0808080808080808080808080808080808080808080808080808080808080808 \
  --grantee-secret-hex 0909090909090909090909090909090909090909090909090909090909090909 \
  --account-id 11 \
  --policy-id 42 \
  --expires-at 50000 \
  --timestamp-ms 1050 \
| curl -s http://127.0.0.1:8545 -H 'content-type: application/json' -d @-
```

Submit an intent:

```bash
cargo run -p kombat-chain-rpc --bin kombat_sign -- cancel-intent \
  --session-secret-hex 0909090909090909090909090909090909090909090909090909090909090909 \
  --account-id 11 \
  --policy-id 42 \
  --nonce 1 \
  --expires-at 10000 \
  --timestamp-ms 1100 \
  --order-id 7 \
| curl -s http://127.0.0.1:8545 -H 'content-type: application/json' -d @-
```

Generate a conditional intent:

```bash
cargo run -p kombat-chain-rpc --bin kombat_sign -- transfer-intent \
  --session-secret-hex 0909090909090909090909090909090909090909090909090909090909090909 \
  --account-id 11 \
  --policy-id 42 \
  --nonce 2 \
  --expires-at 20000 \
  --time-after-ms 12345 \
  --to 17 \
  --token 1 \
  --amount 250 \
  --timestamp-ms 1150 \
| curl -s http://127.0.0.1:8545 -H 'content-type: application/json' -d @-
```

Or use the wrapper script for the current demo flow:

```bash
cd chain
./scripts/kombat_demo.sh watch
./scripts/kombat_demo.sh watch-account
./scripts/kombat_demo.sh account
./scripts/kombat_demo.sh policy
INTENT_ID_HEX=0x... ./scripts/kombat_demo.sh intent-status
./scripts/kombat_demo.sh events
EVENT_ACCOUNT_ID=11 EVENT_TOPIC=account_updated ./scripts/kombat_demo.sh events
EVENT_POLICY_ID=42 EVENT_TOPIC=intent_executed ./scripts/kombat_demo.sh events
./scripts/kombat_demo.sh bootstrap
./scripts/kombat_demo.sh conditional-transfer
./scripts/kombat_demo.sh create-sub-policy
./scripts/kombat_demo.sh revoke-policy
./scripts/kombat_demo.sh propose-owner
./scripts/kombat_demo.sh accept-owner
./scripts/kombat_demo.sh pause-account
./scripts/kombat_demo.sh unpause-account
./scripts/kombat_demo.sh add-policy
./scripts/kombat_demo.sh remove-policy
./scripts/kombat_demo.sh revoke-policy-owner
./scripts/kombat_demo.sh upgrade-account
```

## RPC smoke test

`scripts/smoke_rpc.sh` boots `kombat_chain` on a disposable port and runs a
short end-to-end check against the recent hardening paths: account + policy
creation, SSE event stream plus `Last-Event-ID` resume behavior, operator
snapshot save plus process restart continuity, valid signed intents,
wrong-chain and tampered signed intents, wrong-chain and tampered signed
owner calls, and an oversize HTTP body (expects `413 Payload Too Large`).
The server is shut down on exit, pass or fail.

```bash
cd chain
./scripts/smoke_rpc.sh                 # defaults: port 18545
PORT=19545 ./scripts/smoke_rpc.sh      # override port
KEEP_LOG=1 ./scripts/smoke_rpc.sh      # keep server log even on success
```

Exit code is non-zero if any check fails; the server log path is printed
when a failure occurs or `KEEP_LOG=1` is set.

## Testnet Ops Assets

For controlled testnet rollout, this branch now includes:

- `TESTNET_OPERATIONS_RUNBOOK.md`
- `scripts/save_snapshot.sh`
- `deploy/systemd/kombat-chain.service.example`
- `deploy/systemd/kombat-chain.env.example`
- `deploy/systemd/kombat-chain-snapshot.service.example`
- `deploy/systemd/kombat-chain-snapshot.timer.example`

The current deployment model is still one `kombat_chain` process per
validator host with operator RPC methods protected by
`KOMBAT_OPERATOR_TOKEN`.

For the first provisioned testnet hosts, see:

- `deploy/testnet/README.md`
- `deploy/testnet/validator-inventory.example.toml`
- `scripts/generate_testnet_assets.sh`
- `scripts/install_validator_host.sh`
- `scripts/rollout_testnet_network.sh`
- `scripts/live_network_status.sh`
- `scripts/fanout_owner_call.sh`
- `scripts/seed_network_account.sh`
- `scripts/smoke_live_network.sh`
- `scripts/benchmark_live_network.sh`
- `scripts/smoke_live_timeout_recovery.sh`
- `scripts/benchmark_live_timeout_recovery.sh`
- `scripts/capture_live_network_baseline.sh`
- generated `install-all.sh` / `start-all.sh` / `status-all.sh` helpers
- `scripts/smoke_peer_bootstrap.sh`
- `scripts/smoke_network_round.sh`
- `scripts/smoke_network_auto_round.sh`
- `scripts/smoke_network_timeout_recovery.sh`
- `scripts/smoke_network_auto_timeout_recovery.sh`

The auto-timeout smoke now also proves follower write failover: a signed
intent submitted to a surviving follower while the elected leader is down
triggers timeout recovery, retries the forward against the new leader, and
still commits in the next automatic round.

For live operator visibility, the RPC layer now also exposes:

- `kombat_getConsensusMetrics`
- `kombat_getConsensusLogs`

and `scripts/live_network_status.sh` uses those methods to print per-validator
round, view, reachable/configured peer health, timeout totals, committed
round, latest consensus event, current head hash, and simple live latency
summaries for QC formation, round duration, and last block size.

The validator network can now also opt into a persistent peer transport on the
same RPC port with:

- `KOMBAT_PEER_TRANSPORT_ENABLED=true`

When enabled across the full validator set, internal `/network/*` POST traffic
reuses framed TCP peer connections instead of opening a fresh HTTP request for
each consensus hop. Public RPC, bootstrap metadata, snapshots, and operator
surfaces stay on HTTP.

For a quick end-to-end write benchmark against the real validator hosts, use:

- `scripts/benchmark_live_network.sh`
- `scripts/benchmark_live_timeout_recovery.sh`
- `scripts/capture_live_network_baseline.sh`
