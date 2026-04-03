//! OpenWA immediate-mode debug UI.
//!
//! Spawns a floating egui window (backed by OpenGL via eframe) in a background
//! thread. The window is in the same process as WA.exe, so it can read game
//! memory directly through the `openwa-core` typed structs.
//!
//! # Usage
//!
//! ```rust
//! // In openwa-dll lib.rs, after rebase::init() and hook installation:
//! if std::env::var("OPENWA_DEBUG_UI").is_ok() {
//!     openwa_debugui::spawn();
//! }
//! ```
//!
//! # Log integration
//!
//! Any crate that depends on `openwa-debugui` can push events to the log panel:
//!
//! ```rust
//! openwa_debugui::log::push("some hook fired");
//! ```

mod app;
pub mod log;

use app::DebugApp;

/// Spawn the debug UI window in a background thread.
///
/// Returns immediately. The window runs until closed by the user.
/// This must be called *after* `openwa_core::rebase::init()`.
pub fn spawn() {
    std::thread::spawn(|| {
        if let Err(e) = run_window() {
            // Not much we can do here; the window just won't appear.
            eprintln!("[openwa-debugui] fatal: {e:?}");
        }
    });
}

fn run_window() -> Result<(), eframe::Error> {
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OpenWA Debug")
            .with_inner_size([760.0, 560.0]),
        ..Default::default()
    };

    // On Windows we run in a background thread (DLL injection), so we need
    // to allow the event loop to be created on a non-main thread.
    #[cfg(target_os = "windows")]
    {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        }));
    }

    eframe::run_native(
        "OpenWA Debug",
        options,
        Box::new(|_cc| Ok(Box::new(DebugApp::default()))),
    )
}
