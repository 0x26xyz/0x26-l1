#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  arm_validator_restart_bootstrap.sh <generated-assets-dir>

Description:
  Updates the bootstrap validator's remote env file so future restarts load
  from a peer snapshot instead of replaying the original empty bootstrap block.
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"
BOOTSTRAP_ENV="${ASSET_DIR}/validator-1.env"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "arm_validator_restart_bootstrap.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi
if [[ ! -f "${BOOTSTRAP_ENV}" ]]; then
  echo "arm_validator_restart_bootstrap.sh: missing validator-1.env: ${BOOTSTRAP_ENV}" >&2
  exit 1
fi

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require awk
require python3
require ssh

bootstrap_host="$(awk -F'\t' 'NR==2 {print $2}' "${HOSTS_FILE}")"
peer_rpc_url="$(awk -F'\t' 'NR==3 {print $3}' "${HOSTS_FILE}")"
operator_token="$(awk -F= '/^KOMBAT_OPERATOR_TOKEN=/{print $2}' "${BOOTSTRAP_ENV}")"

if [[ -z "${bootstrap_host}" || -z "${peer_rpc_url}" || -z "${operator_token}" ]]; then
  echo "arm_validator_restart_bootstrap.sh: failed to derive bootstrap host, peer url, or operator token" >&2
  exit 1
fi

ssh "${bootstrap_host}" "python3 - <<'PY'
from pathlib import Path
path = Path('/etc/and-chain/kombat-chain.env')
text = path.read_text()
lines = [line for line in text.splitlines() if not line.startswith('KOMBAT_BOOTSTRAP_RPC_URL=') and not line.startswith('KOMBAT_BOOTSTRAP_BEARER_TOKEN=') and not line.startswith('KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=')]
lines.append('KOMBAT_BOOTSTRAP_RPC_URL=${peer_rpc_url}')
lines.append('KOMBAT_BOOTSTRAP_BEARER_TOKEN=${operator_token}')
path.write_text('\\n'.join(lines) + '\\n')
PY"

echo "armed restart bootstrap on ${bootstrap_host} using ${peer_rpc_url}"
