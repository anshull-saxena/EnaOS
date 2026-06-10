# 4. Data Layer & Memory

> **Status:** Accurate as of v0.1.0-developer-preview
> **Last verified:** June 2026

## 4.1 Persistence Architecture

EnaOS uses **SQLite** for all persistence. There are three separate databases:

```
~/.local/share/enad/
├── snapshots.db    — Workspace snapshot store (WAL mode)
├── memory.db       — Working memory store (WAL mode, FTS5)
└── suggestions.db  — Ambient suggestion store
```

### Design Decisions
- **SQLite over PostgreSQL** — single-machine desktop daemon doesn't need a separate database server
- **No pgvector** — memory queries use SQLite FTS5 full-text search
- **No Redis** — in-memory state is sufficient for event bus and current desktop context
- **Local-first** — all data stays on the machine; no cloud sync in v0.1.0

## 4.2 Snapshot Store (`snapshots.db`)

### Schema
```sql
CREATE TABLE snapshots (
    id TEXT PRIMARY KEY,           -- UUID v4
    label TEXT NOT NULL DEFAULT '', -- Human-readable label
    created_at TEXT NOT NULL,       -- RFC 3339 timestamp
    is_auto INTEGER NOT NULL DEFAULT 0, -- Auto-snapshot flag
    windows TEXT NOT NULL DEFAULT '[]',  -- JSON array of WindowSnapshot
    terminals TEXT NOT NULL DEFAULT '[]', -- JSON array of TerminalSnapshot
    workspace_name TEXT NOT NULL DEFAULT '',
    active_project TEXT,
    is_restored INTEGER NOT NULL DEFAULT 0,
    restored_at TEXT,               -- When the snapshot was last restored
    restored_plan_id TEXT           -- Orchestration plan ID for last restore
);
CREATE INDEX idx_snapshots_created ON snapshots(created_at DESC);
```

### Operations
- **Capture:** `TakeSnapshot` (manual) or auto-snapshot timer
- **List:** `ListSnapshots` with optional limit, returns `SnapshotSummary[]`
- **Get:** `GetSnapshot` by ID, returns full `WorkspaceSnapshot`
- **Delete:** `DeleteSnapshot` by ID
- **Mark Restored:** Internal tracking linking to orchestration plan

### Auto-Snapshot
- Interval: configurable (default: 5 minutes)
- Trigger conditions: significant workspace changes
- Not yet implemented for v0.1.0 — only manual snapshots

## 4.3 Memory Store (`memory.db`)

### Schema
```sql
CREATE TABLE memory (
    id TEXT PRIMARY KEY,           -- UUID v4
    entry_type TEXT NOT NULL,       -- action, intent, context_snapshot, workspace_snapshot
    summary TEXT NOT NULL,          -- Human-readable description
    data TEXT,                      -- Optional JSON payload
    workspace TEXT,                 -- Optional workspace context
    app TEXT,                       -- Optional app context
    source TEXT,                    -- Event source (upower, window, etc.)
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Full-text search virtual table
CREATE VIRTUAL TABLE memory_fts USING fts5(
    summary, data, content='memory', content_rowid='rowid'
);

-- Trigger to keep FTS5 in sync
CREATE TRIGGER memory_ai AFTER INSERT ON memory BEGIN
    INSERT INTO memory_fts(rowid, summary, data) VALUES (new.rowid, new.summary, new.data);
END;
```

### Operations
- **Insert:** Via `MemoryCapture` event subscriber
- **Query:** By type, workspace, app, or FTS5 full-text search
- **Summary:** Entry counts per type, workspace distribution, recent intents
- **No auto-expiry in v0.1.0** — future: pruning worker for old entries

### Memory Types
- `action` — User actions executed
- `intent` — Classified user intents
- `context_snapshot` — Periodic desktop state snapshots
- `workspace_snapshot` — References to SnapshotStore snapshots

## 4.4 Suggestion Store (`suggestions.db`)

### Schema
```sql
CREATE TABLE suggestions (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    subtitle TEXT,
    action_label TEXT,
    action_type TEXT,
    action_params TEXT,
    priority REAL NOT NULL DEFAULT 0.5,
    context_hash TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT,
    dismissed INTEGER NOT NULL DEFAULT 0,
    dismissed_at TEXT,
    dismiss_permanent INTEGER NOT NULL DEFAULT 0
);
```

### Operations
- **Insert:** Via `SuggestionEngine` on event patterns
- **Query:** Active suggestions (not expired, not dismissed)
- **Dismiss:** Temporary (same session) or permanent
- **Cleanup:** Periodic sweep of expired entries (5-minute interval)

## 4.5 Data Privacy & Lifecycle

### Local-Only
- All databases are local — no data leaves the machine
- AI Runtime can be configured to use cloud APIs, but system event data is never sent
- Only user queries (not desktop state) go to cloud LLMs when configured

### Workspace Snapshots
- Contain window titles, terminal sessions, and workspace layout
- No file contents — just metadata about open applications
- Stored locally in `~/.local/share/enad/snapshots.db`

### Working Memory
- Contains classified intents and system events
- Used for contextual command suggestions
- No keystroke logging or raw clipboard content storage

### Data Retention
- v0.1.0: no automatic pruning (manual deletion via IPC)
- Future: periodic summarization + expiry of old entries
- User can delete all data by removing `~/.local/share/enad/`
