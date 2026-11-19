# Testnet Operations Runbook

This is the practical operator guide for the current `&` chain testnet
deployment model.

It is intentionally scoped to what is real on this branch today:

- one `kombat_chain` binary per validator host
- JSON-RPC as the control and operator surface
- snapshot save/load for restart and backup operations
- remote snapshot bootstrap/rejoin from another validator over RPC
- controlled validator rollout, not an open validator set

This is the layer above:

- [CONSENSUS_RUNBOOK.md](./CONSENSUS_RUNBOOK.md)
- [EXECUTION_RECOVERY_RUNBOOK.md](./EXECUTION_RECOVERY_RUNBOOK.md)

Those documents describe failure response. This document describes how to
run the testnet cleanly day to day.

## Current Deployment Model

Each validator host runs:

- one `kombat_chain` process
- one local snapshot directory
- one operator bearer token for protected RPC methods
- one consensus signing key
- one configured validator identity

Current operator-critical env vars:

- `KOMBAT_RPC_BIND`
- `KOMBAT_CHAIN_ID`
- `KOMBAT_VALIDATOR_SET_PATH`
- `KOMBAT_VALIDATOR_ID_HEX`
- `KOMBAT_CONSENSUS_SIGNING_KEY_HEX`
- `KOMBAT_OPERATOR_TOKEN`
- `KOMBAT_PUBLIC_RPC_URL`
- `KOMBAT_LOAD_SNAPSHOT_PATH`
- `KOMBAT_BOOTSTRAP_RPC_URL`
- `KOMBAT_BOOTSTRAP_BEARER_TOKEN`
- `KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS`
- `KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS`
- `KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS`

For controlled testnet, do not expose operator methods publicly without
network-level protection. The bearer token is necessary but not
sufficient.

## Host Baseline

Per validator host:

- modern x86_64 or ARM64 CPU, `16` cores preferred
- `32-64 GB` RAM
- fast NVMe storage
- `1 Gbps` network
- separate filesystem space for:
  - binary + config
  - logs
  - snapshots

Recommended paths:

- binary: `/opt/and-chain/bin/kombat_chain`
- config/env: `/etc/and-chain/kombat-chain.env`
- snapshots: `/var/lib/and-chain/snapshots`
- logs: journalctl or `/var/log/and-chain`

## Preflight Checklist

Before bringing a validator into the testnet:

1. Confirm `KOMBAT_CHAIN_ID` is identical across all validators.
2. Confirm `KOMBAT_VALIDATOR_SET_PATH` is present and identical across
   all validators.
3. Confirm `KOMBAT_VALIDATOR_ID_HEX` matches the validator identity
   assigned to that host.
4. Confirm `KOMBAT_CONSENSUS_SIGNING_KEY_HEX` derives to the same public
   key expected for that validator.
5. Confirm `KOMBAT_OPERATOR_TOKEN` is unique to the environment and not a
   placeholder.
6. Confirm `KOMBAT_PUBLIC_RPC_URL` matches the validator's reachable RPC
   address if peer bootstrap will be used.
7. Confirm the snapshot directory exists and is writable by the service
   user.
8. Confirm the node boots and `kombat_chainInfo` returns successfully.
9. Confirm snapshot save succeeds before traffic:
   - `kombat_saveSnapshot`
10. Confirm the smoke harness passes against the deployed binary shape in a
   staging environment:
   - `chain/scripts/smoke_rpc.sh`

Do not bring four fresh validators online simultaneously without first
verifying one clean bootstrap in staging.

## Bootstrap Procedure

For a brand-new validator with no snapshot:

1. Deploy the binary and env file.
2. Set `KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS` only for fresh boot.
3. Start the service.
4. Verify:
   - `kombat_chainInfo` succeeds
   - logs show clean startup
   - no immediate snapshot/bootstrap failure
5. Save an initial operator snapshot and keep it.

For a validator booting from a known-good snapshot:

1. Put the snapshot on local disk.
2. Set `KOMBAT_LOAD_SNAPSHOT_PATH=/path/to/snapshot`.
3. Start the service.
4. Verify:
   - logs show snapshot loaded
   - `kombat_chainInfo` returns the expected head
   - the validator catches up or resumes progress
5. Remove the bootstrap snapshot env after successful start if your
   service manager persists environment between runs.

For a validator booting from another validator over RPC:

1. Set:
   - `KOMBAT_BOOTSTRAP_RPC_URL=http://<existing-validator>:8545`
   - `KOMBAT_BOOTSTRAP_BEARER_TOKEN=<operator token>`
2. Do not also set `KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS`.
3. Start the service.
4. Verify:
   - logs show remote bootstrap metadata fetched
   - logs show remote snapshot loaded
   - `kombat_chainInfo` returns the expected head
5. Remove the remote bootstrap env vars after successful start if they
   are only intended for first bootstrap/rejoin.

## Snapshot Policy

Minimum testnet snapshot policy:

- save snapshots on a timer, not by hand only
- keep at least:
  - last `24` hourly snapshots
  - last `7` daily snapshots
- keep snapshots on local disk first
- copy them off-host if the host is not disposable

Current snapshot method:

- JSON-RPC `kombat_saveSnapshot`
- requires `Authorization: Bearer <operator token>`

Suggested cadence:

- hourly snapshots during active testnet
- one pre-maintenance snapshot before any planned restart
- one post-maintenance validation snapshot after the node is healthy again

Use the provided helper:

```bash
cd chain
KOMBAT_RPC_URL=http://127.0.0.1:8545 \
KOMBAT_OPERATOR_TOKEN=... \
KOMBAT_SNAPSHOT_DIR=/var/lib/and-chain/snapshots \
./scripts/save_snapshot.sh
```

## Restart Procedure

Planned restart:

1. Save a fresh snapshot.
2. Record:
   - head height/hash
   - event cursor
   - current round / last committed round
3. Restart one validator only.
4. Verify after restart:
   - `kombat_chainInfo` matches or advances from pre-restart head
   - peer count recovers
   - timeout growth stabilizes
   - block production resumes or catch-up occurs

Unplanned restart after crash:

1. Capture logs first.
2. If local state is trusted, restart normally.
3. If local state is suspect, restart from the last known-good snapshot.
4. If bootstrap fails due to:
   - chain mismatch
   - validator mismatch
   - signing-key mismatch
   - corrupt snapshot
   do not bypass the check. Fix config or replace the snapshot.

## Alerts To Wire Up

Minimum alerts for controlled testnet:

- RPC health endpoint unavailable
- `consensus_last_committed_round` flat for too long
- `consensus_timeouts_total` rising sharply
- peer count below expected threshold
- snapshot save failure
- repeated snapshot bootstrap failure
- repeated operator load failure
- head divergence between validators

## Protected Methods

At minimum, keep these operator methods behind the bearer token and
network ACLs:

- `kombat_produceBlock`
- `kombat_saveSnapshot`
- `kombat_loadSnapshot`
- risk setter / market mutation methods

Do not route these methods through public client infrastructure.

## Testnet Go-Live Checklist

Before calling the testnet live:

1. Validate SDK packages against the target RPC URL.
2. Run the smoke harness against a staging node.
3. Confirm every validator host can:
   - save a snapshot
   - restart from a snapshot
   - reject bad snapshot bootstrap cleanly
4. Confirm logs and metrics are collected centrally.
5. Confirm snapshot retention works automatically.
6. Confirm on-call operators have:
   - bearer tokens
   - host access
   - restart and recovery instructions
7. Confirm one maintenance drill:
   - save snapshot
   - restart one validator
   - verify resumed progress

## Not Yet Covered By This Runbook

These are still the next realism steps after controlled testnet launch:

- multi-process validator orchestration in-repo
- live vote / QC exchange across separate validator processes
- validator set change workflow
- automated snapshot shipping / restore promotion
- full upgrade choreography across a live validator cluster

That work should land before calling the network production-grade.
