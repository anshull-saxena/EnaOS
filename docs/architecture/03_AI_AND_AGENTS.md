# 3. AI Runtime & Agent Architecture

> **Status:** Accurate as of v0.1.0-developer-preview
> **Last verified:** June 2026

## 3.1 AI Runtime Architecture

The AI Runtime (`ai-runtime`) is a **Python FastAPI server** that provides LLM-powered features to EnaOS.

### Components

1. **API Server** (`src/api/server.py` + `src/api/routes.py`)
   - FastAPI application with uvicorn
   - Endpoints: `/health`, `/context`, `/chat`, `/chat/stream` (SSE), `/memory`, `/action`, `/orchestrate`
   - CORS enabled for local development
   - Port: 8900

2. **Enad Bridge** (`src/bridge/enad.py`)
   - Unix socket client connecting to enad
   - Subscribes to all event kinds
   - Maintains live `DesktopState` from received events
   - Sends commands and receives responses

3. **Inference** (`src/inference/`)
   - **Provider router** (`provider.py`) — routes to Ollama (local) or configurable cloud API
   - **Prompt builder** (`prompt.py`) — injects desktop context into system prompts
   - **Ollama client** (`ollama.py`) — async HTTP client for Ollama API

4. **Orchestration** (`src/orchestration/`)
   - **Plan parser** (`planner.py`) — LLM-parses natural language intents into structured `ExecutionPlan` JSON
   - Plan format: DAG of actions with dependency edges

5. **Context** (`src/context/`)
   - **Session manager** (`sessions.py`) — in-memory conversation history
   - **State manager** (`state.py`) — live desktop state aggregation

### Data Flow

```
User query → ena-bar → enad (IPC) → AI Runtime (HTTP) → Ollama/Cloud
                                          │
                                    Context injection
                                    (desktop state, sessions)
                                          │
                                    Response → enad → ena-bar
```

## 3.2 Local AI Inference

EnaOS supports **local-first inference** via Ollama:

- **Model:** llama3.2 or compatible (configurable)
- **Startup:** `ollama serve` (manual, not auto-started by enad yet)
- **Integration:** AI Runtime connects to `http://localhost:11434`
- **Streaming:** Server-Sent Events for real-time token delivery

### Provider Router
- Simple queries → local Ollama (fast, private)
- Complex queries → cloud API if configured (OpenAI, Anthropic)
- Fallback: if Ollama unavailable, returns informative error

## 3.3 Agent Architecture

EnaOS currently has **no autonomous agent execution**. The `SpawnAgent` IPC command exists as a stub for future implementation.

### Planned Agent Architecture (Future)
- Sandboxed execution via Podman containers
- Capability-based permission model
- WASM-based plugin SDK (future)

### Current Capabilities
- **Orchestration Engine** executes DAG-based plans with retry and rollback
- **Action Executor** runs individual actions (open app, focus window, etc.)
- No autonomous agents run in v0.1.0

## 3.4 Workflow Execution

Workflows are **DAGs (Directed Acyclic Graphs)** of typed actions:

```
Plan: "Setup development environment"
├── Open editor (requires_approval: false)
├── Start dev server (requires_approval: false)
└── Open docs (requires_approval: false)
```

### Engine Features
- Topological sort for dependency ordering
- `EdgeCondition`: Success (default), Always, OnFailure
- Retry with configurable max attempts and exponential backoff
- Rollback in reverse completion order on failure
- Approval flow: plans can be marked for user approval before execution

## 3.5 Plugin Architecture

EnaOS has **no plugin SDK** in v0.1.0. The architecture supports future WASM-based plugins, but this is not implemented.

**Current extensibility points:**
- New `ActionType` variants in `actions/types.rs`
- New `Command` variants in `types/ipc.rs`
- New `EventPayload` variants in `types/events.rs`
- All IPC types are open for extension
