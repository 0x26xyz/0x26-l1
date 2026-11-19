#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  benchmark_live_network.sh <generated-assets-dir> [count]

Description:
  Seeds one fresh account + policy through the live validator network, then
  submits `count` signed place-order intents through a follower and measures
  end-to-end commit latency across the real 4-host network.

Environment:
  KOMBAT_SIGN_PATH         Override signer binary path
                           default: chain/target/release/kombat_sign
  BENCH_JSON_OUT           optional path to write JSON summary
  BENCH_COUNT              default: 10
  BENCH_ACCOUNT_ID         default: 970000 + unix_time_mod
  BENCH_POLICY_ID          default: 980000 + unix_time_mod
  BENCH_OWNER_SECRET_HEX   default: 1212..12
  BENCH_SESSION_SECRET_HEX default: 1313..13
  BENCH_MARKET_ID          default: 1
  BENCH_CLIENT_ORDER_ID    default: 800000
  BENCH_WAIT_TIMEOUT_MS    default: 15000
EOF
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
COUNT="${2:-${BENCH_COUNT:-10}}"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "benchmark_live_network.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SIGN_BINARY="${KOMBAT_SIGN_PATH:-${CHAIN_DIR}/target/release/kombat_sign}"
SEED_SCRIPT="${CHAIN_DIR}/scripts/seed_network_account.sh"
STATUS_SCRIPT="${CHAIN_DIR}/scripts/live_network_status.sh"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require curl
require jq
require python3

if [[ ! -x "${SIGN_BINARY}" ]]; then
  echo "benchmark_live_network.sh: missing signer binary: ${SIGN_BINARY}" >&2
  exit 1
fi

if [[ ! -f "${SEED_SCRIPT}" ]]; then
  echo "benchmark_live_network.sh: missing seed helper: ${SEED_SCRIPT}" >&2
  exit 1
fi

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }
bad() { printf -- 'FAIL  %s\n' "$*" >&2; }

RUN_TAG="$(python3 -c 'import time; print(int(time.time()) % 100000)')"
ACCOUNT_ID="${BENCH_ACCOUNT_ID:-97${RUN_TAG}}"
POLICY_ID="${BENCH_POLICY_ID:-98${RUN_TAG}}"
OWNER_SECRET_HEX="${BENCH_OWNER_SECRET_HEX:-1212121212121212121212121212121212121212121212121212121212121212}"
SESSION_SECRET_HEX="${BENCH_SESSION_SECRET_HEX:-1313131313131313131313131313131313131313131313131313131313131313}"
MARKET_ID="${BENCH_MARKET_ID:-1}"
CLIENT_ORDER_ID_BASE="${BENCH_CLIENT_ORDER_ID:-800000}"
WAIT_TIMEOUT_MS="${BENCH_WAIT_TIMEOUT_MS:-15000}"
BENCH_JSON_OUT="${BENCH_JSON_OUT:-}"

HOST_ROWS=()
while IFS= read -r row; do
  HOST_ROWS+=("${row}")
done < <(tail -n +2 "${HOSTS_FILE}")
if [[ "${#HOST_ROWS[@]}" -lt 4 ]]; then
  echo "benchmark_live_network.sh: expected at least 4 validator rows in hosts.txt" >&2
  exit 1
fi

FOLLOWER_URL="$(printf '%s\n' "${HOST_ROWS[1]}" | awk -F'\t' '{print $3}')"
CANONICAL_URL="$(printf '%s\n' "${HOST_ROWS[0]}" | awk -F'\t' '{print $3}')"
WORK_DIR="$(mktemp -d -t kombat-live-bench.XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT

rpc_json() {
  local url="$1"
  local method="$2"
  local params="$3"
  curl -sS -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params},\"id\":1}" \
    "${url}"
}

current_ms() {
  python3 -c 'import time; print(int(time.time()*1000))'
}

wait_for_cluster_nonce() {
  local expected_nonce="$1"
  local start_ms
  start_ms="$(current_ms)"
  while true; do
    local ready=1
    while IFS=$'\t' read -r name ssh_host public_rpc_url; do
      if [[ "${name}" == "name" ]]; then
        continue
      fi
      local policy_info
      policy_info="$(rpc_json "${public_rpc_url}" "kombat_getPolicy" "{\"policy_id\":${POLICY_ID}}")"
      local next_nonce
      next_nonce="$(printf '%s' "${policy_info}" | jq -r '.result.next_intent_nonce // "ERR"' 2>/dev/null || echo ERR)"
      if [[ "${next_nonce}" != "${expected_nonce}" ]]; then
        ready=0
        break
      fi
    done < "${HOSTS_FILE}"

    if [[ "${ready}" == "1" ]]; then
      return 0
    fi

    local now_ms
    now_ms="$(current_ms)"
    if (( now_ms - start_ms > WAIT_TIMEOUT_MS )); then
      return 1
    fi
    sleep 0.2
  done
}

info "seeding benchmark account ${ACCOUNT_ID} and policy ${POLICY_ID}"
KOMBAT_SIGN_PATH="${SIGN_BINARY}" \
  bash "${SEED_SCRIPT}" "${ASSET_DIR}" "${ACCOUNT_ID}" "${POLICY_ID}" "${OWNER_SECRET_HEX}" "${SESSION_SECRET_HEX}"

latencies_ms=()
benchmark_start_ms="$(current_ms)"

for ((i=1; i<=COUNT; i++)); do
  nonce="${i}"
  expected_next_nonce="$((i + 1))"
  client_order_id="$((CLIENT_ORDER_ID_BASE + i - 1))"
  request_path="${WORK_DIR}/intent-${i}.json"
  now_ms="$(current_ms)"
  expires_at_ms="$((now_ms + 600000))"

  "${SIGN_BINARY}" place-order-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --chain-id 7 \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce "${nonce}" \
    --market-id "${MARKET_ID}" \
    --client-order-id "${client_order_id}" \
    --side bid \
    --quantity-lots 1 \
    --order-kind limit \
    --price-ticks $((100 + i)) \
    --expires-at "${expires_at_ms}" \
    --timestamp-ms "${now_ms}" \
    > "${request_path}"

  submit_start_ms="$(current_ms)"
  response="$(curl -sS -H 'content-type: application/json' --data-binary @"${request_path}" "${FOLLOWER_URL}")"
  if ! printf '%s' "${response}" | jq -e '.result.class == "direct"' >/dev/null 2>&1; then
    bad "intent ${i} submission failed: ${response}"
    exit 1
  fi

  if ! wait_for_cluster_nonce "${expected_next_nonce}"; then
    bad "intent ${i} failed to reach cluster-wide nonce ${expected_next_nonce}"
    exit 1
  fi

  commit_ms="$(current_ms)"
  latency_ms="$((commit_ms - submit_start_ms))"
  latencies_ms+=("${latency_ms}")
  ok "intent ${i}/${COUNT} committed in ${latency_ms} ms"
done

benchmark_end_ms="$(current_ms)"
total_ms="$((benchmark_end_ms - benchmark_start_ms))"

LATENCIES_CSV="$(IFS=,; printf '%s' "${latencies_ms[*]}")"
export LATENCIES_CSV COUNT total_ms ACCOUNT_ID POLICY_ID FOLLOWER_URL CANONICAL_URL BENCH_JSON_OUT
python3 <<'PY'
import json
import math
import os
import statistics

latencies = [int(value) for value in os.environ["LATENCIES_CSV"].split(",") if value]
count = int(os.environ["COUNT"])
total_ms = int(os.environ["total_ms"])

def percentile(values, pct):
    if not values:
        return "n/a"
    values = sorted(values)
    if len(values) == 1:
        return values[0]
    idx = math.ceil((pct / 100.0) * len(values)) - 1
    idx = max(0, min(idx, len(values) - 1))
    return values[idx]

throughput = (count * 1000.0 / total_ms) if total_ms > 0 else 0.0
summary = {
    "kind": "live_network_benchmark",
    "count": count,
    "total_ms": total_ms,
    "throughput_tps": round(throughput, 4),
    "account_id": int(os.environ["ACCOUNT_ID"]),
    "policy_id": int(os.environ["POLICY_ID"]),
    "follower_url": os.environ["FOLLOWER_URL"],
    "canonical_url": os.environ["CANONICAL_URL"],
    "latency_ms": {
        "min": min(latencies),
        "avg": round(statistics.mean(latencies), 4),
        "p50": percentile(latencies, 50),
        "p90": percentile(latencies, 90),
        "p99": percentile(latencies, 99),
        "max": max(latencies),
        "samples": latencies,
    },
}
print(f"PASS  benchmark summary: count={count} total_ms={total_ms} throughput_tps={throughput:.2f}")
print(
    "PASS  latency summary: "
    f"min={min(latencies)} avg={statistics.mean(latencies):.1f} "
    f"p50={percentile(latencies, 50)} p90={percentile(latencies, 90)} "
    f"p99={percentile(latencies, 99)} max={max(latencies)}"
)
json_out = os.environ.get("BENCH_JSON_OUT")
if json_out:
    with open(json_out, "w", encoding="utf-8") as fh:
        json.dump(summary, fh, indent=2, sort_keys=True)
PY

info "canonical validator status (${CANONICAL_URL})"
bash "${STATUS_SCRIPT}" "${ASSET_DIR}"
