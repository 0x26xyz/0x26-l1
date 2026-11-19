"""Observe → reason → act loop for the LLM trading agent."""

from __future__ import annotations

import json
import logging
import time
from collections import deque
from dataclasses import dataclass, field
from typing import Any

from and_chain_sdk import (
    AndChainClient,
    AndChainRpcError,
    EventFilter,
    Keypair,
    OrderBookResponse,
)

from .openrouter import ChatMessage, OpenRouterClient, OpenRouterError


log = logging.getLogger("llm_trader.agent")


SYSTEM_PROMPT = """You are an autonomous trading agent operating one market on the & chain.

You are NOT a market maker. You take directional positions: at most one open
position at a time, sized within the limits the harness gives you. You may also
choose to do nothing.

Each turn you receive a JSON market snapshot. Reply with a single JSON object
of the form:

{
  "action": "buy" | "sell" | "close" | "hold",
  "size_lots": <integer, 0 if action is hold>,
  "time_in_force": "ioc" | "gtc",
  "limit_price_ticks": <integer or null; null means market/IOC at touch>,
  "reasoning": "<one short sentence>"
}

Rules:
- "buy" opens or adds to a long. "sell" opens or adds to a short.
- "close" flattens any existing position; size_lots is ignored.
- "hold" does nothing this turn.
- Never request a size larger than max_trade_lots in the snapshot.
- Never go beyond max_position_lots (long or short).
- Prefer "ioc" with limit_price_ticks set defensively; use "gtc" only if you
  want to rest passively for a tick or two.
- Output JSON only. No prose, no markdown fences.
"""


@dataclass
class AgentConfig:
    rpc_url: str
    market_id: int
    account_id: int
    policy_id: int
    chain_id: int
    session_secret_hex: str
    openrouter_api_key: str
    openrouter_model: str
    poll_interval_s: float = 10.0
    book_depth: int = 5
    recent_event_window: int = 50
    max_trade_lots: int = 5
    max_position_lots: int = 20
    intent_ttl_ms: int = 30_000
    stale_pending_grace_ms: int = 120_000
    dry_run: bool = False
    bearer_token: str | None = None
    testnet_taker_cadence_enabled: bool = False
    testnet_taker_size_lots: int = 1
    testnet_taker_every_n_holds: int = 1


@dataclass
class AgentState:
    next_client_order_id: int = 1
    position_lots: int = 0  # signed: + long, - short
    last_event_seq: int = 0
    hold_count: int = 0
    pending_intent_nonce: int | None = None
    pending_intent_id_hex: str | None = None
    pending_intent_submitted_at_ms: int | None = None
    next_testnet_taker_side: str = "buy"
    history: deque[dict[str, Any]] = field(default_factory=lambda: deque(maxlen=10))


def now_ms() -> int:
    return int(time.time() * 1000)


class Agent:
    def __init__(self, config: AgentConfig) -> None:
        self.config = config
        self.client = AndChainClient(
            config.rpc_url, bearer_token=config.bearer_token
        )
        self.signer = Keypair.from_secret_hex(config.session_secret_hex)
        self.llm = OpenRouterClient(
            config.openrouter_api_key, model=config.openrouter_model
        )
        self.state = AgentState()

    def run_forever(self) -> None:
        log.info(
            "agent starting market=%d account=%d policy=%d model=%s dry_run=%s",
            self.config.market_id,
            self.config.account_id,
            self.config.policy_id,
            self.config.openrouter_model,
            self.config.dry_run,
        )
        while True:
            try:
                self.step()
            except KeyboardInterrupt:
                log.info("interrupt — stopping")
                return
            except Exception:  # noqa: BLE001 — keep the loop alive in demos
                log.exception("step failed")
            time.sleep(self.config.poll_interval_s)

    def step(self) -> None:
        snapshot = self._build_snapshot()
        try:
            decision = self._ask_llm(snapshot)
        except OpenRouterError as exc:
            if not self.config.testnet_taker_cadence_enabled:
                raise
            log.warning(
                "llm unavailable; using testnet cadence fallback: %s",
                exc,
            )
            decision = {
                "action": "hold",
                "size_lots": 0,
                "time_in_force": "ioc",
                "limit_price_ticks": None,
                "reasoning": "llm unavailable; testnet cadence fallback",
            }
        self._apply(decision, snapshot)

    def _build_snapshot(self) -> dict[str, Any]:
        book = self.client.get_order_book(
            self.config.market_id, self.config.book_depth
        )
        best_bid = book.bids[0].price_ticks if book.bids else None
        best_ask = book.asks[0].price_ticks if book.asks else None
        mid = (
            (best_bid + best_ask) / 2 if best_bid is not None and best_ask is not None else None
        )

        try:
            risk = self.client.get_account_risk_summary(
                self.config.account_id, market_id=self.config.market_id
            ).raw
        except Exception as exc:  # noqa: BLE001
            log.debug("risk summary unavailable: %s", exc)
            risk = {}

        recent = self._recent_fills(book)
        snapshot = {
            "now_ms": now_ms(),
            "market_id": self.config.market_id,
            "book": _book_to_json(book),
            "best_bid_ticks": best_bid,
            "best_ask_ticks": best_ask,
            "mid_ticks": mid,
            "recent_fills": recent,
            "position": {
                "lots": self.state.position_lots,
                "max_lots": self.config.max_position_lots,
            },
            "max_trade_lots": self.config.max_trade_lots,
            "risk_summary": risk,
            "recent_decisions": list(self.state.history),
        }
        return snapshot

    def _recent_fills(self, _book: OrderBookResponse) -> list[dict[str, Any]]:
        try:
            page = self.client.list_events(
                from_seq=self.state.last_event_seq,
                limit=self.config.recent_event_window,
                filter=EventFilter(
                    market_id=self.config.market_id, topic="match_filled"
                ),
            )
        except Exception as exc:  # noqa: BLE001
            log.debug("event fetch failed: %s", exc)
            return []
        events = page.events
        if events:
            self.state.last_event_seq = events[-1].seq + 1
        return [
            {
                "seq": e.seq,
                "ts_ms": e.timestamp_ms,
                "kind": e.kind,
            }
            for e in events
        ]

    def _ask_llm(self, snapshot: dict[str, Any]) -> dict[str, Any]:
        user = (
            "Market snapshot:\n```json\n"
            + json.dumps(snapshot, default=str, indent=2)
            + "\n```\nReturn the JSON decision now."
        )
        decision = self.llm.chat_json(
            [
                ChatMessage("system", SYSTEM_PROMPT),
                ChatMessage("user", user),
            ]
        )
        log.info(
            "llm decision: %s size=%s tif=%s reason=%s",
            decision.get("action"),
            decision.get("size_lots"),
            decision.get("time_in_force"),
            decision.get("reasoning"),
        )
        return decision

    def _apply(self, decision: dict[str, Any], snapshot: dict[str, Any]) -> None:
        action = str(decision.get("action", "hold")).lower()
        if action == "hold":
            if self._submit_testnet_taker_if_due(decision, snapshot):
                return
            self._record(decision, "hold")
            return
        if action == "close":
            self._submit_close(decision, snapshot)
            return
        if action not in ("buy", "sell"):
            log.warning("ignoring unknown action %r", action)
            self._record(decision, "ignored:unknown_action")
            return
        size = self._clamp_size(decision, action)
        if size <= 0:
            log.info("clamped size to zero — skipping")
            self._record(decision, "skipped:zero_size")
            return
        side = "bid" if action == "buy" else "ask"
        self._submit_order(side, size, decision, snapshot)

    def _submit_testnet_taker_if_due(
        self, decision: dict[str, Any], snapshot: dict[str, Any]
    ) -> bool:
        if not self.config.testnet_taker_cadence_enabled:
            return False

        self.state.hold_count += 1
        every = max(1, self.config.testnet_taker_every_n_holds)
        if self.state.hold_count % every != 0:
            return False

        side = self._next_testnet_taker_side()
        limit_ticks = (
            snapshot.get("best_ask_ticks")
            if side == "bid"
            else snapshot.get("best_bid_ticks")
        )
        if limit_ticks is None:
            log.info(
                "testnet taker cadence skipped: no %s touch",
                "ask" if side == "bid" else "bid",
            )
            return False

        size = max(
            1, min(self.config.testnet_taker_size_lots, self.config.max_trade_lots)
        )
        synthetic_decision = {
            **decision,
            "action": "buy" if side == "bid" else "sell",
            "size_lots": size,
            "time_in_force": "ioc",
            "limit_price_ticks": int(limit_ticks),
            "reasoning": "testnet taker cadence after hold decision",
        }
        submitted = self._submit_order(side, size, synthetic_decision, snapshot)
        if submitted:
            self.state.next_testnet_taker_side = "sell" if side == "bid" else "buy"
        return submitted

    def _next_testnet_taker_side(self) -> str:
        max_position = self.config.max_position_lots
        if self.state.position_lots >= max_position:
            return "ask"
        if self.state.position_lots <= -max_position:
            return "bid"
        return "bid" if self.state.next_testnet_taker_side == "buy" else "ask"

    def _clamp_size(self, decision: dict[str, Any], action: str) -> int:
        try:
            requested = int(decision.get("size_lots", 0))
        except (TypeError, ValueError):
            requested = 0
        requested = max(0, min(requested, self.config.max_trade_lots))
        signed_after = self.state.position_lots + (
            requested if action == "buy" else -requested
        )
        if abs(signed_after) > self.config.max_position_lots:
            headroom = self.config.max_position_lots - (
                self.state.position_lots if action == "buy" else -self.state.position_lots
            )
            requested = max(0, headroom)
        return requested

    def _submit_order(
        self,
        side: str,
        size_lots: int,
        decision: dict[str, Any],
        _snapshot: dict[str, Any],
    ) -> bool:
        tif = "gtc" if str(decision.get("time_in_force", "ioc")).lower() == "gtc" else "ioc"
        limit_ticks = decision.get("limit_price_ticks")
        if limit_ticks is None:
            order_kind = {"kind": "market"}
        else:
            order_kind = {"kind": "limit", "price_ticks": int(limit_ticks)}

        nonce = self.client.next_intent_nonce(self.config.policy_id)
        if (
            self.state.pending_intent_nonce is not None
            and nonce <= self.state.pending_intent_nonce
            and self._pending_intent_still_blocks(nonce)
        ):
            log.info(
                "waiting for pending intent nonce=%d intent_id=%s current_policy_nonce=%d",
                self.state.pending_intent_nonce,
                self.state.pending_intent_id_hex,
                nonce,
            )
            self._record(decision, "skipped:pending_intent")
            return False
        self._clear_pending_intent()
        client_order_id = self.state.next_client_order_id
        self.state.next_client_order_id += 1

        intent = {
            "chain_id": self.config.chain_id,
            "account_id": self.config.account_id,
            "policy_id": self.config.policy_id,
            "nonce": nonce,
            "expires_at": now_ms() + self.config.intent_ttl_ms,
            "predicate": None,
            "kind": {
                "kind": "place_order",
                "market_id": self.config.market_id,
                "client_order_id": client_order_id,
                "side": side,
                "quantity_lots": size_lots,
                "order_kind": order_kind,
                "time_in_force": {"kind": tif},
                "post_only": False,
            },
        }

        if self.config.dry_run:
            log.info("DRY RUN intent=%s", json.dumps(intent["kind"]))
            self._record(decision, "dry_run")
            return True

        try:
            result = self.client.submit_typed_intent(self.signer, intent, now_ms())
        except AndChainRpcError as exc:
            if self._preserve_duplicate_nonce_as_pending(exc, nonce):
                self._record(decision, "skipped:duplicate_policy_nonce")
                return False
            log.exception(
                "submit_typed_intent failed: code=%s message=%s data=%s",
                exc.code,
                str(exc),
                exc.data,
            )
            self._record(decision, f"error:rpc:{exc.code}")
            return False
        except Exception as exc:  # noqa: BLE001
            log.exception("submit_typed_intent failed: %s", exc)
            self._record(decision, f"error:{type(exc).__name__}")
            return False

        self.state.pending_intent_nonce = nonce
        self.state.pending_intent_id_hex = (
            str(result.get("intent_id_hex"))
            if isinstance(result, dict) and result.get("intent_id_hex")
            else None
        )
        self.state.pending_intent_submitted_at_ms = now_ms()
        log.info(
            "submitted %s %d lots tif=%s nonce=%d intent_id=%s pending_reason=%s (%s)",
            side,
            size_lots,
            tif,
            nonce,
            self.state.pending_intent_id_hex,
            result.get("pending_reason") if isinstance(result, dict) else None,
            result,
        )
        self._record(decision, "submitted")
        return True

    def _pending_intent_still_blocks(self, current_policy_nonce: int) -> bool:
        pending_nonce = self.state.pending_intent_nonce
        if pending_nonce is None:
            return False
        if current_policy_nonce > pending_nonce:
            return False

        intent_id_hex = self.state.pending_intent_id_hex
        age_ms = self._pending_intent_age_ms()
        if intent_id_hex is None:
            return age_ms <= self.config.intent_ttl_ms

        try:
            status = self.client.get_intent_status(intent_id_hex).raw
        except Exception as exc:  # noqa: BLE001
            log.debug("pending intent status unavailable intent_id=%s: %s", intent_id_hex, exc)
            return age_ms <= self.config.intent_ttl_ms

        stage = str(status.get("stage", "unknown"))
        currently_queued = bool(status.get("currently_queued"))
        if stage in {"executed", "expired", "rejected"}:
            log.info(
                "clearing terminal pending intent nonce=%d intent_id=%s stage=%s rejection=%s order_rejection=%s",
                pending_nonce,
                intent_id_hex,
                stage,
                status.get("rejection_reason"),
                status.get("order_rejection_reason"),
            )
            return False

        # Admission can be acknowledged by a node that later does not carry the
        # intent through block replay. Do not let the demo taker wedge forever.
        if (
            age_ms > self.config.intent_ttl_ms
            and (stage == "unknown" or not currently_queued)
        ):
            if age_ms <= self.config.stale_pending_grace_ms:
                log.warning(
                    "retaining stale pending intent nonce=%d intent_id=%s stage=%s queued=%s age_ms=%d grace_ms=%d",
                    pending_nonce,
                    intent_id_hex,
                    stage,
                    currently_queued,
                    age_ms,
                    self.config.stale_pending_grace_ms,
                )
                return True
            log.warning(
                "clearing stale pending intent nonce=%d intent_id=%s stage=%s queued=%s age_ms=%d",
                pending_nonce,
                intent_id_hex,
                stage,
                currently_queued,
                age_ms,
            )
            return False

        return True

    def _pending_intent_age_ms(self) -> int:
        submitted_at = self.state.pending_intent_submitted_at_ms
        if submitted_at is None:
            return 0
        return max(0, now_ms() - submitted_at)

    def _clear_pending_intent(self) -> None:
        self.state.pending_intent_nonce = None
        self.state.pending_intent_id_hex = None
        self.state.pending_intent_submitted_at_ms = None

    def _preserve_duplicate_nonce_as_pending(
        self, exc: AndChainRpcError, nonce: int
    ) -> bool:
        data = exc.data if isinstance(exc.data, dict) else {}
        if data.get("reason") != "duplicate_policy_nonce":
            return False
        intent_id_hex = data.get("intent_id_hex")
        self.state.pending_intent_nonce = nonce
        self.state.pending_intent_id_hex = (
            str(intent_id_hex) if isinstance(intent_id_hex, str) else None
        )
        self.state.pending_intent_submitted_at_ms = now_ms()
        log.warning(
            "preserving duplicate policy nonce as pending nonce=%d intent_id=%s",
            nonce,
            self.state.pending_intent_id_hex,
        )
        return True

    def _submit_close(
        self, decision: dict[str, Any], snapshot: dict[str, Any]
    ) -> None:
        if self.state.position_lots == 0:
            log.info("close requested but flat — skipping")
            self._record(decision, "skipped:flat")
            return
        side = "ask" if self.state.position_lots > 0 else "bid"
        size = abs(self.state.position_lots)
        flatten_decision = {
            **decision,
            "size_lots": size,
            "time_in_force": "ioc",
            "limit_price_ticks": None,
        }
        self._submit_order(side, size, flatten_decision, snapshot)

    def _record(self, decision: dict[str, Any], outcome: str) -> None:
        self.state.history.append(
            {
                "ts_ms": now_ms(),
                "action": decision.get("action"),
                "size_lots": decision.get("size_lots"),
                "tif": decision.get("time_in_force"),
                "limit_price_ticks": decision.get("limit_price_ticks"),
                "reasoning": decision.get("reasoning"),
                "outcome": outcome,
                "position_after": self.state.position_lots,
            }
        )


def _book_to_json(book: OrderBookResponse) -> dict[str, Any]:
    return {
        "bids": [
            {"price_ticks": lvl.price_ticks, "lots": lvl.total_lots, "orders": lvl.order_count}
            for lvl in book.bids
        ],
        "asks": [
            {"price_ticks": lvl.price_ticks, "lots": lvl.total_lots, "orders": lvl.order_count}
            for lvl in book.asks
        ],
    }
