//! Rust port of `GameInfo__InitSession` (0x004608E0) and two of its helpers:
//! `Replay__ProcessSchemeDefaults` (0x004670F0) and
//! `Replay__ProcessReplayFlags` (0x00467280).
//!
//! Called by every "Start match" dialog handler (e.g.
//! `FrontendLocalMP__OnStartMatch` at 0x004A1260) right before
//! `Frontend__LaunchGameSession`. Commits the lobby / scheme pending-state
//! globals into `GameInfo`.
//!
//! ## Prefix-pointer convention
//!
//! WA's `InitSession` and several of its helpers take a *prefix pointer*
//! `prefix_ptr = G_GAME_INFO - 0x40`, not a `GameInfo*` directly. There is a
//! 0x40-byte header preceding `GameInfo` in the BSS layout (currently
//! unmapped in Rust); the function uses offsets relative to that header, so
//! e.g. `prefix_ptr + 0xD7B8` writes `GameInfo + 0xD778` (`game_version`).
//!
//! The three Rust entry points below take `*mut GameInfo` and compute
//! `prefix_ptr` internally for symmetry with the WA originals.
//!
//! ## What is *not* ported yet
//!
//! `InitTeamsFromLobby` (ex-`ProcessTeamColors`), `CreateWAGameReplay`,
//! `ConvertScheme`, `ValidateTeamSetup` are still bridged to the WA
//! originals. The orchestrator stays byte-identical end-to-end as long
//! as those bridges observe / write the same globals the WA function
//! would.

use core::ffi::{CStr, c_char};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::address::va;
use crate::engine::GameInfo;
use crate::engine::config_load::load_options;
use crate::generated::wa_calls;
use crate::rebase::rb;

/// Set by [`init_session_shim`] each time the Rust orchestrator runs. The
/// snapshot machinery reads this to tag dumps as `_rust` vs `_wa`. We only
/// flip it true (never false) because the snapshot dump fires *after* the
/// shim returns; if some other code path later calls `init_session` directly
/// via [`crate::engine::config_load::init_session`] it'll also flip true,
/// which is the correct answer.
pub static RUST_INIT_SESSION_RAN: AtomicBool = AtomicBool::new(false);

/// Pre-header byte offset preceding `GameInfo` in BSS. WA's InitSession
/// takes this offset pointer because the per-prefix arithmetic includes
/// fields in that 0x40-byte header (currently unmapped in Rust).
const PREFIX_HEADER_SIZE: usize = 0x40;

fn prefix_ptr(gi: *mut GameInfo) -> *mut u8 {
    (gi as *mut u8).wrapping_sub(PREFIX_HEADER_SIZE)
}

// ─── WA-CRT bridges ────────────────────────────────────────────────────────
// `srand`/`rand` need to be the WA-loaded CRT copies (not Rust's libc) because
// the WA build links its own MSVCR80 instance — calling Rust's libc would
// advance a different RNG state than every other WA path expects.

unsafe fn wa_srand(seed: u32) {
    unsafe {
        let f: unsafe extern "cdecl" fn(u32) = core::mem::transmute(rb(va::WA_SRAND));
        f(seed);
    }
}

unsafe fn wa_rand() -> i32 {
    unsafe {
        let f: unsafe extern "cdecl" fn() -> i32 = core::mem::transmute(rb(va::WA_RAND));
        f()
    }
}

unsafe fn wa_time64() -> i64 {
    unsafe {
        let f: unsafe extern "cdecl" fn(*mut i64) -> i64 = core::mem::transmute(rb(va::WA_TIME64));
        f(core::ptr::null_mut())
    }
}

// ─── Bridges to unported WA helpers ────────────────────────────────────────

unsafe fn wa_create_wa_game_replay(prefix: *mut u8, label: *const c_char, time_val: i64) {
    unsafe {
        let f: unsafe extern "stdcall" fn(*mut u8, *const c_char, u32, u32) =
            core::mem::transmute(rb(va::CGAMEINFO_CREATE_WA_GAME_REPLAY));
        f(prefix, label, time_val as u32, (time_val >> 32) as u32);
    }
}

// ─── Helpers for prefix-relative byte access ───────────────────────────────

#[inline(always)]
unsafe fn read_u8(prefix: *mut u8, off: usize) -> u8 {
    unsafe { *prefix.add(off) }
}

#[inline(always)]
unsafe fn read_i32(prefix: *mut u8, off: usize) -> i32 {
    unsafe { *(prefix.add(off) as *const i32) }
}

#[inline(always)]
unsafe fn read_u32(prefix: *mut u8, off: usize) -> u32 {
    unsafe { *(prefix.add(off) as *const u32) }
}

#[inline(always)]
unsafe fn write_u8(prefix: *mut u8, off: usize, val: u8) {
    unsafe { *prefix.add(off) = val }
}

#[inline(always)]
unsafe fn write_u32(prefix: *mut u8, off: usize, val: u32) {
    unsafe { *(prefix.add(off) as *mut u32) = val }
}

/// Re-roll the session RNG: read current state, advance it via
/// `srand(prev); rand() << 16 + rand()`, write new state back, and save the
/// old state as the "previous seed" slot. Mirrors the inline asm/decompile in
/// `InitSession` + `ProcessSchemeDefaults` + the post-replay-loader epilogue.
///
/// Note: `srand`/`rand` here are the WA-CRT copies (see [`wa_srand`]).
unsafe fn advance_rng(prefix: *mut u8, current_off: usize, prev_off: Option<usize>) -> u32 {
    unsafe {
        let prev = read_u32(prefix, current_off);
        wa_srand(prev);
        let r1 = wa_rand() as u32;
        let r2 = wa_rand() as u32;
        // WA combines as `r2 + (r1 << 16)` (decompile: `iVar3 + iVar2 * 0x10000`,
        // where iVar3 = first rand, iVar2 = second). Using wrapping_add since
        // rand() can return 0x7FFF and `(r1 << 16) | (r2 & 0xFFFF)` is the
        // intent, but matching WA's exact arithmetic to stay byte-equivalent.
        let new = r2.wrapping_add(r1.wrapping_shl(16));
        write_u32(prefix, current_off, new);
        if let Some(off) = prev_off {
            write_u32(prefix, off, prev);
        }
        prev
    }
}

// ─── Public entry: GameInfo__InitSession (0x004608E0) ──────────────────────

/// Commit the current lobby / scheme globals into `GameInfo`.
///
/// `type_label` controls whether a `.WAgame` recording file is created:
/// pass `Some("Offline")` / `"Online"` etc. for a fresh match-start; pass
/// `None` for a refresh-only path (e.g. the snapshot-replay re-launch in
/// `openwa-frontend`) where the prior replay file is still open and a
/// second `CreateWAGameReplay` call would crash.
pub unsafe fn init_session(gi: *mut GameInfo, type_label: Option<&CStr>) {
    use crate::engine::launch_source::{LaunchSource, current as launch_source_current};
    RUST_INIT_SESSION_RAN.store(true, Ordering::Relaxed);
    let source = launch_source_current();
    let _ = openwa_core::log::log_line(&format!("[init_session] source={source:?}"));
    unsafe {
        let prefix = prefix_ptr(gi);

        // ── Bookkeeping writes on the prefix-relative offsets ────────────────
        // game_version (GameInfo+0xD778): reset version 14 → 0. Likely a
        // legacy/forward-version guard so InitSession doesn't carry a 14
        // value through from a previous match (14 is the "in-game" marker
        // set elsewhere, not a valid `GameInfo.game_version` value at
        // session boot).
        let game_version_off = 0xD7B8; // GameInfo + 0xD778
        if read_i32(prefix, game_version_off) == 14 {
            write_u32(prefix, game_version_off, 0);
        }

        // GameInfo - 8 = 0 — pre-header dword, role TBD.
        write_u32(prefix, 0x38, 0);

        // Copy 402 bytes from G_SCHEME_DATA → G_SCHEME_DATA + 402. This
        // backs up the live scheme into the second half of the same
        // global buffer before subsequent helpers mutate it.
        core::ptr::copy_nonoverlapping(
            rb(va::G_SCHEME_DATA) as *const u8,
            (rb(va::G_SCHEME_DATA) as *mut u8).add(402),
            402,
        );

        // Two unidentified bookkeeping fields cleared / set to 1.
        write_u32(prefix, 0xDA14, 1); // GameInfo + 0xD9D4 = 1
        write_u8(prefix, 0xDB4C, 0); // GameInfo + 0xDB0C = 0

        // memset GameInfo[0xF91C..0xFC7C] = 0 (0x360 bytes).
        core::ptr::write_bytes(prefix.add(0xF95C), 0, 0x360);

        // ── Helper chain ─────────────────────────────────────────────────────
        // The four WA-bridged helpers below all read from globals the MFC
        // LobbyDialog populates (`&DAT_008779e4` per-player config table,
        // `G_SCHEME_DATA` selected-scheme buffer, etc.). On the Frontend
        // path those globals are fresh from the user's just-completed
        // lobby flow. On the CustomLauncher path they're stale from a
        // prior launch (or have never been populated) — running the
        // helpers would clobber the snapshot-restored team/scheme state
        // the launcher set up. Until the helpers are ported (or the
        // launcher learns to repopulate the globals), skip them.
        //
        // The two Rust-native helpers (`process_scheme_defaults`,
        // `process_replay_flags`) read from GameInfo, which is valid in
        // both modes, so they always run.
        let is_frontend = source == LaunchSource::Frontend;

        // On the CustomLauncher path, synthesise the MFC-lobby globals
        // (`G_PLAYER_ARRAY`, `G_TEAM_DATA`) from the pending match before
        // running `InitTeamsFromLobby` — that helper's outputs
        // (team-record identity, alliance bookkeeping at +0xD0BC, etc.)
        // are what the game team_records need to be valid downstream.
        // `apply()` still runs first as a defensive baseline (it also
        // copies the scheme into G_SCHEME_DATA which the lobby commit
        // doesn't touch); the commit's writes overwrite the team-record
        // bytes that apply() seeded.
        if !is_frontend && let Some(pending) = crate::engine::pending_match::take() {
            crate::engine::pending_match::apply(gi, &pending);
            crate::engine::pending_match::populate_lobby_globals(&pending);
            // Stage map seed + terrain coverage and run
            // `CMapEditor::GenerateRandomLevel`, which writes
            // `data\land.dat`. WA's frontend path defers this materializer
            // to `CPleaseWait::OnTimer` (via `MapView::CopyInfo`), which
            // our CustomLauncher path doesn't run — without this call,
            // the engine reads whatever `land.dat` was left over from a
            // prior session and the user's seed selection has no effect.
            crate::engine::pending_match::apply_map_globals(&pending);
        }

        wa_calls::GameInfo::InitTeamsFromLobby(prefix);

        if is_frontend && let Some(label) = type_label {
            let t = wa_time64();
            wa_create_wa_game_replay(prefix, label.as_ptr(), t);
        }

        process_scheme_defaults(gi);

        // ConvertScheme runs on both paths. On the Frontend path its
        // input (G_SCHEME_DATA) is whatever the lobby populated; on the
        // CustomLauncher path `pending_match::apply` just wrote the
        // user-supplied scheme into G_SCHEME_DATA above. Its only other
        // inputs are GameInfo fields we control (num_teams, game_version,
        // replay_active, replay_field_db58). Without it the
        // scheme-derived bytes (game_speed_config at +0xD988,
        // mine_list_capacity, object_slot_count, the 0xD78C..0xD924
        // per-weapon overlay, etc.) stay at zero and the dispatch loop
        // divides by `game_speed_target = 0`.
        wa_calls::CGameInfo::ConvertScheme(prefix);

        wa_calls::Replay::ValidateTeamSetup(prefix);

        process_replay_flags(gi);

        // ── Re-roll session RNG ──────────────────────────────────────────────
        // Save the current rng at +0xFC7C as the "previous seed" at +0xD774,
        // and write a freshly-rolled value back to +0xFC7C. This matches
        // WA's "every session bumps the RNG" idiom.
        advance_rng(prefix, 0xFCBC, Some(0xD7B4));

        // ── Registry-sourced options ─────────────────────────────────────────
        // After the 2026-05-13 cluster refactor, `load_options` takes the
        // inner `G_GAME_INFO` pointer (matching `gi` here); the WA-side hook
        // shim adjusts `prefix_ptr + 0x40` before calling. Earlier code had
        // to pass `prefix as *mut GameInfo` (a band-aid) because the struct
        // had prefix-coord field offsets in the upper region — that's all
        // unwound now.
        load_options(gi);

        // ── Late CustomLauncher fixup ─────────────────────────────────────────
        // `ConvertScheme` (0x0045D640) at +0x2A75 writes
        // `*[0x0088DC44]` (G_SCHEME_DATA + 0x164, a V3-extended-options
        // byte) into `GameInfo+0xD988` (`game_speed_config`). For V2
        // schemes that source byte is zero-padded, so `game_speed_config`
        // ends up 0 — and `init_game_state_tracking_arrays` reads it raw
        // into `world.game_speed_target`, then `dispatch_frame` divides
        // by it. Identified via the +0xD988 hardware watchpoint.
        //
        // Restore the Frontend-baseline default (Fixed 1.0). Gated on
        // CustomLauncher mode + currently-zero so the WA Frontend path is
        // untouched.
        if source == LaunchSource::CustomLauncher && (*gi).game_speed_config == 0 {
            (*gi).game_speed_config = 0x00010000;
        }
    }
}

// ─── Hook-trampoline cdecl shims ───────────────────────────────────────────
//
// The codegen-emitted trampoline forwards the captured register value to a
// cdecl function. These shims re-derive `*mut GameInfo` from `prefix_ptr`
// and dispatch into the regular Rust impls. Public so the DLL hook layer
// can name them.

/// Cdecl shim invoked by the InitSession hook trampoline (stdcall, but the
/// shim is plain cdecl since the trampoline's job is just to forward args).
/// `type_label` is the raw stack-arg pointer; we re-CStr it here.
pub unsafe extern "stdcall" fn init_session_shim(prefix_ptr: u32, type_label: *const c_char) {
    unsafe {
        let gi = (prefix_ptr as usize + PREFIX_HEADER_SIZE) as *mut GameInfo;
        let label = if type_label.is_null() {
            None
        } else {
            Some(CStr::from_ptr(type_label))
        };
        init_session(gi, label);
    }
}

/// Cdecl shim invoked by the ProcessSchemeDefaults usercall trampoline.
/// `prefix_ptr` is the captured ESI value.
pub unsafe extern "cdecl" fn process_scheme_defaults_shim(prefix_ptr: u32) {
    unsafe {
        let gi = (prefix_ptr as usize + PREFIX_HEADER_SIZE) as *mut GameInfo;
        process_scheme_defaults(gi);
    }
}

/// Cdecl shim invoked by the ProcessReplayFlags usercall trampoline.
/// `prefix_ptr` is the captured EAX value.
pub unsafe extern "cdecl" fn process_replay_flags_shim(prefix_ptr: u32) {
    unsafe {
        let gi = (prefix_ptr as usize + PREFIX_HEADER_SIZE) as *mut GameInfo;
        process_replay_flags(gi);
    }
}

// ─── Replay__ProcessSchemeDefaults (0x004670F0, usercall ESI=prefix) ───────

/// Pick a random sub-scheme variant per "scheme group" by counting how many
/// teams belong to each group (`team_records[i].turn_order_idx == group`)
/// and rolling `rng % count` to select one.
///
/// Reads: `+0xD0FC` (group count, u8), `+0xFCBC` (rng state), `+0x48C`
/// (`team_record_count`), per-team `+0x453 + i*0xBB8` (`turn_order_idx`).
/// Writes: `+0xD7B2` (chosen-variant byte for group 0, derived from initial
/// rng), `+0xFCBC` (rng advance), and `+0xD0FE + group*0x11E` (per-group
/// chosen variant).
pub unsafe fn process_scheme_defaults(gi: *mut GameInfo) {
    unsafe {
        let prefix = prefix_ptr(gi);

        let group_count = read_u8(prefix, 0xD0FC);
        if group_count == 0 {
            // Guard the modulo below; the orchestrator presumably never calls
            // this with group_count == 0 in practice, but stay defensive.
            return;
        }

        // Initial roll: write `prev_rng % group_count` into +0xD7B2 (byte).
        // Note: WA stores this as a single byte even though the modulo could
        // theoretically exceed 255 — group_count is also a u8 so it can't.
        let prev = advance_rng(prefix, 0xFCBC, None);
        write_u8(prefix, 0xD7B2, (prev % group_count as u32) as u8);

        // Per-group variant selection. The output array is bytes at
        // +0xD0BE stride 0x11E (one element per group).
        for group in 0..group_count as i32 {
            let team_record_count = read_u8(prefix, 0x48C);
            if team_record_count == 0 {
                // Nothing to count; leave the slot at its memset-cleared 0.
                continue;
            }

            let mut matching_teams: u32 = 0;
            for team in 0..team_record_count as usize {
                // turn_order_idx at team_records[team] + 3:
                // prefix + 0x493 + team * 0xBB8 = GameInfo + 0x453 + team * 0xBB8.
                let turn_order = read_u8(prefix, 0x493 + team * 0xBB8);
                if turn_order as i32 == group {
                    matching_teams += 1;
                }
            }

            if matching_teams != 0 {
                let prev = advance_rng(prefix, 0xFCBC, None);
                let out_off = 0xD0FE + (group as usize) * 0x11E;
                write_u8(prefix, out_off, (prev % matching_teams) as u8);
            }
        }
    }
}

// ─── Replay__ProcessReplayFlags (0x00467280, usercall EAX=prefix) ──────────

/// Set three replay-feature flag bytes (`+0x489..+0x48B`) based on a tally
/// across the per-team `team_input_configs` records, plus version /
/// in-game-flag gates. Pure logic, no calls into WA.
///
/// The 269-instruction body is three parallel "tally how many teams fall
/// above/below threshold T, set flag = majority" passes plus a couple of
/// gate clauses; mechanically translated from Ghidra without simplification.
///
/// Reads: `+0xD7B8` (game_version), `+0x40` (num_teams, u8), `+0x88/+0x8C`
/// (team_input_configs stride 0x50 — two i32 fields per record), `+0xDB48`
/// (byte flag), `+0xDB58` (i32), `+0xDA1C/+0xDA1D` (signed bytes),
/// `+0xDA14` (i32 == 1 gate), `+0xD98B/+0xD98F` (byte gates),
/// `+0xD1FC..+0xD1FC + 6 * 0x11E` (u16 array, stride 0x11E).
/// Writes: `+0x489`, `+0x48A`, `+0x48B` (flag bytes), `+0xD1FC..` (u16 99 sentinel).
pub unsafe fn process_replay_flags(gi: *mut GameInfo) {
    unsafe {
        let prefix = prefix_ptr(gi);

        let game_version = read_i32(prefix, 0xD7B8);
        let num_teams = read_u8(prefix, 0x40);
        let flag_db48 = read_u8(prefix, 0xDB48);
        let int_db58 = read_i32(prefix, 0xDB58);
        let signed_da1d = read_u8(prefix, 0xDA1D) as i8;
        let int_da14 = read_i32(prefix, 0xDA14);
        let byte_d98b = read_u8(prefix, 0xD98B);
        let byte_d98f = read_u8(prefix, 0xD98F);

        // ── First pass: u16-99-sentinel write at +0xD1FC (stride 0x11E) ────────
        //
        // Decision flag `b3`:
        //   if game_version < -2:
        //     if num_teams == 0: b3 = (flag_db48 != 0) && (int_db58 > 0x3EA)
        //     else: tally over team_input_configs:
        //       below_threshold = (cfg[+0x4C] < -3 && cfg[+0x48] < -2)
        //       above_threshold = (cfg[+0x4C] < 0x3EB && !below_threshold)
        //       hits_count = team_input_configs with cfg[+0x4C] >= 0x3EB
        //       … gate further by per-team_input_configs[+0xDA1D] entry …
        //   else if int_da14 != 1: b3 = (flag_db48 != 0) && ((int_db58 - 0x3EB) <= 0x47)
        let mut b3 = true;
        let mut skip_99_write = false;

        if game_version < -2 {
            if num_teams == 0 {
                if flag_db48 == 0 || int_db58 <= 0x3EA {
                    skip_99_write = true;
                }
            } else {
                // Tally
                let cfg0_p4c = read_i32(prefix, 0x8C);
                let majority_above: bool = if cfg0_p4c < 0x3EB {
                    !(cfg0_p4c < 0 && read_i32(prefix, 0x88) < -2)
                } else {
                    let mut below: u32 = 0;
                    let mut above: u32 = 0;
                    for team in 0..num_teams as usize {
                        let base = team * 0x50;
                        let p4c = read_i32(prefix, 0x8C + base);
                        if p4c < 0x3EB {
                            if p4c < 0 && read_i32(prefix, 0x88 + base) < -2 {
                                below += 1;
                            } else {
                                above += 1;
                            }
                        }
                    }
                    below < above
                };

                // Per-team override gate: if flag_db48 != 0 and signed_da1d >= 0,
                // re-check the specific team's record. If it lands in the
                // "below" region, force skip_99_write; otherwise jump past the
                // remaining flag pass and proceed straight to flag-byte updates.
                let mut goto_post_first_pass = false;
                if flag_db48 != 0 && signed_da1d >= 0 {
                    let team_off = (signed_da1d as usize) * 0x50;
                    let p4c = read_i32(prefix, 0x8C + team_off);
                    if p4c < 0x3EB {
                        if p4c < 0 && read_i32(prefix, 0x88 + team_off) < -2 {
                            skip_99_write = true;
                        } else {
                            goto_post_first_pass = true;
                        }
                    }
                }

                if !goto_post_first_pass && !skip_99_write && !majority_above {
                    skip_99_write = true;
                }
            }
        } else if int_da14 != 1 {
            b3 = flag_db48 != 0 && (int_db58 - 0x3EB) as u32 <= 0x47;
        }

        if !skip_99_write {
            if byte_d98b == 1 && b3 {
                // Write 99 into 6 entries at +0xD1FC stride 0x11E.
                let mut p = prefix.add(0xD1FC) as *mut u16;
                for _ in 0..6 {
                    *p = 99;
                    p = p.add(0x11E / 2);
                }
            } else if num_teams != 0 || int_da14 != 1 {
                // Replace any lingering 99-sentinel in the array with 0.
                let mut p = prefix.add(0xD1FC) as *mut i16;
                for _ in 0..6 {
                    if *p == 99 {
                        *p = 0;
                    }
                    p = p.add(0x11E / 2);
                }
            }
        }

        // ── Second pass: flag at +0x489 ──────────────────────────────────────
        // Threshold here is 0x2E (vs 0x3EA in the first pass). Similar
        // majority-counting structure.
        if game_version < 0x4E {
            if num_teams == 0 {
                let val = flag_db48 != 0 && int_db58 > 0x2E;
                write_u8(prefix, 0x489, val as u8);
            } else {
                let cfg0_p4c = read_i32(prefix, 0x8C);
                if cfg0_p4c < 0x3EB {
                    write_u8(prefix, 0x489, (cfg0_p4c > 0x2E) as u8);
                } else {
                    let mut below: u32 = 0;
                    let mut above: u32 = 0;
                    for team in 0..num_teams as usize {
                        let p4c = read_i32(prefix, 0x8C + team * 0x50);
                        if p4c < 0x3EB {
                            if p4c < 0x2F {
                                below += 1;
                            } else {
                                above += 1;
                            }
                        }
                    }
                    write_u8(prefix, 0x489, (below <= above) as u8);
                }

                if flag_db48 != 0 && signed_da1d >= 0 {
                    let team_off = (signed_da1d as usize) * 0x50;
                    let p4c = read_i32(prefix, 0x8C + team_off);
                    if p4c < 0x3EB {
                        write_u8(prefix, 0x489, (p4c > 0x2E) as u8);
                    }
                }
            }
        } else {
            write_u8(prefix, 0x489, 1);
        }

        // ── Third write: +0x48A is `+0x489 && !+0xD98F` ──────────────────────
        let v489 = read_u8(prefix, 0x489);
        write_u8(prefix, 0x48A, (v489 != 0 && byte_d98f == 0) as u8);

        // ── Fourth pass: flag at +0x48B ──────────────────────────────────────
        // Gated by `(game_version - 0x12D) < 7`. Same shape: count teams
        // whose cfg[+0x4C] == 0x1C2 and cfg[+0x48] in [0x12D, 0x133].
        let gate = (game_version - 0x12D) as u32;
        if gate < 7 {
            if num_teams == 0 {
                write_u8(prefix, 0x48B, (int_db58 == 0x1C2) as u8);
                return;
            }
            let cfg0_p4c = read_i32(prefix, 0x8C);
            let majority_match: bool = if cfg0_p4c < 0x402 {
                cfg0_p4c == 0x1C2 && ((read_i32(prefix, 0x88) - 0x12D) as u32) < 7
            } else {
                let mut other: u32 = 0;
                let mut matches: u32 = 0;
                for team in 0..num_teams as usize {
                    let base = team * 0x50;
                    let p4c = read_i32(prefix, 0x8C + base);
                    if p4c < 0x402 {
                        if p4c == 0x1C2 && ((read_i32(prefix, 0x88 + base) - 0x12D) as u32) < 7 {
                            matches += 1;
                        } else {
                            other += 1;
                        }
                    }
                }
                other < matches
            };
            write_u8(prefix, 0x48B, majority_match as u8);

            if flag_db48 != 0 && signed_da1d >= 0 {
                let team_off = (signed_da1d as usize) * 0x50;
                let p4c = read_i32(prefix, 0x8C + team_off);
                if p4c < 0x402 {
                    let team_match =
                        p4c == 0x1C2 && ((read_i32(prefix, 0x88 + team_off) - 0x12D) as u32) < 7;
                    write_u8(prefix, 0x48B, team_match as u8);
                }
            }
        } else {
            write_u8(prefix, 0x48B, 0);
        }
    }
}
