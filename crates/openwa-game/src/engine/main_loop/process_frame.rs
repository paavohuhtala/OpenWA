//! Rust ports of `GameSession__ProcessFrame` (0x572C80) and
//! `GameSession__AdvanceFrame` (0x56DDC0).
//!
//! Called every iteration of the main game loop in `GameSession__Run`.
//! `process_frame` handles desktop availability checks, keyboard state,
//! frame advance, render dispatch, and minimize requests.
//! `advance_frame` handles timer reads, accumulator updates, and
//! dispatches the frame timing to `GameRuntime__DispatchFrame`.

use super::dispatch_frame::dispatch_frame;
use crate::address::va;
use crate::engine::clock::{effective_timer_freq, read_current_time};
use crate::engine::runtime::GameRuntime;
use crate::engine::game_session::{GameSession, get_game_session};
use crate::engine::game_state;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;

/// Rust port of `GameSession__AdvanceFrame` (0x56DDC0).
///
/// Reads the current timer, stirs a few bits of it into
/// `GameSession::timer_counter` (an entropy accumulator used
/// elsewhere), dispatches frame timing to
/// `GameRuntime::DispatchFrame` (0x529160), and returns the game
/// state from `GameRuntime::get_game_state`.
///
/// # Safety
/// Must be called from within the WA.exe process with a valid `g_GameSession`.
pub unsafe fn advance_frame() -> u32 {
    unsafe {
        let session = get_game_session();
        let freq = (*session).timer_freq;
        let time = read_current_time();

        // Per-path counter stir: QPC branch mixes 2 bits, GetTickCount
        // branch mixes 1. `time` already reflects the chosen source.
        (*session).timer_counter = if freq == 0 {
            (*session)
                .timer_counter
                .wrapping_mul(2)
                .wrapping_add(time & 1)
        } else {
            (*session)
                .timer_counter
                .wrapping_mul(4)
                .wrapping_add(time & 3)
        };

        let runtime = (*session).game_runtime;
        dispatch_frame(runtime, time, effective_timer_freq());

        // Return game state (vtable slot 9)
        GameRuntime::get_game_state_raw(runtime)
    }
}

/// Rust port of `GameSession__ProcessFrame` (0x572C80).
///
/// # Safety
/// Must be called from within the WA.exe process with a valid `g_GameSession`.
pub unsafe fn process_frame() {
    unsafe {
        use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop};
        use windows_sys::Win32::System::Threading::Sleep;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageA, PM_REMOVE, PeekMessageA, TranslateMessage,
        };

        let session = get_game_session();

        // ── Desktop availability check (only when g_DesktopCheckLevel > 1) ──
        let desktop_check_level = *(rb(va::G_DESKTOP_CHECK_LEVEL) as *const u32);
        if desktop_check_level > 1 {
            let desktop = OpenInputDesktop(0, 0, 0x100); // DESKTOP_SWITCHDESKTOP
            if desktop.is_null() {
                // Desktop unavailable (e.g. locked screen)
                if (*session).desktop_lost == 0 {
                    (*session).desktop_lost = 1;
                    (*session).input_active_flag = 0;
                    let keyboard = (*session).keyboard;
                    if !keyboard.is_null() {
                        (*keyboard).clear_key_states();
                    }
                }
            } else {
                CloseDesktop(desktop);
                if (*session).desktop_lost != 0 {
                    (*session).desktop_lost = 0;
                    let keyboard = (*session).keyboard;
                    if !keyboard.is_null() {
                        // Drain pending keyboard messages (WM_KEYFIRST..WM_KEYLAST = 0x100..0x109)
                        let mut msg = core::mem::zeroed();
                        while PeekMessageA(&mut msg, core::ptr::null_mut(), 0x100, 0x109, PM_REMOVE)
                            != 0
                        {
                            TranslateMessage(&msg);
                            DispatchMessageA(&msg);
                        }
                        (*keyboard).poll();
                    }
                }
            }
        }

        // ── Dispatch based on config_ptr presence ──
        let config_ptr = (*session).config_ptr;
        if config_ptr.is_null() {
            // No game info — non-engine path (frontend transition?)
            if (*session).flag_34 != 0 {
                // Tail-call display flush_render (vtable slot 26)
                let display = (*session).display as *mut DisplayGfx;
                DisplayGfx::flush_render_raw(display);
            }
            return;
        }

        // ── Engine frame path ──
        let state = advance_frame();

        // Re-read session after advance_frame (it may have been modified)
        let session = get_game_session();

        if state == game_state::EXIT {
            (*session).exit_flag = 1;
        }

        // Headless mode: only exit conditions matter
        if (*config_ptr).headless_mode != 0 {
            if state == game_state::ROUND_ENDING {
                (*session).exit_flag = 1;
            }
            return;
        }

        // Frame gating and render dispatch
        if (*session).flag_60 == 0 || (*session).flag_5c != 0 || (*session).exit_flag != 0 {
            if (*session).frame_state != 0 {
                Sleep(1);
                // Re-read session after Sleep
                let session = get_game_session();
                check_minimize(session);
                return;
            }
            (*session).frame_state = -1;
        }

        // Call GameRuntime::render_frame (vtable slot 7)
        let runtime = (*session).game_runtime;
        GameRuntime::render_frame_raw(runtime);

        // Re-read session after render
        let session = get_game_session();
        (*session).frame_state = 1;

        check_minimize(session);
    }
}

/// Check and handle minimize request.
unsafe fn check_minimize(session: *mut GameSession) {
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            PostMessageA, SC_MINIMIZE, WM_SYSCOMMAND,
        };

        if (*session).minimize_request != 0 {
            (*session).minimize_request = 0;
            let hwnd = *(rb(va::G_FRONTEND_HWND) as *const *mut core::ffi::c_void);
            PostMessageA(hwnd, WM_SYSCOMMAND, SC_MINIMIZE as usize, 0);
        }
    }
}
