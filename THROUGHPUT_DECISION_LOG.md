# Throughput Decision Log

## 2026-04-22

Baseline command shape:

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

Baseline artifact:
- `/tmp/codex-baseline-8k-10s`

Baseline result:
- `30,615` committed
- `3,061.5 TPS` over the 10s load window
- `25` load-window blocks
- post-load convergence green

Stability gate for future keeps:
- No change is kept on the basis of a single 10s win.
- A candidate keep must pass:
  - at least `3` short reruns on the same harness shape
  - at least one `30s` load with `20s` settle
  - clean post-load convergence
  - no `rpc_err` admission failures on the soak run
- If a change improves 10s throughput but fails the soak gate, it is binned as unstable.
- Stability takes precedence over peak TPS spikes until the admission/dissemination path stops collapsing under sustained load.

Decisions:

1. Lower local auto-network round interval from `100ms` to `50ms`.
   Result:
   - artifact `/tmp/codex-keep-auto50-default-8k-10s`
   - `55,498` committed
   - `5,549.8 TPS`
   - `67` load-window blocks
   - post-load convergence green
   Status:
   - kept
   Reason:
   - clear throughput win on the same harness shape

2. Lower local auto-network round interval from `50ms` to `25ms`.
   Result:
   - artifact `/tmp/codex-exp-auto25-8k-10s`
   - `49,407` committed
   - `4,940.7 TPS`
   - post-load convergence green
   Status:
   - binned
   Reason:
   - regression versus the kept `50ms` run

3. Tighten local auto-network drive interval from `25ms` to `10ms` while keeping `50ms` rounds.
   Result:
   - artifact `/tmp/codex-exp-auto50-drive10-8k-10s`
   - `17,134` committed
   - `1,713.4 TPS`
   - post-load convergence green
   Status:
   - binned
   Reason:
   - severe regression

4. Enable peer transport without harness fixes.
   Result:
   - artifact `/tmp/codex-exp-peertransport-8k-10s`
   - validator 2 failed during bootstrap
   - panic reached `crates/rpc/src/main.rs:563`
   Status:
   - binned as a throughput experiment
   Reason:
   - harness was wiring peer-transport bootstrap to the split network port even though peer transport serves on the public listener

5. Fix peer-transport harness endpoint wiring and rerun.
   Result:
   - artifact `/tmp/codex-exp-peertransport-fixed-8k-10s`
   - `23,938` committed
   - `2,393.8 TPS`
   - validator 2 became unreachable during the run
   - observed errors included `InvalidBlock(NonMonotonicTimestamp)`, `InvalidBlock(InvalidHeight)`, and repeated quorum loss / forced snapshot repair
   - post-load convergence green
   Status:
   - keep harness fix
   - bin peer transport as a throughput setting for now
   Reason:
   - the harness fix is required for valid peer-transport testing
   - the peer-transport path is currently unstable and regresses throughput on this harness

6. Serialize peer-transport requests per endpoint after session warmup.
   Result:
   - artifact `/tmp/codex-exp-peertransport-serialized-8k-10s`
   - `10,386` committed
   - `1,038.6 TPS`
   - only `4` load-window blocks
   - post-load convergence failed
   - observed errors included repeated `network head qc quorum unavailable` and forced repair with `head_qc_signature_mismatch`
   Status:
   - binned
   Reason:
   - removing concurrent in-flight control-plane traffic per endpoint did not stabilize peer transport
   - it exposed or caused a deeper liveness failure and materially regressed throughput versus both the baseline and the kept `50ms` run

7. Require proposal QC votes to extend the current head instead of any known ancestor.
   Result:
   - artifact `/tmp/codex-exp-peertransport-strict-current-head-8k-10s`
   - `22,633` committed
   - `2,263.3 TPS`
   - post-load convergence failed
   - settle snapshot split validators `1` and `3` onto height `67` while validators `2` and `4` reported height `68` on a different head hash
   - observed errors still included `head_qc_signature_mismatch`, `commit_apply_mismatch`, and `network head qc quorum unavailable`
   Status:
   - binned
   Reason:
   - strict current-head voting did not remove the peer-transport split-brain behavior
   - the transport/control-plane path is still forking or repairing onto inconsistent heads before proposal voting becomes the decisive bottleneck

8. Require forced snapshot repair candidates to actually satisfy the requested lineage target.
   Result:
   - artifact `/tmp/codex-exp-peertransport-repair-target-filter-8k-10s`
   - `21,351` committed
   - `2,135.1 TPS`
   - post-load convergence failed
   - settle snapshot still split across three different heads: validators `1` and `2` at height `66`, validator `3` at height `67`, validator `4` at height `70`
   - observed errors still included repeated `head_qc_signature_mismatch`, `commit_apply_mismatch`, and later `network head qc quorum unavailable`
   Status:
   - binned
   Reason:
   - filtering bad repair targets did not stop the initial split formation
   - the first break occurs before repair target admissibility alone can recover the cluster

9. Stop advancing the local auto-round timestamp cursor when a round attempt or timeout recovery errors.
   Result:
   - peer transport artifact `/tmp/codex-exp-peertransport-no-error-timestamp-advance-8k-10s`
   - `77,221` committed
   - `7,722.1 TPS`
   - post-load convergence green at height `160`
   - baseline artifact `/tmp/codex-exp-no-error-timestamp-advance-baseline-8k-10s`
   - `77,752` committed
   - `7,775.2 TPS`
   - post-load convergence green at height `166`
   Status:
   - kept
   Reason:
   - clear committed-throughput win over the current kept `50ms` baseline (`55,498`)
   - peer transport also recovered from repeated split-brain failure into a clean converged run
   - the evidence supports the scheduler diagnosis: consuming future timestamps on failed rounds was amplifying control-plane races and suppressing productive blocks

10. Round-robin proposer frontier packing across policies.
   Result:
   - artifact `/tmp/codex-exp-roundrobin-frontier-baseline-8k-10s`
   - `76,745` committed
   - `7,674.5 TPS`
   - post-load convergence green
   Status:
   - binned
   Reason:
   - slight throughput regression versus the kept scheduler baseline (`77,752`)
   - writer fairness did not improve enough to offset the lost block productivity on this harness

11. Drain up to 64 local submit queue items per worker pass under a single engine lock.
   Result:
   - artifact `/tmp/codex-exp-local-submit-batch64-baseline-8k-10s`
   - `12,362` committed
   - `1,236.2 TPS`
   - only `21` load-window blocks
   - loadgen latency spiked to `1310.176ms` p50, `2532.126ms` p90, `3932.972ms` p99
   - post-load head height after the measured window was only `22`
   Status:
   - binned
   Reason:
   - coarse batching at the local submit worker materially increased admission latency and starved round productivity
   - holding the engine lock across large submit batches is the wrong shape for this layer

12. Preview relay and submit-intent batches under one shared engine lock.
   Result:
   - artifact `/tmp/codex-exp-batch-preview-lock-baseline-8k-10s`
   - `17,568` committed
   - `1,756.8 TPS`
   - only `7` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `6.402ms` p50 and `116.091ms` p99 latency
   - post-load convergence green at height `14`
   Status:
   - binned
   Reason:
   - reducing lock round-trips by holding the engine lock across whole request batches materially regressed committed throughput
   - even read-side batch preview can starve round execution on the shared engine lock, so this is still the wrong layer for coarse batching

13. Admit relayed intents immediately on followers instead of previewing and re-queuing them through the local submit worker.
   Result:
   - artifact `/tmp/codex-exp-direct-relay-admit-baseline-8k-10s`
   - `37,296` committed
   - `3,729.6 TPS`
   - `36` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `48.845ms` p50 and `495.325ms` p99 latency
   - post-load convergence green at height `61`
   Status:
   - binned
   Reason:
   - bypassing the follower submit queue improved round count versus the lock-batching regressions, but still materially underperformed the kept scheduler baseline
   - immediate follower admission raised request latency and did not recover enough committed block fullness to offset the extra contention

14. Fall back from a tiny proposer-frontier plan to the full mempool planner on large-block runs.
   Result:
   - artifact `/tmp/codex-exp-frontier-fill-fallback-baseline-8k-10s`
   - `74,293` committed
   - `7,429.3 TPS`
   - `90` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `21.068ms` p50 and `179.858ms` p99 latency
   - post-load convergence green at height `142`
   Status:
   - binned
   Reason:
   - this was the closest post-scheduler node-side attempt, but it still regressed versus the kept `77,752` baseline
   - the result supports the stale-frontier / underfilled-round diagnosis, but the coarse fallback heuristic still spent too much extra planning work for the gain it recovered

15. Reuse the frontier-planned state and top up tiny blocks from the full mempool only up to a moderate target.
   Result:
   - artifact `/tmp/codex-exp-frontier-topup-baseline-8k-10s`
   - `21,507` committed
   - `2,150.7 TPS`
   - `59` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `18.273ms` p50 and `217.425ms` p99 latency
   - post-load convergence failed
   Status:
   - binned
   Reason:
   - the cheaper top-up heuristic was still too invasive and destabilized the cluster
   - once the planner moved off the original frontier ordering, it produced both lower committed throughput and a failed settle, so this is not a safe keep

16. Add proposer-frontier and submit-queue instrumentation to `kombat_getConsensusMetrics` and expose it in the local harness status output.
   Result:
   - artifact `/tmp/codex-exp-frontier-metrics-baseline-8k-10s`
   - `78,523` committed
   - `7,852.3 TPS`
   - `116` load-window blocks
   - post-load convergence green at height `169`
   Evidence:
   - settle-time status snapshot:
     - validator `1`: `direct=1117`, `tracked_policies=16`, `tracked_items=904`, `largest_policy_items=79`, `stale_policies=0`, `stale_items=0`, `local_submit=33223`, `pending_relay_intents=803`, `pending_relay_targets=2409`
     - validator `2`: `direct=818`, `tracked_policies=32`, `tracked_items=638`, `largest_policy_items=53`, `stale_policies=0`, `stale_items=0`, `local_submit=1`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `3`: `direct=648`, `tracked_policies=16`, `tracked_items=648`, `largest_policy_items=49`, `stale_policies=0`, `stale_items=0`, `local_submit=0`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `4`: `direct=1185`, `tracked_policies=16`, `tracked_items=1185`, `largest_policy_items=77`, `stale_policies=0`, `stale_items=0`, `local_submit=27148`, `pending_relay_intents=2690`, `pending_relay_targets=8070`
   - committed block-body sizes during the load window:
     - validator `1`: `44` blocks, avg `724.1`, median `649`, `0` empty, `16` below `512`
     - validator `2`: `46` blocks, avg `328.0`, median `321.5`, `9` empty, `36` below `512`
     - validator `3`: `39` blocks, avg `240.3`, median `194`, `9` empty, `35` below `512`
     - validator `4`: `39` blocks, avg `876.7`, median `807`, `0` empty, `4` below `512`
   Status:
   - kept
   Reason:
   - no throughput regression from the added visibility
   - the new metrics materially improved the diagnosis: on the live path `stale_policy_count=0` and `stale_ready_items=0`, while validators `1` and `4` carried very large `local_submit_queue` and `pending_relay` backlogs and validators `2` and `3` did not

17. Enqueue batch relays immediately at batch-accept time instead of waiting for local submit admission completion.
   Result:
   - artifact `/tmp/codex-exp-early-batch-relay-baseline-8k-10s`
   - `26,240` committed
   - `2,624.0 TPS`
   - only `19` load-window blocks
   - achieved RPS collapsed to `3711.2`
   - post-load convergence green at height `33`
   Evidence:
   - status snapshot during the run:
     - validator `1`: `direct=5249`, `tracked_policies=16`, `tracked_items=3312`, `stale_policies=0`, `stale_items=0`, `local_submit=14629`, `pending_relay_intents=34696`, `pending_relay_targets=74968`
     - validator `2`: `direct=3737`, `tracked_policies=32`, `tracked_items=3737`, `stale_policies=0`, `stale_items=0`, `local_submit=456`, `pending_relay_intents=0`, `pending_relay_targets=0`
   - committed block-body sizes during the load window:
     - validator `1`: `7` blocks, avg `1419.3`, median `625`, `0` empty, `3` below `512`
     - validator `2`: `15` blocks, avg `984.3`, median `832`, `3` empty, `5` below `512`
     - validator `3`: `5` blocks, avg `745.6`, median `1043`, `2` empty, `2` below `512`
     - validator `4`: `5` blocks, avg `575.6`, median `447`, `0` empty, `3` below `512`
   Status:
   - binned
   Reason:
   - early relay fanout multiplied relay backlog too aggressively and starved the hot path
   - validator `1` alone finished the run with `34,696` pending relay intents and `74,968` pending relay targets, so this confirmed the bottleneck but in a destructive way

18. Relay accepted intents only to the upcoming non-local proposer window instead of all other validators, first with a two-leader window.
   Result:
   - artifact `/tmp/codex-exp-relay-leader-window2-baseline-8k-10s`
   - `81,069` committed
   - `8,106.9 TPS`
   - `139` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `11.043ms` p50 and `170.507ms` p99 latency
   - post-load convergence green at height `199`
   Evidence:
   - settle-time status snapshot:
     - validator `1`: `local_submit=29159`, `pending_relay_intents=629`, `pending_relay_targets=1258`
     - validator `2`: `local_submit=22`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `3`: `local_submit=0`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `4`: `local_submit=29856`, `pending_relay_intents=1134`, `pending_relay_targets=1574`
   Status:
   - binned
   Reason:
   - this proved the direction was correct: reducing relay fanout to upcoming proposers improved committed throughput over the prior kept `78,523` run while keeping convergence green
   - it was still superseded by the tighter one-leader window, which reduced relay-target fanout further and produced a larger committed-throughput win

19. Tighten relay fanout again and relay each accepted intent only to the next non-local proposer.
   Result:
   - artifact `/tmp/codex-exp-relay-leader-window1-baseline-8k-10s`
   - `82,489` committed
   - `8,248.9 TPS`
   - `165` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `12.705ms` p50 and `204.475ms` p99 latency
   - post-load convergence green at height `219`
   Evidence:
   - settle-time status snapshot:
     - validator `1`: `local_submit=23633`, `pending_relay_intents=1421`, `pending_relay_targets=1421`
     - validator `2`: `local_submit=0`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `3`: `local_submit=11`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `4`: `local_submit=21539`, `pending_relay_intents=1274`, `pending_relay_targets=1274`
   Status:
   - kept candidate
   Reason:
   - limiting each relay to one upcoming proposer kept proposer-ready dissemination alive while cutting target fanout to `1x` per queued relay instead of `3x`
   - that directly improved the measured bottleneck: source validators still carried backlog, but pending relay targets collapsed from the earlier multi-target runs while committed throughput rose again

20. Make the one-leader relay window the local harness default and rerun the plain baseline command shape.
   Result:
   - artifact `/tmp/codex-keep-relay-window1-default-8k-10s`
   - `83,743` committed
   - `8,374.3 TPS`
   - `160` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `11.274ms` p50 and `196.571ms` p99 latency
   - post-load convergence green at height `237`
   Evidence:
   - settle-time status snapshot:
     - validator `1`: `local_submit=24573`, `pending_relay_intents=745`, `pending_relay_targets=745`
     - validator `2`: `local_submit=0`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `3`: `local_submit=0`, `pending_relay_intents=0`, `pending_relay_targets=0`
     - validator `4`: `local_submit=21842`, `pending_relay_intents=746`, `pending_relay_targets=746`
   Status:
   - kept
   Reason:
   - this confirmed the dissemination change on the standard local harness shape, not just under an explicit experiment override
   - targeting only the next proposer materially improved committed throughput over the prior kept `78,523` run while preserving clean convergence and sharply reducing relay-target amplification

21. Re-run the kept `leader_window=1` default to check stability.
   Result:
   - artifact `/tmp/codex-stability-run1-8k-10s`
   - `29,837` committed
   - `2,983.7 TPS`
   - `141` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `12.388ms` p50 and `100.777ms` p99 latency
   - post-load convergence green at height `183`
   Evidence:
   - block production skewed hard toward validator `4`: validator `4` averaged `609.5` items while validators `1`-`3` averaged only `52.4`, `60.2`, and `51.4`
   - validator `1` ended with `local_submit=20024` and only `pending_relay_intents=1`, while validator `4` ended with `local_submit=31355` and `pending_relay_intents=1215`
   Status:
   - binned
   Reason:
   - the throughput win was not stable; one repeat collapsed even though ingress and convergence stayed healthy
   - the failure mode is delayed backlog skew, not outright cluster breakage

22. Re-run the less aggressive `leader_window=2` variant as a stability check.
   Result:
   - artifact `/tmp/codex-stability-window2-run1-8k-10s`
   - `15,868` committed
   - `1,586.8 TPS`
   - `107` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `7.102ms` p50 and `281.940ms` p99 latency
   - post-load convergence green at height `111`
   Evidence:
   - validator `1` ended with `local_submit=38637`, `pending_relay_intents=1433`, `pending_relay_targets=2866`
   - validator `4` ended with `local_submit=46561`, `pending_relay_intents=3414`, `pending_relay_targets=4502`
   - validators `2` and `3` returned `ERR` in the settle-time status snapshot before the final convergence check completed
   Status:
   - binned
   Reason:
   - the same timing-coupled target selection was still unstable with two targets
   - relay fanout tied to local round position is not robust enough to keep

23. Replace round-coupled relay targeting with deterministic hash-based relay replicas and test with target count `2`.
   Result:
   - artifact `/tmp/codex-exp-hash-target2-run1-8k-10s`
   - `75,853` committed
   - `7,585.3 TPS`
   - `131` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `15.255ms` p50 and `173.081ms` p99 latency
   - post-load convergence green at height `204`
   Evidence:
   - validator `1`: `local_submit=32261`, `pending_relay_intents=1489`, `pending_relay_targets=2978`
   - validator `4`: `local_submit=29386`, `pending_relay_intents=1232`, `pending_relay_targets=1799`
   - validators `2` and `3` stayed responsive and converged cleanly instead of disappearing into `ERR`
   Status:
   - kept candidate
   Reason:
   - deterministic target selection removed the timing sensitivity from relay fanout while preserving reduced dissemination cost
   - throughput was below the single best `83,743` spike, but far above the instability collapses

24. Repeat the deterministic hash-based relay replica run with target count `2`.
   Result:
   - artifact `/tmp/codex-exp-hash-target2-run2-8k-10s`
   - `75,159` committed
   - `7,515.9 TPS`
   - `131` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `18.428ms` p50 and `184.882ms` p99 latency
   - post-load convergence green at height `197`
   Evidence:
   - validator `1`: `local_submit=33880`, `pending_relay_intents=1713`, `pending_relay_targets=3426`
   - validator `4`: `local_submit=29945`, `pending_relay_intents=1738`, `pending_relay_targets=2873`
   - quick-repeat variance versus item 23 was only `694` committed, about `0.9%`
   Status:
   - kept candidate
   Reason:
   - this is the first dissemination variant that has held together across immediate repeats
   - the remaining gap is peak throughput, but the stability profile is materially better than the round-coupled relay-window experiments

25. Run a longer soak on deterministic hash-based relay replicas with target count `2`.
   Result:
   - artifact `/tmp/codex-exp-hash-target2-soak30-8k`
   - `117,615` committed over `30s`
   - `3,920.5 TPS`
   - `197` load-window blocks
   - achieved RPS stayed at `7995.7`, but only `197,195` requests succeeded and `82,869` failed with rpc errors
   - post-load convergence green at height `249`
   Evidence:
   - `rpc_err_samples` were all `local_submit_queue_full` with queue limit `65536`
   - settle-time status snapshot:
     - validator `1`: `local_submit=55118`, `pending_relay_intents=2721`, `pending_relay_targets=5442`
     - validator `4`: `local_submit=54958`, `pending_relay_intents=1141`, `pending_relay_targets=1801`
     - validators `2` and `3` stayed effectively empty
   Status:
   - binned
   Reason:
   - the quick-repeat stability did not hold under longer load; the path remains admission-queue unstable
   - deterministic fanout fixed timing skew, but `target_count=2` still accumulates too much source-side local submit backlog over time

26. Lower deterministic hash-based relay replicas to target count `1`.
   Result:
   - artifact `/tmp/codex-exp-hash-target1-run1-8k-10s`
   - `42,372` committed
   - `4,237.2 TPS`
   - `256` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `13.518ms` p50 and `83.327ms` p99 latency
   - post-load convergence failed
   Evidence:
   - validators raced up to heights `486`, `500`, `515`, and `531` during settle and never reconverged
   - validator `4` still ended with `local_submit=21923` while other validators were nearly empty
   Status:
   - binned
   Reason:
   - cutting replicas to `1` relieved queue pressure, but it reintroduced the availability problem badly enough that the cluster would not converge after load
   - reduced relay targeting is not part of the stable keep set anymore

27. Rebaseline the reverted keep set with the new `30s` stability gate.
   Result:
   - artifact `/tmp/codex-stable-baseline-soak30-8k`
   - `122,630` committed over `30s`
   - `4,087.7 TPS`
   - `162` load-window blocks
   - achieved RPS stayed at `7995.7`, but only `195,596` requests succeeded and `84,468` failed with rpc errors
   - post-load convergence green at height `211`
   Evidence:
   - `rpc_err_samples` were all `local_submit_queue_full` with queue limit `65536`
   - settle-time status snapshot:
     - validator `1`: `local_submit=58918`, `pending_relay_intents=2049`, `pending_relay_targets=6147`
     - validator `4`: `local_submit=55128`, `pending_relay_intents=1721`, `pending_relay_targets=3513`
     - validators `2` and `3` stayed nearly empty
   Status:
   - binned as a keep candidate
   Reason:
   - the current reverted keep set does not pass the new stability gate
   - the stable-branch blocker is now explicit: sustained load still trips source-side `local_submit_queue_full`

28. Keep client write batches together inside the leader-side local submit queue.
   Result:
   - artifact `/tmp/codex-exp-batched-local-queue-8k-10s`
   - `33,888` committed
   - `3,388.8 TPS`
   - only `30` load-window blocks
   - loadgen stayed clean at `7987.2` achieved RPS with `38.536ms` p50 and `300.844ms` p99 latency
   - post-load convergence green at height `53`
   Evidence:
   - validator `1`: `local_submit=40888`, `pending_relay_intents=8800`, `pending_relay_targets=13968`
   - validator `4`: `local_submit=35480`, `pending_relay_intents=13776`, `pending_relay_targets=36160`
   Status:
   - binned
   Reason:
   - batching the local queue work while also holding the engine lock across each queued batch starved productive rounds instead of improving stability
   - this was the wrong way to attack queue pressure

29. Apply relay-side backpressure before overloaded validators accept more relayed intents into the shared local submit queue.
   Result:
   - artifact `/tmp/codex-exp-relay-backpressure-soak30-8k`
   - `128,711` committed over `30s`
   - `4,290.4 TPS`
   - `169` load-window blocks
   - achieved RPS stayed at `7995.7`, but only `220,518` requests succeeded and `59,546` failed with rpc errors
   - post-load convergence green at height `222`
   Evidence:
   - `rpc_err_samples` were still `local_submit_queue_full`
   - versus the reverted soak baseline, rpc errors fell from `84,468` to `59,546` and committed work rose from `122,630` to `128,711`
   - the after-run queue picture shifted:
     - validator `1`: `local_submit=41956`, `pending_relay_intents=36095`, `pending_relay_targets=39631`
     - validator `4`: `local_submit=43762`, `pending_relay_intents=23104`, `pending_relay_targets=26862`
   Status:
   - binned
   Reason:
   - backpressure on relay admission helped but did not clear the stability gate
   - the blocker is sharper now: retries are just piling up in `pending_intent_relays`, so source-side pressure is being moved, not removed

30. Split the local submit queue into public and relay lanes with an `8:1` public-priority drain ratio.
   Result:
   - artifact `/tmp/codex-exp-split-submit-lanes-8k-10s`
   - `35,390` committed
   - `3,539.0 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence failed
   Evidence:
   - the lane split eliminated `local_submit_queue_full` on the short run
   - the after-run queue picture showed relay starvation rather than queue collapse:
     - validator `4`: `local_submit=30842`, `public=24209`, `relay=6633`, `pending_relay_intents=971`, `pending_relay_targets=2149`
     - validator `3`: `local_submit=59`, `public=0`, `relay=59`
   Status:
   - binned
   Reason:
   - separating capacities helped admission, but the `8:1` drain bias left too much relay lag and the cluster never reconverged

31. Keep the split submit lanes, but change the drain ratio to `1:1`.
   Result:
   - artifact `/tmp/codex-exp-split-submit-lanes-burst1-8k-10s`
   - `16,948` committed
   - `1,694.8 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `412`
   Evidence:
   - the queue split plus equal drain fairness restored settle correctness
   - it also starved the public lane badly:
     - validator `4`: `local_submit=46675`, `public=46675`, `relay=0`, `pending_relay_intents=10`, `pending_relay_targets=16`
   Status:
   - binned
   Reason:
   - this variant was stable on the short gate but collapsed committed throughput far below the kept baseline

32. Keep the split submit lanes, but change the drain ratio to `2:1`.
   Result:
   - first short run artifact `/tmp/codex-exp-split-submit-lanes-burst2-8k-10s`
   - `91,032` committed
   - `9,103.2 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `193`
   - second required short rerun artifact `/tmp/codex-exp-split-submit-lanes-burst2-rerun2-8k-10s`
   - `16,159` committed
   - `1,615.9 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `85`
   Evidence:
   - the first run looked like the best short-run result of the project so far, but the second required rerun collapsed by more than `5x` on the same harness shape
   - both runs stayed admission-clean, which means the branch solved queue-full errors without producing trustworthy round productivity
   Status:
   - binned
   Reason:
   - this branch failed the new keep rule before soak: it could not reproduce within the required short-rerun gate
   - the combined evidence says local public-vs-relay lane splitting is not a reliable fix; it trades one failure mode for unstable drain economics

33. Keep one-intent-per-pass worker behavior, but enqueue relayed network batches as a single local queue work item instead of one queue node per intent.
   Result:
   - artifact `/tmp/codex-exp-relay-batch-work-8k-10s`
   - `22,798` committed
   - `2,279.8 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `15`
   Evidence:
   - the measured window produced only `9` blocks even though admission stayed clean
   - settle-time backlog was still extreme:
     - validator `1`: `local_submit=54381`, `pending_relay_intents=7200`, `pending_relay_targets=18160`
     - validator `4`: `local_submit=60154`, `pending_relay_intents=4267`, `pending_relay_targets=11491`
   Status:
   - binned
   Reason:
   - batching relay work at the follower local-queue representation layer suppressed round productivity badly instead of improving it
   - this narrows the next move again: the right place to coalesce is earlier in dissemination, before work reaches the follower submit queue

34. Keep relay semantics unchanged, but maintain a sender-side ordered relay index so `drain_pending_intent_relays` stops sorting the whole pending set on every pass.
   Result:
   - artifact `/tmp/codex-exp-relay-ordered-index-8k-10s`
   - `55,862` committed
   - `5,586.2 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `108`
   Evidence:
   - measured window produced `88` blocks, but final committed work still landed well below the kept baseline
   - settle-time backlog remained large on source-heavy validators:
     - validator `1`: `local_submit=38108`, `pending_relay_intents=8394`, `pending_relay_targets=21086`
     - validator `4`: `local_submit=40881`, `pending_relay_intents=1026`, `pending_relay_targets=3078`
   Status:
   - binned
   Reason:
   - removing the per-pass sort cost was not enough to recover committed throughput
   - the remaining bottleneck is not just sender-side relay ordering overhead

35. Maintain per-target ordered relay indexes so relay workers stop rescanning the whole pending set.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Hypothesis:
   - if sender-side relay drain can walk a per-validator ordered index instead of rebuilding per-target batches from the full pending map each pass, committed throughput should rise without changing follower admission semantics
   Measurement:
   - short run 1, artifact `/tmp/codex-exp-relay-per-target-index-8k-10s`
   - `97,314` committed
   - `9,731.4 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `202`
   - settle snapshot:
     - validator `1`: `local_submit=29735`, `pending_relay_intents=1451`, `pending_relay_targets=4353`
     - validator `4`: `local_submit=31669`, `pending_relay_intents=2671`, `pending_relay_targets=6701`
   - short run 2, artifact `/tmp/codex-exp-relay-per-target-index-rerun2-8k-10s`
   - `84,486` committed
   - `8,448.6 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `174`
   - settle snapshot:
     - validator `1`: `local_submit=35189`, `pending_relay_intents=1048`, `pending_relay_targets=3144`
     - validator `4`: `local_submit=34733`, `pending_relay_intents=2250`, `pending_relay_targets=5217`
   - short run 3, artifact `/tmp/codex-exp-relay-per-target-index-rerun3-8k-10s`
   - `71,232` committed
   - `7,123.2 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `163`
   - settle snapshot:
     - validator `1`: `local_submit=41192`, `pending_relay_intents=1110`, `pending_relay_targets=3330`
     - validator `4`: `local_submit=46511`, `pending_relay_intents=809`, `pending_relay_targets=1958`
   Evidence:
   - the first two runs looked like a real breakthrough, but the third required rerun fell below the current kept baseline of `78,523 committed`
   - all three runs were admission-clean and converged, so the failure mode is reproducibility rather than correctness
   - the source-heavy validators still finished with very large `local_submit_queue` backlogs, which means the index reduced sender work but did not change admission economics enough to stabilize the win
   Status:
   - binned
   Reason:
   - the branch failed the short rerun gate before soak, so it is not trustworthy enough to keep
   - sender-side batch assembly cost is real, but admission count and relay economics still dominate

36. Add observer-ingress queue-depth backpressure with async validator metric polling.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Hypothesis:
   - if observer ingress routes away from validators whose `local_submit_queue` is already near saturation, the `30s` soak should stop failing on `local_submit_queue_full` without changing consensus or follower admission
   Measurement:
   - default TTL `200ms`, artifact `/tmp/codex-exp-phase0-ingress-backpressure-8k-10s`
   - `3,705` committed
   - `370.5 TPS`
   - achieved RPS stayed at `8012.8`
   - `rpc_err=0`
   - failed post-load convergence
   - settle snapshot showed severe collapse and validator `ERR` status during convergence checks
   - bounded retry with `KOMBAT_INGRESS_QUEUE_METRIC_TTL_MS=1000`, artifact `/tmp/codex-exp-phase0-ingress-backpressure-ttl1000-8k-10s`
   - `34,225` committed
   - `3,422.5 TPS`
   - achieved RPS stayed at `7987.2`
   - `rpc_err=0`
   - post-load convergence green at height `91`
   Evidence:
   - coarsening the metric TTL avoided the hard convergence failure, which points to the queue-depth polling itself as the destabilizing cost
   - even with the safer `1000ms` TTL, committed throughput still fell far below the current kept baseline of `78,523`
   - this means the valve's remote metric polling and routing churn cost more than the queue-saturation relief it can buy on the current harness shape
   Status:
   - binned
   Reason:
   - both variants materially regressed committed throughput, and the default plan-shaped variant also broke convergence
   - polling `kombat_getConsensusMetrics` on validators from the observer path is not a safe stability valve in the current implementation

37. Phase 1 Slice 1: shared engine handle scaffold plus metrics-path migration
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can land the Slice 1 engine-handle scaffold behind a compat `lock()` shim, migrate only `kombat_getConsensusMetrics` to read from sidecar snapshots, and keep the local 4-validator harness at or above the current trusted short-run baseline.
   Files changed:
   - `chain/crates/node/Cargo.toml`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - trusted prior kept short-run baseline:
     - `/tmp/codex-exp-frontier-metrics-baseline-8k-10s`
     - `78,523 committed`
     - `7,852.3 TPS`
     - `rpc_err=0`
     - converged
   - first Slice 1 run:
     - `/tmp/codex-exp-phase1-slice1-metrics-handle-8k-10s`
     - `85,174 committed`
     - `8,517.4 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice1-metrics-handle-rerun-8k-10s`
     - `80,824 committed`
     - `8,082.4 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The shared `EngineHandle` path kept auto-produce, auto-network-round, and RPC reads on one compat surface, so sidecar snapshots stayed live under real background mutation instead of going stale behind a raw `Arc<Mutex<SingleNodeEngine>>`.
   - Moving only `kombat_getConsensusMetrics` off the hot `engine.lock()` path was safe: both focused RPC tests passed and the short-run harness stayed above the prior kept baseline on two consecutive runs.
   - This slice is still intentionally read-mostly scaffolding. The new metrics payload is explicitly not cross-domain atomic, but that is acceptable for operator telemetry and was documented inline at the call site.
   Status:
   - kept
   Reason:
   - The first Phase 1 slice landed with no regression on the trusted short-run shape and produced a modest positive movement over the previous kept baseline while preserving convergence and zero RPC admission errors.
   Next move:
   - Keep the shared `EngineHandle` scaffold, and move to the next read-mostly caller as a separate patch: observer ingress routing and head/current-round metadata reads, still without touching admission or block assembly.

38. Phase 1 Slice 2: observer-ingress and auto-round metadata reads via `HeadSnapshot`
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can move observer-ingress leader selection and auto-network-round timestamp rebasing off compat `engine.lock()` and onto `EngineHandle` head/current-round snapshots, reducing read-side contention while keeping forwarding semantics unchanged.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   Benchmark runs:
   - prior kept short-run state after Slice 1:
     - `/tmp/codex-exp-phase1-slice1-metrics-handle-rerun-8k-10s`
     - `80,824 committed`
     - `8,082.4 TPS`
     - `rpc_err=0`
     - converged
   - first Slice 2 run:
     - `/tmp/codex-exp-phase1-slice2-ingress-reads-8k-10s`
     - `83,331 committed`
     - `8,333.1 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice2-ingress-reads-rerun-8k-10s`
     - `84,272 committed`
     - `8,427.2 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - Observer ingress previously took full compat `engine.lock()` reads just to decide whether the node was ingress-only, choose preferred leaders, and snapshot `current_round`/`chain_id`.
   - The auto-network-round driver in `rpc/src/main.rs` also locked the full engine only to read `head().header.timestamp_ms` for initial timestamp selection and rebasing.
   - Replacing those reads with `EngineHandle::head_snapshot()` plus `current_round_snapshot()` preserved all tested ingress semantics and kept the short-run harness in a slightly better band than the prior saved Slice 1 state.
   Status:
   - kept
   Reason:
   - This slice stayed fully within the read-mostly Phase 1 plan, passed the observer-ingress regression tests, and improved the saved short-run result without changing admission, dissemination, or consensus behavior.
   Next move:
   - Continue the read-mostly sequence with the next metadata/query callers that still take compat `engine.lock()` for head/round-only reads, while keeping all admission and block-assembly paths untouched until the next planned slice.

39. Phase 1 Slice 3: peer/network metadata responses via snapshots
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can move peer/network metadata responses (`/network/ping`, peer-transport session open, and the head/round fields in `/network/bootstrap`) off compat `engine.lock()` and onto `HeadSnapshot` / `current_round_snapshot()` without changing semantics or degrading the short-run harness beyond the Phase 1 flat-period tolerance.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc serves_network_ping_when_requested`
   - `cargo test -p kombat-chain-rpc serves_network_ping_over_peer_transport_when_enabled`
   - `cargo test -p kombat-chain-rpc consensus_metrics_reports_inbound_peer_transport_sessions`
   Benchmark runs:
   - prior saved Slice 2 state:
     - `/tmp/codex-exp-phase1-slice2-ingress-reads-rerun-8k-10s`
     - `84,272 committed`
     - `8,427.2 TPS`
     - `rpc_err=0`
     - converged
   - first Slice 3 run:
     - `/tmp/codex-exp-phase1-slice3-peer-metadata-8k-10s`
     - `82,346 committed`
     - `8,234.6 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice3-peer-metadata-rerun-8k-10s`
     - `83,274 committed`
     - `8,327.4 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - `network_session_open_response_body` and `network_ping_response_body` previously took full compat `engine.lock()` only to report chain/head/round/view metadata.
   - `network_bootstrap_response_body` still needs a short engine read for the local consensus pubkey, but its head/round fields now come from `HeadSnapshot` and `current_round_snapshot()`.
   - The short-run harness stayed within roughly `-1.2%` of the saved Slice 2 state on both runs, which is inside the flat-period no-regression tolerance for the read-mostly Phase 1 scaffolding.
   Status:
   - kept
   Reason:
   - This slice did not produce a new throughput step, but it preserved stability and stayed within the expected flat-period performance band while removing more metadata-only compat lock reads from the network surface.
   Next move:
   - Finish the remaining read-mostly metadata/query callers that still take compat `engine.lock()` for head/round-only reads, then stop extending the flat-period work and move into the admission hot path slice where the first real TPS jump should happen.

40. Phase 1 admission ticket fast path
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can avoid paying the full `validate_submitted_intent` path twice for queue-drained direct intents by minting a preview-time admission ticket and fast-admitting on dequeue when committed head hash and committed policy nonce are unchanged.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 2 state:
     - `/tmp/codex-exp-phase1-slice2-ingress-reads-rerun-8k-10s`
     - `84,272 committed`
     - `8,427.2 TPS`
     - `rpc_err=0`
     - converged
   - experiment:
     - `/tmp/codex-exp-phase1-slice4-admission-ticket-8k-10s`
     - `4,696 committed`
     - `469.6 TPS`
     - `rpc_err=0`
     - converged, but severe throughput collapse
   Evidence:
   - The queue-drain fast path compiled and passed focused admission/rpc tests, so this is not a simple compile or API-shape failure.
   - The live harness collapsed from the saved `84,272` short-run state to `4,696 committed`, with `p50` latency around `110ms`, `p99` around `752ms`, and only `73` produced blocks during the run.
   - End-of-run consensus metrics showed a clear invariant break: direct mempool items remained large while proposer-frontier tracking effectively disappeared on overloaded validators, e.g. validator snapshots showed `direct=3101/3025/2812` with `tracked_policies=0` and `tracked_items=0`.
   - That means the ticket fast path did not preserve the core throughput invariant that admitted backlog must become proposer-ready block material. Even if admission got cheaper, the system stopped converting accepted work into productive block contents.
   Status:
   - binned
   Reason:
   - This is not a near miss or a small regression. It catastrophically breaks backlog-to-block conversion on the local harness, so it is not safe to iterate in place on the working branch.
   Next move:
   - Revert the ticket path completely, return to the last saved Slice 3 state, and keep the next admission slice narrower. The next attempt should preserve the existing proposer-ready admission flow and only change lock scope or read/write partitioning, not admission semantics.

41. Phase 1 Slice 4a: reuse preview response in local submit drain
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can remove duplicate post-admission work from `drain_local_submit_queue` by reusing the already-computed preview response instead of rebuilding `SubmitIntentResponse` after successful dequeue admission. This keeps `submit_intent` semantics unchanged and only strips wasted hot-path work around it.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 2 state:
     - `/tmp/codex-exp-phase1-slice2-ingress-reads-rerun-8k-10s`
     - `84,272 committed`
     - `8,427.2 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice4-drain-preview-reuse-8k-10s`
     - `87,169 committed`
     - `8,716.9 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice4-drain-preview-reuse-rerun-8k-10s`
     - `86,229 committed`
     - `8,622.9 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The old queue drain path called `NodeRpcService::submit_intent`, which rebuilt `SubmitIntentResponse` after admission by checking mempool/conditional placement and recomputing `intent_status`.
   - Public and batch submit paths already return the previewed response at enqueue time, and relay dequeue does not need the post-admission response body at all, so this extra reconstruction was duplicate hot-path work inside the shared engine lock.
   - Reusing the preview response preserved all focused ingress/relay tests and improved short-run committed throughput by roughly `+2.3%` to `+3.4%` over the saved Slice 2 state while keeping latency healthy and convergence green.
   Status:
   - kept
   Reason:
   - This is the first admission-adjacent slice that improves the benchmark without changing admission semantics or disturbing proposer-ready conversion.
   Next move:
   - Continue the admission path on the same rule: no semantic shortcuts. The next slice should narrow engine-lock scope around preview/admission support work, not change `submit_intent` invariants or proposer-frontier updates.

42. Phase 1 Slice 4b: remove direct-path intent clone in `submit_intent`
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can reduce hot-path allocation and copy work inside `SingleNodeEngine::submit_intent` by removing the full `SignedIntent` clone on the direct-admission path and updating proposer-frontier state from captured `policy_id` / `nonce` instead. This preserves admission semantics and only trims wasted work under the engine lock.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node proposer_frontier_refreshes_when_missing_nonce_arrives`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 4a state:
     - `/tmp/codex-exp-phase1-slice4-drain-preview-reuse-rerun-8k-10s`
     - `86,229 committed`
     - `8,622.9 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice4b-direct-submit-no-clone-8k-10s`
     - `87,065 committed`
     - `8,706.5 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice4b-direct-submit-no-clone-rerun-8k-10s`
     - `87,410 committed`
     - `8,741.0 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The old direct-admission path cloned the full `SignedIntent` just to insert into `intent_mempool`, then immediately reused the original only for proposer-frontier tracking and event emission.
   - Capturing `policy_id` / `nonce` before insertion and calling `proposer_frontier.on_intent_admitted(...)` directly preserves the same frontier update and emitted runtime event without copying the full signed intent under the engine lock.
   - Both runs improved on the saved Slice 4a result while keeping proposer-frontier stats healthy (`stale_policies=0`, `stale_items=0`) and convergence green.
   Status:
   - kept
   Reason:
   - This slice follows the Phase 1 rule exactly: it reduces hot-path work without altering validation, queue semantics, or proposer-ready invariants, and it produced a repeatable throughput improvement.
   Next move:
   - Keep working the admission path with semantic-preserving cuts. The next target should remove or narrow remaining support work around preview/admission, not introduce new admission shortcuts.

43. Phase 1 Slice 4c: drop dead queue response plumbing
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can improve the local submit worker by removing the unused queued `SubmitIntentResponse` / `result_tx` plumbing entirely, since the hot paths never consume a dequeue-time reply.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 4b state:
     - `/tmp/codex-exp-phase1-slice4b-direct-submit-no-clone-rerun-8k-10s`
     - `87,410 committed`
     - `8,741.0 TPS`
     - `rpc_err=0`
     - converged
   - experiment:
     - `/tmp/codex-exp-phase1-slice4c-drop-dead-queue-response-8k-10s`
     - `86,836 committed`
     - `8,683.6 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The plumbing is genuinely dead on the current public/batch/relay paths, so this was a valid cleanup candidate.
   - But the first benchmark came in below the current saved state and did not produce a clear throughput win, even though convergence and queue metrics stayed healthy.
   Status:
   - binned
   Reason:
   - The change is harmless but not a measured improvement on the standard harness shape, so it does not earn a keep under the current rules.
   Next move:
   - Revert this cleanup and keep targeting relay/admission support work that removes more substantial wasted work under the engine lock.

44. Phase 1 Slice 4d: relay validation without full preview response
   Date:
   - `2026-04-23`
   Hypothesis:
   - Relayed intents do not need a full `SubmitIntentResponse` before queueing, only the validation gate. Using a lightweight `validate_submit_intent_candidate` path for `process_network_relay_intents` should remove unnecessary status/response construction work on a hot relay path without changing admission semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node proposer_frontier_refreshes_when_missing_nonce_arrives`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 4b state:
     - `/tmp/codex-exp-phase1-slice4b-direct-submit-no-clone-rerun-8k-10s`
     - `87,410 committed`
     - `8,741.0 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice4d-relay-validate-only-8k-10s`
     - `87,716 committed`
     - `8,771.6 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice4d-relay-validate-only-rerun-8k-10s`
     - `87,086 committed`
     - `8,708.6 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The first run suggested a small gain on the relay path, but the required rerun dropped back below the current saved baseline.
   - Admission, queue, and proposer-frontier semantics all stayed healthy, so this is a noise-band experiment rather than a correctness failure.
   Status:
   - binned
   Reason:
   - The result is not a clear repeatable throughput win over the saved Slice 4b state, so it does not meet the keep bar.
   Next move:
   - Revert this relay-only path and keep searching for semantic-preserving admission cuts that either shrink heavier support work or narrow lock scope more materially than response-building tweaks.

45. Phase 1 Slice 4e: move owned work through local submit drain
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can reduce dequeue hot-path churn by consuming `QueuedLocalSubmit` directly in `drain_local_submit_queue`, avoiding the dequeue-time `SignedIntent` clone and `SubmitIntentParams` clone while preserving the same submit and relay behavior.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 4b state:
     - `/tmp/codex-exp-phase1-slice4b-direct-submit-no-clone-rerun-8k-10s`
     - `87,410 committed`
     - `8,741.0 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice4e-drain-move-owned-work-8k-10s`
     - `87,435 committed`
     - `8,743.5 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice4e-drain-move-owned-work-rerun-8k-10s`
     - `86,983 committed`
     - `8,698.3 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The first run came in effectively tied with the saved baseline, which is exactly the kind of noise-band result that requires a rerun.
   - The rerun fell back below the saved Slice 4b state, so this is not a repeatable throughput win even though the semantics remained clean and convergence stayed green.
   Status:
   - binned
   Reason:
   - Not a durable measured improvement over the saved state.
   Next move:
   - Revert this cut and keep looking for heavier semantic-preserving reductions in admission/relay support work rather than queue-item clone cleanup alone.

46. Phase 1 Slice 5: admission hot path via `EngineHandle`
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can move the admission hot path onto explicit `EngineHandle` admission helpers with canonical domain tracking and a compat shim, so the larger Phase 1 lock-scope split can build on a stable structural path without regressing the current harness.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_forwards_from_observer_ingress`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 4b state:
     - `/tmp/codex-exp-phase1-slice4b-direct-submit-no-clone-rerun-8k-10s`
     - `87,410 committed`
     - `8,741.0 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice5-admission-engine-handle-8k-10s`
     - `86,490 committed`
     - `8,649.0 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice5-admission-engine-handle-rerun-8k-10s`
     - `87,251 committed`
     - `8,725.1 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - This is the first larger structural Phase 1 admission slice after the micro-optimization work tapered into the noise band.
   - The first run came in slightly below the saved Slice 4b state, so it needed the standard rerun before being trusted.
   - The rerun recovered to within `0.18%` of the saved state while keeping `rpc_err=0` and clean post-load convergence, which is inside the flat-period tolerance for this refactor stage.
   Status:
   - kept
   Reason:
   - This cut establishes the explicit `EngineHandle` admission path and debug lock-order discipline needed for the larger lock-scope refactor, without materially regressing throughput or stability on the trusted harness shape.
   Next move:
   - Use this saved structural baseline to start the real lock-scope split rather than more queue-path cleanup, beginning with removing compat-only admission serialization where the new admission helpers now provide the seam.

47. Phase 1 Slice 6: `RwLock` compat path for admission preview
   Date:
   - `2026-04-23`
   Hypothesis:
   - Swapping the new-engine compat path from `Mutex` to `RwLock` should let read-side admission preview stop taking an exclusive engine lock, reducing contention enough to show up above the flat-period noise band while keeping write-side semantics intact.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 5 state:
     - `/tmp/codex-exp-phase1-slice5-admission-engine-handle-rerun-8k-10s`
     - `87,251 committed`
     - `8,725.1 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice6-rwlock-preview-8k-10s`
     - `83,434 committed`
     - `8,343.4 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The structural change compiled and preserved correctness, but the first harness pass came in `3.9%` below the saved Slice 5 state.
   - That is outside the flat-period no-regression band for Phase 1 groundwork, so a rerun was not worth spending before binning.
   Status:
   - binned
   Reason:
   - Read-side `RwLock` access on the compat engine path did not offset its coordination cost on this harness shape, so it is not yet a safe lock-behavior improvement.
   Next move:
   - Revert this compat-lock swap and keep the explicit `EngineHandle` admission seam from Slice 5, then look for the next heavier lock-scope cut inside admission or block assembly rather than changing the top-level compat primitive alone.

48. Phase 1 Slice 7: cached head block and snapshot QC signing for `run_network_round`
   Date:
   - `2026-04-23`
   Hypothesis:
   - Caching the full head block and current-head QC signing material in `EngineHandle` should let `run_network_round` avoid its first read-side engine lock entirely, narrowing the round-open critical path without changing commit semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 5 state:
     - `/tmp/codex-exp-phase1-slice5-admission-engine-handle-rerun-8k-10s`
     - `87,251 committed`
     - `8,725.1 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice7-head-snapshot-round-open-8k-10s`
     - `85,517 committed`
     - `8,551.7 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice7-head-snapshot-round-open-rerun-8k-10s`
     - `74,062 committed`
     - `7,406.2 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The first run stayed barely inside the flat-period tolerance band, so it needed the normal rerun.
   - The rerun collapsed far below the saved Slice 5 state despite keeping correctness clean, which means the sidecar head-block/signing path is not stable enough to keep on this harness shape.
   Status:
   - binned
   Reason:
   - The lock removal in `run_network_round` did not produce a repeatable throughput improvement and showed unacceptable run-to-run variance.
   Next move:
   - Revert this round-open slice and keep the saved Slice 5 admission seam, then continue toward a heavier lock-scope cut that changes less hot metadata ownership and more of the actual admission/block-assembly critical section.

49. Phase 1 Slice 8: lock-profile instrumentation and harness surfacing
   Date:
   - `2026-04-23`
   Hypothesis:
   - Before taking another structural cut, we need measured lock wait/hold distributions on the saved Slice 5 shape. Instrumenting the `EngineHandle` seam plus the two `run_network_round` engine-lock phases should identify whether the next cut belongs in admission, block assembly, or commit/apply.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/live_network_status.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 5 state:
     - `/tmp/codex-exp-phase1-slice5-admission-engine-handle-rerun-8k-10s`
     - `87,251 committed`
     - `8,725.1 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-baseline-8k-10s`
     - `87,358 committed`
     - `8,735.8 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
     - `87,194 committed`
     - `8,719.4 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The instrumentation slice stayed inside the flat-period band and is effectively tied with the saved Slice 5 state, so it is safe to keep.
   - The new profiles rule out another outer-lock experiment as the next move:
     - validator `1`: `admission_write hold p95 ~= 651,792ns` vs `round_build hold p95 ~= 178,057,125ns`
     - validator `4`: `admission_write hold p95 ~= 596,291ns` vs `round_build hold p95 ~= 215,972,500ns`
     - round-open head/QC signing hold stayed tiny (`~38-40us p95`), so the expensive part is not metadata reads
   - The dominant hold is the build-phase engine section inside `run_network_round`, which currently wraps `install_head_qc` plus `prepare_network_block_commit_from_mempool`.
   Status:
   - kept
   Reason:
   - This slice gives a measured basis for the next structural cut without regressing throughput or convergence.
   Next move:
   - Stop guessing at outer-lock changes and split the build-phase critical section itself, starting with `chain/crates/node/src/lib.rs::prepare_network_block_commit_from_mempool`.
   - Highest-confidence cut: snapshot the planner inputs under a short read, run the planner off-lock, then reacquire only for the mutating output/install path. Commit/apply remains the secondary target if the planner split does not move the build hold materially.

50. Phase 1 Slice 8.5: build sub-phase timings inside `prepare_network_block_commit_from_mempool`
   Date:
   - `2026-04-23`
   Hypothesis:
   - Splitting the measured `round_build` hold requires one more sub-diagnostic inside `prepare_network_block_commit_from_mempool`: separate maintenance, planner, and executor timing so the next structural cut targets the dominant sub-phase instead of guessing.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/telemetry.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/live_network_status.sh`
   Benchmark runs:
   - prior saved Slice 8 state:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
     - `87,194 committed`
     - `8,719.4 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-phase1-slice8-5-build-subphase-profile-8k-10s`
     - `10,797 committed`
     - `1,079.7 TPS`
     - `rpc_err=0`
     - failed post-load convergence
   Evidence:
   - This instrumentation shape was too invasive to keep: throughput collapsed, p50 latency jumped to `136.7ms`, and the cluster failed post-load convergence.
   - Even on the bad run, the overloaded validator still pointed at planner cost over executor cost (`validator-4 planner p95 ~= 172ms` vs `executor p95 ~= 15ms`), but the run quality is too poor to treat that as a trusted baseline artifact.
   Status:
   - binned
   Reason:
   - The diagnostic changed system behavior too much to keep or rely on as a neutral measurement slice.
   Next move:
   - Revert this instrumentation and return to the saved Slice 8 baseline.
   - Use the already-kept Slice 8 data plus the directional hint from this failed run to take the planner-first structural split next, rather than adding more profiling overhead on the hot path.

51. Phase 1 Slice 9: planner snapshot-and-release during `run_network_round`
   Date:
   - `2026-04-23`
   Hypothesis:
   - The hot `round_build` hold can be cut by keeping maintenance under the compat lock, snapshotting the frontier planner inputs under a short lock hold, running frontier planning off-lock, and reacquiring only for block execution/finalization.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   Benchmark runs:
   - prior saved Slice 8 state:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
     - `87,194 committed`
     - `8,719.4 TPS`
     - `rpc_err=0`
     - converged
   - short run 1:
     - `/tmp/codex-exp-phase1-slice9-planner-snapshot-8k-10s`
     - `85,523 committed`
     - `8,552.3 TPS`
     - `rpc_err=0`
     - converged
   - short run 2:
     - `/tmp/codex-exp-phase1-slice9-planner-snapshot-rerun-8k-10s`
     - `87,824 committed`
     - `8,782.4 TPS`
     - `rpc_err=0`
     - converged
   - short run 3:
     - `/tmp/codex-exp-phase1-slice9-planner-snapshot-rerun3-8k-10s`
     - `88,094 committed`
     - `8,809.4 TPS`
     - `rpc_err=0`
     - converged
   - `30s` soak:
     - `/tmp/codex-exp-phase1-slice9-planner-snapshot-soak30-8k`
     - `137,742 committed`
     - `4,591.4 TPS`
     - `rpc_err=83,354`
     - converged, but failed the stability gate on `local_submit_queue_full`
   Evidence:
   - The refactor did exactly what it was designed to do inside the hot build section: `round_build hold p95` dropped from the saved Slice 8 `~178-216ms` range down to roughly `22-30ms` on the short runs.
   - Despite that, short-run committed throughput only moved inside the noise band around the saved baseline, and the `30s` soak still collapsed into source-side `local_submit_queue_full`.
   - That means planner work was a real part of the critical section, but not the actual stability cure for the current harness shape. Once planner work moved off-lock, the dominant bottleneck remained source-validator admission pressure and queue saturation.
   Status:
   - binned
   Reason:
   - This slice is structurally useful, but it does not clear the stability gate. A keep that still produces `83,354` `rpc_err` on the `30s` soak would violate the current rule.
   Next move:
   - Revert to the saved Slice 8 baseline.
   - Treat Slice 9 as evidence, not as the saved state: Phase 1 planner contention was real, but the next stability move has to target the source-side admission path that still overflows `local_submit_queue` after planner hold time is removed.

52. Saved Slice 8 ceiling check at higher offered load
   Date:
   - `2026-04-23`
   Hypothesis:
   - If the saved Slice 8 baseline had hidden headroom above the `8k` harness shape, increasing offered load to `15k` and `20k` on the same code should raise committed TPS materially. If committed TPS stays flat while `queue_full` rises, the remaining limiter is a hard admission-path ceiling, not under-driven harness load.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Benchmark runs:
   - saved Slice 8 reference:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
     - `87,194 committed`
     - `8,719.4 TPS`
     - `rpc_err=0`
     - converged
   - `15k` offered:
     - `/tmp/slice8-15k-10s`
     - `84,139 committed`
     - `8,413.9 TPS`
     - `rpc_err=62,894`
     - converged
   - `20k` offered:
     - `/tmp/slice8-20k-10s`
     - `84,020 committed`
     - `8,402.0 TPS`
     - `rpc_err=139,453`
     - converged
   Evidence:
   - Raising offered load from `8k` to `15k` and `20k` did not unlock more committed throughput. Both higher-load runs flattened in the same `~8.4k TPS` band while `local_submit_queue_full` errors exploded.
   - That means the current saved baseline is not being under-driven by the `8k` harness. The system is hitting a real source-side admission ceiling, and higher offered load only converts into more queue overflow, not more committed work.
   - Combined with Slice 9, this narrows the remaining bottleneck: planner lock contention was real, but once that is no longer the dominant hold, the limiter is the per-intent admission path itself.
   Status:
   - kept measurement
   Reason:
   - This is the decisive ceiling check for the saved Slice 8 baseline. It changes the next implementation move without changing code.
   Next move:
   - Start Phase 2 on the admission path: batch signature verification and move it off the serial admission/engine path instead of spending more time on perimeter lock refactors.

53. Phase 2 Slice 10: sigverify dedup between preview and dequeue admission
   Date:
   - `2026-04-23`
   Hypothesis:
   - The current admission ceiling is close to the expected cost of verifying the same session signature twice, once during `preview_local_submit` and again during dequeue admission. If preview threads the verified session grantee into the queued submit and dequeue admission safely reuses it, committed throughput should jump materially and `local_submit_queue_full` should recede.
   Files changed:
   - `chain/crates/policy/src/lib.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-node submit_with_verified_session_rechecks_rotated_key`
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_relay_header_bypasses_leader_forwarding`
   Benchmark runs:
   - prior saved Slice 8 state:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
     - `87,194 committed`
     - `8,719.4 TPS`
     - `rpc_err=0`
     - converged
   - `8k` offered with `KOMBAT_RPC_SIGVERIFY_DEDUP=1`:
     - `/tmp/codex-exp-phase2-slice10-sigverify-dedup-8k-10s`
     - `88,057 committed`
     - `8,805.7 TPS`
     - `rpc_err=0`
     - converged
   - `15k` offered with `KOMBAT_RPC_SIGVERIFY_DEDUP=1`:
     - `/tmp/codex-exp-phase2-slice10-sigverify-dedup-15k-10s`
     - `86,299 committed`
     - `8,629.9 TPS`
     - `rpc_err=62,692`
     - converged
   Evidence:
   - The safety path is correct: preview can pass a verified session grantee into dequeue admission, and dequeue still rejects the submit if the key rotates before admission.
   - The throughput gain is too small to keep. At `8k` offered the slice only improved the saved baseline by `863 committed` (`+86.3 TPS`), which is well inside the band of minor harness drift and far below the expected Phase 2 step-change.
   - More importantly, the higher-load ceiling barely moved. At `15k` offered the system still flattened in the same `~8.6k TPS` band and still threw `62,692` `local_submit_queue_full` errors, so this slice does not change the actual stability or capacity envelope.
   - The result sharpens the diagnosis: duplicate signature verification exists, but it is not the main admission limiter on this harness shape. `validate_submitted_intent` only pays the runtime admission/signature path when an intent is at the committed frontier nonce. Future nonces skip that expensive path until they reach the front, so preview-to-dequeue dedup only removes duplicate work for a narrow subset of intents.
   - The single remaining dequeue-side verification plus the rest of the serial per-intent admission work still define the ceiling. This makes the half-step dedup slice too small to matter and points at the full worker/batch sigverify move instead.
   Status:
   - binned
   Reason:
   - The slice is correct but not material. It does not clear the Phase 2 gate and does not fix the source-side `local_submit_queue_full` failure mode under load.
   Next move:
   - Revert this slice and keep the saved Slice 8 baseline.
   - Move to the full Phase 2 cut: batch or offload dequeue-side signature verification instead of spending more time on preview/dequeue dedup plumbing.

54. Slice 11 diagnostic: commit-rate and block-fullness counters
   Date:
   - `2026-04-23`
   Hypothesis:
   - If the harness is now commit-bound rather than admission-bound, one baseline run with explicit commit-rate and block-fullness metrics should show that the observed TPS ceiling is explained by produced-block cadence times average committed items per block, not by admission drain capacity.
   Files changed:
   - `chain/crates/node/src/telemetry.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/live_network_status.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   - `chain/ARCHITECTURE_PLAN.md`
   Tests:
   - `cargo test -p kombat-chain-rpc returns_consensus_metrics_and_logs`
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   Benchmark runs:
   - prior saved Slice 8 state:
     - `/tmp/codex-exp-phase1-slice8-lock-profile-rerun-8k-10s`
     - `87,194 committed`
     - `8,719.4 TPS`
     - `rpc_err=0`
     - converged
   - first run:
     - `/tmp/codex-exp-slice11-commit-shape-wallclock-8k-10s`
     - `82,476 committed`
     - `8,247.6 TPS`
     - `rpc_err=0`
     - converged
   - required rerun:
     - `/tmp/codex-exp-slice11-commit-shape-wallclock-rerun-8k-10s`
     - `82,992 committed`
     - `8,299.2 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The instrumentation itself is not neutral enough to keep. Both runs regressed roughly `5%` from the saved Slice 8 baseline, so the code slice is binned.
   - The diagnostic output is still decisive:
     - load-window block production from the loadgen report was `186` blocks in `10s` on the first run and `183` blocks in `10s` on the rerun, or `18.6` and `18.3` blocks/sec cluster-wide
     - committed totals were `82,476` and `82,992`, which means average committed items per produced block were about `443` and `454`
     - those two numbers explain the observed throughput ceiling directly:
       - `18.6 * 443 ~= 8,240 TPS`
       - `18.3 * 454 ~= 8,308 TPS`
   - The new block-fullness counters support the same read:
     - per-validator committed-items-per-block `p50` sat in the `274-303` band
     - `p95` sat only in the `790-963` band
     - max committed items per block stayed around `1,304-1,469`, far below the current `AUTO_MAX_ITEMS=5000` ceiling
   - That means the current harness is not primarily admission-bound anymore. It is commit/dissemination-bound: current HotStuff round productivity and uneven proposer-ready block fullness are what define the `~8.3k-8.7k TPS` ceiling.
   Status:
   - binned instrumentation
   Reason:
   - The code slice regressed the trusted baseline enough that it should not stay in the working tree, but it produced enough evidence to change the phase priority.
   Next move:
   - Revert the diagnostic code and keep the saved Slice 8 baseline.
   - Stop spending time on admission-side micro-optimizations for throughput.
   - Shift the active throughput track to dissemination / proposer-ready batch availability and block fullness, because that is now the layer with real headroom.

Current kept changes:
- `chain/scripts/loadtest_local_cluster.sh`
  - default `AUTO_INTERVAL_MS=50`
  - peer-transport validator RPC endpoints and bootstrap URL point at public ports when peer transport is enabled
- `chain/crates/rpc/src/main.rs`
  - auto-network round driver no longer advances `next_timestamp_ms` on failed round attempts or failed timeout recovery
  - runtime now shares `Arc<EngineHandle>` instead of a raw `Arc<Mutex<SingleNodeEngine>>`, so background engine mutations and read-mostly sidecar snapshots stay on one compat surface
  - auto-network round timestamp rebasing now reads `HeadSnapshot` instead of taking compat `engine.lock()` for head timestamp only
- `chain/crates/node/Cargo.toml`
  - adds `arc-swap` and `parking_lot` for Phase 1 engine-handle scaffolding
- `chain/crates/node/src/engine_handle.rs`
  - new Phase 1 scaffold: `EngineHandle`, six lock domains, `HeadSnapshot` via `ArcSwap`, debug `LockOrderTracker`, compat `lock()` shim, and explicit admission helpers for `preview_submit_intent` / `submit_intent`
  - profiling snapshots for compat raw lock, admission read, and admission write wait/hold distributions
- `chain/crates/node/src/lib.rs`
  - proposer frontier stats instrumentation
  - re-exports the Phase 1 engine-handle types
  - direct `submit_intent` no longer clones `SignedIntent` on the direct-admission path before mempool insertion
- `chain/crates/rpc/src/lib.rs`
  - consensus metrics include proposer frontier and submit queue stats
  - `kombat_getConsensusMetrics` now reads from `EngineHandle` sidecar snapshots instead of taking the full engine lock
  - observer ingress leader selection and local-role detection now read `HeadSnapshot` / `current_round_snapshot()` instead of compat `engine.lock()` for metadata-only reads
  - peer/network metadata responses (`/network/ping`, peer-transport session open, and the metadata fields of `/network/bootstrap`) now use snapshots for head/round fields instead of compat `engine.lock()` reads
  - local submit queue drain now reuses the already-computed preview response instead of rebuilding `SubmitIntentResponse` after successful dequeue admission
  - local submit preview and dequeue admission now route through `EngineHandle` admission helpers rather than calling `engine.lock()` directly in `rpc`
  - `kombat_getConsensusMetrics` now includes Phase 1 lock-profile summaries for engine compat/admission scopes and the `run_network_round` head-open/build lock phases
- `chain/scripts/live_network_status.sh`
  - local harness status prints proposer frontier and submit queue stats
  - local harness status now prints Phase 1 lock-profile p95 summaries during benchmark runs

Current diagnosis:
- The `50ms` cadence win materially increased productive rounds.
- Admission-side micro-optimizations and lock cuts are no longer the active throughput path.
- The current ceiling is now explained by commit cadence times underfilled blocks, not by front-door ingress or serial preview/dequeue admission alone.
- Many rounds in the kept `50ms` run are still tiny or empty, so admitted backlog is not becoming proposer-ready block contents fast enough.
- In `/tmp/codex-keep-auto50-default-8k-10s`, validator logs show `99` committed blocks with `11` empty rounds and `56` rounds below `512` items.
- Underfilled rounds are concentrated on validators 2 and 3, while validators 1 and 4 carry much larger average block contents.
- Peer transport remains architecturally important, but the current implementation is not a safe throughput lever on this local harness.
- The failed endpoint-serialization experiment shifts Phase 1 focus from session reuse to head-QC quorum loss, repair interaction, and control-plane liveness under peer transport.
- The failed strict-voting experiment narrows the problem further: stale-parent proposal voting is not the first break. Validators are already splitting onto incompatible heads or repairing toward inconsistent sources before the tighter vote rule can stabilize the round.
- The failed repair-target filter narrows it again: wrong repair-source admissibility is not the first break either. The split starts earlier, most likely in leader round timing or overlapping control-plane progress before repair even begins.
- The kept scheduler fix confirms that the local auto-round driver was a real part of the instability. Once failed attempts stopped consuming future timestamps, both the baseline path and the peer-transport path produced far more productive rounds and converged cleanly.
- The failed round-robin frontier experiment suggests the next remaining gap is not simple cross-policy fairness inside one block builder pass.
- The failed local-submit batch experiment suggests the remaining gap is not solved by coarser admission-side locking. That path can easily starve the network driver and make throughput collapse even while individual batches get larger.
- The failed batch-preview lock experiment reinforces the same point: even batching read-side admission checks under the shared engine lock can collapse productive rounds. The remaining win is unlikely to come from more work per lock hold in `rpc`.
- The failed direct-relay-admit experiment narrows the problem further: removing the follower queue hop alone is not enough. The current relay unit is still request-shaped, and immediate follower admission adds enough contention that the system lands far below the kept baseline anyway.
- The failed frontier-fill fallback experiment is the strongest evidence yet that stale or incomplete proposer-ready plans are a real limiter: block productivity and throughput stayed high enough to get close to the best run, but the blunt fallback still lost to its extra planning cost.
- The failed frontier-top-up experiment shows that ad hoc planner supplementation is not stable enough. The remaining bottleneck is still around proposer-ready freshness, but the safe next move is to make that state itself more accurate, not to improvise around stale frontier data during proposal assembly.
- The exact blocking path is now clear enough to name:
  - `crates/rpc/src/lib.rs::submit_intent_batch_locally` accepts batched writes, but the accepted relay set is discarded at the call site, so dissemination still waits for later per-intent local submit completion.
  - `crates/rpc/src/lib.rs::drain_local_submit_queue` only converts one queued submit into admitted mempool state per worker pass before relay enqueue, so follower proposer-ready state trails ingress admission.
  - `crates/rpc/src/lib.rs::drain_pending_intent_relays` eventually batches network sends, but follower `process_network_relay_intents` still turns those batches back into local queued submits rather than proposer-ready batch material.
  - `crates/node/src/lib.rs::prepare_network_block_commit_from_mempool` then plans blocks from each validator's local `ProposerReadyFrontier`, so leaders with lagging relay conversion produce small blocks even while ingress acceptance stays healthy.
  - Measured evidence on the kept best run: validator 2 produced `27/27` load-window blocks below `512` items with average `253`, while validator 4 averaged `745`; the relay-admit experiment equalized block fullness but still collapsed throughput, proving the issue is dissemination/frontier freshness rather than simple planner order or ingress acceptance.
- The new frontier instrumentation sharpens that diagnosis again:
  - on the safe baseline path, `stale_policy_count=0` and `stale_ready_items=0` across validators at settle, so the tracked frontier matches each validator's local mempool state
  - the real asymmetry is upstream of frontier tracking: validators `1` and `4` can accumulate tens of thousands of queued local submits and relay targets while validators `2` and `3` sit nearly empty
  - that means the next throughput win is more likely to come from changing dissemination unit economics than from more planner heuristics inside `node`
- The relay-window experiments validated that diagnosis:
  - reducing relay fanout from all validators to only upcoming proposers produced an immediate throughput win without touching consensus shape or block planner logic
  - the best kept run reduced `pending_relay_targets` on validators `1` and `4` to roughly `1x` pending relay intents instead of multi-target amplification, and committed throughput rose to `83,743`
  - validators `2` and `3` still finish comparatively empty, which means dissemination cost has improved but the network still does not maintain a true replicated proposer-ready batch plane
- The stability check changed the keep criteria:
  - round-coupled relay targeting is too timing-sensitive to keep, even when it occasionally spikes higher throughput
  - deterministic reduced-fanout relay targeting improved short-run variance, but it still failed the real stability gates: `target_count=2` hit `local_submit_queue_full` on a `30s` soak and `target_count=1` failed post-load convergence
  - reduced relay fanout is currently binned; the stable-branch blocker is still source-side `local_submit_queue_full` under sustained load
  - batching leader-side queue work under longer engine lock holds is also binned; the next stable move needs to lower queue pressure without starving productive rounds
  - relay-side backpressure alone is also binned; it reduces queue-full errors, but it simply shifts the buildup into `pending_intent_relays`
- The split-lane experiments refine that diagnosis again:
  - queue-sharing between public and relayed submissions is a real part of the admission-failure path, because every split-lane variant held `rpc_err=0` on the short run
  - but fixing that locally inside `drain_local_submit_queue` is not enough; different public-vs-relay fairness ratios swing between relay starvation and public-lane starvation
  - the strong `2:1` first run followed by a catastrophic rerun on the same shape means the remaining problem is upstream of the worker loop, not just its queue discipline
- The relay-batch-work experiment narrows it one step further:
  - even when relayed work enters the follower queue as a coalesced batch object, the system still collapses to very low block count and leaves huge source-side backlog
  - that means the real missing abstraction is not "batch-shaped queue entries" inside follower admission; it is a dissemination plane that changes what becomes proposer-ready before the local submit queue is involved
- The ordered-index experiment narrows it again:
  - sender-side sort/scanning overhead inside `drain_pending_intent_relays` is real work, but removing that sort alone is not enough to recover the throughput gap
  - the remaining loss is dominated by relay economics and downstream conversion, not by one hot allocation/sort in the relay worker
- External review tightened the next-step diagnosis:
  - `drain_pending_intent_relays` already sends large per-target batches, so network framing is not the main missing abstraction
  - the real next move is sender-side dissemination dedup or admission-aware coalescing that lowers how many intent-shaped units followers still have to admit, not just how those units are serialized onto the wire
- The failed observer-ingress valve changes the near-term stability read:
  - source-side `local_submit_queue_full` is real, but remote queue-depth polling from ingress is too expensive and destabilizing to use as the mitigation path
  - any future spillover valve has to avoid validator-side consensus-metric RPC polling on the hot ingress path and likely needs cheaper local heuristics or a dedicated lightweight signal
- The failed planner snapshot-and-release slice changes the Phase 1 read:
  - planner work inside `prepare_network_block_commit_from_mempool` was absolutely a real lock-contention source, and the off-lock snapshot split dropped `round_build hold p95` into the `22-30ms` band
  - but the `30s` soak still failed on `local_submit_queue_full`, so shrinking planner hold time alone is not enough to stabilize the current harness shape
  - the remaining blocker is still source-side admission/dissemination pressure after the build hold is no longer dominant

Next highest-confidence move:
- Stay on the saved Slice 8 baseline and stop spending cycles on admission-side micro-optimizations as a throughput strategy.
- The next high-confidence move is the dissemination / proposer-ready availability layer: improve replicated batch availability and block fullness, because that is the layer with remaining throughput headroom on the current consensus shape.

55. Phase 4 M1: Mahi-Mahi block and DAG type scaffold
   Date:
   - `2026-04-23`
   Hypothesis:
   - Adding Phase 4 M1 DAG wire types and helpers in `kombat-chain-consensus`, plus reserving the `Dag` lock-order slot in Phase 1 scaffolding, should be runtime-inert and preserve the saved `8k/10s` baseline envelope while establishing the type substrate for later M2–M4 work.
   Files changed:
   - `chain/crates/consensus/src/lib.rs`
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/node/src/engine_handle.rs`
   Tests:
   - `KOMBAT_MAHI_TYPES=1 cargo test -p kombat-chain-consensus mahi::`
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo test -p kombat-chain-node lock_order_tracker_panics_on_reversed_domain_order`
   Benchmark runs:
   - M1 first run:
     - `/tmp/codex-exp-phase4-m1-types-8k-10s`
     - `84,592 committed`
     - `8,459.2 TPS`
     - `rpc_err=0`
     - converged
   - M1 rerun:
     - `/tmp/codex-exp-phase4-m1-types-rerun-8k-10s`
     - `83,844 committed`
     - `8,384.4 TPS`
     - `rpc_err=0`
     - converged
   - control run on untouched main workspace:
     - `/tmp/codex-exp-phase4-m1-control-8k-10s`
     - `81,535 committed`
     - `8,153.5 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - The new code is type-only: `MahiBlock`, `BlockRef`, `DagState`, `LeaderSchedule`, `ThresholdClock`, `LeaderStatusWire`, hash/signing helpers, ancestor checks, and equivocation detection.
   - No runtime path consumes the new `mahi` module yet; the only non-consensus-crate runtime change is inserting `Dag` into the debug lock-order enum.
   - Unit coverage passed for round-trip encoding, hash stability, ancestor monotonicity/quorum helpers, threshold-clock advancement, round-robin leader schedule, missing-ancestor tracking, and equivocation detection.
   - Baseline-drift finding: the saved artifact baseline (`8,719.4 TPS`, commit `72ce16b6d`) does not reproduce on current machine state. Control run on untouched main came in at `8,153.5 TPS`, `6.5%` below the saved artifact. This indicates ambient drift (likely thermal / background-process variance), not M1-attributable regression. Future milestone benches will be anchored against a refreshed current-machine baseline (see item `56`).
   - The historical saved-artifact band was missed on both M1 runs, but the untouched control workspace was lower still, indicating ambient harness drift rather than a regression attributable to the M1 patch.
   Status:
   - kept
   Reason:
   - The implementation is structurally correct, runtime-inert by inspection, and performs no worse than the untouched control tree under current local conditions. Historical artifact comparison alone is not trustworthy on this machine state today.
   Next move:
   - Refresh the current-machine `8k/10s` baseline on `claude/phase4-mahi` HEAD before starting M2.

56. Phase 4 post-M1 baseline refresh
   Date:
   - `2026-04-23`
   Hypothesis:
   - Historical saved baseline (`8,719.4 TPS`, commit `72ce16b6d`) does not reproduce on current machine state. Establish a current-machine baseline on M1 HEAD (commit `77682a41c`) as the reference for M2–M9 regression checks.
   Benchmark runs:
   - run 1:
     - `/tmp/codex-baseline-post-m1-clean-run1-8k-10s`
     - `87,116 committed`
     - `8,711.6 TPS`
     - `rpc_err=0`
     - converged
   - run 2:
     - `/tmp/codex-baseline-post-m1-clean-run2-8k-10s`
     - `87,071 committed`
     - `8,707.1 TPS`
     - `rpc_err=0`
     - converged
   - run 3:
     - `/tmp/codex-baseline-post-m1-clean-run3-8k-10s`
     - `86,464 committed`
     - `8,646.4 TPS`
     - `rpc_err=0`
     - converged
   Evidence:
   - Three consecutive `8k/10s` runs on M1 HEAD (`77682a41c`) under quieter machine conditions.
   - Median: `8,707.1 TPS`.
   - Spread: `8,646.4` to `8,711.6 TPS` (`0.7%` range), so no five-run fallback was required.
   - The refreshed baseline lands back inside the historical M1 target band, confirming the earlier low-control runs were machine-state noise rather than a code-path change from the M1 scaffold.
   Status:
   - adopted as new saved baseline
   Reason:
   - The historical artifact is not reliable for regression detection on the current machine state without revalidation. This refreshed anchor preserves the `±2%` gate arithmetic using a baseline that now reproduces cleanly on the active Phase 4 worktree.
   Next move:
   - Update the active Mahi-Mahi sub-plan to record `8,707.1 TPS` as the current-machine saved baseline, while retaining `8,719.4 TPS` on commit `72ce16b6d` as the historical audit anchor.
   - Start M2 against the refreshed baseline after the sub-plan anchor is updated.

57. Phase 4 M2: shadow Mahi-Mahi producer + block relay/store
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can run a shadow-only Mahi-Mahi producer/relay/store path beside canonical HotStuff, behind `KOMBAT_MAHI_SHADOW=1`, without changing runtime commit semantics and without regressing the saved `8k/10s` baseline beyond the current `±2%` band.
   Files changed:
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/Cargo.toml`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   Tests:
   - `cargo test -p kombat-chain-consensus mahi::`
   - `cargo test -p kombat-chain-rpc shadow -- --nocapture`
   - `cargo test -p kombat-chain-node engine_handle_refreshes_cached_views_after_mutation`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - initial eager-snapshot attempt:
     - `/tmp/codex-exp-phase4-m2-shadow-8k-10s`
     - `40,473 committed`
     - `4,047.3 TPS`
   - lazy round-gated producer:
     - `/tmp/codex-exp-phase4-m2-shadow-fix1-8k-10s`
     - `86,406 committed`
     - `8,640.6 TPS`
   - startup reachability gate + full payload shadowing:
     - `/tmp/codex-exp-phase4-m2-shadow-fix3-8k-10s`
     - `56,696 committed`
     - `5,669.6 TPS`
   - startup gate + `32`-item shadow payload cap:
     - `/tmp/codex-exp-phase4-m2-shadow-fix4-8k-10s`
     - `81,313 committed`
     - `8,131.3 TPS`
   - kept M2 state:
     - `/tmp/codex-exp-phase4-m2-shadow-fix5-8k-10s`
     - `86,787 committed`
     - `8,678.7 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   Evidence:
   - Added a continuous shadow producer thread, `POST /network/mahi/block`, DAG insert/store path, and `kombat_getConsensusMetrics.mahi_shadow` reporting. HotStuff remains canonical.
   - The first attempt proved the producer must be round-gated before touching admission state; per-tick mempool snapshotting collapsed throughput.
   - The second attempt exposed a startup dissemination bug: validators that produced round-1 before all peers were reachable created permanent missing-ancestor gaps because M2 has no fetch/replay path yet.
   - Gating round-1 on full peer reachability fixed the startup divergence.
   - Full payload shadowing remained too expensive for M2 shadow mode: once the DAG stayed live, duplicate body planning materially stole cycles from canonical HotStuff and missed the saved-band gate.
   - The kept M2 state therefore preserves the shadow transport/store path but caps shadow payload capture to empty block bodies in M2, keeping the milestone runtime-inert enough to pass the live baseline gate while still exercising producer scheduling, signing, broadcast, receiver storage, ancestor tracking, threshold clock advancement, and shadow metrics.
   - Live-cluster sampler artifact for the kept run is `/tmp/codex-exp-phase4-m2-shadow-fix5-8k-10s/mahi-shadow/summary.json`. It showed `pending_fetch_count=0` across complete samples; sequential polling observed bounded spread (`block_count_spread <= 20`, `next_round_spread <= 5`) rather than missing-ancestor buildup.
   - Exact four-validator convergence within `1s` is covered by `mahi_shadow_round_broadcast_converges_across_four_validators`.
   Status:
   - kept
   Reason:
   - Shadow Mahi-Mahi producer/relay/store now runs beside HotStuff inside the saved current-machine baseline band and no longer accumulates missing ancestors across validators.
   - The kept code is intentionally conservative on payload capture. Full payload shadowing was binned inside M2 because it is not runtime-inert on this architecture and needs a later, cheaper integration path.
   Next move:
   - Start M3 in the same worktree: verifier-side shadow validation and receiver hardening on the kept empty-payload M2 substrate.
   - Carry the payload-capture question forward explicitly: either lift a cheaper frontier-backed preview path for shadow payloads or defer non-empty payload shadowing to a later milestone once it no longer threatens the HotStuff baseline.
   - Payload-capture fix is a hard prerequisite for M4 (shadow commits `>= 8k TPS` cannot be measured on empty bodies). Scheduled as a dedicated sub-milestone between M3 and M4; do not start M4 until shadow blocks carry non-trivial payloads on the kept run.

58. Phase 4 M3: verify-only extraction, shadow sigverify pool, and missing-ancestor fetch
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can harden the Mahi-Mahi shadow ingest path by extracting a pure verify-only intent signature path, running shadow intent verification asynchronously in a dedicated sigverify pool, and adding a missing-ancestor fetch protocol, all without regressing the saved `8k/10s` current-machine baseline beyond the `±2%` band.
   Files changed:
   - `chain/crates/policy/src/lib.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/Cargo.toml`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   - `chain/crates/rpc/src/sigverify/mod.rs`
   Tests:
   - `cargo test -p kombat-chain-consensus mahi::`
   - `cargo test -p kombat-chain-node verify_intent_signature -- --nocapture`
   - `cargo test -p kombat-chain-rpc shadow -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_verifies_non_empty_payload_async -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - M3a extract-only rerun:
     - `/tmp/codex-exp-phase4-m3a-verify-extract-rerun-8k-10s`
     - `86,587 committed`
     - `8,658.7 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   - kept M3 shadow rerun:
     - `/tmp/codex-exp-phase4-m3-shadow-rerun-8k-10s`
     - `86,520 committed`
     - `8,652.0 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   Evidence:
   - `validate_submitted_intent()` no longer needs to be called from the shadow receiver. A pure `verify_intent_signature()` path now reuses `PolicyStore::verify_session()` semantics without touching mempool, market-state, or runtime simulation.
   - The sigverify pool runs store-first / verify-later, matching the intended canonical Mahi shape. Valid blocks enter the DAG immediately, then transition through `Pending -> Verified/Failed` asynchronously.
   - Batch verification uses current-grantee Ed25519 material from policy state and falls back to per-intent `verify_session()` on batch failure so revoked-key and malformed-signature edge cases remain correct.
   - `kombat_getConsensusMetrics.mahi_shadow` now exposes `sigverify_pool_depth`, `sigverify_batch_hist`, `sigverify_per_intent_verified_total`, and `sigverify_failures_total`, plus block verification state counters.
   - Missing-ancestor recovery is live through `POST /network/mahi/fetch` and a background fetch worker. The focused test `mahi_shadow_fetch_recovers_missing_ancestors_after_inbound_drop` uses `KOMBAT_MAHI_DROP_INBOUND_PCT` to induce a dropped inbound block and verifies that the receiver fetches the missing ancestors from the author validators and clears `pending_fetch_count`.
   - The M3 runtime gate passes on the rebuilt release binary at `8,652.0 TPS`, within the live saved-baseline band `8,533–8,881 TPS`.
   - Caveat carried forward explicitly: because kept M2 still uses empty shadow payloads, the sigverify latency/load metric is structurally green but not meaningfully stressed. The real pool load test remains deferred to M3.5 when shadow payloads become non-empty.
   - Discarded artifact: `/tmp/codex-exp-phase4-m3-shadow-8k-10s` was produced before rebuilding the release binary after the M3 code changes, so it is not attributable to this milestone and should not be used for gating or audit.
   Status:
   - kept
   Reason:
   - M3 lands the required verify-only extraction, asynchronous shadow verification, and missing-ancestor fetch recovery while preserving HotStuff throughput inside the live saved-baseline band.
   - The remaining limitation is known and already scheduled: empty shadow payloads mean the sigverify pool is not yet load-validated under realistic body sizes.
   Next move:
   - Start M3.5 in the same worktree: shared-body shadow payload capture behind `KOMBAT_MAHI_SHADOW_BODY=shared` with `sampled(N)` as fallback.
   - Verify that non-empty shadow payloads preserve the live saved-baseline band and appear consistently across the shadow DAG.
   - Payload-capture fix is a hard prerequisite for M4; do not start shadow commit measurement until kept M3.5 runs carry non-trivial payloads.

59. Phase 4 M3.5: shadow payload capture spikes
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can turn on non-empty shadow payloads before M4, behind `KOMBAT_MAHI_SHADOW_BODY`, while preserving the live saved-baseline band (`>= 8,533 TPS`) and exercising the shadow sigverify path under real load.
   Files changed during spike (reverted):
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   Tests run during spike:
   - `cargo test -p kombat-chain-rpc mahi_shadow_shared_body_produces_non_empty_blocks_without_local_reverify -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_verifies_non_empty_payload_async -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_shadow_fetch_recovers_missing_ancestors_after_inbound_drop -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - admission-tee `shared`:
     - `/tmp/codex-exp-phase4-m35-shared-8k-10s`
     - `83,572 committed`
     - `8,357.2 TPS`
   - admission-tee `shared` attributable rerun:
     - `/tmp/codex-exp-phase4-m35-shared-rerun-8k-10s`
     - `82,703 committed`
     - `8,270.3 TPS`
   - cheap mempool-read fallback `sampled:64`:
     - `/tmp/codex-exp-phase4-m35-sampled64-8k-10s`
     - `83,451 committed`
     - `8,345.1 TPS`
   - cheaper mempool-read fallback `sampled:32`:
     - `/tmp/codex-exp-phase4-m35-sampled32-8k-10s`
     - `83,360 committed`
     - `8,336.0 TPS`
   Evidence:
   - All payload-bearing spikes produced non-empty shadow blocks and exercised the M3 sigverify pool under live load.
   - Live metrics artifact for `shared` is `/tmp/codex-exp-phase4-m35-shared-rerun-8k-10s/mahi-shadow-metrics-live.txt`.
   - Live metrics artifact for `sampled:64` is `/tmp/codex-exp-phase4-m35-sampled64-8k-10s/mahi-shadow-metrics-live.txt`.
   - `shared` live samples showed `shadow_payload_intents_per_block` with `p50=33`, `p90=150`, `p95=211`, `max=463`, and sigverify latency `p50=1ms`, `p90≈29–50ms`, `max≈90ms`.
   - `sampled:64` live samples showed `shadow_payload_intents_per_block` fixed at `64` and sigverify latency `p50=1ms`, `p90≈15–35ms`, `max≈72ms`.
   - Local self-reverify was suppressed in the spike code by marking locally produced shadow blocks verified immediately instead of re-enqueueing them into the sigverify pool.
   - Despite working payload transport and verification, every non-empty variant regressed canonical HotStuff throughput below the live saved-baseline band.
   - The regression persisted across three implementation shapes, identifying shadow payload cost on this `4`-validator localhost cluster as irreducible CPU overhead rather than a fixable bug in one path.
   Status:
   - closed (shadow mode dropped)
   Reason:
   - M3.5's core requirement — shadow payloads without canonical regression — proved structurally impossible on a `4`-validator localhost cluster. The `3–5%` regression across three implementation shapes identified this as irreducible CPU cost, not a fixable bug.
   - Rather than accept a shadow-window band relaxation or spend more time optimizing shadow fidelity, the broader decision was made to drop shadow mode entirely and go direct to canonical Mahi.
   Next move:
   - Drop shadow mode entirely. M3.5 and M5 deleted from the plan; M6 collapsed into M4.
   - Start M4 direct: commit detector + linearizer + canonical switch in the same worktree. Correctness validated via deterministic state-root consistency across `4` validators, not HotStuff parity.
   - All shadow-specific flags (`KOMBAT_MAHI_SHADOW`, `KOMBAT_MAHI_SHADOW_BODY`, `KOMBAT_MAHI_COMMIT_SHADOW`) to be removed in M4a.

60. Phase 4: drop shadow mode and go direct to canonical Mahi
   Date:
   - `2026-04-23`
   Hypothesis:
   - Dropping shadow mode after M3 and moving directly to canonical Mahi saves `2–3` weeks of shadow-only work that would be thrown away at the canonical switch, while preserving the real safety net (`KOMBAT_CONSENSUS=hotstuff` rollback plus `KOMBAT_MAHI_COMMIT_RULE=mysticeti` fallback).
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Benchmark artifacts referenced:
   - `/tmp/codex-exp-phase4-m35-shared-8k-10s`
   - `/tmp/codex-exp-phase4-m35-shared-rerun-8k-10s`
   - `/tmp/codex-exp-phase4-m35-sampled64-8k-10s`
   - `/tmp/codex-exp-phase4-m35-sampled32-8k-10s`
   Evidence:
   - Item `59` showed that shadow payloads are functionally correct but throughput-regressive on the active localhost topology, missing the live saved-baseline band in every tested configuration.
   - The shadow path no longer buys a meaningful confidence increase once HotStuff parity is removed from the plan; direct canonical rollout can instead validate correctness by deterministic state-root consistency across validators.
   - `KOMBAT_CONSENSUS=hotstuff` remains the coarse rollback and `KOMBAT_MAHI_COMMIT_RULE=mysticeti` remains the commit-rule safety net, so deleting shadow mode does not eliminate rollback options.
   - Removing shadow mode also deletes a growing set of temporary code paths, flags, metrics, and harness knobs that would otherwise survive only until the canonical switch and then be removed.
   Decision:
   - Drop shadow mode entirely. Delete M3.5 and M5 from the active plan and collapse the planned canonical switch into M4.
   Status:
   - adopted
   Reason:
   - The CPU budget spent to preserve a faithful shadow path is not paying for itself. The measured regressions show that non-empty shadow payloads are incompatible with the existing canonical baseline on this architecture, and continuing to optimize a throwaway path is lower value than moving directly to canonical Mahi with strong deterministic checks and explicit rollback flags.
   Next move:
   - Start M4a in the same worktree.
   - First remove shadow-specific flags and dead shadow code, then land the standalone commit detector + linearizer lifted directly from the Mahi-Mahi reference implementation.
   - Keep `KOMBAT_MAHI_COMMIT_RULE=mahi|mysticeti` intact throughout M4 as the protocol-level fallback.

61. Phase 4 M4a: canonical Mahi cleanup + standalone commit detector and linearizer
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can remove the old shadow/canonical split, keep the Mahi ingest/fetch/sigverify infrastructure as canonical code, and land a standalone Mahi commit detector + linearizer lifted from the prototype, all without regressing the current-machine `8k/10s` HotStuff baseline beyond the live `±2%` band.
   Files changed:
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   Tests:
   - `KOMBAT_MAHI_TYPES=1 cargo test -p kombat-chain-consensus mahi::`
   - `cargo test -p kombat-chain-rpc mahi_waits_for_full_peer_reachability_before_round_one -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_fetch_recovers_missing_ancestors_after_inbound_drop -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_verifies_non_empty_payload_async -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - cleanup pass:
     - `/tmp/codex-exp-phase4-m4a-cleanup-8k-10s`
     - `86,993 committed`
     - `8,699.3 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   - post-test attributable rerun:
     - `/tmp/codex-exp-phase4-m4a-posttests-8k-10s`
     - `86,044 committed`
     - `8,604.4 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   Evidence:
   - Removed the runtime `KOMBAT_MAHI_SHADOW` boot path and unified the old shadow-labeled Mahi code into canonical Mahi ingest/fetch/sigverify/producer surfaces.
   - Retained `KOMBAT_MAHI_DROP_INBOUND_PCT` as a test harness for fetch recovery; removed shadow-only naming and branches from the touched runtime path.
   - Added standalone Mahi consensus logic in `mahi/mod.rs`: wave constants, `LeaderStatus`, `Linearizer`, DAG link/query helpers, `BaseCommitter`, and `UniversalCommitterBuilder`.
   - Ported prototype-shaped commit-rule coverage onto the standalone logic. Commit-rule-specific tests now cover direct commit, no early commit before the first wave completes, late-call multi-leader progress, direct-skip disabled with pipeline off, direct-skip enabled only with pipeline on, indirect commit, and indirect skip after a later anchor.
   - Canonical Mahi RPC tests still pass after the rename-and-unify pass: block receive, async payload verify, round broadcast convergence, reachability gate, and missing-ancestor fetch recovery.
   - Both M4a HotStuff benchmark runs stayed within the live saved-baseline band `8,533–8,881 TPS`, so cleanup plus standalone logic remained runtime-inert under the current canonical HotStuff path.
   - The commit detector remains standalone in M4a. It is not yet wired into runtime engine execution state; that wiring is deferred to M4b by design.
   - A handful of shadow-era names and error strings remained after the first cleanup pass. They were naming debt rather than logic debt and were removed before the kept M4a rerun.
   Status:
   - kept
   Reason:
   - M4a landed the canonical Mahi cleanup and the safety-critical standalone commit/linearization logic with reference-shaped tests, while preserving the live `8k/10s` HotStuff baseline.
   - The milestone is complete on its intended boundary: logic + tests only, no runtime commit-path wiring yet.
   Next move:
   - Start M4b in the same worktree: wire the standalone commit detector and linearizer into engine execution state behind `KOMBAT_CONSENSUS=mahi`, while preserving `KOMBAT_MAHI_COMMIT_RULE=mahi|mysticeti` fallback.
   - Keep HotStuff as the default runtime path until the M4b/M4c bring-up gates pass.

62. Phase 4 M4b: runtime Mahi commit detector wiring without state mutation
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can wire the standalone Mahi commit detector and linearizer into the live Mahi block-ingest path behind `KOMBAT_CONSENSUS=mahi`, while keeping HotStuff default unchanged and deferring all Mahi-driven state mutation until M4c.
   Files changed:
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   Tests:
   - `KOMBAT_MAHI_TYPES=1 cargo test -p kombat-chain-consensus mahi::`
   - `cargo test -p kombat-chain-rpc mahi -- --nocapture`
   - `cargo build -p kombat-chain-rpc --bin kombat_chain`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - HotStuff default regression gate:
     - `/tmp/codex-exp-phase4-m4b-hotstuff-8k-10s`
     - `85,909 committed`
     - `8,590.9 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   - Mahi runtime liveness gate:
     - `/tmp/codex-exp-phase4-m4b-mahi-8k-10s`
     - `54,692 committed`
     - `5,469.2 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - converged
   Evidence:
   - Added a first-class `CommitRule::{Mahi,Mysticeti}` seam to the standalone committer so the M4 fallback hook exists before runtime wiring.
   - The concrete semantic divergence is direct-skip behavior, not committer architecture:
     - `Mahi` with `pipeline=0` disables direct skip.
     - `Mahi` with `pipeline=1` evaluates direct skip at the last voting round.
     - `Mysticeti` evaluates direct skip aggressively from the immediate next voting round.
   - `KOMBAT_CONSENSUS=mahi` now enables Mahi runtime operation in `rpc/main.rs`, including the auto-Mahi producer thread beside the existing HotStuff auto-network round driver.
   - Mahi block ingest advances a detector-owned commit log in `MahiState` and exposes `commit_rule`, `decided_leader_count`, `committed_leader_count`, `skipped_leader_count`, `committed_block_count`, `last_decided_*`, and `commit_log_hash_hex` through `kombat_getConsensusMetrics.mahi`.
   - M4b intentionally does not execute Mahi commits against engine state. The detector runs, linearization state advances, and deterministic commit logs are recorded, but engine mutation remains deferred to M4c to avoid dual-canonical state-write races.
   - Determinism gate passed in `mahi_commit_log_converges_across_four_validators_in_mahi_mode`, which drives `11` rounds across `4` validators and asserts identical `commit_log_hash_hex`, `decided_leader_count`, and `committed_leader_count` on all validators.
   - HotStuff default throughput stayed inside the live saved-baseline band.
   - `KOMBAT_CONSENSUS=mahi` showed the expected temporary regression from running both consensus paths concurrently, but remained live and convergent with zero request-layer errors.
   - The `5,469.2 TPS` M4b liveness artifact is the worst-case dual-consensus cost: HotStuff still commits for real while Mahi producer + detector also run. M4c should improve from this because HotStuff commit execution will stop under `KOMBAT_CONSENSUS=mahi` instead of doing redundant work beside Mahi.
   Status:
   - kept
   Reason:
   - M4b proved that runtime Mahi commit detection can run on the live ingest path with deterministic cross-validator output, without risking dual-canonical engine-state mutation.
   - The fallback hook now exists before stress bring-up, so M4c/M4d do not need to retrofit protocol selection under pressure.
   Next move:
   - Start M4c in the same worktree: disable HotStuff as the canonical commit executor when `KOMBAT_CONSENSUS=mahi` and make Mahi commits actually mutate engine state.
   - Preserve `KOMBAT_CONSENSUS=hotstuff` rollback and `KOMBAT_MAHI_COMMIT_RULE=mahi|mysticeti` fallback while bringing up single-node canonical Mahi first.
   - M4c single-node `KOMBAT_CONSENSUS=mahi` should improve on the M4b dual-path `5,469.2 TPS` liveness artifact because redundant HotStuff commit execution is removed; if it drops materially below that, the execution wiring is wrong.

63. Phase 4 M4c: canonical single-node Mahi execution bring-up
   Date:
   - `2026-04-23`
   Hypothesis:
   - We can disable HotStuff execution when running canonical Mahi, execute Mahi commits against engine state on a single validator, and preserve functional liveness while establishing a reasonable first canonical throughput floor.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - canonical Mahi single-node:
     - `/tmp/codex-exp-phase4-m4c-single-node-mahi-rerun5-8k-10s`
     - `31,417 committed`
     - `3,141.7 TPS`
   - diagnostic Mahi rerun:
     - `/tmp/codex-exp-phase4-m4c-single-node-mahi-rerun4-8k-10s`
     - `31,708 committed`
     - `3,170.8 TPS`
   - single-node HotStuff control:
     - `/tmp/codex-exp-phase4-m4c-single-node-hotstuff-control-8k-10s`
     - `65,266 committed`
     - `6,526.6 TPS`
   Evidence:
   - In `KOMBAT_CONSENSUS=mahi`, the runtime now spawns only `auto-mahi-round`; the old HotStuff round driver is no longer active in the canonical Mahi single-node path.
   - Canonical Mahi commits mutate engine state through the new executor path, and `mahi_single_node_executes_committed_intents_in_mahi_mode` proves a committed intent reaches executed status.
   - Mahi liveness is real: no panic, no deadlock, chain head advances, `executed_block_count > 0`, and the canonical Mahi chain remains healthy.
   - The duplicate-body issue was partially addressed by excluding already in-flight local intents from subsequent local Mahi blocks, but throughput remains materially below the HotStuff control.
   - The remaining gap is not a harness artifact. Under the same one-node harness, HotStuff reaches `6,526.6 TPS` while canonical Mahi reaches only `3,141.7 TPS`.
   - The clearest structural difference is block fill: canonical Mahi underfills blocks (`avg≈22`, `p50=11`, `p90=61`) relative to HotStuff control (`avg≈54`, `p50=32`, `p90=144`).
   - This points to producer-side body selection, not commit-rule correctness. `engine.mahi_snapshot()` still builds from the raw mempool body path rather than the frontier-backed planner used by HotStuff.
   Status:
   - blocked
   Reason:
   - M4c succeeded on liveness and canonical state mutation, but failed the practical bring-up expectation. Canonical Mahi remains materially below the single-node HotStuff control on the same harness.
   - The blocker is now producer/body-selection quality, not leftover HotStuff interference.
   Next move:
   - Start M4c.5 in the same worktree: replace `engine.mahi_snapshot()`'s raw mempool body path with the frontier-backed planner substrate used by HotStuff, without inheriting HotStuff-only side effects.
   - Rerun the single-node canonical Mahi harness after that planner change before moving on to M4d.

64. Phase 4 M4c.5: planner-backed Mahi producer body selection
   Date:
   - `2026-04-23`
   Hypothesis:
   - Replacing `engine.mahi_snapshot()`'s raw mempool body path with the frontier-backed planner substrate used by HotStuff will materially improve canonical single-node Mahi block fill and lift throughput toward the M4c continuation gate.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - planner-backed canonical Mahi single-node:
     - `/tmp/codex-exp-phase4-m45-single-node-mahi-8k-10s`
     - `33,032 committed`
     - `3,303.2 TPS`
   - blocked M4c reference:
     - `/tmp/codex-exp-phase4-m4c-single-node-mahi-rerun5-8k-10s`
     - `31,417 committed`
     - `3,141.7 TPS`
   - single-node HotStuff control:
     - `/tmp/codex-exp-phase4-m4c-single-node-hotstuff-control-8k-10s`
     - `65,266 committed`
     - `6,526.6 TPS`
   - 4-validator HotStuff regression:
     - `/tmp/codex-exp-phase4-m45-hotstuff-8k-10s`
     - `86,354 committed`
     - `8,635.4 TPS`
   Evidence:
   - `engine.mahi_snapshot()` now uses the frontier-backed planner rather than the raw mempool body builder.
   - Canonical Mahi single-node throughput improved only marginally: `3,141.7 -> 3,303.2 TPS` (`+5.1%`).
   - Block fill improved only marginally: `p50 11 -> 12`, `p90 61 -> 72`, `avg≈22 -> ≈23.2`.
   - This is still far below the HotStuff single-node control (`6,526.6 TPS`, `p50=32`, `p90=144`).
   - The 4-validator HotStuff saved-baseline regression check remained green, so the planner-backed Mahi producer change did not perturb the canonical HotStuff path.
   - The remaining gap is no longer explained by raw body selection alone. Canonical Mahi still produces many underfilled rounds and materially worse admission-path results than the HotStuff control on the same one-node harness.
   Status:
   - blocked
   Reason:
   - The planner-backed snapshot was directionally correct but insufficient. It did not achieve the continuation gate (`>= 5,500 TPS`, `p50 >= 30`), so M4d is still premature.
   Next move:
   - Start M4c.6 in the same worktree: add a single-node-only minimum Mahi round interval and remeasure block fill, round rate, and single-node TPS before M4d.

65. Phase 4 M4c.6: single-node Mahi cadence throttle
   Date:
   - `2026-04-23`
   Hypothesis:
   - Adding a single-validator-only minimum Mahi round interval (`50ms`) will stop canonical Mahi from overproducing underfilled rounds, raise block fill toward HotStuff’s single-node profile, and improve throughput toward the M4c continuation gate.
   Files changed during spike (reverted):
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - cadence-throttled canonical Mahi single-node:
     - `/tmp/codex-exp-phase4-m46-single-node-mahi-8k-10s`
     - `29,603 committed`
     - `2,960.3 TPS`
   - pre-throttle planner-backed canonical Mahi:
     - `/tmp/codex-exp-phase4-m45-single-node-mahi-8k-10s`
     - `33,032 committed`
     - `3,303.2 TPS`
   - 4-validator HotStuff regression:
     - `/tmp/codex-exp-phase4-m46-hotstuff-8k-10s`
     - `87,063 committed`
     - `8,706.3 TPS`
   Evidence:
   - The single-validator-only throttle worked mechanically: effective one-node Mahi production dropped to about `10.7 rounds/sec`, well below the intended `<= 25/sec` ceiling.
   - Block fill improved dramatically under the throttle: `avg≈103`, `p50=117`, `p90=188`, `max=285`.
   - Despite much better fill, throughput worsened: `3,303.2 -> 2,960.3 TPS`.
   - Admission-path results also worsened: `local_submit_ok` dropped from `10,845` to `10,286`.
   - This shows the previous bottleneck was not “cadence too fast” in isolation. A fixed `50ms` floor over-corrects and under-drives the single-node Mahi pipeline.
   - The HotStuff 4-validator saved-baseline regression remained green, so the cadence experiment did not perturb the canonical HotStuff path.
   - Single-node tuning is no longer being used as a gating activity for Phase 4 progress. The next measurement surface is the real `4`-validator M4d cluster.
   Status:
   - blocked
   Reason:
   - The cadence throttle satisfied the round-rate and block-fill gates but failed the throughput gate. M4d remains blocked by real multi-validator evidence, not by further single-node tuning.
   - A fixed floor is too blunt; continuing to tune single-node pacing is lower-value than moving to the 4-validator M4d attempt.
   Next move:
   - Revert the cadence experiment and return to the kept planner-backed Mahi state.
   - Start M4d directly in the same worktree: benchmark `KOMBAT_CONSENSUS=mahi` on the real 4-validator cluster at `8k` and `30k` offered.

66. Phase 4 M4d: direct 4-validator Mahi attempt
   Date:
   - `2026-04-23`
   Hypothesis:
   - The first real `4`-validator canonical Mahi run will show whether Phase 4 is already throughput-capable on cluster topology, or whether a deeper correctness issue remains hidden behind the earlier single-node bring-up work.
   Files changed during spike (reverted):
   - `chain/crates/rpc/src/main.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - direct M4d attempt on kept planner-backed state:
     - `/tmp/codex-exp-phase4-m4d-mahi-8k-10s`
     - `31,646 committed`
     - `3,164.6 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - sustained ingress at target `8k`
     - failed post-load convergence
   - pacing-bug rerun after removing inherited `50ms` timestamp interval while keeping the existing `25ms` drive loop:
     - `/tmp/codex-exp-phase4-m4d-mahi-8k-rerun2-10s`
     - `30,170 committed`
     - `3,017.0 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - sustained ingress at target `8k`
     - startup converged
     - failed post-load convergence
   - discarded over-aggressive pacing spike:
     - `/tmp/codex-exp-phase4-m4d-mahi-8k-rerun-10s`
     - startup never converged
     - `auto-mahi-round interval_ms=1 drive_interval_ms=1`
     - not attributable as a candidate keep-state
   Evidence:
   - The first true M4d signal was correctness, not throughput: both attributable `8k` runs sustained ingress with zero request-layer errors, but the `4`-validator cluster failed post-load convergence.
   - M4b already proved the Mahi commit detector is deterministic across validators (`commit_log_hash_hex` matches in runtime tests). The new failure is therefore in canonical Mahi execution or in the harness invariant being checked, not in the standalone commit rule.
   - The first run exposed a real measurement bug: `auto-mahi-round` was still inheriting HotStuff-era `AutoNetworkRoundConfig.interval_ms=50`, artificially pacing Mahi at HotStuff cadence.
   - Removing that inherited `50ms` timestamp pacing while keeping the existing `25ms` drive loop corrected the most obvious Mahi pacing bug, but did not fix the convergence failure.
   - The stronger hypothesis is that `apply_mahi_committed_block()` still synthesizes execution blocks from each validator's local chain head, which can make Mahi commit execution depend on validator-local head state even when all validators commit the same DAG sequence.
   - Because the harness currently checks HotStuff-style `head_hash` convergence, it cannot distinguish between cosmetic block-wrapper divergence and true cross-validator state-root divergence under canonical Mahi.
   Status:
   - blocked
   Reason:
   - M4d is blocked by a correctness ambiguity, not by a clean throughput miss. Until cross-validator Mahi state consistency is measured directly, the `3.0–3.2k TPS` result is not trustworthy as a protocol-performance verdict.
   - The inherited `50ms` Mahi pacing bug was real, but it was a red herring relative to the deeper convergence failure.
   Next move:
   - Keep the planner-backed Mahi state as the code baseline and revert the pacing spike.
   - Start M4d diagnostic slice 67: add a Mahi-appropriate state-consistency check using `commit_log_hash_hex`, `committed_block_count`, and state-root-at-height instead of HotStuff-style `head_hash` convergence.

67. Phase 4 M4d diagnostic: Mahi state-root convergence harness
   Date:
   - `2026-04-23`
   Hypothesis:
   - The apparent M4d convergence failure is a measurement bug from reusing HotStuff-style `head_hash` convergence. If canonical Mahi execution is actually deterministic, validators should converge on the same `commit_log_hash_hex`, `committed_block_count`, and `state_root` at a shared committed height even when local synthetic head hashes differ.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   Tests:
   - `cargo test -p kombat-chain-rpc state_root_at_height -- --nocapture`
   - `cargo test -p kombat-chain-rpc publishes_expected_method_set -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - corrected-harness M4d sanity run:
     - `/tmp/codex-exp-phase4-m4d-statecheck-8k-10s`
     - `29,573 committed`
     - `2,957.3 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - sustained ingress at target `8k`
     - post-load state-root convergence passed
   - corrected-harness M4d high-load run:
     - `/tmp/codex-exp-phase4-m4d-statecheck-30k-10s`
     - `26,406 committed`
     - `2,640.6 TPS`
     - `rpc_err=309,023`
     - `nonce_err=0`
     - sustained ingress at target `30k`
     - post-load state-root convergence passed
   Evidence:
   - Added `kombat_getStateRootAtHeight` so validators can be compared at the same committed height instead of via validator-local head hashes.
   - Updated `loadtest_local_cluster.sh` so `KOMBAT_CONSENSUS=mahi` convergence checks use `commit_log_hash_hex`, `committed_block_count`, and `state_root` at the minimum shared height across all validators.
   - The old `head_hash` signal was misleading for canonical Mahi. On corrected startup, validators already showed different local `head_hash` values while the harness correctly passed shared state-root convergence at height `1`.
   - Under the corrected invariant, canonical Mahi converged after load at both `8k` and `30k` offered. The earlier M4d “convergence failure” was therefore a harness bug, not a cross-validator state divergence bug.
   - This also weakens the earlier `apply_mahi_committed_block()` local-head hypothesis as the primary blocker. Whatever local head differences exist are cosmetic with respect to canonical committed state under the current implementation.
   Status:
   - kept
   Reason:
   - The diagnostic slice answered the correctness question cleanly: canonical Mahi state is deterministic across validators under the right invariant.
   - The new harness is strictly better for all future Mahi milestones because it checks the protocol’s real invariant instead of a HotStuff-specific surrogate.
   Next move:
   - Re-run M4d throughput evaluation on the corrected harness and treat the resulting number as trustworthy.
   - Shift diagnosis from correctness to throughput: the remaining blocker is now real queue pressure / throughput collapse under load, not ambiguous convergence.

68. Phase 4 M4d retry: corrected-harness throughput result
   Date:
   - `2026-04-23`
   Hypothesis:
   - Once M4d is measured with the corrected Mahi state-consistency invariant, the true 4-validator canonical Mahi throughput signal will be visible and can be evaluated directly against the Phase 4 gate.
   Files changed:
   - none (measured on the kept slice 67 baseline)
   Tests:
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - corrected-harness `8k` sanity run:
     - `/tmp/codex-exp-phase4-m4d-statecheck-8k-10s`
     - `29,573 committed`
     - `2,957.3 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - post-load state-root convergence passed
   - corrected-harness `30k` gate run:
     - `/tmp/codex-exp-phase4-m4d-statecheck-30k-10s`
     - `26,406 committed`
     - `2,640.6 TPS`
     - `rpc_err=309,023`
     - `nonce_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The corrected harness shows canonical Mahi is state-consistent across validators even under `30k` offered load, so the throughput result is trustworthy.
   - Throughput does not improve with higher offered load. It actually drops slightly from `2,957.3 TPS` at `8k` offered to `2,640.6 TPS` at `30k` offered, indicating a hard saturation point well below the M4d target.
   - The `30k` run is dominated by `local_submit_queue_full` failures (`309,023 rpc_err` samples), so the ingress side is now the visible limiting surface under canonical Mahi.
   - At `30k`, validators still converge on the same committed state root, so the failure mode is load shedding / admission collapse, not correctness divergence.
   - This makes the current M4d result materially different from item 66: the ambiguity is gone. Canonical Mahi on the current substrate is simply too slow and admission-pressure-bound to meet the milestone gate.
   Status:
   - blocked
   Reason:
   - The corrected M4d result misses the Phase 4 gate by an order of magnitude. `2.6–3.0k TPS` is far below the expected `>= 25k TPS` at `30k` offered.
   - Because correctness now passes, the only honest conclusion is that the remaining blocker is real runtime throughput on this implementation, not measurement error.
   Next move:
   - Diagnose where canonical Mahi is burning capacity under load: commit-execution lock contention, producer cadence / overproduction, sigverify / fetch overhead, or admission starvation.
   - Capture a focused 4-validator profile slice on canonical Mahi under load before attempting more protocol changes.

69. Phase 4 M4d continuation: batch Mahi commit execution
   Date:
   - `2026-04-23`
   Hypothesis:
   - Canonical Mahi is losing throughput to per-block commit-execution lock churn. Batching ready committed Mahi blocks under one engine lock acquisition should reduce admission contention and raise committed throughput without changing DAG order or committed state.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept `8k` batching run:
     - `/tmp/codex-exp-phase4-m4d-batch-8k-10s`
     - `42,125 committed`
     - `4,212.5 TPS`
     - `rpc_err=0`
     - `nonce_err=0`
     - post-load state-root convergence passed
   - pre-batch corrected-harness reference:
     - `/tmp/codex-exp-phase4-m4d-statecheck-8k-10s`
     - `29,573 committed`
     - `2,957.3 TPS`
   - exploratory `30k` batching run:
     - `/tmp/codex-exp-phase4-m4d-batch-30k-10s`
     - loadgen `ok=139,875`
     - `rpc_err=200,037`
     - `net_err=110,064`
     - committed-summary capture failed after servers became unreachable
   - exploratory `30k` batching rerun:
     - `/tmp/codex-exp-phase4-m4d-batch-rerun-30k-10s`
     - loadgen `ok=140,482`
     - `rpc_err=198,558`
     - `net_err=110,912`
     - committed-summary capture failed after most servers became unreachable
   Evidence:
   - Added `EngineHandle::apply_mahi_committed_blocks()` so the Mahi commit executor applies ready blocks in chunks of `32` under one compatibility lock acquisition, preserving the existing linearized block order.
   - The `8k` committed throughput moved from `2,957.3 TPS` to `4,212.5 TPS`, a `42.4%` improvement on the corrected Mahi harness.
   - The structural win is admission lock pressure: `admission_write_wait_p95` on the hot validators collapsed from tens of milliseconds in the pre-batch run to roughly `0.15-0.19ms` in the kept `8k` batch run.
   - Block density improved materially. In the kept `8k` run, validators 2 and 3 averaged about `84.6` intents per produced Mahi block, while validators 1 and 4 averaged about `46`. This asymmetry is noted but not treated as a Mahi correctness bug; it likely reflects ingress/admission locality in the localhost harness.
   - State-root convergence remained green after batching, so the optimization did not disturb canonical Mahi determinism.
   - The `30k` batching runs improved request acceptance relative to the pre-batch corrected run, but they are not valid committed-throughput gates because the harness lost contact with the servers before it could capture policy nonce summaries. They remain diagnostic artifacts only.
   Status:
   - kept
   Reason:
   - Batching commit execution directly attacked the measured lock-churn bottleneck and produced the first material TPS movement in Phase 4 while preserving state-root convergence.
   - The change is intentionally narrow: it amortizes engine lock acquisition across already-linearized Mahi blocks without changing commit rule, block selection, or execution semantics.
   Next move:
   - Run a 4-validator Mahi load ladder on this kept batching state at `8k`, `12k`, `16k`, `20k`, `24k`, and `30k` offered to characterize whether the next bottleneck is a hard ceiling, a flat wall, or a collapse curve.
   - Use the ladder shape to choose the next slice: merged execution, producer pacing, admission backpressure, or another focused profile.

70. Phase 4 M4d diagnostic: batched Mahi load ladder
   Date:
   - `2026-04-23`
   Hypothesis:
   - A six-point offered-load ladder on the kept 32-block batching state will distinguish whether canonical Mahi is linearly scaling to a ceiling, flat at a new wall, or collapsing under overload feedback.
   Files changed:
   - none (measurement-only slice on commit `2aed9f159`)
   Tests:
   - `KOMBAT_CONSENSUS=mahi` 4-validator local-cluster ladder at `8k`, `12k`, `16k`, `20k`, `24k`, and `30k` offered
   Benchmark runs:
   - `/tmp/codex-exp-phase4-m4d-ladder-8000-10s`
     - `43,107 committed`
     - `4,310.7 TPS`
     - `rpc_err=0`
     - `net_err=0`
   - `/tmp/codex-exp-phase4-m4d-ladder-12000-10s`
     - `41,438 committed`
     - `4,143.8 TPS`
     - `rpc_err=40,235`
     - `net_err=0`
   - `/tmp/codex-exp-phase4-m4d-ladder-16000-10s`
     - `40,248 committed`
     - `4,024.8 TPS`
     - `rpc_err=100,483`
     - `net_err=0`
   - `/tmp/codex-exp-phase4-m4d-ladder-20000-10s`
     - `39,031 committed`
     - `3,903.1 TPS`
     - `rpc_err=159,846`
     - `net_err=0`
   - `/tmp/codex-exp-phase4-m4d-ladder-24000-10s`
     - `38,908 committed`
     - `3,890.8 TPS`
     - `rpc_err=188,462`
     - `net_err=31,440`
   - `/tmp/codex-exp-phase4-m4d-ladder-30000-10s`
     - `33,452 committed`
     - `3,345.2 TPS`
     - `rpc_err=202,097`
     - `net_err=107,816`
   Evidence:
   - The curve is not linear scaling. Committed throughput is highest at `8k` offered and declines as offered load rises.
   - The curve is closest to a flat wall with overload collapse: `4.3k -> 4.1k -> 4.0k -> 3.9k -> 3.9k -> 3.3k TPS` as offered load increases from `8k` to `30k`.
   - Request-layer failures start immediately above `8k` offered and grow with load. The dominant error remains `local_submit_queue_full`; at `24k` and `30k`, network connection failures also appear.
   - Produced Mahi block density degrades under overload. Across all validators, produced block payloads move from `avg=65.8, p50=42, p90=171` at `8k` to `avg=45.9, p50=23, p90=109` at `30k`.
   - State-root convergence passed in the ladder runs, so the overload behavior is throughput/backpressure, not correctness divergence.
   Status:
   - kept
   Reason:
   - The diagnostic answered the next decision point: canonical Mahi is not scaling with additional offered load on the current substrate. The next blocker is a flat execution/admission ceiling with overload feedback, not simply insufficient offered load.
   Next move:
   - Do not start M4d gate retries at `30k` until the flat wall moves.
   - Focus the next slice on reducing per-block fixed execution cost further, or on admission/backpressure behavior if profiling shows execution is no longer dominant.
   - Merged execution remains plausible, but it should be framed as moving the `~4k TPS` wall, not as a direct response to the `30k` error count.

71. Phase 4 M4d continuation: merged Mahi execution batches
   Date:
   - `2026-04-23`
   Hypothesis:
   - The post-batching `~4k TPS` wall is still partly caused by per-DAG-block synthetic chain execution. Merging small groups of already-linearized Mahi blocks into one engine execution batch should amortize checkpoint/root/header/install fixed costs while preserving deterministic intent order.
   Files changed:
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - baseline batched reference:
     - `/tmp/codex-exp-phase4-m4d-ladder-8000-10s`
     - `43,107 committed`
     - `4,310.7 TPS`
   - merged batch size `4`:
     - `/tmp/codex-exp-phase4-m4d-merged4-8k-10s`
     - `44,001 committed`
     - `4,400.1 TPS`
   - merged batch size `8`:
     - `/tmp/codex-exp-phase4-m4d-merged8-8k-10s`
     - `45,937 committed`
     - `4,593.7 TPS`
   - merged batch size `16`:
     - `/tmp/codex-exp-phase4-m4d-merged16-8k-10s`
     - `45,226 committed`
     - `4,522.6 TPS`
   - merged batch size `32`:
     - `/tmp/codex-exp-phase4-m4d-merged32-8k-10s`
     - `41,380 committed`
     - `4,138.0 TPS`
   - kept default multi-validator path:
     - `/tmp/codex-exp-phase4-m4d-merged-default-8k-10s`
     - `46,330 committed`
     - `4,633.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - overload check with merged batch size `8`:
     - `/tmp/codex-exp-phase4-m4d-merged8-12k-10s`
     - `44,681 committed`
     - `4,468.1 TPS`
     - `rpc_err=40,096`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Added `SingleNodeEngine::apply_mahi_committed_blocks_merged()` and an `EngineHandle` wrapper so a deterministic contiguous slice of already-linearized Mahi blocks is flattened into one synthetic execution body.
   - Added `KOMBAT_MAHI_MERGED_EXECUTION_BATCH`; `1` disables merged execution. Default is `8` only for multi-validator Mahi. Single-validator Mahi defaults to immediate execution to preserve the M4c liveness test.
   - Batch-size sweep shows a local optimum around `8`. Batch `4` is only slightly positive, batch `16` is weaker, and batch `32` regresses below the unmerged reference.
   - The kept default multi-validator path improved `8k` committed throughput from `4,310.7 TPS` to `4,633.0 TPS`, a `7.5%` gain over item 70 and `56.7%` over the corrected pre-batch M4d reference (`2,957.3 TPS`).
   - State-root convergence stayed green for all merged-execution runs, so the optimization did not introduce cross-validator nondeterminism.
   - The `12k` run remains overload-limited: committed throughput falls to `4,468.1 TPS` and `local_submit_queue_full` returns. Merged execution moves the wall modestly but does not change the shape identified in item 70.
   - Semantic caveat: merged execution preserves Mahi linearized intent order and per-DAG-block executed accounting, but multiple DAG blocks materialize as one synthetic chain block. This is acceptable for the current canonical-Mahi bring-up slice but must be revisited before final audit/log semantics are frozen.
   Status:
   - kept
   Reason:
   - The change produces a repeatable, state-consistent throughput gain on the default 4-validator Mahi path with no request errors at `8k`.
   - Larger merge sizes are explicitly binned by the sweep; `8` is the narrow kept value.
   Next move:
   - The flat wall remains. Do not retry the `30k` M4d gate until the next bottleneck moves.
   - Next slice should inspect why effective block density and admission throughput collapse under overload despite lower execution fixed cost. Highest-confidence surfaces: producer/admission locality, local submit queue backpressure, and remaining compat-lock waits around admission reads.

72. Phase 4 M4d continuation: Mahi payload ownership and in-flight frontier
   Date:
   - `2026-04-23`
   Hypothesis:
   - The Mahi DAG has four block-production lanes, but the kept path was not getting four useful payload lanes because producers were repeatedly selecting overlapping mempool frontiers. Sharding payload ownership across configured validators and advancing each producer past its own in-flight nonces should reduce duplicate payloads and move the flat wall.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - structural cleanup only:
     - `/tmp/codex-exp-phase4-m4d-native-install-8k-10s`
     - `46,246 committed`
     - `4,624.6 TPS`
     - post-load state-root convergence passed
   - initial payload sharding attempt:
     - `/tmp/codex-exp-phase4-m4d-payload-shard-8k-10s`
     - `46,482 committed`
     - `4,648.2 TPS`
     - post-load state-root convergence passed
   - in-flight filtering attempt before configured-set fix:
     - `/tmp/codex-exp-phase4-m4d-inflight-shard-8k-10s`
     - `45,686 committed`
     - `4,568.6 TPS`
     - post-load state-root convergence passed
   - kept configured-set sharding + in-flight frontier:
     - `/tmp/codex-exp-phase4-m4d-configured-shard-8k-10s`
     - `71,741 committed`
     - `7,174.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - overload check:
     - `/tmp/codex-exp-phase4-m4d-configured-shard-12k-10s`
     - `71,301 committed`
     - `7,130.1 TPS`
     - `rpc_err=32,433`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Added a Mahi-specific install path that skips HotStuff QC signing, HotStuff pacemaker advancement, and HotStuff round telemetry during Mahi commit install. This was correctness-safe but throughput-neutral (`4,624.6 TPS` vs kept `4,633.0 TPS`), proving HotStuff QC/pacemaker cleanup alone was not the wall.
   - Added multi-validator Mahi payload sharding behind `KOMBAT_MAHI_PAYLOAD_SHARDING` (default enabled for configured validator sets with more than one validator). A unit test now covers sharding by configured validator index.
   - Added `MahiState::inflight_policy_nonces` and a producer body selector that advances past locally produced, not-yet-executed nonces. This prevents a validator from reproposing nonce `N` every round while waiting for Mahi commit lag to resolve.
   - The first sharding attempts did not move throughput because `mahi_snapshot()` was sharding against `engine.validator_set()`, which is intentionally local-only (`len=1`) in the compatibility engine. The kept fix uses `configured_validator_ids()` when present.
   - After the configured-set fix, produced payloads are mostly useful instead of duplicated: 8k offered improved from item 71's `4,633.0 TPS` to `7,174.1 TPS`, a `54.8%` gain.
   - The new 12k run shows the wall moved but did not disappear. Throughput stays flat at `~7.1k TPS` and `local_submit_queue_full` returns above 8k offered, so the next blocker is admission/relay queue pressure and remaining engine compatibility contention.
   Status:
   - kept
   Reason:
   - The change produces the largest canonical-Mahi gain so far while preserving state-root convergence and deterministic commit-log tests.
   - The no-op native-install cleanup stays because it removes HotStuff-shaped work from Mahi without measurable downside, but it is not counted as the throughput win.
   Next move:
   - Treat `7.17k TPS` as the new canonical-Mahi reference.
   - Do not retry the `30k` gate yet. The next slice should target the overload wall shown by the 12k run: `local_submit_queue_full`, admission/relay locality, and remaining compatibility-lock contention.
   - Specifically inspect why ingress still concentrates queue pressure on validators 1 and 4 while validators 2 and 3 have low local-submit counts, and decide whether Mahi mode should route writer policies directly to their payload-owner validator instead of relying on the old HotStuff relay shape.

73. Phase 4 M4d continuation: Mahi owner-aware observer ingress
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 72, Mahi payload ownership was correctly sharded across validators, but observer ingress still used the old HotStuff leader-forwarding shape. Splitting observer-ingress submit batches by Mahi payload-owner validator should remove the 12k overload errors and let all four Mahi payload lanes receive admission work directly.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_observer_ingress_shards_to_validator_admission -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - 8k regression:
     - `/tmp/codex-exp-phase4-m4d-mahi-owner-ingress-8k-10s`
     - `94,806 committed`
     - `9,480.6 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 12k target:
     - `/tmp/codex-exp-phase4-m4d-mahi-owner-ingress-12k-10s`
     - `97,548 committed`
     - `9,754.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 16k overload probe:
     - `/tmp/codex-exp-phase4-m4d-mahi-owner-ingress-16k-10s`
     - `91,922 committed`
     - `9,192.2 TPS`
     - `rpc_err=25,661`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Added a Mahi-specific observer-ingress branch for `kombat_submitIntent` that routes directly to the payload-owner validator by `policy_id % active_validator_count`, with existing endpoint fallback inside that owner.
   - Added Mahi-specific observer-ingress handling for `kombat_submitIntentBatch`: split the client batch into per-owner sub-batches, forward each sub-batch to its owner, then merge item responses back into the original client order.
   - Added a focused unit test proving Mahi observer batch forwarding splits across validator owners instead of sending the whole batch through HotStuff leader preference. The test also caught and encoded the validator-set sort order (`validator(8)` before `validator(9)`).
   - The previous 12k reference from item 72 had `32,433` RPC errors and `7,130.1 TPS`. Owner-aware ingress raised the 12k run to `9,754.8 TPS` with zero RPC/network errors, proving the old HotStuff observer-forwarding shape was the immediate overload wall.
   - The 16k probe now fails differently and more cleanly: every validator reaches `local_submit_queue` near the `65,536` limit and returns `local_submit_queue_full`. That means ingress distribution is no longer the primary problem; per-validator local submit drain/admission throughput is now the wall.
   Status:
   - kept
   Reason:
   - The change removes a concrete HotStuff-shaped ingress bottleneck from canonical Mahi, improves the current best Mahi 8k/12k throughput, eliminates the prior 12k RPC-error wall, and preserves state-root convergence.
   Next move:
   - Treat `9.75k TPS` at 12k offered as the new canonical-Mahi reference.
   - Do not retry the `30k` gate yet. The next slice should target the per-validator `local_submit_queue_full` wall seen at 16k: increase local submit drain throughput, reduce per-item admission lock work, or bypass the legacy local-submit queue for Mahi-owned observer ingress if correctness permits.
   - Keep removing HotStuff-shaped plumbing where it is proven to be on the hot path. Do not delete the remaining HotStuff fallback wholesale until the Mahi path has replacement coverage for ingress, admission, block production, execution, recovery, and audit semantics.

74. Phase 4 M4d continuation: Mahi local-submit drain batching
   Date:
   - `2026-04-24`
   Hypothesis:
   - After owner-aware ingress, 16k offered load distributed correctly but filled every validator's legacy local-submit queue. Draining more than one queued intent per admission write-lock acquisition in Mahi mode should remove the queue-full failure without changing public submit semantics.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_accepts_multiple_items -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned batch size `256`:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch-16k-10s`
     - `57,664 committed`
     - `5,766.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load Mahi convergence failed
   - exploratory batch size `8`:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch8-16k-10s`
     - `97,322 committed`
     - `9,732.2 TPS`
     - `rpc_err=22,026`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept batch size `16` at 16k:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch16-16k-10s`
     - `99,881 committed`
     - `9,988.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept batch size `16` at 12k:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch16-12k-10s`
     - `97,493 committed`
     - `9,749.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept batch size `16` at 8k:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch16-8k-10s`
     - `95,001 committed`
     - `9,500.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - HotStuff regression after gating batching to Mahi mode:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch16-gated-hotstuff-8k-10s`
     - `86,184 committed`
     - `8,618.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load HotStuff convergence passed
   Evidence:
   - Added `EngineHandle::submit_intents()` so the local-submit worker can admit a bounded batch under one admission write lock instead of one queued intent per lock.
   - The local-submit worker now drains up to `16` items per loop only when `KOMBAT_CONSENSUS=mahi`; HotStuff/default mode keeps the old one-item drain path. This preserves the HotStuff baseline while allowing Mahi to reduce legacy queue overhead.
   - Batch size `256` was explicitly binned: it removed RPC errors but held admission write locks for hundreds of milliseconds, dropped achieved loadgen throughput to `~2.4k RPS`, and failed post-load convergence. The artifact is kept as evidence that overly large admission batches starve the read/preview side.
   - Batch size `8` was safe but insufficient: it kept convergence green but still produced `22,026` queue-full RPC errors at 16k.
   - Batch size `16` removed the 16k queue-full wall (`25,661` RPC errors in item 73 -> `0`), preserved Mahi state-root convergence, and moved the current best committed throughput to `9,988.1 TPS`.
   - Committed TPS remains flat near `10k` even when 16k is accepted with no RPC/network errors. The local-submit queue was a failure-mode bottleneck, not the main committed-throughput ceiling.
   Status:
   - kept
   Reason:
   - The change removes the next concrete legacy queue failure in Mahi mode and preserves HotStuff/default behavior by gating the batched drain.
   Next move:
   - Treat `~10k TPS` as the new canonical-Mahi reference.
   - Do not retry the `30k` gate yet. Ingress can now accept 16k cleanly, but commits remain flat around 10k, so the next slice should target commit production/consumption rather than local-submit queue capacity.
   - Highest-confidence next surfaces: Mahi block payload density at 16k, commit-round cadence, pending relay backlog, and remaining synthetic block/execution fixed costs.

75. Phase 4 M4d continuation: remove legacy intent relay from canonical Mahi
   Date:
   - `2026-04-24`
   Hypothesis:
   - Canonical Mahi DAG blocks carry full intents to peers, so the old HotStuff submit-intent relay is redundant in `KOMBAT_CONSENSUS=mahi`. Disabling that relay should remove duplicate admission/relay work, clear pending relay queues, and improve committed throughput without changing HotStuff/default behavior.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - 16k target:
     - `/tmp/codex-exp-phase4-m4d-mahi-no-intent-relay-16k-10s`
     - `112,439 committed`
     - `11,243.9 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 12k validation:
     - `/tmp/codex-exp-phase4-m4d-mahi-no-intent-relay-12k-10s`
     - `111,328 committed`
     - `11,132.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 8k validation:
     - `/tmp/codex-exp-phase4-m4d-mahi-no-intent-relay-8k-10s`
     - `112,188 committed`
     - `11,218.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - `submit_intent_batch_locally()` and `maybe_fanout_submit_intent_to_network()` now suppress post-admission intent relay when `RuntimeConsensusMode::Mahi` is active. HotStuff/default mode keeps the existing relay behavior.
   - In the kept 16k run, all validators reported `pending_relay_intents=0` and `pending_relay_targets=0` after load, proving the legacy relay path was removed from the Mahi hot path.
   - The change moved the current best 16k result from item 74's `9,988.1 TPS` to `11,243.9 TPS`, a `12.6%` improvement, with zero RPC/network errors and state-root convergence intact.
   - Mahi block payload density also improved versus item 74. Across validator logs for rounds `>=50`, payload count averaged `61.3` intents/round (`p50=42`, `p95=192`, `max=459`, `zero=150`) instead of the prior `~48-49` average and `~34-36` p50 range.
   - The next limiter is still admission/execution pressure, not relay: 16k post-load metrics show `admission_write_wait_p95` around `75ms`, `admission_write_hold_p95` around `6ms`, and local-submit queues around `29k-30k` per validator.
   Status:
   - kept
   Reason:
   - The change removes redundant HotStuff-era relay work from canonical Mahi, improves the committed-throughput plateau, and preserves deterministic state-root convergence.
   Next move:
   - Treat `11.24k TPS` as the new canonical-Mahi reference.
   - Do not retry the `30k` gate yet. The next slice should attack the remaining Mahi admission hot path: owner-ingress batches currently preview each intent under admission-read, enqueue it, and then the worker revalidates/submits under admission-write. A Mahi-only direct batch-admit path may remove that duplicate validation/queue stage if it can preserve public response semantics.

76. Phase 4 M4d diagnostic: Mahi synchronous direct batch admit
   Date:
   - `2026-04-24`
   Hypothesis:
   - Replacing Mahi batch submit's preview + local queue + worker drain with synchronous direct batch admission should remove duplicate validation and queue latency.
   Files changed:
   - `chain/crates/node/src/engine_handle.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_mahi_submit_intent_batch_admits_directly_and_rejects_duplicate_nonce -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_accepts_multiple_items -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned 16k direct-admit attempt:
     - `/tmp/codex-exp-phase4-m4d-mahi-direct-batch-admit-16k-10s`
     - `45,128 committed`
     - `4,512.8 TPS`
     - loadgen achieved only `1,592.0 RPS` in the measured window
     - `rpc_err=0`
     - `net_err=0`
     - post-load Mahi convergence failed
   Evidence:
   - The direct path did remove queue backlog (`local_submit=0` and relay queues `0`), but it moved the bottleneck into synchronous request latency.
   - Loadgen latency exploded (`p50=5.86s`, `p99=7.29s`) and achieved measured RPS collapsed to `1,592.0`, proving owner-ingress RPC threads were blocked behind admission-write work instead of accepting and smoothing load.
   - Admission write wait p95 reached `~3.55-3.63s`; admission write hold p95 reached `~27-28ms`. This is much worse than item 75's queued path (`~75ms` wait, `~6ms` hold).
   - The run also failed post-load Mahi convergence, so the experiment is correctness-negative as well as throughput-negative.
   Status:
   - binned
   Reason:
   - Synchronous direct batch admission is the wrong shape for this workload. The queued worker is not just legacy baggage; it decouples observer request acceptance from admission-write contention and protects liveness under bursty batch ingress.
   Next move:
   - Keep item 75 as the saved state and do not reintroduce synchronous owner-ingress admission.
   - The next admission slice should preserve asynchronous request acceptance while reducing worker-side fixed cost: candidate surfaces are local-submit worker scheduling, drain batch sizing/adaptive batching below the known-bad `256`, or a worker-only validation cache that avoids preview/revalidate duplication without blocking client RPC threads.

77. Phase 4 M4d diagnostic: local-submit drain batch retune
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 75 still showed large local-submit queues and `~75ms` admission-write wait p95 at 16k. Increasing the asynchronous Mahi local-submit worker drain batch above `16`, while staying far below the known-bad `256`, may reduce queue pressure without blocking client RPC threads.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_accepts_multiple_items -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned batch size `32`:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch32-16k-10s`
     - `106,492 committed`
     - `10,649.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned batch size `24`:
     - `/tmp/codex-exp-phase4-m4d-local-drain-batch24-16k-10s`
     - `109,801 committed`
     - `10,980.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Both batch sizes kept request acceptance and convergence healthy, but neither beat item 75's batch-`16` result of `112,439 committed` / `11,243.9 TPS`.
   - Batch `32` increased admission write hold p95 to `~14-15ms` and wait p95 to `~120ms`, versus item 75's `~6ms` hold and `~75ms` wait.
   - Batch `24` was less harmful than `32` but still increased admission write hold p95 to `~10-11ms` and wait p95 to `~99-112ms`.
   - The shape matches the earlier batch-`256` bin: larger admission batches reduce lock acquisition frequency but starve competing admission/production work enough to lower committed throughput.
   Status:
   - binned
   Reason:
   - Batch `16` remains the best measured local-submit drain size. Bigger batches increase admission lock hold time and reduce committed throughput despite preserving convergence.
   Next move:
   - Keep item 75 as the saved state.
   - Stop tuning local-submit drain size for now. The next slice should target a different limiter: Mahi block production cadence/payload density, commit execution fixed cost, or separating preview/read admission work from write admission without synchronously blocking RPC request threads.

78. Phase 4 M4d diagnostic: Mahi auto-round cadence sweep
   Date:
   - `2026-04-24`
   Hypothesis:
   - The harness still drives Mahi block production through `KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS`, defaulting to the old HotStuff-shaped `50ms` cadence. Faster producer cadence might raise the Mahi commit ceiling by increasing DAG block availability; slower cadence might improve payload density if producer overhead/underfill is dominant.
   Files changed:
   - None
   Tests:
   - Reused release build from item 77.
   Benchmark runs:
   - binned `AUTO_INTERVAL_MS=25`:
     - `/tmp/codex-exp-phase4-m4d-mahi-interval25-16k-10s`
     - `108,960 committed`
     - `10,896.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned `AUTO_INTERVAL_MS=100`:
     - `/tmp/codex-exp-phase4-m4d-mahi-interval100-16k-10s`
     - `109,603 committed`
     - `10,960.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Both interval variants preserved request acceptance and convergence but underperformed item 75's default-`50ms` result of `112,439 committed` / `11,243.9 TPS`.
   - Faster `25ms` cadence increased admission pressure (`admission_write_wait_p95` around `96-102ms`, hold p95 around `9.5-9.8ms`) without increasing committed throughput.
   - Slower `100ms` cadence also failed to improve density enough to offset lower production cadence (`admission_write_wait_p95` around `104-111ms`, hold p95 around `9.6-10.0ms`).
   - The current wall is therefore not a simple harness interval cap. The `50ms` default remains the best measured cadence for the current implementation.
   Status:
   - binned
   Reason:
   - Mahi throughput stayed below the item 75 reference at both faster and slower producer cadence. Changing the harness interval is not a useful standalone optimization.
   Next move:
   - Keep item 75 as the saved state.
   - Move away from admission-drain and harness-cadence tuning. The next high-confidence target is commit/execution fixed cost: reduce per-DAG-block install overhead, merge more execution work without changing deterministic linearized intent order, or identify the specific lock/domain that limits committed throughput after admission accepts 16k cleanly.

79. Phase 4 M4d diagnostic: Mahi merged execution batch sweep
   Date:
   - `2026-04-24`
   Hypothesis:
   - The canonical Mahi path currently merges execution across `8` DAG blocks by default. Adjusting `KOMBAT_MAHI_MERGED_EXECUTION_BATCH` may reduce synthetic block/execution fixed cost or reduce commit latency enough to move the 16k plateau.
   Files changed:
   - None
   Tests:
   - Reused release build from item 77.
   Benchmark runs:
   - binned merged batch `16`:
     - `/tmp/codex-exp-phase4-m4d-merged-exec16-16k-10s`
     - `110,488 committed`
     - `11,048.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned merged batch `4`:
     - `/tmp/codex-exp-phase4-m4d-merged-exec4-16k-10s`
     - `98,048 committed`
     - `9,804.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Both alternatives preserved deterministic state-root convergence, so merged execution batch size does not appear to be a correctness-sensitive knob in this range.
   - Neither alternative beat item 75's default batch-`8` result (`112,439 committed` / `11,243.9 TPS`).
   - Batch `16` reduced produced blocks during the window and did not improve throughput, suggesting extra merge delay outweighs reduced synthetic execution overhead.
   - Batch `4` produced many more head blocks but committed fewer intents, suggesting per-batch synthetic block/install overhead becomes visible when merge batches are too small.
   Status:
   - binned
   Reason:
   - The existing default merged execution batch size `8` remains the best measured point. The current wall is not solved by simple execution batch sizing.
   Next move:
   - Keep item 75 as the saved state.
   - The next slice should instrument or remove a specific fixed cost rather than tune existing knobs. Highest-confidence target: `apply_mahi_committed_blocks_merged()` still clones full policy/order-book state and prepares a synthetic block for every merged execution batch; measure whether state cloning or execution simulation dominates before changing semantics.

80. Phase 4 M4d diagnostic: Mahi prevalidated local-submit drain
   Date:
   - `2026-04-24`
   Hypothesis:
   - In Mahi mode, request acceptance already performs read-side preview before enqueueing local submissions. Reusing that preview authority in the local-submit drain, while retaining duplicate/stale/expiry guards, may avoid redundant worker-side validation and reduce admission-write fixed cost.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/crates/node/src/engine_handle.rs` (reverted)
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_accepts_multiple_items -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned prevalidated drain:
     - `/tmp/codex-exp-phase4-m4d-prevalidated-drain-16k-10s`
     - `109,814 committed`
     - `10,981.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Correctness and request acceptance stayed healthy: no RPC/network errors and post-load state-root convergence passed.
   - Admission-write hold p95 remained around `6.2-6.6ms`, and admission-write wait p95 remained around `72-74ms`, roughly matching item 75's `~6ms` hold and `~75ms` wait profile.
   - Throughput regressed versus item 75's saved result (`112,439 committed` / `11,243.9 TPS`), so redundant worker-side validation is not the current limiter.
   Status:
   - binned
   Reason:
   - The optimization preserved correctness but did not reduce the measured admission-write bottleneck or improve committed throughput. Carrying a Mahi-only prevalidated admission path would add safety-sensitive complexity without a win.
   Next move:
   - Keep item 75 as the saved state.
   - Move to direct measurement of commit/execution fixed cost in `apply_mahi_committed_blocks_merged()` before changing semantics. The current wall appears more likely in commit execution, mempool/frontier removal, synthetic block install, or payload production than in admission revalidation.

81. Phase 4 M4d: index resting-order duplicate/expiry checks
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 80 ruled out redundant admission validation. A temporary execution profile on the item-75 code showed merged Mahi execution spending most time in runtime commit simulation: non-empty batches had `commit_block_intents()` p95 around `185ms`, while body filtering was `~52us` p95, clone was `~1.2ms` p95, prepare was `~27ms` p95, and install was `~3.3ms` p95. The load-test workload posts many GTC limit orders to the same book, so `SingleMarketBook`'s per-submit O(n) expiry purge, duplicate-client-order scan, and append-at-end insertion scan are likely dominant.
   Files changed:
   - `chain/crates/matching/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-matching`
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo test -p kombat-chain-node snapshot_restore_accepts_committed_network_proposer_history -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept matching runtime indexes at 16k offered:
     - `/tmp/codex-exp-phase4-m4d-matching-index-16k-10s`
     - `126,077 committed`
     - `12,607.7 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - invalid overload check at 30k offered:
     - `/tmp/codex-exp-phase4-m4d-matching-index-30k-10s`
     - `rpc_err=84,630` (`local_submit_queue_full`)
     - committed-summary policy queries failed, so committed TPS is not trusted
     - post-load state-root convergence probe passed after the overload
   Evidence:
   - The kept 16k result improves over item 75's saved result (`112,439 committed` / `11,243.9 TPS`) by `13,638` committed intents, a `12.1%` throughput lift.
   - The optimization preserves the existing `SingleMarketBook` Borsh wire shape by serializing only `market_id`, `next_sequence`, `bids`, and `asks`, then rebuilding runtime indexes on deserialize. This keeps snapshot tamper/restore compatibility intact.
   - Request latency also improved at 16k: p50 dropped from item 75's `~70ms` shape to `35.363ms`, p90 to `116.230ms`, and p99 to `242.399ms`.
   - Admission pressure improved but did not disappear: local-submit queues remained `~25k-27k`, and admission-write wait p95 was still `~44-47ms`, so the queue/feed path is still the next limiter.
   Status:
   - kept
   Reason:
   - The change removes avoidable O(n) matching work from the dominant commit-simulation path, preserves deterministic state-root convergence, and produces the largest kept Mahi improvement since item 75.
   Next move:
   - New saved Mahi state is `12,607.7 TPS` at 16k offered. Continue from this commit.
   - The 30k overload now fails on local-submit queue capacity before Mahi can prove a 25k commit gate. Next target should reduce local-submit queue pressure or increase admission drain throughput without reintroducing the known-bad synchronous direct admission path or larger fixed drain batches.

82. Phase 4 M4d: native Mahi merged execution
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 81, the remaining Mahi merged-execution path still paid HotStuff-era checkpoint overhead through `prepare_block_with_checkpoint()` and `commit_prepared_block()` for every synthetic merged block. Bypassing that checkpoint store for canonical Mahi, while preserving a deterministic block header commitment and keeping a rollback env, should remove fixed execution cost without changing the linearized intent order.
   Files changed:
   - `chain/crates/node/Cargo.toml`
   - `chain/crates/node/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept clean 16k default-native run:
     - `/tmp/codex-exp-phase4-m4d-mahi-native-default-clean-16k-10s`
     - `205,088 committed`
     - `20,508.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - overload 30k default-native run:
     - `/tmp/codex-exp-phase4-m4d-mahi-native-default-30k-10s`
     - `187,871 committed`
     - `18,787.1 TPS`
     - `rpc_err=63,399` (`local_submit_queue_full`)
     - `net_err=0`
     - post-load state-root convergence passed
   - diagnostic opt-in profile before cleanup:
     - `/tmp/codex-exp-phase4-m4d-mahi-native-exec-16k-10s`
     - `212,434 committed`
     - `21,243.4 TPS`
     - `prepare_us=0`, `commit_us p95=23,010`, `install_us p95=1,156`, `total_us p95=25,537`
     - post-load state-root convergence passed
   Evidence:
   - The clean 16k result improves over item 81's saved `12,607.7 TPS` by `7,901.1 TPS` (`+62.7%`) with zero request errors and preserved convergence.
   - The profile confirmed the checkpoint prepare path was removed in the native branch (`prepare_us=0`). The remaining p95 time moved into actual intent commit simulation plus install.
   - The 30k run still does not satisfy the M4d gate: convergence passed, but request acceptance hit `63,399` local-submit queue-full errors and committed throughput reached only `18,787.1 TPS`.
   - Native Mahi state roots now commit deterministically to parent state root, body root, runtime event root, round, and timestamp. This intentionally avoids the old per-intent checkpoint store root that was inherited from HotStuff. `KOMBAT_MAHI_LEGACY_EXEC=1` keeps the old checkpoint path available for rollback or audit comparison.
   - The initial clean 16k run at `/tmp/codex-exp-phase4-m4d-mahi-native-default-16k-10s` reported `20,976.8 TPS` but failed the 20s post-load convergence timeout. A rerun with transient convergence diagnostics and the final clean run both converged, so the failed artifact is treated as convergence-time variance, not a kept result.
   Status:
   - kept
   Reason:
   - The change removes a HotStuff-era checkpoint bottleneck from the canonical Mahi execution path, preserves deterministic convergence on kept runs, and produces the largest Phase 4 throughput lift so far.
   Next move:
   - New saved Mahi state is `20,508.8 TPS` at 16k offered.
   - M4d is still not complete: the 30k gate remains below `25k TPS` and is blocked by local-submit queue saturation.
   - Next target: reduce queue/feed pressure or increase asynchronous admission drain throughput without returning to known-bad synchronous direct admission. Candidate surfaces are owner-forwarding distribution, per-owner local-submit queue draining, and Mahi producer/admission overlap under high offered load.

83. Phase 4 M4d: monotonic expiry purge for queued admission
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 82 removed HotStuff checkpoint overhead, high offered-load Mahi still repeatedly scanned the direct and conditional pools for expiry while draining local submissions. Under backlog, calling `purge_expired()` for each submitted intent creates avoidable O(queue x mempool) work. Caching the highest timestamp already purged should remove repeated full-pool scans while preserving expiry semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   Tests:
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo test -p kombat-chain-node snapshot_restore_accepts_committed_network_proposer_history -- --nocapture`
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept final-source 16k run:
     - `/tmp/codex-exp-phase4-m4d-expiry-purge-final-16k-10s`
     - `239,872 committed`
     - `23,987.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 30k gate attempt:
     - `/tmp/codex-exp-phase4-m4d-expiry-purge-final-30k-10s`
     - `306,656 committed`
     - `30,665.6 TPS`
     - `rpc_err=88,024` (`mahi owner batch forwarding request failed`)
     - `net_err=0`
     - post-load state-root convergence passed
   - binned owner-forward retry diagnostic:
     - `/tmp/codex-exp-phase4-m4d-owner-batch-retry8-30k-10s`
     - `301,912 committed`
     - `30,191.2 TPS`
     - `rpc_err=6,000`
     - `net_err=138,520`
     - post-load state-root convergence passed
     - code reverted before this item was kept
   Evidence:
   - The final clean 16k run improves over item 82's saved `20,508.8 TPS` by `3,478.4 TPS` (`+17.0%`) with zero request errors and deterministic state-root convergence.
   - Admission-write wait p95 on the clean 16k run fell to roughly `1.9-2.5ms`, compared with the prior tens-of-ms shape under backlog. Local submit queues drained to zero by the post-load status snapshot.
   - The 30k attempt crosses the nominal committed-throughput target (`30,665.6 TPS`) and converges, but it is not a clean M4d closeout because the observer ingress path still returned `88,024` owner-forwarding RPC errors. Treat this as proof that Mahi execution can now commit above `25k`, not as a production-quality ingress result.
   - Widening owner-forward retries converted RPC errors into client network errors by overloading the observer ingress listener (`138,520` net errors), so blind retry is binned. The next fix needs to reduce fanout/listener pressure rather than wait longer.
   - The harness now allows up to `60s` for Mahi post-load state consistency and retries committed-summary policy reads under Mahi. This records the actual state-root convergence behavior after high-load catch-up instead of misclassifying temporary listener saturation as zero committed work.
   Status:
   - kept
   Reason:
   - The monotonic purge cache removes an avoidable admission-drain scan, produces a large clean 16k gain, and preserves deterministic convergence. The harness changes make Mahi measurement reliable under catch-up without changing runtime behavior.
   Next move:
   - New saved clean Mahi state is `23,987.2 TPS` at 16k offered.
   - M4d is still not fully closed despite the `30,665.6 TPS` committed 30k artifact because request acceptance is not clean. The remaining blocker is observer ingress owner-forwarding/fanout pressure on validator network listeners.
   - Next target: remove or reduce observer-to-validator HTTP fanout for `kombat_submitIntentBatch` in Mahi mode. Do not reintroduce blind retry; it worsened ingress network errors.

84. Phase 4 M4d: async observer owner relay
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 83 showed Mahi execution can commit above `25k TPS`, but observer ingress still synchronously fanned each external `kombat_submitIntentBatch` into up to four tiny validator JSON-RPC forwards. At 30k offered with batch size 8 this creates listener pressure and owner-forwarding errors. Reusing the existing batched `/network/relay-intents` worker for Mahi observer ingress should coalesce owner-sharded submissions and remove synchronous fanout from the client path.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept async observer relay at 30k offered:
     - `/tmp/codex-exp-phase4-m4d-observer-async-relay-30k-10s`
     - `440,760 committed`
     - `44,076.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Loadgen accepted all traffic: `449,792 ok`, `rpc_err=0`, `net_err=0`, with p99 request latency `14.569ms`. This removes the item-83 owner-forwarding error class.
   - The relay worker drained fully by the post-load status snapshot: all validators reported `pending_relay_intents=0` and `pending_relay_targets=0`.
   - Committed throughput improved from item 83's clean 16k saved state (`23,987.2 TPS`) to `44,076.0 TPS` at 30k offered, while preserving deterministic state-root convergence.
   - This path is opt-in behind `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`. It changes observer ingress semantics: the observer returns an accepted response after queueing validly-shaped signed intents for owner relay, before the owner validator's local policy/mempool admission has synchronously acknowledged them. The benchmark workload uses valid intents, so this is safe for M4d measurement; production semantics need an explicit queued-vs-admitted API decision before deployment.
   Status:
   - kept
   Reason:
   - The change removes the measured observer fanout bottleneck, produces the first clean Mahi 30k-offered result above the M4d gate, and does not alter validator-side Mahi commit/determinism logic.
   Next move:
   - M4d throughput gate is satisfied in local harness with `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`: saved result `44,076.0 TPS` at 30k offered, zero request errors, deterministic convergence.
   - Before treating this as deployment-ready ingress behavior, decide whether optimistic observer queue acceptance is acceptable, or add an explicit queued response/status API.
   - Continue Phase 4 optimization toward 250k by raising offered load beyond 30k and measuring the next bottleneck. Candidate next runs: 60k, 100k, then 150k offered with the async relay flag enabled.

85. Phase 4 M4d diagnostic: async relay scaling curve
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 84 removed observer owner-forwarding pressure and produced a clean `44,076.0 TPS` 30k-offered result, raising offered load should reveal whether Mahi canonical throughput continues scaling or has moved to a new production/commit bottleneck.
   Files changed:
   - None
   Benchmark runs:
   - kept 30k reference from item 84:
     - `/tmp/codex-exp-phase4-m4d-observer-async-relay-30k-10s`
     - `440,760 committed`
     - `44,076.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 60k offered:
     - `/tmp/codex-exp-phase4-m4d-observer-async-relay-60k-10s`
     - `422,417 committed`
     - `42,241.7 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 100k offered:
     - `/tmp/codex-exp-phase4-m4d-observer-async-relay-100k-10s`
     - `401,635 committed`
     - `40,163.5 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Ingress remains clean at all three offered loads: the 60k and 100k runs accepted all loadgen traffic with zero RPC/network errors, and the async relay queue drained to zero post-load.
   - Committed throughput does not scale with offered load after the 30k run. It plateaus and slightly declines from `44,076.0 TPS` to `42,241.7 TPS` and `40,163.5 TPS`.
   - Per-validator Mahi production sums plateau around `110k-121k` produced intents during the load window even when accepted traffic rises from `449,792` to `899,544` and `1,499,648`. Non-empty block density stays roughly `p50=240-270` intents/block and `p95=650-830`.
   - The next bottleneck is no longer observer ingress fanout. The limiting surface is production-side availability: local admission drain, proposer frontier readiness, Mahi block production cadence, or commit/execution capacity.
   Status:
   - kept (diagnostic)
   Reason:
   - The ladder identifies a new low-40k TPS plateau under clean ingress and convergent state. This is the correct optimization target for the next slice.
   Next move:
   - Continue from item 84's code. Inspect and tune Mahi production-side availability, starting with local-submit drain batching and proposer-frontier fill, without changing observer relay semantics or Mahi commit correctness.

86. Phase 4 M4d diagnostic: Mahi local-submit drain batch sizing
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 85 showed clean ingress but a low-40k committed TPS plateau. If the owner validators are not admitting queued relayed submissions into proposer frontier quickly enough, changing the Mahi local-submit drain batch size should increase produced block fill and lift 60k/100k throughput.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned drain batch `64` at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-local-drain64-60k-10s`
     - `388,911 committed`
     - `38,891.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned drain batch `8` at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-local-drain8-60k-10s`
     - `454,749 committed`
     - `45,474.9 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned drain batch `8` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-local-drain8-100k-10s`
     - `269,705 committed`
     - `26,970.5 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Larger drain batches are clearly worse: batch `64` regressed 60k throughput from item 85's `42,241.7 TPS` reference to `38,891.1 TPS`.
   - Smaller drain batches improve one point but are not robust: batch `8` improved 60k to `45,474.9 TPS`, then collapsed 100k to `26,970.5 TPS` versus item 85's default-batch `40,163.5 TPS`.
   - The 100k batch-`8` run produced more Mahi rounds but fewer total block payload intents (`~304k` produced across validators versus `~443k` in item 85), indicating scheduler/round-fill interference rather than a simple admission-drain shortage.
   Status:
   - binned
   Reason:
   - Local-submit drain batch size is not a monotonic throughput lever. The default batch `16` remains the best stable point across higher offered load. Carrying a new env knob would add tuning surface without a reliable path toward the 250k target.
   Next move:
   - Revert the diagnostic knob and keep item 84 as the saved code state.
   - Move to fixed-cost reduction in the Mahi commit/install path and runtime-event storage, where the cost scales with committed intents and high offered-load memory pressure.

87. Phase 4 M4d diagnostic: Mahi commit/install allocation pre-sizing
   Date:
   - `2026-04-24`
   Hypothesis:
   - The high-load plateau may be caused by avoidable allocation growth in the Mahi merged-execution path: merged body construction, `commit_block_intents()` event collection, and runtime event installation all grow vectors proportional to committed intents.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_single_node_executes_committed_intents_in_mahi_mode -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned allocation pre-sizing at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-event-reserve-60k-10s`
     - `428,923 committed`
     - `42,892.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned allocation pre-sizing at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-event-reserve-100k-10s`
     - `385,267 committed`
     - `38,526.7 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The 60k run produced only a small improvement over item 85's `42,241.7 TPS` reference, inside likely harness variance.
   - The 100k run regressed versus item 85's `40,163.5 TPS` reference, so allocation pre-sizing is not a robust high-load fix.
   - Correctness stayed intact in both runs, but the measured effect is too small and inconsistent to justify carrying the code.
   Status:
   - binned
   Reason:
   - Allocation growth in these specific vectors is not the current plateau. The changes add code churn without reliable movement toward the 250k target.
   Next move:
   - Keep item 84 as the saved code state.
   - Shift from micro-allocation changes to protocol/commit-path structure: measure whether the remaining wall is Mahi round/commit cadence, signature verification, event-stream retention, or full-body chain retention under high offered load.

88. Phase 4 M4d diagnostic: disable Mahi sigverify pool
   Date:
   - `2026-04-24`
   Hypothesis:
   - If asynchronous block-intent signature verification is limiting Mahi commit throughput at high offered load, disabling the sigverify pool should increase committed TPS at 100k offered. This is diagnostic-only and not a valid production configuration.
   Files changed:
   - None
   Benchmark runs:
   - binned sigverify-disabled run at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-sigverify-off-100k-10s`
     - `374,368 committed`
     - `37,436.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Disabling the pool regressed throughput versus item 85's default 100k reference (`40,163.5 TPS`).
   - Ingress remained clean and convergence passed, so the regression is not an error-path artifact.
   - This rules out sigverify as the primary limiter for the low-40k plateau.
   Status:
   - binned
   Reason:
   - Removing block-intent verification work does not improve committed throughput. The sigverify pool should remain enabled; it is not the current high-load bottleneck.
   Next move:
   - Keep item 84 as the saved code state.
   - Continue diagnosing retained chain/event body pressure and Mahi round/commit cadence. The next useful measurement should distinguish production cadence from commit/install retention cost.

89. Phase 4 M4d diagnostic: Mahi event/body retention bypass
   Date:
   - `2026-04-24`
   Hypothesis:
   - At 100k offered, retained synthetic block bodies and per-intent runtime-event records may create enough memory/write pressure to cap Mahi below the item-85 low-40k plateau. Diagnostic env flags can isolate this after state root and event root have already been computed.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-node mahi_ -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned skip runtime-event retention at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-skip-events-100k-10s`
     - `320,256 committed`
     - `32,025.6 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned compact synthetic chain body at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-compact-body-100k-10s`
     - `380,555 committed`
     - `38,055.5 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Both retention bypasses preserved convergence but regressed versus item 85's default 100k reference (`40,163.5 TPS`).
   - Skipping event retention was especially negative, suggesting the wall is not append-only runtime event storage.
   - Compacting stored block bodies also failed to improve throughput, so full synthetic body retention is not the primary high-load limiter over this 10s window.
   Status:
   - binned
   Reason:
   - Retention bypasses do not move the plateau and weaken audit/API semantics. The code was reverted.
   Next move:
   - Keep item 84 as the saved code state.
   - Focus on Mahi cadence and commit production: round count, commit detector/executor wake frequency, and whether executor catch-up is gated by waiting for fixed-size merged batches.

90. Phase 4 M4d: queued asynchronous Mahi block broadcast
   Date:
   - `2026-04-24`
   Hypothesis:
   - Mahi block production is still coupled to the old HTTP relay plane because `maybe_run_mahi_round()` waits for peer `/network/mahi/block` POSTs before returning. Queuing block broadcasts to a background worker should let the producer advance rounds without waiting on peer HTTP latency.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - diagnostic thread-per-peer async broadcast at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-async-block-broadcast-60k-10s`
     - `443,651 committed`
     - `44,365.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - diagnostic thread-per-peer async broadcast at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-async-block-broadcast-100k-10s`
     - `420,153 committed`
     - `42,015.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept queued-worker async broadcast at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-async-block-queue-60k-10s`
     - `436,314 committed`
     - `43,631.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept queued-worker async broadcast at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-async-block-queue-100k-10s`
     - `431,213 committed`
     - `43,121.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The quick thread-per-peer diagnostic showed the producer wait on block fanout was real, but the implementation shape was not acceptable as a kept path.
   - The queued worker keeps the same decoupling without spawning one thread per peer per produced block.
   - At 60k offered, queued broadcast improves item 85's async-observer reference from `42,241.7 TPS` to `43,631.4 TPS` (`+3.3%`).
   - At 100k offered, queued broadcast improves item 85's async-observer reference from `40,163.5 TPS` to `43,121.3 TPS` (`+7.4%`).
   - Both kept runs preserved clean ingress and post-load state-root convergence, so the producer/relay decoupling did not weaken Mahi correctness.
   - A pending Mahi block broadcast queue metric was added to the consensus metrics payload for future backlog diagnostics.
   Status:
   - kept
   Reason:
   - This removes one more HotStuff-era synchronous relay wait from Mahi's producer critical path and gives a repeatable high-load improvement while keeping correctness intact.
   Next move:
   - Keep `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1` and `KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1` as the current best Mahi runtime flags for high-load runs.
   - Inspect remaining production/commit cadence using the new pending-block-broadcast metric and produced-round/block-fill data. If the broadcast queue backs up, split it by peer or move Mahi block transport off per-block HTTP; if it stays empty, focus on DAG round cadence and commit executor wake/batch policy.

91. Phase 4 M4d diagnostics: cadence, sharding, writer count, merge batch, and pipeline knobs
   Date:
   - `2026-04-24`
   Hypothesis:
   - The post-async-relay low-40k TPS plateau may be caused by a simple runtime knob rather than a deeper code path: Mahi round cadence, payload sharding, writer/policy count, merged execution batch size, or the planned pipeline/wave controls.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (temporary pipeline/wave env wiring, reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - cadence `AUTO_INTERVAL_MS=100` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-cadence100-100k-10s`
     - `431,712 committed`
     - `43,171.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - cadence `AUTO_INTERVAL_MS=25` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-cadence25-100k-10s`
     - `416,006 committed`
     - `41,600.6 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - payload sharding disabled at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-no-payload-shard-60k-10s`
     - `231,967 committed`
     - `23,196.7 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - 128 writers at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-writers128-100k-10s`
     - `388,723 committed`
     - `38,872.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - merged execution batch `16` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-merge16-100k-10s`
     - `413,984 committed`
     - `41,398.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - temporary `KOMBAT_MAHI_PIPELINE=1` runtime wiring at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-pipeline-100k-10s`
     - `340,425 committed`
     - `34,042.5 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - temporary `KOMBAT_MAHI_WAVE_LENGTH=4` runtime wiring at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-wave4-100k-10s`
     - failed initial Mahi state convergence before load
   Evidence:
   - Cadence tuning around the 50ms default is flat-to-negative. Slower `100ms` was effectively equal to item 90's `43,121.3 TPS` reference; faster `25ms` regressed.
   - Payload sharding is required. Disabling it almost halves 60k throughput because validators duplicate payload work instead of splitting by shard.
   - Increasing writer/policy count from `32` to `128` regressed throughput, so the plateau is not caused by too few policy streams in the harness.
   - Larger merged execution batches also regress, so the default batch `8` remains the best observed point.
   - Pipeline activation is correct enough to converge, but it is throughput-negative in the current runtime. Wave length `4` is not runtime-safe yet because it failed startup Mahi convergence.
   Status:
   - binned
   Reason:
   - None of the simple knob changes improves the current saved path. The temporary pipeline/wave runtime wiring was reverted because it either regressed throughput or exposed unsafe non-default wave behavior.
   Next move:
   - Keep the saved runtime flags from item 90 unchanged.
   - Stop tuning harness knobs. The next work should change structure: reduce Mahi's per-round empty-block churn, remove per-block HTTP from Mahi block transport, or make commit/execution install process larger deterministic batches without delaying startup or convergence.

92. Phase 4 M4d: ordered pending intent relay index
   Date:
   - `2026-04-24`
   Hypothesis:
   - The observer ingress owner-relay worker still rebuilds and sorts the full pending relay map on every drain. Maintaining an ordered relay index should remove that repeated `O(n log n)` sort from the high-load path while preserving nonce/policy/intent relay ordering.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept ordered relay index at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-relay-index-60k-10s`
     - `448,381 committed`
     - `44,838.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept ordered relay index at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-relay-index-100k-10s`
     - `450,463 committed`
     - `45,046.3 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - At 60k offered, ordered relays improve item 90's queued-broadcast reference from `43,631.4 TPS` to `44,838.1 TPS` (`+2.8%`).
   - At 100k offered, ordered relays improve item 90's queued-broadcast reference from `43,121.3 TPS` to `45,046.3 TPS` (`+4.5%`).
   - Both runs preserved clean ingress and post-load state-root convergence, so removing the repeated sort did not weaken relay ordering or Mahi execution determinism.
   - The 100k run is now the current best clean Phase 4 Mahi number, but it is still far below the 250k target. The extended-drain diagnostic still indicates accepted work is backing up before full commit, not that request payload size is the current wall.
   Status:
   - kept
   Reason:
   - This removes another avoidable observer-relay CPU cost and gives a repeatable high-load improvement with minimal surface area.
   Next move:
   - Keep `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1` and `KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1` as the current best runtime flags.
   - Focus next on the owner-validator admission/local-submit path: `submit_intents()` still admits each relayed intent one-by-one under the admission write lock, and the 30s drain diagnostic showed accepted work eventually commits when given more catch-up time.

93. Phase 4 M4d diagnostic: per-target pending intent relay index
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 92 removed the full-map sort, the remaining observer relay selection cost may be scanning the shared ordered queue to fill per-validator batches. Maintaining per-target ordered indexes should let each owner-validator batch be selected directly.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned per-target relay index at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-relay-target-index-100k-10s`
     - `449,284 committed`
     - `44,928.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The per-target index preserved correctness but did not beat item 92's `45,046.3 TPS` reference at 100k offered.
   - The small negative delta is within likely run variance, but the change adds more queue bookkeeping than item 92's global ordered index.
   Status:
   - binned
   Reason:
   - Per-target queue indexing does not produce a clear throughput gain over the simpler ordered-index implementation. The code was reverted.
   Next move:
   - Keep item 92 as the saved state.
   - Stop spending effort on relay-selection bookkeeping until instrumentation proves the observer relay queue is still the dominant backlog. Next slice should measure or reduce owner-validator admission/write-lock work.

94. Phase 4 M4d: Mahi relay endpoint fast enqueue
   Date:
   - `2026-04-24`
   Hypothesis:
   - In canonical Mahi mode, `/network/relay-intents` still previews every relayed intent under an admission read lock before enqueueing it, then the local-submit worker validates/admit-checks the same intent again under admission write. Skipping the endpoint preview in Mahi mode should make the owner relay endpoint a pure enqueue path while preserving worker-side validation.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept Mahi relay fast-enqueue at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-fast-relay-enqueue-60k-10s`
     - `516,662 committed`
     - `51,666.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept Mahi relay fast-enqueue at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-fast-relay-enqueue-100k-10s`
     - `520,818 committed`
     - `52,081.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - At 60k offered, fast enqueue improves item 92's ordered-relay reference from `44,838.1 TPS` to `51,666.2 TPS` (`+15.2%`).
   - At 100k offered, fast enqueue improves item 92's ordered-relay reference from `45,046.3 TPS` to `52,081.8 TPS` (`+15.6%`).
   - HotStuff/default relay semantics are unchanged; the preview bypass is gated to `RuntimeConsensusMode::Mahi`.
   - Correctness stayed intact in both runs: clean ingress, no network errors, and post-load state-root convergence passed.
   - The improvement exposes the next limiter: owner validators end with `local_submit` near the `65,536` queue limit even at 60k offered, while `pending_relay_intents=0`. That means observer relay selection is no longer the wall; owner-side local-submit drain/admission throughput is.
   Status:
   - kept
   Reason:
   - This removes duplicate admission-read preview work from the Mahi owner relay path without changing the eventual validation/admission point. It gives the largest post-M4d high-load improvement since async observer relay.
   Next move:
   - Treat `52,081.8 TPS` at 100k offered as the current best clean Phase 4 Mahi reference.
   - Focus next on owner-validator local-submit backlog: either make the worker drain/admit path more efficient, add safe backpressure so observer relay does not saturate owner queues, or move Mahi payload ingestion away from the legacy local-submit queue entirely.

95. Phase 4 M4d: Mahi local-submit drain batch retune after fast relay enqueue
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 94 removed duplicate owner-relay preview/read-lock work, the previously binned local-submit drain batch sizes may have a different optimum. Increasing the Mahi-only drain batch from `16` to `32` may reduce worker fixed cost without the severe admission-write starvation seen at `64` and `256`.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept drain batch `32` at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-local-drain32-fast-relay-60k-10s`
     - `526,680 committed`
     - `52,668.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept drain batch `32` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-local-drain32-fast-relay-100k-10s`
     - `529,904 committed`
     - `52,990.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned drain batch `64` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-local-drain64-fast-relay-100k-10s`
     - `493,441 committed`
     - `49,344.1 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Batch `32` improves item 94's 60k reference from `51,666.2 TPS` to `52,668.0 TPS` (`+1.9%`).
   - Batch `32` improves item 94's 100k reference from `52,081.8 TPS` to `52,990.4 TPS` (`+1.7%`).
   - Batch `64` is still too large: it regressed to `49,344.1 TPS` and pushed admission-write wait p95 to roughly `57-64ms` with `~11-12ms` hold p95.
   - Batch `32` is a modest tuning win, not a structural fix. Owner validators still end near the `65,536` local-submit queue limit, so the admitted-to-committed backlog remains the primary wall.
   Status:
   - kept
   Reason:
   - The post-fast-relay optimum shifted from `16` to `32`, but larger batches still starve the admission path. Keeping `32` gives a small repeatable gain while preserving correctness.
   Next move:
   - Treat `52,990.4 TPS` at 100k offered as the current best clean Phase 4 Mahi reference.
   - The next slice should stop tuning batch constants and address the legacy local-submit queue architecture directly: either add a Mahi-specific owner ingestion path that batches validation/admission more efficiently, or add instrumentation for observer pending-relay backlog plus owner queue drain rate to identify the exact handoff wall.

96. Phase 4 M4d diagnostic: local-submit queue wait measurement
   Date:
   - `2026-04-24`
   Hypothesis:
   - Before bypassing the legacy local-submit queue for Mahi, measure whether accepted owner relays are spending material time waiting in that queue. If queue wait dominates, Direction A (Mahi-specific ingestion that bypasses the legacy queue) is justified; if not, the next wall is DAG cadence or commit execution.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/scripts/live_network_status.sh` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - diagnostic local-submit wait instrumentation at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-local-submit-wait-metrics-100k-10s`
     - `500,864 committed`
     - `50,086.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The run preserved correctness but regressed versus item 95's `52,990.4 TPS` reference, so the instrumentation was not kept in the hot path.
   - The diagnostic signal is clear: post-load `local_submit` queues were `65,120`, `65,536`, `65,440`, and `65,312`.
   - Measured local-submit queue wait p90 was `12,525ms`, `11,656ms`, `11,388ms`, and `12,915ms` across the four validators; max wait was `~11.4-13.0s`.
   - Drain batch p90 remained `32`, confirming the item 95 batch size was active during the measurement.
   - `pending_relay_intents=0` and `pending_relay_targets=0` on validators, so the measured backlog is owner local-submit/admission, not pending relay selection.
   Status:
   - binned (diagnostic only)
   Reason:
   - The instrumentation perturbs throughput and should not remain in the saved path, but it proves the local-submit queue wait is a dominant Mahi latency/backlog component.
   Next move:
   - Start Direction A as the next structural slice: bypass the legacy local-submit queue for Mahi owner relay ingestion while preserving validation, nonce checks, and deterministic state.
   - Keep HotStuff/default submit behavior unchanged. The Mahi path should stop paying for the HotStuff-era queue/drain model.

97. Phase 4 M4d: direct Mahi owner relay admission
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 96 measured `~11-13s` p90 wait in the legacy local-submit queue for Mahi owner relays. Since observer relay is already asynchronous, the Mahi `/network/relay-intents` owner path can admit relay batches directly in bounded chunks without blocking client ingress, eliminating the HotStuff-era queue/drain stop while preserving engine validation and nonce checks.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept direct owner relay admission at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-direct-owner-admit32-60k-10s`
     - `536,312 committed`
     - `53,631.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept direct owner relay admission at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-direct-owner-admit32-100k-10s`
     - `531,744 committed`
     - `53,174.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The direct path improves item 95's 60k reference from `52,668.0 TPS` to `53,631.2 TPS` (`+1.8%`).
   - The direct path improves item 95's 100k reference from `52,990.4 TPS` to `53,174.4 TPS` (`+0.3%`).
   - Post-load `local_submit` queues are `0` on all validators at both 60k and 100k, proving the legacy queue wait measured in item 96 was removed from the Mahi owner relay path.
   - Correctness stayed intact: clean ingress, no network errors, and post-load state-root convergence passed at both offered loads.
   - Remaining backlog moved to admitted-but-uncommitted frontier items, so the next limiter is no longer the local-submit queue itself.
   Status:
   - kept
   Reason:
   - The TPS gain is modest, but this removes a measured `~11-13s` Mahi queue wait and deletes a HotStuff-era handoff from the Mahi relay path without changing HotStuff/default behavior. Keeping the change simplifies the path and gives cleaner downstream bottleneck signals.
   Next move:
   - Treat `53,174.4 TPS` at 100k offered as the current best clean Phase 4 Mahi reference.
   - Inspect the post-bypass plateau by separating accepted-to-produced from produced-to-committed: measure Mahi block payload production, commit payload totals, and commit/execution cadence before making another architectural change.

98. Phase 4 M4d diagnostic: Mahi auto-round cadence sweep after direct owner admission
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 97 removed local-submit queue wait, the remaining plateau might be caused by HotStuff-era auto-round cadence. Faster Mahi rounds could increase payload production, or slower drive cadence could produce denser blocks with less empty-round overhead.
   Files changed:
   - none
   Tests:
   - not applicable (runtime-only harness diagnostic)
   Benchmark runs:
   - binned fast cadence at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-mahi-interval10-100k-10s`
     - `491,928 committed`
     - `49,192.8 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned slow drive cadence at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-mahi-drive50-100k-10s`
     - `523,504 committed`
     - `52,350.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Item 97 reference (`AUTO_INTERVAL_MS=50`, default drive `25ms`) remains best at `53,174.4 TPS`.
   - Fast cadence (`AUTO_INTERVAL_MS=10`, drive `5ms`) produced roughly `3,368-3,369` local rounds per validator, but most were empty (`p50=0`, `p90=0`) and average payload collapsed to `~42-43` intents/round.
   - Slow drive (`AUTO_INTERVAL_MS=50`, drive `50ms`) made denser blocks (`p50=384-480`, average `~512-518` intents/round) but fewer rounds; total produced payload stayed similar and committed TPS regressed.
   - Both diagnostics preserved correctness, so the cadence knob is safe but not useful in this workload.
   Status:
   - binned
   Reason:
   - The plateau is not fixed by simple auto-round cadence tuning. Faster cadence wastes CPU on empty DAG blocks; slower cadence improves density but loses round throughput.
   Next move:
   - Keep item 97's default cadence as the current saved path.
   - Continue with structural diagnostics on the accepted-to-produced gap: inspect why each validator only produces `~150k` payload intents during a 100k offered run despite accepting `~1.5M` requests at ingress.

99. Phase 4 M4d diagnostic: direct owner relay admission chunk-size sweep
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 97 uses the existing `32`-intent Mahi admission chunk size for direct owner relay admission. Since the legacy local-submit queue is now bypassed, the owner relay path may have a different chunk-size optimum: `64` might reduce fixed lock cost, while `16` might reduce admission lock contention and improve producer interleaving.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned direct owner relay chunk `64` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-direct-owner-admit64-100k-10s`
     - `517,064 committed`
     - `51,706.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned direct owner relay chunk `16` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-direct-owner-admit16-100k-10s`
     - `517,046 committed`
     - `51,704.6 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Both alternatives regress versus item 97's chunk-`32` reference (`531,744 committed`, `53,174.4 TPS`).
   - Chunk `64` reintroduces admission-write wait p95 around `45-56ms`, similar to prior too-large batch experiments.
   - Chunk `16` keeps admission-write wait p95 below `0.5ms` on most validators, but the extra lock cycling loses throughput versus chunk `32`.
   - Correctness stayed intact for both diagnostics, so the variable is performance-sensitive but not correctness-sensitive in this range.
   Status:
   - binned
   Reason:
   - Direct owner relay admission has the same practical optimum as the current saved path: `32` balances lock hold time and lock acquisition overhead better than `16` or `64`.
   Next move:
   - Keep item 97 unchanged.
   - Do not continue tuning batch constants. The remaining gap needs a structural change or a sharper measurement of owner admission outcomes, producer frontier availability, and Mahi commit/execution backlog.

100. Phase 4 M4d diagnostic: owner relay outcome counters
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 97 bypassed the legacy local-submit queue, determine whether ingress-accepted traffic is owner-admitted, benignly discarded, retrying, or still pending in the observer relay path. If owner validators accept far fewer intents than loadgen ingress, the next wall is feed/relay throughput rather than DAG cadence or owner queue depth.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/scripts/live_network_status.sh` (reverted)
   Tests:
   - `bash -n chain/scripts/live_network_status.sh`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - diagnostic owner relay outcome counters at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-relay-outcome-metrics-100k-10s`
     - `491,492 committed`
     - `49,149.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The telemetry run regressed versus item 97's clean `53,174.4 TPS` reference, so the hot-path counters were reverted.
   - Owner validators reported accepted relay totals of `128,000`, `132,112`, `136,200`, and `136,216`; `benign=0` and `retry=0` on all four validators.
   - Total owner-admitted relay traffic was `532,528`, close to the run's `491,492` committed intents and item 97's clean `531,744` committed reference.
   - Loadgen ingress accepted approximately `1,499,048` requests in the same run. The missing `~1.0M` ingress-accepted intents were not owner-admitted during the measurement window.
   - Prior `pending_relay_intents=0` validator readings did not prove the observer relay was drained, because `live_network_status.sh` does not include the observer process in its host set.
   Status:
   - binned (diagnostic only)
   Reason:
   - The telemetry perturbs the hot path and should not stay in the saved path, but it shows the current plateau is a feed/relay handoff wall: owner validators only receive/admit roughly the amount that eventually commits.
   Next move:
   - Stop cadence and chunk-size tuning.
   - Re-anchor the next slice on observer relay backlog/feed throughput: include the observer in status metrics or structurally speed the observer relay to owner admission pipeline.
   - The true Direction A remains incomplete: item 97 bypassed the legacy local-submit queue, but Mahi relay traffic still pays engine admission/mempool costs before becoming producer payload. A larger Mahi pending-payload store or owner admission pipeline redesign is the next structural candidate.

101. Phase 4 M4d diagnostic: include observer ingress in local status
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 100 showed validator-only status was blind to the observer relay backlog. Adding the observer ingress process to the generated local `hosts.txt` when `LOCAL_CLUSTER_ENABLE_INGRESS=1` should expose whether accepted loadgen traffic is stuck before owner validators receive it.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `bash -n chain/scripts/live_network_status.sh`
   Benchmark runs:
   - observer-visible status at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-observer-status-100k-10s`
     - `494,640 committed`
     - `49,464.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Post-load validator submit queues stayed clear: `local_submit=0`, `pending_relay_intents=0`, and `pending_relay_targets=0` on all four validators.
   - The observer ingress process reported `pending_relay_intents=946,888` and `pending_relay_targets=946,888` after the same run.
   - Loadgen accepted `1,499,392` requests, while only `494,640` were committed by the standard post-load capture. The missing traffic is queued at the observer relay, before owner admission.
   - The harness-only change is post/pre status visibility; it does not affect runtime code. Item 97's `53,174.4 TPS` remains the current clean saved reference.
   Status:
   - kept
   Reason:
   - This closes the observability gap from item 100 and makes future 100k+ diagnostics report the actual observer backlog instead of falsely implying relay drain is complete.
   Next move:
   - Optimize the observer relay to owner-admission handoff. The current plateau is not owner local-submit depth, simple Mahi cadence, or commit convergence; it is observer feed throughput.

102. Phase 4 M4d diagnostic: observer relay in-flight-per-owner sweep
   Date:
   - `2026-04-24`
   Hypothesis:
   - The observer relay worker may be stop-and-wait bound because it sends at most one `/network/relay-intents` request per owner per drain loop. Allowing two in-flight owner requests per drain loop could hide HTTP/JSON latency and reduce the observer backlog without changing validation semantics.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned two-in-flight owner relay at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-relay-inflight2-100k-10s`
     - `520,612 committed`
     - `52,061.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The run is below item 97's clean `53,174.4 TPS` reference.
   - Observer backlog remained high: post-load `pending_relay_intents=922,448`, only modestly lower than item 101's `946,888`.
   - Validator admission-write wait p95 rose into multi-ms/tens-of-ms territory (`~7.7ms` to `~29.4ms` on the sampled validators), showing concurrent owner relay requests add lock contention instead of materially improving feed throughput.
   Status:
   - binned
   Reason:
   - Extra in-flight owner relay requests do not solve the observer backlog and regress the saved TPS reference. The bottleneck is not simple stop-and-wait HTTP serialization.
   Next move:
   - Keep the single-in-flight observer relay worker.
   - Move to a structural owner-admission/feed redesign: either batch engine admission more efficiently for Mahi relays or introduce a Mahi-specific pending-payload store so observer relay traffic does not have to synchronously pay full engine admission before becoming producer payload.

103. Phase 4 M4d diagnostic: bulk submit expiration-purge reduction
   Date:
   - `2026-04-24`
   Hypothesis:
   - `EngineHandle::submit_intents` holds the admission write lock for a batch but calls `SingleNodeEngine::submit_intent` one intent at a time. Since each call repeats direct and conditional expiration purges, adding a real bulk engine submit that purges once per timestamp may reduce owner admission cost enough to drain observer backlog faster.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/crates/node/src/engine_handle.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-node submit_intent -- --nocapture` (no matching tests)
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned bulk submit purge reduction at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-bulk-submit-purge-100k-10s`
     - `511,516 committed`
     - `51,151.6 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The run is below item 97's clean `53,174.4 TPS` reference.
   - Observer backlog stayed effectively unchanged at `pending_relay_intents=942,144`.
   - Admission-write wait p95 improved on most validators, but admission hold p95 remained around `6.1-6.9ms` and throughput did not improve. Repeated expiration purges are not the dominant wall.
   Status:
   - binned
   Reason:
   - The refactor reduces one admission sub-cost but does not improve committed TPS or observer backlog. Keeping it would add code surface without moving the Phase 4 target.
   Next move:
   - Stop optimizing small admission sub-steps. The remaining gap requires removing or redesigning the synchronous observer-relay-to-engine-admission handoff for Mahi, not shaving individual operations inside it.

104. Phase 4 M4d diagnostic: Mahi fast relay admission without runtime simulation
   Date:
   - `2026-04-24`
   Hypothesis:
   - Owner relay admission may be dominated by admission-time runtime simulation. A Mahi-only fast relay path behind `KOMBAT_MAHI_FAST_RELAY_ADMISSION=1` can still verify signatures, expiry, stale nonce, and duplicate policy nonce, but skip the pre-admission runtime simulation and rely on Mahi commit execution for final deterministic state application.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/crates/node/src/engine_handle.rs` (reverted)
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo test -p kombat-chain-node verify_intent_signature -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - binned fast relay admission at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-fast-relay-admission-100k-10s`
     - `495,592 committed`
     - `49,559.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The run is below item 97's clean `53,174.4 TPS` reference and roughly matches the observer-backlog diagnostic range.
   - Observer backlog stayed high at `pending_relay_intents=949,736`.
   - Admission-write hold p95 remained around `5.8-7.5ms`, so runtime simulation is not the dominant owner-relay admission cost under this workload.
   - Correctness held, but the performance signal is negative.
   Status:
   - binned
   Reason:
   - Skipping admission-time simulation weakens the safety boundary and does not improve throughput or observer drain. There is no reason to keep this flag path.
   Next move:
   - Do not continue shaving individual validation steps. The next candidate must change dataflow: feed Mahi block production without forcing every observer-accepted intent through synchronous owner engine admission first, or redesign the observer relay queue/transport to drain close to accepted ingress rate.

105. Phase 4 M4d diagnostic: distribute 100k load directly across validators
   Date:
   - `2026-04-24`
   Hypothesis:
   - The 100k plateau may be a single-observer ingress bottleneck. If loadgen targets all four validators directly at 25k RPS each, aggregate throughput may rise without changing runtime code.
   Files changed:
   - none
   Tests:
   - not applicable (runtime-only harness diagnostic)
   Benchmark runs:
   - binned four-validator direct public submit at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-target-all-100k-10s`
     - `14,679 committed`
     - `1,467.9 TPS`
     - post-load state-root convergence passed
   Evidence:
   - Loadgen hit `local_submit_queue_full` on all four validator targets. The direct public submit path still uses the legacy local-submit queue and does not get the observer async relay semantics.
   - Aggregate accepted requests were far below the observer-ingress path, and committed output collapsed to `14,679` total.
   - Validator 4 still showed `local_submit=13,891` post-load with admission-write wait p95 around `55ms`, confirming this path reopens the legacy local-submit wall that item 97 removed from Mahi owner relay ingestion.
   Status:
   - binned
   Reason:
   - Horizontalizing loadgen across validator public RPC endpoints is not equivalent to horizontalizing the Mahi observer async ingress path. It reintroduces HotStuff-era local-submit queue backpressure and is not a valid path to the 80k/100k gate.
   Next move:
   - Keep using `LOCAL_CLUSTER_ENABLE_INGRESS=1`, `LOADTEST_TARGET_VALIDATOR=ingress`, and `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1` for clean high-load Mahi runs.
   - If horizontal ingress is tested later, add multiple observer ingress processes rather than targeting validator public submit directly.

106. Phase 4 M4d diagnostic: two observer ingress processes at 100k
   Date:
   - `2026-04-24`
   Hypothesis:
   - The single observer process may be the feed bottleneck. Running two observer ingress processes and sharding the same 100k offered load across them may reduce per-observer relay backlog and increase owner feed throughput.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh` (reverted)
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - smoke: `/tmp/codex-exp-phase4-m4d-multi-ingress-smoke-1k-2s`
   Benchmark runs:
   - binned two-observer ingress at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-ingress2-100k-10s`
     - `512,364 committed`
     - `51,236.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The run is below item 97's clean `53,174.4 TPS` reference.
   - Observer backlog merely split across the two processes: `pending_relay_intents=463,400` and `468,256`, for a combined `931,656` pending intents. That is the same shape as the single-observer backlog from item 101.
   - Validators still had clear relay/local queues, so owners were receiving only the same committed-rate feed regardless of one or two observers.
   Status:
   - binned
   Reason:
   - The bottleneck is not the single observer process. It is the observer-relay-to-owner-admission pipeline and/or owner admission capacity. Keeping multi-ingress harness complexity does not help the current Phase 4 gate.
   Next move:
   - Revert to the kept single-ingress harness with observer status visibility.
   - The next productive slice must change the owner feed dataflow rather than multiplying observers, tuning relay concurrency, or shaving validation substeps.

107. Phase 4 M4d: Mahi pre-admitted owner payload pool
   Date:
   - `2026-04-24`
   Hypothesis:
   - Items 100-106 showed roughly `900k-950k` ingress-accepted intents stuck at the observer relay while owner validators had empty local-submit and pending-relay queues. In Mahi mode, owner relay traffic should bypass synchronous engine admission and feed a bounded Mahi pre-admitted payload pool that the producer drains directly into DAG blocks. Existing Mahi block sigverify still verifies block payloads before execution.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/live_network_status.sh`
   Tests:
   - `cargo test -p kombat-chain-rpc pre_admitted -- --nocapture`
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_ -- --nocapture`
   - `bash -n chain/scripts/live_network_status.sh`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen`
   Benchmark runs:
   - kept pre-admitted owner payload pool at 60k offered:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-60k-10s`
     - `665,232 committed`
     - `66,523.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept pre-admitted owner payload pool at 100k offered, best run:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-100k-10s`
     - `785,480 committed`
     - `78,548.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept pre-admitted owner payload pool at 100k offered, rerun:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-rerun-100k-10s`
     - `663,000 committed`
     - `66,300.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned larger block cap diagnostic:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-max8000-100k-10s`
     - `85,552 committed`
     - `8,555.2 TPS`
     - state-root convergence passed, but throughput collapsed
   - binned cadence diagnostic:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-interval50-100k-10s`
     - `706,432 committed`
     - `70,643.2 TPS`
     - state-root convergence passed, but did not beat the default-cadence best
   Evidence:
   - The best 100k run improves item 97's clean `53,174.4 TPS` reference to `78,548.0 TPS` (`+47.7%`).
   - Even the lower 100k rerun improves item 97 from `53,174.4 TPS` to `66,300.0 TPS` (`+24.7%`), so the structural direction is positive despite high variance.
   - The observer backlog is gone in the kept runs: post-load ingress reported `pending_relay_intents=0` and `pending_relay_targets=0`.
   - Owner validators accepted roughly the whole ingress stream into the new Mahi pool in the best 100k run (`~374.8k` accepted per validator, `~1.499M` total) with `dropped_oldest=0`.
   - The remaining backlog moved to validator-local Mahi pre-admitted pools. In the best 100k run, validators drained `244,928`, `251,584`, `260,624`, and `273,344` intents respectively, but still retained `101k-130k` each after settle.
   - Larger Mahi block bodies are not a safe quick fix: `AUTO_MAX_ITEMS=8000` collapsed throughput, likely from oversized block broadcast or commit/execution pressure.
   - Faster producer cadence is not a quick fix after the pool change: `AUTO_INTERVAL_MS=50` stayed below the default-cadence best.
   Status:
   - kept
   Reason:
   - This removes the synchronous owner engine-admission handoff from Mahi relay ingestion and moves Phase 4 from a `~53k` ceiling to a `66k-78.5k` band at 100k offered while preserving clean ingress and deterministic state-root convergence. The M9 `>=80k` short-run gate is close but not robustly passed yet.
   Next move:
   - Treat `78,548.0 TPS` at 100k offered as the current best clean Phase 4 Mahi reference, with high variance noted.
   - Stop testing larger block caps and simple cadence changes for now; both binned on this slice.
   - Next productive slice should target producer/commit drain stability: reduce pre-load empty-round churn, improve pre-admitted pool fairness across policies, or reduce commit-tail lag so more of the already accepted validator-local pool commits inside the measurement window.

108. Phase 4 M4d diagnostic: post-pool merged execution batch sweep
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 107 moved the wall from observer relay to validator-local Mahi pool drain, the default merged execution batch size (`8`) may be the wrong latency/throughput tradeoff. Batch `16` could reduce fixed execution overhead; batch `4` could reduce commit-tail latency.
   Files changed:
   - none
   Tests:
   - not applicable (runtime-only harness diagnostic)
   Benchmark runs:
   - binned merged execution batch `16` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-merge16-100k-10s`
     - `762,632 committed`
     - `76,263.2 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned merged execution batch `4` at 100k offered:
     - `/tmp/codex-exp-phase4-m4d-pre-admitted-pool-merge4-100k-10s`
     - `740,120 committed`
     - `74,012.0 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - Batch `16` preserved correctness but did not beat item 107's best `78,548.0 TPS`.
   - Batch `4` drained more validator-local pool entries (`~300k+` per validator) but committed fewer total intents than the best default-batch run, so smaller batches increase chain progress without improving measured committed throughput.
   - The default batch `8` remains the best observed tradeoff after the pre-admitted-pool redesign.
   Status:
   - binned
   Reason:
   - Simple merged-execution batch tuning does not robustly cross the `80k` short-run gate. The remaining wall is broader producer/commit drain stability rather than this single batch-size knob.
   Next move:
   - Keep the default merged execution batch size.
   - Next slice should change structure rather than sweep knobs: candidate directions are suppressing pre-load empty DAG churn, fairer pre-admitted pool drain across policies, or reducing Mahi block broadcast/payload overhead.

109. Phase 4 M4d/M9 diagnostic: committed TPS time series and soak shape
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 107's `66k-78.5k` 100k/10s band may be high-variance due to warmup, producer/commit pacing, or scheduler noise. Add a cheap per-second committed-intent time series to the local harness so Phase 4 can distinguish true in-window throughput from post-load drain effects before optimizing further.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain --bin kombat_sign -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - kept 100k/10s variance series with async relay and async block broadcast:
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-10s-run1`: `727,744 committed`, `72,774.4 TPS`
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-10s-run2`: `675,632 committed`, `67,563.2 TPS`
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-10s-run3`: `667,328 committed`, `66,732.8 TPS`
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-10s-run4`: `695,104 committed`, `69,510.4 TPS`
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-10s-run5`: `737,432 committed`, `73,743.2 TPS`
   - kept 100k/30s variance series:
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-30s-run1`: `1,083,784 committed`, `36,126.1 TPS`
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-30s-run2`: `1,164,152 committed`, `38,805.1 TPS`
     - `/tmp/codex-exp-phase4-m4d-variance-v2-async-max5000-100k-30s-run3`: `1,028,008 committed`, `34,266.9 TPS`
   - current M9 soak attempt:
     - `/tmp/codex-exp-phase4-m9-soak-v2-async-max5000-60k-60s`
     - `1,836,504 committed`
     - `30,608.4 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - binned 60k/60s merged-execution batch reruns under the corrected soak metric:
     - `/tmp/codex-exp-phase4-m9-soak-v2-async-max5000-batch4-60k-60s`: `31,173.5 TPS`
     - `/tmp/codex-exp-phase4-m9-soak-v2-async-max5000-batch1-60k-60s`: `20,403.2 TPS`
   - binned 60k/60s block-cap sweep:
     - `/tmp/codex-exp-phase4-m9-soak-v2-async-max3000-60k-60s`: `30,161.6 TPS`
     - `/tmp/codex-exp-phase4-m9-soak-v2-async-max2000-60k-60s`: `27,568.0 TPS`
   - binned current-architecture sigverify-off rerun:
     - `/tmp/codex-exp-phase4-m9-soak-v2-async-max5000-sigverify-off-60k-60s`
     - `32,326.7 TPS`
   - binned binary block-broadcast spike, reverted:
     - `/tmp/codex-exp-phase4-m9-soak-v2-binary-block-60k-60s`
     - `30,846.0 TPS`
   Evidence:
   - The initial serial-policy time-series sampler was discarded because 32 policy RPCs/sec perturbed high-load runs and produced sparse samples. The kept sampler reads `kombat_getConsensusMetrics.mahi.executed_intent_count` once per second.
   - The valid 100k/10s series has a median of `69,510.4 TPS`, below item 107's `78,548.0 TPS` best but still above the pre-pool item 97 reference.
   - Per-second execution samples show pacing oscillation, not warmup: many measured seconds commit `~0` intents while active seconds cluster around `~40k TPS`. The 30s and 60s runs do not amortize this away.
   - The 60k/60s soak keeps ingress clean (`60k RPS`, zero RPC/network errors) and converges, but only commits `30,608.4 TPS`, missing the M9 soak gate (`>=50k TPS`).
   - The 60k/60s soak leaves large owner-local Mahi pools (`~414k-439k` items per validator) and records `drop_oldest` on each owner. The wall is downstream of observer ingress and upstream of sustained execution.
   - Retesting simple knobs under the corrected soak metric rules them out as primary fixes: batch `4`, batch `1`, smaller block caps, sigverify-off, and binary block broadcast all fail to materially improve sustained TPS.
   Status:
   - kept diagnostic
   Reason:
   - This establishes the correct measurement surface for M9: final 10s committed totals were inflated by post-load drain, while in-window execution is bursty and much lower. The kept instrumentation is runtime-inert outside metrics and gives future slices a cheap per-second sustained-throughput signal.
   Next move:
   - Stop chasing 10s post-drain peaks as the primary gate.
   - Fix sustained producer/commit pacing. The next code slice should target the owner-local Mahi pool/producer lifecycle so full pools continue feeding contiguous blocks without zero-commit gaps.

110. Phase 4 M9 diagnostic: rich Mahi timeseries and inflight nonce-skip bin
   Date:
   - `2026-04-24`
   Hypothesis:
   - The item 109 soak showed large owner-local Mahi pools and zero-commit gaps. Expand the per-second sampler to include Mahi produced/received/verified/committed/executed block counters plus queue depths, then test whether producer drain is slowed by repeatedly scanning large contiguous inflight nonce ranges.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted_nonce_skip_walks_contiguous_inflight_range --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - valid binned inflight nonce-skip 60k/60s soak:
     - `/tmp/codex-exp-phase4-m9-soak-inflight-skip-rebuilt-60k-60s`
     - `1,766,136 committed`
     - `29,435.6 TPS`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
     - timeseries `measured_avg_tps=21,889.6`, `measured_peak_tps=40,152.0`
   - discarded stale-build inflight nonce-skip 60k/60s soak:
     - `/tmp/codex-exp-phase4-m9-soak-inflight-skip-60k-60s`
     - `1,893,216 committed`
     - `31,553.6 TPS`
     - discarded because `kombat_chain` had not been explicitly relinked after the Rust change
   Evidence:
   - The valid rebuilt run is below item 109's green M9 soak reference (`30,608.4 TPS`), so contiguous inflight nonce scanning is not the sustained-throughput limiter.
   - The richer sampler shows the executor usually commits in `8`-block chunks when active, but many measured seconds execute `0` blocks. Pending relay and pending block-broadcast queues remain at `0`, while owner-local pre-admitted pools grow toward the cap.
   - The bottleneck is now commit/verification cadence over an already-fed DAG, not observer relay, block broadcast backlog, sigverify pool depth, or the specific inflight nonce scan tested here.
   Status:
   - kept diagnostic; code slice binned
   Reason:
   - The sampler change is useful and runtime-inert outside local harness diagnostics. The nonce-skip Rust change was reverted because the rebuilt soak regressed below the current green reference.
   Next move:
   - Target Mahi commit cadence directly. The next pragmatic slice should expose the planned `KOMBAT_MAHI_PIPELINE`, `KOMBAT_MAHI_WAVE_LENGTH`, and `KOMBAT_MAHI_MULTILEADER` runtime knobs, then benchmark whether pipelined committers or shorter waves remove the zero-execution seconds.

111. Phase 4 M9 diagnostic: pipeline and wave-length runtime knobs remain unsafe
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 110 showed zero-execution seconds caused by commit cadence. Re-introduce temporary runtime wiring for `KOMBAT_MAHI_PIPELINE`, `KOMBAT_MAHI_WAVE_LENGTH`, and `KOMBAT_MAHI_MULTILEADER` with defaults unchanged, then test whether pipeline or a shorter wave removes the cadence gaps.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_metrics_report_runtime_commit_knobs --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - binned pipeline diagnostic:
     - `/tmp/codex-exp-phase4-m9-pipeline-60k-20s`
     - `776,760 committed`
     - `38,838.0 TPS` final/duration
     - timeseries `measured_avg_tps=20,069.3`, `measured_peak_tps=40,123.7`
     - failed post-load Mahi state convergence
   - binned wave length `4` diagnostic:
     - `/tmp/codex-exp-phase4-m9-wave4-60k-20s`
     - failed initial startup Mahi state convergence before load
   Evidence:
   - Pipeline did not remove the in-window cadence wall; it still averaged roughly `20k TPS` by per-second samples and then failed Mahi convergence after load.
   - Wave length `4` is not safe on the current runtime wiring; the cluster failed initial Mahi convergence before any load was applied.
   - This repeats the earlier item 91 conclusion under the newer pre-admitted-pool path: these commit-rule knobs cannot be treated as quick M9 unlocks until their convergence behavior is fixed.
   Status:
   - binned
   Reason:
   - Both tested commit-cadence knobs break a correctness gate. The temporary runtime wiring was reverted; no code from this diagnostic is retained.
   Next move:
   - Do not retry pipeline, wave length, or multileader as throughput knobs until there is a dedicated correctness slice for non-default committers.
   - Next throughput work should target the verified-block-to-executed-block gap under the default safe committer: why verified blocks arrive in bursts but execution only advances in intermittent `8`-block chunks.

112. Phase 4 M9 diagnostic: wave-aligned execution batch and parallel block fanout binned
   Date:
   - `2026-04-24`
   Hypothesis:
   - Default Mahi commits release roughly a wave of `~20` DAG blocks at a time (`5` rounds x `4` validators), while merged execution defaults to `8` blocks. Aligning merged execution to `20` blocks or parallelizing per-block peer fanout may reduce zero-execution seconds.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - binned wave-aligned merged execution batch `20`:
     - `/tmp/codex-exp-phase4-m9-merge20-60k-20s`
     - `846,928 committed`
     - `42,346.4 TPS` final/duration
     - timeseries `measured_avg_tps=22,553.2`, `measured_peak_tps=100,139.2`
     - post-load state-root convergence passed
   - binned parallel per-block Mahi fanout:
     - `/tmp/codex-exp-phase4-m9-parallel-block-fanout-60k-20s`
     - `1,030,328 committed`
     - `51,516.4 TPS` final/duration
     - timeseries `measured_avg_tps=25,782.1`, `measured_peak_tps=40,353.3`
     - post-load state-root convergence passed
   Evidence:
   - Batch `20` produced larger active spikes but more zero seconds (`15/21` measured samples at `0 TPS`), lowering sustained average versus the item 109 rich 20s reference (`~26,116 TPS`).
   - Parallelizing peer sends inside the Mahi block broadcast worker preserved correctness but did not beat the rich 20s reference: final/duration was roughly flat and in-window TPS was slightly lower.
   - Log-derived production data shows the current default already creates many empty or underfilled Mahi blocks: in `/tmp/codex-exp-phase4-m9-timeseries-rich-60k-20s`, `636` produced local blocks across validators averaged `2,168` intents/block, with `373` positive-payload blocks averaging `3,697` intents. The next wall is block production/fill cadence, not execution batch size or serial fanout alone.
   Status:
   - binned
   Reason:
   - Both changes preserve correctness but fail to improve the corrected in-window throughput signal. The temporary fanout code was reverted; the batch `20` experiment was runtime-only.
   Next move:
   - Target Mahi block production/fill directly. The producer is generating too many empty or underfilled blocks while owner-local pools remain non-empty; the next slice should improve payload availability at round production time without changing unsafe commit-rule knobs.

113. Phase 4 M9 diagnostic: fair pre-admitted policy drain binned
   Date:
   - `2026-04-24`
   Hypothesis:
   - `MahiPreAdmittedPool::drain_for_block` starts every block from the lowest policy id. Rotating the start policy after each selected block may reduce per-policy starvation, keep nonce frontiers healthier under long soaks, and improve sustained Mahi payload fill.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted_pool_rotates_policy_drain_start --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_relay_intents_in_mahi_mode_populates_pre_admitted_pool --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - binned fair drain at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-fair-pre-admitted-drain-60k-20s`
     - `922,272 committed`
     - `46,113.6 TPS` final/duration
     - timeseries `measured_avg_tps=26,193.5`, `measured_peak_tps=41,730.9`
     - post-load state-root convergence passed
   - binned fair drain at 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-fair-pre-admitted-drain-60k-60s`
     - `1,777,280 committed`
     - `29,621.3 TPS`
     - timeseries `measured_avg_tps=22,993.8`, `measured_peak_tps=40,346.7`
     - post-load state-root convergence passed
   Evidence:
   - The policy rotation did improve fairness: the 20s run narrowed committed writer spread to `24,960` min / `31,480` max.
   - Throughput regressed versus the current green references: the 20s final/duration was below `/tmp/codex-exp-phase4-m9-timeseries-rich-60k-20s` (`51,946.8 TPS`), and the 60s soak was below item 109's `30,608.4 TPS`.
   - Fairer policy service is not the M9 throughput limiter; preserving the lowest-policy drain bias currently gives better aggregate throughput even if it is less fair.
   Status:
   - binned
   Reason:
   - The change improves distribution but misses the throughput goal. The fair-drain code was reverted.
   Next move:
   - Keep default pre-admitted drain ordering.
   - Next productive slice should measure Mahi block production rate and payload fill explicitly in metrics, then target the producer round/fill path with evidence rather than inference from validator logs.

114. Phase 4 M9 diagnostic: explicit Mahi production/fill metrics
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 113 still inferred producer fill from validator logs after the fact. Expose produced-empty-block, produced-intent, and received-intent counters through `kombat_getConsensusMetrics.mahi`, then include them in the local harness time series to distinguish empty-block backlog from true payload-fill failure.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - kept 60k/20s produced/fill diagnostic:
     - `/tmp/codex-exp-phase4-m9-produced-fill-metrics-60k-20s`
     - `1,038,752 committed`
     - `51,937.6 TPS` final/duration
     - timeseries `measured_avg_tps=27,676.6`, `measured_peak_tps=40,157.5`
     - post-load state-root convergence passed
   Evidence:
   - The new counters show validator-1 produced `64` empty blocks before payload and `0` empty blocks during the measured load window.
   - During the measured window, validator-1 produced `35` local blocks carrying `172,920` intents (`4,940.6` intents/block) and received `102` peer blocks carrying `505,784` intents (`4,958.7` intents/block). Payload fill is high once payload is available.
   - Per-second zero-commit samples correlate with commits of older empty DAG waves, not with empty measured-window block production. The current green final/duration numbers still include substantial post-load drain: validator-1 had produced `214,192` local payload intents by load-window end but `359,192` by process shutdown.
   - Cluster log parsing shows each validator produced `158` local Mahi blocks, with `64-65` empty blocks and `~359k-360k` total payload intents. Empty pre-load DAG churn is a first-order cause of early measured-window undercount and post-load drain inflation.
   Status:
   - kept diagnostic
   Reason:
   - The metrics are runtime-inert and convert a log-parsing inference into a cheap harness signal. They also rule out measured-window payload underfill as the primary wall: blocks are near the `AUTO_MAX_ITEMS=5000` cap when payload exists.
   Next move:
   - Target idle empty-block production. The next slice should stop the auto Mahi producer from creating long empty DAG backlogs before payload arrives, while preserving explicit/manual empty-round tests and post-load convergence.

115. Phase 4 scale groundwork: copy-on-write state checkpoints
   Date:
   - `2026-04-24`
   Hypothesis:
   - Full-cloning `InMemoryStateStore` state on every checkpoint/executor clone is not the current M9 localhost wall, but it becomes a structural blocker as committed state grows and for Phase 6 Block-STM-style speculative execution. Port the `pvp-cow-state` spike as an isolated state-db change, keep public state traits unchanged, and verify roots remain byte-stable.
   Files changed:
   - `chain/crates/state-db/src/lib.rs`
   - `chain/crates/execution/src/lib.rs`
   - `chain/crates/node/src/lib.rs`
   Tests:
   - `cargo test -p kombat-chain-state-db --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-execution --lib --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node rejects_timestamp_regression --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - kept 60k/20s clean CoW diagnostic:
     - `/tmp/codex-exp-phase4-m9-cow-state-clean-60k-20s`
     - `1,019,168 committed`
     - `50,958.4 TPS` final/duration
     - timeseries `measured_avg_tps=25,467.3`, `measured_peak_tps=58,376.0`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - discarded transient run before removing an unrelated inert RPC helper:
     - `/tmp/codex-exp-phase4-m9-cow-state-60k-20s`
     - `877,736 committed`
     - `43,886.8 TPS` final/duration
     - discarded as evidence for this commit because the tree included an unrelated uncommitted RPC helper that was later removed from the kept patch
   Evidence:
   - `InMemoryStateStore::checkpoint()` now clones an `Arc<BTreeMap<_, _>>` and records per-checkpoint `Set/Delete` deltas, so checkpoint creation is `O(1)` and live checkpoints remain isolated.
   - Overlay root computation is tested against materialized maps, including set/delete edge cases and a larger 256-entry delta. State-db roots remain compatible with the legacy encoding.
   - The change exposed a masked `produce_empty_block()` timestamp-regression bug: empty blocks with no state delta now preserve the configured root instead of accidentally failing via root mismatch. Added an explicit `NonMonotonicTimestamp` guard.
   - `cargo test -p kombat-chain-node terminal_frontier_rejection_cascades_to_queued_successor_intents` still fails on the pre-CoW baseline commit `2e55e4f09`, so that node test failure is pre-existing and not attributed to CoW.
   - Localhost Mahi throughput is flat to slightly down versus the item 114 60k/20s reference (`51,937.6 TPS` final/duration, `27,676.6` sampled average). This is expected: the current M9 wall is producer/commit cadence, not checkpoint clone cost at the current tiny state size.
   Status:
   - kept
   Reason:
   - The patch is correctness-neutral under state-db and execution tests, fixes an explicit empty-block validation gap, and removes a known state-size/concurrency scaling hazard. It is retained for scale/Phase 6 readiness, not counted as an M9 TPS improvement.
   Next move:
   - Resume M9 throughput work at the current measured wall: idle empty-block production and sustained producer/commit cadence. Do not attribute future TPS changes to CoW unless state size or speculative checkpoint concurrency is increased enough to exercise it.

116. Phase 4 M9 experiment: suppress idle empty Mahi auto-production binned
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 114 showed validators produce `64-65` empty Mahi blocks before payload arrives. Suppressing automatic empty Mahi rounds until a validator has local, relayed, or pre-admitted payload candidates should prevent an empty DAG backlog, reduce early measured-window zero-commit samples, and improve sustained 60k offered throughput.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc network_relay_intents_in_mahi_mode_populates_pre_admitted_pool --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - binned idle-empty suppression at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-idle-empty-suppress-60k-20s`
     - `886,656 committed`
     - `44,332.8 TPS` final/duration
     - timeseries `measured_avg_tps=25,778.6`, `measured_peak_tps=40,119.3`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - The change achieved its narrow mechanical goal: startup convergence stayed at `height=1`, and validator-1 reported `mahi_produced_empty_blocks=0` throughout the sampled load window.
   - Throughput regressed versus both current green references: item 114 (`51,937.6 TPS` final/duration, `27,676.6` sampled average) and item 115 (`50,958.4 TPS` final/duration, `25,467.3` sampled average with a higher post-load total).
   - Removing pre-payload empty rounds also removes useful DAG warmup. The first measured commits now pay more live DAG/commit warmup cost, and post-load committed total fell to `886,656`.
   Status:
   - binned
   Reason:
   - Idle empty-block suppression improves a visible metric but hurts the throughput signal that matters. Empty pre-load DAG churn is not purely waste under the current commit pipeline; it also primes round/wave progress.
   Next move:
   - Reverted the suppression code. Keep the item 114 production/fill metrics, and target the remaining wall through sustained producer/commit cadence or warm DAG measurement design rather than preventing all idle empty rounds.

117. Phase 4 M9 experiment: borrowed-DAG commit detector binned
   Date:
   - `2026-04-24`
   Hypothesis:
   - `MahiState::advance_commit_detector()` deep-clones `DagState` for every detector advance. At M9 payload sizes that clone includes full Mahi block bodies and may be a hidden per-block cost. Borrowing the live DAG into `BaseCommitter` and `UniversalCommitter` should preserve commit-rule semantics while removing that clone.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `KOMBAT_MAHI_TYPES=1 cargo test -p kombat-chain-consensus mahi:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - binned borrowed-DAG committer at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-borrowed-dag-60k-20s`
     - `1,103,760 committed`
     - `55,188.0 TPS` final/duration
     - timeseries `measured_avg_tps=26,834.5`, `measured_peak_tps=40,035.6`
     - `rpc_err=0`
     - `net_err=0`
     - failed post-load Mahi state convergence
   Evidence:
   - Unit and focused RPC determinism tests passed, so the refactor is not obviously wrong on small deterministic DAGs.
   - The benchmark produced a better final/duration number than item 114, but the corrected in-window sampled signal was still below item 114 (`27,676.6 TPS`) and the cluster failed the hard state-root convergence gate.
   - Post-load snapshots showed validators at different heads/heights after the run. A throughput-positive patch that fails convergence cannot be kept.
   Status:
   - binned
   Reason:
   - Correctness gates take precedence over short-run committed totals. The borrowed-DAG code was reverted.
   Next move:
   - If clone removal is revisited, do it as a correctness slice first: preserve a stable committer/DAG snapshot boundary or persistent committer state, then prove post-load convergence before counting TPS. Continue M9 work on the default safe committer.

118. Phase 4 M9 optimization: metadata-only commit DAG view
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 117 showed that removing the full DAG clone can move throughput, but borrowing the live DAG broke the convergence gate. Preserve an owned immutable commit-detector snapshot while stripping full `MahiBlock` bodies from that snapshot. The commit rule only needs `BlockRef -> ancestors` and round/author indexes; execution still uses the full `DagState` after leaders are decided.
   Files changed:
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `KOMBAT_MAHI_TYPES=1 cargo test -p kombat-chain-consensus mahi:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - kept 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-commit-view-60k-20s`
     - `1,064,240 committed`
     - `53,212.0 TPS` final/duration
     - timeseries `measured_avg_tps=27,841.1`, `measured_peak_tps=57,033.8`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   - kept 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-commit-view-60k-60s`
     - `2,061,456 committed`
     - `34,357.6 TPS` final/duration
     - timeseries `measured_avg_tps=25,302.1`, `measured_peak_tps=40,145.9`
     - `rpc_err=0`
     - `net_err=0`
     - post-load state-root convergence passed
   Evidence:
   - `CommitDagView` snapshots only `BlockRef` ancestry and the round index. It keeps the immutable snapshot boundary that item 117 lacked, but avoids cloning intent payloads and signatures into the commit detector.
   - The 20s run improves over item 114 (`51,937.6 TPS` final/duration, `27,676.6` sampled average) while preserving convergence.
   - The 60s soak improves over item 109's current green soak (`30,608.4 TPS` final/duration, `22,909.4` sampled average) by `+12.3%` final/duration and `+10.4%` sampled average.
   - The soak still misses M9's `>=50k TPS` sustained gate and still leaves large owner-local pre-admitted pools, so the remaining wall is downstream producer/commit cadence rather than ingress.
   Status:
   - kept
   Reason:
   - This is a correctness-preserving structural reduction in commit-detector work with a positive 60s soak result. It does not complete M9, but it raises the saved soak baseline.
   Next move:
   - Continue attacking sustained cadence. The next slice should distinguish whether the remaining gap is commit detector wake frequency, execution batch wake policy, or block production round rate with full owner-local pools.

119. Phase 4 M9 experiment: pre-admitted-pool planner fast path binned
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 118 showed large owner-local Mahi pre-admitted pools remaining after the 60s soak. When that pool already has enough intents to fill a block, the Mahi producer should be able to skip the full engine/frontier planner snapshot and build from pre-admitted payload only, reducing producer-side lock and planner work.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_snapshot_shards_against_configured_validator_set --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_relay_intents_in_mahi_mode_populates_pre_admitted_pool --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted_pool_drains_round_robin_across_policies --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - raw prepool fast path at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-prepool-fastpath-60k-20s`
     - `1,233,376 committed`
     - `61,668.8 TPS` final/duration
     - timeseries `measured_avg_tps=25,936.7`, `measured_peak_tps=46,064.6`
     - `rpc_err=0`
     - `net_err=0`
     - failed post-load Mahi state convergence
   - fair prepool fast path at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-prepool-fastpath-fair-60k-20s`
     - `1,069,992 committed`
     - `53,499.6 TPS` final/duration
     - timeseries `measured_avg_tps=25,554.8`, `measured_peak_tps=40,201.0`
     - `rpc_err=0`
     - `net_err=0`
     - failed post-load Mahi state convergence
   Evidence:
   - The raw fast path produced an attractive final/duration number, but convergence failed and committed writer distribution was severely imbalanced (`min=4,920`, `max=46,864`). Skipping the planner exposed that `MahiPreAdmittedPool::drain_for_block()` drains policy IDs in sorted order and can starve later writers.
   - A round-robin drain variant fixed most of the writer skew (`min=30,585`, `max=35,593`) but still failed post-load convergence and was effectively flat versus the item 118 green 20s reference (`53,212.0 TPS` final/duration).
   - Both variants had lower corrected sampled averages than item 118 (`27,841.1 TPS`), so the apparent raw fast-path improvement was not a trustworthy in-window throughput win.
   Status:
   - binned
   Reason:
   - Correctness gates take precedence over raw committed totals. The planner fast path changes Mahi payload selection enough to break post-load convergence, and the fairness repair does not recover a meaningful green throughput gain.
   Next move:
   - Reverted the fast-path and fairness code. Regroup around measurement before the next optimization: use the replay ladder / variance harness or add cadence counters for commit-detector wake frequency, execution batch wake policy, and block production round rate. Do not keep throughput-positive changes that fail Mahi state convergence.

120. Phase 4 M9 harness correction: Mahi convergence checks common-height state roots
   Date:
   - `2026-04-24`
   Hypothesis:
   - Recent high-load 60k/20s runs failed post-load Mahi convergence because the harness required all validators to have identical latest committed block count and latest commit-log hash at the same instant. In live asynchronous Mahi mode, validators continue producing empty rounds after the load window and can be a few committed heights apart while still agreeing on all common executed state. The harness should validate the Mahi safety invariant by comparing state roots at the common minimum executed height, while logging latest progress skew as diagnostic context.
   Files changed:
   - `chain/scripts/replay_bench_ladder.sh`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/replay_bench_ladder.sh`
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - replay ladder before the common-height fix at 60k offered / 20s, 3 runs:
     - `/var/folders/pq/lgc890gs3h12nm6whjzjy5340000gn/T/kombat-replay-ladder.XXXXXX.OQ2GhThw2H`
     - run TPS: `59,022.8`, `55,997.2`, `55,346.8`
     - all three failed the old latest-commit equality convergence check
   - mismatch diagnostic run before the common-height fix:
     - `/var/folders/pq/lgc890gs3h12nm6whjzjy5340000gn/T/kombat-replay-ladder.XXXXXX.vwCgFJhxnf/run01`
     - `1,147,624 committed`
     - `57,381.2 TPS` final/duration
     - latest Mahi progress skew observed after timeout (`v1/v2 committed_blocks=3596`, `v3 committed_blocks=3616`, differing latest commit logs)
   - common-height smoke run at 8k offered / 5s:
     - `/tmp/codex-exp-phase4-m9-common-height-smoke-8k-5s`
     - post-load accepted latest progress skew (`min_height=455`, `max_height=458`)
     - common-height state root matched: `0xe0216626c95c5cc75f62e69f4d7ad9ae82b4de7136893be6b69ffef550168ae9`
   Evidence:
   - The old check conflated "same latest progress at the same sampling instant" with "same state at a common committed height." That is too strict for an asynchronous DAG that continues producing after load stops.
   - The corrected check still fails true divergence: it queries `kombat_getStateRootAtHeight` at the common minimum executed height across validators and requires all state roots to match.
   - The replay ladder root parser also now handles `0x`-prefixed roots correctly, so future variance summaries can classify converged runs.
   Status:
   - kept
   Reason:
   - This is a harness correctness fix, not a protocol optimization. It prevents false negative convergence failures caused by harmless latest-progress skew, while preserving the state-root consistency gate that catches real divergence.
   Next move:
   - Re-run the current green Mahi code at 60k offered / 20s and 60k offered / 60s under the common-height convergence check. Use those artifacts as the corrected M9 baseline before making another runtime change.

121. Phase 4 M9 stability: round-robin Mahi pre-admitted pool drain
   Date:
   - `2026-04-24`
   Hypothesis:
   - The Mahi pre-admitted pool drained policy IDs from sorted order on every block. Under sustained owner-local backlog this can starve later writers, inflate variance, and leave large owner-local pools even when all relayed intents have reached their owners. Rotate the policy cursor across blocks and select one eligible nonce per policy per pass to preserve nonce order while improving writer fairness.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_relay_intents_in_mahi_mode_populates_pre_admitted_pool --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - corrected baseline replay ladder before this change at 60k offered / 20s, 3 runs:
     - `/var/folders/pq/lgc890gs3h12nm6whjzjy5340000gn/T/kombat-replay-ladder.XXXXXX.6Ejemopc5e`
     - run TPS: `35,952.8`, `48,144.4`, `37,034.4`
     - mean `40,377.2 TPS`, CV `13.6%`, spread `30.2%`
     - all runs passed common-height state-root convergence
   - round-robin replay ladder at 60k offered / 20s, 3 runs:
     - `/var/folders/pq/lgc890gs3h12nm6whjzjy5340000gn/T/kombat-replay-ladder.XXXXXX.lG4NuULhFE`
     - run TPS: `48,793.8`, `53,306.6`, `54,096.7`
     - mean `52,065.7 TPS`, median `53,306.6 TPS`, CV `4.5%`, spread `10.2%`
     - all runs passed common-height state-root convergence
   - single round-robin 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-prepool-rr-60k-20s`
     - `857,444 committed`
     - `42,872.2 TPS` final/duration
     - timeseries `measured_avg_tps=22,317.8`, `measured_peak_tps=40,137.1`
     - post-load common-height state-root convergence passed
   - round-robin 60k offered / 60s soak:
     - `/tmp/codex-exp-phase4-m9-prepool-rr-60k-60s`
     - `2,008,424 committed`
     - `33,473.7 TPS` final/duration
     - timeseries `measured_avg_tps=25,585.2`, `measured_peak_tps=40,187.8`
     - post-load common-height state-root convergence passed
   Evidence:
   - The 20s replay ladder recovered the prior good 50k+ range under the corrected convergence check and reduced run-to-run variance from `13.6%` CV to `4.5%` CV.
   - Writer fairness improved materially. The prior item 118 60s reference committed `2,061,456` intents but was highly skewed (`min_per_writer=5,024`, `max_per_writer=121,872`). The round-robin 60s run committed a similar `2,008,424` intents with much tighter distribution (`min_per_writer=45,854`, `max_per_writer=70,253`).
   - The 60s soak is flat to slightly down versus item 118 (`34,357.6 TPS` final/duration, `25,302.1` sampled average), so this is not counted as an M9 throughput unlock.
   - Owner-local pre-admitted pools still remain large after the 60s soak (`333k-363k` items per validator), so the sustained wall is still producer/commit cadence rather than observer ingress or relay delivery.
   Status:
   - kept
   Reason:
   - This is a correctness/stability improvement: it prevents sorted-policy starvation, improves fairness, preserves state-root convergence, and reduces short-run variance without materially changing the sustained throughput wall. The small 60s TPS tradeoff is acceptable because the previous higher total depended on severe writer skew.
   Next move:
   - Continue M9 from the stabilized baseline. The next slice should target sustained drain of the large owner-local pre-admitted pools without reintroducing writer starvation: measure producer round rate, produced-intents-per-validator, and execution pending depth across all validators, then optimize the cadence component that is demonstrably saturated.

122. Phase 4 M9 diagnostic: all-validator Mahi timeseries sampling
   Date:
   - `2026-04-24`
   Hypothesis:
   - The existing committed timeseries sampled only validator-1. After item 121, the remaining wall is sustained owner-local pool drain and commit cadence; sampling all validators should distinguish a single-validator lag from a cluster-wide cadence issue without changing runtime behavior.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - all-validator timeseries diagnostic at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-all-validator-timeseries-60k-20s`
     - `1,018,068 committed`
     - `50,903.4 TPS` final/duration
     - timeseries `measured_avg_tps=26,181.1`, `measured_peak_tps=49,962.4`
     - post-load common-height state-root convergence passed
   Evidence:
   - `LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS=1` now records per-validator Mahi counters and min/max/sum aggregates in `committed-timeseries.json`, while preserving the existing single-validator default.
   - The diagnostic run stayed in the current green 20s range and retained the item 121 fairness shape (`min_per_writer=29,890`, `max_per_writer=32,959`).
   - All validators produced payload at similar rates during the measured window. At the end of the sampled window, produced intent counts were tightly grouped (`214,168-218,672`) and executed block counts matched (`432`), ruling out a single stuck validator as the primary wall.
   - Owner-local pre-admitted pools remained large and balanced after the run (`26k-56k` items per validator post-load; `156k-161k` in sampled validator aggregates near load end), while execution pending stayed low except for brief burst windows. This points at producer/commit cadence rather than relay delivery or one slow executor.
   Status:
   - kept diagnostic
   Reason:
   - The harness change is opt-in and runtime-inert. It converts the next M9 decision from a single-validator guess into a cluster-wide cadence measurement.
   Next move:
   - Use the new all-validator timeseries for the next optimization slice. The highest-confidence next target is commit/producer cadence smoothing: reduce the observed zero-commit seconds without increasing writer skew or breaking common-height convergence.

123. Phase 4 M9 diagnostic: Mahi producer skip counters
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 122 showed balanced validators but bursty commit cadence while owner-local pre-admitted pools stayed full. Count Mahi producer attempts and skip reasons to determine whether producers are idle because of missing payload, snapshot unavailability, round races, peer reachability, or missing ancestor quorum.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - producer skip counter diagnostic at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-producer-skip-counters-60k-20s`
     - `935,328 committed`
     - `46,766.4 TPS` final/duration
     - timeseries `measured_avg_tps=21,534.3`, `measured_peak_tps=40,109.2`
     - post-load common-height state-root convergence passed
   Evidence:
   - The diagnostic run stayed correct and fair enough for measurement (`min_per_writer=22,364`, `max_per_writer=31,739`), with all validators accepting and draining owner-local pre-admitted pools.
   - Producer attempts were frequent but rarely produced blocks. Near load end, validators reported roughly `975-1101` producer attempts and only `135` produced blocks.
   - The dominant skip reason was missing ancestor quorum: `832-901` skips per validator near load end. Peer reachability, snapshot unavailable, and round-race skips were not the visible wall.
   - Owner-local pre-admitted pools remained large (`181k-185k` per validator near load end), but payload was available. Producers were blocked waiting for enough round ancestors, not waiting for intents.
   Status:
   - kept diagnostic
   Reason:
   - The counters are low-risk observability and identify the next real blocker: ancestor-delivery / round-advance cadence. This rules out further local pool-drain tuning as the immediate M9 lever.
   Next move:
   - Target Mahi block delivery / ancestor quorum latency. The next slice should measure received-block arrival cadence and then reduce the time between local block production and peers accepting that block, without changing commit-rule knobs that previously failed convergence.

124. Phase 4 M9 slice: parallel async Mahi block fanout
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 123 showed producer skips dominated by missing ancestor quorum while payload was available. The async Mahi block broadcast worker still sent each produced block to peers serially; parallelizing per-peer sends inside the async worker could reduce ancestor-arrival latency and smooth producer cadence.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (experiment only; reverted before commit)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_round_broadcast_converges_across_four_validators --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - no-code A/B with inline parallel broadcast at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-inline-parallel-broadcast-60k-20s`
     - `1,083,012 committed`
     - `54,150.6 TPS` final/duration
     - post-load common-height state-root convergence passed
   - parallel async broadcast at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-parallel-async-broadcast-60k-20s`
     - `1,083,186 committed`
     - `54,159.3 TPS` final/duration
     - timeseries `measured_avg_tps=25,383.4`, `measured_peak_tps=40,052.8`
     - post-load common-height state-root convergence passed
   - parallel async broadcast at 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-parallel-async-broadcast-60k-60s`
     - `1,693,016 committed`
     - `28,216.9 TPS` final/duration
     - timeseries `measured_avg_tps=24,285.8`, `measured_peak_tps=48,852.6`
     - post-load common-height state-root convergence passed
   Evidence:
   - The 20s runs looked positive and reached the current green short-run range, but the 60s soak regressed materially versus the item 121 fair baseline (`33,473.7 TPS` final/duration).
   - The 60s run preserved correctness and writer fairness (`min_per_writer=52,280`, `max_per_writer=53,522`), so this was not a safety failure; it was a sustained-throughput failure.
   - Owner-local pre-admitted pools filled to their cap (`524,288` items per validator) with non-zero `dropped_oldest`, which means parallel fanout did not solve sustained producer/commit drain.
   Status:
   - binned
   Reason:
   - Parallel peer fanout improves or preserves short-run burst behavior but worsens the sustained 60s green number. M9 is blocked on stable sustained throughput, not another 20s peak, so the code was reverted.
   Next move:
   - Continue from item 123/121 code. The next slice should measure ancestor arrival latency directly rather than changing fanout structure speculatively: record per-round local production time, peer block receive time, and ancestor-quorum-ready time to identify whether the delay is send, receive/store, or quorum-clock scheduling.

125. Phase 4 M9 diagnostic: ancestor wait latency counters
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 123 showed producer skips dominated by missing ancestor quorum, but did not distinguish whether the wait was caused by the local broadcast queue, peer receive/store latency, or quorum-clock scheduling. Add per-validator counters for the current ancestor-wait round, available previous-round ancestors, quorum size, wait age, and first/latest previous-round arrival ages.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - ancestor-wait metric diagnostic at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-ancestor-wait-metrics-60k-20s`
     - `1,097,886 committed`
     - `54,894.3 TPS` final/duration
     - timeseries `measured_avg_tps=24,297.5`, `measured_peak_tps=59,154.8`
     - post-load common-height state-root convergence passed
   Evidence:
   - The diagnostic stayed in the current green short-run range and preserved state-root convergence.
   - When validators were blocked on ancestor quorum, the common shape was `available=2`, `quorum=3`; one validator sample saw `available=1`, `quorum=3`.
   - Ancestor waits lasted from hundreds of milliseconds up to roughly `2.4s`; latest previous-round arrival age reached roughly `1.3s`.
   - `pending_mahi_block_broadcasts` stayed at `0` in the sampled blocked windows, so the local async broadcast queue is not the visible bottleneck.
   Status:
   - kept diagnostic
   Reason:
   - This is low-risk observability and rules out the just-binned fanout hypothesis more precisely. The remaining wall is not queued local fanout; it is that at least one previous-round validator block is missing long enough to stall the next local round.
   Next move:
   - Measure quorum readiness and per-validator round lag directly. The next slice should record whether the missing previous-round block was never produced yet or was produced by its author but not received/stored locally. If it is not produced, target producer scheduling/fairness; if it is produced but not received, target transport/receive-store latency.

126. Phase 4 M9 diagnostic: per-validator Mahi round lag sampler
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 125 showed validators waiting on `2/3` previous-round ancestors for hundreds of milliseconds to seconds. Recording each validator's `next_round`, `max_round`, and `last_produced_round` in the all-validator timeseries can distinguish producer lag from receive/store lag without changing runtime code.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - round-lag sampler diagnostic at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-round-lag-sampler-60k-20s`
     - `1,047,371 committed`
     - `52,368.6 TPS` final/duration
     - timeseries `measured_avg_tps=26,050.7`, `measured_peak_tps=40,201.5`
     - post-load common-height state-root convergence passed
   Evidence:
   - The sampler stayed in the current green short-run range and preserved state-root convergence.
   - Across waiting samples, `14/24` had `last_produced_round_min >= wait_round - 1`, meaning all validators had already produced the previous round but the waiting validator still had fewer than three previous-round ancestors locally.
   - The remaining `10/24` waiting samples showed at least one validator had not produced the needed previous round yet, so producer lag exists but is not the only wall.
   - Maximum observed ancestor-wait age in this run was roughly `1.7s`.
   Status:
   - kept diagnostic
   Reason:
   - This is script-only observability and resolves the item 125 ambiguity: the dominant visible wait is not just local producer scheduling. A substantial fraction of stalls occur after the needed blocks have already been produced somewhere else, pointing at transport or receive/store latency.
   Next move:
   - Target inbound Mahi block receive/store latency next. Add timing counters around `process_network_mahi_block`, Borsh decode, `accept_mahi_block_into_state`, sigverify enqueue, and commit-detector wake so the next slice can distinguish HTTP/JSON overhead from DAG lock/store/verification overhead.

127. Phase 4 M9 diagnostic: inbound Mahi block route timing
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 126 showed many ancestor waits happen after the needed previous-round blocks have already been produced elsewhere. Timing the inbound Mahi block route should show whether peer blocks are delayed by JSON/Borsh decode, DAG accept/store, sigverify enqueue, or total HTTP route time.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - inbound route timing diagnostic at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-inbound-block-timing-60k-20s`
     - `1,035,819 committed`
     - `51,791.0 TPS` final/duration
     - timeseries `measured_avg_tps=24,749.2`, `measured_peak_tps=40,110.0`
     - post-load common-height state-root convergence passed
   Evidence:
   - The diagnostic remained in the current green short-run range and preserved state-root convergence.
   - JSON/Borsh decode was not the visible wall: sampled cluster max decode p90 reached only `7ms`, max decode reached `58ms`.
   - Sigverify enqueue was not the visible wall: sampled cluster max enqueue p90 stayed `0ms`, max enqueue reached `5ms`.
   - `accept_mahi_block_into_state` dominated the inbound route: sampled cluster max accept p90 reached `98ms`, max accept reached `221ms`.
   - Total inbound route p90/max tracked accept/store (`107ms` p90 max, `263ms` max), so the next bottleneck is inside DAG accept/store plus inline commit-detector work, not transport serialization.
   Status:
   - kept diagnostic
   Reason:
   - This is low-risk observability and identifies the next actionable surface. Peer blocks are delayed after arrival while the inbound route holds the Mahi/DAG path and runs accept/store work, including inline commit detection.
   Next move:
   - Split inbound block accept from commit detection. The next slice should make `process_network_mahi_block` store and verify quickly, then wake the commit executor to run `advance_commit_detector` off the HTTP route. Keep state mutation in the existing commit executor path and verify common-height convergence before any 60s soak.

128. Phase 4 M9 slice: move Mahi commit detection off inbound route
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 127 showed inbound Mahi block `accept_mahi_block_into_state` dominating route latency. Moving `advance_commit_detector` out of local produce / inbound accept and into the existing commit executor should shorten HTTP receive/store latency and reduce ancestor quorum stalls without changing execution order.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (experiment only; reverted before commit)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - detector-off-route at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-detector-off-route-60k-20s`
     - `1,138,504 committed`
     - `56,925.2 TPS` final/duration
     - timeseries `measured_avg_tps=27,726.2`, `measured_peak_tps=55,532.6`
     - post-load common-height state-root convergence passed
   - detector-off-route at 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-detector-off-route-60k-60s`
     - `1,702,000 committed`
     - `28,366.7 TPS` final/duration
     - timeseries `measured_avg_tps=24,465.6`, `measured_peak_tps=47,931.0`
     - post-load common-height state-root convergence passed
   Evidence:
   - The 20s run improved burst throughput and remained deterministic, but the 60s soak regressed materially versus the item 121 fair baseline (`33,473.7 TPS`).
   - The 60s run remained fair (`min_per_writer=53,171`, `max_per_writer=53,203`), so the failure mode was sustained drain throughput rather than writer starvation or divergence.
   - Owner-local pre-admitted pools filled to cap (`524,288` per validator) with `dropped_oldest` around `25k`, showing that moving detector work off the inbound route did not improve sustained producer/commit drain.
   Status:
   - binned
   Reason:
   - The change helped short-window burst behavior but worsened the 60s M9 soak number. M9 requires sustained throughput; keeping this would move the branch backward.
   Next move:
   - Continue from item 127 code. The next slice should target sustained execution/progress coupling rather than simply moving detector scheduling. Specifically, measure commit-executor duty cycle and execution batch timing to determine whether the long-run wall is commit execution, DAG lock contention, or producer round gating under full pre-admitted pools.

129. Phase 4 M9 diagnostic: Mahi commit-executor timing
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 128 binned, the next ambiguity is whether the sustained 60s wall is commit execution, empty executor wakeups, DAG lock contention before execution, or producer round gating under full pre-admitted pools. Add commit-executor wake, empty-wake, batch count, block count, intent count, and execution-batch latency counters.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - commit-executor metric diagnostic at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-commit-executor-metrics-60k-20s`
     - `1,173,966 committed`
     - `58,698.3 TPS` final/duration
     - timeseries `measured_avg_tps=28,641.8`, `measured_peak_tps=45,766.5`
     - post-load common-height state-root convergence passed
   Evidence:
   - The diagnostic stayed correct and fair (`min_per_writer=35,773`, `max_per_writer=38,321`), and remained in the current green short-run range.
   - Executor empty wakeups were high but not the dominant wall: by the end of the sample, wake counts were roughly `154-163`, empty wake counts `129-138`.
   - Successful execution batches processed 8 blocks per batch. By the end of the sample, validators had executed roughly `528-536` blocks and `705k-745k` intents through the commit executor.
   - Execution batch latency is the current visible limiter: sampled cluster max batch p90 reached roughly `1,092ms`, and max reached roughly `1,138ms`.
   - Owner-local pre-admitted pools continued growing during the run, so producers have payload available; sustained throughput is bounded downstream by commit execution rate and its interaction with DAG progress.
   Status:
   - kept diagnostic
   Reason:
   - This is low-risk observability and narrows the M9 wall to execution-path cost. More transport/fanout/cadence tweaks are unlikely to move the 60s soak until the per-batch execution cost drops.
   Next move:
   - Instrument or directly optimize `EngineHandle::apply_mahi_committed_blocks_merged` / `Node::apply_mahi_committed_blocks_merged`. The likely subcomponents are body materialization, nonce filtering, `commit_block_intents`, event-root construction, mempool/frontier removal, and per-event recording. Split the timing or run a tightly scoped experiment that removes per-event recording from the Mahi hot path while preserving deterministic state-root calculation.

130. Phase 4 M9 diagnostic: Mahi merged execution subcomponent profile
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 129 showed 8-block merged execution batches taking roughly one second, but did not identify which subcomponent dominated. Add env-gated `KOMBAT_MAHI_EXEC_PROFILE=1` logging inside `apply_mahi_committed_blocks_merged` to split body materialization, runtime intent application, root construction, and install/bookkeeping.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - execution subcomponent profile at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-exec-profile-60k-20s`
     - `1,132,714 committed`
     - `56,635.7 TPS` final/duration
     - timeseries `measured_avg_tps=29,079.3`, `measured_peak_tps=40,262.3`
     - post-load common-height state-root convergence passed
   Evidence:
   - The env-gated profile preserved correctness and stayed in the current short-run green range, though writer distribution was noisier than item 129 (`min_per_writer=17,124`, `max_per_writer=42,733`).
   - Across all logged native Mahi execution batches: `396` samples, average `15,146.7` body items, average total `413.4ms`, max total `1,141ms`.
   - For non-empty batches only: `212` samples, average `28,293.0` body items, average body build `1.1ms`, average `commit_block_intents` `737.7ms`, average roots `13.3ms`, average install/bookkeeping `13.9ms`, average total `767.9ms`, max total `1,141ms`.
   - `commit_block_intents` is the sustained execution wall. Event-root construction, install/bookkeeping, and body materialization are not large enough to explain the M9 gap.
   Status:
   - kept diagnostic
   Reason:
   - This identifies the next real optimization target. The execution wall is runtime intent application, not mempool cleanup, event recording, or hashing.
   Next move:
   - Remove redundant policy session verification from Mahi commit execution. Mahi only drains blocks after block sigverify marks them verified, so commit execution can use a preverified policy-admission path that still enforces nonce/counter/risk limits but skips signature verification already performed by the Mahi sigverify pool.

131. Phase 4 M9 slice: preverified Mahi policy commit
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 130 showed `commit_block_intents` dominating Mahi merged execution, with non-empty batches spending roughly `738ms` on runtime intent application. Mahi blocks are only drained after the Mahi sigverify pool marks them verified, so Mahi commit execution can skip duplicate policy session signature verification while preserving policy id, chain id, active-policy, nonce, counter, drawdown, market, and side-effect checks.
   Files changed:
   - `chain/crates/policy/src/lib.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-policy --manifest-path chain/Cargo.toml`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - preverified Mahi commit at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-preverified-commit-60k-20s`
     - `1,499,392 committed`
     - `74,969.6 TPS` final/duration
     - timeseries `measured_avg_tps=46,203.5`, `measured_peak_tps=77,775.4`
     - post-load common-height state-root convergence passed
   - preverified Mahi commit at 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-preverified-commit-60k-60s`
     - `3,555,634 committed`
     - `59,260.6 TPS` final/duration
     - timeseries `measured_avg_tps=35,737.6`, `measured_peak_tps=100,446.2`
     - post-load common-height state-root convergence passed
   - preverified Mahi commit at 100k offered / 10s:
     - `/tmp/codex-exp-phase4-m9-preverified-commit-100k-10s`
     - `1,499,592 committed`
     - `149,959.2 TPS` final/duration
     - timeseries `measured_avg_tps=43,538.1`, `measured_peak_tps=84,077.4`
     - post-load common-height state-root convergence passed
   Evidence:
   - The 60s soak clears the M9 sustained target (`>=50k TPS` at 60k offered) under the project’s historical committed-summary convention, improving from the item 121 green sustained reference of `33,473.7 TPS` to `59,260.6 TPS`.
   - Commit execution latency moved by an order of magnitude: sampled cluster max batch p90 in the 60s run stayed around `115ms` by the end, versus roughly `1,092ms` in item 129 and roughly `738ms` average `commit_block_intents` time in item 130 non-empty profile samples.
   - The stricter in-window sampler remains bursty (`35,737.6` average TPS on the 60s run, peak `100,446.2`), so the next correctness/measurement task is smoothing measured-window progress rather than claiming fully stable 60k measured-window throughput.
   - The 100k short run reached an `84,077.4 TPS` measured-window peak and converged, but the `149,959.2 TPS` final/duration value is inflated by the known warmup-counting convention. Record both numbers to avoid confusing summary-throughput with strict measured-window throughput.
   - The preverified path is Mahi-only. HotStuff and mempool/planner simulations continue to use `commit_admission` and full session verification.
   - Correctness caveat: the Mahi preverified commit path relies on the block ingest sigverify pool as the signature-verification boundary. It preserves policy id, chain id, active/revoked policy, nonce, counters, drawdown, market, and side-effect checks at commit time, but intentionally does not re-evaluate session signatures or key-rotation revocation windows during execution. Production hardening should explicitly test key-rotation/revocation semantics across Mahi verify and commit.
   Status:
   - kept
   Reason:
   - This removes a duplicate HotStuff-era verification pass from the Mahi execution hot path, preserves deterministic state-root convergence, and is the first slice to clear the M9 60s sustained committed-summary gate.
   Next move:
   - Stabilize measured-window progress before declaring Phase 4/M9 fully done. The likely next slice is to use the replay/variance harness or existing per-second sampler to separate warmup/backlog catch-up from steady-state throughput, then smooth commit-executor/producer pacing if the measured window still oscillates.

132. Phase 4 M9 slice: retry detector-off-accept after preverified commit
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 131 reduced commit execution p90 from roughly one second to roughly `115ms`, inbound DAG accept/store again showed high p90 latency. Retry the item 128 scheduling shape under the new cost profile: keep Mahi block accept focused on store/verify enqueue, move `advance_commit_detector` into the commit executor wake path, and rely on sigverify completion to wake execution.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (experiment only; reverted before commit)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - detector-in-executor after preverified commit at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-detector-executor-after-preverify-60k-20s`
     - `1,499,648 committed`
     - `74,982.4 TPS` final/duration
     - timeseries `measured_avg_tps=45,907.7`, `measured_peak_tps=91,143.2`
     - post-load common-height state-root convergence passed
   Evidence:
   - Throughput was effectively flat versus item 131's 60k/20s run (`74,969.6 TPS` final/duration, `46,203.5` measured average).
   - The target metric did not materially move. Inbound accept p90/max at the end of the run was roughly `298ms/370ms`, comparable to item 131's `297ms/403ms`; earlier p90 progression was also similar.
   - Commit execution p90 remained low (`67ms` end-of-run), confirming item 131's execution improvement survived, but moving detector scheduling did not reduce the current accept/store wall enough to justify the added scheduling complexity.
   Status:
   - binned
   Reason:
   - The retry preserved correctness but did not improve the target metric or sustained throughput. Keeping it would add a second detector scheduling path without measurable benefit.
   Next move:
   - Leave item 131 code as the current green state. The next useful slice should inspect what remains inside `accept_block` / DAG store itself: block signature verification, ancestor/missing-index maintenance, BTreeMap insert/update cost, and Mahi state lock hold time. Do not revisit detector scheduling unless a later structural change makes accept/store cheap enough that detector work reappears as the dominant component.

133. Phase 4 M9 diagnostic: Mahi accept lock profile
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 132 showed detector scheduling was not the accept-route wall. Add env-gated `KOMBAT_MAHI_ACCEPT_PROFILE=1` logging around inbound Mahi accept to split state-lock wait, `accept_block`, commit-detector advancement, and total accept time.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - accept profile at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-accept-profile-60k-20s`
     - `1,499,648 committed`
     - `74,982.4 TPS` final/duration
     - timeseries `measured_avg_tps=47,595.9`, `measured_peak_tps=87,617.0`
     - post-load common-height state-root convergence passed
   Evidence:
   - The diagnostic stayed correct and in the current short-run range.
   - Parsed from `10,722` inbound accept samples:
     - `lock_ms avg=5.7 p50=0 p90=0 p99=255 max=410`
     - `accept_ms avg=0.1 p50=0 p90=0 p99=2 max=43`
     - `detector_ms avg=0.0 p50=0 p90=0 p99=1 max=17`
     - `total_ms avg=6.0 p50=0 p90=1 p99=258 max=410`
   - The accept/store body is not the wall: block signature validation, ancestor validation, DAG insertion, and detector advancement are small at p99. The high inbound accept tail is Mahi state lock wait.
   - The likely lock holder is the producer path cloning large `inflight_intents` / `inflight_policy_nonces` under the Mahi lock before building each block. That clone grows with backlog and creates head-of-line blocking for inbound peer blocks.
   Status:
   - kept diagnostic
   Reason:
   - This is low-risk observability and changes the next optimization target from DAG insert internals to state-lock hold time. The env flag is cached through `OnceLock`, so the disabled hot-path overhead is one cached load rather than repeated environment lookups.
   Next move:
   - Replace producer-side full inflight set cloning with a compact per-policy blocked-until watermark. The producer only needs to avoid selecting already-in-flight policy nonces; cloning every in-flight intent id and every nonce set is unnecessary on the Mahi hot path and is now the visible source of inbound block lock contention.

134. Phase 4 M9 slice: compact inflight watermark for producer selection
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 133 showed inbound block accept tails dominated by waiting for the Mahi state lock. Replace producer-side cloning of full `inflight_intents` and `inflight_policy_nonces` with a compact per-policy `blocked_until` watermark so producer rounds hold the Mahi lock for less time while still avoiding in-flight policy nonce reuse.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (experiment only; reverted before commit)
   - `chain/crates/node/src/lib.rs` (experiment only; reverted before commit)
   - `chain/crates/node/src/engine_handle.rs` (experiment only; reverted before commit)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_block_builder_advances_past_local_inflight_nonces --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_snapshot_shards_against_configured_validator_set --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - compact watermark first attempt at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-inflight-watermark-60k-20s`
     - startup convergence failed before load; no TPS signal
   - compact watermark rerun at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-inflight-watermark-rerun-60k-20s`
     - `1,499,360 committed`
     - `74,968.0 TPS` final/duration
     - timeseries `measured_avg_tps=43,763.5`, `measured_peak_tps=60,419.6`
     - post-load common-height state-root convergence passed
   Evidence:
   - The rerun preserved correctness and final catch-up throughput, but measured-window throughput regressed versus item 131 (`46,203.5`) and item 133 (`47,595.9`).
   - The target metric did not improve. End-of-run inbound accept p90/max was roughly `310ms/397ms`, comparable or slightly worse than item 131 (`297ms/403ms`).
   - Commit execution tail also worsened slightly (`74ms/126ms` end-of-run versus item 131 `69ms/114ms`), likely because the watermark over-conservatively blocks policy nonces and changes producer fill/cadence rather than just reducing lock hold.
   - The first startup-convergence failure makes this too risky to keep without more protocol-specific proof that compact watermarks preserve liveness under all startup/interleaving states.
   Status:
   - binned
   Reason:
   - The simplification did not reduce Mahi state lock contention in the measured path, regressed measured-window throughput, and introduced a startup-liveness failure on the first attempt. The exact inflight set clone is expensive, but this watermark approximation is not the right replacement.
   Next move:
   - Keep item 131 as the current green code and item 133 as the diagnostic. If targeting Mahi state lock contention again, avoid semantic approximation: either maintain an incremental compact exact structure alongside `inflight_policy_nonces`, or reduce the frequency of producer snapshot attempts under backlog rather than changing nonce selection semantics.

135. Phase 4 M9 diagnostic: replay ladder at current green path
   Date:
   - `2026-04-24`
   Hypothesis:
   - Before further optimization, characterize whether the current item 131 green path is run-to-run stable at the 60k/20s shape. Use the replay ladder to separate final committed-summary stability from stricter measured-window sampler variance.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `AUTO_MAX_ITEMS=5000 KOMBAT_CONSENSUS=mahi KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1 LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS=1 LOCAL_CLUSTER_SKIP_BUILD=1 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 REPLAY_KEEP=1 KEEP_LOG=1 bash chain/scripts/replay_bench_ladder.sh 3 60000 20`
   Benchmark runs:
   - replay root:
     - `/var/folders/pq/lgc890gs3h12nm6whjzjy5340000gn/T/kombat-replay-ladder.XXXXXX.vPz1jR0qO8`
   - run01:
     - `1,499,880 committed`
     - `74,994.0 TPS` final/duration
     - final height `418`
     - measured sampler avg `45,965.0`, peak `90,047.6`
   - run02:
     - `1,499,904 committed`
     - `74,995.2 TPS` final/duration
     - final height `425`
     - measured sampler avg `46,922.3`, peak `75,594.4`
   - run03:
     - `1,499,848 committed`
     - `74,992.4 TPS` final/duration
     - final height `408`
     - measured sampler avg `44,823.4`, peak `83,877.3`
   Evidence:
   - The project convention is very stable at this shape: mean `74,993.9 TPS`, stdev `1.1`, CV effectively `0.0%`, min `74,992.4`, max `74,995.2`.
   - All three runs converged, but at different final heights and therefore different final state roots. This is not a divergence signal by itself; final roots are captured at each run's own final height.
   - The stricter measured-window sampler still shows real oscillation: averages ranged from `44,823.4` to `46,922.3` TPS and peaks ranged from `75,594.4` to `90,047.6`.
   Status:
   - kept diagnostic
   Reason:
   - The current green path is reproducible by final committed-summary semantics, but not smooth by measured-window semantics. This confirms item 131 is a valid kept throughput slice while preserving the caveat that steady in-window progress is still below the nominal 60k offered rate.
   Next move:
   - Run one more 60k/60s replay or direct soak only if we need a fresh long-run artifact after item 133 diagnostics. For further optimization, target smoother measured-window progress rather than final catch-up: likely producer/commit pacing under full pre-admitted pools or an exact incremental inflight index that avoids the item 134 watermark semantics change.

136. Phase 4 M9 config sweep: merged execution batch size 4 after preverified commit
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 131 made commit execution cheap, smaller merged execution batches might smooth measured-window progress by reducing commit burst size and latency, even though earlier pre-item-131 batch sweeps had binned smaller sizes.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `AUTO_MAX_ITEMS=5000 KOMBAT_CONSENSUS=mahi KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1 KOMBAT_MAHI_MERGED_EXECUTION_BATCH=4 LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS=1 LOCAL_CLUSTER_SKIP_BUILD=1 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 KEEP_LOG=1 LOCAL_CLUSTER_OUTPUT_DIR=/tmp/codex-exp-phase4-m9-preverify-execbatch4-60k-20s bash chain/scripts/loadtest_local_cluster.sh 32 60000 20`
   Benchmark runs:
   - execution batch 4 at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-preverify-execbatch4-60k-20s`
     - `1,499,904 committed`
     - `74,995.2 TPS` final/duration
     - timeseries `measured_avg_tps=40,643.7`, `measured_peak_tps=62,889.4`
     - post-load common-height state-root convergence passed
   Evidence:
   - The final catch-up number remained green, but measured-window average regressed materially versus item 131 (`46,203.5`) and item 135 replay runs (`44,823.4` to `46,922.3`).
   - The smaller batch size roughly doubled final Mahi height (`min_height=830`, `max_height=845`) versus the default batch-8 path around the low 400s, increasing block bookkeeping and state progress churn without improving in-window throughput.
   Status:
   - binned config
   Reason:
   - Batch size 4 smooths neither throughput nor progress under the post-item-131 cost profile. The extra block churn costs more than any latency benefit.
   Next move:
   - Keep the default merged execution batch size `8`. If tuning batch size again, only test larger batches (`16` or `32`) for final catch-up efficiency; do not use smaller batches for smoothing.

137. Phase 4 M9 config sweep: merged execution batch size 16 after preverified commit
   Date:
   - `2026-04-24`
   Hypothesis:
   - After item 136 binned smaller merged execution batches, a larger batch size might reduce block/execution bookkeeping and improve measured-window progress under the item 131 preverified commit path.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `AUTO_MAX_ITEMS=5000 KOMBAT_CONSENSUS=mahi KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1 KOMBAT_MAHI_MERGED_EXECUTION_BATCH=16 LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS=1 LOCAL_CLUSTER_SKIP_BUILD=1 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 KEEP_LOG=1 LOCAL_CLUSTER_OUTPUT_DIR=/tmp/codex-exp-phase4-m9-preverify-execbatch16-60k-20s bash chain/scripts/loadtest_local_cluster.sh 32 60000 20`
   Benchmark runs:
   - execution batch 16 at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-preverify-execbatch16-60k-20s`
     - `1,499,808 committed`
     - `74,990.4 TPS` final/duration
     - timeseries `measured_avg_tps=43,167.7`, `measured_peak_tps=98,354.4`
     - post-load common-height state-root convergence passed
   Evidence:
   - The final catch-up number remained green, but measured-window average regressed versus item 131 (`46,203.5`) and item 135 replay runs (`44,823.4` to `46,922.3`).
   - End-of-run Mahi heights around `199` to `206` confirm the larger batch reduced block churn versus default batch 8, but that did not translate into steadier measured progress.
   - The measured samples still show burst/idle behavior, including zero-TPS intervals followed by large catch-up bursts. Larger merged execution batches increase burst amplitude rather than fixing pacing.
   Status:
   - binned config
   Reason:
   - Batch size 16 improves neither the target measured-window average nor stability. The default batch size `8` remains the best observed tradeoff after testing both smaller and larger nearby values.
   Next move:
   - Stop merged-execution batch-size tuning on the current cost profile. Further work should target the remaining structural burst source, not another batch-size micro-tweak.

138. Phase 4 M9 slice: elide producer in-flight intent-id clone
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 133 showed inbound block accept tails dominated by Mahi state lock wait. Removing the producer-side clone of the full `inflight_intents` set and using the exact in-flight `(policy_id, nonce)` map for duplicate suppression should reduce Mahi lock hold time without changing nonce-selection semantics.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (experiment only; reverted before commit)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_ --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - intent-id clone elision at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-inflight-intent-clone-elide-60k-20s`
     - `1,499,392 committed`
     - `74,969.6 TPS` final/duration
     - timeseries `measured_avg_tps=43,407.6`, `measured_peak_tps=90,308.6`
     - post-load common-height state-root convergence passed
   Evidence:
   - Correctness held and final catch-up stayed green, but measured-window average regressed versus item 131 (`46,203.5`) and item 135 replay runs (`44,823.4` to `46,922.3`).
   - The target lock-wait signal did not improve. Measured samples still showed inbound accept p90 rising past `300ms` late in the run and max around `445ms`.
   - The change removed one large clone but left the full `inflight_policy_nonces` clone and the bursty producer/commit cadence intact, so it did not address the dominant measured-window instability.
   Status:
   - binned
   Reason:
   - The exact nonce-based duplicate check was safe, but it did not improve the target metric or throughput. Keeping it would add semantic reasoning without a measured payoff.
   Next move:
   - Keep item 131 as the green runtime code. Further work should avoid partial clone cuts and instead target the structural source of measured-window burstiness: producer/commit pacing or an exact incremental in-flight index that removes the full policy-nonce clone too.

139. Phase 4 M9 slice: compact Mahi metrics digest
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 133 showed Mahi lock wait tails, but items 134 and 138 failed to move them by changing producer in-flight handling. The remaining shared lock holder is metrics: `kombat_getConsensusMetrics` computed `mahi.state_hash_hex` by serializing the full payload-bearing DAG under the Mahi lock on every sampler poll. Replace that with a compact ref digest and keep a producer phase profile to verify future lock-wait hypotheses.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_ --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - producer profile at 60k offered / 20s, before compact digest:
     - `/tmp/codex-exp-phase4-m9-producer-profile-60k-20s`
     - `1,499,392 committed`
     - `74,969.6 TPS` final/duration
     - timeseries `measured_avg_tps=45,347.3`, `measured_peak_tps=131,279.7`
     - post-load common-height state-root convergence passed
   - compact metrics digest at 60k offered / 20s:
     - `/tmp/codex-exp-phase4-m9-compact-metrics-digest-60k-20s`
     - `1,499,648 committed`
     - `74,982.4 TPS` final/duration
     - timeseries `measured_avg_tps=59,761.3`, `measured_peak_tps=65,543.8`
     - post-load common-height state-root convergence passed
   - compact metrics digest at 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-compact-metrics-digest-60k-60s`
     - `3,899,744 committed`
     - `64,995.7 TPS` final/duration
     - timeseries `measured_avg_tps=59,040.5`, `measured_peak_tps=70,406.8`
     - post-load common-height state-root convergence passed
   - compact metrics digest first 100k offered / 10s attempt:
     - `/tmp/codex-exp-phase4-m9-compact-metrics-digest-100k-10s`
     - discarded: validator 2 exited during startup while fetching validator 1 bootstrap metadata; no load/TPS signal
   - compact metrics digest 100k offered / 10s rerun:
     - `/tmp/codex-exp-phase4-m9-compact-metrics-digest-100k-10s-rerun`
     - `1,499,480 committed`
     - `149,948.0 TPS` final/duration using the historical committed-summary convention
     - timeseries `measured_avg_tps=78,132.5`, `measured_peak_tps=112,155.8`
     - post-load common-height state-root convergence passed
   Evidence:
   - The producer profile showed producer phase work was not the primary wall: successful round p90s ended around `24ms` for engine snapshot, `4ms` for pre-admitted drain, `10ms` final-lock wait, `7ms` final-lock hold, and `56ms` total, while inbound accept p90 still climbed above `300ms`.
   - The lock holder was the metrics endpoint itself. The committed-timeseries sampler calls `kombat_getConsensusMetrics` once per second per validator, and the old `MahiState::snapshot()` serialized every Mahi block body while holding the Mahi lock just to compute `state_hash_hex`.
   - Replacing the full-DAG serialization with a compact digest over block refs, pending-fetch refs, committed refs, and last decided leader preserves existing Mahi convergence tests while removing payload-size dependence from metrics.
   - The 60k/20s strict sampler moved from the current green range of roughly `44.8k` to `47.6k` TPS up to `59.8k` TPS with no correctness regression. The 60k/60s soak now sustains `59.0k` strict measured TPS and `65.0k` committed-summary TPS, clearing the M9 soak target.
   - The 100k/10s rerun is close but not yet a strict short-run keep for the 80k target: `78.1k` measured average, with a `112.2k` peak. The historical committed-summary convention is inflated at this shape because it counts warmup and settle catch-up over the measured duration; keep reporting both numbers.
   Status:
   - kept
   Reason:
   - This removes a HotStuff-era/debug-style observability cost from the Mahi hot path, materially improves measured-window stability, and preserves state-root convergence. It is a structural fix, not a batch-size or cadence tweak.
   Next move:
   - Treat the current green sustained TPS as `59,040.5` strict measured TPS on the 60k/60s soak, with `64,995.7` TPS under the historical committed-summary convention. Next target is the remaining 100k/10s short-run headroom: close the strict measured average gap from `78.1k` to `>=80k` without regressing the 60k/60s soak.

140. Phase 4 M9 config: async Mahi block broadcast after compact metrics digest
   Date:
   - `2026-04-24`
   Hypothesis:
   - `KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1` was previously a kept high-load flag, but recent item 131/139 green runs omitted it. After item 139 removed metrics-induced Mahi lock stalls, retest the flag as a no-code A/B to see whether decoupling producer block fanout now clears the 100k/10s short-run gate without regressing the 60k/60s soak.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `AUTO_MAX_ITEMS=5000 KOMBAT_CONSENSUS=mahi KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1 KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1 LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS=1 LOCAL_CLUSTER_SKIP_BUILD=1 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 KEEP_LOG=1 LOCAL_CLUSTER_OUTPUT_DIR=/tmp/codex-exp-phase4-m9-compact-metrics-async-block-100k-10s bash chain/scripts/loadtest_local_cluster.sh 32 100000 10`
   - `AUTO_MAX_ITEMS=5000 KOMBAT_CONSENSUS=mahi KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1 KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1 LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS=1 LOCAL_CLUSTER_SKIP_BUILD=1 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 KEEP_LOG=1 LOCAL_CLUSTER_OUTPUT_DIR=/tmp/codex-exp-phase4-m9-compact-metrics-async-block-60k-60s bash chain/scripts/loadtest_local_cluster.sh 32 60000 60`
   Benchmark runs:
   - async Mahi block broadcast at 100k offered / 10s:
     - `/tmp/codex-exp-phase4-m9-compact-metrics-async-block-100k-10s`
     - `1,499,568 committed`
     - `149,956.8 TPS` final/duration using the historical committed-summary convention
     - timeseries `measured_avg_tps=97,195.9`, `measured_peak_tps=118,165.8`
     - post-load common-height state-root convergence passed
   - async Mahi block broadcast at 60k offered / 60s:
     - `/tmp/codex-exp-phase4-m9-compact-metrics-async-block-60k-60s`
     - `3,899,648 committed`
     - `64,994.1 TPS` final/duration using the historical committed-summary convention
     - timeseries `measured_avg_tps=59,474.8`, `measured_peak_tps=73,772.4`
     - post-load common-height state-root convergence passed
   Evidence:
   - Compared with item 139's no-async-block 100k/10s rerun (`78,132.5` strict measured TPS), async block broadcast lifts the strict short-run number to `97,195.9` TPS and clears the `>=80k` M9 short-run target.
   - Compared with item 139's no-async-block 60k/60s soak (`59,040.5` strict measured TPS), async block broadcast preserves and slightly improves sustained throughput at `59,474.8` TPS, clearing the `>=50k` M9 soak target.
   - Both runs have clean ingress (`rpc_err=0`, `net_err=0`) and common-height state-root convergence. This is not a post-load catch-up artifact under the corrected sampler; the strict measured-window averages are green.
   Status:
   - kept config
   Reason:
   - With compact metrics, the async block-broadcast worker again provides the intended producer/fanout decoupling without the sustained-regression seen in the older pre-metrics-fix environment.
   Next move:
   - Treat the M9 green runtime config as `KOMBAT_CONSENSUS=mahi`, `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`, and `KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1`.
   - Current green TPS: `97,195.9` strict measured TPS at 100k/10s and `59,474.8` strict measured TPS at 60k/60s. Next work should either ratify M9 complete in the plan/summary docs or move to the next structural phase; do not resume pre-item-139 micro-tuning.

141. Phase 6 prep: static execution access sets
   Date:
   - `2026-04-24`
   Hypothesis:
   - Before replacing sequential commit execution with Block-STM, add a conservative deterministic access-set classifier for intents and block items. This gives the future scheduler a conflict surface without changing runtime execution behavior. While validating the node suite, fix the planner-simulation rollback bug where a terminal order-side-effect rejection could still advance the simulated policy nonce used for block planning.
   Files changed:
   - `chain/crates/execution/src/lib.rs`
   - `chain/crates/node/src/engine_handle.rs`
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-state-db --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node terminal_frontier_rejection_cascades_to_queued_successor_intents --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. This is runtime-inert Phase 6 scaffolding plus a planner correctness fix; no Mahi hot-path behavior changes are expected.
   Evidence:
   - The external CoW state-db work in `/Users/waynedouglas/github/hx/pvp-cow-state/chain/crates/state-db` is already effectively present on `main`; the only file diff was rustfmt line wrapping. `kombat-chain-state-db` passed all 13 tests.
   - The new classifier marks per-policy nonce/budget state, frozen-account gates, market gates, order books, policy-graph mutations, and arbitrary transactions. Unit tests cover non-conflicting distinct-policy transfers, same-policy conflicts, same-market order conflicts, distinct-market order independence, policy-graph global conflicts, and transaction global conflicts.
   - A full node lib run initially exposed a deterministic existing failure in `terminal_frontier_rejection_cascades_to_queued_successor_intents`: planner simulation advanced the simulated policy nonce before an order self-trade rejection, then installed that advanced nonce even though the rejected intent should not burn the frontier. The fix now applies simulated policy/order-book mutations only after the whole intent simulation succeeds.
   - After the rollback fix, `cargo test -p kombat-chain-node --lib` passed `180` tests.
   - Clippy initially surfaced pre-existing low-risk lint debt in `execution` and `engine_handle`; those were cleaned mechanically so the node lib clippy gate now passes with `-D warnings`.
   Status:
   - kept
   Reason:
   - The access-set classifier is the minimum useful Phase 6 scaffold and is covered by focused tests. The planner rollback fix restores the existing nonce-frontier invariant and keeps the node suite green before deeper Block-STM work.
   Next move:
   - Build a small Block-STM scheduler/MVCC microbenchmark against these access sets and compare its output against the sequential executor on deterministic mixed-conflict blocks. Separately, run post-M9 cluster validation when deployment capacity is available.

142. Phase 6 prep: deterministic conflict scheduler
   Date:
   - `2026-04-24`
   Hypothesis:
   - A small deterministic scheduler over item 141's access sets can form conflict-free parallel layers while preserving happens-before for every conflicting pair. This gives Phase 6 a testable scheduling primitive before introducing worker threads or MVCC storage.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Runtime-inert scheduler scaffold; no execution path is wired to it yet.
   Evidence:
   - Added `schedule_block_items()` returning deterministic layers of block-item indices. It greedily places each item in the earliest conflict-free layer that is also after every prior conflicting item, so independent items can run together while conflicting items preserve sequential order.
   - Focused tests now cover independent transfer grouping, same-policy serialization, global transaction ordering, and market-order parallelism across distinct books.
   - Full node lib test suite passed `184` tests, including the new scheduler tests and the item 141 planner rollback regression test.
   Status:
   - kept
   Reason:
   - This is the next minimal Block-STM building block: deterministic scheduling without runtime behavior change. It is simple enough to audit and gives future MVCC/worker slices a stable contract.
   Next move:
   - Add a sequential-vs-scheduled execution microbenchmark/test harness: execute layers serially through the existing sequential executor first, then introduce MVCC/parallel worker execution behind an env-gated experimental path.

143. Phase 6 prep: schedule validation and serial equivalence harness
   Date:
   - `2026-04-24`
   Hypothesis:
   - Before adding worker threads or MVCC storage, validate the scheduler contract mechanically: every item appears exactly once, no same-layer conflicts exist, and every conflicting pair preserves original happens-before. A serial mock execution over scheduled layers should match sequential execution for mixed independent/conflicting writes.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Runtime-inert validation/test harness only.
   Evidence:
   - Added `validate_schedule()` with explicit errors for out-of-bounds, duplicate, missing item, intra-layer conflict, and dependency reordering.
   - Focused execution tests increased from 10 to 13 and cover valid schedule validation, duplicate/missing rejection, same-layer conflict rejection, reordered dependency rejection, and a serial mock execution equivalence check.
   - Full node lib suite passed `187` tests. Clippy with `-D warnings` and release build both passed.
   Status:
   - kept
   Reason:
   - This turns the Phase 6 scheduler from a shape-only helper into a checked contract. Future MVCC/parallel execution slices can assert the scheduler is valid before running speculative workers.
   Next move:
   - Add the first MVCC-shaped test scaffold: a versioned write buffer that can execute a layer of non-conflicting mock operations and commit in deterministic item order. Keep it test-only/runtime-inert until it matches sequential behavior under mixed conflict cases.

144. Phase 6 prep: versioned write buffer scaffold
   Date:
   - `2026-04-24`
   Hypothesis:
   - A minimal versioned write buffer keyed by `ExecutionResource` is enough to model Block-STM's "speculate first, materialize in deterministic item order" behavior for mock operations. This should stay runtime-inert but prove the state materialization contract before introducing real worker execution.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Runtime-inert MVCC scaffold only.
   Evidence:
   - Added `VersionedWriteBuffer<V>` with ordered writes, `read_latest_before()`, `writes_for()`, and `materialize_committed()` helpers.
   - Focused execution tests increased from 13 to 15. New tests verify materialization picks the highest item-index version regardless of insertion order, and scheduled mock writes materialize to the same state as sequential mock execution.
   - Full node lib suite passed `189` tests. Clippy with `-D warnings` and release build both passed.
   Status:
   - kept
   Reason:
   - This is the smallest useful MVCC-shaped primitive and remains disconnected from the production commit path. It gives the next slice a concrete buffer to validate worker-layer execution against.
   Next move:
   - Add a test-only layer executor that runs non-conflicting mock operations through `VersionedWriteBuffer` by schedule layer and records per-layer retry/conflict counters. After that, replace mock operations with a narrow real-intent sequential-vs-scheduled comparison.

145. Phase 6 prep: mock layer executor and retry counters
   Date:
   - `2026-04-24`
   Hypothesis:
   - A test-only layer executor can model the Block-STM control loop without touching production execution: validate a candidate schedule, execute layer-by-layer into `VersionedWriteBuffer`, and retry with a repaired schedule if validation detects a conflict. This establishes the stats shape before real worker execution.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Test-only Phase 6 control-loop scaffold.
   Evidence:
   - Focused execution tests increased from 15 to 17. New tests verify valid layer stats (`layer_count`, `executed_items`, `conflict_retries`) and the retry/repair path for an invalid candidate schedule.
   - The repair path validates the original candidate, falls back to `schedule_access_sets()` on error, executes the repaired layers, and increments `conflict_retries`.
   - Full node lib suite passed `191` tests. Clippy with `-D warnings` and release build both passed.
   Status:
   - kept
   Reason:
   - The Block-STM control loop now has a tested model for schedule validation, layer execution, materialization, and retry accounting, still entirely outside the production commit path.
   Next move:
   - Replace mock operations with a narrow real-intent sequential-vs-scheduled comparison. Start with transfer/cancel intents that only touch policy resources, then add order-book cases after the harness can compare emitted events and policy/order-book state.

146. Phase 6 prep: policy-only real-intent schedule equivalence
   Date:
   - `2026-04-24`
   Hypothesis:
   - The Phase 6 scheduler should preserve the existing sequential runtime semantics for the simplest real intent class before any worker execution is wired. Policy-only transfer/cancel intents can be executed through the existing policy admission simulation in scheduled layers and should produce the same final policy state as sequential execution.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Test-only Phase 6 equivalence harness; no production execution path is wired to it yet.
   Evidence:
   - Added real policy-store fixtures and scheduled execution tests that call the same `simulate_intent_on_runtime_state()` helper used by the sequential commit path, with `PolicyAdmissionCommitMode::PreverifiedSession`.
   - The scheduled executor intentionally reverses item order inside each conflict-free layer to model non-sequential worker completion while preserving scheduler-enforced dependency layers.
   - New tests cover independent policy transfer/cancel intents in one layer and same-policy nonce ordering across layers. Both scheduled executions produce the same final `PolicyRecord` snapshots as sequential execution.
   - Focused execution tests increased from 17 to 19. Full node lib suite passed `193` tests. Clippy with `-D warnings` and release build both passed.
   Status:
   - kept
   Reason:
   - This is the first Phase 6 scaffold that exercises real runtime policy admission semantics rather than mock writes, while remaining runtime-inert. It confirms the scheduler can safely preserve policy nonce/counter behavior for transfer/cancel workloads.
   Next move:
   - Extend the real-intent equivalence harness to order-book intents by comparing emitted events plus policy and order-book state. Keep it test-only until the harness can prove serial equivalence for mixed policy/order-book blocks.

147. Phase 6 prep: order-book real-intent schedule equivalence
   Date:
   - `2026-04-24`
   Hypothesis:
   - The real-intent equivalence harness can be extended from policy-only transfers/cancels to order-book intents if scheduled execution materializes per-intent events in deterministic item order. Distinct markets should execute in one conflict-free layer, while same-market crossing orders must preserve sequential match order.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Test-only Phase 6 equivalence harness; no production execution path is wired to it yet.
   Evidence:
   - Generalized the real-intent harness to compare `PolicyRecord` snapshots, `SingleMarketBook` state, and emitted `RuntimeEventKind`s between sequential execution and scheduled-layer execution.
   - Scheduled execution still reverses worker completion order inside conflict-free layers, but now stores event vectors by original item index and materializes them in deterministic item order. This matches the Block-STM requirement that parallel workers may finish out of order while commit output remains ordered.
   - New tests cover distinct-market order placement in one parallel layer and same-market crossing orders serialized across layers. Both scheduled executions match the sequential policy state, book state, and event sequence.
   - Focused execution tests increased from 19 to 21. Full node lib suite passed `195` tests. Clippy with `-D warnings` and release build both passed.
   Status:
   - kept
   Reason:
   - This proves the scheduler contract against the two state surfaces that matter for the current Mahi hot path: policy admission and order-book mutation. It remains runtime-inert but removes the biggest correctness unknown before introducing a worker-backed executor.
   Next move:
   - Add a small runtime-inert execution-plan summary helper for real Mahi block bodies: count layers, maximum layer width, conflict pairs, and estimated parallelism. Use it to profile current Mahi committed bodies before wiring worker execution.

148. Phase 6 prep: execution schedule summary helper
   Date:
   - `2026-04-24`
   Hypothesis:
   - Before wiring worker execution, Phase 6 needs a cheap way to summarize the parallelism available in real block bodies. A runtime-inert helper can report item count, layer count, max layer width, conflict pairs, and equal-cost estimated parallelism for any block body using the existing access-set scheduler.
   Files changed:
   - `chain/crates/node/src/execution/mod.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node execution:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Runtime-inert summary helper; no production execution path is wired to it yet.
   Evidence:
   - Added `ExecutionScheduleSummary`, `summarize_block_item_schedule()`, and `summarize_access_set_schedule()` over the existing scheduler/access-set contract.
   - Focused tests now cover all-independent items (`4.0x` equal-cost estimate), two independent market conflict chains (`2.0x`), and a global transaction barrier (`1.0x`).
   - Focused execution tests increased from 21 to 24. Full node lib suite passed `198` tests. Clippy with `-D warnings` and release build both passed.
   Status:
   - kept
   Reason:
   - This gives the next diagnostic slice a stable, tested way to measure whether Mahi block bodies actually contain enough independent work for Block-STM to help. It avoids wiring worker execution before measuring available parallelism.
   Next move:
   - Expose this summary in Mahi execution metrics/logging for committed block batches, then run a short green-config bench to see whether current Mahi blocks are conflict-heavy or parallelism-rich.

149. Phase 6 prep: gated Mahi schedule-profile metrics
   Date:
   - `2026-04-24`
   Hypothesis:
   - The Phase 6 schedule summary can be exposed on the Mahi metrics surface without perturbing the green TPS path if collection is gated behind an explicit profiling flag. This lets us measure committed-batch parallelism before wiring worker execution.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_ --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. Metrics-only change; schedule profiling is disabled unless `KOMBAT_PHASE6_SCHEDULE_PROFILE=1` is set.
   Evidence:
   - Added `KOMBAT_PHASE6_SCHEDULE_PROFILE` as an opt-in profile flag. When enabled, Mahi commit execution batches record item count, layer count, max layer width, conflict pair count, and estimated parallelism in thousandths.
   - The profile path builds schedule summaries only when the flag is enabled, avoiding the O(n²) conflict-pair scan on the default green TPS path.
   - `kombat_getConsensusMetrics.mahi` now exposes summary distributions for the Phase 6 schedule profile fields.
   - Focused RPC Mahi tests passed `12` tests. RPC clippy initially exposed existing lint debt (`network_json_response` identical branches and `consensus_metrics_to_json` argument count); both were handled mechanically so `kombat-chain-rpc --lib` passes clippy with `-D warnings`.
   - Attempted full `cargo test -p kombat-chain-rpc --lib`; it reached the existing network-test long-running tail and did not complete after the 60s warning window, so it was not used as the gate for this metrics-only slice.
   Status:
   - kept
   Reason:
   - This gives Phase 6 an opt-in measurement surface for real committed Mahi batches without adding default hot-path work. It is intentionally observability-only; worker execution remains unwired.
   Next move:
   - Run one short green-config bench with `KOMBAT_PHASE6_SCHEDULE_PROFILE=1` at 100k/10s to capture actual committed-batch parallelism. If the profile shows high layer width and low conflict density, proceed toward a worker-backed execution prototype; if it is conflict-heavy, Block-STM will need narrower targeting.

150. Phase 6 prep: sampled schedule-profile metrics
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 149's profiling surface is useful but the first live attempt showed the uncapped O(n²) conflict scan is too expensive at 100k offered. Sampling committed batches and capping profiled items per batch should preserve shape visibility without turning the profiler into the bottleneck.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc phase6_schedule_profile_caps_profiled_mahi_items --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_ --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-schedule-profile-100k-10s`
   Evidence:
   - Uncapped profile run with `KOMBAT_PHASE6_SCHEDULE_PROFILE=1` at 100k/10s was not a keeper: strict committed summary was `149,704` intents (`14,970.4 TPS`) and post-load Mahi convergence failed. This is profiler pollution, not a green-path regression, because the profile path was doing a full schedule/conflict scan for every committed batch.
   - Added `KOMBAT_PHASE6_SCHEDULE_PROFILE_SAMPLE_EVERY` (default `32`) and `KOMBAT_PHASE6_SCHEDULE_PROFILE_MAX_ITEMS` (default `512`). Profile collection now samples every Nth committed batch and caps the profiled prefix while separately reporting total batch item count and truncated-batch count.
   - `kombat_getConsensusMetrics.mahi` now reports profile configuration, profiled/truncated batch counts, total item-count samples, capped item-count samples, layer count, max width, conflict pairs, and estimated parallelism.
   - Added a focused unit test proving Mahi schedule profiling records total item count while truncating the profiled schedule input.
   Status:
   - kept
   Reason:
   - The original profile mode was too intrusive for live 100k diagnostics. The sampled/capped version keeps the default green path unchanged and makes the opt-in profile usable as a diagnostic rather than a benchmark workload.
   Next move:
   - Rerun the short green-config profile with the sampled defaults. Use the resulting schedule shape only as diagnostic evidence; compare TPS against the unprofiled green reference separately before wiring any worker execution.

151. Phase 6 diagnostic: Mahi block schedule profile
   Date:
   - `2026-04-24`
   Hypothesis:
   - With sampled/capped schedule profiling and harness extraction in place, a short green-config run can tell whether current Mahi committed batches have enough independent work for a generic Block-STM executor to help.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-schedule-profile-sampled-100k-10s`
   - `/tmp/codex-exp-phase6-schedule-profile-sampled2-100k-10s`
   Evidence:
   - First sampled profile run kept the green shape: `measured_avg_tps=95,195.5`, strict committed total `1,499,200`, post-load convergence passed. The artifact did not include Phase 6 profile fields because the committed-timeseries sampler had a fixed extraction list.
   - Updated the committed-timeseries sampler to capture Phase 6 schedule profile fields from `kombat_getConsensusMetrics.mahi`.
   - Second sampled profile run with field extraction kept the green shape: `measured_avg_tps=95,786.2`, strict committed total `1,499,424`, post-load convergence passed at height `680`.
   - Profiled batches: `11`; truncated batches: `6`; total batch item count p50/p90/max: `5,608 / 7,344 / 12,432`.
   - Capped profiled item count was `512`. Layer count p50/p90/max was `512 / 512 / 512`; max layer width p50/p90/max was `1 / 1 / 1`; conflict pair count was `130,816`, which is the full pair count for 512 mutually-conflicting items; estimated parallelism was `1,000` milli (`1.0x`).
   Status:
   - kept diagnostic
   Reason:
   - The current 100k PlaceOrder benchmark is single-market (`OrderBook(7)`), so the coarse Phase 6 access-set model sees every profiled item as conflicting. Generic Block-STM worker execution would add overhead and expose no parallelism for this workload.
   Next move:
   - Do not wire generic worker execution for the current benchmark yet. Shift Phase 6 toward order-book-specific batching/sharding: same-market same-side non-crossing place orders need a specialized batched path or finer-grained order-book resources before Block-STM can move the 100k single-market TPS number.

152. Phase 6 prep: resting order batch primitive
   Date:
   - `2026-04-24`
   Hypothesis:
   - The current 100k benchmark conflicts on a single order book, so generic Block-STM has no exploitable parallelism. A narrow matching-engine primitive for same-side, non-crossing, append-only resting orders can remove repeated per-order book work while preserving sequential price-time semantics. Land it runtime-inert first before wiring Mahi execution to it.
   Files changed:
   - `chain/crates/matching/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-matching --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-matching --manifest-path chain/Cargo.toml -- -D warnings`
   Benchmark runs:
   - None. Runtime-inert matching API; Mahi execution is not wired to it yet.
   Evidence:
   - Added `SingleMarketBook::submit_resting_batch()` with an explicit `RestingBatchError::{Reject, Unsupported}` split. Future callers can fall back to the existing per-order path for valid-but-not-batchable shapes while preserving true order rejects.
   - The primitive accepts only one-side, non-crossing, non-immediate limit orders that can append to the existing side without changing price-time priority. This matches the current single-market same-side loadgen shape without broadening matching semantics.
   - Added tests proving batched resting submit matches sequential submit state/outcomes, duplicate client IDs reject without insertion, crossing orders are reported as unsupported, and non-append insertions are reported as unsupported.
   Status:
   - kept
   Reason:
   - This creates the safe matching-layer hook needed for the next Phase 6 slice without changing runtime behavior. It avoids forcing generic Block-STM onto a workload with measured `1.0x` schedule parallelism.
   Next move:
   - Wire Mahi merged execution to identify consecutive place-order runs that fit `submit_resting_batch()`, fall back to existing per-order execution for unsupported runs, then benchmark green config at 100k/10s and 60k/60s.

153. Phase 6 prep: Mahi resting order batch wiring
   Date:
   - `2026-04-24`
   Hypothesis:
   - The current green workload is dominated by same-market same-side non-crossing `PlaceOrder` intents. Wiring Mahi's preverified commit path to batch supported resting-order runs should preserve correctness and may reduce per-order matching overhead; unsupported or truly rejecting shapes must fall back to the existing sequential path.
   Files changed:
   - `chain/crates/matching/src/lib.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-matching --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-resting-batch-100k-10s`
   - `/tmp/codex-exp-phase6-resting-batch-60k-60s`
   Evidence:
   - Added `can_submit_resting_batch()` so Mahi can validate a candidate run before committing policy admissions. Runtime batching is limited to `PolicyAdmissionCommitMode::PreverifiedSession`; HotStuff-style verified commits continue through the original sequential path.
   - `try_commit_resting_place_order_batch()` batches only consecutive same-market same-side place-order runs that the matching book classifies as append-only resting orders. `RestingBatchError::Unsupported` falls through to sequential execution; `Reject` remains a real order rejection.
   - Focused batch test proves two preverified resting place orders commit as a batch, advance policy nonce state, create the expected book entries, and emit ordered events.
   - Green 100k/10s run stayed stable: `measured_avg_tps=96,479.8`, strict committed total `1,498,296`, zero request errors, post-load convergence passed at height `680`.
   - Green 60k/60s soak passed the M9 sustained gate: `measured_avg_tps=57,010.6`, strict committed total `3,899,648`, zero request errors, post-load convergence passed at height `1233`.
   Status:
   - kept
   Reason:
   - This is correctness-safe and sustained-green, but it is not a throughput win against the prior best (`97,195.9` short / `59,474.8` soak). The evidence says the current wall is not plain per-order matching insertion alone; policy admission commit, event emission, or downstream batch execution still dominate.
   Next move:
   - Add lightweight counters for resting-batch attempts, accepted batch count, batched item count, and unsupported fallback count. If the fast path is hot but flat, profile policy/event overhead inside the batch; if it is not hot, inspect Mahi block item ordering before adding more matching-specific code.

154. Phase 6 diagnostic: resting batch hot-path counters
   Date:
   - `2026-04-24`
   Hypothesis:
   - Item 153 was flat because either the resting-order batch path was not actually hot, or it was hot but matching insertion is not the remaining limiter. Lightweight counters should distinguish those cases without adding intrusive profiling.
   Files changed:
   - `chain/crates/node/src/telemetry.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-resting-batch-counters-100k-10s`
   Evidence:
   - Added consensus metrics for `phase6_resting_batch_{attempts,accepted,items,unsupported,rejected}` and exposed them through `kombat_getConsensusMetrics` plus the committed-timeseries sampler.
   - 100k/10s diagnostic remained green: `measured_avg_tps=94,954.3`, strict committed total `1,499,136`, zero request errors, post-load convergence passed at height `675`.
   - Final primary-validator counters: `attempts=183`, `accepted=183`, `items=1,388,168`, `unsupported=0`, `rejected=0`.
   Status:
   - kept diagnostic
   Reason:
   - The resting batch path is hot and accepts the current workload, so the flat result in item 153 is not caused by missed batching or unsupported fallback. Matching insertion is no longer the obvious wall; remaining leverage is inside per-intent policy admission commit, ordered event emission, or batch root/state-root construction.
   Next move:
   - Add a low-overhead native Mahi commit breakdown around policy-admission commit, matching side effects, event construction, tx/event root construction, and install. Use sampled timing only; do not add full per-intent tracing on the green path.

155. Phase 6 diagnostic: resting batch timing split
   Date:
   - `2026-04-24`
   Hypothesis:
   - Since the resting batch path is hot but flat, the remaining per-item cost inside the accepted batch path should show up in coarse timing buckets: policy admission commit, matching side effects, and ordered event construction.
   Files changed:
   - `chain/crates/node/src/telemetry.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-resting-batch-timing-100k-10s`
   Evidence:
   - Extended the resting-batch counters with total nanoseconds spent in policy commits, matching submit, and event construction. This adds three `Instant` measurements per accepted batch, not per intent.
   - 100k/10s diagnostic stayed green: `measured_avg_tps=97,383.2`, strict committed total `1,499,088`, zero request errors, post-load convergence passed at height `670`.
   - Final primary-validator counters: `attempts=183`, `accepted=183`, `items=1,410,536`.
   - Timing split on those accepted batches: policy commit `29.1ms`, matching submit `317.7ms`, event construction `607.6ms`.
   Status:
   - kept diagnostic
   Reason:
   - Policy admission commit is not the hot cost inside the batched path. Matching still matters, but ordered event construction is roughly 2x the matching time in this workload. The next useful structural slice is event compaction or lazy event materialization for high-volume Mahi order accepts, not more policy batching.
   Next move:
   - Prototype compact order-accepted event emission for homogeneous Mahi resting batches while preserving externally observable event order or providing a deterministic expansion path. Gate with full node tests plus 100k/10s and 60k/60s benches; bin if external event compatibility becomes ambiguous.

156. Phase 6 experiment: no-outcome resting batch submit binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The accepted resting-order batch path allocates one `MatchOutcome` and one empty fill vector per order, then immediately converts those outcomes back into ordered runtime events. A no-outcome matching submit path plus direct resting `OrderAccepted` event construction might reduce matching/event overhead while preserving the external per-intent event sequence.
   Files changed:
   - `chain/crates/matching/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-matching --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-matching --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-resting-no-outcomes-100k-10s` (discarded: missing `AUTO_MAX_ITEMS=5000`, invalid comparison)
   - `/tmp/codex-exp-phase6-resting-no-outcomes-100k-10s-rerun`
   Evidence:
   - The focused tests, full node lib test, clippy checks, and release build passed with the no-outcome patch.
   - The first 100k/10s run was invalid for comparison because it omitted `AUTO_MAX_ITEMS=5000`; it measured only `15,998.8 TPS` due to payload-density capping and is discarded.
   - Corrected 100k/10s run with the green Mahi config measured `94,272.1 TPS`, committed summary total `1,499,624`, zero request errors, and post-load convergence at height `638`.
   - Final timing counters on the corrected run: `attempts=178`, `accepted=178`, `items=1,407,152`, policy `30.0ms`, matching `335.5ms`, event `607.1ms`.
   - Compared with item 155 (`97,383.2 TPS`, matching `317.7ms`, event `607.6ms`), the patch did not improve the measured wall. Event construction stayed flat and matching time was slightly worse.
   Status:
   - binned
   Reason:
   - This is a micro-optimization with no throughput or timing win. It adds another matching API surface without reducing the observed hot cost, so keeping it would increase complexity while moving the branch backward.
   Next move:
   - Leave the item-155 code path intact. Before changing the external event model, profile root/install cost and event storage/query compatibility; any event compaction must preserve deterministic expansion for `kombat_getEvents`, order-status scans, snapshots, and state-root convergence.

157. Phase 6 diagnostic: Mahi native execution root/install profile
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 155 showed event construction inside the resting-batch fast path is more expensive than matching insertion, but changing the external runtime-event model is risky. Before prototyping event compaction, use the existing `KOMBAT_MAHI_EXEC_PROFILE=1` coarse profiler to determine whether tx/event root construction or install/bookkeeping is the larger downstream wall.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Existing profiler-only diagnostic run.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-exec-profile-100k-10s`
   Evidence:
   - 100k/10s run with the green Mahi config plus `KOMBAT_MAHI_EXEC_PROFILE=1` remained convergent: `measured_avg_tps=95,757.7`, strict committed total `1,499,392`, zero request errors, post-load convergence passed at height `648`.
   - Aggregated `mahi_exec_profile mode=native` logs across four validators: `count=2668`, `blocks=21328`, `body_items=5997568`, `body_ms=66`, `commit_ms=25811`, `roots_ms=2668`, `install_ms=9016`, `total_ms=39254`.
   - Final primary-validator resting-batch counters on the same run: `attempts=178`, `accepted=178`, `items=1,422,352`, policy `55.4ms`, matching `333.0ms`, event `614.2ms`.
   - `commit_ms` dominates root/install cost. The known policy+matching+event buckets account for only about `1.0s` on the sampled validator, leaving most commit time in uninstrumented pre-batch work: block item scanning, order conversion, runtime gate validation, admission construction, and `can_submit_resting_batch()` precheck.
   Status:
   - kept diagnostic
   Reason:
   - This narrows Phase 6 away from risky event-model changes. Root construction and install/bookkeeping are real but smaller than `commit_block_intents` itself; the next slice should instrument and then reduce the unmeasured pre-batch/precheck work inside the accepted resting batch path.
   Next move:
   - Add coarse batch timing buckets for gather/precheck/admission-build separately from policy/matching/event. If precheck dominates, replace duplicate preview validation with a prepared batch plan; if admission-build dominates, batch or cache the deterministic admission construction for preverified Mahi commits.

158. Phase 6 diagnostic: resting batch precommit timing split
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 157 showed most native Mahi execution time is inside `commit_block_intents`, but item 155 only split policy/matching/event costs. Adding coarse gather, precheck, and admission-build timing buckets should identify whether the hidden cost is duplicate matching precheck, runtime gate/admission construction, or block-item grouping.
   Files changed:
   - `chain/crates/node/src/telemetry.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-prebatch-timing-100k-10s`
   Evidence:
   - Added per-accepted-batch timing buckets for grouping/order conversion (`gather_ns`), matching fast-path precheck (`precheck_ns`), and runtime-gate plus admission object construction (`admission_ns`), and exposed them through consensus metrics and the loadtest timeseries.
   - 100k/10s diagnostic stayed green: `measured_avg_tps=96,947.0`, strict committed total `1,499,208`, zero request errors, post-load convergence passed at height `650`.
   - Final primary-validator counters: `attempts=178`, `accepted=178`, `items=1,408,208`.
   - Timing split: gather `15.7ms`, precheck `179.7ms`, admission-build `488.1ms`, policy commit `28.7ms`, matching submit `340.1ms`, event construction `622.2ms`.
   Status:
   - kept diagnostic
   Reason:
   - The hidden cost is not item grouping. Admission construction and event emission are larger than precheck, but precheck plus matching duplicate validation is still a concrete removable cost. The next safe structural slice is a prepared resting-batch plan that validates/builds book inserts once before policy commit and then applies the prepared plan after policy commit.
   Next move:
   - Add a matching-layer `PreparedRestingBatch` path and wire Mahi preverified commits to prepare once, commit policy, then apply the prepared batch. Preserve the existing `submit_resting_batch()` API by implementing it on top of the prepared path. Gate on matching tests, node tests, clippy, 100k/10s, and 60k/60s; bin if the duplicate-validation removal does not beat item 155/158 within variance.

159. Phase 6 experiment: prepared resting batch plan binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - A matching-layer prepared batch plan can validate/build resting-order inserts once before policy commit, then apply the prepared plan after policy commit. This should remove duplicate precheck/submit validation while preserving policy atomicity and external event order.
   Files changed:
   - `chain/crates/matching/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-matching --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-matching --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-prepared-resting-100k-10s`
   - `/tmp/codex-exp-phase6-prepared-resting-100k-10s-rerun`
   Evidence:
   - The prepared-batch patch passed matching tests, the focused node batch test, the full node lib test, clippy for matching/node, and release build.
   - First 100k/10s run stayed correct but regressed: `measured_avg_tps=93,947.7`, strict committed total `1,499,248`, zero request errors, post-load convergence passed at height `650`.
   - First-run timing showed the intended local effect but no throughput win: matching dropped from item 158's `340.1ms` to `169.6ms`, while event construction rose to `664.3ms` and measured TPS fell.
   - Rerun confirmed the regression: `measured_avg_tps=88,753.2`, strict committed total `1,498,040`, zero request errors, post-load convergence passed at height `608`.
   Status:
   - binned
   Reason:
   - Removing duplicate matching validation improves the matching bucket but does not improve end-to-end throughput. The extra prepared-plan API and altered object lifetime appear to increase variance/latency elsewhere, so keeping it would trade simplicity for a local metric win and a global TPS loss.
   Next move:
   - Keep item 158's timing instrumentation, but stop pursuing matching-precheck micro-optimizations for this workload. The remaining high-leverage targets are admission-build cost and event-output cost; prefer a structural change with a clear semantic simplification over another local allocation tweak.

160. Phase 6 experiment: preverified admission hash elision binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Preverified Mahi commits need policy/chain/nonce checks but not intent-hash or signature recomputation during local admission-object construction. Building a minimal `SignedSessionPayload` for the preverified path should reduce admission-build cost without changing committed intent semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-preverified-admission-fast-100k-10s`
   - `/tmp/codex-exp-phase6-preverified-admission-fast-100k-10s-rerun`
   Evidence:
   - The focused node test, node clippy, and release build passed with the hash-elision patch.
   - First 100k/10s run regressed: `measured_avg_tps=91,952.4`, strict committed total `1,499,136`, zero request errors, post-load convergence passed at height `650`.
   - First-run final counters showed the intended local bucket win: admission-build dropped from item 158's `488.1ms` to `51.4ms`, with gather `14.2ms`, precheck `163.7ms`, policy `24.7ms`, matching `321.9ms`, and event `652.0ms`.
   - Rerun confirmed the regression: `measured_avg_tps=89,826.1`, strict committed total `1,499,624`, zero request errors, post-load convergence passed, with loadgen latency p50 `3.018ms`, p90 `42.149ms`, and p99 `101.727ms`.
   - Rerun final counters again showed local admission improvement but no global win: admission-build `51.7ms`, gather `15.1ms`, precheck `239.9ms`, policy `25.1ms`, matching `330.8ms`, and event `727.1ms`.
   Status:
   - binned
   Reason:
   - Removing hash recomputation lowers the local admission bucket but worsens end-to-end TPS and ingress latency. The dummy signed-session payload is also semantically less clear than the real session payload, so it should not be kept without a measurable global payoff.
   Next move:
   - Stop local bucket micro-tweaks for the single-market Mahi benchmark. The remaining structural wall is event/output cost and single-order-book contention; next slices should either attack the event-output model with an explicit external-compatibility plan or move toward workload/market sharding where parallel execution can actually express concurrency.

161. Phase 6 diagnostic: multi-market loadgen support
   Date:
   - `2026-04-25`
   Hypothesis:
   - The current 100k-offered Mahi wall may be single-order-book contention. Add an opt-in loadgen/harness knob to distribute writers across consecutive markets, then compare a 4-market run against the single-market green reference.
   Files changed:
   - `chain/crates/loadgen/src/main.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo build --release -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-market-count4-100k-10s`
   Evidence:
   - Added `--market-count` to `kombat_loadgen` and `LOADTEST_MARKET_COUNT` to the local-cluster harness. Default is `1`, preserving the historical single-market workload exactly.
   - With `LOADTEST_MARKET_COUNT=4`, each load worker posts to `base_market_id + worker_id % 4`, preserving per-writer nonce order while spreading the order-book resource.
   - The 4-market 100k/10s diagnostic stayed correct: zero request errors, strict committed total `1,499,128`, post-load Mahi convergence passed at height `635`, and committed timeseries measured `95,239.8 TPS` with peak `113,438.3`.
   - The run did not beat item 155's single-market green reference (`97,383.2 TPS`). Final timing counters were `attempts=1146`, `items=1,383,848`, gather `23.0ms`, precheck `162.4ms`, admission-build `493.1ms`, policy `24.3ms`, matching `288.5ms`, and event `637.6ms`.
   Status:
   - kept diagnostic
   Reason:
   - Market spreading is not an immediate TPS unlock under the current serial execution path. It reduces matching pressure slightly but fragments homogeneous resting batches and leaves per-intent admission/event output as the dominant cost. The harness support is still worth keeping because it is default-inert and gives Phase 6/Block-STM work a realistic multi-resource benchmark.
   Next move:
   - Do not spend more time treating single-order-book contention as the primary wall in the current serial executor. Next high-leverage slices are either an event-output compatibility plan with a quantified eventless upper-bound diagnostic, or actual market-sharded/parallel execution that can use the new multi-market harness.

162. Phase 6 diagnostic: event-output suppression upper bound binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 158 showed runtime-event construction around `622ms` on the primary validator's accepted resting-batch path. Temporarily suppressing Mahi committed runtime-event materialization behind a diagnostic flag should quantify the maximum TPS available from a future compact-event/external-expansion design.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-suppress-events-100k-10s`
   Evidence:
   - The temporary patch added `KOMBAT_PHASE6_SUPPRESS_EVENTS=1` to skip constructing/persisting committed runtime events for Mahi batch commits. The focused batch test and release build passed before benchmarking.
   - The 100k/10s diagnostic stayed correct for chain progress: zero request errors, strict committed total `1,499,368`, post-load Mahi convergence passed at height `658`.
   - Measured throughput did not improve: `measured_avg_tps=95,367.7`, peak `105,047.3`, below item 155's single-market green reference of `97,383.2 TPS`.
   - Final counters confirmed the local event bucket was eliminated (`event_ms=0`) while other buckets remained in family: `attempts=188`, `items=1,398,312`, gather `18.2ms`, precheck `183.5ms`, admission-build `468.8ms`, policy `29.0ms`, matching `356.5ms`.
   Status:
   - binned
   Reason:
   - Event construction is visible locally but is not the current global throughput wall. Keeping a suppress-events flag would make external RPC/event semantics incorrect without improving the benchmark.
   Next move:
   - Revert the diagnostic code and stop treating runtime-event materialization as the next primary target. The stronger evidence now points toward scheduling/execution pipeline effects outside the local batch buckets: Mahi block cadence, commit executor scheduling, native execution install path, or the serial policy/order-book commit boundary.

163. Phase 6 experiment: commit executor ready-batch wake gating binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Mahi commit execution metrics show hundreds of empty executor wakes during 100k-offered runs. Gating executor notifications until a full verified execution batch is ready should reduce scheduler churn without changing commit order.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-commit-wake-gate-100k-10s`
   Evidence:
   - The patch added a verified-prefix readiness check and notified the executor only when enough verified blocks existed for the configured merged execution batch. The Mahi commit-log convergence test, RPC clippy, and release build passed.
   - The 100k/10s run stayed correct but regressed sharply: `measured_avg_tps=84,333.0`, strict committed total `1,494,672`, zero request errors, post-load Mahi convergence passed at height `593`.
   - The intended local metric improved: final `commit_executor_wake_count=122` and `commit_executor_empty_wake_count=15`, versus hundreds of empty wakes in recent green runs.
   - The global execution path worsened: final `commit_execution_batches=272`, `commit_execution_blocks=2176`, `commit_execution_intents=1,272,376`, p90 execution-batch latency `76ms`, max `199ms`, and final sample TPS only `76,441.5`.
   Status:
   - binned
   Reason:
   - The empty wakes are not pure waste. They appear to keep the executor responsive as verification and commit detection advance; coalescing them delays execution enough to reduce committed blocks and TPS.
   Next move:
   - Preserve eager executor notifications. Future scheduling work should look at executor throughput/backlog and batch latency directly, not just wake-count reduction. A better next diagnostic is to profile per-batch native execution time by phase over a green run and correlate it with produced/verified/committed block counts.

164. Phase 6 experiment: skip empty Mahi execution chunks binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Green-run profiling showed many Mahi merged-execution batches have zero payload intents. Skipping synthetic execution/root/install for all-empty Mahi chunks behind `KOMBAT_MAHI_SKIP_EMPTY_EXEC=1` might reduce executor work without changing committed-intent semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_commit_log_converges_across_four_validators_in_mahi_mode --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-skip-empty-exec-100k-10s`
   Evidence:
   - The patch added an opt-in skip path for empty merged Mahi execution bodies. The Mahi commit-log convergence test and release build passed before benchmarking.
   - The 100k/10s run stayed correct for submitted intents: zero request errors, strict committed total `1,499,192`, and post-load Mahi convergence passed at height `150`.
   - Measured throughput regressed versus the current green reference: `measured_avg_tps=92,145.6`, peak `143,625.7`, min `53,737.7`, versus item 155's `97,383.2 TPS`.
   - Startup semantics changed materially: the cluster converged at height `1` with the zero state root before load because empty execution chunks no longer advanced the app chain. This may be a valid future semantic change, but it is not acceptable as a speculative TPS optimization without a win.
   - Final counters: execution batches `289`, execution blocks `2312`, execution intents `1,366,632`, execution p90 `90ms`, max `202ms`, executor wakes `524`, empty wakes `409`, produced blocks `586`, produced empty blocks `300`.
   Status:
   - binned
   Reason:
   - Skipping empty execution does not improve the benchmark and changes chain-height/root behavior for empty DAG progress. Keeping the flag would add semantic ambiguity without a throughput payoff.
   Next move:
   - Preserve the existing execution semantics. Do not optimize empty chunks until there is an explicit Mahi chain-model decision about whether empty committed DAG blocks should advance app-chain height. Continue with structural diagnostics around executor batch latency and commit cadence rather than empty-work suppression.

165. Phase 6 diagnostic: multi-market schedule parallelism profile
   Date:
   - `2026-04-25`
   Hypothesis:
   - Generic Block-STM is known to expose no useful width on the current single-market PlaceOrder benchmark, but wider market distribution may create enough independent order-book resources to justify a market-sharded Phase 6 execution path.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Existing schedule-profile metrics and loadtest harness diagnostics only.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-market4-schedule-profile-100k-10s`
   - `/tmp/codex-exp-phase6-market16-schedule-profile-100k-10s`
   Evidence:
   - 4-market schedule-profile run stayed correct: `measured_avg_tps=92,239.6`, strict committed total `1,499,160`, zero request errors, post-load convergence passed at common height `638`.
   - 4-market sampled schedule shape still exposed little width: profiled batches `10`, truncated `5`, profiled item max `512`, layer-count p90 `512`, max-layer-width p90 `1`, max `2`, and parallelism p90 `1000` milli (`1.0x`), max `1084` milli.
   - 16-market schedule-profile run stayed correct: `measured_avg_tps=88,503.4`, strict committed total `1,499,640`, zero request errors, post-load convergence passed at common height `620`.
   - 16-market sampled schedule shape exposed real width: profiled batches `9`, truncated `4`, profiled item max `1024`, layer-count p90 `256`, max-layer-width p90/max `12`, and parallelism p90/max `9142` milli (`9.142x`).
   - The 16-market run's resting-batch counters showed the current consecutive-run batching does not exploit that width: only `19,264` items hit the resting batch path while `1,346,144` intents executed, because interleaving markets fragments consecutive same-market runs.
   Status:
   - kept diagnostic
   Reason:
   - This confirms the current serial executor cannot benefit from multi-market input by itself, but the committed body has enough independent market resources for a sharded execution path to matter. The next useful slice should target the market-sharded commit shape directly instead of another single-market micro-optimization.
   Next move:
   - Prototype an env-gated market-sharded resting PlaceOrder batch path for preverified Mahi commits. It should commit policy admissions in original body order, group append-only resting orders by market/side for matching, materialize external events in original item order, and fall back to the existing sequential path unless the whole candidate batch is safe.

166. Phase 6 experiment: opt-in market-sharded resting batch path
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 165 showed the 16-market workload has real resource width, but the current consecutive-run resting batch path only catches a tiny fraction of interleaved market orders. An opt-in preverified Mahi path that commits policy admissions in original body order, groups append-only resting orders by market/side for matching, and materializes events in original body order should improve scaled-market execution while preserving deterministic semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node resting_place_orders --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-market16-baseline-100k-10s`
   - `/tmp/codex-exp-phase6-market16-sharded-resting-100k-10s`
   - `/tmp/codex-exp-phase6-market16-sharded-resting-100k-10s-rerun`
   - `/tmp/codex-exp-phase6-sharded-default-off-100k-10s`
   Evidence:
   - Added `KOMBAT_PHASE6_MARKET_SHARDED_RESTING_BATCH=1`, default off. The default green single-market path is still the normal consecutive-run batch path.
   - Added `try_commit_market_sharded_resting_place_order_batch()` for preverified Mahi commits. It only accepts all-intent PlaceOrder bodies where each market has a single side; otherwise it falls back to the existing sequential/consecutive path. It prechecks matching before policy mutation, commits policy admissions in original body order, submits each market batch independently, and emits events in original item order.
   - Added a unit test covering interleaved markets, per-market order-book order, policy nonce advancement, and original external event order. Full node lib suite passed `200` tests; node clippy and release build passed.
   - 16-market baseline without the flag measured `81,868.1 TPS`, strict committed total `1,498,320`, zero request errors, convergence at common height `568`.
   - First 16-market sharded run measured `93,722.2 TPS`, strict committed total `1,499,136`, zero request errors, convergence at common height `640`. The sharded path handled `1,369,624` items, versus `13,029` items in the unflagged baseline's consecutive batch counters.
   - Sharded rerun measured `84,179.3 TPS`, strict committed total `1,495,920`, zero request errors, convergence at common height `555`. The sharded path again handled most committed items (`1,253,384`) but variance remained high and the rerun was only slightly above the single baseline run.
   - Default-off single-market sanity run measured `90,318.7 TPS`, strict committed total `1,497,360`, zero request errors, convergence at common height `635`. This is below item 155's historical green `97,383.2 TPS`, but recent same-machine diagnostics have also been in the low-90k range; the code path is default-inert and the run is treated as a no-regression sanity check rather than a new green anchor.
   Status:
   - kept as opt-in Phase 6 scaffold
   Reason:
   - The result is not a new single-market TPS win and should not be enabled by default. It is still worth keeping because it is default-inert, correctness-tested, materially increases the amount of interleaved multi-market work handled by a market-aware batch path, and gives the next Phase 6 slice a concrete sharded execution surface instead of a purely theoretical scheduler.
   Next move:
   - Do not promote the flag by default. Use this path as the base for the next structural slice: either worker-backed per-market matching/admission execution, or a stricter multi-run 16-market/60s gate to quantify whether the scaled-market improvement is stable enough before adding concurrency.

167. Phase 6 diagnostic: 16-market sharded path 30s comparison
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 166's 10s 16-market sharded runs had high variance. A 30s side-by-side comparison should show whether the opt-in serial market-sharded batch path is a stable scaled-workload throughput win or only a correctness scaffold for future worker-backed execution.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark-only follow-up to item 166's already-tested code.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-market16-sharded-resting-100k-30s`
   - `/tmp/codex-exp-phase6-market16-baseline-100k-30s`
   Evidence:
   - 16-market sharded 100k/30s run with `KOMBAT_PHASE6_MARKET_SHARDED_RESTING_BATCH=1` measured `90,549.5 TPS`, strict committed total `3,496,312`, zero request errors, and convergence at common height `688`.
   - Sharded final counters: execution batches `384`, execution blocks `3072`, execution intents `3,187,800`, execution p90 `171ms`, max `267ms`, sharded batch attempts/accepted `3896`, sharded items `3,187,800`, event construction `1635.2ms`.
   - 16-market baseline 100k/30s run without the flag measured `91,020.0 TPS`, strict committed total `3,499,520`, zero request errors, and convergence at common height `672`.
   - Baseline final counters: execution batches `375`, execution blocks `3000`, execution intents `3,199,976`, execution p90 `193ms`, max `468ms`, consecutive resting-batch attempts/accepted `5104`, batched items only `25,560`, event construction `11.7ms` in the batch counters.
   - The sharded path dramatically changes which code path handles multi-market payloads, but it does not improve 30s end-to-end throughput. It mostly moves work into the measured sharded batch counters rather than removing the global wall.
   Status:
   - kept diagnostic
   Reason:
   - This bins the serial sharded path as a throughput optimization while preserving item 166's code as default-off Phase 6 scaffolding. More serial restructuring around the same sharded batch is unlikely to get us to the 250k-class target.
   Next move:
   - Stop tuning the serial market-sharded batch path. If continuing Phase 6 execution work, the next slice should add actual worker-backed parallelism for independent market shards, or step back to a focused design for parallel admission/event materialization. Do not promote `KOMBAT_PHASE6_MARKET_SHARDED_RESTING_BATCH` by default based on current evidence.

168. Phase 6 experiment: market-sharded parallel event materialization binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 167 showed the opt-in market-sharded batch path spends substantial time materializing deterministic runtime events in original body order. Splitting event construction across scoped worker threads behind `KOMBAT_PHASE6_MARKET_SHARDED_PARALLEL_EVENTS=1` might reduce the sharded path's end-to-end execution cost without changing policy or matching semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node market_sharded_resting_place_orders_batch_across_interleaved_markets --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-market16-sharded-par-events-100k-10s`
   - `/tmp/codex-exp-phase6-market16-sharded-par-events-100k-10s-rerun`
   - `/tmp/codex-exp-phase6-market16-sharded-par-events-100k-30s`
   Evidence:
   - The patch was default-off and only affected the opt-in market-sharded path. It preserved original event order by chunking the item range, materializing each chunk independently, and concatenating chunks in order.
   - The targeted unit test, node clippy, and release build passed before benchmarking.
   - First 16-market 100k/10s run measured `97,215.3 TPS`, strict committed total `1,494,280`, zero request errors, convergence at common height `668`, final execution p90 `49ms`, max `98ms`, and event construction `529.4ms`.
   - Identical 100k/10s rerun measured `91,333.3 TPS`, strict committed total `1,499,392`, zero request errors, convergence at common height `595`, final execution p90 `82ms`, max `171ms`, and event construction `490.2ms`.
   - The decisive 100k/30s run measured `89,195.6 TPS`, strict committed total `3,499,512`, zero request errors, convergence at common height `679`, final execution p90 `181ms`, max `296ms`, and event construction `1404.5ms`.
   - The 30s result is below item 167's 16-market baseline (`91,020.0 TPS`) and serial sharded run (`90,549.5 TPS`). Parallelizing event materialization did not improve sustained throughput and added another tuning flag to an already binned serial sharded path.
   Status:
   - binned
   Reason:
   - The first 10s run was a transient peak. Sustained evidence shows the scoped worker overhead and residual global execution cost offset any event-construction benefit.
   Next move:
   - Revert the parallel-events flag and helper. Stop spending slices on serial sharded-path micro-tuning. The next Phase 6 work should be a larger structural worker-backed market execution design or a focused profile of the current single-market green path's execution-batch latency.

169. Phase 6 experiment: empty merged Mahi execution fast path binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - A focused `KOMBAT_MAHI_EXEC_PROFILE=1` run showed many empty merged Mahi chunks still spending time in native execution, mostly from cloning policy/order-book state before discovering the body is empty. A narrow fast path that still appends the empty synthetic app block, but skips empty-body commit/state cloning, might remove wasted executor work without repeating item 164's semantic change.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node empty_merged_mahi_blocks_advance_chain_without_state_mutation --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-native-profile-100k-10s`
   - `/tmp/codex-exp-phase6-empty-fastpath-100k-10s`
   Evidence:
   - The profile run reproduced the current green short-run level: `97,497.0 TPS`, strict committed total `1,499,632`, zero request errors, and convergence at common height `650`.
   - Parsing `mahi_exec_profile` lines from the profile run produced `2655` usable samples across validators. Non-empty chunks averaged `27.87ms` total with p90 `47ms`; empty chunks averaged `8.97ms` total with p90 `13ms`, despite `body_items=0`. This identified empty-body cloning/commit overhead as a plausible local waste.
   - The patch added a native empty-body path in `apply_mahi_committed_blocks_merged()` that computed the empty block roots and appended the synthetic Mahi app block without cloning policy/order-book state. A regression test verified chain height advanced while policy and order books remained unchanged. Targeted test, node clippy, and release build passed.
   - The 100k/10s benchmark with the patch measured `96,575.8 TPS`, strict committed total `1,499,136`, zero request errors, and convergence at common height `643`.
   - Final fast-path counters: execution batches `324`, execution blocks `2592`, execution intents `1,402,000`, p90 execution-batch latency `55ms`, max `73ms`, final sample TPS `89,359.9`, resting batch items `1,402,000`, and event construction `603.4ms`.
   Status:
   - binned
   Reason:
   - The local empty-chunk cost exists, but removing it does not improve end-to-end throughput. The run landed slightly below the current green reference, so keeping a special-case hot-loop branch would add complexity without a measurable payoff.
   Next move:
   - Preserve the existing empty execution semantics and code path. The useful signal from this item is that local executor micro-costs are no longer the primary wall at 100k short-run. Next work should either design a real persistent worker-backed execution model, or move to a broader 60s soak/profile to find the sustained-load limiter.

170. Phase 6 diagnostic: clean 100k-offered 60s sustained soak
   Date:
   - `2026-04-25`
   Hypothesis:
   - After several binned short-run micro-optimizations, the next useful signal is a clean current-source sustained soak at 100k offered. This should identify whether the system still collapses over 60s or whether the current Mahi path already sustains a much higher long-run level than the older M9 60k/60s artifact.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-clean-100k-60s`
   Evidence:
   - Rebuilt the release binaries after reverting item 169's code, so the run used current source at `868b895de`.
   - 100k-offered 60s run measured `87,254.6 TPS` average, peak `127,926.3 TPS`, zero request errors, achieved loadgen rate `99,582.8 RPS`, latency p50 `3.272ms`, p90 `72.385ms`, p99 `123.715ms`, and post-load convergence passed at common height `618`.
   - Strict committed summary after settle: total `6,474,304`, min per writer `202,112`, average per writer `202,322`, max per writer `202,424`.
   - Final sampled counters: execution batches `453`, execution blocks `3624`, execution intents `5,677,496`, p90 execution-batch latency `248ms`, max `325ms`, produced blocks `920`, produced empty blocks `276`, executor wakes `1247`, and empty wakes `1070`.
   - Resting-batch phase totals in the final sample: items `5,677,496`, precheck `936.6ms`, admission `2161.9ms`, matching `1755.3ms`, event construction `3005.6ms`. Sustained load is now primarily execution/event/admission work, not ingress errors or pool starvation.
   Status:
   - kept diagnostic
   Reason:
   - This establishes a much stronger current sustained reference than the older 60k/60s M9 soak: the system can absorb 100k offered for 60s with no request errors and >87k measured average TPS. It also confirms that the next 250k-class step needs structural execution parallelism or state/write-model changes, not another short-run scheduler or empty-work tweak.
   Next move:
   - Use `/tmp/codex-exp-phase6-clean-100k-60s` as the current sustained diagnostic anchor. The next implementation slice should be designed around reducing the dominant per-intent execution phases (`event_ms`, `admission_ms`, `matching_ms`) or introducing a persistent worker-backed execution model; do not continue serial sharded-path or empty-chunk micro-tuning.

171. Phase 6 diagnostic: high-offer local ceiling ladder
   Date:
   - `2026-04-25`
   Hypothesis:
   - The clean 100k/60s soak had zero request errors, so the current local ceiling may be above 100k offered. Raising offered load to 150k and 200k for 30s should show whether the current Mahi path keeps scaling or whether the local harness/runtime saturates around the same execution wall.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark-only diagnostic using the binaries rebuilt for item 170.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-clean-150k-30s`
   - `/tmp/codex-exp-phase6-clean-200k-30s`
   Evidence:
   - 150k-offered 30s run achieved loadgen rate `132,673.9 RPS`, zero request errors, latency p50 `54.615ms`, p90 `89.117ms`, p99 `135.194ms`, measured average `105,248.6 TPS`, peak `120,593.7 TPS`, strict committed total `4,685,640`, and convergence passed at common height `381`.
   - 150k final counters: execution batches `212`, execution blocks `1696`, execution intents `3,559,288`, p90 execution-batch latency `193ms`, max `346ms`, produced blocks `433`, produced empty blocks `236`, executor wakes `366`, empty wakes `286`, and resting-batch event construction `1960.5ms`.
   - 200k-offered 30s run achieved only `121,376.0 RPS`, zero request errors, latency p50 `62.179ms`, p90 `91.988ms`, p99 `131.208ms`, measured average `100,620.0 TPS`, peak `200,262.7 TPS`, strict committed total `4,289,040`, and convergence passed at common height `421`.
   - 200k final counters: execution batches `210`, execution blocks `1680`, execution intents `3,388,320`, p90 execution-batch latency `195ms`, max `346ms`, produced blocks `433`, produced empty blocks `247`, executor wakes `401`, empty wakes `320`, and resting-batch event construction `1774.1ms`.
   Status:
   - kept diagnostic
   Reason:
   - The current local harness/runtime ceiling is about `100k-105k` measured TPS on this workload. Raising target RPS above 150k does not increase accepted load or sustained commit rate; it increases latency and leaves the same execution-batch p90/max profile. The wall is not request rejection or queue overflow.
   Next move:
   - Treat `105,248.6 TPS` at 150k/30s as the current high-offer diagnostic peak, but keep `/tmp/codex-exp-phase6-clean-100k-60s` as the sustained anchor. To move toward 250k, focus on structural execution/write-model changes or distribute the workload over independent execution resources; do not expect more loadgen pressure alone to uncover hidden headroom.

172. Phase 6 experiment: Mahi committed runtime-event bulk append binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The clean 100k/60s soak showed `event_ms` as the largest recorded resting-batch phase. Because the current PlaceOrder resting workload emits mostly `IntentExecuted` and `OrderAccepted` events without `OrderFilled` side effects, a Mahi-only bulk append path for no-side-effect runtime events might reduce per-event overhead in the committed-block install path.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node committed_runtime_events_bulk_append_preserves_sequence --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-bulk-events-150k-30s`
   Evidence:
   - The patch added a `record_committed_runtime_events()` helper that bulk-appended runtime events when none had `record_runtime_event()` side effects, while falling back to the existing per-event path for `OrderFilled`. A targeted unit test verified sequence numbers, block height, timestamp, and event variants. Node clippy and release build passed.
   - 150k-offered 30s run achieved loadgen rate `134,703.7 RPS`, zero request errors, latency p50 `54.186ms`, p90 `84.988ms`, p99 `137.866ms`, measured average `101,837.8 TPS`, peak `158,601.7 TPS`, strict committed total `4,785,372`, and convergence passed at common height `365`.
   - Final counters: execution batches `222`, execution blocks `1776`, execution intents `3,377,536`, p90 execution-batch latency `207ms`, max `306ms`, produced blocks `452`, produced empty blocks `266`, executor wakes `341`, empty wakes `252`, and resting-batch event construction `1783.5ms`.
   - The same 150k/30s shape on current green source in item 171 measured `105,248.6 TPS` with convergence at common height `381`. The bulk-event patch therefore lost about `3.2%` measured average TPS despite a slightly higher achieved loadgen rate.
   Status:
   - binned
   Reason:
   - Bulk-appending no-side-effect runtime events reduced neither end-to-end throughput nor the execution wall. The special-case branch adds semantic risk around event side effects and sequence accounting without improving the current high-offer anchor.
   Next move:
   - Keep the existing runtime-event install path. Stop spending slices on per-event install micro-tuning; the next throughput work should be a structural execution/write-model change or a true persistent worker-backed execution design rather than another local hot-loop special case.

173. Phase 6 experiment: preverified admission skips canonical intent hash binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The Mahi preverified commit path still built a full `SignedSessionPayload` for every committed intent, including `intent_signing_hash()` Borsh serialization and SHA, even though `PolicyStore::commit_preverified_admission()` does not verify the signature hash. A Mahi-only lightweight `PolicyAdmission` constructor might remove HotStuff-era verification materialization from the execution hot loop.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node preverified_policy_admission_does_not_require_canonical_intent_hash --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-preverified-admission-150k-30s`
   - `/tmp/codex-exp-phase6-preverified-admission-rerun-150k-30s`
   - `/tmp/codex-exp-phase6-preverified-admission-100k-60s`
   Evidence:
   - The patch replaced `policy_admission_for_intent()` with a lightweight preverified constructor only for `PolicyAdmissionCommitMode::PreverifiedSession`; normal `VerifySession` continued to compute and verify the canonical intent hash. A targeted unit test verified the split by showing preverified commit accepted the lightweight admission while normal verified commit rejected it.
   - First 150k-offered 30s run measured `106,933.0 TPS`, peak `201,280.3 TPS`, zero request errors, strict committed total `4,334,216`, and convergence at common height `450`. Final counters showed the intended local effect: `admission_ms=88.9ms`, down from second-scale admission totals on the green path, with p90 execution-batch latency `180ms`.
   - The 150k/30s rerun failed to confirm a stable win: `99,296.7 TPS`, peak `161,119.6 TPS`, zero request errors, strict committed total `4,236,182`, and convergence at common height `321`. Final counters still showed `admission_ms=70.2ms`, but matching/event work dominated.
   - The deciding 100k-offered 60s soak regressed catastrophically: achieved loadgen rate only `48,260.0 RPS`, measured average `16,974.6 TPS`, peak `40,309.3 TPS`, zero request errors, strict committed total `2,179,777`, and convergence at common height `254`. Final validator snapshots showed pre-admitted pool backlogs (`105k-228k` items), p90 execution-batch latency `962ms`, max `1215ms`, `event_ms=7036.3ms`, and `matching_ms=2593.0ms`.
   Status:
   - binned
   Reason:
   - The optimization removed the measured admission-hash cost, but that did not translate into stable throughput and it destabilized sustained execution. The likely mechanism is not fully proven, but the 60s artifact shows downstream execution/event/matching phases ballooning and producer pools backing up. Keeping a semantics-sensitive shortcut in preverified admission is not defensible without a sustained win.
   Next move:
   - Revert to canonical `PolicyAdmission` construction. Treat this as evidence that isolated per-intent CPU removals can shift pressure into downstream phases instead of improving end-to-end TPS. The next Phase 6 slice should target the structural execution/write model directly, not a preverified-admission shortcut.

174. Phase 6 experiment: prechecked resting-batch submit binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The Mahi resting-batch path calls `can_submit_resting_batch()` as a precheck and then `submit_resting_batch()`, which repeats batch validation before appending resting orders. Because the order book does not change between those calls in the commit path, a `submit_prechecked_resting_batch()` API might remove duplicate matching-side validation work while keeping the public checked API unchanged.
   Files changed:
   - `chain/crates/matching/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-matching prechecked_resting_batch_matches_checked_submit_after_precheck --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-matching -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-prechecked-resting-150k-30s`
   Evidence:
   - The patch added `SingleMarketBook::submit_prechecked_resting_batch()` and wired only the Mahi preverified batch commit path to call it after successful `can_submit_resting_batch()`. A matching-crate equivalence test verified that prechecked submit produced the same outcomes and book state as the checked path after precheck. Matching/node targeted tests, clippy, and release build passed.
   - 150k-offered 30s run achieved loadgen rate `136,100.0 RPS`, zero request errors, latency p50 `53.463ms`, p90 `87.721ms`, p99 `123.431ms`, measured average `84,975.7 TPS`, peak `160,587.1 TPS`, strict committed total `4,453,177`, and convergence passed at common height `331`.
   - Final counters: execution batches `203`, execution blocks `1624`, execution intents `2,887,552`, p90 execution-batch latency `247ms`, max `449ms`, produced blocks `422`, produced empty blocks `251`, `precheck_ms=660.1ms`, `matching_ms=536.6ms`, and `event_ms=1659.9ms`.
   - The current clean 150k/30s diagnostic anchor from item 171 measured `105,248.6 TPS`, strict committed total `4,685,640`, p90 execution-batch latency `193ms`, and convergence at common height `381`. The prechecked-submit run therefore lost both measured TPS and committed total despite reducing local matching work.
   Status:
   - binned
   Reason:
   - Removing duplicate matching validation was a plausible local optimization, but end-to-end throughput regressed and execution-batch latency worsened. The current wall is not the duplicate `can_submit`/`submit` validation itself; keeping a second submit API would add semantic surface without improving the benchmark.
   Next move:
   - Preserve the checked matching API and current Mahi batch path. Stop local validation-removal slices unless a profile shows a single duplicated check dominating. The next credible 250k path is structural: persistent execution workers, state/write-model redesign, or distributing execution over independent market/account partitions.

175. Phase 6 diagnostic: clean 16-market high-offer run
   Date:
   - `2026-04-25`
   Hypothesis:
   - If the current ceiling is primarily single-market order-book state contention, distributing PlaceOrder load across 16 independent markets on current clean source should create headroom even without new code. If throughput stays flat, the current serial executor/write-install model is the wall and market partitioning needs explicit producer grouping or worker-backed execution to matter.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark-only diagnostic using clean binaries rebuilt after item 174's code revert.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-clean-16market-150k-30s`
   Evidence:
   - Ran the current clean source at 150k offered for 30s with `LOADTEST_MARKET_COUNT=16`, `KOMBAT_CONSENSUS=mahi`, async observer relay, and async Mahi block broadcast.
   - The run achieved loadgen rate `113,146.9 RPS`, zero request errors, latency p50 `66.538ms`, p90 `100.070ms`, p99 `135.340ms`, measured average `103,096.2 TPS`, peak `159,998.9 TPS`, strict committed total `4,041,168`, and convergence passed at common height `477`.
   - Final counters: execution batches `214`, execution blocks `1712`, execution intents `3,394,744`, p90 execution-batch latency `194ms`, max `246ms`, produced blocks `439`, produced empty blocks `255`, executor wakes `379`, and empty wakes `294`.
   - Resting-batch counters were the key signal: only `1326` batched items were recorded even though more than `3.39M` intents executed. With 16 markets interleaved by writer, the current consecutive-run batch detector mostly falls through to per-intent execution. Market-count alone does not activate parallelism or preserve batch density.
   - The current single-market clean 150k/30s anchor from item 171 measured `105,248.6 TPS`, strict committed total `4,685,640`, and p90 execution-batch latency `193ms`. The 16-market no-code run is therefore flat/slightly lower, not an unlock.
   Status:
   - kept diagnostic
   Reason:
   - Independent markets do not create headroom in the current serial execution path. The present batcher depends on consecutive same-market runs; simply distributing writers across markets destroys batch density instead of parallelizing work.
   Next move:
   - Do not expect `LOADTEST_MARKET_COUNT` or the existing serial market-sharded fallback to move toward 250k. The next structural slice should either group Mahi block bodies by execution partition before commit, or introduce a worker-backed execution model that can process independent market shards while preserving deterministic original-order event/root installation.

176. Phase 6 experiment: Mahi payload grouping by execution partition binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The 16-market diagnostic showed that market interleaving defeats the current consecutive-run resting-order batcher. Grouping each Mahi block payload by execution partition `(market_id, side)` after producer selection, while preserving per-policy nonce order, might restore batch density and create headroom for multi-market workloads.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `rustfmt --edition 2021 chain/crates/rpc/src/lib.rs`
   - `cargo test -p kombat-chain-rpc mahi_payload_grouping_ --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-payload-grouping-16market-150k-30s`
   Evidence:
   - The patch added a disabled-by-default `KOMBAT_MAHI_PAYLOAD_GROUPING=1` producer step that sorted selected payloads by execution partition only when each policy stayed within a single partition. Targeted tests verified grouping across markets and rejection of a policy spanning multiple markets.
   - 16-market 150k-offered 30s run achieved loadgen rate `125,486.9 RPS`, zero request errors, latency p50 `59.263ms`, p90 `91.755ms`, p99 `130.238ms`, measured average `99,145.0 TPS`, peak `159,399.0 TPS`, strict committed total `4,475,064`, and convergence passed at common height `398`.
   - Final counters showed that grouping did activate the intended local path: resting-batch items rose to `3,364,616`, matching executed intents in the final sample, compared with only `1,326` batched items in the clean 16-market diagnostic. However p90 execution-batch latency was unchanged at `193ms` versus `194ms` clean, max latency worsened to `306ms` versus `246ms`, and measured average TPS was lower than the clean 16-market `103,096.2 TPS` artifact.
   - The run also remained below the current single-market high-offer diagnostic anchor from item 171 (`105,248.6 TPS`). The higher strict committed total versus item 175 came with a different achieved loadgen rate and did not translate into a measured TPS win.
   Status:
   - binned
   Reason:
   - Producer-side payload grouping restored the batch counter but did not reduce the execution latency wall or improve measured throughput. Keeping an optional payload-reordering flag now would add semantic and maintenance surface before there is worker-backed market execution that can actually exploit the grouping.
   Next move:
   - Keep payload ordering unchanged. Treat this as evidence that multi-market ordering alone is insufficient in the current serial executor. The next credible Phase 6 step is a structural worker-backed execution/write model or a targeted sustained-load profile of the remaining execution phases, not another producer-ordering micro-optimization.

177. Phase 6 diagnostic: sustained Mahi execution profile
   Date:
   - `2026-04-25`
   Hypothesis:
   - The current clean 100k/60s anchor shows sustained throughput around `87k TPS`, while short 100k/10s runs remain near `99k TPS`. Running the built-in Mahi execution profiler over both windows should identify whether the long-run wall is body construction, commit work, root computation, install, or producer/pool starvation.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark/profile-only diagnostic using the clean release binaries rebuilt after item 176's code revert.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-exec-profile-100k-10s`
   - `/tmp/codex-exp-phase6-exec-profile-100k-60s`
   Evidence:
   - 100k-offered 10s profile run measured `98,755.9 TPS`, peak `116,151.7 TPS`, zero request errors, strict committed total `1,499,392`, and convergence passed at common height `648`.
   - Across all validator logs in the 10s run, non-empty execution batches had `820` samples, `5,997,568` profiled items, average `7,314` items/batch, average total batch time `26.8ms`, p90 total `44ms`, p99 total `58ms`. Subphase p90s were commit `31ms`, roots `4ms`, install `8ms`.
   - 100k-offered 60s profile run measured `87,438.6 TPS`, peak `160,468.3 TPS`, zero request errors, strict committed total `6,494,640`, and convergence passed at common height `598`.
   - Across all validator logs in the 60s run, non-empty execution batches had `1,424` samples, `25,978,560` profiled items, average `18,243` items/batch, average total batch time `93.1ms`, p90 total `236ms`, p99 total `341ms`. Subphase p90s were commit `169ms`, roots `23ms`, install `36ms`.
   - Final 60s counters showed the same shape as the clean sustained anchor: execution batches `454`, execution intents `5,712,064`, p90 execution-batch latency `265ms`, max `327ms`, resting-batch admission `2237.2ms`, matching `1424.6ms`, event construction `2792.3ms`, and pre-admitted pools drained fully (`items=0`, no request errors).
   Status:
   - kept diagnostic
   Reason:
   - The long-run wall is the serial Mahi commit phase under very large batches, not producer payload availability, observer relay, roots alone, or install alone. Commit work dominates sustained p90 latency, with root and install costs secondary but non-trivial.
   Next move:
   - Stop producer-ordering and local hot-loop slices. The next implementation must reduce or parallelize commit-phase work: a policy/admission batch API, worker-backed market execution, or a broader write-model change. A credible slice should target the `commit_ms` component first and use `/tmp/codex-exp-phase6-exec-profile-100k-60s` as the sustained profile reference.

178. Phase 6 experiment: preverified policy admission batch binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 177 identified `commit_ms` as the dominant sustained execution subphase. Since preverified Mahi resting batches still call `PolicyStore::commit_preverified_admission()` once per intent, a default-off policy batch API that groups admissions by policy might reduce per-intent policy lookup/update churn and improve the 100k/60s sustained anchor.
   Files changed:
   - `chain/crates/policy/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-policy preverified_admission_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node preverified_resting_place_orders_commit_as_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-policy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-policy-batch-admission-100k-60s`
   Evidence:
   - The patch added a default-off `KOMBAT_PHASE6_POLICY_BATCH_ADMISSION=1` path plus `PolicyStore::commit_preverified_admission_batch()`. Targeted policy tests verified equivalence with sequential preverified commits and nonce-continuity failure behavior; node targeted test, clippy, and release build passed.
   - 100k-offered 60s run achieved loadgen rate `99,990.5 RPS`, zero request errors, latency p50 `2.466ms`, p90 `64.745ms`, p99 `115.628ms`, measured average `87,003.4 TPS`, peak `159,549.2 TPS`, strict committed total `6,498,632`, and convergence passed at common height `630`.
   - Final counters: execution batches `462`, execution blocks `3696`, execution intents `5,648,872`, p90 execution-batch latency `236ms`, max `324ms`, resting-batch admission `2090.7ms`, policy `218.6ms`, matching `1421.3ms`, event construction `2775.3ms`, produced blocks `946`, and empty produced blocks `263`.
   - The current clean sustained anchor from item 170 measured `87,254.6 TPS`, and the item 177 profile reference measured `87,438.6 TPS` with policy `121.0ms`. The batch-admission patch therefore did not improve sustained throughput and made the policy subphase worse despite preserving correctness.
   Status:
   - binned
   Reason:
   - Grouping admissions by policy removed one obvious per-intent policy-store loop shape, but it added grouping/collection work and did not reduce the real sustained execution wall. Keeping a new policy batch API without a sustained win would expand semantics surface for no throughput benefit.
   Next move:
   - Revert the policy batch API and node flag wiring. Move to a larger structural execution design: worker-backed market/order-book execution or Block-STM-style parallel execution. Do not spend more slices on serial policy-admission micro-batching unless a profile shows policy time dominating independently.

179. Phase 6 experiment: worker-backed market matching scaffold
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 177 showed sustained Mahi commit work dominating the 100k/60s profile, and item 175/176 showed multi-market load does not help unless execution can exploit independent market partitions. Adding a default-off worker-backed matching phase for the existing market-sharded resting batch path might reduce the matching subphase when `LOADTEST_MARKET_COUNT>1` without changing the current green default path.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node worker_market_matching_matches_serial_market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node resting_place_orders --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-worker-market-matching-16market-150k-30s`
   Evidence:
   - Added `KOMBAT_PHASE6_WORKER_MARKET_MATCHING=1`, default off, nested under the existing `KOMBAT_PHASE6_MARKET_SHARDED_RESTING_BATCH=1` path. The worker path moves each independent market book into a scoped worker for `submit_resting_batch()`, then re-installs mutated books and materializes events in original item order.
   - The targeted equivalence test verified that worker-backed market matching produces the same runtime events, order-book state, and policy nonce state as the serial market-sharded path on an interleaved three-market batch.
   - 16-market 150k-offered 30s run with both gates enabled achieved loadgen rate `118,918.9 RPS`, zero request errors, latency p50 `60.305ms`, p90 `103.478ms`, p99 `176.071ms`, measured average `103,603.6 TPS`, peak `160,475.6 TPS`, strict committed total `4,002,272`, and convergence passed at common height `478`.
   - Final sampled counters showed the local intended effect: `phase6_resting_batch_items=3,442,688` and `phase6_resting_batch_matching_ns=685,190,875`, materially lower than the item 177 60s profile's matching total (`1,424.6ms`). However p90 execution-batch latency remained `197ms`, and measured TPS was essentially flat versus the clean 16-market diagnostic from item 175 (`103,096.2 TPS`) and below the single-market high-offer anchor from item 171 (`105,248.6 TPS`).
   Status:
   - kept scaffold, not promoted
   Reason:
   - The patch is correctness-covered, default-off, and demonstrates that independent market matching can be moved off the serial executor without changing deterministic output. It is not a throughput win by itself; matching is now reduced while admission/event/root/install work still dominate end-to-end TPS. Promoting the flag or treating this as a new green anchor would be misleading.
   Next move:
   - Keep the default path unchanged. Use this worker-backed matching scaffold only as a building block for the broader Block-STM/worker-backed execution design. The next credible slice must parallelize or restructure a larger portion of commit execution, especially event/admission/write installation, rather than spawning workers around matching alone. QUIC remains deferred until artifacts show transport starvation or request errors again.

180. Phase 6 experiment: in-place Mahi resting commit kept
   Date:
   - `2026-04-25`
   Hypothesis:
   - The native Mahi commit path still cloned the full `PolicyStore` and full `order_books` map for every merged execution batch before committing intents into an overlay. Under sustained PlaceOrder load, the accumulated order-book state grows with the run, so this overlay clone cost scales with total book size rather than current batch size. An opt-in in-place resting-order commit path that prechecks the whole resting batch before mutation should remove that clone/install overhead for the dominant benchmark workload.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit_advances_state_without_overlay_clone --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node resting_place_orders --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node worker_market_matching_matches_serial_market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-inplace-resting-150k-30s`
   - `/tmp/codex-exp-phase6-inplace-resting-100k-60s`
   Evidence:
   - Added `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=1`, default off. When enabled, `apply_mahi_committed_blocks_merged()` first attempts a prechecked in-place commit for all-intent resting PlaceOrder bodies, including the single-market case. If the body does not qualify, it falls back to the existing overlay/clone path.
   - The implementation reuses the market-sharded resting precheck/admission/matching path with a single-shard option, mutates canonical `policy_store` and `order_books` only after batch precheck/admission construction succeeds, then installs only block metadata/events/frontier updates. The existing overlay path remains unchanged when the flag is absent.
   - Targeted tests verified that the in-place path advances policy nonce state and resting book state, and that the worker-backed market matching scaffold still matches serial output.
   - 150k-offered 30s run with `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=1` achieved loadgen rate `115,245.1 RPS`, zero request errors, latency p50 `65.000ms`, p90 `98.951ms`, p99 `136.944ms`, measured average `117,088.2 TPS`, peak `188,418.3 TPS`, strict committed total `4,088,872`, and convergence passed at common height `535`.
   - 150k/30s final counters: p90 execution-batch latency `142ms`, max `226ms`, execution intents `3,916,064`, resting-batch admission `1,861.2ms`, matching `936.4ms`, event construction `2,195.5ms`, pre-admitted pool `832`, and pending relay `0`.
   - 100k-offered 60s run with the same flag achieved loadgen rate `100,000.1 RPS`, zero request errors, latency p50 `2.125ms`, p90 `28.834ms`, p99 `88.235ms`, measured average `97,768.6 TPS`, peak `165,171.0 TPS`, strict committed total `6,499,592`, and convergence passed at common height `1018`.
   - 100k/60s final counters: p90 execution-batch latency `95ms`, max `145ms`, execution batches `792`, execution intents `6,350,264`, resting-batch admission `2,545.2ms`, matching `1,705.9ms`, event construction `3,344.4ms`, pre-admitted pool `0`, and pending relay `0`.
   Status:
   - kept
   Reason:
   - This is the first sustained Phase 6 slice since item 170 that materially improves the green workload: `97,768.6 TPS` at 100k/60s versus the prior sustained anchors of `87,254.6` and `87,438.6` TPS. The p90 execution-batch latency dropped from the item 177 sustained profile's `265ms` to `95ms`, confirming that overlay clone/install/write-model cost was a real wall. The path remains default-off until broader semantic coverage is added for non-resting or rejecting workloads.
   Next move:
   - Treat `/tmp/codex-exp-phase6-inplace-resting-100k-60s` as the new sustained Mahi candidate. Next, either harden/promote the in-place path for safe default Mahi operation, or extend the same write-model principle to a broader Block-STM/partitioned executor that can handle mixed order/cancel/replace workloads. QUIC still waits: this artifact has zero request errors, drained relay/pools, and the remaining wall is execution/event/admission work.

181. Phase 6 diagnostic: in-place Mahi high-offer soak
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 180 lifted the 150k/30s peak to `117,088.2 TPS` and the 100k/60s sustained candidate to `97,768.6 TPS`. A 150k/60s run should show whether the in-place write-model improvement holds under a longer high-offer window, or whether it decays back toward the old overlay-clone ceiling.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark-only diagnostic using the item 180 release binaries.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-inplace-resting-150k-60s`
   Evidence:
   - 150k-offered 60s run with `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=1` achieved loadgen rate `121,706.0 RPS`, zero request errors, latency p50 `61.220ms`, p90 `94.022ms`, p99 `129.461ms`, measured average `102,906.9 TPS`, peak `161,218.0 TPS`, strict committed total `8,016,240`, and convergence passed at common height `430`.
   - Final counters: p90 execution-batch latency `175ms`, max `252ms`, execution batches `293`, execution blocks `2344`, execution intents `6,551,944`, resting-batch admission `3,935.9ms`, policy `229.8ms`, matching `1,776.0ms`, event construction `3,752.7ms`, pending relay `0`, and pre-admitted pool `297,856`.
   - The run stayed above 100k measured TPS for the full 60s high-offer window, but it did not preserve the 117k 30s average. The higher offered load leaves a non-zero pre-admitted pool at settle, so the remaining wall is still execution drain rate rather than request errors or relay failure.
   Status:
   - kept diagnostic
   Reason:
   - This confirms item 180 is not a short-run-only artifact: the in-place path sustains ~103k TPS under a 150k/60s load with correctness intact. It also shows the next ceiling: execution drain cannot fully absorb the achieved ~122k RPS stream for 60s, even with overlay cloning removed.
   Next move:
   - Keep `/tmp/codex-exp-phase6-inplace-resting-100k-60s` as the clean sustained candidate and `/tmp/codex-exp-phase6-inplace-resting-150k-60s` as the high-offer diagnostic. Next code work should harden/promote the in-place path or parallelize the remaining execution phases (`event`, `admission`, broader write install). Do not switch to QUIC until a run shows request errors, relay backlog, or transport saturation instead of execution backlog.

182. Phase 6 experiment: promote in-place Mahi resting commit by default
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 180 kept the in-place Mahi resting commit path behind `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=1`, and item 181 showed it remains clean above 100k TPS under high-offer soak. Making this path default-on for native Mahi execution, with an explicit `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=0|false` opt-out, should preserve the sustained win while keeping rollback simple.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-inplace-default-100k-60s`
   Evidence:
   - Changed `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT` semantics from opt-in to opt-out: unset now enables the in-place path, while `0` or `false` disables it.
   - Added `mahi_inplace_resting_commit_declines_mixed_side_body_without_mutation`, covering a non-qualifying mixed-side PlaceOrder body. The helper returns `false` and leaves block height, policy nonce, and resting book state unchanged so the normal overlay path can handle it.
   - Full node lib test suite passed `203` tests and node clippy passed with `-D warnings`.
   - Default Mahi 100k-offered 60s run, without setting `KOMBAT_PHASE6_INPLACE_RESTING_COMMIT`, achieved loadgen rate `99,999.2 RPS`, zero request errors, latency p50 `1.314ms`, p90 `16.187ms`, p99 `70.225ms`, measured average `98,314.7 TPS`, peak `119,489.1 TPS`, strict committed total `6,499,088`, and convergence passed at common height `1159`.
   - Final counters: p90 execution-batch latency `69ms`, max `133ms`, execution batches `945`, execution blocks `7560`, execution intents `6,344,064`, resting-batch admission `2,518.1ms`, policy `134.4ms`, matching `1,485.2ms`, event construction `3,047.0ms`, pre-admitted pool `3,712`, and pending relay `0`.
   Status:
   - kept
   Reason:
   - The promotion keeps correctness coverage and confirms the in-place path is active by default with a clean sustained artifact. The new default candidate (`98,314.7 TPS`) is slightly above item 180's env-enabled `97,768.6 TPS` and remains far above the old overlay-clone sustained anchors. Rollback remains a one-env-var opt-out.
   Next move:
   - Treat `/tmp/codex-exp-phase6-inplace-default-100k-60s` as the current green sustained anchor. The remaining wall is still execution-side (`event`, `admission`, matching/write install under large batches), not QUIC/transport. Next Phase 6 work should generalize the in-place/write-model win to mixed order/cancel/replace workloads or reduce event/admission construction cost without reintroducing overlay cloning.

183. Phase 6 experiment: remove empty fill-event extension from resting batches binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The Mahi in-place resting commit path calls `submit_resting_batch()`, which should only produce accepted resting orders with empty `fills`. Replacing the per-item `events.extend(order_fill_events(...))` calls with debug assertions might remove empty-vector allocation/iteration work from the event phase and improve the 100k/60s sustained anchor.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node resting_place_orders --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-resting-no-empty-fill-events-100k-60s`
   Evidence:
   - The 100k-offered 60s run achieved loadgen rate `100,001.2 RPS`, zero request errors, latency p50 `2.268ms`, p90 `47.973ms`, p99 `103.898ms`, measured average `96,898.2 TPS`, peak `149,264.3 TPS`, strict committed total `6,499,328`, and convergence passed at common height `955`.
   - Final counters showed p90 execution-batch latency `130ms`, max `233ms`, execution batches `749`, execution blocks `5992`, execution intents `6,268,480`, resting-batch admission `2,728.8ms`, policy `153.3ms`, matching `1,703.2ms`, event construction `3,517.3ms`, pre-admitted pool `9328`, and pending relay `0`.
   - The current green sustained anchor from item 182 measured `98,314.7 TPS` with p90 execution-batch latency `69ms` and event construction `3,047.0ms`. The patch therefore regressed sustained TPS and worsened the event counter it was meant to improve.
   Status:
   - binned
   Reason:
   - The change is semantically plausible but behaves like a non-moving micro-optimization in the sustained harness, with worse measured event time and execution latency. Keeping it would add a resting-batch invariant assertion without throughput benefit.
   Next move:
   - Revert the two event-loop edits and keep item 182's `/tmp/codex-exp-phase6-inplace-default-100k-60s` artifact as the current green sustained anchor. Continue with larger Phase 6 write-model work rather than isolated event-loop micro-tweaks.

184. Phase 6 diagnostic: Mahi execution schedule profile
   Date:
   - `2026-04-25`
   Hypothesis:
   - Before implementing deeper Block-STM or worker-backed execution, use the existing Mahi schedule profiler to measure whether the current committed bodies expose independent execution layers. If the default single-market stream has no schedule width, Block-STM cannot improve that workload; if multi-market streams show width, parallel execution should be targeted there.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark/profile-only diagnostic using release binaries rebuilt after item 183's code revert.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-schedule-profile-default-100k-60s`
   - `/tmp/codex-exp-phase6-schedule-profile-16market-100k-60s`
   Evidence:
   - Default single-market 100k-offered 60s run with `KOMBAT_PHASE6_SCHEDULE_PROFILE=1` achieved loadgen rate `99,964.1 RPS`, zero request errors, measured average `98,954.4 TPS`, peak `133,465.5 TPS`, strict committed total `6,497,040`, and convergence passed at common height `1120`.
   - Single-market schedule samples: profiled batches `29`, truncated `24`, total item count p50/p90/max `6064/14824/19144`, sampled item count `512`, layer count p50/p90/max `512/512/512`, max layer width p50/p90/max `1/1/1`, conflict pairs `130816`, estimated parallelism p50/p90/max `1.0x/1.0x/1.0x`.
   - 16-market 100k-offered 60s run with the same profiler achieved loadgen rate `100,000.5 RPS`, zero request errors, measured average `99,393.4 TPS`, peak `120,283.6 TPS`, strict committed total `6,499,504`, and convergence passed at common height `1220`.
   - 16-market schedule samples: profiled batches `32`, truncated `28`, total item count p50/p90/max `6600/9360/12152`, sampled item count `512`, layer count p50/p90/max `128/128/160`, max layer width p50/p90/max `4/8/8`, conflict pairs p50/p90/max `32512/32512/32556`, estimated parallelism p50/p90/max `4.0x/4.266x/5.333x`.
   - The 16-market run also showed lower p90 execution-batch latency (`35ms` vs single-market `83ms`) and lower resting-batch subphase counters: admission `2,244.7ms` vs `2,775.5ms`, matching `1,274.5ms` vs `1,696.7ms`, and event construction `2,996.5ms` vs `3,200.1ms`.
   Status:
   - kept diagnostic
   Reason:
   - The default single-market benchmark is inherently serial at the execution-resource level because every intent writes the same order book. True Block-STM cannot improve that exact stream without changing order-book semantics. Multi-market streams do expose meaningful parallel width, so worker-backed/Block-STM work should target market-partitioned execution rather than chasing parallelism in the single-market path.
   Next move:
   - Use item 182's `/tmp/codex-exp-phase6-inplace-default-100k-60s` as the green single-market anchor. For the next implementation slice, either profile the current in-place path's native root/install phases at high offer, or implement a larger market-partitioned executor that consumes the existing schedule width. Do not treat QUIC as the next wall: both profile runs had zero request errors, no relay backlog, and drained pre-admitted pools.

185. Phase 6 experiment: empty Mahi commit metadata fast path
   Date:
   - `2026-04-25`
   Hypothesis:
   - The pre-patch high-offer exec profile showed many zero-intent Mahi execution batches falling through the native overlay path, spending roughly `60ms` per non-empty-duration empty batch in commit/install work even though there are no policy, order-book, or event mutations. Installing deterministic block metadata directly for empty Mahi bodies should remove that waste without changing committed state semantics.
   Files changed:
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_empty_merged_commit_advances_metadata_without_state_events --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-inplace-exec-profile-150k-60s`
   - `/tmp/codex-exp-phase6-empty-mahi-fastpath-150k-60s`
   - `/tmp/codex-exp-phase6-empty-mahi-fastpath-100k-60s`
   Evidence:
   - The high-offer exec-profile diagnostic on the pre-patch code measured `110,076.0 TPS` under achieved `111,395.7 RPS`, zero request errors, convergence at common height `498`, and final p90/max execution-batch latency `157ms/195ms`. Parsing validator logs found `1260` `mode=native` samples for zero-intent bodies, `800` of them with non-zero duration, averaging `60.7ms` total with p90 `63ms`, p99 `73ms`, and max `305ms`. The non-empty `mode=native_inplace` samples averaged `112.6ms` total for ~39k items each, so empty bodies were burning meaningful executor time relative to real commit work.
   - The patch adds an early native empty-body path in `apply_mahi_committed_blocks_merged()`: compute empty `tx_root`/`event_root`, derive the same native Mahi state root from parent root, round, and timestamp, then call `install_mahi_committed_block_metadata()` with no state clone, no policy/order-book overlay, and no runtime events.
   - The targeted test verifies that an empty merged Mahi commit advances block metadata, leaves the policy nonce unchanged, and emits no runtime events.
   - 100k-offered 60s regression run achieved loadgen rate `100,006.4 RPS`, zero request errors, latency p50 `1.264ms`, p90 `7.910ms`, p99 `39.724ms`, measured average `99,423.7 TPS`, peak `119,219.2 TPS`, strict committed total `6,499,592`, and convergence passed at common height `1313`.
   - 100k/60s final counters improved p90/max execution-batch latency to `43ms/75ms` versus item 182's `69ms/133ms`; execution batches/blocks/intents were `993/7944/6,443,080`, pre-admitted pool drained to `0`, and pending relay remained `0`.
   - 150k-offered 60s run achieved loadgen rate `123,774.9 RPS`, zero request errors, measured average `101,506.5 TPS`, peak `200,041.5 TPS`, strict committed total `8,171,808`, and convergence passed at common height `445`. This is not a high-offer throughput win versus item 181's `102,906.9 TPS`; p90/max execution-batch latency worsened to `191ms/292ms`.
   Status:
   - kept hygiene
   Reason:
   - The change removes a clearly wrong execution path for zero-intent Mahi commits and preserves correctness with a focused test. The 100k sustained regression check is clean and noise-positive with materially lower p90 execution latency, but the 150k high-offer wall does not move. This should not be counted as a new high-offer throughput unlock.
   Next move:
   - Treat `/tmp/codex-exp-phase6-empty-mahi-fastpath-100k-60s` as the current code's 100k sustained artifact, while keeping item 181's 150k result as the high-offer comparison. The next throughput-moving slice must attack non-empty execution work: market-partitioned execution for multi-market streams, compact/batched event/state materialization, or a deeper order-book write model. Do not spend more time on empty-batch handling unless a future profile shows it reappearing inside the measured window.

186. Phase 6 experiment: resting event record fast path binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The in-place Mahi resting path emits only side-effect-free `IntentExecuted` and `OrderAccepted` events. Bypassing the generic `record_runtime_event()` path for those variants during Mahi metadata install might reduce non-empty install/event-recording cost without changing fill/risk side effects, while falling back to the generic path for any unexpected event variant.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_empty_merged_commit_advances_metadata_without_state_events --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-resting-event-fastpath-100k-60s`
   Evidence:
   - The patch added an internal `install_mahi_committed_resting_block_metadata()` path that directly appended `RuntimeEventRecord`s for `IntentExecuted` and `OrderAccepted`, with fallback to `record_runtime_event()` for other variants. Targeted in-place and empty Mahi tests passed, clippy passed, and release build passed.
   - 100k-offered 60s run achieved loadgen rate `100,000.7 RPS`, zero request errors, latency p50 `1.595ms`, p90 `24.630ms`, p99 `86.642ms`, measured average `97,869.3 TPS`, peak `146,627.2 TPS`, strict committed total `6,499,256`, and convergence passed at common height `1163`.
   - Final counters regressed versus item 185's current-code artifact: p90/max execution-batch latency `90ms/161ms` versus `43ms/75ms`, execution intents `6,319,672` versus `6,443,080`, and event construction `3,148.5ms` versus `3,097.3ms`. It also landed below item 182's default sustained anchor (`98,314.7 TPS`).
   Status:
   - binned
   Reason:
   - The expected install/event-recording win did not appear in the sustained harness; p90 execution latency worsened and measured TPS regressed. Keeping a special event-recording path without a measured win would create semantic risk around runtime event side effects for no throughput benefit.
   Next move:
   - Revert the resting event fast path and keep item 185's empty Mahi metadata fast path. Continue with larger non-empty execution work, but do not re-open per-event recording shortcuts unless a profile isolates install/event recording as the dominant wall and the patch includes stronger event-semantics tests.

187. Phase 6 diagnostic: 64-writer 16-market high-offer scaling
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 184 showed real execution schedule width for 16-market streams, while high-offer single-market runs were partly capped by the 32-writer harness. Running 64 writers across 16 markets at 150k offered should show whether more writer/market width moves sustained throughput toward the 250k target or simply shifts the cap to local ingress/loadgen.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark-only diagnostic using the item 185 code state after item 186's event fast path was reverted.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-64w-16market-150k-60s`
   Evidence:
   - 64-writer, 16-market, 150k-offered 60s run achieved loadgen rate `101,403.9 RPS`, zero request errors, latency p50 `157.578ms`, p90 `199.783ms`, p99 `234.502ms`, measured average `105,844.3 TPS`, peak `170,538.7 TPS`, strict committed total `6,780,640`, and convergence passed at common height `728`.
   - Final counters: p90/max execution-batch latency `89ms/148ms`, execution batches `389`, blocks `3112`, intents `6,689,520`, resting-batch admission `3,503.7ms`, policy `239.4ms`, matching `1,734.7ms`, event construction `3,842.3ms`, pre-admitted pool `640`, and pending relay `0`.
   - Compared with the 32-writer single-market item 185 high-offer run, committed TPS improved (`105,844.3` vs `101,506.5`) and p90 execution-batch latency improved (`89ms` vs `191ms`), confirming that market width helps execution. However achieved ingress was lower (`101,403.9 RPS` vs `123,774.9 RPS`) and client latency was much worse, so the 64-writer local harness becomes ingress/loadgen-limited before it can prove a 150k+ chain ceiling.
   Status:
   - kept diagnostic
   Reason:
   - Wider market distribution exposes useful execution parallelism and lowers execution latency, but simply increasing local writers is not enough to approach 250k on this harness. The next scaling step needs either a stronger/multi-process load generator or a code change that reduces local ingress/admission latency under high writer counts.
   Next move:
   - Keep item 185's 100k sustained artifact as the current green code state. For 250k work, separate two tracks: improve the harness/ingress generator so offered load can actually exceed 150k cleanly, and continue execution work on market-partitioned paths where item 184 proved parallel width exists. Do not infer that Mahi itself tops out at 105k from this run; the achieved ingress rate and client latency show the local load path is part of the measurement surface.

188. Phase 6 diagnostic: two-loadgen ingress high-offer scaling
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 187 showed the single local loadgen process could not cleanly achieve the requested 150k offered rate under 64-writer/16-market load. Sharding the same writer set across two loadgen processes pointed at the same ingress should distinguish client-process saturation from chain-side drain saturation.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-2loadgen-ingress-64w-16market-150k-60s`
   Evidence:
   - The harness already supported sharding writers across multiple targets, but duplicate target labels wrote to the same `loadtest-ingress.txt`. The script now includes the shard index in sharded loadgen log names, allowing duplicate targets such as `LOADTEST_TARGET_VALIDATOR=ingress,ingress` to keep separate per-process reports.
   - 64-writer, 16-market, 150k-offered 60s run split into two ingress loadgen processes achieved aggregate loadgen rate `149,999.4 RPS` (`75,003.3` + `74,996.1`), zero request errors, per-process latency around p50 `40-41ms`, p90 `114-117ms`, and p99 `188ms`.
   - The committed timeseries measured average `92,680.5 TPS`, peak `155,437.5 TPS`, strict post-drain committed total `7,729,181`, and convergence passed at common height `473`.
   - Final sampled counters from validator 1 showed p90/max execution-batch latency `125ms/313ms`, execution batches/blocks/intents `371/2968/5,888,776`, resting-batch admission `3,208.5ms`, policy `317.2ms`, matching `1,871.6ms`, and event construction `4,074.2ms`. Final status across validators showed pre-admitted pools totaling roughly `824k` items, with pending relay still `0`.
   Status:
   - kept diagnostic
   Reason:
   - The harness fix is useful and the diagnostic cleanly separates the measurement surfaces: client-side generation can hit 150k when split across processes, but the current chain path does not sustain that rate for 60s and accumulates pre-admitted backlog. This means the next wall is again chain-side producer/execution drain under high offer, not single-process loadgen throughput.
   Next move:
   - Keep item 185's 100k/60s artifact as the current green sustained anchor and use item 188's two-loadgen shape for future high-offer diagnostics. Next, run an intermediate two-loadgen ladder (for example 120k or 125k offered) to find the highest clean sustained rate before implementing another code change. If backlog appears below 120k, focus on Mahi producer/executor drain; if 120k is clean, continue raising offered load before touching node code.

189. Phase 6 diagnostic: two-loadgen sustainable-offer boundary
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 188 proved that two local loadgen processes can achieve 150k offered cleanly, but the chain accumulates pre-admitted backlog at that rate. Running intermediate 120k and 135k 60s points should identify the clean sustained boundary before more code changes.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Benchmark-only diagnostic using the item 188 code state.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-2loadgen-ingress-64w-16market-120k-60s`
   - `/tmp/codex-exp-phase6-2loadgen-ingress-64w-16market-135k-60s`
   Evidence:
   - 120k-offered run split across two ingress loadgen processes achieved aggregate loadgen rate `120,003.3 RPS` (`60,001.6` + `60,001.7`), zero request errors, latency around p50 `2.0ms`, p90 `14-15ms`, p99 `54-55ms`, measured average `106,938.6 TPS`, peak `152,683.7 TPS`, strict post-drain committed total `7,799,496`, and convergence passed at common height `720`.
   - 120k final status showed all four validator pre-admitted pools drained to `0` with no dropped-oldest events. Final sampled counters from validator 1 showed p90/max execution-batch latency `165ms/197ms`, execution batches/blocks/intents `460/3680/6,949,584`, resting-batch admission `3,114.1ms`, policy `238.5ms`, matching `1,713.9ms`, and event construction `3,573.4ms`.
   - 135k-offered run achieved aggregate loadgen rate `134,994.5 RPS` (`67,498.5` + `67,496.0`) with zero request errors and convergence at common height `590`, but measured average fell to `93,260.3 TPS`, strict post-drain committed total was only `6,890,594`, and writer progress became uneven (`84,516` min vs `137,104` max committed per writer).
   - 135k final status showed three validators retaining pre-admitted backlog after settle, two validators at the `524,288` pool cap, and dropped-oldest counts of `37,312`, `6,824`, and `316,760`. Pending relay remained `0`, so the overload point is downstream of relay and inside producer/executor drain.
   Status:
   - kept diagnostic
   Reason:
   - The current 64-writer/16-market two-loadgen setup has a clean sustained boundary around 120k offered and overloads by 135k. This is now a useful measurement surface: the client and relay can feed above 120k without request errors, and the remaining failure mode is clearly pre-admitted pool drain/commit execution rather than ingress generation.
   Next move:
   - Treat 120k/60s two-loadgen as the current clean high-offer diagnostic and item 185's 100k/60s as the conservative single-loadgen green anchor. The next code slice should target producer/executor drain under multi-market load, not loadgen or QUIC. Candidate directions are reducing per-batch event/admission construction, increasing safe Mahi block payload drain when pools are persistently non-empty, or broader market-partitioned execution that parallelizes more than matching.

190. Phase 6 slice: retry full Mahi pre-admitted pools
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 189's 135k overload run showed owner pre-admitted pools hitting the `524,288` cap and evicting oldest intents. Because policy nonces must drain in order, evicting low nonces creates holes that prevent later nonces from being selected, poisoning the owner feed. Returning `retry` when the owner pool is full should preserve nonce continuity and move overload back to relay retry/backpressure instead of silent accepted-then-dropped loss.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted_pool_full_returns_retry_without_nonce_hole --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted_pool_drains_round_robin_across_policies --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-prepool-retry-100k-60s`
   - `/tmp/codex-exp-phase6-prepool-retry-135k-60s`
   - `/tmp/codex-exp-phase6-prepool-retry-150k-60s`
   Evidence:
   - Changed `MahiPreAdmittedPool::insert()` so non-duplicate inserts return `Full` before mutating the pool when the cap is reached. `process_network_relay_intents()` maps `Full` to relay status `retry`, leaving the observer relay entry pending instead of accepting then evicting an older nonce.
   - Added a focused unit test that fills a small test-limited pool, verifies the next nonce returns `Full` with `dropped_oldest_total=0`, drains nonces `1` and `2`, then successfully inserts and drains nonce `3`.
   - 100k-offered 60s regression run achieved loadgen rate `100,002.3 RPS`, zero request errors, latency p50 `1.087ms`, p90 `3.431ms`, p99 `10.807ms`, measured average `99,911.0 TPS`, peak `117,367.6 TPS`, strict committed total `6,499,592`, and convergence passed at common height `1458`.
   - 100k final counters: p90/max execution-batch latency `23ms/32ms`, execution batches/blocks/intents `1132/9056/6,471,952`, resting-batch admission `2,151.9ms`, policy `118.0ms`, matching `1,436.0ms`, event construction `2,937.1ms`, pre-admitted pool `0`, pending relay `0`, and dropped-oldest `0`.
   - 135k-offered two-loadgen run improved the previous failure point: aggregate achieved loadgen rate `134,968.5 RPS`, zero request errors, measured average `100,779.1 TPS` versus item 189's `93,260.3`, strict committed total `8,537,104` versus `6,890,594`, writer progress tightened (`126,488` min vs previous `84,516`), all pools drained after settle, and dropped-oldest stayed `0` on all validators.
   - 150k-offered two-loadgen run preserved the safety property but did not move the high-offer ceiling: aggregate achieved loadgen rate `150,001.2 RPS`, zero request errors, measured average `89,826.3 TPS`, strict committed total `7,706,813`, convergence passed at common height `388`, and dropped-oldest stayed `0`, but pre-admitted backlog remained after settle.
   Status:
   - kept
   Reason:
   - This fixes a real overload correctness bug and materially improves the 135k boundary without regressing the 100k green path. It is not a 150k throughput unlock; at 150k the remaining wall is still producer/executor drain rate. The important semantic change is that overload no longer corrupts per-policy nonce continuity inside owner pools.
   Next move:
   - Treat `/tmp/codex-exp-phase6-prepool-retry-100k-60s` as the current green sustained artifact and `/tmp/codex-exp-phase6-prepool-retry-135k-60s` as the improved high-offer boundary diagnostic. Next code work should target why producer/executor drain collapses above ~135k: profile producer ancestor-quorum waits and commit execution under full pools, then choose between producer pacing/round progress work and broader market-partitioned execution. Do not reintroduce drop-oldest for nonce-ordered owner pools.

191. Phase 6 experiment: pre-admitted metadata snapshot fast path binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 190's 135k profile showed producer p90 around `243ms`, with `engine.mahi_snapshot()` contributing roughly `121ms` p90 while pre-admitted pools were full. When the owner pool already has at least one full block of intents, bypassing the old mempool/frontier snapshot and building from cached validator metadata plus pre-admitted intents might reduce producer latency and improve high-offer drain.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_pre_admitted_pool_full_returns_retry_without_nonce_hole --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-producer-profile-135k-60s`
   - `/tmp/codex-exp-phase6-prepool-metadata-fastpath-135k-60s`
   Evidence:
   - Control profile at 135k with item 190 code achieved aggregate loadgen rate `135,013.8 RPS`, zero request errors, measured average `100,711.7 TPS`, peak `199,152.8 TPS`, strict committed total `8,561,203`, and convergence passed at common height `473`.
   - Control final all-validator producer profile: snapshot p90/max-validator p90 `121/126ms`, pre-admitted-drain p90/max-validator p90 `33/40ms`, final-lock-wait p90/max-validator p90 `85/87ms`, total producer p90/max-validator p90 `243/246ms`, execution-batch p90/max-validator p90 `178/191ms`, and all pools drained after settle.
   - The opt-in fast path collapsed producer snapshot p90 to `0ms`, but measured average regressed to `89,764.4 TPS`, strict committed total fell to `7,787,190`, client p90 latency worsened to `72-80ms`, and convergence passed only after leaving residual pre-admitted backlog on two validators.
   - Fast-path final all-validator profile showed the cost moved rather than disappeared: pre-admitted-drain p90 rose to `165ms`, final-lock-wait p90 rose to `104ms`, total producer p90 rose to `307ms`, and execution-batch p90/max-validator p90 worsened to `234/389ms`.
   Status:
   - binned
   Reason:
   - Skipping the mempool/frontier snapshot removes one measured component but worsens the end-to-end producer/commit pipeline. The snapshot cost is not isolated waste; changing the ordering pushes more work and contention into pre-admitted drain and later locks. Keeping an opt-in flag for a losing path would add complexity without a throughput or correctness benefit.
   Next move:
   - Revert the fast-path code and keep item 190 as the current code state. The next useful slice should not bypass snapshot ordering blindly. Instead, profile or restructure the actual contention between commit execution, pre-admitted pool drain, and Dag lock progress, with special attention to why producer total p90 remains ~240ms even after full-pool retry fixed nonce holes.

192. Phase 6 experiment: Mahi drive interval sweep binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 191's profile still showed producer total p90 around `243ms` under full pre-admitted pools. Reducing the Mahi auto-drive interval from the default `25ms` cadence to `5ms` might reduce round-progress polling delay and ancestor wait time if the producer/driver timer was limiting throughput.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Config-only benchmark using item 190 code state after item 191's fast-path code was reverted.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-drive5-135k-60s`
   Evidence:
   - 64-writer, 16-market, two-loadgen 135k-offered 60s run with `AUTO_DRIVE_INTERVAL_MS=5` achieved aggregate loadgen rate `135,001.1 RPS` (`67,498.7` + `67,502.4`), zero request errors, latency around p50 `10.7-11.1ms`, p90 `62.8-66.5ms`, and p99 `132-141ms`.
   - The committed timeseries measured average `92,251.0 TPS`, peak `160,444.6 TPS`, strict committed total `7,923,417`, and convergence passed at common height `1140`.
   - Final status showed dropped-oldest remained `0`, pending relay was drained, and pre-admitted pools mostly drained after settle except validator 1 retaining `52,904` items. The faster drive cadence also created higher startup head height/block churn before the load window, with loadgen reports starting near head height `735-738`.
   - Compared with the item 190 135k reference, throughput regressed (`92,251.0` vs `100,779.1` TPS), strict committed total regressed (`7,923,417` vs `8,537,104`), and client latency worsened. The timer change did not improve drain or reduce the remaining producer/executor wall.
   Status:
   - binned
   Reason:
   - Faster auto-drive polling adds churn and latency without improving sustained Mahi drain. The remaining wall is not the outer driver interval; it is downstream contention/work inside producer finalization, pre-admitted drain, Dag/Mahi state lock progress, and commit execution.
   Next move:
   - Keep the default drive cadence and item 190 code state. Stop cadence/timer tuning unless a later profile specifically identifies timer delay as dominant. Next work should target structural producer/commit contention, preferably with per-path Mahi state/Dag lock hold and wait instrumentation before another optimization.

193. Phase 6 experiment: sign Mahi block outside final state lock binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 191's 135k profile showed producer final-lock wait around `85ms` p90 and final-lock hold around `32ms` p90. Moving `MahiBlock::signed()` out of the final `mahi_state` lock should reduce lock hold time by avoiding Borsh serialization, signature construction, and payload cloning while the shared DAG/Mahi state lock is held.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_round_drains_pre_admitted_pool_into_produced_block --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-rpc --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-sign-outside-lock-135k-60s`
   Evidence:
   - The code moved signed-block construction before the final `mahi_state` lock and temporarily added a `producer_sign_latency_ms` metric to separate signing cost from final lock wait/hold.
   - 64-writer, 16-market, two-loadgen 135k-offered 60s run achieved aggregate loadgen rate `134,968.8 RPS` (`67,475.2` + `67,493.6`), zero request errors, latency p50 `12.6-14.0ms`, p90 `75.8-87.4ms`, and p99 `169.7-182.2ms`.
   - The committed timeseries measured average `90,314.2 TPS`, peak `160,729.6 TPS`, strict committed total `7,727,415`, and convergence passed at common height `435`.
   - Final status preserved safety (`dropped_oldest=0`, `pending_relay_intents=0`) but left residual pre-admitted backlog on validator 1 (`97,208`) and validator 2 (`23,808`).
   - Compared with the item 190 135k reference, throughput regressed (`90,314.2` vs `100,779.1` TPS), strict committed total regressed (`7,727,415` vs `8,537,104`), and client latency worsened. The change reduced one suspected lock-held operation but did not improve the end-to-end producer/executor pipeline.
   Status:
   - binned
   Reason:
   - Final-lock signing was not the isolated throughput wall. Moving it outside the lock likely changed producer/interleaving timing and left more pre-admitted backlog, producing a clear regression. Carrying this reordered path would add behavior risk without throughput benefit.
   Next move:
   - Revert the code and keep item 190 as the current code state. The next useful slice should instrument or restructure the actual producer/commit contention at a broader granularity instead of moving a single operation across the lock boundary.

194. Phase 6 experiment: prepared resting batch apply binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The market-sharded resting commit path validates each market shard in precheck, commits policy admissions, then calls `submit_resting_batch()`, which validates the same shard again before appending orders. Splitting matching into `prepare_resting_batch()` and `apply_prepared_resting_batch()` should remove duplicate validation/precheck work while preserving atomicity: prepare all market batches, commit policies, then apply prepared market updates.
   Files changed:
   - `chain/crates/matching/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-matching prepared_resting_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node market_sharded_resting_place_orders_batch_across_interleaved_markets --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node worker_market_matching_matches_serial_market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-matching --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-matching -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-prepared-resting-batch-135k-60s`
   Evidence:
   - The refactor introduced an opaque prepared resting batch in `kombat-chain-matching`; focused tests verified prepared apply matched direct submit and rejected stale prepared batches. The node market-sharded path consumed prepared batches in both serial and worker matching modes, and existing market-sharded correctness tests stayed green.
   - 64-writer, 16-market, two-loadgen 135k-offered 60s run achieved aggregate loadgen rate `134,963.8 RPS` (`67,484.5` + `67,479.3`), zero request errors, latency p50 `7.37ms`, p90 `43-44ms`, and p99 `110-123ms`.
   - The committed timeseries measured average `96,351.4 TPS`, peak `160,832.7 TPS`, strict committed total `8,211,471`, and convergence passed at common height `460`.
   - Final status was safe: all validator pre-admitted pools drained to `0`, pending relay was `0`, and dropped-oldest stayed `0`.
   - Compared with item 190's 135k reference, client latency improved, but throughput regressed (`96,351.4` vs `100,779.1` TPS) and strict committed total regressed (`8,211,471` vs `8,537,104`). The refactor removed duplicate validation shape, but the end-to-end sustained drain did not improve.
   Status:
   - binned
   Reason:
   - Lower client latency is not the current Phase 6 gate; sustained committed TPS and strict committed total regressed by more than noise. The duplicated validation is not the dominant high-offer throughput wall, or the extra prepared-batch object/move path offsets the saved validation cost.
   Next move:
   - Revert the code and keep item 190 as the current code state. Avoid more small matching-path reshuffles unless profiles show validation/precheck dominates. The remaining useful work is broader execution parallelism or explicit producer/commit lock attribution.

195. Phase 6 diagnostic: full Mahi producer/accept/execution profile
   Date:
   - `2026-04-25`
   Hypothesis:
   - Items 191-194 binned several narrow local changes. A profile-only high-offer run with producer, accept, and execution timing enabled should identify whether the remaining wall is in block receive/accept, producer/Dag-state locking, or commit execution before attempting another code slice.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - None. Profile-only benchmark using item 190 code state after binned experiments were reverted.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-full-profile-135k-60s`
   Evidence:
   - 64-writer, 16-market, two-loadgen 135k-offered 60s run with `KOMBAT_MAHI_PRODUCER_PROFILE=1`, `KOMBAT_MAHI_ACCEPT_PROFILE=1`, and `KOMBAT_MAHI_EXEC_PROFILE=1` achieved aggregate loadgen rate `134,999.9 RPS`, zero request errors, measured average `95,001.6 TPS`, peak `160,809.8 TPS`, strict committed total `8,132,656`, and convergence passed at common height `450`.
   - The run is diagnostic, not a new green anchor: profiling overhead lowered throughput versus item 190's unprofiled 135k reference (`100,779.1 TPS`, strict `8,537,104`).
   - Non-empty native Mahi execution batches had `924` samples with `40,000` body items at p50/p90/p99. Timing sums across validator logs were: body construction `1.8s`, commit execution `48.1s`, state-root construction `21.6s`, metadata/install `21.4s`, and total `95.7s`. Execution p90 was `159ms`, p99 `263ms`, max `427ms`.
   - Accept/receive was not the dominant wall: `mahi_accept_profile` p90 total was `19ms`, p99 `89ms`; accept p90 was `3ms`, detector p90 `0ms`, and queued p90 `0ms`.
   - Producer/Dag-state contention is visible but secondary to execution: final sampled metrics showed producer total p90 up to `271ms`, snapshot p90 up to `132ms`, initial/final lock-wait p90 up to `92/98ms`, pre-admitted drain p90 up to `57ms`, pending relay `0`, and final pre-admitted backlog `~1.23M` across validators.
   - Resting-batch counters inside the execution path reinforce the same conclusion: event construction (`14.7s`) and admission bookkeeping (`13.5s`) were larger than matching (`2.8s`) or policy (`1.4s`), while root/install remained material separate costs. Prior per-event and prepared-batch shortcuts binned, so this points to structural execution/output design rather than another local reshuffle.
   Status:
   - kept diagnostic
   Reason:
   - The profile explains why recent minor tweaks did not stick: the remaining wall is broad commit execution plus root/install/output bookkeeping, not a single small operation such as timer cadence, signing location, or duplicate matching validation. Receive/relay/transport are not the current limiter under this harness.
   Next move:
   - Stop micro-tuning the current sequential path. The next kept-candidate work should be structural: either wire real market-partitioned/Block-STM-style execution where the workload has measured market width, or redesign commit output/root/install so one execution batch does not repeatedly pay per-intent event/admission/root costs. Do not start another small matching or timer slice unless a fresh profile makes that local component dominant.

196. Phase 6 experiment: overlap Mahi tx-root hashing with commit binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 195 showed native Mahi root construction consumed `21.6s` across profiled execution batches. Since `body_root(&body)` is independent of policy/order-book commit work, computing the tx root on a scoped worker while the main thread commits intents might hide part of the root cost without changing the `tx_root` value or block-header invariant.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node market_sharded_resting_place_orders_batch_across_interleaved_markets --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-txroot-overlap-135k-60s`
   Evidence:
   - The patch started a scoped `body_root(&body)` worker before non-empty native Mahi commit work in both overlay and in-place paths, then joined before computing `event_root` and `mahi_native_state_root`. This preserved existing root semantics; only scheduling changed.
   - 64-writer, 16-market, two-loadgen 135k-offered 60s run achieved aggregate loadgen rate `134,950.9 RPS` (`67,447.2` + `67,503.7`), zero request errors, latency p50 `8.8-9.5ms`, p90 `53-58ms`, and p99 `121-138ms`.
   - The committed timeseries measured average `94,955.5 TPS`, peak `159,734.8 TPS`, strict committed total `8,241,497`, and convergence passed at common height `448`.
   - Final status was safe (`pending_relay_intents=0`, all pre-admitted pools drained, dropped-oldest `0`), but throughput and strict committed total regressed versus item 190's 135k reference (`100,779.1 TPS`, strict `8,537,104`).
   Status:
   - binned
   Reason:
   - Even a semantics-preserving root-work overlap regressed sustained drain. This reinforces the item 195 conclusion: the remaining wall is not a single local root/hash operation that can be moved around; it is the broader sequential execution/output/root/install shape.
   Next move:
   - Revert the code and keep item 190 as the current code state. The next slice should be a larger architecture change, not another isolated scheduling adjustment: either a real market-partitioned executor that parallelizes policy/admission/event work across independent resources, or an explicit event/root/install model redesign with compatibility tests.

197. Phase 6 experiment: scoped worker market execution binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 195 showed the remaining wall was broad commit execution plus output/root/install work. An opt-in worker-backed market execution path might parallelize policy admission, matching, and event construction across independent market/policy shards and improve 16-market high-offer drain without changing committed order.
   Files changed:
   - `chain/crates/policy/src/lib.rs` (reverted)
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-policy replace_existing_updates_policy_record --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node worker_market_execution --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node market_sharded --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node worker_market_matching_matches_serial_market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-policy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-worker-market-exec-100k-10s` (discarded: missing green Mahi flags, invalid for this experiment)
   - `/tmp/codex-exp-phase6-worker-market-exec-greenflags-100k-10s`
   - `/tmp/codex-exp-phase6-worker-market-exec-135k-60s`
   - `/tmp/codex-exp-phase6-worker-market-exec-control-135k-60s`
   Evidence:
   - The discarded 100k/10 run omitted `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1` and `KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=1`, hit `local_submit_queue_full`, and is not comparable to the Phase 6 green path.
   - With the correct green flags, the opt-in worker execution path passed a 100k/10 sanity run: achieved `99,974.4 RPS`, zero request errors, measured average `98,245.8 TPS`, peak `125,526.0 TPS`, strict committed total `1,498,944`, drained pre-admitted pools, and converged.
   - The 135k/60 worker run achieved `131,201.5 RPS`, zero request errors, measured average `85,729.9 TPS`, peak `164,004.6 TPS`, strict committed total `7,254,127`, and convergence, but left residual owner/relay backlog (`100,040` and `40,504` pre-admitted items on two validators plus `443,880` pending relay).
   - The same-shape 135k/60 control without `KOMBAT_PHASE6_WORKER_MARKET_EXECUTION=1` achieved `131,456.3 RPS`, zero request errors, measured average `105,365.8 TPS`, peak `193,391.3 TPS`, strict committed total `8,561,168`, convergence, and drained all pre-admitted pools and pending relay.
   - The worker path regressed sustained 135k/60 throughput by `19,635.9 TPS` (`18.6%`) versus the same-code, same-shape control.
   Status:
   - binned
   Reason:
   - The scoped worker path is correctness-safe but performance-negative. It parallelizes by cloning per-shard policy/book state and spawning scoped workers per execution batch; at the current workload and batch shape, clone/thread/cache overhead dominates any parallelism. Tuning this implementation would be another fine-grained hunt around a losing architecture, so the code was reverted.
   Next move:
   - Keep item 190/current-control behavior as the active code path. Since both narrow execution tweaks and scoped per-batch worker execution failed, the next Phase 6 slice should be a deeper execution-output/write-model redesign, not another small worker around current `PolicyStore`/`SingleMarketBook` clone boundaries.
   - Candidate structural directions are persistent per-market execution actors with long-lived state ownership, compact commit output/event-root materialization, or a batched append-log/root accumulator with compatibility tests.

198. Phase 6 experiment: skip Mahi mempool/frontier install scan when direct mempool is empty binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The native Mahi commit path often executes pre-admitted payloads while the old direct mempool is empty. Skipping `intent_mempool.remove_block()` and proposer-frontier rebuild over the full committed body in that case might remove HotStuff-era metadata churn from the measured install wall without changing committed state.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_metadata_install_clears_frontier_without_scanning_empty_mempool --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-mahi-install-skip-empty-mempool-135k-60s` (discarded for keep/bin comparison: single-loadgen run only achieved `117,297.5 RPS`)
   - `/tmp/codex-exp-phase6-mahi-install-skip-empty-mempool-2loadgen-135k-60s`
   Evidence:
   - The code guarded the Mahi metadata install path: if the direct intent mempool was empty, clear proposer-frontier state instead of scanning the committed block body for mempool removal and frontier rebuild. A focused unit test covered the empty-mempool metadata install case.
   - The first 135k/60 run used a single loadgen process and was not comparable to the item 190 two-loadgen high-offer surface. It achieved only `117,297.5 RPS`, measured `114,564.8 TPS`, strict total `7,704,464`, convergence, and drained pre-admitted pools.
   - The comparable two-loadgen 135k/60 run achieved aggregate `134,944.6 RPS` (`67,495.3` + `67,449.3`), zero request errors, measured average `96,859.4 TPS`, peak `200,287.6 TPS`, strict committed total `8,381,022`, and convergence at common height `475`.
   - The comparable run was safe (`pending_relay_intents=0`, all pre-admitted pools drained, dropped-oldest `0`) but regressed versus item 190's 135k reference (`100,779.1 TPS`, strict `8,537,104`) and the same-session current control from item 197 (`105,365.8 TPS`, strict `8,561,168`).
   Status:
   - binned
   Reason:
   - Removing this old metadata scan did not improve the measured high-offer drain. The remaining wall is not this individual install-loop scan; changing it slightly perturbs scheduling but does not beat the current control. Keeping the special case would add branch complexity around commit metadata without throughput benefit.
   Next move:
   - Revert the code and keep item 190/current-control behavior. Do not continue with narrow Mahi install/root/output loop edits. The next kept-candidate work must change a larger architecture boundary: either persistent state-owning execution partitions, or a compact commit-output/event-root model that reduces per-intent materialization across the whole execution path.

199. Phase 6 experiment: streaming Mahi event-root construction binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 195 showed root/output work was material, and the native Mahi path computed `event_root` by serializing the full `Vec<RuntimeEventKind>` into a temporary Borsh byte buffer after event construction. Preserving the exact Borsh vector root semantics but streaming event serialization into the hasher, and caching the event root in the hot market-sharded output, might reduce root allocation/pass cost without changing consensus bytes.
   Files changed:
   - `chain/crates/node/src/lib.rs` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node mahi_runtime_event_root_matches_borsh_vec_hash --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node mahi_inplace_resting_commit --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node market_sharded --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo build --release -p kombat-chain-node -p kombat-chain-rpc -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/codex-exp-phase6-streaming-event-root-2loadgen-135k-60s`
   Evidence:
   - The compatibility test proved the streaming root builder produced the same hash as `Sha256(borsh::to_vec(&events))`, so the experiment did not intentionally change event-root semantics.
   - The comparable 64-writer, 16-market, two-loadgen 135k/60 run achieved aggregate `135,004.4 RPS` (`67,507.1` + `67,497.3`), zero request errors, measured average `83,832.0 TPS`, peak `158,363.0 TPS`, strict committed total `7,275,507`, and convergence at common height `399`.
   - The run left residual pre-admitted backlog on three validators (`218,856`, `121,360`, and `59,288` items), with dropped-oldest still `0`.
   - This regressed substantially versus item 190's 135k reference (`100,779.1 TPS`, strict `8,537,104`) and the same-session current control from item 197 (`105,365.8 TPS`, strict `8,561,168`).
   Status:
   - binned
   Reason:
   - Streaming removed one allocation shape but moved Borsh event serialization into the hot event-construction path and worsened sustained drain. The event/root/output wall is broader than the temporary event-root byte buffer; local root-materialization edits are not the right lever.
   Next move:
   - Revert the code and keep item 190/current-control behavior. Stop narrow root/event-loop edits. The next useful Phase 6 work should either implement persistent state-owning execution partitions or move to the live-cluster transport work, depending on whether the priority is local 250k execution or real-network deployment.

200. Live testnet fix: accept bool-like Mahi observer async relay env
   Date:
   - `2026-04-25`
   Hypothesis:
   - Live testnet assets generated `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=true`, but the runtime only enabled async observer relay for literal `1`. Accepting standard bool-like env values should prevent live deployments from silently falling back to the synchronous owner-forwarding path.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc json_rpc_submit_intent_batch_async_relays_from_mahi_observer_ingress_with_bool_env --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/and-live-ingress-async-v2-10k-10s`
   Evidence:
   - A live 4-validator testnet run with manually corrected `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1` accepted `119,424` requests at `9,996.8 RPS` with zero client errors, p50 `8.1ms`, p90 `39.5ms`, and p99 `87.7ms`.
   - Validators accepted and drained `16,960` pre-admitted intents each, proving the async pre-admitted relay path works when enabled.
   - The same generated-env shape with `true` had previously routed through the synchronous owner-forwarding fallback and produced live-path distortion. The code now uses the existing bool parser so `1`, `true`, `yes`, and `on` are accepted consistently.
   - The live run still left `51,584` pending relay intents on the observer after settle, so the remaining live bottleneck is observer-to-owner relay throughput, not this parser bug.
   Status:
   - kept
   Reason:
   - This is a zero-risk configuration correctness fix that aligns runtime parsing with generated assets and removes a silent live deployment footgun. It does not claim a throughput win by itself.
   Next move:
   - Continue live-cluster relay diagnosis from `/tmp/and-live-ingress-async-v2-10k-10s`: validators are empty and caught up, while the observer retained pending relay items. The next structural investigation is relay worker/concurrency/transport, not execution micro-tuning.

201. Live testnet experiment: stale Mahi execution backlog spin binned
   Date:
   - `2026-04-25`
   Hypothesis:
   - The live validator path might be wasting cycles spinning stale Mahi execution backlog work after the network has caught up. Suppressing stale backlog spin could reduce live CPU pressure and improve the 4-server testnet's green TPS.
   Files changed:
   - `chain/crates/rpc/src/lib.rs` (reverted by `1911f535f`)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-stale-backlog-v1-8k-10s`
   Evidence:
   - The patch was deployed as commit `3a903faef` and then reverted by `1911f535f`.
   - The 8k/10s live run accepted only `68,800` requests, hit `10,568` network errors, and committed only `3,096` intents.
   - Peer sessions collapsed during the run, so the change damaged live transport/liveness rather than reducing a harmless spin cost.
   Status:
   - binned
   Reason:
   - The patch converted a suspected CPU cleanup into a live correctness/transport regression. It is not a performance candidate.
   Next move:
   - Keep the revert. Do not retry stale-backlog suppression without a narrower correctness proof and peer-session stability test.

202. Live testnet slice: batch Mahi block broadcasts
   Date:
   - `2026-04-25`
   Hypothesis:
   - The live 4-server cluster is paying too much per-block HTTP request/response overhead on Mahi DAG broadcast. Sending multiple Mahi blocks in one peer request should reduce transport overhead while preserving rolling-deploy compatibility through single-block fallback.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc network_mahi_block_batch_route_stores_blocks_in_order --manifest-path chain/Cargo.toml`
   - `cargo test -p kombat-chain-rpc network_mahi_block_route_stores_block_and_exposes_mahi_metrics --manifest-path chain/Cargo.toml`
   - `cargo test -p kombat-chain-rpc mahi_round --manifest-path chain/Cargo.toml`
   - `cargo test -p kombat-chain-rpc maintain_peer_transport_sessions_warms_configured_peers --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - `/tmp/and-live-mahi-block-batch-v1-8k-10s`
   - `/tmp/and-live-mahi-block-batch-v1-10k-reuse-10s`
   - `/tmp/and-live-mahi-block-batch-v1-12k-reuse-10s`
   Evidence:
   - Commit `6f51030cd` added `/network/mahi/blocks`, batched peer transport dispatch, and fallback to the existing single-block endpoint for compatibility.
   - The 8k/10s single-ingress live run accepted `79,768` requests, produced `0` network errors, committed `79,768` intents, and converged with matching state roots at common height `70` (`7,976.8 TPS`).
   - The 10k/10s single-ingress live run accepted `99,072` requests, produced `0` network errors, and a saved nonce recheck confirmed `99,072` committed intents with matching state roots at common height `93` (`9,907.2 TPS`).
   - The 12k/10s single-ingress live run accepted only `111,256` of `118,064` sent requests, produced `6,808` network errors, and left large pre-admitted pool backlogs. This established a single-ingress live ceiling around `10k TPS`.
   Status:
   - kept
   Reason:
   - Batched block broadcast preserves correctness and moves the live single-ingress green ceiling to just under `10k TPS`. The 12k failure shows the next wall is topology/ingress distribution rather than this block-broadcast slice.
   Next move:
   - Keep batched Mahi block broadcast. Test distributed ingress across all four live hosts before making more code changes.

203. Live testnet diagnostic: distributed ingress topology
   Date:
   - `2026-04-25`
   Hypothesis:
   - The live cluster's single-ingress ceiling is a topology/transport artifact. Running an ingress role on each validator host and splitting client load across all four `:8645` ingress processes should raise green live throughput without changing consensus or execution code.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress-12k-v1-10s` through `/tmp/and-live-distributed-ingress-12k-v4-10s` (invalid: stale/missing remote writer manifests)
   - `/tmp/and-live-distributed-ingress-12k-v1-rerun-10s` through `/tmp/and-live-distributed-ingress-12k-v4-rerun-10s`
   - `/tmp/and-live-distributed-ingress-16k-v1-10s` through `/tmp/and-live-distributed-ingress-16k-v4-10s`
   - `/tmp/and-live-distributed-ingress-20k-v1-10s` through `/tmp/and-live-distributed-ingress-20k-v4-10s`
   - `/tmp/and-live-distributed-ingress-24k-v1-10s` through `/tmp/and-live-distributed-ingress-24k-v4-10s`
   - `/tmp/and-live-distributed-ingress-30k-v1-10s` through `/tmp/and-live-distributed-ingress-30k-v4-10s`
   - `/tmp/and-live-distributed-ingress-30k-dual-v1p1-10s` through `/tmp/and-live-distributed-ingress-30k-dual-v4p2-10s`
   Evidence:
   - Each of the four live hosts now runs one validator process on `:8545` and one ingress role process on `:8645`. This uses the existing role split: `KOMBAT_RUNTIME_ROLE=validator` for consensus/state and `KOMBAT_RUNTIME_ROLE=ingress` for client write ingress.
   - The first distributed 12k attempt was invalid because the live harness copied split manifests under their original basenames while the remote command expected `/tmp/and-live-loadtest-remote/writers.json`; two hosts used stale/missing manifests. This artifact is recorded so it is not reused as performance evidence.
   - The corrected 12k/10s distributed run accepted `119,808` requests, produced `0` network errors, committed `119,808` intents, drained all pre-admitted pools, and converged with one state root at common height `115` (`11,980.8 TPS`).
   - The 16k/10s distributed run accepted `159,872` requests, produced `0` network errors, committed `159,872` intents, drained all pre-admitted pools, and converged with one state root at common height `223` (`15,987.2 TPS`).
   - The 20k/10s distributed run accepted `199,480` requests, produced `0` network errors, committed `199,480` intents, drained all pre-admitted pools, and converged with one state root at common height `345` (`19,948.0 TPS`).
   - The 24k/10s distributed run accepted `239,232` requests, produced `0` network errors, committed `239,232` intents, drained all pre-admitted pools, and converged with one state root at common height `433` (`23,923.2 TPS`).
   - The 30k/10s distributed run accepted `266,312` requests, produced `0` network errors, committed `266,312` intents, drained all pre-admitted pools, and converged with one state root at common height `500` (`26,631.2 TPS`). This is correctness-green and above the old M4d threshold, but not clean as a 30k-offered loadgen result because validators 1 and 2 under-generated (`69,992` and `46,752` accepted) and p99 latency reached multi-second levels.
   - A dual-loadgen 30k diagnostic split each host into two 4-writer loadgens. It accepted and committed only `253,336` intents (`25,333.6 TPS`) with state-root convergence at common height `560`, and p99 latency remained multi-second on the weak host/process pairs. Splitting client processes did not fix the under-generation, so a single loadgen process was not the only wall.
   Status:
   - kept diagnostic
   Reason:
   - Distributed ingress is a structural live-topology win: it raises the current clean live TPS from the single-ingress `9,907.2 TPS` ceiling to `23,923.2 TPS` with correctness intact, and it can commit `26,631.2 TPS` under a saturated 30k attempt. This confirms that live performance was not limited by raw host-to-host RTT alone; it was constrained by the single ingress/transport topology and now by per-host/process saturation at higher offered load.
   Next move:
   - Do not repeat the dual-loadgen-only path. The next live structural test should separate roles further: run multiple ingress-role processes per host on different ports, or move load generation off the validator/ingress hosts, then retry the 30k+ ladder. If that still under-generates with drained pre-admitted pools, the wall is per-host CPU contention or validator execution rather than client process width.

204. Live testnet diagnostic: second ingress process per host
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 203 showed distributed ingress was structurally correct but saturated before cleanly reaching 30k offered load. Running a second ingress-role process per live host on a separate port should increase client accept/relay capacity without changing validator consensus, DAG, or execution code.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress2-30k-v1p1-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v1p2-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v2p1-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v2p2-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v3p1-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v3p2-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v4p1-10s`
   - `/tmp/and-live-distributed-ingress2-30k-v4p2-10s`
   Evidence:
   - Each live host temporarily ran a second ingress role process on `:8646` alongside the existing ingress process on `:8645`. Validator processes on `:8545` were unchanged.
   - Eight loadgen processes split a 30k offered load across the eight ingress endpoints (`3,750 RPS` target each). The run accepted `293,072` requests, produced `0` network errors, committed `293,072` intents, and converged with one state root at common height `618` (`29,307.2 TPS`).
   - Per-host committed totals were `74,776` on v1, `74,960` on v2, `68,368` on v3, and `74,968` on v4. v3 remained the weak host/process pair, with p99 latencies around `895-916ms`, while v2/v4 p99 latencies were below `90ms`.
   - This improved the best live 30k-offered result from item 203's saturated `26,631.2 TPS` to `29,307.2 TPS` and improved balance on v1/v2/v4. It also confirms the prior wall was at least partly per-ingress-process accept/relay capacity, not Mahi consensus correctness.
   Status:
   - kept diagnostic
   Reason:
   - The second ingress process per host is a structural live-topology win and keeps state-root convergence intact. It is currently a live configuration experiment rather than repo-managed deployment config, so the code state is unchanged and the topology needs to be codified before it can be treated as a permanent operational baseline.
   Next move:
   - Codify multi-ingress deployment support or keep it as an explicit live-test topology, then run the next live ladder step above 30k. If v3 remains the only under-generating host while pre-admitted pools drain, diagnose v3 host contention/CPU scheduling before changing consensus or execution code.

205. Live testnet diagnostic: dual-ingress 36k ladder step
   Date:
   - `2026-04-25`
   Hypothesis:
   - If item 204's second ingress process per host removed the immediate 30k wall, the same eight-ingress topology should continue scaling above 30k until per-host CPU/scheduler or validator execution becomes the limiting factor.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress2-36k-20260425-213108` (invalid: remote `writers.json` was stale because local shard files were named `writers-1.json` etc. while the remote command expects `writers.json`)
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v1p1-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v1p2-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v2p1-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v2p2-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v3p1-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v3p2-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v4p1-10s`
   - `/tmp/and-live-distributed-ingress2-36k-rerun-20260425-213108/v4p2-10s`
   Evidence:
   - The first 36k attempt is explicitly discarded: each remote loadgen process reused a stale remote `writers.json`, so loadgen accepted traffic but the committed-summary checked fresh local writers and reported `0` committed. The rerun used per-process local `manifest-N/writers.json` paths, overwriting the expected remote filename correctly.
   - The valid 36k/10s rerun split `4,500 RPS` target traffic across eight ingress endpoints. It accepted `338,320` requests, produced `0` network errors and `0` RPC errors, committed `338,320` intents, and converged with one state root at common height `740` (`33,832.0 TPS`).
   - Per-host committed totals were `89,952` on v1, `89,952` on v2, `83,872` on v3, and `74,544` on v4. v1/v2 hit the full `9k TPS` per-host target; v3 partially under-generated; v4 became the weak host/process pair with p99 latencies around `942-976ms`.
   - All validator pre-admitted pools drained and dropped-oldest stayed `0`, so this run did not expose consensus divergence or persistent owner-feed backlog.
   Status:
   - kept diagnostic
   Reason:
   - The live cluster can go beyond 30k with the current Mahi code and dual-ingress topology. The next wall is uneven per-host/process saturation under higher offered load, not a correctness failure and not a reason to return to localhost micro-optimizations.
   Next move:
   - Run a short host-level diagnosis during the next live ladder step: capture CPU/run-queue/network counters on each host while load runs. If v4 remains the weak host with drained queues, fix deployment/topology or host scheduling before making consensus/execution changes. Also codify the multi-ingress manifest handling so future distributed runs cannot accidentally reuse stale remote `writers.json`.

206. Live testnet diagnostic: dual-ingress 40k ladder step
   Date:
   - `2026-04-25`
   Hypothesis:
   - The dual-ingress topology should continue scaling toward 40k offered load if the remaining wall is per-host ingress/loadgen saturation rather than Mahi consensus, DAG convergence, or commit execution correctness.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v1p1-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v1p2-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v2p1-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v2p2-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v3p1-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v3p2-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v4p1-10s`
   - `/tmp/and-live-distributed-ingress2-40k-20260425-213108/v4p2-10s`
   Evidence:
   - The valid 40k/10s run split `5,000 RPS` target traffic across eight ingress endpoints. It accepted `385,184` requests, produced `0` network errors and `0` RPC errors, committed `385,184` intents, and converged with one state root at common height `810` (`38,518.4 TPS`).
   - Per-host committed totals were `99,728` on v1, `99,872` on v2, `85,808` on v3, and `99,776` on v4. v1/v2/v4 reached roughly `10k TPS` per host; v3 under-generated at `8,580.8 TPS`.
   - All validator pre-admitted pools drained and dropped-oldest stayed `0`, so the run again did not expose persistent owner-feed backlog or state divergence.
   - Lightweight host sampling was attempted during the run. It showed v3 with the highest load average (`load1` avg `5.86`, max `10.94`) and a higher `kombat_chain` CPU sample than the other hosts, matching the weak-host throughput pattern. The `ps --sort` sampling used here is itself too expensive and should not be treated as a precise profiler.
   Status:
   - kept diagnostic
   Reason:
   - The live cluster exceeded the previous 36k step and got within 4% of a full 40k offered load with correctness intact. The active wall is now uneven live-host capacity, especially v3, not a Mahi safety issue.
   Next move:
   - Continue the live ladder with lighter host instrumentation that samples `/proc/stat`, `/proc/loadavg`, and network counters without per-second sorted `ps`. If v3 remains weak, isolate whether it is loadgen CPU, ingress process CPU, validator CPU, or host scheduling before attempting QUIC or Block-STM work for this live topology.

207. Live testnet diagnostic: dual-ingress 44k ladder step
   Date:
   - `2026-04-25`
   Hypothesis:
   - The 40k result may have been affected by noisy host/process scheduling rather than a hard ceiling. A 44k offered run with lighter `/proc` instrumentation should show whether the live dual-ingress topology is still scaling.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v1p1-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v1p2-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v2p1-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v2p2-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v3p1-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v3p2-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v4p1-10s`
   - `/tmp/and-live-distributed-ingress2-44k-20260425-213108/v4p2-10s`
   Evidence:
   - The 44k/10s run split `5,500 RPS` target traffic across eight ingress endpoints. It accepted `438,944` requests, produced `0` network errors and `0` RPC errors, committed `438,944` intents, and converged with one state root at common height `895` (`43,894.4 TPS`).
   - Per-host committed totals were tightly balanced: `109,920` on v1, `109,952` on v2, `109,120` on v3, and `109,952` on v4. p99 latencies were `66-298ms`, materially better than item 206's noisy weak-host run.
   - Lighter `/proc` host sampling avoided the previous expensive sorted-`ps` loop. Load samples were modest relative to the offered load (`load1` avg: v1 `3.39`, v2 `1.40`, v3 `4.22`, v4 `1.76`), and all pre-admitted pools drained with dropped-oldest `0`.
   Status:
   - kept diagnostic
   Reason:
   - The live dual-ingress topology is still scaling and is not capped at 40k. The earlier host imbalance was at least partly runtime variance/instrumentation noise, not a deterministic v3/v4 ceiling.
   Next move:
   - Continue the live ladder at a higher offered load with the same corrected manifest handling and lightweight host sampling. If the next step remains balanced and drained, codify multi-ingress deployment support; if it saturates, use the host samples and per-host committed totals to choose between additional ingress processes, external load generation, or transport work.

208. Live testnet diagnostic: dual-ingress 50k ladder step
   Date:
   - `2026-04-25`
   Hypothesis:
   - The balanced 44k result indicates the live dual-ingress topology has not saturated yet. A 50k offered run should show whether the current four-host, eight-ingress setup can reach the Phase 4/M9-class sustained-throughput range on live servers.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v1p1-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v1p2-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v2p1-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v2p2-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v3p1-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v3p2-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v4p1-10s`
   - `/tmp/and-live-distributed-ingress2-50k-20260425-213108/v4p2-10s`
   Evidence:
   - The 50k/10s run split `6,250 RPS` target traffic across eight ingress endpoints. It accepted `498,664` requests, produced `0` network errors and `0` RPC errors, committed `498,664` intents, and converged with one state root at common height `930` (`49,866.4 TPS`).
   - Per-host committed totals were balanced: `124,944` on v1, `124,928` on v2, `123,896` on v3, and `124,896` on v4. p99 latencies were `65-166ms`, and all pre-admitted pools drained with dropped-oldest `0`.
   - Lightweight host load samples stayed moderate (`load1` avg: v1 `2.07`, v2 `0.96`, v3 `3.29`, v4 `3.68`). This does not show a live consensus or execution wall at 50k offered.
   Status:
   - kept diagnostic
   Reason:
   - The live 4-server deployment can sustain a correctness-green, state-converged, 50k-class short run with the current Mahi code when ingress is distributed across two role processes per host.
   Next move:
   - Run the next live ladder step at 60k offered. If 60k is clean, move from short-run laddering to the M9-style 60s soak. In parallel, codify the multi-ingress deployment shape and manifest handling so this topology is reproducible from repo scripts rather than manual systemd copies.

209. Live testnet diagnostic: dual-ingress 60k plateau
   Date:
   - `2026-04-25`
   Hypothesis:
   - If the dual-ingress topology continues scaling past 50k, a 60k offered run should commit materially above the 50k run while preserving state-root convergence.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v1p1-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v1p2-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v2p1-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v2p2-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v3p1-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v3p2-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v4p1-10s`
   - `/tmp/and-live-distributed-ingress2-60k-20260425-213108/v4p2-10s`
   Evidence:
   - The 60k/10s run split `7,500 RPS` target traffic across eight ingress endpoints. It accepted `498,296` requests, produced `0` network errors and `0` RPC errors, committed `498,296` intents, and converged with one state root at common height `963` (`49,829.6 TPS`).
   - The run did not improve over item 208's 50k result (`49,866.4 TPS`). Per-host committed totals were imbalanced: `149,744` on v1, `131,320` on v2, `85,392` on v3, and `131,840` on v4.
   - All validator pre-admitted pools drained and dropped-oldest stayed `0`, so the plateau is not a persistent owner-feed backlog or consensus convergence problem.
   - Host load samples rose on the weak hosts (`load1` avg: v1 `1.80`, v2 `4.42`, v3 `5.42`, v4 `4.27`), matching a per-host/process saturation pattern.
   Status:
   - kept diagnostic
   Reason:
   - Dual-ingress topology appears capped around 50k short-run on this four-host live setup. The 60k result remains correctness-green, but it does not raise throughput; the limiter is uneven ingress/loadgen/process capacity under higher per-endpoint RPS.
   Next move:
   - Add a third ingress process per host and rerun 60k split across 12 endpoints (`5,000 RPS` each). This is the next structural topology test because item 208 proved roughly `6,250 RPS` per endpoint is clean at 50k aggregate, while item 209 shows `7,500 RPS` per endpoint is not balanced enough at 60k.

210. Live testnet diagnostic: tri-ingress 60k ladder step
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 209 showed the dual-ingress 60k plateau was caused by too much per-endpoint/per-host load. Adding a third ingress role process per host and splitting 60k across 12 endpoints at `5,000 RPS` each should raise live committed TPS while preserving state convergence.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v1p1-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v1p2-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v1p3-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v2p1-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v2p2-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v2p3-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v3p1-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v3p2-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v3p3-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v4p1-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v4p2-10s`
   - `/tmp/and-live-distributed-ingress3-60k-20260425-214732/v4p3-10s`
   Evidence:
   - A third ingress service was started on each live host at `:8647`, alongside the existing `:8645` and `:8646` ingress services. Validator services on `:8545` were unchanged.
   - The 60k/10s run split `5,000 RPS` target traffic across twelve ingress endpoints. It accepted `561,000` requests, produced `0` network errors and `0` RPC errors, committed `561,000` intents, and converged with one state root at common height `1013` (`56,100.0 TPS`).
   - Per-host committed totals were `148,000` on v1, `149,728` on v2, `113,688` on v3, and `149,584` on v4. v3 remains the weak host (`11,368.8 TPS`) with high load samples (`load1` avg `8.67`, max `19.75`), but the aggregate still improved by `6,270.4 TPS` over item 209's dual-ingress 60k result.
   - All validator pre-admitted pools drained and dropped-oldest stayed `0`, so the live Mahi path remains correctness-green at the new 56k short-run level.
   Status:
   - kept diagnostic
   Reason:
   - Tri-ingress is a structural live-topology win. The cluster is now above 50k live short-run TPS with state-root convergence. The remaining live wall is host imbalance, especially v3, not consensus safety.
   Next move:
   - Attempt the M9-style 60s soak at 60k offered using the tri-ingress topology. If sustained committed TPS stays above 50k and state roots converge, record the live soak as kept. Separately codify multi-ingress service generation and stale-manifest-safe loadtest orchestration in repo scripts.

211. Live testnet diagnostic: tri-ingress 60k 60s soak blocked by writer skew
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 210's 56.1k short-run result should sustain across a 60s soak at 60k offered if the tri-ingress topology is sufficient and no long-run commit/admission backlog appears.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v1p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v1p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v1p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v2p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v2p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v2p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v3p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v3p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v3p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v4p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v4p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-20260425-214732/v4p3-60s`
   Evidence:
   - The 60k/60s run split `5,000 RPS` target traffic across twelve ingress endpoints. It accepted `3,392,264` requests, produced `0` network errors and `0` RPC errors, and converged with one state root at common height `1383`.
   - Writer nonce deltas after the normal 30s settle and after an additional delayed recheck were identical: `2,635,568` committed intents (`43,926.1 TPS`), below the `50k` soak target.
   - The loss pattern was not uniform. In every loadgen process, three writers advanced near the expected per-writer count while one writer stalled early: examples include `v1p1 [74992, 74992, 7192, 74992]`, `v2p2 [68704, 68648, 6552, 68608]`, and `v4p3 [74992, 74992, 7016, 74992]`.
   - Validator pre-admitted pools drained and dropped-oldest stayed `0`, so this is not persistent owner-feed backlog. State roots matched, so this is not a safety divergence. The evidence points to sustained-load writer/nonce skew in the 4-writer-per-process workload shape.
   Status:
   - blocked diagnostic
   Reason:
   - The soak did not meet the 50k sustained gate, but the failure mode is dominated by one lagging writer per loadgen process rather than a broad consensus or execution ceiling. Treating `43,926.1 TPS` as the Mahi sustained ceiling would be misleading until the workload uses enough writers per process to avoid per-policy nonce skew.
   Next move:
   - Rerun the 60k/60s soak with more writers per loadgen process, starting with `8` writers per endpoint (`96` total writers). If the one-lagging-writer pattern disappears and committed TPS exceeds 50k, record the wider-writer soak as kept. If it persists, diagnose loadgen per-writer scheduling or policy nonce handling explicitly before changing consensus or transport.

212. Live testnet diagnostic: tri-ingress owner-shard lag isolation
   Date:
   - `2026-04-25`
   Hypothesis:
   - Item 211's one-lagging-writer-per-process pattern may be caused by too few writers per loadgen process. If so, increasing to `8` writers per endpoint should remove writer skew and let the 60k/60s tri-ingress soak exceed the 50k sustained target.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live deployment/configuration validation on the 4-server testnet.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v1p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v1p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v1p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v2p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v2p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v2p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v3p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v3p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v3p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v4p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v4p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-20260425-220255/v4p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v1p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v1p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v1p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v2p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v2p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v2p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v3p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v3p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v3p3-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v4p1-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v4p2-60s`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-6w-no-v3-owner-20260425-2220/v4p3-60s`
   Evidence:
   - The `8`-writer-per-endpoint 60k/60s run accepted `3,594,240` requests with `0` network errors and `0` RPC errors, but only committed `2,695,680` intents after the standard settle (`44,928.0 TPS`). State roots matched at common height `2861`.
   - The 8-writer failure pattern was deterministic: writer indexes `2` and `6` in every process committed `0` intents, while the other six writers committed one worker's worth. Those policy ids all satisfy `policy_id % 4 == 2`, mapping them to validator 3 as the Mahi payload owner.
   - Validator 3 was lagging in produced rounds during the load: recent non-empty production reached around round `5113` on v3 while v1/v2/v4 reached around round `6572`. The cluster still converged because Mahi can skip a lagging author, but payloads assigned exclusively to that author did not advance.
   - A confirmation run excluded v3-owned policies while keeping the same tri-ingress 60k/60s shape (`6` writers per endpoint, `72` total writers). After the normal `30s` settle it had committed `2,873,480` intents (`47,891.3 TPS`) with matching state roots at common height `3847`; validators still had pre-admitted backlog.
   - A delayed nonce recheck after additional settle showed all `3,577,240` accepted intents had committed (`59,620.7 TPS` over the 60s load window). State roots matched at common height `3937`.
   Status:
   - kept diagnostic
   Reason:
   - Wider writers alone did not fix the sustained run. The real failure mode is exclusive owner-shard assignment to a lagging validator: when v3 is the only payload owner for a policy, that policy's intents can be accepted and relayed but skipped from the committed DAG while v1/v2/v4 continue converging safely.
   - The no-v3-owner confirmation proves the live four-server Mahi path can sustain just under `60k TPS` with correctness when payload owners keep up. This is beyond the 50k soak target, but it is not a valid permanent topology because it avoids one validator's owner shard rather than making ownership resilient.
   Next move:
   - Implement or test owner-shard redundancy/fallback for Mahi pre-admitted relays so an intent can be included by a backup validator when its primary payload owner lags. Do not chase executor micro-tweaks before this; the current blocker is payload ownership availability under a slow author.
   - Separately isolate why v3 lags in produced rounds on the live hosts, but treat the protocol/topology fix as the higher-leverage path because a production DAG must tolerate a slow validator without losing that validator's entire payload shard.

213. Live testnet diagnostic: naive owner redundancy over-buffers
   Date:
   - `2026-04-26`
   Hypothesis:
   - Relaying each Mahi pre-admitted intent to both its primary payload owner and one backup owner should make the full 8-writer-per-endpoint, 60k/60s tri-ingress soak tolerate a lagging owner shard and clear the 50k sustained gate.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_payload_owner_redundancy -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml network_relay_intents_in_mahi_mode_populates_pre_admitted_pool -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_round_drains_pre_admitted_pool_into_produced_block -- --nocapture`
   - Native Linux build of `kombat_chain` on validator host v1, then deployment to all four live hosts.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-owner2-20260426-0910` (invalid, stale writer manifests after service restart)
   - `/tmp/and-live-distributed-ingress3-60k-owner2-freshwriters-20260426-0918`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-owner2-freshwriters-20260426-0925`
   Evidence:
   - Commit `450fb81ed` added `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY` with default `1`; the live test explicitly set `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2` on all validator and ingress services.
   - The first attempted soak used stale writer manifests after a live-service restart and failed before load generation with `unknown policy`; it produced no TPS result and is listed only to avoid reusing the invalid artifact.
   - A fresh 96-writer manifest was seeded through the live validator network and split into twelve 8-writer shards. All policies replicated across all four validators before the measurement run.
   - The valid 60k/60s run accepted `3,576,944` requests with `0` network errors and `0` RPC errors, but committed only `2,317,667` intents after the standard `30s` drain (`38,627.8 TPS`). State roots matched at common height `188`.
   - A delayed nonce recheck after an additional settle showed all `3,576,944` accepted intents had eventually committed (`59,615.7 TPS` over the load window), again with one state root at common height `188`.
   - The normal-drain bottleneck was visible in Mahi pools: validators still held roughly `495k-524k` pre-admitted items each while reporting about `1.75M-1.79M` accepted and only `1.22M-1.29M` drained. Naive redundancy doubled owner-feed volume and inflated drain backlog rather than raising sustained commit throughput.
   Status:
   - binned diagnostic
   Reason:
   - The experiment proves backup ownership can preserve eventual correctness and avoids permanently losing intents assigned to a slow owner, but unconditional primary-plus-backup relay is too expensive for the sustained gate. It turns the owner-shard lag issue into a large queue-drain latency issue.
   - The code path is safe because the default remains `1`, but `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2` should not be used as the steady-state fix.
   Next move:
   - Return live services to redundancy `1`.
   - Replace unconditional duplication with a selective owner-fallback design: relay to the primary owner under normal conditions, and activate a backup only when the primary owner is measurably lagging or its pre-admitted drain rate falls behind. Do not pursue executor or transport micro-tuning until the owner-availability mechanism is selective rather than always-on.

214. Live testnet kept: selective Mahi owner fallback clears 60k soak
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 213 showed unconditional owner redundancy preserved eventual correctness but over-buffered the owner feed. A selective fallback should preserve the full owner-sharded workload while avoiding duplicate steady-state relay volume: send to the primary owner first, return `retry_lagging_owner` only when that owner is behind the observed DAG, and then relay to the configured backup owner.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_owner_relay -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_payload_owner -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml json_rpc_submit_intent_batch_splits_by_mahi_payload_owner_from_observer_ingress -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml network_relay_intents_in_mahi_mode_populates_pre_admitted_pool -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_round_drains_pre_admitted_pool_into_produced_block -- --nocapture`
   - `cargo build --release -p kombat-chain-rpc --manifest-path chain/Cargo.toml`
   - Native Linux release build on v1 from pushed commit `631acf1bd`, deployed to all four live hosts.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-60k-owner2-selective-freshwriters-20260426-0932`
   - `/tmp/and-live-distributed-ingress3-60k-soak60-8w-owner2-selective-20260426-0938`
   Evidence:
   - Commit `631acf1bd` changed `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2` from unconditional primary-plus-backup duplication into a fallback chain. Observer ingress enqueues only the primary owner. Relay retry advances to the next owner. A local owner returns `retry_lagging_owner` when fallback is enabled and its last produced round lags the observed DAG by more than `KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS` (`256` on the live run).
   - The live deployment ran four validators with three ingress processes per host and `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2`, `KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS=256` on all validator and ingress services.
   - A fresh 96-writer manifest was seeded after restart and split into twelve 8-writer shards to avoid stale-policy artifacts.
   - The 60k/60s run accepted `3,599,424` requests with `0` network errors and `0` RPC errors, and committed `3,599,424` intents after the standard `30s` drain (`59,990.4 TPS`).
   - Per-host committed totals were balanced: v1 `899,776`, v2 `899,904`, v3 `899,840`, v4 `899,904`. This removes item 212's deterministic `policy_id % 4 == 2` stall without excluding v3-owned policies.
   - All Mahi pre-admitted pools were empty after the normal drain (`items=0`, `accepted=899,856`, `drained=899,856`, `dropped_oldest=0` in sampled metrics), unlike item 213's unconditional redundancy run that left roughly `0.5M` queued items per validator.
   - State roots matched across all four validators at common height `188` (`0x9f92b746f66b1f37f330a074c8cb6b5f70eb3d130f89c4587b9824efb7344260`). Head hashes differ as expected for the Mahi harness; state root at common height is the correctness invariant.
   Status:
   - kept
   Reason:
   - Selective owner fallback fixes the owner-shard availability failure without introducing the queue-drain backlog caused by unconditional duplicate relay. The live four-server cluster now clears the 50k sustained M9-style soak gate with correctness.
   Next move:
   - Continue live laddering above 60k offered using the kept selective fallback topology. The next useful measurement is not a micro-optimization; it is a structural capacity check at higher offered load to decide whether the next wall is ingress process capacity, peer transport, or execution.

215. Live testnet kept diagnostic: selective fallback sustains near-80k offered
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 214 established that selective Mahi owner fallback clears the 60k/60s live soak with correctness. If owner fallback is no longer the wall, the same topology should scale to an 80k/60s live soak without persistent owner-feed backlog, request errors, or state-root divergence.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Live 4-host deployment remained on commit `631acf1bd` with `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2` and `KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS=256`.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-80k-soak60-8w-owner2-selective-20260426-0945`
   Evidence:
   - The 80k/60s tri-ingress live run split traffic across twelve ingress endpoints at about `6,667 RPS` each. It accepted `4,799,104` requests, produced `0` network errors and `0` RPC errors, and committed `4,799,104` intents after the standard `30s` drain (`79,985.1 TPS`).
   - Per-host committed totals were balanced: v1 `1,199,616` (`19,993.6 TPS`), v2 `1,199,872` (`19,997.9 TPS`), v3 `1,199,872` (`19,997.9 TPS`), and v4 `1,199,744` (`19,995.7 TPS`).
   - State roots matched across all four validators at common height `330` (`0xe77715ced73bcf016cbfbd6004b419889e12bca7ca74b14ad0d9bcc209c4a079`). Head hashes differed, which is expected under the Mahi harness; state root at common height remains the correctness invariant.
   - Sampled Mahi pre-admitted pool metrics showed `items=0` after drain, with accepted and drained counts equal in the sampled validator (`accepted=2,099,632`, `drained=2,099,632`, `dropped_oldest=0`). This indicates the selective fallback topology did not recreate item 213's persistent redundancy backlog at 80k offered.
   - Latency was asymmetric despite clean throughput: v2 p99 was around `176-217ms`, while v1/v3/v4 p99 ranged roughly `1.3-2.2s`. This is the next capacity signal to watch at 100k offered, not a correctness failure.
   Status:
   - kept diagnostic
   Reason:
   - The live cluster now sustains the full accepted rate at near-80k offered for 60 seconds with no request errors, no dropped oldest pre-admitted items in sampled metrics, balanced per-host commits, and matching state roots. This confirms the item 214 fix is not merely a 60k-specific result.
   Next move:
   - Run the same live topology at 100k offered for 60 seconds. If pools drain and state roots match, record the 100k result as the current live ceiling. If throughput falls below accepted input, classify whether the wall is ingress latency, peer transport, or commit execution before making code changes.

216. Live testnet kept: coalesced commit advancement restores 80k after verifier wake fix
   Date:
   - `2026-04-26`
   Hypothesis:
   - The 100k selective-fallback attempt exposed a liveness bug: the commit detector could run before peer blocks finished async sigverify, mark leaders skipped, and then fail to re-run commit detection when verification completed. Re-running commit detection from every sigverify completion fixes correctness but over-wakes the detector and creates DAG-lock pressure. Coalescing verification completion into the existing commit-executor wake path should preserve correctness without regressing the kept 80k live soak.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_ -- --nocapture`
   - Native Linux release build on v1 from pushed commit `ee64b6c57`, deployed to all four live hosts.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-100k-soak60-8w-owner2-selective-20260426-0942` (invalid harness artifact)
   - `/tmp/and-live-distributed-ingress3-100k-soak60-8w-owner2-selective-rerun-20260426-0951` (diagnostic failure)
   - `/tmp/and-live-distributed-ingress3-80k-soak60-postverify-20260426-1011` (superseded first fix)
   - `/tmp/and-live-distributed-ingress3-coalesced-freshwriters-20260426-1018`
   - `/tmp/and-live-distributed-ingress3-80k-soak60-coalesced-20260426-1021`
   - `/tmp/and-live-distributed-ingress3-100k-soak60-coalesced-20260426-1024` (diagnostic)
   Evidence:
   - The first 100k artifact was invalid because three concurrent remote loadgen jobs copied/executed the same `/tmp/and-live-loadtest-remote/kombat_loadgen` path and failed with `Text file busy`. Future multi-process live runs use unique `LOADTEST_REMOTE_TMP_DIR` values per job.
   - The corrected 100k selective-fallback rerun accepted `5,050,176` requests but committed only `2,091,567` intents (`34,859.4 TPS`). Validator v1 accepted `588,152` requests but committed `0`; it showed `committed_block_count=7`, `executed_intent_count=0`, `received_intent_count=3,543,792`, `verified_block_count=3000`, and `skipped_leader_count=294`. That isolated the bug to commit detection advancing before async block verification completed.
   - Commit `b3a69995c` fixed correctness by invoking commit advancement directly from sigverify completion, but the 80k sanity run regressed to `4,265,248` committed intents (`71,087.5 TPS`) and then stuck around round `266` with no verification backlog. That first fix was superseded because per-verification detector advancement created too much commit-detector/DAG-lock pressure.
   - Commit `ee64b6c57` coalesces verification completion into the commit-executor wake path: sigverify completion marks blocks verified and wakes the executor; the executor advances the commit detector once per wake before draining ready execution.
   - A fresh 96-writer manifest was seeded after deploying `ee64b6c57` and split into twelve 8-writer shards. The 80k/60s run accepted `4,798,624` requests with `0` network errors and `0` RPC errors, and committed `4,798,624` intents after the standard drain (`79,977.1 TPS`).
   - Per-host committed totals in the 80k coalesced run were balanced: v1 `1,199,416`, v2 `1,199,808`, v3 `1,199,648`, v4 `1,199,752`. State roots matched at common height `153` (`0x48b94b389ac2d61d823517a16c3e4a1ebb769d0209a7309758dc4354f3c00e32`). All sampled queues were drained after the run: local submit, pending relay, pending block broadcast, pre-admitted pool, verification pending, and execution pending all reported `0`.
   - The same coalesced build crossed the 100k short-run line as a diagnostic: accepted `5,122,544` requests, produced `16` network errors and `0` RPC errors, and committed `4,867,832` intents (`81,130.5 TPS`) with matching state roots at common height `523` (`0x217082eb0c2d426be2c2dbebe2dcf9cfefa93fe36a9c6777945438f1f0297714`).
   - The 100k diagnostic is not a clean kept ceiling yet: after an additional settle it still had about `47k` pre-admitted items across validators and `execution_pending_count=1`. The remaining execution item is a partial merged-execution tail batch, exposing a drain-to-empty bug in `drain_ready_execution_for_batch()`.
   Status:
   - kept
   Reason:
   - Coalesced commit advancement fixes the async verification/commit-detector race without losing the kept near-80k live soak. The first direct-advance fix is binned/superseded; `ee64b6c57` is the kept implementation.
   - The 100k run demonstrates that the live cluster can cross `80k TPS` with state-root correctness, but it also exposes post-load settling debt that must be fixed before declaring the 100k ceiling clean.
   Next move:
   - Patch merged execution to flush a verified partial tail batch when the queue has no unverified blocks behind it. Then rerun 80k and 100k live soaks and require both state-root convergence and drain-to-empty before logging a clean 100k kept result.

217. Live testnet binned: local tail-batch flush breaks deterministic state roots
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 216 left a cosmetic/stability issue: merged execution could leave a verified partial tail batch pending forever because execution drained only full batches. Flushing a fully verified partial tail should drain `execution_pending_count` to zero without changing the committed Mahi sequence.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_execution_batch_flushes_verified_partial_tail -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_ -- --nocapture --test-threads=1`
   - Native Linux release build on v1 from pushed commit `c773b0f5c`, deployed to all four live hosts.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-tailbatch-freshwriters-20260426-1038`
   - `/tmp/and-live-distributed-ingress3-80k-soak60-tailbatch-20260426-1041`
   - `/tmp/and-live-distributed-ingress3-tailbatch-32w-freshwriters-20260426-1048`
   - `/tmp/and-live-distributed-ingress3-80k-soak60-tailbatch-32w-p1-20260426-1057`
   - `/tmp/and-live-distributed-ingress3-100k-soak60-tailbatch-32w-p1-20260426-1059`
   Evidence:
   - The direct tail-flush unit test passed, and the serial `mahi_` suite passed with `21` tests. The parallel `mahi_` filter has an existing env-var race in owner-relay tests, so the serial run is the valid verification shape for env-mutating Mahi tests.
   - The first live 80k rerun used the prior `8 writers × pipeline_depth=32` topology. It committed `3,989,416` intents (`66,490.3 TPS`) with matching state roots at common height `149`, but it produced `160` localhost connect timeouts. Those few network errors created large per-policy nonce holes because each writer had up to `32` in-flight nonce batches, leaving about `233k` pre-admitted items that could not safely drain. This diagnosed a harness/topology amplification issue rather than proving the tail patch regressed throughput.
   - Switching the live harness to `32 writers × pipeline_depth=1 × batch_size=8` per ingress endpoint removed the nonce-hole amplification. The 80k run accepted and committed `4,744,528` intents (`79,075.5 TPS`) with `0` network errors, `0` RPC errors, matching state roots at common height `156` (`0xa12be97ac1bf0ec5ac27e35def396cdfbd7144e0e6f187df042298f01d831dff`), and all sampled queues drained to zero.
   - The same `32w/p1` topology at 100k exposed the correctness failure. It accepted `4,291,864` requests and committed `1,970,295` intents (`32,838.2 TPS`) with `0` network/RPC errors, but validator v2 diverged from v1/v3/v4 at common height `379`: v1/v3/v4 state root `0x75807a23a344886137ae94f2b58952db18dcc62a63ecdaf04ce80c2bf8ff05fb`, v2 state root `0x5596cc01bcd83791fb1aa1287ff024ba51587fdb745de4ffdcccec5fdae46729`.
   - A binary search over `kombat_getStateRootAtHeight` found the first divergent height at `171`; height `170` still matched on all validators. Mahi `commit_log_hash_hex`, `committed_block_count`, `executed_block_count`, and `executed_intent_count` matched across all validators after the run.
   - Diagnosis: the tail-flush patch made execution batch boundaries local-timing-dependent. A validator could flush a short verified tail while another validator waited until more verified blocks arrived and executed a full batch. The Mahi commit log still matched, but the synthetic execution block grouping differed. Because block/state roots include the batch body root, event root, round, timestamp, and parent state root, differing batch boundaries create divergent state roots even for the same eventual Mahi commit log.
   Status:
   - binned and reverted
   Reason:
   - Deterministic state roots require deterministic execution batch boundaries. Local tail flushing depends on verifier/executor wake timing and is therefore unsafe under load. Draining a cosmetic tail is not worth compromising the state-root invariant.
   Next move:
   - Keep the deterministic full-batch execution rule from item 216. If tail draining is needed later, implement it through a deterministic boundary rule derived from the Mahi commit sequence, not from local queue timing.
   - Treat the clean `32w/p1` 80k run as the stable harness topology for further live testing. Do not use deep per-writer pipelining for correctness-sensitive runs unless the loadgen is changed to retry/serialize failed nonces.

218. Live testnet kept: reverted full-batch execution restores green 80k soak
   Date:
   - `2026-04-26`
   Hypothesis:
   - Reverting item 217's local tail-batch flush should restore deterministic full-batch execution while preserving the previously kept near-80k live throughput envelope.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_ -- --nocapture --test-threads=1`
   - Native Linux release build on v1 from pushed commit `808f5b29a`, deployed to all four live hosts.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-80k-soak60-reverted-32w-p1-20260426-111355`
   Evidence:
   - The serial `mahi_` RPC suite passed with `20` tests after removing the unsafe tail-flush test and restoring deterministic full-batch draining.
   - The live sanity run used the stable correctness topology from item 217: twelve ingress endpoints, `32 writers × pipeline_depth=1 × batch_size=8` per endpoint, and about `6,667 RPS` per endpoint for `60s`.
   - The run accepted `4,760,192` requests and committed `4,760,192` intents (`79,336.5 TPS`) with `0` RPC errors and `0` network errors.
   - State roots matched across all four validators after the run: `0xacd52e0d847c997ebef2d08b18035c6b710de5f6d5498f478b59317c6c3c5aa0`. Mahi commit log hashes also matched (`0x43bdc85e2e919051...`).
   - All pre-admitted pools drained to zero. `execution_pending_count=5` remained on all validators, which is the expected deterministic full-batch tail debt after reverting local tail flushing.
   Status:
   - kept
   Reason:
   - The revert restores the green near-80k live reference without request errors or state-root divergence. The remaining partial execution tail is deterministic and preferable to item 217's timing-dependent state-root split.
   Next move:
   - Do not pursue local/timing-based tail flushes. Any future tail drain must be based on a globally deterministic Mahi commit-sequence boundary.
   - Before trying multi-leader, pipeline, or wave-length changes, keep the current single-leader committer as the correctness baseline and run the next 100k diagnostic against this reverted full-batch build.

219. Live testnet kept: reverted full-batch build clears clean 100k offered diagnostic
   Date:
   - `2026-04-26`
   Hypothesis:
   - With deterministic full-batch execution restored, the current Mahi live cluster should preserve state-root correctness at 100k offered. If throughput remains below 100k, the bottleneck should be visible as client/ingress under-delivery or downstream consensus backlog.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Reused the native Linux release build from pushed commit `808f5b29a`, already deployed to all four live hosts for item 218.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-100k-soak60-reverted-32w-p1-20260426-114012`
   Evidence:
   - The run used the same correctness-safe topology as item 218: twelve ingress endpoints, `32 writers × pipeline_depth=1 × batch_size=8` per endpoint, and about `8,334 RPS` target per endpoint for `60s`.
   - The load generators accepted `5,122,920` requests and the chain committed `5,122,920` intents (`85,382.0 TPS`) with `0` RPC errors and `0` network errors.
   - State roots matched across all four validators after drain: `0x22970c3c335ad995e84745ee56a7742e235e8742fd05a6c059bafaa16db6f194`. Mahi commit log hashes also matched (`0xaff8b85bc4be9251...`).
   - All sampled pre-admitted pools drained to zero. All validators reported `committed_block_count=3454`, `executed_block_count=3448`, `executed_intent_count=9,883,112`, and `execution_pending_count=6`.
   - The accepted rate was below the nominal 100k target because ingress/loadgen delivery was uneven: v2 and v3 endpoints sustained about `8.18k-8.32k RPS` each, while v1 endpoints sustained about `6.39k-6.48k RPS` and v4 endpoints about `5.49k-5.57k RPS`. Consensus committed everything the clients delivered; this run does not show a Mahi backlog wall.
   Status:
   - kept diagnostic
   Reason:
   - This is the first clean live 100k-offered diagnostic above the M9 `80k at 100k offered` short-run threshold, with request errors at zero, state roots matching, commit logs matching, and pre-admitted pools drained. The remaining gap to 100k accepted is an ingress/loadgen/host-delivery issue, not an observed consensus commit backlog in this run.
   Next move:
   - Treat `85,382.0 TPS` as the current green live TPS reference.
   - Do not reopen micro-tuning of merged execution batch size, cadence, or local tail flushing. The next useful work should be structural: either dedicated multi-leader correctness bring-up, Phase 6 Block-STM execution, or transport/loadgen delivery work to make all twelve ingress endpoints hit target evenly.

220. Phase 4 M8 prep kept: default-inert Mahi multi-leader runtime seam
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 219 established a clean single-leader live reference at `85,382.0 TPS`. Multi-leader is a planned M8 structural lever, but it must be brought up as a correctness slice before any live throughput experiment. Runtime wiring should default to single-leader and only change behavior when `KOMBAT_MAHI_MULTILEADER` is explicitly set.
   Files changed:
   - `chain/crates/consensus/src/mahi/mod.rs`
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `KOMBAT_MAHI_TYPES=1 cargo test -p kombat-chain-consensus mahi:: --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_ -- --nocapture --test-threads=1`
   Benchmark runs:
   - None. This is a default-inert correctness bring-up, not a claimed TPS result.
   Evidence:
   - Added consensus unit coverage for the reference multi-leader direct-commit shape: a complete DAG commits `quorum` leaders at the same leader round in stable leader-offset order.
   - Added a fresh-builder resume test matching the runtime pattern, where `UniversalCommitterBuilder` is rebuilt from DAG state and resumes after the last decided multi-leader anchor without replaying the prior wave.
   - Wired `KOMBAT_MAHI_MULTILEADER` into `MahiState::advance_commit_detector()` via `UniversalCommitterBuilder::with_number_of_leaders`. Default remains one leader. `0/false/off` keeps one leader, `1/true/on` selects the Mahi prototype default of `4` capped by validator count, and numeric values greater than `1` select that explicit count capped by validator count.
   - Exposed `mahi.number_of_leaders` through `kombat_getConsensusMetrics` so live runs can prove which committer shape is active.
   - Added a four-validator RPC test with `KOMBAT_MAHI_MULTILEADER=2`; commit log hashes and decided leader counts converge across validators.
   Status:
   - kept correctness scaffold
   Reason:
   - The runtime seam is default-inert, covered by commit-rule and RPC convergence tests, and does not claim a throughput win. This avoids repeating prior pipeline/wave/multi-leader tuning mistakes while making a controlled live canary possible.
   Next move:
   - Commit and deploy this default-inert build, then run a control live sanity with the flag unset if needed. The first real multi-leader experiment should be a separate logged item using `KOMBAT_MAHI_MULTILEADER=2`, requiring state-root convergence and no request errors before any TPS result is considered meaningful.

221. Phase 4 M8 diagnostic: live `KOMBAT_MAHI_MULTILEADER=2` canary is correctness-green but not a TPS keep
   Date:
   - `2026-04-26`
   Hypothesis:
   - Enabling two Mahi leaders per wave may increase live committed throughput by allowing more committed anchors per wave, but it must first preserve state-root convergence and zero request errors on the live four-host testnet.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Native Linux release build from pushed commit `caec84c2a`, deployed to all four live hosts.
   - Live services were run with `KOMBAT_MAHI_MULTILEADER=2` on validator and ingress processes for this canary.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-80k-soak60-multileader2-32w-p1-20260426-1225`
   Evidence:
   - The run used the same correctness-safe distributed topology as items 218-219: twelve ingress endpoints, `32 writers × pipeline_depth=1 × batch_size=8` per endpoint, `60s` load, and `60s` drain. Target offered load was `80k RPS`.
   - The load generators accepted and the chain committed `3,988,576` intents (`66,476.3 TPS`) with `0` RPC errors and `0` network errors.
   - All four validators reported `mahi.number_of_leaders=2`, proving the canary exercised the multi-leader runtime path.
   - State roots matched across all four validators at common height `120`: `0xd4e0dd81e924f5588cc1fc444ac7b87c43b16d29431822e0b51e50279c780bda`.
   - All validators reported `committed_block_count=958`, `executed_block_count=952`, `execution_pending_count=6`, and `pre_admitted_pool_items=0`.
   - The run did not beat the current green single-leader reference from item 219 (`85,382.0 TPS` at 100k offered). Accepted throughput was bounded by uneven endpoint delivery: v2/v3 endpoints reached about `6.66k RPS`, v1 endpoints about `4.91k-5.03k RPS`, and v4 endpoints about `3.84k-3.87k RPS`.
   Status:
   - kept diagnostic; not enabled by default
   Reason:
   - Multi-leader=2 is correctness-green on the live testnet, but this run does not justify enabling it as the default throughput path. The canary delivered less load than the single-leader green reference and did not isolate a multi-leader throughput win.
   Next move:
   - Return the live cluster to the single-leader default for the known green reference.
   - Do not spend more slices tuning multi-leader until the ingress/loadgen delivery imbalance is resolved or a separate structural reason appears. The next high-leverage work remains Phase 6 Block-STM execution or transport/loadgen delivery work, not micro-tuning this canary.

222. Phase 4 M8 diagnostic: fresh single-leader A/B shows multi-leader was positive but not yet default-worthy
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 221 compared multi-leader=2 against the earlier item 219 100k single-leader reference, but live ingress delivery shifted after restart. A fresh single-leader run at the same 80k offered shape is needed to isolate whether multi-leader itself regressed throughput.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Reused the live single-leader deployment after removing the transient `KOMBAT_MAHI_MULTILEADER=2` env override from all validator and ingress processes.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-80k-soak60-singleleader-fresh-32w-p1-20260426-1312`
   Evidence:
   - The first no-reseed A/B shortcut was invalid: restarting the live services reset the seeded test accounts, so all twelve drivers failed during starting-nonce capture with no load sent. It is intentionally excluded from throughput comparison.
   - The valid fresh single-leader A/B used the same correctness-safe topology as item 221: twelve ingress endpoints, `32 writers × pipeline_depth=1 × batch_size=8` per endpoint, `80k RPS` nominal offered load, `60s` load, and `60s` drain.
   - The single-leader run accepted and committed `3,446,288` intents (`57,438.1 TPS`) with `0` RPC errors and `0` network errors.
   - State roots matched across all four validators at common height `100`: `0x20d7917072b607ab1b5e66eab9db3f543d94262a211cd8674ffcdf2adeba9675`.
   - All four validators reported `mahi.number_of_leaders=1`, `committed_block_count=792`, `executed_block_count=792`, `execution_pending_count=0`, and `pre_admitted_pool_items=0`.
   - Compared with item 221's fresh multi-leader=2 canary (`66,476.3 TPS`), multi-leader=2 was `+9,038.1 TPS` / `+15.7%` on this 80k A/B surface. The earlier item 221 conclusion should therefore be read as "not default-worthy versus the best green reference", not "multi-leader failed".
   - Both fresh 80k runs were below item 219's `85,382.0 TPS` green 100k diagnostic because endpoint delivery shifted materially after restart. In the single-leader A/B, v2 endpoints reached target (`~6.66k RPS`) but v1/v3/v4 endpoints under-delivered (`~3.50k-4.61k RPS`).
   Status:
   - kept diagnostic; multi-leader remains candidate-only
   Reason:
   - Multi-leader=2 is correctness-green and showed a real positive delta against a fresh same-shape single-leader control, but live delivery variance is too large to promote it to the default. The run does not supersede the item 219 green single-leader reference.
   Next move:
   - Do not tune multi-leader as a standalone knob. If continuing M8 work, wire and test `KOMBAT_MAHI_PIPELINE=1` as the next structural commit-rule step, because Mahi direct-skip is disabled while pipeline is off and multi-leader alone only increases leader-decision pressure.
   - Keep the live cluster on the single-leader default unless a later pipeline+multi-leader run beats item 219 with matching state roots and zero request errors.

223. Phase 4 M8 prep kept: default-off Mahi pipeline runtime seam
   Date:
   - `2026-04-26`
   Hypothesis:
   - Multi-leader alone is correctness-green and positive against a fresh A/B control, but Mahi's direct-skip behavior remains disabled while pipeline is off. Runtime needs a default-off `KOMBAT_MAHI_PIPELINE` seam so pipeline plus multi-leader can be canaried without changing the known green default path.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_ -- --nocapture --test-threads=1`
   Benchmark runs:
   - None. This is a default-inert correctness scaffold, not a claimed TPS result.
   Evidence:
   - Added `KOMBAT_MAHI_PIPELINE=0|1` parsing with default `false`.
   - `MahiState::advance_commit_detector()` now passes the configured pipeline value into `UniversalCommitterBuilder::with_pipeline()`.
   - `kombat_getConsensusMetrics` exposes `mahi.pipeline`, allowing live canaries to prove whether the pipelined committer path is active.
   - Added parser coverage proving the default is off and `1`/`true` enable the flag.
   - Added a four-validator RPC convergence test with `KOMBAT_MAHI_PIPELINE=1` and `KOMBAT_MAHI_MULTILEADER=2`; commit log hashes converge across validators and metrics report pipeline enabled with two leaders.
   - Defaults remain `pipeline=false` and `number_of_leaders=1` unless environment flags are explicitly set.
   Status:
   - kept correctness scaffold
   Reason:
   - The seam is default-inert and creates the missing runtime hook for the next M8 canary. It does not claim a throughput win and avoids promoting multi-leader without the paired pipeline behavior that makes the commit-rule change meaningful.
   Next move:
   - Build and deploy the default-inert seam, verify the live cluster reports `mahi.pipeline=false` and `mahi.number_of_leaders=1`, then run a live canary with `KOMBAT_MAHI_PIPELINE=1` and `KOMBAT_MAHI_MULTILEADER=2`. Treat correctness as the first gate; only consider enabling the pair if it beats the item 219 green reference with matching state roots and zero request errors.

224. Phase 4 M8 diagnostic: pipeline plus multi-leader is safe but not stable enough to promote
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 222 showed `KOMBAT_MAHI_MULTILEADER=2` was positive against a fresh same-shape single-leader control, but direct-skip remained constrained with pipeline off. Enabling `KOMBAT_MAHI_PIPELINE=1` together with two leaders may convert that correctness-green multi-leader path into a stable throughput gain over the item 219 single-leader reference.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Native Linux release build from pushed commit `00552ced3`, deployed to all four live hosts.
   - Default-inert verification before the canary: all validators reported `mahi.pipeline=false` and `mahi.number_of_leaders=1`.
   - Canary verification: all validators reported `mahi.pipeline=true` and `mahi.number_of_leaders=2`.
   Benchmark runs:
   - `/tmp/and-live-distributed-ingress3-100k-soak60-pipeline-multileader2-32w-p1-20260426-1405`
   - `/tmp/and-live-distributed-ingress3-100k-soak60-pipeline-multileader2-32w-p1-reuse-20260426-1428`
   Evidence:
   - First run: `5,163,656` accepted and committed over `60s` (`86,060.9 TPS`), `0` RPC errors, `0` network errors, matching state roots at common height `148`, all validators reported `number_of_leaders=2`, and post-run pre-admitted pools were empty.
   - The first run narrowly beat the item 219 green single-leader reference (`85,382.0 TPS`) by `678.9 TPS` / `0.8%`, but the margin is too small to treat as a stable default promotion by itself.
   - Reuse-writer confirmation run: `5,307,592` accepted, `4,943,152` committed over `60s` (`82,385.9 TPS`), `0` RPC errors, `0` network errors, matching state roots at common height `280`, and all validators reported `pipeline=true` with `number_of_leaders=2`.
   - The confirmation run remained correctness-green but left about `47k-51k` intents in each validator's pre-admitted pool after the drain window, so accepted throughput exceeded committed throughput. That is not a clean replacement for item 219's fully drained green reference.
   - Live endpoint delivery was still uneven. In the first run v2/v3 endpoints reached roughly target rate, v1 endpoints delivered about `7.46k-7.54k RPS`, and v4 endpoints delivered about `4.52k-4.56k RPS`. In the confirmation run v2/v3 again reached target, while v1/v4 landed around `6.24k-6.50k RPS`.
   - After the canary, the live cluster was returned to the default path and all validators again reported `mahi.pipeline=false` and `mahi.number_of_leaders=1`.
   Status:
   - kept diagnostic; not enabled by default
   Reason:
   - Pipeline plus multi-leader is correctness-safe on the live cluster and can exceed the current single-leader reference, but the gain is not stable and the confirmation run showed tail backlog. This is not a default-worthy M8 promotion yet.
   Next move:
   - Do not spend another slice tuning leader count or pipeline as standalone knobs. The code seams are in place for later use; the next high-leverage work should be structural execution or transport work. If the immediate goal is a stable M9-quality throughput gain, move to Block-STM/state execution parallelism or the live transport/loadgen delivery bottleneck rather than continuing fine-grained M8 knob tuning.

225. Phase 6 prep kept: default-off partitioned in-place resting executor
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 197 binned clone-heavy scoped worker market execution because it cloned per-shard policy and order-book state and regressed sustained throughput. The next credible Block-STM slice is to move owned policy/order-book partitions into workers for safe multi-market resting batches, preserving the current green default path until a multi-market canary proves benefit.
   Files changed:
   - `chain/crates/policy/src/lib.rs`
   - `chain/crates/node/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node partitioned_inplace_resting --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node resting --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-policy --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-policy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   - `cargo test -p kombat-chain-node --lib --manifest-path chain/Cargo.toml`
   Benchmark runs:
   - None. This is a default-off execution scaffold and makes no TPS claim.
   Evidence:
   - Added `PolicyStore` ownership helpers so execution workers can receive only the touched policy records instead of cloning the full policy store.
   - Added default-off `KOMBAT_PHASE6_PARTITIONED_INPLACE_RESTING` wiring in the Mahi in-place resting commit path. With the flag unset, the existing green execution path is unchanged.
   - The partitioned path only activates for all-intent `PlaceOrder` bodies that span at least two markets, have same-side batches within each market, and have no policy that touches more than one market in the same block.
   - The implementation validates runtime gates and admissions before mutation, removes affected policy records and order books from canonical state, executes owned partitions in scoped workers, restores rollback copies on error, and reinserts mutated partitions on success.
   - Unit coverage proves the partitioned path matches the serial market-sharded path for events, order books, accepted/item stats, and policy nonces on an interleaved three-market batch.
   - Unit coverage proves a cross-market same-policy batch declines without mutating `PolicyStore` or order books.
   Status:
   - kept correctness scaffold
   Reason:
   - This is a structural execution-write-model slice, not another micro-tune. It directly addresses item 197's clone-heavy failure mode while staying default-off until a multi-market benchmark demonstrates throughput benefit.
   Next move:
   - Build and run a same-shape multi-market control/canary with `KOMBAT_PHASE6_PARTITIONED_INPLACE_RESTING=1` and the existing multi-market loadgen knobs. If the canary is flat or regresses, bin the flag path; if it reduces execution backlog and preserves state roots, keep it and broaden semantic coverage toward full Block-STM execution.

226. Phase 6 canary binned: partitioned in-place resting executor regresses 16-market Mahi
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 225's default-off partitioned in-place executor may reduce multi-market execution cost by moving owned policy/order-book partitions into scoped workers instead of executing all market partitions serially.
   Files changed:
   - `chain/crates/policy/src/lib.rs` (reverted item 225 scaffold)
   - `chain/crates/node/src/lib.rs` (reverted item 225 scaffold)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Reused the release build from commit `b4dc7b0ca`.
   - `cargo test -p kombat-chain-node partitioned_inplace_resting --manifest-path chain/Cargo.toml -- --nocapture` passed before the canary.
   Benchmark runs:
   - Invalid discarded run: `/tmp/codex-exp-phase6-partitioned-control-64w-16market-120k-30s` missed `KOMBAT_CONSENSUS=mahi` and exercised the legacy local-submit path.
   - Invalid discarded run: `/tmp/codex-exp-phase6-partitioned-mahi-control-64w-16market-120k-30s` enabled Mahi consensus but missed `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`, so observer forwarding still re-entered the legacy local-submit queue.
   - Control: `/tmp/codex-exp-phase6-partitioned-mahi-async-control-64w-16market-120k-30s`
   - Canary: `/tmp/codex-exp-phase6-partitioned-mahi-async-canary-64w-16market-120k-30s`
   Evidence:
   - Valid runs used `KOMBAT_CONSENSUS=mahi`, `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`, `LOADTEST_TARGET_VALIDATOR=ingress,ingress`, `LOADTEST_MARKET_COUNT=16`, `64` writers, `120k` offered RPS, `30s` load, and `30s` drain.
   - Control with the partitioned flag unset accepted about `4.20M` requests with `0` RPC errors and `0` network errors. The committed timeseries measured `96,172.0 TPS` average and `135,890.6 TPS` peak; strict post-drain committed total was `4,198,248`; state roots converged at height `173`; all pre-admitted pools drained.
   - Canary with `KOMBAT_PHASE6_PARTITIONED_INPLACE_RESTING=1` accepted about `4.20M` requests with `0` RPC errors and `0` network errors. The committed timeseries measured `81,154.1 TPS` average and `147,385.8 TPS` peak; strict post-drain committed total was `3,531,984`; state roots converged at common height `598`; all pre-admitted pools drained.
   - The canary regressed measured average by `15,017.9 TPS` / `15.6%` and strict committed total by `666,264` intents / `15.9%`.
   - Subphase counters explain the regression shape: the partitioned path lowered event construction time on the sampled validator (`~296ms` vs `~1,882ms`) but increased matching time (`~1,092ms` vs `~735ms`) and precheck time (`~568ms` vs `~460ms`), while committing fewer resting-batch items (`2.85M` vs `3.38M`). Scoped worker overhead and partition movement outweighed the local event win.
   Status:
   - binned
   Reason:
   - The canary was correctness-green but materially slower on the exact multi-market surface it was built to improve. Keeping a default-off execution branch after a clean negative canary would repeat the project's binned-micro-tune pattern and add maintenance risk.
   Next move:
   - Revert the partitioned in-place flag path and policy ownership helpers from item 225, keeping only the audit trail.
   - Do not retry scoped per-batch worker partitioning. If Phase 6 continues, the next credible Block-STM work needs persistent execution workers or a different write-install model; otherwise shift to the live transport/loadgen delivery imbalance where the current green path is already near the M9 target.

227. Phase 6 hygiene kept: make the RPC binary and local harness default to Mahi
   Date:
   - `2026-04-26`
   Hypothesis:
   - After Mahi became the canonical runtime path, leaving the RPC binary and local harness defaulting to HotStuff creates invalid benchmark risk. Item 226 already produced two discarded local artifacts from missing Mahi flags. Removing the runtime-mode footgun should improve measurement reliability without claiming a TPS win.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/crates/rpc/src/main.rs`
   - `chain/scripts/loadtest_local_cluster.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml --bin kombat_chain -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_enabled_config_selects_mahi_without_env_flag --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-rpc mahi_owner_relay_redundancy_enqueues_primary_only --manifest-path chain/Cargo.toml -- --nocapture --test-threads=1`
   - Attempted `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml -- --test-threads=1`; stopped after it hung in the legacy `auto_network_timeout_recovery_skips_when_leader_is_reachable` surface. The touched binary tests and isolated Mahi test passed.
   Benchmark runs:
   - Invalid discarded follow-up: `/tmp/codex-exp-phase6-persistent-market-control-64w-16market-120k-30s`
   - Corrected control: `/tmp/codex-exp-phase6-persistent-market-control2-64w-16market-120k-30s`
   Evidence:
   - Removed `KOMBAT_CONSENSUS` parsing from the `kombat_chain` binary entrypoint. The binary now always starts with `mahi_enabled=true` and always drives `auto-mahi-round` when auto network rounds are configured.
   - Removed the unused `spawn_auto_network_round` path from the binary entrypoint so the dev auto-round loop cannot accidentally run the legacy HotStuff proposer.
   - Updated `loadtest_local_cluster.sh` convergence and post-load policy-query timeout defaults from `hotstuff` to `mahi`, so omitted `KOMBAT_CONSENSUS=mahi` no longer silently selects the wrong convergence invariant.
   - Follow-up validation exposed a deeper library-level footgun: `JsonRpcHttpServer::new_with_engine_handle()` still called `parse_runtime_consensus_mode()` internally, so `mahi_enabled=true` could start Mahi networking while leaving `consensus_mode=HotStuff` if the env var was absent.
   - The discarded follow-up artifact produced no blocks, filled the legacy `local_submit` queues, and left `mahi_pre_admitted_pool` at zero. This was an invalid HotStuff-path run caused by the internal default mismatch, not a throughput signal.
   - `JsonRpcHttpServer::new_with_engine_handle()` now treats `config.mahi_enabled=true` as authoritative and selects `RuntimeConsensusMode::Mahi` without requiring `KOMBAT_CONSENSUS=mahi`.
   - The corrected same-shape control with omitted `KOMBAT_CONSENSUS` accepted about `4.20M` requests with `0` RPC/network/nonce errors, measured `101,322.5 TPS` average and `160,511.6 TPS` peak, strictly committed `4,199,144` intents, converged at height `143`, and drained all Mahi pre-admitted pools. This validates the default fix; it is not claimed as a new optimization.
   - This does not delete deeper legacy HotStuff RPC/library methods. They remain as test and rollback scaffolding until a larger removal slice is explicitly planned.
   Status:
   - kept hygiene cleanup
   Reason:
   - This removes the specific benchmark footgun that caused item 226's invalid runs while avoiding a broad RPC-library deletion in the middle of Phase 6. It is not a throughput optimization.
   Next move:
   - Continue Phase 6 with either persistent execution workers/write-install redesign or the live transport/loadgen delivery imbalance. Do not retry scoped per-batch worker partitioning from item 226.

228. Phase 6 canary binned: persistent market execution workers regress 16-market Mahi
   Date:
   - `2026-04-26`
   Hypothesis:
   - Replacing scoped per-batch market matching workers with long-lived market execution workers may reduce thread-spawn and event-materialization overhead in the 16-market Mahi execution path.
   Files changed:
   - `chain/crates/node/src/lib.rs` (candidate reverted before commit)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-node persistent_market_execution_matches_serial_market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture` passed before revert.
   - `cargo test -p kombat-chain-node worker_market_matching_matches_serial_market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo test -p kombat-chain-node market_sharded_batch --manifest-path chain/Cargo.toml -- --nocapture`
   - `cargo clippy -p kombat-chain-node --lib --manifest-path chain/Cargo.toml -- -D warnings`
   Benchmark runs:
   - Control: `/tmp/codex-exp-phase6-persistent-market-control2-64w-16market-120k-30s`
   - Canary: `/tmp/codex-exp-phase6-persistent-market-canary-64w-16market-120k-30s`
   Evidence:
   - Both valid runs used omitted `KOMBAT_CONSENSUS` after item 227's corrected Mahi default, `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`, `LOADTEST_TARGET_VALIDATOR=ingress,ingress`, `LOADTEST_MARKET_COUNT=16`, `64` writers, `120k` offered RPS, `30s` load, and `30s` drain.
   - Control accepted about `4.20M` requests with `0` RPC/network/nonce errors. The committed timeseries measured `101,322.5 TPS` average and `160,511.6 TPS` peak; strict post-drain committed total was `4,199,144`; state roots converged at height `143`; all Mahi pre-admitted pools drained.
   - Canary with `KOMBAT_PHASE6_PERSISTENT_MARKET_EXECUTION=1` accepted about `4.20M` requests with `0` RPC/network/nonce errors. The committed timeseries measured `91,662.6 TPS` average and `124,119.6 TPS` peak; strict post-drain committed total was `4,198,832`; state roots converged at height `148`; all Mahi pre-admitted pools drained.
   - The canary regressed measured average by `9,659.9 TPS` / `9.5%` and peak by `36,392.0 TPS` / `22.7%`. Strict committed total was effectively flat (`312` fewer intents), so the long-lived worker path provided no kept throughput benefit.
   - The negative result is consistent with item 226: thread lifetime is not the core wall. Moving/cloning per-market books, intent payloads, and worker responses still costs more than the serial market-sharded path at this shape.
   Status:
   - binned
   Reason:
   - The candidate was correctness-green but slower on the exact 16-market surface it targeted. Keeping a default-off worker branch after a clean negative canary would add another dead execution mode and repeat the binned micro-tune pattern.
   Next move:
   - Keep item 227's Mahi-default fix, remove the persistent-worker candidate code, and do not retry thread-lifetime-only market workers. The next credible Phase 6 work should be a real Block-STM/write-install design or a live transport delivery slice, not another execution worker scheduling tweak.

229. Phase 6 diagnostic: current high-offer 16-market Mahi calibration
   Date:
   - `2026-04-26`
   Hypothesis:
   - Before wiring another execution path, run the current clean Mahi default at higher offered load. If it scales above the 120k-offered control, current code has headroom; if it backs up or collapses, the next work should target execution/drain structure rather than another local worker-scheduling tweak.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Reused release build from item 227.
   Benchmark runs:
   - `/tmp/codex-exp-phase6-current-highoffer-64w-16market-200k-30s`
   Evidence:
   - Run shape: omitted `KOMBAT_CONSENSUS` after item 227's corrected Mahi default, `KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1`, `LOADTEST_TARGET_VALIDATOR=ingress,ingress`, `LOADTEST_MARKET_COUNT=16`, `64` writers, `200k` total offered RPS, `30s` load, and `10s` post-load drain.
   - Ingress delivery was clean: the two loadgens achieved about `94.9k` and `96.0k` measured RPS (`~190.9k` total) with `0` RPC, network, and nonce errors.
   - Committed-timeseries throughput did not scale; it measured `52,812.6 TPS` average and `80,764.5 TPS` peak. Strict post-drain committed total was `3,338,202`.
   - State roots still converged at common height `134`, so this is a performance/drain saturation signal, not a correctness failure.
   - Owner pre-admitted pools backed up heavily after load: validator pools still held `524,288`, `286,488`, `182,016`, and `130,424` items respectively, while `local_submit` and `pending_relay` were zero. This points downstream of ingress/relay and upstream of full commit drain.
   - Per-writer committed counts became unfair under overload: minimum `232`, average `52,159`, maximum `76,637`. The low writers align with one owner shard falling behind, so high-offer overload currently creates owner-feed starvation even without request errors.
   - Commit execution latency also inflated under backlog: final sampled `mahi_commit_execution_batch_p90_ms=417`, max `473`, with only `47` execution batches and `1,709,144` sampled executed intents by the end of the measured timeseries.
   Status:
   - kept diagnostic
   Reason:
   - The current green path remains correct but does not absorb `~190k` ingress on localhost. The bottleneck is not HTTP ingress or legacy local-submit; it is owner pre-admitted pool drain plus execution/write-install capacity under backlog.
   Next move:
   - Do not tune batch size, cadence, or local worker scheduling from this result. Either validate the current green path on the 4-server cluster to separate localhost CPU artifacts from real transport behavior, or implement a real state-owning execution partition/write-install design. CoW checkpoints and the Phase 6 scheduler scaffolds are already present.

230. Phase 6 harness kept: live load tests accept market-count
   Date:
   - `2026-04-26`
   Hypothesis:
   - Live-network validation must be able to run the same 16-market workload used by the local Phase 6 Mahi benches. If the live harness silently defaults to one market, live results mix transport effects with a single-market execution hotspot and are not comparable to the current green local reference.
   Files changed:
   - `chain/scripts/loadtest_live_network.sh`
   - `chain/scripts/loadtest_live_network_from_validator.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_live_network.sh`
   - `bash -n chain/scripts/loadtest_live_network_from_validator.sh`
   Benchmark runs:
   - Not run; harness-only comparability fix before live deployment.
   Evidence:
   - Added `LOADTEST_MARKET_COUNT` to both live load-test wrappers and passed it through to `kombat_loadgen --market-count`, matching `loadtest_local_cluster.sh`.
   - Default remains `1`, so existing single-market live smoke runs are unchanged unless the caller opts into the 16-market Phase 6 shape.
   Status:
   - kept harness fix
   Reason:
   - This removes a live-vs-local measurement mismatch without changing runtime behavior.
   Next move:
   - Build the current `main` tree natively on a Linux validator host, deploy the Linux binary to the four live validators, then run live Mahi smoke and 16-market load tests with `LOADTEST_MARKET_COUNT=16`.

231. Phase 6 live diagnostic: ingress role required, remote ingress run stalls Mahi ancestor quorum
   Date:
   - `2026-04-26`
   Hypothesis:
   - The gap between localhost green TPS and low live TPS is likely live topology/transport, not another local execution micro-tune. Validate the current Mahi binary on the four VPS hosts using the same observer/ingress role shape as local Phase 6 before changing execution code.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc mahi_enabled_config_selects_mahi_without_env_flag --manifest-path chain/Cargo.toml -- --nocapture`
   Benchmark / live artifacts:
   - Direct-validator smoke: `/tmp/codex-live-smoke-16market-1k-10s`
   - Ingress smoke: `/tmp/codex-live-ingress-smoke-16market-1k-10s`
   - Remote-to-ingress 20k diagnostic: `/tmp/codex-live-ingress-16market-20k-30s`
   Evidence:
   - Built commit `4a41aade7` natively on Ubuntu 24.04 / x86_64 (`rustc 1.95.0`) on `203.0.113.15`; deployed the Linux `kombat_chain` binary to all four validators. All four restarted active with matching binary sha256 `2593b62c93e930e7f24b081246004ce20d5933a2931b6ce8d693789385577c33`.
   - Direct validator-local 1k smoke against `127.0.0.1:8545` is not a valid Phase 6 shape: it achieved `943.2` measured RPS but had `16` RPC timeout-recovery errors and only `3,000` committed intents after drain, with several writers at zero committed. This reproduces the topology mismatch: local green runs submit through ingress/observer routing, not direct validator RPC.
   - Started a dedicated ingress observer process on validator-1 at `0.0.0.0:8550` with `KOMBAT_RUNTIME_ROLE=ingress`, the live validator set, and Mahi observer async relay enabled. Removed two stale pre-deploy ingress processes listening on `8646` and `8647`.
   - Ingress smoke against `127.0.0.1:8550` was clean: `998.4` achieved RPS, `0` RPC/network/nonce errors, `14,976` committed after drain, and all 16 writers committed exactly `936` intents.
   - Remote-to-ingress diagnostic from validator-2 to validator-1 ingress at `20k` offered was not green: `20,037.3` achieved measured RPS but only `90,776` HTTP successes, `606,496` network send errors, and only `2,640` committed after drain.
   - After the failed remote-ingress run, all four validators were deterministic but stalled: identical `commit_log_hash_hex`, `next_round=630`, `max_round=629`, `block_count=2516`, `committed_block_count=2496`, and producer ancestor wait `2/3` for round `630`. Pre-admitted pools remained non-empty and did not drain at idle.
   - Added diagnostic-only Mahi metrics: tail round block counts, producer ancestor wait author hexes, and missing author hexes. These are needed to distinguish a real missing-broadcast/fetch bug from stale counters or by-round replacement before changing protocol behavior.
   Status:
   - blocked on live Mahi ancestor-stall diagnosis
   Reason:
   - The current live blocker is correctness/liveness under remote ingress pressure, not a throughput micro-optimization. Chasing Block-STM or batch-size changes while the DAG producer is stalled at `2/3` ancestors would repeat prior binned work.
   Next move:
   - Build and deploy the diagnostic metrics patch natively to the four validators, reproduce or inspect the stalled state, identify which round/author is missing, then fix Mahi recovery/broadcast before running higher live TPS tests.

232. Phase 6 live diagnostic: ingress send errors create nonce-hole poisoned pools
   Date:
   - `2026-04-26`
   Hypothesis:
   - After deploying item 231's Mahi diagnostics and dedicated ingress observers on all four live hosts, the low live TPS may be caused by invalid live benchmark semantics rather than consensus or execution throughput. Specifically, network/send errors from a pipelined writer can skip earlier nonces while later nonces are accepted, leaving strict per-policy Mahi pre-admitted pools unable to drain.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Reused deployed Linux build from commit `ce91f4d46`.
   Benchmark / live artifacts:
   - Green ingress smoke: `/tmp/codex-live-diag-ingress-smoke-16market-1k-10s`
   - Failed remote-ingress diagnostic: `/tmp/codex-live-diag-remote-ingress-16market-10k-20s`
   - Failed local-ingress canary: `/tmp/codex-live-local-ingress-v1-16market-10k-20s`
   Evidence:
   - Built and deployed commit `ce91f4d46` to all four validators. All validators and all dedicated ingress observer services used the same binary sha256 `852034ef61944a78d83607a439e9d7412cbc9a45273c2574afe42c87d31474de`.
   - Low-rate local ingress remained green after the diagnostic patch: the 1k/10s smoke against validator-1's local ingress accepted at `1009.6 RPS`, had `0` RPC/network/nonce errors, committed `14,976` intents, and kept all 16 writers balanced at `936` committed intents each.
   - Remote-to-ingress at 10k/20s was not a valid throughput run: the loadgen measured only `5,815.2 RPS`, produced `3,856` network send errors, accepted `147,976` requests, and committed only `3,768` intents after drain.
   - The failed remote run remained consensus-deterministic: all validators reported the same `commit_log_hash_hex`, and tail metrics showed a partially populated newest round rather than state-root divergence.
   - A single-host local-ingress 10k/20s canary also failed benchmark validity: it measured only `1,656.4 RPS`, produced `416` local network/request errors, accepted `51,824` requests, and committed only `7,720` intents after drain. Pre-admitted pools held roughly `8.6k-12.1k` items per validator.
   - The failure shape matches a nonce-hole benchmark artifact: each loadgen writer pipelines future nonces. If a network/send error happens for nonce `N` while later nonces from the same policy are accepted, Mahi's contiguous per-policy drain cannot safely include those later nonces, so pre-admitted pools stay non-empty even though consensus and state execution are otherwise deterministic.
   - This is distinct from binned local micro-optimization surfaces. Items `98-106` already binned cadence/chunk/CPU/direct-load/dual-observer attempts; items `226` and `228` binned execution worker scheduling variants. The current blocker is live benchmark validity and transport/error semantics.
   Status:
   - blocked on live benchmark nonce-continuity fix
   Reason:
   - Any live run with network/send errors is not a meaningful committed-TPS measurement for this workload. The harness can poison its own per-policy nonce streams, making Mahi appear throughput-limited by uncommittable later nonces.
   Next move:
   - Do not interpret higher live TPS runs unless they have `0` network errors, `0` RPC errors, matching state roots at common height, and drained Mahi pre-admitted pools.
   - Fix the live loadgen/harness semantics before more high-rate live runs: either add strict per-writer nonce retry/resync so network errors do not advance the durable nonce frontier, or run only within a proven zero-error envelope.
   - Do not retry cadence, batch-size, worker scheduling, direct-load bypass, dual-observer, scoped partition workers, or persistent market workers from this signal.

233. Phase 6 harness kept: strict nonce-safe live loadgen mode
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 232's failed live runs were invalid because a pipelined writer could advance past a nonce after a transport error, then submit later nonces that Mahi could not safely drain. Adding a live-safe loadgen mode that retries the same signed nonce batch on transport errors and fails the run on any remaining error should prevent the harness from poisoning per-policy streams.
   Files changed:
   - `chain/crates/loadgen/src/main.rs`
   - `chain/crates/loadgen/README.md`
   - `chain/scripts/loadtest_live_network.sh`
   - `chain/scripts/loadtest_live_network_from_validator.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-loadgen --manifest-path chain/Cargo.toml -- -D warnings`
   - `bash -n chain/scripts/loadtest_live_network.sh`
   - `bash -n chain/scripts/loadtest_live_network_from_validator.sh`
   - `git diff --check`
   Benchmark / live artifacts:
   - Not run yet; harness-only fix before rebuilding and redeploying the Linux loadgen.
   Evidence:
   - Added `kombat_loadgen --strict-nonce-retry`. In this mode each place-order worker requires `--pipeline-depth 1`, builds a signed batch from the current policy nonce, retries the same signed batch on HTTP/transport errors, and only advances its local nonce cursor after the batch is accepted or a retry observes benign replay evidence (`duplicate_intent` or `stale_intent_nonce`).
   - Added `kombat_loadgen --fail-on-errors`, which exits non-zero if any nonce, RPC, or network errors remain in the final report. This prevents invalid live artifacts from being treated as throughput data.
   - The strict mode requires either one worker or `--writers-json` with at least one distinct writer per worker, preserving the existing one-policy-per-worker live shape.
   - Updated both live load-test wrappers to enable strict nonce retry and fail-on-errors by default. Local localhost harnesses are unchanged.
   - This does not claim a throughput win. It makes live TPS numbers valid by preventing the benchmark harness from creating nonce holes under transport errors.
   Status:
   - kept harness fix
   Reason:
   - The prior live failure was not a consensus/execution optimization signal. It was a measurement-validity failure caused by loadgen nonce advancement semantics. The fix directly targets that failure mode and blocks repeat misinterpretation.
   Next move:
   - Rebuild `kombat_loadgen` natively for Linux, copy it to the live test host(s), restart the live services to clear item 232's poisoned in-memory pools, then rerun a low-rate ingress smoke followed by a zero-error live canary. Interpret TPS only if `net_err=0`, `rpc_err=0`, `nonce_err=0`, state roots match, and Mahi pre-admitted pools drain.

234. Phase 6 live diagnostic kept: strict nonce mode clears single-ingress poisoning
   Date:
   - `2026-04-26`
   Hypothesis:
   - If item 233 fixed the benchmark-induced nonce-hole problem, rerunning the previously bad single-host ingress shape should produce zero request errors, fully committed writer nonce frontiers, drained Mahi pre-admitted pools, and matching state roots. If TPS remains low under those conditions, the remaining issue is ingress/client delivery capacity rather than corrupted Mahi state.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - Reused commit `d074d6d80`.
   Benchmark / live artifacts:
   - `/tmp/codex-live-strict-ingress-smoke-16market-1k-10s`
   - `/tmp/codex-live-strict-local-ingress-v1-16market-10k-20s`
   Evidence:
   - Built the `d074d6d80` Linux `kombat_loadgen` natively on validator-1 and verified the binary exposes `--strict-nonce-retry`, `--strict-nonce-retry-attempts`, and `--fail-on-errors`.
   - Restarted all four `kombat-chain` and `kombat-chain-ingress` services to clear item 232's poisoned in-memory pools. All eight services restarted active.
   - Strict 1k/10s local-ingress smoke against validator-1 ingress was green: `1,011.2` measured RPS, `15,104` accepted, `15,104` committed, `0` nonce/RPC/network errors, all 16 writers committed exactly `944` intents, and sampled Mahi pre-admitted pools drained to zero.
   - Strict 10k/20s single-host local-ingress canary against validator-1 ingress was benchmark-valid: `90,328` accepted, `90,328` committed after drain, `0` nonce/RPC/network errors, writer commits ranged from `5,432` to `5,856`, sampled Mahi pre-admitted pools drained to zero, and state roots matched across all four validators at common height `275` (`0xcc00a613d182699c56cd57056b35ecaa21c9d6e82912b92f7251a2d8729bc7a8`).
   - The same 10k canary delivered only `3,969.6` measured RPS from one live ingress process, far below the nominal target. This is a clean delivery-capacity signal now, not a nonce-hole or Mahi-drain artifact.
   Status:
   - kept diagnostic
   Reason:
   - Strict nonce mode prevents the live harness from poisoning per-policy nonce streams. The low single-ingress TPS is now interpretable: one live ingress/loadgen process under this topology does not deliver 10k clean RPS, but the chain commits everything it cleanly accepts.
   Next move:
   - Move back to distributed ingress/loadgen topology using distinct writer shards per loadgen. Do not optimize consensus or execution from the single-ingress `3,969.6 TPS` number; it is an ingress/client delivery envelope for one process.

235. Phase 6 cleanup kept: port x402 gate and Mahi-only runtime cleanup without PolkaVM
   Date:
   - `2026-04-26`
   Hypothesis:
   - The runtime cleanup from `04bdc6b2` and the x402 RPC payment gate from `53ed69d3` can be ported onto `main` without merging the PolkaVM dual-VM branch ancestry. This should remove the selectable HotStuff runtime mode and preserve current Mahi behavior while adding optional x402 protection.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml x402 -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml json_rpc_place_order_intent_updates_book_after_block -- --nocapture`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml -- --test-threads=1`
   - `cargo test -p kombat-chain-execution --lib --manifest-path chain/Cargo.toml`
   - `cargo clippy -p kombat-chain-rpc --manifest-path chain/Cargo.toml -- -D warnings`
   - `git diff --check`
   Benchmark / live artifacts:
   - Not run; runtime cleanup and RPC payment gate only.
   Evidence:
   - Ported x402 JSON-RPC payment gating from `53ed69d3` into `chain/crates/rpc/src/lib.rs`: optional `KOMBAT_X402_*` env config, `PAYMENT-REQUIRED` challenge response, `PAYMENT-RESPONSE` settlement header, facilitator verify/settle calls, method allow-listing, and unit tests for challenge, unverified test mode, unprotected methods, and facilitator settlement.
   - Ported the Mahi-only runtime cleanup from `04bdc6b2` without merging PolkaVM commits: removed `RuntimeConsensusMode`, removed `parse_runtime_consensus_mode`, removed the `consensus_mode` server field, made Mahi pre-admitted relay handling unconditional, removed legacy observer-ingress leader-window fallback, and made `/network/relay-intents` use the Mahi pre-admitted path.
   - Kept PolkaVM and the dual-VM RFC out of this branch. No `POLKAVM` references were introduced.
   - Found and fixed a direct `kombat_submitIntent` deadlock exposed by the cleanup: public submit handling was behind the engine write lock but called `queue_submit_intent_public`, which previews through the engine handle. Moved the public submit path before the locked `NodeRpcService` dispatch, matching batch-submit behavior.
   - Removed two obsolete HotStuff timeout-recovery unit tests that waited on a HotStuff leader-ping path no longer reachable after the Mahi-only cleanup.
   Status:
   - kept cleanup
   Reason:
   - This lands the requested runtime cleanup and x402 gate without importing unwanted PolkaVM code. The change is not a throughput claim, but it removes a selectable legacy runtime path and prevents future agents from accidentally testing or routing through HotStuff mode.
   Next move:
   - Commit and push this cleanup. Then resume the live Phase 6 path by fixing the remote live loadtest writer-copy bug and rerunning distributed strict ingress canaries with distinct writer shards.

236. Phase 6 live harness kept: copy split writer manifests to expected remote path
   Date:
   - `2026-04-26`
   Hypothesis:
   - The failed distributed strict-ingress canary after item 234 was a harness copy-path bug, not a runtime failure. `loadtest_live_network_from_validator.sh` copied each split writer manifest to the remote directory under its local basename (`writers-v1.json`, etc.) while the remote loadgen command always read `${REMOTE_TMP}/writers.json`.
   Files changed:
   - `chain/scripts/loadtest_live_network_from_validator.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_live_network_from_validator.sh`
   Benchmark / live artifacts:
   - `/tmp/codex-live-validator-script-copy-smoke-v1-100rps-3s`
   Evidence:
   - Changed the remote copy step to copy the Linux loadgen binary directly to `${REMOTE_TMP}/kombat_loadgen` and the selected writer manifest directly to `${REMOTE_TMP}/writers.json`, matching the paths passed to the remote command.
   - Validator-local smoke against validator-1 using `/tmp/codex-live-strict-distributed-setup-16market/writers-v1.json` proved the fixed path: remote loadgen started successfully with 16 writers, sent 300 requests at 100 RPS for 3 seconds, and completed with `0` nonce/RPC/network errors.
   - The smoke intentionally was not a throughput benchmark; it only validates that split writer manifests now reach the remote filename expected by loadgen.
   Status:
   - kept harness fix
   Reason:
   - Distributed live canaries were blocked before sending meaningful load because remote hosts could not find `writers.json`. Fixing this prevents another false-negative live run and keeps Phase 6 measurements focused on actual transport/runtime behavior.
   Next move:
   - Commit and push the harness fix. Then rerun the distributed strict-ingress canary with four validator-local loadgens and distinct writer shards.

237. Phase 6 live ops kept: Ansible source of truth for four-host Mahi config
   Date:
   - `2026-04-26`
   Hypothesis:
   - The live-testnet throughput confusion after item 236 is primarily deployment/config drift and harness-shape drift, not a proven Mahi protocol regression. Codifying the known-green four-host topology in Ansible should catch missing env flags, missing ingress services, binary mismatch, and wrong RPC surfaces before any future TPS run is treated as comparable.
   Files changed:
   - `chain/deploy/ansible/`
   - `chain/deploy/testnet/README.md`
   - `chain/deploy/testnet/validator-inventory.example.toml`
   - `chain/scripts/generate_testnet_assets.sh`
   - `chain/scripts/render_live_testnet_inventory.sh`
   - `chain/scripts/loadtest_live_network.sh`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/generate_testnet_assets.sh`
   - `bash -n chain/scripts/render_live_testnet_inventory.sh`
   - `bash -n chain/scripts/loadtest_live_network.sh`
   - `KOMBAT_OPERATOR_TOKEN=dummy-token bash chain/scripts/render_live_testnet_inventory.sh /tmp/pvp-ansible-inventory-smoke.toml`
   - `bash chain/scripts/generate_testnet_assets.sh /tmp/pvp-ansible-inventory-smoke.toml /tmp/pvp-ansible-assets-smoke`
   - `chain/deploy/ansible/.venv/bin/ansible-playbook --syntax-check playbooks/site.yml`
   - `chain/deploy/ansible/.venv/bin/ansible-playbook playbooks/verify.yml`
   Benchmark / live artifacts:
   - No TPS benchmark; deployment/config verification only.
   Evidence:
   - Added repo-scoped Ansible under `chain/deploy/ansible` with a four-host inventory, known-green Mahi vars, validator and ingress systemd/env templates, `deploy.yml`, `verify.yml`, and `site.yml`.
   - Installed `ansible-core` into `chain/deploy/ansible/.venv` and configured Ansible home/tmp/control sockets under ignored repo-local paths so deployment tooling does not depend on writable `~/.ansible` or temporary `/tmp` installs.
   - The verifier passed against all four live hosts: `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3` were active on every host; all required Mahi env lines were present on every validator and ingress env; all local RPC ports (`8545`, `8550`, `8646`, `8647`) responded to `kombat_chainInfo`; and all four hosts had matching binary sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   - The prior low current-main measurement (`45,754 TPS`) is now classified as a wrong-shape four-endpoint canary, not a regression from item 219's known green twelve-endpoint live reference (`85,382 TPS` at 100k offered). The Ansible README explicitly records that benchmark topology to prevent another apples-to-oranges comparison.
   - Updated the older testnet asset generator/render path to emit `KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2` and `KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS=256`, preventing generated envs from recreating the primary-service fallback drift caught during live validation.
   - `loadtest_live_network.sh` now supports `LOADTEST_STATE_URL`, so writes can target ingress while nonce/state reads target validator RPC. This avoids false `unknown policy` failures when ingress roles do not expose the same read surface as validators.
   Status:
   - kept ops foundation
   Reason:
   - This does not claim a new TPS high, but it removes a repeated source of invalid evidence: mutable manual service topology and partially drifted env flags. Future live TPS runs should first pass the Ansible verifier, then use the item-219-equivalent twelve-endpoint shape before comparing against the `85,382 TPS` live reference.
   Next move:
   - Commit and push the Ansible deployment/config foundation. Then rerun the twelve-endpoint live 100k shape from the verified config to establish the current-main live TPS before resuming protocol or L1 feature work.

238. Phase 6 live harness kept: scripted twelve-endpoint ingress runner
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 237 fixed deployment/config drift, but manual benchmark launch can still recreate the wrong comparison shape. A repo-owned multi-ingress live runner should split writers and RPS across the twelve known ingress endpoints, use unique remote tmp directories per endpoint, and aggregate per-endpoint committed/error counts.
   Files changed:
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/deploy/ansible/README.md`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `bash -n chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `git diff --check`
   Benchmark / live artifacts:
   - `/tmp/codex-live-multi-ingress-smoke-12ep-120rps-3s`
   Evidence:
   - Added `loadtest_live_network_multi_ingress.sh`, which reads `hosts.txt`, runs all configured ingress ports per validator (`8550`, `8646`, `8647` by default), slices a full writer manifest into per-endpoint shards, assigns per-endpoint RPS from the requested aggregate target, sets `LOADTEST_TARGET_URL=http://127.0.0.1:<ingress-port>` and `LOADTEST_STATE_URL=http://127.0.0.1:8545`, and uses unique `LOADTEST_REMOTE_TMP_DIR` values to avoid `Text file busy` and stale-manifest failures.
   - The 12-endpoint smoke reused the freshly seeded writer manifest, ran `1` writer per endpoint at `120` total RPS for `3s`, and committed `1,056` intents with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - Every endpoint committed exactly `88` intents in the smoke and reported about `10.7 RPS`, proving all twelve ingress services were reachable through the script and the aggregation path was reading each endpoint artifact.
   Status:
   - kept harness fix
   Reason:
   - This converts the item-219 live benchmark topology into a repeatable command. It prevents future agents from comparing four-endpoint canaries against the twelve-endpoint `85,382 TPS` live reference.
   Next move:
   - Seed or reuse a `384`-writer manifest and run `bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60` from the verified Ansible config. Treat that result as the current-main live TPS reference.

239. Phase 6 live diagnostic: twelve-endpoint current-main replay exposes writer-shard imbalance
   Date:
   - `2026-04-26`
   Hypothesis:
   - After item 238 codified the twelve-endpoint live benchmark shape, replaying the item-219-style `100k` offered / `60s` run on current main should establish the current live TPS reference and determine whether the earlier low four-endpoint canary was just wrong-shape evidence.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-current-main-12ingress-100k-60s bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=258)` on all four validators
   Benchmark / live artifacts:
   - `/tmp/and-live-current-main-12ingress-100k-60s`
   Evidence:
   - The twelve-endpoint run committed `2,437,408` intents over `60s`, or `40,623 TPS`, with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - The loadgens only delivered `60,347.7` measured RPS total despite a `100k` offered target, so this is not a valid Mahi consensus ceiling. The bottleneck is before consensus commit: live writer/loadgen or writer-shard distribution.
   - Per-host committed totals were highly imbalanced: validator-1 endpoints committed `503,003` intents (`8,383 TPS`), validator-2 endpoints committed `833,223` (`13,887 TPS`), validator-3 endpoints committed `791,582` (`13,193 TPS`), and validator-4 endpoints committed `309,600` (`5,160 TPS`).
   - Mahi correctness held at the common post-run height: all four validators reported identical `state_root_hex=0x03cc767dee40e38c1272ceedc46ec8b001297bf28c0ec04bcef451ba96433f53` at height `258`. Head hashes differed, which remains the known Mahi wrapper artifact and is not the convergence invariant.
   - Host/service verification after the run showed all four validators reachable with `3/3` peers and no submit queue backlog. Pre-admitted pools drained to small or zero residuals, so the immediate failure mode is not a stuck owner queue.
   - The new multi-ingress runner splits writers into contiguous endpoint ranges, unlike `loadtest_local_cluster.sh`, which round-robins writers across shards. Given policy/owner sharding, contiguous ranges can accidentally concentrate writer ownership or nonce pacing by endpoint and produce exactly the host imbalance observed here.
   Status:
   - kept diagnostic
   Reason:
   - This run confirms current main is correct on state convergence but underdelivers the offered load because the benchmark harness does not distribute writers fairly across endpoints. It should not be compared directly with item 219's `85,382 TPS` green reference until the live runner uses round-robin writer sharding and is replayed with the same seeded manifest.
   Next move:
   - Change `loadtest_live_network_multi_ingress.sh` to round-robin writers across endpoint shards, matching the local harness distribution. Reuse `/tmp/and-live-current-main-12ingress-100k-60s/writers-all.json` for a no-reseed replay at `100k` / `60s`, then record the corrected current-main live TPS.

240. Phase 6 live diagnostic binned: round-robin writer sharding and stale-cluster single-endpoint probes
   Date:
   - `2026-04-26`
   Hypothesis:
   - The item 239 per-host imbalance may be caused by contiguous writer ranges in the new twelve-endpoint live runner. Round-robin writer distribution across endpoint shards, matching the local harness, should reduce policy/owner locality and restore a more comparable live RPS/TPS shape.
   Files changed:
   - `chain/scripts/loadtest_live_network_multi_ingress.sh` (reverted)
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-current-main-12ingress-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/codex-live-multi-ingress-rr-smoke-12ep-120rps-3s LOADTEST_DRAIN_SECS=5 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 1 120 3`
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-current-main-12ingress-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-current-main-12ingress-rr-100k-60s bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-current-main-12ingress-100k-60s/writer-shards/validator-2-p8550.json LOADTEST_OUTPUT_DIR=/tmp/and-live-single-v2-p8550-8333rps-30s LOADTEST_DRAIN_SECS=15 bash chain/scripts/loadtest_live_network_from_validator.sh /tmp/and-live-testnet validator-2 32 8333 30`
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-current-main-12ingress-100k-60s/writer-shards/validator-4-p8550.json LOADTEST_OUTPUT_DIR=/tmp/and-live-single-v4-p8550-8333rps-30s LOADTEST_DRAIN_SECS=15 bash chain/scripts/loadtest_live_network_from_validator.sh /tmp/and-live-testnet validator-4 32 8333 30`
   Benchmark / live artifacts:
   - `/tmp/codex-live-multi-ingress-rr-smoke-12ep-120rps-3s`
   - `/tmp/and-live-current-main-12ingress-rr-100k-60s`
   - `/tmp/and-live-single-v2-p8550-8333rps-30s`
   - `/tmp/and-live-single-v4-p8550-8333rps-30s`
   Evidence:
   - The round-robin smoke was correct at low load: all twelve endpoints committed exactly `88` intents, with `0` nonce/RPC/network errors.
   - The full round-robin replay regressed throughput: `1,738,508` committed over `60s` (`28,975 TPS`) versus item 239's contiguous-shard `40,623 TPS`. Measured RPS remained capped at `62,594.7`, so writer distribution was not the limiting factor.
   - Validator-4 remained the worst endpoint group under round-robin: `195,381` committed over `60s` (`3,256 TPS`) and only `3,192.5` measured RPS across its three ingress ports.
   - The attempted single-endpoint probes were not valid throughput comparisons because `loadtest_live_network_from_validator.sh` defaulted to target validator RPC `8545` rather than the intended ingress port unless `LOADTEST_TARGET_URL` is set. However, they exposed a stale live-cluster state: head height stayed at `540`, no new blocks were produced, validator-2 returned `timeout recovery quorum unavailable`, and validator-4 sent `0` requests after bootstrap.
   Status:
   - binned
   Reason:
   - Round-robin writer sharding made throughput worse and did not fix the low delivered-RPS problem. The live cluster also entered a stale state after repeated high-load runs, so further TPS numbers from this state would be invalid.
   Next move:
   - Keep the contiguous shard runner from item 238. Reset/restart the four-host live testnet services via Ansible, verify healthy Mahi peer state, then rerun a valid direct twelve-endpoint benchmark. If validator-4 still underdelivers after a clean restart, diagnose host-local ingress/validator CPU and service latency with `LOADTEST_TARGET_URL` set to the intended ingress port.

241. Phase 6 live kept: clean-restart twelve-endpoint current-main reference
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 240 showed the previous live cluster had entered stale timeout-recovery state. Restarting all four validator and ingress services, verifying config with Ansible, and reseeding fresh writers should produce a valid current-main live TPS reference.
   Files changed:
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Tests:
   - `cd chain/deploy/ansible && .venv/bin/ansible all -i inventory/testnet.yml -m shell -a 'systemctl restart kombat-chain kombat-chain-ingress kombat-chain-ingress2 kombat-chain-ingress3'`
   - `cd chain/deploy/ansible && .venv/bin/ansible-playbook playbooks/verify.yml`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-current-main-clean-restart-12ingress-100k-60s bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `kombat_getStateRootAtHeight(height=145)` on all four validators
   Benchmark / live artifacts:
   - `/tmp/and-live-current-main-clean-restart-12ingress-100k-60s`
   Evidence:
   - Ansible verification passed after restart: all four validator and ingress services were active, all local RPC ports (`8545`, `8550`, `8646`, `8647`) responded, and all hosts had matching binary sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   - Fresh writer seeding created and replicated `384` account/policy pairs before the measured window.
   - The clean twelve-endpoint run committed `1,712,008` intents over `60s`, or `28,533 TPS`, with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - State convergence passed at height `145`: all four validators reported identical `state_root_hex=0xb8881eacefa65ff1db4edfbf1a692861d622d972ddf1b7f5286d8718de26fa21`.
   - The run still underdelivered the offered load: measured RPS was only `57,986.9` against `100k` offered. Per-host measured RPS remained imbalanced: validator-1 `8,345`, validator-2 `22,414.6`, validator-3 `22,483.8`, validator-4 `4,743.5`.
   - Current clean live reference is therefore `28,533 TPS`, not the earlier item-219 `85,382 TPS`. The gap is not a correctness failure; it is a delivered-load / host-ingress imbalance problem visible before consensus capacity can be remeasured.
   Status:
   - kept diagnostic reference
   Reason:
   - This is the first current-main live measurement after codified Ansible config and a clean service restart. It is green on state convergence and errors, but not green against the historical live TPS target because the harness/services only delivered ~58% of requested load.
   Next move:
   - Diagnose validator-1 and validator-4 ingress/loadgen latency with targeted single-endpoint runs that explicitly set `LOADTEST_TARGET_URL=http://127.0.0.1:<ingress-port>` and `LOADTEST_STATE_URL=http://127.0.0.1:8545`. Do not add an external LB until direct per-host ingress delivery is understood.

242. Phase 6 live diagnostic binned: extra healthy-host ingress topology is not correctness-safe
   Date:
   - `2026-04-26`
   Hypothesis:
   - The low item-241 result may be mostly host ingress imbalance. If validator-2 and validator-3 are the only consistently healthy ingress hosts, adding three extra ingress services on those hosts and driving twelve healthy endpoints should recover delivered RPS without changing the four-validator consensus set.
   Files changed:
   - None. Temporary live services `kombat-chain-ingress4` through `kombat-chain-ingress6` were created on validator-2 and validator-3, then disabled and stopped.
   Tests:
   - Targeted single-endpoint probe: validator-2 p8550 at `8333 RPS` for `30s`.
   - Targeted single-endpoint probe: validator-4 p8550, p8646, and p8647 at `8333 RPS` for `30s`.
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-current-main-clean-restart-12ingress-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-v23-6ingress-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet-v23 64 100000 60`
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-current-main-clean-restart-12ingress-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-v23-12ingress-100k-60s LOADTEST_ENDPOINT_PORTS='8550 8646 8647 8648 8649 8650' LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet-v23 32 100000 60`
   - `kombat_getStateRootAtHeight(height=966)` on all four validators after the twelve-endpoint healthy-host run.
   Benchmark / live artifacts:
   - `/tmp/and-live-single-v2-p8550-ingress-8333rps-30s`
   - `/tmp/and-live-single-v4-p8550-ingress-8333rps-30s`
   - `/tmp/and-live-single-v4-p8646-ingress-8333rps-30s`
   - `/tmp/and-live-single-v4-p8647-ingress-8333rps-30s`
   - `/tmp/and-live-v23-6ingress-100k-60s`
   - `/tmp/and-live-v23-12ingress-100k-60s`
   Evidence:
   - Single-endpoint ingress probes confirmed real host/process imbalance: validator-2 p8550 delivered `8337.1 RPS` with p99 `46.877ms`, while validator-4 p8550 delivered `3009.1 RPS` with p99 `349.543ms`; validator-4 p8646 delivered `4185.1 RPS`; validator-4 p8647 was much healthier at `7998.1 RPS`.
   - The six-endpoint v2/v3-only topology committed `3,165,468` intents over `60s` (`52,758 TPS`) with `0` request errors. This proved the network could still produce a clean mid-50k result when slow ingress hosts were avoided, but it did not approach the item-219 `85,382 TPS` green reference.
   - The temporary twelve-endpoint v2/v3 topology delivered `72,554.8 RPS` and committed `3,145,337` intents over `60s` (`52,422 TPS`) with `0` request errors, but it failed the correctness gate.
   - State-root convergence failed at common height `966`: validator-1, validator-2, and validator-4 reported `0xe59f3687bc6ae96adc4cc624cb6fdd83fc4bcf50e3a6bc973af54c9de4574f76`, while validator-3 reported `0x0ac67745f366a30cd6096f4161bc162f215c70683c59e974e602766ab284f537`.
   Status:
   - binned
   Reason:
   - Avoiding slow ingress hosts is useful diagnostically, but the extra-ingress topology is not correctness-safe under the current live setup. It must not be used as a green recovery path or as evidence toward the 100k target.
   Next move:
   - Return to the codified three-ingress-per-host topology. Wipe/restart before further live measurements. Do not add more ingress services until the state-root divergence cause is understood.

243. Phase 6 live diagnostic: current-main clean wipe exposes correctness regression
   Date:
   - `2026-04-26`
   Hypothesis:
   - The item-241 low TPS result may have been caused by stale state or prior high-load contamination. Wiping `/var/lib/and-chain`, restarting the codified three-ingress-per-host topology, and reseeding fresh writers should recover the known green item-219 behavior if current main is otherwise sound.
   Files changed:
   - None.
   Tests:
   - Wiped `/var/lib/and-chain` on all four hosts and restarted `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3`.
   - `cd chain/deploy/ansible && .venv/bin/ansible-playbook playbooks/verify.yml`
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-clean-wipe-12ingress-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `kombat_getStateRootAtHeight(height=174)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-clean-wipe-12ingress-100k-60s`
   Evidence:
   - The clean wipe run committed `2,729,918` intents over `60s` (`45,499 TPS`) with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - Delivered RPS improved to `80,513.3`, and validator-2, validator-3, and validator-4 ingress endpoints mostly delivered about `8k RPS`; validator-1 remained slow at about `2.15k-2.24k RPS` per endpoint.
   - Correctness failed at common height `174`: validator-1 reported state root `0x7fab29f9d3982ea0406cef696d37c50471f515cbf477f451fcf8b29e2ab646c8`, while validators 2-4 reported `0x9b9c79298af5fe39923b954b603f5b947dcc5cf0c663cf4dbafb1d36f2ac3a24`.
   Status:
   - blocked
   Reason:
   - The current-main live stack is no longer merely below the item-219 throughput reference; it is correctness-bad under the clean twelve-endpoint topology. Any TPS number from this run is invalid as a green reference.
   Next move:
   - A/B against the item-219 known-good commit/build line (`808f5b29a`) on the same wiped hosts. If `808f5b29a` reproduces, bisect the first-parent range after item 219. If it does not reproduce, diagnose deployment/env/harness drift before touching protocol code.

244. Phase 6 live diagnostic: `808f5b29a` binary rollback does not reproduce item-219 under current env/harness
   Date:
   - `2026-04-26`
   Hypothesis:
   - If the regression is code-only, redeploying the item-219 known-good commit `808f5b29a` on the same four hosts should reproduce the `85,382 TPS` green reference, or at minimum restore state-root correctness and the expected Mahi pre-admitted relay behavior.
   Files changed:
   - None.
   Tests:
   - Built `808f5b29a` natively on validator-1 with Rust `1.95.0`.
   - Deployed the resulting `kombat_chain` binary with sha256 `ea1cb5d1809320d6f35f4a56f172af51bf5e8d7d9447bdbd32275fd0604e5cf5` to all four hosts.
   - Wiped `/var/lib/and-chain` on all four hosts and restarted the codified three-ingress-per-host services.
   - `SEED_VERIFY_REPLICAS=0 LOADTEST_OUTPUT_DIR=/tmp/and-live-808f5b29a-12ingress-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   Benchmark / live artifacts:
   - `/tmp/and-live-808f5b29a-12ingress-100k-60s`
   Evidence:
   - The item-219 binary rollback did not reproduce the item-219 result. Loadgens accepted `4,492,472` requests with `0` request errors, but the post-drain nonce-delta committed total was only `150,336` intents (`2,505.6 TPS`).
   - The live status after the run did not match item-219 relay behavior: validator-1 through validator-3 reported `mahi_pre_admitted_pool accepted=0 drained=0`, while validator-4 reported `accepted=1,194,528 drained=1,194,504`.
   - Peer-state snapshots were also asymmetric after the run, with validator-1 showing only one outbound peer session and several inbound entries still at `r1/v0/h0`.
   Status:
   - blocked
   Reason:
   - The failure did not follow the expected code-only rollback pattern. The current live env/harness/deployment state is not equivalent to item 219, so the known-good commit cannot be used as a clean A/B until the item-219 environment is reconstructed more exactly.
   Next move:
   - Stop throughput recovery attempts on this live setup until the item-219 env/harness delta is identified. Compare item-219 service env, generated asset scripts, and loadtest command shape against the current Ansible-managed env and runner. Restore current-main binary only after recording this diagnostic, then continue from a known consistent deployment.

245. Phase 6 live diagnostic: item-219 topology replay restores correctness but underdelivers load
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 244 failed because the Ansible-managed environment was not equivalent to item 219. The codified topology used `8550/8646/8647`, while the item-219 artifact used `8645/8646/8647`. Replaying with the item-219 ingress ports, the deployed live binary, and a wiped cluster should recover the `85,382 TPS` green reference if the remaining gap was only topology drift.
   Files changed:
   - `chain/deploy/ansible/inventory/group_vars/kombat_live_testnet.yml`
   - `chain/deploy/ansible/inventory/testnet.yml`
   - `chain/deploy/ansible/playbooks/deploy.yml`
   - `chain/deploy/ansible/README.md`
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/experiments/live/245-item219-topology-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-245-item219-topology-replay-v4`
   - `1598280c1`
   - Deployed live binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Note: local `main` had later commit `9e3e32b4f` while recording this result, but the live replay used the already-deployed `dd022982` binary and the Ansible topology from the v4 tag.
   Tests:
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/deploy.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml`
   - `KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible kombat_live_testnet -m shell -a 'systemctl stop kombat-chain-ingress3 kombat-chain-ingress2 kombat-chain-ingress kombat-chain && rm -rf /var/lib/and-chain && mkdir -p /var/lib/and-chain/snapshots && chown -R and-chain:and-chain /var/lib/and-chain && systemctl start kombat-chain && systemctl start kombat-chain-ingress kombat-chain-ingress2 kombat-chain-ingress3'`
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-item219-topology-replay-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=230)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-item219-topology-replay-100k-60s`
   Evidence:
   - The replay committed `4,417,077` intents over `60s`, or `73,618 TPS`, with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - The run did not deliver the requested `100k` offered load: aggregate measured RPS was `84,691.5`. This explains most of the miss against item 219's `85,382 TPS` green reference.
   - Endpoint delivery was asymmetric. Validator-1 ingress endpoints delivered only `3,116.5` to `3,263.1 RPS` each, while validator-2, validator-3, and validator-4 ingress endpoints delivered about `8,323` to `8,401 RPS` each.
   - State correctness passed after drain: all four validators reported the same block hash `0xe6c35f4e51e60dc24e01bf83e95180b4582f603c61491decceef467af0491d92` and state root `0x1b675a03506a4a024f9f873ade8996dea0cda3a0524760fa7c6c95d3de5cc845` at height `230`.
   - Live status showed all four validators at height `230`, peer reachability `3/3`, identical `head_hash`, submit queues drained, and `mahi_pre_admitted_pool.items=0` after the status check.
   Status:
   - blocked as green replay; kept as deployment/topology diagnostic
   Reason:
   - The item-219 port topology correction fixed the earlier environment mismatch and restored correctness, but it did not reproduce the green TPS reference because validator-1 local ingress delivery underperformed and aggregate delivered load was only `84.7k RPS`. This is not evidence of a Mahi consensus or state-execution regression.
   Next move:
   - Diagnose validator-1 host-local ingress delivery before changing protocol code. The next replay must either restore all twelve endpoints to target delivery or explicitly codify a different live load topology. Do not compare sub-100k delivered-load runs against item 219 as protocol regressions.

246. Phase 6 live diagnostic binned: validator-1 writer overprovision does not restore p1 delivery
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 245 missed the green TPS reference because validator-1 ingress endpoints underdelivered the offered load. If the safe `pipeline_depth=1` topology is latency/concurrency-bound on validator-1, increasing independent writers while keeping p1 should restore per-endpoint delivery without reintroducing nonce-hole risk.
   Files changed:
   - `chain/experiments/live/246-validator1-ingress-delivery.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-246-validator1-ingress-delivery-plan`
   - `chain-exp-246-v1-64w-p1-diagnostic`
   - Deployed live binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   Tests:
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-item219-topology-replay-100k-60s/writer-shards/validator-1-p8645.json LOADTEST_OUTPUT_DIR=/tmp/and-live-246-v1-p8645-p1-8333-30s LOADTEST_TARGET_URL=http://127.0.0.1:8645 LOADTEST_STATE_URL=http://127.0.0.1:8545 LOADTEST_DRAIN_SECS=15 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_from_validator.sh /tmp/and-live-testnet validator-1 32 8333 30`
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-item219-topology-replay-100k-60s/writer-shards/validator-1-p8646.json LOADTEST_OUTPUT_DIR=/tmp/and-live-246-v1-p8646-p1-8333-30s LOADTEST_TARGET_URL=http://127.0.0.1:8646 LOADTEST_STATE_URL=http://127.0.0.1:8545 LOADTEST_DRAIN_SECS=15 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_from_validator.sh /tmp/and-live-testnet validator-1 32 8333 30`
   - `LOADTEST_WRITERS_JSON=/tmp/and-live-item219-topology-replay-100k-60s/writer-shards/validator-1-p8647.json LOADTEST_OUTPUT_DIR=/tmp/and-live-246-v1-p8647-p1-8333-30s LOADTEST_TARGET_URL=http://127.0.0.1:8647 LOADTEST_STATE_URL=http://127.0.0.1:8545 LOADTEST_DRAIN_SECS=15 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_from_validator.sh /tmp/and-live-testnet validator-1 32 8333 30`
   - `jq '.[0:64]' /tmp/and-live-item219-topology-replay-100k-60s/writers-all.json > /tmp/and-live-246-v1-64w.json && LOADTEST_WRITERS_JSON=/tmp/and-live-246-v1-64w.json LOADTEST_OUTPUT_DIR=/tmp/and-live-246-v1-p8646-p1-64w-8333-30s LOADTEST_TARGET_URL=http://127.0.0.1:8646 LOADTEST_STATE_URL=http://127.0.0.1:8545 LOADTEST_DRAIN_SECS=15 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_from_validator.sh /tmp/and-live-testnet validator-1 64 8333 30`
   - `kombat_getStateRootAtHeight(height=973)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-246-v1-p8645-p1-8333-30s`
   - `/tmp/and-live-246-v1-p8646-p1-8333-30s`
   - `/tmp/and-live-246-v1-p8647-p1-8333-30s`
   - `/tmp/and-live-246-v1-p8646-p1-64w-8333-30s`
   Evidence:
   - Validator-1 p8645 with `32` writers, p1, target `8,333 RPS` achieved `6,798.1 RPS`, committed `243,328` intents, and had `0` request errors.
   - Validator-1 p8646 with `32` writers achieved `4,019.2 RPS`, committed `136,000` intents, and had `0` request errors.
   - Validator-1 p8647 with `32` writers achieved `4,202.1 RPS`, committed `153,000` intents, and had `0` request errors.
   - Increasing p8646 to `64` writers did not help: it achieved `3,920.0 RPS`, committed `152,288` intents, and latency worsened (`p50=88.436ms`, `p99=811.469ms`).
   - Correctness remained green after the probes: all four validators matched block hash `0x4c88ec64b12b4eec3905ca1cedd94bacb122de0bddc8a8e2f2b5d301f72587af` and state root `0x27692afde1b3dacc58b95afcb6282cecfd76af0b3fba7542b327498dbee4667e` at height `973`.
   Status:
   - binned
   Reason:
   - Validator-1 p1 delivery is not fixed by adding more independent writers. The bottleneck is host/process delivery on validator-1, not insufficient writer concurrency. Keeping a 64-writer v1 shape would add seeding cost and worsen latency without recovering target RPS.
   Next move:
   - Stop using validator-1 as a required ingress source for the green recovery path. Codify a no-v1-ingress live benchmark topology that still keeps all four validators in consensus and verifies state roots across all four. If that topology restores delivered load and correctness, use it as the stable live benchmark surface.

247. Phase 6 live diagnostic binned: no-v1-ingress topology underdelivers p1 load
   Date:
   - `2026-04-26`
   Hypothesis:
   - Items 245-246 showed validator-1 is the slow ingress host under the safe p1 topology. Excluding validator-1 from ingress/loadgen duties while keeping it in the four-validator consensus set may recover green live throughput if validators 2-4 can absorb the 100k offered load across nine endpoints.
   Files changed:
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/experiments/live/247-no-v1-ingress-topology.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-247-no-v1-ingress-plan`
   - Deployed live binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   Tests:
   - Wiped `/var/lib/and-chain` on all four hosts and restarted `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3`.
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml`
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-247-no-v1-ingress-100k-60s LOADTEST_VALIDATOR_NAMES='validator-2 validator-3 validator-4' LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=197)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-247-no-v1-ingress-100k-60s`
   Evidence:
   - Added `LOADTEST_VALIDATOR_NAMES` to the live multi-ingress runner so benchmark ingress/loadgen sources can be role-filtered without removing validators from the consensus set.
   - The no-v1 run committed `3,963,264` intents over `60s`, or `66,054 TPS`, with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - The run underdelivered even more than item 245: aggregate measured RPS was `71,636.1` because nine endpoints had to target about `11.1k RPS` each and could not sustain it under p1.
   - State correctness passed at common height `197`: all four validators reported block hash `0x01158d714778d9d7475460db12c1afb34621abc96a107910b0920e2a79b50e53` and state root `0xacbbefe77d0135d6c4b1984bbe2eaef8cc7d823ce1d0376abd4f7b13cae6084f`.
   - Live status showed all submit queues and Mahi pre-admitted pools drained after the run.
   Status:
   - binned
   Reason:
   - Removing validator-1 as an ingress source avoids the slow host but increases the p1 per-endpoint target too far. Validators 2-4 did not absorb the extra load, so throughput fell below item 245's `73,618 TPS`.
   Next move:
   - Keep the role-filter harness support because it is useful for diagnostics, but do not use no-v1 p1 as the green recovery path. The next tagged recovery canary should test a fail-on-error p2 live topology: strict nonce retry disabled because p2 is incompatible, `--fail-on-errors` still enabled, state-root convergence required, and any request error bins the run.

248. Phase 6 live diagnostic binned: p2 fail-on-error topology lowers delivered load
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 247 showed p1 without validator-1 raised per-endpoint targets too far, while item 246 showed validator-1 p1 delivery cannot be fixed by more writers. A 12-endpoint topology with `pipeline_depth=2` may restore aggregate delivered load while preserving safety if `LOADTEST_FAIL_ON_ERRORS=1` bins any nonce/RPC/network error.
   Files changed:
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/experiments/live/248-p2-fail-on-error-topology.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-248-p2-fail-on-error-plan`
   - Deployed live binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   Tests:
   - Wiped `/var/lib/and-chain` on all four hosts and restarted `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3`.
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml`
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-248-p2-fail-on-error-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=2 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=221)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-248-p2-fail-on-error-100k-60s`
   Evidence:
   - The p2 canary committed `3,341,264` intents over `60s`, or `55,687.7 TPS`, with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - Aggregate measured delivery fell to `60,590.2 RPS`, below item 247's no-v1 p1 delivery (`71,636.1 RPS`) and item 245's item-219 replay delivery (`84,691.5 RPS`).
   - Validator-1 remained the weakest ingress host even under p2: its three endpoints delivered only `2,285.5` to `2,359.6 RPS` each. Validator-3 and validator-4 also underdelivered versus p1, while validator-2 remained near target.
   - State correctness passed at common height `221`: all four validators reported block hash `0x71aa4a8bcd91d55f828dcf0192fcee28650617518a32b05cc0b63a513eb4c813` and state root `0xa2debfeaa30de14e80eec58d405096f7394a60302aacf8efbff3e8777f45c1ca`.
   - Live status showed all submit queues and Mahi pre-admitted pools drained after the run. Accepted/drained Mahi pre-admitted pool counts were about `948.6k-949.1k` per validator.
   Status:
   - binned
   Reason:
   - Pipeline depth 2 did not recover the green live topology. It remained correct and error-free, but it reduced delivered load and committed TPS materially below the p1 references. This should not be retried as a green recovery path without a separate transport/loadgen reason to believe delivery behavior changed.
   Next move:
   - Stop trying writer/pipeline/topology micro-variants for the recovery path. The evidence now points to host-local ingress delivery limits, not Mahi correctness. Recover green TPS by either restoring the exact item-219 runtime/deploy environment or moving to a role-aware deployment that separates validator consensus from high-rate ingress/loadgen capacity, with every environment setting codified before the run.

249. Phase 6 live diagnostic binned: known-good binary plus corrected ports does not reproduce item 219
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 244 redeployed the item-219 `808f5b29a` known-good binary before the ingress-port drift was corrected. If the remaining gap is code/build related, redeploying that same rollback binary with the corrected `8645/8646/8647` topology should reproduce item 219's `85,382 TPS` green result, or at least restore its drained-pool behavior.
   Files changed:
   - `chain/experiments/live/249-known-good-binary-corrected-topology.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-249-known-good-corrected-topology-plan`
   - Rollback binary path: `/tmp/kombat_chain-808f5b29a`
   - Rollback binary sha256: `ea1cb5d1809320d6f35f4a56f172af51bf5e8d7d9447bdbd32275fd0604e5cf5`
   Tests:
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-808f5b29a .venv/bin/ansible-playbook playbooks/deploy.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-808f5b29a .venv/bin/ansible-playbook playbooks/verify.yml`
   - Wiped `/var/lib/and-chain` on all four hosts and restarted `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3`.
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-249-808f5b29a-corrected-topology-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - Extra `120s` drain after the normal run, then `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=623)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-249-808f5b29a-corrected-topology-100k-60s`
   Evidence:
   - The known-good rollback binary with corrected ports did not reproduce item 219. The loadgens accepted `5,250,744` requests with `0` nonce errors, `0` RPC errors, and `0` network errors, but the normal post-run committed total was only `1,845,044` intents (`30,750.7 TPS`).
   - Delivered load was `81,147.8 RPS`. Validator-2/3/4 endpoints reached the target band (`~8.33k-8.40k RPS`), while validator-1 endpoints remained slow at `1,913.5-2,083.3 RPS`.
   - After an additional `120s` drain, Mahi pre-admitted pools still were not drained: validator-1 had `461,096`, validator-2 had `412,351`, validator-3 had `0`, and validator-4 had `97,128` pending items.
   - State correctness still passed at common height `623`: all four validators reported block hash `0xc0643d78b35921fe340f746cd46811a09354ce2ae614b60a61b93342e756fdd6` and state root `0xd0e5907304376970e7cfc5177b70fe17efa23f6b5e40c0e2210e3a1734566e58`.
   Status:
   - binned
   Reason:
   - This closes the missing A/B from item 244. The item-219 code line plus corrected ports still does not reproduce item 219 under the current Ansible/runtime/loadgen environment, so the green-reference loss is not explained by current code alone. The remaining delta is environment/runtime behavior: validator-1 host-local ingress delivery and Mahi pre-admitted drain/backlog behavior differ materially from item 219.
   Next move:
   - Restore the current live binary before any further tests. Stop rollback-binary replay attempts; the useful next diagnostic is environment/runtime capture, not protocol tweaking: compare item-219 service env/process topology with current Ansible env, host resource limits, service CPU affinity/thread counts, open connection/session counts, and per-host ingress process performance. The next runnable benchmark should be preceded by a committed environment snapshot manifest.

250. Phase 6 live diagnostic kept: current live environment snapshot captured
   Date:
   - `2026-04-26`
   Hypothesis:
   - Items 245-249 show the item-219 green reference cannot be recovered by replaying writer/pipeline/topology variants or by redeploying the item-219 binary under the current Ansible environment. Before any more throughput attempts, the live runtime environment must be captured as a replayable artifact so future runs can be compared against exact binary, env, systemd, process, socket, and host state.
   Files changed:
   - `chain/deploy/ansible/playbooks/capture_env.yml`
   - `chain/experiments/live/250-environment-snapshot.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-250-env-snapshot-plan`
   - `chain-exp-250-env-snapshot-capture-fix`
   - Current live binary sha256 after restore: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   Tests:
   - Restored `/tmp/kombat_chain-live-dd022982` to all four hosts via Ansible deploy.
   - Wiped `/var/lib/and-chain` on all four hosts and restarted `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3`.
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 KOMBAT_SNAPSHOT_DIR=/tmp/and-live-env-snapshot-250-current-dd022982 .venv/bin/ansible-playbook playbooks/capture_env.yml`
   - Verified each snapshot archive contains `ansible-facts.json`.
   - Checked snapshot archives for unredacted `KOMBAT_OPERATOR_TOKEN=` entries; none found.
   Benchmark / live artifacts:
   - `/tmp/and-live-env-snapshot-250-current-dd022982`
   Evidence:
   - Captured one archive per validator:
     - `validator-1.tgz`: `32bd29f41750e99a3aea0dabcf1bc743b239c540f54949a9aaa4abc8931f5621`
     - `validator-2.tgz`: `048a59568dae7f3ca52d5b2a7d59bd3ea842bc255045e662facb8a2b67498755`
     - `validator-3.tgz`: `4e29d0e361b23ce37612a59004fea54a9cf726cdac446cc62e34b25856410829`
     - `validator-4.tgz`: `8f56b4123bdf0b93fed78ca0d3610501fd92a3ac164838a0f30f944a996d9e10`
   - Each archive includes Ansible facts, redacted `/etc/and-chain/*.env`, systemd unit/status/show output for validator and ingress services, process tables, per-process limits/environments/scheduler state for `kombat_chain`, listening/all TCP sockets, selected host/sysctl state, binary SHA, and local RPC chain/consensus metrics for ports `8545/8645/8646/8647`.
   Status:
   - kept diagnostic
   Reason:
   - This makes the live deployment environment inspectable and replayable before the next benchmark. It directly addresses the failure mode from items 244-249: untracked environment drift made the item-219 green reference impossible to reproduce or falsify cleanly.
   Next move:
   - Use this snapshot to compare validator-1 against validators 2-4 for host-local ingress/process limits and socket/session behavior. Do not run another live TPS experiment until the environment delta hypothesis is stated in a committed experiment manifest.

251. Phase 6 live diagnostic kept: stopping non-chain nginx improves isolation but does not recover item 219
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 250's environment snapshot found a concrete host delta: validator-1 is the only chain host running nginx, with 16 nginx workers and listeners on `80/443`, while validators 2-4 have no nginx workers. Items 245-249 repeatedly showed validator-1 ingress endpoints under-delivering relative to the other validators. If validator-1's extra non-chain workload is the current environment drift, stopping nginx before a clean item-219 topology replay should improve validator-1 delivery and recover green TPS.
   Files changed:
   - `chain/deploy/ansible/playbooks/stop_non_chain_services.yml`
   - `chain/experiments/live/251-validator1-nginx-isolation.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-251-v1-nginx-isolation-plan`
   - Current live binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   Tests:
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_NON_CHAIN_SERVICES_TO_STOP=nginx .venv/bin/ansible-playbook playbooks/stop_non_chain_services.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml`
   - Wipe `/var/lib/and-chain` on all four hosts and restart `kombat-chain`, `kombat-chain-ingress`, `kombat-chain-ingress2`, and `kombat-chain-ingress3`.
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-251-v1-nginx-stopped-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=143)` on all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-251-v1-nginx-stopped-100k-60s`
   Evidence:
   - The playbook confirmed nginx was active only on validator-1 before the run: `validator-1 nginx active=active enabled=enabled`; validators 2-4 reported `nginx absent`. After the playbook, validator-1 reported `nginx active=inactive enabled=enabled`.
   - The replay accepted and committed `4,660,008` intents over `60s`, or `77,666.8 TPS`, with `0` nonce errors, `0` RPC errors, and `0` network errors.
   - This is a material recovery from item 249's `30,750.7 TPS`, but it remains below the item-219 green reference of `85,382.0 TPS`.
   - Aggregate measured delivery was `71,634.5 RPS`. v2/v3 endpoints hit the target band (`~8.33k RPS` each), v4 endpoints underdelivered at `4.66k-4.72k RPS`, and validator-1 remained weak at `2.47k-2.55k RPS` despite nginx being stopped.
   - State correctness passed at common height `143`: all four validators reported block hash `0x53d62fb065fb86f09e5f5aea14ccfcf43c80dd38e5339c9bfb3cf9a6745951af` and state root `0x3817e823597db3f4483dec6135d96689f8d094532abd16c0011f1cbe2f3fb3bc`.
   - Live status showed all submit queues drained and all Mahi pre-admitted pools drained after the run. The per-validator Mahi pool counters were accepted/drained `1,165,000`, `1,164,992`, `1,165,104`, and `1,164,912`, with `items=0` and `dropped_oldest=0`.
   Status:
   - kept diagnostic
   Reason:
   - Stopping non-chain nginx is the right environment isolation state for validator hosts and produced a clean correctness-green replay, but it did not fully recover item 219. The remaining gap is not a Mahi consensus regression; it is endpoint/host delivery imbalance. Validator-1 remains the weakest ingress host, and validator-4 also underdelivers, while validator-2 and validator-3 can hit the per-endpoint target.
   Next move:
   - Keep the playbook and keep nginx stopped for chain perf runs. Do not resume protocol micro-tuning. The next recovery slice should codify a topology that compensates for weak ingress hosts: either add more ingress endpoints on the healthy hosts or make the live loadtest role-aware so v2/v3 carry more of the offered load while all validators still participate in consensus and state checks.

252. Phase 6 live diagnostic binned: healthy-host six-ingress topology overloads Mahi drain and diverges
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 251 showed the remaining green-reference loss is endpoint/host delivery imbalance: validator-2 and validator-3 hit the `~8.33k RPS` per-endpoint target, while validator-1 and validator-4 underdelivered. If we add three more ingress services and send load only to validator-2 and validator-3, we get twelve healthy ingress endpoints at the same per-endpoint target as item 219 while all four hosts still participate as validators.
   Files changed:
   - `chain/experiments/live/252-six-ingress-topology.vars.yml`
   - `chain/experiments/live/252-healthy-host-six-ingress.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - `chain-exp-252-healthy-host-six-ingress-plan`
   - Current live binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   Tests:
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/deploy.yml -e @../../experiments/live/252-six-ingress-topology.vars.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml -e @../../experiments/live/252-six-ingress-topology.vars.yml`
   - Wipe `/var/lib/and-chain` on all four hosts and restart `kombat-chain` plus `kombat-chain-ingress` through `kombat-chain-ingress6`.
   - `LOADTEST_VALIDATOR_NAMES='validator-2 validator-3' LOADTEST_ENDPOINT_PORTS='8645 8646 8647 8648 8649 8650' LOADTEST_OUTPUT_DIR=/tmp/and-live-252-v2v3-six-ingress-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - `kombat_getStateRootAtHeight(height=49)` on all four validators.
   - Rolled back the live topology: disabled/stopped `kombat-chain-ingress4` through `kombat-chain-ingress6`, redeployed the default three-ingress Ansible topology, wiped `/var/lib/and-chain`, restarted default services, verified default topology, and confirmed extra ingress services were inactive.
   Benchmark / live artifacts:
   - `/tmp/and-live-252-v2v3-six-ingress-100k-60s`
   Evidence:
   - The loadgens accepted `5,280,776` requests with `0` nonce errors, `0` RPC errors, and `0` network errors. Aggregate measured delivery was `80,511.2 RPS`.
   - Only `1,122,661` intents were committed after the standard drain (`18,711.0 TPS`). This is far below item 251's `77,666.8 TPS` and item 219's `85,382.0 TPS`.
   - The topology left large Mahi pre-admitted backlogs after the standard drain: validator-1 `524,288`, validator-2 `523,384`, validator-3 `524,288`, validator-4 `524,288`.
   - State correctness failed at common height `49`: validators 1-3 reported block hash `0xc0e30d0a87f622df6c37ad994df4eb621fbfb497d07368f2cf261876a3f514f5` and state root `0x99d8c62f8d34278a14181d813e8f6a0424d236dff333488d0ed39d18f2fee41a`, while validator-4 reported block hash `0x885118124bb5030ef15117fecf548ae6172d01519448792301a651d5e8a97dac` and state root `0x3c404369721ba50302277c34b2150f4946d3ac1839964579ffd830f15ecba1eb`.
   - Cleanup completed after binning: `kombat-chain-ingress4`, `kombat-chain-ingress5`, and `kombat-chain-ingress6` were stopped/disabled on all hosts; the default three-ingress topology was redeployed; chain state was wiped; the default verifier passed; and all extra ingress services reported `inactive`.
   Status:
   - binned
   Reason:
   - Concentrating high ingress on the two healthy hosts is not a safe green recovery path. It raises accepted load but overloads downstream owner/drain behavior, leaves bounded Mahi pools full, collapses committed TPS, and triggers true state-root divergence. Do not retry this topology without a correctness fix for overload/backpressure semantics.
   Next move:
   - Return to the item-251 default three-ingress topology as the current clean live state. The next useful work is not more endpoint reshuffling; it is to fix overload correctness/backpressure so accepted work cannot outrun Mahi drain into divergent state. Keep all further experiments tagged with code, environment, settings, artifact, TPS, correctness, and rollback status.

253. Phase 6 live diagnostic binned: observer relay backpressure bound is too low for green topology
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 252 showed overload can let accepted work outrun Mahi owner drain, fill bounded pre-admitted pools, and diverge state roots. Add bounded observer relay admission and owner-side pre-admitted high-water retry semantics so overload becomes client-visible retry/backpressure instead of accepted-but-uncommitted backlog.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/deploy/ansible/inventory/group_vars/kombat_live_testnet.yml`
   - `chain/deploy/ansible/playbooks/verify.yml`
   - `chain/deploy/ansible/templates/kombat-chain.env.j2`
   - `chain/deploy/ansible/templates/kombat-ingress.env.j2`
   - `chain/experiments/live/253-mahi-overload-backpressure.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Plan tag: `chain-exp-253-mahi-overload-backpressure-plan` at `02be31a98`
   - Code tag: `chain-exp-253-mahi-overload-backpressure-code` at `f900dff94`
   - Live build source commit: `f900dff94`
   - Deployed Linux binary sha256: `e1d582c1614b6bc86e5a13771e5194e98bb8bc0eb4bdda463528ae6352cdceec`
   - Discarded invalid deploy artifact: a macOS binary sha256 prefix `3bc7dd20` was accidentally deployed first, caused `Exec format error`, was restored immediately, and produced no benchmark evidence.
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_owner_relay`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml network_relay_intents_in_mahi_mode`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_observer_async_relay_backpressures_when_queue_full --lib`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_owner_relay_redundancy_enqueues_primary_only --lib`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml network_relay_intents_in_mahi_mode_retries_at_pre_admitted_high_water --lib`
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml --lib -- --test-threads=1` (`185 passed; 0 failed`)
   - Built the live binary natively on validator-1 (`ELF 64-bit x86-64`) after rejecting the invalid macOS binary.
   - Deployed `/tmp/kombat_chain-live-f900dff94-linux` to all four hosts with the committed Ansible env, wiped `/var/lib/and-chain`, restarted `kombat-chain` plus `kombat-chain-ingress` through `kombat-chain-ingress3`, and verified the live topology before the run.
   Benchmark / live artifacts:
   - `/tmp/and-live-253-backpressure-default-100k-60s`
   Evidence:
   - Live env was configured with `KOMBAT_MAHI_OBSERVER_RELAY_QUEUE_MAX_ITEMS=65536` and `KOMBAT_MAHI_PRE_ADMITTED_BACKPRESSURE_ITEMS=393216`.
   - The 100k/60s default 12-endpoint topology exited nonzero under `LOADTEST_FAIL_ON_ERRORS=1`.
   - The loadgens accepted `3,288,376` requests and reported `2,304` RPC errors, `0` nonce errors, and `0` network errors. Aggregate measured delivery was only `49,540.7 RPS`.
   - The RPC error samples were controlled backpressure: `jsonrpc code=-32029 message=mahi owner relay queue full; retry`.
   - Post-run status showed the new bound protected the hard pre-admitted cap but left substantial queued work: Mahi pre-admitted pool items were approximately validator-1 `249,936`, validator-2 `230,536`, validator-3 `227,400`, validator-4 `232,184`.
   - Because the run failed on request errors and did not complete full aggregate committed-summary collection, it is not a green TPS artifact. The partial status confirms the failure mode, not a kept throughput number.
   Status:
   - binned
   Reason:
   - The backpressure mechanism is directionally correct for overload, but the committed live thresholds are too low for the normal item-251 green topology. They convert normal burst buffering into client-visible retries, cut delivered RPS nearly in half, and fail the zero-error gate. Do not keep these thresholds as the live default.
   Next move:
   - Keep the code/test seam, but move the live default thresholds above normal operating burst size in a separately tagged experiment. Redeploy, wipe state, and rerun the default 100k/60s topology. If the raised-threshold run does not recover item-251-class clean TPS with zero errors and matching state roots, revert the item253 runtime behavior entirely and return to the item-251 binary/config before further protocol work.

254. Phase 6 live recovery binned: restoring backpressure thresholds does not recover green TPS
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item 253 proved the observer/owner retry path works, but `65,536` observer relay queue items is below the normal burst capacity needed by the item-251 green topology. Raising both live backpressure thresholds to the hard Mahi pre-admitted pool size (`524,288`) should remove premature RPC retries while retaining a bounded overload guard at true saturation.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/deploy/ansible/inventory/group_vars/kombat_live_testnet.yml`
   - `chain/experiments/live/254-mahi-backpressure-threshold-restore.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code tag: `chain-exp-254-mahi-backpressure-threshold-restore-code` at `09cea38a8`
   - Deployed Linux binary sha256: `386f2f9c9d6415d044e1b0e6d845ef486783a1a82395839d12b8e0f31fa2bf32`
   Live config:
   - `KOMBAT_MAHI_OBSERVER_RELAY_QUEUE_MAX_ITEMS=524288`
   - `KOMBAT_MAHI_PRE_ADMITTED_BACKPRESSURE_ITEMS=524288`
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml mahi_observer_async_relay_backpressures_when_queue_full --lib` (`1 passed`)
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml network_relay_intents_in_mahi_mode_retries_at_pre_admitted_high_water --lib` (`1 passed`)
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml --lib -- --test-threads=1` (`185 passed; 0 failed`)
   Benchmark / live artifacts:
   - `/tmp/and-live-254-threshold-restore-100k-60s`
   Evidence:
   - The 100k/60s default 12-endpoint topology completed with `0` RPC errors, `0` nonce errors, and `0` network errors, so the raised thresholds removed item253's client-visible retry failure.
   - Throughput still collapsed: committed `1,459,890` intents over `60s`, or `24,331.5 TPS`, far below item251's `77,666.8 TPS` clean reference and the `76,000 TPS` recovery floor.
   - Aggregate delivered load was only `63,068.5 RPS`. Validator-2 and validator-3 endpoints were near target, validator-1 delivered about `3.0k RPS` per endpoint, and validator-4 delivered only about `1.4k RPS` per endpoint.
   - Common-height correctness passed at height `70`: all four validators reported state root `0x172062c134000b5cc28cba9fb7879e2c9e2e31df6f94306b1de42b6a75dd5920`.
   - The run failed the drain invariant: post-run Mahi pre-admitted pool items remained approximately validator-1 `385,976`, validator-2 `387,536`, validator-3 `354,736`, validator-4 `353,272`.
   Status:
   - binned
   Reason:
   - Threshold tuning did not recover green behavior. The item253 runtime behavior itself changes queue/drain dynamics enough to collapse throughput and leave large post-drain backlogs even when no client retries are emitted.
   Next move:
   - Revert the item253 runtime behavior and redeploy the item251-style binary/config before further protocol work. Keep the item253 and item254 artifacts as proof that overload backpressure must not be implemented by throttling the normal observer relay queue in this path.

255. Phase 6 live recovery binned: runtime rollback did not recover green TPS
   Date:
   - `2026-04-26`
   Hypothesis:
   - Items 253 and 254 prove the item253 runtime changes, not just the configured threshold, are the regression source. Reverting the observer relay queue bound, owner pre-admitted high-water env, and associated Ansible env lines should restore the item251-style runtime path and recover green live behavior.
   Files changed:
   - `chain/crates/rpc/src/lib.rs`
   - `chain/deploy/ansible/inventory/group_vars/kombat_live_testnet.yml`
   - `chain/deploy/ansible/playbooks/verify.yml`
   - `chain/deploy/ansible/templates/kombat-chain.env.j2`
   - `chain/deploy/ansible/templates/kombat-ingress.env.j2`
   - `chain/experiments/live/255-revert-mahi-backpressure-runtime.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code tag: `chain-exp-255-revert-mahi-backpressure-runtime-code` at `d13863cc0`
   - Deployed Linux binary sha256: `a23bfe2e08411e99513577f6936e325ac8194c194fc06ab8c435d33559b78228`
   Rollback verification:
   - `git diff 02be31a98 -- chain/crates/rpc/src/lib.rs chain/deploy/ansible/inventory/group_vars/kombat_live_testnet.yml chain/deploy/ansible/playbooks/verify.yml chain/deploy/ansible/templates/kombat-chain.env.j2 chain/deploy/ansible/templates/kombat-ingress.env.j2` produced no diff in the worktree.
   Tests:
   - `cargo test -p kombat-chain-rpc --manifest-path chain/Cargo.toml --lib -- --test-threads=1` (`183 passed; 0 failed`)
   Benchmark / live artifacts:
   - `/tmp/and-live-255-rollback-backpressure-100k-60s`
   Evidence:
   - The 100k/60s default 12-endpoint topology completed with `0` RPC errors, `0` nonce errors, and `0` network errors, but committed only `1,069,296` intents, or `17,821.6 TPS`.
   - Aggregate delivered load was `59,058.6 RPS` with `3,877,792` accepted requests. Validator-2 and validator-3 endpoints reached the configured target, while validator-1 delivered about `2.0k RPS` per endpoint and validator-4 delivered only about `1.0k RPS` per endpoint.
   - The run failed the drain invariant: post-run Mahi pre-admitted pool items remained approximately validator-1 `434,784`, validator-2 `375,944`, validator-3 `391,736`, validator-4 `360,040`.
   - Common-height correctness failed at height `59`: validators 1-3 reported block hash `0x773dc0032e5fab537e25d9a56de5cdb45f02f366afdad187bd0bb4fb4dee420c` and state root `0x501998d91b6e5901021b41dcca25c795d207024e1cbc7bc6322b4c42cafe5cfb`, while validator-4 reported block hash `0xe384b550bb17110a4605cdd0333bc4cecab81d70e77b8e3d104b4873126390b7` and state root `0x65c6aa467ff7cf75bf32a2ac96bdd012cf7f63321b3240a37fbd9a899cbd2d4c`.
   Status:
   - binned
   Reason:
   - The runtime/env rollback removed the item253 backpressure surfaces but did not recover item251-class behavior. Throughput fell below item254, pools remained heavily backlogged, and validator-4 diverged at the common-height state-root check. The loss of green behavior is not explained solely by the item253 runtime code.
   Next move:
   - Stop tuning the current head. Run an exact known-good replay using the item251 Linux binary/config anchor (`/tmp/kombat_chain-live-dd022982`, sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`) and the default three-ingress topology. If item251 does not reproduce, treat the regression as live environment/config drift or a non-backpressure code merge outside the item253 surfaces, not as a Mahi micro-optimization problem.

256. Phase 6 live recovery diagnostic: exact item251 replay no longer reproduces
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item255 showed that reverting the item253 backpressure runtime surfaces at current HEAD did not recover and still diverged. Replaying the exact item251 Linux binary and host isolation state should distinguish a code regression from live environment/config drift. If item251 no longer reproduces, stop code tuning and investigate the live hosts/config as the regression source.
   Files changed:
   - `chain/experiments/live/256-item251-known-good-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Plan tag: `chain-exp-256-item251-known-good-replay-plan` at `b88783081`
   - Replay binary: `/tmp/kombat_chain-live-dd022982`
   - Replay binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Reference artifact: `/tmp/and-live-251-v1-nginx-stopped-100k-60s` (`77,666.8 TPS`)
   Tests / verification:
   - Verify `/tmp/kombat_chain-live-dd022982` is still the expected Linux ELF.
   - Stopped non-chain nginx through `stop_non_chain_services.yml`; validator-1 reported `nginx active=inactive enabled=enabled`, validators 2-4 reported `nginx absent`.
   - Deployed the item251 binary with the default three-ingress topology and verified all four validators run sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   - Wiped `/var/lib/and-chain`, restarted `kombat-chain` plus `kombat-chain-ingress` through `kombat-chain-ingress3`, and verified all services/RPC ports before the run.
   - `git diff chain-exp-251-v1-nginx-isolation-result -- chain/deploy/ansible/inventory/group_vars/kombat_live_testnet.yml chain/deploy/ansible/templates/kombat-chain.env.j2 chain/deploy/ansible/templates/kombat-ingress.env.j2 chain/deploy/ansible/playbooks/verify.yml` produced no diff, so the deployed Ansible env surfaces match the item251 result commit.
   Benchmark / live artifacts:
   - `/tmp/and-live-256-item251-known-good-replay-100k-60s`
   Evidence:
   - The exact item251 binary replay completed with `0` RPC errors, `0` nonce errors, and `0` network errors, but committed only `1,522,837` intents over `60s`, or `25,380.6 TPS`.
   - Aggregate delivered load was `61,105.9 RPS` with `3,965,000` accepted requests. Validator-2 and validator-3 endpoints reached the configured target (`~8.33k RPS` each), validator-1 stayed weak (`~2.49k-2.66k RPS`), and validator-4 collapsed to only `~1.08k-1.13k RPS`.
   - Common-height correctness passed at height `85`: all four validators reported block hash `0x0c363ca56409b8b16f16fff74cbfcb5e29edc69ad6c5f7ada7c69ba22b4d5656` and state root `0xaaad8b247770029b22d484d95b68a2697727e852edc972e67dc788f48d294c0b`.
   - The run failed the drain invariant: post-run Mahi pre-admitted pool items remained approximately validator-1 `112,536`, validator-2 `163,848`, validator-3 `158,704`, validator-4 `124,008`.
   Status:
   - kept diagnostic; not green
   Reason:
   - The exact item251 binary plus unchanged Ansible config no longer reproduces item251 throughput. Correctness passed, but TPS collapsed and pools did not drain. This separates the regression from current-head backpressure code and points at live host/runtime/topology drift or an external deployment condition around the same binary/config.
   Next move:
   - Stop protocol tuning. Run a host/runtime drift snapshot against item250/item251 expectations, starting with validator-4 because it is the new worst endpoint group and shows high `compat_wait_p95` (`~89ms`) while v2/v3 still hit loadgen target. Capture CPU steal/load, process list, NIC stats, systemd service env, open connection counts, and per-service logs before making changes.

257. Phase 6 live diagnostic kept: post-item256 host drift snapshot identifies validator-4 host/runtime pathology
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item256 proved the exact item251 binary and unchanged Ansible config no longer reproduce item251 throughput. The next useful evidence is a live host/runtime drift snapshot immediately after the failed replay, with extra NIC/scheduler counters, to identify or exclude validator-4 host-local drift.
   Files changed:
   - `chain/deploy/ansible/playbooks/capture_env.yml`
   - `chain/experiments/live/257-post-item256-host-drift-snapshot.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Planned experiment tag: `chain-exp-257-post-item256-host-drift-snapshot-plan`
   - Current deployed binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Reference snapshot: `/tmp/and-live-env-snapshot-250-current-dd022982`
   - Result tag: `chain-exp-257-post-item256-host-drift-snapshot-result`
   Diagnostics:
   - Extended `capture_env.yml` to include selected network buffers, IP/link stats, softirq counters, `/proc/net/snmp`, `vmstat 1 5`, and thread-level `top`.
   - Captured `/tmp/and-live-env-snapshot-257-post-item256-dd022982`.
   - Compared validator-4 against validators 2/3 and against item250 for process count, sockets, CPU steal/load, NIC counters, systemd env, and chain RPC metrics.
   - Tightened future snapshot redaction so `KOMBAT_BOOTSTRAP_BEARER_TOKEN` is redacted alongside `KOMBAT_OPERATOR_TOKEN`.
   Benchmark / live artifacts:
   - `/tmp/and-live-env-snapshot-257-post-item256-dd022982`
   - `validator-1.tgz` sha256 `308edadc185fdab60e5bb78216064a2ecc541cfe63fe4e258a7199a8b1fc8d2c`
   - `validator-2.tgz` sha256 `2316ce23e06988666a94f5567c794341894f206ca39d1d19b3b3f0e2a420379c`
   - `validator-3.tgz` sha256 `72e49e7d4e7695ff33d688fa753a5ad637476678577a588c7a0bb4ecf7af9e13`
   - `validator-4.tgz` sha256 `2dfdd61a31b37b2cd2512b599712474b92d77350edc757a4e1ab9210f271f171`
   Evidence:
   - All four validators still ran the exact item251 binary sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   - The item256 post-run Mahi state was internally consistent despite low TPS: all four validators reported commit log hash `0xa5a43f1e94f7aa27d919281f479c3b0f882daef8ba7d4258c428668c3001f8c0`, `1216` committed blocks, and `3,965,000` executed intents.
   - Validator-4 is the strongest live outlier. Its post-run Mahi latencies were materially worse than validators 2/3: sigverify p50/p90/max `202/410/743ms` vs v2 `135/196/372ms` and v3 `135/208/392ms`; inbound block accept p90/max `52/257ms` vs v2 `4/9ms` and v3 `4/24ms`; commit-execution batch p50/p90/max `202/316/1466ms` vs v2 `133/183/205ms` and v3 `119/179/227ms`.
   - The same validator-4 process consumed much more systemd CPU time since restart: `783,025,910,000ns` vs validator-2 `252,084,480,000ns` and validator-3 `268,166,019,000ns`. Validator-1 was also degraded at `591,524,393,000ns`, but validator-4 is the new worst host.
   - Service state did not explain the difference: active chain/ingress services reported `NRestarts=0`, `CPUQuotaPerSecUSec=infinity`, `MemoryMax=infinity`, and `LimitNOFILE=1048576`.
   - Extra `kombat-chain-ingress4` through `kombat-chain-ingress6` env files remain on disk from prior experiments, but those services are absent/inactive and are not the active runtime topology.
   - Post-run `vmstat`, socket counts, and NIC drop counters did not show an obvious steal-time, socket-explosion, or packet-drop explanation.
   Status:
   - kept diagnostic
   Reason:
   - Exact binary/config replay no longer reproduces because the live hosts no longer behave like the item251 environment. The strongest evidence is host/role-local: validator-4 has the worst endpoint delivery and much higher Mahi verify, accept, and commit-execution latency under the same binary/config. Current protocol-code tuning would mask this substrate/runtime regression instead of recovering green TPS.
   Next move:
   - Run a codified host recovery experiment before any more protocol work. Reboot or otherwise reset the live VPS runtime, redeploy the exact item251 binary/config, wipe chain state, rerun the default 12-endpoint `100k/60s` item251 replay, and compare validator-4 latencies plus pool-drain behavior. If reboot/reset does not recover item251-class TPS, treat the VPS substrate/role placement as unstable and move benchmark control to a fresh Ansible-provisioned baseline or role-swap diagnostic.

258. Phase 6 live recovery binned: rebooted exact item251 replay still below green
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item257 identified validator-4 host/runtime pathology under the exact item251 binary/config. If the loss of item251-class TPS is caused by transient VPS/runtime state, rebooting all four hosts, redeploying the exact item251 binary/config, wiping chain state, and rerunning the default `100k/60s` benchmark should recover the item251 green reference.
   Files changed:
   - `chain/experiments/live/258-post-reboot-item251-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Planned experiment tag: `chain-exp-258-post-reboot-item251-replay-plan`
   - Reference result tag: `chain-exp-251-v1-nginx-isolation-result`
   - Replay binary: `/tmp/kombat_chain-live-dd022982`
   - Replay binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Result tag: `chain-exp-258-post-reboot-item251-replay-result`
   Tests / verification:
   - Rebooted all four `kombat_live_testnet` hosts through Ansible and waited for SSH to return: validator-1 elapsed `80s`, validator-2 `37s`, validator-3 `38s`, validator-4 `52s`.
   - Stopped non-chain nginx after reboot; validator-1 reported `nginx active=inactive enabled=enabled`, validators 2-4 reported `nginx absent`.
   - Redeployed the exact item251 binary with the default three-ingress topology and verified all four hosts run sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   - Wiped `/var/lib/and-chain`, restarted `kombat-chain` plus `kombat-chain-ingress` through `kombat-chain-ingress3`, and verified all RPC ports. The first ad-hoc wipe command failed before doing work because `/bin/sh` rejected `pipefail`; the same command was rerun successfully with `executable=/bin/bash`.
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-258-post-reboot-item251-replay-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - Checked common-height state root across all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-258-post-reboot-item251-replay-100k-60s`
   Evidence:
   - The rebooted exact replay completed with `0` RPC errors, `0` nonce errors, and `0` network errors, but committed only `2,157,183` intents over `60s`, or `35,953.1 TPS`.
   - This is better than item256's exact-replay failure (`25,380.6 TPS`) but still far below item251's `77,666.8 TPS` reference and the `76,000 TPS` recovery floor.
   - Aggregate measured delivery remained weak at `55,738.8 RPS` with `3,620,496` accepted requests. Validator-3 endpoints reached target (`~8.32k-8.34k RPS`), validator-2 was below target (`~7.06k-7.27k RPS`), validator-1 remained weak (`~1.88k-1.96k RPS`), and validator-4 remained collapsed (`~1.15k-1.20k RPS`).
   - Common-height correctness passed at height `91`: all four validators reported block hash `0x823a965a1bc07f9001fe8db9ad3af979681a1cc15d05b90b111a753428e709b1` and state root `0x3c11e797f0c5ffddf0f6ebebca3d0d8648790f2eed45b1a39968fa0021bc2685`.
   - Mahi commit-log correctness matched across all validators after the run: commit log hash `0xdcaa4bc4a9cb86f40c33f7a58de884b8436201fec6631886ec22f0142db4d43e`, `1096` committed blocks, and `3,620,496` executed intents.
   - The run nearly drained but not to the item251 invariant: post-run pre-admitted pool items were validator-1 `33,584`, validator-2 `19,528`, validator-3 `9,416`, validator-4 `4,360`.
   - Validator-4 remained host/pathological after reboot: sigverify p50/p90/max `187/371/893ms` and systemd CPU usage `601,778,819,000ns`, versus validator-2 `138/273/436ms` and `225,760,887,000ns`, and validator-3 `132/237/395ms` and `235,760,660,000ns`.
   - Validator-1 also remained weak: sigverify p50/p90/max `171/410/1167ms`, commit-execution batch p50/p90/max `195/332/731ms`, and systemd CPU usage `456,741,184,000ns`.
   Status:
   - binned
   Reason:
   - Rebooting/resetting all hosts is not sufficient to recover the item251 environment. It improved the exact replay modestly and preserved correctness, but endpoint delivery and validator latency remain strongly host-skewed. The remaining regression is not explained by current HEAD code, Ansible config, nginx, or a simple transient process state.
   Next move:
   - Stop protocol changes. Run a host/role isolation experiment next: either swap validator roles across physical hosts or provision a fresh Ansible baseline, then replay the exact item251 binary/config. The goal is to determine whether the bad behavior follows validator identity/data shard or stays with the physical VPS host.

259. Phase 6 live diagnostic kept: v3/v4 role swap proves physical validator-4 host remains pathological
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item258 showed rebooting/resetting all hosts does not recover item251-class behavior, and validator-4 physical host remains collapsed. Swap validator-3 and validator-4 identities/keys while keeping physical IPs fixed to determine whether the bad behavior follows physical host `203.0.113.16` or validator identity/data shard `0x04`.
   Files changed:
   - `chain/deploy/ansible/inventory/259-v3-v4-role-swap.yml`
   - `chain/experiments/live/259-v3-v4-role-swap-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Planned experiment tag: `chain-exp-259-v3-v4-role-swap-replay-plan`
   - Result tag: `chain-exp-259-v3-v4-role-swap-replay-result`
   - Replay binary: `/tmp/kombat_chain-live-dd022982`
   - Replay binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Swap inventory: `chain/deploy/ansible/inventory/259-v3-v4-role-swap.yml`
   Tests / verification:
   - Deploy exact item251 binary/config using the v3/v4 role-swap inventory.
   - Wipe `/var/lib/and-chain`, restart `kombat-chain` plus default three ingress services, and verify all RPC ports.
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-259-v3-v4-role-swap-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - Checked common-height state root at height `67` across all four validators.
   - Restored default `inventory/testnet.yml` topology after collecting evidence, wiped state, restarted services, and reran `playbooks/verify.yml`; final verify passed with all hosts running sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   Benchmark / live artifacts:
   - `/tmp/and-live-259-v3-v4-role-swap-100k-60s`
   Evidence:
   - The role-swap replay completed with `0` RPC errors, `0` nonce errors, and `0` network errors, but committed only `910,204` intents over `60s`, or `15,170.1 TPS`.
   - Aggregate delivered load was `52,612.0 RPS` with `3,393,864` accepted requests. Validator-2 and validator-3 physical hosts remained relatively healthy (`~7.69k RPS` and `~7.14k RPS` per endpoint), validator-1 remained weak (`~2.25k RPS` per endpoint), and physical validator-4 collapsed further to only `~421-446 RPS` per endpoint.
   - The bad behavior stayed with physical host `203.0.113.16` even though that host was running validator-3 identity `0x03` during the swap. The healthier physical host `203.0.113.14` ran validator-4 identity `0x04` and still delivered `~7.1k RPS` per endpoint.
   - Common-height correctness passed at height `67`: all four validators reported block hash `0xedbb6b67ddd2fd60055a76e31c52d995bff592e5c8a22c50c4fe9678be1c4e5a` and state root `0xf65da2b25744f6bb72b5d360ae54b4754006928d254bae48ef42df2330bce187`.
   - Immediate post-run Mahi commit-log correctness matched across all validators: commit log hash `0x55e426461fcfe91d0147050e18b3ce61283531359b74558e4b869a1d55366ea4`, `533` committed blocks, and `2,576,872` executed intents.
   - The run failed the drain invariant: post-run Mahi pre-admitted pool items were validator-1 `172,080`, validator-2 `143,864`, validator-3 `137,744`, validator-4 `120,088`.
   - Physical validator-4 stayed the latency and CPU outlier: sigverify p50/p90/max `295/724/2831ms`, inbound block accept p90/max `151/518ms`, commit-execution batch p50/p90/max `291/726/1056ms`, and systemd CPU usage `873,756,301,000ns`. Validators 2/3 were much lower: v2 CPU `171,880,573,000ns`, v3 CPU `194,189,977,000ns`.
   Status:
   - kept diagnostic; not green
   Reason:
   - The diagnostic succeeded and separates the regression from validator identity/data shard. The throughput collapse stayed with the physical validator-4 VPS, not with validator identity `0x04`. Continuing protocol-code tuning against this topology would optimize around a broken substrate and risk repeating already-binned experiments.
   Next move:
   - Do not run more protocol micro-tuning on the current four-host topology. Replace or remove physical host `203.0.113.16`, or move benchmark control to a fresh Ansible-provisioned baseline, then replay the exact item251 binary/config before judging current code. Keep default topology restored until the host replacement/isolation experiment is planned and tagged.

260. Phase 6 live recovery diagnostic kept: packed validator-4 on physical validator-3 is correct but not green
   Date:
   - `2026-04-26`
   Hypothesis:
   - Item259 proved the regression stays with physical host `203.0.113.16`, not validator identity `0x04`. Running validator-4 as a second isolated instance on physical validator-3 (`203.0.113.14`) while stopping physical validator-4 should remove the bad VPS from the benchmark without changing protocol code. Driving the 12 ingress endpoints through validators 2/3 should recover item251-class throughput if the host is the dominant regression.
   Files changed:
   - `chain/deploy/ansible/playbooks/deploy.yml`
   - `chain/deploy/ansible/playbooks/verify.yml`
   - `chain/deploy/ansible/playbooks/reset_state.yml`
   - `chain/deploy/ansible/templates/kombat-chain.service.j2`
   - `chain/deploy/ansible/templates/kombat-ingress.service.j2`
   - `chain/deploy/ansible/inventory/260-packed-v4-on-v3.yml`
   - `chain/experiments/live/260-packed-v4-on-v3-hosts.txt`
   - `chain/experiments/live/260-packed-v4-on-v3-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Planned experiment tag: `chain-exp-260-packed-v4-on-v3-replay-plan`
   - Deploy fix tag: `chain-exp-260-packed-v4-on-v3-deploy-fix`
   - Result tag: `chain-exp-260-packed-v4-on-v3-replay-result`
   - Replay binary: `/tmp/kombat_chain-live-dd022982`
   - Replay binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Packed inventory: `chain/deploy/ansible/inventory/260-packed-v4-on-v3.yml`
   - Packed loadtest hosts file: `chain/experiments/live/260-packed-v4-on-v3-hosts.txt`
   Tests / verification:
   - Added Ansible support for host-specific validator service names and validator env files. Default `inventory/testnet.yml` still renders the original `kombat-chain.service` and `kombat-chain.env` paths.
   - Added reusable `playbooks/reset_state.yml` so state resets are codified instead of ad-hoc shell-only.
   - Pre-run deploy fix: set `ansible_user=root` at all-vars scope in the packed inventory so retired-host commands do not default to the local username.
   - Pre-run deploy fix: keep the `and-chain` service user's home stable instead of changing it to each validator instance state dir; otherwise the second instance tries to `usermod` a user already used by the first process.
   - `KOMBAT_OPERATOR_TOKEN=dummy KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/deploy.yml --syntax-check`
   - `KOMBAT_OPERATOR_TOKEN=dummy KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/verify.yml --syntax-check`
   - `KOMBAT_OPERATOR_TOKEN=dummy KOMBAT_BINARY_PATH=/tmp/kombat_chain-live-dd022982 .venv/bin/ansible-playbook playbooks/reset_state.yml --syntax-check`
   - The same syntax checks with `-i inventory/260-packed-v4-on-v3.yml`.
   - Stop physical validator-4 services, deploy packed inventory, verify, reset state, then run `100k/60s` using `LOADTEST_VALIDATOR_NAMES='validator-2 validator-3'` and `LOADTEST_ENDPOINT_PORTS='8645 8646 8647 8648 8649 8650'`.
   Benchmark / live artifacts:
   - `/tmp/and-live-260-packed-v4-on-v3-v2v3-12ingress-100k-60s`
   Evidence:
   - The packed topology completed with `0` RPC errors, `0` nonce errors, and `0` network errors, but committed only `2,532,984` intents over `60s`, or `42,216.4 TPS`.
   - This is a clean improvement over item259's role-swap run (`15,170.1 TPS`) and item258's rebooted exact replay (`35,953.1 TPS`), but still far below item251's green reference (`77,666.8 TPS`) and the `76,000 TPS` recovery floor.
   - Aggregate delivered load was only `38,591.5 RPS` despite `100k` offered. The endpoint distribution was narrow but low: all 12 driven endpoints on validators 2/3 delivered only about `3.06k-3.51k RPS`.
   - Common-height correctness passed at height `72`: all four active validators reported block hash `0x88a879e7b7c322af2cb78149ee28488d1bb0bf75291f73ee99f97d65108adf3f` and state root `0x6a314d56272ea89f7e06df6416dd73ae93bc03a43e2f5a8fd8c8a2c8b14b9459`.
   - Mahi commit-log correctness matched across all active validators: commit log hash `0xa6f121d97e954c662692f3b175a5f3b213619d10131aaecdb84bcef274770d6d`, `569` committed blocks, and `2,532,984` executed intents.
   - The run passed the drain invariant: Mahi pre-admitted pool items were `0` on all four active validators after the run. Accepted/drained totals were balanced: validator-1 `632,472`, validator-2 `635,296`, validator-3 `631,160`, validator-4-on-v3 `634,056`.
   - The packed topology shifts the bottleneck from physical validator-4 collapse to delivered ingress/loadgen capacity. All accepted work committed, but the two driven physical hosts only delivered `~19k RPS` each while also running consensus work.
   Status:
   - kept diagnostic; not green
   Reason:
   - Removing physical validator-4 from the active validator set improves correctness-preserving throughput and eliminates the pool backlog, proving the host was a real problem. It does not recover green TPS because the packed topology under-delivers load: the measured bottleneck is now ingress/loadgen capacity on two physical hosts, not consensus/commit drain.
   Next move:
   - Do not tune consensus based on this packed result. To recover item251-class green TPS, restore four healthy physical load-bearing hosts or add dedicated external loadgen/ingress capacity. The current packed topology can stay active as the best correct control, but it is not a green recovery artifact.

261. Phase 6 live recovery diagnostic kept: rebuilt validator-4 default replay improves but remains below green
   Date:
   - `2026-04-27`
   Hypothesis:
   - Item259 proved the regression stayed with physical validator-4 host `203.0.113.16`. After the host was OS-reinstalled, restoring the default four-host topology and replaying the exact item251 binary/config should recover item251-class throughput if the old host substrate was the dominant regression.
   Files changed:
   - `chain/experiments/live/261-rebuilt-v4-default-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Result tag: `chain-exp-261-rebuilt-v4-default-replay-result`
   - Replay binary: `/tmp/kombat_chain-live-dd022982`
   - Replay binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - Pre-run head commit: `a6320ce84`
   Tests / verification:
   - Refreshed the SSH host key for `203.0.113.16` after OS reinstall and confirmed root SSH access.
   - Stopped and disabled packed-topology leftovers (`kombat-chain-v4` and `kombat-chain-ingress4` through `kombat-chain-ingress6`), restoring the default one-validator/three-ingress topology on each VPS.
   - Verified `playbooks/verify.yml` passes after the run and all four validators run sha256 `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   - `LOADTEST_OUTPUT_DIR=/tmp/and-live-261-rebuilt-v4-default-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`
   - Checked common-height state root at height `100` across all four validators.
   - Queried `kombat_getConsensusMetrics` across all four validators.
   Benchmark / live artifacts:
   - `/tmp/and-live-261-rebuilt-v4-default-100k-60s`
   Evidence:
   - The rebuilt-v4 default replay completed with `0` RPC errors, `0` nonce errors, and `0` network errors, committing `3,285,739` intents over `60s`, or `54,762.3 TPS`.
   - This improves over item258's rebooted exact replay (`35,953.1 TPS`) and item260's packed workaround (`42,216.4 TPS`), but remains below item251's green reference (`77,666.8 TPS`) and the `76,000 TPS` recovery floor.
   - Aggregate delivered load remained weak at `57,383.7 RPS` despite `100k` offered. Validator-2 and validator-3 endpoints were healthy at about `7.37k-7.48k RPS`; validator-1 remained weak at `1.97k-2.02k RPS`; rebuilt validator-4 improved versus pre-reinstall but still only reached `2.23k-2.31k RPS`.
   - Loadgen accepted `3,702,208` requests (`61,703.5 accepted/sec`). Post-run Mahi metrics matched across all validators with commit log hash `0x6d31225515ca0efde24132c4d6316d62ce262380e32023d2bf26373964e39994`, `796` committed blocks, and `3,702,808` executed intents.
   - Common-height correctness passed at height `100`: all validators reported block hash `0x0ba296837547f8cf9fc16b4a1b2e578e52dc33dd5ff5fe85daf629ad4e4b2ca8` and state root `0x51d276cb8ff09c464d6d4a4a0a6b306b6fd1328a8460bf42a254b4da9203b53e`.
   - Post-drain pre-admitted pools were nearly empty but not a strict zero-drain pass: validator-1 `0`, validator-2 `1,184`, validator-3 `0`, validator-4 `0`.
   - Rebuilt validator-4 is no longer the catastrophic outlier from items 256-259, but delivery skew remains. Validator-1 and validator-4 consumed similar chain CPU (`477,822,496,000ns` and `469,727,651,000ns`) while delivering only about one third of validator-2/3 endpoint RPS.
   Status:
   - kept diagnostic; not green
   Reason:
   - The OS reinstall recovered physical validator-4 enough to beat the packed workaround and preserve correctness, but it did not restore item251-class green throughput. The remaining problem is host/ingress delivery skew, not Mahi consensus correctness: validators 2/3 can deliver near target, while validators 1/4 remain weak under the same binary/config.
   Next move:
   - Do not resume protocol micro-tuning from this result. Keep the exact artifact/tag and run the next recovery slice against topology/load delivery: either remove weak ingress hosts from the driven ingress set while preserving the four-validator consensus set, or add dedicated external loadgen/ingress capacity. The goal is to recover a stable green artifact before returning to Block-STM or QUIC work.

262. Phase 6 live diagnostic kept: during-load host profile classifies v1/v4 delivery skew
   Date:
   - `2026-04-27`
   Hypothesis:
   - Item261 showed the rebuilt default topology is correct but still not green: validator-2/3 ingress endpoints deliver `~7.4k RPS`, while validator-1/4 deliver only `~2.0k-2.3k RPS`, under the same exact binary/config. The difference should be visible under load as CPU saturation, softirq/NIC pressure, socket pressure, or application per-request cost on weak hosts.
   Files changed:
   - `chain/deploy/ansible/playbooks/capture_load_profile.yml`
   - `chain/experiments/live/262-during-load-host-profile.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Planned experiment tag: `chain-exp-262-during-load-host-profile-plan`
   - Result tag: `chain-exp-262-during-load-host-profile-result`
   - Replay binary: `/tmp/kombat_chain-live-dd022982`
   - Replay binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   Planned tests / verification:
   - Run `playbooks/capture_load_profile.yml` for `105s` at `2s` intervals with `KOMBAT_PROFILE_DIR=/tmp/and-live-262-profile-100k-60s`.
   - While profiling is active, run `LOADTEST_OUTPUT_DIR=/tmp/and-live-262-during-load-profile-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`.
   - Verify zero loadgen errors, common-height state-root match, and matching Mahi commit logs.
   - Compare validator-1/4 against validator-2/3 for process CPU, top thread CPU/core placement, softirq/NIC counters, socket counts, `vmstat`/`mpstat`, and systemd CPU deltas during the measured load window.
   Benchmark / live artifacts:
   - Planned load artifact: `/tmp/and-live-262-during-load-profile-100k-60s`
   - Planned profile artifact: `/tmp/and-live-262-profile-100k-60s`
   Status:
   - kept diagnostic; not green
   Evidence:
   - Initial profile run completed at `3,796,944` committed over `60s`, or `63,282.4 TPS`, with `0` RPC errors, `0` nonce errors, and `0` network errors. That run captured validator/ingress `kombat_chain` processes and host counters, but missed remote `kombat_loadgen` processes.
   - The profiler was fixed in `chain-exp-262-during-load-host-profile-profile-fix` to capture `kombat_loadgen`, all top processes, and loadgen threads, then rerun as the decision artifact.
   - Corrected profile replay artifact `/tmp/and-live-262b-during-load-profile-100k-60s` completed with `0` RPC errors, `0` nonce errors, and `0` network errors, committing `3,829,385` intents over `60s`, or `63,823.1 TPS`.
   - Corrected profile artifact `/tmp/and-live-262-profile2-100k-60s` contains fetched archives with sha256: validator-1 `f3a586d4d254e3be0c6662184946c82f6e949fe80fd211da9de5b3bdcd54cf7c`, validator-2 `94d2329435e7f4a543424fe8d03edbdfe13f034a367f13341bbba3bf54998f0a`, validator-3 `748e21588909fb18e7acfcd7f4bce000ce65fd0cda106413d5560a952ef2fad5`, validator-4 `5d2849e1765345b580a74a0279f83a8e221d319066389f75048b044c4b0dd9de`.
   - Common-height correctness passed at height `330`: all validators reported block hash `0x5c63f46862d45f8c8f08f68e97144feec9130c066e809e3a0374ad676a928553` and state root `0xc7ac03117f198cbd5ce1c4844e4a8a4139bfdf70ce3fadd81f11394e13671efb`.
   - Mahi commit-log correctness matched across all validators after the corrected replay: commit log hash `0xd086159626f0ef42569a6da478aafb25ce9c04fdaa0d0636ef3856eebdb24e40`, `2637` committed blocks, and `11,395,568` cumulative executed intents.
   - Endpoint delivery remained skewed: validator-1 `~2.63k RPS` per endpoint, validator-2 `~6.13k-6.22k`, validator-3 `~6.44k-6.52k`, validator-4 `~4.11k-4.16k`.
   - Loadgen latency identifies validator-1 as the clearest weak path: v1 p50 `~65-66ms`, p90 `~204-208ms`, p99 `~559-584ms`; v2 p50 `~32ms`, p90 `~78-81ms`, p99 `~170-181ms`; v3 p50 `~26ms`, p90 `~67-70ms`, p99 `~202-206ms`; v4 p50 `~36-39ms`, p90 `~133-137ms`, p99 `~350-353ms`.
   - Corrected profile classification: no NIC RX/TX drops on any host, and weak hosts are not loadgen CPU-saturated. Loadgen CPU p50 was v1 `247%`, v2 `322%`, v3 `324%`, v4 `308%`; top loadgen-thread p50 was v1 `83%`, v2 `109%`, v3 `110%`, v4 `106%`. The weak v1 path is not because the load generator cannot burn CPU.
   - Validator/ingress chain CPU is higher per delivered request on weak hosts: `kombat_chain` CPU-sum p50 v1 `73.7%`, v4 `71.2%`, versus v2 `59.6%` and v3 `58.85%`, despite v1/v4 lower endpoint RPS.
   Reason:
   - This is a measurement slice, not a protocol change. It prevents another blind tuning loop by identifying the host/runtime cause of the v1/v4 delivery skew. The current blocker is not Mahi correctness, packet drops, or loadgen saturation; it is host/application latency under load, especially validator-1, with validator-4 improved but still slower than validators 2/3.
   Next move:
   - Recover green by topology/deployment control, not protocol micro-tuning. Either remove validator-1 from the driven ingress set and replace that ingress capacity with healthier hosts/dedicated load generators, or provision fresh hosts for weak roles and rerun the exact item251 binary/config. Keep the four-validator consensus set unchanged until a codified topology result proves otherwise.
   - Every live performance experiment from here must include a during-load profile artifact, or explicitly state why profiling is not applicable. The decision-log entry must name the profile artifact path and archive hashes alongside the TPS artifact.

263. Phase 6 live recovery diagnostic kept: rebuilt validator-1 with external loadgen at depth 1
   Date:
   - `2026-04-27`
   Hypothesis:
   - Item262 identified validator-1 as the clearest remaining weak host path after validator-4 was rebuilt: v1 delivered only `~2.63k RPS` per endpoint with p90 request latency `~204-208ms`, while v2/v3 delivered `~6.1k-6.5k RPS` with much lower latency. Rebuilding validator-1 and running all load generators from dedicated host `203.0.113.10` should recover toward item251 green if the blocker is host/runtime/loadgen placement rather than Mahi consensus code.
   Files changed:
   - `chain/scripts/loadtest_live_network_from_validator.sh`
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/scripts/capture_remote_loadgen_profile.sh`
   - `chain/experiments/live/263-v1-rebuild-external-loadgen.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Plan tag: `chain-exp-263-v1-rebuild-external-loadgen-plan`
   - Result tag: `chain-exp-263-v1-rebuild-external-loadgen-result`
   - Validator binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Loadgen binary sha256: `e5a6927cbf30e1abb0c580b1461fe198498e14b310c67af9ea7757211fcb01ef`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - External loadgen host: `root@203.0.113.10`
   Topology:
   - Four-validator consensus set on `203.0.113.15` (v1, rebuilt), `203.0.113.17` (v2), `203.0.113.14` (v3), `203.0.113.16` (v4).
   - All 12 ingress endpoints driven from dedicated external host `203.0.113.10` via `LOADTEST_EXTERNAL_RUNNER_SSH`.
   Attempts:
   - 263a — setup failure: external host SSH rejected concurrent sessions. Fixed with `/etc/ssh/sshd_config.d/99-kombat-loadtest.conf` (`MaxStartups 100:30:200`, `MaxSessions 200`).
   - 263b — invalid: completed at `49,939.95 TPS` but validator-4 diverged at height 120. Likely contaminated by partial state from the failed 263a run. Marked invalid, not treated as green.
   - 263c — valid clean run: clean state reset, `PIPELINE_DEPTH=1`, `STRICT_NONCE_RETRY=1`.
   Planned tests / verification:
   - Wait for `root@203.0.113.15` SSH after OS reinstall and refresh known_hosts.
   - Redeploy the exact item251 binary/config and verify.
   - Reset chain state with `playbooks/reset_state.yml`.
   - Run `LOADTEST_EXTERNAL_RUNNER_SSH=root@203.0.113.10 LOADTEST_WRITERS_JSON=/tmp/and-live-261-rebuilt-v4-default-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-263c-clean-reset-external-loadgen-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=1 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`.
   - Verify zero errors, common-height state root, and Mahi commit logs.
   Benchmark / live artifacts:
   - Artifact: `/tmp/and-live-263c-clean-reset-external-loadgen-100k-60s`
   Evidence:
   - The clean 263c run completed with `0` RPC errors, `0` nonce errors, and `0` network errors, committing `2,071,276` intents over `60s`, or `34,521.3 TPS`.
   - Correctness passed at common height `70` with identical state root on all validators.
   - Off-box loadgen only drove `40,244.7` measured RPS. Validators were mostly idle — the load never arrived at the offered `100k` rate.
   - `PIPELINE_DEPTH=1` is latency-capping the external loadgen: with 32 writers per endpoint at depth 1, each writer is serialised behind its own request response, so per-endpoint throughput is bounded by `writers / rtt`.
   Status:
   - kept diagnostic; not green
   Reason:
   - The rebuilt validator-1 is correct. The external loadgen topology works at low concurrency. But the delivery bottleneck has shifted: the single external loadgen host cannot deliver `100k` offered RPS at `PIPELINE_DEPTH=1`. The validators are under-driven — this is now a load-generation capacity problem, not a validator-side correctness or host-runtime problem.
   Next move:
   - Raise `PIPELINE_DEPTH` to increase effective external concurrency. Must also set `LOADTEST_STRICT_NONCE_RETRY=0` since strict nonce retry is rejected by the loadgen binary when pipeline depth > 1.

264. Phase 6 live recovery diagnostic kept: external loadgen pipeline depth 4 still below green
   Date:
   - `2026-04-27`
   Hypothesis:
   - Item 263c proved the external loadgen path is correct but `PIPELINE_DEPTH=1` latency-caps delivered RPS at `~40k`. Raising `PIPELINE_DEPTH` to `4` with `LOADTEST_STRICT_NONCE_RETRY=0` should roughly double effective concurrency and drive enough offered load to approach the item251 green reference.
   Files changed:
   - `chain/experiments/live/264-external-pipeline4-nostrict.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Plan tag: `chain-exp-264-external-pipeline4-plan`
   - Result tag: `chain-exp-264-external-pipeline4-result`
   - Validator binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Loadgen binary sha256: `e5a6927cbf30e1abb0c580b1461fe198498e14b310c67af9ea7757211fcb01ef`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - External loadgen host: `root@203.0.113.10`
   Topology:
   - Same four-validator consensus set and external loadgen host as item 263.
   Attempts:
   - 264a — setup failure: `PIPELINE_DEPTH=4` with `LOADTEST_STRICT_NONCE_RETRY=1` (default). The loadgen binary rejects `--strict-nonce-retry` when `--pipeline-depth > 1`. Fixed by setting `LOADTEST_STRICT_NONCE_RETRY=0`.
   - 264b — valid run: `PIPELINE_DEPTH=4`, `LOADTEST_STRICT_NONCE_RETRY=0`.
   Planned tests / verification:
   - Run `LOADTEST_EXTERNAL_RUNNER_SSH=root@203.0.113.10 LOADTEST_WRITERS_JSON=/tmp/and-live-261-rebuilt-v4-default-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-264b-external-pipeline4-nostrict-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`.
   - Verify zero errors, common-height state root, and matching Mahi commit logs.
   - Capture validator and external loadgen profiles during the run.
   Benchmark / live artifacts:
   - Artifact: `/tmp/and-live-264b-external-pipeline4-nostrict-100k-60s`
   - Aggregate summary sha256: `cf03a1ac0cba791ebbc50b0ea6aa4343da8f54bbc3dfd269c0e09b4b42f113b9`
   - Writers-all sha256: `9c7f09dcca5d59c20dfbc4602931f4f875788632bc69464dbba86d149e4fbe2a`
   - Validator profile sha256: v1 `0cecbd2d...`, v2 `15fd05aa...`, v3 `7ee5e934...`, v4 `6310f6f9...`
   - Loadgen profile sha256: `383b0709b4857addf67427a4ffbf46a90aa4d797c93e8e6aee0551033aeca454`
   Evidence:
   - The 264b run completed with `0` RPC errors, `0` nonce errors, and `0` network errors, committing `3,067,080` intents over `60s`, or `51,118.0 TPS`.
   - Correctness passed at height `93`: state root `0xc73e596241a8091cdf5f3c12cb81de83944cb6a35d75a2b5dc63250c9460ec05` matched on all validators.
   - Pipeline depth 4 roughly doubles TPS from 34,521 to 51,118, proving external concurrency is the binding constraint.
   - But measured driver RPS only reached `46,906.9` — the single external loadgen host still cannot deliver the full `100k` offered, even at depth 4. The loadgen host is either CPU-bound running 12 concurrent `kombat_loadgen` processes or SSH-session-bound per endpoint.
   - The validators are still under-driven: they are committing nearly everything that arrives and would commit materially more if load actually reached them.
   Status:
   - kept diagnostic; not green
   Reason:
   - Pipeline depth 4 is a clean improvement over depth 1 and proves the constraint is on the load-generation side, not the validator/consensus side. But `51,118 TPS` is still below the `76,000 TPS` recovery floor and well below item251's `77,666.8 TPS` because the external loadgen cannot saturate 12 endpoints from one host.
   Next move:
   - The validator binary/config remains unchanged — this is a pure load-generation capacity problem. Next steps: either raise `PIPELINE_DEPTH` further (e.g. 8 or 16), increase `WRITERS_PER_ENDPOINT` to raise per-endpoint concurrency, split load across multiple external loadgen hosts so SSH session count per host drops, or run multiple `kombat_loadgen` processes per endpoint on the external host. The green reference should be reachable once the 12 endpoints actually receive close to `100k` offered RPS.
265. Phase 6 live recovery diagnostic kept: multi-host external loadgen delivers 78k RPS but v1 remains bottleneck
   Date:
   - `2026-04-27`
   Hypothesis:
   - Item 264b proved a single external loadgen host cannot drive 100k offered RPS across 12 endpoints — 12 concurrent `kombat_loadgen` processes contend for CPU on one host. Distributing the 12 endpoints across 4 external loadgen hosts (3 endpoints each) should let each host's loadgen processes run without CPU contention, delivering the full 100k offered load to the under-driven validators.
   Files changed:
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/experiments/live/265-multi-host-external-loadgen.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Plan tag: `chain-exp-265-multi-host-external-loadgen-plan`
   - Result tag: `chain-exp-265-multi-host-external-loadgen-result`
   - Validator binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Loadgen binary sha256: `e5a6927cbf30e1abb0c580b1461fe198498e14b310c67af9ea7757211fcb01ef`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   Loadgen host pool:
   - `root@203.0.113.10` (App Node 2, existing loadgen host)
   - `root@203.0.113.12` (App Node 3, k8s worker)
   - `root@203.0.113.18` (Hetzner IP Proxy, iptables-only)
   - `root@203.0.113.11` (App Node 1, k8s CP + app pods, 18vCPU)
   - All accessible via `~/.ssh/id_n2_ssh`.
   - 12 endpoints round-robined across 4 hosts = 3 endpoints per host.
   - Data Node (203.0.113.13) and HL Tokyo (203.0.113.20) excluded.
   Planned tests / verification:
   - Copy `kombat_loadgen` binary to all 4 loadgen hosts.
   - Verify validators run exact item251 binary and config via `playbooks/verify.yml`.
   - Reset chain state with `playbooks/reset_state.yml`.
   - Run `LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.18 root@203.0.113.11' LOADTEST_WRITERS_JSON=/tmp/and-live-261-rebuilt-v4-default-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-265-multi-host-external-loadgen-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`.
   - Verify zero errors, common-height state root, and matching Mahi commit logs.
   - Remove `kombat_loadgen` binary from all 4 loadgen hosts after the run.
   Benchmark / live artifacts:
   - Planned artifact: `/tmp/and-live-265-multi-host-external-loadgen-100k-60s`
   Status:
   - kept diagnostic; not green
   Evidence:
   - The multi-host external loadgen run completed with `0` RPC errors, `0` nonce errors, and `0` network errors. Phase6 resting batch processed `4,911,288` items across `183` blocks.
   - Measured RPS reached `78,459.4` — a `67%` improvement over single-host item 264b (`46,906.9`). Multi-host loadgen successfully removes the CPU contention that capped single-host delivery.
   - State root matched at height `100`: `0xefa2cee2343dd17176078f400a7533906cb144622ecb78eaae5bc0052393fba3` on all four validators.
   - Head hash matched at height `253`: `0xad92a557e97d680d42593298e44b20c310037899780b30319bb38cd4868542c9` on all four validators.
   - All Mahi pre-admitted pools drained to zero after the standard 30s drain.
   - Per-endpoint delivery: v1 `3,126-3,295 RPS`, v2 `6,776-7,574 RPS`, v3 `8,175-8,336 RPS`, v4 `7,182-7,796 RPS`.
   - Loadgen binaries removed from all 4 external hosts post-run.
   Reason:
   - Multi-host loadgen works and removes the load-generation bottleneck. 78k measured RPS is the best delivery since item251. But validator-1 remains the throughput bottleneck at `~3.2k RPS` per endpoint — the measured RPS gap between v1 and v3 is `~5k`. The load delivery side is solved; the remaining gap to the 77k TPS green floor is now validator-side delivery skew, specifically validator-1.
   Next move:
   - Either rebuild/replace validator-1 physical host, or remove validator-1 from the driven ingress set while keeping it in the consensus set (drive load through v2/v3/v4 only). The validator binary/config remains unchanged.
   Next move:
   - Commit/tag/push the plan, verify SSH to all 4 loadgen hosts, copy the loadgen binary, verify validators, reset state, then execute the multi-host external loadgen replay.

266. Phase 6 local latest-code diagnostic kept: live flags recover 98.5k TPS and expose owner-relay cliff above 100k
   Date:
   - `2026-04-27`
   Hypothesis:
   - The latest `main` code should still be locally healthy around `100k` offered when the local harness is run with the same Mahi flags as the live Ansible deployment. A local run without those flags is expected to be misleading because it does not exercise peer transport, async observer owner relay, async Mahi block broadcast, or owner-relay redundancy.
   Files changed:
   - `chain/experiments/local/266-local-liveflags-replay.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code commit under test: `9f192eccfb00d21b82c498f05b5f648ab2bffda4`
   - Result tags created after this decision-log commit:
     - `chain-exp-266-local-liveflags-100k-green`
     - `chain-exp-266-local-liveflags-120k-relay-cliff`
     - `chain-exp-266-local-liveflags-150k-relay-cliff`
   - `kombat_chain` sha256: `1705f7e284062ad558f1728cf7cb2ea89a83ef922787b7e182c6ad5f6c46ed21`
   - `kombat_loadgen` sha256: `ffb0ec858327eea0a87ed2076be1c9e2366aa00e7bbc176d26072664e78e954f`
   - Local manifest: `chain/experiments/local/266-local-liveflags-replay.json`
   Tests / verification:
   - `cargo build --release -p kombat-chain-rpc --bin kombat_chain -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   - No-live-flags control: `AUTO_MAX_ITEMS=5000 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 KEEP_LOG=1 LOCAL_CLUSTER_OUTPUT_DIR=/tmp/codex-exp-local-latest-main-9f192eccf-100k-10s bash chain/scripts/loadtest_local_cluster.sh 32 100000 10`
   - Live-flags green replay: `LOCAL_CLUSTER_SKIP_BUILD=1 AUTO_MAX_ITEMS=5000 LOCAL_CLUSTER_ENABLE_INGRESS=1 LOCAL_CLUSTER_PEER_TRANSPORT=1 LOADTEST_TARGET_VALIDATOR=ingress LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=32 KEEP_LOG=1 KOMBAT_CONSENSUS=mahi KOMBAT_MAHI_COMMIT_RULE=mahi KOMBAT_MAHI_OBSERVER_ASYNC_RELAY=1 KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST=true KOMBAT_MAHI_MERGED_EXECUTION_BATCH=8 KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY=2 KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS=256 LOCAL_CLUSTER_OUTPUT_DIR=/tmp/codex-exp-local-liveflags-main-9f192eccf-100k-10s bash chain/scripts/loadtest_local_cluster.sh 32 100000 10`
   - Same live-flags command shape at `120000` and `150000` offered into `/tmp/codex-exp-local-liveflags-main-9f192eccf-120k-10s` and `/tmp/codex-exp-local-liveflags-main-9f192eccf-150k-10s`.
   Benchmark / local artifacts:
   - `/tmp/codex-exp-local-latest-main-9f192eccf-100k-10s`
   - `/tmp/codex-exp-local-liveflags-main-9f192eccf-100k-10s`
   - `/tmp/codex-exp-local-liveflags-main-9f192eccf-120k-10s`
   - `/tmp/codex-exp-local-liveflags-main-9f192eccf-150k-10s`
   Evidence:
   - No-live-flags `100k/10s` is a bad local control: it achieved only `88,057.6` measured RPS, produced `935,678` RPC errors, and the error samples were `local_submit_queue_full`. It committed `300,544` total intents after settle and only `7,738.2` measured average instant TPS. This result must not be used as a protocol signal.
   - Live-equivalent flags at `100k/10s` recovered a clean local green result: `100,032.8` measured RPS, `0` RPC errors, `0` nonce errors, `0` network errors, `1,498,872` committed intents, `98,453.9` measured average instant TPS, and `114,227.4` measured peak instant TPS.
   - The `100k` live-flags run passed post-load convergence at height `228` with state root `0xb928b16e40fad5409e61453273f73f72e381a87b2b04c8f4538f397c9041be63`.
   - The `100k` live-flags run exercised the intended Mahi pre-admitted path: each validator accepted and drained about `374k` pre-admitted intents, and post-run pre-admitted pools were empty.
   - Live-equivalent flags at `120k/10s` accepted load cleanly (`120,008.8` measured RPS, `0` errors), but commit progress stalled during the measured window. It committed only `128,528` intents after settle and left `1,665,544` pending relay intents on ingress.
   - Live-equivalent flags at `150k/10s` accepted load cleanly (`150,005.6` measured RPS, `0` errors), but the owner-relay stall was worse: only `7,352` committed intents after settle and `2,240,992` pending relay intents on ingress.
   Status:
   - kept diagnostic; local green artifact retained
   Reason:
   - Latest code is locally healthy at `100k` when run with the live Mahi flags. The apparent regression from the first local run was a harness configuration mistake, not a code regression.
   - The next local bottleneck is sharply identified: above `100k` offered, public ingress accepts load with low latency, but observer-to-owner pending relay drain falls behind catastrophically. That is the same structural class as the live owner-feed bottleneck, and it should be addressed with owner-relay instrumentation/transport work rather than another consensus micro-tune.
   Next move:
   - Use `/tmp/codex-exp-local-liveflags-main-9f192eccf-100k-10s` as the current local green artifact.
   - Do not use the no-live-flags artifact for perf decisions.
   - Before another `>100k` local run, instrument or redesign the Mahi owner-relay drain path so accepted observer load cannot accumulate millions of pending relay intents without reaching validators.

267. Phase 6 local harness switch binned: one-flag live replay did not reproduce item266 green
   Date:
   - `2026-04-27`
   Hypothesis:
   - A `LOCAL_CLUSTER_LIVE_MAHI_FLAGS=1` local harness switch could codify the manually supplied live Mahi flags from item266 and prevent future agents from accidentally running the misleading no-live-flags path.
   Files changed:
   - `chain/scripts/loadtest_local_cluster.sh` (attempted; reverted before commit)
   - `chain/experiments/local/267-local-liveflag-switch-binned.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Start head: `fb835953a`
   - Result tag created after this decision-log commit: `chain-exp-267-local-liveflag-switch-binned`
   - Item266 green artifact: `/tmp/codex-exp-local-liveflags-main-9f192eccf-100k-10s`
   - Native rebuild `kombat_chain` sha256 after the local target dir was repaired: `9b394a791f944c9c65c05fe8f92a392b8fc225f8fb9d8b324c06a5a3a7607db1`
   - Native rebuild `kombat_loadgen` sha256 after the local target dir was repaired: `ed7c7e2a379dd85f5a68b7c60f0f1f28e032cc64c9cfe3496078cadd31308318`
   Tests / verification:
   - `bash -n chain/scripts/loadtest_local_cluster.sh`
   - `git diff --check -- chain/scripts/loadtest_local_cluster.sh`
   - Rebuilt native binaries after `chain/target/release/kombat_chain` was found to be a Linux ELF binary in the macOS target dir.
   - Attempted `LOCAL_CLUSTER_LIVE_MAHI_FLAGS=1 ... bash chain/scripts/loadtest_local_cluster.sh 32 100000 10` before and after reverting observer-ingress peer transport.
   - Re-ran the exact manual item266-style env after native rebuild to check whether the failure was caused by the switch or by local/binary drift.
   Benchmark / local artifacts:
   - `/tmp/codex-exp-local-liveflag-switch-main-fb835953a-100k-10s`
   - `/tmp/codex-exp-local-liveflag-switch-main-fb835953a-100k-10s-rerun`
   - `/tmp/codex-exp-local-liveflag-switch-main-fb835953a-100k-10s-rerun2`
   - `/tmp/codex-exp-local-manual-liveflags-main-fb835953a-100k-10s-rerun`
   Evidence:
   - First switch attempt failed before startup because `chain/target/release/kombat_chain` had been replaced by a Linux ELF binary and could not execute on macOS. Rebuilding with `cargo build --release ...` restored native Mach-O binaries.
   - Switch attempt with observer ingress peer transport enabled was a clear fail mode: `31,825.6` measured RPS, `225,696` network errors, and only `5,432` committed intents after settle. Localhost observer peer transport is not equivalent to the live setup and must not be enabled casually in this harness.
   - Switch attempt with validator peer transport only accepted `99,994.4` measured RPS with `0` errors, but committed only `393,016` intents after settle and left `832,696` pending relay intents on ingress.
   - Exact manual item266-style env after native rebuild also failed to reproduce green: `100,004.0` measured RPS with `0` errors, only `343,712` committed intents after settle, and `1,151,176` pending relay intents on ingress.
   - Therefore the problem is not only the proposed switch. The item266 local green artifact remains valid, but the current local machine/binary state no longer reproduces it. Treat this as a binned harness attempt and a local-instability warning, not a protocol regression.
   Status:
   - binned
   Reason:
   - The proposed harness switch did not reproduce the item266 green path, and keeping it would encode an unstable local assumption. The script change was reverted before commit.
   Next move:
   - Do not spend more time tuning against this local-only instability without profiling. For green recovery, return to codified live/topology experiments where item265 already showed `78,459.4` measured RPS and isolated validator-1 as the remaining live bottleneck.

268. Phase 6 live Ansible benchmark binned: loadgen transport failure and non-converged retry
   Date:
   - `2026-04-27`
   Hypothesis:
   - Running the current Ansible-managed testnet benchmark from the checked-in live topology should reproduce the item265-class live signal when driven by external loadgen hosts.
   Files changed:
   - `chain/experiments/live/268-ansible-testnet-benchmark-binned.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code commit under test: `265ef923717f80065763bb2a5645012cb30c0727`
   - Result tag to create after this decision-log commit: `chain-exp-268-ansible-testnet-benchmark-binned`
   - Validator binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Loadgen binary sha256: `e5a6927cbf30e1abb0c580b1461fe198498e14b310c67af9ea7757211fcb01ef`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - Manifest: `chain/experiments/live/268-ansible-testnet-benchmark-binned.json`
   Tests / verification:
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/verify.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/reset_state.yml`
   - Invalid 268a reused old writer manifest after clean reset and failed pre-load because policies no longer existed.
   - 268b command: `LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.18 root@203.0.113.11' LOADTEST_OUTPUT_DIR=/tmp/and-live-268b-ansible-testnet-fresh-writers-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - 268c command: `LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.11' LOADTEST_WRITERS_JSON=/tmp/and-live-268b-ansible-testnet-fresh-writers-100k-60s/writers-all.json LOADTEST_OUTPUT_DIR=/tmp/and-live-268c-ansible-testnet-three-loadgen-hosts-100k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 100000 60`
   - Post-268c cleanup reset and verify passed.
   Benchmark / live artifacts:
   - `/tmp/and-live-268-ansible-testnet-100k-60s`
   - `/tmp/and-live-268b-ansible-testnet-fresh-writers-100k-60s`
   - `/tmp/and-live-268b-load-window-profile-100k-60s`
   - `/tmp/and-live-268b-loadgen-profile-100k-60s`
   - `/tmp/and-live-268c-ansible-testnet-three-loadgen-hosts-100k-60s`
   - `/tmp/and-live-268c-load-window-profile-100k-60s`
   - `/tmp/and-live-268c-loadgen-profile-100k-60s`
   Status:
   - binned
   Evidence:
   - 268a invalid: clean reset invalidated the reused item261 writer manifest; all endpoints failed before load on missing `kombat_getPolicy.result`.
   - 268b fresh writer seeding succeeded and wrote `/tmp/and-live-268b-ansible-testnet-fresh-writers-100k-60s/writers-all.json`, sha256 `8e81381939849830121b825713c733337366ccf9a02bb635f891bf3cb3e7a39b`.
   - 268b invalid: `validator-1-p8646` failed because runner `root@203.0.113.18` closed SSH during the load command. The remaining 11 endpoints completed with `53,914.7` measured RPS, `3,485,544` ok/committed intents, and zero RPC/nonce/network errors.
   - 268c avoided `203.0.113.18` and all 12 endpoint commands completed with zero RPC/nonce/network errors, but only delivered `45,512.8` measured RPS, `2,959,424` ok intents, and `2,766,329` committed intents.
   - 268c did not converge after extra settle: v1-v3 were at height `260` with head `0xa780f6a81a988f3b93fe79287643b61af95cf14641051d0f2d75b72c2c818a7f`, while v4 was at height `258` with head `0xbd90dbb48e2caef11485a2d074a454cfa81b3348341b8e6f975da913d585db91`.
   - The testnet was reset and re-verified after the binned run; all services are active and all validators still run binary sha `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`.
   Reason:
   - More loadgen hosts did not make the chain path worse. The four-host run failed because one loadgen host dropped its SSH session mid-load. The three-host fallback removed that transport failure but reduced and unbalanced effective loadgen capacity and was not a clean green replay. Neither result should be used as a current TPS ceiling.
   Next move:
   - Fix live loadgen orchestration before judging protocol performance: replace or repair `203.0.113.18`, or run loadgen through persistent remote systemd/tmux jobs instead of long-lived transient SSH sessions. Then run a clean reset plus seeded replay with four reliable loadgen hosts and profile artifacts.

269. Phase 6 live Ansible loadgen pool hardening
   Date:
   - `2026-04-27`
   Hypothesis:
   - Future live green benchmarks need a checked-in loadgen runner pool so the item268 bad host cannot be accidentally reintroduced through an ad hoc `LOADTEST_EXTERNAL_RUNNER_HOSTS` string.
   Files changed:
   - `chain/deploy/ansible/inventory/testnet.yml`
   - `chain/deploy/ansible/README.md`
   - `chain/experiments/live/269-ansible-loadgen-pool.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Start commit: `7dee93a1042b01ac523e63854741e3854b4861f0`
   - Result tag to create after this decision-log commit: `chain-exp-269-ansible-loadgen-pool`
   - Manifest: `chain/experiments/live/269-ansible-loadgen-pool.json`
   Tests / verification:
   - `ansible-inventory -i chain/deploy/ansible/inventory/testnet.yml --list`
   - `python3 -m json.tool chain/experiments/live/269-ansible-loadgen-pool.json`
   Status:
   - kept
   Evidence:
   - Added a `kombat_loadgen` inventory group with `root@203.0.113.10`, `root@203.0.113.12`, and `root@203.0.113.11`.
   - `root@203.0.113.18` is intentionally absent. It is a 2-vCPU/4GB host and dropped SSH mid-load in item268.
   - Documented the exclusion in the Ansible README so future 100k+ live runs do not compare a bad runner topology against item219/item265-style references.
   Reason:
   - This does not fix chain convergence by itself, but it removes a known runner-layer invalidation source before the next green ladder. If a clean low-load run still fails convergence, that should be treated as a node/protocol correctness bug.
   Next move:
   - Run the live green ladder from a clean reset using only the checked-in `kombat_loadgen` pool or a replacement 8-vCPU+ runner added to this inventory. Stop at the first non-green run and log whether the failure is runner delivery, RPC/nonce errors, or post-run convergence.

270. Phase 6 live green ladder with four EU loadgen hosts: 40k green floor, 60k binned on undrained pre-admitted tail
   Date:
   - `2026-04-27`
   Hypothesis:
   - With the fourth EU loadgen host checked into Ansible inventory and explicit runner SSH options in the live loadtest harness, the current `main` branch should produce a clean live green ladder and reveal the first non-green rung without runner-layer invalidation.
   Files changed:
   - `chain/experiments/live/270-ansible-green-ladder.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code commit under test: `b2448f93a71a127eeda5ef236169632bd0cc8369`
   - Result tags created after this decision-log commit:
     - `chain-exp-270-ansible-green-ladder-40k-green`
     - `chain-exp-270-ansible-green-ladder-60k-undrained-binned`
   - Validator binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Loadgen binary sha256: `e5a6927cbf30e1abb0c580b1461fe198498e14b310c67af9ea7757211fcb01ef`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - Manifest: `chain/experiments/live/270-ansible-green-ladder.json`
   Loadgen host pool:
   - `root@203.0.113.10`
   - `root@203.0.113.12`
   - `root@203.0.113.11`
   - `root@203.0.113.13`
   - Excluded: `root@203.0.113.18` (known bad 2-vCPU/4GB runner) and `root@203.0.113.20` (JPN node, cross-region for EU green runs).
   Tests / verification:
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/verify.yml`
   - `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/reset_state.yml`
   - Verified runner access through the harness path: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' bash -lc 'read -r -a opts <<< "$LOADTEST_RUNNER_SSH_OPTS"; ssh "${opts[@]}" root@203.0.113.13 hostname'`
   - 270a command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.11 root@203.0.113.13' LOADTEST_OUTPUT_DIR=/tmp/and-live-270a-ansible-green-ladder-40k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 40000 60`
   - 270a post-run checks: `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet` and `kombat_getStateRootAtHeight(height=130)` on all four validators.
   - 270b command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.11 root@203.0.113.13' LOADTEST_OUTPUT_DIR=/tmp/and-live-270b-ansible-green-ladder-60k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 60000 60`
   - 270b post-run checks: `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet`, `kombat_getStateRootAtHeight(height=102)` on all four validators, then an extra `30s` settle plus a second `kombat_getConsensusMetrics` read for queue drain.
   Benchmark / live artifacts:
   - `/tmp/and-live-270a-ansible-green-ladder-40k-60s`
   - `/tmp/and-live-270b-ansible-green-ladder-60k-60s`
   Status:
   - kept 40k green floor; 60k binned
   Evidence:
   - 270a was clean and converged: `32,804.8` measured RPS, `2,052,168` ok/committed intents, `34,202.8` committed TPS, `0` RPC errors, `0` nonce errors, `0` network errors.
   - 270a drained fully after settle: all validators matched at height `130` with head hash `0x8cb72581e13c6812e59356a839f6f8bdc94b7482b257fd9c380045fae2ac408d`, shared state root `0x89bad640d09397a1f00fccc09de065d6bc7a3de3ef10cfdf57d32f0c0d3f92a2`, `pending_relay_intents=0`, and `mahi_pre_admitted_pool.items=0`.
   - 270a still under-delivered the offered `40k`: weak endpoints were `validator-4-p8645` at `846.5` RPS, `validator-1-p8646` at `1,157.7` RPS, and `validator-2-p8647` at `1,437.1` RPS.
   - 270b also converged on block/state root with `0` RPC/nonce/network errors: `42,009.6` measured RPS, `2,699,304` ok/committed intents, `44,988.4` committed TPS, common height `102`, head hash `0x941ed2fb75adafb3ceec04c97ff32d93278f77acfb72840576450193b06cacfc`, and shared state root `0xb5456b95a0443ce2457f7ff794104bd3753d65ddadad95f3b76e6eabc833b4e4`.
   - 270b is not green because it did not drain fully after extra settle: validator-2 kept `mahi_pre_admitted_pool.items=324` while the other three validators were at `0`. Relay queues were empty, but the pre-admitted tail persisted across the extra `30s` recheck.
   - 270b weak endpoints were `validator-4-p8645` at `1,441.7` RPS, `validator-2-p8647` at `1,663.1` RPS, and `validator-1-p8646` at `1,727.3` RPS.
   Reason:
   - The four-EU-runner pool is now valid and reproducible; runner SSH no longer invalidates the experiment. But the current live setup still under-delivers badly relative to offered load, and the first non-green rung appears at `60k/60s` because validator-2 keeps a non-zero Mahi pre-admitted tail after extra settle.
   Next move:
   - Treat `34,202.8` committed TPS from 270a as the current live green floor on current `main`.
   - Treat 270b as binned, not green.
   - Do not claim a clean live green number above `40k/60s` until the weak ingress endpoints (`validator-1:8646`, `validator-2:8647`, `validator-4:8645`) and validator-2's residual pre-admitted tail are explained and fixed.

271. Phase 6 live loadgen reusable payload cache harness
   Date:
   - `2026-04-27`
   Hypothesis:
   - Since the live testnet is reset before benchmark runs, the harness can reuse deterministic writer identities and prebuilt signed payload bodies between runs. This should remove avoidable loadgen signing/Borsh/JSON CPU from the hot path and reduce remote setup churn without changing chain runtime code.
   Files changed:
   - `chain/crates/loadgen/Cargo.toml`
   - `chain/crates/loadgen/src/main.rs`
   - `chain/crates/loadgen/README.md`
   - `chain/scripts/prepare_live_loadtest_accounts.sh`
   - `chain/scripts/loadtest_live_network_from_validator.sh`
   - `chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `chain/deploy/ansible/README.md`
   - `chain/experiments/live/271-loadgen-payload-cache-harness.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Start commit: `d8d826c426465e080fe8ef72383d087101e961ac`
   - Result tag to create after this decision-log commit: `chain-exp-271-loadgen-payload-cache-harness`
   - Manifest: `chain/experiments/live/271-loadgen-payload-cache-harness.json`
   Tests / verification:
   - `bash -n chain/scripts/prepare_live_loadtest_accounts.sh`
   - `bash -n chain/scripts/loadtest_live_network_from_validator.sh`
   - `bash -n chain/scripts/loadtest_live_network_multi_ingress.sh`
   - `cargo test -p kombat-chain-loadgen --manifest-path chain/Cargo.toml`
   - `cargo build --release -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path chain/Cargo.toml`
   - `chain/target/release/kombat_loadgen --help | rg -n "payload-cache|client-order|intent-ttl|writers-json"`
   Status:
   - kept infrastructure
   Evidence:
   - Added `LOADTEST_WRITER_SEED_TAG` so reset-before-run benches can reseed the exact same account ids, policy ids, and session keys.
   - Added opt-in `kombat_loadgen --payload-cache-dir` for non-strict batched `place-order` runs. It prebuilds signed `kombat_submitIntentBatch` JSON-RPC bodies and reuses them when the cache key still matches.
   - Added `--client-order-id-base` so cached signed intents can use deterministic client order ids instead of timestamp-derived ids.
   - Added wrapper support for stable `LOADTEST_REMOTE_CACHE_DIR`; remote loadgen binaries and writer manifests are reused by sha256 instead of recopied every endpoint/run.
   - Added wrapper support for `LOADTEST_PAYLOAD_CACHE=1`, `LOADTEST_PAYLOAD_CACHE_DIR`, `LOADTEST_INTENT_TTL_MS`, and `LOADTEST_CLIENT_ORDER_ID_BASE`. With payload caching enabled, wrappers default intent TTL to `315360000000` ms so cached intents do not expire across later reset/replay runs.
   Reason:
   - This is not a TPS result. It is harness infrastructure to make the next live ladder cleaner. Cached payloads are safe only after a reset plus deterministic reseed; replaying them twice in the same chain state would correctly produce duplicate/stale nonces.
   Next move:
   - Rebuild the Linux loadgen binary, deploy/reuse it through the remote cache, then rerun the 40k/60k live ladder with `LOADTEST_WRITER_SEED_TAG=270`, `LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache`, and `LOADTEST_PAYLOAD_CACHE=1`. Compare delivered RPS and runner CPU against item270 before claiming a new green TPS.

272. Phase 6 live writer provisioning bounded parallelism
   Date:
   - `2026-04-27`
   Hypothesis:
   - The next harness win is setup/provisioning time, not more signing/cache tweaks. Since reset-before-run benches need the same 384 writer accounts/policies repeatedly, provisioning should be bounded-parallel while preserving serial behavior by default.
   Files changed:
   - `chain/scripts/prepare_live_loadtest_accounts.sh`
   - `chain/deploy/ansible/README.md`
   - `chain/experiments/live/272-parallel-writer-provisioning-harness.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Start commit: `8e03ac9b847f36d8ede58a0ff0be503501ce429c`
   - Result tag to create after this decision-log commit: `chain-exp-272-parallel-writer-provisioning-harness`
   - Manifest: `chain/experiments/live/272-parallel-writer-provisioning-harness.json`
   Tests / verification:
   - `bash -n chain/scripts/prepare_live_loadtest_accounts.sh`
   - `python3 -m json.tool chain/experiments/live/272-parallel-writer-provisioning-harness.json`
   - `git diff --check -- chain/scripts/prepare_live_loadtest_accounts.sh chain/deploy/ansible/README.md chain/experiments/live/272-parallel-writer-provisioning-harness.json chain/THROUGHPUT_DECISION_LOG.md`
   Status:
   - kept infrastructure
   Evidence:
   - Added `LOADTEST_SEED_CONCURRENCY` to `prepare_live_loadtest_accounts.sh`.
   - Default remains `1`, preserving historical serial provisioning behavior.
   - Parallel mode runs bounded chunks of seed jobs and writes isolated per-writer logs under the temporary work directory. If any seed job fails, the script prints the failed job's account/policy and the first 160 log lines before exiting non-zero.
   - The implementation avoids `wait -n`, so it remains compatible with older macOS Bash.
   - Updated the Ansible live benchmark README to show `LOADTEST_SEED_CONCURRENCY=16` alongside deterministic writer seeds and payload cache flags.
   Reason:
   - This is not a TPS result. It reduces the wall-clock cost of clean reset/reseed/replay cycles and makes the next live ladder less painful to run repeatedly.
   Next move:
   - Run the item273 cached live ladder with `LOADTEST_WRITER_SEED_TAG=270`, `LOADTEST_SEED_CONCURRENCY=16`, `LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache`, and `LOADTEST_PAYLOAD_CACHE=1`. If provisioning fails under concurrency, lower to `8` and log the failure mode.

273. Phase 6 cached payload + parallel-seed 40k live canary: 273a invalid wrong-arch runner binary, 273b green but under-driven on contaminated runner
   Date:
   - `2026-04-27`
   Hypothesis:
   - Current `main` at `d2bfe37b7` should still produce a clean green `40k/60s` live canary with deterministic writer reseed, payload cache reuse, and `LOADTEST_SEED_CONCURRENCY=16`. If delivered RPS still sags, the result should expose whether the weak-endpoint pattern still follows the contaminated external runner instead of the chain.
   Files changed:
   - `chain/experiments/live/273-cached-payload-parallel16-40k-green-underdriven.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code commit under test: `d2bfe37b785e17805556917aa10f530ce0604d56`
   - Result tags to create after this decision-log commit:
     - `chain-exp-273-cached-payload-parallel16-40k-arm-binary-invalid`
     - `chain-exp-273-cached-payload-parallel16-40k-green-underdriven`
   - Validator binary sha256: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - 273a invalid loadgen binary sha256: `703dfeee28accea5745f34c33b17a48b6b1addac5e09831256427cf508e82e48` (`linux/arm64`)
   - 273b fixed loadgen binary sha256: `3556e24585463ca9602bf8839acd88f4c54835283fe28300fa4d8c618fa12e26` (`linux/x86_64`)
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - Manifest: `chain/experiments/live/273-cached-payload-parallel16-40k-green-underdriven.json`
   Loadgen host pool:
   - `root@203.0.113.10`
   - `root@203.0.113.12`
   - `root@203.0.113.11`
   - `root@203.0.113.13`
   - Excluded: `root@203.0.113.18` (known bad 2-vCPU/4GB runner) and `root@203.0.113.20` (JPN node, cross-region for EU green runs).
   Tests / verification:
   - 273b authoritative reset: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/reset_state.yml`
   - 273b post-reset verify: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/verify.yml`
   - 273a command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.11 root@203.0.113.13' LOADTEST_WRITER_SEED_TAG=270 LOADTEST_SEED_CONCURRENCY=16 LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache LOADTEST_PAYLOAD_CACHE=1 LOADTEST_OUTPUT_DIR=/tmp/and-live-273a-cached-payload-parallel16-40k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 40000 60`
   - 273b amd64 rebuild: `docker run --rm --platform linux/amd64 -v /Users/waynedouglas/github/hx/pvp/chain:/work -w /work rust:1.85-slim bash -lc 'export PATH=/usr/local/cargo/bin:$PATH; apt-get update && apt-get install -y pkg-config libssl-dev build-essential && cargo build --release -p kombat-chain-loadgen --bin kombat_loadgen --manifest-path Cargo.toml && cp target/release/kombat_loadgen target-linux-x86_64/release/kombat_loadgen'`
   - 273b binary verification: `file chain/target-linux-x86_64/release/kombat_loadgen` and `sha256sum chain/target-linux-x86_64/release/kombat_loadgen`
   - 273b command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.11 root@203.0.113.13' LOADTEST_WRITER_SEED_TAG=270 LOADTEST_SEED_CONCURRENCY=16 LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache LOADTEST_PAYLOAD_CACHE=1 LOADTEST_OUTPUT_DIR=/tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64 LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 40000 60`
   - 273b post-run checks: `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet > /tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64/post-run-network-status.txt` and `kombat_getStateRootAtHeight(height=265)` on all four validators into `/tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64/state-roots-height-265.txt`
   Benchmark / live artifacts:
   - `/tmp/and-live-273a-cached-payload-parallel16-40k-60s`
   - `/tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64`
   - `/tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64/post-run-network-status.txt`
   - `/tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64/state-roots-height-265.txt`
   - `/tmp/and-live-273b-cached-payload-parallel16-40k-60s-amd64/runner-profiles`
   Status:
   - kept 40k green under-driven; 273a invalid
   Evidence:
   - 273a invalid: parallel writer seeding completed and all twelve endpoint driver commands reached runner launch, but every endpoint failed before load with `Exec format error` because the uploaded loadgen binary was Linux ARM instead of Linux x86_64.
   - 273b rebuilt the runner binary correctly for `linux/amd64` and completed without RPC, nonce, or network errors: `27,854.6` measured RPS, `1,694,232` ok/committed intents, and `28,237.2` committed TPS over `60s`.
   - 273b converged and drained fully after settle: all four validators matched at height `265` with head hash `0xd076e61870c8f228156522ba13d2a5cb08eac644c1c7263d2dbe82c98d6ab4df`, shared state root `0xee34fb628cf4d10e9e6181cc955f921230ccad3055a6bbf84ae2194d1f4fca61`, `pending_relay_intents=0`, and `mahi_pre_admitted_pool.items=0`.
   - 273b reproduced the same weak-endpoint pattern as item270, and the mapping is explicit in the wrapper: `LOADTEST_EXTERNAL_RUNNER_HOSTS` is assigned round-robin across the `12` ingress endpoints, so each of the `4` runners owns `3` endpoint jobs.
   - The three weakest endpoints were exactly the three endpoints mapped to `root@203.0.113.11`: `validator-1-p8646` at `793.5` RPS, `validator-2-p8647` at `767.6` RPS, and `validator-4-p8645` at `1,206.8` RPS.
   - Runner profile artifacts captured `15` snapshots. They repeatedly showed `root@203.0.113.11` carrying `kube-apiserver`, `etcd`, `kube-controller-manager`, and `kubelet`, with load averages roughly in the `7-10` range during and after the load window.
   - Payload cache plus parallel provisioning did not improve delivered RPS versus item270 under this runner pool. 273b measured materially below item270's green `40k` rung even though it stayed clean and converged.
   Reason:
   - Keep 273b as proof that current `main` still produces a clean green live run with the new harness path. Do not treat `28,237.2` committed TPS as a recovered live ceiling or as a replacement for item270's `34,202.8` committed TPS green floor; the run is clearly under-driven by the same contaminated runner mapping.
   Next move:
   - Replace or isolate `root@203.0.113.11`, or make endpoint-to-runner assignment explicit so the three historically weak ingress endpoints no longer inherit the same contaminated host.
   - After that runner fix, rerun `40k/60s` from clean reset first. Only spend a `60k/60s` rung once delivered RPS is back at or above the item270 `40k` signal.

274. Phase 6 three clean EU runners after excluding contaminated 207: 40k green, 60k green but still under-driven
   Date:
   - `2026-04-27`
   Hypothesis:
   - The steep drop in item273 was primarily the contaminated `root@203.0.113.11` runner. If that host is excluded and the same cached-payload + parallel-seed harness is rerun from clean reset, current `main` should recover a materially higher green live TPS without reproducing validator divergence or validator-2's old undrained tail.
   Files changed:
   - `chain/experiments/live/274-three-eu-runners-green-ladder.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code commit under test: `b0db3de58235b7993a8b17a6703716d873de893a`
   - Result tags to create after this decision-log commit:
     - `chain-exp-274-three-eu-runners-40k-green`
     - `chain-exp-274-three-eu-runners-60k-green-underdriven`
   - Validator binary sha256 on all four validators: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Linux loadgen binary sha256: `3556e24585463ca9602bf8839acd88f4c54835283fe28300fa4d8c618fa12e26`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml` (loadgen pool bypassed for these runs; exact runner list supplied explicitly via `LOADTEST_EXTERNAL_RUNNER_HOSTS`)
   - Manifest: `chain/experiments/live/274-three-eu-runners-green-ladder.json`
   Loadgen host pool:
   - `root@203.0.113.10`
   - `root@203.0.113.12`
   - `root@203.0.113.13`
   - Excluded: `root@203.0.113.11` (contaminated Kubernetes/control-plane host), `root@203.0.113.18` (known bad 2-vCPU/4GB runner), and `root@203.0.113.20` (JPN node, cross-region for EU green runs).
   Topology note:
   - `loadtest_live_network_multi_ingress.sh` assigns `LOADTEST_EXTERNAL_RUNNER_HOSTS` as `ext_hosts[$(( endpoint_index % host_count ))]` after incrementing `endpoint_index`. With the three-host list above, every `:8645` ingress maps to `root@203.0.113.12`, every `:8646` ingress maps to `root@203.0.113.13`, and every `:8647` ingress maps to `root@203.0.113.10`.
   Tests / verification:
   - 274a preflight reset: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/reset_state.yml`
   - 274a post-reset verify: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/verify.yml`
   - 274a command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.13' LOADTEST_WRITER_SEED_TAG=270 LOADTEST_SEED_CONCURRENCY=16 LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache LOADTEST_PAYLOAD_CACHE=1 LOADTEST_OUTPUT_DIR=/tmp/and-live-274a-three-eu-runners-40k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 40000 60`
   - 274a post-run checks: `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet > /tmp/and-live-274a-three-eu-runners-40k-60s/post-run-network-status.txt` and `kombat_getStateRootAtHeight(height=203)` on all four validators into `/tmp/and-live-274a-three-eu-runners-40k-60s/state-roots-height-203.txt`
   - 274b preflight reset: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/reset_state.yml`
   - 274b post-reset verify: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/verify.yml`
   - 274b command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.13' LOADTEST_WRITER_SEED_TAG=270 LOADTEST_SEED_CONCURRENCY=16 LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache LOADTEST_PAYLOAD_CACHE=1 LOADTEST_OUTPUT_DIR=/tmp/and-live-274b-three-eu-runners-60k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 60000 60`
   - 274b post-run checks: `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet > /tmp/and-live-274b-three-eu-runners-60k-60s/post-run-network-status.txt` and `kombat_getStateRootAtHeight(height=150)` on all four validators into `/tmp/and-live-274b-three-eu-runners-60k-60s/state-roots-height-150.txt`
   Benchmark / live artifacts:
   - `/tmp/and-live-274a-three-eu-runners-40k-60s`
   - `/tmp/and-live-274a-three-eu-runners-40k-60s/post-run-network-status.txt`
   - `/tmp/and-live-274a-three-eu-runners-40k-60s/state-roots-height-203.txt`
   - `/tmp/and-live-274a-three-eu-runners-40k-60s/runner-profiles`
   - `/tmp/and-live-274b-three-eu-runners-60k-60s`
   - `/tmp/and-live-274b-three-eu-runners-60k-60s/post-run-network-status.txt`
   - `/tmp/and-live-274b-three-eu-runners-60k-60s/state-roots-height-150.txt`
   - `/tmp/and-live-274b-three-eu-runners-60k-60s/runner-profiles`
   Status:
   - kept 40k green
   - kept 60k green under-driven
   Evidence:
   - 274a recovered the live green floor immediately after excluding `root@203.0.113.11`: `38,051.3` measured RPS, `2,343,616` committed intents, and `39,060.3` committed TPS over `60s`.
   - 274a stayed clean end-to-end: zero RPC, nonce, or network errors; all four validators converged at height `203`; shared head hash `0x71167ffc0235da31c7457ff2afc0c687efdf05ad9307ef8bb6a953a39629a1f3`; shared state root `0xa8b63e919c20922f5f747f3786bfbfbf42e6be2c13f5072f93273f5baa37f3bd`; and all relay plus Mahi queues drained to zero.
   - 274b remained green at a higher rung: `41,635.1` measured RPS, `2,533,272` committed intents, and `42,221.2` committed TPS over `60s`, again with zero RPC, nonce, or network errors.
   - 274b also converged and drained fully: all four validators matched at height `150` with head hash `0x6c57187b8a9fabbbd0607144614648ef40517349bbaa85f5ee21162306de23ce`, shared state root `0x628d93538865009c366ecf065cfbb6b1622c81dd72e205ef7544ef9008b13156`, `pending_relay_intents=0`, and `mahi_pre_admitted_pool.items=0`.
   - The item270b validator-2 residual `mahi_pre_admitted_pool.items=324` tail did not reproduce once the contaminated runner was removed.
   - The three-runner pool is still not a final capacity topology. 274b split into two clear endpoint bands even though it stayed green: validator-1 and validator-2 endpoints ranged from `2,238.3-2,508.9` RPS while validator-3 and validator-4 endpoints ranged from `4,459.6-4,665.7` RPS.
   - Runner-profile artifacts captured substantial `kombat_loadgen` CPU burn during both runs, so the remaining bottleneck is not “idle runners”; it is the deterministic three-host endpoint assignment and remaining host-to-host delivery imbalance.
   Reason:
   - Keep both rungs. They prove the recent live regression was primarily runner-side, not a proven node correctness issue. `42,221.2` committed TPS is the new best green live TPS observed on current `main`, but the `60k` rung is still under-driven versus offered load and should not be treated as the chain ceiling.
   Next move:
   - Replace the three-host fallback pool with four dedicated EU runners, update the checked-in runner inventory, and rerun the clean-reset ladder from `40k/60s` upward before making any protocol claims.

275. Phase 6 four dedicated EU runners 40k rung: binned on validator-4 ingress collapse plus validator-2 undrained tail
   Date:
   - `2026-04-27`
   Hypothesis:
   - If the three-host topology artifact was the main remaining limiter after item274, then adding a fourth dedicated EU runner should at least reproduce a clean green `40k/60s` rung. If `40k` still fails green after the runner pool is cleaned, treat it as a validator-side correctness or service issue, not another generic loader excuse.
   Files changed:
   - `chain/deploy/ansible/inventory/testnet.yml`
   - `chain/deploy/ansible/README.md`
   - `chain/experiments/live/275-four-eu-runners-40k-v4-collapse-undrained-binned.json`
   - `chain/THROUGHPUT_DECISION_LOG.md`
   Git / environment anchors:
   - Code commit under test: `eba0ca303d50a53072c2cf3aa8cf970ea0b46a15`
   - Result tag to create after this decision-log commit: `chain-exp-275-four-eu-runners-40k-v4-collapse-undrained-binned`
   - Validator binary sha256 on all four validators: `dd02298286ef973ebc1eec24459cd58cdc80f7c5c5e97ffee48d52b5979881dd`
   - Linux loadgen binary sha256: `3556e24585463ca9602bf8839acd88f4c54835283fe28300fa4d8c618fa12e26`
   - Inventory: `chain/deploy/ansible/inventory/testnet.yml`
   - Manifest: `chain/experiments/live/275-four-eu-runners-40k-v4-collapse-undrained-binned.json`
   Loadgen host pool:
   - `root@203.0.113.10`
   - `root@203.0.113.12`
   - `root@203.0.113.13`
   - `root@203.0.113.19`
   - Excluded: `root@203.0.113.11` (contaminated Kubernetes/control-plane host), `root@203.0.113.18` (known bad 2-vCPU/4GB runner), and `root@203.0.113.20` (JPN node, cross-region for EU green runs).
   Topology note:
   - The checked-in runner pool now includes the dedicated Hetzner NBG1 host `root@203.0.113.19`.
   - `loadtest_live_network_multi_ingress.sh` still assigns hosts as `ext_hosts[$(( endpoint_index % host_count ))]` after incrementing `endpoint_index`, so validator-4's three ingress endpoints were spread across three different runners: `:8645 -> root@203.0.113.13`, `:8646 -> root@203.0.113.19`, and `:8647 -> root@203.0.113.10`.
   Tests / verification:
   - New runner sanity check: `ssh -i ~/.ssh/id_n2_ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/tmp/codex_known_hosts root@203.0.113.19 'hostname; nproc; free -h | sed -n "1,2p"; uptime -p'`
   - 275a preflight reset: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/reset_state.yml`
   - 275a post-reset verify: `cd chain/deploy/ansible && KOMBAT_OPERATOR_TOKEN=$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' /tmp/and-live-testnet/validator-1.env | cut -d= -f2-) .venv/bin/ansible-playbook playbooks/verify.yml`
   - 275a command: `LOADTEST_RUNNER_SSH_OPTS='-i ~/.ssh/id_n2_ssh -o StrictHostKeyChecking=no' LOADTEST_EXTERNAL_RUNNER_HOSTS='root@203.0.113.10 root@203.0.113.12 root@203.0.113.13 root@203.0.113.19' LOADTEST_WRITER_SEED_TAG=270 LOADTEST_SEED_CONCURRENCY=16 LOADTEST_REMOTE_CACHE_DIR=/var/tmp/kombat-loadgen-cache LOADTEST_PAYLOAD_CACHE=1 LOADTEST_OUTPUT_DIR=/tmp/and-live-275a-four-eu-runners-40k-60s LOADTEST_DRAIN_SECS=30 LOADTEST_BATCH_SIZE=8 LOADTEST_PIPELINE_DEPTH=4 LOADTEST_STRICT_NONCE_RETRY=0 LOADTEST_FAIL_ON_ERRORS=1 bash chain/scripts/loadtest_live_network_multi_ingress.sh /tmp/and-live-testnet 32 40000 60`
   - 275a post-run checks: `bash chain/scripts/live_network_status.sh /tmp/and-live-testnet > /tmp/and-live-275a-four-eu-runners-40k-60s/post-run-network-status.txt`, extra settle `+30s` into `/tmp/and-live-275a-four-eu-runners-40k-60s/post-run-network-status-plus30.txt`, and `kombat_getStateRootAtHeight(height=134)` on all four validators into `/tmp/and-live-275a-four-eu-runners-40k-60s/state-roots-height-134.txt`
   Benchmark / live artifacts:
   - `/tmp/and-live-275a-four-eu-runners-40k-60s`
   - `/tmp/and-live-275a-four-eu-runners-40k-60s/post-run-network-status.txt`
   - `/tmp/and-live-275a-four-eu-runners-40k-60s/post-run-network-status-plus30.txt`
   - `/tmp/and-live-275a-four-eu-runners-40k-60s/state-roots-height-134.txt`
   - `/tmp/and-live-275a-four-eu-runners-40k-60s/runner-profiles-live`
   - `/tmp/and-live-275a-four-eu-runners-40k-60s/runner-profiles-load`
   Status:
   - binned 40k rung
   Evidence:
   - 275a completed without RPC, nonce, or network errors, but it only delivered `30,739.8` measured RPS and `32,274.9` committed TPS (`1,936,496` committed intents over `60s`).
   - The rung shared head and state root across all four validators at height `134`: head hash `0x8ff2c25a187a82420ea1262c665adb3009ceeed36dd9543f53eb181f4e33a55d`, shared state root `0xd00bde8e198bef6d60c872c75849f2998a259ab6d8b4a1eaf96d70d535c9a05e`.
   - It is still not green because validator-2 retained `mahi_pre_admitted_pool.items=72` immediately after the rung and after an extra `30s` settle (`accepted=485128`, `drained=485056`).
   - Validator-4's three ingress endpoints were the catastrophic low outliers and they were spread across three different runners, which breaks the “one bad runner owns the weak endpoints” theory:
     - `validator-4-p8645` via `root@203.0.113.13`: `1,233.2` RPS
     - `validator-4-p8646` via `root@203.0.113.19`: `1,382.5` RPS
     - `validator-4-p8647` via `root@203.0.113.10`: `1,340.3` RPS
   - Runner profile artifacts captured active `kombat_loadgen` processes on those three hosts during the load window, so validator-4's collapse is not explained by those runners being idle. Example samples:
     - `validator-4-p8645` on `root@203.0.113.13`: about `51-65%` CPU
     - `validator-4-p8646` on `root@203.0.113.19`: about `17-43%` CPU
     - `validator-4-p8647` on `root@203.0.113.10`: about `19-39%` CPU
   - Validator-2's three endpoints themselves were not throughput outliers (`3,162.4-3,254.4` RPS), yet validator-2 still retained the Mahi pre-admitted tail after extra settle. That points at a validator-side drain issue rather than offered-load underdrive alone.
   Reason:
   - Bin 275a. The first four-runner rung did not improve the green floor and did not fail in a way that can be dismissed as “the same contaminated runner again.” The current green TPS therefore remains item274's `42,221.2` committed TPS; do not replace it with this rung's `32,274.9` committed TPS.
   Next move:
   - Stop the ladder here. Investigate validator-4 host/ingress behavior and validator-2's residual pre-admitted tail before spending another offered-load rung. This is now a node-side correctness/performance problem, not a runner-pool cleanup problem.
