#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  rollout_testnet_network.sh <generated-assets-dir> [--dry-run]

Description:
  Starts a generated 4-validator testnet in bootstrap order:
  1. validator-1 fresh
  2. validator-2 / validator-3 / validator-4 from peer bootstrap

  The generated-assets-dir should come from:
    scripts/generate_testnet_assets.sh

  Required files:
  - hosts.txt
  - validator-1.env ... validator-4.env

Options:
  --dry-run   Print the remote commands that would run without executing them
EOF
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
DRY_RUN=0
if [[ $# -eq 2 ]]; then
  if [[ "$2" != "--dry-run" ]]; then
    usage >&2
    exit 1
  fi
  DRY_RUN=1
fi

if [[ ! -d "${ASSET_DIR}" ]]; then
  echo "rollout_testnet_network.sh: missing asset dir: ${ASSET_DIR}" >&2
  exit 1
fi

HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "rollout_testnet_network.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require ssh
require curl
require jq

info() { printf -- '----  %s\n' "$*"; }
ok() { printf -- 'PASS  %s\n' "$*"; }
bad() { printf -- 'FAIL  %s\n' "$*" >&2; }

declare -a NAMES=()
declare -a SSH_HOSTS=()
declare -a PUBLIC_URLS=()

while IFS=$'\t' read -r name ssh_host public_rpc_url; do
  if [[ "${name}" == "name" ]]; then
    continue
  fi
  NAMES+=("${name}")
  SSH_HOSTS+=("${ssh_host}")
  PUBLIC_URLS+=("${public_rpc_url}")
done < "${HOSTS_FILE}"

if [[ "${#NAMES[@]}" -lt 4 ]]; then
  echo "rollout_testnet_network.sh: expected at least 4 validators in hosts.txt" >&2
  exit 1
fi

run_remote() {
  local host="$1"
  local command="$2"
  if [[ "${DRY_RUN}" -eq 1 ]]; then
    printf '[dry-run] ssh %s %q\n' "${host}" "${command}"
    return 0
  fi
  ssh "${host}" "${command}"
}

rpc_ready() {
  local url="$1"
  local response
  response=$(curl -sS -H 'content-type: application/json' \
    --data '{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{},"id":1}' \
    "${url}" 2>/dev/null || true)
  printf '%s' "${response}" | jq -e '.result.chain_id' >/dev/null 2>&1
}

wait_for_rpc() {
  local url="$1"
  local label="$2"
  local deadline=$(( $(date +%s) + 30 ))
  while (( $(date +%s) < deadline )); do
    if rpc_ready "${url}"; then
      ok "${label} reachable at ${url}"
      return 0
    fi
    sleep 1
  done
  bad "${label} never became reachable at ${url}"
  return 1
}

bootstrap_round() {
  local url="$1"
  curl -sS "${url%/}/network/bootstrap" | jq -r '.current_round'
}

wait_for_round_at_least() {
  local url="$1"
  local label="$2"
  local expected_round="$3"
  local deadline=$(( $(date +%s) + 30 ))
  while (( $(date +%s) < deadline )); do
    local actual
    actual=$(bootstrap_round "${url}" 2>/dev/null || echo 0)
    if [[ "${actual}" =~ ^[0-9]+$ ]] && (( actual >= expected_round )); then
      ok "${label} current round is ${actual}"
      return 0
    fi
    sleep 1
  done
  bad "${label} never reached round ${expected_round}"
  return 1
}

start_validator() {
  local index="$1"
  local name="${NAMES[$index]}"
  local host="${SSH_HOSTS[$index]}"
  local url="${PUBLIC_URLS[$index]}"
  info "starting ${name} on ${host}"
  run_remote "${host}" \
    'systemctl start kombat-chain-snapshot.timer && systemctl restart kombat-chain && systemctl status kombat-chain --no-pager | head -20'
  if [[ "${DRY_RUN}" -eq 0 ]]; then
    wait_for_rpc "${url}" "${name}"
  fi
}

info "starting bootstrap validator ${NAMES[0]}"
start_validator 0
if [[ "${DRY_RUN}" -eq 0 ]]; then
  wait_for_round_at_least "${PUBLIC_URLS[0]}" "${NAMES[0]}" 1
fi

for index in 1 2 3; do
  info "starting peer validator ${NAMES[$index]}"
  start_validator "${index}"
  if [[ "${DRY_RUN}" -eq 0 ]]; then
    wait_for_round_at_least "${PUBLIC_URLS[$index]}" "${NAMES[$index]}" 2
  fi
done

if [[ "${DRY_RUN}" -eq 1 ]]; then
  ok "dry-run completed"
else
  ok "all validators started in rollout order"
fi
