use super::*;

pub(crate) fn migrate_state_report(
    started_at_ms: u64,
    result: &str,
    dry_run: bool,
    source_fingerprint: Option<[u8; 32]>,
    migration_path: Vec<String>,
    error: Option<String>,
) -> MigrateStateReport {
    MigrateStateReport {
        schema: "kombat.persistence_migration_report.v1".into(),
        binary_git_sha: migration_binary_git_sha(),
        started_at_ms,
        finished_at_ms: unix_time_ms(),
        result: result.into(),
        dry_run,
        source_fingerprint: source_fingerprint.map(|fingerprint| fingerprint_hex(&fingerprint)),
        target_fingerprint: fingerprint_hex(&migrations::current_binary_persistence_fingerprint()),
        migration_path,
        source: None,
        target: None,
        records: MigrateStateRecordsReport::default(),
        namespaces: BTreeMap::new(),
        validations: MigrateStateValidationsReport::default(),
        backup_path: None,
        staged_path: None,
        error,
    }
}

pub(crate) fn migration_binary_git_sha() -> String {
    option_env!("KOMBAT_GIT_SHA")
        .or(option_env!("GIT_SHA"))
        .or(option_env!("VERGEN_GIT_SHA"))
        .unwrap_or("unknown")
        .to_string()
}

pub(crate) fn execute_backend_migration(
    started_at_ms: u64,
    parsed: &MigrateStateArgs,
    source_db_path: &Path,
    source_backend: RedbBackend,
    source_fingerprint: [u8; 32],
    path: &[&migrations::PersistenceMigration],
    path_ids: Vec<String>,
    source_inventory: Option<MigrateStateInventoryReport>,
) -> (i32, MigrateStateReport) {
    let target_fingerprint = source_backend.expected_persistence_fingerprint();
    let backup_path =
        migration_artifact_path(source_db_path, "backup", started_at_ms, &source_fingerprint);
    let staged_path = migration_artifact_path(
        source_db_path,
        "migrating",
        started_at_ms,
        &target_fingerprint,
    );

    let mut report = migrate_state_report(
        started_at_ms,
        if parsed.dry_run {
            "migration_dry_run_validated"
        } else {
            "migration_completed"
        },
        parsed.dry_run,
        Some(source_fingerprint),
        path_ids,
        None,
    );
    report.source = source_inventory;
    report.backup_path = Some(backup_path.display().to_string());
    report.staged_path = Some(staged_path.display().to_string());

    if path.iter().any(|migration| {
        migration.backend_strategy == migrations::BackendMigrationStrategy::NotImplemented
    }) {
        report.result = "migration_implementation_missing".into();
        report.error = Some(
            "migration path contains an edge without a backend execution implementation".into(),
        );
        return (12, report);
    }
    if staged_path.exists() || backup_path.exists() {
        report.result = "target_path_exists".into();
        report.error = Some(format!(
            "staged or backup path already exists: staged={}, backup={}",
            staged_path.display(),
            backup_path.display()
        ));
        return (15, report);
    }

    let target_backend = match RedbBackend::open(
        OpenOptions::new(staged_path.clone()).with_persistence_fingerprint(target_fingerprint),
    ) {
        Ok(backend) => backend,
        Err(error) => {
            report.result = "target_open_failed".into();
            report.error = Some(error.to_string());
            return (12, report);
        }
    };

    if let Err(error) = migrate_backend_namespaces(&source_backend, &target_backend, path) {
        report.result = "target_write_failed".into();
        let mut details = vec![error];
        drop(target_backend);
        if let Some(cleanup_error) = cleanup_staged_path(&staged_path) {
            details.push(cleanup_error);
        }
        report.error = Some(details.join("; "));
        return (12, report);
    }

    let target_inventory = match validate_staged_backend(
        &source_backend,
        &target_backend,
        report.source.as_ref(),
        target_fingerprint,
        path,
    ) {
        Ok(inventory) => inventory,
        Err(error) => {
            report.result = "migration_validation_failed".into();
            let mut details = vec![error];
            drop(target_backend);
            if let Some(cleanup_error) = cleanup_staged_path(&staged_path) {
                details.push(cleanup_error);
            }
            report.error = Some(details.join("; "));
            return (12, report);
        }
    };
    report.target = Some(target_inventory);
    refresh_migrate_state_audit_fields(&mut report, path);

    drop(target_backend);
    drop(source_backend);

    if parsed.dry_run {
        if let Err(error) = fs::remove_file(&staged_path) {
            report.result = "staged_cleanup_failed".into();
            report.error = Some(error.to_string());
            return (12, report);
        }
        return (0, report);
    }

    if let Err(error) = fs::rename(source_db_path, &backup_path) {
        report.result = "backup_swap_failed".into();
        report.error = Some(error.to_string());
        return (12, report);
    }
    if let Err(error) = fs::rename(&staged_path, source_db_path) {
        let rollback_error = fs::rename(&backup_path, source_db_path).err();
        let staged_cleanup_error = fs::remove_file(&staged_path).err();
        report.result = "live_swap_failed".into();
        let mut details = vec![error.to_string()];
        if let Some(rollback_error) = rollback_error {
            details.push(format!(
                "rollback from backup also failed: {rollback_error}"
            ));
        }
        if let Some(staged_cleanup_error) = staged_cleanup_error {
            details.push(format!(
                "staged cleanup also failed: {staged_cleanup_error}"
            ));
        }
        report.error = Some(details.join("; "));
        return (12, report);
    }

    (0, report)
}

pub(crate) fn cleanup_staged_path(staged_path: &Path) -> Option<String> {
    match fs::remove_file(staged_path) {
        Ok(()) => None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => Some(format!("staged cleanup also failed: {error}")),
    }
}

pub(crate) fn migrate_backend_namespaces(
    source_backend: &RedbBackend,
    target_backend: &RedbBackend,
    path: &[&migrations::PersistenceMigration],
) -> Result<(), String> {
    let source_meta = source_backend
        .latest_committed_meta()
        .map_err(|error| format!("read source commit meta: {error}"))?
        .ok_or_else(|| "source backend has no committed meta".to_string())?;
    let source_txn = source_backend
        .read_txn()
        .map_err(|error| format!("read source txn: {error}"))?;
    let mut batch = target_backend.batch();

    for namespace in Namespace::ALL {
        if *namespace == Namespace::Meta {
            continue;
        }
        let iter = source_txn.iter_prefix(*namespace, b"").map_err(|error| {
            format!(
                "iterate source namespace {}: {error}",
                namespace.stable_name()
            )
        })?;
        for entry in iter {
            let (key, value) = entry.map_err(|error| {
                format!("read source namespace {}: {error}", namespace.stable_name())
            })?;
            let mut migrated_value = value;
            for migration in path {
                migrated_value = migrations::migrate_backend_record(
                    migration,
                    *namespace,
                    &key,
                    &migrated_value,
                )?;
            }
            batch.put(*namespace, &key, &migrated_value);
        }
    }

    target_backend
        .commit_batch(batch, source_meta, SyncMode::Sync)
        .map_err(|error| format!("commit staged backend: {error}"))
}

pub(crate) fn validate_staged_backend(
    source_backend: &RedbBackend,
    target_backend: &RedbBackend,
    source_inventory: Option<&MigrateStateInventoryReport>,
    target_fingerprint: [u8; 32],
    path: &[&migrations::PersistenceMigration],
) -> Result<MigrateStateInventoryReport, String> {
    match target_backend
        .check_persistence_fingerprint()
        .map_err(|error| format!("check staged fingerprint: {error}"))?
    {
        kombat_chain_state_db::PersistenceFingerprintCheck::Compatible { fingerprint }
            if fingerprint == target_fingerprint => {}
        other => {
            return Err(format!(
                "staged fingerprint invalid: {}",
                other
                    .error_reason()
                    .unwrap_or_else(|| "unexpected compatible fingerprint".into())
            ));
        }
    }

    let source_meta = source_backend
        .latest_committed_meta()
        .map_err(|error| format!("read source commit meta: {error}"))?;
    let target_meta = target_backend
        .latest_committed_meta()
        .map_err(|error| format!("read staged commit meta: {error}"))?;
    if source_meta != target_meta {
        return Err("staged commit meta differs from source commit meta".into());
    }
    let staged = AnyStateBackend::Redb(target_backend.clone());
    SingleNodeEngine::from_state_backend(&staged)
        .map_err(|error| format!("hydrate staged backend: {error}"))?;

    let target_inventory = collect_backend_inventory(target_backend)?;
    if let Some(source_inventory) = source_inventory {
        validate_migrated_inventory(source_inventory, &target_inventory, path)?;
    }
    Ok(target_inventory)
}

pub(crate) fn validate_migrated_inventory(
    source: &MigrateStateInventoryReport,
    target: &MigrateStateInventoryReport,
    path: &[&migrations::PersistenceMigration],
) -> Result<(), String> {
    if source.total_records != target.total_records {
        return Err(format!(
            "record count mismatch: source={}, target={}",
            source.total_records, target.total_records
        ));
    }
    if source.total_key_bytes != target.total_key_bytes {
        return Err(format!(
            "key byte count mismatch: source={}, target={}",
            source.total_key_bytes, target.total_key_bytes
        ));
    }
    let value_migrated_namespaces = value_migrated_namespaces_for_path(path);
    if value_migrated_namespaces.is_empty() && source.total_value_bytes != target.total_value_bytes
    {
        return Err(format!(
            "value byte count mismatch: source={}, target={}",
            source.total_value_bytes, target.total_value_bytes
        ));
    }

    for namespace in Namespace::ALL {
        let name = namespace.stable_name();
        let source_ns = source
            .namespaces
            .get(name)
            .ok_or_else(|| format!("source inventory missing namespace {name}"))?;
        let target_ns = target
            .namespaces
            .get(name)
            .ok_or_else(|| format!("target inventory missing namespace {name}"))?;
        if source_ns.source_records != target_ns.source_records {
            return Err(format!("namespace {name} record count mismatch"));
        }
        if source_ns.source_key_bytes != target_ns.source_key_bytes {
            return Err(format!("namespace {name} key byte count mismatch"));
        }
        if source_ns.source_key_hash != target_ns.source_key_hash {
            return Err(format!("namespace {name} key hash mismatch"));
        }
        let value_migrated = value_migrated_namespaces.contains(namespace);
        // The Meta namespace is committed by the target backend itself:
        // latest CommitMeta is preserved, while the RFC 055 fingerprint
        // marker is intentionally rewritten to the target fingerprint.
        if *namespace != Namespace::Meta && !value_migrated {
            if source_ns.source_value_bytes != target_ns.source_value_bytes {
                return Err(format!("namespace {name} value byte count mismatch"));
            }
            if source_ns.source_value_hash != target_ns.source_value_hash {
                return Err(format!("namespace {name} value hash mismatch"));
            }
        }
    }

    Ok(())
}

pub(crate) fn refresh_migrate_state_audit_fields(
    report: &mut MigrateStateReport,
    path: &[&migrations::PersistenceMigration],
) {
    let (Some(source), Some(target)) = (&report.source, &report.target) else {
        return;
    };

    let value_migrated_namespaces = value_migrated_namespaces_for_path(path);
    let mut records = MigrateStateRecordsReport::default();
    let mut namespaces = BTreeMap::new();

    for namespace in Namespace::ALL {
        let name = namespace.stable_name();
        let Some(source_ns) = source.namespaces.get(name) else {
            continue;
        };
        let Some(target_ns) = target.namespaces.get(name) else {
            continue;
        };

        let value_migrated = value_migrated_namespaces.contains(namespace);
        let classification = if *namespace == Namespace::Meta {
            records.copied = records.copied.saturating_add(target_ns.source_records);
            "rebuilt"
        } else if value_migrated {
            records.migrated = records.migrated.saturating_add(target_ns.source_records);
            "mixed"
        } else {
            records.copied = records.copied.saturating_add(target_ns.source_records);
            "copied"
        };
        if source_ns.source_records > target_ns.source_records {
            records.skipped = records
                .skipped
                .saturating_add(source_ns.source_records - target_ns.source_records);
        }

        namespaces.insert(
            name.to_string(),
            MigrateStateNamespaceAuditReport {
                source_records: source_ns.source_records,
                target_records: target_ns.source_records,
                classification: classification.into(),
                source_key_hash: source_ns.source_key_hash.clone(),
                target_key_hash: target_ns.source_key_hash.clone(),
                copied_value_hash: if value_migrated {
                    None
                } else {
                    Some(target_ns.source_value_hash.clone())
                },
            },
        );
    }

    report.records = records;
    report.namespaces = namespaces;
    report.validations = MigrateStateValidationsReport::passed_backend_hydration();
}

pub(crate) fn value_migrated_namespaces_for_path(
    path: &[&migrations::PersistenceMigration],
) -> Vec<Namespace> {
    let mut namespaces = Vec::new();
    for migration in path {
        for namespace in migrations::value_migrated_namespaces(migration) {
            if !namespaces.contains(namespace) {
                namespaces.push(*namespace);
            }
        }
    }
    namespaces
}

pub(crate) fn migration_artifact_path(
    source_db_path: &Path,
    kind: &str,
    timestamp_ms: u64,
    fingerprint: &[u8; 32],
) -> PathBuf {
    let file_name = source_db_path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "state.redb".into());
    let short_fingerprint = hex::encode(&fingerprint[..4]);
    source_db_path.with_file_name(format!(
        "{file_name}.{kind}.{timestamp_ms}.{short_fingerprint}"
    ))
}

pub(crate) fn backend_migration_strategy_name(
    strategy: migrations::BackendMigrationStrategy,
) -> &'static str {
    match strategy {
        migrations::BackendMigrationStrategy::CopyAllNamespaces => "copy_all_namespaces",
        migrations::BackendMigrationStrategy::NotImplemented => "not_implemented",
        migrations::BackendMigrationStrategy::Rfc056PolicyAuthorityScope => {
            "rfc056_policy_authority_scope"
        }
    }
}

pub(crate) fn collect_backend_inventory<B: StateBackend>(
    backend: &B,
) -> Result<MigrateStateInventoryReport, String> {
    let meta = backend
        .latest_committed_meta()
        .map_err(|error| format!("read inventory commit meta: {error}"))?;
    let txn = backend
        .read_txn()
        .map_err(|error| format!("read source inventory txn: {error}"))?;
    let mut namespaces = BTreeMap::new();
    let mut total_records = 0u64;
    let mut total_key_bytes = 0u64;
    let mut total_value_bytes = 0u64;

    for namespace in Namespace::ALL {
        let namespace_report = collect_namespace_inventory(&txn, *namespace)?;
        total_records = total_records
            .checked_add(namespace_report.source_records)
            .ok_or_else(|| "source inventory record count overflow".to_string())?;
        total_key_bytes = total_key_bytes
            .checked_add(namespace_report.source_key_bytes)
            .ok_or_else(|| "source inventory key byte count overflow".to_string())?;
        total_value_bytes = total_value_bytes
            .checked_add(namespace_report.source_value_bytes)
            .ok_or_else(|| "source inventory value byte count overflow".to_string())?;
        namespaces.insert(namespace.stable_name().to_string(), namespace_report);
    }

    Ok(MigrateStateInventoryReport {
        head_height: meta.as_ref().map(|meta| meta.height),
        head_hash: meta.as_ref().map(|meta| fingerprint_hex(&meta.block_hash)),
        chain_head_state_root: meta
            .as_ref()
            .map(|meta| fingerprint_hex(&meta.chain_head_state_root)),
        executor_state_root: meta
            .as_ref()
            .map(|meta| fingerprint_hex(&meta.executor_state_root)),
        event_sequence: meta.as_ref().map(|meta| meta.event_seq),
        total_records,
        total_key_bytes,
        total_value_bytes,
        namespaces,
    })
}

pub(crate) fn collect_namespace_inventory<R: ReadTxn>(
    txn: &R,
    namespace: Namespace,
) -> Result<MigrateStateNamespaceReport, String> {
    let mut source_records = 0u64;
    let mut source_key_bytes = 0u64;
    let mut source_value_bytes = 0u64;
    let mut key_hasher = Sha256::new();
    let mut value_hasher = Sha256::new();

    let iter = txn
        .iter_prefix(namespace, b"")
        .map_err(|error| format!("iterate namespace {}: {error}", namespace.stable_name()))?;
    for entry in iter {
        let (key, value) = entry
            .map_err(|error| format!("read namespace {}: {error}", namespace.stable_name()))?;
        source_records = source_records.checked_add(1).ok_or_else(|| {
            format!(
                "namespace {} record count overflow",
                namespace.stable_name()
            )
        })?;
        source_key_bytes = source_key_bytes
            .checked_add(key.len() as u64)
            .ok_or_else(|| {
                format!(
                    "namespace {} key byte count overflow",
                    namespace.stable_name()
                )
            })?;
        source_value_bytes = source_value_bytes
            .checked_add(value.len() as u64)
            .ok_or_else(|| {
                format!(
                    "namespace {} value byte count overflow",
                    namespace.stable_name()
                )
            })?;
        hash_len_prefixed(&mut key_hasher, &key);
        hash_len_prefixed(&mut value_hasher, &value);
    }

    Ok(MigrateStateNamespaceReport {
        source_records,
        source_key_bytes,
        source_value_bytes,
        source_key_hash: format!("0x{}", hex::encode(key_hasher.finalize())),
        source_value_hash: format!("0x{}", hex::encode(value_hasher.finalize())),
    })
}

pub(crate) fn hash_len_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

pub(crate) fn render_migrate_state_output(
    exit_code: i32,
    mut report: MigrateStateReport,
    format: MaintenanceOutputFormat,
    report_path: Option<&PathBuf>,
) -> (i32, String) {
    report.finished_at_ms = unix_time_ms();
    if let Some(path) = report_path {
        if let Err(error) = fs::write(
            path,
            serde_json::to_string_pretty(&report).expect("migration report should serialize"),
        ) {
            let mut failed_report = report.clone();
            failed_report.result = "report_write_failed".into();
            failed_report.error = Some(error.to_string());
            return render_migrate_state_output(exit_code.max(12), failed_report, format, None);
        }
    }

    let rendered = match format {
        MaintenanceOutputFormat::Json => {
            serde_json::to_string_pretty(&report).expect("migration report should serialize")
        }
        MaintenanceOutputFormat::Text => match report.error.as_deref() {
            Some(error) => format!("{}: {error}", report.result),
            None => report.result.clone(),
        },
    };
    (exit_code, rendered)
}

pub(crate) fn fingerprint_hex(fingerprint: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(fingerprint))
}
