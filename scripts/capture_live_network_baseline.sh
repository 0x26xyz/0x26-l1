#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  capture_live_network_baseline.sh <generated-assets-dir> [count] [output-dir]

Description:
  Captures a timestamped live-network baseline by running:
  1. live_network_status.sh
  2. benchmark_live_network.sh
  3. benchmark_live_timeout_recovery.sh

  It writes text logs plus a combined baseline.json report.

Environment:
  KOMBAT_SIGN_PATH   passed through to benchmark helpers
EOF
}

if [[ $# -lt 1 || $# -gt 3 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
COUNT="${2:-5}"
OUTPUT_DIR="${3:-/tmp/and-live-baseline-$(date +%Y%m%d-%H%M%S)}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
STATUS_SCRIPT="${CHAIN_DIR}/scripts/live_network_status.sh"
BENCH_SCRIPT="${CHAIN_DIR}/scripts/benchmark_live_network.sh"
TIMEOUT_BENCH_SCRIPT="${CHAIN_DIR}/scripts/benchmark_live_timeout_recovery.sh"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require python3
require git
require tee

mkdir -p "${OUTPUT_DIR}"

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }

STATUS_TEXT="${OUTPUT_DIR}/status.txt"
NETWORK_TEXT="${OUTPUT_DIR}/benchmark-live-network.txt"
NETWORK_JSON="${OUTPUT_DIR}/benchmark-live-network.json"
TIMEOUT_TEXT="${OUTPUT_DIR}/benchmark-live-timeout-recovery.txt"
TIMEOUT_JSON="${OUTPUT_DIR}/benchmark-live-timeout-recovery.json"
BASELINE_JSON="${OUTPUT_DIR}/baseline.json"

if [[ -n "${KOMBAT_SIGN_PATH:-}" ]]; then
  export KOMBAT_SIGN_PATH
fi

info "capturing live status into ${STATUS_TEXT}"
bash "${STATUS_SCRIPT}" "${ASSET_DIR}" | tee "${STATUS_TEXT}"

info "capturing live write benchmark into ${NETWORK_TEXT}"
BENCH_JSON_OUT="${NETWORK_JSON}" \
  bash "${BENCH_SCRIPT}" "${ASSET_DIR}" "${COUNT}" | tee "${NETWORK_TEXT}"

info "capturing live timeout-recovery benchmark into ${TIMEOUT_TEXT}"
BENCH_JSON_OUT="${TIMEOUT_JSON}" \
  bash "${TIMEOUT_BENCH_SCRIPT}" "${ASSET_DIR}" | tee "${TIMEOUT_TEXT}"

info "assembling baseline report ${BASELINE_JSON}"
python3 - "${CHAIN_DIR}" "${OUTPUT_DIR}" "${BASELINE_JSON}" <<'PY'
import json
import pathlib
import subprocess
import sys
from datetime import datetime, timezone

repo_root = pathlib.Path(sys.argv[1]).parent
output_dir = pathlib.Path(sys.argv[2])
baseline_path = pathlib.Path(sys.argv[3])

def load_json(path: pathlib.Path):
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)

revision = subprocess.check_output(
    ["git", "-C", str(repo_root), "rev-parse", "HEAD"],
    text=True,
).strip()

baseline = {
    "captured_at_utc": datetime.now(timezone.utc).isoformat(),
    "git_rev": revision,
    "artifacts": {
        "status_txt": str(output_dir / "status.txt"),
        "benchmark_live_network_txt": str(output_dir / "benchmark-live-network.txt"),
        "benchmark_live_timeout_recovery_txt": str(output_dir / "benchmark-live-timeout-recovery.txt"),
    },
    "status_text": (output_dir / "status.txt").read_text(encoding="utf-8"),
    "benchmark_live_network": load_json(output_dir / "benchmark-live-network.json"),
    "benchmark_live_timeout_recovery": load_json(output_dir / "benchmark-live-timeout-recovery.json"),
}

with baseline_path.open("w", encoding="utf-8") as fh:
    json.dump(baseline, fh, indent=2, sort_keys=True)
PY

ok "baseline captured under ${OUTPUT_DIR}"
