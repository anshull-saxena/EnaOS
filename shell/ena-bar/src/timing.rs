/// Lightweight timing instrumentation for interaction latency measurement.
///
/// Tracks:
/// - Keystroke → render latency
/// - IPC roundtrip time
/// - Ranking computation time
/// - UI update time
///
/// Only active in verbose/dev mode (controlled by `timing` feature flag).

#[cfg(feature = "timing")]
use std::cell::RefCell;
#[cfg(feature = "timing")]
use std::time::Instant;

/// Timing phase tracker for a single query lifecycle.
#[cfg(feature = "timing")]
#[derive(Debug)]
pub struct QueryTiming {
    pub query: String,
    pub keystroke_ts: Instant,
    pub debounce_end: Option<Instant>,
    pub ipc_start: Option<Instant>,
    pub ipc_end: Option<Instant>,
    pub ranking_end: Option<Instant>,
    pub render_end: Option<Instant>,
}

#[cfg(feature = "timing")]
thread_local! {
    static ACTIVE_TIMING: RefCell<Option<QueryTiming>> = const { RefCell::new(None) };
}

#[cfg(feature = "timing")]
pub fn start_query(query: &str) {
    ACTIVE_TIMING.with(|t| {
        *t.borrow_mut() = Some(QueryTiming {
            query: query.to_string(),
            keystroke_ts: Instant::now(),
            debounce_end: None,
            ipc_start: None,
            ipc_end: None,
            ranking_end: None,
            render_end: None,
        });
    });
}

#[cfg(feature = "timing")]
pub fn mark_debounce_end() {
    ACTIVE_TIMING.with(|t| {
        if let Some(ref mut timing) = *t.borrow_mut() {
            timing.debounce_end = Some(Instant::now());
        }
    });
}

#[cfg(feature = "timing")]
pub fn mark_ipc_start() {
    ACTIVE_TIMING.with(|t| {
        if let Some(ref mut timing) = *t.borrow_mut() {
            timing.ipc_start = Some(Instant::now());
        }
    });
}

#[cfg(feature = "timing")]
pub fn mark_ipc_end() {
    ACTIVE_TIMING.with(|t| {
        if let Some(ref mut timing) = *t.borrow_mut() {
            timing.ipc_end = Some(Instant::now());
        }
    });
}

#[cfg(feature = "timing")]
pub fn mark_render_end() {
    ACTIVE_TIMING.with(|t| {
        if let Some(ref mut timing) = *t.borrow_mut() {
            timing.render_end = Some(Instant::now());
            timing.report();
        }
    });
}

#[cfg(feature = "timing")]
impl QueryTiming {
    fn report(&self) {
        let total = self.render_end.map(|t| t.duration_since(self.keystroke_ts));
        let debounce = match (self.debounce_end, self.keystroke_ts) {
            (Some(end), start) => Some(end.duration_since(start)),
            _ => None,
        };
        let ipc = match (self.ipc_end, self.ipc_start) {
            (Some(end), Some(start)) => Some(end.duration_since(start)),
            _ => None,
        };

        tracing::debug!(
            "TIMING [{}] total={:?} debounce={:?} ipc={:?}",
            self.query,
            total,
            debounce,
            ipc
        );
    }
}

/// No-op stubs when timing feature is disabled.
#[cfg(not(feature = "timing"))]
pub fn start_query(_query: &str) {}
#[cfg(not(feature = "timing"))]
pub fn mark_debounce_end() {}
#[cfg(not(feature = "timing"))]
pub fn mark_ipc_start() {}
#[cfg(not(feature = "timing"))]
pub fn mark_ipc_end() {}
#[cfg(not(feature = "timing"))]
pub fn mark_render_end() {}
