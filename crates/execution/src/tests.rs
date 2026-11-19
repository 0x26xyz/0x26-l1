use ed25519_dalek::{Signer, SigningKey};
use kombat_chain_consensus::{
    block_hash, body_root, intent_id, intent_signing_hash, Block, BlockBody, BlockHeader,
    BlockItem, Hash, Intent, IntentKind, IntentOrderKind, IntentOrderSide, IntentTimeInForce,
    QuorumCertificate, Signature, SignedIntent, SignedTransaction,
};
use kombat_chain_matching::{
    OrderKind as MatchingOrderKind, OrderRequest, OrderSide, SingleMarketBook, TimeInForce,
};
use kombat_chain_oracle::OracleConfig;
use kombat_chain_policy::{PolicyRecord, PolicyState, PolicyStore, PolicyTerms, SessionPayload};
use kombat_chain_state_db::{
    CheckpointableStateStore, InMemoryStateStore, StateCheckpoint, StateView,
};
use revm::{
    interpreter::CallScheme,
    primitives::{keccak256, Address, U256},
    Database,
};

use super::{
    abi_word_from_address, abi_word_from_u64, account_evm_address_key, address_from_u64,
    attestation_vendor_roots_state_key, component_name, compute_event_root, decode_u64_word,
    evm_block_hash_key, evm_usdc_balance_key, evm_usdc_minter_role_key, evm_usdc_system_address,
    policy_usdc_balance_key, precompile_by_address, protocol_precompiles, stage_parent_block_hash,
    validator_to_address, verify_attestation_quote, AttestationVendorRoot,
    AttestationVendorRootPayload, AttestationVendorRootsState, AttestationVerifyClaim,
    AttestationVerifyFailure, AttestationVerifyOutcome, AttestationVerifyRequest,
    AttestationVerifyVendor, BlockError, BlockExecutor, EvmPrecompileCall, ExecutionContext,
    ExecutionEvent, ExecutionPipeline, OracleMarkRecord, PrecompileCall, PrecompileError,
    PrecompileMutability, PreparedBlock, ProtocolStateAccessor, QuarantineReason, RevmShell,
    SubmissionReason, SubmissionStatus, ATTESTATION_QUOTE_FRESHNESS_BLOCKS,
    ATTESTATION_VERIFY_SELECTOR, BEST_BID_ASK_SELECTOR, DEPTH_SELECTOR,
    ERC165_SUPPORTS_INTERFACE_SELECTOR, ERC20_ALLOWANCE_SELECTOR, ERC20_APPROVE_SELECTOR,
    ERC20_BALANCE_OF_SELECTOR, ERC20_DECIMALS_SELECTOR, ERC20_NAME_SELECTOR, ERC20_SYMBOL_SELECTOR,
    ERC20_TOTAL_SUPPLY_SELECTOR, ERC20_TRANSFER_FROM_SELECTOR, ERC20_TRANSFER_SELECTOR,
    EVM_USDC_DEPOSIT_FOR_SELECTOR, EVM_USDC_DEPOSIT_SELECTOR, EVM_USDC_WITHDRAW_SELECTOR,
    MALFORMED_CALLDATA_ERROR_SELECTOR, MARK_PRICE_SELECTOR, MAX_DEPTH_LEVELS,
    MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP, RETAINED_BLOCK_HASH_WINDOW_BLOCKS, SESSION_KEY_AUTHORIZE,
    STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR, SUBMIT_SIGNED_INTENT_SELECTOR,
    UNKNOWN_SELECTOR_ERROR_SELECTOR,
};

fn validator(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn signature(byte: u8) -> [u8; 96] {
    [byte; 96]
}

fn signing_key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn sample_policy_terms() -> PolicyTerms {
    PolicyTerms {
        gas_budget_per_day: 10_000,
        gas_refill_per_day: 10_000,
        max_intents_per_second: 10,
        max_intents_per_day: 100,
        max_volume_per_day: 1_000_000,
        max_drawdown_pct: 50,
        ..PolicyTerms::default()
    }
}

fn sample_active_policy(policy_id: u64, grantee: [u8; 32]) -> PolicyRecord {
    PolicyRecord::new(
        policy_id,
        99,
        grantee,
        1_000,
        1_000_000,
        PolicyState::Active,
        sample_policy_terms(),
    )
}

fn market_word(label: &str) -> [u8; 32] {
    let mut word = [0_u8; 32];
    word[..label.len()].copy_from_slice(label.as_bytes());
    word
}

fn encode_single_bytes_calldata(selector: [u8; 4], bytes: &[u8]) -> Vec<u8> {
    let mut calldata = Vec::with_capacity(4 + 64 + bytes.len().div_ceil(32) * 32);
    calldata.extend_from_slice(&selector);
    calldata.extend_from_slice(&abi_word_from_u64(32));
    calldata.extend_from_slice(&abi_word_from_u64(bytes.len() as u64));
    calldata.extend_from_slice(bytes);
    let padding = (32 - (bytes.len() % 32)) % 32;
    calldata.resize(calldata.len() + padding, 0);
    calldata
}

fn encode_attestation_verify_calldata(
    vendor: u8,
    quote_bytes: &[u8],
    nonce: Hash,
    freshness_anchor_block: u64,
    expected_claim: &[u8],
) -> Vec<u8> {
    fn push_dynamic_bytes(encoded: &mut Vec<u8>, bytes: &[u8]) {
        encoded.extend_from_slice(&abi_word_from_u64(bytes.len() as u64));
        encoded.extend_from_slice(bytes);
        let padding = (32 - (bytes.len() % 32)) % 32;
        encoded.resize(encoded.len() + padding, 0);
    }

    let quote_offset = 32 * 5;
    let quote_tail_len = 32 + quote_bytes.len().div_ceil(32) * 32;
    let expected_claim_offset = quote_offset + quote_tail_len;

    let mut calldata =
        Vec::with_capacity(4 + expected_claim_offset + 32 + expected_claim.len().div_ceil(32) * 32);
    calldata.extend_from_slice(&ATTESTATION_VERIFY_SELECTOR);
    calldata.extend_from_slice(&abi_word_from_u64(vendor as u64));
    calldata.extend_from_slice(&abi_word_from_u64(quote_offset as u64));
    calldata.extend_from_slice(&nonce);
    calldata.extend_from_slice(&abi_word_from_u64(freshness_anchor_block));
    calldata.extend_from_slice(&abi_word_from_u64(expected_claim_offset as u64));
    push_dynamic_bytes(&mut calldata, quote_bytes);
    push_dynamic_bytes(&mut calldata, expected_claim);
    calldata
}

fn attestation_claim_bytes() -> Vec<u8> {
    borsh::to_vec(&AttestationVerifyClaim::RunsCodeHash {
        code_hash: [4; 32],
        model_hash: Some([5; 32]),
    })
    .expect("attestation claim should serialize")
}

fn seed_attestation_vendor_roots<Checkpoint: StateCheckpoint>(
    checkpoint: &mut Checkpoint,
    vendor: AttestationVerifyVendor,
    valid_from_block: u64,
    valid_until_block: Option<u64>,
) {
    let active_roots = AttestationVendorRootsState {
        vendor,
        roots: vec![AttestationVendorRoot {
            root_id: [7; 32],
            valid_from_block,
            valid_until_block,
            payload: vendor_root_payload_bytes(vendor),
        }],
    };
    checkpoint.set(
        &attestation_vendor_roots_state_key(vendor),
        &borsh::to_vec(&active_roots).expect("roots state should serialize"),
    );
}

fn vendor_root_payload_bytes(vendor: AttestationVerifyVendor) -> Vec<u8> {
    let payload = match vendor {
        AttestationVerifyVendor::IntelSgxDcap => AttestationVendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![1],
            tcb_info_signing_cert_der: vec![2],
            tcb_info_url: "https://example.invalid/tcb".to_string(),
            crl_der: vec![3],
        },
        AttestationVerifyVendor::AmdSevSnp => AttestationVendorRootPayload::AmdSevRoot {
            ark_cert_der: vec![1],
            ask_cert_der: vec![2],
            crl_der: vec![3],
        },
        AttestationVerifyVendor::AwsNitro => AttestationVendorRootPayload::AwsNitroRoot {
            root_cert_der: vec![1],
        },
        AttestationVerifyVendor::AppleSecureEnclave => {
            AttestationVendorRootPayload::AppleSecureEnclaveRoot {
                root_cert_der: vec![1],
            }
        }
    };
    borsh::to_vec(&payload).expect("vendor root payload should serialize")
}

fn seed_evm_block_hash<Checkpoint: StateCheckpoint>(
    checkpoint: &mut Checkpoint,
    block: u64,
    hash: Hash,
) {
    checkpoint.set(evm_block_hash_key(block).as_bytes(), &hash);
}

fn encode_words_calldata(selector: [u8; 4], words: &[[u8; 32]]) -> Vec<u8> {
    let mut calldata = Vec::with_capacity(4 + words.len() * 32);
    calldata.extend_from_slice(&selector);
    for word in words {
        calldata.extend_from_slice(word);
    }
    calldata
}

fn encode_address_u256_calldata(selector: [u8; 4], address: Address, amount: U256) -> Vec<u8> {
    encode_words_calldata(
        selector,
        &[abi_word_from_address(address), amount.to_be_bytes::<32>()],
    )
}

fn encode_two_address_u256_calldata(
    selector: [u8; 4],
    first: Address,
    second: Address,
    amount: U256,
) -> Vec<u8> {
    encode_words_calldata(
        selector,
        &[
            abi_word_from_address(first),
            abi_word_from_address(second),
            amount.to_be_bytes::<32>(),
        ],
    )
}

fn decode_word_u64(encoded: &[u8], word_index: usize) -> u64 {
    let start = word_index * 32;
    decode_u64_word(&encoded[start..start + 32]).expect("word should decode")
}

fn decode_hash_word(encoded: &[u8], word_index: usize) -> Hash {
    let start = word_index * 32;
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&encoded[start..start + 32]);
    hash
}

fn decode_attestation_verify_failure(value: u64) -> AttestationVerifyFailure {
    match value {
        0 => AttestationVerifyFailure::None,
        1 => AttestationVerifyFailure::StaleNonce,
        2 => AttestationVerifyFailure::UnknownVendorRoot,
        3 => AttestationVerifyFailure::CertChainInvalid,
        4 => AttestationVerifyFailure::QuoteSignatureInvalid,
        5 => AttestationVerifyFailure::QuoteFormatVersionUnsupported,
        6 => AttestationVerifyFailure::ClaimMismatch,
        7 => AttestationVerifyFailure::QuoteTooLarge,
        8 => AttestationVerifyFailure::Revoked,
        other => panic!("unknown attestation verifier failure discriminant {other}"),
    }
}

fn decode_attestation_verify_outcome_words(encoded: &[u8]) -> AttestationVerifyOutcome {
    assert_eq!(encoded.len(), 32 * 8);
    AttestationVerifyOutcome {
        verified: decode_word_u64(encoded, 0) != 0,
        code_hash: decode_hash_word(encoded, 1),
        model_hash: decode_hash_word(encoded, 2),
        enclave_id: decode_hash_word(encoded, 3),
        signer: decode_hash_word(encoded, 4),
        nonce: decode_hash_word(encoded, 5),
        vendor_root_id: decode_hash_word(encoded, 6),
        failure_reason: decode_attestation_verify_failure(decode_word_u64(encoded, 7)),
    }
}

fn canonical_selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    let mut selector = [0_u8; 4];
    selector.copy_from_slice(&hash.as_slice()[..4]);
    selector
}

fn seed_market_state<Checkpoint: StateCheckpoint>(
    checkpoint: &mut Checkpoint,
    market_id: u64,
    label: &str,
) {
    let mut state = ProtocolStateAccessor::new(checkpoint);
    let mut book = SingleMarketBook::new(market_id);
    book.submit(
        OrderRequest {
            market_id,
            account_id: 10,
            client_order_id: 501,
            side: OrderSide::Bid,
            quantity_lots: 3,
            kind: MatchingOrderKind::Limit { price_ticks: 101 },
            time_in_force: TimeInForce::Gtc,
            post_only: false,
        },
        1_000,
    )
    .expect("bid should rest");
    book.submit(
        OrderRequest {
            market_id,
            account_id: 11,
            client_order_id: 601,
            side: OrderSide::Ask,
            quantity_lots: 4,
            kind: MatchingOrderKind::Limit { price_ticks: 103 },
            time_in_force: TimeInForce::Gtc,
            post_only: false,
        },
        1_000,
    )
    .expect("ask should rest");
    state.set_market_label(label, market_id);
    state.set_order_book(market_id, &book);
    state.set_book_sequence(market_id, 9);
    state.set_oracle_mark(
        market_id,
        &OracleMarkRecord {
            price_ticks: 102,
            oracle_tick: 100,
            updated_at_block: 42,
            timestamp_ms: 1_000,
        },
    );
    state.set_oracle_config(
        market_id,
        &OracleConfig {
            max_staleness_ms: 10_000,
        },
    );
    state.set_market_paused(market_id, false);
}

fn signed_place_order_intent(
    signer: &SigningKey,
    policy_id: u64,
    market_id: u64,
    nonce: u64,
) -> SignedIntent {
    let intent = Intent {
        chain_id: 7,
        account: 99,
        policy_id,
        nonce,
        expires_at: 50_000,
        predicate: None,
        kind: IntentKind::PlaceOrder {
            market_id,
            client_order_id: 77,
            side: IntentOrderSide::Bid,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 101 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    };
    let payload = SessionPayload {
        intent_hash: intent_signing_hash(&intent),
        policy_id,
        chain_id: 7,
    };
    let signature = signer.sign(&borsh::to_vec(&payload).expect("payload should serialize"));

    SignedIntent {
        intent,
        signature: signature.to_bytes(),
    }
}

fn sample_body() -> BlockBody {
    BlockBody {
        items: vec![
            BlockItem::Intent(SignedIntent {
                intent: Intent {
                    chain_id: 7,
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
                bytes: vec![4, 5, 6, 7],
            }),
        ],
    }
}

fn sample_event_root() -> [u8; 32] {
    let tx = SignedTransaction {
        signer: validator(9),
        bytes: vec![4, 5, 6, 7],
    };
    let mut payload = Vec::new();
    payload.extend_from_slice(validator_to_address(tx.signer).as_slice());
    payload.extend_from_slice(&(21_000_u64 + tx.bytes.len() as u64).to_be_bytes());
    payload.extend_from_slice(&(tx.bytes.len() as u64).to_be_bytes());

    compute_event_root(&[
        ExecutionEvent {
            topic: "intent_applied",
            payload: borsh::to_vec(&SignedIntent {
                intent: Intent {
                    chain_id: 7,
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
            })
            .expect("intent serialization should succeed"),
        },
        ExecutionEvent {
            topic: "transaction_executed",
            payload,
        },
    ])
}

fn sample_block(parent_state_root: [u8; 32], state_root: [u8; 32]) -> Block {
    let body = sample_body();
    Block {
        header: BlockHeader {
            chain_id: 7,
            height: 1,
            round: 1,
            proposer: validator(1),
            parent_hash: [1; 32],
            parent_state_root,
            tx_root: body_root(&body),
            event_root: sample_event_root(),
            state_root,
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            timestamp_ms: 1_000,
        },
        body,
    }
}

#[test]
fn reports_component_name() {
    assert_eq!(component_name(), "execution");
}

#[test]
fn constructs_revm_transaction_env() {
    let shell = RevmShell;
    let context = super::ExecutionContext {
        chain_id: 7,
        height: 9,
        timestamp_ms: 777,
        parent_state_root: [0; 32],
    };
    let tx = SignedTransaction {
        signer: validator(3),
        bytes: vec![1, 2, 3, 4],
    };

    let env = shell.transaction_env(&context, 2, &tx);

    assert_eq!(env.cfg.chain_id, 7);
    assert_eq!(env.block.number, U256::from(9_u64));
    assert_eq!(env.tx.caller, validator_to_address(validator(3)));
    assert_eq!(env.tx.chain_id, Some(7));
}

#[test]
fn replays_and_commits_block_deterministically() {
    let preview_store = InMemoryStateStore::new();
    let parent_state_root = preview_store.committed_root();
    let provisional_block = sample_block(parent_state_root, [0; 32]);
    let preview_executor = BlockExecutor::new(7, preview_store.clone());
    let replay_result = preview_executor
        .replay_block(&provisional_block)
        .expect_err("placeholder root should mismatch");
    assert!(matches!(
        replay_result,
        BlockError::StateRootMismatch { .. }
    ));

    let mut checkpoint = preview_store.checkpoint();
    checkpoint.set(
        b"intent/1/7/1",
        &borsh::to_vec(&Intent {
            chain_id: 7,
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
        })
        .expect("intent serialization should succeed"),
    );
    checkpoint.set(b"tx/1/1", &[4, 5, 6, 7]);
    checkpoint.set(evm_block_hash_key(0).as_bytes(), &[1; 32]);
    let expected_root = checkpoint.compute_root();

    let block = sample_block(parent_state_root, expected_root);
    let preview_result = preview_executor
        .replay_block(&block)
        .expect("replay should succeed");

    let mut executor = BlockExecutor::new(7, preview_store);
    let committed_result = executor
        .execute_block(&block)
        .expect("commit should succeed");

    assert_eq!(preview_result, committed_result);
    assert_eq!(
        executor
            .state_store()
            .committed_entries()
            .get(b"intent/1/7/1".as_slice()),
        Some(
            &borsh::to_vec(&Intent {
                chain_id: 7,
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
            })
            .expect("intent serialization should succeed")
        )
    );
    assert_eq!(executor.state_store().committed_root(), expected_root);
}

#[test]
fn rejects_parent_root_mismatch_before_execution() {
    let store = InMemoryStateStore::new();
    let block = sample_block([9; 32], [1; 32]);
    let executor = BlockExecutor::new(7, store);

    let err = executor
        .replay_block(&block)
        .expect_err("parent root mismatch should fail");

    assert!(matches!(err, BlockError::ParentStateRootMismatch { .. }));
}

#[test]
fn rejects_block_with_bad_tx_root() {
    let store = InMemoryStateStore::new();
    let parent_root = store.committed_root();
    let mut block = sample_block(parent_root, [1; 32]);
    block.header.tx_root = [2; 32];
    let executor = BlockExecutor::new(7, store);

    let err = executor
        .replay_block(&block)
        .expect_err("bad tx root should fail");

    assert_eq!(err, BlockError::AdmissionViolation("tx_root mismatch"));
}

#[test]
fn prepares_block_with_canonical_roots() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let genesis = Block {
        header: BlockHeader {
            chain_id: 7,
            height: 0,
            round: 0,
            proposer: validator(1),
            parent_hash: [0; 32],
            parent_state_root: executor.committed_root(),
            tx_root: body_root(&BlockBody { items: vec![] }),
            event_root: compute_event_root(&[]),
            state_root: executor.committed_root(),
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            timestamp_ms: 0,
        },
        body: BlockBody { items: vec![] },
    };

    let PreparedBlock { block, result, .. } = executor
        .prepare_block(
            &genesis,
            1,
            validator(2),
            QuorumCertificate {
                view: 0,
                block_hash: block_hash(&genesis),
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            sample_body(),
            1_000,
        )
        .expect("block preparation should succeed");

    assert_eq!(block.header.state_root, result.state_root);
    assert_eq!(block.header.event_root, result.event_root);
    assert_eq!(block.header.tx_root, result.tx_root);
    assert_eq!(block.header.parent_hash, block_hash(&genesis));
}

#[test]
fn block_execution_stages_parent_hash_for_evm_state() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let genesis = Block {
        header: BlockHeader {
            chain_id: 7,
            height: 0,
            round: 0,
            proposer: validator(1),
            parent_hash: [0; 32],
            parent_state_root: executor.committed_root(),
            tx_root: body_root(&BlockBody { items: vec![] }),
            event_root: compute_event_root(&[]),
            state_root: executor.committed_root(),
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            timestamp_ms: 0,
        },
        body: BlockBody { items: vec![] },
    };

    let prepared = executor
        .prepare_block_with_checkpoint(
            &genesis,
            1,
            validator(2),
            QuorumCertificate {
                view: 0,
                block_hash: block_hash(&genesis),
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            BlockBody { items: vec![] },
            1_000,
        )
        .expect("block preparation should stage parent hash");

    assert_eq!(
        prepared
            .checkpoint
            .get(evm_block_hash_key(0).as_bytes())
            .as_deref(),
        Some(block_hash(&genesis).as_slice())
    );
}

#[test]
fn block_execution_prunes_stale_block_hash_mirror_entries() {
    let store = InMemoryStateStore::new();
    let mut checkpoint = store.checkpoint();
    let stale_height = 1;
    let retained_height = 2;
    let context = ExecutionContext {
        chain_id: 7,
        height: RETAINED_BLOCK_HASH_WINDOW_BLOCKS + 2,
        timestamp_ms: 1_000,
        parent_state_root: [0; 32],
    };
    let parent_height = context.height - 1;
    checkpoint.set(evm_block_hash_key(stale_height).as_bytes(), &[1; 32]);
    checkpoint.set(evm_block_hash_key(retained_height).as_bytes(), &[2; 32]);

    stage_parent_block_hash(&context, [9_u8; 32], &mut checkpoint);

    assert_eq!(
        checkpoint.get(evm_block_hash_key(stale_height).as_bytes()),
        None
    );
    assert_eq!(
        checkpoint
            .get(evm_block_hash_key(retained_height).as_bytes())
            .as_deref(),
        Some([2_u8; 32].as_slice())
    );
    assert_eq!(
        checkpoint
            .get(evm_block_hash_key(parent_height).as_bytes())
            .as_deref(),
        Some([9_u8; 32].as_slice())
    );
}

#[test]
fn collateral_intents_settle_through_market_custody_in_state_root() {
    let mut store = InMemoryStateStore::new();
    let account_id = 99;
    let address = address_from_u64(account_id);
    let market_custody_address = Address::from(super::isolated_margin_market_custody_address(7));
    let mut seed = store.checkpoint();
    seed.set(
        account_evm_address_key(account_id).as_bytes(),
        address.as_slice(),
    );
    seed.set(
        policy_usdc_balance_key(address).as_bytes(),
        &U256::from(100_u64).to_be_bytes::<32>(),
    );
    let seeded_root = store.commit(seed).expect("seed commit should succeed");
    let mut executor = BlockExecutor::new(7, store);
    let genesis = Block {
        header: BlockHeader {
            chain_id: 7,
            height: 0,
            round: 0,
            proposer: validator(1),
            parent_hash: [0; 32],
            parent_state_root: seeded_root,
            tx_root: body_root(&BlockBody { items: vec![] }),
            event_root: compute_event_root(&[]),
            state_root: seeded_root,
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![],
            },
            timestamp_ms: 1_000,
        },
        body: BlockBody { items: vec![] },
    };
    let qc = QuorumCertificate {
        view: 0,
        block_hash: block_hash(&genesis),
        signatures: vec![Signature {
            validator: validator(1),
            bytes: signature(1),
        }],
    };
    let collateral_intent = |nonce, kind| {
        BlockItem::Intent(SignedIntent {
            intent: Intent {
                chain_id: 7,
                account: account_id,
                policy_id: 7,
                nonce,
                expires_at: 10_000,
                predicate: None,
                kind,
            },
            signature: [1; 64],
        })
    };
    let mut pre_deposit_checkpoint = executor.state_store.checkpoint();
    let policy_custody_sum_before_deposit =
        super::read_policy_usdc_balance(&mut pre_deposit_checkpoint, address)
            + super::read_policy_usdc_balance(&mut pre_deposit_checkpoint, market_custody_address);

    let mut deposit = executor
        .prepare_block_with_checkpoint(
            &genesis,
            1,
            validator(2),
            qc.clone(),
            BlockBody {
                items: vec![collateral_intent(
                    1,
                    IntentKind::DepositCollateral {
                        market_id: 7,
                        amount_usdc: 60,
                    },
                )],
            },
            1_100,
        )
        .expect("deposit block should prepare");
    assert_eq!(
        deposit
            .checkpoint
            .get(policy_usdc_balance_key(address).as_bytes()),
        Some(U256::from(40_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        deposit
            .checkpoint
            .get(policy_usdc_balance_key(market_custody_address).as_bytes()),
        Some(U256::from(60_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        super::read_policy_usdc_balance(&mut deposit.checkpoint, address)
            + super::read_policy_usdc_balance(&mut deposit.checkpoint, market_custody_address),
        policy_custody_sum_before_deposit
    );
    let deposit_block = deposit.block.clone();
    executor
        .commit_prepared_block(deposit)
        .expect("deposit block should commit");

    let mut pre_withdraw_checkpoint = executor.state_store.checkpoint();
    let policy_custody_sum_before_withdraw =
        super::read_policy_usdc_balance(&mut pre_withdraw_checkpoint, address)
            + super::read_policy_usdc_balance(&mut pre_withdraw_checkpoint, market_custody_address);
    let mut withdraw = executor
        .prepare_block_with_checkpoint(
            &deposit_block,
            2,
            validator(2),
            QuorumCertificate {
                view: 1,
                block_hash: block_hash(&deposit_block),
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            BlockBody {
                items: vec![collateral_intent(
                    2,
                    IntentKind::WithdrawCollateral {
                        market_id: 7,
                        amount_usdc: 25,
                    },
                )],
            },
            1_200,
        )
        .expect("withdraw block should prepare");
    assert_eq!(
        withdraw
            .checkpoint
            .get(policy_usdc_balance_key(address).as_bytes()),
        Some(U256::from(65_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        withdraw
            .checkpoint
            .get(policy_usdc_balance_key(market_custody_address).as_bytes()),
        Some(U256::from(35_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        super::read_policy_usdc_balance(&mut withdraw.checkpoint, address)
            + super::read_policy_usdc_balance(&mut withdraw.checkpoint, market_custody_address),
        policy_custody_sum_before_withdraw
    );
    let withdraw_block = withdraw.block.clone();
    executor
        .commit_prepared_block(withdraw)
        .expect("withdraw block should commit");

    let mut pre_full_withdraw_checkpoint = executor.state_store.checkpoint();
    let policy_custody_sum_before_full_withdraw =
        super::read_policy_usdc_balance(&mut pre_full_withdraw_checkpoint, address)
            + super::read_policy_usdc_balance(
                &mut pre_full_withdraw_checkpoint,
                market_custody_address,
            );
    let mut full_withdraw = executor
        .prepare_block_with_checkpoint(
            &withdraw_block,
            3,
            validator(2),
            QuorumCertificate {
                view: 2,
                block_hash: block_hash(&withdraw_block),
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            BlockBody {
                items: vec![collateral_intent(
                    3,
                    IntentKind::WithdrawCollateral {
                        market_id: 7,
                        amount_usdc: 35,
                    },
                )],
            },
            1_300,
        )
        .expect("full withdraw block should prepare");
    assert_eq!(
        full_withdraw
            .checkpoint
            .get(policy_usdc_balance_key(address).as_bytes()),
        Some(U256::from(100_u64).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        full_withdraw
            .checkpoint
            .get(policy_usdc_balance_key(market_custody_address).as_bytes()),
        None
    );
    assert_eq!(
        super::read_policy_usdc_balance(&mut full_withdraw.checkpoint, address)
            + super::read_policy_usdc_balance(
                &mut full_withdraw.checkpoint,
                market_custody_address
            ),
        policy_custody_sum_before_full_withdraw
    );
}

#[test]
fn twin_validators_prepare_same_roots() {
    let left = BlockExecutor::new(7, InMemoryStateStore::new());
    let right = BlockExecutor::new(7, InMemoryStateStore::new());
    let genesis = Block {
        header: BlockHeader {
            chain_id: 7,
            height: 0,
            round: 0,
            proposer: validator(1),
            parent_hash: [0; 32],
            parent_state_root: left.committed_root(),
            tx_root: body_root(&BlockBody { items: vec![] }),
            event_root: compute_event_root(&[]),
            state_root: left.committed_root(),
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![Signature {
                    validator: validator(1),
                    bytes: signature(1),
                }],
            },
            timestamp_ms: 0,
        },
        body: BlockBody { items: vec![] },
    };
    let qc = QuorumCertificate {
        view: 0,
        block_hash: block_hash(&genesis),
        signatures: vec![Signature {
            validator: validator(1),
            bytes: signature(1),
        }],
    };

    let left_prepared = left
        .prepare_block(&genesis, 1, validator(2), qc.clone(), sample_body(), 1_000)
        .expect("left preparation should succeed");
    let right_prepared = right
        .prepare_block(&genesis, 1, validator(2), qc, sample_body(), 1_000)
        .expect("right preparation should succeed");

    assert_eq!(
        left_prepared.block.header.tx_root,
        right_prepared.block.header.tx_root
    );
    assert_eq!(
        left_prepared.block.header.event_root,
        right_prepared.block.header.event_root
    );
    assert_eq!(
        left_prepared.block.header.state_root,
        right_prepared.block.header.state_root
    );
    assert_eq!(left_prepared.result, right_prepared.result);
}

#[test]
fn quarantines_bad_block_and_keeps_store_clean() {
    let store = InMemoryStateStore::new();
    let parent_root = store.committed_root();
    let mut block = sample_block(parent_root, [9; 32]);
    let block_id = block_hash(&block);
    let mut executor = BlockExecutor::new(7, store);

    let err = executor
        .verify_block(&block)
        .expect_err("bad block should be quarantined");

    assert!(matches!(err, BlockError::StateRootMismatch { .. }));
    assert_eq!(
        executor
            .quarantined_blocks()
            .get(&block_id)
            .expect("quarantine entry should exist")
            .reason,
        QuarantineReason::StateRootMismatch
    );
    assert_eq!(executor.state_store().committed_root(), parent_root);
    assert!(executor.state_store().committed_entries().is_empty());

    block.header.state_root = sample_block(parent_root, [0; 32]).header.state_root;
}

#[test]
fn recovers_after_quarantined_block_by_executing_clean_block() {
    let store = InMemoryStateStore::new();
    let parent_root = store.committed_root();
    let mut executor = BlockExecutor::new(7, store);
    let bad_block = sample_block(parent_root, [9; 32]);
    let bad_hash = block_hash(&bad_block);

    executor
        .verify_block(&bad_block)
        .expect_err("bad block should be quarantined");

    let mut checkpoint = executor.state_store().checkpoint();
    checkpoint.set(
        b"intent/1/7/1",
        &borsh::to_vec(&Intent {
            chain_id: 7,
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
        })
        .expect("intent serialization should succeed"),
    );
    checkpoint.set(b"tx/1/1", &[4, 5, 6, 7]);
    checkpoint.set(evm_block_hash_key(0).as_bytes(), &[1; 32]);
    let expected_root = checkpoint.compute_root();
    let good_block = sample_block(parent_root, expected_root);

    let result = executor
        .execute_block(&good_block)
        .expect("good block should execute after quarantine");

    assert_eq!(result.state_root, expected_root);
    assert!(executor.clear_quarantine(&bad_hash).is_some());
    assert!(executor.quarantined_blocks().is_empty());
    assert_eq!(executor.state_store().committed_root(), expected_root);
}

#[test]
fn precompile_registry_is_canonical_and_resolves_session_key() {
    let registry = protocol_precompiles();

    assert_eq!(registry.len(), 6);
    assert_eq!(registry[0].id, 1);
    assert_eq!(registry[1].id, 2);
    assert_eq!(registry[2].id, 3);
    assert_eq!(registry[3].id, 4);
    assert_eq!(registry[4].id, 5);
    assert_eq!(registry[5].id, 6);
    assert_eq!(registry[0].address, address_from_u64(0x1002));
    assert_eq!(registry[1].address, address_from_u64(0x1003));
    assert_eq!(registry[2].address, address_from_u64(0x1004));
    assert_eq!(registry[3].address, address_from_u64(0x1005));
    assert_eq!(registry[4].address, evm_usdc_system_address());
    assert_eq!(registry[5].address, address_from_u64(0x1007));
    assert_eq!(registry[0].mutability, PrecompileMutability::Stateful);
    assert_eq!(registry[2].mutability, PrecompileMutability::View);
    assert_eq!(registry[5].mutability, PrecompileMutability::View);
    assert_eq!(
        precompile_by_address(address_from_u64(0x1002), 1)
            .expect("session key precompile should resolve")
            .name,
        "session_key"
    );
    assert_eq!(
        precompile_by_address(address_from_u64(0x1004), 1)
            .expect("clob read precompile should resolve")
            .name,
        "clob_read"
    );
    assert_eq!(
        precompile_by_address(evm_usdc_system_address(), 1)
            .expect("evm usdc system contract should resolve")
            .name,
        "evm_usdc_system"
    );
    assert_eq!(
        precompile_by_address(address_from_u64(0x1007), 1)
            .expect("attestation verifier precompile should resolve")
            .name,
        "attestation_verify"
    );
}

#[test]
fn evm_selectors_match_canonical_signature_hashes() {
    for (signature, selector) in [
        ("submitSignedIntent(bytes)", SUBMIT_SIGNED_INTENT_SELECTOR),
        ("bestBidAsk(bytes32)", BEST_BID_ASK_SELECTOR),
        ("depth(bytes32,uint8,uint16)", DEPTH_SELECTOR),
        ("markPrice(bytes32)", MARK_PRICE_SELECTOR),
        (
            "attestationVerify(uint8,bytes,bytes32,uint64,bytes)",
            ATTESTATION_VERIFY_SELECTOR,
        ),
        (
            "supportsInterface(bytes4)",
            ERC165_SUPPORTS_INTERFACE_SELECTOR,
        ),
        ("name()", ERC20_NAME_SELECTOR),
        ("symbol()", ERC20_SYMBOL_SELECTOR),
        ("decimals()", ERC20_DECIMALS_SELECTOR),
        ("totalSupply()", ERC20_TOTAL_SUPPLY_SELECTOR),
        ("balanceOf(address)", ERC20_BALANCE_OF_SELECTOR),
        ("transfer(address,uint256)", ERC20_TRANSFER_SELECTOR),
        ("allowance(address,address)", ERC20_ALLOWANCE_SELECTOR),
        ("approve(address,uint256)", ERC20_APPROVE_SELECTOR),
        (
            "transferFrom(address,address,uint256)",
            ERC20_TRANSFER_FROM_SELECTOR,
        ),
        ("evmDeposit(uint256)", EVM_USDC_DEPOSIT_SELECTOR),
        ("evmWithdraw(uint256)", EVM_USDC_WITHDRAW_SELECTOR),
        (
            "evmDepositFor(address,uint256)",
            EVM_USDC_DEPOSIT_FOR_SELECTOR,
        ),
    ] {
        assert_eq!(canonical_selector(signature), selector, "{signature}");
    }
}

#[test]
fn dispatches_read_and_stateful_precompiles_deterministically() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    seed_market_state(&mut checkpoint, 7, "BTC-USD");

    let read_output = executor
        .dispatch_precompile(
            &context,
            PrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1004),
                selector: BEST_BID_ASK_SELECTOR,
                input: &abi_word_from_u64(7),
                read_only: true,
            },
            &mut checkpoint,
        )
        .expect("read precompile should succeed");

    assert_eq!(decode_word_u64(&read_output.returned, 0), 101);
    assert_eq!(decode_word_u64(&read_output.returned, 1), 3);
    assert_eq!(decode_word_u64(&read_output.returned, 2), 103);
    assert_eq!(decode_word_u64(&read_output.returned, 3), 4);
    assert_eq!(decode_word_u64(&read_output.returned, 4), 9);
    assert_eq!(read_output.gas_used, 0);
    assert!(!read_output.reverted);

    let write_output = executor
        .dispatch_precompile(
            &context,
            PrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1002),
                selector: SESSION_KEY_AUTHORIZE,
                input: &7_u64.to_be_bytes(),
                read_only: false,
            },
            &mut checkpoint,
        )
        .expect("stateful precompile should succeed");

    assert_eq!(write_output.returned, 7_u64.to_be_bytes().to_vec());
    assert_eq!(
        checkpoint.get(b"precompile/session-key/7"),
        Some(7_u64.to_be_bytes().to_vec())
    );
}

#[test]
fn attestation_verify_precompile_returns_deterministic_scaffold_outcomes() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    let nonce = [9_u8; 32];
    let calldata = encode_attestation_verify_calldata(0, &[1, 2, 3], nonce, 41, &[]);

    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("attestation verifier precompile should resolve");

    assert!(!output.reverted);
    assert_eq!(output.returned.len(), 32 * 8);
    assert_eq!(decode_word_u64(&output.returned, 0), 0);
    assert_eq!(&output.returned[5 * 32..6 * 32], nonce.as_slice());
    assert_eq!(
        decode_word_u64(&output.returned, 7),
        AttestationVerifyFailure::UnknownVendorRoot as u64
    );

    let oversized_quote = vec![0_u8; MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP + 1];
    let oversized_calldata =
        encode_attestation_verify_calldata(0, &oversized_quote, nonce, 41, &[]);
    let oversized_output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &oversized_calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("oversized quote should return a typed failure");

    assert!(!oversized_output.reverted);
    assert_eq!(
        decode_word_u64(&oversized_output.returned, 7),
        AttestationVerifyFailure::QuoteTooLarge as u64
    );
}

#[test]
fn attestation_verify_precompile_reads_active_vendor_root_state() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    let vendor = AttestationVerifyVendor::IntelSgxDcap;
    seed_attestation_vendor_roots(&mut checkpoint, vendor, 40, Some(45));
    seed_evm_block_hash(&mut checkpoint, 41, [9_u8; 32]);
    let expected_claim = attestation_claim_bytes();
    let calldata = encode_attestation_verify_calldata(
        vendor.as_abi(),
        &[1, 2, 3],
        [9_u8; 32],
        41,
        &expected_claim,
    );

    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("attestation verifier should read active roots");

    assert!(!output.reverted);
    assert_eq!(
        decode_word_u64(&output.returned, 7),
        AttestationVerifyFailure::QuoteFormatVersionUnsupported as u64
    );
}

#[test]
fn attestation_verify_helper_matches_evm_precompile_outcome() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let vendor = AttestationVerifyVendor::IntelSgxDcap;
    let quote_bytes = [1_u8, 2, 3];
    let nonce = [9_u8; 32];
    let freshness_anchor_block = 41;
    let expected_claim = attestation_claim_bytes();
    let mut helper_checkpoint = executor.state_store().checkpoint();
    seed_attestation_vendor_roots(&mut helper_checkpoint, vendor, 40, Some(45));
    seed_evm_block_hash(&mut helper_checkpoint, freshness_anchor_block, nonce);
    let mut evm_checkpoint = helper_checkpoint.clone();

    let helper_outcome = verify_attestation_quote(
        &context,
        &mut helper_checkpoint,
        AttestationVerifyRequest {
            vendor,
            quote_bytes: &quote_bytes,
            nonce,
            freshness_anchor_block,
            expected_claim: &expected_claim,
        },
    );
    let calldata = encode_attestation_verify_calldata(
        vendor.as_abi(),
        &quote_bytes,
        nonce,
        freshness_anchor_block,
        &expected_claim,
    );
    let evm_output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut evm_checkpoint,
        )
        .expect("attestation verifier precompile should resolve");

    assert!(!evm_output.reverted);
    assert_eq!(
        decode_attestation_verify_outcome_words(&evm_output.returned),
        helper_outcome
    );
    assert_eq!(
        helper_outcome.failure_reason,
        AttestationVerifyFailure::QuoteFormatVersionUnsupported
    );
}

#[test]
fn attestation_verify_failure_rejection_reasons_are_stable() {
    let cases = [
        (AttestationVerifyFailure::None, None),
        (
            AttestationVerifyFailure::StaleNonce,
            Some("identity_tee_attestation_stale_nonce"),
        ),
        (
            AttestationVerifyFailure::UnknownVendorRoot,
            Some("identity_tee_attestation_unknown_vendor_root"),
        ),
        (
            AttestationVerifyFailure::CertChainInvalid,
            Some("identity_tee_attestation_cert_chain_invalid"),
        ),
        (
            AttestationVerifyFailure::QuoteSignatureInvalid,
            Some("identity_tee_attestation_quote_signature_invalid"),
        ),
        (
            AttestationVerifyFailure::QuoteFormatVersionUnsupported,
            Some("identity_tee_attestation_quote_format_version_unsupported"),
        ),
        (
            AttestationVerifyFailure::ClaimMismatch,
            Some("identity_tee_attestation_claim_mismatch"),
        ),
        (
            AttestationVerifyFailure::QuoteTooLarge,
            Some("identity_tee_attestation_quote_too_large"),
        ),
        (
            AttestationVerifyFailure::Revoked,
            Some("identity_tee_attestation_revoked"),
        ),
    ];
    for (failure, expected) in cases {
        assert_eq!(failure.identity_action_rejection_reason(), expected);
    }
}

#[test]
fn attestation_verify_rejects_malformed_expected_claim_with_active_root() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    let vendor = AttestationVerifyVendor::IntelSgxDcap;
    seed_attestation_vendor_roots(&mut checkpoint, vendor, 40, Some(45));
    seed_evm_block_hash(&mut checkpoint, 41, [9_u8; 32]);
    let calldata =
        encode_attestation_verify_calldata(vendor.as_abi(), &[1, 2, 3], [9_u8; 32], 41, &[0xff]);

    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("attestation verifier should decode expected claims");

    assert!(!output.reverted);
    assert_eq!(
        decode_word_u64(&output.returned, 7),
        AttestationVerifyFailure::ClaimMismatch as u64
    );
}

#[test]
fn attestation_verify_rejects_malformed_active_vendor_root_payload() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let vendor = AttestationVerifyVendor::IntelSgxDcap;
    let expected_claim = attestation_claim_bytes();
    let calldata = encode_attestation_verify_calldata(
        vendor.as_abi(),
        &[1, 2, 3],
        [9_u8; 32],
        41,
        &expected_claim,
    );

    for (name, payload) in [
        ("malformed_payload", vec![0xff]),
        (
            "wrong_vendor_payload",
            vendor_root_payload_bytes(AttestationVerifyVendor::AmdSevSnp),
        ),
    ] {
        let roots = AttestationVendorRootsState {
            vendor,
            roots: vec![AttestationVendorRoot {
                root_id: [7; 32],
                valid_from_block: 40,
                valid_until_block: Some(45),
                payload,
            }],
        };
        let mut checkpoint = executor.state_store().checkpoint();
        checkpoint.set(
            &attestation_vendor_roots_state_key(vendor),
            &borsh::to_vec(&roots).expect("roots state should serialize"),
        );
        seed_evm_block_hash(&mut checkpoint, 41, [9_u8; 32]);

        let output = executor
            .dispatch_evm_precompile(
                &context,
                EvmPrecompileCall {
                    protocol_version: 1,
                    address: address_from_u64(0x1007),
                    caller: Address::ZERO,
                    calldata: &calldata,
                    read_only: true,
                    scheme: CallScheme::Call,
                },
                &mut checkpoint,
            )
            .unwrap_or_else(|_| panic!("root payload case {name} should dispatch"));

        assert!(!output.reverted, "case {name}");
        assert_eq!(
            decode_word_u64(&output.returned, 7),
            AttestationVerifyFailure::CertChainInvalid as u64,
            "case {name}"
        );
    }
}

#[test]
fn attestation_verify_enforces_freshness_anchor_state() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let vendor = AttestationVerifyVendor::IntelSgxDcap;
    let expected_claim = attestation_claim_bytes();
    let nonce = [9_u8; 32];

    for (name, height, anchor, seeded_hash) in [
        ("missing_anchor", 42, 41, None),
        ("mismatched_anchor", 42, 41, Some([8_u8; 32])),
        ("current_block_anchor", 42, 42, Some(nonce)),
        (
            "too_old_anchor",
            ATTESTATION_QUOTE_FRESHNESS_BLOCKS + 2,
            1,
            Some(nonce),
        ),
    ] {
        let context = ExecutionContext {
            chain_id: 7,
            height,
            timestamp_ms: 1_000,
            parent_state_root: executor.committed_root(),
        };
        let mut checkpoint = executor.state_store().checkpoint();
        seed_attestation_vendor_roots(&mut checkpoint, vendor, 1, None);
        if let Some(hash) = seeded_hash {
            seed_evm_block_hash(&mut checkpoint, anchor, hash);
        }
        let calldata = encode_attestation_verify_calldata(
            vendor.as_abi(),
            &[1, 2, 3],
            nonce,
            anchor,
            &expected_claim,
        );

        let output = executor
            .dispatch_evm_precompile(
                &context,
                EvmPrecompileCall {
                    protocol_version: 1,
                    address: address_from_u64(0x1007),
                    caller: Address::ZERO,
                    calldata: &calldata,
                    read_only: true,
                    scheme: CallScheme::Call,
                },
                &mut checkpoint,
            )
            .unwrap_or_else(|_| panic!("freshness case {name} should dispatch"));

        assert!(!output.reverted, "case {name}");
        assert_eq!(
            decode_word_u64(&output.returned, 7),
            AttestationVerifyFailure::StaleNonce as u64,
            "case {name}"
        );
    }

    let boundary_height = ATTESTATION_QUOTE_FRESHNESS_BLOCKS + 2;
    let boundary_anchor = boundary_height - ATTESTATION_QUOTE_FRESHNESS_BLOCKS;
    let context = ExecutionContext {
        chain_id: 7,
        height: boundary_height,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    seed_attestation_vendor_roots(&mut checkpoint, vendor, 1, None);
    seed_evm_block_hash(&mut checkpoint, boundary_anchor, nonce);
    let calldata = encode_attestation_verify_calldata(
        vendor.as_abi(),
        &[1, 2, 3],
        nonce,
        boundary_anchor,
        &expected_claim,
    );

    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("freshness boundary should dispatch");

    assert!(!output.reverted);
    assert_eq!(
        decode_word_u64(&output.returned, 7),
        AttestationVerifyFailure::QuoteFormatVersionUnsupported as u64
    );
}

#[test]
fn attestation_verify_treats_wrong_vendor_or_inactive_root_state_as_empty() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let vendor = AttestationVerifyVendor::IntelSgxDcap;
    let calldata =
        encode_attestation_verify_calldata(vendor.as_abi(), &[1, 2, 3], [9_u8; 32], 41, &[]);

    for (roots_state, expected_failure) in [
        (
            AttestationVendorRootsState {
                vendor: AttestationVerifyVendor::AmdSevSnp,
                roots: vec![AttestationVendorRoot {
                    root_id: [7; 32],
                    valid_from_block: 40,
                    valid_until_block: Some(45),
                    payload: vec![1, 2, 3],
                }],
            },
            AttestationVerifyFailure::UnknownVendorRoot,
        ),
        (
            AttestationVendorRootsState {
                vendor,
                roots: vec![AttestationVendorRoot {
                    root_id: [8; 32],
                    valid_from_block: 50,
                    valid_until_block: None,
                    payload: vec![1, 2, 3],
                }],
            },
            AttestationVerifyFailure::UnknownVendorRoot,
        ),
        (
            AttestationVendorRootsState {
                vendor,
                roots: vec![AttestationVendorRoot {
                    root_id: [9; 32],
                    valid_from_block: 1,
                    valid_until_block: Some(42),
                    payload: vec![1, 2, 3],
                }],
            },
            AttestationVerifyFailure::Revoked,
        ),
    ] {
        let mut checkpoint = executor.state_store().checkpoint();
        checkpoint.set(
            &attestation_vendor_roots_state_key(vendor),
            &borsh::to_vec(&roots_state).expect("roots state should serialize"),
        );
        let output = executor
            .dispatch_evm_precompile(
                &context,
                EvmPrecompileCall {
                    protocol_version: 1,
                    address: address_from_u64(0x1007),
                    caller: Address::ZERO,
                    calldata: &calldata,
                    read_only: true,
                    scheme: CallScheme::Call,
                },
                &mut checkpoint,
            )
            .expect("attestation verifier should return empty-root outcome");

        assert!(!output.reverted);
        assert_eq!(
            decode_word_u64(&output.returned, 7),
            expected_failure as u64
        );
    }
}

#[test]
fn attestation_verify_rejects_malformed_native_and_evm_inputs() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();

    let native_malformed = executor
        .dispatch_precompile(
            &context,
            PrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                selector: ATTESTATION_VERIFY_SELECTOR,
                input: &[],
                read_only: true,
            },
            &mut checkpoint,
        )
        .expect_err("native malformed input should fail");
    assert!(matches!(
        native_malformed,
        PrecompileError::InvalidInput(reason)
            if reason.starts_with("attestationVerify expects")
    ));

    let invalid_vendor_calldata =
        encode_attestation_verify_calldata(4, &[1, 2, 3], [9_u8; 32], 41, &[]);
    let native_invalid_vendor = executor
        .dispatch_precompile(
            &context,
            PrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                selector: ATTESTATION_VERIFY_SELECTOR,
                input: &invalid_vendor_calldata[4..],
                read_only: true,
            },
            &mut checkpoint,
        )
        .expect_err("native invalid vendor should fail");
    assert!(matches!(
        native_invalid_vendor,
        PrecompileError::InvalidInput(reason)
            if reason.starts_with("attestationVerify expects")
    ));

    let evm_invalid_vendor = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1007),
                caller: Address::ZERO,
                calldata: &invalid_vendor_calldata,
                read_only: true,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("evm invalid vendor should return structured revert");
    assert!(evm_invalid_vendor.reverted);
    assert_eq!(
        &evm_invalid_vendor.returned[..4],
        MALFORMED_CALLDATA_ERROR_SELECTOR
    );
}

#[test]
fn rejects_unknown_selector_and_stateful_read_only_calls() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 1,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();

    let selector_err = executor
        .dispatch_precompile(
            &context,
            PrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1004),
                selector: [9, 9, 9, 9],
                input: &[],
                read_only: true,
            },
            &mut checkpoint,
        )
        .expect_err("unknown selector should fail");
    assert!(matches!(
        selector_err,
        PrecompileError::UnknownSelector { .. }
    ));

    let readonly_err = executor
        .dispatch_precompile(
            &context,
            PrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1002),
                selector: SESSION_KEY_AUTHORIZE,
                input: &7_u64.to_be_bytes(),
                read_only: true,
            },
            &mut checkpoint,
        )
        .expect_err("stateful read-only dispatch should fail");
    assert_eq!(
        readonly_err,
        PrecompileError::StatefulCallDisallowed(address_from_u64(0x1002))
    );
}

#[test]
fn evm_precompile_reads_clob_and_oracle_state() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    seed_market_state(&mut checkpoint, 7, "BTC-USD");

    let best_bid_ask = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1004),
                caller: Address::ZERO,
                calldata: &encode_words_calldata(BEST_BID_ASK_SELECTOR, &[market_word("BTC-USD")]),
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("best bid/ask call should resolve");

    assert!(!best_bid_ask.reverted);
    assert_eq!(decode_word_u64(&best_bid_ask.returned, 0), 101);
    assert_eq!(decode_word_u64(&best_bid_ask.returned, 1), 3);
    assert_eq!(decode_word_u64(&best_bid_ask.returned, 2), 103);
    assert_eq!(decode_word_u64(&best_bid_ask.returned, 3), 4);
    assert_eq!(decode_word_u64(&best_bid_ask.returned, 4), 9);

    let depth = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1004),
                caller: Address::ZERO,
                calldata: &encode_words_calldata(
                    DEPTH_SELECTOR,
                    &[
                        market_word("BTC-USD"),
                        abi_word_from_u64(0),
                        abi_word_from_u64(1),
                    ],
                ),
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("depth call should resolve");

    assert!(!depth.reverted);
    assert_eq!(decode_word_u64(&depth.returned, 0), 64);
    assert_eq!(decode_word_u64(&depth.returned, 1), 9);
    assert_eq!(decode_word_u64(&depth.returned, 2), 1);
    assert_eq!(decode_word_u64(&depth.returned, 3), 101);
    assert_eq!(decode_word_u64(&depth.returned, 4), 3);
    assert_eq!(decode_word_u64(&depth.returned, 5), 1);

    let oracle = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1005),
                caller: Address::ZERO,
                calldata: &encode_words_calldata(MARK_PRICE_SELECTOR, &[market_word("BTC-USD")]),
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("oracle call should resolve");

    assert!(!oracle.reverted);
    assert_eq!(decode_word_u64(&oracle.returned, 0), 102);
    assert_eq!(decode_word_u64(&oracle.returned, 1), 100);
    assert_eq!(decode_word_u64(&oracle.returned, 2), 42);
}

#[test]
fn evm_trading_submit_accepts_signed_intent_and_persists_pending_record() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 2_000,
        parent_state_root: executor.committed_root(),
    };
    let signer = signing_key(8);
    let signed_intent = signed_place_order_intent(&signer, 7, 7, 1);
    let tracking_id = intent_id(&signed_intent);
    let intent_bytes = borsh::to_vec(&signed_intent).expect("signed intent should serialize");
    let mut checkpoint = executor.state_store().checkpoint();

    seed_market_state(&mut checkpoint, 7, "BTC-USD");
    let mut state = ProtocolStateAccessor::new(&mut checkpoint);
    let mut policy_store = PolicyStore::default();
    policy_store
        .insert(sample_active_policy(7, signer.verifying_key().to_bytes()))
        .expect("policy insertion should succeed");
    state.set_policy_store(&policy_store);

    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1003),
                caller: Address::ZERO,
                calldata: &encode_single_bytes_calldata(
                    SUBMIT_SIGNED_INTENT_SELECTOR,
                    &intent_bytes,
                ),
                read_only: false,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("submit call should resolve");

    assert!(!output.reverted);
    assert_eq!(
        decode_word_u64(&output.returned, 0),
        SubmissionStatus::Accepted as u64
    );
    assert_eq!(decode_word_u64(&output.returned, 2), 77);
    assert_eq!(
        decode_word_u64(&output.returned, 3),
        SubmissionReason::None as u64
    );
    assert_eq!(decode_word_u64(&output.returned, 4), 1);

    let pending_record = ProtocolStateAccessor::new(&mut checkpoint)
        .pending_intent(&tracking_id)
        .expect("pending intent should be stored");
    assert_eq!(pending_record.order_id, 77);
    assert_eq!(pending_record.signed_intent, signed_intent);
    assert_eq!(
        ProtocolStateAccessor::new(&mut checkpoint)
            .policy_store()
            .next_intent_nonce(7)
            .expect("policy should remain available"),
        2
    );
}

#[test]
fn evm_precompile_reverts_with_structured_errors() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let context = ExecutionContext {
        chain_id: 7,
        height: 42,
        timestamp_ms: 1_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();
    seed_market_state(&mut checkpoint, 7, "BTC-USD");

    let unknown_selector = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1004),
                caller: Address::ZERO,
                calldata: &[9, 9, 9, 9, 0, 0, 0, 0],
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("unknown selector call should resolve");
    assert!(unknown_selector.reverted);
    assert_eq!(
        &unknown_selector.returned[..4],
        &UNKNOWN_SELECTOR_ERROR_SELECTOR
    );

    let static_violation = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1003),
                caller: Address::ZERO,
                calldata: &encode_single_bytes_calldata(SUBMIT_SIGNED_INTENT_SELECTOR, &[1, 2]),
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("static context call should resolve");
    assert!(static_violation.reverted);
    assert_eq!(
        &static_violation.returned[..4],
        &STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR
    );

    let malformed = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1005),
                caller: Address::ZERO,
                calldata: &[
                    MARK_PRICE_SELECTOR[0],
                    MARK_PRICE_SELECTOR[1],
                    MARK_PRICE_SELECTOR[2],
                    MARK_PRICE_SELECTOR[3],
                    1,
                    2,
                    3,
                ],
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("malformed calldata should resolve");
    assert!(malformed.reverted);
    assert_eq!(&malformed.returned[..4], &MALFORMED_CALLDATA_ERROR_SELECTOR);

    let depth_limit = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: address_from_u64(0x1004),
                caller: Address::ZERO,
                calldata: &encode_words_calldata(
                    DEPTH_SELECTOR,
                    &[
                        market_word("BTC-USD"),
                        abi_word_from_u64(0),
                        abi_word_from_u64((MAX_DEPTH_LEVELS + 1) as u64),
                    ],
                ),
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("depth limit error should resolve");
    assert!(depth_limit.reverted);
    assert_eq!(
        decode_word_u64(&depth_limit.returned[4..], 0),
        (MAX_DEPTH_LEVELS + 1) as u64
    );
}

#[test]
fn evm_usdc_system_contract_materializes_code_and_reads_balance() {
    let executor = BlockExecutor::new(7, InMemoryStateStore::new());
    let mut checkpoint = executor.state_store().checkpoint();
    let owner = address_from_u64(0xabc);
    checkpoint.set(
        evm_usdc_balance_key(owner).as_bytes(),
        &U256::from(125_u64).to_be_bytes::<32>(),
    );

    let mut db = super::CheckpointDatabase::new(&mut checkpoint);
    let account = db
        .basic(evm_usdc_system_address())
        .expect("database lookup should succeed")
        .expect("system contract account should exist");

    assert!(!account
        .code
        .as_ref()
        .expect("code should be present")
        .original_bytes()
        .is_empty());

    let context = ExecutionContext {
        chain_id: 7,
        height: 77,
        timestamp_ms: 5_000,
        parent_state_root: executor.committed_root(),
    };
    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: evm_usdc_system_address(),
                caller: owner,
                calldata: &encode_words_calldata(
                    ERC20_BALANCE_OF_SELECTOR,
                    &[abi_word_from_address(owner)],
                ),
                read_only: true,
                scheme: CallScheme::StaticCall,
            },
            &mut checkpoint,
        )
        .expect("balanceOf should resolve");

    assert!(!output.reverted);
    assert_eq!(decode_word_u64(&output.returned, 0), 125);
}

#[test]
fn evm_usdc_deposit_approve_and_transfer_from_update_state_and_logs() {
    let owner = address_from_u64(0x1111);
    let spender = address_from_u64(0x2222);
    let recipient = address_from_u64(0x3333);
    let mut store = InMemoryStateStore::new();
    let mut seed = store.checkpoint();
    seed.set(
        policy_usdc_balance_key(owner).as_bytes(),
        &U256::from(1_000_u64).to_be_bytes::<32>(),
    );
    seed.set(evm_usdc_minter_role_key(owner).as_bytes(), &[1]);
    store.commit(seed).expect("seed commit should succeed");

    let executor = BlockExecutor::new(7, store);
    let context = ExecutionContext {
        chain_id: 7,
        height: 91,
        timestamp_ms: 9_000,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();

    let deposit = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: evm_usdc_system_address(),
                caller: owner,
                calldata: &encode_words_calldata(
                    EVM_USDC_DEPOSIT_SELECTOR,
                    &[U256::from(500_u64).to_be_bytes::<32>()],
                ),
                read_only: false,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("deposit should execute");
    assert!(!deposit.reverted);
    assert_eq!(decode_word_u64(&deposit.returned, 0), 1);
    assert_eq!(deposit.logs.len(), 2);

    let approve = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: evm_usdc_system_address(),
                caller: owner,
                calldata: &encode_address_u256_calldata(ERC20_APPROVE_SELECTOR, spender, U256::MAX),
                read_only: false,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("approve should execute");
    assert!(!approve.reverted);
    assert_eq!(approve.logs.len(), 1);

    let transfer_from = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: evm_usdc_system_address(),
                caller: spender,
                calldata: &encode_two_address_u256_calldata(
                    ERC20_TRANSFER_FROM_SELECTOR,
                    owner,
                    recipient,
                    U256::from(200_u64),
                ),
                read_only: false,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("transferFrom should execute");
    assert!(!transfer_from.reverted);
    assert_eq!(transfer_from.logs.len(), 1);

    assert_eq!(
        checkpoint.get(evm_usdc_balance_key(owner).as_bytes()),
        Some((U256::from(300_u64)).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        checkpoint.get(evm_usdc_balance_key(recipient).as_bytes()),
        Some((U256::from(200_u64)).to_be_bytes::<32>().to_vec())
    );
    assert_eq!(
        checkpoint.get(policy_usdc_balance_key(owner).as_bytes()),
        Some((U256::from(500_u64)).to_be_bytes::<32>().to_vec())
    );
}

#[test]
fn evm_usdc_deposit_for_requires_minter_role() {
    let owner = address_from_u64(0x4444);
    let recipient = address_from_u64(0x5555);
    let mut store = InMemoryStateStore::new();
    let mut seed = store.checkpoint();
    seed.set(
        policy_usdc_balance_key(owner).as_bytes(),
        &U256::from(250_u64).to_be_bytes::<32>(),
    );
    store.commit(seed).expect("seed commit should succeed");

    let executor = BlockExecutor::new(7, store);
    let context = ExecutionContext {
        chain_id: 7,
        height: 92,
        timestamp_ms: 9_500,
        parent_state_root: executor.committed_root(),
    };
    let mut checkpoint = executor.state_store().checkpoint();

    let output = executor
        .dispatch_evm_precompile(
            &context,
            EvmPrecompileCall {
                protocol_version: 1,
                address: evm_usdc_system_address(),
                caller: owner,
                calldata: &encode_address_u256_calldata(
                    EVM_USDC_DEPOSIT_FOR_SELECTOR,
                    recipient,
                    U256::from(100_u64),
                ),
                read_only: false,
                scheme: CallScheme::Call,
            },
            &mut checkpoint,
        )
        .expect("depositFor should execute");

    assert!(output.reverted);
    assert_eq!(
        &output.returned[..4],
        &super::EVM_USDC_UNAUTHORIZED_ERROR_SELECTOR
    );
}

#[test]
fn executes_eip7702_transaction_and_writes_delegation_bytecode() {
    use alloy_consensus::{
        crypto::secp256k1::{public_key_to_address, sign_message},
        SignableTransaction, TxEip7702, TxEnvelope,
    };
    use alloy_eips::{eip7702::Authorization, Encodable2718};
    use kombat_chain_consensus::{body_root, QuorumCertificate};
    use revm::primitives::{Address as RevmAddress, Bytes as RevmBytes, B256, U256};
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    use super::{evm_account_balance_key, evm_account_code_key, evm_account_nonce_key};

    let chain_id = 7_u64;

    // Derive deterministic secp256k1 keypairs for the sender (tx signer) and the
    // authority (EOA delegating its code via EIP-7702).
    let secp = Secp256k1::new();
    let sender_secret_bytes = [0x11u8; 32];
    let sender_secret = SecretKey::from_slice(&sender_secret_bytes).expect("secret");
    let sender_pub = PublicKey::from_secret_key(&secp, &sender_secret);
    let sender_addr: RevmAddress = public_key_to_address(sender_pub);

    let authority_secret_bytes = [0x22u8; 32];
    let authority_secret =
        SecretKey::from_slice(&authority_secret_bytes).expect("authority secret");
    let authority_pub = PublicKey::from_secret_key(&secp, &authority_secret);
    let authority_addr: RevmAddress = public_key_to_address(authority_pub);

    // Trivial target runtime: PUSH1 0x00, PUSH1 0x00, RETURN — halts cleanly with
    // empty return data. The authority's delegation will be resolved to this
    // address, so the call must find *some* non-empty code on the target.
    let target_addr = RevmAddress::from([0x42u8; 20]);
    let target_code: Vec<u8> = vec![0x60, 0x00, 0x60, 0x00, 0xF3];

    // Seed state: sender must have enough balance to pay gas*maxFeePerGas and the
    // authorization base cost. Target must have non-empty code so the delegation
    // resolves to something the EVM can execute.
    let sender_balance = U256::from(10_u128.pow(18));
    let store = InMemoryStateStore::from_entries(vec![
        (
            evm_account_balance_key(sender_addr).into_bytes(),
            sender_balance.to_be_bytes::<32>().to_vec(),
        ),
        (
            evm_account_nonce_key(sender_addr).into_bytes(),
            0_u64.to_be_bytes().to_vec(),
        ),
        (
            evm_account_code_key(target_addr).into_bytes(),
            target_code.clone(),
        ),
    ]);

    let mut executor = BlockExecutor::new(chain_id, store);
    let parent_root = executor.committed_root();

    // Sign the EIP-7702 authorization. The authority delegates its code to
    // `target_addr` for `chain_id` at nonce 0 (authority account is currently
    // empty / nonce 0).
    let auth = Authorization {
        chain_id: U256::from(chain_id),
        address: target_addr,
        nonce: 0,
    };
    let auth_hash = auth.signature_hash();
    let auth_sig =
        sign_message(B256::from_slice(&authority_secret_bytes), auth_hash).expect("auth sig");
    let signed_auth = auth.into_signed(auth_sig);

    // Build the EIP-7702 transaction. The sender calls into `authority_addr`,
    // which — once the authorization is applied — will execute `target_code`.
    let tx = TxEip7702 {
        chain_id,
        nonce: 0,
        gas_limit: 200_000,
        max_fee_per_gas: 1,
        max_priority_fee_per_gas: 0,
        to: authority_addr,
        value: U256::ZERO,
        input: RevmBytes::new(),
        access_list: Default::default(),
        authorization_list: vec![signed_auth],
    };
    let tx_hash = tx.signature_hash();
    let tx_sig = sign_message(B256::from_slice(&sender_secret_bytes), tx_hash).expect("tx sig");
    let signed_tx = tx.into_signed(tx_sig);
    let envelope: TxEnvelope = signed_tx.into();
    let raw_tx_bytes = envelope.encoded_2718();

    // Build the parent block that execution will use as baseline: its state_root
    // must equal the currently committed root, and its height must be 0 (we are
    // executing height 1).
    let parent_body = BlockBody { items: vec![] };
    let parent = Block {
        header: BlockHeader {
            chain_id,
            height: 0,
            round: 0,
            proposer: validator(1),
            parent_hash: [0; 32],
            parent_state_root: [0; 32],
            tx_root: body_root(&parent_body),
            event_root: [0; 32],
            state_root: parent_root,
            qc: QuorumCertificate {
                view: 0,
                block_hash: [0; 32],
                signatures: vec![],
            },
            timestamp_ms: 0,
        },
        body: parent_body,
    };

    let body = BlockBody {
        items: vec![BlockItem::Transaction(SignedTransaction {
            signer: validator(9),
            bytes: raw_tx_bytes,
        })],
    };

    let qc = QuorumCertificate {
        view: 0,
        block_hash: [0; 32],
        signatures: vec![],
    };

    let prepared = executor
        .prepare_block(&parent, 1, validator(1), qc, body, 1_000)
        .expect("prepare should succeed for signed eip-7702 tx");

    let result = executor
        .execute_block(&prepared.block)
        .expect("commit should succeed for eip-7702 tx");

    assert_eq!(result.state_root, prepared.result.state_root);

    // After execution, the authority's committed EVM code must be the 23-byte
    // EIP-7702 delegation designator: `0xEF 0x01 0x00 || target_addr`.
    let delegation_code = executor
        .state_store()
        .committed_entries()
        .get(evm_account_code_key(authority_addr).as_bytes())
        .cloned()
        .expect("authority delegation code should be written");
    assert_eq!(delegation_code.len(), 23);
    assert_eq!(&delegation_code[0..3], &[0xef, 0x01, 0x00]);
    assert_eq!(&delegation_code[3..23], target_addr.as_slice());

    // Authority nonce should have been bumped to 1 by the authorization.
    let authority_nonce_bytes = executor
        .state_store()
        .committed_entries()
        .get(evm_account_nonce_key(authority_addr).as_bytes())
        .cloned()
        .expect("authority nonce should be written");
    assert_eq!(
        decode_u64_word(&authority_nonce_bytes).expect("nonce word"),
        1,
    );

    // Sender nonce should be bumped to 1 as well (its tx was just executed).
    let sender_nonce_bytes = executor
        .state_store()
        .committed_entries()
        .get(evm_account_nonce_key(sender_addr).as_bytes())
        .cloned()
        .expect("sender nonce should be written");
    assert_eq!(decode_u64_word(&sender_nonce_bytes).expect("nonce word"), 1);

    // Target's code must be unchanged — its address was only referenced as a
    // delegation target, never modified.
    let target_code_after = executor
        .state_store()
        .committed_entries()
        .get(evm_account_code_key(target_addr).as_bytes())
        .cloned()
        .expect("target code should still be present");
    assert_eq!(target_code_after, target_code);
}

#[test]
fn eip7702_envelope_decoding_preserves_authorization_list() {
    use alloy_consensus::{
        crypto::secp256k1::{public_key_to_address, sign_message},
        SignableTransaction, TxEip7702, TxEnvelope,
    };
    use alloy_eips::{eip7702::Authorization, Decodable2718, Encodable2718};
    use revm::primitives::{Address as RevmAddress, Bytes as RevmBytes, B256, U256};
    use secp256k1::{PublicKey, Secp256k1, SecretKey};

    let chain_id = 7_u64;

    let secp = Secp256k1::new();
    let sender_secret_bytes = [0xAAu8; 32];
    let sender_secret = SecretKey::from_slice(&sender_secret_bytes).expect("secret");
    let sender_pub = PublicKey::from_secret_key(&secp, &sender_secret);
    let sender_addr: RevmAddress = public_key_to_address(sender_pub);

    let authority_secret_bytes = [0xBBu8; 32];
    let authority_secret =
        SecretKey::from_slice(&authority_secret_bytes).expect("authority secret");
    let authority_pub = PublicKey::from_secret_key(&secp, &authority_secret);
    let authority_addr: RevmAddress = public_key_to_address(authority_pub);

    let target_addr = RevmAddress::from([0x77u8; 20]);

    let auth = Authorization {
        chain_id: U256::from(chain_id),
        address: target_addr,
        nonce: 5,
    };
    let auth_sig = sign_message(
        B256::from_slice(&authority_secret_bytes),
        auth.signature_hash(),
    )
    .expect("auth sig");
    let signed_auth = auth.into_signed(auth_sig);

    let tx = TxEip7702 {
        chain_id,
        nonce: 3,
        gas_limit: 150_000,
        max_fee_per_gas: 42,
        max_priority_fee_per_gas: 7,
        to: authority_addr,
        value: U256::from(11u64),
        input: RevmBytes::from_static(b"hello"),
        access_list: Default::default(),
        authorization_list: vec![signed_auth],
    };
    let tx_sig =
        sign_message(B256::from_slice(&sender_secret_bytes), tx.signature_hash()).expect("tx sig");
    let envelope: TxEnvelope = tx.into_signed(tx_sig).into();
    let raw = envelope.encoded_2718();

    let decoded = TxEnvelope::decode_2718(&mut raw.as_slice()).expect("decode envelope");

    let context = ExecutionContext {
        chain_id,
        height: 1,
        timestamp_ms: 0,
        parent_state_root: [0; 32],
    };
    let tx_env = super::tx_env_from_envelope(&decoded, validator(0), &context)
        .expect("eip7702 envelope should translate to TxEnv");

    // Recovery routes the caller back to the sender key.
    assert_eq!(tx_env.caller, sender_addr);
    assert_eq!(tx_env.chain_id, Some(chain_id));
    assert_eq!(tx_env.nonce, 3);
    assert_eq!(tx_env.authorization_list.len(), 1);
    assert_eq!(tx_env.tx_type, 4);

    // The authorization list must survive the translation unchanged: chain_id,
    // target address, nonce, and the recoverable authority must all match the
    // authorization we just signed.
    let entry = &tx_env.authorization_list[0];
    use revm::context_interface::{either::Either, transaction::AuthorizationTr};
    let left = match entry {
        Either::Left(signed) => signed,
        Either::Right(_) => panic!("expected a signed authorization, got a recovered one"),
    };
    assert_eq!(left.chain_id(), U256::from(chain_id));
    assert_eq!(left.address(), target_addr);
    assert_eq!(left.nonce(), 5);
    assert_eq!(left.authority(), Some(authority_addr));
}
