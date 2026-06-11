use uuid::Uuid;

use crate::actions::types::ActionType;
use crate::orchestration::types::*;
use crate::restore::types::*;
use crate::snapshot::types::*;

/// Converts a workspace snapshot into a structured restoration execution plan.
pub struct RestorePlanner;

impl RestorePlanner {
    /// Generate a restoration preview from a snapshot (no side effects).
    pub fn preview(&self, snapshot: &WorkspaceSnapshot) -> RestorePreview {
        let mut actions = Vec::new();

        for ws in &snapshot.workspaces {
            if ws.is_focused {
                actions.push(RestoreActionPreview {
                    label: format!("Switch to workspace: {}", ws.name),
                    action_type: "switch_workspace".into(),
                    target: ws.name.clone(),
                    requires_approval: false,
                });
            }
        }

        for app in &snapshot.applications {
            actions.push(RestoreActionPreview {
                label: format!("Open: {}", app.name),
                action_type: "open_app".into(),
                target: app.name.clone(),
                requires_approval: false,
            });
            if app.is_focused {
                actions.push(RestoreActionPreview {
                    label: format!("Focus: {} — {}", app.name, app.title),
                    action_type: "focus_window".into(),
                    target: app.name.clone(),
                    requires_approval: false,
                });
            }
        }

        for term in &snapshot.terminals {
            actions.push(RestoreActionPreview {
                label: format!(
                    "Restore terminal: {} ({})",
                    term.app,
                    term.working_directory.as_deref().unwrap_or("default")
                ),
                action_type: "launch_command".into(),
                target: term.app.clone(),
                requires_approval: true,
            });
        }

        for tab in &snapshot.browser_urls {
            actions.push(RestoreActionPreview {
                label: format!("Open URL: {}", tab.url),
                action_type: "open_url".into(),
                target: tab.url.clone(),
                requires_approval: false,
            });
        }

        RestorePreview {
            snapshot_id: snapshot.snapshot_id,
            snapshot_label: snapshot.label.clone(),
            snapshot_taken_at: snapshot.created_at.to_rfc3339(),
            action_count: actions.len() as u32,
            actions,
        }
    }

    /// Convert a snapshot into an ExecutionPlan for the orchestration engine.
    /// Respects the user's selection filters.
    pub fn plan_restoration(
        &self,
        snapshot: &WorkspaceSnapshot,
        selections: Option<&RestoreSelections>,
    ) -> ExecutionPlan {
        let use_selections = selections.unwrap_or(&RestoreSelections {
            applications: true,
            workspaces: true,
            terminals: true,
            browser_urls: true,
            orchestration_context: true,
        });

        let mut nodes: Vec<PlanNode> = Vec::new();
        let mut edges: Vec<PlanEdge> = Vec::new();
        let mut prev_node_id: Option<Uuid> = None;

        // Phase 1: Workspace switch
        if use_selections.workspaces {
            for ws in &snapshot.workspaces {
                if ws.is_focused {
                    let node = PlanNode::new(
                        &format!("Switch to workspace: {}", ws.name),
                        ActionType::SwitchWorkspace {
                            workspace: ws.name.clone(),
                        },
                    );
                    let node_id = node.id;
                    nodes.push(node);

                    if let Some(prev) = prev_node_id {
                        edges.push(PlanEdge {
                            from: prev,
                            to: node_id,
                            condition: EdgeCondition::Success,
                        });
                    }
                    prev_node_id = Some(node_id);
                }
            }
        }

        // Phase 2: Open applications (in parallel)
        if use_selections.applications {
            let mut app_node_ids: Vec<Uuid> = Vec::new();

            for app in &snapshot.applications {
                let node = PlanNode::new(
                    &format!("Open: {}", app.name),
                    ActionType::OpenApp {
                        app: app.name.clone(),
                    },
                );
                let node_id = node.id;
                nodes.push(node);
                app_node_ids.push(node_id);

                if let Some(prev) = prev_node_id {
                    edges.push(PlanEdge {
                        from: prev,
                        to: node_id,
                        condition: EdgeCondition::Success,
                    });
                }

                // Focus window if it was focused.
                if app.is_focused {
                    let focus_node = PlanNode::new(
                        &format!("Focus: {}", app.name),
                        ActionType::FocusWindow {
                            app: Some(app.name.clone()),
                            title: Some(app.title.clone()),
                        },
                    );
                    let focus_id = focus_node.id;
                    nodes.push(focus_node);
                    edges.push(PlanEdge {
                        from: node_id,
                        to: focus_id,
                        condition: EdgeCondition::Success,
                    });
                    app_node_ids.push(focus_id);
                }
            }

            // Chain apps sequentially (first depends on prev, rest depend on first).
            // Actually, make them sequential for reliability.
            for i in 1..app_node_ids.len() {
                // Only add edge if not already added.
                let has_edge = edges.iter().any(|e| e.to == app_node_ids[i]);
                if !has_edge {
                    edges.push(PlanEdge {
                        from: app_node_ids[i - 1],
                        to: app_node_ids[i],
                        condition: EdgeCondition::Success,
                    });
                }
            }

            if !app_node_ids.is_empty() {
                prev_node_id = Some(*app_node_ids.last().unwrap());
            }
        }

        // Phase 3: Terminal sessions
        if use_selections.terminals {
            for term in &snapshot.terminals {
                let command = term.command.clone().unwrap_or_default();
                let mut label = format!("Open terminal: {}", term.app);
                if let Some(ref cwd) = term.working_directory {
                    label.push_str(&format!(" in {}", cwd));
                }

                let node = PlanNode::new(
                    &label,
                    ActionType::LaunchCommand {
                        command: if command.is_empty() {
                            term.app.clone()
                        } else {
                            command
                        },
                        args: Vec::new(),
                    },
                )
                .requires_approval();

                let node_id = node.id;
                nodes.push(node);

                if let Some(prev) = prev_node_id {
                    edges.push(PlanEdge {
                        from: prev,
                        to: node_id,
                        condition: EdgeCondition::Success,
                    });
                }
                prev_node_id = Some(node_id);
            }
        }

        // Phase 4: Browser URLs
        if use_selections.browser_urls {
            for tab in &snapshot.browser_urls {
                let label = format!("Open: {}", tab.url);
                let node = PlanNode::new(
                    &label,
                    ActionType::OpenUrl {
                        url: tab.url.clone(),
                    },
                );
                let node_id = node.id;
                nodes.push(node);

                if let Some(prev) = prev_node_id {
                    edges.push(PlanEdge {
                        from: prev,
                        to: node_id,
                        condition: EdgeCondition::Success,
                    });
                }
                prev_node_id = Some(node_id);
            }
        }

        // Determine if plan needs approval (terminal commands require it).
        let title = format!("Restore: {}", snapshot.label);
        let mut desc = format!("Restoration of {} (", snapshot.label);
        let parts: Vec<&str> = [
            if use_selections.applications {
                Some("apps")
            } else {
                None
            },
            if use_selections.workspaces {
                Some("workspaces")
            } else {
                None
            },
            if use_selections.terminals {
                Some("terminals")
            } else {
                None
            },
            if use_selections.browser_urls {
                Some("browser")
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect();
        desc.push_str(&parts.join(", "));
        desc.push(')');

        let mut plan = ExecutionPlan::new(&title, &desc, nodes, edges);

        // Mark terminal/launch nodes as requiring approval.
        let has_terminal = plan
            .nodes
            .iter()
            .any(|n| matches!(&n.action, ActionType::LaunchCommand { .. }));

        if has_terminal {
            // The plan requires approval because it contains terminal commands.
            // This triggers the approval flow in the orchestration engine.
        }

        // Pre-set requires_approval on terminal nodes.
        for node in &mut plan.nodes {
            if matches!(&node.action, ActionType::LaunchCommand { .. }) {
                node.requires_approval = true;
            }
        }

        plan
    }
}
