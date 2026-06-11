/// First-Run State management for EnaOS.
///
/// Detects fresh installs, manages onboarding progress, and seeds
/// demo data so new users can experience continuity immediately.
///
/// Design:
/// - Uses a marker file at `$TEMP/ena-first-run-completed` to persist status
/// - If no marker and no snapshot DB → fresh install
/// - Seeds demo snapshot + example orchestration plan for demo purposes
/// - Demo data auto-expires after first real snapshot is taken
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

/// First-run status returned to the bar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRunStatus {
    /// Whether this is the first-ever launch (no marker file exists).
    pub is_first_launch: bool,
    /// Whether onboarding has been completed (welcome overlay shown + dismissed).
    pub onboarding_completed: bool,
    /// Whether demo data has been seeded.
    pub demo_seeded: bool,
    /// Suggested first commands for the user to try.
    pub suggested_commands: Vec<String>,
    /// Timestamp of first launch (ISO 8601).
    pub first_launch_at: Option<String>,
}

impl Default for FirstRunStatus {
    fn default() -> Self {
        Self {
            is_first_launch: true,
            onboarding_completed: false,
            demo_seeded: false,
            suggested_commands: vec![
                "open browser".to_string(),
                "check system status".to_string(),
                "create a snapshot".to_string(),
            ],
            first_launch_at: None,
        }
    }
}

/// Manages first-run state and demo data lifecycle.
pub struct FirstRunManager {
    /// Path to the marker file that indicates first-run is complete.
    marker_path: PathBuf,
    /// In-memory status (persisted to marker on write).
    status: Mutex<FirstRunStatus>,
    /// Whether demo data has been seeded in this session.
    demo_seeded: Mutex<bool>,
}

impl FirstRunManager {
    /// Create a new FirstRunManager.
    ///
    /// `data_dir` is typically `$TEMP` or a persistent config path.
    /// `has_db` indicates whether the snapshot/memory databases already exist.
    pub fn new(data_dir: &str, has_db: bool) -> Self {
        let marker_path = PathBuf::from(data_dir).join(".ena-first-run-completed");
        let marker_exists = marker_path.exists();

        let status = FirstRunStatus {
            is_first_launch: !marker_exists && !has_db,
            onboarding_completed: marker_exists,
            demo_seeded: false,
            suggested_commands: vec![
                "open browser".to_string(),
                "check system status".to_string(),
                "create a snapshot".to_string(),
            ],
            first_launch_at: None,
        };

        if status.is_first_launch {
            info!("First launch detected — preparing demo data");
        } else if !marker_exists && has_db {
            info!("Existing database found — returning user, no demo needed");
        }

        Self {
            marker_path,
            status: Mutex::new(status),
            demo_seeded: Mutex::new(false),
        }
    }

    /// Check if this is a fresh first launch.
    pub fn is_first_launch(&self) -> bool {
        self.status.lock().unwrap().is_first_launch
    }

    /// Get the current first-run status.
    pub fn get_status(&self) -> FirstRunStatus {
        self.status.lock().unwrap().clone()
    }

    /// Mark onboarding as completed and persist the marker file.
    pub fn complete_onboarding(&self) {
        let mut status = self.status.lock().unwrap();
        status.onboarding_completed = true;
        status.is_first_launch = false;
        drop(status);

        // Persist marker file.
        if let Some(parent) = self.marker_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&self.marker_path, Utc::now().to_rfc3339()) {
            Ok(_) => info!("First-run marker written to {:?}", self.marker_path),
            Err(e) => warn!("Failed to write first-run marker: {e}"),
        }
    }

    /// Mark demo data as seeded.
    pub fn mark_demo_seeded(&self) {
        *self.demo_seeded.lock().unwrap() = true;
        let mut status = self.status.lock().unwrap();
        status.demo_seeded = true;
    }

    /// Check if demo data has been seeded.
    pub fn is_demo_seeded(&self) -> bool {
        *self.demo_seeded.lock().unwrap()
    }

    /// Check if a demo marker should be cleaned up.
    /// Returns true if real snapshots exist (demo should be removed).
    pub fn should_cleanup_demo(&self, real_snapshot_count: usize) -> bool {
        real_snapshot_count > 1 // Demo snapshot + at least one real one
    }
}

// ── Demo data seeding ─────────────────────────────────────────

/// A seeded demo snapshot for fresh installs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoSnapshot {
    pub id: Uuid,
    pub label: String,
    pub demo: bool,
    pub created_at: DateTime<Utc>,
    pub window_count: i64,
    pub terminal_count: i64,
    pub active_project: String,
    pub summary: String,
}

/// Get the demo snapshot to seed for fresh installs.
pub fn create_demo_snapshot() -> DemoSnapshot {
    DemoSnapshot {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001")
            .unwrap_or_else(|_| Uuid::new_v4()),
        label: "EnaOS Development".to_string(),
        demo: true,
        created_at: Utc::now() - Duration::hours(2),
        window_count: 3,
        terminal_count: 2,
        active_project: "EnaOS".to_string(),
        summary: "Code editor, terminal, and browser open with EnaOS project".to_string(),
    }
}

/// Demo orchestration plan nodes for the seeded example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoPlanNode {
    pub id: String,
    pub label: String,
    pub status: String, // Completed, Running, etc.
}

/// Demo orchestration plan for fresh installs.
pub fn create_demo_orchestration_plan() -> (Uuid, Vec<DemoPlanNode>) {
    let plan_id =
        Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap_or_else(|_| Uuid::new_v4());
    let nodes = vec![
        DemoPlanNode {
            id: "node-1".to_string(),
            label: "Open VS Code".to_string(),
            status: "Completed".to_string(),
        },
        DemoPlanNode {
            id: "node-2".to_string(),
            label: "Start dev server".to_string(),
            status: "Completed".to_string(),
        },
        DemoPlanNode {
            id: "node-3".to_string(),
            label: "Open browser to docs".to_string(),
            status: "Completed".to_string(),
        },
    ];
    (plan_id, nodes)
}
