//! Logical snapshot export/import.
//!
//! A *logical* snapshot is a backend-independent serialisation of every
//! durable namespace plus the latest `CommitMeta`. It is the canonical
//! shape used by `kombat_saveSnapshot` (Phase 8), by validator
//! bootstrap, and by the cross-backend conformance corpus (Phase 9).
//! Live raw-file copies are unsafe (mmap'd pages can race a concurrent
//! commit) and explicitly NOT supported.
//!
//! Export uses a `read_txn`, so the resulting snapshot reflects a
//! consistent point-in-time view even while writes continue. Import is
//! a single atomic `commit_batch` against a fresh backend.
//!
//! Format (binary, borsh-encoded throughout unless noted):
//!
//! ```text
//! [LogicalSnapshotHeader]
//!   magic:                   [u8; 4] = b"K26F"  (LOGICAL_SNAPSHOT_MAGIC)
//!   envelope_version:        u32     = 1
//!   persistence_fingerprint: [u8; 32]
//!   format_version:          u32     = 2
//!   meta:                    CommitMeta          (Borsh)
//! [Vec<LogicalSnapshotEntry>]                      (Borsh, length-prefixed)
//!   namespace_tag:   u8
//!   key:             Vec<u8>
//!   value:           Vec<u8>
//! ```
//!
//! Phase 3 buffers the entry vector in memory before serialising. That
//! is acceptable for a chain in the <100 GB regime; Phase 9 may swap to
//! chunked streaming if profiling shows the buffer is the bottleneck.

use super::{
    BackendError, CommitMeta, Namespace, PersistenceFingerprintCheck, ReadTxn, StateBackend,
    SyncMode, WriteBatch, PERSISTENCE_ENVELOPE_VERSION,
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{Read, Write};

/// Magic prefix on every RFC 055 logical snapshot file.
pub const LOGICAL_SNAPSHOT_MAGIC: [u8; 4] = *b"K26F";

pub const LOGICAL_SNAPSHOT_ENVELOPE_VERSION: u32 = PERSISTENCE_ENVELOPE_VERSION;

/// Format version of the logical snapshot stream. Bumped on any
/// incompatible layout change.
pub const LOGICAL_SNAPSHOT_FORMAT_V2: u32 = 2;

/// Header struct serialised at the start of every logical snapshot.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LogicalSnapshotHeader {
    pub magic: [u8; 4],
    pub envelope_version: u32,
    pub persistence_fingerprint: [u8; 32],
    pub format_version: u32,
    pub meta: CommitMeta,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct LogicalSnapshotEntry {
    namespace_tag: u8,
    key: Vec<u8>,
    value: Vec<u8>,
}

/// Errors specific to logical snapshot import. Export errors are
/// surfaced as `BackendError` directly because they all originate from
/// reading the backend.
#[derive(Debug)]
pub enum ImportError {
    Backend(BackendError),
    /// `latest_committed_meta` is `Some` on the target backend; import
    /// would clobber durable state. Use a fresh backend or wipe first.
    BackendNotEmpty,
    /// Magic bytes did not match `LOGICAL_SNAPSHOT_MAGIC`.
    BadMagic,
    /// Envelope version newer/older than this binary supports.
    UnsupportedEnvelopeVersion(u32),
    /// Format version newer than this binary understands.
    UnsupportedFormatVersion(u32),
    /// Snapshot fingerprint differs from the importing backend.
    SchemaFingerprintMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    /// Namespace tag in the stream does not map to a known namespace
    /// in this binary. A forward-compat snapshot from a newer chain.
    UnknownNamespaceTag(u8),
    /// Borsh decode or `std::io` error during read.
    Decode(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Backend(e) => write!(f, "import backend error: {e}"),
            ImportError::BackendNotEmpty => {
                write!(f, "import target backend already has committed state")
            }
            ImportError::BadMagic => write!(f, "import: bad magic prefix"),
            ImportError::UnsupportedEnvelopeVersion(v) => {
                write!(f, "import: unsupported logical snapshot envelope v{v}")
            }
            ImportError::UnsupportedFormatVersion(v) => {
                write!(f, "import: unsupported logical snapshot format v{v}")
            }
            ImportError::SchemaFingerprintMismatch { expected, actual } => write!(
                f,
                "import: persistence fingerprint mismatch expected={} actual={}",
                hex_bytes(expected),
                hex_bytes(actual)
            ),
            ImportError::UnknownNamespaceTag(t) => {
                write!(f, "import: unknown namespace tag {t}")
            }
            ImportError::Decode(d) => write!(f, "import: decode failure: {d}"),
        }
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

impl std::error::Error for ImportError {}

impl From<BackendError> for ImportError {
    fn from(e: BackendError) -> Self {
        ImportError::Backend(e)
    }
}

/// Errors specific to logical snapshot export.
#[derive(Debug)]
pub enum LogicalSnapshotError {
    Backend(BackendError),
    /// Backend has no committed `CommitMeta` to anchor the export.
    NoCommittedMeta,
    /// Borsh encode or `std::io` error during write.
    Encode(String),
}

impl std::fmt::Display for LogicalSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogicalSnapshotError::Backend(e) => {
                write!(f, "export backend error: {e}")
            }
            LogicalSnapshotError::NoCommittedMeta => {
                write!(f, "export: backend has no committed meta")
            }
            LogicalSnapshotError::Encode(d) => {
                write!(f, "export: encode failure: {d}")
            }
        }
    }
}

impl std::error::Error for LogicalSnapshotError {}

impl From<BackendError> for LogicalSnapshotError {
    fn from(e: BackendError) -> Self {
        LogicalSnapshotError::Backend(e)
    }
}

/// Summary returned by logical snapshot migration helpers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalSnapshotMigrationSummary {
    pub source_header: LogicalSnapshotHeader,
    pub target_header: LogicalSnapshotHeader,
    pub entry_count: u64,
}

/// Errors specific to logical snapshot migration. This intentionally
/// mirrors import's envelope validation taxonomy while adding transform
/// and encode failures.
#[derive(Debug)]
pub enum LogicalSnapshotMigrationError {
    BadMagic,
    UnsupportedEnvelopeVersion(u32),
    UnsupportedFormatVersion(u32),
    UnknownNamespaceTag(u8),
    Decode(String),
    Transform(String),
    Encode(String),
}

impl std::fmt::Display for LogicalSnapshotMigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogicalSnapshotMigrationError::BadMagic => {
                write!(f, "snapshot migration: bad magic prefix")
            }
            LogicalSnapshotMigrationError::UnsupportedEnvelopeVersion(version) => write!(
                f,
                "snapshot migration: unsupported logical snapshot envelope v{version}"
            ),
            LogicalSnapshotMigrationError::UnsupportedFormatVersion(version) => write!(
                f,
                "snapshot migration: unsupported logical snapshot format v{version}"
            ),
            LogicalSnapshotMigrationError::UnknownNamespaceTag(tag) => {
                write!(f, "snapshot migration: unknown namespace tag {tag}")
            }
            LogicalSnapshotMigrationError::Decode(error) => {
                write!(f, "snapshot migration: decode failure: {error}")
            }
            LogicalSnapshotMigrationError::Transform(error) => {
                write!(f, "snapshot migration: transform failure: {error}")
            }
            LogicalSnapshotMigrationError::Encode(error) => {
                write!(f, "snapshot migration: encode failure: {error}")
            }
        }
    }
}

impl std::error::Error for LogicalSnapshotMigrationError {}

/// Export every committed namespace plus the latest `CommitMeta` to
/// `writer`. Returns the meta that anchors this snapshot. The snapshot
/// reflects the backend state at the moment `read_txn()` is taken;
/// subsequent commits are not included.
///
/// Errors out cleanly if the backend has never been committed to.
pub fn export<B, W>(backend: &B, mut writer: W) -> Result<CommitMeta, LogicalSnapshotError>
where
    B: StateBackend,
    W: Write,
{
    let persistence_fingerprint = match backend.check_persistence_fingerprint()? {
        PersistenceFingerprintCheck::Compatible { fingerprint } => fingerprint,
        PersistenceFingerprintCheck::Empty => return Err(LogicalSnapshotError::NoCommittedMeta),
        other => {
            return Err(LogicalSnapshotError::Backend(
                other.into_result().unwrap_err(),
            ))
        }
    };
    let meta = backend
        .latest_committed_meta()?
        .ok_or(LogicalSnapshotError::NoCommittedMeta)?;
    let txn = backend.read_txn()?;

    let header = LogicalSnapshotHeader {
        magic: LOGICAL_SNAPSHOT_MAGIC,
        envelope_version: LOGICAL_SNAPSHOT_ENVELOPE_VERSION,
        persistence_fingerprint,
        format_version: LOGICAL_SNAPSHOT_FORMAT_V2,
        meta: meta.clone(),
    };
    let header_bytes =
        borsh::to_vec(&header).map_err(|e| LogicalSnapshotError::Encode(e.to_string()))?;
    writer
        .write_all(&header_bytes)
        .map_err(|e| LogicalSnapshotError::Encode(e.to_string()))?;

    // Walk every namespace via the same read txn so the export is a
    // single point-in-time view.
    let mut entries: Vec<LogicalSnapshotEntry> = Vec::new();
    for ns in Namespace::SNAPSHOT {
        let iter = txn.iter_prefix(*ns, b"")?;
        for kv in iter {
            let (key, value) = kv?;
            entries.push(LogicalSnapshotEntry {
                namespace_tag: ns.tag(),
                key,
                value,
            });
        }
    }

    let entries_bytes =
        borsh::to_vec(&entries).map_err(|e| LogicalSnapshotError::Encode(e.to_string()))?;
    writer
        .write_all(&entries_bytes)
        .map_err(|e| LogicalSnapshotError::Encode(e.to_string()))?;
    writer
        .flush()
        .map_err(|e| LogicalSnapshotError::Encode(e.to_string()))?;

    Ok(meta)
}

/// Convenience wrapper: export to a `Vec<u8>` and return the meta.
pub fn export_to_vec<B>(backend: &B) -> Result<(Vec<u8>, CommitMeta), LogicalSnapshotError>
where
    B: StateBackend,
{
    let mut buf = Vec::new();
    let meta = export(backend, &mut buf)?;
    Ok((buf, meta))
}

/// Decode and validate the RFC 055 logical snapshot header without
/// checking it against a backend fingerprint.
pub fn logical_snapshot_header_from_slice(
    bytes: &[u8],
) -> Result<LogicalSnapshotHeader, LogicalSnapshotMigrationError> {
    let mut cursor = bytes;
    let header = LogicalSnapshotHeader::deserialize_reader(&mut cursor)
        .map_err(|error| LogicalSnapshotMigrationError::Decode(format!("header: {error}")))?;
    validate_snapshot_header_for_migration(&header)?;
    Ok(header)
}

/// Migrate a logical snapshot stream into a new in-memory byte vector.
/// The caller supplies the canonical registry transform for each
/// record. This helper owns only the snapshot envelope and namespace
/// decoding contract.
pub fn migrate_logical_snapshot_to_vec<R, F>(
    mut reader: R,
    target_fingerprint: [u8; 32],
    transform: F,
) -> Result<(Vec<u8>, LogicalSnapshotMigrationSummary), LogicalSnapshotMigrationError>
where
    R: Read,
    F: FnMut(Namespace, &[u8], &[u8]) -> Result<Vec<u8>, String>,
{
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| LogicalSnapshotMigrationError::Decode(error.to_string()))?;
    migrate_logical_snapshot_slice_to_vec(&bytes, target_fingerprint, transform)
}

/// Slice-based variant used by tests and maintenance tools that have
/// already read the snapshot file.
pub fn migrate_logical_snapshot_slice_to_vec<F>(
    bytes: &[u8],
    target_fingerprint: [u8; 32],
    mut transform: F,
) -> Result<(Vec<u8>, LogicalSnapshotMigrationSummary), LogicalSnapshotMigrationError>
where
    F: FnMut(Namespace, &[u8], &[u8]) -> Result<Vec<u8>, String>,
{
    let mut cursor = bytes;
    let source_header = LogicalSnapshotHeader::deserialize_reader(&mut cursor)
        .map_err(|error| LogicalSnapshotMigrationError::Decode(format!("header: {error}")))?;
    validate_snapshot_header_for_migration(&source_header)?;

    let entries: Vec<LogicalSnapshotEntry> =
        Vec::<LogicalSnapshotEntry>::deserialize_reader(&mut cursor)
            .map_err(|error| LogicalSnapshotMigrationError::Decode(format!("entries: {error}")))?;

    let mut migrated_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let namespace = Namespace::from_tag(entry.namespace_tag).ok_or(
            LogicalSnapshotMigrationError::UnknownNamespaceTag(entry.namespace_tag),
        )?;
        let value = transform(namespace, &entry.key, &entry.value)
            .map_err(LogicalSnapshotMigrationError::Transform)?;
        migrated_entries.push(LogicalSnapshotEntry {
            namespace_tag: entry.namespace_tag,
            key: entry.key,
            value,
        });
    }

    let target_header = LogicalSnapshotHeader {
        persistence_fingerprint: target_fingerprint,
        ..source_header.clone()
    };
    let mut output = borsh::to_vec(&target_header)
        .map_err(|error| LogicalSnapshotMigrationError::Encode(format!("header: {error}")))?;
    output.extend_from_slice(
        &borsh::to_vec(&migrated_entries)
            .map_err(|error| LogicalSnapshotMigrationError::Encode(format!("entries: {error}")))?,
    );

    Ok((
        output,
        LogicalSnapshotMigrationSummary {
            source_header,
            target_header,
            entry_count: migrated_entries.len() as u64,
        },
    ))
}

fn validate_snapshot_header_for_migration(
    header: &LogicalSnapshotHeader,
) -> Result<(), LogicalSnapshotMigrationError> {
    if header.magic != LOGICAL_SNAPSHOT_MAGIC {
        return Err(LogicalSnapshotMigrationError::BadMagic);
    }
    if header.envelope_version != LOGICAL_SNAPSHOT_ENVELOPE_VERSION {
        return Err(LogicalSnapshotMigrationError::UnsupportedEnvelopeVersion(
            header.envelope_version,
        ));
    }
    if header.format_version != LOGICAL_SNAPSHOT_FORMAT_V2 {
        return Err(LogicalSnapshotMigrationError::UnsupportedFormatVersion(
            header.format_version,
        ));
    }
    Ok(())
}

/// Import a logical snapshot stream into a *fresh* backend. Refuses if
/// the target has any committed meta. The import is a single atomic
/// `commit_batch` carrying the meta from the snapshot header.
pub fn import<B, R>(backend: &B, mut reader: R) -> Result<CommitMeta, ImportError>
where
    B: StateBackend,
    R: Read,
{
    if backend.latest_committed_meta()?.is_some() {
        return Err(ImportError::BackendNotEmpty);
    }

    // Read the entire stream into memory; borsh's deserialiser wants
    // `&[u8]` for `Vec<T>`. Phase 9 can swap to chunked streaming.
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| ImportError::Decode(e.to_string()))?;
    import_from_slice(backend, &buf)
}

/// Import from an in-memory slice. Same semantics as `import`.
pub fn import_from_slice<B>(backend: &B, bytes: &[u8]) -> Result<CommitMeta, ImportError>
where
    B: StateBackend,
{
    if backend.latest_committed_meta()?.is_some() {
        return Err(ImportError::BackendNotEmpty);
    }

    let mut cursor = bytes;
    let header = LogicalSnapshotHeader::deserialize_reader(&mut cursor)
        .map_err(|e| ImportError::Decode(format!("header: {e}")))?;
    if header.magic != LOGICAL_SNAPSHOT_MAGIC {
        return Err(ImportError::BadMagic);
    }
    if header.envelope_version != LOGICAL_SNAPSHOT_ENVELOPE_VERSION {
        return Err(ImportError::UnsupportedEnvelopeVersion(
            header.envelope_version,
        ));
    }
    if header.format_version != LOGICAL_SNAPSHOT_FORMAT_V2 {
        return Err(ImportError::UnsupportedFormatVersion(header.format_version));
    }
    let expected = backend.expected_persistence_fingerprint();
    if header.persistence_fingerprint != expected {
        return Err(ImportError::SchemaFingerprintMismatch {
            expected,
            actual: header.persistence_fingerprint,
        });
    }

    let entries: Vec<LogicalSnapshotEntry> =
        Vec::<LogicalSnapshotEntry>::deserialize_reader(&mut cursor)
            .map_err(|e| ImportError::Decode(format!("entries: {e}")))?;

    let mut batch = backend.batch();
    for entry in entries {
        let ns = Namespace::from_tag(entry.namespace_tag)
            .ok_or(ImportError::UnknownNamespaceTag(entry.namespace_tag))?;
        // The `Meta` namespace is also written by `commit_batch` itself
        // (it stores `latest_committed_meta`). Re-applying the
        // exported entries here is fine: the backend will overwrite the
        // serialised meta with `header.meta` as part of the same atomic
        // commit, and any other meta keys are imported verbatim.
        batch.put(ns, &entry.key, &entry.value);
    }

    backend.commit_batch(batch, header.meta.clone(), SyncMode::Sync)?;
    backend
        .check_persistence_fingerprint()?
        .into_result()
        .map_err(ImportError::Backend)?;
    Ok(header.meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{conformance, InMemoryBackend, OpenOptions, RedbBackend};
    use tempfile::TempDir;

    /// Populate a backend with a deterministic mix of state across
    /// multiple namespaces. The corpus here is small but spans every
    /// namespace touched by Phase 0's inventory.
    fn populate_corpus<B: StateBackend>(backend: &B) -> CommitMeta {
        let mut batch = backend.batch();
        // State: account/policy-shaped bytes
        batch.put(Namespace::State, b"acc:00000001", b"acc-1-bytes");
        batch.put(Namespace::State, b"acc:00000002", b"acc-2-bytes");
        batch.put(Namespace::State, b"pol:00000001", b"pol-1-bytes");
        // Blocks
        batch.put(Namespace::Blocks, &10u64.to_be_bytes(), b"block-10");
        batch.put(Namespace::Blocks, &11u64.to_be_bytes(), b"block-11");
        batch.put(Namespace::BlockHashIndex, b"hash-10", &10u64.to_be_bytes());
        // Events / receipts
        batch.put(Namespace::Events, &100u64.to_be_bytes(), b"evt-100");
        batch.put(Namespace::Events, &101u64.to_be_bytes(), b"evt-101");
        batch.put(Namespace::Receipts, b"tx-hash-1", b"receipt-1");
        // Quarantine
        batch.put(Namespace::Quarantine, b"q-block-9", b"reason-corruption");
        // Consensus safety
        batch.put(Namespace::ConsensusSafety, b"validator-set", b"v-set-bytes");
        batch.put(Namespace::ConsensusSafety, b"pacemaker", b"pm-bytes");
        // WALs
        batch.put(Namespace::DirectMempoolWal, b"intent-A", b"signed-A");
        batch.put(Namespace::ConditionalMempoolWal, b"intent-B", b"signed-B");
        batch.put(Namespace::PendingFaucetWal, b"addr-x", b"grant-x");
        batch.put(Namespace::PendingDelegationWal, b"action-D1", b"deleg-1");
        batch.put(Namespace::PendingMmProgramWal, b"action-M1", b"mm-1");
        // Telemetry
        batch.put(
            Namespace::Telemetry,
            b"counter:rounds",
            &42u64.to_be_bytes(),
        );

        let meta = conformance::meta(50);
        backend
            .commit_batch(batch, meta.clone(), SyncMode::Sync)
            .expect("commit");
        meta
    }

    /// Verify that `dst` exposes byte-identical contents in every
    /// namespace as `src` did at the time `populate_corpus` ran.
    fn assert_corpus_matches<A, B>(src: &A, dst: &B)
    where
        A: StateBackend,
        B: StateBackend,
    {
        let s = src.read_txn().expect("src read_txn");
        let d = dst.read_txn().expect("dst read_txn");
        for ns in Namespace::ALL {
            let src_kvs: Vec<_> = s
                .iter_prefix(*ns, b"")
                .expect("src iter")
                .map(|r| r.expect("src kv"))
                .collect();
            let dst_kvs: Vec<_> = d
                .iter_prefix(*ns, b"")
                .expect("dst iter")
                .map(|r| r.expect("dst kv"))
                .collect();
            assert_eq!(src_kvs, dst_kvs, "namespace mismatch: {:?}", ns);
        }
    }

    #[test]
    fn backend_export_import_preserves_meta_and_state_root() {
        let src = InMemoryBackend::new();
        let expected_meta = populate_corpus(&src);

        let (bytes, exported_meta) = export_to_vec(&src).expect("export");
        assert_eq!(exported_meta, expected_meta);

        // Fresh in-memory backend ingests the export.
        let dst = InMemoryBackend::new();
        let imported_meta = import_from_slice(&dst, &bytes).expect("import");
        assert_eq!(imported_meta, expected_meta);
        assert_eq!(
            dst.latest_committed_meta().expect("meta").as_ref(),
            Some(&expected_meta)
        );

        assert_corpus_matches(&src, &dst);
    }

    #[test]
    fn export_excludes_node_local_history_tables() {
        let src = InMemoryBackend::new();
        let mut batch = src.batch();
        batch.put(Namespace::State, b"engine", b"canonical");
        batch.put(Namespace::EquityHistory, b"account-height", b"sample");
        batch.put(
            Namespace::MandateConformanceHistory,
            b"mandate-timestamp-height",
            b"sample",
        );
        let expected_meta = conformance::meta(50);
        src.commit_batch(batch, expected_meta.clone(), SyncMode::Sync)
            .expect("commit");

        let (bytes, exported_meta) = export_to_vec(&src).expect("export");
        assert_eq!(exported_meta, expected_meta);

        let dst = InMemoryBackend::new();
        import_from_slice(&dst, &bytes).expect("import");
        let src_txn = src.read_txn().expect("src txn");
        let dst_txn = dst.read_txn().expect("dst txn");
        assert_eq!(
            src_txn
                .get(Namespace::EquityHistory, b"account-height")
                .expect("src equity"),
            Some(b"sample".to_vec())
        );
        assert_eq!(
            dst_txn
                .get(Namespace::EquityHistory, b"account-height")
                .expect("dst equity"),
            None
        );
        assert_eq!(
            src_txn
                .get(
                    Namespace::MandateConformanceHistory,
                    b"mandate-timestamp-height"
                )
                .expect("src conformance"),
            Some(b"sample".to_vec())
        );
        assert_eq!(
            dst_txn
                .get(
                    Namespace::MandateConformanceHistory,
                    b"mandate-timestamp-height"
                )
                .expect("dst conformance"),
            None
        );
        assert_eq!(
            dst_txn.get(Namespace::State, b"engine").expect("dst state"),
            Some(b"canonical".to_vec())
        );
    }

    #[test]
    fn export_then_import_into_redb_round_trips() {
        // Cross-backend: export from in-memory, import into redb.
        let src = InMemoryBackend::new();
        let expected_meta = populate_corpus(&src);
        let (bytes, _) = export_to_vec(&src).expect("export");

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("imported.redb");
        let dst = RedbBackend::open(OpenOptions::new(path)).expect("redb open");

        let imported_meta = import_from_slice(&dst, &bytes).expect("import");
        assert_eq!(imported_meta, expected_meta);
        assert_corpus_matches(&src, &dst);
    }

    #[test]
    fn export_during_concurrent_writes_is_consistent() {
        // The export must reflect the read_txn snapshot, NOT subsequent
        // writes. Take a read_txn implicitly via export, then commit
        // more state, then import the export elsewhere and assert it
        // does not see the post-export commit.
        //
        // The InMemoryBackend's snapshot semantics are documented in
        // its tests; this exercises the same property through the
        // export path.
        let src = InMemoryBackend::new();
        let expected_meta = populate_corpus(&src);

        // Export first (captures point-in-time).
        let (bytes, exported_meta) = export_to_vec(&src).expect("export");
        assert_eq!(exported_meta, expected_meta);

        // Then commit a second wave of writes.
        let mut batch = src.batch();
        batch.put(Namespace::State, b"acc:00000003", b"acc-3-bytes-LATER");
        batch.delete(Namespace::State, b"acc:00000001");
        src.commit_batch(batch, conformance::meta(51), SyncMode::Sync)
            .expect("commit");

        // Import the export into a fresh backend.
        let dst = InMemoryBackend::new();
        let imported_meta = import_from_slice(&dst, &bytes).expect("import");
        assert_eq!(imported_meta, expected_meta);

        // The destination must reflect the pre-commit state, not the
        // post-commit state.
        let txn = dst.read_txn().expect("read_txn");
        assert_eq!(
            txn.get(Namespace::State, b"acc:00000001").expect("get"),
            Some(b"acc-1-bytes".to_vec()),
            "deleted-after-export key must still be in the export"
        );
        assert_eq!(
            txn.get(Namespace::State, b"acc:00000003").expect("get"),
            None,
            "added-after-export key must NOT be in the export"
        );
    }

    #[test]
    fn import_refuses_populated_backend() {
        let src = InMemoryBackend::new();
        populate_corpus(&src);
        let (bytes, _) = export_to_vec(&src).expect("export");

        // Target backend already has data.
        let dst = InMemoryBackend::new();
        populate_corpus(&dst);
        assert!(matches!(
            import_from_slice(&dst, &bytes),
            Err(ImportError::BackendNotEmpty)
        ));
    }

    #[test]
    fn import_rejects_bad_magic() {
        let dst = InMemoryBackend::new();
        let mut bytes = Vec::new();
        // Build a bogus header with wrong magic.
        let header = LogicalSnapshotHeader {
            magic: *b"NOPE",
            envelope_version: LOGICAL_SNAPSHOT_ENVELOPE_VERSION,
            persistence_fingerprint: dst.expected_persistence_fingerprint(),
            format_version: LOGICAL_SNAPSHOT_FORMAT_V2,
            meta: conformance::meta(1),
        };
        bytes.extend_from_slice(&borsh::to_vec(&header).expect("hdr"));
        // Empty entries vec.
        bytes.extend_from_slice(&borsh::to_vec(&Vec::<LogicalSnapshotEntry>::new()).expect("ents"));
        assert!(matches!(
            import_from_slice(&dst, &bytes),
            Err(ImportError::BadMagic)
        ));
    }

    #[test]
    fn import_rejects_unsupported_format_version() {
        let dst = InMemoryBackend::new();
        let mut bytes = Vec::new();
        let header = LogicalSnapshotHeader {
            magic: LOGICAL_SNAPSHOT_MAGIC,
            envelope_version: LOGICAL_SNAPSHOT_ENVELOPE_VERSION,
            persistence_fingerprint: dst.expected_persistence_fingerprint(),
            format_version: 999,
            meta: conformance::meta(1),
        };
        bytes.extend_from_slice(&borsh::to_vec(&header).expect("hdr"));
        bytes.extend_from_slice(&borsh::to_vec(&Vec::<LogicalSnapshotEntry>::new()).expect("ents"));
        assert!(matches!(
            import_from_slice(&dst, &bytes),
            Err(ImportError::UnsupportedFormatVersion(999))
        ));
    }

    #[test]
    fn import_rejects_unsupported_envelope_version() {
        let dst = InMemoryBackend::new();
        let mut bytes = Vec::new();
        let header = LogicalSnapshotHeader {
            magic: LOGICAL_SNAPSHOT_MAGIC,
            envelope_version: 999,
            persistence_fingerprint: dst.expected_persistence_fingerprint(),
            format_version: LOGICAL_SNAPSHOT_FORMAT_V2,
            meta: conformance::meta(1),
        };
        bytes.extend_from_slice(&borsh::to_vec(&header).expect("hdr"));
        bytes.extend_from_slice(&borsh::to_vec(&Vec::<LogicalSnapshotEntry>::new()).expect("ents"));
        assert!(matches!(
            import_from_slice(&dst, &bytes),
            Err(ImportError::UnsupportedEnvelopeVersion(999))
        ));
    }

    #[test]
    fn import_rejects_snapshot_fingerprint_mismatch() {
        let src_fingerprint = [0x11; 32];
        let dst_fingerprint = [0x22; 32];
        let src = InMemoryBackend::open(
            OpenOptions::new("ignored").with_persistence_fingerprint(src_fingerprint),
        )
        .expect("src open");
        populate_corpus(&src);
        let (bytes, _) = export_to_vec(&src).expect("export");

        let dst = InMemoryBackend::open(
            OpenOptions::new("ignored").with_persistence_fingerprint(dst_fingerprint),
        )
        .expect("dst open");
        assert!(matches!(
            import_from_slice(&dst, &bytes),
            Err(ImportError::SchemaFingerprintMismatch { expected, actual })
                if expected == dst_fingerprint && actual == src_fingerprint
        ));
    }

    #[test]
    fn export_rejects_backend_fingerprint_mismatch_before_labeling_snapshot() {
        let src_fingerprint = [0x11; 32];
        let dst_fingerprint = [0x22; 32];
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("state.redb");

        {
            let src = RedbBackend::open(
                OpenOptions::new(path.clone()).with_persistence_fingerprint(src_fingerprint),
            )
            .expect("src open");
            populate_corpus(&src);
        }

        let reopened =
            RedbBackend::open(OpenOptions::new(path).with_persistence_fingerprint(dst_fingerprint))
                .expect("reopen");
        let err = export_to_vec(&reopened).expect_err("export should reject mismatched marker");
        assert!(matches!(
            err,
            LogicalSnapshotError::Backend(BackendError::SchemaMismatch(ref reason))
                if reason.contains("schema_fingerprint_mismatch")
        ));
    }

    #[test]
    fn import_rejects_unknown_namespace_tag() {
        let dst = InMemoryBackend::new();
        let mut bytes = Vec::new();
        let header = LogicalSnapshotHeader {
            magic: LOGICAL_SNAPSHOT_MAGIC,
            envelope_version: LOGICAL_SNAPSHOT_ENVELOPE_VERSION,
            persistence_fingerprint: dst.expected_persistence_fingerprint(),
            format_version: LOGICAL_SNAPSHOT_FORMAT_V2,
            meta: conformance::meta(1),
        };
        bytes.extend_from_slice(&borsh::to_vec(&header).expect("hdr"));
        let entries = vec![LogicalSnapshotEntry {
            namespace_tag: 200, // not assigned
            key: b"k".to_vec(),
            value: b"v".to_vec(),
        }];
        bytes.extend_from_slice(&borsh::to_vec(&entries).expect("ents"));
        assert!(matches!(
            import_from_slice(&dst, &bytes),
            Err(ImportError::UnknownNamespaceTag(200))
        ));
    }

    #[test]
    fn export_fails_on_empty_backend() {
        let backend = InMemoryBackend::new();
        assert!(matches!(
            export_to_vec(&backend),
            Err(LogicalSnapshotError::NoCommittedMeta)
        ));
    }
}
