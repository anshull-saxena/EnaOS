/// SQLite-backed working memory store.
///
/// Provides persistent storage for EnaOS memory with:
/// - FTS5 full-text search for semantic recall
/// - Automatic expiration and cleanup
/// - Recency-weighted relevance scoring
/// - Workspace-tagged entries
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use tracing::info;

use crate::memory::types::{MemoryEntry, MemoryQuery, MemorySummary, MemoryType};

/// Working memory store backed by SQLite.
pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl MemoryStore {
    /// Create or open a memory store at the given path.
    pub fn open(path: &str) -> Result<Self, String> {
        // Ensure parent directory exists.
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn =
            Connection::open(path).map_err(|e| format!("Failed to open memory database: {e}"))?;

        let store = Self {
            conn: Mutex::new(conn),
        };

        store.init_schema()?;
        info!("Memory store opened at {path}");

        Ok(store)
    }

    /// Initialize the database schema.
    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        // Main memory entries table.
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memory_entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                workspace TEXT,
                summary TEXT NOT NULL,
                details TEXT NOT NULL DEFAULT '{}',
                relevance REAL NOT NULL DEFAULT 1.0
            );

            -- Index for fast type + workspace queries.
            CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(entry_type);
            CREATE INDEX IF NOT EXISTS idx_memory_workspace ON memory_entries(workspace);
            CREATE INDEX IF NOT EXISTS idx_memory_timestamp ON memory_entries(timestamp DESC);

            -- FTS5 virtual table for full-text search.
            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                summary, details,
                content='memory_entries',
                content_rowid='id'
            );

            -- Triggers to keep FTS index in sync.
            CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_fts(rowid, summary, details)
                VALUES (new.id, new.summary, new.details);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, summary, details)
                VALUES ('delete', old.id, old.summary, old.details);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, summary, details)
                VALUES ('delete', old.id, old.summary, old.details);
                INSERT INTO memory_fts(rowid, summary, details)
                VALUES (new.id, new.summary, new.details);
            END;
            ",
        )
        .map_err(|e| format!("Schema init failed: {e}"))?;

        Ok(())
    }

    /// Insert a new memory entry.
    pub fn insert(
        &self,
        entry_type: MemoryType,
        workspace: Option<&str>,
        summary: &str,
        details: &serde_json::Value,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let now = Utc::now();
        let details_str = serde_json::to_string(details).unwrap_or_default();

        conn.execute(
            "INSERT INTO memory_entries (timestamp, entry_type, workspace, summary, details, relevance)
             VALUES (?1, ?2, ?3, ?4, ?5, 1.0)",
            params![
                now.to_rfc3339(),
                entry_type.to_string(),
                workspace,
                summary,
                details_str,
            ],
        )
        .map_err(|e| format!("Insert failed: {e}"))?;

        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// Query memory entries with filtering and relevance ranking.
    pub fn query(&self, q: &MemoryQuery) -> Result<Vec<MemoryEntry>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut sql = String::from(
            "SELECT id, timestamp, entry_type, workspace, summary, details, relevance
             FROM memory_entries WHERE 1=1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        // Filter by entry types.
        if !q.entry_types.is_empty() {
            let placeholders: Vec<_> = (0..q.entry_types.len()).map(|_| "?").collect();
            sql.push_str(&format!(" AND entry_type IN ({})", placeholders.join(",")));
            for t in &q.entry_types {
                params_vec.push(Box::new(t.to_string()));
            }
        }

        // Filter by workspace.
        if let Some(ref ws) = q.workspace {
            sql.push_str(" AND workspace = ?");
            params_vec.push(Box::new(ws.clone()));
        }

        // Filter by time.
        if let Some(since) = q.since {
            sql.push_str(" AND timestamp > ?");
            params_vec.push(Box::new(since.to_rfc3339()));
        }

        // Full-text search.
        if let Some(ref search) = q.search {
            // Use FTS5 for search.
            sql = "SELECT m.id, m.timestamp, m.entry_type, m.workspace, m.summary, m.details, m.relevance
                 FROM memory_entries m
                 JOIN memory_fts f ON m.id = f.rowid
                 WHERE memory_fts MATCH ? AND 1=1".to_string();

            if !q.entry_types.is_empty() {
                let placeholders: Vec<_> = (0..q.entry_types.len()).map(|_| "?").collect();
                sql.push_str(&format!(
                    " AND m.entry_type IN ({})",
                    placeholders.join(",")
                ));
                for t in &q.entry_types {
                    params_vec.push(Box::new(t.to_string()));
                }
            }
            if let Some(ref ws) = q.workspace {
                sql.push_str(" AND m.workspace = ?");
                params_vec.push(Box::new(ws.clone()));
            }
            if let Some(since) = q.since {
                sql.push_str(" AND m.timestamp > ?");
                params_vec.push(Box::new(since.to_rfc3339()));
            }

            // Insert search term as first param.
            let mut final_params: Vec<&dyn rusqlite::types::ToSql> = vec![&search];
            for p in &params_vec {
                final_params.push(p.as_ref());
            }

            sql.push_str(" ORDER BY rank DESC, m.timestamp DESC LIMIT ?");
            final_params.push(&q.limit);

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| format!("Prepare failed: {e}"))?;
            let rows = stmt
                .query_map(rusqlite::params_from_iter(final_params.iter()), |row| {
                    parse_entry(row)
                })
                .map_err(|e| format!("Query failed: {e}"))?;

            return rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Row parse: {e}"));
        }

        // No search — order by recency.
        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");

        let mut final_params: Vec<&dyn rusqlite::types::ToSql> = Vec::new();
        for p in &params_vec {
            final_params.push(p.as_ref());
        }
        final_params.push(&q.limit);

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("Prepare failed: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(final_params.iter()), |row| {
                parse_entry(row)
            })
            .map_err(|e| format!("Query failed: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row parse: {e}"))
    }

    /// Get a summary of the current memory state.
    pub fn summary(&self) -> Result<MemorySummary, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .unwrap_or(0);

        let oldest: Option<String> = conn
            .query_row("SELECT MIN(timestamp) FROM memory_entries", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten();

        let newest: Option<String> = conn
            .query_row("SELECT MAX(timestamp) FROM memory_entries", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .ok()
            .flatten();

        // Entry type counts.
        let mut type_counts = serde_json::Map::new();
        let mut stmt = conn
            .prepare("SELECT entry_type, COUNT(*) FROM memory_entries GROUP BY entry_type")
            .map_err(|e| format!("Prepare failed: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| format!("Query failed: {e}"))?;

        for (t, c) in rows.flatten() {
            type_counts.insert(t, serde_json::Value::Number(c.into()));
        }

        // Distinct workspaces.
        let workspaces: Vec<String> = conn.prepare(
            "SELECT DISTINCT workspace FROM memory_entries WHERE workspace IS NOT NULL ORDER BY workspace"
        ).ok().map(|mut stmt| {
            stmt.query_map([], |row| row.get(0)).ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        }).unwrap_or_default();

        // Recent intents (last 5).
        let recent_intents: Vec<String> = conn.prepare(
            "SELECT summary FROM memory_entries WHERE entry_type = 'intent' ORDER BY timestamp DESC LIMIT 5"
        ).ok().map(|mut stmt| {
            stmt.query_map([], |row| row.get(0)).ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        }).unwrap_or_default();

        // Recent actions (last 5).
        let recent_actions: Vec<String> = conn.prepare(
            "SELECT summary FROM memory_entries WHERE entry_type = 'action' ORDER BY timestamp DESC LIMIT 5"
        ).ok().map(|mut stmt| {
            stmt.query_map([], |row| row.get(0)).ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        }).unwrap_or_default();

        // Current context (most recent context snapshot).
        let current_context: serde_json::Value = conn.query_row(
            "SELECT details FROM memory_entries WHERE entry_type = 'context_snapshot' ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| {
                let s: String = row.get(0)?;
                Ok(serde_json::from_str(&s).unwrap_or(serde_json::json!({})))
            },
        ).unwrap_or(serde_json::json!({}));

        Ok(MemorySummary {
            total_entries: total,
            oldest_entry: oldest.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            }),
            newest_entry: newest.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            }),
            entry_counts: serde_json::Value::Object(type_counts),
            workspaces,
            recent_intents,
            recent_actions,
            current_context,
        })
    }

    /// Expire old entries beyond the configured TTL.
    pub fn expire(&self, max_age_hours: i64) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let cutoff = (Utc::now() - Duration::hours(max_age_hours)).to_rfc3339();

        let deleted = conn
            .execute(
                "DELETE FROM memory_entries WHERE timestamp < ?",
                params![cutoff],
            )
            .map_err(|e| format!("Expire failed: {e}"))?;

        if deleted > 0 {
            info!("Memory: expired {deleted} entries older than {max_age_hours}h");
        }

        Ok(deleted)
    }

    /// Compact the database (vacuum).
    pub fn compact(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        conn.execute("VACUUM", [])
            .map_err(|e| format!("Vacuum failed: {e}"))?;
        Ok(())
    }

    /// Get entry count.
    pub fn count(&self) -> Result<i64, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        conn.query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .map_err(|e| format!("Count failed: {e}"))
    }
}

fn parse_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    let id: i64 = row.get(0)?;
    let timestamp_str: String = row.get(1)?;
    let entry_type_str: String = row.get(2)?;
    let workspace: Option<String> = row.get(3)?;
    let summary: String = row.get(4)?;
    let details_str: String = row.get(5)?;
    let relevance: f32 = row.get(6)?;

    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let entry_type = entry_type_str
        .parse::<MemoryType>()
        .unwrap_or(MemoryType::Event);
    let details = serde_json::from_str(&details_str).unwrap_or(serde_json::json!({}));

    Ok(MemoryEntry {
        id,
        timestamp,
        entry_type,
        workspace,
        summary,
        details,
        relevance,
    })
}
