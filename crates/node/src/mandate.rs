use crate::{
    account_id_is_delegation_reserved, BPS_DENOMINATOR, MAX_MANDATE_CONFORMANCE_WINDOW_MS,
    MAX_MANDATE_CONSTRAINTS, MAX_MANDATE_DESCRIPTION_URI_BYTES, MAX_MANDATE_OBJECTIVES,
    MAX_MANDATE_SESSION_KEYS, MAX_MANDATE_SET_SIZE, MIN_MANDATE_CONFORMANCE_WINDOW_MS,
};
use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::VerifyingKey;
use kombat_chain_consensus::Hash;
use kombat_chain_policy::{
    PolicyAdmission, PolicyAdmissionRules, PolicyError, PolicyOrigin, PolicyRecord, PolicyState,
    PolicyStore, PolicyTerms, Principal,
};
use revm::primitives::keccak256;
use std::collections::BTreeSet;

pub type MandateId = [u8; 32];
pub type MandateSessionKeyRef = [u8; 32];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MandateU256(pub [u8; 32]);

impl MandateU256 {
    pub fn from_u128(value: u128) -> Self {
        let mut bytes = [0_u8; 32];
        bytes[16..].copy_from_slice(&value.to_be_bytes());
        Self(bytes)
    }

    pub(crate) fn is_zero(self) -> bool {
        self.0 == [0_u8; 32]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MandateState {
    Draft,
    Active,
    Breached,
    Suspended,
    Expired,
    Revoked,
}

impl MandateState {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            MandateState::Breached | MandateState::Expired | MandateState::Revoked
        )
    }
}

pub(crate) fn mandate_state_event_name(state: MandateState) -> &'static str {
    match state {
        MandateState::Draft => "draft",
        MandateState::Active => "active",
        MandateState::Breached => "breached",
        MandateState::Suspended => "suspended",
        MandateState::Expired => "expired",
        MandateState::Revoked => "revoked",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MandateConstraint {
    MaxNotionalPerMarket {
        market: u64,
        limit_usdc: MandateU256,
    },
    MaxNotionalTotal {
        limit_usdc: MandateU256,
    },
    MaxConcentrationInMarketPct {
        market: u64,
        pct_bps: u32,
    },
    AllowedMarkets {
        set: BTreeSet<u64>,
    },
    MaxLeverage {
        mult: u32,
    },
    MaxDrawdownPctFromPeak {
        pct_bps: u32,
        window_ms: u64,
    },
    MaxDailyLossUsdc {
        limit_usdc: MandateU256,
    },
    NoPositionHeldLongerThan {
        duration_ms: u64,
    },
    MaxTradeSizeUsdc {
        limit_usdc: MandateU256,
    },
    MaxTradeRatePerMinute {
        count: u32,
    },
    NoSameOperatorCounterparty,
    AllowedCounterparties {
        accounts: BTreeSet<u64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MandateObjective {
    TargetSharpe { min: i64, over_ms: u64 },
    TargetReturnPct { min_bps: i32, over_ms: u64 },
    MaxDrawdownPct { max_bps: u32, over_ms: u64 },
    MinTradeCount { count: u32, over_ms: u64 },
    MaxTradeCount { count: u32, over_ms: u64 },
    MaxSlippageBps { limit: u32 },
    MaxFundingPaidPctOfEquity { pct_bps: u32, over_ms: u64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceState {
    pub score: u8,
    /// Last block timestamp where the emitted conformance state changed.
    pub last_evaluated_at_ms: u64,
    pub pending_objectives: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Mandate {
    pub id: MandateId,
    pub owner: Principal,
    pub agent: u64,
    pub version: u32,
    pub constraints: Vec<MandateConstraint>,
    pub objectives: Vec<MandateObjective>,
    pub objective_weights: Vec<u8>,
    pub conformance_window_ms: u64,
    pub policies: Vec<u64>,
    pub session_keys: Vec<MandateSessionKeyRef>,
    pub activated_at: Option<u64>,
    pub expires_at: u64,
    pub state: MandateState,
    pub description_uri: Option<String>,
    pub description_hash: Option<Hash>,
    pub conformance: ConformanceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MandateObjectiveEvaluation {
    pub(crate) mandate_id: MandateId,
    pub(crate) agent: u64,
    pub(crate) objective_index: u16,
    pub(crate) score: u8,
    pub(crate) conformance_score: u8,
    pub(crate) pending_objectives: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MandateObjectiveEventState {
    pub(crate) score: u8,
    pub(crate) conformance_score: u8,
    pub(crate) pending_objectives: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct CreateMandateInput {
    pub owner: Principal,
    pub agent: u64,
    pub version: u32,
    pub constraints: Vec<MandateConstraint>,
    pub objectives: Vec<MandateObjective>,
    pub objective_weights: Vec<u8>,
    pub conformance_window_ms: u64,
    pub session_keys: Vec<MandateSessionKeyRef>,
    pub expires_at: u64,
    pub description_uri: Option<String>,
    pub description_hash: Option<Hash>,
}

#[derive(Clone, Debug, BorshSerialize)]
struct MandateIdMaterial {
    owner: Principal,
    agent: u64,
    version: u32,
    constraints: Vec<MandateConstraint>,
    objectives: Vec<MandateObjective>,
    objective_weights: Vec<u8>,
    conformance_window_ms: u64,
    session_keys: Vec<MandateSessionKeyRef>,
    expires_at: u64,
    description_hash: Option<Hash>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum MandateActionKind {
    CreateMandate,
    SuspendMandate,
    ResumeMandate,
    RevokeMandate,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum MandateAction {
    CreateMandate(CreateMandateInput),
    SuspendMandate {
        mandate_id: MandateId,
    },
    ResumeMandate {
        mandate_id: MandateId,
    },
    RevokeMandate {
        mandate_id: MandateId,
        reason: String,
    },
}

impl MandateAction {
    pub fn kind(&self) -> MandateActionKind {
        match self {
            MandateAction::CreateMandate(_) => MandateActionKind::CreateMandate,
            MandateAction::SuspendMandate { .. } => MandateActionKind::SuspendMandate,
            MandateAction::ResumeMandate { .. } => MandateActionKind::ResumeMandate,
            MandateAction::RevokeMandate { .. } => MandateActionKind::RevokeMandate,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedMandateAction {
    pub chain_id: u64,
    pub owner: Principal,
    pub nonce: u64,
    pub action: MandateAction,
    pub signature: [u8; 64],
}

#[derive(Clone, Debug, BorshSerialize)]
struct MandateActionPayload {
    domain: String,
    chain_id: u64,
    action_kind: MandateActionKind,
    owner: Principal,
    nonce: u64,
    body_hash: Hash,
}

impl Mandate {
    pub fn derived_id(&self) -> MandateId {
        derive_mandate_id(
            self.owner,
            self.agent,
            self.version,
            self.constraints.clone(),
            self.objectives.clone(),
            self.objective_weights.clone(),
            self.conformance_window_ms,
            self.session_keys.clone(),
            self.expires_at,
            self.description_hash,
        )
    }
}

pub fn mandate_action_body_hash(action: &MandateAction) -> Hash {
    keccak256(borsh::to_vec(action).expect("mandate action should serialize")).into()
}

pub fn mandate_action_signing_bytes(
    chain_id: u64,
    owner: Principal,
    nonce: u64,
    action: &MandateAction,
) -> Vec<u8> {
    let payload = MandateActionPayload {
        domain: "0x26:mandate:v1".into(),
        chain_id,
        action_kind: action.kind(),
        owner,
        nonce,
        body_hash: mandate_action_body_hash(action),
    };
    borsh::to_vec(&payload).expect("mandate action payload should serialize")
}

#[allow(clippy::too_many_arguments)]
pub fn derive_mandate_id(
    owner: Principal,
    agent: u64,
    version: u32,
    constraints: Vec<MandateConstraint>,
    objectives: Vec<MandateObjective>,
    objective_weights: Vec<u8>,
    conformance_window_ms: u64,
    session_keys: Vec<MandateSessionKeyRef>,
    expires_at: u64,
    description_hash: Option<Hash>,
) -> MandateId {
    let material = MandateIdMaterial {
        owner,
        agent,
        version,
        constraints,
        objectives,
        objective_weights,
        conformance_window_ms,
        session_keys,
        expires_at,
        description_hash,
    };
    let bytes = borsh::to_vec(&material).expect("mandate id material should serialize");
    keccak256(bytes).into()
}

pub(crate) fn compile_mandate_policies(
    mandate: &Mandate,
    policy_store: &PolicyStore,
    created_at_ms: u64,
) -> Result<Vec<PolicyRecord>, &'static str> {
    if mandate.session_keys.is_empty() {
        return Ok(Vec::new());
    }

    let mut allowed_markets: Option<BTreeSet<u64>> = None;
    let mut max_trade_notional = 0_u128;
    let mut max_notional_outstanding = 0_u128;
    let mut max_intents_per_minute = 0_u32;
    let mut max_drawdown_pct = u16::MAX;

    for constraint in &mandate.constraints {
        match constraint {
            MandateConstraint::AllowedMarkets { set } => {
                allowed_markets = Some(match allowed_markets.take() {
                    Some(existing) => existing.intersection(set).copied().collect(),
                    None => set.clone(),
                });
            }
            MandateConstraint::MaxNotionalPerMarket { limit_usdc, .. }
            | MandateConstraint::MaxNotionalTotal { limit_usdc } => {
                let limit = mandate_u256_to_u128(*limit_usdc)?;
                max_notional_outstanding = min_nonzero_limit(max_notional_outstanding, limit);
            }
            MandateConstraint::MaxTradeSizeUsdc { limit_usdc } => {
                let limit = mandate_u256_to_u128(*limit_usdc)?;
                max_trade_notional = min_nonzero_limit(max_trade_notional, limit);
            }
            MandateConstraint::MaxTradeRatePerMinute { count } => {
                max_intents_per_minute = min_nonzero_u32(max_intents_per_minute, *count);
            }
            MandateConstraint::MaxDrawdownPctFromPeak { pct_bps, .. } => {
                max_drawdown_pct = max_drawdown_pct
                    .min(u16::try_from(*pct_bps).map_err(|_| "unsupported_mandate_constraint")?);
            }
            MandateConstraint::MaxConcentrationInMarketPct { .. }
            | MandateConstraint::MaxLeverage { .. }
            | MandateConstraint::MaxDailyLossUsdc { .. }
            | MandateConstraint::NoPositionHeldLongerThan { .. }
            | MandateConstraint::AllowedCounterparties { .. }
            | MandateConstraint::NoSameOperatorCounterparty => {}
        }
    }

    let allowed_markets = allowed_markets.unwrap_or_default();
    if allowed_markets.is_empty()
        && mandate
            .constraints
            .iter()
            .any(|constraint| matches!(constraint, MandateConstraint::AllowedMarkets { .. }))
    {
        return Err("allowed_markets_intersection_empty");
    }

    let terms = PolicyTerms {
        gas_budget_per_day: u128::MAX,
        gas_refill_per_day: u128::MAX,
        max_intents_per_second: u32::MAX,
        max_intents_per_day: u32::MAX,
        max_volume_per_day: u128::MAX,
        max_drawdown_pct,
    };
    let admission_rules = PolicyAdmissionRules {
        allowed_markets: allowed_markets.into_iter().collect(),
        max_trade_notional,
        max_notional_outstanding,
        max_intents_per_minute,
    };

    let mut policies = Vec::with_capacity(mandate.session_keys.len());
    for (index, session_key) in mandate.session_keys.iter().enumerate() {
        VerifyingKey::from_bytes(session_key).map_err(|_| "invalid_session_key_material")?;
        let generated_policy_index = u16::try_from(index).map_err(|_| "too_many_session_keys")?;
        let policy_id = derive_mandate_policy_id(mandate.id, generated_policy_index, policy_store)?;
        let policy = PolicyRecord::new(
            policy_id,
            mandate.agent,
            *session_key,
            created_at_ms,
            mandate.expires_at,
            PolicyState::Active,
            terms.clone(),
        )
        .with_admission_rules(admission_rules.clone())
        .with_origin(PolicyOrigin::MandateGenerated {
            mandate_id: mandate.id,
            // Current PolicyStore rows hold combined Admission constraints for one
            // session key. A later splitter can use exact constraint indexes.
            constraint_index: u16::MAX,
            generated_policy_index,
        });
        policies.push(policy);
    }

    Ok(policies)
}

fn derive_mandate_policy_id(
    mandate_id: MandateId,
    generated_policy_index: u16,
    policy_store: &PolicyStore,
) -> Result<u64, &'static str> {
    for collision_nonce in 0_u16..=u16::MAX {
        let mut bytes = Vec::with_capacity(51);
        bytes.extend_from_slice(b"0x26:mandate-policy:v1");
        bytes.extend_from_slice(&mandate_id);
        bytes.extend_from_slice(&generated_policy_index.to_be_bytes());
        bytes.extend_from_slice(&collision_nonce.to_be_bytes());
        let hash = keccak256(bytes);
        let mut id_bytes = [0_u8; 8];
        id_bytes.copy_from_slice(&hash[..8]);
        let policy_id = u64::from_be_bytes(id_bytes);
        if policy_id != 0 && policy_store.get(policy_id).is_none() {
            return Ok(policy_id);
        }
    }
    Err("mandate_policy_id_collision")
}

pub(crate) fn mandate_u256_to_u128(value: MandateU256) -> Result<u128, &'static str> {
    if value.0[..16].iter().any(|byte| *byte != 0) {
        return Err("mandate_limit_too_large");
    }
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&value.0[16..]);
    Ok(u128::from_be_bytes(bytes))
}

fn min_nonzero_limit(current: u128, candidate: u128) -> u128 {
    if current == 0 {
        candidate
    } else {
        current.min(candidate)
    }
}

fn min_nonzero_u32(current: u32, candidate: u32) -> u32 {
    if current == 0 {
        candidate
    } else {
        current.min(candidate)
    }
}

pub(crate) fn validate_mandate_record(mandate: &Mandate) -> Result<(), &'static str> {
    if mandate.id == [0_u8; 32] {
        return Err("mandate_id_zero");
    }
    if mandate.agent == 0 {
        return Err("agent_zero");
    }
    if account_id_is_delegation_reserved(mandate.agent) {
        return Err("reserved_delegation_account_policy");
    }
    if mandate.constraints.len() > MAX_MANDATE_CONSTRAINTS {
        return Err("too_many_constraints");
    }
    if mandate.objectives.len() > MAX_MANDATE_OBJECTIVES {
        return Err("too_many_objectives");
    }
    if mandate.session_keys.len() > MAX_MANDATE_SESSION_KEYS {
        return Err("too_many_session_keys");
    }
    if let Some(description_uri) = &mandate.description_uri {
        if description_uri.as_bytes().len() > MAX_MANDATE_DESCRIPTION_URI_BYTES {
            return Err("description_uri_too_long");
        }
    }
    if !mandate.objective_weights.is_empty() {
        if mandate.objective_weights.len() != mandate.objectives.len() {
            return Err("objective_weight_length_mismatch");
        }
        let weight_sum: u16 = mandate
            .objective_weights
            .iter()
            .map(|weight| u16::from(*weight))
            .sum();
        if weight_sum != 100 {
            return Err("objective_weight_sum_mismatch");
        }
    }
    if !(MIN_MANDATE_CONFORMANCE_WINDOW_MS..=MAX_MANDATE_CONFORMANCE_WINDOW_MS)
        .contains(&mandate.conformance_window_ms)
    {
        return Err("conformance_window_out_of_range");
    }
    if mandate.expires_at == 0 {
        return Err("expires_at_zero");
    }
    if mandate.conformance.score > 100 {
        return Err("conformance_score_out_of_range");
    }
    if mandate.state == MandateState::Draft && mandate.activated_at.is_some() {
        return Err("draft_activated_at_set");
    }
    if mandate.state == MandateState::Active && mandate.activated_at.is_none() {
        return Err("active_activated_at_missing");
    }
    let mut session_keys = BTreeSet::new();
    for session_key in &mandate.session_keys {
        if *session_key == [0_u8; 32] || !session_keys.insert(*session_key) {
            return Err("invalid_session_key");
        }
    }
    for constraint in &mandate.constraints {
        validate_mandate_constraint(constraint)?;
    }
    for objective in &mandate.objectives {
        validate_mandate_objective(objective)?;
    }
    Ok(())
}

fn validate_mandate_constraint(constraint: &MandateConstraint) -> Result<(), &'static str> {
    match constraint {
        MandateConstraint::MaxNotionalPerMarket { market, limit_usdc } => {
            if *market == 0 || limit_usdc.is_zero() {
                return Err("invalid_max_notional_per_market");
            }
        }
        MandateConstraint::MaxNotionalTotal { limit_usdc }
        | MandateConstraint::MaxDailyLossUsdc { limit_usdc }
        | MandateConstraint::MaxTradeSizeUsdc { limit_usdc } => {
            if limit_usdc.is_zero() {
                return Err("invalid_usdc_limit");
            }
        }
        MandateConstraint::MaxConcentrationInMarketPct { market, pct_bps } => {
            if *market == 0 || *pct_bps == 0 || *pct_bps > BPS_DENOMINATOR as u32 {
                return Err("invalid_concentration_bps");
            }
        }
        MandateConstraint::AllowedMarkets { set } => {
            if set.is_empty()
                || set.len() > MAX_MANDATE_SET_SIZE
                || set.iter().any(|market| *market == 0)
            {
                return Err("invalid_allowed_markets");
            }
        }
        MandateConstraint::MaxLeverage { mult } => {
            if *mult == 0 {
                return Err("invalid_max_leverage");
            }
        }
        MandateConstraint::MaxDrawdownPctFromPeak { pct_bps, window_ms } => {
            if *pct_bps == 0 || *pct_bps > BPS_DENOMINATOR as u32 || *window_ms == 0 {
                return Err("invalid_drawdown_constraint");
            }
        }
        MandateConstraint::NoPositionHeldLongerThan { duration_ms } => {
            if *duration_ms == 0 {
                return Err("invalid_holding_period");
            }
        }
        MandateConstraint::MaxTradeRatePerMinute { count } => {
            if *count == 0 {
                return Err("invalid_trade_rate");
            }
        }
        MandateConstraint::NoSameOperatorCounterparty => {}
        MandateConstraint::AllowedCounterparties { accounts } => {
            if accounts.is_empty()
                || accounts.len() > MAX_MANDATE_SET_SIZE
                || accounts.iter().any(|account| *account == 0)
            {
                return Err("invalid_allowed_counterparties");
            }
        }
    }
    Ok(())
}

fn validate_mandate_objective(objective: &MandateObjective) -> Result<(), &'static str> {
    match objective {
        MandateObjective::TargetSharpe { over_ms, .. }
        | MandateObjective::TargetReturnPct { over_ms, .. }
        | MandateObjective::MaxDrawdownPct { over_ms, .. }
        | MandateObjective::MinTradeCount { over_ms, .. }
        | MandateObjective::MaxTradeCount { over_ms, .. }
        | MandateObjective::MaxFundingPaidPctOfEquity { over_ms, .. } => {
            if *over_ms == 0 {
                return Err("objective_window_zero");
            }
        }
        MandateObjective::MaxSlippageBps { limit } => {
            if *limit > BPS_DENOMINATOR as u32 {
                return Err("objective_slippage_out_of_range");
            }
        }
    }
    Ok(())
}

pub(crate) fn mandate_objective_window_ms(objective: &MandateObjective) -> Option<u64> {
    match objective {
        MandateObjective::TargetSharpe { over_ms, .. }
        | MandateObjective::TargetReturnPct { over_ms, .. }
        | MandateObjective::MaxDrawdownPct { over_ms, .. }
        | MandateObjective::MinTradeCount { over_ms, .. }
        | MandateObjective::MaxTradeCount { over_ms, .. }
        | MandateObjective::MaxFundingPaidPctOfEquity { over_ms, .. } => Some(*over_ms),
        MandateObjective::MaxSlippageBps { .. } => None,
    }
}

pub(crate) fn mandate_trade_count_window_ms(mandate: &Mandate) -> Option<u64> {
    mandate
        .objectives
        .iter()
        .filter_map(|objective| match objective {
            MandateObjective::MinTradeCount { over_ms, .. }
            | MandateObjective::MaxTradeCount { over_ms, .. } => Some(*over_ms),
            _ => None,
        })
        .max()
}

pub(crate) fn aggregate_objective_scores(scored: &[(u16, u8)], weights: &[u8]) -> u8 {
    if scored.is_empty() {
        return 100;
    }
    if weights.is_empty() {
        let sum = scored.iter().fold(0_u64, |acc, (_, score)| {
            acc.saturating_add(u64::from(*score))
        });
        return (sum / scored.len() as u64).min(100) as u8;
    }
    scored
        .iter()
        .zip(weights.iter())
        .fold(0_u64, |acc, ((_, score), weight)| {
            acc.saturating_add(u64::from(*score) * u64::from(*weight))
        })
        .checked_div(100)
        .unwrap_or(0)
        .min(100) as u8
}

pub(crate) fn mandate_constraint_index_for_policy_error(
    mandate: &Mandate,
    error: &PolicyError,
    admission: &PolicyAdmission,
) -> u16 {
    for (index, constraint) in mandate.constraints.iter().enumerate() {
        let matches_error = match (constraint, error) {
            (MandateConstraint::AllowedMarkets { set }, PolicyError::MarketNotAllowed) => admission
                .market_id
                .is_none_or(|market_id| !set.contains(&market_id)),
            (
                MandateConstraint::MaxTradeSizeUsdc { limit_usdc },
                PolicyError::TradeNotionalExceeded,
            ) => mandate_u256_to_u128(*limit_usdc)
                .is_ok_and(|limit| admission.intent_notional > limit),
            (
                MandateConstraint::MaxNotionalPerMarket { market, limit_usdc },
                PolicyError::NotionalOutstandingExceeded,
            ) => {
                admission.market_id == Some(*market)
                    && mandate_u256_to_u128(*limit_usdc)
                        .is_ok_and(|limit| admission.notional_outstanding > limit)
            }
            (
                MandateConstraint::MaxNotionalTotal { limit_usdc },
                PolicyError::NotionalOutstandingExceeded,
            ) => mandate_u256_to_u128(*limit_usdc)
                .is_ok_and(|limit| admission.notional_outstanding > limit),
            (
                MandateConstraint::MaxTradeRatePerMinute { .. },
                PolicyError::MinuteIntentLimitExceeded,
            ) => true,
            (
                MandateConstraint::MaxDrawdownPctFromPeak { .. },
                PolicyError::DrawdownExceededAndRevoked,
            ) => true,
            _ => false,
        };
        if matches_error {
            return u16::try_from(index).unwrap_or(u16::MAX);
        }
    }
    u16::MAX
}
