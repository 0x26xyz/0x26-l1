#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHAIN_ID="${CHAIN_ID:-7}"
OPERATOR_TOKEN="${OPERATOR_TOKEN:-network-round-token}"
BINARY_PATH="${KOMBAT_BINARY_PATH:-${ROOT_DIR}/target/debug/kombat_chain}"
SIGN_BINARY_PATH="${KOMBAT_SIGN_PATH:-${ROOT_DIR}/target/debug/kombat_sign}"

WORK_DIR="$(mktemp -d -t kombat-network-round.XXXXXX)"
VALIDATOR_SET_PATH="${WORK_DIR}/validator-set.toml"
PASS=0
FAIL=0
BASE_PORT="${BASE_PORT:-$((18000 + (RANDOM % 1000)))}"

ACCOUNT_ID=11
POLICY_ID=42
MARKET_ID=7
OWNER_SECRET_HEX=0808080808080808080808080808080808080808080808080808080808080808
SESSION_SECRET_HEX=0909090909090909090909090909090909090909090909090909090909090909

PORTS=(
  "${BASE_PORT}"
  "$((BASE_PORT + 1))"
  "$((BASE_PORT + 2))"
  "$((BASE_PORT + 3))"
)
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
  "${WORK_DIR}/validator-1.log"
  "${WORK_DIR}/validator-2.log"
  "${WORK_DIR}/validator-3.log"
  "${WORK_DIR}/validator-4.log"
)
PIDS=("" "" "" "")

color() {
  local code="$1"
  shift
  if [[ -t 1 ]]; then
    printf '\033[%sm%s\033[0m\n' "$code" "$*"
  else
    printf '%s\n' "$*"
  fi
}
ok()   { color "32" "PASS  $*"; PASS=$((PASS + 1)); }
bad()  { color "31" "FAIL  $*"; FAIL=$((FAIL + 1)); }
info() { color "36" "----  $*"; }

cleanup() {
  local pid
  for pid in "${PIDS[@]}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
  if [[ "${FAIL}" -ne 0 || "${KEEP_LOG:-0}" = "1" ]]; then
    info "logs kept in ${WORK_DIR}"
  else
    rm -rf "${WORK_DIR}"
  fi
}
trap cleanup EXIT INT TERM

require() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 2; }
}
require cargo
require curl
require jq
require python3

sign() {
  "${SIGN_BINARY_PATH}" "$@"
}

rpc_post() {
  local url="$1"
  curl -sS -H 'content-type: application/json' --data-binary @- "${url}"
}

rpc_post_operator() {
  local url="$1"
  curl -sS \
    -H 'content-type: application/json' \
    -H "authorization: Bearer ${OPERATOR_TOKEN}" \
    --data-binary @- "${url}"
}

wait_for_ready() {
  local url="$1"
  local label="$2"
  info "waiting for ${label} on ${url}"
  local deadline=$(( $(date +%s) + 20 ))
  while (( $(date +%s) < deadline )); do
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

bootstrap_round() {
  local url="$1"
  curl -sS "${url%/}/network/bootstrap" | jq -r '.current_round'
}

policy_nonce() {
  local url="$1"
  local resp
  resp=$(jq -nc --argjson policy_id "${POLICY_ID}" \
    '{jsonrpc:"2.0",method:"kombat_getPolicy",params:{policy_id:$policy_id},id:3}' | rpc_post "${url}")
  printf '%s' "${resp}" | jq -r '.result.next_intent_nonce'
}

assert_policy_nonce() {
  local url="$1"
  local label="$2"
  local expected="$3"
  local nonce
  nonce=$(policy_nonce "${url}")
  if [[ "${nonce}" == "${expected}" ]]; then
    ok "${label} policy nonce is ${expected}"
  else
    bad "${label} policy nonce mismatch: expected=${expected} actual=${nonce}"
    return 1
  fi
}

assert_order_book() {
  local url="$1"
  local label="$2"
  local resp
  resp=$(jq -nc --argjson market_id "${MARKET_ID}" \
    '{jsonrpc:"2.0",method:"kombat_getOrderBook",params:{market_id:$market_id,depth:10},id:4}' \
    | rpc_post "${url}")
  if printf '%s' "${resp}" | jq -e '.result.asks[0].price_ticks == 103 and .result.asks[0].total_lots == 5' >/dev/null 2>&1; then
    ok "${label} order book reflects committed resting order"
  else
    bad "${label} order book mismatch: ${resp}"
    return 1
  fi
}

assert_order_book_depth() {
  local url="$1"
  local label="$2"
  local expected_levels="$3"
  local resp
  resp=$(jq -nc --argjson market_id "${MARKET_ID}" \
    '{jsonrpc:"2.0",method:"kombat_getOrderBook",params:{market_id:$market_id,depth:10},id:44}' \
    | rpc_post "${url}")
  if printf '%s' "${resp}" | jq -e --argjson expected "${expected_levels}" '.result.asks | length == $expected' >/dev/null 2>&1; then
    ok "${label} order book depth is ${expected_levels}"
  else
    bad "${label} order book depth mismatch: ${resp}"
    return 1
  fi
}

assert_same_head() {
  local expected_height="$1"
  local expected_hash="$2"
  local url="$3"
  local label="$4"
  read -r height hash < <(chain_info_pair "${url}")
  if [[ "${height}" == "${expected_height}" && "${hash}" == "${expected_hash}" ]]; then
    ok "${label} head matches ${expected_height}/${expected_hash}"
  else
    bad "${label} head mismatch: expected=${expected_height}/${expected_hash} actual=${height}/${hash}"
    return 1
  fi
}

create_validator_set() {
  : > "${VALIDATOR_SET_PATH}"
  cat >> "${VALIDATOR_SET_PATH}" <<EOF
epoch = 1

EOF
  local i
  for i in "${!PORTS[@]}"; do
    cat >> "${VALIDATOR_SET_PATH}" <<EOF
[[validators]]
validator_id = "0x${VALIDATOR_IDS[$i]}"
consensus_pubkey = "0x${CONSENSUS_PUBKEYS[$i]}"
network_pubkey = "0x${NETWORK_PUBKEYS[$i]}"
rpc_endpoints = ["http://127.0.0.1:${PORTS[$i]}"]
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

start_validator() {
  local index="$1"
  local mode="${2:-auto}"
  local rpc_port="${PORTS[$index]}"
  local validator_id_hex="${VALIDATOR_IDS[$index]}"
  local consensus_seed_hex="${CONSENSUS_SEEDS[$index]}"
  local log_path="${LOG_PATHS[$index]}"
  local public_rpc_url="http://127.0.0.1:${rpc_port}"

  if [[ "${mode}" != "bootstrap" && "${index}" == "0" ]]; then
    info "starting validator $((index + 1)) fresh on port ${rpc_port}"
    KOMBAT_RPC_BIND="127.0.0.1:${rpc_port}" \
    KOMBAT_CHAIN_ID="${CHAIN_ID}" \
    KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
    KOMBAT_VALIDATOR_ID_HEX="0x${validator_id_hex}" \
    KOMBAT_CONSENSUS_SIGNING_KEY_HEX="0x${consensus_seed_hex}" \
    KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
    KOMBAT_PUBLIC_RPC_URL="${public_rpc_url}" \
    KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100 \
    "${BINARY_PATH}" >"${log_path}" 2>&1 &
  else
    info "starting validator $((index + 1)) from remote bootstrap on port ${rpc_port}"
    KOMBAT_RPC_BIND="127.0.0.1:${rpc_port}" \
    KOMBAT_CHAIN_ID="${CHAIN_ID}" \
    KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
    KOMBAT_VALIDATOR_ID_HEX="0x${validator_id_hex}" \
    KOMBAT_CONSENSUS_SIGNING_KEY_HEX="0x${consensus_seed_hex}" \
    KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
    KOMBAT_PUBLIC_RPC_URL="${public_rpc_url}" \
    KOMBAT_BOOTSTRAP_RPC_URL="http://127.0.0.1:${PORTS[0]}" \
    KOMBAT_BOOTSTRAP_BEARER_TOKEN="${OPERATOR_TOKEN}" \
    "${BINARY_PATH}" >"${log_path}" 2>&1 &
  fi
  PIDS[$index]=$!
}

stop_validator() {
  local index="$1"
  local pid="${PIDS[$index]}"
  local rpc_port="${PORTS[$index]}"
  if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
  fi
  local remaining_pids
  remaining_pids=$(lsof -ti tcp:"${rpc_port}" 2>/dev/null || true)
  if [[ -n "${remaining_pids}" ]]; then
    kill ${remaining_pids} 2>/dev/null || true
    sleep 0.2
    remaining_pids=$(lsof -ti tcp:"${rpc_port}" 2>/dev/null || true)
    if [[ -n "${remaining_pids}" ]]; then
      kill -9 ${remaining_pids} 2>/dev/null || true
    fi
  fi
  PIDS[$index]=""
}

create_account() {
  local url="$1"
  info "creating account ${ACCOUNT_ID} on source validator"
  local resp
  resp=$(sign create-account \
    --account-id "${ACCOUNT_ID}" \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --code-hash-hex 0909090909090909090909090909090909090909090909090909090909090909 \
    --timestamp-ms 1000 | rpc_post "${url}")
  if printf '%s' "${resp}" | jq -e '.result.id == '"${ACCOUNT_ID}"' and (.error | not)' >/dev/null 2>&1; then
    ok "source account created"
  else
    bad "create-account unexpected response: ${resp}"
    return 1
  fi
}

create_policy() {
  local url="$1"
  info "creating policy ${POLICY_ID} on source validator"
  local resp
  resp=$(sign create-policy \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --grantee-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --expires-at 50000 \
    --timestamp-ms 1050 \
    --max-intents-per-second 1000000 \
    --max-intents-per-day 1000000000 \
    --max-volume-per-day 1000000000000 \
    --gas-budget-per-day 1000000000000 \
    --gas-refill-per-day 1000000000000 \
    --max-drawdown-pct 10000 | rpc_post "${url}")
  if printf '%s' "${resp}" | jq -e '.result.accepted == true and .result.policy_id == '"${POLICY_ID}"'' >/dev/null 2>&1; then
    ok "source policy created"
  else
    bad "create-policy unexpected response: ${resp}"
    return 1
  fi
}

leader_index_for_round() {
  local round="$1"
  python3 - "$CHAIN_ID" "$round" <<'PY'
import sys
chain_id = int(sys.argv[1])
round_no = int(sys.argv[2])
total_stake = 400
value = ((chain_id << 32) | (chain_id >> 32)) & 0xFFFFFFFFFFFFFFFF
value ^= round_no
value ^= 0x9E3779B97F4A7C15
value ^= value >> 30
value = (value * 0xBF58476D1CE4E5B9) & 0xFFFFFFFFFFFFFFFF
value ^= value >> 27
value = (value * 0x94D049BB133111EB) & 0xFFFFFFFFFFFFFFFF
value ^= value >> 31
target = value % total_stake
print(target // 100)
PY
}

submit_resting_order_intent() {
  local url="$1"
  local nonce="$2"
  local client_order_id="$3"
  local price_ticks="$4"
  info "submitting resting order intent to round leader"
  local signed_json resp
  signed_json=$(sign place-order-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce "${nonce}" \
    --expires-at 10000 \
    --timestamp-ms 1110 \
    --market-id "${MARKET_ID}" \
    --client-order-id "${client_order_id}" \
    --side ask \
    --quantity-lots 5 \
    --order-kind limit \
    --price-ticks "${price_ticks}" \
    --tif gtc)
  resp=$(printf '%s' "${signed_json}" | rpc_post "${url}")
  if printf '%s' "${resp}" | jq -e '.result.intent_id_hex and (.error | not)' >/dev/null 2>&1; then
    ok "leader admitted resting order intent"
  else
    bad "submit intent unexpected response: ${resp}"
    return 1
  fi
}

run_network_round() {
  local url="$1"
  local expected_height="$2"
  local resp
  resp=$(jq -nc '{jsonrpc:"2.0",method:"kombat_runNetworkRound",params:{timestamp_ms:1200,max_items:10},id:50}' \
    | rpc_post_operator "${url}")
  if printf '%s' "${resp}" | jq -e --argjson expected_height "${expected_height}" '.result.height == $expected_height and .result.item_count == 1 and .result.signature_count == 4 and (.error | not)' >/dev/null 2>&1; then
    printf '%s\n' "${resp}"
  else
    bad "network round unexpected response: ${resp}"
    return 1
  fi
}

info "building kombat_chain and kombat_sign for network round smoke"
(cd "${ROOT_DIR}" && cargo build -p kombat-chain-rpc --bin kombat_chain --bin kombat_sign >/dev/null)

create_validator_set

start_validator 0
wait_for_ready "http://127.0.0.1:${PORTS[0]}" "validator 1"
create_account "http://127.0.0.1:${PORTS[0]}"
create_policy "http://127.0.0.1:${PORTS[0]}"
assert_policy_nonce "http://127.0.0.1:${PORTS[0]}" "validator 1" 1

for index in 1 2 3; do
  start_validator "${index}"
  wait_for_ready "http://127.0.0.1:${PORTS[$index]}" "validator $((index + 1))"
done

read -r source_height source_hash < <(chain_info_pair "http://127.0.0.1:${PORTS[0]}")
source_round=$(bootstrap_round "http://127.0.0.1:${PORTS[0]}")
if [[ "${source_height}" == "1" ]]; then
  ok "source validator head is bootstrapped at height 1"
else
  bad "source validator unexpected head height: ${source_height}"
  exit 1
fi

for index in 1 2 3; do
  assert_same_head "${source_height}" "${source_hash}" "http://127.0.0.1:${PORTS[$index]}" "validator $((index + 1))"
  assert_policy_nonce "http://127.0.0.1:${PORTS[$index]}" "validator $((index + 1))" 1
done

leader_index=$(leader_index_for_round "${source_round}")
leader_port="${PORTS[$leader_index]}"
leader_url="http://127.0.0.1:${leader_port}"
ok "round ${source_round} leader is validator $((leader_index + 1))"

submit_resting_order_intent "${leader_url}" 1 700 103
network_round_resp=$(run_network_round "${leader_url}" 2)
ok "network round committed block with full QC"
expected_height=$(printf '%s' "${network_round_resp}" | jq -r '.result.height')
expected_hash=$(printf '%s' "${network_round_resp}" | jq -r '.result.hash_hex')

for index in 0 1 2 3; do
  url="http://127.0.0.1:${PORTS[$index]}"
  assert_same_head "${expected_height}" "${expected_hash}" "${url}" "validator $((index + 1))"
  assert_policy_nonce "${url}" "validator $((index + 1))" 2
  assert_order_book "${url}" "validator $((index + 1))"
done

info "restarting validator 4 from a fresh remote snapshot"
stop_validator 3
start_validator 3 bootstrap
wait_for_ready "http://127.0.0.1:${PORTS[3]}" "validator 4 restarted"
assert_same_head "${expected_height}" "${expected_hash}" "http://127.0.0.1:${PORTS[3]}" "validator 4 restarted"
assert_policy_nonce "http://127.0.0.1:${PORTS[3]}" "validator 4 restarted" 2
assert_order_book "http://127.0.0.1:${PORTS[3]}" "validator 4 restarted"

submit_resting_order_intent "${leader_url}" 2 701 104
second_round_resp=$(run_network_round "${leader_url}" 3)
ok "second network round committed block with restarted peer present"
second_height=$(printf '%s' "${second_round_resp}" | jq -r '.result.height')
second_hash=$(printf '%s' "${second_round_resp}" | jq -r '.result.hash_hex')

for index in 0 1 2 3; do
  url="http://127.0.0.1:${PORTS[$index]}"
  assert_same_head "${second_height}" "${second_hash}" "${url}" "validator $((index + 1)) after second round"
  assert_policy_nonce "${url}" "validator $((index + 1)) after second round" 3
  assert_order_book_depth "${url}" "validator $((index + 1)) after second round" 2
done

info "summary: ${PASS} passed, ${FAIL} failed"
if [[ "${FAIL}" -ne 0 ]]; then
  exit 1
fi
