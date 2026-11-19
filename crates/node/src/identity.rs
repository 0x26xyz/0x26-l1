use crate::MandateId;
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    BlockRealCapitalGovernanceAction, BlockTeeGovernanceAction, BlockTeeVendor,
    BlockVendorRootPayload, Hash, RealCapitalGovernanceActionId, TeeGovernanceActionId,
};
use kombat_chain_execution::{
    AttestationVendorRoot, AttestationVendorRootsState, AttestationVerifyOutcome,
    AttestationVerifyVendor,
};
use kombat_chain_streams::{
    IdentityAgentKind, IdentityAttestationClaim, IdentityAttestationKind,
    IdentityAttestationRevocationReason, IdentityAttestationVendor, IdentityTierStatus,
    IdentityTierTransitionReason,
};
use revm::primitives::keccak256;
use std::collections::{BTreeMap, BTreeSet};

// RFC 050 AgentIdentityRecord bounded-string caps.
pub const MAX_AGENT_IDENTITY_DISPLAY_NAME_BYTES: usize = 64;
pub const MAX_AGENT_IDENTITY_DESCRIPTION_BYTES: usize = 512;
pub const MAX_AGENT_IDENTITY_CONTACT_URI_BYTES: usize = 256;
// RFC 050 OperatorSelf attestation lifecycle window: actions may choose
// any expiry from 1 hour through 30 days after the action timestamp.
pub const OPERATOR_ATTESTATION_MIN_EXPIRY_MS: u64 = 3_600 * 1_000;
pub const OPERATOR_ATTESTATION_MAX_EXPIRY_MS: u64 = 30 * 86_400 * 1_000;
// RFC 051 TEE attestation launch window mirrors OperatorSelf attestations.
pub const TEE_ATTESTATION_MIN_EXPIRY_MS: u64 = OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
pub const TEE_ATTESTATION_MAX_EXPIRY_MS: u64 = OPERATOR_ATTESTATION_MAX_EXPIRY_MS;
pub const ATTESTATION_QUOTE_FRESHNESS_BLOCKS: u64 = 1_000;
pub const MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP: usize = 32 * 1_024;
pub const MAX_TEE_QUOTE_BYTES_AMD_SEV_SNP: usize = 16 * 1_024;
pub const MAX_TEE_QUOTE_BYTES_AWS_NITRO: usize = 16 * 1_024;
pub const MAX_TEE_QUOTE_BYTES_APPLE_SECURE_ENCLAVE: usize = 8 * 1_024;
pub const TEE_CAP_MULTIPLIER_SCALE_UNITS: u128 = 1_000_000;
pub const TEE_CAP_MULTIPLIER_BASELINE_UNITS: u128 = TEE_CAP_MULTIPLIER_SCALE_UNITS;
pub const TEE_CAP_MULTIPLIER_APPLE_SECURE_ENCLAVE_UNITS: u128 = 400_000;
pub const TEE_GOVERNANCE_NOTICE_BLOCKS: u64 = 172_800;
pub const VENDOR_ROOT_NOTICE_BLOCKS: u64 = TEE_GOVERNANCE_NOTICE_BLOCKS;
pub const TEE_GOVERNANCE_ACTION_RECEIPT_RETENTION_MS: u64 = 30 * 86_400 * 1_000;
pub const MAX_PENDING_TEE_GOVERNANCE_ACTIONS: usize = 1_024;
pub const REAL_CAPITAL_GOVERNANCE_ACTION_RECEIPT_RETENTION_MS: u64 = 30 * 86_400 * 1_000;
pub const MAX_PENDING_REAL_CAPITAL_GOVERNANCE_ACTIONS: usize = 1_024;
pub const REAL_CAPITAL_GOVERNANCE_SUBMISSION_TIMESTAMP_WINDOW_MS: u64 = 5 * 60 * 1_000;
const MAX_TEE_VENDOR_ROOT_CERT_DER_BYTES: usize = 8_192;
const MAX_TEE_VENDOR_TCB_INFO_URL_BYTES: usize = 256;
const MAX_TEE_VENDOR_CRL_DER_BYTES: usize = 32_768;

// RFC 050 — Agent identity tiers.

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AgentIdentityTier {
    Tier0Unattested,
    Tier1OperatorAttested,
    Tier2TeeAttested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AgentKind {
    Unknown,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Unknown => "unknown",
        }
    }
}

// RFC 051 — TEE vendor root and cap-profile consensus state.

pub type VendorRootId = [u8; 32];
pub(crate) type VendorRootReverseIndexKey = (TeeVendor, VendorRootId);
pub(crate) type VendorRootReverseIndexValue = BTreeSet<(u64, AttestationRecordId)>;
pub type GovernanceActionId = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum TeeVendor {
    IntelSgxDcap,
    AmdSevSnp,
    AwsNitro,
    AppleSecureEnclave,
}

impl TeeVendor {
    pub fn all() -> [Self; 4] {
        [
            Self::IntelSgxDcap,
            Self::AmdSevSnp,
            Self::AwsNitro,
            Self::AppleSecureEnclave,
        ]
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::IntelSgxDcap => "intel_sgx_dcap",
            Self::AmdSevSnp => "amd_sev_snp",
            Self::AwsNitro => "aws_nitro",
            Self::AppleSecureEnclave => "apple_secure_enclave",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum VendorRootPayload {
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
pub struct VendorRoot {
    pub vendor: TeeVendor,
    pub root_id: VendorRootId,
    pub payload: VendorRootPayload,
    pub valid_from_block: u64,
    pub valid_until_block: Option<u64>,
    pub introduced_by_governance_action: GovernanceActionId,
    pub revoked_by_governance_action: Option<GovernanceActionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct VendorCapProfile {
    pub vendor: TeeVendor,
    pub cap_multiplier_microunits: u128,
    pub composition_eligibility: bool,
    pub last_updated_governance_action: GovernanceActionId,
}

impl VendorCapProfile {
    pub fn cap_multiplier_decimal_string(&self) -> String {
        tee_cap_multiplier_decimal_string(self.cap_multiplier_microunits)
    }
}

pub fn tee_cap_multiplier_decimal_string(units: u128) -> String {
    let whole = units / TEE_CAP_MULTIPLIER_SCALE_UNITS;
    let fractional = units % TEE_CAP_MULTIPLIER_SCALE_UNITS;
    if fractional == 0 {
        return format!("{whole}.0");
    }
    let mut fractional = format!("{fractional:06}");
    while fractional.ends_with('0') {
        fractional.pop();
    }
    format!("{whole}.{fractional}")
}

pub(crate) fn launch_vendor_cap_profile(vendor: TeeVendor) -> VendorCapProfile {
    let (cap_multiplier_microunits, composition_eligibility) = match vendor {
        TeeVendor::AppleSecureEnclave => (TEE_CAP_MULTIPLIER_APPLE_SECURE_ENCLAVE_UNITS, false),
        TeeVendor::IntelSgxDcap | TeeVendor::AmdSevSnp | TeeVendor::AwsNitro => {
            (TEE_CAP_MULTIPLIER_BASELINE_UNITS, true)
        }
    };
    VendorCapProfile {
        vendor,
        cap_multiplier_microunits,
        composition_eligibility,
        last_updated_governance_action: [0_u8; 32],
    }
}

pub(crate) fn launch_vendor_cap_profiles() -> BTreeMap<TeeVendor, VendorCapProfile> {
    TeeVendor::all()
        .into_iter()
        .map(|vendor| (vendor, launch_vendor_cap_profile(vendor)))
        .collect()
}

pub(crate) fn derive_vendor_root_id(
    vendor: TeeVendor,
    payload: &VendorRootPayload,
) -> VendorRootId {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"tee-vendor-root-id-v1");
    bytes.extend_from_slice(vendor.as_str().as_bytes());
    bytes.extend_from_slice(
        &borsh::to_vec(payload).expect("tee vendor root payload should serialize"),
    );
    *keccak256(bytes)
}

pub fn tee_governance_action_id_for_submission(
    action: &BlockTeeGovernanceAction,
    timestamp_ms: u64,
) -> TeeGovernanceActionId {
    let action_bytes = borsh::to_vec(action).expect("tee governance action should serialize");
    let mut payload = Vec::new();
    payload.extend_from_slice(b"tee-governance-action-id-v1");
    payload.extend_from_slice(&timestamp_ms.to_be_bytes());
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

pub fn tee_governance_action_fingerprint(action: &BlockTeeGovernanceAction) -> Hash {
    let action_bytes = borsh::to_vec(action).expect("tee governance action should serialize");
    let mut payload = Vec::new();
    payload.extend_from_slice(b"tee-governance-action-fingerprint-v1");
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

pub fn real_capital_governance_action_id_for_submission(
    action: &BlockRealCapitalGovernanceAction,
    timestamp_ms: u64,
) -> RealCapitalGovernanceActionId {
    let action_bytes =
        borsh::to_vec(action).expect("real-capital governance action should serialize");
    let mut payload = Vec::new();
    payload.extend_from_slice(b"real-capital-governance-action-id-v1");
    payload.extend_from_slice(&timestamp_ms.to_be_bytes());
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

pub fn real_capital_governance_action_fingerprint(
    action: &BlockRealCapitalGovernanceAction,
) -> Hash {
    let action_bytes =
        borsh::to_vec(action).expect("real-capital governance action should serialize");
    let mut payload = Vec::new();
    payload.extend_from_slice(b"real-capital-governance-action-fingerprint-v1");
    payload.extend_from_slice(&action_bytes);
    *keccak256(payload)
}

pub(crate) fn vendor_root_payload_matches_vendor(
    payload: &VendorRootPayload,
    vendor: TeeVendor,
) -> bool {
    matches!(
        (payload, vendor),
        (
            VendorRootPayload::IntelSgxRoot { .. },
            TeeVendor::IntelSgxDcap
        ) | (VendorRootPayload::AmdSevRoot { .. }, TeeVendor::AmdSevSnp)
            | (VendorRootPayload::AwsNitroRoot { .. }, TeeVendor::AwsNitro)
            | (
                VendorRootPayload::AppleSecureEnclaveRoot { .. },
                TeeVendor::AppleSecureEnclave
            )
    )
}

pub(crate) fn vendor_root_payload_within_bounds(payload: &VendorRootPayload) -> bool {
    match payload {
        VendorRootPayload::IntelSgxRoot {
            root_cert_der,
            tcb_info_signing_cert_der,
            tcb_info_url,
            crl_der,
        } => {
            root_cert_der.len() <= MAX_TEE_VENDOR_ROOT_CERT_DER_BYTES
                && tcb_info_signing_cert_der.len() <= MAX_TEE_VENDOR_ROOT_CERT_DER_BYTES
                && tcb_info_url.len() <= MAX_TEE_VENDOR_TCB_INFO_URL_BYTES
                && crl_der.len() <= MAX_TEE_VENDOR_CRL_DER_BYTES
        }
        VendorRootPayload::AmdSevRoot {
            ark_cert_der,
            ask_cert_der,
            crl_der,
        } => {
            ark_cert_der.len() <= MAX_TEE_VENDOR_ROOT_CERT_DER_BYTES
                && ask_cert_der.len() <= MAX_TEE_VENDOR_ROOT_CERT_DER_BYTES
                && crl_der.len() <= MAX_TEE_VENDOR_CRL_DER_BYTES
        }
        VendorRootPayload::AwsNitroRoot { root_cert_der }
        | VendorRootPayload::AppleSecureEnclaveRoot { root_cert_der } => {
            root_cert_der.len() <= MAX_TEE_VENDOR_ROOT_CERT_DER_BYTES
        }
    }
}

pub(crate) fn tee_vendor_from_block(vendor: BlockTeeVendor) -> TeeVendor {
    match vendor {
        BlockTeeVendor::IntelSgxDcap => TeeVendor::IntelSgxDcap,
        BlockTeeVendor::AmdSevSnp => TeeVendor::AmdSevSnp,
        BlockTeeVendor::AwsNitro => TeeVendor::AwsNitro,
        BlockTeeVendor::AppleSecureEnclave => TeeVendor::AppleSecureEnclave,
    }
}

pub(crate) fn vendor_root_payload_from_block(
    payload: &BlockVendorRootPayload,
) -> VendorRootPayload {
    match payload {
        BlockVendorRootPayload::IntelSgxRoot {
            root_cert_der,
            tcb_info_signing_cert_der,
            tcb_info_url,
            crl_der,
        } => VendorRootPayload::IntelSgxRoot {
            root_cert_der: root_cert_der.clone(),
            tcb_info_signing_cert_der: tcb_info_signing_cert_der.clone(),
            tcb_info_url: tcb_info_url.clone(),
            crl_der: crl_der.clone(),
        },
        BlockVendorRootPayload::AmdSevRoot {
            ark_cert_der,
            ask_cert_der,
            crl_der,
        } => VendorRootPayload::AmdSevRoot {
            ark_cert_der: ark_cert_der.clone(),
            ask_cert_der: ask_cert_der.clone(),
            crl_der: crl_der.clone(),
        },
        BlockVendorRootPayload::AwsNitroRoot { root_cert_der } => VendorRootPayload::AwsNitroRoot {
            root_cert_der: root_cert_der.clone(),
        },
        BlockVendorRootPayload::AppleSecureEnclaveRoot { root_cert_der } => {
            VendorRootPayload::AppleSecureEnclaveRoot {
                root_cert_der: root_cert_der.clone(),
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn vendor_root_payload_to_block(payload: &VendorRootPayload) -> BlockVendorRootPayload {
    match payload {
        VendorRootPayload::IntelSgxRoot {
            root_cert_der,
            tcb_info_signing_cert_der,
            tcb_info_url,
            crl_der,
        } => BlockVendorRootPayload::IntelSgxRoot {
            root_cert_der: root_cert_der.clone(),
            tcb_info_signing_cert_der: tcb_info_signing_cert_der.clone(),
            tcb_info_url: tcb_info_url.clone(),
            crl_der: crl_der.clone(),
        },
        VendorRootPayload::AmdSevRoot {
            ark_cert_der,
            ask_cert_der,
            crl_der,
        } => BlockVendorRootPayload::AmdSevRoot {
            ark_cert_der: ark_cert_der.clone(),
            ask_cert_der: ask_cert_der.clone(),
            crl_der: crl_der.clone(),
        },
        VendorRootPayload::AwsNitroRoot { root_cert_der } => BlockVendorRootPayload::AwsNitroRoot {
            root_cert_der: root_cert_der.clone(),
        },
        VendorRootPayload::AppleSecureEnclaveRoot { root_cert_der } => {
            BlockVendorRootPayload::AppleSecureEnclaveRoot {
                root_cert_der: root_cert_der.clone(),
            }
        }
    }
}

// Cross-crate enum bridge: node and execution crates keep separate vendor
// types to avoid a dependency cycle.
pub(crate) fn execution_tee_vendor(vendor: TeeVendor) -> AttestationVerifyVendor {
    match vendor {
        TeeVendor::IntelSgxDcap => AttestationVerifyVendor::IntelSgxDcap,
        TeeVendor::AmdSevSnp => AttestationVerifyVendor::AmdSevSnp,
        TeeVendor::AwsNitro => AttestationVerifyVendor::AwsNitro,
        TeeVendor::AppleSecureEnclave => AttestationVerifyVendor::AppleSecureEnclave,
    }
}

pub(crate) fn execution_attestation_vendor_roots_state(
    vendor: TeeVendor,
    roots: &[VendorRoot],
) -> AttestationVendorRootsState {
    AttestationVendorRootsState {
        vendor: execution_tee_vendor(vendor),
        roots: roots
            .iter()
            .map(|root| AttestationVendorRoot {
                root_id: root.root_id,
                valid_from_block: root.valid_from_block,
                valid_until_block: root.valid_until_block,
                payload: borsh::to_vec(&root.payload)
                    .expect("tee vendor root payload should serialize"),
            })
            .collect(),
    }
}

pub(crate) fn identity_tier_status(tier: AgentIdentityTier) -> IdentityTierStatus {
    match tier {
        AgentIdentityTier::Tier0Unattested => IdentityTierStatus::Tier0Unattested,
        AgentIdentityTier::Tier1OperatorAttested => IdentityTierStatus::Tier1OperatorAttested,
        AgentIdentityTier::Tier2TeeAttested => IdentityTierStatus::Tier2TeeAttested,
    }
}

pub(crate) fn agent_identity_tier_rank(tier: AgentIdentityTier) -> u8 {
    match tier {
        AgentIdentityTier::Tier0Unattested => 0,
        AgentIdentityTier::Tier1OperatorAttested => 1,
        AgentIdentityTier::Tier2TeeAttested => 2,
    }
}

pub(crate) fn agent_identity_tier_downgraded(
    before: AgentIdentityTier,
    after: AgentIdentityTier,
) -> bool {
    agent_identity_tier_rank(after) < agent_identity_tier_rank(before)
}

pub(crate) fn identity_agent_kind(kind: AgentKind) -> IdentityAgentKind {
    match kind {
        AgentKind::Unknown => IdentityAgentKind::Unknown,
    }
}

pub(crate) fn identity_attestation_kind(kind: AttestationKind) -> IdentityAttestationKind {
    match kind {
        AttestationKind::OperatorSelf => IdentityAttestationKind::OperatorSelf,
        AttestationKind::TeeEnclave => IdentityAttestationKind::TeeEnclave,
        AttestationKind::ThirdParty => IdentityAttestationKind::ThirdParty,
    }
}

pub(crate) fn identity_attestation_vendor_for_tee(vendor: TeeVendor) -> IdentityAttestationVendor {
    match vendor {
        TeeVendor::IntelSgxDcap => IdentityAttestationVendor::IntelSgxDcap,
        TeeVendor::AmdSevSnp => IdentityAttestationVendor::AmdSevSnp,
        TeeVendor::AwsNitro => IdentityAttestationVendor::AwsNitro,
        TeeVendor::AppleSecureEnclave => IdentityAttestationVendor::AppleSecureEnclave,
    }
}

pub(crate) fn identity_attestation_claim(claim: &AttestationClaim) -> IdentityAttestationClaim {
    match claim {
        AttestationClaim::RunsCodeHash {
            code_hash,
            model_hash,
        } => IdentityAttestationClaim::RunsCodeHash {
            code_hash: *code_hash,
            model_hash: *model_hash,
        },
        AttestationClaim::BoundToMandate { mandate_id } => {
            IdentityAttestationClaim::BoundToMandate {
                mandate_id: *mandate_id,
            }
        }
        AttestationClaim::RunsCodeAndBoundToMandate {
            code_hash,
            model_hash,
            mandate_id,
        } => IdentityAttestationClaim::RunsCodeAndBoundToMandate {
            code_hash: *code_hash,
            model_hash: *model_hash,
            mandate_id: *mandate_id,
        },
    }
}

pub(crate) fn identity_tier_transition_reason(
    reason: TierTransitionReason,
) -> IdentityTierTransitionReason {
    match reason {
        TierTransitionReason::AttestationRegistered => {
            IdentityTierTransitionReason::AttestationRegistered
        }
        TierTransitionReason::AttestationRevoked => {
            IdentityTierTransitionReason::AttestationRevoked
        }
        TierTransitionReason::AttestationExpired => {
            IdentityTierTransitionReason::AttestationExpired
        }
        TierTransitionReason::OperatorChanged => IdentityTierTransitionReason::OperatorChanged,
        TierTransitionReason::VendorRootChanged => IdentityTierTransitionReason::VendorRootChanged,
    }
}

pub(crate) fn identity_attestation_revocation_reason(
    reason: AttestationRevocationReason,
) -> IdentityAttestationRevocationReason {
    match reason {
        AttestationRevocationReason::AttestorRevoked => {
            IdentityAttestationRevocationReason::AttestorRevoked
        }
        AttestationRevocationReason::SubjectRequested => {
            IdentityAttestationRevocationReason::SubjectRequested
        }
        AttestationRevocationReason::VendorRootRevoked => {
            IdentityAttestationRevocationReason::VendorRootRevoked
        }
    }
}

pub(crate) fn attestation_claim_code_hash(claim: &AttestationClaim) -> Hash {
    match claim {
        AttestationClaim::RunsCodeHash { code_hash, .. }
        | AttestationClaim::RunsCodeAndBoundToMandate { code_hash, .. } => *code_hash,
        AttestationClaim::BoundToMandate { .. } => [0_u8; 32],
    }
}

pub(crate) fn tee_verifier_outcome_matches_claim(
    claim: &AttestationClaim,
    outcome: &AttestationVerifyOutcome,
) -> bool {
    match claim {
        AttestationClaim::RunsCodeHash {
            code_hash,
            model_hash,
        }
        | AttestationClaim::RunsCodeAndBoundToMandate {
            code_hash,
            model_hash,
            ..
        } => {
            *code_hash == outcome.code_hash
                && model_hash
                    .as_ref()
                    .is_none_or(|expected| *expected == outcome.model_hash)
        }
        AttestationClaim::BoundToMandate { .. } => true,
    }
}

pub(crate) fn agent_kind_event_value(kind: AgentKind) -> String {
    match kind {
        AgentKind::Unknown => "unknown".to_string(),
    }
}

pub(crate) fn account_id_event_value(account: Option<u64>) -> Option<String> {
    account.map(|value| value.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct RegisterAgentIdentityInput {
    pub account: u64,
    pub display_name: String,
    pub description: String,
    pub kind: AgentKind,
    pub operator: Option<u64>,
    pub contact_uri: Option<String>,
}

pub type AttestationRecordId = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AttestationKind {
    OperatorSelf,
    // Reserved for RFC 051 TEE evidence.
    TeeEnclave,
    // Reserved for the later third-party attestation path.
    ThirdParty,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AttestationClaim {
    RunsCodeHash {
        code_hash: Hash,
        model_hash: Option<Hash>,
    },
    BoundToMandate {
        mandate_id: MandateId,
    },
    RunsCodeAndBoundToMandate {
        code_hash: Hash,
        model_hash: Option<Hash>,
        mandate_id: MandateId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AttestationEvidence {
    OperatorSignature,
    TeeQuoteVerified {
        vendor: TeeVendor,
        vendor_root_id: VendorRootId,
        enclave_id: Hash,
        signer: Hash,
        verified_at_block: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TierTransitionReason {
    AttestationRegistered,
    AttestationRevoked,
    AttestationExpired,
    OperatorChanged,
    VendorRootChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AttestationRecord {
    pub record_id: AttestationRecordId,
    pub account: u64,
    pub kind: AttestationKind,
    pub attestor: u64,
    pub claim: AttestationClaim,
    pub evidence: AttestationEvidence,
    pub issued_at_block: u64,
    pub issued_at_timestamp_ms: u64,
    pub expires_at_timestamp_ms: u64,
    pub signature: [u8; 64],
    pub revoked_at_block: Option<u64>,
    pub verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct RegisterOperatorAttestationInput {
    pub attestor: u64,
    pub subject: u64,
    pub claim: AttestationClaim,
    pub expires_at_timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct RefreshOperatorAttestationInput {
    pub attestor: u64,
    pub record_id: AttestationRecordId,
    pub new_expires_at_timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct RevokeAttestationInput {
    pub revoker: u64,
    pub record_id: AttestationRecordId,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct RegisterTeeAttestationInput {
    pub attestor: u64,
    pub subject: u64,
    pub claim: AttestationClaim,
    pub expires_at_timestamp_ms: u64,
    pub quote_bytes: Vec<u8>,
    pub nonce: Hash,
    pub freshness_anchor_block: u64,
    pub vendor: TeeVendor,
}

#[derive(BorshSerialize)]
struct AttestationRecordIdMaterial {
    attestor: u64,
    subject: u64,
    claim: AttestationClaim,
    expires_at_timestamp_ms: u64,
    issued_at_block: u64,
    issued_at_timestamp_ms: u64,
    // The record id commits to the exact signed bytes, not only the logical claim.
    signature: [u8; 64],
}

#[derive(BorshSerialize)]
struct TeeAttestationRecordIdMaterial {
    attestor: u64,
    subject: u64,
    claim: AttestationClaim,
    vendor: TeeVendor,
    expires_at_timestamp_ms: u64,
    quote_hash: Hash,
    nonce: Hash,
    freshness_anchor_block: u64,
    issued_at_block: u64,
    issued_at_timestamp_ms: u64,
    attestor_signature: [u8; 64],
    subject_signature: Option<[u8; 64]>,
}

pub fn derive_attestation_record_id(
    input: &RegisterOperatorAttestationInput,
    issued_at_block: u64,
    issued_at_timestamp_ms: u64,
    signature: &[u8; 64],
) -> AttestationRecordId {
    let material = AttestationRecordIdMaterial {
        attestor: input.attestor,
        subject: input.subject,
        claim: input.claim.clone(),
        expires_at_timestamp_ms: input.expires_at_timestamp_ms,
        issued_at_block,
        issued_at_timestamp_ms,
        signature: *signature,
    };
    let bytes = borsh::to_vec(&material).expect("attestation id material should serialize");
    keccak256(bytes).into()
}

pub fn derive_tee_attestation_record_id(
    input: &RegisterTeeAttestationInput,
    issued_at_block: u64,
    issued_at_timestamp_ms: u64,
    attestor_signature: &[u8; 64],
    subject_signature: Option<&[u8; 64]>,
) -> AttestationRecordId {
    let material = TeeAttestationRecordIdMaterial {
        attestor: input.attestor,
        subject: input.subject,
        claim: input.claim.clone(),
        vendor: input.vendor,
        expires_at_timestamp_ms: input.expires_at_timestamp_ms,
        quote_hash: tee_quote_hash(&input.quote_bytes),
        nonce: input.nonce,
        freshness_anchor_block: input.freshness_anchor_block,
        issued_at_block,
        issued_at_timestamp_ms,
        attestor_signature: *attestor_signature,
        subject_signature: subject_signature.copied(),
    };
    let bytes = borsh::to_vec(&material).expect("TEE attestation id material should serialize");
    keccak256(bytes).into()
}

pub(crate) fn attestation_is_active(record: &AttestationRecord, timestamp_ms: u64) -> bool {
    record.verified
        && record.revoked_at_block.is_none()
        && record.expires_at_timestamp_ms > timestamp_ms
}

pub(crate) fn attestation_claim_matches_mandate(
    claim: &AttestationClaim,
    mandate_id: &MandateId,
) -> bool {
    match claim {
        AttestationClaim::BoundToMandate {
            mandate_id: claim_mandate_id,
        }
        | AttestationClaim::RunsCodeAndBoundToMandate {
            mandate_id: claim_mandate_id,
            ..
        } => claim_mandate_id == mandate_id,
        AttestationClaim::RunsCodeHash { .. } => false,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct RefreshTeeAttestationInput {
    pub attestor: u64,
    pub record_id: AttestationRecordId,
    pub new_expires_at_timestamp_ms: u64,
    pub quote_bytes: Vec<u8>,
    pub nonce: Hash,
    pub freshness_anchor_block: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum DescriptiveUpdate {
    DisplayName(String),
    Description(String),
    Kind(AgentKind),
    Operator(Option<u64>),
    ContactUri(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct UpdateAgentIdentityInput {
    pub account: u64,
    pub update: DescriptiveUpdate,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct DeleteAgentIdentityInput {
    pub account: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AttestationRevocationReason {
    AttestorRevoked,
    SubjectRequested,
    VendorRootRevoked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum IdentityActionKind {
    Register,
    UpdateDescriptive,
    RegisterOperatorAttestation,
    RefreshOperatorAttestation,
    RevokeAttestation,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdentityNonceNamespace {
    AccountSelf,
    Attestor,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum IdentityAction {
    Register(RegisterAgentIdentityInput),
    UpdateDescriptive(UpdateAgentIdentityInput),
    RegisterOperatorAttestation(RegisterOperatorAttestationInput),
    RefreshOperatorAttestation(RefreshOperatorAttestationInput),
    RevokeAttestation(RevokeAttestationInput),
    Delete(DeleteAgentIdentityInput),
}

impl IdentityAction {
    pub fn kind(&self) -> IdentityActionKind {
        match self {
            IdentityAction::Register(_) => IdentityActionKind::Register,
            IdentityAction::UpdateDescriptive(_) => IdentityActionKind::UpdateDescriptive,
            IdentityAction::RegisterOperatorAttestation(_) => {
                IdentityActionKind::RegisterOperatorAttestation
            }
            IdentityAction::RefreshOperatorAttestation(_) => {
                IdentityActionKind::RefreshOperatorAttestation
            }
            IdentityAction::RevokeAttestation(_) => IdentityActionKind::RevokeAttestation,
            IdentityAction::Delete(_) => IdentityActionKind::Delete,
        }
    }

    pub fn account(&self) -> u64 {
        match self {
            IdentityAction::Register(input) => input.account,
            IdentityAction::UpdateDescriptive(input) => input.account,
            IdentityAction::RegisterOperatorAttestation(input) => input.attestor,
            IdentityAction::RefreshOperatorAttestation(input) => input.attestor,
            IdentityAction::RevokeAttestation(input) => input.revoker,
            IdentityAction::Delete(input) => input.account,
        }
    }

    pub fn subject_account(&self) -> u64 {
        match self {
            IdentityAction::Register(input) => input.account,
            IdentityAction::UpdateDescriptive(input) => input.account,
            IdentityAction::RegisterOperatorAttestation(input) => input.subject,
            // Refresh/revoke address the subject by record_id; callers that
            // need the subject must look up the attestation record first.
            IdentityAction::RefreshOperatorAttestation(_) => 0,
            IdentityAction::RevokeAttestation(_) => 0,
            IdentityAction::Delete(input) => input.account,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedIdentityAction {
    pub chain_id: u64,
    pub account: u64,
    pub nonce: u64,
    pub action: IdentityAction,
    pub signature: [u8; 64],
}

/// Serialize-only account-self signing payload.
///
/// Every field of `RegisterAgentIdentityInput` MUST be bound here
/// directly or by hash; this payload signs the action contents, not the
/// storage struct. Keep this BorshSerialize-only unless consensus needs
/// to ingest this payload as state.
#[derive(Clone, Debug, BorshSerialize)]
struct IdentityActionPayload {
    domain: String,
    chain_id: u64,
    action_kind: IdentityActionKind,
    account: u64,
    display_name_hash: Hash,
    description_hash: Hash,
    kind: AgentKind,
    operator: Option<u64>,
    contact_uri_hash: Option<Hash>,
    account_nonce: u64,
}

#[derive(Clone, Debug, BorshSerialize)]
struct IdentityUpdatePayload {
    domain: String,
    chain_id: u64,
    action_kind: IdentityActionKind,
    account: u64,
    update: DescriptiveUpdate,
    account_nonce: u64,
}

#[derive(Clone, Debug, BorshSerialize)]
struct IdentityDeletePayload {
    domain: String,
    chain_id: u64,
    action_kind: IdentityActionKind,
    account: u64,
    account_nonce: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum OperatorAttestationActionKind {
    Register,
    Refresh,
    Revoke,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum TeeAttestationActionKind {
    Register,
    Refresh,
}

/// Serialize-only OperatorSelf attestation signing payload.
///
/// Each OperatorSelf action kind populates a distinct field subset; the
/// discriminator, chain id, signer, record id or subject, and per-attestor
/// nonce prevent cross-action replay.
#[derive(Clone, Debug, BorshSerialize)]
struct OperatorAttestationPayload {
    domain: String,
    chain_id: u64,
    action_kind: OperatorAttestationActionKind,
    signer: u64,
    subject: Option<u64>,
    record_id: Option<AttestationRecordId>,
    claim: Option<AttestationClaim>,
    expires_at_timestamp_ms: Option<u64>,
    nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum TeeAttestationAction {
    Register(RegisterTeeAttestationInput),
    Refresh(RefreshTeeAttestationInput),
}

impl TeeAttestationAction {
    pub fn attestor(&self) -> u64 {
        match self {
            TeeAttestationAction::Register(input) => input.attestor,
            TeeAttestationAction::Refresh(input) => input.attestor,
        }
    }

    pub fn quote_bytes(&self) -> &[u8] {
        match self {
            TeeAttestationAction::Register(input) => &input.quote_bytes,
            TeeAttestationAction::Refresh(input) => &input.quote_bytes,
        }
    }

    pub fn nonce(&self) -> Hash {
        match self {
            TeeAttestationAction::Register(input) => input.nonce,
            TeeAttestationAction::Refresh(input) => input.nonce,
        }
    }

    pub fn freshness_anchor_block(&self) -> u64 {
        match self {
            TeeAttestationAction::Register(input) => input.freshness_anchor_block,
            TeeAttestationAction::Refresh(input) => input.freshness_anchor_block,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeeAttestationSigners {
    SelfAttest {
        attestor_nonce: u64,
        attestor_signature: [u8; 64],
    },
    OperatorAttest {
        attestor_nonce: u64,
        attestor_signature: [u8; 64],
        subject_nonce: u64,
        subject_signature: [u8; 64],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedTeeAttestationAction {
    pub chain_id: u64,
    pub action: TeeAttestationAction,
    pub signers: TeeAttestationSigners,
}

/// Serialize-only RFC 051 TEE attestation signing payload.
///
/// Both the attestor and subject sign these same bytes. The raw quote is
/// represented by `quote_hash`; quote bytes are action inputs and never become
/// consensus state.
#[derive(Clone, Debug, BorshSerialize)]
struct TeeAttestationRegisterPayload {
    domain: String,
    chain_id: u64,
    action_kind: TeeAttestationActionKind,
    attestor: u64,
    subject: u64,
    claim: AttestationClaim,
    vendor: TeeVendor,
    expires_at_timestamp_ms: u64,
    quote_hash: Hash,
    freshness_anchor_block: u64,
    nonce: Hash,
    attestor_nonce: u64,
    subject_nonce: Option<u64>,
}

#[derive(Clone, Debug, BorshSerialize)]
struct TeeAttestationRefreshPayload {
    domain: String,
    chain_id: u64,
    action_kind: TeeAttestationActionKind,
    attestor: u64,
    record_id: AttestationRecordId,
    new_expires_at_timestamp_ms: u64,
    quote_hash: Hash,
    freshness_anchor_block: u64,
    nonce: Hash,
    attestor_nonce: u64,
    subject_nonce: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AgentIdentityRecord {
    pub account: u64,
    pub tier: AgentIdentityTier,
    pub operator: Option<u64>,
    pub kind: AgentKind,
    pub display_name: String,
    pub description: String,
    pub contact_uri: Option<String>,
    pub registered_at_block: u64,
    pub registered_at_timestamp_ms: u64,
    pub last_updated_block: u64,
    pub last_updated_timestamp_ms: u64,
}

pub(crate) fn identity_string_hash(value: &str) -> Hash {
    keccak256(value.as_bytes()).into()
}

pub fn identity_action_signing_bytes(
    chain_id: u64,
    account: u64,
    nonce: u64,
    action: &IdentityAction,
) -> Vec<u8> {
    match action {
        IdentityAction::Register(input) => {
            let payload = IdentityActionPayload {
                domain: "0x26:identity:account-self:v1".into(),
                chain_id,
                action_kind: IdentityActionKind::Register,
                account,
                display_name_hash: identity_string_hash(&input.display_name),
                description_hash: identity_string_hash(&input.description),
                kind: input.kind,
                operator: input.operator,
                contact_uri_hash: input.contact_uri.as_deref().map(identity_string_hash),
                account_nonce: nonce,
            };
            borsh::to_vec(&payload).expect("identity action payload should serialize")
        }
        IdentityAction::UpdateDescriptive(input) => {
            let payload = IdentityUpdatePayload {
                domain: "0x26:identity:account-self:v1".into(),
                chain_id,
                action_kind: IdentityActionKind::UpdateDescriptive,
                account,
                update: input.update.clone(),
                account_nonce: nonce,
            };
            borsh::to_vec(&payload).expect("identity update payload should serialize")
        }
        IdentityAction::RegisterOperatorAttestation(input) => {
            let payload = OperatorAttestationPayload {
                domain: "0x26:identity:operator-attest:v1".into(),
                chain_id,
                action_kind: OperatorAttestationActionKind::Register,
                signer: account,
                subject: Some(input.subject),
                record_id: None,
                claim: Some(input.claim.clone()),
                expires_at_timestamp_ms: Some(input.expires_at_timestamp_ms),
                nonce,
            };
            borsh::to_vec(&payload).expect("operator attestation payload should serialize")
        }
        IdentityAction::RefreshOperatorAttestation(input) => {
            let payload = OperatorAttestationPayload {
                domain: "0x26:identity:operator-attest:v1".into(),
                chain_id,
                action_kind: OperatorAttestationActionKind::Refresh,
                signer: account,
                subject: None,
                record_id: Some(input.record_id),
                claim: None,
                expires_at_timestamp_ms: Some(input.new_expires_at_timestamp_ms),
                nonce,
            };
            borsh::to_vec(&payload).expect("operator attestation payload should serialize")
        }
        IdentityAction::RevokeAttestation(input) => {
            let payload = OperatorAttestationPayload {
                domain: "0x26:identity:operator-attest:v1".into(),
                chain_id,
                action_kind: OperatorAttestationActionKind::Revoke,
                signer: account,
                subject: None,
                record_id: Some(input.record_id),
                claim: None,
                expires_at_timestamp_ms: None,
                nonce,
            };
            borsh::to_vec(&payload).expect("operator attestation payload should serialize")
        }
        IdentityAction::Delete(_) => {
            let payload = IdentityDeletePayload {
                domain: "0x26:identity:account-self:v1".into(),
                chain_id,
                action_kind: IdentityActionKind::Delete,
                account,
                account_nonce: nonce,
            };
            borsh::to_vec(&payload).expect("identity delete payload should serialize")
        }
    }
}

pub(crate) fn tee_quote_hash(quote_bytes: &[u8]) -> Hash {
    keccak256(quote_bytes).into()
}

pub fn max_tee_quote_bytes(vendor: TeeVendor) -> usize {
    match vendor {
        TeeVendor::IntelSgxDcap => MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP,
        TeeVendor::AmdSevSnp => MAX_TEE_QUOTE_BYTES_AMD_SEV_SNP,
        TeeVendor::AwsNitro => MAX_TEE_QUOTE_BYTES_AWS_NITRO,
        TeeVendor::AppleSecureEnclave => MAX_TEE_QUOTE_BYTES_APPLE_SECURE_ENCLAVE,
    }
}

pub fn tee_attestation_signing_bytes(
    chain_id: u64,
    action: &TeeAttestationAction,
    attestor_nonce: u64,
    subject_nonce: Option<u64>,
) -> Vec<u8> {
    match action {
        TeeAttestationAction::Register(input) => {
            let payload = TeeAttestationRegisterPayload {
                domain: "0x26:identity:tee-attest:v1".into(),
                chain_id,
                action_kind: TeeAttestationActionKind::Register,
                attestor: input.attestor,
                subject: input.subject,
                claim: input.claim.clone(),
                vendor: input.vendor,
                expires_at_timestamp_ms: input.expires_at_timestamp_ms,
                quote_hash: tee_quote_hash(&input.quote_bytes),
                freshness_anchor_block: input.freshness_anchor_block,
                nonce: input.nonce,
                attestor_nonce,
                subject_nonce,
            };
            borsh::to_vec(&payload).expect("TEE attestation payload should serialize")
        }
        TeeAttestationAction::Refresh(input) => {
            let payload = TeeAttestationRefreshPayload {
                domain: "0x26:identity:tee-attest:v1".into(),
                chain_id,
                action_kind: TeeAttestationActionKind::Refresh,
                attestor: input.attestor,
                record_id: input.record_id,
                new_expires_at_timestamp_ms: input.new_expires_at_timestamp_ms,
                quote_hash: tee_quote_hash(&input.quote_bytes),
                freshness_anchor_block: input.freshness_anchor_block,
                nonce: input.nonce,
                attestor_nonce,
                subject_nonce,
            };
            borsh::to_vec(&payload).expect("TEE attestation payload should serialize")
        }
    }
}
