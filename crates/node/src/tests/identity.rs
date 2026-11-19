use super::*;

#[test]
fn single_node_engine_materializes_tier0_identity_and_persists_snapshot() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            signing_key(1).verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("account should create");
    engine
        .create_account(
            22,
            signing_key(2).verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");

    assert!(engine.agent_identity(11).is_none());
    let record = engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: "Scout 001".into(),
                description: "Conservative BTC scout".into(),
                kind: AgentKind::Unknown,
                operator: Some(22),
                contact_uri: Some("https://example.invalid/agents/scout-001".into()),
            },
            1_200,
        )
        .expect("identity should register");

    assert_eq!(record.account, 11);
    assert_eq!(record.tier, AgentIdentityTier::Tier0Unattested);
    assert_eq!(record.operator, Some(22));
    assert_eq!(record.display_name, "Scout 001");
    assert_eq!(record.registered_at_timestamp_ms, 1_200);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::IdentityRegistered {
            account: 11,
            tier: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            display_name,
            kind: kombat_chain_streams::IdentityAgentKind::Unknown,
            operator: Some(22),
        } if display_name == "Scout 001"
    )));

    let duplicate = engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: String::new(),
                description: String::new(),
                kind: AgentKind::Unknown,
                operator: None,
                contact_uri: None,
            },
            1_300,
        )
        .expect_err("duplicate identity should reject");
    assert!(matches!(
        duplicate,
        NodeError::InvalidIdentity {
            account: 11,
            reason: "identity_already_registered",
        }
    ));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    assert_eq!(restored.agent_identity(11), Some(&record));
}

#[test]
fn single_node_engine_initializes_rfc051_vendor_state_and_persists_snapshot() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let apple = engine
        .vendor_cap_profile(TeeVendor::AppleSecureEnclave)
        .expect("apple cap profile should exist");
    assert_eq!(apple.cap_multiplier_decimal_string(), "0.4");
    assert!(!apple.composition_eligibility);
    let intel = engine
        .vendor_cap_profile(TeeVendor::IntelSgxDcap)
        .expect("intel cap profile should exist");
    assert_eq!(intel.cap_multiplier_decimal_string(), "1.0");
    assert!(intel.composition_eligibility);
    assert!(engine.vendor_roots(None, false).is_empty());
    assert!(!engine.tee_attestation_enabled());

    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(intel_vendor_root([7; 32], Some(1)));
    assert_eq!(
        engine
            .vendor_roots(Some(TeeVendor::IntelSgxDcap), false)
            .len(),
        1
    );
    engine
        .produce_empty_block(1_200)
        .expect("height one should make the root inactive");
    assert!(engine
        .vendor_roots(Some(TeeVendor::IntelSgxDcap), false)
        .is_empty());
    assert_eq!(
        engine
            .vendor_roots(Some(TeeVendor::IntelSgxDcap), true)
            .len(),
        1
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    assert_eq!(
        restored
            .vendor_cap_profile(TeeVendor::AppleSecureEnclave)
            .expect("apple cap profile should restore")
            .cap_multiplier_decimal_string(),
        "0.4"
    );
    assert_eq!(
        restored.vendor_roots(Some(TeeVendor::IntelSgxDcap), true),
        engine.vendor_roots(Some(TeeVendor::IntelSgxDcap), true)
    );
    let mirror_key = attestation_vendor_roots_state_key(AttestationVerifyVendor::IntelSgxDcap);
    let mirror = AttestationVendorRootsState::try_from_slice(
        &restored
            .get_committed_state(&mirror_key)
            .expect("restored snapshot should hydrate execution root mirror"),
    )
    .expect("execution root mirror should deserialize");
    assert_eq!(mirror.vendor, AttestationVerifyVendor::IntelSgxDcap);
    assert_eq!(mirror.roots.len(), 1);
    assert_eq!(mirror.roots[0].root_id, [7; 32]);
    assert_eq!(
        mirror.roots[0].payload,
        borsh::to_vec(&intel_vendor_root([7; 32], Some(1)).payload)
            .expect("payload should serialize")
    );

    let in_memory_restored = SingleNodeEngine::from_snapshot(engine.snapshot());
    assert!(
        in_memory_restored
            .get_committed_state(&mirror_key)
            .is_some(),
        "in-memory snapshot restore should hydrate execution root mirror"
    );
}

#[test]
fn execution_verifier_decodes_node_vendor_root_payload_variants() {
    let expected_claim = borsh::to_vec(&AttestationClaim::RunsCodeHash {
        code_hash: [4; 32],
        model_hash: Some([5; 32]),
    })
    .expect("attestation claim should serialize");
    let nonce = [9_u8; 32];

    for (index, vendor) in TeeVendor::all().into_iter().enumerate() {
        let mut engine = SingleNodeEngine::new(single_node_config());
        engine
            .tee_vendor_roots
            .entry(vendor)
            .or_default()
            .push(vendor_root_fixture(vendor, [index as u8 + 1; 32]));
        engine.sync_tee_vendor_roots_execution_mirror();

        let execution_vendor = execution_vendor_for(vendor);
        let context = ExecutionContext {
            chain_id: 7,
            height: 1,
            timestamp_ms: 1_000,
            parent_state_root: engine.committed_state_root(),
        };
        let mut checkpoint = engine.executor.state_store().checkpoint();
        checkpoint.set(&evm_block_hash_state_key(0), &nonce);
        let calldata = encode_attestation_verify_calldata(
            execution_vendor,
            &[1, 2, 3],
            nonce,
            0,
            &expected_claim,
        );

        let output = engine
            .executor
            .dispatch_evm_precompile(
                &context,
                EvmPrecompileCall {
                    protocol_version: 1,
                    address: evm_address_from_u64(0x1007),
                    caller: Address::ZERO,
                    calldata: &calldata,
                    read_only: true,
                    scheme: CallScheme::Call,
                },
                &mut checkpoint,
            )
            .expect("attestation verifier precompile should dispatch");

        assert!(!output.reverted, "vendor {vendor:?}");
        assert_eq!(
            decode_abi_word_u64(&output.returned, 7),
            AttestationVerifyFailure::QuoteFormatVersionUnsupported as u64,
            "vendor {vendor:?} payload should decode in execution"
        );
    }
}

#[test]
fn tee_attestation_action_quote_uses_shared_execution_verifier() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(vendor_root_fixture(TeeVendor::IntelSgxDcap, [5_u8; 32]));
    engine.sync_tee_vendor_roots_execution_mirror();
    let freshness_anchor = engine.head().header.height;
    let nonce = block_hash(engine.head());
    engine
        .produce_empty_block(1_150)
        .expect("freshness parent should commit");
    engine
        .apply_committed_state_updates(
            [(evm_block_hash_state_key(freshness_anchor), nonce.to_vec())],
            std::iter::empty(),
        )
        .expect("freshness anchor should mirror into execution state");

    let action = TeeAttestationAction::Register(RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: Some([5_u8; 32]),
        },
        expires_at_timestamp_ms: 1_200 + TEE_ATTESTATION_MIN_EXPIRY_MS,
        quote_bytes: vec![1, 2, 3, 4],
        nonce,
        freshness_anchor_block: freshness_anchor,
        vendor: TeeVendor::IntelSgxDcap,
    });

    let outcome = engine
        .verify_tee_attestation_action_quote(&action, TeeVendor::IntelSgxDcap)
        .expect("verifier request should build");
    assert_eq!(
        outcome.failure_reason,
        AttestationVerifyFailure::QuoteFormatVersionUnsupported
    );
}

#[test]
fn tee_attestation_mandate_match_requires_bound_claim() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(vendor_root_fixture(TeeVendor::IntelSgxDcap, [5_u8; 32]));
    let mandate_id = [9_u8; 32];
    let other_mandate_id = [10_u8; 32];
    let mut record = tee_attestation_record([7_u8; 32]);
    record.claim = AttestationClaim::BoundToMandate { mandate_id };
    engine.index_tee_attestation_record(&record);
    engine
        .agent_attestations_by_account
        .entry(record.account)
        .or_default()
        .insert(record.record_id);
    engine
        .agent_attestations
        .insert(record.record_id, record.clone());

    assert!(engine.has_matching_tee_attestation(11, 1_200, engine.head().header.height));
    assert!(engine.has_matching_tee_attestation_for_mandate(
        11,
        &mandate_id,
        1_200,
        engine.head().header.height
    ));
    assert!(!engine.has_matching_tee_attestation_for_mandate(
        11,
        &other_mandate_id,
        1_200,
        engine.head().header.height
    ));

    let code_only_signer = signing_key(12);
    engine
        .create_account(
            12,
            code_only_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("code-only account should create");
    engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 12,
                display_name: "Code-only".into(),
                description: String::new(),
                kind: AgentKind::Unknown,
                operator: Some(22),
                contact_uri: None,
            },
            1_110,
        )
        .expect("code-only identity should register");
    let mut runs_code_only = tee_attestation_record([8_u8; 32]);
    runs_code_only.account = 12;
    runs_code_only.claim = AttestationClaim::RunsCodeHash {
        code_hash: [4_u8; 32],
        model_hash: None,
    };
    engine.index_tee_attestation_record(&runs_code_only);
    engine
        .agent_attestations_by_account
        .entry(runs_code_only.account)
        .or_default()
        .insert(runs_code_only.record_id);
    engine
        .agent_attestations
        .insert(runs_code_only.record_id, runs_code_only.clone());
    assert_eq!(
        engine.recompute_agent_identity_tier(12, 1_200, engine.head().header.height),
        AgentIdentityTier::Tier2TeeAttested
    );
    assert!(engine.has_matching_tee_attestation(12, 1_200, engine.head().header.height));
    assert!(!engine.has_matching_tee_attestation_for_mandate(
        12,
        &mandate_id,
        1_200,
        engine.head().header.height
    ));
}

#[test]
fn vendor_root_revocation_cascades_to_tee_attestations_and_tier() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(VendorRoot {
            valid_until_block: Some(1),
            ..vendor_root_fixture(TeeVendor::IntelSgxDcap, [5_u8; 32])
        });
    let record = tee_attestation_record([7_u8; 32]);
    engine.index_tee_attestation_record(&record);
    engine
        .agent_attestations_by_account
        .entry(record.account)
        .or_default()
        .insert(record.record_id);
    engine
        .agent_attestations
        .insert(record.record_id, record.clone());
    assert_eq!(
        engine.recompute_agent_identity_tier(11, 1_200, engine.head().header.height),
        AgentIdentityTier::Tier2TeeAttested
    );
    assert!(engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, [5_u8; 32]))
        .is_some_and(|entries| entries.contains(&(11, record.record_id))));

    engine
        .produce_empty_block(1_300)
        .expect("root revocation cascade should apply at valid_until block");

    let revoked = engine
        .attestation(&record.record_id)
        .expect("record should remain for audit");
    assert_eq!(revoked.revoked_at_block, Some(1));
    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier0Unattested)
    );
    assert!(!engine
        .tee_attestations_by_vendor_root
        .contains_key(&(TeeVendor::IntelSgxDcap, [5_u8; 32])));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationRevoked {
            account: 11,
            record_id,
            reason: kombat_chain_streams::IdentityAttestationRevocationReason::VendorRootRevoked,
            tier_before: kombat_chain_streams::IdentityTierStatus::Tier2TeeAttested,
            tier_after: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
        } if *record_id == record.record_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            from: kombat_chain_streams::IdentityTierStatus::Tier2TeeAttested,
            to: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            reason: kombat_chain_streams::IdentityTierTransitionReason::VendorRootChanged,
        }
    )));
    SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("post-cascade snapshot should restore");
}

#[test]
fn tee_governance_vendor_root_actions_commit_via_block_items() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let payload = intel_vendor_root([0; 32], None).payload;
    let valid_from_block = engine
        .head()
        .header
        .height
        .saturating_add(VENDOR_ROOT_NOTICE_BLOCKS)
        .saturating_add(1);
    let introduce = BlockTeeGovernanceAction::IntroduceVendorRoot {
        vendor: BlockTeeVendor::IntelSgxDcap,
        payload: vendor_root_payload_to_block(&payload),
        valid_from_block,
    };
    let too_short_notice = BlockTeeGovernanceAction::IntroduceVendorRoot {
        vendor: BlockTeeVendor::IntelSgxDcap,
        payload: vendor_root_payload_to_block(&payload),
        valid_from_block: valid_from_block.saturating_sub(1),
    };
    let too_short_notice_id = tee_governance_action_id_for_submission(&too_short_notice, 1_250);
    assert!(matches!(
        engine
            .submit_tee_governance_action(too_short_notice_id, too_short_notice, 1_250)
            .expect_err("notice must cover the next committed block"),
        NodeError::InvalidTeeGovernance {
            reason: "vendor_root_notice_too_short",
            ..
        }
    ));
    let introduce_id = tee_governance_action_id_for_submission(&introduce, 1_300);
    assert_eq!(
        engine
            .submit_tee_governance_action(introduce_id, introduce.clone(), 1_300)
            .expect("introduce action should queue"),
        TeeGovernanceActionSubmissionStatus::Accepted
    );
    assert!(engine
        .vendor_roots(Some(TeeVendor::IntelSgxDcap), true)
        .is_empty());

    let block = engine
        .produce_block_from_mempool(1_300, 10)
        .expect("governance action should commit");
    assert!(matches!(
        block.body.items.last(),
        Some(BlockItem::TeeGovernance(governance))
            if governance.action_id == introduce_id
    ));
    assert!(engine.pending_tee_governance_actions.is_empty());
    let root_id = derive_vendor_root_id(TeeVendor::IntelSgxDcap, &payload);
    let roots = engine.vendor_roots(Some(TeeVendor::IntelSgxDcap), true);
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].root_id, root_id);
    assert_eq!(roots[0].valid_from_block, valid_from_block);
    assert_eq!(roots[0].introduced_by_governance_action, introduce_id);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::VendorRootIntroduced {
            root_id: event_root_id,
            valid_from_block: event_valid_from_block,
            ..
        } if *event_root_id == root_id && *event_valid_from_block == valid_from_block
    )));
    assert!(
        engine
            .vendor_roots(Some(TeeVendor::IntelSgxDcap), false)
            .is_empty(),
        "future-valid root should be visible as pending but not active"
    );
    let mirror_key = attestation_vendor_roots_state_key(AttestationVerifyVendor::IntelSgxDcap);
    let mirror = AttestationVendorRootsState::try_from_slice(
        &engine
            .get_committed_state(&mirror_key)
            .expect("committed governance action should update execution mirror"),
    )
    .expect("execution mirror should deserialize");
    assert_eq!(mirror.roots.len(), 1);
    assert_eq!(mirror.roots[0].root_id, root_id);
    SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should accept future-valid vendor roots in the notice window");

    let valid_until_block = engine
        .head()
        .header
        .height
        .saturating_add(VENDOR_ROOT_NOTICE_BLOCKS)
        .saturating_add(1);
    let revoke = BlockTeeGovernanceAction::RevokeVendorRoot {
        vendor: BlockTeeVendor::IntelSgxDcap,
        root_id,
        valid_until_block,
    };
    let revoke_id = tee_governance_action_id_for_submission(&revoke, 1_400);
    assert_eq!(
        engine
            .submit_tee_governance_action(revoke_id, revoke, 1_400)
            .expect("revoke action should queue"),
        TeeGovernanceActionSubmissionStatus::Accepted
    );
    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("revoke action should commit");
    let roots = engine.vendor_roots(Some(TeeVendor::IntelSgxDcap), true);
    assert_eq!(roots[0].valid_until_block, Some(valid_until_block));
    assert_eq!(roots[0].revoked_by_governance_action, Some(revoke_id));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::VendorRootRevoked {
            root_id: event_root_id,
            valid_until_block: event_valid_until_block,
            ..
        } if *event_root_id == root_id && *event_valid_until_block == valid_until_block
    )));
    assert!(matches!(
        engine.processed_tee_governance_actions.get(&revoke_id),
        Some(ProcessedTeeGovernanceAction {
            receipt: TeeGovernanceActionReceipt::RevokeVendorRoot { .. },
            ..
        })
    ));
}

#[test]
fn tee_governance_cap_profile_updates_schedule_and_activate_via_block_items() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let current_profile = engine
        .vendor_cap_profile(TeeVendor::IntelSgxDcap)
        .expect("launch profile should exist")
        .clone();
    let valid_from_block = engine
        .head()
        .header
        .height
        .saturating_add(TEE_GOVERNANCE_NOTICE_BLOCKS)
        .saturating_add(1);
    let update = BlockTeeGovernanceAction::UpdateVendorCapProfile {
        vendor: BlockTeeVendor::IntelSgxDcap,
        cap_multiplier_microunits: 700_000,
        composition_eligibility: false,
        valid_from_block,
    };
    let too_short_notice = BlockTeeGovernanceAction::UpdateVendorCapProfile {
        vendor: BlockTeeVendor::IntelSgxDcap,
        cap_multiplier_microunits: 700_000,
        composition_eligibility: false,
        valid_from_block: valid_from_block.saturating_sub(1),
    };
    let too_short_notice_id = tee_governance_action_id_for_submission(&too_short_notice, 1_250);
    assert!(matches!(
        engine
            .submit_tee_governance_action(too_short_notice_id, too_short_notice, 1_250)
            .expect_err("cap-profile update notice should cover the next committed block"),
        NodeError::InvalidTeeGovernance {
            reason: "vendor_cap_profile_notice_too_short",
            ..
        }
    ));

    let update_id = tee_governance_action_id_for_submission(&update, 1_300);
    assert_eq!(
        engine
            .submit_tee_governance_action(update_id, update.clone(), 1_300)
            .expect("cap-profile update should queue"),
        TeeGovernanceActionSubmissionStatus::Accepted
    );
    let duplicate = BlockTeeGovernanceAction::UpdateVendorCapProfile {
        vendor: BlockTeeVendor::IntelSgxDcap,
        cap_multiplier_microunits: 800_000,
        composition_eligibility: false,
        valid_from_block: valid_from_block.saturating_add(1),
    };
    let duplicate_id = tee_governance_action_id_for_submission(&duplicate, 1_301);
    assert!(matches!(
        engine
            .submit_tee_governance_action(duplicate_id, duplicate, 1_301)
            .expect_err("only one pending cap-profile update per vendor is allowed"),
        NodeError::InvalidTeeGovernance {
            reason: "vendor_cap_profile_update_already_pending",
            ..
        }
    ));

    let block = engine
        .produce_block_from_mempool(1_300, 10)
        .expect("cap-profile governance action should commit");
    assert!(matches!(
        block.body.items.last(),
        Some(BlockItem::TeeGovernance(governance))
            if governance.action_id == update_id
    ));
    assert_eq!(
        engine
            .vendor_cap_profile(TeeVendor::IntelSgxDcap)
            .expect("profile should still exist"),
        &current_profile,
        "notice-window commit must not mutate the active profile immediately"
    );
    let pending_updates = engine.pending_vendor_cap_profile_updates(Some(TeeVendor::IntelSgxDcap));
    assert_eq!(pending_updates.len(), 1);
    assert_eq!(pending_updates[0].governance_action, update_id);
    assert_eq!(pending_updates[0].valid_from_block, valid_from_block);
    assert!(matches!(
        engine.processed_tee_governance_actions.get(&update_id),
        Some(ProcessedTeeGovernanceAction {
            receipt: TeeGovernanceActionReceipt::UpdateVendorCapProfile { .. },
            ..
        })
    ));
    SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should preserve pending cap-profile updates");

    engine.apply_due_tee_vendor_cap_profile_updates(valid_from_block.saturating_sub(1), 1_450);
    assert_eq!(
        engine
            .vendor_cap_profile(TeeVendor::IntelSgxDcap)
            .expect("profile should still exist"),
        &current_profile
    );
    engine.apply_due_tee_vendor_cap_profile_updates(valid_from_block, 1_500);
    let updated = engine
        .vendor_cap_profile(TeeVendor::IntelSgxDcap)
        .expect("profile should update");
    assert_eq!(updated.cap_multiplier_decimal_string(), "0.7");
    assert!(!updated.composition_eligibility);
    assert_eq!(updated.last_updated_governance_action, update_id);
    assert!(engine
        .pending_vendor_cap_profile_updates(Some(TeeVendor::IntelSgxDcap))
        .is_empty());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::VendorCapProfileUpdated {
            vendor,
            old_cap_multiplier_microunits,
            new_cap_multiplier_microunits,
            old_composition_eligibility,
            new_composition_eligibility,
            governance_action,
        } if *vendor == IdentityAttestationVendor::IntelSgxDcap
            && *old_cap_multiplier_microunits == current_profile.cap_multiplier_microunits
            && *new_cap_multiplier_microunits == 700_000
            && *old_composition_eligibility == current_profile.composition_eligibility
            && !*new_composition_eligibility
            && *governance_action == update_id
    )));
}

#[test]
fn snapshot_bytes_preserve_tee_quote_verified_evidence() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            signing_key(1).verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("account should create");
    engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: "TEE Scout".into(),
                description: String::new(),
                kind: AgentKind::Unknown,
                operator: None,
                contact_uri: None,
            },
            1_200,
        )
        .expect("identity should register");
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(VendorRoot {
            vendor: TeeVendor::IntelSgxDcap,
            root_id: [7; 32],
            payload: VendorRootPayload::IntelSgxRoot {
                root_cert_der: vec![1],
                tcb_info_signing_cert_der: vec![2],
                tcb_info_url: "https://example.invalid/tcb.json".into(),
                crl_der: vec![3],
            },
            valid_from_block: 0,
            valid_until_block: None,
            introduced_by_governance_action: [1; 32],
            revoked_by_governance_action: None,
        });
    let record = AttestationRecord {
        record_id: [3; 32],
        account: 11,
        kind: AttestationKind::TeeEnclave,
        attestor: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4; 32],
            model_hash: Some([5; 32]),
        },
        evidence: AttestationEvidence::TeeQuoteVerified {
            vendor: TeeVendor::IntelSgxDcap,
            vendor_root_id: [7; 32],
            enclave_id: [8; 32],
            signer: [9; 32],
            verified_at_block: 0,
        },
        issued_at_block: 0,
        issued_at_timestamp_ms: 1_300,
        expires_at_timestamp_ms: 3_700_000,
        signature: [0; 64],
        revoked_at_block: None,
        verified: true,
    };
    engine.index_tee_attestation_record(&record);
    engine.agent_attestations.insert(record.record_id, record);
    engine
        .agent_attestations_by_account
        .entry(11)
        .or_default()
        .insert([3; 32]);
    assert_eq!(
        engine.recompute_agent_identity_tier(11, 1_300, engine.head().header.height),
        AgentIdentityTier::Tier2TeeAttested
    );

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    let restored = restored
        .attestation(&[3; 32])
        .expect("tee attestation should restore");
    match restored.evidence {
        AttestationEvidence::TeeQuoteVerified {
            vendor,
            vendor_root_id,
            enclave_id,
            signer,
            verified_at_block,
        } => {
            assert_eq!(vendor, TeeVendor::IntelSgxDcap);
            assert_eq!(vendor_root_id, [7; 32]);
            assert_eq!(enclave_id, [8; 32]);
            assert_eq!(signer, [9; 32]);
            assert_eq!(verified_at_block, 0);
        }
        AttestationEvidence::OperatorSignature => panic!("expected tee evidence"),
    }
}

#[test]
fn snapshot_bytes_reject_tee_attestation_evidence_without_root() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            signing_key(1).verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("account should create");
    let mut snapshot = engine.persisted_snapshot();
    snapshot.agent_identities.insert(
        11,
        AgentIdentityRecord {
            account: 11,
            tier: AgentIdentityTier::Tier0Unattested,
            operator: None,
            kind: AgentKind::Unknown,
            display_name: String::new(),
            description: String::new(),
            contact_uri: None,
            registered_at_block: 0,
            registered_at_timestamp_ms: 1_100,
            last_updated_block: 0,
            last_updated_timestamp_ms: 1_100,
        },
    );
    snapshot.agent_attestations.insert(
        [3; 32],
        AttestationRecord {
            record_id: [3; 32],
            account: 11,
            kind: AttestationKind::TeeEnclave,
            attestor: 11,
            claim: AttestationClaim::RunsCodeHash {
                code_hash: [4; 32],
                model_hash: None,
            },
            evidence: AttestationEvidence::TeeQuoteVerified {
                vendor: TeeVendor::IntelSgxDcap,
                vendor_root_id: [7; 32],
                enclave_id: [8; 32],
                signer: [9; 32],
                verified_at_block: 0,
            },
            issued_at_block: 0,
            issued_at_timestamp_ms: 1_200,
            expires_at_timestamp_ms: 3_700_000,
            signature: [0; 64],
            revoked_at_block: None,
            verified: true,
        },
    );
    snapshot
        .agent_attestations_by_account
        .entry(11)
        .or_default()
        .insert([3; 32]);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject tee evidence without a known root");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("attestation evidence invalid"));
}

#[test]
fn snapshot_bytes_reject_invalid_tee_vendor_state() {
    let engine = SingleNodeEngine::new(single_node_config());
    let mut missing_profile = engine.persisted_snapshot();
    missing_profile
        .tee_vendor_cap_profiles
        .remove(&TeeVendor::AwsNitro);
    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&missing_profile).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject missing cap profile");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("tee cap profile set mismatch"));

    let mut bad_root = engine.persisted_snapshot();
    bad_root
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(VendorRoot {
            vendor: TeeVendor::IntelSgxDcap,
            root_id: [9; 32],
            payload: VendorRootPayload::AmdSevRoot {
                ark_cert_der: vec![1],
                ask_cert_der: vec![2],
                crl_der: vec![3],
            },
            valid_from_block: 0,
            valid_until_block: None,
            introduced_by_governance_action: [1; 32],
            revoked_by_governance_action: None,
        });
    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&bad_root).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject mismatched vendor root payload");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("tee vendor root payload invalid"));

    let mut zero_cap = engine.persisted_snapshot();
    zero_cap
        .tee_vendor_cap_profiles
        .get_mut(&TeeVendor::IntelSgxDcap)
        .expect("intel cap profile should exist")
        .cap_multiplier_microunits = 0;
    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&zero_cap).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject zero cap multiplier");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("tee cap profile invalid"));

    let mut missing_cap_governance_action = engine.persisted_snapshot();
    missing_cap_governance_action
        .tee_vendor_cap_profiles
        .get_mut(&TeeVendor::IntelSgxDcap)
        .expect("intel cap profile should exist")
        .cap_multiplier_microunits = 700_000;
    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&missing_cap_governance_action).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject non-launch cap profile without governance action");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("tee cap profile governance action missing"));

    let mut stale_scheduled_cap_update = engine.persisted_snapshot();
    stale_scheduled_cap_update
        .scheduled_tee_vendor_cap_profile_updates
        .insert(
            [4; 32],
            ScheduledTeeVendorCapProfileUpdate {
                governance_action: [4; 32],
                vendor: TeeVendor::IntelSgxDcap,
                cap_multiplier_microunits: 700_000,
                composition_eligibility: false,
                valid_from_block: 0,
            },
        );
    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&stale_scheduled_cap_update).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject stale scheduled cap update");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("tee scheduled cap profile update invalid"));

    let mut duplicate_root = engine.persisted_snapshot();
    duplicate_root
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(VendorRoot {
            vendor: TeeVendor::IntelSgxDcap,
            root_id: [8; 32],
            payload: VendorRootPayload::IntelSgxRoot {
                root_cert_der: vec![1],
                tcb_info_signing_cert_der: vec![2],
                tcb_info_url: String::new(),
                crl_der: vec![3],
            },
            valid_from_block: 0,
            valid_until_block: None,
            introduced_by_governance_action: [1; 32],
            revoked_by_governance_action: None,
        });
    duplicate_root
        .tee_vendor_roots
        .entry(TeeVendor::AmdSevSnp)
        .or_default()
        .push(VendorRoot {
            vendor: TeeVendor::AmdSevSnp,
            root_id: [8; 32],
            payload: VendorRootPayload::AmdSevRoot {
                ark_cert_der: vec![1],
                ask_cert_der: vec![2],
                crl_der: vec![3],
            },
            valid_from_block: 0,
            valid_until_block: None,
            introduced_by_governance_action: [1; 32],
            revoked_by_governance_action: None,
        });
    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&duplicate_root).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject duplicate vendor root ids");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("tee vendor root duplicate"));

    let oversized_payloads = [
        VendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![0; 8_193],
            tcb_info_signing_cert_der: vec![1],
            tcb_info_url: String::new(),
            crl_der: vec![2],
        },
        VendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![0],
            tcb_info_signing_cert_der: vec![1; 8_193],
            tcb_info_url: String::new(),
            crl_der: vec![2],
        },
        VendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![0],
            tcb_info_signing_cert_der: vec![1],
            tcb_info_url: "x".repeat(257),
            crl_der: vec![2],
        },
        VendorRootPayload::IntelSgxRoot {
            root_cert_der: vec![0],
            tcb_info_signing_cert_der: vec![1],
            tcb_info_url: String::new(),
            crl_der: vec![2; 32_769],
        },
    ];
    for (idx, payload) in oversized_payloads.into_iter().enumerate() {
        let mut oversized_root = engine.persisted_snapshot();
        oversized_root
            .tee_vendor_roots
            .entry(TeeVendor::IntelSgxDcap)
            .or_default()
            .push(VendorRoot {
                vendor: TeeVendor::IntelSgxDcap,
                root_id: [idx as u8 + 10; 32],
                payload,
                valid_from_block: 0,
                valid_until_block: None,
                introduced_by_governance_action: [1; 32],
                revoked_by_governance_action: None,
            });
        let err = SingleNodeEngine::from_snapshot_bytes(
            &borsh::to_vec(&oversized_root).expect("snapshot should serialize"),
        )
        .expect_err("snapshot should reject oversized vendor root payload");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("tee vendor root payload invalid"));
    }
}

#[test]
fn signed_identity_action_registers_and_consumes_account_nonce() {
    let signer = signing_key(1);
    let wrong_signer = signing_key(9);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(11, signer.verifying_key().to_bytes(), [0; 32], 1_100)
        .expect("account should create");
    let action = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout 001".into(),
        description: "Conservative BTC scout".into(),
        kind: AgentKind::Unknown,
        operator: None,
        contact_uri: None,
    });
    let signed = signed_identity_action(&signer, 7, 0, action.clone());

    let record = engine
        .submit_identity_action(signed.clone(), 1_200)
        .expect("identity action should register");
    assert_eq!(record.account, 11);
    assert_eq!(record.display_name, "Scout 001");
    assert_eq!(engine.identity_action_nonce(11), 1);

    let replay = engine
        .submit_identity_action(signed, 1_300)
        .expect_err("replayed identity action should reject");
    assert!(matches!(
        replay,
        NodeError::InvalidIdentityActionNonce {
            account: 11,
            expected: 1,
            actual: 0,
        }
    ));

    let bad_signature = signed_identity_action(&wrong_signer, 7, 1, action);
    let err = engine
        .submit_identity_action(bad_signature, 1_400)
        .expect_err("wrong account owner should reject");
    assert!(matches!(
        err,
        NodeError::InvalidIdentityActionSignature { account: 11 }
    ));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    assert_eq!(restored.identity_action_nonce(11), 1);
}

#[test]
fn signed_identity_action_registers_operator_attestation_and_raises_tier() {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    let register = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout 001".into(),
        description: "Conservative BTC scout".into(),
        kind: AgentKind::Unknown,
        operator: Some(22),
        contact_uri: None,
    });
    engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 0, register),
            1_200,
        )
        .expect("identity action should register");

    let mandate_id = [7_u8; 32];
    let attestation_action =
        IdentityAction::RegisterOperatorAttestation(RegisterOperatorAttestationInput {
            attestor: 22,
            subject: 11,
            claim: AttestationClaim::BoundToMandate { mandate_id },
            expires_at_timestamp_ms: 1_200 + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
        });
    let record = engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 0, attestation_action),
            1_200,
        )
        .expect("operator attestation should register");

    assert_eq!(record.account, 11);
    assert_eq!(record.tier, AgentIdentityTier::Tier1OperatorAttested);
    assert_eq!(engine.identity_action_nonce(11), 1);
    assert_eq!(engine.identity_attestor_nonce(22), 1);
    assert!(engine.has_matching_operator_attestation(11, Some(&mandate_id), 1_200));
    assert_eq!(engine.active_attestation_count_for_account(11, 1_200), 1);
    let attestation = engine
        .attestations_for_account(11)
        .next()
        .expect("attestation should be indexed");
    assert_eq!(attestation.kind, AttestationKind::OperatorSelf);
    assert_eq!(attestation.attestor, 22);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationRegistered {
            account: 11,
            kind: kombat_chain_streams::IdentityAttestationKind::OperatorSelf,
            attestor: 22,
            tier_before: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            tier_after: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            ..
        }
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            from: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            to: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            reason: kombat_chain_streams::IdentityTierTransitionReason::AttestationRegistered,
        }
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should restore");
    assert_eq!(
        restored.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier1OperatorAttested)
    );
    assert_eq!(restored.active_attestation_count_for_account(11, 1_200), 1);
    assert_eq!(restored.identity_attestor_nonce(22), 1);
}

#[test]
fn operator_attestation_expiry_sweep_emits_partial_expiry_and_demotes_tier() {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    let register = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout 001".into(),
        description: "Conservative BTC scout".into(),
        kind: AgentKind::Unknown,
        operator: Some(22),
        contact_uri: None,
    });
    engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 0, register),
            1_200,
        )
        .expect("identity action should register");

    let mandate_id = [7_u8; 32];
    let first_expires_at = 1_200 + OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
    let second_expires_at = first_expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
    let first_attestation_action =
        IdentityAction::RegisterOperatorAttestation(RegisterOperatorAttestationInput {
            attestor: 22,
            subject: 11,
            claim: AttestationClaim::BoundToMandate { mandate_id },
            expires_at_timestamp_ms: first_expires_at,
        });
    engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 0, first_attestation_action),
            1_200,
        )
        .expect("first operator attestation should register");
    let second_attestation_action =
        IdentityAction::RegisterOperatorAttestation(RegisterOperatorAttestationInput {
            attestor: 22,
            subject: 11,
            claim: AttestationClaim::BoundToMandate { mandate_id },
            expires_at_timestamp_ms: second_expires_at,
        });
    engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 1, second_attestation_action),
            1_201,
        )
        .expect("second operator attestation should register");
    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier1OperatorAttested)
    );
    assert_eq!(engine.active_attestation_count_for_account(11, 1_201), 2);

    engine
        .produce_empty_block(first_expires_at)
        .expect("first expiry block should produce");

    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier1OperatorAttested)
    );
    assert_eq!(
        engine.active_attestation_count_for_account(11, first_expires_at),
        1
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationExpired {
            account: 11,
            tier_before: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            tier_after: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            ..
        }
    )));
    assert!(!engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            from: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            to: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            reason: kombat_chain_streams::IdentityTierTransitionReason::AttestationExpired,
        }
    )));

    engine
        .produce_empty_block(second_expires_at)
        .expect("second expiry block should produce");

    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier0Unattested)
    );
    assert_eq!(
        engine.active_attestation_count_for_account(11, second_expires_at),
        0
    );
    assert!(!engine.has_matching_operator_attestation(11, Some(&mandate_id), second_expires_at));
    let expired_events = engine
        .runtime_events()
        .iter()
        .filter(|event| {
            matches!(
                &event.kind,
                RuntimeEventKind::AttestationExpired { account: 11, .. }
            )
        })
        .count();
    assert_eq!(expired_events, 2);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            from: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            to: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            reason: kombat_chain_streams::IdentityTierTransitionReason::AttestationExpired,
        }
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("post-expiry snapshot should restore");
    assert_eq!(
        restored.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier0Unattested)
    );
}

#[test]
fn signed_identity_action_refreshes_and_revokes_operator_attestation() {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    let register = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout 001".into(),
        description: "Conservative BTC scout".into(),
        kind: AgentKind::Unknown,
        operator: Some(22),
        contact_uri: None,
    });
    engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 0, register),
            1_200,
        )
        .expect("identity action should register");

    let mandate_id = [7_u8; 32];
    let expires_at = 1_200 + OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
    let attestation_action =
        IdentityAction::RegisterOperatorAttestation(RegisterOperatorAttestationInput {
            attestor: 22,
            subject: 11,
            claim: AttestationClaim::BoundToMandate { mandate_id },
            expires_at_timestamp_ms: expires_at,
        });
    engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 0, attestation_action),
            1_200,
        )
        .expect("operator attestation should register");
    let record_id = engine
        .attestations_for_account(11)
        .next()
        .expect("attestation should be indexed")
        .record_id;

    let refreshed_expires_at = expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
    let refresh_action =
        IdentityAction::RefreshOperatorAttestation(RefreshOperatorAttestationInput {
            attestor: 22,
            record_id,
            new_expires_at_timestamp_ms: refreshed_expires_at,
        });
    let refreshed = engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 1, refresh_action),
            1_300,
        )
        .expect("operator attestation should refresh");
    assert_eq!(refreshed.tier, AgentIdentityTier::Tier1OperatorAttested);
    assert_eq!(engine.identity_attestor_nonce(22), 2);
    assert_eq!(
        engine
            .attestation(&record_id)
            .map(|record| record.expires_at_timestamp_ms),
        Some(refreshed_expires_at)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationRefreshed {
            account: 11,
            record_id: event_record_id,
            new_expires_at_timestamp_ms,
        } if *event_record_id == record_id && *new_expires_at_timestamp_ms == refreshed_expires_at
    )));

    let revoke_action = IdentityAction::RevokeAttestation(RevokeAttestationInput {
        revoker: 11,
        record_id,
    });
    let revoked = engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 1, revoke_action.clone()),
            1_400,
        )
        .expect("subject should revoke attestation");
    assert_eq!(revoked.tier, AgentIdentityTier::Tier0Unattested);
    assert_eq!(engine.identity_action_nonce(11), 2);
    assert_eq!(engine.identity_attestor_nonce(22), 2);
    assert_eq!(
        engine
            .attestation(&record_id)
            .and_then(|record| record.revoked_at_block),
        Some(engine.head().header.height)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationRevoked {
            account: 11,
            record_id: event_record_id,
            reason: kombat_chain_streams::IdentityAttestationRevocationReason::SubjectRequested,
            tier_before: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            tier_after: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
        } if *event_record_id == record_id
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            from: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            to: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            reason: kombat_chain_streams::IdentityTierTransitionReason::AttestationRevoked,
        }
    )));

    let replay = engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 1, revoke_action),
            1_500,
        )
        .expect_err("replayed subject revoke should reject by nonce");
    assert!(matches!(
        replay,
        NodeError::InvalidIdentityActionNonce {
            account: 11,
            expected: 2,
            actual: 1,
        }
    ));
    let refresh_after_revoke =
        IdentityAction::RefreshOperatorAttestation(RefreshOperatorAttestationInput {
            attestor: 22,
            record_id,
            new_expires_at_timestamp_ms: refreshed_expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
        });
    let err = engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 2, refresh_after_revoke),
            1_500,
        )
        .expect_err("refresh after revoke should reject");
    assert!(matches!(
        err,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "attestation_revoked",
        }
    ));
}

#[test]
fn signed_identity_action_updates_operator_and_deletes_identity() {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let next_operator_signer = signing_key(3);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    engine
        .create_account(
            33,
            next_operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("next operator account should create");

    let register = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout 001".into(),
        description: "Conservative BTC scout".into(),
        kind: AgentKind::Unknown,
        operator: Some(22),
        contact_uri: None,
    });
    engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 0, register),
            1_200,
        )
        .expect("identity action should register");
    let attestation_action =
        IdentityAction::RegisterOperatorAttestation(RegisterOperatorAttestationInput {
            attestor: 22,
            subject: 11,
            claim: AttestationClaim::BoundToMandate {
                mandate_id: [7_u8; 32],
            },
            expires_at_timestamp_ms: 1_200 + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
        });
    engine
        .submit_identity_action(
            signed_identity_action(&operator_signer, 7, 0, attestation_action),
            1_200,
        )
        .expect("operator attestation should register");
    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier1OperatorAttested)
    );

    let display_update = IdentityAction::UpdateDescriptive(UpdateAgentIdentityInput {
        account: 11,
        update: DescriptiveUpdate::DisplayName("Scout Alpha".into()),
    });
    let updated = engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 1, display_update),
            1_300,
        )
        .expect("display name update should apply");
    assert_eq!(updated.display_name, "Scout Alpha");
    assert_eq!(updated.tier, AgentIdentityTier::Tier1OperatorAttested);
    assert_eq!(engine.identity_action_nonce(11), 2);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::IdentityUpdated {
            account: 11,
            field,
            before: Some(before),
            after: Some(after),
        } if field == "display_name" && before == "Scout 001" && after == "Scout Alpha"
    )));

    let operator_update = IdentityAction::UpdateDescriptive(UpdateAgentIdentityInput {
        account: 11,
        update: DescriptiveUpdate::Operator(Some(33)),
    });
    let downgraded = engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 2, operator_update),
            1_400,
        )
        .expect("operator update should apply");
    assert_eq!(downgraded.operator, Some(33));
    assert_eq!(downgraded.tier, AgentIdentityTier::Tier0Unattested);
    assert_eq!(engine.identity_action_nonce(11), 3);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::IdentityUpdated {
            account: 11,
            field,
            before: Some(before),
            after: Some(after),
        } if field == "operator" && before == "22" && after == "33"
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            from: kombat_chain_streams::IdentityTierStatus::Tier1OperatorAttested,
            to: kombat_chain_streams::IdentityTierStatus::Tier0Unattested,
            reason: kombat_chain_streams::IdentityTierTransitionReason::OperatorChanged,
        }
    )));

    let delete = IdentityAction::Delete(DeleteAgentIdentityInput { account: 11 });
    let deleted = engine
        .submit_identity_action(signed_identity_action(&subject_signer, 7, 3, delete), 1_500)
        .expect("delete should apply");
    assert_eq!(deleted.account, 11);
    assert!(engine.agent_identity(11).is_none());
    assert_eq!(engine.identity_action_nonce(11), 4);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::IdentityDeleted { account: 11 }
    )));

    let reregister = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout Reborn".into(),
        description: String::new(),
        kind: AgentKind::Unknown,
        operator: None,
        contact_uri: None,
    });
    let err = engine
        .submit_identity_action(
            signed_identity_action(&subject_signer, 7, 4, reregister),
            1_600,
        )
        .expect_err("deleted identity tombstone should block re-registration");
    assert!(matches!(
        err,
        NodeError::InvalidIdentity {
            account: 11,
            reason: "identity_already_registered",
        }
    ));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("post-delete snapshot should restore");
    assert!(restored.agent_identity(11).is_none());
    assert_eq!(restored.identity_action_nonce(11), 4);
    assert_eq!(restored.attestations_for_account(11).count(), 1);
}

#[test]
fn identity_update_validation_rejects_noops_unknowns_and_oversized_fields() {
    let signer = signing_key(1);
    let operator_signer = signing_key(2);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(11, signer.verifying_key().to_bytes(), [0; 32], 1_100)
        .expect("account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    engine
        .create_account(
            44,
            signing_key(4).verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("unregistered account should create");
    engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: "Scout 001".into(),
                description: "Conservative BTC scout".into(),
                kind: AgentKind::Unknown,
                operator: Some(22),
                contact_uri: Some("https://example.invalid/agents/scout-001".into()),
            },
            1_200,
        )
        .expect("identity should register");

    let updated = engine
        .update_agent_identity(
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::Description("Updated scout".into()),
            },
            1_300,
        )
        .expect("description update should apply");
    assert_eq!(updated.description, "Updated scout");
    let updated = engine
        .update_agent_identity(
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::ContactUri(None),
            },
            1_400,
        )
        .expect("contact uri clear should apply");
    assert_eq!(updated.contact_uri, None);

    let cases = [
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::DisplayName("Scout 001".into()),
            },
            "identity_update_noop",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::Description("Updated scout".into()),
            },
            "identity_update_noop",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::Kind(AgentKind::Unknown),
            },
            "identity_update_noop",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::ContactUri(None),
            },
            "identity_update_noop",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::DisplayName(
                    "x".repeat(super::super::MAX_AGENT_IDENTITY_DISPLAY_NAME_BYTES + 1),
                ),
            },
            "display_name_too_long",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::Description(
                    "x".repeat(super::super::MAX_AGENT_IDENTITY_DESCRIPTION_BYTES + 1),
                ),
            },
            "description_too_long",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::ContactUri(Some(
                    "x".repeat(super::super::MAX_AGENT_IDENTITY_CONTACT_URI_BYTES + 1),
                )),
            },
            "contact_uri_too_long",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::Operator(Some(0)),
            },
            "invalid_operator",
        ),
        (
            UpdateAgentIdentityInput {
                account: 11,
                update: DescriptiveUpdate::Operator(Some(99)),
            },
            "unknown_operator",
        ),
        (
            UpdateAgentIdentityInput {
                account: 44,
                update: DescriptiveUpdate::DisplayName("Unregistered".into()),
            },
            "unknown_identity",
        ),
        (
            UpdateAgentIdentityInput {
                account: 99,
                update: DescriptiveUpdate::DisplayName("Unknown".into()),
            },
            "unknown_account",
        ),
    ];
    for (input, expected_reason) in cases {
        let account = input.account;
        let err = engine
            .update_agent_identity(input, 1_500)
            .expect_err("invalid update should reject");
        assert!(
            matches!(
                err,
                NodeError::InvalidIdentity {
                    account: actual_account,
                    reason
                } if actual_account == account && reason == expected_reason
            ),
            "unexpected update error: {err:?}"
        );
    }
}

#[test]
fn identity_delete_validation_rejects_unknowns() {
    let signer = signing_key(1);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(11, signer.verifying_key().to_bytes(), [0; 32], 1_100)
        .expect("account should create");

    let cases = [
        (DeleteAgentIdentityInput { account: 0 }, "invalid_account"),
        (DeleteAgentIdentityInput { account: 99 }, "unknown_account"),
        (DeleteAgentIdentityInput { account: 11 }, "unknown_identity"),
    ];
    for (input, expected_reason) in cases {
        let account = input.account;
        let err = engine
            .delete_agent_identity(input, 1_200)
            .expect_err("invalid delete should reject");
        assert!(
            matches!(
                err,
                NodeError::InvalidIdentity {
                    account: actual_account,
                    reason
                } if actual_account == account && reason == expected_reason
            ),
            "unexpected delete error: {err:?}"
        );
    }
}

#[test]
fn operator_attestation_lifecycle_validation_rejects_bad_inputs() {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let other_signer = signing_key(3);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    engine
        .create_account(33, other_signer.verifying_key().to_bytes(), [0; 32], 1_100)
        .expect("other account should create");
    engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: "Scout 001".into(),
                description: "Conservative BTC scout".into(),
                kind: AgentKind::Unknown,
                operator: Some(22),
                contact_uri: None,
            },
            1_200,
        )
        .expect("identity should register");

    let expires_at = 1_200 + OPERATOR_ATTESTATION_MIN_EXPIRY_MS;
    engine
        .register_operator_attestation(
            RegisterOperatorAttestationInput {
                attestor: 22,
                subject: 11,
                claim: AttestationClaim::BoundToMandate {
                    mandate_id: [7_u8; 32],
                },
                expires_at_timestamp_ms: expires_at,
            },
            [1_u8; 64],
            1_200,
        )
        .expect("operator attestation should register");
    let record_id = engine
        .attestations_for_account(11)
        .next()
        .expect("attestation should index")
        .record_id;
    let base_record = engine
        .attestation(&record_id)
        .expect("attestation should exist")
        .clone();

    let mut wrong_kind = base_record.clone();
    wrong_kind.record_id = [8_u8; 32];
    wrong_kind.kind = AttestationKind::ThirdParty;
    engine
        .agent_attestations
        .insert(wrong_kind.record_id, wrong_kind.clone());
    let mut unverified = base_record.clone();
    unverified.record_id = [9_u8; 32];
    unverified.verified = false;
    engine
        .agent_attestations
        .insert(unverified.record_id, unverified.clone());

    let refresh_cases = [
        (
            RefreshOperatorAttestationInput {
                attestor: 22,
                record_id: [0_u8; 32],
                new_expires_at_timestamp_ms: expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            1_300,
            "unknown_attestation",
        ),
        (
            RefreshOperatorAttestationInput {
                attestor: 22,
                record_id: wrong_kind.record_id,
                new_expires_at_timestamp_ms: expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            1_300,
            "invalid_attestation_kind",
        ),
        (
            RefreshOperatorAttestationInput {
                attestor: 33,
                record_id,
                new_expires_at_timestamp_ms: expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            1_300,
            "identity_attestor_mismatch",
        ),
        (
            RefreshOperatorAttestationInput {
                attestor: 22,
                record_id: unverified.record_id,
                new_expires_at_timestamp_ms: expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            1_300,
            "attestation_unverified",
        ),
        (
            RefreshOperatorAttestationInput {
                attestor: 22,
                record_id,
                new_expires_at_timestamp_ms: expires_at,
            },
            1_300,
            "attestation_expiry_not_extended",
        ),
        (
            RefreshOperatorAttestationInput {
                attestor: 22,
                record_id,
                new_expires_at_timestamp_ms: 1_300
                    + super::super::OPERATOR_ATTESTATION_MAX_EXPIRY_MS
                    + 1,
            },
            1_300,
            "attestation_expiry_too_late",
        ),
        (
            RefreshOperatorAttestationInput {
                attestor: 22,
                record_id,
                new_expires_at_timestamp_ms: expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            expires_at,
            "attestation_expired",
        ),
    ];
    for (input, timestamp_ms, expected_reason) in refresh_cases {
        let account = input.attestor;
        let err = engine
            .refresh_operator_attestation(input, timestamp_ms)
            .expect_err("invalid refresh should reject");
        assert!(
            matches!(
                err,
                NodeError::InvalidIdentityAction {
                    account: actual_account,
                    reason
                } if actual_account == account && reason == expected_reason
            ),
            "unexpected refresh error: {err:?}"
        );
    }

    let invalid_revoker = engine
        .revoke_attestation(
            RevokeAttestationInput {
                revoker: 33,
                record_id,
            },
            1_400,
        )
        .expect_err("unrelated revoker should reject");
    assert!(matches!(
        invalid_revoker,
        NodeError::InvalidIdentityAction {
            account: 33,
            reason: "invalid_revoker",
        }
    ));
    let unknown_revoker = engine
        .revoke_attestation(
            RevokeAttestationInput {
                revoker: 44,
                record_id,
            },
            1_400,
        )
        .expect_err("unknown revoker should reject");
    assert!(matches!(
        unknown_revoker,
        NodeError::InvalidIdentityAction {
            account: 44,
            reason: "unknown_revoker",
        }
    ));

    let second_record = engine
        .register_operator_attestation(
            RegisterOperatorAttestationInput {
                attestor: 22,
                subject: 11,
                claim: AttestationClaim::BoundToMandate {
                    mandate_id: [8_u8; 32],
                },
                expires_at_timestamp_ms: expires_at + OPERATOR_ATTESTATION_MIN_EXPIRY_MS,
            },
            [2_u8; 64],
            1_500,
        )
        .expect("second operator attestation should register");
    let revoked = engine
        .revoke_attestation(
            RevokeAttestationInput {
                revoker: 22,
                record_id: second_record.record_id,
            },
            1_600,
        )
        .expect("attestor revoke should apply");
    assert_eq!(revoked.revoked_at_block, Some(engine.head().header.height));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationRevoked {
            account: 11,
            record_id: event_record_id,
            reason: kombat_chain_streams::IdentityAttestationRevocationReason::AttestorRevoked,
            ..
        } if *event_record_id == second_record.record_id
    )));
    let double_revoke = engine
        .revoke_attestation(
            RevokeAttestationInput {
                revoker: 22,
                record_id: second_record.record_id,
            },
            1_700,
        )
        .expect_err("double revoke should reject");
    assert!(matches!(
        double_revoke,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "attestation_revoked",
        }
    ));
}

#[test]
fn signed_identity_action_rejects_chain_account_and_unknown_account() {
    let signer = signing_key(1);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(11, signer.verifying_key().to_bytes(), [0; 32], 1_100)
        .expect("account should create");
    let action = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 11,
        display_name: "Scout 001".into(),
        description: "Conservative BTC scout".into(),
        kind: AgentKind::Unknown,
        operator: None,
        contact_uri: None,
    });

    let wrong_chain = signed_identity_action(&signer, 8, 0, action.clone());
    let err = engine
        .submit_identity_action(wrong_chain, 1_200)
        .expect_err("wrong chain should reject");
    assert!(matches!(
        err,
        NodeError::InvalidIdentityAction {
            account: 11,
            reason: "chain_mismatch",
        }
    ));

    let mut account_mismatch = signed_identity_action(&signer, 7, 0, action);
    account_mismatch.account = 12;
    let err = engine
        .submit_identity_action(account_mismatch, 1_200)
        .expect_err("account mismatch should reject");
    assert!(matches!(
        err,
        NodeError::InvalidIdentityAction {
            account: 12,
            reason: "account_mismatch",
        }
    ));

    let unknown_action = IdentityAction::Register(RegisterAgentIdentityInput {
        account: 99,
        display_name: "Unknown".into(),
        description: String::new(),
        kind: AgentKind::Unknown,
        operator: None,
        contact_uri: None,
    });
    let unknown_account = signed_identity_action(&signer, 7, 0, unknown_action);
    let err = engine
        .submit_identity_action(unknown_account, 1_200)
        .expect_err("unknown account should reject");
    assert!(matches!(
        err,
        NodeError::InvalidIdentityAction {
            account: 99,
            reason: "unknown_account",
        }
    ));
}

#[test]
fn tee_attestation_signing_payload_binds_quote_and_dual_nonce_shape() {
    let mut action = TeeAttestationAction::Register(RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: None,
        },
        expires_at_timestamp_ms: 3_701_200,
        quote_bytes: vec![1, 2, 3],
        nonce: [3_u8; 32],
        freshness_anchor_block: 7,
        vendor: TeeVendor::IntelSgxDcap,
    });
    let base = tee_attestation_signing_bytes(7, &action, 5, Some(6));
    let changed_subject_nonce = tee_attestation_signing_bytes(7, &action, 5, Some(7));
    assert_ne!(base, changed_subject_nonce);

    if let TeeAttestationAction::Register(input) = &mut action {
        input.quote_bytes = vec![1, 2, 4];
    }
    let changed_quote = tee_attestation_signing_bytes(7, &action, 5, Some(6));
    assert_ne!(base, changed_quote);
}

#[test]
fn tee_attestation_record_id_binds_quote_hash_and_dual_signature_shape() {
    let mut input = RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: None,
        },
        expires_at_timestamp_ms: 3_701_200,
        quote_bytes: vec![1, 2, 3],
        nonce: [3_u8; 32],
        freshness_anchor_block: 7,
        vendor: TeeVendor::IntelSgxDcap,
    };
    let attestor_signature = [8_u8; 64];
    let subject_signature = [9_u8; 64];
    let base = derive_tee_attestation_record_id(
        &input,
        10,
        1_200,
        &attestor_signature,
        Some(&subject_signature),
    );

    input.quote_bytes = vec![1, 2, 4];
    let changed_quote = derive_tee_attestation_record_id(
        &input,
        10,
        1_200,
        &attestor_signature,
        Some(&subject_signature),
    );
    assert_ne!(base, changed_quote);

    input.quote_bytes = vec![1, 2, 3];
    let changed_subject_signature = [10_u8; 64];
    let changed_subject = derive_tee_attestation_record_id(
        &input,
        10,
        1_200,
        &attestor_signature,
        Some(&changed_subject_signature),
    );
    assert_ne!(base, changed_subject);

    let self_attest =
        derive_tee_attestation_record_id(&input, 10, 1_200, &attestor_signature, None);
    assert_ne!(base, self_attest);
}

#[test]
fn materialize_registered_tee_attestation_stores_record_indexes_and_events() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(vendor_root_fixture(TeeVendor::IntelSgxDcap, [5_u8; 32]));
    let input = RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: Some([5_u8; 32]),
        },
        expires_at_timestamp_ms: 1_200 + TEE_ATTESTATION_MIN_EXPIRY_MS,
        quote_bytes: vec![1, 2, 3],
        nonce: [3_u8; 32],
        freshness_anchor_block: 7,
        vendor: TeeVendor::IntelSgxDcap,
    };
    let attestor_signature = [8_u8; 64];
    let subject_signature = [9_u8; 64];
    let outcome = AttestationVerifyOutcome {
        verified: true,
        code_hash: [4_u8; 32],
        model_hash: [5_u8; 32],
        enclave_id: [6_u8; 32],
        signer: [7_u8; 32],
        nonce: [3_u8; 32],
        vendor_root_id: [5_u8; 32],
        failure_reason: AttestationVerifyFailure::None,
    };

    let record = engine
        .materialize_registered_tee_attestation(
            &input,
            outcome,
            &attestor_signature,
            Some(&subject_signature),
            1_200,
        )
        .expect("verified TEE attestation should materialize");
    let expected_record_id = derive_tee_attestation_record_id(
        &input,
        engine.head().header.height,
        1_200,
        &attestor_signature,
        Some(&subject_signature),
    );

    assert_eq!(record.record_id, expected_record_id);
    assert_eq!(record.signature, attestor_signature);
    assert_eq!(
        record.evidence,
        AttestationEvidence::TeeQuoteVerified {
            vendor: TeeVendor::IntelSgxDcap,
            vendor_root_id: [5_u8; 32],
            enclave_id: [6_u8; 32],
            signer: [7_u8; 32],
            verified_at_block: engine.head().header.height,
        }
    );
    assert!(engine
        .agent_attestations_by_account
        .get(&11)
        .is_some_and(|records| records.contains(&record.record_id)));
    assert!(engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, [5_u8; 32]))
        .is_some_and(|records| records.contains(&(11, record.record_id))));
    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier2TeeAttested)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationVerified {
            account: 11,
            record_id,
            vendor: IdentityAttestationVendor::IntelSgxDcap,
            code_hash,
            enclave_id,
        } if *record_id == record.record_id
            && *code_hash == [4_u8; 32]
            && *enclave_id == [6_u8; 32]
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::TierChanged {
            account: 11,
            to: IdentityTierStatus::Tier2TeeAttested,
            reason: IdentityTierTransitionReason::AttestationRegistered,
            ..
        }
    )));
}

#[test]
fn materialize_registered_tee_attestation_rejects_inactive_verified_root() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    let input = RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: None,
        },
        expires_at_timestamp_ms: 1_200 + TEE_ATTESTATION_MIN_EXPIRY_MS,
        quote_bytes: vec![1, 2, 3],
        nonce: [3_u8; 32],
        freshness_anchor_block: 7,
        vendor: TeeVendor::IntelSgxDcap,
    };
    let outcome = |vendor_root_id| AttestationVerifyOutcome {
        verified: true,
        code_hash: [4_u8; 32],
        model_hash: [0_u8; 32],
        enclave_id: [6_u8; 32],
        signer: [7_u8; 32],
        nonce: [3_u8; 32],
        vendor_root_id,
        failure_reason: AttestationVerifyFailure::None,
    };

    let unknown_root = engine
        .materialize_registered_tee_attestation(
            &input,
            outcome([5_u8; 32]),
            &[8_u8; 64],
            None,
            1_200,
        )
        .expect_err("verified outcome for unknown root should reject");
    assert_eq!(
        unknown_root,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_unknown_vendor_root"
        }
    );

    let mut revoked_root = vendor_root_fixture(TeeVendor::IntelSgxDcap, [6_u8; 32]);
    revoked_root.valid_until_block = Some(engine.head().header.height);
    revoked_root.revoked_by_governance_action = Some([2_u8; 32]);
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(revoked_root);
    let revoked = engine
        .materialize_registered_tee_attestation(
            &input,
            outcome([6_u8; 32]),
            &[8_u8; 64],
            None,
            1_200,
        )
        .expect_err("verified outcome for revoked root should reject");
    assert_eq!(
        revoked,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_revoked"
        }
    );
    assert_eq!(engine.attestations_for_account(11).count(), 0);
}

#[test]
fn materialize_registered_tee_attestation_rejects_claim_measurement_mismatch() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(vendor_root_fixture(TeeVendor::IntelSgxDcap, [5_u8; 32]));
    let input = RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: Some([5_u8; 32]),
        },
        expires_at_timestamp_ms: 1_200 + TEE_ATTESTATION_MIN_EXPIRY_MS,
        quote_bytes: vec![1, 2, 3],
        nonce: [3_u8; 32],
        freshness_anchor_block: 7,
        vendor: TeeVendor::IntelSgxDcap,
    };
    let outcome = |code_hash, model_hash| AttestationVerifyOutcome {
        verified: true,
        code_hash,
        model_hash,
        enclave_id: [6_u8; 32],
        signer: [7_u8; 32],
        nonce: [3_u8; 32],
        vendor_root_id: [5_u8; 32],
        failure_reason: AttestationVerifyFailure::None,
    };

    for (code_hash, model_hash) in [([9_u8; 32], [5_u8; 32]), ([4_u8; 32], [9_u8; 32])] {
        let error = engine
            .materialize_registered_tee_attestation(
                &input,
                outcome(code_hash, model_hash),
                &[8_u8; 64],
                None,
                1_200,
            )
            .expect_err("claim measurement mismatch should reject");
        assert_eq!(
            error,
            NodeError::InvalidIdentityAction {
                account: 22,
                reason: "identity_tee_attestation_claim_mismatch"
            }
        );
    }
    assert_eq!(engine.attestations_for_account(11).count(), 0);
}

#[test]
fn tee_verifier_outcome_claim_guard_allows_bound_to_mandate_without_measurement_constraint() {
    let outcome = AttestationVerifyOutcome {
        verified: true,
        code_hash: [9_u8; 32],
        model_hash: [8_u8; 32],
        enclave_id: [7_u8; 32],
        signer: [6_u8; 32],
        nonce: [5_u8; 32],
        vendor_root_id: [4_u8; 32],
        failure_reason: AttestationVerifyFailure::None,
    };

    assert!(tee_verifier_outcome_matches_claim(
        &AttestationClaim::BoundToMandate {
            mandate_id: [3_u8; 32],
        },
        &outcome
    ));
}

#[test]
fn materialize_refreshed_tee_attestation_updates_evidence_and_reverse_index() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    let old_root_id = [5_u8; 32];
    let new_root_id = [9_u8; 32];
    let record = tee_attestation_record([12_u8; 32]);
    let record_id = record.record_id;
    engine.test_seed_tee_attestation_with_root(
        vendor_root_fixture(TeeVendor::IntelSgxDcap, old_root_id),
        record.clone(),
        1_200,
    );
    engine
        .tee_vendor_roots
        .entry(TeeVendor::IntelSgxDcap)
        .or_default()
        .push(vendor_root_fixture(TeeVendor::IntelSgxDcap, new_root_id));

    let same_root_expires_at = record.expires_at_timestamp_ms + TEE_ATTESTATION_MIN_EXPIRY_MS;
    let same_root = engine
        .materialize_refreshed_tee_attestation(
            &RefreshTeeAttestationInput {
                attestor: 22,
                record_id,
                new_expires_at_timestamp_ms: same_root_expires_at,
                quote_bytes: vec![1, 2, 3],
                nonce: [3_u8; 32],
                freshness_anchor_block: 7,
            },
            AttestationVerifyOutcome {
                verified: true,
                code_hash: [4_u8; 32],
                model_hash: [0_u8; 32],
                enclave_id: [6_u8; 32],
                signer: [7_u8; 32],
                nonce: [3_u8; 32],
                vendor_root_id: old_root_id,
                failure_reason: AttestationVerifyFailure::None,
            },
            1_300,
        )
        .expect("same-root refresh should materialize");
    assert_eq!(same_root.expires_at_timestamp_ms, same_root_expires_at);
    assert_eq!(
        same_root.evidence,
        AttestationEvidence::TeeQuoteVerified {
            vendor: TeeVendor::IntelSgxDcap,
            vendor_root_id: old_root_id,
            enclave_id: [6_u8; 32],
            signer: [7_u8; 32],
            verified_at_block: engine.head().header.height,
        }
    );
    assert!(engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, old_root_id))
        .is_some_and(|records| records.contains(&(11, record_id))));
    assert!(!engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, new_root_id))
        .is_some_and(|records| records.contains(&(11, record_id))));

    let before_rejected_refresh = engine
        .attestation(&record_id)
        .expect("record should exist")
        .clone();
    let changed_same_root = engine
        .materialize_refreshed_tee_attestation(
            &RefreshTeeAttestationInput {
                attestor: 22,
                record_id,
                new_expires_at_timestamp_ms: same_root_expires_at + TEE_ATTESTATION_MIN_EXPIRY_MS,
                quote_bytes: vec![1, 2, 4],
                nonce: [4_u8; 32],
                freshness_anchor_block: 8,
            },
            AttestationVerifyOutcome {
                verified: true,
                code_hash: [4_u8; 32],
                model_hash: [0_u8; 32],
                enclave_id: [10_u8; 32],
                signer: [7_u8; 32],
                nonce: [4_u8; 32],
                vendor_root_id: old_root_id,
                failure_reason: AttestationVerifyFailure::None,
            },
            1_400,
        )
        .expect_err("same-root evidence mutation should reject");
    assert_eq!(
        changed_same_root,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_evidence_changed_under_same_root"
        }
    );
    assert_eq!(
        engine.attestation(&record_id),
        Some(&before_rejected_refresh)
    );

    let rotation_expires_at = same_root_expires_at + TEE_ATTESTATION_MIN_EXPIRY_MS;
    let rotated = engine
        .materialize_refreshed_tee_attestation(
            &RefreshTeeAttestationInput {
                attestor: 22,
                record_id,
                new_expires_at_timestamp_ms: rotation_expires_at,
                quote_bytes: vec![4, 5, 6],
                nonce: [5_u8; 32],
                freshness_anchor_block: 9,
            },
            AttestationVerifyOutcome {
                verified: true,
                code_hash: [4_u8; 32],
                model_hash: [0_u8; 32],
                enclave_id: [10_u8; 32],
                signer: [11_u8; 32],
                nonce: [5_u8; 32],
                vendor_root_id: new_root_id,
                failure_reason: AttestationVerifyFailure::None,
            },
            1_500,
        )
        .expect("rotation refresh should materialize");
    assert_eq!(rotated.expires_at_timestamp_ms, rotation_expires_at);
    assert_eq!(
        rotated.evidence,
        AttestationEvidence::TeeQuoteVerified {
            vendor: TeeVendor::IntelSgxDcap,
            vendor_root_id: new_root_id,
            enclave_id: [10_u8; 32],
            signer: [11_u8; 32],
            verified_at_block: engine.head().header.height,
        }
    );
    assert!(!engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, old_root_id))
        .is_some_and(|records| records.contains(&(11, record_id))));
    assert!(engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, new_root_id))
        .is_some_and(|records| records.contains(&(11, record_id))));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::AttestationRefreshed {
            account: 11,
            record_id: event_record_id,
            new_expires_at_timestamp_ms,
        } if *event_record_id == record_id
            && *new_expires_at_timestamp_ms == rotation_expires_at
    )));
    assert_eq!(
        engine.agent_identity(11).map(|identity| identity.tier),
        Some(AgentIdentityTier::Tier2TeeAttested)
    );
}

#[test]
fn materialize_refreshed_tee_attestation_rejects_claim_measurement_mismatch() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    let root_id = [5_u8; 32];
    let mut record = tee_attestation_record([12_u8; 32]);
    record.claim = AttestationClaim::RunsCodeHash {
        code_hash: [4_u8; 32],
        model_hash: Some([5_u8; 32]),
    };
    let record_id = record.record_id;
    engine.test_seed_tee_attestation_with_root(
        vendor_root_fixture(TeeVendor::IntelSgxDcap, root_id),
        record.clone(),
        1_200,
    );
    let input = RefreshTeeAttestationInput {
        attestor: 22,
        record_id,
        new_expires_at_timestamp_ms: record.expires_at_timestamp_ms + TEE_ATTESTATION_MIN_EXPIRY_MS,
        quote_bytes: vec![1, 2, 3],
        nonce: [3_u8; 32],
        freshness_anchor_block: 7,
    };
    let outcome = |code_hash, model_hash| AttestationVerifyOutcome {
        verified: true,
        code_hash,
        model_hash,
        enclave_id: [6_u8; 32],
        signer: [7_u8; 32],
        nonce: [3_u8; 32],
        vendor_root_id: root_id,
        failure_reason: AttestationVerifyFailure::None,
    };
    let before = engine
        .attestation(&record_id)
        .expect("record should exist")
        .clone();

    for (code_hash, model_hash) in [([9_u8; 32], [5_u8; 32]), ([4_u8; 32], [9_u8; 32])] {
        let error = engine
            .materialize_refreshed_tee_attestation(&input, outcome(code_hash, model_hash), 1_300)
            .expect_err("claim measurement mismatch should reject");
        assert_eq!(
            error,
            NodeError::InvalidIdentityAction {
                account: 22,
                reason: "identity_tee_attestation_claim_mismatch"
            }
        );
        assert_eq!(engine.attestation(&record_id), Some(&before));
    }
    assert!(engine
        .tee_attestations_by_vendor_root
        .get(&(TeeVendor::IntelSgxDcap, root_id))
        .is_some_and(|records| records.contains(&(11, record_id))));
}

#[test]
fn signed_tee_attestation_action_validates_envelope_but_stays_disabled() {
    let subject_signer = signing_key(1);
    let operator_signer = signing_key(2);
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(
            11,
            subject_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("subject account should create");
    engine
        .create_account(
            22,
            operator_signer.verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("operator account should create");
    engine
        .register_agent_identity(
            RegisterAgentIdentityInput {
                account: 11,
                display_name: "Scout".into(),
                description: String::new(),
                kind: AgentKind::Unknown,
                operator: Some(22),
                contact_uri: None,
            },
            1_110,
        )
        .expect("identity should register");
    let freshness_anchor = engine.head().header.height;
    let nonce = block_hash(engine.head());
    engine
        .produce_empty_block(1_150)
        .expect("freshness parent should commit");
    let action = TeeAttestationAction::Register(RegisterTeeAttestationInput {
        attestor: 22,
        subject: 11,
        claim: AttestationClaim::RunsCodeHash {
            code_hash: [4_u8; 32],
            model_hash: None,
        },
        expires_at_timestamp_ms: 1_200 + TEE_ATTESTATION_MIN_EXPIRY_MS,
        quote_bytes: vec![1, 2, 3, 4],
        nonce,
        freshness_anchor_block: freshness_anchor,
        vendor: TeeVendor::IntelSgxDcap,
    });
    let signed = signed_tee_operator_attestation_action(
        &operator_signer,
        &subject_signer,
        7,
        0,
        0,
        action.clone(),
    );
    let error = engine
        .submit_tee_attestation_action(signed, 1_200)
        .expect_err("TEE verifier gate should reject");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_not_enabled"
        }
    );
    assert_eq!(engine.identity_attestor_nonce(22), 0);
    assert_eq!(engine.identity_action_nonce(11), 0);

    let mut corrupt_subject_signature = signed_tee_operator_attestation_action(
        &operator_signer,
        &subject_signer,
        7,
        0,
        0,
        action.clone(),
    );
    let TeeAttestationSigners::OperatorAttest {
        subject_signature, ..
    } = &mut corrupt_subject_signature.signers
    else {
        unreachable!("helper creates operator-attest envelope")
    };
    subject_signature[0] ^= 0x01;
    let error = engine
        .submit_tee_attestation_action(corrupt_subject_signature, 1_200)
        .expect_err("subject signature should be verified");
    assert_eq!(
        error,
        NodeError::InvalidIdentityActionSignature { account: 11 }
    );

    let invalid_shape = signed_tee_self_attestation_action(&operator_signer, 7, 0, action.clone());
    let error = engine
        .submit_tee_attestation_action(invalid_shape, 1_200)
        .expect_err("operator-managed attestation needs both signatures");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "invalid_signer_shape"
        }
    );

    let mut too_large_action = action.clone();
    let TeeAttestationAction::Register(input) = &mut too_large_action else {
        unreachable!("test uses register")
    };
    input.quote_bytes = vec![0_u8; MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP + 1];
    let too_large_signed = signed_tee_operator_attestation_action(
        &operator_signer,
        &subject_signer,
        7,
        0,
        0,
        too_large_action,
    );
    let error = engine
        .submit_tee_attestation_action(too_large_signed, 1_200)
        .expect_err("oversized quote should reject");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_quote_too_large"
        }
    );

    let TeeAttestationAction::Register(mut stale_input) = action else {
        unreachable!("test uses register")
    };
    stale_input.nonce = [9_u8; 32];
    let stale = TeeAttestationAction::Register(stale_input);
    let stale_signed =
        signed_tee_operator_attestation_action(&operator_signer, &subject_signer, 7, 0, 0, stale);
    let error = engine
        .submit_tee_attestation_action(stale_signed, 1_200)
        .expect_err("stale nonce should reject");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_stale_nonce"
        }
    );
}

#[test]
fn tee_attestation_freshness_rejects_current_old_and_missing_anchors() {
    let engine = SingleNodeEngine::new(single_node_config());
    let current_anchor = engine.head().header.height;
    let current_nonce = block_hash(engine.head());
    let error = engine
        .validate_tee_attestation_freshness(22, current_nonce, current_anchor)
        .expect_err("current block cannot be a committed freshness anchor");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_stale_nonce"
        }
    );

    let mut old_anchor_engine = SingleNodeEngine::new(single_node_config());
    let mut synthetic_head = old_anchor_engine.head().clone();
    synthetic_head.header.height = ATTESTATION_QUOTE_FRESHNESS_BLOCKS + 2;
    old_anchor_engine.chain.push(synthetic_head);
    let error = old_anchor_engine
        .validate_tee_attestation_freshness(22, block_hash(&old_anchor_engine.chain[0]), 0)
        .expect_err("anchor older than freshness window should reject");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_stale_nonce"
        }
    );

    let mut missing_anchor_engine = SingleNodeEngine::new(single_node_config());
    let mut synthetic_head = missing_anchor_engine.head().clone();
    synthetic_head.header.height = ATTESTATION_QUOTE_FRESHNESS_BLOCKS + 2;
    missing_anchor_engine.chain.push(synthetic_head);
    let missing_anchor = 2;
    let error = missing_anchor_engine
        .validate_tee_attestation_freshness(22, [0_u8; 32], missing_anchor)
        .expect_err("missing committed anchor block should reject");
    assert_eq!(
        error,
        NodeError::InvalidIdentityAction {
            account: 22,
            reason: "identity_tee_attestation_stale_nonce"
        }
    );
}

#[test]
fn tee_attestation_refresh_validation_rejects_bad_inputs() {
    let (mut engine, _subject_signer, _operator_signer) = seed_tee_identity_fixture();
    engine
        .create_account(
            33,
            signing_key(3).verifying_key().to_bytes(),
            [0; 32],
            1_100,
        )
        .expect("alternate attestor account should create");
    let freshness_anchor = engine.head().header.height;
    let nonce = block_hash(engine.head());
    engine
        .produce_empty_block(1_150)
        .expect("freshness parent should commit");

    let base_record = tee_attestation_record([7_u8; 32]);
    let record_id = base_record.record_id;
    let valid_new_expires_at = base_record.expires_at_timestamp_ms + TEE_ATTESTATION_MIN_EXPIRY_MS;
    engine
        .agent_attestations
        .insert(base_record.record_id, base_record.clone());

    let mut wrong_kind = base_record.clone();
    wrong_kind.record_id = [8_u8; 32];
    wrong_kind.kind = AttestationKind::OperatorSelf;
    wrong_kind.evidence = AttestationEvidence::OperatorSignature;
    engine
        .agent_attestations
        .insert(wrong_kind.record_id, wrong_kind.clone());

    let mut unverified = base_record.clone();
    unverified.record_id = [9_u8; 32];
    unverified.verified = false;
    engine
        .agent_attestations
        .insert(unverified.record_id, unverified.clone());

    let mut unknown_subject_identity = base_record.clone();
    unknown_subject_identity.record_id = [13_u8; 32];
    unknown_subject_identity.account = 44;
    engine.agent_attestations.insert(
        unknown_subject_identity.record_id,
        unknown_subject_identity.clone(),
    );

    let mut revoked = base_record.clone();
    revoked.record_id = [10_u8; 32];
    revoked.revoked_at_block = Some(1);
    engine
        .agent_attestations
        .insert(revoked.record_id, revoked.clone());

    let mut invalid_evidence = base_record.clone();
    invalid_evidence.record_id = [11_u8; 32];
    invalid_evidence.evidence = AttestationEvidence::OperatorSignature;
    engine
        .agent_attestations
        .insert(invalid_evidence.record_id, invalid_evidence.clone());

    let refresh = |attestor, record_id, new_expires_at_timestamp_ms| {
        TeeAttestationAction::Refresh(RefreshTeeAttestationInput {
            attestor,
            record_id,
            new_expires_at_timestamp_ms,
            quote_bytes: vec![1, 2, 3, 4],
            nonce,
            freshness_anchor_block: freshness_anchor,
        })
    };

    let cases = vec![
        (
            refresh(0, record_id, valid_new_expires_at),
            1_300,
            0,
            "invalid_account",
        ),
        (
            refresh(44, record_id, valid_new_expires_at),
            1_300,
            44,
            "unknown_attestor",
        ),
        (
            refresh(22, [0_u8; 32], valid_new_expires_at),
            1_300,
            22,
            "unknown_attestation",
        ),
        (
            refresh(22, wrong_kind.record_id, valid_new_expires_at),
            1_300,
            22,
            "invalid_attestation_kind",
        ),
        (
            refresh(22, unknown_subject_identity.record_id, valid_new_expires_at),
            1_300,
            22,
            "unknown_subject_identity",
        ),
        (
            refresh(33, record_id, valid_new_expires_at),
            1_300,
            33,
            "identity_attestor_mismatch",
        ),
        (
            refresh(22, unverified.record_id, valid_new_expires_at),
            1_300,
            22,
            "attestation_unverified",
        ),
        (
            refresh(22, revoked.record_id, valid_new_expires_at),
            1_300,
            22,
            "attestation_revoked",
        ),
        (
            refresh(22, record_id, valid_new_expires_at),
            base_record.expires_at_timestamp_ms,
            22,
            "attestation_expired",
        ),
        (
            refresh(22, record_id, base_record.expires_at_timestamp_ms),
            1_300,
            22,
            "attestation_expiry_not_extended",
        ),
        (
            refresh(22, invalid_evidence.record_id, valid_new_expires_at),
            1_300,
            22,
            "invalid_attestation_evidence",
        ),
        (
            refresh(22, record_id, base_record.expires_at_timestamp_ms + 50),
            1_300,
            22,
            "attestation_expiry_too_soon",
        ),
        (
            refresh(22, record_id, 1_300 + TEE_ATTESTATION_MAX_EXPIRY_MS + 1),
            1_300,
            22,
            "attestation_expiry_too_late",
        ),
    ];

    for (action, timestamp_ms, expected_account, expected_reason) in cases {
        let error = engine
            .validate_tee_attestation_action_input(&action, timestamp_ms)
            .expect_err("invalid TEE refresh should reject");
        assert!(
            matches!(
                error,
                NodeError::InvalidIdentityAction { account, reason }
                    if account == expected_account && reason == expected_reason
            ),
            "unexpected TEE refresh error: {error:?}"
        );
    }
}
