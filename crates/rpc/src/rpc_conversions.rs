use crate::*;

pub(crate) fn parse_eth_call_request(
    params: Value,
) -> Result<ContractCallRequest, JsonRpcErrorObject> {
    let values = match params {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        value => vec![value],
    };
    let call: EthCallObject =
        serde_json::from_value(values.first().cloned().ok_or_else(|| JsonRpcErrorObject {
            code: -32602,
            message: "invalid params".into(),
            data: None,
        })?)
        .map_err(|_| JsonRpcErrorObject {
            code: -32602,
            message: "invalid params".into(),
            data: None,
        })?;
    let caller = call
        .from
        .as_deref()
        .map(decode_hex_array::<20>)
        .transpose()
        .map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?
        .unwrap_or([0_u8; 20]);
    let to = decode_hex_array::<20>(&call.to).map_err(|message| JsonRpcErrorObject {
        code: -32602,
        message,
        data: None,
    })?;
    let data = call
        .data
        .as_deref()
        .map(decode_hex_bytes)
        .transpose()
        .map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?
        .unwrap_or_default();
    let gas_limit = call
        .gas
        .as_deref()
        .map(parse_hex_quantity_u64)
        .transpose()
        .map_err(|message| JsonRpcErrorObject {
            code: -32602,
            message,
            data: None,
        })?
        .unwrap_or(30_000_000);
    Ok(ContractCallRequest {
        caller,
        to,
        data,
        gas_limit,
    })
}

pub(crate) fn parse_single_hex_bytes_param(params: Value) -> Result<Vec<u8>, JsonRpcErrorObject> {
    let values = match params {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        value => vec![value],
    };
    let value = values
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcErrorObject {
            code: -32602,
            message: "invalid params".into(),
            data: None,
        })?;
    decode_hex_bytes(value).map_err(|message| JsonRpcErrorObject {
        code: -32602,
        message,
        data: None,
    })
}

pub(crate) fn parse_single_tx_hash_param(params: Value) -> Result<Hash, JsonRpcErrorObject> {
    let values = match params {
        Value::Array(values) => values,
        Value::Null => Vec::new(),
        value => vec![value],
    };
    let value = values
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| JsonRpcErrorObject {
            code: -32602,
            message: "invalid params".into(),
            data: None,
        })?;
    decode_hex_array::<32>(value).map_err(|message| JsonRpcErrorObject {
        code: -32602,
        message,
        data: None,
    })
}

pub(crate) fn eth_log_to_json(
    log: &EvmLogRecord,
    receipt: &kombat_chain_node::EvmTransactionReceiptRecord,
    transaction_hash: &Hash,
    log_index: u64,
) -> Value {
    json!({
        "address": encode_hex(&log.address),
        "blockHash": encode_hex(&receipt.block_hash),
        "blockNumber": encode_hex_quantity_u64(receipt.block_number),
        "data": encode_hex(&log.data),
        "logIndex": encode_hex_quantity_u64(log_index),
        "removed": false,
        "topics": log
            .topics
            .iter()
            .map(|topic| encode_hex(topic))
            .collect::<Vec<_>>(),
        "transactionHash": encode_hex(transaction_hash),
        "transactionIndex": encode_hex_quantity_u64(receipt.transaction_index),
    })
}

pub(crate) fn eth_transaction_receipt_to_json(
    receipt: &kombat_chain_node::EvmTransactionReceiptRecord,
    raw_tx: &[u8],
) -> Result<Value, JsonRpcErrorObject> {
    let decoded = decode_raw_evm_transaction(raw_tx).map_err(|message| JsonRpcErrorObject {
        code: -32000,
        message,
        data: None,
    })?;
    Ok(json!({
        "blockHash": encode_hex(&receipt.block_hash),
        "blockNumber": encode_hex_quantity_u64(receipt.block_number),
        "contractAddress": Value::Null,
        "cumulativeGasUsed": encode_hex_quantity_u64(receipt.cumulative_gas_used),
        "effectiveGasPrice": encode_hex_quantity_u256(U256::ZERO),
        "from": encode_hex(&receipt.from),
        "gasUsed": encode_hex_quantity_u64(receipt.gas_used),
        "logs": receipt.logs.iter().enumerate().map(|(offset, log)| {
            eth_log_to_json(
                log,
                receipt,
                &receipt.tx_hash,
                receipt.log_index_start.saturating_add(offset as u64),
            )
        }).collect::<Vec<_>>(),
        "logsBloom": encode_hex(&vec![0_u8; 256]),
        "status": if receipt.success { "0x1" } else { "0x0" },
        "to": receipt.to.map(|address| Value::String(encode_hex(&address))).unwrap_or(Value::Null),
        "transactionHash": encode_hex(&receipt.tx_hash),
        "transactionIndex": encode_hex_quantity_u64(receipt.transaction_index),
        "type": encode_hex_quantity_u64(u64::from(decoded.tx_type)),
    }))
}

pub(crate) fn eth_transaction_to_json(
    receipt: &kombat_chain_node::EvmTransactionReceiptRecord,
    raw_tx: &[u8],
) -> Result<Value, JsonRpcErrorObject> {
    let decoded = decode_raw_evm_transaction(raw_tx).map_err(|message| JsonRpcErrorObject {
        code: -32000,
        message,
        data: None,
    })?;
    Ok(json!({
        "blockHash": encode_hex(&receipt.block_hash),
        "blockNumber": encode_hex_quantity_u64(receipt.block_number),
        "from": encode_hex(&decoded.from),
        "gas": encode_hex_quantity_u64(decoded.gas_limit),
        "gasPrice": encode_hex_quantity_u256(U256::ZERO),
        "hash": encode_hex(&decoded.tx_hash),
        "input": encode_hex(&decoded.input),
        "nonce": encode_hex_quantity_u64(decoded.nonce),
        "to": decoded.to.map(|address| Value::String(encode_hex(&address))).unwrap_or(Value::Null),
        "transactionIndex": encode_hex_quantity_u64(receipt.transaction_index),
        "value": encode_hex_quantity_u256(decoded.value),
        "type": encode_hex_quantity_u64(u64::from(decoded.tx_type)),
    }))
}

pub(crate) fn oracle_feed_sample_from_json(
    value: OracleFeedSampleJson,
) -> Result<OracleFeedSample, String> {
    Ok(OracleFeedSample {
        feeder_id: decode_hex_array(&value.feeder_id_hex)?,
        price_ticks: value.price_ticks,
        sources_used: value.sources_used,
        confidence: value.confidence,
    })
}

pub(crate) fn oracle_feeder_allowlist_from_json(
    feeder_ids_hex: Option<Vec<String>>,
) -> Result<(u8, [[u8; 32]; MAX_ORACLE_FEEDERS]), RpcError> {
    let Some(feeder_ids_hex) = feeder_ids_hex else {
        return Ok((DEFAULT_ORACLE_FEEDER_COUNT, DEFAULT_ORACLE_FEEDERS));
    };
    if feeder_ids_hex.is_empty() || feeder_ids_hex.len() > MAX_ORACLE_FEEDERS {
        return Err(RpcError::InvalidRequest(
            "invalid oracle_feeder_ids_hex".into(),
        ));
    }
    let mut allowlist = [[0_u8; 32]; MAX_ORACLE_FEEDERS];
    for (index, feeder_id_hex) in feeder_ids_hex.iter().enumerate() {
        allowlist[index] = decode_hex_array(feeder_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid oracle_feeder_ids_hex".into()))?;
    }
    Ok((feeder_ids_hex.len() as u8, allowlist))
}

pub(crate) fn oracle_feeder_ids_to_json(config: &PerpetualMarketConfig) -> Vec<String> {
    config.oracle_feeder_allowlist[..usize::from(config.oracle_feeder_count)]
        .iter()
        .map(|feeder_id| encode_hex(feeder_id))
        .collect()
}

pub(crate) fn should_force_snapshot_catch_up(error: &NodeError) -> bool {
    matches!(
        error,
        NodeError::InvalidBlock(
            kombat_chain_consensus::BlockValidationError::InvalidHeight
                | kombat_chain_consensus::BlockValidationError::InvalidParentHash
                | kombat_chain_consensus::BlockValidationError::InvalidQuorumCertificate
                | kombat_chain_consensus::BlockValidationError::InvalidTimeoutCertificate
        )
    )
}

pub(crate) fn json_rpc_error_from_node_error(error: NodeError) -> JsonRpcErrorObject {
    JsonRpcErrorObject {
        code: -32000,
        message: format!("{error:?}"),
        data: None,
    }
}

pub(crate) fn signed_intent_from_json(input: &SignedIntentJson) -> Result<SignedIntent, String> {
    Ok(SignedIntent {
        intent: intent_from_json(&input.intent)?,
        signature: decode_hex_array(&input.signature_hex)?,
    })
}

pub(crate) fn signed_owner_call_from_json(
    input: &SignedOwnerCallJson,
) -> Result<SignedOwnerCall, String> {
    Ok(SignedOwnerCall {
        account_id: input.account_id,
        chain_id: input.chain_id,
        call: OwnerCall {
            signer: decode_hex_array(&input.call.signer_hex)?,
            nonce: input.call.nonce,
            action: owner_action_from_json(&input.call.action)?,
        },
        signature: decode_hex_array(&input.signature_hex)?,
    })
}

pub(crate) fn signed_evm_owner_call_from_json(
    input: &SignedEvmOwnerCallJson,
) -> Result<SignedEvmOwnerCall, String> {
    let signature = decode_hex_bytes(&input.signature_hex)?;
    if signature.len() != 65 {
        return Err("invalid signature_hex".into());
    }
    Ok(SignedEvmOwnerCall {
        account_id: input.account_id,
        chain_id: input.chain_id,
        call: OwnerCall {
            signer: decode_hex_array(&input.call.signer_hex)?,
            nonce: input.call.nonce,
            action: owner_action_from_json(&input.call.action)?,
        },
        signature,
    })
}

pub(crate) fn signed_evm_account_claim_from_json(
    input: &SignedEvmAccountClaimJson,
) -> Result<SignedEvmAccountClaim, String> {
    let signature = decode_hex_bytes(&input.signature_hex)?;
    if signature.len() != 65 {
        return Err("invalid signature_hex".into());
    }
    Ok(SignedEvmAccountClaim {
        account_id: input.account_id,
        chain_id: input.chain_id,
        wallet: decode_hex_array(&input.wallet_address_hex)?,
        owner: decode_hex_array(&input.owner_hex)?,
        code_hash: decode_hex_array(&input.code_hash_hex)?,
        expires_at: input.expires_at,
        signature,
    })
}

pub(crate) fn signed_mandate_action_from_json(
    input: &SignedMandateActionJson,
) -> Result<SignedMandateAction, RpcError> {
    let owner = decode_hex_array::<32>(&input.owner_hex)
        .map_err(|_| RpcError::InvalidRequest("invalid owner_hex".into()))?;
    Ok(SignedMandateAction {
        chain_id: input.chain_id,
        owner,
        nonce: input.nonce,
        action: mandate_action_from_json(owner, &input.action)?,
        signature: decode_hex_array::<64>(&input.signature_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid signature_hex".into()))?,
    })
}

pub(crate) fn signed_identity_action_from_json(
    input: &SignedIdentityActionJson,
) -> Result<SignedIdentityAction, RpcError> {
    Ok(SignedIdentityAction {
        chain_id: input.chain_id,
        account: input.account,
        nonce: input.nonce,
        action: identity_action_from_json(&input.action)?,
        signature: decode_hex_array::<64>(&input.signature_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid signature_hex".into()))?,
    })
}

pub(crate) fn signed_tee_attestation_action_from_json(
    input: &SignedTeeAttestationActionJson,
) -> Result<SignedTeeAttestationAction, RpcError> {
    Ok(SignedTeeAttestationAction {
        chain_id: input.chain_id,
        action: tee_attestation_action_from_json(&input.action)?,
        signers: tee_attestation_signers_from_json(&input.signers)?,
    })
}

pub(crate) fn signed_evm_mandate_action_from_json(
    input: &SignedEvmMandateActionJson,
) -> Result<SignedEvmMandateAction, RpcError> {
    let owner = decode_hex_array::<32>(&input.owner_hex)
        .map_err(|_| RpcError::InvalidRequest("invalid owner_hex".into()))?;
    let signature = decode_hex_bytes(&input.signature_hex)
        .map_err(|_| RpcError::InvalidRequest("invalid signature_hex".into()))?;
    if signature.len() != 65 {
        return Err(RpcError::InvalidRequest("invalid signature_hex".into()));
    }
    Ok(SignedEvmMandateAction {
        chain_id: input.chain_id,
        owner,
        nonce: input.nonce,
        valid_until_ms: input.valid_until_ms,
        action: mandate_action_from_json(owner, &input.action)?,
        signature,
    })
}

pub(crate) fn signed_delegation_action_from_json(
    input: &SignedDelegationActionJson,
) -> Result<SignedDelegationAction, RpcError> {
    Ok(SignedDelegationAction {
        chain_id: input.chain_id,
        signer_account_id: input.signer_account_id,
        action_id: decode_hex_array::<32>(&input.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?,
        timestamp_ms: input.timestamp_ms,
        submitter_nonce: input.submitter_nonce,
        action: decode_borsh_hex::<BlockDelegationAction>(&input.action_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_hex".into()))?,
        signature: decode_hex_array::<64>(&input.signature_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid signature_hex".into()))?,
    })
}

pub(crate) fn signed_evm_delegation_action_from_json(
    input: &SignedEvmDelegationActionJson,
) -> Result<SignedEvmDelegationAction, RpcError> {
    let signature = decode_hex_bytes(&input.signature_hex)
        .map_err(|_| RpcError::InvalidRequest("invalid signature_hex".into()))?;
    if signature.len() != 65 {
        return Err(RpcError::InvalidRequest("invalid signature_hex".into()));
    }
    Ok(SignedEvmDelegationAction {
        chain_id: input.chain_id,
        signer_account_id: input.signer_account_id,
        action_id: decode_hex_array::<32>(&input.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?,
        timestamp_ms: input.timestamp_ms,
        submitter_nonce: input.submitter_nonce,
        valid_until_ms: input.valid_until_ms,
        action: decode_borsh_hex::<BlockDelegationAction>(&input.action_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_hex".into()))?,
        signature,
    })
}

pub(crate) fn signed_market_maker_program_action_from_json(
    input: &SignedMarketMakerProgramActionJson,
) -> Result<SignedMarketMakerProgramAction, RpcError> {
    Ok(SignedMarketMakerProgramAction {
        chain_id: input.chain_id,
        signer_account_id: input.signer_account_id,
        action_id: decode_hex_array::<32>(&input.action_id_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_id_hex".into()))?,
        timestamp_ms: input.timestamp_ms,
        submitter_nonce: input.submitter_nonce,
        action: decode_borsh_hex::<BlockMarketMakerProgramAction>(&input.action_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid action_hex".into()))?,
        signature: decode_hex_array::<64>(&input.signature_hex)
            .map_err(|_| RpcError::InvalidRequest("invalid signature_hex".into()))?,
    })
}

pub(crate) fn identity_action_from_json(
    input: &IdentityActionJson,
) -> Result<IdentityAction, RpcError> {
    match input {
        IdentityActionJson::Register {
            account,
            display_name,
            description,
            agent_kind,
            operator,
            contact_uri,
        } => Ok(IdentityAction::Register(RegisterAgentIdentityInput {
            account: *account,
            display_name: display_name.clone(),
            description: description.clone(),
            kind: match agent_kind.as_deref() {
                None => AgentKind::Unknown,
                Some(value) => parse_agent_kind(value)
                    .ok_or_else(|| RpcError::InvalidRequest("invalid agent_kind".into()))?,
            },
            operator: *operator,
            contact_uri: contact_uri.clone(),
        })),
        IdentityActionJson::UpdateDescriptive { account, update } => Ok(
            IdentityAction::UpdateDescriptive(UpdateAgentIdentityInput {
                account: *account,
                update: descriptive_update_from_json(update)?,
            }),
        ),
        IdentityActionJson::RegisterOperatorAttestation {
            attestor,
            subject,
            claim,
            expires_at_timestamp_ms,
        } => Ok(IdentityAction::RegisterOperatorAttestation(
            RegisterOperatorAttestationInput {
                attestor: *attestor,
                subject: *subject,
                claim: attestation_claim_from_json(claim)?,
                expires_at_timestamp_ms: *expires_at_timestamp_ms,
            },
        )),
        IdentityActionJson::RefreshOperatorAttestation {
            attestor,
            record_id_hex,
            new_expires_at_timestamp_ms,
        } => Ok(IdentityAction::RefreshOperatorAttestation(
            RefreshOperatorAttestationInput {
                attestor: *attestor,
                record_id: decode_hex_array::<32>(record_id_hex)
                    .map_err(|_| RpcError::InvalidRequest("invalid record_id_hex".into()))?,
                new_expires_at_timestamp_ms: *new_expires_at_timestamp_ms,
            },
        )),
        IdentityActionJson::RevokeAttestation {
            revoker,
            record_id_hex,
        } => Ok(IdentityAction::RevokeAttestation(RevokeAttestationInput {
            revoker: *revoker,
            record_id: decode_hex_array::<32>(record_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid record_id_hex".into()))?,
        })),
        IdentityActionJson::Delete { account } => {
            Ok(IdentityAction::Delete(DeleteAgentIdentityInput {
                account: *account,
            }))
        }
    }
}

pub(crate) fn tee_attestation_action_from_json(
    input: &TeeAttestationActionJson,
) -> Result<TeeAttestationAction, RpcError> {
    match input {
        TeeAttestationActionJson::RegisterTeeAttestation {
            attestor,
            subject,
            claim,
            expires_at_timestamp_ms,
            quote_hex,
            nonce_hex,
            freshness_anchor_block,
            vendor,
        } => {
            let vendor = parse_tee_vendor(vendor)
                .ok_or_else(|| RpcError::InvalidRequest("invalid tee vendor".into()))?;
            Ok(TeeAttestationAction::Register(
                RegisterTeeAttestationInput {
                    attestor: *attestor,
                    subject: *subject,
                    claim: attestation_claim_from_json(claim)?,
                    expires_at_timestamp_ms: *expires_at_timestamp_ms,
                    quote_bytes: decode_hex_bytes(quote_hex)
                        .map_err(|_| RpcError::InvalidRequest("invalid quote_hex".into()))?,
                    nonce: decode_hex_array::<32>(nonce_hex)
                        .map_err(|_| RpcError::InvalidRequest("invalid nonce_hex".into()))?,
                    freshness_anchor_block: *freshness_anchor_block,
                    vendor: tee_vendor_to_node(vendor),
                },
            ))
        }
        TeeAttestationActionJson::RefreshTeeAttestation {
            attestor,
            record_id_hex,
            new_expires_at_timestamp_ms,
            quote_hex,
            nonce_hex,
            freshness_anchor_block,
        } => Ok(TeeAttestationAction::Refresh(RefreshTeeAttestationInput {
            attestor: *attestor,
            record_id: decode_hex_array::<32>(record_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid record_id_hex".into()))?,
            new_expires_at_timestamp_ms: *new_expires_at_timestamp_ms,
            quote_bytes: decode_hex_bytes(quote_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid quote_hex".into()))?,
            nonce: decode_hex_array::<32>(nonce_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid nonce_hex".into()))?,
            freshness_anchor_block: *freshness_anchor_block,
        })),
    }
}

pub(crate) fn tee_attestation_signers_from_json(
    input: &TeeAttestationSignersJson,
) -> Result<TeeAttestationSigners, RpcError> {
    match input {
        TeeAttestationSignersJson::SelfAttest {
            attestor_nonce,
            attestor_signature_hex,
        } => Ok(TeeAttestationSigners::SelfAttest {
            attestor_nonce: *attestor_nonce,
            attestor_signature: decode_hex_array::<64>(attestor_signature_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid attestor_signature_hex".into()))?,
        }),
        TeeAttestationSignersJson::OperatorAttest {
            attestor_nonce,
            attestor_signature_hex,
            subject_nonce,
            subject_signature_hex,
        } => Ok(TeeAttestationSigners::OperatorAttest {
            attestor_nonce: *attestor_nonce,
            attestor_signature: decode_hex_array::<64>(attestor_signature_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid attestor_signature_hex".into()))?,
            subject_nonce: *subject_nonce,
            subject_signature: decode_hex_array::<64>(subject_signature_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid subject_signature_hex".into()))?,
        }),
    }
}

pub(crate) fn descriptive_update_from_json(
    input: &DescriptiveUpdateJson,
) -> Result<DescriptiveUpdate, RpcError> {
    match input {
        DescriptiveUpdateJson::DisplayName { value } => {
            Ok(DescriptiveUpdate::DisplayName(value.clone()))
        }
        DescriptiveUpdateJson::Description { value } => {
            Ok(DescriptiveUpdate::Description(value.clone()))
        }
        DescriptiveUpdateJson::Kind { value } => Ok(DescriptiveUpdate::Kind(
            parse_agent_kind(value)
                .ok_or_else(|| RpcError::InvalidRequest("invalid kind".into()))?,
        )),
        DescriptiveUpdateJson::Operator { value } => Ok(DescriptiveUpdate::Operator(*value)),
        DescriptiveUpdateJson::ContactUri { value } => {
            Ok(DescriptiveUpdate::ContactUri(value.clone()))
        }
    }
}

pub(crate) fn mandate_action_from_json(
    owner: Principal,
    input: &MandateActionJson,
) -> Result<MandateAction, RpcError> {
    match input {
        MandateActionJson::CreateMandate {
            agent,
            version,
            constraints,
            objectives,
            objective_weights,
            conformance_window_ms,
            session_keys_hex,
            expires_at,
            description_uri,
            description_hash_hex,
        } => {
            let description_hash = description_hash_hex
                .as_deref()
                .map(decode_hex_array::<32>)
                .transpose()
                .map_err(|_| RpcError::InvalidRequest("invalid description_hash_hex".into()))?;
            let mut session_keys = Vec::with_capacity(session_keys_hex.len());
            for session_key in session_keys_hex {
                session_keys
                    .push(decode_hex_array::<32>(session_key).map_err(|_| {
                        RpcError::InvalidRequest("invalid session_keys_hex".into())
                    })?);
            }
            let mut parsed_constraints = Vec::with_capacity(constraints.len());
            for constraint in constraints {
                parsed_constraints.push(mandate_constraint_from_param(constraint.clone())?);
            }
            let parsed_objectives = objectives
                .iter()
                .cloned()
                .map(mandate_objective_from_param)
                .collect();
            Ok(MandateAction::CreateMandate(CreateMandateInput {
                owner,
                agent: *agent,
                version: *version,
                constraints: parsed_constraints,
                objectives: parsed_objectives,
                objective_weights: objective_weights.clone(),
                conformance_window_ms: *conformance_window_ms,
                session_keys,
                expires_at: *expires_at,
                description_uri: description_uri.clone(),
                description_hash,
            }))
        }
        MandateActionJson::SuspendMandate { mandate_id_hex } => Ok(MandateAction::SuspendMandate {
            mandate_id: decode_hex_array::<32>(mandate_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?,
        }),
        MandateActionJson::ResumeMandate { mandate_id_hex } => Ok(MandateAction::ResumeMandate {
            mandate_id: decode_hex_array::<32>(mandate_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?,
        }),
        MandateActionJson::RevokeMandate {
            mandate_id_hex,
            reason,
        } => Ok(MandateAction::RevokeMandate {
            mandate_id: decode_hex_array::<32>(mandate_id_hex)
                .map_err(|_| RpcError::InvalidRequest("invalid mandate_id_hex".into()))?,
            reason: reason.clone(),
        }),
    }
}

pub(crate) fn order_request_from_json(input: &OrderRequestJson) -> Result<OrderRequest, String> {
    let side = match input.side.as_str() {
        "bid" => OrderSide::Bid,
        "ask" => OrderSide::Ask,
        _ => return Err("invalid order side".into()),
    };
    let kind = match input.kind {
        OrderKindJson::Limit { price_ticks } => OrderKind::Limit { price_ticks },
        OrderKindJson::Market => OrderKind::Market,
    };
    let time_in_force = match input.time_in_force {
        TimeInForceJson::Gtc => TimeInForce::Gtc,
        TimeInForceJson::Ioc => TimeInForce::Ioc,
        TimeInForceJson::Fok => TimeInForce::Fok,
        TimeInForceJson::Gtt { expires_at_ms } => TimeInForce::Gtt { expires_at_ms },
    };

    Ok(OrderRequest {
        market_id: input.market_id,
        account_id: input.account_id,
        client_order_id: input.client_order_id,
        side,
        quantity_lots: input.quantity_lots,
        kind,
        time_in_force,
        post_only: input.post_only,
    })
}

pub(crate) fn intent_from_json(input: &IntentJson) -> Result<Intent, String> {
    Ok(Intent {
        chain_id: input.chain_id,
        account: input.account,
        policy_id: input.policy_id,
        nonce: input.nonce,
        expires_at: input.expires_at,
        predicate: predicate_from_json(input.predicate.clone()),
        kind: intent_kind_from_json(&input.kind)?,
    })
}

pub(crate) fn predicate_from_json(input: Option<IntentPredicateJson>) -> Option<IntentPredicate> {
    input.map(|predicate| match predicate {
        IntentPredicateJson::TimeAfter { not_before_ms } => {
            IntentPredicate::TimeAfter { not_before_ms }
        }
        IntentPredicateJson::TimeBefore { not_after_ms } => {
            IntentPredicate::TimeBefore { not_after_ms }
        }
    })
}

pub(crate) fn intent_kind_from_json(input: &IntentKindJson) -> Result<IntentKind, String> {
    Ok(match input {
        IntentKindJson::Transfer { to, token, amount } => IntentKind::Transfer {
            to: *to,
            token: *token,
            amount: *amount,
        },
        IntentKindJson::Cancel { order_id } => IntentKind::Cancel {
            order_id: *order_id,
        },
        IntentKindJson::PlaceOrder {
            market_id,
            client_order_id,
            side,
            quantity_lots,
            order_kind,
            time_in_force,
            post_only,
        } => IntentKind::PlaceOrder {
            market_id: *market_id,
            client_order_id: *client_order_id,
            side: match side.as_str() {
                "bid" => IntentOrderSide::Bid,
                "ask" => IntentOrderSide::Ask,
                _ => return Err("invalid order side".into()),
            },
            quantity_lots: *quantity_lots,
            kind: match order_kind {
                OrderKindJson::Limit { price_ticks } => IntentOrderKind::Limit {
                    price_ticks: *price_ticks,
                },
                OrderKindJson::Market => IntentOrderKind::Market,
            },
            time_in_force: match time_in_force {
                TimeInForceJson::Gtc => IntentTimeInForce::Gtc,
                TimeInForceJson::Ioc => IntentTimeInForce::Ioc,
                TimeInForceJson::Fok => IntentTimeInForce::Fok,
                TimeInForceJson::Gtt { expires_at_ms } => IntentTimeInForce::Gtt {
                    expires_at_ms: *expires_at_ms,
                },
            },
            post_only: *post_only,
        },
        IntentKindJson::CancelOrder {
            market_id,
            client_order_id,
        } => IntentKind::CancelOrder {
            market_id: *market_id,
            client_order_id: *client_order_id,
        },
        IntentKindJson::ReplaceOrder {
            market_id,
            target_client_order_id,
            replacement_client_order_id,
            side,
            quantity_lots,
            order_kind,
            time_in_force,
            post_only,
        } => IntentKind::ReplaceOrder {
            market_id: *market_id,
            target_client_order_id: *target_client_order_id,
            replacement_client_order_id: *replacement_client_order_id,
            side: match side.as_str() {
                "bid" => IntentOrderSide::Bid,
                "ask" => IntentOrderSide::Ask,
                _ => return Err("invalid order side".into()),
            },
            quantity_lots: *quantity_lots,
            kind: match order_kind {
                OrderKindJson::Limit { price_ticks } => IntentOrderKind::Limit {
                    price_ticks: *price_ticks,
                },
                OrderKindJson::Market => IntentOrderKind::Market,
            },
            time_in_force: match time_in_force {
                TimeInForceJson::Gtc => IntentTimeInForce::Gtc,
                TimeInForceJson::Ioc => IntentTimeInForce::Ioc,
                TimeInForceJson::Fok => IntentTimeInForce::Fok,
                TimeInForceJson::Gtt { expires_at_ms } => IntentTimeInForce::Gtt {
                    expires_at_ms: *expires_at_ms,
                },
            },
            post_only: *post_only,
        },
        IntentKindJson::CreateSubPolicy {
            child_policy_id,
            child_grantee_hex,
            child_expires_at,
            child_terms,
        } => IntentKind::CreateSubPolicy {
            child_policy_id: *child_policy_id,
            child_grantee: decode_hex_array(child_grantee_hex)?,
            child_expires_at: *child_expires_at,
            child_terms: PolicyTermsWire {
                gas_budget_per_day: child_terms.gas_budget_per_day,
                gas_refill_per_day: child_terms.gas_refill_per_day,
                max_intents_per_second: child_terms.max_intents_per_second,
                max_intents_per_day: child_terms.max_intents_per_day,
                max_volume_per_day: child_terms.max_volume_per_day,
                max_drawdown_pct: child_terms.max_drawdown_pct,
            },
        },
        IntentKindJson::RevokePolicy { target_policy_id } => IntentKind::RevokePolicy {
            target_policy_id: *target_policy_id,
        },
        IntentKindJson::DepositCollateral {
            market_id,
            amount_usdc,
        } => IntentKind::DepositCollateral {
            market_id: *market_id,
            amount_usdc: *amount_usdc,
        },
        IntentKindJson::WithdrawCollateral {
            market_id,
            amount_usdc,
        } => IntentKind::WithdrawCollateral {
            market_id: *market_id,
            amount_usdc: *amount_usdc,
        },
    })
}

pub(crate) fn owner_action_from_json(input: &OwnerActionJson) -> Result<OwnerAction, String> {
    Ok(match input {
        OwnerActionJson::ProposeOwner { new_owner_hex } => OwnerAction::ProposeOwner {
            new_owner: decode_hex_array(new_owner_hex)?,
        },
        OwnerActionJson::AcceptOwner => OwnerAction::AcceptOwner,
        OwnerActionJson::Pause => OwnerAction::Pause,
        OwnerActionJson::Unpause => OwnerAction::Unpause,
        OwnerActionJson::CreatePolicy {
            policy_id,
            grantee_hex,
            expires_at,
            terms,
        } => OwnerAction::CreatePolicy {
            policy_id: *policy_id,
            grantee: decode_hex_array(grantee_hex)?,
            expires_at: *expires_at,
            terms: kombat_chain_policy::PolicyTerms {
                gas_budget_per_day: terms.gas_budget_per_day,
                gas_refill_per_day: terms.gas_refill_per_day,
                max_intents_per_second: terms.max_intents_per_second,
                max_intents_per_day: terms.max_intents_per_day,
                max_volume_per_day: terms.max_volume_per_day,
                max_drawdown_pct: terms.max_drawdown_pct,
                ..kombat_chain_policy::PolicyTerms::default()
            },
        },
        OwnerActionJson::AddPolicy { policy_id } => OwnerAction::AddPolicy {
            policy_id: *policy_id,
        },
        OwnerActionJson::RemovePolicy { policy_id } => OwnerAction::RemovePolicy {
            policy_id: *policy_id,
        },
        OwnerActionJson::RevokePolicy { policy_id } => OwnerAction::RevokePolicy {
            policy_id: *policy_id,
        },
        OwnerActionJson::Upgrade { code_hash_hex } => OwnerAction::Upgrade {
            code_hash: decode_hex_array(code_hash_hex)?,
        },
    })
}
