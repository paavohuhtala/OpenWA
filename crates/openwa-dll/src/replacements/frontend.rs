//! FrontendChangeScreen replacement.
//!
//! Original: 0x447A20, stdcall(screen_id), ESI = dialog this (__usercall).
//! Navigates between frontend menu screens via MFC CDialog::EndDialog.

use crate::hook;
use crate::log_line;
use openwa_game::address::va;
use openwa_game::game::ScreenId;
use openwa_game::wa::frontend::frontend_change_screen;
use openwa_game::wa::mfc::CWnd;

/// Hook shim for WA-side callers of `FrontendChangeScreen`. Logs the
/// transition then delegates to the Rust port in openwa-game (which is the
/// single source of truth for the navigation logic).
pub(crate) unsafe extern "cdecl" fn frontend_change_screen_impl(dialog: *mut CWnd, screen_id: u32) {
    unsafe {
        let name = ScreenId::try_from(screen_id as i32)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("Unknown({v})"));
        let _ = log_line(&format!(
            "[FrontendChangeScreen] screen_id={screen_id} ({name})"
        ));

        frontend_change_screen(dialog, screen_id);
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        crate::generated::hooks::install_FrontendChangeScreen()?;

        // Frontend::UnhookInputHooks (0x004ED590) — full replacement;
        // multiple WA-side callers in the modal-dialog input-grab path.
        crate::hook::install(
            "Frontend::UnhookInputHooks",
            va::FRONTEND_UNHOOK_INPUT_HOOKS,
            openwa_game::input::hooks::unhook_input_hooks as *const (),
        )?;

        // Frontend::InstallInputHooks (0x004ED3C0) — full replacement; reached
        // from the mode-1/mode-2 entry helpers (0x004ED420 / 0x004ED4F0) when
        // the user opens a window/system menu or otherwise triggers Win32
        // modal UI mid-game.
        crate::hook::install(
            "Frontend::InstallInputHooks",
            va::FRONTEND_INSTALL_INPUT_HOOKS,
            openwa_game::input::hooks::install_input_hooks as *const (),
        )?;

        // Frontend::ForegroundIdleProc (0x004ED0D0) — now ported in Rust.
        // SetWindowsHookExA is registered with the Rust function directly,
        // so the WA-side address is no longer reachable (only static xref
        // was Frontend::InstallInputHooks itself). Trap as a safety net.
        hook::install_trap!(
            "Frontend::ForegroundIdleProc",
            va::FRONTEND_FOREGROUND_IDLE_PROC
        );

        // Frontend::GetMessageProc (0x004ED160) — ported in Rust.
        // SetWindowsHookExA is registered with the Rust function directly,
        // so the WA-side address is no longer reachable (only static xref
        // was Frontend::InstallInputHooks itself). Trap as a safety net.
        hook::install_trap!("Frontend::GetMessageProc", va::FRONTEND_GET_MESSAGE_PROC);

        // Frontend::PumpModalOrSessionFrame (0x004ED050) — inlined into the
        // Rust GetMessageProc port. Only static xref was the just-trapped
        // GetMessageProc itself, so this is also unreachable.
        hook::install_trap!(
            "Frontend::PumpModalOrSessionFrame",
            va::FRONTEND_PUMP_MODAL_OR_SESSION_FRAME
        );

        // Frontend::LaunchGameSession (0x004EC540) — full replacement.
        // 11 WA-side callers (frontend dialog handlers); the WA address
        // must remain callable, so install as a hook rather than a trap.
        crate::hook::install(
            "Frontend::LaunchGameSession",
            va::FRONTEND_LAUNCH_GAME_SESSION,
            openwa_game::wa::frontend::launch_game_session as *const (),
        )?;
    }

    Ok(())
}
