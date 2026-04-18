//! Rust port of `DDGameWrapper__StepFrame` (0x529F30).
//!
//! Called by `dispatch_frame` inside the main frame loop. Advances the
//! game by one simulation tick: polls input, dispatches turn-flow
//! messages, updates the `remaining` time budget, and (on game-end
//! frames) writes the headless log.
//!
//! Strategy: the common gameplay path is Rust. Any frame that touches
//! the game-end state machine — phase transitions, dispatch of phase
//! handlers 2/3/4, `ClearWormBuffers`/`AdvanceFrame`, or the headless
//! end-of-game stats log — falls back to the original `bridge_step_frame`.

use crate::address::va;
use crate::engine::ddgame::DDGame;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::dispatch_frame::is_replay_mode;
use crate::engine::game_session::get_game_session;
use crate::rebase::rb;

// ─── Runtime addresses ─────────────────────────────────────────────────────

static mut STEP_FRAME_ADDR: u32 = 0;
static mut POLL_INPUT_ADDR: u32 = 0;
static mut INPUT_HOOK_MODE_ADDR: u32 = 0;
static mut ON_GAME_END_PHASE1_ADDR: u32 = 0;

/// Initialize bridge addresses. Called once at DLL load.
pub unsafe fn init_step_frame_addrs() {
    unsafe {
        STEP_FRAME_ADDR = rb(va::DDGAMEWRAPPER_STEP_FRAME);
        POLL_INPUT_ADDR = rb(va::DDGAMEWRAPPER_POLL_INPUT);
        INPUT_HOOK_MODE_ADDR = rb(va::G_INPUT_HOOK_MODE);
        ON_GAME_END_PHASE1_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_END_PHASE1);
    }
}

// ─── Bridges ───────────────────────────────────────────────────────────────

/// Bridge for the original `DDGameWrapper__StepFrame` (0x529F30).
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

/// FUN_00536270 — game-end phase 1 arm (network-mode scoreboard reset).
/// Usercall(EAX=wrapper), no stack args, plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_on_game_end_phase1(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym ON_GAME_END_PHASE1_ADDR,
        options(att_syntax),
    );
}

// ─── Helpers ───────────────────────────────────────────────────────────────

#[inline(always)]
fn combine(lo: u32, hi: u32) -> u64 {
    (hi as u64) << 32 | lo as u64
}

// ─── step_frame ────────────────────────────────────────────────────────────

/// Rust port of `DDGameWrapper__StepFrame` (0x529F30).
///
/// Returns true if more frames should be processed.
/// `counter_ptr` is a pointer to a local counter incremented whenever
/// input is polled (passed in EAX in the original usercall).
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

        // ── Block A: top state transition ──────────────────────────────
        //
        // Fires when `hud_status_code ∈ {6, 8}` AND
        // `game_end_phase != hud_status_code`. Sets game_end_phase to
        // that value; in the non-network branch also sets game_state=4
        // and broadcasts msg 0x75 to CTaskTurnGame; in the network
        // branch calls the phase-1 scoreboard-reset handler.
        //
        // Self-gated: the condition `game_end_phase != hud_status_code`
        // becomes false after this block writes game_end_phase, so the
        // bridge-fallback won't re-run Block A even if we bail below.
        let hud_code = (*ddgame).hud_status_code;
        if (hud_code == 6 || hud_code == 8) && (*wrapper).game_end_phase != hud_code as u32 {
            (*wrapper).game_end_phase = hud_code as u32;
            if (*ddgame).network_ecx == 0 {
                (*wrapper).game_state = 4; // EXIT_HEADLESS
                (*wrapper).game_end_clear = 0;
                (*wrapper).game_end_speed = 0;
                if game_info.game_version > 0x4c {
                    // Broadcast msg 0x75 via CTaskTurnGame::HandleMessage.
                    // Original pushes (sender=task, msg=0x75, 0, 0).
                    let task = (*wrapper).task_turn_game;
                    crate::task::CTaskTurnGame::handle_message_raw(
                        task,
                        task as *mut crate::task::CTask,
                        0x75,
                        0,
                        core::ptr::null(),
                    );
                }
            } else {
                bridge_on_game_end_phase1(wrapper);
            }
        }

        // Gate: bail to bridge for the remaining unported blocks —
        // phase-handler dispatch (game_end_phase ∈ {2,3,4}) and the
        // end-of-game body (game_state != 0 || game_end_phase == 4).
        // Read state AFTER Block A (which may have mutated both fields).
        let game_end_phase = (*wrapper).game_end_phase;
        let game_state = (*wrapper).game_state;
        if game_state != 0 || game_end_phase == 2 || game_end_phase == 3 || game_end_phase == 4 {
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

        // ── Common path (Blocks B, C, E, F, G, I) ──────────────────────

        // Block B: skip PollInput + session-accum for game_end_phase ∈
        // {1, 2, 6, 7, 9}. Phase 8 does NOT skip (intentional asymmetry
        // with the {6, 8} set Block A tests against). Phase 2 is gated
        // out above, so the check here is against {1, 6, 7, 9}.
        let skip_input = matches!(game_end_phase, 1 | 6 | 7 | 9);
        if !skip_input {
            // PollInput gate: input-hook mode throttles polling via
            // team-arena counters; when no hook is active we always poll.
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
                let accum =
                    combine((*ddgame)._field_8160, (*ddgame)._field_8164).wrapping_add(0x10000);
                (*ddgame)._field_8160 = accum as u32;
                (*ddgame)._field_8164 = (accum >> 32) as u32;
            }
        }

        // game_end_phase dispatch 2/3/4 — gated out above.

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

        // ── Headful-mode sub-call: two no-op vtable slots ──────────────
        if (*ddgame).is_headful != 0 {
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
