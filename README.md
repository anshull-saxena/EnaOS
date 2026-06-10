<div align="center">

  <img src="docs/assets/banner.png" alt="EnaOS Banner" width="100%">

  <!--
    SCREENSHOTS — Replace these placeholders once captured.
    Run: bash scripts/release-assets.sh --all
  -->
  <!-- <img src="docs/assets/screenshots/hero.png" alt="EnaOS Bar" width="80%"> -->

  <br>

  <p>
    <a href="https://github.com/anshull-saxena/EnaOS/stargazers">
      <img src="https://img.shields.io/github/stars/anshull-saxena/EnaOS?style=flat-square&color=gold" alt="GitHub stars">
    </a>
    <a href="https://github.com/anshull-saxena/EnaOS/blob/main/LICENSE">
      <img src="https://img.shields.io/github/license/anshull-saxena/EnaOS?style=flat-square" alt="MIT License">
    </a>
    <a href="https://github.com/anshull-saxena/EnaOS/actions">
      <img src="https://img.shields.io/github/actions/workflow/status/anshull-saxena/EnaOS/ci.yml?style=flat-square" alt="CI">
    </a>
    <img src="https://img.shields.io/badge/GTK4-purple?style=flat-square&logo=gtk" alt="GTK4">
    <img src="https://img.shields.io/badge/Wayland-teal?style=flat-square" alt="Wayland">
    <img src="https://img.shields.io/badge/Rust-orange?style=flat-square&logo=rust" alt="Rust">
    <img src="https://img.shields.io/badge/Python-blue?style=flat-square&logo=python" alt="Python">
    <img src="https://img.shields.io/badge/Ollama-black?style=flat-square&logo=ollama" alt="Ollama">
  </p>

  <h1>EnaOS</h1>
  <h3>AI-native operating environment for Linux</h3>

  <p>
    Instead of launching apps,<br>
    EnaOS understands context,<br>
    remembers work,<br>
    and restores workflows.
  </p>

  <br>

  <p><strong>Install in one command:</strong></p>
  <pre>curl -fsSL https://enaos.tech/install.sh | bash</pre>

</div>

<br>

---

## Why EnaOS?

AI assistants today work inside a browser window. They see a screenshot of your desktop. They don't know what you were doing before you asked for help. They can't act — they can only suggest.

EnaOS is different. It's **native to the operating system** — a GTK4 bar that lives on your Wayland desktop, a Rust daemon that monitors your system state, and a Python AI runtime that understands context.

It knows which app you're focused on, which workspace you're in, your battery status, network state, recent activity, and saved workspaces. It can restore what you were working on, execute plans with visible progress, and suggest relevant actions — all without leaving the desktop.

**No browser. No Electron. No cloud dependency.**

---

## Capabilities

### ⌨️ Context-Aware Command Palette

Type naturally in the bar. Suggestions adapt in real time based on:

- **Active workspace** — what you're working on
- **Focused application** — which app is in front
- **Recent activity** — what you've done recently
- **Previous workflows** — saved plans and snapshots

Commands execute through the Rust daemon — not a shell script. Results appear inline.

<br>

### 🔄 Workspace Continuity

Interrupted? Close your laptop, reboot, crash. When you come back:

**One click restores your environment:**

- Applications reopen in their workspaces
- Terminal sessions resume
- Browser tabs return to context
- Project context is preserved

No more reconstructing your setup from memory every morning.

<br>

### 🧠 Orchestration Engine

Complex tasks become visible execution plans. Type "set up my dev environment" and EnaOS:

1. Generates a step-by-step plan (open editor, start server, open docs)
2. Shows you the plan with preview before execution
3. Executes each step with visible progress
4. Reports success or rolls back on failure

Every action is observable. Nothing runs in the background without your knowledge.

<br>

### 💾 Restoration System

Workspace snapshots preserve your full desktop state:

- Open windows and their positions
- Terminal sessions and working directories
- Active projects and contexts
- Browser tabs and documentation

Snapshots can be restored fully or partially — pick which applications to reopen.

<br>

### 🖥️ Native Linux Integration

Built directly against Linux desktop infrastructure — no abstractions, no bridges:

| Subsystem | Integration |
|-----------|-------------|
| **Window tracking** | Sway, Hyprland, GNOME (fallback chain) |
| **Power** | UPower (battery percentage, charging state) |
| **Network** | NetworkManager (SSID, signal strength) |
| **Audio** | PulseAudio / PipeWire + MPRIS media |
| **Clipboard** | wl-clipboard (content monitoring) |
| **Notifications** | Freedesktop notification listener |

Every subsystem degrades gracefully — if a service isn't available, EnaOS logs and continues.

<br>

### 🔮 Ambient Intelligence

The bar surfaces proactive suggestions based on system state:

- **Continuity** — "Resume your development workspace from 2 hours ago"
- **Context** — "You just opened a terminal — need a dev server?"
- **Time of day** — "Good morning — ready to pick up where you left off?"
- **Workflow** — "You quit the browser but the build is still running"

Suggestions are non-intrusive, rate-limited, and dismissible.

<br>

---

## Screenshots

<!--
  Screenshots will be captured using scripts/release-assets.sh
  on a Wayland compositor. Placeholder references below.
  Replace with actual image tags once assets exist at those paths.
-->

<table>
  <tr>
    <td align="center"><img src="docs/assets/screenshots/onboarding.png" alt="Onboarding" width="280"><br><em>First-run welcome overlay</em></td>
    <td align="center"><img src="docs/assets/screenshots/command-palette.png" alt="Command palette" width="280"><br><em>Context-aware suggestions</em></td>
    <td align="center"><img src="docs/assets/screenshots/orchestration.png" alt="Orchestration" width="280"><br><em>Execution plan timeline</em></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/assets/screenshots/restoration.png" alt="Restoration" width="280"><br><em>Workspace restoration preview</em></td>
    <td align="center"><img src="docs/assets/screenshots/ambient-suggestions.png" alt="Ambient suggestions" width="280"><br><em>Proactive suggestions</em></td>
    <td></td>
  </tr>
</table>

> **Screenshots pending capture.** Run `bash scripts/release-assets.sh --all` on a Wayland compositor to generate them.

---

## Install

### One-command install (recommended)

```bash
curl -fsSL https://enaos.tech/install.sh | bash
```

The installer detects your environment, installs dependencies, builds components, and creates a launch script.

### Or build from source

```bash
git clone https://github.com/anshull-saxena/EnaOS.git
cd EnaOS

# Build the daemon and bar
cd runtimes/enad && cargo build --release
cd ../shell/ena-bar && cargo build --release

# Install AI runtime dependencies
cd ../ai-runtime && pip install -r requirements.txt

# Run (requires Wayland compositor)
./runtimes/enad/target/release/enad --socket /tmp/enad.sock
./shell/ena-bar/target/release/ena-bar --socket-path /tmp/enad.sock
```

### Prerequisites

- **Linux** with Wayland (GNOME, Sway, or Hyprland)
- **Rust** 1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- **GTK4** development libraries (`libgtk-4-dev libadwaita-1-dev libgtk4-layer-shell-dev`)
- **Python** 3.11+ (for AI runtime, optional but recommended)
- **Ollama** (for local AI inference, optional)

See [docs/QUICKSTART.md](docs/QUICKSTART.md) for distribution-specific setup instructions.

---

## Developer Preview

EnaOS is currently released as **v0.1.0-developer-preview**.

Current focus:

- **Stability** — hardening the daemon and IPC layer (71 tests passing)
- **Reliability** — graceful degradation when subsystems are unavailable
- **User feedback** — validating the interaction model with real usage
- **Linux ecosystem validation** — testing across compositors and distributions

### What's ready

| Feature | Status |
|---------|--------|
| Native GTK4 bar with Wayland layer-shell | ✅ Release 0.1.0 |
| Context-aware command palette | ✅ Release 0.1.0 |
| Workspace snapshots and restoration | ✅ Release 0.1.0 |
| Orchestration engine with plan execution | ✅ Release 0.1.0 |
| Ambient intelligence suggestions | ✅ Release 0.1.0 |
| Desktop integration (battery, network, audio, clipboard, notifications) | ✅ Release 0.1.0 |
| AI runtime with Ollama integration | ✅ Release 0.1.0 |
| First-run onboarding with demo data | ✅ Release 0.1.0 |
| IPC round-trip tests (71) | ✅ Release 0.1.0 |

### Coming next

| Feature | Priority |
|---------|----------|
| Auto-snapshot loop (periodic workspace saves) | High |
| Flatpak packaging | High |
| Global keyboard shortcut (Super+Space) | Medium |
| AI Runtime auto-start by daemon | Medium |
| Multi-compositor window tracking verification | Medium |
| Plugin SDK (WASM-based) | Future |
| Stable API with versioning | Future |

---

## How it works

EnaOS has three components:

```
┌──────────────────────────────────────────────────────────┐
│                    Ena Bar (GTK4)                        │
│     Command palette · Restoration · Timeline · Context   │
│         Ambient suggestions · Welcome overlay            │
└───────────────────────────┬──────────────────────────────┘
                            │ Unix socket (JSON)
┌───────────────────────────▼──────────────────────────────┐
│                    enad (Rust daemon)                     │
│     IPC server · Event bus · Orchestration · Snapshots   │
│     Context engine · Memory · Suggestions · Onboarding   │
│     ┌────────────────────────────────────────────────┐   │
│     │  D-Bus · swaymsg · hyprctl · pactl · wl-paste  │   │
│     └────────────────────────────────────────────────┘   │
└───────────────────────────┬──────────────────────────────┘
                            │ Unix socket (JSON)
┌───────────────────────────▼──────────────────────────────┐
│                 AI Runtime (Python)                       │
│     FastAPI · Ollama · Context injection · SSE streaming  │
└──────────────────────────────────────────────────────────┘
```

**Ena Bar** — A native GTK4 application that renders system state as widgets. Stateless renderer: all business logic lives in the daemon.

**enad** — The core Rust daemon. Owns the event bus, desktop integration (7 subsystems), orchestration engine, workspace snapshots, context-aware suggestions, ambient intelligence, and first-run management. The only component with system access.

**AI Runtime** — An optional Python FastAPI server that provides LLM-powered features via Ollama. Injects live desktop context into prompts. Never executes OS commands directly.

All communication uses a Unix domain socket with line-delimited JSON — no HTTP, no gRPC, no web sockets.

---

## Project structure

```
EnaOS/
├── runtimes/
│   ├── enad/              # Rust system daemon
│   └── ai-runtime/        # Python AI inference layer
├── shell/
│   └── ena-bar/           # Native GTK4 bar (Rust)
├── docs/
│   ├── architecture/      # System architecture documents
│   └── QUICKSTART.md      # Detailed setup guide
├── scripts/               # Installer, deploy scripts
├── CHANGELOG.md           # Release notes
└── CONTRIBUTING.md        # Development guide
```

---

## Design principles

- **Real state only** — Every bar element reflects actual OS state. No simulated UI, no fake workflows.
- **Daemon-driven** — The frontend is a thin renderer. All business logic lives in enad.
- **Graceful degradation** — If a subsystem is unavailable, it logs and continues. enad never crashes.
- **Compositor-agnostic** — Window tracking works on GNOME, Sway, and Hyprland with automated fallback.
- **Local-first** — Designed for local inference via Ollama. Cloud is optional, never required.
- **Privacy-preserving** — System state never leaves your machine. AI queries go to Ollama locally.

---

## Contributing

EnaOS is an early-stage open-source project. Contributions of all kinds are welcome.

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, debugging guide, and pull request process.

### Quick links

- [Quickstart guide](docs/QUICKSTART.md)
- [Architecture overview](docs/architecture/01_OVERVIEW.md)
- [Changelog](CHANGELOG.md)
- [Bug reports](.github/ISSUE_TEMPLATE/bug_report.md)
- [Feature requests](.github/ISSUE_TEMPLATE/feature_request.md)

---

## License

MIT License. See [LICENSE](LICENSE).
