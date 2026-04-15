//! Rust port of `DDGameWrapper__DispatchFrame` (0x529160).
//!
//! The main frame timing/simulation dispatcher. Called each frame by
//! `advance_frame`. Calculates delta time, determines how many game frames
//! to advance, dispatches them via `StepFrame`, and handles post-frame
//! timing updates, headless log output, and game-end detection.
//!
//! ## Sub-function bridges
//!
//! All sub-functions use `usercall(EAX=this)` where `this` is `*mut DDGameWrapper`.
//! Bridges use naked asm to set EAX before calling the original WA function.

use crate::address::va;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_session::GameSession;
use crate::rebase::rb;

// ─── Bridge helpers ────────────────────────────────────────────────────────
//
// All sub-functions take DDGameWrapper* in EAX. For functions with additional
// stdcall stack params, we pop our cdecl `this` arg, move it to EAX, push
// the return address back, and JMP to the target (which does RET N to clean
// the remaining stack params).

/// Generate a naked bridge for a usercall(EAX=this) function with no stack params.
/// The target does plain RET.
macro_rules! bridge_eax_this {
    ($name:ident, $addr:expr, $ret:ty) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name(_this: *mut DDGameWrapper) -> $ret {
            core::arch::naked_asm!(
                "popl %ecx",           // pop return address
                "popl %eax",           // pop this → EAX
                "pushl %ecx",          // push return address back
                "jmpl *({fn})",        // tail-call; target does plain RET
                fn = sym $addr,
                options(att_syntax),
            );
        }
    };
}

/// Generate a naked bridge for a usercall(EAX=this) + N stdcall stack params.
/// The target does RET (N*4) to clean stack params. Our `this` is consumed
/// by the pop-to-EAX, so the target's RET N cleans exactly the right amount.
macro_rules! bridge_eax_this_stdcall {
    ($name:ident, $addr:expr, ($($param:ty),+) -> $ret:ty) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name(_this: *mut DDGameWrapper, $(_: $param),+) -> $ret {
            core::arch::naked_asm!(
                "popl %ecx",           // pop return address
                "popl %eax",           // pop this → EAX
                "pushl %ecx",          // push return address back
                "jmpl *({fn})",        // tail-call; target does RET N cleaning stack params
                fn = sym $addr,
                options(att_syntax),
            );
        }
    };
}

// ─── Runtime addresses (set in init_dispatch_addrs) ────────────────────────

static mut STEP_FRAME_ADDR: u32 = 0;
static mut SHOULD_CONTINUE_ADDR: u32 = 0;
static mut RESET_FRAME_STATE_ADDR: u32 = 0;
static mut UPDATE_FRAME_TIMING_ADDR: u32 = 0;
static mut ADVANCE_FRAME_COUNTERS_ADDR: u32 = 0;
static mut CALC_TIMING_RATIO_ADDR: u32 = 0;
static mut INIT_FRAME_DELAY_ADDR: u32 = 0;
static mut NETWORK_UPDATE_ADDR: u32 = 0;
static mut IS_FRAME_PAUSED_ADDR: u32 = 0;
static mut SETUP_FRAME_PARAMS_ADDR: u32 = 0;
static mut PROCESS_NETWORK_FRAME_ADDR: u32 = 0;
static mut WRITE_HEADLESS_LOG_ADDR: u32 = 0;

/// Initialize all bridge addresses. Must be called once at DLL load.
pub unsafe fn init_dispatch_addrs() {
    STEP_FRAME_ADDR = rb(va::DDGAMEWRAPPER_STEP_FRAME);
    SHOULD_CONTINUE_ADDR = rb(va::DDGAMEWRAPPER_SHOULD_CONTINUE);
    RESET_FRAME_STATE_ADDR = rb(va::DDGAMEWRAPPER_RESET_FRAME_STATE);
    UPDATE_FRAME_TIMING_ADDR = rb(va::DDGAMEWRAPPER_UPDATE_FRAME_TIMING);
    ADVANCE_FRAME_COUNTERS_ADDR = rb(va::DDGAMEWRAPPER_ADVANCE_FRAME_COUNTERS);
    CALC_TIMING_RATIO_ADDR = rb(va::DDGAMEWRAPPER_CALC_TIMING_RATIO);
    INIT_FRAME_DELAY_ADDR = rb(va::DDGAMEWRAPPER_INIT_FRAME_DELAY);
    NETWORK_UPDATE_ADDR = rb(va::DDGAMEWRAPPER_NETWORK_UPDATE);
    IS_FRAME_PAUSED_ADDR = rb(va::DDGAMEWRAPPER_IS_FRAME_PAUSED);
    SETUP_FRAME_PARAMS_ADDR = rb(va::DDGAMEWRAPPER_SETUP_FRAME_PARAMS);
    PROCESS_NETWORK_FRAME_ADDR = rb(va::DDGAMEWRAPPER_PROCESS_NETWORK_FRAME);
    WRITE_HEADLESS_LOG_ADDR = rb(va::DDGAMEWRAPPER_WRITE_HEADLESS_LOG);
    IS_REPLAY_MODE_ADDR = rb(va::DDGAMEWRAPPER_IS_REPLAY_MODE);
}

// ─── Bridge function declarations ──────────────────────────────────────────

// usercall(EAX=this), no stack params, plain RET
bridge_eax_this!(bridge_is_replay_mode, IS_REPLAY_MODE_ADDR, u32);
bridge_eax_this!(bridge_reset_frame_state, RESET_FRAME_STATE_ADDR, ());
bridge_eax_this!(bridge_is_frame_paused, IS_FRAME_PAUSED_ADDR, u32);
bridge_eax_this!(bridge_init_frame_delay, INIT_FRAME_DELAY_ADDR, ());
bridge_eax_this!(bridge_network_update, NETWORK_UPDATE_ADDR, ());

// usercall(EAX=this) + stdcall stack params
// CalcTimingRatio: 1 stack param, RET 0x4
bridge_eax_this_stdcall!(bridge_calc_timing_ratio, CALC_TIMING_RATIO_ADDR, (i32) -> ());
// SetupFrameParams: 3 stack params, RET 0xC
bridge_eax_this_stdcall!(bridge_setup_frame_params, SETUP_FRAME_PARAMS_ADDR, (i32, i32, i32) -> ());
// AdvanceFrameCounters: 5 stack params, RET 0x14
bridge_eax_this_stdcall!(bridge_advance_frame_counters, ADVANCE_FRAME_COUNTERS_ADDR,
    (i32, i32, i32, i32, u32) -> ());
// UpdateFrameTiming: 4 stack params, RET 0x10
bridge_eax_this_stdcall!(bridge_update_frame_timing, UPDATE_FRAME_TIMING_ADDR,
    (u32, u32, u32, u32) -> ());
// StepFrame: 5 stack params, RET 0x14 — returns bool in AL
bridge_eax_this_stdcall!(bridge_step_frame, STEP_FRAME_ADDR,
    (*mut u64, u32, u32, i32, i32) -> u32);
// ShouldContinueFrameLoop: 2 stack params — but actually 3 (wrapper, lo, hi)?
// Let me check: the decompiled shows `FUN_0052a840(param_1, LStack_40.s.LowPart, LStack_40.s.HighPart)`
// param_1 is already in EAX for usercall, so 2 stack params
bridge_eax_this_stdcall!(bridge_should_continue, SHOULD_CONTINUE_ADDR,
    (u32, u32) -> u32);
// ProcessNetworkFrame: 4 stack params, RET 0x10
bridge_eax_this_stdcall!(bridge_process_network_frame, PROCESS_NETWORK_FRAME_ADDR,
    (u32, u32, u32, u32) -> ());
// WriteHeadlessLog: 2 stack params, RET 0x8
bridge_eax_this_stdcall!(bridge_write_headless_log, WRITE_HEADLESS_LOG_ADDR,
    (u32, u32) -> ());

// IsReplayMode needs its own static since it's not in the batch above
static mut IS_REPLAY_MODE_ADDR: u32 = 0;

// ─── Public bridge wrappers (safe-ish typed API) ───────────────────────────

/// Check if the game is in replay mode with certain state conditions.
pub unsafe fn is_replay_mode(wrapper: *mut DDGameWrapper) -> bool {
    bridge_is_replay_mode(wrapper) != 0
}

/// Check if the frame loop should continue processing more frames.
pub unsafe fn should_continue_frame_loop(
    wrapper: *mut DDGameWrapper,
    elapsed_lo: u32,
    elapsed_hi: u32,
) -> bool {
    bridge_should_continue(wrapper, elapsed_lo, elapsed_hi) != 0
}

/// Check if the current frame is paused.
pub unsafe fn is_frame_paused(wrapper: *mut DDGameWrapper) -> bool {
    bridge_is_frame_paused(wrapper) != 0
}

/// Process a single game frame step. Returns true if more frames should be processed.
pub unsafe fn step_frame(
    wrapper: *mut DDGameWrapper,
    remaining: *mut u64,
    frame_duration_lo: u32,
    frame_duration_hi: u32,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    bridge_step_frame(
        wrapper,
        remaining,
        frame_duration_lo,
        frame_duration_hi,
        game_speed_target,
        game_speed,
    ) != 0
}

// ─── Main dispatch function ────────────────────────────────────────────────

// TODO: Port DDGameWrapper__DispatchFrame (0x529160) here.
// For now, the original WA function is called directly from advance_frame
// via the DDGAMEWRAPPER_DISPATCH_FRAME address.
