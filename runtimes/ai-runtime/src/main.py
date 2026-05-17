"""EnaOS AI Runtime — Contextual desktop intelligence layer.

This daemon:
1. Subscribes to enad system events via Unix socket
2. Maintains a live desktop state snapshot
3. Exposes a FastAPI HTTP interface for chat with context injection
4. Streams LLM responses via Server-Sent Events
5. Supports local inference via Ollama
"""

import asyncio
import signal

import uvicorn

from src.api.server import app, desktop_state
from src.bridge.enad import EnadBridge
from src.config import settings


async def on_enad_event(event: dict) -> None:
    """Handle incoming enad system events."""
    desktop_state.update(event)


async def main() -> None:
    """Start the AI runtime."""
    print("[ai-runtime] starting EnaOS AI Runtime")
    print(f"[ai-runtime] enad socket: {settings.enad_socket}")
    print(f"[ai-runtime] ollama: {settings.ollama_url} ({settings.ollama_model})")
    print(f"[ai-runtime] api: http://{settings.host}:{settings.port}")

    # Start enad bridge in background.
    bridge = EnadBridge()
    bridge.on_event(on_enad_event)

    bridge_task = asyncio.create_task(bridge.start())

    # Try to fetch initial context from enad.
    await asyncio.sleep(1)
    context = await bridge.query("DesktopContext")
    if context:
        print(f"[ai-runtime] initial context: {context}")

    # Start FastAPI server.
    config = uvicorn.Config(
        app,
        host=settings.host,
        port=settings.port,
        log_level="info",
    )
    server = uvicorn.Server(config)

    # Handle shutdown.
    loop = asyncio.get_event_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        loop.add_signal_handler(sig, server.should_exit = True)

    # Run server and bridge concurrently.
    await asyncio.gather(
        server.serve(),
        bridge_task,
    )

    # Cleanup.
    await bridge.stop()
    print("[ai-runtime] stopped")


if __name__ == "__main__":
    asyncio.run(main())
