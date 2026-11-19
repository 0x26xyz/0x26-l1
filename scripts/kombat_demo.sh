#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"

OWNER_SECRET_HEX="${OWNER_SECRET_HEX:-0808080808080808080808080808080808080808080808080808080808080808}"
SESSION_SECRET_HEX="${SESSION_SECRET_HEX:-0909090909090909090909090909090909090909090909090909090909090909}"
CHILD_GRANTEE_SECRET_HEX="${CHILD_GRANTEE_SECRET_HEX:-0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a}"
NEW_OWNER_SECRET_HEX="${NEW_OWNER_SECRET_HEX:-0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b}"

ACCOUNT_ID="${ACCOUNT_ID:-11}"
POLICY_ID="${POLICY_ID:-42}"
CHILD_POLICY_ID="${CHILD_POLICY_ID:-77}"
EXTRA_POLICY_ID="${EXTRA_POLICY_ID:-88}"

CREATE_ACCOUNT_TIMESTAMP_MS="${CREATE_ACCOUNT_TIMESTAMP_MS:-1000}"
CREATE_POLICY_TIMESTAMP_MS="${CREATE_POLICY_TIMESTAMP_MS:-1050}"
CANCEL_INTENT_TIMESTAMP_MS="${CANCEL_INTENT_TIMESTAMP_MS:-1100}"
TRANSFER_INTENT_TIMESTAMP_MS="${TRANSFER_INTENT_TIMESTAMP_MS:-1150}"
SUB_POLICY_TIMESTAMP_MS="${SUB_POLICY_TIMESTAMP_MS:-1200}"
REVOKE_POLICY_TIMESTAMP_MS="${REVOKE_POLICY_TIMESTAMP_MS:-1250}"
PROPOSE_OWNER_TIMESTAMP_MS="${PROPOSE_OWNER_TIMESTAMP_MS:-1300}"
ACCEPT_OWNER_TIMESTAMP_MS="${ACCEPT_OWNER_TIMESTAMP_MS:-1350}"
PAUSE_ACCOUNT_TIMESTAMP_MS="${PAUSE_ACCOUNT_TIMESTAMP_MS:-1400}"
UNPAUSE_ACCOUNT_TIMESTAMP_MS="${UNPAUSE_ACCOUNT_TIMESTAMP_MS:-1450}"
ADD_POLICY_TIMESTAMP_MS="${ADD_POLICY_TIMESTAMP_MS:-1500}"
REMOVE_POLICY_TIMESTAMP_MS="${REMOVE_POLICY_TIMESTAMP_MS:-1550}"
REVOKE_POLICY_OWNER_TIMESTAMP_MS="${REVOKE_POLICY_OWNER_TIMESTAMP_MS:-1600}"
UPGRADE_ACCOUNT_TIMESTAMP_MS="${UPGRADE_ACCOUNT_TIMESTAMP_MS:-1650}"

CANCEL_NONCE="${CANCEL_NONCE:-1}"
TRANSFER_NONCE="${TRANSFER_NONCE:-2}"
SUB_POLICY_NONCE="${SUB_POLICY_NONCE:-3}"
REVOKE_NONCE="${REVOKE_NONCE:-4}"
PROPOSE_OWNER_NONCE="${PROPOSE_OWNER_NONCE:-1}"
ACCEPT_OWNER_NONCE="${ACCEPT_OWNER_NONCE:-0}"
PAUSE_ACCOUNT_NONCE="${PAUSE_ACCOUNT_NONCE:-2}"
UNPAUSE_ACCOUNT_NONCE="${UNPAUSE_ACCOUNT_NONCE:-3}"
ADD_POLICY_NONCE="${ADD_POLICY_NONCE:-4}"
REMOVE_POLICY_NONCE="${REMOVE_POLICY_NONCE:-5}"
REVOKE_POLICY_OWNER_NONCE="${REVOKE_POLICY_OWNER_NONCE:-6}"
UPGRADE_ACCOUNT_NONCE="${UPGRADE_ACCOUNT_NONCE:-7}"

TIME_AFTER_MS="${TIME_AFTER_MS:-1300}"
ORDER_ID="${ORDER_ID:-7}"
TRANSFER_TO="${TRANSFER_TO:-17}"
TRANSFER_TOKEN="${TRANSFER_TOKEN:-1}"
TRANSFER_AMOUNT="${TRANSFER_AMOUNT:-250}"
UPGRADE_CODE_HASH_HEX="${UPGRADE_CODE_HASH_HEX:-0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c}"
INTENT_ID_HEX="${INTENT_ID_HEX:-}"
EVENT_LIMIT="${EVENT_LIMIT:-20}"
EVENT_ACCOUNT_ID="${EVENT_ACCOUNT_ID:-}"
EVENT_POLICY_ID="${EVENT_POLICY_ID:-}"
EVENT_TOPIC="${EVENT_TOPIC:-}"

usage() {
  cat <<'EOF'
Usage: kombat_demo.sh <command>

Commands:
  start                  Run kombat_chain with dev auto-production enabled
  watch                  Watch intent_executed SSE events for POLICY_ID
  watch-account          Watch account/account-updated/policy SSE events for ACCOUNT_ID
  chain-info             Query kombat_chainInfo
  account                Query kombat_getAccount for ACCOUNT_ID
  policy                 Query kombat_getPolicy for POLICY_ID
  intent-status          Query kombat_getIntentStatus for INTENT_ID_HEX
  events                 Query kombat_listEvents with optional EVENT_ACCOUNT_ID/EVENT_POLICY_ID/EVENT_TOPIC filters
  bootstrap              Create account, create policy, submit cancel intent
  conditional-transfer   Submit a time-after transfer intent
  create-sub-policy      Submit a create-sub-policy intent
  revoke-policy          Submit a revoke-policy intent for CHILD_POLICY_ID
  propose-owner          Submit a propose-owner owner call
  accept-owner           Submit an accept-owner owner call
  pause-account          Submit a pause owner call
  unpause-account        Submit an unpause owner call
  add-policy             Submit an add-policy owner call for EXTRA_POLICY_ID
  remove-policy          Submit a remove-policy owner call for EXTRA_POLICY_ID
  revoke-policy-owner    Submit an owner-call revoke-policy for POLICY_ID
  upgrade-account        Submit an upgrade owner call

Environment overrides:
  RPC_URL, ACCOUNT_ID, POLICY_ID, CHILD_POLICY_ID, EXTRA_POLICY_ID
  OWNER_SECRET_HEX, SESSION_SECRET_HEX, CHILD_GRANTEE_SECRET_HEX, NEW_OWNER_SECRET_HEX
  *_TIMESTAMP_MS, *_NONCE, TIME_AFTER_MS, ORDER_ID, TRANSFER_TO, TRANSFER_TOKEN, TRANSFER_AMOUNT, UPGRADE_CODE_HASH_HEX
  INTENT_ID_HEX, EVENT_LIMIT, EVENT_ACCOUNT_ID, EVENT_POLICY_ID, EVENT_TOPIC
EOF
}

rpc() {
  curl -s "${RPC_URL}" -H 'content-type: application/json' -d @-
}

sign() {
  (
    cd "${ROOT_DIR}"
    cargo run --quiet -p kombat-chain-rpc --bin kombat_sign -- "$@"
  )
}

start() {
  (
    cd "${ROOT_DIR}"
    KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100 \
    KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS=100 \
    KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS=10 \
    cargo run -p kombat-chain-rpc --bin kombat_chain
  )
}

watch_events() {
  curl -N "${RPC_URL}/events?from_seq=0&policy_id=${POLICY_ID}&topic=intent_executed&follow=true&timeout_ms=30000"
}

watch_account_events() {
  curl -N "${RPC_URL}/events?from_seq=0&account_id=${ACCOUNT_ID}&follow=true&timeout_ms=30000"
}

chain_info() {
  jq -nc '{
    jsonrpc:"2.0",
    method:"kombat_chainInfo",
    params:{},
    id:1
  }' | rpc
}

account() {
  jq -nc --argjson account_id "${ACCOUNT_ID}" '{
    jsonrpc:"2.0",
    method:"kombat_getAccount",
    params:{account_id:$account_id},
    id:2
  }' | rpc
}

policy() {
  jq -nc --argjson policy_id "${POLICY_ID}" '{
    jsonrpc:"2.0",
    method:"kombat_getPolicy",
    params:{policy_id:$policy_id},
    id:3
  }' | rpc
}

intent_status() {
  if [[ -z "${INTENT_ID_HEX}" ]]; then
    echo "INTENT_ID_HEX must be set for intent-status" >&2
    exit 1
  fi
  jq -nc --arg intent_id_hex "${INTENT_ID_HEX}" '{
    jsonrpc:"2.0",
    method:"kombat_getIntentStatus",
    params:{intent_id_hex:$intent_id_hex},
    id:4
  }' | rpc
}

events() {
  local account_filter="null"
  local policy_filter="null"
  local topic_filter="null"

  if [[ -n "${EVENT_ACCOUNT_ID}" ]]; then
    account_filter="${EVENT_ACCOUNT_ID}"
  fi
  if [[ -n "${EVENT_POLICY_ID}" ]]; then
    policy_filter="${EVENT_POLICY_ID}"
  fi
  if [[ -n "${EVENT_TOPIC}" ]]; then
    topic_filter="\"${EVENT_TOPIC}\""
  fi

  jq -nc \
    --argjson account_id "${account_filter}" \
    --argjson policy_id "${policy_filter}" \
    --argjson topic "${topic_filter}" \
    --argjson limit "${EVENT_LIMIT}" \
    '{
      jsonrpc:"2.0",
      method:"kombat_listEvents",
      params:{
        from_seq:0,
        limit:$limit,
        filter:{
          account_id:$account_id,
          policy_id:$policy_id,
          intent_id_hex:null,
          topic:$topic
        }
      },
      id:5
    }' | rpc
}

bootstrap() {
  sign create-account \
    --account-id "${ACCOUNT_ID}" \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --code-hash-hex 0909090909090909090909090909090909090909090909090909090909090909 \
    --timestamp-ms "${CREATE_ACCOUNT_TIMESTAMP_MS}" | rpc
  echo
  sign create-policy \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --grantee-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --expires-at 50000 \
    --timestamp-ms "${CREATE_POLICY_TIMESTAMP_MS}" | rpc
  echo
  sign cancel-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce "${CANCEL_NONCE}" \
    --expires-at 10000 \
    --timestamp-ms "${CANCEL_INTENT_TIMESTAMP_MS}" \
    --order-id "${ORDER_ID}" | rpc
  echo
}

conditional_transfer() {
  sign transfer-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce "${TRANSFER_NONCE}" \
    --expires-at 20000 \
    --time-after-ms "${TIME_AFTER_MS}" \
    --to "${TRANSFER_TO}" \
    --token "${TRANSFER_TOKEN}" \
    --amount "${TRANSFER_AMOUNT}" \
    --timestamp-ms "${TRANSFER_INTENT_TIMESTAMP_MS}" | rpc
  echo
}

create_sub_policy() {
  sign create-sub-policy-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --child-grantee-secret-hex "${CHILD_GRANTEE_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce "${SUB_POLICY_NONCE}" \
    --expires-at 30000 \
    --child-policy-id "${CHILD_POLICY_ID}" \
    --child-expires-at 40000 \
    --timestamp-ms "${SUB_POLICY_TIMESTAMP_MS}" | rpc
  echo
}

revoke_policy() {
  sign revoke-policy-intent \
    --session-secret-hex "${SESSION_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --policy-id "${POLICY_ID}" \
    --nonce "${REVOKE_NONCE}" \
    --expires-at 40000 \
    --target-policy-id "${CHILD_POLICY_ID}" \
    --timestamp-ms "${REVOKE_POLICY_TIMESTAMP_MS}" | rpc
  echo
}

propose_owner() {
  sign propose-owner \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --new-owner-secret-hex "${NEW_OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${PROPOSE_OWNER_NONCE}" \
    --timestamp-ms "${PROPOSE_OWNER_TIMESTAMP_MS}" | rpc
  echo
}

accept_owner() {
  sign accept-owner \
    --pending-owner-secret-hex "${NEW_OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${ACCEPT_OWNER_NONCE}" \
    --timestamp-ms "${ACCEPT_OWNER_TIMESTAMP_MS}" | rpc
  echo
}

pause_account() {
  sign pause-account \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${PAUSE_ACCOUNT_NONCE}" \
    --timestamp-ms "${PAUSE_ACCOUNT_TIMESTAMP_MS}" | rpc
  echo
}

unpause_account() {
  sign unpause-account \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${UNPAUSE_ACCOUNT_NONCE}" \
    --timestamp-ms "${UNPAUSE_ACCOUNT_TIMESTAMP_MS}" | rpc
  echo
}

add_policy() {
  sign add-policy \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${ADD_POLICY_NONCE}" \
    --policy-id "${EXTRA_POLICY_ID}" \
    --timestamp-ms "${ADD_POLICY_TIMESTAMP_MS}" | rpc
  echo
}

remove_policy() {
  sign remove-policy \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${REMOVE_POLICY_NONCE}" \
    --policy-id "${EXTRA_POLICY_ID}" \
    --timestamp-ms "${REMOVE_POLICY_TIMESTAMP_MS}" | rpc
  echo
}

revoke_policy_owner() {
  sign revoke-policy-owner-call \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${REVOKE_POLICY_OWNER_NONCE}" \
    --policy-id "${POLICY_ID}" \
    --timestamp-ms "${REVOKE_POLICY_OWNER_TIMESTAMP_MS}" | rpc
  echo
}

upgrade_account() {
  sign upgrade-account \
    --owner-secret-hex "${OWNER_SECRET_HEX}" \
    --account-id "${ACCOUNT_ID}" \
    --nonce "${UPGRADE_ACCOUNT_NONCE}" \
    --code-hash-hex "${UPGRADE_CODE_HASH_HEX}" \
    --timestamp-ms "${UPGRADE_ACCOUNT_TIMESTAMP_MS}" | rpc
  echo
}

command="${1:-}"
case "${command}" in
  start) start ;;
  watch) watch_events ;;
  watch-account) watch_account_events ;;
  chain-info) chain_info ;;
  account) account ;;
  policy) policy ;;
  intent-status) intent_status ;;
  events) events ;;
  bootstrap) bootstrap ;;
  conditional-transfer) conditional_transfer ;;
  create-sub-policy) create_sub_policy ;;
  revoke-policy) revoke_policy ;;
  propose-owner) propose_owner ;;
  accept-owner) accept_owner ;;
  pause-account) pause_account ;;
  unpause-account) unpause_account ;;
  add-policy) add_policy ;;
  remove-policy) remove_policy ;;
  revoke-policy-owner) revoke_policy_owner ;;
  upgrade-account) upgrade_account ;;
  ""|-h|--help|help) usage ;;
  *)
    echo "unknown command: ${command}" >&2
    usage >&2
    exit 1
    ;;
esac
