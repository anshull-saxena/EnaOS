use super::CommandSuggestion;
/// CommandRanker — scores and ranks command suggestions.
///
/// Scoring formula:
///   final_score = (base_similarity × 0.30)
///               + (recency_decay × 0.20)
///               + (context_relevance × 0.20)
///               + (frequency_boost × 0.10)
///               + (intent_weight × source_bias)
///
/// Confidence thresholding:
///   If top_score < CONFIDENCE_THRESHOLD → return empty list.
///   Sparse, high-confidence suggestions are more valuable than noisy lists.
use super::classifier::ClassifiedIntent;

/// Minimum score for a suggestion to be shown.
const CONFIDENCE_THRESHOLD: f64 = 0.30;

/// Maximum number of suggestions to return.
const MAX_SUGGESTIONS: usize = 6;

pub struct CommandRanker;

impl CommandRanker {
    pub fn new() -> Self {
        Self
    }

    /// Rank candidates with intent bias.
    pub fn rank(
        &self,
        mut candidates: Vec<CommandSuggestion>,
        intent: &ClassifiedIntent,
    ) -> Vec<CommandSuggestion> {
        if candidates.is_empty() {
            return Vec::new();
        }

        for candidate in &mut candidates {
            let base = candidate.score;

            // Intent weight: how much this source is biased by the intent.
            let intent_weight = intent
                .source_biases
                .get(&candidate.source)
                .copied()
                .unwrap_or(0.10);

            // Recency decay: newer suggestions get a small boost.
            // For now, use a simple heuristic based on source type.
            let recency = match candidate.source.as_str() {
                "workflow" => 0.7, // Active plans are always recent
                "snapshot" => 0.5, // Snapshots are somewhat recent
                "context" => 0.8,  // Current context is very recent
                "action" => 0.4,   // Actions are timeless
                "semantic" => 0.6, // Memory entries are moderately recent
                _ => 0.3,
            };

            // Context relevance: does this match the current desktop state?
            let context_relevance = if candidate.source == "context" {
                0.9
            } else {
                0.4
            };

            // Frequency boost: common actions get a small boost.
            let frequency = match candidate.action.as_str() {
                "open_app" => 0.5,
                "focus_window" => 0.4,
                "switch_workspace" => 0.3,
                _ => 0.2,
            };

            // Composite score.
            let final_score = (base * 0.30)
                + (recency * 0.20)
                + (context_relevance * 0.20)
                + (frequency * 0.10)
                + (intent_weight * 0.20);

            candidate.score = final_score.min(1.0);
        }

        // Sort by score descending.
        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate by action + params.
        candidates.dedup_by(|a, b| a.action == b.action && a.action_params == b.action_params);

        candidates
    }

    /// Filter by confidence threshold.
    /// Returns empty Vec if no suggestion meets the threshold.
    pub fn filter_confidence(&self, candidates: Vec<CommandSuggestion>) -> Vec<CommandSuggestion> {
        if candidates.is_empty() {
            return Vec::new();
        }

        // If the top suggestion doesn't meet threshold, return nothing.
        if candidates[0].score < CONFIDENCE_THRESHOLD {
            return Vec::new();
        }

        // Take top N.
        candidates.into_iter().take(MAX_SUGGESTIONS).collect()
    }
}
