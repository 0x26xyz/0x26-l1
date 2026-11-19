# 0x26 Chain: Execution Contracts for a DAG-Ordered Trading Ledger

Wayne Douglas
wayne@pollenapp.ai
17 November 2026

## Abstract

We present the ordering-to-execution interface of 0x26 Chain, a layer-one
trading ledger with DAG ordering. The design defines four contracts:
common execution groups, common decision inputs, explicit application-error
semantics, and recoverable execution boundaries. A recorded partial-flush
change produced different execution roots despite matching DAG commit-log
hashes. We explain the mechanism and prove that fixed-count FIFO grouping
removes verification timing from group membership within a common execution
segment. A cross-replica test confirms the rule under 351 independent
verification schedules, while a model of the reverted flush diverges under
every tested schedule.

Tests at the studied revision check the remaining contracts. A differential
corpus of 2,400 execution steps finds equal roots and committed trading
state on a prechecked resting-order path and the general executor, and
isolates their differences in rejection events and queue cleanup. Failed
payload verification halts execution in the current queue. An atomic
storage commit can install mutated state after an inner application error.
Recovery is the open contract: one validator that resets its volatile DAG
while its peers continue diverges at the next execution height, while a
simultaneous reset from a common head does not.

Existing measurements give performance context. Four validator processes
on one host recorded 97,768.6 sampled order insertions per second before
durable-storage integration, on a constant-price workload built for the
prechecked path. They supply no estimate of deployed speedup or real-flow
hit rate. The system was an engineering prototype with three validators
and zero fault tolerance for continued progress. These findings give
concrete checks for the ordering-to-execution interface.

## 1. Introduction

DAG consensus gives a ledger an ordered sequence. A trading runtime must
turn that sequence into execution groups, prices, policy updates, events,
and stored state. These boundaries matter because the chain header can
depend on how ordered inputs are grouped and applied. Agreement on an
input sequence is therefore only one part of the execution interface.

0x26 Chain combines Mahi-derived DAG ordering with a native trading engine.
Its execution worker merges ordered DAG blocks before applying them.
Merging amortizes checkpoint, root, header, and installation costs. A
prechecked path also reduces full overlay copies for selected resting-order
batches. Both choices change execution work after ordering has finished.
The design must state which local choices can change the replicated result.
[R12, R13]

The paper asks: **what execution contracts preserve repeatable transitions
when a trading ledger separates ordering, verification, batching, and storage?**
We give a concrete answer through four contracts. Replicas use the same
group partition and decision inputs. Application errors have defined state
and retry semantics. Restart restores the partition cursor or establishes
a common new execution segment. Table 1 states each contract and the
verification progress condition.

The main constructive result is a fixed-group execution rule. A worker
consumes only a verified prefix whose length is a multiple of one common
batch size. Under a common sequence and starting offset, this rule gives
the same group membership at every replica regardless of verification
schedule. A recorded partial-flush change supplies the motivating case:
DAG commit logs matched while execution roots diverged. The rule addresses
that specific source of local boundary dependence. Tests at the studied
revision check the rule across replica schedules and locate where it stops
holding across a reset. [R13, R15, R30]

| Contract or condition | Required property | Implementation boundary |
|---|---|---|
| Group partition | Common sequence, offset, and fixed batch size | Operators set the same node-local size; fixed-count FIFO groups use it within one segment |
| Verification progress | Every needed block eventually reaches `Verified` | A `Failed` block stops its group and all later execution; no agreed skip rule |
| Replay inputs | Common prices and other decision inputs | Nonempty payload observations supply replay maps |
| Error and retry | Failed application has a defined state transition | Requeue alone does not undo canonical mutation |
| Recovery boundary | Restore the consumed cursor or agree a new segment | No persisted group cursor; a tested single-validator reset diverges at the next height |

Table 1. Four execution contracts and their verification progress condition.
The table separates implementation behavior from the requirements for
repeatable execution and progress. Section 4.1 states the operator's
batch-size obligation.

The oracle and application-error cases complete the interface account.
Pinned observations bind price inputs to the ordered payload. Explicit
state-installation semantics separate an atomic backend write from an
application rollback. These contracts also define how to assess an
execution fast path. A book-level append rule alone cannot establish
whole-runtime equivalence.

We make four contributions:

1. We specify the contracts between DAG ordering and stateful trading execution.
2. We explain the partial-flush divergence, prove the within-segment
   partition property of the fixed-group rule, and test both across
   independent replica schedules.
3. We trace replay inputs, application errors, storage installation, and
   restart boundaries to their concrete implementation behavior. A
   differential corpus and a reset experiment test the fast-path and
   recovery boundaries.
4. We analyze existing measurements with explicit generator settings,
   workload eligibility, execution stage, and counter definitions.

The system was an engineering prototype operated as a controlled testnet.
It is no longer operated, and the findings describe the tagged May 2026
revision rather than a running network. The deployment used three active
validators and one ingress host. The performance experiments used four
validators on one host or on four hosts. The paper presents the
implementation and its evidence within these stated configurations.
It claims no new DAG consensus protocol.

## 2. Background and Related Work

State-machine replication requires common transition inputs and deterministic
execution [R09]. Calvin sequences inputs before deterministic concurrency
control [R22]. Our grouping case makes one application boundary explicit:
the same ordered stream can produce different execution-header chains if
replicas choose different execution partitions. The fixed-group lemma is
a narrow statement of how the implemented worker avoids that dependence.

Mahi-Mahi orders an uncertified DAG using structural support and randomized
leader selection [R01]. Narwhal and Tusk separate dissemination from ordering
[R05]. Bullshark, Mysticeti, and Shoal++ develop DAG commit mechanisms
[R06, R07, R21]. Sui uses Mysticeti in production [R20]. 0x26 adopts a
Mahi-derived committer organization [R11] with the settings in Section 3.
The initial ordering path used HotStuff-family messages and a pacemaker
[R02, R03, R04]. The contribution here concerns the execution interface.

Block-STM uses multiversion speculation and validation under a preset order
[R08]. BTM with conflict specifications uses independence information to
guide execution [R24]. RapidLane uses deferred objects for semantic shared
updates [R25]. These works establish general execution and specialization
techniques. The 0x26 resting-order path is narrower and serves as a concrete
execution-stage example. Its elementary append rule is not a general
serial-equivalence result.

LMAX uses sequence barriers and batching in a financial exchange [R26].
HyperCore combines consensus with native margin and matching state [R17].
dYdX separates local order books from consensus-validated operations [R18].
Sei Giga separates ordering from execution and later state attestation
[R19]. These are close pipeline and application comparisons. They motivate
clear distinctions between admission, ordering, execution, and state.
Their public performance figures do not form equivalent baselines for
our constant-price insertion runs.

Write-ahead logging and atomic storage updates have established database
foundations [R10]. Persistent data structures provide a general alternative
to copying complete mutable structures [R28]. Section 6 discusses that
alternative to a special-case recognizer. Vote-extension oracles such as
Skip Connect carry external price inputs through consensus [R23]. Our
oracle case concerns deterministic replay of author-carried observations;
it supplies no new price-trust or aggregation protocol.

## 3. System Architecture

### 3.1. Components and topology

The Rust workspace separates consensus, transport, policy, execution,
matching, storage, remote procedure calls (RPC), and node integration.
Signed intents carry policy-bound trading actions. RPC accepts and
forwards requests. The node applies trading and EVM operations.
The storage layer preserves committed state and admitted pending work. [R12]

The deployment configuration uses three active validators and one ingress
host. Its settings select the Mahi rule, one leader slot, execution batch
size eight, and redb. Performance experiments also used four validators
and different ingress arrangements. [R12, R13]

```architecture
Clients -> ingress -> selected policy owner or fallback
Ingress (configured host 1) -> validators 2 | 3 | 4
Validators exchange signed DAG blocks
At each validator: verify -> order -> group -> execute -> local redb
```

Figure 1. Deployment and request path. Ingress selects a policy owner or
fallback. Each validator orders and executes locally. DAG exchange links
are schematic. The three validator
boxes use configured identities 2, 3, and 4. Configured host 1 is the
ingress and does not vote. The diagram does not replicate one submission
three times.

### 3.2. Admission and dissemination

The relay maps each policy to a primary payload owner. It uses the policy
ID modulo the active-validator count. Configured backup owners follow
that validator in the active order. An observer initially sends to the
primary owner. A retry or lag signal can select the next owner.
This selective fallback avoids unconditional primary-plus-backup relay
on the normal path. [R12, R13]

The owner enqueues eligible payloads in a pre-admitted pool.
The pool has a hard item limit and policy-ordering structures.
The producer selects policy frontiers and excludes in-flight IDs and
nonce ranges. This moves work out of the synchronous relay-to-engine
handoff. Signature, policy, and commit validation still apply.
Queue insertion acknowledges admission, rather than finality. [R12]

Queue bounds and nonce ordering interact with load delivery.
A missing nonce can prevent later work for that policy from advancing.
The experiments therefore track queue drain and errors alongside request
and commit rates. Section 7 discusses cases where delivery and retained
queue tails affected the result. [R13]

### 3.3. Ordering, execution, and storage

The committer emits ordered leader decisions. The linearizer collects
each committed causal subgraph once. It sorts the resulting blocks by
round, author, and hash. Merged execution groups that sequence and
constructs typed block items. Trading transitions produce ordered events
and an execution header. [R12]

The durable wrapper constructs a backend write plan before it promotes
the mutated engine clone. The plan contains the canonical engine record,
head metadata, events, receipts, and write-ahead log (WAL) changes.
Restoration checks a separate executor root. Logical snapshots provide
a portable backend representation with version and fingerprint checks. [R12, R14]

### 3.4. Blocks, validation, and decisions

A Mahi block contains an author, round, epoch, timestamp, ancestor
references, typed payloads, and an author signature. The implementation
uses Borsh encoding, SHA-256, and Ed25519. The runtime checks membership,
epoch, signature, ancestor rules, and the local restart boundary.
It tracks missing ancestors for retrieval. Payload verification and
commit execution use separate worker paths. [R12]

The library has a default wave length of five and a minimum of four.
A vote follows a causal support search for a leader reference.
A certificate requires distinct supporting authors. Direct and indirect
rules classify leader slots as committed, skipped, or undecided.
The universal committer returns a decided prefix. The deployment selects
the Mahi rule. Runtime defaults use one leader slot and disable the
pipeline. Library builder defaults are separate from deployment settings. [R12]

### 3.5. Membership and fault model

The system uses controlled membership and computes quorum as
`q(n)=floor(2n/3)+1`. Three active validators require quorum three.
Four require quorum three. Thus the three-validator deployment requires
all three validators for prior-round quorum formation after the restart
boundary. The three-validator configuration has zero fault tolerance for continued
progress, including crash faults. The four-validator experiments use a
different quorum threshold. [R12]

Leader selection uses round-robin order. Published Mahi-Mahi instead uses
a shared random coin [R01]. The runtime rejects a second block from the
same author and round, and its round index retains one reference per
slot. A Byzantine author can send different first blocks to different
replicas. First-arrival rejection can then prevent a replica from installing
a conflicting block needed by an ancestor reference. This is a causal
availability hazard and can give committers different DAG views. A safe
resolution rule for that case is not established here. The configuration
has one leader slot, no shared coin, and no pipeline. It uses Mahi-derived
commit logic, rather than the complete published Mahi-Mahi fault model. [R12]

The linearizer's round-first order respects earlier-round ancestor edges.
This provides the topological order used by the execution path.
The execution analysis below assumes a common decided sequence and a
common replay starting point. It examines what replicas must agree on
after ordering has supplied that sequence.

Payload verification supplies a further progress condition. Ordering can
queue a block before its payload verification completes. Both drain
functions stop at the first block that is not `Verified`. A `Failed` block
therefore blocks its group and all later execution in that queue. The
failure handler marks the block; it has no automatic retry or agreed skip
rule. Duplicate receipt can enqueue verification again, but cannot repair
a deterministically invalid intent signature. If all replicas decide that block
and reject that intent signature under the same verification inputs, all halt at
this boundary. A DAG reset discards the queue, subject to the recovery
gap in Section 5.4. This is a source-derived failure mode, not a recorded
outage. Section 7.5 reproduces the queue behavior in a test. [R29, R30]

## 4. Common Execution Groups

### 4.1. Deterministic execution groups

Mahi ordering and execution grouping are separate operations.
Let `S` be a decided sequence of Mahi blocks. Let `G(S,b)` partition it
into groups of `b` consecutive blocks. The RPC worker drains verified
blocks and sends each group to merged execution. The node emits one
execution header for each group. [R12]

The batch size is node-local configuration. Before each drain, the worker
reads `KOMBAT_MAHI_MERGED_EXECUTION_BATCH`. When this setting is absent,
the default is eight for multiple validators and one for a single validator.
No block header or durable metadata binds this setting. Lemma 1 therefore
requires operators to set the same positive `b` at every replica and keep
it fixed within the execution segment. A configuration mismatch can change
partitions even when the decided sequence and starting offset match.
Section 7.5 confirms this with batch sizes four and eight. [R29, R30]

The native header root has the following form:
`H_k = SHA256(H_(k-1) || T_k || V_k || r_k || t_k)`.
Here `T_k` is the merged body root and `V_k` is the ordered event root.
The other inputs are the execution round and timestamp, encoded as
little-endian integers. Merged execution takes the maximum input
timestamp. Its round is the maximum of the parent round plus one and
the input block rounds. A partition change can therefore
change the header inputs and execution height. [R12]

A partial-batch flush exposed this dependency in a four-validator run.
The flush executed a verified prefix whenever every queued block was
verified. Otherwise, it executed only full groups. The condition compared
the verified count with the local queue length. Message arrival and
verifier timing therefore influenced group boundaries. The run recorded
matching commit-log hashes and block and intent counts, but different
execution roots. The first different root occurred at height 171. [R13, R15]

Figure 2 illustrates a schedule with `b=8` and decided blocks `m1`
through `m8`. Replica A first queues four blocks and verifies all four.
The flush permits it to execute `[m1,...,m4]`. Replica B already has eight
blocks, with some verification pending. It waits and later executes
`[m1,...,m8]` as one group. A later arrival can let A execute
`[m5,...,m8]` separately. Both replicas consume the same sequence through
different execution-header chains.

```timeline
time | Replica A | Replica B
t0 | m1-m4 verified; queue ends | m1-m8 queued; m5-m8 pending
t1 | execute m1-m4; header H_A1 | wait for verification
t2 | execute m5-m8; header H_A2 | execute m1-m8; header H_B1
```

Figure 2. Two-replica partial-flush schedule. Both replicas receive the same
decided prefix. A emits two execution headers while B emits one.
Times denote event order, not measured durations.

The replacement rule drains `v-(v%b)` blocks from a verified prefix of
length `v`. The worker then divides that prefix into groups of size `b`. [R12, R15]

**Lemma 1 (partition agreement).** Let replicas consume the same sequence
`S` from the same offset `o`. Let all replicas use one fixed positive batch
size `b`. Each replica drains only a verified FIFO prefix whose length is
a multiple of `b`, then executes that prefix in consecutive groups of `b`.
Successful group `j` has the same membership at every replica:
`G_j = S[o+jb : o+(j+1)b]`.
Verification schedules can delay a group, but cannot change its membership.

**Proof.** Initially each replica has consumed exactly `o` elements.
Suppose a replica has consumed `o+kb` elements. A drain of `mb` elements
starts at that offset and emits groups `k` through `k+m-1` from `S`.
Their boundaries are the formula above. The new consumed offset is
`o+(k+m)b`, so induction applies to every successful drain.
A verification stall emits no group and changes no boundary. Thus replicas
agree on every group index they both execute. If each needed block is
eventually verified and execution progresses, they emit the same complete
groups. The lemma does not require equal progress at each wall-clock time.
The `Failed` status described in Section 3.5 violates this progress
condition; the partition result does not provide liveness for invalid payloads.

If merged execution returns an application error, the worker requeues
the failed group and the unexecuted suffix in their original order.
It stops that pass and retries, rather than advancing to a new partition.
The lemma assumes successful execution or a state-preserving retry.
Requeueing does not establish that assumption. Section 5.2 traces the
application-error path. [R12]

The lemma concerns one execution segment with a common offset. Section 5.4
checks how restart and catch-up handling interact with that assumption.
It does not establish a common DAG order or the correctness of replay inputs.

Section 7.5 checks the lemma against the implemented drain function.
Each simulated replica receives the same decided sequence under its own
arrival, verification, and drain schedule. All 351 schedules produced the
canonical groups and one execution head. A test-local model of the reverted
flush rule produced different groups and heads under all 39 of its
schedules, while every replica still consumed the same sequence. [R30]

The associated unit test had checked one local queue. It expected four
verified blocks to drain with `b=8`. That test checked the partial-flush
branch, but did not compare execution partitions across replicas.
The case shows why a local queue property and a replicated execution
property need different validation. [R15]

Fixed grouping leaves a remainder `r` with `0 <= r < b` after a drain.
For `r > 0`, another `b-r` decided and verified blocks complete the group.
The deployment enables `KOMBAT_MAHI_ALLOW_IDLE_EMPTY_ROUNDS=1`.
Thus empty blocks can advance the DAG after client load stops. Its drive
interval is 250 ms and its configured logical round interval is 1,000 ms.
The standalone non-test default disables idle rounds, but the producer can
still continue while its payload-backlog predicate is true. [R12]

These settings do not supply a hard drain-time bound. Producer calls can
wait for ancestor quorum, block availability, verification, or execution.
If consecutive additions to the ordered verified prefix are at most
`Delta` apart, and the completing group executes within `E`, the conditional
bound is `(b-r)*Delta + E`. At `b=8`, at most seven additional blocks are
needed. The implementation does not establish a finite `Delta` under its
network and fault model. The 1,000 ms logical interval is not a seven-second
wall-clock guarantee.

**Alternative: committed sub-DAG groups.** The linearizer already returns
a sequence of committed leader sub-DAGs. Executing each nonempty sub-DAG
as one group would derive its boundary from the agreed leader decisions
and canonical deduplication frontier. It would remove the fixed-count
remainder and the offset modulo `b`. The current worker flattens these
sub-DAGs into one queue before fixed-count grouping. [R12]

Fixed-count grouping was introduced to amortize checkpoint, root, header,
and install costs across small DAG blocks. Run 71 tested counts 4, 8, 16,
and 32 and selected 8 as the best point in that local sweep. Later sweeps
retained that setting. The record does not compare it with leader-sub-DAG
grouping, so it does not establish that fixed counts are the better design.
Sub-DAG groups would have variable work and body size. Recovery would still
need the last applied leader and deduplication state, or an equivalent
checkpoint, to avoid omission or duplicate execution. Removing the modulo
offset does not remove all recovery metadata. [R12, R13]

## 5. Replay, Application, and Recovery

### 5.1. Oracle observations as execution inputs

Trading decisions depend on mark and index prices. If replicas read
independent local prices during replay, an agreed intent can produce
different order acceptance. An oracle-replay case exposed this issue
and motivated block-carried price observations. [R16]

The Mahi payload signs those observations. The node derives replay maps
from them. Merged execution selects the newest timestamp per market and
price kind. The comparison uses `>=`, so an equal-timestamp observation
replaces the earlier observation in the ordered input. This tie rule is
deterministic for a common input order. A strictly older observation cannot
replace a fresher one. Regression cases use different local
maps and reverse the order of fresh and stale observations. [R12, R16]

The compatibility path uses local price maps when the payload contains
no observations. Pinned-price replay therefore applies to blocks with
explicit observations. Trading blocks need those inputs to obtain the
same price-dependent decisions across replicas. Direct cancel and
replace operations also require coverage by the replicated-input contract. [R12, R16]

The signature authenticates the block author and binds the observation
bytes. It does not establish price truth. This replay path checks positive
prices and timestamps, but does not require per-observation feed signatures,
a validator median, a price quorum, or a cross-validator deviation bound.
A Byzantine block author can supply a positive false price with a newer
timestamp. If that observation enters the decided input and wins selection,
it controls the replay price for that market and price kind. The design
therefore assumes trusted price input from the participating operators.
Deterministic replay and manipulation-resistant oracle aggregation are
separate properties. [R12]

This case and the grouping case address distinct requirements.
One fixes the partition of agreed inputs. The other fixes the inputs
used inside each partition. Together they define the execution contract:
replicas use the same group boundaries and the same decision inputs.

### 5.2. Application errors and retry

The worker requeues a failed group and its remaining suffix in original
order. That preserves the queue partition. It preserves application
semantics only if the failed attempt leaves a suitable replay state.
A mutable engine can retain a prefix of policy updates even when the
caller receives an error. [R29]

| Stage | Validation and mutation | Error and retry boundary |
|---|---|---|
| 27 April source proxy | Filters nonce continuity. The direct helper constructs admission records, then commits them to canonical state in body order. | A late policy error leaves earlier commits in place. Retry can omit advanced nonces or fail to progress on a persistent later rejection. |
| May implementation | Simulates candidates before execution. General commit uses policy and book overlays; the direct path mutates canonical state. | A general commit-stage error discards its overlays. The direct path has no equivalent undo. The durable wrapper can install changed state despite an inner application error. |

Table 2. Application-error boundaries by implementation stage. The April
revision is the source proxy defined in Section 7.1. These are conditional
source findings, not reproduced failures or a claim of double execution. [R29]

The May wrapper does not supply automatic application rollback. Its type is
`Result<T, DurableCommitError>`. The merged application call supplies a
`T` that is itself `Result<Hash, NodeError>`. The wrapper runs that call on
an engine clone, builds and commits the resulting storage plan, and installs
the clone without checking the inner result. If the application mutates
state and returns an error, a successful storage commit can therefore
produce `Ok(Err(NodeError))` with the mutated engine installed. A storage
failure has a different boundary: the wrapper returns before installation.
Storage-batch atomicity and application-error atomicity are separate. [R29]

The May simulation filter ordinarily rejects a duplicate client-order key
before body execution. It can cascade-reject later intents from that policy
while retaining other policies. This reduces exposure to commit-stage
errors. A duplicate that reaches a resting-batch helper returns an error
for that execution call. The general commit loop also propagates helper
and per-item errors. Table 2 distinguishes the resulting mutation boundaries. [R29]

Rejection context also matters for whole-path equivalence. At the May
revision, the general metadata installation receives the filter's rejected
intent list. The prechecked helper passes an empty list. The two branches
therefore differ when the filter rejects some inputs and the retained body
selects the fast path. Metadata installation uses this list to remove
rejected intents from the mempool and emit `IntentRejected` events.
An empty list emits no `IntentRejected` event for these rejections.
Rejected intents present in the local mempool are not removed by this
commit. The fast commit also omits their rejection-based proposer-frontier
refresh. Other later cleanup is a
separate mechanism. This is a concrete source-level difference in events
and queue handling, not only missing context. The successful book append
property does not establish whole-executor equivalence. [R27, R29]

A differential corpus tests this boundary at the May revision. It replays
2,400 merged execution steps with the direct path enabled and disabled.
Header roots, executor roots, policy state, books, mandate metadata, and
collateral matched on every step. On 203 steps that took the direct path,
the enabled run emitted no `IntentRejected` event for filtered intents.
On 179 of those steps it also left rejected intents in the mempool.
The missing events shift later event sequence numbers. Position ledgers
store the sequence of their last fill, so 50 later steps differ in that
stored field. Every step returned success in both modes, so the corpus
does not exercise the late-error paths in Table 2. Section 7.5 gives the
corpus design and limits. [R30]

A complete error contract must either preserve the prior state on failure
or define an agreed error transition that the retry logic understands.
The code paths above do not establish that complete contract. They make
it an explicit condition of the execution interface rather than an assumed
benefit of using a queue or an outer clone.

### 5.3. Header roots and durable executor state

The native header root commits to execution history and outputs.
It is distinct from a complete authenticated application-state map.
The execution wrapper contains a synthetic QC with no signatures.
It adapts the execution representation; it does not supply an additional
Byzantine finality certificate. [R12]

Durable metadata stores the chain-head root and executor root separately.
Restoration verifies executor state under the latter contract.
The durable wrapper clones the engine, applies the mutation, serializes
a canonical record, and computes namespace differences. It commits the
backend batch before promoting the mutated clone. This storage-installation
order does not condition promotion on application success, as Section 5.2 explains. Consumed WAL rows are
deleted within the same backend batch. [R12]

The redb backend uses `Immediate` durability for `Sync` with fsync enabled.
Its `GroupCommit` and asynchronous modes use `Eventual` durability.
The group-commit name reserves a mode; it does not implement coordinated
group fsync. Atomic visibility and persistence at acknowledgement are
separate properties. Performance results must retain the durability mode
under which they were obtained. [R12]

### 5.4. Restart and catch-up

Restart handling chooses a new local boundary round as
`max(head.round+1, current_round, 1)`. It creates a fresh DAG, linearizer,
and pending-execution queue. Snapshot-based peer repair installs the
recovered engine and rebuilds the same volatile state. The durable
`CommitMeta` stores height and roots, but no consumed Mahi-sequence offset
or batch-size version. The engine snapshot also omits the volatile DAG
queue and linearizer cursor. [R12]

The head round is not an offset in the linearized block sequence.
A DAG round can contribute several blocks, while one execution header
can merge several rounds. The restart procedure therefore starts a new
execution segment; it does not recover the old segment's group position.
Replicas that restart together from the same head with the same settings
can use a common fresh segment. Peer reachability at startup does not
certify that all replicas installed the same boundary. A staggered restart
or catch-up cannot be covered by Lemma 1 without that additional agreement.

**Decided but unexecuted work.** A process restart discards this replica's
pending Mahi block references, including a short decided tail. A local DAG
reset also replaces the queue and linearizer. No stored Mahi-tail journal
replays those exact blocks. Blocks below the new restart round are rejected. [R12]

Payload survival is a separate question. Backend restoration loads admitted
intents from the mempool WAL and prunes nonces already consumed in the
recovered engine state. Remaining eligible intents can be proposed again
in new DAG blocks. This restores requests, not their former DAG grouping.
The pre-admitted pool is process-local and removes entries when a producer
drains them. Payloads held only in that pool or volatile DAG have no
guaranteed local recovery path. A peer copy or client resubmission may be
needed. Thus the tail is not uniformly replayed or uniformly re-proposed. [R12]

**Does quorum three of three coordinate restart?** Losing one validator
stops new prior-round quorum formation. It does not undo blocks that peers
already decided or stop their queued execution. A stalled producer can
reset its own DAG after a 30-second ancestor wait, but each replica selects
that boundary from its own engine head. The startup check counts reachable
peers; it does not certify a common head or an all-replica segment change.
A quorum stall can encourage local resets without establishing a common
new execution boundary. [R12]

The existing restart test reconstructs servers from cloned engine state.
It checks that height and round advance after restart. It does not check
restoration of an old DAG offset across different restart schedules. [R12, R14]

**Tested reset boundary.** A new test applies the production local DAG
reset on a four-validator in-process cluster. This is the call used by
the ancestor-stall self-heal. All four validators first reach one head at
height 4, each with five decided blocks still queued. When all four reset
at that head, execution roots stay equal at every later common height.
The five queued blocks are discarded at each validator. When one validator
resets and its three peers continue, execution roots differ from height 5,
the first height after the reset. The reset validator's commit-log hash
also differs from its peers. Three repetitions gave the same heights. [R30]

The staggered case shows more than a lost group cursor. The reset
validator starts a fresh linearizer at its boundary round and rejects
earlier blocks. Its peers still hold and execute the earlier decided tail.
The two sides therefore execute different block sequences, so the common
sequence assumed by Lemma 1 no longer holds. A complete recovery contract
must restore the decided sequence position as well as the group cursor
and batch-size version, or establish an agreed new segment boundary at
every replica. The studied revision does not provide that contract.
This is an observed divergence of an in-process reset, not a recorded
production incident. The fixed-group argument applies within a common
segment. [R12, R30]

## 6. Execution Specialization and Its Scope

The native executor attempts a prechecked resting-order path before its
full overlay branch. An eligible body has at least two items, all
`PlaceOrder` intents, and one side per market. Orders must be valid limit
orders that may post without crossing. Each order must follow the existing
same-side tail and the previous candidate under price-time priority.
Duplicate active keys reject the batch. Risk checks account for candidate
exposure. Unsupported shapes use the general executor. [R27]

The helper retains original body indices. It commits policy admissions in
body order, appends each market's orders, and restores event order from
indexed outcomes. For a successful eligible matching group, append has the
same book position as sequential insertion: each candidate follows the
tail, uses the next sequence, and produces no fill. This is a consequence
of the predicate, not evidence of whole-executor equivalence. The
differential corpus in Section 7.5 supplies that evidence for roots and
committed trading state, and records the event and queue differences. [R30]

The May prechecked branch bypasses four commit-stage overlays: policy
state, all order books, resting-order mandate metadata, and isolated
collateral. It runs after payload filtering. That filter first clones the
whole policy store and resting-order mandate map. It clones both again
for each candidate that reaches simulation, after nonce and cascade-rejection
checks. `PolicyStore` derives `Clone` over a `BTreeMap`; these are full
copies, not shared snapshots. With `m` simulated candidates, the filter
performs `m+1` full clones of each structure before the fast path starts.
It also clones the accumulated simulated book and collateral maps. [R29]

The four omitted overlays therefore describe only the later commit stage.
Per-candidate full-store copies remain on the fast path's input side.
Affected-market risk simulations, body copies, and the outer durable
engine clone also remain. Their relative runtime cost was not profiled
in the reported runs. The April source proxy has nonce-only filtering
and two commit-stage overlays, for policy and books. The May filter's
copy cost is not part of the April measurements. [R27, R29]

```prechecked
Ordered body -> eligibility and risk checks
Unsupported -> overlay executor
Eligible -> ordered policy updates -> market appends
Outcomes -> original event order -> metadata
```

Figure 3. Execution specialization within the common-group contract.
The paths share the ordered body but have different mutation and error
boundaries, as Section 5.2 explains.

**Alternative state representations.** Copy-on-write structures, persistent
maps, and overlays limited to touched policies or markets could reduce
copying for a broader workload. Persistent structures are established
prior work [R28]. The recorded choice was a local optimization of the
existing native branch: reuse its resting-batch checks and avoid its broad
copies for the benchmark's dominant operation class. The record contains
no comparison with these alternatives. It therefore gives no basis for
claiming that the recognizer is the better general solution. [R13]

The May path places per-candidate full-store copies and an outer engine
copy before the optimized commit. Removing four later overlays does not
remove those costs. The April gain therefore supplies no estimate of
the May deployment's speedup. No measurement compares these costs or
establishes which dominates. Cancels, replaces, two-sided groups, priority-improving orders,
and bodies with funding or oracle items can also require fallback.
Real-flow eligibility and deployed-system benefit remain unmeasured.

## 7. Evaluation

### 7.1. Method and measurement definitions

The evaluation uses development measurements, incident records, the
existing regression results, and new tests written for this paper at the
primary May revision. The performance comparison covers the prechecked
insertion path. Additional cases cover accounting windows, execution
grouping, and load delivery. Section 7.5 reports the new schedule, reset,
and differential tests.
The supporting records contain 275 numbered decisions and 37 experiment
summaries. Results below retain each run's validator count, load setting,
measurement window, and original outcome. [R13, R14]

The measured binaries do not have an established per-run source match in
this evidence. For April code analysis, revision `37d3608` from 27 April
is a post-hoc source proxy. It already contains fixed-count draining and
the default-on insertion flag. It follows runs 180-182 and the partial-flush
revert. It does not identify the exact source of each binary used for runs 118-219.
The development log supplies those measurements and their change history.
The primary May revision `5dabe02` supplies later implementation facts.
Source-derived failure paths and reconstructed generator defaults must
be read with this distinction. [R13, R15, R29]

Two commit-rate definitions appear in these measurements.
The sampled average summarizes commit-progress samples during the load
window. The final-count rate divides the final committed count by the
requested load duration: `R_total = C_final / T_load`.
The final count can include warmup and drain work. Run 131 used a
warmup-counting convention that increased this rate. We retain the two
metrics separately. [R13]

The load generator computes achieved requests per second (RPS) from its
measured observations. That count includes error outcomes. Request
completion percentiles measure the RPC path. Transactions per second
(TPS) describe the stated commit counter and time window. Admission,
execution, durable commit, and client-observed finality are distinct
endpoints in this pipeline. [R12, R13]

The local harness invokes `--workload place-order` without side
or price overrides. The generator defaults are `--side bid` and
`--price-ticks 1000`. The April source proxy has the same defaults.
Fixed bid and ask modes emit a constant price. These defaults reconstruct
the generator shape; complete per-run environment and argument records
are not available for every selected measurement. [R29]

| Workload field | Harness or generator setting | Interpretation |
|---|---|---|
| Request type | Limit `PlaceOrder`; GTC; `post_only=false` | Insertion-only operation mix |
| Side and price | Default bid; constant 1,000 ticks | Same-side equal-price orders append by sequence |
| Size and markets | Quantity one lot; market/count are wrapper options, default 7/1 | Per-run market overrides are not uniformly recorded |
| Timing | Five-second harness warmup; requested load window; final drain | Sampled and final-count windows differ |
| Structural eligibility | Effectively 100% in steady-state eligible bodies under this one-sided construction | A designed best case, not a measured real-flow hit rate |

Table 3. Reconstructed workload settings. Empty and singleton bodies,
rejections, and additional runtime items can still bypass the direct path.
The table does not infer a deployment-wide eligible fraction. [R29]

Real-flow eligibility is unknown. The RPC metrics expose resting-batch
attempts and acceptances, but these are helper-level counts. Market-sharded
helpers count market groups, while the general executor also records
contiguous resting batches. Early whole-body declines are not all counted,
and direct-path statistics are recorded after success. Thus accepted/attempts
is not the fraction of all execution bodies that selected this fast path.
No comparable live hit rate is available for this evaluation. [R29]

### 7.2. Prechecked insertion measurements

| Run | Sampled insertions/s | Final batch p90 ms | Role |
|---|---|---|---|
| 170 | 87,254.6 | 248 | Earlier development anchor |
| 177 | 87,438.6 | 265 | Earlier profiled anchor |
| 180 | 97,768.6 | 95 | Prechecked path opt-in |
| 182 | 98,314.7 | 69 | Default-on confirmation |

Table 4. Local 100k/s insertion runs. Each used four validator processes
on one host and a 60-second requested load window. Batch p90 is the final
execution-batch counter. The best-case workload is defined in Table 3. [R13]

Runs 170 and 177 provide earlier development anchors. Run 180 used
`KOMBAT_PHASE6_INPLACE_RESTING_COMMIT=1`, then default-off. Its sampled
rate is about 12.0% above run 170 and 11.8% above run 177. The final batch
p90 counter changed from 265 ms in run 177 to 95 ms in run 180. These
observations support the design's performance motivation. The runs are
not randomized paired trials: code, profiling, accumulated book state, and
host conditions were not held fixed across repeated measurements. They
do not isolate the causal effect of removing the overlays. [R13]

Run 182 promoted the path to default-on. Unset enables it; `0` or `false`
disables it. The described implementation has this later default. The
confirmation run recorded 98,314.7 sampled insertions/s with no flag
override, zero reported request errors, request latency p50 1.314 ms,
p90 16.187 ms, and p99 70.225 ms. Its final execution-batch counter had
p90 69 ms and maximum 133 ms. The final sample retained 3,712 pre-admitted
items and no pending relay items. [R13]

The high-offer run 181 supplies a useful boundary for the result. Against
a nominal 150k/s target, the client delivered 121,706.0 RPS and the sampled
commit rate was 102,906.9 insertions/s. The record reports zero request
errors and a common-height convergence check, but 297,856 pre-admitted
items remained in the final sample. The faster insertion path therefore
still had an execution-drain limit at this offered load. [R13]

Run 180 records request latency p50 2.125 ms, p90 28.834 ms, and
p99 88.235 ms, with zero reported request errors. Its final batch maximum
is 145 ms. Run 177 also records a profiler-derived nonempty-batch p90 of
236 ms. That statistic uses a different population from its 265 ms final
counter and is kept separate. Request latency and batch execution time
also have different endpoints. [R13]

Run 180's 6,499,592 final commits divided by 60 seconds yield 108,326.5/s.
Run 182's 6,499,088 yield 108,318.1/s. Warmup completions and post-load
drain enter those final counts while the denominator remains 60 seconds.
This inflates the final-count rates above the nominal 100k/s target.
They are not in-window completion rates. The record does not separate
the warmup and drain shares. Table 4 uses the sampled rates instead. [R13]

These observations concern April runs before the May durable backend.
The 27 April source proxy supports the mechanism analysis, without an
exact per-run binary match. The measurements support the restricted
insertion path and its operating limits. They do not measure the later
durable system or a crossing-order workload.

### 7.3. Distributed operation and measurement context

| Run | Setting | Result | Measurement context |
|---|---|---|---|
| 118 | Local four; 60k offered; 60 s | 25,302.1 sampled TPS; 34,357.6 final/duration | Different accounting windows |
| 131 | Local four; 60k offered; 60 s | 35,737.6 sampled TPS; 59,260.6 final/duration | Final count includes the warmup convention |
| 217 | Distributed four; 100k target; 60 s | 1,970,295 final commits; 32,838.2 final/duration | Partial-flush change rejected after root divergence |
| 219 | Distributed four; 100k target; 60 s | 5,122,920 final commits; 85,382.0 final/duration | Six execution blocks remained pending |
| 274b | Distributed four; three load generators; 60k target; 60 s | 2,533,272 final commits; 42,221.2 final/duration | Client delivery below target |
| 275 | Distributed four; four load generators; 40k target; 60 s | 1,936,496 final commits; 32,274.9 final/duration | Delivery collapse and a retained queue tail |

Table 5. Additional April measurements. Local four denotes four validator
processes on one host. Distributed four denotes the four-host configuration.
Final/duration uses the definition in Section 7.1. [R13]

Run 219 used four validators and twelve ingress endpoints. Dividing its
5,122,920 final commits by the 60-second load duration gives 85,382.0/s.
Six execution blocks remained pending. Runs 217 and 219 share a nominal
100k/s target, but run 217 accepted 4,291,864 requests and committed
1,970,295 intents with divergent execution roots. Run 219 accepted and
committed 5,122,920 with matching roots. Their final-count rates differ
by about 2.60 times. Different accepted cohorts and a failed correctness
check prevent use of that ratio as an optimization speedup. The cause of
the committed-rate gap remains unresolved. [R13]

Run 274b used three external EU load generators and delivered 41,635.1 RPS
against a 60k/s target. Run 275 used four load generators and encountered
delivery collapse and an undrained tail. Delivery and policy-nonce gaps
can constrain a run before it identifies an ordering limit. [R13]

Accounting also changes comparisons. The final-count rate is 1.358 times
the sampled rate in run 118 and 1.658 times in run 131. These ratios
compare counters with different windows. Their different values prevent
use of one common correction factor. [R13]

### 7.4. Recorded correctness checks

| Contract | Existing assertion | Recorded coverage |
|---|---|---|
| Pinned-price replay | Different local price maps apply the same pinned payload | Node Mahi filter passed; body and resting-order agreement |
| Observation freshness | A merged body places a stale observation after a fresh one | Node Mahi filter passed; observation and engine maps |
| Durable root separation | Store and restore distinct header and executor roots | Node persistence filter passed; backend and metadata cases |
| Successful prechecked insertion | Height advances; policy nonce reaches three; both orders rest | Node Mahi filter passed; one two-order case |
| Unsupported mixed-side body | Declines with unchanged height, nonce, and resting orders | Node Mahi filter passed; one early-fallback case |

Table 6. Assertion scopes within recorded passing filters. The supporting
data retain exact names, commands, and counts. Overlapping filters are
not aggregated. These recorded filters were not rerun; Section 7.5
reports the tests that were written and run for this paper. [R14, R27]

The two insertion cases directly support the implemented native branch.
They do not supply a differential comparison with general execution.
The append predicate explains a narrower book update. Section 7.5 adds
the differential check of policy state, events, and roots.

The grouping case recorded matching DAG commit-log hashes with different
execution roots. Fixed grouping removes the identified local partition
condition within a common segment. The oracle case motivated pinned
observations and newest-timestamp selection. These recorded cases motivate
the execution contracts. The tests in Section 7.5 check them at the
studied revision. The insertion measurements provide performance context
for the implementation. [R13, R15, R16]

### 7.5. Schedule, reset, and differential tests

We wrote three test groups against the primary May revision `5dabe02`.
They add no production code. Table 7 summarizes the results. [R30]

| Test | Construction | Result |
|---|---|---|
| Fixed-count drain | 351 replica schedules; batch sizes 2, 4, 8; sequences of 16, 37, 64 blocks | Canonical groups and one execution head on every schedule |
| Partial-flush model | 39 schedules; batch size 8; 64 blocks | Different groups and heads on all 39; same consumed sequence |
| Batch-size mismatch | Batch sizes 4 and 8; one 64-block sequence | Same consumed sequence; heights 16 and 8; different heads |
| Failed verification | Block 20 of 64 marked `Failed`; 20 schedules | Two groups execute; 48 blocks remain queued |
| Simultaneous reset | Four validators reset at one head | Roots equal through common height 14 |
| Staggered reset | One of four validators resets; peers continue | Roots differ from height 5, the first after the reset |
| Path differential | 400 scenarios; 2,400 steps; direct path on and off | Roots and committed trading state equal; events and cleanup differ |

Table 7. Tests written for this paper at revision `5dabe02`. The reset
rows repeated with the same heights in three runs. [R30]

**Schedule tests.** Each simulated replica owns a queue and an engine.
Replicas receive one decided sequence of empty blocks. A pseudo-random
schedule interleaves block arrival, verification in arbitrary order, and
drain calls. Drained blocks are applied in consecutive chunks of the batch
size, as the commit executor does. The fixed-count rows use the implemented
drain function. The partial-flush row uses a test-local model of the
reverted rule: drain the whole verified prefix when every queued block is
verified, and otherwise a multiple of the batch size. It reproduces the
mechanism of Section 4.1. It is not the April patch or binary. [R15, R30]

**Reset tests.** These use the existing four-validator in-process cluster
with real block exchange and no client load. After 14 driven rounds every
validator holds height 4 and five queued decided blocks. The test then
calls the production reset on the selected validators, drives 30 further
rounds on all four, and compares execution state roots at each common
height. The staggered case ends with two distinct commit-log hashes.
In an inspected run the reset validator produced one block in those 30
rounds and skipped 29 producer calls for lack of ancestor quorum. The
reset is an in-process state replacement. It is not a process restart
from disk, and the harness has four validators rather than the deployed
three. [R30]

**Path differential.** The direct path is selected by a process-wide flag,
so the corpus runs twice in separate processes. Each scenario has four
policies and two markets. Each step merges one to three blocks holding
two to eight intents. Body shapes include append-compatible inserts,
priority-improving orders, two-sided bodies, crossing orders, duplicate
client-order keys, cancels, and nonce gaps. Half the scenarios first admit
their intents to the mempool. Some policies carry a small daily intent
limit, which makes the payload filter reject and cascade. A per-step
digest covers the head, header roots, executor root, policy state, books,
mandate metadata, collateral, positions with and without the stored fill sequence, events, intent
statuses, and mempool size. [R30]

The direct path executed 1,472 of the 2,400 steps. Outcome, height, head
hash, state root, event root, body root, executor root, policy state,
books, mandate metadata, and collateral matched on all 2,400 steps.
`IntentRejected` events were absent on 203 direct-path steps, and rejected
intents stayed in the mempool on 179 of them. Positions differed on 50
steps, only in the stored last-fill event sequence; positions without
that field matched on every step. Differences appeared
in 170 of the 400 scenarios. The corpus uses unlisted markets, so margin
and leverage checks are not exercised. Every step succeeded in both modes,
so it does not test application-error handling. [R30]

## 8. Discussion and Scope

The main design result is a common execution partition whose membership
does not depend on local verification timing. The partial-flush incident
makes this requirement concrete. The fixed-group lemma supplies the
within-segment construction. Pinned replay inputs, explicit error
semantics, and recovery boundaries state what else the implementation
must preserve around that construction.

This contract applies whenever execution-unit boundaries affect headers,
events, or state. Transport batching alone can use a different contract
when it leaves the transition unchanged. The distinction helps separate
local scheduling freedom from decisions that belong to replicated state.
The application-error analysis adds an important implementation boundary:
an atomic storage batch can preserve a mutated error result, so it does
not by itself provide a state-preserving retry.

The prechecked insertion path illustrates the limits of optimization
inside this interface. Its predicate gives a direct book update and its
recorded workload shows positive sampled-rate observations. The generator
also targets its best case. General clone reduction and real-flow
eligibility require separate evaluation. The May per-candidate filter copies
and outer engine clone prevent transfer of the April gain to deployment
without measurement.

The evidence consists of source analysis, recorded single development runs,
finite regression results, and the tests in Section 7.5. Raw traces and
uniform host records for the April runs are incomplete. No durable
crossing-order measurement was run. The new tests are finite. The schedule
tests use empty blocks. The reset test is an in-process reset on four
validators. The differential corpus omits margin checks and error paths.
The grouping lemma assumes a common segment and successful execution or
a suitable retry state. The implementation has no proof of
application-error atomicity, and the staggered reset shows that it does
not preserve a common sequence across recovery. The three-validator
deployment has zero fault tolerance for continued progress. Price truth
relies on trusted inputs, and published Mahi-Mahi guarantees do not
transfer to the modified configuration.

The reset result sets the main design priority: an agreed recovery
boundary, or execution groups derived from committed leader sub-DAGs with
a persisted position. The next empirical priorities are a process-restart
test on the three-validator configuration, a durable crossing-order run,
and whole-body eligibility instrumentation. Those are future work rather
than completion requirements for this experience report.

AI tools assisted implementation tracing, manuscript drafting, and
consistency checks. The author is responsible for the final technical
account, references, and conclusions.

## 9. Conclusion

0x26 Chain defines a concrete interface between DAG ordering and stateful
trading execution. Fixed-count FIFO grouping removes local verification
timing from group membership within a common segment. Pinned price
observations make replay inputs explicit. Separate header and executor
roots state what execution history and durable restoration each commit to.
These mechanisms provide the constructive execution account.

The integration evidence identifies the conditions around that account.
A local partial flush changed execution roots despite matching DAG commit
logs. Requeueing preserves group order but does not undo partial application
state, and an outer durable clone does not imply rollback on an inner error.
Failed payload verification stops the current queue. Operators must keep
the batch size common within a segment. Tests at the studied revision
confirm the grouping rule across 351 replica schedules and reproduce the
flush mechanism in a model. They also show that a single-validator reset
diverges at the next execution height. Recovery therefore needs an agreed
sequence position and segment boundary, not only a group cursor. These
conditions define the scope of the ordering-to-execution contracts.

The insertion fast path and its measurements supply a scoped performance
case. They show how a narrow update rule can avoid commit-stage overlays.
A differential corpus finds equal roots and committed trading state on
both paths, with differences confined to rejection events, queue cleanup,
and one stored event sequence. The measurements use a best-case generator. The later path retains both
per-intent filter copies and an outer engine clone, so the April gain
does not estimate deployed performance. The paper offers a reusable contract for reasoning about
execution grouping, replay inputs, error semantics, and recovery in a
DAG-ordered trading ledger.

## References

- [R01] Jovanovic, P., Kokoris Kogias, L., Kumara, B., Sonnino, A., Tennage, P., and Zablotchi, I. Mahi-Mahi: Low-Latency Asynchronous BFT DAG-Based Consensus. ICDCS, 2025. [Paper](https://doi.org/10.1109/ICDCS63083.2025.00060)
- [R02] Yin, M., Malkhi, D., Reiter, M. K., Gueta, G. G., and Abraham, I. HotStuff: BFT Consensus in the Lens of Blockchain. PODC, 2019. [Paper](https://arxiv.org/abs/1803.05069v6)
- [R03] Malkhi, D., and Nayak, K. Extended Abstract: HotStuff-2: Optimal Two-Phase Responsive BFT. Cryptology ePrint Archive 2023/397, 2023. [Paper](https://eprint.iacr.org/2023/397)
- [R04] Gelashvili, R., Kokoris-Kogias, L., Sonnino, A., Spiegelman, A., and Xiang, Z. Jolteon and Ditto: Network-Adaptive Efficient Consensus with Asynchronous Fallback. Financial Cryptography and Data Security (FC), 2022. [Paper](https://fc22.ifca.ai/preproceedings/35.pdf)
- [R05] Danezis, G., Kokoris Kogias, E., Sonnino, A., and Spiegelman, A. Narwhal and Tusk: A DAG-based Mempool and Efficient BFT Consensus. EuroSys, 2022. [Paper](https://arxiv.org/abs/2105.11827v4)
- [R06] Spiegelman, A., Giridharan, N., Sonnino, A., and Kokoris-Kogias, L. Bullshark: DAG BFT Protocols Made Practical. ACM CCS, 2022. [Paper](https://arxiv.org/abs/2201.05677v3)
- [R07] Babel, K., Chursin, A., Danezis, G., Kichidis, A., Kokoris-Kogias, L., Koshy, A., Sonnino, A., and Tian, M. Mysticeti: Reaching the Latency Limits with Uncertified DAGs. NDSS, 2025. [Paper](https://www.ndss-symposium.org/ndss-paper/mysticeti-reaching-the-latency-limits-with-uncertified-dags/)
- [R08] Gelashvili, R., Spiegelman, A., Xiang, Z., Danezis, G., Li, Z., Malkhi, D., Xia, Y., and Zhou, R. Block-STM: Scaling Blockchain Execution by Turning Ordering Curse to a Performance Blessing. ACM PPoPP, 2023. [Paper](https://arxiv.org/abs/2203.06871v3)
- [R09] Schneider, F. B. Implementing Fault-Tolerant Services Using the State Machine Approach: A Tutorial. ACM Computing Surveys 22(4), 1990. [Paper](https://www.cs.cornell.edu/fbs/publications/SMSurvey.pdf)
- [R10] Mohan, C., Haderle, D., Lindsay, B., Pirahesh, H., and Schwarz, P. ARIES: A Transaction Recovery Method Supporting Fine-Granularity Locking and Partial Rollbacks Using Write-Ahead Logging. ACM Transactions on Database Systems, 1992. [Paper](https://research.ibm.com/publications/aries-a-transaction-recovery-method-supporting-fine-granularity-locking-and-partial-rollbacks-using-write-ahead-logging)
- [R11] Tennage, P., and contributors. Mahi-Mahi reference implementation. Source code, revision 13a2819800ad3b192ab3c54dfe3ec4d0b34ae361. [Code](https://github.com/PasinduTennage/mahi-mahi-consensus)

- [R12] 0x26 Chain project. 0x26 Chain implementation and deployment configuration. Version 5dabe0284f1ef0bf6aede21a90ecf466685a2b3d, 25 May 2026. Software and technical documentation.
- [R13] 0x26 Chain project. Throughput development measurements and experiment outcomes. April 2026. Selected run data accompany this paper in `anc/evaluation.json`.
- [R14] 0x26 Chain project. Implementation validation and evaluation records. 16 November 2026. Supplementary material: `anc/evaluation.json`.
- [R15] 0x26 Chain project. Execution-grouping case and repair. 26 April 2026. Case summary and change identifiers accompany this paper in `anc/execution-cases.json`.
- [R16] 0x26 Chain project. Oracle replay case and repair follow-up. 4 May 2026. Case summary accompanies this paper in `anc/execution-cases.json`.
- [R17] Hyperliquid. HyperCore overview: HyperBFT consensus and execution. Technical documentation, accessed 16 November 2026. [Documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/hypercore/overview)
- [R18] dYdX. Limit Order Book and Matching. dYdX Chain technical documentation, accessed 16 November 2026. [Documentation](https://docs.dydx.exchange/concepts-limit-order-book-and-matching)
- [R19] Marsh, B., Landers, S., Jog, J., and Ranchal-Pedrosa, A. Sei Giga v2.0. arXiv:2505.14914, 2025, revision 3, 2026. [Paper](https://arxiv.org/abs/2505.14914v3)
- [R20] Sui. Consensus. Sui technical documentation, accessed 16 November 2026. [Documentation](https://docs.sui.io/develop/sui-architecture/consensus)
- [R21] Arun, B., Li, Z., Suri-Payer, F., Das, S., and Spiegelman, A. Shoal++: High Throughput DAG BFT Can Be Fast and Robust! USENIX NSDI, 2025, pp. 813-826. [Paper](https://www.usenix.org/conference/nsdi25/presentation/arun)
- [R22] Thomson, A., Diamond, T., Weng, S.-C., Ren, K., Shao, P., and Abadi, D. J. Calvin: Fast Distributed Transactions for Partitioned Database Systems. ACM SIGMOD, 2012. [Paper](https://www.cs.yale.edu/homes/dna/papers/calvin-sigmod12.pdf)
- [R23] Skip. Connect: Architecture. Technical documentation for the vote-extension oracle, accessed 16 November 2026. [Documentation](https://docs.skip.build/connect/learn/architecture)

- [R24] Anjana, P. S., Amini, M., Kapoor, R., Parmar, R., Ramesh, R., Ravi, S., and Tobkin, J. Efficient Parallel Execution of Blockchain Transactions Leveraging Conflict Specifications. AFT, 2025. [Paper](https://arxiv.org/abs/2503.03203v2)
- [R25] Mitenkov, G., Kabiljo, I., Li, Z., Spiegelman, A., Vusirikala, S., Xiang, Z., Zlateski, A., Lopes, N. P., and Gelashvili, R. Deferred Objects to Enhance Smart Contract Programming with Optimistic Parallel Execution. 2024. [Paper](https://arxiv.org/abs/2405.06117v1)
- [R26] Thompson, M., Farley, D., Barker, M., Gee, P., and Stewart, A. Disruptor: High Performance Alternative to Bounded Queues for Exchanging Data between Concurrent Threads. LMAX, 2011. [Technical paper](https://lmax-exchange.github.io/disruptor/disruptor.html)
- [R27] 0x26 Chain project. Prechecked order-insertion mechanism and supporting cases. 2026. Selected implementation excerpts, test scopes, and analysis accompany this paper in `anc/prechecked-execution.json`.

- [R28] Driscoll, J. R., Sarnak, N., Sleator, D. D., and Tarjan, R. E. Making Data Structures Persistent. Journal of Computer and System Sciences 38(1), 1989, pp. 86-124. [Paper](https://www.cs.cmu.edu/~sleator/papers/making-data-structures-persistent.pdf)
- [R29] 0x26 Chain project. Application-error, workload, and telemetry contract analysis. 2026. Selected source excerpts and scope records accompany this paper in `anc/application-contracts.json`.
- [R30] 0x26 Chain project. Schedule, reset, and differential tests at revision 5dabe028. Test revision 7e85dfef2fb091f9a0ed98547e42137b32c6ceff, 17 November 2026. Test sources, commands, and results accompany this paper in `anc/execution-tests.json` and `anc/tests/`.
