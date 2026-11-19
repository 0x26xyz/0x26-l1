# Consensus Runbook

Draft M1 operator runbook for the validator lane. This is the minimum
response guide for the failure modes called out in
`docs/chain/21-engineering-plan.md`.

## Signals To Watch

The node must expose and alert on:

- `consensus_current_round`
- `consensus_current_view`
- `consensus_last_committed_round`
- `consensus_timeouts_total`
- `consensus_view_changes_total`
- `consensus_peer_count`
- `consensus_qc_formation_ms`
- `consensus_round_duration_ms`

Primary symptoms:

- round number climbing while `last_committed_round` is flat
- `timeouts_total` or `view_changes_total` rising sharply
- peer count dropping below the expected cluster size minus one
- QC and round-duration histograms stepping up together

## Validator Outage

Use when one validator disappears but the rest of the cluster is still
making progress.

1. Confirm the validator is actually missing from peer views, not just from one dashboard.
2. Check whether `consensus_last_committed_round` is still advancing on the remaining validators.
3. If the chain is advancing, leave the validator out and page the operator.
4. Verify the missing validator’s last seen round, peer count, and restart timestamp.
5. Restart only the failed validator process; do not roll the rest of the cluster.
6. After restart, confirm peer count returns to expected and the node catches up to the current head.

## Consensus Stall

Use when rounds keep changing but `last_committed_round` stops moving.

1. Compare `current_round`, `current_view`, and `timeouts_total` across all validators.
2. Identify whether the cluster is stuck on one leader or oscillating through repeated view changes.
3. Check peer-count asymmetry first; partial partition is more likely than a protocol bug.
4. If one node is isolated, treat it as a validator outage and keep the healthy quorum online.
5. If all nodes see the same peers but QC formation has stopped, capture logs and freeze the validator set before any restart.
6. Restart validators one at a time, beginning with the non-leader nodes, and verify head convergence after each restart.
7. If the cluster still cannot form QCs after a controlled restart sweep, stop block production and escalate as a launch blocker.

## Validator Restart

Use for planned restart, config reload, or recovery after crash.

1. Record the validator’s current round, committed round, and peer count before restart.
2. Ensure the remaining cluster still has quorum before taking any validator down.
3. Restart a single validator only; never restart the full cluster together in M1.
4. On boot, verify validator config loads cleanly and peer bootstrap completes.
5. Confirm the restarted validator rejoins the current round or advances to the latest committed head.
6. Check that the validator’s timeout counter stops climbing once it reconnects.

## Escalation Criteria

Escalate immediately if any of the following hold:

- two validators are down in the four-node devnet
- `last_committed_round` is flat for more than five consecutive timeout windows
- restarted validators rejoin peers but still fail to catch up
- validators disagree on the head hash after restart

This runbook is intentionally narrow. It is the M1 draft, not the final
launch-operations manual.
