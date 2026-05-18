"""FastAPI application — AI runtime HTTP interface.

Endpoints:
    POST /chat          — Non-streaming chat
    POST /chat/stream   — Streaming chat via SSE
    POST /action        — Execute a desktop action via enad
    GET  /context       — Current desktop context
    GET  /memory        — Working memory summary
    GET  /health        — Runtime health check
    GET  /sessions      — List active sessions

Orchestration endpoints:
    POST /orchestrate        — Parse intent + submit plan
    POST /orchestrate/stream  — Stream intent parsing
    POST /plan/{id}/approve   — Approve a pending plan
    POST /plan/{id}/reject    — Reject a pending plan
    POST /plan/{id}/cancel    — Cancel a running plan
    GET  /plans               — List all plans
"""

import json
from collections.abc import AsyncIterator

from fastapi import FastAPI
from fastapi.responses import StreamingResponse
from pydantic import BaseModel

from src.context.state import DesktopState
from src.context.sessions import SessionManager
from src.inference.ollama import OllamaProvider
from src.inference.prompt import build_system_prompt, format_memory_context
from src.orchestration.planner import Planner

app = FastAPI(title="EnaOS AI Runtime", version="0.1.0")

# Shared state — injected at startup.
desktop_state = DesktopState()
session_manager = SessionManager()
provider = OllamaProvider()
planner = Planner(provider)
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


class OrchestrateRequest(BaseModel):
    intent: str
    auto_approve: bool = False


class OrchestrateResponse(BaseModel):
    plan_id: str | None = None
    title: str
    description: str
    node_count: int
    requires_approval: bool
    reasons: list[str] = []
    error: str | None = None


class PlanActionResponse(BaseModel):
    status: str
    message: str
    error: str | None = None


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


@app.get("/memory")
async def get_memory() -> dict:
    """Get working memory summary from enad."""
    if not bridge or not bridge.connected:
        return {"error": "enad not connected"}

    summary = await bridge.query_memory("MemorySummary")
    return summary or {}


@app.post("/chat", response_model=ChatResponse)
async def chat(req: ChatRequest) -> ChatResponse:
    """Non-streaming chat — returns full response."""
    session = session_manager.get_or_create(req.session_id)

    # Build system prompt with desktop context + working memory.
    memory_context = ""
    if bridge and bridge.connected:
        memory_data = await bridge.query_memory("MemorySummary")
        if memory_data:
            memory_context = format_memory_context(memory_data)

    system_prompt = build_system_prompt(desktop_state, memory_context)
    session.add_message("user", req.query)

    response_text = await provider.chat(
        messages=session.get_history(),
        system_prompt=system_prompt,
        stream=False,
    )

    session.add_message("assistant", response_text)

    # Record intent and response in memory.
    if bridge and bridge.connected:
        try:
            await bridge.query_memory("MemorySummary")  # Trigger memory capture
        except Exception:
            pass

    return ChatResponse(response=response_text, session_id=session.id)


@app.post("/chat/stream")
async def chat_stream(req: ChatRequest) -> StreamingResponse:
    """Streaming chat — returns tokens via Server-Sent Events."""
    session = session_manager.get_or_create(req.session_id)

    # Build system prompt with desktop context + working memory.
    memory_context = ""
    if bridge and bridge.connected:
        memory_data = await bridge.query_memory("MemorySummary")
        if memory_data:
            memory_context = format_memory_context(memory_data)

    system_prompt = build_system_prompt(desktop_state, memory_context)
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


class MemorySearchRequest(BaseModel):
    query: str


class MemorySearchResponse(BaseModel):
    results: list[dict]
    count: int


@app.post("/memory/search", response_model=MemorySearchResponse)
async def search_memory(req: MemorySearchRequest) -> MemorySearchResponse:
    """Search working memory."""
    if not bridge or not bridge.connected:
        return MemorySearchResponse(results=[], count=0)

    results = await bridge.query_memory("MemorySearch", query=req.query)
    if results and isinstance(results, list):
        return MemorySearchResponse(results=results, count=len(results))
    return MemorySearchResponse(results=[], count=0)


@app.get("/sessions")
async def list_sessions() -> list[dict]:
    """List all active sessions."""
    return session_manager.list_sessions()


@app.delete("/sessions/{session_id}")
async def delete_session(session_id: str) -> dict:
    """Delete a session."""
    session_manager.delete(session_id)
    return {"status": "deleted", "session_id": session_id}


# ── Orchestration endpoints ─────────────────────────────────


@app.post("/orchestrate", response_model=OrchestrateResponse)
async def orchestrate(req: OrchestrateRequest) -> OrchestrateResponse:
    """Parse a natural language intent and submit an execution plan to enad.

    The intent is parsed by the LLM into a structured plan, then
    submitted to enad for approval and execution. The LLM NEVER
    directly executes actions — it only produces plan documents.
    """
    if not bridge or not bridge.connected:
        return OrchestrateResponse(
            title="",
            description="",
            node_count=0,
            requires_approval=False,
            error="enad not connected",
        )

    # Get working memory for context.
    memory_context = ""
    memory_data = await bridge.query_memory("MemorySummary")
    if memory_data:
        memory_context = format_memory_context(memory_data)

    # Parse intent into a plan via LLM.
    plan = await planner.plan(req.intent, desktop_state, memory_context)

    if plan is None:
        return OrchestrateResponse(
            title="",
            description="",
            node_count=0,
            requires_approval=False,
            error="Could not parse intent into an executable plan",
        )

    # Check if plan requires approval.
    needs_approval, reasons = planner.plan_requires_approval(plan)

    # Submit to enad.
    result = await bridge.submit_plan(plan.to_enad_plan())

    if result is None:
        return OrchestrateResponse(
            title=plan.title,
            description=plan.description,
            node_count=len(plan.nodes),
            requires_approval=needs_approval,
            reasons=reasons,
            error="Failed to submit plan to enad",
        )

    if "error" in result:
        return OrchestrateResponse(
            title=plan.title,
            description=plan.description,
            node_count=len(plan.nodes),
            requires_approval=needs_approval,
            reasons=reasons,
            error=result["error"],
        )

    plan_id = result.get("plan_id", plan.id)

    return OrchestrateResponse(
        plan_id=plan_id,
        title=plan.title,
        description=plan.description,
        node_count=len(plan.nodes),
        requires_approval=needs_approval,
        reasons=reasons,
    )


@app.post("/orchestrate/stream")
async def orchestrate_stream(req: OrchestrateRequest) -> StreamingResponse:
    """Stream intent parsing tokens, then auto-submit the plan.

    Returns SSE events:
      - event: token (raw LLM tokens during parsing)
      - event: plan (the full parsed plan as JSON)
      - event: error (if parsing failed)
    """
    memory_context = ""
    if bridge and bridge.connected:
        memory_data = await bridge.query_memory("MemorySummary")
        if memory_data:
            memory_context = format_memory_context(memory_data)

    async def event_stream() -> AsyncIterator[str]:
        tokens: list[str] = []
        try:
            async for token in planner.plan_stream(req.intent, desktop_state, memory_context):
                tokens.append(token)
                yield f"event: token\ndata: {token}\n\n"
        except Exception as e:
            yield f"event: error\ndata: {str(e)}\n\n"
            return

        # Parse the accumulated tokens.
        full_text = "".join(tokens)
        plan = planner._parse_plan_response(full_text)
        if plan is None:
            yield "event: error\ndata: Could not parse intent into a plan\n\n"
            return

        # Submit to enad if connected.
        if bridge and bridge.connected:
            result = await bridge.submit_plan(plan.to_enad_plan())
            if result and "plan_id" in result:
                plan.id = result["plan_id"]

        yield f"event: plan\ndata: {json.dumps({'plan_id': plan.id, 'title': plan.title, 'node_count': len(plan.nodes)})}\n\n"

    return StreamingResponse(
        event_stream(),
        media_type="text/event-stream",
        headers={"Cache-Control": "no-cache", "Connection": "keep-alive"},
    )


@app.post("/plan/{plan_id}/approve", response_model=PlanActionResponse)
async def approve_plan(plan_id: str) -> PlanActionResponse:
    """Approve a pending execution plan."""
    if not bridge or not bridge.connected:
        return PlanActionResponse(status="error", message="", error="enad not connected")

    result = await bridge.approve_plan(plan_id)
    if result is None:
        return PlanActionResponse(status="error", message="", error="No response from enad")

    if "error" in result:
        return PlanActionResponse(status="error", message="", error=result["error"])

    return PlanActionResponse(status="approved", message=f"Plan {plan_id} approved")


@app.post("/plan/{plan_id}/reject", response_model=PlanActionResponse)
async def reject_plan(plan_id: str) -> PlanActionResponse:
    """Reject a pending execution plan."""
    if not bridge or not bridge.connected:
        return PlanActionResponse(status="error", message="", error="enad not connected")

    result = await bridge.reject_plan(plan_id)
    if result is None:
        return PlanActionResponse(status="error", message="", error="No response from enad")

    if "error" in result:
        return PlanActionResponse(status="error", message="", error=result["error"])

    return PlanActionResponse(status="rejected", message=f"Plan {plan_id} rejected")


@app.post("/plan/{plan_id}/cancel")
async def cancel_plan(plan_id: str) -> dict:
    """Cancel a running execution plan."""
    if not bridge or not bridge.connected:
        return {"status": "error", "error": "enad not connected"}

    result = await bridge.cancel_plan(plan_id)
    if result and "error" in result:
        return {"status": "error", "error": result["error"]}

    return {"status": "cancelled", "plan_id": plan_id}


@app.get("/plans")
async def list_plans() -> list[dict]:
    """List all plans (active + pending)."""
    if not bridge or not bridge.connected:
        return []

    plans = await bridge.list_plans()
    return plans or []


# ── Workspace Snapshot endpoints ────────────────────────────


class SnapshotTakeRequest(BaseModel):
    label: str | None = None


class SnapshotTakeResponse(BaseModel):
    snapshot_id: str | None = None
    status: str
    error: str | None = None


@app.post("/snapshot/take", response_model=SnapshotTakeResponse)
async def take_snapshot(req: SnapshotTakeRequest) -> SnapshotTakeResponse:
    """Take a workspace snapshot."""
    if not bridge or not bridge.connected:
        return SnapshotTakeResponse(status="error", error="enad not connected")

    result = await bridge.take_snapshot(req.label)
    if result is None:
        return SnapshotTakeResponse(status="error", error="No response from enad")
    if "error" in result:
        return SnapshotTakeResponse(status="error", error=result["error"])

    return SnapshotTakeResponse(
        snapshot_id=result.get("snapshot_id"),
        status="taken",
    )


@app.get("/snapshots")
async def list_snapshots(limit: int = 20) -> list[dict]:
    """List recent workspace snapshots."""
    if not bridge or not bridge.connected:
        return []
    snapshots = await bridge.list_snapshots(limit)
    return snapshots or []


@app.get("/snapshot/{snapshot_id}")
async def get_snapshot(snapshot_id: str) -> dict:
    """Get a full snapshot by ID."""
    if not bridge or not bridge.connected:
        return {"error": "enad not connected"}
    result = await bridge.get_snapshot(snapshot_id)
    return result or {"error": "Not found"}


@app.delete("/snapshot/{snapshot_id}")
async def delete_snapshot(snapshot_id: str) -> dict:
    """Delete a snapshot."""
    if not bridge or not bridge.connected:
        return {"status": "error", "error": "enad not connected"}
    result = await bridge.delete_snapshot(snapshot_id)
    if result and "error" in result:
        return {"status": "error", "error": result["error"]}
    return {"status": "deleted", "snapshot_id": snapshot_id}


# ── Restoration endpoints ──────────────────────────────────


class RestoreSelections(BaseModel):
    applications: bool = True
    workspaces: bool = True
    terminals: bool = False
    browser_urls: bool = False
    orchestration_context: bool = False


class RestorePreviewResponse(BaseModel):
    snapshot_id: str
    snapshot_label: str
    snapshot_taken_at: str
    action_count: int
    actions: list[dict]
    error: str | None = None


@app.post("/snapshot/{snapshot_id}/preview", response_model=RestorePreviewResponse)
async def preview_restore(snapshot_id: str) -> RestorePreviewResponse:
    """Preview what restoring a snapshot would do."""
    if not bridge or not bridge.connected:
        return RestorePreviewResponse(
            snapshot_id=snapshot_id, snapshot_label="", snapshot_taken_at="",
            action_count=0, actions=[], error="enad not connected",
        )

    result = await bridge.preview_restore(snapshot_id)
    if result is None or "error" in (result or {}):
        return RestorePreviewResponse(
            snapshot_id=snapshot_id, snapshot_label="", snapshot_taken_at="",
            action_count=0, actions=[], error=(result or {}).get("error", "Preview failed"),
        )

    return RestorePreviewResponse(
        snapshot_id=result.get("snapshot_id", snapshot_id),
        snapshot_label=result.get("snapshot_label", ""),
        snapshot_taken_at=result.get("snapshot_taken_at", ""),
        action_count=result.get("action_count", 0),
        actions=result.get("actions", []),
    )


class RestoreRequest(BaseModel):
    selections: RestoreSelections | None = None


class RestoreResponse(BaseModel):
    snapshot_id: str
    plan_id: str | None = None
    action_count: int
    status: str
    error: str | None = None


@app.post("/snapshot/{snapshot_id}/restore", response_model=RestoreResponse)
async def restore_snapshot(snapshot_id: str, req: RestoreRequest) -> RestoreResponse:
    """Restore a workspace snapshot as an orchestration plan.

    The restoration plan goes through the standard approval flow.
    The user must approve it before actions are executed.
    """
    if not bridge or not bridge.connected:
        return RestoreResponse(
            snapshot_id=snapshot_id, action_count=0,
            status="error", error="enad not connected",
        )

    selections = req.selections.model_dump() if req.selections else None
    result = await bridge.restore_snapshot(snapshot_id, selections)

    if result is None:
        return RestoreResponse(
            snapshot_id=snapshot_id, action_count=0,
            status="error", error="No response from enad",
        )
    if "error" in result:
        return RestoreResponse(
            snapshot_id=snapshot_id, action_count=0,
            status="error", error=result["error"],
        )

    return RestoreResponse(
        snapshot_id=result.get("snapshot_id", snapshot_id),
        plan_id=result.get("plan_id"),
        action_count=result.get("action_count", 0),
        status="plan_submitted",
    )
