use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub(crate) struct RpcSubmitPathMetrics {
    pub(crate) local_submit_total: u64,
    pub(crate) local_submit_ok: u64,
    pub(crate) local_submit_err: u64,
    pub(crate) forwarded_submit_total: u64,
    pub(crate) forwarded_submit_ok: u64,
    pub(crate) forwarded_submit_err: u64,
    pub(crate) timeout_recovery_total: u64,
    pub(crate) timeout_recovery_ok: u64,
    pub(crate) timeout_recovery_err: u64,
    pub(crate) timeout_recovery_attempts_on_forwarded_submit: u64,
    pub(crate) local_submit_ms: Vec<u64>,
    pub(crate) forwarded_submit_ms: Vec<u64>,
    pub(crate) leader_preflight_ms: Vec<u64>,
    pub(crate) leader_forward_ms: Vec<u64>,
    pub(crate) timeout_recovery_ms: Vec<u64>,
    pub(crate) forwarded_timeout_recovery_ms: Vec<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NetworkRoundLockProfileSnapshot {
    pub(crate) head_open_wait_ns: Vec<u64>,
    pub(crate) head_open_hold_ns: Vec<u64>,
    pub(crate) build_wait_ns: Vec<u64>,
    pub(crate) build_hold_ns: Vec<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NetworkRoundLockProfilePhase {
    HeadOpen,
    Build,
}

pub(crate) fn push_duration_sample(samples: &mut Vec<u64>, duration: Duration) {
    const MAX_SAMPLES: usize = 128;

    if samples.len() >= MAX_SAMPLES {
        samples.remove(0);
    }
    samples.push(duration.as_millis().min(u64::MAX as u128) as u64);
}

pub(crate) fn push_u64_sample(samples: &mut Vec<u64>, value: u64) {
    const MAX_SAMPLES: usize = 128;

    if samples.len() >= MAX_SAMPLES {
        samples.remove(0);
    }
    samples.push(value);
}

pub(crate) fn duration_as_nanos_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

pub(crate) fn duration_summary_to_json(samples: &[u64]) -> Value {
    if samples.is_empty() {
        return json!({
            "count": 0,
            "last_ms": Value::Null,
            "p50_ms": Value::Null,
            "p90_ms": Value::Null,
            "max_ms": Value::Null,
        });
    }

    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let percentile = |p: f64| -> u64 {
        let idx = ((sorted.len() as f64) * p).ceil() as usize;
        let idx = idx.saturating_sub(1).min(sorted.len() - 1);
        sorted[idx]
    };

    json!({
        "count": samples.len(),
        "last_ms": samples.last().copied(),
        "p50_ms": percentile(0.50),
        "p90_ms": percentile(0.90),
        "max_ms": sorted.last().copied(),
    })
}

pub(crate) fn sample_summary_to_json(samples: &[u64]) -> Value {
    if samples.is_empty() {
        return json!({
            "count": 0,
            "last": Value::Null,
            "p50": Value::Null,
            "p90": Value::Null,
            "max": Value::Null,
        });
    }

    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let percentile = |p: f64| -> u64 {
        let idx = ((sorted.len() as f64) * p).ceil() as usize;
        let idx = idx.saturating_sub(1).min(sorted.len() - 1);
        sorted[idx]
    };

    json!({
        "count": samples.len(),
        "last": samples.last().copied(),
        "p50": percentile(0.50),
        "p90": percentile(0.90),
        "max": sorted.last().copied(),
    })
}

pub(crate) fn nanos_summary_to_json(samples: &[u64]) -> Value {
    if samples.is_empty() {
        return json!({
            "count": 0,
            "last_ns": Value::Null,
            "p50_ns": Value::Null,
            "p95_ns": Value::Null,
            "p99_ns": Value::Null,
            "max_ns": Value::Null,
        });
    }

    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let percentile = |p: f64| -> u64 {
        let idx = ((sorted.len() as f64) * p).ceil() as usize;
        let idx = idx.saturating_sub(1).min(sorted.len() - 1);
        sorted[idx]
    };

    json!({
        "count": samples.len(),
        "last_ns": samples.last().copied(),
        "p50_ns": percentile(0.50),
        "p95_ns": percentile(0.95),
        "p99_ns": percentile(0.99),
        "max_ns": sorted.last().copied(),
    })
}

pub(crate) fn lock_timing_summary_to_json(samples: &kombat_chain_node::LockTimingSamples) -> Value {
    json!({
        "wait_ns": nanos_summary_to_json(&samples.wait_ns),
        "hold_ns": nanos_summary_to_json(&samples.hold_ns),
    })
}
