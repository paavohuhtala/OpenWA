//! Rust port of `GameSession::Run` (0x00572F50).
//!
//! Top-level main-loop driver. Allocates the `GameSession`, runs hardware
//! init, pumps Win32 messages, drives `ProcessFrame` until exit, then runs
//! shutdown. Convention is `__usercall(ESI=GameInfo, stack: arg1..arg4)`,
//! `RET 0x10`.
//!
//! Sub-callees still bridged to WA:
//!  - `GameSession::Constructor` (0x0058BFA0, usercall EAX=this)
//!  - `GameSession::PumpMessages` (0x00572E30, cdecl)
//!  - `GameSession::OnHeadlessPreLoop_Maybe` (0x00572430, stdcall 1 arg)
//!  - `FrontendDialog::UpdateCursor` (0x0040D250, stdcall 1 arg)
//!  - `GameEngine::Shutdown` (0x0056DCD0, stdcall 1 arg)
//!
//! `GameEngine::InitHardware` (0x0056D350) and `GameSession::ProcessFrame`
//! (0x00572C80) are already replaced in Rust — calls go through the hooked
//! WA address (and resolve to the Rust impl via the trampoline).

use core::mem::transmute;

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::engine::game_session::GameSession;
use crate::engine::main_loop::process_frame::process_frame;
use crate::rebase::rb;
use crate::wa_alloc::wa_malloc_struct_zeroed;

// ─── Bridges ─────────────────────────────────────────────────────────────────

/// Bridge to `GameSession::Constructor` — `__usercall(EAX=this)`, plain RET.
/// Returns `this` in EAX.
#[unsafe(naked)]
unsafe extern "C" fn call_session_ctor(_this: *mut GameSession, _target: u32) -> *mut GameSession {
    core::arch::naked_asm!(
        // [ret@0] [this@4] [target@8]
        "movl 4(%esp), %eax",
        "movl 8(%esp), %ecx",
        "calll *%ecx",
        "retl",
        options(att_syntax),
    );
}

/// Drain pending non-`WM_CHAR` messages (`while wParam != 0`).
unsafe fn drain_pending_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageA, MSG, PM_REMOVE, PeekMessageA, TranslateMessage, WM_CHAR,
    };
    unsafe {
        let mut msg: MSG = core::mem::zeroed();
        loop {
            if PeekMessageA(&mut msg, core::ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
                return;
            }
            if msg.message != WM_CHAR {
                TranslateMessage(&msg);
                DispatchMessageA(&msg);
                return;
            }
            if msg.wParam == 0 {
                return;
            }
        }
    }
}

/// Call the GameSession destructor (vtable slot 0) with `flags=1` so the
/// scalar deleting destructor frees the heap allocation.
unsafe fn delete_session(session: *mut GameSession) {
    unsafe {
        let vtable = (*session).vtable as *const u32;
        let dtor = *vtable;
        let f: unsafe extern "thiscall" fn(*mut GameSession, u32) = transmute(dtor);
        f(session, 1);
    }
}

// ─── Implementation ──────────────────────────────────────────────────────────

/// Rust port of `GameSession::Run` (0x00572F50). Returns 1 on a normal
/// shutdown, 0 if `GameEngine::InitHardware` failed.
pub unsafe fn run_game_session(
    game_info: *mut GameInfo,
    arg1_module_state: u32,
    state_buf: *mut u8,
    display_p3: u32,
    display_p4: u32,
) -> u32 {
    use windows_sys::Win32::Graphics::Gdi::ValidateRect;
    use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let h_wnd = *(rb(va::G_FRONTEND_HWND) as *const *mut core::ffi::c_void);
        timeBeginPeriod(1);

        let headless = (*game_info).headless_mode;
        if headless == 0 {
            ValidateRect(h_wnd, core::ptr::null());
        }

        // ── Allocate + construct GameSession ────────────────────────────────
        let session = wa_malloc_struct_zeroed::<GameSession>();
        if !session.is_null() {
            call_session_ctor(
                session as *mut GameSession,
                rb(va::GAME_SESSION_CONSTRUCTOR),
            );
        }

        (*session).hwnd = h_wnd as u32;
        (*session).run_param_1 = arg1_module_state;
        (*session).display_param_1 = (*game_info)._field_f39c;
        *(rb(va::G_GAME_SESSION) as *mut *mut GameSession) = session;
        // WA reads `GameInfo + 0xF3A0` as a u32 — that's a bulk copy of four
        // adjacent typed byte fields (`_config_byte_f3a0`, `detail_level`,
        // `energy_bar`, `info_transparency`) packed in declaration order.
        (*session).display_param_2 = u32::from_ne_bytes([
            (*game_info)._config_byte_f3a0,
            (*game_info).detail_level,
            (*game_info).energy_bar,
            (*game_info).info_transparency,
        ]);
        GetCursorPos(&mut (*session).cursor_initial);
        (*session).flag_2c = 1;

        // ── Optional pre-init message drain (UI / cursor housekeeping) ──────
        let mut do_pre_init_drain = false;
        if headless == 0 {
            if *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8) == 0 {
                let update_cursor: unsafe extern "stdcall" fn(u32) =
                    transmute(rb(va::FRONTEND_DIALOG_UPDATE_CURSOR));
                update_cursor(rb(va::G_INGAME_FRONTEND_DIALOG));
            }
            do_pre_init_drain = (*game_info).headless_mode == 0;
        }
        if do_pre_init_drain {
            // WA explicitly re-zeroes these two slots here; the constructor
            // also zeroes them, so this is redundant defensive code — kept
            // for fidelity.
            (*session)._field_078 = 0;
            (*session)._field_07c = 0;
            drain_pending_messages();
        }

        // ── Hardware init (replaced in Rust; goes through hook trampoline) ──
        let init_hw: unsafe extern "thiscall" fn(*mut GameInfo, u32, u32, u32) -> u32 =
            transmute(rb(va::GAME_ENGINE_INIT_HARDWARE));
        let ok = init_hw(game_info, h_wnd as u32, display_p3, display_p4);

        if ok == 0 {
            // Init failure path — tear down what we have and return 0.
            timeEndPeriod(1);
            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
            if !session.is_null() {
                delete_session(session);
            }
            *(rb(va::G_GAME_SESSION) as *mut *mut GameSession) = core::ptr::null_mut();
            core::ptr::write_bytes(state_buf, 0, 0x1900);
            *(rb(va::G_IN_GAME_LOOP) as *mut u32) = 0;
            return 0;
        }

        // ── Headless display-mode hook (rare path) ──────────────────────────
        if *(rb(va::G_DISPLAY_MODE_FLAG) as *const u8) != 0 {
            let on_headless: unsafe extern "stdcall" fn(u32) =
                transmute(rb(va::GAME_SESSION_ON_HEADLESS_PRE_LOOP));
            on_headless(0);
        }

        // ── Second pre-loop message drain ───────────────────────────────────
        if (*game_info).headless_mode == 0 {
            drain_pending_messages();
        }

        // ── Main loop ───────────────────────────────────────────────────────
        let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
        (*session).config_ptr = game_info;

        let pump: unsafe extern "cdecl" fn() = transmute(rb(va::GAME_SESSION_PUMP_MESSAGES));
        #[allow(clippy::while_immutable_condition)]
        while (*session).exit_flag == 0 {
            pump();
            process_frame();
        }

        // ── Post-loop cleanup ───────────────────────────────────────────────
        *(rb(va::G_IN_GAME_LOOP) as *mut u32) = 0;
        (*session).config_ptr = core::ptr::null_mut();

        if (*session).flag_5c != 0 && (*session).flag_40 == 0 {
            let kb = (*session).keyboard;
            if !kb.is_null() {
                ((*(*kb).vtable).alert_user)(kb, 1, 2);
            }
        }

        // ── Engine shutdown ─────────────────────────────────────────────────
        let shutdown: unsafe extern "stdcall" fn(*mut u8) = transmute(rb(va::GAME_ENGINE_SHUTDOWN));
        shutdown(state_buf);

        let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
        if !session.is_null() {
            delete_session(session);
        }
        *(rb(va::G_GAME_SESSION) as *mut *mut GameSession) = core::ptr::null_mut();
        timeEndPeriod(1);
        1
    }
}
