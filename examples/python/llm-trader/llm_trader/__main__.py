"""CLI: ``and-llm-trader setup`` and ``and-llm-trader run``."""

from __future__ import annotations

import argparse
import logging
import os
import secrets
import sys
import time

from .agent import Agent, AgentConfig
from .bootstrap import bootstrap


def _required_env(name: str) -> str:
    value = os.environ.get(name)
    if not value:
        sys.exit(f"missing required env: {name}")
    return value


def _env_int(name: str, default: int) -> int:
    raw = os.environ.get(name)
    return int(raw) if raw is not None else default


def _env_float(name: str, default: float) -> float:
    raw = os.environ.get(name)
    return float(raw) if raw is not None else default


def _env_bool(name: str, default: bool) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    return raw.lower() in ("1", "true", "yes", "on")


def _cmd_keygen(_args: argparse.Namespace) -> int:
    secret = secrets.token_bytes(32)
    print(secret.hex())
    return 0


def _cmd_setup(args: argparse.Namespace) -> int:
    bootstrap(
        rpc_url=_required_env("AND_RPC_URL"),
        owner_secret_hex=_required_env("AND_OWNER_SECRET_HEX"),
        session_secret_hex=_required_env("AND_SESSION_SECRET_HEX"),
        account_id=_env_int("AND_ACCOUNT_ID", 1),
        policy_id=_env_int("AND_POLICY_ID", 1),
        code_hash_hex=os.environ.get(
            "AND_CODE_HASH_HEX",
            "0x" + "00" * 32,
        ),
        max_volume_per_day_usdc=_env_int("AND_MAX_VOLUME_PER_DAY_USDC", 5_000),
        max_drawdown_pct=_env_int("AND_MAX_DRAWDOWN_BPS", 500),
        max_intents_per_second=_env_int("AND_MAX_INTENTS_PER_SECOND", 1),
        max_intents_per_day=_env_int("AND_MAX_INTENTS_PER_DAY", 1_000),
        gas_budget_per_day_microusdc=_env_int(
            "AND_GAS_BUDGET_PER_DAY_MICROUSDC", 10_000_000
        ),
        gas_refill_per_day_microusdc=_env_int(
            "AND_GAS_REFILL_PER_DAY_MICROUSDC", 10_000_000
        ),
        expires_in_ms=_env_int("AND_POLICY_TTL_MS", 7 * 24 * 60 * 60 * 1000),
        evm_address_hex=os.environ.get("AND_EVM_ADDRESS_HEX") or None,
        bearer_token=os.environ.get("AND_BEARER_TOKEN"),
    )
    return 0


def _cmd_fund(_args: argparse.Namespace) -> int:
    """Submit a session-signed deposit_collateral intent.

    Idempotent: skips if the account already has non-zero isolated collateral
    on the configured market. Drains AND_DEPOSIT_USDC (default 2_000) USDC
    from the policy USDC balance into the market's isolated margin account.
    """
    from and_chain_sdk import AndChainClient, Keypair

    rpc_url = _required_env("AND_RPC_URL")
    bearer = os.environ.get("AND_BEARER_TOKEN")
    market_id = _env_int("AND_MARKET_ID", 1)
    account_id = _env_int("AND_ACCOUNT_ID", 1)
    policy_id = _env_int("AND_POLICY_ID", 1)
    deposit_usdc = _env_int("AND_DEPOSIT_USDC", 2_000)
    deposit_micro = deposit_usdc * 1_000_000
    intent_ttl_ms = _env_int("AND_INTENT_TTL_MS", 30_000)

    client = AndChainClient(rpc_url, bearer_token=bearer)
    session = Keypair.from_secret_hex(_required_env("AND_SESSION_SECRET_HEX"))

    risk = client._call(  # type: ignore[attr-defined]
        "kombat_getAccountRiskSummary",
        {"account_id": account_id, "market_id": market_id},
    )
    existing_collateral = int(risk.get("total_collateral", "0"))
    if existing_collateral > 0:
        logging.getLogger("llm_trader.fund").info(
            "account %d already has %s collateral on market %d — skipping",
            account_id,
            existing_collateral,
            market_id,
        )
        return 0

    chain_id = (
        _env_int("AND_CHAIN_ID", 0)
        or _resolve_chain_id(rpc_url, bearer)
    )
    intent = {
        "chain_id": chain_id,
        "account_id": account_id,
        "policy_id": policy_id,
        "nonce": client.next_intent_nonce(policy_id),
        "expires_at": int(time.time() * 1000) + intent_ttl_ms,
        "predicate": None,
        "kind": {
            "kind": "deposit_collateral",
            "market_id": market_id,
            "amount_usdc": deposit_micro,
        },
    }
    from and_chain_sdk.client import AndChainRpcError

    try:
        result = client.submit_typed_intent(
            session, intent, int(time.time() * 1000)
        )
    except AndChainRpcError as exc:
        logging.getLogger("llm_trader.fund").error(
            "deposit rejected: code=%s message=%r data=%r",
            exc.code,
            str(exc),
            exc.data,
        )
        raise
    logging.getLogger("llm_trader.fund").info(
        "deposited %d USDC (%d micro) to market %d: %s",
        deposit_usdc,
        deposit_micro,
        market_id,
        result,
    )
    return 0


def _cmd_run(args: argparse.Namespace) -> int:
    rpc_url = _required_env("AND_RPC_URL")
    config = AgentConfig(
        rpc_url=rpc_url,
        market_id=_env_int("AND_MARKET_ID", 1),
        account_id=_env_int("AND_ACCOUNT_ID", 1),
        policy_id=_env_int("AND_POLICY_ID", 1),
        chain_id=_env_int("AND_CHAIN_ID", 0)
        or _resolve_chain_id(rpc_url, os.environ.get("AND_BEARER_TOKEN")),
        session_secret_hex=_required_env("AND_SESSION_SECRET_HEX"),
        openrouter_api_key=_required_env("OPENROUTER_API_KEY"),
        openrouter_model=os.environ.get(
            "OPENROUTER_MODEL", "anthropic/claude-sonnet-4.5"
        ),
        poll_interval_s=_env_float("AND_POLL_INTERVAL_S", 10.0),
        book_depth=_env_int("AND_BOOK_DEPTH", 5),
        recent_event_window=_env_int("AND_EVENT_WINDOW", 50),
        max_trade_lots=_env_int("AND_MAX_TRADE_LOTS", 5),
        max_position_lots=_env_int("AND_MAX_POSITION_LOTS", 20),
        intent_ttl_ms=_env_int("AND_INTENT_TTL_MS", 30_000),
        stale_pending_grace_ms=_env_int("AND_STALE_PENDING_GRACE_MS", 120_000),
        dry_run=args.dry_run,
        bearer_token=os.environ.get("AND_BEARER_TOKEN"),
        testnet_taker_cadence_enabled=_env_bool(
            "AND_TESTNET_TAKER_CADENCE_ENABLED", False
        ),
        testnet_taker_size_lots=_env_int("AND_TESTNET_TAKER_SIZE_LOTS", 1),
        testnet_taker_every_n_holds=_env_int("AND_TESTNET_TAKER_EVERY_N_HOLDS", 1),
    )
    Agent(config).run_forever()
    return 0


def _resolve_chain_id(rpc_url: str, bearer: str | None) -> int:
    from and_chain_sdk import AndChainClient

    return AndChainClient(rpc_url, bearer_token=bearer).chain_info().chain_id


def main(argv: list[str] | None = None) -> int:
    logging.basicConfig(
        level=os.environ.get("AND_LOG_LEVEL", "INFO"),
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )

    parser = argparse.ArgumentParser(
        prog="and-llm-trader",
        description="LLM-driven directional trading agent on the & chain (example)",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("keygen", help="print a fresh 32-byte ed25519 secret in hex")

    sub.add_parser(
        "setup",
        help="create the account and mint the session-key policy (one shot)",
    )

    sub.add_parser(
        "fund",
        help="submit a deposit_collateral intent to seed market margin (idempotent)",
    )

    p_run = sub.add_parser("run", help="run the agent loop")
    p_run.add_argument(
        "--dry-run",
        action="store_true",
        help="log decisions but do not submit signed intents",
    )

    args = parser.parse_args(argv)

    handlers = {
        "keygen": _cmd_keygen,
        "setup": _cmd_setup,
        "fund": _cmd_fund,
        "run": _cmd_run,
    }
    return handlers[args.cmd](args)


if __name__ == "__main__":
    raise SystemExit(main())
