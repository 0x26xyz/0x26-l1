use super::{BuildBlockRequest, NodeError};
use ed25519_dalek::{Signer, SigningKey};
use kombat_chain_consensus::{
    block_hash, body_root, quorum_certificate_signing_bytes,
    validate_block_against_parent_and_membership, validate_quorum_certificate_with_membership,
    Block, BlockBody, BlockHeader, ConsensusPubKey, Hash, QuorumCertificate, Signature,
    ValidatorId, ValidatorVerifier,
};
use kombat_chain_network::{ValidatorConfig, ValidatorSet};
use std::collections::BTreeMap;

pub fn select_leader_for_validator_set(
    round: u64,
    chain_id: u64,
    validator_set: &ValidatorSet,
) -> Result<ValidatorId, NodeError> {
    select_leader(round, chain_id, &validator_set.active_validators())
}

pub fn validator_verifiers_for_validator_set(
    validator_set: &ValidatorSet,
) -> Result<Vec<ValidatorVerifier>, NodeError> {
    validator_verifiers(&validator_set.active_validators())
}

pub fn assemble_quorum_certificate(
    block: &Block,
    signatures: Vec<Signature>,
    validator_verifiers: &[ValidatorVerifier],
) -> Result<QuorumCertificate, NodeError> {
    let qc = QuorumCertificate {
        view: block.header.round,
        block_hash: block_hash(block),
        signatures,
    };
    validate_quorum_certificate_with_membership(&qc, &qc.block_hash, validator_verifiers)
        .map_err(NodeError::InvalidBlock)?;
    Ok(qc)
}

pub(super) fn build_block(request: BuildBlockRequest<'_>) -> Result<Block, NodeError> {
    let parent_qc = build_quorum_certificate(request.parent, request.consensus_signing_keys)?;
    let block = Block {
        header: BlockHeader {
            chain_id: request.chain_id,
            height: request.parent.header.height.saturating_add(1),
            round: request.round,
            proposer: request.proposer,
            parent_hash: block_hash(request.parent),
            parent_state_root: request.parent.header.state_root,
            tx_root: body_root(&request.body),
            event_root: request.event_root,
            state_root: request.state_root,
            qc: parent_qc,
            timestamp_ms: request.timestamp_ms,
        },
        body: request.body,
    };

    validate_block_against_parent_and_membership(
        &block,
        &request.parent.header,
        request.validator_verifiers,
        request.proposer,
        request.timestamp_ms,
        request.max_clock_skew_ms,
    )
    .map_err(NodeError::InvalidBlock)?;

    Ok(block)
}

pub(super) fn build_genesis_block(
    chain_id: u64,
    proposer: ValidatorId,
    genesis_state_root: Hash,
    genesis_timestamp_ms: u64,
) -> Block {
    let body = BlockBody { items: vec![] };

    Block {
        header: BlockHeader {
            chain_id,
            height: 0,
            round: 0,
            proposer,
            parent_hash: [0; 32],
            parent_state_root: genesis_state_root,
            tx_root: body_root(&body),
            event_root: [0; 32],
            state_root: genesis_state_root,
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![Signature {
                    validator: proposer,
                    bytes: [0; 96],
                }],
            },
            timestamp_ms: genesis_timestamp_ms,
        },
        body,
    }
}

pub(super) fn build_quorum_certificate(
    block: &Block,
    consensus_signing_keys: &BTreeMap<ValidatorId, [u8; 32]>,
) -> Result<QuorumCertificate, NodeError> {
    let mut qc = QuorumCertificate {
        view: block.header.round,
        block_hash: block_hash(block),
        signatures: Vec::with_capacity(consensus_signing_keys.len()),
    };
    let signing_bytes = quorum_certificate_signing_bytes(&qc).map_err(NodeError::InvalidBlock)?;
    for (validator_id, signing_key) in consensus_signing_keys {
        qc.signatures.push(Signature {
            validator: *validator_id,
            bytes: consensus_signature_bytes(*signing_key, &signing_bytes),
        });
    }

    Ok(qc)
}

pub(super) fn synthetic_mahi_quorum_certificate(block: &Block) -> QuorumCertificate {
    QuorumCertificate {
        view: block.header.round,
        block_hash: block_hash(block),
        signatures: Vec::new(),
    }
}

pub(super) fn select_leader(
    round: u64,
    chain_id: u64,
    validators: &[ValidatorConfig],
) -> Result<ValidatorId, NodeError> {
    let total_stake: u128 = validators
        .iter()
        .filter(|validator| validator.enabled)
        .map(|validator| validator.bond.amount as u128)
        .sum();

    if total_stake == 0 {
        return Err(NodeError::ZeroStake);
    }

    let target = (mix_leader_seed(chain_id, round) as u128) % total_stake;
    let mut cumulative = 0_u128;

    for validator in validators.iter().filter(|validator| validator.enabled) {
        cumulative = cumulative.saturating_add(validator.bond.amount as u128);
        if cumulative > target {
            return Ok(validator.validator_id);
        }
    }

    validators
        .iter()
        .rev()
        .find(|validator| validator.enabled)
        .map(|validator| validator.validator_id)
        .ok_or(NodeError::MissingLeader { round })
}

fn mix_leader_seed(chain_id: u64, round: u64) -> u64 {
    let mut value = chain_id.rotate_left(32) ^ round ^ 0x9e37_79b9_7f4a_7c15_u64;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9_u64);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb_u64);
    value ^ (value >> 31)
}

pub(super) fn validator_verifiers(
    validators: &[ValidatorConfig],
) -> Result<Vec<ValidatorVerifier>, NodeError> {
    validators
        .iter()
        .map(|validator| {
            Ok(ValidatorVerifier {
                validator: validator.validator_id,
                consensus_pubkey: consensus_pubkey_from_config(validator)?,
            })
        })
        .collect()
}

pub(super) fn consensus_pubkey_from_signing_key(signing_key: [u8; 32]) -> ConsensusPubKey {
    SigningKey::from_bytes(&signing_key)
        .verifying_key()
        .to_bytes()
}

pub(super) fn validate_consensus_key_material(
    validator_verifiers: &[ValidatorVerifier],
    consensus_signing_keys: &BTreeMap<ValidatorId, [u8; 32]>,
    error_prefix: &str,
) -> Result<(), std::io::Error> {
    for verifier in validator_verifiers {
        let signing_key = consensus_signing_keys
            .get(&verifier.validator)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{error_prefix} signing key missing for validator"),
                )
            })?;
        if consensus_pubkey_from_signing_key(*signing_key) != verifier.consensus_pubkey {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{error_prefix} signing key material mismatch"),
            ));
        }
    }

    Ok(())
}

fn consensus_pubkey_from_config(validator: &ValidatorConfig) -> Result<ConsensusPubKey, NodeError> {
    let bytes = validator.consensus_pubkey.as_bytes();
    if bytes.len() != 32 {
        return Err(NodeError::InvalidConsensusPubKey {
            validator_id: validator.validator_id,
        });
    }

    let mut consensus_pubkey = [0_u8; 32];
    consensus_pubkey.copy_from_slice(bytes);
    Ok(consensus_pubkey)
}

pub(super) fn consensus_signature_bytes(signing_key: [u8; 32], message: &[u8]) -> [u8; 96] {
    let mut bytes = [0_u8; 96];
    let signature = SigningKey::from_bytes(&signing_key)
        .sign(message)
        .to_bytes();
    bytes[..64].copy_from_slice(&signature);
    bytes
}
