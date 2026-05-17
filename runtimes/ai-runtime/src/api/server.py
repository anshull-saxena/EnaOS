"""FastAPI application — AI runtime HTTP interface.

Endpoints:
    POST /chat          — Non-streaming chat
    POST /chat/stream   — Streaming chat via SSE
    POST /action        — Execute a desktop action via enad
    GET  /context       — Current desktop context
    GET  /health        — Runtime health check
    GET  /sessions      — List active sessions
"""

from collections.abc import AsyncIterator

from fastapi import FastAPI
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from src.context.state import DesktopState
from src.context.sessions import SessionManager
from src.inference.ollama import OllamaProvider
from src.inference.prompt import build_system_prompt

app = FastAPI(title="EnaOS AI Runtime", version="0.1.0")

# Shared state — injected at startup.
desktop_state = DesktopState()
session_manager = SessionManager()
provider = OllamaProvider()
bridge = None  # Set in main.py


class ChatRequest(BaseModel):
    query: str
    session_id: str | None = None


class ChatResponse(BaseModel):
    response: str
    session_id: str


class ActionRequest(BaseModel):
    action: str
    params: dict = {}


class ActionResponse(BaseModel):
    action_id: str | None = None
    status: str
    error: str | None = None


class ContextResponse(BaseModel):
    desktop: dict
    context_string: str


class HealthResponse(BaseModel):
    status: str
    enad_connected: bool
    ollama_available: bool
    model: str


@app.get("/health", response_model=HealthResponse)
async def health() -> HealthResponse:
    """Runtime health check."""
    ollama_ok = await provider.health_check()
    enad_ok = bridge.connected if bridge else False
    return HealthResponse(
        status="healthy" if ollama_ok and enad_ok else "degraded",
        enad_connected=enad_ok,
        ollama_available=ollama_ok,
        model=provider.model,
    )


@app.get("/context", response_model=ContextResponse)
async def get_context() -> ContextResponse:
    """Get current desktop context snapshot."""
    return ContextResponse(
        desktop=desktop_state.to_dict(),
        context_string=desktop_state.to_context_string(),
    )


@app.post("/chat", response_model=ChatResponse)
async def chat(req: ChatRequest) -> ChatResponse:
    """Non-streaming chat — returns full response."""
    session = session_manager.get_or_create(req.session_id)

    system_prompt = build_system_prompt(desktop_state)
    session.add_message("user", req.query)

    response_text = await provider.chat(
        messages=session.get_history(),
        system_prompt=system_prompt,
        stream=False,
    )

    session.add_message("assistant", response_text)
    return ChatResponse(response=response_text, session_id=session.id)


@app.post("/chat/stream")
async def chat_stream(req: ChatRequest) -> StreamingResponse:
    """Streaming chat — returns tokens via Server-Sent Events."""
    session = session_manager.get_or_create(req.session_id)

    system_prompt = build_system_prompt(desktop_state)
    session.add_message("user", req.query)

    async def event_stream() -> AsyncIterator[str]:
        yield f"event: session\ndata: {session.id}\n\n"

        full_response = ""

        try:
            token_stream = await provider.chat(
                messages=session.get_history(),
                system_prompt=system_prompt,
                stream=True,
            )

            async for token in token_stream:  # type: ignore[misc]
                full_response += token
                yield f"event: token\ndata: {token}\n\n"

        except Exception as e:
            yield f"event: error\ndata: {str(e)}\n\n"
            return

        session.add_message("assistant", full_response)
        yield f"event: done\ndata: complete\n\n"

    return StreamingResponse(
        event_stream(),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "Connection": "keep-alive"},
    )


@app.post("/action", response_model=ActionResponse)
async def execute_action(req: ActionRequest) -> ActionResponse:
    """Execute a desktop action via enad.

    The AI runtime NEVER directly manipulates the OS.
    All actions flow through enad for observability and control.
    """
    if not bridge or not bridge.connected:
        return ActionResponse(status="error", error="enad not connected")

    result = await bridge.execute_action(req.action, req.params)

    if result is None:
        return ActionResponse(status="error", error="No response from enad")

    if "error" in result:
        return ActionResponse(status="failed", error=result["error"])

    action_id = result.get("action_id", "")
    return ActionResponse(action_id=str(action_id), status="started")


@app.get("/sessions")
async def list_sessions() -> list[dict]:
    """List all active sessions."""
    return session_manager.list_sessions()


@app.delete("/sessions/{session_id}")
async def delete_session(session_id: str) -> dict:
    """Delete a session."""
    session_manager.delete(session_id)
    return {"status": "deleted", "session_id": session_id}
