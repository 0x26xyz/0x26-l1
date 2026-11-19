#!/usr/bin/env bash
# Differential check of the prechecked resting-order path against the general
# overlay executor. Replays one deterministic corpus through
# `apply_mahi_committed_blocks_merged` twice, in separate processes:
#
#   fast:    KOMBAT_PHASE6_INPLACE_RESTING_COMMIT unset (in-place path on)
#   general: KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=0     (overlay path only)
#
# and compares the per-step state digests field by field.
set -euo pipefail

chain_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="${1:-${chain_dir}/target/paper-differential}"
mkdir -p "${out_dir}"
test_name="tests::paper_differential::paper_differential_corpus_replays_and_records_digests"

run_mode() {
    local mode="$1"
    shift
    (
        cd "${chain_dir}"
        env -u KOMBAT_PHASE6_INPLACE_RESTING_COMMIT "$@" \
            KOMBAT_MAHI_EXEC_PROFILE=1 \
            PAPER_DIFF_OUT="${out_dir}/${mode}.digests" \
            cargo test --locked -p kombat-chain-node --lib "${test_name}" \
            -- --exact --nocapture --test-threads=1 \
            2> "${out_dir}/${mode}.stderr" > "${out_dir}/${mode}.stdout"
    )
    # Pair each corpus step with the execution mode the engine reported.
    awk '
        /^paper_diff_case / { current = $2; next }
        /^mahi_exec_profile mode=/ {
            split($2, kv, "="); print current, kv[2]
        }
    ' "${out_dir}/${mode}.stderr" > "${out_dir}/${mode}.modes"
}

run_mode fast
run_mode general KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=0

python3 - "${out_dir}" <<'PY'
import collections, json, sys
out = sys.argv[1]

def load(mode):
    rows = {}
    for line in open(f"{out}/{mode}.digests"):
        fields = dict(kv.split("=", 1) for kv in line.split())
        rows[fields["case"]] = fields
    modes = dict(line.split() for line in open(f"{out}/{mode}.modes"))
    return rows, modes

fast, fast_modes = load("fast")
general, general_modes = load("general")
assert fast.keys() == general.keys(), "corpus mismatch between runs"

context = {"case", "shape", "intents", "mempool_seeded"}
summary = {
    "steps": len(fast),
    "fast_run_modes": dict(collections.Counter(fast_modes.values())),
    "general_run_modes": dict(collections.Counter(general_modes.values())),
    "steps_on_inplace_path": 0,
    "inplace_steps_identical": 0,
    "inplace_steps_differing": 0,
    "non_inplace_steps_differing": 0,
    "differing_fields_on_inplace_steps": collections.Counter(),
    "inplace_steps_by_shape": collections.Counter(),
    "outcomes_fast": collections.Counter(),
    "outcomes_general": collections.Counter(),
    "examples": [],
}
for case, f in fast.items():
    g = general[case]
    inplace = fast_modes.get(case) == "native_inplace"
    diff = sorted(k for k in f if k not in context and f[k] != g[k])
    summary["outcomes_fast"][f["outcome"].split(":")[0]] += 1
    summary["outcomes_general"][g["outcome"].split(":")[0]] += 1
    if inplace:
        summary["steps_on_inplace_path"] += 1
        summary["inplace_steps_by_shape"][f["shape"]] += 1
        if diff:
            summary["inplace_steps_differing"] += 1
            summary["differing_fields_on_inplace_steps"].update(diff)
            if len(summary["examples"]) < 5:
                summary["examples"].append({"case": case, "shape": f["shape"], "fields": diff})
        else:
            summary["inplace_steps_identical"] += 1
    elif diff:
        summary["non_inplace_steps_differing"] += 1
        if len(summary["examples"]) < 10:
            summary["examples"].append(
                {"case": case, "shape": f["shape"], "fields": diff, "inplace": False}
            )
json.dump(summary, open(f"{out}/summary.json", "w"), indent=2, sort_keys=True)
print(json.dumps(summary, indent=2, sort_keys=True))
PY
