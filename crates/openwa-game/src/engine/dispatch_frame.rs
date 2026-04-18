//! Rust port of `DDGameWrapper__DispatchFrame` (0x529160).
//!
//! Main frame timing/simulation dispatcher. Called each frame by
//! `advance_frame`. Computes delta time, decides how many game frames to
//! advance, dispatches them via `StepFrame`, and handles post-frame timing,
//! headless log output, and game-end detection.

use windows_sys::Win32::System::Threading::ExitProcess;

use crate::address::va;
use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::engine::ddgame::DDGame;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_session::get_game_session;
use crate::input::keyboard::DDKeyboard;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;

// ─── Runtime addresses ─────────────────────────────────────────────────────
//
// All sub-functions use `usercall(EAX=this)` or `usercall(ESI=this)` where
// `this` is `*mut DDGameWrapper`. The bridges below set the appropriate
// register, then `JMP`/`CALL` the target. `RET imm16` on each target cleans
// the remaining stdcall params.

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
static mut IS_REPLAY_MODE_ADDR: u32 = 0;
static mut POLL_INPUT_ADDR: u32 = 0;
static mut INPUT_HOOK_MODE_ADDR: u32 = 0;

/// Initialize all bridge addresses. Must be called once at DLL load.
pub unsafe fn init_dispatch_addrs() {
    unsafe {
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
        POLL_INPUT_ADDR = rb(va::DDGAMEWRAPPER_POLL_INPUT);
        INPUT_HOOK_MODE_ADDR = rb(va::G_INPUT_HOOK_MODE);
    }
}

// ─── Bridge helpers ────────────────────────────────────────────────────────

/// Bridge for usercall(EAX=this), no stack params, plain RET.
macro_rules! bridge_eax_this {
    ($name:ident, $addr:expr_2021, $ret:ty) => {
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
    ($name:ident, $addr:expr_2021, ($($param:ty),+) -> $ret:ty) => {
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

bridge_eax_this!(bridge_is_replay_mode, IS_REPLAY_MODE_ADDR, u32);
bridge_eax_this!(bridge_reset_frame_state, RESET_FRAME_STATE_ADDR, ());
bridge_eax_this!(bridge_init_frame_delay, INIT_FRAME_DELAY_ADDR, ());
bridge_eax_this!(bridge_is_frame_paused, IS_FRAME_PAUSED_ADDR, u32);

bridge_eax_this_stdcall!(bridge_setup_frame_params, SETUP_FRAME_PARAMS_ADDR, (i32, i32, i32) -> ());

// ESI=this: ESI is LLVM-reserved on x86, so we can't pass it as an asm
// operand. Naked bridges save/restore ESI manually and re-push params from
// the incoming stack instead of routing through a Rust-side array (which
// LLVM otherwise optimizes into garbage in release builds).

unsafe fn bridge_network_update(wrapper: *mut DDGameWrapper) {
    unsafe {
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
}

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
        "push esi",
        "mov esi, [esp+8]",
        "push dword ptr [esp+0x1C]",
        "push dword ptr [esp+0x1C]",
        "push dword ptr [esp+0x1C]",
        "push dword ptr [esp+0x1C]",
        "push dword ptr [esp+0x1C]",
        "call [{addr}]",
        "pop esi",
        "ret 24",
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
        "push dword ptr [esp+0x18]",
        "push dword ptr [esp+0x18]",
        "push dword ptr [esp+0x18]",
        "push dword ptr [esp+0x18]",
        "call [{addr}]",
        "pop esi",
        "ret 20",
        addr = sym UPDATE_FRAME_TIMING_ADDR,
    );
}

/// Bridge for DDGameWrapper__ProcessNetworkFrame (0x53DF00).
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

/// Bridge for StepFrame (0x529F30).
/// Thiscall: ECX=this, EAX=counter ptr, 5 stack params, RET 0x14.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_step_frame(
    _this: *mut DDGameWrapper,
    _counter_ptr: *mut u32,
    _remaining: *mut u64,
    _time_lo: u32,
    _time_hi: u32,
    _speed_target: i32,
    _speed: i32,
) -> u32 {
    core::arch::naked_asm!(
        "popl %edx",
        "popl %ecx",
        "popl %eax",
        "pushl %edx",
        "jmpl *({fn})",
        fn = sym STEP_FRAME_ADDR,
        options(att_syntax),
    );
}

/// DDGameWrapper__PollInput — stdcall(wrapper), RET 0x4.
unsafe extern "stdcall" fn bridge_poll_input(wrapper: *mut DDGameWrapper) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
            core::mem::transmute(POLL_INPUT_ADDR as usize);
        func(wrapper);
    }
}

/// ShouldContinueFrameLoop: plain stdcall(wrapper, lo, hi), RET 0xC.
unsafe extern "stdcall" fn bridge_should_continue(
    wrapper: *mut DDGameWrapper,
    elapsed_lo: u32,
    elapsed_hi: u32,
) -> u32 {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut DDGameWrapper, u32, u32) -> u32 =
            core::mem::transmute(SHOULD_CONTINUE_ADDR as usize);
        func(wrapper, elapsed_lo, elapsed_hi)
    }
}

// ─── Public bridge wrappers ────────────────────────────────────────────────

pub unsafe fn is_replay_mode(wrapper: *mut DDGameWrapper) -> bool {
    unsafe { bridge_is_replay_mode(wrapper) != 0 }
}

pub unsafe fn should_continue_frame_loop(
    wrapper: *mut DDGameWrapper,
    elapsed_lo: u32,
    elapsed_hi: u32,
) -> bool {
    unsafe { bridge_should_continue(wrapper, elapsed_lo, elapsed_hi) != 0 }
}

pub unsafe fn is_frame_paused(wrapper: *mut DDGameWrapper) -> bool {
    unsafe { bridge_is_frame_paused(wrapper) != 0 }
}

/// Rust port of `DDGameWrapper__StepFrame` (0x529F30).
///
/// Returns true if more frames should be processed.
/// `counter_ptr` is a pointer to a local counter incremented whenever input
/// is polled (passed in EAX in the original usercall).
///
/// Strategy: the common frame-simulation path is Rust. Any frame that
/// touches the game-end state machine (phase transitions, headless log
/// dump, worm cleanup) falls back to the original `bridge_step_frame`.
pub unsafe fn step_frame(
    wrapper: *mut DDGameWrapper,
    counter_ptr: *mut u32,
    remaining: *mut u64,
    frame_duration_lo: u32,
    frame_duration_hi: u32,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    unsafe {
        let ddgame: *mut DDGame = (*wrapper).ddgame;
        let game_info = &*(*ddgame).game_info;

        // Gate: bail to the original bridge for any path that touches the
        // game-end state machine (phase transitions, dispatch of phase
        // handlers 2/3/4, ClearWormBuffers/AdvanceFrame, headless log
        // dump). The common path Rust implements covers normal gameplay
        // frames where game_end_phase == 0 and game_state == 0.
        //
        // The state-transition block at the top of StepFrame fires when
        // `hud_status_code ∈ {6, 8}` AND `game_end_phase != hud_status_code`.
        // Since game_end_phase is 0 during normal play, any hud_status_code
        // of 6 or 8 triggers a transition → bail.
        let game_end_phase = (*wrapper).game_end_phase;
        let game_state = (*wrapper).game_state;
        let hud_code = (*ddgame).hud_status_code;
        if game_end_phase != 0 || game_state != 0 || hud_code == 6 || hud_code == 8 {
            return bridge_step_frame(
                wrapper,
                counter_ptr,
                remaining,
                frame_duration_lo,
                frame_duration_hi,
                game_speed_target,
                game_speed,
            ) != 0;
        }

        // ── Common path (game_end_phase == 0, game_state == 0) ─────────

        // PollInput gate: input-hook mode throttles polling via team-arena
        // counters; when no hook is active we always poll.
        let hook_mode = *(INPUT_HOOK_MODE_ADDR as *const u32);
        let arena = &(*ddgame).team_arena;
        if hook_mode == 0 || arena.active_worm_count <= arena.active_team_count {
            bridge_poll_input(wrapper);
            *counter_ptr = (*counter_ptr).wrapping_add(1);
        }

        // GameSession replay-active: tweak the replay accumulators.
        let session = get_game_session();
        if (*session).replay_active_flag != 0 {
            // render_interp_a (0x8150) -= 0x10000; render_interp_b mirrors it.
            (*ddgame).render_interp_a = (*ddgame).render_interp_a.wrapping_sub(0x10000);
            (*ddgame).render_interp_b = (*ddgame).render_interp_a;
            // 64-bit add at _field_8160: += 0x10000 with carry into _field_8164.
            let accum = combine((*ddgame)._field_8160, (*ddgame)._field_8164).wrapping_add(0x10000);
            (*ddgame)._field_8160 = accum as u32;
            (*ddgame)._field_8164 = (accum >> 32) as u32;
        }

        // game_end_phase dispatch 2/3/4 — skipped (game_end_phase == 0).

        // ── f34c sentinel block #1: conditional 0x7a broadcast ─────────
        //
        // Structure in original: two separate blocks, each doing
        //   if (frame_counter != f34c) f34c = -1;
        //   if (frame_counter == f34c || frame_counter == f344) { ... }
        //
        // During normal play f34c starts at -1 and the first step resets
        // it to -1 whenever it drifts. frame_counter matches f34c only
        // on genuine wrap, which doesn't happen in any practical run,
        // so the left side of the OR is effectively false. f344 is
        // sound_start_frame — equals frame_counter only on the game-start
        // transition frame (and possibly once more if sound_start_frame
        // is re-written late). Handle both branches faithfully.
        let frame_counter = (*ddgame).frame_counter;
        let gi_mut = (*ddgame).game_info;

        if frame_counter != (*gi_mut)._field_f34c {
            (*gi_mut)._field_f34c = -1;
        }
        let sentinel_match =
            frame_counter == (*gi_mut)._field_f34c || frame_counter == (*gi_mut).sound_start_frame;
        if sentinel_match {
            (*wrapper)._field_404 = 1;
            if (*ddgame).fast_forward_active == 0 {
                // Broadcast 0x7a via CTaskTurnGame::HandleMessage.
                // Original pushes (sender=task, msg=0x7a, 0, 0).
                let task = (*wrapper).task_turn_game;
                crate::task::CTaskTurnGame::handle_message_raw(
                    task,
                    task as *mut crate::task::CTask,
                    0x7a,
                    0,
                    core::ptr::null(),
                );
            }
        }

        // ── f34c sentinel block #2: `remaining` adjust ─────────────────
        if frame_counter != (*gi_mut)._field_f34c {
            (*gi_mut)._field_f34c = -1;
        }
        let sentinel_match_2 =
            frame_counter == (*gi_mut)._field_f34c || frame_counter == (*gi_mut).sound_start_frame;
        if sentinel_match_2 && (*wrapper).frame_delay_counter >= 0 {
            *remaining = 0;
        } else if game_info.sound_mute == 0 && game_info.sound_start_frame <= frame_counter {
            let rem = *remaining;
            let sub = combine(frame_duration_lo, frame_duration_hi);
            *remaining = rem.wrapping_sub(sub);
        }

        // ── Sound-available sub-call: two no-op vtable slots ───────────
        if (*ddgame).sound_available != 0 {
            let keyboard = (*ddgame).keyboard;
            crate::input::keyboard::DDKeyboard::slot_06_noop_raw(keyboard);
            let palette = (*ddgame).palette;
            crate::render::display::palette::Palette::reset_raw(palette);
        }

        // ── End-of-game branch: skipped by gate ────────────────────────

        // ── Return: IsReplayMode || speeds unchanged ───────────────────
        if is_replay_mode(wrapper) {
            return true;
        }
        let cur_target = (*ddgame).game_speed_target.to_raw();
        let cur_speed = (*ddgame).game_speed.to_raw();
        game_speed_target == cur_target && game_speed == cur_speed
    }
}

// ─── Port of DDGameWrapper__CalcTimingRatio (0x52ABF0) ─────────────────────
//
// Adjusts `wrapper.turn_timer_max` (progress) toward `wrapper.turn_timer_current`
// (target). These two fields double as turn-timer state during gameplay and
// as the "ratio smoother" during frame timing; they share memory but are
// written in distinct phases.
unsafe fn calc_timing_ratio(wrapper: *mut DDGameWrapper, ratio: i32) {
    unsafe {
        let ddgame = (*wrapper).ddgame;
        let gi = &*(*ddgame).game_info;

        let sound_started = gi._field_f398 == 0
            && gi.sound_mute == 0
            && gi.sound_start_frame <= (*ddgame).frame_counter;

        if sound_started {
            if ratio != 0 {
                let target = (*wrapper).turn_timer_current;
                let progress = (*wrapper).turn_timer_max;
                let gap = target - progress;
                if gap > 0 {
                    let multiplier = if gap / 5 > 1 { 2 } else { 1 };
                    let step = multiplier * ratio;
                    (*wrapper).turn_timer_max = if gap <= step { target } else { progress + step };
                    (*wrapper)._field_404 = 1;
                }
            }
        } else {
            let target = (*wrapper).turn_timer_current;
            let progress = (*wrapper).turn_timer_max;
            if progress != target {
                (*wrapper).turn_timer_max = target;
                (*wrapper)._field_404 = 1;
            }
        }
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Read the current time using the same method as the GameSession timer.
/// Returns (time_lo, time_hi) matching the convention of AdvanceFrame's params.
unsafe fn read_current_time() -> (u32, u32) {
    unsafe {
        let session = get_game_session();
        if (*session).timer_freq_lo == 0 && (*session).timer_freq_hi == 0 {
            let tick = windows_sys::Win32::System::SystemInformation::GetTickCount();
            (tick.wrapping_mul(1000), 0)
        } else {
            let mut qpc: i64 = 0;
            windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut qpc);
            (qpc as u32, (qpc >> 32) as u32)
        }
    }
}

#[inline(always)]
fn combine(lo: u32, hi: u32) -> u64 {
    (hi as u64) << 32 | lo as u64
}

#[inline(always)]
fn split(v: u64) -> (u32, u32) {
    (v as u32, (v >> 32) as u32)
}

#[inline(always)]
fn time_sub(a_lo: u32, a_hi: u32, b_lo: u32, b_hi: u32) -> u64 {
    combine(a_lo, a_hi).wrapping_sub(combine(b_lo, b_hi))
}

/// Signed 32-bit division matching WA's FUN_005d8786.
/// Returns (quotient, remainder) using only the low 32 bits of the dividend.
/// The original uses x86 IDIV which produces both values.
#[inline(always)]
fn wa_div(dividend_lo: i32, divisor: i32) -> (i32, i32) {
    (dividend_lo / divisor, dividend_lo % divisor)
}

/// Subtract a sign-extended i32 remainder from a 64-bit timestamp.
/// Matches the CDQ + SUB + SBB pattern used after FUN_005d8786 in DispatchFrame.
#[inline(always)]
fn time_sub_i32(time_lo: u32, time_hi: u32, remainder: i32) -> (u32, u32) {
    let rem_u32 = remainder as u32;
    let sign_hi = (remainder >> 31) as u32;
    let lo = time_lo.wrapping_sub(rem_u32);
    let hi = time_hi
        .wrapping_sub(sign_hi)
        .wrapping_sub(if time_lo < rem_u32 { 1 } else { 0 });
    (lo, hi)
}

// ─── Main dispatch function ────────────────────────────────────────────────

/// Rust port of `DDGameWrapper__DispatchFrame` (0x529160).
///
/// Computes delta time, determines how many game frames to advance,
/// dispatches them via `StepFrame`, and handles post-frame timing updates,
/// headless log output, and game-end detection.
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
    unsafe {
        let freq = combine(freq_lo, freq_hi);
        let mut frame_step_counter: u32 = 0;

        let frame_interval = freq / 50;

        let ddgame: *mut DDGame = (*wrapper).ddgame;
        let game_speed_target = (*ddgame).game_speed_target.to_raw();
        let game_speed = (*ddgame).game_speed.to_raw();

        // Actual ticks per frame, scaled for current game speed.
        let frame_duration = ((game_speed as i64).wrapping_mul(frame_interval as i64)
            / (game_speed_target as i64)) as u64;

        let saved_frame_delay = (*wrapper).frame_delay_counter;
        let saved_game_speed = game_speed;

        let has_sound = (*ddgame).sound_available != 0;
        let mut elapsed: u64 = 0;
        // `bVar18` in the decompile: true when we took the "normal" timing
        // branch. Gates the secondary-pause fallthrough and the `elapsed`
        // computation from `initial_ref`.
        let mut used_normal_path = false;

        if has_sound {
            if (*wrapper).pause_detect_lo == 0 && (*wrapper).pause_detect_hi == 0 {
                (*wrapper).pause_detect_lo = time_lo;
                (*wrapper).pause_detect_hi = time_hi;
                (*wrapper).pause_secondary_lo = time_lo;
                (*wrapper).pause_secondary_hi = time_hi;
            }

            let is_replay = is_replay_mode(wrapper);

            if !is_replay || saved_frame_delay >= 0 {
                let delta = time_sub(
                    time_lo,
                    time_hi,
                    (*wrapper).pause_detect_lo,
                    (*wrapper).pause_detect_hi,
                );
                used_normal_path = true;

                let quarter_freq = freq / 4;
                if (delta as i64) >= 0 && delta <= quarter_freq {
                    let gi = &*(*ddgame).game_info;
                    let gi_speed = gi.game_speed_config;
                    // Path A: speed unchanged — divide by frame_interval.
                    // Path B: speed changed — divide by frame_duration.
                    let divisor = if game_speed_target == gi_speed {
                        frame_interval as i32
                    } else {
                        frame_duration as i32
                    };
                    let (ratio, remainder) = wa_div(delta as i32, divisor);
                    calc_timing_ratio(wrapper, ratio);
                    let (pd_lo, pd_hi) = time_sub_i32(time_lo, time_hi, remainder);
                    (*wrapper).pause_detect_lo = pd_lo;
                    (*wrapper).pause_detect_hi = pd_hi;
                    handle_secondary_pause(wrapper, time_lo, time_hi, freq);
                } else {
                    // Delta out of range — resync pause detection.
                    (*wrapper).pause_detect_lo = time_lo;
                    (*wrapper).pause_detect_hi = time_hi;
                    handle_secondary_pause(wrapper, time_lo, time_hi, freq);
                }
            } else {
                // Replay mode with negative frame delay: derive elapsed from the
                // replay tick rate and skip secondary-pause handling.
                let gi = &*(*ddgame).game_info;
                elapsed = freq / (gi.replay_ticks as u64);
                let (ratio, remainder) = wa_div(elapsed as i32, frame_interval as i32);
                calc_timing_ratio(wrapper, ratio);
                let (pd_lo, pd_hi) = time_sub_i32(time_lo, time_hi, remainder);
                (*wrapper).pause_detect_lo = pd_lo;
                (*wrapper).pause_detect_hi = pd_hi;

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

            if (*wrapper).initial_ref_lo == 0 && (*wrapper).initial_ref_hi == 0 {
                (*wrapper).initial_ref_lo = time_lo;
                (*wrapper).initial_ref_hi = time_hi;
            }

            if used_normal_path {
                let init_delta = time_sub(
                    time_lo,
                    time_hi,
                    (*wrapper).initial_ref_lo,
                    (*wrapper).initial_ref_hi,
                );
                if (init_delta as i64) >= 0 {
                    elapsed = init_delta;
                    (*wrapper).initial_ref_lo = time_lo;
                    (*wrapper).initial_ref_hi = time_hi;
                } else {
                    elapsed = 0;
                    // Original intentionally does NOT update initial_ref when
                    // the delta is negative.
                }
            } else {
                (*wrapper).initial_ref_lo = time_lo;
                (*wrapper).initial_ref_hi = time_hi;
            }

            // FPU section: the original x87 code uses 80-bit precision; f64 is
            // close enough for rendering-only timing. The 0x6797e8 constant
            // (exact bit pattern from the data section) drives exponential
            // decay smoothing for frame interpolation.
            const RENDER_DECAY: f64 = f64::from_bits(0xC015126E978D4FDF_u64); // ≈ -5.2679

            let elapsed_f = elapsed as f64;
            let freq_f = freq as f64;

            // fps_scaled ≈ fps_product / 2 (before clamping).
            let mut fps_scaled = (elapsed_f * 3.75 * 65536.0 / freq_f) as i32;
            if fps_scaled > 0x1333 && used_normal_path {
                fps_scaled = 0x1333;
            }
            let mut fps_product = (elapsed_f * 7.5 * 65536.0 / freq_f) as i32;
            if fps_product > 0x2666 && used_normal_path {
                fps_product = 0x2666;
            }
            let fixed_render_scale =
                0x10000 - (65536.0 * (elapsed_f * RENDER_DECAY / freq_f).exp()) as i32;

            // Minimize request: keyboard slot 3 polls the "minimize" action.
            let keyboard: *mut DDKeyboard = (*ddgame).keyboard;
            if ((*(*keyboard).vtable).is_action_active)(keyboard, 0x36) != 0 {
                let session = get_game_session();
                (*session).minimize_request = 1;
            }

            bridge_setup_frame_params(wrapper, fps_scaled, fps_product, fixed_render_scale);

            // AdvanceFrameCounters: two branches differ only in how the product
            // and render-scale are computed when the game is running slower than
            // the target speed.
            let frame_fixed = elapsed.wrapping_mul(0x10000);
            if used_normal_path && frame_duration < frame_interval {
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

            bridge_update_frame_timing(
                wrapper,
                elapsed as u32,
                (elapsed >> 32) as u32,
                freq_lo,
                freq_hi,
            );

            // DSSound::update_channels (slot 1).
            let sound: *mut DSSound = (*ddgame).sound;
            if !sound.is_null() {
                ((*(*sound).vtable).update_channels)(sound);
            }

            // Streaming/active-sound tick.
            let active_sounds: *mut ActiveSoundTable = (*ddgame).active_sounds;
            if !active_sounds.is_null() {
                let active_update: unsafe extern "stdcall" fn(*mut ActiveSoundTable) =
                    core::mem::transmute(rb(va::ACTIVE_SOUND_TABLE_UPDATE) as usize);
                active_update(active_sounds);
            }

            // DisplayGfx slot 2: noop on the stock vtable (shared `ret` stub),
            // kept in case WormKit or another hook replaces it.
            let display: *mut DisplayGfx = (*ddgame).display;
            ((*(*display).base.vtable).slot_02_noop)(display);

            if (*wrapper)._field_410 == 0 {
                // Keyboard slot 1: edge-triggered action poll; result is cached
                // on DDGame for downstream HUD/input code.
                let result = ((*(*keyboard).vtable).is_action_pressed)(keyboard, 0xd);
                (*ddgame).kb_poll_result = result as u32;
            }
        }
        // end of has_sound block

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

        let mut remaining: u64 = if ref_delta < 0 {
            0
        } else {
            let quarter_freq = freq / 4;
            let four_frames = frame_duration.wrapping_mul(4);
            let max_remaining = quarter_freq.max(four_frames);
            (ref_delta as u64).min(max_remaining)
        };

        // Frame delay handling.
        let frame_delay = (*wrapper).frame_delay_counter;
        if frame_delay >= 0 {
            let gi = &*(*ddgame).game_info;
            if gi.sound_mute == 0 && gi.sound_start_frame <= (*ddgame).frame_counter {
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

        let (now_lo, now_hi) = read_current_time();
        let loop_elapsed = time_sub(
            now_lo,
            now_hi,
            (*wrapper).last_frame_time_lo,
            (*wrapper).last_frame_time_hi,
        );

        if (*ddgame).network_ecx != 0 {
            bridge_process_network_frame(wrapper, time_lo, time_hi, freq_lo, freq_hi);
        }

        // Clamp `remaining` for replay/network catch-up. The latch at
        // `G_DISPATCH_FRAME_LATCH` gates this on the second-and-later frame.
        {
            let gi = &*(*ddgame).game_info;
            let frame_latch = rb(va::G_DISPATCH_FRAME_LATCH) as *mut u8;
            if (gi.sound_mute != 0 || (*ddgame).frame_counter < gi.sound_start_frame)
                && remaining < frame_duration
                && *frame_latch != 0
            {
                remaining = frame_duration;
            }
            *frame_latch = 1;
        }

        // Replay mode speed adjustment.
        if is_replay_mode(wrapper) {
            let frame_delay = (*wrapper).frame_delay_counter;
            if frame_delay != 0 {
                if frame_delay > 0 {
                    (*wrapper).frame_delay_counter = frame_delay - 1;
                }
                let gi = &*(*ddgame).game_info;
                let replay_ticks = gi.replay_ticks;
                let speed_accum = combine((*ddgame)._field_8158, (*ddgame)._field_815c);
                let speed_val =
                    (speed_accum / replay_ticks as u64) as i32 - (*ddgame)._field_8160 as i32;
                (*ddgame).render_interp_a = speed_val;
                (*ddgame).render_interp_b = speed_val;

                // Advance the accumulator by 0x320000 (one replay step).
                let accum_ptr = core::ptr::addr_of_mut!((*ddgame)._field_8158) as *mut u64;
                *accum_ptr = (*accum_ptr).wrapping_add(0x320000);

                let session = get_game_session();
                (*session).replay_active_flag = 1;
            }
        }

        // Game-over detection (replay finished).
        {
            let gi = &*(*ddgame).game_info;
            if gi.replay_ticks != 0 {
                let frame_counter = (*ddgame).frame_counter;
                let replay_end = gi.replay_end_frame;
                let speed_val = (*ddgame).render_interp_a;

                if (frame_counter > replay_end || (frame_counter == replay_end && speed_val > 0))
                    && (*wrapper).game_end_phase != 1
                {
                    (*wrapper).game_end_phase = 1;
                    (*wrapper).game_end_speed = 0x10000;
                    (*wrapper).game_state = 5; // EXIT
                }
            }
        }

        // Main frame loop.
        loop {
            let gi = &*(*ddgame).game_info;
            let replay_ticks = gi.replay_ticks;

            if replay_ticks == 0 {
                if remaining == 0 {
                    break;
                }

                let accum_a = combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi);
                let accum_b = combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi);
                let max_accum = accum_a.max(accum_b);

                // Matches original unsigned SUB/SBB: wraps on underflow, and the
                // follow-up compare against `remaining` depends on wrap.
                let budget = frame_duration.wrapping_sub(max_accum);
                let frame_time;
                if budget <= remaining {
                    frame_time = budget;
                } else {
                    // Game not yet started → inflate to budget.
                    if gi.sound_mute != 0 || (*ddgame).frame_counter < gi.sound_start_frame {
                        frame_time = budget;
                        remaining = budget;
                    } else {
                        frame_time = remaining;
                    }
                }
                let (ft_lo, ft_hi) = split(frame_time);

                let session = get_game_session();

                if (*session).flag_5c == 0 || (*ddgame).network_ecx != 0 {
                    let accum_b_new =
                        combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi)
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
                    let accum_a_new =
                        combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi)
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
                        if !step_frame(
                            wrapper,
                            &mut frame_step_counter,
                            &mut remaining,
                            ft_lo,
                            ft_hi,
                            game_speed_target,
                            saved_game_speed,
                        ) {
                            break;
                        }
                    } else {
                        let gi = &*(*ddgame).game_info;
                        if gi.sound_mute == 0 && (*ddgame).frame_counter >= gi.sound_start_frame {
                            remaining = remaining.wrapping_sub(frame_time);
                        }
                    }
                } else {
                    let accum_c_new =
                        combine((*wrapper).frame_accum_c_lo, (*wrapper).frame_accum_c_hi)
                            .wrapping_add(frame_time);
                    (*wrapper).frame_accum_c_lo = accum_c_new as u32;
                    (*wrapper).frame_accum_c_hi = (accum_c_new >> 32) as u32;

                    if accum_c_new >= frame_duration {
                        let excess = accum_c_new - frame_duration;
                        (*wrapper).frame_accum_c_lo = excess as u32;
                        (*wrapper).frame_accum_c_hi = (excess >> 32) as u32;
                        if !step_frame(
                            wrapper,
                            &mut frame_step_counter,
                            &mut remaining,
                            ft_lo,
                            ft_hi,
                            game_speed_target,
                            saved_game_speed,
                        ) {
                            break;
                        }
                    } else {
                        let gi = &*(*ddgame).game_info;
                        if gi.sound_mute == 0 && (*ddgame).frame_counter >= gi.sound_start_frame {
                            remaining = remaining.wrapping_sub(frame_time);
                        }
                    }
                }
            } else {
                // Replay frame dispatch.
                let speed_val = (*ddgame).render_interp_a;
                if speed_val < 0x10000 {
                    break;
                }

                let session = get_game_session();
                if (*session).flag_5c == 0 || (*ddgame).network_ecx != 0 {
                    bridge_reset_frame_state(wrapper);
                }
                if !step_frame(
                    wrapper,
                    &mut frame_step_counter,
                    &mut remaining,
                    frame_duration as u32,
                    (frame_duration >> 32) as u32,
                    game_speed_target,
                    saved_game_speed,
                ) {
                    break;
                }
            }

            if !should_continue_frame_loop(
                wrapper,
                loop_elapsed as u32,
                (loop_elapsed >> 32) as u32,
            ) {
                break;
            }
        }

        // Original: `wrapper.step_count_accum += step_count - 1` if StepFrame ran.
        if frame_step_counter != 0 {
            let steps_minus_one = (frame_step_counter as i32).wrapping_sub(1);
            (*wrapper).step_count_accum = (*wrapper).step_count_accum.wrapping_add(steps_minus_one);
        }

        // Post-frame timing updates.
        {
            let gi = &*(*ddgame).game_info;
            let replay_ticks = gi.replay_ticks;

            if replay_ticks == 0 {
                if gi.sound_mute == 0 && gi.sound_start_frame <= (*ddgame).frame_counter {
                    let accum_a = combine((*wrapper).frame_accum_a_lo, (*wrapper).frame_accum_a_hi);
                    let speed_a = accum_a
                        .wrapping_mul(0x10000)
                        .checked_div(frame_duration)
                        .unwrap_or(0) as i32;
                    (*ddgame).render_interp_a = speed_a;

                    let accum_b = combine((*wrapper).frame_accum_b_lo, (*wrapper).frame_accum_b_hi);
                    let speed_b = accum_b
                        .wrapping_mul(0x10000)
                        .checked_div(frame_duration)
                        .unwrap_or(0) as i32;
                    (*ddgame).render_interp_b = speed_b;

                    // Intentional deviation from vanilla: while truly paused,
                    // clamp interpolation scales to 0 to prevent deterministic
                    // backwards render stutter during pause.
                    if is_frame_paused(wrapper) {
                        (*ddgame).render_interp_a = 0;
                        (*ddgame).render_interp_b = 0;
                    }

                    if (*wrapper).frame_delay_counter >= 0 {
                        (*wrapper).frame_accum_a_lo = 0;
                        (*wrapper).frame_accum_a_hi = 0;
                        (*wrapper).frame_accum_b_lo = 0;
                        (*wrapper).frame_accum_b_hi = 0;
                        (*wrapper).frame_accum_c_lo = 0;
                        (*wrapper).frame_accum_c_hi = 0;
                    }

                    let new_target = (*ddgame).game_speed_target.to_raw();
                    if game_speed_target != new_target
                        || saved_game_speed != (*ddgame).game_speed.to_raw()
                    {
                        if saved_frame_delay >= 0 && (*wrapper).frame_delay_counter < 0 {
                            // Speed change while frame delay was active → reset.
                            (*wrapper).frame_accum_a_lo = 0;
                            (*wrapper).frame_accum_a_hi = 0;
                            (*wrapper).frame_accum_b_lo = 0;
                            (*wrapper).frame_accum_b_hi = 0;
                            (*wrapper).frame_accum_c_lo = 0;
                            (*wrapper).frame_accum_c_hi = 0;
                            (*ddgame).render_interp_a = 0;
                            (*ddgame).render_interp_b = 0;
                        } else {
                            let new_interval = freq / 50;
                            let new_speed = (*ddgame).game_speed.to_raw();
                            let scale = ((new_speed as i64).wrapping_mul(new_interval as i64)
                                / (new_target as i64))
                                as u64;

                            let scaled_a = (((*ddgame).render_interp_a as i64)
                                .wrapping_mul(scale as i64)
                                >> 16) as u64;
                            (*wrapper).frame_accum_a_lo = scaled_a as u32;
                            (*wrapper).frame_accum_a_hi = (scaled_a >> 32) as u32;

                            let scaled_b = (((*ddgame).render_interp_b as i64)
                                .wrapping_mul(scale as i64)
                                >> 16) as u64;
                            (*wrapper).frame_accum_b_lo = scaled_b as u32;
                            (*wrapper).frame_accum_b_hi = (scaled_b >> 32) as u32;

                            // Rescale accum_c (full 64-bit, not just lo dword).
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
                } else {
                    // Before game start — zero speed.
                    (*ddgame).render_interp_a = 0;
                    (*ddgame).render_interp_b = 0;
                }
                (*wrapper).timing_ref_lo = time_lo;
                (*wrapper).timing_ref_hi = time_hi;
            } else {
                // Replay mode — subtract remaining from reference.
                let (rem_lo, rem_hi) = split(remaining);
                (*wrapper).timing_ref_lo = time_lo.wrapping_sub(rem_lo);
                (*wrapper).timing_ref_hi = (time_hi as i64)
                    .wrapping_sub((rem_hi as i32 >> 31) as i64)
                    .wrapping_sub(if time_lo < rem_lo { 1 } else { 0 })
                    as u32;
            }
        }

        let (now_lo, now_hi) = read_current_time();
        (*wrapper).last_frame_time_lo = now_lo;
        (*wrapper).last_frame_time_hi = now_hi;

        // Headless log output: format the current frame counter as "HH:MM:SS.CC\n"
        // and write to CRT stdout. ExitProcess(-1) on write failure.
        {
            let gi = &*(*ddgame).game_info;
            if gi.headless_mode != 0 && gi.headless_log_enabled != 0 {
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

                let iob_func: unsafe extern "C" fn() -> *mut u8 =
                    core::mem::transmute(rb(va::CRT_IOB_FUNC) as usize);
                let stdout_file = iob_func().add(0x20);

                let fputs: unsafe extern "C" fn(*const u8, *mut u8) -> i32 =
                    core::mem::transmute(*(rb(va::CRT_FPUTS_IAT) as *const u32) as usize);
                let result = fputs(buf.as_ptr(), stdout_file);

                if result == -1 {
                    ExitProcess(0xFFFFFFFF);
                }

                let ferror: unsafe extern "C" fn(*mut u8) -> i32 =
                    core::mem::transmute(rb(va::CRT_FERROR) as usize);
                if ferror(stdout_file) != 0 {
                    ExitProcess(0xFFFFFFFF);
                }
            }
        }

        if (*(*wrapper).ddgame).sound_available != 0 {
            bridge_network_update(wrapper);
        }

        // Game-end detection via HomeLock.
        //
        // The original compiles this as `cmp word [gi+0xF3B0], ax` — a 16-bit
        // read — but `home_lock` is authoritatively a `u8`: `LoadOptions` writes
        // only the low byte, nothing else writes 0xF3B0/0xF3B1, and the struct
        // is zero-initialised. Reading as `u8` is bit-identical to the original.
        {
            let gi = &*(*ddgame).game_info;
            let home_lock = gi.home_lock as i32;
            if home_lock != 0
                && home_lock < (*ddgame)._field_77d4 as i32 / 50
                && (*wrapper).game_end_phase == 0
            {
                (*wrapper).game_state = 4; // EXIT_HEADLESS
                (*wrapper).game_end_clear = 0;
                (*wrapper).game_end_speed = 0;

                if gi.game_version > 0x4c {
                    // Broadcast game-end message via CTaskTurnGame::HandleMessage (vtable[2]).
                    // Original (0x529F00): ECX=task, stack = [sender=task, msg=0x75, size=0, data=0].
                    let task = (*wrapper).task_turn_game;
                    crate::task::CTaskTurnGame::handle_message_raw(
                        task,
                        task as *mut crate::task::CTask,
                        0x75,
                        0,
                        core::ptr::null(),
                    );
                }
                (*wrapper).game_end_phase = 1;
            }
        }
    }
}

// ─── Internal helpers ──────────────────────────────────────────────────────

/// Secondary pause detection (LAB_0052928c in the decompile).
///
/// Computes sec_delta from pause_secondary, bounds-checks it, then either
/// resets `timing_jitter_state` or XORs it with the low bit of
/// `sec_delta / (freq/2)`. The `CDQ+SUB+SBB` pattern after the IDIV is
/// modelled by `time_sub_i32`.
unsafe fn handle_secondary_pause(
    wrapper: *mut DDGameWrapper,
    time_lo: u32,
    time_hi: u32,
    freq: u64,
) {
    unsafe {
        let sec_delta = time_sub(
            time_lo,
            time_hi,
            (*wrapper).pause_secondary_lo,
            (*wrapper).pause_secondary_hi,
        );

        if (sec_delta as i64) >= 0 && sec_delta <= freq.wrapping_mul(2) {
            if (*wrapper).timing_jitter_state == 2 {
                (*wrapper).timing_jitter_state = 1;
                (*wrapper).pause_secondary_lo = time_lo;
                (*wrapper).pause_secondary_hi = time_hi;
            } else {
                let half_freq = (freq as i32) / 2;
                let (sec_ratio, sec_remainder) = wa_div(sec_delta as i32, half_freq);
                (*wrapper).timing_jitter_state ^= sec_ratio & 1;
                let (sp_lo, sp_hi) = time_sub_i32(time_lo, time_hi, sec_remainder);
                (*wrapper).pause_secondary_lo = sp_lo;
                (*wrapper).pause_secondary_hi = sp_hi;
            }
            return;
        }
        // Delta negative or too large — reset.
        (*wrapper).timing_jitter_state = 1;
        (*wrapper).pause_secondary_lo = time_lo;
        (*wrapper).pause_secondary_hi = time_hi;
    }
}
