# Testnet Deployment Notes

This directory holds practical assets for bringing up the first
controlled `&` chain testnet on the provisioned validator hosts.

It assumes:

- one `kombat_chain` binary per validator host
- systemd-managed service
- operator snapshot saves via JSON-RPC
- manual host provisioning already complete

## Current Host Set

Provisioned validator IPs:

- `203.0.113.15`
- `203.0.113.17`
- `203.0.113.14`
- `203.0.113.16`

## Required Local Prep

Build the release binary first:

```bash
cd chain
cargo build --release -p kombat-chain-rpc --bin kombat_chain
```

You can either create validator assets manually, or generate them from a
single inventory file.

### Generated Assets

Start from:

- `deploy/testnet/validator-inventory.example.toml`

Fill in:

- real operator token
- real signing keys
- real validator IDs
- real public RPC URLs

Then generate:

```bash
cd chain
./scripts/generate_testnet_assets.sh \
  deploy/testnet/validator-inventory.example.toml \
  /tmp/and-testnet
```

That emits:

- `/tmp/and-testnet/validator-set.toml`
- `/tmp/and-testnet/validator-1.env` style files
- `/tmp/and-testnet/install-all.sh`
- `/tmp/and-testnet/start-all.sh`
- `/tmp/and-testnet/status-all.sh`
- `/tmp/and-testnet` compatible input for `scripts/rollout_testnet_network.sh`
- `/tmp/and-testnet/hosts.txt`

You can install from those generated assets directly. For the current live
four-host Mahi testnet, prefer the Ansible source of truth in
`deploy/ansible/`; the generator remains a lower-level asset renderer.

### Manual Env Files

If you prefer not to use the generator, create one env file per validator from:

- `deploy/systemd/kombat-chain.env.example`

Do not commit real validator secrets.

Each validator env file must set:

- `KOMBAT_RPC_BIND`
- `KOMBAT_CHAIN_ID`
- `KOMBAT_VALIDATOR_SET_PATH`
- `KOMBAT_VALIDATOR_ID_HEX`
- `KOMBAT_CONSENSUS_SIGNING_KEY_HEX`
- `KOMBAT_OPERATOR_TOKEN`
- `KOMBAT_PUBLIC_RPC_URL`

Optional bootstrap inputs:

- `KOMBAT_LOAD_SNAPSHOT_PATH`
- `KOMBAT_BOOTSTRAP_RPC_URL`
- `KOMBAT_BOOTSTRAP_BEARER_TOKEN`

## Install Onto A Host

Use the helper:

```bash
cd chain
./scripts/install_validator_host.sh \
  root@203.0.113.15 \
  /tmp/and-testnet/validator-1.env \
  --validator-set /tmp/and-testnet/validator-set.toml \
  --start
```

Repeat for each validator host.

If you generated assets, you can also use:

```bash
bash /tmp/and-testnet/install-all.sh
bash /tmp/and-testnet/start-all.sh
bash /tmp/and-testnet/status-all.sh
```

For ordered bootstrap startup across the real validator hosts, use:

```bash
cd chain
./scripts/rollout_testnet_network.sh /tmp/and-testnet
```

To inspect the exact remote commands first:

```bash
cd chain
./scripts/rollout_testnet_network.sh /tmp/and-testnet --dry-run
```

## Network State Writes

`kombat_createAccount` and signed `kombat_submitOwnerCall` now route through
the live validator network and replicate across the validator set from a single
RPC entrypoint.

For operational recovery or explicit bulk replay, you can still fan the same
signed owner-call request out to all validators:

```bash
cd chain
./scripts/fanout_owner_call.sh \
  /tmp/and-testnet \
  /path/to/signed-owner-call.json
```

That fallback remains useful if you need to force-replay state writes across a
partially degraded cluster:

- account creation
- policy creation
- pause / unpause
- owner rotation

For initial account + policy bootstrap across the live validator set, use:

```bash
cd chain
./scripts/seed_network_account.sh \
  /tmp/and-testnet \
  930100 \
  930142 \
  0808080808080808080808080808080808080808080808080808080808080808 \
  0909090909090909090909090909090909090909090909090909090909090909
```

To inspect the live cluster quickly:

```bash
cd chain
./scripts/live_network_status.sh /tmp/and-testnet
```

That status view now reads:

- `kombat_chainInfo`
- `kombat_getConsensusMetrics`
- `kombat_getConsensusLogs`

and prints round, current view, peer count, timeout totals, last committed
round, latest consensus event, and the current head hash for each validator,
plus live peer reachability as `reachable/configured` with down-peer URLs when
any validator is unreachable. It also reports QC/round latency summaries and
the latest block size from the consensus telemetry samples.

To run a real write-path smoke against the live validator hosts:

```bash
cd chain
./scripts/smoke_live_network.sh /tmp/and-testnet
```

To benchmark follower-routed write latency across the live validator hosts:

```bash
cd chain
./scripts/benchmark_live_network.sh /tmp/and-testnet 10
```

To benchmark leader failure, survivor recovery, leader rejoin, and one
post-rejoin write on the live validator hosts:

```bash
cd chain
./scripts/benchmark_live_timeout_recovery.sh /tmp/and-testnet
```

To capture a timestamped baseline bundle with live status plus both
benchmarks:

```bash
cd chain
./scripts/capture_live_network_baseline.sh /tmp/and-testnet 5
```

To enable the current persistent validator peer transport on the live network,
set this in every validator env before rollout:

```bash
KOMBAT_PEER_TRANSPORT_ENABLED=true
```

That transport reuses framed TCP connections on the existing RPC port for
internal `/network/*` POST traffic. Roll it out to the whole validator set in
one coordinated restart, not one mixed-mode validator at a time.

To exercise timeout recovery on the real validator hosts:

```bash
cd chain
./scripts/smoke_live_timeout_recovery.sh /tmp/and-testnet
```

To turn `testnet.0x26.xyz/api` into an Nginx failover proxy across the live
validator set:

```bash
cd chain
./scripts/configure_testnet_rpc_failover.sh /tmp/and-testnet root@203.0.113.15
```

After the first successful rollout, arm the bootstrap validator so later
service restarts reload from a peer snapshot instead of replaying the original
empty bootstrap block:

```bash
cd chain
./scripts/arm_validator_restart_bootstrap.sh /tmp/and-testnet
```

What it installs:

- `/opt/and-chain/bin/kombat_chain`
- `/opt/and-chain/scripts/save_snapshot.sh`
- `/etc/and-chain/kombat-chain.env`
- `/etc/and-chain/validator-set.toml` when `--validator-set` is provided
- `/etc/systemd/system/kombat-chain.service`
- `/etc/systemd/system/kombat-chain-snapshot.service`
- `/etc/systemd/system/kombat-chain-snapshot.timer`

## Local Peer-Bootstrap Smoke

Before touching more validator hosts, you can prove the new peer
bootstrap path locally with:

```bash
cd chain
./scripts/smoke_peer_bootstrap.sh
```

That harness starts two separate `kombat_chain` processes locally:

- source validator boots fresh with a validator set and advertised RPC URL
- peer validator bootstraps its state from the source over live HTTP
- peer validator then produces the next block from the bootstrapped head

For the next step toward a real network round, run:

```bash
cd chain
./scripts/smoke_network_round.sh
```

That harness starts four separate `kombat_chain` processes with distinct
validator identities, bootstraps three from the source snapshot, submits a
signed intent to the elected round leader, runs `kombat_runNetworkRound`, and
verifies that all four processes commit the same head and order-book state.

For the next step toward live autonomous rounds, run:

```bash
cd chain
./scripts/smoke_network_auto_round.sh
```

That harness starts the same four-process network shape with
`KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS` enabled on every validator,
submits signed intents to a follower, and verifies that the request is
forwarded to the elected leader and committed in the next round without
any operator `kombat_runNetworkRound` call.

For autonomous leader-failure recovery, run:

```bash
cd chain
./scripts/smoke_network_auto_timeout_recovery.sh
```

That harness keeps the same auto-rounding configuration, kills the
elected leader for the next round, verifies that a follower auto-drives
timeout recovery to the next round without an operator RPC call, proves a
signed intent submitted to another surviving follower still commits across
that failure, then restarts the failed validator from a fresh peer snapshot
and proves the following auto round still commits with all four processes
converged.

For leader-failure recovery across separate processes, run:

```bash
cd chain
./scripts/smoke_network_timeout_recovery.sh
```

That harness establishes a quorum-backed head, stops the elected leader
for the next round, runs `kombat_runNetworkTimeoutRecovery` on a follower,
verifies the remaining validators advance to the next round without
changing head, then restarts the stopped validator from a fresh peer
snapshot and proves the following leader can commit the next block with
all four processes converged again.

## Post-Install Checks

On each validator:

```bash
systemctl status kombat-chain --no-pager
systemctl status kombat-chain-snapshot.timer --no-pager
journalctl -u kombat-chain -n 100 --no-pager
```

Then from your workstation:

```bash
curl -s http://<host>:8545 -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}'
```

For operator snapshot save:

```bash
curl -s http://<host>:8545 \
  -H 'Authorization: Bearer <token>' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","method":"kombat_saveSnapshot","params":{"path":"/var/lib/and-chain/snapshots/manual.snapshot"},"id":2}'
```

## Rollout Order

For the current branch, bring them up one by one:

1. validator 1
2. validator 2
3. validator 3
4. validator 4

Verify each host boots cleanly before touching the next.

## Important Current Limitation

The current `kombat_chain` production entrypoint can now:

- start fresh
- load a local snapshot
- bootstrap its snapshot state from another validator over RPC
- run real vote/QC/commit rounds across separate validator processes
- auto-drive those rounds on a timer when the current leader has queued work

What it still does not do yet is a full persistent P2P validator
transport with autonomous timeout/view-change recovery. The current live
network path is still HTTP-based and timer-driven.
