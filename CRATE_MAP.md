# Chain Crate Map

This map is the implementation companion to
[`docs/chain/21-engineering-plan.md`](../docs/chain/21-engineering-plan.md)
and [`docs/chain/backlog.md`](../docs/chain/backlog.md).

## Crates

| Crate | Purpose | Upstream docs | Early backlog items |
|---|---|---|---|
| `consensus` | block proposal, voting, QC validation, pacemaker | RFC 001, RFC 002 | `CH-002` to `CH-010` |
| `execution` | block execution driver, `revm` shell, rollback boundaries | RFC 003, RFC 004 | `CH-011` to `CH-017` |
| `state-db` | storage traits, snapshots, committed-root pipeline | RFC 003 | `CH-011` to `CH-014` |
| `network` | peer transport, validator messaging, bootstrap surfaces | RFC 002 | `CH-005` to `CH-010` |
| `rpc` | chain info, contract calls, intent submission surfaces | RFC 003, RFC 008 | `CH-015`, `CH-025` |
| `policy` | standard account, policy state machine, session-key admission | RFC 005, RFC 006, RFC 007 | `CH-018` to `CH-023` |
| `matching` | order matching, fills, positions, fees, funding hooks | RFC 011, RFC 012, RFC 013 | `CH-031` to `CH-040` |
| `oracle` | validator mark submission, clipping, stale-mark handling | RFC 014 | `CH-037` to `CH-040` |
| `streams` | event commitment, watch/webhook gateway, stream server shell | RFC 015, RFC 016, RFC 017 | `CH-041` to `CH-046` |
| `node` | binary composition root for all chain services | RFC 001 to RFC 008 | `CH-001`, then all integration work |

## Rules

- Keep consensus-critical logic out of `rpc` and `node`.
- `node` composes crates; it should not become the place where protocol
  rules live.
- Cross-crate interfaces must be documented in the owning RFC before they
  are implemented.
- New crates require an update to this map and to `OWNERS.md`.

