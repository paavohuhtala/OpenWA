//! Replay fast-forward via DDGame+0x98B0 and gameplay milestone tracking.
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
//!
//! ## Gameplay Milestones
//!
//! The frame hook also tracks gameplay milestones via atomic flags:
//! - **Game initialized**: TurnManager_ProcessFrame called at least once
//! - **Match started**: Multiple teams with alive worms detected
//! - **Match completed**: Only one (or zero) teams have alive worms remaining
//!
//! These milestones are reported by `write_gameplay_report()` at DLL detach,
//! providing reliable hook-based game progress detection without timers.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use crate::hook;
use crate::log_line;
use openwa_core::address::va;
use openwa_core::engine::ddgame::{offsets, TeamArenaRef};
use openwa_core::engine::{DDGame, DDGameWrapper};
use openwa_core::rebase::rb;

/// Trampoline to the original TurnManager_ProcessFrame.
static ORIG_TURN_MANAGER: AtomicU32 = AtomicU32::new(0);

/// Whether to set fast-forward flag (only in replay test mode).
static FAST_FORWARD: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Gameplay milestone tracking
// ---------------------------------------------------------------------------

/// Total frames processed by TurnManager_ProcessFrame.
static FRAMES_PROCESSED: AtomicU32 = AtomicU32::new(0);

/// Milestone: at least 2 teams with alive worms detected.
static MATCH_STARTED: AtomicBool = AtomicBool::new(false);

/// Milestone: match decided — only 0 or 1 teams have alive worms.
static MATCH_COMPLETED: AtomicBool = AtomicBool::new(false);

/// Number of teams that had alive worms when match started.
static TEAMS_AT_START: AtomicU32 = AtomicU32::new(0);

/// Frame number when match completion was detected.
static COMPLETION_FRAME: AtomicU32 = AtomicU32::new(0);

/// Number of alive teams when match completed (0 = draw, 1 = winner).
static ALIVE_AT_END: AtomicI32 = AtomicI32::new(-1);

/// Get the DDGame pointer (session+0xA0 → DDGameWrapper.ddgame).
#[inline]
unsafe fn get_ddgame() -> *mut DDGame {
    let session = *(rb(va::G_GAME_SESSION) as *const u32);
    if session == 0 {
        return core::ptr::null_mut();
    }
    let wrapper_ptr = *((session + 0xA0) as *const *const DDGameWrapper);
    if wrapper_ptr.is_null() {
        return core::ptr::null_mut();
    }
    (*wrapper_ptr).ddgame
}

/// Dump a memory region as DWORDs with automatic classification.
///
/// Uses `openwa_core::mem::classify_pointer` for pointer detection.
/// See that module for the classification categories.
pub unsafe fn dump_region(base_ptr: *const u8, offset: usize, size: usize, struct_name: &str) {
    use openwa_core::address::va;
    use openwa_core::mem;
    use openwa_core::rebase::rb;
    use openwa_debug_proto::PointerKind;

    let wa_base = rb(va::IMAGE_BASE);
    let delta = wa_base.wrapping_sub(va::IMAGE_BASE);

    let _ = log_line(&format!(
        "\n=== {}+0x{:04X}..0x{:04X} ===",
        struct_name,
        offset,
        offset + size
    ));

    let dword_count = size / 4;
    for i in 0..dword_count {
        let field_offset = offset + i * 4;
        let val = *(base_ptr.add(field_offset) as *const u32);
        if val == 0 {
            continue;
        }

        if let Some(info) = mem::classify_pointer(val, delta) {
            let detail_str = info.detail.as_deref().unwrap_or("");
            match info.kind {
                PointerKind::Vtable => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [VTABLE] g:0x{:08X} {}",
                        field_offset, val, info.ghidra_value, detail_str
                    ));
                }
                PointerKind::Code => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [CODE] g:0x{:08X}",
                        field_offset, val, info.ghidra_value
                    ));
                }
                PointerKind::Data => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [DATA] g:0x{:08X}",
                        field_offset, val, info.ghidra_value
                    ));
                }
                PointerKind::Object => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [OBJECT] {}",
                        field_offset, val, detail_str
                    ));
                }
                PointerKind::Heap => {
                    let _ = log_line(&format!(
                        "  +0x{:04X}: 0x{:08X} [ptr] {}",
                        field_offset, val, detail_str
                    ));
                }
            }
        } else if val < 0x10000 {
            let _ = log_line(&format!(
                "  +0x{:04X}: 0x{:08X} [small={}]",
                field_offset, val, val
            ));
        } else {
            let _ = log_line(&format!("  +0x{:04X}: 0x{:08X} [value]", field_offset, val));
        }
    }
}

/// Count how many teams have at least one alive worm.
///
/// Iterates all 8 worm slots per team and checks `health > 0` (reliable
/// even before sentinel headers are fully populated). This is more
/// conservative than checking headers, which may be populated lazily.
///
/// Returns (alive_team_count, total_team_count).
unsafe fn count_alive_teams(ddgame: *const DDGame) -> (i32, i32) {
    let arena_base = (ddgame as u32) + offsets::TEAM_ARENA_STATE as u32;
    let arena = TeamArenaRef::from_raw(arena_base);
    let team_count = arena.state().team_count;
    if team_count <= 0 || team_count > 6 {
        return (0, 0);
    }

    let mut alive_teams = 0i32;
    // Teams are 1-indexed in the arena (index 0 is unused preamble).
    for t in 1..=team_count as usize {
        let mut team_has_alive = false;
        for w in 1..=8usize {
            let worm = arena.team_worm(t, w);
            if worm.health > 0 {
                team_has_alive = true;
                break;
            }
        }
        if team_has_alive {
            alive_teams += 1;
        }
    }
    (alive_teams, team_count)
}

/// Hook for TurnManager_ProcessFrame (stdcall, 1 param = TurnGame*).
unsafe extern "stdcall" fn hook_turn_manager(turngame: u32) {
    // Check debug sync BEFORE processing the frame — allows pausing at frame boundary
    let ddgame = get_ddgame();
    if !ddgame.is_null() {
        let game_frame = (*ddgame).frame_counter;
        crate::debug_sync::on_frame_start(game_frame);

        // Hardware watchpoint: arm once at the watch frame
        static WATCH_ARMED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
        if !WATCH_ARMED.load(Ordering::Relaxed) {
            if let Ok(val) = std::env::var("OPENWA_WATCH_FRAME") {
                let target: i32 = val.parse().unwrap_or(0);
                if game_frame >= target {
                    WATCH_ARMED.store(true, Ordering::Relaxed);
                    crate::debug_watchpoint::prepare();
                    crate::debug_watchpoint::on_ddgame_alloc(ddgame as *mut u8);
                }
            }
        }
    }

    // Call original
    let orig: unsafe extern "stdcall" fn(u32) =
        core::mem::transmute(ORIG_TURN_MANAGER.load(Ordering::Relaxed));
    orig(turngame);

    let frame = FRAMES_PROCESSED.fetch_add(1, Ordering::Relaxed) + 1;

    if ddgame.is_null() {
        return;
    }

    // Fast-forward for replay test
    if FAST_FORWARD.load(Ordering::Relaxed) {
        (*ddgame).fast_forward_active = 1;
    }

    // Milestone detection (check every 50 frames to reduce overhead).
    // Skip the first 100 frames to let game state fully initialize.
    if frame >= 100 && frame % 50 == 0 {
        check_milestones(ddgame, frame);
    }
}

/// Check and update gameplay milestones.
unsafe fn check_milestones(ddgame: *const DDGame, frame: u32) {
    if MATCH_COMPLETED.load(Ordering::Relaxed) {
        return;
    }

    let (alive, _total) = count_alive_teams(ddgame);

    if !MATCH_STARTED.load(Ordering::Relaxed) {
        if alive >= 2 {
            MATCH_STARTED.store(true, Ordering::Relaxed);
            TEAMS_AT_START.store(alive as u32, Ordering::Relaxed);
        }
    } else {
        // Match started — check if it's now decided
        if alive <= 1 {
            MATCH_COMPLETED.store(true, Ordering::Relaxed);
            COMPLETION_FRAME.store(frame, Ordering::Relaxed);
            ALIVE_AT_END.store(alive, Ordering::Relaxed);
        }
    }
}

/// Write gameplay milestone report to the validation log.
///
/// Called from DLL_PROCESS_DETACH to provide a final summary of game progress.
/// Uses `[GAMEPLAY PASS]` / `[GAMEPLAY FAIL]` markers so the PowerShell script
/// can distinguish these from static checks.
pub fn write_gameplay_report() {
    use crate::validation::log_validation;

    let frames = FRAMES_PROCESSED.load(Ordering::Relaxed);
    let started = MATCH_STARTED.load(Ordering::Relaxed);
    let completed = MATCH_COMPLETED.load(Ordering::Relaxed);

    let _ = log_validation("");
    let _ = log_validation("--- Gameplay Checks ---");

    // Milestone 1: Game initialized (frame hook was called)
    if frames > 0 {
        let _ = log_validation(&format!(
            "[GAMEPLAY PASS] Game initialized - {} frames processed",
            frames
        ));
    } else {
        let _ = log_validation("[GAMEPLAY FAIL] Game initialized - no frames processed (game may not have started)");
    }

    // Milestone 2: Match started (multiple teams with alive worms)
    if started {
        let teams = TEAMS_AT_START.load(Ordering::Relaxed);
        let _ = log_validation(&format!(
            "[GAMEPLAY PASS] Match started - {} teams with alive worms detected",
            teams
        ));
    } else if frames > 0 {
        let _ = log_validation("[GAMEPLAY FAIL] Match started - never detected multiple alive teams");
    } else {
        let _ = log_validation("[GAMEPLAY FAIL] Match started - game never initialized");
    }

    // Milestone 3: Match completed (one or zero teams remain)
    if completed {
        let end_frame = COMPLETION_FRAME.load(Ordering::Relaxed);
        let alive = ALIVE_AT_END.load(Ordering::Relaxed);
        let outcome = if alive == 1 { "winner decided" } else { "draw (all eliminated)" };
        let _ = log_validation(&format!(
            "[GAMEPLAY PASS] Match completed - {} at frame {}",
            outcome, end_frame
        ));
    } else if started {
        let _ = log_validation("[GAMEPLAY FAIL] Match completed - match started but never finished");
    } else {
        let _ = log_validation("[GAMEPLAY FAIL] Match completed - match never started");
    }
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_REPLAY_TEST").is_ok() {
        FAST_FORWARD.store(true, Ordering::Relaxed);
        let _ = log_line("[Input] Replay test mode — fast-forward enabled");
    }

    let _ = log_line("[Input] Hooking TurnManager_ProcessFrame");

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
