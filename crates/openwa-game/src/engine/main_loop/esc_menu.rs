//! In-game ESC menu state machine.
//!
//! The ESC menu is the in-round overlay shown by pressing Escape — a
//! scoreboard ("First Team to N Wins" header + per-team leaderboard) plus
//! action buttons (Minimize Game, Force Sudden Death, Draw This Round, Quit
//! The Game) and a volume slider. Lives at `runtime.menu_panel_a` (the
//! [`MenuPanel`] item list) with the canvas at `runtime.display_gfx_d` (a
//! [`DisplayBitGrid`]).
//!
//! State at `runtime.esc_menu_state` (i32):
//!  - **0** — closed. [`tick_closed`] polls for Escape to open.
//!  - **1** — open / accepting nav input. Driven WA-side by
//!    `EscMenu_TickState1` (still bridged via [`bridge_state_1_tick`]).
//!  - **2** — confirm / network-end-of-game flow. Driven WA-side by
//!    `EscMenu_TickState2` (still bridged via [`bridge_state_2_tick`]).
//!
//! [`MenuPanel`]: crate::engine::menu_panel::MenuPanel
//! [`DisplayBitGrid`]: crate::bitgrid::DisplayBitGrid

use core::ffi::c_char;

use openwa_core::fixed::Fixed;

use crate::address::va;
use crate::audio::known_sound_id::KnownSoundId;
use crate::audio::sound_ops::dispatch_global_sound;
use crate::bitgrid::{BitGridDisplayVtable, DisplayBitGrid};
use crate::engine::menu_panel::{MenuPanel, append_item_impl};
use crate::engine::runtime::GameRuntime;
use crate::engine::team_arena::TeamArena;
use crate::engine::world::GameWorld;
use crate::input::keyboard::KeyboardAction;
use crate::rebase::rb;
use crate::render::display::font::TextMeasurement;
use crate::render::display::gfx::DisplayGfx;
use crate::render::display::vtable::{draw_text_on_bitmap, measure_text};
use crate::wa::localized_template::LocalizedTemplate;
use crate::wa::string_resource::{StringRes, res};

// ─── Bridged WA addresses ──────────────────────────────────────────────────

static mut STATE_1_TICK_ADDR: u32 = 0;
static mut STATE_2_TICK_ADDR: u32 = 0;
static mut STRING_TOKEN_LOOKUP_ADDR: u32 = 0;
static mut SPRINTF_ROTATING_ADDR: u32 = 0;

// String token table lookup — `FUN_0053EA30(table, token) -> *const c_char`,
// `__stdcall`, RET 8. Resolves a localized template string from the
// gfx-dir's string table (with WA's own escape-code post-processing).
const STRING_TOKEN_LOOKUP_VA: u32 = 0x0053EA30;
// Rotating-buffer sprintf — `FUN_005978A0(format, ...) -> *const c_char`,
// `__cdecl`, varargs (caller cleans). Writes to one of 8 16-KiB rotating
// scratch buffers and returns a pointer to it. WA only ever calls this
// with up to 3 varargs in the ESC-menu path.
const SPRINTF_ROTATING_VA: u32 = 0x005978A0;

/// Initialize the ESC-menu bridge addresses. Called from
/// `dispatch_frame::init_dispatch_addrs` at DLL load.
pub unsafe fn init_addrs() {
    unsafe {
        STATE_1_TICK_ADDR = rb(va::GAME_RUNTIME_ESC_MENU_STATE_1_TICK);
        STATE_2_TICK_ADDR = rb(va::GAME_RUNTIME_ESC_MENU_STATE_2_TICK);
        STRING_TOKEN_LOOKUP_ADDR = rb(STRING_TOKEN_LOOKUP_VA);
        SPRINTF_ROTATING_ADDR = rb(SPRINTF_ROTATING_VA);
    }
}

// ─── Bridges (still WA-side) ───────────────────────────────────────────────

/// Bridge for `GameRuntime::EscMenu_TickState1` (0x00535B10) — per-frame
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

/// Bridge for `GameRuntime::EscMenu_TickState2` (0x00535FC0) — per-frame
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

/// Bridge for `LocalizedTemplate__Resolve` (FUN_0053EA30, stdcall RET 8).
/// Returns a pointer to the resolved template string (with WA's escape
/// codes processed and the result cached on the [`LocalizedTemplate`])
/// for the given token id.
unsafe fn bridge_token_lookup(template: *mut LocalizedTemplate, token: StringRes) -> *const c_char {
    unsafe {
        let func: unsafe extern "stdcall" fn(*mut LocalizedTemplate, u32) -> *const c_char =
            core::mem::transmute(STRING_TOKEN_LOOKUP_ADDR as usize);
        func(template, token.as_offset())
    }
}

/// Bridge for `FUN_005978A0` — sprintf into one of 8 16-KiB rotating
/// scratch buffers. The OpenEscMenu path always passes 3 varargs (the
/// "First Team to %d Wins" template ignores the first two but WA pushes
/// them anyway).
unsafe fn bridge_sprintf_rotating_3(
    format: *const c_char,
    a1: u32,
    a2: u32,
    a3: u32,
) -> *const c_char {
    unsafe {
        let func: unsafe extern "cdecl" fn(*const c_char, u32, u32, u32) -> *const c_char =
            core::mem::transmute(SPRINTF_ROTATING_ADDR as usize);
        func(format, a1, a2, a3)
    }
}

// ─── Inline-ported clipping helpers ────────────────────────────────────────
//
// `FUN_004F66E0` and `FUN_004F67F0` are short clip-and-call wrappers on
// top of the BitGridDisplay vtable's slot 0 / slot 1. The other two tail
// patterns (slot 2 fill_vline, slot 5 put_pixel_clipped) aren't extracted
// in the WA binary but use the same shape inline. All four are inlined
// here as plain Rust to avoid a usercall trampoline per call.
//
// The clip-rect on a `DisplayBitGrid` lives at fields +0x1C/+0x20/+0x24/+0x28
// (`clip_left`/`clip_top`/`clip_right`/`clip_bottom`).

/// Rust port of `FUN_004F66E0` — clipped fill_rect on a `DisplayBitGrid`.
unsafe fn clipped_fill_rect(
    bg: *mut DisplayBitGrid,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    color: u8,
) {
    unsafe {
        if x1 >= x2 || y1 >= y2 {
            return;
        }
        let cl = (*bg).clip_left as i32;
        let ct = (*bg).clip_top as i32;
        let cr = (*bg).clip_right as i32;
        let cb = (*bg).clip_bottom as i32;
        if x1 >= cr || y1 >= cb || x2 <= cl || y2 <= ct {
            return;
        }
        let x1 = x1.max(cl);
        let y1 = y1.max(ct);
        let x2 = x2.min(cr);
        let y2 = y2.min(cb);
        DisplayBitGrid::fill_rect_raw(bg, x1, y1, x2, y2, color);
    }
}

/// Rust port of `FUN_004F67F0` — clipped fill_hline on a `DisplayBitGrid`.
unsafe fn clipped_fill_hline(bg: *mut DisplayBitGrid, x1: i32, x2: i32, y: i32, color: u8) {
    unsafe {
        if x1 >= x2 {
            return;
        }
        let cl = (*bg).clip_left as i32;
        let ct = (*bg).clip_top as i32;
        let cr = (*bg).clip_right as i32;
        let cb = (*bg).clip_bottom as i32;
        if y < ct || y >= cb || x1 >= cr || x2 <= cl {
            return;
        }
        let x1 = x1.max(cl);
        let x2 = x2.min(cr);
        DisplayBitGrid::fill_hline_raw(bg, x1, x2, y, color);
    }
}

/// Inline-replicates the slot-2 (`fill_vline`) clip-and-call pattern from
/// the `OpenEscMenu` border-drawing tail block. Mirrors `clipped_fill_hline`
/// but with x/y swapped.
unsafe fn clipped_fill_vline(bg: *mut DisplayBitGrid, x: i32, y1: i32, y2: i32, color: u8) {
    unsafe {
        if y1 >= y2 {
            return;
        }
        let cl = (*bg).clip_left as i32;
        let ct = (*bg).clip_top as i32;
        let cr = (*bg).clip_right as i32;
        let cb = (*bg).clip_bottom as i32;
        if x < cl || x >= cr || y1 >= cb || y2 <= ct {
            return;
        }
        let y1 = y1.max(ct);
        let y2 = y2.min(cb);
        DisplayBitGrid::fill_vline_raw(bg, x, y1, y2, color);
    }
}

/// Slot-5 (`put_pixel_clipped`) wrapper — slot 5 already does the clip
/// internally; this is just a typed dispatch.
unsafe fn put_pixel_clipped(bg: *mut DisplayBitGrid, x: i32, y: i32, color: u8) {
    unsafe {
        let vt: *const BitGridDisplayVtable = (*bg).vtable;
        ((*vt).put_pixel_clipped)(bg, x, y, color);
    }
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

// Item kinds passed as arg1 to `MenuPanel::AppendItem`. Stored at item +0x00
// and read by the menu render code as the icon/sprite selector.
const KIND_FORCE_SUDDEN_DEATH: i32 = 0;
const KIND_DRAW_THIS_ROUND: i32 = 1;
const KIND_QUIT_THE_GAME: i32 = 2;
const KIND_MINIMIZE_GAME: i32 = 3;
const KIND_VOLUME_SLIDER: i32 = 4;

/// Rust port of the `GameRuntime::OpenEscMenu` leaderboard-sort block
/// (0x53538D..0x5354A6 in the WA function body).
///
/// Walks each populated team and computes a composite score
/// `wins * 10000 + sum_of_alive_worm_HPs`. Static team setup (wins
/// counter, "scored" flag) is read from
/// [`GameInfo::team_records`](crate::engine::game_info::GameInfo::team_records);
/// runtime worm HPs and the per-team eliminated gate come from
/// [`GameWorld::team_arena`](crate::engine::GameWorld::team_arena)
/// (1-based, slot 0 is the sentinel).
///
/// Sort algorithm matches WA's: a quasi-selection-sort that walks each
/// position `i` from 0 and swaps with any `j > i` whose score is larger.
/// Stable for equal scores (only swaps on strict less-than).
///
/// Returns the populated entries (newest at the front), the count (≤ 16),
/// and stores each team's 0-based index in [`LeaderboardEntry::team_idx`]
/// so callers can reach back into `team_records[team_idx]` for color/name.
pub unsafe fn sort_teams(world: *const GameWorld) -> ([LeaderboardEntry; LEADERBOARD_MAX], usize) {
    unsafe {
        let mut out = [LeaderboardEntry {
            team_idx: 0,
            score: 0,
        }; LEADERBOARD_MAX];
        let mut len: usize = 0;

        let game_info = (*world).game_info;
        let arena: *const TeamArena = &(*world).team_arena;
        let team_count = (*game_info).team_record_count as usize;
        if team_count == 0 {
            return (out, 0);
        }

        for team_idx_1b in 1..=team_count {
            let record = &(*game_info).team_records[team_idx_1b - 1];
            // Skip teams whose eliminated_flag is non-zero (not scored).
            if record.eliminated_flag != 0 {
                continue;
            }

            // Sum live worm HPs only when the team's runtime header gate is zero.
            let header = TeamArena::team_header(arena, team_idx_1b);
            let mut hp_sum: i32 = 0;
            if (*header).eliminated == 0 {
                let worm_count = (*header).worm_count;
                for w in 1..=worm_count as usize {
                    let worm = TeamArena::team_worm(arena, team_idx_1b, w);
                    hp_sum = hp_sum.wrapping_add((*worm).health);
                }
            }

            let wins = record.wins_count as i32;
            let score = wins.wrapping_mul(10_000).wrapping_add(hp_sum);
            out[len] = LeaderboardEntry {
                team_idx: (team_idx_1b - 1) as u8,
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

// Format a small unsigned integer as decimal into a stack buffer with
// trailing NUL. Returns the byte length (NOT including NUL). Replaces
// the `_sprintf(buf, "%d", n)` call WA uses for the leaderboard win
// counts; n is at most a u8 so 4 digits + NUL is plenty.
fn format_decimal(buf: &mut [u8; 16], n: u32) -> usize {
    use core::fmt::Write;
    struct B<'a>(&'a mut [u8; 16], usize);
    impl<'a> Write for B<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.1 >= self.0.len() - 1 {
                    return Err(core::fmt::Error);
                }
                self.0[self.1] = b;
                self.1 += 1;
            }
            Ok(())
        }
    }
    let mut w = B(buf, 0);
    let _ = write!(w, "{n}");
    let len = w.1;
    buf[len] = 0;
    len
}

/// Rust port of `GameRuntime::OpenEscMenu` (0x00535200).
///
/// Builds the in-game ESC menu into `runtime.menu_panel_a`:
/// 1. `world_root.hud_data_query(0x7D3, 0x394 buffer)` — fetches a HUD
///    snapshot. Three flag DWORDs (`buf[33]`, `buf[35]`) gate the
///    inclusion of Force-SD / Draw / Quit items below.
/// 2. Background fill on the canvas (`runtime.display_gfx_d`,
///    `world.gfx_color_table[7]`).
/// 3. Empty-string measurement for the line-height baseline.
/// 4. **If `gameinfo[0xD949] == 0`** (leaderboard shown): paint
///    "First Team to N Wins" header centered, two horizontal separator
///    lines, then for each scored team: team name + win count drawn
///    with the team-color font.
/// 5. Reset the panel widget — clear flag/scroll-region fields, clamp
///    cursor to viewport, set `item_count = 0`.
/// 6. Append menu items via [`append_item_impl`]:
///    * "Minimize Game" (always)
///    * "Force Sudden Death" — only when `world.field_1c == 0`,
///      `buf[33] == 0`, `runtime.replay_flag_a == 0`, `buf[35] == 0`,
///      `runtime._field_478 == 0`, `gameinfo[0xD941] == 0`,
///      `gameinfo[0xD948] == 0`.
///    * "Draw This Round" — when the first 3 of those plus
///      `gameinfo[0xD947] == 0`.
///    * "Quit The Game" (always).
///    * Volume slider (always; bound to `&runtime.ui_volume`).
/// 7. Draw the panel border — 4 horizontal edges, 4 vertical edges,
///    then 4 corner pixels.
/// 8. Final state: store `menu_panel_width` / `menu_panel_height`,
///    re-clamp the panel's clip rect/cursor to those dims, and set
///    `esc_menu_state = 1`.
///
/// `__stdcall(this)`, RET 0x4 originally; the WA address has no
/// remaining xrefs once this port is wired in (the only caller was
/// `EscMenu_TickClosed`, which is also Rust). Trapped in
/// `replacements/main_loop.rs` as a safety net.
pub unsafe fn open_esc_menu(runtime: *mut GameRuntime) {
    unsafe {
        let world = (*runtime).world;
        let world_root = (*runtime).world_root;
        let display: *mut DisplayGfx = (*world).display;
        let canvas: *mut DisplayBitGrid = (*runtime).display_gfx_d;
        let panel: *mut MenuPanel = (*runtime).menu_panel_a;
        let game_info = (*world).game_info;
        let template = (*world).localized_template;
        let border_color = (*world).gfx_color_table[6] as u8;
        let bg_color = (*world).gfx_color_table[7] as u8;
        // The volume slider's "aux render obj" is the same palette index
        // used for the panel border (gfx_color_table[6]); WA reads it as
        // a `*mut u8` and passes it through to `MenuPanel::AppendItem`.
        let slider_aux = (*world).gfx_color_table[6] as *mut u8;

        // ─── Block A: hud_data_query ───
        // 916 bytes / 4 = 229 i32s. Two flag DWORDs early in the
        // response (`buf[1]`, `buf[3]` — same DWORDs `is_hud_active`
        // inspects) gate the inclusion of Force-SD / Draw / Quit
        // below.
        let mut hud_buf: [u32; 0xE5] = [0; 0xE5];
        ((*(*world_root).base.vtable).hud_data_query)(
            world_root,
            0x7D3,
            0x394,
            hud_buf.as_mut_ptr() as *mut u8,
        );
        let buf_flag_84 = hud_buf[1];
        let buf_flag_8c = hud_buf[3];

        // ─── Block B: Background fill + panel-width derivation ───
        // The "panel width" used everywhere downstream IS the canvas's
        // pixel width — `runtime.menu_panel_width` is just a copy of
        // `display_gfx_d.width`. WA reads `[EDI+0x14]` (canvas.width)
        // into a local at function entry and re-uses it as the panel
        // width throughout.
        let canvas_w = (*canvas).width as i32;
        let canvas_h = (*canvas).height as i32;
        let panel_width = canvas_w;
        clipped_fill_rect(canvas, 0, 0, canvas_w, canvas_h, bg_color);

        // ─── Block C: Empty-string baseline measurement ───
        // WA passes the literal at 0x643F2B which is the empty string `""`
        // (NUL-terminated). The slot-10 wrapper writes `text_advance` (= 0
        // for an empty string) and `font_max_width` (= the font cell size
        // — used as the line height since WA's font is square).
        static EMPTY: [i8; 1] = [0];
        let TextMeasurement { line_height, .. } =
            measure_text(display, 0xF, EMPTY.as_ptr()).unwrap_or_default();

        // Running y position for items. WA initializes EBP=2 here.
        let mut y: i32 = 2;

        // ─── Block D: Conditional leaderboard ───
        let no_leaderboard = (*game_info).scheme_no_leaderboard != 0;

        if !no_leaderboard {
            // D1 — "First Team to N Wins" header.
            let win_target = (*game_info).scheme_first_to_n_wins as u32;
            let header_template = bridge_token_lookup(template, res::GAME_ROUNDS_TO_WIN);
            // WA pushes (template, 1, 1, win_target) — only the third
            // vararg (win_target) actually substitutes into the `%d`.
            let header_str = bridge_sprintf_rotating_3(header_template, 1, 1, win_target);

            let TextMeasurement {
                total_advance: hdr_w,
                ..
            } = measure_text(display, 0xF, header_str).unwrap_or_default();
            let header_x = (panel_width - hdr_w) / 2;
            let mut tmp_pen_x: i32 = 0;
            let mut tmp_width: i32 = 0;
            draw_text_on_bitmap(
                display,
                0xF,
                canvas,
                header_x,
                2,
                header_str,
                &mut tmp_pen_x,
                &mut tmp_width,
            );

            // D2 — Two horizontal separator lines below the header.
            clipped_fill_hline(canvas, 0, panel_width, line_height + 3, border_color);
            clipped_fill_hline(canvas, 0, panel_width, line_height + 4, border_color);
            y = line_height + 5;

            // D3 — Sort + render leaderboard rows.
            let (entries, num_entries) = sort_teams(world);
            for entry in entries.iter().take(num_entries) {
                let record = &(*game_info).team_records[entry.team_idx as usize];
                let team_color = record.font_palette_idx as i32;
                let wins = record.wins_count as u32;
                let name_ptr = record.name.as_ptr() as *const c_char;

                // Team-color font slot is 9..16 in WA's font table.
                let team_font = team_color + 9;

                let TextMeasurement {
                    total_advance: name_w,
                    ..
                } = measure_text(display, 0xF, name_ptr).unwrap_or_default();
                let name_x = (panel_width - name_w) / 2 - 0x10;
                draw_text_on_bitmap(
                    display,
                    team_font,
                    canvas,
                    name_x,
                    y,
                    name_ptr,
                    &mut tmp_pen_x,
                    &mut tmp_width,
                );

                let mut wins_buf: [u8; 16] = [0; 16];
                let _ = format_decimal(&mut wins_buf, wins);
                let wins_str = wins_buf.as_ptr() as *const c_char;
                let TextMeasurement {
                    total_advance: wins_w,
                    ..
                } = measure_text(display, 0xF, wins_str).unwrap_or_default();

                // Wins are drawn near the *right* edge of the panel,
                // not centered. WA's formula at 0053559a-0053559d:
                // `pen_x = panel_width - wins_w/2 - 0x14`. Drawing them
                // centered (like the name) would overlap the name text.
                let wins_x = panel_width - wins_w / 2 - 0x14;
                draw_text_on_bitmap(
                    display,
                    team_font,
                    canvas,
                    wins_x,
                    y,
                    wins_str,
                    &mut tmp_pen_x,
                    &mut tmp_width,
                );

                y += line_height + 1;
            }

            // Two post-leaderboard horizontal separators (mirroring the
            // two pre-leaderboard separators above the rows).
            clipped_fill_hline(canvas, 0, panel_width, y, border_color);
            y += 1;
            clipped_fill_hline(canvas, 0, panel_width, y, border_color);
            y += 1;
        }

        // Unconditional `ADD EBP, 0x2` at the top of WA's panel-reset
        // block (00535663) — runs in both leaderboard and skip paths.
        y += 2;

        // ─── Block E: Panel reset ───
        // Reads `panel.display_a`'s width/height to clamp the cursor;
        // then zeroes the scroll-region rect / item count.
        let panel_disp_a = (*panel).display_a as *const u8;
        let pa_w = *(panel_disp_a.add(0x14) as *const i32);
        let pa_h = *(panel_disp_a.add(0x18) as *const i32);
        (*panel)._field_18 = 0;
        (*panel).clip_left = 0;
        (*panel).clip_top = 0;
        (*panel).clip_right = pa_w;
        (*panel).clip_bottom = pa_h;
        if (*panel).cursor_x < 0 {
            (*panel).cursor_x = 0;
        }
        if (*panel).cursor_y < 0 {
            (*panel).cursor_y = 0;
        }
        if pa_w < (*panel).cursor_x {
            (*panel).cursor_x = pa_w;
        }
        if pa_h < (*panel).cursor_y {
            (*panel).cursor_y = pa_h;
        }
        (*panel)._field_2c = 0;
        (*panel).item_count = 0;

        // ─── Block F: Action buttons + slider ───
        // All four button items pass `render_ctx = null` (plain centered
        // button). Only the volume slider passes a non-null `render_ctx`
        // (the volume value pointer) to enter the wide-row override.

        let centered_x = panel_width / 2;
        let label = bridge_token_lookup(template, res::GAME_MINIMISE_GAME);
        append_item_impl(
            centered_x,
            panel,
            KIND_MINIMIZE_GAME,
            label,
            y,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        y += line_height + 1;

        // `world.net_session != null` means this is an online game; in
        // that case the Force-SD / Draw-Round actions are hidden because
        // ending the round is a host-only decision.
        let is_online = !(*world).net_session.is_null();
        let replay_flag_a = (*runtime).replay_flag_a;
        let runtime_field_478 = (*runtime)._field_478;
        let no_sd_a = (*game_info).scheme_no_sd;
        let no_sd_b = (*game_info).scheme_sd_secondary_lockout;
        let no_draw = (*game_info).scheme_no_draw;

        let common_show_action_buttons = !is_online && buf_flag_84 == 0 && replay_flag_a == 0;

        if common_show_action_buttons {
            if buf_flag_8c == 0 && runtime_field_478 == 0 && no_sd_a == 0 && no_sd_b == 0 {
                let label = bridge_token_lookup(template, res::GAME_SUDDEN_DEATH);
                append_item_impl(
                    centered_x,
                    panel,
                    KIND_FORCE_SUDDEN_DEATH,
                    label,
                    y,
                    1,
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                );
                y += line_height + 1;
            }
            if no_draw == 0 {
                let label = bridge_token_lookup(template, res::GAME_DRAW_ROUND);
                append_item_impl(
                    centered_x,
                    panel,
                    KIND_DRAW_THIS_ROUND,
                    label,
                    y,
                    1,
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                );
                y += line_height + 1;
            }
        }

        let label = bridge_token_lookup(template, res::GAME_QUIT_GAME);
        append_item_impl(
            centered_x,
            panel,
            KIND_QUIT_THE_GAME,
            label,
            y,
            1,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        );
        y += line_height + 1;

        let label = bridge_token_lookup(template, res::GAME_VOLUME);
        let volume_ptr = (runtime as *mut u8).add(0x420);
        // WA passes EAX = 6 to AppendItem here, but the slider call uses
        // `centered = 0`, so the EAX/x value isn't shifted by half-width
        // — `6` becomes the literal pen_x. (For all other items EAX is
        // panel_width/2 with `centered = 1`, which gets shifted to a
        // centered position.)
        append_item_impl(
            6,
            panel,
            KIND_VOLUME_SLIDER,
            label,
            y,
            0,
            volume_ptr,
            slider_aux,
        );
        let final_y = line_height + 3 + y;

        // ─── Block G: Border drawing ───
        // 4 horizontal edges + 4 vertical edges + 4 corner pixels.
        let pw_minus_1 = panel_width - 1;

        // Horizontal edges: top double + bottom double.
        clipped_fill_hline(canvas, 1, pw_minus_1, 0, border_color);
        clipped_fill_hline(canvas, 0, panel_width, 1, border_color);
        clipped_fill_hline(canvas, 0, panel_width, final_y, border_color);
        clipped_fill_hline(canvas, 1, pw_minus_1, final_y + 1, border_color);

        // Vertical edges: left double + right double.
        clipped_fill_vline(canvas, 0, 1, final_y, border_color);
        clipped_fill_vline(canvas, 1, 0, final_y + 1, border_color);
        clipped_fill_vline(canvas, pw_minus_1, 0, final_y + 1, border_color);
        clipped_fill_vline(canvas, panel_width, 1, final_y, border_color);

        // ─── Block H: Final state writes ───
        (*runtime).menu_panel_width = panel_width;
        (*runtime).menu_panel_height = final_y + 2;

        // 4 corner pixels (top-left, bottom-left, top-right, bottom-right)
        // drawn with color 0 to round off the border.
        put_pixel_clipped(canvas, 0, 0, 0);
        put_pixel_clipped(canvas, 0, final_y + 1, 0);
        put_pixel_clipped(canvas, pw_minus_1, 0, 0);
        put_pixel_clipped(canvas, pw_minus_1, final_y + 1, 0);

        // Outer-rect clamp: re-fill the panel's clip rect with the
        // computed menu dimensions (replacing the display-wide rect set
        // in Block E), then clamp cursor.
        let mp_w = (*runtime).menu_panel_width;
        let mp_h = (*runtime).menu_panel_height;
        (*panel).clip_left = 0;
        (*panel).clip_top = 0;
        (*panel).clip_right = mp_w;
        (*panel).clip_bottom = mp_h;
        if (*panel).cursor_x < 0 {
            (*panel).cursor_x = 0;
        }
        if (*panel).cursor_y < 0 {
            (*panel).cursor_y = 0;
        }
        if mp_w < (*panel).cursor_x {
            (*panel).cursor_x = mp_w;
        }
        if mp_h < (*panel).cursor_y {
            (*panel).cursor_y = mp_h;
        }

        (*runtime).esc_menu_state = 1;
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
///   [`open_esc_menu`], which builds the menu contents into
///   `runtime.menu_panel_a` and transitions `esc_menu_state` to `1`.
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
            open_esc_menu(runtime);
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

    #[test]
    fn format_decimal_writes_null_terminated() {
        let mut buf = [0u8; 16];
        let len = format_decimal(&mut buf, 42);
        assert_eq!(len, 2);
        assert_eq!(&buf[..3], b"42\0");
        let len = format_decimal(&mut buf, 0);
        assert_eq!(len, 1);
        assert_eq!(&buf[..2], b"0\0");
        let len = format_decimal(&mut buf, 9999);
        assert_eq!(len, 4);
        assert_eq!(&buf[..5], b"9999\0");
    }
}
