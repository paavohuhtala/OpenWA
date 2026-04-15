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
        unsafe extern "stdcall" fn $name(_this: *mut DDGameWrapper) -> $ret {
            core::arch::naked_asm!(
                "popl %ecx",           // pop return address
                "popl %eax",           // pop this → EAX (consumed from stack)
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
/// by the pop-to-EAX (4 bytes), and the target's RET N cleans the rest —
/// total cleaned matches what stdcall expects.
macro_rules! bridge_eax_this_stdcall {
    ($name:ident, $addr:expr, ($($param:ty),+) -> $ret:ty) => {
        #[unsafe(naked)]
        unsafe extern "stdcall" fn $name(_this: *mut DDGameWrapper, $(_: $param),+) -> $ret {
            core::arch::naked_asm!(
                "popl %ecx",           // pop return address
                "popl %eax",           // pop this → EAX (consumed from stack)
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

// ─── Helper: read current time ─────────────────────────────────────────────

/// Read the current time using the same method as the GameSession timer.
/// Returns (time_lo, time_hi) matching the convention of AdvanceFrame's params.
unsafe fn read_current_time() -> (u32, u32) {
    let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
    if (*session).timer_freq_lo == 0 && (*session).timer_freq_hi == 0 {
        let tick = windows_sys::Win32::System::SystemInformation::GetTickCount();
        (tick.wrapping_mul(1000), 0)
    } else {
        let mut qpc: i64 = 0;
        windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut qpc);
        (qpc as u32, (qpc >> 32) as u32)
    }
}

/// Combine two u32 halves into a u64.
#[inline(always)]
fn combine(lo: u32, hi: u32) -> u64 {
    (hi as u64) << 32 | lo as u64
}

/// Split a u64 into (lo, hi) u32 halves.
#[inline(always)]
fn split(v: u64) -> (u32, u32) {
    (v as u32, (v >> 32) as u32)
}

/// Signed 64-bit subtraction of two timestamp pairs: (a_lo, a_hi) - (b_lo, b_hi).
#[inline(always)]
fn time_sub(a_lo: u32, a_hi: u32, b_lo: u32, b_hi: u32) -> u64 {
    combine(a_lo, a_hi).wrapping_sub(combine(b_lo, b_hi))
}

// ─── Main dispatch function ────────────────────────────────────────────────

/// Rust port of `DDGameWrapper__DispatchFrame` (0x529160).
///
/// Main frame timing and simulation dispatcher. Computes delta time,
/// determines how many game frames to advance, dispatches them via
/// `StepFrame`, and handles post-frame timing updates, headless log
/// output, and game-end detection.
///
/// # Parameters
/// - `wrapper`: `*mut DDGameWrapper` (param_1 in decompile)
/// - `time_lo`, `time_hi`: current timestamp from AdvanceFrame
/// - `freq_lo`, `freq_hi`: timer frequency from GameSession
///
/// # Safety
/// Must be called from within WA.exe with valid pointers.
pub unsafe fn dispatch_frame(
    wrapper: *mut DDGameWrapper,
    time_lo: u32,
    time_hi: u32,
    freq_lo: u32,
    freq_hi: u32,
) {
    // TEMP: passthrough while debugging
    let orig: unsafe extern "stdcall" fn(*mut DDGameWrapper, u32, u32, u32, u32) =
        core::mem::transmute(rb(va::DDGAMEWRAPPER_DISPATCH_FRAME) as usize);
    orig(wrapper, time_lo, time_hi, freq_lo, freq_hi);
    return;

    use crate::engine::ddgame::DDGame;

    let freq = combine(freq_lo, freq_hi);
    let time = combine(time_lo, time_hi);

    // ── Section 1: Compute frame timing constants ──────────────────────
    //
    // frame_interval = freq / 50 (ticks per frame at 50 fps)
    let frame_interval = freq / 50;

    let ddgame = (*wrapper).ddgame;
    let game_speed_target = (*ddgame).game_speed_target.to_raw(); // Fixed 16.16
    let game_speed = (*ddgame).game_speed.to_raw();

    // frame_duration = game_speed * frame_interval / game_speed_target
    // This is the actual ticks per frame adjusted for game speed.
    let frame_duration = ((game_speed as i64).wrapping_mul(frame_interval as i64)
        / (game_speed_target as i64)) as u64;

    let saved_frame_delay = (*wrapper).frame_delay_counter;
    let saved_game_speed = game_speed;

    // ── Section 2: Network/replay timing adjustments ───────────────────
    //
    // Only runs when sound/network is available (ddgame+0x7ef8 != 0)
    let has_sound = (*ddgame).sound_available != 0;
    let mut elapsed: u64 = 0;
    // bVar19 in the decompile — tracks whether we took the "normal" timing path
    let mut used_normal_path: bool = true;

    if has_sound {
        // Initialize pause detection timestamps on first call
        if (*wrapper).pause_detect_lo == 0 && (*wrapper).pause_detect_hi == 0 {
            (*wrapper).pause_detect_lo = time_lo;
            (*wrapper).pause_detect_hi = time_hi;
            (*wrapper).pause_secondary_lo = time_lo;
            (*wrapper).pause_secondary_hi = time_hi;
        }

        let is_replay = is_replay_mode(wrapper);

        if !is_replay || saved_frame_delay >= 0 {
            // Normal timing path
            let delta = time_sub(
                time_lo,
                time_hi,
                (*wrapper).pause_detect_lo,
                (*wrapper).pause_detect_hi,
            );
            used_normal_path = true;

            let quarter_freq = freq / 4;
            if (delta as i64) >= 0 && delta <= quarter_freq {
                // Delta is within reasonable bounds
                if game_speed_target
                    == *(((*ddgame).game_info as *const u8).add(0xd988) as *const i32)
                {
                    // Speed hasn't changed — use frame_interval directly
                    calc_timing_and_adjust_pause(
                        wrapper,
                        frame_interval,
                        time_lo,
                        time_hi,
                        delta,
                        &mut used_normal_path,
                    );
                } else {
                    // Speed changed — use frame_duration
                    let ratio = (delta as i64) / (frame_duration as i64);
                    bridge_calc_timing_ratio(wrapper, ratio as i32);
                    let adjustment = (ratio as u64).wrapping_mul(frame_duration);
                    let (adj_lo, adj_hi) = split(adjustment);
                    (*wrapper).pause_detect_lo = time_lo.wrapping_sub(adj_lo);
                    (*wrapper).pause_detect_hi = (time_hi as i64)
                        .wrapping_sub((adj_lo as i32 >> 31) as i64)
                        .wrapping_sub(if time_lo < adj_lo { 1 } else { 0 })
                        as u32;
                    if used_normal_path {
                        handle_secondary_pause(
                            wrapper,
                            time_lo,
                            time_hi,
                            freq,
                            delta,
                            frame_duration,
                        );
                    }
                }
            } else {
                // Delta out of range — reset pause detection
                (*wrapper).pause_detect_lo = time_lo;
                (*wrapper).pause_detect_hi = time_hi;
                handle_secondary_pause(wrapper, time_lo, time_hi, freq, delta, frame_duration);
            }
        } else {
            // Replay mode with negative frame delay — use replay-specific timing
            let game_info = (*ddgame).game_info;
            let replay_ticks = *((game_info as *const u8).add(0xef3c) as *const i32);
            used_normal_path = false;
            elapsed = freq / (replay_ticks as u64);
            // Jump to the timing ratio calculation
            let ratio = (elapsed as i64) / (frame_interval as i64);
            bridge_calc_timing_ratio(wrapper, ratio as i32);
            let adjustment = (ratio as u64).wrapping_mul(frame_interval);
            let (adj_lo, _) = split(adjustment);
            (*wrapper).pause_detect_lo = time_lo.wrapping_sub(adj_lo);
            (*wrapper).pause_detect_hi = time_hi
                .wrapping_sub((adj_lo as i32 >> 31) as u32 + if time_lo < adj_lo { 1 } else { 0 });
            // Skip secondary pause handling in replay mode
            let timing_state = (*wrapper).timing_jitter_state;
            if timing_state == 2 {
                (*wrapper).timing_jitter_state = 1;
                (*wrapper).pause_secondary_lo = time_lo;
                (*wrapper).pause_secondary_hi = time_hi;
            } else {
                let sec_ratio = (elapsed as i64) / ((freq_lo / 2) as i64);
                (*wrapper).timing_jitter_state ^= (sec_ratio as i32) & 1;
                let sec_adj = (sec_ratio as u64).wrapping_mul(frame_interval);
                let (sa_lo, sa_hi) = split(sec_adj);
                (*wrapper).pause_secondary_lo = time_lo.wrapping_sub(sa_lo);
                (*wrapper).pause_secondary_hi = time_hi.wrapping_sub(
                    (sa_hi as i32 >> 31) as u32 + if time_lo < sa_lo { 1 } else { 0 },
                );
            }
        }

        // Initialize initial_ref on first call
        if (*wrapper).initial_ref_lo == 0 && (*wrapper).initial_ref_hi == 0 {
            (*wrapper).initial_ref_lo = time_lo;
            (*wrapper).initial_ref_hi = time_hi;
        }

        // Compute elapsed from initial_ref
        if used_normal_path {
            let init_delta = time_sub(
                time_lo,
                time_hi,
                (*wrapper).initial_ref_lo,
                (*wrapper).initial_ref_hi,
            );
            if (init_delta as i64) >= 0 {
                elapsed = init_delta;
            } else {
                elapsed = 0;
            }
        } else {
            (*wrapper).initial_ref_lo = time_lo;
            (*wrapper).initial_ref_hi = time_hi;
        }

        // ── FPU section: compute fps-related values ────────────────────
        //
        // fps_scaled = (int)(elapsed_f * 3.75 / freq_f)
        // Clamped to 0x1333 (4915) when in normal timing path
        let elapsed_f = elapsed as f64;
        let freq_f = freq as f64;

        let mut fps_scaled = (elapsed_f * 3.75 / freq_f) as i32;
        if fps_scaled > 0x1333 && used_normal_path {
            fps_scaled = 0x1333;
        }

        // headless_log_a = elapsed * 7.5 (stored as f64 for log output)
        let headless_log_a = elapsed_f * 7.5;

        // fps_product = (int)(65536 * elapsed * 3.75 / freq * elapsed * 7.5 / freq)
        //             = (int)(65536 * 3.75 * 7.5 * elapsed^2 / freq^2)
        let mut fps_product =
            (65536.0 * elapsed_f * 3.75 / freq_f * elapsed_f * 7.5 / freq_f) as i32;
        if fps_product > 0x2666 && used_normal_path {
            fps_product = 0x2666;
        }

        // fixed_render_scale = 0x10000 - (int)(result from __ftol2_sse)
        // The __ftol2_sse call converts fps_product-related value
        let fixed_render_scale = 0x10000_i32.wrapping_sub(fps_product);

        // DDGame vtable[3](0x36) — check some condition
        let ddgame_vtable = *(ddgame as *const *const u32);
        let vfunc3: unsafe extern "thiscall" fn(*mut DDGame, u32) -> i32 =
            core::mem::transmute(*ddgame_vtable.add(3));
        let minimize_request = vfunc3(ddgame, 0x36);
        if minimize_request != 0 {
            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
            (*session).minimize_request = 1;
        }

        // SetupFrameParams(fps_scaled, game_speed, iStack_44)
        bridge_setup_frame_params(
            wrapper, fps_scaled, game_speed, 0, /* iStack_44 placeholder */
        );

        // ── Compute frame advance parameters ───────────────────────────
        let frame_fixed = (elapsed as u64).wrapping_mul(0x10000);
        let advance_ratio = frame_fixed / frame_duration;

        bridge_advance_frame_counters(
            wrapper,
            fixed_render_scale,
            0, // iStack_44
            game_speed,
            0, // iVar14
            advance_ratio as u32,
        );
        bridge_update_frame_timing(
            wrapper,
            elapsed as u32,
            (elapsed >> 32) as u32,
            time_hi,
            freq_lo,
        );

        // DDGame sub-object at +8: if non-null, call its vtable[1]
        let sub_obj_8 = *((ddgame as *const u8).add(8) as *const *mut u8);
        if !sub_obj_8.is_null() {
            let vtable = *(sub_obj_8 as *const *const u32);
            let vfunc: unsafe extern "thiscall" fn(*mut u8) = core::mem::transmute(*vtable.add(1));
            vfunc(sub_obj_8);
        }

        // DDGame sub-object at +0xC: if non-null, call FUN_005464e0
        let sub_obj_c = *((ddgame as *const u8).add(0xc) as *const *mut u8);
        if !sub_obj_c.is_null() {
            let func: unsafe extern "C" fn(*mut u8) = core::mem::transmute(rb(0x005464e0) as usize);
            func(sub_obj_c);
        }

        // DDGame sub-object at +4: call its vtable[2]
        let sub_obj_4 = *((ddgame as *const u8).add(4) as *const *mut u8);
        let vtable_4 = *(sub_obj_4 as *const *const u32);
        let vfunc2: unsafe extern "thiscall" fn(*mut u8) = core::mem::transmute(*vtable_4.add(2));
        vfunc2(sub_obj_4);

        // Update DDGame+0x7E9C if _field_410 == 0
        if (*wrapper)._field_410 == 0 {
            let ddgame_vtable = *(ddgame as *const *const u32);
            let vfunc1: unsafe extern "thiscall" fn(*mut DDGame, u32) -> u32 =
                core::mem::transmute(*ddgame_vtable.add(1));
            let result = vfunc1(ddgame, 0xd);
            *((ddgame as *mut u8).add(0x7e9c) as *mut u32) = result;
        }
    }
    // ── End of has_sound block ──

    // ── Section 3: Compute elapsed time from reference ─────────────────
    if (*wrapper).timing_ref_lo == 0 && (*wrapper).timing_ref_hi == 0 {
        (*wrapper).timing_ref_lo = time_lo;
        (*wrapper).timing_ref_hi = time_hi;
    }

    let ref_delta = time_sub(
        time_lo,
        time_hi,
        (*wrapper).timing_ref_lo,
        (*wrapper).timing_ref_hi,
    ) as i64;

    let mut remaining: u64;
    if ref_delta < 0 {
        remaining = 0;
    } else {
        let quarter_freq = freq / 4;
        let four_frames = frame_duration.wrapping_mul(4);
        let max_remaining = if quarter_freq < four_frames {
            four_frames
        } else {
            quarter_freq
        };
        if max_remaining < ref_delta as u64 {
            remaining = max_remaining;
        } else {
            remaining = ref_delta as u64;
        }
    }

    // ── Section 4: Frame delay handling ────────────────────────────────
    let frame_delay = (*wrapper).frame_delay_counter;
    if frame_delay >= 0 {
        let game_info = (*ddgame).game_info;
        let game_info_ptr = game_info as *const u8;
        let gi_f348 = *(game_info_ptr.add(0xf348) as *const u8);
        let gi_f344 = *(game_info_ptr.add(0xf344) as *const i32);

        if gi_f348 == 0 && gi_f344 <= (*ddgame).frame_counter {
            let is_replay = is_replay_mode(wrapper);
            if !is_replay {
                remaining = (frame_delay as i64).wrapping_mul(frame_duration as i64) as u64;
            }
            if frame_delay == 0 {
                bridge_init_frame_delay(wrapper);
            } else if !is_replay {
                (*wrapper).frame_delay_counter = 0;
            }
        }
    }

    // ── Section 5: Read elapsed time for frame loop timing ─────────────
    let (now_lo, now_hi) = read_current_time();
    let loop_elapsed = time_sub(
        now_lo,
        now_hi,
        (*wrapper).last_frame_time_lo,
        (*wrapper).last_frame_time_hi,
    );

    // Network frame processing
    if (*ddgame).network_ecx != 0 {
        bridge_process_network_frame(wrapper, time_lo, time_hi, freq_lo, freq_hi);
    }

    // Clamp remaining for replay/network catch-up
    let ddgame = (*wrapper).ddgame; // re-read after potential modification
    let game_info = (*ddgame).game_info as *const u8;
    let gi_f348 = *(game_info.add(0xf348) as *const u8);
    let gi_f344 = *(game_info.add(0xf344) as *const i32);

    if (gi_f348 != 0 || (*ddgame).frame_counter < gi_f344)
        && remaining < frame_duration
        && *(rb(0x008ace34) as *const u8) != 0
    {
        remaining = frame_duration;
    }
    *(rb(0x008ace34) as *mut u8) = 1;

    // ── Replay mode speed adjustment ───────────────────────────────────
    let is_replay = is_replay_mode(wrapper);
    if is_replay {
        let frame_delay = (*wrapper).frame_delay_counter;
        if frame_delay != 0 {
            if frame_delay > 0 {
                (*wrapper).frame_delay_counter = frame_delay - 1;
            }
            let ddgame = (*wrapper).ddgame;
            let replay_ticks = *(((*ddgame).game_info as *const u8).add(0xef3c) as *const i32);
            let speed_accum = combine(
                *((ddgame as *const u8).add(0x8158) as *const u32),
                *((ddgame as *const u8).add(0x815c) as *const u32),
            );
            let speed_val = (speed_accum / replay_ticks as u64) as i32
                - *((ddgame as *const u8).add(0x8160) as *const i32);
            *((ddgame as *mut u8).add(0x8150) as *mut i32) = speed_val;
            *((ddgame as *mut u8).add(0x8154) as *mut i32) = speed_val;

            // Advance speed accumulator by 0x320000
            let ddgame = (*wrapper).ddgame;
            let accum_ptr = (ddgame as *mut u8).add(0x8158) as *mut u64;
            *accum_ptr = (*accum_ptr).wrapping_add(0x320000);

            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
            (*session)._unknown_044[0] = 1; // session+0x44
        }
    }

    // ── Game-over detection (replay finished) ──────────────────────────
    {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let replay_ticks = *(game_info.add(0xef3c) as *const i32);
        if replay_ticks != 0 {
            let replay_end = *(game_info.add(0xf350) as *const i32);
            let replay_start = *(game_info.add(0xf344) as *const i32);
            let frame_counter = (*ddgame).frame_counter;
            let speed_val = *((ddgame as *const u8).add(0x8150) as *const i32);

            if (frame_counter > replay_end || (frame_counter == replay_end && speed_val > 0))
                && (*wrapper).game_end_phase != 1
            {
                (*wrapper).game_end_phase = 1;
                (*wrapper).game_end_speed = 0x10000;
                (*wrapper).game_state = 5; // EXIT
            }
        }
    }

    // ── Section 6: Main frame loop ─────────────────────────────────────
    loop {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let replay_ticks = *(game_info.add(0xef3c) as *const i32);

        if replay_ticks == 0 {
            // Normal (non-replay) frame dispatch
            if remaining == 0 {
                break;
            }

            // Compute available time budget
            let accum_a = combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi);
            let accum_b = combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi);
            let max_accum = if accum_a > accum_b { accum_a } else { accum_b };

            let budget = frame_duration.saturating_sub(max_accum);
            let mut frame_time = remaining;
            if remaining > budget {
                let gi_f348 = *(game_info.add(0xf348) as *const u8);
                let gi_f344 = *(game_info.add(0xf344) as *const i32);
                frame_time = budget;
                if gi_f348 == 0 && gi_f344 <= (*ddgame).frame_counter {
                    frame_time = remaining;
                }
            }

            let (ft_lo, ft_hi) = split(frame_time);
            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);

            if (*session).flag_5c == 0 || (*ddgame).network_ecx != 0 {
                let accum_b_new = combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi)
                    .wrapping_add(frame_time);
                (*wrapper).frame_accum_b_lo = accum_b_new as u32;
                (*wrapper).frame_accum_b_hi = (accum_b_new >> 32) as u32;

                if accum_b_new == frame_duration {
                    (*wrapper).frame_accum_b_lo = 0;
                    (*wrapper).frame_accum_b_hi = 0;
                    bridge_reset_frame_state(wrapper);
                }
            }

            if is_frame_paused(wrapper) {
                // Paused — accumulate into accum_a
                let accum_a_new = combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi)
                    .wrapping_add(frame_time);
                (*wrapper).frame_accum_a_lo = accum_a_new as u32;
                (*wrapper).frame_accum_a_hi = (accum_a_new >> 32) as u32;
                (*wrapper).frame_accum_c_lo = 0;
                (*wrapper).frame_accum_c_hi = 0;

                if combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi)
                    == frame_duration
                {
                    (*wrapper).frame_accum_a_lo = 0;
                    (*wrapper).frame_accum_a_hi = 0;
                    // Step frame
                    let stepped = step_frame(
                        wrapper,
                        &mut remaining,
                        ft_lo,
                        ft_hi,
                        game_speed_target,
                        saved_game_speed,
                    );
                    if !stepped {
                        break;
                    }
                } else {
                    // Check if replay has more frames
                    let gi = (*(*wrapper).ddgame).game_info as *const u8;
                    let gi_f348 = *(gi.add(0xf348) as *const u8);
                    if gi_f348 == 0 {
                        let fc = (*(*wrapper).ddgame).frame_counter;
                        let gi_f344 = *(gi.add(0xf344) as *const i32);
                        if fc >= gi_f344 {
                            remaining = remaining.wrapping_sub(frame_time);
                        }
                    }
                }
            } else {
                // Not paused — accumulate into accum_c
                let accum_c_new = combine((*wrapper).frame_accum_c_lo, (*wrapper).frame_accum_c_hi)
                    .wrapping_add(frame_time);
                (*wrapper).frame_accum_c_lo = accum_c_new as u32;
                (*wrapper).frame_accum_c_hi = (accum_c_new >> 32) as u32;

                if accum_c_new >= frame_duration {
                    (*wrapper).frame_accum_c_lo = (accum_c_new - frame_duration) as u32;
                    (*wrapper).frame_accum_c_hi = ((accum_c_new - frame_duration) >> 32) as u32;
                    // Step frame
                    let stepped = step_frame(
                        wrapper,
                        &mut remaining,
                        ft_lo,
                        ft_hi,
                        game_speed_target,
                        saved_game_speed,
                    );
                    if !stepped {
                        break;
                    }
                } else {
                    let gi = (*(*wrapper).ddgame).game_info as *const u8;
                    let gi_f348 = *(gi.add(0xf348) as *const u8);
                    if gi_f348 == 0 {
                        let fc = (*(*wrapper).ddgame).frame_counter;
                        let gi_f344 = *(gi.add(0xf344) as *const i32);
                        if fc >= gi_f344 {
                            remaining = remaining.wrapping_sub(frame_time);
                        }
                    }
                }
            }
        } else {
            // Replay mode frame dispatch
            let speed_val = *((ddgame as *const u8).add(0x8150) as *const i32);
            if speed_val < 0x10000 {
                break;
            }

            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
            if (*session).flag_5c == 0 || (*ddgame).network_ecx != 0 {
                bridge_reset_frame_state(wrapper);
            }
            let stepped = step_frame(
                wrapper,
                &mut remaining,
                frame_duration as u32,
                (frame_duration >> 32) as u32,
                game_speed_target,
                saved_game_speed,
            );
            if !stepped {
                break;
            }
        }

        // Check if we should continue processing more frames
        if !should_continue_frame_loop(wrapper, loop_elapsed as u32, (loop_elapsed >> 32) as u32) {
            break;
        }
    }

    // ── Section 7: Post-frame timing updates ───────────────────────────
    {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let replay_ticks = *(game_info.add(0xef3c) as *const i32);

        if replay_ticks == 0 {
            let gi_f348 = *(game_info.add(0xf348) as *const u8);
            let gi_f344 = *(game_info.add(0xf344) as *const i32);

            if gi_f348 == 0 && gi_f344 <= (*ddgame).frame_counter {
                // Update speed ratios from accumulators
                let accum_a = combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi);
                let speed_a = accum_a.wrapping_mul(0x10000) / frame_duration;
                *((ddgame as *mut u8).add(0x8150) as *mut u32) = speed_a as u32;

                let accum_b = combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi);
                let speed_b = accum_b.wrapping_mul(0x10000) / frame_duration;
                *((ddgame as *mut u8).add(0x8154) as *mut u32) = speed_b as u32;

                // Reset accumulators if frame_delay >= 0
                if (*wrapper).frame_delay_counter >= 0 {
                    (*wrapper).frame_accum_a_lo = 0;
                    (*wrapper).frame_accum_a_hi = 0;
                    (*wrapper).frame_accum_b_lo = 0;
                    (*wrapper).frame_accum_b_hi = 0;
                    (*wrapper).frame_accum_c_lo = 0;
                    (*wrapper).frame_accum_c_hi = 0;
                }

                // Handle game speed changes
                let ddgame = (*wrapper).ddgame;
                let new_target = (*ddgame).game_speed_target.to_raw();
                if game_speed_target != new_target
                    || saved_game_speed != (*ddgame).game_speed.to_raw()
                {
                    if saved_frame_delay >= 0 && (*wrapper).frame_delay_counter < 0 {
                        // Speed change while frame delay was active → reset
                        (*wrapper).frame_accum_a_lo = 0;
                        (*wrapper).frame_accum_a_hi = 0;
                        (*wrapper).frame_accum_b_lo = 0;
                        (*wrapper).frame_accum_b_hi = 0;
                        (*wrapper).frame_accum_c_lo = 0;
                        (*wrapper).frame_accum_c_hi = 0;
                        *((ddgame as *mut u8).add(0x8150) as *mut u32) = 0;
                        *((ddgame as *mut u8).add(0x8154) as *mut u32) = 0;
                    } else {
                        // Rescale accumulators for new speed
                        let new_interval = freq / 50;
                        let new_speed = (*ddgame).game_speed.to_raw();
                        let scale = ((new_speed as i64).wrapping_mul(new_interval as i64)
                            / (new_target as i64)) as u64;

                        let old_speed_a = *((ddgame as *const u8).add(0x8150) as *const i32);
                        let scaled_a =
                            ((old_speed_a as i64).wrapping_mul(scale as i64) >> 16) as u64;
                        (*wrapper).frame_accum_a_lo = scaled_a as u32;
                        (*wrapper).frame_accum_a_hi = (scaled_a >> 32) as u32;

                        let old_speed_b = *((ddgame as *const u8).add(0x8154) as *const i32);
                        let scaled_b =
                            ((old_speed_b as i64).wrapping_mul(scale as i64) >> 16) as u64;
                        (*wrapper).frame_accum_b_lo = scaled_b as u32;
                        (*wrapper).frame_accum_b_hi = (scaled_b >> 32) as u32;

                        // Rescale accum_c if nonzero
                        let accum_c =
                            combine((*wrapper).frame_accum_c_lo, (*wrapper).frame_accum_c_hi);
                        if accum_c != 0 {
                            let scaled_c = (scale as i64)
                                .wrapping_mul((*wrapper).frame_accum_c_lo as i64)
                                / frame_duration as i64;
                            (*wrapper).frame_accum_c_lo = scaled_c as u32;
                            (*wrapper).frame_accum_c_hi = (scaled_c >> 32) as u32;
                        }
                    }
                }

                (*wrapper).timing_ref_lo = time_lo;
                (*wrapper).timing_ref_hi = time_hi;
            } else {
                // Before game start — zero speed
                let ddgame = (*wrapper).ddgame;
                *((ddgame as *mut u8).add(0x8150) as *mut u32) = 0;
                *((ddgame as *mut u8).add(0x8154) as *mut u32) = 0;
                (*wrapper).timing_ref_lo = time_lo;
                (*wrapper).timing_ref_hi = time_hi;
            }
        } else {
            // Replay mode — subtract remaining from reference
            let (rem_lo, rem_hi) = split(remaining);
            (*wrapper).timing_ref_lo = time_lo.wrapping_sub(rem_lo);
            (*wrapper).timing_ref_hi = (time_hi as i64)
                .wrapping_sub((rem_hi as i32 >> 31) as i64)
                .wrapping_sub(if time_lo < rem_lo { 1 } else { 0 })
                as u32;
        }
    }

    // ── Section 8: Store last frame timestamp ──────────────────────────
    let (now_lo, now_hi) = read_current_time();
    (*wrapper).last_frame_time_lo = now_lo;
    (*wrapper).last_frame_time_hi = now_hi;

    // ── Section 9: Headless log output ─────────────────────────────────
    {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let headless = *(game_info.add(0xf914) as *const i32);
        let log_enabled = *(game_info.add(0xef38) as *const i32);

        if headless != 0 && log_enabled != 0 {
            // Bridge to headless log writer
            bridge_write_headless_log(wrapper, 0, 0); // TODO: pass actual log params
                                                      // TODO: port the fputs + ExitProcess logic
        }
    }

    // ── Section 10: Network update ─────────────────────────────────────
    if (*(*wrapper).ddgame).sound_available != 0 {
        bridge_network_update(wrapper);
    }

    // ── Section 11: Game-end detection (network timeout) ───────────────
    {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let net_timeout = *(game_info.add(0xf3b0) as *const u16);

        if net_timeout != 0 {
            let frame_count_77d4 = *((ddgame as *const u8).add(0x77d4) as *const i32);
            if (net_timeout as i32) < frame_count_77d4 / 50 && (*wrapper).game_end_phase == 0 {
                (*wrapper).game_state = 4; // EXIT_HEADLESS
                (*wrapper).game_end_clear = 0;
                (*wrapper).game_end_speed = 0;

                // Check protocol version for broadcast
                let protocol_ver = *(game_info.add(0xd778) as *const i32);
                if protocol_ver > 0x4c {
                    // Broadcast message via task_turn_game vtable[2]
                    let task = (*wrapper).task_turn_game;
                    let task_vtable = *(task as *const *const u32);
                    let vfunc: unsafe extern "thiscall" fn(*mut u8, u32, u32, u32) =
                        core::mem::transmute(*task_vtable.add(2));
                    vfunc(task, 0x75, 0, 0);
                }
                (*wrapper).game_end_phase = 1;
            }
        }
    }
}

// ─── Internal helpers ──────────────────────────────────────────────────────

/// Handle timing ratio calculation and pause adjustment.
unsafe fn calc_timing_and_adjust_pause(
    wrapper: *mut DDGameWrapper,
    frame_interval: u64,
    time_lo: u32,
    time_hi: u32,
    delta: u64,
    used_normal_path: &mut bool,
) {
    let ratio = (delta as i64) / (frame_interval as i64);
    bridge_calc_timing_ratio(wrapper, ratio as i32);
    let adjustment = (ratio as u64).wrapping_mul(frame_interval);
    let (adj_lo, _) = split(adjustment);
    (*wrapper).pause_detect_lo = time_lo.wrapping_sub(adj_lo);
    (*wrapper).pause_detect_hi =
        time_hi.wrapping_sub(((adj_lo as i32) >> 31) as u32 + if time_lo < adj_lo { 1 } else { 0 });
    // Always goes to secondary pause handling
    // (bVar19 is true here so we fall through)
}

/// Handle secondary pause detection timestamp update.
unsafe fn handle_secondary_pause(
    wrapper: *mut DDGameWrapper,
    time_lo: u32,
    time_hi: u32,
    freq: u64,
    delta: u64,
    frame_duration: u64,
) {
    let sec_delta = time_sub(
        time_lo,
        time_hi,
        (*wrapper).pause_secondary_lo,
        (*wrapper).pause_secondary_hi,
    );

    if (sec_delta as i64) >= 0 {
        let double_freq = freq.wrapping_mul(2);
        if sec_delta <= double_freq {
            if (*wrapper).timing_jitter_state == 2 {
                (*wrapper).timing_jitter_state = 1;
                (*wrapper).pause_secondary_lo = time_lo;
                (*wrapper).pause_secondary_hi = time_hi;
            } else {
                let half_freq = freq / 2;
                let sec_ratio = (sec_delta as i64) / (half_freq as i64);
                (*wrapper).timing_jitter_state ^= (sec_ratio as i32 & 1) as i32;
                let sec_adj = (sec_ratio as u64).wrapping_mul(frame_duration);
                let (sa_lo, sa_hi) = split(sec_adj);
                (*wrapper).pause_secondary_lo = time_lo.wrapping_sub(sa_lo);
                (*wrapper).pause_secondary_hi = time_hi.wrapping_sub(
                    ((sa_hi as i32) >> 31) as u32 + if time_lo < sa_lo { 1 } else { 0 },
                );
            }
            return;
        }
    }
    // Delta negative or too large — reset
    (*wrapper).timing_jitter_state = 1;
    (*wrapper).pause_secondary_lo = time_lo;
    (*wrapper).pause_secondary_hi = time_hi;
}
