# EnaOS Quickstart Guide

> **Goal:** Install, run, and experience EnaOS within 5 minutes.

## Prerequisites

You need a **Linux desktop with Wayland** (GNOME, Sway, or Hyprland). EnaOS does not work on X11 or macOS for the GTK4 bar (macOS is supported for development of the daemon only).

### Install system dependencies

**Debian/Ubuntu:**
```bash
sudo apt-get install -y \
  libgtk-4-dev libadwaita-1-dev libgtk4-layer-shell-dev \
  pkg-config upower network-manager wl-clipboard
```

**Fedora:**
```bash
sudo dnf install -y \
  gtk4-devel libadwaita-devel gtk4-layer-shell-devel \
  pkgconfig upower NetworkManager wl-clipboard
```

**Arch Linux:**
```bash
sudo pacman -S --noconfirm \
  gtk4 libadwaita gtk4-layer-shell \
  pkg-config upower networkmanager wl-clipboard
```

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

---

## Build & Run

### 1. Build the daemon (`enad`)

```bash
git clone https://github.com/anshull-saxena/EnaOS.git
cd EnaOS/runtimes/enad
cargo build --release   # First build: 2-3 minutes
```

### 2. Build the GTK4 bar (`ena-bar`)

```bash
cd ../shell/ena-bar
cargo build --release   # First build: 2-3 minutes
```

### 3. Start the daemon

```bash
./target/release/enad --socket /tmp/enad.sock
```

You should see: `enad ready — awaiting commands on /tmp/enad.sock`

### 4. Start the bar (in another terminal)

```bash
./target/release/ena-bar --socket-path /tmp/enad.sock
```

The bar appears at the bottom of your screen. The status dot turns green when connected.

### 5. Try your first commands

- **Type** `open browser` → command palette shows suggestions
- **Type** `create a snapshot` → saves your workspace state
- **Press Escape** → collapse the bar
- **Click** the "Continue: ..." suggestion → triggers workspace restoration

---

## Troubleshooting

### Bar doesn't appear

| Symptom | Cause | Fix |
|---------|-------|-----|
| Bar window doesn't map | Compositor doesn't support `wlr-layer-shell` | Try Sway or Hyprland. GNOME needs [gtk4-layer-shell extension](https://github.com/wmww/gtk4-layer-shell) |
| "Failed to connect to socket" | enad not running | Start enad first: `./runtimes/enad/target/release/enad` |
| Bar shows grey dot | enad disconnected | Check enad is running, or restart it |

### Daemon issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| "Address already in use" | Stale socket file | `rm -f /tmp/enad.sock` |
| DBus integration warnings | Service not installed | `sudo apt install upower network-manager` |
| Window tracking shows nothing | Unsupported compositor | Currently supports Sway, Hyprland, GNOME |

### AI Runtime issues

The AI runtime is optional for Developer Preview. The bar and daemon work without it.

```bash
# Start Ollama first
ollama serve

# Then start AI runtime
cd runtimes/ai-runtime
pip install -r requirements.txt
python3 -m src.main
```

| Symptom | Cause | Fix |
|---------|-------|-----|
| "Ollama not running" | Ollama not started | `ollama serve` |
| "enad not connected" | enad not started on port 8900 | Start enad first |
| No streaming response | Model not pulled | `ollama pull llama3.2` |

### Build issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| `pkg-config` not found | Missing pkg-config | `sudo apt install pkg-config` |
| `gtk4` not found | Missing GTK4 dev libs | Install deps from table above |
| `gtk4-layer-shell` not found | Missing layer-shell lib | Install `gtk4-layer-shell-dev` |

---

## What to expect on first launch

1. **Welcome overlay** appears (crossfade animation) with 3 suggestion chips
2. **Click a chip** (e.g., "Open Browser") → overlay dismisses, command executes
3. **Or press Escape** → overlay dismisses
4. **Type** in the bar → suggestions appear within 40ms
5. **System context** shows at the bottom: focused app, workspace, battery, WiFi
6. **Restoration suggestion** appears if snapshots exist: "Continue: [workspace]"
7. **Ambient suggestions** appear after window focus changes

---

## Verify it's working

```bash
# Check enad health (Unix socket)
echo '{"id":"00000000-0000-0000-0000-000000000000","kind":{"type":"Ping","body":null}}' | nc -U /tmp/enad.sock

# Expected response:
# {"id":"...","kind":{"type":"Response","body":{"Data":{"payload":{"code":"PONG","latency_ms":...}}}}}

# Check AI runtime health
curl http://localhost:8900/health

# Expected:
# {"status":"healthy","enad_connected":true,"ollama_available":true,"model":"llama3.2"}
```

---

## What's next?

- Read the [Architecture Overview](architecture/01_OVERVIEW.md)
- Check the [Changelog](../CHANGELOG.md)
- Browse the codebase: `runtimes/enad/src/` for the daemon, `shell/ena-bar/src/` for the bar
- See [CONTRIBUTING.md](../CONTRIBUTING.md) to get involved
