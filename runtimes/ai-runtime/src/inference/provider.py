"""Inference provider abstraction.

Defines the interface that all LLM providers must implement.
Currently supports Ollama (local-first).
"""

from abc import ABC, abstractmethod
from collections.abc import AsyncIterator


class InferenceProvider(ABC):
    """Abstract interface for LLM inference."""

    @abstractmethod
    async def chat(
        self,
        messages: list[dict],
        system_prompt: str | None = None,
        stream: bool = False,
    ) -> str | AsyncIterator[str]:
        """Send a chat request and return the response.

        If stream=True, yields tokens incrementally.
        If stream=False, returns the full response string.
        """
        ...

    @abstractmethod
    async def health_check(self) -> bool:
        """Check if the inference provider is available."""
        ...
