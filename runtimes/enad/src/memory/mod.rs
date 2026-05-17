/// Working memory subsystem for EnaOS.
///
/// Provides persistent contextual memory for the operating environment.
/// Memory is stored in SQLite and exposed via enad IPC and the AI runtime.
///
/// Architecture:
///   enad events → MemoryStore → SQLite → AI runtime queries
///
/// Memory layers:
///   - Recent events (last 200, auto-expire)
///   - Action history (last 500, auto-expire)
///   - Context snapshots (per workspace, last 24h)
///   - Workspace snapshots (persistent until overwritten)
///   - Intent log (user queries + AI responses, last 100)
///   - Memory summaries (auto-generated context digests)
///
/// Retrieval:
///   - Recency-weighted relevance scoring
///   - FTS5 full-text search for semantic recall
///   - Type-filtered queries (actions, windows, intents, etc.)

pub mod store;
pub mod types;
pub mod capture;
