#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  loadtest_live_network_multi_ingress.sh <generated-assets-dir> [writers-per-endpoint] [total-rps] [duration-secs]

Description:
  Runs the live testnet benchmark shape used by the known green Mahi live runs:
  multiple ingress endpoints per validator host, unique writer shards per
  endpoint, validator-local loadgen processes, and unique remote tmp dirs.

Defaults:
  writers-per-endpoint: 32
  total-rps:            100000
  duration-secs:        60
  endpoint ports:       8645 8646 8647

Environment:
  LOADTEST_WRITERS_JSON          Reuse a full writer manifest. Must contain
                                 writers-per-endpoint * endpoint-count writers.
  LOADTEST_OUTPUT_DIR            Default: /tmp/and-live-distributed-ingress3-<ts>
  LOADTEST_ENDPOINT_PORTS        Space-separated ingress ports. Default:
                                 "8645 8646 8647"
  LOADTEST_VALIDATOR_NAMES       Optional space-separated validator names from
                                 hosts.txt to use as ingress/loadgen sources.
                                 Consensus/state checks still use hosts.txt.
  LOADTEST_EXTERNAL_RUNNER_SSH   Optional SSH host that runs every loadgen
                                 process. When set, loadgen targets public
                                 validator ingress URLs instead of 127.0.0.1.
  LOADTEST_PIPELINE_DEPTH        Default: 1
  LOADTEST_BATCH_SIZE            Default: 8
  LOADTEST_STRICT_NONCE_RETRY    Passed through to loadtest_live_network_from_validator.sh.
                                Default: 1. Use 0 for pipelined canaries.
  LOADTEST_FAIL_ON_ERRORS        Passed through to loadtest_live_network_from_validator.sh.
                                Default: 1.
  LOADTEST_REQUEST_TIMEOUT_SECS  Passed through to loadtest_live_network_from_validator.sh.
  LOADTEST_DRAIN_SECS            Default: 30
  LOADTEST_MARKET_COUNT          Default: 1
  LOADTEST_REMOTE_LOADGEN_BINARY Passed through to loadtest_live_network_from_validator.sh
  LOADTEST_RUNNER_SSH_OPTS       Passed through to loadtest_live_network_from_validator.sh
  LOADTEST_REMOTE_CACHE_DIR      Stable remote cache root for loadgen binary,
                                 writers, and payload caches.
  LOADTEST_PAYLOAD_CACHE         Passed through to loadtest_live_network_from_validator.sh
  LOADTEST_PAYLOAD_CACHE_DIR     Passed through to loadtest_live_network_from_validator.sh
  LOADTEST_INTENT_TTL_MS         Passed through to loadtest_live_network_from_validator.sh
  LOADTEST_CLIENT_ORDER_ID_BASE  Passed through to loadtest_live_network_from_validator.sh
EOF
}

if [[ $# -lt 1 || $# -gt 4 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
WRITERS_PER_ENDPOINT="${2:-32}"
TOTAL_RPS="${3:-100000}"
DURATION_SECS="${4:-60}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREPARE_SCRIPT="${SCRIPT_DIR}/prepare_live_loadtest_accounts.sh"
RUN_ONE_SCRIPT="${SCRIPT_DIR}/loadtest_live_network_from_validator.sh"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require jq
require python3

if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "loadtest_live_network_multi_ingress.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

if ! [[ "${WRITERS_PER_ENDPOINT}" =~ ^[0-9]+$ ]] || [[ "${WRITERS_PER_ENDPOINT}" -eq 0 ]]; then
  echo "loadtest_live_network_multi_ingress.sh: writers-per-endpoint must be positive" >&2
  exit 1
fi
if ! [[ "${TOTAL_RPS}" =~ ^[0-9]+$ ]] || [[ "${TOTAL_RPS}" -eq 0 ]]; then
  echo "loadtest_live_network_multi_ingress.sh: total-rps must be positive" >&2
  exit 1
fi
if ! [[ "${DURATION_SECS}" =~ ^[0-9]+$ ]] || [[ "${DURATION_SECS}" -eq 0 ]]; then
  echo "loadtest_live_network_multi_ingress.sh: duration-secs must be positive" >&2
  exit 1
fi

LOADTEST_ENDPOINT_PORTS="${LOADTEST_ENDPOINT_PORTS:-8645 8646 8647}"
read -r -a ENDPOINT_PORTS <<< "${LOADTEST_ENDPOINT_PORTS}"
if [[ "${#ENDPOINT_PORTS[@]}" -eq 0 ]]; then
  echo "loadtest_live_network_multi_ingress.sh: LOADTEST_ENDPOINT_PORTS produced no ports" >&2
  exit 1
fi

VALIDATOR_NAMES=()
if [[ -n "${LOADTEST_VALIDATOR_NAMES:-}" ]]; then
  read -r -a VALIDATOR_NAMES <<< "${LOADTEST_VALIDATOR_NAMES}"
  for validator_name in "${VALIDATOR_NAMES[@]}"; do
    if ! awk -F'\t' -v name="${validator_name}" 'NR > 1 && $1 == name {found=1} END {exit found ? 0 : 1}' "${HOSTS_FILE}"; then
      echo "loadtest_live_network_multi_ingress.sh: LOADTEST_VALIDATOR_NAMES includes unknown validator: ${validator_name}" >&2
      exit 1
    fi
  done
else
  while IFS= read -r validator_name; do
    VALIDATOR_NAMES+=("${validator_name}")
  done < <(awk -F'\t' 'NR > 1 {print $1}' "${HOSTS_FILE}")
fi
if [[ "${#VALIDATOR_NAMES[@]}" -eq 0 ]]; then
  echo "loadtest_live_network_multi_ingress.sh: no validators in ${HOSTS_FILE}" >&2
  exit 1
fi

ENDPOINT_COUNT=$(( ${#VALIDATOR_NAMES[@]} * ${#ENDPOINT_PORTS[@]} ))
TOTAL_WRITERS=$(( WRITERS_PER_ENDPOINT * ENDPOINT_COUNT ))
BASE_RPS=$(( TOTAL_RPS / ENDPOINT_COUNT ))
REMAINDER_RPS=$(( TOTAL_RPS % ENDPOINT_COUNT ))
RUN_ID="$(date +%Y%m%d-%H%M%S)"
OUTPUT_DIR="${LOADTEST_OUTPUT_DIR:-/tmp/and-live-distributed-ingress3-${TOTAL_RPS}-${RUN_ID}}"
SHARD_DIR="${OUTPUT_DIR}/writer-shards"
mkdir -p "${SHARD_DIR}"

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }

if [[ -n "${LOADTEST_WRITERS_JSON:-}" ]]; then
  WRITERS_JSON="${LOADTEST_WRITERS_JSON}"
else
  WRITERS_JSON="${OUTPUT_DIR}/writers-all.json"
  info "seeding ${TOTAL_WRITERS} writers into ${WRITERS_JSON}"
  bash "${PREPARE_SCRIPT}" "${ASSET_DIR}" "${TOTAL_WRITERS}" "${WRITERS_JSON}"
fi

ACTUAL_WRITER_COUNT="$(jq 'length' "${WRITERS_JSON}")"
if [[ "${ACTUAL_WRITER_COUNT}" -lt "${TOTAL_WRITERS}" ]]; then
  echo "loadtest_live_network_multi_ingress.sh: ${WRITERS_JSON} has ${ACTUAL_WRITER_COUNT} writers, need ${TOTAL_WRITERS}" >&2
  exit 1
fi

info "running ${ENDPOINT_COUNT} live ingress loadgens: total_rps=${TOTAL_RPS} duration=${DURATION_SECS}s writers_per_endpoint=${WRITERS_PER_ENDPOINT}"
PIDS=()
NAMES=()
endpoint_index=0
for validator_name in "${VALIDATOR_NAMES[@]}"; do
  validator_rpc_url="$(awk -F'\t' -v name="${validator_name}" '$1 == name {print $3}' "${HOSTS_FILE}")"
  if [[ -z "${validator_rpc_url}" ]]; then
    echo "loadtest_live_network_multi_ingress.sh: failed to resolve public RPC URL for ${validator_name}" >&2
    exit 1
  fi
  validator_base_url="${validator_rpc_url%:*}"
  for port in "${ENDPOINT_PORTS[@]}"; do
    endpoint_index=$(( endpoint_index + 1 ))
    start=$(( (endpoint_index - 1) * WRITERS_PER_ENDPOINT ))
    end=$(( start + WRITERS_PER_ENDPOINT ))
    shard="${SHARD_DIR}/${validator_name}-p${port}.json"
    jq ".[${start}:${end}]" "${WRITERS_JSON}" > "${shard}"
    endpoint_rps="${BASE_RPS}"
    if [[ "${endpoint_index}" -le "${REMAINDER_RPS}" ]]; then
      endpoint_rps=$(( endpoint_rps + 1 ))
    fi
    endpoint_dir="${OUTPUT_DIR}/${validator_name}-p${port}-${DURATION_SECS}s"
    mkdir -p "${endpoint_dir}"
    NAMES+=("${validator_name}: ${port}")
    target_url="http://127.0.0.1:${port}"
    state_url="http://127.0.0.1:8545"
    runner_ssh="${LOADTEST_EXTERNAL_RUNNER_SSH:-}"
    if [[ -n "${LOADTEST_EXTERNAL_RUNNER_HOSTS:-}" ]]; then
      read -r -a ext_hosts <<< "${LOADTEST_EXTERNAL_RUNNER_HOSTS}"
      runner_ssh="${ext_hosts[$(( endpoint_index % ${#ext_hosts[@]} ))]}"
    fi
    if [[ -n "${runner_ssh}" ]]; then
      target_url="${validator_base_url}:${port}"
      state_url="${validator_rpc_url}"
    fi
    remote_tmp_dir="/tmp/and-live-loadtest-remote-${RUN_ID}-${validator_name}-${port}"
    if [[ -n "${LOADTEST_REMOTE_CACHE_DIR:-}" ]]; then
      remote_tmp_dir="${LOADTEST_REMOTE_CACHE_DIR}/${validator_name}-p${port}"
    fi
    (
      LOADTEST_OUTPUT_DIR="${endpoint_dir}" \
      LOADTEST_WRITERS_JSON="${shard}" \
      LOADTEST_RUNNER_SSH="${runner_ssh}" \
      LOADTEST_TARGET_URL="${target_url}" \
      LOADTEST_STATE_URL="${state_url}" \
      LOADTEST_REMOTE_TMP_DIR="${remote_tmp_dir}" \
      LOADTEST_PIPELINE_DEPTH="${LOADTEST_PIPELINE_DEPTH:-1}" \
      LOADTEST_BATCH_SIZE="${LOADTEST_BATCH_SIZE:-8}" \
      LOADTEST_STRICT_NONCE_RETRY="${LOADTEST_STRICT_NONCE_RETRY:-1}" \
      LOADTEST_FAIL_ON_ERRORS="${LOADTEST_FAIL_ON_ERRORS:-1}" \
      LOADTEST_REQUEST_TIMEOUT_SECS="${LOADTEST_REQUEST_TIMEOUT_SECS:-}" \
      LOADTEST_DRAIN_SECS="${LOADTEST_DRAIN_SECS:-30}" \
      LOADTEST_MARKET_COUNT="${LOADTEST_MARKET_COUNT:-1}" \
      bash "${RUN_ONE_SCRIPT}" "${ASSET_DIR}" "${validator_name}" "${WRITERS_PER_ENDPOINT}" "${endpoint_rps}" "${DURATION_SECS}"
    ) > "${endpoint_dir}/driver.log" 2>&1 &
    PIDS+=("$!")
  done
done

failed=0
for idx in "${!PIDS[@]}"; do
  if ! wait "${PIDS[$idx]}"; then
    echo "FAIL endpoint ${NAMES[$idx]} failed; see ${OUTPUT_DIR}" >&2
    failed=1
  fi
done
if [[ "${failed}" -ne 0 ]]; then
  exit 1
fi

SUMMARY_JSON="${OUTPUT_DIR}/aggregate-summary.json"
python3 - "${OUTPUT_DIR}" "${SUMMARY_JSON}" <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
rows = []
for loadtest in sorted(root.glob("*-p*-*s/loadtest.txt")):
    text = loadtest.read_text()
    committed_path = loadtest.parent / "committed-summary.json"
    committed = 0
    if committed_path.exists():
        committed = json.loads(committed_path.read_text()).get("total_committed_intents", 0)
    rps_match = re.search(r"achieved_rps_measured:\s*([0-9.]+)", text)
    ok_match = re.search(r"\bok:\s*([0-9]+)", text)
    nonce_match = re.search(r"\bnonce_err:\s*([0-9]+)", text)
    rpc_match = re.search(r"\brpc_err:\s*([0-9]+)", text)
    net_match = re.search(r"\bnet_err:\s*([0-9]+)", text)
    rows.append({
        "endpoint": loadtest.parent.name,
        "measured_rps": float(rps_match.group(1)) if rps_match else 0.0,
        "ok": int(ok_match.group(1)) if ok_match else 0,
        "nonce_err": int(nonce_match.group(1)) if nonce_match else 0,
        "rpc_err": int(rpc_match.group(1)) if rpc_match else 0,
        "net_err": int(net_match.group(1)) if net_match else 0,
        "committed": int(committed),
    })

summary = {
    "endpoint_count": len(rows),
    "measured_rps_total": sum(row["measured_rps"] for row in rows),
    "ok_total": sum(row["ok"] for row in rows),
    "nonce_err_total": sum(row["nonce_err"] for row in rows),
    "rpc_err_total": sum(row["rpc_err"] for row in rows),
    "net_err_total": sum(row["net_err"] for row in rows),
    "committed_total": sum(row["committed"] for row in rows),
    "endpoints": rows,
}
out.write_text(json.dumps(summary, indent=2, sort_keys=True))
print(json.dumps(summary, indent=2, sort_keys=True))
PY

ok "multi-ingress live load test artifacts written to ${OUTPUT_DIR}"
