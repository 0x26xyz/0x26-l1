//! Differential corpus for the prechecked resting-order path.
//!
//! The in-place path is selected by a process-wide cached environment flag
//! (`KOMBAT_PHASE6_INPLACE_RESTING_COMMIT`), so one process cannot run both
//! branches of `apply_mahi_committed_blocks_merged`. This test therefore
//! replays one deterministic corpus and writes a per-step digest to the file
//! named by `PAPER_DIFF_OUT`. `scripts/paper-differential.sh` runs it once
//! with the flag unset (in-place path on) and once with the flag set to `0`
//! (general overlay path only), then compares the two digest files.
//!
//! Without `PAPER_DIFF_OUT` the test still replays the corpus and checks that
//! it is deterministic within the process.

use super::*;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const SCENARIOS: u64 = 400;
const STEPS_PER_SCENARIO: u64 = 6;
const POLICY_IDS: [u64; 4] = [42, 43, 44, 45];
const MARKETS: [u64; 2] = [7, 8];

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn hex8(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(16);
    for byte in &digest[..8] {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn debug_digest<T: std::fmt::Debug>(value: &T) -> String {
    hex8(format!("{value:?}").as_bytes())
}

fn corpus_policy(policy_id: u64, max_intents_per_day: u32) -> PolicyRecord {
    let signer = signing_key(8);
    PolicyRecord::new(
        policy_id,
        policy_id - 31,
        signer.verifying_key().to_bytes(),
        1_000,
        172_800_000,
        PolicyState::Active,
        PolicyTerms {
            gas_budget_per_day: 1_000_000,
            gas_refill_per_day: 1_000_000,
            max_intents_per_second: u32::MAX,
            max_intents_per_day,
            max_volume_per_day: u128::MAX,
            max_drawdown_pct: 100,
            ..PolicyTerms::default()
        },
    )
}

#[derive(Clone, Copy, Debug)]
enum BodyShape {
    /// One side per market; each order at or behind the same-side tail.
    AppendCompatible,
    /// One side per market, but some orders improve on the tail price.
    PriorityImproving,
    /// Both sides in one market, without crossing.
    MixedSideResting,
    /// Orders that cross the opposite side.
    Crossing,
    /// Append-compatible orders plus a duplicate client-order key.
    DuplicateKey,
    /// Append-compatible orders plus a cancel of an earlier order.
    WithCancel,
    /// Append-compatible orders with one policy skipping a nonce.
    NonceGap,
}

const SHAPES: [BodyShape; 7] = [
    BodyShape::AppendCompatible,
    BodyShape::PriorityImproving,
    BodyShape::MixedSideResting,
    BodyShape::Crossing,
    BodyShape::DuplicateKey,
    BodyShape::WithCancel,
    BodyShape::NonceGap,
];

struct Scenario {
    engine: SingleNodeEngine,
    rng: Lcg,
    next_nonce: BTreeMap<u64, u64>,
    next_client_order_id: u64,
    placed: Vec<(u64, u64, u64)>,
    bid_tail: BTreeMap<u64, u64>,
    ask_tail: BTreeMap<u64, u64>,
    side_for_market: BTreeMap<u64, IntentOrderSide>,
    seed_mempool: bool,
    round: u64,
    timestamp_ms: u64,
    all_intents: Vec<SignedIntent>,
    label: String,
}

impl Scenario {
    fn new(seed: u64) -> Self {
        let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03);
        let mut engine = SingleNodeEngine::new(single_node_config());
        let tight_budget = rng.chance(30);
        for policy_id in POLICY_IDS {
            let max_per_day = if tight_budget && rng.chance(50) {
                3 + rng.below(6) as u32
            } else {
                u32::MAX
            };
            engine
                .policy_store_mut()
                .insert(corpus_policy(policy_id, max_per_day))
                .expect("corpus policy should insert");
        }
        let seed_mempool = rng.chance(50);
        Self {
            engine,
            rng,
            next_nonce: POLICY_IDS.iter().map(|id| (*id, 1)).collect(),
            next_client_order_id: 1_000,
            placed: Vec::new(),
            bid_tail: MARKETS.iter().map(|m| (*m, 500)).collect(),
            ask_tail: MARKETS.iter().map(|m| (*m, 600)).collect(),
            side_for_market: BTreeMap::new(),
            seed_mempool,
            round: 0,
            timestamp_ms: 1_200,
            all_intents: Vec::new(),
            label: String::new(),
        }
    }

    fn intent(&mut self, policy_id: u64, kind: IntentKind) -> SignedIntent {
        let nonce = self.next_nonce[&policy_id];
        self.next_nonce.insert(policy_id, nonce + 1);
        signed_intent_for_policy(
            &signing_key(8),
            policy_id,
            policy_id - 31,
            nonce,
            10_000_000,
            kind,
        )
    }

    fn place(
        &mut self,
        policy_id: u64,
        market_id: u64,
        side: IntentOrderSide,
        price_ticks: u64,
        client_order_id: Option<u64>,
    ) -> SignedIntent {
        let client_order_id = client_order_id.unwrap_or_else(|| {
            self.next_client_order_id += 1;
            self.next_client_order_id
        });
        self.placed.push((policy_id, market_id, client_order_id));
        let quantity_lots = 1 + self.rng.below(3);
        self.intent(
            policy_id,
            IntentKind::PlaceOrder {
                market_id,
                client_order_id,
                side,
                quantity_lots,
                kind: IntentOrderKind::Limit { price_ticks },
                time_in_force: IntentTimeInForce::Gtc,
                post_only: false,
            },
        )
    }

    /// A resting price at or behind the current same-side tail.
    fn append_price(&mut self, market_id: u64, side: &IntentOrderSide) -> u64 {
        let step = self.rng.below(3);
        match side {
            IntentOrderSide::Bid => {
                let price = self.bid_tail[&market_id].saturating_sub(step).max(1);
                self.bid_tail.insert(market_id, price);
                price
            }
            IntentOrderSide::Ask => {
                let price = self.ask_tail[&market_id] + step;
                self.ask_tail.insert(market_id, price);
                price
            }
        }
    }

    fn sticky_side(&mut self, market_id: u64) -> IntentOrderSide {
        if let Some(side) = self.side_for_market.get(&market_id) {
            return side.clone();
        }
        let side = if self.rng.chance(50) {
            IntentOrderSide::Bid
        } else {
            IntentOrderSide::Ask
        };
        self.side_for_market.insert(market_id, side.clone());
        side
    }

    fn body_intents(&mut self, shape: BodyShape) -> Vec<SignedIntent> {
        self.side_for_market.clear();
        let count = 2 + self.rng.below(7);
        let mut intents = Vec::new();
        for index in 0..count {
            let policy_id = POLICY_IDS[self.rng.below(POLICY_IDS.len() as u64) as usize];
            let market_id = MARKETS[self.rng.below(MARKETS.len() as u64) as usize];
            let side = self.sticky_side(market_id);
            let price = self.append_price(market_id, &side);
            let last = index + 1 == count;
            let intent = match shape {
                BodyShape::AppendCompatible => self.place(policy_id, market_id, side, price, None),
                BodyShape::PriorityImproving if last => {
                    let improved = match side {
                        IntentOrderSide::Bid => price + 20,
                        IntentOrderSide::Ask => price.saturating_sub(20).max(551),
                    };
                    self.place(policy_id, market_id, side, improved, None)
                }
                BodyShape::MixedSideResting if last => {
                    let (other, other_price) = match side {
                        IntentOrderSide::Bid => (IntentOrderSide::Ask, 900),
                        IntentOrderSide::Ask => (IntentOrderSide::Bid, 100),
                    };
                    self.place(policy_id, market_id, other, other_price, None)
                }
                BodyShape::Crossing if last => {
                    let (other, cross_price) = match side {
                        IntentOrderSide::Bid => (IntentOrderSide::Ask, 1),
                        IntentOrderSide::Ask => (IntentOrderSide::Bid, 5_000),
                    };
                    self.place(policy_id, market_id, other, cross_price, None)
                }
                BodyShape::DuplicateKey if last && !self.placed.is_empty() => {
                    let pick = self.rng.below(self.placed.len() as u64) as usize;
                    let (dup_policy, dup_market, dup_key) = self.placed[pick];
                    let dup_side = self.sticky_side(dup_market);
                    let dup_price = self.append_price(dup_market, &dup_side);
                    self.place(dup_policy, dup_market, dup_side, dup_price, Some(dup_key))
                }
                BodyShape::WithCancel if last && !self.placed.is_empty() => {
                    let pick = self.rng.below(self.placed.len() as u64) as usize;
                    let (cancel_policy, cancel_market, cancel_key) = self.placed[pick];
                    self.intent(
                        cancel_policy,
                        IntentKind::CancelOrder {
                            market_id: cancel_market,
                            client_order_id: cancel_key,
                        },
                    )
                }
                BodyShape::NonceGap if last => {
                    let skipped = self.next_nonce[&policy_id];
                    self.next_nonce.insert(policy_id, skipped + 1);
                    self.place(policy_id, market_id, side, price, None)
                }
                _ => self.place(policy_id, market_id, side, price, None),
            };
            intents.push(intent);
        }
        intents
    }

    fn step(&mut self) -> (BodyShape, usize, String) {
        let shape = if self.rng.chance(40) {
            BodyShape::AppendCompatible
        } else {
            SHAPES[self.rng.below(SHAPES.len() as u64) as usize]
        };
        let intents = self.body_intents(shape);
        self.all_intents.extend(intents.iter().cloned());
        self.timestamp_ms += 100;
        if self.seed_mempool {
            for intent in &intents {
                let _ = self.engine.submit_intent(intent.clone(), self.timestamp_ms);
            }
        }

        let block_count = (1 + self.rng.below(3)).min(intents.len() as u64) as usize;
        let per_block = intents.len().div_ceil(block_count);
        let blocks = intents
            .chunks(per_block)
            .map(|chunk| {
                self.round += 1;
                MahiBlock::signed(
                    validator(9),
                    self.round,
                    1,
                    self.timestamp_ms,
                    Vec::new(),
                    chunk.to_vec(),
                    [9; 32],
                )
            })
            .collect::<Vec<_>>();

        let events_before = self.engine.runtime_events().len();
        let outcome = match self.engine.apply_mahi_committed_blocks_merged(&blocks) {
            Ok(_) => "ok".to_string(),
            Err(error) => format!("err:{}", debug_digest(&error)),
        };
        (shape, intents.len(), self.digest(&outcome, events_before))
    }

    fn digest(&self, outcome: &str, events_before: usize) -> String {
        let engine = &self.engine;
        let head = engine.head();
        let books = engine
            .order_books
            .iter()
            .map(|(market_id, book)| (*market_id, book.bids().to_vec(), book.asks().to_vec()))
            .collect::<Vec<_>>();
        let new_events = &engine.runtime_events()[events_before..];
        let event_kinds = new_events
            .iter()
            .map(|record| (&record.block_height, &record.kind))
            .collect::<Vec<_>>();
        let rejected_events = new_events
            .iter()
            .filter(|record| matches!(record.kind, RuntimeEventKind::IntentRejected { .. }))
            .count();
        let statuses = self
            .all_intents
            .iter()
            .map(|intent| engine.intent_status(intent_id(intent)).stage)
            .collect::<Vec<_>>();
        if std::env::var("PAPER_DIFF_DUMP").ok().as_deref() == Some(self.label.as_str()) {
            eprintln!("paper_diff_dump positions={:?}", engine.positions);
            eprintln!("paper_diff_dump events={:?}", event_kinds);
        }
        // Positions without the stored last-fill event sequence, which shifts
        // whenever an earlier step emitted a different number of events.
        let positions_without_fill_seq = engine
            .positions
            .iter()
            .map(|(key, ledger)| {
                let mut ledger = ledger.clone();
                ledger.last_fill_seq = None;
                (*key, ledger)
            })
            .collect::<Vec<_>>();
        format!(
            "outcome={outcome} height={} head={} state_root={} event_root={} tx_root={} \
             body_items={} executor_root={} policy={} books={} mandates={} collateral={} \
             positions={} positions_without_fill_seq={} events={} event_count={} rejected_events={} statuses={} mempool={}",
            head.header.height,
            hex8(&block_hash(head)),
            hex8(&head.header.state_root),
            hex8(&head.header.event_root),
            hex8(&head.header.tx_root),
            head.body.items.len(),
            hex8(&engine.committed_state_root()),
            debug_digest(engine.policy_store()),
            debug_digest(&books),
            debug_digest(&engine.resting_order_mandates),
            debug_digest(&engine.isolated_collateral),
            debug_digest(&engine.positions),
            debug_digest(&positions_without_fill_seq),
            debug_digest(&event_kinds),
            new_events.len(),
            rejected_events,
            debug_digest(&statuses),
            engine.intent_mempool().len(),
        )
    }
}

fn run_corpus(mut marker: impl FnMut(u64, u64)) -> Vec<String> {
    let mut lines = Vec::new();
    for scenario_index in 0..SCENARIOS {
        let mut scenario = Scenario::new(scenario_index);
        for step_index in 0..STEPS_PER_SCENARIO {
            marker(scenario_index, step_index);
            scenario.label = format!("{scenario_index}.{step_index}");
            let (shape, items, digest) = scenario.step();
            lines.push(format!(
                "case={scenario_index}.{step_index} shape={shape:?} intents={items} \
                 mempool_seeded={} {digest}",
                scenario.seed_mempool
            ));
        }
    }
    lines
}

#[test]
fn paper_differential_corpus_replays_and_records_digests() {
    // The marker goes to stderr so the driver can pair each step with the
    // `mahi_exec_profile mode=...` line that the engine prints when
    // `KOMBAT_MAHI_EXEC_PROFILE` is set.
    let lines = run_corpus(|scenario, step| eprintln!("paper_diff_case {scenario}.{step}"));
    assert_eq!(lines.len() as u64, SCENARIOS * STEPS_PER_SCENARIO);
    if let Ok(path) = std::env::var("PAPER_DIFF_OUT") {
        std::fs::write(&path, lines.join("\n") + "\n").expect("digest file should be writable");
    } else {
        let replay = run_corpus(|_, _| {});
        assert_eq!(lines, replay, "corpus replay should be deterministic");
    }
}
