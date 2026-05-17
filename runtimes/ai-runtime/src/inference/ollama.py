"""Ollama inference provider.

Connects to a local Ollama instance for streaming and non-streaming chat.
Supports any model available in the Ollama library.
"""

import json
from collections.abc import AsyncIterator

import httpx

from src.config import settings
from src.inference.provider import InferenceProvider

SYSTEM_PROMPT = """You are Ena, an AI assistant built into the operating system.
You have direct access to the user's desktop context — focused windows, workspace, battery, network, media, and clipboard state.
Use this context to provide relevant, helpful responses.
Be concise. Do not mention that you have access to system context unless directly asked.
You are not a chatbot — you are part of the OS."""


class OllamaProvider(InferenceProvider):
    """Ollama-based inference with streaming support."""

    def __init__(
        self,
        base_url: str | None = None,
        model: str | None = None,
    ) -> None:
        self.base_url = (base_url or settings.ollama_url).rstrip("/")
        self.model = model or settings.ollama_model
        self._client = httpx.AsyncClient(timeout=120.0)

    async def chat(
        self,
        messages: list[dict],
        system_prompt: str | None = None,
        stream: bool = False,
    ) -> str | AsyncIterator[str]:
        """Chat with the LLM. Returns full response or streams tokens."""
        prompt = system_prompt or SYSTEM_PROMPT

        # Build the messages array with system prompt first.
        ollama_messages = [{"role": "system", "content": prompt}]
        for msg in messages:
            ollama_messages.append({"role": msg["role"], "content": msg["content"]})

        payload = {
            "model": self.model,
            "messages": ollama_messages,
            "stream": stream,
            "options": {
                "temperature": 0.7,
                "num_ctx": 4096,
            },
        }

        if stream:
            return self._stream_response(payload)
        else:
            return await self._get_response(payload)

    async def _get_response(self, payload: dict) -> str:
        """Get a non-streaming response."""
        resp = await self._client.post(
            f"{self.base_url}/api/chat",
            json=payload,
        )
        resp.raise_for_status()
        data = resp.json()
        return data.get("message", {}).get("content", "")

    async def _stream_response(
        self, payload: dict
    ) -> AsyncIterator[str]:
        """Stream tokens from the LLM incrementally."""
        async with self._client.stream(
            "POST",
            f"{self.base_url}/api/chat",
            json=payload,
        ) as resp:
            resp.raise_for_status()
            async for line in resp.aiter_lines():
                if not line:
                    continue
                try:
                    data = json.loads(line)
                    content = data.get("message", {}).get("content", "")
                    if content:
                        yield content
                except json.JSONDecodeError:
                    continue

    async def health_check(self) -> bool:
        """Check if Ollama is running and the model is available."""
        try:
            resp = await self._client.get(f"{self.base_url}/api/tags")
            if resp.status_code == 200:
                models = resp.json().get("models", [])
                return any(self.model in m.get("name", "") for m in models)
        except Exception:
            pass
        return False
