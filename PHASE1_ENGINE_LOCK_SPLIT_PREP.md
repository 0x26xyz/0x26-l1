# Phase 1 Engine Lock Split Prep (HISTORICAL)

> **Status 2026-04-23: historical, not active execution guidance.**
>
> Phase 1 is structurally complete. The kept work from this prep — `EngineHandle`, domain split, `HeadSnapshot` via `ArcSwap`, `LockOrderTracker`, lock-profiling metrics, compat shim — is still in the tree and still load-bearing for Phase 4.
>
> Everything else in this doc (Slice 2/3/4 cuts, admission/block-assembly split sequencing) is superseded by the Phase 4 Mahi-Mahi plan. Do **not** execute Slices 2–4 from this doc. The commit-rate diagnostic (decision log item 51) proved admission-perimeter splits cannot break the current ceiling.
>
> Read this for background on what Phase 1 established, not for what to do next. For active guidance: `docs/plans/2026-04-23-chain-phase4-mahi-mahi-plan.md`.

## Purpose

This note is the execution prep for Phase 1 of the `250k` plan.

Goal:
- replace the single `Arc<Mutex<SingleNodeEngine>>` access pattern with domain-specific accessors
- start with the paths that are currently blocking soak stability and admission throughput
- avoid a blind refactor by inventorying where `engine.lock()` is actually used today

Scope:
- `chain/crates/rpc/src/lib.rs`
- `chain/crates/rpc/src/main.rs`
- `chain/crates/node/src/lib.rs`

## Canonical Lock Domains

From the updated plan, the target lock domains are:

1. `PolicyStore`
2. `MempoolBundle`
   - `intent_mempool`
   - `conditional_pool`
   - `proposer_frontier`
3. `MarketState`
   - order books
   - frozen accounts
   - marks / index prices / oracle config
   - paused markets
4. `ChainTail`
5. `Pacemaker`
6. `RuntimeEvents`

`HeadSnapshot` is outside the lock order and should move to `ArcSwap`.

Canonical acquisition order:

```text
Policy -> Mempool -> MarketState -> ChainTail -> Pacemaker -> Events
```

## Current Hot Lock Sites

### 1. Admission hot path

These are the current throughput-critical lock holders.

- `chain/crates/rpc/src/lib.rs:4861`
  - `drain_local_submit_queue`
  - takes the global engine lock and calls `NodeRpcService::submit_intent`
  - this is the source-admission drain worker that directly races with block assembly

- `chain/crates/rpc/src/lib.rs:9956`
  - `process_network_relay_intents`
  - previews each relayed intent, then enqueues it for the same global-lock admission worker
  - not the current soak-breaking path, but still on the same serialized admission design

- `chain/crates/node/src/lib.rs:4154`
  - `submit_intent`
  - target Phase 1 write critical section:
    - `Policy` read
    - `Mempool` write
    - `MarketState` read

- `chain/crates/node/src/lib.rs:4216`
  - `preview_submit_intent`
  - target Phase 1 read-only variant of the same admission logic

### 2. Block assembly / consensus hot path

These are the lock holders most likely starving admission under soak.

- `chain/crates/rpc/src/lib.rs:8360`
  - `run_network_round`
  - acquires the global engine lock multiple times in one round:
    - read head / round / chain id and sign head QC
    - install head QC and prepare block from mempool
    - sign block QC
    - apply prepared block
  - this is the strongest current contention candidate against `drain_local_submit_queue`

- `chain/crates/node/src/lib.rs:3772`
  - `prepare_network_block_commit_from_mempool`
  - target Phase 1 read critical section:
    - `Mempool` read
    - `MarketState` read
    - `HeadSnapshot` load

- `chain/crates/node/src/lib.rs:4019`
  - `apply_external_block`
  - target Phase 1 commit/apply path:
    - `Policy` write
    - `Mempool` write
    - `MarketState` write
    - `ChainTail`
    - `Pacemaker`

- `chain/crates/node/src/lib.rs:3706`
  - `install_prepared_block_state`
  - another commit/apply variant that will need the same domain split

### 3. Read-only query path

These do not drive the soak failure directly, but they still serialize on the same mutex today.

- `chain/crates/rpc/src/lib.rs:7027`
  - `kombat_getConsensusMetrics`
  - today takes the global engine lock to clone metrics and frontier stats
  - target Phase 1 split:
    - telemetry / pacemaker read
    - frontier snapshot read

- `chain/crates/rpc/src/lib.rs:7065`
  - the generic `handle_json_request` path
  - after network fanout/forwarding checks, many local methods still build `NodeRpcService` on the global engine lock
  - this is the broadest migration surface in `rpc`

- `chain/crates/rpc/src/lib.rs:9667`
  - event stream query path
  - only needs `runtime_events`

- `chain/crates/rpc/src/lib.rs:9140` and `chain/crates/rpc/src/lib.rs:9161`
  - observer ingress leader selection
  - currently takes the global lock just to read:
    - `local_validator_id`
    - `current_round`
    - `chain_id`
  - target Phase 1 split:
    - `Pacemaker` read
    - `HeadSnapshot` / chain metadata read

### 4. Network control / maintenance / repair path

These matter for correctness and operational recovery.

- `chain/crates/rpc/src/lib.rs:5248`
  - `local_timeout_for_round`
  - reads local validator id / round and may sign timeout messages
  - target split:
    - `Pacemaker`
    - validator metadata / signer access

- `chain/crates/rpc/src/lib.rs:9733`
  - `process_network_head_qc_signature`
  - signs against current head, may trigger forced repair

- `chain/crates/rpc/src/lib.rs:9998`
  - `process_network_qc_signature`
  - signs proposal QC, may trigger repair and retry

- `chain/crates/rpc/src/lib.rs:10066`
  - `process_network_commit`
  - applies externally committed blocks under the same global lock

- `chain/crates/rpc/src/lib.rs:10176`
  - `process_network_timeout_certificate`
  - installs timeout messages via the global lock

- `chain/crates/rpc/src/main.rs:755`
  - `spawn_auto_producer`
  - auto block production loop still holds the global lock around produce-from-mempool
  - not the current benchmark path, but it should migrate with the same block-assembly split

## Immediate Conclusions

1. The highest-value Phase 1 cut is not "all query paths first".
   - The current soak failure lines up with the two hottest paths:
     - `drain_local_submit_queue`
     - `run_network_round`

2. `run_network_round` is structurally worse than a single big lock hold.
   - It reacquires the mutex multiple times across one round.
   - After the split, this should become several narrow domain operations rather than one giant engine API.

3. The generic `NodeRpcService` API is a migration hazard.
   - Many `rpc` paths still assume "get mutable engine, build service, do everything".
   - Phase 1 should not try to preserve that shape everywhere.
   - It is better to add narrow `EngineHandle` accessors for the hot paths first.

4. `HeadSnapshot` should be split early.
   - Many places take the global lock just to read head / height / round metadata.
   - Moving head reads out of the lock order buys immediate simplification in both routing and consensus code.

## Recommended Implementation Order

### Slice 1. Build the new engine shape without switching callers

Files:
- `chain/crates/node/src/engine_handle.rs` (new)
- `chain/crates/node/src/lib.rs`

Work:
- define `FineGrainedEngine`
- move domain state into separate fields
- add `HeadSnapshot`
- add debug-only lock-order tracking
- keep a compatibility layer so callers still compile

Success condition:
- no behavior change
- tests remain green

### Slice 2. Split read-mostly metadata and query paths

Files:
- `chain/crates/rpc/src/lib.rs`
- `chain/crates/node/src/lib.rs`

Start with:
- `kombat_getConsensusMetrics`
- observer ingress leader selection
- event stream query path

Why:
- low behavioral risk
- validates `HeadSnapshot` and read accessors
- reduces non-critical contention before touching admission

### Slice 3. Split the admission hot path

Files:
- `chain/crates/rpc/src/lib.rs`
- `chain/crates/node/src/lib.rs`

Start with:
- `drain_local_submit_queue`
- `submit_intent`
- `preview_submit_intent`

Target:
- `Policy` read
- `Mempool` write
- `MarketState` read

Why:
- this is the current soak limiter
- it is the closest structural fix after the failed Phase 0 valve

### Slice 4. Split block assembly and commit/apply

Files:
- `chain/crates/node/src/lib.rs`
- `chain/crates/rpc/src/lib.rs`
- `chain/crates/rpc/src/main.rs`

Start with:
- `prepare_network_block_commit_from_mempool`
- `apply_external_block`
- `install_prepared_block_state`
- `run_network_round`

Why:
- this removes the admission-vs-block-assembly contention wall
- it is the real Phase 1 completion point

## What Not To Do During Phase 1

- Do not re-open queue-local scheduling experiments in `drain_local_submit_queue`.
- Do not mix dissemination redesign into the lock split.
- Do not preserve the old `NodeRpcService` shape at all costs if it forces broad lock holds.
- Do not start from follower relay batching; that is Phase 3 work.

## Suggested Next Code Cut

The next implementation step after this prep note should be:

1. add `chain/crates/node/src/engine_handle.rs`
2. define the domain structs and `HeadSnapshot`
3. wire `SingleNodeEngine` into a compatibility wrapper around the new fields
4. migrate the smallest read-only caller first:
   - `kombat_getConsensusMetrics`
   - observer ingress local-role/current-round lookup

That keeps the first Phase 1 patch small enough to review while still moving directly toward the real lock split.
