use super::*;

#[derive(Clone, Debug)]
pub struct SingleNodeEngineSnapshot {
    pub(crate) configured_validator_verifiers: Vec<ValidatorVerifier>,
    pub(crate) validator_set: Vec<ValidatorId>,
    pub(crate) validator_verifiers: Vec<ValidatorVerifier>,
    pub(crate) consensus_signing_keys: BTreeMap<ValidatorId, [u8; 32]>,
    pub(crate) pacemaker_config: PacemakerConfig,
    pub(crate) pacemaker: PacemakerState,
    pub(crate) chain: Vec<Block>,
    pub(crate) executor: BlockExecutor<InMemoryStateStore>,
    pub(crate) account_store: AccountStore,
    pub(crate) policy_store: PolicyStore,
    pub(crate) order_books: BTreeMap<u64, SingleMarketBook>,
    pub(crate) resting_order_mandates: BTreeMap<(u64, u64, u64), MandateId>,
    pub(crate) markets: BTreeMap<u64, PerpetualMarket>,
    pub(crate) market_marks: BTreeMap<u64, OraclePrice>,
    pub(crate) market_index_prices: BTreeMap<u64, OraclePrice>,
    pub(crate) oracle_feed_submissions: BTreeMap<(u64, u64, [u8; 32]), OracleFeedSubmission>,
    pub(crate) mandates: BTreeMap<MandateId, Mandate>,
    pub(crate) mandates_by_agent: BTreeMap<u64, BTreeSet<MandateId>>,
    pub(crate) mandate_action_nonces: BTreeMap<Principal, u64>,
    pub(crate) mandate_realized_pnl: BTreeMap<MandateId, i128>,
    pub(crate) mandate_daily_pnl_baselines: BTreeMap<(MandateId, u64), i128>,
    pub(crate) account_operators: BTreeMap<u64, u64>,
    pub(crate) agent_identities: BTreeMap<u64, AgentIdentityRecord>,
    pub(crate) agent_identity_tombstones: BTreeSet<u64>,
    pub(crate) identity_action_nonces: BTreeMap<u64, u64>,
    pub(crate) identity_attestor_nonces: BTreeMap<u64, u64>,
    pub(crate) agent_attestations: BTreeMap<AttestationRecordId, AttestationRecord>,
    pub(crate) agent_attestations_by_account: BTreeMap<u64, BTreeSet<AttestationRecordId>>,
    pub(crate) tee_attestations_by_vendor_root:
        BTreeMap<VendorRootReverseIndexKey, VendorRootReverseIndexValue>,
    pub(crate) tee_vendor_roots: BTreeMap<TeeVendor, Vec<VendorRoot>>,
    pub(crate) tee_vendor_cap_profiles: BTreeMap<TeeVendor, VendorCapProfile>,
    pub(crate) market_maker_programs: BTreeMap<MarketMakerProgramId, MarketMakerProgram>,
    pub(crate) market_maker_enrollments: BTreeMap<MarketMakerEnrollmentKey, MarketMakerEnrollment>,
    pub(crate) live_market_maker_bonds_micro_usdc: u128,
    pub(crate) processed_market_maker_program_actions:
        BTreeMap<MarketMakerProgramActionId, ProcessedMarketMakerProgramAction>,
    pub(crate) pending_market_maker_program_actions:
        BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>,
    pub(crate) pending_market_maker_program_action_order:
        BTreeSet<PendingMarketMakerProgramActionOrder>,
    pub(crate) processed_tee_governance_actions:
        BTreeMap<TeeGovernanceActionId, ProcessedTeeGovernanceAction>,
    pub(crate) pending_tee_governance_actions:
        BTreeMap<TeeGovernanceActionId, PendingTeeGovernanceAction>,
    pub(crate) pending_tee_governance_action_order: BTreeSet<PendingTeeGovernanceActionOrder>,
    pub(crate) scheduled_tee_vendor_cap_profile_updates:
        BTreeMap<TeeGovernanceActionId, ScheduledTeeVendorCapProfileUpdate>,
    pub(crate) processed_real_capital_governance_actions:
        BTreeMap<RealCapitalGovernanceActionId, ProcessedRealCapitalGovernanceAction>,
    pub(crate) pending_real_capital_governance_actions:
        BTreeMap<RealCapitalGovernanceActionId, PendingRealCapitalGovernanceAction>,
    pub(crate) pending_real_capital_governance_action_order:
        BTreeSet<PendingRealCapitalGovernanceActionOrder>,
    pub(crate) pending_faucet_grants: BTreeMap<[u8; 20], PendingFaucetGrant>,
    pub(crate) pending_faucet_grant_order: BTreeSet<PendingFaucetGrantOrder>,
    pub(crate) delegations: BTreeMap<DelegationId, Delegation>,
    pub(crate) delegation_shares: BTreeMap<(DelegationId, u64), DelegationShare>,
    pub(crate) real_capital_listings: BTreeMap<VaultId, RealCapitalListing>,
    pub(crate) real_capital_listings_by_agent: BTreeMap<u64, BTreeSet<VaultId>>,
    pub(crate) vault_deposited_micro_usdc: BTreeMap<VaultId, u128>,
    pub(crate) vault_principal_deposited_micro_usdc: BTreeMap<(VaultId, u64), u128>,
    pub(crate) delegation_compensation_claims: BTreeMap<(DelegationId, u64), u128>,
    pub(crate) delegation_termination_payout_weights: BTreeMap<(DelegationId, u64), u128>,
    pub(crate) real_capital_bond_escrow_micro_usdc: BTreeMap<DelegationId, u128>,
    pub(crate) real_capital_compensation_escrow_micro_usdc: u128,
    pub(crate) real_capital_insurance_wallet_micro_usdc: u128,
    pub(crate) real_capital_insurance_fund_micro_usdc: u128,
    pub(crate) protocol_delegation_cap: Option<ProtocolDelegationCap>,
    pub(crate) listing_control_state: ListingControlState,
    pub(crate) redemption_requests: BTreeMap<RedemptionRequestId, RedemptionRequest>,
    pub(crate) processed_delegation_actions:
        BTreeMap<DelegationActionId, ProcessedDelegationAction>,
    pub(crate) pending_delegation_actions: BTreeMap<DelegationActionId, PendingDelegationAction>,
    pub(crate) pending_delegation_action_order: BTreeSet<PendingDelegationActionOrder>,
    pub(crate) market_oracle_config: BTreeMap<u64, OracleConfig>,
    pub(crate) market_risk: BTreeMap<u64, MarketRiskParameters>,
    pub(crate) market_fees: BTreeMap<u64, MarketFeeParameters>,
    pub(crate) paused_markets: BTreeSet<u64>,
    pub(crate) market_funding_index: BTreeMap<u64, i128>,
    pub(crate) market_last_funding_ms: BTreeMap<u64, u64>,
    pub(crate) insurance_funds: BTreeMap<u64, u128>,
    pub(crate) market_bad_debt_balances: BTreeMap<u64, u128>,
    pub(crate) account_market_bad_debt: BTreeMap<(u64, u64), u128>,
    pub(crate) fee_vaults: BTreeMap<u64, u128>,
    pub(crate) validator_reward_pools: BTreeMap<u64, u128>,
    pub(crate) market_maker_rebate_pools: BTreeMap<u64, u128>,
    pub(crate) treasury_balances: BTreeMap<u64, u128>,
    pub(crate) frozen_accounts: BTreeSet<u64>,
    pub(crate) under_maintenance_accounts: BTreeSet<(u64, u64)>,
    pub(crate) liquidation_escalation: BTreeMap<(u64, u64), LiquidationEscalationState>,
    pub(crate) isolated_collateral: BTreeMap<(u64, u64), i128>,
    pub(crate) positions: BTreeMap<(u64, u64), PositionLedger>,
    pub(crate) intent_mempool: DirectIntentMempool,
    pub(crate) conditional_pool: ConditionalIntentPool,
    pub(crate) runtime_events: Vec<RuntimeEventRecord>,
    pub(crate) evm_transaction_receipts: BTreeMap<Hash, EvmTransactionReceiptRecord>,
    pub(crate) next_runtime_event_seq: u64,
    pub(crate) telemetry: ConsensusTelemetry,
    pub(crate) suppress_fill_bankruptcy_reconcile: bool,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) struct PersistedSingleNodeEngineSnapshot {
    pub(crate) configured_validator_verifiers: Vec<ValidatorVerifier>,
    pub(crate) validator_set: Vec<ValidatorId>,
    pub(crate) validator_verifiers: Vec<ValidatorVerifier>,
    pub(crate) consensus_signing_keys: BTreeMap<ValidatorId, [u8; 32]>,
    pub(crate) pacemaker_config: PacemakerConfig,
    pub(crate) pacemaker: PacemakerState,
    pub(crate) chain: Vec<Block>,
    pub(crate) committed_state_root: Hash,
    pub(crate) committed_entries: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) quarantined_blocks: BTreeMap<Hash, PersistedQuarantinedBlock>,
    pub(crate) account_store: AccountStore,
    pub(crate) policy_store: PolicyStore,
    pub(crate) order_books: BTreeMap<u64, SingleMarketBook>,
    pub(crate) resting_order_mandates: BTreeMap<(u64, u64, u64), MandateId>,
    pub(crate) markets: BTreeMap<u64, PerpetualMarket>,
    pub(crate) market_marks: BTreeMap<u64, OraclePrice>,
    pub(crate) market_index_prices: BTreeMap<u64, OraclePrice>,
    pub(crate) oracle_feed_submissions: BTreeMap<(u64, u64, [u8; 32]), OracleFeedSubmission>,
    pub(crate) mandates: BTreeMap<MandateId, Mandate>,
    pub(crate) mandates_by_agent: BTreeMap<u64, BTreeSet<MandateId>>,
    pub(crate) mandate_action_nonces: BTreeMap<Principal, u64>,
    pub(crate) mandate_realized_pnl: BTreeMap<MandateId, i128>,
    pub(crate) mandate_daily_pnl_baselines: BTreeMap<(MandateId, u64), i128>,
    pub(crate) account_operators: BTreeMap<u64, u64>,
    pub(crate) agent_identities: BTreeMap<u64, AgentIdentityRecord>,
    pub(crate) agent_identity_tombstones: BTreeSet<u64>,
    pub(crate) identity_action_nonces: BTreeMap<u64, u64>,
    pub(crate) identity_attestor_nonces: BTreeMap<u64, u64>,
    pub(crate) agent_attestations: BTreeMap<AttestationRecordId, AttestationRecord>,
    pub(crate) agent_attestations_by_account: BTreeMap<u64, BTreeSet<AttestationRecordId>>,
    pub(crate) tee_attestations_by_vendor_root:
        BTreeMap<VendorRootReverseIndexKey, VendorRootReverseIndexValue>,
    pub(crate) tee_vendor_roots: BTreeMap<TeeVendor, Vec<VendorRoot>>,
    pub(crate) tee_vendor_cap_profiles: BTreeMap<TeeVendor, VendorCapProfile>,
    pub(crate) market_maker_programs: BTreeMap<MarketMakerProgramId, MarketMakerProgram>,
    pub(crate) market_maker_enrollments: BTreeMap<MarketMakerEnrollmentKey, MarketMakerEnrollment>,
    pub(crate) live_market_maker_bonds_micro_usdc: u128,
    pub(crate) processed_market_maker_program_actions:
        BTreeMap<MarketMakerProgramActionId, ProcessedMarketMakerProgramAction>,
    pub(crate) pending_market_maker_program_actions:
        BTreeMap<MarketMakerProgramActionId, PendingMarketMakerProgramAction>,
    pub(crate) pending_market_maker_program_action_order:
        BTreeSet<PendingMarketMakerProgramActionOrder>,
    pub(crate) processed_tee_governance_actions:
        BTreeMap<TeeGovernanceActionId, ProcessedTeeGovernanceAction>,
    pub(crate) pending_tee_governance_actions:
        BTreeMap<TeeGovernanceActionId, PendingTeeGovernanceAction>,
    pub(crate) pending_tee_governance_action_order: BTreeSet<PendingTeeGovernanceActionOrder>,
    pub(crate) scheduled_tee_vendor_cap_profile_updates:
        BTreeMap<TeeGovernanceActionId, ScheduledTeeVendorCapProfileUpdate>,
    pub(crate) processed_real_capital_governance_actions:
        BTreeMap<RealCapitalGovernanceActionId, ProcessedRealCapitalGovernanceAction>,
    pub(crate) pending_real_capital_governance_actions:
        BTreeMap<RealCapitalGovernanceActionId, PendingRealCapitalGovernanceAction>,
    pub(crate) pending_real_capital_governance_action_order:
        BTreeSet<PendingRealCapitalGovernanceActionOrder>,
    pub(crate) pending_faucet_grants: BTreeMap<[u8; 20], PendingFaucetGrant>,
    pub(crate) pending_faucet_grant_order: BTreeSet<PendingFaucetGrantOrder>,
    pub(crate) delegations: BTreeMap<DelegationId, Delegation>,
    pub(crate) delegation_shares: BTreeMap<(DelegationId, u64), DelegationShare>,
    pub(crate) real_capital_listings: BTreeMap<VaultId, RealCapitalListing>,
    pub(crate) real_capital_listings_by_agent: BTreeMap<u64, BTreeSet<VaultId>>,
    pub(crate) vault_deposited_micro_usdc: BTreeMap<VaultId, u128>,
    pub(crate) vault_principal_deposited_micro_usdc: BTreeMap<(VaultId, u64), u128>,
    pub(crate) delegation_compensation_claims: BTreeMap<(DelegationId, u64), u128>,
    pub(crate) delegation_termination_payout_weights: BTreeMap<(DelegationId, u64), u128>,
    pub(crate) real_capital_bond_escrow_micro_usdc: BTreeMap<DelegationId, u128>,
    pub(crate) real_capital_compensation_escrow_micro_usdc: u128,
    pub(crate) real_capital_insurance_wallet_micro_usdc: u128,
    pub(crate) real_capital_insurance_fund_micro_usdc: u128,
    pub(crate) protocol_delegation_cap: Option<ProtocolDelegationCap>,
    pub(crate) listing_control_state: ListingControlState,
    pub(crate) redemption_requests: BTreeMap<RedemptionRequestId, RedemptionRequest>,
    pub(crate) processed_delegation_actions:
        BTreeMap<DelegationActionId, ProcessedDelegationAction>,
    pub(crate) pending_delegation_actions: BTreeMap<DelegationActionId, PendingDelegationAction>,
    pub(crate) pending_delegation_action_order: BTreeSet<PendingDelegationActionOrder>,
    pub(crate) market_oracle_config: BTreeMap<u64, OracleConfig>,
    pub(crate) market_risk: BTreeMap<u64, MarketRiskParameters>,
    pub(crate) market_fees: BTreeMap<u64, MarketFeeParameters>,
    pub(crate) paused_markets: BTreeSet<u64>,
    pub(crate) market_funding_index: BTreeMap<u64, i128>,
    pub(crate) market_last_funding_ms: BTreeMap<u64, u64>,
    pub(crate) insurance_funds: BTreeMap<u64, u128>,
    pub(crate) market_bad_debt_balances: BTreeMap<u64, u128>,
    pub(crate) account_market_bad_debt: BTreeMap<(u64, u64), u128>,
    pub(crate) fee_vaults: BTreeMap<u64, u128>,
    pub(crate) validator_reward_pools: BTreeMap<u64, u128>,
    pub(crate) market_maker_rebate_pools: BTreeMap<u64, u128>,
    pub(crate) treasury_balances: BTreeMap<u64, u128>,
    pub(crate) frozen_accounts: BTreeSet<u64>,
    pub(crate) under_maintenance_accounts: BTreeSet<(u64, u64)>,
    pub(crate) liquidation_escalation: BTreeMap<(u64, u64), LiquidationEscalationState>,
    pub(crate) isolated_collateral: BTreeMap<(u64, u64), i128>,
    pub(crate) positions: BTreeMap<(u64, u64), PositionLedger>,
    pub(crate) intent_mempool: DirectIntentMempool,
    pub(crate) conditional_pool: ConditionalIntentPool,
    pub(crate) runtime_events: Vec<RuntimeEventRecord>,
    pub(crate) evm_transaction_receipts: BTreeMap<Hash, EvmTransactionReceiptRecord>,
    pub(crate) next_runtime_event_seq: u64,
    pub(crate) telemetry: ConsensusTelemetry,
    pub(crate) suppress_fill_bankruptcy_reconcile: bool,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) struct PersistedQuarantinedBlock {
    pub(crate) block_hash: Hash,
    pub(crate) height: u64,
    pub(crate) reason: PersistedQuarantineReason,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) enum PersistedQuarantineReason {
    AdmissionViolation(String),
    ParentStateRootMismatch,
    StateRootMismatch,
    StorageCorruption,
    ExecutionRevert,
}

impl PersistedQuarantinedBlock {
    pub(crate) fn from_runtime(block: &kombat_chain_execution::QuarantinedBlock) -> Self {
        Self {
            block_hash: block.block_hash,
            height: block.height,
            reason: PersistedQuarantineReason::from_runtime(&block.reason),
        }
    }

    pub(crate) fn into_runtime(self) -> kombat_chain_execution::QuarantinedBlock {
        kombat_chain_execution::QuarantinedBlock {
            block_hash: self.block_hash,
            height: self.height,
            reason: self.reason.into_runtime(),
        }
    }
}

impl PersistedQuarantineReason {
    fn from_runtime(reason: &kombat_chain_execution::QuarantineReason) -> Self {
        match reason {
            kombat_chain_execution::QuarantineReason::AdmissionViolation(reason) => {
                Self::AdmissionViolation((*reason).to_string())
            }
            kombat_chain_execution::QuarantineReason::ParentStateRootMismatch => {
                Self::ParentStateRootMismatch
            }
            kombat_chain_execution::QuarantineReason::StateRootMismatch => Self::StateRootMismatch,
            kombat_chain_execution::QuarantineReason::StorageCorruption => Self::StorageCorruption,
            kombat_chain_execution::QuarantineReason::ExecutionRevert => Self::ExecutionRevert,
        }
    }

    fn into_runtime(self) -> kombat_chain_execution::QuarantineReason {
        match self {
            Self::AdmissionViolation(reason) => {
                let leaked = Box::leak(reason.into_boxed_str());
                kombat_chain_execution::QuarantineReason::AdmissionViolation(leaked)
            }
            Self::ParentStateRootMismatch => {
                kombat_chain_execution::QuarantineReason::ParentStateRootMismatch
            }
            Self::StateRootMismatch => kombat_chain_execution::QuarantineReason::StateRootMismatch,
            Self::StorageCorruption => kombat_chain_execution::QuarantineReason::StorageCorruption,
            Self::ExecutionRevert => kombat_chain_execution::QuarantineReason::ExecutionRevert,
        }
    }
}

pub(crate) fn write_snapshot_file(
    path: impl AsRef<Path>,
    bytes: &[u8],
) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let timestamp_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "snapshot path missing file name",
        )
    })?;
    let temp_name = format!(".{}.{}.tmp", file_name.to_string_lossy(), timestamp_nanos);
    let temp_path = path.with_file_name(temp_name);

    let mut file = File::create(&temp_path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temp_path, path)?;
    sync_parent_directory(path)?;
    Ok(())
}
