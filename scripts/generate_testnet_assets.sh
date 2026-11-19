#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  generate_testnet_assets.sh <inventory.toml> <output-dir>

Description:
  Generates:
  - validator-set.toml
  - one kombat-chain env file per validator
  - install-all.sh helper
  - start-all.sh helper
  - status-all.sh helper
  - hosts.txt inventory summary

Inventory format:
  See deploy/testnet/validator-inventory.example.toml
EOF
}

if [[ $# -ne 2 ]]; then
  usage >&2
  exit 1
fi

INVENTORY_PATH="$1"
OUTPUT_DIR="$2"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHAIN_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [[ ! -f "${INVENTORY_PATH}" ]]; then
  echo "generate_testnet_assets.sh: missing inventory file: ${INVENTORY_PATH}" >&2
  exit 1
fi

mkdir -p "${OUTPUT_DIR}"

python3 - "${INVENTORY_PATH}" "${OUTPUT_DIR}" "${CHAIN_DIR}" <<'PY'
from __future__ import annotations

import pathlib
import sys

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    print("python3 with tomllib support is required", file=sys.stderr)
    sys.exit(1)


def require(mapping, key, scope):
    value = mapping.get(key)
    if value in (None, ""):
        raise SystemExit(f"missing {scope}.{key}")
    return value


inventory_path = pathlib.Path(sys.argv[1])
output_dir = pathlib.Path(sys.argv[2])
chain_dir = pathlib.Path(sys.argv[3]).resolve()
data = tomllib.loads(inventory_path.read_text())

validators = data.get("validators")
if not isinstance(validators, list) or not validators:
    raise SystemExit("inventory.validators must be a non-empty array")

chain_id = int(data.get("chain_id", 7))
operator_token = require(data, "operator_token", "root")
bootstrap_from_name = data.get("bootstrap_from")
auto_interval = data.get("auto_network_round_interval_ms")
auto_drive_interval = data.get("auto_network_drive_interval_ms")
auto_start_at_unix_ms = data.get("auto_network_start_at_unix_ms")
auto_max_items = int(data.get("auto_network_max_items", 100))
base_timeout_ms = int(data.get("base_timeout_ms", 300))
max_timeout_ms = int(data.get("max_timeout_ms", 20_000))
max_timeout_exponent = int(data.get("max_timeout_exponent", 6))
max_clock_skew_ms = int(data.get("max_clock_skew_ms", 1_000))
consensus_mode = data.get("consensus")
mahi_commit_rule = data.get("mahi_commit_rule")
mahi_observer_async_relay = data.get("mahi_observer_async_relay")
mahi_async_block_broadcast = data.get("mahi_async_block_broadcast")
mahi_merged_execution_batch = data.get("mahi_merged_execution_batch")
mahi_owner_relay_redundancy = data.get("mahi_owner_relay_redundancy")
mahi_owner_relay_lag_rounds = data.get("mahi_owner_relay_lag_rounds")

by_name = {}
for validator in validators:
    name = require(validator, "name", "validator")
    if name in by_name:
        raise SystemExit(f"duplicate validator name: {name}")
    by_name[name] = validator

if bootstrap_from_name is None:
    bootstrap_from_name = validators[0]["name"]
if bootstrap_from_name not in by_name:
    raise SystemExit(f"bootstrap_from references unknown validator: {bootstrap_from_name}")

bootstrap_public_rpc_url = require(
    by_name[bootstrap_from_name], "public_rpc_url", bootstrap_from_name
)

validator_set_lines = [f"epoch = {chain_id}", ""]
hosts_lines = []

for validator in validators:
    name = validator["name"]
    validator_id_hex = require(validator, "validator_id_hex", name)
    consensus_pubkey_hex = require(validator, "consensus_pubkey_hex", name)
    network_pubkey_hex = require(validator, "network_pubkey_hex", name)
    public_rpc_url = require(validator, "public_rpc_url", name)
    region = require(validator, "region", name)
    operator_name = require(validator, "operator_name", name)
    enabled = str(bool(validator.get("enabled", True))).lower()
    bond_asset = require(validator, "bond_asset", name)
    bond_amount = int(validator.get("bond_amount", 100))
    bond_status = require(validator, "bond_status", name)
    ssh_host = validator.get("ssh_host", "")

    validator_set_lines.extend(
        [
            "[[validators]]",
            f'validator_id = "{validator_id_hex}"',
            f'consensus_pubkey = "{consensus_pubkey_hex}"',
            f'network_pubkey = "{network_pubkey_hex}"',
            f'rpc_endpoints = ["{public_rpc_url}"]',
            f'region = "{region}"',
            f'operator_name = "{operator_name}"',
            f"enabled = {enabled}",
            "",
            "[validators.bond]",
            f'asset = "{bond_asset}"',
            f"amount = {bond_amount}",
            f'status = "{bond_status}"',
            "",
        ]
    )

    env_lines = [
        "KOMBAT_RPC_BIND=0.0.0.0:8545",
        f"KOMBAT_CHAIN_ID={chain_id}",
        "KOMBAT_VALIDATOR_SET_PATH=/etc/and-chain/validator-set.toml",
        f'KOMBAT_VALIDATOR_ID_HEX={validator_id_hex}',
        f'KOMBAT_CONSENSUS_SIGNING_KEY_HEX={require(validator, "consensus_signing_key_hex", name)}',
        f'KOMBAT_OPERATOR_TOKEN={operator_token}',
        f'KOMBAT_PUBLIC_RPC_URL={public_rpc_url}',
        "KOMBAT_PEER_TRANSPORT_ENABLED=true",
        f"KOMBAT_BASE_TIMEOUT_MS={base_timeout_ms}",
        f"KOMBAT_MAX_TIMEOUT_MS={max_timeout_ms}",
        f"KOMBAT_MAX_TIMEOUT_EXPONENT={max_timeout_exponent}",
        f"KOMBAT_MAX_CLOCK_SKEW_MS={max_clock_skew_ms}",
    ]
    if auto_interval is not None:
        env_lines.append(
            f"KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS={int(auto_interval)}"
        )
        if auto_drive_interval is not None:
            env_lines.append(
                f"KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS={int(auto_drive_interval)}"
            )
        if auto_start_at_unix_ms is not None:
            env_lines.append(
                f"KOMBAT_DEV_AUTO_NETWORK_START_AT_UNIX_MS={int(auto_start_at_unix_ms)}"
            )
        env_lines.append(f"KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS={auto_max_items}")
    if consensus_mode is not None:
        env_lines.append(f"KOMBAT_CONSENSUS={consensus_mode}")
    if mahi_commit_rule is not None:
        env_lines.append(f"KOMBAT_MAHI_COMMIT_RULE={mahi_commit_rule}")
    if mahi_observer_async_relay is not None:
        env_lines.append(
            f"KOMBAT_MAHI_OBSERVER_ASYNC_RELAY={str(bool(mahi_observer_async_relay)).lower()}"
        )
    if mahi_async_block_broadcast is not None:
        env_lines.append(
            f"KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST={str(bool(mahi_async_block_broadcast)).lower()}"
        )
    if mahi_merged_execution_batch is not None:
        env_lines.append(
            f"KOMBAT_MAHI_MERGED_EXECUTION_BATCH={int(mahi_merged_execution_batch)}"
        )
    if mahi_owner_relay_redundancy is not None:
        env_lines.append(
            f"KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY={int(mahi_owner_relay_redundancy)}"
        )
    if mahi_owner_relay_lag_rounds is not None:
        env_lines.append(
            f"KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS={int(mahi_owner_relay_lag_rounds)}"
        )

    if name == bootstrap_from_name:
        env_lines.append("KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS=1100")
    else:
        env_lines.append(f"KOMBAT_BOOTSTRAP_RPC_URL={bootstrap_public_rpc_url}")
        env_lines.append(f"KOMBAT_BOOTSTRAP_BEARER_TOKEN={operator_token}")

    env_path = output_dir / f"{name}.env"
    env_path.write_text("\n".join(env_lines) + "\n")

    hosts_lines.append(f"{name}\t{ssh_host}\t{public_rpc_url}")

(output_dir / "validator-set.toml").write_text("\n".join(validator_set_lines))
(output_dir / "hosts.txt").write_text(
    "name\tssh_host\tpublic_rpc_url\n" + "\n".join(hosts_lines) + "\n"
)

install_lines = [
    "#!/usr/bin/env bash",
    "set -euo pipefail",
    "",
    f'CHAIN_DIR="{chain_dir}"',
    'OUT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"',
    "",
]
start_lines = [
    "#!/usr/bin/env bash",
    "set -euo pipefail",
    "",
]
status_lines = [
    "#!/usr/bin/env bash",
    "set -euo pipefail",
    "",
]
for validator in validators:
    ssh_host = validator.get("ssh_host")
    if not ssh_host:
        continue
    name = validator["name"]
    install_lines.extend(
        [
            f'echo "==> installing {name} on {ssh_host}"',
            '"${CHAIN_DIR}/scripts/install_validator_host.sh" \\',
            f'  "{ssh_host}" \\',
            f'  "${{OUT_DIR}}/{name}.env" \\',
            '  --validator-set "${OUT_DIR}/validator-set.toml"',
            "",
        ]
    )
    start_lines.extend(
        [
            f'echo "==> starting {name} on {ssh_host}"',
            f'ssh "{ssh_host}" \'systemctl start kombat-chain-snapshot.timer && systemctl restart kombat-chain && systemctl status kombat-chain --no-pager | head -20\'',
            "",
        ]
    )
    status_lines.extend(
        [
            f'echo "==> status {name} on {ssh_host}"',
            f'ssh "{ssh_host}" \'systemctl status kombat-chain --no-pager | head -20; echo; curl -s http://127.0.0.1:8545 -H "content-type: application/json" --data \'\\\'\'{{"jsonrpc":"2.0","method":"kombat_chainInfo","params":{{}},"id":1}}\'\\\'\' || true\'',
            "",
        ]
    )
(output_dir / "install-all.sh").write_text("\n".join(install_lines) + "\n")
(output_dir / "install-all.sh").chmod(0o755)
(output_dir / "start-all.sh").write_text("\n".join(start_lines) + "\n")
(output_dir / "start-all.sh").chmod(0o755)
(output_dir / "status-all.sh").write_text("\n".join(status_lines) + "\n")
(output_dir / "status-all.sh").chmod(0o755)
PY

echo "generated testnet assets in ${OUTPUT_DIR}"
