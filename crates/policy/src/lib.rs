use ed25519_dalek::{Signature, VerifyingKey};
use std::collections::BTreeMap;

pub const CRATE_NAME: &str = "policy";

pub type AccountId = u64;
pub type PolicyId = u64;
pub type Timestamp = u64;
pub type Principal = [u8; 32];
pub type Hash = [u8; 32];
pub type DelegationId = [u8; 32];
pub type MicroUsdc = u128;
pub type SignedPnl = i128;
pub type BlockNumber = u64;
pub type SessionPubKey = [u8; 32];
pub type SessionSignature = [u8; 64];

const SECOND_MS: u64 = 1_000;
const MINUTE_MS: u64 = 60_000;
const DAY_MS: u64 = 86_400_000;
const MAX_SUB_POLICY_DEPTH: u8 = 5;

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct StandardAgentAccount {
    id: AccountId,
    owner: Principal,
    pending_owner: Option<Principal>,
    created_at: Timestamp,
    paused: bool,
    controllers: Vec<PolicyId>,
    nonce: u64,
    code_hash: Hash,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct OwnerCall {
    pub signer: Principal,
    pub nonce: u64,
    pub action: OwnerAction,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SignedOwnerCall {
    pub account_id: AccountId,
    pub chain_id: u64,
    pub call: OwnerCall,
    pub signature: SessionSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum OwnerAction {
    ProposeOwner {
        new_owner: Principal,
    },
    AcceptOwner,
    Pause,
    Unpause,
    CreatePolicy {
        policy_id: PolicyId,
        grantee: SessionPubKey,
        expires_at: Timestamp,
        terms: PolicyTerms,
    },
    AddPolicy {
        policy_id: PolicyId,
    },
    RemovePolicy {
        policy_id: PolicyId,
    },
    RevokePolicy {
        policy_id: PolicyId,
    },
    Upgrade {
        code_hash: Hash,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountError {
    DuplicateAccount(AccountId),
    UnknownAccount(AccountId),
    Unauthorized,
    InvalidNonce { expected: u64, actual: u64 },
    ChainMismatch { expected: u64, actual: u64 },
    NoPendingOwner,
    PendingOwnerMismatch,
    DuplicatePolicy(PolicyId),
    UnknownPolicy(PolicyId),
    InvalidOwnerProposal,
    InvalidOwnerSignature,
    UpgradeNotApproved(Hash),
    UpgradeNoop,
    Policy(PolicyError),
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SessionPayload {
    pub intent_hash: [u8; 32],
    pub policy_id: PolicyId,
    pub chain_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SignedSessionPayload {
    pub payload: SessionPayload,
    pub signature: SessionSignature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionVerificationMaterial {
    pub signing_bytes: Vec<u8>,
    pub current_grantee: SessionPubKey,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PolicyKeyState {
    pub grantee: SessionPubKey,
    pub revoked_grantees: BTreeMap<SessionPubKey, BlockNumber>,
    pub revoked: bool,
    pub revoked_at: Option<BlockNumber>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PolicyState {
    PendingActivation,
    Active,
    Revoked,
    Expired,
    Suspended,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PolicyTerms {
    pub gas_budget_per_day: MicroUsdc,
    pub gas_refill_per_day: MicroUsdc,
    pub max_intents_per_second: u32,
    pub max_intents_per_day: u32,
    pub max_volume_per_day: MicroUsdc,
    pub max_drawdown_pct: u16,
}

impl Default for PolicyTerms {
    fn default() -> Self {
        Self {
            gas_budget_per_day: 0,
            gas_refill_per_day: 0,
            max_intents_per_second: u32::MAX,
            max_intents_per_day: u32::MAX,
            max_volume_per_day: MicroUsdc::MAX,
            max_drawdown_pct: u16::MAX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PolicyAdmissionRules {
    pub allowed_markets: Vec<u64>,
    pub max_trade_notional: MicroUsdc,
    pub max_notional_outstanding: MicroUsdc,
    pub max_intents_per_minute: u32,
}

impl Default for PolicyAdmissionRules {
    fn default() -> Self {
        Self {
            allowed_markets: Vec::new(),
            max_trade_notional: 0,
            max_notional_outstanding: 0,
            max_intents_per_minute: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PolicyCounters {
    pub notional_outstanding: MicroUsdc,
    pub volume_today: MicroUsdc,
    pub volume_today_reset_at: Timestamp,
    pub intents_this_second: u32,
    pub intents_this_sec_start: Timestamp,
    pub intents_this_minute: u32,
    pub intents_this_minute_start: Timestamp,
    pub intents_today: u32,
    pub intents_today_reset_at: Timestamp,
    pub gas_used_today: MicroUsdc,
    pub gas_reset_at: Timestamp,
    pub realized_pnl: SignedPnl,
    pub peak_pnl: SignedPnl,
    pub current_drawdown_pct: u16,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct DelegationReservations {
    pub volume_per_day: MicroUsdc,
    pub gas_budget_per_day: MicroUsdc,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PolicyOrigin {
    Direct,
    MandateGenerated {
        mandate_id: Hash,
        constraint_index: u16,
        generated_policy_index: u16,
    },
}

impl Default for PolicyOrigin {
    fn default() -> Self {
        Self::Direct
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
)]
pub enum PolicyExecutionScope {
    Account(AccountId),
    DelegationTrading { delegation_id: DelegationId },
}

impl PolicyExecutionScope {
    pub fn direct(account_id: AccountId) -> Self {
        Self::Account(account_id)
    }

    pub fn delegation_id(&self) -> Option<&DelegationId> {
        match self {
            Self::Account(_) => None,
            Self::DelegationTrading { delegation_id } => Some(delegation_id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PolicyRecord {
    pub id: PolicyId,
    pub authority_account: AccountId,
    pub execution_scope: PolicyExecutionScope,
    pub parent_policy_id: Option<PolicyId>,
    pub delegation_depth: u8,
    pub key_state: PolicyKeyState,
    pub next_intent_nonce: u64,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub state: PolicyState,
    pub terms: PolicyTerms,
    pub admission_rules: PolicyAdmissionRules,
    pub origin: PolicyOrigin,
    pub reserved: DelegationReservations,
    pub counters: PolicyCounters,
    pub revoked_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyAdmission {
    pub signed_session: SignedSessionPayload,
    pub intent_nonce: u64,
    pub block_number: BlockNumber,
    pub expected_chain_id: u64,
    pub timestamp_ms: Timestamp,
    pub gas_cost: MicroUsdc,
    pub volume: MicroUsdc,
    pub notional_outstanding: MicroUsdc,
    pub intent_notional: MicroUsdc,
    pub market_id: Option<u64>,
    pub realized_pnl_delta: SignedPnl,
    pub current_drawdown_pct: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyAdmissionDelta {
    pub counters: PolicyCounters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PolicyStore {
    policies: BTreeMap<PolicyId, PolicyRecord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AccountStore {
    accounts: BTreeMap<AccountId, StandardAgentAccount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRotationEvent {
    pub policy_id: PolicyId,
    pub old_grantee: SessionPubKey,
    pub new_grantee: SessionPubKey,
    pub block_number: BlockNumber,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyError {
    DuplicatePolicyId(PolicyId),
    UnknownPolicy(PolicyId),
    UnauthorizedGrantee,
    SessionPolicyMismatch {
        expected: PolicyId,
        actual: PolicyId,
    },
    SessionChainMismatch {
        expected: u64,
        actual: u64,
    },
    InvalidSessionSignature,
    InvalidSessionKeyMaterial,
    SessionKeyRevoked {
        revoked_at: BlockNumber,
        block_number: BlockNumber,
    },
    SessionKeyAlreadyActive,
    SessionKeyAlreadyRevoked,
    InvalidIntentNonce {
        expected: u64,
        actual: u64,
    },
    PolicyNotActive(PolicyState),
    PolicyExpired,
    GasBudgetExceeded,
    IntentRateLimitExceeded,
    MinuteIntentLimitExceeded,
    DailyIntentLimitExceeded,
    DailyVolumeExceeded,
    MarketNotAllowed,
    TradeNotionalExceeded,
    NotionalOutstandingExceeded,
    SubPolicyScopeMismatch,
    SubPolicyTermsExceedParent(&'static str),
    DelegationDepthExceeded {
        max_depth: u8,
        attempted_depth: u8,
    },
    DrawdownExceededAndRevoked,
    Overflow,
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

impl StandardAgentAccount {
    pub fn new(id: AccountId, owner: Principal, created_at: Timestamp, code_hash: Hash) -> Self {
        Self {
            id,
            owner,
            pending_owner: None,
            created_at,
            paused: false,
            controllers: Vec::new(),
            nonce: 0,
            code_hash,
        }
    }

    pub fn id(&self) -> AccountId {
        self.id
    }

    pub fn owner(&self) -> Principal {
        self.owner
    }

    pub fn pending_owner(&self) -> Option<Principal> {
        self.pending_owner
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn nonce(&self) -> u64 {
        self.nonce
    }

    pub fn code_hash(&self) -> Hash {
        self.code_hash
    }

    pub fn controllers(&self) -> &[PolicyId] {
        &self.controllers
    }

    pub fn has_controller(&self, policy_id: PolicyId) -> bool {
        self.controllers.binary_search(&policy_id).is_ok()
    }

    pub fn apply_owner_call(
        &mut self,
        call: OwnerCall,
        approved_code_hashes: &[Hash],
    ) -> Result<(), AccountError> {
        self.ensure_nonce(call.nonce)?;

        match call.action {
            OwnerAction::ProposeOwner { new_owner } => {
                self.ensure_owner(call.signer)?;
                if new_owner == self.owner {
                    return Err(AccountError::InvalidOwnerProposal);
                }
                self.pending_owner = Some(new_owner);
            }
            OwnerAction::AcceptOwner => {
                let pending_owner = self.pending_owner.ok_or(AccountError::NoPendingOwner)?;
                if call.signer != pending_owner {
                    return Err(AccountError::PendingOwnerMismatch);
                }
                self.owner = pending_owner;
                self.pending_owner = None;
            }
            OwnerAction::Pause => {
                self.ensure_owner(call.signer)?;
                self.paused = true;
            }
            OwnerAction::Unpause => {
                self.ensure_owner(call.signer)?;
                self.paused = false;
            }
            OwnerAction::CreatePolicy { policy_id, .. } | OwnerAction::AddPolicy { policy_id } => {
                self.ensure_owner(call.signer)?;
                self.add_controller(policy_id)?;
            }
            OwnerAction::RemovePolicy { policy_id } | OwnerAction::RevokePolicy { policy_id } => {
                self.ensure_owner(call.signer)?;
                self.remove_controller(policy_id)?;
            }
            OwnerAction::Upgrade { code_hash } => {
                self.ensure_owner(call.signer)?;
                if code_hash == self.code_hash {
                    return Err(AccountError::UpgradeNoop);
                }
                if !approved_code_hashes.contains(&code_hash) {
                    return Err(AccountError::UpgradeNotApproved(code_hash));
                }
                self.code_hash = code_hash;
            }
        }

        self.nonce = self.nonce.saturating_add(1);
        Ok(())
    }

    fn ensure_owner(&self, signer: Principal) -> Result<(), AccountError> {
        if signer == self.owner {
            Ok(())
        } else {
            Err(AccountError::Unauthorized)
        }
    }

    fn ensure_nonce(&self, nonce: u64) -> Result<(), AccountError> {
        if nonce == self.nonce {
            Ok(())
        } else {
            Err(AccountError::InvalidNonce {
                expected: self.nonce,
                actual: nonce,
            })
        }
    }

    fn add_controller(&mut self, policy_id: PolicyId) -> Result<(), AccountError> {
        match self.controllers.binary_search(&policy_id) {
            Ok(_) => Err(AccountError::DuplicatePolicy(policy_id)),
            Err(index) => {
                self.controllers.insert(index, policy_id);
                Ok(())
            }
        }
    }

    fn remove_controller(&mut self, policy_id: PolicyId) -> Result<(), AccountError> {
        match self.controllers.binary_search(&policy_id) {
            Ok(index) => {
                self.controllers.remove(index);
                Ok(())
            }
            Err(_) => Err(AccountError::UnknownPolicy(policy_id)),
        }
    }
}

impl AccountStore {
    pub fn insert(&mut self, account: StandardAgentAccount) -> Result<(), AccountError> {
        if self.accounts.contains_key(&account.id()) {
            return Err(AccountError::DuplicateAccount(account.id()));
        }
        self.accounts.insert(account.id(), account);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    pub fn get(&self, account_id: AccountId) -> Option<&StandardAgentAccount> {
        self.accounts.get(&account_id)
    }

    pub fn apply_signed_owner_call(
        &mut self,
        signed_call: &SignedOwnerCall,
        expected_chain_id: u64,
        block_number: BlockNumber,
        timestamp_ms: Timestamp,
        approved_code_hashes: &[Hash],
        policy_store: &mut PolicyStore,
    ) -> Result<(), AccountError> {
        if signed_call.chain_id != expected_chain_id {
            return Err(AccountError::ChainMismatch {
                expected: expected_chain_id,
                actual: signed_call.chain_id,
            });
        }

        let signing_bytes = canonical_owner_call_bytes(signed_call)?;
        if !verifies_owner_signature(
            &signed_call.call.signer,
            &signing_bytes,
            &signed_call.signature,
        )? {
            return Err(AccountError::InvalidOwnerSignature);
        }

        self.apply_verified_owner_call(
            signed_call.account_id,
            signed_call.chain_id,
            signed_call.call.clone(),
            expected_chain_id,
            block_number,
            timestamp_ms,
            approved_code_hashes,
            policy_store,
        )
    }

    pub fn apply_verified_owner_call(
        &mut self,
        account_id: AccountId,
        chain_id: u64,
        call: OwnerCall,
        expected_chain_id: u64,
        block_number: BlockNumber,
        timestamp_ms: Timestamp,
        approved_code_hashes: &[Hash],
        policy_store: &mut PolicyStore,
    ) -> Result<(), AccountError> {
        if chain_id != expected_chain_id {
            return Err(AccountError::ChainMismatch {
                expected: expected_chain_id,
                actual: chain_id,
            });
        }

        let account = self
            .accounts
            .get(&account_id)
            .cloned()
            .ok_or(AccountError::UnknownAccount(account_id))?;
        let mut next_account = account;
        let mut next_policy_store = policy_store.clone();

        match &call.action {
            OwnerAction::CreatePolicy {
                policy_id,
                grantee,
                expires_at,
                terms,
            } => {
                next_account.apply_owner_call(call.clone(), approved_code_hashes)?;
                next_policy_store
                    .insert(PolicyRecord::new(
                        *policy_id,
                        account_id,
                        *grantee,
                        timestamp_ms,
                        *expires_at,
                        PolicyState::Active,
                        terms.clone(),
                    ))
                    .map_err(AccountError::Policy)?;
            }
            OwnerAction::AddPolicy { policy_id } => {
                if next_policy_store.get(*policy_id).is_none() {
                    return Err(AccountError::Policy(PolicyError::UnknownPolicy(*policy_id)));
                }
                next_account.apply_owner_call(call.clone(), approved_code_hashes)?;
            }
            OwnerAction::RevokePolicy { policy_id } => {
                next_account.apply_owner_call(call.clone(), approved_code_hashes)?;
                next_policy_store
                    .revoke(*policy_id, block_number, timestamp_ms)
                    .map_err(AccountError::Policy)?;
            }
            OwnerAction::RemovePolicy { .. }
            | OwnerAction::ProposeOwner { .. }
            | OwnerAction::AcceptOwner
            | OwnerAction::Pause
            | OwnerAction::Unpause
            | OwnerAction::Upgrade { .. } => {
                next_account.apply_owner_call(call.clone(), approved_code_hashes)?;
            }
        }

        self.accounts.insert(account_id, next_account);
        *policy_store = next_policy_store;
        Ok(())
    }
}

impl PolicyRecord {
    pub fn new(
        id: PolicyId,
        authority_account: AccountId,
        grantee: SessionPubKey,
        created_at: Timestamp,
        expires_at: Timestamp,
        state: PolicyState,
        terms: PolicyTerms,
    ) -> Self {
        Self {
            id,
            authority_account,
            execution_scope: PolicyExecutionScope::direct(authority_account),
            parent_policy_id: None,
            delegation_depth: 0,
            key_state: PolicyKeyState {
                grantee,
                ..PolicyKeyState::default()
            },
            next_intent_nonce: 1,
            created_at,
            expires_at,
            state,
            terms,
            admission_rules: PolicyAdmissionRules::default(),
            origin: PolicyOrigin::Direct,
            reserved: DelegationReservations::default(),
            counters: PolicyCounters {
                volume_today_reset_at: created_at,
                intents_this_sec_start: created_at,
                intents_this_minute_start: created_at,
                intents_today_reset_at: created_at,
                gas_reset_at: created_at,
                ..PolicyCounters::default()
            },
            revoked_at: None,
        }
    }

    pub fn with_origin(mut self, origin: PolicyOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub fn with_admission_rules(mut self, admission_rules: PolicyAdmissionRules) -> Self {
        self.admission_rules = admission_rules;
        self
    }

    pub fn with_execution_scope(mut self, execution_scope: PolicyExecutionScope) -> Self {
        self.execution_scope = execution_scope;
        self
    }

    fn remaining_daily_volume(&self) -> MicroUsdc {
        self.terms
            .max_volume_per_day
            .saturating_sub(self.counters.volume_today)
            .saturating_sub(self.reserved.volume_per_day)
    }

    fn remaining_gas_budget(&self) -> MicroUsdc {
        self.terms
            .gas_budget_per_day
            .saturating_sub(self.counters.gas_used_today)
            .saturating_sub(self.reserved.gas_budget_per_day)
    }
}

impl PolicyStore {
    pub fn insert(&mut self, record: PolicyRecord) -> Result<(), PolicyError> {
        if self.policies.contains_key(&record.id) {
            return Err(PolicyError::DuplicatePolicyId(record.id));
        }
        self.policies.insert(record.id, record);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.policies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    pub fn get(&self, policy_id: PolicyId) -> Option<&PolicyRecord> {
        self.policies.get(&policy_id)
    }

    pub fn get_mut(&mut self, policy_id: PolicyId) -> Option<&mut PolicyRecord> {
        self.policies.get_mut(&policy_id)
    }

    pub fn records(&self) -> impl Iterator<Item = &PolicyRecord> {
        self.policies.values()
    }

    pub fn create_sub_policy(
        &mut self,
        parent_policy_id: PolicyId,
        mut child: PolicyRecord,
    ) -> Result<(), PolicyError> {
        if self.policies.contains_key(&child.id) {
            return Err(PolicyError::DuplicatePolicyId(child.id));
        }

        let parent = self
            .policies
            .get_mut(&parent_policy_id)
            .ok_or(PolicyError::UnknownPolicy(parent_policy_id))?;

        if parent.state != PolicyState::Active {
            return Err(PolicyError::PolicyNotActive(parent.state));
        }

        if child.authority_account != parent.authority_account
            || child.execution_scope != parent.execution_scope
        {
            return Err(PolicyError::SubPolicyScopeMismatch);
        }

        if child.expires_at > parent.expires_at {
            return Err(PolicyError::SubPolicyTermsExceedParent("expires_at"));
        }

        let attempted_depth = parent.delegation_depth.saturating_add(1);
        if attempted_depth > MAX_SUB_POLICY_DEPTH {
            return Err(PolicyError::DelegationDepthExceeded {
                max_depth: MAX_SUB_POLICY_DEPTH,
                attempted_depth,
            });
        }

        if child.terms.max_intents_per_second > parent.terms.max_intents_per_second {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "max_intents_per_second",
            ));
        }
        if child.terms.max_intents_per_day > parent.terms.max_intents_per_day {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "max_intents_per_day",
            ));
        }
        if parent.admission_rules.max_intents_per_minute != 0
            && (child.admission_rules.max_intents_per_minute == 0
                || child.admission_rules.max_intents_per_minute
                    > parent.admission_rules.max_intents_per_minute)
        {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "max_intents_per_minute",
            ));
        }
        if child.terms.gas_refill_per_day > parent.terms.gas_refill_per_day {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "gas_refill_per_day",
            ));
        }
        if child.terms.max_drawdown_pct > parent.terms.max_drawdown_pct {
            return Err(PolicyError::SubPolicyTermsExceedParent("max_drawdown_pct"));
        }
        if child.terms.max_volume_per_day > parent.remaining_daily_volume() {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "max_volume_per_day",
            ));
        }
        if child.terms.gas_budget_per_day > parent.remaining_gas_budget() {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "gas_budget_per_day",
            ));
        }
        if !parent.admission_rules.allowed_markets.is_empty()
            && (child.admission_rules.allowed_markets.is_empty()
                || child
                    .admission_rules
                    .allowed_markets
                    .iter()
                    .any(|market| !parent.admission_rules.allowed_markets.contains(market)))
        {
            return Err(PolicyError::SubPolicyTermsExceedParent("allowed_markets"));
        }
        if limited_child_exceeds_parent(
            child.admission_rules.max_trade_notional,
            parent.admission_rules.max_trade_notional,
        ) {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "max_trade_notional",
            ));
        }
        if limited_child_exceeds_parent(
            child.admission_rules.max_notional_outstanding,
            parent.admission_rules.max_notional_outstanding,
        ) {
            return Err(PolicyError::SubPolicyTermsExceedParent(
                "max_notional_outstanding",
            ));
        }

        parent.reserved.volume_per_day = parent
            .reserved
            .volume_per_day
            .saturating_add(child.terms.max_volume_per_day);
        parent.reserved.gas_budget_per_day = parent
            .reserved
            .gas_budget_per_day
            .saturating_add(child.terms.gas_budget_per_day);

        child.parent_policy_id = Some(parent_policy_id);
        child.delegation_depth = attempted_depth;
        self.policies.insert(child.id, child);
        Ok(())
    }

    pub fn next_intent_nonce(&self, policy_id: PolicyId) -> Result<u64, PolicyError> {
        self.policies
            .get(&policy_id)
            .map(|policy| policy.next_intent_nonce)
            .ok_or(PolicyError::UnknownPolicy(policy_id))
    }

    pub fn revocation_scope(&self, policy_id: PolicyId) -> Result<Vec<PolicyId>, PolicyError> {
        if !self.policies.contains_key(&policy_id) {
            return Err(PolicyError::UnknownPolicy(policy_id));
        }
        let mut scope = Vec::new();
        self.collect_revocation_scope(policy_id, &mut scope);
        Ok(scope)
    }

    pub fn rotate_key(
        &mut self,
        policy_id: PolicyId,
        new_grantee: SessionPubKey,
        block_number: BlockNumber,
    ) -> Result<KeyRotationEvent, PolicyError> {
        let policy = self
            .policies
            .get_mut(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;

        if policy.state != PolicyState::Active {
            return Err(PolicyError::PolicyNotActive(policy.state));
        }

        if policy.key_state.grantee == new_grantee {
            return Err(PolicyError::SessionKeyAlreadyActive);
        }

        if policy.key_state.revoked_grantees.contains_key(&new_grantee) {
            return Err(PolicyError::SessionKeyAlreadyRevoked);
        }

        let old_grantee = policy.key_state.grantee;
        policy
            .key_state
            .revoked_grantees
            .insert(old_grantee, block_number);
        policy.key_state.grantee = new_grantee;

        Ok(KeyRotationEvent {
            policy_id,
            old_grantee,
            new_grantee,
            block_number,
        })
    }

    pub fn verify_session(
        &self,
        policy_id: PolicyId,
        signed_session: &SignedSessionPayload,
        block_number: BlockNumber,
        expected_chain_id: u64,
    ) -> Result<(), PolicyError> {
        let policy = self
            .policies
            .get(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;

        if signed_session.payload.policy_id != policy_id {
            return Err(PolicyError::SessionPolicyMismatch {
                expected: policy_id,
                actual: signed_session.payload.policy_id,
            });
        }

        if signed_session.payload.chain_id != expected_chain_id {
            return Err(PolicyError::SessionChainMismatch {
                expected: expected_chain_id,
                actual: signed_session.payload.chain_id,
            });
        }

        if policy.state != PolicyState::Active {
            return Err(PolicyError::PolicyNotActive(policy.state));
        }

        if policy.key_state.revoked {
            return Err(PolicyError::PolicyNotActive(PolicyState::Revoked));
        }

        let signing_bytes = canonical_session_bytes(&signed_session.payload)?;

        if verifies_session_signature(
            &policy.key_state.grantee,
            &signing_bytes,
            &signed_session.signature,
        )? {
            return Ok(());
        }

        for (revoked_grantee, revoked_at) in &policy.key_state.revoked_grantees {
            if verifies_session_signature(
                revoked_grantee,
                &signing_bytes,
                &signed_session.signature,
            )? {
                if block_number <= *revoked_at {
                    return Ok(());
                }

                return Err(PolicyError::SessionKeyRevoked {
                    revoked_at: *revoked_at,
                    block_number,
                });
            }
        }

        Err(PolicyError::InvalidSessionSignature)
    }

    pub fn session_verification_material(
        &self,
        policy_id: PolicyId,
        signed_session: &SignedSessionPayload,
        expected_chain_id: u64,
    ) -> Result<SessionVerificationMaterial, PolicyError> {
        let policy = self
            .policies
            .get(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;

        if signed_session.payload.policy_id != policy_id {
            return Err(PolicyError::SessionPolicyMismatch {
                expected: policy_id,
                actual: signed_session.payload.policy_id,
            });
        }

        if signed_session.payload.chain_id != expected_chain_id {
            return Err(PolicyError::SessionChainMismatch {
                expected: expected_chain_id,
                actual: signed_session.payload.chain_id,
            });
        }

        if policy.state != PolicyState::Active {
            return Err(PolicyError::PolicyNotActive(policy.state));
        }

        if policy.key_state.revoked {
            return Err(PolicyError::PolicyNotActive(PolicyState::Revoked));
        }

        Ok(SessionVerificationMaterial {
            signing_bytes: canonical_session_bytes(&signed_session.payload)?,
            current_grantee: policy.key_state.grantee,
        })
    }

    pub fn revoke(
        &mut self,
        policy_id: PolicyId,
        block_number: BlockNumber,
        timestamp_ms: Timestamp,
    ) -> Result<(), PolicyError> {
        if !self.policies.contains_key(&policy_id) {
            return Err(PolicyError::UnknownPolicy(policy_id));
        }
        self.cascade_revoke(policy_id, block_number, timestamp_ms);
        Ok(())
    }

    pub fn suspend(&mut self, policy_id: PolicyId) -> Result<(), PolicyError> {
        if !self.policies.contains_key(&policy_id) {
            return Err(PolicyError::UnknownPolicy(policy_id));
        }
        let mut scope = Vec::new();
        self.collect_revocation_scope(policy_id, &mut scope);
        for scoped_policy_id in scope {
            if let Some(policy) = self.policies.get_mut(&scoped_policy_id) {
                if policy.state == PolicyState::Active {
                    // Suspension is reversible, so child reservations stay charged
                    // against parents. Revoke/expire are the terminal paths that
                    // release reserved volume/gas.
                    policy.state = PolicyState::Suspended;
                }
            }
        }
        Ok(())
    }

    pub fn resume(&mut self, policy_id: PolicyId) -> Result<(), PolicyError> {
        if !self.policies.contains_key(&policy_id) {
            return Err(PolicyError::UnknownPolicy(policy_id));
        }
        let mut scope = Vec::new();
        self.collect_revocation_scope(policy_id, &mut scope);
        for scoped_policy_id in scope {
            if let Some(policy) = self.policies.get_mut(&scoped_policy_id) {
                if policy.state == PolicyState::Suspended {
                    policy.state = PolicyState::Active;
                }
            }
        }
        Ok(())
    }

    pub fn expire(
        &mut self,
        policy_id: PolicyId,
        block_number: BlockNumber,
        timestamp_ms: Timestamp,
    ) -> Result<(), PolicyError> {
        if !self.policies.contains_key(&policy_id) {
            return Err(PolicyError::UnknownPolicy(policy_id));
        }
        self.cascade_expire(policy_id, block_number, timestamp_ms);
        Ok(())
    }

    pub fn preview_admission(
        &self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<PolicyAdmissionDelta, PolicyError> {
        self.verify_session(
            policy_id,
            &admission.signed_session,
            admission.block_number,
            admission.expected_chain_id,
        )?;

        let policy = self
            .policies
            .get(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;

        let mut next_counters = self.compute_admission_delta(policy, admission)?;
        next_counters.current_drawdown_pct = admission.current_drawdown_pct;

        if admission.current_drawdown_pct > policy.terms.max_drawdown_pct {
            return Err(PolicyError::DrawdownExceededAndRevoked);
        }

        Ok(PolicyAdmissionDelta {
            counters: next_counters,
        })
    }

    pub fn preview_preverified_admission(
        &self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<PolicyAdmissionDelta, PolicyError> {
        let policy = self
            .policies
            .get(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;

        if admission.signed_session.payload.policy_id != policy_id {
            return Err(PolicyError::SessionPolicyMismatch {
                expected: policy_id,
                actual: admission.signed_session.payload.policy_id,
            });
        }

        if admission.signed_session.payload.chain_id != admission.expected_chain_id {
            return Err(PolicyError::SessionChainMismatch {
                expected: admission.expected_chain_id,
                actual: admission.signed_session.payload.chain_id,
            });
        }

        self.check_admission_limits(policy, admission)
    }

    pub fn preview_admission_limits(
        &self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<(), PolicyError> {
        let policy = self
            .policies
            .get(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;

        if admission.signed_session.payload.chain_id != admission.expected_chain_id {
            return Err(PolicyError::SessionChainMismatch {
                expected: admission.expected_chain_id,
                actual: admission.signed_session.payload.chain_id,
            });
        }

        self.check_admission_limits(policy, admission).map(|_| ())
    }

    pub fn commit_admission_limits(
        &mut self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<(), PolicyError> {
        let delta = {
            let policy = self
                .policies
                .get(&policy_id)
                .ok_or(PolicyError::UnknownPolicy(policy_id))?;

            if admission.signed_session.payload.chain_id != admission.expected_chain_id {
                return Err(PolicyError::SessionChainMismatch {
                    expected: admission.expected_chain_id,
                    actual: admission.signed_session.payload.chain_id,
                });
            }

            self.check_admission_limits(policy, admission)?
        };
        let policy = self
            .policies
            .get_mut(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;
        policy.counters = delta.counters;
        Ok(())
    }

    pub fn commit_admission(
        &mut self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<(), PolicyError> {
        let delta = match self.preview_admission(policy_id, admission) {
            Ok(delta) => delta,
            Err(PolicyError::DrawdownExceededAndRevoked) => {
                if !self.policies.contains_key(&policy_id) {
                    return Err(PolicyError::UnknownPolicy(policy_id));
                }
                self.cascade_revoke(policy_id, admission.block_number, admission.timestamp_ms);
                return Err(PolicyError::DrawdownExceededAndRevoked);
            }
            Err(error) => return Err(error),
        };
        let policy = self
            .policies
            .get_mut(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;
        if admission.intent_nonce != policy.next_intent_nonce {
            return Err(PolicyError::InvalidIntentNonce {
                expected: policy.next_intent_nonce,
                actual: admission.intent_nonce,
            });
        }
        policy.counters = delta.counters;
        policy.next_intent_nonce = policy.next_intent_nonce.saturating_add(1);
        Ok(())
    }

    pub fn commit_preverified_admission(
        &mut self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<(), PolicyError> {
        let delta = match self.preview_preverified_admission(policy_id, admission) {
            Ok(delta) => delta,
            Err(PolicyError::DrawdownExceededAndRevoked) => {
                if !self.policies.contains_key(&policy_id) {
                    return Err(PolicyError::UnknownPolicy(policy_id));
                }
                self.cascade_revoke(policy_id, admission.block_number, admission.timestamp_ms);
                return Err(PolicyError::DrawdownExceededAndRevoked);
            }
            Err(error) => return Err(error),
        };
        let policy = self
            .policies
            .get_mut(&policy_id)
            .ok_or(PolicyError::UnknownPolicy(policy_id))?;
        if admission.intent_nonce != policy.next_intent_nonce {
            return Err(PolicyError::InvalidIntentNonce {
                expected: policy.next_intent_nonce,
                actual: admission.intent_nonce,
            });
        }
        policy.counters = delta.counters;
        policy.next_intent_nonce = policy.next_intent_nonce.saturating_add(1);
        Ok(())
    }

    pub fn admit(
        &mut self,
        policy_id: PolicyId,
        admission: &PolicyAdmission,
    ) -> Result<(), PolicyError> {
        self.commit_admission(policy_id, admission)
    }

    fn compute_admission_delta(
        &self,
        policy: &PolicyRecord,
        admission: &PolicyAdmission,
    ) -> Result<PolicyCounters, PolicyError> {
        if admission.timestamp_ms >= policy.expires_at {
            return Err(PolicyError::PolicyExpired);
        }

        if policy.state != PolicyState::Active {
            return Err(PolicyError::PolicyNotActive(policy.state));
        }

        let mut next_counters = policy.counters.clone();
        reset_time_windows(&mut next_counters, admission.timestamp_ms);

        let market_scope_exempt_economically_inert_intent = admission.market_id.is_none()
            && admission.volume == 0
            && admission.intent_notional == 0;

        if !policy.admission_rules.allowed_markets.is_empty()
            && !market_scope_exempt_economically_inert_intent
        {
            let market_id = admission.market_id.ok_or(PolicyError::MarketNotAllowed)?;
            if !policy.admission_rules.allowed_markets.contains(&market_id) {
                return Err(PolicyError::MarketNotAllowed);
            }
        }

        if policy.admission_rules.max_trade_notional != 0
            && admission.intent_notional > policy.admission_rules.max_trade_notional
        {
            return Err(PolicyError::TradeNotionalExceeded);
        }

        if policy.admission_rules.max_notional_outstanding != 0
            && admission.notional_outstanding > policy.admission_rules.max_notional_outstanding
        {
            return Err(PolicyError::NotionalOutstandingExceeded);
        }

        next_counters.intents_this_second = checked_add_u32(next_counters.intents_this_second, 1)?;
        if next_counters.intents_this_second > policy.terms.max_intents_per_second {
            return Err(PolicyError::IntentRateLimitExceeded);
        }

        next_counters.intents_this_minute = checked_add_u32(next_counters.intents_this_minute, 1)?;
        if policy.admission_rules.max_intents_per_minute != 0
            && next_counters.intents_this_minute > policy.admission_rules.max_intents_per_minute
        {
            return Err(PolicyError::MinuteIntentLimitExceeded);
        }

        next_counters.intents_today = checked_add_u32(next_counters.intents_today, 1)?;
        if next_counters.intents_today > policy.terms.max_intents_per_day {
            return Err(PolicyError::DailyIntentLimitExceeded);
        }

        next_counters.volume_today =
            checked_add_u128(next_counters.volume_today, admission.volume)?;
        if next_counters.volume_today > policy.terms.max_volume_per_day {
            return Err(PolicyError::DailyVolumeExceeded);
        }

        next_counters.gas_used_today =
            checked_add_u128(next_counters.gas_used_today, admission.gas_cost)?;
        if next_counters.gas_used_today > policy.terms.gas_budget_per_day {
            return Err(PolicyError::GasBudgetExceeded);
        }

        next_counters.notional_outstanding = admission.notional_outstanding;
        next_counters.realized_pnl =
            checked_add_i128(next_counters.realized_pnl, admission.realized_pnl_delta)?;
        next_counters.peak_pnl = next_counters.peak_pnl.max(next_counters.realized_pnl);

        Ok(next_counters)
    }

    fn check_admission_limits(
        &self,
        policy: &PolicyRecord,
        admission: &PolicyAdmission,
    ) -> Result<PolicyAdmissionDelta, PolicyError> {
        if policy.state != PolicyState::Active {
            return Err(PolicyError::PolicyNotActive(policy.state));
        }

        if policy.key_state.revoked {
            return Err(PolicyError::PolicyNotActive(PolicyState::Revoked));
        }

        let mut next_counters = self.compute_admission_delta(policy, admission)?;
        next_counters.current_drawdown_pct = admission.current_drawdown_pct;

        if admission.current_drawdown_pct > policy.terms.max_drawdown_pct {
            return Err(PolicyError::DrawdownExceededAndRevoked);
        }

        Ok(PolicyAdmissionDelta {
            counters: next_counters,
        })
    }

    fn cascade_revoke(
        &mut self,
        policy_id: PolicyId,
        block_number: BlockNumber,
        timestamp_ms: Timestamp,
    ) {
        let (parent_policy_id, reserved_volume, reserved_gas) = {
            let policy = self
                .policies
                .get_mut(&policy_id)
                .expect("policy existence checked before cascade");

            if policy.state == PolicyState::Revoked {
                return;
            }

            policy.state = PolicyState::Revoked;
            policy.key_state.revoked = true;
            policy.key_state.revoked_at = Some(block_number);
            policy.revoked_at = Some(timestamp_ms);

            (
                policy.parent_policy_id,
                policy.terms.max_volume_per_day,
                policy.terms.gas_budget_per_day,
            )
        };

        if let Some(parent_policy_id) = parent_policy_id {
            if let Some(parent) = self.policies.get_mut(&parent_policy_id) {
                parent.reserved.volume_per_day = parent
                    .reserved
                    .volume_per_day
                    .saturating_sub(reserved_volume);
                parent.reserved.gas_budget_per_day = parent
                    .reserved
                    .gas_budget_per_day
                    .saturating_sub(reserved_gas);
            }
        }

        let child_ids: Vec<_> = self
            .policies
            .values()
            .filter(|candidate| candidate.parent_policy_id == Some(policy_id))
            .map(|candidate| candidate.id)
            .collect();

        for child_id in child_ids {
            self.cascade_revoke(child_id, block_number, timestamp_ms);
        }
    }

    fn cascade_expire(
        &mut self,
        policy_id: PolicyId,
        block_number: BlockNumber,
        timestamp_ms: Timestamp,
    ) {
        let (parent_policy_id, reserved_volume, reserved_gas) = {
            let policy = self
                .policies
                .get_mut(&policy_id)
                .expect("policy existence checked before cascade");

            if matches!(policy.state, PolicyState::Revoked | PolicyState::Expired) {
                return;
            }

            policy.state = PolicyState::Expired;
            policy.key_state.revoked = true;
            policy.key_state.revoked_at = Some(block_number);
            policy.revoked_at = Some(timestamp_ms);

            (
                policy.parent_policy_id,
                policy.terms.max_volume_per_day,
                policy.terms.gas_budget_per_day,
            )
        };

        if let Some(parent_policy_id) = parent_policy_id {
            if let Some(parent) = self.policies.get_mut(&parent_policy_id) {
                parent.reserved.volume_per_day = parent
                    .reserved
                    .volume_per_day
                    .saturating_sub(reserved_volume);
                parent.reserved.gas_budget_per_day = parent
                    .reserved
                    .gas_budget_per_day
                    .saturating_sub(reserved_gas);
            }
        }

        let child_ids: Vec<_> = self
            .policies
            .values()
            .filter(|candidate| candidate.parent_policy_id == Some(policy_id))
            .map(|candidate| candidate.id)
            .collect();

        for child_id in child_ids {
            self.cascade_expire(child_id, block_number, timestamp_ms);
        }
    }

    fn collect_revocation_scope(&self, policy_id: PolicyId, scope: &mut Vec<PolicyId>) {
        scope.push(policy_id);
        let child_ids: Vec<_> = self
            .policies
            .values()
            .filter(|candidate| candidate.parent_policy_id == Some(policy_id))
            .map(|candidate| candidate.id)
            .collect();

        for child_id in child_ids {
            self.collect_revocation_scope(child_id, scope);
        }
    }
}

fn canonical_session_bytes(payload: &SessionPayload) -> Result<Vec<u8>, PolicyError> {
    borsh::to_vec(payload).map_err(|_| PolicyError::InvalidSessionSignature)
}

fn canonical_owner_call_bytes(payload: &SignedOwnerCall) -> Result<Vec<u8>, AccountError> {
    borsh::to_vec(&(payload.account_id, payload.chain_id, &payload.call))
        .map_err(|_| AccountError::InvalidOwnerSignature)
}

fn verifies_session_signature(
    pubkey: &SessionPubKey,
    message: &[u8],
    signature: &SessionSignature,
) -> Result<bool, PolicyError> {
    let verifying_key =
        VerifyingKey::from_bytes(pubkey).map_err(|_| PolicyError::InvalidSessionKeyMaterial)?;
    let signature = Signature::from_bytes(signature);
    Ok(verifying_key.verify_strict(message, &signature).is_ok())
}

fn verifies_owner_signature(
    pubkey: &Principal,
    message: &[u8],
    signature: &SessionSignature,
) -> Result<bool, AccountError> {
    let verifying_key =
        VerifyingKey::from_bytes(pubkey).map_err(|_| AccountError::InvalidOwnerSignature)?;
    let signature = Signature::from_bytes(signature);
    Ok(verifying_key.verify_strict(message, &signature).is_ok())
}

fn reset_time_windows(counters: &mut PolicyCounters, timestamp_ms: Timestamp) {
    if bucket_changed(counters.intents_this_sec_start, timestamp_ms, SECOND_MS) {
        counters.intents_this_second = 0;
        counters.intents_this_sec_start = timestamp_ms;
    }

    if bucket_changed(counters.intents_this_minute_start, timestamp_ms, MINUTE_MS) {
        counters.intents_this_minute = 0;
        counters.intents_this_minute_start = timestamp_ms;
    }

    if bucket_changed(counters.volume_today_reset_at, timestamp_ms, DAY_MS) {
        counters.volume_today = 0;
        counters.volume_today_reset_at = timestamp_ms;
    }

    if bucket_changed(counters.intents_today_reset_at, timestamp_ms, DAY_MS) {
        counters.intents_today = 0;
        counters.intents_today_reset_at = timestamp_ms;
    }

    if bucket_changed(counters.gas_reset_at, timestamp_ms, DAY_MS) {
        counters.gas_used_today = 0;
        counters.gas_reset_at = timestamp_ms;
    }
}

fn bucket_changed(previous: Timestamp, current: Timestamp, window_ms: u64) -> bool {
    previous / window_ms != current / window_ms
}

fn limited_child_exceeds_parent(child_limit: MicroUsdc, parent_limit: MicroUsdc) -> bool {
    parent_limit != 0 && (child_limit == 0 || child_limit > parent_limit)
}

fn checked_add_u32(left: u32, right: u32) -> Result<u32, PolicyError> {
    left.checked_add(right).ok_or(PolicyError::Overflow)
}

fn checked_add_u128(left: u128, right: u128) -> Result<u128, PolicyError> {
    left.checked_add(right).ok_or(PolicyError::Overflow)
}

fn checked_add_i128(left: i128, right: i128) -> Result<i128, PolicyError> {
    left.checked_add(right).ok_or(PolicyError::Overflow)
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
