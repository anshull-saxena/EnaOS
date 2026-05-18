#!/usr/bin/env bash
#
# EnaOS Developer Preview 0.1 — Installer
# AI-Native Desktop Runtime
# https://enaos.tech
#
# Usage: curl -fsSL https://enaos.tech/install.sh | bash
#   or:  wget -qO- https://enaos.tech/install.sh | bash
#
# This script builds and installs the EnaOS developer preview from source.
# Requires: Linux with Wayland, Rust 1.75+, Python 3.11+, GTK4 dev libs.
#

set -e

# ── Colors ──────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ── State ───────────────────────────────────────────────────────────
INSTALL_DIR=""
SKIP_PROMPT=false
DRY_RUN=false
COMPONENTS=("enad" "ena-bar" "ai-runtime")
MISSING_DEPS=()
WARNINGS=()

# ── Helpers ─────────────────────────────────────────────────────────
log()     { echo -e "${CYAN}│${NC} $*"; }
ok()      { echo -e "${CYAN}│${NC} ${GREEN}✓${NC} $*"; }
warn()    { echo -e "${CYAN}│${NC} ${YELLOW}⚠${NC} $*"; }
err()     { echo -e "${CYAN}│${NC} ${RED}✗${NC} $*"; }
header()  { echo -e "\n${CYAN}├── ${BOLD}$*${NC}"; }
banner()  {
    echo ""
    echo -e "${CYAN}╔══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${CYAN}║${NC}  ${BOLD}EnaOS Developer Preview 0.1${NC}                         ${CYAN}║${NC}"
    echo -e "${CYAN}║${NC}  AI-Native Desktop Runtime                        ${CYAN}║${NC}"
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
  --no-enad            Skip enad daemon
  --no-ena-bar         Skip GTK4 bar
  --no-ai-runtime      Skip AI runtime
  -h, --help           Show this help message

Examples:
  curl -fsSL https://enaos.tech/install.sh | bash
  curl -fsSL https://enaos.tech/install.sh | bash -s -- --dir ~/opt/enaos -y
EOF
    exit 0
}

# ── Parse Args ──────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        -d|--dir)       INSTALL_DIR="$2"; shift 2 ;;
        -y|--yes)       SKIP_PROMPT=true; shift ;;
        --dry-run)      DRY_RUN=true; shift ;;
        --no-enad)      COMPONENTS=("${COMPONENTS[@]/enad}"); shift ;;
        --no-ena-bar)   COMPONENTS=("${COMPONENTS[@]/ena-bar}"); shift ;;
        --no-ai-runtime) COMPONENTS=("${COMPONENTS[@]/ai-runtime}"); shift ;;
        -h|--help)      usage ;;
        *)              err "Unknown option: $1"; usage ;;
    esac
done

INSTALL_DIR="${INSTALL_DIR:-$HOME/enaos}"

# ── Banner ──────────────────────────────────────────────────────────
banner

log "${BOLD}Installing to:${NC} $INSTALL_DIR"
log "${BOLD}Components:${NC} ${COMPONENTS[*]}"
echo ""

# ── Confirmation ────────────────────────────────────────────────────
if [ "$SKIP_PROMPT" = false ] && [ "$DRY_RUN" = false ]; then
    read -rp "Continue? [Y/n] " CONFIRM
    if [[ "$CONFIRM" =~ ^[Nn] ]]; then
        log "Aborted."
        exit 0
    fi
fi

# ── Checks ──────────────────────────────────────────────────────────
header "Checking system requirements"

# OS check
if [[ "$(uname)" != "Linux" ]]; then
    err "EnaOS requires Linux. Detected: $(uname)"
    log "For development on macOS, clone the repo and build components individually."
    log "  git clone https://github.com/anshull-saxena/EnaOS"
    exit 1
fi
ok "Linux detected ($(cat /etc/os-release 2>/dev/null | grep PRETTY_NAME | cut -d= -f2 | tr -d '"' || echo 'Linux'))"

# Wayland check
if [[ -z "$WAYLAND_DISPLAY" ]] && [[ "$XDG_SESSION_TYPE" != "wayland" ]]; then
    warn "Wayland session not detected"
    WARNINGS+=("Wayland is required for ena-bar. The bar will not work on X11.")
else
    ok "Wayland session detected"
fi

# Rust check
if command -v rustc &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    ok "Rust $RUST_VER"
else
    MISSING_DEPS+=("rust")
    err "Rust not found (required: 1.75+)"
fi

# Cargo check
if ! command -v cargo &>/dev/null; then
    MISSING_DEPS+=("cargo")
fi

# Python check
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

# pip check
if ! command -v pip3 &>/dev/null && ! python3 -m pip --version &>/dev/null; then
    MISSING_DEPS+=("pip3")
    err "pip3 not found"
fi

# GTK4 check (only if building ena-bar)
if [[ " ${COMPONENTS[*]} " =~ " ena-bar " ]]; then
    if pkg-config --exists gtk4 2>/dev/null; then
        GTK_VER=$(pkg-config --modversion gtk4)
        ok "GTK4 $GTK_VER"
    else
        MISSING_DEPS+=("gtk4-dev")
        err "GTK4 development libraries not found"
    fi

    if pkg-config --exists gtk4-layer-shell-0 2>/dev/null; then
        ok "gtk4-layer-shell found"
    else
        MISSING_DEPS+=("gtk4-layer-shell-dev")
        err "gtk4-layer-shell not found (required for Wayland layer-shell)"
    fi
fi

# D-Bus check
if command -v dbus-daemon &>/dev/null || [ -S /run/dbus/system_bus_socket ]; then
    ok "D-Bus available"
else
    warn "D-Bus not detected (required for system integration)"
    WARNINGS+=("D-Bus is needed for battery, network, and audio widgets.")
fi

# Git check
if command -v git &>/dev/null; then
    ok "Git found"
else
    MISSING_DEPS+=("git")
    err "Git not found"
fi

# Ollama check (optional)
if command -v ollama &>/dev/null && curl -s http://localhost:11434/api/tags &>/dev/null; then
    ok "Ollama running (AI runtime will use local LLM)"
elif command -v ollama &>/dev/null; then
    warn "Ollama installed but not running (start with: ollama serve)"
else
    warn "Ollama not found (AI runtime will fall back to OpenAI API)"
fi

echo ""

# ── Install Missing Dependencies ───────────────────────────────────
if [[ ${#MISSING_DEPS[@]} -gt 0 ]]; then
    header "Installing missing dependencies"

    # Detect package manager
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

    # Build install command
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

    # Deduplicate
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
        apt)   log "Installing packages..." && $PKG_INSTALL "${APT_PKGS[@]}" ;;
        dnf)   log "Installing packages..." && $PKG_INSTALL "${DNF_PKGS[@]}" ;;
        pacman) log "Installing packages..." && $PKG_INSTALL "${PACMAN_PKGS[@]}" ;;
    esac

    ok "Dependencies installed"
    echo ""
fi

# ── Clone / Update Repository ──────────────────────────────────────
header "Fetching EnaOS source"

if [ -d "$INSTALL_DIR/.git" ]; then
    log "Repository exists at $INSTALL_DIR — updating..."
    cd "$INSTALL_DIR"
    git pull origin main
    ok "Repository updated"
else
    if [ "$DRY_RUN" = true ]; then
        log "[DRY RUN] Would clone to $INSTALL_DIR"
    else
        mkdir -p "$INSTALL_DIR"
        log "Cloning repository..."
        git clone https://github.com/anshull-saxena/EnaOS "$INSTALL_DIR" 2>&1 | tail -1
        ok "Repository cloned"
    fi
fi

echo ""

# ── Build Components ───────────────────────────────────────────────
cd "$INSTALL_DIR"

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

        # Create venv if it doesn't exist
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

# ── Create Launch Script ───────────────────────────────────────────
header "Creating launch script"

if [ "$DRY_RUN" = true ]; then
    log "[DRY RUN] Would create $INSTALL_DIR/start.sh"
else
    cat > "$INSTALL_DIR/start.sh" <<'LAUNCH'
#!/usr/bin/env bash
# EnaOS Developer Preview — Launch Script
# Starts all components in the correct order.

set -e

INSTALL_DIR="$(cd "$(dirname "$0")" && pwd)"
SOCKET_PATH="/tmp/enad.sock"

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║  EnaOS Developer Preview 0.1                 ║"
echo "║  https://enaos.tech                          ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

# Cleanup previous instance
if [ -S "$SOCKET_PATH" ]; then
    echo "Cleaning up previous enad instance..."
    rm -f "$SOCKET_PATH"
fi

# 1. Start enad
echo "[1/3] Starting enad daemon..."
"$INSTALL_DIR/runtimes/enad/target/release/enad" --socket "$SOCKET_PATH" &
ENAD_PID=$!
sleep 1

if ! kill -0 $ENAD_PID 2>/dev/null; then
    echo "ERROR: enad failed to start"
    exit 1
fi
echo "  → enad running (PID: $ENAD_PID, socket: $SOCKET_PATH)"

# 2. Start AI Runtime
echo "[2/3] Starting AI runtime..."
cd "$INSTALL_DIR/runtimes/ai-runtime"
if [ -d ".venv" ]; then
    source .venv/bin/activate
fi
python3 -m src.main &
AI_PID=$!
sleep 2
echo "  → AI runtime running (PID: $AI_PID, http://localhost:8900)"

# 3. Start ena-bar
echo "[3/3] Starting ena-bar..."
"$INSTALL_DIR/shell/ena-bar/target/release/ena-bar" --socket-path "$SOCKET_PATH" &
BAR_PID=$!
sleep 1
echo "  → ena-bar running (PID: $BAR_PID)"

echo ""
echo "All components started. Press Ctrl+C to stop."
echo ""

# Trap SIGINT and cleanup
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

# Wait for any process to exit
wait
LAUNCH

    chmod +x "$INSTALL_DIR/start.sh"
    ok "Launch script created: $INSTALL_DIR/start.sh"
fi

echo ""

# ── Warnings ───────────────────────────────────────────────────────
if [[ ${#WARNINGS[@]} -gt 0 ]]; then
    header "Warnings"
    for w in "${WARNINGS[@]}"; do
        warn "$w"
    done
    echo ""
fi

# ── Summary ────────────────────────────────────────────────────────
banner

log "${BOLD}Installation complete!${NC}"
echo ""
log "Install directory: ${BOLD}$INSTALL_DIR${NC}"
log ""
log "${BOLD}Quick start:${NC}"
log "  cd $INSTALL_DIR"
log "  ./start.sh"
log ""
log "${BOLD}Or start components individually:${NC}"
log "  # Terminal 1 — Daemon"
log "  $INSTALL_DIR/runtimes/enad/target/release/enad --socket /tmp/enad.sock"
log ""
log "  # Terminal 2 — AI Runtime"
log "  cd $INSTALL_DIR/runtimes/ai-runtime && python3 -m src.main"
log ""
log "  # Terminal 3 — GTK4 Bar"
log "  $INSTALL_DIR/shell/ena-bar/target/release/ena-bar --socket-path /tmp/enad.sock"
log ""
log "${BOLD}Verify installation:${NC}"
log "  curl http://localhost:8900/health"
log ""
log "Documentation: https://enaos.tech"
log "GitHub:        https://github.com/anshull-saxena/EnaOS"
log ""
echo -e "${CYAN}╚══════════════════════════════════════════════════════════╝${NC}"
echo ""
