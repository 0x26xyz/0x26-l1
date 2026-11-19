use super::*;

#[path = "main_maintenance_migration.rs"]
mod main_maintenance_migration;

pub(crate) use main_maintenance_migration::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaintenanceOutputFormat {
    Text,
    Json,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckPersistenceFingerprintArgs {
    pub(crate) data_dir: PathBuf,
    pub(crate) format: MaintenanceOutputFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MigrateStateArgs {
    pub(crate) data_dir: PathBuf,
    pub(crate) format: MaintenanceOutputFormat,
    pub(crate) dry_run: bool,
    pub(crate) confirm: Option<String>,
    pub(crate) report_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MigrateSnapshotArgs {
    pub(crate) input_path: PathBuf,
    pub(crate) output_path: PathBuf,
    pub(crate) format: MaintenanceOutputFormat,
    pub(crate) report_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepairMarketCustodyArgs {
    data_dir: PathBuf,
    format: MaintenanceOutputFormat,
    dry_run: bool,
    confirm: Option<String>,
    report_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuditMarketCustodyArgs {
    data_dir: PathBuf,
    format: MaintenanceOutputFormat,
    top_rows: usize,
    report_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepairIsolatedCollateralArgs {
    data_dir: PathBuf,
    format: MaintenanceOutputFormat,
    dry_run: bool,
    confirm: Option<String>,
    top_rows: usize,
    targets: Vec<IsolatedCollateralRepairTarget>,
    report_path: Option<PathBuf>,
}

#[derive(Serialize)]
pub(crate) struct CheckPersistenceFingerprintOutput {
    pub(crate) result: String,
    pub(crate) binary_fingerprint: String,
    pub(crate) on_disk_fingerprint: Option<String>,
    pub(crate) envelope_version: Option<u32>,
    pub(crate) error: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ListPersistenceMigrationsOutput {
    schema: String,
    binary_fingerprint: String,
    registry_valid: bool,
    migration_count: usize,
    migrations: Vec<PersistenceMigrationOutput>,
    error: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct PersistenceMigrationOutput {
    id: String,
    source_fingerprint: String,
    target_fingerprint: String,
    description: String,
    changed_records: Vec<String>,
    backend_strategy: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct MigrateStateReport {
    schema: String,
    binary_git_sha: String,
    started_at_ms: u64,
    finished_at_ms: u64,
    result: String,
    dry_run: bool,
    source_fingerprint: Option<String>,
    target_fingerprint: String,
    migration_path: Vec<String>,
    source: Option<MigrateStateInventoryReport>,
    target: Option<MigrateStateInventoryReport>,
    records: MigrateStateRecordsReport,
    namespaces: BTreeMap<String, MigrateStateNamespaceAuditReport>,
    validations: MigrateStateValidationsReport,
    backup_path: Option<String>,
    staged_path: Option<String>,
    error: Option<String>,
}

#[derive(Clone, Default, Serialize)]
pub(crate) struct MigrateStateRecordsReport {
    copied: u64,
    migrated: u64,
    skipped: u64,
    warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct MigrateStateNamespaceAuditReport {
    source_records: u64,
    target_records: u64,
    classification: String,
    source_key_hash: String,
    target_key_hash: String,
    copied_value_hash: Option<String>,
}

#[derive(Clone, Default, Serialize)]
pub(crate) struct MigrateStateValidationsReport {
    hydrated: bool,
    snapshot_validators: bool,
    wal_replay_inputs: bool,
    runtime_event_sequence: bool,
    receipts: bool,
    quarantine: bool,
    telemetry: bool,
}

impl MigrateStateValidationsReport {
    fn passed_backend_hydration() -> Self {
        Self {
            hydrated: true,
            snapshot_validators: true,
            wal_replay_inputs: true,
            runtime_event_sequence: true,
            receipts: true,
            quarantine: true,
            telemetry: true,
        }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct MigrateStateInventoryReport {
    head_height: Option<u64>,
    head_hash: Option<String>,
    chain_head_state_root: Option<String>,
    executor_state_root: Option<String>,
    event_sequence: Option<u64>,
    total_records: u64,
    total_key_bytes: u64,
    total_value_bytes: u64,
    namespaces: BTreeMap<String, MigrateStateNamespaceReport>,
}

#[derive(Clone, Serialize)]
pub(crate) struct MigrateStateNamespaceReport {
    source_records: u64,
    source_key_bytes: u64,
    source_value_bytes: u64,
    source_key_hash: String,
    source_value_hash: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct RepairMarketCustodyReport {
    schema: String,
    started_at_ms: u64,
    finished_at_ms: u64,
    result: String,
    dry_run: bool,
    adjustments: Vec<RepairMarketCustodyAdjustmentReport>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct RepairMarketCustodyAdjustmentReport {
    market_id: u64,
    custody_balance_micro_usdc: String,
    required_before_micro_usdc: String,
    treasury_before_micro_usdc: String,
    treasury_after_micro_usdc: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct AuditMarketCustodyReport {
    schema: String,
    started_at_ms: u64,
    finished_at_ms: u64,
    result: String,
    markets: Vec<AuditMarketCustodyMarketReport>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct AuditMarketCustodyMarketReport {
    market_id: u64,
    custody_balance_micro_usdc: Option<String>,
    required_balance_micro_usdc: Option<String>,
    custody_surplus_micro_usdc: Option<String>,
    required_error: Option<String>,
    positive_isolated_collateral_micro_usdc: String,
    negative_isolated_collateral_micro_usdc: String,
    fee_vault_micro_usdc: String,
    insurance_fund_micro_usdc: String,
    validator_reward_pool_micro_usdc: String,
    market_maker_rebate_pool_micro_usdc: String,
    treasury_micro_usdc: String,
    top_isolated_collateral: Vec<AuditIsolatedCollateralRowReport>,
}

#[derive(Clone, Serialize)]
pub(crate) struct AuditIsolatedCollateralRowReport {
    market_id: u64,
    account_id: u64,
    collateral_micro_usdc: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct RepairIsolatedCollateralReport {
    schema: String,
    started_at_ms: u64,
    finished_at_ms: u64,
    result: String,
    dry_run: bool,
    collateral_adjustments: Vec<RepairIsolatedCollateralAdjustmentReport>,
    custody_adjustments: Vec<RepairMarketCustodyAdjustmentReport>,
    after_audit: AuditMarketCustodyReport,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
pub(crate) struct RepairIsolatedCollateralAdjustmentReport {
    market_id: u64,
    account_id: u64,
    before_micro_usdc: String,
    after_micro_usdc: String,
}

pub(crate) fn maybe_run_maintenance_command() -> Option<i32> {
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        return None;
    }
    let command = args[1].as_str();
    let subargs = &args[2..];
    match command {
        "check-persistence-fingerprint" => {
            let (exit_code, output) = run_check_persistence_fingerprint_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        "list-persistence-migrations" => {
            let (exit_code, output) = run_list_persistence_migrations_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        "migrate-state" => {
            let (exit_code, output) = run_migrate_state_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        "migrate-snapshot" => {
            let (exit_code, output) = run_migrate_snapshot_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        "repair-market-custody" => {
            let (exit_code, output) = run_repair_market_custody_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        "audit-market-custody" => {
            let (exit_code, output) = run_audit_market_custody_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        "repair-isolated-collateral" => {
            let (exit_code, output) = run_repair_isolated_collateral_command(subargs);
            println!("{output}");
            Some(exit_code)
        }
        _ => None,
    }
}

pub(crate) fn run_list_persistence_migrations_command(args: &[String]) -> (i32, String) {
    let format = match parse_format_only_args(args) {
        Ok(format) => format,
        Err(error) => {
            let output = ListPersistenceMigrationsOutput {
                schema: "kombat.persistence_migration_registry.v1".into(),
                binary_fingerprint: fingerprint_hex(
                    &migrations::current_binary_persistence_fingerprint(),
                ),
                registry_valid: false,
                migration_count: 0,
                migrations: Vec::new(),
                error: Some(error),
            };
            return render_list_migrations_output(4, output, MaintenanceOutputFormat::Json);
        }
    };

    let registry = migrations::compiled_persistence_migrations();
    let registry_error = migrations::validate_compiled_persistence_migrations(registry).err();
    let output = ListPersistenceMigrationsOutput {
        schema: "kombat.persistence_migration_registry.v1".into(),
        binary_fingerprint: fingerprint_hex(&migrations::current_binary_persistence_fingerprint()),
        registry_valid: registry_error.is_none(),
        migration_count: registry.len(),
        migrations: registry
            .iter()
            .map(|migration| PersistenceMigrationOutput {
                id: migration.id.to_string(),
                source_fingerprint: fingerprint_hex(&migration.source_fingerprint),
                target_fingerprint: fingerprint_hex(&migration.target_fingerprint),
                description: migration.description.to_string(),
                changed_records: migration
                    .changed_records
                    .iter()
                    .map(|record| (*record).to_string())
                    .collect(),
                backend_strategy: backend_migration_strategy_name(migration.backend_strategy)
                    .to_string(),
            })
            .collect(),
        error: registry_error.as_ref().map(ToString::to_string),
    };
    let exit_code = if registry_error.is_some() { 11 } else { 0 };
    render_list_migrations_output(exit_code, output, format)
}

pub(crate) fn run_migrate_state_command(args: &[String]) -> (i32, String) {
    let registry = migrations::compiled_persistence_migrations();
    if let Err(error) = migrations::validate_compiled_persistence_migrations(registry) {
        let started_at_ms = unix_time_ms();
        let format_and_report = parse_migrate_state_args(args)
            .ok()
            .map(|parsed| (parsed.format, parsed.report_path));
        let report = migrate_state_report(
            started_at_ms,
            "registry_invalid",
            true,
            None,
            Vec::new(),
            Some(error.to_string()),
        );
        let (format, report_path) =
            format_and_report.unwrap_or((MaintenanceOutputFormat::Json, None));
        return render_migrate_state_output(11, report, format, report_path.as_ref());
    }
    run_migrate_state_command_with_registry(args, registry)
}

pub(crate) fn run_migrate_snapshot_command(args: &[String]) -> (i32, String) {
    let registry = migrations::compiled_persistence_migrations();
    if let Err(error) = migrations::validate_compiled_persistence_migrations(registry) {
        let started_at_ms = unix_time_ms();
        let format_and_report = parse_migrate_snapshot_args(args)
            .ok()
            .map(|parsed| (parsed.format, parsed.report_path));
        let report = migrate_state_report(
            started_at_ms,
            "registry_invalid",
            false,
            None,
            Vec::new(),
            Some(error.to_string()),
        );
        let (format, report_path) =
            format_and_report.unwrap_or((MaintenanceOutputFormat::Json, None));
        return render_migrate_state_output(11, report, format, report_path.as_ref());
    }
    run_migrate_snapshot_command_with_registry(args, registry)
}

pub(crate) fn run_repair_market_custody_command(args: &[String]) -> (i32, String) {
    let started_at_ms = unix_time_ms();
    let parsed = match parse_repair_market_custody_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let report =
                repair_market_custody_report(started_at_ms, "error", true, Vec::new(), Some(error));
            return render_repair_market_custody_output(
                4,
                report,
                MaintenanceOutputFormat::Json,
                None,
            );
        }
    };

    if !parsed.dry_run && parsed.confirm.as_deref() != Some("repair-market-custody") {
        let report = repair_market_custody_report(
            started_at_ms,
            "confirmation_missing",
            parsed.dry_run,
            Vec::new(),
            Some("--confirm repair-market-custody is required".into()),
        );
        return render_repair_market_custody_output(
            14,
            report,
            parsed.format,
            parsed.report_path.as_ref(),
        );
    }

    let db_path = resolve_state_db_path(&parsed.data_dir);
    if !db_path.exists() {
        let report = repair_market_custody_report(
            started_at_ms,
            "no_state",
            parsed.dry_run,
            Vec::new(),
            None,
        );
        return render_repair_market_custody_output(
            2,
            report,
            parsed.format,
            parsed.report_path.as_ref(),
        );
    }

    let mut opts = OpenOptions::new(db_path)
        .without_table_initialization()
        .abort_repair_on_open();
    opts.create_if_missing = false;
    let backend = match RedbBackend::open(opts) {
        Ok(backend) => AnyStateBackend::Redb(backend),
        Err(BackendError::Locked(error)) => {
            let report = repair_market_custody_report(
                started_at_ms,
                "source_locked",
                parsed.dry_run,
                Vec::new(),
                Some(error),
            );
            return render_repair_market_custody_output(
                13,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
        Err(error) => {
            let report = repair_market_custody_report(
                started_at_ms,
                "source_open_failed",
                parsed.dry_run,
                Vec::new(),
                Some(error.to_string()),
            );
            return render_repair_market_custody_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };

    match SingleNodeEngine::repair_market_custody_in_state_backend(&backend, parsed.dry_run) {
        Ok(node_report) => {
            let adjustments = repair_market_custody_adjustments(node_report);
            let result = if adjustments.is_empty() {
                "already_consistent"
            } else if parsed.dry_run {
                "repair_available"
            } else {
                "repair_completed"
            };
            let report = repair_market_custody_report(
                started_at_ms,
                result,
                parsed.dry_run,
                adjustments,
                None,
            );
            render_repair_market_custody_output(
                0,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
        Err(error) => {
            let report = repair_market_custody_report(
                started_at_ms,
                "repair_failed",
                parsed.dry_run,
                Vec::new(),
                Some(error.to_string()),
            );
            render_repair_market_custody_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
    }
}

pub(crate) fn run_audit_market_custody_command(args: &[String]) -> (i32, String) {
    let started_at_ms = unix_time_ms();
    let parsed = match parse_audit_market_custody_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let report =
                audit_market_custody_report(started_at_ms, "error", Vec::new(), Some(error));
            return render_audit_market_custody_output(
                4,
                report,
                MaintenanceOutputFormat::Json,
                None,
            );
        }
    };

    let db_path = resolve_state_db_path(&parsed.data_dir);
    if !db_path.exists() {
        let report = audit_market_custody_report(started_at_ms, "no_state", Vec::new(), None);
        return render_audit_market_custody_output(
            2,
            report,
            parsed.format,
            parsed.report_path.as_ref(),
        );
    }

    let backend = match open_stopped_redb_backend(&db_path) {
        Ok(backend) => backend,
        Err((exit_code, result, error)) => {
            let report =
                audit_market_custody_report(started_at_ms, result, Vec::new(), Some(error));
            return render_audit_market_custody_output(
                exit_code,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };

    match SingleNodeEngine::audit_market_custody_in_state_backend(&backend, parsed.top_rows) {
        Ok(node_report) => {
            let report = audit_market_custody_report(
                started_at_ms,
                "ok",
                audit_market_custody_markets(node_report),
                None,
            );
            render_audit_market_custody_output(
                0,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
        Err(error) => {
            let report = audit_market_custody_report(
                started_at_ms,
                "audit_failed",
                Vec::new(),
                Some(error.to_string()),
            );
            render_audit_market_custody_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
    }
}

pub(crate) fn run_repair_isolated_collateral_command(args: &[String]) -> (i32, String) {
    let started_at_ms = unix_time_ms();
    let parsed = match parse_repair_isolated_collateral_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let report = repair_isolated_collateral_report(
                started_at_ms,
                "error",
                true,
                Vec::new(),
                Vec::new(),
                empty_audit_market_custody_report(started_at_ms),
                Some(error),
            );
            return render_repair_isolated_collateral_output(
                4,
                report,
                MaintenanceOutputFormat::Json,
                None,
            );
        }
    };

    if !parsed.dry_run && parsed.confirm.as_deref() != Some("repair-isolated-collateral") {
        let report = repair_isolated_collateral_report(
            started_at_ms,
            "confirmation_missing",
            parsed.dry_run,
            Vec::new(),
            Vec::new(),
            empty_audit_market_custody_report(started_at_ms),
            Some("--confirm repair-isolated-collateral is required".into()),
        );
        return render_repair_isolated_collateral_output(
            14,
            report,
            parsed.format,
            parsed.report_path.as_ref(),
        );
    }

    let db_path = resolve_state_db_path(&parsed.data_dir);
    if !db_path.exists() {
        let report = repair_isolated_collateral_report(
            started_at_ms,
            "no_state",
            parsed.dry_run,
            Vec::new(),
            Vec::new(),
            empty_audit_market_custody_report(started_at_ms),
            None,
        );
        return render_repair_isolated_collateral_output(
            2,
            report,
            parsed.format,
            parsed.report_path.as_ref(),
        );
    }

    let backend = match open_stopped_redb_backend(&db_path) {
        Ok(backend) => backend,
        Err((exit_code, result, error)) => {
            let report = repair_isolated_collateral_report(
                started_at_ms,
                result,
                parsed.dry_run,
                Vec::new(),
                Vec::new(),
                empty_audit_market_custody_report(started_at_ms),
                Some(error),
            );
            return render_repair_isolated_collateral_output(
                exit_code,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };

    match SingleNodeEngine::repair_isolated_collateral_in_state_backend(
        &backend,
        parsed.dry_run,
        &parsed.targets,
        parsed.top_rows,
    ) {
        Ok(node_report) => {
            let (collateral_adjustments, custody_adjustments, after_audit) =
                repair_isolated_collateral_parts(started_at_ms, node_report);
            let result = if collateral_adjustments.is_empty() && custody_adjustments.is_empty() {
                "already_consistent"
            } else if parsed.dry_run {
                "repair_available"
            } else {
                "repair_completed"
            };
            let report = repair_isolated_collateral_report(
                started_at_ms,
                result,
                parsed.dry_run,
                collateral_adjustments,
                custody_adjustments,
                after_audit,
                None,
            );
            render_repair_isolated_collateral_output(
                0,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
        Err(error) => {
            let report = repair_isolated_collateral_report(
                started_at_ms,
                "repair_failed",
                parsed.dry_run,
                Vec::new(),
                Vec::new(),
                empty_audit_market_custody_report(started_at_ms),
                Some(error.to_string()),
            );
            render_repair_isolated_collateral_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
    }
}

pub(crate) fn run_migrate_state_command_with_registry(
    args: &[String],
    registry: &[migrations::PersistenceMigration],
) -> (i32, String) {
    let started_at_ms = unix_time_ms();
    let parsed = match parse_migrate_state_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let report =
                migrate_state_report(started_at_ms, "error", true, None, Vec::new(), Some(error));
            return render_migrate_state_output(4, report, MaintenanceOutputFormat::Json, None);
        }
    };

    if !parsed.dry_run && parsed.confirm.as_deref() != Some("migrate-state") {
        let report = migrate_state_report(
            started_at_ms,
            "confirmation_missing",
            parsed.dry_run,
            None,
            Vec::new(),
            Some("--confirm migrate-state is required".into()),
        );
        return render_migrate_state_output(14, report, parsed.format, parsed.report_path.as_ref());
    }

    if let Err(error) = migrations::validate_migration_registry(registry) {
        let report = migrate_state_report(
            started_at_ms,
            "registry_invalid",
            parsed.dry_run,
            None,
            Vec::new(),
            Some(error.to_string()),
        );
        return render_migrate_state_output(11, report, parsed.format, parsed.report_path.as_ref());
    }

    let db_path = resolve_state_db_path(&parsed.data_dir);
    if !db_path.exists() {
        let report = migrate_state_report(
            started_at_ms,
            "no_state",
            parsed.dry_run,
            None,
            Vec::new(),
            None,
        );
        return render_migrate_state_output(0, report, parsed.format, parsed.report_path.as_ref());
    }

    // redb 2.6.3 does not expose a true read-only open, but migration
    // source mode disables table initialization and aborts open-time
    // repair so this path cannot create tables or recover-by-writing
    // the source. The deploy wrapper must still stop validators/ingress
    // before invoking this command; the redb file lock maps live opens
    // to RFC 057 exit code 13.
    let mut opts = OpenOptions::new(&db_path)
        .without_table_initialization()
        .abort_repair_on_open();
    opts.create_if_missing = false;
    let backend = match RedbBackend::open(opts) {
        Ok(backend) => backend,
        Err(BackendError::Locked(error)) => {
            let report = migrate_state_report(
                started_at_ms,
                "source_locked",
                parsed.dry_run,
                None,
                Vec::new(),
                Some(error),
            );
            return render_migrate_state_output(
                13,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
        Err(error) => {
            let report = migrate_state_report(
                started_at_ms,
                "source_open_failed",
                parsed.dry_run,
                None,
                Vec::new(),
                Some(error.to_string()),
            );
            return render_migrate_state_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };

    let check = match backend.check_persistence_fingerprint() {
        Ok(check) => check,
        Err(error) => {
            let report = migrate_state_report(
                started_at_ms,
                "source_check_failed",
                parsed.dry_run,
                None,
                Vec::new(),
                Some(error.to_string()),
            );
            return render_migrate_state_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };
    let source_inventory = match collect_backend_inventory(&backend) {
        Ok(inventory) => Some(inventory),
        Err(error) => {
            let report = migrate_state_report(
                started_at_ms,
                "source_inventory_failed",
                parsed.dry_run,
                None,
                Vec::new(),
                Some(error),
            );
            return render_migrate_state_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };

    match check {
        kombat_chain_state_db::PersistenceFingerprintCheck::Empty => {
            let mut report = migrate_state_report(
                started_at_ms,
                "no_state",
                parsed.dry_run,
                None,
                Vec::new(),
                None,
            );
            report.source = source_inventory;
            render_migrate_state_output(0, report, parsed.format, parsed.report_path.as_ref())
        }
        kombat_chain_state_db::PersistenceFingerprintCheck::Compatible { fingerprint } => {
            let validation_error =
                SingleNodeEngine::from_state_backend(&AnyStateBackend::Redb(backend.clone()))
                    .err()
                    .map(|error| format!("hydrate compatible backend: {error}"));
            let mut report = migrate_state_report(
                started_at_ms,
                if validation_error.is_some() {
                    "compatible_validation_failed"
                } else {
                    "already_compatible"
                },
                parsed.dry_run,
                Some(fingerprint),
                Vec::new(),
                validation_error,
            );
            report.source = source_inventory;
            if report.error.is_none() {
                report.validations = MigrateStateValidationsReport::passed_backend_hydration();
                render_migrate_state_output(0, report, parsed.format, parsed.report_path.as_ref())
            } else {
                render_migrate_state_output(12, report, parsed.format, parsed.report_path.as_ref())
            }
        }
        kombat_chain_state_db::PersistenceFingerprintCheck::SchemaFingerprintMismatch {
            expected: _,
            actual,
        } => {
            let path = match migrations::canonical_migration_path(
                registry,
                actual,
                backend.expected_persistence_fingerprint(),
            ) {
                Ok(path) => path,
                Err(error) => {
                    let report = migrate_state_report(
                        started_at_ms,
                        "registry_invalid",
                        parsed.dry_run,
                        Some(actual),
                        Vec::new(),
                        Some(error.to_string()),
                    );
                    return render_migrate_state_output(
                        11,
                        report,
                        parsed.format,
                        parsed.report_path.as_ref(),
                    );
                }
            };
            let Some(path) = path else {
                let mut report = migrate_state_report(
                    started_at_ms,
                    "no_migration_path",
                    parsed.dry_run,
                    Some(actual),
                    Vec::new(),
                    Some("fingerprint mismatch and no migration path exists".into()),
                );
                report.source = source_inventory;
                return render_migrate_state_output(
                    10,
                    report,
                    parsed.format,
                    parsed.report_path.as_ref(),
                );
            };
            let path_ids = path
                .iter()
                .map(|migration| migration.id.to_string())
                .collect();
            let (exit_code, report) = execute_backend_migration(
                started_at_ms,
                &parsed,
                &db_path,
                backend,
                actual,
                &path,
                path_ids,
                source_inventory,
            );
            render_migrate_state_output(
                exit_code,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            )
        }
        other => {
            let mut report = migrate_state_report(
                started_at_ms,
                "envelope_invalid",
                parsed.dry_run,
                None,
                Vec::new(),
                other.error_reason(),
            );
            report.source = source_inventory;
            render_migrate_state_output(10, report, parsed.format, parsed.report_path.as_ref())
        }
    }
}

pub(crate) fn run_migrate_snapshot_command_with_registry(
    args: &[String],
    registry: &[migrations::PersistenceMigration],
) -> (i32, String) {
    let started_at_ms = unix_time_ms();
    let parsed = match parse_migrate_snapshot_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let report =
                migrate_state_report(started_at_ms, "error", false, None, Vec::new(), Some(error));
            return render_migrate_state_output(4, report, MaintenanceOutputFormat::Json, None);
        }
    };

    if parsed.input_path == parsed.output_path {
        let report = migrate_state_report(
            started_at_ms,
            "target_path_exists",
            false,
            None,
            Vec::new(),
            Some("snapshot input and output paths must differ".into()),
        );
        return render_migrate_state_output(15, report, parsed.format, parsed.report_path.as_ref());
    }
    if parsed.output_path.exists() {
        let report = migrate_state_report(
            started_at_ms,
            "target_path_exists",
            false,
            None,
            Vec::new(),
            Some(format!(
                "snapshot output path already exists: {}",
                parsed.output_path.display()
            )),
        );
        return render_migrate_state_output(15, report, parsed.format, parsed.report_path.as_ref());
    }

    if let Err(error) = migrations::validate_migration_registry(registry) {
        let report = migrate_state_report(
            started_at_ms,
            "registry_invalid",
            false,
            None,
            Vec::new(),
            Some(error.to_string()),
        );
        return render_migrate_state_output(11, report, parsed.format, parsed.report_path.as_ref());
    }

    let source_bytes = match fs::read(&parsed.input_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let report = migrate_state_report(
                started_at_ms,
                "source_open_failed",
                false,
                None,
                Vec::new(),
                Some(error.to_string()),
            );
            return render_migrate_state_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };
    let source_header =
        match kombat_chain_state_db::logical_snapshot_header_from_slice(&source_bytes) {
            Ok(header) => header,
            Err(error) => {
                let report = migrate_state_report(
                    started_at_ms,
                    "envelope_invalid",
                    false,
                    None,
                    Vec::new(),
                    Some(error.to_string()),
                );
                return render_migrate_state_output(
                    10,
                    report,
                    parsed.format,
                    parsed.report_path.as_ref(),
                );
            }
        };
    let target_fingerprint = migrations::current_binary_persistence_fingerprint();
    let path = match migrations::canonical_migration_path(
        registry,
        source_header.persistence_fingerprint,
        target_fingerprint,
    ) {
        Ok(path) => path,
        Err(error) => {
            let report = migrate_state_report(
                started_at_ms,
                "registry_invalid",
                false,
                Some(source_header.persistence_fingerprint),
                Vec::new(),
                Some(error.to_string()),
            );
            return render_migrate_state_output(
                11,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };
    let Some(path) = path else {
        let report = migrate_state_report(
            started_at_ms,
            "no_migration_path",
            false,
            Some(source_header.persistence_fingerprint),
            Vec::new(),
            Some("snapshot fingerprint mismatch and no migration path exists".into()),
        );
        return render_migrate_state_output(10, report, parsed.format, parsed.report_path.as_ref());
    };
    let path_ids: Vec<String> = path
        .iter()
        .map(|migration| migration.id.to_string())
        .collect();

    if path.iter().any(|migration| {
        migration.backend_strategy == migrations::BackendMigrationStrategy::NotImplemented
    }) {
        let report = migrate_state_report(
            started_at_ms,
            "migration_implementation_missing",
            false,
            Some(source_header.persistence_fingerprint),
            path_ids,
            Some(
                "migration path contains an edge without a snapshot execution implementation"
                    .into(),
            ),
        );
        return render_migrate_state_output(12, report, parsed.format, parsed.report_path.as_ref());
    }

    let migrated_bytes = match kombat_chain_state_db::migrate_logical_snapshot_slice_to_vec(
        &source_bytes,
        target_fingerprint,
        |namespace, key, value| {
            let mut migrated_value = value.to_vec();
            for migration in &path {
                migrated_value =
                    migrations::migrate_backend_record(migration, namespace, key, &migrated_value)?;
            }
            Ok(migrated_value)
        },
    ) {
        Ok((bytes, _summary)) => bytes,
        Err(error) => {
            let report = migrate_state_report(
                started_at_ms,
                "snapshot_migration_failed",
                false,
                Some(source_header.persistence_fingerprint),
                path_ids,
                Some(error.to_string()),
            );
            return render_migrate_state_output(
                12,
                report,
                parsed.format,
                parsed.report_path.as_ref(),
            );
        }
    };

    if let Err(error) = SingleNodeEngine::from_snapshot_bytes(&migrated_bytes) {
        let report = migrate_state_report(
            started_at_ms,
            "snapshot_validation_failed",
            false,
            Some(source_header.persistence_fingerprint),
            path_ids,
            Some(error.to_string()),
        );
        return render_migrate_state_output(12, report, parsed.format, parsed.report_path.as_ref());
    }

    if let Err(error) = fs::write(&parsed.output_path, &migrated_bytes) {
        let report = migrate_state_report(
            started_at_ms,
            "target_write_failed",
            false,
            Some(source_header.persistence_fingerprint),
            path_ids,
            Some(error.to_string()),
        );
        return render_migrate_state_output(12, report, parsed.format, parsed.report_path.as_ref());
    }

    let mut report = migrate_state_report(
        started_at_ms,
        if path.is_empty() {
            "snapshot_already_compatible"
        } else {
            "snapshot_migration_completed"
        },
        false,
        Some(source_header.persistence_fingerprint),
        path_ids,
        None,
    );
    report.staged_path = Some(parsed.output_path.display().to_string());
    render_migrate_state_output(0, report, parsed.format, parsed.report_path.as_ref())
}

pub(crate) fn run_check_persistence_fingerprint_command(args: &[String]) -> (i32, String) {
    let parsed = match parse_check_persistence_fingerprint_args(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            let output = CheckPersistenceFingerprintOutput {
                result: "error".into(),
                binary_fingerprint: fingerprint_hex(
                    &kombat_chain_state_db::current_persistence_fingerprint(),
                ),
                on_disk_fingerprint: None,
                envelope_version: None,
                error: Some(error),
            };
            return render_check_output(4, output, MaintenanceOutputFormat::Json);
        }
    };

    let db_path = resolve_state_db_path(&parsed.data_dir);
    if !db_path.exists() {
        let output = CheckPersistenceFingerprintOutput {
            result: "no_state".into(),
            binary_fingerprint: fingerprint_hex(
                &kombat_chain_state_db::current_persistence_fingerprint(),
            ),
            on_disk_fingerprint: None,
            envelope_version: None,
            error: None,
        };
        return render_check_output(2, output, parsed.format);
    }

    let mut opts = OpenOptions::new(db_path)
        .without_table_initialization()
        .abort_repair_on_open();
    opts.create_if_missing = false;
    let backend = match RedbBackend::open(opts) {
        Ok(backend) => backend,
        Err(BackendError::Locked(error)) => {
            let output = CheckPersistenceFingerprintOutput {
                result: "source_locked".into(),
                binary_fingerprint: fingerprint_hex(
                    &kombat_chain_state_db::current_persistence_fingerprint(),
                ),
                on_disk_fingerprint: None,
                envelope_version: None,
                error: Some(error),
            };
            return render_check_output(13, output, parsed.format);
        }
        Err(error) => {
            let output = CheckPersistenceFingerprintOutput {
                result: "error".into(),
                binary_fingerprint: fingerprint_hex(
                    &kombat_chain_state_db::current_persistence_fingerprint(),
                ),
                on_disk_fingerprint: None,
                envelope_version: None,
                error: Some(error.to_string()),
            };
            return render_check_output(4, output, parsed.format);
        }
    };

    let binary_fingerprint = backend.expected_persistence_fingerprint();
    let check = match backend.check_persistence_fingerprint() {
        Ok(check) => check,
        Err(error) => {
            let output = CheckPersistenceFingerprintOutput {
                result: "error".into(),
                binary_fingerprint: fingerprint_hex(&binary_fingerprint),
                on_disk_fingerprint: None,
                envelope_version: None,
                error: Some(error.to_string()),
            };
            return render_check_output(4, output, parsed.format);
        }
    };

    let output = check_persistence_fingerprint_output(binary_fingerprint, check);

    let exit_code = match output.result.as_str() {
        "compatible" => 0,
        "mismatch" => 1,
        "no_state" => 2,
        "envelope_invalid" => 3,
        _ => 4,
    };
    render_check_output(exit_code, output, parsed.format)
}

pub(crate) fn check_persistence_fingerprint_output(
    binary_fingerprint: [u8; 32],
    check: kombat_chain_state_db::PersistenceFingerprintCheck,
) -> CheckPersistenceFingerprintOutput {
    match check {
        kombat_chain_state_db::PersistenceFingerprintCheck::Empty => {
            CheckPersistenceFingerprintOutput {
                result: "no_state".into(),
                binary_fingerprint: fingerprint_hex(&binary_fingerprint),
                on_disk_fingerprint: None,
                envelope_version: None,
                error: None,
            }
        }
        kombat_chain_state_db::PersistenceFingerprintCheck::Compatible { fingerprint } => {
            CheckPersistenceFingerprintOutput {
                result: "compatible".into(),
                binary_fingerprint: fingerprint_hex(&binary_fingerprint),
                on_disk_fingerprint: Some(fingerprint_hex(&fingerprint)),
                envelope_version: Some(kombat_chain_state_db::PERSISTENCE_ENVELOPE_VERSION),
                error: None,
            }
        }
        kombat_chain_state_db::PersistenceFingerprintCheck::SchemaFingerprintMismatch {
            expected: _,
            actual,
        } => CheckPersistenceFingerprintOutput {
            result: "mismatch".into(),
            binary_fingerprint: fingerprint_hex(&binary_fingerprint),
            on_disk_fingerprint: Some(fingerprint_hex(&actual)),
            envelope_version: Some(kombat_chain_state_db::PERSISTENCE_ENVELOPE_VERSION),
            error: None,
        },
        other => CheckPersistenceFingerprintOutput {
            result: "envelope_invalid".into(),
            binary_fingerprint: fingerprint_hex(&binary_fingerprint),
            on_disk_fingerprint: None,
            envelope_version: match other {
                kombat_chain_state_db::PersistenceFingerprintCheck::EnvelopeVersionUnsupported {
                    actual,
                } => Some(actual),
                _ => None,
            },
            error: other.error_reason(),
        },
    }
}

pub(crate) fn parse_check_persistence_fingerprint_args(
    args: &[String],
) -> Result<CheckPersistenceFingerprintArgs, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut format = MaintenanceOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--data-dir requires a value".to_string())?;
                data_dir = Some(PathBuf::from(value));
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }
    Ok(CheckPersistenceFingerprintArgs {
        data_dir: data_dir.ok_or_else(|| "--data-dir is required".to_string())?,
        format,
    })
}

pub(crate) fn parse_format_only_args(args: &[String]) -> Result<MaintenanceOutputFormat, String> {
    let mut format = MaintenanceOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }
    Ok(format)
}

pub(crate) fn parse_migrate_state_args(args: &[String]) -> Result<MigrateStateArgs, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut format = MaintenanceOutputFormat::Text;
    let mut dry_run = false;
    let mut confirm: Option<String> = None;
    let mut report_path: Option<PathBuf> = None;
    let mut target_current = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--data-dir requires a value".to_string())?;
                data_dir = Some(PathBuf::from(value));
            }
            "--to" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--to requires a value".to_string())?;
                if value != "current" {
                    return Err("--to currently supports only current".into());
                }
                target_current = true;
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--confirm" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--confirm requires a value".to_string())?;
                confirm = Some(value.clone());
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--report requires a value".to_string())?;
                report_path = Some(PathBuf::from(value));
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }

    if !target_current {
        return Err("--to current is required".into());
    }

    Ok(MigrateStateArgs {
        data_dir: data_dir.ok_or_else(|| "--data-dir is required".to_string())?,
        format,
        dry_run,
        confirm,
        report_path,
    })
}

pub(crate) fn parse_migrate_snapshot_args(args: &[String]) -> Result<MigrateSnapshotArgs, String> {
    let mut input_path: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut format = MaintenanceOutputFormat::Text;
    let mut report_path: Option<PathBuf> = None;
    let mut target_current = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--input requires a value".to_string())?;
                input_path = Some(PathBuf::from(value));
            }
            "--output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--output requires a value".to_string())?;
                output_path = Some(PathBuf::from(value));
            }
            "--to" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--to requires a value".to_string())?;
                if value != "current" {
                    return Err("--to currently supports only current".into());
                }
                target_current = true;
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--report requires a value".to_string())?;
                report_path = Some(PathBuf::from(value));
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }

    if !target_current {
        return Err("--to current is required".into());
    }

    Ok(MigrateSnapshotArgs {
        input_path: input_path.ok_or_else(|| "--input is required".to_string())?,
        output_path: output_path.ok_or_else(|| "--output is required".to_string())?,
        format,
        report_path,
    })
}

pub(crate) fn parse_repair_market_custody_args(
    args: &[String],
) -> Result<RepairMarketCustodyArgs, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut format = MaintenanceOutputFormat::Text;
    let mut dry_run = false;
    let mut confirm: Option<String> = None;
    let mut report_path: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--data-dir requires a value".to_string())?;
                data_dir = Some(PathBuf::from(value));
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--confirm" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--confirm requires a value".to_string())?;
                confirm = Some(value.clone());
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--report requires a value".to_string())?;
                report_path = Some(PathBuf::from(value));
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }

    Ok(RepairMarketCustodyArgs {
        data_dir: data_dir.ok_or_else(|| "--data-dir is required".to_string())?,
        format,
        dry_run,
        confirm,
        report_path,
    })
}

pub(crate) fn parse_audit_market_custody_args(
    args: &[String],
) -> Result<AuditMarketCustodyArgs, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut format = MaintenanceOutputFormat::Text;
    let mut top_rows = 10usize;
    let mut report_path: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--data-dir requires a value".to_string())?;
                data_dir = Some(PathBuf::from(value));
            }
            "--top-rows" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--top-rows requires a value".to_string())?;
                top_rows = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-rows {value}"))?;
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--report requires a value".to_string())?;
                report_path = Some(PathBuf::from(value));
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }

    Ok(AuditMarketCustodyArgs {
        data_dir: data_dir.ok_or_else(|| "--data-dir is required".to_string())?,
        format,
        top_rows,
        report_path,
    })
}

pub(crate) fn parse_repair_isolated_collateral_args(
    args: &[String],
) -> Result<RepairIsolatedCollateralArgs, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut format = MaintenanceOutputFormat::Text;
    let mut dry_run = false;
    let mut confirm: Option<String> = None;
    let mut top_rows = 10usize;
    let mut targets = Vec::new();
    let mut report_path: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--data-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--data-dir requires a value".to_string())?;
                data_dir = Some(PathBuf::from(value));
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--confirm" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--confirm requires a value".to_string())?;
                confirm = Some(value.clone());
            }
            "--set" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--set requires market:account:collateral".to_string())?;
                targets.push(parse_isolated_collateral_target(value)?);
            }
            "--top-rows" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--top-rows requires a value".to_string())?;
                top_rows = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --top-rows {value}"))?;
            }
            "--report" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--report requires a value".to_string())?;
                report_path = Some(PathBuf::from(value));
            }
            "--format" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--format requires a value".to_string())?;
                format = match value.as_str() {
                    "text" => MaintenanceOutputFormat::Text,
                    "json" => MaintenanceOutputFormat::Json,
                    other => return Err(format!("unsupported --format {other}")),
                };
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }

    if targets.is_empty() {
        return Err("at least one --set market:account:collateral is required".into());
    }

    Ok(RepairIsolatedCollateralArgs {
        data_dir: data_dir.ok_or_else(|| "--data-dir is required".to_string())?,
        format,
        dry_run,
        confirm,
        top_rows,
        targets,
        report_path,
    })
}

pub(crate) fn parse_isolated_collateral_target(
    value: &str,
) -> Result<IsolatedCollateralRepairTarget, String> {
    let mut parts = value.split(':');
    let market_id = parts
        .next()
        .ok_or_else(|| "--set requires market:account:collateral".to_string())?
        .parse::<u64>()
        .map_err(|_| format!("invalid --set market id in {value}"))?;
    let account_id = parts
        .next()
        .ok_or_else(|| "--set requires market:account:collateral".to_string())?
        .parse::<u64>()
        .map_err(|_| format!("invalid --set account id in {value}"))?;
    let collateral_micro_usdc = parts
        .next()
        .ok_or_else(|| "--set requires market:account:collateral".to_string())?
        .parse::<i128>()
        .map_err(|_| format!("invalid --set collateral in {value}"))?;
    if parts.next().is_some() {
        return Err(format!("invalid --set target {value}"));
    }
    Ok(IsolatedCollateralRepairTarget {
        market_id,
        account_id,
        collateral_micro_usdc,
    })
}

pub(crate) fn resolve_state_db_path(data_dir: &Path) -> PathBuf {
    if data_dir.is_file() || data_dir.extension().is_some_and(|ext| ext == "redb") {
        data_dir.to_path_buf()
    } else {
        data_dir.join("state.redb")
    }
}

pub(crate) fn open_stopped_redb_backend(
    db_path: &Path,
) -> Result<AnyStateBackend, (i32, &'static str, String)> {
    let mut opts = OpenOptions::new(db_path)
        .without_table_initialization()
        .abort_repair_on_open();
    opts.create_if_missing = false;
    match RedbBackend::open(opts) {
        Ok(backend) => Ok(AnyStateBackend::Redb(backend)),
        Err(BackendError::Locked(error)) => Err((13, "source_locked", error)),
        Err(error) => Err((12, "source_open_failed", error.to_string())),
    }
}

pub(crate) fn render_check_output(
    exit_code: i32,
    output: CheckPersistenceFingerprintOutput,
    format: MaintenanceOutputFormat,
) -> (i32, String) {
    let rendered = match format {
        MaintenanceOutputFormat::Json => {
            serde_json::to_string_pretty(&output).expect("check output should serialize")
        }
        MaintenanceOutputFormat::Text => match output.error.as_deref() {
            Some(error) => format!("{}: {error}", output.result),
            None => output.result.clone(),
        },
    };
    (exit_code, rendered)
}

pub(crate) fn render_list_migrations_output(
    exit_code: i32,
    output: ListPersistenceMigrationsOutput,
    format: MaintenanceOutputFormat,
) -> (i32, String) {
    let rendered = match format {
        MaintenanceOutputFormat::Json => serde_json::to_string_pretty(&output)
            .expect("migration registry output should serialize"),
        MaintenanceOutputFormat::Text => {
            if let Some(error) = output.error.as_deref() {
                format!("persistence_migrations invalid: {error}")
            } else if output.migrations.is_empty() {
                format!(
                    "persistence_migrations: none\nbinary_fingerprint: {}",
                    output.binary_fingerprint
                )
            } else {
                let mut lines = vec![
                    format!("binary_fingerprint: {}", output.binary_fingerprint),
                    format!("persistence_migrations: {}", output.migrations.len()),
                ];
                lines.extend(output.migrations.iter().map(|migration| {
                    format!(
                        "{} {} -> {}",
                        migration.id, migration.source_fingerprint, migration.target_fingerprint
                    )
                }));
                lines.join("\n")
            }
        }
    };
    (exit_code, rendered)
}

pub(crate) fn repair_market_custody_report(
    started_at_ms: u64,
    result: &str,
    dry_run: bool,
    adjustments: Vec<RepairMarketCustodyAdjustmentReport>,
    error: Option<String>,
) -> RepairMarketCustodyReport {
    RepairMarketCustodyReport {
        schema: "kombat.market_custody_repair.v1".into(),
        started_at_ms,
        finished_at_ms: started_at_ms,
        result: result.into(),
        dry_run,
        adjustments,
        error,
    }
}

pub(crate) fn repair_market_custody_adjustments(
    report: NodeMarketCustodyRepairReport,
) -> Vec<RepairMarketCustodyAdjustmentReport> {
    report
        .adjustments
        .into_iter()
        .map(|adjustment| RepairMarketCustodyAdjustmentReport {
            market_id: adjustment.market_id,
            custody_balance_micro_usdc: adjustment.custody_balance_micro_usdc.to_string(),
            required_before_micro_usdc: adjustment.required_before_micro_usdc.to_string(),
            treasury_before_micro_usdc: adjustment.treasury_before_micro_usdc.to_string(),
            treasury_after_micro_usdc: adjustment.treasury_after_micro_usdc.to_string(),
        })
        .collect()
}

pub(crate) fn audit_market_custody_report(
    started_at_ms: u64,
    result: &str,
    markets: Vec<AuditMarketCustodyMarketReport>,
    error: Option<String>,
) -> AuditMarketCustodyReport {
    AuditMarketCustodyReport {
        schema: "kombat.market_custody_audit.v1".into(),
        started_at_ms,
        finished_at_ms: started_at_ms,
        result: result.into(),
        markets,
        error,
    }
}

pub(crate) fn empty_audit_market_custody_report(started_at_ms: u64) -> AuditMarketCustodyReport {
    audit_market_custody_report(started_at_ms, "not_run", Vec::new(), None)
}

pub(crate) fn audit_market_custody_markets(
    report: NodeMarketCustodyAuditReport,
) -> Vec<AuditMarketCustodyMarketReport> {
    report
        .markets
        .into_iter()
        .map(|market| AuditMarketCustodyMarketReport {
            market_id: market.market_id,
            custody_balance_micro_usdc: market
                .custody_balance_micro_usdc
                .map(|value| value.to_string()),
            required_balance_micro_usdc: market
                .required_balance_micro_usdc
                .map(|value| value.to_string()),
            custody_surplus_micro_usdc: market
                .custody_surplus_micro_usdc
                .map(|value| value.to_string()),
            required_error: market.required_error,
            positive_isolated_collateral_micro_usdc: market
                .positive_isolated_collateral_micro_usdc
                .to_string(),
            negative_isolated_collateral_micro_usdc: market
                .negative_isolated_collateral_micro_usdc
                .to_string(),
            fee_vault_micro_usdc: market.fee_vault_micro_usdc.to_string(),
            insurance_fund_micro_usdc: market.insurance_fund_micro_usdc.to_string(),
            validator_reward_pool_micro_usdc: market.validator_reward_pool_micro_usdc.to_string(),
            market_maker_rebate_pool_micro_usdc: market
                .market_maker_rebate_pool_micro_usdc
                .to_string(),
            treasury_micro_usdc: market.treasury_micro_usdc.to_string(),
            top_isolated_collateral: market
                .top_isolated_collateral
                .into_iter()
                .map(|row| AuditIsolatedCollateralRowReport {
                    market_id: row.market_id,
                    account_id: row.account_id,
                    collateral_micro_usdc: row.collateral_micro_usdc.to_string(),
                })
                .collect(),
        })
        .collect()
}

pub(crate) fn repair_isolated_collateral_report(
    started_at_ms: u64,
    result: &str,
    dry_run: bool,
    collateral_adjustments: Vec<RepairIsolatedCollateralAdjustmentReport>,
    custody_adjustments: Vec<RepairMarketCustodyAdjustmentReport>,
    after_audit: AuditMarketCustodyReport,
    error: Option<String>,
) -> RepairIsolatedCollateralReport {
    RepairIsolatedCollateralReport {
        schema: "kombat.isolated_collateral_repair.v1".into(),
        started_at_ms,
        finished_at_ms: started_at_ms,
        result: result.into(),
        dry_run,
        collateral_adjustments,
        custody_adjustments,
        after_audit,
        error,
    }
}

pub(crate) fn repair_isolated_collateral_parts(
    started_at_ms: u64,
    report: NodeIsolatedCollateralRepairReport,
) -> (
    Vec<RepairIsolatedCollateralAdjustmentReport>,
    Vec<RepairMarketCustodyAdjustmentReport>,
    AuditMarketCustodyReport,
) {
    let collateral_adjustments = report
        .collateral_adjustments
        .into_iter()
        .map(|adjustment| RepairIsolatedCollateralAdjustmentReport {
            market_id: adjustment.market_id,
            account_id: adjustment.account_id,
            before_micro_usdc: adjustment.before_micro_usdc.to_string(),
            after_micro_usdc: adjustment.after_micro_usdc.to_string(),
        })
        .collect();
    let custody_adjustments = repair_market_custody_adjustments(NodeMarketCustodyRepairReport {
        dry_run: report.dry_run,
        adjustments: report.custody_adjustments,
    });
    let after_audit = audit_market_custody_report(
        started_at_ms,
        "ok",
        audit_market_custody_markets(report.after_audit),
        None,
    );
    (collateral_adjustments, custody_adjustments, after_audit)
}

pub(crate) fn render_repair_market_custody_output(
    exit_code: i32,
    mut report: RepairMarketCustodyReport,
    format: MaintenanceOutputFormat,
    report_path: Option<&PathBuf>,
) -> (i32, String) {
    report.finished_at_ms = unix_time_ms();
    if let Some(path) = report_path {
        if let Err(error) = fs::write(
            path,
            serde_json::to_string_pretty(&report).expect("repair report should serialize"),
        ) {
            let mut failed_report = report.clone();
            failed_report.result = "report_write_failed".into();
            failed_report.error = Some(error.to_string());
            return render_repair_market_custody_output(
                exit_code.max(12),
                failed_report,
                format,
                None,
            );
        }
    }
    let rendered = match format {
        MaintenanceOutputFormat::Json => {
            serde_json::to_string_pretty(&report).expect("repair report should serialize")
        }
        MaintenanceOutputFormat::Text => match report.error.as_deref() {
            Some(error) => format!("{}: {error}", report.result),
            None if report.adjustments.is_empty() => report.result.clone(),
            None => format!("{}: {} market(s)", report.result, report.adjustments.len()),
        },
    };
    (exit_code, rendered)
}

pub(crate) fn render_audit_market_custody_output(
    exit_code: i32,
    mut report: AuditMarketCustodyReport,
    format: MaintenanceOutputFormat,
    report_path: Option<&PathBuf>,
) -> (i32, String) {
    report.finished_at_ms = unix_time_ms();
    if let Some(path) = report_path {
        if let Err(error) = fs::write(
            path,
            serde_json::to_string_pretty(&report).expect("audit report should serialize"),
        ) {
            let mut failed_report = report.clone();
            failed_report.result = "report_write_failed".into();
            failed_report.error = Some(error.to_string());
            return render_audit_market_custody_output(
                exit_code.max(12),
                failed_report,
                format,
                None,
            );
        }
    }
    let rendered = match format {
        MaintenanceOutputFormat::Json => {
            serde_json::to_string_pretty(&report).expect("audit report should serialize")
        }
        MaintenanceOutputFormat::Text => match report.error.as_deref() {
            Some(error) => format!("{}: {error}", report.result),
            None => format!("{}: {} market(s)", report.result, report.markets.len()),
        },
    };
    (exit_code, rendered)
}

pub(crate) fn render_repair_isolated_collateral_output(
    exit_code: i32,
    mut report: RepairIsolatedCollateralReport,
    format: MaintenanceOutputFormat,
    report_path: Option<&PathBuf>,
) -> (i32, String) {
    report.finished_at_ms = unix_time_ms();
    report.after_audit.finished_at_ms = report.finished_at_ms;
    if let Some(path) = report_path {
        if let Err(error) = fs::write(
            path,
            serde_json::to_string_pretty(&report).expect("repair report should serialize"),
        ) {
            let mut failed_report = report.clone();
            failed_report.result = "report_write_failed".into();
            failed_report.error = Some(error.to_string());
            return render_repair_isolated_collateral_output(
                exit_code.max(12),
                failed_report,
                format,
                None,
            );
        }
    }
    let rendered = match format {
        MaintenanceOutputFormat::Json => {
            serde_json::to_string_pretty(&report).expect("repair report should serialize")
        }
        MaintenanceOutputFormat::Text => match report.error.as_deref() {
            Some(error) => format!("{}: {error}", report.result),
            None => format!(
                "{}: {} collateral row(s), {} custody adjustment(s)",
                report.result,
                report.collateral_adjustments.len(),
                report.custody_adjustments.len()
            ),
        },
    };
    (exit_code, rendered)
}
