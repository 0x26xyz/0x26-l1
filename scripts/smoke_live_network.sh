#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  smoke_live_network.sh <generated-assets-dir>

Description:
  Exercises the real live validator network using the hosts listed in
  <generated-assets-dir>/hosts.txt:
  1. seed one account + policy through the network
  2. submit one signed intent through a follower
  3. verify head/policy replication across every validator

Environment:
  KOMBAT_SIGN_PATH       Override signer binary path
                         default: chain/target/release/kombat_sign
  SMOKE_ACCOUNT_ID       default: 950100
  SMOKE_POLICY_ID        default: 950142
  SMOKE_OWNER_SECRET_HEX default: 1010..10
  SMOKE_SESSION_SECRET_HEX
                         default: 1111..11
  SMOKE_MARKET_ID        default: 1
  SMOKE_CLIENT_ORDER_ID  default: 700001
  SMOKE_EXPIRES_AT_MS    default: now + 600000
  SMOKE_TARGET_URL       Override submit target URL, e.g. observer ingress
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "smoke_live_network.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SIGN_BINARY="${KOMBAT_SIGN_PATH:-${CHAIN_DIR}/target/release/kombat_sign}"
SEED_SCRIPT="${CHAIN_DIR}/scripts/seed_network_account.sh"
STATUS_SCRIPT="${CHAIN_DIR}/scripts/live_network_status.sh"

RUN_TAG="$(python3 -c 'import time; print(int(time.time()) % 100000)')"
ACCOUNT_ID="${SMOKE_ACCOUNT_ID:-95${RUN_TAG}}"
POLICY_ID="${SMOKE_POLICY_ID:-96${RUN_TAG}}"
OWNER_SECRET_HEX="${SMOKE_OWNER_SECRET_HEX:-1010101010101010101010101010101010101010101010101010101010101010}"
SESSION_SECRET_HEX="${SMOKE_SESSION_SECRET_HEX:-1111111111111111111111111111111111111111111111111111111111111111}"
MARKET_ID="${SMOKE_MARKET_ID:-1}"
CLIENT_ORDER_ID="${SMOKE_CLIENT_ORDER_ID:-700001}"

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
  echo "smoke_live_network.sh: missing signer binary: ${SIGN_BINARY}" >&2
  exit 1
fi
if [[ ! -f "${SEED_SCRIPT}" ]]; then
  echo "smoke_live_network.sh: missing seed helper: ${SEED_SCRIPT}" >&2
  exit 1
fi
if [[ ! -f "${STATUS_SCRIPT}" ]]; then
  echo "smoke_live_network.sh: missing status helper: ${STATUS_SCRIPT}" >&2
  exit 1
fi

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }
bad() { printf -- 'FAIL  %s\n' "$*" >&2; }

HOST_ROWS=()
while IFS= read -r row; do
  HOST_ROWS+=("${row}")
done < <(tail -n +2 "${HOSTS_FILE}")

if [[ "${#HOST_ROWS[@]}" -lt 4 ]]; then
  echo "smoke_live_network.sh: expected at least 4 validator rows in hosts.txt" >&2
  exit 1
fi

FOLLOWER_URL="$(printf '%s\n' "${HOST_ROWS[1]}" | awk -F'\t' '{print $3}')"
TARGET_URL="${SMOKE_TARGET_URL:-${FOLLOWER_URL}}"
WORK_DIR="$(mktemp -d -t kombat-live-smoke.XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT

info "seeding network account and policy"
KOMBAT_SIGN_PATH="${SIGN_BINARY}" \
  bash "${SEED_SCRIPT}" "${ASSET_DIR}" "${ACCOUNT_ID}" "${POLICY_ID}" "${OWNER_SECRET_HEX}" "${SESSION_SECRET_HEX}"

EXPIRES_AT_MS="${SMOKE_EXPIRES_AT_MS:-$(python3 -c 'import time; print(int(time.time()*1000)+600000)')}"
NOW_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"
INTENT_REQUEST="${WORK_DIR}/place-order-intent.json"

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
  --price-ticks 100 \
  --expires-at "${EXPIRES_AT_MS}" \
  --timestamp-ms "${NOW_MS}" \
  > "${INTENT_REQUEST}"

info "submitting signed intent via ${TARGET_URL}"
submit_response="$(curl -sS -H 'content-type: application/json' --data-binary @"${INTENT_REQUEST}" "${TARGET_URL}")"
if ! printf '%s' "${submit_response}" | jq -e '.result.class == "direct" and .result.current_policy_nonce == 1' >/dev/null 2>&1; then
  bad "intent submission failed: ${submit_response}"
  exit 1
fi
ok "follower accepted intent for policy ${POLICY_ID}"

sleep 3

reference_height=""
reference_hash=""
min_height=""
max_height=""
mahi_enabled="$(curl -sS -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","method":"kombat_getConsensusMetrics","params":{},"id":3}' \
  "${FOLLOWER_URL}" | jq -r '.result.mahi.enabled // false')"
while IFS=$'\t' read -r name ssh_host public_rpc_url; do
  if [[ "${name}" == "name" ]]; then
    continue
  fi

  chain_info="$(curl -sS -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}' \
    "${public_rpc_url}")"
  policy_info="$(curl -sS -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"kombat_getPolicy\",\"params\":{\"policy_id\":${POLICY_ID}},\"id\":2}" \
    "${public_rpc_url}")"

  height="$(printf '%s' "${chain_info}" | jq -r '.result.head_height')"
  hash="$(printf '%s' "${chain_info}" | jq -r '.result.head_hash_hex')"
  next_nonce="$(printf '%s' "${policy_info}" | jq -r '.result.next_intent_nonce')"

  if [[ "${mahi_enabled}" == "true" ]]; then
    if [[ -z "${min_height}" || "${height}" -lt "${min_height}" ]]; then
      min_height="${height}"
    fi
    if [[ -z "${max_height}" || "${height}" -gt "${max_height}" ]]; then
      max_height="${height}"
    fi
  elif [[ -z "${reference_height}" ]]; then
    reference_height="${height}"
    reference_hash="${hash}"
  fi

  if [[ "${mahi_enabled}" != "true" && \
    ( "${height}" != "${reference_height}" || "${hash}" != "${reference_hash}" ) ]]; then
    bad "${name} diverged: height=${height} hash=${hash} expected_height=${reference_height} expected_hash=${reference_hash}"
    exit 1
  fi

  if [[ "${next_nonce}" != "2" ]]; then
    bad "${name} policy nonce did not advance: ${policy_info}"
    exit 1
  fi

  if [[ "${mahi_enabled}" == "true" ]]; then
    ok "${name} observed height=${height} hash=${hash} next_intent_nonce=${next_nonce}"
  else
    ok "${name} converged on height=${height} hash=${hash} next_intent_nonce=${next_nonce}"
  fi
done < "${HOSTS_FILE}"

if [[ "${mahi_enabled}" == "true" ]]; then
  expected_commit_log_hash=""
  while IFS=$'\t' read -r name ssh_host public_rpc_url; do
    if [[ "${name}" == "name" ]]; then
      continue
    fi
    metrics="$(curl -sS -H 'content-type: application/json' \
      --data '{"jsonrpc":"2.0","method":"kombat_getConsensusMetrics","params":{},"id":3}' \
      "${public_rpc_url}")"
    dag_snapshot_hash="$(printf '%s' "${metrics}" | jq -r '.result.mahi.dag_snapshot_hash_hex // "n/a"')"
    commit_log_hash="$(printf '%s' "${metrics}" | jq -r '.result.mahi.commit_log_hash_hex')"
    if [[ -z "${expected_commit_log_hash}" ]]; then
      expected_commit_log_hash="${commit_log_hash}"
    elif [[ "${commit_log_hash}" != "${expected_commit_log_hash}" ]]; then
      bad "${name} diverged on mahi commit log hash: commit_log_hash=${commit_log_hash} expected=${expected_commit_log_hash}"
      exit 1
    fi
    ok "${name} mahi commit_log_hash=${commit_log_hash} dag_snapshot_hash=${dag_snapshot_hash}"
  done < "${HOSTS_FILE}"
  if [[ "${min_height}" != "${max_height}" ]]; then
    info "Mahi latest progress skew accepted: min_height=${min_height} max_height=${max_height}"
  fi
fi

info "cluster status"
bash "${STATUS_SCRIPT}" "${ASSET_DIR}"
ok "live network smoke passed"
