from __future__ import annotations

import unittest
from typing import Any

from and_chain_sdk import AndChainRpcError

from llm_trader.agent import Agent, AgentConfig, AgentState
from llm_trader.openrouter import OpenRouterError


def make_agent(**overrides: object) -> Agent:
    config = AgentConfig(
        rpc_url="http://localhost:8545",
        market_id=7,
        account_id=1,
        policy_id=11,
        chain_id=7,
        session_secret_hex="00" * 32,
        openrouter_api_key="test",
        openrouter_model="test-model",
        **overrides,
    )
    agent = object.__new__(Agent)
    agent.config = config
    agent.state = AgentState()
    return agent


class TestnetTakerCadenceTests(unittest.TestCase):
    def test_disabled_cadence_does_not_submit(self) -> None:
        agent = make_agent(testnet_taker_cadence_enabled=False)
        calls: list[tuple[str, int, dict[str, object]]] = []
        agent._submit_order = lambda side, size, decision, snapshot: calls.append(  # type: ignore[method-assign]
            (side, size, decision)
        ) or True

        submitted = agent._submit_testnet_taker_if_due(  # noqa: SLF001
            {"action": "hold"},
            {"best_bid_ticks": 99, "best_ask_ticks": 101},
        )

        self.assertFalse(submitted)
        self.assertEqual(calls, [])

    def test_enabled_cadence_alternates_ioc_touches(self) -> None:
        agent = make_agent(
            testnet_taker_cadence_enabled=True,
            testnet_taker_size_lots=1,
        )
        calls: list[tuple[str, int, dict[str, object]]] = []

        def submit(side: str, size: int, decision: dict[str, object], _snapshot: object) -> bool:
            calls.append((side, size, decision))
            return True

        agent._submit_order = submit  # type: ignore[method-assign]
        snapshot = {"best_bid_ticks": 99, "best_ask_ticks": 101}

        self.assertTrue(agent._submit_testnet_taker_if_due({"action": "hold"}, snapshot))  # noqa: SLF001
        self.assertTrue(agent._submit_testnet_taker_if_due({"action": "hold"}, snapshot))  # noqa: SLF001

        self.assertEqual(calls[0][0], "bid")
        self.assertEqual(calls[0][2]["limit_price_ticks"], 101)
        self.assertEqual(calls[0][2]["time_in_force"], "ioc")
        self.assertEqual(calls[1][0], "ask")
        self.assertEqual(calls[1][2]["limit_price_ticks"], 99)

    def test_cadence_reduces_position_at_local_limit(self) -> None:
        agent = make_agent(
            testnet_taker_cadence_enabled=True,
            testnet_taker_size_lots=1,
            max_position_lots=2,
        )
        agent.state.position_lots = 2
        calls: list[str] = []
        agent._submit_order = lambda side, size, decision, snapshot: calls.append(side) or True  # type: ignore[method-assign]

        submitted = agent._submit_testnet_taker_if_due(  # noqa: SLF001
            {"action": "hold"},
            {"best_bid_ticks": 99, "best_ask_ticks": 101},
        )

        self.assertTrue(submitted)
        self.assertEqual(calls, ["ask"])

    def test_submit_order_waits_for_pending_policy_nonce(self) -> None:
        agent = make_agent()

        class FakeClient:
            nonce = 7

            def __init__(self) -> None:
                self.submitted: list[dict[str, Any]] = []

            def next_intent_nonce(self, _policy_id: int) -> int:
                return self.nonce

            def submit_typed_intent(
                self,
                _signer: object,
                intent: dict[str, Any],
                _timestamp_ms: int,
            ) -> dict[str, Any]:
                self.submitted.append(intent)
                return {
                    "status": "accepted",
                    "intent_id_hex": f"0x{len(self.submitted):064x}",
                    "pending_reason": "stale_market_data",
                }

            def get_intent_status(self, _intent_id_hex: str) -> object:
                class Status:
                    raw = {
                        "stage": "accepted_direct",
                        "currently_queued": True,
                    }

                return Status()

        client = FakeClient()
        agent.client = client  # type: ignore[assignment]
        agent.signer = object()
        decision = {
            "action": "buy",
            "size_lots": 1,
            "time_in_force": "ioc",
            "limit_price_ticks": 101,
        }

        self.assertTrue(agent._submit_order("bid", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(agent.state.pending_intent_nonce, 7)
        self.assertEqual(len(client.submitted), 1)
        self.assertEqual(agent.state.next_client_order_id, 2)

        self.assertFalse(agent._submit_order("bid", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(len(client.submitted), 1)
        self.assertEqual(agent.state.next_client_order_id, 2)

        client.nonce = 8
        self.assertTrue(agent._submit_order("ask", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(agent.state.pending_intent_nonce, 8)
        self.assertEqual(len(client.submitted), 2)

    def test_submit_order_clears_terminal_pending_intent(self) -> None:
        agent = make_agent()

        class FakeClient:
            def __init__(self) -> None:
                self.submitted: list[dict[str, Any]] = []

            def next_intent_nonce(self, _policy_id: int) -> int:
                return 7

            def get_intent_status(self, _intent_id_hex: str) -> object:
                class Status:
                    raw = {
                        "stage": "rejected",
                        "rejection_reason": "intent_expired",
                        "currently_queued": False,
                    }

                return Status()

            def submit_typed_intent(
                self,
                _signer: object,
                intent: dict[str, Any],
                _timestamp_ms: int,
            ) -> dict[str, Any]:
                self.submitted.append(intent)
                return {
                    "status": "accepted",
                    "intent_id_hex": "0x" + "22" * 32,
                    "pending_reason": None,
                }

        client = FakeClient()
        agent.client = client  # type: ignore[assignment]
        agent.signer = object()
        agent.state.pending_intent_nonce = 7
        agent.state.pending_intent_id_hex = "0x" + "11" * 32
        agent.state.pending_intent_submitted_at_ms = 1
        decision = {
            "action": "buy",
            "size_lots": 1,
            "time_in_force": "ioc",
            "limit_price_ticks": 101,
        }

        self.assertTrue(agent._submit_order("bid", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(len(client.submitted), 1)
        self.assertEqual(agent.state.pending_intent_id_hex, "0x" + "22" * 32)

    def test_submit_order_clears_unknown_stale_pending_intent(self) -> None:
        agent = make_agent(intent_ttl_ms=1)

        class FakeClient:
            def __init__(self) -> None:
                self.submitted: list[dict[str, Any]] = []

            def next_intent_nonce(self, _policy_id: int) -> int:
                return 7

            def get_intent_status(self, _intent_id_hex: str) -> object:
                class Status:
                    raw = {
                        "stage": "unknown",
                        "currently_queued": False,
                    }

                return Status()

            def submit_typed_intent(
                self,
                _signer: object,
                intent: dict[str, Any],
                _timestamp_ms: int,
            ) -> dict[str, Any]:
                self.submitted.append(intent)
                return {
                    "status": "accepted",
                    "intent_id_hex": "0x" + "44" * 32,
                    "pending_reason": None,
                }

        client = FakeClient()
        agent.client = client  # type: ignore[assignment]
        agent.signer = object()
        agent.state.pending_intent_nonce = 7
        agent.state.pending_intent_id_hex = "0x" + "33" * 32
        agent.state.pending_intent_submitted_at_ms = 1
        decision = {
            "action": "buy",
            "size_lots": 1,
            "time_in_force": "ioc",
            "limit_price_ticks": 101,
        }

        self.assertTrue(agent._submit_order("bid", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(len(client.submitted), 1)
        self.assertEqual(agent.state.pending_intent_id_hex, "0x" + "44" * 32)

    def test_submit_order_retains_stale_pending_intent_during_grace(self) -> None:
        agent = make_agent(intent_ttl_ms=1, stale_pending_grace_ms=120_000)

        class FakeClient:
            def __init__(self) -> None:
                self.submitted: list[dict[str, Any]] = []

            def next_intent_nonce(self, _policy_id: int) -> int:
                return 7

            def get_intent_status(self, _intent_id_hex: str) -> object:
                class Status:
                    raw = {
                        "stage": "unknown",
                        "currently_queued": False,
                    }

                return Status()

            def submit_typed_intent(
                self,
                _signer: object,
                intent: dict[str, Any],
                _timestamp_ms: int,
            ) -> dict[str, Any]:
                self.submitted.append(intent)
                return {"intent_id_hex": "0x" + "55" * 32}

        client = FakeClient()
        agent.client = client  # type: ignore[assignment]
        agent.signer = object()
        agent.state.pending_intent_nonce = 7
        agent.state.pending_intent_id_hex = "0x" + "33" * 32
        agent.state.pending_intent_submitted_at_ms = 1
        agent.config.stale_pending_grace_ms = 9_007_199_254_740_991
        decision = {
            "action": "buy",
            "size_lots": 1,
            "time_in_force": "ioc",
            "limit_price_ticks": 101,
        }

        self.assertFalse(agent._submit_order("bid", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(len(client.submitted), 0)
        self.assertEqual(agent.state.pending_intent_id_hex, "0x" + "33" * 32)

    def test_submit_order_preserves_duplicate_nonce_as_pending(self) -> None:
        agent = make_agent()

        class FakeClient:
            def next_intent_nonce(self, _policy_id: int) -> int:
                return 7

            def submit_typed_intent(
                self,
                _signer: object,
                _intent: dict[str, Any],
                _timestamp_ms: int,
            ) -> dict[str, Any]:
                raise AndChainRpcError(
                    -32020,
                    "intent submission rejected",
                    {
                        "reason": "duplicate_policy_nonce",
                        "intent_id_hex": "0x" + "66" * 32,
                    },
                )

        agent.client = FakeClient()  # type: ignore[assignment]
        agent.signer = object()
        decision = {
            "action": "buy",
            "size_lots": 1,
            "time_in_force": "ioc",
            "limit_price_ticks": 101,
        }

        self.assertFalse(agent._submit_order("bid", 1, decision, {}))  # noqa: SLF001
        self.assertEqual(agent.state.pending_intent_nonce, 7)
        self.assertEqual(agent.state.pending_intent_id_hex, "0x" + "66" * 32)

    def test_step_uses_cadence_when_llm_is_unavailable(self) -> None:
        agent = make_agent(
            testnet_taker_cadence_enabled=True,
            testnet_taker_size_lots=1,
        )
        calls: list[tuple[str, int, dict[str, object]]] = []

        agent._build_snapshot = lambda: {"best_bid_ticks": 99, "best_ask_ticks": 101}  # type: ignore[method-assign]

        def fail_llm(_snapshot: object) -> dict[str, object]:
            raise OpenRouterError("rate limited")

        def submit(side: str, size: int, decision: dict[str, object], _snapshot: object) -> bool:
            calls.append((side, size, decision))
            return True

        agent._ask_llm = fail_llm  # type: ignore[method-assign]
        agent._submit_order = submit  # type: ignore[method-assign]

        agent.step()

        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0], "bid")
        self.assertEqual(calls[0][1], 1)
        self.assertEqual(calls[0][2]["reasoning"], "testnet taker cadence after hold decision")


if __name__ == "__main__":
    unittest.main()
