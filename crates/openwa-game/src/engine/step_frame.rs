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
//! - **D**: end-game state dispatch keyed on `wrapper.game_state` (NOT
//!   `game_end_phase`, despite the Ghidra `ON_GAME_END_PHASE{2,3,4}` names):
//!   - `game_state == 2` → `FUN_00536470` (usercall EDI=ESI=wrapper)
//!   - `game_state == 3` → `FUN_00536320` (usercall EDI=ESI=wrapper)
//!   - `game_state == 4` → `FUN_005365A0` (usercall ESI=wrapper)
//! - **E/F**: two `_field_f34c` sentinel blocks — broadcast msg 0x7A and
//!   reset/adjust `remaining`.
//! - **G**: headful-only keyboard/palette vtable slot calls.
//! - **H**: end-of-round body. Fires when `game_state == 4 || phase != 0`.
//!   Runs `ClearWormBuffers(-1)`, `AdvanceFrame`, and — if the headless log
//!   stream is non-null — writes the inline end-of-round stats block.
//! - **Return**: `IsReplayMode() || (speed_target,speed) unchanged` on the
//!   non-H path; `false` unconditionally when the log block was written
//!   (disasm 0x52a76e / 0x52a7e3 `XOR AL, AL`). The forced-false after the
//!   log is what lets headless `/getlog` runs terminate after one log
//!   emission — `ProcessFrame` sets `exit_flag` when `advance_frame()`
//!   returns `game_state == 4`.

use core::ffi::{c_char, c_void};

use crate::address::va;
use crate::engine::ddgame::DDGame;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::dispatch_frame::is_replay_mode;
use crate::engine::game_session::get_game_session;
use crate::rebase::rb;

// ─── Runtime addresses (resolved at DLL load) ──────────────────────────────

static mut POLL_INPUT_ADDR: u32 = 0;
static mut INPUT_HOOK_MODE_ADDR: u32 = 0;
static mut ON_GAME_END_PHASE1_ADDR: u32 = 0;
static mut ON_GAME_STATE_2_ADDR: u32 = 0;
static mut ON_GAME_STATE_3_ADDR: u32 = 0;
static mut ON_GAME_STATE_4_ADDR: u32 = 0;
static mut CLEAR_WORM_BUFFERS_ADDR: u32 = 0;
static mut ADVANCE_WORM_FRAME_ADDR: u32 = 0;
static mut LOAD_LOG_LABEL_A_ADDR: u32 = 0;
static mut LOAD_LOG_LABEL_B_ADDR: u32 = 0;
static mut CLASSIFY_INPUT_MSG_ADDR: u32 = 0;
static mut DISPATCH_INPUT_MSG_ADDR: u32 = 0;
static mut WRITE_HEADLESS_LOG_ADDR: u32 = 0;
static mut WA_LOAD_STRING_ADDR: u32 = 0;
static mut CODEPAGE_LUT_BUILDER_ADDR: u32 = 0;
static mut CRT_IOB_FUNC_ADDR: u32 = 0;
static mut CRT_SPRINTF_ADDR: u32 = 0;

/// Initialize bridge addresses. Called once at DLL load.
pub unsafe fn init_step_frame_addrs() {
    unsafe {
        POLL_INPUT_ADDR = rb(va::DDGAMEWRAPPER_POLL_INPUT);
        INPUT_HOOK_MODE_ADDR = rb(va::G_INPUT_HOOK_MODE);
        ON_GAME_END_PHASE1_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_END_PHASE1);
        ON_GAME_STATE_2_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_END_PHASE2);
        ON_GAME_STATE_3_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_END_PHASE3);
        ON_GAME_STATE_4_ADDR = rb(va::DDGAMEWRAPPER_ON_GAME_END_PHASE4);
        CLEAR_WORM_BUFFERS_ADDR = rb(va::DDGAMEWRAPPER_CLEAR_WORM_BUFFERS);
        ADVANCE_WORM_FRAME_ADDR = rb(va::DDGAMEWRAPPER_ADVANCE_WORM_FRAME);
        LOAD_LOG_LABEL_A_ADDR = rb(va::DDGAMEWRAPPER_LOAD_LOG_LABEL_A);
        LOAD_LOG_LABEL_B_ADDR = rb(va::DDGAMEWRAPPER_LOAD_LOG_LABEL_B);
        CLASSIFY_INPUT_MSG_ADDR = rb(va::DDGAMEWRAPPER_CLASSIFY_INPUT_MSG);
        DISPATCH_INPUT_MSG_ADDR = rb(va::DDGAMEWRAPPER_DISPATCH_INPUT_MSG);
        WRITE_HEADLESS_LOG_ADDR = rb(va::DDGAMEWRAPPER_FORMAT_FRAME_TIMESTAMP);
        WA_LOAD_STRING_ADDR = rb(va::WA_LOAD_STRING);
        CODEPAGE_LUT_BUILDER_ADDR = rb(va::CODEPAGE_LUT_BUILDER);
        CRT_IOB_FUNC_ADDR = rb(va::CRT_IOB_FUNC);
        CRT_SPRINTF_ADDR = rb(va::CRT_SPRINTF);
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

/// FUN_00536270 — end-game phase-1 arm (network-mode scoreboard reset).
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

/// FUN_00536470 — game_state==2 handler.
/// Usercall(EDI=wrapper, ESI=wrapper), no stack args, plain RET.
///
/// Save/restore ESI+EDI because they're callee-saved in the MS x86 ABI;
/// this trampoline clobbers both to set up the usercall register inputs.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_on_game_state_2(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "pushl %edi",
        "movl 12(%esp), %edi",    // wrapper arg: ret@+8, arg@+12
        "movl %edi, %esi",
        "call *({fn})",
        "popl %edi",
        "popl %esi",
        "retl $4",                // stdcall: clean 1 stack arg
        fn = sym ON_GAME_STATE_2_ADDR,
        options(att_syntax),
    );
}

/// FUN_00536320 — game_state==3 handler.
/// Usercall(EDI=wrapper, ESI=wrapper), no stack args, plain RET.
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

/// FUN_005365A0 — game_state==4 handler.
/// Usercall(ESI=wrapper), no stack args, plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_on_game_state_4(_wrapper: *mut DDGameWrapper) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",     // wrapper arg: ret@+4, arg@+8
        "call *({fn})",
        "popl %esi",
        "retl $4",
        fn = sym ON_GAME_STATE_4_ADDR,
        options(att_syntax),
    );
}

/// FUN_0055C300 — ClearWormBuffers(task, flag). Stdcall, RET 0x8.
unsafe extern "stdcall" fn bridge_clear_worm_buffers(task: *mut u8, flag: i32) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut u8, i32) =
            core::mem::transmute(CLEAR_WORM_BUFFERS_ADDR as usize);
        func(task, flag);
    }
}

/// FUN_0055C590 — AdvanceFrame(task). Stdcall, RET 0x4.
unsafe extern "stdcall" fn bridge_advance_worm_frame(task: *mut u8) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut u8) =
            core::mem::transmute(ADVANCE_WORM_FRAME_ADDR as usize);
        func(task);
    }
}

// ─── End-of-round log bridges ──────────────────────────────────────────────

/// FUN_0053F100 — writes `[...] ` timestamp prefix line(s) to the headless
/// log stream. Usercall(EDI=ddgame), no stack args, plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_load_log_label_a(_ddgame: *mut DDGame) {
    core::arch::naked_asm!(
        "pushl %edi",
        "movl 8(%esp), %edi",     // ddgame arg
        "call *({fn})",
        "popl %edi",
        "retl $4",
        fn = sym LOAD_LOG_LABEL_A_ADDR,
        options(att_syntax),
    );
}

/// FUN_0053F190 — writes team name and (optional) "(bank)" suffix for
/// the end-of-round header. Usercall(EAX=team_index_plus_one, EDI=ddgame),
/// no stack args, plain RET.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_load_log_label_b(_ddgame: *mut DDGame, _team_idx_plus_1: u32) {
    core::arch::naked_asm!(
        "pushl %edi",
        "movl 8(%esp), %edi",     // ddgame (arg 0): ret@+4, arg0@+8
        "movl 12(%esp), %eax",    // team_idx_plus_1 (arg 1): at +12
        "call *({fn})",
        "popl %edi",
        "retl $8",                // stdcall: clean 2 stack args
        fn = sym LOAD_LOG_LABEL_B_ADDR,
        options(att_syntax),
    );
}

/// FUN_00541100 — classifies a queued input message.
/// Thiscall(ECX=render_buffer_a), no stack args. Returns u64 in EDX:EAX.
/// The caller reads EAX (nonzero = keep going) and EDX (msg subtype).
unsafe extern "thiscall" fn bridge_classify_input_msg(_render_buffer: *mut u8) -> u64 {
    unsafe {
        let func: unsafe extern "thiscall" fn(*mut u8) -> u64 =
            core::mem::transmute(CLASSIFY_INPUT_MSG_ADDR as usize);
        func(_render_buffer)
    }
}

/// FUN_00530F80 — dispatches an input message.
/// Usercall(EAX=local_buf_ptr) + stdcall(wrapper, msg_type, payload_size),
/// RET 0xC (callee cleans 3 stack args).
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_dispatch_input_msg(
    _buf: *const u8,
    _wrapper: *mut DDGameWrapper,
    _msg_type: u32,
    _size: u32,
) {
    core::arch::naked_asm!(
        "popl %ecx",              // return addr
        "popl %eax",              // buf ptr → EAX
        "pushl %ecx",             // restore ret
        "jmpl *({fn})",
        fn = sym DISPATCH_INPUT_MSG_ADDR,
        options(att_syntax),
    );
}

/// DDGameWrapper__WriteHeadlessLog (0x53F0A0).
/// Usercall(EAX=frame_counter_like, stdcall(fmt_fn, buffer)), RET 0x8.
/// Used to render timestamps into stack buffers for the per-team stat line.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_write_headless_log(
    _frame_ctx: i32,
    _sprintf_fn: *const c_void,
    _buf: *mut u8,
) {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",              // frame_ctx → EAX
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym WRITE_HEADLESS_LOG_ADDR,
        options(att_syntax),
    );
}

/// FUN_00593180 — loads a localized string resource by ID.
/// Stdcall(resource_id), returns pointer in EAX.
unsafe fn wa_load_string(id: u32) -> *const c_char {
    unsafe {
        let func: unsafe extern "stdcall" fn(u32) -> *const c_char =
            core::mem::transmute(WA_LOAD_STRING_ADDR as usize);
        func(id)
    }
}

/// FUN_00592280 — codepage LUT builder.
/// Usercall(EAX=codepage), returns LUT pointer in EAX.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_codepage_lut_builder(_acp: u32) -> *const u8 {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",              // acp → EAX
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym CODEPAGE_LUT_BUILDER_ADDR,
        options(att_syntax),
    );
}

// ─── CRT helpers ───────────────────────────────────────────────────────────

#[inline(always)]
unsafe fn headless_stream(gi: *const crate::engine::game_info::GameInfo) -> *mut c_void {
    unsafe { (*gi).headless_log_stream }
}

#[inline(always)]
unsafe fn codepage_recode_on() -> bool {
    unsafe { *(rb(va::G_CODEPAGE_RECODE_FLAG) as *const u8) != 0 }
}

#[inline(always)]
unsafe fn call_putc(ch: i32, stream: *mut c_void) {
    unsafe {
        let putc_ptr = *(rb(va::CRT_PUTC_IAT) as *const u32) as usize;
        let putc: unsafe extern "C" fn(i32, *mut c_void) -> i32 = core::mem::transmute(putc_ptr);
        putc(ch, stream);
    }
}

#[inline(always)]
unsafe fn call_fputs(s: *const u8, stream: *mut c_void) {
    unsafe {
        let fputs_ptr = *(rb(va::CRT_FPUTS_IAT) as *const u32) as usize;
        let fputs: unsafe extern "C" fn(*const u8, *mut c_void) -> i32 =
            core::mem::transmute(fputs_ptr);
        fputs(s, stream);
    }
}

// Per-arity fprintf/snprintf typedefs. Rust function-pointer types cannot
// express `...`, so we cast to concrete signatures per call site.
type Fprintf1 = unsafe extern "C" fn(*mut c_void, *const u8) -> i32;
type Fprintf2 = unsafe extern "C" fn(*mut c_void, *const u8, *const u8) -> i32;
type Fprintf3 = unsafe extern "C" fn(*mut c_void, *const u8, *const u8, *const u8) -> i32;
type Fprintf4 = unsafe extern "C" fn(*mut c_void, *const u8, i32, *const u8) -> i32;
type Fprintf8 = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    u32,
) -> i32;
type Fprintf2Int = unsafe extern "C" fn(*mut c_void, *const u8, *const u8, u32) -> i32;

type Snprintf2 =
    unsafe extern "C" fn(*mut u8, usize, usize, *const u8, *const u8, *const u8) -> i32;
type Snprintf1 = unsafe extern "C" fn(*mut u8, usize, usize, *const u8, *const u8) -> i32;
type Snprintf4 = unsafe extern "C" fn(*mut u8, usize, usize, *const u8, i32, *const u8) -> i32;
type Snprintf8 = unsafe extern "C" fn(
    *mut u8,
    usize,
    usize,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    *const u8,
    u32,
) -> i32;
type Snprintf2Int = unsafe extern "C" fn(*mut u8, usize, usize, *const u8, *const u8, u32) -> i32;

#[inline(always)]
unsafe fn fprintf_ptr() -> usize {
    unsafe { *(rb(va::CRT_FPRINTF_IAT) as *const u32) as usize }
}

#[inline(always)]
unsafe fn snprintf_s_ptr() -> usize {
    rb(va::CRT_SNPRINTF_S) as usize
}

/// Lazily build the codepage LUT (via GetACP + FUN_00592280), then recode
/// the scratch buffer in-place.
unsafe fn codepage_recode_scratch() {
    unsafe {
        let lut_slot = rb(va::G_CODEPAGE_LUT) as *mut u32;
        if *lut_slot == 0 {
            // Call kernel32.GetACP via its IAT pointer, then the WA LUT
            // builder with the returned codepage in EAX.
            let getacp_ptr = *(rb(va::KERNEL32_GETACP_IAT) as *const u32) as usize;
            let getacp: unsafe extern "system" fn() -> u32 = core::mem::transmute(getacp_ptr);
            let acp = getacp();
            *lut_slot = bridge_codepage_lut_builder(acp) as u32;
        }
        let lut = *lut_slot as *const u8;
        let mut p = rb(va::G_LOG_SCRATCH_BUF) as *mut u8;
        while *p != 0 {
            *p = *lut.add(*p as usize + 0x100);
            p = p.add(1);
        }
    }
}

/// Recode the scratch buffer (if needed) and fputs it to `stream`.
#[inline(always)]
unsafe fn recode_and_fputs(stream: *mut c_void) {
    unsafe {
        codepage_recode_scratch();
        call_fputs(rb(va::G_LOG_SCRATCH_BUF) as *const u8, stream);
    }
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

        // ── Block B: PollInput + session accumulator ──────────────────
        //
        // Skip set is {1, 2, 6, 7, 9}. (The original current-state Rust was
        // missing phase 2; re-added here to match disasm at 0x529FAA-CA.)
        let phase_for_skip = (*wrapper).game_end_phase;
        let skip_input = matches!(phase_for_skip, 1 | 2 | 6 | 7 | 9);
        if !skip_input {
            let hook_mode = *(INPUT_HOOK_MODE_ADDR as *const u32);
            let arena = &(*ddgame).team_arena;
            if hook_mode == 0 || arena.active_worm_count <= arena.active_team_count {
                bridge_poll_input(wrapper);
                *counter_ptr = (*counter_ptr).wrapping_add(1);
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
                crate::task::CTaskTurnGame::handle_message_raw(
                    task,
                    task as *mut crate::task::CTask,
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
        //
        // Fires when `game_state == 4 || game_end_phase != 0` (disasm at
        // 0x52A15D-0x52A172). Runs ClearWormBuffers + AdvanceFrame, then
        // — if the headless log stream is non-null — emits the inline
        // end-of-round stats block.
        let fire_h = (*wrapper).game_state == 4 || (*wrapper).game_end_phase != 0;
        if fire_h {
            let task = (*wrapper).task_turn_game;
            bridge_clear_worm_buffers(task as *mut u8, -1);
            bridge_advance_worm_frame(task as *mut u8);

            if headless_stream(game_info_ptr).is_null() {
                // Log disabled — skip both the banner/drain block (0x52a19d)
                // and the per-team stats block (0x52a408).
                return step_frame_return(wrapper, ddgame, game_speed_target, game_speed);
            }

            log_end_of_round(wrapper, ddgame, game_info_ptr);

            // Every log-taking exit path in the original returns `AL=0`
            // (disasm: 0x52a76e / 0x52a7e3 `XOR AL, AL`). Falling through
            // to `step_frame_return` would return true via IsReplayMode /
            // speed match, keeping dispatch_frame's loop alive. Headless
            // mode needs this false to trigger
            // `ProcessFrame::exit_flag = 1` (see process_frame.rs:156-161)
            // and terminate after the first log write — otherwise the log
            // block is re-emitted every end-of-round frame.
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

/// Disasm offsets for the body below (all inside `DDGameWrapper__StepFrame`):
/// - Banner block: 0x52A19D-0x52A32E
/// - HUD suffix: 0x52A273-0x52A313
/// - putc('\n'): 0x52A313-0x52A32E
/// - Input-queue drain: 0x52A32E-0x52A407 (gated by replay_flag_a && !replay_config_flag)
/// - Per-team label-width pass: 0x52A408-0x52A4BE
/// - Per-team stats header: 0x52A4C0-0x52A553
/// - Per-team stats loop: 0x52A580-0x52A712
/// - putc('\n'): 0x52A712-0x52A72A
/// - End-of-round turn count: 0x52A72A-0x52A7EF
unsafe fn log_end_of_round(
    wrapper: *mut DDGameWrapper,
    ddgame: *mut DDGame,
    game_info_ptr: *mut crate::engine::game_info::GameInfo,
) {
    unsafe {
        let stream = headless_stream(game_info_ptr);
        let game_info = &*game_info_ptr;

        // ── Banner ───────────────────────────────────────────────────
        // CALL FUN_0053f100 — writes "[ts] " or "[ts1] [ts2] " prefix.
        bridge_load_log_label_a(ddgame);

        // fprintf(stream, "••• ") — bullet banner prefix (no variadic args).
        let banner_prefix = rb(va::G_LOG_BANNER_PREFIX) as *const u8;
        let fprintf1: Fprintf1 = core::mem::transmute(fprintf_ptr());
        fprintf1(stream, banner_prefix);

        // Load localized "phase label" (via resource id phase_labels[phase])
        // and "Game" label (resource 0x70E).
        let phase = (*wrapper).game_end_phase;
        let phase_label = wa_load_string(phase_label_resource(phase));
        let game_label = wa_load_string(0x70e);

        // Print "%s - %s" with (game_label, phase_label).
        let fmt_game_phase = rb(va::G_FMT_GAME_PHASE) as *const u8;
        if codepage_recode_on() {
            let snprintf2: Snprintf2 = core::mem::transmute(snprintf_s_ptr());
            snprintf2(
                rb(va::G_LOG_SCRATCH_BUF) as *mut u8,
                0x3fff,
                usize::MAX,
                fmt_game_phase,
                game_label as *const u8,
                phase_label as *const u8,
            );
            recode_and_fputs(stream);
        } else {
            let fprintf3: Fprintf3 = core::mem::transmute(fprintf_ptr());
            fprintf3(
                stream,
                fmt_game_phase,
                game_label as *const u8,
                phase_label as *const u8,
            );
        }

        // ── HUD suffix ───────────────────────────────────────────────
        let hud_text = (*ddgame).hud_status_text;
        if !hud_text.is_null() {
            let fmt_suffix = rb(va::G_FMT_HUD_SUFFIX) as *const u8;
            if codepage_recode_on() {
                let snprintf1: Snprintf1 = core::mem::transmute(snprintf_s_ptr());
                snprintf1(
                    rb(va::G_LOG_SCRATCH_BUF) as *mut u8,
                    0x3fff,
                    usize::MAX,
                    fmt_suffix,
                    hud_text as *const u8,
                );
                recode_and_fputs(stream);
            } else {
                let fprintf2: Fprintf2 = core::mem::transmute(fprintf_ptr());
                fprintf2(stream, fmt_suffix, hud_text as *const u8);
            }
        }

        // putc('\n', stream).
        call_putc(b'\n' as i32, stream);

        // ── Input-queue drain ────────────────────────────────────────
        // Gated on `wrapper.replay_flag_a != 0 && !game_info.replay_config_flag`.
        if (*wrapper).replay_flag_a != 0 && game_info.replay_config_flag == 0 {
            // Call vtable[2] (send_game_state) with (render_buffer_a, 0, 0).
            let render_buf = (*wrapper).render_buffer_a;
            let vtable = (*wrapper).vtable;
            let slot2 = (*vtable).send_game_state;
            slot2(wrapper, render_buf, 0, 0);

            input_queue_drain(wrapper);
        }

        // ── Per-team stats block ─────────────────────────────────────
        //
        // Re-checks headless_log_stream (redundant for us, but matches the
        // original) then runs if speech_team_count > 0.
        if headless_stream(game_info_ptr).is_null() {
            return;
        }
        let num_teams = game_info.num_teams;
        let speech_count = game_info.speech_team_count;

        // Local aiStack_420 — per-team label widths + max width.
        // The original computes them but the widths are only used as input
        // to `:%*s ` padding in the per-team loop. We need to match exactly.
        const MAX_TEAMS: usize = 32;
        let mut label_widths: [i32; MAX_TEAMS] = [0; MAX_TEAMS];
        let mut max_width: i32 = 0;

        if speech_count > 0 {
            let gi_base = game_info_ptr as *const u8;
            let team_records_base = gi_base.add(0x450);
            for i in 0..(speech_count as usize).min(MAX_TEAMS) {
                let record = team_records_base.add(i * 3000);
                // Team name strlen at record+6.
                let name_start = record.add(6);
                let mut p = name_start;
                while *p != 0 {
                    p = p.add(1);
                }
                // Reproduce original: `name_end - (record + 7)` = name_len - 1.
                let mut width = (p as isize - record.add(7) as isize) as i32;
                // If num_teams != 0 and speech_bank_id >= 0, add bank name length + 3.
                let speech_bank_id = *(record as *const i8);
                if num_teams != 0 && speech_bank_id >= 0 {
                    let bank = gi_base.add((speech_bank_id as usize) * 0x50 + 4);
                    let mut bp = bank;
                    while *bp != 0 {
                        bp = bp.add(1);
                    }
                    let bank_len_end = bp;
                    // Exactly the original expression:
                    //   bank_end + (name_end - (record+7)) + (3 - (bank+1))
                    // = bank_end - bank - 1 + name_len - 1 + 3
                    // = bank_len + name_len_minus_1 + 2
                    width = bank_len_end as i32 + width + (3 - (bank.add(1) as i32));
                }
                label_widths[i] = width;
                if max_width < width {
                    max_width = width;
                }
            }
        }

        // Header print: WA_LOAD_STRING(0x71D) → "\n%s\n".
        let stats_header_label = wa_load_string(0x71d);
        let fmt_stats_header = rb(va::G_FMT_STATS_HEADER) as *const u8;
        if codepage_recode_on() {
            let snprintf1: Snprintf1 = core::mem::transmute(snprintf_s_ptr());
            snprintf1(
                rb(va::G_LOG_SCRATCH_BUF) as *mut u8,
                0x3fff,
                usize::MAX,
                fmt_stats_header,
                stats_header_label as *const u8,
            );
            recode_and_fputs(stream);
        } else {
            let fprintf2: Fprintf2 = core::mem::transmute(fprintf_ptr());
            fprintf2(stream, fmt_stats_header, stats_header_label as *const u8);
        }

        // Per-team loop (1..=speech_count).
        if speech_count > 0 {
            // Stack buffers — the original uses auStack_440, auStack_450,
            // auStack_430 (16 bytes each for WriteHeadlessLog timestamps).
            // WriteHeadlessLog writes `HH:MM:SS.CC\0` (12 bytes) so 16 is
            // safe.
            let mut buf_a: [u8; 16] = [0; 16];
            let mut buf_b: [u8; 16] = [0; 16];
            let mut buf_c: [u8; 16] = [0; 16];

            for i in 0..speech_count as u32 {
                let team_idx_plus_1 = i + 1;

                // Print team label header (localized) via fprintf/snprintf
                // with ":%*s " — width = max_width - label_widths[i], string
                // is the empty string at 0x643F2B.
                bridge_load_log_label_b(ddgame, team_idx_plus_1);

                let pad_width = max_width - label_widths[i as usize];
                let empty = rb(va::G_EMPTY_CSTR) as *const u8;
                let fmt_pad = rb(va::G_FMT_TEAM_LABEL_PAD) as *const u8;
                if codepage_recode_on() {
                    let snprintf4: Snprintf4 = core::mem::transmute(snprintf_s_ptr());
                    snprintf4(
                        rb(va::G_LOG_SCRATCH_BUF) as *mut u8,
                        0x3fff,
                        usize::MAX,
                        fmt_pad,
                        pad_width,
                        empty,
                    );
                    recode_and_fputs(stream);
                } else {
                    let fprintf4: Fprintf4 = core::mem::transmute(fprintf_ptr());
                    fprintf4(stream, fmt_pad, pad_width, empty);
                }

                // Per-team stat fields live at offsets relative to ddgame's
                // team-stats table. The disasm uses EBP=0x7EC0 as the base
                // offset and steps +4 per team (but for `[EDX+EBP*1 + ...]`
                // indexing — so EBP is per-team). Let's mirror the disasm:
                //   time_total   = [ddgame + 0x7EC0 + i*4]        (EBP)
                //   time_used    = [ddgame + 0x7EC0 + i*4 - 0x18] (-0x18)
                //   turn_count   = [ddgame + 0x7EC0 + i*4 + 0x18] (+0x18)
                let ebp = 0x7ec0u32 + i * 4;
                let dd_base = ddgame as *const u8;
                let time_total = *(dd_base.add(ebp as usize) as *const i32);
                let time_used = *(dd_base.add((ebp - 0x18) as usize) as *const i32);
                let turn_count_u = *(dd_base.add((ebp + 0x18) as usize) as *const u32);

                // WriteHeadlessLog(time_used, sprintf, &buf_a) — time-used.
                bridge_write_headless_log(
                    time_used,
                    CRT_SPRINTF_ADDR as *const c_void,
                    buf_a.as_mut_ptr(),
                );
                // WriteHeadlessLog(time_total, sprintf, &buf_b) — time-total.
                bridge_write_headless_log(
                    time_total,
                    CRT_SPRINTF_ADDR as *const c_void,
                    buf_b.as_mut_ptr(),
                );
                // WriteHeadlessLog(time_total + time_used, sprintf, &buf_c).
                bridge_write_headless_log(
                    time_total.wrapping_add(time_used),
                    CRT_SPRINTF_ADDR as *const c_void,
                    buf_c.as_mut_ptr(),
                );

                // Load 4 localized labels: 0x721, 0x720, 0x71F, 0x71E.
                let lbl_721 = wa_load_string(0x721);
                let lbl_720 = wa_load_string(0x720);
                let lbl_71f = wa_load_string(0x71f);
                let lbl_71e = wa_load_string(0x71e);

                // Final per-team print: "%s %s, %s %s, %s %s, %s %u\n".
                // Disasm order (pushes right-to-left):
                //   push turn_count            (arg 8, u32)
                //   push lbl_721               (arg 7, the %s before %u)
                //   push buf_c                 (arg 6)
                //   push lbl_720               (arg 5)
                //   push buf_b                 (arg 4)
                //   push lbl_71f               (arg 3)
                //   push buf_a                 (arg 2)
                //   push lbl_71e               (arg 1 of variadic = arg 5 of fprintf)
                //   push fmt                   (arg 4 of fprintf)
                //   [push stream]              (arg 1 of fprintf, direct path)
                //   call fprintf
                //
                // Variadic order (fmt first → %s slots in order): lbl_71e,
                // buf_a, lbl_71f, buf_b, lbl_720, buf_c, lbl_721, turn_count.
                let fmt_stats = rb(va::G_FMT_TEAM_STATS) as *const u8;
                if codepage_recode_on() {
                    let snprintf8: Snprintf8 = core::mem::transmute(snprintf_s_ptr());
                    snprintf8(
                        rb(va::G_LOG_SCRATCH_BUF) as *mut u8,
                        0x3fff,
                        usize::MAX,
                        fmt_stats,
                        lbl_71e as *const u8,
                        buf_a.as_ptr(),
                        lbl_71f as *const u8,
                        buf_b.as_ptr(),
                        lbl_720 as *const u8,
                        buf_c.as_ptr(),
                        lbl_721 as *const u8,
                        turn_count_u,
                    );
                    recode_and_fputs(stream);
                } else {
                    let fprintf8: Fprintf8 = core::mem::transmute(fprintf_ptr());
                    fprintf8(
                        stream,
                        fmt_stats,
                        lbl_71e as *const u8,
                        buf_a.as_ptr(),
                        lbl_71f as *const u8,
                        buf_b.as_ptr(),
                        lbl_720 as *const u8,
                        buf_c.as_ptr(),
                        lbl_721 as *const u8,
                        turn_count_u,
                    );
                }
            }
        }

        // putc('\n', stream) after per-team stats.
        call_putc(b'\n' as i32, stream);

        // ── End-of-round turn count line ─────────────────────────────
        let turn_count = (*ddgame).round_turn_count;
        if turn_count != 0 {
            let lbl_722 = wa_load_string(0x722);
            let fmt_turn = rb(va::G_FMT_TURN_COUNT) as *const u8;
            if codepage_recode_on() {
                let snprintf2i: Snprintf2Int = core::mem::transmute(snprintf_s_ptr());
                snprintf2i(
                    rb(va::G_LOG_SCRATCH_BUF) as *mut u8,
                    0x3fff,
                    usize::MAX,
                    fmt_turn,
                    lbl_722 as *const u8,
                    turn_count,
                );
                recode_and_fputs(stream);
            } else {
                let fprintf_ti: Fprintf2Int = core::mem::transmute(fprintf_ptr());
                fprintf_ti(stream, fmt_turn, lbl_722 as *const u8, turn_count);
            }
        }
    }
}

/// Drain the replay input queue at `wrapper.render_buffer_a + 0x14`.
///
/// Layout of each queue node (observed in disasm):
///   +0x00  size (u32) — includes header; payload is `size - 4` bytes
///   +0x04  next (ptr)
///   +0x08  msg_type (u32)
///   +0x0C  payload[size - 4]
///
/// Nodes larger than 0x40C are treated as malformed and drop the loop.
unsafe fn input_queue_drain(wrapper: *mut DDGameWrapper) {
    unsafe {
        let mut local_buf: [u8; 0x408] = [0; 0x408];
        let render_buf = (*wrapper).render_buffer_a;
        loop {
            // list_head at render_buffer_a + 0x14.
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
                // memcpy(local_buf, node+0x0C, payload_size).
                core::ptr::copy_nonoverlapping(
                    node.add(0x0c),
                    local_buf.as_mut_ptr(),
                    payload_size as usize,
                );
            }
            let msg_type = *(node.add(0x8) as *const u32);

            // Classify: thiscall(ECX=render_buf), returns u64 = (edx:eax).
            let packed = bridge_classify_input_msg(render_buf);
            let keep = packed as u32;
            let edx = (packed >> 32) as u32;
            if keep == 0 {
                break;
            }

            if msg_type == 0x16 {
                if edx == 7 || edx == 9 {
                    break;
                }
                let cur_phase = (*wrapper).game_end_phase;
                if cur_phase == edx {
                    // fall through to dispatch
                } else if cur_phase != 0 {
                    continue; // skip this message, next iteration
                } else {
                    (*wrapper).game_end_phase = edx;
                    continue;
                }
            }

            // Dispatch: usercall(EAX=&local_buf) + stdcall(wrapper, msg_type, payload_size).
            bridge_dispatch_input_msg(local_buf.as_ptr(), wrapper, msg_type, payload_size);
            if msg_type == 2 {
                let ddgame = (*wrapper).ddgame;
                (*ddgame).frame_counter = (*ddgame).frame_counter.wrapping_add(1);
            }
        }
    }
}
