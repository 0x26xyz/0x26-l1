use borsh::BorshDeserialize;
use ed25519_dalek::{Signer, SigningKey};

use super::{
    block_hash, body_root, build_timeout_certificate, component_name, intent_id,
    new_view_message_signing_bytes, quorum_certificate_signing_bytes, quorum_threshold,
    timeout_message_signing_bytes, validate_block_against_parent,
    validate_block_against_parent_and_membership, validate_new_view_message,
    validate_quorum_certificate_with_membership, validate_timeout_message, validate_vote_message,
    vote_message_signing_bytes, Block, BlockBody, BlockHeader, BlockItem, BlockValidationError,
    ConsensusMessage, Intent, IntentKind, NewViewMessage, PacemakerConfig, PacemakerState,
    ProposalMessage, QuorumCertificate, Signature, SignedIntent, SignedTransaction, TimeoutMessage,
    ValidatorVerifier, VoteMessage,
};

fn validator(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn signing_key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn verifier(byte: u8) -> ValidatorVerifier {
    ValidatorVerifier {
        validator: validator(byte),
        consensus_pubkey: signing_key(byte).verifying_key().to_bytes(),
    }
}

fn validator_membership() -> Vec<ValidatorVerifier> {
    vec![verifier(11), verifier(12), verifier(13), verifier(14)]
}

fn consensus_signature_bytes(byte: u8, message: &[u8]) -> [u8; 96] {
    let mut bytes = [0; 96];
    let signature = signing_key(byte).sign(message).to_bytes();
    bytes[..64].copy_from_slice(&signature);
    bytes
}

fn signed_qc(view: u64, block_hash: [u8; 32], signers: &[u8]) -> QuorumCertificate {
    let mut qc = QuorumCertificate {
        view,
        block_hash,
        signatures: Vec::with_capacity(signers.len()),
    };
    let signing_bytes = quorum_certificate_signing_bytes(&qc).expect("qc signing bytes");
    qc.signatures = signers
        .iter()
        .copied()
        .map(|signer| Signature {
            validator: validator(signer),
            bytes: consensus_signature_bytes(signer, &signing_bytes),
        })
        .collect();
    qc
}

fn sample_body() -> BlockBody {
    BlockBody {
        items: vec![
            BlockItem::Intent(SignedIntent {
                intent: Intent {
                    chain_id: 1,
                    account: 99,
                    policy_id: 7,
                    nonce: 1,
                    expires_at: 10_000,
                    predicate: None,
                    kind: IntentKind::Transfer {
                        to: 100,
                        token: 1,
                        amount: 25,
                    },
                },
                signature: [1; 64],
            }),
            BlockItem::Transaction(SignedTransaction {
                signer: validator(9),
                bytes: vec![4, 5, 6],
            }),
        ],
    }
}

fn sample_parent() -> BlockHeader {
    BlockHeader {
        chain_id: 1,
        height: 41,
        round: 7,
        proposer: validator(1),
        parent_hash: [0; 32],
        parent_state_root: [1; 32],
        tx_root: [2; 32],
        event_root: [3; 32],
        state_root: [4; 32],
        qc: signed_qc(6, [5; 32], &[11]),
        timestamp_ms: 100,
    }
}

fn sample_block(parent: &BlockHeader) -> Block {
    let body = sample_body();
    let parent_hash = block_hash(&Block {
        header: parent.clone(),
        body: BlockBody { items: vec![] },
    });

    Block {
        header: BlockHeader {
            chain_id: 1,
            height: parent.height + 1,
            round: parent.round + 1,
            proposer: validator(2),
            parent_hash,
            parent_state_root: parent.state_root,
            tx_root: body_root(&body),
            event_root: [8; 32],
            state_root: [9; 32],
            qc: signed_qc(parent.round, parent_hash, &[11, 12, 13]),
            timestamp_ms: parent.timestamp_ms,
        },
        body,
    }
}

fn sample_vote(block_hash: [u8; 32]) -> VoteMessage {
    let mut vote = VoteMessage {
        chain_id: 1,
        height: 42,
        round: 8,
        block_hash,
        voter: validator(12),
        signature: [0; 96],
    };
    let signing_bytes = vote_message_signing_bytes(&vote).expect("vote signing bytes");
    vote.signature = consensus_signature_bytes(12, &signing_bytes);
    vote
}

fn sample_new_view(block_hash: [u8; 32]) -> NewViewMessage {
    let mut new_view = NewViewMessage {
        chain_id: 1,
        height: 42,
        round: 9,
        high_qc: signed_qc(8, block_hash, &[11, 12, 13]),
        sender: validator(14),
        signature: [0; 96],
    };
    let signing_bytes = new_view_message_signing_bytes(&new_view).expect("new-view signing bytes");
    new_view.signature = consensus_signature_bytes(14, &signing_bytes);
    new_view
}

fn sample_timeout(
    sender: [u8; 32],
    round: u64,
    qc_view: u64,
    block_hash: [u8; 32],
) -> TimeoutMessage {
    let mut timeout = TimeoutMessage {
        chain_id: 1,
        height: 42,
        round,
        sender,
        high_qc: signed_qc(qc_view, block_hash, &[11, 12, 13]),
        signature: [0; 96],
    };
    let signing_bytes = timeout_message_signing_bytes(&timeout).expect("timeout signing bytes");
    timeout.signature = consensus_signature_bytes(sender[0], &signing_bytes);
    timeout
}

#[test]
fn reports_component_name() {
    assert_eq!(component_name(), "consensus");
}

#[test]
fn derives_stable_intent_id() {
    let signed_intent = SignedIntent {
        intent: Intent {
            chain_id: 1,
            account: 99,
            policy_id: 7,
            nonce: 1,
            expires_at: 10_000,
            predicate: None,
            kind: IntentKind::Cancel { order_id: 55 },
        },
        signature: [9; 64],
    };

    assert_eq!(intent_id(&signed_intent), intent_id(&signed_intent));
}

#[test]
fn serializes_blocks_deterministically() {
    let parent = sample_parent();
    let left = sample_block(&parent);
    let right = sample_block(&parent);

    let left_bytes = borsh::to_vec(&left).expect("block serialization");
    let right_bytes = borsh::to_vec(&right).expect("block serialization");

    assert_eq!(left_bytes, right_bytes);
    assert_eq!(block_hash(&left), block_hash(&right));
}

#[test]
fn round_trips_block_bytes() {
    let block = sample_block(&sample_parent());
    let bytes = borsh::to_vec(&block).expect("block serialization");
    let decoded = Block::try_from_slice(&bytes).expect("block deserialization");

    assert_eq!(decoded, block);
}

#[test]
fn rejects_truncated_block_bytes() {
    let block = sample_block(&sample_parent());
    let mut bytes = borsh::to_vec(&block).expect("block serialization");
    bytes.pop();

    assert!(Block::try_from_slice(&bytes).is_err());
}

#[test]
fn validates_parent_link_and_qc() {
    let parent = sample_parent();
    let block = sample_block(&parent);

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Ok(())
    );
    assert_eq!(
        validate_block_against_parent_and_membership(
            &block,
            &parent,
            &validator_membership(),
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Ok(())
    );
}

#[test]
fn rejects_duplicate_qc_signers() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.signatures.push(Signature {
        validator: validator(12),
        bytes: block.header.qc.signatures[1].bytes,
    });

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::InvalidQuorumCertificate)
    );
}

#[test]
fn rejects_parent_hash_mismatch() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.parent_hash = [77; 32];

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::InvalidParentHash)
    );
}

#[test]
fn rejects_chain_id_mismatch() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.chain_id = parent.chain_id + 1;

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::InvalidChainId)
    );
}

#[test]
fn rejects_non_increasing_round() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.round = parent.round;

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::NonIncreasingRound)
    );
}

#[test]
fn rejects_unexpected_proposer() {
    let parent = sample_parent();
    let block = sample_block(&parent);

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            validator(99),
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::UnexpectedProposer)
    );
}

#[test]
fn rejects_timestamp_outside_local_clock_skew() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.timestamp_ms = parent.timestamp_ms + 2_001;

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            parent.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::TimestampOutsideLocalClockSkew)
    );
}

#[test]
fn rejects_qc_without_threshold() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.signatures.pop();

    assert_eq!(
        validate_quorum_certificate_with_membership(
            &block.header.qc,
            &block.header.parent_hash,
            &validator_membership()
        ),
        Err(BlockValidationError::InsufficientQuorum)
    );
}

#[test]
fn rejects_qc_with_unknown_validator() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.signatures[0].validator = validator(99);

    assert_eq!(
        validate_quorum_certificate_with_membership(
            &block.header.qc,
            &block.header.parent_hash,
            &validator_membership()
        ),
        Err(BlockValidationError::UnknownValidator)
    );
}

#[test]
fn rejects_qc_view_that_does_not_match_parent_round() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.view = parent.round + 1;

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::InvalidQuorumCertificate)
    );
}

#[test]
fn rejects_qc_with_forged_signature() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.signatures[0].bytes[0] ^= 0x55;

    assert_eq!(
        validate_quorum_certificate_with_membership(
            &block.header.qc,
            &block.header.parent_hash,
            &validator_membership()
        ),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn validates_vote_and_new_view_messages() {
    let parent = sample_parent();
    let block = sample_block(&parent);
    let vote = sample_vote(block.header.parent_hash);
    let new_view = sample_new_view(block.header.parent_hash);

    assert_eq!(
        validate_vote_message(&vote, &validator_membership()),
        Ok(())
    );
    assert_eq!(
        validate_new_view_message(&new_view, &validator_membership()),
        Ok(())
    );
}

#[test]
fn rejects_vote_from_unknown_validator() {
    let mut vote = sample_vote([7; 32]);
    vote.voter = validator(77);

    assert_eq!(
        validate_vote_message(&vote, &validator_membership()),
        Err(BlockValidationError::UnknownValidator)
    );
}

#[test]
fn rejects_vote_with_forged_signature() {
    let mut vote = sample_vote([7; 32]);
    vote.signature[0] ^= 0x55;

    assert_eq!(
        validate_vote_message(&vote, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn serializes_consensus_messages_deterministically() {
    let parent = sample_parent();
    let block = sample_block(&parent);
    let proposal = ConsensusMessage::Proposal(ProposalMessage { chain_id: 1, block });
    let left = borsh::to_vec(&proposal).expect("proposal serialization");
    let right = borsh::to_vec(&proposal).expect("proposal serialization");

    assert_eq!(left, right);
}

#[test]
fn computes_quorum_threshold_for_four_validators() {
    assert_eq!(quorum_threshold(4), Some(3));
    assert_eq!(quorum_threshold(1), Some(1));
    assert_eq!(quorum_threshold(0), None);
}

#[test]
fn validates_timeout_messages() {
    let timeout = sample_timeout(validator(14), 8, 8, [9; 32]);

    assert_eq!(
        validate_timeout_message(&timeout, &validator_membership()),
        Ok(())
    );
}

#[test]
fn builds_timeout_certificate_and_selects_highest_qc() {
    let messages = vec![
        sample_timeout(validator(11), 8, 7, [7; 32]),
        sample_timeout(validator(12), 8, 9, [9; 32]),
        sample_timeout(validator(13), 8, 8, [8; 32]),
    ];

    let timeout_certificate =
        build_timeout_certificate(8, &messages, &validator_membership()).expect("tc");

    assert_eq!(timeout_certificate.round, 8);
    assert_eq!(timeout_certificate.highest_qc.view, 9);
    assert_eq!(timeout_certificate.senders.len(), 3);
}

#[test]
fn rejects_timeout_certificate_with_duplicate_sender() {
    let messages = vec![
        sample_timeout(validator(11), 8, 7, [7; 32]),
        sample_timeout(validator(11), 8, 9, [9; 32]),
        sample_timeout(validator(13), 8, 8, [8; 32]),
    ];

    assert_eq!(
        build_timeout_certificate(8, &messages, &validator_membership()),
        Err(BlockValidationError::InvalidTimeoutCertificate)
    );
}

#[test]
fn pacemaker_resets_after_qc_and_backs_off_after_timeouts() {
    let mut pacemaker = PacemakerState::new(8, signed_qc(7, [7; 32], &[11, 12, 13]));
    let config = PacemakerConfig {
        base_timeout_ms: 300,
        max_timeout_ms: 20_000,
        max_exponent: 6,
        max_clock_skew_ms: 1_000,
    };

    assert_eq!(pacemaker.round_timeout_ms(&config), 300);

    let timeout_certificate = build_timeout_certificate(
        8,
        &[
            sample_timeout(validator(11), 8, 7, [7; 32]),
            sample_timeout(validator(12), 8, 8, [8; 32]),
            sample_timeout(validator(13), 8, 9, [9; 32]),
        ],
        &validator_membership(),
    )
    .expect("tc");

    pacemaker.advance_on_timeout_certificate(&timeout_certificate);
    assert_eq!(pacemaker.current_round, 9);
    assert_eq!(pacemaker.consecutive_failures, 1);
    assert_eq!(pacemaker.round_timeout_ms(&config), 600);
    assert_eq!(pacemaker.highest_qc.view, 9);

    pacemaker.advance_on_qc(signed_qc(10, [10; 32], &[11, 12, 13]));

    assert_eq!(pacemaker.current_round, 11);
    assert_eq!(pacemaker.consecutive_failures, 0);
    assert_eq!(pacemaker.round_timeout_ms(&config), 300);
}

#[test]
fn pacemaker_qc_does_not_regress_round_after_timeout_recovery() {
    let mut pacemaker = PacemakerState::new(8, signed_qc(7, [7; 32], &[11, 12, 13]));
    let timeout_certificate = build_timeout_certificate(
        8,
        &[
            sample_timeout(validator(11), 8, 7, [7; 32]),
            sample_timeout(validator(12), 8, 7, [7; 32]),
            sample_timeout(validator(13), 8, 7, [7; 32]),
        ],
        &validator_membership(),
    )
    .expect("tc");

    pacemaker.advance_on_timeout_certificate(&timeout_certificate);
    assert_eq!(pacemaker.current_round, 9);

    pacemaker.advance_on_qc(signed_qc(7, [7; 32], &[11, 12, 13]));

    assert_eq!(pacemaker.current_round, 9);
    assert_eq!(pacemaker.highest_qc.view, 7);
}

#[test]
fn pacemaker_emits_local_timeout_for_current_round() {
    let pacemaker = PacemakerState::new(5, signed_qc(4, [4; 32], &[11, 12, 13]));

    let timeout = pacemaker.local_timeout(1, 99, validator(14));

    assert_eq!(timeout.round, 5);
    assert_eq!(timeout.height, 99);
    assert_eq!(timeout.sender, validator(14));
    assert_eq!(timeout.high_qc.view, 4);
}

#[test]
fn rejects_non_monotonic_block_timestamp() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.timestamp_ms = parent.timestamp_ms - 1;

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::NonMonotonicTimestamp)
    );
}

#[test]
fn rejects_qc_block_hash_mismatch_with_parent() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.block_hash = [0xAB; 32];

    assert_eq!(
        validate_block_against_parent(
            &block,
            &parent,
            block.header.proposer,
            block.header.timestamp_ms,
            1_000,
        ),
        Err(BlockValidationError::InvalidQuorumCertificate)
    );
}

#[test]
fn rejects_qc_signature_with_nonzero_trailing_bytes() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.signatures[0].bytes[80] = 0xFF;

    assert_eq!(
        validate_quorum_certificate_with_membership(
            &block.header.qc,
            &block.header.parent_hash,
            &validator_membership()
        ),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_qc_replayed_with_modified_view() {
    let parent = sample_parent();
    let mut block = sample_block(&parent);
    block.header.qc.view = parent.round + 9;

    assert_eq!(
        validate_quorum_certificate_with_membership(
            &block.header.qc,
            &block.header.parent_hash,
            &validator_membership()
        ),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_vote_replayed_across_chain_id() {
    let mut vote = sample_vote([7; 32]);
    vote.chain_id += 1;

    assert_eq!(
        validate_vote_message(&vote, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_vote_replayed_across_round() {
    let mut vote = sample_vote([7; 32]);
    vote.round += 1;

    assert_eq!(
        validate_vote_message(&vote, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_vote_with_tampered_voter_field() {
    let mut vote = sample_vote([7; 32]);
    vote.voter = validator(11);

    assert_eq!(
        validate_vote_message(&vote, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_new_view_replayed_across_height() {
    let mut new_view = sample_new_view([7; 32]);
    new_view.height += 1;

    assert_eq!(
        validate_new_view_message(&new_view, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_timeout_replayed_across_round() {
    let mut timeout = sample_timeout(validator(14), 8, 8, [9; 32]);
    timeout.round = 9;

    assert_eq!(
        validate_timeout_message(&timeout, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}

#[test]
fn rejects_cross_message_domain_confusion() {
    let vote = sample_vote([7; 32]);
    let forged_timeout = TimeoutMessage {
        chain_id: vote.chain_id,
        height: vote.height,
        round: vote.round,
        sender: vote.voter,
        high_qc: signed_qc(vote.round - 1, vote.block_hash, &[11, 12, 13]),
        signature: vote.signature,
    };

    assert_eq!(
        validate_timeout_message(&forged_timeout, &validator_membership()),
        Err(BlockValidationError::InvalidSignature)
    );
}
