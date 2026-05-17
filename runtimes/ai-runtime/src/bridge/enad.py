"""Bridge to enad — subscribes to system events via Unix domain socket."""

import asyncio
import json
from typing import Callable, Awaitable

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

    async def query(self, target: str) -> dict | None:
        """Send a QueryState command to enad and return the response."""
        try:
            reader, writer = await asyncio.open_unix_connection(self._socket_path)

            query = {
                "id": "00000000-0000-0000-0000-000000000001",
                "type": "Command",
                "body": {"type": "QueryState", "target": target},
            }
            writer.write((json.dumps(query) + "\n").encode())
            await writer.drain()

            line = await reader.readline()
            writer.close()
            await writer.wait_closed()

            if line:
                resp = json.loads(line.decode().strip())
                body = resp.get("body", {})
                if body.get("type") == "Data":
                    return body.get("payload")

        except Exception as e:
            print(f"[enad-bridge] query error: {e}")

        return None

    async def execute_action(self, action: str, params: dict) -> dict | None:
        """Send an ExecuteAction command to enad."""
        try:
            reader, writer = await asyncio.open_unix_connection(self._socket_path)

            cmd = {
                "id": "00000000-0000-0000-0000-000000000002",
                "type": "Command",
                "body": {
                    "type": "ExecuteAction",
                    "action": action,
                    "params": params,
                },
            }
            writer.write((json.dumps(cmd) + "\n").encode())
            await writer.drain()

            line = await reader.readline()
            writer.close()
            await writer.wait_closed()

            if line:
                resp = json.loads(line.decode().strip())
                body = resp.get("body", {})
                if body.get("type") == "Data":
                    return body.get("payload")
                elif body.get("type") == "Error":
                    return {"error": body.get("message", "Unknown error")}

        except Exception as e:
            print(f"[enad-bridge] action error: {e}")

        return None
