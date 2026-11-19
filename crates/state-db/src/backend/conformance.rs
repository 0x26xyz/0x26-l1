//! Backend conformance corpus.
//!
//! These functions are generic over `StateBackend` and assert the
//! semantics every backend must obey. Each concrete backend module
//! calls them from its own `#[test]` functions; Phase 9 will run the
//! same corpus on every enabled backend in CI.
//!
//! Public to the `state-db` crate (and re-exported under `#[cfg(test)]`
//! so consumers can hook their own backends in if they ever exist).

#![cfg(test)]

use super::{
    BackendError, CommitMeta, KvPair, Namespace, ReadTxn, StateBackend, SyncMode, WriteBatch,
};
use kombat_chain_consensus::Hash;

/// Build a deterministic `CommitMeta` keyed by height. Lets every test
/// at a given height produce the same byte signature so the corpus can
/// later assert byte-identical state roots across backends.
pub fn meta(height: u64) -> CommitMeta {
    CommitMeta {
        schema_version: 1,
        height,
        block_hash: Hash::from([height as u8; 32]),
        chain_head_state_root: Hash::from([(height + 100) as u8; 32]),
        executor_state_root: Hash::from([(height + 200) as u8; 32]),
        event_seq: height * 10,
        timestamp_ms: 1_700_000_000_000 + height,
    }
}

/// Asserts:
/// - put / delete / cross-namespace key isolation
/// - snapshot isolation: a `read_txn` taken before commit2 still sees
///   commit1's view; a fresh `read_txn` after commit2 sees commit2.
pub fn batch_roundtrip<B: StateBackend>(backend: &B) {
    let mut batch = backend.batch();
    batch.put(Namespace::State, b"a", b"1");
    batch.put(Namespace::State, b"b", b"2");
    batch.put(Namespace::Events, &0u64.to_be_bytes(), b"evt0");
    batch.put(Namespace::DirectMempoolWal, b"intent-x", b"signed-x");
    backend
        .commit_batch(batch, meta(1), SyncMode::Sync)
        .expect("commit ok");

    let txn = backend.read_txn().expect("read_txn");
    assert_eq!(
        txn.get(Namespace::State, b"a").expect("get"),
        Some(b"1".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::State, b"b").expect("get"),
        Some(b"2".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::Events, &0u64.to_be_bytes())
            .expect("get"),
        Some(b"evt0".to_vec())
    );
    assert_eq!(
        txn.get(Namespace::DirectMempoolWal, b"intent-x")
            .expect("get"),
        Some(b"signed-x".to_vec())
    );
    // Cross-namespace miss: same key in a different namespace must not
    // collide.
    assert_eq!(txn.get(Namespace::Receipts, b"a").expect("get"), None);

    // Second batch: delete one key, add another. The first read_txn
    // must still observe the original snapshot.
    let mut batch2 = backend.batch();
    batch2.delete(Namespace::State, b"a");
    batch2.put(Namespace::State, b"c", b"3");
    backend
        .commit_batch(batch2, meta(2), SyncMode::Sync)
        .expect("commit ok");

    // Old txn unchanged (snapshot isolation).
    assert_eq!(
        txn.get(Namespace::State, b"a").expect("get"),
        Some(b"1".to_vec())
    );
    assert_eq!(txn.get(Namespace::State, b"c").expect("get"), None);

    // Fresh txn observes the new state.
    let txn2 = backend.read_txn().expect("read_txn");
    assert_eq!(txn2.get(Namespace::State, b"a").expect("get"), None);
    assert_eq!(
        txn2.get(Namespace::State, b"c").expect("get"),
        Some(b"3".to_vec())
    );
}

/// Asserts `latest_committed_meta` returns `None` on a fresh backend
/// and reflects each commit's meta atomically with its data.
pub fn meta_roundtrip<B: StateBackend>(backend: &B) {
    assert!(backend.latest_committed_meta().expect("meta").is_none());

    let mut batch = backend.batch();
    batch.put(Namespace::State, b"k", b"v");
    let m1 = meta(1);
    backend
        .commit_batch(batch, m1.clone(), SyncMode::Sync)
        .expect("commit");
    assert_eq!(backend.latest_committed_meta().expect("meta"), Some(m1));

    let m2 = meta(42);
    let batch = backend.batch();
    backend
        .commit_batch(batch, m2.clone(), SyncMode::Sync)
        .expect("commit");
    assert_eq!(backend.latest_committed_meta().expect("meta"), Some(m2));
}

/// Asserts:
/// - prefix scan returns ascending lexicographic order
/// - scope is namespace-scoped (sibling namespaces don't leak)
/// - sibling prefixes (`acd:`) don't match `acc:`
/// - empty prefix yields the whole namespace in order
/// - no-match prefix yields empty
pub fn prefix_scan_order<B: StateBackend>(backend: &B) {
    let mut batch = backend.batch();
    // Same prefix in two namespaces — must not leak across.
    batch.put(Namespace::State, b"acc:001", b"a1");
    batch.put(Namespace::State, b"acc:003", b"a3");
    batch.put(Namespace::State, b"acc:002", b"a2");
    batch.put(Namespace::State, b"pol:001", b"p1");
    batch.put(Namespace::Events, b"acc:001", b"event-a1");
    // Adjacent prefix that must NOT match scan of "acc:".
    batch.put(Namespace::State, b"acd:001", b"sibling");
    backend
        .commit_batch(batch, meta(1), SyncMode::Sync)
        .expect("commit");

    let txn = backend.read_txn().expect("read_txn");

    let collected: Vec<KvPair> = txn
        .iter_prefix(Namespace::State, b"acc:")
        .expect("iter")
        .map(|r| r.expect("kv"))
        .collect();
    assert_eq!(
        collected,
        vec![
            (b"acc:001".to_vec(), b"a1".to_vec()),
            (b"acc:002".to_vec(), b"a2".to_vec()),
            (b"acc:003".to_vec(), b"a3".to_vec()),
        ]
    );

    let all_state: Vec<Vec<u8>> = txn
        .iter_prefix(Namespace::State, b"")
        .expect("iter")
        .map(|r| r.expect("kv").0)
        .collect();
    assert_eq!(
        all_state,
        vec![
            b"acc:001".to_vec(),
            b"acc:002".to_vec(),
            b"acc:003".to_vec(),
            b"acd:001".to_vec(),
            b"pol:001".to_vec(),
        ]
    );

    // Cross-namespace scan stays in its own lane.
    let events_acc: Vec<Vec<u8>> = txn
        .iter_prefix(Namespace::Events, b"acc:")
        .expect("iter")
        .map(|r| r.expect("kv").1)
        .collect();
    assert_eq!(events_acc, vec![b"event-a1".to_vec()]);

    // No-match prefix yields empty.
    let zero: Vec<KvPair> = txn
        .iter_prefix(Namespace::State, b"zzz")
        .expect("iter")
        .map(|r| r.expect("kv"))
        .collect();
    assert!(zero.is_empty());
}

/// Asserts `delete_prefix_bounded` honours `max_keys`.
pub fn delete_prefix_bounded_caps<B: StateBackend>(backend: &B) {
    let mut batch = backend.batch();
    for i in 0..10u8 {
        batch.put(Namespace::DirectMempoolWal, &[b'k', i], &[i]);
    }
    backend
        .commit_batch(batch, meta(1), SyncMode::Sync)
        .expect("commit");

    let mut batch = backend.batch();
    batch.delete_prefix_bounded(Namespace::DirectMempoolWal, b"k", 4);
    backend
        .commit_batch(batch, meta(2), SyncMode::Sync)
        .expect("commit");

    let txn = backend.read_txn().expect("read_txn");
    let remaining: Vec<KvPair> = txn
        .iter_prefix(Namespace::DirectMempoolWal, b"k")
        .expect("iter")
        .map(|r| r.expect("kv"))
        .collect();
    assert_eq!(remaining.len(), 6, "should have deleted exactly 4 keys");
}

/// Convenience: run the entire conformance corpus against a freshly
/// constructed backend factory. Each test function expects a fresh
/// backend so the assertions are independent.
pub fn run_full_corpus<B, F>(mut factory: F)
where
    B: StateBackend,
    F: FnMut() -> B,
{
    batch_roundtrip(&factory());
    meta_roundtrip(&factory());
    prefix_scan_order(&factory());
    delete_prefix_bounded_caps(&factory());
}

// Hush unused-import warnings when the corpus is compiled in tests
// modules that only use a subset.
#[allow(dead_code)]
fn _ensure_backend_error_used(_e: BackendError) {}
