use crate::codec::decode_hex_array;
use crate::RpcError;
use kombat_chain_consensus::Hash;
use kombat_chain_execution::{decode_raw_evm_transaction, evm_usdc_system_address_bytes};
use revm::primitives::U256;
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const DEFAULT_FAUCET_POLICY_USDC_MICRO: u64 = 100_000_000;
pub(crate) const DEFAULT_FAUCET_EVM_USDC_MICRO: u64 = 25_000_000;
pub(crate) const MAX_FAUCET_POLICY_USDC_MICRO: u64 = 1_000_000_000_000;
pub(crate) const MAX_FAUCET_EVM_USDC_MICRO: u64 = 250_000_000;

const EVM_USDC_DEPOSIT_FOR_SELECTOR: [u8; 4] = [0xd7, 0x41, 0x30, 0xe2];
const FAUCET_REDIS_URL_ENV: &str = "KOMBAT_FAUCET_REDIS_URL";
pub(crate) const FAUCET_OPERATOR_PRIVATE_KEY_ENV: &str = "KOMBAT_FAUCET_OPERATOR_PRIVATE_KEY";
const FAUCET_ADDRESS_COOLDOWN_SECS_ENV: &str = "KOMBAT_FAUCET_ADDRESS_COOLDOWN_SECS";
const FAUCET_IP_WINDOW_SECS_ENV: &str = "KOMBAT_FAUCET_IP_WINDOW_SECS";
const FAUCET_IP_MAX_CLAIMS_ENV: &str = "KOMBAT_FAUCET_IP_MAX_CLAIMS";
const FAUCET_GLOBAL_WINDOW_SECS_ENV: &str = "KOMBAT_FAUCET_GLOBAL_WINDOW_SECS";
const FAUCET_GLOBAL_MAX_CLAIMS_ENV: &str = "KOMBAT_FAUCET_GLOBAL_MAX_CLAIMS";
const DEFAULT_FAUCET_ADDRESS_COOLDOWN_SECS: u64 = 24 * 60 * 60;
const DEFAULT_FAUCET_IP_WINDOW_SECS: u64 = 60 * 60;
const DEFAULT_FAUCET_IP_MAX_CLAIMS: u64 = 20;
const DEFAULT_FAUCET_GLOBAL_WINDOW_SECS: u64 = 60;
const DEFAULT_FAUCET_GLOBAL_MAX_CLAIMS: u64 = 1_000;

#[derive(Clone, Debug)]
pub(crate) struct FaucetLimiter {
    pub(crate) mode: FaucetLimiterMode,
    pub(crate) address_cooldown_secs: u64,
    pub(crate) ip_window_secs: u64,
    pub(crate) ip_max_claims: u64,
    pub(crate) global_window_secs: u64,
    pub(crate) global_max_claims: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum FaucetLimiterMode {
    InMemory(Arc<Mutex<InMemoryFaucetLimiterState>>),
    Redis(redis::Client),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct InMemoryFaucetLimiterState {
    address_until: HashMap<String, u64>,
    ip_windows: HashMap<String, FaucetWindow>,
    global_window: FaucetWindow,
}

#[derive(Clone, Copy, Debug, Default)]
struct FaucetWindow {
    start_secs: u64,
    count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FaucetOperatorSigner {
    private_key: [u8; 32],
    pub(crate) address: [u8; 20],
}

impl FaucetOperatorSigner {
    pub(crate) fn from_env() -> Result<Self, RpcError> {
        let private_key_hex = env::var(FAUCET_OPERATOR_PRIVATE_KEY_ENV).map_err(|_| {
            RpcError::Node(format!("{FAUCET_OPERATOR_PRIVATE_KEY_ENV} must be set"))
        })?;
        Self::from_private_key_hex(&private_key_hex)
    }

    fn from_private_key_hex(private_key_hex: &str) -> Result<Self, RpcError> {
        use alloy_consensus::crypto::secp256k1::public_key_to_address;
        use secp256k1::{PublicKey, Secp256k1, SecretKey};

        let private_key = decode_hex_array::<32>(private_key_hex).map_err(|message| {
            RpcError::Node(format!(
                "invalid {FAUCET_OPERATOR_PRIVATE_KEY_ENV}: {message}"
            ))
        })?;
        let secret = SecretKey::from_slice(&private_key).map_err(|error| {
            RpcError::Node(format!(
                "invalid {FAUCET_OPERATOR_PRIVATE_KEY_ENV}: {error}"
            ))
        })?;
        let public_key = PublicKey::from_secret_key(&Secp256k1::new(), &secret);
        Ok(Self {
            private_key,
            address: public_key_to_address(public_key).into_array(),
        })
    }
}

fn encode_evm_usdc_deposit_for_calldata(target: [u8; 20], amount: U256) -> Vec<u8> {
    let mut calldata = Vec::with_capacity(68);
    calldata.extend_from_slice(&EVM_USDC_DEPOSIT_FOR_SELECTOR);
    calldata.extend_from_slice(&[0_u8; 12]);
    calldata.extend_from_slice(&target);
    calldata.extend_from_slice(&amount.to_be_bytes::<32>());
    calldata
}

pub(crate) fn sign_evm_usdc_deposit_for_tx(
    chain_id: u64,
    nonce: u64,
    signer: FaucetOperatorSigner,
    target: [u8; 20],
    amount: U256,
) -> Result<(Hash, Vec<u8>), RpcError> {
    use alloy_consensus::{
        crypto::secp256k1::sign_message, SignableTransaction, TxEip1559, TxEnvelope,
    };
    use alloy_eips::Encodable2718;
    use alloy_primitives::{Bytes as AlloyBytes, TxKind, B256, U256 as AlloyU256};

    let calldata = encode_evm_usdc_deposit_for_calldata(target, amount);
    let tx = TxEip1559 {
        chain_id,
        nonce,
        gas_limit: 200_000,
        max_fee_per_gas: 0,
        max_priority_fee_per_gas: 0,
        to: TxKind::Call(evm_usdc_system_address_bytes().into()),
        value: AlloyU256::ZERO,
        access_list: Default::default(),
        input: AlloyBytes::from(calldata),
    };
    let signature = sign_message(B256::from_slice(&signer.private_key), tx.signature_hash())
        .map_err(|error| RpcError::Node(format!("failed to sign faucet deposit tx: {error}")))?;
    let envelope: TxEnvelope = tx.into_signed(signature).into();
    let raw_tx = envelope.encoded_2718();
    let decoded = decode_raw_evm_transaction(&raw_tx)
        .map_err(|message| RpcError::Node(format!("signed faucet tx did not decode: {message}")))?;
    if decoded.from != signer.address {
        return Err(RpcError::Node(
            "signed faucet tx recovered unexpected operator address".into(),
        ));
    }
    Ok((decoded.tx_hash, raw_tx))
}

impl FaucetLimiter {
    pub(crate) fn from_env() -> Self {
        let address_cooldown_secs = parse_env_u64_with_default(
            FAUCET_ADDRESS_COOLDOWN_SECS_ENV,
            DEFAULT_FAUCET_ADDRESS_COOLDOWN_SECS,
        );
        let ip_window_secs =
            parse_env_u64_with_default(FAUCET_IP_WINDOW_SECS_ENV, DEFAULT_FAUCET_IP_WINDOW_SECS);
        let ip_max_claims =
            parse_env_u64_with_default(FAUCET_IP_MAX_CLAIMS_ENV, DEFAULT_FAUCET_IP_MAX_CLAIMS);
        let global_window_secs = parse_env_u64_with_default(
            FAUCET_GLOBAL_WINDOW_SECS_ENV,
            DEFAULT_FAUCET_GLOBAL_WINDOW_SECS,
        );
        let global_max_claims = parse_env_u64_with_default(
            FAUCET_GLOBAL_MAX_CLAIMS_ENV,
            DEFAULT_FAUCET_GLOBAL_MAX_CLAIMS,
        );
        let mode = env::var(FAUCET_REDIS_URL_ENV)
            .ok()
            .map(|url| url.trim().to_owned())
            .filter(|url| !url.is_empty())
            .map(|url| {
                redis::Client::open(url)
                    .unwrap_or_else(|error| panic!("invalid {FAUCET_REDIS_URL_ENV}: {error}"))
            })
            .map(FaucetLimiterMode::Redis)
            .unwrap_or_else(|| {
                FaucetLimiterMode::InMemory(Arc::new(Mutex::new(
                    InMemoryFaucetLimiterState::default(),
                )))
            });
        Self {
            mode,
            address_cooldown_secs,
            ip_window_secs,
            ip_max_claims,
            global_window_secs,
            global_max_claims,
        }
    }

    #[cfg(test)]
    pub(crate) fn in_memory_for_tests(
        address_cooldown_secs: u64,
        ip_window_secs: u64,
        ip_max_claims: u64,
        global_window_secs: u64,
        global_max_claims: u64,
    ) -> Self {
        Self {
            mode: FaucetLimiterMode::InMemory(Arc::new(Mutex::new(
                InMemoryFaucetLimiterState::default(),
            ))),
            address_cooldown_secs,
            ip_window_secs,
            ip_max_claims,
            global_window_secs,
            global_max_claims,
        }
    }

    pub(crate) fn check_and_record(
        &self,
        client_id: &str,
        address_hex: &str,
    ) -> Result<(), String> {
        match &self.mode {
            FaucetLimiterMode::InMemory(state) => {
                let mut state = state
                    .lock()
                    .map_err(|_| "faucet limiter lock poisoned".to_string())?;
                state.check_and_record(
                    current_unix_secs(),
                    client_id,
                    address_hex,
                    self.address_cooldown_secs,
                    self.ip_window_secs,
                    self.ip_max_claims,
                    self.global_window_secs,
                    self.global_max_claims,
                )
            }
            FaucetLimiterMode::Redis(client) => {
                self.check_and_record_redis(client, client_id, address_hex)
            }
        }
    }

    fn check_and_record_redis(
        &self,
        client: &redis::Client,
        client_id: &str,
        address_hex: &str,
    ) -> Result<(), String> {
        let mut connection = client
            .get_connection()
            .map_err(|error| format!("faucet redis unavailable: {error}"))?;
        let script = r#"
local address_key = KEYS[1]
local ip_key = KEYS[2]
local global_key = KEYS[3]
if redis.call("EXISTS", address_key) == 1 then
  return {0, "address cooldown"}
end
local ip_count = redis.call("INCR", ip_key)
if ip_count == 1 then
  redis.call("EXPIRE", ip_key, tonumber(ARGV[2]))
end
if ip_count > tonumber(ARGV[3]) then
  return {0, "ip rate limit"}
end
local global_count = redis.call("INCR", global_key)
if global_count == 1 then
  redis.call("EXPIRE", global_key, tonumber(ARGV[4]))
end
if global_count > tonumber(ARGV[5]) then
  return {0, "global rate limit"}
end
redis.call("SET", address_key, "1", "EX", tonumber(ARGV[1]))
return {1, "ok"}
"#;
        let address_key = format!("faucet:addr:{address_hex}");
        let ip_key = format!("faucet:ip:{client_id}");
        let global_key = "faucet:global".to_string();
        let (allowed, reason): (i64, String) = redis::cmd("EVAL")
            .arg(script)
            .arg(3)
            .arg(address_key)
            .arg(ip_key)
            .arg(global_key)
            .arg(self.address_cooldown_secs)
            .arg(self.ip_window_secs)
            .arg(self.ip_max_claims)
            .arg(self.global_window_secs)
            .arg(self.global_max_claims)
            .query(&mut connection)
            .map_err(|error| format!("faucet redis check failed: {error}"))?;
        if allowed == 1 {
            Ok(())
        } else {
            Err(reason)
        }
    }
}

impl InMemoryFaucetLimiterState {
    fn check_and_record(
        &mut self,
        now_secs: u64,
        client_id: &str,
        address_hex: &str,
        address_cooldown_secs: u64,
        ip_window_secs: u64,
        ip_max_claims: u64,
        global_window_secs: u64,
        global_max_claims: u64,
    ) -> Result<(), String> {
        self.address_until
            .retain(|_, until_secs| *until_secs > now_secs);
        if self.address_until.contains_key(address_hex) {
            return Err("address cooldown".into());
        }
        check_faucet_window(
            &mut self.global_window,
            now_secs,
            global_window_secs,
            global_max_claims,
            "global rate limit",
        )?;
        let window = self.ip_windows.entry(client_id.to_owned()).or_default();
        check_faucet_window(
            window,
            now_secs,
            ip_window_secs,
            ip_max_claims,
            "ip rate limit",
        )?;
        self.address_until.insert(
            address_hex.to_owned(),
            now_secs.saturating_add(address_cooldown_secs),
        );
        Ok(())
    }
}

fn check_faucet_window(
    window: &mut FaucetWindow,
    now_secs: u64,
    window_secs: u64,
    max_claims: u64,
    limit_message: &'static str,
) -> Result<(), String> {
    if window.start_secs == 0 || now_secs >= window.start_secs.saturating_add(window_secs) {
        window.start_secs = now_secs;
        window.count = 0;
    }
    if window.count >= max_claims {
        return Err(limit_message.into());
    }
    window.count = window.count.saturating_add(1);
    Ok(())
}

fn parse_env_u64_with_default(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}
