use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use clap::Parser;
use gtk4::prelude::*;
use gtk4::{gdk, glib, EventControllerKey};
use tracing_subscriber::EnvFilter;

mod audio;
mod bar;
mod config;
mod ipc;
mod orchestration_ui;

/// Load embedded CSS stylesheet.
fn load_style() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn main() -> glib::ExitCode {
    // ── Tracing ─────────────────────────────────────────────────
    let args = config::Args::parse();
    let filter = if args.verbose {
        EnvFilter::new("ena_bar=debug")
    } else {
        EnvFilter::new("ena_bar=info")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();

    // ── GTK application ─────────────────────────────────────────
    let app = gtk4::Application::builder()
        .application_id("com.enaos.bar")
        .build();

    let running = Arc::new(AtomicBool::new(true));
    let socket_path = args.socket_path.clone();

    // Initialize audio subsystem stub
    audio::init();

    app.connect_activate(move |_app| {
        load_style();

        // ── Window ──────────────────────────────────────────────
        let window = gtk4::Window::new();
        window.set_title(Some("Ena Bar"));
        window.set_decorated(false);
        window.set_resizable(false);
        window.set_default_size(640, 56);

        // Layer-shell (Linux) or floating bottom-center (macOS dev)
        #[cfg(target_os = "linux")]
        setup_layer_shell(&window);
        #[cfg(not(target_os = "linux"))]
        setup_macos_window(&window);

        // ── Bar widget ──────────────────────────────────────────
        let ena_bar = bar::EnaBar::new();
        window.set_child(Some(&ena_bar.container));

        // ── Keyboard shortcut controller ────────────────────────
        // Note: On Wayland with layer-shell KeyboardMode::OnDemand, the bar
        // receives keyboard events when focused. A true global shortcut
        // (summoning from any app) requires D-Bus integration in enad.
        let bar_for_keys = ena_bar.clone();
        let ctrl = EventControllerKey::new();
        ctrl.connect_key_pressed(move |_ctrl, keyval, _code, _state| {
            match keyval {
                gdk::Key::Escape => {
                    tracing::info!("Escape: dismiss bar");
                    bar_for_keys.set_state(bar::BarState::Collapsed);
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    glib::Propagation::Proceed
                }
                _ => glib::Propagation::Proceed,
            }
        });
        window.add_controller(ctrl);

        // ── IPC channel (mpsc: background thread → GTK main loop) ─
        let (tx, rx) = mpsc::channel::<ipc::EnadEvent>();

        // Poll channel on GTK main loop via idle callback
        // Check running flag to allow clean shutdown on window close.
        let r_idle = running.clone();
        glib::idle_add_local(move || {
            if !r_idle.load(Ordering::Relaxed) {
                glib::ControlFlow::Break
            } else {
                while let Ok(event) = rx.try_recv() {
                    ena_bar.handle_event(event);
                }
                glib::ControlFlow::Continue
            }
        });

        // IPC thread: owns the sender (mpsc::Sender is Send)
        let ipc_tx = tx;
        let ipc_running = running.clone();
        let ipc_socket = socket_path.clone();
        std::thread::spawn(move || {
            ipc::run(ipc_socket, ipc_running, ipc_tx);
        });

        // ── Wire shutdown ───────────────────────────────────────
        let r = running.clone();
        window.connect_close_request(move |_| {
            r.store(false, Ordering::Relaxed);
            glib::Propagation::Proceed
        });

        window.present();
    });

    app.run()
}

/// Set up as a Wayland layer-shell overlay (Linux only).
#[cfg(target_os = "linux")]
fn setup_layer_shell(window: &gtk4::Window) {
    use gtk4_layer_shell::{Edge, LayerShell};

    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Overlay);
    window.set_anchor(window, Edge::Bottom, true);
    window.set_anchor(window, Edge::Left, true);
    window.set_anchor(window, Edge::Right, true);
    window.set_margin(window, Edge::Bottom, 8);
    window.set_margin(window, Edge::Left, 16);
    window.set_margin(window, Edge::Right, 16);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    window.set_exclusive_zone(-1);
    tracing::info!("Layer-shell surface initialized");
}

/// Position at bottom-center on macOS for development.
#[cfg(not(target_os = "linux"))]
fn setup_macos_window(window: &gtk4::Window) {
    if let Some(display) = gdk::Display::default() {
        let monitors = display.monitors();
        if monitors.n_items() > 0 {
            if let Some(obj) = monitors.item(0) {
                if let Some(_monitor) = obj.downcast::<gdk::Monitor>().ok() {
                    window.set_default_size(640, 56);
                    window.set_decorated(false);
                    window.set_resizable(false);
                    window.present();
                    tracing::info!("Window positioned at bottom-center (macOS dev)");
                }
            }
        }
    }
}
