//! Headful replay test support: fast-forward and gameplay milestone tracking.
//!
//! When `OPENWA_REPLAY_TEST=1`, enables:
//! - **Fast-forward**: Sets DDGame+0x98B0 each frame so WA processes up to 50
//!   game frames per render cycle (same as spacebar during replay playback).
//! - **Gameplay milestones**: Tracks match start/completion via alive team counting,
//!   reported at DLL detach for machine-readable test validation.
//!
//! These features are specific to headful (interactive) replay testing.
//! Headless tests run at maximum CPU speed without needing fast-forward,
//! and use log comparison instead of milestone markers.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

use crate::log_line;
use openwa_game::engine::game_session;
use openwa_game::engine::DDGame;
use openwa_game::engine::TeamArena;

// ---------------------------------------------------------------------------
// Fast-forward
// ---------------------------------------------------------------------------

/// Whether fast-forward is active (set once at install, read every frame).
static FAST_FORWARD: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// Gameplay milestone tracking
// ---------------------------------------------------------------------------

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

/// Called every frame from frame_hook (after the original TurnManager runs).
///
/// Handles fast-forward flag setting and periodic milestone checks.
pub unsafe fn on_frame(frame: u32) {
    unsafe {
        let ddgame = game_session::get_ddgame();
        if ddgame.is_null() {
            return;
        }

        // Re-set fast-forward each frame (WA clears it at turn boundaries)
        if FAST_FORWARD.load(Ordering::Relaxed) {
            (*ddgame).fast_forward_active = 1;
        }

        // Milestone detection (check every 50 frames to reduce overhead).
        // Skip the first 100 frames to let game state fully initialize.
        if frame >= 100 && frame.is_multiple_of(50) {
            check_milestones(ddgame, frame);
        }
    }
}

/// Count how many teams have at least one alive worm.
///
/// Returns (alive_team_count, total_team_count).
unsafe fn count_alive_teams(ddgame: *const DDGame) -> (i32, i32) {
    unsafe {
        let arena = &raw const (*ddgame).team_arena;
        let team_count = (*arena).team_count;
        if team_count <= 0 || team_count > 6 {
            return (0, 0);
        }

        let mut alive_teams = 0i32;
        for t in 1..=team_count as usize {
            let mut team_has_alive = false;
            for w in 1..=8usize {
                if (*TeamArena::team_worm(arena, t, w)).health > 0 {
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
}

/// Check and update gameplay milestones.
unsafe fn check_milestones(ddgame: *const DDGame, frame: u32) {
    unsafe {
        if MATCH_COMPLETED.load(Ordering::Relaxed) {
            return;
        }

        let (alive, _total) = count_alive_teams(ddgame);

        if !MATCH_STARTED.load(Ordering::Relaxed) {
            if alive >= 2 {
                MATCH_STARTED.store(true, Ordering::Relaxed);
                TEAMS_AT_START.store(alive as u32, Ordering::Relaxed);
            }
        } else if alive <= 1 {
            MATCH_COMPLETED.store(true, Ordering::Relaxed);
            COMPLETION_FRAME.store(frame, Ordering::Relaxed);
            ALIVE_AT_END.store(alive, Ordering::Relaxed);
        }
    }
}

/// Write gameplay milestone report to the log.
///
/// Called from DLL_PROCESS_DETACH to provide a final summary of game progress.
pub fn write_gameplay_report() {
    use crate::log_line;

    // Final milestone check if DDGame is still alive at detach time.
    unsafe {
        let ddgame = openwa_game::engine::game_session::get_ddgame();
        if !ddgame.is_null() {
            let frame = super::frame_hook::frames_processed();
            check_milestones(ddgame, frame);
        }
    }

    let frames = super::frame_hook::frames_processed();
    let started = MATCH_STARTED.load(Ordering::Relaxed);
    let mut completed = MATCH_COMPLETED.load(Ordering::Relaxed);

    // If DDGame was already cleaned up (null at detach), that means
    // DDGameWrapper::EndGame ran — the game session ended normally. If the
    // match had started, this counts as completion. This handles cases like
    // Surrender where TeamArena health values don't reflect the game-over
    // state, so the per-frame alive-team check never triggers.
    if started && !completed {
        let ddgame_null = unsafe { game_session::get_ddgame().is_null() };
        if ddgame_null && frames > 0 {
            completed = true;
            MATCH_COMPLETED.store(true, Ordering::Relaxed);
            COMPLETION_FRAME.store(frames, Ordering::Relaxed);
        }
    }

    let _ = log_line("--- Gameplay Checks ---");

    if frames > 0 {
        let _ = log_line(&format!(
            "[GAMEPLAY PASS] Game initialized - {} frames processed",
            frames
        ));
    } else {
        let _ = log_line(
            "[GAMEPLAY FAIL] Game initialized - no frames processed (game may not have started)",
        );
    }

    if started {
        let teams = TEAMS_AT_START.load(Ordering::Relaxed);
        let _ = log_line(&format!(
            "[GAMEPLAY PASS] Match started - {} teams with alive worms detected",
            teams
        ));
    } else if frames > 0 {
        let _ = log_line("[GAMEPLAY FAIL] Match started - never detected multiple alive teams");
    } else {
        let _ = log_line("[GAMEPLAY FAIL] Match started - game never initialized");
    }

    if completed {
        let end_frame = COMPLETION_FRAME.load(Ordering::Relaxed);
        let alive = ALIVE_AT_END.load(Ordering::Relaxed);
        let outcome = if alive == 1 {
            "winner decided"
        } else if alive == 0 {
            "draw (all eliminated)"
        } else {
            "game session ended normally"
        };
        let _ = log_line(&format!(
            "[GAMEPLAY PASS] Match completed - {} at frame {}",
            outcome, end_frame
        ));
    } else if started {
        let _ = log_line("[GAMEPLAY FAIL] Match completed - match started but never finished");
    } else {
        let _ = log_line("[GAMEPLAY FAIL] Match completed - match never started");
    }
}

pub fn install() -> Result<(), String> {
    if std::env::var("OPENWA_REPLAY_TEST").is_ok() {
        FAST_FORWARD.store(true, Ordering::Relaxed);
        let _ = log_line("[ReplayTest] Replay test mode — fast-forward enabled");
    }

    Ok(())
}
