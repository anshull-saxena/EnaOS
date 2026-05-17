/// Desktop integration subsystem for enad.
///
/// Each module monitors a specific Linux desktop service via D-Bus or
/// external tools and publishes SystemEvents to the enad event bus.
///
/// Architecture:
///   D-Bus service → zbus proxy → async watcher → EventBus::publish()
///
/// All subsystems are designed to fail gracefully — if a service is
/// unavailable (e.g., UPower on a desktop without battery), the watcher
/// logs a warning and exits cleanly without crashing enad.

#[cfg(target_os = "linux")]
pub mod upower;

#[cfg(target_os = "linux")]
pub mod network;

#[cfg(target_os = "linux")]
pub mod window;

#[cfg(target_os = "linux")]
pub mod workspace;

#[cfg(target_os = "linux")]
pub mod clipboard;

#[cfg(target_os = "linux")]
pub mod notifications;

#[cfg(target_os = "linux")]
pub mod audio;
