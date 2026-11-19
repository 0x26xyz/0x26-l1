#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  smoke_live_timeout_recovery.sh <generated-assets-dir>

Description:
  Exercises timeout recovery on the real validator hosts:
  1. discovers the elected leader for the current round
  2. stops that validator over SSH
  3. waits for the surviving validators to advance round
  4. restarts the stopped validator
  5. runs the live write-path smoke to prove post-rejoin commits still work

Environment:
  KOMBAT_SIGN_PATH  passed through to smoke_live_network.sh
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
  echo "smoke_live_timeout_recovery.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi
if [[ ! -f "${VALIDATOR_SET_FILE}" ]]; then
  echo "smoke_live_timeout_recovery.sh: missing validator-set.toml: ${VALIDATOR_SET_FILE}" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SMOKE_LIVE_NETWORK="${CHAIN_DIR}/scripts/smoke_live_network.sh"

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

if [[ ! -f "${SMOKE_LIVE_NETWORK}" ]]; then
  echo "smoke_live_timeout_recovery.sh: missing smoke helper: ${SMOKE_LIVE_NETWORK}" >&2
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
  echo "smoke_live_timeout_recovery.sh: expected at least 4 validator rows in hosts.txt" >&2
  exit 1
fi

FOLLOW_URL="$(printf '%s\n' "${HOST_ROWS[0]}" | awk -F'\t' '{print $3}')"
CURRENT_ROUND="$(curl -sS "${FOLLOW_URL%/}/network/bootstrap" | jq -r '.current_round')"
CHAIN_ID="$(curl -sS -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}' \
  "${FOLLOW_URL}" | jq -r '.result.chain_id')"

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

info "stopping elected leader ${LEADER_ID_HEX} on ${LEADER_HOST} for round ${CURRENT_ROUND}"
ssh "${LEADER_HOST}" 'systemctl stop kombat-chain'

TARGET_ROUND=$(( CURRENT_ROUND + 1 ))
for url in "${SURVIVOR_URLS[@]}"; do
  info "waiting for survivor ${url} to reach round ${TARGET_ROUND}"
  deadline=$(( $(date +%s) + 30 ))
  reached=0
  while (( $(date +%s) < deadline )); do
    round="$(curl -sS "${url%/}/network/bootstrap" 2>/dev/null | jq -r '.current_round // 0' 2>/dev/null || echo 0)"
    if [[ "${round}" =~ ^[0-9]+$ ]] && (( round >= TARGET_ROUND )); then
      ok "${url} reached round ${round}"
      reached=1
      break
    fi
    sleep 1
  done
  if [[ "${reached}" -ne 1 ]]; then
    bad "${url} did not reach round ${TARGET_ROUND}"
    exit 1
  fi
done

info "restarting stopped leader ${LEADER_HOST}"
ssh "${LEADER_HOST}" 'systemctl start kombat-chain'

deadline=$(( $(date +%s) + 30 ))
reachable=0
while (( $(date +%s) < deadline )); do
  if curl -sS -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}' \
    "${LEADER_URL}" | jq -e '.result.chain_id == 7' >/dev/null 2>&1; then
    ok "restarted leader is reachable at ${LEADER_URL}"
    reachable=1
    break
  fi
  sleep 1
done
if [[ "${reachable}" -ne 1 ]]; then
  bad "restarted leader did not become reachable at ${LEADER_URL}"
  exit 1
fi

info "running post-rejoin live smoke"
if [[ -n "${KOMBAT_SIGN_PATH:-}" ]]; then
  KOMBAT_SIGN_PATH="${KOMBAT_SIGN_PATH}" bash "${SMOKE_LIVE_NETWORK}" "${ASSET_DIR}"
else
  bash "${SMOKE_LIVE_NETWORK}" "${ASSET_DIR}"
fi
ok "live timeout recovery smoke passed"
