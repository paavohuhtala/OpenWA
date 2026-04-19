//! Timer helpers for the main frame loop.
//!
//! `GameSession` stores a `QueryPerformanceFrequency` result in
//! `timer_freq` and uses it to pick between two clocks:
//!
//! - `timer_freq != 0` — standard path: `QueryPerformanceCounter`
//!   values, `freq` ticks per second.
//! - `timer_freq == 0` — synthetic-clock path: `GetTickCount` scaled to
//!   1 MHz. WA deliberately zeroes `timer_freq` in headless and
//!   deterministic-replay modes; the `/getlog` suite depends on it.
//!
//! Both paths have to stay in lockstep — reading the time from one
//! source while passing the other's frequency to `dispatch_frame`
//! makes `frame_interval = freq / 50` zero, wedging the frame loop
//! at 100 % CPU.

use windows_sys::Win32::System::{
    Performance::QueryPerformanceCounter, SystemInformation::GetTickCount,
};

use crate::engine::game_session::get_game_session;

/// Current timer value in the same units `GameSession::timer_freq`
/// measures. Branches per the module doc.
pub unsafe fn read_current_time() -> u64 {
    unsafe {
        let session = get_game_session();
        if (*session).timer_freq == 0 {
            let tick = GetTickCount();
            (tick as u64).wrapping_mul(1000)
        } else {
            let mut qpc: i64 = 0;
            QueryPerformanceCounter(&mut qpc);
            qpc as u64
        }
    }
}

/// Frequency (ticks per second) corresponding to [`read_current_time`] —
/// `GameSession::timer_freq` when non-zero, otherwise the 1 MHz
/// `GetTickCount` scale.
pub unsafe fn effective_timer_freq() -> u64 {
    unsafe {
        let freq = (*get_game_session()).timer_freq;
        if freq == 0 { 1_000_000 } else { freq }
    }
}
