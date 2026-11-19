use borsh::to_vec;
use kombat_chain_consensus::{Block, PacemakerState, ValidatorId};
use std::io;

#[derive(Clone, Debug, PartialEq, Eq, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ConsensusMetricsSnapshot {
    pub current_round: u64,
    pub current_view: u64,
    pub last_committed_round: u64,
    pub timeouts_total: u64,
    pub view_changes_total: u64,
    pub phase6_resting_batch_attempts: u64,
    pub phase6_resting_batch_accepted: u64,
    pub phase6_resting_batch_items: u64,
    pub phase6_resting_batch_unsupported: u64,
    pub phase6_resting_batch_rejected: u64,
    pub phase6_resting_batch_gather_ns: u64,
    pub phase6_resting_batch_precheck_ns: u64,
    pub phase6_resting_batch_admission_ns: u64,
    pub phase6_resting_batch_policy_ns: u64,
    pub phase6_resting_batch_matching_ns: u64,
    pub phase6_resting_batch_event_ns: u64,
    pub qc_formation_ms: Vec<u64>,
    pub peer_count: usize,
    pub round_duration_ms: Vec<u64>,
    pub block_size_bytes: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum ConsensusLogEventKind {
    PeerCountUpdated,
    RoundStarted,
    BlockProposed,
    BlockCommitted,
    TimeoutObserved,
    ViewChanged,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ConsensusLogEntry {
    pub kind: ConsensusLogEventKind,
    pub round: u64,
    pub height: u64,
    pub validator_id: Option<ValidatorId>,
    pub timestamp_ms: u64,
    pub peer_count: usize,
}

#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ConsensusTelemetry {
    metrics: ConsensusMetricsSnapshot,
    logs: Vec<ConsensusLogEntry>,
    current_round_started_at_ms: u64,
}

impl ConsensusTelemetry {
    pub fn new(peer_count: usize, pacemaker: &PacemakerState, timestamp_ms: u64) -> Self {
        let mut telemetry = Self {
            metrics: ConsensusMetricsSnapshot {
                current_round: pacemaker.current_round,
                current_view: pacemaker.highest_qc.view,
                last_committed_round: 0,
                timeouts_total: 0,
                view_changes_total: 0,
                phase6_resting_batch_attempts: 0,
                phase6_resting_batch_accepted: 0,
                phase6_resting_batch_items: 0,
                phase6_resting_batch_unsupported: 0,
                phase6_resting_batch_rejected: 0,
                phase6_resting_batch_gather_ns: 0,
                phase6_resting_batch_precheck_ns: 0,
                phase6_resting_batch_admission_ns: 0,
                phase6_resting_batch_policy_ns: 0,
                phase6_resting_batch_matching_ns: 0,
                phase6_resting_batch_event_ns: 0,
                qc_formation_ms: vec![],
                peer_count,
                round_duration_ms: vec![],
                block_size_bytes: vec![],
            },
            logs: vec![],
            current_round_started_at_ms: timestamp_ms,
        };
        telemetry.push_log(
            ConsensusLogEventKind::PeerCountUpdated,
            pacemaker.current_round,
            0,
            None,
            timestamp_ms,
        );
        telemetry
    }

    pub fn metrics(&self) -> &ConsensusMetricsSnapshot {
        &self.metrics
    }

    pub fn logs(&self) -> &[ConsensusLogEntry] {
        &self.logs
    }

    pub fn remap_local_validator_id(&mut self, validator_id: Option<ValidatorId>) {
        for entry in &mut self.logs {
            if entry.kind != ConsensusLogEventKind::PeerCountUpdated {
                entry.validator_id = validator_id;
            }
        }
    }

    pub fn rewrite_peer_count(&mut self, peer_count: usize) {
        self.metrics.peer_count = peer_count;
        for entry in &mut self.logs {
            entry.peer_count = peer_count;
        }
    }

    pub fn on_round_started(
        &mut self,
        pacemaker: &PacemakerState,
        validator_id: Option<ValidatorId>,
        timestamp_ms: u64,
    ) {
        self.current_round_started_at_ms = timestamp_ms;
        self.refresh_from_pacemaker(pacemaker);
        self.push_log(
            ConsensusLogEventKind::RoundStarted,
            pacemaker.current_round,
            0,
            validator_id,
            timestamp_ms,
        );
    }

    pub fn on_block_produced(
        &mut self,
        block: &Block,
        pacemaker: &PacemakerState,
        validator_id: Option<ValidatorId>,
        timestamp_ms: u64,
    ) {
        let round_duration = timestamp_ms.saturating_sub(self.current_round_started_at_ms);
        self.metrics.qc_formation_ms.push(round_duration);
        self.metrics.round_duration_ms.push(round_duration);
        self.metrics
            .block_size_bytes
            .push(serialized_block_size(block));
        self.metrics.last_committed_round = block.header.round.saturating_sub(1);
        self.refresh_from_pacemaker(pacemaker);
        self.push_log(
            ConsensusLogEventKind::BlockProposed,
            block.header.round,
            block.header.height,
            validator_id,
            timestamp_ms,
        );
        if block.header.round > 1 {
            self.push_log(
                ConsensusLogEventKind::BlockCommitted,
                block.header.round.saturating_sub(1),
                block.header.height.saturating_sub(1),
                validator_id,
                timestamp_ms,
            );
        }
    }

    pub fn on_timeout(
        &mut self,
        pacemaker: &PacemakerState,
        validator_id: Option<ValidatorId>,
        timestamp_ms: u64,
    ) {
        self.metrics.timeouts_total = self.metrics.timeouts_total.saturating_add(1);
        self.refresh_from_pacemaker(pacemaker);
        self.push_log(
            ConsensusLogEventKind::TimeoutObserved,
            pacemaker.current_round,
            0,
            validator_id,
            timestamp_ms,
        );
    }

    pub fn on_view_change(
        &mut self,
        pacemaker: &PacemakerState,
        validator_id: Option<ValidatorId>,
        timestamp_ms: u64,
    ) {
        self.metrics.view_changes_total = self.metrics.view_changes_total.saturating_add(1);
        self.refresh_from_pacemaker(pacemaker);
        self.push_log(
            ConsensusLogEventKind::ViewChanged,
            pacemaker.current_round,
            0,
            validator_id,
            timestamp_ms,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_phase6_resting_batch(
        &mut self,
        attempts: u64,
        accepted: u64,
        items: u64,
        unsupported: u64,
        rejected: u64,
        gather_ns: u64,
        precheck_ns: u64,
        admission_ns: u64,
        policy_ns: u64,
        matching_ns: u64,
        event_ns: u64,
    ) {
        self.metrics.phase6_resting_batch_attempts = self
            .metrics
            .phase6_resting_batch_attempts
            .saturating_add(attempts);
        self.metrics.phase6_resting_batch_accepted = self
            .metrics
            .phase6_resting_batch_accepted
            .saturating_add(accepted);
        self.metrics.phase6_resting_batch_items = self
            .metrics
            .phase6_resting_batch_items
            .saturating_add(items);
        self.metrics.phase6_resting_batch_unsupported = self
            .metrics
            .phase6_resting_batch_unsupported
            .saturating_add(unsupported);
        self.metrics.phase6_resting_batch_rejected = self
            .metrics
            .phase6_resting_batch_rejected
            .saturating_add(rejected);
        self.metrics.phase6_resting_batch_gather_ns = self
            .metrics
            .phase6_resting_batch_gather_ns
            .saturating_add(gather_ns);
        self.metrics.phase6_resting_batch_precheck_ns = self
            .metrics
            .phase6_resting_batch_precheck_ns
            .saturating_add(precheck_ns);
        self.metrics.phase6_resting_batch_admission_ns = self
            .metrics
            .phase6_resting_batch_admission_ns
            .saturating_add(admission_ns);
        self.metrics.phase6_resting_batch_policy_ns = self
            .metrics
            .phase6_resting_batch_policy_ns
            .saturating_add(policy_ns);
        self.metrics.phase6_resting_batch_matching_ns = self
            .metrics
            .phase6_resting_batch_matching_ns
            .saturating_add(matching_ns);
        self.metrics.phase6_resting_batch_event_ns = self
            .metrics
            .phase6_resting_batch_event_ns
            .saturating_add(event_ns);
    }

    pub fn validate_snapshot(
        &self,
        pacemaker: &PacemakerState,
        head: &Block,
        expected_peer_count: usize,
        expected_validator_id: Option<ValidatorId>,
    ) -> Result<(), io::Error> {
        if self.metrics.current_round != pacemaker.current_round {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry current round mismatch",
            ));
        }
        if self.metrics.current_view != pacemaker.highest_qc.view {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry current view mismatch",
            ));
        }
        if self.metrics.last_committed_round != head.header.round.saturating_sub(1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry last committed round mismatch",
            ));
        }
        if self.metrics.peer_count != expected_peer_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry peer count mismatch",
            ));
        }
        if self.logs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry requires a peer-count log",
            ));
        }
        if self.logs[0].kind != ConsensusLogEventKind::PeerCountUpdated {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry first log must be peer-count update",
            ));
        }
        let mut last_timestamp_ms = 0_u64;
        let mut timeout_logs = 0_u64;
        let mut view_change_logs = 0_u64;
        let mut block_proposed_logs = 0_usize;
        let mut block_committed_logs = 0_usize;
        for entry in &self.logs {
            if entry.peer_count != expected_peer_count {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "persisted telemetry log peer count mismatch",
                ));
            }
            if entry.timestamp_ms < last_timestamp_ms {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "persisted telemetry log timestamp regression",
                ));
            }
            last_timestamp_ms = entry.timestamp_ms;
            match entry.kind {
                ConsensusLogEventKind::PeerCountUpdated => {
                    if entry.validator_id.is_some() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "persisted telemetry peer-count log validator mismatch",
                        ));
                    }
                }
                ConsensusLogEventKind::RoundStarted => {
                    if entry.validator_id != expected_validator_id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "persisted telemetry log validator mismatch",
                        ));
                    }
                }
                ConsensusLogEventKind::BlockProposed => {
                    if entry.validator_id != expected_validator_id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "persisted telemetry log validator mismatch",
                        ));
                    }
                    block_proposed_logs = block_proposed_logs.saturating_add(1);
                }
                ConsensusLogEventKind::BlockCommitted => {
                    if entry.validator_id != expected_validator_id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "persisted telemetry log validator mismatch",
                        ));
                    }
                    block_committed_logs = block_committed_logs.saturating_add(1);
                }
                ConsensusLogEventKind::TimeoutObserved => {
                    if entry.validator_id != expected_validator_id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "persisted telemetry log validator mismatch",
                        ));
                    }
                    timeout_logs = timeout_logs.saturating_add(1);
                }
                ConsensusLogEventKind::ViewChanged => {
                    if entry.validator_id != expected_validator_id {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "persisted telemetry log validator mismatch",
                        ));
                    }
                    view_change_logs = view_change_logs.saturating_add(1);
                }
            }
        }
        if self.metrics.timeouts_total != timeout_logs {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry timeout count mismatch",
            ));
        }
        if self.metrics.view_changes_total != view_change_logs {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry view-change count mismatch",
            ));
        }
        if self.metrics.qc_formation_ms.len() != block_proposed_logs
            || self.metrics.round_duration_ms.len() != block_proposed_logs
            || self.metrics.block_size_bytes.len() != block_proposed_logs
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry block metric length mismatch",
            ));
        }
        if block_committed_logs != block_proposed_logs.saturating_sub(1) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "persisted telemetry committed log count mismatch",
            ));
        }

        Ok(())
    }

    pub fn normalize_snapshot_timestamps(&mut self) {
        let mut last_timestamp_ms = 0_u64;
        for entry in &mut self.logs {
            if entry.timestamp_ms < last_timestamp_ms {
                entry.timestamp_ms = last_timestamp_ms;
            } else {
                last_timestamp_ms = entry.timestamp_ms;
            }
        }
    }

    pub fn repair_synthetic_mahi_snapshot(
        &mut self,
        pacemaker: &PacemakerState,
        head: &Block,
        expected_peer_count: usize,
    ) {
        self.metrics.current_round = pacemaker.current_round;
        self.metrics.current_view = pacemaker.highest_qc.view;
        self.metrics.last_committed_round = head.header.round.saturating_sub(1);
        self.metrics.peer_count = expected_peer_count;
        for entry in &mut self.logs {
            entry.peer_count = expected_peer_count;
        }
    }

    fn refresh_from_pacemaker(&mut self, pacemaker: &PacemakerState) {
        self.metrics.current_round = pacemaker.current_round;
        self.metrics.current_view = pacemaker.highest_qc.view;
    }

    fn push_log(
        &mut self,
        kind: ConsensusLogEventKind,
        round: u64,
        height: u64,
        validator_id: Option<ValidatorId>,
        timestamp_ms: u64,
    ) {
        let timestamp_ms = self
            .logs
            .last()
            .map(|entry| entry.timestamp_ms.max(timestamp_ms))
            .unwrap_or(timestamp_ms);
        self.logs.push(ConsensusLogEntry {
            kind,
            round,
            height,
            validator_id,
            timestamp_ms,
            peer_count: self.metrics.peer_count,
        });
    }

    #[cfg(test)]
    pub(crate) fn metrics_mut(&mut self) -> &mut ConsensusMetricsSnapshot {
        &mut self.metrics
    }

    #[cfg(test)]
    pub(crate) fn logs_mut(&mut self) -> &mut Vec<ConsensusLogEntry> {
        &mut self.logs
    }
}

fn serialized_block_size(block: &Block) -> u64 {
    match to_vec(block) {
        Ok(bytes) => bytes.len() as u64,
        Err(_) => 0,
    }
}
