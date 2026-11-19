"""Minimal OpenRouter chat client.

OpenRouter exposes a single OpenAI-compatible endpoint that fronts
hundreds of models. Pick any model id (for example
``anthropic/claude-sonnet-4.5``, ``openai/gpt-5``, ``google/gemini-2.5-pro``,
``meta-llama/llama-3.3-70b-instruct``) and the same code path works.

We use the stdlib HTTP client to keep the example dependency-free.
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from typing import Any
from urllib import error, request


DEFAULT_BASE_URL = "https://openrouter.ai/api/v1"


@dataclass(slots=True)
class ChatMessage:
    role: str
    content: str

    def to_json(self) -> dict[str, str]:
        return {"role": self.role, "content": self.content}


class OpenRouterError(RuntimeError):
    pass


class OpenRouterClient:
    def __init__(
        self,
        api_key: str,
        *,
        model: str,
        base_url: str = DEFAULT_BASE_URL,
        referer: str = "https://0x26.xyz/examples/llm-trader",
        title: str = "and-llm-trader",
        timeout_s: float = 30.0,
        max_retries: int = 3,
    ) -> None:
        if not api_key:
            raise ValueError("OPENROUTER_API_KEY is required")
        self._api_key = api_key
        self._model = model
        self._base_url = base_url.rstrip("/")
        self._referer = referer
        self._title = title
        self._timeout_s = timeout_s
        self._max_retries = max_retries

    @property
    def model(self) -> str:
        return self._model

    def chat_json(
        self,
        messages: list[ChatMessage],
        *,
        temperature: float = 0.2,
        max_tokens: int = 512,
    ) -> dict[str, Any]:
        """Run one chat completion and parse the assistant reply as JSON.

        Falls back to extracting the first balanced ``{...}`` block if the
        model wraps its JSON in prose. Raises ``OpenRouterError`` on any
        non-recoverable failure.
        """
        body = {
            "model": self._model,
            "messages": [m.to_json() for m in messages],
            "temperature": temperature,
            "max_tokens": max_tokens,
            "response_format": {"type": "json_object"},
        }
        raw = self._post("/chat/completions", body)
        try:
            choice = raw["choices"][0]["message"]["content"]
        except (KeyError, IndexError, TypeError) as exc:
            raise OpenRouterError(f"unexpected response shape: {raw!r}") from exc
        return _parse_json_object(choice)

    def _post(self, path: str, body: dict[str, Any]) -> dict[str, Any]:
        payload = json.dumps(body).encode("utf-8")
        headers = {
            "authorization": f"Bearer {self._api_key}",
            "content-type": "application/json",
            "http-referer": self._referer,
            "x-title": self._title,
        }
        url = f"{self._base_url}{path}"
        backoff_s = 1.0
        last_err: Exception | None = None
        for attempt in range(self._max_retries):
            try:
                req = request.Request(url, data=payload, headers=headers, method="POST")
                with request.urlopen(req, timeout=self._timeout_s) as resp:
                    return json.loads(resp.read().decode("utf-8"))
            except error.HTTPError as exc:
                if exc.code in (429, 500, 502, 503, 504) and attempt < self._max_retries - 1:
                    last_err = exc
                    time.sleep(backoff_s)
                    backoff_s *= 2
                    continue
                detail = exc.read().decode("utf-8", errors="replace")
                raise OpenRouterError(f"openrouter HTTP {exc.code}: {detail}") from exc
            except (error.URLError, TimeoutError) as exc:
                last_err = exc
                if attempt < self._max_retries - 1:
                    time.sleep(backoff_s)
                    backoff_s *= 2
                    continue
                raise OpenRouterError(f"openrouter transport error: {exc}") from exc
        raise OpenRouterError(f"openrouter retries exhausted: {last_err}")


def _parse_json_object(text: str) -> dict[str, Any]:
    text = text.strip()
    try:
        value = json.loads(text)
    except json.JSONDecodeError:
        start = text.find("{")
        end = text.rfind("}")
        if start == -1 or end == -1 or end <= start:
            raise OpenRouterError(f"model reply was not JSON: {text!r}")
        value = json.loads(text[start : end + 1])
    if not isinstance(value, dict):
        raise OpenRouterError(f"model reply was JSON but not an object: {value!r}")
    return value
