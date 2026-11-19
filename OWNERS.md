# Chain Ownership Map

This file captures the lane ownership agreed in
[`docs/chain/22-agent-kickoff-board.md`](../docs/chain/22-agent-kickoff-board.md)
for the first execution wave.

| Lane | Owner | Primary crates |
|---|---|---|
| Program / decisions | `Conductor` | workspace root, CI, crate map |
| Consensus + networking | `Atlas` | `consensus`, `network` |
| Execution + state DB | `Forge` | `execution`, `state-db`, `rpc` |
| Agent accounts + policy | `Sentinel` | `policy` |
| Intent runtime | `Relay` | `policy`, `rpc` until a dedicated intent crate exists |
| Trading core and economics | `Ledger` | `matching` |
| Oracle + market-risk inputs | `Ledger` | `oracle` |
| Observability + SDKs + indexer | `Beacon` | `streams` |
| Security / verification | `Aegis` | review and test surfaces across crates |

## Review rules

- Owners approve interface changes to their crates.
- Cross-lane work must cite the backlog item ID in the change.
- If ownership changes, update this file and the kickoff board in the same
  change.

