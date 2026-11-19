#!/usr/bin/env bash
set -euo pipefail

if ! command -v curl >/dev/null 2>&1; then
  echo "save_snapshot.sh: curl is required" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "save_snapshot.sh: jq is required" >&2
  exit 1
fi

RPC_URL="${KOMBAT_RPC_URL:-http://127.0.0.1:8545}"
OPERATOR_TOKEN="${KOMBAT_OPERATOR_TOKEN:-}"
SNAPSHOT_DIR="${KOMBAT_SNAPSHOT_DIR:-/var/lib/and-chain/snapshots}"
RETENTION_COUNT="${KOMBAT_SNAPSHOT_RETENTION_COUNT:-24}"
TIMESTAMP_UTC="$(date -u +%Y%m%dT%H%M%SZ)"
SNAPSHOT_PATH="${KOMBAT_SNAPSHOT_PATH:-${SNAPSHOT_DIR}/engine-${TIMESTAMP_UTC}.snapshot}"

if [[ -z "${OPERATOR_TOKEN}" ]]; then
  echo "save_snapshot.sh: KOMBAT_OPERATOR_TOKEN is required" >&2
  exit 1
fi

mkdir -p "${SNAPSHOT_DIR}"

REQUEST_PAYLOAD="$(jq -cn --arg path "${SNAPSHOT_PATH}" \
  '{jsonrpc:"2.0",method:"kombat_saveSnapshot",params:{path:$path},id:1}')"

RESPONSE="$(
  curl -fsS \
    -H "Authorization: Bearer ${OPERATOR_TOKEN}" \
    -H "content-type: application/json" \
    --data-binary "${REQUEST_PAYLOAD}" \
    "${RPC_URL}"
)"

if ! printf '%s' "${RESPONSE}" | jq -e '.result.path and (.error | not)' >/dev/null 2>&1; then
  echo "save_snapshot.sh: unexpected RPC response: ${RESPONSE}" >&2
  exit 1
fi

RESULT_PATH="$(printf '%s' "${RESPONSE}" | jq -r '.result.path')"
HEAD_HEIGHT="$(printf '%s' "${RESPONSE}" | jq -r '.result.head_height')"
HEAD_HASH="$(printf '%s' "${RESPONSE}" | jq -r '.result.head_hash_hex')"
SNAPSHOT_SIZE="$(printf '%s' "${RESPONSE}" | jq -r '.result.snapshot_size_bytes')"

echo "saved snapshot: path=${RESULT_PATH} head_height=${HEAD_HEIGHT} head_hash=${HEAD_HASH} bytes=${SNAPSHOT_SIZE}"

mapfile -t SNAPSHOTS < <(find "${SNAPSHOT_DIR}" -maxdepth 1 -type f -name '*.snapshot' | sort)
if (( ${#SNAPSHOTS[@]} > RETENTION_COUNT )); then
  DELETE_COUNT=$(( ${#SNAPSHOTS[@]} - RETENTION_COUNT ))
  for (( i=0; i<DELETE_COUNT; i++ )); do
    rm -f "${SNAPSHOTS[$i]}"
    echo "removed old snapshot: ${SNAPSHOTS[$i]}"
  done
fi
