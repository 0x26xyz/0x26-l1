use crate::{MandateId, MandateState};
use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_state_db::{BackendError, Namespace, ReadTxn, WriteBatch};

const UTC_DAY_MS: u64 = 86_400_000;

pub const ACCOUNT_EQUITY_HISTORY_DENSE_WINDOW_MS: u64 = 30 * UTC_DAY_MS;
pub const ACCOUNT_EQUITY_HISTORY_DAILY_WINDOW_MS: u64 = 183 * UTC_DAY_MS;
pub const ACCOUNT_EQUITY_HISTORY_ACTIVE_TAIL_MS: u64 = UTC_DAY_MS;
pub const ACCOUNT_EQUITY_HISTORY_IDLE_SAMPLE_MS: u64 = 60_000;
pub const MANDATE_CONFORMANCE_HISTORY_LIMIT: usize = 128;
pub const ACCOUNT_EQUITY_HISTORY_KEY_LEN: usize = 16;
pub const MANDATE_CONFORMANCE_HISTORY_KEY_LEN: usize = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AccountEquitySample {
    pub block_height: u64,
    pub timestamp_ms: u64,
    pub total_equity: i128,
    pub total_collateral: i128,
    pub total_realized_pnl: i128,
    pub total_unrealized_pnl: i128,
    pub total_maintenance_requirement: u128,
    pub total_liquidation_gap: u128,
    pub total_margin_surplus: i128,
    pub position_count: u32,
    pub under_maintenance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MandateConformanceSample {
    pub mandate_id: MandateId,
    pub block_height: u64,
    pub evaluated_at_ms: u64,
    pub score: u8,
    pub pending_objectives: u16,
    pub state: MandateState,
    pub prev_score: Option<u8>,
    pub delta: Option<i16>,
}

pub fn equity_history_key(
    account_id: u64,
    block_height: u64,
) -> [u8; ACCOUNT_EQUITY_HISTORY_KEY_LEN] {
    let mut key = [0u8; ACCOUNT_EQUITY_HISTORY_KEY_LEN];
    key[..8].copy_from_slice(&account_id.to_be_bytes());
    key[8..].copy_from_slice(&block_height.to_be_bytes());
    key
}

pub fn equity_history_account_prefix(account_id: u64) -> [u8; 8] {
    account_id.to_be_bytes()
}

pub fn decode_equity_history_key(key: &[u8]) -> Result<(u64, u64), BackendError> {
    if key.len() != ACCOUNT_EQUITY_HISTORY_KEY_LEN {
        return Err(BackendError::Corrupt(format!(
            "equity history key has length {}, expected {}",
            key.len(),
            ACCOUNT_EQUITY_HISTORY_KEY_LEN
        )));
    }
    let mut account = [0u8; 8];
    let mut height = [0u8; 8];
    account.copy_from_slice(&key[..8]);
    height.copy_from_slice(&key[8..]);
    Ok((u64::from_be_bytes(account), u64::from_be_bytes(height)))
}

pub fn put_equity_sample<W: WriteBatch>(
    batch: &mut W,
    account_id: u64,
    sample: &AccountEquitySample,
) -> Result<(), BackendError> {
    let key = equity_history_key(account_id, sample.block_height);
    let value = borsh::to_vec(sample)
        .map_err(|e| BackendError::InvalidArgument(format!("equity sample encode: {e}")))?;
    batch.put(Namespace::EquityHistory, &key, &value);
    Ok(())
}

pub fn range_equity_samples_from_txn<R: ReadTxn>(
    txn: &R,
    account_id: u64,
    from_height: u64,
    before_height: u64,
) -> Result<Vec<AccountEquitySample>, BackendError> {
    if from_height >= before_height {
        return Ok(Vec::new());
    }
    let prefix = equity_history_account_prefix(account_id);
    let mut samples = Vec::new();
    for entry in txn.iter_prefix(Namespace::EquityHistory, &prefix)? {
        let (key, value) = entry?;
        let (row_account_id, block_height) = decode_equity_history_key(&key)?;
        if row_account_id != account_id {
            return Err(BackendError::Corrupt(format!(
                "equity history prefix returned account {row_account_id}, expected {account_id}"
            )));
        }
        if block_height < from_height {
            continue;
        }
        if block_height >= before_height {
            break;
        }
        let sample: AccountEquitySample = borsh::from_slice(&value)
            .map_err(|e| BackendError::Corrupt(format!("equity sample decode: {e}")))?;
        samples.push(sample);
    }
    Ok(samples)
}

pub fn all_equity_samples_from_txn<R: ReadTxn>(
    txn: &R,
) -> Result<Vec<(u64, AccountEquitySample)>, BackendError> {
    let mut samples = Vec::new();
    for entry in txn.iter_prefix(Namespace::EquityHistory, b"")? {
        let (key, value) = entry?;
        let (account_id, block_height) = decode_equity_history_key(&key)?;
        let sample: AccountEquitySample = borsh::from_slice(&value)
            .map_err(|e| BackendError::Corrupt(format!("equity sample decode: {e}")))?;
        if sample.block_height != block_height {
            return Err(BackendError::Corrupt(format!(
                "equity sample key height {block_height} != value height {}",
                sample.block_height
            )));
        }
        samples.push((account_id, sample));
    }
    Ok(samples)
}

pub fn mandate_conformance_history_key(
    mandate_id: MandateId,
    evaluated_at_ms: u64,
    block_height: u64,
) -> [u8; MANDATE_CONFORMANCE_HISTORY_KEY_LEN] {
    let mut key = [0u8; MANDATE_CONFORMANCE_HISTORY_KEY_LEN];
    key[..32].copy_from_slice(&mandate_id);
    key[32..40].copy_from_slice(&evaluated_at_ms.to_be_bytes());
    key[40..].copy_from_slice(&block_height.to_be_bytes());
    key
}

pub fn mandate_conformance_history_prefix(mandate_id: MandateId) -> [u8; 32] {
    mandate_id
}

pub fn decode_mandate_conformance_history_key(
    key: &[u8],
) -> Result<(MandateId, u64, u64), BackendError> {
    if key.len() != MANDATE_CONFORMANCE_HISTORY_KEY_LEN {
        return Err(BackendError::Corrupt(format!(
            "mandate conformance history key has length {}, expected {}",
            key.len(),
            MANDATE_CONFORMANCE_HISTORY_KEY_LEN
        )));
    }
    let mut mandate_id = [0u8; 32];
    let mut evaluated_at = [0u8; 8];
    let mut height = [0u8; 8];
    mandate_id.copy_from_slice(&key[..32]);
    evaluated_at.copy_from_slice(&key[32..40]);
    height.copy_from_slice(&key[40..]);
    Ok((
        mandate_id,
        u64::from_be_bytes(evaluated_at),
        u64::from_be_bytes(height),
    ))
}

pub fn put_mandate_conformance_sample<W: WriteBatch>(
    batch: &mut W,
    sample: &MandateConformanceSample,
) -> Result<(), BackendError> {
    let key = mandate_conformance_history_key(
        sample.mandate_id,
        sample.evaluated_at_ms,
        sample.block_height,
    );
    let value = borsh::to_vec(sample).map_err(|e| {
        BackendError::InvalidArgument(format!("mandate conformance sample encode: {e}"))
    })?;
    batch.put(Namespace::MandateConformanceHistory, &key, &value);
    Ok(())
}

pub fn range_mandate_conformance_samples_from_txn<R: ReadTxn>(
    txn: &R,
    mandate_id: MandateId,
) -> Result<Vec<MandateConformanceSample>, BackendError> {
    let prefix = mandate_conformance_history_prefix(mandate_id);
    let mut samples = Vec::new();
    for entry in txn.iter_prefix(Namespace::MandateConformanceHistory, &prefix)? {
        let (key, value) = entry?;
        let (row_mandate_id, evaluated_at_ms, block_height) =
            decode_mandate_conformance_history_key(&key)?;
        if row_mandate_id != mandate_id {
            return Err(BackendError::Corrupt(
                "mandate conformance prefix returned a mismatched mandate id".into(),
            ));
        }
        let sample: MandateConformanceSample = borsh::from_slice(&value).map_err(|e| {
            BackendError::Corrupt(format!("mandate conformance sample decode: {e}"))
        })?;
        if sample.mandate_id != row_mandate_id
            || sample.evaluated_at_ms != evaluated_at_ms
            || sample.block_height != block_height
        {
            return Err(BackendError::Corrupt(
                "mandate conformance sample key/value mismatch".into(),
            ));
        }
        samples.push(sample);
    }
    Ok(samples)
}

pub fn all_mandate_conformance_samples_from_txn<R: ReadTxn>(
    txn: &R,
) -> Result<Vec<(MandateId, MandateConformanceSample)>, BackendError> {
    let mut samples = Vec::new();
    for entry in txn.iter_prefix(Namespace::MandateConformanceHistory, b"")? {
        let (key, value) = entry?;
        let (mandate_id, evaluated_at_ms, block_height) =
            decode_mandate_conformance_history_key(&key)?;
        let sample: MandateConformanceSample = borsh::from_slice(&value).map_err(|e| {
            BackendError::Corrupt(format!("mandate conformance sample decode: {e}"))
        })?;
        if sample.mandate_id != mandate_id
            || sample.evaluated_at_ms != evaluated_at_ms
            || sample.block_height != block_height
        {
            return Err(BackendError::Corrupt(
                "mandate conformance sample key/value mismatch".into(),
            ));
        }
        samples.push((mandate_id, sample));
    }
    Ok(samples)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccountEquitySampleChangeKey {
    total_equity: i128,
    total_collateral: i128,
    total_maintenance_requirement: u128,
    position_count: u32,
    under_maintenance: bool,
}

impl From<&AccountEquitySample> for AccountEquitySampleChangeKey {
    fn from(sample: &AccountEquitySample) -> Self {
        Self {
            total_equity: sample.total_equity,
            total_collateral: sample.total_collateral,
            total_maintenance_requirement: sample.total_maintenance_requirement,
            position_count: sample.position_count,
            under_maintenance: sample.under_maintenance,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MandateConformanceSampleChangeKey {
    pub(crate) score: u8,
    pub(crate) pending_objectives: u16,
    pub(crate) state: MandateState,
}

impl From<&MandateConformanceSample> for MandateConformanceSampleChangeKey {
    fn from(sample: &MandateConformanceSample) -> Self {
        Self {
            score: sample.score,
            pending_objectives: sample.pending_objectives,
            state: sample.state,
        }
    }
}
