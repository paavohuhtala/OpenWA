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
/// Bridge for usercall(EAX=this), no stack params, plain RET.
macro_rules! bridge_eax_this {
    ($name:ident, $addr:expr, $ret:ty) => {
        #[unsafe(naked)]
        unsafe extern "stdcall" fn $name(_this: *mut DDGameWrapper) -> $ret {
            core::arch::naked_asm!(
                "popl %ecx",
                "popl %eax",
                "pushl %ecx",
                "jmpl *({fn})",
                fn = sym $addr,
                options(att_syntax),
            );
        }
    };
}

/// Bridge for usercall(EAX=this) + N stdcall stack params.
macro_rules! bridge_eax_this_stdcall {
    ($name:ident, $addr:expr, ($($param:ty),+) -> $ret:ty) => {
        #[unsafe(naked)]
        unsafe extern "stdcall" fn $name(_this: *mut DDGameWrapper, $(_: $param),+) -> $ret {
            core::arch::naked_asm!(
                "popl %ecx",
                "popl %eax",
                "pushl %ecx",
                "jmpl *({fn})",
                fn = sym $addr,
                options(att_syntax),
            );
        }
    };
}

// ESI bridges use inline asm inside regular functions.
// ESI is LLVM-reserved on x86, so we can't use it as an asm operand,
// but we can save/restore it manually inside the asm block.
// We push params explicitly and use CALL to invoke the target.

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
    IS_REPLAY_MODE_ADDR = rb(va::DDGAMEWRAPPER_IS_REPLAY_MODE);
}

// ─── Bridge function declarations ──────────────────────────────────────────

// ── EAX=this, no stack params, plain RET ──
bridge_eax_this!(bridge_is_replay_mode, IS_REPLAY_MODE_ADDR, u32);
bridge_eax_this!(bridge_reset_frame_state, RESET_FRAME_STATE_ADDR, ());
bridge_eax_this!(bridge_init_frame_delay, INIT_FRAME_DELAY_ADDR, ());
bridge_eax_this!(bridge_is_frame_paused, IS_FRAME_PAUSED_ADDR, u32);

// ── EAX=this + stdcall stack params ──
bridge_eax_this_stdcall!(bridge_setup_frame_params, SETUP_FRAME_PARAMS_ADDR, (i32, i32, i32) -> ());

// ── ESI=this, no stack params, plain RET ──
unsafe fn bridge_network_update(wrapper: *mut DDGameWrapper) {
    core::arch::asm!(
        "push esi",
        "mov esi, {this}",
        "call [{addr}]",
        "pop esi",
        this = in(reg) wrapper,
        addr = sym NETWORK_UPDATE_ADDR,
        out("eax") _, out("ecx") _, out("edx") _,
    );
}

/// Port of DDGameWrapper__CalcTimingRatio (0x52ABF0).
/// ESI=this, 1 stack param (ratio), RET 0x4.
/// Adjusts wrapper+0x400 (timing progress) toward wrapper+0x408 (timing target).
unsafe fn calc_timing_ratio(wrapper: *mut DDGameWrapper, ratio: i32) {
    let w = wrapper as *mut u8;
    let ddgame = (*wrapper).ddgame;
    let game_info = (*ddgame).game_info as *const u8;

    let gi_f398 = *(game_info.add(0xf398) as *const i32);
    let gi_f348 = *(game_info.add(0xf348) as *const u8);
    let gi_f344 = *(game_info.add(0xf344) as *const i32);
    let frame_counter = (*ddgame).frame_counter;

    if gi_f398 == 0 && gi_f348 == 0 && gi_f344 <= frame_counter {
        if ratio != 0 {
            let target = *(w.add(0x408) as *const i32);
            let progress = *(w.add(0x400) as *const i32);
            let gap = target - progress;
            if gap > 0 {
                let multiplier = if gap / 5 > 1 { 2 } else { 1 };
                let step = multiplier * ratio;
                if gap <= step {
                    *(w.add(0x400) as *mut i32) = target;
                } else {
                    *(w.add(0x400) as *mut i32) = progress + step;
                }
                *(w.add(0x404) as *mut u32) = 1;
            }
        }
    } else {
        let target = *(w.add(0x408) as *const i32);
        let progress = *(w.add(0x400) as *const i32);
        if progress != target {
            *(w.add(0x400) as *mut i32) = target;
            *(w.add(0x404) as *mut u32) = 1;
        }
    }
}

// Naked bridges for usercall(ESI=this) + N stdcall params. We use naked
// functions to avoid a Rust/LLVM inline-asm quirk where routing params
// through an intermediate stack array (`args.as_ptr()` + memory pushes)
// produced garbage values in release builds — the array writes would be
// optimized away relative to the asm block's memory reads. The naked
// variant reads stdcall args directly off the incoming stack, so the
// compiler never has to materialize them into an auxiliary array.

/// Bridge for DDGameWrapper__AdvanceFrameCounters (0x52AAA0).
/// Usercall: ESI=this, 5 stdcall params, RET 0x14.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_advance_frame_counters(
    _this: *mut DDGameWrapper,
    _p1: i32,
    _p2: i32,
    _p3: i32,
    _p4: i32,
    _p5: u32,
) {
    core::arch::naked_asm!(
        // Entry stack:
        //   [esp]      retaddr
        //   [esp+4]    this
        //   [esp+8..+0x18]  p1..p5
        "push esi",                         // save callee-saved ESI
        // After push esi:
        //   [esp]      saved_esi
        //   [esp+4]    retaddr
        //   [esp+8]    this
        //   [esp+0xC..+0x1C] p1..p5
        "mov esi, [esp+8]",                 // ESI = this
        "push dword ptr [esp+0x1C]",        // p5 (each push keeps offset same since ESP moves)
        "push dword ptr [esp+0x1C]",        // p4
        "push dword ptr [esp+0x1C]",        // p3
        "push dword ptr [esp+0x1C]",        // p2
        "push dword ptr [esp+0x1C]",        // p1
        "call [{addr}]",                    // target cleans 5 params via RET 0x14
        "pop esi",                           // restore ESI
        "ret 24",                            // stdcall: clean 6 args (this + p1..p5 = 24)
        addr = sym ADVANCE_FRAME_COUNTERS_ADDR,
    );
}

/// Bridge for DDGameWrapper__UpdateFrameTiming (0x52A9C0).
/// Usercall: ESI=this, 4 stdcall params, RET 0x10.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_update_frame_timing(
    _this: *mut DDGameWrapper,
    _p1: u32,
    _p2: u32,
    _p3: u32,
    _p4: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, [esp+8]",
        "push dword ptr [esp+0x18]",        // p4
        "push dword ptr [esp+0x18]",        // p3
        "push dword ptr [esp+0x18]",        // p2
        "push dword ptr [esp+0x18]",        // p1
        "call [{addr}]",
        "pop esi",
        "ret 20",                            // clean 5 args (this + p1..p4 = 20)
        addr = sym UPDATE_FRAME_TIMING_ADDR,
    );
}

/// Bridge for DDGameWrapper__ProcessNetworkFrame (0x52xxxx).
/// Usercall: ESI=this, 4 stdcall params, RET 0x10.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_process_network_frame(
    _this: *mut DDGameWrapper,
    _p1: u32,
    _p2: u32,
    _p3: u32,
    _p4: u32,
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, [esp+8]",
        "push dword ptr [esp+0x18]",
        "push dword ptr [esp+0x18]",
        "push dword ptr [esp+0x18]",
        "push dword ptr [esp+0x18]",
        "call [{addr}]",
        "pop esi",
        "ret 20",
        addr = sym PROCESS_NETWORK_FRAME_ADDR,
    );
}

static mut IS_REPLAY_MODE_ADDR: u32 = 0;

// ── StepFrame: ECX=this (thiscall), EAX=extra ptr, 5 stack params, RET 0x14 ──
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_step_frame(
    _this: *mut DDGameWrapper,
    _counter_ptr: *mut u8, // passed in EAX, NOT on stack to callee
    _remaining: *mut u64,
    _time_lo: u32,
    _time_hi: u32,
    _speed_target: i32,
    _speed: i32,
) -> u32 {
    core::arch::naked_asm!(
        "popl %edx",           // pop return address
        "popl %ecx",           // pop this → ECX (thiscall)
        "popl %eax",           // pop counter_ptr → EAX (extra register param)
        "pushl %edx",          // push return address back
        // Stack now: [ret, remaining, time_lo, time_hi, speed_target, speed]
        // = 5 params for target's RET 0x14
        "jmpl *({fn})",
        fn = sym STEP_FRAME_ADDR,
        options(att_syntax),
    );
}

// ── ShouldContinueFrameLoop: plain stdcall(wrapper, lo, hi), RET 0xC ──
// No register params — wrapper is on the stack
unsafe extern "stdcall" fn bridge_should_continue(
    wrapper: *mut DDGameWrapper,
    elapsed_lo: u32,
    elapsed_hi: u32,
) -> u32 {
    let func: unsafe extern "stdcall" fn(*mut DDGameWrapper, u32, u32) -> u32 =
        core::mem::transmute(SHOULD_CONTINUE_ADDR as usize);
    func(wrapper, elapsed_lo, elapsed_hi)
}

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
/// `counter_ptr` is a pointer to a local counter incremented by StepFrame (passed in EAX).
pub unsafe fn step_frame(
    wrapper: *mut DDGameWrapper,
    counter_ptr: *mut u8,
    remaining: *mut u64,
    frame_duration_lo: u32,
    frame_duration_hi: u32,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    bridge_step_frame(
        wrapper,
        counter_ptr,
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

/// Signed 32-bit division matching WA's FUN_005d8786.
/// Returns (quotient, remainder) using only the low 32 bits of the dividend.
/// The original function uses x86 IDIV which produces both values.
#[inline(always)]
fn wa_div(dividend_lo: i32, divisor: i32) -> (i32, i32) {
    (dividend_lo / divisor, dividend_lo % divisor)
}

/// Subtract a sign-extended i32 remainder from a 64-bit timestamp.
/// Matches the CDQ + SUB + SBB pattern used after FUN_005d8786 in DispatchFrame.
#[inline(always)]
fn time_sub_i32(time_lo: u32, time_hi: u32, remainder: i32) -> (u32, u32) {
    let rem_u32 = remainder as u32;
    let sign_hi = (remainder >> 31) as u32; // 0x00000000 or 0xFFFFFFFF
    let lo = time_lo.wrapping_sub(rem_u32);
    let hi = time_hi
        .wrapping_sub(sign_hi)
        .wrapping_sub(if time_lo < rem_u32 { 1 } else { 0 });
    (lo, hi)
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
    let freq = combine(freq_lo, freq_hi);

    // Local counter passed to StepFrame via EAX — StepFrame increments it
    let mut frame_step_counter: u32 = 0;

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
    // bVar18 in the decompile — tracks whether we took the "normal" timing path.
    // Controls: secondary pause fallthrough, and elapsed computation from initial_ref.
    let mut used_normal_path = false;

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
            // Normal timing path (bVar18 = true)
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
                let gi_speed = *(((*ddgame).game_info as *const u8).add(0xd988) as *const i32);
                if game_speed_target == gi_speed {
                    // Path A: Speed hasn't changed — CalcTimingRatio with frame_interval
                    let (ratio, remainder) = wa_div(delta as i32, frame_interval as i32);
                    calc_timing_ratio(wrapper, ratio);
                    let (pd_lo, pd_hi) = time_sub_i32(time_lo, time_hi, remainder);
                    (*wrapper).pause_detect_lo = pd_lo;
                    (*wrapper).pause_detect_hi = pd_hi;
                } else {
                    // Path B: Speed changed — CalcTimingRatio with frame_duration
                    let (ratio, remainder) = wa_div(delta as i32, frame_duration as i32);
                    calc_timing_ratio(wrapper, ratio);
                    let (pd_lo, pd_hi) = time_sub_i32(time_lo, time_hi, remainder);
                    (*wrapper).pause_detect_lo = pd_lo;
                    (*wrapper).pause_detect_hi = pd_hi;
                }
                // bVar18 is true → fall through to secondary pause (LAB_0052928c)
                handle_secondary_pause(wrapper, time_lo, time_hi, freq);
            } else {
                // Path C: Delta out of range — reset pause detection
                (*wrapper).pause_detect_lo = time_lo;
                (*wrapper).pause_detect_hi = time_hi;
                // Falls through to secondary pause (LAB_0052928c)
                handle_secondary_pause(wrapper, time_lo, time_hi, freq);
            }
        } else {
            // Path D: Replay mode with negative frame delay
            let game_info = (*ddgame).game_info;
            let replay_ticks = *((game_info as *const u8).add(0xef3c) as *const i32);
            elapsed = freq / (replay_ticks as u64);
            // Shared CalcTimingRatio path (LAB_00529246 → LAB_00529252)
            let (ratio, remainder) = wa_div(elapsed as i32, frame_interval as i32);
            calc_timing_ratio(wrapper, ratio);
            let (pd_lo, pd_hi) = time_sub_i32(time_lo, time_hi, remainder);
            (*wrapper).pause_detect_lo = pd_lo;
            (*wrapper).pause_detect_hi = pd_hi;
            // bVar18 is false → skip secondary pause delta check,
            // jump directly to jitter calc (LAB_005292c5)
            if (*wrapper).timing_jitter_state == 2 {
                (*wrapper).timing_jitter_state = 1;
                (*wrapper).pause_secondary_lo = time_lo;
                (*wrapper).pause_secondary_hi = time_hi;
            } else {
                let half_freq = (freq as i32) / 2;
                let (sec_ratio, sec_remainder) = wa_div(elapsed as i32, half_freq);
                (*wrapper).timing_jitter_state ^= sec_ratio & 1;
                let (sp_lo, sp_hi) = time_sub_i32(time_lo, time_hi, sec_remainder);
                (*wrapper).pause_secondary_lo = sp_lo;
                (*wrapper).pause_secondary_hi = sp_hi;
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
                // Original gotos into the else branch → update initial_ref
                (*wrapper).initial_ref_lo = time_lo;
                (*wrapper).initial_ref_hi = time_hi;
            } else {
                elapsed = 0;
                // Original does NOT update initial_ref when delta < 0
            }
        } else {
            (*wrapper).initial_ref_lo = time_lo;
            (*wrapper).initial_ref_hi = time_hi;
        }

        // ── FPU section: compute fps-related values ────────────────────
        //
        // The original uses x87 FPU with 80-bit precision. We use f64 which
        // is close enough for rendering-only timing code.
        //
        // Constant at 0x6797e8 — exact bit pattern from WA.exe data section.
        // Used for exponential decay in render scale computation.
        const RENDER_DECAY: f64 = f64::from_bits(0xC015126E978D4FDF_u64); // ≈ -5.2679

        let elapsed_f = elapsed as f64;
        let freq_f = freq as f64;

        // fps_scaled = (int)(elapsed * 3.75 * 65536 / freq)
        // 16.16 fixed-point ratio. Clamped to 0x1333 when bVar18.
        let mut fps_scaled = (elapsed_f * 3.75 * 65536.0 / freq_f) as i32;
        if fps_scaled > 0x1333 && used_normal_path {
            fps_scaled = 0x1333;
        }

        // fps_product = (int)(elapsed * 7.5 * 65536 / freq)
        // ≈ 2 × fps_scaled (before clamping). Clamped to 0x2666 when bVar18.
        let mut fps_product = (elapsed_f * 7.5 * 65536.0 / freq_f) as i32;
        if fps_product > 0x2666 && used_normal_path {
            fps_product = 0x2666;
        }

        // fixed_render_scale = 0x10000 - (int)(65536 * exp(elapsed * RENDER_DECAY / freq))
        // This is an exponential decay smoothing factor for frame interpolation.
        let fixed_render_scale =
            0x10000 - (65536.0 * (elapsed_f * RENDER_DECAY / freq_f).exp()) as i32;

        // keyboard->vtable[3](keyboard, 0x36) — check minimize request
        let keyboard = (*ddgame).keyboard;
        let kb_vtable = *(keyboard as *const *const u32);
        let vfunc3: unsafe extern "thiscall" fn(*mut u8, u32) -> i32 =
            core::mem::transmute(*kb_vtable.add(3));
        let minimize_request = vfunc3(keyboard as *mut u8, 0x36);
        if minimize_request != 0 {
            let session = *(rb(va::G_GAME_SESSION) as *const *mut GameSession);
            (*session).minimize_request = 1;
        }

        // SetupFrameParams(fps_scaled, fps_product, fixed_render_scale)
        bridge_setup_frame_params(wrapper, fps_scaled, fps_product, fixed_render_scale);

        // ── Compute frame advance parameters ───────────────────────────
        //
        // Two paths based on bVar18 (used_normal_path) and frame_duration vs frame_interval:
        //
        // Simple path (!bVar18 || frame_dur >= frame_int):
        //   advance_ratio = elapsed * 0x10000 / frame_interval
        //   AdvanceFrameCounters(fps_scaled, fixed_render_scale, fps_product,
        //                        fixed_render_scale, advance_ratio)
        //
        // Complex path (bVar18 && frame_dur < frame_int):
        //   Recomputes fps_product and fixed_render_scale from frame_duration * 50.
        //   advance_ratio = elapsed * 0x10000 / frame_duration
        //   AdvanceFrameCounters(fps_scaled, fixed_render_scale, new_fps_product,
        //                        new_fixed_render_scale, advance_ratio)
        let frame_fixed = (elapsed as u64).wrapping_mul(0x10000);

        if used_normal_path && frame_duration < frame_interval {
            // Complex path: game running slower than target speed
            let fd50_f = (frame_duration as f64) * 50.0;
            let new_fps_product = (65536.0 * elapsed_f * 7.5 / fd50_f) as i32;
            let new_render_scale =
                0x10000 - (65536.0 * (elapsed_f * RENDER_DECAY / fd50_f).exp()) as i32;
            let advance_ratio = frame_fixed.checked_div(frame_duration).unwrap_or(0) as u32;
            bridge_advance_frame_counters(
                wrapper,
                fps_scaled,
                fixed_render_scale,
                new_fps_product,
                new_render_scale,
                advance_ratio,
            );
        } else {
            // Simple path: normal speed or replay mode
            let advance_ratio = frame_fixed.checked_div(frame_interval).unwrap_or(0) as u32;
            bridge_advance_frame_counters(
                wrapper,
                fps_scaled,
                fixed_render_scale,
                fps_product,
                fixed_render_scale,
                advance_ratio,
            );
        }

        // UpdateFrameTiming(elapsed_lo, elapsed_hi, freq_lo, freq_hi)
        bridge_update_frame_timing(
            wrapper,
            elapsed as u32,
            (elapsed >> 32) as u32,
            freq_lo,
            freq_hi,
        );

        // sound->vtable[1](sound): DDGame+8 = DSSound*
        let sound = (*ddgame).sound as *mut u8;
        if !sound.is_null() {
            let vtable = *(sound as *const *const u32);
            let vfunc: unsafe extern "thiscall" fn(*mut u8) = core::mem::transmute(*vtable.add(1));
            vfunc(sound);
        }

        // active_sounds: DDGame+0xC — if non-null, call FUN_005464e0
        let active_sounds = (*ddgame).active_sounds as *mut u8;
        if !active_sounds.is_null() {
            let func: unsafe extern "stdcall" fn(*mut u8) =
                core::mem::transmute(rb(0x005464e0) as usize);
            func(active_sounds);
        }

        // display->vtable[2](display): DDGame+4 = DisplayGfx*
        let display = (*ddgame).display as *mut u8;
        let vtable_disp = *(display as *const *const u32);
        let vfunc2: unsafe extern "thiscall" fn(*mut u8) =
            core::mem::transmute(*vtable_disp.add(2));
        vfunc2(display);

        // keyboard->vtable[1](keyboard, 0xd) → store result at DDGame+0x7E9C
        if (*wrapper)._field_410 == 0 {
            let keyboard = (*ddgame).keyboard as *mut u8;
            let kb_vtable = *(keyboard as *const *const u32);
            let vfunc1: unsafe extern "thiscall" fn(*mut u8, u32) -> u32 =
                core::mem::transmute(*kb_vtable.add(1));
            let result = vfunc1(keyboard, 0xd);
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
            // Write u32 at session+0x44 (original: *(undefined4 *)(g_GameSession + 0x44) = 1)
            *((session as *mut u8).add(0x44) as *mut u32) = 1;
        }
    }

    // ── Game-over detection (replay finished) ──────────────────────────
    {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let replay_ticks = *(game_info.add(0xef3c) as *const i32);
        if replay_ticks != 0 {
            let replay_end = *(game_info.add(0xf350) as *const i32);
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

            // Match original unsigned SUB/SBB behavior: this wraps on underflow.
            // The follow-up unsigned compare against `remaining` depends on wrap semantics.
            let budget = frame_duration.wrapping_sub(max_accum);
            let frame_time;
            if budget <= remaining {
                // Budget is smaller or equal — use budget as frame_time
                frame_time = budget;
            } else {
                // Budget > remaining — use remaining, unless game hasn't started
                let gi_f348 = *(game_info.add(0xf348) as *const u8);
                let gi_f344 = *(game_info.add(0xf344) as *const i32);
                if gi_f348 != 0 || (*ddgame).frame_counter < gi_f344 {
                    // Game not started: inflate to budget
                    frame_time = budget;
                    remaining = budget;
                } else {
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
                        &mut frame_step_counter as *mut u32 as *mut u8,
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
                        &mut frame_step_counter as *mut u32 as *mut u8,
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
                &mut frame_step_counter as *mut u32 as *mut u8,
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

    // Original increments wrapper+0xE8 by (step_count - 1) if StepFrame ran.
    // This field is consumed by downstream timing/render code.
    if frame_step_counter != 0 {
        let steps_minus_one = (frame_step_counter as i32).wrapping_sub(1);
        let field_e8 = (wrapper as *mut u8).add(0xE8) as *mut i32;
        *field_e8 = (*field_e8).wrapping_add(steps_minus_one);
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
                {
                    // Update speed ratios from accumulators
                    let accum_a = combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi);
                    let speed_a = accum_a
                        .wrapping_mul(0x10000)
                        .checked_div(frame_duration)
                        .unwrap_or(0) as u32;
                    *((ddgame as *mut u8).add(0x8150) as *mut u32) = speed_a;

                    let accum_b = combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi);
                    let speed_b = accum_b
                        .wrapping_mul(0x10000)
                        .checked_div(frame_duration)
                        .unwrap_or(0) as u32;
                    *((ddgame as *mut u8).add(0x8154) as *mut u32) = speed_b;

                    // Intentional deviation from vanilla: while truly paused,
                    // clamp interpolation scales to 0 to prevent deterministic
                    // backwards render stutter during pause.
                    if is_frame_paused(wrapper) {
                        *((ddgame as *mut u8).add(0x8150) as *mut u32) = 0;
                        *((ddgame as *mut u8).add(0x8154) as *mut u32) = 0;
                    }
                }

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

                        // Rescale accum_c if nonzero (full 64-bit value, not lo dword only).
                        let accum_c =
                            combine((*wrapper).frame_accum_c_lo, (*wrapper).frame_accum_c_hi);
                        if accum_c != 0 {
                            let scaled_c = accum_c
                                .wrapping_mul(scale)
                                .checked_div(frame_duration)
                                .unwrap_or(0);
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
    //
    // In headless mode with logging enabled, formats the current frame
    // counter as a timestamp "HH:MM:SS.CC\n" and writes it to the CRT
    // stdout FILE* via fputs. Calls ExitProcess(-1) on write failure.
    {
        let ddgame = (*wrapper).ddgame;
        let game_info = (*ddgame).game_info as *const u8;
        let headless = *(game_info.add(0xf914) as *const i32);
        let log_enabled = *(game_info.add(0xef38) as *const i32);

        if headless != 0 && log_enabled != 0 {
            use core::fmt::Write;

            let fc = (*ddgame).frame_counter as u32;
            let hours = fc / 180000; // 50fps * 60s * 60m
            let r1 = fc % 180000;
            let minutes = r1 / 3000; // 50fps * 60s
            let r2 = r1 % 3000;
            let seconds = r2 / 50;
            let centiseconds = (r2 % 50) * 100 / 50;

            let mut buf = heapless::String::<32>::new();
            let _ = writeln!(
                buf,
                "{:02}:{:02}:{:02}.{:02}",
                hours, minutes, seconds, centiseconds
            );

            // Write to the same CRT stdout as the original:
            // FUN_005d4e40() returns __iob array, +0x20 = stdout FILE*.
            let iob_func: unsafe extern "C" fn() -> *mut u8 =
                core::mem::transmute(rb(0x005d4e40) as usize);
            let stdout_file = iob_func().add(0x20);

            let fputs: unsafe extern "C" fn(*const u8, *mut u8) -> i32 =
                core::mem::transmute(*(rb(0x00649468) as *const u32) as usize);
            let result = fputs(buf.as_ptr(), stdout_file);

            if result == -1 {
                windows_sys::Win32::System::Threading::ExitProcess(0xFFFFFFFF);
            }

            // Original also checks ferror() on the FILE*
            let ferror: unsafe extern "C" fn(*mut u8) -> i32 =
                core::mem::transmute(rb(0x005d5126) as usize);
            if ferror(stdout_file) != 0 {
                windows_sys::Win32::System::Threading::ExitProcess(0xFFFFFFFF);
            }
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

/// Handle secondary pause detection (LAB_0052928c).
///
/// Computes sec_delta from pause_secondary timestamps, checks bounds,
/// then either resets jitter state or updates via i32 remainder division.
/// The jitter calc at LAB_005292c5 divides sec_delta_lo by freq_lo/2 and
/// uses the REMAINDER (from EDX after IDIV) as the adjustment, matching
/// the same CDQ+SUB+SBB pattern used for pause_detect.
unsafe fn handle_secondary_pause(
    wrapper: *mut DDGameWrapper,
    time_lo: u32,
    time_hi: u32,
    freq: u64,
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
            // LAB_005292c5
            if (*wrapper).timing_jitter_state == 2 {
                // Falls through to LAB_005292d2 (reset)
                (*wrapper).timing_jitter_state = 1;
                (*wrapper).pause_secondary_lo = time_lo;
                (*wrapper).pause_secondary_hi = time_hi;
            } else {
                // Jitter calc at 0x52936c: FUN_005d8786(sec_delta_lo, freq_lo / 2)
                let half_freq = (freq as i32) / 2;
                let (sec_ratio, sec_remainder) = wa_div(sec_delta as i32, half_freq);
                (*wrapper).timing_jitter_state ^= sec_ratio & 1;
                let (sp_lo, sp_hi) = time_sub_i32(time_lo, time_hi, sec_remainder);
                (*wrapper).pause_secondary_lo = sp_lo;
                (*wrapper).pause_secondary_hi = sp_hi;
            }
            return;
        }
    }
    // Delta negative or too large — reset (LAB_005292d2)
    (*wrapper).timing_jitter_state = 1;
    (*wrapper).pause_secondary_lo = time_lo;
    (*wrapper).pause_secondary_hi = time_hi;
}
