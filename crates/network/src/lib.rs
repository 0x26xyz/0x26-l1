use borsh::{BorshDeserialize, BorshSerialize};
use kombat_chain_consensus::ValidatorId;
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashSet},
    fmt, fs,
    path::Path,
};

pub const CRATE_NAME: &str = "network";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorConfig {
    pub validator_id: ValidatorId,
    pub consensus_pubkey: HexKey,
    pub network_pubkey: HexKey,
    pub rpc_endpoints: Vec<String>,
    pub region: String,
    pub operator_name: String,
    pub bond: BondRecord,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BondRecord {
    pub asset: BondAsset,
    pub amount: u64,
    pub status: BondStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BondAsset {
    Usdc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BondStatus {
    Pending,
    Active,
    Slashed,
    Returned,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorSet {
    pub epoch: u64,
    pub validators: Vec<ValidatorConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BootstrapPeers {
    pub local: ValidatorConfig,
    pub peers: Vec<ValidatorPeer>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorPeer {
    pub validator_id: ValidatorId,
    pub network_pubkey: HexKey,
    pub rpc_endpoints: Vec<String>,
    pub region: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct HexKey(Vec<u8>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    ParseToml(String),
    InvalidHex { field: &'static str, value: String },
    InvalidValidatorIdLength { value: String },
    DuplicateValidatorId { validator_id: ValidatorId },
    DuplicateConsensusKey,
    DuplicateNetworkKey,
    EmptyValidatorSet,
    LocalValidatorMissing { validator_id: ValidatorId },
    Io(String),
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

pub fn load_validator_set_from_path(path: impl AsRef<Path>) -> Result<ValidatorSet, ConfigError> {
    let contents = fs::read_to_string(path).map_err(|err| ConfigError::Io(err.to_string()))?;
    load_validator_set_from_str(&contents)
}

pub fn load_validator_set_from_str(input: &str) -> Result<ValidatorSet, ConfigError> {
    let raw: RawValidatorSet =
        toml::from_str(input).map_err(|err| ConfigError::ParseToml(err.to_string()))?;
    ValidatorSet::from_raw(raw)
}

pub fn bootstrap_validator(
    validator_set: &ValidatorSet,
    local_validator_id: ValidatorId,
) -> Result<BootstrapPeers, ConfigError> {
    let local = validator_set
        .validators
        .iter()
        .find(|validator| validator.validator_id == local_validator_id)
        .cloned()
        .ok_or(ConfigError::LocalValidatorMissing {
            validator_id: local_validator_id,
        })?;

    let peers = validator_set
        .active_validators()
        .iter()
        .filter(|validator| validator.validator_id != local_validator_id)
        .map(ValidatorPeer::from)
        .collect();

    Ok(BootstrapPeers { local, peers })
}

impl ValidatorSet {
    pub fn active_validators(&self) -> Vec<ValidatorConfig> {
        self.validators
            .iter()
            .filter(|validator| validator.enabled)
            .cloned()
            .collect()
    }

    fn from_raw(raw: RawValidatorSet) -> Result<Self, ConfigError> {
        let mut validators: Vec<_> = raw
            .validators
            .into_iter()
            .map(ValidatorConfig::try_from)
            .collect::<Result<_, _>>()?;

        if validators.is_empty() {
            return Err(ConfigError::EmptyValidatorSet);
        }

        validators.sort_by_key(|validator| validator.validator_id);

        let mut validator_ids = BTreeSet::new();
        let mut consensus_keys = HashSet::new();
        let mut network_keys = HashSet::new();

        for validator in &validators {
            if !validator_ids.insert(validator.validator_id) {
                return Err(ConfigError::DuplicateValidatorId {
                    validator_id: validator.validator_id,
                });
            }

            if !consensus_keys.insert(validator.consensus_pubkey.clone()) {
                return Err(ConfigError::DuplicateConsensusKey);
            }

            if !network_keys.insert(validator.network_pubkey.clone()) {
                return Err(ConfigError::DuplicateNetworkKey);
            }
        }

        Ok(Self {
            epoch: raw.epoch,
            validators,
        })
    }
}

impl From<&ValidatorConfig> for ValidatorPeer {
    fn from(value: &ValidatorConfig) -> Self {
        Self {
            validator_id: value.validator_id,
            network_pubkey: value.network_pubkey.clone(),
            rpc_endpoints: value.rpc_endpoints.clone(),
            region: value.region.clone(),
        }
    }
}

impl TryFrom<RawValidatorConfig> for ValidatorConfig {
    type Error = ConfigError;

    fn try_from(value: RawValidatorConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            validator_id: parse_validator_id(&value.validator_id)?,
            consensus_pubkey: parse_hex_key("consensus_pubkey", &value.consensus_pubkey)?,
            network_pubkey: parse_hex_key("network_pubkey", &value.network_pubkey)?,
            rpc_endpoints: value.rpc_endpoints,
            region: value.region,
            operator_name: value.operator_name,
            bond: BondRecord::try_from(value.bond)?,
            enabled: value.enabled,
        })
    }
}

impl TryFrom<RawBondRecord> for BondRecord {
    type Error = ConfigError;

    fn try_from(value: RawBondRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            asset: match value.asset.as_str() {
                "USDC" | "Usdc" | "usdc" => BondAsset::Usdc,
                _ => {
                    return Err(ConfigError::ParseToml(format!(
                        "unsupported bond asset {}",
                        value.asset
                    )))
                }
            },
            amount: value.amount,
            status: match value.status.as_str() {
                "Pending" | "pending" => BondStatus::Pending,
                "Active" | "active" => BondStatus::Active,
                "Slashed" | "slashed" => BondStatus::Slashed,
                "Returned" | "returned" => BondStatus::Returned,
                _ => {
                    return Err(ConfigError::ParseToml(format!(
                        "unsupported bond status {}",
                        value.status
                    )))
                }
            },
        })
    }
}

impl fmt::Display for HexKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x")?;
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl HexKey {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn parse_validator_id(input: &str) -> Result<ValidatorId, ConfigError> {
    let bytes = decode_hex(input, "validator_id")?;
    if bytes.len() != 32 {
        return Err(ConfigError::InvalidValidatorIdLength {
            value: input.to_owned(),
        });
    }

    let mut validator_id = [0_u8; 32];
    validator_id.copy_from_slice(&bytes);
    Ok(validator_id)
}

fn parse_hex_key(field: &'static str, input: &str) -> Result<HexKey, ConfigError> {
    decode_hex(input, field).map(HexKey)
}

fn decode_hex(input: &str, field: &'static str) -> Result<Vec<u8>, ConfigError> {
    let trimmed = input.trim();
    let normalized = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if !normalized.len().is_multiple_of(2) {
        return Err(ConfigError::InvalidHex {
            field,
            value: trimmed.to_owned(),
        });
    }

    let mut bytes = Vec::with_capacity(normalized.len() / 2);
    let mut chars = normalized.as_bytes().chunks_exact(2);

    for chunk in &mut chars {
        let pair = std::str::from_utf8(chunk).map_err(|_| ConfigError::InvalidHex {
            field,
            value: trimmed.to_owned(),
        })?;
        let byte = u8::from_str_radix(pair, 16).map_err(|_| ConfigError::InvalidHex {
            field,
            value: trimmed.to_owned(),
        })?;
        bytes.push(byte);
    }

    Ok(bytes)
}

#[derive(Debug, Deserialize)]
struct RawValidatorSet {
    epoch: u64,
    validators: Vec<RawValidatorConfig>,
}

#[derive(Debug, Deserialize)]
struct RawValidatorConfig {
    validator_id: String,
    consensus_pubkey: String,
    network_pubkey: String,
    rpc_endpoints: Vec<String>,
    region: String,
    operator_name: String,
    bond: RawBondRecord,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct RawBondRecord {
    asset: String,
    amount: u64,
    status: String,
}

#[cfg(test)]
mod tests {
    use super::{
        bootstrap_validator, component_name, load_validator_set_from_str, ConfigError, ValidatorSet,
    };

    fn sample_config() -> &'static str {
        r#"
epoch = 7

[[validators]]
validator_id = "0x0303030303030303030303030303030303030303030303030303030303030303"
consensus_pubkey = "0xcccc"
network_pubkey = "0xdddd"
rpc_endpoints = ["https://validator-3.example.com"]
region = "eu-west"
operator_name = "validator-three"
enabled = true

[validators.bond]
asset = "USDC"
amount = 1000
status = "Active"

[[validators]]
validator_id = "0x0101010101010101010101010101010101010101010101010101010101010101"
consensus_pubkey = "0xaaaa"
network_pubkey = "0xbbbb"
rpc_endpoints = ["https://validator-1.example.com"]
region = "us-east"
operator_name = "validator-one"
enabled = true

[validators.bond]
asset = "USDC"
amount = 1000
status = "Active"

[[validators]]
validator_id = "0x0202020202020202020202020202020202020202020202020202020202020202"
consensus_pubkey = "0xeeee"
network_pubkey = "0xffff"
rpc_endpoints = ["https://validator-2.example.com"]
region = "ap-south"
operator_name = "validator-two"
enabled = false

[validators.bond]
asset = "USDC"
amount = 1000
status = "Pending"
"#
    }

    fn validator_id(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn reports_component_name() {
        assert_eq!(component_name(), "network");
    }

    #[test]
    fn loads_validator_set_and_sorts_canonically() {
        let validator_set = load_validator_set_from_str(sample_config()).expect("validator set");

        assert_eq!(validator_set.epoch, 7);
        assert_eq!(validator_set.validators[0].validator_id, validator_id(1));
        assert_eq!(validator_set.validators[1].validator_id, validator_id(2));
        assert_eq!(validator_set.validators[2].validator_id, validator_id(3));
    }

    #[test]
    fn filters_active_validators() {
        let validator_set = load_validator_set_from_str(sample_config()).expect("validator set");
        let active = validator_set.active_validators();

        assert_eq!(active.len(), 2);
        assert!(active.iter().all(|validator| validator.enabled));
    }

    #[test]
    fn rejects_duplicate_validator_ids() {
        let config = sample_config().replace(
            "0x0202020202020202020202020202020202020202020202020202020202020202",
            "0x0101010101010101010101010101010101010101010101010101010101010101",
        );

        assert_eq!(
            load_validator_set_from_str(&config),
            Err(ConfigError::DuplicateValidatorId {
                validator_id: validator_id(1),
            })
        );
    }

    #[test]
    fn rejects_duplicate_keys() {
        let config = sample_config().replace("0xeeee", "0xaaaa");

        assert_eq!(
            load_validator_set_from_str(&config),
            Err(ConfigError::DuplicateConsensusKey)
        );
    }

    #[test]
    fn bootstraps_local_validator_and_active_peers() {
        let validator_set = load_validator_set_from_str(sample_config()).expect("validator set");
        let bootstrap = bootstrap_validator(&validator_set, validator_id(1)).expect("bootstrap");

        assert_eq!(bootstrap.local.validator_id, validator_id(1));
        assert_eq!(bootstrap.peers.len(), 1);
        assert_eq!(bootstrap.peers[0].validator_id, validator_id(3));
    }

    #[test]
    fn fails_when_local_validator_is_missing() {
        let validator_set = load_validator_set_from_str(sample_config()).expect("validator set");

        assert_eq!(
            bootstrap_validator(&validator_set, validator_id(9)),
            Err(ConfigError::LocalValidatorMissing {
                validator_id: validator_id(9),
            })
        );
    }

    #[test]
    fn rejects_invalid_validator_id_length() {
        let config = sample_config().replace(
            "0x0101010101010101010101010101010101010101010101010101010101010101",
            "0x0101",
        );

        assert_eq!(
            load_validator_set_from_str(&config),
            Err(ConfigError::InvalidValidatorIdLength {
                value: "0x0101".to_owned(),
            })
        );
    }

    #[test]
    fn validator_set_type_is_exposed() {
        let validator_set = load_validator_set_from_str(sample_config()).expect("validator set");
        let _: ValidatorSet = validator_set;
    }
}
