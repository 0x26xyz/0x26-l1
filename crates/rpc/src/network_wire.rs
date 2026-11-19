use crate::{
    JsonRpcErrorObject, SignedDelegationActionJson, SignedEvmDelegationActionJson,
    SignedMarketMakerProgramActionJson, SubmitIntentParams,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayIntentsRequest {
    pub(crate) items: Vec<SubmitIntentParams>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayIntentResult {
    pub(crate) intent_id_hex: String,
    pub(crate) status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayIntentsResponse {
    pub(crate) results: Vec<NetworkRelayIntentResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayDelegationActionsRequest {
    pub(crate) items: Vec<SignedDelegationActionJson>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayEvmDelegationActionsRequest {
    pub(crate) items: Vec<SignedEvmDelegationActionJson>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayDelegationActionResult {
    pub(crate) action_id_hex: String,
    pub(crate) status: String,
    pub(crate) error: Option<JsonRpcErrorObject>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayDelegationActionsResponse {
    pub(crate) results: Vec<NetworkRelayDelegationActionResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayMarketMakerProgramActionsRequest {
    pub(crate) items: Vec<SignedMarketMakerProgramActionJson>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayMarketMakerProgramActionResult {
    pub(crate) action_id_hex: String,
    pub(crate) status: String,
    pub(crate) error: Option<JsonRpcErrorObject>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRelayMarketMakerProgramActionsResponse {
    pub(crate) results: Vec<NetworkRelayMarketMakerProgramActionResult>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkMahiBlockRequest {
    pub(crate) block_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkMahiBlockBatchRequest {
    pub(crate) blocks_hex: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkMahiBlockResponse {
    pub(crate) block_ref_hex: String,
    pub(crate) block_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkMahiBlockBatchResponse {
    pub(crate) results: Vec<NetworkMahiBlockResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkMahiFetchRequest {
    pub(crate) refs_hex: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkMahiFetchResponse {
    pub(crate) blocks_hex: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkProposalRequest {
    pub(crate) block_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkHeadSignatureRequest {
    pub(crate) head_hash_hex: String,
    pub(crate) round: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkTimeoutRequest {
    #[serde(default)]
    pub(crate) head_hash_hex: Option<String>,
    #[serde(default)]
    pub(crate) head_height: Option<u64>,
    pub(crate) round: u64,
    #[serde(default)]
    pub(crate) timeout_hex: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkSignatureResponse {
    pub(crate) validator_id_hex: String,
    pub(crate) signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkTimeoutResponse {
    pub(crate) validator_id_hex: Option<String>,
    pub(crate) timeout_hex: Option<String>,
    pub(crate) message_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkCommitRequest {
    pub(crate) block_hex: String,
    pub(crate) qc_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkTimeoutCertificateRequest {
    pub(crate) timeout_messages_hex: Vec<String>,
    pub(crate) timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkTimeoutCertificateResponse {
    pub(crate) round: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkRoundResponse {
    pub(crate) height: u64,
    pub(crate) hash_hex: String,
    pub(crate) item_count: usize,
    pub(crate) peer_count: usize,
    pub(crate) signature_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkTimeoutRecoveryResponse {
    pub(crate) round: u64,
    pub(crate) peer_count: usize,
    pub(crate) message_count: usize,
    pub(crate) leader_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkPingResponse {
    pub(crate) chain_id: u64,
    pub(crate) current_round: u64,
    pub(crate) head_height: u64,
    pub(crate) head_hash_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct NetworkBootstrapMetadataResponse {
    pub(crate) chain_id: u64,
    pub(crate) validator_set_epoch: u64,
    pub(crate) validator_id_hex: String,
    pub(crate) current_round: u64,
    pub(crate) head_height: u64,
    pub(crate) head_hash_hex: String,
    #[serde(default)]
    pub(crate) committed_state_root_hex: Option<String>,
    #[serde(default)]
    pub(crate) public_rpc_url: Option<String>,
    #[serde(default)]
    pub(crate) snapshot_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportSessionOpenResponse {
    pub(crate) session_id: u64,
    pub(crate) chain_id: u64,
    pub(crate) validator_id_hex: Option<String>,
    #[serde(default)]
    pub(crate) current_round: Option<u64>,
    #[serde(default)]
    pub(crate) current_view: Option<u64>,
    #[serde(default)]
    pub(crate) head_height: Option<u64>,
    #[serde(default)]
    pub(crate) head_hash_hex: Option<String>,
    #[serde(default)]
    pub(crate) committed_state_root_hex: Option<String>,
    #[serde(default)]
    pub(crate) last_committed_round: Option<u64>,
    #[serde(default)]
    pub(crate) peer_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportSessionOpenRequest {
    pub(crate) chain_id: Option<u64>,
    pub(crate) validator_id_hex: Option<String>,
    #[serde(default)]
    pub(crate) current_round: Option<u64>,
    #[serde(default)]
    pub(crate) current_view: Option<u64>,
    #[serde(default)]
    pub(crate) head_height: Option<u64>,
    #[serde(default)]
    pub(crate) head_hash_hex: Option<String>,
    #[serde(default)]
    pub(crate) committed_state_root_hex: Option<String>,
    #[serde(default)]
    pub(crate) last_committed_round: Option<u64>,
    #[serde(default)]
    pub(crate) peer_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportSessionCloseResponse {
    pub(crate) closed: bool,
    pub(crate) session_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportSessionHeartbeatRequest {
    pub(crate) chain_id: Option<u64>,
    pub(crate) validator_id_hex: Option<String>,
    #[serde(default)]
    pub(crate) current_round: Option<u64>,
    #[serde(default)]
    pub(crate) current_view: Option<u64>,
    #[serde(default)]
    pub(crate) head_height: Option<u64>,
    #[serde(default)]
    pub(crate) head_hash_hex: Option<String>,
    #[serde(default)]
    pub(crate) committed_state_root_hex: Option<String>,
    #[serde(default)]
    pub(crate) last_committed_round: Option<u64>,
    #[serde(default)]
    pub(crate) peer_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportSessionHeartbeatResponse {
    pub(crate) alive: bool,
    pub(crate) session_id: u64,
    pub(crate) chain_id: Option<u64>,
    pub(crate) validator_id_hex: Option<String>,
    #[serde(default)]
    pub(crate) current_round: Option<u64>,
    #[serde(default)]
    pub(crate) current_view: Option<u64>,
    #[serde(default)]
    pub(crate) head_height: Option<u64>,
    #[serde(default)]
    pub(crate) head_hash_hex: Option<String>,
    #[serde(default)]
    pub(crate) committed_state_root_hex: Option<String>,
    #[serde(default)]
    pub(crate) last_committed_round: Option<u64>,
    #[serde(default)]
    pub(crate) peer_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NetworkPeerReachability {
    pub(crate) reachable_peer_count: usize,
    pub(crate) unreachable_peer_urls: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PeerTransportSessionSnapshot {
    pub(crate) endpoint: String,
    pub(crate) age_ms: u64,
    pub(crate) idle_ms: u64,
    pub(crate) request_count: u64,
    pub(crate) remote_session_id: u64,
    pub(crate) remote_chain_id: u64,
    pub(crate) remote_validator_id_hex: Option<String>,
    pub(crate) remote_current_round: Option<u64>,
    pub(crate) remote_current_view: Option<u64>,
    pub(crate) remote_head_height: Option<u64>,
    pub(crate) remote_head_hash_hex: Option<String>,
    pub(crate) remote_committed_state_root_hex: Option<String>,
    pub(crate) remote_last_committed_round: Option<u64>,
    pub(crate) remote_peer_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PeerTransportEndpointBackoffSnapshot {
    pub(crate) endpoint: String,
    pub(crate) consecutive_failures: u32,
    pub(crate) retry_in_ms: u64,
    pub(crate) last_error: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InboundPeerTransportSessionSnapshot {
    pub(crate) session_id: u64,
    pub(crate) remote_addr: Option<String>,
    pub(crate) age_ms: u64,
    pub(crate) idle_ms: u64,
    pub(crate) request_count: u64,
    pub(crate) remote_chain_id: Option<u64>,
    pub(crate) remote_validator_id_hex: Option<String>,
    pub(crate) remote_current_round: Option<u64>,
    pub(crate) remote_current_view: Option<u64>,
    pub(crate) remote_head_height: Option<u64>,
    pub(crate) remote_head_hash_hex: Option<String>,
    pub(crate) remote_committed_state_root_hex: Option<String>,
    pub(crate) remote_last_committed_round: Option<u64>,
    pub(crate) remote_peer_count: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NetworkRequestError {
    Unreachable(String),
    Rejected(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportRequest {
    pub(crate) path: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PeerTransportResponse {
    pub(crate) body: Option<String>,
    pub(crate) error: Option<JsonRpcErrorObject>,
}
