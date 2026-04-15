//! Rust ports of `GameSession__ProcessFrame` (0x572C80) and
//! `GameSession__AdvanceFrame` (0x56DDC0).
//!
//! Called every iteration of the main game loop in `GameSession__Run`.
//! `process_frame` handles desktop availability checks, keyboard state,
//! frame advance, render dispatch, and minimize requests.
//! `advance_frame` handles timer reads, accumulator updates, and
//! dispatches the frame timing to `DDGameWrapper__DispatchFrame`.

use crate::address::va;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_session::GameSession;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;

/// Game state values returned by `advance_frame` (via `DDGameWrapper::get_state_initialized`).
/// Not an enum because we don't know all variants — transmuting an unknown discriminant is UB.
pub mod game_state {
    /// Game is running normally.
    pub const RUNNING: u32 = 0;
    /// Headless exit condition.
    pub const EXIT_HEADLESS: u32 = 4;
    /// Normal exit condition.
    pub const EXIT: u32 = 5;
}

/// Rust port of `GameSession__AdvanceFrame` (0x56DDC0).
///
/// Reads the current time (via `GetTickCount` or `QueryPerformanceCounter`),
/// updates the timer accumulator, dispatches frame timing to
/// `DDGameWrapper__DispatchFrame` (0x529160), and returns the game state
/// from `DDGameWrapper::get_state_initialized`.
///
/// # Safety
/// Must be called from within the WA.exe process with a valid `g_GameSession`.
pub unsafe fn advance_frame() -> u32 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;

    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    let freq_lo = (*session).timer_freq_lo;
    let freq_hi = (*session).timer_freq_hi;
    let counter = ((*session).timer_counter_hi as u64) << 32 | (*session).timer_counter_lo as u64;

    let (time, new_counter, call_freq_lo, call_freq_hi);

    if freq_lo == 0 && freq_hi == 0 {
        // No QueryPerformanceCounter — use GetTickCount
        let tick = GetTickCount();
        new_counter = counter.wrapping_mul(2).wrapping_add((tick & 1) as u64);
        time = tick as u64 * 1000;
        call_freq_lo = 1_000_000u32;
        call_freq_hi = 0u32;
    } else {
        // Use QueryPerformanceCounter
        let mut qpc: i64 = 0;
        windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut qpc);
        new_counter = counter.wrapping_mul(4).wrapping_add((qpc as u64) & 3);
        time = qpc as u64;
        call_freq_lo = freq_lo;
        call_freq_hi = freq_hi;
    };

    // Store updated accumulator
    (*session).timer_counter_lo = new_counter as u32;
    (*session).timer_counter_hi = (new_counter >> 32) as u32;

    // Dispatch frame timing — stdcall(wrapper, time_lo, time_hi, freq_lo, freq_hi)
    let dispatch: unsafe extern "stdcall" fn(*mut DDGameWrapper, u32, u32, u32, u32) =
        core::mem::transmute(rb(va::DDGAMEWRAPPER_DISPATCH_FRAME) as usize);
    let wrapper = (*session).ddgame_wrapper;
    dispatch(
        wrapper,
        time as u32,
        (time >> 32) as u32,
        call_freq_lo,
        call_freq_hi,
    );

    // Return game state (vtable slot 9)
    DDGameWrapper::get_state_initialized_raw(wrapper)
}

/// Rust port of `GameSession__ProcessFrame` (0x572C80).
///
/// # Safety
/// Must be called from within the WA.exe process with a valid `g_GameSession`.
pub unsafe fn process_frame() {
    use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, OpenInputDesktop};
    use windows_sys::Win32::System::Threading::Sleep;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageA, PeekMessageA, TranslateMessage, PM_REMOVE,
    };

    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);

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
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);

    if state == game_state::EXIT {
        (*session).exit_flag = 1;
    }

    // Headless mode: only exit conditions matter
    if (*config_ptr).headless_mode != 0 {
        if state == game_state::EXIT_HEADLESS {
            (*session).exit_flag = 1;
        }
        return;
    }

    // Frame gating and render dispatch
    if (*session).flag_60 == 0 || (*session).flag_5c != 0 || (*session).exit_flag != 0 {
        if (*session).frame_state != 0 {
            Sleep(1);
            // Re-read session after Sleep
            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
            check_minimize(session);
            return;
        }
        (*session).frame_state = -1;
    }

    // Call DDGameWrapper::render_frame (vtable slot 7)
    let wrapper = (*session).ddgame_wrapper;
    DDGameWrapper::render_frame_raw(wrapper);

    // Re-read session after render
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    (*session).frame_state = 1;

    check_minimize(session);
}

/// Check and handle minimize request.
unsafe fn check_minimize(session: *mut GameSession) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageA, SC_MINIMIZE, WM_SYSCOMMAND};

    if (*session).minimize_request != 0 {
        (*session).minimize_request = 0;
        let hwnd = *(rb(va::G_FRONTEND_HWND) as *const *mut core::ffi::c_void);
        PostMessageA(hwnd, WM_SYSCOMMAND, SC_MINIMIZE as usize, 0);
    }
}
