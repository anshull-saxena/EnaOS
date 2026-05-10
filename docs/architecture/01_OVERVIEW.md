# 1. System Overview & Monorepo Architecture

## 1.1 Monorepo Architecture
EnaOS uses a monolithic repository structure to ensure atomic changes across the system daemon, IPC contracts, desktop shell, and AI runtimes.

```text
/enaos
├── agent-engine/     # Agent lifecycle, sandboxing, and capability registry
├── ai-runtime/       # LLM provider abstraction, local inference (Ollama), orchestration
├── apps/             # Core OS GUI applications (Settings, Files)
├── core/             # Low-level primitives: IPC, Event Bus, Config, Telemetry
├── docs/             # Architecture, API specs, and runbooks
├── ena-bar/          # The primary AI interaction surface (frontend + backend)
├── infrastructure/   # NixOS flakes, Dockerfiles, IaC
├── memory-engine/    # PostgreSQL (Relational) + pgvector (Vector) + Graph DB integration
├── plugins/          # 3rd-party extensibility examples and core plugins
├── scripts/          # DX and CI scripts (build, dev, deploy)
├── sdk/              # Client libraries for IPC/Events (Rust, Python, TS)
├── shell/            # Wayland Compositor and Desktop UI (GTK4/libadwaita or Tauri)
├── system-services/  # Core system daemons (Automation, Auth, Settings)
└── workflow-engine/  # DAG-based AI and system automation execution engine
```

## 1.2 Technology Stack Decisions

### Systems & Performance Critical (Rust)
- **Why:** Memory safety, predictable latency (no GC pauses), exceptional concurrency.
- **Where:** `/core`, `/shell/compositor`, `/system-services`, `/agent-engine/sandbox`.
- **Tools:** `tokio` for async, `tonic` for gRPC/IPC, `tracing` for observability.

### AI Runtime & Orchestration (Python)
- **Why:** De facto standard for AI ecosystems, native support for ML libraries, rapid agent iteration.
- **Where:** `/ai-runtime`, `/workflow-engine`, portions of `/agent-engine`.
- **Tools:** `FastAPI` for internal services, `LangChain`/`LlamaIndex` concepts (custom built for low latency), `Playwright` for browser automation.

### Desktop Shell & UI (TypeScript + Rust/Tauri OR GTK4)
- **Decision:** Hybrid approach. We use **Tauri (Rust + React/TypeScript)** for the Ena Bar to allow rapid UI iteration and rich interactive components, while utilizing **GTK4/libadwaita** for native OS applications (Settings, Files) to maintain standard Linux desktop feel.
- **Compositor:** Wayland-native, potentially building on wlroots or Smithay (Rust) to ensure deep integration with the AI's spatial awareness of windows.

### Data Layer
- **Relational & Vector:** PostgreSQL with `pgvector`. Provides robust transactional guarantees for system state and high-performance similarity search for AI memory.
- **Event Bus / Queues:** Redis (or NATS). High throughput, low latency pub/sub for cross-component messaging (e.g., Ena Bar sending a command to the Workflow Engine).

### Local Inference
- **Engine:** Ollama integrated natively. EnaOS ships with local LLMs (e.g., Llama 3 or Mistral) for privacy-preserving, offline-capable core OS functions.

## 1.3 Development Environment Setup & Container Strategy
- **NixOS & Flakes:** The primary source of truth for the dev environment. `flake.nix` defines exact versions of Rust, Python, Node, Postgres, and Wayland dependencies. This guarantees "works on my machine" consistency.
- **Docker/Podman:** Used for isolating AI agents. When an agent needs to execute potentially unsafe Python code or interact with a headless browser, it spins up a highly constrained ephemeral container.
- **direnv:** Automatically loads the Nix environment upon entering the directory.

## 1.4 Build System & CI/CD
- **Build System:** `just` (Justfile) for task running. Cargo for Rust, Poetry for Python, pnpm for TypeScript.
- **CI/CD:** GitHub Actions or GitLab CI.
  - **Gates:** `cargo clippy`, `rustfmt`, `ruff` (Python), `eslint`, `tsc`.
  - **Testing:** Unit tests per language, Integration tests spinning up the IPC event bus and simulating Ena Bar commands.
  - **Artifacts:** Nightly AppImages/Flatpaks for UI components, and raw binaries for system daemons.

## 1.5 Git Strategy
- **Trunk-Based Development:** Short-lived feature branches branching from `main`.
- **Conventional Commits:** Required for automated changelog generation and semantic versioning of the SDKs.
- **Monorepo Tooling:** `turborepo` or `cargo-workspace` to only build and test affected DAG dependencies on PRs.