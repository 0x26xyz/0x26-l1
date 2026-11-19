use crate::{encode_hex, JsonRpcErrorObject, NetworkBootstrapConfig};
use kombat_chain_consensus::ValidatorVerifier;
use kombat_chain_node::validator_verifiers_for_validator_set;
use serde_json::{json, Value};

impl NetworkBootstrapConfig {
    pub(crate) fn local_validator_id(&self) -> Option<kombat_chain_consensus::ValidatorId> {
        self.local_validator
            .as_ref()
            .map(|validator| validator.validator_id)
    }

    pub(crate) fn serves_network_routes(&self) -> bool {
        self.local_validator.is_some()
    }
}

pub(crate) fn network_peer_url(base: &str, suffix: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    if let Some(prefix) = trimmed.strip_suffix("/api") {
        format!("{prefix}/network/{suffix}")
    } else {
        format!("{trimmed}/network/{suffix}")
    }
}

pub(crate) fn validator_config_json(validator: &kombat_chain_network::ValidatorConfig) -> Value {
    json!({
        "validator_id_hex": encode_hex(&validator.validator_id),
        "consensus_pubkey_hex": encode_hex(validator.consensus_pubkey.as_bytes()),
        "network_pubkey_hex": encode_hex(validator.network_pubkey.as_bytes()),
        "rpc_endpoints": validator.rpc_endpoints,
        "region": validator.region,
        "operator_name": validator.operator_name,
        "enabled": validator.enabled,
        "bond": {
            "asset": format!("{:?}", validator.bond.asset),
            "amount": validator.bond.amount,
            "status": format!("{:?}", validator.bond.status),
        }
    })
}

pub(crate) fn validator_peer_json(peer: &kombat_chain_network::ValidatorPeer) -> Value {
    json!({
        "validator_id_hex": encode_hex(&peer.validator_id),
        "network_pubkey_hex": encode_hex(peer.network_pubkey.as_bytes()),
        "rpc_endpoints": peer.rpc_endpoints,
        "region": peer.region,
    })
}

pub(crate) fn network_validator_verifiers(
    bootstrap: &NetworkBootstrapConfig,
) -> Result<Vec<ValidatorVerifier>, JsonRpcErrorObject> {
    validator_verifiers_for_validator_set(&bootstrap.validator_set).map_err(|error| {
        JsonRpcErrorObject {
            code: -32000,
            message: format!("invalid network validator set: {error:?}"),
            data: None,
        }
    })
}
