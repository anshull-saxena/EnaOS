/// Contextual Command Intelligence engine.
///
/// Provides context-aware command suggestions based on:
/// - Current desktop state (focused app, workspace, system info)
/// - Recent memory (intents, actions, queries)
/// - Active orchestrations and snapshots
/// - Intent classification from user query
///
/// Design principles:
/// - Sparse, high-confidence suggestions (not noisy lists)
/// - Intent-driven ranking (continue -> workflows, restore -> snapshots)
/// - Conservative context augmentation (never invents workflows)
/// - Sub-10ms latency (cached aggregation, no live queries)

pub mod aggregator;
pub mod classifier;
pub mod ranker;
pub mod resolver;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use self::aggregator::{ActivePlan, ContextAggregator, RecentSnapshot};
use self::classifier::IntentClassifier;
use self::ranker::CommandRanker;
use self::resolver::CommandResolver;

/// A single command suggestion returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSuggestion {
    /// Unique ID for this suggestion.
    pub id: String,
    /// Primary label shown to user.
    pub label: String,
    /// Secondary subtitle (e.g., "Interrupted 3m ago").
    pub subtitle: String,
    /// Icon identifier for rendering.
    pub icon: String,
    /// Action type to execute on selection.
    pub action: String,
    /// Parameters for the action.
    pub action_params: serde_json::Value,
    /// Confidence score (0.0-1.0).
    pub score: f64,
    /// Source of this suggestion.
    pub source: String,
}

/// The ContextEngine aggregates desktop state and resolves queries
/// into ranked command suggestions.
pub struct ContextEngine {
    aggregator: Arc<ContextAggregator>,
    classifier: IntentClassifier,
    resolver: CommandResolver,
    ranker: CommandRanker,
}

impl ContextEngine {
    pub fn new() -> Self {
        let aggregator = Arc::new(ContextAggregator::new());

        Self {
            aggregator: aggregator.clone(),
            classifier: IntentClassifier::new(),
            resolver: CommandResolver::new(aggregator),
            ranker: CommandRanker::new(),
        }
    }

    /// Resolve a user query into ranked command suggestions.
    ///
    /// Returns an empty Vec if confidence is below threshold
    /// or if the query is too short to classify.
    pub fn resolve(&self, query: &str) -> Vec<CommandSuggestion> {
        // Suppress: query too short.
        if query.trim().len() < 2 {
            return Vec::new();
        }

        // Classify intent.
        let intent = self.classifier.classify(query);

        // Resolve candidates from all sources.
        let candidates = self.resolver.resolve(query, &intent);

        // Rank with intent bias.
        let ranked = self.ranker.rank(candidates, &intent);

        // Apply confidence threshold.
        self.ranker.filter_confidence(ranked)
    }

    /// Get the current aggregated context snapshot.
    pub fn context_snapshot(&self) -> serde_json::Value {
        self.aggregator.snapshot()
    }

    /// Update the aggregator with a new event.
    /// Called by the event subscription loop.
    pub fn on_event(&self, kind: &str, payload: &serde_json::Value) {
        self.aggregator.update(kind, payload);
    }

    /// Refresh deep state from stores.
    /// Called periodically by an external async task.
    pub fn refresh(
        &self,
        recent_intents: Vec<String>,
        recent_actions: Vec<String>,
        active_plans: Vec<ActivePlan>,
        recent_snapshots: Vec<RecentSnapshot>,
    ) {
        self.aggregator.refresh_from_stores(
            recent_intents,
            recent_actions,
            active_plans,
            recent_snapshots,
        );
    }
}
