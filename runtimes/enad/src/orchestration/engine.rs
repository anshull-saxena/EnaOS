use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{info, warn};
use uuid::Uuid;

use crate::actions::executor::ActionExecutor;
use crate::actions::types::{ActionRequest, ActionType};
use crate::bus::EventBus;
use crate::orchestration::types::*;
use crate::types::events::{EventKind, EventPayload, SystemEvent};

/// Shared mutable state for the orchestration engine.
struct EngineState {
    active_plans: HashMap<Uuid, PlanHandle>,
    pending_approval: Vec<ExecutionPlan>,
}

struct PlanHandle {
    plan: ExecutionPlan,
    cancel_notify: Notify,
}

/// The execution engine runs plans by stepping through nodes
/// in dependency order, handling retries, rollbacks, and cancellation.
pub struct OrchestrationEngine {
    bus: Arc<EventBus>,
    action_executor: Arc<ActionExecutor>,
    state: Arc<Mutex<EngineState>>,
}

impl OrchestrationEngine {
    pub fn new(bus: Arc<EventBus>, action_executor: Arc<ActionExecutor>) -> Self {
        Self {
            bus,
            action_executor,
            state: Arc::new(Mutex::new(EngineState {
                active_plans: HashMap::new(),
                pending_approval: Vec::new(),
            })),
        }
    }

    /// Submit a plan for execution. Returns the plan ID.
    pub async fn submit_plan(&self, mut plan: ExecutionPlan) -> Uuid {
        let plan_id = plan.id;
        let mut state = self.state.lock().await;

        if plan.requires_approval() {
            plan.status = PlanStatus::PendingApproval;
            state.pending_approval.push(plan);
            self.emit_event(
                plan_id,
                PlanStatus::PendingApproval,
                "Plan requires approval",
            );
        } else {
            plan.status = PlanStatus::Approved;
            self.emit_event(plan_id, PlanStatus::Approved, "Plan auto-approved");
            let handle = PlanHandle {
                plan,
                cancel_notify: Notify::new(),
            };
            state.active_plans.insert(plan_id, handle);
        }

        plan_id
    }

    /// Approve a pending plan and begin execution in background.
    pub async fn approve_plan(&self, plan_id: Uuid) -> Result<(), String> {
        let plan = {
            let mut state = self.state.lock().await;
            let idx = state.pending_approval.iter().position(|p| p.id == plan_id);
            match idx {
                Some(i) => state.pending_approval.remove(i),
                None => return Err(format!("No pending plan with id {plan_id}")),
            }
        };

        self.emit_event(plan_id, PlanStatus::Approved, "Plan approved by user");

        let handle = PlanHandle {
            plan,
            cancel_notify: Notify::new(),
        };
        {
            let mut state = self.state.lock().await;
            state.active_plans.insert(plan_id, handle);
        }

        let bus = self.bus.clone();
        let executor = self.action_executor.clone();
        let state_arc = self.state.clone();

        tokio::spawn(async move {
            OrchestrationEngine::execute_plan(plan_id, bus, executor, state_arc).await;
        });

        Ok(())
    }

    /// Reject a pending plan.
    pub async fn reject_plan(&self, plan_id: Uuid) -> Result<(), String> {
        let mut state = self.state.lock().await;
        let idx = state.pending_approval.iter().position(|p| p.id == plan_id);
        match idx {
            Some(i) => {
                state.pending_approval.remove(i);
                self.emit_event(plan_id, PlanStatus::Cancelled, "Plan rejected by user");
                Ok(())
            }
            None => Err(format!("No pending plan with id {plan_id}")),
        }
    }

    /// Cancel a running plan. Current node finishes, then stops.
    pub async fn cancel_plan(&self, plan_id: Uuid) -> Result<(), String> {
        let state = self.state.lock().await;
        if let Some(handle) = state.active_plans.get(&plan_id) {
            self.emit_event(plan_id, PlanStatus::Cancelled, "Plan cancelled by user");
            handle.cancel_notify.notify_one();
            Ok(())
        } else {
            Err(format!("No active plan with id {plan_id}"))
        }
    }

    /// List all plans.
    pub async fn list_plans(&self) -> Vec<ExecutionPlan> {
        let state = self.state.lock().await;
        let mut plans: Vec<ExecutionPlan> = state
            .active_plans
            .values()
            .map(|h| h.plan.clone())
            .collect();
        plans.extend(state.pending_approval.clone());
        plans
    }

    /// Get a single plan by ID.
    pub async fn get_plan(&self, plan_id: Uuid) -> Option<ExecutionPlan> {
        let state = self.state.lock().await;
        if let Some(handle) = state.active_plans.get(&plan_id) {
            return Some(handle.plan.clone());
        }
        state
            .pending_approval
            .iter()
            .find(|p| p.id == plan_id)
            .cloned()
    }

    // ── Execution logic (static, runs in spawned task) ────────

    async fn execute_plan(
        plan_id: Uuid,
        bus: Arc<EventBus>,
        executor: Arc<ActionExecutor>,
        state: Arc<Mutex<EngineState>>,
    ) {
        let plan_clone = {
            let s = state.lock().await;
            s.active_plans.get(&plan_id).map(|h| h.plan.clone())
        };

        let mut plan = match plan_clone {
            Some(p) => p,
            None => return,
        };

        plan.status = PlanStatus::Running;
        Self::save_plan_state(&state, &plan).await;
        Self::emit_static(&bus, plan_id, PlanStatus::Running, "Plan execution started");

        let order = topological_sort(&plan);
        let mut rollback_nodes: Vec<(Uuid, ActionType)> = Vec::new();

        for node_id in &order {
            // Check cancellation.
            if Self::is_cancelled(&state, plan_id).await {
                info!("Plan {plan_id}: cancelled");
                break;
            }

            // Find node index rather than holding a mutable reference.
            let node_idx = match plan.nodes.iter().position(|n| n.id == *node_id) {
                Some(i) => i,
                None => continue,
            };

            if !dependencies_met(node_id, &plan) {
                warn!("Plan {plan_id}: deps not met for node {node_id}");
                plan.nodes[node_idx].status = NodeStatus::Skipped;
                Self::emit_node_static(&bus, plan_id, &plan.nodes[node_idx]);
                continue;
            }

            // Mark as running.
            plan.nodes[node_idx].status = NodeStatus::Running;
            plan.nodes[node_idx].started_at = Some(chrono::Utc::now());
            Self::emit_node_static(&bus, plan_id, &plan.nodes[node_idx]);

            let action = plan.nodes[node_idx].action.clone();
            let max_retries = plan.nodes[node_idx].max_retries;

            let result =
                execute_with_retries(&executor, plan_id, *node_id, &action, max_retries).await;

            match result {
                Ok(output) => {
                    plan.nodes[node_idx].status = NodeStatus::Completed;
                    plan.nodes[node_idx].result = Some(output);
                    plan.nodes[node_idx].completed_at = Some(chrono::Utc::now());
                    Self::emit_node_static(&bus, plan_id, &plan.nodes[node_idx]);

                    if let Some(ref rb) = plan.nodes[node_idx].rollback_action.clone() {
                        rollback_nodes.push((*node_id, rb.clone()));
                    }

                    Self::save_plan_state(&state, &plan).await;
                }
                Err(err) => {
                    plan.nodes[node_idx].status = NodeStatus::Failed;
                    plan.nodes[node_idx].error = Some(err.clone());
                    plan.nodes[node_idx].completed_at = Some(chrono::Utc::now());
                    Self::emit_node_static(&bus, plan_id, &plan.nodes[node_idx]);
                    warn!("Plan {plan_id}: node {node_id} failed — {err}");

                    execute_rollbacks(&executor, &bus, plan_id, &rollback_nodes).await;
                    plan.status = PlanStatus::Failed;
                    Self::emit_static(
                        &bus,
                        plan_id,
                        PlanStatus::Failed,
                        &format!("Node failed: {err}"),
                    );
                    Self::save_plan_state(&state, &plan).await;
                    return;
                }
            }
        }

        let all_done = plan
            .nodes
            .iter()
            .all(|n| matches!(n.status, NodeStatus::Completed | NodeStatus::Skipped));

        if all_done {
            plan.status = PlanStatus::Completed;
            plan.completed_at = Some(chrono::Utc::now());
            Self::emit_static(&bus, plan_id, PlanStatus::Completed, "Plan completed");
        } else {
            plan.status = PlanStatus::Failed;
            plan.completed_at = Some(chrono::Utc::now());
            Self::emit_static(
                &bus,
                plan_id,
                PlanStatus::Failed,
                "Plan completed with failures",
            );
        }

        Self::save_plan_state(&state, &plan).await;
    }

    async fn is_cancelled(state: &Arc<Mutex<EngineState>>, plan_id: Uuid) -> bool {
        let s = state.lock().await;
        if let Some(handle) = s.active_plans.get(&plan_id) {
            tokio::time::timeout(
                std::time::Duration::from_secs(0),
                handle.cancel_notify.notified(),
            )
            .await
            .is_ok()
        } else {
            false
        }
    }

    async fn save_plan_state(state: &Arc<Mutex<EngineState>>, plan: &ExecutionPlan) {
        let mut s = state.lock().await;
        if let Some(handle) = s.active_plans.get_mut(&plan.id) {
            handle.plan = plan.clone();
        }
    }

    // ── Event emission ──

    fn emit_event(&self, plan_id: Uuid, status: PlanStatus, message: &str) {
        Self::emit_static(&self.bus, plan_id, status, message);
    }

    fn emit_static(bus: &EventBus, plan_id: Uuid, status: PlanStatus, message: &str) {
        bus.publish(SystemEvent::new(
            "orchestration",
            EventKind::System,
            EventPayload::OrchestrationPlanEvent {
                plan_id,
                status: format!("{status:?}"),
                message: message.to_string(),
            },
        ));
    }

    fn emit_node_static(bus: &EventBus, plan_id: Uuid, node: &PlanNode) {
        bus.publish(SystemEvent::new(
            "orchestration",
            EventKind::System,
            EventPayload::OrchestrationNodeEvent {
                plan_id,
                node_id: node.id,
                status: format!("{:?}", node.status),
                label: node.label.clone(),
                error: node.error.clone(),
                result: node.result.clone(),
            },
        ));
    }
}

// ── Free functions ────────────────────────────────────────────

async fn execute_with_retries(
    executor: &ActionExecutor,
    plan_id: Uuid,
    node_id: Uuid,
    action: &ActionType,
    max_retries: u32,
) -> Result<String, String> {
    let mut attempts = 0;
    let mut last_error;

    loop {
        let permission = ActionRequest::default_permission(action);
        let request = ActionRequest::new(action.clone(), permission);

        match executor.execute(request).await {
            Ok(_) => return Ok(String::new()),
            Err(e) => {
                attempts += 1;
                last_error = e;
                if attempts <= max_retries {
                    info!("Plan {plan_id}: retry node {node_id} ({attempts}/{max_retries})");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                } else {
                    return Err(last_error);
                }
            }
        }
    }
}

async fn execute_rollbacks(
    executor: &ActionExecutor,
    bus: &EventBus,
    plan_id: Uuid,
    completed: &[(Uuid, ActionType)],
) {
    if completed.is_empty() {
        return;
    }

    OrchestrationEngine::emit_static(
        bus,
        plan_id,
        PlanStatus::RollingBack,
        "Rolling back completed actions",
    );

    for (node_id, rollback_action) in completed.iter().rev() {
        info!("Plan {plan_id}: rollback node {node_id}");
        let permission = ActionRequest::default_permission(rollback_action);
        let request = ActionRequest::new(rollback_action.clone(), permission);
        let _ = executor.execute(request).await;
    }

    OrchestrationEngine::emit_static(bus, plan_id, PlanStatus::RolledBack, "Rollback completed");
}

fn dependencies_met(node_id: &Uuid, plan: &ExecutionPlan) -> bool {
    let deps: Vec<&Uuid> = plan
        .edges
        .iter()
        .filter(|e| e.to == *node_id)
        .map(|e| &e.from)
        .collect();
    if deps.is_empty() {
        return true;
    }
    deps.iter().all(|dep_id| {
        plan.nodes
            .iter()
            .any(|n| n.id == **dep_id && n.status == NodeStatus::Completed)
    })
}

fn topological_sort(plan: &ExecutionPlan) -> Vec<Uuid> {
    let mut sorted = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let node_ids: Vec<Uuid> = plan.nodes.iter().map(|n| n.id).collect();

    fn visit(
        n: Uuid,
        sorted: &mut Vec<Uuid>,
        visited: &mut std::collections::HashSet<Uuid>,
        edges: &[PlanEdge],
    ) {
        if visited.contains(&n) {
            return;
        }
        visited.insert(n);
        for e in edges.iter().filter(|e| e.to == n) {
            visit(e.from, sorted, visited, edges);
        }
        sorted.push(n);
    }

    for id in &node_ids {
        visit(*id, &mut sorted, &mut visited, &plan.edges);
    }
    sorted
}
