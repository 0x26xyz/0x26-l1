//! In-memory `StateBackend` implementation.
//!
//! Used for unit tests, the deterministic backend conformance corpus
//! (Phase 9), and short-lived dev runs. NOT a production backend:
//! `commit_batch` does no fsync because there is no file. The trait's
//! `Sync` semantics are honoured trivially because every commit is
//! atomic against the inner lock.
//!
//! Concurrency: a single `RwLock` over the inner store. A `read_txn`
//! takes a cheap snapshot via `Arc::clone`, so reads never block
//! writes once the txn is taken.

use super::{
    BackendError, BackendFingerprintEnvelope, CommitMeta, KvIter, KvPair, Namespace, OpenOptions,
    ReadTxn, StateBackend, SyncMode, WriteBatch, META_KEY_LATEST, PERSISTENCE_FINGERPRINT_KEY,
};
use borsh::{from_slice as borsh_from_slice, to_vec as borsh_to_vec};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Single key shape used by the in-memory backend's flat keyspace.
type FlatKey = (u8, Vec<u8>); // (namespace tag, key bytes)

/// Inner committed state. Held behind `Arc` so `read_txn` can snapshot
/// in O(1) by cloning the Arc.
#[derive(Default, Clone)]
struct CommittedStore {
    data: BTreeMap<FlatKey, Vec<u8>>,
}

pub struct InMemoryBackend {
    inner: RwLock<Arc<CommittedStore>>,
    persistence_fingerprint: [u8; 32],
}

impl InMemoryBackend {
    /// Construct a brand-new backend not backed by any file.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Arc::new(CommittedStore::default())),
            persistence_fingerprint: super::current_persistence_fingerprint(),
        }
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StateBackend for InMemoryBackend {
    type ReadTxn<'a> = InMemoryReadTxn;
    type WriteBatch = InMemoryWriteBatch;

    fn open(opts: OpenOptions) -> Result<Self, BackendError> {
        // The in-memory backend ignores `path` and always starts empty.
        Ok(Self {
            inner: RwLock::new(Arc::new(CommittedStore::default())),
            persistence_fingerprint: opts.persistence_fingerprint,
        })
    }

    fn read_txn(&self) -> Result<Self::ReadTxn<'_>, BackendError> {
        let guard = self
            .inner
            .read()
            .map_err(|_| BackendError::Internal("inner lock poisoned".into()))?;
        Ok(InMemoryReadTxn {
            snapshot: Arc::clone(&*guard),
        })
    }

    fn batch(&self) -> Self::WriteBatch {
        InMemoryWriteBatch::default()
    }

    fn commit_batch(
        &self,
        batch: Self::WriteBatch,
        meta: CommitMeta,
        _sync: SyncMode,
    ) -> Result<(), BackendError> {
        let meta_bytes = borsh_to_vec(&meta)
            .map_err(|e| BackendError::Corrupt(format!("encode CommitMeta: {e}")))?;
        let fingerprint_envelope =
            BackendFingerprintEnvelope::current(self.persistence_fingerprint);
        let fingerprint_bytes = borsh_to_vec(&fingerprint_envelope).map_err(|e| {
            BackendError::Corrupt(format!("encode BackendFingerprintEnvelope: {e}"))
        })?;
        let mut guard = self
            .inner
            .write()
            .map_err(|_| BackendError::Internal("inner lock poisoned".into()))?;
        // Copy-on-write: clone the Arc'd map for mutation, swap in.
        // Outstanding read txns keep their old snapshot.
        let mut next = (**guard).clone();
        for op in batch.ops {
            match op {
                Op::Put(ns, key, value) => {
                    next.data.insert((ns.tag(), key), value);
                }
                Op::Delete(ns, key) => {
                    next.data.remove(&(ns.tag(), key));
                }
                Op::DeletePrefixBounded(ns, prefix, max_keys) => {
                    delete_prefix_bounded(&mut next.data, ns, &prefix, max_keys);
                }
            }
        }
        // Atomic meta update: stored as a reserved key inside
        // Namespace::Meta. Mirrors redb so both backends produce
        // byte-identical logical exports.
        next.data.insert(
            (Namespace::Meta.tag(), META_KEY_LATEST.to_vec()),
            meta_bytes,
        );
        next.data.insert(
            (Namespace::Meta.tag(), PERSISTENCE_FINGERPRINT_KEY.to_vec()),
            fingerprint_bytes,
        );
        *guard = Arc::new(next);
        Ok(())
    }

    fn latest_committed_meta(&self) -> Result<Option<CommitMeta>, BackendError> {
        let guard = self
            .inner
            .read()
            .map_err(|_| BackendError::Internal("inner lock poisoned".into()))?;
        let bytes = guard
            .data
            .get(&(Namespace::Meta.tag(), META_KEY_LATEST.to_vec()));
        match bytes {
            None => Ok(None),
            Some(b) => {
                let meta: CommitMeta = borsh_from_slice(b)
                    .map_err(|e| BackendError::Corrupt(format!("decode CommitMeta: {e}")))?;
                Ok(Some(meta))
            }
        }
    }

    fn expected_persistence_fingerprint(&self) -> [u8; 32] {
        self.persistence_fingerprint
    }
}

fn delete_prefix_bounded(
    data: &mut BTreeMap<FlatKey, Vec<u8>>,
    namespace: Namespace,
    prefix: &[u8],
    max_keys: u32,
) {
    let tag = namespace.tag();
    let lower = (tag, prefix.to_vec());
    let mut victims: Vec<FlatKey> = Vec::new();
    for (k, _) in data
        .range(lower..)
        .take_while(|(k, _)| k.0 == tag && k.1.starts_with(prefix))
    {
        if (victims.len() as u32) >= max_keys {
            break;
        }
        victims.push(k.clone());
    }
    for k in victims {
        data.remove(&k);
    }
}

#[derive(Default)]
pub struct InMemoryWriteBatch {
    ops: Vec<Op>,
}

enum Op {
    Put(Namespace, Vec<u8>, Vec<u8>),
    Delete(Namespace, Vec<u8>),
    DeletePrefixBounded(Namespace, Vec<u8>, u32),
}

impl WriteBatch for InMemoryWriteBatch {
    fn put(&mut self, namespace: Namespace, key: &[u8], value: &[u8]) {
        self.ops
            .push(Op::Put(namespace, key.to_vec(), value.to_vec()));
    }

    fn delete(&mut self, namespace: Namespace, key: &[u8]) {
        self.ops.push(Op::Delete(namespace, key.to_vec()));
    }

    fn delete_prefix_bounded(&mut self, namespace: Namespace, prefix: &[u8], max_keys: u32) {
        self.ops.push(Op::DeletePrefixBounded(
            namespace,
            prefix.to_vec(),
            max_keys,
        ));
    }
}

pub struct InMemoryReadTxn {
    snapshot: Arc<CommittedStore>,
}

impl ReadTxn for InMemoryReadTxn {
    fn get(&self, namespace: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, BackendError> {
        Ok(self
            .snapshot
            .data
            .get(&(namespace.tag(), key.to_vec()))
            .cloned())
    }

    fn iter_prefix<'a>(
        &'a self,
        namespace: Namespace,
        prefix: &[u8],
    ) -> Result<KvIter<'a>, BackendError> {
        let tag = namespace.tag();
        let lower = (tag, prefix.to_vec());
        let prefix_owned = prefix.to_vec();
        let iter = self
            .snapshot
            .data
            .range(lower..)
            .take_while(move |(k, _)| k.0 == tag && k.1.starts_with(&prefix_owned))
            .map(|(k, v)| -> Result<KvPair, BackendError> { Ok((k.1.clone(), v.clone())) });
        Ok(Box::new(iter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::conformance;

    /// state_backend_in_memory_batch_roundtrip: shared corpus.
    #[test]
    fn state_backend_in_memory_batch_roundtrip() {
        conformance::batch_roundtrip(&InMemoryBackend::new());
    }

    /// state_backend_meta_roundtrip: shared corpus.
    #[test]
    fn state_backend_meta_roundtrip() {
        conformance::meta_roundtrip(&InMemoryBackend::new());
    }

    #[test]
    fn state_backend_fingerprint_missing_marker_detected_without_meta_decode() {
        let backend = InMemoryBackend::new();
        {
            let mut guard = backend.inner.write().expect("lock");
            let mut next = (**guard).clone();
            // Deliberately store malformed CommitMeta bytes. The
            // fingerprint check must report the missing marker without
            // trying to decode this value.
            next.data.insert(
                (Namespace::Meta.tag(), META_KEY_LATEST.to_vec()),
                b"not-borsh-meta".to_vec(),
            );
            *guard = Arc::new(next);
        }

        assert_eq!(
            backend.check_persistence_fingerprint().expect("check"),
            crate::backend::PersistenceFingerprintCheck::MissingMarker
        );
    }

    #[test]
    fn state_backend_fingerprint_corrupt_marker_is_envelope_invalid() {
        let backend = InMemoryBackend::new();
        {
            let mut guard = backend.inner.write().expect("lock");
            let mut next = (**guard).clone();
            next.data.insert(
                (Namespace::Meta.tag(), PERSISTENCE_FINGERPRINT_KEY.to_vec()),
                b"not-borsh-envelope".to_vec(),
            );
            *guard = Arc::new(next);
        }

        assert!(matches!(
            backend.check_persistence_fingerprint().expect("check"),
            crate::backend::PersistenceFingerprintCheck::EnvelopeDecodeInvalid { .. }
        ));
    }

    /// state_backend_prefix_scan_order: shared corpus.
    #[test]
    fn state_backend_prefix_scan_order() {
        conformance::prefix_scan_order(&InMemoryBackend::new());
    }

    /// delete_prefix_bounded honours the cap.
    #[test]
    fn delete_prefix_bounded_caps_deletions() {
        conformance::delete_prefix_bounded_caps(&InMemoryBackend::new());
    }
}
