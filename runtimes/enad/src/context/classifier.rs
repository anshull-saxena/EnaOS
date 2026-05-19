/// IntentClassifier — classifies user queries into operational intent types.
///
/// This is a keyword-based classifier (no ML, no embeddings) that maps
/// the user's query to an intent type with associated bias weights.
///
/// Intent types determine which suggestion sources are prioritized:
/// - "continue" -> workflows, snapshots, active projects
/// - "restore" -> snapshots, restoration plans
/// - "open" -> applications, URLs, projects
/// - "switch" -> workspaces, windows
/// - "search" -> memory, file search, browser
/// - "generic" -> balanced across all sources

use std::collections::HashMap;

/// Classified user intent with bias weights for each suggestion source.
#[derive(Debug, Clone)]
pub struct ClassifiedIntent {
    /// The primary intent type.
    pub intent_type: IntentType,
    /// Bias weights for each suggestion source (0.0-1.0).
    pub source_biases: HashMap<String, f64>,
}

/// Operational intent types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IntentType {
    Continue,
    Restore,
    Open,
    Switch,
    Search,
    Generic,
}

impl std::fmt::Display for IntentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IntentType::Continue => write!(f, "continue"),
            IntentType::Restore => write!(f, "restore"),
            IntentType::Open => write!(f, "open"),
            IntentType::Switch => write!(f, "switch"),
            IntentType::Search => write!(f, "search"),
            IntentType::Generic => write!(f, "generic"),
        }
    }
}

pub struct IntentClassifier {
    /// Keywords mapped to intent types.
    keywords: Vec<(&'static str, IntentType)>,
}

impl IntentClassifier {
    pub fn new() -> Self {
        Self {
            keywords: vec![
                // Continue intent — resume interrupted work
                ("continue", IntentType::Continue),
                ("resume", IntentType::Continue),
                ("restart", IntentType::Continue),
                ("reopen", IntentType::Continue),
                ("pick up", IntentType::Continue),
                ("back to", IntentType::Continue),

                // Restore intent — go back to previous state
                ("restore", IntentType::Restore),
                ("undo", IntentType::Restore),
                ("go back", IntentType::Restore),
                ("previous", IntentType::Restore),
                ("revert", IntentType::Restore),
                ("rollback", IntentType::Restore),

                // Open intent — launch things
                ("open", IntentType::Open),
                ("launch", IntentType::Open),
                ("start", IntentType::Open),
                ("run", IntentType::Open),
                ("show me", IntentType::Open),

                // Switch intent — change context
                ("switch", IntentType::Switch),
                ("change", IntentType::Switch),
                ("move to", IntentType::Switch),
                ("go to", IntentType::Switch),
                ("toggle", IntentType::Switch),

                // Search intent — find things
                ("search", IntentType::Search),
                ("find", IntentType::Search),
                ("look for", IntentType::Search),
                ("query", IntentType::Search),
            ],
        }
    }

    /// Classify a query into an intent type with source biases.
    pub fn classify(&self, query: &str) -> ClassifiedIntent {
        let lower = query.to_lowercase();

        // Check for multi-word keywords first (longer matches are more specific).
        let mut best_match: Option<(&str, &IntentType)> = None;
        for (keyword, intent) in &self.keywords {
            if lower.contains(keyword) {
                // Prefer longer keyword matches (more specific).
                match best_match {
                    None => best_match = Some((keyword, intent)),
                    Some((existing_kw, _)) if keyword.len() > existing_kw.len() => {
                        best_match = Some((keyword, intent));
                    }
                    _ => {}
                }
            }
        }

        let intent_type = best_match
            .map(|(_, intent)| intent.clone())
            .unwrap_or(IntentType::Generic);

        let source_biases = self.compute_biases(&intent_type);

        ClassifiedIntent {
            intent_type,
            source_biases,
        }
    }

    /// Compute source bias weights for an intent type.
    /// These weights are added to the base score during ranking.
    fn compute_biases(&self, intent: &IntentType) -> HashMap<String, f64> {
        let mut biases = HashMap::new();

        match intent {
            IntentType::Continue => {
                biases.insert("workflow".to_string(), 0.40);
                biases.insert("snapshot".to_string(), 0.30);
                biases.insert("action".to_string(), 0.15);
                biases.insert("semantic".to_string(), 0.10);
                biases.insert("context".to_string(), 0.10);
            }
            IntentType::Restore => {
                biases.insert("snapshot".to_string(), 0.50);
                biases.insert("workflow".to_string(), 0.20);
                biases.insert("context".to_string(), 0.10);
                biases.insert("action".to_string(), 0.10);
                biases.insert("semantic".to_string(), 0.05);
            }
            IntentType::Open => {
                biases.insert("action".to_string(), 0.40);
                biases.insert("context".to_string(), 0.25);
                biases.insert("semantic".to_string(), 0.15);
                biases.insert("workflow".to_string(), 0.10);
                biases.insert("snapshot".to_string(), 0.05);
            }
            IntentType::Switch => {
                biases.insert("context".to_string(), 0.45);
                biases.insert("action".to_string(), 0.20);
                biases.insert("semantic".to_string(), 0.15);
                biases.insert("workflow".to_string(), 0.10);
                biases.insert("snapshot".to_string(), 0.05);
            }
            IntentType::Search => {
                biases.insert("semantic".to_string(), 0.40);
                biases.insert("context".to_string(), 0.25);
                biases.insert("action".to_string(), 0.15);
                biases.insert("workflow".to_string(), 0.10);
                biases.insert("snapshot".to_string(), 0.05);
            }
            IntentType::Generic => {
                biases.insert("action".to_string(), 0.15);
                biases.insert("context".to_string(), 0.15);
                biases.insert("semantic".to_string(), 0.15);
                biases.insert("workflow".to_string(), 0.10);
                biases.insert("snapshot".to_string(), 0.10);
            }
        }

        biases
    }
}
