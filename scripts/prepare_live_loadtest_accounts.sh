#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  prepare_live_loadtest_accounts.sh <generated-assets-dir> [count] [writers-json]

Description:
  Seeds one fresh account + policy + session key per load-test writer through
  the live validator network, then writes a JSON manifest suitable for
  `kombat_loadgen --writers-json`.

Environment:
  KOMBAT_SIGN_PATH   passed through to seed_network_account.sh
  LOADTEST_WRITER_SEED_TAG
                    optional deterministic numeric seed tag. When set, the
                    same account ids, policy ids, and session keys are
                    generated after every testnet reset.
  LOADTEST_SEED_CONCURRENCY
                    number of account/policy seed jobs to run concurrently.
                    default: 1 (serial, historical behavior).
EOF
}

if [[ $# -lt 1 || $# -gt 3 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
COUNT="${2:-4}"
WRITERS_JSON="${3:-/tmp/and-live-loadtest-writers-$(date +%Y%m%d-%H%M%S).json}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SEED_SCRIPT="${CHAIN_DIR}/scripts/seed_network_account.sh"

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require jq
require python3

if [[ ! -f "${SEED_SCRIPT}" ]]; then
  echo "prepare_live_loadtest_accounts.sh: missing seed helper: ${SEED_SCRIPT}" >&2
  exit 1
fi

if ! [[ "${COUNT}" =~ ^[0-9]+$ ]] || [[ "${COUNT}" -eq 0 ]]; then
  echo "prepare_live_loadtest_accounts.sh: count must be a positive integer" >&2
  exit 1
fi
LOADTEST_SEED_CONCURRENCY="${LOADTEST_SEED_CONCURRENCY:-1}"
if ! [[ "${LOADTEST_SEED_CONCURRENCY}" =~ ^[0-9]+$ ]] || [[ "${LOADTEST_SEED_CONCURRENCY}" -eq 0 ]]; then
  echo "prepare_live_loadtest_accounts.sh: LOADTEST_SEED_CONCURRENCY must be a positive integer" >&2
  exit 1
fi

mkdir -p "$(dirname "${WRITERS_JSON}")"

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }

WORK_DIR="$(mktemp -d -t kombat-live-loadtest-accounts.XXXXXX)"
trap 'rm -rf "${WORK_DIR}"' EXIT

FULL_WRITERS_JSON="${WORK_DIR}/writers-full.json"
python3 - "${COUNT}" "${FULL_WRITERS_JSON}" <<'PY'
import hashlib
import json
import os
import sys
import time

count = int(sys.argv[1])
out_path = sys.argv[2]
seed_tag = os.environ.get("LOADTEST_WRITER_SEED_TAG")
run_tag = int(seed_tag) if seed_tag else int(time.time()) % 100000
writers = []
for i in range(count):
    owner_secret = hashlib.sha256(f"loadtest-owner:{run_tag}:{i}".encode()).hexdigest()
    session_secret = hashlib.sha256(f"loadtest-session:{run_tag}:{i}".encode()).hexdigest()
    writers.append({
        "account_id": 8800000000 + run_tag * 100 + i,
        "policy_id": 8900000000 + run_tag * 100 + i,
        "owner_secret_hex": owner_secret,
        "session_secret_hex": session_secret,
    })

with open(out_path, "w", encoding="utf-8") as fh:
    json.dump(writers, fh, indent=2, sort_keys=True)
PY

SEED_LOG_DIR="${WORK_DIR}/seed-logs"
mkdir -p "${SEED_LOG_DIR}"

wait_for_seed_batch() {
  local failed=0
  local idx
  for idx in "${!seed_pids[@]}"; do
    if wait "${seed_pids[$idx]}"; then
      ok "seeded load-test writer account=${seed_accounts[$idx]} policy=${seed_policies[$idx]} log=${seed_logs[$idx]}"
    else
      failed=1
      echo "FAIL  seed failed account=${seed_accounts[$idx]} policy=${seed_policies[$idx]} log=${seed_logs[$idx]}" >&2
      sed -n '1,160p' "${seed_logs[$idx]}" >&2 || true
    fi
  done
  seed_pids=()
  seed_accounts=()
  seed_policies=()
  seed_logs=()
  return "${failed}"
}

info "seeding ${COUNT} load-test writers with concurrency=${LOADTEST_SEED_CONCURRENCY}"
seed_pids=()
seed_accounts=()
seed_policies=()
seed_logs=()
seed_failed=0
while IFS= read -r writer; do
  account_id="$(printf '%s\n' "${writer}" | jq -r '.account_id')"
  policy_id="$(printf '%s\n' "${writer}" | jq -r '.policy_id')"
  owner_secret_hex="$(printf '%s\n' "${writer}" | jq -r '.owner_secret_hex')"
  session_secret_hex="$(printf '%s\n' "${writer}" | jq -r '.session_secret_hex')"
  seed_log="${SEED_LOG_DIR}/account-${account_id}-policy-${policy_id}.log"
  info "starting load-test writer seed account=${account_id} policy=${policy_id} log=${seed_log}"
  (
    bash "${SEED_SCRIPT}" "${ASSET_DIR}" "${account_id}" "${policy_id}" "${owner_secret_hex}" "${session_secret_hex}"
  ) > "${seed_log}" 2>&1 &
  seed_pids+=("$!")
  seed_accounts+=("${account_id}")
  seed_policies+=("${policy_id}")
  seed_logs+=("${seed_log}")
  if [[ "${#seed_pids[@]}" -ge "${LOADTEST_SEED_CONCURRENCY}" ]]; then
    if ! wait_for_seed_batch; then
      seed_failed=1
    fi
  fi
done < <(jq -c '.[]' "${FULL_WRITERS_JSON}")
if [[ "${#seed_pids[@]}" -gt 0 ]]; then
  if ! wait_for_seed_batch; then
    seed_failed=1
  fi
fi
if [[ "${seed_failed}" -ne 0 ]]; then
  echo "prepare_live_loadtest_accounts.sh: one or more seed jobs failed; logs are under ${SEED_LOG_DIR}" >&2
  exit 1
fi

jq '[.[] | {account_id, policy_id, session_secret_hex}]' "${FULL_WRITERS_JSON}" > "${WRITERS_JSON}"
ok "wrote load-test writers manifest to ${WRITERS_JSON}"
