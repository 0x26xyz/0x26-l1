//! RFC 056 legacy policy authority-scope migration.
//!
//! This module is the append-only home for the pre-RFC056 engine
//! canonical record layout used by RFC 057 migrations. The current
//! implementation still aliases unchanged nested persisted records to
//! their live crate types; the migration-specific policy layout and
//! canonical record version are pinned here. The fixture directory
//! contains real pre-RFC056 bytes captured from commit `bab6253`; because
//! that snapshot predates RFC055 fingerprint envelopes, command tests
//! extract its legacy engine record and wrap it in a test-only current
//! envelope. A production edge for a non-resettable network must replace
//! the live-type aliases with fully copied source-fingerprint types before
//! registering the edge.

use crate::persistence::{
    EngineCanonicalRecord, ENGINE_CANONICAL_KEY, ENGINE_CANONICAL_RECORD_VERSION,
};
use crate::telemetry::ConsensusTelemetry;
use crate::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    Block, DelegationActionId, Hash, MarketMakerProgramActionId, PacemakerConfig, PacemakerState,
    RealCapitalGovernanceActionId, TeeGovernanceActionId, ValidatorId, ValidatorVerifier,
};
use kombat_chain_matching::SingleMarketBook;
use kombat_chain_oracle::{OracleConfig, OraclePrice};
use kombat_chain_policy::{
    AccountStore, DelegationReservations, PolicyAdmissionRules, PolicyCounters,
    PolicyExecutionScope, PolicyKeyState, PolicyOrigin, PolicyRecord, PolicyState, PolicyStore,
    PolicyTerms, Principal,
};
use kombat_chain_state_db::Namespace;
use kombat_chain_streams::RuntimeEventRecord;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) struct LegacyRfc056PersistedSingleNodeEngineSnapshot {
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
    pub(crate) policy_store: LegacyRfc056PolicyStore,
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

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub(crate) struct LegacyRfc056PolicyStore {
    pub(crate) policies: BTreeMap<u64, LegacyRfc056PolicyRecord>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) struct LegacyRfc056PolicyRecord {
    pub(crate) id: u64,
    pub(crate) account: u64,
    pub(crate) parent_policy_id: Option<u64>,
    pub(crate) delegation_depth: u8,
    pub(crate) key_state: PolicyKeyState,
    pub(crate) next_intent_nonce: u64,
    pub(crate) created_at: u64,
    pub(crate) expires_at: u64,
    pub(crate) state: PolicyState,
    pub(crate) terms: PolicyTerms,
    pub(crate) admission_rules: PolicyAdmissionRules,
    pub(crate) origin: PolicyOrigin,
    pub(crate) reserved: DelegationReservations,
    pub(crate) counters: PolicyCounters,
    pub(crate) revoked_at: Option<u64>,
}

impl TryFrom<LegacyRfc056PersistedSingleNodeEngineSnapshot> for PersistedSingleNodeEngineSnapshot {
    type Error = String;

    fn try_from(
        snapshot: LegacyRfc056PersistedSingleNodeEngineSnapshot,
    ) -> Result<Self, Self::Error> {
        let mut policy_store = PolicyStore::default();
        for (_, policy) in snapshot.policy_store.policies {
            policy_store
                .insert(policy.into_current())
                .map_err(|error| format!("{error:?}"))?;
        }

        Ok(Self {
            configured_validator_verifiers: snapshot.configured_validator_verifiers,
            validator_set: snapshot.validator_set,
            validator_verifiers: snapshot.validator_verifiers,
            consensus_signing_keys: snapshot.consensus_signing_keys,
            pacemaker_config: snapshot.pacemaker_config,
            pacemaker: snapshot.pacemaker,
            chain: snapshot.chain,
            committed_state_root: snapshot.committed_state_root,
            committed_entries: snapshot.committed_entries,
            quarantined_blocks: snapshot.quarantined_blocks,
            account_store: snapshot.account_store,
            policy_store,
            order_books: snapshot.order_books,
            resting_order_mandates: snapshot.resting_order_mandates,
            markets: snapshot.markets,
            market_marks: snapshot.market_marks,
            market_index_prices: snapshot.market_index_prices,
            oracle_feed_submissions: snapshot.oracle_feed_submissions,
            mandates: snapshot.mandates,
            mandates_by_agent: snapshot.mandates_by_agent,
            mandate_action_nonces: snapshot.mandate_action_nonces,
            mandate_realized_pnl: snapshot.mandate_realized_pnl,
            mandate_daily_pnl_baselines: snapshot.mandate_daily_pnl_baselines,
            account_operators: snapshot.account_operators,
            agent_identities: snapshot.agent_identities,
            agent_identity_tombstones: snapshot.agent_identity_tombstones,
            identity_action_nonces: snapshot.identity_action_nonces,
            identity_attestor_nonces: snapshot.identity_attestor_nonces,
            agent_attestations: snapshot.agent_attestations,
            agent_attestations_by_account: snapshot.agent_attestations_by_account,
            tee_attestations_by_vendor_root: snapshot.tee_attestations_by_vendor_root,
            tee_vendor_roots: snapshot.tee_vendor_roots,
            tee_vendor_cap_profiles: snapshot.tee_vendor_cap_profiles,
            market_maker_programs: snapshot.market_maker_programs,
            market_maker_enrollments: snapshot.market_maker_enrollments,
            live_market_maker_bonds_micro_usdc: snapshot.live_market_maker_bonds_micro_usdc,
            processed_market_maker_program_actions: snapshot.processed_market_maker_program_actions,
            pending_market_maker_program_actions: snapshot.pending_market_maker_program_actions,
            pending_market_maker_program_action_order: snapshot
                .pending_market_maker_program_action_order,
            processed_tee_governance_actions: snapshot.processed_tee_governance_actions,
            pending_tee_governance_actions: snapshot.pending_tee_governance_actions,
            pending_tee_governance_action_order: snapshot.pending_tee_governance_action_order,
            scheduled_tee_vendor_cap_profile_updates: snapshot
                .scheduled_tee_vendor_cap_profile_updates,
            processed_real_capital_governance_actions: snapshot
                .processed_real_capital_governance_actions,
            pending_real_capital_governance_actions: snapshot
                .pending_real_capital_governance_actions,
            pending_real_capital_governance_action_order: snapshot
                .pending_real_capital_governance_action_order,
            pending_faucet_grants: snapshot.pending_faucet_grants,
            pending_faucet_grant_order: snapshot.pending_faucet_grant_order,
            delegations: snapshot.delegations,
            delegation_shares: snapshot.delegation_shares,
            real_capital_listings: snapshot.real_capital_listings,
            real_capital_listings_by_agent: snapshot.real_capital_listings_by_agent,
            vault_deposited_micro_usdc: snapshot.vault_deposited_micro_usdc,
            vault_principal_deposited_micro_usdc: snapshot.vault_principal_deposited_micro_usdc,
            delegation_compensation_claims: snapshot.delegation_compensation_claims,
            delegation_termination_payout_weights: snapshot.delegation_termination_payout_weights,
            real_capital_bond_escrow_micro_usdc: snapshot.real_capital_bond_escrow_micro_usdc,
            real_capital_compensation_escrow_micro_usdc: snapshot
                .real_capital_compensation_escrow_micro_usdc,
            real_capital_insurance_wallet_micro_usdc: snapshot
                .real_capital_insurance_wallet_micro_usdc,
            real_capital_insurance_fund_micro_usdc: snapshot.real_capital_insurance_fund_micro_usdc,
            protocol_delegation_cap: snapshot.protocol_delegation_cap,
            listing_control_state: snapshot.listing_control_state,
            redemption_requests: snapshot.redemption_requests,
            processed_delegation_actions: snapshot.processed_delegation_actions,
            pending_delegation_actions: snapshot.pending_delegation_actions,
            pending_delegation_action_order: snapshot.pending_delegation_action_order,
            market_oracle_config: snapshot.market_oracle_config,
            market_risk: snapshot.market_risk,
            market_fees: snapshot.market_fees,
            paused_markets: snapshot.paused_markets,
            market_funding_index: snapshot.market_funding_index,
            market_last_funding_ms: snapshot.market_last_funding_ms,
            insurance_funds: snapshot.insurance_funds,
            market_bad_debt_balances: snapshot.market_bad_debt_balances,
            account_market_bad_debt: snapshot.account_market_bad_debt,
            fee_vaults: snapshot.fee_vaults,
            validator_reward_pools: snapshot.validator_reward_pools,
            market_maker_rebate_pools: snapshot.market_maker_rebate_pools,
            treasury_balances: snapshot.treasury_balances,
            frozen_accounts: snapshot.frozen_accounts,
            under_maintenance_accounts: snapshot.under_maintenance_accounts,
            liquidation_escalation: snapshot.liquidation_escalation,
            isolated_collateral: snapshot.isolated_collateral,
            positions: snapshot.positions,
            intent_mempool: snapshot.intent_mempool,
            conditional_pool: snapshot.conditional_pool,
            runtime_events: snapshot.runtime_events,
            evm_transaction_receipts: snapshot.evm_transaction_receipts,
            next_runtime_event_seq: snapshot.next_runtime_event_seq,
            telemetry: snapshot.telemetry,
            suppress_fill_bankruptcy_reconcile: snapshot.suppress_fill_bankruptcy_reconcile,
        })
    }
}

impl LegacyRfc056PolicyRecord {
    fn into_current(self) -> PolicyRecord {
        PolicyRecord {
            id: self.id,
            authority_account: self.account,
            execution_scope: PolicyExecutionScope::Account(self.account),
            parent_policy_id: self.parent_policy_id,
            delegation_depth: self.delegation_depth,
            key_state: self.key_state,
            next_intent_nonce: self.next_intent_nonce,
            created_at: self.created_at,
            expires_at: self.expires_at,
            state: self.state,
            terms: self.terms,
            admission_rules: self.admission_rules,
            origin: self.origin,
            reserved: self.reserved,
            counters: self.counters,
            revoked_at: self.revoked_at,
        }
    }
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub(crate) struct LegacyRfc056EngineCanonicalRecord {
    pub(crate) version: u32,
    pub(crate) inner: LegacyRfc056PersistedSingleNodeEngineSnapshot,
}

const LEGACY_RFC056_ENGINE_CANONICAL_RECORD_VERSION: u32 = 1;

pub(crate) fn migrate_rfc056_policy_authority_scope_record(
    namespace: Namespace,
    key: &[u8],
    value: &[u8],
) -> Result<Vec<u8>, String> {
    if namespace != Namespace::State || key != ENGINE_CANONICAL_KEY {
        return Ok(value.to_vec());
    }

    let legacy: LegacyRfc056EngineCanonicalRecord = borsh::from_slice(value)
        .map_err(|error| format!("decode legacy RFC056 EngineCanonicalRecord: {error}"))?;
    if legacy.version != LEGACY_RFC056_ENGINE_CANONICAL_RECORD_VERSION {
        return Err(format!(
            "legacy RFC056 engine canonical version mismatch: expected {}, actual {}",
            LEGACY_RFC056_ENGINE_CANONICAL_RECORD_VERSION, legacy.version
        ));
    }

    let inner = PersistedSingleNodeEngineSnapshot::try_from(legacy.inner)?;
    let migrated = EngineCanonicalRecord {
        version: ENGINE_CANONICAL_RECORD_VERSION,
        inner,
    };
    borsh::to_vec(&migrated)
        .map_err(|error| format!("encode migrated RFC056 EngineCanonicalRecord: {error}"))
}
