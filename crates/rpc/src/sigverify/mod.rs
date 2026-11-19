use crossbeam_channel::{bounded, Receiver, Sender};
use ed25519_dalek::{verify_batch, Signature, VerifyingKey};
use kombat_chain_consensus::{mahi::BlockRef as MahiBlockRef, SignedIntent};
use kombat_chain_node::{signed_session_payload_for_intent, verify_intent_signature, EngineHandle};
use kombat_chain_policy::{PolicyError, SessionVerificationMaterial};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

const MAX_SIGVERIFY_LATENCY_SAMPLES: usize = 512;
const MAX_SIGVERIFY_QUEUE_DEPTH: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MahiSigverifyMetricsSnapshot {
    pub queue_depth: usize,
    pub batch_latency_ms: Vec<u64>,
    pub per_intent_verified_total: u64,
    pub failures_total: u64,
}

#[derive(Clone, Debug)]
pub struct MahiSigverifyBatch {
    pub block_ref: MahiBlockRef,
    pub intents: Vec<SignedIntent>,
    pub expected_chain_id: u64,
    pub block_number: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MahiSigverifyResult {
    Verified {
        block_ref: MahiBlockRef,
        verified_intents: usize,
        latency_ms: u64,
    },
    Failed {
        block_ref: MahiBlockRef,
        failures: usize,
        latency_ms: u64,
        reason: String,
    },
}

#[derive(Debug)]
struct SigverifyMetrics {
    queue_depth: AtomicUsize,
    per_intent_verified_total: AtomicU64,
    failures_total: AtomicU64,
    batch_latency_ms: Mutex<Vec<u64>>,
}

impl Default for SigverifyMetrics {
    fn default() -> Self {
        Self {
            queue_depth: AtomicUsize::new(0),
            per_intent_verified_total: AtomicU64::new(0),
            failures_total: AtomicU64::new(0),
            batch_latency_ms: Mutex::new(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MahiSigverifyPool {
    sender: Sender<MahiSigverifyBatch>,
    results: Receiver<MahiSigverifyResult>,
    metrics: Arc<SigverifyMetrics>,
}

impl MahiSigverifyPool {
    pub fn new(engine: Arc<EngineHandle>) -> Self {
        let worker_count = num_cpus::get().saturating_sub(2).max(1);
        let (sender, receiver) = bounded::<MahiSigverifyBatch>(MAX_SIGVERIFY_QUEUE_DEPTH);
        let (result_sender, results) = bounded::<MahiSigverifyResult>(MAX_SIGVERIFY_QUEUE_DEPTH);
        let metrics = Arc::new(SigverifyMetrics::default());
        for _ in 0..worker_count {
            let engine = engine.clone();
            let receiver = receiver.clone();
            let result_sender = result_sender.clone();
            let metrics = metrics.clone();
            thread::spawn(move || loop {
                let Ok(batch) = receiver.recv() else {
                    break;
                };
                metrics.queue_depth.fetch_sub(1, Ordering::SeqCst);
                let started_at = Instant::now();
                let result = verify_batch_job(&engine, batch, started_at, &metrics);
                let _ = result_sender.send(result);
            });
        }
        Self {
            sender,
            results,
            metrics,
        }
    }

    pub fn enqueue(&self, batch: MahiSigverifyBatch) -> Result<(), String> {
        self.metrics.queue_depth.fetch_add(1, Ordering::SeqCst);
        if let Err(error) = self.sender.send(batch) {
            self.metrics.queue_depth.fetch_sub(1, Ordering::SeqCst);
            return Err(format!("mahi sigverify queue closed: {error}"));
        }
        Ok(())
    }

    pub fn results(&self) -> Receiver<MahiSigverifyResult> {
        self.results.clone()
    }

    pub fn snapshot(&self) -> MahiSigverifyMetricsSnapshot {
        MahiSigverifyMetricsSnapshot {
            queue_depth: self.metrics.queue_depth.load(Ordering::SeqCst),
            batch_latency_ms: self
                .metrics
                .batch_latency_ms
                .lock()
                .map(|samples| samples.clone())
                .unwrap_or_default(),
            per_intent_verified_total: self
                .metrics
                .per_intent_verified_total
                .load(Ordering::SeqCst),
            failures_total: self.metrics.failures_total.load(Ordering::SeqCst),
        }
    }
}

fn verify_batch_job(
    engine: &Arc<EngineHandle>,
    batch: MahiSigverifyBatch,
    started_at: Instant,
    metrics: &SigverifyMetrics,
) -> MahiSigverifyResult {
    let materials = match engine.with_policy_read(|policy_store| {
        batch
            .intents
            .iter()
            .map(|intent| {
                let signed_session = signed_session_payload_for_intent(intent);
                policy_store
                    .session_verification_material(
                        intent.intent.policy_id,
                        &signed_session,
                        batch.expected_chain_id,
                    )
                    .map(|material| (material, Signature::from_bytes(&signed_session.signature)))
            })
            .collect::<Result<Vec<(SessionVerificationMaterial, Signature)>, PolicyError>>()
    }) {
        Ok(Ok(materials)) => materials,
        Ok(Err(error)) => {
            return failed_result(
                batch.block_ref,
                started_at,
                1,
                format!("{error:?}"),
                metrics,
            );
        }
        Err(_) => {
            return failed_result(
                batch.block_ref,
                started_at,
                1,
                "engine policy lock poisoned".into(),
                metrics,
            );
        }
    };

    let messages = materials
        .iter()
        .map(|(material, _)| material.signing_bytes.as_slice())
        .collect::<Vec<_>>();
    let signatures = materials
        .iter()
        .map(|(_, signature)| *signature)
        .collect::<Vec<_>>();
    let verifying_keys = match materials
        .iter()
        .map(|(material, _)| {
            VerifyingKey::from_bytes(&material.current_grantee)
                .map_err(|_| PolicyError::InvalidSessionKeyMaterial)
        })
        .collect::<Result<Vec<_>, PolicyError>>()
    {
        Ok(keys) => keys,
        Err(error) => {
            return failed_result(
                batch.block_ref,
                started_at,
                1,
                format!("{error:?}"),
                metrics,
            );
        }
    };

    if verify_batch(&messages, &signatures, &verifying_keys).is_ok() {
        let latency_ms = record_latency(metrics, started_at);
        metrics
            .per_intent_verified_total
            .fetch_add(batch.intents.len() as u64, Ordering::SeqCst);
        return MahiSigverifyResult::Verified {
            block_ref: batch.block_ref,
            verified_intents: batch.intents.len(),
            latency_ms,
        };
    }

    let fallback = match engine.with_policy_read(|policy_store| {
        batch
            .intents
            .iter()
            .map(|intent| {
                verify_intent_signature(
                    policy_store,
                    intent,
                    batch.expected_chain_id,
                    batch.block_number,
                )
            })
            .collect::<Vec<_>>()
    }) {
        Ok(results) => results,
        Err(_) => {
            return failed_result(
                batch.block_ref,
                started_at,
                batch.intents.len(),
                "engine policy lock poisoned".into(),
                metrics,
            );
        }
    };

    let failures = fallback.iter().filter(|result| result.is_err()).count();
    if failures == 0 {
        let latency_ms = record_latency(metrics, started_at);
        metrics
            .per_intent_verified_total
            .fetch_add(batch.intents.len() as u64, Ordering::SeqCst);
        return MahiSigverifyResult::Verified {
            block_ref: batch.block_ref,
            verified_intents: batch.intents.len(),
            latency_ms,
        };
    }

    let reason = fallback
        .into_iter()
        .find_map(|result| result.err())
        .map(|error| format!("{error:?}"))
        .unwrap_or_else(|| "unknown verification failure".into());
    failed_result(batch.block_ref, started_at, failures, reason, metrics)
}

fn failed_result(
    block_ref: MahiBlockRef,
    started_at: Instant,
    failures: usize,
    reason: String,
    metrics: &SigverifyMetrics,
) -> MahiSigverifyResult {
    metrics
        .failures_total
        .fetch_add(failures as u64, Ordering::SeqCst);
    let latency_ms = record_latency(metrics, started_at);
    MahiSigverifyResult::Failed {
        block_ref,
        failures,
        latency_ms,
        reason,
    }
}

fn record_latency(metrics: &SigverifyMetrics, started_at: Instant) -> u64 {
    let latency_ms = started_at.elapsed().as_millis().min(u64::MAX as u128) as u64;
    if let Ok(mut samples) = metrics.batch_latency_ms.lock() {
        if samples.len() >= MAX_SIGVERIFY_LATENCY_SAMPLES {
            let overflow = samples.len() + 1 - MAX_SIGVERIFY_LATENCY_SAMPLES;
            samples.drain(0..overflow);
        }
        samples.push(latency_ms);
    }
    latency_ms
}
