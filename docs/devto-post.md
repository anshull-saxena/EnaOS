---
title: "I'm Building an AI-Native Desktop OS in Rust + GTK4 — and I Need Your Help"
description: "EnaOS is an open-source, Linux-native AI operating environment. No Electron, no browser wrappers — real Rust daemons, GTK4 widgets, and Wayland layer-shell surfaces. Developer Preview 0.1 is ready."
published: false
tags: [rust, gtk4, opensource, linux]
canonical_url: https://enaos.tech
cover_image: https://enaos.tech/banner.png
---

> **⚠️ DRAFT — This post needs updating for the v0.1.0-developer-preview release.**
> The IPC protocol section now uses `{"kind": {"type": ..., "body": ...}}` (adjacently-tagged enums),
> the AI Runtime is built and working, and the architecture docs have been updated.
> See [docs/architecture/](architecture/) for the current state.

*This is not another Electron app that calls itself a desktop OS.*
*This is real: Rust daemons, GTK4 widgets, Wayland layer-shell protocols, and a Unix socket IPC bus connecting them.*

---

## The Pitch

I'm building **EnaOS** — an open-source, AI-native desktop operating environment for Linux.

Not a `.Prompt` file wrapper. Not a chatbot in a Chromium frame. Not a SaaS dashboard pretending to be an OS.

The core idea: **the AI should be wired directly into the operating system event bus**, not looking at screenshots of it. When your battery hits 15%, `enad` (the daemon) pushes a `BatteryStatus` event. The bar renders it. The AI knows. When you switch from VSCode to Firefox, `enad` pushes a `WindowFocusChanged` event. The bar shows it. The AI knows.

No screenshots. No hacks. No wrappers. Just real system state, streamed in real time, rendered natively.

I've built the foundation — the daemon, the bar, the IPC layer, seven desktop integration subsystems — and it **compiles and runs**. The Developer Preview 0.1 is ready.

This post is a deep-dive into what's built, what's next, and exactly how you can contribute.

---

## The Architecture (Deep Dive)

EnaOS follows a strictly daemon-driven architecture with exactly two moving parts:

```
┌──────────────────────────────────────────────────────────────┐
│                    Ena Bar (GTK4/libadwaita)                 │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌────────────────┐ │
│  │StatusDot │ │InputEntry │ │MicButton │ │ ContextLabel   │ │
│  │          │ │           │ │          │ │ Focused: VSCode│ │
│  │          │ │           │ │          │ │ | Workspace 2  │ │
│  │          │ │           │ │          │ │ | ⚡87%        │ │
│  └──────────┘ └───────────┘ └──────────┘ └────────────────┘ │
│                            │                                  │
│               Unix Domain Socket (JSON lines)                 │
└────────────────────────────┼──────────────────────────────────┘
                             │
┌────────────────────────────▼──────────────────────────────────┐
│                      enad (Rust daemon)                       │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                    Event Bus                              │ │
│  │          (tokio broadcast — per-kind + catch-all)        │ │
│  └───┬──────┬──────┬──────┬──────┬──────┬──────┬───────────┘ │
│      │      │      │      │      │      │      │              │
│  ┌───▼───┐┌▼─────┐┌▼─────┐┌▼────┐┌▼─────┐┌▼────┐┌▼────────┐ │
│  │UPower │ │NetMgr│ │Window│ │Work-│ │Clip- │ │Notify│ │Audio   │ │
│  │Battery│ │WiFi  │ │Focus │ │space│ │board │ │fdo   │ │+MPRIS  │ │
│  └───────┘ └──────┘ └──────┘ └─────┘ └──────┘ └──────┘ └────────┘ │
│      │         │        │       │       │        │        │       │
│  ┌───▼─────────▼────────▼───────▼───────▼────────▼────────▼────┐ │
│  │              D-Bus + External Tools Layer                    │ │
│  │  zbus │ swaymsg │ hyprctl │ pactl │ wl-paste │ gdbus │ xprop│ │
│  └─────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

### The Daemon: `enad` (Rust + tokio)

`enad` is the heart of EnaOS. Written in Rust, running on tokio, it's a long-lived system daemon that:

- **Owns a Unix socket server** (`runtimes/enad/src/server.rs`) — accepts connections, subscribes clients to event streams
- **Runs a tokio broadcast event bus** (`runtimes/enad/src/bus.rs`) — per-kind channels plus a catch-all
- **Manages subsystem lifecycle** (`runtimes/enad/src/process.rs`) — graceful startup/shutdown, signal handling
- **Integrates with 7 real desktop subsystems** — each one polls or subscribes to real OS state

Each subsystem module is under `runtimes/enad/src/system/`:

| File | Subsystem | Data Source | State Emitted |
|------|-----------|-------------|---------------|
| `upower.rs` | Battery | D-Bus (org.freedesktop.UPower) | Percentage, charging state, time remaining |
| `network.rs` | Network | D-Bus (org.freedesktop.NetworkManager) | WiFi SSID, connectivity status, strength |
| `window.rs` | Window Focus | swaymsg / hyprctl / gdbus | Active window class, title |
| `workspace.rs` | Workspaces | swaymsg / hyprctl | Active workspace name/number |
| `clipboard.rs` | Clipboard | wl-paste (polling) | Last clipboard content hash |
| `notifications.rs` | Notifications | D-Bus (org.freedesktop.Notifications) | App name, summary, body, urgency |
| `audio.rs` | Audio/Media | pactl + MPRIS D-Bus | Volume level, mute state, media metadata |

The IPC protocol is simple but effective — **line-delimited JSON** over a Unix domain socket:

```rust
// From runtimes/enad/src/types/ipc.rs
pub struct IpcMessage {
    pub id: String,
    pub kind: MessageKind,  // Adjacently tagged: { type, body }
}

pub enum MessageKind {
    Subscribe,  // Client: "subscribe me to these event kinds"
    Ping,       // Client/Server: heartbeat
    Pong,       // Client/Server: heartbeat response
    Command,    // Client: "execute this command"
    Event,      // Server: "here's a system event"
}
```

The beauty of this architecture is its **simplicity**. There's no HTTP, no REST, no WebSocket, no gRPC — just a Unix socket and line-delimited JSON. It's fast, it's local, and it's debuggable with `nc` or `socat`.

### The Bar: `ena-bar` (GTK4 + libadwaita + gtk4-layer-shell)

`ena-bar` is the visual frontend — a native GTK4 application that:

- **Creates a Wayland layer-shell surface** using `gtk4-layer-shell` anchored to the bottom of the screen
- **Stays above normal windows** via `set_layer(Overlay)` and `set_exclusive_zone(-1)`
- **Connects to `enad`** over a Unix socket in a background thread
- **Streams events** through an `std::sync::mpsc::channel` → `glib::idle_add_local` pipeline
- **Renders a minimal widget tree** that reflects the current daemon state

The widget hierarchy (`shell/ena-bar/src/bar.rs`):

```
EnaBar (GtkBox, vertical)
├── StatusDot (GtkDrawingArea)    — frame-clock-driven breathing animation
├── InputEntry (GtkEntry)         — command input, Enter to submit
├── MicButton (GtkButton)         — voice input (PipeWire scaffolding)
├── StatusLabel (GtkLabel)        — ephemeral status messages
└── ContextBox (GtkRevealer)      — context display (collapsible)
```

The bar has four states:

```rust
pub(crate) enum BarState {
    Collapsed,  // Minimal — just the status dot
    Expanded,   // Full input + context visible
    Thinking,   // Input hidden, spinner active
    Result,     // Shows command output
}
```

State transitions are triggered **only** by events from the daemon. The bar never invents state, never simulates, never pretends.

### The IPC Client (`shell/ena-bar/src/ipc.rs`)

```rust
pub fn run(socket_path: &str, running: Arc<AtomicBool>, tx: Sender<EnadEvent>) {
    // Connect to Unix socket
    // Spawn reader thread with BufReader
    // Keepalive ping every 1 second
    // Parse JSON lines into EnadEvent
    // Send events through mpsc channel to GTK main loop
    // Reconnect on disconnect with 2-second backoff
}
```

The client runs on a background thread. Events cross the thread boundary through `mpsc::Sender<EnadEvent>` and are dispatched onto the GTK main loop via `glib::idle_add_local`. This means the UI is always responsive — no blocking, no locking, no `unwrap()` in the hot path.

---

## Why Native GTK4 (Not Tauri, Not Electron)

This was the hardest architectural decision, so let me explain the reasoning.

| Dimension | Electron | Tauri + Web | GTK4 / libadwaita (Rust) |
|-----------|----------|-------------|--------------------------|
| **RAM at idle** | 100-200 MB | 40-80 MB | **8-15 MB** |
| **Startup time** | 1-3 seconds | 300-800ms | **<100ms** |
| **Animation framework** | JS requestAnimationFrame | Web compositor | **GtkFrameClock / GtkAnimation** |
| **Desktop integration** | Shallow (DBus via bridge) | Better (Rust backend) | **Full (D-Bus directly, no bridge)** |
| **Wayland layer-shell** | No native support | Possible via plugin | **First-class (gtk4-layer-shell)** |
| **OS-native theming** | CSS emulation | CSS emulation | **AdwStyleManager / libadwaita** |
| **Memory safety** | V8 GC pauses | V8 GC pauses | **Zero GC, Rust ownership** |

The bar is a system-level component. It should boot in milliseconds and consume negligible RAM. It should feel like part of the compositor, not an application on top of it. That means **no browser engine, no JavaScript runtime, no HTML renderer**.

The Rust GTK4 bindings (`gtk4-rs` v0.11) are remarkably mature. They give us:
- `GtkFrameClock::add_tick_callback` for compositor-synced animations
- `GtkRevealer` with GPU-accelerated transitions
- Direct `gtk4-layer-shell` integration via `IsLayerShell` trait
- CSS theming that respects the system dark mode preference
- `glib::MainContext::channel` for thread-safe event dispatch

The cost? A steeper learning curve. GTK4 + Rust has fewer tutorials than Tauri + React. But for this use case — a permanent, always-running OS-level overlay — native is the only correct choice.

---

## What's Built (Developer Preview 0.1)

The following code compiles, runs, and works today on Wayland:

✅ **`enad` daemon** — 3,000+ lines of Rust across 18 modules. Event bus, Unix socket server, 22 IPC commands, 28 events, orchestration engine, snapshot store, context engine, suggestion engine, working memory, first-run management.

✅ **`ena-bar` GTK4 frontend** — 1,800+ lines of Rust + CSS across 10 modules. Widget tree, IPC client, 4-state machine, command palette, restoration widget, orchestration timeline, ambient suggestions, welcome overlay.

✅ **AI Runtime** — Python FastAPI server with Ollama integration, enad bridge, streaming SSE, orchestration plan parser. Live.

✅ **IPC protocol** — 71 tests including round-trip serde, wire-format compatibility, integration tests. Zero regressions.

✅ **Desktop integration** — UPower, NetworkManager, window tracking (Sway/Hyprland/GNOME), workspace, clipboard, notifications, audio/MPRIS.

✅ **Build system** — CI pipeline (GitHub Actions), release config (LTO, small binaries), conditional Linux dependencies.

---

## What I Need Help Building

Here's the honest roadmap. The foundation is solid. Everything else needs builders.

### 🔴 Critical (Immediate Help Needed)

**1. Complete the Desktop Integration Subsystems**
The 7 subsystem modules in `runtimes/enad/src/system/` work but need production-quality polish:
- Window tracking fallback chain needs exhaustively testing on all 3 compositors
- Clipboard polling should use `inotify` instead of timer-based polling
- Notification signal subscription needs race-free initialization
- Network SSID encoding varies by locale — needs handling

*Skills needed: Rust, tokio async, D-Bus (zbus), Linux system programming*

**2. Flatpak Packaging**
Currently requires manual build. A Flatpak would let users install EnaOS on any distro with a single command.

*Skills needed: Flatpak, Linux packaging*

**3. Screenshots & Demos**
The project needs real screenshots and demo GIFs for the GitHub README and release page. See `scripts/release-assets.sh` for a capture guide.

*Skills needed: Wayland, screen recording, documentation*

### 🟡 Medium Priority

**4. Auto-Snapshot Loop**
Periodic workspace snapshots — configurable interval, event-triggered capture. Currently only manual snapshots.

**5. Global Keyboard Shortcut**
`Super+Space` to summon/hide the bar from any application via D-Bus global shortcut.

**6. AI Runtime Auto-Start**
enad should manage the AI Runtime process lifecycle (start/stop/restart). Currently manual.

### 🟢 Nice-to-Have

**7. PipeWire Audio Capture** — Voice input for the mic button
**8. WASM Plugin SDK** — Extension system for third-party capabilities
**9. Cross-Distro Packages** — deb, rpm, pacman

---

## How to Contribute

### 🚀 Quick Start (5 minutes)

```bash
# 1. Clone and build
git clone https://github.com/anshull-saxena/EnaOS.git
cd EnaOS/runtimes/enad && cargo build --release
cd ../shell/ena-bar && cargo build --release

# 2. Run (requires Wayland compositor)
./runtimes/enad/target/release/enad --socket /tmp/enad.sock
./shell/ena-bar/target/release/ena-bar --socket-path /tmp/enad.sock
```

### 🛠 Prerequisites

| Package | Purpose | Install (Debian/Ubuntu) |
|---------|---------|------------------------|
| Rust 1.75+ | Compiler | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` |
| GTK4 dev | UI framework | `apt install libgtk-4-dev libadwaita-1-dev` |
| gtk4-layer-shell | Wayland overlay | `apt install libgtk4-layer-shell-dev` |
| wl-clipboard | Clipboard (optional) | `apt install wl-clipboard` |

### 📋 Where to Start

| Your Skills | Best Entry Point | File |
|-------------|------------------|------|
| Rust, async, Linux | Desktop integration | `runtimes/enad/src/system/*.rs` |
| Rust, GTK4 | Bar widgets | `shell/ena-bar/src/bar.rs` |
| Python, LLMs | AI Runtime | `runtimes/ai-runtime/src/` |
| Linux, testing | E2E testing | `runtimes/enad/src/tests.rs` |
| Documentation | Docs | `docs/` |

### 🏗 How We Work

- **Trunk-based development** — short-lived branches from `main`
- **Conventional commits** — `feat:`, `fix:`, `docs:`, `refactor:`, `test:`
- **Code review required** — PRs need at least one review
- **MIT License** — your contributions belong to everyone

---

## The Philosophy

EnaOS is built on a few convictions:

### 1. The Desktop Should Be the AI Interface

Not the browser. Not a chatbot widget on a SaaS site. The **operating system itself** should understand context and surface AI capabilities natively.

### 2. Native Over Web — Always

For system-level components, the browser is the wrong tool. Electron apps consume 10x the resources. EnaOS uses GTK4 because the bar is a **system component**, not an app.

### 3. Real State, Not Simulated UI

The bar never invents state. No spinners, no skeletons, no "loading..." — just real system state or nothing.

### 4. Local-First, Privacy-Preserving

Designed for local inference via Ollama. Your system state never leaves your machine.

---

## The Ask

Here's what I need from you:

**If you write Rust** — Open issues in `runtimes/enad/src/system/`. Pick a subsystem, implement it, open a PR.

**If you know GTK4** — The bar needs polishing. Animations, accessibility, theming.

**If you use Linux on Wayland** — Test the bar on your compositor. Report what works.

**If you write docs** — The docs need to grow as the codebase grows.

**If you just want to follow along** — Star the repo. Share this post.

---

## The Roadmap

```
Milestone 0 — Foundation ✅
├── Rust daemon (enad) with event bus + IPC ✅
├── GTK4 frontend (ena-bar) with layer-shell ✅
├── AI Runtime with Ollama integration ✅
├── 71 IPC round-trip tests ✅
├── 7 desktop integration subsystems ✅
└── Developer Preview 0.1 ✅

Milestone 1 — Integration (Current)
├── Complete all 7 desktop subsystems
├── Flatpak packaging
├── Auto-snapshot loop
├── Global keyboard shortcut
└── AI Runtime auto-start

Milestone 2 — Intelligence (Future)
├── Context-aware prompt injection
├── Multi-agent orchestration
├── Plugin SDK (WASM)
└── Stable release
```

---

## Links

- **Website:** [enaos.tech](https://enaos.tech)
- **GitHub:** [github.com/anshull-saxena/EnaOS](https://github.com/anshull-saxena/EnaOS)
- **License:** MIT

---

![EnaOS Logo](https://enaos.tech/logo.png)

*EnaOS is built by developers who believe the desktop OS should be the AI interface — not the browser on top of it.*

*If that resonates, please star the repo, share this post, or open your first PR. Every contribution matters.*
