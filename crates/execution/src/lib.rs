mod precompile_runtime;
pub use crate::precompile_runtime::verify_attestation_quote;
use crate::precompile_runtime::*;

use alloy_consensus::{
    transaction::SignerRecoverable, Transaction as AlloyTransaction, TxEnvelope,
};
use alloy_eips::Decodable2718;
use borsh::BorshDeserialize;
use kombat_chain_consensus::{
    block_hash, body_root, intent_expires_at_ms, intent_id, intent_not_before_ms,
    intent_signing_hash, Block, BlockBody, BlockHeader, BlockItem, Hash, Intent, IntentKind,
    IntentOrderKind, PolicyTermsWire, QuorumCertificate, SignedIntent, SignedTransaction,
    ValidatorId,
};
use kombat_chain_matching::{BookLevel, SingleMarketBook};
use kombat_chain_oracle::{price_status, OracleConfig, OraclePrice};
use kombat_chain_policy::{
    PolicyAdmission, PolicyError, PolicyStore, SessionPayload, SignedSessionPayload,
};
use kombat_chain_state_db::{CheckpointableStateStore, StateCheckpoint, StateError, StateView};
use revm::{
    context::{BlockEnv, CfgEnv, Context as RevmContext, Evm as Revm, FrameStack, Journal, TxEnv},
    context_interface::{ContextTr, Host, LocalContextTr},
    handler::{
        instructions::EthInstructions, EthPrecompiles, ExecuteCommitEvm, ExecuteEvm,
        PrecompileProvider,
    },
    interpreter::{CallInput, CallInputs, CallScheme, Gas, InstructionResult, InterpreterResult},
    primitives::{
        hardfork::SpecId, keccak256, Address, Bytes, Log, LogData, StorageKey, StorageValue,
        TxKind, B256, U256,
    },
    state::{Account, AccountInfo, Bytecode},
    Database, DatabaseCommit,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, str};

pub const CRATE_NAME: &str = "execution";
pub type PrecompileId = u16;
pub type ProtocolVersion = u16;
pub type Selector = [u8; 4];

const POLICY_STORE_STATE_KEY: &[u8] = b"protocol/policy-store";
const PENDING_INTENT_PREFIX: &str = "protocol/pending-intent/";
const ORDER_BOOK_PREFIX: &str = "protocol/order-book/";
const BOOK_SEQUENCE_PREFIX: &str = "protocol/book-sequence/";
const MARKET_LABEL_PREFIX: &str = "protocol/market-label/";
const ORACLE_MARK_PREFIX: &str = "protocol/oracle-mark/";
const ORACLE_CONFIG_PREFIX: &str = "protocol/oracle-config/";
const PAUSED_MARKET_PREFIX: &str = "protocol/paused-market/";
const ATTESTATION_VENDOR_ROOTS_PREFIX: &str = "protocol/attestation-vendor-roots/";
const EVM_ACCOUNT_NONCE_PREFIX: &str = "evm/account/nonce/";
const EVM_ACCOUNT_BALANCE_PREFIX: &str = "evm/account/balance/";
const EVM_ACCOUNT_CODE_PREFIX: &str = "evm/account/code/";
const EVM_STORAGE_PREFIX: &str = "evm/storage/";
const EVM_BLOCK_HASH_PREFIX: &str = "evm/block-hash/";
const EVM_USDC_BALANCE_PREFIX: &str = "evm/usdc/balance/";
const EVM_USDC_ALLOWANCE_PREFIX: &str = "evm/usdc/allowance/";
const EVM_USDC_TOTAL_SUPPLY_KEY: &str = "evm/usdc/total_supply";
const POLICY_USDC_BALANCE_PREFIX: &str = "policy/usdc/balance/";
const ACCOUNT_EVM_ADDRESS_PREFIX: &str = "account/evm-address/";
const EVM_ADDRESS_ACCOUNT_PREFIX: &str = "evm/address-account/";
const EVM_USDC_MINTER_ROLE_PREFIX: &str = "evm/usdc/role/minter/";
const EVM_USDC_SYSTEM_CODE: &[u8] = &[0xfe];

const SESSION_KEY_AUTHORIZE: Selector = [0, 0, 0, 1];
const SUBMIT_SIGNED_INTENT_SELECTOR: Selector = [0xc2, 0xac, 0x56, 0xf2];
const BEST_BID_ASK_SELECTOR: Selector = [0x95, 0x4a, 0x27, 0x26];
const DEPTH_SELECTOR: Selector = [0x47, 0x00, 0xa1, 0x6e];
const MARK_PRICE_SELECTOR: Selector = [0x92, 0x37, 0xe4, 0xce];
const ATTESTATION_VERIFY_SELECTOR: Selector = [0xe8, 0xe1, 0x5c, 0x5e];
const ERC165_SUPPORTS_INTERFACE_SELECTOR: Selector = [0x01, 0xff, 0xc9, 0xa7];
const ERC20_NAME_SELECTOR: Selector = [0x06, 0xfd, 0xde, 0x03];
const ERC20_SYMBOL_SELECTOR: Selector = [0x95, 0xd8, 0x9b, 0x41];
const ERC20_DECIMALS_SELECTOR: Selector = [0x31, 0x3c, 0xe5, 0x67];
const ERC20_TOTAL_SUPPLY_SELECTOR: Selector = [0x18, 0x16, 0x0d, 0xdd];
const ERC20_BALANCE_OF_SELECTOR: Selector = [0x70, 0xa0, 0x82, 0x31];
const ERC20_TRANSFER_SELECTOR: Selector = [0xa9, 0x05, 0x9c, 0xbb];
const ERC20_ALLOWANCE_SELECTOR: Selector = [0xdd, 0x62, 0xed, 0x3e];
const ERC20_APPROVE_SELECTOR: Selector = [0x09, 0x5e, 0xa7, 0xb3];
const ERC20_TRANSFER_FROM_SELECTOR: Selector = [0x23, 0xb8, 0x72, 0xdd];
const EVM_USDC_DEPOSIT_SELECTOR: Selector = [0x6a, 0xc6, 0x11, 0x85];
const EVM_USDC_WITHDRAW_SELECTOR: Selector = [0x1f, 0xdf, 0xd2, 0x51];
const EVM_USDC_DEPOSIT_FOR_SELECTOR: Selector = [0xd7, 0x41, 0x30, 0xe2];
const IERC165_INTERFACE_ID: Selector = [0x01, 0xff, 0xc9, 0xa7];
const IERC20_INTERFACE_ID: Selector = [0x36, 0x37, 0x2b, 0x07];
const IERC20_METADATA_INTERFACE_ID: Selector = [0xa2, 0x19, 0xa0, 0x25];

const UNKNOWN_SELECTOR_ERROR_SELECTOR: Selector = [0xc2, 0xa8, 0x25, 0xf5];
const MALFORMED_CALLDATA_ERROR_SELECTOR: Selector = [0x59, 0x17, 0x0b, 0xf0];
const MALFORMED_SIGNED_INTENT_ERROR_SELECTOR: Selector = [0x88, 0x18, 0x9f, 0xfd];
const MALFORMED_MARKET_ID_ERROR_SELECTOR: Selector = [0x5e, 0xf1, 0x00, 0xf2];
const STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR: Selector = [0x6f, 0x51, 0x5f, 0x79];
const DEPTH_LIMIT_OUT_OF_RANGE_ERROR_SELECTOR: Selector = [0xd6, 0xbb, 0xfe, 0xe2];
const UNKNOWN_MARKET_ERROR_SELECTOR: Selector = [0x64, 0xd7, 0xc3, 0xf1];
const EVM_USDC_INSUFFICIENT_BALANCE_ERROR_SELECTOR: Selector = [0xf4, 0xd6, 0x78, 0xb8];
const EVM_USDC_INSUFFICIENT_ALLOWANCE_ERROR_SELECTOR: Selector = [0x13, 0xbe, 0x25, 0x2b];
const EVM_USDC_UNAUTHORIZED_ERROR_SELECTOR: Selector = [0x82, 0xb4, 0x29, 0x00];
const EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR: Selector = [0xfb, 0x19, 0x25, 0xd9];
const EVM_USDC_UNSUPPORTED_CALL_SCHEME_ERROR_SELECTOR: Selector = [0x8f, 0xaf, 0x27, 0x8a];

const MAX_DEPTH_LEVELS: u16 = 32;
const MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP: usize = 32 * 1_024;
const MAX_TEE_QUOTE_BYTES_AMD_SEV_SNP: usize = 16 * 1_024;
const MAX_TEE_QUOTE_BYTES_AWS_NITRO: usize = 16 * 1_024;
const MAX_TEE_QUOTE_BYTES_APPLE_SECURE_ENCLAVE: usize = 8 * 1_024;
// RFC 051 launch freshness window. This mirrors the node-side action gate.
const ATTESTATION_QUOTE_FRESHNESS_BLOCKS: u64 = 1_000;
const EVM_BLOCKHASH_WINDOW_BLOCKS: u64 = 256;
const RETAINED_BLOCK_HASH_WINDOW_BLOCKS: u64 =
    if ATTESTATION_QUOTE_FRESHNESS_BLOCKS > EVM_BLOCKHASH_WINDOW_BLOCKS {
        ATTESTATION_QUOTE_FRESHNESS_BLOCKS
    } else {
        EVM_BLOCKHASH_WINDOW_BLOCKS
    };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvmPrecompileCall<'a> {
    pub protocol_version: ProtocolVersion,
    pub address: Address,
    pub caller: Address,
    pub calldata: &'a [u8],
    pub read_only: bool,
    pub scheme: CallScheme,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct OracleMarkRecord {
    pub price_ticks: u64,
    pub oracle_tick: u64,
    pub updated_at_block: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PendingIntentRecord {
    pub tracking_id: Hash,
    pub order_id: u64,
    pub signed_intent: SignedIntent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionStatus {
    Rejected = 0,
    Accepted = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionReason {
    None = 0,
    InvalidSignature = 1,
    InvalidNonce = 2,
    PolicyViolation = 3,
    MissingMandateForTier = 4,
    PolicyGasExhausted = 5,
    MarketPaused = 6,
    ConditionalPoolFull = 7,
    UnsupportedCustomPredicate = 8,
    UnsupportedIntentKind = 9,
    UnknownMarket = 10,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttestationVerifyFailure {
    None = 0,
    StaleNonce = 1,
    UnknownVendorRoot = 2,
    CertChainInvalid = 3,
    QuoteSignatureInvalid = 4,
    QuoteFormatVersionUnsupported = 5,
    ClaimMismatch = 6,
    QuoteTooLarge = 7,
    Revoked = 8,
}

impl AttestationVerifyFailure {
    /// Returns the RFC 051 identity-action rejection reason for failed verifier
    /// outcomes. Callers must check `verified == false`; `None` is the success
    /// discriminant's reason, not a substitute for checking the outcome.
    pub fn identity_action_rejection_reason(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::StaleNonce => Some("identity_tee_attestation_stale_nonce"),
            Self::UnknownVendorRoot => Some("identity_tee_attestation_unknown_vendor_root"),
            Self::CertChainInvalid => Some("identity_tee_attestation_cert_chain_invalid"),
            Self::QuoteSignatureInvalid => Some("identity_tee_attestation_quote_signature_invalid"),
            Self::QuoteFormatVersionUnsupported => {
                Some("identity_tee_attestation_quote_format_version_unsupported")
            }
            Self::ClaimMismatch => Some("identity_tee_attestation_claim_mismatch"),
            Self::QuoteTooLarge => Some("identity_tee_attestation_quote_too_large"),
            Self::Revoked => Some("identity_tee_attestation_revoked"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum AttestationVerifyVendor {
    IntelSgxDcap,
    AmdSevSnp,
    AwsNitro,
    AppleSecureEnclave,
}

impl AttestationVerifyVendor {
    pub fn from_abi(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::IntelSgxDcap),
            1 => Some(Self::AmdSevSnp),
            2 => Some(Self::AwsNitro),
            3 => Some(Self::AppleSecureEnclave),
            _ => None,
        }
    }

    pub fn as_abi(self) -> u8 {
        match self {
            Self::IntelSgxDcap => 0,
            Self::AmdSevSnp => 1,
            Self::AwsNitro => 2,
            Self::AppleSecureEnclave => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AttestationVendorRoot {
    pub root_id: Hash,
    pub valid_from_block: u64,
    pub valid_until_block: Option<u64>,
    /// Borsh-encoded RFC 051 VendorRootPayload. The verifier slice owns
    /// per-vendor decoding and validation of this payload.
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AttestationVendorRootsState {
    pub vendor: AttestationVerifyVendor,
    pub roots: Vec<AttestationVendorRoot>,
}

/// Borsh mirror of the node-side VendorRootPayload written into the
/// execution-state root mirror. Variants must be appended in lockstep with the
/// node enum so existing mirrored roots keep decoding.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, BorshDeserialize)]
enum AttestationVendorRootPayload {
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

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
enum AttestationVerifyClaim {
    RunsCodeHash {
        code_hash: Hash,
        model_hash: Option<Hash>,
    },
    BoundToMandate {
        mandate_id: Hash,
    },
    RunsCodeAndBoundToMandate {
        code_hash: Hash,
        model_hash: Option<Hash>,
        mandate_id: Hash,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttestationVerifyOutcome {
    pub verified: bool,
    pub code_hash: Hash,
    pub model_hash: Hash,
    pub enclave_id: Hash,
    pub signer: Hash,
    pub nonce: Hash,
    pub vendor_root_id: Hash,
    pub failure_reason: AttestationVerifyFailure,
}

/// Native RFC 051 attestation-verifier request. This is the same shape as the
/// EVM `attestationVerify(uint8,bytes,bytes32,uint64,bytes)` call after ABI
/// decoding, so node-side admission and EVM callers share one verifier path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttestationVerifyRequest<'a> {
    pub vendor: AttestationVerifyVendor,
    pub quote_bytes: &'a [u8],
    pub nonce: Hash,
    pub freshness_anchor_block: u64,
    pub expected_claim: &'a [u8],
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionContext {
    pub chain_id: u64,
    pub height: u64,
    pub timestamp_ms: u64,
    pub parent_state_root: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionResult {
    pub state_root: Hash,
    pub event_root: Hash,
    pub tx_root: Hash,
    pub gas_used: u64,
    pub evm_transactions: Vec<EvmTransactionExecution>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedEvmTransaction {
    pub tx_hash: Hash,
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub gas_limit: u64,
    pub value: U256,
    pub input: Vec<u8>,
    pub tx_type: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct EvmLogRecord {
    pub address: [u8; 20],
    pub topics: Vec<Hash>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct EvmTransactionExecution {
    pub tx_hash: Hash,
    pub from: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub nonce: u64,
    pub gas_used: u64,
    pub success: bool,
    pub logs: Vec<EvmLogRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedBlock<Checkpoint = ()> {
    pub block: Block,
    pub result: ExecutionResult,
    pub checkpoint: Checkpoint,
}

impl<Checkpoint> PreparedBlock<Checkpoint> {
    pub fn without_checkpoint(self) -> PreparedBlock {
        PreparedBlock {
            block: self.block,
            result: self.result,
            checkpoint: (),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrecompileDescriptor {
    pub id: PrecompileId,
    pub name: &'static str,
    pub address: Address,
    pub version: ProtocolVersion,
    pub abi_hash: Hash,
    pub mutability: PrecompileMutability,
    pub gas_model: GasModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrecompileMutability {
    Pure,
    View,
    Stateful,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GasModel {
    Fixed { base: u64, per_word: u64 },
    Metered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrecompileOutput {
    pub returned: Vec<u8>,
    pub gas_used: u64,
    pub reverted: bool,
    pub logs: Vec<Log>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrecompileCall<'a> {
    pub protocol_version: ProtocolVersion,
    pub address: Address,
    pub selector: Selector,
    pub input: &'a [u8],
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrecompileError {
    UnknownAddress(Address),
    UnknownSelector {
        address: Address,
        selector: Selector,
    },
    VersionMismatch {
        address: Address,
        requested: ProtocolVersion,
        supported: ProtocolVersion,
    },
    StatefulCallDisallowed(Address),
    InvalidInput(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantinedBlock {
    pub block_hash: Hash,
    pub height: u64,
    pub reason: QuarantineReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuarantineReason {
    AdmissionViolation(&'static str),
    ParentStateRootMismatch,
    StateRootMismatch,
    StorageCorruption,
    ExecutionRevert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockError {
    AdmissionViolation(&'static str),
    ExecutionRevert(String),
    ParentStateRootMismatch { expected: Hash, actual: Hash },
    StateRootMismatch { expected: Hash, actual: Hash },
    StorageCorruption,
}

pub trait ExecutionPipeline {
    fn execute_block(&mut self, block: &Block) -> Result<ExecutionResult, BlockError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevmTransactionEnvelope {
    pub block: BlockEnv,
    pub cfg: CfgEnv,
    pub tx: TxEnv,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RevmExecutionContext {
    execution: ExecutionContext,
    protocol_version: ProtocolVersion,
    force_precompile_read_only: bool,
}

#[derive(Clone, Debug)]
struct KombatPrecompiles {
    eth: EthPrecompiles,
}

#[derive(Debug)]
struct CheckpointDatabase<'a, Checkpoint> {
    checkpoint: &'a mut Checkpoint,
    code_cache: BTreeMap<B256, Bytecode>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub struct ExecutionEvent {
    pub topic: &'static str,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemExecution {
    pub event: ExecutionEvent,
    pub gas_used: u64,
    pub evm_transaction: Option<EvmTransactionExecution>,
}

#[derive(Clone, Debug, Default)]
pub struct RevmShell;

#[derive(Clone, Debug)]
pub struct BlockExecutor<Store> {
    chain_id: u64,
    state_store: Store,
    revm_shell: RevmShell,
    quarantined_blocks: BTreeMap<Hash, QuarantinedBlock>,
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

pub fn evm_usdc_system_address_bytes() -> [u8; 20] {
    evm_usdc_system_address().into_array()
}

pub fn evm_usdc_system_code_bytes() -> Vec<u8> {
    EVM_USDC_SYSTEM_CODE.to_vec()
}

pub fn decode_raw_evm_transaction(bytes: &[u8]) -> Result<DecodedEvmTransaction, String> {
    let mut raw = bytes;
    let envelope = TxEnvelope::decode_2718(&mut raw)
        .map_err(|error| format!("decode raw evm transaction: {error}"))?;
    if !raw.is_empty() {
        return Err("raw evm transaction contains trailing bytes".into());
    }
    let from = envelope
        .recover_signer()
        .map_err(|_| "raw evm transaction signature recovery failed".to_string())?;
    Ok(DecodedEvmTransaction {
        tx_hash: keccak256(bytes).0,
        from: from.into_array(),
        to: envelope.to().map(|address| address.into_array()),
        nonce: envelope.nonce(),
        gas_limit: envelope.gas_limit(),
        value: envelope.value(),
        input: envelope.input().to_vec(),
        tx_type: envelope.tx_type().into(),
    })
}

pub fn evm_account_nonce_state_key(address: [u8; 20]) -> Vec<u8> {
    evm_account_nonce_key(Address::from(address)).into_bytes()
}

pub fn evm_account_balance_state_key(address: [u8; 20]) -> Vec<u8> {
    evm_account_balance_key(Address::from(address)).into_bytes()
}

pub fn evm_account_code_state_key(address: [u8; 20]) -> Vec<u8> {
    evm_account_code_key(Address::from(address)).into_bytes()
}

pub fn policy_usdc_balance_state_key(address: [u8; 20]) -> Vec<u8> {
    policy_usdc_balance_key(Address::from(address)).into_bytes()
}

pub fn isolated_margin_market_custody_address(market_id: u64) -> [u8; 20] {
    let hash = keccak256(
        [
            b"isolated-margin-market-custody-v1".as_slice(),
            market_id.to_be_bytes().as_slice(),
        ]
        .concat(),
    );
    let mut address = [0_u8; 20];
    address.copy_from_slice(&hash[..20]);
    address
}

pub fn account_evm_address_state_key(account_id: u64) -> Vec<u8> {
    account_evm_address_key(account_id).into_bytes()
}

pub fn attestation_vendor_roots_state_key(vendor: AttestationVerifyVendor) -> Vec<u8> {
    format!("{ATTESTATION_VENDOR_ROOTS_PREFIX}{}", vendor.as_abi()).into_bytes()
}

pub fn evm_address_account_state_key(address: [u8; 20]) -> Vec<u8> {
    evm_address_account_key(Address::from(address)).into_bytes()
}

pub fn evm_usdc_balance_state_key(address: [u8; 20]) -> Vec<u8> {
    evm_usdc_balance_key(Address::from(address)).into_bytes()
}

pub fn evm_usdc_total_supply_state_key() -> Vec<u8> {
    EVM_USDC_TOTAL_SUPPLY_KEY.as_bytes().to_vec()
}

pub fn evm_usdc_minter_role_state_key(address: [u8; 20]) -> Vec<u8> {
    evm_usdc_minter_role_key(Address::from(address)).into_bytes()
}

pub fn protocol_precompiles() -> Vec<PrecompileDescriptor> {
    let mut precompiles = vec![
        PrecompileDescriptor {
            id: 1,
            name: "session_key",
            address: address_from_u64(0x1002),
            version: 1,
            abi_hash: abi_hash("session_key:v1:authorize(bytes)"),
            mutability: PrecompileMutability::Stateful,
            gas_model: GasModel::Fixed {
                base: 5_000,
                per_word: 65,
            },
        },
        PrecompileDescriptor {
            id: 2,
            name: "trading_intent_submit",
            address: address_from_u64(0x1003),
            version: 1,
            abi_hash: abi_hash("submitSignedIntent(bytes)"),
            mutability: PrecompileMutability::Stateful,
            gas_model: GasModel::Fixed {
                base: 12_000,
                per_word: 16,
            },
        },
        PrecompileDescriptor {
            id: 3,
            name: "clob_read",
            address: address_from_u64(0x1004),
            version: 1,
            abi_hash: abi_hash("bestBidAsk(bytes32)|depth(bytes32,uint8,uint16)"),
            mutability: PrecompileMutability::View,
            gas_model: GasModel::Metered,
        },
        PrecompileDescriptor {
            id: 4,
            name: "oracle_read",
            address: address_from_u64(0x1005),
            version: 1,
            abi_hash: abi_hash("markPrice(bytes32)"),
            mutability: PrecompileMutability::View,
            gas_model: GasModel::Fixed {
                base: 3_000,
                per_word: 0,
            },
        },
        PrecompileDescriptor {
            id: 5,
            name: "evm_usdc_system",
            address: evm_usdc_system_address(),
            version: 1,
            abi_hash: abi_hash(
                "name()|symbol()|decimals()|totalSupply()|balanceOf(address)|transfer(address,uint256)|allowance(address,address)|approve(address,uint256)|transferFrom(address,address,uint256)|supportsInterface(bytes4)|evmDeposit(uint256)|evmWithdraw(uint256)|evmDepositFor(address,uint256)",
            ),
            mutability: PrecompileMutability::View,
            gas_model: GasModel::Fixed {
                base: 9_000,
                per_word: 24,
            },
        },
        PrecompileDescriptor {
            id: 6,
            name: "attestation_verify",
            address: address_from_u64(0x1007),
            version: 1,
            abi_hash: abi_hash("attestationVerify(uint8,bytes,bytes32,uint64,bytes)"),
            // TODO(RFC 051 verifier): re-cost when cert-chain/signature verification
            // replaces the current root-state scaffold.
            mutability: PrecompileMutability::View,
            gas_model: GasModel::Fixed {
                base: 25_000,
                per_word: 64,
            },
        },
    ];
    precompiles.sort_by_key(|descriptor| descriptor.id);
    precompiles
}

pub fn precompile_by_address(
    address: Address,
    protocol_version: ProtocolVersion,
) -> Result<PrecompileDescriptor, PrecompileError> {
    let descriptor = protocol_precompiles()
        .into_iter()
        .find(|descriptor| descriptor.address == address)
        .ok_or(PrecompileError::UnknownAddress(address))?;

    if descriptor.version != protocol_version {
        return Err(PrecompileError::VersionMismatch {
            address,
            requested: protocol_version,
            supported: descriptor.version,
        });
    }

    Ok(descriptor)
}

impl RevmShell {
    pub fn transaction_env(
        &self,
        context: &ExecutionContext,
        index: usize,
        transaction: &SignedTransaction,
    ) -> RevmTransactionEnvelope {
        let block = BlockEnv {
            number: U256::from(context.height),
            timestamp: U256::from(context.timestamp_ms),
            beneficiary: validator_to_address(transaction.signer),
            gas_limit: 30_000_000_u64,
            ..Default::default()
        };

        let cfg = cfg_env_for(context.chain_id);

        let tx = TxEnv {
            caller: validator_to_address(transaction.signer),
            gas_limit: 21_000_u64.saturating_add(transaction.bytes.len() as u64),
            gas_price: 0_u128,
            kind: TxKind::Call(transaction_target(context, index)),
            value: U256::ZERO,
            data: Bytes::from(transaction.bytes.clone()),
            nonce: index as u64,
            chain_id: Some(context.chain_id),
            ..Default::default()
        };

        RevmTransactionEnvelope { block, cfg, tx }
    }

    fn decode_transaction_env(
        &self,
        context: &ExecutionContext,
        transaction: &SignedTransaction,
    ) -> Result<Option<RevmTransactionEnvelope>, BlockError> {
        let mut raw = transaction.bytes.as_slice();
        let decoded = match TxEnvelope::decode_2718(&mut raw) {
            Ok(decoded) => decoded,
            Err(_) => return Ok(None),
        };
        if !raw.is_empty() {
            return Err(BlockError::ExecutionRevert(
                "raw evm transaction contains trailing bytes".into(),
            ));
        }
        let tx = tx_env_from_envelope(&decoded, transaction.signer, context).map_err(|reason| {
            BlockError::ExecutionRevert(format!("raw evm transaction is unsupported: {reason}"))
        })?;
        Ok(Some(RevmTransactionEnvelope {
            block: block_env_for(context, transaction.signer),
            cfg: cfg_env_for(context.chain_id),
            tx,
        }))
    }

    fn contract_call_env(
        &self,
        context: &ExecutionContext,
        caller: Address,
        to: Address,
        data: Vec<u8>,
        gas_limit: u64,
    ) -> RevmTransactionEnvelope {
        RevmTransactionEnvelope {
            block: block_env_for(context, [0; 32]),
            cfg: cfg_env_for(context.chain_id),
            tx: TxEnv {
                caller,
                kind: TxKind::Call(to),
                gas_limit,
                gas_price: 0,
                value: U256::ZERO,
                data: Bytes::from(data),
                nonce: 0,
                chain_id: Some(context.chain_id),
                ..Default::default()
            },
        }
    }
}

impl<Store> BlockExecutor<Store> {
    pub fn new(chain_id: u64, state_store: Store) -> Self {
        Self {
            chain_id,
            state_store,
            revm_shell: RevmShell,
            quarantined_blocks: BTreeMap::new(),
        }
    }

    pub fn from_parts(
        chain_id: u64,
        state_store: Store,
        quarantined_blocks: BTreeMap<Hash, QuarantinedBlock>,
    ) -> Self {
        Self {
            chain_id,
            state_store,
            revm_shell: RevmShell,
            quarantined_blocks,
        }
    }

    pub fn state_store(&self) -> &Store {
        &self.state_store
    }

    pub fn quarantined_blocks(&self) -> &BTreeMap<Hash, QuarantinedBlock> {
        &self.quarantined_blocks
    }

    pub fn clear_quarantine(&mut self, block_hash: &Hash) -> Option<QuarantinedBlock> {
        self.quarantined_blocks.remove(block_hash)
    }
}

impl<Store> BlockExecutor<Store>
where
    Store: CheckpointableStateStore,
{
    pub fn committed_root(&self) -> Hash {
        self.state_store.committed_root()
    }

    pub fn apply_state_updates<I, D>(&mut self, sets: I, deletes: D) -> Result<Hash, BlockError>
    where
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
        D: IntoIterator<Item = Vec<u8>>,
    {
        let expected_root = self.state_store.committed_root();
        let mut checkpoint = self.prepare_checkpoint(expected_root)?;
        for (key, value) in sets {
            checkpoint.set(&key, &value);
        }
        for key in deletes {
            checkpoint.delete(&key);
        }
        self.state_store
            .commit(checkpoint)
            .map_err(map_state_error_to_block_error)
    }

    pub fn replay_block(&self, block: &Block) -> Result<ExecutionResult, BlockError> {
        let context = ExecutionContext {
            chain_id: self.chain_id,
            height: block.header.height,
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
        };
        validate_block_admission(block, &context)?;
        let checkpoint = self.prepare_checkpoint(block.header.parent_state_root)?;
        let (result, _) =
            self.execute_body(&context, block.header.parent_hash, &block.body, checkpoint)?;
        validate_header_roots(block, &result)?;
        Ok(result)
    }

    pub fn prepare_block(
        &self,
        parent: &Block,
        round: u64,
        proposer: ValidatorId,
        qc: QuorumCertificate,
        body: BlockBody,
        timestamp_ms: u64,
    ) -> Result<PreparedBlock, BlockError> {
        self.prepare_block_with_checkpoint(parent, round, proposer, qc, body, timestamp_ms)
            .map(PreparedBlock::without_checkpoint)
    }

    pub fn prepare_block_with_checkpoint(
        &self,
        parent: &Block,
        round: u64,
        proposer: ValidatorId,
        qc: QuorumCertificate,
        body: BlockBody,
        timestamp_ms: u64,
    ) -> Result<PreparedBlock<Store::Checkpoint>, BlockError> {
        let context = ExecutionContext {
            chain_id: self.chain_id,
            height: parent.header.height.saturating_add(1),
            timestamp_ms,
            parent_state_root: parent.header.state_root,
        };
        let parent_hash = block_hash(parent);
        let checkpoint = self.prepare_checkpoint(parent.header.state_root)?;
        let (result, checkpoint) = self.execute_body(&context, parent_hash, &body, checkpoint)?;
        let block = Block {
            header: BlockHeader {
                chain_id: self.chain_id,
                height: context.height,
                round,
                proposer,
                parent_hash,
                parent_state_root: context.parent_state_root,
                tx_root: result.tx_root,
                event_root: result.event_root,
                state_root: result.state_root,
                qc,
                timestamp_ms,
            },
            body,
        };

        Ok(PreparedBlock {
            block,
            result,
            checkpoint,
        })
    }

    pub fn verify_block(&mut self, block: &Block) -> Result<ExecutionResult, BlockError> {
        match self.replay_block(block) {
            Ok(result) => Ok(result),
            Err(error) => {
                self.quarantine_block(block, &error);
                Err(error)
            }
        }
    }

    pub fn commit_prepared_block(
        &mut self,
        prepared: PreparedBlock<Store::Checkpoint>,
    ) -> Result<ExecutionResult, BlockError> {
        validate_header_roots(&prepared.block, &prepared.result)?;
        self.state_store
            .commit(prepared.checkpoint)
            .map_err(|error| match error {
                StateError::RootMismatch { expected, actual } => {
                    BlockError::ParentStateRootMismatch { expected, actual }
                }
            })?;
        Ok(prepared.result)
    }

    pub fn execute_and_commit_body_from_committed_root(
        &mut self,
        height: u64,
        timestamp_ms: u64,
        parent_hash: Hash,
        body: &BlockBody,
    ) -> Result<ExecutionResult, BlockError> {
        let parent_state_root = self.state_store.committed_root();
        let context = ExecutionContext {
            chain_id: self.chain_id,
            height,
            timestamp_ms,
            parent_state_root,
        };
        let checkpoint = self.prepare_checkpoint(parent_state_root)?;
        let (result, checkpoint) = self.execute_body(&context, parent_hash, body, checkpoint)?;
        self.state_store
            .commit(checkpoint)
            .map_err(map_state_error_to_block_error)?;
        Ok(result)
    }

    fn prepare_checkpoint(
        &self,
        expected_parent_state_root: Hash,
    ) -> Result<<Store as CheckpointableStateStore>::Checkpoint, BlockError> {
        let checkpoint = self.state_store.checkpoint();
        if checkpoint.base_root() != expected_parent_state_root {
            return Err(BlockError::ParentStateRootMismatch {
                expected: expected_parent_state_root,
                actual: checkpoint.base_root(),
            });
        }
        Ok(checkpoint)
    }

    fn execute_body(
        &self,
        context: &ExecutionContext,
        parent_hash: Hash,
        body: &BlockBody,
        mut checkpoint: <Store as CheckpointableStateStore>::Checkpoint,
    ) -> Result<
        (
            ExecutionResult,
            <Store as CheckpointableStateStore>::Checkpoint,
        ),
        BlockError,
    > {
        let mut gas_used = 0_u64;
        let mut events = Vec::with_capacity(body.items.len());
        let mut evm_transactions = Vec::new();
        stage_parent_block_hash(context, parent_hash, &mut checkpoint);

        for (index, item) in body.items.iter().enumerate() {
            let item_execution =
                execute_item(&self.revm_shell, context, index, item, &mut checkpoint)?;
            gas_used = gas_used.saturating_add(item_execution.gas_used);
            events.push(item_execution.event);
            if let Some(transaction) = item_execution.evm_transaction {
                evm_transactions.push(transaction);
            }
        }

        let tx_root = body_root(body);
        let event_root = compute_event_root(&events);
        let state_root = checkpoint.compute_root();

        Ok((
            ExecutionResult {
                state_root,
                event_root,
                tx_root,
                gas_used,
                evm_transactions,
            },
            checkpoint,
        ))
    }

    pub fn dispatch_precompile(
        &self,
        context: &ExecutionContext,
        call: PrecompileCall<'_>,
        checkpoint: &mut <Store as CheckpointableStateStore>::Checkpoint,
    ) -> Result<PrecompileOutput, PrecompileError> {
        let descriptor = precompile_by_address(call.address, call.protocol_version)?;
        if call.read_only && descriptor.mutability == PrecompileMutability::Stateful {
            return Err(PrecompileError::StatefulCallDisallowed(call.address));
        }

        match descriptor.id {
            1 => dispatch_session_key(call.selector, call.input, &descriptor, checkpoint),
            2 => dispatch_trading_intent_submit(
                context,
                call.selector,
                call.input,
                &descriptor,
                checkpoint,
            ),
            3 => dispatch_clob_read(call.selector, call.input, &descriptor, checkpoint),
            4 => dispatch_oracle_read(call.selector, call.input, &descriptor, checkpoint),
            5 => dispatch_evm_usdc_native(call.selector, call.input, &descriptor, checkpoint),
            6 => dispatch_attestation_verify(
                context,
                call.selector,
                call.input,
                &descriptor,
                checkpoint,
            ),
            _ => Err(PrecompileError::UnknownAddress(call.address)),
        }
    }

    pub fn dispatch_evm_precompile(
        &self,
        context: &ExecutionContext,
        call: EvmPrecompileCall<'_>,
        checkpoint: &mut <Store as CheckpointableStateStore>::Checkpoint,
    ) -> Result<PrecompileOutput, PrecompileError> {
        dispatch_evm_precompile_with_checkpoint(context, call, checkpoint)
    }

    pub fn execute_contract_call(
        &self,
        context: &ExecutionContext,
        caller: Address,
        to: Address,
        data: Vec<u8>,
        gas_limit: u64,
    ) -> Result<PrecompileOutput, BlockError> {
        let checkpoint = self.prepare_checkpoint(context.parent_state_root)?;
        execute_contract_call_with_checkpoint(
            &self.revm_shell,
            context,
            caller,
            to,
            data,
            gas_limit,
            checkpoint,
            true,
        )
    }

    pub fn simulate_contract_transaction(
        &self,
        context: &ExecutionContext,
        caller: Address,
        to: Address,
        data: Vec<u8>,
        gas_limit: u64,
    ) -> Result<PrecompileOutput, BlockError> {
        let checkpoint = self.prepare_checkpoint(context.parent_state_root)?;
        execute_contract_call_with_checkpoint(
            &self.revm_shell,
            context,
            caller,
            to,
            data,
            gas_limit,
            checkpoint,
            false,
        )
    }
}

fn dispatch_evm_precompile_with_checkpoint<Checkpoint>(
    context: &ExecutionContext,
    call: EvmPrecompileCall<'_>,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    let descriptor = precompile_by_address(call.address, call.protocol_version)?;
    if call.read_only && descriptor.mutability == PrecompileMutability::Stateful {
        return Ok(reverted_output(
            fixed_gas_cost(&descriptor, call.calldata.len().saturating_sub(4), 0),
            encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
        ));
    }

    if call.calldata.len() < 4 {
        return Ok(reverted_output(
            fixed_gas_cost(&descriptor, 0, 0),
            encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
        ));
    }

    let selector = selector_from_slice(&call.calldata[..4]);
    let input = &call.calldata[4..];
    match descriptor.id {
        1 => dispatch_session_key(selector, input, &descriptor, checkpoint),
        2 => dispatch_trading_intent_submit_evm(context, selector, input, &descriptor, checkpoint),
        3 => dispatch_clob_read_evm(selector, input, &descriptor, checkpoint),
        4 => dispatch_oracle_read_evm(selector, input, &descriptor, checkpoint),
        5 => dispatch_evm_usdc_evm(call, selector, input, &descriptor, checkpoint),
        6 => dispatch_attestation_verify_evm(context, selector, input, &descriptor, checkpoint),
        _ => Err(PrecompileError::UnknownAddress(call.address)),
    }
}

impl<Store> ExecutionPipeline for BlockExecutor<Store>
where
    Store: CheckpointableStateStore,
{
    fn execute_block(&mut self, block: &Block) -> Result<ExecutionResult, BlockError> {
        let context = ExecutionContext {
            chain_id: self.chain_id,
            height: block.header.height,
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
        };
        validate_block_admission(block, &context)?;
        let checkpoint = self.prepare_checkpoint(block.header.parent_state_root)?;
        let (result, checkpoint) =
            self.execute_body(&context, block.header.parent_hash, &block.body, checkpoint)?;
        validate_header_roots(block, &result)?;
        if let Err(error) = self
            .state_store
            .commit(checkpoint)
            .map_err(map_state_error_to_block_error)
        {
            self.quarantine_block(block, &error);
            return Err(error);
        }
        Ok(result)
    }
}

fn validate_block_admission(block: &Block, context: &ExecutionContext) -> Result<(), BlockError> {
    if block.header.chain_id != context.chain_id {
        return Err(BlockError::AdmissionViolation("chain_id mismatch"));
    }

    if body_root(&block.body) != block.header.tx_root {
        return Err(BlockError::AdmissionViolation("tx_root mismatch"));
    }

    if block.header.height == 0 {
        return Err(BlockError::AdmissionViolation(
            "execution does not run on genesis",
        ));
    }

    Ok(())
}

fn validate_header_roots(block: &Block, result: &ExecutionResult) -> Result<(), BlockError> {
    if result.state_root != block.header.state_root {
        return Err(BlockError::StateRootMismatch {
            expected: block.header.state_root,
            actual: result.state_root,
        });
    }

    if result.event_root != block.header.event_root {
        return Err(BlockError::AdmissionViolation("event_root mismatch"));
    }

    if result.tx_root != block.header.tx_root {
        return Err(BlockError::AdmissionViolation("tx_root mismatch"));
    }

    Ok(())
}

fn execute_item<Checkpoint>(
    revm_shell: &RevmShell,
    context: &ExecutionContext,
    index: usize,
    item: &BlockItem,
    checkpoint: &mut Checkpoint,
) -> Result<ItemExecution, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    match item {
        BlockItem::Intent(intent) => execute_intent(context, index, intent, checkpoint),
        BlockItem::Transaction(transaction) => {
            execute_transaction(revm_shell, context, index, transaction, checkpoint)
        }
        BlockItem::OraclePrice(price) => execute_oracle_price_marker(price),
        BlockItem::Funding(funding) => execute_funding_marker(funding),
        BlockItem::Faucet(faucet) => execute_faucet_marker(faucet),
        BlockItem::Liquidation(liquidation) => execute_liquidation_marker(liquidation),
        BlockItem::Delegation(delegation) => execute_delegation_marker(delegation),
        BlockItem::MarketMakerProgram(program) => execute_market_maker_program_marker(program),
        BlockItem::TeeGovernance(governance) => execute_tee_governance_marker(governance),
        BlockItem::RealCapitalGovernance(governance) => {
            execute_real_capital_governance_marker(governance)
        }
    }
}

fn execute_intent<Checkpoint>(
    context: &ExecutionContext,
    _index: usize,
    intent: &SignedIntent,
    checkpoint: &mut Checkpoint,
) -> Result<ItemExecution, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    apply_intent_state_side_effect(&intent.intent, checkpoint)?;

    let intent_bytes = borsh::to_vec(&intent.intent)
        .map_err(|_| BlockError::ExecutionRevert("intent serialization failed".into()))?;
    let storage_key = format!(
        "intent/{}/{}/{}",
        context.height, intent.intent.policy_id, intent.intent.nonce
    );
    checkpoint.set(storage_key.as_bytes(), &intent_bytes);

    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "intent_applied",
            payload: borsh::to_vec(intent)
                .map_err(|_| BlockError::ExecutionRevert("intent serialization failed".into()))?,
        },
        gas_used: intent_gas_cost(&intent.intent.kind),
        evm_transaction: None,
    })
}

fn execute_oracle_price_marker(
    price: &kombat_chain_consensus::BlockOraclePrice,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "oracle_price_marker",
            payload: borsh::to_vec(price).map_err(|_| {
                BlockError::ExecutionRevert("oracle price serialization failed".into())
            })?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_funding_marker(
    funding: &kombat_chain_consensus::BlockFunding,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "funding_marker",
            payload: borsh::to_vec(funding)
                .map_err(|_| BlockError::ExecutionRevert("funding serialization failed".into()))?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_faucet_marker(
    faucet: &kombat_chain_consensus::BlockFaucet,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "faucet_marker",
            payload: borsh::to_vec(faucet)
                .map_err(|_| BlockError::ExecutionRevert("faucet serialization failed".into()))?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_liquidation_marker(
    liquidation: &kombat_chain_consensus::BlockLiquidation,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "liquidation_marker",
            payload: borsh::to_vec(liquidation).map_err(|_| {
                BlockError::ExecutionRevert("liquidation serialization failed".into())
            })?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_delegation_marker(
    delegation: &kombat_chain_consensus::BlockDelegation,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "delegation_marker",
            payload: borsh::to_vec(delegation).map_err(|_| {
                BlockError::ExecutionRevert("delegation serialization failed".into())
            })?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_market_maker_program_marker(
    program: &kombat_chain_consensus::BlockMarketMakerProgram,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "market_maker_program_marker",
            payload: borsh::to_vec(program).map_err(|_| {
                BlockError::ExecutionRevert("market maker program serialization failed".into())
            })?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_tee_governance_marker(
    governance: &kombat_chain_consensus::BlockTeeGovernance,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "tee_governance_marker",
            payload: borsh::to_vec(governance).map_err(|_| {
                BlockError::ExecutionRevert("tee governance serialization failed".into())
            })?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn execute_real_capital_governance_marker(
    governance: &kombat_chain_consensus::BlockRealCapitalGovernance,
) -> Result<ItemExecution, BlockError> {
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "real_capital_governance_marker",
            payload: borsh::to_vec(governance).map_err(|_| {
                BlockError::ExecutionRevert("real-capital governance serialization failed".into())
            })?,
        },
        gas_used: 0,
        evm_transaction: None,
    })
}

fn apply_intent_state_side_effect<Checkpoint>(
    intent: &Intent,
    checkpoint: &mut Checkpoint,
) -> Result<(), BlockError>
where
    Checkpoint: StateCheckpoint,
{
    match intent.kind {
        IntentKind::DepositCollateral {
            market_id,
            amount_usdc,
        } => apply_policy_usdc_collateral_delta(
            checkpoint,
            intent.account,
            market_id,
            U256::from(amount_usdc),
            PolicyUsdcCollateralDelta::Debit,
        ),
        IntentKind::WithdrawCollateral {
            market_id,
            amount_usdc,
        } => apply_policy_usdc_collateral_delta(
            checkpoint,
            intent.account,
            market_id,
            U256::from(amount_usdc),
            PolicyUsdcCollateralDelta::Credit,
        ),
        _ => Ok(()),
    }
}

#[derive(Clone, Copy)]
enum PolicyUsdcCollateralDelta {
    Debit,
    Credit,
}

fn apply_policy_usdc_collateral_delta<Checkpoint>(
    checkpoint: &mut Checkpoint,
    account_id: u64,
    market_id: u64,
    amount: U256,
    delta: PolicyUsdcCollateralDelta,
) -> Result<(), BlockError>
where
    Checkpoint: StateCheckpoint,
{
    if amount.is_zero() {
        return Ok(());
    }
    let account_address = read_account_evm_address(checkpoint, account_id)?;
    let market_custody_address = Address::from(isolated_margin_market_custody_address(market_id));
    let account_balance = read_policy_usdc_balance(checkpoint, account_address);
    let market_custody_balance = read_policy_usdc_balance(checkpoint, market_custody_address);
    let (next_account_balance, next_market_custody_balance) = match delta {
        PolicyUsdcCollateralDelta::Debit => (
            account_balance.checked_sub(amount).ok_or_else(|| {
                BlockError::ExecutionRevert("insufficient_policy_usdc_balance".into())
            })?,
            market_custody_balance.checked_add(amount).ok_or_else(|| {
                BlockError::ExecutionRevert("market_custody_balance_overflow".into())
            })?,
        ),
        PolicyUsdcCollateralDelta::Credit => (
            account_balance.checked_add(amount).ok_or_else(|| {
                BlockError::ExecutionRevert("policy_usdc_balance_overflow".into())
            })?,
            market_custody_balance.checked_sub(amount).ok_or_else(|| {
                BlockError::ExecutionRevert("insufficient_market_custody_balance".into())
            })?,
        ),
    };
    write_policy_usdc_balance(checkpoint, account_address, next_account_balance);
    write_policy_usdc_balance(
        checkpoint,
        market_custody_address,
        next_market_custody_balance,
    );
    Ok(())
}

fn read_account_evm_address<Checkpoint>(
    checkpoint: &mut Checkpoint,
    account_id: u64,
) -> Result<Address, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    let value = checkpoint
        .get(account_evm_address_key(account_id).as_bytes())
        .ok_or_else(|| BlockError::ExecutionRevert("account_evm_address_missing".into()))?;
    let bytes: [u8; 20] = value
        .as_slice()
        .try_into()
        .map_err(|_| BlockError::ExecutionRevert("account_evm_address_malformed".into()))?;
    Ok(Address::from(bytes))
}

fn execute_transaction<Checkpoint>(
    revm_shell: &RevmShell,
    context: &ExecutionContext,
    index: usize,
    transaction: &SignedTransaction,
    checkpoint: &mut Checkpoint,
) -> Result<ItemExecution, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    if let Some(envelope) = revm_shell.decode_transaction_env(context, transaction)? {
        let decoded =
            decode_raw_evm_transaction(&transaction.bytes).map_err(BlockError::ExecutionRevert)?;
        let storage_key = format!("tx/{}/{}", context.height, index);
        checkpoint.set(storage_key.as_bytes(), &transaction.bytes);
        return execute_decoded_evm_transaction(context, envelope, decoded, checkpoint);
    }

    let envelope = revm_shell.transaction_env(context, index, transaction);
    let storage_key = format!("tx/{}/{}", context.height, index);
    checkpoint.set(storage_key.as_bytes(), &transaction.bytes);

    let mut payload = Vec::new();
    payload.extend_from_slice(envelope.tx.caller.as_slice());
    payload.extend_from_slice(&(envelope.tx.gas_limit).to_be_bytes());
    payload.extend_from_slice(&(envelope.tx.data.len() as u64).to_be_bytes());

    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "transaction_executed",
            payload,
        },
        gas_used: envelope.tx.gas_limit,
        evm_transaction: None,
    })
}

fn execute_decoded_evm_transaction<Checkpoint>(
    context: &ExecutionContext,
    envelope: RevmTransactionEnvelope,
    decoded: DecodedEvmTransaction,
    checkpoint: &mut Checkpoint,
) -> Result<ItemExecution, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    let result = execute_revm_transaction_with_checkpoint(context, envelope, checkpoint, false)?;
    Ok(ItemExecution {
        event: ExecutionEvent {
            topic: "transaction_executed",
            payload: encode_transaction_result_payload(&result.result),
        },
        gas_used: result.gas_used,
        evm_transaction: Some(evm_transaction_execution_from_result(&decoded, &result)),
    })
}

#[allow(clippy::too_many_arguments)]
fn execute_contract_call_with_checkpoint<Checkpoint>(
    revm_shell: &RevmShell,
    context: &ExecutionContext,
    caller: Address,
    to: Address,
    data: Vec<u8>,
    gas_limit: u64,
    mut checkpoint: Checkpoint,
    read_only: bool,
) -> Result<PrecompileOutput, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    let envelope = revm_shell.contract_call_env(context, caller, to, data, gas_limit);
    let result =
        execute_revm_transaction_with_checkpoint(context, envelope, &mut checkpoint, read_only)?;
    Ok(PrecompileOutput {
        returned: execution_result_output_bytes(&result.result),
        gas_used: result.gas_used,
        reverted: !result.result.is_success(),
        logs: result.result.logs().to_vec(),
    })
}

struct RevmExecutionOutcome {
    result: revm::context_interface::result::ExecutionResult,
    gas_used: u64,
}

fn execute_revm_transaction_with_checkpoint<Checkpoint>(
    context: &ExecutionContext,
    envelope: RevmTransactionEnvelope,
    checkpoint: &mut Checkpoint,
    read_only: bool,
) -> Result<RevmExecutionOutcome, BlockError>
where
    Checkpoint: StateCheckpoint,
{
    let db = CheckpointDatabase::new(checkpoint);
    let mut ctx = RevmContext::new(db, SpecId::PRAGUE);
    ctx.block = envelope.block;
    ctx.cfg = envelope.cfg;
    ctx.chain = RevmExecutionContext {
        execution: context.clone(),
        protocol_version: 1,
        force_precompile_read_only: read_only,
    };
    let mut evm = Revm {
        ctx,
        inspector: (),
        instruction: EthInstructions::new_mainnet_with_spec(SpecId::PRAGUE),
        precompiles: KombatPrecompiles::new(SpecId::PRAGUE),
        frame_stack: FrameStack::new_prealloc(8),
    };
    let tx = envelope.tx;
    if read_only {
        let exec = evm.transact(tx).map_err(map_revm_error_to_block_error)?;
        return Ok(RevmExecutionOutcome {
            gas_used: exec.result.gas_used(),
            result: exec.result,
        });
    }

    let result = evm
        .transact_commit(tx)
        .map_err(map_revm_error_to_block_error)?;
    Ok(RevmExecutionOutcome {
        gas_used: result.gas_used(),
        result,
    })
}

fn execution_result_output_bytes(
    result: &revm::context_interface::result::ExecutionResult,
) -> Vec<u8> {
    match result {
        revm::context_interface::result::ExecutionResult::Success { output, .. } => match output {
            revm::context_interface::result::Output::Call(bytes) => bytes.to_vec(),
            revm::context_interface::result::Output::Create(bytes, _) => bytes.to_vec(),
        },
        revm::context_interface::result::ExecutionResult::Revert { output, .. } => output.to_vec(),
        revm::context_interface::result::ExecutionResult::Halt { .. } => Vec::new(),
    }
}

fn encode_transaction_result_payload(
    result: &revm::context_interface::result::ExecutionResult,
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u64::from(result.is_success()).to_be_bytes());
    payload.extend_from_slice(&result.gas_used().to_be_bytes());
    payload.extend_from_slice(&(execution_result_output_bytes(result).len() as u64).to_be_bytes());
    for log in result.logs() {
        payload.extend_from_slice(log.address.as_slice());
    }
    payload
}

fn evm_transaction_execution_from_result(
    decoded: &DecodedEvmTransaction,
    result: &RevmExecutionOutcome,
) -> EvmTransactionExecution {
    EvmTransactionExecution {
        tx_hash: decoded.tx_hash,
        from: decoded.from,
        to: decoded.to,
        nonce: decoded.nonce,
        gas_used: result.gas_used,
        success: result.result.is_success(),
        logs: result
            .result
            .logs()
            .iter()
            .map(|log| EvmLogRecord {
                address: log.address.into_array(),
                topics: log.data.topics().iter().map(|topic| topic.0).collect(),
                data: log.data.data.to_vec(),
            })
            .collect(),
    }
}

fn compute_event_root(events: &[ExecutionEvent]) -> Hash {
    let bytes = borsh::to_vec(events).expect("events must serialize");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn intent_gas_cost(kind: &IntentKind) -> u64 {
    match kind {
        IntentKind::Transfer { amount, .. } => 10_000_u64.saturating_add(*amount / 100),
        IntentKind::Cancel { .. } => 5_000,
        IntentKind::PlaceOrder {
            quantity_lots,
            kind,
            ..
        } => match kind {
            kombat_chain_consensus::IntentOrderKind::Limit { price_ticks } => 12_000_u64
                .saturating_add(
                    quantity_lots
                        .saturating_mul(*price_ticks)
                        .saturating_div(1_000)
                        .min(10_000),
                ),
            kombat_chain_consensus::IntentOrderKind::Market => {
                12_000_u64.saturating_add(quantity_lots.saturating_div(10).min(10_000))
            }
        },
        IntentKind::CancelOrder { .. } => 5_000,
        IntentKind::ReplaceOrder {
            quantity_lots,
            kind,
            ..
        } => match kind {
            kombat_chain_consensus::IntentOrderKind::Limit { price_ticks } => 12_000_u64
                .saturating_add(
                    quantity_lots
                        .saturating_mul(*price_ticks)
                        .saturating_div(1_000)
                        .min(10_000),
                ),
            kombat_chain_consensus::IntentOrderKind::Market => {
                12_000_u64.saturating_add(quantity_lots.saturating_div(10).min(10_000))
            }
        },
        IntentKind::CreateSubPolicy {
            child_terms: PolicyTermsWire {
                gas_budget_per_day, ..
            },
            ..
        } => 15_000_u64.saturating_add((*gas_budget_per_day / 10).min(10_000) as u64),
        IntentKind::RevokePolicy { .. } => 5_000,
        IntentKind::DepositCollateral { amount_usdc, .. }
        | IntentKind::WithdrawCollateral { amount_usdc, .. } => {
            10_000_u64.saturating_add(amount_usdc.saturating_div(100).min(10_000))
        }
    }
}

fn validator_to_address(validator_id: [u8; 32]) -> Address {
    Address::from_slice(&validator_id[12..])
}

fn block_env_for(context: &ExecutionContext, beneficiary: [u8; 32]) -> BlockEnv {
    BlockEnv {
        number: U256::from(context.height),
        timestamp: U256::from(context.timestamp_ms),
        beneficiary: validator_to_address(beneficiary),
        gas_limit: 30_000_000_u64,
        ..Default::default()
    }
}

#[allow(clippy::field_reassign_with_default)]
fn cfg_env_for(chain_id: u64) -> CfgEnv {
    let mut cfg = CfgEnv::new_with_spec(SpecId::PRAGUE);
    cfg.chain_id = chain_id;
    cfg.set_spec_and_mainnet_gas_params(SpecId::PRAGUE);
    cfg
}

fn tx_env_from_envelope(
    envelope: &TxEnvelope,
    fallback_signer: ValidatorId,
    context: &ExecutionContext,
) -> Result<TxEnv, &'static str> {
    match envelope {
        TxEnvelope::Eip4844(_) => Err("eip4844 is not supported yet"),
        _ => {
            let caller = envelope
                .recover_signer()
                .unwrap_or_else(|_| validator_to_address(fallback_signer));
            let kind = match envelope.to() {
                Some(address) => TxKind::Call(address),
                None => TxKind::Create,
            };
            let access_list = envelope.access_list().cloned().unwrap_or_default();
            // EIP-7702: preserve the signed authorization list so revm can apply
            // the delegations during transaction execution. For every other tx
            // type this list is either unset or empty on the envelope, so we
            // forward whatever alloy exposes rather than branching by type.
            let authorization_list = envelope
                .authorization_list()
                .map(|list| {
                    list.iter()
                        .cloned()
                        .map(revm::context_interface::either::Either::Left)
                        .collect()
                })
                .unwrap_or_default();
            Ok(TxEnv {
                tx_type: envelope.tx_type().into(),
                caller,
                gas_limit: envelope.gas_limit(),
                gas_price: envelope.max_fee_per_gas(),
                kind,
                value: envelope.value(),
                data: envelope.input().clone(),
                nonce: envelope.nonce(),
                chain_id: envelope.chain_id().or(Some(context.chain_id)),
                access_list,
                gas_priority_fee: envelope.max_priority_fee_per_gas(),
                blob_hashes: envelope.blob_versioned_hashes().unwrap_or(&[]).to_vec(),
                max_fee_per_blob_gas: envelope.max_fee_per_blob_gas().unwrap_or(0),
                authorization_list,
            })
        }
    }
}

fn transaction_target(context: &ExecutionContext, index: usize) -> Address {
    let mut target = [0_u8; 20];
    target[0..8].copy_from_slice(&context.height.to_be_bytes()[..]);
    target[12..20].copy_from_slice(&(index as u64).to_be_bytes());
    Address::from(target)
}

fn map_state_error_to_block_error(error: StateError) -> BlockError {
    match error {
        StateError::RootMismatch { .. } => BlockError::StorageCorruption,
    }
}

impl KombatPrecompiles {
    fn new(spec: SpecId) -> Self {
        Self {
            eth: EthPrecompiles::new(spec),
        }
    }
}

impl<'a, Checkpoint> CheckpointDatabase<'a, Checkpoint>
where
    Checkpoint: StateCheckpoint,
{
    fn new(checkpoint: &'a mut Checkpoint) -> Self {
        Self {
            checkpoint,
            code_cache: BTreeMap::new(),
        }
    }

    fn checkpoint_mut(&mut self) -> &mut Checkpoint {
        self.checkpoint
    }

    fn load_account_info(&mut self, address: Address) -> Option<AccountInfo> {
        if address == evm_usdc_system_address() {
            let balance = self
                .checkpoint
                .get(evm_account_balance_key(address).as_bytes())
                .map(|bytes| U256::from_be_slice(&bytes))
                .unwrap_or(U256::ZERO);
            let code = Bytecode::new_raw(Bytes::from_static(EVM_USDC_SYSTEM_CODE));
            let code_hash = code.hash_slow();
            self.code_cache.insert(code_hash, code.clone());
            return Some(AccountInfo::new(balance, 0, code_hash, code));
        }

        let nonce = self
            .checkpoint
            .get(evm_account_nonce_key(address).as_bytes())
            .map(|bytes| decode_u64_word(&bytes).expect("stored nonce should decode"))
            .unwrap_or(0);
        let balance = self
            .checkpoint
            .get(evm_account_balance_key(address).as_bytes())
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO);
        let code_bytes = self
            .checkpoint
            .get(evm_account_code_key(address).as_bytes())
            .unwrap_or_default();
        if nonce == 0 && balance.is_zero() && code_bytes.is_empty() {
            return None;
        }
        let code = if code_bytes.is_empty() {
            Bytecode::default()
        } else {
            Bytecode::new_raw(Bytes::from(code_bytes))
        };
        let code_hash = code.hash_slow();
        self.code_cache.insert(code_hash, code.clone());
        Some(AccountInfo::new(balance, nonce, code_hash, code))
    }

    fn load_storage_value(&self, address: Address, index: StorageKey) -> StorageValue {
        self.checkpoint
            .get(evm_storage_key(address, index).as_bytes())
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(StorageValue::ZERO)
    }
}

impl<'a, Checkpoint> Database for CheckpointDatabase<'a, Checkpoint>
where
    Checkpoint: StateCheckpoint,
{
    type Error = std::convert::Infallible;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        Ok(self.load_account_info(address))
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        Ok(self.code_cache.get(&code_hash).cloned().unwrap_or_default())
    }

    fn storage(
        &mut self,
        address: Address,
        index: StorageKey,
    ) -> Result<StorageValue, Self::Error> {
        Ok(self.load_storage_value(address, index))
    }

    fn block_hash(&mut self, number: u64) -> Result<B256, Self::Error> {
        Ok(self
            .checkpoint
            .get(evm_block_hash_key(number).as_bytes())
            .map(|bytes| {
                let mut hash = [0_u8; 32];
                hash.copy_from_slice(&bytes[..32]);
                hash.into()
            })
            .unwrap_or_default())
    }
}

impl<'a, Checkpoint> DatabaseCommit for CheckpointDatabase<'a, Checkpoint>
where
    Checkpoint: StateCheckpoint,
{
    fn commit(&mut self, changes: revm::primitives::AddressMap<Account>) {
        for (address, account) in changes {
            if account.is_selfdestructed() || account.info.is_empty() {
                self.checkpoint
                    .delete(evm_account_nonce_key(address).as_bytes());
                self.checkpoint
                    .delete(evm_account_balance_key(address).as_bytes());
                self.checkpoint
                    .delete(evm_account_code_key(address).as_bytes());
                for (slot, _) in account.storage {
                    self.checkpoint
                        .delete(evm_storage_key(address, slot).as_bytes());
                }
                continue;
            }

            self.checkpoint.set(
                evm_account_nonce_key(address).as_bytes(),
                &account.info.nonce.to_be_bytes(),
            );
            self.checkpoint.set(
                evm_account_balance_key(address).as_bytes(),
                &account.info.balance.to_be_bytes::<32>(),
            );
            let code_bytes = account
                .info
                .code
                .clone()
                .unwrap_or_default()
                .original_bytes()
                .to_vec();
            if code_bytes.is_empty() {
                self.checkpoint
                    .delete(evm_account_code_key(address).as_bytes());
            } else {
                self.checkpoint
                    .set(evm_account_code_key(address).as_bytes(), &code_bytes);
            }

            for (slot, value) in account.storage {
                if !value.is_changed() {
                    continue;
                }
                if value.present_value.is_zero() {
                    self.checkpoint
                        .delete(evm_storage_key(address, slot).as_bytes());
                } else {
                    self.checkpoint.set(
                        evm_storage_key(address, slot).as_bytes(),
                        &value.present_value.to_be_bytes::<32>(),
                    );
                }
            }
        }
    }
}

impl<'a, Checkpoint>
    PrecompileProvider<
        RevmContext<
            BlockEnv,
            TxEnv,
            CfgEnv,
            CheckpointDatabase<'a, Checkpoint>,
            Journal<CheckpointDatabase<'a, Checkpoint>>,
            RevmExecutionContext,
        >,
    > for KombatPrecompiles
where
    Checkpoint: StateCheckpoint,
{
    type Output = InterpreterResult;

    fn set_spec(&mut self, spec: SpecId) -> bool {
        <EthPrecompiles as PrecompileProvider<
            RevmContext<
                BlockEnv,
                TxEnv,
                CfgEnv,
                CheckpointDatabase<'a, Checkpoint>,
                Journal<CheckpointDatabase<'a, Checkpoint>>,
                RevmExecutionContext,
            >,
        >>::set_spec(&mut self.eth, spec)
    }

    fn run(
        &mut self,
        context: &mut RevmContext<
            BlockEnv,
            TxEnv,
            CfgEnv,
            CheckpointDatabase<'a, Checkpoint>,
            Journal<CheckpointDatabase<'a, Checkpoint>>,
            RevmExecutionContext,
        >,
        inputs: &CallInputs,
    ) -> Result<Option<Self::Output>, String> {
        let chain = context.chain().clone();
        if precompile_by_address(inputs.bytecode_address, chain.protocol_version).is_ok() {
            let input_bytes = match &inputs.input {
                CallInput::SharedBuffer(range) => {
                    if let Some(slice) = context.local().shared_memory_buffer_slice(range.clone()) {
                        slice.to_vec()
                    } else {
                        Vec::new()
                    }
                }
                CallInput::Bytes(bytes) => bytes.to_vec(),
            };
            let output = dispatch_evm_precompile_with_checkpoint(
                &chain.execution,
                EvmPrecompileCall {
                    protocol_version: chain.protocol_version,
                    address: inputs.bytecode_address,
                    caller: inputs.caller,
                    calldata: &input_bytes,
                    read_only: chain.force_precompile_read_only || inputs.is_static,
                    scheme: inputs.scheme,
                },
                context.db_mut().checkpoint_mut(),
            )
            .map_err(|error| format!("protocol precompile dispatch failed: {error:?}"))?;

            if !output.reverted {
                for log in output.logs.iter().cloned() {
                    context.log(log);
                }
            }

            return Ok(Some(InterpreterResult {
                result: if output.reverted {
                    InstructionResult::Revert
                } else {
                    InstructionResult::Return
                },
                gas: {
                    let mut gas = Gas::new(inputs.gas_limit);
                    let underflow = gas.record_cost(output.gas_used);
                    assert!(underflow, "Gas underflow is not possible");
                    gas
                },
                output: Bytes::from(output.returned),
            }));
        }

        self.eth.run(context, inputs)
    }

    fn warm_addresses(&self) -> Box<impl Iterator<Item = Address>> {
        let mut addresses: Vec<Address> = self.eth.warm_addresses().collect();
        addresses.extend(
            protocol_precompiles()
                .into_iter()
                .map(|descriptor| descriptor.address),
        );
        Box::new(addresses.into_iter())
    }

    fn contains(&self, address: &Address) -> bool {
        self.eth.contains(address) || precompile_by_address(*address, 1).is_ok()
    }
}

fn map_revm_error_to_block_error<DbError: std::fmt::Debug>(
    error: revm::context_interface::result::EVMError<DbError>,
) -> BlockError {
    BlockError::ExecutionRevert(format!("{error:?}"))
}

impl<Store> BlockExecutor<Store> {
    fn quarantine_block(&mut self, block: &Block, error: &BlockError) {
        let reason = match error {
            BlockError::AdmissionViolation(reason) => QuarantineReason::AdmissionViolation(reason),
            BlockError::ExecutionRevert(_) => QuarantineReason::ExecutionRevert,
            BlockError::ParentStateRootMismatch { .. } => QuarantineReason::ParentStateRootMismatch,
            BlockError::StateRootMismatch { .. } => QuarantineReason::StateRootMismatch,
            BlockError::StorageCorruption => QuarantineReason::StorageCorruption,
        };

        self.quarantined_blocks.insert(
            block_hash(block),
            QuarantinedBlock {
                block_hash: block_hash(block),
                height: block.header.height,
                reason,
            },
        );
    }
}

#[cfg(test)]
mod tests;
