use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use tracing::info;
use uuid::Uuid;

use super::types::{Suggestion, SuggestionKind, SuggestionSummary};

/// SQLite-backed store for ambient suggestions and dismissal memory.
pub struct SuggestionStore {
    conn: Mutex<Connection>,
}

impl SuggestionStore {
    /// Open or create the database at `path`.
    pub fn open(path: &str) -> Result<Self, String> {
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)
            .map_err(|e| format!("Failed to open suggestion database: {e}"))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        info!("Suggestion store opened at {path}");
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS suggestions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                suggestion_id TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                context_hash TEXT NOT NULL DEFAULT '',
                priority REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                action_label TEXT,
                action_type TEXT,
                action_payload TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_suggestions_active
                ON suggestions(expires_at DESC);

            CREATE TABLE IF NOT EXISTS dismissed_suggestions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                suggestion_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                context_hash TEXT NOT NULL DEFAULT '',
                dismissed_at TEXT NOT NULL,
                cooldown_until TEXT NOT NULL,
                permanent INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_dismissed_context
                ON dismissed_suggestions(context_hash);

            CREATE INDEX IF NOT EXISTS idx_dismissed_cooldown
                ON dismissed_suggestions(cooldown_until DESC);
            ",
        )
        .map_err(|e| format!("Suggestion schema init failed: {e}"))?;
        Ok(())
    }

    // ── Active suggestions ────────────────────────────────────

    /// Insert a new suggestion into the active pool.
    pub fn insert(&self, suggestion: &Suggestion) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        conn.execute(
            "INSERT OR REPLACE INTO suggestions
             (suggestion_id, kind, title, description, context_hash,
              priority, created_at, expires_at,
              action_label, action_type, action_payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                suggestion.id.to_string(),
                suggestion.kind.as_str(),
                suggestion.title,
                suggestion.description,
                suggestion.context_hash,
                suggestion.priority,
                suggestion.created_at.to_rfc3339(),
                suggestion.expires_at.to_rfc3339(),
                suggestion.action.as_ref().map(|a| a.label.as_str()),
                suggestion.action.as_ref().map(|a| a.action_type.as_str()),
                suggestion
                    .action
                    .as_ref()
                    .map(|a| serde_json::to_string(&a.payload).unwrap_or_default()),
            ],
        )
        .map_err(|e| format!("Suggestion insert failed: {e}"))?;
        Ok(())
    }

    /// List active (non-expired) suggestions, ordered by priority descending.
    pub fn list_active(&self, limit: usize) -> Result<Vec<SuggestionSummary>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let now = Utc::now().to_rfc3339();
        let mut stmt = conn
            .prepare(
                "SELECT suggestion_id, kind, title, description, priority, created_at,
                        action_label, action_type
                 FROM suggestions
                 WHERE expires_at > ?1
                 ORDER BY priority DESC
                 LIMIT ?2",
            )
            .map_err(|e| format!("Query prep failed: {e}"))?;

        let rows = stmt
            .query_map(params![now, limit as i64], |row| {
                let sid: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let title: String = row.get(2)?;
                let desc: String = row.get(3)?;
                let priority: f64 = row.get(4)?;
                let ts: String = row.get(5)?;
                let act_label: Option<String> = row.get(6)?;
                let act_type: Option<String> = row.get(7)?;
                let created_at = DateTime::parse_from_rfc3339(&ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());
                Ok(SuggestionSummary {
                    id: Uuid::parse_str(&sid).unwrap_or_default(),
                    kind,
                    title,
                    description: desc,
                    priority,
                    created_at,
                    action_label: act_label,
                    action_type: act_type,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row parse: {e}"))
    }

    /// Get a single suggestion by ID.
    pub fn get(&self, suggestion_id: &Uuid) -> Result<Option<Suggestion>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let mut stmt = conn
            .prepare(
                "SELECT suggestion_id, kind, title, description, context_hash,
                        priority, created_at, expires_at,
                        action_label, action_type, action_payload
                 FROM suggestions WHERE suggestion_id = ?1",
            )
            .map_err(|e| format!("Query prep failed: {e}"))?;

        let result = stmt.query_row(params![suggestion_id.to_string()], |row| {
            let sid: String = row.get(0)?;
            let kind_str: String = row.get(1)?;
            let title: String = row.get(2)?;
            let desc: String = row.get(3)?;
            let ch: String = row.get(4)?;
            let priority: f64 = row.get(5)?;
            let ts: String = row.get(6)?;
            let expires: String = row.get(7)?;
            let act_label: Option<String> = row.get(8)?;
            let act_type: Option<String> = row.get(9)?;
            let act_payload: Option<String> = row.get(10)?;

            let created_at = DateTime::parse_from_rfc3339(&ts)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let expires_at = DateTime::parse_from_rfc3339(&expires)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let action = act_label.zip(act_type).map(|(label, action_type)| {
                super::types::SuggestionAction {
                    label,
                    action_type,
                    payload: act_payload
                        .and_then(|p| serde_json::from_str(&p).ok())
                        .unwrap_or(serde_json::Value::Null),
                }
            });

            Ok(Suggestion {
                id: Uuid::parse_str(&sid).unwrap_or_default(),
                kind: SuggestionKind::from_str(&kind_str).unwrap_or(SuggestionKind::ContextHint),
                title,
                description: desc,
                context_hash: ch,
                priority,
                created_at,
                expires_at,
                action,
            })
        });

        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Suggestion get failed: {e}")),
        }
    }

    /// Remove a suggestion from the active pool.
    pub fn remove(&self, suggestion_id: &Uuid) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM suggestions WHERE suggestion_id = ?1",
                params![suggestion_id.to_string()],
            )
            .map_err(|e| format!("Suggestion remove failed: {e}"))?;
        Ok(deleted > 0)
    }

    /// Clean up expired suggestions.
    pub fn expire(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let now = Utc::now().to_rfc3339();
        let deleted = conn
            .execute(
                "DELETE FROM suggestions WHERE expires_at < ?1",
                params![now],
            )
            .map_err(|e| format!("Suggestion expire failed: {e}"))?;
        Ok(deleted)
    }

    // ── Dismissal memory ──────────────────────────────────────

    /// Record a dismissal. If `permanent`, the context hash is blocked forever.
    /// Otherwise, it respects per-kind cooldown.
    pub fn record_dismissal(
        &self,
        suggestion_id: &Uuid,
        kind: &SuggestionKind,
        context_hash: &str,
        permanent: bool,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let dismissed_at = Utc::now();
        let cooldown_until = if permanent {
            // Far future — effectively permanent
            Utc::now() + Duration::days(365 * 10)
        } else {
            Utc::now() + Duration::minutes(kind.cooldown_minutes() as i64)
        };

        conn.execute(
            "INSERT INTO dismissed_suggestions
             (suggestion_id, kind, context_hash, dismissed_at, cooldown_until, permanent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                suggestion_id.to_string(),
                kind.as_str(),
                context_hash,
                dismissed_at.to_rfc3339(),
                cooldown_until.to_rfc3339(),
                permanent as i32,
            ],
        )
        .map_err(|e| format!("Dismissal record failed: {e}"))?;

        // Also remove from active pool.
        let _ = conn.execute(
            "DELETE FROM suggestions WHERE suggestion_id = ?1",
            params![suggestion_id.to_string()],
        );

        Ok(())
    }

    /// Check if a context hash is currently blocked by a dismissal.
    pub fn is_context_blocked(&self, context_hash: &str) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let now = Utc::now().to_rfc3339();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dismissed_suggestions
                 WHERE context_hash = ?1 AND cooldown_until > ?2",
                params![context_hash, now],
                |row| row.get(0),
            )
            .map_err(|e| format!("Blocked check failed: {e}"))?;
        Ok(count > 0)
    }

    /// Clean up expired dismissal records.
    pub fn expire_dismissals(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let now = Utc::now().to_rfc3339();
        let deleted = conn
            .execute(
                "DELETE FROM dismissed_suggestions WHERE cooldown_until < ?1",
                params![now],
            )
            .map_err(|e| format!("Dismissal expire failed: {e}"))?;
        Ok(deleted)
    }
}
