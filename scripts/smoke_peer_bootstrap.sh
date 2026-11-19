#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_PORT="${SOURCE_PORT:-18645}"
PEER_PORT="${PEER_PORT:-18646}"
CHAIN_ID="${CHAIN_ID:-7}"
OPERATOR_TOKEN="${OPERATOR_TOKEN:-peer-bootstrap-token}"
BINARY_PATH="${KOMBAT_BINARY_PATH:-${ROOT_DIR}/target/debug/kombat_chain}"

WORK_DIR="$(mktemp -d -t kombat-peer-bootstrap.XXXXXX)"
VALIDATOR_SET_PATH="${WORK_DIR}/validator-set.toml"
SOURCE_LOG="${WORK_DIR}/source.log"
PEER_LOG="${WORK_DIR}/peer.log"
SOURCE_PID=""
PEER_PID=""
PASS=0
FAIL=0

LOCAL_SEED_HEX=0707070707070707070707070707070707070707070707070707070707070707
LOCAL_PUBKEY_HEX=ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c
PEER_SEED_HEX=0202020202020202020202020202020202020202020202020202020202020202
PEER_VALIDATOR_ID_HEX=0202020202020202020202020202020202020202020202020202020202020202

color() {
  local code="$1"; shift
  if [[ -t 1 ]]; then printf '\033[%sm%s\033[0m\n' "$code" "$*"; else printf '%s\n' "$*"; fi
}
ok()   { color "32" "PASS  $*"; PASS=$((PASS+1)); }
bad()  { color "31" "FAIL  $*"; FAIL=$((FAIL+1)); }
info() { color "36" "----  $*"; }

cleanup() {
  for pid in "${PEER_PID}" "${SOURCE_PID}"; do
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
require curl
require jq
require cargo

info "building kombat_chain binary for peer bootstrap smoke"
(cd "${ROOT_DIR}" && cargo build -p kombat-chain-rpc --bin kombat_chain >/dev/null)

cat > "${VALIDATOR_SET_PATH}" <<EOF
epoch = 1

[[validators]]
validator_id = "0x${LOCAL_PUBKEY_HEX}"
consensus_pubkey = "0x${LOCAL_PUBKEY_HEX}"
network_pubkey = "0x1111"
rpc_endpoints = ["http://127.0.0.1:${SOURCE_PORT}"]
region = "local-a"
operator_name = "validator-a"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0x0202020202020202020202020202020202020202020202020202020202020202"
consensus_pubkey = "0x8139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b394"
network_pubkey = "0x2222"
rpc_endpoints = ["http://127.0.0.1:${PEER_PORT}"]
region = "local-b"
operator_name = "validator-b"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0x0303030303030303030303030303030303030303030303030303030303030303"
consensus_pubkey = "0xfd1724385aa0c75b64fb78cd602fa1d991fdebf76b13c58ed702eac835e9f618"
network_pubkey = "0x3333"
rpc_endpoints = ["http://127.0.0.1:18647"]
region = "local-c"
operator_name = "validator-c"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0x0404040404040404040404040404040404040404040404040404040404040404"
consensus_pubkey = "0x43a72e714401762df66b68c26dfbdf2682aaec9f2474eca4613e424a0fbafd3c"
network_pubkey = "0x4444"
rpc_endpoints = ["http://127.0.0.1:18648"]
region = "local-d"
operator_name = "validator-d"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"
EOF

rpc_post() {
  local url="$1"
  shift
  curl -sS -H 'content-type: application/json' --data-binary @- "${url}"
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

current_head_triplet() {
  local url="$1"
  local resp
  resp=$(printf '%s' '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":2}' | rpc_post "${url}")
  printf '%s %s %s\n' \
    "$(printf '%s' "${resp}" | jq -r '.result.head_height')" \
    "$(printf '%s' "${resp}" | jq -r '.result.head_hash_hex')" \
    "$(printf '%s' "${resp}" | jq -r '.result.current_round')"
}

start_source() {
  info "starting source validator"
  KOMBAT_RPC_BIND="127.0.0.1:${SOURCE_PORT}" \
  KOMBAT_CHAIN_ID="${CHAIN_ID}" \
  KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
  KOMBAT_VALIDATOR_ID_HEX="0x${LOCAL_PUBKEY_HEX}" \
  KOMBAT_CONSENSUS_SIGNING_KEY_HEX="0x${LOCAL_SEED_HEX}" \
  KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
  KOMBAT_PUBLIC_RPC_URL="http://127.0.0.1:${SOURCE_PORT}" \
  KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100 \
  "${BINARY_PATH}" >"${SOURCE_LOG}" 2>&1 &
  SOURCE_PID=$!
}

start_peer() {
  info "starting peer validator from source snapshot"
  KOMBAT_RPC_BIND="127.0.0.1:${PEER_PORT}" \
  KOMBAT_CHAIN_ID="${CHAIN_ID}" \
  KOMBAT_VALIDATOR_SET_PATH="${VALIDATOR_SET_PATH}" \
  KOMBAT_VALIDATOR_ID_HEX="0x${PEER_VALIDATOR_ID_HEX}" \
  KOMBAT_CONSENSUS_SIGNING_KEY_HEX="0x${PEER_SEED_HEX}" \
  KOMBAT_OPERATOR_TOKEN="${OPERATOR_TOKEN}" \
  KOMBAT_PUBLIC_RPC_URL="http://127.0.0.1:${PEER_PORT}" \
  KOMBAT_BOOTSTRAP_RPC_URL="http://127.0.0.1:${SOURCE_PORT}" \
  KOMBAT_BOOTSTRAP_BEARER_TOKEN="${OPERATOR_TOKEN}" \
  KOMBAT_DEV_ENABLE_MANUAL_PRODUCE=true \
  "${BINARY_PATH}" >"${PEER_LOG}" 2>&1 &
  PEER_PID=$!
}

start_source
wait_for_ready "http://127.0.0.1:${SOURCE_PORT}" "source validator"

read -r source_height source_hash source_round < <(current_head_triplet "http://127.0.0.1:${SOURCE_PORT}")
if [[ "${source_height}" == "1" ]]; then
  ok "source validator bootstrapped height 1"
else
  bad "source validator unexpected head height: ${source_height}"
fi

start_peer
wait_for_ready "http://127.0.0.1:${PEER_PORT}" "peer validator"

read -r peer_height peer_hash peer_round < <(current_head_triplet "http://127.0.0.1:${PEER_PORT}")
if [[ "${peer_height}" == "${source_height}" && "${peer_hash}" == "${source_hash}" ]]; then
  ok "peer validator bootstrapped matching head ${peer_height} ${peer_hash}"
else
  bad "peer validator head mismatch: source=${source_height}/${source_hash} peer=${peer_height}/${peer_hash}"
fi

produce_resp=$(jq -nc '{jsonrpc:"2.0",method:"kombat_produceBlock",params:{timestamp_ms:1200,max_items:10},id:3}' \
  | curl -sS -H 'content-type: application/json' -H "authorization: Bearer ${OPERATOR_TOKEN}" --data-binary @- "http://127.0.0.1:${PEER_PORT}")
if printf '%s' "${produce_resp}" | jq -e '.result.height == 2 and (.error | not)' >/dev/null 2>&1; then
  ok "peer validator continued block production from bootstrapped state"
else
  bad "peer validator failed to continue block production: ${produce_resp}"
fi

read -r final_height final_hash final_round < <(current_head_triplet "http://127.0.0.1:${PEER_PORT}")
if [[ "${final_height}" == "2" ]]; then
  ok "peer validator final head is 2"
else
  bad "peer validator unexpected final head: ${final_height}"
fi

info "summary: ${PASS} passed, ${FAIL} failed"
if [[ "${FAIL}" -ne 0 ]]; then
  exit 1
fi
