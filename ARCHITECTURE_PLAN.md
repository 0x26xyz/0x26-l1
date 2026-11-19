# Chain Throughput Recovery And Architecture Plan (SUPERSEDED)

> **SUPERSEDED 2026-04-23** by the plan docs in `docs/plans/`:
>
> - **Active main plan:** `docs/plans/2026-04-23-chain-250k-tps-integration-plan.md`
> - **Active Phase 4 sub-plan:** `docs/plans/2026-04-23-chain-phase4-mahi-mahi-plan.md`
> - **Active decision log:** `chain/THROUGHPUT_DECISION_LOG.md` (still live; continue appending)
>
> This document's phase sequence (Phase 2 "Dissemination Plane Replacement", etc.) and "Immediate next phase: Phase 1" recommendation are pre-replan and no longer match execution order. Retained for historical context only — the trusted benchmark command and stability gate framing here remain valid and were absorbed into the active plans.
>
> If you are a fresh agent reading this: treat this doc as historical, go to the `docs/plans/` files above for current execution guidance.

## Purpose

This plan exists to stop ad hoc tuning and force sequential execution.

We will work in order.
We will not mix benchmark knob-turning, architectural redesign, and subsystem cleanup in the same phase.
Every phase must have an entry condition, exit condition, and benchmark gate.

## Current Proven State

Trusted benchmark command:

```sh
AUTO_MAX_ITEMS=5000 \
LOCAL_CLUSTER_ENABLE_INGRESS=1 \
LOADTEST_TARGET_VALIDATOR=ingress \
LOADTEST_BATCH_SIZE=8 \
LOADTEST_PIPELINE_DEPTH=32 \
KEEP_LOG=1 \
LOCAL_CLUSTER_OUTPUT_DIR=/tmp/<run-name> \
bash chain/scripts/loadtest_local_cluster.sh 32 8000 10
```

Current trusted kept result:
- artifact: `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
- committed: `87,194`
- committed TPS over 10s load window: `8,719.4`
- convergence: green

Current key diagnosis:
- The original limiter included round productivity.
- The `50ms` cadence change materially improved throughput.
- Admission-side micro-optimizations and submit-path lock cuts are no longer the active throughput path.
- The current ceiling is now best explained by commit cadence times underfilled blocks.
- The next limiter is uneven proposer-ready backlog conversion and poor block fullness across leaders.
- Peer transport is not yet stable enough to serve as the consensus/control fast lane.

2026-04-23 diagnostic update:
- A dedicated commit-shape diagnostic was binned as instrumentation because it regressed the baseline, but the evidence from its artifacts changed the phase priority.
- On the baseline harness shape, load-window block production was `186` and `183` blocks in `10s` on two runs, while committed totals were `82,476` and `82,992`.
- That implies an average committed block fill of about `443-454` items per produced block, which matches the observed `~8.3k TPS` ceiling.
- Conclusion: further Phase 1/Phase 2 admission tuning is unlikely to move committed TPS materially on this architecture. The next active throughput phase is dissemination/block fullness work.

Supporting evidence:
- kept `50ms` run validator logs showed `99` committed blocks
- `11` of those were empty
- `56` carried fewer than `512` items
- underfilled rounds were concentrated on validators 2 and 3

## Global Rules

Rules for all phases:
- Touch only `chain/` files.
- Do not keep speculative changes.
- Every experiment must state:
  - hypothesis
  - exact command
  - artifact path
  - result
  - kept, binned, or blocked
- After two failed experiments in the same area, stop tuning and return to design.
- Do not accept a benchmark result if the path is unstable or correctness is unclear.
- Keep [THROUGHPUT_DECISION_LOG.md](/Users/waynedouglas/github/hx/pvp/chain/THROUGHPUT_DECISION_LOG.md:1) up to date.

Stability gate for any kept throughput change:
- `3x` short reruns on the same harness shape
- `30s` load with `20s` settle
- clean post-load convergence
- zero `rpc_err` admission failures on the soak run
- if a change fails the soak gate, it is binned even if its 10s TPS is higher

## Execution Order

We will execute these phases sequentially:

1. Phase 0: Measurement Discipline
2. Phase 1: Control Plane Stabilization
3. Phase 2: Dissemination Plane Replacement
4. Phase 3: Block Assembly And Scheduler Rework
5. Phase 4: Catch-Up / Repair Redesign
6. Phase 5: Pipelined / Chained Consensus
7. Phase 6: Execution / Storage Parallelization

Only one phase may be in active implementation at a time.

Current active throughput phase:
- Phase 2: Dissemination Plane Replacement
- Rationale: commit cadence x block fullness now explains the throughput ceiling more convincingly than admission-side cost.

## Phase 0: Measurement Discipline

Goal:
- preserve one trusted benchmark shape
- stop wheel-spinning on low-value harness tuning

In scope:
- benchmark command stability
- artifact capture
- benchmark result validation
- harness fixes only when they are required to make a test path valid

Out of scope:
- throughput tuning not tied to a code hypothesis
- architecture changes

Required outputs:
- one trusted benchmark command
- one trusted baseline artifact
- one repo decision log

Exit criteria:
- baseline command is stable
- artifacts are reproducible
- no benchmark path is known-broken in a way that invalidates interpretation

Current status:
- complete enough to proceed

## Phase 1: Control Plane Stabilization

Goal:
- make validator-to-validator control traffic correct and boring
- eliminate unstable fast-lane behavior before building on top of it

Why this phase exists:
- peer transport is currently MVP quality
- the fixed peer-transport harness run still regressed badly
- observed failures included:
  - `InvalidBlock(NonMonotonicTimestamp)`
  - `InvalidBlock(InvalidHeight)`
  - quorum loss
  - repeated forced snapshot repair

Primary code areas:
- [chain/crates/rpc/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/lib.rs:6693)
- [chain/crates/rpc/src/main.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/main.rs:120)
- [chain/crates/consensus/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/consensus/src/lib.rs:224)

Hypotheses to validate:
- peer-transport session state is allowing stale or duplicated control-plane state
- timestamp progression is not being enforced consistently across repaired / relayed paths
- commit / proposal application can race or arrive out of monotonic order under peer transport

Required work:
- make peer-transport bootstrap and session lifecycle deterministic
- eliminate normal-path `NonMonotonicTimestamp`
- eliminate normal-path `InvalidHeight`
- eliminate forced snapshot repair from the happy path

Benchmark gate:
- rerun the standard 8k/10s harness with peer transport enabled
- success means:
  - no timestamp or height validation churn
  - no forced repair on normal progress path
  - no peer reachability collapse during the run
  - throughput at least matches the current trusted non-peer-transport baseline

Exit criteria:
- peer transport becomes a trustworthy control plane option
- if it still regresses throughput after correctness fixes, keep it disabled for benchmarks and mark blocked

## Phase 2: Dissemination Plane Replacement

Goal:
- replace async request-shaped relay after single-validator admission with a real proposer-ready dissemination lane

Why this phase exists:
- current path is duct tape
- ingress forwards to one validator
- replication to the rest happens later via async relay workers
- this creates uneven proposer-ready state across leaders

Primary code areas:
- [chain/crates/rpc/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/lib.rs:9108)
- [chain/crates/rpc/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/lib.rs:5081)
- [chain/crates/rpc/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/lib.rs:9829)

Target architecture:
- ingress admission produces batch objects, not only individual queued intents
- validators replicate and acknowledge proposer-ready batches
- proposers consume batch references or batch material with stable availability guarantees
- dissemination latency becomes independent from per-intent local worker drain behavior

Required outputs:
- design for batch object format and ownership
- validator replication semantics
- proposer consumption semantics
- failure semantics for partial dissemination

Benchmark gate:
- leader block fullness distribution becomes materially more even across validators
- empty/tiny rounds fall substantially from the current kept `50ms` profile
- committed TPS exceeds current trusted baseline without harming convergence

Exit criteria:
- proposal inputs are no longer dominated by uneven local request relay timing

## Phase 3: Block Assembly And Scheduler Rework

Goal:
- improve economics of turning ready backlog into committed block contents

Why this phase exists:
- after dissemination is fixed, scheduler and planner work becomes worth doing
- before that, scheduler work is compensating for bad inputs

Primary code areas:
- [chain/crates/node/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/node/src/lib.rs:3715)
- [chain/crates/node/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/node/src/lib.rs:5220)
- [chain/crates/rpc/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/lib.rs:6811)

Required work:
- reduce rescanning and repeated planning cost
- reduce empty and tiny proposals
- improve fairness of proposer-ready consumption without sacrificing fullness
- make block assembly draw from precomputed or incrementally maintained ready material when possible

Benchmark gate:
- empty rounds approach zero on the healthy path
- the majority of committed rounds clear a meaningful fill threshold
- committed TPS exceeds the Phase 2 baseline on the same harness shape

Exit criteria:
- block assembly is no longer the dominant throughput limiter on the current consensus shape

## Phase 4: Catch-Up / Repair Redesign

Goal:
- remove forced snapshot repair as a normal operational tool

Why this phase exists:
- forced snapshot repair is demo-quality once throughput and divergence increase
- a real validator network needs incremental certified recovery

Primary code areas:
- [chain/crates/rpc/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/rpc/src/lib.rs:5501)
- [chain/crates/node/src/lib.rs](/Users/waynedouglas/github/hx/pvp/chain/crates/node/src/lib.rs:1137)

Target architecture:
- certified lineage recovery
- block / QC based catch-up
- bounded replay and targeted state reconciliation

Benchmark gate:
- induced lagging-validator scenarios recover without snapshot-style repair
- normal benchmark path shows no repair churn

Exit criteria:
- forced snapshot repair is fallback-only, not normal-path infrastructure

## Phase 5: Pipelined / Chained Consensus

Goal:
- stop paying roughly one synchronous round cost per committed block

Why this phase exists:
- even a well-filled current design still has a structural commit cadence ceiling

Required outputs:
- chosen pipelining model
- safety and recovery story
- interface with dissemination batches and catch-up path

Benchmark gate:
- clear throughput gain over the best non-pipelined baseline
- no regression in convergence or validator stability

Exit criteria:
- committed throughput is no longer tightly coupled to one-block-per-round behavior

## Phase 6: Execution / Storage Parallelization

Goal:
- unlock the true `250k`-class architecture path

Why this phase exists:
- current execution and storage shape is still largely single-lane and not conflict-aware

Target architecture:
- parallel conflict-aware execution
- concurrency-friendly state/storage design
- role separation between ingress, validators, execution workers, and read nodes

Benchmark gate:
- materially higher throughput on realistic mixed workloads
- stable tail latency and recovery behavior

Exit criteria:
- the system is structurally capable of scaling beyond the current single-lane execution ceiling

## Sequential Working Mode

We will work sequentially like this:

1. Confirm current phase.
2. Write the exact hypothesis for the next change.
3. Change only the files needed for that hypothesis.
4. Run the benchmark.
5. Mark the result:
   - kept
   - binned
   - blocked
6. Update the decision log.
7. Only then move to the next hypothesis.

## What We Should Do Next

Immediate next phase:
- Phase 1: Control Plane Stabilization

Immediate next task:
- isolate the source of peer-transport `NonMonotonicTimestamp`, `InvalidHeight`, and quorum-loss behavior on the local 4-validator harness

Immediate non-goals:
- more harness scheduler tuning
- broad consensus redesign
- unrelated service cleanup
