//! FrontendChangeScreen replacement.
//!
//! Original: 0x447A20, stdcall(screen_id), ESI = dialog this (__usercall).
//! Navigates between frontend menu screens via MFC CDialog::EndDialog.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::hook;
use crate::log_line;
use openwa_game::address::va;
use openwa_game::game::ScreenId;
use openwa_game::wa::mfc::{CDialogHandle, CWndHandle};
use openwa_game::wa_call;

// Frontend dialog struct offsets (MFC CDialog-derived)
const DIALOG_FLAGS: usize = 0x3C;
const DIALOG_SCREEN_ID: usize = 0x44;
const DIALOG_PALETTE_OBJ: usize = 0x12C;
const DIALOG_PALETTE_PARAM: usize = 0x134;
const VTABLE_TRANSITION_METHOD: usize = 0x15C;
const FLAG_INIT_BITS: u32 = 0x18;
const FLAG_CLEAR_BIT: u32 = 0x10;

/// Trampoline to the original FrontendChangeScreen (for fallback if needed).
static ORIG_FRONTEND_CHANGE_SCREEN: AtomicU32 = AtomicU32::new(0);

// Naked trampoline: captures ESI (dialog this, __usercall) + 1 stack arg (screen_id).
crate::hook::usercall_trampoline!(fn trampoline;
    impl_fn = frontend_change_screen_impl; reg = esi;
    stack_params = 1; ret_bytes = "0x4");

/// Rust reimplementation of FrontendChangeScreen.
///
/// Two code paths based on g_FrontendFrame:
/// - If 0 (initializing): store screen_id in dialog fields
/// - If nonzero (normal): disable window → palette anim → vtable calls → EndDialog
unsafe extern "cdecl" fn frontend_change_screen_impl(dialog: u32, screen_id: u32) {
    unsafe {
        let g_frontend_frame = wa_call::read_global(va::G_FRONTEND_FRAME);

        if g_frontend_frame == 0 {
            // Init path: store screen_id, clear flag bit
            let flags = *((dialog as usize + DIALOG_FLAGS) as *const u32);
            if (flags & FLAG_INIT_BITS) != 0 {
                *((dialog as usize + DIALOG_SCREEN_ID) as *mut u32) = screen_id;
                *((dialog as usize + DIALOG_FLAGS) as *mut u32) = flags & !FLAG_CLEAR_BIT;
            }
        } else {
            // Normal path: full screen transition
            let wnd = CWndHandle(dialog);
            let dlg = CDialogHandle(dialog);

            wnd.enable_window(false);

            let palette_param = *((dialog as usize + DIALOG_PALETTE_PARAM) as *const u32);
            let eax_value = *((dialog as usize + DIALOG_PALETTE_OBJ) as *const u32);
            openwa_game::wa::frontend::palette_animation(eax_value, palette_param);

            for i in 1u32..=2 {
                let vtable = *(dialog as *const u32);
                wa_call::thiscall_indirect_1(vtable + VTABLE_TRANSITION_METHOD as u32, dialog, i);
            }

            wnd.enable_window(true);
            dlg.end_dialog(screen_id);
        }

        // Log the transition
        let name = ScreenId::try_from(screen_id as i32)
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|v| format!("Unknown({v})"));
        let _ = log_line(&format!(
            "[FrontendChangeScreen] screen_id={screen_id} ({name})"
        ));
    }
}

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline_ptr = crate::hook::install(
            "FrontendChangeScreen",
            va::FRONTEND_CHANGE_SCREEN,
            trampoline as *const (),
        )?;
        ORIG_FRONTEND_CHANGE_SCREEN.store(trampoline_ptr as u32, Ordering::Relaxed);

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
