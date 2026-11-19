#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  fanout_owner_call.sh <generated-assets-dir> <owner-call-request.json>

Description:
  Sends the same signed kombat_submitOwnerCall JSON-RPC request to every
  validator listed in <generated-assets-dir>/hosts.txt.

  This is an operational helper for the current live network while owner
  calls remain local-state mutations rather than block-driven network writes.
EOF
}

if [[ $# -ne 2 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
REQUEST_FILE="$2"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"

if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "fanout_owner_call.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi
if [[ ! -f "${REQUEST_FILE}" ]]; then
  echo "fanout_owner_call.sh: missing request file: ${REQUEST_FILE}" >&2
  exit 1
fi

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require curl
require jq

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }
bad() { printf -- 'FAIL  %s\n' "$*" >&2; }

while IFS=$'\t' read -r name ssh_host public_rpc_url; do
  if [[ "${name}" == "name" ]]; then
    continue
  fi

  info "submitting owner call to ${name} (${public_rpc_url})"
  response=$(curl -sS -H 'content-type: application/json' \
    --data-binary @"${REQUEST_FILE}" \
    "${public_rpc_url}")

  if printf '%s' "${response}" | jq -e '.result.accepted == true' >/dev/null 2>&1; then
    ok "${name} accepted owner call"
  else
    bad "${name} owner call failed: ${response}"
    exit 1
  fi
done < "${HOSTS_FILE}"

ok "owner call fanout succeeded across all validators"
