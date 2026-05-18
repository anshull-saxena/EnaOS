#!/usr/bin/env bash
#
# EnaOS Developer Preview 0.1 — Adaptive Installer
# AI-Native Operating Environment
# https://enaos.tech
#
# Usage: curl -fsSL https://enaos.tech/install.sh | bash
#   or:  wget -qO- https://enaos.tech/install.sh | bash
#
# Automatically detects the environment and adapts installation.
#

set -e

# ── Colors ──────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
DIM='\033[2m'
BOLD='\033[1m'
NC='\033[0m'

# ── State ───────────────────────────────────────────────────────────
INSTALL_DIR=""
SKIP_PROMPT=false
DRY_RUN=false
FORCE_MODE=""
COMPONENTS=()
MISSING_DEPS=()
WARNINGS=()
ENV_MODE=""

# ── Helpers ─────────────────────────────────────────────────────────
log()     { echo -e "${CYAN}│${NC} $*"; }
ok()      { echo -e "${CYAN}│${NC} ${GREEN}✓${NC} $*"; }
warn()    { echo -e "${CYAN}│${NC} ${YELLOW}⚠${NC} $*"; }
err()     { echo -e "${CYAN}│${NC} ${RED}✗${NC} $*"; }
info()    { echo -e "${CYAN}│${NC} ${DIM}$*${NC}"; }
header()  { echo -e "\n${CYAN}├── ${BOLD}$*${NC}"; }
divider() { echo -e "${CYAN}├──────────────────────────────────────────────────────────────${NC}"; }
banner()  {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${NC}  ${BOLD}EnaOS Developer Preview 0.1${NC}                         ${CYAN}║${NC}"
    echo -e "${CYAN}║${NC}  AI-Native Operating Environment                  ${CYAN}║${NC}"
    echo -e "${CYAN}║${NC}  https://enaos.tech                               ${CYAN}║${NC}"
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  -d, --dir DIR        Install directory (default: ~/enaos)
  -y, --yes            Skip confirmation prompts
  --dry-run            Show what would be done without executing
  --mode MODE          Force installation mode: desktop, headless
  --no-enad            Skip enad daemon
  --no-ena-bar         Skip GTK4 bar
  --no-ai-runtime      Skip AI runtime
  -h, --help           Show this help message

Examples:
  curl -fsSL https://enaos.tech/install.sh | bash
  curl -fsSL https://enaos.tech/install.sh | bash -s -- --mode headless -y
  curl -fsSL https://enaos.tech/install.sh | bash -s -- --dir ~/opt/enaos
EOF
    exit 0
}

# ── Parse Args ──────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        -d|--dir)       INSTALL_DIR="$2"; shift 2 ;;
        -y|--yes)       SKIP_PROMPT=true; shift ;;
        --dry-run)      DRY_RUN=true; shift ;;
        --mode)         FORCE_MODE="$2"; shift 2 ;;
        --no-enad)      COMPONENTS+=("skip-enad"); shift ;;
        --no-ena-bar)   COMPONENTS+=("skip-ena-bar"); shift ;;
        --no-ai-runtime) COMPONENTS+=("skip-ai-runtime"); shift ;;
        -h|--help)      usage ;;
        *)              err "Unknown option: $1"; usage ;;
    esac
done

INSTALL_DIR="${INSTALL_DIR:-$HOME/enaos}"

# ── Banner ──────────────────────────────────────────────────────────
banner

# ════════════════════════════════════════════════════════════════════
#  ENVIRONMENT DETECTION
# ════════════════════════════════════════════════════════════════════
header "Detecting environment"

IS_LINUX=false
IS_WAYLAND=false
IS_X11=false
IS_DESKTOP=false
IS_HEADLESS=false
IS_MACOS=false
IS_WINDOWS=false
HAS_GTK4=false
HAS_GTK4_LAYER_SHELL=false
HAS_DBUS=false
HAS_SYSTEMD=false
HAS_DISPLAY=false

# OS detection
case "$(uname)" in
    Linux)
        IS_LINUX=true
        DISTRO=$(cat /etc/os-release 2>/dev/null | grep PRETTY_NAME | cut -d= -f2 | tr -d '"' || echo "Linux")
        ok "Linux: $DISTRO"
        ;;
    Darwin)
        IS_MACOS=true
        ok "macOS detected ($(sw_vers -productVersion 2>/dev/null || echo 'macOS'))"
        ;;
    *)
        err "Unsupported platform: $(uname)"
        IS_WINDOWS=true
        ;;
esac

if $IS_LINUX; then
    # Display server
    if [[ -n "$WAYLAND_DISPLAY" ]] || [[ "$XDG_SESSION_TYPE" == "wayland" ]]; then
        IS_WAYLAND=true
        HAS_DISPLAY=true
        ok "Wayland session: $WAYLAND_DISPLAY"
    elif [[ -n "$DISPLAY" ]] || [[ "$XDG_SESSION_TYPE" == "x11" ]]; then
        IS_X11=true
        HAS_DISPLAY=true
        warn "X11 session detected (Wayland recommended)"
    fi

    # Desktop environment
    if [[ -n "$XDG_CURRENT_DESKTOP" ]] || [[ -n "$XDG_SESSION_DESKTOP" ]]; then
        IS_DESKTOP=true
        ok "Desktop environment: ${XDG_CURRENT_DESKTOP:-${XDG_SESSION_DESKTOP:-unknown}}"
    fi

    # GTK4
    if pkg-config --exists gtk4 2>/dev/null; then
        HAS_GTK4=true
        ok "GTK4 $(pkg-config --modversion gtk4)"
    fi

    # gtk4-layer-shell
    if pkg-config --exists gtk4-layer-shell-0 2>/dev/null; then
        HAS_GTK4_LAYER_SHELL=true
        ok "gtk4-layer-shell available"
    fi

    # D-Bus
    if command -v dbus-daemon &>/dev/null || [ -S /run/dbus/system_bus_socket ]; then
        HAS_DBUS=true
        ok "D-Bus available"
    fi

    # systemd
    if command -v systemctl &>/dev/null && systemctl is-system-running &>/dev/null 2>&1; then
        HAS_SYSTEMD=true
        ok "systemd available"
    fi
fi

# Determine mode
if [[ -n "$FORCE_MODE" ]]; then
    ENV_MODE="$FORCE_MODE"
    info "Installation mode forced: $ENV_MODE"
elif $IS_LINUX && $IS_WAYLAND && $HAS_GTK4; then
    ENV_MODE="desktop"
    ok "Mode: Full Desktop"
elif $IS_LINUX; then
    ENV_MODE="headless"
    ok "Mode: Headless Developer"
elif $IS_MACOS; then
    ENV_MODE="unsupported"
    warn "Mode: Unsupported (macOS)"
else
    ENV_MODE="unsupported"
    warn "Mode: Unsupported ($(uname))"
fi

echo ""

# ════════════════════════════════════════════════════════════════════
#  COMPONENT SELECTION
# ════════════════════════════════════════════════════════════════════
header "Selecting components"

# Default components based on mode
if [[ ${#COMPONENTS[@]} -eq 0 ]]; then
    case "$ENV_MODE" in
        desktop)
            COMPONENTS=("enad" "ena-bar" "ai-runtime" "desktop-integration")
            ;;
        headless)
            COMPONENTS=("enad" "ai-runtime")
            ;;
        unsupported)
            COMPONENTS=()
            ;;
    esac
fi

# Apply manual overrides
NEW_COMPONENTS=()
for c in "${COMPONENTS[@]}"; do
    case "$c" in
        skip-enad) ;;
        skip-ena-bar) ;;
        skip-ai-runtime) ;;
        *) NEW_COMPONENTS+=("$c") ;;
    esac
done
COMPONENTS=("${NEW_COMPONENTS[@]}")

if [[ ${#COMPONENTS[@]} -eq 0 ]]; then
    case "$ENV_MODE" in
        desktop)
            COMPONENTS=("enad" "ena-bar" "ai-runtime" "desktop-integration")
            ;;
        headless)
            COMPONENTS=("enad" "ai-runtime")
            ;;
        unsupported)
            COMPONENTS=()
            ;;
    esac
fi

log "Components: ${COMPONENTS[*]}"
log "Mode: $ENV_MODE"
echo ""

# ════════════════════════════════════════════════════════════════════
#  UNSUPPORTED ENVIRONMENT
# ════════════════════════════════════════════════════════════════════
if [[ "$ENV_MODE" == "unsupported" ]]; then
    header "Environment not supported for automatic installation"
    echo ""

    if $IS_MACOS; then
        log "EnaOS is a Linux-native operating environment built on Wayland and GTK4."
        log "It integrates deeply with Linux desktop infrastructure (D-Bus, upower,"
        log "NetworkManager, Wayland compositor) and cannot run natively on macOS."
    else
        log "EnaOS requires Linux. Detected: $(uname)"
    fi

    echo ""
    log "${BOLD}For development on this system:${NC}"
    echo ""
    log "  1. Clone the repository:"
    log "     git clone https://github.com/anshull-saxena/EnaOS"
    log ""
    log "  2. Build individual components:"
    log "     cd EnaOS/runtimes/enad && cargo build --release"
    log "     cd EnaOS/runtimes/ai-runtime && pip install -r requirements.txt"
    log ""
    log "  3. For the GTK4 bar, use a Linux VM or WSL2 with Wayland support."
    echo ""
    log "${BOLD}For a Linux desktop:${NC}"
    log "  curl -fsSL https://enaos.tech/install.sh | bash"
    echo ""
    echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
    echo ""
    exit 0
fi

# ════════════════════════════════════════════════════════════════════
#  DEPENDENCY CHECKS
# ════════════════════════════════════════════════════════════════════
header "Checking dependencies"

# Rust
if command -v rustc &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    ok "Rust $RUST_VER"
else
    MISSING_DEPS+=("rust")
    err "Rust not found (required: 1.75+)"
fi

# Cargo
if ! command -v cargo &>/dev/null; then
    MISSING_DEPS+=("cargo")
fi

# Python
if command -v python3 &>/dev/null; then
    PY_VER=$(python3 --version 2>&1 | awk '{print $2}')
    PY_MAJOR=$(echo "$PY_VER" | cut -d. -f1)
    PY_MINOR=$(echo "$PY_VER" | cut -d. -f2)
    if [[ "$PY_MAJOR" -ge 3 ]] && [[ "$PY_MINOR" -ge 11 ]]; then
        ok "Python $PY_VER"
    else
        MISSING_DEPS+=("python3.11+")
        err "Python $PY_VER found (required: 3.11+)"
    fi
else
    MISSING_DEPS+=("python3.11+")
    err "Python3 not found (required: 3.11+)"
fi

# pip
if ! command -v pip3 &>/dev/null && ! python3 -m pip --version &>/dev/null; then
    MISSING_DEPS+=("pip3")
    err "pip3 not found"
fi

# GTK4 (desktop mode only)
if [[ " ${COMPONENTS[*]} " =~ " ena-bar " ]]; then
    if $HAS_GTK4; then
        ok "GTK4 $(pkg-config --modversion gtk4)"
    else
        MISSING_DEPS+=("gtk4-dev")
        err "GTK4 development libraries not found"
    fi

    if $HAS_GTK4_LAYER_SHELL; then
        ok "gtk4-layer-shell available"
    else
        MISSING_DEPS+=("gtk4-layer-shell-dev")
        err "gtk4-layer-shell not found (required for Wayland layer-shell)"
    fi
else
    info "GTK4 not required (headless mode)"
fi

# D-Bus (desktop mode only)
if [[ " ${COMPONENTS[*]} " =~ " desktop-integration " ]]; then
    if $HAS_DBUS; then
        ok "D-Bus available"
    else
        warn "D-Bus not detected (needed for battery, network, audio widgets)"
        WARNINGS+=("D-Bus is needed for full desktop integration.")
    fi
fi

# Git
if command -v git &>/dev/null; then
    ok "Git found"
else
    MISSING_DEPS+=("git")
    err "Git not found"
fi

# Ollama (optional)
if command -v ollama &>/dev/null && curl -s http://localhost:11434/api/tags &>/dev/null; then
    ok "Ollama running (AI runtime will use local LLM)"
elif command -v ollama &>/dev/null; then
    warn "Ollama installed but not running (start with: ollama serve)"
else
    info "Ollama not found (AI runtime will use OpenAI API if configured)"
fi

echo ""

# ════════════════════════════════════════════════════════════════════
#  INSTALL MISSING DEPENDENCIES
# ════════════════════════════════════════════════════════════════════
if [[ ${#MISSING_DEPS[@]} -gt 0 ]]; then
    header "Installing missing dependencies"

    if command -v apt-get &>/dev/null; then
        PKG_MGR="apt"
        PKG_INSTALL="sudo apt-get install -y"
        PKG_UPDATE="sudo apt-get update"
    elif command -v dnf &>/dev/null; then
        PKG_MGR="dnf"
        PKG_INSTALL="sudo dnf install -y"
        PKG_UPDATE="sudo dnf check-update || true"
    elif command -v pacman &>/dev/null; then
        PKG_MGR="pacman"
        PKG_INSTALL="sudo pacman -S --noconfirm"
        PKG_UPDATE="sudo pacman -Sy"
    elif command -v zypper &>/dev/null; then
        PKG_MGR="zypper"
        PKG_INSTALL="sudo zypper install -y"
        PKG_UPDATE="sudo zypper refresh"
    else
        err "No supported package manager found (apt, dnf, pacman, zypper)"
        log "Install missing dependencies manually, then re-run this script."
        exit 1
    fi

    ok "Package manager: $PKG_MGR"

    APT_PKGS=()
    DNF_PKGS=()
    PACMAN_PKGS=()

    for dep in "${MISSING_DEPS[@]}"; do
        case "$dep" in
            rust|cargo)
                APT_PKGS+=("rustc" "cargo")
                DNF_PKGS+=("rust" "cargo")
                PACMAN_PKGS+=("rust" "cargo")
                ;;
            python3.11+)
                APT_PKGS+=("python3" "python3-pip" "python3-venv")
                DNF_PKGS+=("python3" "python3-pip")
                PACMAN_PKGS+=("python" "python-pip")
                ;;
            pip3)
                APT_PKGS+=("python3-pip")
                DNF_PKGS+=("python3-pip")
                PACMAN_PKGS+=("python-pip")
                ;;
            gtk4-dev)
                APT_PKGS+=("libgtk-4-dev" "libadwaita-1-dev" "pkg-config")
                DNF_PKGS+=("gtk4-devel" "libadwaita-devel" "pkg-config")
                PACMAN_PKGS+=("gtk4" "libadwaita" "pkg-config")
                ;;
            gtk4-layer-shell-dev)
                APT_PKGS+=("libgtk4-layer-shell-dev")
                DNF_PKGS+=("gtk4-layer-shell-devel")
                PACMAN_PKGS+=("gtk4-layer-shell")
                ;;
            git)
                APT_PKGS+=("git")
                DNF_PKGS+=("git")
                PACMAN_PKGS+=("git")
                ;;
        esac
    done

    APT_PKGS=($(echo "${APT_PKGS[@]}" | tr ' ' '\n' | sort -u | tr '\n' ' '))
    DNF_PKGS=($(echo "${DNF_PKGS[@]}" | tr ' ' '\n' | sort -u | tr '\n' ' '))
    PACMAN_PKGS=($(echo "${PACMAN_PKGS[@]}" | tr ' ' '\n' | sort -u | tr '\n' ' '))

    log "Will install: ${APT_PKGS[*]:-${DNF_PKGS[*]:-${PACMAN_PKGS[*]}}}"
    echo ""

    if [ "$SKIP_PROMPT" = false ]; then
        read -rp "Install these packages? [Y/n] " PKG_CONFIRM
        if [[ "$PKG_CONFIRM" =~ ^[Nn] ]]; then
            err "Cannot proceed without dependencies. Aborted."
            exit 1
        fi
    fi

    log "Updating package lists..."
    $PKG_UPDATE &>/dev/null || true

    case "$PKG_MGR" in
        apt)    log "Installing packages..." && $PKG_INSTALL "${APT_PKGS[@]}" ;;
        dnf)    log "Installing packages..." && $PKG_INSTALL "${DNF_PKGS[@]}" ;;
        pacman) log "Installing packages..." && $PKG_INSTALL "${PACMAN_PKGS[@]}" ;;
    esac

    ok "Dependencies installed"
    echo ""
fi

# ── Confirmation ────────────────────────────────────────────────────
if [ "$SKIP_PROMPT" = false ] && [ "$DRY_RUN" = false ]; then
    log "${BOLD}Installation plan:${NC}"
    log "  Mode: $ENV_MODE"
    log "  Target: $INSTALL_DIR"
    log "  Components: ${COMPONENTS[*]}"
    echo ""
    read -rp "Continue? [Y/n] " CONFIRM
    if [[ "$CONFIRM" =~ ^[Nn] ]]; then
        log "Aborted."
        exit 0
    fi
fi

# ════════════════════════════════════════════════════════════════════
#  CLONE / UPDATE REPOSITORY
# ════════════════════════════════════════════════════════════════════
header "Fetching EnaOS source"

if [ -d "$INSTALL_DIR/.git" ]; then
    log "Repository exists at $INSTALL_DIR — updating..."
    cd "$INSTALL_DIR"
    git pull origin main
    ok "Repository updated"
elif [ "$DRY_RUN" = true ]; then
    log "[DRY RUN] Would clone to $INSTALL_DIR"
else
    mkdir -p "$INSTALL_DIR"
    log "Cloning repository..."
    git clone https://github.com/anshull-saxena/EnaOS "$INSTALL_DIR" 2>&1 | tail -1
    ok "Repository cloned"
fi

echo ""
if [ "$DRY_RUN" = false ] || [ -d "$INSTALL_DIR/.git" ]; then
    cd "$INSTALL_DIR"
fi

# ════════════════════════════════════════════════════════════════════
#  BUILD COMPONENTS
# ════════════════════════════════════════════════════════════════════

# ── enad ────────────────────────────────────────────────────────────
if [[ " ${COMPONENTS[*]} " =~ " enad " ]]; then
    header "Building enad (System Daemon)"

    if [ "$DRY_RUN" = true ]; then
        log "[DRY RUN] Would build enad: cd runtimes/enad && cargo build --release"
    else
        cd runtimes/enad
        log "Compiling enad (this may take a few minutes)..."
        cargo build --release 2>&1 | tail -5
        ok "enad built: $(realpath target/release/enad)"
        cd "$INSTALL_DIR"
    fi
    echo ""
fi

# ── ena-bar ─────────────────────────────────────────────────────────
if [[ " ${COMPONENTS[*]} " =~ " ena-bar " ]]; then
    header "Building ena-bar (GTK4 Frontend)"

    if [ "$DRY_RUN" = true ]; then
        log "[DRY RUN] Would build ena-bar: cd shell/ena-bar && cargo build --release"
    else
        cd shell/ena-bar
        log "Compiling ena-bar (this may take a few minutes)..."
        cargo build --release 2>&1 | tail -5
        ok "ena-bar built: $(realpath target/release/ena-bar)"
        cd "$INSTALL_DIR"
    fi
    echo ""
fi

# ── AI Runtime ─────────────────────────────────────────────────────
if [[ " ${COMPONENTS[*]} " =~ " ai-runtime " ]]; then
    header "Setting up AI Runtime (Python)"

    if [ "$DRY_RUN" = true ]; then
        log "[DRY RUN] Would install Python deps: cd runtimes/ai-runtime && pip3 install -r requirements.txt"
    else
        cd runtimes/ai-runtime

        if [ ! -d ".venv" ]; then
            log "Creating Python virtual environment..."
            python3 -m venv .venv
            ok "Virtual environment created"
        fi

        source .venv/bin/activate
        log "Installing Python dependencies..."
        pip install -q -r requirements.txt 2>&1 | tail -3
        ok "AI Runtime dependencies installed"
        cd "$INSTALL_DIR"
    fi
    echo ""
fi

# ════════════════════════════════════════════════════════════════════
#  DESKTOP INTEGRATION (systemd, desktop entry)
# ════════════════════════════════════════════════════════════════════
if [[ " ${COMPONENTS[*]} " =~ " desktop-integration " ]] && $HAS_SYSTEMD && ! $DRY_RUN; then
    header "Creating desktop integration"

    # systemd service
    SERVICE_FILE="$HOME/.config/systemd/user/enaos.service"
    mkdir -p "$HOME/.config/systemd/user"

    cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=EnaOS System Daemon
After=network.target dbus.service

[Service]
Type=simple
ExecStart=$INSTALL_DIR/runtimes/enad/target/release/enad --socket /tmp/enad.sock
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    ok "systemd user service created: enaos.service"
    info "Enable with: systemctl --user enable --now enaos.service"

    # Desktop entry
    DESKTOP_FILE="$HOME/.local/share/applications/enaos.desktop"
    mkdir -p "$HOME/.local/share/applications"

    cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Name=EnaOS
Comment=AI-Native Operating Environment
Exec=$INSTALL_DIR/start.sh
Icon=utilities-terminal
Terminal=true
Type=Application
Categories=System;Utility;
EOF

    ok "Desktop entry created: enaos.desktop"
    echo ""
fi

# ════════════════════════════════════════════════════════════════════
#  LAUNCH SCRIPT
# ════════════════════════════════════════════════════════════════════
header "Creating launch script"

if [ "$DRY_RUN" = true ]; then
    log "[DRY RUN] Would create $INSTALL_DIR/start.sh"
else
    # Build launch script based on installed components
    HAS_ENA_BAR=false
    [[ " ${COMPONENTS[*]} " =~ " ena-bar " ]] && HAS_ENA_BAR=true

    cat > "$INSTALL_DIR/start.sh" <<LAUNCH
#!/usr/bin/env bash
# EnaOS Developer Preview — Launch Script
# Mode: $ENV_MODE
# Generated by installer

set -e

INSTALL_DIR="\$(cd "\$(dirname "\$0")" && pwd)"
SOCKET_PATH="/tmp/enad.sock"

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║  EnaOS Developer Preview 0.1                 ║"
echo "║  https://enaos.tech                          ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

# Cleanup previous instance
if [ -S "\$SOCKET_PATH" ]; then
    echo "Cleaning up previous enad instance..."
    rm -f "\$SOCKET_PATH"
fi

# 1. Start enad
echo "[1/2] Starting enad daemon..."
"\$INSTALL_DIR/runtimes/enad/target/release/enad" --socket "\$SOCKET_PATH" &
ENAD_PID=\$!
sleep 1

if ! kill -0 \$ENAD_PID 2>/dev/null; then
    echo "ERROR: enad failed to start"
    exit 1
fi
echo "  → enad running (PID: \$ENAD_PID, socket: \$SOCKET_PATH)"

# 2. Start AI Runtime
echo "[2/2] Starting AI runtime..."
cd "\$INSTALL_DIR/runtimes/ai-runtime"
if [ -d ".venv" ]; then
    source .venv/bin/activate
fi
python3 -m src.main &
AI_PID=\$!
sleep 2
echo "  → AI runtime running (PID: \$AI_PID, http://localhost:8900)"
LAUNCH

    if $HAS_ENA_BAR; then
        cat >> "$INSTALL_DIR/start.sh" <<'LAUNCH'

# 3. Start ena-bar
echo "[3/3] Starting ena-bar..."
"$INSTALL_DIR/shell/ena-bar/target/release/ena-bar" --socket-path "$SOCKET_PATH" &
BAR_PID=$!
sleep 1
echo "  → ena-bar running (PID: $BAR_PID)"
LAUNCH
    fi

    cat >> "$INSTALL_DIR/start.sh" <<'LAUNCH'

echo ""
echo "All components started. Press Ctrl+C to stop."
echo ""

cleanup() {
    echo ""
    echo "Shutting down..."
    kill $BAR_PID 2>/dev/null || true
    kill $AI_PID 2>/dev/null || true
    kill $ENAD_PID 2>/dev/null || true
    rm -f "$SOCKET_PATH"
    echo "Done."
    exit 0
}

trap cleanup SIGINT SIGTERM
wait
LAUNCH

    chmod +x "$INSTALL_DIR/start.sh"
    ok "Launch script created: $INSTALL_DIR/start.sh"
fi

echo ""

# ════════════════════════════════════════════════════════════════════
#  WARNINGS
# ════════════════════════════════════════════════════════════════════
if [[ ${#WARNINGS[@]} -gt 0 ]]; then
    header "Warnings"
    for w in "${WARNINGS[@]}"; do
        warn "$w"
    done
    echo ""
fi

# ════════════════════════════════════════════════════════════════════
#  SUMMARY
# ════════════════════════════════════════════════════════════════════
banner

log "${BOLD}Installation complete!${NC}"
echo ""
log "${BOLD}Mode:${NC} $ENV_MODE"
log "${BOLD}Install directory:${NC} $INSTALL_DIR"
log "${BOLD}Components:${NC} ${COMPONENTS[*]}"
echo ""

if [[ "$ENV_MODE" == "desktop" ]]; then
    log "${BOLD}Quick start:${NC}"
    log "  cd $INSTALL_DIR"
    log "  ./start.sh"
    echo ""
    log "${BOLD}Or start individually:${NC}"
    log "  Terminal 1 — Daemon"
    log "    $INSTALL_DIR/runtimes/enad/target/release/enad --socket /tmp/enad.sock"
    log ""
    log "  Terminal 2 — AI Runtime"
    log "    cd $INSTALL_DIR/runtimes/ai-runtime && python3 -m src.main"
    log ""
    log "  Terminal 3 — GTK4 Bar"
    log "    $INSTALL_DIR/shell/ena-bar/target/release/ena-bar --socket-path /tmp/enad.sock"
elif [[ "$ENV_MODE" == "headless" ]]; then
    log "${BOLD}Quick start (headless):${NC}"
    log "  cd $INSTALL_DIR"
    log "  ./start.sh"
    echo ""
    log "${BOLD}Or start individually:${NC}"
    log "  Terminal 1 — Daemon"
    log "    $INSTALL_DIR/runtimes/enad/target/release/enad --socket /tmp/enad.sock"
    log ""
    log "  Terminal 2 — AI Runtime"
    log "    cd $INSTALL_DIR/runtimes/ai-runtime && python3 -m src.main"
    echo ""
    log "${BOLD}Note:${NC} ena-bar (GTK4 UI) was not installed."
    log "  To add it, run on a Linux desktop with Wayland:"
    log "  cd $INSTALL_DIR/shell/ena-bar && cargo build --release"
fi

echo ""
log "${BOLD}Verify:${NC}"
log "  curl http://localhost:8900/health"
echo ""
log "Documentation: https://enaos.tech"
log "GitHub:        https://github.com/anshull-saxena/EnaOS"
echo ""
echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""
