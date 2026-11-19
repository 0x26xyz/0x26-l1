pub mod backend;

pub use backend::{
    current_persistence_fingerprint, export, export_to_vec, import, import_from_slice,
    logical_snapshot_header_from_slice, migrate_logical_snapshot_slice_to_vec,
    migrate_logical_snapshot_to_vec, AnyReadTxn, AnyStateBackend, AnyWriteBatch, BackendError,
    BackendFingerprintEnvelope, CommitMeta, ImportError, InMemoryBackend, KvIter, KvPair,
    LogicalSnapshotError, LogicalSnapshotHeader, LogicalSnapshotMigrationError,
    LogicalSnapshotMigrationSummary, Namespace, OpenOptions, PersistenceFingerprintCheck, ReadTxn,
    RedbBackend, StateBackend, SyncMode, WriteBatch, LOGICAL_SNAPSHOT_ENVELOPE_VERSION,
    LOGICAL_SNAPSHOT_FORMAT_V2, LOGICAL_SNAPSHOT_MAGIC, META_KEY_LATEST,
    PERSISTENCE_ENVELOPE_MAGIC, PERSISTENCE_ENVELOPE_VERSION, PERSISTENCE_FINGERPRINT_KEY,
};

use borsh::{io, BorshSerialize};
use kombat_chain_consensus::Hash;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

pub const CRATE_NAME: &str = "state-db";

pub type StateKey = Vec<u8>;
pub type StateValue = Vec<u8>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateError {
    RootMismatch { expected: Hash, actual: Hash },
}

pub trait StateView {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>>;
    fn set(&mut self, key: &[u8], value: &[u8]);
    fn delete(&mut self, key: &[u8]);
}

pub trait StateCheckpoint: StateView {
    fn base_root(&self) -> Hash;
    fn compute_root(&self) -> Hash;
}

pub trait CheckpointableStateStore {
    type Checkpoint: StateCheckpoint + Clone;

    fn committed_root(&self) -> Hash;
    fn checkpoint(&self) -> Self::Checkpoint;
    fn commit(&mut self, checkpoint: Self::Checkpoint) -> Result<Hash, StateError>;
}

/// In-memory state store with copy-on-write checkpoint semantics.
///
/// The committed map is held behind an `Arc`, so taking a checkpoint is `O(1)` —
/// it clones only the `Arc` handle, not the underlying `BTreeMap`. Checkpoint
/// mutations accumulate in a per-checkpoint delta overlay; the base map is
/// never mutated through a checkpoint. Committing produces a fresh `Arc` with
/// the delta applied; other live checkpoints keep their old `Arc` and observe
/// pre-commit state until they are dropped (and will fail a later `commit()`
/// with `RootMismatch` because their `base_root` no longer matches).
///
/// This is a prerequisite for Phase 6 (Block-STM) parallel execution, where
/// multiple workers speculate against concurrent checkpoints without the
/// memory cost of per-checkpoint full-state clones.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InMemoryStateStore {
    committed: Arc<BTreeMap<StateKey, StateValue>>,
    committed_root: Hash,
}

/// Per-checkpoint pending write in the delta overlay.
///
/// `Set` shadows the base entry with a new value. `Delete` marks the key as
/// removed even if the base map still has it. The choice is stored explicitly
/// rather than inferred from absence, because absence in the delta means "no
/// change" (read falls through to base), while `Delete` means "hide the base
/// entry for reads and root computation."
#[derive(Clone, Debug, PartialEq, Eq)]
enum DeltaValue {
    Set(StateValue),
    Delete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InMemoryCheckpoint {
    base_root: Hash,
    base: Arc<BTreeMap<StateKey, StateValue>>,
    delta: BTreeMap<StateKey, DeltaValue>,
    cached_root: RefCell<Option<Hash>>,
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        let committed: Arc<BTreeMap<StateKey, StateValue>> = Arc::new(BTreeMap::new());
        let committed_root = compute_state_root_from_entries(&committed);
        Self {
            committed,
            committed_root,
        }
    }

    pub fn with_committed_root(committed_root: Hash) -> Self {
        Self {
            committed: Arc::new(BTreeMap::new()),
            committed_root,
        }
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (StateKey, StateValue)>) -> Self {
        let committed: BTreeMap<_, _> = entries.into_iter().collect();
        let committed_root = compute_state_root_from_entries(&committed);
        Self {
            committed: Arc::new(committed),
            committed_root,
        }
    }

    pub fn committed_entries(&self) -> &BTreeMap<StateKey, StateValue> {
        &self.committed
    }
}

impl StateView for InMemoryCheckpoint {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        match self.delta.get(key) {
            Some(DeltaValue::Set(value)) => Some(value.clone()),
            Some(DeltaValue::Delete) => None,
            None => self.base.get(key).cloned(),
        }
    }

    fn set(&mut self, key: &[u8], value: &[u8]) {
        self.delta
            .insert(key.to_vec(), DeltaValue::Set(value.to_vec()));
        self.cached_root.replace(None);
    }

    fn delete(&mut self, key: &[u8]) {
        self.delta.insert(key.to_vec(), DeltaValue::Delete);
        self.cached_root.replace(None);
    }
}

impl StateCheckpoint for InMemoryCheckpoint {
    fn base_root(&self) -> Hash {
        self.base_root
    }

    fn compute_root(&self) -> Hash {
        if let Some(root) = *self.cached_root.borrow() {
            return root;
        }

        // No pending changes — the effective map equals the base map, so the
        // base root is still valid. Skip the full overlay walk.
        if self.delta.is_empty() {
            self.cached_root.replace(Some(self.base_root));
            return self.base_root;
        }

        let root = compute_state_root_from_overlay(&self.base, &self.delta);
        self.cached_root.replace(Some(root));
        root
    }
}

impl CheckpointableStateStore for InMemoryStateStore {
    type Checkpoint = InMemoryCheckpoint;

    fn committed_root(&self) -> Hash {
        self.committed_root
    }

    fn checkpoint(&self) -> Self::Checkpoint {
        InMemoryCheckpoint {
            base_root: self.committed_root,
            base: Arc::clone(&self.committed),
            delta: BTreeMap::new(),
            cached_root: RefCell::new(Some(self.committed_root)),
        }
    }

    fn commit(&mut self, checkpoint: Self::Checkpoint) -> Result<Hash, StateError> {
        let InMemoryCheckpoint {
            base_root,
            base,
            delta,
            cached_root,
        } = checkpoint;

        if base_root != self.committed_root {
            return Err(StateError::RootMismatch {
                expected: self.committed_root,
                actual: base_root,
            });
        }

        // Fast path: checkpoint made no changes, commit is a no-op.
        if delta.is_empty() {
            return Ok(self.committed_root);
        }

        // Apply the delta onto a fresh map. We cannot mutate `base` in place
        // because other live checkpoints may still hold references to the same
        // `Arc`; `make_mut` would either clone (defeating the win) or mutate
        // shared state (unsound). An explicit dereference-and-clone here is
        // the price of isolation, and runs only once per commit rather than
        // once per checkpoint.
        let mut new_committed: BTreeMap<StateKey, StateValue> = (*base).clone();
        for (key, delta_val) in delta {
            match delta_val {
                DeltaValue::Set(value) => {
                    new_committed.insert(key, value);
                }
                DeltaValue::Delete => {
                    new_committed.remove(&key);
                }
            }
        }

        let new_root = cached_root
            .into_inner()
            .unwrap_or_else(|| compute_state_root_from_entries(&new_committed));

        self.committed = Arc::new(new_committed);
        self.committed_root = new_root;
        Ok(self.committed_root)
    }
}

fn compute_state_root_from_entries(entries: &BTreeMap<StateKey, StateValue>) -> Hash {
    let mut writer = HashWriter(Sha256::new());
    (entries.len() as u32)
        .serialize(&mut writer)
        .expect("state entry count must serialize");
    for (key, value) in entries {
        key.serialize(&mut writer)
            .expect("state key must serialize");
        value
            .serialize(&mut writer)
            .expect("state value must serialize");
    }
    writer.finish()
}

/// Compute the state root over `base + delta` without materialising the merged
/// map. The wire encoding must match `compute_state_root_from_entries` exactly
/// so legacy roots stay stable: count of entries (u32 borsh), followed by each
/// `(key, value)` pair in key-sorted order. `DeltaValue::Delete` hides the
/// corresponding base entry. `DeltaValue::Set` shadows it.
fn compute_state_root_from_overlay(
    base: &BTreeMap<StateKey, StateValue>,
    delta: &BTreeMap<StateKey, DeltaValue>,
) -> Hash {
    // First pass: count effective entries so the borsh count prefix matches.
    let mut count: u32 = 0;
    {
        let mut base_iter = base.iter().peekable();
        let mut delta_iter = delta.iter().peekable();
        loop {
            match (base_iter.peek(), delta_iter.peek()) {
                (None, None) => break,
                (Some((bk, _)), Some((dk, dv))) => match bk.cmp(dk) {
                    std::cmp::Ordering::Less => {
                        count += 1;
                        base_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        if matches!(dv, DeltaValue::Set(_)) {
                            count += 1;
                        }
                        delta_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        if matches!(dv, DeltaValue::Set(_)) {
                            count += 1;
                        }
                        base_iter.next();
                        delta_iter.next();
                    }
                },
                (Some(_), None) => {
                    count += 1;
                    base_iter.next();
                }
                (None, Some((_, dv))) => {
                    if matches!(dv, DeltaValue::Set(_)) {
                        count += 1;
                    }
                    delta_iter.next();
                }
            }
        }
    }

    // Second pass: hash entries in sorted order, delta values overriding base.
    let mut writer = HashWriter(Sha256::new());
    count
        .serialize(&mut writer)
        .expect("state entry count must serialize");

    let mut base_iter = base.iter().peekable();
    let mut delta_iter = delta.iter().peekable();
    loop {
        match (base_iter.peek(), delta_iter.peek()) {
            (None, None) => break,
            (Some((bk, bv)), Some((dk, dv))) => match bk.cmp(dk) {
                std::cmp::Ordering::Less => {
                    bk.serialize(&mut writer).expect("state key must serialize");
                    bv.serialize(&mut writer)
                        .expect("state value must serialize");
                    base_iter.next();
                }
                std::cmp::Ordering::Greater => {
                    if let DeltaValue::Set(dv) = dv {
                        dk.serialize(&mut writer).expect("state key must serialize");
                        dv.serialize(&mut writer)
                            .expect("state value must serialize");
                    }
                    delta_iter.next();
                }
                std::cmp::Ordering::Equal => {
                    if let DeltaValue::Set(dv) = dv {
                        dk.serialize(&mut writer).expect("state key must serialize");
                        dv.serialize(&mut writer)
                            .expect("state value must serialize");
                    }
                    base_iter.next();
                    delta_iter.next();
                }
            },
            (Some((bk, bv)), None) => {
                bk.serialize(&mut writer).expect("state key must serialize");
                bv.serialize(&mut writer)
                    .expect("state value must serialize");
                base_iter.next();
            }
            (None, Some((dk, dv))) => {
                if let DeltaValue::Set(dv) = dv {
                    dk.serialize(&mut writer).expect("state key must serialize");
                    dv.serialize(&mut writer)
                        .expect("state value must serialize");
                }
                delta_iter.next();
            }
        }
    }

    writer.finish()
}

struct HashWriter(Sha256);

impl HashWriter {
    fn finish(self) -> Hash {
        self.0.finalize().into()
    }
}

impl io::Write for HashWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use kombat_chain_consensus::Hash;
    use sha2::{Digest, Sha256};

    use super::{
        component_name, CheckpointableStateStore, InMemoryStateStore, StateCheckpoint, StateError,
        StateKey, StateValue, StateView,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn legacy_root(entries: &BTreeMap<StateKey, StateValue>) -> Hash {
        let canonical_entries: Vec<_> = entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let bytes = borsh::to_vec(&canonical_entries).expect("state entries must serialize");

        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher.finalize().into()
    }

    #[test]
    fn reports_component_name() {
        assert_eq!(component_name(), "state-db");
    }

    #[test]
    fn computes_deterministic_committed_roots() {
        let left = InMemoryStateStore::from_entries(vec![
            (b"b".to_vec(), b"2".to_vec()),
            (b"a".to_vec(), b"1".to_vec()),
        ]);
        let right = InMemoryStateStore::from_entries(vec![
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec()),
        ]);

        assert_eq!(left.committed_root(), right.committed_root());
    }

    #[test]
    fn hides_uncommitted_writes_until_commit() {
        let mut store = InMemoryStateStore::new();
        let mut checkpoint = store.checkpoint();
        checkpoint.set(b"policy/7", b"active");

        assert_eq!(store.committed_entries().get(b"policy/7".as_slice()), None);

        let committed_root = store.commit(checkpoint).expect("checkpoint should commit");

        assert_eq!(
            store.committed_entries().get(b"policy/7".as_slice()),
            Some(&b"active".to_vec())
        );
        assert_eq!(store.committed_root(), committed_root);
    }

    #[test]
    fn rejects_checkpoint_from_stale_root() {
        let mut store = InMemoryStateStore::new();
        let mut old_checkpoint = store.checkpoint();
        old_checkpoint.set(b"a", b"1");

        let mut new_checkpoint = store.checkpoint();
        new_checkpoint.set(b"b", b"2");
        store
            .commit(new_checkpoint)
            .expect("new checkpoint should commit");

        let err = store
            .commit(old_checkpoint)
            .expect_err("stale checkpoint should fail");

        assert!(matches!(err, StateError::RootMismatch { .. }));
    }

    #[test]
    fn deletes_keys_inside_checkpoint() {
        let mut store = InMemoryStateStore::from_entries(vec![(b"a".to_vec(), b"1".to_vec())]);
        let mut checkpoint = store.checkpoint();
        checkpoint.delete(b"a");

        let computed_root = checkpoint.compute_root();
        store.commit(checkpoint).expect("checkpoint should commit");

        assert_eq!(store.committed_entries().get(b"a".as_slice()), None);
        assert_eq!(store.committed_root(), computed_root);
    }

    #[test]
    fn preserves_legacy_root_encoding_without_extra_entry_clones() {
        let entries = BTreeMap::from([
            (b"a".to_vec(), b"1".to_vec()),
            (b"longer-key".to_vec(), b"payload".to_vec()),
        ]);

        let store = InMemoryStateStore::from_entries(entries.clone());

        assert_eq!(store.committed_root(), legacy_root(&entries));
    }

    #[test]
    fn reuses_computed_checkpoint_root_on_commit() {
        let mut store = InMemoryStateStore::from_entries(vec![(b"a".to_vec(), b"1".to_vec())]);
        let mut checkpoint = store.checkpoint();
        checkpoint.set(b"b", b"2");

        let computed_root = checkpoint.compute_root();
        let committed_root = store.commit(checkpoint).expect("checkpoint should commit");

        assert_eq!(committed_root, computed_root);
        assert_eq!(store.committed_root(), computed_root);
    }

    // --- New CoW-specific tests ---

    #[test]
    fn checkpoint_clone_shares_base_arc() {
        // After the CoW change, two checkpoints from the same store must share
        // the same underlying base map via Arc, not hold independent clones.
        // This is the structural win that makes Phase 6 (Block-STM) workable.
        let store = InMemoryStateStore::from_entries(vec![(b"policy/1".to_vec(), b"v1".to_vec())]);
        let a = store.checkpoint();
        let b = store.checkpoint();
        assert!(Arc::ptr_eq(&a.base, &b.base));
        // And they share with the store itself.
        assert!(Arc::ptr_eq(&a.base, &store.committed));
    }

    #[test]
    fn commit_does_not_mutate_other_live_checkpoints() {
        // After committing one checkpoint, any other live checkpoint must
        // still observe pre-commit state through its Arc'd base and must fail
        // commit() with RootMismatch (isolation invariant).
        let mut store = InMemoryStateStore::from_entries(vec![(b"a".to_vec(), b"1".to_vec())]);

        let sibling = store.checkpoint();
        let mut writer_checkpoint = store.checkpoint();
        writer_checkpoint.set(b"b", b"2");
        store
            .commit(writer_checkpoint)
            .expect("writer should commit");

        // Sibling still sees only "a".
        assert_eq!(sibling.get(b"a"), Some(b"1".to_vec()));
        assert_eq!(sibling.get(b"b"), None);

        // Sibling's attempt to commit now fails because its base_root is stale.
        let err = store
            .commit(sibling)
            .expect_err("sibling checkpoint must not replay onto advanced state");
        assert!(matches!(err, StateError::RootMismatch { .. }));
    }

    #[test]
    fn delete_without_prior_value_is_noop_in_root() {
        // Deleting a key that doesn't exist in base must not affect the root.
        let mut store = InMemoryStateStore::from_entries(vec![(b"a".to_vec(), b"1".to_vec())]);
        let expected_root = store.committed_root();

        let mut checkpoint = store.checkpoint();
        checkpoint.delete(b"nonexistent");

        assert_eq!(checkpoint.compute_root(), expected_root);

        let committed_root = store
            .commit(checkpoint)
            .expect("checkpoint should commit cleanly");
        assert_eq!(committed_root, expected_root);
    }

    #[test]
    fn set_then_delete_same_key_leaves_no_trace() {
        // A set followed by a delete of the same key within one checkpoint
        // must produce the same effective state as if neither had happened.
        let store = InMemoryStateStore::from_entries(vec![(b"a".to_vec(), b"1".to_vec())]);
        let expected_root = store.committed_root();

        let mut checkpoint = store.checkpoint();
        checkpoint.set(b"transient", b"scratch");
        checkpoint.delete(b"transient");

        assert_eq!(checkpoint.get(b"transient"), None);
        assert_eq!(checkpoint.compute_root(), expected_root);
    }

    #[test]
    fn overlay_root_matches_materialised_root() {
        // The overlay root computation must produce byte-identical hashes to
        // materialising base + delta and hashing directly. If these diverge
        // under any combination of set/delete shapes, cross-validator state
        // convergence breaks at commit time — the critical correctness
        // invariant for multi-validator Mahi.
        let base_entries = vec![
            (b"account/1".to_vec(), b"balance=10".to_vec()),
            (b"account/2".to_vec(), b"balance=20".to_vec()),
            (b"account/3".to_vec(), b"balance=30".to_vec()),
            (b"policy/7".to_vec(), b"active".to_vec()),
        ];

        let mut store = InMemoryStateStore::from_entries(base_entries.clone());
        let mut checkpoint = store.checkpoint();

        // Mix of set (new), set (overwrite), delete (existing), delete (absent).
        checkpoint.set(b"account/4", b"balance=40");
        checkpoint.set(b"account/1", b"balance=11");
        checkpoint.delete(b"account/2");
        checkpoint.delete(b"policy/nonexistent");

        let overlay_root = checkpoint.compute_root();

        // Build the materialised equivalent and hash it directly.
        let mut materialised: BTreeMap<StateKey, StateValue> = base_entries.into_iter().collect();
        materialised.insert(b"account/4".to_vec(), b"balance=40".to_vec());
        materialised.insert(b"account/1".to_vec(), b"balance=11".to_vec());
        materialised.remove(&b"account/2".to_vec());
        let materialised_root = super::compute_state_root_from_entries(&materialised);

        assert_eq!(overlay_root, materialised_root);

        // Commit and verify the store ends up with the same root.
        let committed_root = store.commit(checkpoint).expect("checkpoint should commit");
        assert_eq!(committed_root, overlay_root);
    }

    #[test]
    fn large_delta_commit_matches_materialised_equivalent() {
        // Stress: a checkpoint with many sets over a non-trivial base must
        // commit to the same root as constructing the equivalent base+delta
        // map directly. Exercises the overlay iterator at non-trivial sizes.
        let base_entries: Vec<_> = (0..256)
            .map(|i| (format!("k{:04}", i).into_bytes(), vec![i as u8]))
            .collect();
        let mut store = InMemoryStateStore::from_entries(base_entries.clone());

        let mut checkpoint = store.checkpoint();
        for i in 256..512 {
            checkpoint.set(format!("k{:04}", i).as_bytes(), &[i as u8, 0xff]);
        }
        for i in 0..128 {
            checkpoint.delete(format!("k{:04}", i).as_bytes());
        }

        let overlay_root = checkpoint.compute_root();

        let mut materialised: BTreeMap<StateKey, StateValue> = base_entries.into_iter().collect();
        for i in 256..512 {
            materialised.insert(format!("k{:04}", i).into_bytes(), vec![i as u8, 0xff]);
        }
        for i in 0..128 {
            materialised.remove(&format!("k{:04}", i).into_bytes());
        }
        let materialised_root = super::compute_state_root_from_entries(&materialised);

        assert_eq!(overlay_root, materialised_root);
        let committed_root = store.commit(checkpoint).expect("checkpoint should commit");
        assert_eq!(committed_root, overlay_root);
    }
}
