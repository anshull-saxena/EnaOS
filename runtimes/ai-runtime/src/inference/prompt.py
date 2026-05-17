"""Context injection pipeline.

Builds system prompts by combining the base prompt with live desktop context
and working memory.
"""

from src.context.state import DesktopState

BASE_SYSTEM_PROMPT = """You are Ena, an AI assistant built into the operating system.
You have direct access to the user's desktop context — focused windows, workspace, battery, network, media, and clipboard state.
You also have access to working memory — recent actions, intents, and context from this session.
Use this context to provide relevant, helpful responses.
Be concise. Do not mention that you have access to system context unless directly asked.
You are not a chatbot — you are part of the OS."""


def build_system_prompt(desktop_state: DesktopState, memory_context: str = "") -> str:
    """Build the full system prompt with injected desktop context and memory."""
    context_block = desktop_state.to_context_string()

    prompt = f"{BASE_SYSTEM_PROMPT}\n\n{context_block}"

    if memory_context:
        prompt += f"\n\nWorking memory:\n{memory_context}"

    return prompt


def build_user_message(query: str, desktop_state: DesktopState) -> str:
    """Build the user message, optionally referencing context if relevant."""
    return query


def format_memory_context(memory_data: dict) -> str:
    """Format memory data from enad into a readable context block."""
    parts = []

    if recent_intents := memory_data.get("recent_intents", []):
        parts.append("Recent questions:")
        for intent in recent_intents[-3:]:
            parts.append(f"  - {intent}")

    if recent_actions := memory_data.get("recent_actions", []):
        parts.append("Recent actions:")
        for action in recent_actions[-3:]:
            parts.append(f"  - {action}")

    if workspaces := memory_data.get("workspaces", []):
        parts.append(f"Active workspaces: {', '.join(workspaces)}")

    if current := memory_data.get("current_context", {}):
        if app := current.get("focused_app"):
            parts.append(f"Previously focused: {app}")

    if not parts:
        return ""

    return "\n".join(parts)

