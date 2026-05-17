/// Desktop action execution subsystem.
///
/// All AI-driven desktop actions flow through this module.
/// The AI runtime NEVER directly manipulates the OS.
///
/// Architecture:
///   AI Runtime → Intent → ActionRequest → enad actions → OS APIs
///
/// Every action:
///   - is observable (emits lifecycle events)
///   - supports cancellation
///   - reports failures
///   - streams execution status
///   - respects permission boundaries

pub mod types;
pub mod executor;
pub mod handlers;
