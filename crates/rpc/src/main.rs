use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use kombat_chain_consensus::PacemakerConfig;
use kombat_chain_network::{bootstrap_validator, load_validator_set_from_path};
use kombat_chain_node::{
    migrations, EngineHandle, IsolatedCollateralRepairReport as NodeIsolatedCollateralRepairReport,
    IsolatedCollateralRepairTarget, MarketCustodyAuditReport as NodeMarketCustodyAuditReport,
    MarketCustodyRepairReport as NodeMarketCustodyRepairReport, SingleNodeConfig, SingleNodeEngine,
};
use kombat_chain_rpc::{
    parse_mahi_commit_rule, JsonRpcHttpServer, JsonRpcServerConfig, NetworkBootstrapConfig,
};
use kombat_chain_state_db::{
    AnyStateBackend, BackendError, Namespace, OpenOptions, ReadTxn, RedbBackend, StateBackend,
    SyncMode, WriteBatch,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod main_maintenance;

use main_maintenance::{maybe_run_maintenance_command, resolve_state_db_path};

#[cfg(test)]
pub(crate) use main_maintenance::{
    check_persistence_fingerprint_output, collect_backend_inventory,
    parse_check_persistence_fingerprint_args, parse_format_only_args, parse_migrate_snapshot_args,
    parse_migrate_state_args, run_check_persistence_fingerprint_command,
    run_list_persistence_migrations_command, run_migrate_snapshot_command_with_registry,
    run_migrate_state_command, run_migrate_state_command_with_registry,
    validate_migrated_inventory, MaintenanceOutputFormat, MigrateStateInventoryReport,
};

const PEER_TRANSPORT_MAGIC: &[u8; 8] = b"KMBTPR1\n";
const MAX_PEER_TRANSPORT_FRAME_BYTES: usize = 8 * 1024 * 1024;
const PEER_TRANSPORT_FRAME_READ_TIMEOUT_MS: u64 = 60_000;
const REMOTE_BOOTSTRAP_ATTEMPTS: usize = 12;
const REMOTE_BOOTSTRAP_RETRY_DELAY_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AutoProduceConfig {
    interval_ms: u64,
    max_items: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AutoNetworkRoundConfig {
    interval_ms: u64,
    drive_interval_ms: u64,
    max_items: usize,
    start_at_unix_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PeerTransportKeepaliveConfig {
    interval_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RemoteBootstrapConfig {
    bootstrap_rpc_url: String,
    bearer_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeStateBackendConfig {
    db_path: PathBuf,
    admission_sync_mode: SyncMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct RemoteBootstrapMetadata {
    chain_id: u64,
    current_round: u64,
    head_height: u64,
    snapshot_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PeerTransportRequest {
    path: String,
    headers: BTreeMap<String, String>,
    body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PeerTransportResponse {
    body: Option<String>,
    error: Option<PeerTransportError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PeerTransportSessionOpenResponse {
    session_id: u64,
    chain_id: u64,
    validator_id_hex: Option<String>,
    current_round: u64,
    head_height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PeerTransportSessionOpenRequest {
    chain_id: Option<u64>,
    validator_id_hex: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PeerTransportError {
    message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeRole {
    Validator,
    Ingress,
}

fn main() {
    if let Some(exit_code) = maybe_run_maintenance_command() {
        process::exit(exit_code);
    }

    let bind = env::var("KOMBAT_RPC_BIND").unwrap_or_else(|_| "127.0.0.1:8545".into());
    let runtime_role = parse_runtime_role();
    let mahi_commit_rule = parse_mahi_commit_rule();
    let manual_block_production_enabled = parse_env_bool("KOMBAT_DEV_ENABLE_MANUAL_PRODUCE", false);
    let peer_transport_enabled = parse_env_bool("KOMBAT_PEER_TRANSPORT_ENABLED", false);
    let operator_bearer_token = env::var("KOMBAT_OPERATOR_TOKEN").ok();
    let peer_relay_bearer_token = env::var("KOMBAT_PEER_RELAY_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty());
    let load_snapshot_path = env::var("KOMBAT_LOAD_SNAPSHOT_PATH").ok();
    let remote_bootstrap = parse_remote_bootstrap_config();
    let runtime_state_backend = parse_runtime_state_backend_config(runtime_role);
    let state_backend = runtime_state_backend
        .as_ref()
        .map(open_runtime_state_backend);
    let bootstrap_empty_block_timestamp_ms =
        parse_optional_env_u64("KOMBAT_BOOTSTRAP_EMPTY_BLOCK_TIMESTAMP_MS");
    let consensus_signing_key = parse_env_hash32("KOMBAT_CONSENSUS_SIGNING_KEY_HEX", [7; 32]);
    let config = SingleNodeConfig {
        chain_id: parse_env_u64("KOMBAT_CHAIN_ID", 7),
        validator_id: parse_optional_env_hash32("KOMBAT_VALIDATOR_ID_HEX").unwrap_or_else(|| {
            SigningKey::from_bytes(&consensus_signing_key)
                .verifying_key()
                .to_bytes()
        }),
        consensus_signing_key,
        genesis_timestamp_ms: parse_env_u64("KOMBAT_GENESIS_TIMESTAMP_MS", 1_000),
        genesis_state_root: parse_env_hash32("KOMBAT_GENESIS_STATE_ROOT_HEX", [0; 32]),
        pacemaker: PacemakerConfig {
            base_timeout_ms: parse_env_u64("KOMBAT_BASE_TIMEOUT_MS", 300),
            max_timeout_ms: parse_env_u64("KOMBAT_MAX_TIMEOUT_MS", 20_000),
            max_exponent: parse_env_u32("KOMBAT_MAX_TIMEOUT_EXPONENT", 6),
            max_clock_skew_ms: parse_env_u64("KOMBAT_MAX_CLOCK_SKEW_MS", 1_000),
        },
    };
    let network_bootstrap = parse_network_bootstrap_config(&config, runtime_role);
    let auto_network_round = parse_auto_network_round_config();
    let peer_transport_runtime_enabled =
        peer_transport_enabled_for_network(peer_transport_enabled, network_bootstrap.is_some());
    let peer_transport_keepalive = parse_peer_transport_keepalive_config(
        peer_transport_runtime_enabled,
        network_bootstrap.is_some(),
    );

    let engine = build_engine(
        runtime_role,
        &config,
        network_bootstrap
            .as_ref()
            .map(|bootstrap| &bootstrap.validator_set),
        load_snapshot_path.as_deref(),
        remote_bootstrap.as_ref(),
        peer_transport_enabled,
        bootstrap_empty_block_timestamp_ms,
        state_backend.as_deref(),
    );

    let engine = Arc::new(match state_backend {
        Some(backend) => EngineHandle::new_durable(
            engine,
            backend,
            runtime_state_backend
                .as_ref()
                .expect("state backend config should exist when backend is open")
                .admission_sync_mode,
        ),
        None => EngineHandle::new(engine),
    });
    if let Some(auto_produce) = parse_auto_produce_config() {
        println!(
            "kombat_chain dev auto-produce enabled: interval_ms={}, max_items={}",
            auto_produce.interval_ms, auto_produce.max_items
        );
        spawn_auto_producer(engine.clone(), auto_produce);
    }

    let server = JsonRpcHttpServer::new_with_engine_handle(
        engine,
        JsonRpcServerConfig {
            manual_block_production_enabled,
            mahi_enabled: true,
            operator_bearer_token: operator_bearer_token.clone(),
            peer_relay_bearer_token: peer_relay_bearer_token.clone(),
            network_bootstrap: network_bootstrap.clone(),
            peer_transport_enabled: peer_transport_runtime_enabled,
        },
    );
    if let Some(auto_network_round) = auto_network_round {
        if matches!(runtime_role, RuntimeRole::Validator) && network_bootstrap.is_some() {
            println!(
                "kombat_chain dev auto-mahi-round enabled: interval_ms={}, max_items={}",
                auto_network_round.interval_ms, auto_network_round.max_items
            );
            spawn_auto_mahi_round(server.clone(), auto_network_round);
        } else {
            println!(
                "kombat_chain ignoring auto-network-round config because validator set is not configured"
            );
        }
    }
    if let Some(peer_transport_keepalive) = peer_transport_keepalive {
        println!(
            "kombat_chain peer transport keepalive enabled: interval_ms={}",
            peer_transport_keepalive.interval_ms
        );
        spawn_peer_transport_keepalive(server.clone(), peer_transport_keepalive);
    }
    if let Some(network_bootstrap) = &network_bootstrap {
        println!(
            "kombat_chain network topology enabled: epoch={}, local_validator={}, peers={}",
            network_bootstrap.validator_set.epoch,
            network_bootstrap
                .local_validator
                .as_ref()
                .map(|validator| format!("0x{}", hex::encode(validator.validator_id)))
                .unwrap_or_else(|| "observer".into()),
            network_bootstrap.bootstrap_peers.peers.len()
        );
    }
    println!(
        "kombat_chain runtime role={}",
        match runtime_role {
            RuntimeRole::Validator => "validator",
            RuntimeRole::Ingress => "ingress",
        }
    );
    println!("kombat_chain manual kombat_produceBlock enabled={manual_block_production_enabled}");
    println!("kombat_chain peer transport enabled={peer_transport_runtime_enabled}");
    println!(
        "kombat_chain operator token configured={}",
        operator_bearer_token.is_some()
    );
    println!(
        "kombat_chain peer relay token configured={}",
        peer_relay_bearer_token.is_some()
    );
    println!("kombat_chain consensus mode=mahi");
    println!(
        "kombat_chain mahi commit rule={}",
        match mahi_commit_rule {
            kombat_chain_consensus::mahi::CommitRule::Mahi => "mahi",
            kombat_chain_consensus::mahi::CommitRule::Mysticeti => "mysticeti",
        }
    );
    println!("kombat_chain serving on http://{bind}");
    server.serve(&bind).expect("json-rpc server should bind");
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis()
        .try_into()
        .expect("unix timestamp should fit in u64")
}

fn parse_runtime_state_backend_config(
    runtime_role: RuntimeRole,
) -> Option<RuntimeStateBackendConfig> {
    let default_backend = match runtime_role {
        RuntimeRole::Validator => "redb",
        RuntimeRole::Ingress => "memory",
    };
    let backend = env::var("KOMBAT_STATE_BACKEND")
        .unwrap_or_else(|_| default_backend.into())
        .to_ascii_lowercase();
    match backend.as_str() {
        "redb" => Some(RuntimeStateBackendConfig {
            db_path: parse_runtime_state_db_path(),
            admission_sync_mode: parse_runtime_admission_sync_mode(),
        }),
        "memory" | "in-memory" | "in_memory" | "none" | "disabled" => None,
        other => panic!("KOMBAT_STATE_BACKEND must be redb or memory, got {other}"),
    }
}

fn parse_runtime_state_db_path() -> PathBuf {
    if let Ok(path) = env::var("KOMBAT_STATE_DB_PATH") {
        return PathBuf::from(path);
    }
    let state_dir = env::var("KOMBAT_STATE_DIR").unwrap_or_else(|_| "/var/lib/and-chain".into());
    resolve_state_db_path(Path::new(&state_dir))
}

fn parse_runtime_admission_sync_mode() -> SyncMode {
    let mode = env::var("KOMBAT_MEMPOOL_FSYNC")
        .unwrap_or_else(|_| "sync".into())
        .to_ascii_lowercase();
    match mode.as_str() {
        "sync" => SyncMode::Sync,
        "group_commit" | "group-commit" => SyncMode::GroupCommit {
            max_delay_ms: parse_env_u64("KOMBAT_MEMPOOL_GROUP_COMMIT_MS", 10),
        },
        "async_batched_testnet" | "async-batched-testnet" | "testnet" => {
            SyncMode::AsyncBatchedTestnet
        }
        other => panic!(
            "KOMBAT_MEMPOOL_FSYNC must be sync, group_commit, or async_batched_testnet, got {other}"
        ),
    }
}

fn open_runtime_state_backend(config: &RuntimeStateBackendConfig) -> Arc<AnyStateBackend> {
    if let Some(parent) = config.db_path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            panic!(
                "failed to create state backend directory {}: {error}",
                parent.display()
            )
        });
    }
    let backend =
        RedbBackend::open(OpenOptions::new(config.db_path.clone())).unwrap_or_else(|error| {
            panic!(
                "failed to open redb state backend {}: {error}",
                config.db_path.display()
            )
        });
    println!(
        "kombat_chain state backend=redb path={} admission_sync_mode={:?}",
        config.db_path.display(),
        config.admission_sync_mode
    );
    Arc::new(AnyStateBackend::Redb(backend))
}

fn build_engine(
    runtime_role: RuntimeRole,
    config: &SingleNodeConfig,
    configured_validator_set: Option<&kombat_chain_network::ValidatorSet>,
    load_snapshot_path: Option<&str>,
    remote_bootstrap: Option<&RemoteBootstrapConfig>,
    peer_transport_enabled: bool,
    bootstrap_empty_block_timestamp_ms: Option<u64>,
    state_backend: Option<&AnyStateBackend>,
) -> SingleNodeEngine {
    if let Some(backend) = state_backend {
        let check = backend
            .check_persistence_fingerprint()
            .unwrap_or_else(|error| panic!("failed to check state backend fingerprint: {error}"));
        if !check.is_compatible() {
            panic!(
                "state backend fingerprint check failed: {}",
                check
                    .error_reason()
                    .unwrap_or_else(|| "unknown incompatibility".into())
            );
        }

        if backend
            .latest_committed_meta()
            .unwrap_or_else(|error| panic!("failed to read state backend meta: {error}"))
            .is_some()
        {
            let mut engine = SingleNodeEngine::from_state_backend(backend)
                .unwrap_or_else(|error| panic!("failed to hydrate redb state backend: {error}"));
            bind_engine_for_runtime(&mut engine, runtime_role, config, "redb state backend");
            if load_snapshot_path.is_some() || remote_bootstrap.is_some() {
                println!(
                    "kombat_chain ignoring snapshot/bootstrap config because redb state exists"
                );
            }
            if bootstrap_empty_block_timestamp_ms.is_some() {
                println!(
                    "kombat_chain ignoring bootstrap empty block timestamp because redb state exists"
                );
            }
            println!(
                "kombat_chain hydrated redb state at height {}",
                engine.head().header.height
            );
            return engine;
        }

        let engine = build_engine(
            runtime_role,
            config,
            configured_validator_set,
            load_snapshot_path,
            remote_bootstrap,
            peer_transport_enabled,
            bootstrap_empty_block_timestamp_ms,
            None,
        );
        engine
            .initialize_state_backend(backend, unix_time_ms())
            .unwrap_or_else(|error| panic!("failed to initialize redb state backend: {error}"));
        println!(
            "kombat_chain initialized redb state at height {}",
            engine.head().header.height
        );
        return engine;
    }

    match load_snapshot_path {
        Some(path) => {
            let mut engine = SingleNodeEngine::load_snapshot_file(path)
                .unwrap_or_else(|error| panic!("failed to load snapshot from {path}: {error}"));
            bind_engine_for_runtime(&mut engine, runtime_role, config, path);
            if bootstrap_empty_block_timestamp_ms.is_some() {
                println!(
                    "kombat_chain ignoring bootstrap empty block timestamp because snapshot was loaded"
                );
            }
            println!("kombat_chain loaded snapshot from {path}");
            engine
        }
        None => {
            if let Some(remote_bootstrap) = remote_bootstrap {
                let metadata = fetch_remote_bootstrap_metadata(
                    config,
                    remote_bootstrap,
                    peer_transport_enabled,
                );
                assert_eq!(
                    metadata.chain_id, config.chain_id,
                    "remote bootstrap chain_id does not match configured chain_id"
                );
                let snapshot_url = metadata.snapshot_path.unwrap_or_else(|| {
                    format!(
                        "{}/network/snapshot",
                        remote_bootstrap.bootstrap_rpc_url.trim_end_matches('/')
                    )
                });
                let snapshot_bytes = fetch_remote_snapshot_bytes(
                    config,
                    &snapshot_url,
                    remote_bootstrap,
                    peer_transport_enabled,
                );
                let mut engine = SingleNodeEngine::from_snapshot_bytes(&snapshot_bytes)
                    .unwrap_or_else(|error| {
                        panic!("failed to load remote snapshot from {snapshot_url}: {error}")
                    });
                bind_engine_for_runtime(&mut engine, runtime_role, config, &snapshot_url);
                if bootstrap_empty_block_timestamp_ms.is_some() {
                    println!(
                        "kombat_chain ignoring bootstrap empty block timestamp because remote snapshot was loaded"
                    );
                }
                println!(
                    "kombat_chain bootstrapped snapshot from {} at height {} round {}",
                    remote_bootstrap.bootstrap_rpc_url,
                    metadata.head_height,
                    metadata.current_round
                );
                return engine;
            }
            let mut engine =
                SingleNodeEngine::new_with_validator_set(config.clone(), configured_validator_set);
            if matches!(runtime_role, RuntimeRole::Ingress) {
                engine.clear_local_validator_binding();
            }
            if let Some(timestamp_ms) = bootstrap_empty_block_timestamp_ms {
                assert!(
                    matches!(runtime_role, RuntimeRole::Validator),
                    "bootstrap empty block timestamp is not supported in ingress mode"
                );
                engine
                    .produce_empty_block(timestamp_ms)
                    .expect("bootstrap empty block should produce");
            }
            engine
        }
    }
}

fn validate_loaded_engine(engine: &SingleNodeEngine, config: &SingleNodeConfig) {
    assert_eq!(
        engine.chain_id(),
        config.chain_id,
        "snapshot chain_id does not match configured chain_id"
    );
    assert!(
        engine
            .configured_validator_ids()
            .contains(&config.validator_id),
        "snapshot validator set does not include configured validator_id"
    );
    let expected_consensus_pubkey = SigningKey::from_bytes(&config.consensus_signing_key)
        .verifying_key()
        .to_bytes();
    assert_eq!(
        engine.configured_validator_consensus_pubkey(config.validator_id),
        Some(expected_consensus_pubkey),
        "snapshot consensus signing key does not match configured validator_id"
    );
}

fn bind_engine_for_runtime(
    engine: &mut SingleNodeEngine,
    runtime_role: RuntimeRole,
    config: &SingleNodeConfig,
    source: &str,
) {
    if matches!(runtime_role, RuntimeRole::Validator) {
        validate_loaded_engine(engine, config);
        engine
            .rebind_local_validator(config.validator_id, config.consensus_signing_key)
            .unwrap_or_else(|error| {
                panic!("failed to rebind local validator from {source}: {error}")
            });
    } else {
        engine.clear_local_validator_binding();
    }
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    parse_optional_env_u64(name).unwrap_or(default)
}

fn parse_optional_env_u64(name: &str) -> Option<u64> {
    env::var(name).ok().map(|value| {
        value
            .parse::<u64>()
            .unwrap_or_else(|_| panic!("{name} must parse as u64"))
    })
}

fn parse_env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<u32>()
                .unwrap_or_else(|_| panic!("{name} must parse as u32"))
        })
        .unwrap_or(default)
}

fn parse_env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => panic!("{name} must parse as bool-like true/false"),
        })
        .unwrap_or(default)
}

fn parse_runtime_role() -> RuntimeRole {
    match env::var("KOMBAT_RUNTIME_ROLE")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        None | Some("validator") => RuntimeRole::Validator,
        Some("ingress") | Some("observer") => RuntimeRole::Ingress,
        Some(other) => panic!("KOMBAT_RUNTIME_ROLE must be validator or ingress, got {other}"),
    }
}

fn parse_env_hash32(name: &str, default: [u8; 32]) -> [u8; 32] {
    parse_optional_env_hash32(name).unwrap_or(default)
}

fn parse_optional_env_hash32(name: &str) -> Option<[u8; 32]> {
    env::var(name)
        .ok()
        .map(|value| decode_hex_array::<32>(&value, name))
}

fn decode_hex_array<const N: usize>(input: &str, name: &str) -> [u8; N] {
    let normalized = input.strip_prefix("0x").unwrap_or(input);
    let bytes = hex::decode(normalized).unwrap_or_else(|_| panic!("{name} must be valid hex"));
    bytes
        .try_into()
        .unwrap_or_else(|_| panic!("{name} must decode to {N} bytes"))
}

fn parse_auto_produce_config() -> Option<AutoProduceConfig> {
    let interval_ms = parse_optional_env_u64("KOMBAT_DEV_AUTO_PRODUCE_INTERVAL_MS")?;
    Some(AutoProduceConfig {
        interval_ms: interval_ms.max(1),
        max_items: parse_env_usize("KOMBAT_DEV_AUTO_PRODUCE_MAX_ITEMS", 100).clamp(1, 10_000),
    })
}

fn parse_auto_network_round_config() -> Option<AutoNetworkRoundConfig> {
    let interval_ms = parse_optional_env_u64("KOMBAT_DEV_AUTO_NETWORK_ROUND_INTERVAL_MS")?;
    let drive_interval_ms = parse_optional_env_u64("KOMBAT_DEV_AUTO_NETWORK_DRIVE_INTERVAL_MS")
        .unwrap_or(interval_ms.min(25))
        .max(1);
    Some(AutoNetworkRoundConfig {
        interval_ms: interval_ms.max(1),
        drive_interval_ms,
        max_items: parse_env_usize("KOMBAT_DEV_AUTO_NETWORK_MAX_ITEMS", 100).clamp(1, 10_000),
        start_at_unix_ms: parse_optional_env_u64("KOMBAT_DEV_AUTO_NETWORK_START_AT_UNIX_MS"),
    })
}

fn parse_peer_transport_keepalive_config(
    peer_transport_enabled: bool,
    network_bootstrap_enabled: bool,
) -> Option<PeerTransportKeepaliveConfig> {
    if !peer_transport_enabled || !network_bootstrap_enabled {
        return None;
    }
    if !parse_env_bool("KOMBAT_PEER_TRANSPORT_KEEPALIVE_ENABLED", true) {
        return None;
    }
    Some(PeerTransportKeepaliveConfig {
        interval_ms: parse_optional_env_u64("KOMBAT_PEER_TRANSPORT_KEEPALIVE_INTERVAL_MS")
            .unwrap_or(5_000)
            .max(100),
    })
}

fn peer_transport_enabled_for_network(
    peer_transport_enabled: bool,
    network_bootstrap_enabled: bool,
) -> bool {
    peer_transport_enabled && network_bootstrap_enabled
}

fn parse_remote_bootstrap_config() -> Option<RemoteBootstrapConfig> {
    let bootstrap_rpc_url = env::var("KOMBAT_BOOTSTRAP_RPC_URL").ok()?;
    Some(RemoteBootstrapConfig {
        bootstrap_rpc_url,
        bearer_token: env::var("KOMBAT_BOOTSTRAP_BEARER_TOKEN").ok(),
    })
}

fn parse_network_bootstrap_config(
    config: &SingleNodeConfig,
    runtime_role: RuntimeRole,
) -> Option<NetworkBootstrapConfig> {
    let path = env::var("KOMBAT_VALIDATOR_SET_PATH").ok()?;
    Some(match runtime_role {
        RuntimeRole::Validator => build_validator_network_bootstrap_config(
            config,
            &path,
            env::var("KOMBAT_PUBLIC_RPC_URL").ok(),
        ),
        RuntimeRole::Ingress => {
            build_ingress_network_bootstrap_config(&path, env::var("KOMBAT_PUBLIC_RPC_URL").ok())
        }
    })
}

fn build_validator_network_bootstrap_config(
    config: &SingleNodeConfig,
    validator_set_path: impl AsRef<Path>,
    public_rpc_url: Option<String>,
) -> NetworkBootstrapConfig {
    let validator_set = load_validator_set_from_path(&validator_set_path).unwrap_or_else(|error| {
        panic!(
            "failed to load validator set from {}: {error:?}",
            validator_set_path.as_ref().display()
        )
    });
    let bootstrap_peers = bootstrap_validator(&validator_set, config.validator_id)
        .unwrap_or_else(|error| panic!("failed to bootstrap local validator: {error:?}"));
    let expected_consensus_pubkey = SigningKey::from_bytes(&config.consensus_signing_key)
        .verifying_key()
        .to_bytes();
    assert_eq!(
        bootstrap_peers.local.consensus_pubkey.as_bytes(),
        expected_consensus_pubkey.as_slice(),
        "validator set consensus pubkey does not match configured signing key"
    );
    let local_validator = bootstrap_peers.local.clone();

    NetworkBootstrapConfig {
        validator_set,
        bootstrap_peers,
        local_validator: Some(local_validator),
        public_rpc_url,
    }
}

fn build_ingress_network_bootstrap_config(
    validator_set_path: impl AsRef<Path>,
    public_rpc_url: Option<String>,
) -> NetworkBootstrapConfig {
    let validator_set = load_validator_set_from_path(&validator_set_path).unwrap_or_else(|error| {
        panic!(
            "failed to load validator set from {}: {error:?}",
            validator_set_path.as_ref().display()
        )
    });
    let active_validators = validator_set.active_validators();
    let bootstrap_local = active_validators
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("validator set must contain at least one active validator"));
    let bootstrap_peers = kombat_chain_network::BootstrapPeers {
        local: bootstrap_local,
        peers: active_validators
            .iter()
            .map(kombat_chain_network::ValidatorPeer::from)
            .collect(),
    };

    NetworkBootstrapConfig {
        validator_set,
        bootstrap_peers,
        local_validator: None,
        public_rpc_url,
    }
}

fn fetch_remote_bootstrap_metadata(
    config: &SingleNodeConfig,
    remote_bootstrap: &RemoteBootstrapConfig,
    peer_transport_enabled: bool,
) -> RemoteBootstrapMetadata {
    retry_remote_bootstrap("bootstrap metadata", || {
        try_fetch_remote_bootstrap_metadata(config, remote_bootstrap, peer_transport_enabled)
    })
}

fn try_fetch_remote_bootstrap_metadata(
    config: &SingleNodeConfig,
    remote_bootstrap: &RemoteBootstrapConfig,
    peer_transport_enabled: bool,
) -> Result<RemoteBootstrapMetadata, String> {
    let mut peer_transport_error = None;
    if peer_transport_enabled {
        match fetch_remote_bootstrap_metadata_over_peer_transport(config, remote_bootstrap) {
            Ok(metadata) => return Ok(metadata),
            Err(error) => {
                eprintln!(
                    "kombat_chain peer transport metadata bootstrap failed for {}: {}; falling back to HTTP metadata fetch",
                    remote_bootstrap.bootstrap_rpc_url,
                    error
                );
                peer_transport_error = Some(error);
            }
        }
    }

    match fetch_remote_bootstrap_metadata_over_http(remote_bootstrap) {
        Ok(metadata) => Ok(metadata),
        Err(http_error) => match peer_transport_error {
            Some(peer_error) => Err(format!(
                "peer transport metadata fetch failed: {peer_error}; HTTP metadata fetch failed: {http_error}"
            )),
            None => Err(http_error),
        },
    }
}

fn fetch_remote_bootstrap_metadata_over_http(
    remote_bootstrap: &RemoteBootstrapConfig,
) -> Result<RemoteBootstrapMetadata, String> {
    let bootstrap_url = format!(
        "{}/network/bootstrap",
        remote_bootstrap.bootstrap_rpc_url.trim_end_matches('/')
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("bootstrap http client build failed: {error}"))?;
    let response = client
        .get(&bootstrap_url)
        .send()
        .map_err(|error| format!("failed to fetch remote bootstrap metadata: {error}"))?
        .error_for_status()
        .map_err(|error| format!("remote bootstrap metadata request failed: {error}"))?;
    response
        .json::<RemoteBootstrapMetadata>()
        .map_err(|error| format!("failed to decode remote bootstrap metadata: {error}"))
}

fn fetch_remote_bootstrap_metadata_over_peer_transport(
    config: &SingleNodeConfig,
    remote_bootstrap: &RemoteBootstrapConfig,
) -> Result<RemoteBootstrapMetadata, String> {
    let mut headers = BTreeMap::new();
    if let Some(token) = remote_bootstrap.bearer_token.as_deref() {
        headers.insert("authorization".into(), format!("Bearer {token}"));
    }
    let response = send_peer_transport_request(
        config,
        &remote_bootstrap.bootstrap_rpc_url,
        &PeerTransportRequest {
            path: "/network/bootstrap".into(),
            headers,
            body: String::new(),
        },
    )
    .map_err(|error| format!("failed to fetch remote bootstrap metadata: {error}"))?;
    if let Some(error) = response.error {
        return Err(format!(
            "remote bootstrap metadata request failed: {}",
            error.message
        ));
    }
    serde_json::from_str(
        response
            .body
            .as_deref()
            .ok_or_else(|| "remote bootstrap metadata response missing body".to_string())?,
    )
    .map_err(|error| format!("failed to decode remote bootstrap metadata: {error}"))
}

fn send_peer_transport_request(
    config: &SingleNodeConfig,
    endpoint: &str,
    request: &PeerTransportRequest,
) -> Result<PeerTransportResponse, String> {
    let url =
        reqwest::Url::parse(endpoint).map_err(|error| format!("invalid endpoint: {error}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| "endpoint missing host".to_string())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "endpoint missing port".to_string())?;
    let address = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&address)
        .map_err(|error| format!("connect {address} failed: {error}"))?;
    stream
        .set_nodelay(true)
        .map_err(|error| format!("set_nodelay failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| format!("set_read_timeout failed: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| format!("set_write_timeout failed: {error}"))?;
    stream
        .write_all(PEER_TRANSPORT_MAGIC)
        .map_err(|error| format!("write peer transport magic failed: {error}"))?;
    let session_open = PeerTransportRequest {
        path: "/network/session/open".into(),
        headers: BTreeMap::new(),
        body: serde_json::to_string(&PeerTransportSessionOpenRequest {
            chain_id: Some(config.chain_id),
            validator_id_hex: Some(format!("0x{}", hex::encode(config.validator_id))),
        })
        .map_err(|error| format!("peer transport session open encode failed: {error}"))?,
    };
    write_peer_transport_request(&mut stream, &session_open)?;
    let session_open_response = read_peer_transport_response(&mut stream)?;
    if let Some(error) = session_open_response.error {
        return Err(format!(
            "peer transport session open failed: {}",
            error.message
        ));
    }
    let open_body = session_open_response
        .body
        .as_deref()
        .ok_or_else(|| "peer transport session open response missing body".to_string())?;
    let _: PeerTransportSessionOpenResponse = serde_json::from_str(open_body)
        .map_err(|error| format!("invalid peer transport session open response: {error}"))?;
    write_peer_transport_request(&mut stream, request)?;
    read_peer_transport_response(&mut stream)
}

fn write_peer_transport_request(
    stream: &mut TcpStream,
    request: &PeerTransportRequest,
) -> Result<(), String> {
    let payload = serde_json::to_vec(request)
        .map_err(|error| format!("peer transport request serialization failed: {error}"))?;
    if payload.len() > MAX_PEER_TRANSPORT_FRAME_BYTES {
        return Err("peer transport request too large".into());
    }
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .map_err(|error| format!("write request length failed: {error}"))?;
    stream
        .write_all(&payload)
        .map_err(|error| format!("write request payload failed: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("flush request failed: {error}"))
}

fn read_peer_transport_response(stream: &mut TcpStream) -> Result<PeerTransportResponse, String> {
    let mut length_bytes = [0_u8; 4];
    read_peer_transport_exact(stream, &mut length_bytes, "read response length")?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length > MAX_PEER_TRANSPORT_FRAME_BYTES {
        return Err("peer transport response too large".into());
    }
    let mut payload = vec![0_u8; length];
    read_peer_transport_exact(stream, &mut payload, "read response payload")?;
    serde_json::from_slice(&payload)
        .map_err(|error| format!("invalid peer transport response: {error}"))
}

fn read_peer_transport_exact(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    label: &str,
) -> Result<(), String> {
    let started_at = Instant::now();
    let timeout = Duration::from_millis(PEER_TRANSPORT_FRAME_READ_TIMEOUT_MS);
    let mut offset = 0;

    while offset < buffer.len() {
        match stream.read(&mut buffer[offset..]) {
            Ok(0) => return Err(format!("{label} failed: unexpected EOF")),
            Ok(read) => offset += read,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut
                ) =>
            {
                if started_at.elapsed() >= timeout {
                    return Err(format!("{label} timed out"));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(format!("{label} failed: {error}")),
        }
    }

    Ok(())
}

fn fetch_remote_snapshot_bytes(
    config: &SingleNodeConfig,
    snapshot_url: &str,
    remote_bootstrap: &RemoteBootstrapConfig,
    peer_transport_enabled: bool,
) -> Vec<u8> {
    retry_remote_bootstrap("snapshot", || {
        try_fetch_remote_snapshot_bytes(
            config,
            snapshot_url,
            remote_bootstrap,
            peer_transport_enabled,
        )
    })
}

fn try_fetch_remote_snapshot_bytes(
    config: &SingleNodeConfig,
    snapshot_url: &str,
    remote_bootstrap: &RemoteBootstrapConfig,
    peer_transport_enabled: bool,
) -> Result<Vec<u8>, String> {
    let mut peer_transport_error = None;
    if peer_transport_enabled {
        match fetch_remote_snapshot_bytes_over_peer_transport(config, remote_bootstrap) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => {
                eprintln!(
                    "kombat_chain peer transport snapshot bootstrap failed for {}: {}; falling back to HTTP snapshot fetch",
                    remote_bootstrap.bootstrap_rpc_url,
                    error
                );
                peer_transport_error = Some(error);
            }
        }
    }

    match fetch_remote_snapshot_bytes_over_http(snapshot_url, remote_bootstrap) {
        Ok(snapshot) => Ok(snapshot),
        Err(http_error) => match peer_transport_error {
            Some(peer_error) => Err(format!(
                "peer transport snapshot fetch failed: {peer_error}; HTTP snapshot fetch failed: {http_error}"
            )),
            None => Err(http_error),
        },
    }
}

fn fetch_remote_snapshot_bytes_over_http(
    snapshot_url: &str,
    remote_bootstrap: &RemoteBootstrapConfig,
) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("snapshot http client build failed: {error}"))?;
    let mut request = client.get(snapshot_url);
    if let Some(token) = remote_bootstrap.bearer_token.as_deref() {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .map_err(|error| format!("failed to fetch remote snapshot: {error}"))?
        .error_for_status()
        .map_err(|error| format!("remote snapshot request failed: {error}"))?;
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("failed to read remote snapshot bytes: {error}"))
}

fn fetch_remote_snapshot_bytes_over_peer_transport(
    config: &SingleNodeConfig,
    remote_bootstrap: &RemoteBootstrapConfig,
) -> Result<Vec<u8>, String> {
    let mut headers = BTreeMap::new();
    if let Some(token) = remote_bootstrap.bearer_token.as_deref() {
        headers.insert("authorization".into(), format!("Bearer {token}"));
    }
    let response = send_peer_transport_request(
        config,
        &remote_bootstrap.bootstrap_rpc_url,
        &PeerTransportRequest {
            path: "/network/snapshot".into(),
            headers,
            body: String::new(),
        },
    )
    .map_err(|error| format!("failed to fetch remote snapshot: {error}"))?;
    if let Some(error) = response.error {
        return Err(format!("remote snapshot request failed: {}", error.message));
    }
    BASE64_STANDARD
        .decode(
            response
                .body
                .as_deref()
                .ok_or_else(|| "remote snapshot response missing body".to_string())?,
        )
        .map_err(|error| format!("failed to decode remote snapshot bytes: {error}"))
}

fn retry_remote_bootstrap<T, F>(label: &str, mut fetch: F) -> T
where
    F: FnMut() -> Result<T, String>,
{
    for attempt in 1..=REMOTE_BOOTSTRAP_ATTEMPTS {
        match fetch() {
            Ok(value) => return value,
            Err(error) if attempt == REMOTE_BOOTSTRAP_ATTEMPTS => {
                panic!(
                    "failed to fetch remote {label} after {REMOTE_BOOTSTRAP_ATTEMPTS} attempts: {error}"
                );
            }
            Err(error) => {
                eprintln!(
                    "kombat_chain remote {label} bootstrap attempt {attempt}/{REMOTE_BOOTSTRAP_ATTEMPTS} failed: {error}; retrying"
                );
                thread::sleep(Duration::from_millis(
                    REMOTE_BOOTSTRAP_RETRY_DELAY_MS + u64::from(process::id() % 1_000),
                ));
            }
        }
    }

    unreachable!("remote bootstrap retry loop always returns or panics")
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("{name} must parse as usize"))
        })
        .unwrap_or(default)
}

fn spawn_auto_producer(engine: Arc<EngineHandle>, config: AutoProduceConfig) {
    thread::spawn(move || {
        let sleep_interval = Duration::from_millis(config.interval_ms);
        let mut next_timestamp_ms = engine
            .head_snapshot()
            .head_timestamp_ms
            .saturating_add(config.interval_ms);

        loop {
            thread::sleep(sleep_interval);

            match engine.produce_block_from_mempool(next_timestamp_ms, config.max_items) {
                Ok(None) => continue,
                Ok(Some(Ok(block))) => {
                    println!(
                        "kombat_chain auto-produced block height={} items={}",
                        block.height, block.item_count
                    );
                    next_timestamp_ms = block.timestamp_ms.saturating_add(config.interval_ms);
                }
                Ok(Some(Err(error))) => {
                    eprintln!("kombat_chain auto-produce error: {error:?}");
                    next_timestamp_ms = next_timestamp_ms.saturating_add(config.interval_ms);
                }
                Err(_) => break,
            }
        }
    });
}

fn spawn_auto_mahi_round(server: JsonRpcHttpServer, config: AutoNetworkRoundConfig) {
    thread::spawn(move || {
        let sleep_interval = Duration::from_millis(config.drive_interval_ms);
        let mut next_wake = initial_auto_network_wake(config, sleep_interval);
        let mut next_timestamp_ms = server
            .engine()
            .head_snapshot()
            .head_timestamp_ms
            .saturating_add(config.interval_ms);

        loop {
            let now = Instant::now();
            if now < next_wake {
                thread::sleep(next_wake.duration_since(now));
            }
            next_wake = next_wake
                .checked_add(sleep_interval)
                .unwrap_or_else(Instant::now);
            let Some(scheduled_timestamp_ms) = rebase_auto_network_round_timestamp(
                next_timestamp_ms,
                server.engine().head_snapshot().head_timestamp_ms,
                config.interval_ms,
                unix_now_ms(),
            ) else {
                continue;
            };
            match server.maybe_run_mahi_round(scheduled_timestamp_ms, config.max_items) {
                Ok(Some((round, item_count, block_count))) => {
                    println!(
                        "kombat_chain auto-mahi-round produced round={} items={} block_count={}",
                        round, item_count, block_count
                    );
                    next_timestamp_ms = scheduled_timestamp_ms.saturating_add(config.interval_ms);
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("kombat_chain auto-mahi-round error: {error}");
                }
            }
        }
    });
}

fn initial_auto_network_wake(config: AutoNetworkRoundConfig, default_delay: Duration) -> Instant {
    let now = Instant::now();
    let Some(start_at_unix_ms) = config.start_at_unix_ms else {
        return now + default_delay;
    };
    let now_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    if start_at_unix_ms <= now_unix_ms {
        return now + default_delay;
    }
    now + Duration::from_millis(start_at_unix_ms - now_unix_ms)
}

fn rebase_auto_network_round_timestamp(
    scheduled_timestamp_ms: u64,
    head_timestamp_ms: u64,
    interval_ms: u64,
    wall_timestamp_ms: u64,
) -> Option<u64> {
    let next_head_timestamp_ms = head_timestamp_ms.saturating_add(interval_ms);
    let candidate = scheduled_timestamp_ms.max(next_head_timestamp_ms);
    if candidate > wall_timestamp_ms {
        return None;
    }
    Some(candidate)
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn spawn_peer_transport_keepalive(server: JsonRpcHttpServer, config: PeerTransportKeepaliveConfig) {
    thread::spawn(move || {
        let sleep_interval = Duration::from_millis(config.interval_ms);
        loop {
            thread::sleep(sleep_interval);
            let _ = server.maintain_peer_transport_sessions();
        }
    });
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
