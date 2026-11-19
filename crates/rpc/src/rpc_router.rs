use crate::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcRouter {
    methods: BTreeMap<&'static str, &'static str>,
}

impl RpcRouter {
    pub fn new() -> Self {
        Self::new_with_config(JsonRpcServerConfig::default())
    }

    pub fn new_with_config(config: JsonRpcServerConfig) -> Self {
        let mut methods = BTreeMap::from([
            (
                "eth_blockNumber",
                "latest committed block height as an Ethereum hex quantity",
            ),
            ("eth_call", "read-only contract call"),
            ("eth_chainId", "chain id as an Ethereum hex quantity"),
            ("eth_estimateGas", "simulate a contract transaction and return gas used"),
            ("eth_gasPrice", "fixed zero gas price for the current chain"),
            ("eth_getBalance", "EVM account balance as an Ethereum hex quantity"),
            (
                "eth_getTransactionByHash",
                "transaction lookup by Ethereum transaction hash",
            ),
            ("eth_getCode", "contract code at an address"),
            (
                "eth_getTransactionCount",
                "account nonce as an Ethereum hex quantity",
            ),
            (
                "eth_getTransactionReceipt",
                "transaction receipt lookup by Ethereum transaction hash",
            ),
            ("eth_sendRawTransaction", "submit a signed raw EVM transaction"),
            (
                "kombat_cancelOrder",
                "cancel a resting order by market/account/client id",
            ),
            ("kombat_createAccount", "create a standard agent account"),
            (
                "kombat_createEvmAccount",
                "create an EVM-wallet-owned account with an EIP-712 account claim",
            ),
            ("kombat_chainInfo", "chain metadata and current head"),
            (
                "kombat_getConsensusLogs",
                "recent consensus telemetry log entries",
            ),
            (
                "kombat_getConsensusMetrics",
                "current consensus telemetry counters and block timing samples",
            ),
            ("kombat_getAccount", "account snapshot by account id"),
            (
                "kombat_getAccountByEvmAddress",
                "account snapshot linked to an EVM address",
            ),
            (
                "kombat_getAgentIdentity",
                "RFC 050 agent identity record and current consensus tier",
            ),
            (
                "kombat_listAttestations",
                "RFC 050 attestation records for one account",
            ),
            (
                "kombat_getCompositionAccess",
                "RFC 050 derived composition-access flags for one account",
            ),
            (
                "kombat_getReputationMultiplier",
                "RFC 050 reputation multiplier read for one account",
            ),
            (
                "kombat_getIdentityActionNonce",
                "next RFC 050 account-self identity action nonce for one account",
            ),
            (
                "kombat_getIdentityAttestorNonce",
                "next RFC 050 operator-attestation nonce for one attestor account",
            ),
            (
                "kombat_submitIdentityAction",
                "submit a signed RFC 050 identity action",
            ),
            (
                "kombat_submitTeeAttestation",
                "submit a signed RFC 051 TEE attestation action",
            ),
            (
                "kombat_listVendorRoots",
                "RFC 051 TEE vendor root registry read surface",
            ),
            (
                "kombat_getVendorCapProfile",
                "RFC 051 TEE vendor cap-profile read surface",
            ),
            (
                "kombat_getWalletBalances",
                "policy and EVM USDC balances for one EVM address",
            ),
            (
                "kombat_getStateRootAtHeight",
                "canonical block hash and state root at one committed height",
            ),
            (
                "kombat_getIntentStatus",
                "latest observed intent lifecycle status",
            ),
            (
                "kombat_getOpenOrders",
                "resting orders for an account in one market",
            ),
            ("kombat_getOrderBook", "book levels for a single market"),
            (
                "kombat_getOrderStatus",
                "latest observed status for one account order",
            ),
            (
                "kombat_getCandles",
                "server-built OHLCV candles for one market",
            ),
            (
                "kombat_getMarketSnapshot",
                "UI boot snapshot for one market including book and latest trade",
            ),
            (
                "kombat_getAccountTradingState",
                "UI boot snapshot for one account's positions, orders, margin, and fills",
            ),
            (
                "kombat_getAccountPnlWindow",
                "committed realised P&L, funding, and fees for one account over a time window",
            ),
            (
                "kombat_getAccountEquityHistory",
                "committed account equity samples over a block-timestamp window",
            ),
            (
                "kombat_getMandateConformanceHistory",
                "recent aggregate mandate conformance evaluations",
            ),
            (
                "kombat_getLiquidationHistory",
                "historical liquidation executions from the runtime event log",
            ),
            (
                "kombat_getLiquidationEscalationHistory",
                "historical liquidation escalation updates from the runtime event log",
            ),
            (
                "kombat_getOracleHistory",
                "historical oracle config, mark price, and index price updates from the runtime event log",
            ),
            (
                "kombat_getMarketTradingHistory",
                "historical market pause and resume updates from the runtime event log",
            ),
            (
                "kombat_getInsuranceFundHistory",
                "historical insurance fund balance updates from the runtime event log",
            ),
            (
                "kombat_getBadDebtHistory",
                "historical bad debt balance updates from the runtime event log",
            ),
            (
                "kombat_getSolvencyHistory",
                "historical solvency-breach events from the runtime event log",
            ),
            (
                "kombat_getRiskStateHistory",
                "historical per-account risk-state updates from the runtime event log",
            ),
            (
                "kombat_getLiquidationCandidateHistory",
                "historical liquidation-candidate updates from the runtime event log",
            ),
            (
                "kombat_getFundingHistory",
                "historical funding index updates and account funding settlements",
            ),
            (
                "kombat_getFundingState",
                "latest funding index and indicative funding inputs for one market",
            ),
            (
                "kombat_getInsuranceFund",
                "latest insurance fund and bad-debt balances for one market",
            ),
            (
                "kombat_getFeeHistory",
                "historical fee charges from the runtime event log",
            ),
            (
                "kombat_getRevenueSplitHistory",
                "historical fee revenue distributions from the runtime event log",
            ),
            (
                "kombat_liquidatePosition",
                "operator-close one under-maintenance isolated position through the matcher",
            ),
            (
                "kombat_runLiquidationPass",
                "operator-drain under-maintenance positions in deterministic market/account order",
            ),
            (
                "kombat_previewLiquidation",
                "read-only liquidation simulation for one isolated position at current market state",
            ),
            (
                "kombat_previewIntent",
                "read-only initial-margin preflight for a candidate trading intent",
            ),
            (
                "kombat_getPosition",
                "position summary for one account in one market",
            ),
            (
                "kombat_getAccountHistory",
                "historical account creation and authority updates from the runtime event log",
            ),
            (
                "kombat_getRiskSnapshot",
                "mark-based risk snapshot for one account in one market",
            ),
            (
                "kombat_getSuggestedLiquidation",
                "current recommended liquidation plan for one account in one market",
            ),
            (
                "kombat_listSuggestedLiquidations",
                "current recommended liquidation plans for under-maintenance positions, with optional market/account/outcome filters",
            ),
            ("kombat_getMarketState", "latest mark state for one market"),
            ("kombat_getMarket", "perpetual market registry row by market id"),
            (
                "kombat_listPerpetualMarkets",
                "listed perpetual markets with optional status filter",
            ),
            (
                "kombat_getDelegation",
                "performance-share delegation row by delegation id",
            ),
            ("kombat_getMandate", "mandate row by mandate id"),
            (
                "kombat_listMandates",
                "mandates with optional agent and state filters",
            ),
            (
                "kombat_listDelegations",
                "performance-share delegations with optional agent and status filters",
            ),
            (
                "kombat_listVaults",
                "RFC 043 vault discovery surface; defaults to listed USDC vaults sorted by TVL",
            ),
            (
                "kombat_listRedemptionRequests",
                "RFC 049 principal redemption requests for vault UI cancel/finalize state",
            ),
            (
                "kombat_listVaultPositions",
                "RFC 049 principal vault positions with shares, locks, NAV, and estimated value",
            ),
            (
                "kombat_listRealCapitalListings",
                "RFC 049 real-capital vault listing and cap read surface",
            ),
            (
                "kombat_getRealCapitalListing",
                "RFC 049 real-capital listing lookup by vault id",
            ),
            (
                "kombat_submitRealCapitalGovernanceAction",
                "operator-authenticated enqueue-only RFC 049 real-capital governance action",
            ),
            (
                "kombat_getRealCapitalGovernanceActionStatus",
                "read pending or processed status for an RFC 049 real-capital governance action id",
            ),
            (
                "kombat_getVault",
                "RFC 043 vault record by vault id (returns hidden vaults too)",
            ),
            (
                "kombat_listMarketStates",
                "latest market state ranked by liquidation pressure, with optional stressed/outcome filters",
            ),
            (
                "kombat_getLiquidationPressureSummary",
                "node-wide liquidation pressure summary across stressed markets and accounts",
            ),
            (
                "kombat_getAccountTradeHistory",
                "order fills for one account across markets",
            ),
            (
                "kombat_getPositionHistory",
                "historical position updates from the runtime event log",
            ),
            (
                "kombat_listRiskSnapshots",
                "mark-based risk snapshots across markets for one account",
            ),
            (
                "kombat_getAccountRiskSummary",
                "aggregated isolated risk summary across markets for one account",
            ),
            (
                "kombat_listAccountRiskSummaries",
                "aggregated isolated risk summaries across stressed accounts, with optional market/outcome filters",
            ),
            (
                "kombat_listLiquidationCandidates",
                "under-maintenance isolated positions ready for liquidation",
            ),
            (
                "kombat_listPositions",
                "position summaries across markets for one account",
            ),
            ("kombat_getTradeHistory", "order fills for one market"),
            (
                "kombat_setIsolatedCollateral",
                "operator-set isolated collateral for one account in one market",
            ),
            (
                "kombat_setMarketRisk",
                "operator-set market maintenance margin parameters",
            ),
            (
                "kombat_setFundingIndex",
                "operator-set the cumulative funding index for one market",
            ),
            (
                "kombat_setIndexPrice",
                "operator-set the external index price for one market",
            ),
            (
                "kombat_setMarkPrice",
                "operator-set the mark price for one market",
            ),
            (
                "kombat_submitOraclePrices",
                "operator-submit feeder samples and commit deterministic median oracle price",
            ),
            (
                "kombat_advanceFundingInterval",
                "operator-advance one market funding interval from current mark and index",
            ),
            (
                "kombat_setMarketFees",
                "operator-set maker and taker fee parameters for one market",
            ),
            (
                "kombat_setMarketTradingState",
                "operator-pause or resume new trading on one market",
            ),
            (
                "kombat_setMarketOracleConfig",
                "operator-set per-market oracle staleness configuration",
            ),
            (
                "kombat_listPerpetualMarket",
                "operator-list a perpetual market registry row",
            ),
            (
                "kombat_setPerpetualMarketStatus",
                "operator-set a perpetual market status",
            ),
            (
                "kombat_openDelegation",
                "legacy operator bridge: open a performance-share delegation (RFC 026)",
            ),
            (
                "kombat_depositToDelegation",
                "legacy operator bridge: deposit principal policy USDC into a performance-share delegation (RFC 026)",
            ),
            (
                "kombat_submitDepositToDelegation",
                "operator bridge: submit a high-level delegation deposit into the RFC 026 pending queue",
            ),
            (
                "kombat_requestDelegationRedemption",
                "legacy operator bridge: request redemption of principal delegation shares (RFC 026)",
            ),
            (
                "kombat_finalizeDelegationRedemption",
                "legacy operator bridge: finalize a ready delegation redemption (RFC 026)",
            ),
            (
                "kombat_cancelDelegationRedemption",
                "legacy operator bridge: cancel a pending delegation redemption (RFC 026)",
            ),
            (
                "kombat_closeDelegationToDeposits",
                "legacy operator bridge: close a performance-share delegation to new deposits (RFC 026)",
            ),
            (
                "kombat_beginDelegationWinding",
                "legacy operator bridge: begin performance-share delegation winding (RFC 026)",
            ),
            (
                "kombat_closeDelegation",
                "legacy operator bridge: close a fully wound performance-share delegation and return the agent bond (RFC 026)",
            ),
            (
                "kombat_submitDelegationAction",
                "legacy operator bridge: submit a Borsh-encoded delegation action into the RFC 026 pending queue",
            ),
            (
                "kombat_submitSignedDelegationAction",
                "submit an account-owner-signed RFC 026 delegation action into the pending queue",
            ),
            (
                "kombat_submitEvmDelegationAction",
                "submit an EVM-wallet-signed RFC 026 delegation action into the pending queue",
            ),
            (
                "kombat_getDelegationActionStatus",
                "read pending or processed status for an RFC 026 delegation action id",
            ),
            (
                "kombat_submitMarketMakerProgramAction",
                "operator bridge: submit a Borsh-encoded RFC 027 market-maker program action into the pending queue",
            ),
            (
                "kombat_defineMarketMakerProgram",
                "operator bridge: define an RFC 027 market-maker program through the pending queue",
            ),
            (
                "kombat_enrollMarketMakerProgram",
                "operator bridge: enroll a market maker in an RFC 027 program through the pending queue",
            ),
            (
                "kombat_submitSignedMarketMakerProgramAction",
                "submit an account-owner-signed RFC 027 market-maker program action into the pending queue",
            ),
            (
                "kombat_getMarketMakerProgramActionStatus",
                "read pending or processed status for an RFC 027 market-maker program action id",
            ),
            (
                "kombat_getMarketMakerProgram",
                "read an RFC 027 market-maker program by program id",
            ),
            (
                "kombat_getMarketMakerEnrollment",
                "read an RFC 027 market-maker enrollment by market maker, market, and program id",
            ),
            (
                "kombat_createMandate",
                "operator-create and Tier-0 activate a mandate (doc 05 testnet bridge)",
            ),
            (
                "kombat_submitMandateAction",
                "submit a signed mandate owner action",
            ),
            (
                "kombat_submitEvmMandateAction",
                "submit an EVM-wallet-signed mandate owner action",
            ),
            (
                "kombat_setAccountOperator",
                "operator-set temporary account operator linkage for doc 05 same-operator checks",
            ),
            (
                "kombat_getAccountOperator",
                "read temporary account operator linkage for doc 05 same-operator checks",
            ),
            (
                "kombat_getMandateActionNonce",
                "next mandate-action nonce for an owner principal",
            ),
            (
                "kombat_suspendMandate",
                "operator-suspend a mandate and block generated policies",
            ),
            (
                "kombat_resumeMandate",
                "operator-resume a suspended mandate and generated policies",
            ),
            (
                "kombat_revokeMandate",
                "operator-revoke a mandate and cascade generated policy revocation",
            ),
            (
                "kombat_expireDueMandates",
                "operator-expire due mandates at a timestamp",
            ),
            (
                "kombat_setPolicyUsdcBalance",
                "operator-set chain-policy USDC balance for one EVM address",
            ),
            (
                "kombat_faucetFundAccount",
                "testnet faucet top-up for policy USDC and EVM USDC",
            ),
            (
                "kombat_setEvmUsdcMinter",
                "operator-grant or revoke the EVM USDC bridge minter role",
            ),
            ("kombat_getPolicy", "policy snapshot by policy id"),
            ("kombat_getState", "deterministic committed-state lookup"),
            ("kombat_listEvents", "runtime events since sequence"),
            ("kombat_loadSnapshot", "operator-load a persisted engine snapshot"),
            ("kombat_replaceOrder", "atomically replace a resting order"),
            ("kombat_saveSnapshot", "operator-save a persisted engine snapshot"),
            (
                "kombat_submitOrder",
                "submit a direct order to the matching engine",
            ),
            ("kombat_submitOwnerCall", "submit a signed owner call"),
            (
                "kombat_submitEvmOwnerCall",
                "submit an EVM-wallet-signed owner call for a wallet-owned account",
            ),
            ("kombat_submitIntent", "submit a signed typed intent"),
            (
                "kombat_submitIntentBatch",
                "submit many signed typed intents in one rpc call",
            ),
            (
                "kombat_runNetworkTimeoutRecovery",
                "operator-trigger timeout certificate recovery across validators",
            ),
        ]);
        if config.manual_block_production_enabled {
            methods.insert(
                "kombat_produceBlock",
                "produce a block from the current mempool",
            );
        }
        Self { methods }
    }

    pub fn methods(&self) -> &BTreeMap<&'static str, &'static str> {
        &self.methods
    }
}

impl Default for RpcRouter {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn policy_response(policy: &PolicyRecord) -> PolicyResponse {
    let (execution_account_id, execution_scope) =
        policy_execution_scope_response(&policy.execution_scope);
    PolicyResponse {
        id: policy.id,
        authority_account: policy.authority_account,
        account: policy.authority_account,
        execution_account_id,
        execution_scope,
        parent_policy_id: policy.parent_policy_id,
        state: policy.state,
        next_intent_nonce: policy.next_intent_nonce,
        delegation_depth: policy.delegation_depth,
        revoked_at: policy.revoked_at,
        terms: PolicyTermsWireJson {
            gas_budget_per_day: policy.terms.gas_budget_per_day,
            gas_refill_per_day: policy.terms.gas_refill_per_day,
            max_intents_per_second: policy.terms.max_intents_per_second,
            max_intents_per_day: policy.terms.max_intents_per_day,
            max_volume_per_day: policy.terms.max_volume_per_day,
            max_drawdown_pct: policy.terms.max_drawdown_pct,
        },
    }
}

fn policy_execution_scope_response(
    scope: &PolicyExecutionScope,
) -> (u64, PolicyExecutionScopeResponse) {
    match scope {
        PolicyExecutionScope::Account(account_id) => (
            *account_id,
            PolicyExecutionScopeResponse {
                kind: PolicyExecutionScopeKindResponse::Account,
                delegation_id_hex: None,
            },
        ),
        PolicyExecutionScope::DelegationTrading { delegation_id } => (
            delegation_trading_account_id(delegation_id),
            PolicyExecutionScopeResponse {
                kind: PolicyExecutionScopeKindResponse::DelegationTrading,
                delegation_id_hex: Some(encode_hex(delegation_id)),
            },
        ),
    }
}

pub(crate) fn account_response(
    account: &kombat_chain_policy::StandardAgentAccount,
    frozen: bool,
    evm_address: Option<[u8; 20]>,
) -> AccountResponse {
    AccountResponse {
        id: account.id(),
        owner: account.owner(),
        pending_owner: account.pending_owner(),
        created_at: account.created_at(),
        paused: account.paused(),
        frozen,
        controllers: account.controllers().to_vec(),
        nonce: account.nonce(),
        code_hash: account.code_hash(),
        evm_address,
    }
}
