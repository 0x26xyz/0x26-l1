use kombat_chain_consensus::{IntentId, MarketMakerIdentity};

pub const CRATE_NAME: &str = "streams";

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IntentAdmissionClass {
    Direct,
    Conditional,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IntentStatusStage {
    Unknown,
    AcceptedDirect,
    AcceptedConditional,
    Promoted,
    Executed,
    Expired,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IntentRejectionReason {
    UnknownPolicy,
    PolicyNotActive,
    PolicyExpired,
    AccountFrozen,
    MarketTradingPaused,
    StaleMarketData,
    InvalidIntentNonce,
    InvalidSessionSignature,
    SessionPolicyMismatch,
    SessionChainMismatch,
    SessionKeyRevoked,
    UnauthorizedGrantee,
    GasBudgetExceeded,
    IntentRateLimitExceeded,
    MinuteIntentLimitExceeded,
    DailyIntentLimitExceeded,
    DailyVolumeExceeded,
    MarketNotAllowed,
    TradeNotionalExceeded,
    NotionalOutstandingExceeded,
    MandateConstraintViolated,
    DrawdownExceededAndRevoked,
    SubPolicyScopeMismatch,
    SubPolicyTermsExceedParent,
    DelegationDepthExceeded,
    Overflow,
    InternalError,
    /// Cascaded from a terminal rejection at a lower nonce for the same
    /// policy. The predecessor intent consumed the frontier nonce slot in
    /// the same block-assembly pass and failed non-transiently, so this
    /// intent cannot execute in strict-order and is evicted from the
    /// mempool. The client must resubmit starting at the rejected nonce.
    PredecessorIntentRejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IntentPendingReason {
    TimeAfter,
    NonceFrontier,
    MarketTradingPaused,
    StaleMarketData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OrderRejectionReason {
    ZeroMarketId,
    ZeroAccountId,
    ZeroQuantity,
    ZeroClientOrderId,
    ZeroLimitPrice,
    MarketMismatch,
    MarketOrderRequiresImmediateTif,
    PostOnlyRequiresLimitOrder,
    PostOnlyIncompatibleWithImmediateTif,
    GttRequiresFutureExpiry,
    PostOnlyWouldCross,
    FillOrKillInsufficientLiquidity,
    SelfTradePrevented,
    DuplicateClientOrderId,
    UnknownRestingOrder,
    ReplaceAccountMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentStatus {
    pub intent_id: IntentId,
    pub stage: IntentStatusStage,
    pub rejection_reason: Option<IntentRejectionReason>,
    pub order_rejection_reason: Option<OrderRejectionReason>,
    pub pending_reason: Option<IntentPendingReason>,
    pub next_eligible_at_ms: Option<u64>,
    pub current_policy_nonce: Option<u64>,
    pub policy_id: Option<u64>,
    pub nonce: Option<u64>,
    pub currently_queued: bool,
    pub latest_event_seq: Option<u64>,
    pub block_height: Option<u64>,
    pub timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct RuntimeEventRecord {
    pub seq: u64,
    pub block_height: Option<u64>,
    pub timestamp_ms: u64,
    pub kind: RuntimeEventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Tip5Hash {
    pub limb0: u64,
    pub limb1: u64,
    pub limb2: u64,
    pub limb3: u64,
    pub limb4: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum RuntimeEventKind {
    AccountCreated {
        account_id: u64,
        owner: [u8; 32],
    },
    AccountUpdated {
        account_id: u64,
        action: AccountUpdateAction,
        reason: Option<RiskTransitionSource>,
        owner: [u8; 32],
        pending_owner: Option<[u8; 32]>,
        paused: bool,
        nonce: u64,
        code_hash: [u8; 32],
        controllers: Vec<u64>,
    },
    AccountOperatorUpdated {
        account_id: u64,
        previous_operator_id: Option<u64>,
        operator_id: u64,
    },
    IntentAccepted {
        intent_id: IntentId,
        policy_id: u64,
        nonce: u64,
        class: IntentAdmissionClass,
    },
    IntentPromoted {
        intent_id: IntentId,
        policy_id: u64,
        nonce: u64,
    },
    IntentExecuted {
        intent_id: IntentId,
        policy_id: u64,
        nonce: u64,
    },
    IntentExpired {
        intent_id: IntentId,
        policy_id: u64,
        nonce: u64,
    },
    IntentRejected {
        intent_id: IntentId,
        policy_id: u64,
        nonce: u64,
        reason: IntentRejectionReason,
        order_reason: Option<OrderRejectionReason>,
    },
    OrderAccepted {
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        status: OrderLifecycleStatus,
        filled_lots: u64,
        remaining_lots: u64,
    },
    OrderFilled {
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        counterparty_account_id: u64,
        counterparty_client_order_id: u64,
        mandate_id: Option<[u8; 32]>,
        role: OrderFillRole,
        side: OrderFillSide,
        price_ticks: u64,
        quantity_lots: u64,
    },
    FeeCharged {
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        role: OrderFillRole,
        fee_amount: u128,
        fee_vault_balance: u128,
    },
    FeeRevenueDistributed {
        market_id: u64,
        source_fee_amount: u128,
        validator_rewards_delta: u128,
        validator_reward_balance: u128,
        market_maker_rebate_delta: u128,
        market_maker_rebate_balance: u128,
        insurance_fund_delta: u128,
        insurance_fund_balance: u128,
        treasury_delta: u128,
        treasury_balance: u128,
    },
    MarketListed {
        market_id: u64,
        base_symbol: String,
        quote_asset: u32,
        oracle_feed_id: u64,
        status: MarketLifecycleStatus,
    },
    MarketStatusUpdated {
        market_id: u64,
        status: MarketLifecycleStatus,
    },
    MarketMakerProgramDefined {
        program_id: [u8; 32],
        name: String,
        markets: std::collections::BTreeSet<u64>,
        rebate_multiplier_units: u16,
        compliance_window_ms: u64,
        reactivated: bool,
    },
    MarketMakerProgramRetired {
        program_id: [u8; 32],
    },
    MarketMakerProgramActionRejected {
        action_kind: String,
        reason: String,
    },
    MarketMakerEnrolled {
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: [u8; 32],
        bond_micro_usdc: u128,
    },
    MarketMakerEnrollmentExited {
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: [u8; 32],
        bond_refunded_micro_usdc: u128,
    },
    MmProgramBonusPaid {
        market_maker_identity: MarketMakerIdentity,
        economic_account_id: u64,
        market_id: u64,
        program_id: [u8; 32],
        requested_bonus_micro_usdc: u128,
        paid_bonus_micro_usdc: u128,
    },
    MmProgramBonusForfeited {
        market_maker_identity: MarketMakerIdentity,
        economic_account_id: u64,
        market_id: u64,
        program_id: [u8; 32],
        forfeited_bonus_micro_usdc: u128,
    },
    MarketMakerEnrollmentGraceEntered {
        market_maker_identity: MarketMakerIdentity,
        economic_account_id: u64,
        market_id: u64,
        program_id: [u8; 32],
        breach: MarketMakerBreachKind,
    },
    MarketMakerEnrollmentRecovered {
        market_maker_identity: MarketMakerIdentity,
        economic_account_id: u64,
        market_id: u64,
        program_id: [u8; 32],
        recovered_from: MarketMakerEnrollmentStatus,
    },
    MarketMakerEnrollmentNonCompliant {
        market_maker_identity: MarketMakerIdentity,
        economic_account_id: u64,
        market_id: u64,
        program_id: [u8; 32],
        breach_counter: u16,
    },
    DelegationOpened {
        delegation_id: [u8; 32],
        agent: u64,
        mandate_id: [u8; 32],
        performance_share_units: u16,
        crystallization_cadence_ms: u64,
        notice_period_ms: u64,
        agent_bond: u128,
    },
    DelegationDeposited {
        delegation_id: [u8; 32],
        principal: u64,
        amount: u128,
        shares_minted: u128,
        total_shares: u128,
    },
    DelegationRedemptionRequested {
        delegation_id: [u8; 32],
        principal: u64,
        request_id: [u8; 32],
        shares_requested: u128,
        ready_at_ms: u64,
    },
    DelegationRedemptionFinalized {
        delegation_id: [u8; 32],
        principal: u64,
        request_id: [u8; 32],
        shares_burned: u128,
        usdc_out: u128,
        compensation_out: u128,
        total_shares: u128,
    },
    DelegationRedemptionCancelled {
        delegation_id: [u8; 32],
        principal: u64,
        request_id: [u8; 32],
        shares_unlocked: u128,
    },
    DelegationCrystallized {
        delegation_id: [u8; 32],
        agent: u64,
        agent_fee: u128,
        nav_pre_fee: u128,
        nav_post_fee: u128,
        hwm_new: u128,
    },
    DelegationStatusChanged {
        delegation_id: [u8; 32],
        agent: u64,
        old_status: DelegationLifecycleStatus,
        new_status: DelegationLifecycleStatus,
        reason: DelegationStatusReason,
        bond_returned: u128,
        bond_slashed: u128,
        principal_compensation: u128,
        insurance_fund_delta: u128,
    },
    DelegationActionRejected {
        action_kind: String,
        reason: String,
    },
    MandateCreated {
        mandate_id: [u8; 32],
        owner: [u8; 32],
        agent: u64,
        version: u32,
        expires_at: u64,
    },
    MandateActivated {
        mandate_id: [u8; 32],
        agent: u64,
        activated_at_ms: u64,
    },
    MandateObjectiveEvaluated {
        mandate_id: [u8; 32],
        agent: u64,
        objective_index: u16,
        score: u8,
        conformance_score: u8,
        pending_objectives: u16,
        evaluated_at_ms: u64,
    },
    MandateConformanceEvaluated {
        mandate_id: [u8; 32],
        agent: u64,
        evaluated_at_ms: u64,
        score: u8,
        pending_objectives: u16,
        state: String,
        prev_score: Option<u8>,
        delta: Option<i16>,
    },
    MandateConstraintTriggered {
        mandate_id: [u8; 32],
        agent: u64,
        constraint_index: u16,
        action: MandateEnforcementAction,
        triggered_at_ms: u64,
    },
    MandateBreached {
        mandate_id: [u8; 32],
        agent: u64,
        constraint_index: u16,
        detected_at_ms: u64,
    },
    MarketTradingStateUpdated {
        market_id: u64,
        paused: bool,
    },
    OracleConfigUpdated {
        market_id: u64,
        max_staleness_ms: u64,
    },
    PositionUpdated {
        market_id: u64,
        account_id: u64,
        net_lots: i64,
        open_notional: u128,
        realized_pnl: i128,
        average_entry_price_ticks: Option<u64>,
        buy_lots: u64,
        sell_lots: u64,
        buy_notional: u128,
        sell_notional: u128,
    },
    MarkPriceUpdated {
        market_id: u64,
        mark_price_ticks: u64,
    },
    IndexPriceUpdated {
        market_id: u64,
        index_price_ticks: u64,
    },
    MarketOpenInterestUpdated {
        market_id: u64,
        open_interest_lots: u128,
    },
    FundingIndexUpdated {
        market_id: u64,
        funding_index: i128,
    },
    FundingApplied {
        market_id: u64,
        account_id: u64,
        funding_delta: i128,
        funding_index: i128,
    },
    CollateralDeposited {
        market_id: u64,
        account_id: u64,
        amount_usdc: u64,
        collateral: i128,
    },
    CollateralWithdrawn {
        market_id: u64,
        account_id: u64,
        amount_usdc: u64,
        collateral: i128,
    },
    RiskStateUpdated {
        market_id: u64,
        account_id: u64,
        net_lots: i64,
        collateral: i128,
        equity: i128,
        maintenance_requirement: u128,
        margin_surplus: i128,
        under_maintenance: bool,
    },
    LiquidationCandidateUpdated {
        market_id: u64,
        account_id: u64,
        net_lots: i64,
        equity: i128,
        maintenance_requirement: u128,
        margin_surplus: i128,
        under_maintenance: bool,
    },
    // Per-market margin/liquidation insurance update. RFC 049 real-capital
    // bond-slash insurance is currently an accounting accumulator, not this
    // spendable market fund.
    InsuranceFundUpdated {
        market_id: u64,
        delta: i128,
        balance: u128,
        source: InsuranceFundUpdateSource,
    },
    BadDebtUpdated {
        market_id: u64,
        account_id: u64,
        delta: u128,
        balance: u128,
        source: RiskTransitionSource,
    },
    BadDebtRecovered {
        market_id: u64,
        account_id: u64,
        recovered: u128,
        balance: u128,
        source: RiskTransitionSource,
    },
    SolvencyBreached {
        market_id: u64,
        account_id: u64,
        residual_bad_debt: u128,
    },
    LiquidationExecuted {
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
        requested_lots: u64,
        filled_lots: u64,
        remaining_lots: u64,
        executed_notional: u128,
        penalty: u128,
        insurance_cover: u128,
        liquidator_reward: u128,
        insurance_fund_balance: u128,
    },
    LiquidationOutcome {
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
        outcome: LiquidationExecutionOutcome,
        post_net_lots: i64,
        post_margin_surplus: i128,
        post_under_maintenance: bool,
        post_account_frozen: bool,
        post_market_bad_debt_balance: u128,
    },
    LiquidationEscalationUpdated {
        market_id: u64,
        account_id: u64,
        action: LiquidationEscalationAction,
        stall_count: u8,
        freeze_threshold: u8,
        freeze_imminent: bool,
        post_margin_surplus: i128,
        post_under_maintenance: bool,
        post_account_frozen: bool,
    },
    OrderReplaced {
        market_id: u64,
        account_id: u64,
        target_client_order_id: u64,
        replacement_client_order_id: u64,
        status: OrderLifecycleStatus,
        filled_lots: u64,
        remaining_lots: u64,
    },
    OrderCancelled {
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        remaining_lots: u64,
    },
    PolicyCreated {
        account_id: u64,
        parent_policy_id: Option<u64>,
        policy_id: u64,
    },
    PolicyRevoked {
        account_id: u64,
        policy_id: u64,
        cascaded: bool,
    },
    MandateSuspended {
        mandate_id: [u8; 32],
        agent: u64,
        by: [u8; 32],
        suspended_at_ms: u64,
    },
    MandateResumed {
        mandate_id: [u8; 32],
        agent: u64,
        by: [u8; 32],
        resumed_at_ms: u64,
    },
    MandateRevoked {
        mandate_id: [u8; 32],
        agent: u64,
        by: [u8; 32],
        reason: String,
        revoked_at_ms: u64,
    },
    MandateExpired {
        mandate_id: [u8; 32],
        agent: u64,
        expired_at_ms: u64,
    },
    IdentityRegistered {
        account: u64,
        tier: IdentityTierStatus,
        display_name: String,
        kind: IdentityAgentKind,
        operator: Option<u64>,
    },
    IdentityUpdated {
        account: u64,
        field: String,
        before: Option<String>,
        after: Option<String>,
    },
    IdentityDeleted {
        account: u64,
    },
    AttestationRegistered {
        account: u64,
        record_id: [u8; 32],
        kind: IdentityAttestationKind,
        attestor: u64,
        claim: IdentityAttestationClaim,
        expires_at_timestamp_ms: u64,
        tier_before: IdentityTierStatus,
        tier_after: IdentityTierStatus,
    },
    AttestationVerified {
        account: u64,
        record_id: [u8; 32],
        vendor: IdentityAttestationVendor,
        code_hash: [u8; 32],
        enclave_id: [u8; 32],
    },
    AttestationRefreshed {
        account: u64,
        record_id: [u8; 32],
        new_expires_at_timestamp_ms: u64,
    },
    AttestationRevoked {
        account: u64,
        record_id: [u8; 32],
        reason: IdentityAttestationRevocationReason,
        tier_before: IdentityTierStatus,
        tier_after: IdentityTierStatus,
    },
    AttestationExpired {
        account: u64,
        record_id: [u8; 32],
        tier_before: IdentityTierStatus,
        tier_after: IdentityTierStatus,
    },
    TierChanged {
        account: u64,
        from: IdentityTierStatus,
        to: IdentityTierStatus,
        reason: IdentityTierTransitionReason,
    },
    VendorRootIntroduced {
        vendor: IdentityAttestationVendor,
        root_id: [u8; 32],
        valid_from_block: u64,
        governance_action: [u8; 32],
    },
    VendorRootRevoked {
        vendor: IdentityAttestationVendor,
        root_id: [u8; 32],
        valid_until_block: u64,
        governance_action: [u8; 32],
    },
    VendorCapProfileUpdated {
        vendor: IdentityAttestationVendor,
        old_cap_multiplier_microunits: u128,
        new_cap_multiplier_microunits: u128,
        old_composition_eligibility: bool,
        new_composition_eligibility: bool,
        governance_action: [u8; 32],
    },
    TeeGovernanceActionRejected {
        governance_action: [u8; 32],
        action_kind: String,
        reason: String,
    },
    RealCapitalListingUpserted {
        vault_id: [u8; 16],
        delegation_id: [u8; 32],
        serving_agent: u64,
        old_serving_agent: Option<u64>,
        mode: RealCapitalListingMode,
        deposit_enabled: bool,
        listed_for_discovery: bool,
        per_principal_cap_micro_usdc: u128,
        total_vault_tvl_cap_micro_usdc: u128,
        governance_action: [u8; 32],
    },
    RealCapitalListingDepositEnabledUpdated {
        vault_id: [u8; 16],
        delegation_id: [u8; 32],
        serving_agent: u64,
        deposit_enabled: bool,
        governance_action: [u8; 32],
    },
    RealCapitalListingDiscoveryUpdated {
        vault_id: [u8; 16],
        delegation_id: [u8; 32],
        serving_agent: u64,
        listed_for_discovery: bool,
        governance_action: [u8; 32],
    },
    RealCapitalProtocolCapUpdated {
        old_total_protocol_tvl_micro_usdc: Option<u128>,
        new_total_protocol_tvl_micro_usdc: Option<u128>,
        governance_action: [u8; 32],
    },
    RealCapitalGlobalKillSwitchUpdated {
        old_disabled: bool,
        new_disabled: bool,
        governance_action: [u8; 32],
    },
    RealCapitalGovernanceActionRejected {
        governance_action: [u8; 32],
        action_kind: String,
        reason: String,
    },
    // RFC 049 Rust-side insurance wallet spend under TestnetLogicalRemint.
    // This is separate from per-market InsuranceFundUpdated.
    RealCapitalInsuranceWalletSpent {
        amount: u128,
        balance: u128,
        source: RealCapitalInsuranceWalletSpendSource,
    },
    BridgeDepositLocked {
        deposit_nonce: u64,
        tx_order_in_block: u64,
        recipient: [u8; 20],
        amount_nicks: u64,
        nock_tx_id: Tip5Hash,
        note_name_first: Tip5Hash,
        note_name_last: Tip5Hash,
        nock_as_of: Tip5Hash,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum MarketLifecycleStatus {
    Active,
    Paused,
    ReduceOnly,
    Delisted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum DelegationLifecycleStatus {
    Open,
    ClosedToDeposits,
    Winding,
    Closed,
    Terminated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum DelegationStatusReason {
    ClosedToDeposits,
    WindingBegun,
    WindingComplete,
    MandateBreached,
    UnauthorizedTradingAttempt,
    RiskLimitBreach,
    AgentAttestationRevoked,
    WindingGraceFailure,
    GovernanceFault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum MandateEnforcementAction {
    AdmissionRejected,
    /// Reserved for the Fill-mode constraint evaluator; no current producer
    /// emits this until fill rollback is wired into matching.
    FillRolledBack,
    BreachApplied,
    PoliciesCascadeRevoked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityTierStatus {
    Tier0Unattested,
    Tier1OperatorAttested,
    Tier2TeeAttested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityAgentKind {
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityAttestationKind {
    OperatorSelf,
    TeeEnclave,
    ThirdParty,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityAttestationClaim {
    RunsCodeHash {
        code_hash: [u8; 32],
        model_hash: Option<[u8; 32]>,
    },
    BoundToMandate {
        mandate_id: [u8; 32],
    },
    RunsCodeAndBoundToMandate {
        code_hash: [u8; 32],
        model_hash: Option<[u8; 32]>,
        mandate_id: [u8; 32],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityAttestationVendor {
    // RFC 050 non-TEE marker emitted for OperatorSignature verification.
    OperatorSelf,
    IntelSgxDcap,
    AmdSevSnp,
    AwsNitro,
    AppleSecureEnclave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityAttestationRevocationReason {
    AttestorRevoked,
    SubjectRequested,
    VendorRootRevoked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum IdentityTierTransitionReason {
    AttestationRegistered,
    AttestationRevoked,
    AttestationExpired,
    OperatorChanged,
    VendorRootChanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum RealCapitalListingMode {
    Disabled,
    CappedPilot,
    Public,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum MarketMakerEnrollmentStatus {
    Active,
    GracePeriod,
    NonCompliant,
    Exited,
    Slashed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum MarketMakerBreachKind {
    Uptime,
    Spread,
    Size,
    TwoSidedWindows,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeEventTopic {
    AccountCreated,
    AccountUpdated,
    AccountOperatorUpdated,
    IntentAccepted,
    IntentPromoted,
    IntentExecuted,
    IntentExpired,
    IntentRejected,
    OrderAccepted,
    OrderFilled,
    FeeCharged,
    FeeRevenueDistributed,
    MarketListed,
    MarketStatusUpdated,
    MarketMakerProgramDefined,
    MarketMakerProgramRetired,
    MarketMakerProgramActionRejected,
    MarketMakerEnrolled,
    MarketMakerEnrollmentExited,
    MmProgramBonusPaid,
    MmProgramBonusForfeited,
    MarketMakerEnrollmentGraceEntered,
    MarketMakerEnrollmentRecovered,
    MarketMakerEnrollmentNonCompliant,
    DelegationOpened,
    DelegationDeposited,
    DelegationRedemptionRequested,
    DelegationRedemptionFinalized,
    DelegationRedemptionCancelled,
    DelegationCrystallized,
    DelegationStatusChanged,
    DelegationActionRejected,
    MandateCreated,
    MandateActivated,
    MandateObjectiveEvaluated,
    MandateConformanceEvaluated,
    MandateConstraintTriggered,
    MandateBreached,
    MandateSuspended,
    MandateResumed,
    MandateRevoked,
    MandateExpired,
    MarketTradingStateUpdated,
    OracleConfigUpdated,
    PositionUpdated,
    MarkPriceUpdated,
    IndexPriceUpdated,
    MarketOpenInterestUpdated,
    FundingIndexUpdated,
    FundingApplied,
    CollateralDeposited,
    CollateralWithdrawn,
    RiskStateUpdated,
    LiquidationCandidateUpdated,
    InsuranceFundUpdated,
    BadDebtUpdated,
    BadDebtRecovered,
    SolvencyBreached,
    LiquidationExecuted,
    LiquidationOutcome,
    LiquidationEscalationUpdated,
    OrderReplaced,
    OrderCancelled,
    PolicyCreated,
    PolicyRevoked,
    IdentityRegistered,
    IdentityUpdated,
    IdentityDeleted,
    AttestationRegistered,
    AttestationVerified,
    AttestationRefreshed,
    AttestationRevoked,
    AttestationExpired,
    TierChanged,
    VendorRootIntroduced,
    VendorRootRevoked,
    VendorCapProfileUpdated,
    TeeGovernanceActionRejected,
    RealCapitalListingUpserted,
    RealCapitalListingDepositEnabledUpdated,
    RealCapitalListingDiscoveryUpdated,
    RealCapitalProtocolCapUpdated,
    RealCapitalGlobalKillSwitchUpdated,
    RealCapitalGovernanceActionRejected,
    RealCapitalInsuranceWalletSpent,
    BridgeDepositLocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum AccountUpdateAction {
    ProposeOwner,
    AcceptOwner,
    Pause,
    Unpause,
    Freeze,
    Unfreeze,
    AddPolicy,
    RemovePolicy,
    Upgrade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum InsuranceFundUpdateSource {
    LiquidationPenalty,
    BankruptcyCover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum RealCapitalInsuranceWalletSpendSource {
    KeeperReward,
    ForcedLiquidationCost,
    BadDebtCover,
    UserPayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum RiskTransitionSource {
    LiquidationSolvency,
    LiquidationStall,
    CollateralAdjustment,
    InsuranceSweep,
    MarkPriceReprice,
    FundingUpdate,
    RiskParameterUpdate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum LiquidationExecutionOutcome {
    ClearedMaintenance,
    StillUnderMaintenance,
    FrozeAccount,
    CreatesBadDebt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum LiquidationEscalationAction {
    Advanced,
    Reset,
    FrozeAccount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OrderLifecycleStatus {
    Filled,
    PartiallyFilledCancelled,
    PartiallyFilledResting,
    Cancelled,
    Resting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OrderFillRole {
    Maker,
    Taker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OrderFillSide {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventFilter {
    pub block_height: Option<u64>,
    pub account_id: Option<u64>,
    pub market_id: Option<u64>,
    pub policy_id: Option<u64>,
    pub intent_id: Option<IntentId>,
    pub client_order_id: Option<u64>,
    pub topic: Option<RuntimeEventTopic>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventCursor {
    pub next_seq: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventPage {
    pub events: Vec<RuntimeEventRecord>,
    pub next_cursor: EventCursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamQuery {
    pub from_seq: u64,
    pub before_seq: Option<u64>,
    pub limit: usize,
    pub filter: EventFilter,
    pub descending: bool,
}

pub struct EventStream<'a> {
    events: &'a [RuntimeEventRecord],
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

impl RuntimeEventKind {
    pub fn account_id(&self) -> Option<u64> {
        match self {
            RuntimeEventKind::AccountCreated { account_id, .. }
            | RuntimeEventKind::AccountUpdated { account_id, .. }
            | RuntimeEventKind::AccountOperatorUpdated { account_id, .. }
            | RuntimeEventKind::OrderAccepted { account_id, .. }
            | RuntimeEventKind::OrderFilled { account_id, .. }
            | RuntimeEventKind::FeeCharged { account_id, .. }
            | RuntimeEventKind::PositionUpdated { account_id, .. }
            | RuntimeEventKind::FundingApplied { account_id, .. }
            | RuntimeEventKind::CollateralDeposited { account_id, .. }
            | RuntimeEventKind::CollateralWithdrawn { account_id, .. }
            | RuntimeEventKind::RiskStateUpdated { account_id, .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { account_id, .. }
            | RuntimeEventKind::SolvencyBreached { account_id, .. }
            | RuntimeEventKind::BadDebtUpdated { account_id, .. }
            | RuntimeEventKind::BadDebtRecovered { account_id, .. }
            | RuntimeEventKind::LiquidationExecuted { account_id, .. }
            | RuntimeEventKind::LiquidationOutcome { account_id, .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { account_id, .. }
            | RuntimeEventKind::OrderReplaced { account_id, .. }
            | RuntimeEventKind::OrderCancelled { account_id, .. }
            | RuntimeEventKind::PolicyCreated { account_id, .. }
            | RuntimeEventKind::PolicyRevoked { account_id, .. } => Some(*account_id),
            RuntimeEventKind::IdentityRegistered { account, .. }
            | RuntimeEventKind::IdentityUpdated { account, .. }
            | RuntimeEventKind::IdentityDeleted { account, .. }
            | RuntimeEventKind::AttestationRegistered { account, .. }
            | RuntimeEventKind::AttestationVerified { account, .. }
            | RuntimeEventKind::AttestationRefreshed { account, .. }
            | RuntimeEventKind::AttestationRevoked { account, .. }
            | RuntimeEventKind::AttestationExpired { account, .. }
            | RuntimeEventKind::TierChanged { account, .. } => Some(*account),
            RuntimeEventKind::DelegationOpened { agent, .. }
            | RuntimeEventKind::DelegationCrystallized { agent, .. }
            | RuntimeEventKind::DelegationStatusChanged { agent, .. }
            | RuntimeEventKind::MandateCreated { agent, .. }
            | RuntimeEventKind::MandateActivated { agent, .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { agent, .. }
            | RuntimeEventKind::MandateConformanceEvaluated { agent, .. }
            | RuntimeEventKind::MandateConstraintTriggered { agent, .. }
            | RuntimeEventKind::MandateBreached { agent, .. }
            | RuntimeEventKind::MandateSuspended { agent, .. }
            | RuntimeEventKind::MandateResumed { agent, .. }
            | RuntimeEventKind::MandateRevoked { agent, .. }
            | RuntimeEventKind::MandateExpired { agent, .. } => Some(*agent),
            RuntimeEventKind::MarketMakerEnrolled {
                market_maker_identity,
                ..
            }
            | RuntimeEventKind::MarketMakerEnrollmentExited {
                market_maker_identity,
                ..
            } => Some(match market_maker_identity {
                MarketMakerIdentity::Account(account_id) => *account_id,
                MarketMakerIdentity::Delegation { serving_agent, .. } => *serving_agent,
            }),
            RuntimeEventKind::MmProgramBonusPaid {
                economic_account_id,
                ..
            }
            | RuntimeEventKind::MmProgramBonusForfeited {
                economic_account_id,
                ..
            }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered {
                economic_account_id,
                ..
            }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered {
                economic_account_id,
                ..
            }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant {
                economic_account_id,
                ..
            } => Some(*economic_account_id),
            RuntimeEventKind::RealCapitalListingUpserted { serving_agent, .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { serving_agent, .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { serving_agent, .. } => {
                Some(*serving_agent)
            }
            RuntimeEventKind::DelegationDeposited { principal, .. }
            | RuntimeEventKind::DelegationRedemptionRequested { principal, .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { principal, .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { principal, .. } => Some(*principal),
            RuntimeEventKind::IntentAccepted { .. }
            | RuntimeEventKind::IntentPromoted { .. }
            | RuntimeEventKind::IntentExecuted { .. }
            | RuntimeEventKind::IntentExpired { .. }
            | RuntimeEventKind::IntentRejected { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }

    pub fn topic(&self) -> RuntimeEventTopic {
        match self {
            RuntimeEventKind::AccountCreated { .. } => RuntimeEventTopic::AccountCreated,
            RuntimeEventKind::AccountUpdated { .. } => RuntimeEventTopic::AccountUpdated,
            RuntimeEventKind::AccountOperatorUpdated { .. } => {
                RuntimeEventTopic::AccountOperatorUpdated
            }
            RuntimeEventKind::IntentAccepted { .. } => RuntimeEventTopic::IntentAccepted,
            RuntimeEventKind::IntentPromoted { .. } => RuntimeEventTopic::IntentPromoted,
            RuntimeEventKind::IntentExecuted { .. } => RuntimeEventTopic::IntentExecuted,
            RuntimeEventKind::IntentExpired { .. } => RuntimeEventTopic::IntentExpired,
            RuntimeEventKind::IntentRejected { .. } => RuntimeEventTopic::IntentRejected,
            RuntimeEventKind::OrderAccepted { .. } => RuntimeEventTopic::OrderAccepted,
            RuntimeEventKind::OrderFilled { .. } => RuntimeEventTopic::OrderFilled,
            RuntimeEventKind::FeeCharged { .. } => RuntimeEventTopic::FeeCharged,
            RuntimeEventKind::FeeRevenueDistributed { .. } => {
                RuntimeEventTopic::FeeRevenueDistributed
            }
            RuntimeEventKind::MarketListed { .. } => RuntimeEventTopic::MarketListed,
            RuntimeEventKind::MarketStatusUpdated { .. } => RuntimeEventTopic::MarketStatusUpdated,
            RuntimeEventKind::MarketMakerProgramDefined { .. } => {
                RuntimeEventTopic::MarketMakerProgramDefined
            }
            RuntimeEventKind::MarketMakerProgramRetired { .. } => {
                RuntimeEventTopic::MarketMakerProgramRetired
            }
            RuntimeEventKind::MarketMakerProgramActionRejected { .. } => {
                RuntimeEventTopic::MarketMakerProgramActionRejected
            }
            RuntimeEventKind::MarketMakerEnrolled { .. } => RuntimeEventTopic::MarketMakerEnrolled,
            RuntimeEventKind::MarketMakerEnrollmentExited { .. } => {
                RuntimeEventTopic::MarketMakerEnrollmentExited
            }
            RuntimeEventKind::MmProgramBonusPaid { .. } => RuntimeEventTopic::MmProgramBonusPaid,
            RuntimeEventKind::MmProgramBonusForfeited { .. } => {
                RuntimeEventTopic::MmProgramBonusForfeited
            }
            RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. } => {
                RuntimeEventTopic::MarketMakerEnrollmentGraceEntered
            }
            RuntimeEventKind::MarketMakerEnrollmentRecovered { .. } => {
                RuntimeEventTopic::MarketMakerEnrollmentRecovered
            }
            RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. } => {
                RuntimeEventTopic::MarketMakerEnrollmentNonCompliant
            }
            RuntimeEventKind::DelegationOpened { .. } => RuntimeEventTopic::DelegationOpened,
            RuntimeEventKind::DelegationDeposited { .. } => RuntimeEventTopic::DelegationDeposited,
            RuntimeEventKind::DelegationRedemptionRequested { .. } => {
                RuntimeEventTopic::DelegationRedemptionRequested
            }
            RuntimeEventKind::DelegationRedemptionFinalized { .. } => {
                RuntimeEventTopic::DelegationRedemptionFinalized
            }
            RuntimeEventKind::DelegationRedemptionCancelled { .. } => {
                RuntimeEventTopic::DelegationRedemptionCancelled
            }
            RuntimeEventKind::DelegationCrystallized { .. } => {
                RuntimeEventTopic::DelegationCrystallized
            }
            RuntimeEventKind::DelegationStatusChanged { .. } => {
                RuntimeEventTopic::DelegationStatusChanged
            }
            RuntimeEventKind::DelegationActionRejected { .. } => {
                RuntimeEventTopic::DelegationActionRejected
            }
            RuntimeEventKind::MandateCreated { .. } => RuntimeEventTopic::MandateCreated,
            RuntimeEventKind::MandateActivated { .. } => RuntimeEventTopic::MandateActivated,
            RuntimeEventKind::MandateObjectiveEvaluated { .. } => {
                RuntimeEventTopic::MandateObjectiveEvaluated
            }
            RuntimeEventKind::MandateConformanceEvaluated { .. } => {
                RuntimeEventTopic::MandateConformanceEvaluated
            }
            RuntimeEventKind::MandateConstraintTriggered { .. } => {
                RuntimeEventTopic::MandateConstraintTriggered
            }
            RuntimeEventKind::MandateBreached { .. } => RuntimeEventTopic::MandateBreached,
            RuntimeEventKind::MandateSuspended { .. } => RuntimeEventTopic::MandateSuspended,
            RuntimeEventKind::MandateResumed { .. } => RuntimeEventTopic::MandateResumed,
            RuntimeEventKind::MandateRevoked { .. } => RuntimeEventTopic::MandateRevoked,
            RuntimeEventKind::MandateExpired { .. } => RuntimeEventTopic::MandateExpired,
            RuntimeEventKind::MarketTradingStateUpdated { .. } => {
                RuntimeEventTopic::MarketTradingStateUpdated
            }
            RuntimeEventKind::OracleConfigUpdated { .. } => RuntimeEventTopic::OracleConfigUpdated,
            RuntimeEventKind::PositionUpdated { .. } => RuntimeEventTopic::PositionUpdated,
            RuntimeEventKind::MarkPriceUpdated { .. } => RuntimeEventTopic::MarkPriceUpdated,
            RuntimeEventKind::IndexPriceUpdated { .. } => RuntimeEventTopic::IndexPriceUpdated,
            RuntimeEventKind::MarketOpenInterestUpdated { .. } => {
                RuntimeEventTopic::MarketOpenInterestUpdated
            }
            RuntimeEventKind::FundingIndexUpdated { .. } => RuntimeEventTopic::FundingIndexUpdated,
            RuntimeEventKind::FundingApplied { .. } => RuntimeEventTopic::FundingApplied,
            RuntimeEventKind::CollateralDeposited { .. } => RuntimeEventTopic::CollateralDeposited,
            RuntimeEventKind::CollateralWithdrawn { .. } => RuntimeEventTopic::CollateralWithdrawn,
            RuntimeEventKind::RiskStateUpdated { .. } => RuntimeEventTopic::RiskStateUpdated,
            RuntimeEventKind::LiquidationCandidateUpdated { .. } => {
                RuntimeEventTopic::LiquidationCandidateUpdated
            }
            RuntimeEventKind::InsuranceFundUpdated { .. } => {
                RuntimeEventTopic::InsuranceFundUpdated
            }
            RuntimeEventKind::BadDebtUpdated { .. } => RuntimeEventTopic::BadDebtUpdated,
            RuntimeEventKind::BadDebtRecovered { .. } => RuntimeEventTopic::BadDebtRecovered,
            RuntimeEventKind::SolvencyBreached { .. } => RuntimeEventTopic::SolvencyBreached,
            RuntimeEventKind::LiquidationExecuted { .. } => RuntimeEventTopic::LiquidationExecuted,
            RuntimeEventKind::LiquidationOutcome { .. } => RuntimeEventTopic::LiquidationOutcome,
            RuntimeEventKind::LiquidationEscalationUpdated { .. } => {
                RuntimeEventTopic::LiquidationEscalationUpdated
            }
            RuntimeEventKind::OrderReplaced { .. } => RuntimeEventTopic::OrderReplaced,
            RuntimeEventKind::OrderCancelled { .. } => RuntimeEventTopic::OrderCancelled,
            RuntimeEventKind::PolicyCreated { .. } => RuntimeEventTopic::PolicyCreated,
            RuntimeEventKind::PolicyRevoked { .. } => RuntimeEventTopic::PolicyRevoked,
            RuntimeEventKind::IdentityRegistered { .. } => RuntimeEventTopic::IdentityRegistered,
            RuntimeEventKind::IdentityUpdated { .. } => RuntimeEventTopic::IdentityUpdated,
            RuntimeEventKind::IdentityDeleted { .. } => RuntimeEventTopic::IdentityDeleted,
            RuntimeEventKind::AttestationRegistered { .. } => {
                RuntimeEventTopic::AttestationRegistered
            }
            RuntimeEventKind::AttestationVerified { .. } => RuntimeEventTopic::AttestationVerified,
            RuntimeEventKind::AttestationRefreshed { .. } => {
                RuntimeEventTopic::AttestationRefreshed
            }
            RuntimeEventKind::AttestationRevoked { .. } => RuntimeEventTopic::AttestationRevoked,
            RuntimeEventKind::AttestationExpired { .. } => RuntimeEventTopic::AttestationExpired,
            RuntimeEventKind::TierChanged { .. } => RuntimeEventTopic::TierChanged,
            RuntimeEventKind::VendorRootIntroduced { .. } => {
                RuntimeEventTopic::VendorRootIntroduced
            }
            RuntimeEventKind::VendorRootRevoked { .. } => RuntimeEventTopic::VendorRootRevoked,
            RuntimeEventKind::VendorCapProfileUpdated { .. } => {
                RuntimeEventTopic::VendorCapProfileUpdated
            }
            RuntimeEventKind::TeeGovernanceActionRejected { .. } => {
                RuntimeEventTopic::TeeGovernanceActionRejected
            }
            RuntimeEventKind::RealCapitalListingUpserted { .. } => {
                RuntimeEventTopic::RealCapitalListingUpserted
            }
            RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. } => {
                RuntimeEventTopic::RealCapitalListingDepositEnabledUpdated
            }
            RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. } => {
                RuntimeEventTopic::RealCapitalListingDiscoveryUpdated
            }
            RuntimeEventKind::RealCapitalProtocolCapUpdated { .. } => {
                RuntimeEventTopic::RealCapitalProtocolCapUpdated
            }
            RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. } => {
                RuntimeEventTopic::RealCapitalGlobalKillSwitchUpdated
            }
            RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. } => {
                RuntimeEventTopic::RealCapitalInsuranceWalletSpent
            }
            RuntimeEventKind::RealCapitalGovernanceActionRejected { .. } => {
                RuntimeEventTopic::RealCapitalGovernanceActionRejected
            }
            RuntimeEventKind::BridgeDepositLocked { .. } => RuntimeEventTopic::BridgeDepositLocked,
        }
    }

    pub fn policy_id(&self) -> Option<u64> {
        match self {
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::OrderAccepted { .. }
            | RuntimeEventKind::OrderFilled { .. }
            | RuntimeEventKind::FeeCharged { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::OrderReplaced { .. }
            | RuntimeEventKind::OrderCancelled { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
            RuntimeEventKind::IntentAccepted { policy_id, .. }
            | RuntimeEventKind::IntentPromoted { policy_id, .. }
            | RuntimeEventKind::IntentExecuted { policy_id, .. }
            | RuntimeEventKind::IntentExpired { policy_id, .. }
            | RuntimeEventKind::IntentRejected { policy_id, .. }
            | RuntimeEventKind::PolicyRevoked { policy_id, .. } => Some(*policy_id),
            RuntimeEventKind::PolicyCreated { policy_id, .. } => Some(*policy_id),
        }
    }

    pub fn market_id(&self) -> Option<u64> {
        match self {
            RuntimeEventKind::OrderAccepted { market_id, .. }
            | RuntimeEventKind::OrderFilled { market_id, .. }
            | RuntimeEventKind::FeeCharged { market_id, .. }
            | RuntimeEventKind::FeeRevenueDistributed { market_id, .. }
            | RuntimeEventKind::MarketListed { market_id, .. }
            | RuntimeEventKind::MarketStatusUpdated { market_id, .. }
            | RuntimeEventKind::MarketTradingStateUpdated { market_id, .. }
            | RuntimeEventKind::OracleConfigUpdated { market_id, .. }
            | RuntimeEventKind::PositionUpdated { market_id, .. }
            | RuntimeEventKind::MarkPriceUpdated { market_id, .. }
            | RuntimeEventKind::IndexPriceUpdated { market_id, .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { market_id, .. }
            | RuntimeEventKind::FundingIndexUpdated { market_id, .. }
            | RuntimeEventKind::FundingApplied { market_id, .. }
            | RuntimeEventKind::CollateralDeposited { market_id, .. }
            | RuntimeEventKind::CollateralWithdrawn { market_id, .. }
            | RuntimeEventKind::RiskStateUpdated { market_id, .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { market_id, .. }
            | RuntimeEventKind::InsuranceFundUpdated { market_id, .. }
            | RuntimeEventKind::BadDebtUpdated { market_id, .. }
            | RuntimeEventKind::BadDebtRecovered { market_id, .. }
            | RuntimeEventKind::SolvencyBreached { market_id, .. }
            | RuntimeEventKind::LiquidationExecuted { market_id, .. }
            | RuntimeEventKind::LiquidationOutcome { market_id, .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { market_id, .. }
            | RuntimeEventKind::MarketMakerEnrolled { market_id, .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { market_id, .. }
            | RuntimeEventKind::MmProgramBonusPaid { market_id, .. }
            | RuntimeEventKind::MmProgramBonusForfeited { market_id, .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { market_id, .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { market_id, .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { market_id, .. }
            | RuntimeEventKind::OrderCancelled { market_id, .. }
            | RuntimeEventKind::OrderReplaced { market_id, .. } => Some(*market_id),
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::IntentAccepted { .. }
            | RuntimeEventKind::IntentPromoted { .. }
            | RuntimeEventKind::IntentExecuted { .. }
            | RuntimeEventKind::IntentExpired { .. }
            | RuntimeEventKind::IntentRejected { .. }
            | RuntimeEventKind::PolicyCreated { .. }
            | RuntimeEventKind::PolicyRevoked { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }

    pub fn intent_id(&self) -> Option<IntentId> {
        match self {
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::OrderAccepted { .. }
            | RuntimeEventKind::OrderFilled { .. }
            | RuntimeEventKind::FeeCharged { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::OrderReplaced { .. }
            | RuntimeEventKind::OrderCancelled { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
            RuntimeEventKind::IntentAccepted { intent_id, .. }
            | RuntimeEventKind::IntentPromoted { intent_id, .. }
            | RuntimeEventKind::IntentExecuted { intent_id, .. }
            | RuntimeEventKind::IntentExpired { intent_id, .. }
            | RuntimeEventKind::IntentRejected { intent_id, .. } => Some(*intent_id),
            RuntimeEventKind::PolicyCreated { .. } | RuntimeEventKind::PolicyRevoked { .. } => None,
        }
    }

    pub fn client_order_id(&self) -> Option<u64> {
        match self {
            RuntimeEventKind::OrderAccepted {
                client_order_id, ..
            }
            | RuntimeEventKind::OrderFilled {
                client_order_id, ..
            }
            | RuntimeEventKind::FeeCharged {
                client_order_id, ..
            }
            | RuntimeEventKind::OrderCancelled {
                client_order_id, ..
            } => Some(*client_order_id),
            RuntimeEventKind::OrderReplaced {
                replacement_client_order_id,
                ..
            } => Some(*replacement_client_order_id),
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::IntentAccepted { .. }
            | RuntimeEventKind::IntentPromoted { .. }
            | RuntimeEventKind::IntentExecuted { .. }
            | RuntimeEventKind::IntentExpired { .. }
            | RuntimeEventKind::IntentRejected { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::PolicyCreated { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::PolicyRevoked { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }

    pub fn intent_nonce(&self) -> Option<u64> {
        match self {
            RuntimeEventKind::IntentAccepted { nonce, .. }
            | RuntimeEventKind::IntentPromoted { nonce, .. }
            | RuntimeEventKind::IntentExecuted { nonce, .. }
            | RuntimeEventKind::IntentExpired { nonce, .. }
            | RuntimeEventKind::IntentRejected { nonce, .. } => Some(*nonce),
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::OrderAccepted { .. }
            | RuntimeEventKind::OrderFilled { .. }
            | RuntimeEventKind::FeeCharged { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::OrderReplaced { .. }
            | RuntimeEventKind::OrderCancelled { .. }
            | RuntimeEventKind::PolicyCreated { .. }
            | RuntimeEventKind::PolicyRevoked { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }

    pub fn intent_status_stage(&self) -> Option<IntentStatusStage> {
        match self {
            RuntimeEventKind::IntentAccepted {
                class: IntentAdmissionClass::Direct,
                ..
            } => Some(IntentStatusStage::AcceptedDirect),
            RuntimeEventKind::IntentAccepted {
                class: IntentAdmissionClass::Conditional,
                ..
            } => Some(IntentStatusStage::AcceptedConditional),
            RuntimeEventKind::IntentPromoted { .. } => Some(IntentStatusStage::Promoted),
            RuntimeEventKind::IntentExecuted { .. } => Some(IntentStatusStage::Executed),
            RuntimeEventKind::IntentExpired { .. } => Some(IntentStatusStage::Expired),
            RuntimeEventKind::IntentRejected { .. } => Some(IntentStatusStage::Rejected),
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::OrderAccepted { .. }
            | RuntimeEventKind::OrderFilled { .. }
            | RuntimeEventKind::FeeCharged { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::OrderReplaced { .. }
            | RuntimeEventKind::OrderCancelled { .. }
            | RuntimeEventKind::PolicyCreated { .. }
            | RuntimeEventKind::PolicyRevoked { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }

    pub fn rejection_reason(&self) -> Option<IntentRejectionReason> {
        match self {
            RuntimeEventKind::IntentRejected { reason, .. } => Some(*reason),
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::IntentAccepted { .. }
            | RuntimeEventKind::IntentPromoted { .. }
            | RuntimeEventKind::IntentExecuted { .. }
            | RuntimeEventKind::IntentExpired { .. }
            | RuntimeEventKind::OrderAccepted { .. }
            | RuntimeEventKind::OrderFilled { .. }
            | RuntimeEventKind::FeeCharged { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::OrderReplaced { .. }
            | RuntimeEventKind::OrderCancelled { .. }
            | RuntimeEventKind::PolicyCreated { .. }
            | RuntimeEventKind::PolicyRevoked { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }

    pub fn order_rejection_reason(&self) -> Option<OrderRejectionReason> {
        match self {
            RuntimeEventKind::IntentRejected { order_reason, .. } => *order_reason,
            RuntimeEventKind::AccountCreated { .. }
            | RuntimeEventKind::AccountUpdated { .. }
            | RuntimeEventKind::AccountOperatorUpdated { .. }
            | RuntimeEventKind::IntentAccepted { .. }
            | RuntimeEventKind::IntentPromoted { .. }
            | RuntimeEventKind::IntentExecuted { .. }
            | RuntimeEventKind::IntentExpired { .. }
            | RuntimeEventKind::OrderAccepted { .. }
            | RuntimeEventKind::OrderFilled { .. }
            | RuntimeEventKind::FeeCharged { .. }
            | RuntimeEventKind::FeeRevenueDistributed { .. }
            | RuntimeEventKind::MarketListed { .. }
            | RuntimeEventKind::MarketStatusUpdated { .. }
            | RuntimeEventKind::MarketMakerProgramDefined { .. }
            | RuntimeEventKind::MarketMakerProgramRetired { .. }
            | RuntimeEventKind::MarketMakerProgramActionRejected { .. }
            | RuntimeEventKind::MarketMakerEnrolled { .. }
            | RuntimeEventKind::MarketMakerEnrollmentExited { .. }
            | RuntimeEventKind::DelegationOpened { .. }
            | RuntimeEventKind::DelegationDeposited { .. }
            | RuntimeEventKind::DelegationRedemptionRequested { .. }
            | RuntimeEventKind::DelegationRedemptionFinalized { .. }
            | RuntimeEventKind::DelegationRedemptionCancelled { .. }
            | RuntimeEventKind::DelegationCrystallized { .. }
            | RuntimeEventKind::DelegationStatusChanged { .. }
            | RuntimeEventKind::DelegationActionRejected { .. }
            | RuntimeEventKind::MandateCreated { .. }
            | RuntimeEventKind::MandateActivated { .. }
            | RuntimeEventKind::MandateObjectiveEvaluated { .. }
            | RuntimeEventKind::MandateConformanceEvaluated { .. }
            | RuntimeEventKind::MandateConstraintTriggered { .. }
            | RuntimeEventKind::MandateBreached { .. }
            | RuntimeEventKind::MandateSuspended { .. }
            | RuntimeEventKind::MandateResumed { .. }
            | RuntimeEventKind::MandateRevoked { .. }
            | RuntimeEventKind::MandateExpired { .. }
            | RuntimeEventKind::MarketTradingStateUpdated { .. }
            | RuntimeEventKind::OracleConfigUpdated { .. }
            | RuntimeEventKind::PositionUpdated { .. }
            | RuntimeEventKind::FundingIndexUpdated { .. }
            | RuntimeEventKind::FundingApplied { .. }
            | RuntimeEventKind::MarkPriceUpdated { .. }
            | RuntimeEventKind::IndexPriceUpdated { .. }
            | RuntimeEventKind::MarketOpenInterestUpdated { .. }
            | RuntimeEventKind::CollateralDeposited { .. }
            | RuntimeEventKind::CollateralWithdrawn { .. }
            | RuntimeEventKind::RiskStateUpdated { .. }
            | RuntimeEventKind::LiquidationCandidateUpdated { .. }
            | RuntimeEventKind::InsuranceFundUpdated { .. }
            | RuntimeEventKind::BadDebtUpdated { .. }
            | RuntimeEventKind::BadDebtRecovered { .. }
            | RuntimeEventKind::SolvencyBreached { .. }
            | RuntimeEventKind::LiquidationExecuted { .. }
            | RuntimeEventKind::LiquidationOutcome { .. }
            | RuntimeEventKind::LiquidationEscalationUpdated { .. }
            | RuntimeEventKind::OrderReplaced { .. }
            | RuntimeEventKind::OrderCancelled { .. }
            | RuntimeEventKind::PolicyCreated { .. }
            | RuntimeEventKind::PolicyRevoked { .. }
            | RuntimeEventKind::IdentityRegistered { .. }
            | RuntimeEventKind::IdentityUpdated { .. }
            | RuntimeEventKind::IdentityDeleted { .. }
            | RuntimeEventKind::AttestationRegistered { .. }
            | RuntimeEventKind::AttestationVerified { .. }
            | RuntimeEventKind::AttestationRefreshed { .. }
            | RuntimeEventKind::AttestationRevoked { .. }
            | RuntimeEventKind::AttestationExpired { .. }
            | RuntimeEventKind::TierChanged { .. }
            | RuntimeEventKind::VendorRootIntroduced { .. }
            | RuntimeEventKind::VendorRootRevoked { .. }
            | RuntimeEventKind::VendorCapProfileUpdated { .. }
            | RuntimeEventKind::TeeGovernanceActionRejected { .. }
            | RuntimeEventKind::RealCapitalListingUpserted { .. }
            | RuntimeEventKind::RealCapitalListingDepositEnabledUpdated { .. }
            | RuntimeEventKind::RealCapitalListingDiscoveryUpdated { .. }
            | RuntimeEventKind::RealCapitalProtocolCapUpdated { .. }
            | RuntimeEventKind::RealCapitalGlobalKillSwitchUpdated { .. }
            | RuntimeEventKind::RealCapitalInsuranceWalletSpent { .. }
            | RuntimeEventKind::RealCapitalGovernanceActionRejected { .. }
            | RuntimeEventKind::MmProgramBonusPaid { .. }
            | RuntimeEventKind::MmProgramBonusForfeited { .. }
            | RuntimeEventKind::MarketMakerEnrollmentGraceEntered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentRecovered { .. }
            | RuntimeEventKind::MarketMakerEnrollmentNonCompliant { .. }
            | RuntimeEventKind::BridgeDepositLocked { .. } => None,
        }
    }
}

impl EventFilter {
    pub fn matches(&self, event: &RuntimeEventRecord) -> bool {
        if let Some(block_height) = self.block_height {
            if event.block_height != Some(block_height) {
                return false;
            }
        }

        if let Some(topic) = self.topic {
            if event.kind.topic() != topic {
                return false;
            }
        }

        if let Some(account_id) = self.account_id {
            if event.kind.account_id() != Some(account_id) {
                return false;
            }
        }

        if let Some(market_id) = self.market_id {
            if event.kind.market_id() != Some(market_id) {
                return false;
            }
        }

        if let Some(policy_id) = self.policy_id {
            if event.kind.policy_id() != Some(policy_id) {
                return false;
            }
        }

        if let Some(intent_id) = self.intent_id {
            if event.kind.intent_id() != Some(intent_id) {
                return false;
            }
        }

        if let Some(client_order_id) = self.client_order_id {
            if event.kind.client_order_id() != Some(client_order_id) {
                return false;
            }
        }

        true
    }
}

impl<'a> EventStream<'a> {
    pub fn new(events: &'a [RuntimeEventRecord]) -> Self {
        Self { events }
    }

    pub fn query(&self, query: StreamQuery) -> EventPage {
        let events: Vec<_> = if query.descending {
            let before_seq = query.before_seq.unwrap_or(u64::MAX);
            let end = self.events.partition_point(|event| event.seq < before_seq);
            self.events[..end]
                .iter()
                .rev()
                .filter(|event| query.filter.matches(event))
                .take(query.limit)
                .cloned()
                .collect()
        } else {
            let before_seq = query.before_seq.unwrap_or(u64::MAX);
            let start = self
                .events
                .partition_point(|event| event.seq < query.from_seq);
            let end = start + self.events[start..].partition_point(|event| event.seq < before_seq);
            self.events[start..end]
                .iter()
                .filter(|event| query.filter.matches(event))
                .take(query.limit)
                .cloned()
                .collect()
        };

        let next_seq = if query.descending {
            events
                .last()
                .map(|event| event.seq)
                .unwrap_or(query.before_seq.unwrap_or(0))
        } else {
            events
                .last()
                .map(|event| event.seq.saturating_add(1))
                .unwrap_or(query.from_seq)
        };

        EventPage {
            events,
            next_cursor: EventCursor { next_seq },
        }
    }

    pub fn latest_intent_event(&self, intent_id: IntentId) -> Option<&RuntimeEventRecord> {
        self.events
            .iter()
            .rev()
            .find(|event| event.kind.intent_id() == Some(intent_id))
    }
}

#[cfg(test)]
#[path = "streams_tests.rs"]
mod tests;
