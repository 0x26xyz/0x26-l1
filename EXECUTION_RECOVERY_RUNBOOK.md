# Execution Recovery Runbook

Draft M2 operator runbook for state-root mismatch and bad-block recovery.

## Trigger Conditions

Use this runbook when any validator reports:

- `StateRootMismatch`
- `ParentStateRootMismatch`
- execution admission failure on a proposed block that other validators are still discussing
- a block entering local quarantine

## Expected Behavior

The validator must:

1. reject the block for voting or commit
2. keep the committed store at the last clean root
3. record the block hash in local quarantine
4. continue from the last committed state once a clean block is available

If the local store changed after a rejected block, treat that as a launch-blocking bug.

## Immediate Checks

1. Capture the rejected block hash, height, and quarantine reason.
2. Confirm the local committed root is unchanged from the last accepted block.
3. Compare the rejected block hash across validators to see whether the same bad block propagated.
4. Check whether the failure is `ParentStateRootMismatch` or `StateRootMismatch`; the operator response differs.

## Parent Root Mismatch

Use when the block references a parent root the local validator does not have committed.

1. Verify the validator is not behind the cluster’s committed head.
2. If behind, resync from the last agreed committed block before accepting more proposals.
3. If not behind, quarantine the block and compare parent root against another healthy validator.
4. Do not clear quarantine until the parent chain is confirmed or the block is discarded.

## State Root Mismatch

Use when execution from the committed parent root computes a different post-state root than the header.

1. Quarantine the block immediately.
2. Confirm no writes escaped the checkpoint boundary.
3. Re-run verification from a clean snapshot if a second validator produced the same block hash.
4. If the recomputed root still differs, keep the block quarantined and reject it from voting.
5. Accept the next valid block from the last committed root; do not mutate local committed state to “match” the bad block.

## Recovery Drill

The minimum drill for M2 is:

1. Inject a block with an invalid `state_root`.
2. Verify the node quarantines it.
3. Verify the committed root and visible state remain unchanged.
4. Submit a valid block from the same parent.
5. Confirm that block executes and commits successfully.
6. Clear the quarantine entry for the rejected block once the cluster has advanced.

## Escalation

Escalate immediately if:

- two validators disagree on the committed root after rejecting the same block
- a rejected block changes local committed state
- quarantine entries accumulate without the cluster advancing
- repeated root mismatches occur for different proposers in the same height range

This is a draft M2 runbook. It is intentionally narrow and focused on
state-root mismatch recovery only.
