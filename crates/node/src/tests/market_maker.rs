use super::*;

#[test]
fn market_maker_identity_keeps_signer_and_economic_accounts_distinct() {
    let delegation_id = [7_u8; 32];
    let naked = MarketMakerIdentity::Account(31);
    let delegated = MarketMakerIdentity::Delegation {
        delegation_id,
        serving_agent: 31,
    };

    assert_eq!(
        super::super::market_maker_identity_signer_account(naked),
        31
    );
    assert_eq!(
        super::super::market_maker_identity_economic_account(naked),
        31
    );
    assert_eq!(
        super::super::market_maker_identity_signer_account(naked),
        super::super::market_maker_identity_economic_account(naked)
    );
    assert_eq!(
        super::super::market_maker_identity_signer_account(delegated),
        31
    );
    assert_eq!(
        super::super::market_maker_identity_economic_account(delegated),
        super::super::delegation_trading_account_id(&delegation_id)
    );
    assert_ne!(
        super::super::market_maker_identity_economic_account(delegated),
        super::super::market_maker_identity_signer_account(delegated)
    );
}

#[test]
fn one_serving_agent_two_delegations_keep_distinct_mm_economic_accounts() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(31, [31; 32], [9; 32], 1_100)
        .expect("serving agent account should create");
    let first = sample_open_delegation(&mut engine, 31, 1_200);
    let second = sample_open_delegation(&mut engine, 31, 1_300);
    let first_identity = MarketMakerIdentity::Delegation {
        delegation_id: first.delegation_id,
        serving_agent: 31,
    };
    let second_identity = MarketMakerIdentity::Delegation {
        delegation_id: second.delegation_id,
        serving_agent: 31,
    };

    assert_eq!(
        super::super::market_maker_identity_signer_account(first_identity),
        31
    );
    assert_eq!(
        super::super::market_maker_identity_signer_account(second_identity),
        31
    );
    assert_ne!(
        super::super::market_maker_identity_economic_account(first_identity),
        super::super::market_maker_identity_economic_account(second_identity)
    );
    assert_ne!(
        super::super::market_maker_enrollment_key(first_identity, 7, [1; 32]),
        super::super::market_maker_enrollment_key(second_identity, 7, [1; 32])
    );
}

#[test]
fn live_delegation_for_trading_account_excludes_terminal_delegations() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .create_account(31, [31; 32], [9; 32], 1_100)
        .expect("serving agent account should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_200);
    let trading_account = super::super::delegation_trading_account_id(&delegation.delegation_id);

    assert!(matches!(
        super::super::market_maker_identity_for_economic_account(&engine.delegations, trading_account),
        Some(MarketMakerIdentity::Delegation { delegation_id, serving_agent })
            if delegation_id == delegation.delegation_id && serving_agent == 31
    ));

    engine
        .delegations
        .get_mut(&delegation.delegation_id)
        .expect("delegation should exist")
        .status = DelegationStatus::Winding;
    assert!(matches!(
        super::super::market_maker_identity_for_economic_account(&engine.delegations, trading_account),
        Some(MarketMakerIdentity::Delegation { delegation_id, serving_agent })
            if delegation_id == delegation.delegation_id && serving_agent == 31
    ));

    engine
        .delegations
        .get_mut(&delegation.delegation_id)
        .expect("delegation should exist")
        .status = DelegationStatus::Closed;
    assert_eq!(
        super::super::market_maker_identity_for_economic_account(
            &engine.delegations,
            trading_account
        ),
        None
    );

    engine
        .delegations
        .get_mut(&delegation.delegation_id)
        .expect("delegation should exist")
        .status = DelegationStatus::Terminated;
    assert_eq!(
        super::super::market_maker_identity_for_economic_account(
            &engine.delegations,
            trading_account
        ),
        None
    );
}

#[test]
fn best_multiplier_eligible_market_maker_enrollment_uses_economic_account_identity() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, [31; 32], [9; 32], 1_150)
        .expect("serving agent should create");
    engine
        .create_account(41, [41; 32], [9; 32], 1_151)
        .expect("naked market maker should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_200);
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 3) as u64);
    seed_account_policy_usdc(&mut engine, 41, PROGRAM_BOND_MIN_MICRO_USDC as u64);

    let mut delegated_low = sample_market_maker_program_input([7]);
    delegated_low.name = "delegated-low".into();
    delegated_low.rebate_multiplier_units = 15_000;
    let delegated_low = engine
        .define_market_maker_program(delegated_low, 1_250)
        .expect("delegated low program should define");

    let mut delegated_high = sample_market_maker_program_input([7]);
    delegated_high.name = "delegated-high".into();
    delegated_high.rebate_multiplier_units = 25_000;
    let delegated_high = engine
        .define_market_maker_program(delegated_high, 1_260)
        .expect("delegated high program should define");

    let mut delegated_tie = sample_market_maker_program_input([7]);
    delegated_tie.name = "delegated-tie".into();
    delegated_tie.rebate_multiplier_units = 25_000;
    let delegated_tie = engine
        .define_market_maker_program(delegated_tie, 1_265)
        .expect("delegated tie program should define");

    let mut naked_program = sample_market_maker_program_input([7]);
    naked_program.name = "naked".into();
    naked_program.rebate_multiplier_units = 30_000;
    let naked_program = engine
        .define_market_maker_program(naked_program, 1_270)
        .expect("naked program should define");

    let delegated_identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: delegated_identity,
                market_id: 7,
                program_id: delegated_low.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("delegated low enrollment should succeed");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: delegated_identity,
                market_id: 7,
                program_id: delegated_high.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_301,
            None,
        )
        .expect("delegated high enrollment should succeed");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: delegated_identity,
                market_id: 7,
                program_id: delegated_tie.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_301,
            None,
        )
        .expect("delegated tie enrollment should succeed");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(41),
                market_id: 7,
                program_id: naked_program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_302,
            None,
        )
        .expect("naked enrollment should succeed");

    let delegated_trading_account =
        super::super::delegation_trading_account_id(&delegation.delegation_id);
    let (delegated_enrollment, delegated_program) = engine
        .best_multiplier_eligible_market_maker_enrollment(7, delegated_trading_account)
        .expect("delegated trading account should resolve to delegated enrollment");
    let expected_tied_program_id =
        std::cmp::min(delegated_high.program_id, delegated_tie.program_id);
    assert_eq!(
        delegated_enrollment.market_maker_identity,
        delegated_identity
    );
    assert_eq!(delegated_program.program_id, expected_tied_program_id);
    assert_eq!(delegated_program.rebate_multiplier_units, 25_000);

    let (naked_enrollment, naked_selected_program) = engine
        .best_multiplier_eligible_market_maker_enrollment(7, 41)
        .expect("naked account should resolve to naked enrollment");
    assert_eq!(
        naked_enrollment.market_maker_identity,
        MarketMakerIdentity::Account(41)
    );
    assert_eq!(naked_selected_program.program_id, naked_program.program_id);

    let delegated_low_key = (delegated_identity, 7, delegated_low.program_id);
    engine
        .market_maker_enrollments
        .get_mut(&delegated_low_key)
        .expect("delegated low enrollment should exist")
        .status = MarketMakerEnrollmentStatus::GracePeriod;
    assert_eq!(
        engine
            .best_multiplier_eligible_market_maker_enrollment(7, delegated_trading_account)
            .expect("delegated tied winner should remain while low is in grace")
            .1
            .program_id,
        expected_tied_program_id
    );

    let delegated_high_key = (delegated_identity, 7, delegated_high.program_id);
    engine
        .market_maker_enrollments
        .get_mut(&delegated_high_key)
        .expect("delegated high enrollment should exist")
        .status = MarketMakerEnrollmentStatus::NonCompliant;
    let delegated_tie_key = (delegated_identity, 7, delegated_tie.program_id);
    engine
        .market_maker_enrollments
        .get_mut(&delegated_tie_key)
        .expect("delegated tie enrollment should exist")
        .status = MarketMakerEnrollmentStatus::NonCompliant;
    assert!(
        engine
            .best_multiplier_eligible_market_maker_enrollment(7, delegated_trading_account)
            .is_none(),
        "only Active enrollments should confer the multiplier"
    );
}

#[test]
fn best_live_market_maker_enrollment_prefers_active_over_higher_multiplier_grace() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, [31; 32], [9; 32], 1_150)
        .expect("serving agent should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_200);
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 3) as u64);

    let mut active_program = sample_market_maker_program_input([7]);
    active_program.name = "active-low".into();
    active_program.rebate_multiplier_units = 15_000;
    let active_program = engine
        .define_market_maker_program(active_program, 1_250)
        .expect("active program should define");

    let mut grace_program = sample_market_maker_program_input([7]);
    grace_program.name = "grace-high".into();
    grace_program.rebate_multiplier_units = 25_000;
    let grace_program = engine
        .define_market_maker_program(grace_program, 1_260)
        .expect("grace program should define");

    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id: active_program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("active enrollment should succeed");
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id: grace_program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_301,
            None,
        )
        .expect("grace enrollment should succeed");
    engine
        .market_maker_enrollments
        .get_mut(&(identity, 7, grace_program.program_id))
        .expect("grace enrollment should exist")
        .status = MarketMakerEnrollmentStatus::GracePeriod;

    let trading_account = super::super::delegation_trading_account_id(&delegation.delegation_id);
    let (selected_enrollment, selected_program) = engine
        .best_live_market_maker_enrollment(7, trading_account)
        .expect("best live enrollment should resolve");
    assert_eq!(selected_enrollment.market_maker_identity, identity);
    assert_eq!(selected_program.program_id, active_program.program_id);

    engine
        .market_maker_enrollments
        .get_mut(&(identity, 7, active_program.program_id))
        .expect("active enrollment should exist")
        .status = MarketMakerEnrollmentStatus::NonCompliant;
    let (_, forfeiting_program) = engine
        .best_live_market_maker_enrollment(7, trading_account)
        .expect("grace/noncompliant enrollments should still resolve for forfeiture");
    assert_eq!(forfeiting_program.program_id, grace_program.program_id);
}

#[test]
fn single_node_engine_lists_perpetual_market() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    let market = engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");

    assert_eq!(market.market_id, 7);
    assert_eq!(market.status, MarketStatus::Active);
    assert_eq!(market.kind, MarketKind::Perpetual);
    assert_eq!(engine.perpetual_market(7), Some(&market));
    assert_eq!(engine.market_oracle_config(7).max_staleness_ms, 5_000);
    assert_eq!(engine.market_risk_parameters(7).maintenance_margin_bps, 500);
    assert_eq!(engine.market_funding_index(7), 0);
    assert_eq!(engine.insurance_fund_balance(7), 0);
    assert!(!engine.market_trading_paused(7));
    assert!(engine.book_levels(7, 1, 1_100).0.is_empty());
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketListed {
            market_id: 7,
            base_symbol,
            quote_asset: 1,
            oracle_feed_id: 77,
            status: MarketLifecycleStatus::Active,
        } if base_symbol == "BTC"
    )));

    engine.set_market_trading_paused(7, true, 1_200);
    assert_eq!(
        engine.perpetual_market(7).map(|market| market.status),
        Some(MarketStatus::Paused)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketStatusUpdated {
            market_id: 7,
            status: MarketLifecycleStatus::Paused,
        }
    )));
    engine.set_market_trading_paused(7, false, 1_300);
    assert_eq!(
        engine.perpetual_market(7).map(|market| market.status),
        Some(MarketStatus::Active)
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketStatusUpdated {
            market_id: 7,
            status: MarketLifecycleStatus::Active,
        }
    )));
}

#[test]
fn single_node_engine_persists_perpetual_market_snapshot() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .set_perpetual_market_status(7, MarketStatus::Paused, 1_200)
        .expect("status update should succeed");
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketStatusUpdated {
            market_id: 7,
            status: MarketLifecycleStatus::Paused,
        }
    )));

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    let market = restored.perpetual_market(7).expect("market should restore");

    assert_eq!(market.status, MarketStatus::Paused);
    assert_eq!(market.updated_at_ms, 1_200);
    assert_eq!(restored.market_oracle_config(7).max_staleness_ms, 5_000);
    assert_eq!(
        restored.market_risk_parameters(7).maintenance_margin_bps,
        500
    );
    assert!(restored.market_trading_paused(7));
}

#[test]
fn single_node_engine_defines_retires_and_reactivates_market_maker_program() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let input = sample_market_maker_program_input([7]);
    let program_id = derive_market_maker_program_id(&input);

    let program = engine
        .define_market_maker_program(input.clone(), 1_200)
        .expect("program should define");

    assert_eq!(program.program_id, program_id);
    assert_eq!(program.status, MarketMakerProgramStatus::Active);
    assert_eq!(engine.market_maker_program(&program_id), Some(&program));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerProgramDefined {
            program_id: event_program_id,
            name,
            markets,
            rebate_multiplier_units: 20_000,
            compliance_window_ms: 3_600_000,
            reactivated: false,
        } if event_program_id == &program_id && name == "perps-gold" && markets == &BTreeSet::from([7])
    )));

    let duplicate = engine
        .define_market_maker_program(input.clone(), 1_250)
        .expect_err("active duplicate should reject");
    assert_eq!(
        duplicate,
        NodeError::InvalidMarketMakerProgram {
            program_id,
            reason: "program_already_active",
        }
    );

    let retired = engine
        .retire_market_maker_program(program_id, 1_300)
        .expect("program should retire");
    assert_eq!(retired.status, MarketMakerProgramStatus::Retired);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerProgramRetired { program_id: event_program_id }
            if event_program_id == program_id
    )));

    let reactivated = engine
        .define_market_maker_program(input, 1_400)
        .expect("retired program spec should reactivate");
    assert_eq!(reactivated.program_id, program_id);
    assert_eq!(reactivated.status, MarketMakerProgramStatus::Active);
    assert_eq!(reactivated.defined_at_ms, 1_200);
    assert_eq!(reactivated.last_updated_at_ms, 1_400);
    assert!(engine.runtime_events().iter().any(|event| matches!(
        &event.kind,
        RuntimeEventKind::MarketMakerProgramDefined {
            program_id: event_program_id,
            reactivated: true,
            ..
        } if event_program_id == &program_id
    )));
}

#[test]
fn single_node_engine_rejects_invalid_market_maker_program() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let mut input = sample_market_maker_program_input([7]);
    input.rebate_multiplier_units = 10_000;
    let program_id = derive_market_maker_program_id(&input);

    let err = engine
        .define_market_maker_program(input, 1_200)
        .expect_err("non-bonus multiplier should reject");

    assert_eq!(
        err,
        NodeError::InvalidMarketMakerProgram {
            program_id,
            reason: "rebate_multiplier_out_of_range",
        }
    );

    let mut input = sample_market_maker_program_input([7]);
    input.name = "bad\nname".into();
    let program_id = derive_market_maker_program_id(&input);
    assert_eq!(
        engine
            .define_market_maker_program(input, 1_200)
            .expect_err("non-printable program name should reject"),
        NodeError::InvalidMarketMakerProgram {
            program_id,
            reason: "invalid_program_name",
        }
    );

    let input = sample_market_maker_program_input(
        (1..=(MAX_MM_PROGRAM_MARKETS as u64 + 1)).collect::<Vec<_>>(),
    );
    let program_id = derive_market_maker_program_id(&input);
    assert_eq!(
        engine
            .define_market_maker_program(input, 1_200)
            .expect_err("oversized market set should reject"),
        NodeError::InvalidMarketMakerProgram {
            program_id,
            reason: "invalid_program_markets",
        }
    );

    let mut input = sample_market_maker_program_input([7]);
    input.compliance_window_ms = COMPLIANCE_WINDOW_MAX_MS + 1;
    let program_id = derive_market_maker_program_id(&input);
    assert_eq!(
        engine
            .define_market_maker_program(input, 1_200)
            .expect_err("oversized compliance window should reject"),
        NodeError::InvalidMarketMakerProgram {
            program_id,
            reason: "compliance_window_out_of_range",
        }
    );

    let mut input = sample_market_maker_program_input([7]);
    input.observation_cadence_ms = OBSERVATION_CADENCE_MAX_MS + 1;
    let program_id = derive_market_maker_program_id(&input);
    assert_eq!(
        engine
            .define_market_maker_program(input, 1_200)
            .expect_err("oversized observation cadence should reject"),
        NodeError::InvalidMarketMakerProgram {
            program_id,
            reason: "observation_cadence_out_of_range",
        }
    );
}

#[test]
fn single_node_engine_persists_market_maker_program_snapshot() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    engine
        .retire_market_maker_program(program.program_id, 1_300)
        .expect("program should retire");

    let restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload");
    let restored_program = restored
        .market_maker_program(&program.program_id)
        .expect("program should restore");

    assert_eq!(restored_program.status, MarketMakerProgramStatus::Retired);
    assert_eq!(restored_program.last_updated_at_ms, 1_300);
    assert_eq!(restored.market_maker_programs().count(), 1);
}

#[test]
fn single_node_engine_enrolls_and_exits_market_maker() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, [31; 32], [9; 32], 1_150)
        .expect("market maker account should create");
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    let key = (MarketMakerIdentity::Account(31), 7, program.program_id);

    let enrollment = engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("market maker should enroll");

    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::Active);
    assert_eq!(engine.market_maker_enrollment(&key), Some(&enrollment));
    assert_eq!(
        engine.live_market_maker_bonds_micro_usdc(),
        PROGRAM_BOND_MIN_MICRO_USDC
    );
    let mut address = [0_u8; 20];
    address[12..].copy_from_slice(&31_u64.to_be_bytes());
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(address)),
        Some(
            U256::from(PROGRAM_BOND_MIN_MICRO_USDC)
                .to_be_bytes::<32>()
                .to_vec()
        )
    );
    assert!(matches!(
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: MarketMakerIdentity::Account(31),
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_350,
                None,
            )
            .expect_err("duplicate live enrollment should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "enrollment_already_live",
            ..
        }
    ));

    let exited = engine
        .exit_market_maker_enrollment(
            MarketMakerIdentity::Account(31),
            7,
            program.program_id,
            1_400,
            None,
        )
        .expect("market maker should exit");

    assert_eq!(exited.status, MarketMakerEnrollmentStatus::Exited);
    assert_eq!(engine.market_maker_enrollment(&key), Some(&exited));
    assert_eq!(engine.live_market_maker_bonds_micro_usdc(), 0);
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(address)),
        Some(
            U256::from(PROGRAM_BOND_MIN_MICRO_USDC * 2)
                .to_be_bytes::<32>()
                .to_vec()
        )
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerEnrolled {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id,
            bond_micro_usdc,
        } if program_id == program.program_id
            && bond_micro_usdc == PROGRAM_BOND_MIN_MICRO_USDC
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerEnrollmentExited {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id,
            bond_refunded_micro_usdc,
        } if program_id == program.program_id
            && bond_refunded_micro_usdc == PROGRAM_BOND_MIN_MICRO_USDC
    )));
}

#[test]
fn single_node_engine_rejects_invalid_market_maker_enrollment() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, [31; 32], [9; 32], 1_150)
        .expect("market maker account should create");
    seed_account_policy_usdc(&mut engine, 31, PROGRAM_BOND_MIN_MICRO_USDC as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");

    let below_floor = engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC - 1),
            },
            1_300,
            None,
        )
        .expect_err("below-floor bond should reject");
    assert!(matches!(
        below_floor,
        NodeError::InvalidMarketMakerProgram {
            reason: "insufficient_enrollment_bond",
            ..
        }
    ));

    seed_account_policy_usdc(&mut engine, 31, PROGRAM_BOND_MIN_MICRO_USDC as u64 - 1);
    let underfunded = engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_350,
            None,
        )
        .expect_err("underfunded bond should reject");
    assert!(matches!(
        underfunded,
        NodeError::InvalidMarketMakerProgram {
            reason: "insufficient_market_maker_policy_usdc_bond",
            ..
        }
    ));

    seed_account_policy_usdc(&mut engine, 31, PROGRAM_BOND_MIN_MICRO_USDC as u64);
    engine
        .retire_market_maker_program(program.program_id, 1_400)
        .expect("program should retire");
    let retired = engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_450,
            None,
        )
        .expect_err("retired program should reject enrollment");
    assert!(matches!(
        retired,
        NodeError::InvalidMarketMakerProgram {
            reason: "program_not_active",
            ..
        }
    ));
}

#[test]
fn single_node_engine_enrolls_and_exits_delegation_market_maker_with_serving_agent_bond() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(
            31,
            signing_key(31).verifying_key().to_bytes(),
            [9; 32],
            1_150,
        )
        .expect("serving agent account should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_200);
    engine
        .close_delegation_to_deposits(delegation.delegation_id, 1_225)
        .expect("delegation should close to deposits");
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    let key = (identity, 7, program.program_id);

    let enrollment = engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_250,
            None,
        )
        .expect("delegation identity should enroll");

    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::Active);
    assert_eq!(engine.market_maker_enrollment(&key), Some(&enrollment));
    assert_eq!(
        engine.live_market_maker_bonds_micro_usdc(),
        PROGRAM_BOND_MIN_MICRO_USDC
    );
    let address = engine
        .get_committed_state(&account_evm_address_state_key(31))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("serving agent address should exist");
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(address)),
        Some(
            U256::from(PROGRAM_BOND_MIN_MICRO_USDC)
                .to_be_bytes::<32>()
                .to_vec()
        )
    );

    let exited = engine
        .exit_market_maker_enrollment(identity, 7, program.program_id, 1_300, None)
        .expect("delegation market maker should exit");

    assert_eq!(exited.status, MarketMakerEnrollmentStatus::Exited);
    assert_eq!(engine.market_maker_enrollment(&key), Some(&exited));
    assert_eq!(engine.live_market_maker_bonds_micro_usdc(), 0);
    assert_eq!(
        engine.get_committed_state(&policy_usdc_balance_state_key(address)),
        Some(
            U256::from(PROGRAM_BOND_MIN_MICRO_USDC * 2)
                .to_be_bytes::<32>()
                .to_vec()
        )
    );
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerEnrolled {
            market_maker_identity,
            market_id: 7,
            program_id,
            bond_micro_usdc,
        } if market_maker_identity == identity
            && program_id == program.program_id
            && bond_micro_usdc == PROGRAM_BOND_MIN_MICRO_USDC
    )));
    assert!(engine.runtime_events().iter().any(|event| matches!(
        event.kind,
        RuntimeEventKind::MarketMakerEnrollmentExited {
            market_maker_identity,
            market_id: 7,
            program_id,
            bond_refunded_micro_usdc,
        } if market_maker_identity == identity
            && program_id == program.program_id
            && bond_refunded_micro_usdc == PROGRAM_BOND_MIN_MICRO_USDC
    )));
}

#[test]
fn single_node_engine_rejects_invalid_delegation_market_maker_enrollment() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(
            31,
            signing_key(31).verifying_key().to_bytes(),
            [9; 32],
            1_150,
        )
        .expect("serving agent account should create");
    engine
        .create_account(
            32,
            signing_key(32).verifying_key().to_bytes(),
            [8; 32],
            1_151,
        )
        .expect("second account should create");
    let delegation = sample_open_delegation(&mut engine, 31, 1_200);
    seed_account_policy_usdc(&mut engine, 31, (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64);
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_250)
        .expect("program should define");

    assert!(matches!(
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: MarketMakerIdentity::Delegation {
                        delegation_id: [7; 32],
                        serving_agent: 31,
                    },
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_260,
                None,
            )
            .expect_err("unknown delegation should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "unknown_market_maker_delegation",
            ..
        }
    ));
    assert!(matches!(
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: MarketMakerIdentity::Delegation {
                        delegation_id: delegation.delegation_id,
                        serving_agent: 32,
                    },
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_261,
                None,
            )
            .expect_err("serving agent mismatch should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "delegation_market_maker_serving_agent_mismatch",
            ..
        }
    ));

    engine
        .begin_delegation_winding(delegation.delegation_id, 1_262)
        .expect("delegation should enter winding");
    assert!(matches!(
        engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: MarketMakerIdentity::Delegation {
                        delegation_id: delegation.delegation_id,
                        serving_agent: 31,
                    },
                    market_id: 7,
                    program_id: program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_263,
                None,
            )
            .expect_err("winding delegation should reject enrollment"),
        NodeError::InvalidMarketMakerProgram {
            reason: "delegation_market_maker_not_enrollable",
            ..
        }
    ));

    let mut active_engine = SingleNodeEngine::new(single_node_config());
    active_engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    active_engine
        .create_account(
            31,
            signing_key(31).verifying_key().to_bytes(),
            [9; 32],
            1_150,
        )
        .expect("serving agent account should create");
    let active_delegation = sample_open_delegation(&mut active_engine, 31, 1_200);
    seed_account_policy_usdc(
        &mut active_engine,
        31,
        (PROGRAM_BOND_MIN_MICRO_USDC * 2) as u64,
    );
    let active_program = active_engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_250)
        .expect("program should define");
    active_engine
        .suspend_mandate(active_delegation.mandate_id, 1_255)
        .expect("mandate should suspend");
    assert!(matches!(
        active_engine
            .enroll_market_maker(
                EnrollMarketMakerInput {
                    market_maker_identity: MarketMakerIdentity::Delegation {
                        delegation_id: active_delegation.delegation_id,
                        serving_agent: 31,
                    },
                    market_id: 7,
                    program_id: active_program.program_id,
                    bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                },
                1_256,
                None,
            )
            .expect_err("inactive mandate should reject enrollment"),
        NodeError::InvalidMarketMakerProgram {
            reason: "delegation_market_maker_mandate_not_active",
            ..
        }
    ));
}

#[test]
fn close_delegation_auto_exits_live_delegation_market_maker_enrollment() {
    let (mut engine, _signer, delegation, program_id) =
        setup_signed_delegation_market_maker_enrollment_fixture();
    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("delegation-backed market maker should enroll");
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(31))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("serving agent address should exist");
    let pre_close_balance = policy_usdc_balance_for_test(&engine, agent_address);

    let closed = engine
        .close_delegation(delegation.delegation_id, 1_400)
        .expect("close should auto-exit delegation-backed enrollment");

    assert_eq!(closed.status, DelegationStatus::Closed);
    assert_eq!(
        engine
            .market_maker_enrollment(&(identity, 7, program_id))
            .expect("enrollment should persist for audit")
            .status,
        MarketMakerEnrollmentStatus::Exited
    );
    assert_eq!(engine.live_market_maker_bonds_micro_usdc(), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, agent_address),
        pre_close_balance + PROGRAM_BOND_MIN_MICRO_USDC + AGENT_BOND_MIN
    );
}

#[test]
fn terminate_delegation_auto_exits_live_delegation_market_maker_enrollment() {
    let (mut engine, _signer, delegation, program_id) =
        setup_signed_delegation_market_maker_enrollment_fixture();
    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("delegation-backed market maker should enroll");
    let agent_address = engine
        .get_committed_state(&account_evm_address_state_key(31))
        .and_then(|bytes| bytes.as_slice().try_into().ok())
        .expect("serving agent address should exist");
    let pre_terminate_balance = policy_usdc_balance_for_test(&engine, agent_address);

    let termination = engine
        .terminate_delegation(
            delegation.delegation_id,
            BlockDelegationFaultReason::MandateBreached,
            None,
            1_400,
        )
        .expect("terminate should auto-exit delegation-backed enrollment");

    assert_eq!(termination.delegation.status, DelegationStatus::Terminated);
    assert_eq!(
        engine
            .market_maker_enrollment(&(identity, 7, program_id))
            .expect("enrollment should persist for audit")
            .status,
        MarketMakerEnrollmentStatus::Exited
    );
    assert_eq!(engine.live_market_maker_bonds_micro_usdc(), 0);
    assert_eq!(
        policy_usdc_balance_for_test(&engine, agent_address),
        pre_terminate_balance + PROGRAM_BOND_MIN_MICRO_USDC
    );
}

#[test]
fn snapshot_rejects_live_terminal_delegation_market_maker_enrollment() {
    let (mut engine, _signer, delegation, program_id) =
        setup_signed_delegation_market_maker_enrollment_fixture();
    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: identity,
                market_id: 7,
                program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("delegation-backed market maker should enroll");

    let mut snapshot = engine.persisted_snapshot();
    snapshot
        .delegations
        .get_mut(&delegation.delegation_id)
        .expect("delegation should exist in snapshot")
        .status = DelegationStatus::Closed;

    let err = SingleNodeEngine::try_from_persisted_snapshot(snapshot)
        .expect_err("live delegated MM enrollment on closed delegation should reject");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn single_node_engine_allows_market_maker_enrollment_on_paused_market() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .set_perpetual_market_status(7, MarketStatus::Paused, 1_150)
        .expect("market pause should succeed");
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("paused market program should define");
    engine
        .create_account(31, [31; 32], [9; 32], 1_250)
        .expect("market maker account should create");
    seed_account_policy_usdc(&mut engine, 31, PROGRAM_BOND_MIN_MICRO_USDC as u64);

    let enrollment = engine
        .enroll_market_maker(
            EnrollMarketMakerInput {
                market_maker_identity: MarketMakerIdentity::Account(31),
                market_id: 7,
                program_id: program.program_id,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            },
            1_300,
            None,
        )
        .expect("paused market should allow enrollment");

    assert_eq!(enrollment.status, MarketMakerEnrollmentStatus::Active);
}

#[test]
fn pending_market_maker_program_actions_are_drained_into_produced_block() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let action = sample_market_maker_program_define_action([7]);
    let action_id = market_maker_program_action_id_for_submission(&action, 1_200, Some(7));

    assert_eq!(
        engine
            .submit_market_maker_program_action(action_id, action.clone(), 1_200)
            .expect("market-maker program action should queue"),
        MarketMakerProgramActionSubmissionStatus::Accepted
    );
    assert_eq!(
        engine
            .submit_market_maker_program_action(action_id, action.clone(), 1_200)
            .expect("duplicate pending action should be idempotent"),
        MarketMakerProgramActionSubmissionStatus::DuplicatePending
    );
    assert_eq!(engine.pending_market_maker_program_action_count(), 1);

    engine
        .produce_block_from_mempool(1_300, 10)
        .expect("pending market-maker program action should drain");

    let program_id = derive_market_maker_program_id(&sample_market_maker_program_input([7]));
    assert_eq!(engine.pending_market_maker_program_action_count(), 0);
    assert!(engine.market_maker_program(&program_id).is_some());
    assert_eq!(
        engine.processed_market_maker_program_action(action_id),
        Some(&ProcessedMarketMakerProgramAction {
            processed_at_ms: 1_300,
            fingerprint: super::super::market_maker_program_action_fingerprint(&action),
            receipt: MarketMakerProgramActionReceipt::Define { program_id },
        })
    );
    assert_eq!(
        engine
            .submit_market_maker_program_action(action_id, action, 1_200)
            .expect("processed duplicate should be idempotent"),
        MarketMakerProgramActionSubmissionStatus::AlreadyProcessed
    );
}

#[test]
fn signed_market_maker_program_action_queues_with_account_owner_signature() {
    let wrong_signer = signing_key(32);
    let (mut engine, signer, program_id) = setup_signed_market_maker_enrollment_fixture();
    let action = market_maker_enroll_action(program_id);
    let signed = signed_market_maker_program_action(
        &signer,
        engine.chain_id(),
        31,
        action.clone(),
        1_300,
        Some(44),
    );

    let mut wrong_scope = signed.clone();
    wrong_scope.signer_account_id = 32;
    wrong_scope.signature = wrong_signer
        .sign(&market_maker_program_action_signing_bytes(&wrong_scope))
        .to_bytes();
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(wrong_scope)
            .expect_err("wrong signer scope should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "market_maker_program_action_signer_mismatch",
            ..
        }
    ));

    assert_eq!(
        engine
            .submit_signed_market_maker_program_action(signed.clone())
            .expect("signed market-maker program action should queue"),
        MarketMakerProgramActionSubmissionStatus::Accepted
    );
    assert_eq!(
        engine
            .submit_signed_market_maker_program_action(signed.clone())
            .expect("signed duplicate should stay pending"),
        MarketMakerProgramActionSubmissionStatus::DuplicatePending
    );

    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("signed market-maker program action should drain");

    assert_eq!(engine.pending_market_maker_program_action_count(), 0);
    assert_eq!(
        engine
            .market_maker_enrollment(&(MarketMakerIdentity::Account(31), 7, program_id))
            .expect("enrollment should exist")
            .status,
        MarketMakerEnrollmentStatus::Active
    );
    assert_eq!(
        engine
            .processed_market_maker_program_action(signed.action_id)
            .expect("receipt should be recorded")
            .receipt,
        MarketMakerProgramActionReceipt::Enroll {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 7,
            program_id,
        }
    );
}

#[test]
fn signed_market_maker_program_action_queues_with_delegation_identity_owner_signature() {
    let (mut engine, signer, delegation, program_id) =
        setup_signed_delegation_market_maker_enrollment_fixture();
    let identity = MarketMakerIdentity::Delegation {
        delegation_id: delegation.delegation_id,
        serving_agent: 31,
    };
    let action = delegation_market_maker_enroll_action(delegation.delegation_id, 31, program_id);
    let signed =
        signed_market_maker_program_action(&signer, engine.chain_id(), 31, action, 1_300, Some(44));

    assert_eq!(
        engine
            .submit_signed_market_maker_program_action(signed.clone())
            .expect("delegation-backed market-maker action should queue"),
        MarketMakerProgramActionSubmissionStatus::Accepted
    );

    engine
        .produce_block_from_mempool(1_400, 10)
        .expect("signed delegation-backed market-maker action should drain");

    assert_eq!(engine.pending_market_maker_program_action_count(), 0);
    assert_eq!(
        engine
            .market_maker_enrollment(&(identity, 7, program_id))
            .expect("delegation-backed enrollment should exist")
            .status,
        MarketMakerEnrollmentStatus::Active
    );
    assert_eq!(
        engine
            .processed_market_maker_program_action(signed.action_id)
            .expect("receipt should be recorded")
            .receipt,
        MarketMakerProgramActionReceipt::Enroll {
            market_maker_identity: identity,
            market_id: 7,
            program_id,
        }
    );
}

#[test]
fn signed_market_maker_program_action_rejects_tampering_and_governance_actions() {
    let (mut engine, signer, program_id) = setup_signed_market_maker_enrollment_fixture();
    let action = market_maker_enroll_action(program_id);
    let signed = signed_market_maker_program_action(
        &signer,
        engine.chain_id(),
        31,
        action.clone(),
        1_300,
        Some(44),
    );

    let mut tampered_action = signed.clone();
    tampered_action.action = BlockMarketMakerProgramAction::Exit {
        market_maker_identity: MarketMakerIdentity::Account(31),
        market_id: 7,
        program_id,
    };
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(tampered_action)
            .expect_err("same id with different action body should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "market_maker_program_action_id_mismatch",
            ..
        }
    ));

    let mut tampered_signature = signed.clone();
    tampered_signature.signature[0] ^= 0x01;
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(tampered_signature)
            .expect_err("tampered signature should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "invalid_market_maker_program_action_signature",
            ..
        }
    ));

    let wrong_chain = signed_market_maker_program_action(
        &signer,
        engine.chain_id() + 1,
        31,
        action,
        1_301,
        Some(45),
    );
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(wrong_chain)
            .expect_err("chain mismatch should reject"),
        NodeError::InvalidMarketMakerProgram {
            reason: "chain_mismatch",
            ..
        }
    ));

    let define = signed_market_maker_program_action(
        &signer,
        engine.chain_id(),
        31,
        sample_market_maker_program_define_action([7]),
        1_302,
        Some(46),
    );
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(define)
            .expect_err("governance define signer should not be configured yet"),
        NodeError::InvalidMarketMakerProgram {
            reason: "market_maker_governance_signer_not_configured",
            ..
        }
    ));

    let retire = signed_market_maker_program_action(
        &signer,
        engine.chain_id(),
        31,
        BlockMarketMakerProgramAction::Retire { program_id },
        1_303,
        Some(47),
    );
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(retire)
            .expect_err("governance retire signer should not be configured yet"),
        NodeError::InvalidMarketMakerProgram {
            reason: "market_maker_governance_signer_not_configured",
            ..
        }
    ));
}

#[test]
fn signed_market_maker_program_action_enforces_per_signer_pending_cap() {
    let (mut engine, signer, program_id) = setup_signed_market_maker_enrollment_fixture();
    let action = market_maker_enroll_action(program_id);
    for nonce in 0..MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS_PER_SIGNER {
        let signed = signed_market_maker_program_action(
            &signer,
            engine.chain_id(),
            31,
            action.clone(),
            1_300 + nonce as u64,
            Some(nonce as u64),
        );
        assert_eq!(
            engine
                .submit_signed_market_maker_program_action(signed)
                .expect("action should queue before per-signer cap"),
            MarketMakerProgramActionSubmissionStatus::Accepted
        );
    }

    let duplicate = signed_market_maker_program_action(
        &signer,
        engine.chain_id(),
        31,
        action.clone(),
        1_300,
        Some(0),
    );
    assert_eq!(
        engine
            .submit_signed_market_maker_program_action(duplicate)
            .expect("duplicate should bypass per-signer cap"),
        MarketMakerProgramActionSubmissionStatus::DuplicatePending
    );

    let overflow = signed_market_maker_program_action(
        &signer,
        engine.chain_id(),
        31,
        action,
        1_300 + MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS_PER_SIGNER as u64,
        Some(MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS_PER_SIGNER as u64),
    );
    assert!(matches!(
        engine
            .submit_signed_market_maker_program_action(overflow)
            .expect_err("per-signer cap should reject new action"),
        NodeError::InvalidMarketMakerProgram {
            reason: "max_pending_market_maker_program_actions_per_signer",
            ..
        }
    ));
}

#[test]
fn pending_market_maker_program_action_queue_rejects_when_full() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let action = sample_market_maker_program_define_action([7]);

    for index in 0..MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS {
        assert_eq!(
            engine
                .submit_market_maker_program_action(
                    indexed_action_id(index),
                    action.clone(),
                    1_200 + index as u64,
                )
                .expect("action should queue before global cap"),
            MarketMakerProgramActionSubmissionStatus::Accepted
        );
    }

    assert!(matches!(
        engine
            .submit_market_maker_program_action(
                indexed_action_id(MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS),
                action,
                1_200 + MAX_PENDING_MARKET_MAKER_PROGRAM_ACTIONS as u64,
            )
            .expect_err("global queue cap should reject overflow"),
        NodeError::InvalidMarketMakerProgram {
            reason: "market_maker_program_action_queue_full",
            ..
        }
    ));
}

#[test]
fn pending_market_maker_program_actions_survive_snapshot_roundtrip() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let action = sample_market_maker_program_define_action([7]);
    let action_id = market_maker_program_action_id_for_submission(&action, 1_200, Some(77));
    engine
        .submit_market_maker_program_action(action_id, action.clone(), 1_200)
        .expect("market-maker program action should queue");

    let mut restored = SingleNodeEngine::from_snapshot_bytes(&engine.snapshot_bytes())
        .expect("snapshot should reload pending market-maker action");

    assert_eq!(restored.pending_market_maker_program_action_count(), 1);
    let pending = restored
        .pending_market_maker_program_action(action_id)
        .expect("pending action should restore");
    assert_eq!(pending.action_id, action_id);
    assert_eq!(pending.submitted_at_ms, 1_200);
    assert_eq!(
        pending.fingerprint,
        super::super::market_maker_program_action_fingerprint(&action)
    );

    restored
        .produce_block_from_mempool(1_300, 10)
        .expect("restored pending action should drain");
    let program_id = derive_market_maker_program_id(&sample_market_maker_program_input([7]));
    assert!(restored.market_maker_program(&program_id).is_some());
    assert!(matches!(
        restored.processed_market_maker_program_action(action_id),
        Some(ProcessedMarketMakerProgramAction {
            receipt: MarketMakerProgramActionReceipt::Define {
                program_id: processed_program_id,
            },
            ..
        }) if *processed_program_id == program_id
    ));
}

#[test]
fn snapshot_bytes_reject_market_maker_enrollment_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    engine
        .create_account(31, [31; 32], [9; 32], 1_150)
        .expect("market maker account should create");
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    let mut snapshot = engine.persisted_snapshot();
    let key = (MarketMakerIdentity::Account(31), 7, program.program_id);
    snapshot.market_maker_enrollments.insert(
        key,
        MarketMakerEnrollment {
            market_maker_identity: MarketMakerIdentity::Account(31),
            market_id: 8,
            program_id: program.program_id,
            enrolled_at_ms: 1_300,
            last_updated_at_ms: 1_300,
            status: MarketMakerEnrollmentStatus::Active,
            bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
            current_compliance: MarketMakerComplianceState::default(),
            observation_history: Vec::new(),
            last_compliance_checked_ms: 0,
            lifetime_compliant_rebates_micro_usdc: MarketMakerU256::default(),
            lifetime_forfeited_rebates_micro_usdc: MarketMakerU256::default(),
        },
    );

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject tampered enrollment");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("market-maker enrollment mismatch"),
        "{err}"
    );
}

#[test]
fn snapshot_bytes_reject_market_maker_enrollment_cap_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    for market_maker in 1..=(MAX_ENROLLMENTS_PER_MARKET as u64 + 1) {
        engine
            .create_account(market_maker, [market_maker as u8; 32], [9; 32], 1_250)
            .expect("market maker account should create");
    }
    let mut snapshot = engine.persisted_snapshot();
    for market_maker in 1..=(MAX_ENROLLMENTS_PER_MARKET as u64 + 1) {
        snapshot.market_maker_enrollments.insert(
            (
                MarketMakerIdentity::Account(market_maker),
                7,
                program.program_id,
            ),
            MarketMakerEnrollment {
                market_maker_identity: MarketMakerIdentity::Account(market_maker),
                market_id: 7,
                program_id: program.program_id,
                enrolled_at_ms: 1_300 + market_maker,
                last_updated_at_ms: 1_300 + market_maker,
                status: MarketMakerEnrollmentStatus::Active,
                bond_micro_usdc: MarketMakerU256::from_u128(PROGRAM_BOND_MIN_MICRO_USDC),
                current_compliance: MarketMakerComplianceState::default(),
                observation_history: Vec::new(),
                last_compliance_checked_ms: 0,
                lifetime_compliant_rebates_micro_usdc: MarketMakerU256::default(),
                lifetime_forfeited_rebates_micro_usdc: MarketMakerU256::default(),
            },
        );
    }
    snapshot.live_market_maker_bonds_micro_usdc =
        PROGRAM_BOND_MIN_MICRO_USDC * (MAX_ENROLLMENTS_PER_MARKET as u128 + 1);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject enrollment cap mismatch");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        err.to_string()
            .contains("market-maker enrollment cap mismatch"),
        "{err}"
    );
}

#[test]
fn snapshot_bytes_reject_market_maker_program_id_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let mut program = engine
        .define_market_maker_program(sample_market_maker_program_input([7]), 1_200)
        .expect("program should define");
    let program_id = program.program_id;
    program.name = "tampered".into();
    let mut snapshot = engine.persisted_snapshot();
    snapshot.market_maker_programs.insert(program_id, program);

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject tampered program id material");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("market-maker program mismatch"));
}

#[test]
fn snapshot_bytes_reject_market_maker_program_cap_mismatch() {
    let mut engine = SingleNodeEngine::new(single_node_config());
    engine
        .list_perpetual_market(7, "BTC".into(), 1, 77, btc_perp_config(), 1_100)
        .expect("market listing should succeed");
    let mut snapshot = engine.persisted_snapshot();
    for index in 0..=MAX_MM_PROGRAMS {
        let mut input = sample_market_maker_program_input([7]);
        input.name = format!("mm-program-{index}");
        let program_id = derive_market_maker_program_id(&input);
        snapshot.market_maker_programs.insert(
            program_id,
            MarketMakerProgram {
                program_id,
                name: input.name,
                markets: input.markets,
                defined_at_ms: 1_200 + index as u64,
                last_updated_at_ms: 1_200 + index as u64,
                status: MarketMakerProgramStatus::Active,
                min_uptime_bps: input.min_uptime_bps,
                max_spread_bps: input.max_spread_bps,
                min_quote_size_micro_usdc: input.min_quote_size_micro_usdc,
                min_two_sided_windows_per_hour: input.min_two_sided_windows_per_hour,
                rebate_multiplier_units: input.rebate_multiplier_units,
                enrollment_bond_micro_usdc: input.enrollment_bond_micro_usdc,
                compliance_window_ms: input.compliance_window_ms,
                observation_cadence_ms: input.observation_cadence_ms,
            },
        );
    }

    let err = SingleNodeEngine::from_snapshot_bytes(
        &borsh::to_vec(&snapshot).expect("snapshot should serialize"),
    )
    .expect_err("snapshot should reject active program cap overflow");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("market-maker program cap mismatch"));
}
