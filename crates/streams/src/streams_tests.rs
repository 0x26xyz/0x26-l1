use super::{
    component_name, EventFilter, EventStream, IntentStatusStage,
    RealCapitalInsuranceWalletSpendSource, RuntimeEventKind, RuntimeEventRecord, RuntimeEventTopic,
    StreamQuery, Tip5Hash,
};

fn event(seq: u64, kind: RuntimeEventKind) -> RuntimeEventRecord {
    RuntimeEventRecord {
        seq,
        block_height: Some(10),
        timestamp_ms: 1_000 + seq,
        kind,
    }
}

#[test]
fn reports_component_name() {
    assert_eq!(component_name(), "streams");
}

#[test]
fn real_capital_insurance_wallet_spent_reports_topic() {
    let event = RuntimeEventKind::RealCapitalInsuranceWalletSpent {
        amount: 7,
        balance: 11,
        source: RealCapitalInsuranceWalletSpendSource::KeeperReward,
    };

    assert_eq!(
        event.topic(),
        RuntimeEventTopic::RealCapitalInsuranceWalletSpent
    );
    assert_eq!(event.account_id(), None);
    assert_eq!(event.market_id(), None);
    assert_eq!(event.policy_id(), None);
}

#[test]
fn bridge_deposit_locked_reports_topic() {
    let tip5 = Tip5Hash {
        limb0: 1,
        limb1: 2,
        limb2: 3,
        limb3: 4,
        limb4: 5,
    };
    let event = RuntimeEventKind::BridgeDepositLocked {
        deposit_nonce: 42,
        tx_order_in_block: 3,
        recipient: [0x11; 20],
        amount_nicks: 65_536,
        nock_tx_id: tip5,
        note_name_first: tip5,
        note_name_last: tip5,
        nock_as_of: tip5,
    };

    assert_eq!(event.topic(), RuntimeEventTopic::BridgeDepositLocked);
    assert_eq!(event.account_id(), None);
    assert_eq!(event.market_id(), None);
    assert_eq!(event.policy_id(), None);
}

#[test]
fn filters_events_by_policy_and_topic_with_cursor() {
    let intent_id = [7; 32];
    let events = [
        event(
            0,
            RuntimeEventKind::IntentAccepted {
                intent_id,
                policy_id: 42,
                nonce: 1,
                class: super::IntentAdmissionClass::Direct,
            },
        ),
        event(
            1,
            RuntimeEventKind::IntentExecuted {
                intent_id,
                policy_id: 42,
                nonce: 1,
            },
        ),
        event(
            2,
            RuntimeEventKind::PolicyRevoked {
                account_id: 11,
                policy_id: 43,
                cascaded: false,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 1,
        before_seq: None,
        limit: 10,
        filter: EventFilter {
            block_height: None,
            account_id: None,
            market_id: None,
            policy_id: Some(42),
            intent_id: None,
            client_order_id: None,
            topic: Some(RuntimeEventTopic::IntentExecuted),
        },
        descending: false,
    });

    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].seq, 1);
    assert_eq!(page.next_cursor.next_seq, 2);
}

#[test]
fn ascending_query_honours_before_seq() {
    let intent_id = [7; 32];
    let events = [
        event(
            0,
            RuntimeEventKind::IntentAccepted {
                intent_id,
                policy_id: 42,
                nonce: 1,
                class: super::IntentAdmissionClass::Direct,
            },
        ),
        event(
            1,
            RuntimeEventKind::IntentExecuted {
                intent_id,
                policy_id: 42,
                nonce: 1,
            },
        ),
        event(
            2,
            RuntimeEventKind::PolicyRevoked {
                account_id: 11,
                policy_id: 43,
                cascaded: false,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: Some(2),
        limit: 10,
        filter: EventFilter::default(),
        descending: false,
    });

    assert_eq!(
        page.events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(page.next_cursor.next_seq, 2);
}

#[test]
fn filters_events_by_intent_id() {
    let left_intent = [1; 32];
    let right_intent = [2; 32];
    let events = [
        event(
            0,
            RuntimeEventKind::IntentAccepted {
                intent_id: left_intent,
                policy_id: 42,
                nonce: 1,
                class: super::IntentAdmissionClass::Direct,
            },
        ),
        event(
            1,
            RuntimeEventKind::IntentAccepted {
                intent_id: right_intent,
                policy_id: 42,
                nonce: 2,
                class: super::IntentAdmissionClass::Conditional,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: None,
        limit: 10,
        filter: EventFilter {
            block_height: None,
            account_id: None,
            market_id: None,
            policy_id: None,
            intent_id: Some(right_intent),
            client_order_id: None,
            topic: None,
        },
        descending: false,
    });

    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].seq, 1);
}

#[test]
fn finds_latest_event_for_intent() {
    let intent_id = [7; 32];
    let events = [
        event(
            0,
            RuntimeEventKind::IntentAccepted {
                intent_id,
                policy_id: 42,
                nonce: 1,
                class: super::IntentAdmissionClass::Conditional,
            },
        ),
        event(
            1,
            RuntimeEventKind::IntentPromoted {
                intent_id,
                policy_id: 42,
                nonce: 1,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let latest = stream
        .latest_intent_event(intent_id)
        .expect("intent should have event");

    assert_eq!(latest.seq, 1);
    assert_eq!(
        latest.kind.intent_status_stage(),
        Some(IntentStatusStage::Promoted)
    );
    assert_eq!(latest.kind.intent_nonce(), Some(1));
}

#[test]
fn filters_events_by_account_id() {
    let events = [
        event(
            0,
            RuntimeEventKind::AccountCreated {
                account_id: 11,
                owner: [1; 32],
            },
        ),
        event(
            1,
            RuntimeEventKind::PolicyCreated {
                account_id: 12,
                parent_policy_id: None,
                policy_id: 42,
            },
        ),
        event(
            2,
            RuntimeEventKind::PolicyCreated {
                account_id: 11,
                parent_policy_id: None,
                policy_id: 43,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: None,
        limit: 10,
        filter: EventFilter {
            block_height: None,
            account_id: Some(11),
            market_id: None,
            policy_id: None,
            intent_id: None,
            client_order_id: None,
            topic: None,
        },
        descending: false,
    });

    assert_eq!(page.events.len(), 2);
    assert_eq!(page.events[0].seq, 0);
    assert_eq!(page.events[1].seq, 2);
}

#[test]
fn filters_events_by_client_order_id() {
    let events = [
        event(
            0,
            RuntimeEventKind::OrderAccepted {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                status: super::OrderLifecycleStatus::Resting,
                filled_lots: 0,
                remaining_lots: 5,
            },
        ),
        event(
            1,
            RuntimeEventKind::OrderFilled {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                counterparty_account_id: 33,
                counterparty_client_order_id: 2,
                mandate_id: None,
                role: super::OrderFillRole::Maker,
                side: super::OrderFillSide::Sell,
                price_ticks: 103,
                quantity_lots: 3,
            },
        ),
        event(
            2,
            RuntimeEventKind::OrderAccepted {
                market_id: 7,
                account_id: 22,
                client_order_id: 9,
                status: super::OrderLifecycleStatus::Resting,
                filled_lots: 0,
                remaining_lots: 1,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: None,
        limit: 10,
        filter: EventFilter {
            block_height: None,
            account_id: Some(22),
            market_id: Some(7),
            policy_id: None,
            intent_id: None,
            client_order_id: Some(1),
            topic: None,
        },
        descending: false,
    });

    assert_eq!(page.events.len(), 2);
    assert_eq!(page.events[0].seq, 0);
    assert_eq!(page.events[1].seq, 1);
}

#[test]
fn filters_events_by_market_id() {
    let events = [
        event(
            0,
            RuntimeEventKind::OrderAccepted {
                market_id: 7,
                account_id: 22,
                client_order_id: 1,
                status: super::OrderLifecycleStatus::Resting,
                filled_lots: 0,
                remaining_lots: 5,
            },
        ),
        event(
            1,
            RuntimeEventKind::OrderAccepted {
                market_id: 9,
                account_id: 22,
                client_order_id: 2,
                status: super::OrderLifecycleStatus::Resting,
                filled_lots: 0,
                remaining_lots: 2,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: None,
        limit: 10,
        filter: EventFilter {
            block_height: None,
            account_id: None,
            market_id: Some(9),
            policy_id: None,
            intent_id: None,
            client_order_id: None,
            topic: Some(RuntimeEventTopic::OrderAccepted),
        },
        descending: false,
    });

    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].seq, 1);
}

#[test]
fn queries_events_descending_from_latest() {
    let events = [
        event(
            0,
            RuntimeEventKind::AccountCreated {
                account_id: 11,
                owner: [1; 32],
            },
        ),
        event(
            1,
            RuntimeEventKind::PolicyCreated {
                account_id: 11,
                parent_policy_id: None,
                policy_id: 42,
            },
        ),
        event(
            2,
            RuntimeEventKind::IntentAccepted {
                intent_id: [7; 32],
                policy_id: 42,
                nonce: 1,
                class: super::IntentAdmissionClass::Direct,
            },
        ),
    ];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: None,
        limit: 2,
        filter: EventFilter::default(),
        descending: true,
    });
    assert_eq!(
        page.events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(page.next_cursor.next_seq, 1);

    let older = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: Some(page.next_cursor.next_seq),
        limit: 2,
        filter: EventFilter::default(),
        descending: true,
    });
    assert_eq!(
        older
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(older.next_cursor.next_seq, 0);
}

#[test]
fn filters_events_by_block_height() {
    let mut first = event(
        0,
        RuntimeEventKind::AccountCreated {
            account_id: 11,
            owner: [1; 32],
        },
    );
    first.block_height = Some(10);
    let mut second = event(
        1,
        RuntimeEventKind::PolicyCreated {
            account_id: 11,
            parent_policy_id: None,
            policy_id: 42,
        },
    );
    second.block_height = Some(11);
    let events = [first, second];
    let stream = EventStream::new(&events);

    let page = stream.query(StreamQuery {
        from_seq: 0,
        before_seq: None,
        limit: 10,
        filter: EventFilter {
            block_height: Some(11),
            ..EventFilter::default()
        },
        descending: false,
    });
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].seq, 1);
}
