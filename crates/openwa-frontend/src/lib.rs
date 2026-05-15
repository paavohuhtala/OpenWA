//! OpenWA custom match-launcher frontend (prototype).
//!
//! Spawns a floating egui window alongside WA's MFC frontend. The window
//! exposes a "Start match" button that populates `GameInfo` from a small
//! set of UI controls and calls
//! [`openwa_game::wa::frontend::launch_game_session`] directly, bypassing
//! WA's dialog system.
//!
//! Long-term goal: replace WA's MFC frontend entirely. This crate is the
//! first prototype slice — just enough UI to launch a live offline match.
//!
//! # Usage
//!
//! ```rust
//! // In openwa-dll lib.rs, after rebase::init() and hook installation:
//! if std::env::var("OPENWA_FRONTEND").is_ok() {
//!     openwa_frontend::spawn();
//! }
//! ```

mod app;
mod launch;

use app::MatchLauncherApp;

/// Spawn the match-launcher window in a background thread.
///
/// Returns immediately. The window runs until closed by the user.
/// Must be called *after* `openwa_game::rebase::init()`.
pub fn spawn() {
    std::thread::spawn(|| {
        if let Err(e) = run_window() {
            eprintln!("[openwa-frontend] fatal: {e:?}");
        }
    });
}

fn run_window() -> Result<(), eframe::Error> {
    let mut options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("OpenWA Match Launcher")
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    #[cfg(target_os = "windows")]
    {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        }));
    }

    eframe::run_native(
        "OpenWA Match Launcher",
        options,
        Box::new(|_cc| Ok(Box::new(MatchLauncherApp::default()))),
    )
}
