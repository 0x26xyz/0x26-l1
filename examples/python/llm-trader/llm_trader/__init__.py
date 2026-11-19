"""LLM-driven directional trading agent for the & chain.

This is an *example* application showing how an autonomous agent can
read the order book, ask any OpenRouter-hosted model for a decision,
and submit signed intents through a session-key policy whose hard
limits are enforced on-chain.

The agent is deliberately **not** a market maker. It takes one-sided
directional positions, manages a single open position with target /
stop logic, and uses IOC takes by default.
"""

from .agent import Agent, AgentConfig
from .openrouter import OpenRouterClient

__all__ = ["Agent", "AgentConfig", "OpenRouterClient"]
