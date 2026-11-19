#!/usr/bin/env bash
set -euo pipefail

# replay_bench_ladder.sh — deterministic replay harness.
#
# Runs the canonical local-cluster bench N times with identical settings,
# then analyses how TPS and committed state root vary across runs. Purpose:
# separate input variance from runtime variance while chasing Phase 4 M9
# stability.
#
# Interpretation:
#
#   state roots identical across all N runs
#     AND TPS variance low
#       -> deterministic path, nothing to fix here.
#     AND TPS variance high
#       -> runtime variance only (scheduling, thermal, allocator, etc).
#          Input stream is reaching the same committed state each time.
#          Variance source is NOT the intents; investigate the runtime.
#
#   state roots differ across runs
#     AND TPS variance high
#       -> input variance dominates. Different runs commit different
#          intent subsets. Investigate loadgen dispatch ordering,
#          worker scheduling, or timing-sensitive admission/relay paths.
#     AND TPS variance low (unlikely)
#       -> somehow the same TPS is produced from different intent sets.
#          Possible but would need specific explanation.
#
#   state roots disagree across validators within a run
#     -> correctness bug. The canonical bench already detects this and
#        fails post-load convergence. Escalate immediately; do not
#        optimise on top of divergent state.
#
# Requires: jq, python3, the loadtest_local_cluster.sh harness in this dir.
#
# Usage:
#   bash chain/scripts/replay_bench_ladder.sh [iterations] [rps] [duration]
#
# Defaults: 5 iterations, 8000 rps, 10 seconds.
#
# Environment variables forwarded to the underlying bench (AUTO_INTERVAL_MS,
# LOADTEST_BATCH_SIZE, LOADTEST_PIPELINE_DEPTH, LOCAL_CLUSTER_ENABLE_INGRESS,
# LOADTEST_TARGET_VALIDATOR, etc) pass through unchanged so the wrapper
# matches whatever shape you're already benching.

if [[ $# -gt 3 ]]; then
  cat <<'USAGE' >&2
Usage: replay_bench_ladder.sh [iterations] [rps] [duration]
  iterations  number of bench runs           (default 5)
  rps         offered requests per second    (default 8000)
  duration    load window in seconds         (default 10)
USAGE
  exit 1
fi

ITERATIONS="${1:-5}"
RPS="${2:-8000}"
DURATION="${3:-10}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BENCH_SCRIPT="${SCRIPT_DIR}/loadtest_local_cluster.sh"

if [[ ! -x "${BENCH_SCRIPT}" && ! -f "${BENCH_SCRIPT}" ]]; then
  echo "replay_bench_ladder.sh: cannot find ${BENCH_SCRIPT}" >&2
  exit 1
fi

for tool in jq python3 bash; do
  command -v "${tool}" >/dev/null 2>&1 || {
    echo "missing required tool: ${tool}" >&2
    exit 2
  }
done

REPLAY_ROOT="$(mktemp -d -t kombat-replay-ladder.XXXXXX)"
trap 'rm -rf "${REPLAY_ROOT}"' EXIT INT TERM

printf -- '----  replay ladder: %d runs @ %s rps / %ss\n' \
  "${ITERATIONS}" "${RPS}" "${DURATION}"
printf -- '----  artifacts: %s\n\n' "${REPLAY_ROOT}"

# Per-run outputs captured here; summary computed in python at the end.
SUMMARY_TSV="${REPLAY_ROOT}/summary.tsv"
: > "${SUMMARY_TSV}"

# Cool-down between runs to let the cluster fully release ports and give
# the OS a chance to settle allocator/thermal state before the next iter.
COOLDOWN_SECS="${REPLAY_COOLDOWN_SECS:-5}"

for ((i = 1; i <= ITERATIONS; i++)); do
  iter_label="$(printf 'run%02d' "${i}")"
  iter_dir="${REPLAY_ROOT}/${iter_label}"
  mkdir -p "${iter_dir}"

  printf -- '==== %s starting ====\n' "${iter_label}"

  # KEEP_LOG=1 preserves the per-run output dir so we can parse it.
  # LOCAL_CLUSTER_OUTPUT_DIR pins it so we know where to look.
  run_status=0
  (
    KEEP_LOG=1 \
    LOCAL_CLUSTER_OUTPUT_DIR="${iter_dir}" \
      bash "${BENCH_SCRIPT}" 32 "${RPS}" "${DURATION}" \
      > "${iter_dir}/run.log" 2>&1
  ) || run_status=$?

  # Extract the metrics we care about, regardless of whether the run
  # passed or failed its own gates — failing runs are still informative.
  committed="$(jq -r '.total_committed_intents // "null"' \
    "${iter_dir}/committed-summary.json" 2>/dev/null || printf 'null')"
  final_line="$(grep -E 'post-load cluster converged at height=' "${iter_dir}/run.log" || true)"
  final_height="$(printf '%s\n' "${final_line}" \
    | sed -nE 's/.*height=([0-9]+).*/\1/p' | head -n 1)"
  final_hash="$(printf '%s\n' "${final_line}" \
    | sed -nE 's/.*(state_root|hash)=(0x)?([a-fA-F0-9]+).*/\3/p' | head -n 1)"

  # Normalise missing values to a stable sentinel.
  [[ -z "${final_height}" ]] && final_height="null"
  [[ -z "${final_hash}" ]] && final_hash="null"

  printf '%s\t%s\t%s\t%s\t%s\n' \
    "${iter_label}" "${run_status}" "${committed}" "${final_height}" "${final_hash}" \
    >> "${SUMMARY_TSV}"

  tps="null"
  if [[ "${committed}" =~ ^[0-9]+$ ]]; then
    tps="$(python3 -c "print(f'{${committed} / ${DURATION}:.1f}')")"
  fi
  printf -- '==== %s done: exit=%s committed=%s tps=%s height=%s root=%s\n\n' \
    "${iter_label}" "${run_status}" "${committed}" "${tps}" "${final_height}" "${final_hash:0:16}"

  if (( i < ITERATIONS )); then
    sleep "${COOLDOWN_SECS}"
  fi
done

printf -- '----  aggregate summary\n\n'

python3 - "${SUMMARY_TSV}" "${DURATION}" "${ITERATIONS}" <<'PY'
import sys, statistics
from collections import Counter

tsv, duration, iterations = sys.argv[1:4]
duration = int(duration)
iterations = int(iterations)

rows = []
with open(tsv, "r", encoding="utf-8") as fh:
    for line in fh:
        parts = line.rstrip("\n").split("\t")
        if len(parts) != 5:
            continue
        label, status, committed, height, root = parts
        committed_val = int(committed) if committed.isdigit() else None
        height_val = int(height) if height.isdigit() else None
        rows.append({
            "label": label,
            "status": int(status) if status.isdigit() else None,
            "committed": committed_val,
            "tps": (committed_val / duration) if committed_val is not None else None,
            "height": height_val,
            "root": root if root != "null" else None,
        })

header = f"{'run':<7} {'exit':<5} {'committed':>12} {'tps':>10} {'height':>8} {'state_root':<18}"
print(header)
print("-" * len(header))
for r in rows:
    tps_str = f"{r['tps']:.1f}" if r["tps"] is not None else "-"
    committed_str = str(r["committed"]) if r["committed"] is not None else "-"
    height_str = str(r["height"]) if r["height"] is not None else "-"
    root_str = (r["root"][:16] + "…") if r["root"] else "-"
    print(
        f"{r['label']:<7} "
        f"{str(r['status']) if r['status'] is not None else '-':<5} "
        f"{committed_str:>12} "
        f"{tps_str:>10} "
        f"{height_str:>8} "
        f"{root_str:<18}"
    )

print()

tps_values = [r["tps"] for r in rows if r["tps"] is not None]
if len(tps_values) >= 2:
    mean_tps = statistics.mean(tps_values)
    stdev_tps = statistics.pstdev(tps_values)
    cv = (stdev_tps / mean_tps * 100.0) if mean_tps else float("nan")
    min_tps = min(tps_values)
    max_tps = max(tps_values)
    spread_pct = ((max_tps - min_tps) / mean_tps * 100.0) if mean_tps else float("nan")
    print(f"tps: mean={mean_tps:.1f}  stdev={stdev_tps:.1f}  cv={cv:.1f}%"
          f"  min={min_tps:.1f}  max={max_tps:.1f}  spread={spread_pct:.1f}%")
else:
    print("tps: insufficient successful runs for variance analysis")

roots = [r["root"] for r in rows if r["root"]]
unique_roots = list(Counter(roots).items())
if roots:
    print(f"state_roots: {len(unique_roots)} unique value(s) across {len(roots)} converged run(s)")
    for root, count in unique_roots:
        print(f"  {root} ({count} run{'s' if count > 1 else ''})")

heights = [r["height"] for r in rows if r["height"] is not None]
if heights:
    print(f"heights: min={min(heights)} max={max(heights)}")

failed = [r for r in rows if r["status"] not in (0, None)]
if failed:
    print(f"warning: {len(failed)} run(s) exited non-zero — metrics may be partial")

print()
print("interpretation:")
print(
    "  (note: each run's state root is captured at that run's own final "
    "height. if TPS varies, heights vary, and state roots at different "
    "heights are expected to differ even under a deterministic input "
    "stream. read the signals together, not in isolation.)"
)
print()

unique_heights = len(set(heights)) if heights else 0

if len(unique_roots) == 1 and len(roots) == iterations:
    print(
        "  all runs converged to the SAME state root at their final heights "
        "-> strongest signal: input stream is deterministic AND all runs "
        "reached the same commit point.\n"
        "  any remaining tps variance is pre-final-commit pacing (scheduling, "
        "thermal, allocator). not caused by the intent stream or by different "
        "commit progress."
    )
elif len(unique_roots) > 1 and unique_heights == 1:
    print(
        f"  all runs reached the same final height but committed to "
        f"{len(unique_roots)} different state roots -> input stream is NOT "
        "deterministic at this harness shape.\n"
        "  loadgen dispatch ordering, worker scheduling, or admission-order "
        "timing is producing different committed subsets.\n"
        "  this is a clear source of tps variance; runtime-only "
        "interpretation does not apply."
    )
elif len(unique_roots) > 1 and unique_heights > 1:
    print(
        f"  runs converged to different heights ({unique_heights} distinct "
        f"values) AND different state roots ({len(unique_roots)} distinct). "
        "cannot distinguish input nondeterminism from 'same stream, "
        "different progress' with final-height comparison alone.\n"
        "  to disambiguate: query state root at a common earlier height (e.g. "
        "minimum height across runs) via kombat_getStateRootAtHeight. if roots "
        "at the common height match, stream is deterministic and tps variance "
        "is runtime-only. if they differ, stream is nondeterministic."
    )
else:
    print(
        "  insufficient converged runs to classify. increase iterations or "
        "investigate convergence failures before interpreting variance."
    )

PY

echo
echo "----  temporary artifacts at ${REPLAY_ROOT}"
echo "----  to keep them past this shell: set REPLAY_KEEP=1"

if [[ "${REPLAY_KEEP:-0}" == "1" ]]; then
  trap - EXIT INT TERM
  echo "----  REPLAY_KEEP=1 set; keeping ${REPLAY_ROOT}"
fi
