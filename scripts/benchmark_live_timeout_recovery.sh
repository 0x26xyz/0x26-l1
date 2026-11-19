#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  benchmark_live_timeout_recovery.sh <generated-assets-dir>

Description:
  Exercises live leader-failure recovery on the real validator hosts and
  reports timing for:
  1. leader stop -> surviving validators advance round
  2. leader restart -> leader becomes reachable again
  3. one post-rejoin follower-routed intent commit

Environment:
  KOMBAT_SIGN_PATH             passed through to the signer
  BENCH_JSON_OUT               optional path to write JSON summary
  BENCH_OWNER_SECRET_HEX       default: 1414..14
  BENCH_SESSION_SECRET_HEX     default: 1515..15
  BENCH_ACCOUNT_ID             default: 990000 + unix_time_mod
  BENCH_POLICY_ID              default: 991000 + unix_time_mod
  BENCH_MARKET_ID              default: 1
  BENCH_CLIENT_ORDER_ID        default: 900000
  BENCH_WAIT_TIMEOUT_MS        default: 15000
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"
VALIDATOR_SET_FILE="${ASSET_DIR}/validator-set.toml"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "benchmark_live_timeout_recovery.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi
if [[ ! -f "${VALIDATOR_SET_FILE}" ]]; then
  echo "benchmark_live_timeout_recovery.sh: missing validator-set.toml: ${VALIDATOR_SET_FILE}" >&2
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
require ssh

if [[ ! -x "${SIGN_BINARY}" ]]; then
  echo "benchmark_live_timeout_recovery.sh: missing signer binary: ${SIGN_BINARY}" >&2
  exit 1
fi
if [[ ! -f "${SEED_SCRIPT}" ]]; then
  echo "benchmark_live_timeout_recovery.sh: missing seed helper: ${SEED_SCRIPT}" >&2
  exit 1
fi
if [[ ! -f "${STATUS_SCRIPT}" ]]; then
  echo "benchmark_live_timeout_recovery.sh: missing status helper: ${STATUS_SCRIPT}" >&2
  exit 1
fi

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }
bad() { printf -- 'FAIL  %s\n' "$*" >&2; }

LEADER_HOST=""
leader_stopped=0
cleanup() {
  if [[ "${leader_stopped}" -eq 1 && -n "${LEADER_HOST}" ]]; then
    ssh "${LEADER_HOST}" 'systemctl start kombat-chain' >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

current_ms() {
  python3 -c 'import time; print(int(time.time()*1000))'
}

rpc_json() {
  local url="$1"
  local method="$2"
  local params="$3"
  curl -sS -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params},\"id\":1}" \
    "${url}"
}

RUN_TAG="$(python3 -c 'import time; print(int(time.time()) % 100000)')"
ACCOUNT_ID="${BENCH_ACCOUNT_ID:-99${RUN_TAG}}"
POLICY_ID="${BENCH_POLICY_ID:-991${RUN_TAG}}"
OWNER_SECRET_HEX="${BENCH_OWNER_SECRET_HEX:-1414141414141414141414141414141414141414141414141414141414141414}"
SESSION_SECRET_HEX="${BENCH_SESSION_SECRET_HEX:-1515151515151515151515151515151515151515151515151515151515151515}"
MARKET_ID="${BENCH_MARKET_ID:-1}"
CLIENT_ORDER_ID="${BENCH_CLIENT_ORDER_ID:-900000}"
WAIT_TIMEOUT_MS="${BENCH_WAIT_TIMEOUT_MS:-15000}"
BENCH_JSON_OUT="${BENCH_JSON_OUT:-}"

HOST_ROWS=()
while IFS= read -r row; do
  HOST_ROWS+=("${row}")
done < <(tail -n +2 "${HOSTS_FILE}")

if [[ "${#HOST_ROWS[@]}" -lt 4 ]]; then
  echo "benchmark_live_timeout_recovery.sh: expected at least 4 validator rows in hosts.txt" >&2
  exit 1
fi

FOLLOW_URL="$(printf '%s\n' "${HOST_ROWS[0]}" | awk -F'\t' '{print $3}')"
FOLLOWER_SUBMIT_URL="$(printf '%s\n' "${HOST_ROWS[1]}" | awk -F'\t' '{print $3}')"

CURRENT_ROUND="$(curl -sS "${FOLLOW_URL%/}/network/bootstrap" | jq -r '.current_round')"
CHAIN_ID="$(rpc_json "${FOLLOW_URL}" "kombat_chainInfo" "{}" | jq -r '.result.chain_id')"

LEADER_ID_HEX="$(
  python3 - "${VALIDATOR_SET_FILE}" "${CHAIN_ID}" "${CURRENT_ROUND}" <<'PY'
import sys, tomllib
path, chain_id, round_no = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
with open(path, "rb") as f:
    data = tomllib.load(f)
validators = [v for v in data["validators"] if v.get("enabled", True)]
total_stake = sum(int(v["bond"]["amount"]) for v in validators)
if total_stake <= 0:
    raise SystemExit("zero stake")
value = ((chain_id & ((1 << 64) - 1)) << 32) & ((1 << 64) - 1)
value ^= round_no & ((1 << 64) - 1)
value ^= 0x9e3779b97f4a7c15
value &= (1 << 64) - 1
value = ((value ^ (value >> 30)) * 0xbf58476d1ce4e5b9) & ((1 << 64) - 1)
value = ((value ^ (value >> 27)) * 0x94d049bb133111eb) & ((1 << 64) - 1)
value ^= value >> 31
target = value % total_stake
cumulative = 0
for validator in validators:
    cumulative += int(validator["bond"]["amount"])
    if cumulative > target:
        print(validator["validator_id"])
        break
else:
    print(validators[-1]["validator_id"])
PY
)"

LEADER_HOST=""
LEADER_URL=""
SURVIVOR_URLS=()
for row in "${HOST_ROWS[@]}"; do
  name="$(printf '%s\n' "${row}" | awk -F'\t' '{print $1}')"
  ssh_host="$(printf '%s\n' "${row}" | awk -F'\t' '{print $2}')"
  public_rpc_url="$(printf '%s\n' "${row}" | awk -F'\t' '{print $3}')"
  validator_env="${ASSET_DIR}/${name}.env"
  validator_id_hex="$(awk -F= '/^KOMBAT_VALIDATOR_ID_HEX=/{print $2}' "${validator_env}")"
  if [[ "${validator_id_hex}" == "${LEADER_ID_HEX}" ]]; then
    LEADER_HOST="${ssh_host}"
    LEADER_URL="${public_rpc_url}"
  else
    SURVIVOR_URLS+=("${public_rpc_url}")
  fi
done

if [[ -z "${LEADER_HOST}" || -z "${LEADER_URL}" ]]; then
  bad "failed to map leader ${LEADER_ID_HEX} to a host"
  exit 1
fi

TARGET_ROUND=$(( CURRENT_ROUND + 1 ))
stop_started_ms="$(current_ms)"
info "stopping elected leader ${LEADER_ID_HEX} on ${LEADER_HOST} for round ${CURRENT_ROUND}"
ssh "${LEADER_HOST}" 'systemctl stop kombat-chain'
leader_stopped=1

survivor_deadline_ms=$(( stop_started_ms + WAIT_TIMEOUT_MS ))
survivor_reached_ms=0
survivors_reached_target=0
survivor_rounds_csv=""
while (( "$(current_ms)" < survivor_deadline_ms )); do
  current_rounds=()
  current_reached=0
  for url in "${SURVIVOR_URLS[@]}"; do
    round="$(curl -sS "${url%/}/network/bootstrap" 2>/dev/null | jq -r '.current_round // 0' 2>/dev/null || echo 0)"
    current_rounds+=("${round}")
    if [[ "${round}" =~ ^[0-9]+$ ]] && (( round >= TARGET_ROUND )); then
      current_reached=$((current_reached + 1))
    fi
  done
  if (( current_reached > 0 )); then
    survivors_reached_target="${current_reached}"
    survivor_rounds_csv="$(IFS=,; printf '%s' "${current_rounds[*]}")"
    survivor_reached_ms="$(current_ms)"
    break
  fi
  sleep 0.2
done
if (( survivors_reached_target == 0 )); then
  bad "no survivor reached round ${TARGET_ROUND}"
  exit 1
fi
ok "survivor recovery reached round ${TARGET_ROUND} on ${survivors_reached_target}/${#SURVIVOR_URLS[@]} survivors"
recovery_ms="$((survivor_reached_ms - stop_started_ms))"

restart_started_ms="$(current_ms)"
info "restarting stopped leader ${LEADER_HOST}"
ssh "${LEADER_HOST}" 'systemctl start kombat-chain'
leader_stopped=0

leader_reachable=0
leader_deadline_ms=$(( restart_started_ms + WAIT_TIMEOUT_MS ))
while (( "$(current_ms)" < leader_deadline_ms )); do
  if rpc_json "${LEADER_URL}" "kombat_chainInfo" "{}" | jq -e '.result.chain_id == 7' >/dev/null 2>&1; then
    leader_reachable=1
    break
  fi
  sleep 0.2
done
if [[ "${leader_reachable}" -ne 1 ]]; then
  bad "restarted leader did not become reachable at ${LEADER_URL}"
  exit 1
fi
leader_rejoin_ms="$(( $(current_ms) - restart_started_ms ))"
ok "restarted leader is reachable at ${LEADER_URL}"

info "seeding post-rejoin account ${ACCOUNT_ID} and policy ${POLICY_ID}"
KOMBAT_SIGN_PATH="${SIGN_BINARY}" \
  bash "${SEED_SCRIPT}" "${ASSET_DIR}" "${ACCOUNT_ID}" "${POLICY_ID}" "${OWNER_SECRET_HEX}" "${SESSION_SECRET_HEX}"

WORK_DIR="$(mktemp -d -t kombat-live-timeout-bench.XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT

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

    if (( $(current_ms) - start_ms > WAIT_TIMEOUT_MS )); then
      return 1
    fi
    sleep 0.2
  done
}

request_path="${WORK_DIR}/post-rejoin-intent.json"
now_ms="$(current_ms)"
expires_at_ms="$((now_ms + 600000))"
"${SIGN_BINARY}" place-order-intent \
  --session-secret-hex "${SESSION_SECRET_HEX}" \
  --chain-id 7 \
  --account-id "${ACCOUNT_ID}" \
  --policy-id "${POLICY_ID}" \
  --nonce 1 \
  --market-id "${MARKET_ID}" \
  --client-order-id "${CLIENT_ORDER_ID}" \
  --side bid \
  --quantity-lots 1 \
  --order-kind limit \
  --price-ticks 101 \
  --expires-at "${expires_at_ms}" \
  --timestamp-ms "${now_ms}" \
  > "${request_path}"

submit_started_ms="$(current_ms)"
response="$(curl -sS -H 'content-type: application/json' --data-binary @"${request_path}" "${FOLLOWER_SUBMIT_URL}")"
if ! printf '%s' "${response}" | jq -e '.result.class == "direct"' >/dev/null 2>&1; then
  bad "post-rejoin intent submission failed: ${response}"
  exit 1
fi

if ! wait_for_cluster_nonce "2"; then
  bad "post-rejoin intent failed to reach cluster-wide nonce 2"
  exit 1
fi
post_rejoin_commit_ms="$(( $(current_ms) - submit_started_ms ))"
ok "post-rejoin follower-routed intent committed in ${post_rejoin_commit_ms} ms"

ok "timeout recovery benchmark: leader_failover_ms=${recovery_ms} leader_rejoin_ms=${leader_rejoin_ms} post_rejoin_commit_ms=${post_rejoin_commit_ms}"
if [[ -n "${BENCH_JSON_OUT}" ]]; then
  export ACCOUNT_ID POLICY_ID LEADER_HOST LEADER_URL LEADER_ID_HEX CURRENT_ROUND TARGET_ROUND recovery_ms leader_rejoin_ms post_rejoin_commit_ms FOLLOWER_SUBMIT_URL survivors_reached_target survivor_rounds_csv
  python3 - "${BENCH_JSON_OUT}" <<'PY'
import json
import os
import sys

path = sys.argv[1]
summary = {
    "kind": "live_timeout_recovery_benchmark",
    "account_id": int(os.environ["ACCOUNT_ID"]),
    "policy_id": int(os.environ["POLICY_ID"]),
    "leader_host": os.environ["LEADER_HOST"],
    "leader_url": os.environ["LEADER_URL"],
    "leader_id_hex": os.environ["LEADER_ID_HEX"],
    "current_round": int(os.environ["CURRENT_ROUND"]),
    "target_round": int(os.environ["TARGET_ROUND"]),
    "leader_failover_ms": int(os.environ["recovery_ms"]),
    "survivors_reached_target_round": int(os.environ["survivors_reached_target"]),
    "survivor_rounds_at_recovery": [int(value) for value in os.environ["survivor_rounds_csv"].split(",") if value],
    "leader_rejoin_ms": int(os.environ["leader_rejoin_ms"]),
    "post_rejoin_commit_ms": int(os.environ["post_rejoin_commit_ms"]),
    "follower_submit_url": os.environ["FOLLOWER_SUBMIT_URL"],
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)
PY
fi
info "cluster status"
bash "${STATUS_SCRIPT}" "${ASSET_DIR}"
