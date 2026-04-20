//! Ring buffer of `dispatch_frame` interp/accum snapshots.
//!
//! Captured on every dispatch to aid debugging render-interpolation
//! stutter. Read by the debug UI to draw live plots. Thread-safe
//! (written from the game thread, read from the debug UI thread).

use std::collections::VecDeque;
use std::sync::Mutex;

/// Number of dispatch samples retained. At ~60 Hz render rate that's
/// roughly five seconds of history — enough to see multi-second
/// oscillation patterns.
pub const HISTORY_LEN: usize = 300;

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
    /// True if this dispatch was routed through vanilla 0x529160
    /// (`OPENWA_DISPATCH_ORIGINAL=1`).
    pub via_original: bool,
    /// Per-branch counters accumulated across all `should_interpolate`
    /// invocations during this dispatch. Index meaning:
    ///   0 — phase ∈ {1,2,6,7,9} (→ interpolate true)
    ///   1 — fade_request != 0 (→ interpolate true)
    ///   2 — online bridge
    ///   3 — _field_434 != 0 (→ interpolate true)
    ///   4 — flag_5c != 0 (→ interpolate true)
    ///   5 — all-three offline gates (→ interpolate true)
    ///   6 — offline bridge (fell all the way through)
    pub path_hits: [u16; 7],
    /// The value should_interpolate returned on its last invocation
    /// during this dispatch. `true` = interpolate, `false` = paused.
    pub last_result: bool,
    /// How many times offline-bridge AL byte was zero this dispatch.
    pub offline_zero: u16,
    /// How many times offline-bridge AL byte was nonzero this dispatch.
    pub offline_nonzero: u16,
    /// Last raw EAX value returned by the offline bridge (dirty upper bits).
    pub offline_last_raw: u32,
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
