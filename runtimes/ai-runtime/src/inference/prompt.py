"""Context injection pipeline.

Builds system prompts by combining the base prompt with live desktop context.
"""

from src.context.state import DesktopState

BASE_SYSTEM_PROMPT = """You are Ena, an AI assistant built into the operating system.
You have direct access to the user's desktop context — focused windows, workspace, battery, network, media, and clipboard state.
Use this context to provide relevant, helpful responses.
Be concise. Do not mention that you have access to system context unless directly asked.
You are not a chatbot — you are part of the OS."""


def build_system_prompt(desktop_state: DesktopState) -> str:
    """Build the full system prompt with injected desktop context."""
    context_block = desktop_state.to_context_string()
    return f"{BASE_SYSTEM_PROMPT}\n\n{context_block}"


def build_user_message(query: str, desktop_state: DesktopState) -> str:
    """Build the user message, optionally referencing context if relevant."""
    # For now, just return the query. Context is in the system prompt.
    return query
