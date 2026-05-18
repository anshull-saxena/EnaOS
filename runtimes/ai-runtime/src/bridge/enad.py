"""Bridge to enad — subscribes to system events via Unix domain socket."""

import asyncio
import json
from typing import Any, Callable, Awaitable

from src.config import settings

EventCallback = Callable[[dict], Awaitable[None]]


class EnadBridge:
    """Subscribes to enad's event bus over Unix domain socket.

    Maintains a persistent connection and forwards all system events
    to registered callbacks. Reconnects automatically on failure.
    """

    def __init__(self) -> None:
        self._callbacks: list[EventCallback] = []
        self._running = False
        self._connected = False
        self._socket_path = settings.enad_socket

    def on_event(self, callback: EventCallback) -> None:
        """Register an event callback."""
        self._callbacks.append(callback)

    @property
    def connected(self) -> bool:
        return self._connected

    async def start(self) -> None:
        """Begin the connection loop. Runs until stopped."""
        self._running = True
        while self._running:
            try:
                await self._connect_and_listen()
            except Exception as e:
                print(f"[enad-bridge] connection error: {e}")

            if self._running:
                self._connected = False
                await asyncio.sleep(2)

    async def stop(self) -> None:
        """Stop the bridge."""
        self._running = False

    async def _connect_and_listen(self) -> None:
        """Connect to enad, subscribe, and forward events."""
        reader, writer = await asyncio.open_unix_connection(self._socket_path)

        self._connected = True
        print("[enad-bridge] connected to enad")

        try:
            # Subscribe to all events.
            subscribe = {
                "id": "00000000-0000-0000-0000-000000000000",
                "type": "Subscribe",
                "body": {"kinds": []},
            }
            writer.write((json.dumps(subscribe) + "\n").encode())
            await writer.drain()

            # Read events line by line.
            while self._running:
                line = await reader.readline()
                if not line:
                    print("[enad-bridge] connection closed by enad")
                    break

                try:
                    msg = json.loads(line.decode().strip())
                    await self._dispatch(msg)
                except json.JSONDecodeError:
                    continue

        finally:
            self._connected = False
            writer.close()
            await writer.wait_closed()

    async def _dispatch(self, msg: dict) -> None:
        """Dispatch an IPC message to all callbacks."""
        msg_type = msg.get("type")

        if msg_type == "Event":
            event = msg.get("body", {})
            for cb in self._callbacks:
                try:
                    await cb(event)
                except Exception as e:
                    print(f"[enad-bridge] callback error: {e}")

    async def _send_command(self, body: dict) -> dict | None:
        """Send a command to enad and return the response body."""
        try:
            reader, writer = await asyncio.open_unix_connection(self._socket_path)

            cmd = {
                "id": "00000000-0000-0000-0000-000000000001",
                "type": "Command",
                "body": body,
            }
            writer.write((json.dumps(cmd) + "\n").encode())
            await writer.drain()

            line = await reader.readline()
            writer.close()
            await writer.wait_closed()

            if line:
                resp = json.loads(line.decode().strip())
                body_resp = resp.get("body", {})
                if body_resp.get("type") == "Data":
                    return body_resp.get("payload")
                elif body_resp.get("type") == "Error":
                    return {"error": body_resp.get("message", "Unknown error")}
                elif body_resp.get("type") == "Ok":
                    return {"status": "ok", "message": body_resp.get("message")}

        except Exception as e:
            print(f"[enad-bridge] command error: {e}")

        return None

    async def query(self, target: str) -> dict | None:
        """Send a QueryState command to enad and return the response."""
        return await self._send_command({"type": "QueryState", "target": target})

    async def query_memory(self, query_type: str, **kwargs) -> dict | None:
        """Query enad memory.

        query_type: "MemoryRecent", "MemorySummary", or "MemorySearch"
        """
        if query_type == "MemorySearch":
            target = {"type": "MemorySearch", "query": kwargs.get("query", "")}
        else:
            target = query_type

        return await self._send_command({"type": "QueryState", "target": target})

    async def execute_action(self, action: str, params: dict) -> dict | None:
        """Send an ExecuteAction command to enad."""
        return await self._send_command({
            "type": "ExecuteAction",
            "action": action,
            "params": params,
        })

    # ── Orchestration commands ──

    async def submit_plan(self, plan: dict) -> dict | None:
        """Submit an execution plan to enad.

        plan: The full ExecutionPlan dict as expected by enad.
        Returns {plan_id, status} or {error}.
        """
        return await self._send_command({
            "type": "SubmitPlan",
            "plan": plan,
        })

    async def approve_plan(self, plan_id: str) -> dict | None:
        """Approve a pending plan for execution."""
        return await self._send_command({
            "type": "ApprovePlan",
            "plan_id": plan_id,
        })

    async def reject_plan(self, plan_id: str) -> dict | None:
        """Reject a pending plan."""
        return await self._send_command({
            "type": "RejectPlan",
            "plan_id": plan_id,
        })

    async def cancel_plan(self, plan_id: str) -> dict | None:
        """Cancel a running plan."""
        return await self._send_command({
            "type": "CancelPlan",
            "plan_id": plan_id,
        })

    async def list_plans(self) -> list[dict] | None:
        """List all plans (active + pending)."""
        result = await self._send_command({"type": "ListPlans"})
        if isinstance(result, list):
            return result
        return None

    # ── Snapshot commands ──

    async def take_snapshot(self, label: str | None = None) -> dict | None:
        """Take a workspace snapshot."""
        body: dict[str, object] = {"type": "TakeSnapshot"}
        if label:
            body["label"] = label
        return await self._send_command(body)

    async def list_snapshots(self, limit: int = 20) -> list[dict] | None:
        """List recent snapshots."""
        result = await self._send_command({"type": "ListSnapshots", "limit": limit})
        if isinstance(result, list):
            return result
        return None

    async def get_snapshot(self, snapshot_id: str) -> dict | None:
        """Get a full snapshot by ID."""
        return await self._send_command({
            "type": "GetSnapshot",
            "snapshot_id": snapshot_id,
        })

    async def delete_snapshot(self, snapshot_id: str) -> dict | None:
        """Delete a snapshot."""
        return await self._send_command({
            "type": "DeleteSnapshot",
            "snapshot_id": snapshot_id,
        })

    # ── Restoration commands ──

    async def preview_restore(self, snapshot_id: str) -> dict | None:
        """Preview what restoring a snapshot would do."""
        return await self._send_command({
            "type": "PreviewRestore",
            "snapshot_id": snapshot_id,
        })

    async def restore_snapshot(self, snapshot_id: str, selections: dict | None = None) -> dict | None:
        """Restore a snapshot as an orchestration plan."""
        body: dict[str, object] = {
            "type": "RestoreSnapshot",
            "snapshot_id": snapshot_id,
        }
        if selections:
            body["selections"] = selections
        return await self._send_command(body)
