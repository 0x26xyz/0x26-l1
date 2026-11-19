#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  loadtest_local_cluster.sh [writer-count] [rps] [duration-secs]

Description:
  Starts a 4-validator localhost cluster with peer bootstrap, seeds one writer
  account/policy per worker, runs `kombat_loadgen` against a local validator,
  and captures pre/post cluster status for fast transport and consensus
  iteration without the remote deploy loop.

Environment:
  LOCAL_CLUSTER_PROFILE          build profile: dev|release (default: release)
  LOCAL_CLUSTER_SKIP_BUILD       set to 1 to skip cargo build
  LOCAL_CLUSTER_OUTPUT_DIR       directory for generated assets and logs
  BASE_PORT                      base public RPC port; default random 21000-21999
  NETWORK_BASE_PORT              base internal network RPC port; default: BASE_PORT+100
  CHAIN_ID                       default: 7
  OPERATOR_TOKEN                 default: network-round-token
  AUTO_INTERVAL_MS               default: 50
  AUTO_DRIVE_INTERVAL_MS         default: min(AUTO_INTERVAL_MS, 25)
  AUTO_MAX_ITEMS                 default: 100
  LOCAL_CLUSTER_PEER_TRANSPORT  set to 1 to enable validator peer transport
  PEER_KEEPALIVE_MS             default: 250
  LOCAL_CLUSTER_SUBMIT_FORWARDING set to 0 to disable follower->leader submit forwarding
  LOCAL_CLUSTER_ENABLE_INGRESS    set to 1 to start a dedicated observer ingress
  INGRESS_PORT                    public RPC port for ingress; default: BASE_PORT+10
  LOCAL_CLUSTER_ENABLE_MARKET_DATA set to 1 to start QuestDB plus kombat_market_data
  MARKET_DATA_API_PORT            default: BASE_PORT+210
  QUESTDB_HTTP_PORT               default: BASE_PORT+200
  QUESTDB_PG_PORT                 default: BASE_PORT+201
  QUESTDB_ILP_TCP_PORT            default: BASE_PORT+202
  QUESTDB_IMAGE                   default: questdb/questdb:8.2.3
  LOADTEST_TARGET_VALIDATOR      validator-1|validator-2|validator-3|validator-4|ingress|all|comma-list (default: validator-2)
  LOADTEST_WARMUP_SECS           default: 5
  LOADTEST_PROGRESS_EVERY_SECS   default: 5
  LOADTEST_DRAIN_SECS            default: 10
  LOADTEST_REQUEST_TIMEOUT_SECS  optional per-request timeout for loadgen
  LOADTEST_COMMIT_TIMESERIES    set to 0 to disable per-second committed TPS sampling
  LOADTEST_COMMIT_SAMPLE_SECS   committed TPS sample interval, default 1
  LOADGEN_STATE_URL              optional read/state URL for loadgen bootstrap reads
  LOADTEST_PIPELINE_DEPTH        default: 1
  LOADTEST_BATCH_SIZE            default: 1
  LOADTEST_MARKET_ID             default: 7
  LOADTEST_MARKET_COUNT          default: 1
  KEEP_LOG                       set to 1 to keep output dir on success
EOF
}

if [[ $# -gt 3 ]]; then
  usage >&2
  exit 1
fi

WRITER_COUNT="${1:-4}"
TARGET_RPS="${2:-8}"
DURATION_SECS="${3:-20}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
PROFILE="${LOCAL_CLUSTER_PROFILE:-release}"
SKIP_BUILD="${LOCAL_CLUSTER_SKIP_BUILD:-0}"
CHAIN_ID="${CHAIN_ID:-7}"
OPERATOR_TOKEN="${OPERATOR_TOKEN:-network-round-token}"
AUTO_INTERVAL_MS="${AUTO_INTERVAL_MS:-50}"
AUTO_DRIVE_INTERVAL_MS="${AUTO_DRIVE_INTERVAL_MS:-$(( AUTO_INTERVAL_MS < 25 ? AUTO_INTERVAL_MS : 25 ))}"
AUTO_MAX_ITEMS="${AUTO_MAX_ITEMS:-100}"
LOCAL_CLUSTER_PEER_TRANSPORT="${LOCAL_CLUSTER_PEER_TRANSPORT:-0}"
PEER_KEEPALIVE_MS="${PEER_KEEPALIVE_MS:-250}"
LOCAL_CLUSTER_SUBMIT_FORWARDING="${LOCAL_CLUSTER_SUBMIT_FORWARDING:-1}"
LOCAL_CLUSTER_ENABLE_INGRESS="${LOCAL_CLUSTER_ENABLE_INGRESS:-0}"
LOCAL_CLUSTER_ENABLE_MARKET_DATA="${LOCAL_CLUSTER_ENABLE_MARKET_DATA:-0}"
LOADTEST_TARGET_VALIDATOR="${LOADTEST_TARGET_VALIDATOR:-validator-2}"
LOADTEST_WARMUP_SECS="${LOADTEST_WARMUP_SECS:-5}"
LOADTEST_PROGRESS_EVERY_SECS="${LOADTEST_PROGRESS_EVERY_SECS:-5}"
LOADTEST_DRAIN_SECS="${LOADTEST_DRAIN_SECS:-10}"
LOADTEST_PIPELINE_DEPTH="${LOADTEST_PIPELINE_DEPTH:-1}"
LOADTEST_BATCH_SIZE="${LOADTEST_BATCH_SIZE:-1}"
LOADTEST_MARKET_ID="${LOADTEST_MARKET_ID:-7}"
LOADTEST_MARKET_COUNT="${LOADTEST_MARKET_COUNT:-1}"
RPC_TIMEOUT_SECS="${LOADTEST_RPC_TIMEOUT_SECS:-3}"
choose_free_base_port() {
  local candidate
  local port
  for _ in $(seq 1 200); do
    candidate=$((21000 + (RANDOM % 1000)))
    local blocked=0
    for port in \
      "${candidate}" "$((candidate + 1))" "$((candidate + 2))" "$((candidate + 3))" \
      "$((candidate + 10))" \
      "$((candidate + 100))" "$((candidate + 101))" "$((candidate + 102))" "$((candidate + 103))" \
      "$((candidate + 200))" "$((candidate + 201))" "$((candidate + 202))" "$((candidate + 210))"; do
      if lsof -ti tcp:"${port}" >/dev/null 2>&1; then
        blocked=1
        break
      fi
    done
    if [[ "${blocked}" -eq 0 ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done
  return 1
}

BASE_PORT="${BASE_PORT:-$(choose_free_base_port)}"
if [[ -z "${BASE_PORT}" ]]; then
  echo "loadtest_local_cluster.sh: failed to choose a free localhost port range" >&2
  exit 1
fi
NETWORK_BASE_PORT="${NETWORK_BASE_PORT:-$((BASE_PORT + 100))}"
INGRESS_PORT="${INGRESS_PORT:-$((BASE_PORT + 10))}"
QUESTDB_HTTP_PORT="${QUESTDB_HTTP_PORT:-$((BASE_PORT + 200))}"
QUESTDB_PG_PORT="${QUESTDB_PG_PORT:-$((BASE_PORT + 201))}"
QUESTDB_ILP_TCP_PORT="${QUESTDB_ILP_TCP_PORT:-$((BASE_PORT + 202))}"
MARKET_DATA_API_PORT="${MARKET_DATA_API_PORT:-$((BASE_PORT + 210))}"
QUESTDB_IMAGE="${QUESTDB_IMAGE:-questdb/questdb:8.2.3}"

if ! [[ "${WRITER_COUNT}" =~ ^[0-9]+$ ]] || [[ "${WRITER_COUNT}" -eq 0 ]]; then
  echo "loadtest_local_cluster.sh: writer-count must be a positive integer" >&2
  exit 1
fi
if ! [[ "${TARGET_RPS}" =~ ^[0-9]+$ ]] || ! [[ "${DURATION_SECS}" =~ ^[0-9]+$ ]]; then
  echo "loadtest_local_cluster.sh: rps and duration must be integers" >&2
  exit 1
fi

if [[ "${PROFILE}" == "release" ]]; then
  BUILD_ARGS=(--release)
  BINARY_DIR="${CHAIN_DIR}/target/release"
else
  BUILD_ARGS=()
  BINARY_DIR="${CHAIN_DIR}/target/debug"
fi

BINARY_PATH="${KOMBAT_BINARY_PATH:-${BINARY_DIR}/kombat_chain}"
SIGN_BINARY_PATH="${KOMBAT_SIGN_PATH:-${BINARY_DIR}/kombat_sign}"
LOADGEN_BINARY_PATH="${LOADGEN_BINARY_PATH:-${BINARY_DIR}/kombat_loadgen}"
MARKET_DATA_BINARY_PATH="${MARKET_DATA_BINARY_PATH:-${BINARY_DIR}/kombat_market_data}"
PREPARE_SCRIPT="${CHAIN_DIR}/scripts/prepare_live_loadtest_accounts.sh"
STATUS_SCRIPT="${CHAIN_DIR}/scripts/live_network_status.sh"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require cargo
require curl
require jq
require python3
require lsof
require tee
if [[ "${LOCAL_CLUSTER_ENABLE_MARKET_DATA}" == "1" ]]; then
  require docker
fi

OUTPUT_DIR="${LOCAL_CLUSTER_OUTPUT_DIR:-$(mktemp -d -t kombat-local-loadtest.XXXXXX)}"
mkdir -p "${OUTPUT_DIR}"
VALIDATOR_SET_PATH="${OUTPUT_DIR}/validator-set.toml"
HOSTS_FILE="${OUTPUT_DIR}/hosts.txt"
WRITERS_JSON="${OUTPUT_DIR}/writers.json"
START_NONCES_JSON="${OUTPUT_DIR}/start-nonces.json"
COMMIT_SUMMARY_JSON="${OUTPUT_DIR}/committed-summary.json"
COMMIT_TIMESERIES_JSON="${OUTPUT_DIR}/committed-timeseries.json"
LOADTEST_LOG="${OUTPUT_DIR}/loadtest.txt"
STATUS_BEFORE_LOG="${OUTPUT_DIR}/status-before.txt"
STATUS_AFTER_LOG="${OUTPUT_DIR}/status-after.txt"
QUESTDB_LOG_PATH="${OUTPUT_DIR}/questdb.txt"
MARKET_DATA_LOG_PATH="${OUTPUT_DIR}/market-data.txt"
MARKET_DATA_API_URL_FILE="${OUTPUT_DIR}/market-data-url.txt"

PASS=0
FAIL=0
PORTS=(
  "${BASE_PORT}"
  "$((BASE_PORT + 1))"
  "$((BASE_PORT + 2))"
  "$((BASE_PORT + 3))"
)
NETWORK_PORTS=(
  "${NETWORK_BASE_PORT}"
  "$((NETWORK_BASE_PORT + 1))"
  "$((NETWORK_BASE_PORT + 2))"
  "$((NETWORK_BASE_PORT + 3))"
)
VALIDATOR_NAMES=(validator-1 validator-2 validator-3 validator-4)
VALIDATOR_IDS=(
  0101010101010101010101010101010101010101010101010101010101010101
  0202020202020202020202020202020202020202020202020202020202020202
  0303030303030303030303030303030303030303030303030303030303030303
  0404040404040404040404040404040404040404040404040404040404040404
)
CONSENSUS_SEEDS=(
  0101010101010101010101010101010101010101010101010101010101010101
  0202020202020202020202020202020202020202020202020202020202020202
  0303030303030303030303030303030303030303030303030303030303030303
  0404040404040404040404040404040404040404040404040404040404040404
)
CONSENSUS_PUBKEYS=(
  8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c
  8139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b394
  ed4928c628d1c2c6eae90338905995612959273a5c63f93636c14614ac8737d1
  ca93ac1705187071d67b83c7ff0efe8108e8ec4530575d7726879333dbdabe7c
)
NETWORK_PUBKEYS=(
  212121212121212121212121212121212121212121212121
  222222222222222222222222222222222222222222222222
  232323232323232323232323232323232323232323232323
  242424242424242424242424242424242424242424242424
)
REGIONS=(local-a local-b local-c local-d)
OPERATORS=(validator-a validator-b validator-c validator-d)
LOG_PATHS=(
  "${OUTPUT_DIR}/validator-1.log"
  "${OUTPUT_DIR}/validator-2.log"
  "${OUTPUT_DIR}/validator-3.log"
  "${OUTPUT_DIR}/validator-4.log"
)
INGRESS_LOG_PATH="${OUTPUT_DIR}/ingress.log"
PIDS=("" "" "" "")
INGRESS_PID=""
COMMIT_SAMPLER_PID=""
QUESTDB_PID=""
QUESTDB_CONTAINER="kombat-local-questdb-${BASE_PORT}"
MARKET_DATA_PID=""

ok()   { printf -- 'PASS  %s\n' "$*"; PASS=$((PASS + 1)); }
bad()  { printf -- 'FAIL  %s\n' "$*" >&2; FAIL=$((FAIL + 1)); }
info() { printf -- '----  %s\n' "$*"; }

cleanup() {
  local pid
  for pid in "${PIDS[@]}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  if [[ -n "${INGRESS_PID}" ]] && kill -0 "${INGRESS_PID}" 2>/dev/null; then
    kill "${INGRESS_PID}" 2>/dev/null || true
    wait "${INGRESS_PID}" 2>/dev/null || true
  fi
  if [[ -n "${COMMIT_SAMPLER_PID}" ]] && kill -0 "${COMMIT_SAMPLER_PID}" 2>/dev/null; then
    kill "${COMMIT_SAMPLER_PID}" 2>/dev/null || true
    wait "${COMMIT_SAMPLER_PID}" 2>/dev/null || true
  fi
  if [[ -n "${MARKET_DATA_PID}" ]] && kill -0 "${MARKET_DATA_PID}" 2>/dev/null; then
    kill "${MARKET_DATA_PID}" 2>/dev/null || true
    wait "${MARKET_DATA_PID}" 2>/dev/null || true
  fi
  if [[ "${LOCAL_CLUSTER_ENABLE_MARKET_DATA}" == "1" ]]; then
    docker stop "${QUESTDB_CONTAINER}" >/dev/null 2>&1 || true
    if [[ -n "${QUESTDB_PID}" ]]; then
      wait "${QUESTDB_PID}" 2>/dev/null || true
    fi
  fi
  if [[ "${FAIL}" -ne 0 || "${KEEP_LOG:-0}" == "1" ]]; then
    info "logs kept in ${OUTPUT_DIR}"
  else
    rm -rf "${OUTPUT_DIR}"
  fi
}
trap cleanup EXIT INT TERM

rpc_post() {
  local url="$1"
  curl --max-time "${RPC_TIMEOUT_SECS}" -sS -H 'content-type: application/json' --data-binary @- "${url}"
}

wait_for_ready() {
  local url="$1"
  local label="$2"
  local pid="${3:-}"
  local deadline=$(( $(date +%s) + 20 ))
  while (( $(date +%s) < deadline )); do
    if [[ -n "${pid}" ]] && ! kill -0 "${pid}" 2>/dev/null; then
      bad "${label} exited before becoming ready"
      return 1
    fi
    local resp
    resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}' \
      | rpc_post "${url}" 2>/dev/null || true)
    if printf '%s' "${resp}" | jq -e '.result.chain_id' >/dev/null 2>&1; then
      ok "${label} reachable"
      return 0
    fi
    sleep 0.2
  done
  bad "${label} never became ready"
  return 1
}

chain_info_pair() {
  local url="$1"
  local resp
  resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":2}' | rpc_post "${url}")
  printf '%s %s\n' \
    "$(printf '%s' "${resp}" | jq -r '.result.head_height')" \
    "$(printf '%s' "${resp}" | jq -r '.result.head_hash_hex')"
}

chain_info_triplet() {
  local url="$1"
  local resp
  resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":2}' | rpc_post "${url}")
  printf '%s %s %s\n' \
    "$(printf '%s' "${resp}" | jq -r '.result.head_height')" \
    "$(printf '%s' "${resp}" | jq -r '.result.head_hash_hex')" \
    "$(printf '%s' "${resp}" | jq -r '.result.committed_state_root_hex')"
}

mahi_commit_snapshot() {
  local url="$1"
  local resp
  resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_getConsensusMetrics","params":{},"id":3}' | rpc_post "${url}")
  printf '%s %s\n' \
    "$(printf '%s' "${resp}" | jq -r '.result.mahi.committed_block_count')" \
    "$(printf '%s' "${resp}" | jq -r '.result.mahi.commit_log_hash_hex')"
}

state_root_at_height_pair() {
  local url="$1"
  local height="$2"
  local resp
  resp=$(printf '{"jsonrpc":"2.0","method":"kombat_getStateRootAtHeight","params":{"height":%s},"id":4}' "${height}" | rpc_post "${url}")
  printf '%s %s\n' \
    "$(printf '%s' "${resp}" | jq -r '.result.block_hash_hex')" \
    "$(printf '%s' "${resp}" | jq -r '.result.state_root_hex')"
}

bootstrap_round() {
  local url="$1"
  curl -sS "${url%/}/network/bootstrap" | jq -r '.current_round'
}

wait_for_cluster_convergence() {
  if [[ "${KOMBAT_CONSENSUS:-mahi}" == "mahi" ]]; then
    wait_for_mahi_state_consistency "$1"
    return
  fi
  local label="$1"
  local deadline=$(( $(date +%s) + 20 ))
  while (( $(date +%s) < deadline )); do
    local expected_height=""
    local expected_hash=""
    local all_match=1
    local index
    for index in 0 1 2 3; do
      local url="http://127.0.0.1:${PORTS[$index]}"
      read -r height hash < <(chain_info_pair "${url}" 2>/dev/null || printf ' -\n')
      if ! [[ "${height}" =~ ^[0-9]+$ ]] || [[ -z "${hash}" || "${hash}" == "-" ]]; then
        all_match=0
        break
      fi
      if [[ -z "${expected_height}" ]]; then
        expected_height="${height}"
        expected_hash="${hash}"
      elif [[ "${height}" != "${expected_height}" || "${hash}" != "${expected_hash}" ]]; then
        all_match=0
        break
      fi
    done
    if [[ "${all_match}" -eq 1 ]]; then
      printf '%s %s\n' "${expected_height}" "${expected_hash}"
      return 0
    fi
    sleep 0.2
  done
  bad "${label} cluster never converged"
  return 1
}

wait_for_mahi_state_consistency() {
  local label="$1"
  local deadline=$(( $(date +%s) + 60 ))
  local last_snapshot=""
  while (( $(date +%s) < deadline )); do
    local min_height=""
    local max_height=""
    local all_valid=1
    local roots_match=1
    local snapshot=""
    local index
    for index in 0 1 2 3; do
      local url="http://127.0.0.1:${PORTS[$index]}"
      local height head_hash committed_root committed_block_count commit_log_hash
      read -r height head_hash committed_root < <(chain_info_triplet "${url}" 2>/dev/null || printf ' - -\n')
      read -r committed_block_count commit_log_hash < <(mahi_commit_snapshot "${url}" 2>/dev/null || printf ' -\n')
      snapshot+=$(printf 'v%s height=%s committed_blocks=%s commit_log=%s committed_root=%s; ' \
        "$((index + 1))" \
        "${height}" \
        "${committed_block_count}" \
        "${commit_log_hash:0:16}" \
        "${committed_root:0:16}")
      if ! [[ "${height}" =~ ^[0-9]+$ ]] \
        || ! [[ "${committed_block_count}" =~ ^[0-9]+$ ]] \
        || [[ -z "${commit_log_hash}" || "${commit_log_hash}" == "-" || "${commit_log_hash}" == "null" ]]; then
        all_valid=0
        break
      fi
      if [[ -z "${min_height}" || "${height}" -lt "${min_height}" ]]; then
        min_height="${height}"
      fi
      if [[ -z "${max_height}" || "${height}" -gt "${max_height}" ]]; then
        max_height="${height}"
      fi
    done
    if [[ "${all_valid}" -eq 1 ]] && [[ "${min_height}" =~ ^[0-9]+$ ]]; then
      local expected_state_root=""
      local common_snapshot=""
      for index in 0 1 2 3; do
        local url="http://127.0.0.1:${PORTS[$index]}"
        local block_hash state_root
        read -r block_hash state_root < <(state_root_at_height_pair "${url}" "${min_height}" 2>/dev/null || printf ' -\n')
        common_snapshot+=$(printf 'v%s common_height=%s block_hash=%s state_root=%s; ' \
          "$((index + 1))" \
          "${min_height}" \
          "${block_hash:0:16}" \
          "${state_root:0:16}")
        if [[ -z "${state_root}" || "${state_root}" == "-" || "${state_root}" == "null" ]]; then
          roots_match=0
          break
        fi
        if [[ -z "${expected_state_root}" ]]; then
          expected_state_root="${state_root}"
        elif [[ "${state_root}" != "${expected_state_root}" ]]; then
          roots_match=0
          break
        fi
      done
      snapshot="${snapshot}common_check: ${common_snapshot}"
      if [[ "${roots_match}" -eq 1 ]]; then
        if [[ "${min_height}" != "${max_height}" ]]; then
          printf -- '----  Mahi latest progress skew accepted: min_height=%s max_height=%s\n' \
            "${min_height}" \
            "${max_height}" >&2
        fi
        printf '%s %s\n' "${min_height}" "${expected_state_root}"
        return 0
      fi
    fi
    last_snapshot="${snapshot}"
    sleep 0.2
  done
  if [[ -n "${last_snapshot}" ]]; then
    printf 'Mahi convergence last observed state:\n%s' "${last_snapshot}" >&2
  fi
  bad "${label} Mahi state never converged"
  return 1
}

first_responsive_url() {
  local index
  for index in 0 1 2 3; do
    local url="http://127.0.0.1:${PORTS[$index]}"
    local resp
    resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":99}' \
      | rpc_post "${url}" 2>/dev/null || true)
    if printf '%s' "${resp}" | jq -e '.result.chain_id' >/dev/null 2>&1; then
      printf '%s\n' "${url}"
      return 0
    fi
  done
  return 1
}

policy_nonce() {
  local url="$1"
  local policy_id="$2"
  local resp
  resp=$(jq -nc --argjson policy_id "${policy_id}" \
    '{jsonrpc:"2.0",method:"kombat_getPolicy",params:{policy_id:$policy_id},id:3}' | rpc_post "${url}")
  printf '%s' "${resp}" | jq -r '.result.next_intent_nonce'
}

start_commit_timeseries_sampler() {
  if [[ "${LOADTEST_COMMIT_TIMESERIES:-1}" == "0" ]]; then
    return
  fi
  local sample_interval_secs="${LOADTEST_COMMIT_SAMPLE_SECS:-1}"
  local sample_urls="${READ_URL}"
  if [[ "${LOADTEST_COMMIT_SAMPLE_ALL_VALIDATORS:-0}" == "1" ]]; then
    sample_urls="http://127.0.0.1:${PORTS[0]},http://127.0.0.1:${PORTS[1]},http://127.0.0.1:${PORTS[2]},http://127.0.0.1:${PORTS[3]}"
  fi
  info "sampling committed nonce deltas every ${sample_interval_secs}s into ${COMMIT_TIMESERIES_JSON}"
  python3 - \
    "${sample_urls}" \
    "${COMMIT_TIMESERIES_JSON}" \
    "${RPC_TIMEOUT_SECS}" \
    "${LOADTEST_WARMUP_SECS}" \
    "${DURATION_SECS}" \
    "${sample_interval_secs}" <<'PY' &
import json
import statistics
import subprocess
import sys
import time

urls_arg, out_path, timeout_secs, warmup_secs, duration_secs, interval_secs = sys.argv[1:]
urls = [url for url in urls_arg.split(",") if url]
primary_url = urls[0]
timeout_secs = str(timeout_secs)
warmup_secs = float(warmup_secs)
duration_secs = float(duration_secs)
interval_secs = max(0.1, float(interval_secs))
sample_end_secs = warmup_secs + duration_secs

def fetch_metrics(url: str) -> dict:
    payload = '{"jsonrpc":"2.0","method":"kombat_getConsensusMetrics","params":{},"id":1}'
    raw = subprocess.check_output(
        [
            "curl",
            "-sS",
            "--max-time",
            timeout_secs,
            "-H",
            "content-type: application/json",
            "--data",
            payload,
            url,
        ],
        text=True,
        stderr=subprocess.DEVNULL,
    )
    response = json.loads(raw)
    result = response["result"]
    mahi = result.get("mahi") or {}
    queues = result.get("submit_queue_stats") or {}
    resting_batch = result.get("phase6_resting_batch") or {}
    def summary_int(name: str, field: str) -> int:
        return int((mahi.get(name) or {}).get(field) or 0)
    return {
        "total_committed_intents": int(mahi.get("executed_intent_count", 0)),
        "mahi_next_round": int(mahi.get("next_round", 0)),
        "mahi_max_round": int(mahi.get("max_round", 0)),
        "mahi_last_produced_round": int(mahi.get("last_produced_round") or 0),
        "mahi_producer_attempts": int(mahi.get("producer_attempts", 0)),
        "mahi_producer_skip_ancestor_quorum": int(mahi.get("producer_skip_ancestor_quorum", 0)),
        "mahi_producer_skip_peer_reachability": int(mahi.get("producer_skip_peer_reachability", 0)),
        "mahi_producer_skip_snapshot_unavailable": int(mahi.get("producer_skip_snapshot_unavailable", 0)),
        "mahi_producer_skip_round_race": int(mahi.get("producer_skip_round_race", 0)),
        "mahi_producer_ancestor_wait_round": int(mahi.get("producer_ancestor_wait_round") or 0),
        "mahi_producer_ancestor_wait_available": int(mahi.get("producer_ancestor_wait_available") or 0),
        "mahi_producer_ancestor_wait_quorum": int(mahi.get("producer_ancestor_wait_quorum") or 0),
        "mahi_producer_ancestor_wait_age_ms": int(mahi.get("producer_ancestor_wait_age_ms") or 0),
        "mahi_producer_ancestor_wait_first_arrival_age_ms": int(mahi.get("producer_ancestor_wait_first_arrival_age_ms") or 0),
        "mahi_producer_ancestor_wait_latest_arrival_age_ms": int(mahi.get("producer_ancestor_wait_latest_arrival_age_ms") or 0),
        "mahi_producer_initial_lock_wait_p90_ms": summary_int("producer_initial_lock_wait_latency_ms", "p90_ms"),
        "mahi_producer_initial_lock_wait_max_ms": summary_int("producer_initial_lock_wait_latency_ms", "max_ms"),
        "mahi_producer_initial_lock_hold_p90_ms": summary_int("producer_initial_lock_hold_latency_ms", "p90_ms"),
        "mahi_producer_initial_lock_hold_max_ms": summary_int("producer_initial_lock_hold_latency_ms", "max_ms"),
        "mahi_producer_snapshot_p90_ms": summary_int("producer_snapshot_latency_ms", "p90_ms"),
        "mahi_producer_snapshot_max_ms": summary_int("producer_snapshot_latency_ms", "max_ms"),
        "mahi_producer_pre_admitted_drain_p90_ms": summary_int("producer_pre_admitted_drain_latency_ms", "p90_ms"),
        "mahi_producer_pre_admitted_drain_max_ms": summary_int("producer_pre_admitted_drain_latency_ms", "max_ms"),
        "mahi_producer_final_lock_wait_p90_ms": summary_int("producer_final_lock_wait_latency_ms", "p90_ms"),
        "mahi_producer_final_lock_wait_max_ms": summary_int("producer_final_lock_wait_latency_ms", "max_ms"),
        "mahi_producer_final_lock_hold_p90_ms": summary_int("producer_final_lock_hold_latency_ms", "p90_ms"),
        "mahi_producer_final_lock_hold_max_ms": summary_int("producer_final_lock_hold_latency_ms", "max_ms"),
        "mahi_producer_total_p90_ms": summary_int("producer_total_latency_ms", "p90_ms"),
        "mahi_producer_total_max_ms": summary_int("producer_total_latency_ms", "max_ms"),
        "mahi_produced_blocks": int(mahi.get("produced_blocks", 0)),
        "mahi_produced_empty_blocks": int(mahi.get("produced_empty_blocks", 0)),
        "mahi_produced_intent_count": int(mahi.get("produced_intent_count", 0)),
        "mahi_received_blocks": int(mahi.get("received_blocks", 0)),
        "mahi_received_intent_count": int(mahi.get("received_intent_count", 0)),
        "mahi_committed_block_count": int(mahi.get("committed_block_count", 0)),
        "mahi_execution_pending_count": int(mahi.get("execution_pending_count", 0)),
        "mahi_executed_block_count": int(mahi.get("executed_block_count", 0)),
        "mahi_verified_block_count": int(mahi.get("verified_block_count", 0)),
        "mahi_sigverify_pool_depth": int(mahi.get("sigverify_pool_depth", 0)),
        "mahi_sigverify_per_intent_verified_total": int(mahi.get("sigverify_per_intent_verified_total", 0)),
        "mahi_inbound_block_decode_p90_ms": summary_int("inbound_block_decode_latency_ms", "p90_ms"),
        "mahi_inbound_block_decode_max_ms": summary_int("inbound_block_decode_latency_ms", "max_ms"),
        "mahi_inbound_block_accept_p90_ms": summary_int("inbound_block_accept_latency_ms", "p90_ms"),
        "mahi_inbound_block_accept_max_ms": summary_int("inbound_block_accept_latency_ms", "max_ms"),
        "mahi_inbound_block_sigverify_enqueue_p90_ms": summary_int("inbound_block_sigverify_enqueue_latency_ms", "p90_ms"),
        "mahi_inbound_block_sigverify_enqueue_max_ms": summary_int("inbound_block_sigverify_enqueue_latency_ms", "max_ms"),
        "mahi_inbound_block_total_p90_ms": summary_int("inbound_block_total_latency_ms", "p90_ms"),
        "mahi_inbound_block_total_max_ms": summary_int("inbound_block_total_latency_ms", "max_ms"),
        "mahi_commit_executor_wake_count": int(mahi.get("commit_executor_wake_count", 0)),
        "mahi_commit_executor_empty_wake_count": int(mahi.get("commit_executor_empty_wake_count", 0)),
        "mahi_commit_execution_batches": int(mahi.get("commit_execution_batches", 0)),
        "mahi_commit_execution_blocks": int(mahi.get("commit_execution_blocks", 0)),
        "mahi_commit_execution_intents": int(mahi.get("commit_execution_intents", 0)),
        "mahi_commit_execution_batch_p90_ms": summary_int("commit_execution_batch_latency_ms", "p90_ms"),
        "mahi_commit_execution_batch_max_ms": summary_int("commit_execution_batch_latency_ms", "max_ms"),
        "mahi_phase6_schedule_profile_enabled": bool(mahi.get("phase6_schedule_profile_enabled", False)),
        "mahi_phase6_schedule_profile_sample_every": int(mahi.get("phase6_schedule_profile_sample_every", 0)),
        "mahi_phase6_schedule_profile_max_items": int(mahi.get("phase6_schedule_profile_max_items", 0)),
        "mahi_phase6_schedule_profiled_batches": int(mahi.get("phase6_schedule_profiled_batches", 0)),
        "mahi_phase6_schedule_truncated_batches": int(mahi.get("phase6_schedule_truncated_batches", 0)),
        "mahi_phase6_schedule_total_item_count_p50": summary_int("phase6_schedule_total_item_count", "p50"),
        "mahi_phase6_schedule_total_item_count_p90": summary_int("phase6_schedule_total_item_count", "p90"),
        "mahi_phase6_schedule_total_item_count_max": summary_int("phase6_schedule_total_item_count", "max"),
        "mahi_phase6_schedule_item_count_p50": summary_int("phase6_schedule_item_count", "p50"),
        "mahi_phase6_schedule_item_count_p90": summary_int("phase6_schedule_item_count", "p90"),
        "mahi_phase6_schedule_item_count_max": summary_int("phase6_schedule_item_count", "max"),
        "mahi_phase6_schedule_layer_count_p50": summary_int("phase6_schedule_layer_count", "p50"),
        "mahi_phase6_schedule_layer_count_p90": summary_int("phase6_schedule_layer_count", "p90"),
        "mahi_phase6_schedule_layer_count_max": summary_int("phase6_schedule_layer_count", "max"),
        "mahi_phase6_schedule_max_layer_width_p50": summary_int("phase6_schedule_max_layer_width", "p50"),
        "mahi_phase6_schedule_max_layer_width_p90": summary_int("phase6_schedule_max_layer_width", "p90"),
        "mahi_phase6_schedule_max_layer_width_max": summary_int("phase6_schedule_max_layer_width", "max"),
        "mahi_phase6_schedule_conflict_pair_count_p50": summary_int("phase6_schedule_conflict_pair_count", "p50"),
        "mahi_phase6_schedule_conflict_pair_count_p90": summary_int("phase6_schedule_conflict_pair_count", "p90"),
        "mahi_phase6_schedule_conflict_pair_count_max": summary_int("phase6_schedule_conflict_pair_count", "max"),
        "mahi_phase6_schedule_parallelism_milli_p50": summary_int("phase6_schedule_parallelism_milli", "p50"),
        "mahi_phase6_schedule_parallelism_milli_p90": summary_int("phase6_schedule_parallelism_milli", "p90"),
        "mahi_phase6_schedule_parallelism_milli_max": summary_int("phase6_schedule_parallelism_milli", "max"),
        "phase6_resting_batch_attempts": int(resting_batch.get("attempts", 0)),
        "phase6_resting_batch_accepted": int(resting_batch.get("accepted", 0)),
        "phase6_resting_batch_items": int(resting_batch.get("items", 0)),
        "phase6_resting_batch_unsupported": int(resting_batch.get("unsupported", 0)),
        "phase6_resting_batch_rejected": int(resting_batch.get("rejected", 0)),
        "phase6_resting_batch_gather_ns": int(resting_batch.get("gather_ns", 0)),
        "phase6_resting_batch_precheck_ns": int(resting_batch.get("precheck_ns", 0)),
        "phase6_resting_batch_admission_ns": int(resting_batch.get("admission_ns", 0)),
        "phase6_resting_batch_policy_ns": int(resting_batch.get("policy_ns", 0)),
        "phase6_resting_batch_matching_ns": int(resting_batch.get("matching_ns", 0)),
        "phase6_resting_batch_event_ns": int(resting_batch.get("event_ns", 0)),
        "mahi_pre_admitted_pool_items": int(queues.get("mahi_pre_admitted_pool_items", 0)),
        "mahi_pre_admitted_pool_drained_total": int(queues.get("mahi_pre_admitted_pool_drained_total", 0)),
        "pending_mahi_block_broadcasts": int(queues.get("pending_mahi_block_broadcasts", 0)),
        "pending_relay_intents": int(queues.get("pending_relay_intents", 0)),
    }

def summarize_validators(validator_metrics: list[dict]) -> dict:
    if not validator_metrics:
        return {}
    keys = [
        "total_committed_intents",
        "mahi_next_round",
        "mahi_max_round",
        "mahi_last_produced_round",
        "mahi_producer_attempts",
        "mahi_producer_skip_ancestor_quorum",
        "mahi_producer_skip_peer_reachability",
        "mahi_producer_skip_snapshot_unavailable",
        "mahi_producer_skip_round_race",
        "mahi_producer_ancestor_wait_round",
        "mahi_producer_ancestor_wait_available",
        "mahi_producer_ancestor_wait_quorum",
        "mahi_producer_ancestor_wait_age_ms",
        "mahi_producer_ancestor_wait_first_arrival_age_ms",
        "mahi_producer_ancestor_wait_latest_arrival_age_ms",
        "mahi_producer_initial_lock_wait_p90_ms",
        "mahi_producer_initial_lock_wait_max_ms",
        "mahi_producer_initial_lock_hold_p90_ms",
        "mahi_producer_initial_lock_hold_max_ms",
        "mahi_producer_snapshot_p90_ms",
        "mahi_producer_snapshot_max_ms",
        "mahi_producer_pre_admitted_drain_p90_ms",
        "mahi_producer_pre_admitted_drain_max_ms",
        "mahi_producer_final_lock_wait_p90_ms",
        "mahi_producer_final_lock_wait_max_ms",
        "mahi_producer_final_lock_hold_p90_ms",
        "mahi_producer_final_lock_hold_max_ms",
        "mahi_producer_total_p90_ms",
        "mahi_producer_total_max_ms",
        "mahi_produced_blocks",
        "mahi_produced_empty_blocks",
        "mahi_produced_intent_count",
        "mahi_received_blocks",
        "mahi_received_intent_count",
        "mahi_committed_block_count",
        "mahi_execution_pending_count",
        "mahi_executed_block_count",
        "mahi_verified_block_count",
        "mahi_sigverify_pool_depth",
        "mahi_inbound_block_decode_p90_ms",
        "mahi_inbound_block_decode_max_ms",
        "mahi_inbound_block_accept_p90_ms",
        "mahi_inbound_block_accept_max_ms",
        "mahi_inbound_block_sigverify_enqueue_p90_ms",
        "mahi_inbound_block_sigverify_enqueue_max_ms",
        "mahi_inbound_block_total_p90_ms",
        "mahi_inbound_block_total_max_ms",
        "mahi_commit_executor_wake_count",
        "mahi_commit_executor_empty_wake_count",
        "mahi_commit_execution_batches",
        "mahi_commit_execution_blocks",
        "mahi_commit_execution_intents",
        "mahi_commit_execution_batch_p90_ms",
        "mahi_commit_execution_batch_max_ms",
        "mahi_phase6_schedule_profiled_batches",
        "mahi_phase6_schedule_truncated_batches",
        "mahi_phase6_schedule_total_item_count_p50",
        "mahi_phase6_schedule_total_item_count_p90",
        "mahi_phase6_schedule_total_item_count_max",
        "mahi_phase6_schedule_item_count_p50",
        "mahi_phase6_schedule_item_count_p90",
        "mahi_phase6_schedule_item_count_max",
        "mahi_phase6_schedule_layer_count_p50",
        "mahi_phase6_schedule_layer_count_p90",
        "mahi_phase6_schedule_layer_count_max",
        "mahi_phase6_schedule_max_layer_width_p50",
        "mahi_phase6_schedule_max_layer_width_p90",
        "mahi_phase6_schedule_max_layer_width_max",
        "mahi_phase6_schedule_conflict_pair_count_p50",
        "mahi_phase6_schedule_conflict_pair_count_p90",
        "mahi_phase6_schedule_conflict_pair_count_max",
        "mahi_phase6_schedule_parallelism_milli_p50",
        "mahi_phase6_schedule_parallelism_milli_p90",
        "mahi_phase6_schedule_parallelism_milli_max",
        "phase6_resting_batch_attempts",
        "phase6_resting_batch_accepted",
        "phase6_resting_batch_items",
        "phase6_resting_batch_unsupported",
        "phase6_resting_batch_rejected",
        "phase6_resting_batch_gather_ns",
        "phase6_resting_batch_precheck_ns",
        "phase6_resting_batch_admission_ns",
        "phase6_resting_batch_policy_ns",
        "phase6_resting_batch_matching_ns",
        "phase6_resting_batch_event_ns",
        "mahi_pre_admitted_pool_items",
        "mahi_pre_admitted_pool_drained_total",
        "pending_mahi_block_broadcasts",
    ]
    summary = {}
    for key in keys:
        values = [int(metrics.get(key, 0)) for metrics in validator_metrics]
        summary[f"{key}_min"] = min(values)
        summary[f"{key}_max"] = max(values)
        summary[f"{key}_sum"] = sum(values)
    return summary

samples = []
started_at = time.monotonic()
previous_total = None
previous_elapsed = None
sample_index = 0

while True:
    target_time = started_at + (sample_index * interval_secs)
    sleep_secs = target_time - time.monotonic()
    if sleep_secs > 0:
        time.sleep(sleep_secs)

    elapsed = time.monotonic() - started_at
    phase = "warmup" if elapsed < warmup_secs else "measured"
    sample = {
        "elapsed_secs": round(elapsed, 3),
        "phase": phase,
    }
    try:
        validator_metrics = []
        for index, sample_url in enumerate(urls, start=1):
            metrics = fetch_metrics(sample_url)
            metrics["validator_index"] = index
            metrics["url"] = sample_url
            validator_metrics.append(metrics)
        metrics = validator_metrics[0]
        sample.update(metrics)
        if len(validator_metrics) > 1:
            sample["validators"] = validator_metrics
            sample.update(summarize_validators(validator_metrics))
        total = metrics["total_committed_intents"]
        sample["total_committed_intents"] = total
        if previous_total is not None and previous_elapsed is not None:
            delta_secs = max(0.000001, elapsed - previous_elapsed)
            delta = total - previous_total
            sample["delta_committed_intents"] = delta
            sample["delta_secs"] = round(delta_secs, 3)
            sample["instant_tps"] = delta / delta_secs
        previous_total = total
        previous_elapsed = elapsed
    except Exception as error:
        sample["error"] = str(error)

    samples.append(sample)
    sample_index += 1
    if elapsed >= sample_end_secs:
        break

measured_tps = [
    float(sample["instant_tps"])
    for sample in samples
    if sample.get("phase") == "measured" and "instant_tps" in sample
]
summary = {
    "sample_url": primary_url,
    "sample_urls": urls,
    "source": "kombat_getConsensusMetrics.mahi.executed_intent_count",
    "sample_interval_secs": interval_secs,
    "warmup_secs": warmup_secs,
    "duration_secs": duration_secs,
    "samples": samples,
    "measured_sample_count": len(measured_tps),
    "measured_avg_instant_tps": statistics.mean(measured_tps) if measured_tps else None,
    "measured_peak_instant_tps": max(measured_tps) if measured_tps else None,
    "measured_min_instant_tps": min(measured_tps) if measured_tps else None,
}

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)

if measured_tps:
    print(
        "PASS  committed timeseries: samples={} measured_avg_tps={:.1f} measured_peak_tps={:.1f}".format(
            len(samples),
            summary["measured_avg_instant_tps"],
            summary["measured_peak_instant_tps"],
        )
    )
else:
    print("PASS  committed timeseries: samples={} measured_avg_tps=n/a".format(len(samples)))
PY
  COMMIT_SAMPLER_PID=$!
}

wait_commit_timeseries_sampler() {
  if [[ -z "${COMMIT_SAMPLER_PID}" ]]; then
    return
  fi
  if ! wait "${COMMIT_SAMPLER_PID}"; then
    info "committed timeseries sampler failed; continuing because final committed summary remains authoritative"
  fi
  COMMIT_SAMPLER_PID=""
}

stop_validator() {
  local index="$1"
  local pid="${PIDS[$index]}"
  local rpc_port="${PORTS[$index]}"
  local network_port="${NETWORK_PORTS[$index]}"
  if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  fi
  local remaining_pids
  remaining_pids="$(
    {
      lsof -ti tcp:"${rpc_port}" 2>/dev/null || true
      lsof -ti tcp:"${network_port}" 2>/dev/null || true
    } | sort -u
  )"
  if [[ -n "${remaining_pids}" ]]; then
    kill ${remaining_pids} 2>/dev/null || true
    sleep 0.2
    remaining_pids="$(
      {
        lsof -ti tcp:"${rpc_port}" 2>/dev/null || true
        lsof -ti tcp:"${network_port}" 2>/dev/null || true
      } | sort -u
    )"
    if [[ -n "${remaining_pids}" ]]; then
      kill -9 ${remaining_pids} 2>/dev/null || true
    fi
  fi
  PIDS[$index]=""
}

stop_ingress() {
  local rpc_port="${INGRESS_PORT}"
  if [[ -n "${INGRESS_PID}" ]] && kill -0 "${INGRESS_PID}" 2>/dev/null; then
    kill "${INGRESS_PID}" 2>/dev/null || true
    wait "${INGRESS_PID}" 2>/dev/null || true
  fi
  local remaining_pids
  remaining_pids="$(lsof -ti tcp:"${rpc_port}" 2>/dev/null || true)"
  if [[ -n "${remaining_pids}" ]]; then
    kill ${remaining_pids} 2>/dev/null || true
    sleep 0.2
    remaining_pids="$(lsof -ti tcp:"${rpc_port}" 2>/dev/null || true)"
    if [[ -n "${remaining_pids}" ]]; then
      kill -9 ${remaining_pids} 2>/dev/null || true
    fi
  fi
  INGRESS_PID=""
}

start_questdb() {
  docker rm -f "${QUESTDB_CONTAINER}" >/dev/null 2>&1 || true
  info "starting QuestDB ${QUESTDB_IMAGE} on http://127.0.0.1:${QUESTDB_HTTP_PORT}"
  docker run --rm \
    --name "${QUESTDB_CONTAINER}" \
    -p "127.0.0.1:${QUESTDB_HTTP_PORT}:9000" \
    -p "127.0.0.1:${QUESTDB_PG_PORT}:8812" \
    -p "127.0.0.1:${QUESTDB_ILP_TCP_PORT}:9009" \
    "${QUESTDB_IMAGE}" >"${QUESTDB_LOG_PATH}" 2>&1 &
  QUESTDB_PID=$!

  local deadline=$(( $(date +%s) + 45 ))
  while (( $(date +%s) < deadline )); do
    if ! kill -0 "${QUESTDB_PID}" 2>/dev/null; then
      bad "QuestDB exited before becoming ready"
      return 1
    fi
    if curl --max-time 2 -fsS "http://127.0.0.1:${QUESTDB_HTTP_PORT}/ping" >/dev/null 2>&1; then
      ok "QuestDB reachable"
      return 0
    fi
    sleep 0.5
  done
  bad "QuestDB never became ready"
  return 1
}

start_market_data() {
  local rpc_url="$1"
  if [[ -n "${MARKET_DATA_PID}" ]] && kill -0 "${MARKET_DATA_PID}" 2>/dev/null; then
    kill "${MARKET_DATA_PID}" 2>/dev/null || true
    wait "${MARKET_DATA_PID}" 2>/dev/null || true
  fi
  info "starting kombat_market_data on http://127.0.0.1:${MARKET_DATA_API_PORT}"
  env \
    KOMBAT_MARKET_DATA_RPC_URL="${rpc_url}" \
    KOMBAT_MARKET_DATA_QUESTDB_URL="http://127.0.0.1:${QUESTDB_HTTP_PORT}" \
    KOMBAT_MARKET_DATA_BIND="127.0.0.1:${MARKET_DATA_API_PORT}" \
    KOMBAT_MARKET_DATA_SOURCE="local-${BASE_PORT}" \
    "${MARKET_DATA_BINARY_PATH}" >"${MARKET_DATA_LOG_PATH}" 2>&1 &
  MARKET_DATA_PID=$!

  local deadline=$(( $(date +%s) + 20 ))
  while (( $(date +%s) < deadline )); do
    if ! kill -0 "${MARKET_DATA_PID}" 2>/dev/null; then
      bad "kombat_market_data exited before becoming ready"
      return 1
    fi
    if curl --max-time 2 -fsS "http://127.0.0.1:${MARKET_DATA_API_PORT}/healthz" >/dev/null 2>&1; then
      printf 'http://127.0.0.1:%s\n' "${MARKET_DATA_API_PORT}" > "${MARKET_DATA_API_URL_FILE}"
      ok "kombat_market_data reachable"
      return 0
    fi
    sleep 0.2
  done
  bad "kombat_market_data never became ready"
  return 1
}

create_validator_set() {
  : > "${VALIDATOR_SET_PATH}"
  cat >> "${VALIDATOR_SET_PATH}" <<EOF
epoch = 1

EOF
  local i
  for i in "${!PORTS[@]}"; do
    local validator_rpc_port="${NETWORK_PORTS[$i]}"
    if [[ "${LOCAL_CLUSTER_PEER_TRANSPORT}" == "1" ]]; then
      validator_rpc_port="${PORTS[$i]}"
    fi
    cat >> "${VALIDATOR_SET_PATH}" <<EOF
[[validators]]
validator_id = "0x${VALIDATOR_IDS[$i]}"
consensus_pubkey = "0x${CONSENSUS_PUBKEYS[$i]}"
network_pubkey = "0x${NETWORK_PUBKEYS[$i]}"
rpc_endpoints = ["http://127.0.0.1:${validator_rpc_port}"]
region = "${REGIONS[$i]}"
operator_name = "${OPERATORS[$i]}"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

EOF
  done
}

create_hosts_file() {
  {
    printf 'name\tssh_host\tpublic_rpc_url\n'
    local i
    for i in "${!PORTS[@]}"; do
      printf '%s\t%s\t%s\n' \
        "${VALIDATOR_NAMES[$i]}" \
        "local" \
        "http://127.0.0.1:${PORTS[$i]}"
    done
    if [[ "${LOCAL_CLUSTER_ENABLE_INGRESS}" == "1" ]]; then
      printf '%s\t%s\t%s\n' \
        "ingress" \
        "local" \
        "http://127.0.0.1:${INGRESS_PORT}"
    fi
  } > "${HOSTS_FILE}"
}

start_validator() {
  local index="$1"
  local mode="${2:-auto}"
  local bootstrap_port="${NETWORK_PORTS[0]}"
  if [[ "${LOCAL_CLUSTER_PEER_TRANSPORT}" == "1" ]]; then
    bootstrap_port="${PORTS[0]}"
  fi
  local bootstrap_url="${3:-http://127.0.0.1:${bootstrap_port}}"
  local rpc_port="${PORTS[$index]}"
  local network_port="${NETWORK_PORTS[$index]}"
  local validator_id_hex="${VALIDATOR_IDS[$index]}"
  local consensus_seed_hex="${CONSENSUS_SEEDS[$index]}"
  local log_path="${LOG_PATHS[$index]}"
  local public_rpc_url="http://127.0.0.1:${rpc_port}"
  local network_rpc_bind="127.0.0.1:${network_port}"

  stop_validator "${index}"
  if [[ "${mode}" != "bootstrap" && "${index}" == "0" ]]; then
    info "starting validator $((index + 1)) fresh on port ${rpc_port}"
    KOMBAT_RPC_BIND="127.0.0.1:${rpc_port}" \
    KOMBAT_NETWORK_RPC_BIND="${network_rpc_bind}" \
    KOMBAT_CHAIN_ID="${CHAIN_ID}" \
    KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
    KOMBAT_VALIDATOR_ID_HEX="0x${validator_id_hex}" \
    KOMBAT_CONSENSUS_SIGNING_KEY_HEX="0x${consensus_seed_hex}" \
    KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
    KOMBAT_PUBLIC_RPC_URL="${public_rpc_url}" \
    KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100 \
    KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS="${AUTO_INTERVAL_MS}" \
    KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS="${AUTO_DRIVE_INTERVAL_MS}" \
    KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS="${AUTO_MAX_ITEMS}" \
    KOMBAT_PEER_TRANSPORT_ENABLED="${LOCAL_CLUSTER_PEER_TRANSPORT}" \
    KOMBAT_PEER_TRANSPORT_KEEPALIVE_INTERVAL_MS="${PEER_KEEPALIVE_MS}" \
    KOMBAT_NETWORK_SUBMIT_FORWARDING_ENABLED="${LOCAL_CLUSTER_SUBMIT_FORWARDING}" \
    "${BINARY_PATH}" >"${log_path}" 2>&1 &
  else
    info "starting validator $((index + 1)) from peer bootstrap on port ${rpc_port}"
    KOMBAT_RPC_BIND="127.0.0.1:${rpc_port}" \
    KOMBAT_NETWORK_RPC_BIND="${network_rpc_bind}" \
    KOMBAT_CHAIN_ID="${CHAIN_ID}" \
    KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
    KOMBAT_VALIDATOR_ID_HEX="0x${validator_id_hex}" \
    KOMBAT_CONSENSUS_SIGNING_KEY_HEX="0x${consensus_seed_hex}" \
    KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
    KOMBAT_PUBLIC_RPC_URL="${public_rpc_url}" \
    KOMBAT_BOOTSTRAP_RPC_URL="${bootstrap_url}" \
    KOMBAT_BOOTSTRAP_BEARER_TOKEN="${OPERATOR_TOKEN}" \
    KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS="${AUTO_INTERVAL_MS}" \
    KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS="${AUTO_DRIVE_INTERVAL_MS}" \
    KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS="${AUTO_MAX_ITEMS}" \
    KOMBAT_PEER_TRANSPORT_ENABLED="${LOCAL_CLUSTER_PEER_TRANSPORT}" \
    KOMBAT_PEER_TRANSPORT_KEEPALIVE_INTERVAL_MS="${PEER_KEEPALIVE_MS}" \
    KOMBAT_NETWORK_SUBMIT_FORWARDING_ENABLED="${LOCAL_CLUSTER_SUBMIT_FORWARDING}" \
    "${BINARY_PATH}" >"${log_path}" 2>&1 &
  fi
  PIDS[$index]=$!
}

start_ingress() {
  stop_ingress
  info "starting observer ingress on port ${INGRESS_PORT}"
  env \
    KOMBAT_RUNTIME_ROLE=ingress \
    KOMBAT_RPC_BIND="127.0.0.1:${INGRESS_PORT}" \
    KOMBAT_CHAIN_ID="${CHAIN_ID}" \
    KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
    KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
    KOMBAT_PUBLIC_RPC_URL="http://127.0.0.1:${INGRESS_PORT}" \
    KOMBAT_NETWORK_SUBMIT_FORWARDING_ENABLED=1 \
    "${BINARY_PATH}" >"${INGRESS_LOG_PATH}" 2>&1 &
  INGRESS_PID=$!
}

if [[ "${SKIP_BUILD}" != "1" ]]; then
  if [[ "${LOCAL_CLUSTER_ENABLE_MARKET_DATA}" == "1" ]]; then
    info "building kombat_chain, kombat_sign, kombat_loadgen, and kombat_market_data (${PROFILE})"
  else
    info "building kombat_chain, kombat_sign, and kombat_loadgen (${PROFILE})"
  fi
  build_cmd=(cargo build)
  if [[ "${#BUILD_ARGS[@]}" -gt 0 ]]; then
    build_cmd+=("${BUILD_ARGS[@]}")
  fi
  build_cmd+=(
    -p kombat-chain-rpc --bin kombat_chain --bin kombat_sign
    -p kombat-chain-loadgen
  )
  if [[ "${LOCAL_CLUSTER_ENABLE_MARKET_DATA}" == "1" ]]; then
    build_cmd+=(-p kombat-chain-market-data --bin kombat_market_data)
  fi
  build_cmd+=(--manifest-path "${CHAIN_DIR}/Cargo.toml")
  "${build_cmd[@]}" >/dev/null
fi

for path in "${BINARY_PATH}" "${SIGN_BINARY_PATH}" "${LOADGEN_BINARY_PATH}" "${PREPARE_SCRIPT}" "${STATUS_SCRIPT}"; do
  if [[ ! -f "${path}" ]]; then
    echo "loadtest_local_cluster.sh: missing required file: ${path}" >&2
    exit 1
  fi
done
if [[ "${LOCAL_CLUSTER_ENABLE_MARKET_DATA}" == "1" && ! -f "${MARKET_DATA_BINARY_PATH}" ]]; then
  echo "loadtest_local_cluster.sh: missing required file: ${MARKET_DATA_BINARY_PATH}" >&2
  exit 1
fi

create_validator_set
create_hosts_file

start_validator 0
wait_for_ready "http://127.0.0.1:${PORTS[0]}" "validator 1" "${PIDS[0]}"

info "preseeding ${WRITER_COUNT} load-test writers on validator 1 before follower bootstrap"
SEED_VERIFY_REPLICAS=0 \
SEED_NETWORK_BYPASS_FANOUT=1 \
KOMBAT_SIGN_PATH="${SIGN_BINARY_PATH}" \
  bash "${PREPARE_SCRIPT}" "${OUTPUT_DIR}" "${WRITER_COUNT}" "${WRITERS_JSON}"

for index in 1 2 3; do
  start_validator "${index}"
  wait_for_ready "http://127.0.0.1:${PORTS[$index]}" "validator $((index + 1))" "${PIDS[$index]}"
done

if [[ "${LOCAL_CLUSTER_ENABLE_INGRESS}" == "1" ]]; then
  start_ingress
  wait_for_ready "http://127.0.0.1:${INGRESS_PORT}" "ingress" "${INGRESS_PID}"
fi

read -r baseline_height baseline_hash < <(wait_for_cluster_convergence "initial startup")
if [[ "${KOMBAT_CONSENSUS:-mahi}" == "mahi" ]]; then
  ok "startup converged at height=${baseline_height} state_root=${baseline_hash}"
else
  ok "startup converged at height=${baseline_height} hash=${baseline_hash}"
fi
bash "${STATUS_SCRIPT}" "${OUTPUT_DIR}" | tee "${STATUS_BEFORE_LOG}"

declare -a TARGET_INDICES=()
if [[ "${LOADTEST_TARGET_VALIDATOR}" == "all" ]]; then
  TARGET_INDICES=(0 1 2 3)
else
  IFS=',' read -r -a TARGET_NAMES <<< "${LOADTEST_TARGET_VALIDATOR}"
  for target_name in "${TARGET_NAMES[@]}"; do
    case "${target_name}" in
      validator-1) TARGET_INDICES+=(0) ;;
      validator-2) TARGET_INDICES+=(1) ;;
      validator-3) TARGET_INDICES+=(2) ;;
      validator-4) TARGET_INDICES+=(3) ;;
      ingress)
        if [[ "${LOCAL_CLUSTER_ENABLE_INGRESS}" != "1" ]]; then
          echo "loadtest_local_cluster.sh: ingress target requires LOCAL_CLUSTER_ENABLE_INGRESS=1" >&2
          exit 1
        fi
        TARGET_INDICES+=(-1)
        ;;
      *)
        echo "loadtest_local_cluster.sh: unknown LOADTEST_TARGET_VALIDATOR entry=${target_name}" >&2
        exit 1
        ;;
    esac
  done
fi
if [[ "${#TARGET_INDICES[@]}" -eq 0 ]]; then
  echo "loadtest_local_cluster.sh: no target validators selected" >&2
  exit 1
fi
if [[ "${TARGET_INDICES[0]}" -eq -1 ]]; then
  TARGET_URL="http://127.0.0.1:${INGRESS_PORT}"
else
  TARGET_URL="http://127.0.0.1:${PORTS[${TARGET_INDICES[0]}]}"
fi
READ_URL="http://127.0.0.1:${PORTS[0]}"
if [[ "${LOCAL_CLUSTER_ENABLE_MARKET_DATA}" == "1" ]]; then
  start_questdb
  start_market_data "${READ_URL}"
  info "market-data API URL: http://127.0.0.1:${MARKET_DATA_API_PORT}"
fi
LOADGEN_STATE_URL_EFFECTIVE="${LOADGEN_STATE_URL:-}"
if [[ -z "${LOADGEN_STATE_URL_EFFECTIVE}" ]]; then
  for index in "${TARGET_INDICES[@]}"; do
    if [[ "${index}" -eq -1 ]]; then
      LOADGEN_STATE_URL_EFFECTIVE="${READ_URL}"
      break
    fi
  done
fi

info "capturing starting policy nonces from ${READ_URL}"
python3 - "${WRITERS_JSON}" "${READ_URL}" "${START_NONCES_JSON}" <<'PY'
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

start_commit_timeseries_sampler

if [[ "${#TARGET_INDICES[@]}" -gt 1 ]]; then
  SHARD_DIR="${OUTPUT_DIR}/loadgen-shards"
  mkdir -p "${SHARD_DIR}"
  python3 - "${WRITERS_JSON}" "${SHARD_DIR}" "${#TARGET_INDICES[@]}" <<'PY'
import json
import os
import sys

writers_path, out_dir, shard_count = sys.argv[1:4]
shard_count = int(shard_count)
with open(writers_path, "r", encoding="utf-8") as fh:
    writers = json.load(fh)

shards = [[] for _ in range(shard_count)]
for idx, writer in enumerate(writers):
    shards[idx % shard_count].append(writer)

for idx, shard in enumerate(shards):
    path = os.path.join(out_dir, f"writers-{idx + 1}.json")
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(shard, fh, indent=2, sort_keys=True)
PY

  : > "${LOADTEST_LOG}"
  declare -a LOADGEN_PIDS=()
  declare -a LOADGEN_LOGS=()
  TARGET_COUNT="${#TARGET_INDICES[@]}"
  BASE_SHARD_RPS=$((TARGET_RPS / TARGET_COUNT))
  SHARD_RPS_REMAINDER=$((TARGET_RPS % TARGET_COUNT))
  for shard_index in "${!TARGET_INDICES[@]}"; do
    index="${TARGET_INDICES[$shard_index]}"
    SHARD_PATH="${SHARD_DIR}/writers-$((shard_index + 1)).json"
    SHARD_WRITERS=$(jq 'length' "${SHARD_PATH}")
    if [[ "${SHARD_WRITERS}" -eq 0 ]]; then
      continue
    fi
    SHARD_RPS="${BASE_SHARD_RPS}"
    if [[ "${shard_index}" -lt "${SHARD_RPS_REMAINDER}" ]]; then
      SHARD_RPS=$((SHARD_RPS + 1))
    fi
    if [[ "${index}" -eq -1 ]]; then
      SHARD_URL="http://127.0.0.1:${INGRESS_PORT}"
      SHARD_LABEL="ingress"
    else
      SHARD_URL="http://127.0.0.1:${PORTS[$index]}"
      SHARD_LABEL="${VALIDATOR_NAMES[$index]}"
    fi
    SHARD_LOG="${OUTPUT_DIR}/loadtest-${SHARD_LABEL}-$((shard_index + 1)).txt"
    LOADGEN_LOGS+=("${SHARD_LOG}")
    info "running localhost load against ${SHARD_LABEL} (${SHARD_URL}) with ${SHARD_WRITERS} writers @ ${SHARD_RPS} RPS" | tee -a "${LOADTEST_LOG}"
    LOADGEN_ARGS=(
      --url "${SHARD_URL}"
      --workload place-order
      --writers-json "${SHARD_PATH}"
      --concurrency "${SHARD_WRITERS}"
      --pipeline-depth "${LOADTEST_PIPELINE_DEPTH}"
      --batch-size "${LOADTEST_BATCH_SIZE}"
      --rps "${SHARD_RPS}"
      --duration-secs "${DURATION_SECS}"
      --warmup-secs "${LOADTEST_WARMUP_SECS}"
      --progress-every-secs "${LOADTEST_PROGRESS_EVERY_SECS}"
      --chain-id "${CHAIN_ID}"
      --market-id "${LOADTEST_MARKET_ID}"
      --market-count "${LOADTEST_MARKET_COUNT}"
    )
    if [[ -n "${LOADGEN_STATE_URL_EFFECTIVE}" ]]; then
      LOADGEN_ARGS+=(--state-url "${LOADGEN_STATE_URL_EFFECTIVE}")
    fi
    if [[ -n "${LOADTEST_REQUEST_TIMEOUT_SECS:-}" ]]; then
      LOADGEN_ARGS+=(--request-timeout-secs "${LOADTEST_REQUEST_TIMEOUT_SECS}")
    fi
    "${LOADGEN_BINARY_PATH}" "${LOADGEN_ARGS[@]}" | tee "${SHARD_LOG}" &
    LOADGEN_PIDS+=($!)
  done

  for pid in "${LOADGEN_PIDS[@]}"; do
    wait "${pid}"
  done

  for shard_log in "${LOADGEN_LOGS[@]}"; do
    printf '\n=== %s ===\n' "$(basename "${shard_log}")" >> "${LOADTEST_LOG}"
    cat "${shard_log}" >> "${LOADTEST_LOG}"
  done
else
  info "running localhost load against ${LOADTEST_TARGET_VALIDATOR} (${TARGET_URL})"
  LOADGEN_ARGS=(
    --url "${TARGET_URL}"
    --workload place-order
    --writers-json "${WRITERS_JSON}"
    --concurrency "${WRITER_COUNT}"
    --pipeline-depth "${LOADTEST_PIPELINE_DEPTH}"
    --batch-size "${LOADTEST_BATCH_SIZE}"
    --rps "${TARGET_RPS}"
    --duration-secs "${DURATION_SECS}"
    --warmup-secs "${LOADTEST_WARMUP_SECS}"
    --progress-every-secs "${LOADTEST_PROGRESS_EVERY_SECS}"
    --chain-id "${CHAIN_ID}"
    --market-id "${LOADTEST_MARKET_ID}"
    --market-count "${LOADTEST_MARKET_COUNT}"
  )
  if [[ -n "${LOADGEN_STATE_URL_EFFECTIVE}" ]]; then
    LOADGEN_ARGS+=(--state-url "${LOADGEN_STATE_URL_EFFECTIVE}")
  fi
  if [[ -n "${LOADTEST_REQUEST_TIMEOUT_SECS:-}" ]]; then
    LOADGEN_ARGS+=(--request-timeout-secs "${LOADTEST_REQUEST_TIMEOUT_SECS}")
  fi
  "${LOADGEN_BINARY_PATH}" "${LOADGEN_ARGS[@]}" | tee "${LOADTEST_LOG}"
fi

wait_commit_timeseries_sampler

if [[ "${LOADTEST_DRAIN_SECS}" -gt 0 ]]; then
  info "waiting ${LOADTEST_DRAIN_SECS}s for policy nonce frontiers to settle"
  sleep "${LOADTEST_DRAIN_SECS}"
fi

SUMMARY_URL="$(first_responsive_url || true)"
if [[ -z "${SUMMARY_URL}" ]]; then
  SUMMARY_URL="${TARGET_URL}"
fi
SUMMARY_URLS=(
  "${SUMMARY_URL}"
  "http://127.0.0.1:${PORTS[0]}"
  "http://127.0.0.1:${PORTS[1]}"
  "http://127.0.0.1:${PORTS[2]}"
  "http://127.0.0.1:${PORTS[3]}"
)
SUMMARY_URLS_CSV="$(printf '%s\n' "${SUMMARY_URLS[@]}" | awk '!seen[$0]++' | paste -sd, -)"

info "capturing committed nonce deltas from ${SUMMARY_URL} into ${COMMIT_SUMMARY_JSON}"
python3 - \
  "${START_NONCES_JSON}" \
  "${SUMMARY_URLS_CSV}" \
  "${COMMIT_SUMMARY_JSON}" \
  "${RPC_TIMEOUT_SECS}" \
  "${OUTPUT_DIR}/validator-1.log" \
  "${OUTPUT_DIR}/validator-2.log" \
  "${OUTPUT_DIR}/validator-3.log" \
  "${OUTPUT_DIR}/validator-4.log" <<'PY'
import json
import os
import re
import statistics
import subprocess
import sys
import time

start_path, urls_csv, out_path, timeout_secs, *log_paths = sys.argv[1:]
urls = [url for url in urls_csv.split(",") if url]
timeout_secs = str(timeout_secs)
with open(start_path, "r", encoding="utf-8") as fh:
    start_rows = json.load(fh)

def fetch_policy(url: str, policy_id: int):
    payload = (
        '{"jsonrpc":"2.0","method":"kombat_getPolicy","params":{"policy_id":'
        + str(policy_id)
        + '},"id":1}'
    )
    raw = subprocess.check_output(
        [
            "curl",
            "-sS",
            "--max-time",
            timeout_secs,
            "-H",
            "content-type: application/json",
            "--data",
            payload,
            url,
        ],
        text=True,
    )
    response = json.loads(raw)
    return response["result"]["next_intent_nonce"]

rows = []
summary = None
try:
    deadline = time.monotonic() + (60.0 if (os.environ.get("KOMBAT_CONSENSUS") or "mahi") == "mahi" else 10.0)
    last_error = "policy query failed"
    while True:
        rows = []
        failed = False
        for row in start_rows:
            end_nonce = None
            for url in urls:
                try:
                    end_nonce = fetch_policy(url, row["policy_id"])
                    break
                except Exception as error:
                    last_error = f"policy query failed for policy_id={row['policy_id']} via {url}: {error}"
                    continue
            if end_nonce is None:
                failed = True
                break
            committed = max(0, end_nonce - row["start_next_intent_nonce"])
            rows.append(
                {
                    **row,
                    "end_next_intent_nonce": end_nonce,
                    "committed_intents": committed,
                }
            )
        if not failed:
            break
        if time.monotonic() >= deadline:
            raise RuntimeError(last_error)
        time.sleep(0.5)
    summary = {
        "writers": rows,
        "total_committed_intents": sum(row["committed_intents"] for row in rows),
        "min_committed_per_writer": min(row["committed_intents"] for row in rows),
        "max_committed_per_writer": max(row["committed_intents"] for row in rows),
        "avg_committed_per_writer": statistics.mean(
            row["committed_intents"] for row in rows
        ),
        "estimated_from_logs": False,
    }
except Exception as error:
    committed_by_height = {}
    pattern = re.compile(r"committed height=(\d+) items=(\d+)")
    for path in log_paths:
        if not os.path.exists(path):
            continue
        with open(path, "r", encoding="utf-8") as fh:
            for line in fh:
                match = pattern.search(line)
                if not match:
                    continue
                height = int(match.group(1))
                items = int(match.group(2))
                committed_by_height[height] = max(committed_by_height.get(height, 0), items)
    summary = {
        "writers": [],
        "total_committed_intents": sum(committed_by_height.values()),
        "min_committed_per_writer": None,
        "max_committed_per_writer": None,
        "avg_committed_per_writer": None,
        "estimated_from_logs": True,
        "estimated_unique_block_count": len(committed_by_height),
        "error": str(error),
    }

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)

if summary["estimated_from_logs"]:
    print(
        "PASS  committed summary (log estimate): total={} blocks={} reason={}".format(
            summary["total_committed_intents"],
            summary["estimated_unique_block_count"],
            summary["error"],
        )
    )
else:
    print(
        "PASS  committed summary: total={} min_per_writer={} avg_per_writer={:.2f} max_per_writer={}".format(
            summary["total_committed_intents"],
            summary["min_committed_per_writer"],
            summary["avg_committed_per_writer"],
            summary["max_committed_per_writer"],
        )
    )
PY

bash "${STATUS_SCRIPT}" "${OUTPUT_DIR}" | tee "${STATUS_AFTER_LOG}"
read -r final_height final_hash < <(wait_for_cluster_convergence "post-load convergence")
if [[ "${KOMBAT_CONSENSUS:-mahi}" == "mahi" ]]; then
  ok "post-load cluster converged at height=${final_height} state_root=${final_hash}"
else
  ok "post-load cluster converged at height=${final_height} hash=${final_hash}"
fi

info "summary: ${PASS} passed, ${FAIL} failed"
info "artifacts: ${OUTPUT_DIR}"
if [[ "${FAIL}" -ne 0 ]]; then
  exit 1
fi
