use borsh::{to_vec, BorshDeserialize};
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use kombat_chain_consensus::{
    delegation_block_action_kind, intent_signing_hash, market_maker_program_block_action_kind,
    BlockDelegationAction, BlockMarketMakerProgramAction, Intent, IntentKind, IntentOrderKind,
    IntentOrderSide, IntentPredicate, IntentTimeInForce, MarketMakerIdentity, SignedIntent,
};
use kombat_chain_node::{
    delegation_action_id_for_submission, delegation_action_signing_bytes,
    market_maker_program_action_id_for_submission, market_maker_program_action_signing_bytes,
    MarketMakerU256, SignedDelegationAction, SignedMarketMakerProgramAction,
};
use kombat_chain_policy::{OwnerAction, OwnerCall, PolicyTerms, SessionPayload};
use kombat_chain_rpc::{
    CreateAccountParams, IntentJson, IntentKindJson, IntentPredicateJson, JsonRpcRequest,
    OrderKindJson, OwnerActionJson, OwnerCallJson, PolicyTermsWireJson, SignedDelegationActionJson,
    SignedIntentJson, SignedMarketMakerProgramActionJson, SignedOwnerCallJson, SubmitIntentParams,
    SubmitOwnerCallParams, SubmitSignedDelegationActionParams,
    SubmitSignedMarketMakerProgramActionParams, TimeInForceJson,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

mod kombat_sign_requests;

use kombat_sign_requests::*;

#[derive(Debug, Parser)]
#[command(name = "kombat_sign")]
#[command(about = "Generate signed kombat JSON-RPC request bodies")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    CreateAccount {
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        code_hash_hex: String,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 1)]
        request_id: u64,
    },
    CreatePolicy {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        grantee_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long, default_value_t = 0)]
        nonce: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value = "100")]
        gas_budget_per_day: String,
        #[arg(long, default_value = "100")]
        gas_refill_per_day: String,
        #[arg(long, default_value_t = 2)]
        max_intents_per_second: u32,
        #[arg(long, default_value_t = 10)]
        max_intents_per_day: u32,
        #[arg(long, default_value = "1000")]
        max_volume_per_day: String,
        #[arg(long, default_value_t = 25)]
        max_drawdown_pct: u16,
        #[arg(long, default_value_t = 2)]
        request_id: u64,
    },
    ProposeOwner {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        new_owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 7)]
        request_id: u64,
    },
    AcceptOwner {
        #[arg(long)]
        pending_owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 8)]
        request_id: u64,
    },
    PauseAccount {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 9)]
        request_id: u64,
    },
    UnpauseAccount {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 10)]
        request_id: u64,
    },
    AddPolicy {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 11)]
        request_id: u64,
    },
    RemovePolicy {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 12)]
        request_id: u64,
    },
    RevokePolicyOwnerCall {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 13)]
        request_id: u64,
    },
    UpgradeAccount {
        #[arg(long)]
        owner_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        code_hash_hex: String,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 14)]
        request_id: u64,
    },
    CancelIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        order_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 3)]
        request_id: u64,
    },
    PlaceOrderIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        client_order_id: u64,
        #[arg(long)]
        side: String,
        #[arg(long)]
        quantity_lots: u64,
        #[arg(long)]
        order_kind: String,
        #[arg(long)]
        price_ticks: Option<u64>,
        #[arg(long, default_value = "gtc")]
        tif: String,
        #[arg(long)]
        tif_expires_at_ms: Option<u64>,
        #[arg(long, default_value_t = false)]
        post_only: bool,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 15)]
        request_id: u64,
    },
    CancelOrderIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        client_order_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 16)]
        request_id: u64,
    },
    ReplaceOrderIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        target_client_order_id: u64,
        #[arg(long)]
        replacement_client_order_id: u64,
        #[arg(long)]
        side: String,
        #[arg(long)]
        quantity_lots: u64,
        #[arg(long)]
        order_kind: String,
        #[arg(long)]
        price_ticks: Option<u64>,
        #[arg(long, default_value = "gtc")]
        tif: String,
        #[arg(long)]
        tif_expires_at_ms: Option<u64>,
        #[arg(long, default_value_t = false)]
        post_only: bool,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 17)]
        request_id: u64,
    },
    TransferIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        to: u64,
        #[arg(long)]
        token: u32,
        #[arg(long)]
        amount: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 4)]
        request_id: u64,
    },
    CreateSubPolicyIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        child_grantee_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        child_policy_id: u64,
        #[arg(long)]
        child_expires_at: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value = "100")]
        gas_budget_per_day: String,
        #[arg(long, default_value = "100")]
        gas_refill_per_day: String,
        #[arg(long, default_value_t = 2)]
        max_intents_per_second: u32,
        #[arg(long, default_value_t = 10)]
        max_intents_per_day: u32,
        #[arg(long, default_value = "1000")]
        max_volume_per_day: String,
        #[arg(long, default_value_t = 25)]
        max_drawdown_pct: u16,
        #[arg(long, default_value_t = 5)]
        request_id: u64,
    },
    RevokePolicyIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        target_policy_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 6)]
        request_id: u64,
    },
    DepositCollateralIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        amount_usdc: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 18)]
        request_id: u64,
    },
    WithdrawCollateralIntent {
        #[arg(long)]
        session_secret_hex: String,
        #[arg(long)]
        account_id: u64,
        #[arg(long)]
        policy_id: u64,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        nonce: u64,
        #[arg(long)]
        expires_at: u64,
        #[arg(long)]
        time_after_ms: Option<u64>,
        #[arg(long)]
        time_before_ms: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        amount_usdc: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long, default_value_t = 19)]
        request_id: u64,
    },
    EncodeDelegationAction {
        #[command(subcommand)]
        action: DelegationActionCommand,
    },
    EncodeMarketMakerProgramAction {
        #[command(subcommand)]
        action: MarketMakerProgramActionCommand,
    },
    DeriveDelegationActionId {
        #[arg(long)]
        action_hex: String,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long)]
        submitter_nonce: Option<u64>,
    },
    DeriveMarketMakerProgramActionId {
        #[arg(long)]
        action_hex: String,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long)]
        submitter_nonce: Option<u64>,
    },
    SignDelegationAction {
        #[arg(long)]
        signer_secret_hex: String,
        #[arg(long)]
        signer_account_id: u64,
        #[arg(long)]
        action_hex: String,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long)]
        submitter_nonce: Option<u64>,
        #[arg(long)]
        wait_timeout_ms: Option<u64>,
        #[arg(long, default_value_t = 20)]
        request_id: u64,
    },
    SignMarketMakerProgramAction {
        #[arg(long)]
        signer_secret_hex: String,
        #[arg(long)]
        signer_account_id: u64,
        #[arg(long)]
        action_hex: String,
        #[arg(long, default_value_t = 7)]
        chain_id: u64,
        #[arg(long)]
        timestamp_ms: u64,
        #[arg(long)]
        submitter_nonce: Option<u64>,
        #[arg(long)]
        wait_timeout_ms: Option<u64>,
        #[arg(long, default_value_t = 21)]
        request_id: u64,
    },
}

#[derive(Debug, Subcommand)]
enum DelegationActionCommand {
    Open {
        #[arg(long)]
        agent: u64,
        #[arg(long)]
        mandate_id_hex: String,
        #[arg(long)]
        performance_share_units: u16,
        #[arg(long)]
        crystallization_cadence_ms: u64,
        #[arg(long)]
        notice_period_ms: u64,
        #[arg(long)]
        agent_bond: u128,
    },
    Deposit {
        #[arg(long)]
        principal: u64,
        #[arg(long)]
        delegation_id_hex: String,
        #[arg(long)]
        amount: u128,
    },
    RequestRedemption {
        #[arg(long)]
        principal: u64,
        #[arg(long)]
        delegation_id_hex: String,
        #[arg(long)]
        shares: u128,
    },
    FinalizeRedemption {
        #[arg(long)]
        request_id_hex: String,
    },
    CancelRedemption {
        #[arg(long)]
        principal: u64,
        #[arg(long)]
        request_id_hex: String,
    },
    CloseToDeposits {
        #[arg(long)]
        delegation_id_hex: String,
    },
    BeginWinding {
        #[arg(long)]
        delegation_id_hex: String,
    },
    Close {
        #[arg(long)]
        delegation_id_hex: String,
    },
}

#[derive(Debug, Subcommand)]
enum MarketMakerProgramActionCommand {
    Define {
        #[arg(long)]
        name: String,
        #[arg(long, value_delimiter = ',')]
        markets: Vec<u64>,
        #[arg(long)]
        min_uptime_bps: u16,
        #[arg(long)]
        max_spread_bps: u16,
        #[arg(long)]
        min_quote_size_micro_usdc: u128,
        #[arg(long)]
        min_two_sided_windows_per_hour: u16,
        #[arg(long)]
        rebate_multiplier_units: u16,
        #[arg(long)]
        enrollment_bond_micro_usdc: u128,
        #[arg(long)]
        compliance_window_ms: u64,
        #[arg(long)]
        observation_cadence_ms: u64,
    },
    Retire {
        #[arg(long)]
        program_id_hex: String,
    },
    Enroll {
        #[arg(long)]
        market_maker: Option<u64>,
        #[arg(long)]
        market_maker_identity_account: Option<u64>,
        #[arg(long)]
        market_maker_identity_delegation_id_hex: Option<String>,
        #[arg(long)]
        market_maker_identity_serving_agent: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        program_id_hex: String,
        #[arg(long)]
        bond_micro_usdc: u128,
    },
    Exit {
        #[arg(long)]
        market_maker: Option<u64>,
        #[arg(long)]
        market_maker_identity_account: Option<u64>,
        #[arg(long)]
        market_maker_identity_delegation_id_hex: Option<String>,
        #[arg(long)]
        market_maker_identity_serving_agent: Option<u64>,
        #[arg(long)]
        market_id: u64,
        #[arg(long)]
        program_id_hex: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let request = match cli.command {
        Command::CreateAccount {
            account_id,
            owner_secret_hex,
            code_hash_hex,
            timestamp_ms,
            request_id,
        } => create_account_request(
            account_id,
            &owner_secret_hex,
            &code_hash_hex,
            timestamp_ms,
            request_id,
        ),
        Command::CreatePolicy {
            owner_secret_hex,
            grantee_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            expires_at,
            timestamp_ms,
            gas_budget_per_day,
            gas_refill_per_day,
            max_intents_per_second,
            max_intents_per_day,
            max_volume_per_day,
            max_drawdown_pct,
            request_id,
        } => create_policy_request(CreatePolicyArgs {
            owner_secret_hex: &owner_secret_hex,
            grantee_secret_hex: &grantee_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            expires_at,
            timestamp_ms,
            gas_budget_per_day: &gas_budget_per_day,
            gas_refill_per_day: &gas_refill_per_day,
            max_intents_per_second,
            max_intents_per_day,
            max_volume_per_day: &max_volume_per_day,
            max_drawdown_pct,
            request_id,
        }),
        Command::ProposeOwner {
            owner_secret_hex,
            new_owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        } => propose_owner_request(
            &owner_secret_hex,
            &new_owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        ),
        Command::AcceptOwner {
            pending_owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        } => accept_owner_request(
            &pending_owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        ),
        Command::PauseAccount {
            owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        } => pause_account_request(
            &owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        ),
        Command::UnpauseAccount {
            owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        } => unpause_account_request(
            &owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            timestamp_ms,
            request_id,
        ),
        Command::AddPolicy {
            owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            timestamp_ms,
            request_id,
        } => add_policy_request(
            &owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            timestamp_ms,
            request_id,
        ),
        Command::RemovePolicy {
            owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            timestamp_ms,
            request_id,
        } => remove_policy_request(
            &owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            timestamp_ms,
            request_id,
        ),
        Command::RevokePolicyOwnerCall {
            owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            timestamp_ms,
            request_id,
        } => revoke_policy_owner_call_request(
            &owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            policy_id,
            timestamp_ms,
            request_id,
        ),
        Command::UpgradeAccount {
            owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            code_hash_hex,
            timestamp_ms,
            request_id,
        } => upgrade_account_request(
            &owner_secret_hex,
            account_id,
            chain_id,
            nonce,
            &code_hash_hex,
            timestamp_ms,
            request_id,
        ),
        Command::CancelIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            order_id,
            timestamp_ms,
            request_id,
        } => cancel_intent_request(CancelIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            order_id,
            timestamp_ms,
            request_id,
        }),
        Command::PlaceOrderIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            client_order_id,
            side,
            quantity_lots,
            order_kind,
            price_ticks,
            tif,
            tif_expires_at_ms,
            post_only,
            timestamp_ms,
            request_id,
        } => place_order_intent_request(PlaceOrderIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            client_order_id,
            side: &side,
            quantity_lots,
            order_kind: &order_kind,
            price_ticks,
            tif: &tif,
            tif_expires_at_ms,
            post_only,
            timestamp_ms,
            request_id,
        }),
        Command::CancelOrderIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            client_order_id,
            timestamp_ms,
            request_id,
        } => cancel_order_intent_request(CancelOrderIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            client_order_id,
            timestamp_ms,
            request_id,
        }),
        Command::ReplaceOrderIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            target_client_order_id,
            replacement_client_order_id,
            side,
            quantity_lots,
            order_kind,
            price_ticks,
            tif,
            tif_expires_at_ms,
            post_only,
            timestamp_ms,
            request_id,
        } => replace_order_intent_request(ReplaceOrderIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            target_client_order_id,
            replacement_client_order_id,
            side: &side,
            quantity_lots,
            order_kind: &order_kind,
            price_ticks,
            tif: &tif,
            tif_expires_at_ms,
            post_only,
            timestamp_ms,
            request_id,
        }),
        Command::TransferIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            to,
            token,
            amount,
            timestamp_ms,
            request_id,
        } => transfer_intent_request(TransferIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            to,
            token,
            amount,
            timestamp_ms,
            request_id,
        }),
        Command::CreateSubPolicyIntent {
            session_secret_hex,
            child_grantee_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            child_policy_id,
            child_expires_at,
            timestamp_ms,
            gas_budget_per_day,
            gas_refill_per_day,
            max_intents_per_second,
            max_intents_per_day,
            max_volume_per_day,
            max_drawdown_pct,
            request_id,
        } => create_sub_policy_intent_request(CreateSubPolicyIntentArgs {
            session_secret_hex: &session_secret_hex,
            child_grantee_secret_hex: &child_grantee_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            child_policy_id,
            child_expires_at,
            timestamp_ms,
            gas_budget_per_day: &gas_budget_per_day,
            gas_refill_per_day: &gas_refill_per_day,
            max_intents_per_second,
            max_intents_per_day,
            max_volume_per_day: &max_volume_per_day,
            max_drawdown_pct,
            request_id,
        }),
        Command::RevokePolicyIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            target_policy_id,
            timestamp_ms,
            request_id,
        } => revoke_policy_intent_request(RevokePolicyIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            target_policy_id,
            timestamp_ms,
            request_id,
        }),
        Command::DepositCollateralIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            amount_usdc,
            timestamp_ms,
            request_id,
        } => collateral_intent_request(CollateralIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            amount_usdc,
            timestamp_ms,
            request_id,
            withdraw: false,
        }),
        Command::WithdrawCollateralIntent {
            session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            amount_usdc,
            timestamp_ms,
            request_id,
        } => collateral_intent_request(CollateralIntentArgs {
            session_secret_hex: &session_secret_hex,
            account_id,
            policy_id,
            chain_id,
            nonce,
            expires_at,
            time_after_ms,
            time_before_ms,
            market_id,
            amount_usdc,
            timestamp_ms,
            request_id,
            withdraw: true,
        }),
        Command::EncodeDelegationAction { action } => encode_delegation_action_response(action),
        Command::EncodeMarketMakerProgramAction { action } => {
            encode_market_maker_program_action_response(action)
        }
        Command::DeriveDelegationActionId {
            action_hex,
            timestamp_ms,
            submitter_nonce,
        } => derive_delegation_action_id_response(&action_hex, timestamp_ms, submitter_nonce),
        Command::DeriveMarketMakerProgramActionId {
            action_hex,
            timestamp_ms,
            submitter_nonce,
        } => derive_market_maker_program_action_id_response(
            &action_hex,
            timestamp_ms,
            submitter_nonce,
        ),
        Command::SignDelegationAction {
            signer_secret_hex,
            signer_account_id,
            action_hex,
            chain_id,
            timestamp_ms,
            submitter_nonce,
            wait_timeout_ms,
            request_id,
        } => signed_delegation_action_request(SignedDelegationActionArgs {
            signer_secret_hex: &signer_secret_hex,
            signer_account_id,
            action_hex: &action_hex,
            chain_id,
            timestamp_ms,
            submitter_nonce,
            wait_timeout_ms,
            request_id,
        }),
        Command::SignMarketMakerProgramAction {
            signer_secret_hex,
            signer_account_id,
            action_hex,
            chain_id,
            timestamp_ms,
            submitter_nonce,
            wait_timeout_ms,
            request_id,
        } => signed_market_maker_program_action_request(SignedMarketMakerProgramActionArgs {
            signer_secret_hex: &signer_secret_hex,
            signer_account_id,
            action_hex: &action_hex,
            chain_id,
            timestamp_ms,
            submitter_nonce,
            wait_timeout_ms,
            request_id,
        }),
    };

    println!(
        "{}",
        serde_json::to_string(&request).expect("request should serialize")
    );
}

#[cfg(test)]
#[path = "kombat_sign_tests.rs"]
mod tests;
