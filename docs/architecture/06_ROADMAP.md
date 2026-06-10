# 6. Milestone Roadmap

> **Status:** Accurate as of v0.1.0-developer-preview
> **Last verified:** June 2026

## Milestone 0: Foundation ✅ (Completed)

**Goal:** System daemon, GTK4 bar, and IPC infrastructure.

### Delivered
- ✅ Rust daemon (enad) with tokio event bus + Unix socket IPC
- ✅ GTK4 bar (ena-bar) with Wayland layer-shell overlay
- ✅ 22 IPC command variants, 3 response variants, 28 event payload variants
- ✅ 7 desktop integration subsystems (UPower, NetworkManager, window focus, workspace, clipboard, notifications, audio/MPRIS)
- ✅ Process lifecycle manager with zombie reaping
- ✅ DAG-based orchestration engine with retry + rollback
- ✅ Workspace snapshot store (SQLite)
- ✅ Restoration planner with action preview
- ✅ Context-aware command suggestion engine (intent classification, 5-source resolution)
- ✅ Ambient suggestion engine (event-driven, rate-limited)
- ✅ Working memory store (SQLite with FTS5)
- ✅ First-run onboarding with welcome overlay
- ✅ Python AI runtime with FastAPI + Ollama integration
- ✅ **71 tests** (IPC round-trip, wire format, integration)
- ✅ Stability Sprint 1 — 4 critical IPC bugs fixed
- ✅ Stability Sprint 2 — test infrastructure, failure testing, UI resilience audit
- ✅ MIT licensed on GitHub

---

## Milestone 1: Validation & Integration (Current — Q3 2026)

**Goal:** Production-quality subsystems, comprehensive testing, and packaging.

### In Progress
- Auto-snapshot loop (periodic + event-triggered)
- Auto-expiry for old memory entries and snapshots
- Snapshot restoration end-to-end verification
- Multi-compositor window tracking verification (Sway, Hyprland, GNOME)

### Planned
- Complete all 7 desktop subsystem implementations with graceful degradation
- End-to-end integration tests for all user flows
- CI pipeline with automated quality gates
- Flatpak packaging
- AI Runtime auto-start by enad process manager
- Global keyboard shortcut (D-Bus)
- PipeWire audio capture for voice input
- Manual snapshots with one-click restore

---

## Milestone 2: Intelligence (Future — Q4 2026)

**Goal:** Context-aware AI, multi-agent orchestration, and developer API.

### Planned
- Context-aware prompt injection (desktop state → LLM context)
- Natural language → command resolution via AI Runtime
- Multi-agent orchestration with baton-passing
- Memory engine with semantic search (local embeddings)
- Plugin SDK (WASM-based)
- Developer Preview API documentation
- Cross-distro packaging (deb, rpm, pacman)

---

## Milestone 3: Ecosystem (Future — 2027)

**Goal:** Stable release, plugin ecosystem, and community adoption.

### Planned
- Plugin marketplace
- Native Settings and Files GTK4 apps
- Stable API with versioning guarantees
- Performance profiling and optimization
- Accessibility compliance (ATK/AT-SPI)
- Internationalization
- Comprehensive user documentation

---

## Known Gaps (v0.1.0)

| Area | Gap | Priority |
|------|-----|----------|
| **Packaging** | No Flatpak/AppImage — manual build required | High |
| **AI Runtime** | Requires manual `ollama serve` — not auto-started | Medium |
| **Window Tracking** | Not verified on all compositors | Medium |
| **Auto-Snapshot** | Not implemented — only manual snapshots | Medium |
| **Snapshot Pruning** | No auto-expiry for old snapshots | Low |
| **macOS** | Development-only mode, no desktop integration | Low |
| **Agent Execution** | `SpawnAgent` stub only — no autonomous agents | Future |
| **Plugin System** | Not started | Future |
