# Same order, different state: what broke between consensus and execution in a trading-chain prototype

Earlier this year I built 0x26 Chain, a prototype layer-one ledger for trading. It ordered transactions with a DAG protocol derived from Mahi-Mahi and executed them in a native order-book engine. It ran as a controlled testnet with three validators. It was an engineering spike, and it is no longer running.

This post is about the most useful thing the prototype taught me. Four validators agreed on exactly the same sequence of transactions and still ended up with different state. Consensus was fine. The bug was in the gap between consensus and execution, and I think anyone putting a stateful engine behind DAG ordering can fall into the same gap.

The full write-up, with the proof, tests and measurements, is in the paper: [link to PDF].

## The incident

DAG blocks in this system are small, so executing them one at a time was slow. Every execution step pays for a checkpoint, a state root, a header and an install. The fix was to merge eight DAG blocks into one execution group and pay those costs once.

That left a tail. If five decided blocks were sitting in the queue and no more arrived, they waited. So I added what looked like a harmless improvement: if every block in the queue has been verified, execute what is there, even if it is fewer than eight.

Under load on four hosts, one validator diverged. The evidence was strange:

- The DAG commit-log hash matched on all four validators.
- The committed block count and the executed intent count matched.
- The execution state roots did not. A binary search found the first difference at height 171.

Every validator had executed the same transactions in the same order. What differed was how they were grouped.

## Why grouping changes the state

Each execution header hashes the previous header, the body root, the event root, the round and the timestamp. A group of four blocks followed by another group of four produces two headers. One group of eight produces one. Same transactions, different header chain, different root.

The flush condition compared "how many blocks have I verified" with "how many blocks are in my queue". Both numbers depend on when messages arrive and when the verifier finishes, which is different on every machine. So one validator saw four verified blocks in a queue of four and flushed. Another already had eight queued with some still verifying, waited, and executed all eight together.

A local scheduling decision had become part of the replicated state.

The unit test for the flush passed, because it checked one queue. It never compared two replicas.

## The rule that fixes it

The replacement is one line. With `v` verified blocks at the front of the queue and a batch size `b`, drain `v - (v % b)` and execute that in consecutive groups of `b`.

Verification timing can now delay a group but cannot change which blocks are in it. Group `j` is always blocks `j*b` through `(j+1)*b - 1` of the decided sequence. The paper has the short induction.

I tested this at the code revision the paper studies. Each simulated replica gets the same decided sequence but its own random schedule of arrivals, verifications and drains:

- With the fixed rule, 351 schedules all produced the same groups and the same execution head.
- With a model of the old flush rule, all 39 schedules diverged, while still consuming the same sequence.

## What the rule does not fix

The rule holds only under conditions, and testing them was more interesting than testing the rule.

**The batch size is local configuration.** Each node read it from an environment variable. Nothing in a block header recorded it. Two nodes set to four and eight execute the same sequence and get different heads. I tested that too, and they do.

**One bad block stops everything.** The queue only drains verified blocks. A block whose payload fails verification is never skipped or retried, so it and everything behind it waits. If every validator rejects the same block, they all halt at the same place. In the test, two groups executed and the remaining 48 blocks sat there.

**Recovery breaks it.** This is the finding I would act on first. A validator that resets its volatile DAG state starts a fresh queue from its last executed head. I ran this on a four-validator in-process cluster:

- When all four reset at the same head, their state roots stayed equal afterwards.
- When one reset and the other three kept going, its state root differed at the very next height.

The reset validator had thrown away five decided blocks that its peers still held and later executed. It was no longer even working from the same sequence, so no grouping rule could save it. Fixing this needs an agreed recovery point, or groups derived from the consensus decisions themselves instead of a block count.

## Two more gaps in the same interface

**Prices have to travel with the block.** Order acceptance depends on mark and index prices. If each replica reads its own local price while replaying an agreed order, they can disagree about whether the order is valid. The prototype moved to carrying price observations inside the signed block. That makes replay deterministic. It does not make the price trustworthy, because a block author can still supply a false one. Those are separate problems.

**An atomic write is not a rollback.** The storage wrapper ran each state change on a copy of the engine, committed the result to disk atomically, and installed the copy. It did not look at whether the change had returned an error. So an operation that modified some state and then failed could have its partial state committed. The disk write was atomic. The application error handling was not.

## A fast path, checked properly

The prototype had an optimised path for batches of resting limit orders that could simply be appended to the book. It skipped several full copies of state. I had argued it was safe because appending is equivalent to inserting when nothing crosses.

That argument only covers the order book. So I ran 2,400 randomised execution steps through both the fast path and the general path and compared everything:

- State roots, order books, policy state and collateral matched on every step.
- On 203 steps the fast path emitted no rejection event for orders the pre-filter had rejected.
- On 179 of those it also left the rejected orders in the mempool.
- The missing events shifted later event sequence numbers, and one of those numbers is stored in position records.

The two paths agreed on everything a state root covers, and disagreed on what clients were told. A root comparison alone would have called them equivalent.

## About the throughput numbers

One run recorded 97,768 order insertions per second. I want to be clear about what that number is:

- It was four validator processes on one machine.
- Durable storage had not been integrated yet.
- The load generator sent one-sided orders at a constant price, which is the best possible case for the fast path. No order crossed, so no trade executed.
- A second way of counting the same run gives 108,000 per second, above the 100,000 offered, because it includes warm-up and drain.

It tells you the insertion path was not the bottleneck on that workload. It does not tell you how fast a real exchange would run.

## What I would do differently

- **Derive execution boundaries from agreed data.** A block count is simple, but the consensus layer already produces agreed boundaries. Use them.
- **Put configuration that affects state into the state.** If changing a setting changes the root, the setting belongs in the header.
- **Test replicated properties across replicas.** A single-queue test cannot catch a divergence.
- **Compare more than roots.** Equal roots hid real differences in events and cleanup.
- **Design recovery first.** Restart was the least tested path and the one that broke the main guarantee.

The paper has the details, the limits of each test, and the test code. AI tools helped with tracing the code, writing the tests and drafting; I am responsible for the technical account.
