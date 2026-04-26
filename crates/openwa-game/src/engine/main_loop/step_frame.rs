//! Rust port of `GameRuntime__StepFrame` (0x529F30).
//!
//! Called by `dispatch_frame` inside the main frame loop. Advances the
//! game by one simulation tick: polls input, runs the end-of-game state
//! machine, updates the `remaining` time budget, and (on end-of-game
//! frames with a headless log enabled) writes the end-of-round stats block.
//!
//! Block layout follows the original:
//! - **A**: top state transition (`hud_status_code ∈ {6, 8}` → phase/state arm).
//! - **B**: PollInput + GameSession replay accumulators. Skipped when
//!   `game_end_phase ∈ {1, 2, 6, 7, 9}`.
//! - **D**: end-game state dispatch keyed on `wrapper.game_state`:
//!   - `NETWORK_END_AWAITING_PEERS` → `GameRuntime__OnGameState2` (usercall EDI=ESI=wrapper)
//!   - `NETWORK_END_STARTED` → `GameRuntime__OnGameState3` (usercall EDI=ESI=wrapper)
//!   - `ROUND_ENDING` → `GameRuntime__OnGameState4` (usercall ESI=wrapper)
//! - **E/F**: two `_field_f34c` sentinel blocks — broadcast msg 0x7A and
//!   reset/adjust `remaining`.
//! - **G**: headful-only keyboard/palette vtable slot calls.
//! - **H**: end-of-round body. Fires when `game_state == ROUND_ENDING || phase != 0`.
//!   Runs `ClearWormBuffers(-1)`, `AdvanceWormFrame`, and — if the headless
//!   log stream is non-null — writes the inline end-of-round stats block.
//! - **Return**: `IsReplayMode() || (speed_target,speed) unchanged` on the
//!   non-H path; `false` unconditionally when the log block was written
//!   (disasm 0x52A76E / 0x52A7E3 `XOR AL, AL`). The forced-false after the
//!   log is what lets headless `/getlog` runs terminate after one log
//!   emission — `ProcessFrame` sets `exit_flag` when `advance_frame()`
//!   returns `ROUND_ENDING`.

use core::ffi::{c_char, c_void};

use openwa_core::fixed::{Fixed, Fixed64};

use super::dispatch_frame::is_replay_mode;
use crate::address::va;
use crate::engine::game_info::GameInfo;
use crate::engine::game_session::get_game_session;
use crate::engine::game_state;
use crate::engine::log_sink::LogOutput;
use crate::engine::net_session::NetSession;
use crate::engine::runtime::GameRuntime;
use crate::engine::world::GameWorld;
use crate::frontend::input_hooks::InputHookMode;
use crate::game::message::{TurnEndMaybeMessage, Unknown122Message};
use crate::input::buffer_object::BufferObject;
use crate::input::keyboard::DDKeyboard;
use crate::rebase::rb;
use crate::render::display::palette::Palette;
use crate::task::WorldRootEntity;
use crate::wa::string_resource::{StringRes, res, wa_load_string};

// ─── Runtime addresses (resolved at DLL load) ──────────────────────────────

static mut POLL_INPUT_ADDR: u32 = 0;
static mut BEGIN_NETWORK_GAME_END_ADDR: u32 = 0;
static mut CLEAR_WORM_BUFFERS_ADDR: u32 = 0;
static mut ADVANCE_WORM_FRAME_ADDR: u32 = 0;
static mut DISPATCH_INPUT_MSG_ADDR: u32 = 0;

/// Initialize bridge addresses. Called once at DLL load.
pub unsafe fn init_step_frame_addrs() {
    unsafe {
        POLL_INPUT_ADDR = rb(va::GAME_RUNTIME_POLL_INPUT);
        BEGIN_NETWORK_GAME_END_ADDR = rb(va::GAME_RUNTIME_BEGIN_NETWORK_GAME_END);
        CLEAR_WORM_BUFFERS_ADDR = rb(va::GAME_RUNTIME_CLEAR_WORM_BUFFERS);
        ADVANCE_WORM_FRAME_ADDR = rb(va::GAME_RUNTIME_ADVANCE_WORM_FRAME);
        DISPATCH_INPUT_MSG_ADDR = rb(va::GAME_RUNTIME_DISPATCH_INPUT_MSG);
    }
}

// ─── Phase / end-game state bridges ────────────────────────────────────────

/// GameRuntime__PollInput — stdcall(runtime), RET 0x4.
unsafe extern "stdcall" fn bridge_poll_input(runtime: *mut GameRuntime) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut GameRuntime) =
            core::mem::transmute(POLL_INPUT_ADDR as usize);
        func(runtime);
    }
}

/// `GameRuntime__BeginNetworkGameEnd` (0x00536270) — network-mode entry
/// from Block A when `network_ecx != 0`. Usercall(EAX=wrapper), plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_begin_network_game_end(_runtime: *mut GameRuntime) {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym BEGIN_NETWORK_GAME_END_ADDR,
        options(att_syntax),
    );
}

/// Shared prologue for `OnGameState3` / `OnNetworkEndAwaitPeers`:
/// decrement `net_end_countdown` toward zero, then decide whether peers
/// are still converging (sync-in-progress OR any peer score > 0). Returns
/// `true` if we must keep waiting: peers not yet converged AND countdown
/// has not yet hit zero.
#[inline]
unsafe fn peer_sync_keep_waiting(runtime: *mut GameRuntime, net: *mut NetSession) -> bool {
    unsafe {
        let cd = (*runtime).net_end_countdown;
        if cd != 0 {
            (*runtime).net_end_countdown = cd - 1;
        }

        let stalled = ((*(*net).vtable).sync_in_progress)(net) != 0
            || NetSession::max_peer_score_raw(net) != 0;

        stalled && (*runtime).net_end_countdown != 0
    }
}

/// Common tail: enter `ROUND_ENDING`, reset the round-end counters, and
/// broadcast msg 0x75 to the turn-game task (new versions only).
#[inline]
unsafe fn enter_round_ending(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        (*runtime).game_state = game_state::ROUND_ENDING;
        (*runtime).game_end_clear = 0;
        (*runtime).game_end_speed = 0;

        let gi = &*(*world).game_info;
        if gi.game_version > 0x4c {
            let task = (*runtime).world_root;
            WorldRootEntity::handle_typed_message_raw(task, task, TurnEndMaybeMessage);
        }
    }
}

/// Rust port of `GameRuntime__OnGameState3` (0x00536320).
///
/// Handles `game_state == NETWORK_END_STARTED`. Waits for peers to acknowledge
/// the end-of-round via `NetSession`, with a bounded countdown — once peers
/// converge or `net_end_countdown` hits zero, transitions to `ROUND_ENDING`.
unsafe fn on_game_state_3(runtime: *mut GameRuntime) {
    unsafe {
        let net = (*(*runtime).world).net_session;
        if peer_sync_keep_waiting(runtime, net) {
            return;
        }
        enter_round_ending(runtime);
    }
}

/// Rust port of `GameRuntime__OnNetworkEndAwaitPeers` (0x00536470).
///
/// Handles `game_state == NETWORK_END_AWAITING_PEERS`. Same shape as
/// `on_game_state_3`, but while the countdown is still active also sweeps
/// per-peer ready flags (`world.net_peer_ready_flags[i]`): if any active
/// peer is not yet marked ready, keep waiting.
unsafe fn on_network_end_await_peers(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let net = (*world).net_session;
        if peer_sync_keep_waiting(runtime, net) {
            return;
        }

        // Per-peer ready sweep — only runs while countdown hasn't expired.
        // (Once `net_end_countdown == 0` the transition is forced regardless.)
        if (*runtime).net_end_countdown != 0 {
            let gi_base = (*world).game_info as *const u8;
            let peer_count = *gi_base as u32; // game_info[0] byte = peer count
            for i in 0..peer_count {
                let active = ((*(*net).vtable).peer_active)(net, i) != 0;
                if active && (*world).net_peer_ready_flags[i as usize] == 0 {
                    return;
                }
            }
        }

        enter_round_ending(runtime);
    }
}

/// Rust port of `GameRuntime__OnRoundEndingCountdown` (0x005365A0).
///
/// Drives the ~1-second delay between `ROUND_ENDING` and `EXIT`:
///  1. Query the turn-game HUD data snapshot (msg 0x7D3) into a local scratch
///     buffer (consumed for side effects — the game uses it to push end-of-
///     round stats through `WorldRootEntity::hud_data_query`).
///  2. If `game_end_clear` is still counting down, decrement and clamp at 0.
///  3. Otherwise advance `game_end_speed` by `Fixed(0x51E)` until its integer
///     part reaches 1.0; once it does, transition to `EXIT`.
unsafe fn on_round_ending_countdown(runtime: *mut GameRuntime) {
    unsafe {
        let mut buf: [core::mem::MaybeUninit<u8>; 0x394] =
            [core::mem::MaybeUninit::uninit(); 0x394];
        let task = (*runtime).world_root;
        ((*(*task).base.vtable).hud_data_query)(task, 0x7d3, 0x394, buf.as_mut_ptr() as *mut u8);

        if (*runtime).game_end_clear != 0 {
            let next = (*runtime).game_end_clear.wrapping_sub(1);
            (*runtime).game_end_clear = if (next as i32) < 1 { 0 } else { next };
            return;
        }

        let speed = (*runtime).game_end_speed;
        if (speed & 0xffff0000) < 0x10000 {
            (*runtime).game_end_speed = speed.wrapping_add(0x51e);
        } else {
            (*runtime).game_state = game_state::EXIT;
        }
    }
}

/// `GameRuntime__ClearWormBuffers` (0x0055C300). Stdcall(task, flag), RET 0x8.
unsafe extern "stdcall" fn bridge_clear_worm_buffers(task: *mut u8, flag: i32) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut u8, i32) =
            core::mem::transmute(CLEAR_WORM_BUFFERS_ADDR as usize);
        func(task, flag);
    }
}

/// `GameRuntime__AdvanceWormFrame` (0x0055C590). Stdcall(task), RET 0x4.
unsafe extern "stdcall" fn bridge_advance_worm_frame(task: *mut u8) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut u8) =
            core::mem::transmute(ADVANCE_WORM_FRAME_ADDR as usize);
        func(task);
    }
}

// ─── End-of-round log bridges ──────────────────────────────────────────────

/// `GameRuntime__DispatchInputMsg` (0x00530F80). Usercall(EAX=local_buf) +
/// stdcall(wrapper, msg_type, payload_size), RET 0xC.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_dispatch_input_msg(
    _buf: *const u8,
    _runtime: *mut GameRuntime,
    _msg_type: u32,
    _size: u32,
) {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym DISPATCH_INPUT_MSG_ADDR,
        options(att_syntax),
    );
}

#[inline(always)]
unsafe fn headless_stream(gi: *const GameInfo) -> *mut c_void {
    unsafe { (*gi).headless_log_stream }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

#[inline(always)]
unsafe fn phase_label_resource(phase: u32) -> StringRes {
    unsafe {
        let table = rb(va::G_PHASE_LABEL_RES_TABLE) as *const u32;
        let id = *table.add(phase as usize);
        StringRes::from_offset(id).expect("phase label offset out of range")
    }
}

// ─── step_frame ────────────────────────────────────────────────────────────

/// Rust port of `GameRuntime__StepFrame` (0x529F30).
///
/// Returns true if more frames should be processed (bool packed into the
/// low byte of the thiscall return value).
///
/// `input_poll_count` is a caller-owned counter incremented whenever
/// input is polled (passed in EAX in the original usercall).
pub unsafe fn step_frame(
    runtime: *mut GameRuntime,
    input_poll_count: &mut u32,
    remaining: &mut u64,
    frame_duration: u64,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    unsafe {
        let world: *mut GameWorld = (*runtime).world;
        let game_info_ptr = (*world).game_info;
        let game_info = &*game_info_ptr;

        // ── Block A: top state transition ──────────────────────────────
        let hud_code = (*world).hud_status_code;
        if (hud_code == 6 || hud_code == 8) && (*runtime).game_end_phase != hud_code as u32 {
            (*runtime).game_end_phase = hud_code as u32;
            if (*world).net_session.is_null() {
                (*runtime).game_state = game_state::ROUND_ENDING;
                (*runtime).game_end_clear = 0;
                (*runtime).game_end_speed = 0;
                if game_info.game_version >= 0x4d {
                    let task = (*runtime).world_root;
                    WorldRootEntity::handle_typed_message_raw(task, task, TurnEndMaybeMessage);
                }
            } else {
                bridge_begin_network_game_end(runtime);
            }
        }

        // ── Block B: PollInput + session accumulator ──────────────────
        // Skip set is {1, 2, 6, 7, 9} (disasm 0x529FAA-CA).
        let phase_for_skip = (*runtime).game_end_phase;
        let skip_input = matches!(phase_for_skip, 1 | 2 | 6 | 7 | 9);
        if !skip_input {
            let hook_mode = InputHookMode::get();
            let arena = &(*world).team_arena;
            if hook_mode == InputHookMode::Off || arena.active_worm_count <= arena.active_team_count
            {
                bridge_poll_input(runtime);
                *input_poll_count = input_poll_count.wrapping_add(1);
            }

            let session = get_game_session();
            if (*session).replay_active_flag != 0 {
                (*world).render_interp_a -= Fixed::ONE;
                (*world).render_interp_b = (*world).render_interp_a;
                (*world).replay_frame_accum =
                    (*world).replay_frame_accum.wrapping_add(Fixed64::ONE);
            }
        }

        // ── Block D: end-game state dispatch (keyed on game_state) ────
        match (*runtime).game_state {
            game_state::NETWORK_END_AWAITING_PEERS => on_network_end_await_peers(runtime),
            game_state::NETWORK_END_STARTED => on_game_state_3(runtime),
            game_state::ROUND_ENDING => on_round_ending_countdown(runtime),
            _ => {}
        }

        // ── Block E: f34c sentinel #1 (conditional 0x7a broadcast) ────
        let frame_counter = (*world).frame_counter;
        let gi_mut = (*world).game_info;

        if frame_counter != (*gi_mut)._field_f34c {
            (*gi_mut)._field_f34c = -1;
        }
        let sentinel_match =
            frame_counter == (*gi_mut)._field_f34c || frame_counter == (*gi_mut).sound_start_frame;
        if sentinel_match {
            (*runtime)._field_404 = 1;
            if (*world).fast_forward_active == 0 {
                let task = (*runtime).world_root;
                WorldRootEntity::handle_typed_message_raw(task, task, Unknown122Message);
            }
        }

        // ── Block F: f34c sentinel #2 (`remaining` adjust) ────────────
        if frame_counter != (*gi_mut)._field_f34c {
            (*gi_mut)._field_f34c = -1;
        }
        let sentinel_match_2 =
            frame_counter == (*gi_mut)._field_f34c || frame_counter == (*gi_mut).sound_start_frame;
        if sentinel_match_2 && (*runtime).frame_delay_counter >= 0 {
            *remaining = 0;
        } else if game_info.sound_mute == 0 && game_info.sound_start_frame <= frame_counter {
            *remaining = remaining.wrapping_sub(frame_duration);
        }

        // ── Block G: headful-mode keyboard/palette no-op slots ────────
        if (*world).is_headful != 0 {
            let keyboard = (*world).keyboard;
            DDKeyboard::slot_06_noop_raw(keyboard);
            let palette = (*world).palette;
            Palette::reset_raw(palette);
        }

        // ── Block H: end-of-round body ────────────────────────────────
        // Fires on `game_state == 4 || game_end_phase != 0` (disasm 0x52A15D).
        let fire_h =
            (*runtime).game_state == game_state::ROUND_ENDING || (*runtime).game_end_phase != 0;
        if fire_h {
            let task = (*runtime).world_root;
            bridge_clear_worm_buffers(task as *mut u8, -1);
            bridge_advance_worm_frame(task as *mut u8);

            if headless_stream(game_info_ptr).is_null() {
                return step_frame_return(runtime, world, game_speed_target, game_speed);
            }

            log_end_of_round(runtime, world, game_info_ptr);

            // Every log-taking exit returns AL=0 in the original (disasm
            // 0x52A76E / 0x52A7E3 `XOR AL, AL`). Falling through to
            // `step_frame_return` here would return true via IsReplayMode
            // or speed match and keep dispatch_frame's loop alive, which
            // suppresses `ProcessFrame::exit_flag` and causes the log to
            // re-emit every end-of-round tick.
            return false;
        }

        // ── Return: IsReplayMode || speeds unchanged ──────────────────
        step_frame_return(runtime, world, game_speed_target, game_speed)
    }
}

#[inline]
unsafe fn step_frame_return(
    runtime: *mut GameRuntime,
    world: *mut GameWorld,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    unsafe {
        if is_replay_mode(runtime) {
            return true;
        }
        let cur_target = (*world).game_speed_target.to_raw();
        let cur_speed = (*world).game_speed.to_raw();
        game_speed_target == cur_target && game_speed == cur_speed
    }
}

// ─── End-of-round headless log ─────────────────────────────────────────────

/// Emit the log-line timestamp prefix:
/// `[recorded_t] [sim_t] ` outside replay playback (both counters
/// available → drift between recording and simulation is visible), or
/// just `[sim_t] ` during replay playback where `recorded_frame_counter`
/// holds the `-1` sentinel.
///
/// Rust port of `GameRuntime__WriteLogTimestampPrefix` (0x0053F100).
unsafe fn write_timestamp_prefix(out: &mut LogOutput, world: *mut GameWorld) {
    unsafe {
        let recorded = (*world).recorded_frame_counter;
        if recorded >= 0 {
            out.write_byte(b'[');
            out.write_timestamp_frames(recorded as u32);
            out.write_bytes(b"] ");
        }
        out.write_byte(b'[');
        out.write_timestamp_frames((*world).frame_counter as u32);
        out.write_bytes(b"] ");
    }
}

/// Emit the team name + optional ` (bank_name)` suffix.
/// Rust port of `GameRuntime__WriteLogTeamLabel` (0x0053F190).
///
/// Team record layout at `game_info + 0x450 + team_idx * 3000`:
///   +0  (i8)    speech_bank_id (-1 = no bank)
///   +6  (cstr)  team name
/// Bank name lives at `game_info + bank_id * 0x50 + 4`.
unsafe fn write_team_label(
    out: &mut LogOutput,
    game_info_ptr: *const GameInfo,
    team_idx_plus_1: u32,
) {
    unsafe {
        let gi_base = game_info_ptr as *const u8;
        let record = gi_base.add(0x450 + (team_idx_plus_1 as usize - 1) * 3000);
        out.write_cstr(record.add(6) as *const c_char);

        // Suffix is gated on `game_info[0] != 0` — same test used for the
        // label-width bank lookup (see the width calc below).
        if *gi_base != 0 {
            let bank_id = *(record as *const i8);
            if bank_id >= 0 {
                let bank_name = gi_base.add((bank_id as usize) * 0x50 + 4) as *const c_char;
                out.write_bytes(b" (");
                out.write_cstr(bank_name);
                out.write_byte(b')');
            }
        }
    }
}

/// End-of-round stats block. Corresponds to the inline log at 0x52A19D-0x52A7EF
/// inside the original `GameRuntime__StepFrame`. Emits: timestamp banner,
/// optional HUD-status suffix, input-queue drain (replay mode only), per-team
/// turn/retreat/total stats, and the optional turn-count footer.
unsafe fn log_end_of_round(
    runtime: *mut GameRuntime,
    world: *mut GameWorld,
    game_info_ptr: *mut GameInfo,
) {
    unsafe {
        let stream = headless_stream(game_info_ptr);
        let game_info = &*game_info_ptr;
        let mut out = LogOutput::new(stream);

        // ── Banner: `[ts] [ts] ••• <Game> - <phase label>` ───────────
        write_timestamp_prefix(&mut out, world);
        // Original emits the banner bullets via direct `fprintf` — never
        // recoded, so bytes 0x95 land on disk literally. Use the raw path.
        out.write_raw_bytes(b"\x95\x95\x95 ");
        out.write_cstr(wa_load_string(res::LOG_GAMEENDS));
        out.write_bytes(b" - ");
        out.write_cstr(wa_load_string(phase_label_resource(
            (*runtime).game_end_phase,
        )));

        // ── HUD suffix ` (<hud_text>)` ───────────────────────────────
        let hud_text = (*world).hud_status_text;
        if !hud_text.is_null() {
            out.write_bytes(b" (");
            out.write_cstr(hud_text);
            out.write_byte(b')');
        }

        out.write_byte(b'\n');

        // ── Input-queue drain (replay mode only) ─────────────────────
        if (*runtime).replay_flag_a != 0 && game_info.replay_config_flag == 0 {
            let render_buf = (*runtime).render_buffer_a;
            GameRuntime::send_game_state_raw(runtime, render_buf, 0, 0);
            input_queue_drain(runtime);
        }

        // ── Per-team stats block ─────────────────────────────────────
        // Re-check headless_log_stream (matches the original 0x52A413 JZ).
        // Note: re-check uses the original stream, not a re-fetch — so we
        // skip the rest iff the field was nulled by the drain above.
        if headless_stream(game_info_ptr).is_null() {
            return;
        }
        let num_teams = game_info.num_teams;
        let speech_count = game_info.speech_team_count;

        // Per-team label widths feed the column-alignment padding emitted
        // after each team label. Width formula is expanded literally from
        // the original snprintf-less bookkeeping (name_end - (record+7),
        // bank_end + name_len_minus_1 + (3 - (bank+1))).
        const MAX_TEAMS: usize = 32;
        let mut label_widths: [i32; MAX_TEAMS] = [0; MAX_TEAMS];
        let mut max_width: i32 = 0;

        if speech_count > 0 {
            let gi_base = game_info_ptr as *const u8;
            let team_records_base = gi_base.add(0x450);
            for (i, item) in label_widths
                .iter_mut()
                .enumerate()
                .take(speech_count as usize)
            {
                let record = team_records_base.add(i * 3000);
                let name_start = record.add(6);
                let mut p = name_start;
                while *p != 0 {
                    p = p.add(1);
                }
                let mut width = (p as isize - record.add(7) as isize) as i32;
                let speech_bank_id = *(record as *const i8);
                if num_teams != 0 && speech_bank_id >= 0 {
                    let bank = gi_base.add((speech_bank_id as usize) * 0x50 + 4);
                    let mut bp = bank;
                    while *bp != 0 {
                        bp = bp.add(1);
                    }
                    width = bp as i32 + width + (3 - (bank.add(1) as i32));
                }
                *item = width;
                if max_width < width {
                    max_width = width;
                }
            }
        }

        // Header: `\n<stats_header>\n` (LOG_TEAM_TIMES, "Team time totals:").
        out.write_byte(b'\n');
        out.write_cstr(wa_load_string(res::LOG_TEAM_TIMES));
        out.write_byte(b'\n');

        if speech_count > 0 {
            // Labels shared across rows (resolved once).
            let lbl_turn = wa_load_string(res::LOG_TEAM_TIME_TURN);
            let lbl_retreat = wa_load_string(res::LOG_TEAM_TIME_RETREAT);
            let lbl_total = wa_load_string(res::LOG_TEAM_TIME_TOTAL);
            let lbl_count = wa_load_string(res::LOG_TEAM_TURN_COUNT);

            for i in 0..speech_count as u32 {
                let team_idx_plus_1 = i + 1;

                // `<team_name>[ (bank)]:<pad> ` — pad + trailing space
                // matches the original `:%*s ` format.
                write_team_label(&mut out, game_info_ptr, team_idx_plus_1);
                out.write_byte(b':');
                out.write_spaces(max_width - label_widths[i as usize]);
                out.write_byte(b' ');

                // Per-team stat fields (disasm 0x52A59D-0x52A614, using
                // EBP = 0x7EC0 + i*4 as the indexing base):
                //   time_total = [world + 0x7EC0 + i*4]
                //   time_used  = [world + 0x7EC0 + i*4 - 0x18]
                //   turn_count = [world + 0x7EC0 + i*4 + 0x18]
                let ebp = 0x7ec0u32 + i * 4;
                let dd_base = world as *const u8;
                let time_total = *(dd_base.add(ebp as usize) as *const i32);
                let time_used = *(dd_base.add((ebp - 0x18) as usize) as *const i32);
                let turn_count_u = *(dd_base.add((ebp + 0x18) as usize) as *const u32);

                // Row format: `<turn> <ta>, <retreat> <tb>, <total> <tc>, <count> <n>\n`
                // — localized labels interleaved with timestamps and the
                // turn count. Slot order matches the original disasm.
                out.write_cstr(lbl_turn);
                out.write_byte(b' ');
                out.write_timestamp_frames(time_used as u32);
                out.write_bytes(b", ");
                out.write_cstr(lbl_retreat);
                out.write_byte(b' ');
                out.write_timestamp_frames(time_total as u32);
                out.write_bytes(b", ");
                out.write_cstr(lbl_total);
                out.write_byte(b' ');
                out.write_timestamp_frames(time_total.wrapping_add(time_used) as u32);
                out.write_bytes(b", ");
                out.write_cstr(lbl_count);
                out.write_byte(b' ');
                out.write_u32(turn_count_u);
                out.write_byte(b'\n');
            }
        }

        out.write_byte(b'\n');

        // ── End-of-round jetpack fuel total (English: "Total Jet Pack fuel used: N"). ─
        let jetpack_fuel = (*world).round_jetpack_fuel_total;
        if jetpack_fuel != 0 {
            out.write_cstr(wa_load_string(res::LOG_JETPACK_FUEL_TOTAL));
            out.write_byte(b' ');
            // Original uses `%d` but the value is a u32 counter (never
            // negative in practice); emit as unsigned.
            out.write_u32(jetpack_fuel);
            out.write_bytes(b"\n\n");
        }
    }
}

/// Drain the replay input queue at `wrapper.render_buffer_a + 0x14`.
///
/// Queue node layout (disasm 0x52A362-0x52A3B0):
///   +0x00  size (u32; payload is `size - 4` bytes)
///   +0x04  next (ptr)
///   +0x08  msg_type (u32)
///   +0x0C  payload[size - 4]
///
/// Nodes with `size > 0x40C` are treated as malformed and drop the loop.
unsafe fn input_queue_drain(runtime: *mut GameRuntime) {
    unsafe {
        let mut local_buf: [u8; 0x408] = [0; 0x408];
        let render_buf = (*runtime).render_buffer_a;
        loop {
            let list_head_slot = render_buf.add(0x14) as *mut *mut u8;
            let node = *list_head_slot;
            if node.is_null() {
                break;
            }
            let size = *(node as *const u32);
            if size > 0x40c {
                break;
            }
            let payload_size = size.wrapping_sub(4);
            if payload_size != 0 {
                core::ptr::copy_nonoverlapping(
                    node.add(0x0c),
                    local_buf.as_mut_ptr(),
                    payload_size as usize,
                );
            }
            let msg_type = *(node.add(0x8) as *const u32);

            BufferObject::classify_input_msg_raw(render_buf as *mut BufferObject);

            bridge_dispatch_input_msg(local_buf.as_ptr(), runtime, msg_type, payload_size);
            if msg_type == 2 {
                let world = (*runtime).world;
                (*world).frame_counter = (*world).frame_counter.wrapping_add(1);
            }
        }
    }
}
