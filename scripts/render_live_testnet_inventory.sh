#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  render_live_testnet_inventory.sh <output-path>

Description:
  Writes a concrete 4-host inventory file for the currently provisioned
  testnet validators using direct host RPC URLs.

  Required env:
    KOMBAT_OPERATOR_TOKEN

Optional env:
    CHAIN_ID                        default: 7
    VALIDATOR_1_RPC_URL             default: http://203.0.113.15:8545
    VALIDATOR_2_RPC_URL             default: http://203.0.113.17:8545
    VALIDATOR_3_RPC_URL             default: http://203.0.113.14:8545
    VALIDATOR_4_RPC_URL             default: http://203.0.113.16:8545
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

OUTPUT_PATH="$1"
: "${KOMBAT_OPERATOR_TOKEN:?KOMBAT_OPERATOR_TOKEN is required}"

CHAIN_ID="${CHAIN_ID:-7}"
VALIDATOR_1_RPC_URL="${VALIDATOR_1_RPC_URL:-http://203.0.113.15:8545}"
VALIDATOR_2_RPC_URL="${VALIDATOR_2_RPC_URL:-http://203.0.113.17:8545}"
VALIDATOR_3_RPC_URL="${VALIDATOR_3_RPC_URL:-http://203.0.113.14:8545}"
VALIDATOR_4_RPC_URL="${VALIDATOR_4_RPC_URL:-http://203.0.113.16:8545}"

cat > "${OUTPUT_PATH}" <<EOF
chain_id = ${CHAIN_ID}
operator_token = "${KOMBAT_OPERATOR_TOKEN}"
bootstrap_from = "validator-1"
consensus = "mahi"
mahi_commit_rule = "mahi"
mahi_observer_async_relay = true
mahi_async_block_broadcast = true
mahi_merged_execution_batch = 8
mahi_owner_relay_redundancy = 2
mahi_owner_relay_lag_rounds = 256
auto_network_round_interval_ms = 50
auto_network_drive_interval_ms = 25
auto_network_max_items = 5000
base_timeout_ms = 300
max_timeout_ms = 20000
max_timeout_exponent = 6
max_clock_skew_ms = 1000

[[validators]]
name = "validator-1"
ssh_host = "root@203.0.113.15"
public_rpc_url = "${VALIDATOR_1_RPC_URL}"
validator_id_hex = "0x0101010101010101010101010101010101010101010101010101010101010101"
consensus_signing_key_hex = "0x0101010101010101010101010101010101010101010101010101010101010101"
consensus_pubkey_hex = "0x8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c"
network_pubkey_hex = "0x212121212121212121212121212121212121212121212121"
region = "eu-west"
operator_name = "validator-one"
enabled = true
bond_asset = "USDC"
bond_amount = 100
bond_status = "Active"

[[validators]]
name = "validator-2"
ssh_host = "root@203.0.113.17"
public_rpc_url = "${VALIDATOR_2_RPC_URL}"
validator_id_hex = "0x0202020202020202020202020202020202020202020202020202020202020202"
consensus_signing_key_hex = "0x0202020202020202020202020202020202020202020202020202020202020202"
consensus_pubkey_hex = "0x8139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b394"
network_pubkey_hex = "0x222222222222222222222222222222222222222222222222"
region = "eu-west"
operator_name = "validator-two"
enabled = true
bond_asset = "USDC"
bond_amount = 100
bond_status = "Active"

[[validators]]
name = "validator-3"
ssh_host = "root@203.0.113.14"
public_rpc_url = "${VALIDATOR_3_RPC_URL}"
validator_id_hex = "0x0303030303030303030303030303030303030303030303030303030303030303"
consensus_signing_key_hex = "0x0303030303030303030303030303030303030303030303030303030303030303"
consensus_pubkey_hex = "0xed4928c628d1c2c6eae90338905995612959273a5c63f93636c14614ac8737d1"
network_pubkey_hex = "0x232323232323232323232323232323232323232323232323"
region = "eu-central"
operator_name = "validator-three"
enabled = true
bond_asset = "USDC"
bond_amount = 100
bond_status = "Active"

[[validators]]
name = "validator-4"
ssh_host = "root@203.0.113.16"
public_rpc_url = "${VALIDATOR_4_RPC_URL}"
validator_id_hex = "0x0404040404040404040404040404040404040404040404040404040404040404"
consensus_signing_key_hex = "0x0404040404040404040404040404040404040404040404040404040404040404"
consensus_pubkey_hex = "0xca93ac1705187071d67b83c7ff0efe8108e8ec4530575d7726879333dbdabe7c"
network_pubkey_hex = "0x242424242424242424242424242424242424242424242424"
region = "eu-central"
operator_name = "validator-four"
enabled = true
bond_asset = "USDC"
bond_amount = 100
bond_status = "Active"
EOF

echo "wrote live inventory to ${OUTPUT_PATH}"
