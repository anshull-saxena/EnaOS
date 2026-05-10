# 3. AI, Agents, and Workflow Runtime

## 3.1 AI Runtime Architecture
The AI Runtime (`ena-ai`) is a Python daemon built with FastAPI, LangGraph/LangChain, and LiteLLM concepts.

**Components:**
1. **Provider Router:** Abstracts LLM APIs. If the user asks a simple OS question, it routes to local Ollama (Llama-3). If the user asks to summarize a 50-page PDF, it routes to Anthropic Claude 3.5 Sonnet or OpenAI GPT-4o based on user config.
2. **Context Injector:** Before any prompt is sent, this layer injects real-time context from the Memory Engine (e.g., "The user is currently looking at a terminal running a rust build error").
3. **Streaming Engine:** All interactions are streamed via Server-Sent Events (SSE) or WebSockets back to the Ena Bar for instant feedback.

## 3.2 Local AI Inference Architecture
Privacy is paramount. EnaOS ships with local capabilities out-of-the-box.
- **Ollama Integration:** The OS manages an internal Ollama service.
- **Model Management:** The OS pulls, updates, and unloads models dynamically to save VRAM.
- **GPU Acceleration:** The NixOS configuration automatically provisions CUDA/ROCm/Metal drivers ensuring local models run hardware-accelerated.
- **Fallback Chain:** Tasks attempting to use local inference gracefully fail over to cloud providers if local hardware lacks the VRAM, asking user permission first.

## 3.3 Agent Lifecycle Architecture
Agents in EnaOS are autonomous, background-running entities that accomplish multi-step tasks.

1. **Spawning:** The AI Runtime determines a request requires an agent (e.g., "Scrape this website and build a spreadsheet"). It requests the Agent Orchestrator to spawn an instance.
2. **Sandboxing:** The Agent Orchestrator spins up an ephemeral Podman/Docker container or a restricted WASM runtime (like Wasmtime). This prevents malicious code execution.
3. **Execution & Capabilities:** Agents are injected with tools. To use a tool (e.g., "Write File"), the agent sends a request over IPC. The Agent Engine checks the sandbox permissions before fulfilling it.
4. **Termination:** Upon success or failure, the container is destroyed, and the result is logged to the Memory Engine.

## 3.4 Workflow Execution Design
Workflows are deterministic or semi-deterministic DAGs (Directed Acyclic Graphs).
- **Trigger:** Time-based (Cron), Event-based (e.g., "When I open VS Code"), or AI-triggered.
- **Nodes:** Nodes can be shell scripts, API calls, or LLM prompts.
- **Engine:** A lightweight Rust executor that parses YAML/JSON workflow definitions and executes them concurrently, emitting progress events to the Event Bus.

## 3.5 Plugin Architecture
Extensibility is core to EnaOS.
- **Architecture:** Plugins are WASM modules or standalone local binaries communicating via gRPC.
- **Manifest:** Every plugin has an `ena-plugin.json` declaring its desired capabilities (e.g., `read_files`, `desktop_notifications`).
- **Discovery:** The Ena Bar dynamically loads plugin capabilities and injects them as tools into the LLM context.
- **Example:** A Spotify plugin registers the `pause_music` and `search_song` tools, making them natively available to the AI via voice or text.