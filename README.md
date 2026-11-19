# 0x26 L1

Source snapshot of 0x26 Chain, a prototype layer-one trading ledger that
combined Mahi-Mahi-derived DAG ordering with a native order-book engine.

It was an engineering prototype run as a controlled testnet. It is no longer
operated. This repository exists as the public artifact for the paper
*0x26 Chain: Execution Contracts for a DAG-Ordered Trading Ledger*.

**Read the paper:** [paper/0x26-chain-execution-contracts.pdf](paper/0x26-chain-execution-contracts.pdf)
(20 pages). A shorter account is in [paper/BLOG_POST.md](paper/BLOG_POST.md).

## What is here

- `crates/`: the Rust workspace (consensus, execution, matching, policy,
  storage, RPC, node).
- `paper/`: the paper (PDF, source, figures), its supplementary records, and
  a shorter blog-post version.
- `THROUGHPUT_DECISION_LOG.md` and `experiments/`: the development log and
  experiment records the paper's measurements come from.
- `deploy/` and `scripts/`: the testnet deployment tooling, kept for reference.
- `WORKSPACE.md`: the original workspace README.

## How this snapshot differs from the studied revision

The paper studies internal revision `5dabe028` (25 May 2026). This snapshot is
that revision plus the tests written for the paper, with these changes:

- Server IP addresses are replaced with documentation addresses
  (`203.0.113.x`), and testnet bot secrets are redacted. Hashes of log
  sections recorded in `paper/anc/` were taken before this redaction, so
  sections that contained an address no longer match them.
- `crates/loadgen` and `crates/market-data` depend on a client SDK that is not
  included. Their sources are kept but excluded from the workspace, and
  `Cargo.lock` was regenerated without them.
- Git history is not included, so the revision hashes cited in the paper do
  not appear here.

## Reproducing the paper's tests

Requires a stable Rust toolchain.

```bash
cargo test -p kombat-chain-rpc --lib tests::paper_schedule -- --nocapture --test-threads=1
```

```bash
./scripts/paper-differential.sh
```

The first command runs the schedule and reset tests (Section 7.5, Table 7).
The second replays the 2,400-step corpus with the prechecked path on and off
and prints a field-by-field comparison. Expected results are in
`paper/anc/execution-tests.json`.

## Attribution

The ordering layer follows the committer organization of the Mahi-Mahi
reference implementation by Pasindu Tennage and contributors, which builds on
Mysticeti by Mysten Labs. The configuration here differs from both published
protocols, so their safety and liveness results do not carry over. `NOTICE`
lists the sources and papers.

## Licence

The source is published for reading, building, and reproducing the paper's
results, and for non-commercial research and teaching. It is not an open
source licence. See `LICENSE`.
