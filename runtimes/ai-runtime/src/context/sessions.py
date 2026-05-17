"""Session context manager.

Maintains conversation history per session. Each session has its own
message history that gets sent to the LLM alongside desktop context.
"""

import time
import uuid
from dataclasses import dataclass, field


@dataclass
class Session:
    """A single conversation session."""

    id: str = field(default_factory=lambda: str(uuid.uuid4()))
    created_at: float = field(default_factory=time.time)
    messages: list[dict] = field(default_factory=list)
    _max_history: int = 20

    def add_message(self, role: str, content: str) -> None:
        """Add a message to the session history."""
        self.messages.append({
            "role": role,
            "content": content,
            "ts": time.time(),
        })
        if len(self.messages) > self._max_history:
            self.messages = self.messages[-self._max_history:]

    def get_history(self) -> list[dict]:
        """Return message history (without timestamps)."""
        return [{"role": m["role"], "content": m["content"]} for m in self.messages]


class SessionManager:
    """Manages multiple conversation sessions."""

    def __init__(self) -> None:
        self._sessions: dict[str, Session] = {}

    def get_or_create(self, session_id: str | None = None) -> Session:
        """Get an existing session or create a new one."""
        if session_id and session_id in self._sessions:
            return self._sessions[session_id]

        session = Session(id=session_id or str(uuid.uuid4()))
        self._sessions[session.id] = session
        return session

    def get(self, session_id: str) -> Session | None:
        return self._sessions.get(session_id)

    def delete(self, session_id: str) -> None:
        self._sessions.pop(session_id, None)

    def list_sessions(self) -> list[dict]:
        return [
            {
                "id": s.id,
                "created_at": s.created_at,
                "message_count": len(s.messages),
            }
            for s in self._sessions.values()
        ]
