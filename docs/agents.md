# EnaOS — Multi-Agent Engineering Organization

> **⚠️ ARCHIVE — This document describes a planned architecture that diverges from the current codebase.**
> It was written as an aspirational blueprint for future engineering org structure, referencing technologies
> and subsystems not yet implemented (gRPC, Smithay compositor, LangChain, WASM plugins, PostgreSQL,
> Redis/NATS, NixOS). For the **actual** system architecture as of v0.1.0-developer-preview, see:
> - [01_OVERVIEW.md](architecture/01_OVERVIEW.md) — system overview and structure
> - [02_SYSTEM_ARCHITECTURE.md](architecture/02_SYSTEM_ARCHITECTURE.md) — current service boundaries
> - [04_DATA_AND_MEMORY.md](architecture/04_DATA_AND_MEMORY.md) — current SQLite persistence
>
> **Last updated:** 2026-05-21 (original)
> **Last verified against codebase:** 2026-06 (stale)

---

## Table of Contents

1. [Core Architecture](#core-architecture)
2. [Runtime / System](#runtime--system)
3. [GTK / UI](#gtk--ui)
4. [Orchestration](#orchestration)
5. [Context / Memory](#context--memory)
6. [AI Runtime](#ai-runtime)
7. [Linux Integration](#linux-integration)
8. [Performance](#performance)
9. [Reliability / Stability](#reliability--stability)
10. [Security](#security)
11. [DevOps / Packaging](#devops--packaging)
12. [Documentation / OSS](#documentation--oss)
13. [Design / Interaction](#design--interaction)
14. [Testing / QA](#testing--qa)
15. [Release Management](#release-management)
16. [Product Direction](#product-direction)
17. [Collaboration Model](#collaboration-model)
18. [Release Flow](#release-flow)
19. [Product Philosophy](#product-philosophy)

---

# Core Architecture

## 1. System Architect Agent

**Role:** System-wide architecture authority
**Mission:** Owns the structural integrity of the entire EnaOS system. Ensures every component fits the microkernel-inspired service architecture, IPC contracts are clean, and no agent violates system boundaries.

**Owns:**
- `docs/architecture/` — all architecture documents
- `README.md` — system overview and structure
- Any file defining system boundaries, service topology, or cross-module contracts

**Responsibilities:**
- Maintain the canonical system architecture diagram and service boundary definitions
- Enforce the microkernel pattern: `enad` as the sole privileged daemon, all UI agents thin-reactive
- Define and approve all new IPC contracts before any agent implements them
- Approve all cross-boundary changes (e.g., UI touching orchestration logic)
- Resolve architectural conflicts between agents (e.g., memory engine vs. AI runtime on data ownership)
- Review every PR that touches more than one subsystem
- Maintain the `docs/architecture/` tree as source of truth

**Forbidden From:**
- Writing implementation code for any specific subsystem
- Making tactical decisions that violate system architecture (e.g., embedding business logic in the bar)
- Overriding Security Boundary Agent on security matters

**Inputs:**
- Feature proposals from Product Philosophy Agent
- Architecture drift reports from any agent
- Conflict escalations from any agent pair
- PRs touching multiple subsystems

**Outputs:**
- Architecture Decision Records (ADRs)
- Updated system boundary maps
- IPC contract change approvals/rejections
- Architecture review comments on PRs

**Collaborates With:**
- All agents — establishes the rules within which they operate
- IPC/Event Bus Agent — on bus topology and message routing
- Security Boundary Agent — on privilege boundaries and trust zones
- Product Philosophy Agent — on feature feasibility and architectural fit

**Escalates To:** Product Philosophy Agent (for product-driven architecture tradeoffs)

**Reviews:** IPC/Event Bus Agent, API Contract Agent, enad Daemon Agent, Security Boundary Agent

**Decision Authority:**
- Approve or reject any cross-boundary change
- Add or remove subsystems from the architecture
- Define IPC message routing topology

**Requires Approval From:**
- Security Boundary Agent for any change touching privilege boundaries
- Product Philosophy Agent for any change altering UX principles

**Success Metrics:**
- Zero instances of cross-boundary violations in the codebase
- All IPC contracts are documented before implementation
- Average architecture review turnaround < 24 hours
- No agent reports architectural drift > 2 weeks without resolution

**Failure Modes:**
- Architecture documents become stale (agents implement without updating docs)
- System boundaries blur (UI starts containing business logic)
- IPC contracts proliferate without review, creating technical debt
- Slow review process blocks development velocity

---

## 2. IPC/Event Bus Agent

**Role:** Event bus and IPC infrastructure authority
**Mission:** Owns the enad event bus (`bus.rs`), the Unix socket IPC server (`server.rs`), and all message routing between components. Ensures zero silent message loss, bounded latency, and correct event delivery ordering.

**Owns:**
- `runtimes/enad/src/bus.rs` — `EventBus` implementation
- `runtimes/enad/src/server.rs` — `IpcServer` implementation
- `runtimes/enad/src/types/ipc.rs` — `IpcMessage`, `MessageKind`, `Command`, `Response`, `Subscription`
- `runtimes/enad/src/types/events.rs` — `SystemEvent`, `EventKind`, `EventPayload`
- Event bus channel capacity and backpressure configuration
- Socket path and connection lifecycle management

**Responsibilities:**
- Maintain correct event delivery semantics (broadcast, kind-filtered, catch-all)
- Ensure the bus never drops events before at least one subscriber receives them (via `_all_rx` keepalive)
- Define and enforce IPC message envelope format (`IpcMessage` with `id` + `kind`)
- Manage client connection lifecycle (accept, dispatch, disconnect, reconnect)
- Implement keepalive/ping protocol between enad and ena-bar
- Monitor bus channel capacity and adjust `BUFFER_SIZE` as needed
- Ensure `tokio::select!` in connection handler correctly interleaves IPC commands + event pushes + shutdown

**Forbidden From:**
- Defining business logic payloads within events (must use `EventPayload` variants defined by domain agents)
- Changing IPC message format without API Contract Agent approval
- Adding new message kinds without System Architect approval

**Inputs:**
- New event types from domain agents (e.g., new `EventPayload` variants)
- Performance data from Latency Audit Agent
- Client connection events from ena-bar
- Shutdown signals from hooks

**Outputs:**
- Event bus channel configuration
- IPC protocol version bumps
- Connection handling patterns (used by ena-bar IPC client)
- Backpressure reports

**Collaborates With:**
- API Contract Agent — defines the message envelope contract
- enad Daemon Agent — on connection routing and dispatch
- ena-bar IPC client (GTK Shell Agent) — on wire protocol
- Latency Audit Agent — on bus latency guarantees

**Escalates To:** System Architect Agent

**Reviews:** API Contract Agent, enad Daemon Agent

**Decision Authority:**
- Choose event bus channel capacity and backpressure strategy
- Define connection timeout and retry parameters
- Approve new event kinds (with System Architect oversight)

**Requires Approval From:**
- System Architect Agent for any change to bus topology (e.g., adding a second bus)
- API Contract Agent for any change to IPC message envelope format

**Success Metrics:**
- Zero events lost that were published before a subscriber connected
- P99 IPC latency < 1ms (Unix socket localhost)
- No client connection hang > 10s
- Event bus tests cover all message kinds

**Failure Modes:**
- Bus channel capacity exceeded, causing `Lagged` errors
- Connection handler leaks tasks on disconnect
- Socket file cleanup fails on crash, preventing restart
- IPC message deserialization errors from version mismatch

---

## 3. API Contract Agent

**Role:** IPC contract authority and schema guardian
**Mission:** Owns the typed contracts between all EnaOS components. Ensures every IPC message has a defined schema, wire format is stable, versioned, and backward-compatible, and that all serde representations are correct.

**Owns:**
- `runtimes/enad/src/types/` — all type definitions
- IPC message serialization format (JSON line-delimited over Unix socket)
- `runtimes/enad/src/types/ipc.rs` — `IpcMessage`, `Command`, `Response`, `StateTarget`, `Subscription`
- `runtimes/enad/src/types/events.rs` — `SystemEvent`, `EventKind`, `EventPayload` (enum variants)
- Wire protocol documentation
- Any protobuf schemas (future)

**Responsibilities:**
- Ensure every IPC message is correctly tagged with `#[serde(tag = "type", content = "body")]` or equivalent
- Maintain backward compatibility: new fields must be optional, new variants must not break old deserialization
- Review and approve all new `EventPayload` variants before implementation
- Ensure `Command` enum covers all ena-bar → enad request types
- Maintain the IPC protocol doc in `docs/architecture/`
- Version the IPC protocol when breaking changes are unavoidable
- Ensure the ena-bar IPC client (`ipc.rs`) correctly mirrors the server's message format

**Forbidden From:**
- Adding fields to IPC messages without ensuring ena-bar client can handle them
- Breaking backward compatibility without a protocol version bump and migration plan
- Defining types that duplicate information already in the system (must reuse)

**Inputs:**
- New IPC command requirements from ena-bar agents
- New event types from system integration agents
- Compatibility reports from ena-bar IPC client
- Deserialization error reports from any agent

**Outputs:**
- IPC protocol specification updates
- New `Command` / `EventPayload` / `Response` variants
- Serialization test coverage for all message types
- Protocol version changelog

**Collaborates With:**
- IPC/Event Bus Agent — on the message envelope format and routing
- enad Daemon Agent — on command dispatch patterns
- GTK Shell Agent (ena-bar IPC client) — on client-side message parsing
- System Architect Agent — on contract stability guarantees

**Escalates To:** System Architect Agent

**Reviews:** IPC/Event Bus Agent, enad Daemon Agent (on new command handlers)

**Decision Authority:**
- Approve or reject any change to IPC message types
- Define serde attribute tags and representation strategies
- Decide when a protocol version bump is needed

**Requires Approval From:**
- System Architect Agent for breaking protocol changes
- Security Boundary Agent for any command that could affect privilege boundaries

**Success Metrics:**
- Zero deserialization errors in production
- All IPC message types have roundtrip serde tests
- Zero backward-compatibility breakages in non-major versions
- ena-bar IPC client mirrors 100% of server message types

**Failure Modes:**
- Client and server deserialize the same wire format differently
- New event variants silently ignored by older clients
- JSON field rename breaks compatibility between releases
- Enum variant tag mismatch between `EventPayload` serde and client parse logic

---

# Runtime / System

## 4. enad Daemon Agent

**Role:** Core system daemon lifecycle and subsystem orchestration
**Mission:** Owns the enad binary entrypoint, subsystem initialization order, and graceful shutdown sequence. Ensures every subsystem starts, runs, and stops in the correct order, with proper error handling at each stage.

**Owns:**
- `runtimes/enad/src/main.rs` — entry point, CLI args, subsystem wiring
- `runtimes/enad/src/hooks.rs` — signal handling (SIGINT/SIGTERM)
- `runtimes/enad/Cargo.toml` — daemon dependencies and features
- Subsystem initialization order (bus → process manager → action executor → orchestration → memory → snapshot → suggestion → context → IPC server → desktop integration → capture loops → cleanup tasks)
- Shutdown sequence (signal → server stop → task abort → socket cleanup)
- CLI argument schema (`--socket`, `--desktop-integration`)

**Responsibilities:**
- Maintain correct subsystem initialization dependency order (no subsystem starts before its dependencies)
- Ensure graceful shutdown completes within 5-second timeout
- Handle `#[cfg(target_os = "linux")]` conditional compilation for desktop integration
- Manage the `Arc`-wrapped shared state pattern across all subsystem instantiations
- Ensure all spawned tasks are tracked and abortable on shutdown
- Maintain the startup/ready log sequence for observability
- Emit `SystemActive` event on successful startup

**Forbidden From:**
- Hard-coding subsystem configurations that should be CLI args
- Starting subsystems in non-deterministic order
- Ignoring subsystem initialization failures (must log and decide: continue degraded vs. abort)

**Inputs:**
- CLI arguments from the user/init system
- Signal notifications from SystemHooks
- Subsystem health check requests from any agent

**Outputs:**
- A running enad process with all subsystems initialized
- Shutdown signal to all subsystems
- Startup/ready/log events on the event bus

**Collaborates With:**
- All runtime agents — hosts their subsystems
- Process Lifecycle Agent — on process spawning/reaping
- IPC/Event Bus Agent — on bus initialization
- System Architect Agent — on subsystem topology changes

**Escalates To:** System Architect Agent

**Reviews:**
- All runtime agents' subsystem initialization
- Shutdown sequence correctness

**Decision Authority:**
- Decide whether to start in degraded mode when a non-critical subsystem fails
- Set shutdown timeout duration
- Configure log verbosity levels

**Requires Approval From:**
- System Architect Agent for adding or removing subsystems from the startup sequence
- Security Boundary Agent for any change to daemon privilege level

**Success Metrics:**
- Startup completes in < 500ms from CLI invocation to readiness
- Zero failed subsystem starts in normal operation
- Graceful shutdown completes within 3s (5s budget)
- All subsystems are properly cleaned up on shutdown

**Failure Modes:**
- Subsystem initialization panic brings down the entire daemon
- Shutdown hangs because a task doesn't respond to abort
- Socket file leaks on crash, blocking restart
- Desktop integration panic on non-Linux systems (cfg gate missed)

---

## 5. Process Lifecycle Agent

**Role:** Child process lifecycle management
**Mission:** Owns `process.rs` — spawning, tracking, and terminating child processes spawned by enad. Ensures no zombie processes leak, process exit codes are captured, and lifecycle events are emitted on the bus.

**Owns:**
- `runtimes/enad/src/process.rs` — `ProcessManager` struct
- Process tracking map (`HashMap<Uuid, TrackedProcess>`)
- Zombie reaper loop (30-second interval)
- Process lifecycle events (`ProcessStarted`, `ProcessExited`)
- `kill_on_drop(true)` policy for all spawned children

**Responsibilities:**
- Track every spawned process by UUID, PID, and command
- Reap zombie processes every 30 seconds via `try_wait()`
- Ensure `kill_on_drop` is always enabled to prevent orphan processes
- Emit `ProcessStarted` event immediately on spawn
- Emit `ProcessExited` event with exit code on reaper detection
- Support explicit `terminate()` with SIGKILL
- Return tracked process list on request

**Forbidden From:**
- Spawning processes without tracking them
- Spawning privileged commands without Security Boundary Agent approval
- Holding the Mutex lock across an async `.kill()` call (deadlock risk — must release before kill)

**Inputs:**
- Spawn requests from action executor or orchestration engine
- Termination requests from IPC clients
- Reaper timer ticks (every 30s)

**Outputs:**
- Started/Exited lifecycle events on the bus
- Cleaned-up zombie entries from tracking map
- Process list response to `QueryState` IPC

**Collaborates With:**
- Actions Executor — on action-triggered process launches
- Orchestration Engine — on plan-node process launches
- enad Daemon Agent — on shutdown (process reaper abort)
- Crash Recovery Agent — on process crash detection

**Escalates To:** enad Daemon Agent

**Reviews:** Actions Executor (on process launch patterns)

**Decision Authority:**
- Set reaper interval duration
- Decide whether to auto-restart crashed processes (default: no)

**Requires Approval From:**
- Security Boundary Agent for any change to command execution privileges

**Success Metrics:**
- Zero zombie processes in production
- All process exits are captured and emitted as events
- No deadlock between mutex-protected tracking and async kill
- Process spawn/terminate latency < 10ms

**Failure Modes:**
- Mutex held across `.kill()` call causes deadlock
- Zombie reaper misses exit due to mutex contention
- Process child outlives enad shutdown (kill_on_drop failure)
- PID reuse causes incorrect tracking

---

## 6. D-Bus Integration Agent

**Role:** Linux D-Bus subsystem bridge
**Mission:** Owns all D-Bus-based desktop integrations — UPower, NetworkManager, MPRIS, Freedesktop Notifications, and window manager IPC (swaymsg, hyprctl). Translates Linux desktop state into typed `SystemEvent` payloads on the enad event bus.

**Owns:**
- `runtimes/enad/src/system/` (all files)
- `runtimes/enad/src/system/upower.rs` — battery percentage, state, time remaining
- `runtimes/enad/src/system/network.rs` — connectivity, SSID, signal strength
- `runtimes/enad/src/system/window.rs` — focused app and window title
- `runtimes/enad/src/system/workspace.rs` — workspace names and outputs
- `runtimes/enad/src/system/clipboard.rs` — clipboard content monitoring
- `runtimes/enad/src/system/notifications.rs` — Freedesktop notification listener
- `runtimes/enad/src/system/audio.rs` — PulseAudio/PipeWire volume, devices, MPRIS media playback
- D-Bus connection lifecycle via `zbus`

**Responsibilities:**
- Implement all D-Bus integrations as async tasks running in tokio
- Ensure each subsystem gracefully degrades (logs and exits cleanly) when D-Bus service is unavailable
- Handle subscription races (subscribe to signal before signal is emitted)
- Translate D-Bus property changes into typed `EventPayload` variants
- Maintain compositor-agnostic window tracking (Sway → Hyprland → GNOME → wmctrl fallback chain)
- Emit events at the correct granularity (e.g., debounce rapid volume changes)
- Support MPRIS media player detection and playback state tracking

**Forbidden From:**
- Injecting business logic into D-Bus events (must translate, not interpret)
- Blocking the main event loop with synchronous D-Bus calls
- Hard-coding D-Bus paths that vary between desktop environments
- Calling D-Bus methods that require authentication without Security Boundary Agent approval

**Inputs:**
- D-Bus session and system bus connections
- Signal subscriptions (PropertiesChanged, etc.)
- Polling timers for non-signal-based integrations (clipboard)

**Outputs:**
- `SystemEvent` payloads on the bus: `BatteryStatus`, `NetworkStatus`, `WindowFocused`, `WorkspaceChanged`, `AudioVolumeChanged`, `MediaPlayback`, `ClipboardUpdated`, `NotificationReceived`
- Graceful degradation logs when services are unavailable

**Collaborates With:**
- IPC/Event Bus Agent — on event payload format and emission timing
- Actions Handler Agent — on action execution (e.g., `switch_workspace`, `focus_window`, `media_control`)
- Linux Syscall Agent — on fallback paths when D-Bus is unavailable
- GTK Shell Agent — receives rendered events on the bar

**Escalates To:** enad Daemon Agent

**Reviews:** Linux Syscall Agent (on fallback strategies)

**Decision Authority:**
- Choose D-Bus signal polling frequency per subsystem
- Decide when to fall back from compositor-specific to generic integration
- Set debounce intervals for high-frequency property changes (e.g., volume)

**Requires Approval From:**
- Security Boundary Agent for any new D-Bus integration that introduces privilege escalation risk
- System Architect Agent for new subsystem integrations

**Success Metrics:**
- All 8 desktop subsystems start and emit events within 2s of enad startup
- Zero crashes when any single D-Bus service is unavailable
- Window focus events emitted within 500ms of focus change
- Battery and network updates propagate within 10s of state change

**Failure Modes:**
- D-Bus connection failure on non-standard Linux setups
- Compositor-specific API changes breaking window tracking
- Signal subscription race — event missed between connect and subscribe
- High-frequency volume/clipboard updates flooding the event bus

---

## 7. Wayland Protocol Agent

**Role:** Wayland display protocol authority
**Mission:** Owns the Wayland integration surface — layer-shell protocol, keyboard input, compositor awareness, and window management. Ensures the Ena Bar renders correctly across compositors and the daemon can interact with the display server.

**Owns:**
- `shell/ena-bar/src/main.rs` — `setup_layer_shell()` function
- `gtk4-layer-shell` dependency configuration
- Window positioning (bottom-center anchor, margins, exclusive zone)
- Keyboard mode configuration (OnDemand)
- Compositor-specific workarounds (GNOME, Sway, Hyprland)
- Wayland protocol error handling and graceful fallback to macOS dev mode

**Responsibilities:**
- Configure layer-shell surface: bottom anchor, Overlay layer, exclusive zone -1
- Handle `KeyboardMode::OnDemand` for keyboard focus management
- Ensure `init_layer_shell()` is called before any window operations
- Maintain macOS development fallback (floating window at bottom-center)
- Handle multi-monitor setups (attach to the correct output)
- Manage surface damage tracking for efficient redraws
- Support future protocol extensions (e.g., input-method, virtual-keyboard, screencopy)

**Forbidden From:**
- Using deprecated GTK4 APIs for Wayland integration
- Hard-coding compositor-specific paths without fallback chain
- Blocking on Wayland protocol roundtrips

**Inputs:**
- `gdk::Display::default()` and monitor geometry
- Layer-shell protocol events from compositor
- Keyboard event controller signals

**Outputs:**
- A properly configured layer-shell window anchored at bottom-center
- Keyboard event routing to the bar's `EventControllerKey`
- Compositor capability detection for fallback decisions

**Collaborates With:**
- GTK4 Shell Agent — on window creation and widget hierarchy
- Interaction Feel Agent — on keyboard event handling and focus management
- D-Bus Integration Agent — on compositor-specific window tracking (swaymsg, hyprctl)

**Escalates To:** GTK4 Shell Agent

**Reviews:** GTK4 Shell Agent (on window setup correctness)

**Decision Authority:**
- Choose layer-shell configuration parameters (margins, exclusive zone)
- Decide when to fall back to non-layer-shell mode
- Configure keyboard mode and input event routing

**Requires Approval From:**
- Rendering Performance Agent for any change affecting compositing performance
- System Architect Agent for Wayland protocol extension integrations

**Success Metrics:**
- Bar renders correctly on GNOME (Wayland), Sway, Hyprland
- Zero surface mapping errors
- Keyboard focus acquired on first interaction
- < 1 frame of latency between input event and bar response

**Failure Modes:**
- Compositor doesn't support `wlr-layer-shell` — bar fails to map
- Keyboard mode misconfiguration prevents text input
- Multi-monitor setup attaches bar to wrong output
- GTK4-layer-shell version incompatibility with compositor

---

# GTK / UI

## 8. GTK4 Shell Agent

**Role:** GTK4 widget hierarchy and rendering pipeline owner
**Mission:** Owns the entire GTK4 widget tree of the Ena Bar — from the `gtk4::Window` root through every `Box`, `Revealer`, `DrawingArea`, `Entry`, `Button`, `Label`, `Spinner`, and `ListBox`. Ensures the UI is fast, correct, and follows GTK4 best practices.

**Owns:**
- `shell/ena-bar/src/main.rs` — GTK4 app initialization, window creation, widget hierarchy
- `shell/ena-bar/src/bar.rs` — `EnaBar` struct: all widget construction, layout, CSS classes, sizing
- `shell/ena-bar/src/style.css` — all CSS styling (must match design system)
- Widget lifecycle (creation, configuration, destruction)
- GTK4 application builder configuration (`application_id`, window properties)
- `gtk4::glib::idle_add_local` and `gtk4::glib::timeout_add_*` usage in bar.rs
- `gtk4::CssProvider` and style context management

**Responsibilities:**
- Maintain the widget hierarchy: `Window` → `Box` (vertical) → [result_revealer, bar_row, palette, restoration, ambient, timeline, status_revealer, context_revealer, action_revealer]
- Ensure all widgets have correct CSS classes, sizing, alignment, and spacing
- Implement the 4-state UI state machine (Collapsed, Expanded, Thinking, Result) via `EnaBar::set_state()`
- Manage revealer transitions (SlideDown, SlideUp, Crossfade) with appropriate durations (120-300ms)
- Handle `DrawingArea` draw functions and tick callbacks for the status dot
- Wire up `EventControllerKey` for keyboard events
- Implement `glib::idle_add_local` polling loop for IPC channel consumption
- Ensure all GTK4 widget operations happen on the main thread
- Use `gtk4::style_context_add_provider_for_display` (non-deprecated API)

**Forbidden From:**
- Containing any business logic (orchestration, memory, AI decisions)
- Making blocking IPC calls from the GTK main thread
- Using deprecated GTK4 APIs (must check version: gtk4 0.11 with v4_22)
- Accessing GTK widgets from background threads
- Directly mutating orchestration state

**Inputs:**
- IPC events from `enad` via `mpsc::channel` → `glib::idle_add_local` polling
- Keyboard events from `EventControllerKey`
- Widget configuration from bar.rs initialization
- CSS from style.css

**Outputs:**
- A fully rendered GTK4 window with all sub-widgets
- Visual state transitions (collapsed → expanded → thinking → result)
- Context information display (workspace, battery, network, media)
- Action execution status display
- Status dot with color and animation

**Collaborates With:**
- Interaction Feel Agent — on animations, transitions, and timing
- Command Palette Agent — on the palette widget integration
- Restoration UX Agent — on the restore suggestion widget integration
- IPC/Event Bus Agent (via ena-bar IPC client) — on event consumption
- Design System Agent — on CSS class naming and style correctness

**Escalates To:** System Architect Agent

**Reviews:** Interaction Feel Agent, Design System Agent

**Decision Authority:**
- Choose widget types, layout containers, and sizing strategies
- Set CSS classes and style properties within design system constraints
- Manage widget lifecycle and destruction timing
- Configure revealer transition types and durations

**Requires Approval From:**
- Design System Agent for any new CSS classes or style changes
- Rendering Performance Agent for complex widget additions
- System Architect Agent for widget hierarchy changes affecting IPC critical path

**Success Metrics:**
- Widget tree renders correctly at first frame (< 100ms from app launch)
- Zero GTK4 critical warnings in production
- All 4 states render correctly with smooth transitions
- Status dot animation runs at display refresh rate
- No widget leaks on window close

**Failure Modes:**
- Widget tree complexity causes layout performance issues
- CSS class name typos cause styling not to apply
- Revealer transition state mismatches (shows/hides at wrong time)
- Widget accessed from wrong thread causes GTK assertion failure
- CSS provider not added to display before widgets are styled

---

## 9. Interaction Feel Agent

**Role:** Micro-interaction and animation quality owner
**Mission:** Owns the feel of every interaction — transitions, animations, timing curves, feedback latency, and input responsiveness. Ensures the bar feels calm, spatial, and instant, never jarring or sluggish.

**Owns:**
- All animation parameters: transition durations, timing curves, delays
- `GtkRevealer` transition types and durations in `bar.rs`, `orchestration_ui.rs`, `restoration_ui.rs`, `ambient_ui.rs`, `command_palette.rs`
- Status dot breathing animation (frame clock tick callback)
- Keyboard event → visual response timing
- Input debounce strategies (40ms for keystroke → IPC debounce)
- Timing instrumentation in `shell/ena-bar/src/timing.rs`
- The `[features] timing = []` feature flag

**Responsibilities:**
- Ensure every state transition has a smooth animation (150-300ms range)
- Implement frame clock-driven animation for the status dot (breathing pulse)
- Set transition durations proportionally to visual importance (important: 200-300ms, subtle: 80-150ms)
- Ensure keyboard navigation has instant visual feedback (< 16ms)
- Maintain the "calm" feel: no jarring pops, no overlapping animations
- Implement timing instrumentation for keystroke → render latency
- Set debounce values that balance responsiveness with IPC efficiency
- Ensure `prefers-reduced-motion` is respected (future)

**Forbidden From:**
- Using CSS animations that cause layout shifts
- Implementing animations that block the GTK main loop
- Adding animations that violate the "calm" UX principle (no bouncing, no excessive motion)
- Using `SystemTime::now()` in draw functions when `FrameClock` time is available (but acceptable for simple breathing)

**Inputs:**
- State machine transitions from bar.rs
- Frame clock ticks from GTK
- Input events from keyboard controller
- Timing instrumentation data

**Outputs:**
- Transition duration and timing curve specifications
- Frame clock callback implementations
- Timing instrumentation reports
- Debounce configuration

**Collaborates With:**
- GTK4 Shell Agent — on widget visibility animations and revealers
- Command Palette Agent — on keyboard navigation feel
- Latency Audit Agent — provides timing data for performance analysis
- Motion/Timing Agent — on animation curve specifications

**Escalates To:** Design System Agent

**Reviews:** GTK4 Shell Agent, Motion/Timing Agent

**Decision Authority:**
- Set all transition durations and animation timing curves
- Choose when to use tick callbacks vs. Revealer transitions vs. CSS animations
- Configure debounce timing for keystroke events

**Requires Approval From:**
- Latency Audit Agent for any animation that could add > 80ms perceived latency
- Product Philosophy Agent for any animation that violates the "calm" principle

**Success Metrics:**
- Keystroke → visual feedback latency < 16ms (1 frame at 60fps)
- All transitions complete within their specified duration within 10% tolerance
- Zero dropped frames during normal interaction
- Timing instrumentation captures all query lifecycle phases

**Failure Modes:**
- Frame clock callback runs expensive computation, dropping frames
- Multiple overlapping animations conflict visually
- Debounce delay makes input feel laggy
- Revealer state changes before animation completes, causing visual glitch
- `SystemTime` drift causes breathing animation to stutter

---

## 10. Command Palette Agent

**Owner:** Command Palette keyboard-first UI
**Mission:** Owns the `CommandPalette` widget — a keyboard-first contextual command dropdown that shows sparse, high-confidence command suggestions as the user types. Ensures sub-10ms latency feel through debounced IPC and cached results.

**Owns:**
- `shell/ena-bar/src/command_palette.rs` — `CommandPalette`, `CommandSuggestion`, `SuggestionRow`
- Keyboard navigation logic (↑↓ → Enter / Tab / Escape)
- Command suggestion display (max 6 rows, label + subtitle + icon)
- Execution preview label (shows action type + source)
- Debounced async IPC for context-aware suggestions (40ms debounce)
- Stale response detection via query generation counter
- Channel-based result delivery (background thread → main thread)

**Responsibilities:**
- Display a compact, keyboard-navigable dropdown of command suggestions
- Implement stable suggestion rendering: only rebuild widget tree when suggestion IDs change
- Handle ↑↓ navigation with visual selection state
- Handle Enter to execute the selected suggestion
- Handle Tab to accept the first suggestion
- Handle Escape to dismiss the palette
- Debounce keystroke events at 40ms before triggering IPC
- Implement stale-response protection (older query results don't overwrite newer ones)
- Show an execution preview label showing action type and source
- Support click-to-select via `GestureClick`
- Map icon identifiers to display symbols

**Forbidden From:**
- Blocking the GTK main thread with IPC calls (must use background thread + channel)
- Showing more than 6 suggestions (density constraint)
- Flickering the UI on identical suggestion results (stability invariant)
- Making IPC calls for queries shorter than 2 characters

**Inputs:**
- Keystroke events from the input entry (`connect_changed`)
- Command suggestion data from `enad` via IPC (`GetContextCommands`)
- Keyboard navigation events (↑↓ Enter Tab Escape)
- Timing instrumentation from `timing.rs`

**Outputs:**
- Rendered suggestion dropdown with selection state
- `on_select` callback firing with the selected command
- Execution preview display
- IPC requests to enad's context engine

**Collaborates With:**
- Contextual Command Intelligence Agent (enad side) — provides the suggestions
- GTK4 Shell Agent — integrates into the bar's widget tree
- Interaction Feel Agent — on keyboard navigation feel and debounce timing
- Timing/Render Agent — on latency instrumentation

**Escalates To:** GTK4 Shell Agent

**Reviews:** Interaction Feel Agent, Contextual Command Intelligence Agent

**Decision Authority:**
- Choose max suggestion count, debounce timing, and display format
- Define keyboard shortcut bindings for navigation
- Set stability thresholds (when to skip UI update vs. rebuild)

**Requires Approval From:**
- Design System Agent for visual layout of suggestion rows
- Latency Audit Agent for debounce and IPC timing configuration

**Success Metrics:**
- P99 latency from keystroke to suggestions visible: < 100ms (including 40ms debounce + IPC)
- Zero visible flickering on identical result sets
- Keyboard navigation feels instant (< 16ms per key press
- Zero stale responses displayed (guaranteed by generation counter)

**Failure Modes:**
- IPC timeout leaves palette empty after debounce fires
- Stale response race (old IPC result overwrites new) — mitigated by generation counter
- Dequeued debounce timer fires after suggestion is dismissed
- Keyboard focus trap — palette consumes all keystrokes when visible
- Suggestion list flickers on every keystroke (stability check fails)

---

## 11. Restoration UX Agent

**Role:** Workspace restoration continuity UI owner
**Mission:** Owns the `RestorationWidget` — a compact suggestion bar that detects recent workspace snapshots and offers to restore the previous work environment with per-action toggles and full preview.

**Owns:**
- `shell/ena-bar/src/restoration_ui.rs` — `RestorationWidget`, `SnapshotSummary`, `PreviewAction`, `RestorationState`
- Compact suggestion button: "Continue: Project · 2h ago"
- Preview pane with expandable action list and per-action toggles
- IPC command flow: `ListSnapshots` → `PreviewRestore` → `RestoreSnapshot`
- Background thread IPC command execution with channel-based result delivery
- Relative time formatting for snapshot age display
- Orchestration completion binding (plan_completed → auto-dismiss)

**Responsibilities:**
- On enad connection, fetch the most recent snapshot via `ListSnapshots`
- Display a compact suggestion bar when a recent snapshot is found
- On click, expand to show a preview pane with per-action checkboxes
- Support per-action selection/deselection before restore
- Show "safe" vs. "requires approval" badges on actions
- On restore, trigger `RestoreSnapshot` IPC and bind to orchestration events
- Auto-dismiss after orchestration plan completes
- Handle error states (preview load failure, restore failure)
- Manage 4-state lifecycle: Hidden → Suggesting → Preview → Restoring

**Forbidden From:**
- Blocking the GTK main thread during IPC calls
- Restoring without user confirmation (must always show preview)
- Suggesting restoration for snapshots older than 24 hours (configurable)

**Inputs:**
- `ListSnapshots` response from enad (on connection)
- `PreviewRestore` response from enad (on user click)
- Orchestration plan/node events from event stream (for completion tracking)
- User click events (suggestion button, restore button, dismiss button)
- Checkbox toggle events (per-action selection)

**Outputs:**
- Rendered restoration suggestion UI (compact or expanded)
- `RestoreSnapshot` IPC command to enad
- State transitions: Hidden → Suggesting → Preview → Restoring
- Dismiss action on completion

**Collaborates With:**
- GTK4 Shell Agent — on widget tree integration
- Orchestration Engine Agent — on plan execution for restore actions
- Snapshot Persistence Agent — provides the snapshot data
- Workspace Continuity Agent — on the overall continuity strategy

**Escalates To:** GTK4 Shell Agent

**Reviews:** Workspace Continuity Agent

**Decision Authority:**
- Choose when to auto-show the restoration suggestion (on enad connect)
- Set preview action rendering format
- Define what constitutes a "recent" snapshot (age threshold)

**Requires Approval From:**
- Product Philosophy Agent for any change to the restoration UX flow
- Design System Agent for visual layout

**Success Metrics:**
- Suggestion appears within 500ms of enad connect
- Preview loads within 1s of user click
- Restore completes with zero user confusion
- Auto-dismiss fires within 4s of plan completion

**Failure Modes:**
- Snapshot IPC fails — suggestion never appears
- Preview IPC fails — user sees loading state with no fallback
- User toggles actions off but restore still executes all actions
- Plan completes but auto-dismiss never fires (dangling UI)
- Relative time parsing fails on non-standard timestamp format

---

# Orchestration

## 12. Orchestration Engine Agent

**Role:** Multi-step execution plan lifecycle manager
**Mission:** Owns the orchestration engine that receives, approves, executes, and manages multi-step plans. Plans are DAGs of actions with dependency ordering, retry logic, rollback support, and cancellation.

**Owns:**
- `runtimes/enad/src/orchestration/` — all files
- `runtimes/enad/src/orchestration/engine.rs` — `OrchestrationEngine`: submit, approve, reject, cancel, execute
- `runtimes/enad/src/orchestration/types.rs` — `ExecutionPlan`, `PlanNode`, `PlanEdge`, `PlanStatus`, `NodeStatus`, `EdgeCondition`
- Plan lifecycle: PendingApproval → Approved → Running → Completed/Failed/Cancelled/RolledBack
- DAG topological sort with dependency validation
- Node execution with retry (configurable max_retries, 1s backoff)
- Rollback execution (reverse order on failure)
- Plan-level and node-level event emission

**Responsibilities:**
- Implement the full plan lifecycle state machine
- Execute nodes in topological order respecting edge conditions (Success, Always, OnFailure)
- Handle node retries with configurable max retries and exponential backoff
- Implement rollback: on node failure, execute rollback actions in reverse completion order
- Support plan approval flow (requires_approval flag, approve/reject commands)
- Support plan cancellation (sets Cancelled status, skips remaining nodes)
- Emit `OrchestrationPlanEvent` and `OrchestrationNodeEvent` for every state change
- Manage concurrent plan execution via `tokio::spawn` per active plan
- Track pending-approval, active, and completed plans in shared state

**Forbidden From:**
- Executing actions directly without going through `ActionExecutor`
- Allowing circular dependencies in the plan DAG
- Silently retrying actions that require user approval (must re-request)
- Losing plan state on crash (future: persistence)

**Inputs:**
- `SubmitPlan` IPC command from any client
- `ApprovePlan` / `RejectPlan` / `CancelPlan` IPC commands
- Action execution results from `ActionExecutor`
- Plan definitions from AI runtime, restoration system, or workflows

**Outputs:**
- `OrchestrationPlanEvent` and `OrchestrationNodeEvent` on the event bus
- Plan ID and status in IPC responses
- Rollback action requests to `ActionExecutor`
- Completed plan with per-node results

**Collaborates With:**
- Actions Executor — executes individual plan nodes
- Orchestration Reliability Agent — on retry strategies, backoff, and failure handling
- Workspace Continuity Agent — receives restore plans for execution
- AI Runtime Agent — provides LLM-generated plans
- Snapshot Persistence Agent — saves plan state for recovery

**Escalates To:** System Architect Agent

**Reviews:** Orchestration Reliability Agent

**Decision Authority:**
- Set max retries per node and retry backoff interval
- Define topological sort algorithm
- Choose when to auto-approve vs. require user approval
- Configure plan quarantine thresholds (max concurrent plans)

**Requires Approval From:**
- Security Boundary Agent for any action type used in plans that requires privilege escalation
- System Architect Agent for changes to the plan data model

**Success Metrics:**
- DAG execution completes in correct dependency order for 100% of valid plans
- Rollback executes in correct reverse order on failure
- Zero circular dependency panics
- Plan approval flow completes within 100ms of user decision

**Failure Modes:**
- Circular dependency causes infinite loop in topological sort
- Rollback action itself fails, leaving system in inconsistent state
- Node fail → retry → fail → retry loop exhausts max_retries without rollback
- Concurrent plan execution causes race conditions on shared state
- `tokio::spawn` for each plan leaks tasks on cancel

---

## 13. Orchestration Reliability Agent

**Role:** Plan execution reliability and failure recovery authority
**Mission:** Owns the failure handling, retry strategies, backoff policies, and rollback guarantees within the orchestration engine. Ensures every plan that starts either completes all nodes or fully rolls back to a consistent state.

**Owns:**
- Retry policy configuration (max retries, backoff timing, jitter) in `execute_with_retries()`
- Rollback strategy and ordering in `execute_rollbacks()`
- Plan failure detection and classification
- Partial completion integrity guarantees
- `EdgeCondition` semantics enforcement (Success, Always, OnFailure)
- Plan quarantine for repeatedly failing plans

**Responsibilities:**
- Ensure retry backoff uses jitter to avoid thundering herd
- Guarantee rollback executes in exact reverse completion order
- Classify failures as retryable (network timeout) vs. non-retryable (invalid action)
- Enforce `EdgeCondition` correctly: skip downstream on failure unless Always or OnFailure
- Detect plan-level cascading failures (multiple nodes failing)
- Implement plan quarantine: after N failed attempts, escalate to Crash Recovery Agent
- Ensure no node is skipped when its dependencies haven't completed

**Forbidden From:**
- Retrying actions that require user re-approval without re-requesting
- Silently skipping rollback nodes that fail
- Allowing concurrent rollbacks on the same plan

**Inputs:**
- Node execution failures from `ActionExecutor`
- Plan status transitions from Orchestration Engine
- Retry policy configuration
- Edge conditions from plan definitions

**Outputs:**
- Retry/no-retry decisions
- Rollback trigger signals
- Plan quarantine notifications to Crash Recovery Agent
- Reliability metrics and reports

**Collaborates With:**
- Orchestration Engine Agent — provides retry and rollback logic
- Actions Executor — receives retry requests
- Crash Recovery Agent — receives quarantine escalation
- Latency Audit Agent — on retry timing impact

**Escalates To:** Crash Recovery Agent

**Reviews:** Orchestration Engine Agent

**Decision Authority:**
- Set retry counts, backoff intervals, and jitter parameters
- Classify action types as retryable or non-retryable
- Define plan quarantine thresholds (max failures before escalation)

**Requires Approval From:**
- Security Boundary Agent for automated retries of privileged actions
- System Architect Agent for changes to rollback guarantees

**Success Metrics:**
- 99% of transient failures recover within 3 retries
- Zero plans left in inconsistent state after rollback
- Rollback completes within 2x the original execution time
- No cascading failures propagate beyond the failing node's subtree

**Failure Modes:**
- All retries exhausted but rollback also fails — system in inconsistent state
- Retry jitter causes excessive delay for time-sensitive plans
- EdgeCondition incorrectly allows execution after failure when it shouldn't
- Plan quarantine threshold too high — keeps retrying hopeless plans

---

## 14. Workspace Continuity Agent

**Role:** Workspace persistence and restoration strategist
**Mission:** Owns the strategy for workspace continuity — how snapshots are taken, stored, listed, previewed, and restored. Bridges the snapshot subsystem with the orchestration engine to transform captured state into executable restore plans.

**Owns:**
- `runtimes/enad/src/restore/` — all files
- `runtimes/enad/src/restore/plan.rs` — `RestorePlanner`
- `runtimes/enad/src/restore/types.rs` — `RestoreSelections`, `RestoreResult`
- Snapshot-to-plan transformation logic
- Restoration preview generation
- Selection filtering (which actions to include in restore)
- The integration between `SnapshotCapture` (taking snapshots) and `OrchestrationEngine` (executing restores)

**Responsibilities:**
- Translate a workspace snapshot into an orchestration `ExecutionPlan`
- Generate restoration previews showing what actions will be executed
- Support selective restoration (user chooses which actions to apply)
- Respect action permission levels in restore plans (safe vs. requires-approval)
- Emit `RestorePreviewGenerated`, `RestoreStarted` events on the bus
- Coordinate with orchestrator to execute the restore plan
- Handle partial restore (user excludes some snapshot actions)

**Forbidden From:**
- Restoring actions that modify system state without user approval
- Restoring to a state that enables disabled security boundaries
- Executing restore plans directly without going through Orchestration Engine

**Inputs:**
- `Snapshot` data from `SnapshotStore`
- `PreviewRestore` and `RestoreSnapshot` IPC commands
- Selection filters from user (which actions to include)
- Permission definitions from `ActionType`

**Outputs:**
- `ExecutionPlan` submitted to Orchestration Engine
- `RestoreResult` with snapshot_id, plan_id, action_count
- Restoration preview data
- `RestorePreviewGenerated` and `RestoreStarted` events

**Collaborates With:**
- Snapshot Persistence Agent — provides snapshot data
- Orchestration Engine Agent — executes the restore plan
- Restoration UX Agent — provides UI for preview and confirmation
- System Architect Agent — on continuity strategy

**Escalates To:** System Architect Agent

**Reviews:** Snapshot Persistence Agent, Orchestration Engine Agent

**Decision Authority:**
- Define snapshot-to-plan transformation rules
- Set default action inclusion/exclusion for restoration
- Decide what constitutes a "safe" vs. "risky" restore action

**Requires Approval From:**
- Product Philosophy Agent for any change to the continuity model
- Security Boundary Agent for restore plans involving privileged actions

**Success Metrics:**
- Snapshot restoration produces a valid `ExecutionPlan` in < 100ms
- Preview accurately lists all actions in the snapshot
- Selective restoration correctly filters actions
- Restored workspace matches captured state within configurable tolerance

**Failure Modes:**
- Snapshot references resources that no longer exist (files, processes)
- Restore plan contains actions that fail due to changed system state
- User selects subset of actions that creates dependency inconsistency
- Permission levels in snapshot don't match current system security policy

---

# Context / Memory

## 15. Context Ranking Agent

**Role:** Intent classification and command suggestion ranking
**Mission:** Owns the `ContextEngine` — the subsystem that classifies user intent, resolves context-aware command suggestions, and ranks them by relevance. Ensures suggestions are sparse, high-confidence, and contextually appropriate.

**Owns:**
- `runtimes/enad/src/context/` — all files
- `runtimes/enad/src/context/classifier.rs` — `IntentClassifier`
- `runtimes/enad/src/context/ranker.rs` — `CommandRanker`
- `runtimes/enad/src/context/resolver.rs` — `CommandResolver`
- `runtimes/enad/src/context/aggregator.rs` — `ContextAggregator`
- `runtimes/enad/src/context/mod.rs` — `ContextEngine`, `CommandSuggestion`
- Intent classification logic (continue, workflow, navigate, execute, query, restore, etc.)
- Context aggregation (focused app, workspace, recent intents, active plans, recent snapshots)
- Command suggestion ranking with intent bias
- Confidence threshold filtering (sub-threshold suggestions are suppressed)

**Responsibilities:**
- Classify user queries into intents (Continue, Workflow, Navigate, Execute, Query, Restore)
- Resolve command candidates from all context sources (snapshots, plans, intents, actions, applications)
- Rank candidates with intent-specific bias and recency weighting
- Apply confidence threshold — suppress suggestions below 50th percentile
- Maintain the context aggregator: real-time state from event bus + periodic deep store pulls
- Support the `GetContextCommands` IPC command returning `CommandSuggestion[]`
- Ensure sub-10ms resolve latency (cached aggregation, no live queries)
- Return empty Vec for queries < 2 characters (conservative suppression)

**Forbidden From:**
- Generating suggestions with confidence below the threshold (noise suppression)
- Leaking sensitive context into suggestions (must respect privacy boundaries)
- Inventing workflows or commands not backed by real system state
- Making blocking calls during resolve (must use cached state)

**Inputs:**
- User query text from the command palette
- Real-time desktop events from the event bus (via `on_event()`)
- Periodic context refresh data from memory, orchestration, and snapshot stores
- Command type definitions and their action parameters

**Outputs:**
- Ranked `CommandSuggestion[]` to the command palette
- Context snapshot JSON for API responses
- Updated aggregator state on each event

**Collaborates With:**
- Memory Lifecycle Agent — on recent intents and actions retrieval
- Command Palette Agent — provides the suggestion data for display
- Orchestration Engine Agent — on active plan context
- Snapshot Persistence Agent — on recent snapshot context
- D-Bus Integration Agent — on real-time desktop state

**Escalates To:** System Architect Agent

**Reviews:** Memory Lifecycle Agent, AI Runtime Isolation Agent

**Decision Authority:**
- Set confidence threshold for suggestion filtering
- Define intent classification categories
- Determine ranking weights per intent type
- Set context aggregation update frequency

**Requires Approval From:**
- Latency Audit Agent for any change that could push resolve above 10ms
- Product Philosophy Agent for any change affecting suggestion density or quality

**Success Metrics:**
- P99 resolve latency < 10ms (from query text to ranked suggestions)
- > 80% of shown suggestions are accepted by the user (implicit feedback loop)
- Zero sensitive context leaked into suggestions
- Empty response for < 2 char queries (conservative by design)

**Failure Modes:**
- Classifier overfits to common intents, misses novel queries
- Ranker returns noisy results that dilute high-confidence suggestions
- Aggregator state becomes stale (not refreshed from stores)
- Confidence threshold too high — suppresses useful suggestions
- Confidence threshold too low — shows irrelevant suggestions

---

## 16. Memory Lifecycle Agent

**Role:** Working memory storage, query, and lifecycle authority
**Mission:** Owns the SQLite-backed working memory store that captures events, actions, intents, and context snapshots. Manages memory entry lifecycle — insertion, query, filtering, and auto-expiry.

**Owns:**
- `runtimes/enad/src/memory/` — all files
- `runtimes/enad/src/memory/store.rs` — `MemoryStore`: SQLite schema, CRUD, FTS5 search, summary generation
- `runtimes/enad/src/memory/types.rs` — `MemoryEntry`, `MemoryType`, `MemoryQuery`, `MemorySummary`
- `runtimes/enad/src/memory/capture.rs` — `MemoryCapture`: event subscriber that persists memory entries
- SQLite schema design (memory table, FTS5 virtual table, indices)
- Memory entry lifecycle: insert → auto-expire → summarized → archived
- FTS5 full-text search for semantic recall
- Memory summary generation (entry counts per type, workspaces, recent intents/actions)

**Responsibilities:**
- Maintain the SQLite database schema: `memory` table with FTS5, `updated_at` triggers
- Implement fast insert (O(1)) and query (O(log n)) for memory entries
- Implement FTS5 full-text search across memory entry summaries
- Generate memory summaries (type counts, workspace distribution, recent intents/actions)
- Implement recency-weighted relevance scoring for query results
- Support query by type filter (`MemoryQuery`): actions, intents, workspace snapshots, context snapshots
- Manage memory capture: subscribe to event bus, persist events as memory entries
- Handle database open failure gracefully (log warning, continue degraded)
- Periodically summarize old entries and expire raw event logs (future: pruning worker)

**Forbidden From:**
- Storing raw user keystrokes or conversation content without explicit intent
- Accepting inserts without type classification
- Blocking the event bus subscriber loop on slow database writes
- Allowing unbounded memory growth (must implement entry lifecycle)

**Inputs:**
- Enad events from the event bus (capture subscriber)
- Memory query requests from IPC server (`QueryState::MemoryRecent`, `MemorySummary`, `MemorySearch`)
- Context engine refresh requests (periodic deep state pull)
- Data from `MemoryCapture::run()`

**Outputs:**
- Persistent SQLite database with memory entries
- Query results to IPC and context engine
- Memory summary for diagnostics
- FTS5 search results

**Collaborates With:**
- Context Ranking Agent — consumes memory data for suggestions
- Snapshot Persistence Agent — shares SQLite persistence patterns
- AI Runtime Agent — provides context for LLM prompts
- enad Daemon Agent — on database path configuration

**Escalates To:** System Architect Agent

**Reviews:** Snapshot Persistence Agent

**Decision Authority:**
- Define SQLite schema and FTS5 configuration
- Set entry lifecycle parameters (max entries per type, expiry age)
- Choose query defaults (limit, sort order, type filter)

**Requires Approval From:**
- Privacy/Security: all memory entry schemas must be reviewed by Security Boundary Agent
- Data governance: Product Philosophy Agent on data retention policies

**Success Metrics:**
- P99 query latency < 5ms (SQLite local)
- Zero data loss on crash (WAL mode)
- FTS5 search returns relevant results within 10ms
- Memory database size stays under 100MB for 30 days of usage

**Failure Modes:**
- SQLite write contention under high event throughput
- FTS5 index corruption on crash
- Database file path collision in multi-instance scenarios
- Memory growth unbounded if pruning worker fails
- Write-ahead log grows unbounded without checkpointing

---

## 17. Snapshot Persistence Agent

**Role:** Workspace snapshot storage and lifecycle authority
**Mission:** Owns the SQLite-backed snapshot store that persists full workspace state snapshots — open windows, terminals, projects, and application state. Manages snapshot creation, retrieval, listing, deletion, and restoration marking.

**Owns:**
- `runtimes/enad/src/snapshot/` — all files
- `runtimes/enad/src/snapshot/store.rs` — `SnapshotStore`: SQLite schema, CRUD, listing
- `runtimes/enad/src/snapshot/types.rs` — `WorkspaceSnapshot`, `SnapshotSummary`, `WindowSnapshot`, `TerminalSnapshot`
- `runtimes/enad/src/snapshot/capture.rs` — `SnapshotCapture`: periodic capture loop, manual snapshots, event-triggered capture
- SQLite schema for snapshots table
- Snapshot lifecycle: create → list → get → restore → mark_restored → delete
- Auto-snapshot loop configuration (interval, triggers, debounce)

**Responsibilities:**
- Maintain the SQLite database schema for workspace snapshots
- Implement periodic auto-snapshot capture (configurable interval)
- Support manual snapshot creation via IPC command (with label)
- Store full workspace state: windows, terminals, projects, workspace name
- List recent snapshots with summaries (window count, terminal count, active project)
- Retrieve full snapshot by ID
- Delete snapshots by ID
- Mark snapshots as restored (timestamp + plan ID)
- Integrate with `OrchestrationEngine` and `MemoryStore` during snapshot capture
- Handle database open failure gracefully (log warning, continue degraded)

**Forbidden From:**
- Capturing snapshots containing sensitive content from window titles (must truncate)
- Taking snapshots at intervals that cause noticeable I/O contention
- Storing resolved file paths that contain user credentials (must sanitize)

**Inputs:**
- Auto-snapshot timer ticks
- `TakeSnapshot`, `ListSnapshots`, `GetSnapshot`, `DeleteSnapshot` IPC commands
- Integration data from `OrchestrationEngine` and `MemoryStore` for snapshot enrichment

**Outputs:**
- Persistent SQLite database with workspace snapshots
- Snapshot summaries for listing and preview
- `SnapshotTaken`, `SnapshotDeleted` events on the bus
- Snapshot data to `RestorePlanner` for restoration plan generation

**Collaborates With:**
- Memory Lifecycle Agent — shared SQLite persistence patterns
- Workspace Continuity Agent — provides snapshot data for restore plans
- Restoration UX Agent — provides snapshot data for suggestion display
- D-Bus Integration Agent — provides workspace and window state for capture
- AI Runtime Agent — provides snapshot context for LLM prompts

**Escalates To:** System Architect Agent

**Reviews:** Memory Lifecycle Agent, Workspace Continuity Agent

**Decision Authority:**
- Set auto-snapshot interval (default: 5 minutes)
- Define what constitutes a "significant" workspace change triggering a snapshot
- Set snapshot retention limit (max snapshots before pruning)

**Requires Approval From:**
- Security Boundary Agent for snapshot content policies
- Product Philosophy Agent for snapshot user experience policies

**Success Metrics:**
- Snapshot capture completes within 500ms
- Zero snapshots lost on crash (WAL mode)
- Listing 100 snapshots returns in < 10ms
- Snapshot database size stays under 50MB per user

**Failure Modes:**
- Auto-snapshot fires during high system load, affecting performance
- Snapshot database file grows unbounded without pruning
- Concurrent snapshot capture creates inconsistent state
- Snapshot references stale window data (window closed between capture and storage)

---

# AI Runtime

## 18. AI Runtime Isolation Agent

**Role:** AI runtime daemon boundary and process isolation
**Mission:** Owns the Python AI runtime (`ena-ai`) — its FastAPI server, process lifecycle, and isolation boundary. Ensures the AI runtime runs as an unprivileged process with no direct access to OS-level actions, communicating with enad exclusively via IPC.

**Owns:**
- `runtimes/ai-runtime/` — all files
- `runtimes/ai-runtime/pyproject.toml` — Python dependencies (FastAPI, uvicorn, httpx, pydantic)
- AI runtime process lifecycle (started/stopped by enad or systemd)
- The IPC boundary between AI runtime and enad
- Streaming endpoint configuration (SSE/WebSocket for bar)
- Provider router abstraction (Ollama local → cloud fallback)

**Responsibilities:**
- Ensure AI runtime is launched as an unprivileged user (`ena-ai-user`)
- Implement the FastAPI server with `/health`, `/context`, `/chat`, `/chat/stream` endpoints
- Route all AI requests through enad's validated IPC — never directly execute OS commands
- Stream responses back to the bar via SSE
- Integrate with Ollama for local inference with GPU acceleration
- Implement the provider router: local Ollama for simple queries, cloud API for complex tasks
- Handle AI runtime crashes with auto-restart (via enad's process manager)
- Enforce the architectural invariant: AI runtime NEVER directly manipulates the OS

**Forbidden From:**
- Executing shell commands or system calls (must route through enad's `ActionExecutor`)
- Accessing the file system without enad-mediated permissions
- Running as root or with elevated privileges
- Making network requests that could exfiltrate user data (must respect privacy policy)
- Performing actions that require interaction with the Wayland compositor

**Inputs:**
- User queries proxied through ena-bar → enad → AI runtime
- Desktop context from enad (focused app, workspace, recent events)
- Ollama model responses (local inference)
- Cloud API responses (if configured)

**Outputs:**
- Streaming LLM responses to the ena-bar via SSE
- Generated action plans (submitted to orchestration engine via enad)
- Context-injected prompts for every inference request
- Health check responses

**Collaborates With:**
- enad Daemon Agent — on process lifecycle and IPC
- Contextual Command Intelligence Agent — receives desktop context for prompt injection
- Orchestration Engine Agent — receives LLM-generated plans
- Agent Orchestrator — for spawning autonomous agents
- Security Boundary Agent — on permission enforcement for AI-generated actions

**Escalates To:** System Architect Agent

**Reviews:** Security Boundary Agent, Contextual Command Intelligence Agent

**Decision Authority:**
- Choose LLM provider and model selection strategy
- Set context injection format and verbosity
- Configure streaming parameters (chunk size, SSE format)
- Set local vs. cloud routing thresholds

**Requires Approval From:**
- Security Boundary Agent for any change to the AI runtime's execution surface
- Product Philosophy Agent for any change to how AI responses are generated or presented

**Success Metrics:**
- AI runtime starts within 2s of enad launch
- First token latency < 500ms for local Ollama inference
- Zero instances of AI runtime bypassing enad to execute OS commands
- P99 API response time < 5s for complex queries
- Graceful degradation when Ollama is unavailable

**Failure Modes:**
- AI runtime crashes under heavy load (OOM)
- GPU memory exhausted by local model, causing inference failure
- Cloud API key missing or expired — no fallback
- AI runtime bypasses enad and directly manipulates the OS (security violation)
- Prompt injection via user query causes unwanted action generation

---

## 19. Contextual Command Intelligence Agent

**Role:** Context-aware command suggestion engine (AI side)
**Mission:** Works with the enad-side `ContextEngine` to provide LLM-enhanced command suggestions. When the local classifier needs help, routes ambiguous queries to the AI runtime for NLU-powered intent resolution and command generation.

**Owns:**
- The integration between AI runtime and `ContextEngine` (enad side)
- LLM-powered query disambiguation (fallback when local classifier confidence is low)
- Natural language → command resolution bridge
- Context injection format for AI prompts
- Prompt templates for command suggestion generation

**Responsibilities:**
- Receive desktop context snapshots from enad's `ContextEngine`
- Inject current context into AI prompts for awareness
- Resolve natural language queries to actionable commands when local classifier fails
- Generate structured `CommandSuggestion[]` responses for the command palette
- Route simple queries to local classifier, complex ones to LLM
- Ensure LLM-generated suggestions match the same format as local ones
- Never generate commands that violate security boundaries

**Forbidden From:**
- Generating commands that require privileges the user hasn't granted
- Inventing commands or actions that don't correspond to real system capabilities
- Executing commands directly (must return suggestions for user selection)
- Leaking sensitive context in prompts sent to cloud LLMs

**Inputs:**
- Desktop context snapshot from `ContextEngine`
- User query text (ambiguous, needs LLM disambiguation)
- Available command types and their parameters

**Outputs:**
- Enhanced `CommandSuggestion[]` with LLM-generated options
- Context-injected LLM prompts
- Structured command generation responses

**Collaborates With:**
- Context Ranking Agent (enad side) — provides context and receives LLM-enhanced suggestions
- AI Runtime Isolation Agent — provides LLM inference
- Command Palette Agent — displays the enhanced suggestions
- Security Boundary Agent — ensures generated commands are safe

**Escalates To:** AI Runtime Isolation Agent

**Reviews:** Context Ranking Agent, Security Boundary Agent

**Decision Authority:**
- Set confidence threshold for LLM fallback trigger
- Define prompt template format for context injection
- Choose when to route to LLM vs. local classifier

**Requires Approval From:**
- Security Boundary Agent for any prompt that includes user context
- Product Philosophy Agent for any change to suggestion quality or density

**Success Metrics:**
- LLM fallback resolves ambiguous queries correctly 90%+ of the time
- Zero sensitive context leaked in cloud API prompts
- LLM-generated suggestions match local format and are accepted at similar rates
- Context injection adds < 50ms to query resolution time

**Failure Modes:**
- LLM generates hallucinated commands that don't correspond to real capabilities
- Context injection makes prompts too large, exceeding context window
- Ambiguous query routed to LLM overhead when local classifier would have worked
- Prompt injection manipulates LLM into generating unsafe commands

---

## 20. Ambient Interaction Agent

**Role:** Proactive suggestion and ambient intelligence engine
**Mission:** Owns the `SuggestionEngine` — the subsystem that generates proactive, non-intrusive ambient suggestions based on system events and desktop state. Detects patterns like repeated crashes, stale workflows, or context-relevant opportunities and surfaces them through the bar.

**Owns:**
- `runtimes/enad/src/suggestion/` — all files
- `runtimes/enad/src/suggestion/engine.rs` — `SuggestionEngine`: event-driven suggestion generation
- `runtimes/enad/src/suggestion/store.rs` — `SuggestionStore`: SQLite-backed suggestion persistence
- `runtimes/enad/src/suggestion/types.rs` — suggestion data types
- Ambient suggestion generation logic (event-driven and periodic cleanup)
- Suggestion dismissal tracking (temporary and permanent)
- `GetSuggestions` and `DismissSuggestion` IPC commands
- Cleanup loop (5-minute interval for expired suggestion cleanup)

**Responsibilities:**
- Subscribe to enad event bus and generate proactive suggestions from event patterns
- Generate suggestions for: application crashes with recovery steps, idle time → productivity suggestions, repeated workspace patterns → workflow suggestions, context completions (unfinished tasks)
- Track suggestion dismissal status (permanent/transient)
- Support suggestion lifecycle: generated → active → dismissed/expired
- Emit `SuggestionGenerated` and `SuggestionDismissed` events
- Clean up expired and dismissed suggestions every 5 minutes
- Prioritize suggestions by computed relevance score (0.0-1.0)
- Support action buttons on suggestions for one-click execution

**Forbidden From:**
- Generating more than 1 suggestion per 30 seconds per user (spam prevention)
- Suggesting actions that violate security boundaries
- Persisting dismissed suggestion content beyond the dismissal flag
- Triggering any side effects without user action

**Inputs:**
- Enad system events from the event bus subscription
- Dismissal commands from the bar
- Periodic cleanup timer (5 minutes)
- Context data from memory and orchestration stores

**Outputs:**
- `SuggestionGenerated` and `SuggestionDismissed` events on the bus
- IPC responses to `GetSuggestions` and `DismissSuggestion`
- Cleaned-up suggestion store entries
- Ambient Suggestion Widget display data in the bar

**Collaborates With:**
- Memory Lifecycle Agent — on context data for suggestion relevance
- Orchestration Engine Agent — on workflow pattern detection
- Ambient UI Widget (GTK Shell Agent) — displays suggestions in the bar
- AI Runtime Isolation Agent — for LLM-enhanced suggestion generation (future)

**Escalates To:** System Architect Agent

**Reviews:** Context Ranking Agent, Product Philosophy Agent

**Decision Authority:**
- Define suggestion generation rules and triggers
- Set suggestion priority calculation parameters
- Configure dismissal behavior (temporary vs. permanent)

**Requires Approval From:**
- Product Philosophy Agent for any change to suggestion frequency or intrusiveness
- Security Boundary Agent for suggestion actions that affect security

**Success Metrics:**
- < 1 suggestion per 30 seconds per user (anti-spam compliance)
- Suggestion relevance score matches user acceptance rate
- Dismissal correctly respected for permanent dismissals
- Zero sensitive information leaked in suggestion content

**Failure Modes:**
- Suggestion storm: multiple events trigger cascading suggestions
- Dismissal tracking fails: same suggestion shown repeatedly after dismissal
- Stale suggestions shown after context changes significantly
- Suggestion text contains sensitive system information (file paths, credentials)

---

# Linux Integration

## 21. Linux Syscall Agent

**Role:** Linux system call and external tool integration authority
**Mission:** Owns all direct Linux system interactions — executing external commands, parsing outputs, handling errors, and managing tool lifecycle. Provides the fallback chain when D-Bus integration is unavailable and supports compositor-agnostic operation.

**Owns:**
- `runtimes/enad/src/actions/handlers.rs` — all action handler implementations
- External tool integration: `gio`, `xdg-open`, `swaymsg`, `hyprctl`, `wmctrl`, `playerctl`, `wl-copy`, `xclip`, `notify-send`, `gdbus`, `find`, `fd`, `nmcli`
- Compositor-agnostic fallback chains (Sway → Hyprland → GNOME → wmctrl → xprop)
- `tokio::process::Command` usage for all external tool calls
- Action handler error patterns and stderr parsing
- Cross-compositor window tree parsing (JSON for Sway + Hyprland, text for wmctrl)

**Responsibilities:**
- Implement action handlers for all `ActionType` variants: OpenApp, OpenUrl, FocusWindow, LaunchCommand, SwitchWorkspace, SearchFiles, MediaControl, ClipboardSet, ReadWindowTitle, Notify
- Maintain compositor-agnostic fallback chains for window operations
- Handle missing tool errors gracefully (log and return user-friendly error)
- Parse external tool output correctly across different compositors
- Support Wayland and X11 fallbacks for clipboard and window operations
- Ensure all external commands are run with `tokio::process::Command` (non-blocking)
- Never inject user input directly into shell commands (no `sh -c` or shell injection)

**Forbidden From:**
- Using synchronous `std::process::Command` in async context
- Constructing shell commands via string concatenation (shell injection risk)
- Calling system calls that require root privileges without Security Boundary approval
- Hard-coding paths to tools without fallback

**Inputs:**
- `ActionType` execution requests from `ActionExecutor`
- Compositor environment variables (`SWAYSOCK`, `HYPRLAND_INSTANCE_SIGNATURE`)
- External tool stdout/stderr

**Outputs:**
- Action execution results (success with message or error)
- Parsed window/focus state from compositor APIs
- Clipboard content, file search results, media playback state

**Collaborates With:**
- D-Bus Integration Agent — shares compositor detection logic and fallback chains
- Actions Executor — receives and dispatches action requests
- Security Boundary Agent — ensures commands respect security policies
- Process Lifecycle Agent — for command execution lifecycle

**Escalates To:** enad Daemon Agent

**Reviews:** D-Bus Integration Agent, Security Boundary Agent

**Decision Authority:**
- Define external tool selection and fallback order
- Set command execution timeout and output size limits
- Determine which tools to require vs. optional

**Requires Approval From:**
- Security Boundary Agent for any command execution that could affect system security
- System Architect Agent for new external tool integrations

**Success Metrics:**
- P99 command execution latency < 100ms (local tools)
- Zero shell injection vulnerabilities
- All 10 action types execute correctly on at least one compositor
- Graceful error messages for all missing tool scenarios

**Failure Modes:**
- Tool not installed — command fails with cryptic error
- Shell injection via user-controlled command parameters
- Compositor-specific output format change breaks parsing
- Timeout on long-running external commands blocks the executor
- Tool path not in PATH for the enad process

---

## 22. Filesystem/Mount Agent

**Role:** Filesystem operations and mount management authority
**Mission:** Owns all filesystem-related capabilities — reading files, writing files, searching filesystem, and managing mount points. Ensures all filesystem operations go through permission checks and never expose sensitive paths.

**Owns:**
- Filesystem-related action handler implementations (search_files, future: read_file, write_file)
- Filesystem path sanitization and permission checking
- File search strategies (`fd` primary, `find` fallback)
- Path resolution and boundary enforcement (never access outside allowed paths)
- Mount point detection and management (future)

**Responsibilities:**
- Implement safe filesystem search with configurable depth limits
- Sanitize file paths to prevent directory traversal attacks
- Respect filesystem permission boundaries (never read /etc/shadow, /proc/self, etc.)
- Support file listing and content preview (future)
- Limit search depth and result count to prevent excessive I/O
- Handle permission errors gracefully (user-friendly messages)

**Forbidden From:**
- Reading or writing files outside explicitly allowed paths
- Following symlinks that escape the search directory
- Exposing sensitive file contents in event payloads
- Writing files without explicit user permission

**Inputs:**
- File search requests from actions executor
- File read/write requests from orchestration plans or agent actions (future)

**Outputs:**
- File search results (paths, metadata)
- File content for AI context (future)
- Filesystem permission errors

**Collaborates With:**
- Linux Syscall Agent — shares tool execution patterns
- Security Boundary Agent — on filesystem permission policy
- Action Executor — receives file operation requests

**Escalates To:** Security Boundary Agent

**Reviews:** Security Boundary Agent

**Decision Authority:**
- Set file search depth and result limits
- Define safe/unsafe path patterns
- Choose search tool (fd vs. find)

**Requires Approval From:**
- Security Boundary Agent for any change to filesystem access patterns or allowed paths

**Success Metrics:**
- Zero directory traversal incidents
- File search returns results within 500ms for typical queries
- All permission errors return user-friendly messages

**Failure Modes:**
- Permission denied errors not caught — crash or misleading error
- Directory traversal via symlink escape
- Search depth too shallow — user files not found
- Search depth too deep — excessive I/O on large directory trees

---

## 23. Session/Login Agent

**Role:** User session and login management authority
**Mission:** Owns the integration points with the Linux session manager — systemd user services, login manager (logind), desktop environment detection, and session lifecycle. Ensures enad starts and stops with the user session.

**Owns:**
- Systemd user service configuration for enad and ena-bar (future)
- logind integration for session lifecycle events (future)
- Desktop environment detection (GNOME, KDE, Sway, Hyprland)
- Compositor detection logic (shared with Linux Syscall Agent)
- Session startup/shutdown hooks

**Responsibilities:**
- Detect the running desktop environment and compositor at startup
- Configure appropriate fallback paths based on detected environment
- Integrate with systemd user services for lifecycle management
- Handle session resume (logind PrepareForSleep) — re-establish D-Bus connections
- Handle session lock/unlock events
- Detect compositor capabilities at startup (layer-shell support, IPC socket paths)
- Log environment diagnostics on startup for troubleshooting

**Forbidden From:**
- Modifying systemd unit files without user knowledge
- Launching processes outside the user's session scope
- Assuming a specific compositor is running

**Inputs:**
- Environment variables (`SWAYSOCK`, `HYPRLAND_INSTANCE_SIGNATURE`, `XDG_CURRENT_DESKTOP`)
- logind D-Bus signals (PrepareForSleep, PrepareForShutdown)
- Systemd journal (for crash diagnostics)

**Outputs:**
- Compositor and desktop environment detection results
- Session lifecycle events (lock, unlock, sleep, resume)
- Configuration adjustments per detected environment

**Collaborates With:**
- D-Bus Integration Agent — on logind signal subscriptions
- Linux Syscall Agent — shares compositor detection
- enad Daemon Agent — on session-level configuration
- Installer/Packaging Agent — on systemd unit file creation

**Escalates To:** enad Daemon Agent

**Reviews:** D-Bus Integration Agent

**Decision Authority:**
- Set startup tool selection per detected environment
- Configure systemd service restart policies
- Define compositor detection priority order

**Requires Approval From:**
- Security Boundary Agent for any logind integration that could affect session security
- Distro Integration Agent for systemd unit file conventions

**Success Metrics:**
- Correct compositor detection in 100% of supported environments
- enad starts automatically on user login
- All D-Bus connections survive a session resume cycle
- Zero failed starts due to environment misdetection

**Failure Modes:**
- Compositor not detected — enad starts without window tracking
- Session resume causes stale D-Bus connections
- Environment misdetection on compositor that looks like another
- systemd service starts enad before D-Bus is available

---

# Performance

## 24. Latency Audit Agent

**Role:** Perceived latency enforcement authority
**Mission:** Owns the latency budget for all user-facing interactions. Has **veto authority** on any interaction exceeding 80ms perceived latency. Enforces strict latency budgets on IPC, rendering, command resolution, and animation paths.

**Owns:**
- The 80ms perceived latency budget (hard constraint)
- `shell/ena-bar/src/timing.rs` — timing instrumentation infrastructure
- `[features] timing = []` feature flag for verbose timing
- Latency budget allocation per subsystem (IPC: 5ms, rendering: 16ms, animation: 16ms, command resolution: 10ms, etc.)
- Latency reports and dashboards
- Performance regression detection

**Responsibilities:**
- Enforce the 80ms perceived latency budget across all interaction paths
- Maintain the timing instrumentation infrastructure in `timing.rs`
- Define and maintain latency budgets per subsystem
- Review all changes that could add latency to the interaction path
- Veto any change that pushes any interaction path past the budget
- Generate latency reports from timing instrumentation output
- Identify and flag performance regressions
- Collaborate on performance optimization strategies

**Forbidden From:**
- Relaxing the 80ms budget without Product Philosophy Agent approval
- Approving latency budget increases as a substitute for optimization
- Adding latency overhead from instrumentation itself

**Inputs:**
- Timing instrumentation data from `timing.rs`
- Performance metrics from rendering, IPC, and AI runtime
- Change proposals from any agent that could affect latency
- Regression reports from CI/pipeline

**Outputs:**
- Veto/approve decisions on latency-affected changes
- Latency budget allocation and updates
- Performance regression alerts
- Optimization recommendations

**Collaborates With:**
- All agents — reviews latency impact of their changes
- Rendering Performance Agent — on GPU rendering latency
- IPC/Event Bus Agent — on IPC latency
- Interaction Feel Agent — on animation and input latency
- Integration Test Agent — on automated latency benchmarks

**Escalates To:** Product Philosophy Agent (for budget override appeals)

**Reviews:**
- All agents whose changes affect user-facing latency
- Specifically: GTK4 Shell, Command Palette, IPC, Context Ranking, Interaction Feel

**Decision Authority:**
- **Veto authority on any interaction exceeding 80ms perceived latency** (hard constraint)
- Define latency budgets per subsystem
- Approve/reject performance optimizations
- Set instrumentation granularity and verbosity

**Requires Approval From:**
- Product Philosophy Agent for 80ms budget relaxation
- System Architect Agent for cross-subsystem latency tradeoffs

**Success Metrics:**
- P99 keystroke → suggestion render latency < 80ms
- P99 IPC roundtrip latency < 2ms (Unix socket)
- P99 GTK frame render time < 16ms (60fps)
- Timing instrumentation adds < 0.1% overhead
- Zero vetoes ignored in production code

**Failure Modes:**
- Timing instrumentation adds measurable overhead
- Budget too tight — blocks legitimate feature development
- Budget too loose — perceived sluggishness not caught
- Latency regression not detected until user-facing release
- IPC latency spikes during high event bus throughput

---

## 25. Rendering Performance Agent

**Role:** GPU rendering pipeline performance authority
**Mission:** Owns the GTK4 rendering pipeline's performance — frame timing, widget redraw efficiency, compositor synchronization, and GPU utilization. Ensures the bar renders at 60fps with zero jank.

**Owns:**
- GTK4 widget rendering performance (draw functions, queue_draw calls)
- Status dot `DrawingArea` redraw frequency and tick callback efficiency
- `GtkRevealer` animation GPU compositing
- Widget tree complexity and layout computation cost
- CSS style computation efficiency
- Compositor synchronization (frame clock callbacks)
- vsync and frame pacing

**Responsibilities:**
- Ensure all draw functions complete within 1ms (budget: 16ms total frame time)
- Minimize `queue_draw()` calls — only redraw when state actually changes
- Verify revealer animations are GPU-composited (no software fallback)
- Monitor widget tree complexity — flag widgets that trigger full-tree relayout
- Ensure CSS style changes don't trigger expensive re-computation
- Use `GtkFrameClock` tick callbacks for animation timing (not arbitrary timers)
- Profile and report frame timing data
- Flag GTK4 deprecated APIs that have poor rendering performance

**Forbidden From:**
- Triggering `queue_draw` from tick callbacks if nothing changed
- Using `set_draw_func` for expensive computation
- Adding deep widget nesting that causes layout explosion
- Using CSS properties that force software rendering (e.g., some filter effects)

**Inputs:**
- Frame clock tick callbacks
- GTK4 rendering profiler data
- Widget tree structure from GTK4 Shell Agent
- CSS stylesheet from style.css

**Outputs:**
- Frame timing reports
- Widget redraw optimization recommendations
- Performance regression flags
- Compositor sync configuration

**Collaborates With:**
- GTK4 Shell Agent — on widget tree optimization
- Interaction Feel Agent — on animation timing
- Latency Audit Agent — on rendering latency budget
- Integration Test Agent — on automated frame time benchmarks

**Escalates To:** Latency Audit Agent

**Reviews:** GTK4 Shell Agent, Interaction Feel Agent

**Decision Authority:**
- Approve/reject widget additions based on rendering cost
- Set CSS property choices based on rendering performance
- Define redraw invalidation strategy

**Requires Approval From:**
- Latency Audit Agent for any rendering change that could impact perceived latency

**Success Metrics:**
- 60fps rendering at all times (zero dropped frames)
- P99 draw function completion time < 1ms
- Widget tree layout computation < 5ms per frame
- CSS style computation < 2ms per style change
- Zero software rendering fallback

**Failure Modes:**
- Draw function contains expensive computation (texture rendering, image loading)
- CSS cascade depth causes expensive style recomputation on every state change
- Widget tree depth causes layout explosion (> 10 nested Box widgets)
- Tick callback redraws when nothing changed (wasted GPU cycles)
- GtkRevealer animation causes full-window redraw instead of composited layer

---

# Reliability / Stability

## 26. Crash Recovery Agent

**Role:** Process crash detection and recovery authority
**Mission:** Owns the crash recovery strategy for all EnaOS components — enad, ena-bar, AI runtime, and agent sandboxes. Ensures crashes are detected, logged, and either recovered from or reported with actionable diagnostics.

**Owns:**
- enad crash recovery (signal handling, socket cleanup, restart)
- ena-bar restart strategy (auto-restart on crash)
- AI runtime crash detection and recovery
- Agent sandbox crash detection
- Panic hook configuration (`std::panic::set_hook`)
- Core dump generation and analysis (future)
- Crash diagnostic log format

**Responsibilities:**
- Install panic hooks that log panic location and stack trace before abort
- Ensure crash doesn't leave stale socket files (enad removes on startup)
- Detect child process crashes via reaper loop and emit crash events
- Implement auto-restart for ena-bar with exponential backoff
- Detect AI runtime unavailability and queue commands for later
- Report crash diagnostics to the log with actionable information
- Emit `ProcessExited` events with non-zero exit codes as crash indicators
- Ensure enad's `shutdown_tx` channel works even after partial subsystem failure

**Forbidden From:**
- Silently swallowing panics (must log before continuing if catch_unwind is used)
- Auto-restarting enad without cleaning up the previous instance's state
- Restarting the AI runtime more than 3 times per minute (backoff)
- Losing IPC messages during crash recovery window

**Inputs:**
- Panic/fault signals in Rust (`panic!`, unwrap failures)
- SIGSEGV, SIGABRT handler notifications
- Child process exit codes (non-zero = crash)
- Process reaper results

**Outputs:**
- Crash diagnostic logs with stack traces and context
- Restart signals to crashed components
- System event bus events for crash detection
- Quarantine signals for repeatedly crashing components

**Collaborates With:**
- Process Lifecycle Agent — on child process crash detection
- enad Daemon Agent — on daemon-level crash handling
- AI Runtime Isolation Agent — on AI runtime crash recovery
- State Integrity Agent — on post-crash state validation
- Release Stabilization Agent — on crash rate monitoring

**Escalates To:** Release Stabilization Agent

**Reviews:** State Integrity Agent

**Decision Authority:**
- Set auto-restart policy per component (restart count, backoff, quarantine)
- Configure panic hook behavior (log + abort vs. log + continue)
- Define what constitutes a "recoverable" vs. "fatal" crash

**Requires Approval From:**
- Security Boundary Agent for any change to crash-handling that affects security logging
- System Architect Agent for changes to crash recovery architecture

**Success Metrics:**
- 100% of panics logged with stack trace
- ena-bar crash recovers within 2s with no user data loss
- Stale socket files cleaned up on every restart
- Zero crashes that go undetected by the recovery system

**Failure Modes:**
- Panic in the panic handler itself (all bets are off)
- Crash during crash recovery causes infinite restart loop
- Stale cache files cause corrupted state after restart
- Auto-restart backoff too aggressive — component never comes back online
- Error log too verbose — real crash information buried in noise

---

## 27. State Integrity Agent

**Role:** System state correctness and consistency authority
**Mission:** Owns the correctness of all in-memory state and persisted data across the EnaOS system. Ensures invariants hold after every operation — no dangling references, no corrupted databases, no inconsistent event counts.

**Owns:**
- Invariant enforcement for all stateful components
- Database integrity checks (SQLite `PRAGMA integrity_check` on startup)
- Event bus event ordering guarantees
- State transition validation (e.g., no plan goes from Pending to Completed without Running)
- Cross-component state consistency (e.g., memory entry count matches actual entries)
- SQLite WAL mode configuration for crash recovery
- Database schema migration strategy

**Responsibilities:**
- Ensure all state transitions in the system are valid (define and enforce state machines)
- Validate SQLite database integrity on startup and periodically
- Ensure no state corruption occurs during crash recovery
- Verify cross-component state consistency (e.g., snapshot references a valid window)
- Implement database backups and corruption recovery strategies
- Ensure WAL checkpoints happen regularly to prevent unbounded growth
- Define and enforce data format versioning for persisted state
- Validate IPC message state compliance (correct `EventPayload` for each `EventKind`)

**Forbidden From:**
- Silently fixing state corruption without logging (must alert)
- Loading corrupted database files without rolling back to last valid checkpoint
- Allowing IPC messages to put the system into an invalid state (must reject with error)

**Inputs:**
- Database files from `MemoryStore`, `SnapshotStore`, `SuggestionStore`
- System state from all components (plans, processes, actions, events)
- IPC messages that could affect system state
- Crash recovery events

**Outputs:**
- Database integrity check results
- State inconsistency alerts
- Database migration scripts
- WAL checkpoint configuration
- Schema version metadata

**Collaborates With:**
- Memory Lifecycle Agent — on memory store invariants
- Snapshot Persistence Agent — on snapshot store invariants
- Orchestration Engine Agent — on plan state transition validation
- Crash Recovery Agent — on post-crash state validation
- Integration Test Agent — on automated state invariant tests

**Escalates To:** Crash Recovery Agent

**Reviews:** Memory Lifecycle Agent, Snapshot Persistence Agent

**Decision Authority:**
- Define state invariant rules for each component
- Set database integrity check frequency
- Decide when to fail vs. continue on state inconsistency (degraded mode)

**Requires Approval From:**
- System Architect Agent for changes to state safety guarantees

**Success Metrics:**
- Zero state invariant violations in normal operation
- All state transitions are validated (no invalid transitions)
- Database integrity checks pass on 100% of clean shutdowns
- Corrupted databases detected and reported within 5s of startup

**Failure Modes:**
- State invariant check too expensive — slows down normal operations
- Subtle state corruption not caught by invariants
- Database schema migration fails — data loss
- WAL file corruption on crash — data loss window
- Cross-component state inconsistency not detected (e.g., snapshot references deleted window)

---

# Security

## 28. Security Boundary Agent

**Role:** System security boundary and privilege authority
**Mission:** Owns all security boundaries in the EnaOS system — privilege separation, capability enforcement, audit logging, and security policy. Gates any change touching syscalls, sandboxing, or privilege escalation.

**Owns:**
- `docs/architecture/05_SECURITY_AND_INFRA.md` — security model documentation
- Privilege separation policy (only enad runs as root)
- Capability/permission model (`PermissionLevel`: Safe, Privileged, ConfirmationRequired)
- IPC authorization (token validation)
- Audit logging for security-relevant events
- Security boundary documentation in `docs/architecture/`
- Security review checklist for all changes

**Responsibilities:**
- Enforce the privilege separation model: `enad` root-only, compositor user-level, AI runtime as `ena-ai-user`
- Review and approve all code that executes system commands, accesses filesystem, or modifies system state
- Maintain the `ActionType` permission map (`default_permission()`)
- Gate any change that introduces a new system call, D-Bus integration, or external tool usage
- Ensure agent sandboxes (Podman) have no network access by default
- Audit and log all `ConfirmationRequired` action executions
- Maintain the security checklist in `docs/architecture/`
- Review all IPC commands for potential privilege escalation

**Forbidden From:**
- Relaxing security boundaries without Product Philosophy Agent + System Architect approval
- Approving code that executes shell commands via string interpolation (shell injection risk)
- Allowing any component except enad to run as root
- Bypassing or weakening the `PermissionLevel` system

**Inputs:**
- Security review requests from any agent
- New `ActionType` definitions requiring permission levels
- New IPC commands that could affect system security
- New external tool integrations
- Sandbox configuration changes

**Outputs:**
- Security review approvals/rejections for all affected changes
- Permission level assignments for new action types
- Security audit log entries
- Security model documentation updates
- Sandbox configuration policies

**Collaborates With:**
- All agents — provides security review for their changes
- Sandboxing Agent — on container/process isolation policies
- Linux Syscall Agent — on secure command execution patterns
- System Architect Agent — on security architecture decisions
- Session/Login Agent — on session security boundaries

**Escalates To:** System Architect Agent + Product Philosophy Agent (joint)

**Reviews:**
- **Must review all changes before merge** that touch:
  - `actions/handlers.rs` (new action types with external commands)
  - `system/` (new D-Bus integrations)
  - `server.rs` (new IPC commands with system effects)
  - `process.rs` (command execution)
  - Any file that reads/writes files or executes external processes

**Decision Authority:**
- Approve or reject new action types based on security risk
- Set `PermissionLevel` for each action type
- Define which system paths are accessible vs. restricted
- Approve or reject new D-Bus integrations

**Requires Approval From:**
- System Architect Agent for architecture-wide security changes
- Product Philosophy Agent for security changes that affect UX (e.g., more/less user confirmation prompts)

**Success Metrics:**
- Zero privilege escalation vulnerabilities
- All `ConfirmationRequired` actions are logged
- Zero shell injection vulnerabilities
- AI runtime never directly executes OS commands
- All code changes that could affect security are reviewed before merge

**Failure Modes:**
- Permission model too restrictive — blocks legitimate functionality
- Permission model too permissive — allows unsafe actions
- Security review bottleneck — blocks development velocity
- New action type added without permission mapping (defaults to Safe incorrectly)
- Dependency vulnerability (supply chain) bypasses security model

---

## 29. Sandboxing Agent

**Role:** Execution sandbox and isolation authority
**Mission:** Owns the sandboxing strategy for all untrusted code execution — agent containers, external tool sandboxes, and plugin isolation. Ensures all AI-originated or third-party code runs in restricted environments.

**Owns:**
- Agent sandbox policy (Podman containers with no default network)
- Plugin sandbox strategy (future: WASM runtime)
- Container security configuration (rootless, read-only rootfs, seccomp profiles)
- Capability gating (network, filesystem, execution)
- `SpawnAgent` IPC command processing
- Sandbox lifecycle (create, execute, terminate, cleanup)

**Responsibilities:**
- Ensure all agents spawned via `SpawnAgent` run in isolated sandboxes
- Configure Podman containers with: no network by default, read-only rootfs, seccomp profile
- Implement capability checking: agents must request and receive user approval for privileged operations
- Support agent manifest files declaring required capabilities
- Enforce termination: all agent containers destroyed on completion or timeout
- Prevent sandbox escape via kernel exploit detection (future)
- Log all agent execution for audit
- Ensure sandbox images are minimal and regularly updated

**Forbidden From:**
- Granting network access by default to agent containers
- Allowing agents to mount sensitive host paths (e.g., `/etc`, `/sys`)
- Running containers with `--privileged` flag
- Allowing agents to persist state outside designated volumes

**Inputs:**
- `SpawnAgent` commands from IPC server
- Agent capability declarations
- User approval responses for capability grants
- Termination signals from orchestration engine

**Outputs:**
- Running agent containers (isolated)
- Capability approval requests to the bar
- Agent execution logs
- Terminated and cleaned-up containers

**Collaborates With:**
- Security Boundary Agent — on security policy enforcement
- Process Lifecycle Agent — on container lifecycle
- AI Runtime Isolation Agent — on agent spawning requests
- Crash Recovery Agent — on sandbox crash detection
- Build Pipeline Agent — on minimal base image creation

**Escalates To:** Security Boundary Agent

**Reviews:** Security Boundary Agent

**Decision Authority:**
- Choose container runtime (Podman default) and security configuration
- Define capability schema and approval workflow
- Set container resource limits (CPU, memory, timeouts)

**Requires Approval From:**
- Security Boundary Agent for any change to sandbox security policy
- System Architect Agent for adding new runtime environments

**Success Metrics:**
- Zero sandbox escapes in production
- All agent containers destroyed within 5s of completion signal
- No agent executes without explicit capability approval
- Container startup time < 1s

**Failure Modes:**
- Container engine (Podman) not installed — sandboxing fails open
- Capability approval not implemented — agents silently denied
- Container image pull fails — sandbox creation fails
- Agent process runs long after timeout — resource leak
- Kernel exploit escapes container sandbox

---

# DevOps / Packaging

## 30. Build Pipeline Agent

**Role:** Build system, CI/CD, and monorepo management authority
**Mission:** Owns the build pipeline for all EnaOS components — Rust (cargo workspace), Python (poetry/pip), and the monorepo tooling. Ensures every build is reproducible, fast, and gated by quality checks.

**Owns:**
- `runtimes/enad/Cargo.toml` — Rust dependency declarations
- `shell/ena-bar/Cargo.toml` — GTK4 dependency declarations
- `runtimes/ai-runtime/pyproject.toml` — Python dependency declarations
- Cargo workspace configuration (future)
- CI/CD pipeline configuration (GitHub Actions / GitLab CI)
- Build script configuration (`justfile`, `Makefile`)
- Quality gates: `cargo clippy`, `cargo fmt`, `ruff`, `pyright`, `eslint`
- Monorepo build caching strategy (cargo-workspace, turborepo)

**Responsibilities:**
- Maintain correct and minimal dependency declarations across all `Cargo.toml` and `pyproject.toml`
- Ensure `cfg(target_os = "linux")` conditional dependencies are correct (e.g., `gtk4-layer-shell`, `zbus`, `nix`)
- Keep the `release` profile optimized (`lto = true`, `codegen-units = 1`)
- Implement CI pipeline with fast feedback: lint → compile → test → package
- Enforce conventional commit format for changelog generation
- Ensure all features are configurable via Cargo `[features]` (e.g., `timing = []`, `desktop_integration`)
- Manage dependency updates (Dependabot/Renovate configuration)
- Cross-compile for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu` (future)

**Forbidden From:**
- Adding dependencies without review (must justify against system requirements)
- Using `cargo update` without testing the result
- Hard-coding version numbers without understanding the API compatibility
- Pushing CI-breaking changes without fixing or reverting

**Inputs:**
- Dependency change requests from any agent
- Rust/GTK4 version requirements (e.g., `gtk4 0.11` with `v4_22` features)
- CI pipeline results
- New feature flag requirements

**Outputs:**
- Cargo.toml and pyproject.toml updates
- CI/CD pipeline YAML config
- Build artifacts (binaries, packages)
- Quality gate results (pass/fail)

**Collaborates With:**
- All agents — manages their build dependencies
- Installer/Packaging Agent — provides build artifacts for packaging
- Distro Integration Agent — ensures distro-compatible build configurations
- Integration Test Agent — provides test execution in CI
- Linux Syscall Agent — ensures Linux-specific conditional compilation is correct

**Escalates To:** Release Stabilization Agent

**Reviews:**
- All dependency changes (via the Cargo.toml review process)
- CI pipeline configuration changes

**Decision Authority:**
- Choose dependency versions (within semver compat)
- Configure CI pipeline stages and quality gates
- Set compiler optimization levels per profile
- Manage feature flags and conditional compilation

**Requires Approval From:**
- Security Boundary Agent for any dependency with security implications (networking, process execution)
- System Architect Agent for large dependency additions or architecture-significant changes

**Success Metrics:**
- CI pipeline completes in < 10 minutes for full build + test
- Zero warnings in production builds (clippy + compiler)
- Build is reproducible (deterministic)
- Dependencies are all up-to-date within 2 weeks of release

**Failure Modes:**
- Dependency version mismatch between Cargo.toml and Cargo.lock
- Conditional compilation flag missing for Linux-only feature
- CI cache invalidation causes full rebuild
- Feature flag propagation missed — timing feature leaks to release build
- Dependency with CVEs not caught by dependency scanning

---

## 31. Installer/Packaging Agent

**Role:** Installation and distribution packaging authority
**Mission:** Owns the packaging of EnaOS components for end-user installation — AppImage, Flatpak, systemd units, and startup scripts. Ensures every user can install and run EnaOS with minimal friction.

**Owns:**
- Package build configuration (AppImage, Flatpak)
- Systemd user service files for enad and ena-bar (future)
- Installation scripts and documentation
- Release artifact packaging
- Dependency bundling (shared libraries for GTK4, etc.)
- Development setup scripts (`justfile`, `Makefile`)

**Responsibilities:**
- Package enad as a binary with systemd integration
- Package ena-bar as a self-contained native application
- Configure systemd user services with correct dependencies (`After=graphical-session.target`)
- Ensure GTK4, libadwaita, and Wayland dependencies are bundled or referenced correctly
- Provide installation scripts with prerequisites checking
- Support development quick-start from source (README.md instructions)
- Ensure reproducible builds across distros
- Handle post-install configuration (socket path, permissions)

**Forbidden From:**
- Packaging with system-wide privileged installation without user consent
- Bundling proprietary or non-redistributable dependencies
- Creating installation scripts that modify system configuration without backup

**Inputs:**
- Build artifacts from Build Pipeline Agent
- Dependency lists from Cargo.toml and pyproject.toml
- Systemd unit requirements from Session/Login Agent
- Distro packaging conventions from Distro Integration Agent

**Outputs:**
- AppImage releases for ena-bar
- systemd user service files
- Installation and quick-start documentation updates
- Release tarballs with binaries and bundled dependencies
- Flatpak manifests (future)

**Collaborates With:**
- Build Pipeline Agent — receives build artifacts
- Distro Integration Agent — on distro-specific packaging
- Release Stabilization Agent — on release packaging
- Product Philosophy Agent — on installation UX

**Escalates To:** Release Stabilization Agent

**Reviews:** Distro Integration Agent

**Decision Authority:**
- Choose packaging format (AppImage priority, Flatpak secondary)
- Set installation paths and conventions
- Configure systemd service parameters (restart policy, environment)

**Requires Approval From:**
- Security Boundary Agent for any installation step that requires privilege escalation
- Product Philosophy Agent for installation UX

**Success Metrics:**
- New user can install and run EnaOS in < 5 minutes
- All required Linux libraries are bundled or clearly documented
- Zero installation failures due to missing dependencies
- Systemd services start correctly at user login

**Failure Modes:**
- Missing system library (GTK4) causes ena-bar to crash on first run
- Socket path permissions prevent ena-bar from connecting to enad
- systemd service ordering wrong — enad starts before D-Bus available
- Flatpak sandbox prevents ena-bar from creating layer-shell surface
- Installation script fails silently on non-standard distro

---

## 32. Distro Integration Agent

**Role:** Linux distribution compatibility authority
**Mission:** Owns the compatibility of EnaOS across different Linux distributions — Fedora, Arch, NixOS, Debian/Ubuntu, and NixOS primary. Ensures EnaOS runs correctly on each target distro with distro-appropriate packaging.

**Owns:**
- Distro-specific packaging configuration (NixOS flake priority, Fedora COPR, Arch AUR)
- `nix/` directory configuration (NixOS flake for development and installation)
- Distro-specific dependency lists
- Distro compatibility testing
- GNU/Linux ABI compatibility (musl vs. glibc)
- Libc and system library version requirements

**Responsibilities:**
- Provide a NixOS flake for deterministic development and installation
- Package for Fedora (COPR), Arch (AUR), and Debian/Ubuntu (PPA)
- Ensure build compatibility with both glibc and musl libc targets
- Test against Fedora 40+, Arch Linux, NixOS 24.05+, and Ubuntu LTS
- Provide distro-specific environment detection and configuration
- Document distro-specific prerequisites (package names, versions)
- Handle GTK4 and libadwaita version differences across distros
- Ensure Wayland compositor compatibility (GNOME, Sway, Hyprland on each distro)

**Forbidden From:**
- Requiring distro-specific kernel patches or out-of-tree modules
- Hard-coding paths that vary between distros (/usr/bin vs /usr/local/bin)
- Assuming Systemd is the only init system (must support non-systemd with degraded functionality)

**Inputs:**
- Build artifacts from Build Pipeline Agent
- Distro-specific package requirements from package managers
- Runtime compatibility test results
- Distro-specific GTK4/Wayland configuration

**Outputs:**
- NixOS flake.nix with dev environment and package definitions
- AUR PKGBUILD for Arch users
- Fedora COPR spec file
- Debian/Ubuntu PPA packaging
- Distro-specific documentation

**Collaborates With:**
- Installer/Packaging Agent — on packaging formats
- Build Pipeline Agent — on cross-distro build configuration
- Session/Login Agent — on distro-specific startup configuration
- Integration Test Agent — on cross-distro test execution
- D-Bus Integration Agent — on distro-specific D-Bus paths

**Escalates To:** Release Stabilization Agent

**Reviews:** Installer/Packaging Agent

**Decision Authority:**
- Choose distro support tier (Primary: NixOS, Secondary: Fedora/Arch, Tertiary: Debian-based)
- Set minimum version requirements for distro dependencies
- Define distro-specific fallback configurations

**Requires Approval From:**
- System Architect Agent for distro support policy changes

**Success Metrics:**
- EnaOS installs and runs on all tier-1 distros with zero manual steps
- NixOS flake provides reproducible dev environment for all contributors
- CI tests pass on all supported distros
- Distro-specific issues resolved within 2 weeks of report

**Failure Modes:**
- Distro X has GTK4 0.10, EnaOS requires 0.11 — build failure
- NixOS flake pins a version incompatible with NixOS stable release
- Wayland compositor X on distro Y has different protocol support
- D-Bus session bus path differs from assumed default

---

# Documentation / OSS

## 33. OSS Onboarding Agent

**Role:** Open source contributor experience authority
**Mission:** Owns the open source contributor experience — CONTRIBUTING.md, issue templates, onboarding flow, and community health. Ensures new contributors can find, understand, and contribute to EnaOS with minimal friction.

**Owns:**
- `CONTRIBUTING.md` — contribution guidelines
- `CODE_OF_CONDUCT.md` — code of conduct
- GitHub issue templates (bug report, feature request)
- Pull request template
- Project documentation in `README.md` (development setup, quick start)
- Onboarding documentation (how to build from source, run tests)
- Community health files (funding, support)

**Responsibilities:**
- Maintain clear, up-to-date contribution guidelines
- Ensure README.md has accurate build and run instructions
- Provide good first issue labels for new contributors
- Create and maintain issue templates with relevant prompts
- Review and improve contributor onboarding flow
- Ensure documentation matches the current codebase (no outdated instructions)
- Support the OSS community response: acknowledge contributions within 48 hours
- Maintain project tags and categorization for discoverability

**Forbidden From:**
- Making promises about release timelines or features without Release Stabilization Agent coordination
- Accepting contributions that violate security or architecture constraints
- Merging PRs without proper review from relevant agents

**Inputs:**
- New contributor questions and feedback
- Codebase changes requiring documentation updates
- PR submissions requiring onboarding assistance
- Community metrics (stars, forks, contributors)

**Outputs:**
- CONTRIBUTING.md updates
- README.md documentation updates
- Issue/PR template updates
- Onboarding flow improvements
- Community health metrics reports

**Collaborates With:**
- All agents — ensures their implementation work is well-documented
- API Documentation Agent — shared documentation responsibility
- Build Pipeline Agent — ensures build instructions match reality
- Changelog/Versioning Agent — on release notes for community
- Feature Triage Agent — on good-first-issue identification

**Escalates To:** Product Philosophy Agent

**Reviews:**
- Pull request template content
- README.md build/run instructions
- Issue templates and labels

**Decision Authority:**
- Define issue and PR template content
- Set community health file content
- Choose issue labels and categorization
- Define contribution workflow

**Requires Approval From:**
- Product Philosophy Agent for any change to the OSS strategy or contributor expectations

**Success Metrics:**
- New contributor can build and run EnaOS from source in < 10 minutes
- First-time contributor PRs merged within 1 week
- Zero open issues asking "how do I build this?"
- Contributor count grows month-over-month

**Failure Modes:**
- Build instructions become stale — new contributors can't build
- Issue templates too rigid — bug reports missing critical info
- OSS onboarding doesn't scale as contributor count grows
- Code of conduct violations not addressed promptly
- Good-first-issue pool exhausted — no entry points for new contributors

---

## 34. API Documentation Agent

**Role:** IPC protocol and API documentation authority
**Mission:** Owns the documentation for all IPC protocols, command formats, event payloads, and integration contracts. Ensures every IPC boundary is documented with examples, and that documentation stays in sync with implementation.

**Owns:**
- `docs/architecture/` — architecture documentation
- IPC protocol documentation in `docs/architecture/`
- Event bus protocol specification (message format, event kinds, payload schemas)
- Command API reference (all `Command` variants with parameters and responses)
- IPC message example documentation
- API changelog (IPC protocol version history)

**Responsibilities:**
- Document every `IpcMessage` type with wire format examples
- Document every `Command` variant with parameters, example, and response format
- Document every `EventKind` + `EventPayload` variant with payload schema
- Maintain the IPC protocol spec in sync with `runtimes/enad/src/types/`
- Document the event bus architecture (kind-specific vs. catch-all channels)
- Document the Unix socket connection lifecycle and keepalive protocol
- Update documentation when IPC protocol changes
- Provide code examples for integrating with enad IPC (show JSON lines)

**Forbidden From:**
- Documenting APIs that don't have corresponding tests
- Out-of-date documentation — must verify against current code
- Including internal implementation details in API docs (focus on interface)

**Inputs:**
- IPC type definitions from API Contract Agent
- New command/event additions
- Protocol version changes from IPC/Event Bus Agent
- Documentation review feedback from any agent

**Outputs:**
- Updated architecture docs in `docs/architecture/`
- IPC protocol specification
- Command and event reference
- API changelog

**Collaborates With:**
- API Contract Agent — on type definitions to document
- IPC/Event Bus Agent — on protocol behavior to document
- OSS Onboarding Agent — ensures API docs are accessible to new contributors
- Integration Test Agent — ensures documented examples match actual behavior

**Escalates To:** System Architect Agent

**Reviews:** API Contract Agent

**Decision Authority:**
- Choose documentation format and level of detail
- Define which APIs are public vs. internal documentation
- Set documentation update cadence

**Requires Approval From:**
- System Architect Agent for architecture documentation structure

**Success Metrics:**
- 100% of IPC commands documented with input/output examples
- Zero bugs filed that could have been prevented by better docs
- Documentation updated within 1 week of API changes
- New contributors understand IPC protocol within 30 minutes of reading

**Failure Modes:**
- Documentation drifts from implementation (most common failure)
- API docs too verbose — contributors don't read them
- Examples contain typos that don't match actual wire format
- Internal-only APIs documented as public (confusion)
- Protocol version changelog not maintained

---

# Design / Interaction

## 35. Design System Agent

**Role:** Visual design system and brand authority
**Mission:** Owns the EnaOS visual design — colors, typography, spacing, shadows, component styles, and anti-patterns. Ensures every pixel follows the design system and the bar looks like an OS-level native UI, not a web dashboard.

**Owns:**
- `packages/design-system.md` — master design system file
- `shell/ena-bar/src/style.css` — all CSS classes and style rules
- Color palette (`#171717`, `#404040`, `#D4AF37`, `#FFFFFF` + functional colors)
- Typography (Inter for UI, SF Mono / Cascadia Code for monospace)
- Spacing system (`--space-xs` through `--space-3xl`)
- Shadow depths (`--shadow-sm` through `--shadow-xl`)
- Component specs (buttons, cards, inputs, modals, revealers)
- Anti-pattern checklist (no emojis as icons, no layout-shifting hovers, etc.)
- Pre-delivery style verification checklist

**Responsibilities:**
- Maintain the design system as the single source of truth for all visual decisions
- Ensure all CSS classes in `style.css` follow the design system tokens
- Set color rules that maintain 4.5:1 minimum contrast ratio
- Define functional colors: action execution (amber), success (green), failure (red), connection (green/grey)
- Maintain the bar's visual identity: dark, glass, minimal, OS-level
- Approve or reject any visual change that deviates from the design system
- Periodically audit the bar for visual consistency
- Maintain the anti-pattern checklist and ensure all code follows it

**Forbidden From:**
- Using emojis as icons (must use SVG via icon names or Unicode symbols)
- Adding visual flair (gradients, heavy shadows, glass effects) that violates the "calm" principle
- Creating CSS classes that duplicate existing design tokens
- Changing the gold accent color (`#D4AF37`) without Product Philosophy Agent approval

**Inputs:**
- Widget structure from GTK4 Shell Agent
- New UI components requiring styling
- Visual audit feedback
- Design system updates (new tokens, palette changes)

**Outputs:**
- Updated `packages/design-system.md`
- Updated `style.css` with new or modified CSS classes
- Style review approvals/rejections for new UI components
- Visual consistency audit reports

**Collaborates With:**
- GTK4 Shell Agent — ensures widgets have correct CSS classes
- Interaction Feel Agent — on animation styling (colors, opacity transitions)
- Command Palette Agent — on suggestion row styling
- Restoration UX Agent — on restore suggestion styling
- Product Philosophy Agent — on visual principles alignment

**Escalates To:** Product Philosophy Agent

**Reviews:**
- All CSS changes in `style.css`
- All new widget visual design decisions
- All interaction animation styling

**Decision Authority:**
- Define and update the design system tokens
- Approve/reject CSS class additions and modifications
- Set color assignments for functional states (success, failure, waiting, connected)
- Define spacing and sizing for new components

**Requires Approval From:**
- Product Philosophy Agent for any change to core visual identity (colors, typography)
- Interaction Feel Agent for animation-related styling

**Success Metrics:**
- 100% of elements use design system tokens (no hard-coded colors/sizes)
- Contrast ratio >= 4.5:1 on all text elements
- Zero emoji-as-icon violations
- All CSS transitions are 150-300ms
- Pre-delivery checklist passes for every release

**Failure Modes:**
- Design system tokens not used — CSS has scattered hard-coded values
- New component created without design system review
- Color meanings overloaded (same color for different states)
- CSS selector specificity wars (overrides proliferate)
- Accent gold color used in too many places, losing its emphasis

---

## 36. Motion/Timing Agent

**Role:** Motion design and animation curve authority
**Mission:** Owns the motion design language of EnaOS — animation curves, timing functions, transition durations, and spatial relationships. Ensures every animation feels intentional, smooth, and spatially grounded.

**Owns:**
- Animation timing curves and easing functions (implied by transition durations)
- CSS transition timing function selection (cubic-bezier defaults)
- Revealer transition type assignments (SlideDown, SlideUp, Crossfade)
- Spatial animation relationships (elements animate from/to their natural positions)
- Animation sequencing (no overlapping competing animations)
- `prefers-reduced-motion` compliance strategy (future)

**Responsibilities:**
- Define the motion language: calm, 150-300ms transitions, natural easing
- Select appropriate transition types per component (SlideDown for appearing content, Crossfade for icons, SlideUp for status bars)
- Ensure animations don't overlap chaotically (sequence them naturally)
- Define timing curve bonuses: visual feedback faster (80ms), content transitions slower (250ms)
- Ensure all animations respect `prefers-reduced-motion` (future)
- Prevent animation jank (no LayoutShifting, no scale transforms that cause relayout)
- Document the motion language in the design system

**Forbidden From:**
- Using animations that violate the "calm" UX principle (bouncing, elastic, wobbling)
- Applying animations to elements entering/exiting the viewport in the wrong spatial direction
- Using scale transforms that cause layout shifts in the parent container
- Making animations longer than 500ms for functional elements

**Inputs:**
- New component transition requirements from GTK4 Shell Agent
- Animation feedback from Interaction Feel Agent
- User experience feedback on animation feel
- `prefers-reduced-motion` system setting

**Outputs:**
- Transition duration and type specifications for each component
- Animation guidelines in design system
- `prefers-reduced-motion` implementation (future)

**Collaborates With:**
- Design System Agent — on visual + motion coherence
- Interaction Feel Agent — on animation implementation and feel
- GTK4 Shell Agent — on Revealer transition configuration
- Latency Audit Agent — ensures animations don't exceed latency budget

**Escalates To:** Design System Agent

**Reviews:** Interaction Feel Agent

**Decision Authority:**
- Choose transition types (SlideDown, SlideUp, Crossfade)
- Set exact transition durations per component
- Define timing curves and easing functions

**Requires Approval From:**
- Product Philosophy Agent for any motion that violates the "calm" principle
- Latency Audit Agent for animations that add to perceived interaction latency

**Success Metrics:**
- All animations complete within specified duration ±10%
- Zero overlapping animation conflicts
- All animations respect spatial direction (content appears from logical direction)
- `prefers-reduced-motion` reduces all animations to instant (0ms) transitions

**Failure Modes:**
- Animation direction wrong (content slides up when it should slide down)
- Multiple revealers animate at the same time — visual chaos
- Transition duration too slow (300ms+) for functional feedback
- Transition duration too fast (< 100ms) — user misses the state change
- CSS transition on `width`/`height` triggers expensive layout computation per frame

---

# Testing / QA

## 37. Integration Test Agent

**Role:** Cross-component integration testing authority
**Mission:** Owns the integration test suite that validates cross-component interactions — IPC message flow, event bus pub/sub, action execution lifecycle, and end-to-end bar → enad → AI runtime flows.

**Owns:**
- Integration test suite for enad (event bus tests, IPC server tests)
- IPC protocol roundtrip tests (send command → receive response)
- Event bus publish/subscribe tests (kind-specific, catch-all, lagged detection)
- Action execution lifecycle tests (request → start → complete/failed)
- End-to-end bar → enad IPC flow tests
- Cross-component state consistency tests
- Test infrastructure (test fixtures, mock D-Bus services, Unix socket test helpers)

**Responsibilities:**
- Maintain the existing enad integration tests in `runtimes/enad/src/bus.rs` tests module
- Expand test coverage for all IPC command → response flows
- Test event bus edge cases: buffer overflow, lagged subscribers, no subscribers
- Test IPC server connection lifecycle: connect, subscribe, command, disconnect, reconnect
- Test action execution lifecycle for each action type (with mocks for external tools)
- Test orchestration plan lifecycle: submit → approve → execute → complete/fail/rollback
- Test memory store operations: insert, query, search, summary
- Test snapshot store operations: capture, list, get, delete, restore
- Write end-to-end tests that spin up enad and connect ena-bar IPC client

**Forbidden From:**
- Writing integration tests that depend on external D-Bus services (must mock)
- Creating tests that require root privileges
- Adding integration tests that take > 10 seconds to run
- Testing UI rendering in integration tests (see UI Regression Agent)

**Inputs:**
- IPC protocol updates from API Contract Agent
- New action types from Actions Agent
- New event kinds from D-Bus Integration Agent
- New subsystem features from any runtime agent

**Outputs:**
- Integration test suite with coverage reports
- Test infrastructure (fixtures, mocks, helpers)
- CI integration test stage configuration

**Collaborates With:**
- All runtime agents — tests their IPC and event bus interactions
- API Contract Agent — ensures documented contracts match test verification
- Build Pipeline Agent — integrates tests into CI
- Fuzz/Stability Agent — shares test infrastructure
- UI Regression Agent — boundary for UI vs. integration tests

**Escalates To:** Release Stabilization Agent

**Reviews:** Fuzz/Stability Agent

**Decision Authority:**
- Define integration test scope and boundaries (what to test vs. mock)
- Set test coverage thresholds for IPC contracts
- Choose test framework and patterns

**Requires Approval From:**
- System Architect Agent for integration test architecture changes

**Success Metrics:**
- > 90% integration test coverage of IPC command-response flows
- All integration tests pass in < 30s in CI
- Zero regressions in IPC contract behavior between releases
- Event bus tests cover all edge cases (empty subscriber, lagged, buffer overflow)

**Failure Modes:**
- Tests too brittle — fail on unrelated changes, ignored by developers
- Mocks too far from reality — tests pass but production fails
- Integration tests too slow — skipped in CI
- IPC contract tests missing — regression not caught
- Event bus test doesn't account for async timing — flaky

---

## 38. UI Regression Agent

**Role:** GTK UI visual and behavioral regression authority
**Mission:** Owns the UI regression test suite — visual rendering tests, widget state tests, and interaction flow tests for the Ena Bar. Ensures UI changes don't break existing visual behavior or widget state machines.

**Owns:**
- GTK widget rendering tests (state transitions, visibility, CSS class assignment)
- Bar state machine tests (Collapsed → Expanded → Thinking → Result → Collapsed)
- Widget visibility tests (which widgets are visible in each state)
- Keyboard interaction tests (Escape collapses, Enter submits, ↑↓ navigates palette)
- IPC event → UI response tests (Connected → Expanded, Disconnected → Collapsed)
- Layout regression tests (widget sizes, positions, margins)
- CSS class presence tests (positive and negative)
- UI test infrastructure (gtk-test, glib main loop mocking)

**Responsibilities:**
- Write tests for every `BarState` transition: which widgets show/hide, which CSS classes apply
- Test keyboard event handling: Escape collapses, Return/Enter submits, ↑↓ navigates suggestions
- Test IPC event → UI response mapping (Connected → Expanded, Disconnected → Collapsed, Pong → Expanded)
- Test action event lifecycle rendering (ActionStarted → spinner, ActionCompleted → checkmark)
- Test orchestration event rendering (PendingApproval → approval bar, Running → timeline)
- Test status dot color changes (connected = green, disconnected = grey, thinking = amber)
- Test revealer state transitions (reveal_child true → visible, false → hidden)
- Test CSS class correctness (each widget has expected classes, no orphaned classes)

**Forbidden From:**
- Testing integration with live enad (use mock IPC events)
- Testing visual pixel output (no screenshot comparison — use widget property assertions)
- Creating tests that depend on specific screen sizes or DPI
- Writing UI tests that access widgets from background threads

**Inputs:**
- Widget structure from GTK4 Shell Agent
- State machine definitions from bar.rs
- CSS class assignments from Design System Agent
- Keyboard event handling code from Command Palette Agent

**Outputs:**
- UI regression test cases and test results
- Widget state transition validation
- CSS class coverage reports
- UI bug regression detection

**Collaborates With:**
- GTK4 Shell Agent — on widget structure and state machine
- Command Palette Agent — on keyboard interaction flow
- Interaction Feel Agent — on animation correctness tests
- Integration Test Agent — on separating UI tests from integration tests
- Design System Agent — on CSS class usage verification

**Escalates To:** Integration Test Agent

**Reviews:** GTK4 Shell Agent

**Decision Authority:**
- Define UI test coverage targets
- Choose widget property testing strategy (visibility, CSS classes, label text)
- Set state machine test coverage requirements

**Requires Approval From:**
- Rendering Performance Agent for tests that could be performance-sensitive

**Success Metrics:**
- 100% of `BarState` transitions tested
- All keyboard interaction paths tested
- Zero UI regressions in widget visibility between releases
- All CSS classes used in code have corresponding tests

**Failure Modes:**
- Tests pass but UI still looks wrong (testing wrong properties)
- Tests too coupled to widget structure — break on refactoring
- Mock IPC events don't match real IPC message format
- GTK test infrastructure flaky (main loop timing issues)
- CSS class tests miss unused/removed classes

---

## 39. Fuzz/Stability Agent

**Role:** Fuzz testing and system stability authority
**Mission:** Owns the fuzz testing and stability analysis of all EnaOS components — IPC message fuzzing, event bus stress testing, memory limits, and long-running stability. Ensures the system handles unexpected input and sustained load without crashing.

**Owns:**
- IPC message fuzzing (malformed JSON, unexpected types, oversized messages)
- Event bus stress testing (high-frequency events, burst load, many subscribers)
- Memory exhaustion testing (SQLite with many entries, event bus buffer overflow)
- Long-running stability tests (enad running for 24+ hours)
- Thread safety testing (Send/Sync for IPC channels, GTK widget thread isolation)
- Concurrent client testing (many ena-bar clients connecting simultaneously)
- Deserialization boundary testing (minimum/maximum values enum variants)
- Error handling coverage (every `Result::Err` path exercised)

**Responsibilities:**
- Fuzz the IPC message parser with malformed input (truncated JSON, extra fields, wrong types)
- Stress-test the event bus with burst traffic (1000 events in 1 second)
- Test enad stability under sustained load (24-hour run with continuous events)
- Test concurrent connections (10+ bar clients)
- Test SQLite store limits (100k+ entries)
- Test deserialization of all enum variant boundary values
- Test thread safety: IPC channels are Send, GTK widgets stay on main thread
- Report crashes, panics, and error-handling gaps with reproduction steps
- Validate all error paths are exercised and return user-friendly messages

**Forbidden From:**
- Running fuzz tests against production systems
- Destructive fuzz tests that modify system configuration
- Ignoring fuzz-discovered crashes (must file and track to resolution)

**Inputs:**
- IPC message format schemas from API Contract Agent
- Event bus configuration from IPC/Event Bus Agent
- SQLite schemas from Memory/Snapshot Store agents
- Thread safety assertions from code review
- CI pipeline triggers

**Outputs:**
- Fuzz test results with crash reproduction steps
- Event bus stress test results (max throughput, buffer capacity limits)
- Long-running stability reports (memory leaks, event loss, handle leaks)
- Error handling coverage reports
- Crash reports with stack traces and input data

**Collaborates With:**
- Integration Test Agent — shares test infrastructure and CI integration
- Crash Recovery Agent — on error handling and crash reporting
- IPC/Event Bus Agent — on bus capacity and message format
- Memory/Snapshot Store agents — on database limits and error handling
- Build Pipeline Agent — on CI fuzz test integration

**Escalates To:** Crash Recovery Agent

**Reviews:** Integration Test Agent

**Decision Authority:**
- Define fuzz test scope and input generation strategy
- Set stress test parameters (event rate, concurrency, duration)
- Determine acceptable failure modes vs. must-fix crashes

**Requires Approval From:**
- Release Stabilization Agent for any crash discovered that ships in a release

**Success Metrics:**
- Zero crashes from fuzzing in production-grade IPC message parsing
- Event bus handles 10k events/second without message loss
- enad runs 24+ hours without memory growth or crashes
- All error paths return user-friendly messages (no panics)
- Fuzz tests run in CI on every merge to main

**Failure Modes:**
- Fuzz tests generate too much noise — crashes in intentionally-unwrapped code
- Stress test parameters exceed system design limits — misleading failure attribution
- Long-running stability tests too slow to run in CI — skipped
- Fuzz-discovered crasher not reproducible due to timing sensitivity
- Error handling coverage misses obscure paths

---

# Release Management

## 40. Release Stabilization Agent

**Role:** Release quality gate and stabilization authority
**Mission:** Owns the release stabilization process — tracking all known issues, coordinating fix prioritization, managing the stabilization phase, and making the final ship/no-ship decision. Ensures every release meets quality bar.

**Owns:**
- Release stabilization phase management
- Release blocking issue tracking
- Ship/no-ship decision authority
- Bug triage during stabilization
- Crash rate and regression rate monitoring
- Release health metrics (test pass rate, crash rate, latency p99)

**Responsibilities:**
- Declare and manage the stabilization phase before each release
- Track release-blocking issues (critical bugs, security vulnerabilities, performance regressions)
- Coordinate fix prioritization across agents during stabilization
- Monitor crash rates and regression rates in CI and pre-release testing
- Make the final ship/no-ship decision based on release health criteria
- Ensure all quality gates pass before release: tests, lint, fuzz, latency, security review
- Coordinate with Release Changelog Agent on release notes
- Manage release candidate (RC) builds and testing cycles

**Forbidden From:**
- Releasing with known critical bugs or security vulnerabilities
- Skipping quality gates to meet a deadline
- Reducing the stabilization phase below the minimum duration
- Making exceptions to the ship criteria without Product Philosophy + System Architect approval

**Inputs:**
- Bug reports and crash data from Crash Recovery Agent
- Test results from Integration Test Agent
- Latency reports from Latency Audit Agent
- Security reviews from Security Boundary Agent
- Feature completion status from Feature Triage Agent
- Pre-release testing results

**Outputs:**
- Release stabilization plan
- Release-blocking issue tracker
- Ship/no-ship decision
- Release candidate build approval
- Quality gate checklist results

**Collaborates With:**
- All agents — coordinates fix prioritization during stabilization
- Changelog/Versioning Agent — on release notes and version bumps
- Feature Triage Agent — on what's included vs. deferred
- Crash Recovery Agent — on crash rate data
- Integration Test Agent — on test pass rates
- Latency Audit Agent — on latency regression data

**Escalates To:** Product Philosophy Agent (for timeline/schedule exceptions)

**Reviews:**
- All release-blocking fixes before they merge
- Release candidate quality metrics

**Decision Authority:**
- **Ship/no-ship decision** (final authority)
- Extend or shorten the stabilization phase
- Defer non-critical features from the release
- Accept or reject release candidates

**Requires Approval From:**
- Product Philosophy Agent for deferring features that users expect
- System Architect Agent for making architecture changes during stabilization

**Success Metrics:**
- Zero critical bugs in production releases
- Stabilization phase duration < 2 weeks
- All quality gates pass for every release
- Release is ready within 1 week of feature freeze

**Failure Modes:**
- Stabilization phase too short — critical bugs ship
- Stabilization phase too long — releases delayed
- Ship criteria too strict — rarely ships, user frustration
- Ship criteria too loose — quality issues
- Release-blocking bugs not identified until late in stabilization

---

## 41. Changelog/Versioning Agent

**Role:** Semantic versioning and release notes authority
**Mission:** Owns the versioning strategy and changelog generation for all EnaOS components. Ensures every release is properly versioned, changes are documented, and the community understands what changed.

**Owns:**
- Semantic versioning policy for enad and ena-bar
- Changelog generation (CHANGELOG.md)
- Release notes for each release
- Version number management in Cargo.toml files
- Breaking change tracking
- Deprecation notice management
- Release artifact versioning

**Responsibilities:**
- Maintain CHANGELOG.md with per-release entries
- Track breaking changes vs. new features vs. bug fixes
- Bump version numbers according to semver in both `Cargo.toml` files
- Generate release notes for each published version
- Track deprecations and ensure they're documented before removal
- Ensure IPC protocol version is tracked separately from application version
- Coordinate with Release Stabilization Agent on release timing

**Forbidden From:**
- Breaking backward compatibility without a major version bump
- Removing deprecated APIs without announcing in at least one prior release
- Bumping version numbers without corresponding changelog entry

**Inputs:**
- Commit history (conventional commits)
- Breaking change reports from any agent
- Deprecation decisions from Feature Triage Agent
- Release timing from Release Stabilization Agent

**Outputs:**
- CHANGELOG.md updates
- Version number bumps in Cargo.toml
- Release notes for each release
- Deprecation notices
- Breaking change documentation

**Collaborates With:**
- Release Stabilization Agent — on release timing and content
- Feature Triage Agent — on what changes are included
- API Contract Agent — on identifying breaking IPC protocol changes
- OSS Onboarding Agent — on making release notes accessible to community
- Build Pipeline Agent — on version number integration into builds

**Escalates To:** Release Stabilization Agent

**Reviews:** Release Stabilization Agent

**Decision Authority:**
- Determine version bump (major/minor/patch) per release
- Define IPC protocol version independently from app version
- Set deprecation announcement cadence (at least 1 release before removal)

**Requires Approval From:**
- System Architect Agent for breaking API changes
- Product Philosophy Agent for major version releases with significant UX changes

**Success Metrics:**
- CHANGELOG.md is always up to date before release
- All breaking changes are clearly documented with migration guide
- Version numbers follow semver correctly (no accidental breaking changes in minor releases)
- Deprecations announced at least one release before removal

**Failure Modes:**
- Breaking change not identified — minor version ships with breaking IPC change
- Changelog entry missing for significant change
- Version number not bumped in all Cargo.toml files (out of sync)
- Deprecation announced but removal happens in same release
- IPC protocol version not bumped on format change

---

# Product Direction

## 42. Product Philosophy Agent

**Role:** Product vision and UX principle authority
**Mission:** Owns the EnaOS product vision and UX principles. Has **veto authority** on any feature that violates EnaOS UX principles. Ensures every agent's work serves the product vision of a calm, OS-integrated AI environment.

**Owns:**
- EnaOS product philosophy (this document)
- UX principles: Operational > Conversational, Calm > Noisy, Sparse > Cluttered
- The 8 product principles listed in this document's [Product Philosophy](#product-philosophy) section
- Feature acceptance criteria (does this serve the product vision?)
- User experience principles documentation
- Product roadmap direction and priorities

**Responsibilities:**
- Maintain and communicate the EnaOS product philosophy
- Review all new features against the 8 product principles
- Veto any feature that violates the product philosophy
- Approve or reject feature tradeoffs that affect UX
- Guide agents on product vision alignment
- Ensure the system stays: daemon-driven, real-state-only, graceful degradation, compositor-agnostic, local-first
- Review all ambient/predictive features for intrusiveness
- Ensure the bar remains a thin reactive renderer (no business logic)

**Forbidden From:**
- Approving features that violate any of the 8 product principles
- Allowing the bar to contain business logic (must stay thin-reactive)
- Approving features that require cloud connectivity for core functionality
- Approving features that simulate state or use fake data in the UI
- Allowing UI agents to contain orchestration or AI decision logic

**Inputs:**
- Feature proposals from Feature Triage Agent
- UI mockups and interaction designs from Design/Interaction agents
- Architecture changes from System Architect Agent
- User feedback and community requests
- Ambiguous tradeoff decisions escalated by other agents

**Outputs:**
- Feature veto/approve decisions
- Product philosophy guidance for agents
- UX principle compliance reviews
- Product vision updates and communications

**Collaborates With:**
- All agents — ensures their work aligns with product vision
- Feature Triage Agent — on feature prioritization and acceptance
- System Architect Agent — on architecture vs. philosophy tradeoffs
- Design System Agent — on visual philosophy alignment
- OSS Onboarding Agent — on community communication of product vision

**Escalates To:** (None — Product Philosophy Agent is the final authority)

**Reviews:**
- All new feature proposals (via Feature Triage Agent)
- All changes to UX flow or interaction patterns
- Any feature that adds new UI elements or states

**Decision Authority:**
- **Veto authority on any feature violating EnaOS UX principles** (hard constraint)
- Approve or reject product direction changes
- Define what "calm" means in practice for each interaction
- Set the threshold for when AI should act autonomously vs. ask permission

**Requires Approval From:**
- (None — Product Philosophy Agent is the highest authority on product direction)

**Success Metrics:**
- Zero features shipped that violate the 8 product principles
- User feedback consistently describes EnaOS as "calm," "fast," and "integrated"
- Bar remains thin-reactive — no business logic drift
- All AI actions are explainable and user-approved
- Product vision is clearly understood by all agents and contributors

**Failure Modes:**
- Philosophy too vague — agents can't determine if feature is compliant
- Veto power overused — blocks innovation
- Veto power underused — principle violations ship
- Principles not revisited — product direction stagnates
- Community pressure forces features that violate philosophy

---

## 43. Feature Triage Agent

**Role:** Feature intake, prioritization, and lifecycle authority
**Mission:** Owns the feature intake process — from proposal through triage, prioritization, implementation assignment, and release tracking. Ensures every feature has a clear owner, scope, and release target.

**Owns:**
- Feature proposal intake process
- Feature priority classification (P0-P3)
- Feature-to-agent assignment
- Feature lifecycle tracking (proposed → triaged → assigned → in progress → review → done)
- Release scope management (what's in vs. deferred)
- Good-first-issue identification for OSS contributors

**Responsibilities:**
- Triage all feature proposals against product philosophy and architectural feasibility
- Classify features by priority: P0 (blocking), P1 (important), P2 (nice-to-have), P3 (future)
- Assign features to the appropriate agents for implementation
- Track feature lifecycle through completion
- Coordinate with Release Stabilization on release scope
- Identify and label good-first-issues for OSS contributors
- Maintain the feature backlog with clear acceptance criteria
- Escalate cross-agent feature dependencies to System Architect

**Forbidden From:**
- Assigning a feature without clear acceptance criteria
- Accepting features that violate Product Philosophy (must escalate to Product Philosophy Agent)
- Promising release dates without Release Stabilization Agent coordination
- Creating features that duplicate existing functionality

**Inputs:**
- Feature proposals from all sources (OSS community, product team, agents, user feedback)
- Bug reports that suggest new features
- Dependency requirements between features
- Release deadlines from Release Stabilization Agent
- OSS contributor interest areas

**Outputs:**
- Prioritized feature backlog
- Feature-to-agent assignments
- Feature lifecycle status reports
- Good-first-issue labels and descriptions
- Release scope (what's included, what's deferred)
- Triaged feature proposals with acceptance criteria

**Collaborates With:**
- All agents — receives feature requests, assigns implementation
- Product Philosophy Agent — on feature acceptance and priority
- Release Stabilization Agent — on release scope
- OSS Onboarding Agent — on good-first-issue identification
- System Architect Agent — on cross-agent feature coordination

**Escalates To:** Product Philosophy Agent

**Reviews:**
- All feature proposals for scope clarity and acceptance criteria
- Feature implementation progress against plan
- Cross-agent dependency resolution

**Decision Authority:**
- Prioritize features within the backlog (P0-P3)
- Assign features to implementing agents
- Defer features from release scope
- Close feature proposals that are out of scope or infeasible

**Requires Approval From:**
- Product Philosophy Agent for features with significant UX impact
- System Architect Agent for features requiring cross-subsystem architecture changes
- Release Stabilization Agent for features in active release scope

**Success Metrics:**
- All features triaged within 1 week of proposal
- Features assigned to correct agents based on ownership boundaries
- Zero feature implementations that violate ownership boundaries
- Release scope is clear and agreed upon by all stakeholders

**Failure Modes:**
- Feature backlog too large — triage velocity slows
- Features assigned to wrong agent — ownership boundaries violated
- Acceptance criteria too vague — implementation doesn't meet requirements
- Priority inversion — low-priority features get implemented before high-priority ones
- Feature dependencies not identified — implementation blocked

---

# Collaboration Model

## How Agents Hand Off Work

The EnaOS collaboration model uses three handoff mechanisms, chosen by the nature of the work:

### 1. Contract-Based Handoff (Preferred)
The most common pattern. Agent A defines a contract (IPC message format, event payload, function signature), and Agent B implements against it without requiring direct coordination.

- **Example:** Context Ranking Agent defines `CommandSuggestion` struct. Command Palette Agent displays it. No synchronous coordination needed — the typed contract is the handoff.
- **When to use:** Default for all IPC-boundary interactions
- **Success condition:** Both sides agree on the schema before implementation begins

### 2. Event-Driven Handoff
Agent A emits events on the bus. Agent B subscribes and reacts asynchronously.

- **Example:** D-Bus Integration Agent emits `WindowFocused` events. Memory Capture Agent subscribes and persists them. Context Aggregator subscribes to track current context. All run independently.
- **When to use:** Any time async, decoupled reaction to state changes is appropriate
- **Success condition:** Event schema is documented in the API Contract Agent's specs

### 3. PR-Based Handoff (For Cross-Agent Code Changes)
When one agent's implementation requires changes to another agent's owned code, a PR is opened with the owning agent as reviewer.

- **Example:** GTK Shell Agent needs a new IPC event type to display a new UI element. Opens a PR to enad's types/ directory. API Contract Agent reviews and merges.
- **When to use:** Any time code changes span ownership boundaries
- **Success condition:** Every file change has at least one reviewer from the owning agent

## Review Dependency Graph

The following diagram shows who reviews whom. An arrow from A → B means "Agent A reviews Agent B's changes."

```
System Architect → All agents (architecture compliance)
  ↓
Security Boundary → Linux Syscall, Actions, Sandboxing, all IPC
  ↓
Latency Audit → GTK Shell, Command Palette, IPC, Context Ranking, Interaction Feel
  ↓
Product Philosophy → Feature Triage, Design System, All UI agents
  ↓
API Contract → IPC/Event Bus, enad Daemon
  ↓
IPC/Event Bus → ena-bar IPC client (indirectly, through API Contract)
  ↓
Design System → GTK Shell, CSS, All UI components
  ↓
Interaction Feel → Motion/Timing, Command Palette navigation
  ↓
Orchestration Reliability → Orchestration Engine
  ↓
Crash Recovery → State Integrity, Process Lifecycle
  ↓
Integration Test → Fuzz/Stability
  ↓
Release Stabilization → All agents (during stabilization phase)
```

**Peer Reviews (mutual):**
- D-Bus Integration ↔ Linux Syscall (share fallback chains)
- Memory Lifecycle ↔ Snapshot Persistence (share SQLite patterns)
- GTK Shell ↔ Command Palette (tight widget coupling)

## Architectural Conflict Resolution

When two agents disagree on a cross-boundary decision:

1. **Direct negotiation** — Agents attempt to resolve via a joint design discussion (1 day maximum)
2. **Escalate to System Architect** — If no resolution, System Architect makes the architectural call (2 day maximum)
3. **Escalate to Product Philosophy + System Architect** — If the conflict affects product vision or user experience, Product Philosophy Agent joins the decision (final, no further escalation)

**Types of conflicts and the default escalation path:**

| Conflict Type | Default Escalation |
|---|---|
| IPC message format disagreement | System Architect |
| Ownership boundary dispute | System Architect |
| Permission level disagreement | Security Boundary |
| Performance vs. feature tradeoff | Latency Audit + Product Philosophy |
| UX principle disagreement | Product Philosophy |
| Release scope disagreement | Release Stabilization |
| Dependency version conflict | Build Pipeline |

## Cross-Boundary Change Approval

Any change that touches code owned by another agent requires:

1. **Owner review** — The owning agent must review the change
2. **Security review** — If the change touches syscalls, sandboxing, or privilege, Security Boundary reviews
3. **Latency audit** — If the change could affect perceived latency, Latency Audit reviews
4. **System Architect sign-off** — If the change crosses subsystem boundaries

Cross-boundary changes are tracked with a special label and have a mandated 24-hour review window.

---

# Release Flow

## Feature Proposal → Implementation → Review → Release

### Phase 1: Feature Proposal Intake
1. Feature proposed by any source (agent, contributor, user feedback, product team)
2. Feature Triage Agent triages the proposal: acceptance criteria, priority (P0-P3), feasibility check
3. Product Philosophy Agent reviews for principle compliance (veto possible)
4. System Architect Agent reviews for architectural fit
5. Proposed accepted or rejected with rationale

### Phase 2: Implementation Assignment
1. Feature Triage Agent assigns to implementing agent(s)
2. Implementing agent produces a design brief (1-2 paragraphs) shared with reviewers
3. Cross-agent dependencies identified and coordinated via System Architect
4. Implementation begins on a feature branch

### Phase 3: Cross-Agent Integration
1. IPC contract changes go through API Contract Agent review
2. New UI components go through Design System + Interaction Feel review
3. New D-Bus integrations go through Linux Integration Agent + Security Boundary review
4. All latencies measured and reviewed by Latency Audit Agent
5. Integration Agent writes cross-component tests

### Phase 4: Review Gates (Pre-Merge)
**Mandatory checks before merging to main:**
- [ ] Code compiles with zero warnings (`cargo clippy`, `ruff`)
- [ ] All existing tests pass
- [ ] Integration tests pass (for IPC changes)
- [ ] UI regression tests pass (for UI changes)
- [ ] Fuzz tests pass (for parser changes)
- [ ] Security review completed (for security-relevant changes)
- [ ] Latency budget confirmed (for latency-affected changes)
- [ ] Product Philosophy compliance confirmed (for UX changes)
- [ ] Design System compliance confirmed (for style changes)
- [ ] Owned files reviewed by owning agent
- [ ] Architecture docs and API docs updated

### Phase 5: Stabilization Phase
1. Release Stabilization Agent declares feature freeze
2. All remaining bugs are triaged: release-blocking (must fix) vs. defer
3. Release-blocking fixes prioritized and assigned
4. Full test suite run: unit → integration → UI → fuzz
5. Long-running stability test (24h) validates no memory leaks or crash regressions
6. Latency audit measures all interaction paths against 80ms budget
7. Security audit validates all boundaries
8. Release candidate (RC) built

### Phase 6: Release
1. Release Stabilization Agent makes ship/no-ship decision
2. Changelog/Versioning Agent updates CHANGELOG.md and bumps version
3. Release tagged and artifacts built
4. Installer/Packaging Agent publishes packages
5. Release notes published to GitHub Releases and enaos.tech
6. OSS Onboarding Agent announces to community

### OSS Contribution Path
External contributors follow the same flow, with these additions:

1. **First contribution:** OSS Onboarding Agent provides onboarding support (CONTRIBUTING.md, good-first-issue)
2. **PR submission:** PR template guides the contributor through required information
3. **Review:** At least one core agent reviews the PR. The owning agent must approve
4. **CI passes:** All quality gates must pass automatically
5. **Merge:** Core agent merges after review gates pass
6. **Credit:** Contributor added to release notes

---

# Product Philosophy

## The 8 Principles

Every agent's work must preserve these 8 product principles. Any agent that produces output violating these principles must escalate to Product Philosophy Agent.

### 1. Operational > Conversational
EnaOS is an operating environment, not a chatbot. Interactions are commands, queries, and actions — not conversations. The UI shows results and state, not chat bubbles.

**Practical guidance:**
- The bar shows system context and command results, not a chat history
- No "assistant persona" or avatar
- Commands are executed silently and results shown, not narrated
- The default state is collapsed — the UI is ambient, not demanding attention

### 2. Calm > Noisy
The system never demands attention. Suggestions are ambient, notifications are sparse, and the default state is quiet.

**Practical guidance:**
- One suggestion at a time, auto-dismissed after a few seconds
- Status dot is subtle (12px, no animation unless state changes)
- No sound effects, no popups, no notification badges
- The bar collapses to a minimal pill when not in use
- Information density preference: sparse over packed

### 3. Sparse > Cluttered
Every UI element must earn its place. Empty states are correct. More UI is not better.

**Practical guidance:**
- Command palette shows max 6 suggestions (often 0-3)
- Context display shows exactly what changed, not everything
- Action execution shows a single status line, not a dashboard
- Auto-hide redundant information (battery only when discharging and < 50%)
- Empty is correct — don't fill space with placeholders

### 4. Environment-Centric > App-Centric
The system is aware of the user's workspace, applications, and files — not just the current app. Context spans the entire desktop.

**Practical guidance:**
- Context display shows workspace, focused app, battery, network, media
- Memory captures workspace-wide state, not single-app
- Restoration restores the entire workspace, not just one application
- Suggestions are aware of what's happening across the desktop

### 5. Orchestration > Autonomy
AI proposes, user disposes. Plans require approval before execution. The system never acts autonomously on the user's behalf without permission.

**Practical guidance:**
- All orchestration plans require user approval before execution
- Every action has a `PermissionLevel` and the bar shows what it will do
- The "execution preview" label in the command palette shows the planned action
- Restore shows a full action preview before executing
- No AI agent executes without sandboxing and capability approval

### 6. Trust over "AI Magic"
Everything the system does should be explainable. No black boxes, no "AI magic," no decisions without rationale.

**Practical guidance:**
- The execution preview shows what action will be taken and why
- The action label and source are shown for every suggestion
- The status dot color indicates system state (green = connected, grey = disconnected)
- Error messages are specific and actionable, not "something went wrong"
- System context is transparently displayed — the user can see what the system sees

### 7. Native over Web-Like
EnaOS is a native desktop environment, not a web app. Fonts, rendering, interactions, and performance must feel native.

**Practical guidance:**
- GTK4 native widgets, not HTML/CSS rendered in a webview
- System fonts (Inter on all platforms, SF Mono for code)
- Native keyboard shortcuts, no web-style interactions
- Frame clock-driven animations, not JavaScript timers
- < 80ms perceived latency (native feel)

### 8. Contextual over Interruptive
Information and suggestions are presented in context, not as interrupts. The system responds to the user's current state rather than pushing notifications.

**Practical guidance:**
- Suggestion widget shows one line at a time, auto-dismisses
- Status messages auto-hide after 2-5 seconds
- No modal dialogs except for critical approvals
- Restoration suggestion appears as a compact bar, not a popup
- Command palette appears only when the user types

## Escalation to Product Philosophy Agent

Any agent that believes another agent's output violates these principles should:
1. First attempt direct resolution with the other agent
2. If unresolved, escalate to Product Philosophy Agent with specific principle citations

**Product Philosophy Agent responses:**
- **Upheld:** The violation is confirmed — the offending output must change
- **Overruled:** The output does not violate the principle as interpreted
- **Exception granted:** The violation is acceptable for this specific case (rare, documented)

---

> *This document is the definitive blueprint for the EnaOS engineering organization. Every agent — whether human or automated — operates within these boundaries, responsibilities, and collaboration patterns.*
