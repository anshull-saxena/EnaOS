# 2. System Architecture & Service Boundaries

> **Status:** Accurate as of v0.1.0-developer-preview
> **Last verified:** June 2026

## 2.1 Service Boundaries

EnaOS follows a **strictly daemon-driven architecture** with precisely three components:

| Component | Language | Runtime | Privilege | Persistence |
|-----------|----------|---------|-----------|-------------|
| **enad** (core daemon) | Rust | tokio | System-level | SQLite (bundled) |
| **ena-bar** (GTK4 frontend) | Rust | GTK4 loop | User-level | None (stateless) |
| **AI Runtime** | Python | FastAPI/uvicorn | User-level | In-memory sessions |

### Enad — The Single Privileged Daemon

`enad` is the **only** component with system access. It owns:
- Unix socket IPC server (`server.rs`)
- tokio broadcast event bus (`bus.rs`)
- 7 desktop integration subsystems (UPower, NetworkManager, window focus, workspace, clipboard, notifications, audio/MPRIS)
- Process lifecycle management (`process.rs`)
- Action execution (`actions/executor.rs`)
- DAG-based orchestration engine (`orchestration/engine.rs`)
- Workspace snapshot store (`snapshot/store.rs`)
- Restoration planner (`restore/plan.rs`)
- Contextual command intelligence engine (`context/`)
- Ambient suggestion engine (`suggestion/`)
- Working memory store (`memory/store.rs`)
- First-run onboarding manager (`first_run.rs`)

### Ena-bar — The Stateless Renderer

`ena-bar` is a thin GTK4 application with **zero business logic**:
- Connects to enad via Unix socket
- Renders system state as widgets (status dot, context label, command palette, restoration suggestion, ambient suggestions, orchestration timeline)
- Never invents state — every widget corresponds to a real daemon event

### AI Runtime — The Inference Layer

The AI Runtime is an **optional** Python FastAPI server:
- Connects to enad via Unix socket bridge
- Maintains a live DesktopState from enad events
- Routes queries to local Ollama or cloud API (configured)
- Streams responses via SSE
- Generates structured execution plans from natural language
- Never executes OS commands directly — always routes through enad

## 2.2 Inter-Process Communication

### Transport: Unix Domain Socket

All communication uses a **single Unix domain socket** with **line-delimited JSON**.

### Protocol: Adjacently-Tagged Enums

Every message follows the structure:

```json
{"id": "uuid", "kind": {"type": "MessageType", "body": ...}}
```

Message types:
- **Command** — Client → Server requests (22 variants)
- **Response** — Server → Client replies (Ok, Data, Error)
- **Event** — Server → Client push events (28 EventPayload variants)
- **Subscribe** — Client subscription to event kinds
- **Ping / Pong** — Keepalive heartbeat

### No gRPC, HTTP, SSE, WebSocket, or Event Bus Services

EnaOS deliberately avoids:
- gRPC — unnecessary overhead for single-machine IPC
- HTTP/REST — adds latency, requires server implementation
- WebSocket/SSE — Unix socket JSON lines are simpler and faster
- Redis/NATS — no distributed event bus needed for single-machine operation

## 2.3 State Management

### Frontend (ena-bar)
- **Stateless** — all state comes from enad events
- Widget states derived from latest event data
- No local persistence

### Daemon (enad)
- **Three SQLite databases** in `~/.local/share/enad/`:
  - `snapshots.db` — workspace snapshots (WAL mode)
  - `memory.db` — working memory with FTS5 search
  - `suggestions.db` — ambient suggestion store
- **In-memory state**: event bus state, active orchestration plans, current desktop context

### AI Runtime
- **In-memory sessions** — conversation history per session
- **No persistence** — sessions lost on restart (future: persistence)

## 2.4 Desktop Shell Architecture

### Wayland Layer-Shell
- `ena-bar` uses `gtk4-layer-shell` protocol to anchor to the bottom of the screen
- Layer: Overlay (`set_layer(Overlay)`)
- Exclusive zone: -1 (bar sits above normal windows as an overlay)
- Keyboard mode: OnDemand (focusable but doesn't steal focus)

### Compositor Support
- GNOME (Mutter) — works with [gtk4-layer-shell extension](https://github.com/wmww/gtk4-layer-shell)
- Sway — native wlr-layer-shell support
- Hyprland — native wlr-layer-shell support
- macOS — development-only fallback (floating window, no layer-shell)

### No Custom Compositor
EnaOS **does not ship a custom Wayland compositor**. It runs on top of existing compositors and integrates via layer-shell and external tools.

## 2.5 AI Runtime Architecture

The AI Runtime is **not** a daemon managed by enad — it's a standalone Python FastAPI server that:
1. Connects to enad via Unix socket (bridge/enad.py)
2. Subscribes to all event kinds
3. Maintains a live `DesktopState` from received events
4. Provides HTTP endpoints: `/chat`, `/chat/stream` (SSE), `/health`, `/context`, `/memory`, `/action`, `/orchestrate`
5. Integrates with Ollama for local inference
6. Uses enad for all system actions (never executes directly)

```text
┌─────────┐   Unix Socket    ┌──────────┐   HTTP/SSE    ┌──────────┐
│  enad   │ ◄──────────────► │ AI       │ ◄───────────► │  Ollama  │
│ (Rust)  │                  │ Runtime  │               │ (local)  │
│         │                  │ (Python) │               └──────────┘
│ events  │                  │          │
│ actions │                  └──────────┘
└─────────┘                       │
                                  │ HTTP/SSE
                                  ▼
                            ┌──────────┐
                            │ ena-bar  │
                            │ (GTK4)   │
                            └──────────┘
```

> **Key invariant:** The AI Runtime never directly manipulates the OS. All actions are routed through enad's validated IPC endpoints.
