//! In-game ESC menu state machine.
//!
//! The ESC menu is the in-round overlay shown by pressing Escape — a
//! scoreboard ("First Team to N Wins" header + per-team leaderboard) plus
//! action buttons (Minimize Game, Force Sudden Death, Draw This Round, Quit
//! The Game) and a volume slider. Lives at `runtime._field_30` (the
//! [`MenuPanel`-shaped] item list) with the canvas at `runtime._field_2c`.
//!
//! State at `runtime.esc_menu_state` (i32):
//!  - **0** — closed. [`tick_closed`] polls for Escape to open.
//!  - **1** — open / accepting nav input. Driven WA-side by
//!    `EscMenu_TickState1` (still bridged via [`bridge_state_1_tick`]).
//!  - **2** — confirm / network-end-of-game flow. Driven WA-side by
//!    `EscMenu_TickState2` (still bridged via [`bridge_state_2_tick`]).
//!
//! [`MenuPanel`-shaped]: a 16-item list with stride 0x38 starting at
//! `panel + 0x30`, count at `+0x3B0`, scroll-region rect at `+0x1C..+0x28`.

use openwa_core::fixed::Fixed;

use crate::address::va;
use crate::audio::known_sound_id::KnownSoundId;
use crate::audio::sound_ops::dispatch_global_sound;
use crate::engine::game_info::GameInfo;
use crate::engine::runtime::GameRuntime;
use crate::input::keyboard::KeyboardAction;
use crate::rebase::rb;

// ─── Bridged WA addresses ──────────────────────────────────────────────────

static mut OPEN_ESC_MENU_ADDR: u32 = 0;
static mut STATE_1_TICK_ADDR: u32 = 0;
static mut STATE_2_TICK_ADDR: u32 = 0;

/// Initialize the ESC-menu bridge addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        OPEN_ESC_MENU_ADDR = rb(va::GAME_RUNTIME_OPEN_ESC_MENU);
        STATE_1_TICK_ADDR = rb(va::GAME_RUNTIME_ESC_MENU_STATE_1_TICK);
        STATE_2_TICK_ADDR = rb(va::GAME_RUNTIME_ESC_MENU_STATE_2_TICK);
    }
}

// ─── Bridges ───────────────────────────────────────────────────────────────

/// Bridge for `GameRuntime__OpenEscMenu` (0x00535200) — builds the in-game
/// ESC menu (scoreboard header, leaderboard rows, action buttons + volume
/// slider) into `runtime._field_30`, then sets `esc_menu_state = 1`.
/// Plain `__stdcall(this)`, RET 0x4. 628 instructions, 30 calls — too big
/// for an incidental port.
pub unsafe fn bridge_open_esc_menu(runtime: *mut GameRuntime) {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut GameRuntime) =
            core::mem::transmute(OPEN_ESC_MENU_ADDR as usize);
        func(runtime)
    }
}

/// Bridge for `GameRuntime__EscMenu_TickState1` (0x00535B10) — per-frame
/// tick while the menu is open (`esc_menu_state == 1`); handles arrow-key
/// nav + Enter to activate a menu item. Usercall EDI=this, plain RET.
/// ~159 instructions.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn bridge_state_1_tick(_this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, [esp+8]",
        "call [{addr}]",
        "pop edi",
        "ret 4",
        addr = sym STATE_1_TICK_ADDR,
    );
}

/// Bridge for `GameRuntime__EscMenu_TickState2` (0x00535FC0) — per-frame
/// tick while `esc_menu_state == 2` (confirm / network-end-of-game flow;
/// calls `BeginNetworkGameEnd`). Usercall EDI=this, plain RET. ~176
/// instructions.
#[unsafe(naked)]
pub unsafe extern "stdcall" fn bridge_state_2_tick(_this: *mut GameRuntime) {
    core::arch::naked_asm!(
        "push edi",
        "mov edi, [esp+8]",
        "call [{addr}]",
        "pop edi",
        "ret 4",
        addr = sym STATE_2_TICK_ADDR,
    );
}

// ─── Rust ports ────────────────────────────────────────────────────────────

/// Rust port of `GameRuntime::IsHudActive` (0x00534C30).
///
/// Predicate: "should the ESC menu be allowed to open / stay open?" Calls
/// `WorldRootEntity::hud_data_query` (vtable slot 3) with msg `0x7D3` to
/// fill a 916-byte (`0x394`) scratch buffer with the end-of-round HUD
/// snapshot, then inspects two early DWORDs of that buffer plus several
/// state flags on `runtime` and `world`.
///
/// Returns `true` only when the game is in pure-running mode:
/// - `game_end_phase == 0` (game-over animation not active)
/// - and either `replay_flag_a != 0` (replay short-circuits the buffer
///   and per-runtime flag checks — see WA's `JNZ 0x534C7D` after testing
///   `[ESI+0x490]`), or all of:
///   - `runtime._field_460 == 0`
///   - `world.fast_forward_request == 0`
///   - `buf[1] == 0` and `buf[2] == 0` (DWORDs at offsets +4/+8 of the
///     0x7D3 response — `buf[0]` is intentionally ignored by WA)
///
/// Called from [`super::dispatch_frame::setup_frame_params`] and from
/// [`tick_closed`]. Hooked at the WA address via
/// `usercall_trampoline!(reg = esi)` so the still-WA-side caller
/// `OpenEscMenu` routes through this Rust port.
pub unsafe fn is_hud_active(runtime: *mut GameRuntime) -> bool {
    unsafe {
        let mut buf: [u32; 0xE5] = [0; 0xE5];
        let task = (*runtime).world_root;
        ((*(*task).base.vtable).hud_data_query)(task, 0x7D3, 0x394, buf.as_mut_ptr() as *mut u8);

        if (*runtime).game_end_phase != 0 {
            return false;
        }
        if (*runtime).replay_flag_a != 0 {
            return true;
        }
        if (*runtime)._field_460 != 0 {
            return false;
        }
        if (*(*runtime).world).fast_forward_request != 0 {
            return false;
        }
        if buf[1] != 0 {
            return false;
        }
        if buf[2] != 0 {
            return false;
        }
        true
    }
}

/// One row in the ESC-menu leaderboard: a team index plus its composite
/// score (`wins * 10000 + sum_of_alive_worm_HPs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaderboardEntry {
    /// Index into the GameInfo per-team array (0..16).
    pub team_idx: u8,
    /// Composite score: `wins * 10000 + sum_of_alive_worm_HPs`.
    pub score: i32,
}

/// Maximum number of entries in the ESC-menu leaderboard.
pub const LEADERBOARD_MAX: usize = 16;

// Per-team stride within GameInfo (0xBB8 = 3000 bytes).
const TEAM_STRIDE: usize = 0xBB8;
// Per-worm stride within a team's worm array (0x9C = 156 bytes).
const WORM_STRIDE: usize = 0x9C;

// GameInfo offsets used by the leaderboard sort (relative to team_off).
// `team_off = team_idx * TEAM_STRIDE`.
const OFF_TEAM_COUNT: usize = 0x44C; // u8, total team-slot count
const OFF_TEAM_SCORED: usize = 0x452; // u8 per team, == 0 means include
const OFF_TEAM_WINS: usize = 0x455; // u8 per team
const OFF_TEAM_WORMS: usize = 0x4188; // worm array base, stride 0x9C
const OFF_TEAM_WORM_GATE: usize = 0x4618; // i32 per team, == 0 means count HP
const OFF_TEAM_WORM_COUNT: usize = 0x4624; // i32 per team

/// Rust port of the `GameRuntime::OpenEscMenu` leaderboard-sort block
/// (0x53538D..0x5354A6 in the WA function body).
///
/// Walks GameInfo's per-team records, computes a composite score
/// `wins * 10000 + sum_of_alive_worm_HPs` for each team where
/// `gameinfo[+0x452 + team_off] == 0`, and returns the entries sorted
/// **descending** by score (winner first → top of menu). Worm HPs are
/// summed only when the team's per-team gate at `+0x4618 + team_off` is
/// zero.
///
/// Sort algorithm matches WA's: a quasi-selection-sort that walks each
/// position `i` from 0 and swaps with any `j > i` whose score is larger.
/// Stable for equal scores (only swaps on strict less-than).
///
/// Returns the populated entries plus the count (≤ 16).
pub unsafe fn sort_teams(
    game_info: *const GameInfo,
) -> ([LeaderboardEntry; LEADERBOARD_MAX], usize) {
    unsafe {
        let mut out = [LeaderboardEntry {
            team_idx: 0,
            score: 0,
        }; LEADERBOARD_MAX];
        let mut len: usize = 0;

        let base = game_info as *const u8;
        let team_count = *base.add(OFF_TEAM_COUNT) as usize;
        if team_count == 0 {
            return (out, 0);
        }

        for team_idx in 0..team_count {
            let team_off = team_idx * TEAM_STRIDE;
            // Skip teams whose +0x452 byte is non-zero (not scored).
            if *base.add(team_off + OFF_TEAM_SCORED) != 0 {
                continue;
            }

            // Sum live worm HPs only when the team-level gate is zero.
            let mut hp_sum: i32 = 0;
            let gate = *(base.add(team_off + OFF_TEAM_WORM_GATE) as *const i32);
            if gate == 0 {
                let worm_count = *(base.add(team_off + OFF_TEAM_WORM_COUNT) as *const i32);
                if worm_count > 0 {
                    let worms_base = base.add(team_off + OFF_TEAM_WORMS);
                    for w in 0..worm_count as usize {
                        let hp = *(worms_base.add(w * WORM_STRIDE) as *const i32);
                        hp_sum = hp_sum.wrapping_add(hp);
                    }
                }
            }

            let wins = *base.add(team_off + OFF_TEAM_WINS) as i32;
            let score = wins.wrapping_mul(10_000).wrapping_add(hp_sum);
            out[len] = LeaderboardEntry {
                team_idx: team_idx as u8,
                score,
            };
            len += 1;
            if len == LEADERBOARD_MAX {
                break;
            }
        }

        // WA's selection-sort: for each i, swap with any j > i whose score
        // is strictly larger. Result: descending order (winner first).
        if len >= 2 {
            for i in 0..len - 1 {
                for j in (i + 1)..len {
                    if out[i].score < out[j].score {
                        out.swap(i, j);
                    }
                }
            }
        }

        (out, len)
    }
}

/// Rust port of `GameRuntime::EscMenu_TickClosed` (0x005351B0).
///
/// Per-frame tick while the ESC menu is **closed**
/// (`runtime.esc_menu_state == 0`). Polls the keyboard for the
/// just-pressed edge of `KeyboardAction::Escape`:
///
/// - If Escape isn't pressed this frame → no-op.
/// - If Escape is pressed and [`is_hud_active`] returns `true` → call
///   [`bridge_open_esc_menu`], which builds the menu contents into
///   `runtime._field_30` and transitions `esc_menu_state` to `1`.
/// - If Escape is pressed but the HUD is *not* active (replay tail,
///   end-of-round, fast-forward, etc.) → reject with
///   [`KnownSoundId::WarningBeep`] at `runtime.ui_volume`.
pub unsafe fn tick_closed(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let keyboard = (*world).keyboard;
        if !KeyboardAction::Escape.is_active2(keyboard) {
            return;
        }
        if is_hud_active(runtime) {
            bridge_open_esc_menu(runtime);
        } else {
            dispatch_global_sound(
                runtime,
                KnownSoundId::WarningBeep.into(),
                8,
                Fixed::ONE,
                (*runtime).ui_volume,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a zeroed buffer big enough for `team_count` GameInfo team slots
    /// plus the highest offset the sort touches.
    fn synth_game_info(team_count: u8) -> Vec<u8> {
        // Largest offset accessed for team 15: 15*0xBB8 + 0x4188 + 15*0x9C + 4 ≈ 0xFF60.
        let mut buf = vec![0u8; 0x10_000];
        buf[OFF_TEAM_COUNT] = team_count;
        buf
    }

    fn set_u8(buf: &mut [u8], off: usize, v: u8) {
        buf[off] = v;
    }

    fn set_i32(buf: &mut [u8], off: usize, v: i32) {
        buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn set_team_record(
        buf: &mut [u8],
        team_idx: usize,
        scored_zero: bool,
        wins: u8,
        gate_zero: bool,
        worm_hps: &[i32],
    ) {
        let team_off = team_idx * TEAM_STRIDE;
        set_u8(
            buf,
            team_off + OFF_TEAM_SCORED,
            if scored_zero { 0 } else { 1 },
        );
        set_u8(buf, team_off + OFF_TEAM_WINS, wins);
        set_i32(
            buf,
            team_off + OFF_TEAM_WORM_GATE,
            if gate_zero { 0 } else { 1 },
        );
        set_i32(buf, team_off + OFF_TEAM_WORM_COUNT, worm_hps.len() as i32);
        for (i, &hp) in worm_hps.iter().enumerate() {
            set_i32(buf, team_off + OFF_TEAM_WORMS + i * WORM_STRIDE, hp);
        }
    }

    #[test]
    fn empty_team_count_returns_zero_entries() {
        let buf = synth_game_info(0);
        unsafe {
            let (_, len) = sort_teams(buf.as_ptr() as *const GameInfo);
            assert_eq!(len, 0);
        }
    }

    #[test]
    fn skips_teams_with_nonzero_scored_byte() {
        let mut buf = synth_game_info(3);
        set_team_record(&mut buf, 0, false, 5, true, &[100, 100]); // skipped
        set_team_record(&mut buf, 1, true, 2, true, &[50]);
        set_team_record(&mut buf, 2, true, 0, true, &[80, 80]);
        unsafe {
            let (entries, len) = sort_teams(buf.as_ptr() as *const GameInfo);
            assert_eq!(len, 2);
            assert_eq!(entries[0].team_idx, 1); // wins*10000 + 50 = 20050
            assert_eq!(entries[0].score, 20_050);
            assert_eq!(entries[1].team_idx, 2); // 0*10000 + 160 = 160
            assert_eq!(entries[1].score, 160);
        }
    }

    #[test]
    fn descending_by_composite_score() {
        let mut buf = synth_game_info(4);
        // wins dominates ties: 3 > 2 > 1 > 0
        set_team_record(&mut buf, 0, true, 1, true, &[42]); // 10042
        set_team_record(&mut buf, 1, true, 3, true, &[7]); // 30007
        set_team_record(&mut buf, 2, true, 2, true, &[300]); // 20300
        set_team_record(&mut buf, 3, true, 0, true, &[9999]); // 9999
        unsafe {
            let (entries, len) = sort_teams(buf.as_ptr() as *const GameInfo);
            assert_eq!(len, 4);
            assert_eq!(entries[0].team_idx, 1);
            assert_eq!(entries[0].score, 30_007);
            assert_eq!(entries[1].team_idx, 2);
            assert_eq!(entries[1].score, 20_300);
            assert_eq!(entries[2].team_idx, 0);
            assert_eq!(entries[2].score, 10_042);
            assert_eq!(entries[3].team_idx, 3);
            assert_eq!(entries[3].score, 9_999);
        }
    }

    #[test]
    fn worm_gate_nonzero_zeros_hp_contribution() {
        let mut buf = synth_game_info(2);
        // Team 0: gate non-zero → HP ignored, score = wins*10000
        set_team_record(&mut buf, 0, true, 5, false, &[1000, 1000, 1000]);
        // Team 1: gate zero → HP counts, score = 4*10000 + 50
        set_team_record(&mut buf, 1, true, 4, true, &[50]);
        unsafe {
            let (entries, len) = sort_teams(buf.as_ptr() as *const GameInfo);
            assert_eq!(len, 2);
            assert_eq!(entries[0].team_idx, 0);
            assert_eq!(entries[0].score, 50_000);
            assert_eq!(entries[1].team_idx, 1);
            assert_eq!(entries[1].score, 40_050);
        }
    }

    #[test]
    fn equal_scores_preserve_input_order() {
        let mut buf = synth_game_info(3);
        // All three teams produce score = 100. Original order: 0, 1, 2.
        set_team_record(&mut buf, 0, true, 0, true, &[100]);
        set_team_record(&mut buf, 1, true, 0, true, &[100]);
        set_team_record(&mut buf, 2, true, 0, true, &[100]);
        unsafe {
            let (entries, len) = sort_teams(buf.as_ptr() as *const GameInfo);
            assert_eq!(len, 3);
            assert_eq!(entries[0].team_idx, 0);
            assert_eq!(entries[1].team_idx, 1);
            assert_eq!(entries[2].team_idx, 2);
        }
    }
}
