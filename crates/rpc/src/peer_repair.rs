use crate::{PeerTransportSessionHeartbeatResponse, PeerTransportSessionMetadata};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

pub(crate) const PEER_SNAPSHOT_REPAIR_MIN_HEAD_GAP: u64 = 1;
pub(crate) const PEER_SNAPSHOT_REPAIR_FORCE_HEAD_GAP: u64 = 16;
pub(crate) const PEER_SNAPSHOT_REPAIR_COOLDOWN: Duration = Duration::from_secs(15);
pub(crate) const PEER_SNAPSHOT_OBSERVER_REPAIR_COOLDOWN: Duration = Duration::from_secs(3);
pub(crate) const PEER_SNAPSHOT_REPAIR_STALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub(crate) struct PeerSnapshotRepairCandidate {
    pub(crate) endpoints: Vec<String>,
    pub(crate) remote_validator_id_hex: Option<String>,
    pub(crate) remote_head_height: u64,
    pub(crate) remote_head_hash_hex: Option<String>,
    pub(crate) remote_committed_state_root_hex: Option<String>,
    pub(crate) authoritative_same_height: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PeerSnapshotRepairState {
    pub(crate) next_attempt_at: Option<Instant>,
    pub(crate) stalled_head_height: Option<u64>,
    pub(crate) stalled_head_hash_hex: Option<String>,
    pub(crate) stalled_since: Option<Instant>,
    pub(crate) attempts_total: u64,
    pub(crate) repaired_total: u64,
    pub(crate) repair_err_total: u64,
    pub(crate) last_error: Option<String>,
    pub(crate) last_repaired_head_height: Option<u64>,
    pub(crate) last_repaired_head_hash_hex: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PeerSnapshotRepairStateSnapshot {
    pub(crate) stalled_head_height: Option<u64>,
    pub(crate) stalled_head_hash_hex: Option<String>,
    pub(crate) stalled_for_ms: Option<u64>,
    pub(crate) next_retry_in_ms: Option<u64>,
    pub(crate) attempts_total: u64,
    pub(crate) repaired_total: u64,
    pub(crate) repair_err_total: u64,
    pub(crate) last_error: Option<String>,
    pub(crate) last_repaired_head_height: Option<u64>,
    pub(crate) last_repaired_head_hash_hex: Option<String>,
}

pub(crate) fn peer_snapshot_repair_candidate(
    local_metadata: &PeerTransportSessionMetadata,
    endpoints: &[String],
    heartbeat: &PeerTransportSessionHeartbeatResponse,
) -> Option<PeerSnapshotRepairCandidate> {
    let local_head_height = local_metadata.remote_head_height?;
    let remote_head_height = heartbeat.head_height?;
    let local_head_hash_hex = local_metadata.remote_head_hash_hex.as_deref()?;
    let remote_head_hash_hex = heartbeat.head_hash_hex.as_deref()?;
    if remote_head_hash_hex == local_head_hash_hex {
        return None;
    }
    if remote_head_height < local_head_height {
        return None;
    }

    Some(PeerSnapshotRepairCandidate {
        endpoints: endpoints.to_vec(),
        remote_validator_id_hex: heartbeat.validator_id_hex.clone(),
        remote_head_height,
        remote_head_hash_hex: heartbeat.head_hash_hex.clone(),
        remote_committed_state_root_hex: heartbeat.committed_state_root_hex.clone(),
        authoritative_same_height: false,
    })
}

pub(crate) fn peer_snapshot_repair_authoritative_same_height_candidate(
    local_metadata: &PeerTransportSessionMetadata,
    observations: &[PeerSnapshotRepairCandidate],
    validator_count: usize,
) -> Option<PeerSnapshotRepairCandidate> {
    let local_head_height = local_metadata.remote_head_height?;
    let local_head_hash_hex = local_metadata.remote_head_hash_hex.as_deref()?;
    let local_committed_state_root_hex =
        local_metadata.remote_committed_state_root_hex.as_deref()?;
    let majority_threshold = validator_count / 2 + 1;
    if majority_threshold <= 1 {
        return None;
    }

    let mut grouped = BTreeMap::<(u64, String, String), BTreeSet<String>>::new();
    let mut first_candidate = BTreeMap::<(u64, String, String), PeerSnapshotRepairCandidate>::new();
    let mut same_root_validator_ids = BTreeSet::<String>::new();
    for observation in observations {
        if observation.remote_head_height < local_head_height {
            continue;
        }
        let Some(remote_head_hash_hex) = observation.remote_head_hash_hex.as_deref() else {
            continue;
        };
        let Some(remote_committed_state_root_hex) =
            observation.remote_committed_state_root_hex.as_deref()
        else {
            continue;
        };
        if remote_head_hash_hex == local_head_hash_hex {
            continue;
        }
        if remote_committed_state_root_hex != local_committed_state_root_hex {
            continue;
        }
        let Some(remote_validator_id_hex) = observation.remote_validator_id_hex.as_deref() else {
            continue;
        };
        if observation.remote_head_height == local_head_height {
            same_root_validator_ids.insert(remote_validator_id_hex.to_owned());
        }
        let key = (
            observation.remote_head_height,
            remote_head_hash_hex.to_owned(),
            remote_committed_state_root_hex.to_owned(),
        );
        grouped
            .entry(key.clone())
            .or_default()
            .insert(remote_validator_id_hex.to_owned());
        first_candidate
            .entry(key)
            .or_insert_with(|| observation.clone());
    }

    if let Some((_, candidate)) = grouped
        .into_iter()
        .filter_map(|(key, validators)| {
            let count = validators.len();
            let mut candidate = first_candidate.get(&key)?.clone();
            candidate.authoritative_same_height = count >= majority_threshold;
            candidate
                .authoritative_same_height
                .then_some((count, candidate))
        })
        .max_by(|(left_count, left), (right_count, right)| {
            left_count
                .cmp(right_count)
                .then_with(|| left.remote_head_hash_hex.cmp(&right.remote_head_hash_hex))
        })
    {
        return Some(candidate);
    }

    let required_remote_validators = if local_metadata.remote_validator_id_hex.is_some() {
        validator_count.saturating_sub(1)
    } else {
        validator_count
    };
    if same_root_validator_ids.len() < required_remote_validators {
        return None;
    }

    first_candidate
        .into_values()
        .filter(|candidate| {
            candidate
                .remote_head_hash_hex
                .as_deref()
                .is_some_and(|remote_hash| remote_hash > local_head_hash_hex)
        })
        .max_by(|left, right| left.remote_head_hash_hex.cmp(&right.remote_head_hash_hex))
        .map(|mut candidate| {
            candidate.authoritative_same_height = true;
            candidate
        })
}

pub(crate) fn peer_snapshot_repair_candidate_is_better(
    candidate: &PeerSnapshotRepairCandidate,
    current: &PeerSnapshotRepairCandidate,
) -> bool {
    candidate.remote_head_height > current.remote_head_height
        || (candidate.remote_head_height == current.remote_head_height
            && candidate.authoritative_same_height
            && !current.authoritative_same_height)
        || (candidate.remote_head_height == current.remote_head_height
            && candidate.authoritative_same_height == current.authoritative_same_height
            && candidate
                .remote_head_hash_hex
                .as_deref()
                .unwrap_or_default()
                > current.remote_head_hash_hex.as_deref().unwrap_or_default())
}
