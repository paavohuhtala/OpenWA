//! Rust port of `DDGameWrapper__DispatchFrame` (0x529160).
//!
//! Main frame timing/simulation dispatcher. Called each frame by
//! `advance_frame`. Computes delta time, decides how many game frames to
//! advance, dispatches them via `StepFrame`, and handles post-frame timing,
//! headless log output, and game-end detection.

use openwa_core::fixed::{Fixed, Fixed64};
use windows_sys::Win32::System::Threading::ExitProcess;

use super::fixed_slew::fixed_slew_toward;
use crate::address::va;
use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::engine::clock::read_current_time;
use crate::engine::ddgame::DDGame;
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_session::get_game_session;
use crate::engine::game_state;
use crate::input::keyboard::DDKeyboard;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;
use crate::task::{CTask, CTaskTurnGame};

// ─── Runtime addresses ─────────────────────────────────────────────────────
//
// All sub-functions use `usercall(EAX=this)` or `usercall(ESI=this)` where
// `this` is `*mut DDGameWrapper`. The bridges below set the appropriate
// register, then `JMP`/`CALL` the target. `RET imm16` on each target cleans
// the remaining stdcall params.

static mut UPDATE_FRAME_TIMING_ADDR: u32 = 0;
static mut CALC_TIMING_RATIO_ADDR: u32 = 0;
static mut INIT_FRAME_DELAY_ADDR: u32 = 0;
static mut NETWORK_UPDATE_ADDR: u32 = 0;
static mut PEER_INPUT_QUEUE_SCAN_ADDR: u32 = 0;
static mut SHOULD_INTERPOLATE_OFFLINE_TAIL_ADDR: u32 = 0;
static mut SETUP_FRAME_PARAMS_ADDR: u32 = 0;
static mut PROCESS_NETWORK_FRAME_ADDR: u32 = 0;

/// Initialize all bridge addresses. Must be called once at DLL load.
pub unsafe fn init_dispatch_addrs() {
    unsafe {
        UPDATE_FRAME_TIMING_ADDR = rb(va::DDGAMEWRAPPER_UPDATE_FRAME_TIMING);
        CALC_TIMING_RATIO_ADDR = rb(va::DDGAMEWRAPPER_CALC_TIMING_RATIO);
        INIT_FRAME_DELAY_ADDR = rb(va::DDGAMEWRAPPER_INIT_FRAME_DELAY);
        NETWORK_UPDATE_ADDR = rb(va::DDGAMEWRAPPER_NETWORK_UPDATE);
        PEER_INPUT_QUEUE_SCAN_ADDR = rb(va::DDGAMEWRAPPER_PEER_INPUT_QUEUE_SCAN);
        SHOULD_INTERPOLATE_OFFLINE_TAIL_ADDR =
            rb(va::DDGAMEWRAPPER_SHOULD_INTERPOLATE_OFFLINE_TAIL);
        SETUP_FRAME_PARAMS_ADDR = rb(va::DDGAMEWRAPPER_SETUP_FRAME_PARAMS);
        PROCESS_NETWORK_FRAME_ADDR = rb(va::DDGAMEWRAPPER_PROCESS_NETWORK_FRAME);
        super::step_frame::init_step_frame_addrs();
        crate::engine::log_sink::init_log_sink_addrs();
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

bridge_eax_this!(bridge_init_frame_delay, INIT_FRAME_DELAY_ADDR, ());

// Bridge for `DDGameWrapper__PeerInputQueueScan_Maybe` (0x0052E880).
// Usercall EAX=this + 1 stdcall stack param (peer_idx), RET 0x4. Returns
// nonzero in AL if any non-trivial message type is pending in the per-peer
// input queue.
//
// Still bridged — its own callee `NetSession__PeerInputQueuePop_Maybe`
// (0x0053E300) would require further bridging, and this whole code path is
// network-only, so headless replay tests don't exercise it.
bridge_eax_this_stdcall!(
    bridge_peer_input_queue_scan,
    PEER_INPUT_QUEUE_SCAN_ADDR,
    (u32) -> u8
);

/// Bridge for `DDGameWrapper__ShouldInterpolate_OfflineTail_Maybe` (0x0052F9C0).
/// Plain stdcall(wrapper), RET 0x4. Tail callee of the offline
/// `ShouldInterpolate` branch, still bridged (205 instructions, 51 basic
/// blocks — too much for an incidental port).
///
/// Returns nonzero in AL when the final path decides interp must be suppressed.
unsafe fn bridge_should_interpolate_offline_tail(wrapper: *mut DDGameWrapper) -> u8 {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut DDGameWrapper) -> u8 =
            core::mem::transmute(SHOULD_INTERPOLATE_OFFLINE_TAIL_ADDR as usize);
        func(wrapper)
    }
}

bridge_eax_this_stdcall!(bridge_setup_frame_params, SETUP_FRAME_PARAMS_ADDR, (Fixed, Fixed, Fixed) -> ());

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

// ─── Public bridge wrappers ────────────────────────────────────────────────

/// Rust port of `DDGameWrapper__IsReplayMode` (0x00537060).
///
/// A replay is "running" when the game info has a non-zero `replay_ticks`,
/// the wrapper's `game_state` is either `INITIALIZED` or `ROUND_ENDING`,
/// two unnamed flag fields (`_field_424` / `_field_434`) are zero, the
/// session's `flag_5c` is zero, and the simulation has reached the replay's
/// recorded end frame (`frame_counter >= replay_end_frame`).
pub unsafe fn is_replay_mode(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        let ddgame = (*wrapper).ddgame;
        let gi = &*(*ddgame).game_info;
        if gi.replay_ticks == 0 {
            return false;
        }
        let gs = (*wrapper).game_state;
        if gs != game_state::INITIALIZED && gs != game_state::ROUND_ENDING {
            return false;
        }
        if (*wrapper)._field_434 != 0 || (*wrapper)._field_424 != 0 {
            return false;
        }
        if (*get_game_session()).flag_5c != 0 {
            return false;
        }
        (*ddgame).frame_counter >= gi.replay_end_frame
    }
}

/// Rust port of `DDGameWrapper__ShouldContinueFrameLoop` (0x0052A840).
///
/// Gates the inner frame-catch-up loop in `dispatch_frame`. Returns `true`
/// (keep looping) while the wall-clock time since `last_frame_time` is
/// within a budget of `multiplier × elapsed`. `multiplier` is 3× for
/// regular play or when the current game speed target matches the scheme
/// config, and 10× for replay / fast-forward with a non-matching speed —
/// those paths get a longer wall-clock window before the loop gives up.
///
/// Always returns `true` before the first frame (`last_frame_time == 0`).
pub unsafe fn should_continue_frame_loop(wrapper: *mut DDGameWrapper, elapsed: u64) -> bool {
    unsafe {
        if (*wrapper).last_frame_time == 0 {
            return true;
        }

        let ddgame = (*wrapper).ddgame;
        let gi = &*(*ddgame).game_info;

        let regular_play = (*wrapper).replay_flag_a == 0 && (*ddgame).fast_forward_request == 0;
        let speed_matches_scheme = (*ddgame).game_speed_target.to_raw() == gi.game_speed_config;
        let multiplier: u64 = if regular_play || speed_matches_scheme {
            3
        } else {
            10
        };

        let budget = multiplier.wrapping_mul(elapsed);
        let wall_elapsed =
            crate::engine::clock::read_current_time().wrapping_sub((*wrapper).last_frame_time);

        budget >= wall_elapsed
    }
}

/// Rust port of `DDGameWrapper::ShouldInterpolate` (0x00534880).
/// Returns with inverted semantics vs the disasm: WA's function returns
/// nonzero in AL when interp is SUPPRESSED; we return `true` when it's
/// computed (accum_c path).
///
/// Returns `true` whenever render interpolation should be computed and the
/// main-loop `frame_accum_c` path should be taken. Returns `false` when the
/// wrapper is in a paused-style phase (`game_end_phase ∈ {1,2,6,7,9}`),
/// `render_scale_fade_request != 0`, or one of the offline bail gates fires
/// — in those cases the `frame_accum_a` branch is used instead.
///
/// Dispatch:
/// - **Online** (`ddgame.net_session != null`): delegates to
///   `should_interpolate_online` (pure Rust; the inner peer-input-queue
///   scan `DDGameWrapper__PeerInputQueueScan_Maybe` (0x0052E880) remains
///   bridged).
/// - **Offline**: short-circuits to `true` (interpolate) when `_field_434 != 0`,
///   `g_GameSession.flag_5c != 0`, or all three of `replay_flag_b != 0`,
///   `_field_410 != 0`, `game_info.input_state_f918 == 0` hold. Otherwise
///   delegates to `should_interpolate_offline` (pure Rust; the deep tail
///   `DDGameWrapper__ShouldInterpolate_OfflineTail_Maybe` (0x0052F9C0)
///   remains bridged).
pub unsafe fn should_interpolate(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        let phase = (*wrapper).game_end_phase;
        if matches!(phase, 1 | 2 | 6 | 7 | 9) {
            return true;
        }
        if (*wrapper).render_scale_fade_request != 0 {
            return true;
        }

        let ddgame = (*wrapper).ddgame;
        if !(*ddgame).net_session.is_null() {
            return should_interpolate_online(wrapper);
        }

        if (*wrapper)._field_434 != 0 {
            return true;
        }
        if (*get_game_session()).flag_5c != 0 {
            return true;
        }

        let all_offline_gates = (*wrapper).replay_flag_b != 0
            && (*wrapper)._field_410 != 0
            && (*(*ddgame).game_info).input_state_f918 == 0;
        if all_offline_gates {
            return true;
        }

        should_interpolate_offline(wrapper)
    }
}

/// Rust port of `DDGameWrapper__ShouldInterpolate_OfflineCheck` (0x0052F770)
/// — offline branch of `ShouldInterpolate`, only caller is the outer
/// `should_interpolate`. The native WA semantics (return nonzero in AL to
/// SUPPRESS interpolation) are inverted here so this function follows the
/// same convention as `should_interpolate`: `true` = compute interp.
///
/// Gates, in order:
/// 1. `ddgame.fast_forward_request != 0` → suppress.
/// 2. `game_info.replay_config_flag == 0` → suppress.
/// 3. `team_arena.active_team_count != 0` AND
///    `ddgame._field_7dbc[team_arena.last_active_alliance] == 0` → suppress.
/// 4. `wrapper._field_49c <= 0xC` (version gate) → suppress.
/// 5. Per-team sweep over `game_info.num_teams`: if any team has both its
///    `_field_7dbc[i]` flag set and its `team_starting_marker[i] == 0`,
///    compute interp (early return).
/// 6. Fall through to `DDGameWrapper__ShouldInterpolate_OfflineTail_Maybe`
///    (0x0052F9C0, still bridged).
///
/// Note: step 3 uses byte-level pointer arithmetic from the `ddgame` base to
/// match the original's unchecked read — `last_active_alliance` can be `-1`
/// (sentinel), which would read just before `_field_7dbc` (land in the last
/// byte of `_field_7d88`). Safe Rust array indexing would panic instead.
unsafe fn should_interpolate_offline(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        let ddgame = (*wrapper).ddgame;

        if (*ddgame).fast_forward_request != 0 {
            return false;
        }

        let gi = (*ddgame).game_info;
        let replay_cfg = (*gi).replay_config_flag;
        if replay_cfg == 0 {
            return false;
        }

        let arena = &(*ddgame).team_arena;
        let alliance_gate_open = arena.active_team_count == 0 || {
            let flag_ptr =
                (ddgame as *const u8).offset(0x7dbc + arena.last_active_alliance as isize);
            *flag_ptr != 0
        };
        if !alliance_gate_open {
            return false;
        }

        if (*wrapper)._field_49c <= 0xc {
            return false;
        }

        let team_count = (*gi).num_teams;
        for i in 0..team_count as usize {
            if (*ddgame)._field_7dbc[i] != 0 && (*wrapper).team_starting_marker[i] == 0 {
                return true;
            }
        }

        bridge_should_interpolate_offline_tail(wrapper) == 0
    }
}

// ─── Online ShouldInterpolate branch ───────────────────────────────────────
// `DDGameWrapper__ShouldInterpolate_OnlineCheck` (0x0052DC70) + its
// three gate helpers (`_OnlineGate_ScoringB/ScoringA/StartingMarker_Maybe`
// at 0x0052D830 / 0x0052D920 / 0x0052DB60).
//
// Only reached when `ddgame.net_session != null`. Offline headless replay
// tests do NOT exercise this code path — it's a mechanical disasm
// transcription with no runtime verification. Every register convention
// and field offset below is pinned to the ASM at the matching VA; if any
// hooks into this path seem off, re-check the disassembly against the
// corresponding function before assuming the Rust is wrong.
//
// Shared iteration shape for D830/D920: "count = (self_peer_idx == 0) ?
// peer_count : 1". Semantics suspected to be "server iterates all peers;
// client only checks peer 0 (the server)" — unconfirmed.

/// Port of `DDGameWrapper__ShouldInterpolate_OnlineGate_ScoringB_Maybe`
/// (0x0052D830). Usercall EAX=this, plain RET. Returns `false` if any
/// scoring peer (per `team_scoring_b[i] > 1`) has `peer_score(i) > 70`,
/// else `true`.
unsafe fn online_gate_d830(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        let net = (*(*wrapper).ddgame).net_session;
        let count = if (*net).self_peer_idx == 0 {
            (*net).peer_count
        } else {
            1
        };
        for i in 0..count {
            if (*wrapper).team_scoring_b[i as usize] > 1 {
                let score = ((*(*net).vtable).peer_score)(net, i as u32);
                if score > 0x46 {
                    return false;
                }
            }
        }
        true
    }
}

/// Port of `DDGameWrapper__ShouldInterpolate_OnlineGate_ScoringA_Maybe`
/// (0x0052D920). Usercall EAX=this, plain RET. Returns `false` if any peer
/// i has `peer_pending_maybe(i) != 0` AND `team_scoring_a[i] > 0`, else
/// `true`.
unsafe fn online_gate_d920(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        let net = (*(*wrapper).ddgame).net_session;
        let count = if (*net).self_peer_idx == 0 {
            (*net).peer_count
        } else {
            1
        };
        for i in 0..count {
            let pending = ((*(*net).vtable).peer_pending_maybe)(net, i as u32);
            if pending != 0 && (*wrapper).team_scoring_a[i as usize] > 0 {
                return false;
            }
        }
        true
    }
}

/// Port of `DDGameWrapper__ShouldInterpolate_OnlineGate_StartingMarker_Maybe`
/// (0x0052DB60). Usercall EAX=this, plain RET. Returns `true` iff every
/// entry `team_starting_marker[0..net.peer_count]` is non-zero.
unsafe fn online_gate_db60(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        let net = (*(*wrapper).ddgame).net_session;
        let count = (*net).peer_count;
        let mut zero_count = 0;
        for i in 0..count as usize {
            if (*wrapper).team_starting_marker[i] == 0 {
                zero_count += 1;
            }
        }
        zero_count == 0
    }
}

/// Rust port of `DDGameWrapper__ShouldInterpolate_OnlineCheck` (0x0052DC70)
/// — online branch of `ShouldInterpolate`, only caller is the outer
/// `should_interpolate`. WA semantics inverted for readability: returns
/// `true` if interpolation should be computed.
///
/// Dispatch:
/// 1. All three gates D830/D920/DB60 must pass; otherwise interp is computed
///    (early `return true`).
/// 2. If `ddgame.team_arena.enemy_team_count == 0` → suppress (WA returned 1).
/// 3. Otherwise delegate to `DDGameWrapper__PeerInputQueueScan_Maybe`
///    (0x0052E880, still bridged) passing `team_arena.last_active_alliance`
///    as peer_idx; that function returns nonzero iff any non-skipped
///    message is pending, which WA propagates as "suppress interp".
unsafe fn should_interpolate_online(wrapper: *mut DDGameWrapper) -> bool {
    unsafe {
        if !online_gate_d830(wrapper) || !online_gate_d920(wrapper) || !online_gate_db60(wrapper) {
            return true;
        }

        let arena = &(*(*wrapper).ddgame).team_arena;
        if arena.enemy_team_count == 0 {
            return false;
        }

        let peer_idx = arena.last_active_alliance as u32;
        bridge_peer_input_queue_scan(wrapper, peer_idx) == 0
    }
}

/// Rust port of `DDGameWrapper__AdvanceFrameCounters` (0x0052AAA0).
///
/// Steps three Fixed slew states toward targets derived from wrapper flags,
/// then bumps `ddgame.scaled_frame_accum` by `advance_ratio` and decays the
/// `_field_450` countdown by the same amount (clamped at 0).
///
/// Slews (all via [`fixed_slew_toward`]):
/// - **Slot A** — state `_field_3fc`, target `Fixed::ONE` if `_field_40c != 0`
///   else `Fixed::ZERO`. `min_step = min_step_a`, `rate = rate_a`. The
///   `force_set` flag is driven by `game_info._field_f398` (sound-suppression
///   latch). When the slew reports already-settled, sets
///   `wrapper.game_mode_flag = 1`.
/// - **Slot B** — state `_field_454`, target `Fixed::ONE` when
///   `_field_40c == 0 && _field_414 == 0 && _field_450 != 0`, else `Fixed::ZERO`.
///   `min_step = min_step_b`, `rate = rate_b`, no force_set. Note this uses
///   the *updated* `_field_450` after the countdown above — intentional per the
///   original ordering.
/// - **Slot C** — state `_field_27c`, target `Fixed::ONE` when
///   `_field_278 >= 0x65`, else `Fixed::ZERO`. Same `min_step_b` / `rate_b`.
///
/// Note: ASM at `0x0052AAA0` loads EDI from `[ESI+0x40c]` on entry purely to
/// compute slot A's target; the register is overwritten again at `0x0052AB40`
/// (MOV EDI,[ESP+0x1C]) before FixedSlewToward would see it, so this port
/// has no implicit-EDI concern.
unsafe fn advance_frame_counters(
    wrapper: *mut DDGameWrapper,
    min_step_a: Fixed,
    rate_a: Fixed,
    min_step_b: Fixed,
    rate_b: Fixed,
    advance_ratio: Fixed,
) {
    unsafe {
        let ddgame = (*wrapper).ddgame;
        let gi = &*(*ddgame).game_info;

        // Slot A slew.
        let target_a = if (*wrapper)._field_40c != 0 {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        if fixed_slew_toward(
            &mut (*wrapper)._field_3fc,
            target_a,
            min_step_a,
            rate_a,
            gi._field_f398 != 0,
        ) {
            (*wrapper).game_mode_flag = 1;
        }

        // Running frame counter + countdown.
        (*ddgame).scaled_frame_accum = (*ddgame).scaled_frame_accum.wrapping_add(advance_ratio);

        if (*wrapper)._field_450 != Fixed::ZERO {
            let next = (*wrapper)._field_450.wrapping_sub(advance_ratio);
            (*wrapper)._field_450 = if next < Fixed::ZERO {
                Fixed::ZERO
            } else {
                next
            };
        }

        // Slot B slew — uses the *updated* _field_450.
        let target_b = if (*wrapper)._field_40c == 0
            && (*wrapper)._field_414 == 0
            && (*wrapper)._field_450 != Fixed::ZERO
        {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        let _ = fixed_slew_toward(
            &mut (*wrapper)._field_454,
            target_b,
            min_step_b,
            rate_b,
            false,
        );

        // Slot C slew.
        let target_c = if (*wrapper)._field_278 >= 0x65 {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        let _ = fixed_slew_toward(
            &mut (*wrapper)._field_27c,
            target_c,
            min_step_b,
            rate_b,
            false,
        );
    }
}

/// Rust port of `DDGameWrapper::StepRenderScaleFade` (0x005344B0).
///
/// Steps `DDGame::render_scale` one frame toward a target selected by the
/// sign of `wrapper.render_scale_fade_request`:
/// - `< 0` → fade in (target `Fixed::ONE`), latch cleared to 0 once reached.
/// - `> 0` / `0` → fade out (target `Fixed::ZERO`), latch cleared once reached.
///
/// Step size is `0x0F5C` per frame (~0.06 in 16.16), clamped to `[0, 1.0]`.
/// Returns the post-update latch value; `0` means the fade has settled.
unsafe fn step_render_scale_fade(wrapper: *mut DDGameWrapper) -> i32 {
    const FADE_STEP: Fixed = Fixed::from_raw(0x0F5C);

    unsafe {
        let ddgame = (*wrapper).ddgame;
        let request = (*wrapper).render_scale_fade_request;
        let target = if request < 0 { Fixed::ONE } else { Fixed::ZERO };

        let mut scale = (*ddgame).render_scale;
        if scale < target {
            scale += FADE_STEP;
        } else if scale > target {
            scale -= FADE_STEP;
        }
        scale = scale.clamp(Fixed::ZERO, Fixed::ONE);
        (*ddgame).render_scale = scale;

        if (request < 0 && scale == Fixed::ONE) || (request > 0 && scale == Fixed::ZERO) {
            (*wrapper).render_scale_fade_request = 0;
        }

        (*wrapper).render_scale_fade_request
    }
}

/// Frame phases in which `reset_frame_state` must skip its frame-counter
/// increment (pause / end-of-round / similar). Matches the set used by
/// `step_frame` Block B's `skip_input` check.
#[inline]
fn is_paused_phase(v: i32) -> bool {
    matches!(v, 1 | 2 | 6 | 7 | 9)
}

/// Rust port of `DDGameWrapper__ResetFrameState` (0x0052A910).
///
/// Runs once per frame between step iterations. Always broadcasts msg 5 to
/// `task_turn_game`. In headful mode also runs `init_frame_delay`. Then, if
/// neither `hud_status_code` nor `game_end_phase` sit in the paused set
/// (`{1,2,6,7,9}`) and input-hooking is inactive or the arena has caught up,
/// runs the render-scale fade step and — if the fade says the scene is
/// fully composed — bumps `DDGame::_field_77d4` (the "active gameplay
/// frames" counter).
pub unsafe fn reset_frame_state(wrapper: *mut DDGameWrapper) {
    unsafe {
        let task = (*wrapper).task_turn_game;
        CTaskTurnGame::handle_message_raw(task, task as *mut CTask, 5, 0, core::ptr::null());

        let ddgame = (*wrapper).ddgame;

        if (*ddgame).is_headful != 0 {
            bridge_init_frame_delay(wrapper);
        }

        if is_paused_phase((*ddgame).hud_status_code)
            || is_paused_phase((*wrapper).game_end_phase as i32)
        {
            return;
        }

        // Input-hook gate: when hooked, wait until the arena's worm-count
        // catches up with the team-count before counting the frame.
        let hook_mode = *(rb(va::G_INPUT_HOOK_MODE) as *const u32);
        if hook_mode != 0 {
            let arena = &(*ddgame).team_arena;
            if arena.active_worm_count > arena.active_team_count {
                return;
            }
        }

        if step_render_scale_fade(wrapper) == 0 {
            (*ddgame)._field_77d4 = (*ddgame)._field_77d4.wrapping_add(1);
        }
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

/// Signed 32-bit division matching WA's `Crt__SignedDivMod_Maybe` (0x005D8786).
/// Returns (quotient, remainder) using only the low 32 bits of the dividend.
/// The original uses x86 IDIV which produces both values.
#[inline(always)]
fn wa_div(dividend_lo: i32, divisor: i32) -> (i32, i32) {
    (dividend_lo / divisor, dividend_lo % divisor)
}

/// Subtract a sign-extended i32 remainder from a 64-bit timestamp.
/// Matches the CDQ + SUB + SBB pattern used after `Crt__SignedDivMod_Maybe`
/// (0x005D8786) in DispatchFrame.
#[inline(always)]
fn time_sub_i32(time: u64, remainder: i32) -> u64 {
    time.wrapping_sub(remainder as i64 as u64)
}

// ─── Internal helpers ──────────────────────────────────────────────────────

/// Secondary pause detection (LAB_0052928c in the decompile).
///
/// Computes sec_delta from pause_secondary, bounds-checks it, then either
/// resets `timing_jitter_state` or XORs it with the low bit of
/// `sec_delta / (freq/2)`. The `CDQ+SUB+SBB` pattern after the IDIV is
/// modelled by `time_sub_i32`.
unsafe fn handle_secondary_pause(wrapper: *mut DDGameWrapper, time: u64, freq: u64) {
    unsafe {
        let sec_delta = time.wrapping_sub((*wrapper).pause_secondary);

        if (sec_delta as i64) >= 0 && sec_delta <= freq.wrapping_mul(2) {
            if (*wrapper).timing_jitter_state == 2 {
                (*wrapper).timing_jitter_state = 1;
                (*wrapper).pause_secondary = time;
            } else {
                let half_freq = (freq as i32) / 2;
                let (sec_ratio, sec_remainder) = wa_div(sec_delta as i32, half_freq);
                (*wrapper).timing_jitter_state ^= sec_ratio & 1;
                (*wrapper).pause_secondary = time_sub_i32(time, sec_remainder);
            }
            return;
        }
        // Delta negative or too large — reset.
        (*wrapper).timing_jitter_state = 1;
        (*wrapper).pause_secondary = time;
    }
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
pub unsafe fn dispatch_frame(wrapper: *mut DDGameWrapper, time: u64, freq: u64) {
    unsafe {
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

        let is_headful = (*ddgame).is_headful != 0;
        let mut elapsed: u64 = 0;
        // `bVar18` in the decompile: true when we took the "normal" timing
        // branch. Gates the secondary-pause fallthrough and the `elapsed`
        // computation from `initial_ref`.
        let mut used_normal_path = false;

        if is_headful {
            if (*wrapper).pause_detect == 0 {
                (*wrapper).pause_detect = time;
                (*wrapper).pause_secondary = time;
            }

            let is_replay = is_replay_mode(wrapper);

            if !is_replay || saved_frame_delay >= 0 {
                let delta = time.wrapping_sub((*wrapper).pause_detect);
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
                    (*wrapper).pause_detect = time_sub_i32(time, remainder);
                    handle_secondary_pause(wrapper, time, freq);
                } else {
                    // Delta out of range — resync pause detection.
                    (*wrapper).pause_detect = time;
                    handle_secondary_pause(wrapper, time, freq);
                }
            } else {
                // Replay mode with negative frame delay: derive elapsed from the
                // replay tick rate and skip secondary-pause handling.
                let gi = &*(*ddgame).game_info;
                elapsed = freq / (gi.replay_ticks as u64);
                let (ratio, remainder) = wa_div(elapsed as i32, frame_interval as i32);
                calc_timing_ratio(wrapper, ratio);
                (*wrapper).pause_detect = time_sub_i32(time, remainder);

                if (*wrapper).timing_jitter_state == 2 {
                    (*wrapper).timing_jitter_state = 1;
                    (*wrapper).pause_secondary = time;
                } else {
                    let half_freq = (freq as i32) / 2;
                    let (sec_ratio, sec_remainder) = wa_div(elapsed as i32, half_freq);
                    (*wrapper).timing_jitter_state ^= sec_ratio & 1;
                    (*wrapper).pause_secondary = time_sub_i32(time, sec_remainder);
                }
            }

            if (*wrapper).initial_ref == 0 {
                (*wrapper).initial_ref = time;
            }

            if used_normal_path {
                let init_delta = time.wrapping_sub((*wrapper).initial_ref);
                if (init_delta as i64) >= 0 {
                    elapsed = init_delta;
                    (*wrapper).initial_ref = time;
                } else {
                    elapsed = 0;
                    // Original intentionally does NOT update initial_ref when
                    // the delta is negative.
                }
            } else {
                (*wrapper).initial_ref = time;
            }

            // FPU section: the original x87 code uses 80-bit precision; f64 is
            // close enough for rendering-only timing. The 0x6797e8 constant
            // (exact bit pattern from the data section) drives exponential
            // decay smoothing for frame interpolation.
            const RENDER_DECAY: f64 = f64::from_bits(0xC015126E978D4FDF_u64); // ≈ -5.2679

            let elapsed_f = elapsed as f64;
            let freq_f = freq as f64;

            // fps_scaled ≈ fps_product / 2 (before clamping).
            let mut fps_scaled = Fixed::from_raw((elapsed_f * 3.75 * 65536.0 / freq_f) as i32);
            if fps_scaled > Fixed::from_raw(0x1333) && used_normal_path {
                fps_scaled = Fixed::from_raw(0x1333);
            }
            let mut fps_product = Fixed::from_raw((elapsed_f * 7.5 * 65536.0 / freq_f) as i32);
            if fps_product > Fixed::from_raw(0x2666) && used_normal_path {
                fps_product = Fixed::from_raw(0x2666);
            }
            let fixed_render_scale = Fixed::ONE
                - Fixed::from_raw((65536.0 * (elapsed_f * RENDER_DECAY / freq_f).exp()) as i32);

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
                let new_fps_product = Fixed::from_raw((65536.0 * elapsed_f * 7.5 / fd50_f) as i32);
                let new_render_scale = Fixed::ONE
                    - Fixed::from_raw((65536.0 * (elapsed_f * RENDER_DECAY / fd50_f).exp()) as i32);
                let advance_ratio =
                    Fixed::from_raw(frame_fixed.checked_div(frame_duration).unwrap_or(0) as i32);
                advance_frame_counters(
                    wrapper,
                    fps_scaled,
                    fixed_render_scale,
                    new_fps_product,
                    new_render_scale,
                    advance_ratio,
                );
            } else {
                let advance_ratio =
                    Fixed::from_raw(frame_fixed.checked_div(frame_interval).unwrap_or(0) as i32);
                advance_frame_counters(
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
                freq as u32,
                (freq >> 32) as u32,
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
        // end of is_headful block

        if (*wrapper).timing_ref == 0 {
            (*wrapper).timing_ref = time;
        }

        let ref_delta = time.wrapping_sub((*wrapper).timing_ref) as i64;

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

        let now = read_current_time();
        let loop_elapsed = now.wrapping_sub((*wrapper).last_frame_time);

        if !(*ddgame).net_session.is_null() {
            bridge_process_network_frame(
                wrapper,
                time as u32,
                (time >> 32) as u32,
                freq as u32,
                (freq >> 32) as u32,
            );
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
                let clock_raw = (*ddgame).replay_speed_accum.to_raw() as u64 / replay_ticks as u64;
                let speed_val = Fixed::from_raw(clock_raw as i32)
                    - (*ddgame).replay_frame_accum.to_fixed_wrapping();
                (*ddgame).render_interp_a = speed_val;
                (*ddgame).render_interp_b = speed_val;

                // Advance the accumulator by one replay step: 50 Fixed units.
                (*ddgame).replay_speed_accum = (*ddgame)
                    .replay_speed_accum
                    .wrapping_add(Fixed64::from_raw(0x32_0000));

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

                if (frame_counter > replay_end
                    || (frame_counter == replay_end && speed_val > Fixed::ZERO))
                    && (*wrapper).game_end_phase != 1
                {
                    (*wrapper).game_end_phase = 1;
                    (*wrapper).game_end_speed = 0x10000;
                    (*wrapper).game_state = game_state::EXIT;
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

                let max_accum = (*wrapper).frame_accum_a.max((*wrapper).frame_accum_b);

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
                let session = get_game_session();

                if (*session).flag_5c == 0 || !(*ddgame).net_session.is_null() {
                    (*wrapper).frame_accum_b = (*wrapper).frame_accum_b.wrapping_add(frame_time);
                    if (*wrapper).frame_accum_b == frame_duration {
                        (*wrapper).frame_accum_b = 0;
                        reset_frame_state(wrapper);
                    }
                }

                if !should_interpolate(wrapper) {
                    (*wrapper).frame_accum_a = (*wrapper).frame_accum_a.wrapping_add(frame_time);
                    (*wrapper).frame_accum_c = 0;

                    if (*wrapper).frame_accum_a == frame_duration {
                        (*wrapper).frame_accum_a = 0;
                        if !super::step_frame::step_frame(
                            wrapper,
                            &mut frame_step_counter,
                            &mut remaining,
                            frame_time,
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
                    (*wrapper).frame_accum_c = (*wrapper).frame_accum_c.wrapping_add(frame_time);

                    if (*wrapper).frame_accum_c >= frame_duration {
                        (*wrapper).frame_accum_c -= frame_duration;
                        if !super::step_frame::step_frame(
                            wrapper,
                            &mut frame_step_counter,
                            &mut remaining,
                            frame_time,
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
                if speed_val < Fixed::ONE {
                    break;
                }

                let session = get_game_session();
                if (*session).flag_5c == 0 || !(*ddgame).net_session.is_null() {
                    reset_frame_state(wrapper);
                }
                if !super::step_frame::step_frame(
                    wrapper,
                    &mut frame_step_counter,
                    &mut remaining,
                    frame_duration,
                    game_speed_target,
                    saved_game_speed,
                ) {
                    break;
                }
            }

            if !should_continue_frame_loop(wrapper, loop_elapsed) {
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
                    let speed_a = (*wrapper)
                        .frame_accum_a
                        .wrapping_mul(0x10000)
                        .checked_div(frame_duration)
                        .unwrap_or(0) as i32;
                    (*ddgame).render_interp_a = Fixed::from_raw(speed_a);

                    let speed_b = (*wrapper)
                        .frame_accum_b
                        .wrapping_mul(0x10000)
                        .checked_div(frame_duration)
                        .unwrap_or(0) as i32;
                    (*ddgame).render_interp_b = Fixed::from_raw(speed_b);

                    if (*wrapper).frame_delay_counter >= 0 {
                        (*wrapper).frame_accum_a = 0;
                        (*wrapper).frame_accum_b = 0;
                        (*wrapper).frame_accum_c = 0;
                    }

                    let new_target = (*ddgame).game_speed_target.to_raw();
                    if game_speed_target != new_target
                        || saved_game_speed != (*ddgame).game_speed.to_raw()
                    {
                        if saved_frame_delay >= 0 && (*wrapper).frame_delay_counter < 0 {
                            // Speed change while frame delay was active → reset.
                            (*wrapper).frame_accum_a = 0;
                            (*wrapper).frame_accum_b = 0;
                            (*wrapper).frame_accum_c = 0;
                            (*ddgame).render_interp_a = Fixed::ZERO;
                            (*ddgame).render_interp_b = Fixed::ZERO;
                        } else {
                            let new_interval = freq / 50;
                            let new_speed = (*ddgame).game_speed.to_raw();
                            let scale = ((new_speed as i64).wrapping_mul(new_interval as i64)
                                / (new_target as i64))
                                as u64;

                            (*wrapper).frame_accum_a = (((*ddgame).render_interp_a.to_raw() as i64)
                                .wrapping_mul(scale as i64)
                                >> 16)
                                as u64;

                            (*wrapper).frame_accum_b = (((*ddgame).render_interp_b.to_raw() as i64)
                                .wrapping_mul(scale as i64)
                                >> 16)
                                as u64;

                            if (*wrapper).frame_accum_c != 0 {
                                (*wrapper).frame_accum_c = (*wrapper)
                                    .frame_accum_c
                                    .wrapping_mul(scale)
                                    .checked_div(frame_duration)
                                    .unwrap_or(0);
                            }
                        }
                    }
                } else {
                    // Before game start — zero speed.
                    (*ddgame).render_interp_a = Fixed::ZERO;
                    (*ddgame).render_interp_b = Fixed::ZERO;
                }
                (*wrapper).timing_ref = time;
            } else {
                // Replay mode — subtract remaining from reference.
                (*wrapper).timing_ref = time.wrapping_sub(remaining);
            }
        }

        (*wrapper).last_frame_time = read_current_time();

        // Headless log output: format the current frame counter as "HH:MM:SS.CC\n"
        // and write to CRT stdout. ExitProcess(-1) on write failure.
        {
            let gi = &*(*ddgame).game_info;
            if gi.headless_mode != 0 && !gi.headless_log_stream.is_null() {
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

        if (*(*wrapper).ddgame).is_headful != 0 {
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
                (*wrapper).game_state = game_state::ROUND_ENDING;
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
