# 4. Data Layer & Memory Engine

## 4.1 Memory Engine Architecture
The Memory Engine provides EnaOS with continuous context. It acts as the OS's hippocampus.

**Layers:**
1. **Short-Term Memory (Redis):** Holds the current active state. What windows are open? What is the user typing right now? What is currently on the screen (via periodic OCR)? Fast, ephemeral, expires after 10-15 minutes.
2. **Working Memory (Graph DB - e.g., Neo4j or Postgres via Apache AGE):** Maps relationships. "User A opened Project B and frequently talks to Person C."
3. **Long-Term Memory (PostgreSQL + pgvector):** Historical logs and semantic embeddings of documents, chat histories, and significant system events.

## 4.2 Database Schema Suggestions
We utilize PostgreSQL for structured and vector data.

**`user_context_events` Table (Relational)**
- `id` (UUID)
- `timestamp` (TIMESTAMPTZ)
- `event_type` (Enum: APP_OPEN, TYPED_TEXT, BROWSER_NAV)
- `app_name` (VARCHAR)
- `metadata` (JSONB)

**`semantic_memory` Table (Vector)**
- `id` (UUID)
- `source_id` (UUID - FK to events or files)
- `content` (TEXT)
- `embedding` (VECTOR(1536) - or dynamic based on model)
- `created_at` (TIMESTAMPTZ)

**`agents_history` Table**
- `id` (UUID)
- `agent_id` (UUID)
- `task_description` (TEXT)
- `status` (Enum: PENDING, RUNNING, SUCCESS, FAILED)
- `result_data` (JSONB)

## 4.3 Data Privacy & Lifecycle
- **Local First:** All databases run locally. No personal data is sent to cloud providers unless explicitly authorized per-request (e.g., asking Claude to summarize a local file).
- **Pruning:** A background worker periodically summarizes older `user_context_events` into semantic embeddings and deletes the raw, high-volume event logs to save disk space.