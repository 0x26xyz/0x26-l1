# Kombat Chain Live Testnet Ansible

This directory is the source of truth for the four-host Mahi live testnet
deployment. It manages:

- one validator service per host on `:8545`
- three ingress services per host on `:8645`, `:8646`, and `:8647`
- the Mahi high-load env flags that must be identical on every validator and
  ingress process
- QuestDB plus `kombat_market_data` on the loadgen host for durable UI candle
  history
- the validator set and systemd units
- post-deploy verification that catches config drift before TPS runs

## Local Ansible Install

Ansible should be installed into this directory's local virtualenv, not `/tmp`,
so deployment tooling survives shell/session cleanup without touching system
packages:

```bash
cd chain/deploy/ansible
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
```

Use it from the repo root:

```bash
chain/deploy/ansible/run_playbook.sh \
  -i chain/deploy/ansible/inventory/testnet.yml \
  chain/deploy/ansible/playbooks/verify.yml
```

Use `run_playbook.sh` instead of invoking `ansible-playbook` directly from the
repo root. It pins `ANSIBLE_CONFIG` and SSH control sockets so Ansible does not
fall back to `~/.ansible/cp`.

## GitHub Actions Binary Artifacts

The `Build Chain Binaries` workflow builds Linux `amd64` deploy artifacts for:

- `kombat_chain`
- `kombat_sign`
- `kombat_loadgen`
- `kombat_market_data`

Every run uploads a SHA-pinned artifact named
`chain-binaries-linux-amd64-<git sha>`. Pushes to `main` also refresh
`chain-binaries-linux-amd64-main-latest`.

Download one of those artifacts from the Actions UI or with GitHub CLI:

```bash
gh run download -n chain-binaries-linux-amd64-main-latest
export KOMBAT_BINARY_PATH="$PWD/chain/target-linux-x86_64/release/kombat_chain"
```

For a pinned deploy, download the SHA-specific artifact instead and point
`KOMBAT_BINARY_PATH` at that extracted `kombat_chain` binary. `site.yml` also
deploys the QuestDB-backed market-data service to the loadgen host; it uses
`target-linux-x86_64/release/kombat_market_data` by default, or
`KOMBAT_MARKET_DATA_BINARY_PATH` when set.

## Deploy

Build or provide a Linux `amd64` `kombat_chain` binary first. The default
local path is `chain/target-linux-x86_64/release/kombat_chain`, not
`chain/target/release/kombat_chain`, so macOS native builds do not get copied
to the validators by mistake. For native builds on a validator host, copy the
binary back locally and set `KOMBAT_BINARY_PATH`.

```bash
export KOMBAT_OPERATOR_TOKEN=...
export KOMBAT_BINARY_PATH=/tmp/kombat_chain

chain/deploy/ansible/run_playbook.sh \
  -i chain/deploy/ansible/inventory/testnet.yml \
  chain/deploy/ansible/playbooks/site.yml
```

For a local Docker-based `amd64` build without clobbering your native
`target/release`, use the checked-in builder image wrapper. It builds/reuses
`kombat-chain-linux-builder:1.93`, writes artifacts under
`chain/target-linux-x86_64/release`, and prints the deployed binary hashes:

```bash
chain/scripts/build_linux_binaries.sh
```

## Persistence Migration

For a non-resettable environment, do not use the destructive reset playbooks to
cross a persistence-fingerprint mismatch. Use the upgrade playbook so the target
binary and env are rendered first without a restart, then the offline migration
gate stops validator/ingress services, checks or migrates `state.redb`, hydrates
the result with the target binary, starts services, and runs verification:

```bash
cd chain/deploy/ansible
KOMBAT_OPERATOR_TOKEN=... .venv/bin/ansible-playbook \
  playbooks/upgrade.yml
```

On redb-backed validator hosts, the playbook first requires an existing
`state.redb`; missing durable state fails before services are stopped. Memory
ingress hosts are skipped by the migration gate. It then runs
`check-persistence-fingerprint`, performs a `migrate-state` dry-run when the
backend fingerprint differs, then runs the confirmed migration only if the binary
has a compiled registry path. A mismatch without a registry path fails the
playbook; it does not reset `/var/lib/and-chain`. Before services are started
again, the playbook always runs a target-binary `migrate-state --dry-run`
hydration check, even when the fingerprint is already compatible. This catches
runtime snapshot-validator failures that are stricter than the schema
fingerprint envelope.

For manual debugging, the same sequence can be run as two steps:

```bash
cd chain/deploy/ansible
KOMBAT_OPERATOR_TOKEN=... .venv/bin/ansible-playbook \
  playbooks/deploy.yml -e and_chain_restart_services=false
KOMBAT_OPERATOR_TOKEN=... .venv/bin/ansible-playbook playbooks/migrate_state.yml
```

To render files without restarting services:

```bash
cd chain/deploy/ansible
.venv/bin/ansible-playbook playbooks/deploy.yml \
  -e and_chain_restart_services=false
```

## Restart Persistence TODO

Plain deploy/restart must not be treated as a state reset. The destructive
reset paths are `reset_state.yml`, `reset_and_seed.yml`, and
`reset_seed_and_start_perps_demo.yml`; those intentionally remove
`/var/lib/and-chain` and rebuild from seed state.

Current gap: validator state is process-local unless the service is started
from `KOMBAT_LOAD_SNAPSHOT_PATH` or remote bootstrap. The seed flow saves a
bootstrap snapshot and uses it for a one-shot restart, then removes
`KOMBAT_LOAD_SNAPSHOT_PATH`. That means a later plain restart of validator-1 can
boot a fresh engine if no explicit snapshot load is configured, and other
validators/ingress can then bootstrap from that fresh head.

TODO before treating testnet restarts as production-safe: add a non-destructive
restart path that saves a latest snapshot before restarting validator-1, keeps a
stable load target such as `/var/lib/and-chain/snapshots/latest.snapshot`, starts
validator-1 from that snapshot, and only then restarts the rest of the validator
set and ingress services. Longer term, replace this operator snapshot handoff
with durable block/state persistence in the node.

## Verification Gate

Run this before trusting live TPS numbers:

```bash
cd chain/deploy/ansible
.venv/bin/ansible-playbook playbooks/verify.yml
```

The verifier fails if:

- any of the four systemd services per host is inactive
- any validator or ingress env is missing the known-green Mahi flags
- any local RPC port fails `kombat_chainInfo`

It also prints a durable redb-validator head summary. By default that audit is
report-only because the current testnet pins public gateway traffic to
validator-2 while native-MAHI peer convergence is still being hardened. To make
head/root divergence fail the gate, run with:

```bash
KOMBAT_VERIFY_HEAD_CONSISTENCY_STRICT=1 \
  .venv/bin/ansible-playbook playbooks/verify.yml
```

This specifically prevents the previous drift where primary ingress/validator
services lacked `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2` while ingress2/3 had it.

## Initial State Seed

After a state wipe, seed deterministic testnet state on every validator before
using the perps UI:

```bash
cd chain/deploy/ansible
KOMBAT_OPERATOR_TOKEN=... .venv/bin/ansible-playbook playbooks/seed_initial_state.yml
```

For the usual wipe flow, use the combined playbook:

```bash
cd chain/deploy/ansible
KOMBAT_OPERATOR_TOKEN=... .venv/bin/ansible-playbook playbooks/reset_and_seed.yml
```

The seed is idempotent. It currently ensures market `7` (`BTC`) is listed with
the oracle feeder allowlist used by `kombat-feeder-live`; the live feeder then
repopulates mark/index prices on its next tick.

For a fresh perps UI demo environment after a wipe, use the perps-demo wrapper
instead. It resets the QuestDB candle store, redeploys/restarts market-data,
resets chain state, seeds the five demo markets plus RFC 027 MM prerequisites,
restarts ingress so public RPC re-bootstraps from the seeded validator state,
verifies validator services, deploys/restarts the live feeder, and
deploys/restarts the demo liquidity bot. The chain reset is intentionally
destructive and can be repeated for every testnet demo run:

```bash
cd chain/deploy/ansible
KOMBAT_OPERATOR_TOKEN=... \
KOMBAT_FEEDER_BINARY_PATH=/path/to/kombat_feeder \
  .venv/bin/ansible-playbook playbooks/reset_seed_and_start_perps_demo.yml
```

Use `reset_and_seed.yml` when you only need clean chain state. Use
`reset_seed_and_start_perps_demo.yml` when the UI needs fresh QuestDB candles,
live mark/index, order-book depth, trade tape, and candle data immediately
after reset. During the RFC 027 MM/taker hardening window, the live-testnet env
temporarily pins `KOMBAT_MAHI_MULTILEADER=0` to force single-leader MAHI. The
runtime parser treats `1`/`true` as "enable default multileader", not "one
leader". `KOMBAT_MAHI_ALLOW_IDLE_EMPTY_ROUNDS=1` must stay present on validators
and ingress processes or reset/bootstrap liveness regresses.

Temporary live-testnet relaxations must be called out in the inventory and
experiment log before deploy. `KOMBAT_MAHI_PAYLOAD_SHARDING=0`, when used,
means a MAHI producer may drain all locally ready payload frontiers instead of
only the producer's policy shard. It is consensus-safe because committed
payload replay remains deterministic, but it is not production-scaled: it trades
shard ownership for lower testnet bot/origin-relay friction and can waste block
capacity on duplicate inclusions. Remove the override and restore default
payload sharding before treating the environment as production-shaped.

## Demo Perps Liquidity Bot

For UI/demo candles and RFC 027 market-maker smoke coverage, deploy the
testnet liquidity bot to `loadgen-1`:

```bash
cd chain/deploy/ansible
KOMBAT_OPERATOR_TOKEN=... .venv/bin/ansible-playbook playbooks/deploy_perps_demo_liquidity_bot.yml
```

The service is `kombat-perps-demo-liquidity-bot`. The reset seed playbook creates
the deterministic maker/taker accounts, policy USDC, and isolated collateral on
every validator. On startup the bot creates missing owner policies with
`kombat_sign`, defines the `testnet-prime-mm` RFC 027 program, and enrolls the
maker for each configured market. It then signs every cancel/place/cross order
as a session-key intent (`kombat_submitIntent`) instead of using direct order
RPC, keeping an HLP-style multi-level bid/ask ladder around the live oracle mark
for the configured five markets and sending a one-lot IOC cross each loop so
`kombat_getCandles`, trade tape, book snapshots, and market streams have live
data.

The remaining RFC 027 production work is compliance observation,
rebate/slashing accounting, and the eventual native market-maker strategy
service. The deployed bot is now consensus-path safe for quote submission; direct
order mode exists only behind `KOMBAT_LIQUIDITY_BOT_ORDER_SUBMISSION_MODE=direct`
for controlled tests.

## Benchmark Topology

The current green live-testnet reference in the decision log is item 219:
`85,382 TPS` at `100k` offered for `60s`, with:

- 4 validator services on `:8545`
- 12 ingress services total: `:8645`, `:8646`, and `:8647` on each host
- `32 writers x pipeline_depth=1 x batch_size=8` per ingress endpoint
- state-root convergence at common height
- drained Mahi pre-admitted pools after the standard drain

Do not compare a 4-endpoint canary against that 12-endpoint reference.

External loadgen hosts are tracked in the `kombat_loadgen` inventory group in
`inventory/testnet.yml`. Do not use `203.0.113.18` for green runs: it is a
2-vCPU/4GB host and dropped SSH mid-load in item 268. Add a replacement
8-vCPU+ dedicated runner before treating a 100k+ live failure as a chain limit.
Keep `203.0.113.20` out of the green runner pool as well: it is the JPN node
and adds unnecessary cross-region latency for the EU testnet.
Keep `203.0.113.11` out of the green runner pool too: item 273 showed that
its three round-robin-assigned ingress endpoints collapsed again while the host
was still carrying Kubernetes control-plane services. The checked-in green pool
is now four dedicated EU runners:
`203.0.113.10`, `203.0.113.12`, `203.0.113.13`, and `203.0.113.19`.

The scripted form is:

```bash
LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.13 root@203.0.113.19' \
LOADTEST_WRITER_SEED_TAG=270 \
LOADTEST_SEED_CONCURRENCY=16 \
LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache \
LOADTEST_PAYLOAD_CACHE=1 \
LOADTEST_WRITERS_JSON=/path/to/384-writers.json \
LOADTEST_OUTPUT_DIR=/tmp/and-live-distributed-ingress3-current-main-100k-60s \
bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60
```

When the testnet is reset before each run, keep `LOADTEST_WRITER_SEED_TAG`,
`LOADTEST_REMOTE_CACHE_DIR`, and `LOADTEST_PAYLOAD_CACHE=1` stable. Use
`LOADTEST_SEED_CONCURRENCY=16` for reset/reseed runs unless the chain rejects
the provisioning burst. That lets the runner reuse the remote Linux loadgen
binary, writer shards, and prebuilt signed payload bodies instead of
copying/re-signing everything on every run.
