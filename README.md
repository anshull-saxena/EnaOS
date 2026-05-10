# 🌌 EnaOS

### **The AI-Native Operating System for autonomous work.**

EnaOS is a radical rethink of the desktop environment. Instead of a grid of isolated applications, EnaOS provides a unified, intent-driven interface that orchestrates AI agents, manages deep contextual memory, and executes complex workflows directly within the OS kernel.

---

## 📍 Progress Tracker

- [x] **Monorepo Architecture:** Professional scaffold for a polyglot system.
- [x] **Cinematic Landing Page:** Production-grade marketing presence.
- [x] **Waitlist Engine:** Automated synthesis registration via Supabase.
- [x] **Design System:** "Obsidian & Glass" theme defined.
- [ ] **Ena Bar (Desktop):** Porting the HUD from web to a native system layer.
- [ ] **Core Orchestrator:** Rust-based daemon for system-level task management.
- [ ] **Vector Memory Engine:** Persistent contextual storage for user intent.
- [ ] **Local Inference Bridge:** Stable Ollama/Llama.cpp system integration.

---

## 🎯 Current Focus (Top 5)

1.  **Native Ena Bar:** Transitioning the floating HUD into a high-performance desktop application.
2.  **Kernel Orchestrator:** Building the Rust core that manages agent lifecycles and system IPC.
3.  **Context Synthesis:** Finalizing the schema for how short-term intent becomes long-term memory.
4.  **Agent Handoffs:** Refining the "Baton-Passing" protocol for multi-agent task execution.
5.  **Local LLM Optimization:** Tuning local models for zero-latency desktop interaction.

---

## 🛡 Why EnaOS?

Traditional operating systems were designed for a world where humans manually operate tools. In the age of AI, this model is the bottleneck.

> "Computing hasn't changed fundamentally in 40 years. We still open files, switch between apps, and manually move data. EnaOS is built for a future where your computer doesn't just store data—it understands your intent."

*   **App-Agnostic:** Stop opening tools. Start achieving goals.
*   **Local-First:** Privacy-centric intelligence powered by local inference.
*   **Context-Aware:** A system that remembers the "why" behind every task.

---

## ⚡ What it does

| Capability | Status | Outcome |
| :--- | :--- | :--- |
| **The Ena Bar** | `IN-DEVELOPMENT` | A floating, omnipresent HUD for voice, text, and multimodal intent. |
| **Agentic Kernel** | `PROTOTYPING` | Native orchestration for Research, Coding, and Analysis agents. |
| **Contextual Memory** | `PENDING` | A system-wide vector store that links past interactions to current tasks. |
| **Execution Trace** | `IN-DEVELOPMENT` | Complete visibility into AI reasoning and autonomous actions. |
| **Local Inference** | `STABLE` | First-class support for Llama 3 and DeepSeek via Ollama integration. |

---

## 🧠 Architecture Overview

EnaOS is structured as a high-performance monorepo, utilizing Rust for system-level safety and Python for AI runtime orchestration.

```text
       [ USER INTENT ]
              │
      ┌───────▼───────┐
      │   ENA BAR     │ (Global HUD / Shell UI)
      └───────┬───────┘
              │
    ┌─────────┴─────────┐
    │  CORE ORCHESTRATOR│ (Rust-based System Daemon - PENDING)
    └─────────┬─────────┘
      ┌───────┼───────┐
┌─────▼────┐┌─▼───────┐┌─────▼────┐
│AI RUNTIME││ MEMORY  ││  SHELL   │
│(Local/API)││(Vector) ││ (Wayland)│
└──────────┘└─────────┘└──────────┘
```

The system operates on a "Baton-Passing" loop, where intent is parsed by the kernel, assigned to specific agents, and executed within a secure sandbox while maintaining a live execution trace for the user.

---

## 🚀 Quick Start

### Path A: Building the Environment
EnaOS is currently in early developer preview. You can initialize the project structure and marketing layer:

```bash
# Clone the monorepo
git clone https://github.com/EnaOS/EnaOS.git && cd EnaOS

# Install dependencies (Node.js 18+ required)
cd apps/landing-page && npm install

# Run the cinematic landing page locally
npm run dev
```

### Path B: CLI Installer
> ⚠️ **Status: Pending.** The automated install script is currently being synthesized.

---

## 🧩 Core Modules

### 📡 Ena Bar (`/ena-bar`)
The centerpiece of the OS. A floating interaction layer that replaces the taskbar. It uses `framer-motion` for fluid HUD expansion and real-time audio visualization.

### 🤖 Agent Engine (`/agent-engine`)
Handles the lifecycle of autonomous workers. It is designed to support multi-agent handoffs, allowing a "Researcher Agent" to pass findings to a "Coding Agent."

### 💾 Memory Engine (`/memory-engine`)
`UNDER DEVELOPMENT` — A persistent contextual layer built on a vector-graph hybrid database. It ensures system-wide context retrieval across different sessions.

---

## 🛠 Planned Use Cases

*   **Autonomous PR Management:** "Ena, review the latest issue on GitHub and draft a fix in the `/core` directory."
*   **Deep Research Synthesis:** "Research advancements in solid-state batteries and prepare a technical brief."
*   **Contextual Debugging:** "Explain why this Rust build is failing based on my changes from today."

---

## ⚙️ Configuration & Extensibility

### Configuration
EnaOS will be configurable via `config.yaml`.
> ⚠️ **Status: Specification Pending.**

### Extensibility
Build your own agents using the Ena SDK.
*   **Rust SDK:** `/sdk/rust` — For high-performance system agents.
*   **TypeScript SDK:** `/sdk/typescript` — For UI-driven extensions.

---

## 🛡 Security & Reliability

*   **Sandboxed Execution:** `PENDING` — Autonomous actions are designed to run in isolated environments.
*   **Human-in-the-Loop:** High-impact actions require manual approval via the Ena Bar.
*   **Transparency:** Every "thought" and "action" is logged in the Execution Trace for user auditing.

---

## 📚 Docs & Support

| Target | Resource |
| :--- | :--- |
| **Project Status** | [Roadmap & Synthesis Stage](#) |
| **Developer Guide** | [Building for EnaOS](#) |
| **Community** | [Join the Discord](#) |

---

## 📄 License

EnaOS is released under the **MIT License**. Built by the community for the autonomous age.
