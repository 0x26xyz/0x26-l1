//! redb-backed `StateBackend` implementation.
//!
//! redb crate types (`Database`, `WriteTransaction`, `Table`,
//! `AccessGuard`, ...) MUST stay private to this module. The public
//! surface returns owned `Vec<u8>` and `Box<dyn Iterator>`.
//!
//! Durability profile (per RFC 042):
//! - `SyncMode::Sync` with `OpenOptions::fsync_on_sync_commit = true`
//!   uses `redb::Durability::Immediate` — the only redb mode
//!   guaranteed persistent on commit return. "Ack means
//!   crash-durable."
//! - `SyncMode::Sync` with `fsync_on_sync_commit = false` uses
//!   `redb::Durability::Eventual` (testnet-only opt-in; ack does NOT
//!   imply crash-durability in this configuration).
//! - `SyncMode::GroupCommit { .. }` maps to `redb::Durability::Eventual`.
//!   Per redb's upstream docs, Eventual transactions are "queued and
//!   should become persistent after commit returns." Visible after
//!   clean shutdown; **NOT crash-durable at ack**. There is no real
//!   group coordination yet — the mode-name reserves the slot for a
//!   future "fsync once per N commits" implementation.
//! - `SyncMode::AsyncBatchedTestnet` likewise maps to `Eventual`.
//!   Same crash-durability tier as `GroupCommit`. Testnet only.
//! - `set_quick_repair(true)` is enabled on `Sync` commits when
//!   `OpenOptions::fsync_on_sync_commit = true` so a crashed redb file
//!   reopens fast; the cost is a slightly larger commit footprint, well
//!   inside the budget for a 50ms block target.

use super::{
    BackendError, BackendFingerprintEnvelope, CommitMeta, KvIter, KvPair, Namespace, OpenOptions,
    ReadTxn, StateBackend, SyncMode, WriteBatch, META_KEY_LATEST, PERSISTENCE_FINGERPRINT_KEY,
};
use ::redb::{
    Database, DatabaseError, Durability, ReadTransaction, ReadableTable, TableDefinition,
    TableError,
};
use borsh::{from_slice as borsh_from_slice, to_vec as borsh_to_vec};
use std::path::Path;
use std::sync::Arc;

// One redb `TableDefinition` per `Namespace`. Table names are part of
// the on-disk schema; never rename or repurpose without bumping
// `CommitMeta.schema_version`.
const TABLE_STATE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("state");
const TABLE_BLOCKS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("blocks");
const TABLE_BLOCK_HASH_INDEX: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("block_hash_index");
const TABLE_EVENTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("events");
const TABLE_RECEIPTS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("receipts");
const TABLE_QUARANTINE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("quarantine");
const TABLE_CONSENSUS_SAFETY: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("consensus_safety");
const TABLE_DIRECT_MEMPOOL_WAL: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("direct_mempool_wal");
const TABLE_CONDITIONAL_MEMPOOL_WAL: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("conditional_mempool_wal");
const TABLE_PENDING_FAUCET_WAL: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("pending_faucet_wal");
const TABLE_PENDING_DELEGATION_WAL: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("pending_delegation_wal");
const TABLE_PENDING_MM_PROGRAM_WAL: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("pending_mm_program_wal");
const TABLE_META: TableDefinition<&[u8], &[u8]> = TableDefinition::new("meta");
const TABLE_TELEMETRY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("telemetry");
const TABLE_EQUITY_HISTORY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("equity_history");
const TABLE_MANDATE_CONFORMANCE_HISTORY: TableDefinition<&[u8], &[u8]> =
    TableDefinition::new("mandate_conformance_history");

const ALL_NAMESPACES: &[Namespace] = &[
    Namespace::State,
    Namespace::Blocks,
    Namespace::BlockHashIndex,
    Namespace::Events,
    Namespace::Receipts,
    Namespace::Quarantine,
    Namespace::ConsensusSafety,
    Namespace::DirectMempoolWal,
    Namespace::ConditionalMempoolWal,
    Namespace::PendingFaucetWal,
    Namespace::PendingDelegationWal,
    Namespace::PendingMmProgramWal,
    Namespace::Meta,
    Namespace::Telemetry,
    Namespace::EquityHistory,
    Namespace::MandateConformanceHistory,
];

fn table_def(ns: Namespace) -> TableDefinition<'static, &'static [u8], &'static [u8]> {
    match ns {
        Namespace::State => TABLE_STATE,
        Namespace::Blocks => TABLE_BLOCKS,
        Namespace::BlockHashIndex => TABLE_BLOCK_HASH_INDEX,
        Namespace::Events => TABLE_EVENTS,
        Namespace::Receipts => TABLE_RECEIPTS,
        Namespace::Quarantine => TABLE_QUARANTINE,
        Namespace::ConsensusSafety => TABLE_CONSENSUS_SAFETY,
        Namespace::DirectMempoolWal => TABLE_DIRECT_MEMPOOL_WAL,
        Namespace::ConditionalMempoolWal => TABLE_CONDITIONAL_MEMPOOL_WAL,
        Namespace::PendingFaucetWal => TABLE_PENDING_FAUCET_WAL,
        Namespace::PendingDelegationWal => TABLE_PENDING_DELEGATION_WAL,
        Namespace::PendingMmProgramWal => TABLE_PENDING_MM_PROGRAM_WAL,
        Namespace::Meta => TABLE_META,
        Namespace::Telemetry => TABLE_TELEMETRY,
        Namespace::EquityHistory => TABLE_EQUITY_HISTORY,
        Namespace::MandateConformanceHistory => TABLE_MANDATE_CONFORMANCE_HISTORY,
    }
}

fn redb_io<E: std::fmt::Display>(detail: &str) -> impl Fn(E) -> BackendError + '_ {
    move |err| BackendError::Io(format!("{detail}: {err}"))
}

fn redb_open_error(err: DatabaseError) -> BackendError {
    match err {
        DatabaseError::DatabaseAlreadyOpen => {
            BackendError::Locked("redb open: database already open".into())
        }
        other => BackendError::Io(format!("redb open: {other}")),
    }
}

#[derive(Clone)]
pub struct RedbBackend {
    db: Arc<Database>,
    fsync_on_sync_commit: bool,
    persistence_fingerprint: [u8; 32],
}

impl RedbBackend {
    fn open_path(
        path: &Path,
        create_if_missing: bool,
        fsync_on_sync_commit: bool,
        initialize_tables: bool,
        abort_repair_on_open: bool,
        persistence_fingerprint: [u8; 32],
    ) -> Result<Self, BackendError> {
        let mut builder = Database::builder();
        if abort_repair_on_open {
            builder.set_repair_callback(|session| session.abort());
        } else {
            builder.set_repair_callback(|_session| {
                // Invoked if redb needs to repair on open. Do not abort:
                // a validator MUST repair to a known-good state before
                // serving. Phase 6 wires this to logs/metrics.
            });
        }
        let db = if create_if_missing {
            builder.create(path)
        } else {
            builder.open(path)
        }
        .map_err(redb_open_error)?;
        if initialize_tables {
            // Ensure all tables exist so subsequent reads/writes don't have
            // to materialise an empty table inside an awkward read txn.
            let mut wtxn = db.begin_write().map_err(redb_io("begin init txn"))?;
            if fsync_on_sync_commit {
                wtxn.set_durability(Durability::Immediate);
                wtxn.set_quick_repair(true);
            }
            for ns in ALL_NAMESPACES {
                wtxn.open_table(table_def(*ns))
                    .map_err(redb_io("open table during init"))?;
            }
            wtxn.commit().map_err(redb_io("commit init txn"))?;
        }
        Ok(Self {
            db: Arc::new(db),
            fsync_on_sync_commit,
            persistence_fingerprint,
        })
    }
}

impl StateBackend for RedbBackend {
    type ReadTxn<'a> = RedbReadTxn;
    type WriteBatch = RedbWriteBatch;

    fn open(opts: OpenOptions) -> Result<Self, BackendError> {
        if !opts.create_if_missing && !opts.path.exists() {
            return Err(BackendError::Io(format!(
                "redb path does not exist: {}",
                opts.path.display()
            )));
        }
        Self::open_path(
            &opts.path,
            opts.create_if_missing,
            opts.fsync_on_sync_commit,
            opts.initialize_tables,
            opts.abort_repair_on_open,
            opts.persistence_fingerprint,
        )
    }

    fn read_txn(&self) -> Result<Self::ReadTxn<'_>, BackendError> {
        let txn = self.db.begin_read().map_err(redb_io("begin read txn"))?;
        Ok(RedbReadTxn { txn })
    }

    fn batch(&self) -> Self::WriteBatch {
        RedbWriteBatch::default()
    }

    fn commit_batch(
        &self,
        batch: Self::WriteBatch,
        meta: CommitMeta,
        sync: SyncMode,
    ) -> Result<(), BackendError> {
        let mut wtxn = self.db.begin_write().map_err(redb_io("begin write txn"))?;
        // Block-commit callers always pass `Sync`. Admission callers
        // (Phase 7) may opt in to weaker durability for testnet.
        let durability = match sync {
            SyncMode::Sync if self.fsync_on_sync_commit => Durability::Immediate,
            SyncMode::Sync => Durability::Eventual,
            SyncMode::GroupCommit { .. } => Durability::Eventual,
            SyncMode::AsyncBatchedTestnet => Durability::Eventual,
        };
        wtxn.set_durability(durability);
        if matches!(sync, SyncMode::Sync) && self.fsync_on_sync_commit {
            wtxn.set_quick_repair(true);
        }

        // Apply ops first so that a failed serialisation/encoding does
        // not leave a "meta written, body missing" footprint. (redb
        // gives us all-or-nothing per txn anyway, but the order keeps
        // the failure mode boring.)
        for op in batch.ops {
            match op {
                Op::Put(ns, key, value) => {
                    let mut table = wtxn
                        .open_table(table_def(ns))
                        .map_err(redb_io("open table for put"))?;
                    table
                        .insert(key.as_slice(), value.as_slice())
                        .map_err(redb_io("table insert"))?;
                }
                Op::Delete(ns, key) => {
                    let mut table = wtxn
                        .open_table(table_def(ns))
                        .map_err(redb_io("open table for delete"))?;
                    table
                        .remove(key.as_slice())
                        .map_err(redb_io("table remove"))?;
                }
                Op::DeletePrefixBounded(ns, prefix, max_keys) => {
                    let mut table = wtxn
                        .open_table(table_def(ns))
                        .map_err(redb_io("open table for delete-prefix"))?;
                    let upper = prefix_upper_bound(&prefix);
                    // Collect victims first to avoid mutating during iter.
                    let victims: Vec<Vec<u8>> = match upper {
                        Some(ref u) => table
                            .range::<&[u8]>((
                                std::ops::Bound::Included(prefix.as_slice()),
                                std::ops::Bound::Excluded(u.as_slice()),
                            ))
                            .map_err(redb_io("range for delete-prefix"))?
                            .take(max_keys as usize)
                            .map(|r| {
                                r.map(|(k, _)| k.value().to_vec())
                                    .map_err(redb_io("range item for delete-prefix"))
                            })
                            .collect::<Result<_, _>>()?,
                        None => table
                            .range::<&[u8]>((
                                std::ops::Bound::Included(prefix.as_slice()),
                                std::ops::Bound::Unbounded,
                            ))
                            .map_err(redb_io("range for delete-prefix"))?
                            .take(max_keys as usize)
                            .map(|r| {
                                r.map(|(k, _)| k.value().to_vec())
                                    .map_err(redb_io("range item for delete-prefix"))
                            })
                            .collect::<Result<_, _>>()?,
                    };
                    for k in victims {
                        table
                            .remove(k.as_slice())
                            .map_err(redb_io("delete-prefix remove"))?;
                    }
                }
            }
        }

        // Atomic meta update: same write txn, commits with the rest.
        let meta_bytes = borsh_to_vec(&meta)
            .map_err(|err| BackendError::Corrupt(format!("encode CommitMeta: {err}")))?;
        let fingerprint_envelope =
            BackendFingerprintEnvelope::current(self.persistence_fingerprint);
        let fingerprint_bytes = borsh_to_vec(&fingerprint_envelope).map_err(|err| {
            BackendError::Corrupt(format!("encode BackendFingerprintEnvelope: {err}"))
        })?;
        {
            let mut meta_table = wtxn
                .open_table(TABLE_META)
                .map_err(redb_io("open meta table"))?;
            meta_table
                .insert(META_KEY_LATEST, meta_bytes.as_slice())
                .map_err(redb_io("insert meta"))?;
            meta_table
                .insert(PERSISTENCE_FINGERPRINT_KEY, fingerprint_bytes.as_slice())
                .map_err(redb_io("insert persistence fingerprint"))?;
        }

        wtxn.commit().map_err(redb_io("commit txn"))?;
        Ok(())
    }

    fn latest_committed_meta(&self) -> Result<Option<CommitMeta>, BackendError> {
        let rtxn = self
            .db
            .begin_read()
            .map_err(redb_io("begin read for meta"))?;
        let table = match rtxn.open_table(TABLE_META) {
            Ok(t) => t,
            Err(::redb::TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(err) => return Err(BackendError::Io(format!("open meta: {err}"))),
        };
        let value = table.get(META_KEY_LATEST).map_err(redb_io("get meta"))?;
        match value {
            None => Ok(None),
            Some(guard) => {
                let bytes = guard.value().to_vec();
                let meta: CommitMeta = borsh_from_slice(&bytes)
                    .map_err(|err| BackendError::Corrupt(format!("decode CommitMeta: {err}")))?;
                Ok(Some(meta))
            }
        }
    }

    fn expected_persistence_fingerprint(&self) -> [u8; 32] {
        self.persistence_fingerprint
    }
}

/// Compute the exclusive upper bound of a byte prefix for prefix scans.
/// Returns `None` if `prefix` is empty or all `0xff`, in which case
/// callers should treat the upper bound as "+∞".
fn prefix_upper_bound(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut upper = prefix.to_vec();
    while let Some(last) = upper.last_mut() {
        if *last == 0xff {
            upper.pop();
            continue;
        }
        *last = last.wrapping_add(1);
        return Some(upper);
    }
    None
}

#[derive(Default)]
pub struct RedbWriteBatch {
    ops: Vec<Op>,
}

enum Op {
    Put(Namespace, Vec<u8>, Vec<u8>),
    Delete(Namespace, Vec<u8>),
    DeletePrefixBounded(Namespace, Vec<u8>, u32),
}

impl WriteBatch for RedbWriteBatch {
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

pub struct RedbReadTxn {
    txn: ReadTransaction,
}

impl ReadTxn for RedbReadTxn {
    fn get(&self, namespace: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, BackendError> {
        // Migration probes open old backends without table
        // initialization. Missing read tables are therefore empty; the
        // write path still fails loudly if a caller tries to mutate an
        // unmaterialised namespace unexpectedly.
        let table = match self.txn.open_table(table_def(namespace)) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(err) => return Err(redb_io("open table for get")(err)),
        };
        match table.get(key).map_err(redb_io("get"))? {
            None => Ok(None),
            Some(guard) => Ok(Some(guard.value().to_vec())),
        }
    }

    fn iter_prefix<'a>(
        &'a self,
        namespace: Namespace,
        prefix: &[u8],
    ) -> Result<KvIter<'a>, BackendError> {
        // Eager collect: redb iterators borrow the table, and the
        // table borrows the transaction. The trait says the iterator
        // borrows from `&self` (the txn) — collecting up-front keeps
        // the lifetime story simple and matches the byte-owned trait
        // contract. Phase 9 can revisit if profiling demands it.
        // Missing tables are empty for the same migration-read reason
        // as `get`; writes intentionally do not swallow this case.
        let table = match self.txn.open_table(table_def(namespace)) {
            Ok(table) => table,
            Err(TableError::TableDoesNotExist(_)) => {
                return Ok(Box::new(std::iter::empty::<Result<KvPair, BackendError>>()));
            }
            Err(err) => return Err(redb_io("open table for iter")(err)),
        };
        let upper = prefix_upper_bound(prefix);
        let collected: Vec<Result<KvPair, BackendError>> = match upper {
            Some(ref u) => table
                .range::<&[u8]>((
                    std::ops::Bound::Included(prefix),
                    std::ops::Bound::Excluded(u.as_slice()),
                ))
                .map_err(redb_io("range for iter"))?
                .map(|r| {
                    r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec()))
                        .map_err(redb_io("range item"))
                })
                .collect(),
            None => table
                .range::<&[u8]>((
                    std::ops::Bound::Included(prefix),
                    std::ops::Bound::Unbounded,
                ))
                .map_err(redb_io("range for iter"))?
                .map(|r| {
                    r.map(|(k, v)| (k.value().to_vec(), v.value().to_vec()))
                        .map_err(redb_io("range item"))
                })
                .collect(),
        };
        Ok(Box::new(collected.into_iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::conformance;
    use tempfile::TempDir;

    fn open_fresh() -> (RedbBackend, TempDir) {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("state.redb");
        let backend = RedbBackend::open(OpenOptions::new(path)).expect("open");
        (backend, tmp)
    }

    #[test]
    fn state_backend_redb_batch_roundtrip() {
        let (backend, _tmp) = open_fresh();
        conformance::batch_roundtrip(&backend);
    }

    #[test]
    fn state_backend_redb_meta_roundtrip() {
        let (backend, _tmp) = open_fresh();
        conformance::meta_roundtrip(&backend);
    }

    #[test]
    fn state_backend_redb_prefix_scan_order() {
        let (backend, _tmp) = open_fresh();
        conformance::prefix_scan_order(&backend);
    }

    #[test]
    fn state_backend_redb_delete_prefix_bounded() {
        let (backend, _tmp) = open_fresh();
        conformance::delete_prefix_bounded_caps(&backend);
    }

    /// Reopen-after-close: a redb-backed file must replay the last
    /// committed meta + state when reopened. This is the proof point
    /// for the Phase 6 durability beachhead. (Process kill testing
    /// arrives in Phase 6; this is the cheap reopen check.)
    #[test]
    fn state_backend_redb_persists_across_reopen() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("state.redb");
        let expected_fingerprint = crate::backend::current_persistence_fingerprint();

        // First session: write a batch + meta and commit durably.
        {
            let backend = RedbBackend::open(OpenOptions::new(path.clone())).expect("open");
            let mut batch = backend.batch();
            batch.put(Namespace::State, b"persistent", b"yes");
            batch.put(Namespace::Events, &7u64.to_be_bytes(), b"e7");
            backend
                .commit_batch(batch, conformance::meta(99), SyncMode::Sync)
                .expect("commit");
        }

        // Second session: reopen and read.
        let backend = RedbBackend::open(OpenOptions::new(path)).expect("reopen");
        let txn = backend.read_txn().expect("read_txn");
        assert_eq!(
            txn.get(Namespace::State, b"persistent").expect("get"),
            Some(b"yes".to_vec())
        );
        assert_eq!(
            txn.get(Namespace::Events, &7u64.to_be_bytes())
                .expect("get"),
            Some(b"e7".to_vec())
        );
        assert_eq!(
            backend.latest_committed_meta().expect("meta"),
            Some(conformance::meta(99))
        );
        assert_eq!(
            backend.check_persistence_fingerprint().expect("check"),
            crate::backend::PersistenceFingerprintCheck::Compatible {
                fingerprint: expected_fingerprint
            }
        );
    }

    #[test]
    fn state_backend_redb_detects_fingerprint_mismatch_on_reopen() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("state.redb");
        let source_fingerprint = [0x11; 32];
        let target_fingerprint = [0x22; 32];

        {
            let backend = RedbBackend::open(
                OpenOptions::new(path.clone()).with_persistence_fingerprint(source_fingerprint),
            )
            .expect("open");
            let mut batch = backend.batch();
            batch.put(Namespace::State, b"k", b"v");
            backend
                .commit_batch(batch, conformance::meta(1), SyncMode::Sync)
                .expect("commit");
            assert_eq!(
                backend.check_persistence_fingerprint().expect("check"),
                crate::backend::PersistenceFingerprintCheck::Compatible {
                    fingerprint: source_fingerprint
                }
            );
        }

        let reopened = RedbBackend::open(
            OpenOptions::new(path).with_persistence_fingerprint(target_fingerprint),
        )
        .expect("reopen");
        assert_eq!(
            reopened.check_persistence_fingerprint().expect("check"),
            crate::backend::PersistenceFingerprintCheck::SchemaFingerprintMismatch {
                expected: target_fingerprint,
                actual: source_fingerprint,
            }
        );
    }

    #[test]
    fn state_backend_redb_missing_tables_read_as_empty_when_not_initialized() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("state.redb");

        {
            let backend =
                RedbBackend::open(OpenOptions::new(path.clone()).without_table_initialization())
                    .expect("open without init");
            let mut batch = backend.batch();
            batch.put(Namespace::State, b"k", b"v");
            backend
                .commit_batch(batch, conformance::meta(1), SyncMode::Sync)
                .expect("commit");
        }

        let mut opts = OpenOptions::new(path).without_table_initialization();
        opts.create_if_missing = false;
        let reopened = RedbBackend::open(opts).expect("reopen without init");
        let txn = reopened.read_txn().expect("txn");
        assert_eq!(txn.get(Namespace::Receipts, b"missing").expect("get"), None);
        let receipts: Vec<_> = txn
            .iter_prefix(Namespace::Receipts, b"")
            .expect("iter")
            .collect::<Result<_, _>>()
            .expect("collect");
        assert!(receipts.is_empty());
    }

    #[test]
    fn prefix_upper_bound_empty_or_all_ffs_returns_none() {
        assert!(prefix_upper_bound(&[]).is_none());
        assert!(prefix_upper_bound(&[0xff]).is_none());
        assert!(prefix_upper_bound(&[0xff, 0xff, 0xff]).is_none());
    }

    #[test]
    fn prefix_upper_bound_increments_last_non_ff() {
        assert_eq!(prefix_upper_bound(b"acc:"), Some(b"acc;".to_vec()));
        assert_eq!(
            prefix_upper_bound(&[0x01, 0xff, 0x02]),
            Some(vec![0x01, 0xff, 0x03])
        );
        // Trailing 0xff bytes are popped before incrementing the next.
        assert_eq!(prefix_upper_bound(&[0x01, 0xff]), Some(vec![0x02]));
    }
}
