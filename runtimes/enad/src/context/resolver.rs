/// CommandResolver — matches queries against context sources to produce
/// candidate command suggestions.
///
/// Five matchers, each producing candidates from a different source:
/// 1. ActionMatcher — maps query to available desktop actions
/// 2. WorkflowMatcher — surfaces recent/interrupted orchestrations
/// 3. ContextMatcher — app/workspace-aware suggestions
/// 4. SnapshotMatcher — suggests restorable sessions
/// 5. SemanticMatcher — fuzzy text match against recent memory
///
/// Each candidate has a base score (0.0-1.0) that the ranker
/// will adjust with intent bias and confidence thresholding.

use std::sync::Arc;

use uuid::Uuid;

use super::aggregator::{ContextAggregator, AggregatedContext};
use super::classifier::{ClassifiedIntent, IntentType};
use super::CommandSuggestion;

pub struct CommandResolver {
    aggregator: Arc<ContextAggregator>,
}

impl CommandResolver {
    pub fn new(aggregator: Arc<ContextAggregator>) -> Self {
        Self { aggregator }
    }

    /// Resolve a query into candidate suggestions from all sources.
    pub fn resolve(&self, query: &str, intent: &ClassifiedIntent) -> Vec<CommandSuggestion> {
        let ctx = self.aggregator.get_context();
        let lower = query.to_lowercase();
        let mut candidates = Vec::new();

        // 1. Workflow matcher — active/interrupted plans.
        candidates.extend(self.match_workflows(&lower, &ctx, intent));

        // 2. Snapshot matcher — restorable sessions.
        candidates.extend(self.match_snapshots(&lower, &ctx, intent));

        // 3. Action matcher — available desktop actions.
        candidates.extend(self.match_actions(&lower, &ctx, intent));

        // 4. Context matcher — workspace/app-aware suggestions.
        candidates.extend(self.match_context(&lower, &ctx, intent));

        // 5. Semantic matcher — fuzzy match against recent memory.
        candidates.extend(self.match_semantic(&lower, &ctx, intent));

        candidates
    }

    /// Match against active/interrupted orchestrations.
    fn match_workflows(
        &self,
        query: &str,
        ctx: &AggregatedContext,
        _intent: &ClassifiedIntent,
    ) -> Vec<CommandSuggestion> {
        let mut results = Vec::new();

        for plan in &ctx.active_plans {
            let title_lower = plan.title.to_lowercase();
            let similarity = fuzzy_score(query, &title_lower);

            // Only surface if there's some relevance or intent match.
            if similarity > 0.1
                || matches!(_intent.intent_type, IntentType::Continue | IntentType::Restore)
            {
                let subtitle = match plan.status.as_str() {
                    "PendingApproval" => "Requires approval".to_string(),
                    "Running" => "Currently running".to_string(),
                    _ => format!("Status: {}", plan.status),
                };

                results.push(CommandSuggestion {
                    id: Uuid::new_v4().to_string(),
                    label: format!("Continue: {}", plan.title),
                    subtitle,
                    icon: "workflow".to_string(),
                    action: "ResumePlan".to_string(),
                    action_params: serde_json::json!({ "plan_id": plan.id }),
                    score: similarity,
                    source: "workflow".to_string(),
                });
            }
        }

        results
    }

    /// Match against recent snapshots.
    fn match_snapshots(
        &self,
        query: &str,
        ctx: &AggregatedContext,
        _intent: &ClassifiedIntent,
    ) -> Vec<CommandSuggestion> {
        let mut results = Vec::new();

        for snap in &ctx.recent_snapshots {
            let label_lower = snap.label.to_lowercase();
            let similarity = fuzzy_score(query, &label_lower);

            if similarity > 0.1
                || matches!(_intent.intent_type, IntentType::Restore | IntentType::Continue)
            {
                results.push(CommandSuggestion {
                    id: Uuid::new_v4().to_string(),
                    label: format!("Restore: {}", snap.label),
                    subtitle: format!("Taken {}", format_timestamp(&snap.taken_at)),
                    icon: "snapshot".to_string(),
                    action: "PreviewRestore".to_string(),
                    action_params: serde_json::json!({ "snapshot_id": snap.id }),
                    score: similarity,
                    source: "snapshot".to_string(),
                });
            }
        }

        results
    }

    /// Match against available desktop actions.
    fn match_actions(
        &self,
        query: &str,
        ctx: &AggregatedContext,
        intent: &ClassifiedIntent,
    ) -> Vec<CommandSuggestion> {
        let mut results = Vec::new();

        // App launch suggestions.
        if matches!(intent.intent_type, IntentType::Open | IntentType::Generic) {
            // Suggest apps based on query match.
            let common_apps = [
                ("firefox", "Firefox", "open_app"),
                ("code", "VS Code", "open_app"),
                ("terminal", "Terminal", "open_app"),
                ("files", "Files", "open_app"),
                ("settings", "Settings", "open_app"),
                ("browser", "Browser", "open_app"),
            ];

            for (keyword, name, action_type) in common_apps {
                if query.contains(keyword) || query.is_empty() {
                    let sim = if query.is_empty() { 0.1 } else { fuzzy_score(query, keyword) };
                    if sim > 0.0 {
                        results.push(CommandSuggestion {
                            id: Uuid::new_v4().to_string(),
                            label: format!("Open {}", name),
                            subtitle: "Application".to_string(),
                            icon: "app".to_string(),
                            action: action_type.to_string(),
                            action_params: serde_json::json!({ "app": name }),
                            score: sim,
                            source: "action".to_string(),
                        });
                    }
                }
            }
        }

        // Workspace switch suggestions.
        if !ctx.desktop.workspace.is_empty()
            && matches!(intent.intent_type, IntentType::Switch | IntentType::Generic)
        {
            let ws = &ctx.desktop.workspace;
            if fuzzy_score(query, &ws.to_lowercase()) > 0.1 || query.contains("workspace") {
                results.push(CommandSuggestion {
                    id: Uuid::new_v4().to_string(),
                    label: format!("Switch to {}", ws),
                    subtitle: "Workspace".to_string(),
                    icon: "workspace".to_string(),
                    action: "switch_workspace".to_string(),
                    action_params: serde_json::json!({ "workspace": ws }),
                    score: 0.5,
                    source: "action".to_string(),
                });
            }
        }

        // Media control if something is playing.
        if !ctx.desktop.media_player.is_empty() && query.contains("media") || query.contains("music") || query.contains("play") {
            results.push(CommandSuggestion {
                id: Uuid::new_v4().to_string(),
                label: format!("Pause {}", ctx.desktop.media_player),
                subtitle: "Media control".to_string(),
                icon: "media".to_string(),
                action: "media_control".to_string(),
                action_params: serde_json::json!({ "action": "pause" }),
                score: 0.4,
                source: "action".to_string(),
            });
        }

        results
    }

    /// Match against current desktop context.
    fn match_context(
        &self,
        query: &str,
        ctx: &AggregatedContext,
        intent: &ClassifiedIntent,
    ) -> Vec<CommandSuggestion> {
        let mut results = Vec::new();

        // If focused app matches query, suggest app-specific actions.
        if !ctx.desktop.focused_app.is_empty() {
            let app_lower = ctx.desktop.focused_app.to_lowercase();
            if fuzzy_score(query, &app_lower) > 0.2 {
                results.push(CommandSuggestion {
                    id: Uuid::new_v4().to_string(),
                    label: format!("Focus {}", ctx.desktop.focused_app),
                    subtitle: if !ctx.desktop.focused_title.is_empty() {
                        ctx.desktop.focused_title.clone()
                    } else {
                        "Currently focused".to_string()
                    },
                    icon: "window".to_string(),
                    action: "focus_window".to_string(),
                    action_params: serde_json::json!({
                        "app": ctx.desktop.focused_app,
                    }),
                    score: 0.5,
                    source: "context".to_string(),
                });
            }
        }

        results
    }

    /// Fuzzy match against recent memory entries.
    fn match_semantic(
        &self,
        query: &str,
        ctx: &AggregatedContext,
        intent: &ClassifiedIntent,
    ) -> Vec<CommandSuggestion> {
        let mut results = Vec::new();

        for intent_text in &ctx.recent_intents {
            let sim = fuzzy_score(query, &intent_text.to_lowercase());
            if sim > 0.25 {
                results.push(CommandSuggestion {
                    id: Uuid::new_v4().to_string(),
                    label: format!("Repeat: {}", truncate(intent_text, 40)),
                    subtitle: "Recent query".to_string(),
                    icon: "memory".to_string(),
                    action: "RepeatQuery".to_string(),
                    action_params: serde_json::json!({ "query": intent_text }),
                    score: sim,
                    source: "semantic".to_string(),
                });
            }
        }

        // Deduplicate by label.
        results.dedup_by_key(|s| s.label.clone());
        results
    }
}

/// Simple fuzzy string similarity (0.0-1.0).
/// Checks if query tokens appear in target, with partial matching.
fn fuzzy_score(query: &str, target: &str) -> f64 {
    if query.is_empty() || target.is_empty() {
        return 0.0;
    }

    let query_tokens: Vec<&str> = query.split_whitespace().collect();
    let target_lower = target.to_lowercase();

    let mut matched = 0;
    let total = query_tokens.len();

    for token in &query_tokens {
        let token_lower = token.to_lowercase();
        if target_lower.contains(&token_lower) {
            matched += 1;
        } else {
            // Partial match: check if token is a prefix of any word in target.
            for word in target_lower.split_whitespace() {
                if word.starts_with(&token_lower) || token_lower.starts_with(word) {
                    matched += 1;
                    break;
                }
            }
        }
    }

    if total == 0 {
        return 0.0;
    }

    let base = matched as f64 / total as f64;

    // Bonus for exact match.
    if target_lower == query.to_lowercase() {
        return 1.0;
    }

    // Bonus for prefix match.
    if target_lower.starts_with(&query.to_lowercase()) {
        return (base + 0.3).min(1.0);
    }

    base
}

fn format_timestamp(rfc3339: &str) -> String {
    // Simple formatting: extract "X minutes ago" style.
    // For now, just return a shortened version.
    if rfc3339.len() > 19 {
        let date_part = &rfc3339[..19];
        format!("{}", date_part.replace('T', " "))
    } else {
        rfc3339.to_string()
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
