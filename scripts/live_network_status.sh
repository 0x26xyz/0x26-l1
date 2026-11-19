#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  live_network_status.sh <generated-assets-dir>

Description:
  Prints a compact live status table for the validator hosts listed in
  <generated-assets-dir>/hosts.txt, including consensus counters and the
  latest consensus telemetry event.
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

ASSET_DIR="$1"
HOSTS_FILE="${ASSET_DIR}/hosts.txt"
if [[ ! -f "${HOSTS_FILE}" ]]; then
  echo "live_network_status.sh: missing hosts.txt: ${HOSTS_FILE}" >&2
  exit 1
fi

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required tool: $1" >&2
    exit 2
  }
}
require curl
require jq

RPC_TIMEOUT_SECS="${KOMBAT_STATUS_RPC_TIMEOUT_SECS:-3}"

rpc_json() {
  local url="$1"
  local method="$2"
  local params="$3"
  curl --max-time "${RPC_TIMEOUT_SECS}" -sS -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params},\"id\":1}" \
    "${url}"
}

printf '%-12s %-22s %-6s %-8s %-8s %-8s %-9s %-10s %-10s %-24s\n' \
  "name" "url" "chain" "height" "round" "view" "peers" "timeouts" "committed" "last_event"
printf '%-12s %-22s %-6s %-8s %-8s %-8s %-9s %-10s %-10s %-24s\n' \
  "------------" "----------------------" "------" "--------" "--------" "--------" "---------" "----------" "----------" "------------------------"

while IFS=$'\t' read -r name ssh_host public_rpc_url; do
  if [[ "${name}" == "name" ]]; then
    continue
  fi

  chain_info="$(rpc_json "${public_rpc_url}" "kombat_chainInfo" "{}" 2>/dev/null || true)"
  metrics="$(rpc_json "${public_rpc_url}" "kombat_getConsensusMetrics" "{}" 2>/dev/null || true)"
  logs="$(rpc_json "${public_rpc_url}" "kombat_getConsensusLogs" '{"limit":1}' 2>/dev/null || true)"

  chain_id="$(printf '%s' "${chain_info}" | jq -r '.result.chain_id // "ERR"' 2>/dev/null || echo ERR)"
  height="$(printf '%s' "${chain_info}" | jq -r '.result.head_height // "ERR"' 2>/dev/null || echo ERR)"
  head_hash="$(printf '%s' "${chain_info}" | jq -r '.result.head_hash_hex // "ERR"' 2>/dev/null || echo ERR)"
  round="$(printf '%s' "${metrics}" | jq -r '.result.current_round // "ERR"' 2>/dev/null || echo ERR)"
  view="$(printf '%s' "${metrics}" | jq -r '.result.current_view // "ERR"' 2>/dev/null || echo ERR)"
  configured_peers="$(printf '%s' "${metrics}" | jq -r '.result.peer_count // "ERR"' 2>/dev/null || echo ERR)"
  reachable_peers="$(printf '%s' "${metrics}" | jq -r '.result.reachable_peer_count // "ERR"' 2>/dev/null || echo ERR)"
  timeouts="$(printf '%s' "${metrics}" | jq -r '.result.timeouts_total // "ERR"' 2>/dev/null || echo ERR)"
  committed="$(printf '%s' "${metrics}" | jq -r '.result.last_committed_round // "ERR"' 2>/dev/null || echo ERR)"
  views="$(printf '%s' "${metrics}" | jq -r '.result.view_changes_total // "ERR"' 2>/dev/null || echo ERR)"
  unreachable_urls="$(printf '%s' "${metrics}" | jq -r '.result.unreachable_peer_urls // [] | join(",")' 2>/dev/null || echo "")"
  outbound_peer_session_count="$(printf '%s' "${metrics}" | jq -r '.result.peer_transport_session_count // "ERR"' 2>/dev/null || echo ERR)"
  inbound_peer_session_count="$(printf '%s' "${metrics}" | jq -r '.result.inbound_peer_transport_session_count // "ERR"' 2>/dev/null || echo ERR)"
  inbound_peer_validators="$(printf '%s' "${metrics}" | jq -r '.result.inbound_peer_transport_sessions // [] | map(.remote_validator_id_hex // .remote_addr // "unknown") | join(",")' 2>/dev/null || echo "")"
  outbound_peer_state="$(printf '%s' "${metrics}" | jq -r '.result.peer_transport_sessions // [] | map((.remote_validator_id_hex // .endpoint // "unknown") + "@r" + ((.remote_current_round // "?") | tostring) + "/v" + ((.remote_current_view // "?") | tostring) + "/h" + ((.remote_head_height // "?") | tostring)) | join(",")' 2>/dev/null || echo "")"
  inbound_peer_state="$(printf '%s' "${metrics}" | jq -r '.result.inbound_peer_transport_sessions // [] | map((.remote_validator_id_hex // .remote_addr // "unknown") + "@r" + ((.remote_current_round // "?") | tostring) + "/v" + ((.remote_current_view // "?") | tostring) + "/h" + ((.remote_head_height // "?") | tostring)) | join(",")' 2>/dev/null || echo "")"
  peer_transport_backoff="$(printf '%s' "${metrics}" | jq -r '.result.peer_transport_endpoint_backoff // [] | map(.endpoint + ":failures=" + (.consecutive_failures | tostring) + ",retry_ms=" + (.retry_in_ms | tostring)) | join(";")' 2>/dev/null || echo "")"
  qc_last_ms="$(printf '%s' "${metrics}" | jq -r '.result.qc_formation_ms[-1] // "n/a"' 2>/dev/null || echo n/a)"
  qc_avg_ms="$(printf '%s' "${metrics}" | jq -r 'if (.result.qc_formation_ms | length) > 0 then ((.result.qc_formation_ms | add) / (.result.qc_formation_ms | length) | floor) else "n/a" end' 2>/dev/null || echo n/a)"
  round_last_ms="$(printf '%s' "${metrics}" | jq -r '.result.round_duration_ms[-1] // "n/a"' 2>/dev/null || echo n/a)"
  round_avg_ms="$(printf '%s' "${metrics}" | jq -r 'if (.result.round_duration_ms | length) > 0 then ((.result.round_duration_ms | add) / (.result.round_duration_ms | length) | floor) else "n/a" end' 2>/dev/null || echo n/a)"
  block_last_bytes="$(printf '%s' "${metrics}" | jq -r '.result.block_size_bytes[-1] // "n/a"' 2>/dev/null || echo n/a)"
  frontier_direct_items="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.direct_mempool_items // "ERR"' 2>/dev/null || echo ERR)"
  frontier_conditional_items="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.conditional_mempool_items // "ERR"' 2>/dev/null || echo ERR)"
  frontier_policy_count="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.tracked_policy_count // "ERR"' 2>/dev/null || echo ERR)"
  frontier_ready_items="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.tracked_ready_items // "ERR"' 2>/dev/null || echo ERR)"
  frontier_largest_policy_items="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.largest_tracked_policy_ready_items // "ERR"' 2>/dev/null || echo ERR)"
  frontier_stale_policies="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.stale_policy_count // "ERR"' 2>/dev/null || echo ERR)"
  frontier_stale_items="$(printf '%s' "${metrics}" | jq -r '.result.proposer_frontier.stale_ready_items // "ERR"' 2>/dev/null || echo ERR)"
  local_submit_queue_items="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.local_submit_queue_items // "ERR"' 2>/dev/null || echo ERR)"
  pending_relay_intents="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.pending_relay_intents // "ERR"' 2>/dev/null || echo ERR)"
  pending_relay_targets="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.pending_relay_targets // "ERR"' 2>/dev/null || echo ERR)"
  mahi_pre_admitted_pool_items="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.mahi_pre_admitted_pool_items // "ERR"' 2>/dev/null || echo ERR)"
  mahi_pre_admitted_pool_accepted_total="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.mahi_pre_admitted_pool_accepted_total // "ERR"' 2>/dev/null || echo ERR)"
  mahi_pre_admitted_pool_drained_total="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.mahi_pre_admitted_pool_drained_total // "ERR"' 2>/dev/null || echo ERR)"
  mahi_pre_admitted_pool_dropped_oldest_total="$(printf '%s' "${metrics}" | jq -r '.result.submit_queue_stats.mahi_pre_admitted_pool_dropped_oldest_total // "ERR"' 2>/dev/null || echo ERR)"
  compat_raw_wait_p95_ns="$(printf '%s' "${metrics}" | jq -r '.result.phase1_lock_profile.engine.compat_raw.wait_ns.p95_ns // "ERR"' 2>/dev/null || echo ERR)"
  compat_raw_hold_p95_ns="$(printf '%s' "${metrics}" | jq -r '.result.phase1_lock_profile.engine.compat_raw.hold_ns.p95_ns // "ERR"' 2>/dev/null || echo ERR)"
  admission_write_wait_p95_ns="$(printf '%s' "${metrics}" | jq -r '.result.phase1_lock_profile.engine.admission_write.wait_ns.p95_ns // "ERR"' 2>/dev/null || echo ERR)"
  admission_write_hold_p95_ns="$(printf '%s' "${metrics}" | jq -r '.result.phase1_lock_profile.engine.admission_write.hold_ns.p95_ns // "ERR"' 2>/dev/null || echo ERR)"
  network_round_open_hold_p95_ns="$(printf '%s' "${metrics}" | jq -r '.result.phase1_lock_profile.network_round.head_open.hold_ns.p95_ns // "ERR"' 2>/dev/null || echo ERR)"
  network_round_build_hold_p95_ns="$(printf '%s' "${metrics}" | jq -r '.result.phase1_lock_profile.network_round.build.hold_ns.p95_ns // "ERR"' 2>/dev/null || echo ERR)"
  if [[ "${configured_peers}" =~ ^[0-9]+$ && "${reachable_peers}" =~ ^[0-9]+$ ]]; then
    peers="${reachable_peers}/${configured_peers}"
    down_peers="$(( configured_peers - reachable_peers ))"
  else
    peers="ERR"
    down_peers="ERR"
  fi
  last_event="$(
    printf '%s' "${logs}" | jq -r '
      if (.result | type) == "array" and (.result | length) > 0 then
        (.result[0].kind // "unknown")
        + "@r"
        + ((.result[0].round // "ERR") | tostring)
        + "/h"
        + ((.result[0].height // "ERR") | tostring)
      else
        "ERR"
      end
    ' 2>/dev/null || echo ERR
  )"

  printf '%-12s %-22s %-6s %-8s %-8s %-8s %-6s %-10s %-10s %-24s\n' \
    "${name}" "${public_rpc_url}" "${chain_id}" "${height}" "${round}" "${view}" "${peers}" "${timeouts}" "${committed}" "${last_event}"
  printf '  view_changes: %s\n' "${views}"
  printf '  peer_reachability: reachable=%s configured=%s down=%s\n' "${reachable_peers}" "${configured_peers}" "${down_peers}"
  printf '  peer_transport_sessions: outbound=%s inbound=%s\n' "${outbound_peer_session_count}" "${inbound_peer_session_count}"
  if [[ -n "${outbound_peer_state}" ]]; then
    printf '  outbound_peer_state: %s\n' "${outbound_peer_state}"
  fi
  if [[ -n "${inbound_peer_validators}" ]]; then
    printf '  inbound_peer_validators: %s\n' "${inbound_peer_validators}"
  fi
  if [[ -n "${inbound_peer_state}" ]]; then
    printf '  inbound_peer_state: %s\n' "${inbound_peer_state}"
  fi
  if [[ -n "${peer_transport_backoff}" ]]; then
    printf '  peer_transport_backoff: %s\n' "${peer_transport_backoff}"
  fi
  printf '  latency_ms: qc_last=%s qc_avg=%s round_last=%s round_avg=%s\n' "${qc_last_ms}" "${qc_avg_ms}" "${round_last_ms}" "${round_avg_ms}"
  printf '  last_block_size_bytes: %s\n' "${block_last_bytes}"
  printf '  proposer_frontier: direct=%s conditional=%s tracked_policies=%s tracked_items=%s largest_policy_items=%s stale_policies=%s stale_items=%s\n' \
    "${frontier_direct_items}" "${frontier_conditional_items}" "${frontier_policy_count}" "${frontier_ready_items}" "${frontier_largest_policy_items}" "${frontier_stale_policies}" "${frontier_stale_items}"
  printf '  submit_queues: local_submit=%s pending_relay_intents=%s pending_relay_targets=%s\n' \
    "${local_submit_queue_items}" "${pending_relay_intents}" "${pending_relay_targets}"
  printf '  mahi_pre_admitted_pool: items=%s accepted=%s drained=%s dropped_oldest=%s\n' \
    "${mahi_pre_admitted_pool_items}" "${mahi_pre_admitted_pool_accepted_total}" "${mahi_pre_admitted_pool_drained_total}" "${mahi_pre_admitted_pool_dropped_oldest_total}"
  printf '  phase1_lock_ns: compat_wait_p95=%s compat_hold_p95=%s admission_write_wait_p95=%s admission_write_hold_p95=%s round_open_hold_p95=%s round_build_hold_p95=%s\n' \
    "${compat_raw_wait_p95_ns}" "${compat_raw_hold_p95_ns}" "${admission_write_wait_p95_ns}" "${admission_write_hold_p95_ns}" "${network_round_open_hold_p95_ns}" "${network_round_build_hold_p95_ns}"
  if [[ -n "${unreachable_urls}" ]]; then
    printf '  unreachable_peer_urls: %s\n' "${unreachable_urls}"
  fi
  printf '  head_hash: %s\n' "${head_hash}"
done < "${HOSTS_FILE}"
