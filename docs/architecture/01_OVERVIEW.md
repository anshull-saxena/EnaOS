# 1. System Overview

## 1.1 Repository Structure

EnaOS is a polyglot monorepo with two primary runtimes and a GTK4 shell:

```text
EnaOS/
├── runtimes/
│   ├── enad/              # Rust system daemon (core)
│   │   ├── src/
│   │   │   ├── main.rs          # Entry point, subsystem orchestration
│   │   │   ├── bus.rs           # Event bus (tokio broadcast)
│   │   │   ├── server.rs        # Unix socket IPC server
│   │   │   ├── process.rs       # Process lifecycle manager
│   │   │   ├── hooks.rs         # Signal handling
│   │   │   ├── system/          # Desktop integration subsystems
│   │   │   │   ├── upower.rs    # Battery/power (D-Bus)
│   │   │   │   ├── network.rs   # NetworkManager (D-Bus)
│   │   │   │   ├── window.rs    # Window focus (swaymsg/hyprctl/gdbus)
│   │   │   │   ├── workspace.rs # Workspace awareness
│   │   │   │   ├── clipboard.rs # Clipboard monitoring
│   │   │   │   ├── notifications.rs  # Freedesktop notifications
│   │   │   │   └── audio.rs     # PulseAudio + MPRIS
│   │   │   ├── actions/         # Action types, executor, handlers
│   │   │   ├── orchestration/   # DAG execution engine
│   │   │   ├── snapshot/        # Workspace snapshot store (SQLite)
│   │   │   ├── restore/         # Restoration planner
│   │   │   ├── context/         # Command intelligence engine
│   │   │   ├── suggestion/      # Ambient suggestion engine (SQLite)
│   │   │   ├── memory/          # Working memory store (SQLite)
│   │   │   ├── types/           # IPC types, event types
│   │   │   └── first_run.rs     # Onboarding management
│   │   └── Cargo.toml
│   └── ai-runtime/        # Python AI inference layer
│       ├── src/
│       │   ├── main.py          # FastAPI server entry point
│       │   ├── api/server.py    # HTTP API (FastAPI)
│       │   ├── bridge/enad.py   # Unix socket bridge to enad
│       │   ├── inference/       # Ollama integration, prompts
│       │   ├── orchestration/   # LLM-based plan parser
│       │   └── context/         # Desktop state, sessions
│       └── requirements.txt
│
├── shell/
│   └── ena-bar/           # Native GTK4 bar (Rust)
│       ├── src/
│       │   ├── main.rs          # GTK4 app, layer-shell setup
│       │   ├── bar.rs           # Widget tree, state machine
│       │   ├── ipc.rs           # Unix socket client
│       │   ├── command_palette.rs    # Context-aware command suggestions
│       │   ├── restoration_ui.rs     # Workspace restoration widget
│       │   ├── orchestration_ui.rs   # Execution plan timeline
│       │   ├── ambient_ui.rs         # Ambient suggestions
│       │   ├── welcome_overlay.rs    # First-run onboarding
│       │   ├── timing.rs         # Interaction latency instrumentation
│       │   ├── audio.rs          # Audio capture stub
│       │   ├── config.rs         # CLI args
│       │   └── style.css         # Dark theme
│       └── Cargo.toml
│
├── apps/                  # Tauri + React bar (legacy/alternative)
├── packages/              # Shared types, design system
├── docs/                  # Architecture, quickstart, changelog
└── scripts/               # Deploy scripts
```

## 1.2 Technology Stack

### Runtime
| Component | Language | Framework | Persistence | IPC |
|-----------|----------|-----------|-------------|-----|
| **Daemon (enad)** | Rust | tokio | SQLite (rusqlite, bundled) | Unix socket (JSON line) |
| **GTK4 Bar (ena-bar)** | Rust | gtk4-rs, gtk4-layer-shell | None (stateless renderer) | Unix socket client |
| **AI Runtime** | Python 3.11+ | FastAPI, uvicorn | None (in-memory sessions) | Unix socket → enad |

### Desktop Integration
| Subsystem | Integration | Backend |
|-----------|-------------|---------|
| Battery | D-Bus | zbus (org.freedesktop.UPower) |
| Network | D-Bus | zbus (org.freedesktop.NetworkManager) |
| Window Focus | External tools | swaymsg / hyprctl / gdbus / xprop fallback |
| Workspace | External tools | swaymsg / hyprctl |
| Clipboard | External tools | wl-paste / xclip (polling) |
| Audio | External tools + D-Bus | pactl + MPRIS signal subscription |
| Notifications | D-Bus | zbus (org.freedesktop.Notifications) |

### IPC Protocol
- **Transport:** Unix domain socket
- **Format:** Line-delimited JSON
- **Encoding:** Adjacently-tagged serde enums (`{"kind": {"type": "...", "body": ...}}`)
- **Message types:** Command, Response, Event, Subscribe, Ping, Pong
- **Latency:** < 1ms P99 (localhost Unix socket)
- **Tests:** 71 round-trip, wire-format, and integration tests

## 1.3 Architectural Principles

### Daemon-Driven Architecture
The frontend is a thin reactive renderer. All business logic lives in `enad`:
- Event bus, IPC server, desktop integration
- Orchestration engine, snapshot/restore
- Command intelligence, ambient suggestions
- First-run management, memory

### Real State Only
No simulated UI, no fake workflows. Every bar element reflects actual OS state:
- Status dot: green = connected, grey = disconnected
- Context label: shows only actively tracked state
- Action bar: shows real execution results from enad

### Graceful Degradation
If a subsystem is unavailable, it logs and exits cleanly — enad never crashes:
- D-Bus service unavailable → log warning, continue
- AI runtime unavailable → bar works without AI features
- macOS → layer-shell disabled, floating window fallback

### Compositor-Agnostic
Window tracking uses a fallback chain: Sway → Hyprland → GNOME → generic wmctrl/xprop

## 1.4 Key Files

| File | Purpose |
|------|---------|
| `runtimes/enad/src/main.rs` | Daemon entry point, subsystem wiring |
| `runtimes/enad/src/bus.rs` | Event bus (tokio broadcast) |
| `runtimes/enad/src/server.rs` | Unix socket IPC server |
| `runtimes/enad/src/types/ipc.rs` | IPC message types (IpcMessage, Command, Response) |
| `runtimes/enad/src/types/events.rs` | System event types (EventKind, EventPayload) |
| `shell/ena-bar/src/bar.rs` | GTK4 widget tree and state machine |
| `shell/ena-bar/src/ipc.rs` | Unix socket IPC client |
| `shell/ena-bar/src/welcome_overlay.rs` | First-run onboarding widget |
| `shell/ena-bar/src/style.css` | Dark theme stylesheet |
| `runtimes/ai-runtime/src/main.py` | AI runtime entry point |

## 1.5 Build System

### Rust Components
- No workspace — each crate builds independently
- Release profile: `lto = true`, `codegen-units = 1`, `opt-level = 2`
- Conditional dependencies via `cfg(target_os = "linux")` for `gtk4-layer-shell`, `zbus`, `nix`
- Feature flags: `timing = []` for instrumentation, `desktop_integration` CLI arg

### Python Components
- Standard `pip install -r requirements.txt`
- Virtual environment recommended: `python3 -m venv .venv`

### Tests
- 71 tests total (3 pre-existing, 68 added in Stabilization Sprints)
- IPC round-trip serde tests for all message types
- Wire-format compatibility tests with bar JSON construction
- Integration tests with real IPC server

## 1.6 Git Strategy
- Trunk-based development: short-lived branches from `main`
- Conventional commits: `feat:` / `fix:` / `docs:` / `refactor:` / `test:`
- PRs require at least one review
