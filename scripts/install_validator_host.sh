#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  install_validator_host.sh <host> <env-file> [--validator-set <path>] [--start]

Description:
  Copies the current kombat_chain validator assets onto a remote host,
  installs systemd units, and enables the validator service plus snapshot
  timer.

Arguments:
  host       SSH destination, e.g. root@203.0.113.15
  env-file   Local env file to install as /etc/and-chain/kombat-chain.env

Options:
  --validator-set <path>
            Local validator-set TOML to install as
            /etc/and-chain/validator-set.toml
  --start    Start the validator service immediately after install

Environment:
  KOMBAT_BINARY_PATH   Override local binary path
                       default: chain/target-linux-x86_64/release/kombat_chain
EOF
}

if [[ $# -lt 2 ]]; then
  usage >&2
  exit 1
fi

HOST="$1"
ENV_FILE="$2"
shift 2

START_AFTER_INSTALL=0
VALIDATOR_SET_FILE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --validator-set)
      [[ $# -ge 2 ]] || { echo "install_validator_host.sh: --validator-set requires a path" >&2; exit 1; }
      VALIDATOR_SET_FILE="$2"
      shift 2
      ;;
    --start)
      START_AFTER_INSTALL=1
      shift
      ;;
    *)
      echo "install_validator_host.sh: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINARY_PATH="${KOMBAT_BINARY_PATH:-${CHAIN_DIR}/target-linux-x86_64/release/kombat_chain}"

SERVICE_FILE="${CHAIN_DIR}/deploy/systemd/kombat-chain.service.example"
ENV_TEMPLATE="${CHAIN_DIR}/deploy/systemd/kombat-chain.env.example"
SNAPSHOT_SERVICE_FILE="${CHAIN_DIR}/deploy/systemd/kombat-chain-snapshot.service.example"
SNAPSHOT_TIMER_FILE="${CHAIN_DIR}/deploy/systemd/kombat-chain-snapshot.timer.example"
SAVE_SNAPSHOT_SCRIPT="${CHAIN_DIR}/scripts/save_snapshot.sh"

for path in \
  "${BINARY_PATH}" \
  "${ENV_FILE}" \
  "${SERVICE_FILE}" \
  "${SNAPSHOT_SERVICE_FILE}" \
  "${SNAPSHOT_TIMER_FILE}" \
  "${SAVE_SNAPSHOT_SCRIPT}"; do
  if [[ ! -f "${path}" ]]; then
    echo "install_validator_host.sh: missing required file: ${path}" >&2
    exit 1
  fi
done

if [[ -n "${VALIDATOR_SET_FILE}" && ! -f "${VALIDATOR_SET_FILE}" ]]; then
  echo "install_validator_host.sh: missing validator set file: ${VALIDATOR_SET_FILE}" >&2
  exit 1
fi

TMP_DIR="/tmp/and-chain-install.$$"

echo "==> creating remote directories on ${HOST}"
ssh "${HOST}" "\
  set -euo pipefail && \
  id and-chain >/dev/null 2>&1 || useradd --system --home /var/lib/and-chain --shell /usr/sbin/nologin and-chain && \
  mkdir -p /opt/and-chain/bin /opt/and-chain/scripts /etc/and-chain /var/lib/and-chain/snapshots ${TMP_DIR} && \
  chown -R and-chain:and-chain /var/lib/and-chain \
"

echo "==> copying validator assets to ${HOST}"
copy_paths=(
  "${BINARY_PATH}"
  "${ENV_FILE}"
  "${SERVICE_FILE}"
  "${SNAPSHOT_SERVICE_FILE}"
  "${SNAPSHOT_TIMER_FILE}"
  "${SAVE_SNAPSHOT_SCRIPT}"
)
if [[ -n "${VALIDATOR_SET_FILE}" ]]; then
  copy_paths+=("${VALIDATOR_SET_FILE}")
fi
scp "${copy_paths[@]}" "${HOST}:${TMP_DIR}/"

echo "==> installing systemd assets on ${HOST}"
ssh "${HOST}" "\
  set -euo pipefail && \
  install -o root -g root -m 0755 ${TMP_DIR}/kombat_chain /opt/and-chain/bin/kombat_chain && \
  install -o root -g root -m 0755 ${TMP_DIR}/save_snapshot.sh /opt/and-chain/scripts/save_snapshot.sh && \
  install -o root -g root -m 0644 ${TMP_DIR}/$(basename "${ENV_FILE}") /etc/and-chain/kombat-chain.env && \
  $(if [[ -n "${VALIDATOR_SET_FILE}" ]]; then printf 'install -o root -g root -m 0644 %s/%s /etc/and-chain/validator-set.toml && \\\n  ' "${TMP_DIR}" "$(basename "${VALIDATOR_SET_FILE}")"; fi) \
  install -o root -g root -m 0644 ${TMP_DIR}/$(basename "${SERVICE_FILE}") /etc/systemd/system/kombat-chain.service && \
  install -o root -g root -m 0644 ${TMP_DIR}/$(basename "${SNAPSHOT_SERVICE_FILE}") /etc/systemd/system/kombat-chain-snapshot.service && \
  install -o root -g root -m 0644 ${TMP_DIR}/$(basename "${SNAPSHOT_TIMER_FILE}") /etc/systemd/system/kombat-chain-snapshot.timer && \
  rm -rf ${TMP_DIR} && \
  systemctl daemon-reload && \
  systemctl enable kombat-chain.service kombat-chain-snapshot.timer >/dev/null \
"

if [[ ${START_AFTER_INSTALL} -eq 1 ]]; then
  echo "==> starting validator on ${HOST}"
  ssh "${HOST}" "systemctl restart kombat-chain && systemctl start kombat-chain-snapshot.timer && systemctl status kombat-chain --no-pager | head -20"
else
  echo "==> install complete on ${HOST}"
  echo "    next:"
  echo "    ssh ${HOST} 'systemctl start kombat-chain-snapshot.timer && systemctl start kombat-chain'"
fi

echo "==> done"
