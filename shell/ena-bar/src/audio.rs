//! Audio capture scaffolding for Ena Bar microphone input.
//!
//! Future implementation will use:
//! - **Linux:** PipeWire (pw-stream) for low-latency microphone capture
//! - **macOS:** CoreAudio AudioUnit for system audio input

#![allow(dead_code)]

use tracing::warn;

/// Audio capture state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioState {
    /// Microphone inactive.
    Idle,
    /// Capturing audio from microphone.
    Listening,
    /// Audio data being processed by AI runtime.
    Processing,
}

/// Initialize the audio subsystem.
///
/// Stub: real PipeWire/CoreAudio integration added in future phase.
pub fn init() {
    warn!("Audio subsystem: PipeWire integration not yet implemented");
}

/// Request microphone capture.
pub fn start_capture() -> Option<()> {
    warn!("Audio capture requested but not implemented");
    None
}

/// Stop active microphone capture.
pub fn stop_capture() {
    warn!("Audio capture stop requested but not implemented");
}
