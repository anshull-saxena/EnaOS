# 5. Security, Infrastructure, and Build

> **Status:** Accurate as of v0.1.0-developer-preview
> **Last verified:** June 2026

## 5.1 Security Model

EnaOS follows a **minimal privilege** model:

| Component | Privilege | Rationale |
|-----------|-----------|-----------|
| **enad** | User-level | D-Bus session bus access, process spawning |
| **ena-bar** | User-level | GTK4 display, Unix socket client |
| **AI Runtime** | User-level | HTTP server, Ollama client |

### No Root Required
- enad does **not** run as root in v0.1.0
- All D-Bus integrations use the **session bus** (not system bus)
- Process spawning uses the user's own permissions
- Systemd service runs as user service (`systemctl --user`)

### IPC Security
- Unix socket at `/tmp/enad.sock` — local-only, no network exposure
- No authentication tokens in v0.1.0 (Unix socket file permissions restrict access)
- All IPC is over localhost — no encryption needed

### No gRPC, No mTLS, No Token-Based Auth
EnaOS is a single-user desktop daemon. The complexity of distributed system security (gRPC auth, mTLS, JWT tokens) is not needed for a local-only Unix socket IPC.

## 5.2 Permission System

### Action Permissions (`ActionType`)
Actions have a `PermissionLevel`:

```rust
pub enum PermissionLevel {
    Safe,                // No confirmation needed
    ConfirmationRequired,// User must approve before execution
}
```

| Action | Permission | Reason |
|--------|-----------|--------|
| `OpenApp` | Safe | Just launches an app |
| `OpenUrl` | Safe | Opens browser |
| `FocusWindow` | Safe | Brings window to front |
| `LaunchCommand` | ConfirmationRequired | Executes shell command |
| `SwitchWorkspace` | Safe | Changes workspace |
| `MediaControl` | Safe | Play/pause/next/prev |
| `ClipboardSet` | Safe | Sets clipboard content |

### No Agent Sandboxing (v0.1.0)
- Autonomous agent execution is not implemented
- `SpawnAgent` command exists as a stub for future use
- No Podman/container sandboxing in this release

## 5.3 Build System

### Rust Components
- **No cargo workspace** — each crate builds independently
- **Release profile:** `lto = true`, `codegen-units = 1`, `opt-level = 2`
- **Conditional dependencies:** `cfg(target_os = "linux")` for `gtk4-layer-shell`, `zbus`, `nix`
- **Features:** `timing = []` for latency instrumentation

```bash
# Build daemon
cd runtimes/enad && cargo build --release

# Build GTK4 bar
cd shell/ena-bar && cargo build --release
```

### Python Components
- **Dependencies:** `pip install -r requirements.txt`
- **Virtual environment:** `python3 -m venv .venv`
- **No Docker/Podman** — runs directly on host

### CI Pipeline
- GitHub Actions workflow (`.github/workflows/ci.yml`)
- Matrix: enad (build + test + clippy), ena-bar (build + clippy)
- Runs on push and pull request to `main`

## 5.4 Package Management

### Current (v0.1.0)
- Manual build from source
- Installer script: `curl -fsSL https://enaos.tech/install.sh | bash`
- Adaptive: detects OS, compositor, installs dependencies

### Not Yet Available
- ❌ Flatpak — not packaged
- ❌ AppImage — not packaged
- ❌ NixOS flake — not available
- ❌ Distro packages (deb, rpm, pacman) — not available

## 5.5 Infrastructure

### Website
- `https://enaos.tech` — project website (placeholder)
- `https://github.com/anshull-saxena/EnaOS` — source repository

### Developer Preview v0.1.0
- **No ISO** — requires existing Linux installation
- **No Docker image** — runs natively on Wayland
- **No cloud service** — all local operation
