#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  loadtest_live_network_from_validator.sh <generated-assets-dir> <validator-name> [writer-count] [rps] [duration-secs]

Description:
  Seeds distinct writers if needed, copies a Linux `kombat_loadgen` binary to
  the selected validator host, then runs the write load locally on that host
  against `http://127.0.0.1:8545` to avoid WAN-induced client distortion.

Environment:
  LOADTEST_WRITERS_JSON           Reuse an existing writers manifest instead of seeding new writers
  LOADTEST_OUTPUT_DIR             Directory for logs and generated writer manifests
  LOADTEST_WARMUP_SECS            default: 5
  LOADTEST_PROGRESS_EVERY_SECS    default: 5
  LOADTEST_DRAIN_SECS             default: 10
  LOADTEST_PIPELINE_DEPTH         default: 1
  LOADTEST_BATCH_SIZE             default: 1
  LOADTEST_STRICT_NONCE_RETRY     default: 1
  LOADTEST_STRICT_NONCE_RETRY_ATTEMPTS default: 4
  LOADTEST_FAIL_ON_ERRORS         default: 1
  LOADTEST_REQUEST_TIMEOUT_SECS   optional per-request HTTP timeout for loadgen
  LOADTEST_TARGET_URL             default: http://127.0.0.1:8545 on selected validator
  LOADTEST_STATE_URL              default: http://127.0.0.1:8545 on selected validator
  LOADTEST_RUNNER_SSH             SSH host that runs kombat_loadgen. Defaults to
                                  the selected validator SSH host.
  LOADTEST_RUNNER_SSH_OPTS        Optional space-separated ssh/scp options
                                  used for remote runner access.
  LOADTEST_CHAIN_ID               default: 7
  LOADTEST_MARKET_ID              default: 1
  LOADTEST_MARKET_COUNT           default: 1
  LOADTEST_REMOTE_TMP_DIR         default: /tmp/and-live-loadtest-remote
  LOADTEST_REMOTE_CACHE_DIR       optional stable remote cache root. When set,
                                  loadgen binaries, writer manifests, and
                                  payload caches survive between runs.
  LOADTEST_REMOTE_LOADGEN_BINARY  default: chain/target-linux-x86_64/release/kombat_loadgen
  LOADTEST_PAYLOAD_CACHE          set to 1 to pass a stable remote payload
                                  cache dir to kombat_loadgen
  LOADTEST_PAYLOAD_CACHE_DIR      explicit remote payload cache dir
  LOADTEST_INTENT_TTL_MS          passed through to kombat_loadgen
  LOADTEST_CLIENT_ORDER_ID_BASE   passed through to kombat_loadgen
  KOMBAT_SIGN_PATH                passed through when seeding new writers
EOF
}

if [[ $# -lt 2 || $# -gt 5 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
VALIDATOR_NAME="$2"
WRITER_COUNT="${3:-4}"
TARGET_RPS="${4:-40}"
DURATION_SECS="${5:-30}"

HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "loadtest_live_network_from_validator.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PREPARE_SCRIPT="${CHAIN_DIR}/scripts/prepare_live_loadtest_accounts.sh"
STATUS_SCRIPT="${CHAIN_DIR}/scripts/live_network_status.sh"
LOADGEN_BINARY="${LOADTEST_REMOTE_LOADGEN_BINARY:-${CHAIN_DIR}/target-linux-x86_64/release/kombat_loadgen}"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require jq
require python3
require scp
require ssh
require tee

TARGET_SSH="$(awk -F'\t' -v name="${VALIDATOR_NAME}" '$1 == name {print $2}' "${HOSTS_FILE}")"
TARGET_RPC_URL="$(awk -F'\t' -v name="${VALIDATOR_NAME}" '$1 == name {print $3}' "${HOSTS_FILE}")"
if [[ -z "${TARGET_SSH}" || -z "${TARGET_RPC_URL}" ]]; then
  echo "loadtest_live_network_from_validator.sh: failed to resolve ${VALIDATOR_NAME} in ${HOSTS_FILE}" >&2
  exit 1
fi
RUNNER_SSH="${LOADTEST_RUNNER_SSH:-${TARGET_SSH}}"
RUNNER_SSH_OPTS=()
if [[ -n "${LOADTEST_RUNNER_SSH_OPTS:-}" ]]; then
  read -r -a RUNNER_SSH_OPTS <<< "${LOADTEST_RUNNER_SSH_OPTS}"
fi

if [[ ! -x "${LOADGEN_BINARY}" ]]; then
  echo "loadtest_live_network_from_validator.sh: missing Linux loadgen binary: ${LOADGEN_BINARY}" >&2
  echo "build it first, for example with Docker into chain/target-linux-x86_64/release/kombat_loadgen" >&2
  exit 1
fi

OUTPUT_DIR="${LOADTEST_OUTPUT_DIR:-/tmp/and-live-loadtest-node-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "${OUTPUT_DIR}"

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
LOADTEST_REMOTE_TMP_DIR="${LOADTEST_REMOTE_TMP_DIR:-/tmp/and-live-loadtest-remote}"
LOADTEST_REMOTE_CACHE_DIR="${LOADTEST_REMOTE_CACHE_DIR:-}"
LOADTEST_TARGET_URL="${LOADTEST_TARGET_URL:-http://127.0.0.1:8545}"
LOADTEST_STATE_URL="${LOADTEST_STATE_URL:-http://127.0.0.1:8545}"
LOADTEST_INTENT_TTL_MS="${LOADTEST_INTENT_TTL_MS:-}"
LOADTEST_CLIENT_ORDER_ID_BASE="${LOADTEST_CLIENT_ORDER_ID_BASE:-}"
if [[ "${LOADTEST_PAYLOAD_CACHE:-0}" == "1" || -n "${LOADTEST_PAYLOAD_CACHE_DIR:-}" ]]; then
  LOADTEST_INTENT_TTL_MS="${LOADTEST_INTENT_TTL_MS:-315360000000}"
  LOADTEST_CLIENT_ORDER_ID_BASE="${LOADTEST_CLIENT_ORDER_ID_BASE:-1000000000000}"
fi

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
  echo "loadtest_live_network_from_validator.sh: missing writers manifest: ${WRITERS_JSON}" >&2
  exit 1
fi

ACTUAL_WRITER_COUNT="$(jq 'length' "${WRITERS_JSON}")"
if ! [[ "${ACTUAL_WRITER_COUNT}" =~ ^[0-9]+$ ]] || [[ "${ACTUAL_WRITER_COUNT}" -eq 0 ]]; then
  echo "loadtest_live_network_from_validator.sh: writers manifest is empty: ${WRITERS_JSON}" >&2
  exit 1
fi
if ! [[ "${TARGET_RPS}" =~ ^[0-9]+$ ]] || ! [[ "${DURATION_SECS}" =~ ^[0-9]+$ ]]; then
  echo "loadtest_live_network_from_validator.sh: rps and duration must be integers" >&2
  exit 1
fi

LOADTEST_LOG="${OUTPUT_DIR}/loadtest.txt"
STATUS_LOG="${OUTPUT_DIR}/status.txt"
COMMIT_SUMMARY_JSON="${OUTPUT_DIR}/committed-summary.json"
START_NONCES_JSON="${OUTPUT_DIR}/start-nonces.json"

info "capturing starting policy nonces from ${TARGET_RPC_URL}"
python3 - "${WRITERS_JSON}" "${TARGET_RPC_URL}" "${START_NONCES_JSON}" <<'PY'
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

REMOTE_WRITERS_JSON="${LOADTEST_REMOTE_TMP_DIR}/writers.json"
REMOTE_LOADGEN_BINARY="${LOADTEST_REMOTE_TMP_DIR}/kombat_loadgen"
REMOTE_LOADGEN_SHA="${LOADTEST_REMOTE_TMP_DIR}/kombat_loadgen.sha256"
REMOTE_WRITERS_SHA="${LOADTEST_REMOTE_TMP_DIR}/writers.sha256"

local_loadgen_sha="$(shasum -a 256 "${LOADGEN_BINARY}" | awk '{print $1}')"
local_writers_sha="$(shasum -a 256 "${WRITERS_JSON}" | awk '{print $1}')"

info "preparing loadgen cache for ${VALIDATOR_NAME} on runner ${RUNNER_SSH}"
ssh "${RUNNER_SSH_OPTS[@]}" "${RUNNER_SSH}" "mkdir -p '${LOADTEST_REMOTE_TMP_DIR}'"
remote_loadgen_sha="$(ssh "${RUNNER_SSH_OPTS[@]}" "${RUNNER_SSH}" "cat '${REMOTE_LOADGEN_SHA}' 2>/dev/null || true")"
if [[ "${remote_loadgen_sha}" != "${local_loadgen_sha}" ]]; then
  info "copying loadgen binary for ${VALIDATOR_NAME} to runner ${RUNNER_SSH}"
  scp "${RUNNER_SSH_OPTS[@]}" "${LOADGEN_BINARY}" "${RUNNER_SSH}:${REMOTE_LOADGEN_BINARY}"
  ssh "${RUNNER_SSH_OPTS[@]}" "${RUNNER_SSH}" "printf '%s\n' '${local_loadgen_sha}' > '${REMOTE_LOADGEN_SHA}'"
else
  ok "reusing cached loadgen binary on ${RUNNER_SSH}"
fi
remote_writers_sha="$(ssh "${RUNNER_SSH_OPTS[@]}" "${RUNNER_SSH}" "cat '${REMOTE_WRITERS_SHA}' 2>/dev/null || true")"
if [[ "${remote_writers_sha}" != "${local_writers_sha}" ]]; then
  info "copying writers manifest for ${VALIDATOR_NAME} to runner ${RUNNER_SSH}"
  scp "${RUNNER_SSH_OPTS[@]}" "${WRITERS_JSON}" "${RUNNER_SSH}:${REMOTE_WRITERS_JSON}"
  ssh "${RUNNER_SSH_OPTS[@]}" "${RUNNER_SSH}" "printf '%s\n' '${local_writers_sha}' > '${REMOTE_WRITERS_SHA}'"
else
  ok "reusing cached writers manifest on ${RUNNER_SSH}"
fi

REMOTE_CMD=(
  "chmod +x '${REMOTE_LOADGEN_BINARY}'"
  "&&"
  "'${REMOTE_LOADGEN_BINARY}'"
  "--url" "'${LOADTEST_TARGET_URL}'"
  "--state-url" "'${LOADTEST_STATE_URL}'"
  "--workload" "place-order"
  "--writers-json" "'${REMOTE_WRITERS_JSON}'"
  "--concurrency" "${ACTUAL_WRITER_COUNT}"
  "--pipeline-depth" "${LOADTEST_PIPELINE_DEPTH}"
  "--batch-size" "${LOADTEST_BATCH_SIZE}"
  "--rps" "${TARGET_RPS}"
  "--duration-secs" "${DURATION_SECS}"
  "--warmup-secs" "${LOADTEST_WARMUP_SECS}"
  "--progress-every-secs" "${LOADTEST_PROGRESS_EVERY_SECS}"
  "--chain-id" "${LOADTEST_CHAIN_ID}"
  "--market-id" "${LOADTEST_MARKET_ID}"
  "--market-count" "${LOADTEST_MARKET_COUNT}"
)
if [[ -n "${LOADTEST_INTENT_TTL_MS}" ]]; then
  REMOTE_CMD+=("--intent-ttl-ms" "${LOADTEST_INTENT_TTL_MS}")
fi
if [[ -n "${LOADTEST_CLIENT_ORDER_ID_BASE}" ]]; then
  REMOTE_CMD+=("--client-order-id-base" "${LOADTEST_CLIENT_ORDER_ID_BASE}")
fi
if [[ "${LOADTEST_PAYLOAD_CACHE:-0}" == "1" || -n "${LOADTEST_PAYLOAD_CACHE_DIR:-}" ]]; then
  REMOTE_PAYLOAD_CACHE_DIR="${LOADTEST_PAYLOAD_CACHE_DIR:-${LOADTEST_REMOTE_TMP_DIR}/payload-cache}"
  REMOTE_CMD+=("--payload-cache-dir" "'${REMOTE_PAYLOAD_CACHE_DIR}'")
fi
if [[ "${LOADTEST_STRICT_NONCE_RETRY}" == "1" || "${LOADTEST_STRICT_NONCE_RETRY}" == "true" ]]; then
  REMOTE_CMD+=("--strict-nonce-retry" "--strict-nonce-retry-attempts" "${LOADTEST_STRICT_NONCE_RETRY_ATTEMPTS}")
fi
if [[ "${LOADTEST_FAIL_ON_ERRORS}" == "1" || "${LOADTEST_FAIL_ON_ERRORS}" == "true" ]]; then
  REMOTE_CMD+=("--fail-on-errors")
fi
if [[ -n "${LOADTEST_REQUEST_TIMEOUT_SECS}" ]]; then
  REMOTE_CMD+=("--request-timeout-secs" "${LOADTEST_REQUEST_TIMEOUT_SECS}")
fi

info "running write load for ${VALIDATOR_NAME} on runner ${RUNNER_SSH} with target=${LOADTEST_TARGET_URL} writers=${ACTUAL_WRITER_COUNT} rps=${TARGET_RPS} duration=${DURATION_SECS}s"
ssh "${RUNNER_SSH_OPTS[@]}" "${RUNNER_SSH}" "$(printf '%s ' "${REMOTE_CMD[@]}")" | tee "${LOADTEST_LOG}"

if [[ "${LOADTEST_DRAIN_SECS}" -gt 0 ]]; then
  info "waiting ${LOADTEST_DRAIN_SECS}s for policy nonce frontiers to settle"
  sleep "${LOADTEST_DRAIN_SECS}"
fi

info "capturing committed nonce deltas into ${COMMIT_SUMMARY_JSON}"
python3 - "${START_NONCES_JSON}" "${TARGET_RPC_URL}" "${COMMIT_SUMMARY_JSON}" <<'PY'
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

ok "validator-local live load test artifacts written to ${OUTPUT_DIR}"
