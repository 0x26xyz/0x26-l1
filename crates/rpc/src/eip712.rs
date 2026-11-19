use crate::{owner_action_name, SignedEvmDelegationAction};
use kombat_chain_consensus::{
    delegation_block_action_kind, BlockDelegationAction, DelegationActionId, Hash,
};
use kombat_chain_node::{delegation_action_scope, mandate_action_body_hash, MandateAction};
use kombat_chain_policy::{OwnerCall, Principal};

const EVM_OWNER_PRINCIPAL_PREFIX: &[u8; 12] = b"0x26:evm:v1!";
const EIP712_DOMAIN_NAME: &str = "0x26";
const EIP712_DOMAIN_VERSION: &str = "1";
const EIP712_DOMAIN_TYPE: &str = "EIP712Domain(string name,string version,uint256 chainId)";
const EVM_ACCOUNT_CLAIM_TYPE: &str = "EvmAccountClaim(uint256 chainId,uint256 accountId,address wallet,bytes32 owner,bytes32 codeHash,uint256 expiresAt)";
const EVM_OWNER_CALL_TYPE: &str = "EvmOwnerCall(uint256 accountId,uint256 chainId,bytes32 signer,uint256 nonce,string actionKind,bytes actionBytes)";
const EVM_MANDATE_ACTION_TYPE: &str = "EvmMandateAction(bytes32 owner,uint256 chainId,uint256 nonce,uint256 validUntilMs,string actionKind,bytes32 actionHash)";
const EVM_DELEGATION_ACTION_TYPE: &str = "EvmDelegationAction(uint256 chainId,uint256 signerAccountId,bytes32 actionId,string actionKind,bytes32 actionScope,uint256 timestampMs,uint256 submitterNonce,uint256 validUntilMs,bytes32 bodyHash)";

pub fn evm_owner_principal(address: [u8; 20]) -> Principal {
    let mut principal = [0_u8; 32];
    principal[..12].copy_from_slice(EVM_OWNER_PRINCIPAL_PREFIX);
    principal[12..].copy_from_slice(&address);
    principal
}

pub(crate) fn evm_address_from_owner_principal(owner: Principal) -> Option<[u8; 20]> {
    if owner[..12] != *EVM_OWNER_PRINCIPAL_PREFIX {
        return None;
    }
    owner[12..].try_into().ok()
}

pub(crate) fn evm_owner_call_eip712_digest(
    account_id: u64,
    chain_id: u64,
    call: &OwnerCall,
) -> Result<[u8; 32], String> {
    let action_bytes =
        borsh::to_vec(&call.action).map_err(|_| "owner action serialization failed".to_string())?;
    let domain_separator = eip712_domain_separator(chain_id);
    let struct_hash = eip712_owner_call_struct_hash(
        account_id,
        chain_id,
        call.signer,
        call.nonce,
        owner_action_name(&call.action),
        &action_bytes,
    );
    let mut payload = Vec::with_capacity(2 + 32 + 32);
    payload.extend_from_slice(b"\x19\x01");
    payload.extend_from_slice(&domain_separator);
    payload.extend_from_slice(&struct_hash);
    Ok(keccak_array(&payload))
}

pub(crate) fn evm_account_claim_eip712_digest(
    chain_id: u64,
    account_id: u64,
    wallet: [u8; 20],
    owner: Principal,
    code_hash: Hash,
    expires_at: u64,
) -> [u8; 32] {
    let domain_separator = eip712_domain_separator(chain_id);
    let struct_hash = eip712_account_claim_struct_hash(
        chain_id, account_id, wallet, owner, code_hash, expires_at,
    );
    let mut payload = Vec::with_capacity(2 + 32 + 32);
    payload.extend_from_slice(b"\x19\x01");
    payload.extend_from_slice(&domain_separator);
    payload.extend_from_slice(&struct_hash);
    keccak_array(&payload)
}

pub(crate) fn evm_mandate_action_eip712_digest(
    chain_id: u64,
    owner: Principal,
    nonce: u64,
    valid_until_ms: u64,
    action: &MandateAction,
) -> Result<[u8; 32], String> {
    let domain_separator = eip712_domain_separator(chain_id);
    let struct_hash = eip712_mandate_action_struct_hash(
        chain_id,
        owner,
        nonce,
        valid_until_ms,
        mandate_action_name(action),
        mandate_action_body_hash(action),
    );
    let mut payload = Vec::with_capacity(2 + 32 + 32);
    payload.extend_from_slice(b"\x19\x01");
    payload.extend_from_slice(&domain_separator);
    payload.extend_from_slice(&struct_hash);
    Ok(keccak_array(&payload))
}

pub(crate) fn evm_delegation_action_eip712_digest(
    signed_action: &SignedEvmDelegationAction,
) -> Result<[u8; 32], String> {
    let domain_separator = eip712_domain_separator(signed_action.chain_id);
    let raw_scope = delegation_action_scope(&signed_action.action);
    let body_hash = delegation_action_body_hash(&signed_action.action)?;
    let struct_hash = eip712_delegation_action_struct_hash(
        signed_action.chain_id,
        signed_action.signer_account_id,
        signed_action.action_id,
        delegation_block_action_kind(&signed_action.action),
        keccak_array(&raw_scope),
        signed_action.timestamp_ms,
        signed_action.submitter_nonce,
        signed_action.valid_until_ms,
        body_hash,
    );
    let mut payload = Vec::with_capacity(2 + 32 + 32);
    payload.extend_from_slice(b"\x19\x01");
    payload.extend_from_slice(&domain_separator);
    payload.extend_from_slice(&struct_hash);
    Ok(keccak_array(&payload))
}

fn mandate_action_name(action: &MandateAction) -> &'static str {
    match action {
        MandateAction::CreateMandate(_) => "create_mandate",
        MandateAction::SuspendMandate { .. } => "suspend_mandate",
        MandateAction::ResumeMandate { .. } => "resume_mandate",
        MandateAction::RevokeMandate { .. } => "revoke_mandate",
    }
}

fn delegation_action_body_hash(action: &BlockDelegationAction) -> Result<Hash, String> {
    let action_bytes =
        borsh::to_vec(action).map_err(|_| "delegation action serialization failed".to_string())?;
    Ok(keccak_array(&action_bytes))
}

fn eip712_domain_separator(chain_id: u64) -> [u8; 32] {
    let mut payload = Vec::with_capacity(32 * 4);
    payload.extend_from_slice(&keccak_array(EIP712_DOMAIN_TYPE.as_bytes()));
    payload.extend_from_slice(&keccak_array(EIP712_DOMAIN_NAME.as_bytes()));
    payload.extend_from_slice(&keccak_array(EIP712_DOMAIN_VERSION.as_bytes()));
    payload.extend_from_slice(&eip712_u64_word(chain_id));
    keccak_array(&payload)
}

fn eip712_owner_call_struct_hash(
    account_id: u64,
    chain_id: u64,
    signer: Principal,
    nonce: u64,
    action_kind: &str,
    action_bytes: &[u8],
) -> [u8; 32] {
    let mut payload = Vec::with_capacity(32 * 7);
    payload.extend_from_slice(&keccak_array(EVM_OWNER_CALL_TYPE.as_bytes()));
    payload.extend_from_slice(&eip712_u64_word(account_id));
    payload.extend_from_slice(&eip712_u64_word(chain_id));
    payload.extend_from_slice(&signer);
    payload.extend_from_slice(&eip712_u64_word(nonce));
    payload.extend_from_slice(&keccak_array(action_kind.as_bytes()));
    payload.extend_from_slice(&keccak_array(action_bytes));
    keccak_array(&payload)
}

fn eip712_account_claim_struct_hash(
    chain_id: u64,
    account_id: u64,
    wallet: [u8; 20],
    owner: Principal,
    code_hash: Hash,
    expires_at: u64,
) -> [u8; 32] {
    let mut payload = Vec::with_capacity(32 * 7);
    payload.extend_from_slice(&keccak_array(EVM_ACCOUNT_CLAIM_TYPE.as_bytes()));
    payload.extend_from_slice(&eip712_u64_word(chain_id));
    payload.extend_from_slice(&eip712_u64_word(account_id));
    payload.extend_from_slice(&eip712_address_word(wallet));
    payload.extend_from_slice(&owner);
    payload.extend_from_slice(&code_hash);
    payload.extend_from_slice(&eip712_u64_word(expires_at));
    keccak_array(&payload)
}

fn eip712_mandate_action_struct_hash(
    chain_id: u64,
    owner: Principal,
    nonce: u64,
    valid_until_ms: u64,
    action_kind: &str,
    action_hash: Hash,
) -> [u8; 32] {
    let mut payload = Vec::with_capacity(32 * 7);
    payload.extend_from_slice(&keccak_array(EVM_MANDATE_ACTION_TYPE.as_bytes()));
    payload.extend_from_slice(&owner);
    payload.extend_from_slice(&eip712_u64_word(chain_id));
    payload.extend_from_slice(&eip712_u64_word(nonce));
    payload.extend_from_slice(&eip712_u64_word(valid_until_ms));
    payload.extend_from_slice(&keccak_array(action_kind.as_bytes()));
    payload.extend_from_slice(&action_hash);
    keccak_array(&payload)
}

fn eip712_delegation_action_struct_hash(
    chain_id: u64,
    signer_account_id: u64,
    action_id: DelegationActionId,
    action_kind: &str,
    action_scope_hash: Hash,
    timestamp_ms: u64,
    submitter_nonce: u64,
    valid_until_ms: u64,
    body_hash: Hash,
) -> [u8; 32] {
    let mut payload = Vec::with_capacity(32 * 10);
    payload.extend_from_slice(&keccak_array(EVM_DELEGATION_ACTION_TYPE.as_bytes()));
    payload.extend_from_slice(&eip712_u64_word(chain_id));
    payload.extend_from_slice(&eip712_u64_word(signer_account_id));
    payload.extend_from_slice(&action_id);
    payload.extend_from_slice(&keccak_array(action_kind.as_bytes()));
    payload.extend_from_slice(&action_scope_hash);
    payload.extend_from_slice(&eip712_u64_word(timestamp_ms));
    payload.extend_from_slice(&eip712_u64_word(submitter_nonce));
    payload.extend_from_slice(&eip712_u64_word(valid_until_ms));
    payload.extend_from_slice(&body_hash);
    keccak_array(&payload)
}

pub(crate) fn recover_evm_owner_call_address(
    digest: &[u8; 32],
    signature: &[u8],
) -> Result<[u8; 20], String> {
    use alloy_consensus::crypto::secp256k1::public_key_to_address;
    use secp256k1::ecdsa::{RecoverableSignature, RecoveryId};
    use secp256k1::{Message, Secp256k1};

    if signature.len() != 65 {
        return Err("invalid signature length".into());
    }
    let mut recovery_id = signature[64];
    if recovery_id >= 27 {
        recovery_id = recovery_id.saturating_sub(27);
    }
    let recovery_id = RecoveryId::try_from(i32::from(recovery_id))
        .map_err(|_| "invalid recovery id".to_string())?;
    let recoverable = RecoverableSignature::from_compact(&signature[..64], recovery_id)
        .map_err(|_| "invalid signature".to_string())?;
    let message = Message::from_digest(*digest);
    let public_key = Secp256k1::new()
        .recover_ecdsa(&message, &recoverable)
        .map_err(|_| "signature recovery failed".to_string())?;
    Ok(public_key_to_address(public_key).into_array())
}

fn eip712_u64_word(value: u64) -> [u8; 32] {
    let mut out = [0_u8; 32];
    out[24..].copy_from_slice(&value.to_be_bytes());
    out
}

fn eip712_address_word(value: [u8; 20]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    out[12..].copy_from_slice(&value);
    out
}

fn keccak_array(bytes: &[u8]) -> [u8; 32] {
    alloy_primitives::keccak256(bytes).into()
}
