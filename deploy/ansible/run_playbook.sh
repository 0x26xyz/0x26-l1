#!/usr/bin/env bash
# Run live-testnet Ansible playbooks with repo-local config and sandbox-safe
# temp/control paths, regardless of the caller's current working directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export ANSIBLE_CONFIG="${ANSIBLE_CONFIG:-${SCRIPT_DIR}/ansible.cfg}"
export ANSIBLE_HOME="${ANSIBLE_HOME:-${SCRIPT_DIR}/.home}"
export ANSIBLE_LOCAL_TEMP="${ANSIBLE_LOCAL_TEMP:-${SCRIPT_DIR}/.tmp}"
export ANSIBLE_SSH_CONTROL_PATH_DIR="${ANSIBLE_SSH_CONTROL_PATH_DIR:-/tmp/and-chain-ansible-cp}"

mkdir -p "${ANSIBLE_HOME}" "${ANSIBLE_LOCAL_TEMP}" "${ANSIBLE_SSH_CONTROL_PATH_DIR}"

# Local deployment secrets live in an ignored .env file. Load only the values
# that are not already supplied by the caller so explicit one-shot env overrides
# keep taking precedence.
LOCAL_ENV_FILE="${SCRIPT_DIR}/.env"
strip_matching_outer_quotes() {
  local value="$1"
  while [[ "${#value}" -ge 2 ]]; do
    local first="${value:0:1}"
    local last="${value: -1}"
    if [[ "${first}" == "'" && "${last}" == "'" ]]; then
      value="${value:1:${#value}-2}"
      continue
    fi
    if [[ "${first}" == '"' && "${last}" == '"' ]]; then
      value="${value:1:${#value}-2}"
      continue
    fi
    break
  done
  printf '%s' "${value}"
}

if [[ -f "${LOCAL_ENV_FILE}" && -z "${OPENROUTER_API_KEY:-}" ]]; then
  OPENROUTER_API_KEY="$(grep -m1 '^OPENROUTER_API_KEY=' "${LOCAL_ENV_FILE}" | cut -d= -f2- || true)"
  export OPENROUTER_API_KEY
fi

if [[ -f "${LOCAL_ENV_FILE}" && -z "${KOMBAT_OPERATOR_TOKEN:-}" ]]; then
  KOMBAT_OPERATOR_TOKEN="$(grep -m1 '^KOMBAT_OPERATOR_TOKEN=' "${LOCAL_ENV_FILE}" | cut -d= -f2- || true)"
  KOMBAT_OPERATOR_TOKEN="$(strip_matching_outer_quotes "${KOMBAT_OPERATOR_TOKEN}")"
  export KOMBAT_OPERATOR_TOKEN
fi

if [[ -f "${LOCAL_ENV_FILE}" && -z "${KOMBAT_PEER_RELAY_TOKEN:-}" ]]; then
  KOMBAT_PEER_RELAY_TOKEN="$(grep -m1 '^KOMBAT_PEER_RELAY_TOKEN=' "${LOCAL_ENV_FILE}" | cut -d= -f2- || true)"
  KOMBAT_PEER_RELAY_TOKEN="$(strip_matching_outer_quotes "${KOMBAT_PEER_RELAY_TOKEN}")"
  export KOMBAT_PEER_RELAY_TOKEN
fi

exec "${SCRIPT_DIR}/.venv/bin/ansible-playbook" "$@"
