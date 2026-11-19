use ed25519_dalek::SigningKey;
use kombat_chain_consensus::PacemakerConfig;
use kombat_chain_node::{component_name, SingleNodeConfig, SingleNodeEngine};

fn main() {
    let components = [
        kombat_chain_consensus::component_name(),
        kombat_chain_execution::component_name(),
        kombat_chain_state_db::component_name(),
        kombat_chain_network::component_name(),
        kombat_chain_policy::component_name(),
        kombat_chain_matching::component_name(),
        kombat_chain_oracle::component_name(),
        kombat_chain_streams::component_name(),
        component_name(),
    ];

    let config = SingleNodeConfig {
        chain_id: 1,
        validator_id: SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes(),
        consensus_signing_key: [7; 32],
        genesis_timestamp_ms: 1_000,
        genesis_state_root: [0; 32],
        pacemaker: PacemakerConfig {
            base_timeout_ms: 300,
            max_timeout_ms: 20_000,
            max_exponent: 6,
            max_clock_skew_ms: 1_000,
        },
    };

    let mut engine = SingleNodeEngine::new(config);
    let produced = engine
        .produce_empty_block(1_100)
        .expect("single-node bootstrap block should produce");

    println!(
        "kombat-chain-node single-node mode: components={}, height={}, round={}",
        components.join(", "),
        produced.header.height,
        produced.header.round,
    );
}
