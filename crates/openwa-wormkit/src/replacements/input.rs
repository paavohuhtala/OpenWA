//! Replay fast-forward via DDGame+0x98B0.
//!
//! When `OPENWA_REPLAY_TEST=1`, hooks TurnManager_ProcessFrame (0x55FDA0)
//! and sets DDGame+0x98B0 (fast-forward active flag) each frame.
//!
//! When this flag is set, FUN_005307A0 processes up to 50 game frames per
//! render cycle. Sound is suppressed (FUN_00546B50) and rendering is skipped
//! (FUN_00529F30). The flag gets cleared at turn boundaries (FUN_00534540,
//! FUN_0055BDD0), so we re-set it every frame.
//!
//! This is the same mechanism triggered by key 0x35 (spacebar) during replay.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::hook;
use crate::log_line;
use openwa_lib::rebase::rb;
use openwa_types::address::va;
use openwa_types::ddgame::DDGame;

/// Trampoline to the original TurnManager_ProcessFrame.
static ORIG_TURN_MANAGER: AtomicU32 = AtomicU32::new(0);

/// Get the DDGame pointer (session+0xA0 → DDGameWrapper+0x488).
#[inline]
unsafe fn get_ddgame() -> *mut DDGame {
    let session = *(rb(va::G_GAME_SESSION) as *const u32);
    if session == 0 {
        return core::ptr::null_mut();
    }
    let wrapper = *((session + 0xA0) as *const u32);
    if wrapper == 0 {
        return core::ptr::null_mut();
    }
    *((wrapper + 0x488) as *const *mut DDGame)
}

/// Hook for TurnManager_ProcessFrame (stdcall, 1 param = TurnGame*).
///
/// Called every frame from TurnGame_HandleMessage case 2 (FrameFinish).
/// Sets DDGame.fast_forward_active = 1 to enable multi-frame processing.
unsafe extern "stdcall" fn hook_turn_manager(turngame: u32) {
    static DIAG_COUNT: AtomicU32 = AtomicU32::new(0);

    // Call original first
    let orig: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(ORIG_TURN_MANAGER.load(Ordering::Relaxed));
    orig(turngame);

    let ddgame = get_ddgame();
    if ddgame.is_null() {
        return;
    }

    // Set fast-forward active flag
    (*ddgame).fast_forward_active = 1;

    let diag = DIAG_COUNT.fetch_add(1, Ordering::Relaxed);
    if diag == 0 {
        let _ = log_line(&format!(
            "[Input] Fast-forward active (DDGame=0x{:08X})",
            ddgame as u32
        ));
    }
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_REPLAY_TEST").is_err() {
        return Ok(());
    }

    let _ = log_line("[Input] Replay test mode — hooking TurnManager_ProcessFrame for fast-forward");

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
