//! RFC 042 Phase 9: deterministic cross-backend corpus.
//!
//! The Phase 1 conformance corpus (`conformance.rs`) asserts that
//! each backend obeys the trait contract individually. Phase 9 goes
//! further: it runs the *same* deterministic scenario on every
//! enabled backend and asserts that their final logical exports are
//! **byte-identical**.
//!
//! That is the strongest possible cross-backend equivalence claim —
//! anything that differs (sort order, encoding, meta key shape,
//! namespace coverage) shows up as a byte-level diff. Phase 1
//! semantics aren't strong enough on their own: they let two
//! backends both pass the trait but diverge in iteration order or
//! the way they encode internal meta keys, which would silently
//! break replay or migration tooling that reads the logical export
//! format.
//!
//! Coverage: every namespace in `Namespace::ALL`, both `put` and
//! `delete` ops, including same-key value updates and overwrites,
//! plus a `delete_prefix_bounded` call.
//!
//! Phase 9 also publishes a small bench module (`bench`) that
//! reports commit latency, WAL admission latency, recovery time,
//! and file growth. Those tests are `#[ignore]` by default so a
//! plain `cargo test` stays fast — operators run them with
//! `cargo test --release -- --ignored phase9_bench`.

#![cfg(test)]

use super::{
    export_to_vec, CommitMeta, InMemoryBackend, Namespace, OpenOptions, ReadTxn, RedbBackend,
    StateBackend, SyncMode, WriteBatch,
};
use kombat_chain_consensus::Hash;

/// One step in the deterministic scenario. Every step is a single
/// `commit_batch` call so the scenario also exercises the meta-update
/// path that runs once per commit.
#[derive(Clone)]
struct Step {
    meta: CommitMeta,
    ops: Vec<Op>,
}

#[derive(Clone)]
enum Op {
    Put {
        ns: Namespace,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        ns: Namespace,
        key: Vec<u8>,
    },
    DeletePrefixBounded {
        ns: Namespace,
        prefix: Vec<u8>,
        max_keys: u32,
    },
}

fn meta_at(height: u64, event_seq: u64) -> CommitMeta {
    CommitMeta {
        schema_version: 1,
        height,
        block_hash: Hash::from([height as u8; 32]),
        chain_head_state_root: Hash::from([(height as u8).wrapping_add(0xA0); 32]),
        executor_state_root: Hash::from([(height as u8).wrapping_add(0xB0); 32]),
        event_seq,
        timestamp_ms: 1_700_000_000_000 + height,
    }
}

/// Build the scenario. Touches every namespace at least once,
/// includes overwrites at the same key, and runs a bounded prefix
/// delete on a WAL namespace. Output is fully deterministic — bytes
/// in are bytes out.
fn scenario() -> Vec<Step> {
    vec![
        // Step 1: seed all namespaces with a baseline put.
        Step {
            meta: meta_at(1, 0),
            ops: vec![
                Op::Put {
                    ns: Namespace::State,
                    key: b"acct:1".to_vec(),
                    value: b"v1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::Blocks,
                    key: 1u64.to_be_bytes().to_vec(),
                    value: b"block-1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::BlockHashIndex,
                    key: vec![0xb1; 32],
                    value: 1u64.to_be_bytes().to_vec(),
                },
                Op::Put {
                    ns: Namespace::Events,
                    key: 0u64.to_be_bytes().to_vec(),
                    value: b"evt-0".to_vec(),
                },
                Op::Put {
                    ns: Namespace::Receipts,
                    key: vec![0xc1; 32],
                    value: b"receipt-1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::Quarantine,
                    key: vec![0xd1; 32],
                    value: b"quarantined-1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::ConsensusSafety,
                    key: b"validator-set".to_vec(),
                    value: b"set-v1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::DirectMempoolWal,
                    key: vec![0xa1; 32],
                    value: b"signed-a1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::ConditionalMempoolWal,
                    key: vec![0xa2; 32],
                    value: b"signed-a2".to_vec(),
                },
                Op::Put {
                    ns: Namespace::PendingFaucetWal,
                    key: vec![0xe1; 20],
                    value: b"faucet-1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::PendingDelegationWal,
                    key: vec![0xf1; 32],
                    value: b"deleg-1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::PendingMmProgramWal,
                    key: vec![0xf2; 32],
                    value: b"mm-1".to_vec(),
                },
                Op::Put {
                    ns: Namespace::Telemetry,
                    key: b"counter:rounds".to_vec(),
                    value: b"7".to_vec(),
                },
            ],
        },
        // Step 2: overwrite a key (value-aware diff).
        Step {
            meta: meta_at(2, 1),
            ops: vec![
                Op::Put {
                    ns: Namespace::State,
                    key: b"acct:1".to_vec(),
                    value: b"v1-updated".to_vec(),
                },
                Op::Put {
                    ns: Namespace::PendingFaucetWal,
                    key: vec![0xe1; 20],
                    value: b"faucet-1-updated".to_vec(),
                },
                Op::Put {
                    ns: Namespace::Events,
                    key: 1u64.to_be_bytes().to_vec(),
                    value: b"evt-1".to_vec(),
                },
            ],
        },
        // Step 3: deletes (consume WAL rows + a state key).
        Step {
            meta: meta_at(3, 2),
            ops: vec![
                Op::Delete {
                    ns: Namespace::DirectMempoolWal,
                    key: vec![0xa1; 32],
                },
                Op::Delete {
                    ns: Namespace::PendingDelegationWal,
                    key: vec![0xf1; 32],
                },
                Op::Put {
                    ns: Namespace::Receipts,
                    key: vec![0xc2; 32],
                    value: b"receipt-2".to_vec(),
                },
            ],
        },
        // Step 4: bounded prefix delete on the conditional WAL.
        Step {
            meta: meta_at(4, 3),
            ops: vec![
                Op::Put {
                    ns: Namespace::ConditionalMempoolWal,
                    key: vec![0xa3; 32],
                    value: b"signed-a3".to_vec(),
                },
                Op::Put {
                    ns: Namespace::ConditionalMempoolWal,
                    key: vec![0xa4; 32],
                    value: b"signed-a4".to_vec(),
                },
                Op::DeletePrefixBounded {
                    ns: Namespace::ConditionalMempoolWal,
                    prefix: vec![0xa3],
                    max_keys: 16,
                },
            ],
        },
        // Step 5: more puts including a same-key overwrite of the
        // baseline telemetry row.
        Step {
            meta: meta_at(5, 4),
            ops: vec![
                Op::Put {
                    ns: Namespace::Telemetry,
                    key: b"counter:rounds".to_vec(),
                    value: b"42".to_vec(),
                },
                Op::Put {
                    ns: Namespace::Blocks,
                    key: 5u64.to_be_bytes().to_vec(),
                    value: b"block-5".to_vec(),
                },
            ],
        },
    ]
}

fn run_scenario_on<B: StateBackend>(backend: &B) {
    for step in scenario() {
        let mut batch = backend.batch();
        for op in &step.ops {
            match op {
                Op::Put { ns, key, value } => batch.put(*ns, key, value),
                Op::Delete { ns, key } => batch.delete(*ns, key),
                Op::DeletePrefixBounded {
                    ns,
                    prefix,
                    max_keys,
                } => batch.delete_prefix_bounded(*ns, prefix, *max_keys),
            }
        }
        backend
            .commit_batch(batch, step.meta, SyncMode::Sync)
            .expect("commit ok");
    }
}

/// `cross_backend_corpus_byte_identical_export`: the central Phase 9
/// claim. Run the same deterministic scenario on `InMemoryBackend`
/// and `RedbBackend`, export both, and assert byte-equality.
///
/// If this ever fails, the diff comes from the backends — not the
/// scenario — because the scenario itself is fixed and ops are
/// applied in identical order. A failure means: a backend has an
/// internal meta key the other doesn't, or its iteration order
/// differs from BTreeMap lexicographic, or it encodes a value
/// differently.
#[test]
fn cross_backend_corpus_byte_identical_export() {
    let in_mem = InMemoryBackend::new();
    run_scenario_on(&in_mem);
    let (in_mem_export, in_mem_meta) = export_to_vec(&in_mem).expect("inmem export");

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase9_corpus.redb");
    let redb = RedbBackend::open(OpenOptions::new(path)).expect("redb open");
    run_scenario_on(&redb);
    let (redb_export, redb_meta) = export_to_vec(&redb).expect("redb export");

    assert_eq!(in_mem_meta, redb_meta, "cross-backend export meta differs");
    assert_eq!(
        in_mem_export.len(),
        redb_export.len(),
        "backends produced different export sizes: in_mem={} redb={}",
        in_mem_export.len(),
        redb_export.len()
    );
    if in_mem_export != redb_export {
        // Find the first divergence to make debugging tractable.
        let first_diff = in_mem_export
            .iter()
            .zip(redb_export.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| in_mem_export.len().min(redb_export.len()));
        panic!(
            "cross-backend export diverges at byte offset {first_diff}; \
             in_mem byte = {:#04x}, redb byte = {:#04x}",
            in_mem_export.get(first_diff).copied().unwrap_or_default(),
            redb_export.get(first_diff).copied().unwrap_or_default()
        );
    }
}

/// `cross_backend_meta_byte_identical`: smaller and more diagnostic
/// — checks that the `latest_committed_meta` round-trip is
/// byte-identical between backends. If the export equality test
/// above fails but this one passes, the meta serialization is fine
/// and the divergence is in the data namespaces; if both fail, the
/// meta encoding is the problem.
#[test]
fn cross_backend_meta_byte_identical() {
    let in_mem = InMemoryBackend::new();
    run_scenario_on(&in_mem);
    let in_mem_meta = in_mem
        .latest_committed_meta()
        .expect("inmem meta")
        .expect("inmem meta present");

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let path = tmp.path().join("phase9_corpus_meta.redb");
    let redb = RedbBackend::open(OpenOptions::new(path)).expect("redb open");
    run_scenario_on(&redb);
    let redb_meta = redb
        .latest_committed_meta()
        .expect("redb meta")
        .expect("redb meta present");

    let in_mem_bytes = borsh::to_vec(&in_mem_meta).expect("encode inmem meta");
    let redb_bytes = borsh::to_vec(&redb_meta).expect("encode redb meta");
    assert_eq!(
        in_mem_bytes, redb_bytes,
        "cross-backend meta diverges; in_mem={in_mem_meta:?}, redb={redb_meta:?}"
    );
}

// =============================================================================
// Phase 9 bench report
// =============================================================================
//
// Plain `#[ignore]` tests so `cargo test` stays fast. Run with:
//
//   cargo test --release -p kombat-chain-state-db -- --ignored phase9_bench --nocapture
//
// Each prints a one-line summary covering the dimension RFC 042 calls
// out (commit latency, WAL admission latency, recovery time, file
// growth). They're not statistically rigorous — they exist to surface
// regressions visible to a human, not to publish numbers.

mod bench {
    use super::*;
    use std::time::Instant;

    /// `phase9_bench_commit_latency`: average latency of a `Sync`
    /// commit of one State put per batch on each backend.
    #[test]
    #[ignore]
    fn phase9_bench_commit_latency() {
        const N: u64 = 200;

        let in_mem = InMemoryBackend::new();
        let in_mem_t = Instant::now();
        for i in 0..N {
            let mut batch = in_mem.batch();
            batch.put(Namespace::State, &i.to_be_bytes(), b"v");
            in_mem
                .commit_batch(batch, meta_at(i + 1, i), SyncMode::Sync)
                .expect("commit");
        }
        let in_mem_avg_us = in_mem_t.elapsed().as_micros() as f64 / N as f64;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let redb_path = tmp.path().join("phase9_bench_commit.redb");
        let redb = RedbBackend::open(OpenOptions::new(redb_path)).expect("redb");
        let redb_t = Instant::now();
        for i in 0..N {
            let mut batch = redb.batch();
            batch.put(Namespace::State, &i.to_be_bytes(), b"v");
            redb.commit_batch(batch, meta_at(i + 1, i), SyncMode::Sync)
                .expect("commit");
        }
        let redb_avg_us = redb_t.elapsed().as_micros() as f64 / N as f64;

        println!(
            "[phase9_bench commit_latency] in_mem_avg={in_mem_avg_us:.1}us \
             redb_sync_avg={redb_avg_us:.1}us (n={N})"
        );
    }

    /// `phase9_bench_wal_admission_latency`: same shape as commit
    /// latency but the put targets `DirectMempoolWal` and we measure
    /// in both `Sync` and `GroupCommit { max_delay_ms: 5 }` modes on
    /// redb so the operator can see the queued-fsync speedup.
    #[test]
    #[ignore]
    fn phase9_bench_wal_admission_latency() {
        const N: u64 = 200;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let sync_path = tmp.path().join("phase9_bench_wal_sync.redb");
        let sync_redb = RedbBackend::open(OpenOptions::new(sync_path)).expect("redb sync");
        let sync_t = Instant::now();
        for i in 0..N {
            let mut batch = sync_redb.batch();
            let key: Vec<u8> = (0..32).map(|b| (i as u8).wrapping_add(b)).collect();
            batch.put(Namespace::DirectMempoolWal, &key, b"signed");
            sync_redb
                .commit_batch(batch, meta_at(i + 1, i), SyncMode::Sync)
                .expect("commit sync");
        }
        let sync_avg_us = sync_t.elapsed().as_micros() as f64 / N as f64;

        let group_path = tmp.path().join("phase9_bench_wal_group.redb");
        let group_redb = RedbBackend::open(OpenOptions::new(group_path)).expect("redb group");
        let group_t = Instant::now();
        for i in 0..N {
            let mut batch = group_redb.batch();
            let key: Vec<u8> = (0..32).map(|b| (i as u8).wrapping_add(b)).collect();
            batch.put(Namespace::DirectMempoolWal, &key, b"signed");
            group_redb
                .commit_batch(
                    batch,
                    meta_at(i + 1, i),
                    SyncMode::GroupCommit { max_delay_ms: 5 },
                )
                .expect("commit group");
        }
        let group_avg_us = group_t.elapsed().as_micros() as f64 / N as f64;

        println!(
            "[phase9_bench wal_admission_latency] redb_sync_avg={sync_avg_us:.1}us \
             redb_group_commit_avg={group_avg_us:.1}us (n={N})"
        );
    }

    /// `phase9_bench_recovery_time`: write N commits to a redb file,
    /// drop the backend, reopen it, walk every namespace via
    /// `iter_prefix(_, &[])`. Reports total wall-clock recovery
    /// time. This is a proxy for the cold-boot scan a node does to
    /// hydrate caches that live outside the canonical record.
    #[test]
    #[ignore]
    fn phase9_bench_recovery_time() {
        const N: u64 = 1_000;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let path = tmp.path().join("phase9_bench_recovery.redb");

        {
            let redb = RedbBackend::open(OpenOptions::new(path.clone())).expect("redb");
            for i in 0..N {
                let mut batch = redb.batch();
                let k: Vec<u8> = (0..32).map(|b| (i as u8).wrapping_add(b)).collect();
                batch.put(Namespace::State, &k, b"v");
                batch.put(Namespace::Events, &i.to_be_bytes(), b"evt");
                batch.put(Namespace::DirectMempoolWal, &k, b"signed");
                redb.commit_batch(batch, meta_at(i + 1, i), SyncMode::Sync)
                    .expect("commit");
            }
        }

        let recovered_t = Instant::now();
        let redb = RedbBackend::open(OpenOptions::new(path)).expect("redb reopen");
        let txn = redb.read_txn().expect("read_txn");
        let mut row_count = 0_usize;
        for ns in Namespace::ALL {
            let iter = txn.iter_prefix(*ns, &[]).expect("iter");
            for kv in iter {
                kv.expect("kv");
                row_count += 1;
            }
        }
        let recovery_ms = recovered_t.elapsed().as_millis();
        println!(
            "[phase9_bench recovery_time] redb_recover_ms={recovery_ms} rows={row_count} (n={N})"
        );
    }

    /// `phase9_bench_file_growth`: write N commits, report the file
    /// size on disk. Useful for spotting unbounded growth or
    /// fragmentation regressions over time.
    #[test]
    #[ignore]
    fn phase9_bench_file_growth() {
        const N: u64 = 1_000;

        let tmp = tempfile::TempDir::new().expect("tempdir");
        let path = tmp.path().join("phase9_bench_filesize.redb");
        let redb = RedbBackend::open(OpenOptions::new(path.clone())).expect("redb");
        for i in 0..N {
            let mut batch = redb.batch();
            let k: Vec<u8> = (0..32).map(|b| (i as u8).wrapping_add(b)).collect();
            batch.put(Namespace::State, &k, b"v");
            redb.commit_batch(batch, meta_at(i + 1, i), SyncMode::Sync)
                .expect("commit");
        }
        drop(redb);

        let bytes = std::fs::metadata(&path).expect("stat").len();
        println!(
            "[phase9_bench file_growth] redb_bytes={bytes} bytes_per_commit={:.1} (n={N})",
            bytes as f64 / N as f64
        );
    }
}
