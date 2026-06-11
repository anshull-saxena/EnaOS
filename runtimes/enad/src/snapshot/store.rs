use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, params};
use tracing::info;
use uuid::Uuid;

use crate::snapshot::types::{SnapshotSummary, WorkspaceSnapshot};

/// SQLite-backed workspace snapshot store.
pub struct SnapshotStore {
    conn: Mutex<Connection>,
}

impl SnapshotStore {
    pub fn open(path: &str) -> Result<Self, String> {
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn =
            Connection::open(path).map_err(|e| format!("Failed to open snapshot database: {e}"))?;

        let store = Self {
            conn: Mutex::new(conn),
        };

        store.init_schema()?;
        info!("Snapshot store opened at {path}");

        Ok(store)
    }

    fn init_schema(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS workspace_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_id TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                label TEXT NOT NULL DEFAULT '',
                is_auto INTEGER NOT NULL DEFAULT 0,
                is_restored INTEGER NOT NULL DEFAULT 0,
                env_checksum TEXT NOT NULL DEFAULT '',
                active_project TEXT,
                context_summary TEXT,

                -- JSON blobs
                workspaces_json TEXT NOT NULL DEFAULT '[]',
                applications_json TEXT NOT NULL DEFAULT '[]',
                windows_json TEXT NOT NULL DEFAULT '[]',
                terminals_json TEXT NOT NULL DEFAULT '[]',
                browser_urls_json TEXT NOT NULL DEFAULT '[]',
                orchestration_plans_json TEXT NOT NULL DEFAULT '[]',
                recent_actions_json TEXT NOT NULL DEFAULT '[]',
                ai_conversations_json TEXT NOT NULL DEFAULT '[]'
            );

            CREATE INDEX IF NOT EXISTS idx_snapshot_created ON workspace_snapshots(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_snapshot_label ON workspace_snapshots(label);
            CREATE INDEX IF NOT EXISTS idx_snapshot_auto ON workspace_snapshots(is_auto);

            -- Auto-expire snapshots older than 7 days.
            CREATE TABLE IF NOT EXISTS snapshot_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| format!("Snapshot schema init failed: {e}"))?;

        Ok(())
    }

    /// Store a workspace snapshot.
    pub fn insert(&self, snapshot: &WorkspaceSnapshot) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let to_json =
            |v: &serde_json::Value| serde_json::to_string(v).unwrap_or_else(|_| "[]".into());

        conn.execute(
            "INSERT INTO workspace_snapshots
             (snapshot_id, created_at, label, is_auto, is_restored, env_checksum,
              active_project, context_summary,
              workspaces_json, applications_json, windows_json,
              terminals_json, browser_urls_json,
              orchestration_plans_json, recent_actions_json, ai_conversations_json)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                snapshot.snapshot_id.to_string(),
                snapshot.created_at.to_rfc3339(),
                snapshot.label,
                snapshot.is_auto as i32,
                snapshot.env_checksum,
                snapshot.active_project,
                snapshot.context_summary,
                to_json(&serde_json::to_value(&snapshot.workspaces).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.applications).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.windows).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.terminals).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.browser_urls).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.orchestration_plans).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.recent_actions).unwrap_or_default()),
                to_json(&serde_json::to_value(&snapshot.ai_conversations).unwrap_or_default()),
            ],
        )
        .map_err(|e| format!("Snapshot insert failed: {e}"))?;

        Ok(())
    }

    /// Retrieve a full snapshot by ID.
    pub fn get(&self, snapshot_id: &Uuid) -> Result<Option<WorkspaceSnapshot>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT snapshot_id, created_at, label, is_auto, env_checksum,
                    active_project, context_summary,
                    workspaces_json, applications_json, windows_json,
                    terminals_json, browser_urls_json,
                    orchestration_plans_json, recent_actions_json, ai_conversations_json
             FROM workspace_snapshots WHERE snapshot_id = ?1",
            )
            .map_err(|e| format!("Prepare failed: {e}"))?;

        let result = stmt.query_row(params![snapshot_id.to_string()], |row| parse_snapshot(row));

        match result {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Snapshot get failed: {e}")),
        }
    }

    /// List snapshots (summaries, newest first).
    pub fn list(&self, limit: usize) -> Result<Vec<SnapshotSummary>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let mut stmt = conn
            .prepare(
                "SELECT snapshot_id, created_at, label, is_auto, is_restored,
                    applications_json, terminals_json, orchestration_plans_json
             FROM workspace_snapshots
             ORDER BY created_at DESC
             LIMIT ?1",
            )
            .map_err(|e| format!("Prepare failed: {e}"))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let sid: String = row.get(0)?;
                let ts: String = row.get(1)?;
                let label: String = row.get(2)?;
                let is_auto: i32 = row.get(3)?;
                let is_restored: i32 = row.get(4)?;
                let apps_json: String = row.get(5)?;
                let terms_json: String = row.get(6)?;
                let plans_json: String = row.get(7)?;

                let apps: Vec<serde_json::Value> =
                    serde_json::from_str(&apps_json).unwrap_or_default();
                let terms: Vec<serde_json::Value> =
                    serde_json::from_str(&terms_json).unwrap_or_default();
                let plans: Vec<serde_json::Value> =
                    serde_json::from_str(&plans_json).unwrap_or_default();

                let created_at = DateTime::parse_from_rfc3339(&ts)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);

                Ok(SnapshotSummary {
                    snapshot_id: Uuid::parse_str(&sid).unwrap_or_default(),
                    created_at,
                    label,
                    is_auto: is_auto != 0,
                    app_count: apps.len(),
                    terminal_count: terms.len(),
                    plan_count: plans.len(),
                    is_restored: is_restored != 0,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Row parse: {e}"))
    }

    /// Mark a snapshot as having been restored.
    pub fn mark_restored(&self, snapshot_id: &Uuid) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        conn.execute(
            "UPDATE workspace_snapshots SET is_restored = 1 WHERE snapshot_id = ?1",
            params![snapshot_id.to_string()],
        )
        .map_err(|e| format!("Mark restored failed: {e}"))?;
        Ok(())
    }

    /// Delete a snapshot.
    pub fn delete(&self, snapshot_id: &Uuid) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;
        let deleted = conn
            .execute(
                "DELETE FROM workspace_snapshots WHERE snapshot_id = ?1",
                params![snapshot_id.to_string()],
            )
            .map_err(|e| format!("Delete failed: {e}"))?;
        Ok(deleted > 0)
    }

    /// Expire old snapshots (keep last 48 hours or 20 snapshots, whichever is more).
    pub fn expire(&self) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {e}"))?;

        let cutoff = (Utc::now() - Duration::hours(48)).to_rfc3339();

        // Count total.
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM workspace_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        // Keep at least 20 snapshots regardless of age.
        let deleted = if total > 20 {
            conn.execute(
                "DELETE FROM workspace_snapshots
                 WHERE created_at < ?1
                 AND snapshot_id NOT IN (
                     SELECT snapshot_id FROM workspace_snapshots
                     ORDER BY created_at DESC LIMIT 20
                 )",
                params![cutoff],
            )
            .map_err(|e| format!("Expire failed: {e}"))?
        } else {
            0
        };

        if deleted > 0 {
            info!("Snapshot store: expired {deleted} old snapshots");
        }

        Ok(deleted)
    }
}

fn parse_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceSnapshot> {
    let sid: String = row.get(0)?;
    let ts: String = row.get(1)?;
    let label: String = row.get(2)?;
    let is_auto: i32 = row.get(3)?;
    let env_checksum: String = row.get(4)?;
    let active_project: Option<String> = row.get(5)?;
    let context_summary: Option<String> = row.get(6)?;

    let workspaces_json: String = row.get(7)?;
    let applications_json: String = row.get(8)?;
    let windows_json: String = row.get(9)?;
    let terminals_json: String = row.get(10)?;
    let browser_urls_json: String = row.get(11)?;
    let plans_json: String = row.get(12)?;
    let actions_json: String = row.get(13)?;
    let convos_json: String = row.get(14)?;

    let created_at = DateTime::parse_from_rfc3339(&ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    Ok(WorkspaceSnapshot {
        snapshot_id: Uuid::parse_str(&sid).unwrap_or_default(),
        created_at,
        label,
        workspaces: serde_json::from_str(&workspaces_json).unwrap_or_default(),
        applications: serde_json::from_str(&applications_json).unwrap_or_default(),
        windows: serde_json::from_str(&windows_json).unwrap_or_default(),
        terminals: serde_json::from_str(&terminals_json).unwrap_or_default(),
        browser_urls: serde_json::from_str(&browser_urls_json).unwrap_or_default(),
        orchestration_plans: serde_json::from_str(&plans_json).unwrap_or_default(),
        recent_actions: serde_json::from_str(&actions_json).unwrap_or_default(),
        ai_conversations: serde_json::from_str(&convos_json).unwrap_or_default(),
        active_project,
        context_summary,
        is_auto: is_auto != 0,
        env_checksum,
    })
}
