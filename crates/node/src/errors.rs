use crate::{MarketMakerProgramId, CRATE_NAME};
use kombat_chain_consensus::{
    BlockValidationError, IntentId, RealCapitalGovernanceActionId, TeeGovernanceActionId,
    ValidatorId,
};
use kombat_chain_execution::BlockError;
use kombat_chain_matching::OrderRejectReason;
use kombat_chain_network::ConfigError;
use kombat_chain_policy::{AccountError, PolicyError, Principal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeError {
    InvalidBlock(BlockValidationError),
    LocalValidatorUnavailable,
    InvalidConsensusPubKey {
        validator_id: ValidatorId,
    },
    MissingConsensusSigningKey {
        validator_id: ValidatorId,
    },
    InvalidClusterSize {
        expected: usize,
        actual: usize,
    },
    BootstrapConfig(ConfigError),
    MissingLeader {
        round: u64,
    },
    ZeroStake,
    Persistence {
        reason: String,
    },
    Execution(BlockError),
    Order(OrderRejectReason),
    Account(AccountError),
    Policy(PolicyError),
    PositionFlat {
        market_id: u64,
        account_id: u64,
    },
    InvalidCollateralMovement {
        market_id: u64,
        account_id: u64,
        reason: &'static str,
    },
    NotUnderMaintenance {
        market_id: u64,
        account_id: u64,
    },
    StaleMarketData {
        market_id: u64,
    },
    UnknownMarket {
        market_id: u64,
    },
    MarketAlreadyListed {
        market_id: u64,
    },
    InvalidMarketConfig {
        market_id: u64,
        reason: &'static str,
    },
    InvalidOracleSubmission {
        market_id: u64,
        reason: &'static str,
    },
    OracleAggregationFailed {
        market_id: u64,
        reason: &'static str,
    },
    InvalidDelegation {
        agent: u64,
        reason: &'static str,
    },
    InvalidMarketMakerProgram {
        program_id: MarketMakerProgramId,
        reason: &'static str,
    },
    InvalidTeeGovernance {
        action_id: TeeGovernanceActionId,
        reason: &'static str,
    },
    InvalidRealCapitalGovernance {
        action_id: RealCapitalGovernanceActionId,
        reason: &'static str,
    },
    InvalidMandate {
        agent: u64,
        reason: &'static str,
    },
    InvalidIdentity {
        account: u64,
        reason: &'static str,
    },
    InvalidIdentityAction {
        account: u64,
        reason: &'static str,
    },
    InvalidIdentityActionNonce {
        account: u64,
        expected: u64,
        actual: u64,
    },
    InvalidIdentityActionSignature {
        account: u64,
    },
    InvalidMandateAction {
        owner: Principal,
        reason: &'static str,
    },
    InvalidMandateActionNonce {
        owner: Principal,
        expected: u64,
        actual: u64,
    },
    InvalidMandateActionSignature {
        owner: Principal,
    },
    MarketTradingPaused {
        market_id: u64,
    },
    AccountFrozen {
        account_id: u64,
    },
    DuplicateIntent(IntentId),
    DuplicatePolicyNonce {
        policy_id: u64,
        nonce: u64,
    },
    StaleIntentNonce {
        policy_id: u64,
        expected: u64,
        actual: u64,
    },
    IntentExpired {
        intent_id: IntentId,
        expires_at: u64,
        timestamp_ms: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifyIntentError {
    Policy(PolicyError),
}

impl From<VerifyIntentError> for NodeError {
    fn from(error: VerifyIntentError) -> Self {
        match error {
            VerifyIntentError::Policy(error) => Self::Policy(error),
        }
    }
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}
