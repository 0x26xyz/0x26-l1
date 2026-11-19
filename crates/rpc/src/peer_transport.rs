use crate::response::write_http_response;
use crate::{JsonRpcErrorObject, PeerTransportRequest, PeerTransportResponse, TransportError};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub(crate) const MAX_PEER_TRANSPORT_FRAME_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const PEER_TRANSPORT_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const PEER_TRANSPORT_SESSION_STALE_TIMEOUT: Duration = Duration::from_secs(45);
pub(crate) const PEER_TRANSPORT_RECONNECT_BACKOFF_BASE: Duration = Duration::from_millis(250);
pub(crate) const PEER_TRANSPORT_RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(5);
pub(crate) const PEER_TRANSPORT_MAGIC: &[u8; 8] = b"KMBTPR1\n";

#[derive(Debug)]
pub(crate) struct PeerTransportSession {
    pub(crate) stream: TcpStream,
    pub(crate) connected_at: Instant,
    pub(crate) last_used_at: Instant,
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

#[derive(Clone, Debug, Default)]
pub(crate) struct PeerTransportSessionMetadata {
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

#[derive(Debug)]
pub(crate) struct ActivePeerTransportSession {
    pub(crate) connected_at: Instant,
    pub(crate) last_used_at: Instant,
    pub(crate) request_count: u64,
    pub(crate) remote_addr: Option<String>,
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

#[derive(Debug)]
pub(crate) struct PeerTransportEndpointState {
    pub(crate) consecutive_failures: u32,
    pub(crate) last_error: String,
    pub(crate) last_failure_at: Instant,
    pub(crate) next_retry_at: Instant,
}

pub(crate) fn read_peer_transport_request(
    stream: &mut TcpStream,
) -> Result<PeerTransportRequest, TransportError> {
    let mut length_bytes = [0_u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .map_err(TransportError::Read)?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length > MAX_PEER_TRANSPORT_FRAME_BYTES {
        return Err(TransportError::RequestTooLarge);
    }
    let mut payload = vec![0_u8; length];
    stream
        .read_exact(&mut payload)
        .map_err(TransportError::Read)?;
    serde_json::from_slice(&payload).map_err(|error| {
        TransportError::Read(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid peer transport request: {error}"),
        ))
    })
}

pub(crate) fn write_peer_transport_request(
    stream: &mut TcpStream,
    request: &PeerTransportRequest,
) -> Result<(), TransportError> {
    let payload = serde_json::to_vec(request).map_err(|error| {
        TransportError::Write(std::io::Error::other(format!(
            "peer transport request serialization failed: {error}"
        )))
    })?;
    if payload.len() > MAX_PEER_TRANSPORT_FRAME_BYTES {
        return Err(TransportError::RequestTooLarge);
    }
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .map_err(TransportError::Write)?;
    stream.write_all(&payload).map_err(TransportError::Write)?;
    stream.flush().map_err(TransportError::Write)
}

pub(crate) fn read_peer_transport_response(
    stream: &mut TcpStream,
) -> Result<PeerTransportResponse, TransportError> {
    let mut length_bytes = [0_u8; 4];
    stream
        .read_exact(&mut length_bytes)
        .map_err(TransportError::Read)?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length > MAX_PEER_TRANSPORT_FRAME_BYTES {
        return Err(TransportError::RequestTooLarge);
    }
    let mut payload = vec![0_u8; length];
    stream
        .read_exact(&mut payload)
        .map_err(TransportError::Read)?;
    serde_json::from_slice(&payload).map_err(|error| {
        TransportError::Read(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid peer transport response: {error}"),
        ))
    })
}

pub(crate) fn write_peer_transport_response(
    stream: &mut TcpStream,
    response: &PeerTransportResponse,
) -> Result<(), TransportError> {
    let payload = serde_json::to_vec(response).map_err(|error| {
        TransportError::Write(std::io::Error::other(format!(
            "peer transport response serialization failed: {error}"
        )))
    })?;
    if payload.len() > MAX_PEER_TRANSPORT_FRAME_BYTES {
        return Err(TransportError::RequestTooLarge);
    }
    stream
        .write_all(&(payload.len() as u32).to_be_bytes())
        .map_err(TransportError::Write)?;
    stream.write_all(&payload).map_err(TransportError::Write)?;
    stream.flush().map_err(TransportError::Write)
}

pub(crate) fn write_network_handler_error(
    stream: &mut TcpStream,
    error: TransportError,
) -> Result<(), TransportError> {
    match error {
        TransportError::Read(error) => {
            write_http_response(stream, "400 Bad Request", "text/plain", &error.to_string())
        }
        TransportError::RequestTooLarge => write_http_response(
            stream,
            "413 Payload Too Large",
            "text/plain",
            "request too large",
        ),
        TransportError::Write(error) => Err(TransportError::Write(error)),
        TransportError::Bind(error) => Err(TransportError::Bind(error)),
        TransportError::Accept(error) => Err(TransportError::Accept(error)),
    }
}

pub(crate) fn json_rpc_error_to_transport_error(error: JsonRpcErrorObject) -> TransportError {
    let kind = if error.code == -32602 {
        std::io::ErrorKind::InvalidInput
    } else {
        std::io::ErrorKind::Other
    };
    TransportError::Read(std::io::Error::new(kind, error.message))
}

pub(crate) fn peer_transport_response_from_result(
    result: Result<String, JsonRpcErrorObject>,
) -> PeerTransportResponse {
    match result {
        Ok(body) => PeerTransportResponse {
            body: Some(body),
            error: None,
        },
        Err(error) => PeerTransportResponse {
            body: None,
            error: Some(error),
        },
    }
}
