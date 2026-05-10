# 6. Initial Milestone Roadmap

## Milestone 1: The Foundation (Months 1-2)
**Goal: IPC, Shell, and Basic Ena Bar**
- Set up monorepo (`cargo workspace`, `turborepo`).
- Define Protobuf schemas for IPC.
- Build basic Wayland compositor (Smithay).
- Build Ena Bar UI (Tauri) anchoring to the bottom of the screen.
- Establish Redis/NATS event bus.

## Milestone 2: The Brain (Months 3-4)
**Goal: AI Runtime and Memory Engine**
- Implement Python AI Runtime with FastAPI.
- Integrate Ollama for local fallback.
- Integrate OpenAI/Anthropic SDKs.
- Setup PostgreSQL + pgvector.
- Implement the "Context Injector" - capturing active window titles and feeding them to the prompt.

## Milestone 3: The Hands (Months 5-6)
**Goal: Agent Engine and Workflows**
- Build the Agent Orchestrator (sandboxed Podman containers).
- Implement the capability/permission system.
- Build standard tools (File Reader, Browser Automation via Playwright).
- Create the first autonomous agent (e.g., "Summarize my inbox").

## Milestone 4: The Ecosystem (Months 7-8)
**Goal: Plugins, Polish, and Developer Preview**
- Finalize the Plugin WASM architecture.
- Release SDKs (Rust, Python, TS).
- Build the "Settings" and "Files" native GTK4 apps integrating with the OS daemon.
- Launch Developer Preview ISO (NixOS based).