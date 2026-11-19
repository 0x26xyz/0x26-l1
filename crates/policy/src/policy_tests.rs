use ed25519_dalek::{Signer, SigningKey};

use super::{
    component_name, AccountError, AccountStore, OwnerAction, OwnerCall, PolicyAdmission,
    PolicyError, PolicyRecord, PolicyState, PolicyStore, PolicyTerms, SessionPayload,
    SignedOwnerCall, SignedSessionPayload, StandardAgentAccount,
};

fn principal(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn hash(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn signing_key(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&[byte; 32])
}

fn session_intent_hash(message: &[u8]) -> [u8; 32] {
    let mut intent_hash = [0_u8; 32];
    for (index, byte) in message.iter().copied().enumerate() {
        intent_hash[index % 32] ^= byte;
    }
    intent_hash
}

fn signed_session(
    signer: &SigningKey,
    policy_id: u64,
    chain_id: u64,
    message: &[u8],
) -> SignedSessionPayload {
    let payload = SessionPayload {
        intent_hash: session_intent_hash(message),
        policy_id,
        chain_id,
    };
    let signature = signer.sign(&borsh::to_vec(&payload).expect("payload should serialize"));

    SignedSessionPayload {
        payload,
        signature: signature.to_bytes(),
    }
}

fn signed_owner_call(
    signer: &SigningKey,
    account_id: u64,
    chain_id: u64,
    nonce: u64,
    action: OwnerAction,
) -> SignedOwnerCall {
    let call = OwnerCall {
        signer: signer.verifying_key().to_bytes(),
        nonce,
        action,
    };
    let signing_bytes =
        borsh::to_vec(&(account_id, chain_id, &call)).expect("owner call payload should serialize");
    let signature = signer.sign(&signing_bytes).to_bytes();

    SignedOwnerCall {
        account_id,
        chain_id,
        call,
        signature,
    }
}

fn sample_terms() -> PolicyTerms {
    PolicyTerms {
        gas_budget_per_day: 100,
        gas_refill_per_day: 100,
        max_intents_per_second: 2,
        max_intents_per_day: 3,
        max_volume_per_day: 500,
        max_drawdown_pct: 25,
        ..PolicyTerms::default()
    }
}

fn sample_policy(state: PolicyState) -> PolicyRecord {
    let signer = signing_key(8);
    PolicyRecord::new(
        7,
        11,
        signer.verifying_key().to_bytes(),
        1_000,
        172_800_000,
        state,
        sample_terms(),
    )
}

fn sample_admission(
    signer: &SigningKey,
    intent_nonce: u64,
    timestamp_ms: u64,
    block_number: u64,
) -> PolicyAdmission {
    PolicyAdmission {
        signed_session: signed_session(signer, 7, 7, b"intent:place-order"),
        intent_nonce,
        block_number,
        expected_chain_id: 7,
        timestamp_ms,
        gas_cost: 30,
        volume: 40,
        notional_outstanding: 80,
        intent_notional: 40,
        market_id: Some(7),
        realized_pnl_delta: 5,
        current_drawdown_pct: 10,
    }
}

#[test]
fn reports_component_name() {
    assert_eq!(component_name(), "policy");
}

#[test]
fn creates_account_with_no_controllers() {
    let account = StandardAgentAccount::new(7, principal(1), 1_000, hash(9));

    assert_eq!(account.id(), 7);
    assert_eq!(account.owner(), principal(1));
    assert_eq!(account.pending_owner(), None);
    assert_eq!(account.created_at(), 1_000);
    assert_eq!(account.nonce(), 0);
    assert_eq!(account.controllers(), &[]);
    assert!(!account.paused());
}

#[test]
fn owner_can_add_pause_unpause_and_revoke_policy() {
    let mut account = StandardAgentAccount::new(7, principal(1), 1_000, hash(9));

    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 0,
                action: OwnerAction::AddPolicy { policy_id: 42 },
            },
            &[],
        )
        .expect("owner should add policy");
    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 1,
                action: OwnerAction::Pause,
            },
            &[],
        )
        .expect("owner should pause account");
    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 2,
                action: OwnerAction::Unpause,
            },
            &[],
        )
        .expect("owner should unpause account");
    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 3,
                action: OwnerAction::RevokePolicy { policy_id: 42 },
            },
            &[],
        )
        .expect("owner should revoke policy");

    assert_eq!(account.nonce(), 4);
    assert!(!account.paused());
    assert_eq!(account.controllers(), &[]);
}

#[test]
fn controller_list_is_sorted_and_rejects_duplicates() {
    let mut account = StandardAgentAccount::new(7, principal(1), 1_000, hash(9));

    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 0,
                action: OwnerAction::AddPolicy { policy_id: 9 },
            },
            &[],
        )
        .expect("first policy should be added");
    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 1,
                action: OwnerAction::AddPolicy { policy_id: 4 },
            },
            &[],
        )
        .expect("second policy should be inserted canonically");

    let err = account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 2,
                action: OwnerAction::AddPolicy { policy_id: 9 },
            },
            &[],
        )
        .expect_err("duplicate controller should fail");

    assert_eq!(account.controllers(), &[4, 9]);
    assert_eq!(err, AccountError::DuplicatePolicy(9));
    assert_eq!(account.nonce(), 2);
}

#[test]
fn ownership_transfer_is_two_step() {
    let mut account = StandardAgentAccount::new(7, principal(1), 1_000, hash(9));

    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 0,
                action: OwnerAction::ProposeOwner {
                    new_owner: principal(2),
                },
            },
            &[],
        )
        .expect("owner proposal should succeed");

    let err = account
        .apply_owner_call(
            OwnerCall {
                signer: principal(3),
                nonce: 1,
                action: OwnerAction::AcceptOwner,
            },
            &[],
        )
        .expect_err("wrong pending owner should fail");
    assert_eq!(err, AccountError::PendingOwnerMismatch);

    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(2),
                nonce: 1,
                action: OwnerAction::AcceptOwner,
            },
            &[],
        )
        .expect("pending owner should accept");

    assert_eq!(account.owner(), principal(2));
    assert_eq!(account.pending_owner(), None);
    assert_eq!(account.nonce(), 2);
}

#[test]
fn rejects_unauthorized_owner_mutation() {
    let mut account = StandardAgentAccount::new(7, principal(1), 1_000, hash(9));

    let err = account
        .apply_owner_call(
            OwnerCall {
                signer: principal(9),
                nonce: 0,
                action: OwnerAction::Pause,
            },
            &[],
        )
        .expect_err("non-owner should not mutate account");

    assert_eq!(err, AccountError::Unauthorized);
    assert_eq!(account.nonce(), 0);
}

#[test]
fn upgrade_requires_protocol_approval() {
    let mut account = StandardAgentAccount::new(7, principal(1), 1_000, hash(1));

    let err = account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 0,
                action: OwnerAction::Upgrade { code_hash: hash(2) },
            },
            &[hash(3)],
        )
        .expect_err("unapproved upgrade should fail");
    assert_eq!(err, AccountError::UpgradeNotApproved(hash(2)));
    assert_eq!(account.code_hash(), hash(1));
    assert_eq!(account.nonce(), 0);

    account
        .apply_owner_call(
            OwnerCall {
                signer: principal(1),
                nonce: 0,
                action: OwnerAction::Upgrade { code_hash: hash(3) },
            },
            &[hash(3)],
        )
        .expect("approved upgrade should succeed");

    assert_eq!(account.code_hash(), hash(3));
    assert_eq!(account.nonce(), 1);
}

#[test]
fn signed_owner_call_can_create_policy() {
    let mut accounts = AccountStore::default();
    let mut policies = PolicyStore::default();
    let owner = signing_key(8);
    accounts
        .insert(StandardAgentAccount::new(
            11,
            owner.verifying_key().to_bytes(),
            1_000,
            hash(9),
        ))
        .expect("account should insert");

    accounts
        .apply_signed_owner_call(
            &signed_owner_call(
                &owner,
                11,
                7,
                0,
                OwnerAction::CreatePolicy {
                    policy_id: 42,
                    grantee: signing_key(9).verifying_key().to_bytes(),
                    expires_at: 50_000,
                    terms: sample_terms(),
                },
            ),
            7,
            1,
            1_100,
            &[hash(9)],
            &mut policies,
        )
        .expect("signed owner call should create policy");

    assert!(policies.get(42).is_some());
    assert_eq!(
        accounts
            .get(11)
            .expect("account should exist")
            .controllers(),
        &[42]
    );
}

#[test]
fn signed_owner_call_rejects_bad_signature() {
    let mut accounts = AccountStore::default();
    let mut policies = PolicyStore::default();
    let owner = signing_key(8);
    accounts
        .insert(StandardAgentAccount::new(
            11,
            owner.verifying_key().to_bytes(),
            1_000,
            hash(9),
        ))
        .expect("account should insert");

    let mut signed_call = signed_owner_call(
        &owner,
        11,
        7,
        0,
        OwnerAction::CreatePolicy {
            policy_id: 42,
            grantee: signing_key(9).verifying_key().to_bytes(),
            expires_at: 50_000,
            terms: sample_terms(),
        },
    );
    signed_call.signature = [0; 64];

    let err = accounts
        .apply_signed_owner_call(&signed_call, 7, 1, 1_100, &[hash(9)], &mut policies)
        .expect_err("bad signature should fail");

    assert_eq!(err, AccountError::InvalidOwnerSignature);
}

#[test]
fn policy_can_be_created_admitted_and_revoked() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    store
        .admit(7, &sample_admission(&signer, 1, 1_500, 1))
        .expect("admission should succeed");
    store.revoke(7, 2, 2_000).expect("revoke should succeed");

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:cancel"),
                intent_nonce: 2,
                block_number: 2,
                expected_chain_id: 7,
                timestamp_ms: 2_100,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: 1,
                current_drawdown_pct: 5,
            },
        )
        .expect_err("revoked policy should reject later admissions");

    assert_eq!(err, PolicyError::PolicyNotActive(PolicyState::Revoked));
}

#[test]
fn sub_policy_creation_reserves_parent_capacity() {
    let mut store = PolicyStore::default();
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("parent insertion should succeed");

    let mut child_terms = sample_terms();
    child_terms.gas_budget_per_day = 40;
    child_terms.max_volume_per_day = 120;
    child_terms.max_intents_per_day = 2;
    child_terms.max_intents_per_second = 1;
    child_terms.max_drawdown_pct = 20;

    let child = PolicyRecord::new(
        8,
        11,
        signing_key(9).verifying_key().to_bytes(),
        1_100,
        100_000,
        PolicyState::Active,
        child_terms,
    );

    store
        .create_sub_policy(7, child)
        .expect("sub-policy should fit within parent bounds");

    let parent = store.get(7).expect("parent policy should exist");
    let child = store.get(8).expect("child policy should exist");

    assert_eq!(parent.reserved.gas_budget_per_day, 40);
    assert_eq!(parent.reserved.volume_per_day, 120);
    assert_eq!(child.parent_policy_id, Some(7));
    assert_eq!(child.delegation_depth, 1);
}

#[test]
fn revoking_parent_cascades_to_child_and_releases_reservation() {
    let mut store = PolicyStore::default();
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("parent insertion should succeed");

    let mut child_terms = sample_terms();
    child_terms.gas_budget_per_day = 40;
    child_terms.max_volume_per_day = 120;
    child_terms.max_intents_per_day = 2;
    child_terms.max_intents_per_second = 1;
    child_terms.max_drawdown_pct = 20;

    let child = PolicyRecord::new(
        8,
        11,
        signing_key(9).verifying_key().to_bytes(),
        1_100,
        100_000,
        PolicyState::Active,
        child_terms,
    );

    store
        .create_sub_policy(7, child)
        .expect("sub-policy should fit within parent bounds");
    store.revoke(7, 2, 2_000).expect("revoke should cascade");

    let parent = store.get(7).expect("parent policy should exist");
    let child = store.get(8).expect("child policy should exist");

    assert_eq!(parent.state, PolicyState::Revoked);
    assert_eq!(child.state, PolicyState::Revoked);
    assert_eq!(parent.reserved.gas_budget_per_day, 0);
    assert_eq!(parent.reserved.volume_per_day, 0);
    assert_eq!(child.revoked_at, Some(2_000));
}

#[test]
fn policy_counters_replay_deterministically() {
    let mut left = PolicyStore::default();
    let mut right = PolicyStore::default();
    let signer = signing_key(8);
    left.insert(sample_policy(PolicyState::Active))
        .expect("left insertion should succeed");
    right
        .insert(sample_policy(PolicyState::Active))
        .expect("right insertion should succeed");

    let admission = sample_admission(&signer, 1, 1_500, 1);

    left.admit(7, &admission)
        .expect("left admission should succeed");
    right
        .admit(7, &admission)
        .expect("right admission should succeed");

    assert_eq!(left.get(7), right.get(7));
}

#[test]
fn gas_budget_violation_rejects_without_counter_mutation() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:a"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 60,
                volume: 10,
                notional_outstanding: 20,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: 1,
                current_drawdown_pct: 5,
            },
        )
        .expect("first admission should fit daily gas budget");

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:b"),
                intent_nonce: 2,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_700,
                gas_cost: 50,
                volume: 10,
                notional_outstanding: 25,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: 1,
                current_drawdown_pct: 5,
            },
        )
        .expect_err("second admission should exceed daily gas budget");

    let policy = store.get(7).expect("policy should remain present");
    assert_eq!(err, PolicyError::GasBudgetExceeded);
    assert_eq!(policy.counters.gas_used_today, 60);
    assert_eq!(policy.counters.volume_today, 10);
}

#[test]
fn rate_limits_and_daily_resets_are_deterministic() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    for timestamp_ms in [1_500, 1_700] {
        store
            .admit(
                7,
                &PolicyAdmission {
                    signed_session: signed_session(&signer, 7, 7, &[timestamp_ms as u8]),
                    intent_nonce: if timestamp_ms == 1_500 { 1 } else { 2 },
                    block_number: 1,
                    expected_chain_id: 7,
                    timestamp_ms,
                    gas_cost: 10,
                    volume: 10,
                    notional_outstanding: 10,
                    intent_notional: 10,
                    market_id: Some(7),
                    realized_pnl_delta: 1,
                    current_drawdown_pct: 5,
                },
            )
            .expect("first two same-second admissions should succeed");
    }

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:c"),
                intent_nonce: 3,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_900,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: 1,
                current_drawdown_pct: 5,
            },
        )
        .expect_err("third same-second admission should hit rate limit");
    assert_eq!(err, PolicyError::IntentRateLimitExceeded);

    store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:d"),
                intent_nonce: 3,
                block_number: 2,
                expected_chain_id: 7,
                timestamp_ms: 2_500,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: 1,
                current_drawdown_pct: 5,
            },
        )
        .expect("new second should reset the per-second limit");

    store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:e"),
                intent_nonce: 4,
                block_number: 86_401,
                expected_chain_id: 7,
                timestamp_ms: 86_401_000,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: 1,
                current_drawdown_pct: 5,
            },
        )
        .expect("new day should reset the daily windows");

    let policy = store.get(7).expect("policy should exist");
    assert_eq!(policy.counters.intents_today, 1);
    assert_eq!(policy.counters.volume_today, 10);
}

#[test]
fn market_and_notional_limits_reject_without_counter_mutation() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    let mut policy = sample_policy(PolicyState::Active);
    policy.admission_rules.allowed_markets = vec![7];
    policy.admission_rules.max_trade_notional = 50;
    policy.admission_rules.max_notional_outstanding = 90;
    store
        .insert(policy)
        .expect("policy insertion should succeed");

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:wrong-market"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(8),
                realized_pnl_delta: 0,
                current_drawdown_pct: 0,
            },
        )
        .expect_err("wrong market should reject");
    assert_eq!(err, PolicyError::MarketNotAllowed);

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:too-large"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 10,
                volume: 60,
                notional_outstanding: 60,
                intent_notional: 60,
                market_id: Some(7),
                realized_pnl_delta: 0,
                current_drawdown_pct: 0,
            },
        )
        .expect_err("oversized trade should reject");
    assert_eq!(err, PolicyError::TradeNotionalExceeded);

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:too-much-risk"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 10,
                volume: 40,
                notional_outstanding: 100,
                intent_notional: 40,
                market_id: Some(7),
                realized_pnl_delta: 0,
                current_drawdown_pct: 0,
            },
        )
        .expect_err("notional cap should reject");
    assert_eq!(err, PolicyError::NotionalOutstandingExceeded);

    let policy = store.get(7).expect("policy should exist");
    assert_eq!(policy.counters.intents_today, 0);
    assert_eq!(policy.next_intent_nonce, 1);
}

#[test]
fn market_scoped_policy_allows_marketless_zero_notional_control_intent() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    let mut policy = sample_policy(PolicyState::Active);
    policy.admission_rules.allowed_markets = vec![7];
    store
        .insert(policy)
        .expect("policy insertion should succeed");

    store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:create-sub-policy"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 15,
                volume: 0,
                notional_outstanding: 0,
                intent_notional: 0,
                market_id: None,
                realized_pnl_delta: 0,
                current_drawdown_pct: 0,
            },
        )
        .expect("marketless control intent should bypass market scope");

    let policy = store.get(7).expect("policy should exist");
    assert_eq!(policy.counters.intents_today, 1);
    assert_eq!(policy.counters.volume_today, 0);
    assert_eq!(policy.counters.gas_used_today, 15);
}

#[test]
fn market_scoped_policy_allows_marketless_zero_notional_control_intent_under_load() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    let mut policy = sample_policy(PolicyState::Active);
    policy.admission_rules.allowed_markets = vec![7];
    policy.counters.notional_outstanding = 250;
    store
        .insert(policy)
        .expect("policy insertion should succeed");

    store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:create-sub-policy:loaded"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 15,
                volume: 0,
                notional_outstanding: 250,
                intent_notional: 0,
                market_id: None,
                realized_pnl_delta: 0,
                current_drawdown_pct: 0,
            },
        )
        .expect("marketless inert intent should bypass market scope under load");

    let policy = store.get(7).expect("policy should exist");
    assert_eq!(policy.counters.notional_outstanding, 250);
    assert_eq!(policy.counters.intents_today, 1);
    assert_eq!(policy.counters.volume_today, 0);
}

#[test]
fn market_scoped_policy_still_rejects_marketless_value_transfer() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    let mut policy = sample_policy(PolicyState::Active);
    policy.admission_rules.allowed_markets = vec![7];
    store
        .insert(policy)
        .expect("policy insertion should succeed");

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:transfer"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 10,
                volume: 25,
                notional_outstanding: 25,
                intent_notional: 25,
                market_id: None,
                realized_pnl_delta: 0,
                current_drawdown_pct: 0,
            },
        )
        .expect_err("marketless transfer should still require market scope");
    assert_eq!(err, PolicyError::MarketNotAllowed);
}

#[test]
fn minute_rate_limit_resets_deterministically() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    let mut policy = sample_policy(PolicyState::Active);
    policy.terms.max_intents_per_second = 10;
    policy.terms.max_intents_per_day = 10;
    policy.admission_rules.max_intents_per_minute = 1;
    store
        .insert(policy)
        .expect("policy insertion should succeed");

    store
        .admit(7, &sample_admission(&signer, 1, 1_500, 1))
        .expect("first minute admission should fit");
    let err = store
        .admit(7, &sample_admission(&signer, 2, 2_500, 1))
        .expect_err("second same-minute admission should reject");
    assert_eq!(err, PolicyError::MinuteIntentLimitExceeded);

    store
        .admit(7, &sample_admission(&signer, 2, 61_000, 2))
        .expect("new minute should reset the limit");

    let policy = store.get(7).expect("policy should exist");
    assert_eq!(policy.counters.intents_this_minute, 1);
    assert_eq!(policy.next_intent_nonce, 3);
}

#[test]
fn drawdown_auto_revokes_policy() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    let err = store
        .admit(
            7,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 7, 7, b"intent:loss"),
                intent_nonce: 1,
                block_number: 1,
                expected_chain_id: 7,
                timestamp_ms: 1_500,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: -5,
                current_drawdown_pct: 40,
            },
        )
        .expect_err("drawdown breach should revoke policy");

    let policy = store.get(7).expect("policy should exist");
    assert_eq!(err, PolicyError::DrawdownExceededAndRevoked);
    assert_eq!(policy.state, PolicyState::Revoked);
    assert_eq!(policy.revoked_at, Some(1_500));
}

#[test]
fn child_drawdown_auto_revoke_releases_parent_reservations() {
    let mut store = PolicyStore::default();
    let signer = signing_key(9);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("parent insertion should succeed");

    let mut child_terms = sample_terms();
    child_terms.gas_budget_per_day = 40;
    child_terms.max_volume_per_day = 120;
    child_terms.max_intents_per_day = 2;
    child_terms.max_intents_per_second = 1;
    child_terms.max_drawdown_pct = 20;

    let child = PolicyRecord::new(
        8,
        11,
        signer.verifying_key().to_bytes(),
        1_100,
        100_000,
        PolicyState::Active,
        child_terms,
    );

    store
        .create_sub_policy(7, child)
        .expect("sub-policy should fit within parent bounds");

    let err = store
        .admit(
            8,
            &PolicyAdmission {
                signed_session: signed_session(&signer, 8, 7, b"intent:loss"),
                intent_nonce: 1,
                block_number: 2,
                expected_chain_id: 7,
                timestamp_ms: 1_700,
                gas_cost: 10,
                volume: 10,
                notional_outstanding: 10,
                intent_notional: 10,
                market_id: Some(7),
                realized_pnl_delta: -5,
                current_drawdown_pct: 40,
            },
        )
        .expect_err("child drawdown breach should revoke child");

    let parent = store.get(7).expect("parent policy should exist");
    let child = store.get(8).expect("child policy should exist");

    assert_eq!(err, PolicyError::DrawdownExceededAndRevoked);
    assert_eq!(child.state, PolicyState::Revoked);
    assert_eq!(parent.reserved.gas_budget_per_day, 0);
    assert_eq!(parent.reserved.volume_per_day, 0);
}

#[test]
fn session_verification_accepts_current_grantee_signature() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    let result = store.verify_session(
        7,
        &signed_session(&signer, 7, 7, b"intent:place-order"),
        1,
        7,
    );

    assert_eq!(result, Ok(()));
}

#[test]
fn session_verification_rejects_wrong_chain_or_signature() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    let wrong_signer = signing_key(9);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    let wrong_chain = store
        .verify_session(7, &signed_session(&signer, 7, 99, b"intent"), 1, 7)
        .expect_err("wrong chain should fail");
    assert_eq!(
        wrong_chain,
        PolicyError::SessionChainMismatch {
            expected: 7,
            actual: 99,
        }
    );

    let wrong_signature = store
        .verify_session(7, &signed_session(&wrong_signer, 7, 7, b"intent"), 1, 7)
        .expect_err("wrong key should fail");
    assert_eq!(wrong_signature, PolicyError::InvalidSessionSignature);
}

#[test]
fn session_verification_rejects_tampered_intent_hash() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    let mut signed = signed_session(&signer, 7, 7, b"intent:place-order");
    signed.payload.intent_hash[0] ^= 0x55;

    let error = store
        .verify_session(7, &signed, 1, 7)
        .expect_err("tampered hash should fail");
    assert_eq!(error, PolicyError::InvalidSessionSignature);
}

#[test]
fn key_rotation_allows_same_block_and_rejects_next_block() {
    let mut store = PolicyStore::default();
    let old_signer = signing_key(8);
    let new_signer = signing_key(9);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    let event = store
        .rotate_key(7, new_signer.verifying_key().to_bytes(), 10)
        .expect("rotation should succeed");
    assert_eq!(event.policy_id, 7);

    store
        .verify_session(7, &signed_session(&old_signer, 7, 7, b"intent:old"), 10, 7)
        .expect("old key should still verify in the rotation block");

    let err = store
        .verify_session(7, &signed_session(&old_signer, 7, 7, b"intent:old"), 11, 7)
        .expect_err("old key should fail after the rotation block");
    assert_eq!(
        err,
        PolicyError::SessionKeyRevoked {
            revoked_at: 10,
            block_number: 11,
        }
    );

    store
        .verify_session(7, &signed_session(&new_signer, 7, 7, b"intent:new"), 10, 7)
        .expect("new key should verify immediately");
}

#[test]
fn session_verification_rejects_policy_id_replay() {
    let mut store = PolicyStore::default();
    let signer = signing_key(8);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy 7 insertion should succeed");

    let mut other_policy = sample_policy(PolicyState::Active);
    other_policy.id = 8;
    other_policy.key_state.grantee = signing_key(10).verifying_key().to_bytes();
    store
        .insert(other_policy)
        .expect("policy 8 insertion should succeed");

    let signed_for_seven = signed_session(&signer, 7, 7, b"intent:place-order");

    let err = store
        .verify_session(8, &signed_for_seven, 1, 7)
        .expect_err("session signed for policy 7 should not verify against policy 8");
    assert_eq!(
        err,
        PolicyError::SessionPolicyMismatch {
            expected: 8,
            actual: 7,
        }
    );
}

#[test]
fn preverified_admission_skips_signature_but_preserves_state_checks() {
    let mut store = PolicyStore::default();
    let wrong_signer = signing_key(9);
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    let admission = sample_admission(&wrong_signer, 1, 1_700, 1);
    let signature_error = store
        .commit_admission(7, &admission)
        .expect_err("regular admission should verify the session signature");
    assert_eq!(signature_error, PolicyError::InvalidSessionSignature);

    store
        .commit_preverified_admission(7, &admission)
        .expect("preverified admission should trust prior signature verification");
    assert_eq!(store.next_intent_nonce(7), Ok(2));

    let mut replay = sample_admission(&wrong_signer, 1, 1_800, 2);
    let nonce_error = store
        .commit_preverified_admission(7, &replay)
        .expect_err("preverified admission should still enforce nonce ordering");
    assert_eq!(
        nonce_error,
        PolicyError::InvalidIntentNonce {
            expected: 2,
            actual: 1,
        }
    );

    replay.intent_nonce = 2;
    replay.signed_session.payload.chain_id = 99;
    let chain_error = store
        .commit_preverified_admission(7, &replay)
        .expect_err("preverified admission should still reject chain replay");
    assert_eq!(
        chain_error,
        PolicyError::SessionChainMismatch {
            expected: 7,
            actual: 99,
        }
    );
}

#[test]
fn signed_owner_call_rejects_account_id_replay() {
    let mut accounts = AccountStore::default();
    let mut policies = PolicyStore::default();
    let owner = signing_key(8);
    accounts
        .insert(StandardAgentAccount::new(
            11,
            owner.verifying_key().to_bytes(),
            1_000,
            hash(9),
        ))
        .expect("account 11 should insert");
    accounts
        .insert(StandardAgentAccount::new(
            12,
            owner.verifying_key().to_bytes(),
            1_000,
            hash(9),
        ))
        .expect("account 12 should insert");

    let mut signed_call = signed_owner_call(&owner, 11, 7, 0, OwnerAction::Pause);
    signed_call.account_id = 12;

    let err = accounts
        .apply_signed_owner_call(&signed_call, 7, 1, 1_100, &[hash(9)], &mut policies)
        .expect_err("owner call signed for account 11 must not apply to account 12");

    assert_eq!(err, AccountError::InvalidOwnerSignature);
}

#[test]
fn signed_owner_call_rejects_chain_id_replay() {
    let mut accounts = AccountStore::default();
    let mut policies = PolicyStore::default();
    let owner = signing_key(8);
    accounts
        .insert(StandardAgentAccount::new(
            11,
            owner.verifying_key().to_bytes(),
            1_000,
            hash(9),
        ))
        .expect("account should insert");

    let mut signed_call = signed_owner_call(&owner, 11, 7, 0, OwnerAction::Pause);
    signed_call.chain_id = 99;

    let err = accounts
        .apply_signed_owner_call(&signed_call, 99, 1, 1_100, &[hash(9)], &mut policies)
        .expect_err("owner call signed for chain 7 must not replay on chain 99");

    assert_eq!(err, AccountError::InvalidOwnerSignature);
}

#[test]
fn signed_owner_call_rejects_tampered_action_with_replayed_signature() {
    let mut accounts = AccountStore::default();
    let mut policies = PolicyStore::default();
    let owner = signing_key(8);
    accounts
        .insert(StandardAgentAccount::new(
            11,
            owner.verifying_key().to_bytes(),
            1_000,
            hash(9),
        ))
        .expect("account should insert");

    let mut signed_call = signed_owner_call(&owner, 11, 7, 0, OwnerAction::Pause);
    signed_call.call.action = OwnerAction::Unpause;

    let err = accounts
        .apply_signed_owner_call(&signed_call, 7, 1, 1_100, &[hash(9)], &mut policies)
        .expect_err("signature bound to Pause should not authorize Unpause");

    assert_eq!(err, AccountError::InvalidOwnerSignature);
}

#[test]
fn revoking_already_revoked_policy_is_idempotent() {
    let mut store = PolicyStore::default();
    store
        .insert(sample_policy(PolicyState::Active))
        .expect("policy insertion should succeed");

    store
        .revoke(7, 2, 2_000)
        .expect("first revoke should succeed");
    let snapshot = store.get(7).cloned().expect("policy should exist");

    store
        .revoke(7, 5, 9_000)
        .expect("redundant revoke should be accepted as a no-op");

    let after = store.get(7).expect("policy should still exist");
    assert_eq!(after.state, PolicyState::Revoked);
    assert_eq!(after.revoked_at, snapshot.revoked_at);
    assert_eq!(after.key_state.revoked_at, snapshot.key_state.revoked_at);
}
