use crate::codec::{deserialize_u128_from_string_or_number, serialize_u128_as_string};
use kombat_chain_consensus::{BlockDelegationAction, DelegationActionId};
use kombat_chain_node::MandateAction;
use kombat_chain_policy::{OwnerCall, Principal};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedIntentJson {
    pub intent: IntentJson,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedOwnerCallJson {
    pub account_id: u64,
    pub chain_id: u64,
    pub call: OwnerCallJson,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEvmOwnerCallJson {
    pub account_id: u64,
    pub chain_id: u64,
    pub call: OwnerCallJson,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedEvmOwnerCall {
    pub account_id: u64,
    pub chain_id: u64,
    pub call: OwnerCall,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEvmAccountClaimJson {
    pub account_id: u64,
    pub chain_id: u64,
    pub wallet_address_hex: String,
    pub owner_hex: String,
    pub code_hash_hex: String,
    pub expires_at: u64,
    pub signature_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedEvmAccountClaim {
    pub account_id: u64,
    pub chain_id: u64,
    pub wallet: [u8; 20],
    pub owner: Principal,
    pub code_hash: [u8; 32],
    pub expires_at: u64,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedEvmMandateAction {
    pub chain_id: u64,
    pub owner: Principal,
    pub nonce: u64,
    pub valid_until_ms: u64,
    pub action: MandateAction,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedEvmDelegationAction {
    pub chain_id: u64,
    pub signer_account_id: u64,
    pub action_id: DelegationActionId,
    pub action: BlockDelegationAction,
    pub timestamp_ms: u64,
    pub submitter_nonce: u64,
    pub valid_until_ms: u64,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerCallJson {
    pub signer_hex: String,
    pub nonce: u64,
    #[serde(flatten)]
    pub action: OwnerActionJson,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentJson {
    pub chain_id: u64,
    pub account: u64,
    pub policy_id: u64,
    pub nonce: u64,
    pub expires_at: u64,
    pub predicate: Option<IntentPredicateJson>,
    #[serde(flatten)]
    pub kind: IntentKindJson,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRequestJson {
    pub market_id: u64,
    pub account_id: u64,
    pub client_order_id: u64,
    pub side: String,
    pub quantity_lots: u64,
    #[serde(flatten)]
    pub kind: OrderKindJson,
    pub time_in_force: TimeInForceJson,
    pub post_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "order_kind", rename_all = "snake_case")]
pub enum OrderKindJson {
    Limit { price_ticks: u64 },
    Market,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tif", rename_all = "snake_case")]
pub enum TimeInForceJson {
    Gtc,
    Ioc,
    Fok,
    Gtt { expires_at_ms: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "predicate_kind", rename_all = "snake_case")]
pub enum IntentPredicateJson {
    TimeAfter { not_before_ms: u64 },
    TimeBefore { not_after_ms: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntentKindJson {
    Transfer {
        to: u64,
        token: u32,
        amount: u64,
    },
    Cancel {
        order_id: u64,
    },
    PlaceOrder {
        market_id: u64,
        client_order_id: u64,
        side: String,
        quantity_lots: u64,
        #[serde(flatten)]
        order_kind: OrderKindJson,
        time_in_force: TimeInForceJson,
        post_only: bool,
    },
    CancelOrder {
        market_id: u64,
        client_order_id: u64,
    },
    ReplaceOrder {
        market_id: u64,
        target_client_order_id: u64,
        replacement_client_order_id: u64,
        side: String,
        quantity_lots: u64,
        #[serde(flatten)]
        order_kind: OrderKindJson,
        time_in_force: TimeInForceJson,
        post_only: bool,
    },
    CreateSubPolicy {
        child_policy_id: u64,
        child_grantee_hex: String,
        child_expires_at: u64,
        child_terms: PolicyTermsWireJson,
    },
    RevokePolicy {
        target_policy_id: u64,
    },
    DepositCollateral {
        market_id: u64,
        amount_usdc: u64,
    },
    WithdrawCollateral {
        market_id: u64,
        amount_usdc: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OwnerActionJson {
    ProposeOwner {
        new_owner_hex: String,
    },
    AcceptOwner,
    Pause,
    Unpause,
    CreatePolicy {
        policy_id: u64,
        grantee_hex: String,
        expires_at: u64,
        terms: PolicyTermsWireJson,
    },
    AddPolicy {
        policy_id: u64,
    },
    RemovePolicy {
        policy_id: u64,
    },
    RevokePolicy {
        policy_id: u64,
    },
    Upgrade {
        code_hash_hex: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyTermsWireJson {
    #[serde(
        serialize_with = "serialize_u128_as_string",
        deserialize_with = "deserialize_u128_from_string_or_number"
    )]
    pub gas_budget_per_day: u128,
    #[serde(
        serialize_with = "serialize_u128_as_string",
        deserialize_with = "deserialize_u128_from_string_or_number"
    )]
    pub gas_refill_per_day: u128,
    pub max_intents_per_second: u32,
    pub max_intents_per_day: u32,
    #[serde(
        serialize_with = "serialize_u128_as_string",
        deserialize_with = "deserialize_u128_from_string_or_number"
    )]
    pub max_volume_per_day: u128,
    pub max_drawdown_pct: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventFilterJson {
    pub block_height: Option<u64>,
    pub account_id: Option<u64>,
    pub market_id: Option<u64>,
    pub policy_id: Option<u64>,
    pub intent_id_hex: Option<String>,
    pub client_order_id: Option<u64>,
    pub topic: Option<String>,
}
