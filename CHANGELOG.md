# Changelog

## Developer Preview 0.1 (2026-06)

**EnaOS is an AI-native desktop operating environment for Linux.**
Native GTK4, Rust daemon, Wayland layer-shell — no browser, no Electron, no JavaScript runtime.

### Features

#### Core Daemon (`enad`)
- Event bus (tokio broadcast) with per-kind subscription + catch-all
- Unix socket IPC server with line-delimited JSON, adjacently-tagged enums
- Process lifecycle manager (spawn, track, reap zombies)
- Signal handling (SIGINT/SIGTERM graceful shutdown)
- Desktop integration: UPower (battery), NetworkManager (WiFi), window focus (Sway/Hyprland/GNOME), workspace, clipboard, notifications, audio/MPRIS
- Orchestration engine: DAG-based plan execution with topological sort, retry, rollback, and cancellation
- Workspace snapshot store (SQLite with WAL mode): capture, list, get, delete, auto-expire
- Restoration planner: snapshot-to-execution-plan transformation with preview
- Contextual command intelligence engine: intent classification, 5-source command resolution, confidence-ranked suggestions
- Ambient suggestion engine: event-driven proactive suggestions with rate limiting and dismissal tracking
- First-run manager: fresh install detection, onboarding tracking, demo data seeding

#### GTK4 Bar (`ena-bar`)
- Wayland layer-shell surface (bottom-center, overlay layer, exclusive zone -1)
- 4-state UI: Collapsed → Expanded → Thinking → Result
- Status dot with frame-clock-driven breathing animation
- Context-aware command palette: 40ms debounce, background-thread IPC, stale-response protection
- Restoration suggestion widget: compact bar → preview pane with per-action toggles
- Orchestration timeline: real-time node status with approval prompt
- Ambient suggestion widget: non-intrusive, auto-dismiss, one-click action
- Welcome overlay: crossfade intro with 3 suggestion chips, 12s auto-dismiss
- System context display: focused app, workspace, battery, WiFi, media playback
- Action execution feedback: animated status bar with auto-dismiss
- Reconnection: 2s backoff, "disconnected" indicator, status dot color changes
- macOS development mode: floating bottom-center window (no layer-shell)

#### AI Runtime (`ai-runtime`)
- Python FastAPI server with /chat, /chat/stream (SSE), /health, /context, /memory, /action, /orchestrate
- Ollama integration for local LLM inference with streaming tokens
- Enad bridge: subscribes to enad events via Unix socket, maintains live DesktopState
- Context injection: system prompt built from desktop state + working memory
- Orchestration planner: LLM-parses natural language intent into structured execution plans
- Provider router: local Ollama for simple queries (future: cloud fallback)
- Session management: per-user conversation history with memory

### IPC Protocol
- Adjacently-tagged enums: `{"kind": {"type": "Command|Response|Event|Ping|Pong|Subscribe", "body": ...}}`
- 22 Command variants, 3 Response variants, 28 EventPayload variants
- Full round-trip serde verification across all message types
- Wire-format compatibility tested against bar's exact JSON construction
- Malformed-input handling: PARSE_ERROR responses

### Testing
- **71 tests** (was 3)
- IPC unit tests: 30+ round-trip tests covering every Command, Response, EventPayload variant
- Wire format tests: 7 tests matching bar's exact JSON construction
- Event tests: 25+ tests covering Action, Audio, Notification, Snapshot, Restore events
- Integration tests: 10 tokio-based tests with real IPC server (ping-pong, first-run, onboarding, context commands, demo data, snapshots, suggestion dismissal, malformed input, unavailable server)
- Regression guard: `test_no_flatten_in_kind` prevents Sprint 1 IPC format regression

### Bug Fixes
- **Critical:** IPC envelope format mismatch (Sprint 1) — `#[serde(flatten)]` removed from `kind` field
- **Critical:** Restoration response parsing — `get_response_body` now navigates `kind.body` correctly
- **Critical:** Ambient suggestion type discriminator — `payload.type` parsed at correct nesting level
- **Critical:** Snapshot field name mismatches — `id`→`snapshot_id`, `window_count`→`app_count`
- **Critical:** Preview action field name mismatches — `type`→`action_type`, `safe`→`requires_approval`
- **macOS:** EAGAIN/EWOULDBLOCK in integration tests — fixed by using tokio async UnixStream

### Known Issues
- GTK4 layer-shell requires compositor support (GNOME needs extension; Sway/Hyprland work natively)
- AI runtime requires manual `ollama serve` — not auto-started by enad yet
- Window tracking fallback chain not exhaustively verified on all compositors
- No Flatpak/AppImage packaging yet — manual build required
- Orchestration plans are in-memory only (not persisted across enad restarts)
- macOS: development-only mode, no desktop integration (no layer-shell, no D-Bus)
