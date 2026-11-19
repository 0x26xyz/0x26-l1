use super::{
    build_engine, build_ingress_network_bootstrap_config, build_validator_network_bootstrap_config,
    check_persistence_fingerprint_output, collect_backend_inventory,
    fetch_remote_bootstrap_metadata, fetch_remote_snapshot_bytes, parse_auto_network_round_config,
    parse_auto_produce_config, parse_check_persistence_fingerprint_args, parse_env_bool,
    parse_format_only_args, parse_migrate_snapshot_args, parse_migrate_state_args,
    parse_peer_transport_keepalive_config, peer_transport_enabled_for_network,
    read_peer_transport_response, rebase_auto_network_round_timestamp,
    run_check_persistence_fingerprint_command, run_list_persistence_migrations_command,
    run_migrate_snapshot_command_with_registry, run_migrate_state_command,
    run_migrate_state_command_with_registry, validate_migrated_inventory, AutoNetworkRoundConfig,
    AutoProduceConfig, MaintenanceOutputFormat, MigrateStateInventoryReport,
    PeerTransportKeepaliveConfig, PeerTransportResponse, PeerTransportSessionOpenRequest,
    RemoteBootstrapConfig, RuntimeRole, PEER_TRANSPORT_MAGIC,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::SigningKey;
use kombat_chain_consensus::PacemakerConfig;
use kombat_chain_network::load_validator_set_from_path;
use kombat_chain_node::{
    migrations::{BackendMigrationStrategy, PersistenceMigration},
    SingleNodeConfig, SingleNodeEngine,
};
use kombat_chain_state_db::{
    AnyStateBackend, CommitMeta, Namespace, OpenOptions, ReadTxn, RedbBackend, StateBackend,
    WriteBatch,
};
use serde_json::json;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn single_node_config(chain_id: u64) -> SingleNodeConfig {
    SingleNodeConfig {
        chain_id,
        validator_id: [9; 32],
        consensus_signing_key: [7; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [0; 32],
        pacemaker: PacemakerConfig {
            base_timeout_ms: 300,
            max_timeout_ms: 20_000,
            max_exponent: 6,
            max_clock_skew_ms: 1_000,
        },
    }
}

fn temp_snapshot_path(label: &str) -> PathBuf {
    static NEXT_SNAPSHOT_ID: AtomicU64 = AtomicU64::new(0);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let sequence = NEXT_SNAPSHOT_ID.fetch_add(1, Ordering::Relaxed);
    env::temp_dir().join(format!(
        "kombat-main-{label}-{}-{unique}-{sequence}.bin",
        std::process::id(),
    ))
}

fn temp_validator_set_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    env::temp_dir().join(format!(
        "kombat-validator-set-{label}-{}-{unique}.toml",
        std::process::id()
    ))
}

fn temp_state_dir(label: &str) -> PathBuf {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic")
        .as_nanos();
    let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    env::temp_dir().join(format!(
        "kombat-state-{label}-{}-{unique}-{sequence}",
        std::process::id(),
    ))
}

fn commit_marker_only_state(path: &std::path::Path, fingerprint: [u8; 32]) {
    commit_marker_only_state_value(path, fingerprint, b"v");
}

fn commit_marker_only_state_value(
    path: &std::path::Path,
    fingerprint: [u8; 32],
    state_value: &[u8],
) {
    let backend =
        RedbBackend::open(OpenOptions::new(path).with_persistence_fingerprint(fingerprint))
            .expect("redb open");
    let mut batch = backend.batch();
    batch.put(Namespace::State, b"k", state_value);
    backend
        .commit_batch(
            batch,
            CommitMeta {
                schema_version: 1,
                height: 0,
                block_hash: [1; 32],
                chain_head_state_root: [2; 32],
                executor_state_root: [2; 32],
                event_seq: 0,
                timestamp_ms: 1_700_000_000_000,
            },
            kombat_chain_state_db::SyncMode::Sync,
        )
        .expect("commit");
}

fn commit_marker_only_state_without_table_initialization(
    path: &std::path::Path,
    fingerprint: [u8; 32],
) {
    let backend = RedbBackend::open(
        OpenOptions::new(path)
            .with_persistence_fingerprint(fingerprint)
            .without_table_initialization(),
    )
    .expect("redb open");
    let mut batch = backend.batch();
    batch.put(Namespace::State, b"k", b"v");
    backend
        .commit_batch(
            batch,
            CommitMeta {
                schema_version: 1,
                height: 0,
                block_hash: [1; 32],
                chain_head_state_root: [2; 32],
                executor_state_root: [2; 32],
                event_seq: 0,
                timestamp_ms: 1_700_000_000_000,
            },
            kombat_chain_state_db::SyncMode::Sync,
        )
        .expect("commit");
}

fn commit_hydratable_engine_state(path: &std::path::Path, fingerprint: [u8; 32]) {
    let mut engine = SingleNodeEngine::new(single_node_config(7));
    engine.produce_empty_block(1_500).expect("block");
    let snapshot_path = temp_snapshot_path("migration-source");
    engine
        .save_logical_snapshot_file(&snapshot_path, 1_600)
        .expect("save logical snapshot");
    let snapshot = fs::read(&snapshot_path).expect("read logical snapshot");
    let _ = fs::remove_file(&snapshot_path);
    let (rewritten, _summary) = kombat_chain_state_db::migrate_logical_snapshot_slice_to_vec(
        &snapshot,
        fingerprint,
        |_namespace, _key, value| Ok(value.to_vec()),
    )
    .expect("rewrite source fingerprint");
    let backend =
        RedbBackend::open(OpenOptions::new(path).with_persistence_fingerprint(fingerprint))
            .expect("redb open");
    kombat_chain_state_db::import_from_slice(&backend, &rewritten).expect("import source");
}

fn commit_captured_rfc056_engine_state(path: &std::path::Path, fingerprint: [u8; 32]) {
    let (meta, legacy_engine_value) = captured_rfc056_engine_canonical_entry();
    let backend =
        RedbBackend::open(OpenOptions::new(path).with_persistence_fingerprint(fingerprint))
            .expect("redb open");
    let mut batch = backend.batch();
    batch.put(
        Namespace::State,
        ENGINE_CANONICAL_KEY_BYTES,
        &legacy_engine_value,
    );
    backend
        .commit_batch(batch, meta, kombat_chain_state_db::SyncMode::Sync)
        .expect("commit captured RFC056 source");
}

fn test_migration_edge(
    id: &'static str,
    source_fingerprint: [u8; 32],
    backend_strategy: BackendMigrationStrategy,
) -> PersistenceMigration {
    PersistenceMigration {
        id,
        source_fingerprint,
        target_fingerprint: kombat_chain_node::migrations::current_binary_persistence_fingerprint(),
        description: "test migration",
        changed_records: &[],
        backend_strategy,
    }
}

fn migration_inventory(path: &std::path::Path) -> MigrateStateInventoryReport {
    let backend = RedbBackend::open(OpenOptions::new(path)).expect("open inventory backend");
    collect_backend_inventory(&backend).expect("inventory")
}

const RFC056_SOURCE_SNAPSHOT_BAB6253: &[u8] = include_bytes!(
        "../../node/src/migrations/legacy/rfc056_policy_authority_scope/fixtures/source_snapshot_bab6253.k26s"
    );
const LEGACY_LOGICAL_SNAPSHOT_MAGIC_V1: [u8; 8] = *b"K26S\x01\x00\x00\x00";
const LEGACY_LOGICAL_SNAPSHOT_FORMAT_V1: u32 = 1;
const LEGACY_STATE_NAMESPACE_TAG: u8 = 0;
const ENGINE_CANONICAL_KEY_BYTES: &[u8] = b"engine_canonical_v1";

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
struct TestLogicalSnapshotEntry {
    namespace_tag: u8,
    key: Vec<u8>,
    value: Vec<u8>,
}

#[derive(Clone, Debug, BorshDeserialize)]
struct LegacyLogicalSnapshotHeaderV1 {
    magic: [u8; 8],
    format_version: u32,
    meta: LegacyCommitMetaV1,
}

#[derive(Clone, Debug, BorshDeserialize)]
struct LegacyCommitMetaV1 {
    schema_version: u32,
    height: u64,
    block_hash: [u8; 32],
    state_root: [u8; 32],
    event_seq: u64,
    timestamp_ms: u64,
}

fn captured_rfc056_engine_canonical_entry() -> (CommitMeta, Vec<u8>) {
    let mut cursor = RFC056_SOURCE_SNAPSHOT_BAB6253;
    let header = LegacyLogicalSnapshotHeaderV1::deserialize_reader(&mut cursor)
        .expect("legacy snapshot header decode");
    assert_eq!(header.magic, LEGACY_LOGICAL_SNAPSHOT_MAGIC_V1);
    assert_eq!(header.format_version, LEGACY_LOGICAL_SNAPSHOT_FORMAT_V1);
    let entries: Vec<TestLogicalSnapshotEntry> =
        Vec::<TestLogicalSnapshotEntry>::deserialize_reader(&mut cursor)
            .expect("legacy snapshot entries decode");
    assert!(
        cursor.is_empty(),
        "legacy fixture has trailing bytes after entries"
    );
    let engine_entry = entries
        .into_iter()
        .find(|entry| {
            entry.namespace_tag == LEGACY_STATE_NAMESPACE_TAG
                && entry.key == ENGINE_CANONICAL_KEY_BYTES
        })
        .expect("legacy engine canonical entry");
    (
        CommitMeta {
            schema_version: header.meta.schema_version,
            height: header.meta.height,
            block_hash: header.meta.block_hash,
            chain_head_state_root: header.meta.state_root,
            executor_state_root: header.meta.state_root,
            event_seq: header.meta.event_seq,
            timestamp_ms: header.meta.timestamp_ms,
        },
        engine_entry.value,
    )
}

fn current_enveloped_captured_rfc056_snapshot(source_fingerprint: [u8; 32]) -> Vec<u8> {
    let (meta, legacy_engine_value) = captured_rfc056_engine_canonical_entry();
    let header = kombat_chain_state_db::LogicalSnapshotHeader {
        magic: kombat_chain_state_db::LOGICAL_SNAPSHOT_MAGIC,
        envelope_version: kombat_chain_state_db::LOGICAL_SNAPSHOT_ENVELOPE_VERSION,
        persistence_fingerprint: source_fingerprint,
        format_version: kombat_chain_state_db::LOGICAL_SNAPSHOT_FORMAT_V2,
        meta,
    };
    let entries = vec![TestLogicalSnapshotEntry {
        namespace_tag: LEGACY_STATE_NAMESPACE_TAG,
        key: ENGINE_CANONICAL_KEY_BYTES.to_vec(),
        value: legacy_engine_value,
    }];
    let mut bytes = borsh::to_vec(&header).expect("snapshot header encode");
    bytes.extend_from_slice(&borsh::to_vec(&entries).expect("snapshot entries encode"));
    bytes
}

#[test]
fn check_persistence_fingerprint_args_parse() {
    let args = vec![
        "--data-dir".to_string(),
        "/var/lib/and-chain".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let parsed = parse_check_persistence_fingerprint_args(&args).expect("parse");
    assert_eq!(parsed.data_dir, PathBuf::from("/var/lib/and-chain"));
    assert_eq!(parsed.format, MaintenanceOutputFormat::Json);
}

#[test]
fn check_persistence_fingerprint_reports_no_state() {
    let dir = temp_state_dir("no-state");
    fs::create_dir_all(&dir).expect("mkdir");
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_check_persistence_fingerprint_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 2);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "no_state");
    assert!(parsed["binary_fingerprint"]
        .as_str()
        .expect("fingerprint")
        .starts_with("0x"));
}

#[test]
fn check_persistence_fingerprint_reports_compatible() {
    let dir = temp_state_dir("compatible");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(
        &path,
        kombat_chain_state_db::current_persistence_fingerprint(),
    );
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_check_persistence_fingerprint_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "compatible");
    assert_eq!(parsed["binary_fingerprint"], parsed["on_disk_fingerprint"]);
    assert_eq!(
        parsed["envelope_version"],
        kombat_chain_state_db::PERSISTENCE_ENVELOPE_VERSION
    );
}

#[test]
fn check_persistence_fingerprint_reports_mismatch() {
    let dir = temp_state_dir("mismatch");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(&path, [0x11; 32]);
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_check_persistence_fingerprint_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 1);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "mismatch");
    assert_eq!(
        parsed["on_disk_fingerprint"],
        format!("0x{}", "11".repeat(32))
    );
}

#[test]
fn check_persistence_fingerprint_reports_source_locked_when_redb_is_already_open() {
    let dir = temp_state_dir("check-source-locked");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(
        &path,
        kombat_chain_state_db::current_persistence_fingerprint(),
    );
    let _held = RedbBackend::open(OpenOptions::new(&path)).expect("hold source open");
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_check_persistence_fingerprint_command(&args);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");

    assert_eq!(exit_code, 13);
    assert_eq!(parsed["result"], "source_locked");
    assert!(parsed["error"].as_str().expect("error").contains("redb"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_persistence_fingerprint_reports_corrupt_marker_as_envelope_invalid() {
    let output = check_persistence_fingerprint_output(
        [0x33; 32],
        kombat_chain_state_db::PersistenceFingerprintCheck::EnvelopeDecodeInvalid {
            reason: "short read".into(),
        },
    );

    assert_eq!(output.result, "envelope_invalid");
    assert_eq!(output.binary_fingerprint, format!("0x{}", "33".repeat(32)));
    assert_eq!(output.on_disk_fingerprint, None);
    assert_eq!(output.envelope_version, None);
    assert_eq!(
        output.error.as_deref(),
        Some("envelope_decode_invalid: short read")
    );
}

#[test]
fn list_persistence_migrations_args_parse() {
    let args = vec!["--format".to_string(), "json".to_string()];
    let parsed = parse_format_only_args(&args).expect("parse");
    assert_eq!(parsed, MaintenanceOutputFormat::Json);
}

#[test]
fn list_persistence_migrations_reports_compiled_registry() {
    let args = vec!["--format".to_string(), "json".to_string()];
    let (exit_code, output) = run_list_persistence_migrations_command(&args);

    assert_eq!(exit_code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["schema"], "kombat.persistence_migration_registry.v1");
    assert_eq!(parsed["registry_valid"], true);
    assert_eq!(
        parsed["migration_count"],
        kombat_chain_node::migrations::compiled_persistence_migrations().len()
    );
    assert!(parsed["binary_fingerprint"]
        .as_str()
        .expect("fingerprint")
        .starts_with("0x"));
}

#[test]
fn migrate_state_args_parse() {
    let args = vec![
        "--data-dir".to_string(),
        "/var/lib/and-chain".to_string(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let parsed = parse_migrate_state_args(&args).expect("parse");
    assert_eq!(parsed.data_dir, PathBuf::from("/var/lib/and-chain"));
    assert_eq!(parsed.format, MaintenanceOutputFormat::Json);
    assert!(parsed.dry_run);
}

#[test]
fn migrate_snapshot_args_parse() {
    let args = vec![
        "--input".to_string(),
        "old.k26".to_string(),
        "--output".to_string(),
        "new.k26".to_string(),
        "--to".to_string(),
        "current".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--report".to_string(),
        "report.json".to_string(),
    ];
    let parsed = parse_migrate_snapshot_args(&args).expect("parse");
    assert_eq!(parsed.input_path, PathBuf::from("old.k26"));
    assert_eq!(parsed.output_path, PathBuf::from("new.k26"));
    assert_eq!(parsed.format, MaintenanceOutputFormat::Json);
    assert_eq!(parsed.report_path, Some(PathBuf::from("report.json")));
}

#[test]
fn migrate_state_dry_run_reports_no_state() {
    let dir = temp_state_dir("migrate-no-state");
    fs::create_dir_all(&dir).expect("mkdir");
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["schema"], "kombat.persistence_migration_report.v1");
    assert_eq!(parsed["result"], "no_state");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["source"], serde_json::Value::Null);
}

#[test]
fn migrate_state_dry_run_refuses_mismatch_without_path() {
    let dir = temp_state_dir("migrate-no-path");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(&path, [0x22; 32]);
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 10);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "no_migration_path");
    assert_eq!(
        parsed["source_fingerprint"],
        format!("0x{}", "22".repeat(32))
    );
    assert_eq!(parsed["migration_path"], json!([]));
    assert_eq!(parsed["source"]["total_records"], 3);
    assert_eq!(parsed["source"]["namespaces"]["state"]["source_records"], 1);
    assert_eq!(parsed["source"]["namespaces"]["meta"]["source_records"], 2);
    assert!(parsed["source"]["namespaces"]["state"]["source_key_hash"]
        .as_str()
        .expect("state key hash")
        .starts_with("0x"));
}

#[test]
fn migrate_state_dry_run_writes_report_file() {
    let dir = temp_state_dir("migrate-report");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    let report_path = dir.join("migration-report.json");
    commit_marker_only_state(&path, [0x44; 32]);
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--report".to_string(),
        report_path.to_string_lossy().into_owned(),
    ];
    let (exit_code, output) = run_migrate_state_command(&args);

    assert_eq!(exit_code, 10);
    let stdout_json: serde_json::Value = serde_json::from_str(&output).expect("stdout json");
    let report_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("report file"))
            .expect("report json");
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(stdout_json, report_json);
    assert_eq!(
        report_json["schema"],
        "kombat.persistence_migration_report.v1"
    );
    assert_eq!(report_json["result"], "no_migration_path");
    assert_eq!(
        report_json["source"]["namespaces"]["state"]["source_records"],
        1
    );
}

#[test]
fn migrate_state_dry_run_treats_missing_source_tables_as_empty() {
    let dir = temp_state_dir("migrate-missing-tables");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state_without_table_initialization(&path, [0x55; 32]);
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 10);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "no_migration_path");
    assert_eq!(parsed["source"]["total_records"], 3);
    assert_eq!(
        parsed["source"]["namespaces"]["receipts"]["source_records"],
        0
    );
    assert_eq!(
        parsed["source"]["namespaces"]["pending_mm_program_wal"]["source_records"],
        0
    );
}

#[test]
fn migrate_state_reports_source_locked_when_redb_is_already_open() {
    let dir = temp_state_dir("migrate-source-locked");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_hydratable_engine_state(&path, [0x56; 32]);
    let _held = RedbBackend::open(OpenOptions::new(&path)).expect("hold source open");
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");

    assert_eq!(exit_code, 13);
    assert_eq!(parsed["result"], "source_locked");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_compatible_dry_run_hydrates_source_backend() {
    let dir = temp_state_dir("migrate-compatible-hydrates");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_hydratable_engine_state(
        &path,
        kombat_chain_state_db::current_persistence_fingerprint(),
    );
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");

    assert_eq!(exit_code, 0, "migration output: {parsed}");
    assert_eq!(parsed["result"], "already_compatible");
    assert_eq!(parsed["validations"]["hydrated"], true);
    assert_eq!(parsed["validations"]["snapshot_validators"], true);
    assert!(parsed["source"]["total_records"].as_u64().expect("records") > 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_compatible_dry_run_rejects_unhydrated_backend() {
    let dir = temp_state_dir("migrate-compatible-hydration-fails");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(
        &path,
        kombat_chain_state_db::current_persistence_fingerprint(),
    );
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");

    assert_eq!(exit_code, 12);
    assert_eq!(parsed["result"], "compatible_validation_failed");
    assert!(
        parsed["error"]
            .as_str()
            .expect("error")
            .contains("hydrate compatible backend"),
        "unexpected output: {parsed}"
    );
    assert_eq!(parsed["validations"]["hydrated"], false);
    assert!(path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_dry_run_builds_and_removes_validated_staged_backend() {
    let source_fingerprint = [0x66; 32];
    let dir = temp_state_dir("migrate-staged-dry-run");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_hydratable_engine_state(&path, source_fingerprint);
    let registry = [test_migration_edge(
        "test-copy-all",
        source_fingerprint,
        BackendMigrationStrategy::CopyAllNamespaces,
    )];
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &registry);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    let staged_path = PathBuf::from(parsed["staged_path"].as_str().expect("staged path"));

    assert_eq!(exit_code, 0);
    assert_eq!(parsed["result"], "migration_dry_run_validated");
    assert_eq!(parsed["migration_path"], json!(["test-copy-all"]));
    assert_eq!(
        parsed["source"]["total_records"],
        parsed["target"]["total_records"]
    );
    assert_eq!(
        parsed["source"]["namespaces"]["state"]["source_value_hash"],
        parsed["target"]["namespaces"]["state"]["source_value_hash"]
    );
    assert!(parsed["binary_git_sha"].is_string());
    assert_eq!(parsed["validations"]["hydrated"], true);
    assert_eq!(parsed["validations"]["runtime_event_sequence"], true);
    assert_eq!(parsed["namespaces"]["state"]["classification"], "copied");
    assert_eq!(parsed["namespaces"]["meta"]["classification"], "rebuilt");
    assert!(parsed["records"]["copied"].as_u64().expect("copied") > 0);
    assert!(!staged_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_dry_run_migrates_captured_rfc056_engine_record() {
    let source_fingerprint = [0xac; 32];
    let dir = temp_state_dir("migrate-captured-rfc056");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_captured_rfc056_engine_state(&path, source_fingerprint);
    let registry = [test_migration_edge(
        "test-rfc056-captured-backend",
        source_fingerprint,
        BackendMigrationStrategy::Rfc056PolicyAuthorityScope,
    )];
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &registry);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    let staged_path = PathBuf::from(parsed["staged_path"].as_str().expect("staged path"));

    assert_eq!(exit_code, 0, "migration output: {parsed}");
    assert_eq!(parsed["result"], "migration_dry_run_validated");
    assert_eq!(
        parsed["migration_path"],
        json!(["test-rfc056-captured-backend"])
    );
    assert_eq!(parsed["validations"]["hydrated"], true);
    assert_eq!(parsed["validations"]["snapshot_validators"], true);
    assert_eq!(parsed["namespaces"]["state"]["classification"], "mixed");
    assert!(parsed["namespaces"]["state"]["copied_value_hash"].is_null());
    assert_eq!(parsed["records"]["migrated"], 1);
    assert_ne!(
        parsed["source"]["namespaces"]["state"]["source_value_hash"],
        parsed["target"]["namespaces"]["state"]["source_value_hash"]
    );
    assert!(!staged_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_rejects_staged_backend_that_cannot_hydrate() {
    let source_fingerprint = [0x67; 32];
    let dir = temp_state_dir("migrate-staged-hydration-fails");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(&path, source_fingerprint);
    let registry = [test_migration_edge(
        "test-copy-all-invalid-engine",
        source_fingerprint,
        BackendMigrationStrategy::CopyAllNamespaces,
    )];
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &registry);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    let staged_path = PathBuf::from(parsed["staged_path"].as_str().expect("staged path"));

    assert_eq!(exit_code, 12);
    assert_eq!(parsed["result"], "migration_validation_failed");
    assert!(
        parsed["error"]
            .as_str()
            .expect("error")
            .contains("hydrate staged backend"),
        "unexpected output: {parsed}"
    );
    assert!(!staged_path.exists());
    assert!(path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_real_run_hydration_failure_aborts_before_swap_and_cleans_staged() {
    let source_fingerprint = [0x68; 32];
    let dir = temp_state_dir("migrate-real-hydration-fails");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(&path, source_fingerprint);
    let registry = [test_migration_edge(
        "test-copy-all-invalid-engine-real",
        source_fingerprint,
        BackendMigrationStrategy::CopyAllNamespaces,
    )];
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--confirm".to_string(),
        "migrate-state".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &registry);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    let backup_path = PathBuf::from(parsed["backup_path"].as_str().expect("backup path"));
    let staged_path = PathBuf::from(parsed["staged_path"].as_str().expect("staged path"));

    assert_eq!(exit_code, 12);
    assert_eq!(parsed["result"], "migration_validation_failed");
    assert!(
        parsed["error"]
            .as_str()
            .expect("error")
            .contains("hydrate staged backend"),
        "unexpected output: {parsed}"
    );
    assert!(path.exists());
    assert!(!backup_path.exists());
    assert!(!staged_path.exists());

    let mut source_opts = OpenOptions::new(&path);
    source_opts.create_if_missing = false;
    let source = RedbBackend::open(source_opts).expect("open source");
    assert!(matches!(
        source.check_persistence_fingerprint().expect("check source"),
        kombat_chain_state_db::PersistenceFingerprintCheck::SchemaFingerprintMismatch {
            actual,
            ..
        } if actual == source_fingerprint
    ));
    let source_txn = source.read_txn().expect("source read txn");
    assert_eq!(
        source_txn
            .get(Namespace::State, b"k")
            .expect("read source marker"),
        Some(b"v".to_vec())
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_validation_allows_declared_migrated_namespace_value_changes() {
    let dir = temp_state_dir("migrate-validation-classes");
    fs::create_dir_all(&dir).expect("mkdir");
    let source_path = dir.join("source.redb");
    let target_path = dir.join("target.redb");
    commit_marker_only_state_value(&source_path, [0x99; 32], b"v");
    commit_marker_only_state_value(&target_path, [0xaa; 32], b"w");

    let source = migration_inventory(&source_path);
    let target = migration_inventory(&target_path);
    let copy_edge = test_migration_edge(
        "test-copy-validation",
        [0x99; 32],
        BackendMigrationStrategy::CopyAllNamespaces,
    );
    let migrated_edge = test_migration_edge(
        "test-rfc056-validation",
        [0x99; 32],
        BackendMigrationStrategy::Rfc056PolicyAuthorityScope,
    );

    let copy_error =
        validate_migrated_inventory(&source, &target, &[&copy_edge]).expect_err("copy mismatch");
    assert!(
        copy_error.contains("namespace state value hash mismatch"),
        "unexpected copy validation error: {copy_error}"
    );
    validate_migrated_inventory(&source, &target, &[&migrated_edge])
        .expect("declared migrated namespace may change values");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_snapshot_writes_target_fingerprint_and_validates_import() {
    let source_fingerprint = [0xab; 32];
    let dir = temp_state_dir("migrate-snapshot-rfc056");
    fs::create_dir_all(&dir).expect("mkdir");
    let input_path = dir.join("old.k26");
    let output_path = dir.join("new.k26");
    fs::write(
        &input_path,
        current_enveloped_captured_rfc056_snapshot(source_fingerprint),
    )
    .expect("write source snapshot");
    let registry = [test_migration_edge(
        "test-rfc056-snapshot",
        source_fingerprint,
        BackendMigrationStrategy::Rfc056PolicyAuthorityScope,
    )];
    let args = vec![
        "--input".to_string(),
        input_path.to_string_lossy().into_owned(),
        "--output".to_string(),
        output_path.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_snapshot_command_with_registry(&args, &registry);

    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(exit_code, 0, "migration output: {parsed}");
    assert_eq!(parsed["result"], "snapshot_migration_completed");
    assert_eq!(parsed["migration_path"], json!(["test-rfc056-snapshot"]));
    let migrated_bytes = fs::read(&output_path).expect("read migrated snapshot");
    let migrated =
        SingleNodeEngine::from_snapshot_bytes(&migrated_bytes).expect("migrated snapshot load");
    let migrated_policy = migrated.policy_store().get(42).expect("policy");
    assert_eq!(migrated_policy.authority_account, 11);
    assert_eq!(
        migrated_policy.execution_scope,
        kombat_chain_policy::PolicyExecutionScope::Account(11)
    );

    let header =
        kombat_chain_state_db::logical_snapshot_header_from_slice(&migrated_bytes).expect("hdr");
    assert_eq!(
        header.persistence_fingerprint,
        kombat_chain_node::migrations::current_binary_persistence_fingerprint()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_snapshot_rejects_untagged_legacy_snapshots() {
    let dir = temp_state_dir("migrate-snapshot-untagged");
    fs::create_dir_all(&dir).expect("mkdir");
    let input_path = dir.join("legacy.bin");
    let output_path = dir.join("new.k26");
    fs::write(&input_path, RFC056_SOURCE_SNAPSHOT_BAB6253).expect("write legacy snapshot");
    let args = vec![
        "--input".to_string(),
        input_path.to_string_lossy().into_owned(),
        "--output".to_string(),
        output_path.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_snapshot_command_with_registry(&args, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");

    assert_eq!(exit_code, 10);
    assert_eq!(parsed["result"], "envelope_invalid");
    assert!(!output_path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migrate_state_real_run_swaps_staged_backend_and_retains_backup() {
    let source_fingerprint = [0x77; 32];
    let dir = temp_state_dir("migrate-real-run");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_hydratable_engine_state(&path, source_fingerprint);
    let registry = [test_migration_edge(
        "test-copy-all-real",
        source_fingerprint,
        BackendMigrationStrategy::CopyAllNamespaces,
    )];
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--confirm".to_string(),
        "migrate-state".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &registry);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    let backup_path = PathBuf::from(parsed["backup_path"].as_str().expect("backup path"));

    assert_eq!(exit_code, 0);
    assert_eq!(parsed["result"], "migration_completed");
    assert!(backup_path.exists());

    let migrated = RedbBackend::open(OpenOptions::new(&path)).expect("open migrated");
    assert!(migrated
        .check_persistence_fingerprint()
        .expect("check migrated")
        .is_compatible());
    let migrated_engine =
        SingleNodeEngine::from_state_backend(&AnyStateBackend::Redb(migrated.clone()))
            .expect("hydrate migrated");
    assert_eq!(migrated_engine.head().header.height, 1);

    let mut backup_opts = OpenOptions::new(&backup_path);
    backup_opts.create_if_missing = false;
    let backup = RedbBackend::open(backup_opts).expect("open backup");
    assert!(matches!(
        backup.check_persistence_fingerprint().expect("check backup"),
        kombat_chain_state_db::PersistenceFingerprintCheck::SchemaFingerprintMismatch {
            actual,
            ..
        } if actual == source_fingerprint
    ));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_file(&backup_path);
}

#[test]
fn migrate_state_refuses_unimplemented_backend_edge() {
    let source_fingerprint = [0x88; 32];
    let dir = temp_state_dir("migrate-unimplemented");
    fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.redb");
    commit_marker_only_state(&path, source_fingerprint);
    let registry = [test_migration_edge(
        "test-not-implemented",
        source_fingerprint,
        BackendMigrationStrategy::NotImplemented,
    )];
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--dry-run".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command_with_registry(&args, &registry);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 12);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "migration_implementation_missing");
}

#[test]
fn migrate_state_real_run_requires_confirmation() {
    let dir = temp_state_dir("migrate-confirm");
    fs::create_dir_all(&dir).expect("mkdir");
    let args = vec![
        "--data-dir".to_string(),
        dir.to_string_lossy().into_owned(),
        "--to".to_string(),
        "current".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ];
    let (exit_code, output) = run_migrate_state_command(&args);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(exit_code, 14);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert_eq!(parsed["result"], "confirmation_missing");
}

fn validator_set_toml(config: &SingleNodeConfig) -> String {
    let local_validator_id = format!("0x{}", hex::encode(config.validator_id));
    let local_consensus_pubkey = format!(
        "0x{}",
        hex::encode(
            SigningKey::from_bytes(&config.consensus_signing_key)
                .verifying_key()
                .to_bytes()
        )
    );
    let validator_two_consensus_pubkey = format!(
        "0x{}",
        hex::encode(SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes())
    );
    format!(
        r#"
epoch = 7

[[validators]]
validator_id = "{local_validator_id}"
consensus_pubkey = "{local_consensus_pubkey}"
network_pubkey = "0x1111"
rpc_endpoints = ["https://validator-1.testnet.0x26.xyz/api"]
region = "eu-west"
operator_name = "validator-one"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0x0202020202020202020202020202020202020202020202020202020202020202"
consensus_pubkey = "{validator_two_consensus_pubkey}"
network_pubkey = "0x3333"
rpc_endpoints = ["https://validator-2.testnet.0x26.xyz/api"]
region = "eu-west"
operator_name = "validator-two"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0x0303030303030303030303030303030303030303030303030303030303030303"
consensus_pubkey = "0x4444444444444444444444444444444444444444444444444444444444444444"
network_pubkey = "0x5555"
rpc_endpoints = ["https://validator-3.testnet.0x26.xyz/api"]
region = "eu-central"
operator_name = "validator-three"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"

[[validators]]
validator_id = "0x0404040404040404040404040404040404040404040404040404040404040404"
consensus_pubkey = "0x6666666666666666666666666666666666666666666666666666666666666666"
network_pubkey = "0x7777"
rpc_endpoints = ["https://validator-4.testnet.0x26.xyz/api"]
region = "eu-central"
operator_name = "validator-four"
enabled = true

[validators.bond]
asset = "USDC"
amount = 100
status = "Active"
"#
    )
}

#[test]
fn peer_transport_response_reader_retries_nonblocking_would_block() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener addr should exist");
    let expected = PeerTransportResponse {
        body: Some("ready".to_string()),
        error: None,
    };
    let payload = serde_json::to_vec(&expected).expect("peer transport response should serialize");
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("client should connect");
        thread::sleep(Duration::from_millis(50));
        stream
            .write_all(&(payload.len() as u32).to_be_bytes())
            .expect("response length should write");
        stream
            .write_all(&payload)
            .expect("response payload should write");
        stream.flush().expect("response should flush");
    });

    let mut stream = std::net::TcpStream::connect(address).expect("client should connect");
    stream
        .set_nonblocking(true)
        .expect("client stream should become nonblocking");

    let actual = read_peer_transport_response(&mut stream).expect("response reader should retry");

    assert_eq!(actual, expected);
    join.join().expect("server thread should join");
}

fn spawn_bootstrap_server(
    engine: SingleNodeEngine,
    config: &SingleNodeConfig,
    bearer_token: Option<&str>,
    request_count: usize,
) -> (String, thread::JoinHandle<()>) {
    let path = temp_validator_set_path("server");
    std::fs::write(&path, validator_set_toml(config)).expect("validator set should write");
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener addr should exist");
    let url = format!("http://{address}");
    let network_bootstrap =
        build_validator_network_bootstrap_config(config, &path, Some(url.clone()));
    let validator_id = config.validator_id;
    let chain_id = engine.chain_id();
    let current_round = engine.current_round();
    let head_height = engine.head().header.height;
    let snapshot = engine.snapshot_bytes();
    let metadata = serde_json::json!({
        "chain_id": chain_id,
        "current_round": current_round,
        "head_height": head_height,
        "snapshot_path": format!("{url}/network/snapshot"),
        "validator_set_epoch": network_bootstrap.validator_set.epoch,
    })
    .to_string();
    let expected_auth_header = bearer_token.map(|token| format!("authorization: Bearer {token}"));
    let join = thread::spawn(move || {
        for _ in 0..request_count {
            let (mut stream, _) = listener.accept().expect("client should connect");
            let mut prefix = [0_u8; PEER_TRANSPORT_MAGIC.len()];
            stream
                .read_exact(&mut prefix)
                .expect("request prefix should read");
            if prefix == *PEER_TRANSPORT_MAGIC {
                loop {
                    let mut length_bytes = [0_u8; 4];
                    stream
                        .read_exact(&mut length_bytes)
                        .expect("peer transport request length should read");
                    let length = u32::from_be_bytes(length_bytes) as usize;
                    let mut payload = vec![0_u8; length];
                    stream
                        .read_exact(&mut payload)
                        .expect("peer transport request payload should read");
                    let request: super::PeerTransportRequest = serde_json::from_slice(&payload)
                        .expect("peer transport request should parse");
                    let response = if request.path == "/network/session/open" {
                        let session_open: PeerTransportSessionOpenRequest =
                            serde_json::from_str(&request.body)
                                .expect("peer transport session open request should parse");
                        assert_eq!(session_open.chain_id, Some(chain_id));
                        assert!(
                            session_open
                                .validator_id_hex
                                .as_deref()
                                .is_some_and(|validator_id: &str| validator_id.starts_with("0x")),
                            "session open should include validator id metadata"
                        );
                        PeerTransportResponse {
                            body: Some(
                                json!({
                                    "session_id": 1,
                                    "chain_id": chain_id,
                                    "validator_id_hex": format!("0x{}", hex::encode(validator_id)),
                                    "current_round": current_round,
                                    "head_height": head_height,
                                })
                                .to_string(),
                            ),
                            error: None,
                        }
                    } else {
                        if request.path == "/network/snapshot" {
                            if let Some(expected_auth_header) = &expected_auth_header {
                                let expected_auth_value = expected_auth_header
                                    .split_once(": ")
                                    .expect("auth header should split")
                                    .1
                                    .to_string();
                                assert_eq!(
                                    request.headers.get("authorization"),
                                    Some(&expected_auth_value),
                                    "snapshot peer transport request should include bearer token"
                                );
                            }
                        }
                        PeerTransportResponse {
                            body: if request.path == "/network/bootstrap" {
                                Some(metadata.clone())
                            } else if request.path == "/network/snapshot" {
                                Some(BASE64_STANDARD.encode(&snapshot))
                            } else {
                                panic!("unexpected peer transport request path: {}", request.path);
                            },
                            error: None,
                        }
                    };
                    let payload = serde_json::to_vec(&response)
                        .expect("peer transport response should serialize");
                    stream
                        .write_all(&(payload.len() as u32).to_be_bytes())
                        .expect("peer transport response length should write");
                    stream
                        .write_all(&payload)
                        .expect("peer transport response payload should write");
                    stream
                        .flush()
                        .expect("peer transport response should flush");
                    if request.path != "/network/session/open" {
                        break;
                    }
                }
                continue;
            }
            let mut request_bytes = prefix.to_vec();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("request should read");
                if read == 0 {
                    break;
                }
                request_bytes.extend_from_slice(&buffer[..read]);
                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request_bytes).expect("request should be utf8");
            let response = if request.starts_with("GET /network/bootstrap ") {
                format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        metadata.len(),
                        metadata
                    )
            } else if request.starts_with("GET /network/snapshot ") {
                if let Some(expected_auth_header) = &expected_auth_header {
                    assert!(
                        request.contains(expected_auth_header),
                        "snapshot request should include bearer token"
                    );
                }
                format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        snapshot.len()
                    )
            } else {
                panic!("unexpected request: {request}");
            };
            stream
                .write_all(response.as_bytes())
                .expect("response should write");
            if request.starts_with("GET /network/snapshot ") {
                stream.write_all(&snapshot).expect("snapshot should write");
            }
            stream.flush().expect("stream should flush");
        }
        let _ = std::fs::remove_file(path);
    });
    (url, join)
}

fn spawn_snapshot_fallback_server(
    engine: SingleNodeEngine,
    config: &SingleNodeConfig,
    bearer_token: Option<&str>,
) -> (String, thread::JoinHandle<()>) {
    let path = temp_validator_set_path("snapshot-fallback");
    std::fs::write(&path, validator_set_toml(config)).expect("validator set should write");
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("listener addr should exist");
    let url = format!("http://{address}");
    let chain_id = engine.chain_id();
    let current_round = engine.current_round();
    let head_height = engine.head().header.height;
    let validator_id = config.validator_id;
    let snapshot = engine.snapshot_bytes();
    let expected_auth_header = bearer_token.map(|token| format!("authorization: Bearer {token}"));
    let join = thread::spawn(move || {
        let mut peer_snapshot_dropped = false;
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("client should connect");
            let mut prefix = [0_u8; PEER_TRANSPORT_MAGIC.len()];
            stream
                .read_exact(&mut prefix)
                .expect("request prefix should read");
            if prefix == *PEER_TRANSPORT_MAGIC {
                loop {
                    let mut length_bytes = [0_u8; 4];
                    stream
                        .read_exact(&mut length_bytes)
                        .expect("peer transport request length should read");
                    let length = u32::from_be_bytes(length_bytes) as usize;
                    let mut payload = vec![0_u8; length];
                    stream
                        .read_exact(&mut payload)
                        .expect("peer transport request payload should read");
                    let request: super::PeerTransportRequest = serde_json::from_slice(&payload)
                        .expect("peer transport request should parse");
                    if request.path == "/network/session/open" {
                        let response = PeerTransportResponse {
                            body: Some(
                                json!({
                                    "session_id": 1,
                                    "chain_id": chain_id,
                                    "validator_id_hex": format!("0x{}", hex::encode(validator_id)),
                                    "current_round": current_round,
                                    "head_height": head_height,
                                })
                                .to_string(),
                            ),
                            error: None,
                        };
                        let response_payload = serde_json::to_vec(&response)
                            .expect("peer transport response should serialize");
                        stream
                            .write_all(&(response_payload.len() as u32).to_be_bytes())
                            .expect("peer transport response length should write");
                        stream
                            .write_all(&response_payload)
                            .expect("peer transport response payload should write");
                        stream
                            .flush()
                            .expect("peer transport response should flush");
                        continue;
                    }
                    assert_eq!(
                        request.path, "/network/snapshot",
                        "unexpected peer transport request path"
                    );
                    if let Some(expected_auth_header) = &expected_auth_header {
                        let expected_auth_value = expected_auth_header
                            .split_once(": ")
                            .expect("auth header should split")
                            .1
                            .to_string();
                        assert_eq!(
                            request.headers.get("authorization"),
                            Some(&expected_auth_value),
                            "snapshot peer transport request should include bearer token"
                        );
                    }
                    peer_snapshot_dropped = true;
                    break;
                }
                continue;
            }

            let mut request_bytes = prefix.to_vec();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buffer).expect("request should read");
                if read == 0 {
                    break;
                }
                request_bytes.extend_from_slice(&buffer[..read]);
                if request_bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(request_bytes).expect("request should be utf8");
            assert!(
                request.starts_with("GET /network/snapshot "),
                "unexpected fallback request: {request}"
            );
            if let Some(expected_auth_header) = &expected_auth_header {
                assert!(
                    request.contains(expected_auth_header),
                    "snapshot request should include bearer token"
                );
            }
            let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    snapshot.len()
                );
            stream
                .write_all(response.as_bytes())
                .expect("response should write");
            stream.write_all(&snapshot).expect("snapshot should write");
            stream.flush().expect("stream should flush");
        }
        assert!(
            peer_snapshot_dropped,
            "peer transport snapshot path should be exercised"
        );
        let _ = std::fs::remove_file(path);
    });
    (url, join)
}

#[test]
fn parses_auto_produce_config_from_env() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::set_var("KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS", "250") };
    unsafe { env::set_var("KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS", "15") };

    let config = parse_auto_produce_config();

    unsafe { env::remove_var("KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS") };

    assert_eq!(
        config,
        Some(AutoProduceConfig {
            interval_ms: 250,
            max_items: 15,
        })
    );
}

#[test]
fn auto_produce_config_is_disabled_without_interval() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS") };

    assert_eq!(parse_auto_produce_config(), None);
}

#[test]
fn parses_auto_network_round_config_from_env() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::set_var("KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS", "300") };
    unsafe { env::set_var("KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS", "25") };
    unsafe { env::set_var("KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS", "12") };
    unsafe { env::set_var("KOMBAT_DEV_AUTO_NETWORK_START_AT_UNIX_MS", "12345") };

    let config = parse_auto_network_round_config();

    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_START_AT_UNIX_MS") };

    assert_eq!(
        config,
        Some(AutoNetworkRoundConfig {
            interval_ms: 300,
            drive_interval_ms: 25,
            max_items: 12,
            start_at_unix_ms: Some(12345),
        })
    );
}

#[test]
fn auto_network_round_config_is_disabled_without_interval() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS") };
    unsafe { env::remove_var("KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS") };

    assert_eq!(parse_auto_network_round_config(), None);
}

#[test]
fn rebases_auto_network_round_timestamp_when_head_advances() {
    assert_eq!(
        rebase_auto_network_round_timestamp(1_100, 1_000, 100, 1_050),
        None
    );
    assert_eq!(
        rebase_auto_network_round_timestamp(1_100, 1_250, 100, 1_350),
        Some(1_350)
    );
}

#[test]
fn rebases_auto_network_round_timestamp_without_running_ahead_of_wall_clock() {
    assert_eq!(
        rebase_auto_network_round_timestamp(1_100, 1_000, 100, 1_500),
        Some(1_100)
    );
    assert_eq!(
        rebase_auto_network_round_timestamp(1_600, 1_000, 100, 1_500),
        None
    );
}

#[test]
fn rebased_auto_network_round_timestamp_saturates_on_large_head() {
    assert_eq!(
        rebase_auto_network_round_timestamp(5, u64::MAX, 100, 10),
        None
    );
}

#[test]
fn peer_transport_keepalive_defaults_when_enabled() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::remove_var("KOMBAT_PEER_TRANSPORT_KEEPALIVE_ENABLED") };
    unsafe { env::remove_var("KOMBAT_PEER_TRANSPORT_KEEPALIVE_INTERVAL_MS") };

    assert_eq!(
        parse_peer_transport_keepalive_config(true, true),
        Some(PeerTransportKeepaliveConfig { interval_ms: 5_000 })
    );
    assert_eq!(parse_peer_transport_keepalive_config(false, true), None);
    assert_eq!(parse_peer_transport_keepalive_config(true, false), None);
}

#[test]
fn peer_transport_keepalive_can_be_disabled() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::set_var("KOMBAT_PEER_TRANSPORT_KEEPALIVE_ENABLED", "0") };

    assert_eq!(parse_peer_transport_keepalive_config(true, true), None);

    unsafe { env::remove_var("KOMBAT_PEER_TRANSPORT_KEEPALIVE_ENABLED") };
}

#[test]
fn peer_transport_runtime_enablement_requires_network_only() {
    assert!(peer_transport_enabled_for_network(true, true));
    assert!(!peer_transport_enabled_for_network(false, true));
    assert!(!peer_transport_enabled_for_network(true, false));
}

#[test]
fn parses_bool_flags_from_env() {
    let _guard = env_lock().lock().expect("env lock should not be poisoned");
    unsafe { env::set_var("KOMBAT_DEV_ENABLE_MANUAL_PRODUCE", "true") };
    assert!(parse_env_bool("KOMBAT_DEV_ENABLE_MANUAL_PRODUCE", false));
    unsafe { env::set_var("KOMBAT_DEV_ENABLE_MANUAL_PRODUCE", "0") };
    assert!(!parse_env_bool("KOMBAT_DEV_ENABLE_MANUAL_PRODUCE", true));
    unsafe { env::remove_var("KOMBAT_DEV_ENABLE_MANUAL_PRODUCE") };
}

#[test]
fn build_engine_loads_snapshot_file_and_ignores_bootstrap_block() {
    let config = single_node_config(7);
    let snapshot_path = temp_snapshot_path("load");
    let mut engine = SingleNodeEngine::new(config.clone());
    engine
        .produce_empty_block(1_100)
        .expect("bootstrap block should produce");
    engine
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot should save");

    let restored = build_engine(
        RuntimeRole::Validator,
        &config,
        None,
        Some(
            snapshot_path
                .to_str()
                .expect("snapshot path should be utf8"),
        ),
        None,
        false,
        Some(1_200),
        None,
    );

    assert_eq!(restored.head().header.height, 1);
    assert_eq!(restored.head().header.timestamp_ms, 1_100);
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn build_engine_loads_snapshot_file_and_continues_block_production() {
    let config = single_node_config(7);
    let snapshot_path = temp_snapshot_path("continue");
    let mut engine = SingleNodeEngine::new(config.clone());
    let first = engine
        .produce_empty_block(1_100)
        .expect("first block should produce")
        .clone();
    let first_hash = kombat_chain_consensus::block_hash(&first);
    engine
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot should save");

    let mut restored = build_engine(
        RuntimeRole::Validator,
        &config,
        None,
        Some(
            snapshot_path
                .to_str()
                .expect("snapshot path should be utf8"),
        ),
        None,
        false,
        Some(1_200),
        None,
    );
    let second = restored
        .produce_empty_block(1_200)
        .expect("restored engine should continue block production")
        .clone();

    assert_eq!(second.header.height, 2);
    assert_eq!(second.header.parent_hash, first_hash);
    assert_eq!(second.header.timestamp_ms, 1_200);
    assert_eq!(
        restored.current_round(),
        second.header.round.saturating_add(1)
    );
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn build_engine_rejects_snapshot_with_mismatched_chain_id() {
    let snapshot_path = temp_snapshot_path("mismatch");
    let mismatched_config = single_node_config(8);
    SingleNodeEngine::new(mismatched_config)
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot should save");

    let result = std::panic::catch_unwind(|| {
        let config = single_node_config(7);
        let _ = build_engine(
            RuntimeRole::Validator,
            &config,
            None,
            Some(
                snapshot_path
                    .to_str()
                    .expect("snapshot path should be utf8"),
            ),
            None,
            false,
            None,
            None,
        );
    });

    assert!(result.is_err(), "mismatched chain id should panic");
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn build_engine_rejects_snapshot_without_configured_validator_id() {
    let snapshot_path = temp_snapshot_path("validator-mismatch");
    SingleNodeEngine::new(single_node_config(7))
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot should save");

    let result = std::panic::catch_unwind(|| {
        let mut config = single_node_config(7);
        config.validator_id = [8; 32];
        let _ = build_engine(
            RuntimeRole::Validator,
            &config,
            None,
            Some(
                snapshot_path
                    .to_str()
                    .expect("snapshot path should be utf8"),
            ),
            None,
            false,
            None,
            None,
        );
    });

    assert!(
        result.is_err(),
        "snapshot missing configured validator id should panic"
    );
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn build_engine_rejects_missing_snapshot_file() {
    let snapshot_path = temp_snapshot_path("missing");

    let result = std::panic::catch_unwind(|| {
        let config = single_node_config(7);
        let _ = build_engine(
            RuntimeRole::Validator,
            &config,
            None,
            Some(
                snapshot_path
                    .to_str()
                    .expect("snapshot path should be utf8"),
            ),
            None,
            false,
            None,
            None,
        );
    });

    assert!(result.is_err(), "missing snapshot file should panic");
}

#[test]
fn build_engine_rejects_truncated_snapshot_file() {
    let snapshot_path = temp_snapshot_path("truncated");
    std::fs::write(&snapshot_path, [1_u8, 2, 3, 4]).expect("truncated snapshot should write");

    let result = std::panic::catch_unwind(|| {
        let config = single_node_config(7);
        let _ = build_engine(
            RuntimeRole::Validator,
            &config,
            None,
            Some(
                snapshot_path
                    .to_str()
                    .expect("snapshot path should be utf8"),
            ),
            None,
            false,
            None,
            None,
        );
    });

    assert!(result.is_err(), "truncated snapshot file should panic");
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn build_engine_rejects_snapshot_with_mismatched_consensus_signing_key() {
    let snapshot_path = temp_snapshot_path("signing-key-mismatch");
    SingleNodeEngine::new(single_node_config(7))
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot should save");

    let result = std::panic::catch_unwind(|| {
        let mut config = single_node_config(7);
        config.consensus_signing_key = [8; 32];
        let _ = build_engine(
            RuntimeRole::Validator,
            &config,
            None,
            Some(
                snapshot_path
                    .to_str()
                    .expect("snapshot path should be utf8"),
            ),
            None,
            false,
            None,
            None,
        );
    });

    assert!(
        result.is_err(),
        "snapshot with mismatched configured signing key should panic"
    );
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn builds_network_bootstrap_config_from_validator_set() {
    let config = single_node_config(7);
    let path = temp_validator_set_path("bootstrap");
    std::fs::write(&path, validator_set_toml(&config)).expect("validator set should write");

    let bootstrap = build_validator_network_bootstrap_config(
        &config,
        &path,
        Some("https://validator-1.testnet.0x26.xyz/api".into()),
    );

    assert_eq!(bootstrap.validator_set.epoch, 7);
    assert_eq!(
        bootstrap
            .local_validator
            .as_ref()
            .expect("validator role should include local validator")
            .validator_id,
        config.validator_id
    );
    assert_eq!(bootstrap.bootstrap_peers.peers.len(), 3);
    assert_eq!(
        bootstrap.public_rpc_url,
        Some("https://validator-1.testnet.0x26.xyz/api".into())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn build_network_bootstrap_config_rejects_mismatched_consensus_key() {
    let config = single_node_config(7);
    let path = temp_validator_set_path("bootstrap-mismatch");
    let bad_config = validator_set_toml(&SingleNodeConfig {
        consensus_signing_key: [8; 32],
        ..config.clone()
    });
    std::fs::write(&path, bad_config).expect("validator set should write");

    let result = std::panic::catch_unwind(|| {
        let _ = build_validator_network_bootstrap_config(&config, &path, None);
    });

    assert!(result.is_err(), "mismatched consensus key should panic");
    let _ = std::fs::remove_file(path);
}

#[test]
fn builds_ingress_network_bootstrap_config_without_local_validator() {
    let config = single_node_config(7);
    let path = temp_validator_set_path("ingress-bootstrap");
    std::fs::write(&path, validator_set_toml(&config)).expect("validator set should write");

    let bootstrap = build_ingress_network_bootstrap_config(
        &path,
        Some("https://ingress.testnet.0x26.xyz/api".into()),
    );

    assert!(bootstrap.local_validator.is_none());
    assert_eq!(
        bootstrap.bootstrap_peers.peers.len(),
        bootstrap.validator_set.active_validators().len()
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn build_engine_clears_local_validator_binding_in_ingress_mode() {
    let config = single_node_config(7);
    let restored = build_engine(
        RuntimeRole::Ingress,
        &config,
        None,
        None,
        None,
        false,
        None,
        None,
    );

    assert!(restored.local_validator_id().is_none());
    assert!(restored.local_consensus_signing_key().is_none());
    assert!(restored.validator_set().is_empty());
}

#[test]
fn build_engine_initializes_empty_redb_backend() {
    let config = single_node_config(7);
    let dir = temp_state_dir("runtime-redb-init");
    std::fs::create_dir_all(&dir).expect("state dir should create");
    let path = dir.join("state.redb");
    let backend =
        AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("redb should open"));

    let restored = build_engine(
        RuntimeRole::Validator,
        &config,
        None,
        None,
        None,
        false,
        Some(1_100),
        Some(&backend),
    );

    assert_eq!(restored.head().header.height, 1);
    let meta = backend
        .latest_committed_meta()
        .expect("meta should read")
        .expect("meta should exist");
    assert_eq!(meta.height, 1);
    let hydrated = SingleNodeEngine::from_state_backend(&backend).expect("hydrate redb");
    assert_eq!(hydrated.head().header.height, 1);
    drop(backend);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn build_engine_prefers_existing_redb_backend_over_snapshot() {
    let config = single_node_config(7);
    let dir = temp_state_dir("runtime-redb-existing");
    std::fs::create_dir_all(&dir).expect("state dir should create");
    let path = dir.join("state.redb");
    let backend =
        AnyStateBackend::Redb(RedbBackend::open(OpenOptions::new(path)).expect("redb should open"));
    let snapshot_path = temp_snapshot_path("ignored-redb-snapshot");

    let restored = build_engine(
        RuntimeRole::Validator,
        &config,
        None,
        None,
        None,
        false,
        Some(1_100),
        Some(&backend),
    );
    assert_eq!(restored.head().header.height, 1);

    let mut snapshot_engine = SingleNodeEngine::new(config.clone());
    snapshot_engine
        .produce_empty_block(1_100)
        .expect("first snapshot block should produce");
    snapshot_engine
        .produce_empty_block(1_200)
        .expect("second snapshot block should produce");
    snapshot_engine
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot should save");

    let restored_again = build_engine(
        RuntimeRole::Validator,
        &config,
        None,
        Some(
            snapshot_path
                .to_str()
                .expect("snapshot path should be utf8"),
        ),
        None,
        false,
        Some(1_300),
        Some(&backend),
    );

    assert_eq!(restored_again.head().header.height, 1);
    drop(backend);
    let _ = std::fs::remove_file(snapshot_path);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn fetches_remote_bootstrap_metadata_and_snapshot() {
    let config = single_node_config(7);
    let mut engine = SingleNodeEngine::new(config.clone());
    engine
        .produce_empty_block(1_100)
        .expect("bootstrap block should produce");
    let expected_snapshot = engine.snapshot_bytes();
    let (url, join) = spawn_bootstrap_server(engine, &config, Some("bootstrap-token"), 2);

    let metadata = fetch_remote_bootstrap_metadata(
        &config,
        &RemoteBootstrapConfig {
            bootstrap_rpc_url: url.clone(),
            bearer_token: Some("bootstrap-token".into()),
        },
        false,
    );
    assert_eq!(metadata.chain_id, 7);
    assert_eq!(metadata.head_height, 1);
    let snapshot = fetch_remote_snapshot_bytes(
        &config,
        metadata
            .snapshot_path
            .as_deref()
            .expect("snapshot path should be present"),
        &RemoteBootstrapConfig {
            bootstrap_rpc_url: url,
            bearer_token: Some("bootstrap-token".into()),
        },
        false,
    );
    assert_eq!(snapshot, expected_snapshot);
    join.join().expect("server thread should join");
}

#[test]
fn fetches_remote_bootstrap_metadata_over_peer_transport() {
    let config = single_node_config(7);
    let mut engine = SingleNodeEngine::new(config.clone());
    engine
        .produce_empty_block(1_100)
        .expect("bootstrap block should produce");
    let (url, join) = spawn_bootstrap_server(engine, &config, Some("bootstrap-token"), 1);

    let metadata = fetch_remote_bootstrap_metadata(
        &config,
        &RemoteBootstrapConfig {
            bootstrap_rpc_url: url,
            bearer_token: Some("bootstrap-token".into()),
        },
        true,
    );
    assert_eq!(metadata.chain_id, 7);
    assert_eq!(metadata.head_height, 1);
    join.join().expect("server thread should join");
}

#[test]
fn fetches_remote_snapshot_bytes_over_peer_transport() {
    let config = single_node_config(7);
    let mut engine = SingleNodeEngine::new(config.clone());
    engine
        .produce_empty_block(1_100)
        .expect("bootstrap block should produce");
    let expected_snapshot = engine.snapshot_bytes();
    let (url, join) = spawn_bootstrap_server(engine, &config, Some("bootstrap-token"), 1);

    let snapshot = fetch_remote_snapshot_bytes(
        &config,
        &format!("{url}/network/snapshot"),
        &RemoteBootstrapConfig {
            bootstrap_rpc_url: url,
            bearer_token: Some("bootstrap-token".into()),
        },
        true,
    );
    assert_eq!(snapshot, expected_snapshot);
    join.join().expect("server thread should join");
}

#[test]
fn falls_back_to_http_when_peer_transport_snapshot_fetch_fails() {
    let config = single_node_config(7);
    let mut engine = SingleNodeEngine::new(config.clone());
    engine
        .produce_empty_block(1_100)
        .expect("bootstrap block should produce");
    let expected_snapshot = engine.snapshot_bytes();
    let (url, join) = spawn_snapshot_fallback_server(engine, &config, Some("bootstrap-token"));

    let snapshot = fetch_remote_snapshot_bytes(
        &config,
        &format!("{url}/network/snapshot"),
        &RemoteBootstrapConfig {
            bootstrap_rpc_url: url,
            bearer_token: Some("bootstrap-token".into()),
        },
        true,
    );

    assert_eq!(snapshot, expected_snapshot);
    join.join().expect("server thread should join");
}

#[test]
fn build_engine_bootstraps_from_remote_snapshot() {
    let config = single_node_config(7);
    let mut engine = SingleNodeEngine::new(config.clone());
    let first = engine
        .produce_empty_block(1_100)
        .expect("bootstrap block should produce")
        .clone();
    let first_hash = kombat_chain_consensus::block_hash(&first);
    let (url, join) = spawn_bootstrap_server(engine, &config, Some("bootstrap-token"), 2);

    let mut restored = build_engine(
        RuntimeRole::Validator,
        &config,
        None,
        None,
        Some(&RemoteBootstrapConfig {
            bootstrap_rpc_url: url,
            bearer_token: Some("bootstrap-token".into()),
        }),
        false,
        Some(1_200),
        None,
    );
    let second = restored
        .produce_empty_block(1_200)
        .expect("restored engine should continue block production")
        .clone();

    assert_eq!(second.header.height, 2);
    assert_eq!(second.header.parent_hash, first_hash);
    assert_eq!(second.header.timestamp_ms, 1_200);
    join.join().expect("server thread should join");
}

#[test]
fn build_engine_bootstraps_distinct_validator_from_remote_snapshot() {
    let local_config = single_node_config(7);
    let path = temp_validator_set_path("distinct-bootstrap");
    std::fs::write(&path, validator_set_toml(&local_config)).expect("validator set should write");
    let validator_set =
        load_validator_set_from_path(&path).expect("validator set should load from path");
    let engine =
        SingleNodeEngine::new_with_validator_set(local_config.clone(), Some(&validator_set));
    let (url, join) = spawn_bootstrap_server(engine, &local_config, Some("bootstrap-token"), 2);

    let mut peer_config = single_node_config(7);
    peer_config.validator_id = [2; 32];
    peer_config.consensus_signing_key = [8; 32];

    let mut restored = build_engine(
        RuntimeRole::Validator,
        &peer_config,
        Some(&validator_set),
        None,
        Some(&RemoteBootstrapConfig {
            bootstrap_rpc_url: url,
            bearer_token: Some("bootstrap-token".into()),
        }),
        false,
        None,
        None,
    );
    let produced = restored
        .produce_empty_block(1_100)
        .expect("bootstrapped peer should produce under its own validator id")
        .clone();

    assert_eq!(restored.validator_set(), &[peer_config.validator_id]);
    assert_eq!(produced.header.proposer, peer_config.validator_id);
    assert_eq!(
        restored.validator_consensus_pubkey(peer_config.validator_id),
        Some(
            SigningKey::from_bytes(&peer_config.consensus_signing_key)
                .verifying_key()
                .to_bytes()
        )
    );

    join.join().expect("server thread should join");
    let _ = std::fs::remove_file(path);
}

#[test]
fn build_engine_rejects_remote_bootstrap_chain_id_mismatch() {
    let local_config = single_node_config(7);
    let remote_config = single_node_config(8);
    let engine = SingleNodeEngine::new(remote_config.clone());
    let (url, join) = spawn_bootstrap_server(engine, &remote_config, Some("bootstrap-token"), 1);

    let result = std::panic::catch_unwind(|| {
        let _ = build_engine(
            RuntimeRole::Validator,
            &local_config,
            None,
            None,
            Some(&RemoteBootstrapConfig {
                bootstrap_rpc_url: url,
                bearer_token: Some("bootstrap-token".into()),
            }),
            false,
            None,
            None,
        );
    });

    assert!(result.is_err(), "mismatched remote chain id should panic");
    join.join().expect("server thread should join");
}
