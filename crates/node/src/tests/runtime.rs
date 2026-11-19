use super::*;

#[test]
fn admits_intents_into_canonical_mempool_order() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                2,
                10_000,
                IntentKind::Transfer {
                    to: 33,
                    token: 1,
                    amount: 20,
                },
            ),
            1_100,
        )
        .expect("second nonce should admit");
    engine
        .submit_intent(
            signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 }),
            1_100,
        )
        .expect("first nonce should admit");

    let body = engine.intent_mempool().build_block_body(
        engine.policy_store(),
        &engine.order_books,
        &engine.frozen_accounts,
        &engine.markets,
        &engine.paused_markets,
        &engine.market_marks,
        &engine.market_index_prices,
        &engine.market_oracle_config,
        7,
        1,
        1_100,
        10,
    );

    assert_eq!(body.items.len(), 2);
    let first = match &body.items[0] {
        BlockItem::Intent(intent) => intent,
        BlockItem::Transaction(_)
        | BlockItem::OraclePrice(_)
        | BlockItem::Funding(_)
        | BlockItem::Faucet(_)
        | BlockItem::Liquidation(_)
        | BlockItem::Delegation(_)
        | BlockItem::MarketMakerProgram(_)
        | BlockItem::TeeGovernance(_)
        | BlockItem::RealCapitalGovernance(_) => {
            panic!("expected intent")
        }
    };
    let second = match &body.items[1] {
        BlockItem::Intent(intent) => intent,
        BlockItem::Transaction(_)
        | BlockItem::OraclePrice(_)
        | BlockItem::Funding(_)
        | BlockItem::Faucet(_)
        | BlockItem::Liquidation(_)
        | BlockItem::Delegation(_)
        | BlockItem::MarketMakerProgram(_)
        | BlockItem::TeeGovernance(_)
        | BlockItem::RealCapitalGovernance(_) => {
            panic!("expected intent")
        }
    };
    assert_eq!(first.intent.nonce, 1);
    assert_eq!(second.intent.nonce, 2);
}

#[test]
fn rejects_duplicate_intents_and_expired_submissions() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    let intent_id = engine
        .submit_intent(intent.clone(), 1_100)
        .expect("first submission should admit");

    let duplicate = engine
        .submit_intent(intent, 1_100)
        .expect_err("duplicate should fail");
    assert_eq!(duplicate, NodeError::DuplicateIntent(intent_id));

    let expired = engine
        .submit_intent(
            signed_intent(&signer, 2, 1_050, IntentKind::Cancel { order_id: 9 }),
            1_100,
        )
        .expect_err("expired intent should fail");
    assert_eq!(
        expired,
        NodeError::IntentExpired {
            intent_id: kombat_chain_consensus::intent_id(&signed_intent(
                &signer,
                2,
                1_050,
                IntentKind::Cancel { order_id: 9 },
            )),
            expires_at: 1_050,
            timestamp_ms: 1_100,
        }
    );

    let stale = engine
        .submit_intent(
            signed_intent(&signer, 0, 10_000, IntentKind::Cancel { order_id: 3 }),
            1_100,
        )
        .expect_err("stale nonce should fail");
    assert_eq!(
        stale,
        NodeError::StaleIntentNonce {
            policy_id: 42,
            expected: 1,
            actual: 0,
        }
    );
}

#[test]
fn rejects_intent_with_tampered_chain_id_after_signing() {
    let signer = signing_key(8);
    let mut config = single_node_config();
    config.chain_id = 8;
    let mut engine = SingleNodeEngine::new(config);
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let mut intent = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    intent.intent.chain_id = 8;

    let error = engine
        .submit_intent(intent, 1_100)
        .expect_err("tampered chain id should fail signature verification");
    assert_eq!(
        error,
        NodeError::Policy(PolicyError::InvalidSessionSignature)
    );
}

#[test]
fn verify_intent_signature_is_read_only_for_valid_session() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let intent = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    let expected_nonce = engine
        .policy_store()
        .get(42)
        .expect("policy should exist")
        .next_intent_nonce;

    verify_intent_signature(
        engine.policy_store(),
        &intent,
        engine.head().header.chain_id,
        engine.head().header.height.saturating_add(1),
    )
    .expect("valid signature should verify");

    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should still exist")
            .next_intent_nonce,
        expected_nonce
    );
    assert!(engine.intent_mempool().is_empty());
    assert_eq!(engine.conditional_intent_pool().len(), 0);
}

#[test]
fn verify_intent_signature_matches_admission_error_for_tampered_chain_id() {
    let signer = signing_key(8);
    let mut config = single_node_config();
    config.chain_id = 8;
    let mut engine = SingleNodeEngine::new(config);
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let mut intent = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    intent.intent.chain_id = 8;

    let verify_error = verify_intent_signature(
        engine.policy_store(),
        &intent,
        engine.head().header.chain_id,
        engine.head().header.height.saturating_add(1),
    )
    .expect_err("tampered chain id should fail signature verification");
    assert_eq!(
        verify_error,
        VerifyIntentError::Policy(PolicyError::InvalidSessionSignature)
    );

    let admission_error = engine
        .submit_intent(intent, 1_100)
        .expect_err("admission path should reject the same tampered intent");
    assert_eq!(
        admission_error,
        NodeError::Policy(PolicyError::InvalidSessionSignature)
    );
}

#[test]
fn queues_time_after_intents_until_not_before_boundary() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_time_after_intent(
                &signer,
                1,
                1_300,
                10_000,
                IntentKind::Transfer {
                    to: 33,
                    token: 1,
                    amount: 20,
                },
            ),
            1_100,
        )
        .expect("time-after intent should admit into conditional pool");

    assert!(engine.intent_mempool().is_empty());
    assert_eq!(engine.conditional_intent_pool().len(), 1);

    let early_block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("before not-before boundary the block should stay empty")
        .clone();
    assert!(early_block.body.items.is_empty());
    assert_eq!(engine.conditional_intent_pool().len(), 1);

    let promoted_block = engine
        .produce_block_from_mempool(1_300, 10)
        .expect("intent should promote once the not-before boundary is reached")
        .clone();
    assert_eq!(promoted_block.body.items.len(), 1);
    assert!(engine.conditional_intent_pool().is_empty());
    assert!(engine.intent_mempool().is_empty());
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        2
    );
}

#[test]
fn reports_intent_status_across_queue_promotion_and_execution() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    let queued_id = engine
        .submit_intent(queued, 1_100)
        .expect("conditional intent should admit");

    assert_eq!(
        engine.intent_status(queued_id),
        IntentStatus {
            intent_id: queued_id,
            stage: IntentStatusStage::AcceptedConditional,
            rejection_reason: None,
            order_rejection_reason: None,
            pending_reason: Some(crate::IntentPendingReason::TimeAfter),
            next_eligible_at_ms: Some(1_300),
            current_policy_nonce: Some(1),
            policy_id: Some(42),
            nonce: Some(1),
            currently_queued: true,
            latest_event_seq: Some(0),
            block_height: None,
            timestamp_ms: Some(1_100),
        }
    );

    engine
        .produce_block_from_mempool(1_300, 0)
        .expect("promotion-only block should succeed");
    let promoted = engine.intent_status(queued_id);
    assert_eq!(promoted.stage, IntentStatusStage::Promoted);
    assert!(promoted.currently_queued);
    assert_eq!(promoted.pending_reason, None);
    assert_eq!(promoted.latest_event_seq, Some(1));

    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("promoted intent should execute");
    let executed = engine.intent_status(queued_id);
    assert_eq!(executed.stage, IntentStatusStage::Executed);
    assert!(!executed.currently_queued);
    assert_eq!(executed.block_height, Some(2));
    assert_eq!(executed.latest_event_seq, Some(2));
}

#[test]
fn snapshot_restore_preserves_queued_intents_and_policy_counters() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    let queued_id = engine
        .submit_intent(queued, 1_100)
        .expect("conditional intent should admit");
    let before = engine.intent_status(queued_id);
    let snapshot = engine.snapshot();

    let mut restored = SingleNodeEngine::from_snapshot(snapshot);
    assert_eq!(restored.intent_status(queued_id), before);
    assert_eq!(restored.conditional_intent_pool().len(), 1);
    assert!(restored.intent_mempool().is_empty());
    assert_eq!(
        restored
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        1
    );

    restored
        .produce_block_from_mempool(1_300, 0)
        .expect("promotion-only block should succeed after restore");
    let promoted = restored.intent_status(queued_id);
    assert_eq!(promoted.stage, IntentStatusStage::Promoted);
    assert!(promoted.currently_queued);
    assert_eq!(promoted.latest_event_seq, Some(1));

    restored
        .produce_block_from_mempool(1_400, 10)
        .expect("queued intent should execute after restore");
    let executed = restored.intent_status(queued_id);
    assert_eq!(executed.stage, IntentStatusStage::Executed);
    assert_eq!(executed.block_height, Some(2));
    assert_eq!(
        restored
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        2
    );
}

#[test]
fn snapshot_restore_preserves_bad_debt_and_recovery_continuity() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_market_risk_parameters(
        7,
        MarketRiskParameters {
            maintenance_margin_bps: 500,
            liquidation_penalty_bps: 0,
            liquidator_fee_bps: 0,
        },
    );
    engine.set_isolated_collateral(7, 22, 10);
    engine.set_isolated_collateral(7, 33, 100);
    engine.set_isolated_collateral(7, 44, 100);
    engine.set_isolated_collateral(7, 55, 100);

    for (order, timestamp_ms) in [
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 33,
                client_order_id: 2,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_001,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 44,
                client_order_id: 3,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 3,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 90 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_002,
        ),
        (
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 55,
                client_order_id: 4,
                side: kombat_chain_matching::OrderSide::Ask,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 91 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_003,
        ),
    ] {
        engine
            .submit_order(order, timestamp_ms)
            .expect("order should succeed");
    }

    let outcome = engine
        .liquidate_position_with_size(7, 22, 99, 5, Some(2), 1_004)
        .expect("partial liquidation should succeed");
    assert_eq!(outcome.filled_lots, 2);
    assert!(engine.account_frozen(22));
    let bad_debt_before = engine.market_bad_debt_balance(7);
    assert!(bad_debt_before > 0);

    let snapshot = engine.snapshot();
    let mut restored = SingleNodeEngine::from_snapshot(snapshot);
    assert_eq!(restored.market_bad_debt_balance(7), bad_debt_before);
    assert!(restored.account_frozen(22));
    assert_eq!(
        restored.account_market_bad_debt.get(&(7, 22)),
        Some(&bad_debt_before)
    );

    restored.update_isolated_collateral(7, 22, 100, 1_005);

    assert_eq!(restored.market_bad_debt_balance(7), 0);
    assert!(!restored.account_frozen(22));
    assert!(!restored.margin_snapshot(7, 22).under_maintenance);
    assert!(restored.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::BadDebtRecovered {
            market_id,
            account_id,
            recovered,
            balance,
            source,
        } if *market_id == 7
            && *account_id == 22
            && *recovered == bad_debt_before
            && *balance == 0
            && *source == RiskTransitionSource::CollateralAdjustment
    )));
}

#[test]
fn snapshot_restore_preserves_committed_head_and_next_block_parentage() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    let first_id = engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    let first_block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce")
        .clone();
    let head_before = engine.head().clone();
    let head_hash_before = block_hash(&head_before);
    let committed_root_before = engine.committed_state_root();
    assert_eq!(
        engine.intent_status(first_id).stage,
        IntentStatusStage::Executed
    );
    assert_eq!(head_before.header.height, 1);

    let snapshot = engine.snapshot();
    let mut restored = SingleNodeEngine::from_snapshot(snapshot);

    assert_eq!(restored.head(), &head_before);
    assert_eq!(restored.committed_state_root(), committed_root_before);
    assert_eq!(restored.blocks().last(), Some(&first_block));
    assert_eq!(
        restored
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        2
    );

    let second = signed_intent(&signer, 2, 10_000, IntentKind::Cancel { order_id: 8 });
    let second_id = restored
        .submit_intent(second, 1_300)
        .expect("second intent should admit after restore");
    let second_block = restored
        .produce_block_from_mempool(1_400, 10)
        .expect("second block should produce after restore")
        .clone();

    assert_eq!(second_block.header.parent_hash, head_hash_before);
    assert_eq!(second_block.header.height, head_before.header.height + 1);
    assert_eq!(
        restored.intent_status(second_id).stage,
        IntentStatusStage::Executed
    );
    assert_eq!(
        restored
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        3
    );
}

#[test]
fn snapshot_file_roundtrip_preserves_restart_continuity() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-node-snapshot-{}-{}.borsh",
        std::process::id(),
        engine.head().header.height
    ));
    engine
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot file should write");

    let mut restored =
        SingleNodeEngine::load_snapshot_file(&snapshot_path).expect("snapshot should reload");
    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");

    assert_eq!(restored.head(), engine.head());
    assert_eq!(
        restored.committed_state_root(),
        engine.committed_state_root()
    );
    assert_eq!(
        restored
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        2
    );

    let second = signed_intent(&signer, 2, 10_000, IntentKind::Cancel { order_id: 8 });
    let second_id = restored
        .submit_intent(second, 1_300)
        .expect("second intent should admit after file reload");
    let next_block = restored
        .produce_block_from_mempool(1_400, 10)
        .expect("next block should produce after file reload");
    assert_eq!(next_block.header.height, 2);
    assert_eq!(
        restored.intent_status(second_id).stage,
        IntentStatusStage::Executed
    );
}

#[test]
fn logical_snapshot_file_load_preserves_startup_bootstrap_path() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-node-logical-startup-snapshot-{}-{}.k26s",
        std::process::id(),
        engine.head().header.height
    ));
    engine
        .save_logical_snapshot_file(&snapshot_path, 1_250)
        .expect("logical snapshot file should write");

    let mut restored = SingleNodeEngine::load_snapshot_file(&snapshot_path)
        .expect("startup snapshot loader should accept logical snapshots");
    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");

    assert_eq!(restored.head(), engine.head());
    assert_eq!(
        restored.committed_state_root(),
        engine.committed_state_root()
    );

    let second = signed_intent(&signer, 2, 10_000, IntentKind::Cancel { order_id: 8 });
    let second_id = restored
        .submit_intent(second, 1_300)
        .expect("second intent should admit after logical snapshot file reload");
    let next_block = restored
        .produce_block_from_mempool(1_400, 10)
        .expect("next block should produce after logical snapshot file reload");
    assert_eq!(next_block.header.height, 2);
    assert_eq!(
        restored.intent_status(second_id).stage,
        IntentStatusStage::Executed
    );
}

#[test]
fn snapshot_restore_after_empty_block_can_produce_mempool_block() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .produce_empty_block(1_100)
        .expect("empty bootstrap block should produce");

    let mut restored = SingleNodeEngine::from_snapshot(engine.snapshot());
    let next_block = restored
        .produce_block_from_mempool(1_200, 0)
        .expect("restored engine should execute empty mempool block");

    assert_eq!(next_block.header.height, 2);
    assert_eq!(next_block.header.timestamp_ms, 1_200);
}

#[test]
fn genesis_snapshot_restore_can_produce_first_block() {
    let mut restored = SingleNodeEngine::from_snapshot_bytes(
        &SingleNodeEngine::new(single_node_config()).snapshot_bytes(),
    )
    .expect("genesis snapshot should restore");
    let first_block = restored
        .produce_empty_block(1_100)
        .expect("restored genesis snapshot should produce first block");

    assert_eq!(first_block.header.height, 1);
    assert_eq!(first_block.header.timestamp_ms, 1_100);
}

#[test]
fn snapshot_file_save_creates_missing_parent_directories() {
    let engine = SingleNodeEngine::new(single_node_config());
    let snapshot_dir = std::env::temp_dir().join(format!(
        "kombat-node-snapshot-dir-{}-{}",
        std::process::id(),
        engine.head().header.height
    ));
    let snapshot_path = snapshot_dir.join("nested/engine.borsh");

    engine
        .save_snapshot_file(&snapshot_path)
        .expect("snapshot file should write through missing directories");

    let restored =
        SingleNodeEngine::load_snapshot_file(&snapshot_path).expect("snapshot should reload");
    assert_eq!(restored.head(), engine.head());

    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");
    std::fs::remove_dir_all(snapshot_dir).expect("snapshot directory should clean up");
}

#[test]
fn snapshot_file_load_rejects_missing_file() {
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-node-snapshot-missing-{}.borsh",
        std::process::id()
    ));
    let err = SingleNodeEngine::load_snapshot_file(&snapshot_path)
        .expect_err("missing snapshot file should not reload");

    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn snapshot_file_load_rejects_truncated_bytes() {
    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-node-snapshot-truncated-{}.borsh",
        std::process::id()
    ));
    std::fs::write(&snapshot_path, [1_u8, 2, 3, 4]).expect("truncated snapshot should write");

    let err = SingleNodeEngine::load_snapshot_file(&snapshot_path)
        .expect_err("truncated snapshot file should not reload");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    let _ = std::fs::remove_file(snapshot_path);
}

#[test]
fn snapshot_file_overwrite_reloads_latest_engine_state() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let snapshot_path = std::env::temp_dir().join(format!(
        "kombat-node-snapshot-overwrite-{}.borsh",
        std::process::id()
    ));

    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");
    engine
        .save_snapshot_file(&snapshot_path)
        .expect("first snapshot should write");

    let second = signed_intent(&signer, 2, 10_000, IntentKind::Cancel { order_id: 8 });
    engine
        .submit_intent(second, 1_300)
        .expect("second intent should admit");
    let second_block = engine
        .produce_block_from_mempool(1_400, 10)
        .expect("second block should produce")
        .clone();
    engine
        .save_snapshot_file(&snapshot_path)
        .expect("second snapshot should overwrite");

    let restored =
        SingleNodeEngine::load_snapshot_file(&snapshot_path).expect("snapshot should reload");
    std::fs::remove_file(&snapshot_path).expect("snapshot file should clean up");

    assert_eq!(restored.head(), &second_block);
    assert_eq!(restored.head().header.height, 2);
    assert_eq!(
        restored
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        3
    );
}

#[test]
fn snapshot_bytes_reject_empty_chain() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.chain.clear();

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject empty chain");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("requires a genesis block"));
}

#[test]
fn snapshot_bytes_reject_real_capital_bond_escrow_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    sample_open_delegation(&mut engine, 22, 1_000);
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.real_capital_bond_escrow_micro_usdc.clear();

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject missing bond escrow");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("real-capital bond escrow mismatch"));
}

#[test]
fn snapshot_bytes_reject_real_capital_compensation_escrow_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    engine
        .deposit_to_delegation(11, delegation.delegation_id, 150_000_000, 1_100)
        .expect("deposit should mint shares");
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::RiskLimitBreach,
            None,
            1_200,
        )
        .expect("termination should credit compensation escrow");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.real_capital_compensation_escrow_micro_usdc = 0;

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject compensation escrow mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("real-capital compensation escrow mismatch"));
}

#[test]
fn snapshot_bytes_reject_real_capital_insurance_wallet_above_gross_accumulator() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::WindingGraceFailure,
            None,
            1_200,
        )
        .expect("termination should credit insurance wallet");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.real_capital_insurance_wallet_micro_usdc =
        snapshot.real_capital_insurance_fund_micro_usdc + 1;
    let wallet_balance = snapshot.real_capital_insurance_wallet_micro_usdc;
    set_snapshot_policy_usdc_balance(
        &mut snapshot,
        crate::real_capital_insurance_wallet_address(),
        wallet_balance,
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject wallet above gross insurance");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("real-capital insurance wallet mismatch"));
}

#[test]
fn snapshot_bytes_allows_real_capital_insurance_wallet_below_gross_accumulator() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let delegation = sample_open_delegation(&mut engine, 22, 1_000);
    engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::WindingGraceFailure,
            None,
            1_200,
        )
        .expect("termination should credit insurance wallet");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.real_capital_insurance_wallet_micro_usdc =
        snapshot.real_capital_insurance_fund_micro_usdc - 1;
    let wallet_balance = snapshot.real_capital_insurance_wallet_micro_usdc;
    set_snapshot_policy_usdc_balance(
        &mut snapshot,
        crate::real_capital_insurance_wallet_address(),
        wallet_balance,
    );

    SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect("snapshot reload should allow wallet below gross insurance accumulator");
}

#[test]
fn snapshot_bytes_allows_missing_isolated_margin_market_custody_before_activation() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, 50);
    engine.fee_vaults.insert(7, 3);
    engine.insurance_funds.insert(7, 4);

    assert_eq!(
        engine
            .market_custody_required_balance(7)
            .expect("market custody requirement should compute"),
        57
    );
    assert_eq!(
        policy_usdc_balance_for_test(&engine, crate::isolated_margin_market_custody_address(7)),
        0
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot reload should allow missing custody before activation");
    assert_eq!(
        restored
            .market_custody_required_balance(7)
            .expect("restored custody requirement should compute"),
        57
    );
    assert_eq!(
        policy_usdc_balance_for_test(&restored, crate::isolated_margin_market_custody_address(7),),
        0
    );
}

#[test]
fn snapshot_bytes_allows_matching_isolated_margin_market_custody_entry() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, 50);
    engine.fee_vaults.insert(7, 3);
    engine.insurance_funds.insert(7, 4);
    let required_balance = engine
        .market_custody_required_balance(7)
        .expect("market custody requirement should compute");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    set_snapshot_policy_usdc_balance(
        &mut snapshot,
        crate::isolated_margin_market_custody_address(7),
        u128::try_from(required_balance).expect("required balance should be non-negative"),
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect("snapshot reload should accept matching market custody");

    assert_eq!(
        policy_usdc_balance_for_test(&restored, crate::isolated_margin_market_custody_address(7),),
        57
    );
}

#[test]
fn snapshot_bytes_reject_isolated_margin_market_custody_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, 50);
    engine.fee_vaults.insert(7, 3);
    engine.insurance_funds.insert(7, 4);
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    set_snapshot_policy_usdc_balance(
        &mut snapshot,
        crate::isolated_margin_market_custody_address(7),
        56,
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject mismatched market custody");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("market custody mismatch"));
}

#[test]
fn snapshot_bytes_reject_isolated_margin_market_custody_zero_entry() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, 50);
    engine.fee_vaults.insert(7, 3);
    engine.insurance_funds.insert(7, 4);
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.committed_entries.push((
        policy_usdc_balance_state_key(crate::isolated_margin_market_custody_address(7)),
        U256::ZERO.to_be_bytes::<32>().to_vec(),
    ));
    snapshot
        .committed_entries
        .sort_by(|left, right| left.0.cmp(&right.0));
    snapshot.committed_state_root =
        InMemoryStateStore::from_entries(snapshot.committed_entries.clone()).committed_root();

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject zero market custody entry");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("market custody malformed"));
}

#[test]
fn snapshot_bytes_reject_negative_isolated_margin_market_custody_requirement() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.set_isolated_collateral(7, 22, -100);
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    set_snapshot_policy_usdc_balance(
        &mut snapshot,
        crate::isolated_margin_market_custody_address(7),
        1,
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject negative market custody requirement");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("market custody mismatch"));
}

#[test]
fn snapshot_bytes_reject_tampered_block_chain_id() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.chain[1].header.chain_id = 999;

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject wrong chain id");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("chain id mismatch"));
}

#[test]
fn snapshot_bytes_reject_signing_key_material_mismatch() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let validator_id = *snapshot
        .validator_set
        .first()
        .expect("single-node snapshot should contain validator");
    snapshot
        .consensus_signing_keys
        .insert(validator_id, [55; 32]);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject mismatched signing key material");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("signing key material mismatch"));
}

#[test]
fn snapshot_bytes_reject_under_maintenance_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            22,
            signing_key(8).verifying_key().to_bytes(),
            [9; 32],
            1_000,
        )
        .expect("account should create");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.under_maintenance_accounts.insert((7, 22));

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject stale under-maintenance entry");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("under-maintenance mismatch"));
}

#[test]
fn snapshot_bytes_reject_frozen_account_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            22,
            signing_key(8).verifying_key().to_bytes(),
            [9; 32],
            1_000,
        )
        .expect("account should create");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.frozen_accounts.insert(22);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject stale frozen account");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("frozen account mismatch"));
}

#[test]
fn snapshot_bytes_reject_liquidation_escalation_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            22,
            signing_key(8).verifying_key().to_bytes(),
            [9; 32],
            1_000,
        )
        .expect("account should create");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.liquidation_escalation.insert(
        (7, 22),
        crate::LiquidationEscalationState {
            consecutive_non_improving_still_under: 1,
            last_margin_surplus: -1,
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject stale liquidation escalation");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("liquidation escalation mismatch"));
}

#[test]
fn snapshot_bytes_reject_market_bad_debt_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            22,
            signing_key(8).verifying_key().to_bytes(),
            [9; 32],
            1_000,
        )
        .expect("account should create");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.account_market_bad_debt.insert((7, 22), 5);
    snapshot.market_bad_debt_balances.insert(7, 4);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject bad-debt total mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("market bad debt mismatch"));
}

#[test]
fn snapshot_bytes_reject_order_book_market_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .order_books
        .insert(7, kombat_chain_matching::SingleMarketBook::new(7));
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let book = snapshot
        .order_books
        .remove(&7)
        .expect("snapshot should contain market book");
    snapshot.order_books.insert(8, book);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject order book market mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("order book market mismatch"));
}

#[test]
fn snapshot_bytes_reject_resting_order_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .order_books
        .entry(7)
        .or_insert_with(|| kombat_chain_matching::SingleMarketBook::new(7))
        .submit(
            kombat_chain_matching::OrderRequest {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                side: kombat_chain_matching::OrderSide::Bid,
                quantity_lots: 5,
                kind: kombat_chain_matching::OrderKind::Limit { price_ticks: 100 },
                time_in_force: kombat_chain_matching::TimeInForce::Gtc,
                post_only: false,
            },
            1_000,
        )
        .expect("resting order should exist");
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    tamper_snapshot_book(&mut snapshot, 7, |book| {
        book.bids[0].remaining_lots = 0;
    });

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject resting-order mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("resting order mismatch"));
}

#[test]
fn snapshot_bytes_reject_position_ledger_notional_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.positions.insert(
        (7, 22),
        PositionLedger {
            open_net_lots: 0,
            open_notional: 5,
            ..PositionLedger::default()
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect_err("snapshot reload should reject inconsistent flat position ledger");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("position ledger mismatch"));
}

#[test]
fn snapshot_bytes_reject_position_ledger_fill_metadata_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.positions.insert(
        (7, 22),
        PositionLedger {
            open_net_lots: 1,
            open_notional: 100,
            last_fill_seq: Some(1),
            last_fill_timestamp_ms: None,
            ..PositionLedger::default()
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect_err("snapshot reload should reject partial fill metadata");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("position ledger mismatch"));
}

#[test]
fn snapshot_bytes_reject_zero_market_id_state() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.paused_markets.insert(0);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject zero market id state");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("market state mismatch"));
}

#[test]
fn snapshot_bytes_reject_zero_oracle_price() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.market_marks.insert(
        7,
        crate::OraclePrice {
            price_ticks: 0,
            timestamp_ms: 1_000,
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject zero oracle price");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("oracle price mismatch"));
}

#[test]
fn snapshot_bytes_reject_invalid_oracle_submission() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.oracle_feed_submissions.insert(
        (7, 1_200, [1; 32]),
        crate::OracleFeedSubmission {
            market_id: 7,
            oracle_feed_id: 78,
            feeder_id: [1; 32],
            price_ticks: 105,
            sources_used: 2,
            confidence: 80,
            submitted_at_ms: 1_200,
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject mismatched oracle submission");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("oracle submission mismatch"));
}

#[test]
fn snapshot_bytes_reject_invalid_market_fee_parameters() {
    for invalid in [
        MarketFeeParameters {
            maker_fee_bps: 10_001,
            maker_rebate_bps: 0,
            taker_fee_bps: 0,
        },
        MarketFeeParameters {
            maker_fee_bps: 0,
            maker_rebate_bps: 10_001,
            taker_fee_bps: 0,
        },
        MarketFeeParameters {
            maker_fee_bps: 25,
            maker_rebate_bps: 76,
            taker_fee_bps: 50,
        },
    ] {
        let engine = SingleNodeEngine::new(single_node_config());
        let mut snapshot =
            crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
                .expect("snapshot should deserialize");
        snapshot.market_fees.insert(7, invalid);

        let err = SingleNodeEngine::from_snapshot_bytes(
            &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
        )
        .expect_err("snapshot reload should reject invalid fee bps");

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("market parameter mismatch"));
    }
}

#[test]
fn snapshot_bytes_repairs_synthetic_highest_qc_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.pacemaker.highest_qc.view = 0;

    let restored = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect("snapshot reload should repair synthetic highest qc mismatch");

    assert_eq!(
        restored.pacemaker.highest_qc.view,
        restored.head().header.round
    );
    assert_eq!(
        restored.pacemaker.highest_qc.block_hash,
        block_hash(restored.head())
    );
}

#[test]
fn snapshot_bytes_repairs_synthetic_round_regression() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.pacemaker.current_round = 1;

    let restored = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect("snapshot reload should repair synthetic round regression");

    assert_eq!(
        restored.pacemaker.current_round,
        restored.head().header.round.saturating_add(1)
    );
}

#[test]
fn snapshot_bytes_accept_historical_native_timestamp_regression() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .produce_empty_block(1_200)
        .expect("block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.chain[1].header.timestamp_ms = 900;

    let restored = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect("historical native timestamp regression should preserve committed state");

    assert_eq!(restored.head().header.height, 1);
    assert_eq!(restored.head().header.timestamp_ms, 900);
}

#[test]
fn snapshot_bytes_reject_committed_state_root_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.committed_entries.push((
        b"tampered-state-key".to_vec(),
        b"tampered-state-value".to_vec(),
    ));

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject committed state root mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("committed state root mismatch"));
}

#[test]
fn snapshot_bytes_reject_runtime_event_sequence_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.runtime_events[0].seq = 99;

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject runtime event sequence mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("runtime event sequence mismatch"));
}

#[test]
fn snapshot_bytes_reject_next_runtime_event_sequence_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.next_runtime_event_seq = 99;

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject next runtime event sequence mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("next runtime event sequence mismatch"));
}

#[test]
fn snapshot_bytes_reject_runtime_event_block_height_beyond_head() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.runtime_events[0].block_height = Some(99);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject runtime event block height beyond head");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("runtime event block height exceeds head"));
}

#[test]
fn snapshot_bytes_reject_quarantined_block_hash_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.quarantined_blocks.insert(
        [7; 32],
        crate::PersistedQuarantinedBlock {
            block_hash: [8; 32],
            height: 1,
            reason: crate::PersistedQuarantineReason::StateRootMismatch,
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject quarantined block hash mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("quarantined block hash mismatch"));
}

#[test]
fn snapshot_bytes_reject_quarantined_committed_block() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    let first_block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce")
        .clone();

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let committed_hash = block_hash(&first_block);
    snapshot.quarantined_blocks.insert(
        committed_hash,
        crate::PersistedQuarantinedBlock {
            block_hash: committed_hash,
            height: first_block.header.height,
            reason: crate::PersistedQuarantineReason::StateRootMismatch,
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject committed block quarantine");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("quarantines committed block"));
}

#[test]
fn snapshot_bytes_reject_direct_known_intent_set_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.intent_mempool.known_intents.clear();

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject direct known intent mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("direct known intent set mismatch"));
}

#[test]
fn snapshot_bytes_reject_conditional_activation_mismatch() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    engine
        .submit_intent(queued, 1_100)
        .expect("conditional intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let (activation_key, signed_intent) = snapshot
        .conditional_pool
        .time_after_by_order
        .iter()
        .next()
        .map(|(key, intent)| (*key, intent.clone()))
        .expect("conditional pool should contain queued intent");
    snapshot
        .conditional_pool
        .time_after_by_order
        .remove(&activation_key);
    let mut tampered_key = activation_key;
    tampered_key.not_before_ms = activation_key.not_before_ms.saturating_add(1);
    snapshot
        .conditional_pool
        .time_after_by_order
        .insert(tampered_key, signed_intent);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject conditional activation mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("conditional activation mismatch"));
}

#[test]
fn snapshot_bytes_reject_queued_policy_nonce_duplicate() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let first = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let duplicate = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    snapshot
        .conditional_pool
        .submit(duplicate.clone(), intent_id(&duplicate), 1_300)
        .expect("conditional duplicate should insert into tampered snapshot");

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject queued policy nonce duplicate");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("queued policy nonce duplicate"));
}

#[test]
fn snapshot_bytes_reject_direct_pool_time_after_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    engine
        .submit_intent(queued, 1_100)
        .expect("conditional intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let (activation_key, signed_intent) = snapshot
        .conditional_pool
        .time_after_by_order
        .iter()
        .next()
        .map(|(key, intent)| (*key, intent.clone()))
        .expect("conditional pool should contain queued intent");
    snapshot
        .conditional_pool
        .time_after_by_order
        .remove(&activation_key);
    snapshot
        .conditional_pool
        .known_intents
        .remove(&intent_id(&signed_intent));
    snapshot
        .intent_mempool
        .submit(signed_intent.clone(), intent_id(&signed_intent))
        .expect("tampered direct insert should succeed");

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject direct pool time-after intent");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("direct pool contains time-after intent"));
}

#[test]
fn snapshot_bytes_reject_conditional_intent_policy_missing() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    engine
        .submit_intent(queued, 1_100)
        .expect("conditional intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    let (activation_key, mut signed_intent) = snapshot
        .conditional_pool
        .time_after_by_order
        .iter()
        .next()
        .map(|(key, intent)| (*key, intent.clone()))
        .expect("conditional pool should contain queued intent");
    snapshot
        .conditional_pool
        .time_after_by_order
        .remove(&activation_key);
    snapshot
        .conditional_pool
        .known_intents
        .remove(&intent_id(&signed_intent));
    signed_intent.intent.policy_id = 999;
    snapshot
        .conditional_pool
        .submit(
            signed_intent.clone(),
            intent_id(&signed_intent),
            activation_key.not_before_ms,
        )
        .expect("tampered conditional insert should succeed");

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject conditional intent missing policy");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("conditional intent policy missing"));
}

#[test]
fn snapshot_bytes_reject_conditional_intent_nonce_regression() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let direct = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    engine
        .submit_intent(direct, 1_100)
        .expect("direct intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block should build and commit");

    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    let queued_id = intent_id(&queued);

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot
        .conditional_pool
        .submit(queued, queued_id, 1_300)
        .expect("tampered conditional insert should succeed");

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject conditional intent nonce regression");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("conditional intent nonce regression"));
}

#[test]
fn snapshot_bytes_reject_direct_queued_intent_terminal_event() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let queued = signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 });
    let queued_id = engine
        .submit_intent(queued.clone(), 1_100)
        .expect("direct intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.runtime_events.push(crate::RuntimeEventRecord {
        seq: snapshot.next_runtime_event_seq,
        block_height: None,
        timestamp_ms: 1_200,
        kind: RuntimeEventKind::IntentRejected {
            intent_id: queued_id,
            policy_id: queued.intent.policy_id,
            nonce: queued.intent.nonce,
            reason: IntentRejectionReason::InternalError,
            order_reason: None,
        },
    });
    snapshot.next_runtime_event_seq = snapshot.next_runtime_event_seq.saturating_add(1);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject direct queued intent terminal event");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("direct queued intent event mismatch"));
}

#[test]
fn snapshot_bytes_reject_conditional_queued_intent_promoted_event() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let queued = signed_time_after_intent(
        &signer,
        1,
        1_300,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    let queued_id = engine
        .submit_intent(queued.clone(), 1_100)
        .expect("conditional intent should admit");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.runtime_events.push(crate::RuntimeEventRecord {
        seq: snapshot.next_runtime_event_seq,
        block_height: None,
        timestamp_ms: 1_200,
        kind: RuntimeEventKind::IntentPromoted {
            intent_id: queued_id,
            policy_id: queued.intent.policy_id,
            nonce: queued.intent.nonce,
        },
    });
    snapshot.next_runtime_event_seq = snapshot.next_runtime_event_seq.saturating_add(1);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject conditional queued intent promoted event");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("conditional queued intent event mismatch"));
}

#[test]
fn snapshot_bytes_reject_telemetry_timeout_count_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.record_timeout(1_010);
    engine
        .produce_empty_block(1_100)
        .expect("block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.telemetry.metrics_mut().timeouts_total = snapshot
        .telemetry
        .metrics()
        .timeouts_total
        .saturating_add(1);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject telemetry timeout mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("timeout count mismatch"));
}

#[test]
fn snapshot_bytes_reject_telemetry_validator_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .produce_empty_block(1_100)
        .expect("block should produce");

    let mut snapshot =
        crate::PersistedSingleNodeEngineSnapshot::try_from_slice(&engine.snapshot_bytes())
            .expect("snapshot should deserialize");
    snapshot.telemetry.logs_mut()[1].validator_id = Some(validator(1));

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot reload should reject telemetry validator mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("log validator mismatch"));
}

#[test]
fn local_cluster_node_snapshot_reload_rejects_telemetry_peer_count_mismatch() {
    let cluster = LocalCluster::new(local_cluster_config()).expect("cluster should start");
    let mut snapshot = cluster
        .nodes()
        .last()
        .expect("cluster should have a node")
        .snapshot();
    snapshot.telemetry.metrics_mut().peer_count = 0;
    snapshot.telemetry.logs_mut()[0].peer_count = 0;

    let err = crate::LocalClusterNode::try_from_snapshot(snapshot)
        .expect_err("node snapshot reload should reject telemetry peer count mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("peer count mismatch"));
}

#[test]
fn time_before_uses_effective_deadline_at_admission() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let err = engine
        .submit_intent(
            signed_time_before_intent(
                &signer,
                1,
                1_090,
                10_000,
                IntentKind::Cancel { order_id: 9 },
            ),
            1_100,
        )
        .expect_err("time-before deadline should cap effective expiry");

    assert_eq!(
        err,
        NodeError::IntentExpired {
            intent_id: kombat_chain_consensus::intent_id(&signed_time_before_intent(
                &signer,
                1,
                1_090,
                10_000,
                IntentKind::Cancel { order_id: 9 },
            )),
            expires_at: 1_090,
            timestamp_ms: 1_100,
        }
    );
}

#[test]
fn produces_block_from_mempool_and_commits_policy_counters() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::Transfer {
                    to: 33,
                    token: 1,
                    amount: 20,
                },
            ),
            1_100,
        )
        .expect("intent should admit");

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block should build from mempool")
        .clone();

    assert_eq!(block.body.items.len(), 1);
    assert_eq!(engine.intent_mempool().len(), 0);
    let policy = engine.policy_store().get(42).expect("policy should exist");
    assert_eq!(policy.counters.intents_today, 1);
    assert_eq!(policy.counters.volume_today, 20);
    assert_eq!(policy.counters.gas_used_today, 10);
    assert_eq!(policy.next_intent_nonce, 2);
    assert_ne!(block.header.state_root, block.header.parent_state_root);
    assert_ne!(block.header.event_root, [0; 32]);
}

#[test]
fn create_sub_policy_intent_commits_child_policy() {
    let signer = signing_key(8);
    let child_signer = signing_key(12);
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mut policy = sample_policy();
    policy.admission_rules.allowed_markets = vec![7];
    engine
        .policy_store_mut()
        .insert(policy)
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::CreateSubPolicy {
                    child_policy_id: 43,
                    child_grantee: child_signer.verifying_key().to_bytes(),
                    child_expires_at: 50_000,
                    child_terms: PolicyTermsWire {
                        gas_budget_per_day: 40,
                        gas_refill_per_day: 40,
                        max_intents_per_second: 1,
                        max_intents_per_day: 3,
                        max_volume_per_day: 120,
                        max_drawdown_pct: 20,
                    },
                },
            ),
            1_100,
        )
        .expect("create-sub-policy intent should admit");

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("create-sub-policy block should build")
        .clone();

    assert_eq!(block.body.items.len(), 1);
    let parent = engine
        .policy_store()
        .get(42)
        .expect("parent policy should exist");
    let child = engine
        .policy_store()
        .get(43)
        .expect("child policy should exist");

    assert_eq!(parent.next_intent_nonce, 2);
    assert_eq!(parent.reserved.gas_budget_per_day, 40);
    assert_eq!(parent.reserved.volume_per_day, 120);
    assert_eq!(child.parent_policy_id, Some(42));
    assert_eq!(child.authority_account, 11);
    assert_eq!(child.execution_scope, parent.execution_scope);
    assert_eq!(child.admission_rules.allowed_markets, vec![7]);
    assert_eq!(
        child.key_state.grantee,
        child_signer.verifying_key().to_bytes()
    );
}

#[test]
fn revoke_policy_intent_cascades_and_releases_reservations() {
    let signer = signing_key(8);
    let child_signer = signing_key(12);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::CreateSubPolicy {
                    child_policy_id: 43,
                    child_grantee: child_signer.verifying_key().to_bytes(),
                    child_expires_at: 50_000,
                    child_terms: PolicyTermsWire {
                        gas_budget_per_day: 40,
                        gas_refill_per_day: 40,
                        max_intents_per_second: 1,
                        max_intents_per_day: 3,
                        max_volume_per_day: 120,
                        max_drawdown_pct: 20,
                    },
                },
            ),
            1_100,
        )
        .expect("create-sub-policy intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("create-sub-policy block should build");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                2,
                10_000,
                IntentKind::RevokePolicy {
                    target_policy_id: 43,
                },
            ),
            1_300,
        )
        .expect("revoke intent should admit");

    let block = engine
        .produce_block_from_mempool(1_400, 10)
        .expect("revoke block should build")
        .clone();

    assert_eq!(block.body.items.len(), 1);
    let parent = engine
        .policy_store()
        .get(42)
        .expect("parent policy should exist");
    let child = engine
        .policy_store()
        .get(43)
        .expect("child policy should exist");

    assert_eq!(parent.next_intent_nonce, 3);
    assert_eq!(parent.reserved.gas_budget_per_day, 0);
    assert_eq!(parent.reserved.volume_per_day, 0);
    assert_eq!(child.state, PolicyState::Revoked);
    assert_eq!(child.revoked_at, Some(1_400));
}

#[test]
fn block_production_purges_expired_queued_intents() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let queued = signed_intent(
        &signer,
        1,
        1_150,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    engine
        .submit_intent(queued.clone(), 1_100)
        .expect("intent should queue before expiry");
    let queued_id = intent_id(&queued);
    assert_eq!(engine.intent_mempool().len(), 1);

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("expired intent should be purged during block production")
        .clone();

    assert!(block.body.items.is_empty());
    assert!(engine.intent_mempool().is_empty());
    assert_eq!(
        engine.intent_status(queued_id).stage,
        IntentStatusStage::Expired
    );
    assert_eq!(
        engine
            .runtime_events()
            .last()
            .expect("expired intent should record runtime event")
            .kind,
        RuntimeEventKind::IntentExpired {
            intent_id: queued_id,
            policy_id: 42,
            nonce: 1,
        }
    );

    let err = engine
        .submit_intent(queued, 1_200)
        .expect_err("re-submitting the same expired intent should not be treated as duplicate");
    assert!(matches!(err, NodeError::IntentExpired { .. }));
}

#[test]
fn block_builder_skips_nonce_gaps_until_frontier_is_contiguous() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                2,
                10_000,
                IntentKind::Transfer {
                    to: 33,
                    token: 1,
                    amount: 20,
                },
            ),
            1_100,
        )
        .expect("future nonce can queue");

    let before_frontier = engine.intent_mempool().build_block_body(
        engine.policy_store(),
        &engine.order_books,
        &engine.frozen_accounts,
        &engine.markets,
        &engine.paused_markets,
        &engine.market_marks,
        &engine.market_index_prices,
        &engine.market_oracle_config,
        7,
        1,
        1_100,
        10,
    );
    assert!(before_frontier.items.is_empty());

    engine
        .submit_intent(
            signed_intent(&signer, 1, 10_000, IntentKind::Cancel { order_id: 7 }),
            1_100,
        )
        .expect("frontier nonce should admit");

    let after_frontier = engine.intent_mempool().build_block_body(
        engine.policy_store(),
        &engine.order_books,
        &engine.frozen_accounts,
        &engine.markets,
        &engine.paused_markets,
        &engine.market_marks,
        &engine.market_index_prices,
        &engine.market_oracle_config,
        7,
        1,
        1_100,
        10,
    );
    assert_eq!(after_frontier.items.len(), 2);
}

#[test]
fn mahi_block_builder_shards_policy_frontiers_by_validator_index() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    for policy_id in 40..44_u64 {
        engine
            .policy_store_mut()
            .insert(PolicyRecord::new(
                policy_id,
                policy_id + 1_000,
                signer.verifying_key().to_bytes(),
                1_000,
                172_800_000,
                PolicyState::Active,
                PolicyTerms {
                    gas_budget_per_day: 100,
                    gas_refill_per_day: 100,
                    max_intents_per_second: 10,
                    max_intents_per_day: 10,
                    max_volume_per_day: 1_000,
                    max_drawdown_pct: 25,
                    ..PolicyTerms::default()
                },
            ))
            .expect("policy should insert");
        engine
            .submit_intent(
                signed_intent_for_policy(
                    &signer,
                    policy_id,
                    policy_id + 1_000,
                    1,
                    10_000,
                    IntentKind::Transfer {
                        to: 33,
                        token: 1,
                        amount: 20,
                    },
                ),
                1_100,
            )
            .expect("intent should admit");
    }

    let body = engine
        .intent_mempool()
        .select_mahi_block_body_from_frontier(
            engine.policy_store(),
            1_100,
            10,
            &engine.proposer_frontier,
            2,
            4,
            &BTreeMap::new(),
        );
    let policy_ids = body
        .items
        .iter()
        .filter_map(|item| match item {
            BlockItem::Intent(signed_intent) => Some(signed_intent.intent.policy_id),
            BlockItem::Transaction(_)
            | BlockItem::OraclePrice(_)
            | BlockItem::Funding(_)
            | BlockItem::Faucet(_)
            | BlockItem::Liquidation(_)
            | BlockItem::Delegation(_)
            | BlockItem::MarketMakerProgram(_)
            | BlockItem::TeeGovernance(_)
            | BlockItem::RealCapitalGovernance(_) => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(policy_ids, vec![42]);
}

#[test]
fn mahi_block_builder_advances_past_local_inflight_nonces() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    for nonce in 1..=3_u64 {
        engine
            .submit_intent(
                signed_intent(
                    &signer,
                    nonce,
                    10_000,
                    IntentKind::Transfer {
                        to: 33,
                        token: 1,
                        amount: 20,
                    },
                ),
                1_100,
            )
            .expect("intent should admit");
    }

    let body = engine
        .intent_mempool()
        .select_mahi_block_body_from_frontier(
            engine.policy_store(),
            1_100,
            10,
            &engine.proposer_frontier,
            0,
            1,
            &BTreeMap::from([(42, BTreeSet::from([1, 2]))]),
        );
    let nonces = body
        .items
        .iter()
        .filter_map(|item| match item {
            BlockItem::Intent(signed_intent) => Some(signed_intent.intent.nonce),
            BlockItem::Transaction(_)
            | BlockItem::OraclePrice(_)
            | BlockItem::Funding(_)
            | BlockItem::Faucet(_)
            | BlockItem::Liquidation(_)
            | BlockItem::Delegation(_)
            | BlockItem::MarketMakerProgram(_)
            | BlockItem::TeeGovernance(_)
            | BlockItem::RealCapitalGovernance(_) => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(nonces, vec![3]);
}

#[test]
fn mahi_snapshot_shards_against_configured_validator_set() {
    let signer = signing_key(8);
    let config = SingleNodeConfig {
        chain_id: 7,
        validator_id: validator(2),
        consensus_signing_key: [2; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [3; 32],
        pacemaker: pacemaker(),
    };
    let mut engine =
        SingleNodeEngine::new_with_validator_set(config, Some(&four_node_validator_set()));
    for policy_id in 40..44_u64 {
        engine
            .policy_store_mut()
            .insert(PolicyRecord::new(
                policy_id,
                policy_id + 1_000,
                signer.verifying_key().to_bytes(),
                1_000,
                172_800_000,
                PolicyState::Active,
                PolicyTerms {
                    gas_budget_per_day: 100,
                    gas_refill_per_day: 100,
                    max_intents_per_second: 10,
                    max_intents_per_day: 10,
                    max_volume_per_day: 1_000,
                    max_drawdown_pct: 25,
                    ..PolicyTerms::default()
                },
            ))
            .expect("policy should insert");
        engine
            .submit_intent(
                signed_intent_for_policy(
                    &signer,
                    policy_id,
                    policy_id + 1_000,
                    1,
                    10_000,
                    IntentKind::Transfer {
                        to: 33,
                        token: 1,
                        amount: 20,
                    },
                ),
                1_100,
            )
            .expect("intent should admit");
    }

    let snapshot = EngineHandle::new(engine)
        .mahi_snapshot(1_100, 10, &BTreeMap::new())
        .expect("snapshot lock should succeed")
        .expect("snapshot should exist");
    let policy_ids = snapshot
        .intents
        .iter()
        .map(|intent| intent.intent.policy_id)
        .collect::<Vec<_>>();

    assert_eq!(policy_ids, vec![41]);
}

#[test]
fn rejects_frontier_intent_that_fails_policy_simulation() {
    let signer = signing_key(8);
    let mut policy = sample_policy();
    policy.terms.gas_budget_per_day = 10;
    policy.terms.gas_refill_per_day = 10;

    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(policy)
        .expect("policy should insert");

    let first = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::Transfer {
            to: 33,
            token: 1,
            amount: 20,
        },
    );
    let second = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::Transfer {
            to: 34,
            token: 1,
            amount: 30,
        },
    );
    let second_id = intent_id(&second);

    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .submit_intent(second, 1_100)
        .expect("second intent should queue");

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block production should succeed");

    assert_eq!(block.body.items.len(), 1);
    assert!(!engine.intent_mempool().contains_intent(second_id));
    let rejected = engine.intent_status(second_id);
    assert_eq!(rejected.stage, IntentStatusStage::Rejected);
    assert_eq!(
        rejected.rejection_reason,
        Some(IntentRejectionReason::GasBudgetExceeded)
    );
    assert!(engine.runtime_events().iter().any(|event| {
        event.kind
            == RuntimeEventKind::IntentRejected {
                intent_id: second_id,
                policy_id: 42,
                nonce: 2,
                reason: IntentRejectionReason::GasBudgetExceeded,
                order_reason: None,
            }
    }));
}

#[test]
fn proposer_frontier_refreshes_when_missing_nonce_arrives() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                2,
                10_000,
                IntentKind::Transfer {
                    to: 33,
                    token: 1,
                    amount: 20,
                },
            ),
            1_100,
        )
        .expect("future nonce should queue");

    engine
        .submit_intent(
            signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::Transfer {
                    to: 34,
                    token: 1,
                    amount: 30,
                },
            ),
            1_101,
        )
        .expect("frontier nonce should admit");

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("frontier should refresh into a two-intent block");
    assert_eq!(block.body.items.len(), 2);
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should exist")
            .next_intent_nonce,
        3
    );
    assert!(engine.intent_mempool().is_empty());
}

#[test]
fn proposer_frontier_stats_report_stale_ready_items() {
    let signer_a = signing_key(8);
    let signer_b = signing_key(9);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy a should insert");
    engine
        .policy_store_mut()
        .insert(PolicyRecord::new(
            43,
            12,
            signer_b.verifying_key().to_bytes(),
            1_000,
            172_800_000,
            PolicyState::Active,
            PolicyTerms {
                gas_budget_per_day: 100,
                gas_refill_per_day: 100,
                max_intents_per_second: 2,
                max_intents_per_day: 10,
                max_volume_per_day: 1_000,
                max_drawdown_pct: 25,
                ..PolicyTerms::default()
            },
        ))
        .expect("policy b should insert");

    engine
        .submit_intent(
            signed_intent(&signer_a, 1, 10_000, IntentKind::Cancel { order_id: 7 }),
            1_100,
        )
        .expect("a1 should admit");
    engine
        .submit_intent(
            signed_intent_for_policy(
                &signer_b,
                43,
                12,
                1,
                10_000,
                IntentKind::Cancel { order_id: 8 },
            ),
            1_101,
        )
        .expect("b1 should admit");

    engine.proposer_frontier.contiguous_by_policy.remove(&43);

    let stats = engine.proposer_frontier_stats();
    assert_eq!(stats.direct_mempool_items, 2);
    assert_eq!(stats.tracked_policy_count, 1);
    assert_eq!(stats.tracked_ready_items, 1);
    assert_eq!(stats.stale_policy_count, 1);
    assert_eq!(stats.stale_ready_items, 1);
}

#[test]
fn reports_nonce_frontier_pending_reason_for_future_direct_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let future = signed_intent(&signer, 2, 10_000, IntentKind::Cancel { order_id: 7 });
    let future_id = engine
        .submit_intent(future, 1_100)
        .expect("future nonce intent should queue");

    let status = engine.intent_status(future_id);
    assert_eq!(status.stage, IntentStatusStage::AcceptedDirect);
    assert_eq!(
        status.pending_reason,
        Some(crate::IntentPendingReason::NonceFrontier)
    );
    assert_eq!(status.current_policy_nonce, Some(1));
    assert_eq!(status.next_eligible_at_ms, None);
}

#[test]
fn reports_market_trading_paused_pending_reason_for_queued_order_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let queued_intent_id = intent_id(&intent);
    engine
        .submit_intent(intent, 1_100)
        .expect("intent should admit while market is active");
    engine.set_market_trading_paused(7, true, 1_101);

    let status = engine.intent_status(queued_intent_id);
    assert_eq!(
        status.pending_reason,
        Some(crate::IntentPendingReason::MarketTradingPaused)
    );
}

#[test]
fn place_order_intent_admits_while_market_trading_is_paused() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    engine.set_market_trading_paused(7, true, 1_000);

    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let queued_intent_id = intent_id(&intent);

    let admitted_id = engine
        .submit_intent(intent, 1_001)
        .expect("transient market pause should defer, not reject");
    assert_eq!(admitted_id, queued_intent_id);
    assert!(engine.intent_mempool().contains_intent(queued_intent_id));
    assert_eq!(
        engine.intent_status(queued_intent_id).pending_reason,
        Some(crate::IntentPendingReason::MarketTradingPaused)
    );
}

#[test]
fn reports_stale_market_data_pending_reason_for_queued_order_intent() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    engine.update_market_mark_price(7, 103, 1_000);
    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let queued_intent_id = intent_id(&intent);
    engine
        .submit_intent(intent, 1_001)
        .expect("intent should admit while market data is fresh");
    engine.update_market_oracle_config(
        7,
        kombat_chain_oracle::OracleConfig {
            max_staleness_ms: 100,
        },
        1_200,
    );

    let status = engine.intent_status(queued_intent_id);
    assert_eq!(
        status.pending_reason,
        Some(crate::IntentPendingReason::StaleMarketData)
    );
}

#[test]
fn paused_order_intents_stay_queued_during_block_planning() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let queued_intent_id = intent_id(&intent);
    engine
        .submit_intent(intent, 1_100)
        .expect("intent should admit while market is active");
    engine.set_market_trading_paused(7, true, 1_150);

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("empty block should still produce");
    assert!(block.body.items.is_empty());
    assert!(engine.intent_mempool().contains_intent(queued_intent_id));
    assert_eq!(
        engine.intent_status(queued_intent_id).pending_reason,
        Some(crate::IntentPendingReason::MarketTradingPaused)
    );
}

#[test]
fn owner_pause_call_emits_account_updated_event() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(11, signer.verifying_key().to_bytes(), [9; 32], 1_000)
        .expect("account should create");

    engine
        .submit_owner_call(signed_owner_call(&signer, 11, 0, OwnerAction::Pause), 1_100)
        .expect("pause owner call should succeed");

    assert!(engine.runtime_events().iter().any(|event| {
        event.kind
            == RuntimeEventKind::AccountUpdated {
                account_id: 11,
                action: AccountUpdateAction::Pause,
                reason: None,
                owner: signer.verifying_key().to_bytes(),
                pending_owner: None,
                paused: true,
                nonce: 1,
                code_hash: [9; 32],
                controllers: vec![],
            }
    }));
}

#[test]
fn place_order_intent_commits_into_matching_book() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );

    engine
        .submit_intent(intent, 1_100)
        .expect("place order intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block production should succeed");

    assert_eq!(
        engine
            .resting_order(7, 11, 99, 1_200)
            .expect("resting order should exist")
            .price_ticks,
        103
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::OrderAccepted {
            market_id,
            account_id,
            client_order_id,
            ..
        } if *market_id == 7 && *account_id == 11 && *client_order_id == 99
    )));
}

#[test]
fn preverified_resting_place_orders_commit_as_batch() {
    let signer = signing_key(8);
    let first = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 100,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let second = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 101,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let items = vec![BlockItem::Intent(first), BlockItem::Intent(second)];
    let mut policy_store = PolicyStore::default();
    policy_store
        .insert(sample_policy())
        .expect("policy should insert");
    let mut order_books = BTreeMap::<u64, kombat_chain_matching::SingleMarketBook>::new();
    let mut resting_order_mandates = BTreeMap::new();
    let mut stats = crate::RestingBatchCommitStats::default();

    let (consumed, events) = crate::try_commit_resting_place_order_batch(
        &mut policy_store,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut order_books,
        &mut resting_order_mandates,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        7,
        &items,
        0,
        1,
        1_200,
        crate::PolicyAdmissionCommitMode::PreverifiedSession,
        &mut stats,
    )
    .expect("batch commit should not error")
    .expect("resting place-order run should batch");

    assert_eq!(consumed, 2);
    assert_eq!(events.len(), 4);
    assert_eq!(
        policy_store
            .get(42)
            .expect("policy should remain present")
            .next_intent_nonce,
        3
    );
    let book = order_books.get(&7).expect("book should be created");
    assert_eq!(book.bids().len(), 2);
    assert_eq!(book.bids()[0].client_order_id, 100);
    assert_eq!(book.bids()[1].client_order_id, 101);
    assert_eq!(
        stats,
        crate::RestingBatchCommitStats {
            attempts: 1,
            accepted: 1,
            items: 2,
            unsupported: 0,
            rejected: 0,
            gather_ns: stats.gather_ns,
            precheck_ns: stats.precheck_ns,
            admission_ns: stats.admission_ns,
            policy_ns: stats.policy_ns,
            matching_ns: stats.matching_ns,
            event_ns: stats.event_ns,
        }
    );
    assert!(stats.gather_ns > 0);
    assert!(stats.precheck_ns > 0);
    assert!(stats.admission_ns > 0);
    assert!(stats.policy_ns > 0);
    assert!(stats.matching_ns > 0);
    assert!(stats.event_ns > 0);
}

#[test]
fn mahi_inplace_resting_commit_advances_state_without_overlay_clone() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let body = BlockBody {
        items: vec![
            BlockItem::Intent(signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::PlaceOrder {
                    market_id: 7,
                    client_order_id: 100,
                    side: IntentOrderSide::Bid,
                    quantity_lots: 1,
                    kind: IntentOrderKind::Limit { price_ticks: 100 },
                    time_in_force: IntentTimeInForce::Gtc,
                    post_only: false,
                },
            )),
            BlockItem::Intent(signed_intent(
                &signer,
                2,
                10_000,
                IntentKind::PlaceOrder {
                    market_id: 7,
                    client_order_id: 101,
                    side: IntentOrderSide::Bid,
                    quantity_lots: 1,
                    kind: IntentOrderKind::Limit { price_ticks: 100 },
                    time_in_force: IntentTimeInForce::Gtc,
                    post_only: false,
                },
            )),
        ],
    };
    let parent = engine.head().clone();
    let positions = engine.positions.clone();
    let market_marks = engine.market_marks.clone();
    let market_index_prices = engine.market_index_prices.clone();

    let applied = engine
        .try_apply_mahi_resting_body_in_place(
            &parent,
            1,
            1_200,
            validator(9),
            body,
            &market_marks,
            &market_index_prices,
            &positions,
            std::time::Duration::ZERO,
            std::time::Instant::now(),
            false,
        )
        .expect("in-place resting commit should not error");

    assert!(applied);
    assert_eq!(engine.head().header.height, parent.header.height + 1);
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should remain present")
            .next_intent_nonce,
        3
    );
    assert!(engine.resting_order(7, 11, 100, 1_200).is_some());
    assert!(engine.resting_order(7, 11, 101, 1_200).is_some());
}

#[test]
fn mahi_inplace_resting_commit_declines_mixed_side_body_without_mutation() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let body = BlockBody {
        items: vec![
            BlockItem::Intent(signed_intent(
                &signer,
                1,
                10_000,
                IntentKind::PlaceOrder {
                    market_id: 7,
                    client_order_id: 100,
                    side: IntentOrderSide::Bid,
                    quantity_lots: 1,
                    kind: IntentOrderKind::Limit { price_ticks: 100 },
                    time_in_force: IntentTimeInForce::Gtc,
                    post_only: false,
                },
            )),
            BlockItem::Intent(signed_intent(
                &signer,
                2,
                10_000,
                IntentKind::PlaceOrder {
                    market_id: 7,
                    client_order_id: 101,
                    side: IntentOrderSide::Ask,
                    quantity_lots: 1,
                    kind: IntentOrderKind::Limit { price_ticks: 100 },
                    time_in_force: IntentTimeInForce::Gtc,
                    post_only: false,
                },
            )),
        ],
    };
    let parent = engine.head().clone();
    let positions = engine.positions.clone();
    let market_marks = engine.market_marks.clone();
    let market_index_prices = engine.market_index_prices.clone();

    let applied = engine
        .try_apply_mahi_resting_body_in_place(
            &parent,
            1,
            1_200,
            validator(9),
            body,
            &market_marks,
            &market_index_prices,
            &positions,
            std::time::Duration::ZERO,
            std::time::Instant::now(),
            false,
        )
        .expect("mixed-side body should decline without error");

    assert!(!applied);
    assert_eq!(engine.head().header.height, parent.header.height);
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should remain present")
            .next_intent_nonce,
        1
    );
    assert!(engine.resting_order(7, 11, 100, 1_200).is_none());
    assert!(engine.resting_order(7, 11, 101, 1_200).is_none());
}

#[test]
fn mahi_empty_merged_commit_advances_metadata_without_state_events() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let parent = engine.head().clone();
    let event_count = engine.runtime_events().len();
    let block = MahiBlock::signed(validator(9), 1, 1, 1_200, Vec::new(), Vec::new(), [9; 32]);

    engine
        .apply_mahi_committed_blocks_merged(&[block])
        .expect("empty Mahi commit should install metadata");

    assert_eq!(engine.head().header.height, parent.header.height + 1);
    assert!(engine.head().body.items.is_empty());
    assert_eq!(engine.runtime_events().len(), event_count);
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should remain present")
            .next_intent_nonce,
        1
    );
}

#[test]
fn mahi_committed_block_uses_pinned_oracle_prices_for_intent_replay() {
    let signer = signing_key(8);
    let mut proposer = SingleNodeEngine::new(single_node_config());
    let mut follower = SingleNodeEngine::new(single_node_config());
    for engine in [&mut proposer, &mut follower] {
        list_test_perpetual_market(engine, 7);
        engine
            .policy_store_mut()
            .insert(sample_policy())
            .expect("policy should insert");
        engine.set_isolated_collateral(7, 11, 1_000);
    }
    proposer.update_market_index_price(7, 100, 1_001);
    proposer.update_market_mark_price(7, 100, 1_001);
    follower.update_market_index_price(7, 1_000, 1_001);
    follower.update_market_mark_price(7, 1_000, 1_001);

    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let block = MahiBlock::signed_with_oracle_payloads(
        validator(9),
        1,
        1,
        1_200,
        Vec::new(),
        proposer.mahi_oracle_price_payload(),
        vec![intent],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        [9; 32],
    );

    let proposer_head = proposer
        .apply_mahi_committed_block(&block)
        .expect("proposer should commit Mahi block")
        .clone();
    let follower_head = follower
        .apply_mahi_committed_block(&block)
        .expect("follower should replay Mahi block")
        .clone();

    assert!(matches!(
        proposer_head.body.items.as_slice(),
        [BlockItem::OraclePrice(_), BlockItem::Intent(_)]
    ));
    assert_eq!(proposer_head.body, follower_head.body);
    assert_eq!(
        proposer.mark_price(7).expect("proposer mark").price_ticks,
        100
    );
    assert_eq!(
        follower.mark_price(7).expect("follower mark").price_ticks,
        100
    );
    assert_eq!(
        proposer
            .resting_order(7, 11, 99, 1_200)
            .expect("proposer order should rest")
            .price_ticks,
        100
    );
    assert_eq!(
        follower
            .resting_order(7, 11, 99, 1_200)
            .expect("follower order should rest")
            .price_ticks,
        100
    );
}

#[test]
fn mahi_oracle_price_items_do_not_roll_back_newer_committed_prices() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.update_market_index_price(7, 200, 2_000);
    engine.update_market_mark_price(7, 200, 2_000);

    let stale_block = MahiBlock::signed_with_oracle_payloads(
        validator(9),
        1,
        1,
        2_100,
        Vec::new(),
        vec![BlockOraclePrice {
            market_id: 7,
            mark: Some(BlockOracleValue {
                price_ticks: 100,
                timestamp_ms: 1_000,
            }),
            index: Some(BlockOracleValue {
                price_ticks: 100,
                timestamp_ms: 1_000,
            }),
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        [9; 32],
    );

    engine
        .apply_mahi_committed_block(&stale_block)
        .expect("stale oracle-only Mahi block should commit");

    assert_eq!(
        engine
            .mark_price(7)
            .expect("mark should remain")
            .price_ticks,
        200
    );
    assert_eq!(
        engine
            .mark_price(7)
            .expect("mark should retain timestamp")
            .timestamp_ms,
        2_000
    );
    assert_eq!(
        engine
            .index_price(7)
            .expect("index should retain timestamp")
            .timestamp_ms,
        2_000
    );

    let fresh_block = MahiBlock::signed_with_oracle_payloads(
        validator(9),
        2,
        1,
        2_600,
        Vec::new(),
        vec![BlockOraclePrice {
            market_id: 7,
            mark: Some(BlockOracleValue {
                price_ticks: 250,
                timestamp_ms: 2_500,
            }),
            index: Some(BlockOracleValue {
                price_ticks: 250,
                timestamp_ms: 2_500,
            }),
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        [9; 32],
    );

    engine
        .apply_mahi_committed_block(&fresh_block)
        .expect("fresh oracle-only Mahi block should commit");

    assert_eq!(
        engine
            .mark_price(7)
            .expect("mark should update")
            .price_ticks,
        250
    );
    assert_eq!(
        engine
            .mark_price(7)
            .expect("mark should update timestamp")
            .timestamp_ms,
        2_500
    );
    assert_eq!(
        engine
            .index_price(7)
            .expect("index should update timestamp")
            .timestamp_ms,
        2_500
    );
}

#[test]
fn market_open_interest_lots_sums_long_side_only() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    // RFC 046 — three accounts on market 7: 100-long, 100-short, flat.
    engine.positions.insert(
        (7, 1),
        PositionLedger {
            open_net_lots: 100,
            open_notional: 0,
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (7, 2),
        PositionLedger {
            open_net_lots: -100,
            open_notional: 0,
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (7, 3),
        PositionLedger {
            open_net_lots: 0,
            open_notional: 0,
            ..PositionLedger::default()
        },
    );
    assert_eq!(engine.market_open_interest_lots(7), 100);
    assert_eq!(engine.market_open_interest_lots(99), 0);

    // Partial close: shrink both sides by 30 lots.
    engine.positions.get_mut(&(7, 1)).unwrap().open_net_lots = 70;
    engine.positions.get_mut(&(7, 2)).unwrap().open_net_lots = -70;
    assert_eq!(engine.market_open_interest_lots(7), 70);

    // Empty market.
    let empty = SingleNodeEngine::new(single_node_config());
    assert_eq!(empty.market_open_interest_lots(7), 0);
}

#[test]
fn record_market_open_interest_diff_emits_per_market_changes_in_id_order() {
    // RFC 046 — multi-market block: changes on markets 7 and 8 emit
    // exactly two events, ascending market_id, both after any prior
    // events. A market whose pre==post emits no event.
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.positions.insert(
        (7, 1),
        PositionLedger {
            open_net_lots: 50,
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (8, 1),
        PositionLedger {
            open_net_lots: 30,
            ..PositionLedger::default()
        },
    );
    engine.positions.insert(
        (9, 1),
        PositionLedger {
            open_net_lots: 25,
            ..PositionLedger::default()
        },
    );
    let pre = engine.market_open_interest_snapshot();
    // Market 7: shrinks. Market 8: grows. Market 9: unchanged.
    engine.positions.get_mut(&(7, 1)).unwrap().open_net_lots = 20;
    engine.positions.get_mut(&(8, 1)).unwrap().open_net_lots = 90;
    let pre_event_count = engine.runtime_events().len();
    engine.record_market_open_interest_diff(&pre, 1, 1_000);
    let new_events = &engine.runtime_events()[pre_event_count..];
    assert_eq!(new_events.len(), 2);
    match (&new_events[0].kind, &new_events[1].kind) {
        (
            kombat_chain_streams::RuntimeEventKind::MarketOpenInterestUpdated {
                market_id: m0,
                open_interest_lots: oi0,
            },
            kombat_chain_streams::RuntimeEventKind::MarketOpenInterestUpdated {
                market_id: m1,
                open_interest_lots: oi1,
            },
        ) => {
            assert_eq!(*m0, 7, "market 7 should come first (ascending id)");
            assert_eq!(*oi0, 20);
            assert_eq!(*m1, 8);
            assert_eq!(*oi1, 90);
        }
        other => panic!(
            "expected two MarketOpenInterestUpdated events, got {:?}",
            other
        ),
    }
}

#[test]
fn record_market_open_interest_diff_emits_no_event_when_unchanged() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine.positions.insert(
        (7, 1),
        PositionLedger {
            open_net_lots: 50,
            ..PositionLedger::default()
        },
    );
    let pre = engine.market_open_interest_snapshot();
    let pre_event_count = engine.runtime_events().len();
    engine.record_market_open_interest_diff(&pre, 1, 1_000);
    assert_eq!(engine.runtime_events().len(), pre_event_count);
}

#[test]
fn mahi_merged_oracle_payload_keeps_newest_observation_per_market() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    list_test_perpetual_market(&mut engine, 7);
    engine.update_market_index_price(7, 500, 900);
    engine.update_market_mark_price(7, 500, 900);

    let fresh = BlockOraclePrice {
        market_id: 7,
        mark: Some(BlockOracleValue {
            price_ticks: 1_000,
            timestamp_ms: 2_000,
        }),
        index: Some(BlockOracleValue {
            price_ticks: 1_000,
            timestamp_ms: 2_000,
        }),
    };
    let stale = BlockOraclePrice {
        market_id: 7,
        mark: Some(BlockOracleValue {
            price_ticks: 100,
            timestamp_ms: 1_000,
        }),
        index: Some(BlockOracleValue {
            price_ticks: 100,
            timestamp_ms: 1_000,
        }),
    };
    let block_1 = MahiBlock::signed_with_oracle_payloads(
        validator(9),
        1,
        1,
        2_100,
        Vec::new(),
        vec![fresh.clone()],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        [9; 32],
    );
    let block_2 = MahiBlock::signed_with_oracle_payloads(
        validator(9),
        2,
        1,
        2_200,
        Vec::new(),
        vec![stale],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        [9; 32],
    );

    let head = engine
        .apply_mahi_committed_blocks_merged(&[block_1, block_2])
        .expect("merged oracle-only block should commit")
        .clone();

    let oracle_items = head
        .body
        .items
        .iter()
        .filter_map(|item| match item {
            BlockItem::OraclePrice(observation) => Some(observation),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(oracle_items, vec![&fresh]);
    assert_eq!(
        engine.mark_price(7).expect("mark should be applied"),
        crate::OraclePrice {
            price_ticks: 1_000,
            timestamp_ms: 2_000,
        }
    );
    assert_eq!(
        engine.index_price(7).expect("index should be applied"),
        crate::OraclePrice {
            price_ticks: 1_000,
            timestamp_ms: 2_000,
        }
    );
}

#[test]
fn mahi_merged_stale_replace_payload_rejects_without_block_failure() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let stale_replace = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::ReplaceOrder {
            market_id: 7,
            target_client_order_id: 404,
            replacement_client_order_id: 405,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let stale_replace_id = intent_id(&stale_replace);
    let block = MahiBlock::signed(
        validator(9),
        1,
        1,
        1_200,
        Vec::new(),
        vec![stale_replace],
        [9; 32],
    );

    engine
        .apply_mahi_committed_blocks_merged(&[block])
        .expect("stale replace should reject without blocking Mahi execution");

    assert!(engine.head().body.items.is_empty());
    let status = engine.intent_status(stale_replace_id);
    assert_eq!(status.stage, IntentStatusStage::Rejected);
    assert_eq!(
        status.order_rejection_reason,
        Some(OrderRejectionReason::UnknownRestingOrder)
    );
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should remain present")
            .next_intent_nonce,
        1
    );
}

#[test]
fn mahi_committed_block_applies_delegation_payload() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let timestamp_ms = 1_100;
    let delegation_id = derive_delegation_id(22, &mandate.id, timestamp_ms);
    let action_id = [8_u8; 32];
    let delegation = BlockDelegation {
        action_id,
        timestamp_ms,
        action: BlockDelegationAction::Open {
            agent: 22,
            mandate_id: mandate.id,
            performance_share_units: 1_000,
            crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
            notice_period_ms: NOTICE_PERIOD_MIN_MS,
            agent_bond: AGENT_BOND_MIN,
        },
    };
    let block = MahiBlock::signed_with_delegations(
        validator(9),
        1,
        1,
        1_200,
        Vec::new(),
        Vec::new(),
        vec![delegation],
        [9; 32],
    );

    let committed = engine
        .apply_mahi_committed_block(&block)
        .expect("Mahi delegation block should apply")
        .clone();

    assert!(matches!(
        committed.body.items.as_slice(),
        [BlockItem::Delegation(BlockDelegation {
            action_id: committed_action_id,
            timestamp_ms: 1_100,
            ..
        })] if *committed_action_id == action_id
    ));
    assert!(engine.delegation(&delegation_id).is_some());
    assert!(matches!(
        engine.processed_delegation_action(action_id),
        Some(ProcessedDelegationAction {
            receipt: DelegationActionReceipt::Open {
                delegation_id: processed_delegation_id,
            },
            ..
        }) if *processed_delegation_id == delegation_id
    ));
}

#[test]
fn mahi_committed_block_applies_market_maker_program_payload() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_000)
        .expect("market listing should succeed");
    let timestamp_ms = 1_100;
    let action = sample_market_maker_program_define_action([7]);
    let action_id = market_maker_program_action_id_for_submission(&action, timestamp_ms, None);
    let program_id = derive_market_maker_program_id(&sample_market_maker_program_input([7]));
    let program = BlockMarketMakerProgram {
        action_id,
        timestamp_ms,
        action,
    };
    let block = MahiBlock::signed_with_payloads(
        validator(9),
        1,
        1,
        1_200,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![program],
        [9; 32],
    );

    let committed = engine
        .apply_mahi_committed_block(&block)
        .expect("Mahi market-maker program block should apply")
        .clone();

    assert!(matches!(
        committed.body.items.as_slice(),
        [BlockItem::MarketMakerProgram(BlockMarketMakerProgram {
            action_id: committed_action_id,
            timestamp_ms: 1_100,
            ..
        })] if *committed_action_id == action_id
    ));
    assert!(engine.market_maker_program(&program_id).is_some());
    assert!(matches!(
        engine.processed_market_maker_program_action(action_id),
        Some(ProcessedMarketMakerProgramAction {
            receipt: MarketMakerProgramActionReceipt::Define {
                program_id: processed_program_id,
            },
            ..
        }) if *processed_program_id == program_id
    ));
}

#[test]
fn mahi_committed_block_applies_real_capital_governance_payload() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    seed_account_policy_usdc(&mut engine, 11, 250_000_000);
    let delegation = engine
        .open_delegation(
            22,
            mandate.id,
            1_000,
            CRYSTALLIZATION_CADENCE_MIN_MS,
            NOTICE_PERIOD_MIN_MS,
            AGENT_BOND_MIN,
            1_000,
        )
        .expect("delegation should open");
    let vault_id = crate::vault_id_for_delegation(&delegation.delegation_id);
    let timestamp_ms = 1_100;
    let action = BlockRealCapitalGovernanceAction::UpsertListing {
        vault_id,
        delegation_id: delegation.delegation_id,
        serving_agent: delegation.agent,
        mode: BlockVaultRealCapitalMode::CappedPilot,
        pilot_min_serving_agent_tier: None,
        tier0_pilot_exception: true,
        deposit_enabled: true,
        listed_for_discovery: true,
        per_principal_micro_usdc: 500_000_000,
        total_vault_tvl_micro_usdc: 500_000_000,
        pilot_principal_allowlist: Some(BTreeSet::from([11])),
    };
    let action_id = real_capital_governance_action_id_for_submission(&action, timestamp_ms);
    let governance = kombat_chain_consensus::BlockRealCapitalGovernance {
        action_id,
        timestamp_ms,
        action,
    };
    let block = MahiBlock::signed_with_all_payloads(
        validator(9),
        1,
        1,
        1_200,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![governance],
        [9; 32],
    );

    let committed = engine
        .apply_mahi_committed_block(&block)
        .expect("Mahi real-capital governance block should apply")
        .clone();

    assert!(matches!(
        committed.body.items.as_slice(),
        [BlockItem::RealCapitalGovernance(
            kombat_chain_consensus::BlockRealCapitalGovernance {
                action_id: committed_action_id,
                timestamp_ms: 1_100,
                ..
            }
        )] if *committed_action_id == action_id
    ));
    let listing = engine
        .real_capital_listing(&vault_id)
        .expect("listing should commit");
    assert_eq!(listing.last_updated_governance_action, action_id);
    assert!(matches!(
        engine.processed_real_capital_governance_action(action_id),
        Some(ProcessedRealCapitalGovernanceAction {
            receipt: RealCapitalGovernanceActionReceipt::UpsertListing {
                vault_id: processed_vault_id,
                delegation_id: processed_delegation_id,
                serving_agent,
            },
            ..
        }) if *processed_vault_id == vault_id
            && *processed_delegation_id == delegation.delegation_id
            && *serving_agent == delegation.agent
    ));
}

#[test]
fn mahi_duplicate_delegation_payload_is_idempotent() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let mandate = create_sample_mandate(&mut engine, 22, 1, 900);
    seed_account_policy_usdc(&mut engine, 22, AGENT_BOND_MIN as u64);
    let timestamp_ms = 1_100;
    let delegation_id = derive_delegation_id(22, &mandate.id, timestamp_ms);
    let action_id = [8_u8; 32];
    let delegation = BlockDelegation {
        action_id,
        timestamp_ms,
        action: BlockDelegationAction::Open {
            agent: 22,
            mandate_id: mandate.id,
            performance_share_units: 1_000,
            crystallization_cadence_ms: CRYSTALLIZATION_CADENCE_MIN_MS,
            notice_period_ms: NOTICE_PERIOD_MIN_MS,
            agent_bond: AGENT_BOND_MIN,
        },
    };
    let first = MahiBlock::signed_with_delegations(
        validator(9),
        1,
        1,
        1_200,
        Vec::new(),
        Vec::new(),
        vec![delegation.clone()],
        [9; 32],
    );
    let second = MahiBlock::signed_with_delegations(
        validator(9),
        2,
        1,
        1_300,
        Vec::new(),
        Vec::new(),
        vec![delegation],
        [9; 32],
    );

    engine
        .apply_mahi_committed_block(&first)
        .expect("first Mahi delegation block should apply");
    engine
        .apply_mahi_committed_block(&second)
        .expect("duplicate Mahi delegation block should skip idempotently");

    assert!(engine.delegation(&delegation_id).is_some());
    assert!(matches!(
        engine.processed_delegation_action(action_id),
        Some(ProcessedDelegationAction {
            receipt: DelegationActionReceipt::Open {
                delegation_id: processed_delegation_id,
            },
            ..
        }) if *processed_delegation_id == delegation_id
    ));
    assert_eq!(
        engine
            .runtime_events()
            .iter()
            .filter(|event| matches!(
                &event.kind,
                RuntimeEventKind::DelegationOpened {
                    delegation_id: event_delegation_id,
                    ..
                } if *event_delegation_id == delegation_id
            ))
            .count(),
        1
    );
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::DelegationActionRejected { .. }
    )));
}

#[test]
fn mahi_snapshot_roundtrip_accepts_synthetic_qc_blocks() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    let block = MahiBlock::signed(validator(9), 1, 1, 1_200, Vec::new(), Vec::new(), [9; 32]);

    engine
        .apply_mahi_committed_blocks_merged(&[block])
        .expect("empty Mahi commit should install metadata");

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot with synthetic Mahi QC should reload");

    assert_eq!(restored.head(), engine.head());
    assert_eq!(
        restored.committed_state_root(),
        engine.committed_state_root()
    );
}

#[test]
fn market_sharded_resting_place_orders_batch_across_interleaved_markets() {
    let signer = signing_key(8);
    let policy = |policy_id, account| {
        PolicyRecord::new(
            policy_id,
            account,
            signer.verifying_key().to_bytes(),
            1_000,
            172_800_000,
            PolicyState::Active,
            PolicyTerms {
                gas_budget_per_day: 1_000,
                gas_refill_per_day: 1_000,
                max_intents_per_second: 100,
                max_intents_per_day: 10_000,
                max_volume_per_day: 1_000_000,
                max_drawdown_pct: 25,
                ..PolicyTerms::default()
            },
        )
    };
    let place = |policy_id, account, market_id, client_order_id| {
        signed_intent_for_policy(
            &signer,
            policy_id,
            account,
            1,
            10_000,
            IntentKind::PlaceOrder {
                market_id,
                client_order_id,
                side: IntentOrderSide::Bid,
                quantity_lots: 1,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        )
    };
    let body = BlockBody {
        items: vec![
            BlockItem::Intent(place(42, 11, 7, 100)),
            BlockItem::Intent(place(43, 12, 8, 200)),
            BlockItem::Intent(place(44, 13, 7, 101)),
            BlockItem::Intent(place(45, 14, 8, 201)),
        ],
    };
    let mut policy_store = PolicyStore::default();
    for (policy_id, account) in [(42, 11), (43, 12), (44, 13), (45, 14)] {
        policy_store
            .insert(policy(policy_id, account))
            .expect("policy should insert");
    }
    let mut order_books = BTreeMap::<u64, kombat_chain_matching::SingleMarketBook>::new();
    let mut resting_order_mandates = BTreeMap::new();

    let output = crate::try_commit_market_sharded_resting_place_order_batch(
        &mut policy_store,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut order_books,
        &mut resting_order_mandates,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        7,
        &body,
        1,
        1_200,
    )
    .expect("market-sharded batch should not error")
    .expect("interleaved markets should batch");

    assert_eq!(output.resting_batch_stats.attempts, 2);
    assert_eq!(output.resting_batch_stats.accepted, 2);
    assert_eq!(output.resting_batch_stats.items, 4);
    for policy_id in [42, 43, 44, 45] {
        assert_eq!(
            policy_store
                .get(policy_id)
                .expect("policy should remain present")
                .next_intent_nonce,
            2
        );
    }
    let market_7 = order_books.get(&7).expect("market 7 book should exist");
    let market_8 = order_books.get(&8).expect("market 8 book should exist");
    assert_eq!(market_7.bids()[0].client_order_id, 100);
    assert_eq!(market_7.bids()[1].client_order_id, 101);
    assert_eq!(market_8.bids()[0].client_order_id, 200);
    assert_eq!(market_8.bids()[1].client_order_id, 201);

    let accepted_clients = output
        .events
        .iter()
        .filter_map(|event| match event {
            RuntimeEventKind::OrderAccepted {
                client_order_id, ..
            } => Some(*client_order_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(accepted_clients, vec![100, 200, 101, 201]);
}

#[test]
fn worker_market_matching_matches_serial_market_sharded_batch() {
    let signer = signing_key(8);
    let policy = |policy_id, account| {
        PolicyRecord::new(
            policy_id,
            account,
            signer.verifying_key().to_bytes(),
            1_000,
            172_800_000,
            PolicyState::Active,
            PolicyTerms {
                gas_budget_per_day: 1_000,
                gas_refill_per_day: 1_000,
                max_intents_per_second: 100,
                max_intents_per_day: 10_000,
                max_volume_per_day: 1_000_000,
                max_drawdown_pct: 25,
                ..PolicyTerms::default()
            },
        )
    };
    let place = |policy_id, account, market_id, client_order_id| {
        signed_intent_for_policy(
            &signer,
            policy_id,
            account,
            1,
            10_000,
            IntentKind::PlaceOrder {
                market_id,
                client_order_id,
                side: IntentOrderSide::Bid,
                quantity_lots: 1,
                kind: IntentOrderKind::Limit { price_ticks: 100 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        )
    };
    let body = BlockBody {
        items: vec![
            BlockItem::Intent(place(42, 11, 7, 100)),
            BlockItem::Intent(place(43, 12, 8, 200)),
            BlockItem::Intent(place(44, 13, 9, 300)),
            BlockItem::Intent(place(45, 14, 7, 101)),
            BlockItem::Intent(place(46, 15, 8, 201)),
            BlockItem::Intent(place(47, 16, 9, 301)),
        ],
    };
    let mut policy_store = PolicyStore::default();
    for (policy_id, account) in [(42, 11), (43, 12), (44, 13), (45, 14), (46, 15), (47, 16)] {
        policy_store
            .insert(policy(policy_id, account))
            .expect("policy should insert");
    }
    let order_books = BTreeMap::<u64, kombat_chain_matching::SingleMarketBook>::new();
    let mut serial_policy_store = policy_store.clone();
    let mut serial_order_books = order_books.clone();
    let mut serial_resting_order_mandates = BTreeMap::new();
    let mut worker_policy_store = policy_store;
    let mut worker_order_books = order_books;
    let mut worker_resting_order_mandates = BTreeMap::new();

    let serial_output =
        crate::try_commit_market_sharded_resting_place_order_batch_with_matching_mode(
            &mut serial_policy_store,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut serial_order_books,
            &mut serial_resting_order_mandates,
            &BTreeMap::new(),
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            7,
            &body,
            1,
            1_200,
            false,
        )
        .expect("serial market-sharded batch should not error")
        .expect("serial path should batch");
    let worker_output =
        crate::try_commit_market_sharded_resting_place_order_batch_with_matching_mode(
            &mut worker_policy_store,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &mut worker_order_books,
            &mut worker_resting_order_mandates,
            &BTreeMap::new(),
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeSet::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            7,
            &body,
            1,
            1_200,
            true,
        )
        .expect("worker market-sharded batch should not error")
        .expect("worker path should batch");

    assert_eq!(worker_output.events, serial_output.events);
    assert_eq!(worker_order_books, serial_order_books);
    assert_eq!(
        worker_output.resting_batch_stats.accepted,
        serial_output.resting_batch_stats.accepted
    );
    assert_eq!(
        worker_output.resting_batch_stats.items,
        serial_output.resting_batch_stats.items
    );
    for policy_id in [42, 43, 44, 45, 46, 47] {
        assert_eq!(
            worker_policy_store
                .get(policy_id)
                .expect("worker policy should exist")
                .next_intent_nonce,
            serial_policy_store
                .get(policy_id)
                .expect("serial policy should exist")
                .next_intent_nonce
        );
    }
}

#[test]
fn place_order_intent_admits_while_market_data_is_stale() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");
    engine.update_market_mark_price(7, 103, 1_000);
    engine.update_market_oracle_config(
        7,
        kombat_chain_oracle::OracleConfig {
            max_staleness_ms: 100,
        },
        1_000,
    );

    let intent = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );

    let queued_intent_id = intent_id(&intent);
    let admitted_id = engine
        .submit_intent(intent, 1_101)
        .expect("stale market should defer front-of-queue order intents");
    assert_eq!(admitted_id, queued_intent_id);
    assert!(engine.intent_mempool().contains_intent(queued_intent_id));
    let status = engine.intent_status(queued_intent_id);
    assert_eq!(
        status.pending_reason,
        Some(crate::IntentPendingReason::StaleMarketData)
    );
}

#[test]
fn terminal_frontier_rejection_cascades_to_queued_successor_intents() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let first = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 100,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let second = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 101,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let third = signed_intent(
        &signer,
        3,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 102,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let first_id = intent_id(&first);
    let second_id = intent_id(&second);
    let third_id = intent_id(&third);

    engine
        .submit_intent(first, 1_100)
        .expect("first intent should admit");
    engine
        .submit_intent(second, 1_101)
        .expect("second intent should admit");
    engine
        .submit_intent(third, 1_102)
        .expect("third intent should admit");

    let first_block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("first block should produce");
    assert_eq!(first_block.body.items.len(), 1);

    // First intent executed normally.
    assert_eq!(
        engine.intent_status(first_id).stage,
        IntentStatusStage::Executed
    );
    // Second intent was the frontier nonce and hit self-trade prevention.
    assert_eq!(
        engine.intent_status(second_id).rejection_reason,
        Some(IntentRejectionReason::InternalError)
    );
    assert_eq!(
        engine.intent_status(second_id).order_rejection_reason,
        Some(OrderRejectionReason::SelfTradePrevented)
    );
    // Third intent cascade-rejected because its predecessor's nonce slot
    // will never land. It must not stay in the mempool pinning the
    // frontier.
    assert_eq!(
        engine.intent_status(third_id).stage,
        IntentStatusStage::Rejected
    );
    assert_eq!(
        engine.intent_status(third_id).rejection_reason,
        Some(IntentRejectionReason::PredecessorIntentRejected)
    );
    assert!(!engine.intent_mempool().contains_intent(second_id));
    assert!(!engine.intent_mempool().contains_intent(third_id));

    // A subsequent block is empty because the mempool is drained. The
    // policy nonce frontier is unchanged (rejected intents do not burn
    // the nonce), so the client can resubmit starting at nonce 2.
    let second_block = engine
        .produce_block_from_mempool(1_300, 10)
        .expect("second block should still produce");
    assert!(second_block.body.items.is_empty());
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should still exist")
            .next_intent_nonce,
        2
    );

    // Recovery path: resubmitting at the unconsumed frontier nonce works
    // and the chain moves forward.
    let resume = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 103,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 104 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let resume_id = intent_id(&resume);
    engine
        .submit_intent(resume, 1_400)
        .expect("resubmitted intent at frontier should admit");
    let third_block = engine
        .produce_block_from_mempool(1_500, 10)
        .expect("third block should produce");
    assert_eq!(third_block.body.items.len(), 1);
    assert_eq!(
        engine.intent_status(resume_id).stage,
        IntentStatusStage::Executed
    );
    assert_eq!(
        engine
            .policy_store()
            .get(42)
            .expect("policy should still exist")
            .next_intent_nonce,
        3
    );
}

#[test]
fn terminal_frontier_rejection_cascade_is_scoped_per_policy() {
    let signer_a = signing_key(8);
    let signer_b = signing_key(9);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy a should insert");
    engine
        .policy_store_mut()
        .insert(PolicyRecord::new(
            43,
            12,
            signer_b.verifying_key().to_bytes(),
            1_000,
            172_800_000,
            PolicyState::Active,
            PolicyTerms {
                gas_budget_per_day: 100,
                gas_refill_per_day: 100,
                max_intents_per_second: 2,
                max_intents_per_day: 10,
                max_volume_per_day: 1_000,
                max_drawdown_pct: 25,
                ..PolicyTerms::default()
            },
        ))
        .expect("policy b should insert");

    // Policy A: frontier self-trade rejection, then an orphaned successor.
    let a_first = signed_intent(
        &signer_a,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 100,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let a_second = signed_intent(
        &signer_a,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 101,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let a_third = signed_intent(
        &signer_a,
        3,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 102,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    // Policy B: two clean bids that should commit even though policy A
    // suffered a cascading rejection in the same block.
    let b_first = signed_intent_for_policy(
        &signer_b,
        43,
        12,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 200,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 101 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let b_second = signed_intent_for_policy(
        &signer_b,
        43,
        12,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 201,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 100 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );

    let a_third_id = intent_id(&a_third);
    let b_first_id = intent_id(&b_first);
    let b_second_id = intent_id(&b_second);

    engine
        .submit_intent(a_first, 1_100)
        .expect("a1 should admit");
    engine
        .submit_intent(a_second, 1_101)
        .expect("a2 should admit");
    engine
        .submit_intent(a_third, 1_102)
        .expect("a3 should admit");
    engine
        .submit_intent(b_first, 1_103)
        .expect("b1 should admit");
    engine
        .submit_intent(b_second, 1_104)
        .expect("b2 should admit");

    let block = engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block should produce");
    // Three committed items: a1, b1, b2. a2 rejected, a3 cascade-rejected.
    assert_eq!(block.body.items.len(), 3);

    // Policy B is unaffected by policy A's cascade.
    assert_eq!(
        engine.intent_status(b_first_id).stage,
        IntentStatusStage::Executed
    );
    assert_eq!(
        engine.intent_status(b_second_id).stage,
        IntentStatusStage::Executed
    );
    assert_eq!(
        engine
            .policy_store()
            .get(43)
            .expect("policy b should exist")
            .next_intent_nonce,
        3
    );

    // Policy A's third intent was cascade-rejected, not left pending.
    assert_eq!(
        engine.intent_status(a_third_id).stage,
        IntentStatusStage::Rejected
    );
    assert_eq!(
        engine.intent_status(a_third_id).rejection_reason,
        Some(IntentRejectionReason::PredecessorIntentRejected)
    );
}

#[test]
fn terminal_frontier_rejection_cascades_across_many_queued_successors() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    // Nonce 1 commits cleanly; nonce 2 self-trades against it; nonces
    // 3..=8 are queued successors that must all cascade-reject in the
    // same pass so the mempool drains and later blocks aren't empty
    // forever.
    let bid = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 100,
            side: IntentOrderSide::Bid,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let crosser = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 101,
            side: IntentOrderSide::Ask,
            quantity_lots: 1,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    engine
        .submit_intent(bid, 1_100)
        .expect("nonce 1 should admit");
    engine
        .submit_intent(crosser, 1_101)
        .expect("nonce 2 should admit");

    let mut successor_ids = Vec::new();
    for (offset, nonce) in (3u64..=8).enumerate() {
        let client_order_id = 110 + offset as u64;
        let successor = signed_intent(
            &signer,
            nonce,
            10_000,
            IntentKind::PlaceOrder {
                market_id: 7,
                client_order_id,
                side: IntentOrderSide::Bid,
                quantity_lots: 1,
                kind: IntentOrderKind::Limit { price_ticks: 90 },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        );
        successor_ids.push(intent_id(&successor));
        engine
            .submit_intent(successor, 1_102 + nonce)
            .expect("successor intent should admit");
    }

    let block = engine
        .produce_block_from_mempool(1_200, 20)
        .expect("block should produce");
    assert_eq!(block.body.items.len(), 1);

    for id in &successor_ids {
        assert!(!engine.intent_mempool().contains_intent(*id));
        assert_eq!(engine.intent_status(*id).stage, IntentStatusStage::Rejected);
        assert_eq!(
            engine.intent_status(*id).rejection_reason,
            Some(IntentRejectionReason::PredecessorIntentRejected)
        );
    }

    // Next block is empty because the mempool is now fully drained.
    let follow_up = engine
        .produce_block_from_mempool(1_300, 20)
        .expect("follow-up block should produce");
    assert!(follow_up.body.items.is_empty());
}

#[test]
fn replace_order_intent_replaces_resting_book_order() {
    let signer = signing_key(8);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .policy_store_mut()
        .insert(sample_policy())
        .expect("policy should insert");

    let place = signed_intent(
        &signer,
        1,
        10_000,
        IntentKind::PlaceOrder {
            market_id: 7,
            client_order_id: 99,
            side: IntentOrderSide::Ask,
            quantity_lots: 5,
            kind: IntentOrderKind::Limit { price_ticks: 103 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );
    let replace = signed_intent(
        &signer,
        2,
        10_000,
        IntentKind::ReplaceOrder {
            market_id: 7,
            target_client_order_id: 99,
            replacement_client_order_id: 100,
            side: IntentOrderSide::Ask,
            quantity_lots: 4,
            kind: IntentOrderKind::Limit { price_ticks: 102 },
            time_in_force: IntentTimeInForce::Gtc,
            post_only: false,
        },
    );

    engine
        .submit_intent(place, 1_100)
        .expect("place intent should admit");
    engine
        .submit_intent(replace, 1_110)
        .expect("replace intent should admit");
    engine
        .produce_block_from_mempool(1_200, 10)
        .expect("block production should succeed");

    assert!(engine.resting_order(7, 11, 99, 1_200).is_none());
    let replaced = engine
        .resting_order(7, 11, 100, 1_200)
        .expect("replacement should rest");
    assert_eq!(replaced.price_ticks, 102);
    assert_eq!(replaced.remaining_lots, 4);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::OrderReplaced {
            market_id,
            account_id,
            target_client_order_id,
            replacement_client_order_id,
            ..
        } if *market_id == 7
            && *account_id == 11
            && *target_client_order_id == 99
            && *replacement_client_order_id == 100
    )));
}
