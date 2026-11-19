#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  seed_network_account.sh <generated-assets-dir> <account-id> <policy-id> <owner-secret-hex> <session-secret-hex>

Description:
  Submits a single kombat_createAccount followed by a single signed
  kombat_submitOwnerCall into the live validator network, then verifies
  that the resulting account and policy state is visible on every validator.

Environment:
  KOMBAT_SIGN_PATH   Override signer binary path
                     default: chain/target/release/kombat_sign
  CODE_HASH_HEX      Override account code hash
                     default: 0909...0909
  SEED_NETWORK_BYPASS_FANOUT
                     set to 1 to bypass validator network fanout during local seeding
  SEED_VERIFY_REPLICAS
                     set to 0 to skip per-validator replication checks
EOF
}

if [[ $# -ne 5 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
ACCOUNT_ID="$2"
POLICY_ID="$3"
OWNER_SECRET_HEX="$4"
SESSION_SECRET_HEX="$5"

HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "seed_network_account.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SIGN_BINARY="${KOMBAT_SIGN_PATH:-${CHAIN_DIR}/target/release/kombat_sign}"
CODE_HASH_HEX="${CODE_HASH_HEX:-0909090909090909090909090909090909090909090909090909090909090909}"
SEED_NETWORK_BYPASS_FANOUT="${SEED_NETWORK_BYPASS_FANOUT:-0}"
SEED_VERIFY_REPLICAS="${SEED_VERIFY_REPLICAS:-1}"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require curl
require jq

if [[ ! -x "${SIGN_BINARY}" ]]; then
  echo "seed_network_account.sh: missing signer binary: ${SIGN_BINARY}" >&2
  exit 1
fi

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }
bad() { printf -- 'FAIL  %s\n' "$*" >&2; }

WORK_DIR="$(mktemp -d -t kombat-seed-account.XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT

TIMESTAMP_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"
CREATE_ACCOUNT_REQUEST="${WORK_DIR}/create-account.json"
CREATE_POLICY_REQUEST="${WORK_DIR}/create-policy.json"

"${SIGN_BINARY}" create-account \
  --account-id "${ACCOUNT_ID}" \
  --owner-secret-hex "${OWNER_SECRET_HEX}" \
  --code-hash-hex "${CODE_HASH_HEX}" \
  --timestamp-ms "${TIMESTAMP_MS}" \
  > "${CREATE_ACCOUNT_REQUEST}"

TIMESTAMP_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"
"${SIGN_BINARY}" create-policy \
  --owner-secret-hex "${OWNER_SECRET_HEX}" \
  --grantee-secret-hex "${SESSION_SECRET_HEX}" \
  --account-id "${ACCOUNT_ID}" \
  --policy-id "${POLICY_ID}" \
  --expires-at 99999999999999 \
  --timestamp-ms "${TIMESTAMP_MS}" \
  --max-intents-per-second 1000000 \
  --max-intents-per-day 1000000000 \
  --max-volume-per-day 1000000000000 \
  --gas-budget-per-day 1000000000000 \
  --gas-refill-per-day 1000000000000 \
  --max-drawdown-pct 10000 \
  > "${CREATE_POLICY_REQUEST}"

ENTRYPOINT_URL=""
while IFS=$'\t' read -r name ssh_host public_rpc_url; do
  if [[ "${name}" == "name" ]]; then
    continue
  fi
  if [[ -z "${ENTRYPOINT_URL}" ]]; then
    ENTRYPOINT_URL="${public_rpc_url}"
  fi
done < "${HOSTS_FILE}"

if [[ -z "${ENTRYPOINT_URL}" ]]; then
  echo "seed_network_account.sh: no validator rpc urls found in ${HOSTS_FILE}" >&2
  exit 1
fi

info "submitting account ${ACCOUNT_ID} via ${ENTRYPOINT_URL}"
create_account_headers=(-H 'content-type: application/json')
if [[ "${SEED_NETWORK_BYPASS_FANOUT}" == "1" ]]; then
  create_account_headers+=(-H 'x-and-network-relay: create-account')
fi
response=$(curl -sS "${create_account_headers[@]}" \
  --data-binary @"${CREATE_ACCOUNT_REQUEST}" \
  "${ENTRYPOINT_URL}")
if ! printf '%s' "${response}" | jq -e '.result.id == '"${ACCOUNT_ID}" >/dev/null 2>&1; then
  bad "network account creation failed: ${response}"
  exit 1
fi
ok "network created account ${ACCOUNT_ID}"

info "submitting policy ${POLICY_ID} via ${ENTRYPOINT_URL}"
create_policy_headers=(-H 'content-type: application/json')
if [[ "${SEED_NETWORK_BYPASS_FANOUT}" == "1" ]]; then
  create_policy_headers+=(-H 'x-and-network-relay: owner-call')
fi
response=$(curl -sS "${create_policy_headers[@]}" \
  --data-binary @"${CREATE_POLICY_REQUEST}" \
  "${ENTRYPOINT_URL}")
if ! printf '%s' "${response}" | jq -e '.result.policy_id == '"${POLICY_ID}" >/dev/null 2>&1; then
  bad "network policy creation failed: ${response}"
  exit 1
fi
ok "network created policy ${POLICY_ID}"

if [[ "${SEED_VERIFY_REPLICAS}" != "0" ]]; then
  while IFS=$'\t' read -r name ssh_host public_rpc_url; do
    if [[ "${name}" == "name" ]]; then
      continue
    fi

    info "verifying replicated state on ${name} (${public_rpc_url})"
    account_response=$(curl -sS -H 'content-type: application/json' \
      --data "{\"jsonrpc\":\"2.0\",\"method\":\"kombat_getAccount\",\"params\":{\"account_id\":${ACCOUNT_ID}},\"id\":1}" \
      "${public_rpc_url}")
    if ! printf '%s' "${account_response}" | jq -e '.result.id == '"${ACCOUNT_ID}" >/dev/null 2>&1; then
      bad "${name} missing account ${ACCOUNT_ID}: ${account_response}"
      exit 1
    fi

    policy_response=$(curl -sS -H 'content-type: application/json' \
      --data "{\"jsonrpc\":\"2.0\",\"method\":\"kombat_getPolicy\",\"params\":{\"policy_id\":${POLICY_ID}},\"id\":2}" \
      "${public_rpc_url}")
    if ! printf '%s' "${policy_response}" | jq -e '.result.id == '"${POLICY_ID}" >/dev/null 2>&1; then
      bad "${name} missing policy ${POLICY_ID}: ${policy_response}"
      exit 1
    fi

    ok "${name} replicated account ${ACCOUNT_ID} and policy ${POLICY_ID}"
  done < "${HOSTS_FILE}"
fi

ok "seeded account ${ACCOUNT_ID} and policy ${POLICY_ID} through the live validator network"
