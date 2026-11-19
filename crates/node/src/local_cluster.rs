use crate::{
    block_hash, bootstrap_validator, build_block, build_genesis_block, build_quorum_certificate,
    select_leader, validate_block_against_parent, validate_block_against_parent_and_membership,
    validate_consensus_key_material, validator_verifiers, write_snapshot_file, BuildBlockRequest,
    ConsensusLogEntry, ConsensusMetricsSnapshot, ConsensusTelemetry, NodeError,
};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::{
    Block, BlockBody, BlockValidationError, Hash, PacemakerConfig, PacemakerState, ValidatorId,
    ValidatorVerifier,
};
use kombat_chain_network::{BootstrapPeers, ValidatorConfig, ValidatorPeer, ValidatorSet};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const LOCAL_CLUSTER_SIZE: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalClusterConfig {
    pub chain_id: u64,
    pub validator_set: ValidatorSet,
    pub consensus_signing_keys: BTreeMap<ValidatorId, [u8; 32]>,
    pub genesis_timestamp_ms: u64,
    pub genesis_state_root: Hash,
    pub pacemaker: PacemakerConfig,
}

#[derive(Clone, Debug)]
pub struct LocalCluster {
    chain_id: u64,
    validator_set: ValidatorSet,
    validator_verifiers: Vec<ValidatorVerifier>,
    pub(crate) consensus_signing_keys: BTreeMap<ValidatorId, [u8; 32]>,
    pacemaker_config: PacemakerConfig,
    pub(crate) nodes: Vec<LocalClusterNode>,
}

#[derive(Clone, Debug)]
pub struct LocalClusterNode {
    pub(crate) validator_id: ValidatorId,
    pub(crate) bootstrap_peers: BootstrapPeers,
    pub(crate) pacemaker: PacemakerState,
    pub(crate) chain: Vec<Block>,
    pub(crate) telemetry: ConsensusTelemetry,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) struct LocalClusterNodeSnapshot {
    pub(crate) validator_id: ValidatorId,
    pub(crate) bootstrap_peers: BootstrapPeers,
    pub(crate) pacemaker: PacemakerState,
    pub(crate) chain: Vec<Block>,
    pub(crate) telemetry: ConsensusTelemetry,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub(crate) struct PersistedLocalClusterSnapshot {
    pub(crate) chain_id: u64,
    pub(crate) validator_set: ValidatorSet,
    pub(crate) validator_verifiers: Vec<ValidatorVerifier>,
    pub(crate) consensus_signing_keys: BTreeMap<ValidatorId, [u8; 32]>,
    pub(crate) pacemaker_config: PacemakerConfig,
    pub(crate) node_snapshots: Vec<Vec<u8>>,
}

impl LocalCluster {
    pub fn new(config: LocalClusterConfig) -> Result<Self, NodeError> {
        let active_validators = config.validator_set.active_validators();
        if active_validators.len() != LOCAL_CLUSTER_SIZE {
            return Err(NodeError::InvalidClusterSize {
                expected: LOCAL_CLUSTER_SIZE,
                actual: active_validators.len(),
            });
        }

        let genesis_leader = select_leader(0, config.chain_id, &active_validators)?;
        let genesis = build_genesis_block(
            config.chain_id,
            genesis_leader,
            config.genesis_state_root,
            config.genesis_timestamp_ms,
        );
        let validator_verifiers = validator_verifiers(&active_validators)?;
        let genesis_qc = build_quorum_certificate(&genesis, &config.consensus_signing_keys)?;

        let mut nodes = Vec::with_capacity(active_validators.len());
        for validator in active_validators {
            let bootstrap_peers =
                bootstrap_validator(&config.validator_set, validator.validator_id)
                    .map_err(NodeError::BootstrapConfig)?;
            let pacemaker = PacemakerState::new(1, genesis_qc.clone());
            let telemetry = ConsensusTelemetry::new(
                bootstrap_peers.peers.len(),
                &pacemaker,
                config.genesis_timestamp_ms,
            );
            nodes.push(LocalClusterNode {
                validator_id: validator.validator_id,
                bootstrap_peers,
                pacemaker,
                chain: vec![genesis.clone()],
                telemetry,
            });
        }

        Ok(Self {
            chain_id: config.chain_id,
            validator_set: config.validator_set,
            validator_verifiers,
            consensus_signing_keys: config.consensus_signing_keys,
            pacemaker_config: config.pacemaker,
            nodes,
        })
    }

    pub fn nodes(&self) -> &[LocalClusterNode] {
        &self.nodes
    }

    pub fn head(&self) -> &Block {
        self.nodes
            .first()
            .expect("local cluster must have nodes")
            .head()
    }

    pub fn leader_for_round(&self, round: u64) -> Result<ValidatorId, NodeError> {
        select_leader(
            round,
            self.chain_id,
            &self.validator_set.active_validators(),
        )
    }

    pub fn produce_empty_block(&mut self, timestamp_ms: u64) -> Result<&Block, NodeError> {
        let parent_state_root = self.head().header.state_root;
        self.produce_block(
            BlockBody { items: vec![] },
            parent_state_root,
            [0; 32],
            timestamp_ms,
        )
    }

    pub fn produce_block(
        &mut self,
        body: BlockBody,
        state_root: Hash,
        event_root: Hash,
        timestamp_ms: u64,
    ) -> Result<&Block, NodeError> {
        let active_validators = self.validator_set.active_validators();
        let parent = self.head().clone();
        let round = self
            .nodes
            .first()
            .expect("local cluster must have nodes")
            .current_round()
            .max(parent.header.round.saturating_add(1));
        let leader = select_leader(round, self.chain_id, &active_validators)?;
        let block = build_block(BuildBlockRequest {
            chain_id: self.chain_id,
            proposer: leader,
            parent: &parent,
            validator_verifiers: &self.validator_verifiers,
            consensus_signing_keys: &self.consensus_signing_keys,
            body,
            state_root,
            event_root,
            timestamp_ms,
            round,
            max_clock_skew_ms: self.pacemaker_config.max_clock_skew_ms,
        })?;
        let produced_qc = build_quorum_certificate(&block, &self.consensus_signing_keys)?;
        let canonical_chain = self
            .nodes
            .first()
            .expect("local cluster must have nodes")
            .chain
            .clone();
        let sync_context = ClusterSyncContext {
            validator_verifiers: &self.validator_verifiers,
            active_validators: &active_validators,
            consensus_signing_keys: &self.consensus_signing_keys,
            chain_id: self.chain_id,
            max_clock_skew_ms: self.pacemaker_config.max_clock_skew_ms,
        };

        for node in &mut self.nodes {
            sync_cluster_node_to_parent(
                node,
                &canonical_chain,
                &parent,
                timestamp_ms,
                &sync_context,
            )?;
            validate_block_against_parent_and_membership(
                &block,
                &node.head().header,
                &self.validator_verifiers,
                leader,
                timestamp_ms,
                self.pacemaker_config.max_clock_skew_ms,
            )
            .map_err(NodeError::InvalidBlock)?;
            node.pacemaker.advance_on_qc(produced_qc.clone());
            node.telemetry.on_block_produced(
                &block,
                &node.pacemaker,
                Some(node.validator_id),
                timestamp_ms,
            );
            node.telemetry
                .on_round_started(&node.pacemaker, Some(node.validator_id), timestamp_ms);
            node.chain.push(block.clone());
        }

        Ok(self.head())
    }

    pub(crate) fn snapshot(&self) -> PersistedLocalClusterSnapshot {
        PersistedLocalClusterSnapshot {
            chain_id: self.chain_id,
            validator_set: self.validator_set.clone(),
            validator_verifiers: self.validator_verifiers.clone(),
            consensus_signing_keys: self.consensus_signing_keys.clone(),
            pacemaker_config: self.pacemaker_config.clone(),
            node_snapshots: self
                .nodes
                .iter()
                .map(LocalClusterNode::snapshot_bytes)
                .collect(),
        }
    }

    fn try_from_snapshot(snapshot: PersistedLocalClusterSnapshot) -> Result<Self, std::io::Error> {
        let active_validators = snapshot.validator_set.active_validators();
        let expected_validator_ids = active_validators
            .iter()
            .map(|validator| validator.validator_id)
            .collect::<BTreeSet<_>>();
        let expected_validators = active_validators
            .iter()
            .map(|validator| (validator.validator_id, validator.clone()))
            .collect::<BTreeMap<_, _>>();

        if expected_validator_ids.len() != LOCAL_CLUSTER_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "persisted cluster snapshot requires {LOCAL_CLUSTER_SIZE} active validators"
                ),
            ));
        }
        if snapshot.node_snapshots.len() != expected_validator_ids.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted cluster snapshot node count mismatch",
            ));
        }
        if snapshot.validator_verifiers.len() != expected_validator_ids.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted cluster snapshot verifier count mismatch",
            ));
        }
        let expected_verifiers = validator_verifiers(&active_validators).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("persisted cluster snapshot verifier construction failed: {error:?}"),
            )
        })?;
        if snapshot.validator_verifiers != expected_verifiers {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted cluster snapshot verifier config mismatch",
            ));
        }

        let signing_key_ids = snapshot
            .consensus_signing_keys
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        if signing_key_ids != expected_validator_ids {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted cluster snapshot signing keys do not match active validators",
            ));
        }
        validate_consensus_key_material(
            &snapshot.validator_verifiers,
            &snapshot.consensus_signing_keys,
            "persisted cluster snapshot",
        )?;

        let mut nodes = Vec::with_capacity(snapshot.node_snapshots.len());
        let mut node_ids = BTreeSet::new();
        for bytes in &snapshot.node_snapshots {
            let node = LocalClusterNode::from_snapshot_bytes(bytes)?;
            if node.head().header.chain_id != snapshot.chain_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot node chain id mismatch",
                ));
            }
            if node.bootstrap_peers.local.validator_id != node.validator_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot local validator mismatch",
                ));
            }
            if !expected_validator_ids.contains(&node.validator_id) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot contains unknown validator id",
                ));
            }
            let expected_local = expected_validators
                .get(&node.validator_id)
                .expect("validated above");
            if &node.bootstrap_peers.local != expected_local {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot local validator config mismatch",
                ));
            }
            if !node_ids.insert(node.validator_id) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot contains duplicate validator id",
                ));
            }
            let actual_peer_ids = node
                .bootstrap_peers
                .peers
                .iter()
                .map(|peer| peer.validator_id)
                .collect::<BTreeSet<_>>();
            let expected_peer_ids = expected_validator_ids
                .iter()
                .copied()
                .filter(|validator_id| *validator_id != node.validator_id)
                .collect::<BTreeSet<_>>();
            if actual_peer_ids != expected_peer_ids {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot peer set mismatch",
                ));
            }
            let actual_peers = node
                .bootstrap_peers
                .peers
                .iter()
                .cloned()
                .map(|peer| (peer.validator_id, peer))
                .collect::<BTreeMap<_, _>>();
            let expected_peers = expected_validators
                .iter()
                .filter_map(|(validator_id, validator)| {
                    (*validator_id != node.validator_id)
                        .then_some((*validator_id, ValidatorPeer::from(validator)))
                })
                .collect::<BTreeMap<_, _>>();
            if actual_peers != expected_peers {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted cluster snapshot peer config mismatch",
                ));
            }
            for window in node.chain.windows(2) {
                let expected_proposer = select_leader(
                    window[1].header.round,
                    snapshot.chain_id,
                    &active_validators,
                )
                .map_err(|error| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("persisted cluster snapshot leader selection failed: {error:?}"),
                    )
                })?;
                validate_block_against_parent_and_membership(
                    &window[1],
                    &window[0].header,
                    &snapshot.validator_verifiers,
                    expected_proposer,
                    window[1].header.timestamp_ms,
                    snapshot.pacemaker_config.max_clock_skew_ms,
                )
                .map_err(|error| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("persisted cluster snapshot block validation failed: {error:?}"),
                    )
                })?;
            }
            nodes.push(node);
        }
        if node_ids != expected_validator_ids {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted cluster snapshot node validator set mismatch",
            ));
        }
        let mut canonical_by_height = BTreeMap::new();
        for node in &nodes {
            for (height, block) in node.chain.iter().enumerate() {
                let hash = block_hash(block);
                match canonical_by_height.get(&height) {
                    Some(expected_hash) if *expected_hash != hash => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "persisted cluster snapshot node prefix mismatch",
                        ));
                    }
                    Some(_) => {}
                    None => {
                        canonical_by_height.insert(height, hash);
                    }
                }
            }
        }

        Ok(Self {
            chain_id: snapshot.chain_id,
            validator_set: snapshot.validator_set,
            validator_verifiers: snapshot.validator_verifiers,
            consensus_signing_keys: snapshot.consensus_signing_keys,
            pacemaker_config: snapshot.pacemaker_config,
            nodes,
        })
    }

    pub fn snapshot_bytes(&self) -> Vec<u8> {
        borsh::to_vec(&self.snapshot()).expect("local cluster snapshot should serialize")
    }

    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        let snapshot = PersistedLocalClusterSnapshot::try_from_slice(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        Self::try_from_snapshot(snapshot)
    }

    pub fn save_snapshot_file(&self, path: impl AsRef<Path>) -> Result<(), std::io::Error> {
        write_snapshot_file(path, &self.snapshot_bytes())
    }

    pub fn load_snapshot_file(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let bytes = fs::read(path)?;
        Self::from_snapshot_bytes(&bytes)
    }
}

struct ClusterSyncContext<'a> {
    validator_verifiers: &'a [ValidatorVerifier],
    active_validators: &'a [ValidatorConfig],
    consensus_signing_keys: &'a BTreeMap<ValidatorId, [u8; 32]>,
    chain_id: u64,
    max_clock_skew_ms: u64,
}

fn sync_cluster_node_to_parent(
    node: &mut LocalClusterNode,
    canonical_chain: &[Block],
    parent: &Block,
    timestamp_ms: u64,
    context: &ClusterSyncContext<'_>,
) -> Result<(), NodeError> {
    if node.head().header.height > parent.header.height {
        return Err(NodeError::InvalidBlock(BlockValidationError::InvalidHeight));
    }

    while node.head().header.height < parent.header.height {
        let next_height = node.head().header.height.saturating_add(1);
        let canonical_block = canonical_chain
            .get(next_height as usize)
            .ok_or(NodeError::InvalidBlock(BlockValidationError::InvalidHeight))?;
        let leader = select_leader(
            canonical_block.header.round,
            context.chain_id,
            context.active_validators,
        )?;
        validate_block_against_parent_and_membership(
            canonical_block,
            &node.head().header,
            context.validator_verifiers,
            leader,
            timestamp_ms,
            context.max_clock_skew_ms,
        )
        .map_err(NodeError::InvalidBlock)?;
        let produced_qc =
            build_quorum_certificate(canonical_block, context.consensus_signing_keys)?;
        node.pacemaker.advance_on_qc(produced_qc);
        node.telemetry.on_block_produced(
            canonical_block,
            &node.pacemaker,
            Some(node.validator_id),
            canonical_block.header.timestamp_ms,
        );
        node.telemetry.on_round_started(
            &node.pacemaker,
            Some(node.validator_id),
            canonical_block.header.timestamp_ms,
        );
        node.chain.push(canonical_block.clone());
    }

    if block_hash(node.head()) != block_hash(parent) {
        return Err(NodeError::InvalidBlock(
            BlockValidationError::InvalidParentHash,
        ));
    }

    Ok(())
}

impl LocalClusterNode {
    pub(crate) fn snapshot(&self) -> LocalClusterNodeSnapshot {
        LocalClusterNodeSnapshot {
            validator_id: self.validator_id,
            bootstrap_peers: self.bootstrap_peers.clone(),
            pacemaker: self.pacemaker.clone(),
            chain: self.chain.clone(),
            telemetry: self.telemetry.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot(snapshot: LocalClusterNodeSnapshot) -> Self {
        Self::try_from_snapshot(snapshot)
            .expect("persisted local cluster node snapshot should validate")
    }

    pub(crate) fn try_from_snapshot(
        mut snapshot: LocalClusterNodeSnapshot,
    ) -> Result<Self, std::io::Error> {
        if snapshot.chain.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted local cluster node snapshot requires a genesis block",
            ));
        }
        if snapshot.bootstrap_peers.local.validator_id != snapshot.validator_id {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted local cluster node snapshot local validator mismatch",
            ));
        }
        if snapshot
            .bootstrap_peers
            .peers
            .iter()
            .any(|peer| peer.validator_id == snapshot.validator_id)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted local cluster node snapshot peers contain local validator",
            ));
        }

        let chain_id = snapshot.chain[0].header.chain_id;
        for block in &snapshot.chain {
            if block.header.chain_id != chain_id {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "persisted local cluster node snapshot chain id mismatch",
                ));
            }
        }
        for window in snapshot.chain.windows(2) {
            validate_block_against_parent(
                &window[1],
                &window[0].header,
                window[1].header.proposer,
                window[1].header.timestamp_ms,
                0,
            )
            .map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("persisted local cluster node block validation failed: {error:?}"),
                )
            })?;
        }

        let head = snapshot.chain.last().expect("checked above");
        if snapshot.pacemaker.highest_qc.block_hash != block_hash(head)
            || snapshot.pacemaker.highest_qc.view != head.header.round
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted local cluster node snapshot highest qc mismatch",
            ));
        }
        if snapshot.pacemaker.current_round < head.header.round.saturating_add(1) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persisted local cluster node snapshot round regression",
            ));
        }
        snapshot.telemetry.normalize_snapshot_timestamps();
        snapshot.telemetry.validate_snapshot(
            &snapshot.pacemaker,
            head,
            snapshot.bootstrap_peers.peers.len(),
            Some(snapshot.validator_id),
        )?;

        Ok(Self {
            validator_id: snapshot.validator_id,
            bootstrap_peers: snapshot.bootstrap_peers,
            pacemaker: snapshot.pacemaker,
            chain: snapshot.chain,
            telemetry: snapshot.telemetry,
        })
    }

    pub(crate) fn snapshot_bytes(&self) -> Vec<u8> {
        borsh::to_vec(&self.snapshot()).expect("local cluster node snapshot should serialize")
    }

    pub(crate) fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        let snapshot = LocalClusterNodeSnapshot::try_from_slice(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        Self::try_from_snapshot(snapshot)
    }

    pub fn validator_id(&self) -> ValidatorId {
        self.validator_id
    }

    pub fn bootstrap_peers(&self) -> &BootstrapPeers {
        &self.bootstrap_peers
    }

    pub fn head(&self) -> &Block {
        self.chain
            .last()
            .expect("local cluster node must have genesis")
    }

    pub fn current_round(&self) -> u64 {
        self.pacemaker.current_round
    }

    pub fn block_count(&self) -> usize {
        self.chain.len()
    }

    pub fn metrics(&self) -> &ConsensusMetricsSnapshot {
        self.telemetry.metrics()
    }

    pub fn logs(&self) -> &[ConsensusLogEntry] {
        self.telemetry.logs()
    }
}
