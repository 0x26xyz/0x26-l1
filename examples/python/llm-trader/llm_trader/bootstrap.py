"""One-shot bootstrap: create an account and a session-key policy.

Run this once per agent. The owner key authorises the account; the
session key the agent uses for every trading intent is bound to a
mandate (max volume / drawdown / intent rate) the chain enforces.
This is the on-chain safety floor — even a misbehaving model can't
exceed it.
"""

from __future__ import annotations

import logging
import time

from and_chain_sdk import (
    AndChainClient,
    Keypair,
    PolicyTermsBuilder,
)


log = logging.getLogger("llm_trader.bootstrap")


def now_ms() -> int:
    return int(time.time() * 1000)


def bootstrap(
    rpc_url: str,
    *,
    owner_secret_hex: str,
    session_secret_hex: str,
    account_id: int,
    policy_id: int,
    code_hash_hex: str,
    max_volume_per_day_usdc: int,
    max_drawdown_pct: int,
    max_intents_per_second: int,
    max_intents_per_day: int,
    gas_budget_per_day_microusdc: int,
    gas_refill_per_day_microusdc: int,
    expires_in_ms: int,
    evm_address_hex: str | None = None,
    bearer_token: str | None = None,
) -> None:
    client = AndChainClient(rpc_url, bearer_token=bearer_token)
    owner = Keypair.from_secret_hex(owner_secret_hex)
    session = Keypair.from_secret_hex(session_secret_hex)

    chain_id = client.chain_info().chain_id

    try:
        account = client.get_account(account_id)
        log.info("account %d already exists (owner=%s)", account_id, account.owner_hex)
    except Exception:  # noqa: BLE001 — first run, expected
        log.info(
            "creating account %d (evm=%s)",
            account_id,
            evm_address_hex or "<unlinked>",
        )
        client.create_account(
            account_id=account_id,
            owner_hex=owner.public_hex(),
            code_hash_hex=code_hash_hex,
            timestamp_ms=now_ms(),
            evm_address_hex=evm_address_hex,
        )
        account = client.get_account(account_id)

    terms = (
        PolicyTermsBuilder()
        .max_volume_per_day_usdc(max_volume_per_day_usdc)
        .max_drawdown_pct(max_drawdown_pct)
        .max_intents_per_second(max_intents_per_second)
        .max_intents_per_day(max_intents_per_day)
        .gas_budget_per_day_microusdc(gas_budget_per_day_microusdc)
        .gas_refill_per_day_microusdc(gas_refill_per_day_microusdc)
        .build()
    )

    try:
        existing = client.get_policy(policy_id)
        log.info(
            "policy %d already exists (state=%s, account=%d) — skipping create",
            policy_id,
            existing.state,
            existing.account,
        )
        return
    except Exception:  # noqa: BLE001 — not yet minted
        pass

    log.info("minting policy %d for account %d", policy_id, account_id)
    client.submit_typed_owner_call(
        owner,
        {
            "account_id": account_id,
            "chain_id": chain_id,
            "nonce": account.nonce,
            "action": {
                "kind": "create_policy",
                "policy_id": policy_id,
                "grantee_hex": session.public_hex(),
                "expires_at": now_ms() + expires_in_ms,
                "terms": terms,
            },
        },
        now_ms(),
    )
    client.wait_for_policy(policy_id, expected_account=account_id)
    log.info(
        "policy %d minted: max_volume_per_day=%d USDC, max_drawdown=%d bps, "
        "max_intents_per_sec=%d",
        policy_id,
        max_volume_per_day_usdc,
        max_drawdown_pct,
        max_intents_per_second,
    )
