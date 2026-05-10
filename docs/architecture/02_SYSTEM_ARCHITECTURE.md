# 2. System Architecture & Boundaries

## 2.1 Service Boundaries
EnaOS is built on a microkernel-inspired service architecture. Services run as independent processes (daemons) managed by Systemd, communicating over a structured Event Bus and IPC layer.

1. **Ena Shell / Compositor:** Handles window rendering, input events, and Wayland protocol.
2. **Ena Bar:** The user-facing overlay. Entirely decoupled from the compositor, running as a specialized Wayland layer-shell client.
3. **Core OS Daemon (`enad`):** The central privileged service handling system automation, D-Bus bridging, and hardware access.
4. **AI Runtime (`ena-ai`):** Unprivileged Python daemon handling NLP, embeddings, and provider routing.
5. **Agent Orchestrator:** Manages lifecycle of autonomous agents, spinning up sandboxes.
6. **Memory Engine:** Abstracted database access layer.

## 2.2 Inter-Process Communication (IPC) & Event Architecture

### The Dual-Layer Strategy
We employ a two-tiered communication strategy depending on the requirement.

1. **gRPC / Protocol Buffers (Synchronous, Point-to-Point)**
   - Used for hard contracts, request/response cycles.
   - Example: Ena Bar requests the AI Runtime to generate a response, waiting for the stream.
   - Example: Shell requests Auth daemon to verify a fingerprint.
   - **Why:** Strongly typed, auto-generated SDKs across Rust/Python/TS, extremely fast.

2. **Event Bus - NATS / Redis Pub/Sub (Asynchronous, Broadcast)**
   - Used for state changes, broadcast events, and decoupling.
   - Example: Compositor publishes `WindowFocusChanged(app="browser")`. AI Runtime listens to this to update the user's active context in the Memory Engine.
   - Example: Agent publishes `TaskProgress(percent=50)`. Ena Bar listens to update the UI without needing to poll the Agent.

## 2.3 State Management Approach
State in EnaOS is heavily distributed but globally queryable.
- **UI State:** Handled locally within the Ena Bar (React/Zustand) or GTK apps.
- **System State:** Owned by `enad` and exposed via gRPC.
- **Contextual/AI State:** This is the core innovation. A rolling window of the user's last X minutes of activity (screen OCR, active window, typed text) is maintained in-memory (Redis) and periodically flushed to the Vector/Relational DB for long-term semantic retrieval.

## 2.4 Desktop Shell Architecture
EnaOS is not just a skin over GNOME. It is a custom Wayland environment.
- **Compositor Engine:** Built on `Smithay` (Rust). It provides full control over window placement.
- **AI Spatial Awareness:** The compositor exposes an API that allows the AI to query the screen coordinate graph. The AI can highlight windows, draw overlays, or move applications.
- **Layer Shell:** The Ena Bar uses `wlr-layer-shell` to anchor itself to the bottom of the screen, bypassing normal window management rules to appear omnipresent.

## 2.5 System Daemon Structure (`enad`)
The `enad` daemon acts as the bridge between standard Linux systems (Systemd, D-Bus, udev) and the AI ecosystem.
- **D-Bus Proxy:** It listens to D-Bus signals (e.g., NetworkManager, UPower) and translates them into EnaOS Pub/Sub events.
- **Automation Executor:** If the AI decides to "Turn off Wi-Fi", it sends a gRPC command to `enad`, which safely executes the required privileged `nmcli` or D-Bus call.
- **Privilege Separation:** `enad` is the ONLY component running as root. AI models and agents run as restricted users and MUST request actions through `enad`'s validated gRPC endpoints.