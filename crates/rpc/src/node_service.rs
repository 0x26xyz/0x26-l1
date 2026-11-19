use crate::*;
use std::fs;

impl<Store> RpcService<Store> {
    pub fn new(chain_id: u64, executor: BlockExecutor<Store>) -> Self {
        Self {
            chain_id,
            head: None,
            executor,
        }
    }

    pub fn set_head(&mut self, head: Block) {
        self.head = Some(head);
    }

    pub fn head(&self) -> Option<&Block> {
        self.head.as_ref()
    }

    pub fn executor(&self) -> &BlockExecutor<Store> {
        &self.executor
    }
}

impl<Store> RpcService<Store>
where
    Store: CheckpointableStateStore,
{
    pub fn eth_chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn eth_block_number(&self) -> Result<u64, RpcError> {
        Ok(self
            .head
            .as_ref()
            .ok_or(RpcError::HeadNotSet)?
            .header
            .height)
    }

    pub fn eth_get_code(&self, address: [u8; 20]) -> Vec<u8> {
        if address == evm_usdc_system_address_bytes() {
            return evm_usdc_system_code_bytes();
        }
        self.executor
            .state_store()
            .checkpoint()
            .get(&evm_account_code_state_key(address))
            .unwrap_or_default()
    }

    pub fn eth_get_balance(&self, address: [u8; 20]) -> U256 {
        self.executor
            .state_store()
            .checkpoint()
            .get(&evm_account_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO)
    }

    pub fn eth_get_transaction_count(&self, address: [u8; 20]) -> U256 {
        self.executor
            .state_store()
            .checkpoint()
            .get(&evm_account_nonce_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO)
    }

    pub fn eth_estimate_gas(
        &self,
        request: &ContractCallRequest,
    ) -> Result<ContractCallResponse, RpcError> {
        if request.gas_limit == 0 {
            return Err(RpcError::InvalidRequest("gas_limit must be non-zero"));
        }

        let head = self.head.as_ref().ok_or(RpcError::HeadNotSet)?;
        let context = ExecutionContext {
            chain_id: self.chain_id,
            height: head.header.height.saturating_add(1),
            timestamp_ms: head.header.timestamp_ms,
            parent_state_root: self.executor.committed_root(),
        };
        let response = self
            .executor
            .simulate_contract_transaction(
                &context,
                request.caller.into(),
                request.to.into(),
                request.data.clone(),
                request.gas_limit,
            )
            .map_err(|error| RpcError::Node(format!("eth_estimateGas failed: {error:?}")))?;
        Ok(ContractCallResponse {
            returned: response.returned,
            gas_used: response.gas_used,
            state_root: self.executor.committed_root(),
        })
    }
}

impl<'a> NodeRpcService<'a> {
    pub fn new(engine: &'a mut SingleNodeEngine) -> Self {
        Self { engine }
    }

    pub fn eth_chain_id(&self) -> u64 {
        self.engine.chain_id()
    }

    pub fn eth_block_number(&self) -> u64 {
        self.engine.head().header.height
    }

    pub fn eth_get_code(&self, address: [u8; 20]) -> Vec<u8> {
        if address == evm_usdc_system_address_bytes() {
            return evm_usdc_system_code_bytes();
        }
        self.engine
            .get_committed_state(&evm_account_code_state_key(address))
            .unwrap_or_default()
    }

    pub fn eth_get_balance(&self, address: [u8; 20]) -> U256 {
        self.engine
            .get_committed_state(&evm_account_balance_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO)
    }

    pub fn eth_get_transaction_count(&self, address: [u8; 20]) -> U256 {
        self.engine
            .get_committed_state(&evm_account_nonce_state_key(address))
            .map(|bytes| U256::from_be_slice(&bytes))
            .unwrap_or(U256::ZERO)
    }

    pub fn eth_call(
        &self,
        request: &ContractCallRequest,
    ) -> Result<ContractCallResponse, RpcError> {
        if request.gas_limit == 0 {
            return Err(RpcError::InvalidRequest("gas_limit must be non-zero"));
        }
        let response = self
            .engine
            .execute_contract_call(
                request.caller,
                request.to,
                request.data.clone(),
                request.gas_limit,
            )
            .map_err(|error| RpcError::Node(format!("eth_call failed: {error:?}")))?;
        Ok(ContractCallResponse {
            returned: response.returned,
            gas_used: response.gas_used,
            state_root: self.engine.committed_state_root(),
        })
    }

    pub fn eth_gas_price(&self) -> U256 {
        U256::ZERO
    }

    pub fn eth_send_raw_transaction(&mut self, raw_tx: Vec<u8>) -> Result<Hash, RpcError> {
        self.engine
            .submit_raw_evm_transaction(raw_tx, self.engine.current_time_ms())
            .map_err(|error| RpcError::Node(format!("eth_sendRawTransaction failed: {error:?}")))
    }

    pub fn eth_get_transaction_receipt(
        &self,
        tx_hash: Hash,
    ) -> Result<Option<(kombat_chain_node::EvmTransactionReceiptRecord, Vec<u8>)>, RpcError> {
        let Some(receipt) = self.engine.evm_transaction_receipt(&tx_hash).cloned() else {
            return Ok(None);
        };
        let raw_tx = self
            .engine
            .evm_transaction_raw_bytes(&tx_hash)
            .ok_or_else(|| RpcError::Node("committed raw transaction bytes missing".into()))?;
        Ok(Some((receipt, raw_tx)))
    }

    pub fn eth_get_transaction_by_hash(
        &self,
        tx_hash: Hash,
    ) -> Result<Option<(kombat_chain_node::EvmTransactionReceiptRecord, Vec<u8>)>, RpcError> {
        let Some(receipt) = self.engine.evm_transaction_receipt(&tx_hash).cloned() else {
            return Ok(None);
        };
        let raw_tx = self
            .engine
            .evm_transaction_raw_bytes(&tx_hash)
            .ok_or_else(|| RpcError::Node("committed raw transaction bytes missing".into()))?;
        Ok(Some((receipt, raw_tx)))
    }

    pub fn eth_estimate_gas(
        &self,
        request: &ContractCallRequest,
    ) -> Result<ContractCallResponse, RpcError> {
        if request.gas_limit == 0 {
            return Err(RpcError::InvalidRequest("gas_limit must be non-zero"));
        }
        let response = self
            .engine
            .simulate_contract_transaction(
                request.caller,
                request.to,
                request.data.clone(),
                request.gas_limit,
            )
            .map_err(|error| RpcError::Node(format!("eth_estimateGas failed: {error:?}")))?;
        Ok(ContractCallResponse {
            returned: response.returned,
            gas_used: response.gas_used,
            state_root: self.engine.committed_state_root(),
        })
    }

    pub fn create_account(
        &mut self,
        account_id: u64,
        owner: [u8; 32],
        code_hash: [u8; 32],
        evm_address: Option<[u8; 20]>,
        timestamp_ms: u64,
    ) -> Result<AccountResponse, RpcError> {
        if evm_address.is_some() {
            return Err(RpcError::InvalidRequest("evm address claim required"));
        }
        self.engine
            .create_account(account_id, owner, code_hash, timestamp_ms)
            .map_err(|error| {
                account_creation_rpc_error(account_id, owner, code_hash, timestamp_ms, &error)
            })?;
        self.get_account(account_id)
    }

    pub fn create_evm_account(
        &mut self,
        signed_account_claim: SignedEvmAccountClaim,
        timestamp_ms: u64,
    ) -> Result<AccountResponse, RpcError> {
        self.verify_evm_account_claim(&signed_account_claim, timestamp_ms)?;
        self.ensure_account_evm_address_link_available(
            signed_account_claim.account_id,
            signed_account_claim.wallet,
        )?;
        self.engine
            .create_account(
                signed_account_claim.account_id,
                signed_account_claim.owner,
                signed_account_claim.code_hash,
                timestamp_ms,
            )
            .map_err(|error| {
                account_creation_rpc_error(
                    signed_account_claim.account_id,
                    signed_account_claim.owner,
                    signed_account_claim.code_hash,
                    timestamp_ms,
                    &error,
                )
            })?;
        self.link_account_evm_address(
            signed_account_claim.account_id,
            signed_account_claim.wallet,
        )?;
        self.get_account(signed_account_claim.account_id)
    }

    fn verify_evm_account_claim(
        &self,
        signed_account_claim: &SignedEvmAccountClaim,
        timestamp_ms: u64,
    ) -> Result<(), RpcError> {
        if signed_account_claim.chain_id != self.engine.chain_id() {
            return Err(RpcError::InvalidRequest("evm account claim chain mismatch"));
        }
        if signed_account_claim.expires_at < timestamp_ms {
            return Err(RpcError::InvalidRequest("evm account claim expired"));
        }
        let expected_owner = evm_owner_principal(signed_account_claim.wallet);
        if signed_account_claim.owner != expected_owner {
            return Err(RpcError::InvalidRequest("evm account claim owner mismatch"));
        }
        let digest = evm_account_claim_eip712_digest(
            signed_account_claim.chain_id,
            signed_account_claim.account_id,
            signed_account_claim.wallet,
            signed_account_claim.owner,
            signed_account_claim.code_hash,
            signed_account_claim.expires_at,
        );
        let recovered = recover_evm_owner_call_address(&digest, &signed_account_claim.signature)
            .map_err(|_| RpcError::InvalidRequest("invalid evm account claim signature"))?;
        if recovered != signed_account_claim.wallet {
            return Err(RpcError::InvalidRequest(
                "invalid evm account claim signature",
            ));
        }
        Ok(())
    }

    pub fn chain_info(&self) -> ChainInfoResponse {
        ChainInfoResponse {
            chain_id: self.engine.chain_id(),
            source_network_id: source_network_id_for_chain_id(self.engine.chain_id()),
            head_height: self.engine.head().header.height,
            head_timestamp_ms: self.engine.head().header.timestamp_ms,
            head_hash: kombat_chain_consensus::block_hash(self.engine.head()),
            committed_state_root: self.engine.committed_state_root(),
            quarantined_block_count: self.engine.quarantined_block_count(),
        }
    }

    pub fn consensus_metrics(&self) -> &kombat_chain_node::ConsensusMetricsSnapshot {
        self.engine.metrics()
    }

    pub fn consensus_logs(&self, limit: usize) -> &[kombat_chain_node::ConsensusLogEntry] {
        let logs = self.engine.logs();
        let start = logs.len().saturating_sub(limit);
        &logs[start..]
    }

    pub fn get_state(&self, key: &[u8]) -> StateQueryResponse {
        StateQueryResponse {
            key: key.to_vec(),
            value: self.engine.get_committed_state(key),
        }
    }

    pub(crate) fn account_evm_address(&self, account_id: u64) -> Option<[u8; 20]> {
        self.engine
            .get_committed_state(&account_evm_address_state_key(account_id))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
    }

    fn account_id_for_evm_address(&self, address: [u8; 20]) -> Option<u64> {
        self.engine
            .get_committed_state(&evm_address_account_state_key(address))
            .and_then(|bytes| bytes.as_slice().try_into().ok())
            .map(u64::from_be_bytes)
    }

    fn ensure_account_evm_address_link_available(
        &self,
        account_id: u64,
        address: [u8; 20],
    ) -> Result<(), RpcError> {
        if let Some(existing) = self.account_evm_address(account_id) {
            if existing != address {
                return Err(RpcError::InvalidRequest(
                    "account is already linked to another EVM address",
                ));
            }
        }
        if let Some(existing_account_id) = self.account_id_for_evm_address(address) {
            if existing_account_id != account_id {
                return Err(RpcError::InvalidRequest(
                    "EVM address is already linked to another account",
                ));
            }
        }
        Ok(())
    }

    fn link_account_evm_address(
        &mut self,
        account_id: u64,
        address: [u8; 20],
    ) -> Result<(), RpcError> {
        self.ensure_account_evm_address_link_available(account_id, address)?;
        let mut sets = BTreeMap::new();
        sets.insert(account_evm_address_state_key(account_id), address.to_vec());
        sets.insert(
            evm_address_account_state_key(address),
            account_id.to_be_bytes().to_vec(),
        );
        self.engine
            .apply_committed_state_updates(sets, BTreeSet::new())
            .map(|_| ())
            .map_err(|error| RpcError::Node(format!("failed to link account address: {error:?}")))
    }

    pub fn get_state_root_at_height(
        &self,
        height: u64,
    ) -> Result<StateRootAtHeightResponse, RpcError> {
        let block = self
            .engine
            .blocks()
            .iter()
            .find(|block| block.header.height == height)
            .ok_or(RpcError::InvalidRequest("unknown block height"))?;
        Ok(StateRootAtHeightResponse {
            height,
            block_hash: kombat_chain_consensus::block_hash(block),
            state_root: block.header.state_root,
        })
    }

    pub fn submit_intent(
        &mut self,
        signed_intent: SignedIntent,
        timestamp_ms: u64,
    ) -> Result<SubmitIntentResponse, RpcError> {
        let intent_id = self
            .engine
            .submit_intent(signed_intent.clone(), timestamp_ms)
            .map_err(|error| intent_submission_rpc_error(&signed_intent, timestamp_ms, &error))?;

        let class = if self.engine.intent_mempool().contains_intent(intent_id) {
            IntentAdmissionClass::Direct
        } else if self
            .engine
            .conditional_intent_pool()
            .contains_intent(intent_id)
        {
            IntentAdmissionClass::Conditional
        } else {
            return Err(RpcError::Node(
                "intent disappeared after successful admission".into(),
            ));
        };

        let status = self.engine.intent_status(intent_id);

        Ok(SubmitIntentResponse {
            intent_id,
            class,
            pending_reason: status.pending_reason,
            next_eligible_at_ms: status.next_eligible_at_ms,
            current_policy_nonce: status.current_policy_nonce,
        })
    }

    pub fn get_intent_status(&self, intent_id: IntentId) -> IntentStatusResponse {
        self.engine.intent_status(intent_id)
    }

    pub fn get_policy(&self, policy_id: u64) -> Result<PolicyResponse, RpcError> {
        let policy = self
            .engine
            .policy_store()
            .get(policy_id)
            .ok_or(RpcError::UnknownPolicy(policy_id))?;
        Ok(policy_response(policy))
    }

    pub fn get_account(&self, account_id: u64) -> Result<AccountResponse, RpcError> {
        let account = self
            .engine
            .account_store()
            .get(account_id)
            .ok_or(RpcError::UnknownAccount(account_id))?;
        Ok(account_response(
            account,
            self.engine.account_frozen(account_id),
            self.account_evm_address(account_id),
        ))
    }

    pub fn get_account_by_evm_address(
        &self,
        address: [u8; 20],
    ) -> Result<Option<AccountResponse>, RpcError> {
        let Some(account_id) = self.account_id_for_evm_address(address) else {
            return Ok(None);
        };
        self.get_account(account_id).map(Some)
    }

    pub fn get_account_history(
        &self,
        account_id: u64,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<AccountHistoryRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(account_history_record_from_event)
            .filter(|event| event.account_id == account_id)
            .take(limit)
            .collect()
    }

    pub fn get_order_book(&self, market_id: u64, depth: usize) -> OrderBookResponse {
        let timestamp_ms = self.engine.head().header.timestamp_ms;
        let (bids, asks) = self.engine.book_levels(market_id, depth, timestamp_ms);
        OrderBookResponse {
            market_id,
            bids: bids.into_iter().map(book_level_response).collect(),
            asks: asks.into_iter().map(book_level_response).collect(),
        }
    }

    pub fn get_open_orders(
        &self,
        market_id: u64,
        account_id: u64,
        limit: usize,
    ) -> Vec<RestingOrderResponse> {
        let timestamp_ms = self.engine.head().header.timestamp_ms;
        self.engine
            .account_orders(market_id, account_id, timestamp_ms, limit)
            .into_iter()
            .map(resting_order_response)
            .collect()
    }

    pub fn get_trade_history(
        &self,
        market_id: u64,
        account_id: Option<u64>,
        client_order_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<TradeFillResponse> {
        EventStream::new(self.engine.runtime_events())
            .query(StreamQuery {
                from_seq: from_seq.unwrap_or(0),
                before_seq: None,
                limit,
                filter: EventFilter {
                    block_height: None,
                    account_id,
                    market_id: Some(market_id),
                    policy_id: None,
                    intent_id: None,
                    client_order_id,
                    topic: Some(RuntimeEventTopic::OrderFilled),
                },
                descending: false,
            })
            .events
            .into_iter()
            .filter_map(trade_fill_from_event)
            .collect()
    }

    pub fn get_account_pnl_window(
        &self,
        account_id: u64,
        market_id: Option<u64>,
        from_ms: u64,
        to_ms: Option<u64>,
    ) -> Result<AccountPnlWindowResponse, RpcError> {
        let head_to_ms = self.engine.head().header.timestamp_ms.saturating_add(1);
        let effective_to_ms = to_ms.unwrap_or(head_to_ms).min(head_to_ms);
        if from_ms >= effective_to_ms {
            return Err(RpcError::InvalidRequest("from_ms must be less than to_ms"));
        }
        if effective_to_ms.saturating_sub(from_ms) > MAX_ACCOUNT_PNL_WINDOW_MS {
            return Err(RpcError::InvalidRequest("pnl window exceeds 30 days"));
        }

        let range = self
            .engine
            .seq_range_for_timestamps(from_ms, effective_to_ms);
        let mut market_ids = BTreeSet::new();
        let mut funding_raw_by_market = BTreeMap::<u64, i128>::new();
        let mut fees_paid = 0_u128;
        let mut fill_count = 0_usize;
        let mut fee_count = 0_usize;
        let mut funding_count = 0_usize;
        let mut event_count = 0_usize;
        let mut first_seq = None;
        let mut last_seq = None;

        for event in pnl_window_events_for_topic(
            self.engine.runtime_events(),
            range,
            account_id,
            market_id,
            RuntimeEventTopic::OrderFilled,
            MAX_ACCOUNT_PNL_WINDOW_EVENTS.saturating_sub(event_count),
        )? {
            if let RuntimeEventKind::OrderFilled { market_id, .. } = &event.kind {
                event_count = event_count.saturating_add(1);
                fill_count = fill_count.saturating_add(1);
                market_ids.insert(*market_id);
                note_pnl_window_seq(event.seq, &mut first_seq, &mut last_seq);
            }
        }

        for event in pnl_window_events_for_topic(
            self.engine.runtime_events(),
            range,
            account_id,
            market_id,
            RuntimeEventTopic::FeeCharged,
            MAX_ACCOUNT_PNL_WINDOW_EVENTS.saturating_sub(event_count),
        )? {
            if let RuntimeEventKind::FeeCharged {
                market_id,
                fee_amount,
                ..
            } = &event.kind
            {
                event_count = event_count.saturating_add(1);
                fee_count = fee_count.saturating_add(1);
                fees_paid = fees_paid.saturating_add(*fee_amount);
                market_ids.insert(*market_id);
                note_pnl_window_seq(event.seq, &mut first_seq, &mut last_seq);
            }
        }

        for event in pnl_window_events_for_topic(
            self.engine.runtime_events(),
            range,
            account_id,
            market_id,
            RuntimeEventTopic::FundingApplied,
            MAX_ACCOUNT_PNL_WINDOW_EVENTS.saturating_sub(event_count),
        )? {
            if let RuntimeEventKind::FundingApplied {
                market_id,
                funding_delta,
                ..
            } = &event.kind
            {
                event_count = event_count.saturating_add(1);
                funding_count = funding_count.saturating_add(1);
                funding_raw_by_market
                    .entry(*market_id)
                    .and_modify(|value| *value = value.saturating_add(*funding_delta))
                    .or_insert(*funding_delta);
                market_ids.insert(*market_id);
                note_pnl_window_seq(event.seq, &mut first_seq, &mut last_seq);
            }
        }

        for event in pnl_window_events_for_topic(
            self.engine.runtime_events(),
            range,
            account_id,
            market_id,
            RuntimeEventTopic::PositionUpdated,
            MAX_ACCOUNT_PNL_WINDOW_EVENTS.saturating_sub(event_count),
        )? {
            if let RuntimeEventKind::PositionUpdated { market_id, .. } = &event.kind {
                event_count = event_count.saturating_add(1);
                market_ids.insert(*market_id);
                note_pnl_window_seq(event.seq, &mut first_seq, &mut last_seq);
            }
        }

        let mut total_realized_pnl = 0_i128;
        let mut funding_paid = 0_i128;
        for market_id in market_ids {
            let market = self.engine.perpetual_market(market_id);
            let start_raw = realized_pnl_at_seq(self.engine, account_id, market_id, range.from_seq);
            let end_raw = realized_pnl_at_seq(self.engine, account_id, market_id, range.before_seq);
            let start = scaled_signed_raw_tick_lot_pnl_usdc_for_rpc(market, start_raw);
            let end = scaled_signed_raw_tick_lot_pnl_usdc_for_rpc(market, end_raw);
            total_realized_pnl = total_realized_pnl.saturating_add(end.saturating_sub(start));
            funding_paid =
                funding_paid.saturating_add(scaled_signed_raw_tick_lot_pnl_usdc_for_rpc(
                    market,
                    funding_raw_by_market.get(&market_id).copied().unwrap_or(0),
                ));
        }

        Ok(AccountPnlWindowResponse {
            account_id,
            market_id,
            from_ms,
            to_ms: effective_to_ms,
            total_realized_pnl,
            funding_paid,
            trading_realized_pnl: total_realized_pnl.saturating_sub(funding_paid),
            fees_paid,
            fill_count,
            fee_count,
            funding_count,
            first_seq,
            last_seq,
        })
    }

    pub fn get_account_equity_history(
        &self,
        account_id: u64,
        from_ms: u64,
        to_ms: u64,
        limit: Option<usize>,
    ) -> Result<AccountEquityHistoryResponse, RpcError> {
        if from_ms >= to_ms {
            return Err(RpcError::InvalidRequest("from_ms must be less than to_ms"));
        }
        let limit = limit.unwrap_or(DEFAULT_ACCOUNT_EQUITY_HISTORY_LIMIT);
        if limit == 0 || limit > MAX_ACCOUNT_EQUITY_HISTORY_LIMIT {
            return Err(RpcError::InvalidRequest("limit must be between 1 and 1024"));
        }

        let range = self.engine.seq_range_for_timestamps(from_ms, to_ms);
        let samples = downsample_equity_samples(
            self.engine
                .equity_history_samples(account_id, range.from_height, range.before_height),
            limit,
        );
        Ok(AccountEquityHistoryResponse {
            account_id,
            from_ms,
            to_ms,
            samples,
        })
    }

    pub fn get_candles(
        &self,
        market_id: u64,
        interval: &str,
        from_ms: u64,
        to_ms: u64,
        limit: usize,
    ) -> Result<Vec<CandleResponse>, RpcError> {
        if from_ms >= to_ms {
            return Err(RpcError::InvalidRequest("from must be less than to"));
        }
        let interval_ms = candle_interval_ms(interval)
            .ok_or(RpcError::InvalidRequest("unsupported candle interval"))?;
        let mut candles = BTreeMap::<u64, CandleResponse>::new();
        for event in self.engine.runtime_events() {
            if event.timestamp_ms < from_ms || event.timestamp_ms >= to_ms {
                continue;
            }
            let Some(fill) = trade_fill_from_event(event.clone()) else {
                continue;
            };
            if fill.market_id != market_id || fill.role != OrderFillRole::Taker {
                continue;
            }
            let bucket = (fill.timestamp_ms / interval_ms) * interval_ms;
            let entry = candles.entry(bucket).or_insert_with(|| CandleResponse {
                market_id,
                interval: interval.to_string(),
                timestamp_ms: bucket,
                open: fill.price_ticks,
                high: fill.price_ticks,
                low: fill.price_ticks,
                close: fill.price_ticks,
                volume_lots: 0,
                trade_count: 0,
                first_seq: fill.seq,
                last_seq: fill.seq,
            });
            entry.high = entry.high.max(fill.price_ticks);
            entry.low = entry.low.min(fill.price_ticks);
            entry.close = fill.price_ticks;
            entry.volume_lots = entry.volume_lots.saturating_add(fill.quantity_lots);
            entry.trade_count = entry.trade_count.saturating_add(1);
            entry.first_seq = entry.first_seq.min(fill.seq);
            entry.last_seq = entry.last_seq.max(fill.seq);
        }
        Ok(candles.into_values().take(limit).collect())
    }

    pub fn get_market_snapshot(&self, market_id: u64, depth: usize) -> MarketSnapshotResponse {
        MarketSnapshotResponse {
            market: self.get_perpetual_market(market_id),
            state: self.get_market_state(market_id),
            order_book: self.get_order_book(market_id, depth),
            latest_trade: self.latest_market_trade(market_id),
            next_seq: self.next_event_seq(),
        }
    }

    pub fn get_account_trading_state(
        &self,
        account_id: u64,
        market_id: Option<u64>,
        depth: usize,
        fill_limit: usize,
    ) -> AccountTradingStateResponse {
        let positions = match market_id {
            Some(market_id) => vec![self.get_position(market_id, account_id)],
            None => self.list_positions(account_id, false, depth),
        };
        let mut open_orders = Vec::new();
        match market_id {
            Some(market_id) => {
                open_orders.extend(self.get_open_orders(market_id, account_id, depth));
            }
            None => {
                for market in self.list_perpetual_markets(None, usize::MAX) {
                    open_orders.extend(self.get_open_orders(
                        market.market.market_id,
                        account_id,
                        depth.saturating_sub(open_orders.len()),
                    ));
                    if open_orders.len() >= depth {
                        break;
                    }
                }
            }
        }
        AccountTradingStateResponse {
            account_id,
            market_id,
            positions,
            open_orders,
            risk_summary: self.get_account_risk_summary(account_id, market_id, None, false, depth),
            recent_fills: self.get_account_trade_history(account_id, market_id, None, fill_limit),
            next_seq: self.next_event_seq(),
        }
    }

    fn latest_market_trade(&self, market_id: u64) -> Option<TradeFillResponse> {
        self.engine
            .runtime_events()
            .iter()
            .rev()
            .filter_map(|event| trade_fill_from_event(event.clone()))
            .find(|fill| fill.market_id == market_id && fill.role == OrderFillRole::Taker)
    }

    pub(crate) fn next_event_seq(&self) -> u64 {
        self.engine
            .runtime_events()
            .last()
            .map(|event| event.seq.saturating_add(1))
            .unwrap_or(0)
    }

    pub fn get_liquidation_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        liquidator_account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<LiquidationRecordResponse> {
        let mut records = Vec::new();
        for event in self
            .engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
        {
            if let Some(record) = liquidation_record_from_event(event) {
                records.push(record);
                continue;
            }
            if let Some(outcome) = liquidation_outcome_from_event(event) {
                if let Some(record) = records.iter_mut().rev().find(|record| {
                    record.market_id == outcome.market_id
                        && record.account_id == outcome.account_id
                        && record.liquidator_account_id == outcome.liquidator_account_id
                        && record.outcome.is_none()
                }) {
                    record.outcome = Some(outcome.outcome);
                    record.post_net_lots = Some(outcome.post_net_lots);
                    record.post_margin_surplus = Some(outcome.post_margin_surplus);
                    record.post_under_maintenance = Some(outcome.post_under_maintenance);
                    record.post_account_frozen = Some(outcome.post_account_frozen);
                    record.post_market_bad_debt_balance =
                        Some(outcome.post_market_bad_debt_balance);
                }
            }
        }

        records
            .into_iter()
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .filter(|event| {
                liquidator_account_id.is_none_or(|expected| event.liquidator_account_id == expected)
            })
            .take(limit)
            .collect()
    }

    pub fn get_liquidation_escalation_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<LiquidationEscalationRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(liquidation_escalation_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_oracle_history(
        &self,
        market_id: Option<u64>,
        topic: Option<RuntimeEventTopic>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<OracleHistoryRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter(|event| {
                market_id.is_none_or(|expected| event.kind.market_id() == Some(expected))
            })
            .filter(|event| topic.is_none_or(|expected| event.kind.topic() == expected))
            .filter_map(oracle_history_record_from_event)
            .take(limit)
            .collect()
    }

    pub fn get_market_trading_history(
        &self,
        market_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<MarketTradingStateRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(market_trading_state_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_insurance_fund_history(
        &self,
        market_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<InsuranceFundUpdateRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(insurance_fund_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_funding_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<FundingRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(funding_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == Some(expected)))
            .take(limit)
            .collect()
    }

    pub fn get_funding_state(&self, market_id: u64) -> FundingStateResponse {
        FundingStateResponse {
            market_id,
            funding_index: self.engine.market_funding_index(market_id),
            premium_rate_ppm: self.engine.premium_rate_ppm(market_id),
            indicative_funding_rate_ppm: self.engine.indicative_funding_rate_ppm(market_id),
            mark_price_ticks: self.engine.mark_price_ticks(market_id),
            index_price_ticks: self.engine.index_price_ticks(market_id),
        }
    }

    pub fn get_insurance_fund(&self, market_id: u64) -> InsuranceFundResponse {
        InsuranceFundResponse {
            market_id,
            insurance_fund_balance: self.engine.insurance_fund_balance(market_id),
            bad_debt_balance: self.engine.market_bad_debt_balance(market_id),
        }
    }

    pub fn get_bad_debt_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<BadDebtRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(bad_debt_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_solvency_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<SolvencyBreachRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(solvency_breach_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_fee_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        client_order_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<FeeRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(fee_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .filter(|event| {
                client_order_id.is_none_or(|expected| event.client_order_id == expected)
            })
            .take(limit)
            .collect()
    }

    pub fn get_revenue_split_history(
        &self,
        market_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<RevenueSplitRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(revenue_split_record_from_event)
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_position(&self, market_id: u64, account_id: u64) -> PositionResponse {
        position_response_from_ledger(
            self.engine,
            market_id,
            account_id,
            self.engine.position(market_id, account_id),
        )
    }

    pub fn get_position_history(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<PositionHistoryRecordResponse> {
        self.engine
            .runtime_events()
            .iter()
            .filter(|event| event.seq >= from_seq.unwrap_or(0))
            .filter_map(|event| position_history_record_from_event(self.engine, event))
            .filter(|event| market_id.is_none_or(|expected| event.market_id == expected))
            .filter(|event| account_id.is_none_or(|expected| event.account_id == expected))
            .take(limit)
            .collect()
    }

    pub fn get_risk_snapshot(&self, market_id: u64, account_id: u64) -> RiskSnapshotResponse {
        let suggested_liquidation_lots =
            suggested_liquidation_lots(self.engine, market_id, account_id);
        risk_snapshot_from_ledger(
            self.engine,
            market_id,
            account_id,
            self.engine.position(market_id, account_id),
            self.engine.margin_snapshot(market_id, account_id),
            self.engine.mark_price_ticks(market_id),
            RiskSnapshotSupplement {
                liquidation_stall_count: self.engine.liquidation_stall_count(market_id, account_id),
                liquidation_freeze_threshold: self.engine.liquidation_freeze_threshold(),
                liquidation_freeze_imminent: self
                    .engine
                    .liquidation_freeze_imminent(market_id, account_id),
                suggested_liquidation_lots,
                latest_account_risk_transition: latest_account_risk_transition(
                    self.engine,
                    account_id,
                    Some(market_id),
                ),
            },
        )
    }

    pub fn get_suggested_liquidation(
        &self,
        market_id: u64,
        account_id: u64,
    ) -> Option<SuggestedLiquidationPlanResponse> {
        let snapshot = self.get_risk_snapshot(market_id, account_id);
        Some(SuggestedLiquidationPlanResponse {
            market_id,
            account_id,
            current_liquidation_gap: snapshot.liquidation_gap,
            suggested_liquidation_lots: snapshot.suggested_liquidation_lots?,
            suggested_liquidation_outcome: snapshot.suggested_liquidation_outcome?,
            clears_maintenance: snapshot.suggested_liquidation_clears_maintenance,
            post_margin_surplus: snapshot.suggested_post_margin_surplus?,
            post_under_maintenance: snapshot.suggested_post_under_maintenance?,
            post_account_frozen: snapshot.suggested_post_account_frozen?,
            post_market_bad_debt_balance: snapshot.suggested_post_market_bad_debt_balance?,
        })
    }

    pub fn list_suggested_liquidations(
        &self,
        market_id: Option<u64>,
        account_id: Option<u64>,
        outcome: Option<SuggestedLiquidationOutcome>,
        limit: usize,
    ) -> Vec<SuggestedLiquidationPlanResponse> {
        self.list_liquidation_candidates(
            market_id,
            account_id,
            false,
            outcome,
            limit.saturating_mul(4).max(limit),
        )
        .into_iter()
        .filter_map(|snapshot| {
            Some(SuggestedLiquidationPlanResponse {
                market_id: snapshot.market_id,
                account_id: snapshot.account_id,
                current_liquidation_gap: snapshot.liquidation_gap,
                suggested_liquidation_lots: snapshot.suggested_liquidation_lots?,
                suggested_liquidation_outcome: snapshot.suggested_liquidation_outcome?,
                clears_maintenance: snapshot.suggested_liquidation_clears_maintenance,
                post_margin_surplus: snapshot.suggested_post_margin_surplus?,
                post_under_maintenance: snapshot.suggested_post_under_maintenance?,
                post_account_frozen: snapshot.suggested_post_account_frozen?,
                post_market_bad_debt_balance: snapshot.suggested_post_market_bad_debt_balance?,
            })
        })
        .filter(|plan| account_id.is_none_or(|value| plan.account_id == value))
        .filter(|plan| outcome.is_none_or(|value| plan.suggested_liquidation_outcome == value))
        .take(limit)
        .collect()
    }

    pub fn get_perpetual_market(&self, market_id: u64) -> Option<PerpetualMarketResponse> {
        self.engine
            .perpetual_market(market_id)
            .cloned()
            .map(|market| PerpetualMarketResponse { market })
    }

    pub fn list_perpetual_markets(
        &self,
        status: Option<MarketStatus>,
        limit: usize,
    ) -> Vec<PerpetualMarketResponse> {
        self.engine
            .perpetual_markets()
            .filter(|market| status.is_none_or(|value| market.status == value))
            .take(limit)
            .cloned()
            .map(|market| PerpetualMarketResponse { market })
            .collect()
    }

    pub fn get_agent_identity(&self, account: u64) -> AgentIdentityResponse {
        self.engine
            .agent_identity(account)
            .map(|record| {
                agent_identity_record_response(
                    record,
                    self.engine.active_attestation_count_for_account(
                        account,
                        self.engine.head().header.timestamp_ms,
                    ),
                )
            })
            .unwrap_or_else(|| AgentIdentityResponse {
                account,
                tier: IdentityTierResponse::Tier0Unattested,
                operator: None,
                kind: "unknown",
                display_name: String::new(),
                description: String::new(),
                contact_uri: None,
                active_attestation_count: 0,
                registered_at_block: None,
                registered_at_timestamp_ms: None,
                last_updated_block: None,
                last_updated_timestamp_ms: None,
                virtual_default: true,
            })
    }

    pub fn list_attestations(
        &self,
        account: u64,
        kind: Option<&str>,
        include_revoked: bool,
        include_expired: bool,
        limit: usize,
        cursor: Option<&str>,
    ) -> AttestationListResponse {
        let timestamp_ms = self.engine.head().header.timestamp_ms;
        let after_record_id = cursor.and_then(|value| decode_hex_array::<32>(value).ok());
        let mut items = Vec::new();
        let mut next_cursor = None;
        for record in self
            .engine
            .attestations_for_account(account)
            .filter(|record| kind.is_none_or(|value| attestation_kind_name(record.kind) == value))
            .filter(|record| include_revoked || record.revoked_at_block.is_none())
            .filter(|record| include_expired || record.expires_at_timestamp_ms > timestamp_ms)
            .filter(|record| after_record_id.is_none_or(|after| record.record_id > after))
        {
            if items.len() >= limit {
                next_cursor = items
                    .last()
                    .map(|item: &AttestationRecordResponse| item.record_id_hex.clone());
                break;
            }
            items.push(attestation_record_response(record));
        }
        AttestationListResponse {
            items,
            next_cursor,
            scaffold: false,
        }
    }

    pub fn get_composition_access(&self, account: u64) -> CompositionAccessResponse {
        let tier = self
            .engine
            .agent_identity(account)
            .map(|identity| identity.tier)
            .unwrap_or(AgentIdentityTier::Tier0Unattested);
        match tier {
            AgentIdentityTier::Tier0Unattested => CompositionAccessResponse {
                account,
                performance_share_delegator: false,
                performance_share_delegatee: false,
                reputation_collateral: false,
                a2a_contract: false,
            },
            AgentIdentityTier::Tier1OperatorAttested => CompositionAccessResponse {
                account,
                performance_share_delegator: true,
                performance_share_delegatee: false,
                reputation_collateral: false,
                a2a_contract: false,
            },
            AgentIdentityTier::Tier2TeeAttested => {
                let composition_eligible =
                    self.account_has_composition_eligible_tee_attestation(account);
                CompositionAccessResponse {
                    account,
                    performance_share_delegator: true,
                    performance_share_delegatee: composition_eligible,
                    reputation_collateral: composition_eligible,
                    a2a_contract: composition_eligible,
                }
            }
        }
    }

    pub fn get_reputation_multiplier(&self, account: u64) -> ReputationMultiplierResponse {
        ReputationMultiplierResponse {
            account,
            multiplier: "1.0",
        }
    }

    fn account_has_composition_eligible_tee_attestation(&self, account: u64) -> bool {
        let timestamp_ms = self.engine.head().header.timestamp_ms;
        self.engine.attestations_for_account(account).any(|record| {
            if record.kind != AttestationKind::TeeEnclave
                || !record.verified
                || record.revoked_at_block.is_some()
                || record.expires_at_timestamp_ms <= timestamp_ms
            {
                return false;
            }
            let AttestationEvidence::TeeQuoteVerified {
                vendor,
                vendor_root_id,
                ..
            } = record.evidence
            else {
                return false;
            };
            self.engine
                .vendor_cap_profile(vendor)
                .is_some_and(|profile| profile.composition_eligibility)
                && self
                    .engine
                    .vendor_roots(Some(vendor), false)
                    .iter()
                    .any(|root| root.root_id == vendor_root_id)
        })
    }

    pub fn get_identity_action_nonce(&self, account: u64) -> u64 {
        self.engine.identity_action_nonce(account)
    }

    pub fn get_identity_attestor_nonce(&self, account: u64) -> u64 {
        self.engine.identity_attestor_nonce(account)
    }

    pub fn submit_identity_action(
        &mut self,
        params: SubmitIdentityActionParams,
    ) -> Result<AgentIdentityResponse, RpcError> {
        let signed_action = signed_identity_action_from_json(&params.signed_identity_action)?;
        let deleted_account = match &signed_action.action {
            IdentityAction::Delete(input) => Some(input.account),
            _ => None,
        };
        let record = self
            .engine
            .submit_identity_action(signed_action, params.timestamp_ms)
            .map_err(identity_rpc_error)?;
        if let Some(account) = deleted_account {
            return Ok(self.get_agent_identity(account));
        }
        Ok(agent_identity_record_response(
            &record,
            self.engine.active_attestation_count_for_account(
                record.account,
                self.engine.head().header.timestamp_ms,
            ),
        ))
    }

    pub fn submit_tee_attestation(
        &mut self,
        params: SubmitTeeAttestationParams,
    ) -> Result<AgentIdentityResponse, RpcError> {
        let signed_action =
            signed_tee_attestation_action_from_json(&params.signed_tee_attestation_action)?;
        let record = self
            .engine
            .submit_tee_attestation_action(signed_action, params.timestamp_ms)
            .map_err(identity_rpc_error)?;
        Ok(agent_identity_record_response(
            &record,
            self.engine.active_attestation_count_for_account(
                record.account,
                self.engine.head().header.timestamp_ms,
            ),
        ))
    }

    pub fn list_vendor_roots(
        &self,
        vendor: Option<TeeVendorResponse>,
        include_inactive: bool,
    ) -> VendorRootsResponse {
        let vendor_filter = vendor.map(tee_vendor_to_node);
        let roots = self
            .engine
            .vendor_roots(vendor_filter, include_inactive)
            .iter()
            .map(vendor_root_record_response)
            .collect();
        let head_height = self.engine.head().header.height;
        let mut pending_governance_actions = self
            .engine
            .vendor_roots(vendor_filter, true)
            .iter()
            .flat_map(|root| pending_vendor_root_governance_actions(root, head_height))
            .collect::<Vec<_>>();
        pending_governance_actions.extend(
            self.engine
                .pending_vendor_cap_profile_updates(vendor_filter)
                .iter()
                .map(pending_vendor_cap_profile_update_response),
        );
        VendorRootsResponse {
            vendor,
            roots,
            pending_governance_actions,
            tee_attestation_enabled: self.engine.tee_attestation_enabled(),
            scaffold: false,
        }
    }

    pub fn get_vendor_cap_profile(&self, vendor: TeeVendorResponse) -> VendorCapProfileResponse {
        let profile = self
            .engine
            .vendor_cap_profile(tee_vendor_to_node(vendor))
            .expect("RFC 051 launch cap profiles are initialized for every vendor");
        vendor_cap_profile_response(profile, self.engine.tee_attestation_enabled())
    }

    pub fn list_perpetual_market(
        &mut self,
        params: ListPerpetualMarketParams,
    ) -> Result<PerpetualMarketResponse, RpcError> {
        let (oracle_feeder_count, oracle_feeder_allowlist) =
            oracle_feeder_allowlist_from_json(params.oracle_feeder_ids_hex)?;
        let config = PerpetualMarketConfig {
            initial_margin_bps: params.initial_margin_bps,
            maintenance_margin_bps: params.maintenance_margin_bps,
            max_leverage: params.max_leverage,
            lot_size_microunits: params.lot_size_microunits,
            tick_size_bps: params.tick_size_bps,
            funding_interval_ms: params.funding_interval_ms,
            oracle_max_staleness_ms: params.oracle_max_staleness_ms.unwrap_or(5_000),
            breaker_price_band_bps: params.breaker_price_band_bps.unwrap_or(0),
            oracle_min_sources: params
                .oracle_min_sources
                .unwrap_or(DEFAULT_ORACLE_MIN_SOURCES),
            oracle_min_confidence: params
                .oracle_min_confidence
                .unwrap_or(DEFAULT_ORACLE_MIN_CONFIDENCE),
            oracle_min_samples: params
                .oracle_min_samples
                .unwrap_or(DEFAULT_ORACLE_MIN_SAMPLES),
            oracle_feeder_count,
            oracle_feeder_allowlist,
        };
        self.engine
            .list_perpetual_market(
                params.market_id,
                params.base_symbol,
                params.quote_asset.unwrap_or(1),
                params.oracle_feed_id,
                config,
                params.timestamp_ms,
            )
            .map(|market| PerpetualMarketResponse { market })
            .map_err(market_lifecycle_rpc_error)
    }

    pub fn set_perpetual_market_status(
        &mut self,
        market_id: u64,
        status: MarketStatus,
        timestamp_ms: u64,
    ) -> Result<PerpetualMarketResponse, RpcError> {
        self.engine
            .set_perpetual_market_status(market_id, status, timestamp_ms)
            .map(|market| PerpetualMarketResponse { market })
            .map_err(market_lifecycle_rpc_error)
    }

    // v1 doc 05 — Mandates read + Tier-0 create bridge.

    fn mandate_response_for(&self, mandate: Mandate) -> MandateResponse {
        let latest_delta = self.engine.latest_mandate_conformance_delta(&mandate.id);
        MandateResponse {
            mandate,
            latest_delta,
        }
    }

    pub fn get_mandate(&self, mandate_id: &MandateId) -> Option<MandateResponse> {
        self.engine
            .mandate(mandate_id)
            .cloned()
            .map(|mandate| self.mandate_response_for(mandate))
    }

    pub fn list_mandates(
        &self,
        owner: Option<Principal>,
        agent: Option<u64>,
        state: Option<MandateState>,
        limit: usize,
    ) -> Vec<MandateResponse> {
        self.engine
            .mandates()
            .filter(|mandate| owner.is_none_or(|value| mandate.owner == value))
            .filter(|mandate| agent.is_none_or(|value| mandate.agent == value))
            .filter(|mandate| state.is_none_or(|value| mandate.state == value))
            .take(limit)
            .cloned()
            .map(|mandate| self.mandate_response_for(mandate))
            .collect()
    }

    pub fn get_mandate_conformance_history(
        &self,
        mandate_id: MandateId,
        limit: Option<usize>,
    ) -> Result<MandateConformanceHistoryResponse, RpcError> {
        let limit = limit.unwrap_or(DEFAULT_MANDATE_CONFORMANCE_HISTORY_LIMIT);
        if limit == 0 || limit > MANDATE_CONFORMANCE_HISTORY_LIMIT {
            return Err(RpcError::InvalidRequest("limit must be between 1 and 128"));
        }
        Ok(MandateConformanceHistoryResponse {
            mandate_id,
            samples: downsample_mandate_conformance_samples(
                self.engine.mandate_conformance_history_samples(mandate_id),
                limit,
            ),
        })
    }

    fn mandate_activation_tier_satisfied(
        &self,
        agent: u64,
        required_tier: IdentityTierResponse,
        current_tier: IdentityTierResponse,
        mandate_id: &MandateId,
        timestamp_ms: u64,
    ) -> bool {
        match required_tier {
            IdentityTierResponse::Tier0Unattested => true,
            IdentityTierResponse::Tier1OperatorAttested => {
                identity_tier_rank(current_tier) >= identity_tier_rank(required_tier)
                    && self.engine.has_matching_operator_attestation(
                        agent,
                        Some(mandate_id),
                        timestamp_ms,
                    )
            }
            IdentityTierResponse::Tier2TeeAttested => {
                identity_tier_rank(current_tier) >= identity_tier_rank(required_tier)
                    && self.engine.has_matching_tee_attestation_for_mandate(
                        agent,
                        mandate_id,
                        timestamp_ms,
                        self.engine.head().header.height,
                    )
            }
        }
    }

    pub fn create_mandate(
        &mut self,
        params: CreateMandateParams,
    ) -> Result<MandateResponse, RpcError> {
        let required_tier = identity_tier_for_activation(params.activation_tier.unwrap_or(0))?;
        let current_tier = self.get_agent_identity(params.agent).tier;
        let owner = decode_hex_array::<32>(&params.owner_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid owner_hex".into()))?;
        let description_hash = params
            .description_hash_hex
            .as_deref()
            .map(decode_hex_array::<32>)
            .transpose()
            .map_err(|_| RpcError::InvalidRequest("invalid description_hash_hex".into()))?;
        let mut session_keys = Vec::with_capacity(params.session_keys_hex.len());
        for session_key in &params.session_keys_hex {
            session_keys.push(
                decode_hex_array::<32>(session_key)
                    .map_err(|_| RpcError::InvalidRequest("invalid session_keys_hex".into()))?,
            );
        }
        let mut constraints = Vec::with_capacity(params.constraints.len());
        for constraint in params.constraints {
            constraints.push(mandate_constraint_from_param(constraint)?);
        }
        let mut objectives = Vec::with_capacity(params.objectives.len());
        for objective in params.objectives {
            objectives.push(mandate_objective_from_param(objective));
        }
        let mandate_id = derive_mandate_id(
            owner,
            params.agent,
            params.version,
            constraints.clone(),
            objectives.clone(),
            params.objective_weights.clone(),
            params.conformance_window_ms,
            session_keys.clone(),
            params.expires_at,
            description_hash,
        );
        if !self.mandate_activation_tier_satisfied(
            params.agent,
            required_tier,
            current_tier,
            &mandate_id,
            params.timestamp_ms,
        ) {
            return Err(mandate_activation_tier_rpc_error(
                params.agent,
                current_tier,
                required_tier,
            ));
        }
        let mandate = self
            .engine
            .create_mandate(
                CreateMandateInput {
                    owner,
                    agent: params.agent,
                    version: params.version,
                    constraints,
                    objectives,
                    objective_weights: params.objective_weights,
                    conformance_window_ms: params.conformance_window_ms,
                    session_keys,
                    expires_at: params.expires_at,
                    description_uri: params.description_uri,
                    description_hash,
                },
                params.timestamp_ms,
            )
            .map_err(mandate_rpc_error)?;
        Ok(self.mandate_response_for(mandate))
    }

    pub fn submit_mandate_action(
        &mut self,
        params: SubmitMandateActionParams,
    ) -> Result<MandateResponse, RpcError> {
        let signed_action = signed_mandate_action_from_json(&params.signed_mandate_action)?;
        let mandate = self
            .engine
            .submit_mandate_action(signed_action, params.timestamp_ms)
            .map_err(mandate_rpc_error)?;
        Ok(self.mandate_response_for(mandate))
    }

    pub fn submit_evm_mandate_action(
        &mut self,
        params: SubmitEvmMandateActionParams,
    ) -> Result<MandateResponse, RpcError> {
        let signed_action = signed_evm_mandate_action_from_json(&params.signed_mandate_action)?;
        self.verify_evm_mandate_action(&signed_action)?;
        let mandate = self
            .engine
            .submit_verified_mandate_action(
                signed_action.chain_id,
                signed_action.owner,
                signed_action.nonce,
                signed_action.action,
                params.timestamp_ms,
            )
            .map_err(mandate_rpc_error)?;
        Ok(self.mandate_response_for(mandate))
    }

    fn verify_evm_mandate_action(
        &self,
        signed_action: &SignedEvmMandateAction,
    ) -> Result<(), RpcError> {
        if signed_action.chain_id != self.engine.chain_id() {
            return Err(mandate_rpc_error(NodeError::InvalidMandateAction {
                owner: signed_action.owner,
                reason: "chain_mismatch",
            }));
        }
        if signed_action.valid_until_ms < current_unix_ms() {
            return Err(mandate_rpc_error(NodeError::InvalidMandateAction {
                owner: signed_action.owner,
                reason: "signature_expired",
            }));
        }
        let expected_address =
            evm_address_from_owner_principal(signed_action.owner).ok_or_else(|| {
                mandate_rpc_error(NodeError::InvalidMandateAction {
                    owner: signed_action.owner,
                    reason: "evm_signer_not_supported",
                })
            })?;
        let digest = evm_mandate_action_eip712_digest(
            signed_action.chain_id,
            signed_action.owner,
            signed_action.nonce,
            signed_action.valid_until_ms,
            &signed_action.action,
        )
        .map_err(|_| {
            mandate_rpc_error(NodeError::InvalidMandateAction {
                owner: signed_action.owner,
                reason: "invalid_evm_signature",
            })
        })?;
        let recovered =
            recover_evm_owner_call_address(&digest, &signed_action.signature).map_err(|_| {
                mandate_rpc_error(NodeError::InvalidMandateAction {
                    owner: signed_action.owner,
                    reason: "invalid_evm_signature",
                })
            })?;
        if recovered != expected_address {
            return Err(mandate_rpc_error(NodeError::InvalidMandateAction {
                owner: signed_action.owner,
                reason: "evm_signer_mismatch",
            }));
        }
        Ok(())
    }

    pub fn set_account_operator(
        &mut self,
        params: SetAccountOperatorParams,
    ) -> Result<AccountOperatorResponse, RpcError> {
        self.engine
            .set_account_operator(params.account_id, params.operator_id, params.timestamp_ms)
            .map_err(mandate_rpc_error)?;
        Ok(AccountOperatorResponse {
            account_id: params.account_id,
            operator_id: Some(params.operator_id),
        })
    }

    pub fn get_account_operator(&self, account_id: u64) -> AccountOperatorResponse {
        AccountOperatorResponse {
            account_id,
            operator_id: self.engine.account_operator(account_id),
        }
    }

    pub fn get_mandate_action_nonce(&self, owner: &Principal) -> u64 {
        self.engine.mandate_action_nonce(owner)
    }

    pub fn suspend_mandate(
        &mut self,
        params: MandateLifecycleParams,
    ) -> Result<MandateResponse, RpcError> {
        let mandate_id = decode_hex_array::<32>(&params.mandate_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?;
        let mandate = self
            .engine
            .suspend_mandate(mandate_id, params.timestamp_ms)
            .map_err(mandate_rpc_error)?;
        Ok(self.mandate_response_for(mandate))
    }

    pub fn resume_mandate(
        &mut self,
        params: MandateLifecycleParams,
    ) -> Result<MandateResponse, RpcError> {
        let mandate_id = decode_hex_array::<32>(&params.mandate_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?;
        let mandate = self
            .engine
            .resume_mandate(mandate_id, params.timestamp_ms)
            .map_err(mandate_rpc_error)?;
        Ok(self.mandate_response_for(mandate))
    }

    pub fn revoke_mandate(
        &mut self,
        params: RevokeMandateParams,
    ) -> Result<MandateResponse, RpcError> {
        let mandate_id = decode_hex_array::<32>(&params.mandate_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?;
        let mandate = self
            .engine
            .revoke_mandate(
                mandate_id,
                params.reason.unwrap_or_else(|| "owner_revoked".into()),
                params.timestamp_ms,
            )
            .map_err(mandate_rpc_error)?;
        Ok(self.mandate_response_for(mandate))
    }

    pub fn expire_due_mandates(
        &mut self,
        timestamp_ms: u64,
    ) -> Result<Vec<MandateResponse>, RpcError> {
        let mandates = self
            .engine
            .expire_due_mandates(timestamp_ms)
            .map_err(mandate_rpc_error)?;
        Ok(mandates
            .into_iter()
            .map(|mandate| self.mandate_response_for(mandate))
            .collect())
    }

    pub fn list_positions(
        &self,
        account_id: u64,
        include_flat: bool,
        limit: usize,
    ) -> Vec<PositionResponse> {
        self.engine
            .positions(account_id, include_flat, limit)
            .into_iter()
            .map(|(market_id, ledger)| {
                position_response_from_ledger(self.engine, market_id, account_id, ledger)
            })
            .collect()
    }

    pub fn get_account_trade_history(
        &self,
        account_id: u64,
        market_id: Option<u64>,
        from_seq: Option<u64>,
        limit: usize,
    ) -> Vec<TradeFillResponse> {
        EventStream::new(self.engine.runtime_events())
            .query(StreamQuery {
                from_seq: from_seq.unwrap_or(0),
                before_seq: None,
                limit,
                filter: EventFilter {
                    block_height: None,
                    market_id,
                    account_id: Some(account_id),
                    policy_id: None,
                    intent_id: None,
                    client_order_id: None,
                    topic: Some(RuntimeEventTopic::OrderFilled),
                },
                descending: false,
            })
            .events
            .into_iter()
            .filter_map(trade_fill_from_event)
            .collect()
    }

    pub fn get_order_status(
        &self,
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
    ) -> OrderStatusResponse {
        let timestamp_ms = self.engine.head().header.timestamp_ms;
        let resting =
            self.engine
                .resting_order(market_id, account_id, client_order_id, timestamp_ms);

        let mut filled_lots = 0_u64;
        let mut replacement_client_order_id = None;
        let mut latest_event_seq = None;
        let mut latest_block_height = None;
        let mut latest_timestamp_ms = None;
        let mut latest_stage = OrderStatusStage::Unknown;

        for event in self.engine.runtime_events() {
            let kind = &event.kind;
            let is_same_order = match kind {
                kombat_chain_streams::RuntimeEventKind::OrderAccepted {
                    market_id: event_market_id,
                    account_id: event_account_id,
                    client_order_id: event_client_order_id,
                    ..
                }
                | kombat_chain_streams::RuntimeEventKind::OrderFilled {
                    market_id: event_market_id,
                    account_id: event_account_id,
                    client_order_id: event_client_order_id,
                    ..
                }
                | kombat_chain_streams::RuntimeEventKind::OrderCancelled {
                    market_id: event_market_id,
                    account_id: event_account_id,
                    client_order_id: event_client_order_id,
                    ..
                } => {
                    *event_market_id == market_id
                        && *event_account_id == account_id
                        && *event_client_order_id == client_order_id
                }
                kombat_chain_streams::RuntimeEventKind::OrderReplaced {
                    market_id: event_market_id,
                    account_id: event_account_id,
                    target_client_order_id,
                    replacement_client_order_id: event_replacement_client_order_id,
                    ..
                } => {
                    *event_market_id == market_id
                        && *event_account_id == account_id
                        && (*target_client_order_id == client_order_id
                            || *event_replacement_client_order_id == client_order_id)
                }
                _ => false,
            };

            if !is_same_order {
                continue;
            }

            if let kombat_chain_streams::RuntimeEventKind::OrderFilled { quantity_lots, .. } = kind
            {
                filled_lots = filled_lots.saturating_add(*quantity_lots);
            }

            latest_event_seq = Some(event.seq);
            latest_block_height = event.block_height;
            latest_timestamp_ms = Some(event.timestamp_ms);
            latest_stage = match kind {
                kombat_chain_streams::RuntimeEventKind::OrderAccepted { status, .. } => {
                    order_status_stage_from_lifecycle(*status)
                }
                kombat_chain_streams::RuntimeEventKind::OrderFilled { .. } => {
                    if resting.is_some() {
                        OrderStatusStage::PartiallyFilledResting
                    } else {
                        OrderStatusStage::Filled
                    }
                }
                kombat_chain_streams::RuntimeEventKind::OrderCancelled { .. } => {
                    if filled_lots > 0 {
                        OrderStatusStage::PartiallyFilledCancelled
                    } else {
                        OrderStatusStage::Cancelled
                    }
                }
                kombat_chain_streams::RuntimeEventKind::OrderReplaced {
                    target_client_order_id,
                    replacement_client_order_id: event_replacement_client_order_id,
                    status,
                    ..
                } => {
                    if *target_client_order_id == client_order_id {
                        replacement_client_order_id = Some(*event_replacement_client_order_id);
                        OrderStatusStage::Replaced
                    } else {
                        order_status_stage_from_lifecycle(*status)
                    }
                }
                _ => latest_stage,
            };
        }

        let stage = if resting.is_some() {
            if filled_lots > 0 {
                OrderStatusStage::PartiallyFilledResting
            } else {
                OrderStatusStage::Resting
            }
        } else {
            latest_stage
        };

        OrderStatusResponse {
            market_id,
            account_id,
            client_order_id,
            stage,
            filled_lots,
            remaining_lots: resting
                .as_ref()
                .map(|order| order.remaining_lots)
                .unwrap_or(0),
            replacement_client_order_id,
            latest_event_seq,
            block_height: latest_block_height,
            timestamp_ms: latest_timestamp_ms,
        }
    }

    pub fn cancel_order(
        &mut self,
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        timestamp_ms: u64,
    ) -> CancelOrderResponse {
        let cancelled = self
            .engine
            .cancel_order(market_id, account_id, client_order_id, timestamp_ms)
            .map(resting_order_response);
        CancelOrderResponse {
            cancelled: cancelled.is_some(),
            order: cancelled,
        }
    }

    pub fn submit_owner_call(
        &mut self,
        signed_owner_call: SignedOwnerCall,
        timestamp_ms: u64,
    ) -> Result<SubmitOwnerCallResponse, RpcError> {
        self.engine
            .submit_owner_call(signed_owner_call.clone(), timestamp_ms)
            .map_err(|error| owner_call_submission_rpc_error(&signed_owner_call, &error))?;
        Ok(owner_call_response_from_parts(
            signed_owner_call.account_id,
            signed_owner_call.chain_id,
            &signed_owner_call.call,
        ))
    }

    pub fn submit_evm_owner_call(
        &mut self,
        signed_owner_call: SignedEvmOwnerCall,
        timestamp_ms: u64,
    ) -> Result<SubmitOwnerCallResponse, RpcError> {
        self.verify_evm_owner_call(&signed_owner_call)
            .map_err(|error| {
                owner_call_submission_rpc_error_from_parts(
                    signed_owner_call.account_id,
                    &signed_owner_call.call,
                    &NodeError::Account(error),
                )
            })?;
        self.engine
            .submit_verified_owner_call(
                signed_owner_call.account_id,
                signed_owner_call.chain_id,
                signed_owner_call.call.clone(),
                timestamp_ms,
            )
            .map_err(|error| {
                owner_call_submission_rpc_error_from_parts(
                    signed_owner_call.account_id,
                    &signed_owner_call.call,
                    &error,
                )
            })?;
        Ok(owner_call_response_from_parts(
            signed_owner_call.account_id,
            signed_owner_call.chain_id,
            &signed_owner_call.call,
        ))
    }

    fn verify_evm_owner_call(
        &self,
        signed_owner_call: &SignedEvmOwnerCall,
    ) -> Result<(), AccountError> {
        if signed_owner_call.chain_id != self.engine.chain_id() {
            return Err(AccountError::ChainMismatch {
                expected: self.engine.chain_id(),
                actual: signed_owner_call.chain_id,
            });
        }
        let account = self
            .engine
            .account_store()
            .get(signed_owner_call.account_id)
            .ok_or(AccountError::UnknownAccount(signed_owner_call.account_id))?;
        let linked_address = self
            .account_evm_address(signed_owner_call.account_id)
            .ok_or(AccountError::Unauthorized)?;
        let expected_signer = evm_owner_principal(linked_address);
        if account.owner() != expected_signer || signed_owner_call.call.signer != expected_signer {
            return Err(AccountError::Unauthorized);
        }
        let digest = evm_owner_call_eip712_digest(
            signed_owner_call.account_id,
            signed_owner_call.chain_id,
            &signed_owner_call.call,
        )
        .map_err(|_| AccountError::InvalidOwnerSignature)?;
        let recovered = recover_evm_owner_call_address(&digest, &signed_owner_call.signature)
            .map_err(|_| AccountError::InvalidOwnerSignature)?;
        if recovered != linked_address {
            return Err(AccountError::InvalidOwnerSignature);
        }
        Ok(())
    }

    pub fn submit_order(
        &mut self,
        order: OrderRequest,
        timestamp_ms: u64,
    ) -> Result<SubmitOrderResponse, RpcError> {
        self.engine
            .submit_order(order, timestamp_ms)
            .map(submit_order_response)
            .map_err(|error| order_submission_rpc_error(timestamp_ms, &error))
    }

    pub fn preview_intent(
        &self,
        order: OrderRequest,
        replace_client_order_id: Option<u64>,
        timestamp_ms: Option<u64>,
    ) -> Result<PreviewIntentResponse, RpcError> {
        let timestamp_ms = timestamp_ms.unwrap_or_else(|| self.engine.current_time_ms());
        self.engine
            .preview_order_initial_margin(order, replace_client_order_id, timestamp_ms)
            .map(preview_intent_response)
            .map_err(|error| order_submission_rpc_error(timestamp_ms, &error))
    }

    pub fn liquidate_position(
        &mut self,
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
        client_order_id: u64,
        requested_lots: Option<u64>,
        timestamp_ms: u64,
    ) -> Result<LiquidationResponse, RpcError> {
        let event_start = self.engine.runtime_events().len();
        let liquidation = match requested_lots {
            Some(requested_lots) => self.engine.liquidate_position_with_size(
                market_id,
                account_id,
                liquidator_account_id,
                client_order_id,
                Some(requested_lots),
                timestamp_ms,
            ),
            None => self.engine.liquidate_position(
                market_id,
                account_id,
                liquidator_account_id,
                client_order_id,
                timestamp_ms,
            ),
        };
        liquidation
            .map(|outcome| {
                let post_position = self.engine.position(market_id, account_id);
                let post_margin = self.engine.margin_snapshot(market_id, account_id);
                let (
                    step_count,
                    requested_lots,
                    executed_notional,
                    penalty,
                    insurance_cover,
                    liquidator_reward,
                    insurance_fund_balance,
                ) = liquidation_event_totals(
                    &self.engine.runtime_events()[event_start..],
                    market_id,
                    account_id,
                    liquidator_account_id,
                );

                LiquidationResponse {
                    market_id,
                    account_id,
                    liquidator_account_id,
                    step_count,
                    status: outcome.status,
                    fills: outcome.fills,
                    requested_lots,
                    filled_lots: outcome.filled_lots,
                    remaining_lots: outcome.remaining_lots,
                    executed_notional,
                    penalty,
                    insurance_cover,
                    liquidator_reward,
                    insurance_fund_balance,
                    post_net_lots: post_position.net_lots(),
                    post_equity: post_margin.equity,
                    post_maintenance_requirement: post_margin.maintenance_requirement,
                    post_margin_surplus: post_margin.margin_surplus,
                    post_under_maintenance: post_margin.under_maintenance,
                    post_account_frozen: self.engine.account_frozen(account_id),
                    post_market_bad_debt_balance: self.engine.market_bad_debt_balance(market_id),
                }
            })
            .map_err(|error| order_submission_rpc_error(timestamp_ms, &error))
    }

    pub fn run_liquidation_pass(
        &mut self,
        liquidator_account_id: u64,
        first_client_order_id: u64,
        max_positions: usize,
        timestamp_ms: u64,
    ) -> LiquidationPassResponse {
        let report = self.engine.run_liquidation_pass(
            liquidator_account_id,
            first_client_order_id,
            max_positions,
            timestamp_ms,
        );
        LiquidationPassResponse {
            liquidator_account_id,
            scanned_positions: report.scanned_positions,
            attempted_positions: report.attempted_positions,
            succeeded_positions: report.succeeded_positions,
            failed_positions: report.failed_positions,
            skipped_stale_markets: report.skipped_stale_markets,
            skipped_frozen_accounts: report.skipped_frozen_accounts,
            steps: report
                .steps
                .into_iter()
                .map(|step| match step.result {
                    Ok(outcome) => LiquidationPassStepResponse {
                        market_id: step.market_id,
                        account_id: step.account_id,
                        client_order_id: step.client_order_id,
                        status: Some(outcome.status),
                        filled_lots: outcome.filled_lots,
                        remaining_lots: outcome.remaining_lots,
                        reason: None,
                    },
                    Err(error) => LiquidationPassStepResponse {
                        market_id: step.market_id,
                        account_id: step.account_id,
                        client_order_id: step.client_order_id,
                        status: None,
                        filled_lots: 0,
                        remaining_lots: 0,
                        reason: Some(order_submission_reason(&error)),
                    },
                })
                .collect(),
        }
    }

    pub fn preview_liquidation(
        &self,
        market_id: u64,
        account_id: u64,
        liquidator_account_id: u64,
        client_order_id: u64,
        requested_lots: Option<u64>,
        timestamp_ms: u64,
    ) -> Result<LiquidationResponse, RpcError> {
        let mut preview_engine = self.engine.clone();
        NodeRpcService::new(&mut preview_engine).liquidate_position(
            market_id,
            account_id,
            liquidator_account_id,
            client_order_id,
            requested_lots,
            timestamp_ms,
        )
    }

    pub fn replace_order(
        &mut self,
        market_id: u64,
        account_id: u64,
        client_order_id: u64,
        replacement: OrderRequest,
        timestamp_ms: u64,
    ) -> Result<SubmitOrderResponse, RpcError> {
        self.engine
            .replace_order(
                market_id,
                account_id,
                client_order_id,
                replacement,
                timestamp_ms,
            )
            .map(submit_order_response)
            .map_err(|error| order_submission_rpc_error(timestamp_ms, &error))
    }

    pub fn produce_block(
        &mut self,
        timestamp_ms: u64,
        max_items: usize,
    ) -> Result<ProduceBlockResponse, RpcError> {
        let block = self
            .engine
            .produce_block_from_mempool(timestamp_ms, max_items)
            .map_err(|error| RpcError::Node(format!("{error:?}")))?;
        Ok(ProduceBlockResponse {
            height: block.header.height,
            hash: kombat_chain_consensus::block_hash(block),
            state_root: block.header.state_root,
            item_count: block.body.items.len(),
        })
    }

    pub fn save_snapshot_file(&mut self, path: String) -> Result<SnapshotFileResponse, RpcError> {
        // RFC 042 Phase 8: snapshot RPC writes the canonical Phase 3
        // logical-snapshot format. The legacy borsh-blob path on the
        // engine (`save_snapshot_file`/`load_snapshot_file`) is no
        // longer wired through RPC.
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.engine
            .save_logical_snapshot_file(&path, timestamp_ms)
            .map_err(|error| RpcError::Node(format!("failed to save snapshot: {error}")))?;
        let snapshot_size_bytes = fs::metadata(&path)
            .map_err(|error| RpcError::Node(format!("failed to inspect snapshot: {error}")))?
            .len();
        Ok(snapshot_file_response(
            path,
            snapshot_size_bytes,
            self.chain_info(),
        ))
    }

    pub fn load_snapshot_file(
        &mut self,
        path: String,
        force_reset: bool,
    ) -> Result<SnapshotFileResponse, RpcError> {
        let snapshot_size_bytes = fs::metadata(&path)
            .map_err(|error| RpcError::Node(format!("failed to inspect snapshot: {error}")))?
            .len();
        self.engine
            .load_logical_snapshot_file(&path, force_reset)
            .map_err(|error| RpcError::Node(format!("failed to load snapshot: {error}")))?;
        Ok(snapshot_file_response(
            path,
            snapshot_size_bytes,
            self.chain_info(),
        ))
    }

    pub fn list_events(
        &self,
        from_seq: Option<u64>,
        before_seq: Option<u64>,
        limit: usize,
        filter: EventFilter,
        descending: bool,
    ) -> EventPage {
        EventStream::new(self.engine.runtime_events()).query(StreamQuery {
            from_seq: from_seq.unwrap_or(0),
            before_seq,
            limit,
            filter,
            descending,
        })
    }
}

impl<Store> RpcBackend for RpcService<Store>
where
    Store: CheckpointableStateStore,
    <Store as CheckpointableStateStore>::Checkpoint: StateView,
{
    fn chain_info(&self) -> Result<ChainInfoResponse, RpcError> {
        let head = self.head.as_ref().ok_or(RpcError::HeadNotSet)?;

        Ok(ChainInfoResponse {
            chain_id: self.chain_id,
            source_network_id: source_network_id_for_chain_id(self.chain_id),
            head_height: head.header.height,
            head_timestamp_ms: head.header.timestamp_ms,
            head_hash: kombat_chain_consensus::block_hash(head),
            committed_state_root: self.executor.committed_root(),
            quarantined_block_count: self.executor.quarantined_blocks().len(),
        })
    }

    fn get_state(&self, key: &[u8]) -> Result<StateQueryResponse, RpcError> {
        let checkpoint = self.executor.state_store().checkpoint();
        Ok(StateQueryResponse {
            key: key.to_vec(),
            value: checkpoint.get(key),
        })
    }

    fn get_state_root_at_height(&self, height: u64) -> Result<StateRootAtHeightResponse, RpcError> {
        let head = self.head.as_ref().ok_or(RpcError::HeadNotSet)?;
        if head.header.height != height {
            return Err(RpcError::InvalidRequest(
                "state root at height unavailable in stateless rpc backend",
            ));
        }
        Ok(StateRootAtHeightResponse {
            height,
            block_hash: kombat_chain_consensus::block_hash(head),
            state_root: head.header.state_root,
        })
    }

    fn eth_call(&self, request: &ContractCallRequest) -> Result<ContractCallResponse, RpcError> {
        if request.gas_limit == 0 {
            return Err(RpcError::InvalidRequest("gas_limit must be non-zero"));
        }

        let head = self.head.as_ref().ok_or(RpcError::HeadNotSet)?;
        let context = ExecutionContext {
            chain_id: self.chain_id,
            height: head.header.height.saturating_add(1),
            timestamp_ms: head.header.timestamp_ms,
            parent_state_root: self.executor.committed_root(),
        };
        let response = self
            .executor
            .execute_contract_call(
                &context,
                request.caller.into(),
                request.to.into(),
                request.data.clone(),
                request.gas_limit,
            )
            .map_err(|error| RpcError::Node(format!("eth_call failed: {error:?}")))?;

        Ok(ContractCallResponse {
            returned: response.returned,
            gas_used: response.gas_used,
            state_root: self.executor.committed_root(),
        })
    }

    fn list_precompiles(&self) -> Vec<PrecompileDescriptor> {
        protocol_precompiles()
    }
}
