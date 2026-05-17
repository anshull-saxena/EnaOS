---
title: "I'm Building an AI-Native Desktop OS in Rust + GTK4 — and I Need Your Help"
description: "EnaOS is an open-source, Linux-native AI operating environment. No Electron, no browser wrappers — real Rust daemons, GTK4 widgets, and Wayland layer-shell surfaces. I've built the foundation. Now I need contributors to finish the vision."
published: false
tags: [rust, gtk4, opensource, linux]
canonical_url: https://enaos.tech
cover_image: https://enaos.tech/banner.png
---

*This is not another Electron app that calls itself a desktop OS.*
*This is real: Rust daemons, GTK4 widgets, Wayland layer-shell protocols, and a Unix socket IPC bus connecting them.*

---

## The Pitch

I'm building **EnaOS** — an open-source, AI-native desktop operating environment for Linux.

Not a `.Prompt` file wrapper. Not a chatbot in a Chromium frame. Not a SaaS dashboard pretending to be an OS.

The core idea: **the AI should be wired directly into the operating system event bus**, not looking at screenshots of it. When your battery hits 15%, `enad` (the daemon) pushes a `BatteryStatus` event. The bar renders it. The AI knows. When you switch from VSCode to Firefox, `enad` pushes a `WindowFocusChanged` event. The bar shows it. The AI knows.

No screenshots. No hacks. No wrappers. Just real system state, streamed in real time, rendered natively.

I've built the foundation — the daemon, the bar, the IPC layer, seven desktop integration subsystems — and it **compiles and runs**. But there's a mountain of work ahead, and I need help.

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
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    pub body: Option<serde_json::Value>,
}

pub enum MessageType {
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
- **Stays above normal windows** via `set_layer(SWAY_LAYER_SHELL_LAYER_OVERLAY)` and `set_exclusive_zone(-1)`
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
pub fn run(socket_path: &str, tx: Sender<EnadEvent>, running: Arc<AtomicBool>) {
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

## What's Built (The Foundation)

The following code compiles, runs, and works today on Wayland:

✅ **`enad` daemon** — 2,800+ lines of Rust across 10 modules. Event bus, Unix socket server, process lifecycle, signal handling. All 7 subsystem stubs with D-Bus integration logic.

✅ **`ena-bar` GTK4 frontend** — 1,200+ lines of Rust + CSS across 6 modules. Widget tree, IPC client, 4-state state machine, frame clock animation, keyboard shortcuts, mic scaffolding.

✅ **IPC protocol** — JSON-line Unix socket protocol with Subscribe/Ping/Pong/Command/Event message types. Keepalive, auto-reconnect, per-kind subscription filtering.

✅ **Desktop integration stubs** — UPower (D-Bus), NetworkManager (D-Bus), window tracking (swaymsg/hyprctl/gdbus), workspace tracking, clipboard, notifications, audio.

✅ **Build system** — Cargo workspace, GTK4 with v4_22 features, gtk4-layer-shell 0.8 on Linux, macOS fallback for development.

✅ **Clean build** — Zero errors, zero warnings on `cargo build`.

---

## What I Need Help Building

Here's the honest roadmap. The foundation is solid. Everything else needs builders.

### 🔴 Critical (Immediate Help Needed)

**1. Complete the Desktop Integration Subsystems**
The 7 subsystem modules in `runtimes/enad/src/system/` have solid D-Bus scaffolding but need production-quality implementation:
- `window.rs` — needs multi-compositor detection (Sway vs Hyprland vs GNOME)
- `clipboard.rs` — needs efficient polling without CPU spin
- `notifications.rs` — needs proper D-Bus signal subscription
- `network.rs` — needs signal strength normalization + SSID encoding
- `upower.rs` — needs online/offline transition handling
- `audio.rs` — needs PulseAudio volume events + MPRIS media metadata
- `workspace.rs` — needs compositor-specific JSON parsing

*Skills needed: Rust, tokio async, D-Bus (zbus), Linux system programming*

**2. End-to-End Integration Testing**
Currently each subsystem compiles but hasn't been tested end-to-end with a real `enad` + `ena-bar` session. We need:
- A test script that starts `enad`, connects `ena-bar`, subscribes to events, and verifies the pipeline
- CI integration with GitHub Actions
- A mock daemon for testing the bar in isolation (without a real Wayland compositor)

*Skills needed: Linux, shell scripting, CI/CD, testing*

**3. Real-Time System Context Card**
The `ena-bar` context label should display a rich, formatted view of system state:
- Focused application name + icon
- Workspace name/number
- Battery percentage with visual indicator
- Network SSID + signal strength
- Unread notification count

Currently it shows raw text. We need a proper widget.

*Skills needed: GTK4, Rust, UI design*

### 🟡 Medium Priority

**4. AI Runtime (`runtimes/ai-runtime/`)**
This is the next major layer — a Python (or Rust) service that:
- Connects to Ollama for local LLM inference
- Injects system context into prompts (focused window, clipboard, battery, etc.)
- Streams responses back through `enad` → `ena-bar`
- Supports tool-calling (execute shell commands, open files, search web)

This module doesn't exist yet. It needs to be designed and built from scratch.

*Skills needed: Python or Rust, LLM inference (Ollama), prompt engineering, FastAPI*

**5. Wayland Compositor Compatibility Testing**
We need to verify `ena-bar` works correctly on:
- GNOME (Mutter)
- Sway (wlroots)
- Hyprland (wlroots-hyprland)
- KDE (KWin)
- River
- niri

Each compositor handles layer-shell differently. We need test reports and fixes.

*Skills needed: Wayland, various compositors, GTK4*

**6. Global Keyboard Shortcut**
Currently the bar receives keyboard events only when focused. A true global shortcut (e.g., `Super+Space`) requires:
- Linux: D-Bus global shortcut registration (or compositor-specific protocol)
- macOS: Global hotkey API
- The shortcut should summon/hide the bar from any application

*Skills needed: D-Bus, Wayland protocols, platform-specific APIs*

**7. PipeWire Audio Capture**
The mic button exists but does nothing. Proper voice input requires:
- PipeWire stream capture in Rust (pipewire-rs)
- Audio level visualization in the bar
- Voice activity detection (VAD) for push-to-talk

*Skills needed: PipeWire, Rust, audio processing*

### 🟢 Nice-to-Have

**8. Developer Preview ISO** — NixOS-based live image with `enad` + `ena-bar` pre-installed
**9. Flatpak packaging** — For distribution-independent installation
**10. Plugin SDK** — WASM-based extension system
**11. Memory Engine** — PostgreSQL + pgvector for persistent AI context
**12. Agent Engine** — Multi-agent orchestration with baton-passing

---

## How to Contribute

### 🚀 Quick Start (5 minutes)

```bash
# 1. Clone the repository
git clone https://github.com/anshull-saxena/EnaOS.git
cd EnaOS

# 2. Build the daemon
cd runtimes/enad
cargo build --release

# 3. Build the bar
cd shell/ena-bar
cargo build --release

# 4. Run them (requires a Wayland compositor)
./runtimes/enad/target/release/enad --socket /tmp/enad.sock
./shell/ena-bar/target/release/ena-bar --socket-path /tmp/enad.sock
```

### 🛠 Prerequisites

| Package | Purpose | Install (Debian/Ubuntu) | Install (Fedora) | Install (Arch) |
|---------|---------|------------------------|------------------|----------------|
| Rust 1.75+ | Compiler | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` | same | same |
| GTK4 dev | UI framework | `apt install libgtk-4-dev` | `dnf install gtk4-devel` | `pacman -S gtk4` |
| libadwaita | GTK4 widgets | `apt install libadwaita-1-dev` | `dnf install libadwaita-devel` | `pacman -S libadwaita` |
| gtk4-layer-shell | Wayland overlay | `apt install libgtk4-layer-shell-dev` | `dnf install gtk4-layer-shell-devel` | `pacman -S gtk4-layer-shell` |
| pkg-config | Build tooling | `apt install pkg-config` | `dnf install pkgconfig` | `pacman -S pkgconf` |
| UPower | Battery (optional) | `apt install upower` | `dnf install upower` | `pacman -S upower` |
| NetworkManager | Network (optional) | `apt install network-manager` | `dnf install NetworkManager` | `pacman -S networkmanager` |
| wl-clipboard | Clipboard (optional) | `apt install wl-clipboard` | `dnf install wl-clipboard` | `pacman -S wl-clipboard` |

### 📋 Where to Start

| Your Skills | Best Entry Point | File |
|-------------|------------------|------|
| Rust, async, Linux | Desktop integration | `runtimes/enad/src/system/*.rs` |
| Rust, GTK4 | Bar widgets, animations | `shell/ena-bar/src/bar.rs` |
| GTK4, CSS | Styling, theming | `shell/ena-bar/src/style.css` |
| Linux, testing | End-to-end testing | `runtimes/enad/src/main.rs` |
| Python, LLMs | AI Runtime | `runtimes/ai-runtime/` (empty) |
| DevOps, Nix | CI, packaging | `.github/` + `infrastructure/` |
| Documentation | README, docs | `docs/architecture/*.md` |

### 🏗 How We Work

- **Trunk-based development** — short-lived branches from `main`
- **Conventional commits** — `feat:`, `fix:`, `docs:`, `refactor:`, `test:`
- **Code review required** — PRs need at least one review before merge
- **No spec work without implementation** — we ship code, not proposals
- **License:** MIT — your contributions belong to everyone

---

## The Philosophy

EnaOS is built on a few convictions:

### 1. The Desktop Should Be the AI Interface

Not the browser. Not a chatbot widget on a SaaS site. The **operating system itself** should understand context and surface AI capabilities natively. The OS knows what you're doing. The OS knows what files you have. The OS controls audio, clipboard, notifications, and hardware. That's the integration surface AI needs — not a screenshot of it.

### 2. Native Over Web — Always

For system-level components, the browser is the wrong tool. Electron apps are not desktop software — they're websites pretending to be desktop software. They consume 10x the resources, feel 0.5x as responsive, and integrate with the OS through leaky abstractions.

EnaOS uses GTK4 because the bar is a **system component**, not an app. It should boot in milliseconds, consume single-digit megabytes, and feel as native as GNOME Shell itself.

### 3. Real State, Not Simulated UI

The bar never invents state. If the daemon disconnects, the status dot turns grey. If the battery module hasn't emitted an event, no battery indicator appears. No spinners, no skeletons, no "loading..." — just real system state or nothing.

This discipline keeps the architecture honest. Every widget in the bar corresponds to a real daemon event. If a widget exists without a corresponding event source, that's a bug.

### 4. Local-First, Privacy-Preserving

EnaOS is designed for local inference via Ollama. Your system state — focused windows, clipboard contents, workspace layout — never leaves your machine. Cloud AI is an optional fallback, not the default.

---

## The Ask

Here's what I need from you:

**If you write Rust** — There are open issues in `runtimes/enad/src/system/` that need your eyes. D-Bus integration, tokio async, Linux system programming. Pick a subsystem, implement it, open a PR.

**If you know GTK4** — The bar needs polishing. Animations, accessibility, widget architecture, theming. `shell/ena-bar/src/bar.rs` is the main file.

**If you're into AI/LLMs** — The AI Runtime needs to be designed from scratch. How do we inject system context into prompts? How do we handle tool-calling safely? Join [#architecture discussions](https://github.com/anshull-saxena/EnaOS/discussions).

**If you love Linux desktops** — Test the bar on your compositor. Report what works and what doesn't. Help us build the compatibility matrix.

**If you write docs** — The architecture documents need to evolve as the codebase grows. Help others understand the system.

**If you just want to follow along** — Star the repo. Share this post. The more people watching, the faster this grows.

---

## The Roadmap

```
Phase 1 (Current) — Foundation ✅
├── Rust daemon (enad) with event bus + IPC ✅
├── GTK4 frontend (ena-bar) with layer-shell ✅
├── 7 desktop integration subsystems (stubs) ✅
└── MIT licensed on GitHub ✅

Phase 2 (Next) — Integration
├── Complete all 7 desktop subsystems
├── CI pipeline + end-to-end tests
├── AI Runtime (Ollama integration)
├── Global keyboard shortcut
└── Flatpak packaging

Phase 3 — Intelligence
├── Context-aware prompt injection
├── Multi-agent orchestration
├── Memory engine (pgvector)
└── Developer Preview ISO

Phase 4 — Ecosystem
├── Plugin SDK (WASM)
├── Agent marketplace
├── Native Settings + Files apps
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
