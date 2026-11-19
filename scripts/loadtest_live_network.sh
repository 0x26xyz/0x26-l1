#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  loadtest_live_network.sh <generated-assets-dir> [writer-count] [rps] [duration-secs]

Description:
  Seeds a distinct writer per worker if needed, builds `kombat_loadgen`, then
  runs a sustained live write-load test through a follower validator using the
  real 4-validator network.

Environment:
  LOADTEST_WRITERS_JSON         Reuse an existing writers manifest instead of seeding new writers
  LOADTEST_OUTPUT_DIR           Directory for logs and generated writer manifests
  LOADTEST_WARMUP_SECS          default: 5
  LOADTEST_PROGRESS_EVERY_SECS  default: 5
  LOADTEST_DRAIN_SECS           default: 10
  LOADTEST_PIPELINE_DEPTH       default: 1
  LOADTEST_BATCH_SIZE           default: 1
  LOADTEST_STRICT_NONCE_RETRY   default: 1
  LOADTEST_STRICT_NONCE_RETRY_ATTEMPTS default: 4
  LOADTEST_FAIL_ON_ERRORS       default: 1
  LOADTEST_REQUEST_TIMEOUT_SECS optional per-request HTTP timeout for loadgen
  LOADTEST_TARGET_URL           override target RPC URL
  LOADTEST_STATE_URL            override state-read RPC URL
  LOADTEST_CHAIN_ID             default: 7
  LOADTEST_MARKET_ID            default: 1
  LOADTEST_MARKET_COUNT         default: 1
  KOMBAT_SIGN_PATH              passed through when seeding new writers
EOF
}

if [[ $# -lt 1 || $# -gt 4 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
WRITER_COUNT="${2:-4}"
TARGET_RPS="${3:-40}"
DURATION_SECS="${4:-30}"

HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "loadtest_live_network.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PREPARE_SCRIPT="${CHAIN_DIR}/scripts/prepare_live_loadtest_accounts.sh"
STATUS_SCRIPT="${CHAIN_DIR}/scripts/live_network_status.sh"
LOADGEN_BINARY="${CHAIN_DIR}/target/release/kombat_loadgen"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require cargo
require jq
require python3
require tee

OUTPUT_DIR="${LOADTEST_OUTPUT_DIR:-/tmp/and-live-loadtest-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "${OUTPUT_DIR}"

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }

if [[ -n "${LOADTEST_WRITERS_JSON:-}" ]]; then
  WRITERS_JSON="${LOADTEST_WRITERS_JSON}"
else
  WRITERS_JSON="${OUTPUT_DIR}/writers.json"
  info "seeding ${WRITER_COUNT} load-test writers into ${WRITERS_JSON}"
  bash "${PREPARE_SCRIPT}" "${ASSET_DIR}" "${WRITER_COUNT}" "${WRITERS_JSON}"
fi

if [[ ! -f "${WRITERS_JSON}" ]]; then
  echo "loadtest_live_network.sh: missing writers manifest: ${WRITERS_JSON}" >&2
  exit 1
fi

ACTUAL_WRITER_COUNT="$(jq 'length' "${WRITERS_JSON}")"
if ! [[ "${ACTUAL_WRITER_COUNT}" =~ ^[0-9]+$ ]] || [[ "${ACTUAL_WRITER_COUNT}" -eq 0 ]]; then
  echo "loadtest_live_network.sh: writers manifest is empty: ${WRITERS_JSON}" >&2
  exit 1
fi
if ! [[ "${TARGET_RPS}" =~ ^[0-9]+$ ]] || ! [[ "${DURATION_SECS}" =~ ^[0-9]+$ ]]; then
  echo "loadtest_live_network.sh: rps and duration must be integers" >&2
  exit 1
fi

DEFAULT_VALIDATOR_URL="$(awk -F'\t' 'NR==3 {print $3}' "${HOSTS_FILE}")"
FOLLOWER_URL="${LOADTEST_TARGET_URL:-${DEFAULT_VALIDATOR_URL}}"
STATE_URL="${LOADTEST_STATE_URL:-${DEFAULT_VALIDATOR_URL}}"
if [[ -z "${FOLLOWER_URL}" ]]; then
  echo "loadtest_live_network.sh: failed to derive follower rpc url from ${HOSTS_FILE}" >&2
  exit 1
fi
if [[ -z "${STATE_URL}" ]]; then
  echo "loadtest_live_network.sh: failed to derive state rpc url from ${HOSTS_FILE}" >&2
  exit 1
fi

LOADTEST_WARMUP_SECS="${LOADTEST_WARMUP_SECS:-5}"
LOADTEST_PROGRESS_EVERY_SECS="${LOADTEST_PROGRESS_EVERY_SECS:-5}"
LOADTEST_DRAIN_SECS="${LOADTEST_DRAIN_SECS:-10}"
LOADTEST_PIPELINE_DEPTH="${LOADTEST_PIPELINE_DEPTH:-1}"
LOADTEST_BATCH_SIZE="${LOADTEST_BATCH_SIZE:-1}"
LOADTEST_STRICT_NONCE_RETRY="${LOADTEST_STRICT_NONCE_RETRY:-1}"
LOADTEST_STRICT_NONCE_RETRY_ATTEMPTS="${LOADTEST_STRICT_NONCE_RETRY_ATTEMPTS:-4}"
LOADTEST_FAIL_ON_ERRORS="${LOADTEST_FAIL_ON_ERRORS:-1}"
LOADTEST_REQUEST_TIMEOUT_SECS="${LOADTEST_REQUEST_TIMEOUT_SECS:-}"
LOADTEST_CHAIN_ID="${LOADTEST_CHAIN_ID:-7}"
LOADTEST_MARKET_ID="${LOADTEST_MARKET_ID:-1}"
LOADTEST_MARKET_COUNT="${LOADTEST_MARKET_COUNT:-1}"

LOADTEST_LOG="${OUTPUT_DIR}/loadtest.txt"
STATUS_LOG="${OUTPUT_DIR}/status.txt"
COMMIT_SUMMARY_JSON="${OUTPUT_DIR}/committed-summary.json"

info "building kombat_loadgen"
cargo build --release -p kombat-chain-loadgen --manifest-path "${CHAIN_DIR}/Cargo.toml"

if [[ ! -x "${LOADGEN_BINARY}" ]]; then
  echo "loadtest_live_network.sh: missing loadgen binary: ${LOADGEN_BINARY}" >&2
  exit 1
fi

info "capturing starting policy nonces from ${STATE_URL}"
START_NONCES_JSON="${OUTPUT_DIR}/start-nonces.json"
python3 - "${WRITERS_JSON}" "${STATE_URL}" "${START_NONCES_JSON}" <<'PY'
import json
import subprocess
import sys

writers_path, url, out_path = sys.argv[1:4]
with open(writers_path, "r", encoding="utf-8") as fh:
    writers = json.load(fh)

rows = []
for writer in writers:
    payload = (
        '{"jsonrpc":"2.0","method":"kombat_getPolicy","params":{"policy_id":'
        + str(writer["policy_id"])
        + '},"id":1}'
    )
    raw = subprocess.check_output(
        [
            "curl",
            "-sS",
            "-H",
            "content-type: application/json",
            "--data",
            payload,
            url,
        ],
        text=True,
    )
    response = json.loads(raw)
    rows.append(
        {
            "account_id": writer["account_id"],
            "policy_id": writer["policy_id"],
            "start_next_intent_nonce": response["result"]["next_intent_nonce"],
        }
    )

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(rows, fh, indent=2, sort_keys=True)
PY

info "running live write load against ${FOLLOWER_URL} with writers=${ACTUAL_WRITER_COUNT} rps=${TARGET_RPS} duration=${DURATION_SECS}s"
LOADGEN_ARGS=(
  --url "${FOLLOWER_URL}"
  --state-url "${STATE_URL}"
  --workload place-order
  --writers-json "${WRITERS_JSON}"
  --concurrency "${ACTUAL_WRITER_COUNT}"
  --pipeline-depth "${LOADTEST_PIPELINE_DEPTH}"
  --batch-size "${LOADTEST_BATCH_SIZE}"
  --rps "${TARGET_RPS}"
  --duration-secs "${DURATION_SECS}"
  --warmup-secs "${LOADTEST_WARMUP_SECS}"
  --progress-every-secs "${LOADTEST_PROGRESS_EVERY_SECS}"
  --chain-id "${LOADTEST_CHAIN_ID}"
  --market-id "${LOADTEST_MARKET_ID}"
  --market-count "${LOADTEST_MARKET_COUNT}"
)
if [[ "${LOADTEST_STRICT_NONCE_RETRY}" == "1" || "${LOADTEST_STRICT_NONCE_RETRY}" == "true" ]]; then
  LOADGEN_ARGS+=(--strict-nonce-retry --strict-nonce-retry-attempts "${LOADTEST_STRICT_NONCE_RETRY_ATTEMPTS}")
fi
if [[ "${LOADTEST_FAIL_ON_ERRORS}" == "1" || "${LOADTEST_FAIL_ON_ERRORS}" == "true" ]]; then
  LOADGEN_ARGS+=(--fail-on-errors)
fi
if [[ -n "${LOADTEST_REQUEST_TIMEOUT_SECS}" ]]; then
  LOADGEN_ARGS+=(--request-timeout-secs "${LOADTEST_REQUEST_TIMEOUT_SECS}")
fi

"${LOADGEN_BINARY}" \
  "${LOADGEN_ARGS[@]}" \
  | tee "${LOADTEST_LOG}"

if [[ "${LOADTEST_DRAIN_SECS}" -gt 0 ]]; then
  info "waiting ${LOADTEST_DRAIN_SECS}s for policy nonce frontiers to settle"
  sleep "${LOADTEST_DRAIN_SECS}"
fi

info "capturing committed nonce deltas into ${COMMIT_SUMMARY_JSON}"
python3 - "${START_NONCES_JSON}" "${STATE_URL}" "${COMMIT_SUMMARY_JSON}" <<'PY'
import json
import statistics
import subprocess
import sys

start_path, url, out_path = sys.argv[1:4]
with open(start_path, "r", encoding="utf-8") as fh:
    start_rows = json.load(fh)

rows = []
for row in start_rows:
    payload = (
        '{"jsonrpc":"2.0","method":"kombat_getPolicy","params":{"policy_id":'
        + str(row["policy_id"])
        + '},"id":1}'
    )
    raw = subprocess.check_output(
        [
            "curl",
            "-sS",
            "-H",
            "content-type: application/json",
            "--data",
            payload,
            url,
        ],
        text=True,
    )
    response = json.loads(raw)
    end_nonce = response["result"]["next_intent_nonce"]
    committed = max(0, end_nonce - row["start_next_intent_nonce"])
    rows.append(
        {
            **row,
            "end_next_intent_nonce": end_nonce,
            "committed_intents": committed,
        }
    )

summary = {
    "writers": rows,
    "total_committed_intents": sum(row["committed_intents"] for row in rows),
    "min_committed_per_writer": min(row["committed_intents"] for row in rows),
    "max_committed_per_writer": max(row["committed_intents"] for row in rows),
    "avg_committed_per_writer": statistics.mean(
        row["committed_intents"] for row in rows
    ),
}

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)

print(
    "PASS  committed summary: total={} min_per_writer={} avg_per_writer={:.2f} max_per_writer={}".format(
        summary["total_committed_intents"],
        summary["min_committed_per_writer"],
        summary["avg_committed_per_writer"],
        summary["max_committed_per_writer"],
    )
)
for row in rows:
    print(
        "PASS  writer account={} policy={} start_nonce={} end_nonce={} committed={}".format(
            row["account_id"],
            row["policy_id"],
            row["start_next_intent_nonce"],
            row["end_next_intent_nonce"],
            row["committed_intents"],
        )
    )
PY

info "capturing post-load network status into ${STATUS_LOG}"
bash "${STATUS_SCRIPT}" "${ASSET_DIR}" | tee "${STATUS_LOG}"

ok "live load test artifacts written to ${OUTPUT_DIR}"
