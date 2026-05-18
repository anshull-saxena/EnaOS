use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::actions::types::ActionType;

/// Top-level execution plan — a structured multi-step intent.
/// LLMs generate plans; enad executes them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub nodes: Vec<PlanNode>,
    pub edges: Vec<PlanEdge>,
    pub status: PlanStatus,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A single step within an execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanNode {
    pub id: Uuid,
    pub label: String,
    pub action: ActionType,
    pub status: NodeStatus,
    pub retry_count: u32,
    pub max_retries: u32,
    pub error: Option<String>,
    pub result: Option<String>,
    pub rollback_action: Option<ActionType>,
    pub requires_approval: bool,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Dependency edge between two plan nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub condition: EdgeCondition,
}

/// When a downstream node should execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeCondition {
    /// Execute only if upstream succeeded.
    Success,
    /// Execute even if upstream failed (best-effort).
    Always,
    /// Execute only if upstream failed.
    OnFailure,
}

/// Lifecycle status of a plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlanStatus {
    /// Awaiting user approval before execution.
    PendingApproval,
    /// Approved and queued for execution.
    Approved,
    /// Nodes are actively executing.
    Running,
    /// All nodes completed successfully.
    Completed,
    /// One or more nodes failed (partial or full).
    Failed,
    /// User cancelled the plan.
    Cancelled,
    /// Rolling back completed nodes.
    RollingBack,
    /// All rollbacks completed.
    RolledBack,
}

/// Status of a single plan node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

impl ExecutionPlan {
    pub fn new(title: &str, description: &str, nodes: Vec<PlanNode>, edges: Vec<PlanEdge>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.to_string(),
            description: description.to_string(),
            nodes,
            edges,
            status: PlanStatus::PendingApproval,
            created_at: Utc::now(),
            approved_at: None,
            completed_at: None,
        }
    }

    pub fn requires_approval(&self) -> bool {
        self.nodes.iter().any(|n| n.requires_approval)
    }

    pub fn find_node(&self, node_id: &Uuid) -> Option<&PlanNode> {
        self.nodes.iter().find(|n| n.id == *node_id)
    }

    pub fn find_node_mut(&mut self, node_id: &Uuid) -> Option<&mut PlanNode> {
        self.nodes.iter_mut().find(|n| n.id == *node_id)
    }

    pub fn completed_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Completed)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Failed)
            .count()
    }

    pub fn progress(&self) -> (usize, usize) {
        let done = self.nodes.iter()
            .filter(|n| matches!(n.status, NodeStatus::Completed | NodeStatus::Failed | NodeStatus::Skipped | NodeStatus::Cancelled))
            .count();
        (done, self.nodes.len())
    }
}

impl PlanNode {
    pub fn new(label: &str, action: ActionType) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.to_string(),
            action,
            status: NodeStatus::Pending,
            retry_count: 0,
            max_retries: 2,
            error: None,
            result: None,
            rollback_action: None,
            requires_approval: false,
            started_at: None,
            completed_at: None,
        }
    }

    pub fn with_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    pub fn with_rollback(mut self, action: ActionType) -> Self {
        self.rollback_action = Some(action);
        self
    }

    pub fn requires_approval(mut self) -> Self {
        self.requires_approval = true;
        self
    }

    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }
}
