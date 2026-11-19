use crate::parse_mahi_owner_relay_redundancy;
use kombat_chain_consensus::{Hash, ValidatorId};
use kombat_chain_network::ValidatorConfig;
use sha2::{Digest, Sha256};

pub(crate) fn mahi_payload_owner_ids_for_policy(
    active_validators: &[ValidatorConfig],
    policy_id: u64,
) -> Vec<ValidatorId> {
    if active_validators.is_empty() {
        return Vec::new();
    }
    let redundancy = parse_mahi_owner_relay_redundancy(active_validators.len());
    let primary_index = (policy_id as usize) % active_validators.len();
    (0..redundancy)
        .map(|offset| {
            active_validators[(primary_index + offset) % active_validators.len()].validator_id
        })
        .collect()
}

pub(crate) fn mahi_payload_primary_owner_id_for_policy(
    active_validators: &[ValidatorConfig],
    policy_id: u64,
) -> Option<ValidatorId> {
    mahi_payload_owner_ids_for_policy(active_validators, policy_id)
        .into_iter()
        .next()
}

pub(crate) fn mahi_payload_next_owner_id_for_policy(
    active_validators: &[ValidatorConfig],
    policy_id: u64,
    current_owner: ValidatorId,
) -> Option<ValidatorId> {
    let owners = mahi_payload_owner_ids_for_policy(active_validators, policy_id);
    let current_index = owners.iter().position(|owner| *owner == current_owner)?;
    owners.get(current_index.saturating_add(1)).copied()
}

pub(crate) fn mahi_restart_boundary_round_from_head(head: &kombat_chain_node::HeadSnapshot) -> u64 {
    head.head_round
        .saturating_add(1)
        .max(head.current_round)
        .max(1)
}

pub(crate) fn hash_bytes(bytes: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}
