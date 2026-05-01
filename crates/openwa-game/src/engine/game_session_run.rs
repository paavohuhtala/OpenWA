//! Rust port of `GameSession::Run` (0x00572F50) plus its in-process helpers.
//!
//! Top-level main-loop driver. Allocates the `GameSession`, runs hardware
//! init, pumps Win32 messages, drives `ProcessFrame` until exit, then runs
//! shutdown. Convention is `__usercall(ESI=GameInfo, stack: arg1..arg4)`,
//! `RET 0x10`.
//!
//! Sub-callees still bridged to WA:
//!  - `Frontend::UnhookInputHooks` (0x004ED590, cdecl) — invoked from
//!    `pump_messages`.
//!  - `FrontendDialog::UpdateCursor` (0x0040D250, stdcall 1 arg) — also
//!    invoked indirectly from `on_headless_pre_loop`.
//!  - `GameEngine::Shutdown` (0x0056DCD0, stdcall 1 arg)
//!

use core::mem::transmute;

use windows_sys::Win32::Foundation::HWND;

use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::engine::game_session::{GameSession, GameSessionVtable, get_game_session};
use crate::engine::hardware_init::{init_hardware, shutdown};
use crate::engine::main_loop::process_frame::process_frame;
use crate::engine::pump_messages::pump_messages;
use crate::rebase::rb;
use crate::render::display::context::{FastcallResult, RenderContext};
use crate::wa_alloc::wa_malloc_struct_zeroed;

// ─── Pure-Rust helpers ───────────────────────────────────────────────────────

/// Rust port of `GameSession::Constructor` (0x0058BFA0). Convention
/// is `__usercall(EAX=this)`. The original zeroes ~32 contiguous u32 fields
/// covering offsets 0x10..=0xC4; we rely on `wa_malloc_struct_zeroed` for
/// those, leaving only the three non-zero writes (vtable, screen-center
/// sentinel, gate flag).
pub unsafe fn construct_session(this: *mut GameSession) {
    unsafe {
        (*this).vtable = rb(va::GAME_SESSION_VTABLE) as *const GameSessionVtable;
        (*this).screen_center_x = i32::MIN;
        (*this).flag_60 = 1;
    }
}

/// Rust port of `GameSession::OnHeadlessPreLoop_Maybe` (0x00572430).
///
/// Called once before the main loop enters when `g_DisplayModeFlag != 0`,
/// and from two WA-side SYSCOMMAND minimize paths (`FUN_004ed701`,
/// `Unknown__OnSYSCOMMAND`). Idempotent — bails immediately when
/// `flag_5c` is already set.
///
/// Behaviour:
///  - clear `flag_2c` and `input_active_flag`
///  - zero the keyboard's current and previous key-state buffers
///  - if fullscreen, release the cursor clip
///  - reapply the in-game frontend cursor
///  - mark `flag_5c = 1` so this only runs once
///  - if `param_1 != 0`, minimize the frontend window
///  - kick the renderer's slot-8 thunk (full-screen flush / mode reset)
///  - if `flag_2c` had been set, restore the cursor to its session-start
///    position (`cursor_initial`)
pub unsafe extern "stdcall" fn on_headless_pre_loop(param_1: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        ClipCursor, SW_MINIMIZE, SetCursorPos, ShowWindow,
    };

    unsafe {
        let session = get_game_session();
        if (*session).flag_5c != 0 {
            return;
        }

        let saved_mouse_acquired = (*session).mouse_acquired;
        let keyboard = (*session).keyboard;
        (*session).mouse_acquired = 0;
        (*session).mouse_button_state = 0;
        // WA dereferences `keyboard` unconditionally; mirror that exactly.
        (*keyboard).clear_key_states();

        if *(rb(va::G_FULLSCREEN_FLAG) as *const u32) != 0 {
            ClipCursor(core::ptr::null());
        }

        let update_cursor: unsafe extern "stdcall" fn(u32) =
            transmute(rb(va::FRONTEND_DIALOG_UPDATE_CURSOR));
        update_cursor(rb(va::G_INGAME_FRONTEND_DIALOG));

        (*session).flag_5c = 1;

        if param_1 != 0 {
            let h_wnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);
            ShowWindow(h_wnd, SW_MINIMIZE);
        }

        let ctx = *(rb(va::G_RENDER_CONTEXT) as *const *mut RenderContext);
        let mut buf: FastcallResult = core::mem::zeroed();
        RenderContext::renderer_slot8_raw(ctx, &mut buf);

        if saved_mouse_acquired != 0 {
            SetCursorPos((*session).cursor_initial.x, (*session).cursor_initial.y);
        }
    }
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

/// Call the GameSession scalar deleting destructor with `flags=1` so it
/// frees the heap allocation after running the C++ destructor body.
unsafe fn delete_session(session: *mut GameSession) {
    unsafe {
        GameSession::destructor_raw(session, 1);
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
        let hwnd = *(rb(va::G_FRONTEND_HWND) as *const HWND);
        timeBeginPeriod(1);

        let headless = (*game_info).headless_mode;
        if headless == 0 {
            ValidateRect(hwnd, core::ptr::null());
        }

        // ── Allocate + construct GameSession ────────────────────────────────
        let session = wa_malloc_struct_zeroed::<GameSession>();
        if !session.is_null() {
            construct_session(session);
        }

        (*session).hwnd = hwnd;
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
        (*session).mouse_acquired = 1;

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
            (*session).mouse_delta_x = 0;
            (*session).mouse_delta_y = 0;
            drain_pending_messages();
        }

        // ── Hardware init (replaced in Rust; direct call) ──────────────────
        let ok = init_hardware(game_info, hwnd, display_p3, display_p4);

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
            on_headless_pre_loop(0);
        }

        // ── Second pre-loop message drain ───────────────────────────────────
        if (*game_info).headless_mode == 0 {
            drain_pending_messages();
        }

        // ── Main loop ───────────────────────────────────────────────────────
        let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
        (*session).config_ptr = game_info;

        #[allow(clippy::while_immutable_condition)]
        while (*session).exit_flag == 0 {
            pump_messages();
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

        // ── Engine shutdown (Rust port; WA address is trapped) ──────────────
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
