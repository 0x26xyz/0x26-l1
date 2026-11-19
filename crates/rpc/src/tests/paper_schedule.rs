//! Cross-replica schedule checks for the fixed-count execution-group rule.
//!
//! Each simulated replica owns its own `MahiState` queue and its own
//! `SingleNodeEngine`. All replicas receive the same decided block sequence,
//! but block arrival, payload verification, and drain calls are interleaved by
//! an independent pseudo-random schedule per replica. Drained blocks are split
//! and applied exactly as the commit executor does: consecutive chunks of the
//! batch size through `apply_mahi_committed_blocks_merged`.
//!
//! The partial-flush drain used as a negative control is a test-local model of
//! the reverted rule ("if every queued block is verified, drain the whole
//! verified prefix; otherwise drain a multiple of the batch size"). It is not
//! the original patch.

use super::*;
use crate::{MahiBlockVerificationStatus, MahiState};

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
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrainRule {
    FixedGroups,
    PartialFlushModel,
}

fn decided_sequence(length: u64) -> Vec<MahiBlock> {
    (1..=length)
        .map(|index| {
            let author = 1 + (index % 4) as u8;
            MahiBlock::signed(
                validator(author),
                index,
                1,
                2_000 + index * 10,
                Vec::new(),
                Vec::new(),
                [author; 32],
            )
        })
        .collect()
}

fn fresh_state() -> MahiState {
    crate::build_mahi_state(
        true,
        Some(&network_bootstrap_config()),
        kombat_chain_consensus::mahi::CommitRule::Mahi,
        1,
    )
    .expect("mahi state should build from the test bootstrap config")
}

fn verified_prefix_len(state: &MahiState) -> usize {
    state
        .pending_execution
        .iter()
        .take_while(|block| {
            state.verification_status.get(&block.block_ref()).copied()
                == Some(MahiBlockVerificationStatus::Verified)
        })
        .count()
}

fn drain(state: &mut MahiState, rule: DrainRule, batch_size: usize) -> Vec<MahiBlock> {
    match rule {
        DrainRule::FixedGroups => state.drain_ready_execution_for_batch(batch_size),
        DrainRule::PartialFlushModel => {
            let verified = verified_prefix_len(state);
            let take = if verified == state.pending_execution.len() {
                verified
            } else {
                verified - (verified % batch_size)
            };
            state.pending_execution.drain(..take).collect()
        }
    }
}

struct ReplicaRun {
    /// Rounds of the blocks in each executed group, in execution order.
    groups: Vec<Vec<u64>>,
    head_hash: [u8; 32],
    head_height: u64,
    left_in_queue: usize,
}

fn run_replica(
    sequence: &[MahiBlock],
    rule: DrainRule,
    batch_size: usize,
    schedule_seed: u64,
    failed_round: Option<u64>,
) -> ReplicaRun {
    let mut rng = Lcg(schedule_seed);
    let mut state = fresh_state();
    let mut engine = single_node_engine();
    let mut groups = Vec::new();
    let mut arrived = 0_usize;
    let mut unverified = Vec::<usize>::new();

    let mut apply = |state: &mut MahiState, engine: &mut SingleNodeEngine| {
        let drained = drain(state, rule, batch_size);
        for chunk in drained.chunks(batch_size) {
            engine
                .apply_mahi_committed_blocks_merged(chunk)
                .expect("empty merged group should apply");
            groups.push(chunk.iter().map(|block| block.round).collect::<Vec<_>>());
        }
    };

    while arrived < sequence.len() || !unverified.is_empty() {
        match rng.below(3) {
            0 if arrived < sequence.len() => {
                let block = sequence[arrived].clone();
                state
                    .verification_status
                    .insert(block.block_ref(), MahiBlockVerificationStatus::Pending);
                state.pending_execution.push_back(block);
                unverified.push(arrived);
                arrived += 1;
            }
            1 if !unverified.is_empty() => {
                let pick = rng.below(unverified.len() as u64) as usize;
                let index = unverified.swap_remove(pick);
                let block = &sequence[index];
                let status = if failed_round == Some(block.round) {
                    MahiBlockVerificationStatus::Failed
                } else {
                    MahiBlockVerificationStatus::Verified
                };
                state.verification_status.insert(block.block_ref(), status);
            }
            _ => apply(&mut state, &mut engine),
        }
    }
    apply(&mut state, &mut engine);

    ReplicaRun {
        groups,
        head_hash: block_hash(engine.head()),
        head_height: engine.head().header.height,
        left_in_queue: state.pending_execution.len(),
    }
}

fn canonical_groups(length: u64, batch_size: usize) -> Vec<Vec<u64>> {
    let rounds = (1..=length).collect::<Vec<_>>();
    rounds
        .chunks(batch_size)
        .filter(|chunk| chunk.len() == batch_size)
        .map(<[u64]>::to_vec)
        .collect()
}

#[test]
fn paper_fixed_groups_agree_across_replica_schedules() {
    let mut schedules = 0_u64;
    for batch_size in [2_usize, 4, 8] {
        for length in [16_u64, 37, 64] {
            let sequence = decided_sequence(length);
            let expected = canonical_groups(length, batch_size);
            let reference = run_replica(&sequence, DrainRule::FixedGroups, batch_size, 1, None);
            assert_eq!(reference.groups, expected);
            for seed in 2..=40_u64 {
                let replica = run_replica(
                    &sequence,
                    DrainRule::FixedGroups,
                    batch_size,
                    seed * 7_919 + length,
                    None,
                );
                schedules += 1;
                assert_eq!(
                    replica.groups, expected,
                    "fixed groups should not depend on the schedule \
                     (b={batch_size}, n={length}, seed={seed})"
                );
                assert_eq!(replica.head_hash, reference.head_hash);
                assert_eq!(replica.head_height, reference.head_height);
                assert_eq!(replica.left_in_queue, length as usize % batch_size);
            }
        }
    }
    println!("paper_schedule fixed_groups schedules={schedules} divergent=0");
}

#[test]
fn paper_partial_flush_model_diverges_across_replica_schedules() {
    let batch_size = 8_usize;
    let length = 64_u64;
    let sequence = decided_sequence(length);
    let reference = run_replica(&sequence, DrainRule::PartialFlushModel, batch_size, 1, None);
    let mut divergent_partitions = 0_u64;
    let mut divergent_heads = 0_u64;
    let schedules = 39_u64;
    for seed in 2..=40_u64 {
        let replica = run_replica(
            &sequence,
            DrainRule::PartialFlushModel,
            batch_size,
            seed * 7_919 + length,
            None,
        );
        // Every replica still consumes the same decided sequence in order.
        let consumed = replica.groups.concat();
        assert_eq!(consumed, (1..=length).collect::<Vec<_>>());
        assert_eq!(replica.left_in_queue, 0);
        if replica.groups != reference.groups {
            divergent_partitions += 1;
        }
        if replica.head_hash != reference.head_hash {
            divergent_heads += 1;
        }
    }
    println!(
        "paper_schedule partial_flush_model schedules={schedules} \
         divergent_partitions={divergent_partitions} divergent_heads={divergent_heads}"
    );
    assert!(
        divergent_partitions > 0,
        "the partial-flush model should make group boundaries schedule-dependent"
    );
    assert!(
        divergent_heads > 0,
        "different partitions of one decided sequence should change the execution head"
    );
}

#[test]
fn paper_batch_size_mismatch_changes_execution_head() {
    let sequence = decided_sequence(64);
    let eight = run_replica(&sequence, DrainRule::FixedGroups, 8, 11, None);
    let four = run_replica(&sequence, DrainRule::FixedGroups, 4, 11, None);
    assert_eq!(eight.groups.concat(), four.groups.concat());
    assert_ne!(eight.groups, four.groups);
    assert_ne!(eight.head_hash, four.head_hash);
    println!(
        "paper_schedule batch_mismatch heights b8={} b4={} heads_differ=true",
        eight.head_height, four.head_height
    );
}

#[test]
fn paper_failed_verification_blocks_later_groups() {
    let batch_size = 8_usize;
    let length = 64_u64;
    let failed_round = 20_u64;
    let sequence = decided_sequence(length);
    for seed in 1..=20_u64 {
        let replica = run_replica(
            &sequence,
            DrainRule::FixedGroups,
            batch_size,
            seed * 104_729,
            Some(failed_round),
        );
        // Rounds 1..=16 form two complete groups ahead of the failed block.
        assert_eq!(replica.groups, canonical_groups(16, batch_size));
        assert_eq!(replica.left_in_queue, (length - 16) as usize);
    }
    println!("paper_schedule failed_verification executed_groups=2 stuck_blocks=48");
}

fn paper_cluster_view(
    servers: &[(u8, JsonRpcHttpServer)],
) -> Vec<(u8, u64, u64, String, u64, u64)> {
    servers
        .iter()
        .map(|(byte, server)| {
            let head = server.engine().head_snapshot();
            let metrics = consensus_metrics(server);
            (
                *byte,
                head.head_height,
                head.head_round,
                encode_hex(&head.head_hash)[..12].to_string(),
                metrics["mahi"]["execution_pending_count"]
                    .as_u64()
                    .unwrap_or(u64::MAX),
                metrics["mahi"]["committed_block_count"]
                    .as_u64()
                    .unwrap_or(u64::MAX),
            )
        })
        .collect()
}

fn paper_drive_rounds_loosely(servers: &[(u8, JsonRpcHttpServer)], from: u64, rounds: u64) {
    for round in from..from + rounds {
        let timestamp_ms = 1_000 + round * 100;
        for (_, server) in servers {
            let _ = server.maybe_run_mahi_round(timestamp_ms, 10);
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    std::thread::sleep(Duration::from_millis(800));
}

fn paper_state_roots_by_height(server: &JsonRpcHttpServer) -> BTreeMap<u64, [u8; 32]> {
    let engine = server.engine().lock().expect("engine lock should succeed");
    engine
        .blocks()
        .iter()
        .map(|block| (block.header.height, block.header.state_root))
        .collect()
}

struct PaperResetOutcome {
    pre_reset_height: u64,
    pre_reset_pending: Vec<u64>,
    heights: Vec<u64>,
    common_height: u64,
    first_divergent_height: Option<u64>,
    commit_log_hashes: BTreeSet<String>,
}

/// Runs the four-validator in-process cluster to a common head, applies the
/// production local DAG reset (`reset_mahi_state_from_durable_head`, the same
/// call the ancestor-stall self-heal uses) to the selected validators, keeps
/// driving rounds on every validator, and compares execution state roots at
/// every common height.
fn paper_run_reset_case(reset_indices: &[usize]) -> PaperResetOutcome {
    let servers = start_mahi_consensus_servers();
    drive_mahi_rounds_until_converged(&servers, 14);
    std::thread::sleep(Duration::from_millis(500));
    let before = paper_cluster_view(&servers);
    let pre_reset_height = before[0].1;
    assert!(
        before
            .iter()
            .all(|view| view.1 == pre_reset_height && view.3 == before[0].3),
        "cluster should share one head before the reset: {before:?}"
    );
    for index in reset_indices {
        servers[*index]
            .1
            .reset_mahi_state_from_durable_head("paper-reset-case")
            .expect("reset should report a boundary round");
    }
    paper_drive_rounds_loosely(&servers, 15, 30);
    let after = paper_cluster_view(&servers);
    let roots = servers
        .iter()
        .map(|(_, server)| paper_state_roots_by_height(server))
        .collect::<Vec<_>>();
    let common_height = roots
        .iter()
        .map(|by_height| by_height.len() as u64)
        .min()
        .unwrap_or(1)
        .saturating_sub(1);
    let first_divergent_height = (0..=common_height).find(|height| {
        roots
            .iter()
            .map(|by_height| by_height[height])
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    });
    let commit_log_hashes = servers
        .iter()
        .map(|(_, server)| consensus_metrics(server)["mahi"]["commit_log_hash_hex"].to_string())
        .collect();
    PaperResetOutcome {
        pre_reset_height,
        pre_reset_pending: before.iter().map(|view| view.4).collect(),
        heights: after.iter().map(|view| view.1).collect(),
        common_height,
        first_divergent_height,
        commit_log_hashes,
    }
}

#[test]
fn paper_simultaneous_reset_from_common_head_keeps_roots_equal() {
    let outcome = paper_run_reset_case(&[0, 1, 2, 3]);
    println!(
        "paper_restart simultaneous pre_reset_height={} dropped_pending={:?} heights={:?} \
         common_height={} first_divergent={:?} distinct_commit_logs={}",
        outcome.pre_reset_height,
        outcome.pre_reset_pending,
        outcome.heights,
        outcome.common_height,
        outcome.first_divergent_height,
        outcome.commit_log_hashes.len(),
    );
    assert!(outcome.common_height > outcome.pre_reset_height);
    assert_eq!(outcome.first_divergent_height, None);
}

#[test]
fn paper_staggered_reset_diverges_execution_roots() {
    let outcome = paper_run_reset_case(&[1]);
    println!(
        "paper_restart staggered pre_reset_height={} dropped_pending={:?} heights={:?} \
         common_height={} first_divergent={:?} distinct_commit_logs={}",
        outcome.pre_reset_height,
        outcome.pre_reset_pending,
        outcome.heights,
        outcome.common_height,
        outcome.first_divergent_height,
        outcome.commit_log_hashes.len(),
    );
    // Recorded behavior at this revision, not a desired property: one
    // validator that resets its volatile DAG while its peers continue
    // executes different groups from the first height after the reset.
    assert!(outcome.common_height > outcome.pre_reset_height);
    assert_eq!(
        outcome.first_divergent_height,
        Some(outcome.pre_reset_height + 1)
    );
}
