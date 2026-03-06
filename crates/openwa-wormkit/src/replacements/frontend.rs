//! FrontendChangeScreen replacement.
//!
//! Original: 0x447A20, stdcall(screen_id), ESI = dialog this (__usercall).
//! Navigates between frontend menu screens via MFC CDialog::EndDialog.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::log_line;
use openwa_lib::wa::mfc::{CDialogHandle, CWndHandle};
use openwa_lib::wa_call;
use openwa_types::address::va;
use openwa_types::frontend::ScreenId;

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
    let g_frontend_frame = wa_call::read_global(va::G_FRONTEND_FRAME);

    if g_frontend_frame == 0 {
        // Init path: store screen_id, clear flag bit 0x10
        let flags = *((dialog + 0x3c) as *const u32);
        if (flags & 0x18) != 0 {
            *((dialog + 0x44) as *mut u32) = screen_id;
            *((dialog + 0x3c) as *mut u32) = flags & 0xFFFF_FFEF;
        }
    } else {
        // Normal path: full screen transition
        let wnd = CWndHandle(dialog);
        let dlg = CDialogHandle(dialog);

        wnd.enable_window(false);

        // Frontend__PaletteAnimation: EAX = [dialog+0x12c], param = [dialog+0x134]
        let palette_param = *((dialog + 0x134) as *const u32);
        let eax_value = *((dialog + 0x12c) as *const u32);
        openwa_lib::wa::frontend::palette_animation(eax_value, palette_param);

        // Virtual calls: vtable[0x15C](1) then vtable[0x15C](2)
        for i in 1u32..=2 {
            let vtable = *(dialog as *const u32);
            wa_call::thiscall_indirect_1(vtable + 0x15C, dialog, i);
        }

        wnd.enable_window(true);
        dlg.end_dialog(screen_id);
    }

    // Log the transition
    let name = ScreenId::try_from(screen_id as i32)
        .map(|s| format!("{s:?}"))
        .unwrap_or_else(|v| format!("Unknown({v})"));
    let _ = log_line(&format!("[FrontendChangeScreen] screen_id={screen_id} ({name})"));
}

pub fn install() -> Result<(), String> {
    unsafe {
        let trampoline_ptr = crate::hook::install(
            "FrontendChangeScreen",
            va::FRONTEND_CHANGE_SCREEN,
            trampoline as *const (),
        )?;
        ORIG_FRONTEND_CHANGE_SCREEN.store(trampoline_ptr as u32, Ordering::Relaxed);
    }

    Ok(())
}
