#!/usr/bin/env bash
set -euo pipefail

# Testnet-only liquidity shim for the perps UI.
#
# Default mode submits session-signed intents, so quote/cross orders travel
# through the same consensus path as normal user flow. The legacy direct order
# mode remains available only as an explicit testnet escape hatch.

RPC_URL="${KOMBAT_LIQUIDITY_BOT_RPC_URL:-${RPC_URL:-https://0x26.xyz/api}}"
CHAIN_ID="${KOMBAT_CHAIN_ID:-${KOMBAT_LIQUIDITY_BOT_CHAIN_ID:-7}}"
MARKET_ID="${KOMBAT_LIQUIDITY_BOT_MARKET_ID:-7}"
MARKET_IDS_RAW="${KOMBAT_LIQUIDITY_BOT_MARKET_IDS:-$MARKET_ID}"
MAKER_ACCOUNT_ID="${KOMBAT_LIQUIDITY_BOT_MAKER_ACCOUNT_ID:-260001}"
TAKER_ACCOUNT_ID="${KOMBAT_LIQUIDITY_BOT_TAKER_ACCOUNT_ID:-260002}"
ORDER_SUBMISSION_MODE="${KOMBAT_LIQUIDITY_BOT_ORDER_SUBMISSION_MODE:-intent}"
SIGN_BIN="${KOMBAT_LIQUIDITY_BOT_SIGN_BINARY:-kombat_sign}"
MAKER_POLICY_ID="${KOMBAT_LIQUIDITY_BOT_MAKER_POLICY_ID:-26000101}"
TAKER_POLICY_ID="${KOMBAT_LIQUIDITY_BOT_TAKER_POLICY_ID:-26000201}"
MAKER_SESSION_SECRET_HEX="${KOMBAT_LIQUIDITY_BOT_MAKER_SESSION_SECRET_HEX:-}"
TAKER_SESSION_SECRET_HEX="${KOMBAT_LIQUIDITY_BOT_TAKER_SESSION_SECRET_HEX:-}"
MAKER_OWNER_SECRET_HEX="${KOMBAT_LIQUIDITY_BOT_MAKER_OWNER_SECRET_HEX:-}"
TAKER_OWNER_SECRET_HEX="${KOMBAT_LIQUIDITY_BOT_TAKER_OWNER_SECRET_HEX:-}"
POLICY_EXPIRES_AT="${KOMBAT_LIQUIDITY_BOT_POLICY_EXPIRES_AT:-99999999999999}"
POLICY_MAX_INTENTS_PER_SECOND="${KOMBAT_LIQUIDITY_BOT_POLICY_MAX_INTENTS_PER_SECOND:-1000000}"
POLICY_MAX_INTENTS_PER_DAY="${KOMBAT_LIQUIDITY_BOT_POLICY_MAX_INTENTS_PER_DAY:-1000000000}"
POLICY_MAX_VOLUME_PER_DAY="${KOMBAT_LIQUIDITY_BOT_POLICY_MAX_VOLUME_PER_DAY:-1000000000000000000000000}"
POLICY_GAS_BUDGET_PER_DAY="${KOMBAT_LIQUIDITY_BOT_POLICY_GAS_BUDGET_PER_DAY:-1000000000000000000000000}"
POLICY_GAS_REFILL_PER_DAY="${KOMBAT_LIQUIDITY_BOT_POLICY_GAS_REFILL_PER_DAY:-1000000000000000000000000}"
POLICY_MAX_DRAWDOWN_PCT="${KOMBAT_LIQUIDITY_BOT_POLICY_MAX_DRAWDOWN_PCT:-10000}"
INTENT_EXPIRES_IN_MS="${KOMBAT_LIQUIDITY_BOT_INTENT_EXPIRES_IN_MS:-3600000}"
INTENT_SUBMIT_ATTEMPTS="${KOMBAT_LIQUIDITY_BOT_INTENT_SUBMIT_ATTEMPTS:-16}"
INTENT_SUBMIT_DELAY_MS="${KOMBAT_LIQUIDITY_BOT_INTENT_SUBMIT_DELAY_MS:-500}"
QUOTE_INTERVAL_MS="${KOMBAT_LIQUIDITY_BOT_QUOTE_INTERVAL_MS:-30000}"
QUOTE_REFRESH_EVERY_CYCLES="${KOMBAT_LIQUIDITY_BOT_QUOTE_REFRESH_EVERY_CYCLES:-4}"
QUOTE_REFRESH_PRICE_MOVE_BPS="${KOMBAT_LIQUIDITY_BOT_QUOTE_REFRESH_PRICE_MOVE_BPS:-2}"
CLEANUP_OPEN_ORDERS_ON_START="${KOMBAT_LIQUIDITY_BOT_CLEANUP_OPEN_ORDERS_ON_START:-}"
SPREAD_BPS="${KOMBAT_LIQUIDITY_BOT_SPREAD_BPS:-5}"
QUOTE_LEVELS="${KOMBAT_LIQUIDITY_BOT_QUOTE_LEVELS:-10}"
LEVEL_STEP_BPS="${KOMBAT_LIQUIDITY_BOT_LEVEL_STEP_BPS:-5}"
MAKER_QUANTITY_LOTS="${KOMBAT_LIQUIDITY_BOT_MAKER_QUANTITY_LOTS:-${KOMBAT_LIQUIDITY_BOT_QUANTITY_LOTS:-5000000}}"
LEVEL_SIZE_GROWTH_BPS="${KOMBAT_LIQUIDITY_BOT_LEVEL_SIZE_GROWTH_BPS:-2500}"
TAKER_QUANTITY_LOTS="${KOMBAT_LIQUIDITY_BOT_TAKER_QUANTITY_LOTS:-1}"
CROSS_EVERY="${KOMBAT_LIQUIDITY_BOT_CROSS_EVERY:-1}"
DEFAULT_CLIENT_ORDER_ID_BASE="$((2600000000000 + $(date +%s) * 1000))"
CLIENT_ORDER_ID_BASE="${KOMBAT_LIQUIDITY_BOT_CLIENT_ORDER_ID_BASE:-$DEFAULT_CLIENT_ORDER_ID_BASE}"
DEFAULT_MM_SUBMITTER_NONCE_BASE="$((2700000000000 + $(date +%s) * 1000))"
MM_SUBMITTER_NONCE_BASE="${KOMBAT_LIQUIDITY_BOT_MM_SUBMITTER_NONCE_BASE:-$DEFAULT_MM_SUBMITTER_NONCE_BASE}"
SEED_COLLATERAL="${KOMBAT_LIQUIDITY_BOT_SEED_COLLATERAL:-1}"
SEED_POLICY_USDC="${KOMBAT_LIQUIDITY_BOT_SEED_POLICY_USDC:-0}"
COLLATERAL="${KOMBAT_LIQUIDITY_BOT_COLLATERAL:-1000000000000000}"
AUTO_COLLATERAL="${KOMBAT_LIQUIDITY_BOT_AUTO_COLLATERAL:-1}"
COLLATERAL_BUFFER_BPS="${KOMBAT_LIQUIDITY_BOT_COLLATERAL_BUFFER_BPS:-20000}"
OPERATOR_TOKEN="${KOMBAT_OPERATOR_TOKEN:-${KOMBAT_LIQUIDITY_BOT_OPERATOR_TOKEN:-}}"
MM_PROGRAM_ENABLED="${KOMBAT_LIQUIDITY_BOT_MM_PROGRAM_ENABLED:-1}"
MM_PROGRAM_NAME="${KOMBAT_LIQUIDITY_BOT_MM_PROGRAM_NAME:-testnet-prime-mm}"
MM_MIN_UPTIME_BPS="${KOMBAT_LIQUIDITY_BOT_MM_MIN_UPTIME_BPS:-9000}"
MM_MAX_SPREAD_BPS="${KOMBAT_LIQUIDITY_BOT_MM_MAX_SPREAD_BPS:-250}"
MM_MIN_QUOTE_SIZE_MICRO_USDC="${KOMBAT_LIQUIDITY_BOT_MM_MIN_QUOTE_SIZE_MICRO_USDC:-1000000000000}"
MM_MIN_TWO_SIDED_WINDOWS_PER_HOUR="${KOMBAT_LIQUIDITY_BOT_MM_MIN_TWO_SIDED_WINDOWS_PER_HOUR:-12}"
MM_REBATE_MULTIPLIER_UNITS="${KOMBAT_LIQUIDITY_BOT_MM_REBATE_MULTIPLIER_UNITS:-20000}"
MM_ENROLLMENT_BOND_MICRO_USDC="${KOMBAT_LIQUIDITY_BOT_MM_ENROLLMENT_BOND_MICRO_USDC:-10000000000}"
MM_COMPLIANCE_WINDOW_MS="${KOMBAT_LIQUIDITY_BOT_MM_COMPLIANCE_WINDOW_MS:-3600000}"
MM_OBSERVATION_CADENCE_MS="${KOMBAT_LIQUIDITY_BOT_MM_OBSERVATION_CADENCE_MS:-60000}"
MM_WAIT_TIMEOUT_MS="${KOMBAT_LIQUIDITY_BOT_MM_WAIT_TIMEOUT_MS:-60000}"
MM_BOOTSTRAP_RETRY_ATTEMPTS="${KOMBAT_LIQUIDITY_BOT_MM_BOOTSTRAP_RETRY_ATTEMPTS:-24}"
MM_BOOTSTRAP_RETRY_MS="${KOMBAT_LIQUIDITY_BOT_MM_BOOTSTRAP_RETRY_MS:-5000}"
MM_MAKER_OWNER_HEX="${KOMBAT_LIQUIDITY_BOT_MAKER_OWNER_HEX:-}"
MM_MAKER_CODE_HASH_HEX="${KOMBAT_LIQUIDITY_BOT_MAKER_CODE_HASH_HEX:-0x0909090909090909090909090909090909090909090909090909090909090909}"
MM_MAKER_EVM_ADDRESS_HEX="${KOMBAT_LIQUIDITY_BOT_MAKER_EVM_ADDRESS_HEX:-}"
TAKER_OWNER_HEX="${KOMBAT_LIQUIDITY_BOT_TAKER_OWNER_HEX:-}"
TAKER_CODE_HASH_HEX="${KOMBAT_LIQUIDITY_BOT_TAKER_CODE_HASH_HEX:-$MM_MAKER_CODE_HASH_HEX}"
TAKER_EVM_ADDRESS_HEX="${KOMBAT_LIQUIDITY_BOT_TAKER_EVM_ADDRESS_HEX:-}"
MM_POLICY_USDC="${KOMBAT_LIQUIDITY_BOT_MM_POLICY_USDC:-}"
TAKER_POLICY_USDC="${KOMBAT_LIQUIDITY_BOT_TAKER_POLICY_USDC:-$MM_POLICY_USDC}"

require_bin() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "perps_demo_liquidity_bot: missing required binary: $1" >&2
    exit 1
  fi
}

require_bin curl
require_bin jq
require_bin awk
require_bin python3

if [[ "$ORDER_SUBMISSION_MODE" != "intent" && "$ORDER_SUBMISSION_MODE" != "direct" ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_ORDER_SUBMISSION_MODE must be intent or direct" >&2
  exit 1
fi
if [[ "$ORDER_SUBMISSION_MODE" == "intent" ]]; then
  require_bin "$SIGN_BIN"
  for required in \
    MAKER_SESSION_SECRET_HEX \
    TAKER_SESSION_SECRET_HEX \
    MAKER_OWNER_SECRET_HEX \
    TAKER_OWNER_SECRET_HEX
  do
    if [[ -z "${!required}" ]]; then
      echo "perps_demo_liquidity_bot: ${required} is required in intent mode" >&2
      exit 1
    fi
  done
fi
if [[ -z "$CLEANUP_OPEN_ORDERS_ON_START" ]]; then
  if [[ "$ORDER_SUBMISSION_MODE" == "intent" ]]; then
    CLEANUP_OPEN_ORDERS_ON_START=0
  else
    CLEANUP_OPEN_ORDERS_ON_START=1
  fi
fi
if [[ "$CLEANUP_OPEN_ORDERS_ON_START" != "0" && "$CLEANUP_OPEN_ORDERS_ON_START" != "1" ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_CLEANUP_OPEN_ORDERS_ON_START must be 0 or 1" >&2
  exit 1
fi

MARKET_IDS_RAW="${MARKET_IDS_RAW//,/ }"
read -r -a MARKET_IDS <<<"$MARKET_IDS_RAW"
if [[ "${#MARKET_IDS[@]}" -eq 0 ]]; then
  echo "perps_demo_liquidity_bot: at least one market id is required" >&2
  exit 1
fi
for market_id in "${MARKET_IDS[@]}"; do
  if ! [[ "$market_id" =~ ^[0-9]+$ ]]; then
    echo "perps_demo_liquidity_bot: invalid market id: $market_id" >&2
    exit 1
  fi
done
if ! [[ "$QUOTE_LEVELS" =~ ^[0-9]+$ ]] || (( QUOTE_LEVELS == 0 )); then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_QUOTE_LEVELS must be a positive integer" >&2
  exit 1
fi
if ! [[ "$LEVEL_STEP_BPS" =~ ^[0-9]+$ ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_LEVEL_STEP_BPS must be a non-negative integer" >&2
  exit 1
fi
if ! [[ "$LEVEL_SIZE_GROWTH_BPS" =~ ^[0-9]+$ ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_LEVEL_SIZE_GROWTH_BPS must be a non-negative integer" >&2
  exit 1
fi
if ! [[ "$INTENT_SUBMIT_DELAY_MS" =~ ^[0-9]+$ ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_INTENT_SUBMIT_DELAY_MS must be a non-negative integer" >&2
  exit 1
fi
if ! [[ "$QUOTE_REFRESH_EVERY_CYCLES" =~ ^[0-9]+$ ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_QUOTE_REFRESH_EVERY_CYCLES must be a non-negative integer" >&2
  exit 1
fi
if ! [[ "$QUOTE_REFRESH_PRICE_MOVE_BPS" =~ ^[0-9]+$ ]]; then
  echo "perps_demo_liquidity_bot: KOMBAT_LIQUIDITY_BOT_QUOTE_REFRESH_PRICE_MOVE_BPS must be a non-negative integer" >&2
  exit 1
fi

declare -A NEXT_INTENT_NONCE_BY_POLICY=()
RESERVED_INTENT_NONCE=""
SUBMIT_ORDER_RESULT=""

now_ms() {
  # Avoid GNU date assumptions; seconds precision is enough for this bot.
  printf '%s000\n' "$(date +%s)"
}

account_hex32() {
  printf '0x%064x\n' "$1"
}

account_evm_address() {
  printf '0x%040x\n' "$1"
}

market_ids_json() {
  printf '%s\n' "${MARKET_IDS[@]}" | jq -Rsc 'split("\n")[:-1] | map(tonumber)'
}

sleep_ms() {
  local ms="$1"
  sleep "$(awk "BEGIN { printf \"%.3f\", ${ms} / 1000 }")"
}

throttle_intent_submission() {
  if [[ "$ORDER_SUBMISSION_MODE" == "intent" && "$INTENT_SUBMIT_DELAY_MS" -gt 0 ]]; then
    sleep_ms "$INTENT_SUBMIT_DELAY_MS"
  fi
}

transient_rpc_error() {
  local response="$1"
  local code message
  code="$(jq -r '.error.code // empty' <<<"$response" 2>/dev/null || true)"
  message="$(jq -r '.error.message // empty' <<<"$response" 2>/dev/null || true)"
  [[ "$code" == "-32000" && "$message" == "timeout recovery quorum unavailable" ]]
}

rpc_body() {
  local method="$1"
  local params="$2"
  local id="$3"
  jq -nc \
    --arg method "$method" \
    --argjson params "$params" \
    --argjson id "$id" \
    '{jsonrpc:"2.0", id:$id, method:$method, params:$params}'
}

rpc_post() {
  local body="$1"
  shift
  curl -sS \
    -H 'content-type: application/json' \
    "$@" \
    --data "$body" \
    "$RPC_URL"
}

rpc() {
  local method="$1"
  local params="$2"
  local id="${3:-1}"
  rpc_post "$(rpc_body "$method" "$params" "$id")"
}

operator_rpc() {
  local method="$1"
  local params="$2"
  local id="${3:-1}"
  if [[ -z "$OPERATOR_TOKEN" ]]; then
    echo "perps_demo_liquidity_bot: KOMBAT_OPERATOR_TOKEN not set; cannot call ${method}" >&2
    return 1
  fi
  rpc_post "$(rpc_body "$method" "$params" "$id")" \
    -H "authorization: Bearer ${OPERATOR_TOKEN}"
}

ensure_account() {
  local account_id="$1"
  local owner_hex="$2"
  local evm_address_hex="$3"
  local code_hash_hex="${4:-$MM_MAKER_CODE_HASH_HEX}"
  local response
  response="$(rpc "kombat_getAccount" \
    "$(jq -nc --argjson account_id "$account_id" '{account_id:$account_id}')" \
    40 || true)"
  if jq -e '.result.id == '"$account_id" >/dev/null 2>&1 <<<"$response"; then
    return 0
  fi

  response="$(operator_rpc "kombat_createAccount" \
    "$(jq -nc \
      --argjson account_id "$account_id" \
      --arg owner_hex "$owner_hex" \
      --arg code_hash_hex "$code_hash_hex" \
      --arg evm_address_hex "$evm_address_hex" \
      --argjson timestamp_ms "$(now_ms)" \
      '{account_id:$account_id, owner_hex:$owner_hex, code_hash_hex:$code_hash_hex, evm_address_hex:$evm_address_hex, timestamp_ms:$timestamp_ms}')" \
    41)"
  if jq -e '.error' >/dev/null 2>&1 <<<"$response"; then
    echo "perps_demo_liquidity_bot: create account failed for ${account_id}: ${response}" >&2
    return 1
  fi
}

account_owner_nonce() {
  local account_id="$1"
  local response
  response="$(rpc "kombat_getAccount" \
    "$(jq -nc --argjson account_id "$account_id" '{account_id:$account_id}')" \
    39 || true)"
  jq -er '.result.nonce // 0' <<<"$response" 2>/dev/null || printf '0\n'
}

ensure_policy() {
  local account_id="$1"
  local policy_id="$2"
  local owner_secret_hex="$3"
  local session_secret_hex="$4"
  local response owner_nonce body

  response="$(rpc "kombat_getPolicy" \
    "$(jq -nc --argjson policy_id "$policy_id" '{policy_id:$policy_id}')" \
    45 || true)"
  if jq -e '.result.id == '"$policy_id" >/dev/null 2>&1 <<<"$response"; then
    return 0
  fi

  owner_nonce="$(account_owner_nonce "$account_id")"
  body="$("$SIGN_BIN" create-policy \
    --owner-secret-hex "$owner_secret_hex" \
    --grantee-secret-hex "$session_secret_hex" \
    --account-id "$account_id" \
    --chain-id "$CHAIN_ID" \
    --nonce "$owner_nonce" \
    --policy-id "$policy_id" \
    --expires-at "$POLICY_EXPIRES_AT" \
    --timestamp-ms "$(now_ms)" \
    --gas-budget-per-day "$POLICY_GAS_BUDGET_PER_DAY" \
    --gas-refill-per-day "$POLICY_GAS_REFILL_PER_DAY" \
    --max-intents-per-second "$POLICY_MAX_INTENTS_PER_SECOND" \
    --max-intents-per-day "$POLICY_MAX_INTENTS_PER_DAY" \
    --max-volume-per-day "$POLICY_MAX_VOLUME_PER_DAY" \
    --max-drawdown-pct "$POLICY_MAX_DRAWDOWN_PCT" \
    --request-id "$policy_id")"
  response="$(rpc_post "$body")"
  if ! jq -e '.error == null and .result.accepted == true' >/dev/null 2>&1 <<<"$response"; then
    echo "perps_demo_liquidity_bot: create policy failed account=${account_id} policy=${policy_id}: ${response}" >&2
    return 1
  fi
}

set_policy_usdc_balance() {
  local address_hex="$1"
  local amount="$2"
  local response
  response="$(operator_rpc "kombat_setPolicyUsdcBalance" \
    "$(jq -nc --arg address_hex "$address_hex" --arg amount "$amount" '{address_hex:$address_hex, amount:$amount}')" \
    42)"
  if jq -e '.error' >/dev/null 2>&1 <<<"$response"; then
    echo "perps_demo_liquidity_bot: policy USDC seed failed for ${address_hex}: ${response}" >&2
    return 1
  fi
}

ensure_trade_accounts() {
  if [[ -z "$MM_MAKER_OWNER_HEX" ]]; then
    MM_MAKER_OWNER_HEX="$(account_hex32 "$MAKER_ACCOUNT_ID")"
  fi
  if [[ -z "$MM_MAKER_EVM_ADDRESS_HEX" ]]; then
    MM_MAKER_EVM_ADDRESS_HEX="$(account_evm_address "$MAKER_ACCOUNT_ID")"
  fi
  if [[ -z "$TAKER_OWNER_HEX" ]]; then
    TAKER_OWNER_HEX="$(account_hex32 "$TAKER_ACCOUNT_ID")"
  fi
  if [[ -z "$TAKER_EVM_ADDRESS_HEX" ]]; then
    TAKER_EVM_ADDRESS_HEX="$(account_evm_address "$TAKER_ACCOUNT_ID")"
  fi
  if [[ -z "$MM_POLICY_USDC" ]]; then
    MM_POLICY_USDC="10000000000000"
  fi
  if [[ -z "$TAKER_POLICY_USDC" ]]; then
    TAKER_POLICY_USDC="$MM_POLICY_USDC"
  fi

  ensure_account "$MAKER_ACCOUNT_ID" "$MM_MAKER_OWNER_HEX" "$MM_MAKER_EVM_ADDRESS_HEX" "$MM_MAKER_CODE_HASH_HEX"
  ensure_account "$TAKER_ACCOUNT_ID" "$TAKER_OWNER_HEX" "$TAKER_EVM_ADDRESS_HEX" "$TAKER_CODE_HASH_HEX"

  if [[ "$SEED_POLICY_USDC" == "1" ]]; then
    set_policy_usdc_balance "$MM_MAKER_EVM_ADDRESS_HEX" "$MM_POLICY_USDC"
    set_policy_usdc_balance "$TAKER_EVM_ADDRESS_HEX" "$TAKER_POLICY_USDC"
  fi

  if [[ "$ORDER_SUBMISSION_MODE" == "intent" ]]; then
    ensure_policy "$MAKER_ACCOUNT_ID" "$MAKER_POLICY_ID" "$MAKER_OWNER_SECRET_HEX" "$MAKER_SESSION_SECRET_HEX"
    ensure_policy "$TAKER_ACCOUNT_ID" "$TAKER_POLICY_ID" "$TAKER_OWNER_SECRET_HEX" "$TAKER_SESSION_SECRET_HEX"
  fi
}

define_market_maker_program() {
  local response program_id reason attempt
  for ((attempt = 1; attempt <= MM_BOOTSTRAP_RETRY_ATTEMPTS; attempt++)); do
    response="$(operator_rpc "kombat_defineMarketMakerProgram" \
      "$(jq -nc \
        --arg name "$MM_PROGRAM_NAME" \
        --argjson markets "$(market_ids_json)" \
        --argjson min_uptime_bps "$MM_MIN_UPTIME_BPS" \
        --argjson max_spread_bps "$MM_MAX_SPREAD_BPS" \
        --arg min_quote_size_micro_usdc "$MM_MIN_QUOTE_SIZE_MICRO_USDC" \
        --argjson min_two_sided_windows_per_hour "$MM_MIN_TWO_SIDED_WINDOWS_PER_HOUR" \
        --argjson rebate_multiplier_units "$MM_REBATE_MULTIPLIER_UNITS" \
        --arg enrollment_bond_micro_usdc "$MM_ENROLLMENT_BOND_MICRO_USDC" \
        --argjson compliance_window_ms "$MM_COMPLIANCE_WINDOW_MS" \
        --argjson observation_cadence_ms "$MM_OBSERVATION_CADENCE_MS" \
        --argjson timestamp_ms "$(now_ms)" \
        --argjson submitter_nonce "$((MM_SUBMITTER_NONCE_BASE + 1))" \
        --argjson wait_timeout_ms "$MM_WAIT_TIMEOUT_MS" \
        '{
          name:$name,
          markets:$markets,
          min_uptime_bps:$min_uptime_bps,
          max_spread_bps:$max_spread_bps,
          min_quote_size_micro_usdc:$min_quote_size_micro_usdc,
          min_two_sided_windows_per_hour:$min_two_sided_windows_per_hour,
          rebate_multiplier_units:$rebate_multiplier_units,
          enrollment_bond_micro_usdc:$enrollment_bond_micro_usdc,
          compliance_window_ms:$compliance_window_ms,
          observation_cadence_ms:$observation_cadence_ms,
          timestamp_ms:$timestamp_ms,
          submitter_nonce:$submitter_nonce,
          wait_timeout_ms:$wait_timeout_ms
        }')" \
      43)"

    if ! transient_rpc_error "$response"; then
      break
    fi
    echo "perps_demo_liquidity_bot: define MM program waiting for RPC quorum attempt=${attempt}/${MM_BOOTSTRAP_RETRY_ATTEMPTS}" >&2
    sleep_ms "$MM_BOOTSTRAP_RETRY_MS"
  done

  if jq -e '.error' >/dev/null 2>&1 <<<"$response"; then
    reason="$(jq -r '.error.data.reason // empty' <<<"$response")"
    program_id="$(jq -r '.error.data.program_id_hex // empty' <<<"$response")"
    if [[ "$reason" == "program_already_active" && -n "$program_id" ]]; then
      printf '%s\n' "$program_id"
      return 0
    fi
    echo "perps_demo_liquidity_bot: define MM program failed: ${response}" >&2
    return 1
  fi

  program_id="$(jq -r '.result.program_id_hex // .result.action_status.receipt.program_id_hex // empty' <<<"$response")"
  if [[ -z "$program_id" || "$program_id" == "null" ]]; then
    echo "perps_demo_liquidity_bot: define MM program did not return program_id_hex: ${response}" >&2
    return 1
  fi
  printf '%s\n' "$program_id"
}

enroll_market_maker_program() {
  local program_id_hex="$1"
  local market_id="$2"
  local response reason attempt
  for ((attempt = 1; attempt <= MM_BOOTSTRAP_RETRY_ATTEMPTS; attempt++)); do
    response="$(operator_rpc "kombat_enrollMarketMakerProgram" \
      "$(jq -nc \
        --argjson market_maker "$MAKER_ACCOUNT_ID" \
        --argjson market_id "$market_id" \
        --arg program_id_hex "$program_id_hex" \
        --arg bond_micro_usdc "$MM_ENROLLMENT_BOND_MICRO_USDC" \
        --argjson timestamp_ms "$(now_ms)" \
        --argjson submitter_nonce "$((MM_SUBMITTER_NONCE_BASE + 100 + market_id))" \
        --argjson wait_timeout_ms "$MM_WAIT_TIMEOUT_MS" \
        '{
          market_maker:$market_maker,
          market_id:$market_id,
          program_id_hex:$program_id_hex,
          bond_micro_usdc:$bond_micro_usdc,
          timestamp_ms:$timestamp_ms,
          submitter_nonce:$submitter_nonce,
          wait_timeout_ms:$wait_timeout_ms
        }')" \
      "$((44 + market_id))")"

    if ! transient_rpc_error "$response"; then
      break
    fi
    echo "perps_demo_liquidity_bot: enroll MM program waiting for RPC quorum market=${market_id} attempt=${attempt}/${MM_BOOTSTRAP_RETRY_ATTEMPTS}" >&2
    sleep_ms "$MM_BOOTSTRAP_RETRY_MS"
  done
  if jq -e '.error' >/dev/null 2>&1 <<<"$response"; then
    reason="$(jq -r '.error.data.reason // empty' <<<"$response")"
    if [[ "$reason" == "enrollment_already_live" ]]; then
      return 0
    fi
    echo "perps_demo_liquidity_bot: enroll MM program failed market=${market_id}: ${response}" >&2
    return 1
  fi
}

ensure_market_maker_program() {
  if [[ "$MM_PROGRAM_ENABLED" != "1" ]]; then
    return 0
  fi

  local program_id market_id
  program_id="$(define_market_maker_program)"
  for market_id in "${MARKET_IDS[@]}"; do
    enroll_market_maker_program "$program_id" "$market_id"
  done
  echo "perps_demo_liquidity_bot: RFC027 MM program=${program_id} enrolled maker=${MAKER_ACCOUNT_ID} markets=${MARKET_IDS[*]}" >&2
}

json_or_empty_object() {
  local value="${1:-}"
  if [[ -n "$value" ]] && jq -e . >/dev/null 2>&1 <<<"$value"; then
    printf '%s\n' "$value"
  else
    printf '{}\n'
  fi
}

unchanged_order_json() {
  local client_order_id="$1"
  jq -cn --argjson client_order_id "$client_order_id" \
    '{status:"unchanged", client_order_id:$client_order_id}'
}

get_mark_price_ticks() {
  local market_id="$1"
  rpc "kombat_getMarketSnapshot" \
    "$(jq -nc --argjson market_id "$market_id" '{market_id:$market_id, depth:1}')" \
    10 \
    | jq -er '
      if .error then empty
      else (.result.state.mark_price_ticks // .result.state.index_price_ticks)
      end
    '
}

get_market_initial_margin_bps() {
  local market_id="$1"
  rpc "kombat_getMarket" \
    "$(jq -nc --argjson market_id "$market_id" '{market_id:$market_id}')" \
    11 \
    | jq -er '
      if .error then empty
      else (.result.initial_margin_bps // 1000)
      end
    '
}

maker_collateral_for_market() {
  local market_id="$1"
  if [[ "$AUTO_COLLATERAL" != "1" ]]; then
    printf '%s\n' "$COLLATERAL"
    return 0
  fi

  local mark initial_margin_bps
  if ! mark="$(get_mark_price_ticks "$market_id")"; then
    printf '%s\n' "$COLLATERAL"
    return 0
  fi
  if ! initial_margin_bps="$(get_market_initial_margin_bps "$market_id")"; then
    initial_margin_bps=1000
  fi

  python3 - "$mark" "$initial_margin_bps" "$SPREAD_BPS" "$LEVEL_STEP_BPS" \
    "$MAKER_QUANTITY_LOTS" "$LEVEL_SIZE_GROWTH_BPS" "$QUOTE_LEVELS" \
    "$COLLATERAL" "$COLLATERAL_BUFFER_BPS" <<'PY'
import sys

(
    mark,
    initial_margin_bps,
    spread_bps,
    level_step_bps,
    base_quantity_lots,
    level_size_growth_bps,
    quote_levels,
    collateral_floor,
    collateral_buffer_bps,
) = map(int, sys.argv[1:])

raw_notional = 0
for level in range(1, quote_levels + 1):
    level_bps = spread_bps + (level - 1) * level_step_bps
    spread = max(mark * level_bps // 10_000, 1)
    bid = max(mark - spread, 1)
    ask = mark + spread
    quantity_lots = (
        base_quantity_lots
        * (10_000 + (level - 1) * level_size_growth_bps)
        // 10_000
    )
    quantity_lots = max(quantity_lots, 1)
    raw_notional += (bid + ask) * quantity_lots

required_initial = (raw_notional * initial_margin_bps + 9_999) // 10_000
buffered = (required_initial * collateral_buffer_bps + 9_999) // 10_000
print(max(collateral_floor, buffered))
PY
}

set_collateral() {
  local market_id="$1"
  local account_id="$2"
  local amount="${3:-$COLLATERAL}"
  local response
  response="$(operator_rpc "kombat_setIsolatedCollateral" \
    "$(jq -nc \
      --argjson market_id "$market_id" \
      --argjson account_id "$account_id" \
      --arg collateral "$amount" \
      --argjson timestamp_ms "$(now_ms)" \
      '{market_id:$market_id, account_id:$account_id, collateral:$collateral, timestamp_ms:$timestamp_ms}')" \
    20)"
  if jq -e '.error' >/dev/null 2>&1 <<<"$response"; then
    echo "perps_demo_liquidity_bot: collateral seed failed for account ${account_id}: ${response}" >&2
    return 1
  fi
}

policy_id_for_account() {
  local account_id="$1"
  if [[ "$account_id" == "$MAKER_ACCOUNT_ID" ]]; then
    printf '%s\n' "$MAKER_POLICY_ID"
  elif [[ "$account_id" == "$TAKER_ACCOUNT_ID" ]]; then
    printf '%s\n' "$TAKER_POLICY_ID"
  else
    echo "perps_demo_liquidity_bot: no policy configured for account ${account_id}" >&2
    return 1
  fi
}

session_secret_for_account() {
  local account_id="$1"
  if [[ "$account_id" == "$MAKER_ACCOUNT_ID" ]]; then
    printf '%s\n' "$MAKER_SESSION_SECRET_HEX"
  elif [[ "$account_id" == "$TAKER_ACCOUNT_ID" ]]; then
    printf '%s\n' "$TAKER_SESSION_SECRET_HEX"
  else
    echo "perps_demo_liquidity_bot: no session secret configured for account ${account_id}" >&2
    return 1
  fi
}

refresh_intent_nonce() {
  local policy_id="$1"
  local response nonce
  response="$(rpc "kombat_getPolicy" \
    "$(jq -nc --argjson policy_id "$policy_id" '{policy_id:$policy_id}')" \
    46)"
  nonce="$(jq -er '.result.next_intent_nonce' <<<"$response" 2>/dev/null)" || {
    echo "perps_demo_liquidity_bot: unable to read next intent nonce for policy ${policy_id}: ${response}" >&2
    return 1
  }
  NEXT_INTENT_NONCE_BY_POLICY["$policy_id"]="$nonce"
}

reserve_intent_nonce() {
  local policy_id="$1"
  if [[ -z "${NEXT_INTENT_NONCE_BY_POLICY[$policy_id]:-}" ]]; then
    refresh_intent_nonce "$policy_id"
  fi
  RESERVED_INTENT_NONCE="${NEXT_INTENT_NONCE_BY_POLICY[$policy_id]}"
  NEXT_INTENT_NONCE_BY_POLICY["$policy_id"]=$((RESERVED_INTENT_NONCE + 1))
}

rewind_reserved_intent_nonce() {
  local policy_id="$1"
  if [[ -n "$RESERVED_INTENT_NONCE" && "${NEXT_INTENT_NONCE_BY_POLICY[$policy_id]:-0}" -gt "$RESERVED_INTENT_NONCE" ]]; then
    NEXT_INTENT_NONCE_BY_POLICY["$policy_id"]="$RESERVED_INTENT_NONCE"
  fi
}

response_error_reason() {
  local response="$1"
  jq -r '.error.data.reason // empty' <<<"$response" 2>/dev/null || true
}

response_nonce_frontier_error() {
  local response="$1"
  local reason
  reason="$(response_error_reason "$response")"
  [[ "$reason" == "invalid_intent_nonce" || "$reason" == "stale_intent_nonce" ]]
}

response_duplicate_nonce_error() {
  [[ "$(response_error_reason "$1")" == "duplicate_policy_nonce" ]]
}

cancel_order_direct() {
  local market_id="$1"
  local account_id="$2"
  local client_order_id="$3"
  rpc "kombat_cancelOrder" \
    "$(jq -nc \
      --argjson market_id "$market_id" \
      --argjson account_id "$account_id" \
      --argjson client_order_id "$client_order_id" \
      --argjson timestamp_ms "$(now_ms)" \
      '{market_id:$market_id, account_id:$account_id, client_order_id:$client_order_id, timestamp_ms:$timestamp_ms}')" \
    30 >/dev/null 2>&1 || true
}

cancel_order_intent() {
  local market_id="$1"
  local account_id="$2"
  local client_order_id="$3"
  local policy_id session_secret timestamp_ms expires_at body response nonce_attempt submit_attempt
  policy_id="$(policy_id_for_account "$account_id")" || return 0
  session_secret="$(session_secret_for_account "$account_id")" || return 0

  for ((nonce_attempt = 1; nonce_attempt <= INTENT_SUBMIT_ATTEMPTS; nonce_attempt++)); do
    reserve_intent_nonce "$policy_id" || return 0
    timestamp_ms="$(now_ms)"
    expires_at=$((timestamp_ms + INTENT_EXPIRES_IN_MS))
    body="$("$SIGN_BIN" cancel-order-intent \
      --session-secret-hex "$session_secret" \
      --account-id "$account_id" \
      --policy-id "$policy_id" \
      --chain-id "$CHAIN_ID" \
      --nonce "$RESERVED_INTENT_NONCE" \
      --expires-at "$expires_at" \
      --market-id "$market_id" \
      --client-order-id "$client_order_id" \
      --timestamp-ms "$timestamp_ms" \
      --request-id "$client_order_id")"

    for ((submit_attempt = 1; submit_attempt <= INTENT_SUBMIT_ATTEMPTS; submit_attempt++)); do
      response="$(rpc_post "$body" || true)"
      throttle_intent_submission
      if jq -e '.error == null and .result' >/dev/null 2>&1 <<<"$response"; then
        return 0
      fi
      if transient_rpc_error "$response" && (( submit_attempt < INTENT_SUBMIT_ATTEMPTS )); then
        continue
      fi
      break
    done

    if response_duplicate_nonce_error "$response"; then
      return 0
    fi
    if response_nonce_frontier_error "$response" && (( nonce_attempt < INTENT_SUBMIT_ATTEMPTS )); then
      refresh_intent_nonce "$policy_id" || return 0
      continue
    fi
    if transient_rpc_error "$response"; then
      rewind_reserved_intent_nonce "$policy_id"
    fi
    echo "perps_demo_liquidity_bot: cancel intent failed account=${account_id} order=${client_order_id}: ${response}" >&2
    return 0
  done
}

cancel_order() {
  if [[ "$ORDER_SUBMISSION_MODE" == "intent" ]]; then
    cancel_order_intent "$@"
  else
    cancel_order_direct "$@"
  fi
}

cleanup_maker_open_orders() {
  local market_id="$1"
  local response ids
  response="$(rpc "kombat_getOpenOrders" \
    "$(jq -nc \
      --argjson market_id "$market_id" \
      --argjson account_id "$MAKER_ACCOUNT_ID" \
      '{market_id:$market_id, account_id:$account_id, limit:1000}')" \
    25 || true)"
  ids="$(jq -r '.result[]?.client_order_id // empty' <<<"$response" 2>/dev/null || true)"
  if [[ -z "$ids" ]]; then
    return 0
  fi
  while IFS= read -r client_order_id; do
    [[ -z "$client_order_id" ]] && continue
    cancel_order "$market_id" "$MAKER_ACCOUNT_ID" "$client_order_id"
  done <<<"$ids"
}

submit_order_direct() {
  local market_id="$1"
  local account_id="$2"
  local client_order_id="$3"
  local side="$4"
  local price_ticks="$5"
  local tif="$6"
  local post_only="$7"
  local quantity_lots="$8"
  local response

  response="$(rpc "kombat_submitOrder" \
    "$(jq -nc \
      --argjson market_id "$market_id" \
      --argjson account_id "$account_id" \
      --argjson client_order_id "$client_order_id" \
      --arg side "$side" \
      --argjson quantity_lots "$quantity_lots" \
      --argjson price_ticks "$price_ticks" \
      --arg tif "$tif" \
      --argjson post_only "$post_only" \
      --argjson timestamp_ms "$(now_ms)" \
      '{order:{
        market_id:$market_id,
        account_id:$account_id,
        client_order_id:$client_order_id,
        side:$side,
        quantity_lots:$quantity_lots,
        order_kind:"limit",
        price_ticks:$price_ticks,
        time_in_force:{tif:$tif},
        post_only:$post_only
      }, timestamp_ms:$timestamp_ms}')" \
    "$client_order_id")"

  if jq -e '.error' >/dev/null 2>&1 <<<"$response"; then
    echo "perps_demo_liquidity_bot: submit failed account=${account_id} order=${client_order_id} side=${side} price=${price_ticks}: ${response}" >&2
    SUBMIT_ORDER_RESULT=""
    return 1
  fi

  SUBMIT_ORDER_RESULT="$(jq -rc '.result | {status, filled_lots, remaining_lots, posted_order_id}' <<<"$response")"
}

submit_order_intent() {
  local market_id="$1"
  local account_id="$2"
  local client_order_id="$3"
  local side="$4"
  local price_ticks="$5"
  local tif="$6"
  local post_only="$7"
  local quantity_lots="$8"
  local policy_id session_secret timestamp_ms expires_at body response nonce_attempt submit_attempt
  local post_only_args=()

  policy_id="$(policy_id_for_account "$account_id")" || return 1
  session_secret="$(session_secret_for_account "$account_id")" || return 1
  if [[ "$post_only" == "true" ]]; then
    post_only_args=(--post-only)
  fi

  for ((nonce_attempt = 1; nonce_attempt <= INTENT_SUBMIT_ATTEMPTS; nonce_attempt++)); do
    reserve_intent_nonce "$policy_id" || return 1
    timestamp_ms="$(now_ms)"
    expires_at=$((timestamp_ms + INTENT_EXPIRES_IN_MS))
    body="$("$SIGN_BIN" place-order-intent \
      --session-secret-hex "$session_secret" \
      --account-id "$account_id" \
      --policy-id "$policy_id" \
      --chain-id "$CHAIN_ID" \
      --nonce "$RESERVED_INTENT_NONCE" \
      --expires-at "$expires_at" \
      --market-id "$market_id" \
      --client-order-id "$client_order_id" \
      --side "$side" \
      --quantity-lots "$quantity_lots" \
      --order-kind limit \
      --price-ticks "$price_ticks" \
      --tif "$tif" \
      "${post_only_args[@]}" \
      --timestamp-ms "$timestamp_ms" \
      --request-id "$client_order_id")"

    for ((submit_attempt = 1; submit_attempt <= INTENT_SUBMIT_ATTEMPTS; submit_attempt++)); do
      response="$(rpc_post "$body" || true)"
      throttle_intent_submission
      if jq -e '.error == null and .result' >/dev/null 2>&1 <<<"$response"; then
        SUBMIT_ORDER_RESULT="$(jq -rc '.result | {intent_id_hex, class, pending_reason, current_policy_nonce}' <<<"$response")"
        return 0
      fi
      if transient_rpc_error "$response" && (( submit_attempt < INTENT_SUBMIT_ATTEMPTS )); then
        continue
      fi
      break
    done

    if response_duplicate_nonce_error "$response"; then
      SUBMIT_ORDER_RESULT="$(jq -cn --argjson nonce "$RESERVED_INTENT_NONCE" '{status:"duplicate_policy_nonce", nonce:$nonce}')"
      return 0
    fi
    if response_nonce_frontier_error "$response" && (( nonce_attempt < INTENT_SUBMIT_ATTEMPTS )); then
      refresh_intent_nonce "$policy_id" || return 1
      continue
    fi
    if transient_rpc_error "$response"; then
      rewind_reserved_intent_nonce "$policy_id"
    fi
    echo "perps_demo_liquidity_bot: submit intent failed account=${account_id} order=${client_order_id} side=${side} price=${price_ticks}: ${response}" >&2
    SUBMIT_ORDER_RESULT=""
    return 1
  done
}

submit_order() {
  if [[ "$ORDER_SUBMISSION_MODE" == "intent" ]]; then
    submit_order_intent "$@"
  else
    submit_order_direct "$@"
  fi
}

main() {
  echo "perps_demo_liquidity_bot: rpc=${RPC_URL} markets=${MARKET_IDS[*]} maker=${MAKER_ACCOUNT_ID} taker=${TAKER_ACCOUNT_ID} order_mode=${ORDER_SUBMISSION_MODE}" >&2

  ensure_trade_accounts
  ensure_market_maker_program
  if [[ "$ORDER_SUBMISSION_MODE" == "intent" ]]; then
    refresh_intent_nonce "$MAKER_POLICY_ID"
    refresh_intent_nonce "$TAKER_POLICY_ID"
    echo "perps_demo_liquidity_bot: intent policies maker=${MAKER_POLICY_ID} taker=${TAKER_POLICY_ID}" >&2
  fi

  if [[ "$SEED_COLLATERAL" == "1" ]]; then
    local market_id maker_collateral
    for market_id in "${MARKET_IDS[@]}"; do
      maker_collateral="$(maker_collateral_for_market "$market_id")"
      set_collateral "$market_id" "$MAKER_ACCOUNT_ID" "$maker_collateral"
      set_collateral "$market_id" "$TAKER_ACCOUNT_ID"
      echo "perps_demo_liquidity_bot: seeded isolated collateral market=${market_id} maker=${maker_collateral} taker=${COLLATERAL}" >&2
    done
  fi

  local market_id
  if [[ "$CLEANUP_OPEN_ORDERS_ON_START" == "1" ]]; then
    for market_id in "${MARKET_IDS[@]}"; do
      cleanup_maker_open_orders "$market_id"
    done
  else
    echo "perps_demo_liquidity_bot: startup open-order cleanup disabled" >&2
  fi

  local i=0
  declare -A last_bid_ids=()
  declare -A last_ask_ids=()
  declare -A last_quote_mark_by_market=()
  while true; do
    local market_index=0
    for market_id in "${MARKET_IDS[@]}"; do
      local mark taker_id taker_side taker_price taker_order_offset best_bid best_ask refresh_market
      if ! mark="$(get_mark_price_ticks "$market_id")"; then
        echo "perps_demo_liquidity_bot: mark/index unavailable market=${market_id}; retrying" >&2
        market_index=$((market_index + 1))
        continue
      fi

      local level maker_bids_json="[]" maker_asks_json="[]" taker_result="{}"
      best_bid=0
      best_ask=0
      refresh_market=1
      if [[ -n "${last_quote_mark_by_market[$market_id]:-}" ]]; then
        local last_quote_mark mark_move_bps
        refresh_market=0
        last_quote_mark="${last_quote_mark_by_market[$market_id]}"
        if (( QUOTE_REFRESH_EVERY_CYCLES > 0 && i % QUOTE_REFRESH_EVERY_CYCLES == 0 )); then
          refresh_market=1
        fi
        mark_move_bps="$(python3 - "$last_quote_mark" "$mark" <<'PY'
import sys
last_mark = int(sys.argv[1])
mark = int(sys.argv[2])
if last_mark <= 0:
    print(10_000)
else:
    print(abs(mark - last_mark) * 10_000 // last_mark)
PY
)"
        if (( QUOTE_REFRESH_PRICE_MOVE_BPS > 0 && mark_move_bps >= QUOTE_REFRESH_PRICE_MOVE_BPS )); then
          refresh_market=1
        fi
      fi
      for ((level = 1; level <= QUOTE_LEVELS; level++)); do
        local level_bps spread bid ask bid_id ask_id order_offset key level_quantity_lots bid_result ask_result old_bid_id old_ask_id
        level_bps=$((SPREAD_BPS + (level - 1) * LEVEL_STEP_BPS))
        spread=$((mark * level_bps / 10000))
        if (( spread < 1 )); then
          spread=1
        fi
        bid=$((mark - spread))
        if (( bid < 1 )); then
          bid=1
        fi
        ask=$((mark + spread))
        if (( level == 1 )); then
          best_bid="$bid"
          best_ask="$ask"
        fi

        order_offset=$((i * 100000 + market_index * 1000 + level * 10))
        bid_id=$((CLIENT_ORDER_ID_BASE + order_offset + 1))
        ask_id=$((CLIENT_ORDER_ID_BASE + order_offset + 2))
        key="${market_id}:${level}"
        level_quantity_lots=$((MAKER_QUANTITY_LOTS * (10000 + (level - 1) * LEVEL_SIZE_GROWTH_BPS) / 10000))
        if (( level_quantity_lots < 1 )); then
          level_quantity_lots=1
        fi

        old_bid_id="${last_bid_ids[$key]:-}"
        old_ask_id="${last_ask_ids[$key]:-}"
        if (( refresh_market == 1 )) || [[ -z "$old_bid_id" ]]; then
          SUBMIT_ORDER_RESULT=""
          if submit_order "$market_id" "$MAKER_ACCOUNT_ID" "$bid_id" bid "$bid" gtc true "$level_quantity_lots"; then
            bid_result="$SUBMIT_ORDER_RESULT"
            last_bid_ids[$key]="$bid_id"
            if [[ -n "$old_bid_id" ]]; then
              cancel_order "$market_id" "$MAKER_ACCOUNT_ID" "$old_bid_id"
            fi
          else
            bid_result=""
          fi
        else
          bid_result="$(unchanged_order_json "$old_bid_id")"
        fi

        if (( refresh_market == 1 )) || [[ -z "$old_ask_id" ]]; then
          SUBMIT_ORDER_RESULT=""
          if submit_order "$market_id" "$MAKER_ACCOUNT_ID" "$ask_id" ask "$ask" gtc true "$level_quantity_lots"; then
            ask_result="$SUBMIT_ORDER_RESULT"
            last_ask_ids[$key]="$ask_id"
            if [[ -n "$old_ask_id" ]]; then
              cancel_order "$market_id" "$MAKER_ACCOUNT_ID" "$old_ask_id"
            fi
          else
            ask_result=""
          fi
        else
          ask_result="$(unchanged_order_json "$old_ask_id")"
        fi

        maker_bids_json="$(jq -cn \
          --argjson levels "$maker_bids_json" \
          --argjson level "$level" \
          --argjson price_ticks "$bid" \
          --argjson offset_bps "$level_bps" \
          --argjson quantity_lots "$level_quantity_lots" \
          --argjson order "$(json_or_empty_object "$bid_result")" \
          '$levels + [{level:$level, offset_bps:$offset_bps, price_ticks:$price_ticks, quantity_lots:$quantity_lots, order:$order}]')"
        maker_asks_json="$(jq -cn \
          --argjson levels "$maker_asks_json" \
          --argjson level "$level" \
          --argjson price_ticks "$ask" \
          --argjson offset_bps "$level_bps" \
          --argjson quantity_lots "$level_quantity_lots" \
          --argjson order "$(json_or_empty_object "$ask_result")" \
          '$levels + [{level:$level, offset_bps:$offset_bps, price_ticks:$price_ticks, quantity_lots:$quantity_lots, order:$order}]')"
      done
      if (( refresh_market == 1 )); then
        last_quote_mark_by_market[$market_id]="$mark"
      fi

      if (( CROSS_EVERY > 0 && i % CROSS_EVERY == 0 )); then
        if (( i % 2 == 0 )); then
          taker_side=bid
          taker_price="$best_ask"
        else
          taker_side=ask
          taker_price="$best_bid"
        fi
        taker_order_offset=$((i * 100000 + market_index * 1000 + 999))
        taker_id=$((CLIENT_ORDER_ID_BASE + taker_order_offset))
        SUBMIT_ORDER_RESULT=""
        submit_order "$market_id" "$TAKER_ACCOUNT_ID" "$taker_id" "$taker_side" "$taker_price" ioc false "$TAKER_QUANTITY_LOTS" || true
        taker_result="$SUBMIT_ORDER_RESULT"
      fi

      jq -nc \
        --argjson iteration "$i" \
        --argjson market_id "$market_id" \
        --argjson mark "$mark" \
        --argjson best_bid "$best_bid" \
        --argjson best_ask "$best_ask" \
        --argjson quote_levels "$QUOTE_LEVELS" \
        --argjson maker_bids "$maker_bids_json" \
        --argjson maker_asks "$maker_asks_json" \
        --argjson taker "$(json_or_empty_object "$taker_result")" \
        '{iteration:$iteration, market_id:$market_id, mark_price_ticks:$mark, best_bid:$best_bid, best_ask:$best_ask, quote_levels:$quote_levels, maker_bids:$maker_bids, maker_asks:$maker_asks, taker:$taker}'

      market_index=$((market_index + 1))
    done

    i=$((i + 1))
    sleep_ms "$QUOTE_INTERVAL_MS"
  done
}

main "$@"
