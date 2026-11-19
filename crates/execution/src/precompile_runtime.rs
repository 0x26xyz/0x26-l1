use crate::*;
use borsh::BorshDeserialize;
use std::str;

pub(crate) fn evm_account_nonce_key(address: Address) -> String {
    format!(
        "{EVM_ACCOUNT_NONCE_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn evm_account_balance_key(address: Address) -> String {
    format!(
        "{EVM_ACCOUNT_BALANCE_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn evm_account_code_key(address: Address) -> String {
    format!(
        "{EVM_ACCOUNT_CODE_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn evm_storage_key(address: Address, slot: StorageKey) -> String {
    format!(
        "{EVM_STORAGE_PREFIX}{}/{}",
        encode_hex(address.as_slice()),
        encode_hex(&slot.to_be_bytes::<32>())
    )
}

pub(crate) fn evm_block_hash_key(number: u64) -> String {
    format!("{EVM_BLOCK_HASH_PREFIX}{number}")
}

pub(crate) fn stage_parent_block_hash<Checkpoint>(
    context: &ExecutionContext,
    parent_hash: Hash,
    checkpoint: &mut Checkpoint,
) where
    Checkpoint: StateCheckpoint,
{
    if let Some(parent_height) = context.height.checked_sub(1) {
        checkpoint.set(evm_block_hash_key(parent_height).as_bytes(), &parent_hash);
        if let Some(prune_height) = parent_height.checked_sub(RETAINED_BLOCK_HASH_WINDOW_BLOCKS) {
            // Per-block pruning keeps steady-state mirrors bounded. A future
            // migration from a pre-pruning snapshot may still want a one-time
            // bulk prune during restore to shrink old history faster.
            checkpoint.delete(evm_block_hash_key(prune_height).as_bytes());
        }
    }
}

pub(crate) fn evm_usdc_system_address() -> Address {
    address_from_u64(0x1014)
}

pub(crate) fn evm_usdc_balance_key(address: Address) -> String {
    format!(
        "{EVM_USDC_BALANCE_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn evm_usdc_allowance_key(owner: Address, spender: Address) -> String {
    format!(
        "{EVM_USDC_ALLOWANCE_PREFIX}{}/{}",
        encode_hex(owner.as_slice()),
        encode_hex(spender.as_slice())
    )
}

pub(crate) fn policy_usdc_balance_key(address: Address) -> String {
    format!(
        "{POLICY_USDC_BALANCE_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn account_evm_address_key(account_id: u64) -> String {
    format!("{ACCOUNT_EVM_ADDRESS_PREFIX}{account_id}")
}

pub(crate) fn evm_address_account_key(address: Address) -> String {
    format!(
        "{EVM_ADDRESS_ACCOUNT_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn evm_usdc_minter_role_key(address: Address) -> String {
    format!(
        "{EVM_USDC_MINTER_ROLE_PREFIX}{}",
        encode_hex(address.as_slice())
    )
}

pub(crate) fn read_u256_key<Checkpoint>(checkpoint: &mut Checkpoint, key: &str) -> U256
where
    Checkpoint: StateCheckpoint,
{
    checkpoint
        .get(key.as_bytes())
        .map(|bytes| U256::from_be_slice(&bytes))
        .unwrap_or(U256::ZERO)
}

pub(crate) fn write_u256_key<Checkpoint>(checkpoint: &mut Checkpoint, key: &str, value: U256)
where
    Checkpoint: StateCheckpoint,
{
    if value.is_zero() {
        checkpoint.delete(key.as_bytes());
    } else {
        checkpoint.set(key.as_bytes(), &value.to_be_bytes::<32>());
    }
}

pub(crate) fn read_evm_usdc_balance<Checkpoint>(
    checkpoint: &mut Checkpoint,
    address: Address,
) -> U256
where
    Checkpoint: StateCheckpoint,
{
    read_u256_key(checkpoint, &evm_usdc_balance_key(address))
}

pub(crate) fn write_evm_usdc_balance<Checkpoint>(
    checkpoint: &mut Checkpoint,
    address: Address,
    value: U256,
) where
    Checkpoint: StateCheckpoint,
{
    write_u256_key(checkpoint, &evm_usdc_balance_key(address), value);
}

pub(crate) fn read_evm_usdc_allowance<Checkpoint>(
    checkpoint: &mut Checkpoint,
    owner: Address,
    spender: Address,
) -> U256
where
    Checkpoint: StateCheckpoint,
{
    read_u256_key(checkpoint, &evm_usdc_allowance_key(owner, spender))
}

pub(crate) fn write_evm_usdc_allowance<Checkpoint>(
    checkpoint: &mut Checkpoint,
    owner: Address,
    spender: Address,
    value: U256,
) where
    Checkpoint: StateCheckpoint,
{
    write_u256_key(checkpoint, &evm_usdc_allowance_key(owner, spender), value);
}

pub(crate) fn read_policy_usdc_balance<Checkpoint>(
    checkpoint: &mut Checkpoint,
    address: Address,
) -> U256
where
    Checkpoint: StateCheckpoint,
{
    read_u256_key(checkpoint, &policy_usdc_balance_key(address))
}

pub(crate) fn write_policy_usdc_balance<Checkpoint>(
    checkpoint: &mut Checkpoint,
    address: Address,
    value: U256,
) where
    Checkpoint: StateCheckpoint,
{
    write_u256_key(checkpoint, &policy_usdc_balance_key(address), value);
}

pub(crate) fn read_evm_usdc_total_supply<Checkpoint>(checkpoint: &mut Checkpoint) -> U256
where
    Checkpoint: StateCheckpoint,
{
    read_u256_key(checkpoint, EVM_USDC_TOTAL_SUPPLY_KEY)
}

pub(crate) fn write_evm_usdc_total_supply<Checkpoint>(checkpoint: &mut Checkpoint, value: U256)
where
    Checkpoint: StateCheckpoint,
{
    write_u256_key(checkpoint, EVM_USDC_TOTAL_SUPPLY_KEY, value);
}

pub(crate) fn has_evm_usdc_minter_role<Checkpoint>(
    checkpoint: &mut Checkpoint,
    address: Address,
) -> bool
where
    Checkpoint: StateCheckpoint,
{
    checkpoint
        .get(evm_usdc_minter_role_key(address).as_bytes())
        .is_some_and(|bytes| bytes.iter().any(|byte| *byte != 0))
}

pub(crate) fn dispatch_session_key<Checkpoint>(
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    if selector != SESSION_KEY_AUTHORIZE {
        return Err(PrecompileError::UnknownSelector {
            address: descriptor.address,
            selector,
        });
    }
    if input.len() < 8 {
        return Err(PrecompileError::InvalidInput(
            "session_key input must include policy id",
        ));
    }

    let policy_id = u64::from_be_bytes(
        input[0..8]
            .try_into()
            .expect("slice length already checked"),
    );
    let storage_key = format!("precompile/session-key/{policy_id}");
    checkpoint.set(storage_key.as_bytes(), input);

    Ok(success_output(
        policy_id.to_be_bytes().to_vec(),
        fixed_gas_cost(descriptor, input.len(), 8),
    ))
}

pub(crate) struct ProtocolStateAccessor<'a, Checkpoint> {
    checkpoint: &'a mut Checkpoint,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'a, Checkpoint> ProtocolStateAccessor<'a, Checkpoint>
where
    Checkpoint: StateCheckpoint,
{
    pub(crate) fn new(checkpoint: &'a mut Checkpoint) -> Self {
        Self { checkpoint }
    }

    pub(crate) fn policy_store(&self) -> PolicyStore {
        self.checkpoint
            .get(POLICY_STORE_STATE_KEY)
            .map(|bytes| {
                PolicyStore::try_from_slice(&bytes).expect("policy store state should deserialize")
            })
            .unwrap_or_default()
    }

    pub(crate) fn set_policy_store(&mut self, store: &PolicyStore) {
        let bytes = borsh::to_vec(store).expect("policy store should serialize");
        self.checkpoint.set(POLICY_STORE_STATE_KEY, &bytes);
    }

    pub(crate) fn pending_intent(&self, tracking_id: &Hash) -> Option<PendingIntentRecord> {
        self.checkpoint
            .get(&pending_intent_state_key(tracking_id))
            .map(|bytes| {
                PendingIntentRecord::try_from_slice(&bytes)
                    .expect("pending intent state should deserialize")
            })
    }

    pub(crate) fn set_pending_intent(&mut self, record: &PendingIntentRecord) {
        let bytes = borsh::to_vec(record).expect("pending intent should serialize");
        self.checkpoint
            .set(&pending_intent_state_key(&record.tracking_id), &bytes);
    }

    pub(crate) fn order_book(&self, market_id: u64) -> Option<SingleMarketBook> {
        self.checkpoint
            .get(order_book_state_key(market_id).as_bytes())
            .map(|bytes| {
                SingleMarketBook::try_from_slice(&bytes).expect("order book should deserialize")
            })
    }

    pub(crate) fn set_order_book(&mut self, market_id: u64, book: &SingleMarketBook) {
        let bytes = borsh::to_vec(book).expect("order book should serialize");
        self.checkpoint
            .set(order_book_state_key(market_id).as_bytes(), &bytes);
    }

    pub(crate) fn book_sequence(&self, market_id: u64) -> u64 {
        self.checkpoint
            .get(book_sequence_state_key(market_id).as_bytes())
            .map(|bytes| decode_u64_word(&bytes).expect("book sequence should decode"))
            .unwrap_or(0)
    }

    pub(crate) fn set_book_sequence(&mut self, market_id: u64, sequence: u64) {
        self.checkpoint.set(
            book_sequence_state_key(market_id).as_bytes(),
            &sequence.to_be_bytes(),
        );
    }

    pub(crate) fn market_id_for_label(&self, label: &str) -> Option<u64> {
        self.checkpoint
            .get(market_label_state_key(label).as_bytes())
            .map(|bytes| decode_u64_word(&bytes).expect("market id mapping should decode"))
    }

    pub(crate) fn set_market_label(&mut self, label: &str, market_id: u64) {
        self.checkpoint.set(
            market_label_state_key(label).as_bytes(),
            &market_id.to_be_bytes(),
        );
    }

    pub(crate) fn oracle_mark(&self, market_id: u64) -> Option<OracleMarkRecord> {
        self.checkpoint
            .get(oracle_mark_state_key(market_id).as_bytes())
            .map(|bytes| {
                OracleMarkRecord::try_from_slice(&bytes)
                    .expect("oracle mark state should deserialize")
            })
    }

    pub(crate) fn set_oracle_mark(&mut self, market_id: u64, mark: &OracleMarkRecord) {
        let bytes = borsh::to_vec(mark).expect("oracle mark should serialize");
        self.checkpoint
            .set(oracle_mark_state_key(market_id).as_bytes(), &bytes);
    }

    pub(crate) fn oracle_config(&self, market_id: u64) -> OracleConfig {
        self.checkpoint
            .get(oracle_config_state_key(market_id).as_bytes())
            .map(|bytes| {
                OracleConfig::try_from_slice(&bytes).expect("oracle config should deserialize")
            })
            .unwrap_or_default()
    }

    pub(crate) fn set_oracle_config(&mut self, market_id: u64, config: &OracleConfig) {
        let bytes = borsh::to_vec(config).expect("oracle config should serialize");
        self.checkpoint
            .set(oracle_config_state_key(market_id).as_bytes(), &bytes);
    }

    pub(crate) fn market_paused(&self, market_id: u64) -> bool {
        self.checkpoint
            .get(paused_market_state_key(market_id).as_bytes())
            .is_some()
    }

    pub(crate) fn set_market_paused(&mut self, market_id: u64, paused: bool) {
        let key = paused_market_state_key(market_id);
        if paused {
            self.checkpoint.set(key.as_bytes(), &[1]);
        } else {
            self.checkpoint.delete(key.as_bytes());
        }
    }

    pub(crate) fn active_attestation_vendor_roots(
        &self,
        vendor: AttestationVerifyVendor,
        block_height: u64,
    ) -> Vec<AttestationVendorRoot> {
        self.attestation_vendor_roots(vendor)
            .into_iter()
            .filter(|root| {
                root.valid_from_block <= block_height
                    && root
                        .valid_until_block
                        .is_none_or(|valid_until| block_height < valid_until)
            })
            .collect()
    }

    pub(crate) fn attestation_vendor_roots(
        &self,
        vendor: AttestationVerifyVendor,
    ) -> Vec<AttestationVendorRoot> {
        self.checkpoint
            .get(&attestation_vendor_roots_state_key(vendor))
            .map(|bytes| {
                AttestationVendorRootsState::try_from_slice(&bytes)
                    .expect("attestation vendor roots state should deserialize")
            })
            .filter(|state| state.vendor == vendor)
            .map(|state| state.roots)
            .unwrap_or_default()
    }

    pub(crate) fn evm_block_hash(&self, number: u64) -> Option<Hash> {
        self.checkpoint
            .get(evm_block_hash_key(number).as_bytes())
            .and_then(|bytes| {
                if bytes.len() < 32 {
                    return None;
                }
                let mut hash = [0_u8; 32];
                hash.copy_from_slice(&bytes[..32]);
                Some(hash)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SubmissionResult {
    status: SubmissionStatus,
    tracking_id: Hash,
    order_id: u64,
    reason: SubmissionReason,
    nonce_consumed: bool,
}

pub(crate) fn dispatch_trading_intent_submit<Checkpoint>(
    context: &ExecutionContext,
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    if selector != SUBMIT_SIGNED_INTENT_SELECTOR {
        return Err(PrecompileError::UnknownSelector {
            address: descriptor.address,
            selector,
        });
    }

    let signed_intent = SignedIntent::try_from_slice(input)
        .map_err(|_| PrecompileError::InvalidInput("trading submit expects borsh SignedIntent"))?;
    let mut state = ProtocolStateAccessor::new(checkpoint);
    let submission = admit_trading_intent(context, &mut state, signed_intent);
    let returned = encode_submission_result(submission);
    let gas_used = fixed_gas_cost(descriptor, input.len(), returned.len());
    Ok(success_output(returned, gas_used))
}

pub(crate) fn dispatch_trading_intent_submit_evm<Checkpoint>(
    context: &ExecutionContext,
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    if selector != SUBMIT_SIGNED_INTENT_SELECTOR {
        return Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_unknown_selector_error(selector),
        ));
    }

    let signed_intent_bytes = match decode_single_bytes_argument(input) {
        Some(bytes) => bytes,
        None => {
            return Ok(reverted_output(
                fixed_gas_cost(descriptor, input.len(), 0),
                encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
            ));
        }
    };

    let signed_intent = match SignedIntent::try_from_slice(signed_intent_bytes) {
        Ok(signed_intent) => signed_intent,
        Err(_) => {
            return Ok(reverted_output(
                fixed_gas_cost(descriptor, signed_intent_bytes.len(), 0),
                encode_zero_arg_error(MALFORMED_SIGNED_INTENT_ERROR_SELECTOR),
            ));
        }
    };

    let mut state = ProtocolStateAccessor::new(checkpoint);
    let submission = admit_trading_intent(context, &mut state, signed_intent);
    let returned = encode_submission_result(submission);
    let gas_used = fixed_gas_cost(descriptor, signed_intent_bytes.len(), returned.len());
    Ok(success_output(returned, gas_used))
}

pub(crate) fn dispatch_clob_read<Checkpoint>(
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    let state = ProtocolStateAccessor::new(checkpoint);
    let (returned, gas_used) = match selector {
        BEST_BID_ASK_SELECTOR => {
            let market_id = decode_u64_word(input).map_err(|_| {
                PrecompileError::InvalidInput("bestBidAsk expects uint64 market id")
            })?;
            let book = state
                .order_book(market_id)
                .unwrap_or_else(|| SingleMarketBook::new(market_id));
            let returned = encode_best_bid_ask_response(&book, state.book_sequence(market_id), 0);
            (
                returned.clone(),
                fixed_gas_cost(descriptor, input.len(), returned.len()),
            )
        }
        DEPTH_SELECTOR => {
            let (market_id, side, depth_limit) = decode_depth_words(input).map_err(|_| {
                PrecompileError::InvalidInput("depth expects market id, side, depth limit")
            })?;
            let book = state
                .order_book(market_id)
                .unwrap_or_else(|| SingleMarketBook::new(market_id));
            let levels = match side {
                0 => book.bid_levels(0, depth_limit as usize),
                1 => book.ask_levels(0, depth_limit as usize),
                _ => {
                    return Err(PrecompileError::InvalidInput(
                        "depth side must be 0 for bids or 1 for asks",
                    ))
                }
            };
            let returned = encode_depth_response(&levels, state.book_sequence(market_id));
            (
                returned.clone(),
                base_and_level_gas_cost(7_000, levels.len(), returned.len()),
            )
        }
        _ => {
            return Err(PrecompileError::UnknownSelector {
                address: descriptor.address,
                selector,
            })
        }
    };

    Ok(success_output(returned, gas_used))
}

pub(crate) fn dispatch_clob_read_evm<Checkpoint>(
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    let state = ProtocolStateAccessor::new(checkpoint);
    match selector {
        BEST_BID_ASK_SELECTOR => {
            let market = match decode_fixed_bytes32(input) {
                Some(word) => word,
                None => {
                    return Ok(reverted_output(
                        fixed_gas_cost(descriptor, input.len(), 0),
                        encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                    ));
                }
            };
            let label = match decode_market_label(market) {
                Ok(label) => label,
                Err(revert) => {
                    return Ok(reverted_output(
                        fixed_gas_cost(descriptor, input.len(), 0),
                        revert,
                    ));
                }
            };
            let Some(market_id) = state.market_id_for_label(&label) else {
                return Ok(reverted_output(
                    fixed_gas_cost(descriptor, input.len(), 0),
                    encode_one_word_error(UNKNOWN_MARKET_ERROR_SELECTOR, market),
                ));
            };
            let book = state
                .order_book(market_id)
                .unwrap_or_else(|| SingleMarketBook::new(market_id));
            let returned = encode_best_bid_ask_response(&book, state.book_sequence(market_id), 0);
            let gas_used = fixed_gas_cost(descriptor, input.len(), returned.len());
            Ok(success_output(returned, gas_used))
        }
        DEPTH_SELECTOR => {
            let Some((market, side, depth_limit)) = decode_depth_call(input) else {
                return Ok(reverted_output(
                    fixed_gas_cost(descriptor, input.len(), 0),
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            if depth_limit == 0 || depth_limit > MAX_DEPTH_LEVELS {
                return Ok(reverted_output(
                    fixed_gas_cost(descriptor, input.len(), 0),
                    encode_two_word_error(
                        DEPTH_LIMIT_OUT_OF_RANGE_ERROR_SELECTOR,
                        depth_limit as u64,
                        MAX_DEPTH_LEVELS as u64,
                    ),
                ));
            }
            let label = match decode_market_label(market) {
                Ok(label) => label,
                Err(revert) => {
                    return Ok(reverted_output(
                        fixed_gas_cost(descriptor, input.len(), 0),
                        revert,
                    ));
                }
            };
            let Some(market_id) = state.market_id_for_label(&label) else {
                return Ok(reverted_output(
                    fixed_gas_cost(descriptor, input.len(), 0),
                    encode_one_word_error(UNKNOWN_MARKET_ERROR_SELECTOR, market),
                ));
            };
            let book = state
                .order_book(market_id)
                .unwrap_or_else(|| SingleMarketBook::new(market_id));
            let levels = match side {
                0 => book.bid_levels(0, depth_limit as usize),
                1 => book.ask_levels(0, depth_limit as usize),
                _ => {
                    return Ok(reverted_output(
                        fixed_gas_cost(descriptor, input.len(), 0),
                        encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                    ));
                }
            };
            let returned = encode_depth_response(&levels, state.book_sequence(market_id));
            let gas_used = base_and_level_gas_cost(7_000, levels.len(), returned.len());
            Ok(success_output(returned, gas_used))
        }
        _ => Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_unknown_selector_error(selector),
        )),
    }
}

pub(crate) fn dispatch_oracle_read<Checkpoint>(
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    if selector != MARK_PRICE_SELECTOR {
        return Err(PrecompileError::UnknownSelector {
            address: descriptor.address,
            selector,
        });
    }
    let market_id = decode_u64_word(input)
        .map_err(|_| PrecompileError::InvalidInput("markPrice expects uint64 market id"))?;
    let state = ProtocolStateAccessor::new(checkpoint);
    let mark = state
        .oracle_mark(market_id)
        .ok_or(PrecompileError::InvalidInput("unknown market"))?;
    let returned = encode_oracle_mark_response(&mark);
    let gas_used = fixed_gas_cost(descriptor, input.len(), returned.len());
    Ok(success_output(returned, gas_used))
}

pub(crate) fn dispatch_oracle_read_evm<Checkpoint>(
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    if selector != MARK_PRICE_SELECTOR {
        return Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_unknown_selector_error(selector),
        ));
    }
    let market = match decode_fixed_bytes32(input) {
        Some(word) => word,
        None => {
            return Ok(reverted_output(
                fixed_gas_cost(descriptor, input.len(), 0),
                encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
            ));
        }
    };
    let label = match decode_market_label(market) {
        Ok(label) => label,
        Err(revert) => {
            return Ok(reverted_output(
                fixed_gas_cost(descriptor, input.len(), 0),
                revert,
            ));
        }
    };
    let state = ProtocolStateAccessor::new(checkpoint);
    let Some(market_id) = state.market_id_for_label(&label) else {
        return Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_one_word_error(UNKNOWN_MARKET_ERROR_SELECTOR, market),
        ));
    };
    let Some(mark) = state.oracle_mark(market_id) else {
        return Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_one_word_error(UNKNOWN_MARKET_ERROR_SELECTOR, market),
        ));
    };
    let returned = encode_oracle_mark_response(&mark);
    let gas_used = fixed_gas_cost(descriptor, input.len(), returned.len());
    Ok(success_output(returned, gas_used))
}

pub(crate) fn dispatch_attestation_verify(
    context: &ExecutionContext,
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut impl StateCheckpoint,
) -> Result<PrecompileOutput, PrecompileError> {
    if selector != ATTESTATION_VERIFY_SELECTOR {
        return Err(PrecompileError::UnknownSelector {
            address: descriptor.address,
            selector,
        });
    }
    let call = decode_attestation_verify_input(input).ok_or(PrecompileError::InvalidInput(
        "attestationVerify expects ABI encoded vendor, quote bytes, nonce, freshness anchor, expected claim",
    ))?;
    let outcome = verify_attestation_quote(context, checkpoint, call);
    let returned = encode_attestation_verify_outcome(outcome);
    let gas_used = fixed_gas_cost(descriptor, input.len(), returned.len());
    Ok(success_output(returned, gas_used))
}

pub(crate) fn dispatch_attestation_verify_evm(
    context: &ExecutionContext,
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut impl StateCheckpoint,
) -> Result<PrecompileOutput, PrecompileError> {
    if selector != ATTESTATION_VERIFY_SELECTOR {
        return Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_unknown_selector_error(selector),
        ));
    }
    let Some(call) = decode_attestation_verify_input(input) else {
        return Ok(reverted_output(
            fixed_gas_cost(descriptor, input.len(), 0),
            encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
        ));
    };
    let outcome = verify_attestation_quote(context, checkpoint, call);
    let returned = encode_attestation_verify_outcome(outcome);
    let gas_used = fixed_gas_cost(descriptor, input.len(), returned.len());
    Ok(success_output(returned, gas_used))
}

pub fn verify_attestation_quote<Checkpoint>(
    context: &ExecutionContext,
    checkpoint: &mut Checkpoint,
    request: AttestationVerifyRequest<'_>,
) -> AttestationVerifyOutcome
where
    Checkpoint: StateCheckpoint,
{
    // The verifier is read-only; `&mut Checkpoint` matches the state API used
    // by view precompiles and lets this helper share their accessor path.
    // TODO(RFC 051 verifier): this public entry point stays scaffold-backed
    // until vendor-specific quote verification replaces attest_verify_scaffold.
    let state = ProtocolStateAccessor::new(checkpoint);
    attest_verify_scaffold(context, &state, &request)
}

pub(crate) fn attest_verify_scaffold<Checkpoint>(
    context: &ExecutionContext,
    state: &ProtocolStateAccessor<'_, Checkpoint>,
    call: &AttestationVerifyRequest<'_>,
) -> AttestationVerifyOutcome
where
    Checkpoint: StateCheckpoint,
{
    let active_roots = state.active_attestation_vendor_roots(call.vendor, context.height);
    let failure_reason = if call.quote_bytes.len() > max_attestation_quote_bytes(call.vendor) {
        AttestationVerifyFailure::QuoteTooLarge
    } else if active_roots.is_empty() {
        inactive_vendor_root_failure(state, call.vendor, context.height)
    } else if decode_attestation_verify_claim(call.expected_claim).is_none() {
        AttestationVerifyFailure::ClaimMismatch
    } else if !active_vendor_roots_have_decodable_payload(&active_roots, call.vendor) {
        AttestationVerifyFailure::CertChainInvalid
    } else if !attestation_freshness_satisfied(context, state, call) {
        AttestationVerifyFailure::StaleNonce
    } else {
        // TODO(RFC 051 verifier): verify the decoded RFC 050 AttestationClaim
        // against quote-bound code_hash/model_hash/mandate_id using the
        // deterministic vendor-specific verifier before returning success.
        AttestationVerifyFailure::QuoteFormatVersionUnsupported
    };
    AttestationVerifyOutcome {
        verified: false,
        code_hash: [0; 32],
        model_hash: [0; 32],
        enclave_id: [0; 32],
        signer: [0; 32],
        nonce: call.nonce,
        vendor_root_id: [0; 32],
        failure_reason,
    }
}

pub(crate) fn inactive_vendor_root_failure<Checkpoint>(
    state: &ProtocolStateAccessor<'_, Checkpoint>,
    vendor: AttestationVerifyVendor,
    block_height: u64,
) -> AttestationVerifyFailure
where
    Checkpoint: StateCheckpoint,
{
    if state.attestation_vendor_roots(vendor).iter().any(|root| {
        root.valid_until_block
            .is_some_and(|valid_until| block_height >= valid_until)
    }) {
        AttestationVerifyFailure::Revoked
    } else {
        AttestationVerifyFailure::UnknownVendorRoot
    }
}

pub(crate) fn attestation_freshness_satisfied<Checkpoint>(
    context: &ExecutionContext,
    state: &ProtocolStateAccessor<'_, Checkpoint>,
    call: &AttestationVerifyRequest<'_>,
) -> bool
where
    Checkpoint: StateCheckpoint,
{
    let min_anchor = context
        .height
        .saturating_sub(ATTESTATION_QUOTE_FRESHNESS_BLOCKS);
    if call.freshness_anchor_block >= context.height || call.freshness_anchor_block < min_anchor {
        return false;
    }
    state
        .evm_block_hash(call.freshness_anchor_block)
        .is_some_and(|hash| hash == call.nonce)
}

pub(crate) fn active_vendor_roots_have_decodable_payload(
    roots: &[AttestationVendorRoot],
    vendor: AttestationVerifyVendor,
) -> bool {
    roots.iter().any(|root| {
        AttestationVendorRootPayload::try_from_slice(&root.payload)
            .ok()
            .is_some_and(|payload| attestation_vendor_root_payload_matches_vendor(&payload, vendor))
    })
}

pub(crate) fn attestation_vendor_root_payload_matches_vendor(
    payload: &AttestationVendorRootPayload,
    vendor: AttestationVerifyVendor,
) -> bool {
    matches!(
        (payload, vendor),
        (
            AttestationVendorRootPayload::IntelSgxRoot { .. },
            AttestationVerifyVendor::IntelSgxDcap
        ) | (
            AttestationVendorRootPayload::AmdSevRoot { .. },
            AttestationVerifyVendor::AmdSevSnp
        ) | (
            AttestationVendorRootPayload::AwsNitroRoot { .. },
            AttestationVerifyVendor::AwsNitro
        ) | (
            AttestationVendorRootPayload::AppleSecureEnclaveRoot { .. },
            AttestationVerifyVendor::AppleSecureEnclave
        )
    )
}

pub(crate) fn decode_attestation_verify_claim(input: &[u8]) -> Option<AttestationVerifyClaim> {
    AttestationVerifyClaim::try_from_slice(input).ok()
}

pub(crate) fn max_attestation_quote_bytes(vendor: AttestationVerifyVendor) -> usize {
    match vendor {
        AttestationVerifyVendor::IntelSgxDcap => MAX_TEE_QUOTE_BYTES_INTEL_SGX_DCAP,
        AttestationVerifyVendor::AmdSevSnp => MAX_TEE_QUOTE_BYTES_AMD_SEV_SNP,
        AttestationVerifyVendor::AwsNitro => MAX_TEE_QUOTE_BYTES_AWS_NITRO,
        AttestationVerifyVendor::AppleSecureEnclave => MAX_TEE_QUOTE_BYTES_APPLE_SECURE_ENCLAVE,
    }
}

pub(crate) fn dispatch_evm_usdc_native<Checkpoint>(
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    let gas_used = fixed_gas_cost(descriptor, input.len(), 32);
    match selector {
        ERC20_TOTAL_SUPPLY_SELECTOR => Ok(success_output(
            abi_encode_u256(read_evm_usdc_total_supply(checkpoint)),
            gas_used,
        )),
        ERC20_BALANCE_OF_SELECTOR => {
            let owner = decode_address_word(input)
                .map_err(|_| PrecompileError::InvalidInput("balanceOf expects address"))?;
            Ok(success_output(
                abi_encode_u256(read_evm_usdc_balance(checkpoint, owner)),
                gas_used,
            ))
        }
        ERC20_ALLOWANCE_SELECTOR => {
            let (owner, spender) = decode_two_address_words(input).map_err(|_| {
                PrecompileError::InvalidInput("allowance expects owner and spender")
            })?;
            Ok(success_output(
                abi_encode_u256(read_evm_usdc_allowance(checkpoint, owner, spender)),
                gas_used,
            ))
        }
        ERC165_SUPPORTS_INTERFACE_SELECTOR => {
            let interface_id = decode_interface_id(input)
                .map_err(|_| PrecompileError::InvalidInput("supportsInterface expects bytes4"))?;
            Ok(success_output(
                abi_encode_bool(supported_usdc_interface(interface_id)),
                gas_used,
            ))
        }
        ERC20_NAME_SELECTOR => Ok(success_output(abi_encode_string("USDC"), gas_used)),
        ERC20_SYMBOL_SELECTOR => Ok(success_output(abi_encode_string("USDC"), gas_used)),
        ERC20_DECIMALS_SELECTOR => Ok(success_output(abi_encode_u256(U256::from(6_u64)), gas_used)),
        _ => Err(PrecompileError::InvalidInput(
            "evm_usdc stateful methods require EVM caller context",
        )),
    }
}

pub(crate) fn dispatch_evm_usdc_evm<Checkpoint>(
    call: EvmPrecompileCall<'_>,
    selector: Selector,
    input: &[u8],
    descriptor: &PrecompileDescriptor,
    checkpoint: &mut Checkpoint,
) -> Result<PrecompileOutput, PrecompileError>
where
    Checkpoint: StateCheckpoint,
{
    let address = descriptor.address;
    let gas_used = fixed_gas_cost(descriptor, input.len(), 32);

    match call.scheme {
        CallScheme::Call | CallScheme::StaticCall => {}
        CallScheme::CallCode | CallScheme::DelegateCall => {
            return Ok(reverted_output(
                gas_used,
                encode_zero_arg_error(EVM_USDC_UNSUPPORTED_CALL_SCHEME_ERROR_SELECTOR),
            ));
        }
    }

    match selector {
        ERC20_NAME_SELECTOR => Ok(success_output(abi_encode_string("USDC"), gas_used)),
        ERC20_SYMBOL_SELECTOR => Ok(success_output(abi_encode_string("USDC"), gas_used)),
        ERC20_DECIMALS_SELECTOR => Ok(success_output(abi_encode_u256(U256::from(6_u64)), gas_used)),
        ERC20_TOTAL_SUPPLY_SELECTOR => Ok(success_output(
            abi_encode_u256(read_evm_usdc_total_supply(checkpoint)),
            gas_used,
        )),
        ERC20_BALANCE_OF_SELECTOR => {
            let Ok(owner) = decode_address_word(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR),
                ));
            };
            Ok(success_output(
                abi_encode_u256(read_evm_usdc_balance(checkpoint, owner)),
                gas_used,
            ))
        }
        ERC20_ALLOWANCE_SELECTOR => {
            let Ok((owner, spender)) = decode_two_address_words(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR),
                ));
            };
            Ok(success_output(
                abi_encode_u256(read_evm_usdc_allowance(checkpoint, owner, spender)),
                gas_used,
            ))
        }
        ERC165_SUPPORTS_INTERFACE_SELECTOR => {
            let Ok(interface_id) = decode_interface_id(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            Ok(success_output(
                abi_encode_bool(supported_usdc_interface(interface_id)),
                gas_used,
            ))
        }
        ERC20_TRANSFER_SELECTOR => {
            if call.read_only {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
                ));
            }
            let Ok((to, amount)) = decode_address_u256_words(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            if to == Address::ZERO {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR),
                ));
            }
            let from_balance = read_evm_usdc_balance(checkpoint, call.caller);
            if from_balance < amount {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_INSUFFICIENT_BALANCE_ERROR_SELECTOR),
                ));
            }
            write_evm_usdc_balance(checkpoint, call.caller, from_balance - amount);
            let to_balance = read_evm_usdc_balance(checkpoint, to);
            write_evm_usdc_balance(checkpoint, to, to_balance + amount);
            Ok(success_output_with_logs(
                abi_encode_bool(true),
                gas_used,
                vec![erc20_transfer_log(address, call.caller, to, amount)],
            ))
        }
        ERC20_APPROVE_SELECTOR => {
            if call.read_only {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
                ));
            }
            let Ok((spender, amount)) = decode_address_u256_words(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            if spender == Address::ZERO {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR),
                ));
            }
            write_evm_usdc_allowance(checkpoint, call.caller, spender, amount);
            Ok(success_output_with_logs(
                abi_encode_bool(true),
                gas_used,
                vec![erc20_approval_log(address, call.caller, spender, amount)],
            ))
        }
        ERC20_TRANSFER_FROM_SELECTOR => {
            if call.read_only {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
                ));
            }
            let Ok((from, to, amount)) = decode_two_address_and_u256_words(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            if to == Address::ZERO || from == Address::ZERO {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR),
                ));
            }
            let allowance = read_evm_usdc_allowance(checkpoint, from, call.caller);
            if allowance < amount && allowance != U256::MAX {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_INSUFFICIENT_ALLOWANCE_ERROR_SELECTOR),
                ));
            }
            let from_balance = read_evm_usdc_balance(checkpoint, from);
            if from_balance < amount {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_INSUFFICIENT_BALANCE_ERROR_SELECTOR),
                ));
            }
            if allowance != U256::MAX {
                write_evm_usdc_allowance(checkpoint, from, call.caller, allowance - amount);
            }
            write_evm_usdc_balance(checkpoint, from, from_balance - amount);
            let to_balance = read_evm_usdc_balance(checkpoint, to);
            write_evm_usdc_balance(checkpoint, to, to_balance + amount);
            Ok(success_output_with_logs(
                abi_encode_bool(true),
                gas_used,
                vec![erc20_transfer_log(address, from, to, amount)],
            ))
        }
        EVM_USDC_DEPOSIT_SELECTOR => {
            if call.read_only {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
                ));
            }
            let Ok(amount) = decode_u256_word(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            let policy_balance = read_policy_usdc_balance(checkpoint, call.caller);
            if policy_balance < amount {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_INSUFFICIENT_BALANCE_ERROR_SELECTOR),
                ));
            }
            write_policy_usdc_balance(checkpoint, call.caller, policy_balance - amount);
            let evm_balance = read_evm_usdc_balance(checkpoint, call.caller);
            write_evm_usdc_balance(checkpoint, call.caller, evm_balance + amount);
            let total_supply = read_evm_usdc_total_supply(checkpoint);
            write_evm_usdc_total_supply(checkpoint, total_supply + amount);
            Ok(success_output_with_logs(
                abi_encode_bool(true),
                gas_used,
                vec![
                    erc20_transfer_log(address, Address::ZERO, call.caller, amount),
                    evm_usdc_deposit_log(address, call.caller, amount),
                ],
            ))
        }
        EVM_USDC_WITHDRAW_SELECTOR => {
            if call.read_only {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
                ));
            }
            let Ok(amount) = decode_u256_word(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            let evm_balance = read_evm_usdc_balance(checkpoint, call.caller);
            if evm_balance < amount {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_INSUFFICIENT_BALANCE_ERROR_SELECTOR),
                ));
            }
            write_evm_usdc_balance(checkpoint, call.caller, evm_balance - amount);
            let policy_balance = read_policy_usdc_balance(checkpoint, call.caller);
            write_policy_usdc_balance(checkpoint, call.caller, policy_balance + amount);
            let total_supply = read_evm_usdc_total_supply(checkpoint);
            write_evm_usdc_total_supply(checkpoint, total_supply - amount);
            Ok(success_output_with_logs(
                abi_encode_bool(true),
                gas_used,
                vec![
                    erc20_transfer_log(address, call.caller, Address::ZERO, amount),
                    evm_usdc_withdraw_log(address, call.caller, amount),
                ],
            ))
        }
        EVM_USDC_DEPOSIT_FOR_SELECTOR => {
            if call.read_only {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(STATIC_CONTEXT_VIOLATION_ERROR_SELECTOR),
                ));
            }
            let Ok((target, amount)) = decode_address_u256_words(input) else {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(MALFORMED_CALLDATA_ERROR_SELECTOR),
                ));
            };
            if target == Address::ZERO {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_MALFORMED_ADDRESS_ERROR_SELECTOR),
                ));
            }
            if !has_evm_usdc_minter_role(checkpoint, call.caller) {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_UNAUTHORIZED_ERROR_SELECTOR),
                ));
            }
            let policy_balance = read_policy_usdc_balance(checkpoint, call.caller);
            if policy_balance < amount {
                return Ok(reverted_output(
                    gas_used,
                    encode_zero_arg_error(EVM_USDC_INSUFFICIENT_BALANCE_ERROR_SELECTOR),
                ));
            }
            write_policy_usdc_balance(checkpoint, call.caller, policy_balance - amount);
            let target_balance = read_evm_usdc_balance(checkpoint, target);
            write_evm_usdc_balance(checkpoint, target, target_balance + amount);
            let total_supply = read_evm_usdc_total_supply(checkpoint);
            write_evm_usdc_total_supply(checkpoint, total_supply + amount);
            Ok(success_output_with_logs(
                abi_encode_bool(true),
                gas_used,
                vec![
                    erc20_transfer_log(address, Address::ZERO, target, amount),
                    evm_usdc_deposit_log(address, target, amount),
                ],
            ))
        }
        _ => Ok(reverted_output(
            gas_used,
            encode_unknown_selector_error(selector),
        )),
    }
}

pub(crate) fn admit_trading_intent<Checkpoint>(
    context: &ExecutionContext,
    state: &mut ProtocolStateAccessor<'_, Checkpoint>,
    signed_intent: SignedIntent,
) -> SubmissionResult
where
    Checkpoint: StateCheckpoint,
{
    let tracking_id = intent_id(&signed_intent);
    if state.pending_intent(&tracking_id).is_some() {
        return rejected_submission(tracking_id, SubmissionReason::InvalidNonce);
    }

    if intent_expires_at_ms(&signed_intent.intent) <= context.timestamp_ms {
        return rejected_submission(tracking_id, SubmissionReason::PolicyViolation);
    }

    if intent_not_before_ms(&signed_intent.intent).is_some() {
        return rejected_submission(tracking_id, SubmissionReason::UnsupportedIntentKind);
    }

    if let Some(market_id) = order_market_id(&signed_intent.intent.kind) {
        if state.oracle_mark(market_id).is_none() {
            return rejected_submission(tracking_id, SubmissionReason::UnknownMarket);
        }
        if state.market_paused(market_id) {
            return rejected_submission(tracking_id, SubmissionReason::MarketPaused);
        }
        if !market_price_inputs_ready_at(market_id, context.timestamp_ms, state) {
            return rejected_submission(tracking_id, SubmissionReason::PolicyViolation);
        }
    }

    if !supported_trading_intent_kind(&signed_intent.intent.kind) {
        return rejected_submission(tracking_id, SubmissionReason::UnsupportedIntentKind);
    }

    let admission = match policy_admission_for_intent(
        &signed_intent,
        context.chain_id,
        context.height,
        context.timestamp_ms,
    ) {
        Ok(admission) => admission,
        Err(reason) => return rejected_submission(tracking_id, reason),
    };

    let mut policy_store = state.policy_store();
    match policy_store.commit_admission(signed_intent.intent.policy_id, &admission) {
        Ok(()) => {
            state.set_policy_store(&policy_store);
            let order_id = allocated_order_id(&signed_intent.intent.kind);
            state.set_pending_intent(&PendingIntentRecord {
                tracking_id,
                order_id,
                signed_intent,
            });
            SubmissionResult {
                status: SubmissionStatus::Accepted,
                tracking_id,
                order_id,
                reason: SubmissionReason::None,
                nonce_consumed: true,
            }
        }
        Err(error) => rejected_submission(tracking_id, submission_reason_for_policy_error(error)),
    }
}

pub(crate) fn rejected_submission(tracking_id: Hash, reason: SubmissionReason) -> SubmissionResult {
    SubmissionResult {
        status: SubmissionStatus::Rejected,
        tracking_id,
        order_id: 0,
        reason,
        nonce_consumed: false,
    }
}

pub(crate) fn fixed_gas_cost(
    descriptor: &PrecompileDescriptor,
    input_len: usize,
    output_len: usize,
) -> u64 {
    match descriptor.gas_model {
        GasModel::Fixed { base, per_word } => {
            let words = (input_len + output_len).div_ceil(32) as u64;
            base.saturating_add(per_word.saturating_mul(words))
        }
        GasModel::Metered => 0,
    }
}

pub(crate) fn base_and_level_gas_cost(base: u64, level_count: usize, output_len: usize) -> u64 {
    let per_level_words = 3_u64;
    let metered_words = (level_count as u64).saturating_mul(per_level_words);
    let copy_words = (output_len as u64).div_ceil(32);
    base.saturating_add(metered_words.saturating_mul(300))
        .saturating_add(copy_words.saturating_mul(0))
}

pub(crate) fn encode_submission_result(result: SubmissionResult) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(32 * 5);
    encoded.extend(abi_word_from_u64(result.status as u64));
    encoded.extend(abi_word_from_hash(result.tracking_id));
    encoded.extend(abi_word_from_u64(result.order_id));
    encoded.extend(abi_word_from_u64(result.reason as u64));
    encoded.extend(abi_word_from_bool(result.nonce_consumed));
    encoded
}

pub(crate) fn encode_best_bid_ask_response(
    book: &SingleMarketBook,
    book_seq: u64,
    now_ms: u64,
) -> Vec<u8> {
    let best_bid = book.best_bid(now_ms);
    let best_ask = book.best_ask(now_ms);
    let mut encoded = Vec::with_capacity(32 * 5);
    encoded.extend(abi_word_from_u64(
        best_bid.map_or(0, |order| order.price_ticks),
    ));
    encoded.extend(abi_word_from_u64(
        best_bid.map_or(0, |order| order.remaining_lots),
    ));
    encoded.extend(abi_word_from_u64(
        best_ask.map_or(0, |order| order.price_ticks),
    ));
    encoded.extend(abi_word_from_u64(
        best_ask.map_or(0, |order| order.remaining_lots),
    ));
    encoded.extend(abi_word_from_u64(book_seq));
    encoded
}

pub(crate) fn encode_depth_response(levels: &[BookLevel], book_seq: u64) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(64 + 32 + (levels.len() * 96));
    encoded.extend(abi_word_from_u64(64));
    encoded.extend(abi_word_from_u64(book_seq));
    encoded.extend(abi_word_from_u64(levels.len() as u64));
    for level in levels {
        encoded.extend(abi_word_from_u64(level.price_ticks));
        encoded.extend(abi_word_from_u64(level.total_lots));
        encoded.extend(abi_word_from_u64(level.order_count));
    }
    encoded
}

pub(crate) fn encode_oracle_mark_response(mark: &OracleMarkRecord) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(32 * 3);
    encoded.extend(abi_word_from_u64(mark.price_ticks));
    encoded.extend(abi_word_from_u64(mark.oracle_tick));
    encoded.extend(abi_word_from_u64(mark.updated_at_block));
    encoded
}

pub(crate) fn encode_attestation_verify_outcome(outcome: AttestationVerifyOutcome) -> Vec<u8> {
    debug_assert_eq!(
        outcome.verified,
        outcome.failure_reason == AttestationVerifyFailure::None
    );
    let mut encoded = Vec::with_capacity(32 * 8);
    encoded.extend(abi_word_from_bool(outcome.verified));
    encoded.extend(abi_word_from_hash(outcome.code_hash));
    encoded.extend(abi_word_from_hash(outcome.model_hash));
    encoded.extend(abi_word_from_hash(outcome.enclave_id));
    encoded.extend(abi_word_from_hash(outcome.signer));
    encoded.extend(abi_word_from_hash(outcome.nonce));
    encoded.extend(abi_word_from_hash(outcome.vendor_root_id));
    encoded.extend(abi_word_from_u64(outcome.failure_reason as u64));
    encoded
}

pub(crate) fn reverted_output(gas_used: u64, returned: Vec<u8>) -> PrecompileOutput {
    PrecompileOutput {
        returned,
        gas_used,
        reverted: true,
        logs: Vec::new(),
    }
}

pub(crate) fn selector_from_slice(bytes: &[u8]) -> Selector {
    let mut selector = [0_u8; 4];
    selector.copy_from_slice(bytes);
    selector
}

pub(crate) fn decode_u64_word(bytes: &[u8]) -> Result<u64, ()> {
    if bytes.len() != 32 && bytes.len() != 8 {
        return Err(());
    }
    let slice = if bytes.len() == 32 {
        &bytes[24..32]
    } else {
        bytes
    };
    Ok(u64::from_be_bytes(slice.try_into().map_err(|_| ())?))
}

pub(crate) fn decode_u256_word(bytes: &[u8]) -> Result<U256, ()> {
    if bytes.len() != 32 {
        return Err(());
    }
    Ok(U256::from_be_slice(bytes))
}

pub(crate) fn decode_address_word(bytes: &[u8]) -> Result<Address, ()> {
    if bytes.len() != 32 {
        return Err(());
    }
    if bytes[..12].iter().any(|byte| *byte != 0) {
        return Err(());
    }
    let mut address = [0_u8; 20];
    address.copy_from_slice(&bytes[12..32]);
    Ok(Address::from(address))
}

pub(crate) fn decode_interface_id(bytes: &[u8]) -> Result<Selector, ()> {
    if bytes.len() != 32 {
        return Err(());
    }
    if bytes[..28].iter().any(|byte| *byte != 0) {
        return Err(());
    }
    let mut selector = [0_u8; 4];
    selector.copy_from_slice(&bytes[28..32]);
    Ok(selector)
}

pub(crate) fn decode_address_u256_words(bytes: &[u8]) -> Result<(Address, U256), ()> {
    if bytes.len() != 64 {
        return Err(());
    }
    Ok((
        decode_address_word(&bytes[0..32])?,
        decode_u256_word(&bytes[32..64])?,
    ))
}

pub(crate) fn decode_two_address_words(bytes: &[u8]) -> Result<(Address, Address), ()> {
    if bytes.len() != 64 {
        return Err(());
    }
    Ok((
        decode_address_word(&bytes[0..32])?,
        decode_address_word(&bytes[32..64])?,
    ))
}

pub(crate) fn decode_two_address_and_u256_words(
    bytes: &[u8],
) -> Result<(Address, Address, U256), ()> {
    if bytes.len() != 96 {
        return Err(());
    }
    Ok((
        decode_address_word(&bytes[0..32])?,
        decode_address_word(&bytes[32..64])?,
        decode_u256_word(&bytes[64..96])?,
    ))
}

pub(crate) fn decode_depth_words(bytes: &[u8]) -> Result<(u64, u8, u16), ()> {
    if bytes.len() != 96 {
        return Err(());
    }
    let market_id = decode_u64_word(&bytes[0..32])?;
    let side = bytes[63];
    let depth_limit = u16::from_be_bytes(bytes[94..96].try_into().map_err(|_| ())?);
    Ok((market_id, side, depth_limit))
}

pub(crate) fn decode_single_bytes_argument(input: &[u8]) -> Option<&[u8]> {
    if input.len() < 64 {
        return None;
    }
    let offset = decode_u64_word(&input[0..32]).ok()? as usize;
    if offset != 32 || input.len() < offset + 32 {
        return None;
    }
    let len = decode_u64_word(&input[offset..offset + 32]).ok()? as usize;
    let data_start = offset + 32;
    let data_end = data_start.checked_add(len)?;
    if input.len() < data_end {
        return None;
    }
    Some(&input[data_start..data_end])
}

pub(crate) fn decode_dynamic_bytes_argument(input: &[u8], offset: usize) -> Option<&[u8]> {
    if offset % 32 != 0 || input.len() < offset + 32 {
        return None;
    }
    let len = decode_u64_word(&input[offset..offset + 32]).ok()? as usize;
    let data_start = offset.checked_add(32)?;
    let data_end = data_start.checked_add(len)?;
    if input.len() < data_end {
        return None;
    }
    Some(&input[data_start..data_end])
}

pub(crate) fn decode_attestation_verify_input(
    input: &[u8],
) -> Option<AttestationVerifyRequest<'_>> {
    if input.len() < 160 {
        return None;
    }
    let vendor_code = decode_u64_word(&input[0..32]).ok()?;
    let vendor = AttestationVerifyVendor::from_abi(u8::try_from(vendor_code).ok()?)?;
    let quote_offset = decode_u64_word(&input[32..64]).ok()? as usize;
    let nonce = decode_fixed_bytes32(&input[64..96])?;
    let freshness_anchor_block = decode_u64_word(&input[96..128]).ok()?;
    let expected_claim_offset = decode_u64_word(&input[128..160]).ok()? as usize;
    let quote_bytes = decode_dynamic_bytes_argument(input, quote_offset)?;
    let expected_claim = decode_dynamic_bytes_argument(input, expected_claim_offset)?;
    Some(AttestationVerifyRequest {
        vendor,
        quote_bytes,
        nonce,
        freshness_anchor_block,
        expected_claim,
    })
}

pub(crate) fn decode_fixed_bytes32(input: &[u8]) -> Option<[u8; 32]> {
    if input.len() != 32 {
        return None;
    }
    let mut word = [0_u8; 32];
    word.copy_from_slice(input);
    Some(word)
}

pub(crate) fn decode_depth_call(input: &[u8]) -> Option<([u8; 32], u8, u16)> {
    if input.len() != 96 {
        return None;
    }
    let market = decode_fixed_bytes32(&input[0..32])?;
    let side = input[63];
    let depth_limit = u16::from_be_bytes(input[94..96].try_into().ok()?);
    Some((market, side, depth_limit))
}

pub(crate) fn decode_market_label(word: [u8; 32]) -> Result<String, Vec<u8>> {
    let label_len = word.iter().position(|byte| *byte == 0).unwrap_or(32);
    if label_len == 0 {
        return Err(encode_zero_arg_error(MALFORMED_MARKET_ID_ERROR_SELECTOR));
    }
    if word[label_len..].iter().any(|byte| *byte != 0) {
        return Err(encode_zero_arg_error(MALFORMED_MARKET_ID_ERROR_SELECTOR));
    }
    let label_bytes = &word[..label_len];
    if label_bytes.iter().any(|byte| !(0x20..=0x7e).contains(byte)) {
        return Err(encode_zero_arg_error(MALFORMED_MARKET_ID_ERROR_SELECTOR));
    }
    let label = str::from_utf8(label_bytes)
        .map_err(|_| encode_zero_arg_error(MALFORMED_MARKET_ID_ERROR_SELECTOR))?;
    Ok(label.to_string())
}

pub(crate) fn encode_unknown_selector_error(selector: Selector) -> Vec<u8> {
    let mut word = [0_u8; 32];
    word[28..32].copy_from_slice(&selector);
    encode_one_word_error(UNKNOWN_SELECTOR_ERROR_SELECTOR, word)
}

pub(crate) fn encode_zero_arg_error(selector: Selector) -> Vec<u8> {
    selector.to_vec()
}

pub(crate) fn encode_one_word_error(selector: Selector, word: [u8; 32]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(36);
    encoded.extend(selector);
    encoded.extend(word);
    encoded
}

pub(crate) fn encode_two_word_error(selector: Selector, first: u64, second: u64) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(68);
    encoded.extend(selector);
    encoded.extend(abi_word_from_u64(first));
    encoded.extend(abi_word_from_u64(second));
    encoded
}

pub(crate) fn abi_word_from_u64(value: u64) -> [u8; 32] {
    let mut word = [0_u8; 32];
    word[24..32].copy_from_slice(&value.to_be_bytes());
    word
}

pub(crate) fn abi_word_from_bool(value: bool) -> [u8; 32] {
    abi_word_from_u64(u64::from(value))
}

pub(crate) fn abi_word_from_hash(value: Hash) -> [u8; 32] {
    value
}

pub(crate) fn abi_word_from_u256(value: U256) -> [u8; 32] {
    value.to_be_bytes::<32>()
}

pub(crate) fn abi_word_from_address(value: Address) -> [u8; 32] {
    let mut word = [0_u8; 32];
    word[12..32].copy_from_slice(value.as_slice());
    word
}

pub(crate) fn abi_encode_u256(value: U256) -> Vec<u8> {
    abi_word_from_u256(value).to_vec()
}

pub(crate) fn abi_encode_bool(value: bool) -> Vec<u8> {
    abi_word_from_bool(value).to_vec()
}

pub(crate) fn abi_encode_string(value: &str) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(96);
    encoded.extend(abi_word_from_u64(32));
    encoded.extend(abi_word_from_u64(value.len() as u64));
    let mut padded = value.as_bytes().to_vec();
    let padding = (32 - (padded.len() % 32)) % 32;
    padded.resize(padded.len() + padding, 0);
    encoded.extend(padded);
    encoded
}

pub(crate) fn success_output(returned: Vec<u8>, gas_used: u64) -> PrecompileOutput {
    PrecompileOutput {
        returned,
        gas_used,
        reverted: false,
        logs: Vec::new(),
    }
}

pub(crate) fn success_output_with_logs(
    returned: Vec<u8>,
    gas_used: u64,
    logs: Vec<Log>,
) -> PrecompileOutput {
    PrecompileOutput {
        returned,
        gas_used,
        reverted: false,
        logs,
    }
}

pub(crate) fn supported_usdc_interface(interface_id: Selector) -> bool {
    matches!(
        interface_id,
        IERC165_INTERFACE_ID | IERC20_INTERFACE_ID | IERC20_METADATA_INTERFACE_ID
    )
}

pub(crate) fn topic_from_selector_hash(signature: &str) -> B256 {
    keccak256(signature.as_bytes())
}

pub(crate) fn erc20_transfer_log(
    address: Address,
    from: Address,
    to: Address,
    amount: U256,
) -> Log {
    Log {
        address,
        data: LogData::new_unchecked(
            vec![
                topic_from_selector_hash("Transfer(address,address,uint256)"),
                B256::from(abi_word_from_address(from)),
                B256::from(abi_word_from_address(to)),
            ],
            Bytes::copy_from_slice(&abi_word_from_u256(amount)),
        ),
    }
}

pub(crate) fn erc20_approval_log(
    address: Address,
    owner: Address,
    spender: Address,
    amount: U256,
) -> Log {
    Log {
        address,
        data: LogData::new_unchecked(
            vec![
                topic_from_selector_hash("Approval(address,address,uint256)"),
                B256::from(abi_word_from_address(owner)),
                B256::from(abi_word_from_address(spender)),
            ],
            Bytes::copy_from_slice(&abi_word_from_u256(amount)),
        ),
    }
}

pub(crate) fn evm_usdc_deposit_log(address: Address, user: Address, amount: U256) -> Log {
    Log {
        address,
        data: LogData::new_unchecked(
            vec![
                topic_from_selector_hash("EvmDeposit(address,uint256)"),
                B256::from(abi_word_from_address(user)),
            ],
            Bytes::copy_from_slice(&abi_word_from_u256(amount)),
        ),
    }
}

pub(crate) fn evm_usdc_withdraw_log(address: Address, user: Address, amount: U256) -> Log {
    Log {
        address,
        data: LogData::new_unchecked(
            vec![
                topic_from_selector_hash("EvmWithdraw(address,uint256)"),
                B256::from(abi_word_from_address(user)),
            ],
            Bytes::copy_from_slice(&abi_word_from_u256(amount)),
        ),
    }
}

pub(crate) fn pending_intent_state_key(tracking_id: &Hash) -> Vec<u8> {
    format!("{PENDING_INTENT_PREFIX}{}", encode_hex(tracking_id)).into_bytes()
}

pub(crate) fn order_book_state_key(market_id: u64) -> String {
    format!("{ORDER_BOOK_PREFIX}{market_id}")
}

pub(crate) fn book_sequence_state_key(market_id: u64) -> String {
    format!("{BOOK_SEQUENCE_PREFIX}{market_id}")
}

pub(crate) fn market_label_state_key(label: &str) -> String {
    format!("{MARKET_LABEL_PREFIX}{label}")
}

pub(crate) fn oracle_mark_state_key(market_id: u64) -> String {
    format!("{ORACLE_MARK_PREFIX}{market_id}")
}

pub(crate) fn oracle_config_state_key(market_id: u64) -> String {
    format!("{ORACLE_CONFIG_PREFIX}{market_id}")
}

pub(crate) fn paused_market_state_key(market_id: u64) -> String {
    format!("{PAUSED_MARKET_PREFIX}{market_id}")
}

pub(crate) fn supported_trading_intent_kind(kind: &IntentKind) -> bool {
    matches!(
        kind,
        IntentKind::PlaceOrder { .. }
            | IntentKind::CancelOrder { .. }
            | IntentKind::ReplaceOrder { .. }
    )
}

pub(crate) fn allocated_order_id(kind: &IntentKind) -> u64 {
    match kind {
        IntentKind::PlaceOrder {
            client_order_id, ..
        }
        | IntentKind::ReplaceOrder {
            replacement_client_order_id: client_order_id,
            ..
        } => *client_order_id,
        _ => 0,
    }
}

pub(crate) fn order_market_id(kind: &IntentKind) -> Option<u64> {
    match kind {
        IntentKind::PlaceOrder { market_id, .. }
        | IntentKind::CancelOrder { market_id, .. }
        | IntentKind::ReplaceOrder { market_id, .. } => Some(*market_id),
        _ => None,
    }
}

pub(crate) fn market_price_inputs_ready_at<Checkpoint>(
    market_id: u64,
    timestamp_ms: u64,
    state: &ProtocolStateAccessor<'_, Checkpoint>,
) -> bool
where
    Checkpoint: StateCheckpoint,
{
    let config = state.oracle_config(market_id);
    let mark = state.oracle_mark(market_id).map(|record| OraclePrice {
        price_ticks: record.price_ticks,
        timestamp_ms: record.timestamp_ms,
    });
    let status = price_status(timestamp_ms, mark, config);
    status.is_none_or(|status| !status.stale)
}

pub(crate) fn policy_admission_for_intent(
    signed_intent: &SignedIntent,
    expected_chain_id: u64,
    block_number: u64,
    timestamp_ms: u64,
) -> Result<PolicyAdmission, SubmissionReason> {
    let (gas_cost, volume, notional_outstanding) = direct_intent_costs(&signed_intent.intent.kind);
    Ok(PolicyAdmission {
        signed_session: SignedSessionPayload {
            payload: SessionPayload {
                intent_hash: intent_signing_hash(&signed_intent.intent),
                policy_id: signed_intent.intent.policy_id,
                chain_id: signed_intent.intent.chain_id,
            },
            signature: signed_intent.signature,
        },
        intent_nonce: signed_intent.intent.nonce,
        block_number,
        expected_chain_id,
        timestamp_ms,
        gas_cost,
        volume,
        notional_outstanding,
        intent_notional: volume,
        market_id: intent_market_id(&signed_intent.intent.kind),
        realized_pnl_delta: 0,
        current_drawdown_pct: 0,
    })
}

pub(crate) fn intent_market_id(kind: &IntentKind) -> Option<u64> {
    match kind {
        IntentKind::PlaceOrder { market_id, .. }
        | IntentKind::CancelOrder { market_id, .. }
        | IntentKind::ReplaceOrder { market_id, .. }
        | IntentKind::DepositCollateral { market_id, .. }
        | IntentKind::WithdrawCollateral { market_id, .. } => Some(*market_id),
        IntentKind::Transfer { .. }
        | IntentKind::Cancel { .. }
        | IntentKind::CreateSubPolicy { .. }
        | IntentKind::RevokePolicy { .. } => None,
    }
}

pub(crate) fn direct_intent_costs(kind: &IntentKind) -> (u128, u128, u128) {
    match kind {
        IntentKind::Transfer { amount, .. } => {
            let amount = u128::from(*amount);
            (10, amount, amount)
        }
        IntentKind::Cancel { .. } => (5, 0, 0),
        IntentKind::PlaceOrder {
            quantity_lots,
            kind,
            ..
        }
        | IntentKind::ReplaceOrder {
            quantity_lots,
            kind,
            ..
        } => {
            let quantity = u128::from(*quantity_lots);
            let notional = match kind {
                IntentOrderKind::Limit { price_ticks } => {
                    quantity.saturating_mul(u128::from(*price_ticks))
                }
                IntentOrderKind::Market => quantity,
            };
            (12, notional, notional)
        }
        IntentKind::CancelOrder { .. } => (5, 0, 0),
        IntentKind::CreateSubPolicy { .. } => (15, 0, 0),
        IntentKind::RevokePolicy { .. } => (5, 0, 0),
        IntentKind::DepositCollateral { amount_usdc, .. }
        | IntentKind::WithdrawCollateral { amount_usdc, .. } => {
            let amount = u128::from(*amount_usdc);
            (10, amount, amount)
        }
    }
}

pub(crate) fn submission_reason_for_policy_error(error: PolicyError) -> SubmissionReason {
    match error {
        PolicyError::InvalidIntentNonce { .. } => SubmissionReason::InvalidNonce,
        PolicyError::InvalidSessionSignature | PolicyError::InvalidSessionKeyMaterial => {
            SubmissionReason::InvalidSignature
        }
        PolicyError::GasBudgetExceeded => SubmissionReason::PolicyGasExhausted,
        _ => SubmissionReason::PolicyViolation,
    }
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

pub(crate) fn abi_hash(signature: &str) -> Hash {
    // Protocol descriptor fingerprint. EVM call selectors are keccak256[..4].
    let mut hasher = Sha256::new();
    hasher.update(signature.as_bytes());
    hasher.finalize().into()
}

pub(crate) fn address_from_u64(value: u64) -> Address {
    let mut bytes = [0_u8; 20];
    bytes[12..20].copy_from_slice(&value.to_be_bytes());
    Address::from(bytes)
}
