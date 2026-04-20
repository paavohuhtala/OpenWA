//! Ring buffer of `dispatch_frame` interp/accum snapshots.
//!
//! Captured on every dispatch to aid debugging render-interpolation
//! stutter. Read by the debug UI to draw live plots. Thread-safe
//! (written from the game thread, read from the debug UI thread).

use std::collections::VecDeque;
use std::sync::Mutex;

/// Number of dispatch samples retained. Sized for high-refresh-rate
/// monitors: at 240 Hz that's ~10 seconds of history; at 60 Hz it's
/// ~40 seconds. Enough to see multi-second oscillation patterns.
pub const HISTORY_LEN: usize = 2400;

/// One dispatch_frame's post-exit state snapshot.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterpSample {
    /// `dispatch_frame` call index (monotonic counter, not sim frame).
    pub dispatch_index: u64,
    /// Simulation frame counter at dispatch exit.
    pub frame_counter: i32,
    /// `DDGame::render_interp_a` raw 16.16 value at exit.
    pub interp_a_raw: i32,
    /// `DDGame::render_interp_b` raw 16.16 value at exit.
    pub interp_b_raw: i32,
    /// `DDGameWrapper::frame_accum_a` (u64 QPC ticks) at exit.
    pub accum_a: u64,
    /// `DDGameWrapper::frame_accum_b` at exit.
    pub accum_b: u64,
    /// `DDGameWrapper::frame_accum_c` at exit.
    pub accum_c: u64,
    /// `DDGameWrapper::frame_delay_counter` at exit.
    pub frame_delay_counter: i32,
}

static HISTORY: Mutex<VecDeque<InterpSample>> = Mutex::new(VecDeque::new());

/// Total number of dispatches observed since DLL load. Used as the
/// monotonic x-axis for time-series plots.
static DISPATCH_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Record one dispatch_frame exit snapshot. Oldest sample is dropped
/// when the buffer reaches [`HISTORY_LEN`].
pub fn push(mut sample: InterpSample) {
    sample.dispatch_index = DISPATCH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut guard) = HISTORY.lock() {
        if guard.len() >= HISTORY_LEN {
            guard.pop_front();
        }
        guard.push_back(sample);
    }
}

/// Copy the current history buffer. Returns oldest-first.
pub fn snapshot() -> Vec<InterpSample> {
    HISTORY
        .lock()
        .map(|g| g.iter().copied().collect())
        .unwrap_or_default()
}

/// Clear the ring buffer (e.g. on user request from debug UI).
pub fn clear() {
    if let Ok(mut guard) = HISTORY.lock() {
        guard.clear();
    }
}
