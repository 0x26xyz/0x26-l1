#!/usr/bin/env bash
# Live smoke-test harness for the kombat_chain RPC binary.
#
# Boots the RPC on a disposable local port, exercises the recent
# hardening paths (create account/policy, valid signed intent,
# snapshot save/load continuity, wrong-chain/tampered rejections for
# intents and owner calls, oversize HTTP body), and shuts the server
# down cleanly.
#
# Usage:
#   chain/scripts/smoke_rpc.sh              # run full smoke
#   PORT=19545 chain/scripts/smoke_rpc.sh   # override bind port
#   KEEP_LOG=1 chain/scripts/smoke_rpc.sh   # keep server log on success

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${PORT:-18545}"
RPC_URL="http://127.0.0.1:${PORT}"
OPERATOR_TOKEN="${OPERATOR_TOKEN:-smoke-operator-token}"
if [[ "${CARGO_TARGET_DIR:-}" == /* ]]; then
  BUILD_TARGET_DIR="${CARGO_TARGET_DIR}"
else
  BUILD_TARGET_DIR="${ROOT_DIR}/${CARGO_TARGET_DIR:-target}"
fi
KOMBAT_CHAIN_BIN="${BUILD_TARGET_DIR}/debug/kombat_chain"
KOMBAT_SIGN_BIN="${BUILD_TARGET_DIR}/debug/kombat_sign"

WORK_DIR="$(mktemp -d -t kombat-smoke.XXXXXX)"
SERVER_LOG="${WORK_DIR}/server.log"
SNAPSHOT_FILE="${WORK_DIR}/engine.snapshot"
SERVER_PID=""
SNAPSHOT_HEAD_HEIGHT=""
SNAPSHOT_HEAD_HASH=""
SNAPSHOT_EVENT_NEXT_SEQ=""
FIRST_INTENT_ID=""
RESTORED_INTENT_ID=""
RELOADED_INTENT_ID=""

ACCOUNT_ID=11
POLICY_ID=42
CHAIN_ID=7
MARKET_ID=7
OWNER_SECRET_HEX=0808080808080808080808080808080808080808080808080808080808080808
SESSION_SECRET_HEX=0909090909090909090909090909090909090909090909090909090909090909
BAD_VALIDATOR_ID_HEX=ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

PASS=0
FAIL=0

color() {
  local code="$1"; shift
  if [[ -t 1 ]]; then printf '\033[%sm%s\033[0m\n' "$code" "$*"; else printf '%s\n' "$*"; fi
}
ok()   { color "32" "PASS  $*"; PASS=$((PASS+1)); }
bad()  { color "31" "FAIL  $*"; FAIL=$((FAIL+1)); }
info() { color "36" "----  $*"; }

cleanup() {
  if [[ -n "${SERVER_PID}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    stop_server
  fi
  if [[ "${FAIL}" -ne 0 || "${KEEP_LOG:-0}" = "1" ]]; then
    info "server log kept at: ${SERVER_LOG}"
  else
    rm -rf "${WORK_DIR}"
  fi
}
trap cleanup EXIT INT TERM

require() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1" >&2; exit 2; }
}
require curl
require jq
require cargo
require nc

build_binaries() {
  info "building kombat_chain smoke binaries"
  (
    cd "${ROOT_DIR}"
    cargo build --quiet -p kombat-chain-rpc --bin kombat_chain --bin kombat_sign
  )
}

sign() {
  "${KOMBAT_SIGN_BIN}" "$@"
}

rpc_post() {
  curl -sS -H 'content-type: application/json' --data-binary @- "${RPC_URL}"
}

rpc_post_operator() {
  curl -sS \
    -H 'content-type: application/json' \
    -H "authorization: Bearer ${OPERATOR_TOKEN}" \
    --data-binary @- "${RPC_URL}"
}

sse_get() {
  local path="$1"
  shift || true
  curl -sS "$@" "${RPC_URL}${path}"
}

wait_for_ready() {
  info "waiting for RPC to accept connections on port ${PORT}"
  local deadline=$(( $(date +%s) + 30 ))
  while (( $(date +%s) < deadline )); do
    local resp
    resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}' \
      | curl -sS -H 'content-type: application/json' --data-binary @- "${RPC_URL}" 2>/dev/null || true)
    if printf '%s' "${resp}" | jq -e '.result.chain_id' >/dev/null 2>&1; then
      ok "server reachable ($(printf '%s' "${resp}" | jq -rc '.result'))"
      return 0
    fi
    sleep 0.3
  done
  bad "server never answered kombat_chainInfo"
  tail -n 40 "${SERVER_LOG}" >&2 || true
  return 1
}

wait_for_policy() {
  info "verifying policy ${POLICY_ID} is present in committed state"
  local deadline=$(( $(date +%s) + 10 ))
  while (( $(date +%s) < deadline )); do
    local resp
    resp=$(jq -nc --argjson policy_id "${POLICY_ID}" \
      '{jsonrpc:"2.0",method:"kombat_getPolicy",params:{policy_id:$policy_id},id:3}' | rpc_post)
    if printf '%s' "${resp}" | jq -e '.result.id == '"${POLICY_ID}"'' >/dev/null 2>&1; then
      ok "policy ${POLICY_ID} active: $(printf '%s' "${resp}" | jq -rc '.result | {id, account, state}')"
      return 0
    fi
    sleep 0.2
  done
  bad "policy ${POLICY_ID} not returned by kombat_getPolicy"
  return 1
}

submit_resting_order() {
  info "submitting direct resting order client_order_id=700"
  local resp
  resp=$(jq -nc \
    --argjson market_id "${MARKET_ID}" \
    --argjson account_id "${ACCOUNT_ID}" \
    --argjson client_order_id 700 \
    '{jsonrpc:"2.0",method:"kombat_submitOrder",params:{order:{
      market_id:$market_id,
      account_id:$account_id,
      client_order_id:$client_order_id,
      side:"ask",
      quantity_lots:5,
      order_kind:"limit",
      price_ticks:103,
      time_in_force:{tif:"gtc"},
      post_only:false
    },timestamp_ms:1110},id:96}' | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.status == "resting" and .result.posted_order_id == 700 and (.error | not)' >/dev/null 2>&1; then
    ok "resting order accepted: $(printf '%s' "${resp}" | jq -rc '.result | {status, posted_order_id, remaining_lots}')"
  else
    bad "resting order unexpected response: ${resp}"
    return 1
  fi
}

assert_open_order_present() {
  local context="$1"
  info "verifying resting order survives ${context}"
  local resp
  resp=$(jq -nc --argjson market_id "${MARKET_ID}" --argjson account_id "${ACCOUNT_ID}" \
    '{jsonrpc:"2.0",method:"kombat_getOpenOrders",params:{market_id:$market_id,account_id:$account_id,limit:10},id:95}' \
    | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result | length == 1 and .[0].client_order_id == 700 and .[0].remaining_lots == 5 and .[0].side == "ask"' >/dev/null 2>&1; then
    ok "resting order present after ${context}: $(printf '%s' "${resp}" | jq -rc '.result[0] | {client_order_id, side, remaining_lots, price_ticks}')"
  else
    bad "open-order state unexpected after ${context}: ${resp}"
    return 1
  fi
}

cancel_resting_order() {
  local context="$1"
  info "cancelling resting order after ${context}"
  local resp
  resp=$(jq -nc --argjson market_id "${MARKET_ID}" --argjson account_id "${ACCOUNT_ID}" \
    '{jsonrpc:"2.0",method:"kombat_cancelOrder",params:{market_id:$market_id,account_id:$account_id,client_order_id:700,timestamp_ms:1450},id:94}' \
    | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.cancelled == true and .result.order.client_order_id == 700 and (.error | not)' >/dev/null 2>&1; then
    ok "resting order cancelled after ${context}: $(printf '%s' "${resp}" | jq -rc '.result | {cancelled, order: {client_order_id: .order.client_order_id, price_ticks: .order.price_ticks}}')"
  else
    bad "cancel order unexpected response after ${context}: ${resp}"
    return 1
  fi
}

wait_for_policy_nonce() {
  local expected_nonce="$1"
  info "waiting for policy ${POLICY_ID} next_intent_nonce=${expected_nonce}"
  local deadline=$(( $(date +%s) + 10 ))
  while (( $(date +%s) < deadline )); do
    local resp
    resp=$(jq -nc --argjson policy_id "${POLICY_ID}" \
      '{jsonrpc:"2.0",method:"kombat_getPolicy",params:{policy_id:$policy_id},id:97}' | rpc_post)
    if printf '%s' "${resp}" | jq -e --argjson expected "${expected_nonce}" '.result.next_intent_nonce == $expected' >/dev/null 2>&1; then
      ok "policy nonce advanced: $(printf '%s' "${resp}" | jq -rc '.result | {id, next_intent_nonce, state}')"
      return 0
    fi
    sleep 0.2
  done
  bad "policy ${POLICY_ID} never reached next_intent_nonce=${expected_nonce}"
  return 1
}

chain_info() {
  printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":99}' | rpc_post
}

current_head_pair() {
  local resp
  resp=$(chain_info)
  if printf '%s' "${resp}" | jq -e '.result.head_height and .result.head_hash_hex and (.error | not)' >/dev/null 2>&1; then
    printf '%s %s\n' \
      "$(printf '%s' "${resp}" | jq -r '.result.head_height')" \
      "$(printf '%s' "${resp}" | jq -r '.result.head_hash_hex')"
  else
    return 1
  fi
}

wait_for_intent_executed() {
  local intent_id_hex="$1"
  info "waiting for intent ${intent_id_hex} to execute"
  local deadline=$(( $(date +%s) + 10 ))
  while (( $(date +%s) < deadline )); do
    local resp
    resp=$(jq -nc --arg intent_id_hex "${intent_id_hex}" \
      '{jsonrpc:"2.0",method:"kombat_getIntentStatus",params:{intent_id_hex:$intent_id_hex},id:98}' \
      | rpc_post)
    if printf '%s' "${resp}" | jq -e '.result.stage == "executed"' >/dev/null 2>&1; then
      ok "intent executed: $(printf '%s' "${resp}" | jq -rc '.result | {intent_id_hex, stage, block_height, timestamp_ms}')"
      return 0
    fi
    sleep 0.2
  done
  bad "intent ${intent_id_hex} never reached executed stage"
  return 1
}

assert_head_matches_snapshot() {
  info "verifying restarted head matches saved snapshot head"
  local resp head_height head_hash
  resp=$(chain_info)
  head_height=$(printf '%s' "${resp}" | jq -r '.result.head_height // empty')
  head_hash=$(printf '%s' "${resp}" | jq -r '.result.head_hash_hex // empty')
  if [[ "${head_height}" == "${SNAPSHOT_HEAD_HEIGHT}" && "${head_hash}" == "${SNAPSHOT_HEAD_HASH}" ]]; then
    ok "restarted head matches snapshot: height=${head_height} hash=${head_hash}"
  else
    bad "restarted head mismatch: ${resp}"
    return 1
  fi
}

event_next_seq() {
  jq -nc '{jsonrpc:"2.0",method:"kombat_listEvents",params:{from_seq:0,limit:1000},id:93}' | rpc_post
}

capture_snapshot_event_cursor() {
  info "capturing event cursor at snapshot boundary"
  local resp
  resp=$(event_next_seq)
  SNAPSHOT_EVENT_NEXT_SEQ=$(printf '%s' "${resp}" | jq -r '.result.next_cursor.next_seq // empty')
  if [[ "${SNAPSHOT_EVENT_NEXT_SEQ}" =~ ^[0-9]+$ ]]; then
    ok "captured snapshot event cursor next_seq=${SNAPSHOT_EVENT_NEXT_SEQ}"
  else
    bad "failed to capture snapshot event cursor: ${resp}"
    return 1
  fi
}

assert_event_cursor_equals_snapshot() {
  local context="$1"
  info "verifying event cursor matches snapshot after ${context}"
  local resp current_next
  resp=$(event_next_seq)
  current_next=$(printf '%s' "${resp}" | jq -r '.result.next_cursor.next_seq // empty')
  if [[ "${current_next}" == "${SNAPSHOT_EVENT_NEXT_SEQ}" ]]; then
    ok "event cursor matches snapshot after ${context}: next_seq=${current_next}"
  else
    bad "event cursor mismatch after ${context}: ${resp}"
    return 1
  fi
}

assert_event_cursor_advanced_after_snapshot() {
  local context="$1"
  info "verifying event cursor advanced after ${context}"
  local resp current_next
  resp=$(event_next_seq)
  current_next=$(printf '%s' "${resp}" | jq -r '.result.next_cursor.next_seq // empty')
  if [[ "${current_next}" =~ ^[0-9]+$ ]] && (( current_next > SNAPSHOT_EVENT_NEXT_SEQ )); then
    ok "event cursor advanced after ${context}: next_seq=${current_next}"
  else
    bad "event cursor did not advance after ${context}: ${resp}"
    return 1
  fi
}

load_snapshot_via_rpc() {
  info "loading operator snapshot from ${SNAPSHOT_FILE} via kombat_loadSnapshot"
  local resp
  resp=$(jq -nc --arg path "${SNAPSHOT_FILE}" \
    '{jsonrpc:"2.0",method:"kombat_loadSnapshot",params:{path:$path},id:5}' \
    | rpc_post_operator)
  if printf '%s' "${resp}" | jq -e --arg path "${SNAPSHOT_FILE}" --argjson head_height "${SNAPSHOT_HEAD_HEIGHT}" --arg head_hash "${SNAPSHOT_HEAD_HASH}" \
    '.result.path == $path and .result.head_height == $head_height and .result.head_hash_hex == $head_hash and (.error | not)' >/dev/null 2>&1; then
    ok "snapshot loaded via RPC: $(printf '%s' "${resp}" | jq -rc '.result | {path, head_height, head_hash_hex}')"
  else
    bad "load snapshot unexpected response: ${resp}"
    return 1
  fi
  assert_head_matches_snapshot
}

assert_failed_operator_load_preserves_state() {
  local context="$1"
  local path="$2"
  local expected_fragment="$3"
  local before_height before_hash before_next_seq
  local after_height after_hash after_next_seq
  local resp

  info "verifying operator snapshot load rejects ${context}"
  read -r before_height before_hash <<<"$(current_head_pair)"
  before_next_seq=$(event_next_seq | jq -r '.result.next_cursor.next_seq // empty')

  resp=$(jq -nc --arg path "${path}" \
    '{jsonrpc:"2.0",method:"kombat_loadSnapshot",params:{path:$path},id:88}' \
    | rpc_post_operator)
  if printf '%s' "${resp}" | jq -e --arg fragment "${expected_fragment}" \
    '.error.code == -32000 and (.error.message | contains($fragment))' >/dev/null 2>&1; then
    ok "operator snapshot load rejects ${context}: ${expected_fragment}"
  else
    bad "operator snapshot load wrong response for ${context}: ${resp}"
    return 1
  fi

  read -r after_height after_hash <<<"$(current_head_pair)"
  after_next_seq=$(event_next_seq | jq -r '.result.next_cursor.next_seq // empty')
  if [[ "${after_height}" == "${before_height}" && "${after_hash}" == "${before_hash}" && "${after_next_seq}" == "${before_next_seq}" ]]; then
    ok "failed operator load preserved live state for ${context}"
  else
    bad "failed operator load mutated live state for ${context}"
    return 1
  fi
}

verify_event_stream() {
  info "verifying SSE event stream for policy_created and Last-Event-ID resume"
  local stream last_id cursor_next
  stream=$(sse_get "/events?limit=10&policy_id=${POLICY_ID}&topic=policy_created")
  last_id=$(printf '%s\n' "${stream}" | awk '/^id: /{print $2; exit}')
  cursor_next=$(printf '%s\n' "${stream}" \
    | awk 'BEGIN{cursor=0} /^event: cursor$/{cursor=1; next} cursor && /^data: /{sub(/^data: /,""); print; exit}' \
    | jq -r '.next_seq // empty')

  if [[ -n "${last_id}" ]] \
    && [[ "${cursor_next}" =~ ^[0-9]+$ ]] \
    && grep -q '^event: policy_created$' <<<"${stream}" \
    && grep -q "\"policy_id\":${POLICY_ID}" <<<"${stream}"; then
    ok "event stream returned policy_created with cursor next_seq=${cursor_next}"
  else
    bad "event stream missing expected policy_created payload: ${stream}"
    return 1
  fi

  local resumed expected_next
  resumed=$(sse_get "/events?limit=10&policy_id=${POLICY_ID}&topic=policy_created" \
    -H "Last-Event-ID: ${last_id}")
  expected_next=$((last_id + 1))

  if ! grep -q '^event: policy_created$' <<<"${resumed}" \
    && grep -q '^event: cursor$' <<<"${resumed}" \
    && grep -q "\"next_seq\":${expected_next}" <<<"${resumed}"; then
    ok "event stream resumes from Last-Event-ID=${last_id} without replay"
  else
    bad "event stream resume returned unexpected payload: ${resumed}"
    return 1
  fi
}

start_server() {
  local snapshot_path="${1:-}"
  local chain_id_override="${2:-}"
  local validator_id_override="${3:-}"
  local -a command_env=(
    "KOMBAT_RPC_BIND=127.0.0.1:${PORT}"
    "KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS=100"
    "KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS=10"
    "KOMBAT_OPERATOR_TOKEN=${OPERATOR_TOKEN}"
  )
  if [[ -n "${snapshot_path}" ]]; then
    info "starting kombat_chain on ${RPC_URL} from snapshot ${snapshot_path} (log: ${SERVER_LOG})"
    command_env+=("KOMBAT_LOAD_SNAPSHOT_PATH=${snapshot_path}")
    if [[ -n "${chain_id_override}" ]]; then
      command_env+=("KOMBAT_CHAIN_ID=${chain_id_override}")
    fi
    if [[ -n "${validator_id_override}" ]]; then
      command_env+=("KOMBAT_VALIDATOR_ID_HEX=${validator_id_override}")
    fi
  else
    info "starting kombat_chain on ${RPC_URL} (log: ${SERVER_LOG})"
    command_env+=("KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100")
  fi
  (
    cd "${ROOT_DIR}"
    env "${command_env[@]}" \
      "${KOMBAT_CHAIN_BIN}" \
      >"${SERVER_LOG}" 2>&1
  ) &
  SERVER_PID=$!
}

assert_snapshot_bootstrap_rejected() {
  local context="$1"
  local expected_fragment="$2"
  local snapshot_path="${3:-${SNAPSHOT_FILE}}"
  local chain_id_override="${4:-}"
  local validator_id_override="${5:-}"
  local status=0

  info "verifying snapshot bootstrap rejects ${context}"
  start_server "${snapshot_path}" "${chain_id_override}" "${validator_id_override}"

  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
    if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
      break
    fi
    sleep 0.2
  done

  if kill -0 "${SERVER_PID}" 2>/dev/null; then
    bad "snapshot bootstrap for ${context} did not fail"
    stop_server
    return 1
  fi

  wait "${SERVER_PID}" || status=$?
  SERVER_PID=""

  if [[ "${status}" -eq 0 ]]; then
    bad "snapshot bootstrap for ${context} exited successfully"
    return 1
  fi

  if grep -q "${expected_fragment}" "${SERVER_LOG}"; then
    ok "snapshot bootstrap rejects ${context}: ${expected_fragment}"
  else
    bad "snapshot bootstrap for ${context} logged unexpected failure"
    return 1
  fi
}

create_truncated_snapshot() {
  local target="${WORK_DIR}/engine.truncated.snapshot"
  cp "${SNAPSHOT_FILE}" "${target}"
  truncate -s 32 "${target}"
  printf '%s\n' "${target}"
}

stop_server() {
  info "shutting down server pid=${SERVER_PID}"
  kill "${SERVER_PID}" 2>/dev/null || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    kill -0 "${SERVER_PID}" 2>/dev/null || break
    sleep 0.2
  done
  kill -9 "${SERVER_PID}" 2>/dev/null || true
  wait "${SERVER_PID}" 2>/dev/null || true
  SERVER_PID=""
}

create_account() {
  info "submitting kombat_createAccount for account_id=${ACCOUNT_ID}"
  local resp
  resp=$(sign create-account \
    --account-id "${ACCOUNT_ID}" \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --code-hash-hex 0909090909090909090909090909090909090909090909090909090909090909 \
    --timestamp-ms 1000 | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.id == '"${ACCOUNT_ID}"' and (.error | not)' >/dev/null 2>&1; then
    ok "account created: $(printf '%s' "${resp}" | jq -rc '.result | {id, owner_hex, nonce}')"
  else
    bad "create-account unexpected response: ${resp}"
    return 1
  fi
}

create_policy() {
  info "submitting owner-call createPolicy for policy_id=${POLICY_ID}"
  local resp
  resp=$(sign create-policy \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --grantee-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --expires-at 50000 \
    --timestamp-ms 1050 | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.accepted == true and .result.policy_id == '"${POLICY_ID}"'' >/dev/null 2>&1; then
    ok "policy create accepted: $(printf '%s' "${resp}" | jq -rc '.result')"
  else
    bad "create-policy unexpected response: ${resp}"
    return 1
  fi
}

submit_valid_intent() {
  info "submitting VALID signed cancel intent (nonce=1)"
  local signed_json
  signed_json=$(sign cancel-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce 1 \
    --expires-at 10000 \
    --timestamp-ms 1100 \
    --order-id 7)
  local resp
  resp=$(printf '%s' "${signed_json}" | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.intent_id_hex and (.error | not)' >/dev/null 2>&1; then
    FIRST_INTENT_ID=$(printf '%s' "${resp}" | jq -r '.result.intent_id_hex')
    ok "valid intent accepted: $(printf '%s' "${resp}" | jq -rc '.result | {intent_id_hex, class, pending_reason}')"
  else
    bad "valid intent unexpectedly rejected: ${resp}"
    return 1
  fi
}

save_snapshot() {
  info "saving operator snapshot to ${SNAPSHOT_FILE}"
  local resp
  resp=$(jq -nc --arg path "${SNAPSHOT_FILE}" \
    '{jsonrpc:"2.0",method:"kombat_saveSnapshot",params:{path:$path},id:4}' \
    | rpc_post_operator)
  if printf '%s' "${resp}" | jq -e --arg path "${SNAPSHOT_FILE}" \
    '.result.path == $path and .result.head_height >= 1 and .result.snapshot_size_bytes > 0 and (.error | not)' >/dev/null 2>&1; then
    SNAPSHOT_HEAD_HEIGHT=$(printf '%s' "${resp}" | jq -r '.result.head_height')
    SNAPSHOT_HEAD_HASH=$(printf '%s' "${resp}" | jq -r '.result.head_hash_hex')
    ok "snapshot saved: $(printf '%s' "${resp}" | jq -rc '.result | {path, head_height, snapshot_size_bytes}')"
  else
    bad "save snapshot unexpected response: ${resp}"
    return 1
  fi
}

restart_from_snapshot() {
  stop_server
  start_server "${SNAPSHOT_FILE}"
  wait_for_ready
  wait_for_policy
  assert_head_matches_snapshot
}

submit_restored_valid_intent() {
  info "submitting restored VALID signed cancel intent (nonce=2 after load)"
  local signed_json resp
  signed_json=$(sign cancel-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce 2 \
    --expires-at 30000 \
    --timestamp-ms 1275 \
    --order-id 13)
  resp=$(printf '%s' "${signed_json}" | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.intent_id_hex and (.error | not)' >/dev/null 2>&1; then
    RESTORED_INTENT_ID=$(printf '%s' "${resp}" | jq -r '.result.intent_id_hex')
    ok "restored valid intent accepted after load: $(printf '%s' "${resp}" | jq -rc '.result | {intent_id_hex, class, pending_reason}')"
  else
    bad "restored valid intent unexpectedly rejected after load: ${resp}"
    return 1
  fi
}

submit_reloaded_valid_intent() {
  info "submitting reloaded VALID signed cancel intent (nonce=2 after operator load)"
  local signed_json resp
  signed_json=$(sign cancel-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce 2 \
    --expires-at 35000 \
    --timestamp-ms 1350 \
    --order-id 14)
  resp=$(printf '%s' "${signed_json}" | rpc_post)
  if printf '%s' "${resp}" | jq -e '.result.intent_id_hex and (.error | not)' >/dev/null 2>&1; then
    RELOADED_INTENT_ID=$(printf '%s' "${resp}" | jq -r '.result.intent_id_hex')
    ok "reloaded valid intent accepted after operator load: $(printf '%s' "${resp}" | jq -rc '.result | {intent_id_hex, class, pending_reason}')"
  else
    bad "reloaded valid intent unexpectedly rejected after operator load: ${resp}"
    return 1
  fi
}

submit_wrong_chain_intent() {
  info "submitting WRONG-CHAIN signed cancel intent (nonce=2, chain_id=8)"
  local signed_json wrong_chain_json resp reason expected actual
  signed_json=$(sign cancel-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --chain-id 8 \
    --nonce 2 \
    --expires-at 10000 \
    --timestamp-ms 1150 \
    --order-id 9)
  wrong_chain_json=$(printf '%s' "${signed_json}" | jq -c '.')

  resp=$(printf '%s' "${wrong_chain_json}" | rpc_post)
  reason=$(printf '%s' "${resp}" | jq -rc '.error.data.reason // empty')
  expected=$(printf '%s' "${resp}" | jq -rc '.error.data.expected_chain_id // empty')
  actual=$(printf '%s' "${resp}" | jq -rc '.error.data.actual_chain_id // empty')
  if [[ "${reason}" == "session_chain_mismatch" && "${expected}" == "${CHAIN_ID}" && "${actual}" == "8" ]]; then
    ok "wrong-chain intent correctly rejected with reason=${reason} expected=${expected} actual=${actual}"
  else
    bad "wrong-chain intent wrong response: ${resp}"
    return 1
  fi
}

submit_tampered_intent() {
  info "submitting TAMPERED signed cancel intent (nonce=2, mutated order_id after signing)"
  local signed_json tampered_json resp reason
  signed_json=$(sign cancel-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce 2 \
    --expires-at 10000 \
    --timestamp-ms 1200 \
    --order-id 10)
  tampered_json=$(printf '%s' "${signed_json}" | jq -c '
    .params.signed_intent.intent.order_id = 11')

  resp=$(printf '%s' "${tampered_json}" | rpc_post)
  reason=$(printf '%s' "${resp}" | jq -rc '.error.data.reason // empty')
  if [[ "${reason}" == "invalid_session_signature" ]]; then
    ok "tampered intent correctly rejected with reason=${reason}"
  else
    bad "tampered intent wrong response: ${resp}"
    return 1
  fi
}

submit_wrong_chain_owner_call() {
  info "submitting WRONG-CHAIN signed pause owner call (nonce=1, chain_id=8)"
  local signed_json resp reason expected actual
  signed_json=$(sign pause-account \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --chain-id 8 \
    --nonce 1 \
    --timestamp-ms 1250)

  resp=$(printf '%s' "${signed_json}" | rpc_post)
  reason=$(printf '%s' "${resp}" | jq -rc '.error.data.reason // empty')
  expected=$(printf '%s' "${resp}" | jq -rc '.error.data.expected_chain_id // empty')
  actual=$(printf '%s' "${resp}" | jq -rc '.error.data.actual_chain_id // empty')
  if [[ "${reason}" == "chain_mismatch" && "${expected}" == "${CHAIN_ID}" && "${actual}" == "8" ]]; then
    ok "wrong-chain owner call correctly rejected with reason=${reason} expected=${expected} actual=${actual}"
  else
    bad "wrong-chain owner call wrong response: ${resp}"
    return 1
  fi
}

submit_tampered_owner_call() {
  info "submitting TAMPERED signed pause owner call (nonce=1, mutated action after signing)"
  local signed_json tampered_json resp reason
  signed_json=$(sign pause-account \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce 1 \
    --timestamp-ms 1300)
  tampered_json=$(printf '%s' "${signed_json}" | jq -c '
    .params.signed_owner_call.call.kind = "unpause"')

  resp=$(printf '%s' "${tampered_json}" | rpc_post)
  reason=$(printf '%s' "${resp}" | jq -rc '.error.data.reason // empty')
  if [[ "${reason}" == "invalid_owner_signature" ]]; then
    ok "tampered owner call correctly rejected with reason=${reason}"
  else
    bad "tampered owner call wrong response: ${resp}"
    return 1
  fi
}

submit_oversize_payload() {
  local status
  info "submitting oversized Content-Length header (expect HTTP 413)"
  status=$(
    printf \
      'POST / HTTP/1.1\r\nHost: 127.0.0.1:%s\r\nContent-Type: application/json\r\nContent-Length: %d\r\nConnection: close\r\n\r\n' \
      "${PORT}" \
      $((1024 * 1024 + 1)) \
      | nc 127.0.0.1 "${PORT}" \
      | awk 'NR==1 {print $2; exit}'
  )
  if [[ -z "${status}" ]]; then
    status="000"
  fi
  if [[ "${status}" == "413" || "${status}" == "400" ]]; then
    ok "oversize request rejected with HTTP ${status}"
  else
    bad "expected oversize request rejection, got HTTP ${status}"
    return 1
  fi
}

# ---- run ------------------------------------------------------------

build_binaries
start_server
wait_for_ready

create_account
create_policy
wait_for_policy
verify_event_stream

submit_valid_intent
wait_for_intent_executed "${FIRST_INTENT_ID}"
wait_for_policy_nonce 2
submit_resting_order
assert_open_order_present "snapshot save preparation"
capture_snapshot_event_cursor
save_snapshot
submit_wrong_chain_intent
submit_tampered_intent
restart_from_snapshot
wait_for_policy_nonce 2
assert_open_order_present "process restart"
assert_event_cursor_equals_snapshot "process restart"
submit_restored_valid_intent
wait_for_intent_executed "${RESTORED_INTENT_ID}"
wait_for_policy_nonce 3
assert_event_cursor_advanced_after_snapshot "restored intent execution"
load_snapshot_via_rpc
wait_for_policy_nonce 2
assert_open_order_present "operator snapshot load"
assert_event_cursor_equals_snapshot "operator snapshot load"
submit_reloaded_valid_intent
wait_for_intent_executed "${RELOADED_INTENT_ID}"
wait_for_policy_nonce 3
assert_event_cursor_advanced_after_snapshot "reloaded intent execution"
assert_failed_operator_load_preserves_state "missing snapshot path" \
  "${WORK_DIR}/missing.snapshot" \
  "failed to inspect snapshot"
assert_failed_operator_load_preserves_state "truncated snapshot file" \
  "$(create_truncated_snapshot)" \
  "failed to load snapshot"
cancel_resting_order "operator snapshot load"
submit_wrong_chain_owner_call
submit_tampered_owner_call
submit_oversize_payload
stop_server
assert_snapshot_bootstrap_rejected "validator mismatch" \
  "snapshot validator set does not include configured validator_id" \
  "${SNAPSHOT_FILE}" "" "${BAD_VALIDATOR_ID_HEX}"
assert_snapshot_bootstrap_rejected "chain mismatch" \
  "snapshot chain_id does not match configured chain_id" \
  "${SNAPSHOT_FILE}" "8" ""
assert_snapshot_bootstrap_rejected "missing snapshot path" \
  "failed to load snapshot" \
  "${WORK_DIR}/missing.snapshot" "" ""
assert_snapshot_bootstrap_rejected "truncated snapshot file" \
  "failed to load snapshot" \
  "$(create_truncated_snapshot)" "" ""

info "summary: ${PASS} passed, ${FAIL} failed"
if [[ "${FAIL}" -ne 0 ]]; then
  exit 1
fi
