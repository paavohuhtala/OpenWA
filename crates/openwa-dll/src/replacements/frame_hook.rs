//! Per-frame hook on TurnManager_ProcessFrame (0x55FDA0).
//!
//! Provides the central frame boundary hook used by multiple subsystems:
//! - Debug synchronization (pause/resume/step via debug CLI)
//! - Hardware watchpoint arming at specific frames
//!
//! This hook is always installed (both normal and baseline modes).

use std::sync::atomic::{AtomicU32, Ordering};

use crate::hook;
use crate::log_line;
use openwa_game::address::va;
use openwa_game::engine::game_session;

/// Trampoline to the original TurnManager_ProcessFrame.
static ORIG_TURN_MANAGER: AtomicU32 = AtomicU32::new(0);

/// Total frames processed by TurnManager_ProcessFrame.
static FRAMES_PROCESSED: AtomicU32 = AtomicU32::new(0);

/// Get the number of frames processed so far.
pub fn frames_processed() -> u32 {
    FRAMES_PROCESSED.load(Ordering::Relaxed)
}

/// Hook for TurnManager_ProcessFrame (stdcall, 1 param = WorldRoot*).
unsafe extern "stdcall" fn hook_turn_manager(turngame: u32) {
    unsafe {
        // Check debug sync BEFORE processing the frame — allows pausing at frame boundary
        let world = game_session::get_game_world();
        if !world.is_null() {
            let game_frame = (*world).frame_counter;
            crate::debug_sync::on_frame_start(game_frame);

            // Hardware watchpoint: arm once at the watch frame
            static WATCH_ARMED: core::sync::atomic::AtomicBool =
                core::sync::atomic::AtomicBool::new(false);
            if !WATCH_ARMED.load(Ordering::Relaxed)
                && let Ok(val) = std::env::var("OPENWA_WATCH_FRAME")
            {
                let target: i32 = val.parse().unwrap_or(0);
                if game_frame >= target {
                    WATCH_ARMED.store(true, Ordering::Relaxed);
                    crate::debug_watchpoint::prepare();
                    // Select watchpoint base: GameSession, Display, GameRuntime, or GameWorld
                    let watch_base = if std::env::var("OPENWA_WATCH_SESSION").is_ok() {
                        game_session::get_game_session() as *mut u8
                    } else if std::env::var("OPENWA_WATCH_DISPLAY").is_ok() {
                        let runtime = game_session::get_runtime();
                        *(runtime.byte_add(0x4D0) as *const *mut u8)
                    } else if std::env::var("OPENWA_WATCH_WRAPPER").is_ok() {
                        game_session::get_runtime() as *mut u8
                    } else {
                        world as *mut u8
                    };
                    crate::debug_watchpoint::on_base_known(watch_base);
                }
            }
        }

        // Call original
        let orig: unsafe extern "stdcall" fn(u32) =
            core::mem::transmute(ORIG_TURN_MANAGER.load(Ordering::Relaxed));
        orig(turngame);

        let frame = FRAMES_PROCESSED.fetch_add(1, Ordering::Relaxed) + 1;

        // Snapshot capture: run all captures once at frame 10 when env var is set
        if frame == 10 && std::env::var("OPENWA_CAPTURE_SNAPSHOTS").is_ok() {
            super::bitgrid::capture_line_snapshots();
            super::bitgrid::capture_blit_snapshots();
            super::bitgrid::capture_stippled_tiled_snapshots();
        }

        // Replay test: fast-forward + milestone tracking (no-op if not in replay test mode)
        super::replay_test::on_frame(frame);
    }
}

pub fn install() -> Result<(), String> {
    let _ = log_line("[FrameHook] Hooking TurnManager_ProcessFrame");

    unsafe {
        let trampoline = hook::install(
            "TurnManager_ProcessFrame",
            va::TURN_MANAGER_PROCESS_FRAME,
            hook_turn_manager as *const (),
        )?;
        ORIG_TURN_MANAGER.store(trampoline as u32, Ordering::Relaxed);
    }

    Ok(())
}
