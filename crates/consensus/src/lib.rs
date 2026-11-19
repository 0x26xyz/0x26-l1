use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::{Signature as Ed25519Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::cmp::min;
use std::collections::{BTreeMap, BTreeSet};

pub const CRATE_NAME: &str = "consensus";
pub mod mahi;

pub type Hash = [u8; 32];
pub type AccountId = u64;
pub type PolicyId = u64;
pub type IntentId = Hash;
pub type DelegationActionId = Hash;
pub type MarketMakerProgramActionId = Hash;
pub type MarketMakerProgramId = Hash;
pub type TeeGovernanceActionId = Hash;
pub type RealCapitalGovernanceActionId = Hash;
pub type ValidatorId = [u8; 32];
pub type SignatureBytes = [u8; 96];
pub type SessionSignature = [u8; 64];
pub type ConsensusPubKey = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorVerifier {
    pub validator: ValidatorId,
    pub consensus_pubkey: ConsensusPubKey,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PolicyTermsWire {
    pub gas_budget_per_day: u128,
    pub gas_refill_per_day: u128,
    pub max_intents_per_second: u32,
    pub max_intents_per_day: u32,
    pub max_volume_per_day: u128,
    pub max_drawdown_pct: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Intent {
    pub chain_id: u64,
    pub account: AccountId,
    pub policy_id: PolicyId,
    pub nonce: u64,
    pub expires_at: u64,
    pub predicate: Option<IntentPredicate>,
    pub kind: IntentKind,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum IntentKind {
    Transfer {
        to: AccountId,
        token: u32,
        amount: u64,
    },
    Cancel {
        order_id: u64,
    },
    PlaceOrder {
        market_id: u64,
        client_order_id: u64,
        side: IntentOrderSide,
        quantity_lots: u64,
        kind: IntentOrderKind,
        time_in_force: IntentTimeInForce,
        post_only: bool,
    },
    CancelOrder {
        market_id: u64,
        client_order_id: u64,
    },
    ReplaceOrder {
        market_id: u64,
        target_client_order_id: u64,
        replacement_client_order_id: u64,
        side: IntentOrderSide,
        quantity_lots: u64,
        kind: IntentOrderKind,
        time_in_force: IntentTimeInForce,
        post_only: bool,
    },
    CreateSubPolicy {
        child_policy_id: PolicyId,
        child_grantee: [u8; 32],
        child_expires_at: u64,
        child_terms: PolicyTermsWire,
    },
    RevokePolicy {
        target_policy_id: PolicyId,
    },
    DepositCollateral {
        market_id: u64,
        amount_usdc: u64,
    },
    WithdrawCollateral {
        market_id: u64,
        amount_usdc: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum IntentPredicate {
    TimeAfter { not_before_ms: u64 },
    TimeBefore { not_after_ms: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum IntentOrderSide {
    Bid,
    Ask,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum IntentOrderKind {
    Limit { price_ticks: u64 },
    Market,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum IntentTimeInForce {
    Gtc,
    Ioc,
    Fok,
    Gtt { expires_at_ms: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockHeader {
    pub chain_id: u64,
    pub height: u64,
    pub round: u64,
    pub proposer: ValidatorId,
    pub parent_hash: Hash,
    pub parent_state_root: Hash,
    pub tx_root: Hash,
    pub event_root: Hash,
    pub state_root: Hash,
    pub qc: QuorumCertificate,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockBody {
    pub items: Vec<BlockItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockItem {
    OraclePrice(BlockOraclePrice),
    Intent(SignedIntent),
    Transaction(SignedTransaction),
    Funding(BlockFunding),
    Faucet(BlockFaucet),
    Liquidation(BlockLiquidation),
    Delegation(BlockDelegation),
    MarketMakerProgram(BlockMarketMakerProgram),
    TeeGovernance(BlockTeeGovernance),
    RealCapitalGovernance(BlockRealCapitalGovernance),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedIntent {
    pub intent: Intent,
    pub signature: SessionSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SignedTransaction {
    pub signer: ValidatorId,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockFunding {
    pub market_id: u64,
    pub funding_index: i128,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockOraclePrice {
    pub market_id: u64,
    pub mark: Option<BlockOracleValue>,
    pub index: Option<BlockOracleValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockOracleValue {
    pub price_ticks: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockFaucet {
    pub address: [u8; 20],
    pub policy_target_micro_usdc: [u8; 32],
    pub evm_target_micro_usdc: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockLiquidation {
    pub market_id: u64,
    pub account_id: u64,
    pub liquidator_account_id: u64,
    pub client_order_id: u64,
    pub requested_lots: Option<u64>,
    pub filled_lots: u64,
    pub remaining_lots: u64,
    pub keeper_reward_micro_usdc: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockDelegation {
    pub action_id: DelegationActionId,
    pub timestamp_ms: u64,
    pub action: BlockDelegationAction,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockMarketMakerProgram {
    pub action_id: MarketMakerProgramActionId,
    pub timestamp_ms: u64,
    pub action: BlockMarketMakerProgramAction,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockTeeGovernance {
    pub action_id: TeeGovernanceActionId,
    pub timestamp_ms: u64,
    pub action: BlockTeeGovernanceAction,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BlockRealCapitalGovernance {
    pub action_id: RealCapitalGovernanceActionId,
    pub timestamp_ms: u64,
    pub action: BlockRealCapitalGovernanceAction,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockTeeGovernanceAction {
    IntroduceVendorRoot {
        vendor: BlockTeeVendor,
        payload: BlockVendorRootPayload,
        valid_from_block: u64,
    },
    RevokeVendorRoot {
        vendor: BlockTeeVendor,
        root_id: Hash,
        valid_until_block: u64,
    },
    UpdateVendorCapProfile {
        vendor: BlockTeeVendor,
        cap_multiplier_microunits: u128,
        composition_eligibility: bool,
        valid_from_block: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum BlockTeeVendor {
    IntelSgxDcap,
    AmdSevSnp,
    AwsNitro,
    AppleSecureEnclave,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockVendorRootPayload {
    IntelSgxRoot {
        root_cert_der: Vec<u8>,
        tcb_info_signing_cert_der: Vec<u8>,
        tcb_info_url: String,
        crl_der: Vec<u8>,
    },
    AmdSevRoot {
        ark_cert_der: Vec<u8>,
        ask_cert_der: Vec<u8>,
        crl_der: Vec<u8>,
    },
    AwsNitroRoot {
        root_cert_der: Vec<u8>,
    },
    AppleSecureEnclaveRoot {
        root_cert_der: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockRealCapitalGovernanceAction {
    UpsertListing {
        vault_id: [u8; 16],
        delegation_id: Hash,
        serving_agent: u64,
        mode: BlockVaultRealCapitalMode,
        pilot_min_serving_agent_tier: Option<BlockIdentityTier>,
        tier0_pilot_exception: bool,
        deposit_enabled: bool,
        listed_for_discovery: bool,
        per_principal_micro_usdc: u128,
        total_vault_tvl_micro_usdc: u128,
        pilot_principal_allowlist: Option<BTreeSet<u64>>,
    },
    SetListingDepositEnabled {
        vault_id: [u8; 16],
        deposit_enabled: bool,
    },
    SetListingDiscovery {
        vault_id: [u8; 16],
        listed_for_discovery: bool,
    },
    UpdateProtocolCap {
        total_protocol_tvl_micro_usdc: Option<u128>,
    },
    UpdateGlobalKillSwitch {
        disabled: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockVaultRealCapitalMode {
    Disabled,
    CappedPilot,
    Public,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockIdentityTier {
    Tier0Unattested,
    Tier1OperatorAttested,
    Tier2TeeAttested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum MarketMakerIdentity {
    Account(u64),
    Delegation {
        delegation_id: [u8; 32],
        serving_agent: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockMarketMakerProgramAction {
    Define {
        name: String,
        markets: BTreeSet<u64>,
        min_uptime_bps: u16,
        max_spread_bps: u16,
        min_quote_size_micro_usdc: [u8; 32],
        min_two_sided_windows_per_hour: u16,
        rebate_multiplier_units: u16,
        enrollment_bond_micro_usdc: [u8; 32],
        compliance_window_ms: u64,
        observation_cadence_ms: u64,
    },
    Retire {
        program_id: MarketMakerProgramId,
    },
    Enroll {
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: MarketMakerProgramId,
        bond_micro_usdc: [u8; 32],
    },
    Exit {
        market_maker_identity: MarketMakerIdentity,
        market_id: u64,
        program_id: MarketMakerProgramId,
    },
}

pub const MARKET_MAKER_PROGRAM_ACTION_KIND_DEFINE: &str = "define";
pub const MARKET_MAKER_PROGRAM_ACTION_KIND_RETIRE: &str = "retire";
pub const MARKET_MAKER_PROGRAM_ACTION_KIND_ENROLL: &str = "enroll";
pub const MARKET_MAKER_PROGRAM_ACTION_KIND_EXIT: &str = "exit";
pub const TEE_GOVERNANCE_ACTION_KIND_INTRODUCE_VENDOR_ROOT: &str = "introduce_vendor_root";
pub const TEE_GOVERNANCE_ACTION_KIND_REVOKE_VENDOR_ROOT: &str = "revoke_vendor_root";
pub const TEE_GOVERNANCE_ACTION_KIND_UPDATE_VENDOR_CAP_PROFILE: &str = "update_vendor_cap_profile";
pub const REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPSERT_LISTING: &str = "upsert_listing";
pub const REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DEPOSIT_ENABLED: &str =
    "set_listing_deposit_enabled";
pub const REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DISCOVERY: &str = "set_listing_discovery";
pub const REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_PROTOCOL_CAP: &str = "update_protocol_cap";
pub const REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_GLOBAL_KILL_SWITCH: &str =
    "update_global_kill_switch";

pub fn market_maker_program_block_action_kind(
    action: &BlockMarketMakerProgramAction,
) -> &'static str {
    match action {
        BlockMarketMakerProgramAction::Define { .. } => MARKET_MAKER_PROGRAM_ACTION_KIND_DEFINE,
        BlockMarketMakerProgramAction::Retire { .. } => MARKET_MAKER_PROGRAM_ACTION_KIND_RETIRE,
        BlockMarketMakerProgramAction::Enroll { .. } => MARKET_MAKER_PROGRAM_ACTION_KIND_ENROLL,
        BlockMarketMakerProgramAction::Exit { .. } => MARKET_MAKER_PROGRAM_ACTION_KIND_EXIT,
    }
}

pub fn tee_governance_block_action_kind(action: &BlockTeeGovernanceAction) -> &'static str {
    match action {
        BlockTeeGovernanceAction::IntroduceVendorRoot { .. } => {
            TEE_GOVERNANCE_ACTION_KIND_INTRODUCE_VENDOR_ROOT
        }
        BlockTeeGovernanceAction::RevokeVendorRoot { .. } => {
            TEE_GOVERNANCE_ACTION_KIND_REVOKE_VENDOR_ROOT
        }
        BlockTeeGovernanceAction::UpdateVendorCapProfile { .. } => {
            TEE_GOVERNANCE_ACTION_KIND_UPDATE_VENDOR_CAP_PROFILE
        }
    }
}

pub fn real_capital_governance_block_action_kind(
    action: &BlockRealCapitalGovernanceAction,
) -> &'static str {
    match action {
        BlockRealCapitalGovernanceAction::UpsertListing { .. } => {
            REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPSERT_LISTING
        }
        BlockRealCapitalGovernanceAction::SetListingDepositEnabled { .. } => {
            REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DEPOSIT_ENABLED
        }
        BlockRealCapitalGovernanceAction::SetListingDiscovery { .. } => {
            REAL_CAPITAL_GOVERNANCE_ACTION_KIND_SET_LISTING_DISCOVERY
        }
        BlockRealCapitalGovernanceAction::UpdateProtocolCap { .. } => {
            REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_PROTOCOL_CAP
        }
        BlockRealCapitalGovernanceAction::UpdateGlobalKillSwitch { .. } => {
            REAL_CAPITAL_GOVERNANCE_ACTION_KIND_UPDATE_GLOBAL_KILL_SWITCH
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockDelegationFaultReason {
    MandateBreached,
    UnauthorizedTradingAttempt,
    RiskLimitBreach,
    AgentAttestationRevoked,
    WindingGraceFailure,
    GovernanceFault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockDelegationSlashPolicy {
    PrincipalProRata,
    WindingGrace,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BlockDelegationAction {
    Open {
        agent: u64,
        mandate_id: [u8; 32],
        performance_share_units: u16,
        crystallization_cadence_ms: u64,
        notice_period_ms: u64,
        agent_bond: u128,
    },
    Deposit {
        principal: u64,
        delegation_id: [u8; 32],
        amount: u128,
    },
    RequestRedemption {
        principal: u64,
        delegation_id: [u8; 32],
        shares: u128,
    },
    FinalizeRedemption {
        request_id: [u8; 32],
    },
    CancelRedemption {
        principal: u64,
        request_id: [u8; 32],
    },
    CloseToDeposits {
        delegation_id: [u8; 32],
    },
    BeginWinding {
        delegation_id: [u8; 32],
    },
    InsurancePayout {
        delegation_id: [u8; 32],
        requested_amount_micro_usdc: u128,
    },
    Close {
        delegation_id: [u8; 32],
    },
    Terminate {
        delegation_id: [u8; 32],
        reason: BlockDelegationFaultReason,
        slash_policy: Option<BlockDelegationSlashPolicy>,
    },
    ForcedLiquidation {
        delegation_id: [u8; 32],
        market_id: u64,
        keeper_account: u64,
        requested_lots: Option<u64>,
    },
}

pub const DELEGATION_ACTION_KIND_OPEN: &str = "open";
pub const DELEGATION_ACTION_KIND_DEPOSIT: &str = "deposit";
pub const DELEGATION_ACTION_KIND_REQUEST_REDEMPTION: &str = "request_redemption";
pub const DELEGATION_ACTION_KIND_FINALIZE_REDEMPTION: &str = "finalize_redemption";
pub const DELEGATION_ACTION_KIND_CANCEL_REDEMPTION: &str = "cancel_redemption";
pub const DELEGATION_ACTION_KIND_CLOSE_TO_DEPOSITS: &str = "close_to_deposits";
pub const DELEGATION_ACTION_KIND_BEGIN_WINDING: &str = "begin_winding";
pub const DELEGATION_ACTION_KIND_INSURANCE_PAYOUT: &str = "insurance_payout";
pub const DELEGATION_ACTION_KIND_CLOSE: &str = "close";
pub const DELEGATION_ACTION_KIND_TERMINATE: &str = "terminate";
pub const DELEGATION_ACTION_KIND_FORCED_LIQUIDATION: &str = "forced_liquidation";

pub fn delegation_block_action_kind(action: &BlockDelegationAction) -> &'static str {
    match action {
        BlockDelegationAction::Open { .. } => DELEGATION_ACTION_KIND_OPEN,
        BlockDelegationAction::Deposit { .. } => DELEGATION_ACTION_KIND_DEPOSIT,
        BlockDelegationAction::RequestRedemption { .. } => {
            DELEGATION_ACTION_KIND_REQUEST_REDEMPTION
        }
        BlockDelegationAction::FinalizeRedemption { .. } => {
            DELEGATION_ACTION_KIND_FINALIZE_REDEMPTION
        }
        BlockDelegationAction::CancelRedemption { .. } => DELEGATION_ACTION_KIND_CANCEL_REDEMPTION,
        BlockDelegationAction::CloseToDeposits { .. } => DELEGATION_ACTION_KIND_CLOSE_TO_DEPOSITS,
        BlockDelegationAction::BeginWinding { .. } => DELEGATION_ACTION_KIND_BEGIN_WINDING,
        BlockDelegationAction::InsurancePayout { .. } => DELEGATION_ACTION_KIND_INSURANCE_PAYOUT,
        BlockDelegationAction::Close { .. } => DELEGATION_ACTION_KIND_CLOSE,
        BlockDelegationAction::Terminate { .. } => DELEGATION_ACTION_KIND_TERMINATE,
        BlockDelegationAction::ForcedLiquidation { .. } => {
            DELEGATION_ACTION_KIND_FORCED_LIQUIDATION
        }
    }
}

pub fn canonical_delegation_block_action_kind(kind: &str) -> Option<&'static str> {
    match kind {
        DELEGATION_ACTION_KIND_OPEN => Some(DELEGATION_ACTION_KIND_OPEN),
        DELEGATION_ACTION_KIND_DEPOSIT => Some(DELEGATION_ACTION_KIND_DEPOSIT),
        DELEGATION_ACTION_KIND_REQUEST_REDEMPTION => {
            Some(DELEGATION_ACTION_KIND_REQUEST_REDEMPTION)
        }
        DELEGATION_ACTION_KIND_FINALIZE_REDEMPTION => {
            Some(DELEGATION_ACTION_KIND_FINALIZE_REDEMPTION)
        }
        DELEGATION_ACTION_KIND_CANCEL_REDEMPTION => Some(DELEGATION_ACTION_KIND_CANCEL_REDEMPTION),
        DELEGATION_ACTION_KIND_CLOSE_TO_DEPOSITS => Some(DELEGATION_ACTION_KIND_CLOSE_TO_DEPOSITS),
        DELEGATION_ACTION_KIND_BEGIN_WINDING => Some(DELEGATION_ACTION_KIND_BEGIN_WINDING),
        DELEGATION_ACTION_KIND_INSURANCE_PAYOUT => Some(DELEGATION_ACTION_KIND_INSURANCE_PAYOUT),
        DELEGATION_ACTION_KIND_CLOSE => Some(DELEGATION_ACTION_KIND_CLOSE),
        DELEGATION_ACTION_KIND_TERMINATE => Some(DELEGATION_ACTION_KIND_TERMINATE),
        DELEGATION_ACTION_KIND_FORCED_LIQUIDATION => {
            Some(DELEGATION_ACTION_KIND_FORCED_LIQUIDATION)
        }
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuorumCertificate {
    pub view: u64,
    pub block_hash: Hash,
    pub signatures: Vec<Signature>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Signature {
    pub validator: ValidatorId,
    pub bytes: SignatureBytes,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ConsensusMessage {
    Proposal(ProposalMessage),
    Vote(VoteMessage),
    NewView(NewViewMessage),
    Timeout(TimeoutMessage),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProposalMessage {
    pub chain_id: u64,
    pub block: Block,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VoteMessage {
    pub chain_id: u64,
    pub height: u64,
    pub round: u64,
    pub block_hash: Hash,
    pub voter: ValidatorId,
    pub signature: SignatureBytes,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NewViewMessage {
    pub chain_id: u64,
    pub height: u64,
    pub round: u64,
    pub high_qc: QuorumCertificate,
    pub sender: ValidatorId,
    pub signature: SignatureBytes,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TimeoutMessage {
    pub chain_id: u64,
    pub height: u64,
    pub round: u64,
    pub sender: ValidatorId,
    pub high_qc: QuorumCertificate,
    pub signature: SignatureBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeoutCertificate {
    pub round: u64,
    pub highest_qc: QuorumCertificate,
    pub senders: Vec<ValidatorId>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PacemakerConfig {
    pub base_timeout_ms: u64,
    pub max_timeout_ms: u64,
    pub max_exponent: u32,
    pub max_clock_skew_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PacemakerState {
    pub current_round: u64,
    pub consecutive_failures: u32,
    pub highest_qc: QuorumCertificate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockValidationError {
    InvalidChainId,
    InvalidHeight,
    NonIncreasingRound,
    InvalidParentHash,
    InvalidParentStateRoot,
    InvalidQuorumCertificate,
    NonMonotonicTimestamp,
    TimestampOutsideLocalClockSkew,
    UnknownValidator,
    InvalidValidatorKeyMaterial,
    InvalidSignature,
    InsufficientQuorum,
    InvalidTimeoutCertificate,
    UnexpectedProposer,
    InvalidBlockBody,
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

pub fn block_hash(block: &Block) -> Hash {
    hash_bytes(&borsh::to_vec(&block.header).expect("header serialization must succeed"))
}

pub fn body_root(body: &BlockBody) -> Hash {
    hash_bytes(&borsh::to_vec(body).expect("body serialization must succeed"))
}

pub fn intent_id(intent: &SignedIntent) -> IntentId {
    hash_bytes(&borsh::to_vec(intent).expect("intent serialization must succeed"))
}

pub fn intent_signing_hash(intent: &Intent) -> Hash {
    hash_bytes(&borsh::to_vec(intent).expect("intent serialization must succeed"))
}

pub fn intent_not_before_ms(intent: &Intent) -> Option<u64> {
    match intent.predicate {
        Some(IntentPredicate::TimeAfter { not_before_ms }) => Some(not_before_ms),
        _ => None,
    }
}

pub fn intent_expires_at_ms(intent: &Intent) -> u64 {
    match intent.predicate {
        Some(IntentPredicate::TimeBefore { not_after_ms }) => min(intent.expires_at, not_after_ms),
        _ => intent.expires_at,
    }
}

pub fn intent_is_live_at(intent: &Intent, timestamp_ms: u64) -> bool {
    if let Some(not_before_ms) = intent_not_before_ms(intent) {
        if timestamp_ms < not_before_ms {
            return false;
        }
    }

    timestamp_ms < intent_expires_at_ms(intent)
}

pub fn validate_block_against_parent(
    block: &Block,
    parent: &BlockHeader,
    expected_proposer: ValidatorId,
    now_ms: u64,
    max_clock_skew_ms: u64,
) -> Result<(), BlockValidationError> {
    if block.header.chain_id != parent.chain_id {
        return Err(BlockValidationError::InvalidChainId);
    }

    if block.header.height != parent.height + 1 {
        return Err(BlockValidationError::InvalidHeight);
    }

    if block.header.round <= parent.round {
        return Err(BlockValidationError::NonIncreasingRound);
    }

    if block.header.parent_hash != hash_header(parent) {
        return Err(BlockValidationError::InvalidParentHash);
    }

    if block.header.parent_state_root != parent.state_root {
        return Err(BlockValidationError::InvalidParentStateRoot);
    }

    if block.header.timestamp_ms < parent.timestamp_ms {
        return Err(BlockValidationError::NonMonotonicTimestamp);
    }

    if block.header.timestamp_ms.abs_diff(now_ms) > max_clock_skew_ms {
        return Err(BlockValidationError::TimestampOutsideLocalClockSkew);
    }

    if block.header.proposer != expected_proposer {
        return Err(BlockValidationError::UnexpectedProposer);
    }

    if block.header.qc.view != parent.round {
        return Err(BlockValidationError::InvalidQuorumCertificate);
    }

    validate_quorum_certificate(&block.header.qc, &block.header.parent_hash)
}

pub fn validate_block_against_parent_and_membership(
    block: &Block,
    parent: &BlockHeader,
    validators: &[ValidatorVerifier],
    expected_proposer: ValidatorId,
    now_ms: u64,
    max_clock_skew_ms: u64,
) -> Result<(), BlockValidationError> {
    validate_block_against_parent(block, parent, expected_proposer, now_ms, max_clock_skew_ms)?;
    validate_quorum_certificate_with_membership(
        &block.header.qc,
        &block.header.parent_hash,
        validators,
    )
}

pub fn validate_quorum_certificate(
    qc: &QuorumCertificate,
    expected_block_hash: &Hash,
) -> Result<(), BlockValidationError> {
    if qc.block_hash != *expected_block_hash {
        return Err(BlockValidationError::InvalidQuorumCertificate);
    }

    let unique_validators: BTreeSet<_> = qc.signatures.iter().map(|sig| sig.validator).collect();
    if unique_validators.len() != qc.signatures.len() {
        return Err(BlockValidationError::InvalidQuorumCertificate);
    }

    Ok(())
}

pub fn validate_quorum_certificate_with_membership(
    qc: &QuorumCertificate,
    expected_block_hash: &Hash,
    validators: &[ValidatorVerifier],
) -> Result<(), BlockValidationError> {
    validate_quorum_certificate(qc, expected_block_hash)?;

    let membership: BTreeMap<_, _> = validators
        .iter()
        .map(|validator| (validator.validator, validator.consensus_pubkey))
        .collect();
    let threshold =
        quorum_threshold(validators.len()).ok_or(BlockValidationError::InsufficientQuorum)?;

    if qc.signatures.len() < threshold {
        return Err(BlockValidationError::InsufficientQuorum);
    }

    let signing_bytes = quorum_certificate_signing_bytes(qc)?;
    for signature in &qc.signatures {
        let consensus_pubkey = membership
            .get(&signature.validator)
            .ok_or(BlockValidationError::UnknownValidator)?;
        verify_consensus_signature(consensus_pubkey, &signing_bytes, &signature.bytes)?;
    }

    Ok(())
}

pub fn validate_vote_message(
    vote: &VoteMessage,
    validators: &[ValidatorVerifier],
) -> Result<(), BlockValidationError> {
    let consensus_pubkey = validators
        .iter()
        .find(|validator| validator.validator == vote.voter)
        .map(|validator| validator.consensus_pubkey)
        .ok_or(BlockValidationError::UnknownValidator)?;
    let signing_bytes = vote_message_signing_bytes(vote)?;
    verify_consensus_signature(&consensus_pubkey, &signing_bytes, &vote.signature)?;

    Ok(())
}

pub fn validate_new_view_message(
    new_view: &NewViewMessage,
    validators: &[ValidatorVerifier],
) -> Result<(), BlockValidationError> {
    let consensus_pubkey = validators
        .iter()
        .find(|validator| validator.validator == new_view.sender)
        .map(|validator| validator.consensus_pubkey)
        .ok_or(BlockValidationError::UnknownValidator)?;
    let signing_bytes = new_view_message_signing_bytes(new_view)?;
    verify_consensus_signature(&consensus_pubkey, &signing_bytes, &new_view.signature)?;

    validate_quorum_certificate_with_membership(
        &new_view.high_qc,
        &new_view.high_qc.block_hash,
        validators,
    )
}

pub fn validate_timeout_message(
    timeout: &TimeoutMessage,
    validators: &[ValidatorVerifier],
) -> Result<(), BlockValidationError> {
    let consensus_pubkey = validators
        .iter()
        .find(|validator| validator.validator == timeout.sender)
        .map(|validator| validator.consensus_pubkey)
        .ok_or(BlockValidationError::UnknownValidator)?;
    let signing_bytes = timeout_message_signing_bytes(timeout)?;
    verify_consensus_signature(&consensus_pubkey, &signing_bytes, &timeout.signature)?;

    validate_quorum_certificate_with_membership(
        &timeout.high_qc,
        &timeout.high_qc.block_hash,
        validators,
    )
}

pub fn build_timeout_certificate(
    round: u64,
    messages: &[TimeoutMessage],
    validators: &[ValidatorVerifier],
) -> Result<TimeoutCertificate, BlockValidationError> {
    let threshold =
        quorum_threshold(validators.len()).ok_or(BlockValidationError::InsufficientQuorum)?;
    if messages.len() < threshold {
        return Err(BlockValidationError::InsufficientQuorum);
    }

    let mut senders = BTreeSet::new();
    let mut highest_qc: Option<QuorumCertificate> = None;
    let first_message = messages
        .first()
        .ok_or(BlockValidationError::InvalidTimeoutCertificate)?;
    let expected_chain_id = first_message.chain_id;

    for message in messages {
        if message.round != round {
            return Err(BlockValidationError::InvalidTimeoutCertificate);
        }
        if message.chain_id != expected_chain_id {
            return Err(BlockValidationError::InvalidTimeoutCertificate);
        }

        validate_timeout_message(message, validators)?;

        if !senders.insert(message.sender) {
            return Err(BlockValidationError::InvalidTimeoutCertificate);
        }

        let should_replace = highest_qc
            .as_ref()
            .map(|current| message.high_qc.view > current.view)
            .unwrap_or(true);
        if should_replace {
            highest_qc = Some(message.high_qc.clone());
        }
    }

    Ok(TimeoutCertificate {
        round,
        highest_qc: highest_qc.ok_or(BlockValidationError::InvalidTimeoutCertificate)?,
        senders: senders.into_iter().collect(),
    })
}

pub fn quorum_threshold(validator_count: usize) -> Option<usize> {
    if validator_count == 0 {
        return None;
    }

    Some((validator_count * 2) / 3 + 1)
}

impl PacemakerConfig {
    pub fn timeout_ms(&self, consecutive_failures: u32) -> u64 {
        let exponent = consecutive_failures.min(self.max_exponent);
        let timeout = self
            .base_timeout_ms
            .saturating_mul(2_u64.saturating_pow(exponent));
        timeout.min(self.max_timeout_ms)
    }
}

impl PacemakerState {
    pub fn new(start_round: u64, highest_qc: QuorumCertificate) -> Self {
        Self {
            current_round: start_round,
            consecutive_failures: 0,
            highest_qc,
        }
    }

    pub fn round_timeout_ms(&self, config: &PacemakerConfig) -> u64 {
        config.timeout_ms(self.consecutive_failures)
    }

    pub fn advance_on_qc(&mut self, qc: QuorumCertificate) {
        let next_round = qc.view.saturating_add(1);
        if qc.view >= self.highest_qc.view {
            self.highest_qc = qc;
        }
        self.current_round = self.current_round.max(next_round);
        self.consecutive_failures = 0;
    }

    pub fn local_timeout(&self, chain_id: u64, height: u64, sender: ValidatorId) -> TimeoutMessage {
        TimeoutMessage {
            chain_id,
            height,
            round: self.current_round,
            sender,
            high_qc: self.highest_qc.clone(),
            signature: [0; 96],
        }
    }

    pub fn advance_on_timeout_certificate(&mut self, timeout_certificate: &TimeoutCertificate) {
        if timeout_certificate.highest_qc.view > self.highest_qc.view {
            self.highest_qc = timeout_certificate.highest_qc.clone();
        }

        self.current_round = self
            .current_round
            .max(timeout_certificate.round.saturating_add(1));
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }
}

fn hash_header(header: &BlockHeader) -> Hash {
    hash_bytes(&borsh::to_vec(header).expect("header serialization must succeed"))
}

pub fn quorum_certificate_signing_bytes(
    qc: &QuorumCertificate,
) -> Result<Vec<u8>, BlockValidationError> {
    borsh::to_vec(&(b"qc", qc.view, qc.block_hash))
        .map_err(|_| BlockValidationError::InvalidSignature)
}

fn vote_message_signing_bytes(vote: &VoteMessage) -> Result<Vec<u8>, BlockValidationError> {
    borsh::to_vec(&(
        b"vote",
        vote.chain_id,
        vote.height,
        vote.round,
        vote.block_hash,
        vote.voter,
    ))
    .map_err(|_| BlockValidationError::InvalidSignature)
}

fn new_view_message_signing_bytes(
    new_view: &NewViewMessage,
) -> Result<Vec<u8>, BlockValidationError> {
    borsh::to_vec(&(
        b"new_view",
        new_view.chain_id,
        new_view.height,
        new_view.round,
        new_view.high_qc.view,
        new_view.high_qc.block_hash,
        new_view.sender,
    ))
    .map_err(|_| BlockValidationError::InvalidSignature)
}

pub fn timeout_message_signing_bytes(
    timeout: &TimeoutMessage,
) -> Result<Vec<u8>, BlockValidationError> {
    borsh::to_vec(&(
        b"timeout",
        timeout.chain_id,
        timeout.height,
        timeout.round,
        timeout.high_qc.view,
        timeout.high_qc.block_hash,
        timeout.sender,
    ))
    .map_err(|_| BlockValidationError::InvalidSignature)
}

fn verify_consensus_signature(
    consensus_pubkey: &ConsensusPubKey,
    message: &[u8],
    signature: &SignatureBytes,
) -> Result<(), BlockValidationError> {
    if signature[64..].iter().any(|byte| *byte != 0) {
        return Err(BlockValidationError::InvalidSignature);
    }

    let verifying_key = VerifyingKey::from_bytes(consensus_pubkey)
        .map_err(|_| BlockValidationError::InvalidValidatorKeyMaterial)?;
    let mut ed25519_bytes = [0_u8; 64];
    ed25519_bytes.copy_from_slice(&signature[..64]);
    let signature = Ed25519Signature::from_bytes(&ed25519_bytes);
    verifying_key
        .verify_strict(message, &signature)
        .map_err(|_| BlockValidationError::InvalidSignature)
}

fn hash_bytes(bytes: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

#[cfg(test)]
#[path = "consensus_tests.rs"]
mod tests;
