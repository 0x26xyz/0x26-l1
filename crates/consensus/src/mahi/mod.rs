use borsh::{to_vec, BorshDeserialize, BorshSerialize};
use ed25519_dalek::{Signature as Ed25519Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    quorum_threshold, BlockDelegation, BlockFaucet, BlockMarketMakerProgram, BlockOraclePrice,
    BlockRealCapitalGovernance, BlockTeeGovernance, ConsensusPubKey, Hash, SignatureBytes,
    SignedIntent, ValidatorId,
};

pub const DEFAULT_WAVE_LENGTH: u64 = 5;
pub const MINIMUM_WAVE_LENGTH: u64 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum CommitRule {
    #[default]
    Mahi,
    Mysticeti,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct BlockRef {
    pub round: u64,
    pub author: ValidatorId,
    pub hash: Hash,
}

#[derive(BorshSerialize)]
struct MahiBlockUnsigned<'a> {
    author: &'a ValidatorId,
    round: u64,
    epoch: u64,
    timestamp_ms: u64,
    ancestors: &'a [BlockRef],
    oracle_prices: &'a [BlockOraclePrice],
    intents: &'a [SignedIntent],
    faucets: &'a [BlockFaucet],
    delegations: &'a [BlockDelegation],
    market_maker_programs: &'a [BlockMarketMakerProgram],
    tee_governance: &'a [BlockTeeGovernance],
    real_capital_governance: &'a [BlockRealCapitalGovernance],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MahiBlock {
    pub author: ValidatorId,
    pub round: u64,
    pub epoch: u64,
    pub timestamp_ms: u64,
    pub ancestors: Vec<BlockRef>,
    pub oracle_prices: Vec<BlockOraclePrice>,
    pub intents: Vec<SignedIntent>,
    pub faucets: Vec<BlockFaucet>,
    pub delegations: Vec<BlockDelegation>,
    pub market_maker_programs: Vec<BlockMarketMakerProgram>,
    pub tee_governance: Vec<BlockTeeGovernance>,
    pub real_capital_governance: Vec<BlockRealCapitalGovernance>,
    pub author_signature: SignatureBytes,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum LeaderStatusWire {
    Commit(BlockRef),
    Skip { author: ValidatorId, round: u64 },
    Undecided { author: ValidatorId, round: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeaderStatus {
    Commit(BlockRef),
    Skip(ValidatorId, u64),
    Undecided(ValidatorId, u64),
}

impl LeaderStatus {
    pub fn round(&self) -> u64 {
        match self {
            Self::Commit(reference) => reference.round,
            Self::Skip(_, round) | Self::Undecided(_, round) => *round,
        }
    }

    pub fn author(&self) -> ValidatorId {
        match self {
            Self::Commit(reference) => reference.author,
            Self::Skip(author, _) | Self::Undecided(author, _) => *author,
        }
    }

    pub fn is_decided(&self) -> bool {
        !matches!(self, Self::Undecided(..))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedSubDag {
    pub anchor: BlockRef,
    pub blocks: Vec<MahiBlock>,
}

impl CommittedSubDag {
    pub fn new(anchor: BlockRef, blocks: Vec<MahiBlock>) -> Self {
        Self { anchor, blocks }
    }

    pub fn sort(&mut self) {
        self.blocks
            .sort_by_key(|block| (block.round, block.author, block.hash()));
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Linearizer {
    pub committed: BTreeSet<BlockRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCommitterOptions {
    pub wave_length: u64,
    pub leader_offset: u64,
    pub round_offset: u64,
    pub pipelined: bool,
    pub commit_rule: CommitRule,
}

impl Default for BaseCommitterOptions {
    fn default() -> Self {
        Self {
            wave_length: DEFAULT_WAVE_LENGTH,
            leader_offset: 0,
            round_offset: 0,
            pipelined: true,
            commit_rule: CommitRule::Mahi,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseCommitter {
    validators: Vec<ValidatorId>,
    dag: CommitDagView,
    options: BaseCommitterOptions,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniversalCommitter {
    dag: CommitDagView,
    committers: Vec<BaseCommitter>,
    previously_committed_leaders: BTreeMap<(ValidatorId, u64), LeaderStatus>,
    wave_length: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniversalCommitterBuilder {
    validators: Vec<ValidatorId>,
    dag: CommitDagView,
    wave_length: u64,
    number_of_leaders: usize,
    pipeline: bool,
    commit_rule: CommitRule,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct DagState {
    pub blocks: BTreeMap<BlockRef, MahiBlock>,
    pub by_round: BTreeMap<u64, BTreeMap<ValidatorId, BlockRef>>,
    pub pending_fetch: BTreeSet<BlockRef>,
    pub last_committed_round: u64,
    pub last_commit_leader: Option<BlockRef>,
    pub previously_committed_leaders: BTreeMap<(ValidatorId, u64), LeaderStatusWire>,
    pub committed_blocks: BTreeSet<BlockRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommitDagView {
    ancestors: BTreeMap<BlockRef, Vec<BlockRef>>,
    by_round: BTreeMap<u64, BTreeMap<ValidatorId, BlockRef>>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LeaderSchedule {
    validators: Vec<ValidatorId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ThresholdClock {
    round: u64,
    seen_authors: BTreeSet<ValidatorId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MahiBlockValidationError {
    InvalidValidatorKeyMaterial,
    InvalidSignature,
    InvalidAncestorOrder,
    MissingAncestorQuorum,
}

impl MahiBlock {
    fn unsigned(&self) -> MahiBlockUnsigned<'_> {
        MahiBlockUnsigned {
            author: &self.author,
            round: self.round,
            epoch: self.epoch,
            timestamp_ms: self.timestamp_ms,
            ancestors: &self.ancestors,
            oracle_prices: &self.oracle_prices,
            intents: &self.intents,
            faucets: &self.faucets,
            delegations: &self.delegations,
            market_maker_programs: &self.market_maker_programs,
            tee_governance: &self.tee_governance,
            real_capital_governance: &self.real_capital_governance,
        }
    }

    pub fn signing_bytes(&self) -> Vec<u8> {
        to_vec(&self.unsigned()).expect("mahi unsigned block serialization should succeed")
    }

    pub fn signed(
        author: ValidatorId,
        round: u64,
        epoch: u64,
        timestamp_ms: u64,
        ancestors: Vec<BlockRef>,
        intents: Vec<SignedIntent>,
        signing_key: [u8; 32],
    ) -> Self {
        Self::signed_with_payloads(
            author,
            round,
            epoch,
            timestamp_ms,
            ancestors,
            intents,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            signing_key,
        )
    }

    pub fn signed_with_delegations(
        author: ValidatorId,
        round: u64,
        epoch: u64,
        timestamp_ms: u64,
        ancestors: Vec<BlockRef>,
        intents: Vec<SignedIntent>,
        delegations: Vec<BlockDelegation>,
        signing_key: [u8; 32],
    ) -> Self {
        Self::signed_with_payloads(
            author,
            round,
            epoch,
            timestamp_ms,
            ancestors,
            intents,
            Vec::new(),
            delegations,
            Vec::new(),
            signing_key,
        )
    }

    pub fn signed_with_payloads(
        author: ValidatorId,
        round: u64,
        epoch: u64,
        timestamp_ms: u64,
        ancestors: Vec<BlockRef>,
        intents: Vec<SignedIntent>,
        faucets: Vec<BlockFaucet>,
        delegations: Vec<BlockDelegation>,
        market_maker_programs: Vec<BlockMarketMakerProgram>,
        signing_key: [u8; 32],
    ) -> Self {
        Self::signed_with_oracle_payloads(
            author,
            round,
            epoch,
            timestamp_ms,
            ancestors,
            Vec::new(),
            intents,
            faucets,
            delegations,
            market_maker_programs,
            signing_key,
        )
    }

    pub fn signed_with_oracle_payloads(
        author: ValidatorId,
        round: u64,
        epoch: u64,
        timestamp_ms: u64,
        ancestors: Vec<BlockRef>,
        oracle_prices: Vec<BlockOraclePrice>,
        intents: Vec<SignedIntent>,
        faucets: Vec<BlockFaucet>,
        delegations: Vec<BlockDelegation>,
        market_maker_programs: Vec<BlockMarketMakerProgram>,
        signing_key: [u8; 32],
    ) -> Self {
        Self::signed_with_all_payloads(
            author,
            round,
            epoch,
            timestamp_ms,
            ancestors,
            oracle_prices,
            intents,
            faucets,
            delegations,
            market_maker_programs,
            Vec::new(),
            Vec::new(),
            signing_key,
        )
    }

    pub fn signed_with_all_payloads(
        author: ValidatorId,
        round: u64,
        epoch: u64,
        timestamp_ms: u64,
        ancestors: Vec<BlockRef>,
        oracle_prices: Vec<BlockOraclePrice>,
        intents: Vec<SignedIntent>,
        faucets: Vec<BlockFaucet>,
        delegations: Vec<BlockDelegation>,
        market_maker_programs: Vec<BlockMarketMakerProgram>,
        tee_governance: Vec<BlockTeeGovernance>,
        real_capital_governance: Vec<BlockRealCapitalGovernance>,
        signing_key: [u8; 32],
    ) -> Self {
        let mut block = Self {
            author,
            round,
            epoch,
            timestamp_ms,
            ancestors,
            oracle_prices,
            intents,
            faucets,
            delegations,
            market_maker_programs,
            tee_governance,
            real_capital_governance,
            author_signature: [0; 96],
        };
        let signature = SigningKey::from_bytes(&signing_key)
            .sign(&block.signing_bytes())
            .to_bytes();
        block.author_signature[..64].copy_from_slice(&signature);
        block
    }

    pub fn hash(&self) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(to_vec(self).expect("mahi block serialization should succeed"));
        hasher.finalize().into()
    }

    pub fn block_ref(&self) -> BlockRef {
        BlockRef {
            round: self.round,
            author: self.author,
            hash: self.hash(),
        }
    }

    pub fn ancestors_strictly_before_round(&self) -> bool {
        self.ancestors
            .iter()
            .all(|ancestor| ancestor.round < self.round)
    }

    pub fn has_ancestor_quorum(&self, validator_count: usize) -> bool {
        if self.round <= 1 {
            return self.ancestors.is_empty();
        }
        let Some(quorum) = quorum_threshold(validator_count) else {
            return false;
        };
        let previous_round = self.round.checked_sub(1);
        let mut authors = BTreeSet::new();
        for ancestor in &self.ancestors {
            if Some(ancestor.round) == previous_round {
                authors.insert(ancestor.author);
            }
        }
        authors.len() >= quorum
    }

    pub fn validate_author_signature(
        &self,
        consensus_pubkey: &ConsensusPubKey,
    ) -> Result<(), MahiBlockValidationError> {
        if self.author_signature[64..].iter().any(|byte| *byte != 0) {
            return Err(MahiBlockValidationError::InvalidSignature);
        }
        let verifying_key = VerifyingKey::from_bytes(consensus_pubkey)
            .map_err(|_| MahiBlockValidationError::InvalidValidatorKeyMaterial)?;
        let mut ed25519_bytes = [0_u8; 64];
        ed25519_bytes.copy_from_slice(&self.author_signature[..64]);
        let signature = Ed25519Signature::from_bytes(&ed25519_bytes);
        verifying_key
            .verify_strict(&self.signing_bytes(), &signature)
            .map_err(|_| MahiBlockValidationError::InvalidSignature)
    }

    pub fn validate_ancestor_rules(
        &self,
        validator_count: usize,
    ) -> Result<(), MahiBlockValidationError> {
        if !self.ancestors_strictly_before_round() {
            return Err(MahiBlockValidationError::InvalidAncestorOrder);
        }
        if !self.has_ancestor_quorum(validator_count) {
            return Err(MahiBlockValidationError::MissingAncestorQuorum);
        }
        Ok(())
    }
}

impl DagState {
    pub fn insert_block(&mut self, block: MahiBlock) -> BlockRef {
        let reference = block.block_ref();
        self.by_round
            .entry(block.round)
            .or_default()
            .insert(block.author, reference);
        self.pending_fetch.remove(&reference);
        self.blocks.insert(reference, block);
        reference
    }

    pub fn note_missing_ancestors(&mut self, block: &MahiBlock) {
        for ancestor in &block.ancestors {
            if !self.blocks.contains_key(ancestor) {
                self.pending_fetch.insert(*ancestor);
            }
        }
    }

    pub fn blocks_at_round(&self, round: u64) -> Vec<BlockRef> {
        self.by_round
            .get(&round)
            .map(|entries| entries.values().copied().collect())
            .unwrap_or_default()
    }

    pub fn highest_round(&self) -> u64 {
        self.by_round.keys().next_back().copied().unwrap_or(0)
    }

    pub fn get_block(&self, reference: BlockRef) -> Option<&MahiBlock> {
        self.blocks.get(&reference)
    }

    pub fn get_blocks_by_round(&self, round: u64) -> Vec<BlockRef> {
        self.blocks_at_round(round)
    }

    pub fn get_blocks_at_authority_round(&self, author: ValidatorId, round: u64) -> Vec<BlockRef> {
        self.by_round
            .get(&round)
            .and_then(|entries| entries.get(&author).copied())
            .into_iter()
            .collect()
    }

    pub fn linked(&self, anchor: BlockRef, candidate: BlockRef) -> bool {
        if anchor == candidate {
            return true;
        }
        let Some(candidate_block) = self.get_block(candidate) else {
            return false;
        };
        let mut pending = candidate_block.ancestors.clone();
        let mut seen = BTreeSet::new();
        while let Some(reference) = pending.pop() {
            if !seen.insert(reference) {
                continue;
            }
            if reference == anchor {
                return true;
            }
            if let Some(block) = self.get_block(reference) {
                pending.extend(block.ancestors.iter().copied());
            }
        }
        false
    }
}

impl From<&DagState> for CommitDagView {
    fn from(dag: &DagState) -> Self {
        Self {
            ancestors: dag
                .blocks
                .iter()
                .map(|(reference, block)| (*reference, block.ancestors.clone()))
                .collect(),
            by_round: dag.by_round.clone(),
        }
    }
}

impl From<DagState> for CommitDagView {
    fn from(dag: DagState) -> Self {
        Self::from(&dag)
    }
}

impl CommitDagView {
    pub fn highest_round(&self) -> u64 {
        self.by_round.keys().next_back().copied().unwrap_or(0)
    }

    pub fn get_blocks_by_round(&self, round: u64) -> Vec<BlockRef> {
        self.by_round
            .get(&round)
            .map(|entries| entries.values().copied().collect())
            .unwrap_or_default()
    }

    pub fn get_blocks_at_authority_round(&self, author: ValidatorId, round: u64) -> Vec<BlockRef> {
        self.by_round
            .get(&round)
            .and_then(|entries| entries.get(&author).copied())
            .into_iter()
            .collect()
    }

    pub fn ancestors(&self, reference: BlockRef) -> Option<&[BlockRef]> {
        self.ancestors.get(&reference).map(Vec::as_slice)
    }

    pub fn linked(&self, anchor: BlockRef, candidate: BlockRef) -> bool {
        if anchor == candidate {
            return true;
        }
        let Some(candidate_ancestors) = self.ancestors(candidate) else {
            return false;
        };
        let mut pending = candidate_ancestors.to_vec();
        let mut seen = BTreeSet::new();
        while let Some(reference) = pending.pop() {
            if !seen.insert(reference) {
                continue;
            }
            if reference == anchor {
                return true;
            }
            if let Some(ancestors) = self.ancestors(reference) {
                pending.extend(ancestors.iter().copied());
            }
        }
        false
    }
}

impl Linearizer {
    pub fn new() -> Self {
        Self::default()
    }

    fn collect_sub_dag(&mut self, dag: &DagState, leader: BlockRef) -> Option<CommittedSubDag> {
        let leader_block = dag.get_block(leader)?.clone();
        let mut to_commit = Vec::new();
        let mut buffer = vec![leader_block];
        if !self.committed.insert(leader) {
            return Some(CommittedSubDag::new(leader, Vec::new()));
        }
        while let Some(block) = buffer.pop() {
            to_commit.push(block.clone());
            for reference in &block.ancestors {
                if self.committed.insert(*reference) {
                    if let Some(ancestor) = dag.get_block(*reference) {
                        buffer.push(ancestor.clone());
                    }
                }
            }
        }
        Some(CommittedSubDag::new(leader, to_commit))
    }

    pub fn handle_commit(
        &mut self,
        dag: &DagState,
        committed_leaders: Vec<BlockRef>,
    ) -> Vec<CommittedSubDag> {
        let mut committed = Vec::new();
        for leader in committed_leaders {
            let Some(mut sub_dag) = self.collect_sub_dag(dag, leader) else {
                continue;
            };
            sub_dag.sort();
            committed.push(sub_dag);
        }
        committed
    }
}

impl BaseCommitter {
    pub fn new(validators: Vec<ValidatorId>, dag: impl Into<CommitDagView>) -> Self {
        Self {
            validators,
            dag: dag.into(),
            options: BaseCommitterOptions::default(),
        }
    }

    pub fn with_options(mut self, options: BaseCommitterOptions) -> Self {
        assert!(options.wave_length >= MINIMUM_WAVE_LENGTH);
        self.options = options;
        self
    }

    fn wave_number(&self, round: u64) -> u64 {
        round.saturating_sub(self.options.round_offset) / self.options.wave_length
    }

    fn leader_round(&self, wave: u64) -> u64 {
        wave * self.options.wave_length + self.options.round_offset
    }

    fn decision_round(&self, wave: u64) -> u64 {
        wave * self.options.wave_length + self.options.wave_length - 1 + self.options.round_offset
    }

    pub fn elect_leader(&self, round: u64) -> Option<ValidatorId> {
        let wave = self.wave_number(round);
        if self.leader_round(wave) != round || self.validators.is_empty() {
            return None;
        }
        let schedule = LeaderSchedule::new(self.validators.clone());
        Some(schedule.leader_for_round(round + self.options.leader_offset))
    }

    fn find_support(&self, author: ValidatorId, round: u64, from: BlockRef) -> Option<BlockRef> {
        let ancestors = self.dag.ancestors(from)?;
        if from.round <= round {
            return None;
        }
        for include in ancestors {
            if include.round < round {
                continue;
            }
            if include.author == author && include.round == round {
                return Some(*include);
            }
            if let Some(support) = self.find_support(author, round, *include) {
                return Some(support);
            }
        }
        None
    }

    fn is_vote(&self, potential_vote: BlockRef, leader: BlockRef) -> bool {
        self.find_support(leader.author, leader.round, potential_vote) == Some(leader)
    }

    fn is_certificate(&self, potential_certificate: BlockRef, leader: BlockRef) -> bool {
        let Some(quorum) = quorum_threshold(self.validators.len()) else {
            return false;
        };
        let Some(ancestors) = self.dag.ancestors(potential_certificate) else {
            return false;
        };
        let mut voters = BTreeSet::new();
        for reference in ancestors {
            if self.is_vote(*reference, leader) {
                voters.insert(reference.author);
                if voters.len() >= quorum {
                    return true;
                }
            }
        }
        false
    }

    fn decide_leader_from_anchor(
        &self,
        anchor: BlockRef,
        leader: ValidatorId,
        leader_round: u64,
    ) -> LeaderStatus {
        let leader_blocks = self.dag.get_blocks_at_authority_round(leader, leader_round);
        let wave = self.wave_number(leader_round);
        let decision_round = self.decision_round(wave);
        let potential_certificates: Vec<_> = self
            .dag
            .get_blocks_by_round(decision_round)
            .into_iter()
            .filter(|block| self.dag.linked(anchor, *block))
            .collect();

        let mut certified: Vec<_> = leader_blocks
            .into_iter()
            .filter(|leader_block| {
                potential_certificates
                    .iter()
                    .any(|certificate| self.is_certificate(*certificate, *leader_block))
            })
            .collect();

        assert!(
            certified.len() <= 1,
            "more than one certified leader at round {leader_round}"
        );

        match certified.pop() {
            Some(reference) => LeaderStatus::Commit(reference),
            None => LeaderStatus::Skip(leader, leader_round),
        }
    }

    fn can_skip_leader(&self, voting_round: u64, leader: ValidatorId, leader_round: u64) -> bool {
        let Some(quorum) = quorum_threshold(self.validators.len()) else {
            return false;
        };
        let leader_blocks = self.dag.get_blocks_at_authority_round(leader, leader_round);
        let voting_blocks = self.dag.get_blocks_by_round(voting_round);
        let voting_authors: BTreeSet<_> = voting_blocks.iter().map(|block| block.author).collect();
        if voting_authors.len() < quorum {
            return false;
        }
        if leader_blocks.is_empty() {
            return true;
        }
        for leader_block in leader_blocks {
            let mut non_voters = BTreeSet::new();
            for voting_block in &voting_blocks {
                if !self.is_vote(*voting_block, leader_block) {
                    non_voters.insert(voting_block.author);
                    if non_voters.len() >= quorum {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn enough_mysticeti_blame(
        &self,
        voting_round: u64,
        leader: ValidatorId,
        leader_round: u64,
    ) -> bool {
        let Some(quorum) = quorum_threshold(self.validators.len()) else {
            return false;
        };
        let voting_blocks = self.dag.get_blocks_by_round(voting_round);
        let mut blame_authors = BTreeSet::new();
        for voting_block in voting_blocks {
            let Some(ancestors) = self.dag.ancestors(voting_block) else {
                continue;
            };
            let references_leader = ancestors
                .iter()
                .any(|ancestor| ancestor.author == leader && ancestor.round == leader_round);
            if !references_leader {
                blame_authors.insert(voting_block.author);
                if blame_authors.len() >= quorum {
                    return true;
                }
            }
        }
        false
    }

    fn enough_leader_support(&self, decision_round: u64, leader: BlockRef) -> bool {
        let Some(quorum) = quorum_threshold(self.validators.len()) else {
            return false;
        };
        let decision_blocks = self.dag.get_blocks_by_round(decision_round);
        let mut certifiers = BTreeSet::new();
        for decision_block in decision_blocks {
            if self.is_certificate(decision_block, leader) {
                certifiers.insert(decision_block.author);
                if certifiers.len() >= quorum {
                    return true;
                }
            }
        }
        false
    }

    pub fn try_direct_decide(&self, leader: ValidatorId, leader_round: u64) -> LeaderStatus {
        match self.options.commit_rule {
            CommitRule::Mahi => {
                let last_voting_round = leader_round + self.options.wave_length - 2;
                if self.options.pipelined
                    && self.can_skip_leader(last_voting_round, leader, leader_round)
                {
                    return LeaderStatus::Skip(leader, leader_round);
                }
            }
            CommitRule::Mysticeti => {
                let voting_round = leader_round + 1;
                if self.enough_mysticeti_blame(voting_round, leader, leader_round) {
                    return LeaderStatus::Skip(leader, leader_round);
                }
            }
        }

        let wave = self.wave_number(leader_round);
        let decision_round = self.decision_round(wave);
        let mut supported: Vec<_> = self
            .dag
            .get_blocks_at_authority_round(leader, leader_round)
            .into_iter()
            .filter(|candidate| self.enough_leader_support(decision_round, *candidate))
            .collect();

        assert!(
            supported.len() <= 1,
            "more than one supported leader at round {leader_round}"
        );
        supported
            .pop()
            .map(LeaderStatus::Commit)
            .unwrap_or(LeaderStatus::Undecided(leader, leader_round))
    }

    pub fn try_indirect_decide<'a>(
        &self,
        leader: ValidatorId,
        leader_round: u64,
        leaders: impl Iterator<Item = &'a LeaderStatus>,
    ) -> LeaderStatus {
        let anchors =
            leaders.filter(|status| leader_round + self.options.wave_length <= status.round());
        for anchor in anchors {
            match anchor {
                LeaderStatus::Commit(anchor) => {
                    return self.decide_leader_from_anchor(*anchor, leader, leader_round);
                }
                LeaderStatus::Skip(..) => {}
                LeaderStatus::Undecided(..) => break,
            }
        }
        LeaderStatus::Undecided(leader, leader_round)
    }
}

impl UniversalCommitterBuilder {
    pub fn new(validators: Vec<ValidatorId>, dag: impl Into<CommitDagView>) -> Self {
        Self {
            validators,
            dag: dag.into(),
            wave_length: DEFAULT_WAVE_LENGTH,
            number_of_leaders: 1,
            pipeline: false,
            commit_rule: CommitRule::Mahi,
        }
    }

    pub fn with_wave_length(mut self, wave_length: u64) -> Self {
        self.wave_length = wave_length;
        self
    }

    pub fn with_number_of_leaders(mut self, number_of_leaders: usize) -> Self {
        self.number_of_leaders = number_of_leaders;
        self
    }

    pub fn with_pipeline(mut self, pipeline: bool) -> Self {
        self.pipeline = pipeline;
        self
    }

    pub fn with_commit_rule(mut self, commit_rule: CommitRule) -> Self {
        self.commit_rule = commit_rule;
        self
    }

    pub fn build(self) -> UniversalCommitter {
        let mut committers = Vec::new();
        let pipeline_stages = if self.pipeline { self.wave_length } else { 1 };
        for round_offset in 0..pipeline_stages {
            for leader_offset in 0..self.number_of_leaders {
                let options = BaseCommitterOptions {
                    wave_length: self.wave_length,
                    round_offset,
                    leader_offset: leader_offset as u64,
                    pipelined: self.pipeline,
                    commit_rule: self.commit_rule,
                };
                committers.push(
                    BaseCommitter::new(self.validators.clone(), self.dag.clone())
                        .with_options(options),
                );
            }
        }
        UniversalCommitter {
            dag: self.dag,
            committers,
            previously_committed_leaders: BTreeMap::new(),
            wave_length: self.wave_length,
        }
    }
}

impl UniversalCommitter {
    pub fn try_commit(
        &mut self,
        last_decided: BlockRef,
        threshold_round: u64,
    ) -> Vec<LeaderStatus> {
        let highest_known_round = self.dag.highest_round();
        if highest_known_round < self.wave_length {
            return Vec::new();
        }

        let mut leaders = Vec::new();
        for round in (last_decided.round..=highest_known_round).rev() {
            if round + self.wave_length > threshold_round {
                continue;
            }
            for committer in self.committers.iter().rev() {
                let Some(leader) = committer.elect_leader(round) else {
                    continue;
                };
                let mut status = self
                    .previously_committed_leaders
                    .get(&(leader, round))
                    .cloned()
                    .unwrap_or(LeaderStatus::Undecided(leader, round));
                if !status.is_decided() {
                    status = committer.try_direct_decide(leader, round);
                    if !status.is_decided() {
                        status = committer.try_indirect_decide(leader, round, leaders.iter());
                    }
                    if status.is_decided() {
                        self.previously_committed_leaders
                            .insert((leader, round), status.clone());
                    }
                }
                leaders.insert(0, status);
            }
        }

        let decided: Vec<_> = leaders
            .into_iter()
            .filter(|leader| leader.round() > 0)
            .collect();
        if last_decided.round == 0 {
            return decided
                .into_iter()
                .take_while(|leader| leader.is_decided())
                .collect();
        }
        // A caller may resume from a synthetic local restart boundary
        // instead of a concrete decided leader. In that case the boundary
        // author is outside the validator set and decisions start strictly
        // after the boundary round.
        let boundary_author_is_validator = self
            .committers
            .first()
            .is_some_and(|committer| committer.validators.contains(&last_decided.author));
        if last_decided.hash == [0; 32] && !boundary_author_is_validator {
            let mut sequence = Vec::new();
            for leader in decided
                .into_iter()
                .filter(|leader| leader.round() > last_decided.round)
            {
                if !leader.is_decided() {
                    break;
                }
                sequence.push(leader);
            }
            return sequence;
        }

        let mut after_last = false;
        let mut sequence = Vec::new();
        for leader in decided {
            if !after_last {
                if leader.round() == last_decided.round && leader.author() == last_decided.author {
                    after_last = true;
                }
                continue;
            }
            if !leader.is_decided() {
                break;
            }
            sequence.push(leader);
        }
        sequence
    }
}

impl LeaderSchedule {
    pub fn new(validators: Vec<ValidatorId>) -> Self {
        assert!(
            !validators.is_empty(),
            "leader schedule requires validators"
        );
        Self { validators }
    }

    pub fn leader_for_round(&self, round: u64) -> ValidatorId {
        self.validators[(round as usize) % self.validators.len()]
    }

    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }
}

impl ThresholdClock {
    pub fn new(round: u64) -> Self {
        Self {
            round,
            seen_authors: BTreeSet::new(),
        }
    }

    pub fn observe_block(&mut self, block: BlockRef, validator_count: usize) {
        let Some(quorum) = quorum_threshold(validator_count) else {
            return;
        };
        match block.round.cmp(&self.round) {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Greater => {
                self.seen_authors.clear();
                self.seen_authors.insert(block.author);
                self.round = block.round;
            }
            std::cmp::Ordering::Equal => {
                self.seen_authors.insert(block.author);
                if self.seen_authors.len() >= quorum {
                    self.seen_authors.clear();
                    self.round = block.round + 1;
                }
            }
        }
    }

    pub fn round(&self) -> u64 {
        self.round
    }
}

pub fn detects_equivocation(left: BlockRef, right: BlockRef) -> bool {
    left.author == right.author && left.round == right.round && left.hash != right.hash
}

#[cfg(test)]
mod tests {
    use super::{
        detects_equivocation, BaseCommitter, BaseCommitterOptions, BlockRef, CommitRule, DagState,
        LeaderSchedule, LeaderStatus, LeaderStatusWire, Linearizer, MahiBlock,
        MahiBlockValidationError, ThresholdClock, UniversalCommitterBuilder, DEFAULT_WAVE_LENGTH,
    };
    use crate::{quorum_threshold, Intent, IntentKind, SignedIntent, ValidatorId};
    use ed25519_dalek::SigningKey;

    fn mahi_types_enabled() -> bool {
        std::env::var("KOMBAT_MAHI_TYPES").ok().as_deref() == Some("1")
    }

    fn validator(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn signed_intent(nonce: u64) -> SignedIntent {
        SignedIntent {
            intent: Intent {
                chain_id: 7,
                account: 100 + nonce,
                policy_id: 200 + nonce,
                nonce,
                expires_at: 10_000 + nonce,
                predicate: None,
                kind: IntentKind::Transfer {
                    to: 500 + nonce,
                    token: 1,
                    amount: 25 + nonce,
                },
            },
            signature: [nonce as u8; 64],
        }
    }

    fn block_ref(round: u64, author: u8, hash: u8) -> BlockRef {
        BlockRef {
            round,
            author: validator(author),
            hash: [hash; 32],
        }
    }

    fn sample_block() -> MahiBlock {
        MahiBlock {
            author: validator(9),
            round: 7,
            epoch: 3,
            timestamp_ms: 123_456,
            ancestors: vec![
                block_ref(6, 1, 11),
                block_ref(6, 2, 12),
                block_ref(6, 3, 13),
                block_ref(5, 4, 14),
            ],
            oracle_prices: Vec::new(),
            intents: vec![signed_intent(1), signed_intent(2)],
            faucets: Vec::new(),
            delegations: Vec::new(),
            market_maker_programs: Vec::new(),
            tee_governance: Vec::new(),
            real_capital_governance: Vec::new(),
            author_signature: [55; 96],
        }
    }

    fn signing_key(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn committee() -> Vec<ValidatorId> {
        vec![validator(1), validator(2), validator(3), validator(4)]
    }

    fn genesis_ref() -> BlockRef {
        BlockRef {
            round: 0,
            author: validator(0),
            hash: [0; 32],
        }
    }

    fn build_complete_dag(rounds: u64) -> DagState {
        build_complete_dag_from(1, rounds)
    }

    fn build_complete_dag_from(start_round: u64, end_round: u64) -> DagState {
        let validators = committee();
        let mut dag = DagState::default();
        let mut previous_round = Vec::new();
        for round in start_round..=end_round {
            let ancestors = if round == start_round {
                Vec::new()
            } else {
                previous_round.clone()
            };
            let mut current_round = Vec::new();
            for (index, validator_id) in validators.iter().copied().enumerate() {
                let block = MahiBlock::signed(
                    validator_id,
                    round,
                    1,
                    1_000 + round,
                    ancestors.clone(),
                    vec![signed_intent(round * 10 + index as u64)],
                    [index as u8 + 1; 32],
                );
                current_round.push(dag.insert_block(block));
            }
            previous_round = current_round;
        }
        dag
    }

    fn quorum() -> usize {
        quorum_threshold(committee().len()).expect("committee should have quorum")
    }

    fn validity_threshold() -> usize {
        committee().len() - quorum() + 1
    }

    fn leader_for_round(round: u64) -> ValidatorId {
        LeaderSchedule::new(committee()).leader_for_round(round)
    }

    fn without_author(references: &[BlockRef], author: ValidatorId) -> Vec<BlockRef> {
        references
            .iter()
            .copied()
            .filter(|reference| reference.author != author)
            .collect()
    }

    fn insert_dag_layer(
        dag: &mut DagState,
        connections: Vec<(ValidatorId, Vec<BlockRef>)>,
    ) -> Vec<BlockRef> {
        let round = connections
            .first()
            .and_then(|(_, parents)| parents.first().map(|parent| parent.round + 1))
            .unwrap_or(1);
        let mut references = Vec::new();
        for (index, (author, parents)) in connections.into_iter().enumerate() {
            let block = MahiBlock::signed(
                author,
                round,
                1,
                1_000 + round * 10 + index as u64,
                parents,
                vec![signed_intent(round * 100 + index as u64)],
                [author[0]; 32],
            );
            references.push(dag.insert_block(block));
        }
        references
    }

    fn extend_complete_dag(
        dag: &mut DagState,
        previous_round: Vec<BlockRef>,
        target_round: u64,
    ) -> Vec<BlockRef> {
        let mut current = previous_round;
        let start_round = current
            .first()
            .map(|reference| reference.round + 1)
            .unwrap_or(1);
        for _round in start_round..=target_round {
            current = insert_dag_layer(
                dag,
                committee()
                    .into_iter()
                    .map(|author| (author, current.clone()))
                    .collect(),
            );
        }
        current
    }

    #[test]
    fn block_ref_round_trip() {
        if !mahi_types_enabled() {
            return;
        }

        let reference = block_ref(6, 1, 11);
        let encoded = borsh::to_vec(&reference).expect("block ref should serialize");
        let decoded =
            borsh::from_slice::<BlockRef>(&encoded).expect("block ref should deserialize");
        assert_eq!(decoded, reference);
    }

    #[test]
    fn mahi_block_round_trip() {
        if !mahi_types_enabled() {
            return;
        }

        let block = sample_block();
        let encoded = borsh::to_vec(&block).expect("mahi block should serialize");
        let decoded =
            borsh::from_slice::<MahiBlock>(&encoded).expect("mahi block should deserialize");
        assert_eq!(decoded, block);
    }

    #[test]
    fn leader_status_wire_round_trip() {
        if !mahi_types_enabled() {
            return;
        }

        let status = LeaderStatusWire::Skip {
            author: validator(2),
            round: 10,
        };
        let encoded = borsh::to_vec(&status).expect("leader status should serialize");
        let decoded = borsh::from_slice::<LeaderStatusWire>(&encoded)
            .expect("leader status should deserialize");
        assert_eq!(decoded, status);
    }

    #[test]
    fn dag_state_round_trip() {
        if !mahi_types_enabled() {
            return;
        }

        let block = sample_block();
        let mut dag = DagState::default();
        let reference = dag.insert_block(block.clone());
        dag.note_missing_ancestors(&block);
        dag.last_committed_round = 6;
        dag.last_commit_leader = Some(reference);
        dag.previously_committed_leaders.insert(
            (reference.author, reference.round),
            LeaderStatusWire::Commit(reference),
        );
        dag.committed_blocks.insert(reference);
        let encoded = borsh::to_vec(&dag).expect("dag state should serialize");
        let decoded =
            borsh::from_slice::<DagState>(&encoded).expect("dag state should deserialize");
        assert_eq!(decoded, dag);
    }

    #[test]
    fn leader_schedule_round_trip() {
        if !mahi_types_enabled() {
            return;
        }

        let schedule = LeaderSchedule::new(vec![validator(1), validator(2), validator(3)]);
        let encoded = borsh::to_vec(&schedule).expect("leader schedule should serialize");
        let decoded = borsh::from_slice::<LeaderSchedule>(&encoded)
            .expect("leader schedule should deserialize");
        assert_eq!(decoded, schedule);
    }

    #[test]
    fn threshold_clock_round_trip() {
        if !mahi_types_enabled() {
            return;
        }

        let mut clock = ThresholdClock::new(0);
        clock.observe_block(block_ref(0, 1, 11), 4);
        clock.observe_block(block_ref(0, 2, 12), 4);
        let encoded = borsh::to_vec(&clock).expect("threshold clock should serialize");
        let decoded = borsh::from_slice::<ThresholdClock>(&encoded)
            .expect("threshold clock should deserialize");
        assert_eq!(decoded, clock);
    }

    #[test]
    fn hash_stability_uses_unsigned_prefix_and_signed_block_hash() {
        if !mahi_types_enabled() {
            return;
        }

        let block = sample_block();
        let signing_bytes = block.signing_bytes();
        let full_bytes = borsh::to_vec(&block).expect("mahi block should serialize");
        debug_assert!(
            full_bytes.starts_with(&signing_bytes),
            "MahiBlock unsigned fields must remain a prefix of the full block representation"
        );

        let same_block = sample_block();
        assert_eq!(block.hash(), same_block.hash());

        let mut changed_signature = sample_block();
        changed_signature.author_signature = [77; 96];
        assert_ne!(block.hash(), changed_signature.hash());

        let mut changed_payload = sample_block();
        changed_payload.timestamp_ms += 1;
        assert_ne!(block.hash(), changed_payload.hash());
    }

    #[test]
    fn signed_block_verifies_against_author_key() {
        if !mahi_types_enabled() {
            return;
        }

        let signing_key = signing_key(9);
        let author = SigningKey::from_bytes(&signing_key)
            .verifying_key()
            .to_bytes();
        let block = MahiBlock::signed(
            author,
            7,
            3,
            123_456,
            vec![
                block_ref(6, 1, 11),
                block_ref(6, 2, 12),
                block_ref(6, 3, 13),
            ],
            vec![signed_intent(1)],
            signing_key,
        );
        assert_eq!(block.author, author);
        assert_eq!(
            block.validate_author_signature(&author),
            Ok(()),
            "freshly signed block should verify"
        );
    }

    #[test]
    fn ancestor_helpers_enforce_monotonicity_and_quorum() {
        if !mahi_types_enabled() {
            return;
        }

        let valid = sample_block();
        assert!(valid.ancestors_strictly_before_round());
        assert!(valid.has_ancestor_quorum(4));

        let invalid_monotonicity = MahiBlock {
            ancestors: vec![
                block_ref(7, 1, 11),
                block_ref(6, 2, 12),
                block_ref(6, 3, 13),
            ],
            ..sample_block()
        };
        assert!(!invalid_monotonicity.ancestors_strictly_before_round());

        let missing_quorum = MahiBlock {
            ancestors: vec![
                block_ref(6, 1, 11),
                block_ref(6, 1, 12),
                block_ref(5, 2, 13),
            ],
            ..sample_block()
        };
        assert!(!missing_quorum.has_ancestor_quorum(4));
        assert_eq!(
            missing_quorum.validate_ancestor_rules(4),
            Err(MahiBlockValidationError::MissingAncestorQuorum)
        );
    }

    #[test]
    fn round_one_blocks_use_empty_genesis_ancestor_set() {
        if !mahi_types_enabled() {
            return;
        }

        let round_one = MahiBlock {
            author: validator(1),
            round: 1,
            epoch: 3,
            timestamp_ms: 1_000,
            ancestors: Vec::new(),
            oracle_prices: Vec::new(),
            intents: vec![signed_intent(1)],
            faucets: Vec::new(),
            delegations: Vec::new(),
            market_maker_programs: Vec::new(),
            tee_governance: Vec::new(),
            real_capital_governance: Vec::new(),
            author_signature: [1; 96],
        };
        assert!(round_one.ancestors_strictly_before_round());
        assert!(round_one.has_ancestor_quorum(4));
        assert_eq!(round_one.validate_ancestor_rules(4), Ok(()));
    }

    #[test]
    fn equivocation_detection_helper_flags_same_author_round_different_hash() {
        if !mahi_types_enabled() {
            return;
        }

        let left = sample_block().block_ref();
        let mut right_block = sample_block();
        right_block.author_signature = [88; 96];
        let right = right_block.block_ref();
        assert!(detects_equivocation(left, right));

        let other_round = block_ref(left.round + 1, 9, 99);
        assert!(!detects_equivocation(left, other_round));
    }

    #[test]
    fn threshold_clock_advances_after_quorum() {
        if !mahi_types_enabled() {
            return;
        }

        let mut clock = ThresholdClock::new(0);
        clock.observe_block(block_ref(0, 1, 11), 4);
        clock.observe_block(block_ref(0, 2, 12), 4);
        assert_eq!(clock.round(), 0);
        clock.observe_block(block_ref(0, 3, 13), 4);
        assert_eq!(clock.round(), 1);
        clock.observe_block(block_ref(1, 1, 21), 4);
        clock.observe_block(block_ref(1, 2, 22), 4);
        clock.observe_block(block_ref(1, 3, 23), 4);
        assert_eq!(clock.round(), 2);
    }

    #[test]
    fn leader_schedule_is_round_robin() {
        if !mahi_types_enabled() {
            return;
        }

        let schedule = LeaderSchedule::new(vec![validator(1), validator(2), validator(3)]);
        assert_eq!(schedule.leader_for_round(0), validator(1));
        assert_eq!(schedule.leader_for_round(1), validator(2));
        assert_eq!(schedule.leader_for_round(2), validator(3));
        assert_eq!(schedule.leader_for_round(3), validator(1));
    }

    #[test]
    fn dag_state_tracks_missing_ancestors_and_round_index() {
        if !mahi_types_enabled() {
            return;
        }

        let block = sample_block();
        let mut dag = DagState::default();
        dag.note_missing_ancestors(&block);
        assert_eq!(dag.pending_fetch.len(), block.ancestors.len());
        let reference = dag.insert_block(block);
        assert_eq!(dag.blocks_at_round(reference.round), vec![reference]);
    }

    #[test]
    fn leader_status_reports_round_author_and_decision() {
        if !mahi_types_enabled() {
            return;
        }

        let committed = LeaderStatus::Commit(block_ref(7, 9, 44));
        assert_eq!(committed.round(), 7);
        assert_eq!(committed.author(), validator(9));
        assert!(committed.is_decided());

        let undecided = LeaderStatus::Undecided(validator(3), 12);
        assert_eq!(undecided.round(), 12);
        assert_eq!(undecided.author(), validator(3));
        assert!(!undecided.is_decided());
    }

    #[test]
    fn linearizer_collects_subdag_once_and_sorts_deterministically() {
        if !mahi_types_enabled() {
            return;
        }

        let mut dag = DagState::default();
        let r1a = MahiBlock::signed(
            validator(1),
            1,
            1,
            1_000,
            vec![],
            vec![signed_intent(1)],
            [1; 32],
        );
        let r1b = MahiBlock::signed(
            validator(2),
            1,
            1,
            1_001,
            vec![],
            vec![signed_intent(2)],
            [2; 32],
        );
        let r2 = MahiBlock::signed(
            validator(3),
            2,
            1,
            1_100,
            vec![r1a.block_ref(), r1b.block_ref()],
            vec![signed_intent(3)],
            [3; 32],
        );
        let r3 = MahiBlock::signed(
            validator(4),
            3,
            1,
            1_200,
            vec![r2.block_ref()],
            vec![signed_intent(4)],
            [4; 32],
        );
        let r4 = MahiBlock::signed(
            validator(5),
            4,
            1,
            1_300,
            vec![r2.block_ref()],
            vec![signed_intent(5)],
            [5; 32],
        );

        let r1a_ref = dag.insert_block(r1a);
        let r1b_ref = dag.insert_block(r1b);
        let r2_ref = dag.insert_block(r2);
        let r3_ref = dag.insert_block(r3);
        let r4_ref = dag.insert_block(r4);

        let mut linearizer = Linearizer::new();
        let committed = linearizer.handle_commit(&dag, vec![r3_ref, r4_ref]);
        assert_eq!(committed.len(), 2);
        assert_eq!(
            committed[0]
                .blocks
                .iter()
                .map(|block| (block.round, block.author, block.hash()))
                .collect::<Vec<_>>(),
            vec![
                (r1a_ref.round, r1a_ref.author, r1a_ref.hash),
                (r1b_ref.round, r1b_ref.author, r1b_ref.hash),
                (r2_ref.round, r2_ref.author, r2_ref.hash),
                (r3_ref.round, r3_ref.author, r3_ref.hash),
            ]
        );
        assert_eq!(
            committed[1]
                .blocks
                .iter()
                .map(|block| block.round)
                .collect::<Vec<_>>(),
            vec![4]
        );
        assert!(linearizer.committed.contains(&r1a_ref));
        assert!(linearizer.committed.contains(&r1b_ref));
        assert!(linearizer.committed.contains(&r2_ref));
        assert!(linearizer.committed.contains(&r3_ref));
        assert!(linearizer.committed.contains(&r4_ref));
    }

    #[test]
    fn universal_committer_direct_commit_matches_reference_shape() {
        if !mahi_types_enabled() {
            return;
        }

        let validators = committee();
        let dag = build_complete_dag(DEFAULT_WAVE_LENGTH * 2 + 1);
        let mut committer = UniversalCommitterBuilder::new(validators.clone(), dag).build();
        let sequence = committer.try_commit(genesis_ref(), DEFAULT_WAVE_LENGTH * 2);
        assert_eq!(sequence.len(), 1);
        match sequence[0] {
            LeaderStatus::Commit(reference) => {
                let expected =
                    LeaderSchedule::new(validators).leader_for_round(DEFAULT_WAVE_LENGTH);
                assert_eq!(reference.author, expected);
            }
            ref other => panic!("expected direct commit, got {other:?}"),
        }
    }

    #[test]
    fn universal_committer_resumes_after_synthetic_restart_boundary() {
        if !mahi_types_enabled() {
            return;
        }

        let validators = committee();
        let restart_boundary = DEFAULT_WAVE_LENGTH;
        let dag =
            build_complete_dag_from(restart_boundary, restart_boundary + DEFAULT_WAVE_LENGTH * 2);
        let mut committer = UniversalCommitterBuilder::new(validators.clone(), dag).build();
        let boundary = BlockRef {
            round: restart_boundary - 1,
            author: validator(0),
            hash: [0; 32],
        };

        let sequence = committer.try_commit(boundary, restart_boundary + DEFAULT_WAVE_LENGTH * 2);

        assert!(
            !sequence.is_empty(),
            "committer should decide post-restart leaders"
        );
        assert!(sequence
            .iter()
            .all(|status| status.round() > boundary.round && status.is_decided()));
    }

    #[test]
    fn universal_committer_multileader_direct_commit_matches_reference_shape() {
        if !mahi_types_enabled() {
            return;
        }

        let validators = committee();
        let number_of_leaders = quorum();
        let leader_round = DEFAULT_WAVE_LENGTH;
        let dag = build_complete_dag(DEFAULT_WAVE_LENGTH * 2 + 1);
        let mut committer = UniversalCommitterBuilder::new(validators.clone(), dag)
            .with_number_of_leaders(number_of_leaders)
            .build();
        let sequence = committer.try_commit(genesis_ref(), DEFAULT_WAVE_LENGTH * 2);
        assert_eq!(sequence.len(), number_of_leaders);
        for (offset, status) in sequence.iter().enumerate() {
            match status {
                LeaderStatus::Commit(reference) => {
                    assert_eq!(
                        reference.author,
                        LeaderSchedule::new(validators.clone())
                            .leader_for_round(leader_round + offset as u64)
                    );
                    assert_eq!(reference.round, leader_round);
                }
                other => panic!("expected committed multileader at offset {offset}, got {other:?}"),
            }
        }
    }

    #[test]
    fn universal_committer_multileader_fresh_builder_resumes_after_last_leader() {
        if !mahi_types_enabled() {
            return;
        }

        let number_of_leaders = quorum();
        let threshold_round = DEFAULT_WAVE_LENGTH * 3;
        let dag = build_complete_dag(threshold_round + 1);
        let mut first = UniversalCommitterBuilder::new(committee(), dag.clone())
            .with_number_of_leaders(number_of_leaders)
            .build();
        let first_sequence = first.try_commit(genesis_ref(), DEFAULT_WAVE_LENGTH * 2);
        assert_eq!(first_sequence.len(), number_of_leaders);
        let last = match first_sequence.last().expect("first wave should commit") {
            LeaderStatus::Commit(reference) => *reference,
            other => panic!("expected committed last leader, got {other:?}"),
        };

        let mut resumed = UniversalCommitterBuilder::new(committee(), dag)
            .with_number_of_leaders(number_of_leaders)
            .build();
        let second_sequence = resumed.try_commit(last, threshold_round);
        assert_eq!(second_sequence.len(), number_of_leaders);
        for (offset, status) in second_sequence.iter().enumerate() {
            match status {
                LeaderStatus::Commit(reference) => {
                    assert_eq!(reference.round, DEFAULT_WAVE_LENGTH * 2);
                    assert_eq!(
                        reference.author,
                        leader_for_round(DEFAULT_WAVE_LENGTH * 2 + offset as u64)
                    );
                }
                other => panic!("expected second-wave committed leader, got {other:?}"),
            }
        }
    }

    #[test]
    fn universal_committer_does_not_commit_before_first_wave_completes() {
        if !mahi_types_enabled() {
            return;
        }

        let validators = committee();
        for round in 0..DEFAULT_WAVE_LENGTH {
            let dag = build_complete_dag(round);
            let mut committer = UniversalCommitterBuilder::new(validators.clone(), dag).build();
            let sequence = committer.try_commit(genesis_ref(), DEFAULT_WAVE_LENGTH);
            assert!(
                sequence.is_empty(),
                "unexpected commit at round {round}: {sequence:?}"
            );
        }
    }

    #[test]
    fn universal_committer_late_call_commits_each_reference_leader_once() {
        if !mahi_types_enabled() {
            return;
        }

        let validators = committee();
        let leader_count = DEFAULT_WAVE_LENGTH * 2;
        let decision_round = DEFAULT_WAVE_LENGTH * leader_count + DEFAULT_WAVE_LENGTH;
        let dag = build_complete_dag(decision_round);
        let mut committer = UniversalCommitterBuilder::new(validators.clone(), dag).build();
        let sequence = committer.try_commit(genesis_ref(), decision_round + 1);
        assert_eq!(sequence.len(), leader_count as usize);
        for (index, status) in sequence.iter().enumerate() {
            let leader_round = (index as u64 + 1) * DEFAULT_WAVE_LENGTH;
            match status {
                LeaderStatus::Commit(reference) => {
                    assert_eq!(reference.author, leader_for_round(leader_round));
                    assert_eq!(reference.round, leader_round);
                }
                other => panic!("expected committed leader at round {leader_round}, got {other:?}"),
            }
        }
    }

    #[test]
    fn base_committer_direct_skip_stays_disabled_when_pipeline_is_off() {
        if !mahi_types_enabled() {
            return;
        }

        let leader_round = DEFAULT_WAVE_LENGTH;
        let mut dag = DagState::default();
        let previous_round = extend_complete_dag(&mut dag, Vec::new(), leader_round - 1);
        let leader = leader_for_round(leader_round);
        let round_without_leader = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .filter(|author| *author != leader)
                .map(|author| (author, previous_round.clone()))
                .collect(),
        );
        extend_complete_dag(
            &mut dag,
            round_without_leader,
            leader_round + DEFAULT_WAVE_LENGTH - 1,
        );

        let committer = BaseCommitter::new(committee(), dag).with_options(BaseCommitterOptions {
            wave_length: DEFAULT_WAVE_LENGTH,
            leader_offset: 0,
            round_offset: 0,
            pipelined: false,
            commit_rule: CommitRule::Mahi,
        });
        assert_eq!(
            committer.try_direct_decide(leader, leader_round),
            LeaderStatus::Undecided(leader, leader_round)
        );
    }

    #[test]
    fn base_committer_direct_skip_requires_pipeline_when_leader_is_missing() {
        if !mahi_types_enabled() {
            return;
        }

        let leader_round = DEFAULT_WAVE_LENGTH;
        let mut dag = DagState::default();
        let previous_round = extend_complete_dag(&mut dag, Vec::new(), leader_round - 1);
        let leader = leader_for_round(leader_round);
        let round_without_leader = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .filter(|author| *author != leader)
                .map(|author| (author, previous_round.clone()))
                .collect(),
        );
        extend_complete_dag(
            &mut dag,
            round_without_leader,
            leader_round + DEFAULT_WAVE_LENGTH - 1,
        );

        let committer = BaseCommitter::new(committee(), dag).with_options(BaseCommitterOptions {
            wave_length: DEFAULT_WAVE_LENGTH,
            leader_offset: 0,
            round_offset: 0,
            pipelined: true,
            commit_rule: CommitRule::Mahi,
        });
        assert_eq!(
            committer.try_direct_decide(leader, leader_round),
            LeaderStatus::Skip(leader, leader_round)
        );
    }

    #[test]
    fn base_committer_mysticeti_rule_skips_missing_leader_without_pipeline() {
        if !mahi_types_enabled() {
            return;
        }

        let leader_round = DEFAULT_WAVE_LENGTH;
        let mut dag = DagState::default();
        let previous_round = extend_complete_dag(&mut dag, Vec::new(), leader_round - 1);
        let leader = leader_for_round(leader_round);
        let round_without_leader = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .filter(|author| *author != leader)
                .map(|author| (author, previous_round.clone()))
                .collect(),
        );
        let voting_round = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .map(|author| (author, round_without_leader.clone()))
                .collect(),
        );
        assert_eq!(voting_round[0].round, leader_round + 1);

        let committer = BaseCommitter::new(committee(), dag).with_options(BaseCommitterOptions {
            wave_length: DEFAULT_WAVE_LENGTH,
            leader_offset: 0,
            round_offset: 0,
            pipelined: false,
            commit_rule: CommitRule::Mysticeti,
        });
        assert_eq!(
            committer.try_direct_decide(leader, leader_round),
            LeaderStatus::Skip(leader, leader_round)
        );
    }

    #[test]
    fn universal_committer_indirect_commit_matches_reference_shape() {
        if !mahi_types_enabled() {
            return;
        }

        let leader_round = DEFAULT_WAVE_LENGTH;
        let decision_round = leader_round + DEFAULT_WAVE_LENGTH - 1;
        let mut dag = DagState::default();

        let references_1 = extend_complete_dag(&mut dag, Vec::new(), leader_round);
        let leader_1 = leader_for_round(leader_round);
        let references_without_leader_1 = without_author(&references_1, leader_1);

        let references_with_votes_for_leader_1 = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .take(quorum())
                .map(|author| (author, references_1.clone()))
                .collect(),
        );
        let references_without_votes_for_leader_1 = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .skip(quorum())
                .map(|author| (author, references_without_leader_1.clone()))
                .collect(),
        );

        let mut references_3 = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .take(validity_threshold())
                .map(|author| (author, references_with_votes_for_leader_1.clone()))
                .collect(),
        );

        let mixed_references: Vec<_> = references_without_votes_for_leader_1
            .into_iter()
            .chain(references_with_votes_for_leader_1)
            .take(quorum())
            .collect();
        references_3.extend(insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .skip(validity_threshold())
                .map(|author| (author, mixed_references.clone()))
                .collect(),
        ));

        let threshold_round = DEFAULT_WAVE_LENGTH * 3 + 1;
        extend_complete_dag(&mut dag, references_3, threshold_round - 1);

        let mut committer = UniversalCommitterBuilder::new(committee(), dag).build();
        let sequence = committer.try_commit(genesis_ref(), threshold_round);
        assert_eq!(sequence.len(), 2);
        match sequence[0] {
            LeaderStatus::Commit(reference) => {
                assert_eq!(reference.author, leader_1);
                assert_eq!(reference.round, leader_round);
            }
            ref other => panic!("expected indirect commit for first leader, got {other:?}"),
        }
        match sequence[1] {
            LeaderStatus::Commit(reference) => {
                assert_eq!(reference.author, leader_for_round(leader_round * 2));
                assert_eq!(reference.round, leader_round * 2);
            }
            ref other => panic!("expected later leader commit, got {other:?}"),
        }
        assert_eq!(decision_round, leader_round + DEFAULT_WAVE_LENGTH - 1);
    }

    #[test]
    fn universal_committer_indirect_skip_missing_leader_after_later_anchor() {
        if !mahi_types_enabled() {
            return;
        }

        let leader_round = DEFAULT_WAVE_LENGTH;
        let later_leader_round = DEFAULT_WAVE_LENGTH * 2;
        let threshold_round = DEFAULT_WAVE_LENGTH * 3 + 1;
        let mut dag = DagState::default();
        let previous_round = extend_complete_dag(&mut dag, Vec::new(), leader_round - 1);
        let leader = leader_for_round(leader_round);
        let round_without_leader = insert_dag_layer(
            &mut dag,
            committee()
                .into_iter()
                .filter(|author| *author != leader)
                .map(|author| (author, previous_round.clone()))
                .collect(),
        );
        extend_complete_dag(&mut dag, round_without_leader, threshold_round - 1);

        let mut committer = UniversalCommitterBuilder::new(committee(), dag).build();
        let sequence = committer.try_commit(genesis_ref(), threshold_round);
        assert_eq!(sequence.len(), 2);
        assert_eq!(sequence[0], LeaderStatus::Skip(leader, leader_round));
        match sequence[1] {
            LeaderStatus::Commit(reference) => {
                assert_eq!(reference.author, leader_for_round(later_leader_round));
                assert_eq!(reference.round, later_leader_round);
            }
            ref other => panic!("expected later committed leader, got {other:?}"),
        }
    }

    #[test]
    fn base_committer_elects_round_robin_leader_only_on_leader_rounds() {
        if !mahi_types_enabled() {
            return;
        }

        let committer = BaseCommitter::new(committee(), DagState::default());
        assert_eq!(committer.elect_leader(0), Some(validator(1)));
        assert_eq!(committer.elect_leader(1), None);
        assert_eq!(
            committer.elect_leader(DEFAULT_WAVE_LENGTH),
            Some(validator(2))
        );
    }
}
