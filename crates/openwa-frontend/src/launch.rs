//! Match-launch action: restore a previously-captured `GameInfo` snapshot
//! and call [`openwa_game::wa::frontend::launch_game_session`].
//!
//! ## How this works (v0 prototype)
//!
//! `GameInfo` is the cumulative output of WA's MFC menu navigation flow:
//! scheme picker, team picker, terrain picker, mode-flag writes. Replicating
//! all of that from a custom UI is a substantial RE task. Instead, we let
//! the user launch any match once through WA's normal frontend — the
//! [`openwa_game::engine::game_info_snapshot`] hook captures `GameInfo` at
//! the entry to `launch_game_session`. After that match ends and the user
//! is back at the frontend, our Launch button restores the captured bytes
//! and invokes `launch_game_session` again. This sidesteps GameInfo
//! population entirely for the first slice.
//!
//! Team names from the UI are overlaid on top of the restored snapshot.
//!
//! ## Threading
//!
//! `launch_game_session` has main-thread affinity (mouse cursor APIs,
//! SetActiveWindow/SetFocus, DirectInput / DirectDraw acquisition inside
//! the game-session main loop), so we can't invoke it directly from the
//! egui thread. We stash the `LaunchRequest` in a static slot and schedule
//! an `extern "C"` shim onto WA's main thread via
//! [`openwa_game::main_thread`]; a WH_GETMESSAGE hook drains the slot and
//! runs the shim synchronously on the main thread.

use std::ptr;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use openwa_core::log::log_line;
use openwa_game::address::va;
use openwa_game::engine::game_info::GameInfo;
use openwa_game::engine::game_info_snapshot;
use openwa_game::engine::pending_match::{self, PendingCustomMatch};
use openwa_game::main_thread;
use openwa_game::rebase::rb;
use openwa_game::wa::frontend::SUPPRESS_PRE_LAUNCH_VT13;
use openwa_game::wa::mfc::CWinApp;

fn log(msg: &str) {
    let _ = log_line(&format!("[frontend] {msg}"));
}

/// Selects how `GameInfo` gets populated before `launch_game_session`.
#[derive(Clone, Debug)]
pub enum LaunchMode {
    /// Restore a previously-captured snapshot of a real WA-frontend
    /// match's `GameInfo`, overlay team names, then (optionally) re-run
    /// InitSession. Requires the user to have started one match through
    /// WA's normal frontend in this process.
    Snapshot {
        /// Whether to re-run `GameInfo__InitSession` after restoring.
        call_init_session: bool,
    },
    /// Park a [`PendingCustomMatch`] and drive InitSession in
    /// `CustomLauncher` mode — no snapshot required. The Rust-side
    /// populate logic in `pending_match::apply` writes team identity +
    /// scheme bytes; the rest of InitSession (Rust-native helpers,
    /// LoadOptions) fills in the runtime config.
    Fresh(PendingCustomMatch),
}

/// User-tunable match settings collected by the launcher UI.
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    /// Team A display name (overlaid on snapshot, up to 15 ASCII bytes).
    pub team_a_name: String,
    /// Team B display name (overlaid on snapshot, up to 15 ASCII bytes).
    pub team_b_name: String,
    /// Population strategy. See [`LaunchMode`].
    pub mode: LaunchMode,
}

impl Default for LaunchRequest {
    fn default() -> Self {
        Self {
            team_a_name: "Red".to_owned(),
            team_b_name: "Blue".to_owned(),
            mode: LaunchMode::Snapshot {
                call_init_session: false,
            },
        }
    }
}

/// Result of a launch *schedule* attempt (not the match itself).
#[derive(Debug)]
pub enum LaunchOutcome {
    /// Scheduled onto the main thread. The match will start on the next
    /// MFC message pump tick.
    Scheduled,
    /// Refused (already in-session, no snapshot captured yet, etc.).
    Refused(&'static str),
}

/// Returns true when WA is sitting at the frontend (no active game session).
pub fn is_idle_at_frontend() -> bool {
    unsafe { *(rb(va::G_IN_GAME_SESSION_FLAG) as *const u8) == 0 }
}

/// Returns true if a `GameInfo` snapshot has been captured during this
/// process run.
pub fn has_snapshot() -> bool {
    game_info_snapshot::is_captured()
}

static PENDING_REQUEST: Mutex<Option<LaunchRequest>> = Mutex::new(None);

extern "C" fn run_pending_launch() {
    let req = match PENDING_REQUEST.lock().ok().and_then(|mut g| g.take()) {
        Some(r) => r,
        None => {
            log("main-thread shim fired with empty slot — bug");
            return;
        }
    };

    if !is_idle_at_frontend() {
        log("main-thread shim: not idle, aborting");
        return;
    }

    if let LaunchMode::Snapshot { .. } = &req.mode {
        if let Err(e) = game_info_snapshot::restore() {
            log(&format!("snapshot restore failed: {e}"));
            return;
        }
    }

    unsafe {
        let app = cwin_app();
        if app.is_null() {
            log("aborting: CWinApp singleton is null");
            return;
        }

        // `g_TopModalDialog` — currently-modal MFC CDialog. Passing it lets
        // `launch_game_session` do the full pre/post-game dance (hide → run →
        // rebuild framebuffer → restore audio → re-show + focus).
        const G_TOP_MODAL_DIALOG: u32 = 0x007A03DC;
        let dialog = *(rb(G_TOP_MODAL_DIALOG) as *const *mut openwa_game::wa::mfc::CWnd);
        if dialog.is_null() {
            log("aborting: g_TopModalDialog is null");
            return;
        }

        let want_init_session = match &req.mode {
            LaunchMode::Snapshot { call_init_session } => *call_init_session,
            LaunchMode::Fresh(pending) => {
                pending_match::set(pending.clone());
                true
            }
        };

        overlay_team_names(&req);

        if want_init_session {
            log("calling InitSession in CustomLauncher mode (bridged helpers skipped)");
            // game_version=500: hardcoded by FrontendLocalMP::OnStartMatch
            // (0x4A1260) right before InitSession.
            let gi = rb(va::G_GAME_INFO) as *mut GameInfo;
            (*gi).game_version = 500;
            let _src_guard = openwa_game::engine::launch_source::LaunchSourceGuard::new(
                openwa_game::engine::launch_source::LaunchSource::CustomLauncher,
            );
            openwa_game::engine::config_load::init_session(None);
            log("InitSession returned");
        }

        // Slot 13 expects MFC dialog-handler context our shim doesn't have;
        // its paired post-game render-children walk depends on the nodes
        // slot 13 attaches, so they must skip together.
        SUPPRESS_PRE_LAUNCH_VT13.store(true, Ordering::Release);
        openwa_game::wa::frontend::launch_game_session(app, dialog, ptr::null(), 0);
        SUPPRESS_PRE_LAUNCH_VT13.store(false, Ordering::Release);

        // Replicate the redraw portion of FrontendChangeScreen (palette anim +
        // vtable[0x15C] x2) *without* EndDialog — the suppressed slot-13 +
        // post-game walk leaves the frontend palette at fade-to-black.
        const DIALOG_PALETTE_OBJ: usize = 0x12C;
        const DIALOG_PALETTE_PARAM: usize = 0x134;
        const VTABLE_TRANSITION_METHOD: u32 = 0x15C;
        let eax_value = *((dialog as usize + DIALOG_PALETTE_OBJ) as *const u32);
        let palette_param = *((dialog as usize + DIALOG_PALETTE_PARAM) as *const u32);
        openwa_game::wa::frontend::palette_animation(eax_value, palette_param);
        let vtable = *(dialog as *const u32);
        for i in 1u32..=2 {
            openwa_game::wa_call::thiscall_indirect_1(
                vtable + VTABLE_TRANSITION_METHOD,
                dialog as u32,
                i,
            );
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Overlay UI-supplied team names onto the snapshot-restored team records.
unsafe fn overlay_team_names(req: &LaunchRequest) {
    unsafe {
        let info = rb(va::G_GAME_INFO) as *mut GameInfo;
        write_team_name(info, 0, &req.team_a_name);
        write_team_name(info, 1, &req.team_b_name);
    }
}

unsafe fn write_team_name(info: *mut GameInfo, slot: usize, name: &str) {
    unsafe {
        let rec = &mut (*info).team_records[slot];
        // WA's team_record.name is CP1252; the UI hands us UTF-8.
        openwa_core::cp1252::encode_into_fixed(&mut rec.name, name);
    }
}

unsafe fn cwin_app() -> *mut CWinApp {
    unsafe { *(rb(va::G_CWINAPP) as *const *mut CWinApp) }
}

// ─── Public entry ──────────────────────────────────────────────────────────

/// Queue a match-launch onto WA's main thread. Returns immediately.
pub fn launch(req: &LaunchRequest) -> LaunchOutcome {
    if !is_idle_at_frontend() {
        return LaunchOutcome::Refused("game session already active");
    }
    if matches!(req.mode, LaunchMode::Snapshot { .. }) && !has_snapshot() {
        return LaunchOutcome::Refused(
            "no GameInfo snapshot yet — start one match through the WA frontend first",
        );
    }

    {
        let mut slot = match PENDING_REQUEST.lock() {
            Ok(g) => g,
            Err(_) => return LaunchOutcome::Refused("request mutex poisoned"),
        };
        if slot.is_some() {
            return LaunchOutcome::Refused("another launch already pending");
        }
        *slot = Some(req.clone());
    }

    if main_thread::schedule(run_pending_launch) {
        log("warning: overwrote another scheduled main-thread callback");
    }

    LaunchOutcome::Scheduled
}
