use crate::{
    account_id_is_delegation_reserved, delegation_trading_account_id,
    live_delegation_for_trading_account, Delegation, DelegationId, MarketStatus, NodeError,
    PerpetualMarket, BPS_DENOMINATOR, COMPLIANCE_WINDOW_MAX_MS, MAX_MM_PROGRAM_MARKETS,
    OBSERVATION_CADENCE_MAX_MS, OBSERVATION_CADENCE_MIN_MS, PROGRAM_BOND_MIN_MICRO_USDC,
    PROGRAM_GRACE_WINDOW_MS, PROGRAM_REENTER_COOLDOWN_MS, REBATE_MULTIPLIER_BASE_UNITS,
    REBATE_MULTIPLIER_CAP_UNITS,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    market_maker_program_block_action_kind, BlockMarketMakerProgramAction, Hash,
    MarketMakerIdentity, MarketMakerProgramActionId,
};
use kombat_chain_streams::RuntimeEventKind;
use revm::primitives::keccak256;
use std::collections::{BTreeMap, BTreeSet};

pub type MarketMakerProgramId = [u8; 32];
pub type MarketMakerEnrollmentKey = (MarketMakerIdentity, u64, MarketMakerProgramId);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketMakerU256(pub [u8; 32]);

impl MarketMakerU256 {
    pub fn from_u128(value: u128) -> Self {
        let mut bytes = [0_u8; 32];
        bytes[16..].copy_from_slice(&value.to_be_bytes());
        Self(bytes)
    }

    pub(crate) fn as_u128(self) -> Option<u128> {
        if self.0[..16].iter().any(|byte| *byte != 0) {
            return None;
        }
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&self.0[16..]);
        Some(u128::from_be_bytes(bytes))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MarketMakerProgramStatus {
    Active,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MarketMakerEnrollmentStatus {
    Active,
    GracePeriod,
    NonCompliant,
    Exited,
    Slashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MarketMakerBreachKind {
    Uptime,
    Spread,
    Size,
    TwoSidedWindows,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketMakerComplianceState {
    pub window_start_ms: u64,
    pub window_end_ms: u64,
    pub uptime_bps_current: u16,
    pub avg_spread_bps_current: u16,
    pub median_quote_size_current: MarketMakerU256,
    pub two_sided_windows_last_h: u16,
    pub last_window_breach: Option<MarketMakerBreachKind>,
    pub breach_counter: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketMakerObservationSample {
    pub observed_at_ms: u64,
    pub bid_present: bool,
    pub ask_present: bool,
    pub two_sided: bool,
    pub spread_bps: u16,
    pub quote_size_micro_usdc: MarketMakerU256,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketMakerProgram {
    pub program_id: MarketMakerProgramId,
    pub name: String,
    pub markets: BTreeSet<u64>,
    pub defined_at_ms: u64,
    pub last_updated_at_ms: u64,
    pub status: MarketMakerProgramStatus,
    pub min_uptime_bps: u16,
    pub max_spread_bps: u16,
    pub min_quote_size_micro_usdc: MarketMakerU256,
    pub min_two_sided_windows_per_hour: u16,
    pub rebate_multiplier_units: u16,
    pub enrollment_bond_micro_usdc: MarketMakerU256,
    pub compliance_window_ms: u64,
    pub observation_cadence_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MarketMakerEnrollment {
    pub market_maker_identity: MarketMakerIdentity,
    pub market_id: u64,
    pub program_id: MarketMakerProgramId,
    pub enrolled_at_ms: u64,
    pub last_updated_at_ms: u64,
    pub status: MarketMakerEnrollmentStatus,
    pub bond_micro_usdc: MarketMakerU256,
    pub current_compliance: MarketMakerComplianceState,
    pub observation_history: Vec<MarketMakerObservationSample>,
    pub last_compliance_checked_ms: u64,
    /// RFC 027 multiplier bonus credited while `Active`.
    ///
    /// This excludes the baseline maker rebate, which settles separately on
    /// every maker fill whether or not the enrollment is compliant.
    pub lifetime_compliant_rebates_micro_usdc: MarketMakerU256,
    /// RFC 027 multiplier bonus forfeited while `GracePeriod` /
    /// `NonCompliant`.
    ///
    /// This excludes the baseline maker rebate, which still pays during those
    /// statuses.
    pub lifetime_forfeited_rebates_micro_usdc: MarketMakerU256,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct DefineMarketMakerProgramInput {
    pub name: String,
    pub markets: BTreeSet<u64>,
    pub min_uptime_bps: u16,
    pub max_spread_bps: u16,
    pub min_quote_size_micro_usdc: MarketMakerU256,
    pub min_two_sided_windows_per_hour: u16,
    pub rebate_multiplier_units: u16,
    pub enrollment_bond_micro_usdc: MarketMakerU256,
    pub compliance_window_ms: u64,
    pub observation_cadence_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct EnrollMarketMakerInput {
    pub market_maker_identity: MarketMakerIdentity,
    pub market_id: u64,
    pub program_id: MarketMakerProgramId,
    pub bond_micro_usdc: MarketMakerU256,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedMarketMakerProgramAction {
    pub chain_id: u64,
    pub signer_account_id: u64,
    pub action_id: MarketMakerProgramActionId,
    pub timestamp_ms: u64,
    pub submitter_nonce: Option<u64>,
    pub action: BlockMarketMakerProgramAction,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, BorshSerialize)]
struct MarketMakerProgramActionPayload {
    domain: String,
    chain_id: u64,
    signer_account_id: u64,
    action_id: MarketMakerProgramActionId,
    action_kind: String,
    action_scope: Vec<u8>,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
    body_hash: Hash,
}

#[derive(Clone, Debug, BorshSerialize)]
struct MarketMakerProgramIdMaterial {
    name: String,
    markets: BTreeSet<u64>,
    min_uptime_bps: u16,
    max_spread_bps: u16,
    min_quote_size_micro_usdc: MarketMakerU256,
    min_two_sided_windows_per_hour: u16,
    rebate_multiplier_units: u16,
    enrollment_bond_micro_usdc: MarketMakerU256,
    compliance_window_ms: u64,
    observation_cadence_ms: u64,
}

pub fn derive_market_maker_program_id(
    input: &DefineMarketMakerProgramInput,
) -> MarketMakerProgramId {
    let material = MarketMakerProgramIdMaterial {
        name: input.name.clone(),
        markets: input.markets.clone(),
        min_uptime_bps: input.min_uptime_bps,
        max_spread_bps: input.max_spread_bps,
        min_quote_size_micro_usdc: input.min_quote_size_micro_usdc,
        min_two_sided_windows_per_hour: input.min_two_sided_windows_per_hour,
        rebate_multiplier_units: input.rebate_multiplier_units,
        enrollment_bond_micro_usdc: input.enrollment_bond_micro_usdc,
        compliance_window_ms: input.compliance_window_ms,
        observation_cadence_ms: input.observation_cadence_ms,
    };
    keccak256(
        [
            b"mm-program".as_slice(),
            &borsh::to_vec(&material).expect("market-maker program id material should serialize"),
        ]
        .concat(),
    )
    .into()
}

pub fn market_maker_program_action_id_for_submission(
    action: &BlockMarketMakerProgramAction,
    timestamp_ms: u64,
    submitter_nonce: Option<u64>,
) -> MarketMakerProgramActionId {
    if let Some(nonce) = submitter_nonce {
        derive_market_maker_program_action_id_from_nonce(
            market_maker_program_block_action_kind(action),
            &market_maker_program_action_scope(action),
            nonce,
        )
    } else {
        derive_legacy_market_maker_program_action_id(action, timestamp_ms)
    }
}

/// Stable wire contract; changing this breaks in-flight retry idempotency.
pub fn derive_market_maker_program_action_id_from_nonce(
    action_kind: &str,
    scope: &[u8],
    nonce: u64,
) -> MarketMakerProgramActionId {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"mm-program-action-id-v1");
    payload.extend_from_slice(action_kind.as_bytes());
    payload.extend_from_slice(scope);
    payload.extend_from_slice(&nonce.to_be_bytes());
    *keccak256(payload)
}

/// Stable wire contract; changing this breaks legacy timestamp-keyed retries.
pub fn derive_legacy_market_maker_program_action_id(
    action: &BlockMarketMakerProgramAction,
    timestamp_ms: u64,
) -> MarketMakerProgramActionId {
    let action_bytes = borsh::to_vec(action).expect("market-maker program action should serialize");
    let mut payload = Vec::new();
    payload.extend_from_slice(b"legacy-mm-program-action-id");
    payload.extend_from_slice(&timestamp_ms.to_be_bytes());
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

/// Stable action-body fingerprint. Changing this breaks replay protection for
/// persisted market-maker program action receipts.
pub fn market_maker_program_action_fingerprint(action: &BlockMarketMakerProgramAction) -> Hash {
    let action_bytes = borsh::to_vec(action).expect("market-maker program action should serialize");
    let mut payload = Vec::new();
    payload.extend_from_slice(b"mm-program-action-fingerprint");
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

pub fn market_maker_program_action_signing_bytes(
    signed_action: &SignedMarketMakerProgramAction,
) -> Vec<u8> {
    let payload = MarketMakerProgramActionPayload {
        domain: "0x26:market-maker-program-action:v1".into(),
        chain_id: signed_action.chain_id,
        signer_account_id: signed_action.signer_account_id,
        action_id: signed_action.action_id,
        action_kind: market_maker_program_block_action_kind(&signed_action.action).to_string(),
        action_scope: market_maker_program_action_scope(&signed_action.action),
        timestamp_ms: signed_action.timestamp_ms,
        submitter_nonce: signed_action.submitter_nonce,
        body_hash: market_maker_program_action_fingerprint(&signed_action.action),
    };
    borsh::to_vec(&payload).expect("market-maker program action payload should serialize")
}

fn market_maker_program_action_scope(action: &BlockMarketMakerProgramAction) -> Vec<u8> {
    let mut scope = Vec::new();
    match action {
        BlockMarketMakerProgramAction::Define { .. } => {
            scope.extend_from_slice(&market_maker_program_action_program_id(action));
        }
        BlockMarketMakerProgramAction::Retire { program_id } => {
            scope.extend_from_slice(program_id);
        }
        BlockMarketMakerProgramAction::Enroll {
            market_maker_identity,
            market_id,
            program_id,
            ..
        }
        | BlockMarketMakerProgramAction::Exit {
            market_maker_identity,
            market_id,
            program_id,
        } => {
            scope.extend_from_slice(&market_maker_identity_scope_bytes(*market_maker_identity));
            scope.extend_from_slice(&market_id.to_be_bytes());
            scope.extend_from_slice(program_id);
        }
    }
    scope
}

pub(crate) fn market_maker_program_action_program_id(
    action: &BlockMarketMakerProgramAction,
) -> MarketMakerProgramId {
    match action {
        BlockMarketMakerProgramAction::Define {
            name,
            markets,
            min_uptime_bps,
            max_spread_bps,
            min_quote_size_micro_usdc,
            min_two_sided_windows_per_hour,
            rebate_multiplier_units,
            enrollment_bond_micro_usdc,
            compliance_window_ms,
            observation_cadence_ms,
        } => derive_market_maker_program_id(&DefineMarketMakerProgramInput {
            name: name.clone(),
            markets: markets.clone(),
            min_uptime_bps: *min_uptime_bps,
            max_spread_bps: *max_spread_bps,
            min_quote_size_micro_usdc: MarketMakerU256(*min_quote_size_micro_usdc),
            min_two_sided_windows_per_hour: *min_two_sided_windows_per_hour,
            rebate_multiplier_units: *rebate_multiplier_units,
            enrollment_bond_micro_usdc: MarketMakerU256(*enrollment_bond_micro_usdc),
            compliance_window_ms: *compliance_window_ms,
            observation_cadence_ms: *observation_cadence_ms,
        }),
        BlockMarketMakerProgramAction::Retire { program_id }
        | BlockMarketMakerProgramAction::Enroll { program_id, .. }
        | BlockMarketMakerProgramAction::Exit { program_id, .. } => *program_id,
    }
}

pub(crate) fn market_maker_identity_signer_account(identity: MarketMakerIdentity) -> u64 {
    match identity {
        MarketMakerIdentity::Account(account_id) => account_id,
        MarketMakerIdentity::Delegation { serving_agent, .. } => serving_agent,
    }
}

pub(crate) fn market_maker_identity_economic_account(identity: MarketMakerIdentity) -> u64 {
    match identity {
        MarketMakerIdentity::Account(account_id) => account_id,
        MarketMakerIdentity::Delegation { delegation_id, .. } => {
            delegation_trading_account_id(&delegation_id)
        }
    }
}

fn market_maker_identity_scope_bytes(identity: MarketMakerIdentity) -> Vec<u8> {
    match identity {
        // Preserve the legacy scope contract for naked account identities so
        // existing retry/idempotency behavior is unchanged for RFC 027 v1.
        // This account form is deliberately unprefixed. Delegation identities
        // are the first prefixed extension and always begin with 0x01. Any
        // future account-scope re-encoding must bump the action-id/scope
        // contract rather than silently reusing a 0x00-prefixed form.
        MarketMakerIdentity::Account(account_id) => account_id.to_be_bytes().to_vec(),
        MarketMakerIdentity::Delegation {
            delegation_id,
            serving_agent,
        } => {
            let mut bytes = Vec::with_capacity(1 + delegation_id.len() + 8);
            bytes.push(1);
            bytes.extend_from_slice(&delegation_id);
            bytes.extend_from_slice(&serving_agent.to_be_bytes());
            bytes
        }
    }
}

pub(crate) fn market_maker_enrollment_key(
    market_maker_identity: MarketMakerIdentity,
    market_id: u64,
    program_id: MarketMakerProgramId,
) -> MarketMakerEnrollmentKey {
    (market_maker_identity, market_id, program_id)
}

pub(crate) fn market_maker_enrollment_status_is_live(status: MarketMakerEnrollmentStatus) -> bool {
    matches!(
        status,
        MarketMakerEnrollmentStatus::Active
            | MarketMakerEnrollmentStatus::GracePeriod
            | MarketMakerEnrollmentStatus::NonCompliant
    )
}

pub(crate) fn market_maker_enrollment_status_selection_priority(
    status: MarketMakerEnrollmentStatus,
) -> u8 {
    match status {
        MarketMakerEnrollmentStatus::Active => 1,
        MarketMakerEnrollmentStatus::GracePeriod | MarketMakerEnrollmentStatus::NonCompliant => 0,
        MarketMakerEnrollmentStatus::Exited | MarketMakerEnrollmentStatus::Slashed => {
            unreachable!("selection priority is only used for live enrollments")
        }
    }
}

pub(crate) fn market_maker_compliance_history_retention_ms(program: &MarketMakerProgram) -> u64 {
    program.compliance_window_ms.max(3_600_000)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn market_maker_enrollment_status_is_multiplier_eligible(
    status: MarketMakerEnrollmentStatus,
) -> bool {
    matches!(status, MarketMakerEnrollmentStatus::Active)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn market_maker_identity_for_economic_account(
    delegations: &BTreeMap<DelegationId, Delegation>,
    account_id: u64,
) -> Option<MarketMakerIdentity> {
    if let Some(delegation) = live_delegation_for_trading_account(delegations, account_id) {
        return Some(MarketMakerIdentity::Delegation {
            delegation_id: delegation.delegation_id,
            serving_agent: delegation.agent,
        });
    }
    if account_id_is_delegation_reserved(account_id) {
        return None;
    }
    Some(MarketMakerIdentity::Account(account_id))
}

pub(crate) fn next_market_maker_observation_slot(
    enrollment: &MarketMakerEnrollment,
    program: &MarketMakerProgram,
    timestamp_ms: u64,
) -> Option<u64> {
    let observed_at_ms = timestamp_ms
        .checked_div(program.observation_cadence_ms)?
        .checked_mul(program.observation_cadence_ms)?;
    if observed_at_ms < enrollment.enrolled_at_ms {
        return None;
    }
    if enrollment
        .observation_history
        .last()
        .is_some_and(|sample| sample.observed_at_ms >= observed_at_ms)
    {
        return None;
    }
    Some(observed_at_ms)
}

pub(crate) fn market_maker_spread_bps(bid_price_ticks: u64, ask_price_ticks: u64) -> u16 {
    if ask_price_ticks <= bid_price_ticks {
        return 0;
    }
    let numerator = u128::from(ask_price_ticks.saturating_sub(bid_price_ticks))
        .saturating_mul(2)
        .saturating_mul(BPS_DENOMINATOR);
    let denominator = u128::from(ask_price_ticks).saturating_add(u128::from(bid_price_ticks));
    if denominator == 0 {
        return 0;
    }
    let rounded_up = numerator
        .saturating_add(denominator.saturating_sub(1))
        .saturating_div(denominator);
    rounded_up
        .min(BPS_DENOMINATOR)
        .try_into()
        .unwrap_or(u16::MAX)
}

fn market_maker_first_breach_kind(
    uptime_bps_current: u16,
    avg_spread_bps_current: u16,
    median_quote_size_current: MarketMakerU256,
    two_sided_windows_last_h: u16,
    program: &MarketMakerProgram,
) -> Option<MarketMakerBreachKind> {
    if uptime_bps_current < program.min_uptime_bps {
        return Some(MarketMakerBreachKind::Uptime);
    }
    if median_quote_size_current
        .as_u128()
        .zip(program.min_quote_size_micro_usdc.as_u128())
        .is_none_or(|(actual, min_required)| actual < min_required)
    {
        return Some(MarketMakerBreachKind::Size);
    }
    if two_sided_windows_last_h < program.min_two_sided_windows_per_hour {
        return Some(MarketMakerBreachKind::TwoSidedWindows);
    }
    if avg_spread_bps_current > program.max_spread_bps {
        return Some(MarketMakerBreachKind::Spread);
    }
    None
}

pub(crate) fn aggregate_market_maker_compliance(
    observation_history: &[MarketMakerObservationSample],
    program: &MarketMakerProgram,
    observed_at_ms: u64,
) -> MarketMakerComplianceState {
    let window_start_ms = observed_at_ms.saturating_sub(program.compliance_window_ms);
    let last_hour_start_ms = observed_at_ms.saturating_sub(3_600_000);
    let compliance_samples: Vec<&MarketMakerObservationSample> = observation_history
        .iter()
        .filter(|sample| sample.observed_at_ms > window_start_ms)
        .collect();
    let hourly_two_sided_count = observation_history
        .iter()
        .filter(|sample| sample.observed_at_ms > last_hour_start_ms && sample.two_sided)
        .count();
    let uptime_sample_count = compliance_samples
        .iter()
        .filter(|sample| sample.bid_present || sample.ask_present)
        .count();
    let spread_samples: Vec<&MarketMakerObservationSample> = compliance_samples
        .iter()
        .copied()
        .filter(|sample| sample.two_sided)
        .collect();
    let avg_spread_bps_current = if spread_samples.is_empty() {
        0
    } else {
        let total = spread_samples.iter().fold(0_u128, |acc, sample| {
            acc.saturating_add(u128::from(sample.spread_bps))
        });
        let average = total.saturating_div(spread_samples.len() as u128);
        average.min(BPS_DENOMINATOR).try_into().unwrap_or(u16::MAX)
    };
    let mut quote_sizes: Vec<u128> = compliance_samples
        .iter()
        .filter_map(|sample| sample.quote_size_micro_usdc.as_u128())
        .collect();
    quote_sizes.sort_unstable();
    let median_quote_size_current = quote_sizes
        .get(quote_sizes.len().saturating_sub(1) / 2)
        .copied()
        .map(MarketMakerU256::from_u128)
        .unwrap_or_default();
    let uptime_bps_current = if compliance_samples.is_empty() {
        0
    } else {
        let numerator = (uptime_sample_count as u128).saturating_mul(BPS_DENOMINATOR);
        let denominator = compliance_samples.len() as u128;
        numerator
            .saturating_div(denominator)
            .min(BPS_DENOMINATOR)
            .try_into()
            .unwrap_or(u16::MAX)
    };
    let two_sided_windows_last_h = hourly_two_sided_count.try_into().unwrap_or(u16::MAX);
    MarketMakerComplianceState {
        window_start_ms,
        window_end_ms: observed_at_ms,
        uptime_bps_current,
        avg_spread_bps_current,
        median_quote_size_current,
        two_sided_windows_last_h,
        last_window_breach: market_maker_first_breach_kind(
            uptime_bps_current,
            avg_spread_bps_current,
            median_quote_size_current,
            two_sided_windows_last_h,
            program,
        ),
        breach_counter: 0,
    }
}

pub(crate) fn evaluate_market_maker_enrollment_transition(
    enrollment: &mut MarketMakerEnrollment,
    program: &MarketMakerProgram,
    observed_at_ms: u64,
    economic_account_id: u64,
) -> Vec<RuntimeEventKind> {
    let compliant = market_maker_first_breach_kind(
        enrollment.current_compliance.uptime_bps_current,
        enrollment.current_compliance.avg_spread_bps_current,
        enrollment.current_compliance.median_quote_size_current,
        enrollment.current_compliance.two_sided_windows_last_h,
        program,
    )
    .is_none();
    let mut events = Vec::new();
    match enrollment.status {
        MarketMakerEnrollmentStatus::Active if !compliant => {
            enrollment.status = MarketMakerEnrollmentStatus::GracePeriod;
            enrollment.last_updated_at_ms = observed_at_ms;
            enrollment.current_compliance.breach_counter = enrollment
                .current_compliance
                .breach_counter
                .saturating_add(1);
            let breach = enrollment
                .current_compliance
                .last_window_breach
                .unwrap_or(MarketMakerBreachKind::Uptime);
            events.push(RuntimeEventKind::MarketMakerEnrollmentGraceEntered {
                market_maker_identity: enrollment.market_maker_identity,
                economic_account_id,
                market_id: enrollment.market_id,
                program_id: enrollment.program_id,
                breach: market_maker_breach_event(breach),
            });
        }
        MarketMakerEnrollmentStatus::GracePeriod => {
            if compliant {
                enrollment.status = MarketMakerEnrollmentStatus::Active;
                enrollment.last_updated_at_ms = observed_at_ms;
                events.push(RuntimeEventKind::MarketMakerEnrollmentRecovered {
                    market_maker_identity: enrollment.market_maker_identity,
                    economic_account_id,
                    market_id: enrollment.market_id,
                    program_id: enrollment.program_id,
                    recovered_from: market_maker_enrollment_status_event(
                        MarketMakerEnrollmentStatus::GracePeriod,
                    ),
                });
            } else if observed_at_ms
                >= enrollment
                    .last_updated_at_ms
                    .saturating_add(PROGRAM_GRACE_WINDOW_MS)
            {
                enrollment.status = MarketMakerEnrollmentStatus::NonCompliant;
                enrollment.last_updated_at_ms = observed_at_ms;
                events.push(RuntimeEventKind::MarketMakerEnrollmentNonCompliant {
                    market_maker_identity: enrollment.market_maker_identity,
                    economic_account_id,
                    market_id: enrollment.market_id,
                    program_id: enrollment.program_id,
                    breach_counter: enrollment
                        .current_compliance
                        .breach_counter
                        .try_into()
                        .unwrap_or(u16::MAX),
                });
            }
        }
        MarketMakerEnrollmentStatus::NonCompliant => {
            if compliant
                && observed_at_ms
                    >= enrollment
                        .last_updated_at_ms
                        .saturating_add(PROGRAM_REENTER_COOLDOWN_MS)
            {
                enrollment.status = MarketMakerEnrollmentStatus::Active;
                enrollment.last_updated_at_ms = observed_at_ms;
                events.push(RuntimeEventKind::MarketMakerEnrollmentRecovered {
                    market_maker_identity: enrollment.market_maker_identity,
                    economic_account_id,
                    market_id: enrollment.market_id,
                    program_id: enrollment.program_id,
                    recovered_from: market_maker_enrollment_status_event(
                        MarketMakerEnrollmentStatus::NonCompliant,
                    ),
                });
            }
        }
        MarketMakerEnrollmentStatus::Exited | MarketMakerEnrollmentStatus::Slashed => {}
        MarketMakerEnrollmentStatus::Active => {}
    }
    events
}

pub(crate) fn validate_market_maker_program_input(
    input: &DefineMarketMakerProgramInput,
    markets: &BTreeMap<u64, PerpetualMarket>,
) -> Result<(), NodeError> {
    let program_id = derive_market_maker_program_id(input);
    let reject = |reason| NodeError::InvalidMarketMakerProgram { program_id, reason };
    if !valid_market_maker_program_name(&input.name) {
        return Err(reject("invalid_program_name"));
    }
    if input.markets.is_empty()
        || input.markets.len() > MAX_MM_PROGRAM_MARKETS
        || input.markets.iter().any(|market_id| *market_id == 0)
    {
        return Err(reject("invalid_program_markets"));
    }
    for market_id in &input.markets {
        let Some(market) = markets.get(market_id) else {
            return Err(reject("unknown_market"));
        };
        if market.status == MarketStatus::Delisted {
            return Err(reject("market_not_tradable"));
        }
    }
    if input.min_uptime_bps == 0 || input.min_uptime_bps > BPS_DENOMINATOR as u16 {
        return Err(reject("min_uptime_out_of_range"));
    }
    if input.max_spread_bps == 0 || input.max_spread_bps > BPS_DENOMINATOR as u16 {
        return Err(reject("max_spread_out_of_range"));
    }
    if input.rebate_multiplier_units <= REBATE_MULTIPLIER_BASE_UNITS
        || input.rebate_multiplier_units > REBATE_MULTIPLIER_CAP_UNITS
    {
        return Err(reject("rebate_multiplier_out_of_range"));
    }
    let bond = input
        .enrollment_bond_micro_usdc
        .as_u128()
        .ok_or_else(|| reject("enrollment_bond_too_large"))?;
    if bond < PROGRAM_BOND_MIN_MICRO_USDC {
        return Err(reject("enrollment_bond_below_min"));
    }
    if input
        .min_quote_size_micro_usdc
        .as_u128()
        .ok_or_else(|| reject("min_quote_size_too_large"))?
        == 0
    {
        return Err(reject("min_quote_size_zero"));
    }
    if input.compliance_window_ms == 0 || input.compliance_window_ms > COMPLIANCE_WINDOW_MAX_MS {
        return Err(reject("compliance_window_out_of_range"));
    }
    if !(OBSERVATION_CADENCE_MIN_MS..=OBSERVATION_CADENCE_MAX_MS)
        .contains(&input.observation_cadence_ms)
    {
        return Err(reject("observation_cadence_out_of_range"));
    }
    Ok(())
}

fn valid_market_maker_program_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes().len() <= 32
        && name
            .as_bytes()
            .iter()
            .all(|byte| (0x20..=0x7e).contains(byte))
}

pub(crate) fn validate_market_maker_program_record(
    program: &MarketMakerProgram,
) -> Result<(), &'static str> {
    if !valid_market_maker_program_name(&program.name) {
        return Err("invalid_program_name");
    }
    if program.markets.is_empty()
        || program.markets.len() > MAX_MM_PROGRAM_MARKETS
        || program.markets.iter().any(|market_id| *market_id == 0)
    {
        return Err("invalid_program_markets");
    }
    if program.min_uptime_bps == 0 || program.min_uptime_bps > BPS_DENOMINATOR as u16 {
        return Err("min_uptime_out_of_range");
    }
    if program.max_spread_bps == 0 || program.max_spread_bps > BPS_DENOMINATOR as u16 {
        return Err("max_spread_out_of_range");
    }
    if program.rebate_multiplier_units <= REBATE_MULTIPLIER_BASE_UNITS
        || program.rebate_multiplier_units > REBATE_MULTIPLIER_CAP_UNITS
    {
        return Err("rebate_multiplier_out_of_range");
    }
    if program
        .enrollment_bond_micro_usdc
        .as_u128()
        .ok_or("enrollment_bond_too_large")?
        < PROGRAM_BOND_MIN_MICRO_USDC
    {
        return Err("enrollment_bond_below_min");
    }
    if program
        .min_quote_size_micro_usdc
        .as_u128()
        .ok_or("min_quote_size_too_large")?
        == 0
    {
        return Err("min_quote_size_zero");
    }
    if program.compliance_window_ms == 0 || program.compliance_window_ms > COMPLIANCE_WINDOW_MAX_MS
    {
        return Err("compliance_window_out_of_range");
    }
    if !(OBSERVATION_CADENCE_MIN_MS..=OBSERVATION_CADENCE_MAX_MS)
        .contains(&program.observation_cadence_ms)
    {
        return Err("observation_cadence_out_of_range");
    }
    let input = DefineMarketMakerProgramInput {
        name: program.name.clone(),
        markets: program.markets.clone(),
        min_uptime_bps: program.min_uptime_bps,
        max_spread_bps: program.max_spread_bps,
        min_quote_size_micro_usdc: program.min_quote_size_micro_usdc,
        min_two_sided_windows_per_hour: program.min_two_sided_windows_per_hour,
        rebate_multiplier_units: program.rebate_multiplier_units,
        enrollment_bond_micro_usdc: program.enrollment_bond_micro_usdc,
        compliance_window_ms: program.compliance_window_ms,
        observation_cadence_ms: program.observation_cadence_ms,
    };
    if derive_market_maker_program_id(&input) != program.program_id {
        return Err("program_id_mismatch");
    }
    if program.defined_at_ms == 0 || program.last_updated_at_ms == 0 {
        return Err("program_timestamp_zero");
    }
    Ok(())
}

pub(crate) fn validate_market_maker_enrollment_record(
    enrollment: &MarketMakerEnrollment,
    program: &MarketMakerProgram,
) -> Result<(), &'static str> {
    if market_maker_identity_signer_account(enrollment.market_maker_identity) == 0
        || market_maker_identity_economic_account(enrollment.market_maker_identity) == 0
    {
        return Err("market_maker_identity_invalid");
    }
    if enrollment.market_id == 0 {
        return Err("market_zero");
    }
    if enrollment.program_id != program.program_id {
        return Err("program_mismatch");
    }
    if enrollment.enrolled_at_ms == 0 || enrollment.last_updated_at_ms == 0 {
        return Err("timestamp_zero");
    }
    if enrollment.last_updated_at_ms < enrollment.enrolled_at_ms {
        return Err("timestamp_order");
    }
    if enrollment.current_compliance.window_end_ms < enrollment.current_compliance.window_start_ms {
        return Err("compliance_window_order");
    }
    if enrollment.current_compliance.uptime_bps_current > BPS_DENOMINATOR as u16
        || enrollment.current_compliance.avg_spread_bps_current > BPS_DENOMINATOR as u16
    {
        return Err("compliance_bps_out_of_range");
    }
    let mut last_observed_at_ms = None;
    for sample in &enrollment.observation_history {
        if sample.observed_at_ms == 0 {
            return Err("observation_timestamp_zero");
        }
        if sample.spread_bps > BPS_DENOMINATOR as u16 {
            return Err("observation_spread_out_of_range");
        }
        if last_observed_at_ms.is_some_and(|last| sample.observed_at_ms <= last) {
            return Err("observation_history_order");
        }
        last_observed_at_ms = Some(sample.observed_at_ms);
    }
    if let Some(last_observed_at_ms) = last_observed_at_ms {
        if enrollment.current_compliance.window_end_ms < last_observed_at_ms
            || enrollment.last_compliance_checked_ms < last_observed_at_ms
        {
            return Err("observation_history_mismatch");
        }
    }
    let bond = enrollment
        .bond_micro_usdc
        .as_u128()
        .ok_or("bond_overflow")?;
    let program_bond = program
        .enrollment_bond_micro_usdc
        .as_u128()
        .ok_or("program_bond_overflow")?;
    if bond < program_bond {
        return Err("bond_below_program_floor");
    }
    Ok(())
}

fn market_maker_breach_event(
    kind: MarketMakerBreachKind,
) -> kombat_chain_streams::MarketMakerBreachKind {
    match kind {
        MarketMakerBreachKind::Uptime => kombat_chain_streams::MarketMakerBreachKind::Uptime,
        MarketMakerBreachKind::Spread => kombat_chain_streams::MarketMakerBreachKind::Spread,
        MarketMakerBreachKind::Size => kombat_chain_streams::MarketMakerBreachKind::Size,
        MarketMakerBreachKind::TwoSidedWindows => {
            kombat_chain_streams::MarketMakerBreachKind::TwoSidedWindows
        }
    }
}

fn market_maker_enrollment_status_event(
    status: MarketMakerEnrollmentStatus,
) -> kombat_chain_streams::MarketMakerEnrollmentStatus {
    match status {
        MarketMakerEnrollmentStatus::Active => {
            kombat_chain_streams::MarketMakerEnrollmentStatus::Active
        }
        MarketMakerEnrollmentStatus::GracePeriod => {
            kombat_chain_streams::MarketMakerEnrollmentStatus::GracePeriod
        }
        MarketMakerEnrollmentStatus::NonCompliant => {
            kombat_chain_streams::MarketMakerEnrollmentStatus::NonCompliant
        }
        MarketMakerEnrollmentStatus::Exited => {
            kombat_chain_streams::MarketMakerEnrollmentStatus::Exited
        }
        MarketMakerEnrollmentStatus::Slashed => {
            kombat_chain_streams::MarketMakerEnrollmentStatus::Slashed
        }
    }
}
