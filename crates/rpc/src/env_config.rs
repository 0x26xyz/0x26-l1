use kombat_chain_consensus::mahi::CommitRule;
use std::env;
use std::sync::OnceLock;

pub(crate) const MAHI_MERGED_EXECUTION_BATCH_ENV: &str = "KOMBAT_MAHI_MERGED_EXECUTION_BATCH";
pub(crate) const MAHI_OBSERVER_ASYNC_RELAY_ENV: &str = "KOMBAT_MAHI_OBSERVER_ASYNC_RELAY";
pub(crate) const MAHI_ALLOW_IDLE_EMPTY_ROUNDS_ENV: &str = "KOMBAT_MAHI_ALLOW_IDLE_EMPTY_ROUNDS";
pub(crate) const MAHI_ASYNC_BLOCK_BROADCAST_ENV: &str = "KOMBAT_MAHI_ASYNC_BLOCK_BROADCAST";
pub(crate) const MAHI_PIPELINE_ENV: &str = "KOMBAT_MAHI_PIPELINE";
pub(crate) const MAHI_MULTILEADER_ENV: &str = "KOMBAT_MAHI_MULTILEADER";
pub(crate) const MAHI_OWNER_RELAY_REDUNDANCY_ENV: &str = "KOMBAT_MAHI_OWNER_RELAY_REDUNDANCY";
pub(crate) const MAHI_OWNER_RELAY_LAG_ROUNDS_ENV: &str = "KOMBAT_MAHI_OWNER_RELAY_LAG_ROUNDS";
pub(crate) const SIGNED_DELEGATION_GOSSIP_ENV: &str = "KOMBAT_SIGNED_DELEGATION_GOSSIP";
pub(crate) const SIGNED_MARKET_MAKER_PROGRAM_GOSSIP_ENV: &str =
    "KOMBAT_SIGNED_MARKET_MAKER_PROGRAM_GOSSIP";
pub(crate) const LEGACY_DELEGATION_BRIDGE_ENABLED_ENV: &str =
    "KOMBAT_LEGACY_DELEGATION_BRIDGE_ENABLED";
pub(crate) const LOCAL_BRIDGE_MUTATIONS_ENABLED_ENV: &str = "KOMBAT_LOCAL_BRIDGE_MUTATIONS_ENABLED";
pub(crate) const PHASE6_SCHEDULE_PROFILE_ENV: &str = "KOMBAT_PHASE6_SCHEDULE_PROFILE";
pub(crate) const PHASE6_SCHEDULE_PROFILE_SAMPLE_EVERY_ENV: &str =
    "KOMBAT_PHASE6_SCHEDULE_PROFILE_SAMPLE_EVERY";
pub(crate) const PHASE6_SCHEDULE_PROFILE_MAX_ITEMS_ENV: &str =
    "KOMBAT_PHASE6_SCHEDULE_PROFILE_MAX_ITEMS";
pub(crate) const DEFAULT_PHASE6_SCHEDULE_PROFILE_SAMPLE_EVERY: u64 = 32;
pub(crate) const DEFAULT_PHASE6_SCHEDULE_PROFILE_MAX_ITEMS: usize = 512;
pub(crate) const DEFAULT_MAHI_MERGED_EXECUTION_BATCH_SIZE: usize = 8;
const DEFAULT_MAHI_MULTILEADER_COUNT: usize = 4;
pub(crate) const DEFAULT_MAHI_OWNER_RELAY_LAG_ROUNDS: u64 = 256;
pub(crate) const NETWORK_SUBMIT_FORWARDING_ENV: &str = "KOMBAT_NETWORK_SUBMIT_FORWARDING_ENABLED";

pub(crate) fn parse_env_bool_with_default(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

pub(crate) fn mahi_allow_idle_empty_rounds() -> bool {
    #[cfg(test)]
    let default = true;
    #[cfg(not(test))]
    let default = false;
    parse_env_bool_with_default(MAHI_ALLOW_IDLE_EMPTY_ROUNDS_ENV, default)
}

pub fn parse_mahi_commit_rule() -> CommitRule {
    match env::var("KOMBAT_MAHI_COMMIT_RULE")
        .unwrap_or_else(|_| "mahi".into())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "mysticeti" => CommitRule::Mysticeti,
        _ => CommitRule::Mahi,
    }
}

pub(crate) fn parse_optional_env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.trim().parse().ok()
}

pub(crate) fn parse_env_u64_with_default(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(crate) fn parse_env_usize_with_default(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(crate) fn parse_mahi_merged_execution_batch_size() -> usize {
    env::var(MAHI_MERGED_EXECUTION_BATCH_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAHI_MERGED_EXECUTION_BATCH_SIZE)
}

pub(crate) fn parse_mahi_owner_relay_redundancy(validator_count: usize) -> usize {
    parse_env_usize_with_default(MAHI_OWNER_RELAY_REDUNDANCY_ENV, 1)
        .max(1)
        .min(validator_count.max(1))
}

pub(crate) fn parse_mahi_owner_relay_lag_rounds() -> u64 {
    parse_env_u64_with_default(
        MAHI_OWNER_RELAY_LAG_ROUNDS_ENV,
        DEFAULT_MAHI_OWNER_RELAY_LAG_ROUNDS,
    )
}

pub(crate) fn parse_mahi_pipeline() -> bool {
    parse_env_bool_with_default(MAHI_PIPELINE_ENV, false)
}

pub(crate) fn parse_mahi_number_of_leaders(validator_count: usize) -> usize {
    let validator_count = validator_count.max(1);
    let configured = env::var(MAHI_MULTILEADER_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "" | "0" | "false" | "no" | "off" => Some(1),
            "1" | "true" | "yes" | "on" => Some(DEFAULT_MAHI_MULTILEADER_COUNT),
            _ => value.parse::<usize>().ok(),
        })
        .unwrap_or(1);
    configured.max(1).min(validator_count)
}

pub(crate) fn network_submit_forwarding_enabled() -> bool {
    env::var(NETWORK_SUBMIT_FORWARDING_ENV)
        .ok()
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => true,
        })
        .unwrap_or(true)
}

pub(crate) fn signed_delegation_gossip_enabled() -> bool {
    parse_env_bool_with_default(SIGNED_DELEGATION_GOSSIP_ENV, true)
}

pub(crate) fn signed_market_maker_program_gossip_enabled() -> bool {
    parse_env_bool_with_default(SIGNED_MARKET_MAKER_PROGRAM_GOSSIP_ENV, true)
}

pub(crate) fn legacy_delegation_bridge_enabled() -> bool {
    parse_env_bool_with_default(LEGACY_DELEGATION_BRIDGE_ENABLED_ENV, false)
}

pub(crate) fn local_bridge_mutations_enabled() -> bool {
    parse_env_bool_with_default(LOCAL_BRIDGE_MUTATIONS_ENABLED_ENV, false)
}

pub(crate) fn mahi_accept_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env::var("KOMBAT_MAHI_ACCEPT_PROFILE").is_ok())
}

pub(crate) fn mahi_producer_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env::var("KOMBAT_MAHI_PRODUCER_PROFILE").is_ok())
}

pub(crate) fn phase6_schedule_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| parse_env_bool_with_default(PHASE6_SCHEDULE_PROFILE_ENV, false))
}

pub(crate) fn phase6_schedule_profile_sample_every() -> u64 {
    static VALUE: OnceLock<u64> = OnceLock::new();
    *VALUE.get_or_init(|| {
        parse_env_u64_with_default(
            PHASE6_SCHEDULE_PROFILE_SAMPLE_EVERY_ENV,
            DEFAULT_PHASE6_SCHEDULE_PROFILE_SAMPLE_EVERY,
        )
    })
}

pub(crate) fn phase6_schedule_profile_max_items() -> usize {
    static VALUE: OnceLock<usize> = OnceLock::new();
    *VALUE.get_or_init(|| {
        parse_env_usize_with_default(
            PHASE6_SCHEDULE_PROFILE_MAX_ITEMS_ENV,
            DEFAULT_PHASE6_SCHEDULE_PROFILE_MAX_ITEMS,
        )
    })
}
