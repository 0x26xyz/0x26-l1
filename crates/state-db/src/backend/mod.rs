//! Durable state backend for RFC 042.
//!
//! This module pins the trait contract that the validator's local
//! durability layer must implement, and the namespace enum every caller
//! addresses. Concrete backends (`in_memory`, `redb`, ...) implement
//! this trait; their internal types must NOT escape this module.
//!
//! See `docs/v1/rfcs/042-state-persistence.md` and
//! `chain/crates/node/src/persistence.rs` for the design.

use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::Hash;
use sha3::{Digest, Keccak256};
use std::path::PathBuf;

pub mod conformance;
#[cfg(test)]
pub mod cross_backend_corpus;
pub mod in_memory;
pub mod logical_snapshot;
pub mod redb;

pub use in_memory::InMemoryBackend;
pub use logical_snapshot::{
    export, export_to_vec, import, import_from_slice, logical_snapshot_header_from_slice,
    migrate_logical_snapshot_slice_to_vec, migrate_logical_snapshot_to_vec, ImportError,
    LogicalSnapshotError, LogicalSnapshotHeader, LogicalSnapshotMigrationError,
    LogicalSnapshotMigrationSummary, LOGICAL_SNAPSHOT_ENVELOPE_VERSION, LOGICAL_SNAPSHOT_FORMAT_V2,
    LOGICAL_SNAPSHOT_MAGIC,
};
pub use redb::RedbBackend;

/// Logical durable namespace. Backends implement these as separate
/// tables / column families / keyspaces; the rest of the chain only
/// sees this enum.
///
/// The set is fixed by RFC 042. Adding a variant requires a schema
/// migration (`schema_version` bump in `CommitMeta`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Namespace {
    /// Executor committed state plus typed engine economic state
    /// (accounts, policies, mandates, order books, positions, markets,
    /// marks, delegations, MM program state, faucet accounting,
    /// collateral, funding, fees, insurance, treasury, frozen /
    /// maintenance state).
    State,
    /// Canonical block records keyed by height.
    Blocks,
    /// `block_hash -> height` lookup index.
    BlockHashIndex,
    /// Runtime event records keyed by event seq.
    Events,
    /// EVM transaction receipts keyed by transaction hash.
    Receipts,
    /// Quarantined block records.
    Quarantine,
    /// Consensus safety metadata (validator set / verifiers / signing
    /// keys, pacemaker state and config).
    ConsensusSafety,
    /// Direct intent mempool WAL: admitted-but-not-yet-committed direct
    /// intents.
    DirectMempoolWal,
    /// Conditional intent pool WAL: predicate-gated admitted intents.
    ConditionalMempoolWal,
    /// Pending faucet grants WAL.
    PendingFaucetWal,
    /// Pending delegation actions WAL.
    PendingDelegationWal,
    /// Pending market-maker program actions WAL.
    PendingMmProgramWal,
    /// Backend metadata: latest committed-block tuple, schema version,
    /// and reserved meta keys. Atomic with every block commit.
    Meta,
    /// Local observability counters (consensus telemetry, etc.).
    /// Persisted for RPC continuity but **not** state-rooted; never
    /// affects consensus, admission, custody, or replay.
    Telemetry,
    /// Node-local account equity history samples. Persisted for RPC
    /// continuity but **not** state-rooted, gossiped, or exported in
    /// logical snapshots.
    EquityHistory,
    /// Node-local mandate conformance history samples. Persisted for
    /// RPC continuity but **not** state-rooted, gossiped, or exported
    /// in logical snapshots.
    MandateConformanceHistory,
}

/// Reserved key inside `Namespace::Meta` used for `latest_committed_meta`.
/// Borsh-encoded `CommitMeta`.
pub const META_KEY_LATEST: &[u8] = b"latest_committed_meta";

/// Reserved key inside `Namespace::Meta` used for the RFC 055 backend
/// fingerprint marker. This is read before any typed committed state is
/// decoded by node hydration.
pub const PERSISTENCE_FINGERPRINT_KEY: &[u8] = b"persistence_fingerprint_v1";

pub const PERSISTENCE_ENVELOPE_MAGIC: [u8; 4] = *b"K26F";
pub const PERSISTENCE_ENVELOPE_VERSION: u32 = 1;

const PERSISTENCE_FINGERPRINT_DOMAIN: &[u8] = b"kombat:persistence-fingerprint:v2";

/// Fixed backend metadata record written atomically with every commit.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BackendFingerprintEnvelope {
    pub magic: [u8; 4],
    pub envelope_version: u32,
    pub persistence_fingerprint: [u8; 32],
}

impl BackendFingerprintEnvelope {
    pub fn current(persistence_fingerprint: [u8; 32]) -> Self {
        Self {
            magic: PERSISTENCE_ENVELOPE_MAGIC,
            envelope_version: PERSISTENCE_ENVELOPE_VERSION,
            persistence_fingerprint,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistenceFingerprintCheck {
    Empty,
    Compatible {
        fingerprint: [u8; 32],
    },
    MissingMarker,
    EnvelopeMagicInvalid {
        actual: [u8; 4],
    },
    EnvelopeVersionUnsupported {
        actual: u32,
    },
    EnvelopeDecodeInvalid {
        reason: String,
    },
    SchemaFingerprintMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
}

impl PersistenceFingerprintCheck {
    pub fn is_compatible(&self) -> bool {
        matches!(
            self,
            PersistenceFingerprintCheck::Empty | PersistenceFingerprintCheck::Compatible { .. }
        )
    }

    pub fn error_reason(&self) -> Option<String> {
        match self {
            PersistenceFingerprintCheck::Empty | PersistenceFingerprintCheck::Compatible { .. } => {
                None
            }
            PersistenceFingerprintCheck::MissingMarker => Some("envelope_missing".into()),
            PersistenceFingerprintCheck::EnvelopeMagicInvalid { actual } => {
                Some(format!("envelope_magic_invalid: 0x{}", hex_bytes(actual)))
            }
            PersistenceFingerprintCheck::EnvelopeVersionUnsupported { actual } => Some(format!(
                "envelope_version_unsupported: expected {}, actual {}",
                PERSISTENCE_ENVELOPE_VERSION, actual
            )),
            PersistenceFingerprintCheck::EnvelopeDecodeInvalid { reason } => {
                Some(format!("envelope_decode_invalid: {reason}"))
            }
            PersistenceFingerprintCheck::SchemaFingerprintMismatch { expected, actual } => {
                Some(format!(
                    "schema_fingerprint_mismatch: expected 0x{}, actual 0x{}",
                    hex_bytes(expected),
                    hex_bytes(actual)
                ))
            }
        }
    }

    pub fn into_result(self) -> Result<Self, BackendError> {
        if let Some(reason) = self.error_reason() {
            Err(BackendError::SchemaMismatch(reason))
        } else {
            Ok(self)
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

/// Initial RFC 055 fingerprint. This is a manifest fingerprint for the
/// durable backend envelope and namespace/key surface. Later slices will
/// extend the root with the full persisted BorshSchema graph.
pub fn current_persistence_fingerprint() -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(PERSISTENCE_FINGERPRINT_DOMAIN);
    hasher.update(PERSISTENCE_ENVELOPE_MAGIC);
    hasher.update(PERSISTENCE_ENVELOPE_VERSION.to_le_bytes());
    hasher.update(META_KEY_LATEST);
    hasher.update(PERSISTENCE_FINGERPRINT_KEY);
    hasher.update(LOGICAL_SNAPSHOT_MAGIC);
    hasher.update(LOGICAL_SNAPSHOT_ENVELOPE_VERSION.to_le_bytes());
    hasher.update(LOGICAL_SNAPSHOT_FORMAT_V2.to_le_bytes());
    for ns in Namespace::ALL {
        hasher.update([ns.tag()]);
        hasher.update(ns.stable_name().as_bytes());
        hasher.update([Namespace::SNAPSHOT.contains(ns) as u8]);
    }
    hasher.finalize().into()
}

impl Namespace {
    /// Every backend namespace, in stable enum-declaration order. New
    /// variants must be appended; never reorder.
    pub const ALL: &'static [Namespace] = &[
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

    /// Namespaces included in logical snapshots. Local derived tables
    /// that are persisted only for RPC continuity are intentionally
    /// excluded.
    pub const SNAPSHOT: &'static [Namespace] = &[
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
    ];

    /// Stable byte tag used by backends that key namespaces in a flat
    /// keyspace (e.g. the in-memory test backend). New variants must
    /// take a fresh tag and never reuse a retired one.
    pub(crate) fn tag(self) -> u8 {
        match self {
            Namespace::State => 0,
            Namespace::Blocks => 1,
            Namespace::BlockHashIndex => 2,
            Namespace::Events => 3,
            Namespace::Receipts => 4,
            Namespace::Quarantine => 5,
            Namespace::ConsensusSafety => 6,
            Namespace::DirectMempoolWal => 7,
            Namespace::ConditionalMempoolWal => 8,
            Namespace::PendingFaucetWal => 9,
            Namespace::PendingDelegationWal => 10,
            Namespace::PendingMmProgramWal => 11,
            Namespace::Meta => 12,
            Namespace::Telemetry => 13,
            Namespace::EquityHistory => 14,
            Namespace::MandateConformanceHistory => 15,
        }
    }

    /// Stable report/manifest name for this namespace. These names are
    /// part of the RFC 055/057 persisted-surface contract.
    pub fn stable_name(self) -> &'static str {
        match self {
            Namespace::State => "state",
            Namespace::Blocks => "blocks",
            Namespace::BlockHashIndex => "block_hash_index",
            Namespace::Events => "events",
            Namespace::Receipts => "receipts",
            Namespace::Quarantine => "quarantine",
            Namespace::ConsensusSafety => "consensus_safety",
            Namespace::DirectMempoolWal => "direct_mempool_wal",
            Namespace::ConditionalMempoolWal => "conditional_mempool_wal",
            Namespace::PendingFaucetWal => "pending_faucet_wal",
            Namespace::PendingDelegationWal => "pending_delegation_wal",
            Namespace::PendingMmProgramWal => "pending_mm_program_wal",
            Namespace::Meta => "meta",
            Namespace::Telemetry => "telemetry",
            Namespace::EquityHistory => "equity_history",
            Namespace::MandateConformanceHistory => "mandate_conformance_history",
        }
    }

    /// Inverse of `tag`. Returns `None` for unknown tags (forward-compat
    /// import: a future namespace tag in an older binary should fail
    /// loudly, not silently land in the wrong namespace).
    pub(crate) fn from_tag(tag: u8) -> Option<Namespace> {
        match tag {
            0 => Some(Namespace::State),
            1 => Some(Namespace::Blocks),
            2 => Some(Namespace::BlockHashIndex),
            3 => Some(Namespace::Events),
            4 => Some(Namespace::Receipts),
            5 => Some(Namespace::Quarantine),
            6 => Some(Namespace::ConsensusSafety),
            7 => Some(Namespace::DirectMempoolWal),
            8 => Some(Namespace::ConditionalMempoolWal),
            9 => Some(Namespace::PendingFaucetWal),
            10 => Some(Namespace::PendingDelegationWal),
            11 => Some(Namespace::PendingMmProgramWal),
            12 => Some(Namespace::Meta),
            13 => Some(Namespace::Telemetry),
            14 => Some(Namespace::EquityHistory),
            15 => Some(Namespace::MandateConformanceHistory),
            _ => None,
        }
    }
}

/// Durability mode for a single `commit_batch` call.
///
/// Block commit paths must always use `Sync`. `GroupCommit` and
/// `AsyncBatchedTestnet` are reserved for admission paths where the
/// operator has accepted the documented latency/durability tradeoff.
///
/// **Crash-durability matrix:**
///
/// | Mode | Visible after clean shutdown | Crash-durable at ack |
/// |------|:---:|:---:|
/// | `Sync` | yes | yes |
/// | `GroupCommit` | yes | **no** |
/// | `AsyncBatchedTestnet` | yes (eventually) | **no** |
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SyncMode {
    /// Commit returns only after the backend has fsync'd the
    /// transaction. On `redb`, that maps to `Durability::Immediate`
    /// (the only redb mode guaranteed persistent on return).
    /// Block commit MUST use this mode. Caller may rely on
    /// "ack means crash-durable."
    Sync,
    /// Queued durability: the commit is queued for fsync but the
    /// caller returns before the fsync completes. The transaction is
    /// guaranteed visible after a clean shutdown but **may be lost on
    /// process / host crash before the queued fsync flushes**.
    ///
    /// On `redb` this maps to `Durability::Eventual` — quoting the
    /// upstream docs: "queued and should become persistent after
    /// commit returns; only `Immediate` is guaranteed persistent on
    /// return."
    ///
    /// Phase 7 baseline: there is no real *group* coordination yet
    /// (multiple commits → one shared fsync). The mode-name reserves
    /// the slot for that future work; today it is just queued
    /// durability with no batching. `max_delay_ms` is informational
    /// for callers and currently unused on the backend side.
    ///
    /// Callers using `GroupCommit` MUST NOT claim "RPC ack means
    /// durable" to clients. Use `Sync` if the contract requires it.
    GroupCommit { max_delay_ms: u64 },
    /// Testnet-only: rely on OS writeback. The transaction may be
    /// lost on crash even after the call returns. Never used on
    /// mainnet. Same crash-durability tier as `GroupCommit`; differs
    /// in latency profile and is intended for high-throughput
    /// testnet scenarios.
    AsyncBatchedTestnet,
}

/// Tuple bound to every atomic commit. Written into `Namespace::Meta`
/// in the same transaction as the rest of the batch.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CommitMeta {
    /// On-disk schema version. Bumped on any incompatible change.
    pub schema_version: u32,
    /// Block height this commit advances to.
    pub height: u64,
    pub block_hash: Hash,
    /// Root committed in the chain-head block header. Native Mahi may
    /// advance this independently from executor state.
    pub chain_head_state_root: Hash,
    /// Root of executor/economic state after this commit.
    pub executor_state_root: Hash,
    /// Authoritative event seq cursor after this commit.
    pub event_seq: u64,
    pub timestamp_ms: u64,
}

/// Open-time configuration. Per-backend tuning lives in the concrete
/// impl's own option structs; this is the cross-backend surface.
#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub path: PathBuf,
    /// Create the backing store if it does not exist.
    pub create_if_missing: bool,
    /// If true, `commit_batch(_, _, Sync)` is required to fsync each
    /// commit. The backend may apply additional safety knobs (e.g.
    /// redb `quick_repair`); this flag is the cross-backend gate.
    pub fsync_on_sync_commit: bool,
    /// Create/open every backend namespace during open. Normal node
    /// startup uses this so future reads/writes have all tables ready.
    /// Maintenance probes disable it so checking an existing backend
    /// does not perform an otherwise unnecessary write transaction.
    pub initialize_tables: bool,
    /// Abort backend repair during open. Maintenance commands use this
    /// while inspecting source state so opening a backend cannot mutate
    /// it through recovery.
    pub abort_repair_on_open: bool,
    pub persistence_fingerprint: [u8; 32],
}

impl OpenOptions {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            create_if_missing: true,
            fsync_on_sync_commit: true,
            initialize_tables: true,
            abort_repair_on_open: false,
            persistence_fingerprint: current_persistence_fingerprint(),
        }
    }

    pub fn with_persistence_fingerprint(mut self, fingerprint: [u8; 32]) -> Self {
        self.persistence_fingerprint = fingerprint;
        self
    }

    pub fn without_table_initialization(mut self) -> Self {
        self.initialize_tables = false;
        self
    }

    pub fn abort_repair_on_open(mut self) -> Self {
        self.abort_repair_on_open = true;
        self
    }
}

/// Backend errors are kept coarse on purpose. Each variant carries a
/// human-readable detail string; backend-specific structured errors are
/// stringified before they leave the backend module.
#[derive(Debug)]
pub enum BackendError {
    /// I/O, allocator, or filesystem failure.
    Io(String),
    /// Schema/version mismatch detected during open or recovery.
    SchemaMismatch(String),
    /// Stored bytes failed to decode (Borsh / namespace key).
    Corrupt(String),
    /// Namespace/key shape was invalid for the backend.
    InvalidArgument(String),
    /// The backend is already open or otherwise locked.
    Locked(String),
    /// Used by the in-memory backend to surface poisoned-lock state in
    /// tests; production backends should not return this.
    Internal(String),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Io(detail) => write!(f, "backend io: {detail}"),
            BackendError::SchemaMismatch(detail) => {
                write!(f, "backend schema mismatch: {detail}")
            }
            BackendError::Corrupt(detail) => write!(f, "backend corrupt: {detail}"),
            BackendError::InvalidArgument(detail) => {
                write!(f, "backend invalid argument: {detail}")
            }
            BackendError::Locked(detail) => write!(f, "backend locked: {detail}"),
            BackendError::Internal(detail) => write!(f, "backend internal: {detail}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Type alias used everywhere a yielded key/value pair is returned
/// from a read transaction. Owned bytes by design — backend-specific
/// guard / pinned-slice types must not leak above this module.
pub type KvPair = (Vec<u8>, Vec<u8>);

/// Boxed iterator over `Result<KvPair, BackendError>` items. Each
/// concrete backend may use its own internal iterator; we erase it at
/// the trait boundary so callers never depend on backend lifetimes.
pub type KvIter<'txn> = Box<dyn Iterator<Item = Result<KvPair, BackendError>> + 'txn>;

/// Read transaction view. Backed by a consistent point-in-time
/// snapshot for the lifetime of the txn.
pub trait ReadTxn {
    fn get(&self, namespace: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, BackendError>;

    /// Iterate every (key, value) in `namespace` whose key starts with
    /// `prefix`, in ascending key order. The empty prefix iterates the
    /// entire namespace.
    fn iter_prefix<'a>(
        &'a self,
        namespace: Namespace,
        prefix: &[u8],
    ) -> Result<KvIter<'a>, BackendError>;
}

/// Staged write batch. Committed atomically by `StateBackend::commit_batch`.
///
/// `delete_prefix_bounded` is a maintenance helper only; production
/// block commits delete exact WAL keys derived from the block body or
/// action ids.
pub trait WriteBatch {
    fn put(&mut self, namespace: Namespace, key: &[u8], value: &[u8]);
    fn delete(&mut self, namespace: Namespace, key: &[u8]);
    fn delete_prefix_bounded(&mut self, namespace: Namespace, prefix: &[u8], max_keys: u32);
}

/// The local durable persistence contract.
pub trait StateBackend: Send + Sync + 'static {
    type ReadTxn<'a>: ReadTxn + 'a
    where
        Self: 'a;
    type WriteBatch: WriteBatch;

    fn open(opts: OpenOptions) -> Result<Self, BackendError>
    where
        Self: Sized;

    /// Snapshot read view consistent for the lifetime of the txn.
    fn read_txn(&self) -> Result<Self::ReadTxn<'_>, BackendError>;

    /// Empty staged batch.
    fn batch(&self) -> Self::WriteBatch;

    /// Atomically apply `batch`, write `meta` into `Namespace::Meta`,
    /// and durably commit per `sync`. On `Sync`, returns only after
    /// durability per the backend profile.
    fn commit_batch(
        &self,
        batch: Self::WriteBatch,
        meta: CommitMeta,
        sync: SyncMode,
    ) -> Result<(), BackendError>;

    /// Latest persisted `CommitMeta`, or `None` if no commit has ever
    /// been made.
    fn latest_committed_meta(&self) -> Result<Option<CommitMeta>, BackendError>;

    fn expected_persistence_fingerprint(&self) -> [u8; 32];

    fn check_persistence_fingerprint(&self) -> Result<PersistenceFingerprintCheck, BackendError> {
        let txn = self.read_txn()?;
        let expected = self.expected_persistence_fingerprint();
        let Some(marker_bytes) = txn.get(Namespace::Meta, PERSISTENCE_FINGERPRINT_KEY)? else {
            return if txn.get(Namespace::Meta, META_KEY_LATEST)?.is_some() {
                Ok(PersistenceFingerprintCheck::MissingMarker)
            } else {
                Ok(PersistenceFingerprintCheck::Empty)
            };
        };
        let envelope: BackendFingerprintEnvelope = match borsh::from_slice(&marker_bytes) {
            Ok(envelope) => envelope,
            Err(err) => {
                return Ok(PersistenceFingerprintCheck::EnvelopeDecodeInvalid {
                    reason: err.to_string(),
                });
            }
        };
        if envelope.magic != PERSISTENCE_ENVELOPE_MAGIC {
            return Ok(PersistenceFingerprintCheck::EnvelopeMagicInvalid {
                actual: envelope.magic,
            });
        }
        if envelope.envelope_version != PERSISTENCE_ENVELOPE_VERSION {
            return Ok(PersistenceFingerprintCheck::EnvelopeVersionUnsupported {
                actual: envelope.envelope_version,
            });
        }
        if envelope.persistence_fingerprint != expected {
            return Ok(PersistenceFingerprintCheck::SchemaFingerprintMismatch {
                expected,
                actual: envelope.persistence_fingerprint,
            });
        }
        Ok(PersistenceFingerprintCheck::Compatible {
            fingerprint: envelope.persistence_fingerprint,
        })
    }
}

/// Runtime-selected backend wrapper. Phase 1 has only the in-memory
/// variant; Phase 2 adds `Redb`. Callers in production code (engine,
/// node, RPC) hold this enum so backend selection is a single
/// `match`, never a generic-erasure tower.
///
/// `AnyStateBackend` does not itself implement `StateBackend` — the
/// associated GAT lifetimes don't compose across variants. Instead it
/// exposes the same operations as inherent methods that match-and-
/// delegate.
pub enum AnyStateBackend {
    InMemory(InMemoryBackend),
    Redb(RedbBackend),
}

impl std::fmt::Debug for AnyStateBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyStateBackend::InMemory(_) => f.write_str("AnyStateBackend::InMemory"),
            AnyStateBackend::Redb(_) => f.write_str("AnyStateBackend::Redb"),
        }
    }
}

/// Erased read transaction returned by `AnyStateBackend::read_txn`.
pub enum AnyReadTxn<'a> {
    InMemory(<InMemoryBackend as StateBackend>::ReadTxn<'a>),
    Redb(<RedbBackend as StateBackend>::ReadTxn<'a>),
}

/// Erased write batch returned by `AnyStateBackend::batch`.
pub enum AnyWriteBatch {
    InMemory(<InMemoryBackend as StateBackend>::WriteBatch),
    Redb(<RedbBackend as StateBackend>::WriteBatch),
}

impl AnyStateBackend {
    pub fn read_txn(&self) -> Result<AnyReadTxn<'_>, BackendError> {
        match self {
            AnyStateBackend::InMemory(b) => b.read_txn().map(AnyReadTxn::InMemory),
            AnyStateBackend::Redb(b) => b.read_txn().map(AnyReadTxn::Redb),
        }
    }

    pub fn batch(&self) -> AnyWriteBatch {
        match self {
            AnyStateBackend::InMemory(b) => AnyWriteBatch::InMemory(b.batch()),
            AnyStateBackend::Redb(b) => AnyWriteBatch::Redb(b.batch()),
        }
    }

    pub fn commit_batch(
        &self,
        batch: AnyWriteBatch,
        meta: CommitMeta,
        sync: SyncMode,
    ) -> Result<(), BackendError> {
        match (self, batch) {
            (AnyStateBackend::InMemory(b), AnyWriteBatch::InMemory(wb)) => {
                b.commit_batch(wb, meta, sync)
            }
            (AnyStateBackend::Redb(b), AnyWriteBatch::Redb(wb)) => b.commit_batch(wb, meta, sync),
            _ => Err(BackendError::InvalidArgument(
                "batch backend kind does not match storage backend".into(),
            )),
        }
    }

    pub fn latest_committed_meta(&self) -> Result<Option<CommitMeta>, BackendError> {
        match self {
            AnyStateBackend::InMemory(b) => b.latest_committed_meta(),
            AnyStateBackend::Redb(b) => b.latest_committed_meta(),
        }
    }

    pub fn expected_persistence_fingerprint(&self) -> [u8; 32] {
        match self {
            AnyStateBackend::InMemory(b) => b.expected_persistence_fingerprint(),
            AnyStateBackend::Redb(b) => b.expected_persistence_fingerprint(),
        }
    }

    pub fn check_persistence_fingerprint(
        &self,
    ) -> Result<PersistenceFingerprintCheck, BackendError> {
        match self {
            AnyStateBackend::InMemory(b) => b.check_persistence_fingerprint(),
            AnyStateBackend::Redb(b) => b.check_persistence_fingerprint(),
        }
    }
}

impl ReadTxn for AnyReadTxn<'_> {
    fn get(&self, namespace: Namespace, key: &[u8]) -> Result<Option<Vec<u8>>, BackendError> {
        match self {
            AnyReadTxn::InMemory(t) => t.get(namespace, key),
            AnyReadTxn::Redb(t) => t.get(namespace, key),
        }
    }

    fn iter_prefix<'a>(
        &'a self,
        namespace: Namespace,
        prefix: &[u8],
    ) -> Result<KvIter<'a>, BackendError> {
        match self {
            AnyReadTxn::InMemory(t) => t.iter_prefix(namespace, prefix),
            AnyReadTxn::Redb(t) => t.iter_prefix(namespace, prefix),
        }
    }
}

impl WriteBatch for AnyWriteBatch {
    fn put(&mut self, namespace: Namespace, key: &[u8], value: &[u8]) {
        match self {
            AnyWriteBatch::InMemory(b) => b.put(namespace, key, value),
            AnyWriteBatch::Redb(b) => b.put(namespace, key, value),
        }
    }

    fn delete(&mut self, namespace: Namespace, key: &[u8]) {
        match self {
            AnyWriteBatch::InMemory(b) => b.delete(namespace, key),
            AnyWriteBatch::Redb(b) => b.delete(namespace, key),
        }
    }

    fn delete_prefix_bounded(&mut self, namespace: Namespace, prefix: &[u8], max_keys: u32) {
        match self {
            AnyWriteBatch::InMemory(b) => b.delete_prefix_bounded(namespace, prefix, max_keys),
            AnyWriteBatch::Redb(b) => b.delete_prefix_bounded(namespace, prefix, max_keys),
        }
    }
}
