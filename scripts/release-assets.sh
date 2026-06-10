#!/usr/bin/env bash
# EnaOS v0.1.0 Developer Preview — Release Asset Capture Script
#
# This script documents how to capture screenshots and demo GIFs
# for the GitHub release. Run from the project root.
#
# Prerequisites:
#   - Wayland compositor running (Sway or Hyprland recommended)
#   - enad daemon built and running
#   - ena-bar built and running
#   - slurp + grim (screenshot) or wf-recorder (GIF) installed
#
# Usage:
#   bash scripts/release-assets.sh [--all | screenshot | gif]

set -e

OUTPUT_DIR="docs/assets/release"
mkdir -p "$OUTPUT_DIR"

echo "╔════════════════════════════════════════════╗"
echo "║  EnaOS v0.1.0 Release Asset Capture       ║"
echo "╚════════════════════════════════════════════╝"
echo ""
echo "Output directory: $OUTPUT_DIR"
echo ""

capture_screenshot() {
    local name="$1"
    local path="$OUTPUT_DIR/${name}.png"
    echo "  Capturing: $name"
    echo "  Select area with mouse (click and drag)..."
    slurp | grim -g - "$path"
    echo "  → Saved: $path"
    echo ""
}

capture_gif() {
    local name="$1"
    local duration="$2"
    local path="$OUTPUT_DIR/${name}.gif"
    echo "  Recording GIF: $name (${duration}s)"
    echo "  Select area with mouse (click and drag)..."
    local geometry
    geometry=$(slurp)
    wf-recorder -g "$geometry" -t "$duration" -c gif -f "$path"
    echo "  → Saved: $path"
    echo ""
}

echo "══════════════════════════════════════════════"
echo "  REQUIRED RELEASE SCREENSHOTS"
echo "══════════════════════════════════════════════"
echo ""
echo "1. ena-bar-collapsed.png"
echo "   Bar in collapsed state (minimal — just status dot)"
echo "   → Make sure no input is focused, bar shows only status dot"
echo ""
echo "2. ena-bar-expanded.png"
echo "   Bar in expanded state with input entry visible"
echo "   → Click on the bar to focus the input entry"
echo ""
echo "3. ena-bar-context.png"
echo "   Bar showing system context (focused app, workspace, battery, WiFi)"
echo "   → Type a character and see context label at bottom"
echo ""
echo "4. ena-bar-command-palette.png"
echo "   Command palette with suggestions visible"
echo "   → Type 'open ' to trigger context-aware suggestions"
echo ""
echo "5. ena-bar-restoration.png"
echo "   Restoration suggestion widget"
echo "   → Ensure a snapshot exists, then show the 'Continue' suggestion"
echo ""
echo "6. ena-bar-welcome.png"
echo "   Welcome overlay on first launch"
echo "   → Run ena-bar on a fresh enad instance (delete ~/.local/share/enad/)"
echo ""
echo "7. ena-bar-battery-network.png"
echo "   Close-up of battery + network indicators in context label"
echo "   → Has WiFi connected and battery discharging"
echo ""
echo "══════════════════════════════════════════════"
echo "  REQUIRED DEMO GIFS"
echo "══════════════════════════════════════════════"
echo ""
echo "1. demo-typing-command.gif (10s)"
echo "   User types 'open browser' → suggestions appear → selects one"
echo "   Shows: 40ms debounce, suggestion rendering, command execution"
echo ""
echo "2. demo-snapshot-restore.gif (15s)"
echo "   User creates a snapshot → later restores it with preview"
echo "   Shows: snapshot creation, restoration suggestion, preview with toggles"
echo ""
echo "3. demo-onboarding.gif (8s)"
echo "   First launch experience with welcome overlay"
echo "   Shows: crossfade animation, suggestion chips, dismissal"
echo ""
echo "4. demo-ambient-suggestions.gif (8s)"
echo "   Bar shows ambient suggestions after window focus change"
echo "   Shows: non-intrusive suggestion, one-click action"
echo ""
echo "══════════════════════════════════════════════"
echo "  CAPTURE COMMANDS"
echo "══════════════════════════════════════════════"
echo ""
echo "  Interactive capture:"
echo "    bash scripts/release-assets.sh screenshot  # for stills"
echo "    bash scripts/release-assets.sh gif 5       # for 5s GIF"
echo ""
echo "  Or use tools directly:"
echo "    slurp | grim -g - docs/assets/release/screenshot.png"
echo "    slurp | wf-recorder -g \$(slurp) -t 10 -c gif -f demo.gif"
echo ""

# ── Interactive mode ──────────────────────────────────────────────
case "${1:-}" in
    screenshot)
        capture_screenshot "${2:-screenshot}"
        ;;
    gif)
        capture_gif "${2:-demo}" "${3:-10}"
        ;;
    --all)
        echo "Interactive mode: press Enter after setting up each shot."
        echo ""
        for name in \
            "ena-bar-collapsed" \
            "ena-bar-expanded" \
            "ena-bar-context" \
            "ena-bar-command-palette" \
            "ena-bar-restoration" \
            "ena-bar-welcome" \
            "ena-bar-battery-network"; do
            read -rp "  Press Enter to capture: $name"
            capture_screenshot "$name"
        done
        echo "All screenshots captured in $OUTPUT_DIR/"
        ;;
    *)
        echo "Usage: bash scripts/release-assets.sh [--all | screenshot [name] | gif [name] [duration]]"
        echo ""
        echo "  --all        Interactive walkthrough capturing all 7 required screenshots"
        echo "  screenshot   Capture a single screenshot (default name: screenshot)"
        echo "  gif          Record a GIF (default: 10s)"
        ;;
esac
