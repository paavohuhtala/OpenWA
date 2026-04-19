//! Rust port of `DDGameWrapper__StepFrame` (0x529F30).
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
//!   - `game_state == 2` → `DDGameWrapper__OnGameState2` (usercall EDI=ESI=wrapper)
//!   - `game_state == 3` → `DDGameWrapper__OnGameState3` (usercall EDI=ESI=wrapper)
//!   - `game_state == 4` → `DDGameWrapper__OnGameState4` (usercall ESI=wrapper)
//! - **E/F**: two `_field_f34c` sentinel blocks — broadcast msg 0x7A and
//!   reset/adjust `remaining`.
//! - **G**: headful-only keyboard/palette vtable slot calls.
//! - **H**: end-of-round body. Fires when `game_state == 4 || phase != 0`.
//!   Runs `ClearWormBuffers(-1)`, `AdvanceWormFrame`, and — if the headless
//!   log stream is non-null — writes the inline end-of-round stats block.
//! - **Return**: `IsReplayMode() || (speed_target,speed) unchanged` on the
//!   non-H path; `false` unconditionally when the log block was written
//!   (disasm 0x52A76E / 0x52A7E3 `XOR AL, AL`). The forced-false after the
//!   log is what lets headless `/getlog` runs terminate after one log
//!   emission — `ProcessFrame` sets `exit_flag` when `advance_frame()`
//!   returns `game_state == 4`.

use core::ffi::{c_char, c_void};

use crate::address::va;
use crate::engine::ddgame::DDGame;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::dispatch_frame::is_replay_mode;
use crate::engine::game_info::GameInfo;
use crate::engine::game_session::get_game_session;
use crate::engine::log_sink::LogOutput;
use crate::rebase::rb;
use crate::task::{CTask, CTaskTurnGame};

// ─── Runtime addresses (resolved at DLL load) ──────────────────────────────

static mut POLL_INPUT_ADDR: u32 = 0;
static mut INPUT_HOOK_MODE_ADDR: u32 = 0;
static mut BEGIN_NETWORK_GAME_END_ADDR: u32 = 0;
static mut ON_GAME_STATE_2_ADDR: u32 = 0;
static mut ON_GAME_STATE_3_ADDR: u32 = 0;
static mut ON_GAME_STATE_4_ADDR: u32 = 0;
static mut CLEAR_WORM_BUFFERS_ADDR: u32 = 0;
static mut ADVANCE_WORM_FRAME_ADDR: u32 = 0;
static mut CLASSIFY_INPUT_MSG_ADDR: u32 = 0;
static mut DISPATCH_INPUT_MSG_ADDR: u32 = 0;
static mut WA_LOAD_STRING_ADDR: u32 = 0;

/// Initialize bridge addresses. Called once at DLL load.
pub unsafe fn init_step_frame_addrs() {
    unsafe {
        POLL_INPUT_ADDR = rb(va::DDGAMEWRAPPER_POLL_INPUT);
        INPUT_HOOK_MODE_ADDR = rb(va::G_INPUT_HOOK_MODE);
        BEGIN_NETWORK_GAME_END_ADDR = rb(va::DDGAMEWRAPPER_BEGIN_NETWORK_GAME_END);
        ON_GAME_STATE_2_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_STATE_2);
        ON_GAME_STATE_3_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_STATE_3);
        ON_GAME_STATE_4_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_STATE_4);
        CLEAR_WORM_BUFFERS_ADDR = rb(va::DDGAMEWRAPPER_CLEAR_WORM_BUFFERS);
        ADVANCE_WORM_FRAME_ADDR = rb(va::DDGAMEWRAPPER_ADVANCE_WORM_FRAME);
        CLASSIFY_INPUT_MSG_ADDR = rb(va::BUFFER_OBJECT_CLASSIFY_INPUT_MSG);
        DISPATCH_INPUT_MSG_ADDR = rb(va::DDGAMEWRAPPER_DISPATCH_INPUT_MSG);
        WA_LOAD_STRING_ADDR = rb(va::WA_LOAD_STRING);
    }
}

// ─── Phase / end-game state bridges ────────────────────────────────────────

/// DDGameWrapper__PollInput — stdcall(wrapper), RET 0x4.
unsafe extern "stdcall" fn bridge_poll_input(wrapper: *mut DDGameWrapper) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
            core::mem::transmute(POLL_INPUT_ADDR as usize);
        func(wrapper);
    }
}

/// `DDGameWrapper__BeginNetworkGameEnd` (0x00536270) — network-mode entry
/// from Block A when `network_ecx != 0`. Usercall(EAX=wrapper), plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_begin_network_game_end(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym BEGIN_NETWORK_GAME_END_ADDR,
        options(att_syntax),
    );
}

/// `DDGameWrapper__OnGameState2` (0x00536470). Usercall(EDI=ESI=wrapper),
/// plain RET. Save/restore ESI+EDI — they're callee-saved in the MS x86 ABI.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_on_game_state_2(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "pushl %edi",
        "movl 12(%esp), %edi",
        "movl %edi, %esi",
        "call *({fn})",
        "popl %edi",
        "popl %esi",
        "retl $4",
        fn = sym ON_GAME_STATE_2_ADDR,
        options(att_syntax),
    );
}

/// `DDGameWrapper__OnGameState3` (0x00536320). Usercall(EDI=ESI=wrapper), plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_on_game_state_3(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "pushl %edi",
        "movl 12(%esp), %edi",
        "movl %edi, %esi",
        "call *({fn})",
        "popl %edi",
        "popl %esi",
        "retl $4",
        fn = sym ON_GAME_STATE_3_ADDR,
        options(att_syntax),
    );
}

/// `DDGameWrapper__OnGameState4` (0x005365A0). Usercall(ESI=wrapper), plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_on_game_state_4(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",
        "call *({fn})",
        "popl %esi",
        "retl $4",
        fn = sym ON_GAME_STATE_4_ADDR,
        options(att_syntax),
    );
}

/// `DDGameWrapper__ClearWormBuffers` (0x0055C300). Stdcall(task, flag), RET 0x8.
unsafe extern "stdcall" fn bridge_clear_worm_buffers(task: *mut u8, flag: i32) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut u8, i32) =
            core::mem::transmute(CLEAR_WORM_BUFFERS_ADDR as usize);
        func(task, flag);
    }
}

/// `DDGameWrapper__AdvanceWormFrame` (0x0055C590). Stdcall(task), RET 0x4.
unsafe extern "stdcall" fn bridge_advance_worm_frame(task: *mut u8) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut u8) =
            core::mem::transmute(ADVANCE_WORM_FRAME_ADDR as usize);
        func(task);
    }
}

// ─── End-of-round log bridges ──────────────────────────────────────────────

/// `BufferObject__ClassifyInputMsg` (0x00541100). Thiscall(ECX=render_buffer),
/// returns packed u64 (EDX:EAX): EAX=keep-going flag, EDX=msg subtype.
unsafe extern "thiscall" fn bridge_classify_input_msg(_render_buffer: *mut u8) -> u64 {
    unsafe {
        let func: unsafe extern "thiscall" fn(*mut u8) -> u64 =
            core::mem::transmute(CLASSIFY_INPUT_MSG_ADDR as usize);
        func(_render_buffer)
    }
}

/// `DDGameWrapper__DispatchInputMsg` (0x00530F80). Usercall(EAX=local_buf) +
/// stdcall(wrapper, msg_type, payload_size), RET 0xC.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_dispatch_input_msg(
    _buf: *const u8,
    _wrapper: *mut DDGameWrapper,
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

/// `WA__LoadStringResource` (0x00593180). Stdcall(resource_id) → pointer.
unsafe fn wa_load_string(id: u32) -> *const c_char {
    unsafe {
        let func: unsafe extern "stdcall" fn(u32) -> *const c_char =
            core::mem::transmute(WA_LOAD_STRING_ADDR as usize);
        func(id)
    }
}

#[inline(always)]
unsafe fn headless_stream(gi: *const GameInfo) -> *mut c_void {
    unsafe { (*gi).headless_log_stream }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

#[inline(always)]
fn combine(lo: u32, hi: u32) -> u64 {
    (hi as u64) << 32 | lo as u64
}

#[inline(always)]
unsafe fn phase_label_resource(phase: u32) -> u32 {
    unsafe {
        let table = rb(va::G_PHASE_LABEL_RES_TABLE) as *const u32;
        *table.add(phase as usize)
    }
}

// ─── step_frame ────────────────────────────────────────────────────────────

/// Rust port of `DDGameWrapper__StepFrame` (0x529F30).
///
/// Returns true if more frames should be processed (bool packed into the
/// low byte of the thiscall return value).
///
/// `input_poll_count` is a caller-owned counter incremented whenever
/// input is polled (passed in EAX in the original usercall).
pub unsafe fn step_frame(
    wrapper: *mut DDGameWrapper,
    input_poll_count: &mut u32,
    remaining: *mut u64,
    frame_duration_lo: u32,
    frame_duration_hi: u32,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    unsafe {
        let ddgame: *mut DDGame = (*wrapper).ddgame;
        let game_info_ptr = (*ddgame).game_info;
        let game_info = &*game_info_ptr;

        // ── Block A: top state transition ──────────────────────────────
        let hud_code = (*ddgame).hud_status_code;
        if (hud_code == 6 || hud_code == 8) && (*wrapper).game_end_phase != hud_code as u32 {
            (*wrapper).game_end_phase = hud_code as u32;
            if (*ddgame).network_ecx == 0 {
                (*wrapper).game_state = 4; // EXIT_HEADLESS
                (*wrapper).game_end_clear = 0;
                (*wrapper).game_end_speed = 0;
                if game_info.game_version >= 0x4d {
                    let task = (*wrapper).task_turn_game;
                    CTaskTurnGame::handle_message_raw(
                        task,
                        task as *mut CTask,
                        0x75,
                        0,
                        core::ptr::null(),
                    );
                }
            } else {
                bridge_begin_network_game_end(wrapper);
            }
        }

        // ── Block B: PollInput + session accumulator ──────────────────
        // Skip set is {1, 2, 6, 7, 9} (disasm 0x529FAA-CA).
        let phase_for_skip = (*wrapper).game_end_phase;
        let skip_input = matches!(phase_for_skip, 1 | 2 | 6 | 7 | 9);
        if !skip_input {
            let hook_mode = *(INPUT_HOOK_MODE_ADDR as *const u32);
            let arena = &(*ddgame).team_arena;
            if hook_mode == 0 || arena.active_worm_count <= arena.active_team_count {
                bridge_poll_input(wrapper);
                *input_poll_count = input_poll_count.wrapping_add(1);
            }

            let session = get_game_session();
            if (*session).replay_active_flag != 0 {
                (*ddgame).render_interp_a = (*ddgame).render_interp_a.wrapping_sub(0x10000);
                (*ddgame).render_interp_b = (*ddgame).render_interp_a;
                let accum =
                    combine((*ddgame)._field_8160, (*ddgame)._field_8164).wrapping_add(0x10000);
                (*ddgame)._field_8160 = accum as u32;
                (*ddgame)._field_8164 = (accum >> 32) as u32;
            }
        }

        // ── Block D: end-game state dispatch (keyed on game_state) ────
        match (*wrapper).game_state {
            2 => bridge_on_game_state_2(wrapper),
            3 => bridge_on_game_state_3(wrapper),
            4 => bridge_on_game_state_4(wrapper),
            _ => {}
        }

        // ── Block E: f34c sentinel #1 (conditional 0x7a broadcast) ────
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
                let task = (*wrapper).task_turn_game;
                CTaskTurnGame::handle_message_raw(
                    task,
                    task as *mut CTask,
                    0x7a,
                    0,
                    core::ptr::null(),
                );
            }
        }

        // ── Block F: f34c sentinel #2 (`remaining` adjust) ────────────
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

        // ── Block G: headful-mode keyboard/palette no-op slots ────────
        if (*ddgame).is_headful != 0 {
            let keyboard = (*ddgame).keyboard;
            crate::input::keyboard::DDKeyboard::slot_06_noop_raw(keyboard);
            let palette = (*ddgame).palette;
            crate::render::display::palette::Palette::reset_raw(palette);
        }

        // ── Block H: end-of-round body ────────────────────────────────
        // Fires on `game_state == 4 || game_end_phase != 0` (disasm 0x52A15D).
        let fire_h = (*wrapper).game_state == 4 || (*wrapper).game_end_phase != 0;
        if fire_h {
            let task = (*wrapper).task_turn_game;
            bridge_clear_worm_buffers(task as *mut u8, -1);
            bridge_advance_worm_frame(task as *mut u8);

            if headless_stream(game_info_ptr).is_null() {
                return step_frame_return(wrapper, ddgame, game_speed_target, game_speed);
            }

            log_end_of_round(wrapper, ddgame, game_info_ptr);

            // Every log-taking exit returns AL=0 in the original (disasm
            // 0x52A76E / 0x52A7E3 `XOR AL, AL`). Falling through to
            // `step_frame_return` here would return true via IsReplayMode
            // or speed match and keep dispatch_frame's loop alive, which
            // suppresses `ProcessFrame::exit_flag` and causes the log to
            // re-emit every end-of-round tick.
            return false;
        }

        // ── Return: IsReplayMode || speeds unchanged ──────────────────
        step_frame_return(wrapper, ddgame, game_speed_target, game_speed)
    }
}

#[inline]
unsafe fn step_frame_return(
    wrapper: *mut DDGameWrapper,
    ddgame: *mut DDGame,
    game_speed_target: i32,
    game_speed: i32,
) -> bool {
    unsafe {
        if is_replay_mode(wrapper) {
            return true;
        }
        let cur_target = (*ddgame).game_speed_target.to_raw();
        let cur_speed = (*ddgame).game_speed.to_raw();
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
/// Rust port of `DDGameWrapper__WriteLogTimestampPrefix` (0x0053F100).
unsafe fn write_timestamp_prefix(out: &mut LogOutput, ddgame: *mut DDGame) {
    unsafe {
        let recorded = (*ddgame).recorded_frame_counter;
        if recorded >= 0 {
            out.write_byte(b'[');
            out.write_timestamp_frames(recorded as u32);
            out.write_bytes(b"] ");
        }
        out.write_byte(b'[');
        out.write_timestamp_frames((*ddgame).frame_counter as u32);
        out.write_bytes(b"] ");
    }
}

/// Emit the team name + optional ` (bank_name)` suffix.
/// Rust port of `DDGameWrapper__WriteLogTeamLabel` (0x0053F190).
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
/// inside the original `DDGameWrapper__StepFrame`. Emits: timestamp banner,
/// optional HUD-status suffix, input-queue drain (replay mode only), per-team
/// turn/retreat/total stats, and the optional turn-count footer.
unsafe fn log_end_of_round(
    wrapper: *mut DDGameWrapper,
    ddgame: *mut DDGame,
    game_info_ptr: *mut GameInfo,
) {
    unsafe {
        let stream = headless_stream(game_info_ptr);
        let game_info = &*game_info_ptr;
        let mut out = LogOutput::new(stream);

        // ── Banner: `[ts] [ts] ••• <Game> - <phase label>` ───────────
        write_timestamp_prefix(&mut out, ddgame);
        // Original emits the banner bullets via direct `fprintf` — never
        // recoded, so bytes 0x95 land on disk literally. Use the raw path.
        out.write_raw_bytes(b"\x95\x95\x95 ");
        out.write_cstr(wa_load_string(0x70e)); // "Game"
        out.write_bytes(b" - ");
        out.write_cstr(wa_load_string(phase_label_resource(
            (*wrapper).game_end_phase,
        )));

        // ── HUD suffix ` (<hud_text>)` ───────────────────────────────
        let hud_text = (*ddgame).hud_status_text;
        if !hud_text.is_null() {
            out.write_bytes(b" (");
            out.write_cstr(hud_text);
            out.write_byte(b')');
        }

        out.write_byte(b'\n');

        // ── Input-queue drain (replay mode only) ─────────────────────
        if (*wrapper).replay_flag_a != 0 && game_info.replay_config_flag == 0 {
            let render_buf = (*wrapper).render_buffer_a;
            DDGameWrapper::send_game_state_raw(wrapper, render_buf, 0, 0);
            input_queue_drain(wrapper);
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
            for i in 0..(speech_count as usize).min(MAX_TEAMS) {
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
                label_widths[i] = width;
                if max_width < width {
                    max_width = width;
                }
            }
        }

        // Header: `\n<stats_header>\n` (resource 0x71D, "Team time totals:").
        out.write_byte(b'\n');
        out.write_cstr(wa_load_string(0x71d));
        out.write_byte(b'\n');

        if speech_count > 0 {
            // Labels shared across rows (resolved once).
            let lbl_71e = wa_load_string(0x71e);
            let lbl_71f = wa_load_string(0x71f);
            let lbl_720 = wa_load_string(0x720);
            let lbl_721 = wa_load_string(0x721);

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
                //   time_total = [ddgame + 0x7EC0 + i*4]
                //   time_used  = [ddgame + 0x7EC0 + i*4 - 0x18]
                //   turn_count = [ddgame + 0x7EC0 + i*4 + 0x18]
                let ebp = 0x7ec0u32 + i * 4;
                let dd_base = ddgame as *const u8;
                let time_total = *(dd_base.add(ebp as usize) as *const i32);
                let time_used = *(dd_base.add((ebp - 0x18) as usize) as *const i32);
                let turn_count_u = *(dd_base.add((ebp + 0x18) as usize) as *const u32);

                // Row format: `<lbl_71e> <ta>, <lbl_71f> <tb>, <lbl_720> <tc>, <lbl_721> <n>\n`
                // — localized labels interleaved with timestamps and the
                // turn count. Slot order matches the original disasm.
                out.write_cstr(lbl_71e);
                out.write_byte(b' ');
                out.write_timestamp_frames(time_used as u32);
                out.write_bytes(b", ");
                out.write_cstr(lbl_71f);
                out.write_byte(b' ');
                out.write_timestamp_frames(time_total as u32);
                out.write_bytes(b", ");
                out.write_cstr(lbl_720);
                out.write_byte(b' ');
                out.write_timestamp_frames(time_total.wrapping_add(time_used) as u32);
                out.write_bytes(b", ");
                out.write_cstr(lbl_721);
                out.write_byte(b' ');
                out.write_u32(turn_count_u);
                out.write_byte(b'\n');
            }
        }

        out.write_byte(b'\n');

        // ── End-of-round turn count (resource 0x722: "Turn count:"). ─
        let turn_count = (*ddgame).round_turn_count;
        if turn_count != 0 {
            out.write_cstr(wa_load_string(0x722));
            out.write_byte(b' ');
            // Original uses `%d` but the value is a u32 counter (never
            // negative in practice); emit as unsigned.
            out.write_u32(turn_count);
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
unsafe fn input_queue_drain(wrapper: *mut DDGameWrapper) {
    unsafe {
        let mut local_buf: [u8; 0x408] = [0; 0x408];
        let render_buf = (*wrapper).render_buffer_a;
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

            let packed = bridge_classify_input_msg(render_buf);
            let keep = packed as u32;
            let edx = (packed >> 32) as u32;
            if keep == 0 {
                break;
            }

            // msg_type 0x16 can override `game_end_phase` when the current
            // phase is 0. edx ∈ {7, 9} aborts the drain entirely.
            if msg_type == 0x16 {
                if edx == 7 || edx == 9 {
                    break;
                }
                let cur_phase = (*wrapper).game_end_phase;
                if cur_phase != edx {
                    if cur_phase != 0 {
                        continue;
                    }
                    (*wrapper).game_end_phase = edx;
                    continue;
                }
            }

            bridge_dispatch_input_msg(local_buf.as_ptr(), wrapper, msg_type, payload_size);
            if msg_type == 2 {
                let ddgame = (*wrapper).ddgame;
                (*ddgame).frame_counter = (*ddgame).frame_counter.wrapping_add(1);
            }
        }
    }
}
